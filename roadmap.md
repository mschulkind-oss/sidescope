---
title: "Sidescope roadmap"
date: 2026-09-26
status: in-review
tags: [roadmap]
summary: "Finish the product design, then implement the custom Wayland application."
vantage:
  status-chip: true
---

# Roadmap

The [dashboard expansion handoff](docs/plans/dashboard.md) is ready for independent integration review and [host verification](docs/qa/dashboard.md); broader product decisions remain in the linked design.

This is a routing table, not a feature catalog or a progress log. Details live in the linked documents. Order: settle the product experience and risky integration boundaries, then produce a build handoff, then implement the approved design. Remove completed work rather than recording history here.

## Dashboard integration

1. [Independent review and host QA](docs/plans/dashboard.md) — review the expanded native dashboard before accepting the integration; see [actual observations and unverified limits](docs/qa/dashboard.md).

## Command-driven charts

1. [Verify command charts on the host](docs/tasks/command-charts.md) — the shipped charts, the headless [authoring tools](docs/reference/command-charts/authoring-tools.md) and the two quota wrappers are verified with synthetic commands and fixtures only; the real collector and producer, desktop, screen reader and unexplained visual failure remain in the [QA record](docs/qa/command-charts.md#not-verified).

## ▶️ Actionable

| # | Next agent action | Doc | Stops at |
| --- | --- | --- | --- |
| **1** | Draw complete-column visual directions with AI, weather, mail and error states; establish width, type, imagery and keyboard behavior | [Design](docs/design/sidebar.md#experience) | User reviews the product experience and rules [placement](docs/design/sidebar.md#OQ-SS2) and [email disclosure](docs/design/sidebar.md#OQ-SS3) |
| **2** | Verify real provider availability and refresh/credential terms for OpenRouter, DeepSeek, Claude, Codex, z.ai, Agy and Muse Code, whose reported shape differs between ai-usagebar releases; recheck [weather/air terms](docs/research/dashboard.md) before distribution and investigate mail separately | [AI source research](docs/research/ai-usage-sources.md) · [design](docs/design/sidebar.md#architecture) | Record viable sources or explicitly revise unsupported promises |
| **3** | After product design review, plan and implement remaining mail, settings and custom-data capabilities with tests | [Design](docs/design/sidebar.md) · [readiness checklist](docs/OPEN_SOURCE_CHECKLIST.md) | `just check-ci` passes, target Sway experience is verified, and owner reviews history before publication |

## 💬 Needs you

| Question | Blocks | Until then |
| --- | --- | --- |
| [OQ-SS4: which name should the repository, crate and app ID ship under?](docs/NAME_CANDIDATES.md#OQ-SS4) | Settled | Renamed to **Sidescope** (`sidescope`, `org.mschulkind.Sidescope`) on 2026-11-18 |

No immediate ruling blocks the design work. The two live questions in the [design](docs/design/sidebar.md#open-questions) become owner actions when the full-column directions have been presented. Recheck with `rg -n '💬.*OQ-' docs/` before moving them here.

## 🔒 Waiting

| Blocked on | Clears when |
| --- | --- |
| Host-native library/build and real Sway desktop unavailable inside this jail | Owner runs `just doctor`, `just build`, then `just dev` on their workstation and reports any output; see [QA limits](docs/qa/prototype.md#not-established) |

## 🧊 Icebox

| Candidate | Revisit when |
| --- | --- |
| Compiled third-party widget plugins | Revisit only if the [command-driven chart contract](docs/reference/command-charts/README.md) cannot serve a concrete source or interaction |
| [Compositor-dependent edge panel](docs/research/wayland-sidebar.md#workspace-only-appearance-without-a-layer-surface) | The owner wants an output-wide panel or accepts workspace tracking; a one-workspace dashboard fits an undecorated tiled window better |

## What this file does not cover

Exploratory content lives in [widget ideas](docs/ideas/widgets.md); the provisional name and its availability evidence live in [name candidates](docs/NAME_CANDIDATES.md); publication prerequisites live in the [readiness checklist](docs/OPEN_SOURCE_CHECKLIST.md). Committed work gets a row when there is a next action; questions get a row when they gate that action; both leave when resolved.
