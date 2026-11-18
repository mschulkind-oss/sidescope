#!/usr/bin/env bash
# One entry point for native builds: isolated Nix artifacts in the jail,
# system libraries and a separate Cargo target on the host.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
if [[ $# -eq 0 ]]; then
  echo 'native-env: expected a command' >&2
  exit 2
fi
if [[ -n "${IN_NIX_SHELL:-}" ]]; then
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/nix}"
  exec "$@"
fi
if [[ -n "${YOLO_HOST_DIR:-}" ]]; then
  if ! command -v nix >/dev/null 2>&1; then
    echo 'native-env: this jail needs Nix to supply GTK development libraries' >&2
    exit 1
  fi
  exec nix develop --command "$@"
fi
export CARGO_TARGET_DIR="$ROOT/target/host"
exec "$@"
