---
title: "Six-provider prototype QA"
date: 2026-09-26
status: in-review
tags: [qa, wayland]
summary: "Observed headless Sway rendering, provider selection and shortcut behavior; remaining desktop checks."
vantage:
  status-chip: true
---

# Prototype QA

**Status:** Historical synthetic prototype QA. Current expansion observations are in [dashboard QA](dashboard.md).

## Observed in this session

- `just check-ci` passed after the UI changes: formatting, Clippy with warnings denied, 18 report/config/pace/layout unit tests, 6 keyboard/refresh/scroll/pacing-label unit tests, 2 command-line integration tests and native-environment shell tests. The config tests cover automatic visibility, explicit selection and order, missing providers, malformed settings and named accounts. A missing `headline` on an older percentage report gets a bar without turning monetary balances into percentages.
- The [visual harness](../../scripts/visual-test.sh) started an isolated real Sway compositor, placed a browser placeholder beside the GTK application, and measured a **640 px stripe at 2560×1440**. The ignored captures include a full output, 640×1440 crop, `?` shortcut guide and 378 px narrow layout at 1280×900. The harness uses `wtype` to send `?` and `Esc` as real compositor keys; checking a center pixel now fails the test if the overlay did not appear or did not close. It also sends `J` in the narrow layout and checks that the visible column changes. Initial virtual-keyboard events were occasionally lost, so the harness sends harmless lead-in keys before each tested shortcut.
- I opened the final full-width, cropped, overlay and narrow captures, the scrolled narrow capture and a separate cropped run with `enabled = ['openai']`. At the target width, six sample cards form two independent equal-width columns: shorter DeepSeek is followed immediately by Claude in the right column, rather than leaving a row-height gap. Orange `+13 pts OVER PACE`, green `8 pts UNDER PACE` and blue `ON PACE` labels accompany used-percentage bars with white elapsed-time ticks. Balances have no bars and the stale z.ai card has no current pace cue. A one-card Codex selection fills the column. At 378 px the original provider order returns in one scrollable column and the footer stays visible. Sample cards say `SAMPLE`, not `LIVE`.
- Synthetic Antigravity captures used the committed [Agy fixture](../../tests/fixtures/agy-usage.json) with reset timestamps generated relative to capture time (no account data). I inspected `screenshots/agy-default-column.png` and `screenshots/agy-optin-column.png`: Gemini's 5-hour and weekly rows show elapsed-time ticks and point differences without upstream prose, while neither Claude & GPT OSS row appears by default. With `show_agy_gpt_oss = true`, both appear (four rows total), and longer names wrap cleanly at 640 px. The generic "Values are shown as reported…" footer note is absent. At 378 px, I inspected `screenshots/agy-optin-agy-bottom.png`; all four rows and the persistent status footer are readable after keyboard scrolling. The harness asserts that the Agy card is reachable by scrolling but cannot automatically assert rendered text, which was checked by eye.
- A separate `SIDESCOPE_VISUAL_LIVE_STUB=1` harness run supplied a credential-free, one-second-delayed report command. Its inspected Sway captures showed **CHECKING REPORT…** during `R`, then **REPORT CHECKED HH:MM:SS** after completion. That timestamp refers to the local report command, not the time any provider was fetched; it cannot establish cache freshness. The shortcut guide no longer offers `Q` to quit.
- The owner's earlier screenshot showed only Codex with a report while five unconfigured placeholders occupied the column; live mode now omits providers absent from the report by default. That screenshot is useful feedback about the previous build, **not** proof that this revision runs on the host.

## Window-focus check (2026-09-27)

`just test-visual` now focuses the stripe through headless Sway, blurs it by focusing the browser placeholder, and compares header and footer pixels at both 640 px and 378 px. I inspected the generated ignored `screenshots/stripe-wide-{focused,unfocused}.png` and `screenshots/stripe-narrow-focus-{focused,unfocused}.png`: a green header/footer frame becomes quiet gray after blur. The explicit focus-status badge was removed; color is the only focus cue. The cards remain readable at both widths. This tests compositor activation in the private harness, not every host compositor or theme.

## Not established

The jail has not exercised an authenticated live report or a five-minute poll; the timer and in-flight guard have unit tests, but the polling interval is provisional and provider-specific safe intervals remain unverified. The pacing projection uses upstream's human-readable `N% elapsed` detail when present, otherwise a valid `reset_at` plus `window_secs` to estimate elapsed time at report-check time. An inconsistent explicit detail or a missing, invalid, past or implausibly distant reset loses the cue; a correctly timestamped cached report could still be old without a separate trustworthy freshness field. The rendered omission of the generic disclaimer was manually checked in synthetic captures, not automatically asserted as visible text. An actual host-native build and keyboard test **of this revision**, source freshness on the owner's accounts, another compositor, keyboard accessibility, resource benchmarks and a real browser session remain unverified. The harness's browser is a geometry placeholder. Fixture percentages and balances are invented. Screenshots are ignored local QA artifacts, not assets for publication; never commit authenticated captures.

The headless shell sometimes logs GLib session-bus/machine-id warnings while the UI and screenshot tests still work. A single proportional-set-size (PSS) reading is not a performance benchmark.

## Next verification

On the host, run `just build`, then `just demo` and `just dev` if `ai-usagebar` is installed. Check the 640 px masonry layout, `?`/`Esc`/`R`/`J`/`K`, live provider visibility, over/under pace color and ticks, long labels, the Agy default/opt-in preference and refresh failures on the owner's Sway workspace. Re-run `just check-ci` and `just test-visual` after later edits, and inspect the ignored full, crop, overlay and narrow captures directly.
