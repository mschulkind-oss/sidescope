---
title: "Command charts: authoring and debugging tools"
status: accepted
stage: CURRENT
verified: 2026-11-20
verified_commit: 232aaec
covers:
  - src/chart_cli.rs
  - src/chart_cli/
  - src/chart/facts.rs
  - src/chart/schema.rs
  - src/chart_view.rs
  - src/chart_runtime.rs
  - src/chart_runtime/scheduler.rs
  - examples/charts/quota-estimate.py
  - examples/charts/quota-observed.py
tags: [reference, charts, cli, authoring]
summary: "How `sidescope charts` proves what a chart shows without a display: the single reduction both the window and the reports read, the clocks, the check chain, capture, the quota wrappers, and the traps."
---

# Command charts: authoring and debugging tools

**Status:** Verified 2026-11-20 against `232aaec` plus the commit that adds this page. MEASURED: the full Rust and shell test suites, the compatibility and cookbook tests over invented fixtures, and a private-compositor run of the chart suite including a sampled gauge. UNMEASURED: any real producer, account, collector or host desktop; a request deferred at wall-clock local midnight (only the scheduler's unit tests); a stdout file cut short by a full disk; screen-reader text. See [what is not established](#what-is-not-established) and the [QA record](../../qa/command-charts.md). Orientation: [README](README.md). Task-oriented walkthrough: the [chart cookbook](../../chart-cookbook.md).

`sidescope charts` lets an author, human or agent, decide whether a chart source and card are right without opening a window. It adds no second way to run a command, parse an answer or total a number. It adds a typed middle layer between them and a command surface over that layer.

## Principles

These outlive any implementation.

- **A1. One reduction, two presentations.** A card is reduced once, to a typed value that carries the outcome, exact decimal totals, quality, bound, group membership and the full Details rows. The window draws a model that is a function of that value alone, and the reports serialize the same value, so the two cannot disagree. Display strings are copied from the model unchanged; drawing floats are copied and marked non-authoritative; exact numbers are decimal strings and never floats.
- **A2. Execution, normalization, reduction and rendering stay separate.** `replay` calls the same normalizer the runtime calls. `run` uses the same bounded runner under a one-shot policy. Neither verb contains process or arithmetic code of its own.
- **A3. Provenance is not fact.** The `dataset` block and each card's outcome, summary, intervals, groups and rows are facts about the data. `data_origin`, `attempt`, the two clocks, `checks` and per-source results describe this invocation. Replay reproduces the first kind and never imitates the second: a replay's execution and freshness are `not-run`, and no display text presents replayed data as live.
- **A4. Missing is never zero, and quota points are never money.** An absent leaf is zero only where a mapping rule says so and its parent exists; a null is missing; a total with a missing input is an `exact: null` with a separate known subtotal. Estimated quota is `percentage points`, never `percent` and never dollars.
- **A5. Exit 0 means nothing failed, never that the chart is what you meant.** The caller asserts the outcome it expects (`no-observations`, `no-intervals`, `no-known-values` and an all-zero `plotted` are four different truthful results), normally with `jq -e`.

## How it works

**Command family.** `schema` prints every chart key, default, range, enum, pointer scope, payload version, limit, error kind and the report description from one set of descriptor tables that validation, help and the generated [key matrix](key-matrix.md) also read; tests compare accepted and described keys in both directions. `check` validates and resolves each source's request and runs nothing. `replay` normalizes saved output per source and builds the selected cards. `run` executes exactly the named sources, once each, then builds their cards. Every verb but `schema` prints exactly one JSON document, including for a usage error or a missing explicit configuration file, and nothing initializes a display. The commands are unsandboxed; `run` carries the application's authority and inherited environment, and `--demo`, `--report`, `--browser` and `doctor` still execute nothing.

**Selection is resolved before anything happens.** Arguments, configuration, selectors, data files and the capture path are validated first; a failure ends the command with no process spawned and no file written. An empty scope is `invalid`, never green. A repeated selector, an unknown source or card, and a card whose source was not selected are errors, not omissions.

**Clocks.** Each source carries a request instant (resolves the window and drives the open-right-edge downgrade) and a model instant (builds the cards). In `run` they are two real captures and can differ by the command's duration; in `replay` both are the explicit `--at`, or the pair recorded in a capture manifest, so a multi-source run can be rebuilt exactly. `run` refuses `--at`. A window resolved at local midnight can be deferred; one-shot retries it once and a still-deferred request ends `undetermined` (exit 3), not as an invented answer.

**Check chain and status.** Per source the checks are `config`, `request`, `execution`, `dataset`, `cards`, `freshness`, `coverage` and `appearance`. A check after a failed one is `not-run` with a dependency reason, not `unknown`. An aggregate is `fail` if any source failed, else `unknown`, else `not-run`, else `pass`, so one failed source can never leave an aggregate `pass`. Status is `findings` if any check failed, `undetermined` only when a *required* check is `unknown`, otherwise `ok`. `coverage` (a collector watermark the producer may not assert) and `appearance` are never required. A source whose output was refused after an exit 0 has a failed `dataset`, and its `freshness` is therefore `not-run`, not `pass`.

**One-shot policy.** The scheduler gained a mode in which a source's first terminal attempt (success, failure, timeout, bound, or a deferred request retried once) is final: no refresh, no backoff retry, no queued manual run, even while slower sources still run. The runner hands back exit-0 stdout as bytes, so a capture is byte-exact even when the output is not UTF-8 and then fails to parse.

**Capture.** `--capture DIR` creates the directory owner-only and writes, for each source whose command exited 0 within the output bound, one stdout file (owner read-write), byte-exact and never truncated, then a manifest last. Files are created exclusively, so one that appears after the preflight is never overwritten. The manifest records the invocation clock and per source the resolved window, both clocks, byte count and outcome; it holds no argument vector, stderr or environment. A write failure after execution is `capture-write`: the report is still printed with status `findings` and the manifest says `complete: false`. `--from-capture` needs a configuration that contains every recorded source and refuses a directory without a manifest; sources recorded as failed replay as `unavailable` with the recorded error id. A capture is real data kept at the caller's request: keep it out of version control.

**Privacy and bounds.** Raw output never appears in a report. The argument vector appears only with `--show-argv`, and a stderr excerpt only for a source that sets `capture_stderr`. A report over 1 MiB is replaced by a small `report-too-large` document with exit 2, never a truncated report under a passing status. Details are paged (`rows_total`, `offset`, `rows_returned`, `complete` are always stated); a sum card's present time-cell values, or its ranked Details rows' known subtotals, add up to its known subtotal on every page; Other equals its members and is never added to them; a gauge's last reading is never a period sum.

## Quota adapters and profiles

The columns mapper refuses non-integral JSON numbers and that does not change. Quota percentages arrive as numbers rounded to four places, so two explicit single-file Python wrappers sit beside the example profiles, each the source's command and each starting exactly one producer child with no shell and no other query.

- `quota-estimate.py` reads the producer's decimal text exactly and emits a native answer of estimated quota *points* (unit `percentage points`, an interval total, an estimate, with a nonnegative unknown contribution), so a bounded-percent card on it is a card mismatch. A bucket whose unweighted count is above zero is partial; a model with no weight is a visible partial lower bound, never a complete zero. Fitted weights become at most four short notices; a missing `quotaWeights` is a non-zero exit.
- `quota-observed.py` emits a `sidescope-chart-v1.1` *sampled gauge* of one account and quota window: each reading is its own interval start, readings are never summed, a reset or a gap beyond a stated limit sets a break, and the headline is the last non-null reading with its instant. A null newest reading keeps the last reading and sets `newest_is_null`. The wrapper's gap limit is its own documented heuristic, not producer data.
- Both refuse rather than trim: more than the interval, series or cell limits, an over-long notice, invalid decimal text and misaligned arrays are non-zero exits with no partial output, shown as `unavailable`. Each reads at most 1 MiB from the producer and from standard input, and each has a pure `--convert-stdin` mode that runs nothing, which is how fixtures reach `replay`.
- Three example profiles stay separate: the legacy ledger profile (byte-stable), a filtered money profile that names its columns and groupings, and the quota profile. The legacy and filtered profiles contain no quota source or card, so an older producer is never handed a quota query. Sidescope never probes capabilities, never rewrites a query and never falls back; the author checks each feature by hand, and a successful probe proves only that a column is listed. Observed and estimated cards are never overlaid, and no association between an estimate and an account is inferred, because a ledger estimate carries none.

The v1.1 payload additions (`sampled`, `breaks`) and their refusals are in the [dataset contract](dataset-and-mapping.md); sampled display is in [native charts](native-charts.md).

## Traps

- **An objects-layout answer used to read as zero.** With `omitted = "zero"`, a missing leaf is zero only when its immediate parent is an object, or an array and the leaf a canonical index; a missing, null or scalar parent is an invalid mapping. The guard does not catch every wrong leaf name and does not prove intent.
- **A refused exit-0 answer once reported fresh.** Freshness was derived from execution alone; it is now blocked by execution, then by dataset, and only then judged by age. The regression is pinned in the public run tests.
- **A key matrix cell is escaped once.** The enum choices were escaped twice, which ends a Markdown table cell early; the checked-in matrix test compares the file with the command's output.
- **A grouped answer fed to a source with no dimension** is an invalid dataset (duplicate series id), not a merge; and a group-none answer fed to a source that reads `/keys/0` is an invalid mapping. Choose the profile that matches the query.
- **The window sum of estimated points can exceed 100**, so it is never drawn on a bar scale.
- **Replay proves construction, not the late-correction path.** Replacing an earlier answer is the runtime's publication step; the compatibility tests publish the producer's before-and-after pair through it and compare the consumer's snapshot with the second answer alone.

## Where it lives

`chart_cli` (argument parsing, selection, per-source evaluation, report assembly, capture, text and help), `chart::facts` (`reduce_card` and the typed card value), `chart::schema` (the descriptor tables, the key matrix and help text), `chart_view` (`present_card`, the GTK-free model), `chart_runtime` (`start_one_shot`, the attempt record) with its scheduler policy, `process` (raw stdout bytes), and the two wrappers under `examples/charts/`. Tests: the `chart_facts`, `charts_cli_*`, `chart_quota_wrappers`, `chart_profiles`, `chart_compat` and `chart_cookbook` suites.

## Current values

| Value | Setting |
| :--- | :--- |
| Report, capture and schema identifiers | `sidescope-chart-report-v1`, `sidescope-chart-capture-v1`, `sidescope-chart-schema-v1`; fields are only added within v1 and consumers ignore unknown ones |
| Exit codes | 0 ok, 1 findings, 2 invalid, 3 undetermined |
| Report size, Details page | 1 MiB; 500 rows by default, 5000 at most |
| Wrapper limits | 2048 readings or buckets, 256 series, 100000 cells, 512-character notices, 32 notices, 1 MiB in |
| Observed gap limit | 45 minutes by default, 1 to 1440 whole minutes |

Every other limit and key is printed by `sidescope charts schema` and listed in the [key matrix](key-matrix.md).

## What is not established

- **A real producer.** No run used a real ledger, quota journal, account or credential; the wrappers were checked against the producer's source and its published fixtures, not a live process.
- **Known gaps, not blocking.** Reserved payload names are refused only in their defined position (a top-level or series-object `sampled` or `breaks` is not refused); a ranked gauge card over 16 series with no intervals reports capacity before no-intervals; ranked Details place Unknown rows after every named row; the `--offset N, --limit N` option text in help runs into its description; and `quota-estimate.py --convert-stdin` does not cross-check the answer's grouping against its flags (not reachable in live mode). The [QA record](../../qa/command-charts.md) carries them.
- **Midnight, full disks, pixels.** The one-shot retry at local midnight, a stdout file cut short by a full disk, and the exact pixels of a card are not exercised by the report tools; the private-compositor suite covers the sampled gauge on synthetic data.
