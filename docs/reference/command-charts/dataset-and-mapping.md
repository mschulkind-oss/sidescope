---
title: "Command charts: dataset contract and column mapping"
status: accepted
stage: CURRENT
verified: 2026-11-20
verified_commit: 14fb83d
covers:
  - src/chart/dataset.rs
  - src/chart/decimal.rs
  - src/chart/time.rs
  - src/chart/reduce.rs
  - src/chart_mapping.rs
tags: [reference, charts, data-contract]
summary: "The `sidescope-chart-v1` answer, its validation and quality semantics, and the generic `columns-v1` mapper that adapts existing interval/column JSON without expressions."
---

# Command charts: dataset contract and column mapping

**Status:** Verified 2026-11-20 against `14fb83d`. MEASURED against synthetic fixtures only; no live producer was read. Orientation and vocabulary: [README](README.md).

A command reaches the chart system in one of two ways, both ending in the same validated, immutable **chart dataset**: it prints the **native** answer (`sidescope-chart-v1`) itself, or an existing tool's interval/column JSON is adapted by the **`columns-v1` mapper** named in the source configuration. Everything after normalization sees only the dataset.

## Invariants

- **Atomic.** Any violation refuses the whole answer. There is no partial parse, no truncation, and no repair. An oversized answer is refused, never shortened.
- **Counts before allocation.** Interval, series, metric, and cell limits are checked before any derived structure is built.
- **Exact arithmetic.** Values are decimal strings or null, summed exactly. Floating point appears only when drawing; label rounding never changes an accumulated total.
- **Missing is never zero.** A cell is `complete`, `partial` (a known portion of the value), or `missing` (no value). `complete` means complete with respect to what the producer measured, **not** complete source coverage. `estimate` is a separate, per-metric property: a fully priced list-price figure is still an estimate.
- **No incompatible mixing.** Units never share an axis or a sum; no currency conversion exists. Ratios and percentiles arrive precomputed; they are never averaged or summed.
- **Identity is the whole tuple.** Two series may not share a full dimension tuple, all series declare the same dimension names, and a shortened display label is never an identity.

## The native answer

The shape, in prose; the types in `chart::dataset` and the fixtures under `tests/fixtures/charts/native/` are the machine-readable truth.

| Field | Rule |
| :--- | :--- |
| `version` | Exactly `sidescope-chart-v1` or `sidescope-chart-v1.1`. Another value is refused as an unsupported version; a missing one is an invalid dataset. |
| `generated_at` | RFC 3339 instant the answer was assembled. Says nothing about whether the producer's collector is current. |
| `observed_through` | Nullable instant through which the producer **asserts** collection. Null means unknown, not current; a commit or scan time is not a coverage assertion. |
| `revision` | Optional opaque text for diagnostics; neither a clock nor an ordering key. |
| `range` | `start`, `end`, `timezone`. Instants carry an offset or `Z`; start inclusive, end exclusive; a known IANA zone. |
| `intervals` | Ordered, non-overlapping, positive-length, each inside the range. Gaps are allowed. |
| `metrics` | Object keyed by unique id: `label`, `unit`, `quantity` (`interval_total` or `gauge`), `estimate` (boolean), optional `unknown_contribution` (`nonnegative` or `unknown`, default `unknown`); in v1.1 only, optional `sampled` (boolean). |
| `series` | Unique `id`, string-valued `dimensions`, and a `metrics` map holding **every** declared metric with aligned `values` and `quality` arrays; in v1.1 only, a sampled metric may add `breaks` beside them. |
| `notices` | Optional plain-text caveats shown in Info. No markup; control characters are refused. |

Unknown optional fields are ignored within the recognized version, with one exception: `sampled` and `breaks` are **reserved** names. A payload labeled `sidescope-chart-v1` that contains either, even as `null` or `false`, is an invalid dataset, because a v1 consumer would otherwise silently drop the meaning. In v1.1 the whole answer is refused when `sampled` is not a boolean, is `true` on anything but a `gauge`, or when `breaks` appears beside a metric that is not sampled or is anything but a strictly increasing array of integers from 1 to the interval count minus 1.

