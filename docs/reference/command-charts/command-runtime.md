---
title: "Command charts: command runtime, configuration and diagnostics"
status: accepted
stage: CURRENT
verified: 2026-11-20
verified_commit: 232aaec
covers:
  - src/chart/config.rs
  - src/chart/error.rs
  - src/chart/state.rs
  - src/chart/time.rs
  - src/chart_runtime.rs
  - src/chart_runtime/scheduler.rs
  - src/process.rs
  - src/chart_host.rs
  - src/cli.rs
tags: [reference, charts, runtime]
summary: "How a chart source is configured, turned into a request, run as a bounded process, scheduled and retried, published as a snapshot, and diagnosed; what the runtime refuses to promise."
---

# Command charts: command runtime, configuration and diagnostics

**Status:** Verified 2026-11-20 against `232aaec` plus the commit that adds the authoring-tools page. MEASURED with synthetic commands in unit tests, an integration test, and a private headless compositor; UNMEASURED against a real collector or a host desktop. Orientation: [README](README.md).

The runtime owns every side effect of the subsystem: resolving the question to ask, running the owner's command under bounds, deciding when to run it again, and publishing a **snapshot** per source. It imports no GTK. A pure **scheduler** state machine decides what to start, cancel, and publish from events and an injected monotonic time; a thin worker layer performs those actions.

## Authority and trust

- A command runs with Sidescope's authority and inherited environment. It is **not sandboxed**, and nothing in the interface calls it safe or read-only.
- Presence of a `[[chart_sources]]` entry in the owner's configuration **is** the enablement; there is no `enabled` key. `sidescope doctor` prints each full command and whether its executable exists (it runs nothing); `sidescope charts run --source ID` runs exactly the sources you name, once each, with the same authority (see [authoring tools](authoring-tools.md)); Info shows only the executable name, never a path or arguments, so screenshots do not carry them.
- A dataset, mapping, or command output can never supply a command or change a card. Sidescope never syncs, starts a collector, migrates, installs, or authenticates on a command's behalf.
- The runtime starts only in live mode (decided by `starts_chart_runtime`). `--demo`, `--report`, and `--browser` never construct it, and demo scenarios build snapshots directly from fixtures.

## Configuration

Configuration is validated at parse time, before any window opens; an invalid chart table exits with status 2 and names the key. Existing configurations without chart tables are unchanged, and the new keys are additive with no migration.

```toml
[[chart_sources]]
id = "example-metrics"            # [a-z0-9][a-z0-9_-]*, unique
command = ["example-chart-data", "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}"]
format = "sidescope-chart-v1"     # or "columns-v1" with a mapping table (required exactly then)
window = "3h"                     # or calendar_days = 5; exactly one
timezone = "America/New_York"     # required IANA zone
refresh_seconds = 30              # optional; also timeout_seconds, stale_after_seconds, capture_stderr

[[chart_cards]]
id = "recent-amount"              # unique among chart cards; widgets use a separate chart-card- name space
source = "example-metrics"
kind = "time-series"              # | "time-bars" | "ranked-bars"
metric = "amount"
summary = "sum"                   # optional; also title, group_by, top_n, stack_by, bounded_percent
```

- **Placeholders.** Only the whole-argument forms `{from}`, `{to}`, `{timezone}` exist. Each becomes exactly one argument; there is never a shell. An argument that contains braces but is not exactly one of them (for example `--from={from}`) is refused rather than passed through as a silently wrong literal.
- **Executable.** argv[0] is a literal path or `PATH` name. A leading `~`, or any of `$ * ? | ; \` ` is refused. Anything needing expansion or a pipeline needs an explicit wrapper executable.
- **Windows.** A fixed window is a positive whole-minute duration (`90m`, `3h`, `2d`); a calendar window counts local dates including today, starting at local midnight in the named zone. Both end at one request instant captured per run. Bucket choice stays a literal command argument; Sidescope never infers it.
- **Request.** The inspectable request (source, argv with placeholders substituted, window, zone, local revision) is computed without executing anything. `{from}` and `{to}` are RFC 3339 UTC with whole seconds and a `Z`, and `{to}` is that single instant.
- **Card keys.** `group_by` defaults to empty (reduce every dimension); `top_n` is ranked-only; `stack_by` is time-bars-only and names one dimension; `bounded_percent` declares a 0 to 100 axis. For `columns-v1` sources a card naming an undeclared metric or dimension, or a reduction its metric does not permit, is refused at configuration time; for native sources those facts are learned only from data, so the same problems appear as a card-level mismatch while the source stays published (rules R4, R17).
- **Stale threshold.** Defaults to the larger of a fixed floor and twice the refresh interval; an explicit value below twice the refresh is refused.

## Request resolution and deferral

Resolving a request yields a request, an invalid-configuration error, or a **deferral**. Deferral exists for one situation: a one-date calendar window at the first second of the local day is momentarily empty and becomes positive a second later. A deferral publishes nothing, counts no failure, keeps prior data, and reschedules after a short fixed delay. It is not an error, and it must never suspend a source. An invalid window (unknown zone, zero length) is an invalid-configuration error and does suspend.

## Process boundary

Each run uses the shared bounded runner in `process`:

