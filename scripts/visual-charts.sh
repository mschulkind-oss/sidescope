#!/usr/bin/env bash
# Sourced by visual-test.sh inside its private compositor/session. Every command
# here is a synthetic fixture; no real usage ledger or account is touched.
chart_geometry() {
  read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x,.rect.y,.rect.width,.rect.height] | @tsv' | head -1)
}

chart_launch() { # log, then the command line
  local log=$1; shift
  "$@" > "$log" 2>&1 & app_pid=$!
  for _ in {1..100}; do
    if swaymsg -t get_tree | jq -e '[.. | objects | select(.name? == "Sidescope")] | length == 1' >/dev/null; then break; fi
    kill -0 "$app_pid" || { cat "$log" >&2; return 1; }
    sleep 0.1
  done
  swaymsg '[title="Sidescope"] focus' >/dev/null
  swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
  swaymsg 'resize set width 640 px' >/dev/null
  sleep 1.5
}

chart_stop() {
  if [[ -n "${app_pid:-}" ]]; then kill "$app_pid" 2>/dev/null || :; wait "$app_pid" 2>/dev/null || :; app_pid=; fi
}

chart_capture_pair() { # prefix: top and bottom at 640px, then 378px
  local prefix=$1
  swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
  swaymsg 'resize set width 640 px' >/dev/null
  sleep 1
  wtype -d 30 -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up
  sleep 0.4
  chart_geometry
  grim -g "$x,$y ${width}x${height}" "$prefix-wide-top.png"
  wtype -d 30 -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down
  sleep 0.4
  grim -g "$x,$y ${width}x${height}" "$prefix-wide-bottom.png"
  swaymsg 'output HEADLESS-1 mode 1280x900' >/dev/null
  swaymsg 'resize set width 378 px' >/dev/null
  sleep 1.2
  chart_geometry
  wtype -d 30 -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up
  sleep 0.3
  local shot=0
  grim -g "$x,$y ${width}x${height}" "$prefix-narrow-0.png"
  for shot in 1 2 3 4 5 6; do
    wtype -d 30 -k Page_Down -k Page_Down
    sleep 0.3
    grim -g "$x,$y ${width}x${height}" "$prefix-narrow-$shot.png"
  done
}

