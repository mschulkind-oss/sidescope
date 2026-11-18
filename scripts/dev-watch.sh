#!/usr/bin/env bash
# Rebuild a live window after a full second without source edits.
set -euo pipefail
# The jail blocks `find` (recursive-search guard); its shim both drowns the
# watcher in warnings and hides src changes, so walk src with globstar.
shopt -s globstar nullglob
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
TARGET=${CARGO_TARGET_DIR:-"$ROOT/target"}
app_pid=
cleanup() {
  if [[ -n "$app_pid" ]]; then
    kill "$app_pid" 2>/dev/null || :
    wait "$app_pid" 2>/dev/null || :
    app_pid=
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

snapshot() {
  {
    local path
    for path in src/**/*; do
      if [[ -f "$path" ]]; then stat -c '%n %y %s' "$path"; fi
    done
    for path in Cargo.toml Cargo.lock build.rs; do
      if [[ -f "$path" ]]; then stat -c '%n %y %s' "$path"; fi
    done
  } | LC_ALL=C sort
}

rebuild() {
  echo 'Building Sidescope…'
  if ! cargo build; then
    echo 'Build failed; keeping the previous window open until the next edit.' >&2
    return
  fi
  cleanup
  "$TARGET/debug/sidescope" --live & app_pid=$!
  echo 'Live window restarted. Watching src/, Cargo.toml, Cargo.lock and build.rs (1s cooldown).'
}

last=$(snapshot)
rebuild
deadline=0
while :; do
  sleep 0.2
  current=$(snapshot)
  now=$(date +%s%3N)
  if [[ "$current" != "$last" ]]; then
    last=$current
    deadline=$((now + 1000))
  fi
  if (( deadline > 0 && now >= deadline )); then
    deadline=0
    rebuild
  fi
done
