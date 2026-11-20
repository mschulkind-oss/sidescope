---
title: "Response: chart-friendly agent-activity series"
date: 2026-11-20
status: draft
tags: [response, charts, agent-activity]
summary: "What agent-activity built for each request in the chart-friendly series feature request, what it built differently, and what it did not build."
---

# Response: chart-friendly agent-activity series

> [!NOTE]
> Received upstream response, updated 2026-11-20 through `49f89cc`. Personal usage measurements are omitted from this local copy. The capability check described below establishes the earlier series features, not all later quota additions: check the named quota columns separately and explicitly verify the quota-series command. A ledger write timestamp is never proof of collection coverage.

**Audience:** the author of the [feature request](agent-activity-charts.md), on the Sidescope side. This answers each request, R1 to R7, with what `agent-activity` now does.

## 1. Summary

Six of the seven requests are built, three of them in a narrower form than sketched because the ledger cannot truthfully say more. One part of R6, several queries in one call, is not built. R7 is settled by an owner ruling and needed no code.

Two things the request did not ask for were added afterwards at the owner's direction: Claude plan usage as an estimated percent of its quota bars, and a command for the observed quota readings (section 7).

Everything is in four commits on `main` of `agent-activity`, on top of the commit the request was read at:

```text
b7ac08a  feat(ledger): series total, filters, last commit time and published fixtures
9a15507  feat(ledger): series capabilities, column selection, open buckets and requested range
73a5063  feat(ledger): report Claude plan usage as an estimated percent of its quota bars
49f89cc  feat(quota): quota series command for the observed readings
```

**None of it is deployed.** The installed binary on the owner's machine changes only when the owner deploys. Until then the installed `agent-activity` answers as it did at `6fbde52` or earlier, so a consumer should check for the new behavior instead of assuming it (section 4).

The contract is in `docs/reference/ledger-v1.md`, sections "GET /api/ledger/series", "GET /api/ledger/capabilities" and "CLI". Synthetic answers are in `docs/reference/ledger-v1-fixtures/`.

## 2. Answer to each request

| ID | Outcome | In one line |
| :--- | :--- | :--- |
| R1 | Built, narrower | `range.requested` is there; a bucket is marked `open` or not marked at all |
| R2 | Built, narrower | `lastCommitAt` is on the series answer; there is no `observedThrough` |
| R3 | Built | `ledger capabilities` and `GET /api/ledger/capabilities` |
| R4 | Built | Twelve fixture files, each checked against the reader by a test |
| R5 | Built | `--group-by none`, and `--model`, `--harness`, `--role`, `--channel` filters |
| R6 | Half built | `--columns` is built; a batch of queries is not |
| R7 | Settled, no code | One workspace directory is one project |

### R1. Requested and effective bounds; open buckets

Built as sketched, with one difference in `state`.

- **`range.requested`** holds the `from` and `to` the query named, before widening. An end the query left out is `null`. `range.from` and `range.to` remain the bounds the answer covers.
- **`state`** is `"open"` on a bucket whose `end` is after `generatedAt`. That is the only value.
- **There is no `provisional` and no `closed`.** A response is counted when its history file is read, which can be days after it was made (a session synced from another machine, a file the collector was behind on). The ledger therefore cannot assert that any bucket is final. A bucket with no `state` is past, not final, which matches the request's rule that absent means unknown.

The fixture `series-open-bucket.json` shows both fields, and the pair `series-late-correction-before.json` and `series-late-correction-after.json` shows an old bucket changing.

### R2. Collection freshness

Built in part.

- **`lastCommitAt`** is now on every series answer: the time the collector last wrote to the ledger, or `null` for a ledger never written. It is the value `ledger status` already reported.
- **There is no `collection.observedThrough`.** The ledger has no watermark it could assert, so the field would always be `null`. The request says a last-commit time must not be passed off as a watermark, and this follows that.
- **`lastCommitAt` moves only when the collector finds something new.** An old value means either a stopped collector or a machine where no agent has run. It cannot tell those apart.

What remains unread is in the `coverage` block that every answer already carries (`filesBehind`, `bytesBehind`, `oldestUnreadModifiedAt`).

### R3. Capability and metric descriptors

Built as a command and a route with the same body. It reads no profile and no ledger.

```sh
agent-activity ledger capabilities --format json
```

It lists the accepted layouts, buckets, group dimensions and filters, the three refusal limits (400 buckets, 60,000 values, three group dimensions), the omitted-zero rule, the decimal format, and for each column its `unit`, `quantity`, `channel`, `estimate` and, for a cost, its `completenessColumn`.

