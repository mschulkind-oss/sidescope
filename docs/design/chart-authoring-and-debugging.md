---
title: Chart authoring and debugging without the GUI
date: 2026-11-20
status: accepted
stage: DECIDED
next: Complete the implementation plan against the current tree, then build and verify the approved chart tooling and profiles.
---

# Chart authoring and debugging without the GUI

**Status:** 2026-11-20 — approved for implementation; code remains unbuilt for the additions below. The owner directed a planned fan-out build after reviewing this design's scope.

## Verdict

The generic chart renderer shipped (code as of `1f3f347`), with pure model tests, but an agent using the supported CLI cannot yet prove that a card it configured shows what it meant. `doctor` never runs a source or builds a card; deeper checks currently require Rust tests or inspecting the window.

This document proposes five additions and refinements. The new CLI, typed reports and quota adaptations are not implemented; the money/activity cards already have a shipped starting profile.

1. **One `charts` command family** on the existing binary: `schema`, `check`, `replay`, `run`. All emit one versioned JSON report.
2. **One typed semantic result per card** (*card facts*, coined here). The GUI model is built from it and the CLI serializes it, so the two cannot disagree by construction.
3. **A baseline dashboard of five money and activity cards** for the four questions originally asked, plus a capability-gated profile that uses the parts of upstream responses R1–R7 that were built.
4. **Claude plan quota cards, kept apart from money**: estimated quota contribution from the ledger and observed quota utilization from the quota journal. Each comes through an explicit wrapper, and the observed readings need one small generic dataset extension.
5. **A linked cookbook, generated key reference and troubleshooting flow**, plus compatibility tests over upstream's synthetic fixtures and new invented quota fixtures.

**Needs your ruling:** None. Every choice below is technical and is decided here.

## Principles

| Id | Principle |
| :--- | :--- |
| P1 | An agent proves **intent**, not exit status: the card ids, metric, window, grouping, stack, summary and numeric meaning it meant are asserted against structured facts. |
| P2 | The GUI and the inspector read **the same typed result**. The CLI never parses Info lines, table rows or error strings, and never recomputes a reduction. |
| P3 | Every report says **what it did not check**. Static validation says data was not checked; replay never claims freshness; nothing claims pixel appearance. |
| P4 | **Execution is explicit.** A source runs only when named on a `run` command line, once, with the existing bounded runner. No other mode gains execution. |
| P5 | **Exact, display and drawing numbers are three different things** and are labeled as such everywhere. |
| P6 | **Adoption is deliberate.** Authoring happens in a candidate file; nothing writes, enables or rewrites the owner's configuration. |
| P7 | The generic core stays source-independent. Agent-activity names live only in profiles, wrappers, fixtures and the guide. |
| P8 | **Quota is not money, and an estimate is not a reading.** Estimated quota contribution, observed quota utilization and dollar estimates are separate metrics on separate cards, never summed, overlaid or correlated by the tool. |

## 1. Status and facts

### 1.1 What is shipped (code as of `1f3f347`)

