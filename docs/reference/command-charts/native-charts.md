---
title: "Command charts: native charts, reductions and interactions"
status: accepted
stage: CURRENT
verified: 2026-11-20
verified_commit: 14fb83d
covers:
  - src/chart/reduce.rs
  - src/chart/geometry.rs
  - src/chart_view.rs
  - src/chart_view/
  - src/chart_host.rs
tags: [reference, charts, presentation, accessibility]
summary: "How a dataset becomes a time series, time bars, or ranked bars: reductions and quality propagation, ranking and capacity, honest labels, axes, accessibility, and keyboard interaction."
---

# Command charts: native charts, reductions and interactions

**Status:** Verified 2026-11-20 against `14fb83d`. MEASURED: pure-model tests, Cairo rendering into image surfaces, and captures from a private headless compositor at 640 px and 378 px, opened and judged. UNMEASURED: host-desktop appearance, other compositors, and screen-reader speech. Orientation: [README](README.md).

Three primitives plus a shared summary header cover every view: **time series** (a line over real timestamps), **time bars** (vertical bars whose width follows real interval duration, optionally stacked by one dimension), and **ranked bars** (horizontal bars by category). Small multiples are a composition of these, not another widget. Every view is built by a pure function from a card configuration, a source snapshot, and the current time, so each source status and every dataset shape yields a model rather than a panic.

## Reductions

- A card reduces its metric over intervals with one reduction: **sum** for `interval_total` metrics, **last** for gauges. An unsupported pairing is a configuration or card error. Percentiles and ratios are never combined.
- `group_by` retains only the named dimensions and merges series that share a retained tuple, interval by interval. An absent dimension is a card mismatch. An empty list merges everything.
- **Quality propagates, never improves.** A sum is `missing` if *any* included cell is missing, `partial` if none are missing but one is partial, otherwise `complete`. Missing is never read as zero. The known subtotal (exact sum of present values) may be shown only beside an "incomplete" label, never as a complete-looking total.
- A gauge shows its last observation with that cell's quality and its actual interval. If a newer cell is missing the value is labeled **last known**, not current.
- "At least" is permitted only when the metric declares `unknown_contribution = nonnegative`. Otherwise the wording is "incomplete". A headline with a tiny negative net keeps its sign rather than rounding to zero.

## Honest meaning of each view

- **Time series.** For interval totals the y value is the amount **per bucket**, not a running total or a rate. Points sit at the interval center. Gaps break the line; there is no smoothing and no interpolation. A single point draws a mark, not a fabricated trend.
- **Time bars.** Widths follow real duration, so a 25-hour daylight-saving day is visibly wider than a 24-hour day. A bar that ends after the request instant is marked **so far** with a hatched, dashed style and a starred label. Stacking is allowed only for nonnegative interval totals; a missing component makes that stack incomplete. `stack_by` and `group_by` must agree; time bars stack by one dimension.
- **Ranked bars.** Each row is the reduction over the returned range, with its share of the known subtotal. Shares use known amounts, never the unknown remainder.

### Ranking and signed values

