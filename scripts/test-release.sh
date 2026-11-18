#!/usr/bin/env bash
# Exercise release in an isolated repository with a temporary bare origin.
# Never tag or push the workspace.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin"
cat > "$TMP/bin/just" <<'MOCK'
#!/usr/bin/env bash
if [[ "$1" == check-ci ]]; then
  printf 'checked\n' >> "$CHECK_LOG"
  exit "${CHECK_EXIT:-0}"
fi
exec "$REAL_JUST" "$@"
MOCK
chmod +x "$TMP/bin/just"
REAL_JUST=$(command -v just)
# mise shims search the temporary repo for tool configuration; use its resolved binary.
if command -v mise >/dev/null 2>&1; then
  installed=$(mise where just 2>/dev/null || :)
  if [[ -x "$installed/just" ]]; then REAL_JUST="$installed/just"; fi
fi
export REAL_JUST CHECK_LOG="$TMP/checked" PATH="$TMP/bin:$PATH"
git init -q --bare "$TMP/origin.git"
git init -q -b main "$TMP/repo"
cd "$TMP/repo"
git config user.name 'Release Test'
git config user.email 'release-test@example.invalid'
cp "$ROOT/Justfile" .
printf '[package]\nname = "sidescope"\nversion = "0.1.0"\n' > Cargo.toml
git add Justfile Cargo.toml
git commit -qm 'test fixture'
git remote add origin "$TMP/origin.git"
git push -q -u origin main
baseline=$(git status --porcelain --untracked-files=all)
head=$(git rev-parse HEAD)
expect_failure() {
  local message=$1
  shift
  if "$@" > "$TMP/output" 2>&1; then
    echo "Release unexpectedly passed: $message" >&2
    exit 1
  fi
  if ! grep -Fq "$message" "$TMP/output"; then
    cat "$TMP/output" >&2
    echo "Missing rejection: $message" >&2
    exit 1
  fi
}
expect_unchanged() {
  [[ "$(git status --porcelain --untracked-files=all)" == "$baseline" ]]
  [[ "$(git rev-parse HEAD)" == "$head" ]]
  [[ -z "$(git tag --list)" ]]
  [[ -z "$(git --git-dir="$TMP/origin.git" tag --list)" ]]
}
expect_failure 'Expected a version' just release '0.1.0; touch unsafe'
expect_failure 'Version 0.2.0 does not match' just release 0.2.0
printf 'dirty\n' > scratch
expect_failure 'Working tree is not clean' just release 0.1.0
rm scratch
expect_unchanged
[[ -z "$(just last-release)" ]]
git tag v0.1.0
expect_failure 'Tag v0.1.0 already exists locally' just release 0.1.0
[[ "$(just last-release)" == v0.1.0 ]]
git tag -d v0.1.0 >/dev/null
git remote remove origin
expect_failure 'No origin remote configured' just release 0.1.0
git remote add origin "$TMP/origin.git"
git fetch -q origin main
git branch --set-upstream-to=origin/main main >/dev/null
expect_unchanged
CHECK_EXIT=1 expect_failure 'recipe `release` failed' just release 0.1.0
[[ "$(wc -l < "$CHECK_LOG")" -eq 1 ]]
expect_unchanged
# A tag already on the remote must not be reused, even if it is absent locally.
git tag v0.1.0
git push -q origin refs/tags/v0.1.0
git tag -d v0.1.0 >/dev/null
expect_failure 'Tag v0.1.0 already exists on origin' just release 0.1.0
git push -q origin :refs/tags/v0.1.0
expect_unchanged
just release 0.1.0 > "$TMP/output"
grep -Fq 'Published tag v0.1.0' "$TMP/output"
[[ "$(wc -l < "$CHECK_LOG")" -eq 2 ]]
[[ "$(git tag --list)" == v0.1.0 ]]
[[ "$(git --git-dir="$TMP/origin.git" tag --list)" == v0.1.0 ]]
[[ "$(git rev-parse HEAD)" == "$head" ]]
[[ "$(git status --porcelain --untracked-files=all)" == "$baseline" ]]
printf 'release rejection and isolated tag-push checks: passed\n'
