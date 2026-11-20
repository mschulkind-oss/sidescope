#!/bin/sh
# usage: noisy-stderr.sh JSON - 1 MiB on stderr (with ESC bytes), then valid stdout.
{ printf '\033[31mwarn \n'; head -c 1048576 /dev/zero | tr '\0' 'e'; } >&2
cat "$1"
