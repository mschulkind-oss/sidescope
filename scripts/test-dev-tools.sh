#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# The host side must not inherit a Nix target directory from an old shell.
HOST_OUTPUT=$(env -u YOLO_HOST_DIR -u IN_NIX_SHELL CARGO_TARGET_DIR="$TMP/shared" \
  bash scripts/native-env.sh bash -c 'printf "%s" "$CARGO_TARGET_DIR"')
[[ "$HOST_OUTPUT" == "$ROOT/target/host" ]]

# Already inside a Nix shell: keep its target and do not enter Nix again.
NIX_OUTPUT=$(IN_NIX_SHELL=impure YOLO_HOST_DIR=/fake CARGO_TARGET_DIR="$TMP/nix" \
  bash scripts/native-env.sh bash -c 'printf "%s" "$CARGO_TARGET_DIR"')
[[ "$NIX_OUTPUT" == "$TMP/nix" ]]

# The jail side delegates to Nix, not to host pkg-config or a shared target.
mkdir "$TMP/bin"
cat > "$TMP/bin/nix" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$MOCK_LOG"
MOCK
chmod +x "$TMP/bin/nix"
MOCK_LOG="$TMP/call" PATH="$TMP/bin:$PATH" YOLO_HOST_DIR=/fake \
  env -u IN_NIX_SHELL bash scripts/native-env.sh cargo build
[[ "$(< "$TMP/call")" == 'develop --command cargo build' ]]

# A host without native dependencies must receive a diagnostic, not a silent Nix fallback.
mkdir "$TMP/empty"
if env -u YOLO_HOST_DIR -u IN_NIX_SHELL PATH="$TMP/empty" \
    "$BASH" scripts/doctor.sh --build > "$TMP/doctor-output" 2>&1; then
  echo 'doctor unexpectedly passed without the native build toolchain' >&2
  exit 1
fi
grep -q 'missing' "$TMP/doctor-output"
if env -u YOLO_HOST_DIR -u IN_NIX_SHELL PATH="$TMP/empty" \
    "$BASH" scripts/doctor.sh --live > "$TMP/live-output" 2>&1; then
  echo 'live doctor unexpectedly passed without ai-usagebar' >&2
  exit 1
fi
grep -q 'missing ai-usagebar' "$TMP/live-output"
printf 'native environment routing and dependency diagnostics: passed\n'
