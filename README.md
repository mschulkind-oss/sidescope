# Sidescope

A native Rust + GTK4/libadwaita dashboard beside a tiled browser. The primary target is a 640 px stripe alongside a 1920 px browser on a 2560×1440 display. It now includes AI balance/quota cards, CPU and per-logical-core graphs, RAM/swap history, network throughput, root filesystem capacity, hardware temperatures/fan RPM, explicit-location weather and modeled air quality. It remains a prototype, not a supported-compositor matrix or a source-reliability guarantee.

Live data work stays off the GTK thread. Demo data is synthetic and deterministic. No credentials, personal reports or location guesses are bundled.

## Install

Sidescope is a native Linux application for Wayland compositors (such as Sway). Several installation paths are available depending on your environment:

### Nix Flake

The Nix flake bundles GTK4 and libadwaita runtime libraries in their tested versions, requiring no system-level package installation:

```bash
nix run github:mschulkind-oss/sidescope
```

### Python / uvx launcher

Launch via `uvx` with an automatic runtime environment preflight:

```bash
# Verify your system GTK4 / libadwaita libraries:
uvx sidescope doctor

# Launch the dashboard:
uvx sidescope --live
```

If required runtime libraries (`libgtk-4-1`, `libadwaita-1-0`) are missing, `uvx sidescope doctor` reports them alongside the exact distro install command.

### Homebrew (Linux)

Install via Homebrew on Linux using system GTK4 and libadwaita:

```bash
brew install mschulkind-oss/tap/sidescope
```

*(Or download `sidescope.rb` from the GitHub release assets.)*

### Cargo (crates.io)

```bash
cargo install --locked sidescope
```

Requires GTK4 and libadwaita development packages installed on your system (e.g. `libgtk-4-dev` and `libadwaita-1-dev` on Ubuntu/Debian).

### Release tarball

