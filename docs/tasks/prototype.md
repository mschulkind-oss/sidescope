---
title: "Prototype tasks"
date: 2026-09-26
status: in-review
tags: [tasks, prototype]
summary: "Concrete handoff checks for the provider UI and its real Sway harness."
vantage:
  status-chip: true
---

# Prototype tasks

**Status:** CURRENT. This is the bounded [prototype plan](../plans/prototype.md), not the full product backlog.

| Work item | Completion evidence | Remaining risk |
| --- | --- | --- |
| Reuse the `ai-usagebar` versioned JSON report | Parser tests cover missing providers, stale/error states, incompatible schema and money-vs-percent behavior | No live account access verified |
| Draw six sample cards in a 640 px native GTK stripe | Full-height headless Sway capture shows six compact cards in two columns; 378 px falls back to one column | Historical capture; expanded weather/system QA is in [dashboard tasks](dashboard.md). No mail or accessibility audit |
| Pack independent card columns | Pure shortest-column assignment test plus target-width Sway crop; a breakpoint restores original source order at 378 px | Exact height measurement may vary with host fonts and scaling |
| Show quota pace honestly | Tests cover prose and timestamp/window derivation, invalid or absent fields, stale quotas and balances; inspect marker and labels in wide and narrow captures | Check-time estimates can differ from upstream near reset boundaries |
| Clarify Antigravity pools and windows | Synthetic Agy report tests default Gemini-only and opt-in both GPT/OSS windows; screenshots inspect 5-hour/weekly labels and absence of generic disclaimer | Older reports without exact window length cannot get an invented pace cue |
| Choose active providers locally | TOML parser and visible-card tests cover automatic visibility, explicit order, missing entries, empty list and named accounts; a one-card capture was inspected | Settings require restart; no multi-account presentation yet |
| Provide refresh and shortcut help | Unit tests cover shortcut routing, bounded scrolling, check-status wording and no-overlap gating; real Sway key injection exercised overlay open/close, narrow-view scrolling and a delayed synthetic live report's status change | No host keyboard/accessibility audit; five-minute polling policy remains provisional |
| Build a standalone desktop test environment | Nix shell and private Sway/grim/wtype harness generate 1440p and narrow captures | No target workstation verification of this revision or second compositor exercised |
| Run read-only quality gate | `just check-ci` must pass after final edits | Re-run after any code changes |
| Inspect local screenshots | Open both ignored PNGs after the final successful capture | Visual judgment is not a pixel regression test |
| Audit public-repo prerequisites | [Open-source checklist](../OPEN_SOURCE_CHECKLIST.md) tracks missing release and history gates | Do not publish without owner review |

Future product work remains in the [roadmap](../../roadmap.md). Do not treat checked prototype behavior as a decision to ship unsupported provider integrations.
