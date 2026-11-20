#!/bin/sh
# usage: FAKE_RECORD=LOG FAKE_PID=FILE flood.sh ARGS...
# Fake producer: records one argument line and its pid, writes 3 MiB, then
# becomes `sleep 30` (same pid) so a live process proves the wrapper did not
# reap it.
for argument in "$@"; do
  printf '%s\t' "$argument" >> "$FAKE_RECORD"
done
printf '\n' >> "$FAKE_RECORD"
echo $$ > "$FAKE_PID"
head -c 3145728 /dev/zero | tr '\0' 'x'
exec sleep 30
