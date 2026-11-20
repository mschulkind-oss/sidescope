---
title: "Command-driven charts QA"
date: 2026-11-20
status: accepted
stage: DECIDED
tags: [qa, charts]
---

# Command-driven charts QA

Integration of the command-driven charts ([design](../design/command-charts.md), [plan](../design/command-charts-plan.md), [tasks](../tasks/command-charts.md)). Everything below used synthetic commands and fixtures; **no real agent-activity ledger, account or credential was read.**

## Gates run

| Command | Result |
| :--- | :--- |
| `just format`, `just check-ci` (fmt, clippy `-D warnings`, all cargo suites, dev-tool/watch/deploy/release shell tests) | passed after the repairs below; suites: lib 89, bin 29, `chart_foundation` 41, `chart_integration` 4, `chart_mapping` 31, `chart_runtime` 35, `chart_view` 41, `cli` 8 |
| `just test-visual` (private headless Sway: 21 themes, density, all existing suites, then `scripts/visual-charts.sh`) | **exit 0** on the third full run (logs were kept outside the repository) |
| `SIDESCOPE_VISUAL_CHARTS_ONLY=1 just test-visual` after the final script hardening | exit 0 |
| `uvx vantage-check` on the changed documents | see the final section |

## Red runs, causes and repairs

1. **Full `just test-visual`, run 1: exit 1** inside the chart suite, after the seven demo scenarios and before the Details step. The log of that run was not kept (the cleanup only saved `app.log`), so the exact failing assertion is **not proven**. The suite passed alone beforehand and passed unchanged on the third full run. Most likely a dropped virtual-keyboard event (the existing suites already prefix keys with `abc` for this reason). Repairs: `visual-test.sh` now saves every `charts-*.log` on failure, and the Details step resends the key until the app reports the popover instead of assuming one attempt landed. Treat this as an unconfirmed flake that is now observable.
2. **Full `test-visual`, run 2: exit 1.** Cause, from the saved log: the new check `CHART_A11Y plot_label=true card_label=true` read `false`, because the suite runs with `GTK_A11Y=none`, which has no accessibility context to hold properties. Repair: the Details scenario runs with `GTK_A11Y=test`, the GTK backend that records properties, and asserts via `gtk_test_accessible_has_property`.
3. **Run 3: exit 0.**

Other defects found by looking at captures and fixed: the Details table rendered white (popover content is outside `.chart-card`, so the card-scoped CSS never matched; now scoped by a `chart-card-popover` class) and was clipped to 320 px (now 560 px minimum, scrolls horizontally past that). The `latency` example metric id is `latency_ms`; a wrong id in my first test config produced a card mismatch, now corrected.

## Accessibility

The lane set the accessible name as tooltip text only. Integration now calls `update_property(Property::Label)` on the card and the plot (and keeps the tooltip). That API needs the `gtk4` `v4_10` feature, which I enabled in `Cargo.toml`; `libadwaita` `v1_4` already requires a newer GTK, so no practical minimum moves. Evidence: the real application under GTK's test accessibility backend reports the Label property present on both widgets. **Not verified:** the label's text value through a screen reader (no AT-SPI bus in the jail).

## Behavior checked end to end

- No chart configuration: no runtime thread, no section, no CSS provider; existing suites unchanged and green. `--demo`, `--report` and `--browser` never construct a runtime (`starts_chart_runtime`, unit-tested).
- Invalid chart config exits 2 before a window and names the key (`tests/cli.rs`); `--help` documents both tables and the authority notice; `sidescope doctor` prints each full command and "commands run with this app's authority".
- Real runtime plus normalizer plus models, no display (`tests/chart_integration.rs`): the shipped profile with `fake-ledger.sh` loads all four sources; one command run per source on first load although several cards share it (minute history once, five-day snapshot once for daily bars and ranking); a missing executable yields Unavailable cards with no path in the banner; the two native synthetic scripts feed unrelated metrics through the same path.
- Private Sway, live mode (`scripts/visual-charts.sh`): spend, latency, the generic columns profile and a hung source together. Every healthy source reports `dataset=true`; a real `j` key still scrolls while the hung source is in flight; SIGTERM stops the app and **both the hung command and its descendant are gone**; the config file is byte-identical afterwards.
- A real Enter key opens Details on the focused plot; a snapshot replaced underneath keeps the same widgets and the popover open; Escape closes it.
- Scenarios `charts-full|partial|stale|failed|unavailable|dst|many-series|details` are synthetic, built in `src/chart_host.rs`, and cover every banner, so loading, stale, refresh-failed and unavailable are all visible.

