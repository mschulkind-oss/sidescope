---
title: "Feature request: chart-friendly agent-activity series"
date: 2026-11-20
status: draft
tags: [request, charts, agent-activity]
summary: "Proposed, additive improvements to agent-activity's series output that would make generic chart consumers safer. Nothing here blocks Sidescope."
vantage:
  status-chip: true
---

# Feature request: chart-friendly agent-activity series

**Audience:** the author of `agent-activity`. **Status of every request below: proposed.** None has been delivered, accepted, scheduled, or even seen by the upstream author. This document asks; it does not announce.

## 1. Summary and non-blocking promise

Sidescope's [command-charts reference](../reference/command-charts/README.md) describes how it draws time-series and ranked charts from any local command. Its first example reads `agent-activity ledger series --format json --layout columns`. **The current CLI is sufficient for those first charts**: Sidescope will ship a profile that works with today's output, and an owner-written wrapper can fill any gap. No request here is a prerequisite for building, testing, or releasing Sidescope's charts.

The requests remove guesswork a consumer currently resolves by convention. They do not ask `agent-activity` to emit a Sidescope-specific format, draw anything, or integrate with GTK. Every proposed field is additive and optional.

## 2. Upstream reference

The mount is outside this repository, so these are plain locators, not links. They were read on 2026-11-20; the upstream tree was under active change, so re-read before acting.

```text
upstream:   agent-activity, branch checkout at /ctx/agent-activity (read-only mount)
commit:     6fbde52ea5e0b36f34ee199b74bb8d7bd2a4960e (2026-11-20)
            "fix(ledger): price a Copilot cache write recorded by lifetime at Copilot's one rate"
cli flags:  cmd/agent-activity/ledger.go:137-199
wire types: internal/ledger/wire/types.go:48-124   (Header, Range, Usage, Subscription, Coverage)
contract:   docs/reference/ledger-v1.md:464-628    (GET /api/ledger/series, columns layout, CLI)
```

## 3. What already ships (not requested)

Read from the files above. Credit where due: much of what a chart needs exists.

| Capability | Where it lives today |
| :--- | :--- |
| Bucketed series, `minute`/`5min`/`15min`/`hour`/`day`, IANA `--timezone` | `series` flags |
| Half-open UTC `buckets` with `start`/`end`; DST days of 23 or 25 hours | series contract |
| Group by up to three of `model`, `workspace`, `role`, `harness`, `channel`; `--workspace` exact filter | series flags |
| Compact `--layout columns`; all-zero columns omitted, reader treats absent as zero | series contract |
| Decimal USD strings (`knownCostUSD`); matching `unpriced` counters for completeness | columns layout |
| Subscription counters and Copilot list-price estimate kept apart from metered cost | `Usage.Subscription` |
| Header: `version`, `generation`, `generatedAt`, `listEstimate`, `range`, `coverage` (including `filesBehind`, `bytesBehind`, `oldestUnreadModifiedAt`, `statement`) | `Header`, `Coverage` |
| Refusals `too-many-buckets` (over 400) and `too-many-groups` (over 60,000 values) | series contract |
| `Collector` block (present, phase, watch) | **HTTP server only**; the CLI does not fill it |
| Read-only CLI beside a running server; usage failure is exit 2 | CLI section |

## 4. Use cases

Synthetic and generic; none requires Sidescope.

1. **Recent spending.** A status display polls the last three hours at minute resolution every 30 seconds and draws one line. It needs to know whether the right-hand bucket is still open and whether the collector is current.
2. **Daily spending by project.** Five calendar dates, day buckets, grouped by workspace, drawn as bars and a ranking from one answer.
3. **One selected facet.** A user picks one model or one channel and wants only that slice, not every group.
4. **A single total.** A summary header wants one number per bucket without receiving every group.
5. **Compatibility checks.** A consumer written today should detect, rather than guess, that a future `agent-activity` changed a layout or limit.

## 5. Requests

**Must-help** items would remove an ambiguity a careful consumer cannot resolve from today's output. **Optional** items reduce cost or wrapper code but are not needed for correctness. Both are proposals; the maintainer owns whether, when, and how.

| ID | Priority | Request |
| :--- | :--- | :--- |
| R1 | Must-help | Requested and effective bounds, with open and provisional buckets marked |
| R2 | Must-help | Collection freshness reported separately from `generatedAt` |
| R3 | Must-help | Capability and metric descriptors |
| R4 | Must-help | Synthetic golden responses published for consumers |
| R5 | Optional | Direct filters and an explicit total (no-group) output |
| R6 | Optional | Selective columns and an optional batch of queries from one snapshot |
| R7 | Optional | Stable project identity separate from the workspace label |

### R1. Requested and effective bounds; provisional intervals

**Gap.** `range.from`/`range.to` return the aligned bounds, and `range.source` says where they came from. The caller's literal request is not echoed, so a consumer cannot tell an exact rolling window from an outward-aligned one. Nothing marks the bucket still accruing or one that late corrections may still change.

