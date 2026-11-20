---
title: "Sidescope User Guide"
date: 2026-11-20
status: accepted
tags: [guide, configuration, widgets]
summary: "How to install runtime dependencies, configure widgets, manage ai-usagebar credentials, and tile Sidescope on Wayland."
vantage:
  status-chip: true
---

# Sidescope User Guide

Sidescope is a native Wayland status column built with Rust, GTK4, and libadwaita. It is designed to sit beside a tiled browser (640 px wide on a 2560×1440 display) and keep system telemetry, AI quotas, and weather visible at a glance.

---

## 1. First Run and `doctor`

Run `sidescope` with no arguments to launch in live mode. System telemetry cards work immediately with zero configuration.

If something fails to start or a card shows an unavailable state, run the built-in diagnostics:

```bash
sidescope doctor
```

If you launch via `uvx`, run `uvx sidescope doctor` — its Python preflight checks for `libgtk-4.so.1` and `libadwaita-1.so.0` before invoking the native binary.

### System runtime libraries

Sidescope dynamically links GTK4 and libadwaita (except when run via the Nix flake, which bundles them):

- **Ubuntu / Debian:** `sudo apt install libgtk-4-1 libadwaita-1-0`
- **Arch Linux:** `sudo pacman -S gtk4 libadwaita`
- **Fedora:** `sudo dnf install gtk4 libadwaita`

---

## 2. Supported Widgets

