#!/usr/bin/env bash
# Check native host prerequisites without installing or changing anything.
set -euo pipefail
MODE=${1:-all}
if [[ "$MODE" != all && "$MODE" != --build && "$MODE" != --live && "$MODE" != --visual ]]; then
  echo 'Usage: doctor.sh [--build|--live|--visual]' >&2
  exit 2
fi
missing=0
missing_build=0
check_command() {
  local name=$1 purpose=$2
  if command -v "$name" >/dev/null 2>&1; then
    printf 'ok      %-16s %s\n' "$name" "$purpose"
  else
    printf 'missing %-16s %s\n' "$name" "$purpose"
    missing=$((missing + 1))
    if [[ "$name" != ai-usagebar ]]; then missing_build=$((missing_build + 1)); fi
  fi
}
check_library() {
  local name=$1 purpose=$2
  if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists "$name"; then
    printf 'ok      %-16s %s (%s)\n' "$name" "$purpose" "$(pkg-config --modversion "$name")"
  else
    printf 'missing %-16s %s (development package, not just runtime)\n' "$name" "$purpose"
    missing=$((missing + 1))
    missing_build=$((missing_build + 1))
  fi
}
if [[ "$MODE" != --visual ]]; then
  echo 'Native application build:'
  check_command cargo 'Rust package manager'
  check_command rustc 'Rust compiler'
  check_command cc 'C compiler / linker'
  check_command pkg-config 'native library discovery'
  check_command python3 'quota wrapper tests'
  check_library gtk4 'GTK4 widgets'
  check_library libadwaita-1 'libadwaita widgets'
fi
if [[ "$MODE" == --live ]]; then
  check_command ai-usagebar 'provides usage --json; use just demo without it'
fi
if [[ "$MODE" == --visual ]]; then
  echo 'Headless Wayland visual test:'
  check_command sway 'headless compositor'
  check_command grim 'Wayland screenshot tool'
  check_command wtype 'Wayland keyboard injection for overlay test'
  check_command jq 'Sway geometry assertion'
  check_command file 'capture format check'
fi
if [[ "$MODE" == all ]]; then
  echo 'Optional for live data: ai-usagebar (required by just dev, not just demo).'
  echo 'Optional for visual tests: just doctor-visual.'
fi
if (( missing > 0 )); then
  echo "Missing $missing prerequisite(s)." >&2
  if [[ "$MODE" == --visual ]]; then
    echo 'Install the missing Sway/Wayland visual-test tools or run just build without them.' >&2
  elif (( missing_build > 0 )); then
    echo 'For native libraries, install GTK4/libadwaita development packages and build tools. Examples: Debian/Ubuntu: libgtk-4-dev libadwaita-1-dev pkg-config build-essential; Fedora: gtk4-devel libadwaita-devel pkgconf-pkg-config gcc.' >&2
  fi
  if [[ "$MODE" == --live ]]; then
    echo 'For live data, install/configure ai-usagebar or run just demo for sample cards.' >&2
  fi
  exit 1
fi
echo 'Required dependencies found.'
