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
| `just test-visual` (private headless Sway: 21 themes, density, all existing suites, then `scripts/visual-charts.sh`) | **exit 0** on the third full run (logs in `.yolo/durable/charts-qa/`) |
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
