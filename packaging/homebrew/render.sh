#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <version-without-v> <sha256> <output-path>" >&2
  exit 2
fi

version=$1
sha256=$2
output=$3
template_dir=$(cd "$(dirname "$0")" && pwd)

sed \
  -e "s/version \"[^\"]*\"/version \"$version\"/" \
  -e "s/v[0-9][0-9.]*\\/sidescope-v[0-9][0-9.]*-ubuntu24.04-x86_64.tar.gz/v$version\\/sidescope-v$version-ubuntu24.04-x86_64.tar.gz/" \
  -e "s/REPLACE_WITH_RELEASE_TARBALL_SHA256/$sha256/" \
  "$template_dir/sidescope.rb" > "$output"
