---
title: "Wayland sidebar landscape"
date: 2026-09-26
status: in-review
tags: [research, wayland, ui]
summary: "Comparable projects, compositor constraints, and lightweight UI candidates."
vantage:
  status-chip: true
---

# A sidebar beside a tiled browser

**Status:** CURRENT. Sources reviewed 2026-09-26; no local compositor tests or resource measurements performed.

## What already exists

A *vertical panel* organizes modules along a screen edge; a *sidebar* here needs room for stacked cards with readable detail. These are neighboring products, not synonyms.

| Tool | Reusable mechanism | Limitation for this use | Disposition |
| --- | --- | --- | --- |
| [Eww](https://elkowar.github.io/eww/configuration.html) | Custom windows specified in Yuck with GTK styling; Wayland `exclusive` can reserve an edge | GTK3 and gtk-layer-shell dependencies; custom behavior becomes configuration/scripts | **Reference**, not the product: study its layout and edge behavior |
| [AGS/Astal](https://aylur.github.io/ags/guide/quick-start.html) | GTK windows composed with JavaScript/JSX and system-integration libraries | GJS/GTK runtime and tooling; still application code to maintain | **Reference** for custom layout and system integration |
| [Waybar](https://github.com/Alexays/Waybar/wiki/Configuration) | Existing vertical bar with script-backed modules and CSS | Better at compact status than detailed cards; already present in the user's top bar | **Keep for top bar**, not the proposed detailed view |
| [Ironbar](https://github.com/JakeStanger/ironbar/wiki/configuration-guide) | Vertical GTK4 bar, custom widgets and popups | Layout remains bar-oriented | **Alternative** if cards turn out to be compact modules |
| [nwg-panel](https://github.com/nwg-piotr/nwg-panel) | Configurable GTK panel and executor widgets | Bespoke card layout is not its core abstraction | **Deprioritize** |
| [Plasma widgets](https://develop.kde.org/docs/plasma/widget/setup/) | QML widgets in Plasma panels or desktop | Depends on Plasma, not a standalone cross-compositor app | **Reject for this target** |

The install/runtime observations above are from project docs, not reproducible measurements. Verify current versions, distribution packages and functionality before adopting one.

## What Wayland permits

The standard [xdg-shell protocol](https://wayland.app/protocols/xdg-shell) describes ordinary application windows. An ordinary window participates in the compositor's tiling layout; it **does not reserve a screen-edge strip by itself**. Sway can tile a normal window beside a browser, and its [window criteria and commands](https://man.archlinux.org/man/sway.5.en) can identify and arrange it. Sway configuration should remain user-owned and optional; none of the application's data fetching or drawing needs Sway IPC.

The [wlr-layer-shell protocol](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) permits edge-anchored desktop surfaces with an *exclusive zone* (an amount of edge space the compositor is asked to withhold from other surfaces). That is a compositor-controlled request, not a universal guarantee. It changes the window into a panel-like surface rather than a normal tiled client. [gtk4-layer-shell compatibility notes](https://github.com/wmww/gtk4-layer-shell#supported-desktops) list Sway and other supported desktops but not GNOME Wayland. A normal-window fallback is essential if panel mode is ever added.

### Workspace-only appearance without a layer surface

**Checked against the Sway manual and protocol on 2026-09-26; not tested on the owner's compositor.** The visible narrow strip labeled "Status Stripe" at the top of the prototype capture is Sway's normal window title decoration, not the dashboard's `Usage` header. Sway's [`border none` rule](https://man.archlinux.org/man/sway.5.en) removes that decoration from an ordinary application window. The same manual documents assignment by Wayland `app_id` (the application's identifying string), so the window can remain on the chosen workspace. For example, with a user-selected workspace number:

```text
assign [app_id="org.mschulkind.Sidescope"] workspace number 3
for_window [app_id="org.mschulkind.Sidescope"] border none
```

An ordinary tiled window occupies its share of a horizontal split; the browser fills the remaining share. Sway can resize the stripe to approximately 640 logical pixels in that layout, but neither the application's requested initial width nor a static Sway rule guarantees a hard width across changes in output scale, tile count or arrangement. Verify the actual `app_id` via [`swaymsg -t get_tree`](https://man.archlinux.org/man/sway.5.en) on the owner's machine before installing a rule. The dashboard itself needs no Sway IPC (inter-process communication with the compositor) just to be placed and undecorated.

Inactive-window transparency is a separate compositor or focus-event-script policy, not something an unfocused app can reliably cancel by painting itself opaque. Sway's [`opacity` command](https://man.archlinux.org/man/sway.5.en) can set an initial value, but a focus-event script may override it later. Find the user's actual opacity mechanism, then exempt this `app_id` in that mechanism; if the effect is SwayFX's [inactive dimming](https://github.com/wlrfx/swayfx), a per-window zero-dimming rule may suffice, but dimming and transparency are different effects. The [SwayFX focus-event transparency helper](https://git.pogmom.me/pogmommy/swayfx/src/commit/1d3cbe9f278297fffa83e4e0c99d8ba46903cc3e/contrib/inactive-windows-transparency.py) is an example of policy that must exclude the dashboard itself.

A Waybar-like layer surface is bound to an **output**, not a workspace: the [layer-shell protocol](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) defines output, layer and edge reservation but has no workspace parameter. It would normally remain visible on other workspaces on that output. A workspace-specific layer panel would need extra Sway IPC to show/hide itself when workspaces change, manage reservation changes and handle output/fullscreen cases. Layer surfaces also default to **no keyboard focus**; enabling on-demand focus would be needed for this dashboard's refresh, help and scrolling keys. **Verdict:** prefer an undecorated, opacity-exempt tiled window for a dashboard wanted on one workspace. Reconsider layer-shell only if output-wide panel behavior (or its added workspace-tracking complexity) is desired.

> [!IMPORTANT]
> Generic Wayland rendering and automatic edge reservation are different promises. A Sway-friendly prototype does not establish support on all Wayland desktops.

## Native UI shortlist

| Stack | Why consider it | Risk and verdict |
| --- | --- | --- |
| [GTK4](https://docs.gtk.org/gtk4/css-overview.html) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/styles-and-appearance.html) + [gtk-rs](https://gtk-rs.org/) | Styled text, images, layout and [accessibility](https://docs.gtk.org/gtk4/section-accessibility.html); [GTK4 layer-shell library](https://github.com/wmww/gtk4-layer-shell) exists | Native libraries required; **Proposed application stack**; validate bindings and actual resource use |
| [Slint](https://github.com/slint-ui/slint) + Rust | Declarative layout and renderer options | Layer-shell route, input/accessibility and [license choice](https://github.com/slint-ui/slint#license) need proof; **challenger prototype only** |
| [iced](https://github.com/iced-rs/iced) + Rust | Customizable native UI | Layer-shell integration/version risk; **defer** |
| [Qt Quick](https://doc.qt.io/qt-6/qtquickcontrols-styles.html) | Flexible visuals and controls | Verify QML plus [LayerShellQt](https://invent.kde.org/plasma/layer-shell-qt) path; **defer unless Qt is already preferred** |
| [gotk4](https://github.com/diamondburned/gotk4) + Go | GTK from Go | Binding coverage and stability need validation; **conditional**, not a reason to pick Go first |
| [Wry/WebKit](https://github.com/tauri-apps/wry) | HTML/CSS design freedom | Browser engine and subprocesses complicate a lean app; **defer until measured need** |

These are dependency and integration assessments, **not a memory ranking**. Build representative cards in the chosen app at realistic dimensions, imagery and refresh rates; capture startup, idle and active CPU plus process-tree [proportional set size (PSS)](https://docs.kernel.org/filesystems/proc.html) from `/proc/*/smaps_rollup` on the same machine. Include renderer subprocesses and record compositor, scale and graphics backend. Existing Waybar/Eww usage may provide context but is not a contest to decide whether to build the app; shared-library memory must not be summed as if entirely private.

## Fast-moving — verify before building

- Current GTK/gtk-rs and gtk4-layer-shell versions and API compatibility.
- Distribution packages and layer-shell availability on the target desktop(s).
- Legitimate, stable, user-authorized AI, weather and email sources, including permission and refresh limits. The [provider source review](./ai-usage-sources.md) identifies an existing versioned local report and implementation leads, **not authenticated or provider-authorized verification**.
- Licensing and dependency compatibility for GTK4 bindings and any remote data providers.

## Experiments that would change the recommendation

1. Render a complete-column GTK4 design on Sway; test resizing, restart, workspace assignment, multiple outputs, fullscreen, readable type and accessible focus order.
2. Verify GTK bindings and measure actual application resource use before asserting an efficiency advantage. If a hard requirement fails, evaluate a native alternative against the same accepted product design.
3. Only if the user chooses automatic edge reservation instead of tiling, test layer-shell placement on Sway and another compositor and specify a fallback on unsupported desktops.
4. Inspect the existing AI usage source without publishing tokens, account identifiers or internal paths; determine whether the data can be consumed safely and reliably. Investigate weather and email providers with the same rigor and revise unsupported promises explicitly.

## Sources and confidence

Project documentation and the linked protocol descriptions above were read during the 2026-09-26 research workflow. Protocol capability and documented toolkit support are **documentation findings**. Product fit and stack priority are **inferences**; no code, visual mockup, compositor compatibility matrix or benchmark was verified locally. Each table links to the documentation relevant to its finding; follow the links to recheck fast-moving details.
