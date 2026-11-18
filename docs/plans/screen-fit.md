# Screen-fit density

User-approved opt-in direction: retain Comfortable/theme-curated layouts by default; add a separate runtime density button and D shortcut, with optional closed `density = "screen-fit" | "comfortable"` config and no writes.

Implementation:
- Measure real GTK card heights at the viewport's actual width; span CPU and pack smaller cards into independent columns. Evaluate exactly two CPU candidates (three/four cells); prefer broader traces if they fit the actual available height.
- Move redundant numbering/subtitles/reset/source prose into accessible Info; keep every reading, selected pool, source timestamp, attribution and stale/error state. Weather retains current conditions, five daily graphics and hourly Details; inline telemetry saves repeated graph headings.
- Preserve unchanged remote/provider subtrees across local/report updates, and keep source Info rooted even when readings change. Replan only on geometry/density/theme/topology changes; genuine overflow retains the normal scrollbar.
- Validate closed/default config, measured plans and bounded overflow in Rust; verify full seven-provider/twelve-core fixtures, controls, theme independence, popover/focus preservation and actual scroll ranges in private Sway. Directly inspect generated ignored PNGs.

Evidence and limits: [dashboard QA](../qa/dashboard.md#screen-fit-density). No source integrations, theme variants, config persistence or compositor resizing were added.
