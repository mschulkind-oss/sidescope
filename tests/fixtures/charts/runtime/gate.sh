#!/bin/sh
# usage: gate.sh GATE_FILE JSON PID_FILE - wait for GATE_FILE, then print JSON.
echo $$ > "$3"
echo run >> "$3.runs"
while [ ! -e "$1" ]; do sleep 0.02; done
cat "$2"
