#!/usr/bin/env bash
# Install this checkout for the current user, or remove it again.
#
#   scripts/deploy.sh install     build the release binary and install it
#   scripts/deploy.sh uninstall   remove the installed binary
#
# The destination is a user-level binary directory: SIDESCOPE_BIN_DIR when
# set, otherwise $XDG_BIN_HOME, otherwise ~/.local/bin. Nothing here needs root,
# writes into a system prefix, or edits the user's own configuration.
#
# SIDESCOPE_DEPLOY_BINARY installs that executable instead of building one,
# which is how the test and a packager use this script.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
bin_dir="${SIDESCOPE_BIN_DIR:-${XDG_BIN_HOME:-$HOME/.local/bin}}"
binary="$bin_dir/sidescope"

case "${1:-}" in
  install)
    source_binary="${SIDESCOPE_DEPLOY_BINARY:-}"
    if [[ -z "$source_binary" ]]; then
      target_dir=$("$ROOT/scripts/native-env.sh" bash -c 'printf %s "${CARGO_TARGET_DIR:-$PWD/target}"')
      "$ROOT/scripts/native-env.sh" cargo build --release
      source_binary="$target_dir/release/sidescope"
    fi
    if [[ ! -x "$source_binary" ]]; then
      echo "deploy: no executable to install at $source_binary" >&2
      exit 1
    fi
    mkdir -p "$bin_dir"
    # Stage beside the destination and move it into place, so a stripe launching
    # from a compositor binding never opens a half-written binary.
    staging="$binary.new.$$"
    trap 'rm -f "${staging:-}"' EXIT
    cp "$source_binary" "$staging"
    chmod 755 "$staging"
    mv -f "$staging" "$binary"
    revision=$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || printf 'unknown')
    printf 'Installed %s\n' "$binary"
    printf '  from %s (%s)\n' "$source_binary" "$revision"
    printf '  launch it from your compositor, for example: exec %s --live\n' "$binary"
    printf '    (a bare launch shows the bundled sample data; --live reads your ai-usagebar report)\n'
    case ":$PATH:" in
      *":$bin_dir:"*) ;;
      *)
        printf 'Note: %s is not on your PATH, so use the full path above or add it.\n' "$bin_dir" >&2
        ;;
    esac
    ;;
  uninstall)
    if [[ -f "$binary" ]]; then
      rm -f "$binary"
      printf 'Removed %s\n' "$binary"
    else
      printf 'Nothing installed at %s\n' "$binary"
    fi
    ;;
  *)
    echo 'usage: deploy.sh install|uninstall' >&2
    exit 2
    ;;
esac
