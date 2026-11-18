#!/usr/bin/env bash
# Verify the user-level install path without touching the real ~/.local.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
bin_dir="$work/bin"
source_binary="$work/sidescope"
printf '#!/bin/sh\nprintf "sidescope test double\\n"\n' > "$source_binary"
chmod 755 "$source_binary"

install_once() {
  SIDESCOPE_BIN_DIR="$bin_dir" SIDESCOPE_DEPLOY_BINARY="$source_binary" \
    bash "$ROOT/scripts/deploy.sh" install
}

install_once > "$work/first.log" 2>&1
[[ -x "$bin_dir/sidescope" ]] || {
  echo 'deploy did not install an executable' >&2
  cat "$work/first.log" >&2
  exit 1
}
cmp -s "$bin_dir/sidescope" "$source_binary" || {
  echo 'installed binary differs from the one it was given' >&2
  exit 1
}
grep -q 'is not on your PATH' "$work/first.log" || {
  echo 'deploy did not warn about a binary directory outside PATH' >&2
  cat "$work/first.log" >&2
  exit 1
}

# Re-installing replaces the binary and leaves no staging file behind.
printf '#!/bin/sh\nprintf "second build\\n"\n' > "$source_binary"
install_once > "$work/second.log" 2>&1
grep -q 'second build' "$bin_dir/sidescope" || {
  echo 'deploy did not replace an existing installation' >&2
  exit 1
}
for stray in "$bin_dir"/sidescope.new.*; do
  [[ -e "$stray" ]] || continue
  echo "deploy left a staging file behind: $stray" >&2
  exit 1
done

# A directory outside PATH is reported, but the install still lands.
SIDESCOPE_BIN_DIR="$bin_dir" bash "$ROOT/scripts/deploy.sh" uninstall > "$work/uninstall.log" 2>&1
[[ ! -e "$bin_dir/sidescope" ]] || {
  echo 'uninstall left the binary in place' >&2
  cat "$work/uninstall.log" >&2
  exit 1
}
SIDESCOPE_BIN_DIR="$bin_dir" bash "$ROOT/scripts/deploy.sh" uninstall >> "$work/uninstall.log" 2>&1
grep -q 'Nothing installed' "$work/uninstall.log" || {
  echo 'uninstall was not idempotent' >&2
  cat "$work/uninstall.log" >&2
  exit 1
}

# A missing source and an unknown action both fail rather than reporting success.
if SIDESCOPE_BIN_DIR="$bin_dir" SIDESCOPE_DEPLOY_BINARY="$work/absent" \
  bash "$ROOT/scripts/deploy.sh" install > "$work/absent.log" 2>&1; then
  echo 'deploy accepted a source binary that does not exist' >&2
  exit 1
fi
if bash "$ROOT/scripts/deploy.sh" frobnicate > "$work/usage.log" 2>&1; then
  echo 'deploy accepted an unknown action' >&2
  exit 1
fi
grep -q 'usage: deploy.sh install|uninstall' "$work/usage.log" || {
  echo 'deploy did not explain its usage on an unknown action' >&2
  cat "$work/usage.log" >&2
  exit 1
}

printf 'user-level deploy installs, replaces, warns and removes the binary: passed\n'
