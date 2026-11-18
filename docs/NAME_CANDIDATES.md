---
title: "Name candidates for the Wayland status column"
date: 2026-09-27
status: accepted
tags: [naming, product]
summary: "Settled on Sidescope for the native Rust and GTK status-column application, with availability checked on 2026-11-18."
vantage:
  status-chip: true
---

# Name candidates

**Settled:** The project is named **Sidescope** (`sidescope`, `org.mschulkind.Sidescope`), chosen on 2026-11-18. The repository, crate, binary, config path, environment variables, and release workflow have been renamed from the working name `status-stripe`.

The name fits a native Rust/GTK window tiled beside a browser on Wayland, showing AI-usage cards plus weather, opt-in mail and local cards, without implying a Sway-only panel, a top bar, or an AI-only product.

## Open question

1. ✅ **OQ-SS4: Which name should the repository, crate and app ID ship under?**

   <!-- vantage: question id=OQ-SS4 status=answered -->

   **Answer:**
   > **Sidescope** (`sidescope`, `org.mschulkind.Sidescope`).
   >
   > Checked on 2026-11-18: `mschulkind-oss/sidescope` (`404`), `crates.io` (`404`), `PyPI` (`404`), `npm` (`404`), `AUR` (`0`), Flathub `org.mschulkind.Sidescope` (`404`), `.dev`/`.app`/`.io` DNS free (`.com` taken), and 1 exact GitHub repository (`sagnikonly/sidescope`, 1 star, TypeScript, no description) plus an empty `sidescope` GitHub organization.

## Current ranking

| Rank | Name | Status | Fit and cost |
| --- | --- | --- | --- |
| 1 | **Deskledge** | First choice on availability | A small ledge beside your work; clear everywhere checked, all four domains free. Reads as furniture, and "desk" is close to "dashboard". |
| 2 | **Dayledge** | First choice on fit, costlier name clearance | Short and easy to say; "day" may undersell weekly quotas. **`dayledge.com` is registered**, so the obvious domain is not available. |
| 3 | **glanceloom** | Clean fallback | A woven view of many cards; clear everywhere checked, all four domains free. A little long and manufactured-sounding. |
| 4 | **Status Shelf** | Descriptive alternative | Keeps continuity with the working name; "shelf" may suggest a horizontal row. Clear in every channel checked. |
| 5 | **statuscolumn** | Weakest of the checked set | Literally describes the layout, but one exact GitHub repo already uses the name and it is generic as a package and installation name. |
| — | **sidecar** | Ruled out | Taken at every channel checked: an existing GitHub user, 31 exact repositories, the crates.io crate, PyPI and npm. The engineering term (a companion process) also fights the product meaning. |

