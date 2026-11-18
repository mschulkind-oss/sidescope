---
title: "Dashboard expansion QA"
date: 2026-10-05
status: in-review
stage: BUILT
next: "Independent acceptance review and owner-configured host verification"
tags: [qa, dashboard, wayland]
summary: "Actual quality-gate and private Sway observations; live account and user-location checks not performed."
---

# Dashboard QA

**Status:** Historical expansion evidence below is based on `42e6753`. The [compact redesign follow-up](#compact-redesign-follow-up), against `c1dcc79`, supersedes the old tall-weather/core and reserved-free-space observations; it passed independent acceptance review and the parent's full integration gate.

## Automated evidence

- Regression tests first failed for missing DeepSeek text-section balance and OpenRouter extra comparison/month metric. Both now pass. A negative-money regression and malformed report CLI regression also failed before correction.
- `just format` passed. `PATH=/bin:$PATH just check-ci` passed: formatting, Clippy with warnings denied, **49 Rust tests** (37 library, 9 UI-routing/state, 3 CLI), including the independent-review security/lifecycle repairs below, native wrapper/doctor shell tests, debounced dev-watch/failed-build/child cleanup tests and isolated release tests. Release tests push only into their own temporary repository; this working tree has no new commit/tag.
- New tests cover balance-only normalization; CPU guest/iowait, reset/hotplug and bounded histories; MemAvailable RAM, separate/absent swap; network elapsed time, loopback, reset/hotplug; disk arithmetic; real local Linux sampling smoke; explicit location/units; current/twelve-hour/five-day forecast projection, malformed/null/time/unit states; modeled AQI/PM2.5; bounded subprocess output/time and environment forwarding; independent remote stale retention without growing errors; malformed report rejection before GTK.
- `just test-visual` passed after repairing the private harness's session bus/schema setup. The harness asserts **640 px at 2560×1440**, focus/blur header/footer changes at 640 and 378 px, `?`/`Esc`, keyboard scrolling, and no GTK/Adwaita warnings (including missing BreakpointBin minimum size).
- The visual harness uses synthetic deterministic dashboard data, a private Sway compositor, a private D-Bus session and a browser *placeholder*. GTK accessibility services are disabled only inside this isolated harness; this is not an accessibility audit.

## Independent-review repairs

- Four new process/config regressions were run against the defective implementation first: all failed (ambient `.curlrc` was read; wrapper descendants remained alive after timeout and direct-child success/failure). The combined failure/overflow test initially stopped at its failing first case; its oversized-output cleanup case is also verified after repair. All five process tests now pass, including the existing normal-output/environment/budget test.
- Curl now receives `--disable` as its **first argument**. The isolation regression uses a temporary synthetic `.curlrc` with a fake Authorization header, extra URL and invalid-option sentinel, and runs real curl with `--version`: no transfer, real credential or ambient personal config is involved.
- Sources run in isolated process groups. One nonblocking stdout/deadline loop replaces the detached blocking reader; RAII terminates the group on success, timeout, failure, read/setup errors and output overflow, and reaps the direct child. Exit is observed without reaping before group termination, avoiding PID reuse while cleanup is pending. Successful buffered output is preserved, including exactly 1 MiB; invalid UTF-8 remains rejected.
- Synthetic shell wrappers fork a pipe-holding sleep. Regressions check bounded return, preserved successful output, stopped descendants and `ECHILD` after direct-child reap. No reader thread remains because none is created. As approved in follow-up coordination, orphan zombie reaping belongs to init (tests accept absent or zombie descendants); a descendant deliberately escaping the process group is outside this boundary. No app-wide subreaper behavior added.
- After these repairs, the parent independently ran `just format`, `PATH=/bin:$PATH just check-ci`, `just test-visual` and `git diff --check`: all passed. The full gate passed 49 Rust tests and all shell checks; regenerated 640 px and narrow forecast/air screenshots were directly inspected.

## Direct image inspection

Opened ignored PNGs with the image-read tool, not merely checked capture success:

| Local image | Actual observation |
| --- | --- |
| `screenshots/stripe-column.png` | Two independent 640 px masonry columns; OpenRouter $18.40 and DeepSeek $9.84 show only current balance, no comparison/bar. Daily history unavailable is explicit. CPU total and logical-core traces are visible; RAM and swap have separate values/traces. No erroneous "No metrics reported" banner on graph-only CPU. |
| `screenshots/stripe-dashboard-middle.png` | Core traces remain legible; receive/transmit rates and scaled traces, disk used/free, current weather and hourly steps, and modeled AQI/PM2.5 are visible. Source/model labels are explicit. |
| `screenshots/stripe-dashboard-bottom.png` | All twelve hourly steps plus five low/high days and conditions fit within the tall scrollable weather card; Open-Meteo attribution/model time remains visible. Left column ends earlier: expected empty tail beside the last tall card, not shared-height row gaps. |
| `screenshots/stripe-narrow.png`, `screenshots/stripe-narrow-dashboard-middle.png` | Single-column order at 378 px; labels/values wrap without sideways clipping. RAM used/total wraps cleanly; footer stays fixed while content scrolls. |
| `screenshots/stripe-narrow-dashboard-bottom.png` | Forecast tail and modeled air card are keyboard-reachable and readable, including units and source attribution. |
| `screenshots/stripe-help.png` | Centered readable shortcut guide over dimmed dashboard; dismissal restores the dashboard. |

Dashboard captures were regenerated again after the process-only review repairs. No authenticated/account screenshots were taken or committed.

## Failures encountered and repaired

- Initial balance/negative-money/CLI regression failures were intentional and then fixed.
- Ordinary compile/lint failures (GTK accessibility API feature gate, type-complexity, collapsible-if) were corrected; graph text labels/tooltips preserve descriptions without raising the minimum GTK dependency.
- Plain `just check-ci` hit this jail's PATH-injected blocked `find` wrapper in the *existing* dev-watch test. Rerunning with the installed `/bin` tools first passed the entire gate, without editing the watcher or jail configuration.
- Strict visual warning detection initially exposed pre-existing missing GLib schemas/session bus, then an unusable default D-Bus config and absent test-bus accessibility service. The harness now supplies a private minimal bus config, locates the installed GTK schemas and explicitly disables test-only accessibility services. Final run has no GTK/Adwaita warning match.

- `uvx vantage-check` (0.8.1) checked all ten touched Markdown documents with no errors; two pre-existing deprecated `oq` directives remain in the broader design because reader-version policy was not changed. The planning index still routes the two existing owner decisions; this repository declares no stage-role vocabulary.

## Not established / configuration required

- No authenticated ai-usagebar provider request, full five-minute live polling soak, or real user-location forecast/AQ request was performed. Official Open-Meteo API/terms pages and public upstream report/panel source were retrieved with curl; remote parsing was checked with synthetic fixtures only.
- User must install/configure ai-usagebar for `just dev`, retain their normal inherited credential environment, install curl, and explicitly set latitude/longitude/place/units in the non-secret preferences. Without location, no remote request occurs.
- No dated trustworthy provider daily-history contract was found. History is unavailable, never inferred from balance deltas/top-ups. PurpleAir is not implemented; the approved alternative is Open-Meteo/CAMS **modeled**, not a sensor reading. Free API use is non-commercial under the checked terms.
- Local telemetry is Linux-only; histories are bounded session-memory samples, root disk is `/` only, virtual interfaces can double-count traffic, and very high core counts create a long card. The GTK host rebuilds card views on sample updates; target-host resource/scroll soak remains to be measured.
- No real browser/workstation, other compositor, scaling/large-font accessibility audit or performance benchmark. Source HTTP errors are deliberately generic, not a provider-specific retry/auth taxonomy. Email, arbitrary custom plugins and settings UI remain outside this change.

## Compact redesign follow-up

Implemented against `c1dcc79`, 2026-10-05. No staging/commit, authenticated requests, personal config or personal weather screenshots. Public widget inspiration and mechanisms are in [research](../research/dashboard.md#compact-visual-mechanisms--2026-10-05-follow-up).

### Failure-first and final gates

- Extracted the old X formula into `src/plot.rs` and ran two regression tests **before fixing it**. A single sample mapped to 119 instead of 238; two-sample spacing was 238 instead of 2 on a 238 px test plot. Both failed, then passed with fixed 120-slot right alignment. Tests cover empty/one/two/partial/full/rolling/oversized histories, percentage bounds and missing slots.
- Disk regression first returned `(70, 30)` instead of `(70, 20)` for 100 total / 30 free / 20 available blocks. Now Used remains 70, Available is 20 and reserved difference is 10. Tests reject available > free, free > total, zero block size and multiplication overflow. `root_disk` uses f_bavail for Available, not total-minus-available for Used; labels/README match df semantics.
- Structured weather regression first failed because the old card emitted 18 generic metric blocks. Current tests cover current/twelve hours/five days, Celsius/Fahrenheit, unknown glyph codes, all-null/partial/constant curves, missing/reversed daily ranges and full hourly text alternatives. Sampler tests cover missing failure/recovery slots, reset/new cores, numeric core ordering and bounded histories. UI tests retain source-fetched timestamps when shortening balance prose, and retain structured weather on stale failures.
- `just format`, `PATH=/bin:$PATH just check-ci` and `just test-visual` passed. The full gate runs **58 Rust tests** (44 library, 11 UI/state, 3 CLI), warning-denied Clippy/format checks and native/dev-watch/isolated-release shell tests. No repository commit/tag created. `git diff --check` passed; `git diff --cached --name-only` is empty.
- Visual harness asserts 640/378 px geometry, focus/blur, shortcuts/scrolling, no GTK/Adwaita diagnostics, weather ≤380 px and core rows ≤32 px. Nine deterministic synthetic scenarios cover one/two/full/rolling/missing history, Fahrenheit, stale, unconfigured and 64 cores. The fixtures are demo-only; no live data path is changed by their scenario environment variable.

### Measured density

GTK vertical measurement at the same representative **288 px / 350 px outer-card widths**, before and after; logs are under ignored `screenshots/`:

| Card | Before wide / narrow | After wide / narrow |
| --- | --- | --- |
| Weather | 1222 / 1188 px | **376 / 376 px** |
| CPU, eight logical cores | 642 / 625 px | **258 / 258 px** |
| OpenRouter / DeepSeek | 215 / 199 px | 130 / 130 px |
| Claude / Codex | 309 / 309 px | 236 / 236 px |
| Memory | 368 / 324 px | 236 / 236 px |
| Network | 303 / 303 px | 236 / 236 px |
| Disk | 191 / 191 px | 136 / 136 px |

Core row contents measure **24 px**; paired rows have 2 px gaps, replacing approximately 56 px stacked label/chart rows. Sixty-four cores measure 986 px and remain scrollable; at 640 px the entire grid (cores 0–63) fits in the synthetic system-only capture. Shared padding is 10 px/card and 12 px/stack, down from 18/25; the header is roughly 71 px instead of 120, footer roughly 34 instead of 48. Supporting text is ≥12 px, main metric values 16 px, titles 20 px. Weather uses roughly 280 px of plot/card content at the target window width.

### PNGs opened directly

All paths below are local ignored artifacts, not committed assets:

- `screenshots/compact-before/stripe-column.png`, `.../stripe-dashboard-bottom.png`: original tall cores/weather, measured in `.../stripe-geometry.log`.
- `screenshots/stripe-column.png`, `stripe-dashboard-middle.png`, `stripe-dashboard-bottom.png`: tightened masonry; CPU total + paired cores; ordinary unfilled trend lines with faint grids and unused warm-up space; network auto-Y labels; compact weather with real Cairo sun/cloud/rain glyphs, temperature curve, five-day ranges and visible source/model time. The middle and bottom captures now look nearly identical because little vertical scrolling remains at 640 px; this is expected, not a lost scroll action.
- `screenshots/stripe-narrow.png`, `stripe-narrow-dashboard-bottom.png`: single-column 378 px layout, readable labels/values and full forecast/source footer without sideways clipping.
- `screenshots/stripe-scenario-{one,two,full,rolling}-wide.png`: lone right-edge dots, short right-edge two-point segments, then full-width and rolled traces. No startup stretching.
- `screenshots/stripe-scenario-missing-wide.png`: line breaks preserve missing temporal slots; absent daily endpoints display `—` and do not acquire invented ranges.
- `screenshots/stripe-scenario-one-hourly-details.png`: real Tab/Shift-Tab/Space events open the native Details popover; all twelve hourly temperatures/conditions are visible as text. Escape dismisses it.
- `screenshots/stripe-scenario-fahrenheit-narrow-bottom.png`: Fahrenheit current/curve/daily values; no Celsius label left behind.
- `screenshots/stripe-scenario-stale-wide.png`, `stripe-scenario-unconfigured-wide.png`: retained values and model/source attribution remain stale-labeled; unconfigured cards show configuration guidance, no fabricated forecast.
- `screenshots/stripe-scenario-high-cores-wide.png`, `...-wide-bottom.png`, `...-narrow.png`, `...-narrow-bottom.png`: 64 compact core traces, numeric order and keyboard-reachable lower cards.

### Repairs and remaining limits

- Initial symbolic weather icons appeared as missing-image placeholders in actual PNGs: the optional icon names are absent from this environment's theme. Replaced only these glyphs with small local Cairo drawings; no new dependency or theme configuration. An attempted accessibility helper was feature-gated by gtk4 v4_10; kept existing minimum dependencies and provided a native keyboard-reachable text popover instead. Initial Return-only popover capture did not activate it; normal focus traversal + Space does, and the final image was inspected.
- No known blocker remains in checked synthetic/default-font modes. Long place labels, larger fonts/scaling, a full screen-reader audit and other host/compositor themes are not accepted by these measurements. Stale/errors and unusually long labels may intentionally exceed the ready synthetic weather budget.
- No real account, owner-location forecast, DST transition, target-host df side-by-side capture, live telemetry/polling soak or performance benchmark. The disk formula was compared to local statvfs/df counters without a screenshot; filesystem activity can change bytes between reads. Sources still have the existing bounded-process/privacy/refresh behavior. The GTK host still rebuilds views on telemetry updates; very high core counts are not virtualized, though 64-core geometry/scrolling is checked. Graph slots represent the sampler's nominal two-second cadence, not a new wall-clock timestamping system.
- Independent review found no remaining issues after the focused repairs. The parent independently passed the full integration gate and inspected regenerated wide, narrow, live-popover and axis-strip images. Owner-configured host verification remains outstanding.

### Independent-review interaction/axis repairs

The independent review blocked the initial implementation on Details lifetime under local sampling; the static `--report` popover image did not prove live-update behavior. That limitation is now covered explicitly:

- **P1 failure first:** `just test-visual` with the new `live-details` regression failed on its first two-second update: `local sample replaced unchanged weather Details widget` (`/tmp/compact-review-failure.log`). The fixture opens the actual native popover, delivers three different synthetic local samples at the production cadence, and uses the same `apply_local_sample` path as the real sampler.
- **P1 repair:** Local ticks retain both layouts' existing card containers/columns, replacing only CPU/memory/network/disk child contents. Unchanged remote weather stays rooted; the same Details widget, open popover and window keyboard-focus object survive. Source topology changes and non-local source renders may still rebuild the full layout. Final log `screenshots/stripe-scenario-live-details-geometry.log` records ticks 1/2/3 with `visible=true focus_preserved=true`, then real Escape/Space events produce closed/open states at update count 3. Directly opened `...-after-updates.png` and `...-keyboard-reopened.png` show the updated CPU value and all twelve hourly text rows.
- **P2 diagnosis:** The apparent absent 12:00 in long-image previews was not reproduced as absent pixels: GTK reported a valid text node, correct bounds, opacity 1 and enough allocation; a full-resolution pixel crop showed 12:00/17:00/23:00. Renderer selection did not alter this. Do not infer clipping from scaled full-column previews alone. The former axis nevertheless used wrapping, expanding labels; a new nonwrapping-axis invariant failed against that layout (`/tmp/compact-axis-failure.log`).
- **P2 hardening/evidence:** Axis start/center/end now use natural-width nonwrapping labels in a CenterBox. Geometry checks enforce text fits its allocation. The harness samples actual compositor pixels within both endpoint allocations and captures `screenshots/stripe-scenario-{one,two,full,rolling,missing}-axis.png`; all five strips were opened directly and clearly show **12:00 / 17:00 / 23:00**. All applicable scenarios pass endpoint pixel checks (298 bright start components; 285 end components). No image library, renderer override or extra runtime dependency remains in the implementation.
- **Final review-repair gates:** `just format`, `PATH=/bin:$PATH just check-ci` (58 Rust tests plus all shell checks), `just test-visual` (now ten scenarios including synthetic live updates) and `git diff --check` passed. Logs: `/tmp/compact-review-check-ci.log`, `/tmp/compact-review-visual.log`. Regenerated wide/narrow, live-popover and axis-strip images were inspected directly. Weather/core density is unchanged. The parent repeated formatting, all 58 Rust tests and shell checks, the ten-scenario visual harness and diff validation successfully before integration; independent re-review approved the repairs.
- The live-update regression is synthetic, not an owner-location/account check or a long resource soak. Independent re-review remains required.

## Themes follow-up

Implemented against `main8b15256`, 2026-10-05; **independently reviewed and verified for integration**. No staging/commit, personal config, account request or credential capture. Research/mechanism, plan and task sections are appended to the corresponding dashboard documents.

### Failure-first and checked gates

- Clear/61°F at the representative **288 px masonry card** failed first: `ordinary weather conditions must never word-wrap at 288px card width` in `/tmp/themes-clear-failure.log`. The production label now does not wrap, expands into the available row and end-ellipsizes when necessary. GTK logs Clear at **174×31, not ellipsized**; the default weather budget remains **376/376 px** at the settled wide/narrow measurements.
- Tight long-condition regression at a 512 px two-column window logs `CONDITION_TIGHT ellipsized=true tooltip_complete=true width=110`; its PNG shows `Thunderstorm …` instead of a split word. Tooltip and the keyboard-reachable Details text retain the full current condition; Details also retains every hourly value.
- `just format`, `PATH=/bin:$PATH just check-ci` and `just test-visual` passed after final repairs. **63 Rust tests** (48 library, 12 UI/state, 3 CLI), warning-denied lint/format and all native/dev-watch/isolated-release shell tests passed. Logs: `/tmp/themes-format.log`, `/tmp/themes-check-ci.log`, `/tmp/themes-visual.log`.
- Independent review found one alternative-theme omission: the Close-guide button retained native styling. A new regression failed first; alternative button/hover rules now include it while Original remains unchanged. The parent reran the full gate: **64 Rust tests**, all shell checks, all 21 wide/narrow captures and the exact baseline comparison passed. Direct inspection of Paper/Terminal help screenshots confirms themed readable buttons. The parent also directly inspected every wide theme image and targeted narrow/picker images.
- Registry tests cover 21 unique safe ids, finite/bounded complete tokens, independent six-series colors, non-palette structural distinctness, original CSS/graph/range constants, cycle wrap and config missing/default/valid/invalid/injection-shaped ids. Shortcut tests cover both modes and guide suppression.
- Real compositor keyboard events cycle all 21 themes, including reverse wrap; selector button and wheel signal callbacks, scrollable picker and final picker choice are exercised inside GTK. Wheel signal emission is synthetic, **not** physical-pointer wheel verification. The wheel controller is attached only to the footer selector, not the dashboard. Native picker keys are not intercepted by dashboard shortcuts.
- Three two-second synthetic ticks switch Paper → Terminal → Blueprint and invoke both local-only sampling and full report rendering; selected id/name persist. Existing Details lifetime/focus checks still pass. The harness compares configuration bytes before/after a full browse cycle: **no implicit writes**.
- Exact Original comparison: `python3 scripts/compare-theme-baseline.py screenshots/themes-baseline/full-wide.png screenshots/stripe-scenario-full-wide.png` passed, **857,560 pixels checked; zero differences**. Exclusions are only footer chrome (`y >= 1350`) and current-condition repair (`128 <= x < 312`, `615 <= y < 650`) in this fixed 640×1415 synthetic fixture. Original graphs, cards, title/body text, header, layout and all other checked pixels match exactly. Saved baseline PNGs remain ignored; the comparison is not a generic arbitrary-screenshot tool.
- `git diff --check` passed; `git diff --cached --name-only` is empty.

### Directly opened screenshots

Each theme's wide and narrow image was opened with the image-read tool during this implementation, not inferred from capture success. All 21 wide images and targeted narrow/interaction images were opened again after the final gate. Wide images show all six synthetic system/environment cards at 640 px; narrow images show the scrolled forecast/air/source tail at 378 px. Every listed current-temperature row keeps **Clear** intact beside **61°F**. No clipped weather values, axis endpoints, source attribution or selector name was observed.

| Theme | Wide PNG | Narrow PNG | Direct observation |
| --- | --- | --- | --- |
| Original | `screenshots/stripe-theme-original-wide.png` | `screenshots/stripe-theme-original-narrow.png` | Existing night-green cards, thin solid green traces; Clear is intact next to 61°F. |
| Paper Ledger | `screenshots/stripe-theme-paper-wide.png` | `screenshots/stripe-theme-paper-narrow.png` | Warm paper, nearly square fine borders, thin ruled multicolor charts; readable narrow ranges. |
| Phosphor Terminal | `screenshots/stripe-theme-terminal-wide.png` | `screenshots/stripe-theme-terminal-narrow.png` | Square green-black instruments, dotted point traces, yellow weather ink; clear narrow source/footer. |
| Blueprint | `screenshots/stripe-theme-blueprint-wide.png` | `screenshots/stripe-theme-blueprint-narrow.png` | Double blue frames, five-row technical grid and dashed curves; gold sun/temp and complete narrow forecast. |
| Editorial | `screenshots/stripe-theme-editorial-wide.png` | `screenshots/stripe-theme-editorial-narrow.png` | Ivory square panels, stronger ungridded ink lines, vermilion transmit; unclipped narrow values. |
| Signal Contrast | `screenshots/stripe-theme-contrast-wide.png` | `screenshots/stripe-theme-contrast-narrow.png` | Black/white bold rails, bright distinct series and pink weather; high-contrast narrow labels. |
| Retro Console | `screenshots/stripe-theme-retro-wide.png` | `screenshots/stripe-theme-retro-narrow.png` | Cream/brown chunky borders, dashed point plots with faint fill; legible narrow forecast. |
| Cloud Soft | `screenshots/stripe-theme-cloud-wide.png` | `screenshots/stripe-theme-cloud-narrow.png` | Rounded pale cards with subtle shadows, lavender filled weather curve; full narrow card/source. |
| Quiet Minimal | `screenshots/stripe-theme-minimal-wide.png` | `screenshots/stripe-theme-minimal-narrow.png` | Flat white low-radius panels, hairline plots with no grid/fill; quiet readable narrow source. |
| Nord Observatory | `screenshots/stripe-theme-nord-wide.png` | `screenshots/stripe-theme-nord-narrow.png` | Rounded slate panels, pale filled series, warm weather glyph; complete narrow five-day ranges. |
| Solar Study | `screenshots/stripe-theme-solar-wide.png` | `screenshots/stripe-theme-solar-narrow.png` | Ochre laboratory surfaces, fine dashed ink traces; readable narrow units and range track. |
| Rose Atelier | `screenshots/stripe-theme-rose-wide.png` | `screenshots/stripe-theme-rose-narrow.png` | Blush rounded shadow cards, rose weather fill and dark text; no narrow clipping. |
| Ocean Depths | `screenshots/stripe-theme-ocean-wide.png` | `screenshots/stripe-theme-ocean-narrow.png` | Teal rounded instruments, broad cyan/pale-blue curves and fills; readable narrow weather/footer. |
| Ember Workshop | `screenshots/stripe-theme-ember-wide.png` | `screenshots/stripe-theme-ember-narrow.png` | Charcoal/copper square-ish borders, dashed faint fills and gold glyph; intact narrow condition. |
| Orchid Studio | `screenshots/stripe-theme-orchid-wide.png` | `screenshots/stripe-theme-orchid-narrow.png` | Plum rounded panels, dotted points and faint fills; golden weather curve and clear narrow text. |
| Forest Fieldnotes | `screenshots/stripe-theme-forest-wide.png` | `screenshots/stripe-theme-forest-narrow.png` | Moss rounded paper, green ink filled CPU and earthy weather; complete narrow attribution. |
| Cobalt Instruments | `screenshots/stripe-theme-cobalt-wide.png` | `screenshots/stripe-theme-cobalt-narrow.png` | Thick electric-blue frames, denser grid, solid point traces; bright gold weather and narrow readability. |
| Sandstone | `screenshots/stripe-theme-sand-wide.png` | `screenshots/stripe-theme-sand-narrow.png` | Linen rounded hairlines, larger titles and fine ungridded curves; full narrow forecast/source. |
| Mint Modern | `screenshots/stripe-theme-mint-wide.png` | `screenshots/stripe-theme-mint-narrow.png` | Fresh aqua pill cards, subtle solid fills, independent purple RAM; no narrow clipping. |
| Noir Cinema | `screenshots/stripe-theme-noir-wide.png` | `screenshots/stripe-theme-noir-narrow.png` | Graphite nearly square panels, restrained fine ungridded inks and lilac weather; readable narrow labels. |
| Arcade Midnight | `screenshots/stripe-theme-arcade-wide.png` | `screenshots/stripe-theme-arcade-narrow.png` | Indigo thick frames, bright dashed point traces and subtle fills; complete narrow weather/source/footer. |

Additional directly opened ignored evidence:

- `screenshots/themes-baseline/full-wide.png` and `screenshots/stripe-scenario-full-wide.png`: unchanged Original charts/cards; only Partly cloudy becomes one line and selector occupies footer chrome.
- `screenshots/stripe-scenario-long-condition-tight.png`: tight two-column condition safely ellipsized; no character wrapping.
- `screenshots/stripe-scenario-one-hourly-details.png`: actual Tab/Shift-Tab/Space opens the native popover; current condition and all twelve hourly rows are visible.
- `screenshots/stripe-scenario-theme-picker-open.png`: numbered native picker with a bounded scroll area.
- `screenshots/stripe-scenario-theme-live-after-updates.png`: Blueprint remains selected after the third synthetic local/report update; CPU shows 3% and Cairo plots use dashed themed colors.
- `screenshots/stripe-scenario-missing-wide.png`: temporal gaps and missing daily endpoints remain explicit, not zero.
- `screenshots/stripe-scenario-stale-narrow-bottom.png`: retained forecast/AQI visibly STALE; source/model and refresh-failed wording remain readable.

### Repairs and remaining limits

Ordinary float-literal/API compile errors were repaired without adding dependencies. The new selector changed initial keyboard focus, exposing the old hourly-popover harness's implicit first-focus assumption: direct image inspection found that Space had activated Previous instead. The fixture now explicitly focuses Details before real Tab/Shift-Tab/Space and requires its open-state log. A picker regression initially traversed GTK's implicit Viewport as though it were the option list; source-named option lookup corrected it. The first long-running worker timed out during verification, not due to a code diagnostic; the revived worker reran all final gates successfully.

Only DejaVu Sans is installed in this Nix test environment. Generic Serif/Monospace directions therefore fall back here: typography tokens and title weights/sizes are configured and tested, but host serif/monospace appearance is **not visually established**. No new fonts/downloads/dependencies were added. No high-DPI/large-font/screen-reader audit, real pointer wheel test, authenticated account, owner-location forecast, compositor matrix or prolonged resource/live-source soak. Alternative larger-font host geometry remains to be checked. Semantic status/pace remains text-labeled; this is not a formal WCAG contrast audit. Independent review approved with the small button omission subsequently repaired and verified.

## Dense CPU readability repair

This follow-up supersedes the earlier CPU density/unchanged-plot observations, not the weather/data behavior.

- Inspected the user's `scratch/clip_1791215233.png` first: Retro's dots, dashes and fill overwhelmed narrow full-history core traces; the total was unnecessarily dotted too.
- Core IDs and right-aligned current values now sit above full-width 28 px plots. Rows measure **42 px**, with 6 px gaps (48 px budget). The twelve-core fixture card measures **444 px**; the intentionally taller 64-core card is **1692 px** and remains keyboard-scrollable.
- CPU traces are thin continuous unfilled lines, with quiet 0/100% guides. Other plots drop decorative points/dashes/fills below 6 px sample spacing; sparse forecast plots retain theme expression. Isolated samples still get visible dots. No values, peaks, gaps, percent bounds or fixed two-second/four-minute X coordinates change; no smoothing/downsampling was introduced. Original non-CPU style tokens remain unchanged.
- `tests/fixtures/cpu-busy.json` is a seeded (12012), synthetic twelve-core workload: 120 slots each, irregular idle/bursts, independent 100% excursions, two missing slots on two cores, and current percentages like the reference. Total is the slot-wise arithmetic mean, missing if any constituent is missing. This is not a captured account or `/proc` history.
- Failure first: the unguarded existing style failed `dense_cpu_styles_are_continuous_thin_unfilled_and_quiet` and `retro_full_history_cannot_overlay_markers_dashes_or_fill_on_signal` ("120 markers over an 80px core trace cause blobs"). New tests cover the style guard across every theme, sparse-style preservation, header label splitting/density, full fixture/current/mean/gaps/excursions, alongside existing warm-up/rolling/reset tests. Log: `scratch/cpu-tests-first.log`.
- Final gates: `just format`, `PATH=/bin:$PATH just check-ci` (**68 Rust tests** plus shell checks), `just test-visual`, and `git diff --check`. Logs: `scratch/cpu-check-ci.log`, `scratch/cpu-visual.log`. The harness also verifies Details surviving local updates, theme selection/persistence, warm-up/rolling/missing history, scrolling and GTK diagnostics.
- Directly opened **all twelve** requested dense CPU images: `screenshots/stripe-theme-{original,retro,terminal,blueprint,contrast,arcade}-wide.png` and `...-cpu-narrow.png`. Each shows the full twelve-core/120-slot fixture at actual 640/378 px window widths. Raw narrow spikes remain distinct instead of point blobs; IDs and percentages align consistently. Retro is clear dark ink on cream, Terminal/Blueprint/Arcade retain distinct surfaces and sparse forecast accents, and Contrast keeps high-contrast thin CPU ink. Also opened one/two/full/rolling/missing history and live Details/theme-update images.
- Diagnostics repaired during QA: lint rejected a constant inequality assertion; resize transiently allocated a 95 px core plot in the 220 px minimum card (width invariant now allows that valid transitional size). Narrow captures initially inherited prior scroll positions; the harness now resets the narrow view before CPU captures and preserves failure logs.
- Remaining limits: only installed DejaVu Sans/default scale in private headless Sway; host monospace/serif, high-DPI/large-font, prolonged live-source soak and compositor portability remain unverified. Higher core counts intentionally need more scrolling; no virtualization or broader layout change was added. Independent review found no issues. The parent repeated formatting, the full 68-test and shell gate, all-theme visual captures and diff validation successfully, then directly inspected regenerated Retro/Contrast/Arcade images.

## Curated theme layout correction

Implemented against `1ee724f`; independent acceptance review found no issues and approved integration. This supersedes the previous decision to make **every** CPU layout roomy: the complaint concerned Retro's decorations, not the original compact geometry. The corrected scope is **21 curated themes**, not 42 combinations or an independent layout preference.

### Layout contract

- `Theme.layout` owns CPU geometry, weather presentation, RAM/network arrangement and card spacing. Existing heading font/size/weight, component colors and graph inks are unchanged. Theme ids/config/21-choice picker remain backward compatible; an independent `layout` key is rejected. Previous/next, brackets and selector-only wheel preview the complete curated direction, without config writes.
- Original restores the `de6cf67` inline-label cores: **24 px plots, 24 px measured rows, 2 px row gaps**. Roomy uses **28 px full-width plots, 42 px measured rows, 6 px gaps**. Both retain the existing dense CPU style guard: no marker blobs, dash/fill overlays, smoothing, sample changes or gap bridging.
- Theme switches intentionally rebuild card geometry. Local ticks still leave remote Details/source text rooted; full source renders and ticks preserve the selected theme's geometry.

| Themes | CPU | Weather | RAM/network | Spacing / emphasis |
| --- | --- | --- | --- | --- |
| Original | Compact | Curve and daily rows | Stacked | Original spacing/heading tokens |
| Terminal, Blueprint, Retro, Solar, Ember, Cobalt, Arcade | Compact | Twelve-condition strip and five daily range tiles | Inline | Tight 2 px spacing, existing technical heading tokens |
| Cloud, Minimal, Nord, Mint | Roomy | Condition strip and daily tiles | Inline | Quiet 2 px spacing, existing soft/minimal headings |
| Paper | Roomy | Curve and daily rows | Stacked | Tight ruled spacing |
| Editorial | Compact | Curve and daily rows | Stacked | Tight editorial spacing |
| Contrast, Rose, Ocean, Orchid, Forest, Sand, Noir | Roomy | Curve and daily rows | Stacked | Airier 4 px spacing, existing display heading tokens |

### Checked evidence and direct observations

- Failure-first registry/config/geometry checks, then `just format`, `PATH=/bin:$PATH just check-ci`: **69 Rust tests** (54 library, 12 UI/state, 3 CLI), warning-denied lint and all shell checks passed. Final logs: `/tmp/layout-check-accepted.log`, `/tmp/layout-visual-accepted.log`. `just test-visual` passed all original scenarios, all-theme browsing/wrap/no-write checks, and added strip null/Fahrenheit/live-Details scenarios. Selection tests now assert actual CPU/weather/telemetry subtrees after both local and report renders, not only the selected id.
- Directly opened **each of all 63** final theme PNGs: `screenshots/stripe-theme-{original,paper,terminal,blueprint,editorial,contrast,retro,cloud,minimal,nord,solar,rose,ocean,ember,orchid,forest,cobalt,sand,mint,noir,arcade}-{wide,cpu-narrow,narrow}.png`. All use the synthetic **12-core × 120-slot** fixture; wide is 640 px, narrow is 378 px. Compact images show short inline-label traces; roomy images clearly separate right-aligned percentages and broader plots. Retro's green peaks remain thin and distinct, with no old dot blobs. Narrow weather/air images show complete attribution and readable selector names/indices for every theme.
- Terminal/Blueprint/Retro/Solar/Ember/Cobalt/Arcade images visibly combine inline telemetry and short condition strips; Cloud/Minimal/Nord/Mint visibly combine those treatments with roomy cores. Other presentation images retain hourly curves/daily rows. The five daily tiles preserve low/high numbers and range marks rather than expanding into old tall lists. Original non-CPU geometry remains unchanged.
- Directly opened strip missing-data wide/narrow images: three unknown-condition glyphs stay in their hour slots; missing daily endpoints remain `—` without fabricated ranges. Fahrenheit narrow image shows **64°F**, **54–70°F** hourly range and Fahrenheit daily values. Strip Details after updates and keyboard-reopened images retain all twelve textual temperatures/conditions; all three two-second ticks log `visible=true focus_preserved=true`. Original Details, theme-live, picker, tight long-condition and Paper/Terminal help images were also opened.
- Legacy Original pixel comparison uses the existing condition/footer exclusions plus only CPU drawing rectangles affected by the retained density-style repair: total `[25,305) × [226,257)`, core plots `[84,161)` / `[229,305) × [278,381)`. **833,121 remaining pixels have zero differences**, including compact labels, card geometry, non-CPU graphs and text. The strict pre-density whole-image comparator is not used to reject the intentional CPU ink changes. Log: `/tmp/layout-baseline-accepted.log`; its decoder is unchanged.
- Visual QA repaired a same-frame geometry-probe race by waiting boundedly for allocation, a missing test-scenario alias, stretched strip glyphs by centering fixed-size squares, and orphaned inline RAM units by explicitly splitting used/total quantities. All final gates/captures were rerun after these repairs. `git diff --check` passes and the index is empty; no staging/commit.

Remaining limits are unchanged: installed DejaVu Sans/default scale and private headless Sway only; no physical wheel verification, authenticated accounts, owner-location requests, host font/scaling/accessibility audit or prolonged live-source/resource soak. Theme switches may intentionally close popovers; local ticks preserve unchanged remote Details. The parent independently passed formatting, the full 69-test and shell gate, the all-theme visual harness and diff validation, and directly inspected representative regenerated compact/roomy CPU, telemetry, weather-strip and Fahrenheit captures. Independent review approved the corrected 21-theme contract.

## Screen-fit density

Implemented against `be69bd4` with Comfortable as the unchanged default card/theme geometry. Separate footer button / **D**, plus optional closed top-level `density = "screen-fit" | "comfortable"`; no config writes, theme variants, authenticated requests, staging or commits. [Plan](../plans/screen-fit.md).

### Checks and actual geometry

- `just format`, `PATH=/bin:$PATH just check-ci`, `PATH=/bin:$PATH just test-visual` and `git diff --check` passed. **75 Rust tests** (58 library, 14 UI/state, 3 CLI) plus the existing shell gates. New checks cover density defaults/closed validation, all 21 theme IDs independently, shorter-column/span planning, actual-height three/four-cell selection, overflow, keyboard controls, complete context and stale pace suppression.
- The new permanent `scripts/visual-density.sh` suite is sourced by the normal private Sway harness. At that check `tests/fixtures/screen-fit-usage.json` had all six providers ready, with source timestamps and four distinct optional Agy budgets; deterministic twelve-core CPU, RAM/swap, network, disk and current/weather/AQ fixtures brought the total to **12 cards**; the fixtures still hold the six-provider report, so this layout is unchanged. `show_agy_gpt_oss = true` proves the preference is not discarded to make the screen fit.
- Sway outer stripe **640×1440**, actual GTK client **636×1413**. Every one of the **21 themes** asserts vertical adjustment **upper = page = 1264 px**, twelve retained cards, visible metric/budget readings and allocated text without horizontal clipping. Original full content plan: **1181 px** plus 24 px stack padding, within the 1264 px viewport.
- Original allocated **outer** card heights (including padding/border), measured from real GTK bounds: CPU **212 px** across **612 px**, three core cells per row; OpenRouter/DeepSeek **83 px** each; Claude/Codex/z.ai **170 px** each; Agy with all four budgets **280 px**; memory **162 px**; network **124 px**; disk **88 px**; weather **272 px**; air **120 px**. Smaller cards have **302 px** outer width. Main values remain 16 px, weather temperature 26 px, supporting text ≥12 px; no global type reduction.
- Actual height response: at outer **640×1360**, client 636×1333, the CPU switches to four cells/row (**186 px**) and upper = page = **1184 px**. At outer 640×900, upper **1179** > page **724**: honest scroll fallback. At outer 378×900, client 374×873, full source-order single column has upper **2098** > page **724**. Sixty-four-core wide fallback: **1517 > 1264**. Lengthy stale/error wide fixture: **1445 > 1264**, narrow **2560 > 724**; errors and source timestamps remain visible, and stale quotas have no current pace marker/legend.
- Real keyboard controls toggle density and change themes with config-file byte comparison; no writes. Synthetic selected-theme local/report updates retain density. Hourly **Details** and CPU source **Info** each remain open, rooted and keyboard-focused across three two-second local and provider/report updates. Geometry is not rebuilt on unchanged sample/report ticks. Window/density/theme/topology changes may intentionally rebuild geometry.
- Missing history/forecast slots, null daily values/condition codes and extreme balance/network/temperature values also render without horizontal text clipping. No card or provider budget is clamped, hidden or removed to satisfy the scrollbar assertion.

### Directly inspected images

Opened PNGs through the image-read tool, not capture success alone:

- `screenshots/stripe-density-theme-{original,paper,terminal,blueprint,editorial,contrast,retro,cloud,minimal,nord,solar,rose,ocean,ember,orchid,forest,cobalt,sand,mint,noir,arcade}.png`: all 21 Screen-fit palettes/ink/surfaces remain recognizable; broad CPU with thin readable peaks, primary quota/balance readings, all four Agy budgets, current plus five-day weather, air/source timestamps are visible on one screen. No crowded header/sideways clipping or tiny-type workaround.
- `stripe-density-screen-fit-{original,paper,terminal}-comfortable.png`: existing curated Comfortable cards/CPU/weather retained; full fixture honestly scrolls (Original 1642 > 1251; Paper 1698 > 1251; Terminal 1576 > 1251).
- `stripe-density-fit-{details,info}-original-details.png`, `stripe-density-fit-theme-original-after-updates.png`: open keyboard-reachable source/hourly text and updated readings; theme/density survive.
- `stripe-density-fit-{missing,extreme,errors,high}-original-wide.png`, `stripe-density-short-{height,overflow}.png` and narrow/top/bottom captures: gaps and `—` retain missing values, long readings wrap, stale errors are explicit without current pace cues, high-core/short-height/narrow content remains readable and reachable by scrolling.

### Repaired failures / limits

An early planner test had the wrong stable-column expectation and was corrected. Initial visual Details test retained a launch-size widget before compositor allocation; the newly height-aware host replaced that unrooted tree, exposing a test-only popup crash. Test controls now resolve after initial allocation, and full visual gates pass. Image inspection also caught a synthetic stale fixture retaining current pace: rendering now defensively omits pace cues for any non-ready card. A source Info screenshot exposed a character-width text column; natural-width propagation and bounded wrapping now produce a readable wide/narrow panel. The narrow regression initially rejected a 330 px allocation, so source context's wrapping width was bounded to 38 characters; the final wide/narrow Info captures were directly inspected.

This is private compositor/synthetic evidence, not authenticated source verification, a real browser/desktop, a large-font/fractional-scaling accessibility audit or a performance benchmark. Long real errors, more cores/budgets or different font metrics can genuinely overflow; normal scrolling is deliberate. Reading updates preserve stable packing rather than reflowing every two seconds, so changed-height data can require scrolling until a geometry change. Independent re-review found no remaining issues. The parent independently passed formatting, all 75 Rust tests and shell checks, the full legacy/density visual suite and diff validation, then directly inspected regenerated ready and source-Info recovery images.

### Independent Screen-fit review repairs (P1 / P2)

- **P1, failure-first:** the new Rust graph-context regression failed with `Info omitted graph reading: RAM · 43%`; the real GTK geometry regression independently failed with `memory utilization missing from visible readings: 43%`. Screen-fit now places RAM/swap utilization percentages beside their readings (43% / 12% in the fixture), and every graph label plus its fixed/automatic Y scale appears in keyboard-reachable source Info. Network Info retains `Receive/Transmit · auto Y 0–256 KiB/s` and exact byte/s scale limits. Tooltip descriptions remain; Comfortable geometry is untouched. Memory still allocates **302×162 px**, and all 21 themes still fit twelve cards with **upper = page = 1264**.
- **P2, failure-first:** a real GTK regression opened weather Details and called the production report-failure path; it failed because the error transition replaced the open control. Error text is no longer a card-layout invalidation key. A separately reconciled, normally hidden banner handles first failure, changed failure and recovery without detaching card hosts or column parents. Affected provider Info headers also remain rooted while their stale/readiness presentation updates.
- Two permanent scenarios, `fit-report-details` and `fit-report-info`, exercise failure → long changed failure → recovery with weather Details or OpenRouter source Info open. Every transition asserts all twelve host identities, column-parent identity, popup visibility and keyboard focus; post-allocation checks also verify banner text/visibility and readable wrapping. The long banner honestly increases **upper to 1367 > page 1264**; recovery restores **upper = page = 1264**, without closing either popover or losing packing.
- Final `just format`, `PATH=/bin:$PATH just check-ci` (**75 Rust tests** and shell checks), full `PATH=/bin:$PATH just test-visual`, whitespace/index checks passed. Full legacy visuals, all 21 Screen-fit previews, short-height/narrow/high-core/long-error fallbacks, controls and successful/error-transition live popup tests ran. Final logs: `/tmp/screen-fit-review-check.log`, `/tmp/screen-fit-review-visual.log`; failure-first logs: `/tmp/fit-review-p1-red.log`, `/tmp/fit-review-p1-geometry-red.log`, `/tmp/fit-review-p2-red.log`.
- Direct image inspection after fixes covered ready Original/Paper/Terminal with visible memory percentages, expanded CPU graph Info, narrow fallback, and `stripe-density-fit-report-{details,info}-original-{failure,changed-failure,recovery}.png`. Error text remains visible, the long banner scrolls rather than clips content, and open readable popovers survive both failure and recovery. No scope expansion. Independent re-review approved both repairs; the parent's integration rerun passed all 75 tests, shell and visual checks, with direct inspection of regenerated ready and recovery images.

## Report-driven AI tiles

- **What changed:** the report is the inventory. `cards_from_json_at` builds one tile per reported agent, in report order, grouping a named account (`anthropic@work`) with its base agent and labelling it from `PROVIDER_LABELS` when this build names that agent, otherwise from the report's own display name. An agent the report omits has no tile; `enabled` narrows the reported set while still honoring an id the report omits, which renders as unavailable. `visible_cards`/`visible_cards_at` no longer distinguish sample from live, and config validation rejects only an empty or duplicated id.
- **Why:** the owner asked for tiles only for the agents ai-usagebar reports, with that tool's own enable list as the inventory and configuration. That includes Muse Code, whose Session and Weekly windows the installed ai-usagebar reports (`ai-usagebar --vendor muse`, 2026-10-13: `5h 2% · 7d 4%`, reset horizons, "Muse Code Power Usage"; upstream 1.35.0 documents the opposite, so both readings are recorded in the [source research](../research/ai-usage-sources.md)).
- **Tests:** `tiles_follow_the_report_as_the_inventory`, `tiles_show_only_the_reported_agents`, `muse_reads_its_quota_windows_and_falls_back_to_reported_rows`, `named_claude_account_is_not_silently_reported_missing`, `config_rejects_duplicate_and_empty_provider_ids` and `invalid_local_config_fails_before_opening_a_window` cover the model, and the named-account case keeps one tile per agent.
- **Fixtures stay at six providers**, so the calibrated Screen-fit layout is unchanged (thirteen cards, density suite still greps `cards=13`). A host that enables a seventh vendor gets a fourteenth tile: fourteen filled tiles fitted 640×1413 in a direct run at a tighter Screen-fit spacing, and a host also showing every optional Agy row may honestly scroll.
- **Not established:** no live `ai-usagebar usage --json` capture from a host with Muse enabled was committed; the tile shape follows the owner's `--vendor muse` output and the schema-v1 entry it implies, so the first live run should confirm the labels and windows land as expected.
