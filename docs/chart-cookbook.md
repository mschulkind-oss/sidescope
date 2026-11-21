---
title: "Chart cookbook: author, replay and debug a chart without a display"
date: 2026-11-20
status: accepted
tags: [guide, charts, cookbook]
summary: "Three worked chart recipes (spending, Claude plan quota, a latency gauge), the troubleshooting flow from a report's status to one next command, and the profile notes."
vantage:
  status-chip: true
---

# Chart cookbook

How to write a chart source and card, prove they say what you intend, and find the fault when they do not, using only `sidescope charts` and `jq`. No window opens and nothing starts a producer unless you type `charts run`. Background: the [user guide](guide.md#7-charts-from-local-commands), the [command-chart reference](reference/command-charts/README.md) and the generated [key matrix](reference/command-charts/key-matrix.md).

Every block marked as runnable here is executed by the test suite exactly as written, from the repository root, with the built `sidescope` and `jq` on `PATH`, and must print the lines shown beneath it. From a checkout, `just run -- charts ...` stands in for `sidescope charts ...`. Each block makes its own scratch directory with `mktemp -d`.

> [!WARNING]
> `charts run` executes the configured command with this application's authority and environment, unsandboxed, exactly once per source you name. Nothing else here runs a command: `schema`, `check` and `replay` only read. A capture written with `--capture` is real producer output; keep it out of version control.

## 1. The loop

| Step | Command | What it proves |
| :--- | :--- | :--- |
| 1 | `charts schema` | Which keys exist, their defaults and ranges, the payload versions and the limits. Reads no configuration. |
| 2 | `charts check --config FILE` | The candidate file is valid and each source's request resolves. Runs nothing, reads no data. |
| 3 | `charts replay --config FILE --at INSTANT --data SOURCE=FILE` | Saved producer output, normalized and reduced as the application would, with your clock. Runs nothing. |
| 4 | `charts run --config FILE --source ID [--capture DIR]` | The real command once, then the same reduction. The only verb that executes anything. |
| 5 | `jq -e` on the report | That the chart is what you *intended*. Exit 0 means only that nothing failed. |

Every verb but `schema` prints exactly one JSON report (`sidescope-chart-report-v1`) on stdout, even for a usage error, so a script never parses prose. Exit codes: `0` ok, `1` findings (a source failed, a dataset was refused, a card was refused or unavailable), `2` invalid (usage, configuration, selection, unreadable data, capture path, an oversized report), `3` undetermined (a required check could not be decided). An empty answer is a truthful `0`: assert the outcome you expect, because `no-observations`, `no-intervals`, `no-known-values` and an all-zero `plotted` are four different results.

The payload versions and the limits come from the command itself:

<!-- cookbook: run -->
```bash
sidescope charts schema --section payload | jq -r '.sections.payload.versions[] | "\(.id): adds \(.adds | join(",") | if . == "" then "nothing" else . end)"'
sidescope charts schema --section limits | jq -r '.sections.limits[] | select(.name == "stdout_bytes") | "\(.name) = \(.value)"'
```

Prints:

```text
sidescope-chart-v1: adds nothing
sidescope-chart-v1.1: adds sampled,breaks
stdout_bytes = 1048576
```

## 2. Recipe: the money dashboard

The shipped [filtered profile](../examples/charts/agent-activity-filtered.toml) reads a ledger CLI through the generic `columns-v1` mapper. Check it, then replay saved synthetic answers against its cards. The answers below are invented fixtures; no ledger or account is touched.

<!-- cookbook: run -->
```bash
sidescope charts check --config examples/charts/agent-activity-filtered.toml \
  | jq -r '"status \(.status); sources \(.sources | length); cards \(.cards | length); execution \(.checks[] | select(.id == "execution") | .status)"'
```

Prints:

```text
status ok; sources 5; cards 6; execution not-run
```

`recent` asks for no grouping, so it replays against a group-none answer. This one holds three days of the 23-hour spring-forward week; the known cost is `0.0401` per day.

<!-- cookbook: run -->
```bash
sidescope charts replay --config examples/charts/agent-activity-filtered.toml \
  --at 2026-11-16T15:04:05Z \
  --data recent=tests/fixtures/charts/upstream/series-dst-23-hour-day.json \
  --card recent-cost --details recent-cost \
  | jq -r '.cards[0] | "\(.outcome.kind) \(.summary.quality) \(.summary.bound) exact=\(.summary.exact)", "rows \(.details.rows_total), grain \(.details.grain)"'
```

Prints:

```text
plotted complete exact exact=0.1203
rows 3, grain interval-group
```

A bucket still open at the right edge is not a complete total. The next answer carries two buckets marked `open`:

<!-- cookbook: run -->
```bash
sidescope charts replay --config examples/charts/agent-activity-filtered.toml \
  --at 2026-11-16T15:04:05Z \
  --data recent=tests/fixtures/charts/upstream/series-open-bucket.json \
  --card recent-cost \
  | jq -r '.cards[0] | "\(.summary.quality) \(.summary.bound) exact=\(.summary.exact) open_right_edge=\(.intervals.open_right_edge) partial=\(.intervals.partial) complete=\(.intervals.complete)"'
```

Prints:

```text
partial at-least exact=0 open_right_edge=true partial=2 complete=1
```

Assert intent, not just success. The pattern below is the whole point of the loop: `jq -e` exits non-zero when the chart is not what you meant.

<!-- cookbook: run -->
```bash
sidescope charts replay --config examples/charts/agent-activity-filtered.toml \
  --at 2026-11-16T15:04:05Z \
  --data recent=tests/fixtures/charts/upstream/series-zero-only.json \
  --card recent-cost \
  | jq -e '.cards[0] | .outcome.kind == "plotted" and .summary.exact == "0" and .summary.quality == "complete"'
echo "exit $?"
```

Prints:

```text
true
exit 0
```

## 3. Recipe: Claude plan quota

Two different quantities, never overlaid or summed, are in the [quota profile](../examples/charts/agent-activity-quota.toml): *estimated contribution* (percentage points of quota that recorded subscription responses are estimated to have used) and *observed utilization* (the provider's reported bar level at each reading). Two wrappers in `examples/charts/` produce them; each has a `--convert-stdin` mode that converts a saved producer answer and runs nothing.

**Feature check, by hand.** The estimate cards need a producer that lists the four quota columns. This copy of the producer's capabilities answer has them; the same answer with the quota descriptors removed does not. A successful probe proves only that the columns are listed, not that the weights are good.

<!-- cookbook: run -->
```bash
check='["subscriptionWeeklyPercent","subscriptionWeeklyUnweighted","subscriptionFiveHourPercent","subscriptionFiveHourUnweighted"] - (.series.columns | keys) | length == 0'
jq -e "$check" tests/fixtures/charts/upstream/capabilities.json
jq -e "$check" tests/fixtures/charts/upstream/capabilities-without-quota.json || echo "quota columns missing: do not adopt the estimate cards"
```

Prints:

```text
true
false
quota columns missing: do not adopt the estimate cards
```

**Estimated contribution.** Convert a saved answer with `--convert-stdin`, replay it through the profile's `quota-estimate` source, and read exact decimals (`0.5` percentage points, never `50%` of anything):

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
python3 examples/charts/quota-estimate.py --quota weekly --bucket 5min --group-by model \
  --from 2026-11-16T12:00:00Z --to 2026-11-16T12:15:00Z --timezone UTC --convert-stdin \
  < tests/fixtures/charts/upstream/series-quota-percent.json > "$work/estimate.json"
sidescope charts replay --config examples/charts/agent-activity-quota.toml \
  --at 2026-11-16T12:15:00Z --data "quota-estimate=$work/estimate.json" \
  --card quota-weekly-by-model --details quota-weekly-by-model \
  | jq -r '.cards[0] | "\(.outcome.kind) exact=\(.summary.exact) unit=\(.details.rows[0].unit)"'
```

Prints:

```text
plotted exact=0.5 unit=percentage points
```

The unit is `percentage points`, so `bounded_percent = true` on an estimate card is refused rather than drawn on a 0 to 100 axis (a weekly window can sum above 100):

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
python3 examples/charts/quota-estimate.py --quota weekly --bucket 5min --group-by model \
  --from 2026-11-16T12:00:00Z --to 2026-11-16T12:15:00Z --timezone UTC --convert-stdin \
  < tests/fixtures/charts/upstream/series-quota-percent.json > "$work/estimate.json"
cat > "$work/bounded.toml" <<TOML
[[chart_sources]]
id = "estimate"
command = ["true"]
format = "sidescope-chart-v1"
window = "15m"
timezone = "UTC"

[[chart_cards]]
id = "bounded"
source = "estimate"
kind = "time-series"
metric = "weekly_quota_points"
group_by = ["model"]
bounded_percent = true
TOML
sidescope charts replay --config "$work/bounded.toml" --at 2026-11-16T12:15:00Z \
  --data "estimate=$work/estimate.json" \
  | jq -r '"exit-status " + .status, (.cards[0].outcome | "\(.kind) \(.reason) \(.subject)")'
```

Prints:

```text
exit-status findings
refused card-mismatch bounded-percent
```

A model with no fitted weight is a lower bound, never a complete zero. Here the wrapper reports a partial series for the model without a five-hour weight:

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
python3 examples/charts/quota-estimate.py --quota five-hour --bucket 5min --group-by model \
  --from 2026-11-16T12:00:00Z --to 2026-11-16T12:15:00Z --timezone UTC --convert-stdin \
  < tests/fixtures/charts/quota/estimate-no-weight.json > "$work/estimate.json"
sidescope charts replay --config examples/charts/agent-activity-quota.toml \
  --at 2026-11-16T12:15:00Z --data "quota-estimate-recent=$work/estimate.json" \
  --card quota-five-hour-recent \
  | jq -r '.cards[0].summary | "\(.quality) \(.bound)"'
```

Prints:

```text
partial at-least
```

A wrapper that cannot be honest exits non-zero and prints nothing: an answer with no `quotaWeights` is not converted, and as a source it shows as `unavailable`.

<!-- cookbook: run -->
```bash
python3 examples/charts/quota-estimate.py --quota weekly --bucket 5min --group-by model \
  --from 2026-11-16T12:00:00Z --to 2026-11-16T12:15:00Z --timezone UTC --convert-stdin \
  < tests/fixtures/charts/quota/estimate-missing-weights.json 2>/dev/null | wc -c | tr -d ' '
python3 examples/charts/quota-estimate.py --quota weekly --bucket 5min --group-by model \
  --from 2026-11-16T12:00:00Z --to 2026-11-16T12:15:00Z --timezone UTC --convert-stdin \
  < tests/fixtures/charts/quota/estimate-missing-weights.json >/dev/null 2>&1
echo "exit $?"
```

Prints:

```text
0
exit 1
```

**Observed utilization.** Readings are about fifteen minutes apart; the chart claims nothing between them. A reset breaks the line (no delta is drawn across it) and the headline is the last reading with its own instant:

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
python3 examples/charts/quota-observed.py --account-key acct-a --window five_hour \
  --from 2026-11-16T12:00:00Z --to 2026-11-16T15:00:00Z --timezone UTC --convert-stdin \
  < tests/fixtures/charts/quota/observed-resets.json > "$work/observed.json"
sidescope charts replay --config examples/charts/agent-activity-quota.toml \
  --at 2026-11-16T15:00:00Z --data "quota-observed-primary-five_hour=$work/observed.json" \
  --card quota-observed-primary-five_hour --details quota-observed-primary-five_hour \
  | jq -r '.cards[0] | "last=\(.summary.exact) at \(.summary.observed.at)", ("breaks " + ([.details.rows[].break_before] | map(if . then "1" else "0" end) | join("")))'
```

Prints:

```text
last=5 at 2026-11-16T13:00:00Z
breaks 00101
```

A newest reading that is null keeps the last known one and says so:

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
python3 examples/charts/quota-observed.py --account-key acct-a --window five_hour \
  --from 2026-11-16T12:00:00Z --to 2026-11-16T15:00:00Z --timezone UTC --convert-stdin \
  < tests/fixtures/charts/quota/observed-newest-null.json > "$work/observed.json"
sidescope charts replay --config examples/charts/agent-activity-quota.toml \
  --at 2026-11-16T15:00:00Z --data "quota-observed-primary-five_hour=$work/observed.json" \
  --card quota-observed-primary-five_hour \
  | jq -r '.cards[0].summary | "last=\(.exact) newest_is_null=\(.newest_is_null)"'
```

Prints:

```text
last=20.5 newest_is_null=true
```

The estimate and the observed cards never share an account: a ledger estimate carries no account key, so which observed account an estimate belongs to is unknown and none is inferred.

## 4. Recipe: a latency gauge with `summary = "last"`

A gauge is a reading, not an amount accrued in an interval, so it is reduced by `last` and never summed. The synthetic source `synthetic-latency.sh` prints a native answer for two services. This candidate file is the whole recipe; `check` runs nothing.

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
cat > "$work/latency.toml" <<TOML
[[chart_sources]]
id = "latency"
command = ["$PWD/examples/charts/synthetic-latency.sh", "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_cards]]
id = "api-latency"
source = "latency"
kind = "time-series"
metric = "latency_ms"
summary = "last"
group_by = ["service"]
TOML
sidescope charts check --config "$work/latency.toml" \
  | jq -r '"status \(.status)", (.cards[0] | "\(.outcome.kind) \(.intent.summary)")'
```

Prints:

```text
status ok
not-evaluated last
```

`run` executes the one named source, here a deterministic shell script. Capture it, then replay the capture: the two agree on every fact and differ only in provenance. A gauge over several groups has no single headline, so `summary` is `null` and each group's last reading is a row.

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
cat > "$work/latency.toml" <<TOML
[[chart_sources]]
id = "latency"
command = ["$PWD/examples/charts/synthetic-latency.sh", "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_cards]]
id = "api-latency"
source = "latency"
kind = "time-series"
metric = "latency_ms"
summary = "last"
group_by = ["service"]
TOML
sidescope charts run --config "$work/latency.toml" --source latency --capture "$work/cap" > "$work/run.json"
sidescope charts replay --config "$work/latency.toml" --from-capture "$work/cap" > "$work/replay.json"
jq -r '.cards[0] | "\(.outcome.kind) groups=\(.groups.total) rows=\(.rows | length) summary=\(.summary)"' "$work/run.json"
facts='[.cards[] | {outcome, intent, summary, groups, rows}]'
if [ "$(jq -S "$facts" "$work/run.json")" = "$(jq -S "$facts" "$work/replay.json")" ]; then echo "same facts"; fi
jq -r '.sources[0] | "origin \(.data_origin), attempt \(.attempt), execution \(.result[] | select(.id == "execution") | .status)"' "$work/replay.json"
```

Prints:

```text
plotted groups=2 rows=2 summary=null
same facts
origin replay, attempt null, execution not-run
```

## 5. Troubleshooting flow

Start from the report, not from the screen: read `status`, then each source's `error.kind`, then each card's `outcome.kind`. One branch, one next command.

| You see | It means | Next command |
| :--- | :--- | :--- |
| exit `2`, `errors[].id` is `usage` or `invalid-config` | The invocation or the file is wrong; the report names the key. | `sidescope charts schema --section source` (or `card`, `mapping`), then fix the key and `check` again. |
| `error.kind` is `missing-executable` or `spawn` | The command cannot start. | Run `command -v` on the executable, then `check --show-argv` to see the resolved argument vector. |
| `error.kind` is `nonzero-exit` | The producer failed; its stderr is withheld unless the source sets `capture_stderr`. | Run the argv by hand, or set `capture_stderr = true` and re-run `charts run` to see the sanitized excerpt. |
| `error.kind` is `timeout` or `output-bound` | The command ran past `timeout_seconds` or printed more than 1 MiB. | Narrow the window or bucket; see `charts schema --section limits`. |
| `error.kind` is `cancelled` | The run was cancelled before it finished. | Run again. |
| `error.kind` is `parse` | Output is not one JSON document of the declared format. | Re-run with `--capture DIR`, then `replay --data` the captured file and read the JSON yourself. |
| `error.kind` is `unsupported-version` | The payload version is not one this build accepts. | `charts schema --section payload` lists the versions. |
| `error.kind` is `invalid-mapping` | The mapping points at nothing the answer has, or a pointer's parent is missing (a different layout, such as `--layout objects`). | Compare the answer with the pointer scopes in `charts schema --section mapping`. A missing leaf under an existing parent is zero only with `omitted = "zero"`. |
| `error.kind` is `invalid-dataset` | The answer is well formed but not a valid dataset (a duplicate series id, a bad interval, an invalid `sampled` or `breaks` value). | The error text names the field; a duplicate series id usually means the query groups the answer and the source declares no dimension. |
| card `outcome.kind` is `refused`, reason `card-mismatch` | The card names a metric, summary or option the source cannot supply. | Read `outcome.subject`, `wanted` and `available`; fix the card, not the producer. |
| card `outcome.kind` is `refused`, reason `capacity` | More groups, series or cells than a card may draw. | Reduce `group_by` or raise `top_n` within range; see the limits. |
| card `outcome.kind` is `unavailable` | The source failed or was refused; the card has nothing to show. | Fix the source error above first. |
| `no-observations` | The answer has no series. | Truthful for an idle producer; assert it if you expect it. |
| `no-intervals` | Series exist but no interval falls in the card's range. | Check `request.from`/`to` against the answer's range and `--at`. |
| `no-known-values` | Every cell is missing (null). | The producer knows nothing for the window; this is not zero. |
| `plotted` but the headline is wrong | Reduction or grouping is not what you meant. | `replay --details CARD` and read the exact rows; assert them with `jq -e`. |

Checks after a failed one are `not-run` with a dependency reason, not `unknown`; an aggregate `freshness` is never `pass` when a source failed. A deferred request (the producer is asked at local midnight, for example) ends `undetermined` and exits `3`.

The next block shows two branches on saved answers: an `--layout objects` answer the columns mapping cannot read (`invalid-mapping`, never drawn as zero), and a grouped answer fed to a source that declares no dimension (`invalid-dataset`).

<!-- cookbook: run -->
```bash
sidescope charts replay --config examples/charts/agent-activity-filtered.toml \
  --at 2026-11-16T15:04:05Z \
  --data daily=tests/fixtures/charts/upstream/series-objects.json --card daily-cost \
  | jq -r '"status \(.status)", "error \(.sources[0].error.kind)", "card \(.cards[0].outcome.kind) because \(.cards[0].outcome.source_error)", "banner: \(.cards[0].display.banner)"'
sidescope charts replay --config examples/charts/agent-activity-filtered.toml \
  --at 2026-11-16T15:04:05Z \
  --data copilot-cost=tests/fixtures/charts/upstream/series-unpriced.json --card copilot-estimate \
  | jq -r '"error \(.sources[] | select(.id == "copilot-cost") | .error | "\(.kind): \(.text)")"'
```

Prints:

```text
status findings
error invalid-mapping
card unavailable because invalid-mapping
banner: Source unavailable: invalid mapping
error invalid-dataset: duplicate series id
```

A producer that fails exits `1` and prints no raw output. Here a fake producer prints `{not json` and exits `0`: the source is a `parse` failure, the freshness check does not claim a pass, and no raw stdout is in the report.

<!-- cookbook: run -->
```bash
work=$(mktemp -d)
printf '#!/bin/sh\necho "{not json"\n' > "$work/bad.sh"
chmod +x "$work/bad.sh"
cat > "$work/bad.toml" <<TOML
[[chart_sources]]
id = "bad"
command = ["$work/bad.sh"]
format = "sidescope-chart-v1"
window = "2m"
timezone = "UTC"

[[chart_cards]]
id = "c"
source = "bad"
kind = "time-series"
metric = "m"
TOML
sidescope charts run --config "$work/bad.toml" --source bad > "$work/report.json"
echo "exit $?"
jq -r '"status \(.status)", "error \(.sources[0].error.kind)", "freshness \(.checks[] | select(.id == "freshness") | .status)", "raw output leaked: \(tostring | contains("{not json"))"' "$work/report.json"
```

Prints:

```text
exit 1
status findings
error parse
freshness not-run
raw output leaked: false
```

## 6. Profile notes

- **Three profiles, never merged.** The legacy [agent-activity profile](../examples/charts/agent-activity.toml) is byte-stable and groups by workspace. The [filtered profile](../examples/charts/agent-activity-filtered.toml) asks the producer for only the channels and columns it draws; it needs a producer whose capabilities probe succeeds. The [quota profile](../examples/charts/agent-activity-quota.toml) holds the wrappers' sources and cards and contains no money. The legacy and filtered profiles contain no quota source or card, so an older producer is never handed a quota query. Adoption is copying tables by hand into your own configuration; Sidescope never loads a profile on its own.
- **What a successful probe does not establish.** `agent-activity ledger capabilities` listing a column proves the producer can name it, not that the collector is current, that weights were fitted from enough data, or that the numbers match an invoice.
- **Why the shared subscription estimate is not carried forward.** Subscription usage has no per-response dollar figure; the older estimate card stays separate from metered cost and is not reused for quota, because points of quota and dollars are different quantities and are never converted.
- **Points versus readings.** Estimated contribution is an interval total in `percentage points`: it is summed, may exceed 100 over a window, and is never a bar level. Observed utilization is a gauge of `percent` readings: it is read as the last reading, never summed, and only a gauge of `percent` may use `bounded_percent`.
- **The fitted-weights basis.** The estimate multiplies recorded usage by weights the producer fitted from one owner's readings. The weights are not published by the provider; a model without a weight is a partial lower bound, and the wrapper prints the fit's date range as a notice.
- **The API-key caveat.** Claude Code used with a direct API key is classified `subscription` upstream and may be counted in the estimate; the wrappers do not correct it.
- **"Project" means the ledger's workspace directory label**, a display name and not an inferred project identity.
- **Captures stay out of version control.** A capture directory holds real producer output with owner-only permissions; commit synthetic fixtures instead.

## 7. Key matrix

Every key, default and range that validation accepts is in the generated [key matrix](reference/command-charts/key-matrix.md); `just charts-matrix` regenerates it and a test fails when it is stale. The same tables print in `sidescope charts --help`.
