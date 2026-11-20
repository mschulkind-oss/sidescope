#!/bin/sh
# usage: FAKE_RECORD=LOG fail.sh ARGS...
# Fake producer: records one argument line and exits 3 with nothing on stdout.
for argument in "$@"; do
  printf '%s\t' "$argument" >> "$FAKE_RECORD"
done
printf '\n' >> "$FAKE_RECORD"
exit 3