## Captures inspected (ignored under `screenshots/`, 640 px and 378 px)

`stripe-charts-{full,partial,stale,failed,unavailable,dst,many-series,live}-{wide-top,wide-bottom,narrow-0..6}.png`, `stripe-charts-details-open{,-output}.png`. Opened and judged: lines with gap breaks and hollow partial markers, daily bars with the hatched dashed "so far" bar and `Mon*` label, the 25-hour DST day, stacked bars, ranked bars with hatching and `*`, Other with 15 hidden series, `Stale` / `Refresh failed` / `Source unavailable` / `Loading` banners, the live generic-mapper cards ("At least … USD", `(unknown)` category), narrow headers wrapping to two lines without clipping. Remaining observations: the Details popover is a separate surface that can extend left of a docked window; its Quality column needs horizontal scrolling at 560 px; at 378 px the section sits below the other cards, reached by scrolling.

## Not verified

Host-live behavior with the owner's real `agent-activity` collector and desktop (no account access here); screen-reader announcement text; SIGKILL of the app (descendants of a direct child survive; only graceful quit and SIGTERM stop whole process groups); the runtime lane's red-first run was not captured and the foundation lane recorded no before/after release size, so none is claimed; Details popover placement under other compositors.

## Review repair round (after the independent review of `e86379e`)

The review blocked completion with six P1 and eight P2 findings. Each software bug got a permanent regression first; red means the new test failed before the fix. Exceptions are stated plainly.

| Finding | Resolution | Regression |
| :--- | :--- | :--- |
| P1 unknown-series capacity underflow (a valid dataset aborted the app) | `named_limit` uses checked arithmetic; Unknown groups alone beyond the 16-series plot refuse visibly with their count, and Details keeps every row | `chart_view_repair`: 17 and 40 unknown groups (red: panic/empty message) |
| P1 one-day window at local midnight became permanently suspended | structured `WindowError::NotYetPositive` / `RequestError::Deferred` and scheduler `Event::Deferred`: nothing is published, failures are not counted, prior data stays, retry after one second; `resolve_request` keeps its old error for other callers | `chart_runtime`: deferred keeps the source schedulable, midnight resolves to Deferred (written after the code, **no red run captured**) |
| P1 native `bounded_percent` bypassed the unit check | card-level "Card mismatch" unless the metric unit is `percent`; the source stays published | `chart_view_repair` (red) |
| P1 ranked gauges hid older observations | ranking keeps the observed interval; rows say "last known", Details shows the actual interval, readout names it | `chart_view_repair` (red) |
| P1 profile dropped the coverage statement | all four profile sources map `/coverage/statement` to notices; synthetic fixtures carry a synthetic statement; Info shows it | `chart_mapping` (red) and `chart_integration` (Info line) |
| P1 producer "unknown" model fell into Other | new generic mapping key `unknown_value` per dimension (rule R16 in the plan, design paragraph); the profile sets it for the model dimension; series identity keeps the raw text | `chart_mapping` (red), `chart_view_repair` (Unknown below the top-N cut stays its own row) |
| P2 time bars ignored `group_by` | card-level mismatch naming `stack_by`/`group_by` unless they agree; the profile's `stack_by` cards are unchanged | `chart_view_repair` (red) |
| P2 tiny negative headline lost its sign | `>-0.0001` versus `<0.0001` | `chart_view_repair` (red) |
| P2 fractional seconds dropped in Details | interval text keeps nonzero fractions; Info timestamps stay whole seconds | `chart_view_repair` (red) |
| P2 signed cells with positive net showed shares | signedness comes from the input cells | `chart_view_repair` (red) |
| P2 opt-in stderr vanished on failure | bounded sanitized excerpt returned with the failure when `capture_stderr = true`; default still discards; flooding stderr on a failing command stays bounded | `chart_runtime` (the old assertion that failures carry no stderr was replaced; **no red run captured**, written after the code) |
| P2 startup queried every chart twice | startup and quota-timer emissions run under `AutomaticRefresh`; only a user click requests a chart refresh | bin unit test (red: did not compile), and the live suite counts exactly one latency-source run after load |
| P2 guide called every incomplete total a lower bound | reworded; "At least" only for declared nonnegative unknowns | docs |
| P2 design called today's syntax unsupported | removed | docs |

