---
title: "A workspace dashboard built as a native Wayland application"
date: 2026-09-26
status: in-review
tags: [design, wayland]
summary: "The intended product, architecture and behavior of Sidescope before implementation."
vantage:
  status-chip: true
---

# A workspace dashboard built as a native Wayland application

**Status:** DESIGN, 2026-09-26. A [dashboard prototype](../qa/dashboard.md) now includes provider, system, weather and modeled air cards; this broader full product design is not yet accepted.

> **In short.** Build one purpose-made, visually polished application for the spare column beside a tiled browser. Design its card system, data boundaries and full experience now; implementation can proceed in slices without replacing an interim product.

**Why it matters.** A compact AI-usage indicator belongs in the top bar; details, weather, mail and other personally useful information need room and should appear only on the chosen workspace.

**The shape.** A native window owns layout, cards and interaction; independent data adapters supply typed card states without controlling the window.

**Cost.** Owning a native application, data integrations and polish costs more than scripting an existing panel. In return the layout, error handling and interaction model are ours rather than constrained by a bar framework.

**Start at [Experience](#experience)** for the intended product, then [Architecture](#architecture) for how to build it.

**Needs your ruling:** [OQ-SS2](#OQ-SS2), [OQ-SS3](#OQ-SS3).

**Reads with:** [landscape research](../research/wayland-sidebar.md) (evidence and risks), [widget ideas](../ideas/widgets.md) (candidate cards), [roadmap](../../roadmap.md) (design work and handoff).

---

## Goal and scope

Sidescope is a **custom desktop application**, not an Eww configuration or a skin for Waybar. The target is **one quarter of a 16:9 display**: 640 px beside a 1920 px browser on the owner's 2560×1440 Sway desktop. A narrower 378 px window is only a stress case, not the layout target. The app draws attractive, legible cards, including imagery when it adds information, and makes it easy to add personally useful cards later. It should work as an ordinary Wayland application wherever the compositor supports arranging windows; Sway is the first environment to verify, not a runtime API requirement.

A complete product needs: adjustable card order and visibility; a readable narrow layout; refresh, timestamp, error and privacy behavior shared across cards; detailed AI usage, weather and an opt-in email summary when viable sources are found; and an extension path for future custom sources. **Designing these together does not mean shipping a fake first product or claiming all providers have been verified.** The final choice of supported providers follows source investigation, not guesswork.

### Principles

- **P1. Detail where it fits.** Show the primary value at a glance, then supporting context within the available width; no sideways scrolling or tiny top-bar-sized text.
- **P2. One coherent surface.** Card margins, typography, imagery, information density, interaction and failure states follow the same visual system. Cards can vary in content without looking like unrelated mini-apps.
- **P3. Honest data.** Source, last-success time and freshness state are visible when relevant. Estimated quotas are labeled as estimates; a failed refresh cannot turn cached information into a current value.
- **P4. Quiet by default.** Independent refresh schedules, no animation or polling without purpose, and no embedded browser engine unless a measured requirement justifies one.
- **P5. Private by default.** No credentials in config, logs or card output. Sensitive previews are opt-in; a glance at the desktop should not disclose message content.
- **P6. Portable core.** Rendering, configuration and data retrieval do not depend on Sway IPC or an edge-panel protocol.

## Experience

The app launches into a usable column on the chosen workspace. The compositor decides its position and width, as it does for other tiled windows; on Sway the user tiles it to the left of the browser. The app never resizes or rearranges other windows. A Sway recipe may make launching into that workspace convenient, but the application functions without that recipe. Multiple outputs, restarting on the same workspace, fullscreen behavior and narrow-width limits require a target-desktop validation pass, not a portability promise.

The default [Screen-fit density](../plans/screen-fit.md) spans CPU, efficiently packs smaller cards and keeps supporting context in accessible details, without reducing all type or hiding real overflow; provider rows keep their reset window visible in the legend under the bar. The alternative Comfortable density uses masonry—independent vertical columns where the next card goes to the shorter column—at the 640 px target width, and a single column in source order when the window narrows; the column scrolls if cards exceed its height. Density is independent of the curated theme browser. Every local history graph names its visible span in the card header (120 two-second slots, **4 min**), with the slot arithmetic in **Info** and the plot tooltip; the weather curve keeps its explicit **12 hours** label. Their order and visibility are user-configurable and persisted locally. With zero cards enabled, show an intentional empty state and a way to configure cards. The design should provide a stable top-to-bottom reading order, readable contrast, keyboard navigation, meaningful labels for images, focus treatment and reduced-motion behavior. The prototype uses a bright header/footer frame when the compositor focuses the stripe; losing focus returns to a quieter frame without obscuring the cards. It does not add a textual focus-status badge. The 640 px primary width anchors spacing, typography and imagery; a 378 px stress capture checks graceful narrowing. Palette, illustration style and interaction details still need full-column review; a bare functional list is not a design-complete interface.

A quota card with both usage and elapsed-window percentages shows a marked time position on its usage bar and a labeled point difference. Over pace means a larger share of quota used than time elapsed; under pace means the reverse. The labels convey direction even without color. The time position describes the reset window and the clock, so a card whose report states the window keeps the mark even when its reading is stale; the point difference compares that reading with the window, so a balance card, a missing time fraction or a stale reading cannot acquire a pace verdict. If a provider supplies both an absolute reset timestamp and an exact window length, the fraction can be calculated at report-check time; it must be omitted when either field is absent or contradictory, and a stale reading's own `N% elapsed` prose goes out of date with the reading rather than being recomputed on its behalf. For Antigravity, Gemini and Claude/GPT OSS are separate budgets; hide the latter pool by default and expose both of its windows through a non-secret local preference. Use the same `5-hour` and `Weekly` terms across quota providers, retaining pool names where two budgets would otherwise collide.

Each card has a primary value or message, supporting detail appropriate to its data, source attribution where useful, and a last-updated indicator for live data. Loading keeps the card's footprint stable. Offline, authentication failure, permission denial, rate limit, source parse failure and no-data states must be distinguishable where the adapter can identify them. The last successful result can remain visible with a **stale** label; an error must not silently overwrite it. If no prior success exists, show an explanatory empty/error view. Manual refresh is available without turning repeated clicks into parallel requests.

The first content families are defined now rather than added as architectural afterthoughts:

| Card | Intended information | Boundary |
| --- | --- | --- |
| AI usage | One tile per agent the local ai-usagebar report enables, in report order and under the agent's own name, showing provider/account label, balance or quota, reset horizon when available, timestamp and source confidence | Follow the [seven-source investigation](../research/ai-usage-sources.md); never invent a percentage from a monetary balance or incomplete data. Muse Code reports the same Session and Weekly windows as the other plans; a report carrying only custom-provider rows reads those rather than inventing a window |
| Weather | Entered location, current conditions, useful near-term outlook and appropriate imagery | No automatic location tracking; respect provider terms and units |
| Email | Opt-in unread summary and account label, with explicit auth/refresh state | No message body, subject or sender in the default view; credential store and least-privilege access required |
| Local/custom | Locally defined status or image cards that use the same card state and layout vocabulary | No arbitrary untrusted code in the UI process by default |

An existing Waybar quota module stays independent; the application does not take ownership of the top bar. The [ideas document](../ideas/widgets.md) ranks implementation effort, not whether these content families deserve a place in the design.

## Architecture

**Proposed stack:** Rust application with GTK4 and libadwaita for layout, text, image display, styling and accessibility. This is a design choice to validate against current bindings and the target desktop, **not a claim of measured memory superiority**. GTK CSS is not web CSS. A single process with asynchronous data work and the toolkit event loop is the starting shape; do not add an always-running helper service or a web runtime without an observed need.

| Component | Owns | Does not own |
| --- | --- | --- |
| Window and card host | Responsive column, card order, visibility, focus, shared state presentation | Provider authentication or quota parsing |
| Card view | Accessible rendering of a typed result, including timestamp and error treatment | Network requests or credential persistence |
| Data adapter | One source's settings validation, retrieval, normalization and error classification | GTK objects, window geometry or global card order |
| Refresh coordinator | Per-card scheduling, one in-flight request per card, manual-refresh coalescing, cancellation on shutdown | Inventing provider limits or silently retrying authentication failures |
| Settings and secret storage | Non-secret local choices in settings; credentials in a platform credential store where needed | Publishing credentials in logs or exportable UI state |

A normalized card result contains the data needed by its view, source identity, collection time, and a freshness/error state. The adapter is the sole writer of its fetched data; the coordinator decides when to invoke it; the host owns ordering and enabled state. An adapter failure changes that card only. A provider-specific rate limit overrides any generic refresh preference. Until provider terms are known, production refresh intervals and staleness windows are deliberately **not specified as universal constants**; each supported adapter must define bounded defaults, units, backoff, and how cached data is invalidated before it can ship. Preferences must not permit hammering a service below its allowed interval. The prototype temporarily polls its single `ai-usagebar` report every five minutes and relies on that tool's caching and backoff; this is not an adapter-level refresh design or evidence that the interval is safe for every vendor.

Start with built-in adapters behind an internal interface. **Future custom widgets are a product requirement**, but a stable third-party plugin ABI, arbitrary code execution in the UI process and a scripting language are not yet chosen. Design a documented, versioned data input contract before publishing an extension mechanism; evaluate a local command/JSON adapter with explicit permissions and timeouts against a compiled plugin only after the built-in adapters establish what extensions actually need. This keeps the visual/card contract stable without pretending an untested plugin API is ready. The first command/JSON mechanism now exists for charts only, with explicit permissions, timeouts and no plugin ABI: see [command-driven charts](command-charts.md); other card types still use built-in adapters.

The ordinary tiled-window mode is the **intended product**, not a placeholder for a panel. [Layer shell](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) could request reserved edge space but would create a different type of surface and vary by compositor. It is not on the build path unless the user explicitly chooses that interaction model in [OQ-SS2](#OQ-SS2).

## Alternatives and trade-offs

| Alternative | Disposition | Reason |
| --- | --- | --- |
| [Eww](https://elkowar.github.io/eww/configuration.html) or [AGS](https://aylur.github.io/ags/guide/quick-start.html) | **Useful references, not the product** | They can prototype a sidebar, but the requested project is an owned application with a coherent visual and data model |
| Another native toolkit such as [Slint](https://github.com/slint-ui/slint) | **Fallback if GTK fails a concrete requirement** | Test bindings, accessibility, rendering and memory; do not rewrite the product design merely to change toolkit |
| Browser/Electron | **Reject as a starting stack** | A permanently visible information column does not require an embedded browser; benchmark before stating numeric savings |
| Layer-shell-only panel | **Reject as the default** | Edge reservation is compositor-dependent and loses ordinary tiling behavior |
| Public plugin ABI on day one | **Defer the ABI, not extensibility** | A stable contract without real integrations would freeze guesses about security and data shapes |

## Risks and evidence gates

| Risk | Design response / proof needed |
| --- | --- |
| No reliable AI quota API | Investigate the existing indicator's data provenance; support only a source whose accuracy, permission and freshness can be explained |
| Account access leaks | Keep credentials out of settings and fixtures; threat-review storage, logs and screenshots before enabling email |
| Native toolkit is heavier than expected | Measure startup, idle and refresh CPU and process-tree proportional set size on the same host; publish methodology rather than asserting a memory target |
| Narrow column becomes cluttered | Review full-height mockups with all three content families, long labels, large type, empty/error states and overflow |
| "Generic Wayland" is overstated | Test a normal window on Sway first and another compositor before naming it supported; keep compositor-specific placement instructions separate |
| Future custom cards break UI consistency | Keep data source and card presentation separate; decide a safe extension contract against real built-in adapters |

## What is deliberately unresolved

The six AI platforms are now named; their source reliability, licensing of remote weather/email data, per-provider refresh defaults, visual dimensions and public extension format require investigation and design review. They are not silently delegated to an implementer: the [roadmap](../../roadmap.md) routes that work before an implementation handoff. The full-product implementation plan will be prepared **after** the design is accepted against the resulting repo tree; the [prototype sketch](../plans/prototype.md) documents only the experimental build. Work can then be sequenced, but every slice should build toward this application rather than a different interim tool.

## Open Questions

1. 💬 **OQ-SS2: Is ordinary tiling the desired placement model?** A tiled window fits the stated browser workspace. An automatically reserved edge panel changes workspace/fullscreen behavior and cannot be promised on every compositor. This rules whether edge-panel support belongs in the product design.

   <!-- vantage: oq id=OQ-SS2 leaning="Use one ordinary tiled Wayland window; leave layer-shell edge reservation out of this product unless there is a concrete workflow it cannot serve." -->

   _Leaning:_ Use one ordinary tiled Wayland window; leave layer-shell edge reservation out of this product unless there is a concrete workflow it cannot serve.

   **Answer:**
   > _(empty — fill in when decided)_

2. 💬 **OQ-SS3: How much email should be visible at a glance?** Account name and unread count are low-disclosure defaults; previews make the column more actionable but materially change privacy and layout. This decides the mail card's visual/privacy contract, not whether to investigate mail integrations.

   <!-- vantage: oq id=OQ-SS3 leaning="Show account label and unread count by default; omit sender, subject and body from the always-visible column." -->

   _Leaning:_ Show account label and unread count by default; omit sender, subject and body from the always-visible column.

   **Answer:**
   > _(empty — fill in when decided)_