**Coverage limit:** "Clear" in these tables means the exact spelling returned a 404 from the endpoints listed in [the re-check method](#re-check-2026-11-18). It is not a reservation, a trademark search, or a claim about any channel not listed.

## Re-check 2026-11-18

Read-only HTTPS requests, no writes or reservations. Each row is the exact lower-case spelling.

| Name | GitHub user / org / own repo | Exact GitHub repo names | crates.io | PyPI | npm | AUR | `.com` / `.dev` / `.app` / `.io` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| dayledge | 404 / 404 / 404 | 0 | 404 | 404 | 404 | 0 | **registered** / free / free / free |
| deskledge | 404 / 404 / 404 | 0 | 404 | 404 | 404 | 0 | free / free / free / free |
| glanceloom | 404 / 404 / 404 | 0 | 404 | 404 | 404 | 0 | free / free / free / free |
| status-shelf | 404 / 404 / 404 | 0 | 404 | 404 | 404 | 0 | not checked |
| cardnook | 404 / 404 / 404 | 0 | 404 | 404 | 404 | 0 | not checked |
| statuscolumn | 404 / 404 / 404 | 1 | 404 | 404 | 404 | 0 | not checked |
| nookfolio | 404 / 404 / 404 | 2 | 404 | 404 | 404 | 0 | not checked |
| sidecar | **200** (user exists) | **31** | **taken** | **taken** | **taken** | 0 | not checked |

Method: GitHub user and organization endpoints and `api.github.com/repos/mschulkind-oss/<name>`; the repository search API (`<name> in:name`, filtered locally to exact case-insensitive matches) for exact repo names; `index.crates.io/<xx>/<yy>/<name>`; `pypi.org/pypi/<name>/json`; `registry.npmjs.org/<name>`; `aur.archlinux.org/rpc/v5/info/<name>`; and DNS NS lookups for the four TLDs over DNS-over-HTTPS.

`dayledge.com` resolves through Cloudflare nameservers (`gerald`/`sureena.ns.cloudflare.com`) and currently answers 403 "DNS points to prohibited IP". That is a registered domain with a broken A record, not a live product. Treat it as taken.

**Not checked, still unknown:** Debian, Ubuntu, Fedora and Nixpkgs package names; Flatpak/Flathub app IDs; PyPI and npm for every name except those in the table; trademarks; social handles; and confusingly-similar products beyond the GitHub searches. An empty result in one registry never implies availability in another.

## Publication-name gate

The repository has a Rust package named `status-stripe`, a Nix development shell and a tag-triggered release workflow, but no published distro package. Before adopting a name, list the intended release channels, then check the exact proposed package and executable names in each relevant catalog: crates.io if publishing a crate, Flathub if shipping a Flatpak, and Nixpkgs, Debian/Ubuntu, Fedora or AUR if targeting those ecosystems. Check the application ID separately if using Flatpak, plus repository and organization names, domains, confusingly similar software, and trademarks. Distinguish exact namespace collisions from similar-product confusion; an empty search is not a reservation, and a display name need not be identical to the package identifier. Do not claim availability in every distribution based on the GitHub and crates.io checks above.

## Naming work still to do

Once the owner rules on [OQ-SS4](#OQ-SS4), the rename is mechanical but not small:

- Crate and binary name in `Cargo.toml`, the `just deploy` install target, and `scripts/`.
- App ID `org.mschulkind.StatusStripe` in `src/main.rs`; the Sway `assign`/`for_window` rules in [the Wayland research](./research/wayland-sidebar.md) use the same string.
- The footer build line, the `INSTALL.txt` text and the release artifact name generated by `.github/workflows/release.yml`.
- `README.md` title, the description paragraph and every `github.com/mschulkind-oss/<name>` URL.
- The `status-stripe` config directory (`~/.config/status-stripe/config.toml`) and the `STATUS_STRIPE_*` environment variables. Renaming these breaks existing local setups, so if a rename happens it should be now rather than after a first release.

## Earlier candidate checks (2026-09-27)

Preserved as the record behind the current ranking.

A parallel naming pass suggested *Sivela*, *Tavela*, *Merilo*, *Aveta*, *Alongside*, *Perch*, *Dayboard*, and *Window Seat*. A second pass considered *Dayledge*, *Deskledge*, *Sideward*, and *Stillside*. The checks below favored names with fewer observed collisions, not names proven available.

| Name | Verdict | Observed evidence |
| --- | --- | --- |
| **Dayledge** | First provisional choice: short, a daily place for cards; “day” may undersell weekly quotas. | [GitHub name search](https://api.github.com/search/repositories?q=dayledge+in:name&per_page=100) returned 6 results, none an exact repo name; [GitHub user](https://api.github.com/users/dayledge), [crates.io index](https://index.crates.io/da/yl/dayledge), [Flathub proposed ID](https://flathub.org/api/v2/appstream/org.mschulkind.Dayledge), and [proposed public repo](https://api.github.com/repos/mschulkind-oss/dayledge) returned 404; [AUR exact-name query](https://aur.archlinux.org/rpc/v5/info/dayledge) returned zero. |
| **Deskledge** | Backup: a small landing place beside work; may sound like physical furniture. | [GitHub name search](https://api.github.com/search/repositories?q=deskledge+in:name&per_page=100) returned 4 results, none exact; [GitHub user](https://api.github.com/users/deskledge), [crates.io index](https://index.crates.io/de/sk/deskledge), [Flathub proposed ID](https://flathub.org/api/v2/appstream/org.mschulkind.Deskledge), and [proposed public repo](https://api.github.com/repos/mschulkind-oss/deskledge) returned 404; [AUR](https://aur.archlinux.org/rpc/v5/info/deskledge) returned zero. |
| **glanceloom** | Existing backup: strongly evokes mixed cards, but sounds manufactured. | Recheck found no exact [GitHub repo-name](https://api.github.com/search/repositories?q=glanceloom+in:name&per_page=100) results; [GitHub user](https://api.github.com/users/glanceloom), [crates.io index](https://index.crates.io/gl/an/glanceloom), and [proposed Flathub ID](https://flathub.org/api/v2/appstream/org.mschulkind.Glanceloom) returned 404; [AUR](https://aur.archlinux.org/rpc/v5/info/glanceloom) returned zero. |
| **Tavela** | Do not select without deeper brand review. | No exact GitHub repo name in the [search](https://api.github.com/search/repositories?q=tavela+in:name&per_page=100), but the [GitHub user](https://api.github.com/users/tavela) exists, an unrelated [iPhone app](https://apps.apple.com/us/app/tavela/id6781354143) uses the name, and [tavela.app](https://tavela.app/) is an education product. Empty package indexes would not resolve this product-name conflict. |
| **Sivela, Merilo, Aveta, Alongside, Perch, Dayboard, Window Seat, Sideward, Stillside** | Set aside: exact project or user names, adjacent app concepts, or relevant brand conflicts surfaced. | Examples: [Sivela repo](https://github.com/jalever06/Sivela), [Merilo repo](https://github.com/AlexandrKudrya/Merilo), [Dayboard dashboard](https://github.com/jah2488/dayboard), [Perch desk dashboard](https://github.com/mayukh4/perch), [Sideward user](https://api.github.com/users/sideward), and [Stillside repo](https://github.com/hausegger/stillside). These are screening findings, not legal conclusions. |

**Coverage limit (2026-09-27 pass):** The release scaffold produces a draft GitHub release with an Ubuntu-built tarball, **not** an Ubuntu distribution package. The Nix flake is a development shell, not a Nixpkgs entry, and no crates.io or Flathub publication is configured. Exact package checks for Debian and Ubuntu could **not** be established: Debian package and source pages redirected to a bot challenge, and an Ubuntu package query timed out. Fedora's package page and Nixpkgs search landing page loaded, but no reliable exact-name result was extracted from them. Package availability in those four ecosystems is **unknown**, not clear. Domain, app-store, broad similar-product, and trademark searches were not done in that pass.

## Original invented-name ranking

The long list that the current shortlist was drawn from, re-checked on 2026-09-27. Names 1–5 were clear in the checked channels at that time; everything from 6 onward was `Unknown` — no HTTP availability checks were made for that spelling.

| Rank | Candidate | Tier | Why it fits / what it costs | Checked-channel evidence |
| ---: | --- | --- | --- | --- |
| 1 | **glanceloom** | Recommended | A woven view of many cards; evocative, though a little long. | Clear in checked channels |
| 2 | **glantrail** | Recommended | A quick-read strip at the screen edge; slightly dock-like. | Clear in checked channels |
| 3 | **sidelume** | Recommended | A lit-up side space; short, but less explicit about data. | Clear in checked channels |
| 4 | **nookfolio** | Recommended | A small collection of useful cards; less immediate to say aloud. | Clear in checked channels, but two exact GitHub repos now use the name |
| 5 | **cardnook** | Recommended | Literal cards in a side nook; may undersell future layouts. | Clear in checked channels |
| 6 | glancery | Backup | Place to glance at things; invented suffix feels playful. | Unknown |
| 7 | sideloom | Backup | Many data threads gathered at one side; may sound like a craft tool. | Unknown |
| 8 | dayniche | Backup | A personal daily niche; less suitable for technical dashboards. | Unknown |
| 9 | panehaven | Backup | Calm home for information panes; generic sound. | Unknown |
| 10 | nooksignal | Backup | Useful signal tucked out of the way; a bit long. | Unknown |
| 11 | cardlume | Backup | Illuminated cards; two invented parts to explain. | Unknown |
| 12 | glancove | Backup | A glanceable cove; meaning takes a beat. | Unknown |
| 13 | stillfolio | Backup | Quiet collection of information; could sound like a notes app. | Unknown |
| 14 | sidemorrow | Backup | A side view of what's next; too future-oriented for current usage. | Unknown |
| 15 | paneplume | Backup | Light, airy window; attractive but vague. | Unknown |
| 16 | nookline | Exploratory | A slim, tucked-away line of information; sounds like a service. | Unknown |
| 17 | glancebay | Exploratory | A place to dock quick reads; slightly nautical. | Unknown |
| 18 | cardharbor | Exploratory | Sheltered collection of cards; too broad. | Unknown |
| 19 | dayweave | Exploratory | Interleaves daily signals; could imply a planner. | Unknown |
| 20 | sidemint | Exploratory | Fresh and unobtrusive; weak link to function. | Unknown |
| 21 | glancegrove | Exploratory | A calm cluster of signals; long for a binary name. | Unknown |
| 22 | panenook | Exploratory | A nook made of panes; repetitive image. | Unknown |
| 23 | nookscope | Exploratory | A small view into many things; might suggest surveillance. | Unknown |
| 24 | hushfolio | Exploratory | Quiet personal collection; could imply document storage. | Unknown |
| 25 | cardglade | Exploratory | Clear, open card space; nature image may feel arbitrary. | Unknown |
| 26 | dayharbor | Exploratory | Daily refuge for status; weaker technical identity. | Unknown |
| 27 | sidetrellis | Exploratory | Structure that supports many cards; long and garden-like. | Unknown |
| 28 | panehollow | Exploratory | Recessed window; might suggest emptiness. | Unknown |
| 29 | glancecove | Exploratory | Compact glance spot; overlaps the cove image above. | Unknown |
| 30 | cardmorrow | Exploratory | Cards for the day ahead; usage is about now, too. | Unknown |
| 31 | stillnook | Exploratory | Quiet corner; doesn't signal information. | Unknown |
| 32 | lumefolio | Exploratory | Bright collection; invented first word weakens recognition. | Unknown |
| 33 | panehush | Exploratory | Unobtrusive window; awkward spoken stress. | Unknown |
| 34 | signalnook | Exploratory | Signals kept in a small corner; sounds like monitoring software. | Unknown |
| 35 | cardmeadow | Exploratory | Spacious cards; may promise more room than 640 px. | Unknown |
| 36 | glancesill | Exploratory | Information perched beside the main window; sill is obscure. | Unknown |
| 37 | sideglade | Exploratory | A calm side area; weak product specificity. | Unknown |
| 38 | nooktide | Exploratory | Changing information in a little space; tide can imply motion. | Unknown |
| 39 | dayplume | Exploratory | Lightweight daily view; vague and ornamental. | Unknown |
| 40 | panemere | Exploratory | Still pool of panes; meaning is hard to infer. | Unknown |
| 41 | cardwharf | Exploratory | Cards gathered at an edge; industrial tone. | Unknown |
| 42 | **paneweave** | Collision | Good multi-card metaphor, displaced by glanceloom because of an existing exact repo name. | Exact GitHub repo `querielo/paneweave`; user 404; crate 404 |
| 43 | **quietpane** | Collision | Apt tone, but an existing exact repo name; also generic. | Exact GitHub repo `kgntmr/quietpane`; user 404; crate 404 |
| 44 | **daynook** | Collision | Appealing daily corner, but the GitHub user name is taken. | GitHub user exists; no exact repo result; crate 404 |
| 45 | **cardmere** | Collision | Soft card metaphor, but the GitHub user name is taken. | GitHub user exists; no exact repo result; crate 404 |

A further checked spelling, **dayfold**, was excluded from the ranked 45: a GitHub user and multiple exact repo names already exist, including `dayfold/dayfold` (crates.io returned 404). Do not treat an unknown row as available. If the owner prefers a backup, repeat the exact checks for it and then check organization names, package indexes, domains, search results for confusingly similar products, and trademarks in the intended markets before renaming the application or claiming any handle.

## Availability evidence and limits (2026-09-27 pass)

On 2026-09-27, read-only HTTP requests checked **10 exact spellings** against the public [GitHub user endpoint](https://docs.github.com/en/rest/users/users#get-a-user), [GitHub repository search](https://docs.github.com/en/rest/search/search#search-repositories) (`<name> in:name`, filtered locally to exact case-insensitive repository names), and the [crates.io crate endpoint](https://crates.io/data-access). Each request had a seven-second timeout. The repo searches returned `incomplete_results: false`; no writes, reservations, or account creation were attempted.

**Clear in checked channels** means a GitHub user lookup returned 404, the repo-name search returned no exact match, and the crates.io crate lookup returned 404 *at check time*. It does **not** guarantee that a GitHub organization, a future repo, a renamed or deleted repo, another registry, a domain, a social handle, or a trademark is available. In particular, GitHub repo search is an index lookup, not an exhaustive reservation check. **Collision** means at least one positive match on those checks. **Unknown** means no HTTP availability checks were made for that spelling. PyPI and npm were not checked in that pass. Registry state can change at any moment; the [2026-11-18 re-check](#re-check-2026-11-18) is the current evidence.