Differences from the sketch:

- **One answer, not two blocks.** The descriptor is its own answer under `series`, and the per-column metadata is `series.columns`. It is not repeated on every series answer.
- **`quantity` is always `interval_total`.** The series has no gauges.
- **`estimate` is true for both cost columns.** Metered cost is a list-price estimate; the subscription cost is at the provider's published rates.

### R4. Synthetic fixtures

Built. The files are in `docs/reference/ledger-v1-fixtures/`, with a `README.md` saying what each exercises. Every name, path and number in them is invented.

| Requested case | File |
| :--- | :--- |
| `empty` | `series-empty.json` |
| `zero-only` | `series-zero-only.json` |
| `unpriced` | `series-unpriced.json` |
| `subscription` | `series-subscription.json` |
| `late-correction` | `series-late-correction-before.json`, `series-late-correction-after.json` |
| `dst-day` | `series-dst-23-hour-day.json`, `series-dst-25-hour-day.json` |
| `too-many-buckets` | `error-too-many-buckets.json` |

Three more were added: `series-objects.json` (the `objects` layout with two dimensions), `series-open-bucket.json` (R1 and R6) and `capabilities.json` (R3).

A test, `TestSeriesFixtures`, builds each synthetic ledger and fails if the reader's answer differs from the committed file, so a fixture changes only when the answer does. The refusal fixture is the HTTP body; the command prints `ledger series: too-many-buckets` on standard error and exits 2.

### R5. Filters and a total

Built as sketched.

- **`--group-by none`** returns exactly one group with `"keys": []` and `"key": ""`. It is present even when nothing was counted, and it equals the sum of the groups any other grouping of the same query returns.
- **`--model`, `--harness`, `--role` and `--channel`** each take one exact value and combine with each other and with `--workspace`.
- **`--model "(unknown)"`** selects responses with no recorded model.
- **`--channel`** takes the keys of the `channel` dimension: `metered`, `subscription`, or `subscription:` followed by a provider, such as `subscription:github-copilot`.
- **A value nothing has is not an error.** It gives an answer with no groups, or zeros under `none`. This confirms the point the request asked to have confirmed.
- **The answer echoes the filters it applied** in `filters`, by dimension name.
- **An empty `--group-by` is still the default**, `model`.

Two things a consumer should know:

- **`coverage` ignores every filter except `workspace`.** It describes the workspaces read, not the slice returned.
- **A channel filter or grouping is slower.** The per-minute totals do not keep the provider, so those queries add up the responses themselves. On a copy of the owner's ledger, a 30-day total filtered to one channel took 1.9 seconds, against a few hundred milliseconds for the same range without it. Three hours at minute resolution took 0.1 to 0.5 seconds either way.

### R6. Selective columns and batch

- **`--columns` is built.** With `--layout columns`, `--columns knownCostUSD,unpriced` returns only those counters. The values equal the same columns of the full answer, and a counter that is zero throughout is still left out. An unknown or repeated name, or `--columns` without `--layout columns`, is refused as `invalid-columns` (exit 2).
- **A batch of queries is not built.** The request says to build it only if polling cost warrants it, and the measured cost above does not. Two answers read from the same ledger state carry the same `generation`, so a consumer that draws two linked views can compare the two values and poll again if they differ.

### R7. Project identity

Settled by the owner on 2026-11-20: **one workspace directory is one project.** There is no separate project identity and none is planned. A chart by project groups by `workspace`.

Work in a git worktree inside a workspace directory is already counted under that workspace. A response from a directory that is not a registered workspace is labeled with the name of its history folder, such as `--home-user--`.

One known defect, tracked in the `agent-activity` backlog and not fixed: a few history-folder labels name a directory that is a registered workspace, so a small amount of usage is counted apart from its project.

## 3. Compatibility

The request asked that new fields appear only behind an opt-in flag, and offered a weaker promise as the alternative. `agent-activity` took the weaker promise: existing fields keep their names, types and meanings, and only additions appear.

| Addition | Appears |
| :--- | :--- |
| `lastCommitAt` | On every series answer |
| `range.requested` | On every series answer from the command or the HTTP route |
| `state` on a bucket | Only on a bucket that ends after `generatedAt` |
| `filters` | Only when a filter was given |
| `keys` on a group, `layout` | On every series answer (added one commit before the request was read) |

So the JSON for an unchanged query is not byte-identical to the older output. A reader that ignores unknown fields is unaffected. The table output is unchanged.

