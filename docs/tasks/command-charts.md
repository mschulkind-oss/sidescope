---
title: "Command-driven charts tasks"
date: 2026-11-20
status: in-review
stage: BUILT
next: "Owner runs the charts on the host with the real collector; investigate the unexplained first full-visual-suite failure"
tags: [tasks, charts]
---

# Command-driven charts tasks

The charts are built, reviewed, repaired, and documented in the [reference](../reference/command-charts/README.md). Evidence, every red run, and every unverified limit are in the [QA record](../qa/command-charts.md). What remains is verification that this jail cannot do.

- [ ] **Host-live check.** On the owner's workstation, run the example profile against the real `agent-activity` collector and confirm the numbers, labels, and labeled partial or unavailable states look right on the real Sway desktop. Steps are in the [examples README](../../examples/charts/README.md#trying-it-on-your-host).
- [ ] **Screen reader.** With a real AT-SPI session, listen to the card and plot labels; only property presence has been checked.
- [ ] **Unexplained visual failure.** The first full `just test-visual` run failed once inside the chart suite and its log was not kept. Hardening was added, but the cause is unproven. If a full run ever fails there again, the saved `charts-*.log` names the assertion; until then treat it as a known unconfirmed flake.
- [ ] **Release size and SIGKILL.** Optionally record the binary-size effect of the timezone database and decimal library, and decide whether Sidescope should protect descendants after its own SIGKILL.
- [ ] **Agent-activity requests.** Deliver the [proposed requests](../requests/agent-activity-charts.md) to the upstream author; nothing here waits on them.