run_chart_suite() {
  local scenario prefix log
  local base="${CAPTURE%.png}-charts"
  mkdir -p "$XDG_CONFIG_HOME/sidescope"
  printf 'enabled = []\ndensity = "comfortable"\n' > "$XDG_CONFIG_HOME/sidescope/config.toml"
  for scenario in charts-full charts-partial charts-stale charts-failed charts-unavailable charts-dst charts-many-series; do
    chart_stop
    swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
    log="$RUNTIME/charts-$scenario.log"
    prefix="$base-${scenario#charts-}"
    SIDESCOPE_VISUAL_SCENARIO="$scenario" chart_launch "$log" "$BIN" --report "$ROOT/tests/fixtures/usage.json"
    wtype -d 200 'abc'
    chart_capture_pair "$prefix"
    kill -0 "$app_pid" || { cat "$log" >&2; return 1; }
    if grep -Ei 'panicked|CRITICAL|Gtk-WARNING|Adwaita-WARNING' "$log"; then cat "$log" >&2; return 1; fi
    printf 'Chart scenario captured: %s-*.png\n' "$prefix"
  done

  # A real Enter key opens Details; a snapshot replaced underneath keeps it open.
  chart_stop
  log="$RUNTIME/charts-details.log"
  GTK_A11Y=test SIDESCOPE_VISUAL_SCENARIO=charts-details chart_launch "$log" "$BIN" --report "$ROOT/tests/fixtures/usage.json"
  for _ in {1..40}; do grep -q 'CHART_FOCUS plot_focused=true' "$log" && break; sleep 0.25; done
  grep -q 'CHART_FOCUS plot_focused=true' "$log"
  grep -q 'CHART_A11Y plot_label=true card_label=true' "$log"
  # The virtual keyboard can drop the first events after a window maps; resend
  # until the app reports the popover, never assuming one attempt landed.
  for _ in {1..4}; do
    wtype -d 200 'abc' -k Return
    for _ in {1..10}; do grep -q 'CHART_DETAILS visible=true' "$log" && break; sleep 0.25; done
    grep -q 'CHART_DETAILS visible=true' "$log" && break
  done
  for _ in {1..40}; do grep -q 'CHART_DETAILS_UPDATE' "$log" && break; sleep 0.25; done
  grep -q 'CHART_DETAILS visible=true' "$log"
  grep -q 'CHART_DETAILS_UPDATE visible=true same_widgets=true' "$log"
  chart_geometry
  grim -g "$x,$y ${width}x${height}" "$base-details-open.png"
  grim -o HEADLESS-1 "$base-details-open-output.png"
  wtype -k Escape
  sleep 0.5
  grep -q 'CHART_DETAILS visible=false' "$log"
  if grep -Ei 'panicked|CRITICAL|Gtk-WARNING|Adwaita-WARNING' "$log"; then cat "$log" >&2; return 1; fi
  printf 'Chart Details keyboard and in-place update verified: %s-details-open.png\n' "$base"

  # Live: real synthetic commands, a generic columns profile and a hung source.
  chart_stop
  local hang="$RUNTIME/hang"
  mkdir -p "$hang"
  local ex="$ROOT/examples/charts"
  {
    printf 'enabled = []\ndensity = "comfortable"\n'
    cat <<TOML
[[chart_sources]]
id = "spend"
command = ["$ex/synthetic-spend.sh", "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_sources]]
id = "latency"
command = ["$ex/synthetic-latency.sh", "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_sources]]
id = "hung"
command = ["$ROOT/tests/fixtures/charts/runtime/hang.sh", "$hang"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"
timeout_seconds = 60

[[chart_cards]]
id = "spend-line"
source = "spend"
kind = "time-series"
metric = "amount"
group_by = ["project"]

[[chart_cards]]
id = "spend-ranking"
source = "spend"
kind = "ranked-bars"
metric = "amount"
group_by = ["project"]

[[chart_cards]]
id = "latency-line"
source = "latency"
kind = "time-series"
metric = "latency_ms"
group_by = ["service"]

[[chart_cards]]
id = "hung-card"
source = "hung"
kind = "time-series"
metric = "amount"
title = "Hung source"

TOML
    sed 's#"agent-activity", "ledger", "series"#"'"$ex"'/fake-ledger.sh", "ledger", "series"#' "$ex/agent-activity.toml"
  } > "$XDG_CONFIG_HOME/sidescope/config.toml"
  cp "$XDG_CONFIG_HOME/sidescope/config.toml" "$RUNTIME/chart-config-before.toml"
  log="$RUNTIME/charts-live.log"
  chart_launch "$log" "$BIN" --live
  for _ in {1..60}; do
    if grep -q 'CHART_SOURCE id=spend dataset=true' "$log" \
      && grep -q 'CHART_SOURCE id=latency dataset=true' "$log" \
      && grep -q 'CHART_SOURCE id=recent dataset=true' "$log" \
      && grep -q 'CHART_SOURCE id=daily dataset=true' "$log"; then break; fi
    sleep 0.25
  done
  for id in spend latency recent daily; do grep -q "CHART_SOURCE id=$id dataset=true error=none" "$log"; done
  swaymsg '[title="Sidescope"] focus' >/dev/null
  wtype -d 200 'abc'
  chart_capture_pair "$base-live"
  # The hung command never blocks GTK: a real key still scrolls the column.
  chart_geometry
  wtype -d 30 -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up -k Page_Up
  sleep 0.4
  grim -g "$x,$y ${width}x${height}" "$RUNTIME/live-before-j.png"
  wtype -d 100 'jjjj'
  sleep 0.5
  grim -g "$x,$y ${width}x${height}" "$RUNTIME/live-after-j.png"
  if cmp -s "$RUNTIME/live-before-j.png" "$RUNTIME/live-after-j.png"; then
    echo 'J did not scroll while a chart source was hung' >&2; return 1
  fi
  [[ -s "$hang/child" && -s "$hang/descendant" ]]
  local child descendant
  child=$(< "$hang/child"); descendant=$(< "$hang/descendant")
  kill -0 "$child" && kill -0 "$descendant"
  # SIGTERM stops the stripe and every process group it started.
  kill -TERM "$app_pid"
  for _ in {1..50}; do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.1; done
  if kill -0 "$app_pid" 2>/dev/null; then echo 'SIGTERM did not stop the stripe' >&2; return 1; fi
  wait "$app_pid" || :
  app_pid=
  sleep 0.3
  if kill -0 "$child" 2>/dev/null || kill -0 "$descendant" 2>/dev/null; then
    echo "Hung chart source survived SIGTERM: child=$child descendant=$descendant" >&2; return 1
  fi
  cmp "$RUNTIME/chart-config-before.toml" "$XDG_CONFIG_HOME/sidescope/config.toml"
  if grep -Ei 'panicked|CRITICAL|Gtk-WARNING|Adwaita-WARNING' "$log"; then cat "$log" >&2; return 1; fi
  cp "$log" "$base-live-geometry.log"
  printf 'Live charts verified: spend/latency/columns profile loaded, hung source never blocked GTK, SIGTERM left no orphan (child %s, descendant %s)\n' "$child" "$descendant"
}
run_chart_suite
