#!/bin/sh
# usage: garbage.sh COUNT_FILE JSON - exit 0 with output that is not JSON.
if [ -s "$1" ]; then
  echo "SECRET-STDOUT not json {"
  exit 0
fi
echo run >> "$1"
cat "$2"
