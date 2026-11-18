---
title: "Six-provider visual prototype and headless desktop validation"
date: 2026-09-26
status: in-review
tags: [plan, prototype]
summary: "Implementation handoff and limits of the first real Wayland build."
vantage:
  status-chip: true
---

# Six-provider visual prototype

**Status:** SKETCH, 2026-09-26. This plan is for a prototype, not an accepted product specification.

## Build order and boundaries

1. Establish a Rust/GTK4 app with one ordinary Wayland window and a reproducible Nix shell; do not assume host Sway or native libraries are present in the jail.
2. Parse only the [versioned AI usage report](../research/ai-usage-sources.md). Render six provider slots with synthetic sample data; live mode automatically hides unreported providers unless local config explicitly selects them. Preserve stale/error states; balance values do not become percentage bars.
3. Offer explicit live subprocess invocation so a contributor with `ai-usagebar` installed can exercise their own accounts without embedding credentials. A provisional five-minute whole-report refresh and manual keyboard refresh now exist; a production per-provider policy remains unverified. The [dashboard expansion](dashboard.md) adds a bounded report subprocess timeout.
4. Launch a private headless Sway compositor at 2560×1440, tile a browser *placeholder* at roughly 1920 px beside the real 640 px GTK app, assert the stripe width from Sway's window tree, capture with `grim`, and inspect both the full output and cropped stripe directly. Repeat at 1280×900/378 px only as a narrow stress test. This is not proof of behavior against a real browser on the user's desktop.
5. Replace shared-height rows with two independently measured card columns at the target width; use a responsive breakpoint to display the same cards in original order at narrow widths. Place each new wide card in the shorter column, with stable tie-breaking. When a quota report includes an explicit elapsed percentage in its detail, mark that time position on the used-percentage bar, color and label the signed point difference, and omit the current pace cue for stale results, balances or unrecognized prose.
6. Extend pace projection to reports with both `reset_at` and `window_secs`, using one injected check time in tests. Normalize exact five-hour and seven-day window labels; keep Antigravity pool identity, show its Gemini windows by default, and include both Claude/GPT OSS windows only with `show_agy_gpt_oss = true`. Remove the generic values disclaimer rather than obscuring the source-specific failure and stale labels.
7. Make the screenshot a documented QA artifact, not a committed account capture. Cover over/under/even/missing pace in a credential-free fixture, inspect both target and narrow screenshots, and follow up with a manual session test on the user's Sway environment.

## Repository map

| Path | Purpose |
| --- | --- |
| [`src/lib.rs`](../../src/lib.rs) | Versioned report parsing, non-secret TOML provider selection and typed presentation state; unit tested |
| [`src/main.rs`](../../src/main.rs) | GTK window, responsive card layout, shortcut help and local subprocess refresh |
| [`tests/fixtures/usage.json`](../../tests/fixtures/usage.json) | Credential-free deterministic data |
| [`scripts/visual-test.sh`](../../scripts/visual-test.sh) | Isolated real Sway launch, window assertion and compositor capture |
| [`flake.nix`](../../flake.nix) | Reproducible jail-build and desktop-test dependencies |
| [`Justfile`](../../Justfile), [`scripts/native-env.sh`](../../scripts/native-env.sh) and [`scripts/doctor.sh`](../../scripts/doctor.sh) | Simple build/run commands, separate host/jail Cargo targets and system dependency diagnostics |
| [`yolo-jail.toml`](../../yolo-jail.toml) | Generic dependencies on next jail rebuild; Nix shell works in this session |

## Prototype limits

The earlier [product design](../design/sidebar.md) still governs intent. This historical slice was **AI only**; the [dashboard expansion](dashboard.md) now includes Linux telemetry, explicit-location weather and modeled air quality. Email, drag-and-drop ordering, accessibility audit and a safe public custom-widget protocol remain unimplemented. The optional local config orders enabled providers but needs a restart. Screenshot inspection includes a real compositor `?`/`Esc` shortcut check; it is not a keyboard accessibility audit or a memory benchmark. Unverified provider APIs must not be described as officially supported.
