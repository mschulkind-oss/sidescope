#!/bin/sh
# usage: emit.sh COUNT_FILE JSON [ignored...] - count the invocation, print JSON.
echo run >> "$1"
cat "$2"