Ranking sorts by descending value with a stable-id tie-break, takes the top *N* named rows, and aggregates every remaining included row into **Other**, whose quality aggregates exactly like the named bars. A group whose dimension value is empty is **Unknown** (see the mapper's `unknown_value` in [the dataset page](dataset-and-mapping.md#cell-rules)). **Unknown is never folded into Other**, even below the top-*N* cut; it stays its own labeled row. Details always list the full ranking. Gauges cannot be aggregated honestly, so ranked gauges keep their observed interval, say "last known" when it is not the newest interval, and refuse instead of producing an Other.

Signed metrics rank by descending **signed** value, omit share percentages, and draw bars on either side of zero. Signedness is decided from the input cells, not the net total, so a metric with some negative cells and a positive net is still signed.

### Capacity

A plot never shows more than a fixed maximum of series; silent disappearance is forbidden. For `interval_total` metrics, groups beyond the cap become named rows plus Other (the cap reserves a slot for Other and one for each Unknown group). Unknown groups alone beyond the cap, and any gauge set beyond the cap, refuse **visibly** with their count. In every case Details keeps every row, and checked arithmetic means a valid dataset can never abort the application.

## Card-level mismatch

A card can be wrong while its source is fine. A card naming a metric the source lacks, an unsupported summary, `bounded_percent` on a metric whose unit is not `percent`, or time bars whose `group_by` disagrees with `stack_by` shows a **Card mismatch** message in that card only. The source stays published, so sibling cards on it are unaffected.

## Axes and color

- Nonnegative data include zero; signed data include zero and both extrema; a percent metric uses 0 to 100 only when the card declares `bounded_percent`. Ranges round outward to 1-2-5 steps, are recomputed only on an accepted snapshot, and never animate. All-zero data show a labeled zero baseline with a nonzero span; all-missing data draw no baseline implying observations. Logarithmic axes do not exist.
- Shared axes are used for comparable small multiples. A compact timeline shows three x labels. Time labels use the configured zone and Details always print the numeric UTC offset, which disambiguates repeated daylight-saving hours.
- Colors derive from the **full dimension tuple** through a hash into a small fixed categorical ramp, so a group keeps its color across rankings, windows, datasets, and themes; Unknown has its own neutral swatch. The ramp is chosen to remain separable under common color-vision differences.
- **Nothing depends on color or hover.** Partial values carry a non-color mark (hollow markers or hatching), incomplete totals carry words, and Unknown and so-far use pattern as well as hue.

## States

A card shows a banner for each source status: Loading, Stale, Refresh failed (with the prior data kept and labeled), or Source unavailable with a short reason. Refresh failed is immediate; Stale appears after the freshness threshold. With no prior success the card shows unavailable plus the reason instead of an empty chart.

## Interaction and accessibility

- A plot is keyboard-focusable. Left/Right and Home/End move a selection through intervals or ranked rows, Enter opens **Details**, and Esc closes it. Hover and keyboard selection show exact interval, group, unrounded value, quality, and unit.
- **Details** is a table (interval, group, value, quality, unit) in a popover; **Info** shows source status ([runtime page](command-runtime.md#diagnostics)). Both survive a refresh that replaces the snapshot underneath: the same widgets are updated in place, and open Details and focus are preserved when the card id is unchanged.
- The accessible label of the card and the plot states kind, title, unit, range, and freshness in words, and long visual labels keep their full accessible names. Colliding shortened labels gain parent context.
- The popover content is outside the card widget, so its styling is scoped by its own class; it needs horizontal scrolling at its minimum width and can extend outside a docked window on some compositors.
- Layout follows the application's 640 px column and density settings; chart cards are packed after the other cards, wide and narrow, and the section is reached by scrolling at 378 px.

## What the charts are not

Not a generic plotting library: no log axes, no smoothing, no secondary axes, no comparisons to a previous period, and no custom drawing from data. Not a chooser of what is measured. Not a place for tool-specific vocabulary.

## What a designer can change here

A new primitive needs a model type, drawing, a Details table row definition, a non-color encoding for every state, and a capacity rule; start from the [honest-meaning list](#honest-meaning-of-each-view). A new reduction needs a defined quality rule first. Changing sizes or the ramp is a Current values edit; changing the meaning of a state is a contract change.

## Current values

| Value | Setting | Owner |
| :--- | :--- | :--- |
| Maximum series drawn in one plot | 16 | `MAX_VISIBLE_SERIES` in `chart::dataset` |
| Ranked-bars `top_n` default / range | 5 / 1 to 16 | `DEFAULT_TOP_N` in `chart_view::model`, `MAX_TOP_N` in `chart::config` |
| Legend entries before overflow | 5 | `MAX_LEGEND_ENTRIES` in `chart_view::model` |
| Readout groups listed per selection | 5 | `MAX_READOUT_GROUPS` in `chart_view::model` |
| Plot height / ranked row height | 120 px / 22 px | `PLOT_HEIGHT`, `RANKED_ROW_HEIGHT` in `chart_view::model` |
| Points marked individually in a line | 48 | `MAX_MARKED_POINTS` in `chart_view::draw` |
| Categorical ramp length | 8 colors | `CATEGORY_COUNT` in `chart_view::palette` |
| Details table minimum width / maximum height | 560 px / 320 px | `chart_view::widget` |
| Details table header | Interval, Group, Value, Quality, Unit | `TABLE_HEADER` in `chart_view::model` |

## Where the code lives

`chart::reduce` (sums, last, `collapse_series`), `chart::geometry` (time axis, value scale), `chart_view` (`build_model`, `draw_plot`, `card_widget`, `update_card_widget`, `css`) with its `model`, `rank`, `draw`, `palette`, `format`, `a11y`, and `widget` children, and `chart_host` (dashboard section, demo scenarios, snapshot application). The application polls the runtime and re-evaluates freshness on a short timer; only a user Refresh asks for a new query.
