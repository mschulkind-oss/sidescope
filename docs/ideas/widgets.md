---
title: "Candidate cards for the spare column"
date: 2026-09-26
status: draft
tags: [ideas, widgets]
summary: "Ranked, costed starting ideas; not a commitment to build each one."
vantage:
  status-chip: true
---

# Candidate cards for the spare column

**Status:** SKETCH, 2026-09-26. An exploratory content catalog, not the architecture or a build queue.

**Constraint:** Design the complete column with multiple content families before implementing it. Costs below are **rough implementation time for one developer**, assuming a UI shell already exists; they are estimates, not measurements or promises. Account/API permissions can move a card out of range entirely. The [design proposal](../design/sidebar.md) owns the product commitment; this catalog ranks content and describes trade-offs.

| ID | Candidate | Value in this workspace | Estimated effort | Status |
| --- | --- | --- | --- | --- |
| 1 | Detailed AI quota / usage | Six target platforms—OpenRouter, DeepSeek, Claude, Codex, z.ai and Agy—expand the cramped Waybar indication on the browser workspace | Source-dependent; no reliable multi-provider estimate yet | Six-card visual prototype built; live source feasibility unresolved |
| 2 | Weather for entered location | Attractive, easy to scan, no account necessarily required | 2–5 days plus provider-term review | Design family; source and visual treatment unresolved |
| 3 | Email unread count | Useful glance at one mailbox | 1–3 weeks plus ongoing OAuth/privacy work | Design family; auth and privacy need review |
| 4 | Static image / local note | Visual breathing space or a small chosen prompt | 1–3 days after shell | Optional local customization candidate |

## Principles

1. The card earns its place only if it is more useful at this width than the existing top-bar module.
2. Stale, unavailable and permission-denied values are distinguishable; a screenshot should not reveal sensitive account detail by default.
3. Plan all core card families together; do not freeze a public plugin API or add a collector process for every possible service before their requirements are understood.

## Candidates and displacement

### 1. Detailed AI quota / usage

**Hook.** Give each of the six [target AI platforms](../research/ai-usage-sources.md) room to show its available balance or quota, reset time when supplied and timestamp, only where the browser is tiled. **Turn:** detail is local to a workspace, not squeezed into a persistent global bar.

**Cost and failure:** The effort estimate assumes an authorized stable source and permission to reuse it. If it is scraped from a session, exposes tokens or lacks trustworthy freshness, the estimate is invalid; do not ship invented precision. This **adds** a detailed view rather than replacing Waybar's global compact indicator. A source audit determines what can truthfully be promised for this card; it does not erase the weather design.

### 2. Weather for entered location

**Hook.** A small current-condition image and one-line forecast can make the space pleasant as well as useful. **Turn:** the user provides a place; avoid background location tracking. A public API may still have terms and rate limits. This **adds** a different kind of glanceable data; it does not replace #1. If quotas are not available, explicitly revise that card's source promise rather than quietly substituting weather as the product.

### 3. Email unread count

**Hook.** One private, opt-in number can be enough; do not build an inbox in a stripe. **Turn:** no subject lines, senders or message previews in the default view. Authorization and token storage cost more than the visual card. This **adds** a different information source; its authorization cost makes it riskier than #1 or #2, not absent from the full-column design.

### 4. Static image / local note

**Hook.** A selected image or short local prompt breaks up the data cards. **Turn:** no remote feed or synchronization needed. This **adds** a quieter visual/card type; it does not substitute for live integrations. It can be scoped as local customization without turning the app into a notes editor.

## Open threads

- Verify the existing usage indicator's provenance and whether its output may be reused safely without publishing private details. The source may turn out to be unusable.
- Measure the real column width and inspect visual mockups on the user's screen; without that constraint, card density is guesswork.
- Determine whether a weather source can be used under its terms without an account, and what data may be cached.
- Originally developed under the working name “Status Stripe”; now named **Sidescope**.

**My pick:** design a complete column with #1, #2 and #3, then investigate all three data boundaries before an implementation handoff. #4 is optional; it should not displace the information cards. Implementation order may vary, but no disposable interim product is planned.
