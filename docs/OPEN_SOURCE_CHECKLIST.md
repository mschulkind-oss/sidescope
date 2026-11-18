---
title: "Open-source readiness for Sidescope"
date: 2026-09-26
status: draft
tags: [open-source, checklist]
summary: "Public-repo and release gates for an unreleased GTK prototype."
vantage:
  status-chip: true
---

# Open-source readiness

**Status:** SKETCH, 2026-09-26. This is a prospective checklist, not a claim of compliance. This repository has prototype source, an Apache-2.0 license file, CI and release scaffolding, but no verified public release or distribution channel. The [roadmap](../roadmap.md) places the next actions; do not create a release just to check boxes.

## Before a public push

- [x] Choose a durable project/repository name — settled as **Sidescope** (`sidescope`, `org.mschulkind.Sidescope`) in [OQ-SS4](./NAME_CANDIDATES.md#OQ-SS4).
- [ ] Accept the [standalone-app design](./design/sidebar.md) after visual and data-source review; validate the proposed Rust/GTK4 stack before writing the implementation plan and build layout.
- [ ] Confirm the existing Apache-2.0 license matches the maintainer's canonical portfolio copy. Review the current `README`, repo-specific agent guidance and project-relevant ignore patterns before publishing.
- [ ] Verify `just doctor`, `just build` and `just dev` against the maintainer's actual host GTK/libadwaita packages; then verify the Nix development shell, toolchain pinning, `Justfile`, CI and weekly dependency updates on a fresh checkout. The tool pin in `mise.toml` targets tested `just` 1.58.0; verify it and the Nix-provided `just` on a fresh checkout.
- [ ] Keep contributor-facing documentation in the public tree. Put personal research, private account details and workstation-specific config in private side storage rather than in public commits.
- [ ] If a contributor jail config is needed, commit only generic package requirements; put personal mounts, devices, ports and secrets in gitignored `yolo-jail.local.toml` and run `yolo check` on any config edits.
- [ ] Before the **first public push**, audit the **entire git history**, tree and fixtures for credentials, personal identifiers, private infrastructure names and local paths; decide whether to squash unpublished exploratory history. Do not rewrite public history after publication. Take a backup bundle before destructive cleanup and repeat the scan after changes.
- [ ] On the host, create the repository under `mschulkind-oss`, add the remote and push only after that audit. Confirm it is included in the maintainer's standards/license coverage on the maintainer's host. Do not treat a local commit as publication.

## Before a usable release

- [ ] Build the designed application with automated behavior tests and a manual Sway visual/tiling check; state which other desktops have actually been tested.
- [ ] Publish measured startup/idle/active resource observations including subprocesses, with test hardware and methodology; avoid an unmeasured “lightweight” claim.
- [ ] Document data provenance, API terms, privacy model, credential handling and stale-data behavior for each live card. Never ship real usage or email captures as fixtures.
- [ ] Provide contributor setup, screenshots made with non-sensitive sample data, supported-platform notes, package/install instructions and a verification command. Establish issue/security/contribution paths through the org defaults.
- [ ] Validate distribution needs for a Wayland-specific app rather than assuming Homebrew is appropriate. Close host-side release permissions and standards-list gaps before the first tag.

## Release scaffolding (not a released product)

`just release 0.1.0` is the **only tag-cutting path** and is destructive: after a clean working tree, matching `Cargo.toml` version, local and remote tag checks, an `origin` branch matching the local commit, and `just check-ci`, it creates and pushes the annotated tag. It does not create a GitHub release inline. `just last-release` prints the highest version-sorted local `v*` tag, or nothing before the first tag. Run `bash scripts/test-release.sh` to test rejection and a successful tag push **only in an isolated temporary repository**; the workspace's tags and remote remain untouched. A remote may move between preflight and push; if a push fails, the new tag remains local for investigation. Review the public history and approve publication **before invoking the recipe**. This repo has no verified remote configured here; the recipe was not invoked on this checkout.

A pushed `vMAJOR.MINOR.PATCH` tag triggers `.github/workflows/release.yml` only under `mschulkind-oss`. It runs the same Nix quality gate as CI, then builds with Ubuntu 24.04 system GTK4/libadwaita libraries and pinned Rust 1.98.1 so the executable does not depend on Nix store paths. The workflow checks linked library availability, attaches an x86_64 Ubuntu 24.04 tarball (binary, README, LICENSE, install notes) and checksum, and creates a **draft** GitHub release for human review. It does not package GTK or `ai-usagebar`; users must install GTK4/libadwaita runtime libraries and provide the live data tool separately. This is not a statically linked or cross-distribution artifact. Before publishing a draft, test installation and runtime on a clean Ubuntu 24.04 Wayland host; no such distribution validation has been recorded.

The maintainer's portfolio playbook is the authoritative convention; it is not stored in this repository. Recheck this project-specific list against that playbook before publication.