**Sketch (additive, JSON only).**

```json
{
  "range": {"from": "2026-11-20T09:01:00.000Z", "to": "2026-11-20T12:03:00.000Z", "source": "explicit",
            "requested": {"from": "2026-11-20T09:01:30.000Z", "to": "2026-11-20T12:02:10.000Z"}},
  "buckets": [
    {"start": "2026-11-20T12:02:00.000Z", "end": "2026-11-20T12:03:00.000Z", "state": "open"}
  ]
}
```

Minute buckets align the requested start down to `09:01:00` and the requested end up to `12:03:00`; `requested` keeps the literal instants so a consumer can see exactly what was widened. `state` would be `open` (ends after `generatedAt`), `provisional` (older, but the source says it may still change) or `closed` (the producer asserts the bucket is final). **Absent means unknown.** A consumer must not read an absent `state` as `closed` or as finalized data, and a producer that cannot tell omits it.

### R2. Collection freshness separate from answer generation

**Gap.** `generatedAt` is when the answer was assembled. The CLI opens the ledger read-only, so it can succeed while the collector has stopped. `coverage` carries backlog counters but no single statement a consumer can test.

**Sketch.**

```json
{
  "generatedAt": "2026-11-20T12:02:10.000Z",
  "collection": {"observedThrough": null, "lastCommitAt": "2026-11-20T12:01:55.000Z", "basis": "last-commit"}
}
```

`observedThrough` is the instant through which the producer **explicitly asserts** collection, or `null` when it cannot say. A last commit or scan time is not by itself a coverage watermark: nothing guarantees that every event before it was collected, so `lastCommitAt` (or a scan time) is reported separately as freshness metadata and must not be copied into `observedThrough` unless the producer can assert the boundary. `basis` is a short producer-defined string naming what `observedThrough` rests on, and is omitted when it is `null`. **Null is a legitimate answer and means unknown, not current**, even while `lastCommitAt` is present. A consumer must never treat an unchanged `generation` as proof of failure.

### R3. Capability and metric descriptors

**Gap.** A consumer learns the layout, the group dimensions, the limits, and the omitted-zero rule from prose. A descriptor lets it detect incompatibility mechanically, without parsing help text.