| Group | Cards | Requirements |
| --- | --- | --- |
| **System telemetry** | CPU + per-logical-core graphs, Memory/Swap, Network throughput, Root Disk, Hardware Sensors (`hwmon` temps & fans) | Built-in (`/proc`, `/sys/class/hwmon`); zero config |
| **AI usage** | OpenRouter, DeepSeek, Claude, Codex, z.ai, Antigravity (`Agy`), Muse Code | Optional: [`ai-usagebar`](https://github.com/akitaonrails/ai-usagebar) on `PATH` |
| **Weather & Air** | Current conditions, 12-hour & 5-day forecast, US AQI & PM2.5 | Optional: `curl` on `PATH` + explicit `[weather]` coordinates |
| **Charts** | Time series, time bars and ranked bars from your own command's JSON | Optional: `[[chart_sources]]` and `[[chart_cards]]`; see [section 7](#7-charts-from-local-commands) |

---

## 3. Configuration File

Configuration is optional. When present, Sidescope reads:

```text
~/.config/sidescope/config.toml
```

*(Or `$XDG_CONFIG_HOME/sidescope/config.toml` when `XDG_CONFIG_HOME` is set.)*

Sidescope never writes to `config.toml` at runtime. Invalid keys or unknown values fail startup with an error instead of silently falling back. See [config.example.toml](../config.example.toml) for a commented template, or run `sidescope --help` to print the full schema.

---

## 4. Setting Up AI Usage (`ai-usagebar`) and Credentials

Sidescope delegates all AI provider authentication and API polling to [`ai-usagebar`](https://github.com/akitaonrails/ai-usagebar) by running `ai-usagebar usage --json` as a subprocess every five minutes (or immediately when you press `R`).

### Installing `ai-usagebar`

Install [`ai-usagebar`](https://github.com/akitaonrails/ai-usagebar) following its README and make sure the `ai-usagebar` binary is on your `PATH`. You can verify Sidescope sees it with:

```bash
sidescope doctor
```

### How credentials work

- **Sidescope never reads, stores, or bundles API keys or OAuth tokens.**
- `ai-usagebar` inherits your desktop launch environment unchanged and reads whatever provider credentials you have configured for `ai-usagebar` (or local agent auth files).
- **`yolo-jail` setups:** If you run coding agents inside `yolo-jail`, Sidescope itself runs on your host Wayland desktop and calls your host `ai-usagebar`. Keep your provider credentials on the host for `ai-usagebar` (or share read-only via your `yolo-jail` credential setup) rather than copying keys into Sidescope.

### Filtering or hiding AI cards

By default, every provider `ai-usagebar` reports gets a card in the report's order. To narrow or reorder them, set `enabled` in `config.toml`:

```toml
enabled = ["openrouter", "deepseek", "openai", "anthropic", "antigravity", "muse"]

# Optional: include Antigravity's separate Claude & GPT OSS quota pools.
show_agy_gpt_oss = true
```

To hide all AI usage cards (keeping only system telemetry and weather):

```toml
enabled = []
```

---

## 5. Setting Up Weather and Air Quality

Sidescope **never** guesses your location from IP or GPS. Weather and air-quality cards remain unconfigured until you add explicit coordinates to `~/.config/sidescope/config.toml`:

```toml
[weather]
latitude = 40.7128
longitude = -74.0060
place = "New York"
units = "celsius" # or "fahrenheit"
```

- Requires `curl` on `PATH`.
- Coordinates are sent to [Open-Meteo](https://open-meteo.com/) (free API is for non-commercial use); your `place` label stays local.

---

## 6. Hardware Sensors

By default, Sidescope automatically discovers labeled CPU package / GPU edge temperatures and labeled fans under `/sys/class/hwmon`.

To override the automatic selection, add a `[sensors]` table using stable `/sys/devices/...` paths (avoid `hwmonN` numbers, which can shift across reboots):

```toml
[sensors]
channels = [
  { chip = "coretemp", device = "/sys/devices/platform/coretemp.0", channel = "temp1", label = "CPU package" },
  { chip = "amdgpu", device = "/sys/devices/pci0000:00/0000:00:01.1/0000:01:00.0", channel = "fan1", label = "GPU fan" },
]
```

Setting `channels = []` intentionally hides sensor channels.

---

## 7. Charts from Local Commands

Charts are opt-in. With no `[[chart_sources]]` table nothing runs and no chart card exists, and none appear in `--demo`, `--report` or `--browser`.

A **source** is one command, written as an argument list (no shell), that prints a versioned JSON answer. A **card** chooses a metric from a source and draws it. Several cards can share one source, so one command answer can feed a daily bar chart and a ranking at once. Cards appear under the other cards, in the order you list them.

> **Not sandboxed.** A source runs with Sidescope's authority and environment, like any program you start yourself. `sidescope doctor` prints each full command and whether its executable exists. Sidescope never installs, authenticates or starts anything for a source, and adds no automatic sync or collector action.

```toml
[[chart_sources]]
id = "example-metrics"
command = ["example-chart-data", "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}"]
format = "sidescope-chart-v1"   # or "columns-v1" with a [chart_sources.mapping] table
window = "3h"                   # or calendar_days = 5 (including today)
timezone = "America/New_York"
refresh_seconds = 30            # at least 5; timeout_seconds 1-60

[[chart_cards]]
id = "example-spend"
source = "example-metrics"
kind = "time-series"            # or "time-bars" or "ranked-bars"
metric = "amount"
group_by = ["project"]
```

`{from}`, `{to}` and `{timezone}` are replaced one whole argument at a time. Limits: one command per source at a time, four at once, 1 MiB of output and ten seconds by default. A failed run retries after 30, 60, 120, then 300 seconds; a missing executable or invalid configuration waits until you press **R** (or restart). The previous answer stays on screen, labeled as stale or as a failed refresh.

What a card tells you, whatever the source: the range and bucket, the unit, whether a total is **incomplete** (a known subtotal that leaves out something unknown; only a source that declares its unknown contributions nonnegative is shown as "At least" a lower bound, drawn hatched and starred), whether the current day is still open (**so far**), and how fresh the answer is. Missing data is never drawn as zero. **Details** lists every value with its exact interval and quality; **Info** shows the executable name, last attempt and success, next retry, latency and request window. Focus a plot and press Left/Right, Home/End to inspect a point, Enter to open Details, Esc to close it. Quit with Sidescope's window or SIGTERM and every command it started is stopped.

Ready-made, synthetic examples live in [`examples/charts/`](../examples/charts/README.md): two POSIX shell sources in the native format, and an opt-in profile for an agent-spend ledger that uses the generic `columns-v1` mapper. The profile's cards are labeled a **list-price estimate**, never an invoice. Design: [command-driven charts](design/command-charts.md).

---

## 8. Themes, Density, and Keyboard Shortcuts

### Runtime shortcuts

| Key | Action |
| --- | --- |
| `?` | Toggle the built-in keyboard shortcut overlay (`Esc` closes) |
| `R` | Refresh live report immediately |
| `D` | Toggle density (`screen-fit` vs `comfortable`) |
| `[` / `]` | Cycle through the 21 built-in themes |
| `J` / `K` or `↑` / `↓` | Scroll cards (`PageUp` / `PageDown` also work) |

### Persisting theme and density

Runtime theme/density changes are session-only. To set defaults in `~/.config/sidescope/config.toml`:

```toml
theme = "paper"
density = "screen-fit" # or "comfortable"
```

Valid `theme` IDs: `original`, `paper`, `terminal`, `blueprint`, `editorial`, `contrast`, `retro`, `cloud`, `minimal`, `nord`, `solar`, `rose`, `ocean`, `ember`, `orchid`, `forest`, `cobalt`, `sand`, `mint`, `noir`, `arcade`.

---

## 9. Wayland / Sway Tiling

Sidescope runs as an ordinary tiled Wayland window with `app_id` `org.mschulkind.Sidescope`. To pin it to a workspace and remove Sway's title bar:

```text
assign [app_id="org.mschulkind.Sidescope"] workspace number 3
for_window [app_id="org.mschulkind.Sidescope"] border none
```
