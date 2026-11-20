---
title: "Command-driven charts source research"
date: 2026-11-20
status: accepted
tags: [research, charts]
summary: "Dependency, tree, process-boundary and upstream-contract findings behind the command-driven charts build plan; no live account data."
---

# Command-driven charts: source research

Grounded at Sidescope `1c2b0e1` and the read-only agent-activity checkout `bcdbf46`, 2026-11-20. Behavior is owned by the [design](../design/command-charts.md); the build hand-off is the [plan](../design/command-charts-plan.md). No personal usage, credentials or authenticated captures were read or recorded.

## Dependencies

| Need | Finding | Choice |
| :--- | :--- | :--- |
| Decimals | Design requires up to 24 integer plus 18 fractional digits (42 digits). `i128` holds 38–39 digits and `rust_decimal` 28, so neither fits | `bigdecimal` 0.4 (`default-features = false`), behind a strict grammar wrapper that enforces the digit limits and refuses overflow on sums |
| Zones and calendars | The tree has no time crate; `libc` only. Host `/usr/share/zoneinfo` exists in the jail but is not guaranteed on the user's host | `jiff` 1 with `tzdb-bundle-always`, so DST tests are deterministic and need no system tzdata |
| JSON Pointer | `serde_json::Value::pointer` implements RFC 6901 but returns `None` for both a bad pointer and a missing value | Reuse it for lookup; validate pointer syntax separately at configuration time |
| Accessibility | `gtk4` 0.11 exposes `AccessibleExt`; `GTK_A11Y=none` in the private test bus, so assertions run on properties, not the AT-SPI tree | Pure text models carry accessible names and table rows; widget checks run in the visual harness |

A scratch crate (not committed) confirmed that `jiff` and `bigdecimal` build under `scripts/native-env.sh`, that a 42-digit sum is exact, that `2026-11-01` in `America/New_York` is 25 hours long, and that an unknown zone is refused. `jiff` 1.0.1 spells the zoned conversion `civil::Date::in_tz("Zone")`, not `to_zoned`; read its docs rather than other versions' examples. The bundled database enlarges the release binary; the foundation stage records the size delta against `just test-release`.

## Tree facts

| Surface | Fact | Consequence |
| :--- | :--- | :--- |
| [`src/lib.rs`](../../src/lib.rs#L138-L177) | `AppConfig` is `deny_unknown_fields`, built by `parse_config`; each optional table validates after parse. No struct literal builds it outside `Default` | Additive `chart_sources`/`chart_cards` vectors default empty; existing configs and `tests/cli.rs` are unaffected |
| [`src/process.rs`](../../src/process.rs#L63-L142) | `output()` polls one nonblocking stdout pipe every 20 ms; stderr is `Stdio::null()`; 1 MiB constant; group kill then reap; spawn failure is one undifferentiated message | Charts need stderr capture, caller limits, cancellation and a distinct missing-executable signal; the same loop must drain both pipes to avoid deadlock and keep `output()` behavior and tests |
| [`src/main.rs`](../../src/main.rs#L2372-L2412) | Local sampling and weather use a worker thread plus a 200 ms `glib::timeout_add_local` poll of an `mpsc` channel; `render_dashboard` reuses widgets by card id and compares `Card` values | Chart runtime stays GTK-free and is polled the same way; chart cards are not `Card`s and are integrated beside them |
| [`src/main.rs`](../../src/main.rs#L274-L343) | `demo = !matches!(mode, LaunchMode::Live)`; `--report` is demo-like, with no sampling or weather | The chart runtime starts only in live mode; visual evidence uses the live-stub harness plus synthetic demo scenarios |
| [`src/plot.rs`](../../src/plot.rs) | Geometry assumes 120 fixed two-second slots | New timestamp/duration geometry lives in `chart::geometry`; `plot.rs` is untouched |
| `scripts/visual-test.sh` | Private Sway, `SIDESCOPE_VISUAL_LIVE_STUB=1` runs `--live` with a stub `ai-usagebar`; `SIDESCOPE_VISUAL_CONFIG` copies a config; demo scenarios set `SIDESCOPE_VISUAL_SCENARIO`; `kill $app_pid` is SIGTERM | Chart e2e reuses both paths. SIGTERM currently bypasses Rust destructors, so child groups need parent-death protection plus a graceful signal handler |
| `Justfile` | Everything routes through `scripts/native-env.sh`; `check-ci` = fmt check + clippy + tests; `test` adds four shell suites | New gates use the same wrapper; no new recipe is required |

## Upstream ledger contract (`bcdbf46`)

Read from `docs/reference/ledger-v1.md` (series section and CLI table), `internal/ledger/wire/types.go` and `cmd/agent-activity/ledger.go`. These checkout paths are outside this repository.

- `ledger series --format json --layout columns --bucket <minute|5min|15min|hour|day> --group-by <dims> --from <rfc3339> --to <rfc3339> --timezone <iana> [--workspace <label>]`: flag and value are separate arguments, so whole-argument placeholders suffice.
- Header carries `version`, `generation`, `generatedAt`, `listEstimate` (always true), `range{from,to,source}` (effective, aligned outward) and `coverage{…, statement}`; `buckets[{start,end}]` are half-open UTC; `groups[{key,keys[],columns{…}}]`.
- Columns omit a counter that is zero in every bucket of the group; `knownCostUSD` and `subscriptionKnownCostUSD` are decimal strings; `unpriced` and `subscriptionUnpriced` are the matching incompleteness counters; counts are integers.
- Limits: 400 buckets, 60,000 group-by-bucket values, up to three group dimensions. Three hours at `minute` fits; a week at `minute` is refused.
- Not available upstream: explicit no-group output, separate requested-versus-effective bounds, collection freshness distinct from `generatedAt`, and a capability surface. These are the feature requests in the request lane.
- The checkout is read-only here and another agent is changing it. Re-read it before writing the request document or example profile; do not treat this table as current.

## Synthetic cases the fixtures must carry

Omitted-zero columns; exact decimal sums (0.1 + 0.2); partially priced cost; subscription channel and per-token subscription estimate; empty and all-null data; out-of-order and overlapping intervals; group keys that collide when joined with `|`; late correction of an old bucket between two answers; a current bucket ending after the request instant; DST days of 23 and 25 hours; oversized answers at each limit.
