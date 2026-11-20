#!/bin/sh
# Synthetic USD interval totals for two example projects.
# Usage: synthetic-spend.sh --from RFC3339 --to RFC3339 --timezone ZONE
# Deterministic synthetic data in the sidescope-chart-v1 format; no network,
# no files, no clock reads. Runs with the caller's authority like any command.
set -eu
from=""; to=""; zone="UTC"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --from) from="$2"; shift 2 ;;
    --to) to="$2"; shift 2 ;;
    --timezone) zone="$2"; shift 2 ;;
    *) echo "unknown argument" >&2; exit 2 ;;
  esac
done
[ -n "$from" ] && [ -n "$to" ] || { echo "--from and --to are required" >&2; exit 2; }
exec awk -v from="$from" -v to="$to" -v zone="$zone" '
function days_from_civil(y, m, d,   era, yoe, doy, doe) {
  if (m <= 2) y -= 1
  era = int(y / 400)
  yoe = y - era * 400
  doy = int((153 * (m + (m > 2 ? -3 : 9)) + 2) / 5) + d - 1
  doe = yoe * 365 + int(yoe / 4) - int(yoe / 100) + doy
  return era * 146097 + doe - 719468
}
function epoch(t) {
  return days_from_civil(substr(t, 1, 4) + 0, substr(t, 6, 2) + 0, substr(t, 9, 2) + 0) * 86400 \
    + substr(t, 12, 2) * 3600 + substr(t, 15, 2) * 60 + substr(t, 18, 2)
}
function iso(e,   z, y, era, doe, yoe, doy, mp, d, m, s) {
  z = int(e / 86400); s = e - z * 86400
  z += 719468
  era = int(z / 146097)
  doe = z - era * 146097
  yoe = int((doe - int(doe / 1460) + int(doe / 36524) - int(doe / 146096)) / 365)
  y = yoe + era * 400
  doy = doe - (365 * yoe + int(yoe / 4) - int(yoe / 100))
  mp = int((5 * doy + 2) / 153)
  d = doy - int((153 * mp + 2) / 5) + 1
  m = mp + (mp < 10 ? 3 : -9)
  if (m <= 2) y += 1
  return sprintf("%04d-%02d-%02dT%02d:%02d:%02dZ", y, m, d, int(s / 3600), int((s % 3600) / 60), s % 60)
}
BEGIN {
  a = epoch(from); b = epoch(to)
  if (b <= a) { print "empty window" > "/dev/stderr"; exit 2 }
  step = 60 * int(((b - a + 59) / 60 + 359) / 360)
  start = int(a / step) * step
  n = int((b - start + step - 1) / step)
  end = start + n * step
  now = b
  printf "{\"version\":\"sidescope-chart-v1\",\"generated_at\":\"%s\",\"observed_through\":null,\"revision\":\"synthetic-1\",", iso(now)
  printf "\"range\":{\"start\":\"%s\",\"end\":\"%s\",\"timezone\":\"%s\"},\"intervals\":[", iso(start), iso(end), zone
  for (i = 0; i < n; i++) printf "%s{\"start\":\"%s\",\"end\":\"%s\"}", (i ? "," : ""), iso(start + i * step), iso(start + (i + 1) * step)
  printf "],"
  printf "\"metrics\":{\"amount\":{\"label\":\"Synthetic spend\",\"unit\":\"USD\",\"quantity\":\"interval_total\",\"estimate\":true}},\"series\":["
  for (k = 0; k < 2; k++) {
    printf "%s{\"id\":\"%s\",\"dimensions\":{\"project\":\"%s\"},\"metrics\":{\"amount\":{\"values\":[", (k ? "," : ""), (k ? "beta" : "alpha"), (k ? "Example B" : "Example A")
    for (i = 0; i < n; i++) { c = ((int(start / step) + i) * (7 + 4 * k) + 13 * k) % 41; printf "%s\"%d.%02d\"", (i ? "," : ""), int(c / 100), c % 100 }
    printf "],\"quality\":["
    for (i = 0; i < n; i++) printf "%s\"complete\"", (i ? "," : "")
    printf "]}}}"
  }
  printf "],"
  printf "\"notices\":[\"Synthetic data for demonstration.\"]}\n"
}'
