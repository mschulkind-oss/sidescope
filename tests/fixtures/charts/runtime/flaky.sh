#!/bin/sh
# usage: flaky.sh COUNT_FILE JSON - the first run prints JSON, later runs fail.
if [ -s "$1" ]; then
  echo "SECRET-STDOUT"
  echo "SECRET-STDERR" >&2
  exit 1
fi
echo run >> "$1"
cat "$2"