- The child is spawned in its own process group; stdout and stderr are drained together every pass, so a command that fills one pipe while the other is idle cannot deadlock.
- **Stop-and-reap order.** On timeout, cancellation, output overflow, and shutdown, the whole owned group is killed *before* the leader is reaped (the only order that avoids process-id reuse), then the child is reaped. A descendant that escaped the group is out of scope.
- After a normal exit the final buffered bytes are read before the size bound is enforced once; otherwise a large answer would be silently truncated.
- The child is spawned from the scheduler's long-lived thread with parent-death protection, because that signal follows the creating thread. Graceful quit and SIGTERM of Sidescope stop whole groups through the shutdown hook; **SIGKILL of Sidescope protects only the direct child**, not its descendants.
- stdout over its bound is refused as an output-bound failure, never truncated. Invalid UTF-8, a missing executable (distinguished from other spawn failures), and a nonzero exit are distinct failures.
- **stderr** is discarded by default. With `capture_stderr = true` the first bytes up to a small bound are kept, sanitized to plain text, and shown in Info. At timeout and at cancellation, bytes already buffered in the pipe are read once under the same bounds after the group is stopped, and nothing waits past the deadline. stderr is never copied into a persistent log or screenshot by default.

## Scheduling and publication

- **One in-flight run per source; global concurrency cap.** A manual refresh while a run is in flight sets a single queued refresh, not a queue of clicks. Identical sources feed every attached card from one answer.
- **Revisions.** Each source carries a local request revision. A configuration change cancels the in-flight run, bumps the revision, and clears that source's cached answer; a finished run carrying an obsolete revision is discarded even if cancellation raced completion, so old-window values never appear under new labels. An unchanged source entry is left alone and is not refetched. Card-only changes never reach the runtime. The application reads configuration once at startup; a live reconfiguration entry point exists and is tested, but no settings editor calls it.
- **Backoff.** Consecutive failures wait on a ramp, never less than the source's own refresh interval; success resets it. Invalid configuration and a missing executable **suspend** scheduled retry until the owner presses Refresh (which clears suspension) or the application restarts.
- **Publication.** A success replaces the whole dataset. A failure keeps the previous dataset and records the latest error, so a source is `Loading`, `Fresh`, `Stale`, `RefreshFailed` (with or without staleness), or `Unavailable`. `Fresh` and `Stale` depend only on time since the last success and the stale threshold, never on `generated_at`.
- **Startup and refresh policy.** Startup and the quota timer's automatic emissions never trigger a second chart query; only an explicit Refresh does, and a cold start runs each source exactly once.

## Diagnostics

Twelve error kinds exist (`ChartErrorKind`): invalid configuration, missing executable, spawn failed, nonzero exit, timeout, output bound, cancelled, parse failure, unsupported version, invalid mapping, invalid dataset, card mismatch. Only the first two suspend retry. Messages are plain, sanitized, length-bounded text that carries **no** stdout, stderr, or argument values. Info lists the source, executable name, last attempt, last success with age, next retry (or the suspended reason), latency, revision, stale threshold, requested window, returned range, time zone, generation time, collection watermark (or "unknown"), source revision, notices, last error, and the opt-in stderr excerpt.

## What the runtime is not

Not a sandbox, not a retry-forever daemon, not a history store, and not transactionally consistent across sources: different sources' answers are independent. It does not detect a stopped collector from `generated_at` alone; collection freshness is "unknown" unless the producer asserts a watermark.

## What a designer can change here

Add a placeholder, a window form, or an error kind only together with configuration validation, the request resolver, and the Info/diagnostic text, since all three must agree. A new transport (watch stream, batch) would be a different runner behind the same normalizer contract. Changing the ramp, caps, or bounds is a Current values edit only.

## Current values

| Value | Setting | Owner |
| :--- | :--- | :--- |
| Default refresh / minimum refresh | 30 s / 5 s | `DEFAULT_REFRESH_SECONDS`, `MIN_REFRESH_SECONDS` in `chart::config` |
| Default timeout / allowed range | 10 s / 1 to 60 s | `DEFAULT_TIMEOUT_SECONDS`, `MAX_TIMEOUT_SECONDS` in `chart::config` |
| Stale threshold default | max(90 s, 2 × refresh); explicit value ≥ 2 × refresh | `DEFAULT_STALE_FLOOR_SECONDS` in `chart::config` |
| stdout bound / stderr capture bound | 1 MiB / 8 KiB | `STDOUT_LIMIT_BYTES`, `STDERR_LIMIT_BYTES` in `chart::config` |
| Concurrent commands | 4 | `MAX_CONCURRENT_COMMANDS` in `chart::config` |
| Failure backoff | 30, 60, 120, then 300 s | `BACKOFF_SECONDS` in `chart_runtime::scheduler` |
| Deferral retry | 1 s | `resolve_request_at` in `chart::config` |
| Process poll pass | about 20 ms | `run_bounded` in `process` |
| Application poll of the runtime | 200 ms | `main` timer (`chart_host`) |
| Message length | 200 characters | `MESSAGE_LIMIT` in `chart::error` |

## Where the code lives

`chart::config` (parsing, validation, `resolve_request_at`), `chart::state` (`SourceSnapshot`, `SourceStatus`), `chart::error`, `chart_runtime` and `chart_runtime::scheduler`, `chart_runtime::OneShot` (`start_one_shot`: each selected source attempted once, no refresh, backoff retry or queued manual run; a request still deferred after one retry is terminal), `process` (`run_bounded`, `CancelToken`, exit-0 stdout as raw bytes), `chart_host` (live/demo wiring, `starts_chart_runtime`, `AutomaticRefresh`), `cli` (help schema, `doctor`), `chart_cli` (the headless `charts` verbs).