A **sampled** metric is a gauge whose every value is a reading taken at its interval's **start** instant; the interval end only orders the samples. `breaks` lists interval indices at which a line must not connect to the sample before it. Unknown optional fields other than these two are still ignored; a duplicate object key, a missing required field, malformed JSON, or exit status 0 with non-JSON output is a refusal (never an empty dataset). An empty series list is a successful "No observations", not a zero total. `unknown_contribution = nonnegative` is the only declaration that lets the interface say "at least"; otherwise an incomplete total is merely incomplete.

```json
{
  "version": "sidescope-chart-v1",
  "generated_at": "2026-11-20T12:02:10Z",
  "observed_through": null,
  "revision": "sample-7",
  "range": {"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:02:00Z", "timezone": "UTC"},
  "intervals": [
    {"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:01:00Z"},
    {"start": "2026-11-20T12:01:00Z", "end": "2026-11-20T12:02:00Z"}
  ],
  "metrics": {"amount": {"label": "Estimated amount", "unit": "USD", "quantity": "interval_total", "estimate": true}},
  "series": [{
    "id": "sample-project",
    "dimensions": {"project": "Example"},
    "metrics": {"amount": {"values": ["0.12", "0.03"], "quality": ["complete", "partial"]}}
  }],
  "notices": ["Synthetic example; the second interval has an unknown remainder."]
}
```

### Cell consistency and currentness

`complete` and `partial` require a value; `missing` requires null; any other quality word is refused. One further rule runs after validation and is easy to miss: **any interval that ends after the request's end instant has every `complete` cell downgraded to `partial`**, and `missing` stays missing. A bucket still accruing cannot be complete, whatever the producer says. A **sampled** metric is exempt: a reading at an instant is not an open bucket. A producer may additionally mark older buckets partial itself.

### Decimals and time

Values use a plain signed decimal grammar: optional minus, no plus, no exponent, no leading or trailing dot, no leading zeros, no whitespace. After zero normalization at most 24 integer and 18 fractional digits are accepted; a sum past 24 integer digits is an overflow and refuses the snapshot. `-0` prints as `0`; `1.50` prints as `1.5`. Equality of the underlying decimal is numeric, so text is always produced through the normalizing printer, never by formatting the inner type directly. Time uses RFC 3339 with an explicit offset; fractional seconds are accepted on input, and the request instants sent to commands are whole-second UTC.

### Signed values

