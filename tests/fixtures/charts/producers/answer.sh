#!/bin/sh
# usage: FAKE_RECORD=LOG FAKE_ANSWER=FILE answer.sh ARGS...
# Fake producer: appends one tab-separated line holding its arguments to
# $FAKE_RECORD (one line per spawn) and prints the file $FAKE_ANSWER.
for argument in "$@"; do
  printf '%s\t' "$argument" >> "$FAKE_RECORD"
done
printf '\n' >> "$FAKE_RECORD"
cat "$FAKE_ANSWER"
