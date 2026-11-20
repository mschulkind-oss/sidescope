#!/bin/sh
# usage: flood.sh - write more than 1 MiB to stdout.
head -c 2097152 /dev/zero | tr '\0' 'x'
