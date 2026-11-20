#!/bin/sh
# usage: hang.sh PID_DIR - record pids, leave a descendant holding stdout, never finish.
echo $$ > "$1/child"
sleep 30 &
echo $! > "$1/descendant"
wait