## 4. Checking for the new behavior

The installed binary may predate these commits. Two checks need no version parsing:

- **`agent-activity ledger capabilities --format json` exits 0** only on a build at or after `9a15507`. An older build exits 2 with `ledger: unknown ledger command`. Success means every item in this response is available.
- **A series answer that has `lastCommitAt`** is from a build at or after `b7ac08a`, which has the total and the filters but not necessarily `--columns`, `state` or `range.requested`.

## 5. Not built

- A `provisional` or `closed` bucket state (R1), and a coverage watermark (R2). The ledger cannot assert either.
- A batch of queries from one snapshot (R6).
- A project identity apart from the workspace label (R7), by owner ruling.
- Dollar figures for Claude plan responses. They stay in tokens; only a provider that bills by token (GitHub Copilot) has a subscription cost. A spending chart will show the plan as zero dollars. Chart the plan in quota percent instead (section 7).

## 6. Suggested profile for the request's use cases

Each command exits 0 with one JSON object on standard output.

```sh
# 1. Recent spending: three hours by minute, one line, metered only.
agent-activity ledger series --format json --layout columns --bucket minute \
  --group-by none --channel metered --columns knownCostUSD,unpriced \
  --from "$(date -u -d '3 hours ago' +%FT%TZ)"

# 2. Daily spending by project: five days, bars and a ranking from one answer.
agent-activity ledger series --format json --layout columns --bucket day \
  --group-by workspace --from "$(date -u -d '5 days ago' +%FT%TZ)"

# 3. One selected facet: one model, by hour, for the last day.
agent-activity ledger series --format json --layout columns \
  --group-by none --model claude-fable-5-2

# 4. A single total per bucket, every channel.
agent-activity ledger series --format json --layout columns --group-by none

# 5. Compatibility check.
agent-activity ledger capabilities --format json
```

In use case 1 the last bucket carries `"state": "open"`; draw it as still filling. To judge whether the ledger is current, compare `lastCommitAt` with `generatedAt` and show `coverage.filesBehind`, remembering that an old `lastCommitAt` on an idle machine is normal.

## 7. Claude plan usage in quota percent

The owner's direction, 2026-11-20: Claude plan usage is to be charted, if at all, as the share of the plan's quota it consumed, not in dollars. Two additions serve that.

### Estimated percent, from recorded usage

The `columns` layout of the series has four more columns. All four are zero, and so absent, for anything that is not a Claude Code subscription response.

| Column | Meaning |
| :--- | :--- |
| `subscriptionWeeklyPercent` | Estimated percent of the weekly quota bar the bucket's usage consumed |
| `subscriptionFiveHourPercent` | The same for the five-hour bar |
| `subscriptionWeeklyUnweighted`, `subscriptionFiveHourUnweighted` | Responses the estimate leaves out because their model has no fitted weight |

```sh
agent-activity ledger series --format json --layout columns --group-by model \
  --channel subscription --columns subscriptionWeeklyPercent,subscriptionWeeklyUnweighted
```

How to read them:

- **They are estimates.** Each response's tokens are priced at list rates and multiplied by a weight fitted from the owner's own quota readings. The provider publishes no weights. The answer names the weights it used in `quotaWeights`, with the date they were fitted through.
- **They add up; they are not the bar.** A column sums across buckets and groups to "what this usage added". The bar itself resets and is also moved by other clients on the account.
- **They are percent of one plan's bars**, the plan named in `quotaWeights.seatPriceUSDPerMonth`.
- **The weights are part of the build.** They change only when the owner refits and deploys.

The upstream response reports a comparison within the data used to fit the weights, not held-out validation. Personal usage measurements are omitted from this local copy.

`ledger capabilities` describes the columns with `unit: "percent"` and `estimate: true`, and the fixture `series-quota-percent.json` shows them with invented weights.

### Observed readings and reset edges

A separate command prints what the provider reported:

```sh
agent-activity quota series --format json --window seven_day
```

It returns, per account, the reading times in `at` and, per window, `percent` aligned with them, `resets` (the reading index from which each reported reset time holds, so every entry after the first is a reset edge) and `edges` (recorded changes such as `usage_changed` with its delta). Readings are taken about every 15 minutes and the last seven days are returned; six days of two accounts is about 120 KB.

The contract is in `docs/quota/backend.md`, section "Readings as a series". The command reads the retained journal only and never contacts the provider.

Drawn together, the reading is where the bar stood and the estimate is what recorded usage added to it.

