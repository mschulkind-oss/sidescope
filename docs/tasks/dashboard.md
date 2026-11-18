---
title: "Dashboard expansion tasks"
date: 2026-10-05
status: in-review
stage: BUILT
next: "Independent acceptance review, then owner-configured host checks"
tags: [tasks, dashboard]
---

# Dashboard tasks

- [x] Reproduce missing DeepSeek section balance and OpenRouter comparison bugs before fixes.
- [x] Normalize balance-only cards including negative and percentage-headline money; do not invent daily spend.
- [x] Add CPU total/all logical cores, RAM/swap, receive/transmit and root filesystem cards with bounded session histories.
- [x] Add validated explicit location/units, unconfigured states, Open-Meteo current/hourly/five-day forecast and clearly modeled CAMS air quality.
- [x] Keep process sampling/network work off GTK; finite remote/report time/output budgets, one remote request worker at a time, bounded sample queue and stale-success retention.
- [x] Reuse native masonry and narrow card host, declare BreakpointBin minimum size, exercise private Sway scrolling and warning checks.
- [x] Add synthetic deterministic fixtures, README and non-secret [example config](../../config.example.toml), research/plan/QA artifacts and remove obsolete AI-only claims.
- [x] Format and full quality gate; directly inspect wide/narrow and scrolled PNGs.
- [ ] Parent independent acceptance review and integration commit (nothing staged/committed by worker).
- [ ] Owner enters chosen weather coordinates/units and checks `just dev` on the target desktop; no authenticated or user-location remote verification claimed.

Evidence and remaining limits: [dashboard QA](../qa/dashboard.md). Scope/direction: [implementation plan](../plans/dashboard.md).

## Compact redesign follow-up

- [x] Inspect public weather-widget image and cite reusable density mechanisms, not brand art.
- [x] Fail first on one/two/history growth X mapping; fix stable 120-slot right alignment and rolling geometry in shared tested plot module.
- [x] Add missing-slot history/forecast gaps, fixed percentage Y scale, explicit network auto-Y label and numeric core ordering/reset tests.
- [x] Render total CPU then dense two-column 24 px core rows; verify 64 cores at 640/378 px.
- [x] Replace tall weather metrics with typed current/12-hour curve/five-day ranges, native Cairo glyphs and keyboard-reachable hourly Details; cover units/nulls/unknown codes/stale/unconfigured states.
- [x] Tighten common cards/frame, keep visible trust/source timestamps and full prose tooltips.
- [x] Fail first on df Available mismatch; use f_bavail while preserving f_bfree-based Used and checked arithmetic/reserved semantics.
- [x] Extend isolated visual harness with measurement budgets and nine deterministic states; inspect before/after PNGs directly; run format/full gate/visual checks.
- [x] Update README/research/plan/QA; ignore generated screenshots/logs; leave all changes unstaged/uncommitted.
- [ ] Parent independent acceptance review and commit; inspect hourly popover and final source/model/check states on target host.
- [ ] Owner host large-font/scaling/accessibility/performance and real configured forecast checks.

## Themes follow-up — 2026-10-05

- [x] Fail the Clear/61°F regression before repair at representative 288 px masonry width.
- [x] Nonwrapping short conditions; long-condition ellipsis, tooltip and Details text.
- [x] Original + 20 named, closed-registry themes with component and Cairo style tokens.
- [x] Footer previous/next, index/name, `[ ]`, local wheel and scrollable picker; native popup key ownership.
- [x] Validate optional config id; no implicit writes; preserve theme through local and report renders.
- [x] Token safety/distinctness, original constants, config/cycle/series tests and compositor interaction assertions.
- [x] Full gates; all 21 wide/narrow screenshots opened; exact default pixel comparison.
- [ ] Independent acceptance review; host fonts/scaling/accessibility/real-source soak.
