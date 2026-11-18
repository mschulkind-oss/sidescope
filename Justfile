default:
    @just --list

setup:
    ./scripts/native-env.sh cargo fetch

doctor:
    ./scripts/native-env.sh bash scripts/doctor.sh

doctor-visual:
    ./scripts/native-env.sh bash scripts/doctor.sh --visual

build:
    ./scripts/native-env.sh bash scripts/doctor.sh --build
    ./scripts/native-env.sh cargo build

dev:
    ./scripts/native-env.sh bash scripts/doctor.sh --live
    ./scripts/native-env.sh bash scripts/dev-watch.sh

demo:
    ./scripts/native-env.sh bash scripts/doctor.sh --build
    ./scripts/native-env.sh cargo run -- --report tests/fixtures/usage.json

run *args:
    ./scripts/native-env.sh cargo run -- {{ args }}

check: format lint test

check-ci: lint-ci test

format:
    ./scripts/native-env.sh cargo fmt

lint:
    ./scripts/native-env.sh cargo clippy --all-targets -- -D warnings

lint-ci:
    ./scripts/native-env.sh cargo fmt --check
    ./scripts/native-env.sh cargo clippy --all-targets -- -D warnings

test:
    ./scripts/native-env.sh cargo test
    bash scripts/test-dev-tools.sh
    bash scripts/test-dev-watch.sh
    bash scripts/test-deploy.sh
    bash scripts/test-release.sh

test-visual:
    ./scripts/native-env.sh bash scripts/doctor.sh --visual
    ./scripts/native-env.sh cargo build
    ./scripts/native-env.sh bash scripts/visual-test.sh

# Install this checkout for the current user: a release binary in $XDG_BIN_HOME
# or ~/.local/bin. No root, no system prefix, no edits to your configuration.
# `just release <version>` is the separate tag-cutting path.
deploy:
    ./scripts/native-env.sh bash scripts/doctor.sh --build
    bash scripts/deploy.sh install

uninstall:
    bash scripts/deploy.sh uninstall

# The only supported tag-cutting path; invoke on the host after publication review.
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    version={{ quote(version) }}
    if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
      echo 'Expected a version like 0.1.0 (without the v prefix).' >&2
      exit 2
    fi
    tag="v$version"
    if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
      echo 'Working tree is not clean; commit or remove changes before release.' >&2
      exit 1
    fi
    package_version=$(awk -F '"' '/^\[package\]/{package=1; next} /^\[/{package=0} package && /^version = /{print $2; exit}' Cargo.toml)
    if [[ "$version" != "$package_version" ]]; then
      echo "Version $version does not match Cargo.toml package version $package_version." >&2
      exit 1
    fi
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
      echo "Tag $tag already exists locally." >&2
      exit 1
    fi
    if ! git remote get-url origin >/dev/null 2>&1; then
      echo 'No origin remote configured.' >&2
      exit 1
    fi
    branch=$(git symbolic-ref --quiet --short HEAD) || { echo 'Detached HEAD; release from a branch.' >&2; exit 1; }
    upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null) || { echo 'No upstream branch configured.' >&2; exit 1; }
    if [[ "$upstream" != "origin/$branch" || "$(git rev-parse HEAD)" != "$(git rev-parse '@{upstream}')" ]]; then
      echo 'Branch must match its origin upstream before release.' >&2
      exit 1
    fi
    remote_head=$(git ls-remote --exit-code origin "refs/heads/$branch" | cut -f1) || {
      echo 'Cannot read the origin branch; check network and permissions.' >&2
      exit 1
    }
    if [[ "$remote_head" != "$(git rev-parse HEAD)" ]]; then
      echo 'Origin moved since the last fetch; fetch and reconcile before release.' >&2
      exit 1
    fi
    if [[ -n "$(git ls-remote --tags origin "refs/tags/$tag" "refs/tags/$tag^{}")" ]]; then
      echo "Tag $tag already exists on origin." >&2
      exit 1
    fi
    just check-ci
    git tag -a "$tag" -m "$tag"
    if ! git push origin "refs/tags/$tag"; then
      echo "Tag $tag exists locally but was not pushed; resolve the failure before retrying." >&2
      exit 1
    fi
    printf 'Published tag %s from %s; the tag workflow will create a draft release.\n' "$tag" "$branch"

# Local tags only; does not contact the remote or assume a first release exists.
last-release:
    @git tag --list 'v[0-9]*' --sort=-version:refname | sed -n '1p'

done:
    just check-ci
    test -z "$(git status --porcelain)"