Negative values are legal. They change presentation, not storage: the sign comes from the input cells, so a metric whose net total is positive but which contains negative cells is still signed. See [native charts](native-charts.md#ranking-and-signed-values).

## The `columns-v1` mapper

The mapper turns one existing JSON answer into a dataset using RFC 6901 JSON Pointers as exact field selectors. It is **not** a programming language: it supports aligned arrays, dimension renaming, decimal conversion, a missing-column policy, and one quality counter per metric. It has no arithmetic, no joins, no pricing logic, and no vendor field names. Anything more belongs in an owner-written wrapper that prints the native answer.

Pointer scoping decides most mapping errors:

- `buckets`, `groups`, `range_start`, `range_end`, `generated_at`, `observed_through`, `revision`, and every `notices` entry are **absolute** (relative to the whole answer).
- `interval_start` and `interval_end` are relative to **one bucket**.
- Each dimension `pointer`, each metric `values`, and each `quality_counter` are relative to **one group**.

Shape of the mapping (keys as parsed by `MappingConfig`; unknown keys are refused):

```toml
[chart_sources.mapping]
buckets = "/buckets"            # array of interval objects
interval_start = "/start"
interval_end = "/end"
groups = "/groups"
dimensions = [{ name = "model", pointer = "/keys/0", unknown_value = "unknown" }]
range_start = "/range/from"     # optional; defaults to the first interval start
range_end = "/range/to"         # optional; defaults to the last interval end
generated_at = "/generatedAt"   # optional; defaults to the request end
observed_through = "/collected" # optional; absent means unknown
revision = "/generation"        # optional; string or integer
notices = ["/coverage/statement"]

[[chart_sources.mapping.metrics]]
id = "metered_cost"
label = "Metered list-price estimate"
unit = "USD"
quantity = "interval_total"
estimate = true
unknown_contribution = "nonnegative"
values = "/columns/knownCostUSD"
quality_counter = "/columns/unpriced"
omitted = "zero"
```

### Cell rules

- A value array must have exactly one entry per bucket; a shorter or longer array refuses the answer. Entries are decimal strings, integers, or null. **JSON floats are refused** because they reintroduce rounding.
- An explicit `null` entry or column is **missing**, never zero. A column the producer omitted is missing under the default `omitted = "missing"` and all zeros only when the mapping declares `omitted = "zero"` because the producer documents that rule.
- **The parent guard.** Under `omitted = "zero"` a missing leaf is zero only if its immediate parent (the pointer up to its last `/`) resolves to a JSON object, where any token counts, the empty key included, or to an array, where only a canonical index (`0` or a number without sign or leading zero) counts. A missing, `null` or scalar parent, or a non-index token against an array (`-1`, `01`, `1.5`, a name), is an invalid mapping, so an answer in a different layout (for example values nested one level deeper) is refused instead of drawn as exact zeros. A pointer whose parent is the group itself, and `omitted = "missing"`, are unchanged. The guard is generic and knows no vendor layout; it does not catch every wrong leaf name and does not prove the author's intent.
- With a `quality_counter`: zero means complete, a positive count means partial (the value is the known portion), and a null or absent counter under the default policy means missing. Without a counter, every present value is complete. A cost profile that omits the counter would overstate certainty; the generic mapper cannot know a metric is a cost, so the **example profile's own test**, not the mapper, rejects that.
- Series ids are the JSON array of the ordered key tuple, so tuples like `["a|b","c"]` and `["a","b|c"]` never collide.
- A dimension's `unknown_value` (rule R16) names the producer's "no value" placeholder. The dimension value becomes empty, which the interface shows as **Unknown**; the series identity keeps the raw text so distinct tuples stay distinct. The renderer recognizes only the empty value and hard-codes no producer sentinel.
- Error text names mapping keys and never contains data values.

## What this layer is not

- Not stateful: a corrected old bucket in a later answer replaces, never accumulates.
- Not a unit converter, a currency converter, or a place to compute ratios.
- Not responsible for deciding what a source's coverage statement means; it only forwards named notice pointers to Info.

## What a designer can change here

A new producer needs only a wrapper or a mapping, never renderer changes. A new quality convention, a computed column, or a non-array layout is a wrapper's job. Raising a limit changes the Current values below and the draw-time capacity rules in [native charts](native-charts.md#capacity); it must stay a refusal, not a truncation.

## Current values

| Value | Setting | Owner |
| :--- | :--- | :--- |
| Integer / fraction digits | 24 / 18 | `MAX_INTEGER_DIGITS`, `MAX_FRACTION_DIGITS` in `chart::decimal` |
| Intervals / series / metrics | 2,048 / 256 / 32 | `MAX_INTERVALS`, `MAX_SERIES`, `MAX_METRICS` in `chart::dataset` |
| Total cells (series × metrics × intervals) | 100,000 | `MAX_CELLS` in `chart::dataset` |
| Notices per answer | 32 | `MAX_NOTICES` in `chart::dataset` |
| Text field length (ids, labels, notices; dimension values may be empty) | 1 to 512 characters | `MAX_TEXT_CHARS` in `chart::dataset` |
| Native version strings | `sidescope-chart-v1`, `sidescope-chart-v1.1` | `NATIVE_PAYLOAD_VERSIONS` in `chart::dataset` |
| Fixed window ceiling / calendar days ceiling | 30 days / 400 | `MAX_FIXED_WINDOW_MINUTES`, `MAX_CALENDAR_DAYS` in `chart::time` |

## Where the code lives

`chart::dataset` (types, validation, native parser), `chart::decimal`, `chart::time`, `chart::reduce`, `chart::facts` (the typed card facts built from a dataset), `chart_mapping` (`map_columns`, `normalize_output`). Fixtures: `tests/fixtures/charts/native/` and the synthetic ledger-shaped `tests/fixtures/charts/ledger/`.
