#!/usr/bin/env bash
# Run inside `nix develop`; creates its own real headless Sway compositor.
set -euo pipefail
# GTK needs a private session bus; never attach tests to the user's desktop bus.
if [[ "${SIDESCOPE_PRIVATE_BUS:-0}" != 1 ]]; then
  bus_config=$(mktemp)
  cat > "$bus_config" <<'BUS'
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow user="*"/>
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
  </policy>
</busconfig>
BUS
  trap 'rm -f "$bus_config"' EXIT
  dbus-run-session --config-file="$bus_config" -- env SIDESCOPE_PRIVATE_BUS=1 bash "$0" "$@"
  exit $?
fi
# Nix stores GTK schemas outside the normal XDG data search path.
gtk_prefix=$(pkg-config --variable=prefix gtk4)
for schemas in "$gtk_prefix"/share/gsettings-schemas/*/glib-2.0/schemas; do
  if [[ -f "$schemas/gschemas.compiled" ]]; then export GSETTINGS_SCHEMA_DIR="$schemas"; break; fi
done
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/debug/sidescope"
CAPTURE=${1:-"$ROOT/screenshots/stripe.png"}
mkdir -p "$(dirname "$CAPTURE")"
RUNTIME=$(mktemp -d)
chmod 700 "$RUNTIME"
export XDG_RUNTIME_DIR="$RUNTIME"
export XDG_CONFIG_HOME="$RUNTIME/config"
if [[ "${SIDESCOPE_VISUAL_LIVE_STUB:-0}" == 1 ]]; then
  mkdir -p "$RUNTIME/bin"
  printf '#!/bin/sh\nsleep 1\ncat "%s"\n' "$ROOT/tests/fixtures/usage.json" > "$RUNTIME/bin/ai-usagebar"
  chmod +x "$RUNTIME/bin/ai-usagebar"
  export PATH="$RUNTIME/bin:$PATH"
fi
if [[ -n "${SIDESCOPE_VISUAL_CONFIG:-}" ]]; then
  mkdir -p "$XDG_CONFIG_HOME/sidescope"
  cp "$SIDESCOPE_VISUAL_CONFIG" "$XDG_CONFIG_HOME/sidescope/config.toml"
fi
if [[ "${SIDESCOPE_VISUAL_AGY:-0}" == 1 ]]; then
  report=$(< "$ROOT/tests/fixtures/agy-usage.json")
  session_reset=$(date -u -d '+2 hours' +'%Y-%m-%dT%H:%M:%SZ')
  weekly_reset=$(date -u -d '+3 days' +'%Y-%m-%dT%H:%M:%SZ')
  report=${report//@SESSION_RESET@/$session_reset}
  report=${report//@WEEKLY_RESET@/$weekly_reset}
  printf '%s\n' "$report" > "$RUNTIME/agy-usage.json"
  SIDESCOPE_VISUAL_REPORT="$RUNTIME/agy-usage.json"
fi
export WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_LIBINPUT_NO_DEVICES=1
export WLR_RENDERER=pixman LIBGL_ALWAYS_SOFTWARE=1 GDK_BACKEND=wayland
# The private test bus intentionally has no desktop accessibility services.
export GTK_A11Y=none
# Test-only GTK measurement assertions run inside the real renderer.
export SIDESCOPE_VISUAL_GEOMETRY=1
export XDG_CURRENT_DESKTOP=sway
unset WAYLAND_DISPLAY SWAYSOCK
sway_pid= browser_pid= app_pid= keyboard_pid=
cleanup() {
  local status=$?
  if (( status != 0 )) && [[ -f "$RUNTIME/app.log" ]]; then
    cp "$RUNTIME/app.log" "${CAPTURE%.png}-failed.log"
    tail -60 "$RUNTIME/app.log" >&2
  fi
  for chart_log in "$RUNTIME"/charts-*.log; do
    if (( status != 0 )) && [[ -f "$chart_log" ]]; then cp "$chart_log" "${CAPTURE%.png}-failed-$(basename "$chart_log")"; fi
  done
  for pid in "$keyboard_pid" "$app_pid" "$browser_pid" "$sway_pid"; do
    if [[ -n "$pid" ]]; then kill "$pid" 2>/dev/null || :; wait "$pid" 2>/dev/null || :; fi
  done
  rm -rf "$RUNTIME"
}
trap cleanup EXIT
printf 'output HEADLESS-1 mode 2560x1440\noutput HEADLESS-1 bg #101720 solid_color\nfocus_follows_mouse no\n' > "$RUNTIME/sway.conf"
sway -c "$RUNTIME/sway.conf" > "$RUNTIME/sway.log" 2>&1 & sway_pid=$!
for _ in {1..100}; do
  for sock in "$RUNTIME"/wayland-*; do
    if [[ -S "$sock" ]]; then WAYLAND_DISPLAY=$(basename "$sock"); export WAYLAND_DISPLAY; break 2; fi
  done
  if ! kill -0 "$sway_pid" 2>/dev/null; then break; fi
  sleep 0.1
done
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then echo 'Sway failed to start:' >&2; tail -40 "$RUNTIME/sway.log" >&2; exit 1; fi
for _ in {1..100}; do
  for sock in "$RUNTIME"/sway-ipc.*.sock; do
    if [[ -S "$sock" ]]; then SWAYSOCK=$sock; export SWAYSOCK; break 2; fi
  done
  sleep 0.1
done
: "${SWAYSOCK:?Sway IPC missing}"
swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
"$BIN" --browser > "$RUNTIME/browser.log" 2>&1 & browser_pid=$!
sleep 1
if [[ "${SIDESCOPE_VISUAL_LIVE_STUB:-0}" == 1 ]]; then
  "$BIN" --live > "$RUNTIME/app.log" 2>&1 & app_pid=$!
else
  "$BIN" --report "${SIDESCOPE_VISUAL_REPORT:-$ROOT/tests/fixtures/usage.json}" > "$RUNTIME/app.log" 2>&1 & app_pid=$!
fi
for _ in {1..100}; do
  if swaymsg -t get_tree | jq -e '[.. | objects | select(.name? == "Sidescope")] | length > 0' >/dev/null; then break; fi
  if ! kill -0 "$app_pid" 2>/dev/null; then echo 'Application exited:' >&2; tail -60 "$RUNTIME/app.log" >&2; exit 1; fi
  sleep 0.1
done
if ! swaymsg -t get_tree | jq -e '[.. | objects | select(.name? == "Sidescope")] | length > 0' >/dev/null; then
  echo 'Stripe not found in Sway tree:' >&2
  swaymsg -t get_tree >&2
  tail -50 "$RUNTIME/app.log" >&2
  exit 1
fi
# Headless Sway has no physical keyboard. Keep a virtual keyboard attached
# while testing activation; otherwise GTK never receives active toplevel state.
wtype -d 60000 'abc' > "$RUNTIME/keyboard.log" 2>&1 & keyboard_pid=$!
sleep 0.5
if ! kill -0 "$keyboard_pid" 2>/dev/null; then
  echo 'Virtual keyboard exited before focus tests' >&2; exit 1
fi
# Sample plain header/footer pixels as well as full captures: the focus change must
# come from Sway, affect both ends of the column, and reverse when focus leaves.
assert_focus_transition() {
  local prefix=$1 width=$2 x y height sample_x header_y footer_y point
  read -r x y height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x, .rect.y, .rect.height] | @tsv' | head -1)
  sample_x=$((x + 8))
  header_y=$((y + 40))
  footer_y=$((y + height - 20))
  capture_focus_state() {
    local state=$1
    grim -g "$x,$y ${width}x${height}" "$prefix-$state.png"
    for point in header footer; do
      local sample_y=$header_y
      [[ $point == footer ]] && sample_y=$footer_y
      grim -t ppm -g "$sample_x,$sample_y 1x1" "$RUNTIME/$point-$state.ppm"
    done
  }
  capture_focus_state unfocused
  swaymsg '[title="Sidescope"] focus' >/dev/null
  sleep 0.5
  if ! swaymsg -t get_tree | jq -e '[.. | objects | select(.name? == "Sidescope" and .focused? == true)] | length == 1' >/dev/null; then
    echo 'Sway did not focus Sidescope' >&2; exit 1
  fi
  capture_focus_state focused
  swaymsg '[title="Browser placeholder"] focus' >/dev/null
  sleep 0.5
  capture_focus_state blurred
  for point in header footer; do
    if cmp -s "$RUNTIME/$point-unfocused.ppm" "$RUNTIME/$point-focused.ppm"; then
      echo "${width}px $point did not change when Sway focused the stripe" >&2; tail -20 "$RUNTIME/app.log" >&2; exit 1
    fi
    if ! cmp -s "$RUNTIME/$point-unfocused.ppm" "$RUNTIME/$point-blurred.ppm"; then
      echo "${width}px $point did not return to unfocused after Sway blurred the stripe" >&2; exit 1
    fi
  done
  printf 'Focus transition at %spx: %s-{unfocused,focused,blurred}.png\n' "$width" "$prefix"
}
swaymsg '[title="Sidescope"] focus' >/dev/null
swaymsg 'resize set width 640 px' >/dev/null
swaymsg '[title="Browser placeholder"] focus' >/dev/null
sleep 2
# Sway's tree is the geometry authority; allow a few pixels for window borders.
WIDTH=$(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | .rect.width' | head -1)
if [[ ! "$WIDTH" =~ ^[0-9]+$ ]] || (( WIDTH < 636 || WIDTH > 644 )); then
  echo "Expected a 640px stripe on a 2560px output; Sway reported $WIDTH" >&2
  exit 1
fi
if [[ "${SIDESCOPE_VISUAL_DENSITY_ONLY:-0}" == 1 ]]; then
  source "$ROOT/scripts/visual-density.sh"
  exit
fi
if [[ "${SIDESCOPE_VISUAL_CHARTS_ONLY:-0}" == 1 ]]; then
  source "$ROOT/scripts/visual-charts.sh"
  exit
fi
assert_focus_transition "${CAPTURE%.png}-wide" 640
grim -o HEADLESS-1 "$CAPTURE"
COLUMN="${CAPTURE%.png}-column.png"
grim -g '1920,0 640x1440' "$COLUMN"
printf '1440p screenshot: %s (stripe: %s, actual width: %s)\n' "$CAPTURE" "$COLUMN" "$WIDTH"
file "$CAPTURE" "$COLUMN"
if [[ "${SIDESCOPE_VISUAL_LIVE_STUB:-0}" == 1 ]]; then
  swaymsg '[title="Sidescope"] focus' >/dev/null
  sleep 0.5
  wtype -d 200 'abcr'
  sleep 0.2
  CHECKING="${CAPTURE%.png}-checking.png"
  grim -g '1920,0 640x1440' "$CHECKING"
  sleep 1.4
  CHECKED="${CAPTURE%.png}-checked.png"
  grim -g '1920,0 640x1440' "$CHECKED"
  if cmp -s "$CHECKING" "$CHECKED"; then
    echo 'Live-mode check status did not change after a refresh request' >&2
    exit 1
  fi
  printf 'Refresh states: %s, %s\n' "$CHECKING" "$CHECKED"
fi
# Inspect the newly added dashboard below the provider fold, then return to top.
swaymsg '[title="Sidescope"] focus' >/dev/null
wtype -d 100 'abc' -k Page_Down
sleep 0.5
grim -g '1920,0 640x1440' "${CAPTURE%.png}-dashboard-middle.png"
wtype -d 100 -k Page_Down -k Page_Down -k Page_Down -k Page_Down
sleep 0.5
grim -g '1920,0 640x1440' "${CAPTURE%.png}-dashboard-bottom.png"
wtype -d 100 -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up
sleep 0.5
# Exercise the shortcut guide using a real compositor key event, then dismiss it.
swaymsg '[title="Sidescope"] focus' >/dev/null
sleep 0.5
grim -t ppm -g '2240,720 1x1' "$RUNTIME/focused-center.ppm"
# The first virtual-keyboard events can be lost while Sway attaches the device.
wtype -d 200 'abc?'
sleep 1
HELP="${CAPTURE%.png}-help.png"
grim -g '1920,0 640x1440' "$HELP"
grim -t ppm -g '2240,720 1x1' "$RUNTIME/help-center.ppm"
if cmp -s "$RUNTIME/focused-center.ppm" "$RUNTIME/help-center.ppm"; then
  echo 'Keyboard shortcut overlay did not appear' >&2
  tail -20 "$RUNTIME/app.log" >&2
  exit 1
fi
wtype -d 200 'abc' -k Escape
sleep 0.5
grim -t ppm -g '2240,720 1x1' "$RUNTIME/dismissed-center.ppm"
if ! cmp -s "$RUNTIME/focused-center.ppm" "$RUNTIME/dismissed-center.ppm"; then
  echo 'Escape did not dismiss the shortcut overlay' >&2
  exit 1
fi
printf 'Keyboard shortcut overlay: %s\n' "$HELP"
file "$HELP"
# Keep the earlier narrow condition as a secondary stress test.
swaymsg 'output HEADLESS-1 mode 1280x900' >/dev/null
swaymsg '[title="Sidescope"] focus' >/dev/null
swaymsg 'resize set width 378 px' >/dev/null
swaymsg '[title="Browser placeholder"] focus' >/dev/null
sleep 1
assert_focus_transition "${CAPTURE%.png}-narrow-focus" 378
NARROW="${CAPTURE%.png}-narrow.png"
grim -o HEADLESS-1 "$NARROW"
printf 'Narrow screenshot: %s\n' "$NARROW"
file "$NARROW"
# An explicit provider subset may fit entirely; the all-card and Agy fixtures must scroll.
if [[ -z "${SIDESCOPE_VISUAL_CONFIG:-}" || "${SIDESCOPE_VISUAL_AGY:-0}" == 1 ]]; then
  swaymsg '[title="Sidescope"] focus' >/dev/null
  sleep 0.5
  NARROW_X=$(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | .rect.x' | head -1)
  grim -g "$NARROW_X,0 378x900" "$RUNTIME/narrow-focused.png"
  wtype -d 200 'abcj'
  sleep 0.5
  SCROLLED="${CAPTURE%.png}-scrolled.png"
  grim -g "$NARROW_X,0 378x900" "$SCROLLED"
  if cmp -s "$RUNTIME/narrow-focused.png" "$SCROLLED"; then
    echo 'J did not scroll the narrow provider view' >&2
    exit 1
  fi
  printf 'Keyboard-scrolled narrow screenshot: %s\n' "$SCROLLED"
  file "$SCROLLED"
  if [[ "${SIDESCOPE_VISUAL_AGY:-0}" == 1 ]]; then
    wtype -d 200 'jjjjjj'
    sleep 0.5
    BOTTOM="${CAPTURE%.png}-agy-bottom.png"
    grim -g "$NARROW_X,0 378x900" "$BOTTOM"
    if cmp -s "$SCROLLED" "$BOTTOM"; then
      echo 'Agy card was not reachable by narrow keyboard scrolling' >&2
      exit 1
    fi
    printf 'Agy card at narrow scroll bottom: %s\n' "$BOTTOM"
    file "$BOTTOM"
  fi
fi
# Diagnostic snapshot only: one run is not a performance benchmark.
if [[ -r "/proc/$app_pid/smaps_rollup" ]]; then
  awk '/^Pss:/{print "App PSS snapshot: " $2 " kB"}' "/proc/$app_pid/smaps_rollup"
fi

# Capture the full compact forecast and source footer at narrow width.
NARROW_X=$(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | .rect.x' | head -1)
swaymsg '[title="Sidescope"] focus' >/dev/null
wtype -d 100 -k Page_Down -k Page_Down -k Page_Down
sleep 0.5
grim -g "$NARROW_X,0 378x900" "${CAPTURE%.png}-narrow-dashboard-middle.png"
wtype -d 100 -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down
sleep 0.5
grim -g "$NARROW_X,0 378x900" "${CAPTURE%.png}-narrow-dashboard-bottom.png"
if [[ -n "${SIDESCOPE_VISUAL_GEOMETRY:-}" ]]; then
  cp "$RUNTIME/app.log" "${CAPTURE%.png}-geometry.log"
fi
if grep -Ei 'BreakpointBin.*(minimum|size)|CRITICAL|Gtk-WARNING|Adwaita-WARNING' "$RUNTIME/app.log"; then
  echo 'GTK warning in visual application log' >&2
  exit 1
fi

# Deterministic all-local/environment fixtures: never use personal config or sources.
# Removing AI cards brings graphs into the first viewport for warm-up comparisons.
mkdir -p "$XDG_CONFIG_HOME/sidescope"
printf 'enabled = []\n' > "$XDG_CONFIG_HOME/sidescope/config.toml"
for scenario in clear long-condition theme-picker theme-live live-details one two full rolling missing busy-cpu fahrenheit stale unconfigured high-cores strip-missing strip-fahrenheit strip-live-details; do
  kill "$app_pid"; wait "$app_pid" || :; app_pid=
  swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
  log="$RUNTIME/scenario-$scenario.log"
  if [[ "$scenario" == strip-* ]]; then
    printf 'enabled = []\ntheme = "terminal"\n' > "$XDG_CONFIG_HOME/sidescope/config.toml"
  else
    printf 'enabled = []\n' > "$XDG_CONFIG_HOME/sidescope/config.toml"
  fi
  SIDESCOPE_VISUAL_SCENARIO="$scenario" "$BIN" --report "$ROOT/tests/fixtures/usage.json" > "$log" 2>&1 & app_pid=$!
  for _ in {1..100}; do
    if swaymsg -t get_tree | jq -e '[.. | objects | select(.name? == "Sidescope")] | length == 1' >/dev/null; then break; fi
    if ! kill -0 "$app_pid" 2>/dev/null; then cat "$log" >&2; exit 1; fi
    sleep 0.1
  done
  swaymsg '[title="Sidescope"] focus' >/dev/null
  swaymsg 'resize set width 640 px' >/dev/null
  sleep 0.6
  if ! kill -0 "$app_pid" 2>/dev/null; then
    cp "$log" "${CAPTURE%.png}-scenario-$scenario-geometry.log"
    cat "$log" >&2
    exit 1
  fi
  read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x, .rect.y, .rect.width, .rect.height] | @tsv' | head -1)
  [[ "$width" == 640 ]] || { echo "Scenario $scenario not 640px" >&2; exit 1; }
  prefix="${CAPTURE%.png}-scenario-$scenario"
  grim -g "$x,$y ${width}x${height}" "$prefix-wide.png"
  if grep -q '^AXIS_RECT weather-axis-0 ' "$log"; then
    read -r client_x client_y < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.window_rect.x, .window_rect.y] | @tsv' | head -1)
    read -r first_x first_y first_w first_h < <(awk '$1 == "AXIS_RECT" && $2 == "weather-axis-0" {print $3,$4,$5,$6; exit}' "$log")
    read -r last_x last_y last_w last_h < <(awk '$1 == "AXIS_RECT" && $2 == "weather-axis-2" {print $3,$4,$5,$6; exit}' "$log")
    for endpoint in 0 2; do
      read -r axis_x axis_y axis_w axis_h < <(awk -v name="weather-axis-$endpoint" '$1 == "AXIS_RECT" && $2 == name {print $3,$4,$5,$6; exit}' "$log")
      grim -t ppm -g "$((x + client_x + axis_x)),$((y + client_y + axis_y)) ${axis_w}x${axis_h}" "$RUNTIME/axis.ppm"
      bright=$(tail -n +4 "$RUNTIME/axis.ppm" | od -An -tu1 | awk '{for(i=1;i<=NF;i++) if($i>100) n++} END {print n+0}')
      (( bright > 20 )) || { echo "Missing axis endpoint pixels: $scenario/$endpoint" >&2; exit 1; }
      printf 'Axis endpoint %s/%s: %s bright components\n' "$scenario" "$endpoint" "$bright"
    done
    grim -g "$((x + client_x + first_x)),$((y + client_y + first_y)) $((last_x + last_w - first_x))x${first_h}" "$prefix-axis.png"
  fi
  if [[ "$scenario" == long-condition ]]; then
    swaymsg 'resize set width 512 px' >/dev/null
    sleep 0.5
    grep -Eq 'CONDITION_TIGHT ellipsized=(true|false) tooltip_complete=true' "$log"
    read -r tight_x tight_y tight_w tight_h < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x,.rect.y,.rect.width,.rect.height] | @tsv' | head -1)
    grim -g "$tight_x,$tight_y ${tight_w}x${tight_h}" "$prefix-tight.png"
  fi
  if [[ "$scenario" == theme-picker ]]; then
    sleep 0.4
    grep -q 'THEME_CONTROLS previous_next=true selector_wheel=true scrollable_picker=true' "$log"
    grim -g "$x,$y ${width}x${height}" "$prefix-open.png"
    wtype -k Escape
  fi
  if [[ "$scenario" == theme-live ]]; then
    sleep 6.5
    grep -q 'THEME_PICKER selected=arcade popdown=true' "$log"
    grep -q 'THEME_LIVE_UPDATE tick=3 selected=blueprint local_and_report_preserved=true' "$log"
    grim -g "$x,$y ${width}x${height}" "$prefix-after-updates.png"
  fi
  if [[ "$scenario" == live-details || "$scenario" == strip-live-details ]]; then
    sleep 6.5
    cp "$log" "$prefix-geometry.log"
    if ! kill -0 "$app_pid" 2>/dev/null; then cat "$log" >&2; exit 1; fi
    grep -q 'LIVE_DETAILS_UPDATE tick=3 visible=true focus_preserved=true' "$log"
    grim -g "$x,$y ${width}x${height}" "$prefix-after-updates.png"
    wtype -k Escape
    sleep 0.3
    wtype -k space
    sleep 0.3
    grep -q 'LIVE_DETAILS visible=false updates=3' "$log"
    grep -q 'LIVE_DETAILS visible=true updates=3' "$log"
    grim -g "$x,$y ${width}x${height}" "$prefix-keyboard-reopened.png"
  fi
  if [[ "$scenario" == one ]]; then
    # Fixture gives Details focus; Tab/Shift-Tab exercise normal traversal.
    wtype -d 100 -k Tab -M shift -k Tab -m shift -k space
    sleep 0.3
    grep -q 'HOURLY_DETAILS visible=true' "$log"
    grim -g "$x,$y ${width}x${height}" "$prefix-hourly-details.png"
    wtype -k Escape
  fi
  if [[ "$scenario" == high-cores ]]; then
    wtype -d 50 -k Page_Down -k Page_Down -k Page_Down
    sleep 0.3
    grim -g "$x,$y ${width}x${height}" "$prefix-wide-bottom.png"
  fi
  if [[ "$scenario" == clear || "$scenario" == long-condition || "$scenario" == stale || "$scenario" == missing || "$scenario" == fahrenheit || "$scenario" == high-cores || "$scenario" == strip-* ]]; then
    swaymsg 'output HEADLESS-1 mode 1280x900' >/dev/null
    swaymsg 'resize set width 378 px' >/dev/null
    sleep 0.4
    read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x, .rect.y, .rect.width, .rect.height] | @tsv' | head -1)
    [[ "$width" == 378 ]] || { echo "Scenario $scenario not 378px" >&2; exit 1; }
    grim -g "$x,$y ${width}x${height}" "$prefix-narrow.png"
    wtype -d 50 -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down
    sleep 0.3
    grim -g "$x,$y ${width}x${height}" "$prefix-narrow-bottom.png"
  fi
  cp "$log" "$prefix-geometry.log"
  if ! kill -0 "$app_pid" 2>/dev/null || grep -Ei 'panicked|CRITICAL|Gtk-WARNING|Adwaita-WARNING' "$log"; then
    echo "Visual scenario failed: $scenario" >&2; cat "$log" >&2; exit 1
  fi
  printf 'Synthetic visual scenario: %s\n' "$prefix"
done
if cmp -s "${CAPTURE%.png}-scenario-one-wide.png" "${CAPTURE%.png}-scenario-two-wide.png"; then
  echo 'Warm-up snapshots must differ' >&2; exit 1
fi

# The registry order is part of the browsing contract; real keys select each theme.
# Each theme's curated CPU/weather/telemetry layout only renders in Comfortable,
# so pin that here; Screen-fit across all 21 themes is the density suite's job.
printf 'enabled = []\ndensity = "comfortable"\n' > "$XDG_CONFIG_HOME/sidescope/config.toml"
kill "$app_pid"; wait "$app_pid" || :; app_pid=
swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
log="$RUNTIME/themes.log"
cp "$XDG_CONFIG_HOME/sidescope/config.toml" "$RUNTIME/theme-config-before.toml"
SIDESCOPE_VISUAL_SCENARIO=busy-cpu "$BIN" --report "$ROOT/tests/fixtures/usage.json" > "$log" 2>&1 & app_pid=$!
for _ in {1..100}; do
  if swaymsg -t get_tree | jq -e '[.. | objects | select(.name? == "Sidescope")] | length == 1' >/dev/null; then break; fi
  sleep 0.1
done
swaymsg '[title="Sidescope"] focus' >/dev/null
wtype -d 100 'abc'
for theme in original paper terminal blueprint editorial contrast retro cloud minimal nord solar rose ocean ember orchid forest cobalt sand mint noir arcade; do
  swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
  swaymsg 'resize set width 640 px' >/dev/null
  wtype -d 30 -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up
  sleep 0.35
  read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x,.rect.y,.rect.width,.rect.height] | @tsv' | head -1)
  grim -g "$x,$y ${width}x${height}" "${CAPTURE%.png}-theme-$theme-wide.png"
  if [[ "$theme" == paper || "$theme" == terminal ]]; then
    wtype '?'
    sleep 0.25
    grim -g "$x,$y ${width}x${height}" "${CAPTURE%.png}-theme-$theme-help.png"
    wtype -k Escape
    sleep 0.15
  fi
  swaymsg 'output HEADLESS-1 mode 1280x900' >/dev/null
  swaymsg 'resize set width 378 px' >/dev/null
  wtype -d 30 -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up
  sleep 0.35
  read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x,.rect.y,.rect.width,.rect.height] | @tsv' | head -1)
  grim -g "$x,$y ${width}x${height}" "${CAPTURE%.png}-theme-$theme-cpu-narrow.png"
  wtype -d 30 -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down
  sleep 0.35
  read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x,.rect.y,.rect.width,.rect.height] | @tsv' | head -1)
  grim -g "$x,$y ${width}x${height}" "${CAPTURE%.png}-theme-$theme-narrow.png"
  wtype ']'
  sleep 0.15
  kill -0 "$app_pid"
done
# After a complete 21-choice cycle, the original is selected again. Reverse wraps.
grep -q '^THEME_SELECTED original$' "$log"
wtype '['; sleep 0.15
grep -q '^THEME_SELECTED arcade$' "$log"
wtype ']'; sleep 0.15
cmp "$RUNTIME/theme-config-before.toml" "$XDG_CONFIG_HOME/sidescope/config.toml"
cp "$log" "${CAPTURE%.png}-themes-geometry.log"
if grep -Ei 'panicked|CRITICAL|Gtk-WARNING|Adwaita-WARNING' "$log"; then cat "$log" >&2; exit 1; fi
printf 'All 21 themes captured at 640px and 378px; runtime keyboard cycle and reverse wrap verified.\n'
source "$ROOT/scripts/visual-density.sh"
source "$ROOT/scripts/visual-charts.sh"