Results: `just format` then `just check-ci` exit 0 (lib 89, bin 30, foundation 41, integration 4, mapping 33, runtime 39, view 41, view-repair 9, cli 8, plus the shell suites). The chart-only compositor suite (`SIDESCOPE_VISUAL_CHARTS_ONLY=1 just test-visual`) exited 0; the live check now also proves a single startup query. Captures reopened at 640 px and 378 px: the profile's models card now draws a hatched grey **Unknown** row beside `model-one`, the many-series ranking still shows Other for fifteen hidden series, and no horizontal clipping appeared. A full 21-theme and density sweep was **not** rerun: nothing outside chart cards and chart-only CSS changed, and the existing suites do not render charts.

The first-full-run visual failure remains **unproven**: the earlier green runs only show it is not deterministic, and the key-resend hardening is a mitigation, not a proven fix.

Next gate: a second independent review of this repair commit. This record is not a graduation of the charts documents.

## Verification round: two repairs found partial

An independent verification of the repair commit resolved twelve findings and blocked two that had been only partly repaired. Both are corrected here; the table above overstated them.

- **Ranked capacity (P1).** The first repair budgeted time series and time bars only. A ranked card with 17 distinct Unknown tuples drew 17 rows with no refusal, and 17 ordinary groups with `top_n = 16` drew 16 named rows plus Other. Red, on the previous commit: `cargo test --test chart_view_repair` failed 2 of 11 (`seventeen_unknown_tuples_refuse_a_ranked_plot_and_keep_every_detail_row`: empty message; `ranked_plots_never_show_more_than_sixteen_rows_and_keep_the_full_ranking`: "0 unknown + 17 named, top 16: 17 rows"). Fix: the ranked named limit is the smaller of `top_n` and the checked remaining capacity (one slot for Other, one per Unknown group); Unknown groups alone beyond the plot refuse visibly, and the full ranking stays in Details. Green: 11 of 11, and the 41 existing view tests unchanged.
- **Buffered opt-in stderr at timeout and cancellation (P2).** The first repair drained stderr only on success and nonzero exit. Bytes written in the last polling interval before a deadline, or before a cancellation, were lost. Red, on the previous commit: `cargo test --test chart_runtime stderr_written` failed 2 of 2 (cancellation round 0 and timeout round 0 returned empty stderr although a marker file, created only after the write returned, proved the bytes were in the pipe first). Fix: on cancellation and timeout the owned process group is stopped and reaped as before, then the already-buffered stderr is read once under the existing size and per-pass bounds; nothing waits after the deadline. Default runs still discard stderr (`capture_stderr` opts in). Green: `chart_runtime` 41 of 41 (39 before plus these 2), the five existing `process::` tests unchanged.

The tests prove the write preceded termination through the marker rather than a sleep, and repeat (25 cancellation rounds, 10 timeout rounds) because the pre-fix race is probabilistic; with the fix the outcome is deterministic.

Still true: the first full-run visual failure is unproven (green reruns do not establish its cause); host-live collector behavior, screen-reader text, other compositors and a wall-clock midnight run are unverified; the midnight deferral and original stderr-on-failure tests had no historical red run. Next gate: parent verification of these two corrections. No graduation of the charts documents.

Final results for this round: `just format` then `just check-ci` exit 0 (lib 89, bin 30, foundation 41, integration 4, mapping 33, runtime 41, view 41, view-repair 11, cli 8, shell suites). The chart-only compositor suite exited 0; the many-series ranking capture (five rows plus Other for 20 groups) is unchanged at 640 px.
