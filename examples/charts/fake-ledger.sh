#!/bin/sh
# Prints a synthetic ledger-shaped `series` answer from tests/fixtures/charts/ledger/
# so agent-activity.toml can run end to end without the real CLI or any ledger:
# swap the profile's `command` for ["/path/to/fake-ledger.sh", "--bucket", "minute", ...].
# It ignores --from/--to/--timezone and picks the answer by --bucket and --group-by.
# Usage: fake-ledger.sh [ledger series flags...]
set -eu
bucket=day; group=workspace
while [ "$#" -gt 0 ]; do
  case "$1" in
    --bucket) bucket="$2"; shift 2 ;;
    --group-by) group="$2"; shift 2 ;;
    ledger|series) shift ;;
    --format|--layout|--from|--to|--timezone|--workspace|--profile|--timeout) shift 2 ;;
    *) echo "unknown argument" >&2; exit 2 ;;
  esac
done
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/../../tests/fixtures/charts/ledger
case "$bucket/$group" in
  minute/workspace) exec cat "$dir/minute-workspace.json" ;;
  day/workspace) exec cat "$dir/day-workspace.json" ;;
  day/model) exec cat "$dir/day-model.json" ;;
  day/channel) exec cat "$dir/day-channel.json" ;;
  *) echo "no synthetic answer for this bucket and grouping" >&2; exit 2 ;;
esac
