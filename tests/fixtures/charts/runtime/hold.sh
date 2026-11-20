#!/bin/sh
# usage: hold.sh PID_FILE - record the pid, then become a long sleep (no descendants).
echo $$ > "$1"
exec sleep 30