- The renderer, runtime, mapper and four reference pages are delivered: [overview](../reference/command-charts/README.md), [dataset and mapping](../reference/command-charts/dataset-and-mapping.md), [command runtime](../reference/command-charts/command-runtime.md), [native charts](../reference/command-charts/native-charts.md). The [user guide section 7](../guide.md#7-charts-from-local-commands) and the [examples README](../../examples/charts/README.md) cover opt-in, a basic example and authority.
- The command surface ([`cli.rs`](../../src/cli.rs)) is `Run`, `Help`, `Version`, `Doctor`. There is no `charts` subcommand, schema discovery, JSON diagnostics, offline fixture path or headless chart run. `--report FILE` is the older usage-report path, not chart JSON.
- `doctor` validates configuration and checks that each executable exists. It never runs a source or builds a card, and a missing executable is only a warning. A valid config with `doctor` exit 0 is therefore not end-to-end proof.
- The pieces needed are already separated and GTK-free: request resolution with an injected timestamp, pure normalization and mapping ([`chart_mapping.rs`](../../src/chart_mapping.rs)), the runtime with its bounded process runner ([`chart_runtime.rs`](../../src/chart_runtime.rs)), and a pure model builder ([`model.rs`](../../src/chart_view/model.rs)).
- The card model is not serializable. Its table and Info content are human strings, its plot values are drawing floats, and a refusal is a message beside an empty plot rather than a typed state.
- The shipped profile [`agent-activity.toml`](../../examples/charts/agent-activity.toml) has four sources and six cards and works against older upstream binaries.

### 1.2 What changed upstream

The [upstream response](../requests/agent-activity-charts-response.md) to the [original request](../requests/agent-activity-charts.md) answers R1–R7 at upstream commit `9a15507` (after `b7ac08a`). Not all of it was built as asked: R1 and R2 are narrower, and R6 delivered column selection but no batch query. Two later commits add Claude plan quota data ([response §7](../requests/agent-activity-charts-response.md#7-claude-plan-usage-in-quota-percent)):

| Upstream commit | Adds | Reads |
| :--- | :--- | :--- |
| `73a5063` | Four estimated quota columns on `ledger series` and a `quotaWeights` block | The ledger |
| `49f89cc` | `agent-activity quota series`, observed quota readings per account and window | The retained quota journal only; no credential or provider call |

None of this is deployed, according to the producer. An installed binary may pre-date any of these commits, and a source checkout does not prove installed capability. A successful capabilities probe establishes only the `9a15507` features. The quota column descriptors must be looked for explicitly, and the quota series command must be checked separately ([section 3.5](#35-quota-profile-and-feature-checks)).

### 1.3 Verification limits that remain

All current evidence is synthetic. There has been no live collector, real account, desktop session or screen-reader speech, no descendant-kill guarantee under SIGKILL, and no wall-clock midnight run. This design adds semantic proof. It adds no appearance proof and no proof that real observations are complete.

## 2. Terms

| Term | Meaning |
| :--- | :--- |
| Source, card, dataset, mapping | As defined in the [overview](../reference/command-charts/README.md). |
| Candidate file (coined here) | A TOML file holding only the chart tables under development, separate from the owner's configuration. |
| Card facts (coined here) | The typed, serializable semantic result of reducing one card against one snapshot at one instant. |
| Report | The JSON document every `charts` verb prints, schema `sidescope-chart-report-v1`. |
| Replay | Feeding a saved stdout file through the same normalizer and model builder with an explicit clock. No process runs. |
| Capture (coined here) | A raw stdout file written by `charts run` only when the caller names a directory. |
| Legacy profile / filtered profile | The shipped agent-activity profile, and the proposed profile that requires upstream `9a15507`. |

## 3. The charts this design proposes

The four original questions map to five baseline cards and one optional card. Money here is always a **list-price estimate of metered responses**, not an invoice. Claude plan use is charted as quota, not dollars, in [section 3.2](#32-claude-plan-quota-two-different-quantities) to [section 3.5](#35-quota-profile-and-feature-checks).

| Card id | Question it answers | Kind and reduction | Source (window) | Grouping | Numeric meaning |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `recent-cost` | What have I spent in the last 2–3 hours, minute by minute? | `time-series`, `summary = "sum"` | `recent` (`window = "3h"`, minute buckets; `"2h"` preset) | none (`group_by = []`) | Amount **per minute**, not a rate or running total; the headline displays the returned-window sum, rounded for reading, with exact values in the report. |
| `daily-cost` | How do the last 3–5 calendar dates compare, and which projects made them up? | `time-bars`, `summary = "sum"` | `daily` (`calendar_days = 5`; `3` preset) | `stack_by = "project"` | One bar per local date, DST-correct width; today is hatched "so far". |
| `project-ranking` | Which projects cost most over those dates? | `ranked-bars`, `top_n = 5` | `daily` (shared, no second query) | `group_by = ["project"]` | Exact per-project sum; remainder in Other; Unknown never folded. |
| `model-ranking` | Which models cost most over those dates? | `ranked-bars`, `top_n = 5` | `models` (`calendar_days = 5`) | `group_by = ["model"]` | As above; upstream `(unknown)` shown as Unknown. |
| `subscription-activity` | How much subscription use happened, kept apart from spend? | `time-bars` | `subscription` (`calendar_days = 5`) | `stack_by = "channel"` | Response **counts**, unit `responses`. Never dollars. |
| `copilot-estimate` (optional, filtered profile only) | What did the one token-billed subscription provider cost at list price? | `time-bars`, `summary = "sum"` | `copilot-cost` (`calendar_days = 5`, `--channel subscription:github-copilot`) | none | List-price estimate for that single channel; never added to metered cost. |

Labelling decisions that are part of the design:

- Every cost title carries "list-price estimate". The cost metric declares `estimate = true` and `unknown_contribution = "nonnegative"`, so unpriced responses give "at least" rather than a complete figure.
- "Project" is the display name for one upstream workspace directory (R7). No project identity is inferred.
- The legacy profile's shared `subscription-estimate` card is **not carried into the filtered profile**. Upstream counts a plain flat-rate response as neither priced nor unpriced, so for Claude-only use that card shows an exact, complete zero. A title or a quality counter cannot stop that reading as "free".
- The filtered profile instead offers `copilot-estimate` from its own source, filtered to the one provider that bills by token. It never shares a stack with flat-rate channels.
- Flat-rate plan use is answered by `subscription-activity` (counts) and the quota cards in [section 3.3](#33-estimated-quota-contribution-cards). The legacy profile and its card stay unchanged; the cookbook states this limit.
- `calendar_days = 5` means today plus the four preceding local dates. Upstream's literal "five days ago" example can span six dates and is not copied.

### 3.1 Two profiles, chosen by capability

| Profile | Requires | Difference |
| :--- | :--- | :--- |
| Legacy: [`agent-activity.toml`](../../examples/charts/agent-activity.toml), kept unchanged | Any upstream build with `--layout columns` | `recent` groups by workspace and the card merges groups. |
| Filtered (proposed, beside it) | `agent-activity ledger capabilities --format json` succeeds (at least `9a15507`) | `recent` uses `--group-by none --channel metered --columns knownCostUSD,unpriced` with `dimensions = []`. Day sources add `--columns` for the columns they map. |

The proposed `recent` source in the filtered profile:

```toml
[[chart_sources]]
id = "recent"
command = [
  "agent-activity", "ledger", "series",
  "--format", "json", "--layout", "columns",
  "--bucket", "minute", "--group-by", "none", "--channel", "metered",
  "--columns", "knownCostUSD,unpriced",
  "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}",
]
format = "columns-v1"
window = "3h"
timezone = "America/New_York"   # example only; the owner chooses the zone

[chart_sources.mapping]
buckets = "/buckets"
interval_start = "/start"
interval_end = "/end"
groups = "/groups"
dimensions = []                  # group-none: one group with no keys
range_start = "/range/from"
range_end = "/range/to"
generated_at = "/generatedAt"
revision = "/generation"
notices = ["/coverage/statement"]
```

The metric table is unchanged from the legacy profile, including `omitted = "zero"`, because upstream still omits all-zero columns under `--columns`.

The author chooses the profile by running the capabilities probe themselves. Sidescope never probes, never rewrites a query after a failure, and never falls back automatically. A filtered profile against an old binary fails visibly as `nonzero-exit`.

New profiles set `group_by` explicitly to match `stack_by` on time-bars cards. The legacy form, `stack_by` with `group_by = []`, is valid and keeps working.

### 3.2 Claude plan quota: two different quantities

| | Estimated contribution | Observed utilization |
| :--- | :--- | :--- |
| What it is | Estimated percentage **points** of plan quota contributed by recorded Claude Code subscription responses in each bucket | The provider's reported bar level at each successful reading |
| Producer command | `agent-activity ledger series` with the four quota columns (`73a5063`) | `agent-activity quota series --format json` (`49f89cc`) |
| Numeric nature | Additive interval total; a window sum can exceed 100 | Gauge; not additive, never summed |
| Basis | Weights fitted at build time from one owner's readings; the provider publishes none | Readings about every 15 minutes; failed readings are left out |
| Identity | Model, workspace and the other ledger dimensions; **no account** | Account and window (`five_hour`, `seven_day`) |
| Not included | Resets, other clients on the account, models with no weight | Anything between readings |

The two are shown side by side under distinct titles and are never overlaid or reconciled. Ledger estimates carry no account key, and a plan's monthly price is not an account identity, so which observed account an estimate belongs to is **unknown** to the tool. No association key is added.

Source caveat, stated in the profile and cookbook: Claude Code use with a direct API key is currently classified upstream as subscription and may be counted in the estimate. The mapper and wrappers do not correct this.

### 3.3 Estimated quota contribution cards

| Card id | Question it answers | Kind and reduction | Source (window) | Grouping | Numeric meaning |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `quota-weekly-daily` | How much weekly quota did each recent date's work contribute, by model? | `time-bars`, `summary = "sum"` | `quota-estimate` (`calendar_days = 5`, day buckets, by model) | `stack_by = "model"`, `group_by = ["model"]` | Estimated weekly-quota points contributed per date. Not the bar level. |
| `quota-weekly-by-model` | Which models consumed the most weekly quota? | `ranked-bars`, `top_n = 5` | `quota-estimate` (shared) | `group_by = ["model"]` | Exact sum of estimated points per model. |
| `quota-weekly-by-project` (optional) | Which projects consumed the most weekly quota? | `ranked-bars`, `top_n = 5` | `quota-estimate-projects` (same query by workspace) | `group_by = ["project"]` | As above, per workspace directory. |
| `quota-five-hour-recent` | How fast am I contributing to the five-hour limit right now? | `time-series`, `summary = "sum"` | `quota-estimate-recent` (`window = "5h"`, 5-minute buckets, group none) | `group_by = []` | Estimated five-hour-quota points per 5 minutes. The window sum is a contribution, not the bar. |

The producer emits the percent columns as JSON numbers rounded to four decimal places. The `columns-v1` mapper refuses non-integral JSON numbers and that policy does not change. These sources therefore run an **explicit wrapper** shipped beside the profile:

- It runs the ledger query with `--columns` naming one percent column and its matching unweighted-count column, and emits a native `sidescope-chart-v1` answer.
- It reads each number's original decimal text exactly (Python standard library, `json.loads(..., parse_float=decimal.Decimal)`) and writes it as a decimal string. No value passes through a binary float.
- The metric is declared with unit `percentage points`, `quantity = "interval_total"`, `estimate = true` and `unknown_contribution = "nonnegative"`. The unit is deliberately not `percent`, so `bounded_percent = true` on these cards is refused as a card mismatch rather than drawn on a 0–100 axis.
- A bucket whose unweighted count is above zero is `partial`. Responses from models with no weight are thus a visible lower bound, never a complete zero. An omitted all-zero count column means zero; an explicit null means missing.
- `quotaWeights` becomes at most four short notices: the generation date with the fitted-from and fitted-through dates, the plan price basis, one count of grouped models with no weight, and a fixed statement that the weights are fitted, not published by the provider, and exclude resets and other clients. There is no per-model notice: a model with no weight is visible as its own `partial` series with a known value of zero, in `rows` and Details.
- The fitted-through date is shown as given. Its age produces no accuracy or staleness verdict.
- If the answer lacks `quotaWeights`, the wrapper exits non-zero. An old producer refuses the unknown columns itself. Either way the card is `unavailable`, never an empty success.

The response's fit comparison was in-sample. Nothing here claims accuracy for the estimate, and no owner reading is copied into fixtures or documents.

### 3.4 Observed quota utilization cards

| Card id | Question it answers | Kind and reduction | Source (window) | Grouping | Numeric meaning |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `quota-observed-<account>-<window>` (one per configured account and quota window) | Where did the provider say this account's bar for this one window stood? | `time-series`, `summary = "last"` | `quota-observed-<account>-<window>` (`window = "7d"` for `seven_day`, `"24h"` for `five_hour`) | `group_by = []` (the source has exactly one series) | One line. The headline is the last non-null reading as a number, always worded "last observed … as of" its reading instant. |

The producer's answer is nested per account, with irregular reading instants shared by that account's windows, numbers or nulls, reset entries and edges. It does not fit the aligned-column mapper, so this is a second explicit wrapper:

- **One source and one card per account and quota window.** A gauge card with more than one series has a series count as its headline, not a number, so each window gets its own source. Different quota windows have different denominators and are never summed or shown on one card.
- The wrapper takes exactly one account selector, `--account-key` (the producer's stable key, preferred) or `--account-label`, and a required `--window` of `five_hour` or `seven_day` that it passes to the producer. It emits at most one series. It still reads the retained journal only.
- Selector outcomes are fixed:

| Producer answer | Wrapper result | Card outcome |
| :--- | :--- | :--- |
| `accounts = []` (no journal) | Success, no series | `no-observations` |
| Selector matches no account in a non-empty list | Non-zero exit | `unavailable` |
| `--account-label` matches more than one account | Non-zero exit; labels are never merged | `unavailable` |
| Account found, selected window not reported | Success, no series, with a notice saying so | `no-observations` |
| Account and window found, no reading in range | Success, one series, no intervals | `no-intervals` |
| Every reading in range is null | Success, one series | `no-known-values` |

- The headline never says "current". A fresh, successful query of a week-old reading still reads "last observed … as of" that reading's instant. If the newest reading is null, the headline is the latest non-null reading and the card facts set `summary.newest_is_null`.
- Which ledger estimate, if any, belongs to this account stays unknown.
- Percent values are converted from their exact decimal text, as in [section 3.3](#33-estimated-quota-contribution-cards). Unit `percent`, `quantity = "gauge"`. `bounded_percent` stays `false` in the profile, because no provider bound on the reported value is established.
- A null reading stays null and is `missing`. Nothing is filled, carried forward or zeroed.
- `truncated = true` becomes a notice that earlier readings exist but were not returned. The seven-day retention is stated in the cookbook.
- `observed_through` stays unknown. Neither `generatedAt` nor the newest reading is treated as a collector watermark, because failed readings are simply absent.

Both quota wrappers refuse rather than trim. Each is the source's command: it runs under the existing runner (owned process group, timeout, 1 MiB stdout bound), starts exactly one producer command as its own child with no shell, and makes no other query. That child stays in the owned group. The wrapper also bounds **producer stdout while reading**, at 1 MiB before JSON decoding; the outer bound alone would not protect an internal pipe. Producer stderr flows to the outer runner's existing discard or bounded opt-in capture, not a second unbounded buffer. Pure conversion applies the same stdin bound. An input overflow refuses the conversion and terminates the child; timeout and cancellation still stop the whole owned group.

| Condition | Wrapper behavior |
| :--- | :--- |
| More than 2048 readings or buckets, 256 series or 100000 cells | Non-zero exit. Nothing is truncated, downsampled or merged. |
| A notice would exceed 512 characters, or more than 32 notices are needed | Non-zero exit. A caveat is never shortened or dropped to fit. |
| Producer `truncated = true` | One notice that history is incomplete. Each returned reading keeps `complete` quality, because it is a whole reading. |
| A number that is not valid decimal text, or misaligned arrays | Non-zero exit. |

Readings are instants, and dataset v1 has only positive intervals plotted at their centers. Encoding a reading as an interval would misplace it and imply the level held between readings. One small generic extension is therefore proposed. It is needed only by the observed cards; the `charts` verbs, the money cards and the estimate cards do not depend on it and can land first.

**Version and format.** The source key stays `format = "sidescope-chart-v1"`, which names the native family. That family accepts a payload `version` of `sidescope-chart-v1` or `sidescope-chart-v1.1` and refuses anything else as `unsupported-version`. Nothing probes or guesses a version. `charts schema` lists the accepted payload versions and the features each adds, and the guide tells authors to check that list before adopting the observed cards.

**Reserved fields.** `sampled` and `breaks` are reserved names. A payload labeled `sidescope-chart-v1` that contains either is refused as `invalid-dataset`; every other unknown optional field is still ignored. Builds from before this change cannot see a mislabelled payload, so the wrapper always labels its output `sidescope-chart-v1.1`, which those builds refuse. Existing `sidescope-chart-v1` answers without the new reserved fields keep their behavior.

**Validation under v1.1.** Any failure refuses the whole answer as `invalid-dataset`; nothing is repaired.

| Field | Rule |
| :--- | :--- |
| `sampled` | Boolean on a metric. `true` only with `quantity = "gauge"`. Metrics without it follow v1 rules exactly. |
| `breaks` | Allowed only beside one series' `values` and `quality` for a sampled metric. Integers, strictly increasing, each from 1 to the interval count minus 1. Length is therefore bounded by the existing interval limit. |

**How the wrapper builds intervals.** It passes the request's `{from}` and `{to}` to the producer (inclusive start, exclusive end) and declares exactly that as the dataset range, with no widening.

- Readings `at[0] < at[1] < …` inside the range become intervals `[at[i], at[i+1])`. The last, or only, reading gets `[at[n-1], to)`, which is positive because every reading is before `to`.
- No reading in range means no intervals.
- The wrapper exits non-zero on duplicate or non-increasing instants, an instant outside the range, a percent that is neither null nor a non-negative number, or a reset index that is out of range or not increasing. No upper bound on a percent is assumed.
- A break is listed before the reading at each reset entry after the first, and before any reading more than `--max-gap-minutes` after the previous one (whole minutes, 1 to 1440, default 45).

**Display of a sampled metric.** The interval end is ordering only, never a held duration. The readout, Details and accessible text always give the reading instant. The renderer's existing cap on individual point marks stays: at or below it every reading is marked; above it only segments between consecutive unbroken readings are drawn, plus a mark for any reading isolated by breaks or nulls. A segment joins two readings and claims nothing between them. The headline for `summary = "last"` is the latest non-null reading with its instant, whatever the source's freshness status.

| Addition (coined here) | Where | Meaning |
| :--- | :--- | :--- |
| `sampled = true` | A metric with `quantity = "gauge"` | Each value is a reading taken at its interval's **start** instant. The interval end only orders samples. Lines plot at the start instant; readouts, Details and accessible text give the instant; the right-edge "so far" downgrade does not apply; `time-bars` refuses the metric as a card mismatch. |
| `breaks = [index, …]` | One series' values for one metric | The line is not connected from the previous cell into each listed cell. Details rows and card facts carry `break_before`. |

The wrapper lists a break at every reset entry after the first, so a line never crosses a rollover or a reported reset-time change. It also lists a break where two readings are further apart than `--max-gap-minutes` (default 45), because the producer's account-level sampling-gap edges are not in the answer. That spacing rule is the wrapper's documented heuristic, not producer data.

Limits, stated rather than hidden:

- The core has no annotated markers. A reset shows as a break plus a notice counting reset-time changes; the reset time itself is not drawn.
- Edges other than resets (limit, availability, window added or removed) appear only as notice counts. `utilizationDelta` is not charted, and no delta is ever computed across a reset.
- Readings are about 15 minutes apart. The chart shows marks and segments between actual readings and claims nothing about the time between them.

### 3.5 Quota profile and feature checks

The quota cards ship as a third opt-in profile beside the other two. It is never merged into the legacy or filtered profile, so an older producer is never silently given quota cards. The author checks each feature by hand before adopting:

| Cards | Check the author runs | Passes when |
| :--- | :--- | :--- |
| Estimated contribution | `agent-activity ledger capabilities --format json` | The column descriptors list the four quota columns. Probe success alone is not enough. |
| Observed utilization | `agent-activity quota series --format json --window five_hour` | It exits 0. It reads the retained journal only. |

Each wrapper also has a pure conversion mode that reads a producer answer on standard input and runs no process. Fixtures go through that mode and then through `charts replay`.

## 4. Authoring procedure

The same steps serve a human and an agent.

1. **State the intent** as one row of the [section 3](#3-the-charts-this-design-proposes) table: question, kind, metric, window, grouping, summary, numeric meaning.
2. **Discover the vocabulary**: `charts schema` for keys, defaults, enums and limits. For agent-activity, run the upstream capabilities probe by hand to choose a profile.
3. **Write a candidate file** containing only `[[chart_sources]]` and `[[chart_cards]]`. Do not touch the owner's configuration.
4. **`charts check`** the candidate. This proves the configuration parses and shows the resolved window. It says `data: not-run`.
5. **`charts replay`** against a synthetic fixture or a capture, with an explicit clock. Assert the intent with `jq` ([section 5.6](#56-asserting-intent-with-shell-and-jq)).
6. **`charts run --source ID`** once per source when a real collector is available. Assert the same expressions against real data. Optionally capture for further offline iteration.
7. **Adopt deliberately**: copy the proven tables into the real configuration by hand, then run `charts check` with no `--config` to confirm the merged result.

Step 6 is optional where no collector exists. The report then truthfully shows only replay evidence.

## 5. The `charts` command family

The binary is `sidescope`. A candidate file holding only chart tables loads as it stands, because every other configuration field is optional.

### 5.1 Command table

| Command | Runs a source? | Needs data? | Purpose |
| :--- | :--- | :--- | :--- |
| `sidescope charts schema [--section NAME]` | No | No | Print the config keys, defaults, ranges, enums, pointer scopes, dataset limits, error kinds and report schema as JSON. Reads no configuration. |
| `sidescope charts check [--config FILE] [--at INSTANT]` | No | No | Validate configuration; resolve each source's window, zone and request; list each card's declared intent. Data and card checks are `not-run`. |
| `sidescope charts replay [--config FILE] (--at INSTANT --data SOURCE=FILE … \| --from-capture DIR)` | No | Saved stdout | Normalise each file as that source's answer and build every card fed by a named source. |
| `sidescope charts run [--config FILE] (--source ID … \| --all-sources) [--capture DIR]` | **Yes, once each** | Live | Execute exactly the named sources once through the existing runner, then build their cards. |

Shared options:

| Option | Applies to | Meaning |
| :--- | :--- | :--- |
| `--card ID` (repeatable) | check, replay, run | Restrict card output; default is every card of the selected sources. |
| `--details CARD [--offset N] [--limit N]` | replay, run | Include that card's full Details rows, paged ([section 5.5](#55-bounds-privacy-and-authority)). |
| `--show-argv` | check, replay, run | Include the resolved argument vector. Off by default. |
| `--format json\|text` | all | `json` is the contract and the default; `text` is a short human summary of the same report, not a second source of facts. |

`sidescope charts --help` and each verb's `--help` list these tables and link the cookbook. The top-level help gains one line pointing at `charts`.

Decisions:

- `--config` defaults to the owner's configuration for all verbs, because post-adoption verification needs it. The report records `config.origin` as `default` or `explicit`. Reading is safe, and `run` on the real configuration does only what the live application already does, once.
- `run` refuses to start without `--source` or `--all-sources`. There is no implicit "run everything". Giving both is a usage error.
- **Everything is resolved before anything happens.** Arguments, configuration, source and card selectors, data files and the capture directory are all checked first. Any failure ends the command with no process spawned and no file written.
- An explicit `--config` path that is missing or unreadable is `invalid`. Only the default path may be absent, and then the scope is empty.
- **An empty scope is never green.** If no source or no card is selected after resolution, the status is `invalid` with error `empty-scope`.
- `--card` narrows output to cards of the selected sources. A card id that is unknown, or whose source is not selected or has no `--data`, is an error, not an omission. A repeated `--source`, `--card` or `--data` source id is a usage error.
- In `replay`, only sources given data are evaluated; other sources and their cards are absent from the report. A data file that is unreadable or larger than the 1 MiB stdout bound is an error before evaluation. A readable file that fails to parse is a normal source finding.
- `schema` reads no configuration. The `charts` control path never reads source credentials. Explicitly executed commands run unsandboxed and may use inherited credentials under their own contracts; the example ledger and quota-journal commands make no provider call.
- `doctor`, `--demo`, `--report` and `--browser` are unchanged and gain no execution.
- All verbs are headless: no display, compositor or GTK initialisation, and no unrelated provider call. The binary still links the GTK system libraries; no GTK-free packaging is promised.

### 5.2 Clock semantics

| Verb | Clock | Recorded as |
| :--- | :--- | :--- |
| `check` | `--at` if given, otherwise one wall-clock capture | `clock.origin`: `explicit` or `wall` |
| `replay` | `--at` with `--data`, or each source's recorded clocks with `--from-capture`; there is no default | `explicit` or `recorded` |
| `run` | One wall-clock capture per source request; `--at` is refused | `wall` |

Each source carries two clocks in the report:

| Field | Meaning | `run` | `replay` |
| :--- | :--- | :--- | :--- |
| `clocks.request_at` | The instant that resolved the window and drives right-edge downgrade during normalization | Captured just before that source's spawn | `--at`, or the capture's recorded value |
| `clocks.model_at` | The instant used to build cards | Captured when that source's attempt ended | `--at`, or the capture's recorded value |

The two differ in `run` when a command takes seconds, and sources in one run have different clocks. `--from-capture` reuses each source's recorded pair, so a multi-source run can be rebuilt exactly. The top-level `clock` is the invocation instant for `run`, the explicit instant for ordinary replay, or the recorded invocation instant from the capture manifest for `--from-capture`; replay never adds a hidden wall clock.

Replay is proof of construction only. It computes no live age: its `freshness` check is always `not-run`, whatever the clocks say. `run` snapshots are that invocation's own one-shot results, not a view into a running window's in-process state.

A one-date window resolved at local midnight can be Deferred. `run` retries once after one second, as the runtime does. If the request is still deferred, the source's request check is `unknown` and the overall status is `undetermined`.

### 5.3 Report shape

Schema id `sidescope-chart-report-v1`. Fields are only added within v1; a rename or meaning change needs v2. Consumers must ignore unknown fields.

Minimal successful replay (abridged to one source and one card):

```json
{
  "schema": "sidescope-chart-report-v1",
  "command": "replay",
  "status": "ok",
  "clock": { "at": "2026-11-20T15:00:30Z", "origin": "explicit" },
  "config": { "path": "candidate.toml", "origin": "explicit", "valid": true },
  "checks": [
    { "id": "config", "status": "pass" },
    { "id": "request", "status": "pass" },
    { "id": "execution", "status": "not-run", "why": "replay does not run commands" },
    { "id": "dataset", "status": "pass" },
    { "id": "cards", "status": "pass" },
    { "id": "freshness", "status": "not-run", "why": "replayed data has no live age" },
    { "id": "appearance", "status": "not-run", "why": "no pixels are produced" }
  ],
  "sources": [{
    "id": "recent",
    "data_origin": "replay",
    "clocks": { "request_at": "2026-11-20T15:00:30Z", "model_at": "2026-11-20T15:00:30Z" },
    "request": { "window": "3h", "from": "2026-11-20T12:00:30Z",
                 "to": "2026-11-20T15:00:30Z", "timezone": "America/New_York",
                 "executable": "agent-activity", "argc": 21 },
    "attempt": null,
    "dataset": { "status": "valid",
                 "range": { "start": "2026-11-20T12:00:00Z", "end": "2026-11-20T15:01:00Z" },
                 "intervals": 181, "series": 1, "dimensions": [],
                 "metrics": [{ "id": "metered_cost", "unit": "USD", "quantity": "interval_total",
                               "estimate": true, "unknown_contribution": "nonnegative" }],
                 "generated_at": "2026-11-20T15:00:31Z", "observed_through": null,
                 "revision": "41", "notices_count": 1,
                 "notices": ["Synthetic example: no collection watermark asserted."] },
    "error": null
  }],
  "cards": [{
    "id": "recent-cost", "source": "recent", "kind": "time-series",
    "intent": { "metric": "metered_cost", "summary": "sum", "group_by": [],
                "stack_by": null, "top_n": null, "bounded_percent": false },
    "outcome": { "kind": "plotted" },
    "summary": { "reduction": "sum", "exact": "4.2137", "quality": "partial",
                 "bound": "at-least", "known_subtotal": "4.2137", "signed": false },
    "intervals": { "total": 181, "complete": 180, "partial": 1, "missing": 0,
                   "open_right_edge": true },
    "groups": { "total": 1, "named_shown": 1, "other_members": 0, "unknown_shown": 0,
                "capacity": null, "legend_shown": 1, "legend_overflow": 0 },
    "rows": [{ "group": [], "role": "named", "members": 1, "exact": "4.2137",
               "known_subtotal": "4.2137", "quality": "partial" }],
    "details": { "grain": "interval-group", "rows_total": 181, "included": false },
    "display": { "title": "Metered list-price estimate (≈3h · 1 min)",
                 "headline": { "label": "Total", "value": "≥ $4.21", "note": "incomplete" },
                 "banner": null, "footnote": null,
                 "accessible_name": "…" },
    "drawing": { "authoritative": false,
                 "axis": { "min": 0.0, "max": 0.1, "ticks": [0.0, 0.05, 0.1] } }
  }]
}
```

Minimal failure from `run`: one source failed, and a sibling native source is valid but one of its cards is mismatched.

```json
{
  "schema": "sidescope-chart-report-v1",
  "command": "run",
  "status": "findings",
  "clock": { "at": "2026-11-20T15:00:00Z", "origin": "wall" },
  "config": { "path": "candidate.toml", "origin": "explicit", "valid": true },
  "checks": [
    { "id": "config", "status": "pass" },
    { "id": "request", "status": "pass" },
    { "id": "execution", "status": "fail" },
    { "id": "dataset", "status": "not-run", "why": "dependency failed: execution, for one source" },
    { "id": "cards", "status": "fail" },
    { "id": "freshness", "status": "not-run", "why": "dependency failed: execution, for one source" },
    { "id": "appearance", "status": "not-run", "why": "no pixels are produced" }
  ],
  "sources": [
    { "id": "recent", "data_origin": "run",
      "attempt": { "latency_ms": 41, "exit": 2 },
      "dataset": { "status": "absent" },
      "error": { "kind": "nonzero-exit", "text": "exit status 2", "stderr_excerpt": null } },
    { "id": "latency", "data_origin": "run",
      "attempt": { "latency_ms": 12, "exit": 0 },
      "dataset": { "status": "valid", "intervals": 60, "series": 3 },
      "error": null }
  ],
  "cards": [
    { "id": "recent-cost", "source": "recent",
      "outcome": { "kind": "unavailable", "source_error": "nonzero-exit" } },
    { "id": "p95", "source": "latency",
      "outcome": { "kind": "refused", "reason": "card-mismatch",
                   "subject": "metric", "wanted": "p95_ms", "available": ["p50_ms", "p99_ms"] } }
  ]
}
```

### 5.4 Enumerations and exit codes

| Field | Values |
| :--- | :--- |
| `status` | `ok`, `findings`, `invalid`, `undetermined` |
| `checks[].id` | `config`, `request`, `execution`, `dataset`, `cards`, `freshness`, `coverage`, `appearance` |
| `checks[].required` | `true` or `false` (omitted in the examples). Only a required check ending `unknown` makes the status `undetermined`. `coverage`, the collector watermark, is never required: it is `unknown` whenever the producer asserts none, and a valid run still exits 0. |
| `sources[].result` | The same check ids per source. Each top-level check is the worst result across selected sources, so one failed source can never leave an aggregate at `pass`. |
| `errors[].id` (report level) | `usage`, `config-unreadable`, `invalid-config`, `unknown-source`, `unknown-card`, `conflicting-selection`, `empty-scope`, `data-unreadable`, `capture-path`, `capture-write`, `report-too-large` |
| `checks[].status` | `pass`, `fail`, `not-run`, `unknown` |
| `sources[].data_origin` | `none` (check), `replay`, `run` |
| `sources[].error.kind` | `invalid-config`, `missing-executable`, `spawn`, `nonzero-exit`, `timeout`, `output-bound`, `cancelled`, `parse`, `unsupported-version`, `invalid-mapping`, `invalid-dataset`. These are stable ids for the existing runtime kinds; `error.text` is human wording and not a contract. The twelfth kind, `card-mismatch`, is reported per card. |
| `cards[].outcome.kind` | `not-evaluated` (check), `plotted`, `no-observations` (the answer has no series), `no-intervals` (series exist but no interval falls in the card's range), `no-known-values` (cells exist and every one is null), `refused`, `unavailable`. Data that is all zero is `plotted`. |
| `outcome.reason` when refused | `card-mismatch`, `capacity` |
| `summary.quality` | `complete`, `partial`, `missing` |
| `summary.bound` | `exact`, `at-least`, `incomplete` |
| `summary.exact` | The reduced value as a decimal string when quality is `complete` or `partial`; under `at-least` it is the known lower bound within the metric's declared estimate, not a guarantee about an invoice or the provider's observed quota bar. It is `null` when quality is `missing`, so an incomplete total can never be read as a complete one. |
| `summary.known_subtotal` | For a sum, the exact sum of present values as a decimal string, or null if none are present. A real observed zero is `"0"`, not null. For a gauge it is null: `summary.exact` is the selected reading, never a sum of readings. The rounded headline stays in `display`. |
| `rows[].role` | `named`, `other`, `unknown` |

| Exit | Meaning |
| :--- | :--- |
| 0 | `ok`: every required check passed for a non-empty scope. `no-observations`, `no-intervals` and `no-known-values` are truthful successes; the caller asserts which one it expected. |
| 1 | `findings`: a source failed, a dataset was refused, or a card was refused or unavailable. |
| 2 | `invalid`: usage error, unreadable or invalid configuration, bad selection, empty scope or an oversized report. A JSON report with an `errors` array is always printed under the default format, so every verb yields exactly one JSON document. |
| 3 | `undetermined`: a required check ended `unknown`, for example a request still deferred. |

Exit 0 never means "the chart is what you intended". It means nothing failed, which is why step 5 of the procedure asserts intent explicitly.

### 5.5 Bounds, privacy and authority

| Concern | Rule |
| :--- | :--- |
| Authority | `run` executes with the application's authority and inherited environment, unsandboxed, as live mode does. Help and the guide say so. The command comes only from the configuration file, never from data or report content. |
| Execution scope | Exactly the named sources, once each, with existing timeout, stdout bound, group kill and reap. No refresh loop, no backoff retry, no unrelated provider. |
| Never automatic | No sync, install, start, authentication or deployment of any collector. No query rewriting after failure. |
| Raw output | Never included in a report and never echoed to standard output. Written to disk only under `--capture DIR`. |
| Capture contents | One `<source-id>.stdout` for each source whose command exited 0 within the 1 MiB bound, byte-exact, **even if the output then fails to parse or map**; the file may hold malformed JSON and is not a valid snapshot by itself. A non-zero exit, timeout, bound overflow, spawn failure or cancellation writes no stdout file; nothing is ever stored truncated. `manifest.json` (schema `sidescope-chart-capture-v1`) holds the original invocation clock and each source's id, resolved window and zone, both clocks, byte count, `captured` flag and outcome or error id, plus a top-level `complete` flag. No argv, stderr or environment is recorded. |
| Capture writing | Each stdout file is written when its source becomes terminal; the manifest is written once, last. Every file is created exclusively, so an existing file is never overwritten even if it appears after the preflight. The directory is created owner-only and files are owner read-write only. |
| Replay from a capture | Needs a valid configuration containing every source id in the manifest; otherwise `unknown-source`. Sources with a stdout file are normalized again, reproducing any parse or mapping refusal. Sources recorded as failed are reported `unavailable` with the recorded error id. Recorded clocks reproduce construction; they are not proof of anything current. |
| Facts and provenance | In every report, `dataset` and the card facts are semantic facts about the data. `data_origin`, `attempt`, `clocks`, `checks` and `sources[].result` are provenance about this invocation. Replay reproduces the first and never imitates the second. |
| Check precedence | Per source, a check after a failed one is `not-run` with a dependency reason. Across sources an aggregate is `fail` if any source failed, else `unknown` if any is unknown, else `not-run` if any did not run, else `pass`. Status is `findings` if any check failed; otherwise `undetermined` if a required check is `unknown`; otherwise `ok`. `coverage` is never required and never causes exit 3. `freshness` passes only for a source whose attempt in this invocation succeeded, judged by that attempt's age. |
| Capture failures | The directory must be creatable and none of the target files may exist; both are checked before any spawn (`capture-path`). A write failure after execution is `capture-write`: the report is still printed with status `findings`, files already written are kept, and the manifest, if it can be written, says `complete: false` and marks each source's `captured` flag. Nothing is retried or deleted. A capture without a manifest is refused by `--from-capture`. A report that cannot be serialized within its bound is still the minimal `report-too-large` report, never partial output. |
| Capture privacy | A capture is real data kept at the caller's request. Nothing else is written anywhere, and the cookbook says to keep captures out of version control. |
| Argument vector | Omitted unless `--show-argv`; the default shows executable basename and argument count. |
| Stderr | An excerpt appears only when the source sets `capture_stderr = true`, under the existing 8 KiB bound and sanitizing. |
| Persistence | No log, cache or history is written without `--capture`. Replay and run otherwise hold data in process only. |
| Default card content | Each card carries `rows`: one structured entry per plotted group or ranked row, at most 16 including Other and every Unknown. Time-card rows include every plotted series, even when its legend entry is elided; legend counts are separate. Each entry has the retained group tuple (null for Other), `role`, member count, exact reduced value, nullable known subtotal and quality. Folded members are inspectable in Details, not duplicated as displayed rows. |
| Details rows | Only under `--details CARD`, with `--offset` (default 0) and `--limit` (default 500, maximum 5000); offset must be nonnegative and limit positive. Each row is structured: interval bounds or sample instant, retained group tuple, `shown_as` (`named`, `other`, `unknown`, or null if the card refused to plot), exact value or null, nullable known subtotal, quality, unit and `break_before`. |
| Row grain | Preserve the existing full Details row set before top-N folding, with no extra Other aggregate row. `details.grain = "interval-group"` for time cards: one row per interval or reading per retained group. `details.grain = "group"` for ranked cards: one range-reduced row per retained group, or its actual selected observation for a gauge. Tuple identity and `shown_as` expose Other membership and Unknown without parsing labels. |
| Adding rows up | Only for `sum` cards: present time-card cell values, or ranked Details rows' known subtotals, sum to the card's known subtotal. Displayed groups' known subtotals give the same result. Never add Other to its own members, and preserve null when no values are known. A gauge `last` value is one reading, not a period sum. |
| Order | Sources and cards in configuration order; `rows` in plot or ranking order. Time Details are ordered by interval start then retained tuple; ranked Details retain full value order with a stable-id tie-break. Paging is stable for the same data and clocks. |
| Report size | A report is at most 1 MiB. If it would be larger, the command prints a small report with error `report-too-large` naming the card and exits 2. It never prints a truncated report under a passing status. |
| Truthful projection | A page always states `rows_total`, `offset`, `rows_returned` and `complete`. Nothing is silently truncated or presented as full. |

### 5.6 Asserting intent with shell and `jq`

No assertion language is added. The report is plain JSON and `jq -e` already sets the exit status. A `select` that matches no card produces no output, which `jq -e` reports as failure, so each line below also proves the card exists. The examples use Bash here-strings and invented fixture values; `--card` and `--details` narrow a report to one card for a minimal probe.

```bash
r=$(sidescope charts replay --config candidate.toml \
      --at 2026-11-20T15:00:30Z \
      --data recent=fixtures/recent.json --data daily=fixtures/daily.json)

# The card I meant exists, is plotted, and reduces the metric I meant, ungrouped.
jq -e '.cards[] | select(.id=="recent-cost")
       | .outcome.kind=="plotted" and .intent.metric=="metered_cost"
         and .intent.group_by==[] and .summary.reduction=="sum"' <<<"$r"

# Wrong grouping: a ranking must rank by project, with Unknown kept apart from Other.
jq -e '.cards[] | select(.id=="project-ranking")
       | .outcome.kind=="plotted" and .intent.group_by==["project"]
         and ([.rows[] | select(.role=="named")] | length)<=5
         and ([.rows[] | select(.role=="unknown")] | length)==1
         and (.rows[] | select(.role=="other") | .members==3 and .exact=="1.25")' <<<"$r"

# Membership: the Unknown group is listed on its own, never among Other's members.
sidescope charts replay --config candidate.toml --at 2026-11-20T15:00:30Z \
    --data daily=fixtures/daily.json --card project-ranking --details project-ranking |
  jq -e '.cards[0].details.rows
         | all(.[]; (.group==[""]) == (.shown_as=="unknown"))'

# Quality: unpriced responses must make the total a lower bound, never "complete".
jq -e '.cards[] | select(.id=="recent-cost") | .summary.bound=="at-least"' <<<"$r"

# No data is not a refusal, and a refusal is not no data.
jq -e '.cards[] | select(.id=="recent-cost") | .outcome.kind!="refused"' <<<"$r"

# Replay must never be mistaken for a live proof.
jq -e '.checks[] | select(.id=="freshness") | .status=="not-run"' <<<"$r"
```

Source notices are included as their complete bounded plain-text array, not merely a count. This keeps coverage caveats and quota fit dates readable without the window; it does not promote prose notices to typed numeric measurements.

## 6. Shared architecture

### 6.1 What already does this job, and why it is insufficient

| Existing piece | What it covers | Gap |
| :--- | :--- | :--- |
| `doctor` | Config validity, executable presence | Runs nothing; builds no card; text output only. |
| Info popover and Details table | Truthful human diagnostics | Human strings; reachable only through the window. |
| `build_model` and `ChartCardModel` | The single place card semantics are computed | Not serializable; exact values already flattened to strings and floats; refusal is a message. |
| Runtime and process runner | Bounded execution and error kinds | No one-shot public entry; snapshots are in-process. |

Nothing existing emits machine-readable card semantics. No new runner, normalizer or reduction is needed, only a typed middle and a command surface.

### 6.2 Card facts as the single boundary

Model building is split into two pure steps inside the existing module:

```
(card, snapshot, now) ──reduce──▶ CardFacts ──present──▶ ChartCardModel ──▶ GTK drawing
                                      │                        │
                                      └── serialise ───────────┴── display strings, verbatim
                                                    ▼
                                             charts report
```

- `CardFacts` is the only output of reduction. The existing `ChartCardModel` becomes a function of `CardFacts` alone, with no second look at the snapshot. Agreement is structural, not tested-for.
- `CardFacts` carries the outcome as an enum (plotted, no observations, refused with typed reason, unavailable). It also carries exact decimal reduced values and known subtotals, quality and bound, signedness, interval counts, right-edge openness, group membership (named, Other with member count and exact remainder, each Unknown, capacity and what exceeded it), legend overflow and the full Details row set.
- The report's `display` block copies strings from the model's existing getters without interpretation. The `drawing` block copies axis floats and is marked `authoritative: false`.
- `charts run` uses the existing runtime and process runner under a **one-shot policy** (coined here); it adds no second executor. Only the selected sources are scheduled, each exactly once. After a source's first terminal attempt (success, failure, timeout, bound exceeded, or a deferred request retried once) the scheduler issues no refresh, no backoff retry and no queued manual run for it. This holds while slower sources are still running. When the last selected source is terminal, the runtime shuts down through the existing group-kill and reap path before the report is printed.
- `charts replay` calls the existing normalizer and builds a snapshot marked as replayed. Neither verb contains process or math code of its own.
- The one-shot path proves construction for a single answer. It does not exercise the runtime's replacement of an earlier answer; that is tested separately ([section 8.1](#81-compatibility-tests)).

### 6.3 Three kinds of number

| Kind | Where | Type | Use |
| :--- | :--- | :--- | :--- |
| Exact | `summary.exact`, `known_subtotal`, Details values | Decimal strings | Assertions and totals. Never floats. |
| Display | `display.*` | Human strings, rounded | What a person reads; not for arithmetic. |
| Drawing | `drawing.*` | `f64` | Geometry only; explicitly non-authoritative. |

### 6.4 One schema source

Chart source, card and mapping keys, defaults, ranges and enums are declared once in descriptor tables. Chart validation, `charts schema`, chart help and the generated key matrix read those tables. Tests compare accepted chart keys and described chart keys in both directions. This is not a second hand-kept catalog or a reflection framework.

The schema covers chart configuration, native payload versions and the report, not the unrelated weather, hardware or AI settings. The existing whole-application loader still validates a merged configuration; non-chart keys are documented in the user guide. No migration of every application's validator is a prerequisite for the chart tools.

## 7. Upstream responses R1–R7

| Id | Upstream built | Disposition here |
| :--- | :--- | :--- |
| R1 | `range.requested`, and bucket `state: "open"` only | **No core change.** The mapper ignores the new fields. Sidescope already knows its own request, and Info already shows requested against returned range. The existing right-edge rule (interval ending after request end becomes partial) is at least as strict as `open`. Absent `state` is treated as unknown finality, never as closed. |
| R2 | `lastCommitAt`; no watermark | **No new generic field.** It is not mapped to `observed_through`, which stays unknown, and never drives a stale verdict. Freshness stays based on last success age. The profile forwards the coverage statement as a caveat, not the numeric backlog counters. Those remain in producer output and can be inspected in a deliberate raw capture or surfaced as bounded notices by a wrapper; none proves a collection watermark. |
| R3 | `capabilities` probe | **Authoring input, not a runtime dependency.** The author runs it to choose a profile. Exit 2 with an unknown-command message means "use the legacy profile"; any other failure is a real fault to investigate. No version parsing. |
| R4 | Twelve synthetic fixtures | **Import consumer-safe copies** with provenance noted as upstream `9a15507`, and drive them through `charts replay` ([section 8](#8-compatibility-and-documentation)). No account captures. |
| R5 | `--group-by none`, exact filters | **Used by the filtered profile** for `recent`. A no-match answer is a successful `no-observations` or a zero, never an error. The coverage notice is workspace-wide and the guide says it is not filter-specific completeness. |
| R6 | `--columns`; no batch query | **Used by the filtered profile.** `omitted = "zero"` stays. Sources remain independent; `revision` is reported per source as a diagnostic, with no cross-source consistency loop. Daily bars and project ranking keep sharing one source. |
| R7 | One workspace directory is one project | **Adopted as labeling.** No project registry and no fuzzy merge. The guide notes the known upstream defect that some registered workspace folders are counted separately. |

A labeled-metadata key in the mapper was considered for `lastCommitAt` and rejected. A commit time beside "observed through: unknown" invites the idle-versus-stopped inference upstream says cannot be made. Someone who wants it can emit a plain notice from an explicit wrapper.

The quota additions after R1–R7 are handled as follows; the design is in [section 3.2](#32-claude-plan-quota-two-different-quantities) to [section 3.5](#35-quota-profile-and-feature-checks).

| Upstream addition | Disposition here |
| :--- | :--- |
| Estimated quota columns and `quotaWeights` (`73a5063`) | **Explicit wrapper, no mapper change.** Exact decimal text is preserved, the unweighted count drives quality, and the weights' basis is carried as notices. Cards are additive points, never a bounded gauge. |
| Observed quota readings (`49f89cc`) | **Explicit wrapper plus the `sidescope-chart-v1.1` sampled-gauge and break additions.** One source per configured account; windows stay separate; nulls, resets, gaps and truncation stay visible. |
| Link between the two | **None exists upstream and none is invented.** The cards sit side by side; the association is unknown. |

Unbuilt upstream, and therefore out of scope: closed or provisional states, a watermark, batch queries, separate project ids, dollar figures for flat-rate plans, published quota weights, and an account key on ledger estimates.

## 8. Compatibility and documentation

### 8.1 Compatibility tests

Tests drive the public `charts replay` boundary with the imported fixtures and assert on the report, so they exercise what an agent uses.

| Fixture | What the test pins |
| :--- | :--- |
| `series-empty.json` | `no-observations`, exit 0, not zero and not refused. |
| `series-zero-only.json` | Omitted all-zero columns map to exact zero and complete quality under `omitted = "zero"`. |
| `series-unpriced.json` | Partial quality and `bound: "at-least"`. |
| `series-subscription.json` | Subscription metrics stay separate; the metered summary is unaffected. |
| `series-late-correction-before/after.json` | Replayed separately, each maps to its own totals. Replacement itself is proved at the runtime's shared publication path: one source answers with "before" then "after", and the snapshot a consumer reads equals the second answer alone. Independent replays do not prove this. |
| `series-dst-23-hour-day.json`, `series-dst-25-hour-day.json` | Real-duration intervals and correct calendar windows. |
| `series-open-bucket.json` | Right-edge partial with `open_right_edge: true` at a chosen `--at`. |
| `series-objects.json` | The objects layout is refused by the columns mapping with a typed error, not drawn. |
| `error-too-many-buckets.json` | A refusal body is never a successful dataset. The command-line path (stderr and exit 2) is `nonzero-exit`. |
| `capabilities.json` | The shipped profiles' metric declarations (unit, estimate, completeness column) agree with the descriptors. A copy without the quota descriptors fails the quota profile's documented check. |
| `series-quota-percent.json` (upstream, invented values) | Through the estimate wrapper's conversion mode: `0.5` and `2` arrive as the exact decimals `0.5` and `2`, the omitted unweighted column gives complete cells, and the weights' basis appears in notices. |
| Estimate cases (new, invented here) | A model with no weight gives `partial` and `at-least`, not a complete zero. A window summing above 100 is plotted with no 0–100 axis. A missing `quotaWeights` block is a non-zero exit. An old fitted-through date is shown as given. |
| Observed cases (new, invented here) | Two accounts with two windows each stay unmerged. A reset breaks the line and no delta is derived across it. Null readings stay missing. A long gap breaks the line. `truncated` yields a notice. An empty journal is `no-observations`. A newest null reading leaves the headline at the last known reading with its instant. |

Upstream promises unchanged names, types and meanings with additive fields. The mapper's tolerance of unknown fields is what makes that safe, and the tests above hold it in place. Byte-identical old answers are not assumed.

### 8.2 Documentation set

| Piece | Content |
| :--- | :--- |
| Guide [section 7](#7-upstream-responses-r1r7) | Gains a short "author and verify a chart" path linking the cookbook; corrects any statement that these tools do not exist once they do. |
| Authoring cookbook (new) | The [section 4](#4-authoring-procedure) procedure worked three times: the agent-activity money dashboard; the quota cards with their wrappers, feature checks and limits; and the already shipped `synthetic-latency.sh` example from the [examples](../../examples/charts/README.md) (gauge, `summary = "last"`, native format), so the tooling is visibly not cost-specific. |
| Key matrix (generated) | Every key with default, range, enum and one example; a pointer-scope table (absolute, per bucket, per group); quantity to reduction; `omitted` and quality counter outcomes. |
| Troubleshooting flow | Starts from `status` and `outcome.kind` and branches by error kind to the single next command. |
| Profile notes | Legacy versus filtered versus quota, the probe and what its success does not establish, why the shared subscription estimate is not carried forward, estimated points versus observed readings, the fitted-weights basis and the API-key classification caveat, R7 project meaning, and "adoption is copying tables by hand". |
| Dataset reference | Gains the `sidescope-chart-v1.1` additions (`sampled`, `breaks`) with one worked example. |
| `--help` | Command and option tables, the authority warning on `run`, and links to the above. |

## 9. Alternatives considered

| Alternative | Verdict | Reason |
| :--- | :--- | :--- |
| Extend `doctor` to run sources | Rejected | Changes a command people rely on as non-executing; mixes unrelated provider checks into chart proof. |
| Serialise `ChartCardModel` directly | Rejected | Its values are already display strings and floats; refusal is a message. It would force string matching. |
| A separate inspector that recomputes reductions | Rejected | Two implementations of the same math will drift; violates P2. |
| Screenshots or a headless compositor as the proof | Rejected | Proves pixels, not meaning; heavy; explicitly not required. |
| Debug server, daemon or agents-only protocol | Rejected | Out of bounds, and a one-shot CLI covers the need. |
| A bespoke assertion language | Rejected | `jq -e` over a stable schema is sufficient and already known. |
| Automatic capability probing and profile switching | Rejected | Hidden execution and silent query rewriting; contradicts explicit adoption. |
| One verb with mode flags instead of `check`/`replay`/`run` | Rejected | Separate verbs make "this one executes" visible in the command itself. |
| Default clock for `replay` | Rejected | A hidden wall clock makes fixture results change with the day. |
| Generic labeled-metadata mapper key for `lastCommitAt` | Rejected for now | See [section 7](#7-upstream-responses-r1r7); a wrapper notice covers a real need without core change. |
| Let the columns mapper accept fractional JSON numbers | Rejected | Weakens the exact-decimal policy for every source to serve one producer; a wrapper keeps the exact text. |
| Encode readings as ordinary intervals between samples | Rejected | Misplaces each reading at an interval center and implies the level held until the next one. |
| Insert fake missing cells to break a line at a reset | Rejected | Fabricates a failed reading in Details; a typed break says what happened. |
| Dataset v2 with markers, overlays and irregular per-series time axes | Rejected | Far more than the two behaviours needed. Reset markers are named as a limit instead. |
| Overlay estimated contribution on an observed account's bar | Rejected | No account link exists; an overlay would assert one. |
| Keep the shared subscription estimate card with a clearer title | Rejected | For flat-rate use it is an exact, complete zero; wording cannot undo that. |

## 10. Costs and risks

| Item | Cost or risk | Mitigation |
| :--- | :--- | :--- |
| Splitting model building around `CardFacts` | Touches the most heavily tested module | The existing model tests stay as the regression net; behavior is meant to be identical. |
| Report schema becomes a public contract | Later changes are constrained | Additive-only v1; `display` and `drawing` are documented as non-contractual text and geometry. |
| `run` is a new way to execute owner commands | Misuse by an agent on the real config | Explicit source selection, once only, no capture unless asked; authority stated in help. |
| Captures contain real usage data | Privacy if committed | Opt-in directory, no overwrite; the cookbook says to keep captures out of version control. |
| Fixture copies age | Upstream may move on | Provenance commit recorded; additive-tolerance tests fail loudly on meaning changes. |
| Semantic proof mistaken for visual proof | Overclaiming | The `appearance` check is permanently `not-run` in every report. |
| Estimated quota read as the real bar | A fitted, single-owner estimate taken for provider truth | Unit `percentage points`, "estimated contribution" titles, weights-basis notices, and no overlay with observed readings. |
| Wrappers are new code outside the core | Two producer-specific scripts and a Python dependency for the quota profile only | They are examples with a pure conversion mode covered by fixtures; the core gains only the `v1.1` additions. |
| The `v1.1` additions touch the renderer | Line placement and breaking change for sampled metrics | Opt-in by version string; `v1` behavior is untouched and stays under the existing tests. |
| The gap rule is a heuristic | A long pause between readings may be normal or a fault | The threshold is an explicit wrapper argument and is documented as not being producer data. |

## 11. Acceptance criteria

Each is observable from outside the process.

1. `charts schema` lists every source, card and mapping key chart validation accepts, with defaults, ranges and cross-field rules; a test fails on omitted or invented chart keys. Unrelated application configuration remains outside this schema.
2. `charts check` on a valid candidate exits 0 with `dataset` and `cards` checks `not-run` and every card `not-evaluated`. An invalid candidate exits 2 with a JSON report naming the key.
3. `charts replay` with the same inputs and `--at` produces byte-identical reports on repeated runs.
4. For every imported fixture, the report matches [section 8.1](#81-compatibility-tests), and the GUI model built from the same snapshot shows the same headline, badges and row set, because both derive from one `CardFacts` value.
5. A native source with one mismatched card reports that card `refused` with `reason: "card-mismatch"`, its siblings `plotted`, the source `valid`, and exit 1.
6. An over-capacity ranked card reports `refused` with `reason: "capacity"`. An empty answer reports `no-observations`. Neither is distinguished by message text.
7. `charts run --source X` spawns exactly one process group for `X` and none for other sources or providers. `charts run --all-sources` with one fast and one slow source spawns each exactly once, even when the fast source's refresh interval elapses first. Without a source selection, with an unknown source or card, or with an existing capture file, it exits 2, spawns nothing and writes nothing.
8. No verb initialises a display, and `--demo`, `--report`, `--browser` and `doctor` still construct no chart runtime.
9. No report contains raw stdout. Argv appears only with `--show-argv`, and no file is written without `--capture`.
10. A `--details` page reports `grain`, `rows_total`, `offset`, `rows_returned` and `complete` consistently. A sum card's known subtotal equals the sum of present time-cell values, or of ranked Details rows' known subtotals, across all pages. Plotted groups' known subtotals also match it; Other equals its members' known subtotals. No values known yields null, not an observed zero. No check counts both Other and its members, and no period sum is asserted for a gauge.
11. The filtered profile and the legacy profile both pass `check`. The filtered `recent` card replayed against a group-none fixture is `plotted` with `groups.total == 1`.
12. The cookbook's commands, run as written against shipped fixtures, produce the outputs the cookbook shows.
13. Every verb prints exactly one JSON report under the default format, including for usage errors and a missing explicit `--config`. An empty scope exits 2. A report that would exceed 1 MiB exits 2 with `report-too-large` and no partial card output.
14. An empty series list reports `no-observations`; series with no interval in range report `no-intervals`; cells that are all null report `no-known-values`; an all-zero dataset reports `plotted`. These are four distinct typed outcomes. A `sum` summary with any missing input has `exact: null`; `known_subtotal` holds its present-value sum, or null if no values were present. A gauge `last` summary takes the latest non-null cell with that cell's own quality and instant: earlier nulls do not invalidate it, and a newer null sets `summary.newest_is_null` without changing the value.
15. A valid `run` whose producer asserts no watermark exits 0 with `coverage` `unknown` and not required. A run with one failed source never reports an aggregate `freshness` of `pass`.
16. `replay --from-capture` of a multi-source capture yields the same **semantic facts** as the `run` that wrote it: each source's `dataset` block and each card's `outcome`, `intent`, `summary`, `intervals`, `groups`, `rows` and Details. Provenance differs and must: `data_origin` is `replay`, `attempt` is null, `execution` and `freshness` are `not-run`, and no display text presents the data as live. A source the capture recorded as failed replays as `unavailable` with the recorded error id, not as no data.
17. A run with one failed source and no other problem has status `findings` and exit 1, never `undetermined`. A check skipped because an earlier check failed for that source is `not-run` with a dependency reason, not `unknown`.
18. The late-correction pair, published in sequence through the runtime's shared publication path, leaves a consumer snapshot equal to the second answer alone.
19. The estimate wrapper's conversion of `series-quota-percent.json` yields exact decimal strings identical to the source text's values. Setting `bounded_percent = true` on an estimated quota card is reported as `refused` with `card-mismatch`.
20. Each estimate and observed case in [section 8.1](#81-compatibility-tests) produces the stated outcome through the wrapper's conversion mode and `charts replay`, using invented fixtures only.
21. A `sidescope-chart-v1.1` answer with a sampled metric reports reading instants, not interval centers, in card facts and Details, reports `break_before` at each listed break, and applies no right-edge downgrade. A payload labeled `sidescope-chart-v1` that contains `sampled` or `breaks` is refused as `invalid-dataset`, while one with an unrelated unknown optional field is accepted. An unlisted version is `unsupported-version`. Each invalid `sampled` or `breaks` form in [section 3.4](#34-observed-quota-utilization-cards) is refused whole. `charts schema` lists both payload versions.
22. Each selector outcome in the [section 3.4](#34-observed-quota-utilization-cards) table yields the stated card outcome. A single-window observed card has a numeric headline with a reading instant, and a successful query of an old reading is never labeled current.
23. Each refusal in the wrapper bounds table is a non-zero exit with no partial output, shown as `unavailable`.
24. The legacy and filtered profiles contain no quota source or card.

## 12. Facts this design relies on

These were confirmed against the code and the upstream wire definitions before this design was finalized. They are not open.

| Fact | Consequence here |
| :--- | :--- |
| The executable is `sidescope`. | Command examples are literal. |
| Every non-chart configuration field is optional, so a chart-only file loads. | Candidate files need no preamble. |
| The existing loader treats a missing file as defaults. | `charts` verbs must treat a missing **explicit** path as `invalid` ([section 5.1](#51-command-table)). |
| Unfiltered `knownCostUSD` covers metered responses only. | Day sources need no channel filter. |
| With `omitted = "zero"`, an absent quality counter column counts as zero for every bucket; an explicit null is still missing. | All-zero column omission never produces false `missing` cells. |
| A plain flat-rate subscription response is neither priced nor unpriced upstream; the unpriced counter covers only token-billed providers with a missing price. | The shared subscription estimate can be an exact, complete zero, which is why it is not carried forward ([section 3](#3-the-charts-this-design-proposes)). |
| `synthetic-latency.sh` already ships with the examples. | The cookbook reuses it. |
| A time-bars card with `stack_by` and `group_by = []` stacks by that dimension; a non-empty `group_by` must match it exactly. | The legacy profile is valid; new profiles set both explicitly. |
| The columns mapper refuses non-integral JSON numbers. | Quota percent columns need the wrapper in [section 3.3](#33-estimated-quota-contribution-cards). |
| Dataset v1 has positive intervals only, plotted at interval centers, with no break or marker primitive. | Observed readings need the additions in [section 3.4](#34-observed-quota-utilization-cards). |

## 13. Open questions

None. Section 12 records confirmed facts, and every mechanism above is decided here. The association between estimated quota and an observed account is not an open question: it is unknown by design.