**Sketch.** A new read-only command (name is the maintainer's choice) and a matching optional block on the series answer:

```json
{
  "capabilities": {
    "schema": "ledger-series",
    "layouts": ["objects", "columns"],
    "buckets": ["minute", "5min", "15min", "hour", "day"],
    "dimensions": ["model", "workspace", "role", "harness", "channel"],
    "limits": {"buckets": 400, "values": 60000, "groupDimensions": 3},
    "columnsOmitAllZero": true,
    "decimal": {"format": "plain", "maxFractionDigits": 12}
  },
  "metrics": {
    "knownCostUSD": {"unit": "USD", "quantity": "interval_total", "estimate": true,
                     "completenessColumn": "unpriced", "channel": "metered"},
    "subscriptionKnownCostUSD": {"unit": "USD", "quantity": "interval_total", "estimate": true,
                                 "completenessColumn": "subscriptionUnpriced", "channel": "subscription"},
    "responses": {"unit": "count", "quantity": "interval_total", "estimate": false}
  }
}
```

`quantity` is `interval_total` (accrued within the bucket, reducible by sum) or `gauge`. Both are defined in the [dataset contract](../reference/command-charts/dataset-and-mapping.md#the-native-answer). The descriptor needs no Sidescope vocabulary; it names units and meanings that any chart consumer needs.

### R4. Synthetic golden compatibility fixtures

**Gap.** Consumers can only test against output they invented or captured from a real profile. Real captures carry personal workspace paths and usage, and must not be committed.

**Request.** Publish a small set of fully synthetic responses with the repository, each with a stable name and a one-line statement of what it exercises. Required cases:

| Name | Exercises |
| :--- | :--- |
| `empty` | No groups; a successful answer, not an error |
| `zero-only` | Groups whose every column is omitted (all zeros) |
| `unpriced` | Positive `unpriced` with a known subtotal |
| `subscription` | Plain subscription tokens (no cost) beside a Copilot-priced estimate |
| `late-correction` | The same query before and after an old bucket changes value |
| `dst-day` | A 23-hour and a 25-hour day in a zone with DST |
| `too-many-buckets` | The refusal and its exit code |

### R5. Direct filters and an explicit total

**Gap.** Today only `--workspace` filters. Choosing one model or channel means requesting every group. An empty `--group-by` means the default (`model`), so there is no "give me one total."

**Sketch.**

```sh
agent-activity ledger series --format json --layout columns --bucket minute \
  --group-by none --channel metered --from 2026-11-20T09:00:00Z --to 2026-11-20T12:00:00Z
```

- `--group-by none` returns one group with `"keys": []` and `"key": ""` (the empty tuple).
- `--model`, `--channel`, `--harness`, `--role` each take an exact value and mirror `--workspace`.
- Unknown values return an empty answer, not an error, as `--workspace` is documented to behave (confirm before adopting).
- Totals are the exact sum of the groups the unfiltered query would return, within the decimal rules.

### R6. Selective columns and batch

**Gap.** Large grouped answers carry columns a given chart ignores, and two linked views poll twice and may straddle a collection pass.

**Sketch.** `--columns knownCostUSD,unpriced` limits the output (unknown names are a usage failure). A batch form accepts several queries and returns them under one `generation`, in order. **Build only if measured polling cost warrants it.** Both are strictly optional.

### R7. Stable project identity

**Gap.** Workspace labels are paths. A renamed directory or a git worktree is a different label for what a person calls one project. Merging them is upstream policy, so this is a request for a decision, not a design.

**Sketch.** An optional `projects` array mapping a stable `id` to a display `label` and the workspace labels it covers, or a `workspaceId` dimension beside `workspace`. Absent means each workspace is its own project.

## 6. Backwards compatibility

- Every request adds optional fields, optional flags, or a new command. No existing field changes meaning, type, or name. `maxFractionDigits` is 12 because USD amounts are formatted from pico-dollar fractions with trailing zeros trimmed.
- Readers already ignore unknown JSON fields; this FR relies on that and on nothing else.
- The new fields (`range.requested`, bucket `state`, `collection`, `capabilities`, `metrics`) are opt-in metadata: they appear only when the caller asks for them with a new flag or command (the maintainer chooses its name). Output for current flags, including `--format table` and current JSON, stays byte-identical for an unchanged query, and a golden test should pin that. If the maintainer prefers to emit them by default, the promise is weaker: existing fields keep their meaning and type, and only additions appear.
- A new descriptor must report only what is true. Where `agent-activity` cannot say (for example freshness), the answer is `null`, never a guessed value.
- The existing CLI and HTTP routes share one query path today, and the documented rule "`--format json` prints the same body for the same query" should continue to hold for new fields.

## 7. Privacy

- Fixtures (R4) and examples use invented names such as `example-project`, `/home/user/code/alpha`, and model names that exist only in a test catalog. No transcript text, prompt content, account identifier, credential, or measured personal usage appears.
- New descriptors (R3) describe the schema, never the data. They add no profile path, user name, or host detail.
- R2's `basis` is a fixed short vocabulary, not free-form text from collected files.
- Consumers should treat `workspace` labels as personal and keep them out of logs and screenshots; this FR does not change that.

## 8. Acceptance criteria

Criteria are per request. Each applies only if the maintainer chooses to deliver that request.

| Request | Accepted when |
| :--- | :--- |
| R1 | An answer for an explicit `--from`/`--to` includes `range.requested` equal to the literal instants parsed from the flags; the open bucket at the right edge has `state: "open"`; a query wholly in the past has no `open` bucket; without the opt-in flag current output is byte-identical; the minute-aligned example above widens `09:01:30` to `09:01:00` and reports both |
| R2 | `collection.observedThrough` is an RFC 3339 instant no later than `generatedAt`, or `null`; with the collector stopped it does not advance across two queries; it is `null` when the producer cannot determine it, and is never filled from `lastCommitAt` alone |
| R3 | The descriptor lists every column the `columns` layout can emit, each with `unit`, `quantity`, `estimate`; each cost column names its `unpriced` counterpart; limits match the refusals actually enforced (400, 60,000, three dimensions) |
| R4 | Each named fixture is committed, parses as the documented contract, contains no real path or account data, and has a test that regenerates or validates it; the DST fixture shows 23-hour and 25-hour buckets |
| R5 | `--group-by none` and each new filter work with both layouts; filtered and total sums equal the grouped sums over the same fixture; the existing default grouping is unchanged; `--group-by` empty is still the default |
| R6 | Selected columns equal the same columns from the full answer; a batch's answers share one `generation`; the single-query path is unchanged |
| R7 | One project with two workspace labels appears under one id in a fixture; without the feature, output is unchanged |

## 9. Non-goals

- No Sidescope-specific output format, plugin, schema name, or `--sidescope` flag.
- No GTK, drawing, colors, sparklines, or any display logic upstream.
- No watch or streaming transport; bounded snapshot polling is sufficient.
- No change to pricing, coverage semantics, or the list-estimate framing.
- No authentication, sync, collector start, or migration triggered by a read query.
- No currency conversion and no arithmetic expressions in the CLI.
- No promise on timing. Sidescope neither waits for nor depends on these requests.

## 10. Consumer posture

Until any of this ships, Sidescope maps today's columns with an explicit profile: `knownCostUSD` with its `unpriced` counter, omitted columns declared zero, the returned `range` treated as effective bounds, collection freshness shown as unknown, and the header's `coverage` statement displayed as a caveat. If a request is delivered, the profile may adopt it behind a capability check; if it is not, nothing breaks. The upstream author may decline, rename, or reshape any request.