Download `sidescope-<tag>-ubuntu24.04-x86_64.tar.gz` from [Releases](https://github.com/mschulkind-oss/sidescope/releases), then unpack it anywhere on your `PATH`. The archive carries the binary, this README, the license and `INSTALL.txt`.

Runtime requirements for the tarball:
- Linux x86_64 on a Wayland compositor (Sway is what is tested)
- GTK4 and libadwaita runtime libraries — Ubuntu: `libgtk-4-1 libadwaita-1-0`
- `ai-usagebar` on `PATH` for live usage data; without it the app runs on synthetic sample data
- `curl` for configured weather and air-quality cards

### From source

```bash
git clone https://github.com/mschulkind-oss/sidescope
cd sidescope
mise install    # gets the pinned Rust and just toolchains
just doctor     # checks your GTK4/libadwaita development packages
just build      # compile only
just deploy     # release build installed for your user, no root
```

`just deploy` needs Nix only inside the developer jail; on a normal host it uses your system libraries and a host Rust toolchain. See [Develop the dashboard](#develop-the-dashboard) for the full command set, `just uninstall` to remove the installed binary, and [Open-source checklist](docs/OPEN_SOURCE_CHECKLIST.md) for what is and is not validated yet.

## Start reading

1. [Design proposal](docs/design/sidebar.md) — what the application should feel like, how it is structured and the decisions still open.
2. [Roadmap](roadmap.md) — the next design work, evidence gates and eventual build handoff.
3. [Research](docs/research/wayland-sidebar.md) — existing tools, Wayland constraints, native UI choices and evidence gaps.
4. [Widget ideas](docs/ideas/widgets.md) — candidate cards and rough effort estimates.
5. [Prototype plan](docs/plans/prototype.md), [AI source research](docs/research/ai-usage-sources.md) and [visual QA](docs/qa/prototype.md) — historical first-slice scope; [dashboard QA](docs/qa/dashboard.md) covers the current expansion.
6. [Open-source checklist](docs/OPEN_SOURCE_CHECKLIST.md) — what must exist before publication or release.
7. [Name candidates](docs/NAME_CANDIDATES.md) — the unsettled product and repository name, with current availability evidence.

## Current direction

A Rust + GTK4/libadwaita application runs as an ordinary tiled Wayland window. The target Sway workspace can place it beside the browser; the app itself should not take over window management. GTK is a design choice to validate on the target desktop, **not a measured claim that it uses less memory**. Eww and other sidebars informed the design but are not an interim product.

## Develop the dashboard

Run these from the repository root:

```bash
just doctor       # Check native build dependencies
just build        # Compile, do not launch
just dev          # Launch live data; rebuild/restart after source changes settle
just demo         # Compile changed code and launch the synthetic dashboard
just check-ci     # Format/lint verification and tests
just test-visual  # Private headless Sway, screenshots and geometry check
just deploy       # Release build installed for your user (no root)
```

The [environment wrapper](scripts/native-env.sh) uses **your system's GTK4/libadwaita development libraries and Rust toolchain on the host**, writing to `target/host`. Inside the yolo jail it enters the pinned Nix development shell and writes to `target/nix`. No per-side directory setting in your yolo config is needed. If host `just doctor` reports missing packages, install your distribution's GTK4/libadwaita development packages and compiler tools; the [doctor script](scripts/doctor.sh) prints example package names. Nix is not required for host builds. `just build` leaves a dynamically linked executable at `target/host/debug/sidescope` on the host (or `target/nix/debug/sidescope` in the jail); it never copies jail library paths into a host build. `just doctor-visual` checks optional screenshot tools.

`just test-visual` starts a private Sway session, tiles a browser placeholder with the real app at 2560×1440 and verifies the stripe is about 640 px wide. It writes ignored full-output, cropped-column, shortcut-overlay and narrow-stress screenshots under the local screenshots directory; open them after visual changes. The harness also asserts compact weather/core height budgets and captures deterministic one/two/full/rolling/missing history, Fahrenheit, stale, unconfigured, busy twelve-core/120-slot and 64-core scenarios (without remote requests), with measurement logs beside the PNGs. A synthetic two-second update regression checks that hourly Details stays open/focused across three local updates and can still be closed/reopened using the keyboard. Tight axis-strip captures and endpoint pixel checks avoid relying on scaled long-column previews. `just dev` checks for an installed `ai-usagebar` before running and invokes `ai-usagebar usage --json`; if it is absent, use `just demo` for sample cards. While `just dev` runs, it watches [`src/`](src/), [`Cargo.toml`](Cargo.toml), [`Cargo.lock`](Cargo.lock) and the optional Rust build script (`build.rs`), waits until edits have been quiet for one second, then rebuilds and restarts the window. If compilation fails, the existing window remains open; fix the source and save again to retry. Stop the watcher with Ctrl-C. It does not watch your personal configuration or upstream usage data; restart manually after editing config. Sidescope itself reads no credentials; the ai-usagebar subprocess inherits the launch environment unchanged (including through `just dev`). Do not copy credential files into this project. Install `curl` for configured weather/air requests; a missing command becomes an unavailable card. Tiles follow the agents your `ai-usagebar` reports, under that tool's own names. Read the [source caveats](docs/research/ai-usage-sources.md) before treating a live report as authoritative.

### Install for your user

```bash
just deploy       # Release build, installed as sidescope in ~/.local/bin
just uninstall    # Remove that binary again
```

`just deploy` needs no root and writes nothing outside your own directories: no system prefix, no desktop entry, no edit to `~/.config/sidescope`. It installs the release build as `sidescope` in `${SIDESCOPE_BIN_DIR:-${XDG_BIN_HOME:-~/.local/bin}}`, staging the file beside its destination so a compositor launch never opens a half-written binary, then prints the path, the source revision, and a warning when that directory is not on your `PATH`. Launch the installed binary with `--live` the way you launch the app today, for example `exec sidescope --live` in your Sway configuration; that reads your `ai-usagebar` report. A bare launch without `--live` renders the bundled **synthetic sample** instead — useful for looking at the design without any provider configured, and marked by the footer's `SAMPLE DATA` label. `--report <file>` renders a specific report file. Live mode still needs `ai-usagebar` on `PATH`, and your provider list stays in `~/.config/sidescope/config.toml`. `just release <version>` remains the separate tag-cutting path.

### Choose providers

By default the tiles are exactly the agents your local `ai-usagebar` reports, in its order; narrowing or reordering them is optional. To explicitly choose and order providers, create `~/.config/sidescope/config.toml` (or `$XDG_CONFIG_HOME/sidescope/config.toml`):

```toml
enabled = ["openai", "anthropic", "antigravity"]
# Optional: include Antigravity's independent Claude & GPT OSS 5-hour and weekly quotas.
show_agy_gpt_oss = true
```

**The report is the inventory.** Every agent your `ai-usagebar` reports gets a tile, in the report's own order and under that agent's name, so enabling or disabling a vendor there is the only configuration this dashboard needs; an agent it does not report has no tile. `enabled` narrows that list rather than defining it, and any id it names is accepted, including one the report omits today. Agy shows its Gemini 5-hour and weekly quotas by default; `show_agy_gpt_oss = true` also shows both separate Claude & GPT OSS quotas. Leave that line out (or set it to `false`) to keep them hidden. A report that carries only a custom provider's spend and call rows still reads without inventing a quota window, and an explicitly selected provider stays visible even when its report entry is missing, so you can see the unavailable state. `enabled = []` intentionally hides all AI cards. Restart after editing the config; invalid IDs or syntax cause a startup error rather than silently using unexpected defaults. This file contains **only non-secret preferences and an optional explicit location**, never tokens or passwords. `enabled = []` hides AI providers only; system, weather and air cards remain visible. See [config.example.toml](config.example.toml). The visual harness ignores your personal config; optionally set `SIDESCOPE_VISUAL_CONFIG=/path/to/test-config.toml` to capture a chosen subset.

### Themes

The original appearance remains **Original** (1/21). The footer has previous/next buttons, the current name/index and a scrollable click-to-choose menu. Press `[` / `]` for previous/next, or wheel **over the selector only**; ordinary dashboard scrolling is not reassigned. Choices wrap at both ends and apply instantly without restarting; a theme switch intentionally rebuilds card geometry. Native picker navigation takes precedence while its menu is open.

The 20 alternatives are **Paper Ledger**, **Phosphor Terminal**, **Blueprint**, **Editorial**, **Signal Contrast**, **Retro Console**, **Cloud Soft**, **Quiet Minimal**, **Nord Observatory**, **Solar Study**, **Rose Atelier**, **Ocean Depths**, **Ember Workshop**, **Orchid Studio**, **Forest Fieldnotes**, **Cobalt Instruments**, **Sandstone**, **Mint Modern**, **Noir Cinema** and **Arcade Midnight**. These vary component surfaces, borders/radii/shadows, typography, plot grids/dashes/points/fills and independently colored CPU/RAM/swap/RX/TX/weather series—not just whole-palette substitutions. Generic serif/monospace fonts fall back to installed fonts.

Optionally add a top-level preference before `[weather]`:

```toml
theme = "paper" # omitted means "original"
```

Each of the **21 curated themes** owns a cohesive layout as well as component/graph styling. **Original** restores compact inline-label CPU cores; technical themes (Terminal, Blueprint, Retro, Solar, Ember, Cobalt, Arcade) combine compact cores, inline RAM/network readings and a twelve-hour condition-glyph strip with five small daily range tiles. Presentation themes use roomy full-width core plots and weather curves/daily rows; Cloud, Minimal, Nord and Mint mix roomy cores with compact strips/inline telemetry. Card spacing and heading typography complete each direction. Comfortable has no separate component-layout setting or exhaustive variant picker; the independent density control below is not another theme registry. Strip tooltips and keyboard **Details** retain every hourly temperature/condition; `—` and unknown glyphs retain missing data.

Valid ids, in menu order: `original`, `paper`, `terminal`, `blueprint`, `editorial`, `contrast`, `retro`, `cloud`, `minimal`, `nord`, `solar`, `rose`, `ocean`, `ember`, `orchid`, `forest`, `cobalt`, `sand`, `mint`, `noir`, `arcade`. Invalid ids fail startup; arbitrary CSS is not accepted. Runtime choices survive telemetry/report updates, but are **session-only**: the app never writes config or credentials. Edit the optional preference yourself for the next launch. [Theme QA](docs/qa/dashboard.md#themes-follow-up) records all screenshots and limitations.

### Screen-fit density

Click the footer density button or press **D** to switch between **Screen-fit** and **Comfortable**. This control is separate from the 21-theme browser. Screen-fit is the default; **Comfortable** preserves each theme's curated CPU/weather/telemetry layout, and density never changes the selected palette or graph ink. Both choices survive local samples and report checks. Runtime choices are session-only, with no config writes.

For the next launch, optionally add a top-level preference before `[weather]`:

```toml
density = "screen-fit" # or "comfortable"; omitted means screen-fit
```

Screen-fit puts CPU across the wide column, chooses three or four readable core cells per row using measured card heights and the actual available viewport height, then packs smaller cards into two independent columns. RAM/swap and network readings sit beside their histories, with memory utilization percentages still visible and graph labels/scales in source Info; weather keeps current conditions and five graphic daily ranges, with all twelve hourly readings in **Details**. Card numbering, subtitles and optional source explanations move into keyboard-reachable **Info** popovers/tooltips. Every provider row keeps its reset window (for example **Resets in 2d 22h**) in the legend under its bar; the bar's tick marks elapsed time in place of Comfortable's separate **TIME n%** line. The tick belongs to the report's reset window and the clock, so a provider that states its window keeps it even when the reading it sits on is stale. Main readings, all selected provider budgets, relative source/model freshness, attribution, stale labels and errors remain visible. Supporting text is at least 12 px; this is not a universal font shrink or a clipped fixed-height dashboard.

The deterministic full thirteen-card fixture (the six reported agents, with all four optional Agy budgets, twelve busy CPU cores, RAM/swap, network, disk, four sensor readings, weather and modeled air) fits without scrolling at 640×1440 across all 21 themes in private Sway. Shorter/narrower windows, 64 cores, lengthy errors or unusually long values can still scroll honestly. At narrow widths cards return to one column in source order. Geometry is replanned on window/density/theme changes, not every two-second sample; unchanged hourly Details and source Info controls stay rooted across readings/report updates. Changing density or theme intentionally rebuilds geometry.

`just test-visual` now also captures both densities, all 21 Screen-fit themes and narrow/high-core/error/null/extreme-value fallbacks, asserting real GTK scrollbar ranges, visible readings and horizontal text allocations. See [density QA](docs/qa/dashboard.md#screen-fit-density).

### Keyboard and refresh

The header and footer brighten when the compositor gives the stripe keyboard focus and return to a quieter color when focus leaves. This follows the window manager's active-window state, not pointer hover or a provider's live-data state; no focus-status text is shown. The footer's first line names the running build — **v0.1.0 · \<git-revision\>**, with **-dirty** appended when tracked files had uncommitted changes at build time — so a visible window identifies its own revision without a log. Press `?` to show or hide the shortcut guide; `Esc` dismisses it. Close the window to quit. `J`/`K` or ↓/↑ scroll the cards, and Page Down/Page Up move a page; these work even when the scroll area is not focused. In live mode, `R` or the header button requests an immediate update. The footer shows **CHECKING REPORT…** while the command runs and **REPORT CHECKED HH:MM:SS** when it finishes. That is the local report-check time, **not** proof of a fresh provider network request: `ai-usagebar` may return cached data. The report subprocess is limited to 45 seconds and 1 MiB of output. Failed report checks mark retained cards stale and remove current pace verdicts, keeping only the window's time mark. Live mode also polls the whole `ai-usagebar` report every **five minutes**, skipping a tick while a request is already running; the upstream tool handles provider caching and rate-limit backoff. This is a provisional prototype schedule, **not** a verified safe interval for every provider. Sample mode does not poll or offer a refresh button.

In Comfortable at 640 px, cards form **masonry columns**: each new card goes beneath the shorter column, without empty row-height gaps. At narrow widths they return to one column in configured provider order. Quota bars show the **percentage used**; when the source also states how much of the reset window has elapsed, a light tick marks that time position. Orange **OVER PACE** means usage is further along than time; green **UNDER PACE** means the reverse. The labeled difference is in **percentage points** (for example, 58% used minus 57% elapsed is `+1 pt OVER PACE`), not money or requests. An on-pace bar is blue. The marker appears when the source supplies elapsed time, or both a valid reset timestamp and window length. Elapsed time is a property of the window, so a valid reset timestamp and window length keep marking the time position on a stale row — the clock keeps moving — while the point-difference verdict, which compares the stale reading with that window, does not appear. Monetary balances never get quota bars. Five-hour and weekly quota labels use the same wording across providers; Agy adds its pool name so its separate budgets do not look interchangeable.

## System and environment cards

- CPU total and every logical core: `/proc/stat` deltas, including idle/iowait treatment; reset or newly added cores warm up rather than spiking.
- RAM used/total uses `MemTotal − MemAvailable`; swap has a separate value/history and explicitly reports when none is configured.
- Receive/transmit bytes per second use monotonic elapsed time across non-loopback interfaces. Virtual interfaces can double-count; these are not internet-only rates.
- Sensors read `/sys/class/hwmon` without commands, privileges or control writes. Defaults select labeled CPU package/GPU edge temperatures and labeled fans, not unlabeled motherboard temperatures. Temperatures use °C (millidegree inputs); fans use RPM. **0 RPM · stopped** is a reading, not absent data. Explicit `[sensors] channels` replace defaults; use the chip name, canonical device path (never `hwmonN`), channel and your friendly label as in [config.example.toml](config.example.toml). Nonzero RPM never implies a fan's physical role. A disappeared/faulty/unreadable source stays individually unavailable or visibly stale with its last success; hover a row or open Screen-fit Info for identity/time and raw-history slot/gap context. Sensor histories retain 120 signed raw 2-second slots in memory, with missing slots rather than zeros; temperature channels show mini trend plots with a stable auto-ranged °C scale while fans stay readings-only. No sensor configuration or machine readings are written by the app. See [sensor QA](docs/qa/sensors.md).
- Disk shows **Used** and **Available** for the root filesystem `/`, not every drive. Used is `(f_blocks − f_bfree) × f_frsize`; Available is `f_bavail × f_frsize`, normal-user usable space matching `df`'s Available column. Reserved blocks are neither used nor available. Values use GiB; `df -h` rounds differently.

Local telemetry is Linux-only, sampled every two seconds. CPU/RAM/swap/network histories retain at most **120 fixed slots** per series (about four minutes), in memory only; restart clears them. Each graph card names that span in its header corner (**last 4 min**); **Info** and the plot tooltip repeat the slot arithmetic. Newest values stay on the right, unused warm-up space stays blank on the left, and each new slot moves older points left without stretching the trace. Missing slots break the line rather than becoming zero. CPU/RAM/swap use a fixed 0–100% Y scale; network explicitly labels its automatic Y scale. Comfortable CPU shows a total trace followed by a two-column logical-core grid; Screen-fit uses a full-width three/four-column grid when wide. Compact cores have an inline ID/percentage and 24 px plot; roomy cores place the ID and right-aligned percentage above a full-width 28 px plot. Both retain thin, solid, unfilled traces with quiet 0/100% guides. Peaks are raw, not smoothed. Dense plots suppress theme markers/dashes/fills when samples are less than 6 px apart; larger, sparser plots retain those accents. Local ticks update only local card contents; unchanged weather widgets and an open/focused hourly Details popover remain intact. Failed reads retain the last success as stale with how long ago it was, or show unavailable when no sample exists. Freshness text is coarse and relative (**just now**, **24 min ago**, **2 h ago**); the footer's **REPORT CHECKED HH:MM:SS** stays a precise local clock because it is rewritten on every check.

OpenRouter shows the account **current balance** plus, when the report carries them, this key's spend today (with week/month detail) and this key's limit remaining — never a purchased-credit denominator, percentage or inferred spending. Key tier and recent models stay in source Info, not on the card face. DeepSeek shows only the **current balance**. The inspected ai-usagebar report has no dated provider daily-history series; the cards say daily spend history is unavailable. Neither balance deltas nor top-ups are used as spending evidence.

### Configure weather and air quality

Without a location, both cards explain how to configure them and make **no requests**. Add your own coordinates and local place label to the same preferences file:

```toml
[weather]
latitude = 0.0       # Replace with your chosen latitude
longitude = 0.0      # Replace with your chosen longitude
place = "Your chosen place"
units = "celsius"   # or "fahrenheit"
```

These are example coordinates, not a detected location. The label stays local; the coordinates are sent to Open-Meteo over HTTPS. No tracking, IP lookup or geocoding. Restart after changes. Original weather displays a compact current-condition glyph and temperature, one twelve-step hourly temperature curve and five small daily low/high range rows in the provider's local timezone; strip-oriented themes use the compact presentation described above. The current condition never word-wraps: longer text can ellipsize, with full tooltip/Details text. The **Details** button exposes the current condition plus all twelve hourly temperatures and conditions as keyboard-reachable text. Daily ranges share one five-day temperature scale; hourly min/max and endpoint times label the curve. Unknown/null forecast values use gaps or `—` with unavailable text details, never zero. Cairo glyphs are drawn locally, without external artwork or optional icon-theme dependencies. The synthetic weather card measures about 376 px high at both the target and narrow widths.

Weather and air refresh automatically every 30 minutes with one worker in flight; `R` refreshes **AI only**. Each HTTPS request has connection/total timeouts and a 1 MiB response limit. On failure, last successes remain visibly stale with the model's age and the check time; no rapid retries. Source errors are generic and never display process stderr or secrets.

Air quality is the practical **Open-Meteo/CAMS modeled alternative**, with US AQI and PM2.5, **not PurpleAir or a nearby sensor observation**. PurpleAir support/key handling is not implemented; do not put any key in this config. Attribution is visible on the cards: [Open-Meteo](https://open-meteo.com/) / [CAMS ENSEMBLE](https://ads.atmosphere.copernicus.eu/datasets/cams-europe-air-quality-forecasts), CC BY 4.0. The [free API terms](https://open-meteo.com/en/terms) restrict use to non-commercial purposes; commercial endpoints/accounts are not supported. Forecasts and AQI are not health advice.

See [dashboard research](docs/research/dashboard.md), [implementation plan](docs/plans/dashboard.md), [tasks](docs/tasks/dashboard.md) and [QA](docs/qa/dashboard.md). Authenticated provider reliability, a real user-configured remote forecast, host desktop behavior, accessibility and resource benchmarks remain unverified. Email, custom extensions and card settings UI remain outside this expansion.
