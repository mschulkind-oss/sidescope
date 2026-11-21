---
title: "Command-driven charts QA"
date: 2026-11-20
status: in-review
stage: BUILT
next: "Host-live verification with the real collector and a screen reader; investigate the unexplained first visual-suite failure"
tags: [qa, charts]
summary: "Gates, every red run and repair, review sequence, captures inspected, and what remains unverified for the command-driven charts; synthetic data only."
---

# Command-driven charts QA

Integration of the command-driven charts, documented in the [reference](../reference/command-charts/README.md); open work is in the [tasks](../tasks/command-charts.md). Everything below used synthetic commands and fixtures; **no real agent-activity ledger, account or credential was read.**

## Gates run at the initial integration (`e86379e`)

The final results at `14fb83d` are in [the final state](#final-state-at-14fb83d); the counts below are the earlier ones.

| Command | Result |
| :--- | :--- |
| `just format`, `just check-ci` (fmt, clippy `-D warnings`, all cargo suites, dev-tool/watch/deploy/release shell tests) | passed after the repairs below; suites: lib 89, bin 29, `chart_foundation` 41, `chart_integration` 4, `chart_mapping` 31, `chart_runtime` 35, `chart_view` 41, `cli` 8 |
| `just test-visual` (private headless Sway: 21 themes, density, all existing suites, then `scripts/visual-charts.sh`) | **exit 0** on the third full run (logs were kept outside the repository) |
| `SIDESCOPE_VISUAL_CHARTS_ONLY=1 just test-visual` after the final script hardening | exit 0 |

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
| P1 producer "unknown" model fell into Other | new generic mapping key `unknown_value` per dimension (rule R16, described in the reference); the profile sets it for the model dimension; series identity keeps the raw text | `chart_mapping` (red), `chart_view_repair` (Unknown below the top-N cut stays its own row) |
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

The review repair round was then independently verified; see the next section.

## Verification round: two repairs found partial

An independent verification of the repair commit resolved twelve findings and blocked two that had been only partly repaired. Both are corrected here; the table above overstated them.

- **Ranked capacity (P1).** The first repair budgeted time series and time bars only. A ranked card with 17 distinct Unknown tuples drew 17 rows with no refusal, and 17 ordinary groups with `top_n = 16` drew 16 named rows plus Other. Red, on the previous commit: `cargo test --test chart_view_repair` failed 2 of 11 (`seventeen_unknown_tuples_refuse_a_ranked_plot_and_keep_every_detail_row`: empty message; `ranked_plots_never_show_more_than_sixteen_rows_and_keep_the_full_ranking`: "0 unknown + 17 named, top 16: 17 rows"). Fix: the ranked named limit is the smaller of `top_n` and the checked remaining capacity (one slot for Other, one per Unknown group); Unknown groups alone beyond the plot refuse visibly, and the full ranking stays in Details. Green: 11 of 11, and the 41 existing view tests unchanged.
- **Buffered opt-in stderr at timeout and cancellation (P2).** The first repair drained stderr only on success and nonzero exit. Bytes written in the last polling interval before a deadline, or before a cancellation, were lost. Red, on the previous commit: `cargo test --test chart_runtime stderr_written` failed 2 of 2 (cancellation round 0 and timeout round 0 returned empty stderr although a marker file, created only after the write returned, proved the bytes were in the pipe first). Fix: on cancellation and timeout the owned process group is stopped and reaped as before, then the already-buffered stderr is read once under the existing size and per-pass bounds; nothing waits after the deadline. Default runs still discard stderr (`capture_stderr` opts in). Green: `chart_runtime` 41 of 41 (39 before plus these 2), the five existing `process::` tests unchanged.

The tests prove the write preceded termination through the marker rather than a sleep, and repeat (25 cancellation rounds, 10 timeout rounds) because the pre-fix race is probabilistic; with the fix the outcome is deterministic.

Still true after this round: the first full-run visual failure is unproven (green reruns do not establish its cause); host-live collector behavior, screen-reader text, other compositors and a wall-clock midnight run are unverified; the midnight deferral and original stderr-on-failure tests had no historical red run.

Final results for this round: `just format` then `just check-ci` exit 0 (lib 89, bin 30, foundation 41, integration 4, mapping 33, runtime 41, view 41, view-repair 11, cli 8, shell suites). The chart-only compositor suite exited 0; the many-series ranking capture (five rows plus Other for 20 groups) is unchanged at 640 px.

## Review sequence and acceptance

The actual sequence, without compression:

1. `e86379e` passed the full gates (above) on its third full visual run.
2. An independent review of `e86379e` found six P1 and eight P2 issues.
3. `e78d8ac` repaired all fourteen. The original reviewer verified twelve as resolved and found two only partly repaired: ranked-plot capacity and buffered stderr at timeout and cancellation.
4. `14fb83d` repaired those two (red-first for both; see the previous section).
5. **No final reviewer verdict exists for `14fb83d`.** The two corrections were accepted by the parent session instead: it read the 229-line diff directly, reran `chart_view_repair` (11 of 11) and the two `stderr_written` tests of `chart_runtime` (2 of 2), then compiled a separate probe against the public runner and ran three near-deadline diagnostics. Each used a fresh marker file created only after the stderr write returned, which proved the write preceded termination, and each finished near its one-second deadline (about 1.0009 s, 1.0173 s, and 1.0017 s). The probe and its log are retained outside the repository.
6. The worker's final full `just check-ci` and chart-only compositor suite at `14fb83d` passed (below). The independent review was not repeated over the whole range: the initial review, the bounded repair verification, and the parent's direct acceptance are the evidence.

## Final state at `14fb83d`

| Command | Result |
| :--- | :--- |
| `just format`, then `just check-ci` | passed; 298 Rust tests: lib 89, bin 30, `chart_foundation` 41, `chart_integration` 4, `chart_mapping` 33, `chart_runtime` 41, `chart_view` 41, `chart_view_repair` 11, `cli` 8, plus the dev-tool, watch, deploy, and release shell suites |
| `SIDESCOPE_VISUAL_CHARTS_ONLY=1 just test-visual` | exit 0 |

The full 21-theme and density sweep was not rerun after the repair commits; nothing outside chart cards and chart-only styles changed, and the existing suites do not render charts. The documentation stage changed no source, Cargo, or fixture file.

## Not verified, and where it is routed

Everything in [not verified](#not-verified) and the unproven first visual failure remains open and is tracked in the [task list](../tasks/command-charts.md) and the [roadmap](../../roadmap.md). In short: the owner's real collector and desktop, screen-reader announcement text, SIGKILL cleanup of descendants, other compositors' Details placement, a wall-clock midnight run, and a historical red run for the midnight deferral and the first stderr repair. No before/after release size was measured, and none is claimed. Do not read any green rerun as erasing the red first run: its cause is unknown, and the second red run was a test-context error (`GTK_A11Y=none`), not a product defect.

## Mapper parent guard, chart v1.1 and typed card facts (appended)

- **Defect: an objects-layout answer was drawn as exact zero.** `column()` returned `Absent` for any missing pointer and `omitted = "zero"` zero-filled it, so a ledger answer whose values sit one level deeper than the mapping says (`/columns/knownCostUSD` with `values: [{"knownCostUSD": ...}]`) mapped to one series of complete zeros: missing read as zero. Witnessed red before the fix: `cargo test --test chart_mapping` failed 4 of the 4 new tests (`objects layout was drawn: Ok(...)` from the first), 33 existing passed. Fix: a missing leaf is zero only when its immediate parent is an object, or an array and the leaf a canonical index; green 37 of 37.
- **Limit of the guard.** It does not catch every wrong leaf name (a typo under an existing object is still an omitted leaf) and does not prove the author's intent; a producer that really omits a column is still read as the declared zero.
- **Behavior moved with the facts refactor.** The GUI model is now `present_card(reduce_card(..))`; every existing view test passes unchanged (`chart_view` 41 existing, `chart_view_repair` 11). Two texts are new: the sampled time-bars mismatch and the replay banner; a third, `No intervals in this range`, replaces what was `No known values in this range` for an answer with series but no interval. The collapse overflow text `invalid dataset: sum overflow` is now the same `Totals exceed the supported numeric range` as the other two overflow sites.
- Nothing here ran a real command; every fixture is synthetic. Sampled-gauge drawing was tested through the pure model, not through a compositor capture.

## Chart authoring and debugging tools: integration QA (2026-11-20)

Scope: the headless `sidescope charts` commands, the two quota wrappers and three example profiles, built in four tasks and landed in `bb6dc52` (typed card facts, sampled gauges, the mapper parent guard), `e414206` (wrappers, fixtures, profiles), `5074740` and `232aaec` (the commands), then verified here with the compatibility tests, the cookbook and key matrix, and a private-compositor run. Reference: [authoring tools](../reference/command-charts/authoring-tools.md). Everything used synthetic commands and fixtures; **no real producer, ledger, quota journal, account, credential or collector was run or read.**

### Red events, preserved

- **The adapter lane failed the receipt cleanliness gate** because a pre-existing, untracked setup symlink made the worktree look dirty. That red event stands as the lane's record. The gate lives outside this repository and has since been narrowed to exempt only that one managed link; this stage did not change it and no product file changed for it, so a later green gate is not evidence that the first run passed.
- **Independent review of the commands (`5074740`)** found one major and five minor findings. The major, a refused exit-0 answer reporting `freshness: pass`, was repaired red-first in `232aaec` (`a_refused_exit_zero_output_never_reads_as_fresh` failed before, passes after). The five minors are left as the backlog below; none blocks.
- **A key-matrix defect found in this stage.** `charts schema --format text` escaped the enum-choice separator twice (`\\|`), which ends a Markdown table cell early. Witnessed red first: `cargo test --test charts_cli_usage schema_text_escapes_the_choice_separator_once` failed on the unfixed code; after a one-line fix in the descriptor module it passes, and the generated matrix file now equals the command output. This is the only source change in this stage (plus a regression test); no other source file changed.

### Evidence

| Check | Result |
| :--- | :--- |
| `just format`, then `just check-ci` | exit 0: formatting, clippy with warnings as errors, 476 Rust tests and the four shell suites |
| Rust tests by binary | lib 97, bin 30, `chart_compat` 22 (new), `chart_cookbook` 3 (new), `chart_facts` 29, `chart_foundation` 48, `chart_integration` 4, `chart_mapping` 37, `chart_profiles` 15, `chart_quota_wrappers` 42, `chart_runtime` 41, `chart_view` 47, `chart_view_repair` 11, `charts_cli_capture` 7, `charts_cli_replay` 15, `charts_cli_run` 7, `charts_cli_usage` 13, `cli` 8 |
| Cookbook and matrix | every runnable cookbook block was run as written and matched its printed lines; mutating one printed value, and then one matrix line, each failed the matching test, and regenerating restored it |
| Private compositor, chart suite only (`SIDESCOPE_VISUAL_CHARTS_ONLY=1 just test-visual`) | exit 0 (all seven demo scenarios, Details keyboard and in-place update, the live synthetic profile with a hung source and SIGTERM cleanup) |
| Docs | `vantage-check --strict` on every touched or referring page: nothing to fix; `vantage-check index` runs and lists only the two existing unrelated open questions; no `[planning.stages]` is declared, so Ready-to-build, Ready-to-graduate and Stage-conflict sections are not produced and no automated stage check was available |

The compatibility tests drive the real binary over the imported and invented fixtures and assert hand-derived values (the jq line that derives each is above the test): an empty answer, an all-zero answer, unpriced lower bounds, the metered subtotal beside stacked subscription channels, the open-bucket downgrade, the 23-hour and 25-hour days, the objects layout refused instead of drawn as zero, the producer's error body as stdout and as stderr of a failing producer, the capability check with and without the quota columns, exact quota points through the estimate wrapper and the bounded-percent refusal, every observed selector and edge case, the late-correction pair through the runtime's publication path, and the filtered `recent` card on a group-none answer. They pin landed behavior, so none was first red; the cookbook and matrix tests were first red because their documents did not exist.

### Public CLI semantic proof

Through the real binary, no display: a candidate with one mismatched card reports that card refused with its siblings plotted; `replay` is byte-identical on repeat; `replay --from-capture` equals the `run` that wrote it on every semantic fact, with `data_origin` `replay`, a null attempt and `execution` and `freshness` `not-run`; usage errors, a missing explicit configuration, unreadable data and `--at` on `run` each print exactly one JSON document with exit 2; no source is spawned for an unselected id.

Hostile boundaries exercised by hand: an existing capture file (`capture-path`, nothing written); a 1 MiB plus one byte data file and a missing data file (`data-unreadable`); non-UTF-8 data (a `parse` finding, exit 1); a capture directory with no manifest (`data-unreadable`); `run` with no selector (`usage`, no process started, no marker). A good capture directory is `700` with `600` files.

### Pictures inspected

The ignored captures of the sampled-gauge card (`stripe-charts-sampled-wide-bottom.png` at 640 px and `stripe-charts-sampled-narrow-5.png` at 378 px) were opened and read, not merely produced. The card shows seven synthetic readings as marks at their own instants, joined only between consecutive readings; the reset and the 55-minute gap each break the line (a four-reading segment, one isolated mark, a two-reading segment); the headline reads "Last observed 14.00%" with "as of" the newest reading's instant; the time axis and the Details and Info buttons are intact at both widths; and in the live capture the card renders beside the other synthetic charts without overlap. The first full-suite captures did not show this card on screen (the page step skips it), so a dedicated capture was added to the visual script. Observed in the pictures and left alone: the "as of" instant prints with a `+00:00` suffix.

### Not verified

- A real producer or account: the wrappers were checked against the producer's source and published fixtures only.
- A request deferred at local midnight (scheduler unit tests only), a stdout file cut short by a full disk, host-desktop behavior, screen-reader speech and accessibility, and release size: unchanged from the sections above.
- The unexplained first full-suite visual failure stays unexplained; the green chart-only run here does not explain it.
- The full 21-theme sweep was not rerun; this stage changed no theme or non-chart style.
- An existing capture directory keeps its own permissions (only the files are forced to owner read-write), because only a directory the command creates is made owner-only.

### Backlog (nonblocking, from the independent review)

1. Reserved payload names are refused only in their defined position: a `sampled` or `breaks` at the top level or on a series object of a `sidescope-chart-v1` payload is accepted, and a stray series-level `breaks` in a v1.1 payload is accepted and never applied. Refuse them at every structural level, or narrow the reference wording.
2. A ranked gauge card over 16 series with no intervals reports capacity before no-intervals (the time card has the planned order).
3. Ranked Details place Unknown rows after every named row rather than in value order (unchanged GUI behavior); document or sort.
4. The `--offset N, --limit N` help line runs into its description with no separator.
5. `quota-estimate.py --convert-stdin` does not compare the answer's echoed grouping with its flags (unreachable in live mode); a test currently pins the mismatched conversion.
