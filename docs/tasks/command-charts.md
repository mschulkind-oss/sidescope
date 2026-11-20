---
title: "Command-driven charts tasks"
date: 2026-11-20
status: accepted
stage: DECIDED
next: "Run T1–T3 in parallel against BASE, then T5, then T6; T4 is independent"
depends-on:
  - ../design/command-charts-plan.md
tags: [tasks, charts]
---

# Command-driven charts tasks

Track: Heavy. Detail, interfaces, fences, tests and gates live in the [plan](../design/command-charts-plan.md); behavior in the [design](../design/command-charts.md); evidence in [research](../research/command-charts.md). `BASE` = the foundation commit: `dd48573`.

- [x] **T0 foundation** (sequential): dependencies, module exports, exact decimal/time/config/dataset/reduction/geometry/state types, shells, synthetic native fixtures.
- [ ] **T1 runtime lane** (after T0): bounded process runner, scheduler/backoff, cancellation, diagnostics.
- [ ] **T2 native view lane** (after T0): chart model, drawing, accessibility, widgets, PNG evidence.
- [ ] **T3 mapping lane** (after T0): generic column mapper, synthetic command examples, opt-in agent-activity profile.
- [ ] **T4 request document** (independent): agent-activity feature requests.
- [ ] **T5 integration** (after T1–T3): main wiring, CLI/docs/config, private-Sway visual and orphan checks, QA record.
- [ ] **T6 combined independent review** (after T4, T5): one reviewer over the whole range.
