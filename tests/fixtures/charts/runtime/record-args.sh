#!/bin/sh
# usage: record-args.sh OUT JSON FROM TO ZONE - record the substituted placeholders.
printf '%s\n%s\n%s\n' "$3" "$4" "$5" > "$1"
cat "$2"
