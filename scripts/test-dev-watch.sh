#!/usr/bin/env bash
# Exercise debounced rebuilds and child cleanup without a Wayland session.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
just --show dev | grep -Fq './scripts/native-env.sh bash scripts/dev-watch.sh'
TMP=$(mktemp -d)
watch_pid=
cleanup() {
  if [[ -n "$watch_pid" ]]; then
    kill "$watch_pid" 2>/dev/null || :
    wait "$watch_pid" 2>/dev/null || :
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT
mkdir -p "$TMP/project/scripts" "$TMP/project/src" "$TMP/bin" "$TMP/target"
cp "$ROOT/scripts/dev-watch.sh" "$TMP/project/scripts/dev-watch.sh"
printf 'initial\n' > "$TMP/project/src/main.rs"
printf '[package]\n' > "$TMP/project/Cargo.toml"
cat > "$TMP/bin/cargo" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == build ]]
printf 'build\n' >> "$DEV_LOG"
if [[ -e "$DEV_FAIL_BUILD" ]]; then exit 1; fi
mkdir -p "$CARGO_TARGET_DIR/debug"
cat > "$CARGO_TARGET_DIR/debug/sidescope" <<'APP'
#!/usr/bin/env bash
printf 'start %s %s\n' "$BASHPID" "$1" >> "$DEV_LOG"
trap 'printf "stop %s\n" "$BASHPID" >> "$DEV_LOG"; exit 0' TERM
while :; do sleep 0.1 & wait $! || :; done
APP
chmod +x "$CARGO_TARGET_DIR/debug/sidescope"
MOCK
chmod +x "$TMP/bin/cargo"
# `find` is blocked inside the jail; the watcher must not call it. This mock
# fails loudly so a reintroduced dependency shows up as a missing src rebuild.
cat > "$TMP/bin/find" <<'MOCK'
#!/usr/bin/env bash
printf 'find %s\n' "$*" >> "$DEV_LOG"
exit 1
MOCK
chmod +x "$TMP/bin/find"
export CARGO_TARGET_DIR="$TMP/target" DEV_LOG="$TMP/events" DEV_FAIL_BUILD="$TMP/fail"
export PATH="$TMP/bin:$PATH"
cd "$TMP/project"
bash scripts/dev-watch.sh > "$TMP/output" 2>&1 & watch_pid=$!
wait_for() {
  local pattern=$1 expected=$2
  for _ in {1..100}; do
    if [[ $(grep -c "$pattern" "$DEV_LOG" 2>/dev/null || :) == "$expected" ]]; then return; fi
    sleep 0.1
  done
  printf 'Timed out waiting for %s = %s\n' "$pattern" "$expected" >&2
  test ! -e "$DEV_LOG" || grep -n . "$DEV_LOG" >&2
  test ! -e "$TMP/output" || grep -n . "$TMP/output" >&2
  exit 1
}
wait_for '^start .* --live$' 1
printf 'edit-one\n' >> src/main.rs
sleep 0.3
printf 'edit-two\n' >> src/main.rs
sleep 0.4
[[ $(grep -c '^build$' "$DEV_LOG") == 1 ]]
wait_for '^build$' 2
wait_for '^start .* --live$' 2
[[ $(grep -c '^stop ' "$DEV_LOG") == 1 ]]
touch "$DEV_FAIL_BUILD"
printf 'invalid\n' >> src/main.rs
wait_for '^build$' 3
sleep 0.3
[[ $(grep -c '^start ' "$DEV_LOG") == 2 ]]
[[ $(grep -c '^stop ' "$DEV_LOG") == 1 ]]
rm "$DEV_FAIL_BUILD"
printf 'fixed\n' >> src/main.rs
wait_for '^build$' 4
wait_for '^start .* --live$' 3
printf 'new style\n' > src/style.css
wait_for '^build$' 5
wait_for '^start .* --live$' 4
kill "$watch_pid"
wait "$watch_pid" 2>/dev/null || :
watch_pid=
wait_for '^stop ' 4
[[ $(grep -c '^find ' "$DEV_LOG" 2>/dev/null || :) == 0 ]]
printf 'debounced reload without find, failed-build retention and child cleanup: passed\n'
