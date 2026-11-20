# Sidescope

A native Rust + GTK4/libadwaita status column designed to tile beside your browser on Wayland.

Sidescope keeps live system telemetry, AI usage quotas, and local weather visible in a quiet 640 px column without taking over window management or storing credentials.

---

## What it shows

- **System telemetry (zero config):** CPU and per-logical-core history graphs, Memory/Swap, Network throughput, Root Disk capacity, and `hwmon` temperatures/fan RPM.
- **AI usage (optional):** Balances and reset-window pace indicators for OpenRouter, DeepSeek, Claude, Codex, z.ai, Antigravity, and Muse Code via [`ai-usagebar`](https://github.com/akitaonrails/ai-usagebar).
- **Weather & Air quality (opt-in):** Current conditions, 12-hour/5-day forecast, and US AQI/PM2.5 from Open-Meteo using explicit coordinates you choose.

---

## Install

Sidescope targets Linux Wayland desktops (tested on Sway). Except for Nix, packages rely on your system GTK4 and libadwaita runtime libraries (`libgtk-4-1 libadwaita-1-0` on Ubuntu/Debian, `gtk4 libadwaita` on Arch/Fedora).

### `uvx` (with built-in runtime `doctor`)

```bash
uvx sidescope doctor   # check GTK4/libadwaita, Wayland, and optional tools
uvx sidescope          # launch Sidescope
```

### Homebrew (Linux)

```bash
brew install mschulkind-oss/tap/sidescope
```

### Cargo

```bash
cargo install --locked sidescope
```

*(Requires GTK4 and libadwaita development headers, e.g. `libgtk-4-dev libadwaita-1-dev` on Ubuntu/Debian.)*

### Nix Flake

```bash
nix run github:mschulkind-oss/sidescope
```

*(Bundles GTK4, libadwaita, and `curl` with no host library setup needed.)*

### Release tarball

Download `sidescope-<tag>-ubuntu24.04-x86_64.tar.gz` from [Releases](https://github.com/mschulkind-oss/sidescope/releases) and unpack `sidescope` onto your `PATH`.

### From source

```bash
git clone https://github.com/mschulkind-oss/sidescope
cd sidescope
mise install    # pinned Rust and just toolchains
just doctor     # check GTK4/libadwaita dev packages
just deploy     # release build installed to ~/.local/bin
```

---

## Quick start

```bash
sidescope doctor   # verify Wayland, config.toml, and optional tools
sidescope          # launch live dashboard
sidescope --help   # print CLI usage, config.toml path, and full TOML schema
```

To configure **AI usage (`ai-usagebar`)**, **credential handling** (including `yolo-jail` setups), **Weather**, **Hardware Sensors**, **Themes**, and **Sway tiling rules**, see the **[User Guide](docs/guide.md)** and [config.example.toml](config.example.toml).

---

## Privacy & credentials

- Sidescope reads **no** API keys, tokens, or account files itself; AI cards read normalized JSON from `ai-usagebar usage --json`.
- Location is **never** inferred via IP or GPS — weather requests happen only when you set explicit coordinates in `~/.config/sidescope/config.toml`.
- Runtime theme and density cycling is session-only; Sidescope never modifies your config file.

---

## Development & documentation

```bash
just build         # compile debug binary
just dev           # watch sources and run live dashboard
just check-ci      # format, clippy (-D warnings), and all unit/CLI/shell tests
just test-visual   # headless Sway visual and geometry suite
```

- [User Guide](docs/guide.md) — setup, widgets, `ai-usagebar` credentials, configuration, and Sway rules
- [Design proposal](docs/design/sidebar.md) — product architecture and design decisions
- [Roadmap](roadmap.md) — upcoming work and open questions
- [Open-source checklist](docs/OPEN_SOURCE_CHECKLIST.md) — release readiness and verification notes
