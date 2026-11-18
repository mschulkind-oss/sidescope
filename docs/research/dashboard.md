---
title: "Dashboard data sources"
date: 2026-10-05
status: in-review
tags: [research, dashboard]
summary: "Official forecast/air terms and inspected local source contracts; no authenticated verification."
---

# Dashboard source research — 2026-10-05

Official documentation retrieved with `curl` (no authenticated requests):
- https://open-meteo.com/en/docs — current temperature/weather code, hourly temperature/weather code, daily min/max/weather code; explicit coordinates, temperature units, `timezone=auto`, five forecast days.
- https://open-meteo.com/en/docs/air-quality-api — current US AQI and PM2.5, modeled CAMS European/global forecasts, not a nearby sensor observation. Attribution to Open-Meteo and CAMS ENSEMBLE required.
- https://open-meteo.com/en/terms — free service is non-commercial, under 10,000 requests/day, 5,000/hour, 600/minute; CC BY 4.0 attribution, no warranty. Two requests every 30 minutes is below these limits; commercial use requires a separate agreement and is not supported by this adapter.

The ai-usagebar report and panels at pinned f78e266 were re-read from public source. `sections` is a lossless tagged list; DeepSeek's balance is normally `type=text`, not a metric. OpenRouter's `Credit balance` metric has a monetary value even with a percentage headline. DeepSeek may have a configured display denominator; any percentage/comparison must be discarded, preserving only the explicitly reported money amount. Neither balance changes nor top-ups establish spend. OpenRouter has a *current* today/week/month prose block, not dated historical daily observations. No trustworthy dated daily-history contract is present, so the UI must say history is unavailable, not generate one.

PurpleAir requires a key. This implementation chooses the approved practical alternative, Open-Meteo/CAMS, visibly labeled modeled. It does not implement or request PurpleAir credentials. No geolocation, IP location lookup, credential copying or authenticated source verification.

Linux local sources: `/proc/stat` cumulative logical CPU ticks (guest already included in user/nice); `/proc/meminfo` MemAvailable-based RAM plus separate swap; `/proc/net/dev` non-loopback interface counters (virtual interfaces may double-count); `statvfs` for `/` (root filesystem, not all physical drives). Used follows `f_blocks − f_bfree`; **Available** follows `f_bavail`, excluding normal-user reserved space as `df` does. Sampling uses monotonic elapsed time and handles resets/hotplug without spikes.

Relevant research, implementation-plan, system-doc and vantage-docs skills were read; Vantage 0.8.1 style guide/checks were used. No jail configuration changes required.

## Compact visual mechanisms — 2026-10-05 follow-up

Read the existing design/plan/source QA first. Parent research established these public references; this pass fetched their public pages with curl and directly opened the Weathergraph widget image:

- [Weathergraph](https://weathergraph.app/) and [FAQ](https://weathergraph.app/faq): aligned hour-by-hour forecasts in one temperature curve, with compact condition/temperature above the graph. The public [widget image](https://weathergraph.app/_next/static/media/weathergraph-widgets-iphone.d2353b45.png) was downloaded to `/tmp/weathergraph-widgets.png` and visually inspected. Its hourly/daily strips convey trends without a large repeated metric block for every forecast step.
- [GNOME Weather](https://apps.gnome.org/en/Weather/): separate hourly and daily outlooks, rather than one long undifferentiated list.
- [KWeather](https://apps.kde.org/kweather/): flat condition/temperature forecast presentation.

Adopt mechanisms, not branded art: local Cairo line glyphs; current temperature + conditions; a bounded twelve-step temperature curve; five dense icon/low/high rows with a shared range scale. No downloaded art/dependency is included in the app. The current adapter has no precipitation/wind series, so none is drawn or inferred. Hourly text remains available through Details; timestamps, units, sources and stale states remain visible. Source pages/images are public, not personal weather captures. Relevant skill files were not available in this worker's filesystem/tool set; no new external skill tooling was installed.

## Fast-moving — verify before distribution

Recheck the linked API documentation/terms, pinned ai-usagebar report schema and source permissions before distribution. Documentation/source inspection is not a live reliability claim. Commercial Open-Meteo use requires a separate agreement and adapter changes. PurpleAir is not implemented because the approved modeled alternative needs no secret.

## Theme mechanisms — 2026-10-05

This change uses local implementation research, not a new external inspiration survey. GTK's `Label` WordChar wrapping was the cause of the supplied Clear/61°F split: a short horizontal condition label could request too little width. A nonwrapping, expanding label with end ellipsis avoids splitting ordinary words without imposing a large card minimum. Its full text remains in the label/tooltip and the keyboard-reachable Details popover.

GTK CSS cannot recolor Cairo paths. A closed Rust registry therefore supplies independent component colors and plot styles to both renderers; runtime changes reload the CSS provider and invalidate drawing areas without rebuilding cards. Generic Sans/Serif/Monospace families avoid mandatory font downloads. The current Nix test shell exposes only DejaVu Sans, so serif/mono requests fall back there; host font-family appearance remains a separate verification item.
