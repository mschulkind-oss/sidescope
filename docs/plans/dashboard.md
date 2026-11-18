---
title: "Dashboard expansion handoff"
date: 2026-10-05
status: in-review
stage: BUILT
next: "Independent review of code, tests and generated screenshots; then host verification"
tags: [plan, dashboard]
---

# Dashboard expansion implementation plan

Written against `42e6753`, 2026-10-05; implemented in the uncommitted working tree under the explicit user dashboard request. The [design](../design/sidebar.md) provides the existing native card vocabulary; user-requested expansion governs this slice. Behavior direction wins, the tree wins on fact, and the plan is advice.

Scope: retain native GTK responsive masonry and provider selection; fix balance projection; add bounded in-memory Linux CPU/core, RAM/swap, network and root filesystem cards; explicit-location Open-Meteo weather and modeled air quality. No email, public plugins, credential handling, geolocation or invented spend history.

1. Reproduce balance defects with regression tests before changing projection.
2. Add pure tested local sample/delta/history and weather/config parsers; finite bounded process/network work stays off GTK thread.
3. Share the card presentation model, including accessible graph descriptions; merge independently refreshed sources into both layouts. Mark retained failures stale and keep errors bounded.
4. Supply deterministic synthetic local/remote demo data, config example, source attribution and accurate limitations.
5. Run targeted tests, format, full check-ci and real headless compositor visual tests; inspect PNGs including scrolled dashboard content. Parent performs independent review/commit.

## Map and reuse

| Path | Seam |
| --- | --- |
| [report model](../../src/lib.rs) | Balance sections, Graph presentation, WeatherConfig validation |
| [GTK host](../../src/main.rs) | Reuse card_view/pack_columns for all cards; independent refresh state |
| [local telemetry](../../src/dashboard.rs) | New pure parsers/deltas, bounded histories, stale retention, synthetic demo |
| [remote weather](../../src/weather.rs) | New explicit-location forecast/air parsers and source attribution |
| [process boundary](../../src/process.rs) | New bounded inherited-environment subprocess execution |
| [visual harness](../../scripts/visual-test.sh) | Existing private Sway plus private bus/schema setup and scrolled dashboard captures |

## Traps and ships-with

- Constraint: DeepSeek `type=text` sections carry balance; percentage-headline variants put remaining money in detail. OpenRouter comparison prose describes *used* credit, not remaining balance.
- Constraint: no dated daily-history report contract; never synthesize spend from balance movements.
- Unit cases: negative/text/percentage balances; CPU guest/iowait/reset, memory/swap, network elapsed/hotplug, history bounds, disk overflow; malformed/null/unit/time forecasts; process timeout/output limit; independent stale remote retention.
- Integration: existing CLI tests and visual harness with deterministic dashboard fixtures at both widths.
- Docs/config: [README](../../README.md), [example config](../../config.example.toml), research/tasks/QA plus corrected historical prototype scope.
- Do not edit jail configuration or handle credentials. No email, plugins, geolocation or unsupported live-source claims.

## Compact redesign follow-up

Executed against `c1dcc79`, still uncommitted and awaiting independent parent review. The user's dense weather/system redesign and subsequent df discrepancy govern this slice; no source/provider/config expansion.

1. Extract the existing X calculation into shared `src/plot.rs`; reproduce one/two/warm-up stretching regressions, then replace with 120 fixed right-aligned two-second slots. Keep percentage Y domains fixed and label network automatic Y scaling separately.
2. Evolve graph samples to optional slots. Failed reads, reset/new cores and recovery must not fabricate zeros or connect across missing slots; retain bounded stale successes and timestamps. Numeric core order, total first, two-column mini traces with inline label/value and 24 px rows.
3. Replace weather's 18 generic metrics with a bounded typed presentation: current glyph/temp, twelve-step curve with accessible text Details, five daily range rows. Null data breaks curves/ranges; Celsius/Fahrenheit and source/model/check/stale/unconfigured states stay honest.
4. Reduce shared frame/card typography/padding/gaps (12 px supporting text floor); shorten repetitive prose while keeping source timestamps and full tooltip context. No theme/art dependencies.
5. Add the approved disk correction test first: 100 total / 30 free / 20 available yields 70 used, 20 available, 10 reserved. Use f_bavail for Available, never total-minus-available for Used; check invalid counters/overflow.
6. Extend isolated Sway QA with density budgets and synthetic graph/weather/high-core states. Read generated PNGs directly, run format/check-ci/visual gates and document before/after measurements. No personal capture/config, staging or commit.

Historical expansion QA above is superseded on compact layout/free-space semantics by the follow-up [QA](../qa/dashboard.md#compact-redesign-follow-up).

## Themes follow-up — 2026-10-05

- Keep Original as choice 1 of 21. Return its existing CSS verbatim (plus confined footer selector chrome) and retain exact Cairo color/width/grid/dot/range values. No default spacing, font, palette or chart redesign.
- Define curated art directions in a closed registry: header/footer/focus; card/featured/title/body/muted/status; provider/local/remote accents; six independent plot inks; graph background/grid/dash/width/points/fill; glyph/temp/range track/endpoints; border/radius/shadow/type/compact padding. Alternative fills remain faint and break at missing slots.
- Browse without restart using footer previous/next, visible name/index, `[ ]`, a selector-only wheel controller and a bounded scrollable picker. Theme selection is session state outside telemetry data. Native picker navigation owns its keys while open; ordinary dashboard scrolling remains unchanged.
- Optional `theme = "id"` is validated before GTK starts. Only predefined ids are accepted, never CSS. No automatic preference writes and no credential/data-source changes.
- Prove failure-first Clear geometry, Original pixel preservation with explicit condition/footer exclusions, registry coverage, runtime cycle, selector callbacks/picker, synthetic local/report update retention, and direct inspection of every theme at 640/378 px.
