#!/bin/sh
# usage: concurrency.sh DIR JSON - log how many copies run at once.
touch "$1/active.$$"
ls "$1" | grep -c '^active\.' >> "$1/high-water"
sleep 0.4
cat "$2"
rm -f "$1/active.$$"
