---
title: "Command-driven charts implementation seed"
date: 2026-11-20
status: draft
stage: SKETCH
next: "After design acceptance, prepare a complete build handoff against the current repository."
tags: [plan, seed, charts]
summary: "Implementation observations surfaced during design; incomplete and not authority to build."
---

# Command-driven charts implementation seed

**Status:** 2026-11-20 — seeded during design; not a handoff, and incomplete until the design is accepted.

The [design](command-charts.md) wins on behavior. A planner completes this document only after that design is accepted with no unanswered questions. This seed records what design inspection surfaced; it deliberately has no implementation task/file map.

## Existing boundaries worth checking at planning time

- The [process runner](../../src/process.rs) already bounds stdout, execution time, and process-group lifetime. Preserve its cleanup tests when adding command-source diagnostics; stderr is currently discarded. Capturing both pipes must remain bounded and avoid pipe deadlock.
- [Plot geometry](../../src/plot.rs) assumes fixed local-history slots. Do not retrofit multi-hour CLI data into four-minute sensor history or change those cards' semantics accidentally.
- [Configuration](../../config.example.toml) is owner-authored and non-secret. Inspect its current parser and settings lifecycle before choosing the additive chart tables.
- The [quality gate](../../Justfile) is `just check-ci`; `just format` precedes the commit. Visual acceptance requires inspecting synthetic PNGs from the existing headless compositor path, not just a capture exit status.

## Consumer evidence to preserve

The upstream checkout observed during design is `bcdbf46`. Re-read the current CLI contract before writing an example profile because another agent is actively changing that repository. Never copy personal usage into fixtures.

```text
/ctx/agent-activity/docs/reference/ledger-v1.md
/ctx/agent-activity/internal/ledger/wire/types.go
/ctx/agent-activity/cmd/agent-activity/ledger.go
```

Synthetic fixture candidates surfaced by the design: omitted-zero columns, exact decimal sums, partially priced costs, subscription channels, empty/all-null data, out-of-order/overlapping intervals, tuple-label collisions, late historical corrections, current buckets, and DST calendar days. They belong at the consumer JSON boundary, not in a test-only data path.

## Planning work still owed

Check dependencies for exact decimal arithmetic, timezone/calendar calculations, generic JSON pointers, GTK chart accessibility, and settings parsing against the tree at planning time. Choose implementations then; no library choice is settled here. Establish the direct-contract source first so adding generic column mapping cannot become a prerequisite that blocks usable charts.
