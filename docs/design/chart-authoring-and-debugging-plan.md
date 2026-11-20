---
title: "Plan: chart authoring and debugging tools"
date: 2026-11-20
status: accepted
stage: DECIDED
next: "Launch Task 1 (core) and Task 2 (adapters) in parallel; Task 3 (CLI) after Task 1 lands; Task 4 (compatibility and docs) after Tasks 1 to 3"
depends-on:
  - chart-authoring-and-debugging.md
---

# Plan: chart authoring and debugging tools

**Status:** 2026-11-20. Completed against the tree, written against `5e61849`. Design: [chart-authoring-and-debugging.md](chart-authoring-and-debugging.md) (accepted, no open question). Roles: the designer decided, this planner read the tree, the implementers build. No production code is written by this file.

**Precedence.** The design wins on behavior. The tree wins on cosmetic fact (a moved line, a renamed helper): follow it and say so in the commit. This file is advice and is the first thing to be wrong; never twist code to match it. **Stop and report, do not commit,** when: a red test does not fail the way the task predicts; a file the task names is dirty with someone else's work; a named symbol is gone; the fix needs a file outside the task fence; a gate fails outside the task's scope.

**Marking.** *Must* lines are constraints (facts of the repo or of the design). *Advice* lines carry their reason. *Cheap* marks a choice that is the implementer's and not worth deliberation. *Stop and ask* marks a choice that is not the implementer's.

## Rulings recorded while planning

| Id | Ruling | Why it is here |
| :--- | :--- | :--- |
| P-1 | **Mapper parent-container guard (supervisor-approved).** The design row for `series-objects.json` says the objects layout is "refused by the columns mapping with a typed error, not drawn". At `5e61849` it is drawn as exact, complete zero: `column()` in [`chart_mapping.rs`](../../src/chart_mapping.rs) returns `Absent` for any missing pointer and `omitted = "zero"` zero-fills it. Witnessed red by the planner (a throwaway test mapped an objects-shaped answer to one series). Fix in Task 1: under `omitted = "zero"`, a missing leaf is zero **only if its immediate parent resolves to a JSON object or array**; a missing, `null` or scalar parent is `InvalidMapping`. Applies to value and quality-counter pointers alike. **Array parents:** a parent that is an array accepts only a canonical numeric leaf token (`0` or `[1-9][0-9]*`, no sign, no leading zero, within `usize`); an omitted valid index (including any index into an empty array) zero-fills. A non-numeric token against an array (for example `/columns/knownCostUSD` where `columns` is `[]`) or a malformed index (`-1`, `01`, `1.5`, `+1`, empty) is `InvalidMapping`, not an omitted optional leaf. An object parent accepts any token, including the empty key, so a missing key in `{}` zero-fills Explicit leaf `null` stays missing; `omitted = "missing"` is unchanged; root-parent pointers such as `/cost` (parent is the group itself) are unchanged; escaped tokens (`~0`, `~1`) are handled by splitting at the last literal `/` (a `/` inside a token is always `~1`). Generic only: no vendor layout or version check. The guard does not catch every wrong leaf name and does not prove author intent; the QA note says so. | Observed shipped defect plus a design clarification; "missing is never zero". |
| P-2 | `legend_shown` in the report equals the number of legend entries the GUI model shows (`ChartCardModel::legend().len()`), so it is 0 for a single unlabeled group. The design's abridged example shows 1; that sample is illustrative, the field is defined here. | The model already hides a lone unlabeled legend entry. |
| P-3 | Numeric overflow while summing (today a message "Totals exceed the supported numeric range") is `refused` with `reason: "capacity"` and `limit: "numeric-range"`. Design enumerates only `card-mismatch` and `capacity`. | Maps existing behavior onto the fixed enum; additive field only. |
| P-4 | `summary.bound`: `exact` when quality is `complete`; `at-least` when quality is `partial` and the metric declares `unknown_contribution = "nonnegative"`; otherwise `incomplete`. `summary.exact` is null when quality is `missing` (the known sum is then only in `known_subtotal`). | Pins the design's enumerations to one rule. |
| P-5 | A source whose request is still deferred after the single one-second retry yields card outcome `not-evaluated` and source check `request: unknown` (status `undetermined`, exit 3). | Design [§5.2](chart-authoring-and-debugging.md#52-clock-semantics) names the check result but not the card outcome. |
| P-6 | The runner must hand back exit-0 stdout **bytes** even when they are not UTF-8, so capture is byte-exact "even if the output then fails to parse". Today `process::run_with_stderr` drops them (`RunFailure::NotUtf8`). | Capture contract needs bytes the runner discards. |

## Build order

| Task | Scope | Starts | Files shared with another task |
| :--- | :--- | :--- | :--- |
| 1 core | `CardFacts`, typed outcomes, v1.1 sampled gauges, mapper guard (P-1) | now, **independent** | none with Task 2 |
| 2 adapters | quota wrappers, synthetic and imported fixtures, three profiles | now, **independent** | none with Task 1 |
| 3 cli | `sidescope charts` verbs, descriptors, one-shot runtime, capture | **after 1** (needs `reduce_card`, v1.1) | edits what Task 1 left; no Task 2 file |
| 4 compat and docs | replay compatibility over fixtures and wrappers, cookbook, key matrix, guide | **after 1, 2, 3** | `examples/charts/README.md` (Task 2 first) |

Tasks 1 and 2 may run in separate worktrees at once; they touch disjoint paths (see each fence) and merge in any order. What gets expensive if Task 1 lands late: Task 3 blocks. What gets expensive if Task 3 lands late: Task 4 blocks, and nothing else is user-visible.

## Map (every file that changes)

| Path | Task | Change |
| :--- | :--- | :--- |
| `src/chart/facts.rs` | 1 | **new**: `CardFacts` and `reduce_card` |
| [`src/chart/mod.rs`](../../src/chart/mod.rs) | 1 | add `pub mod facts;` only |
| [`src/chart/dataset.rs`](../../src/chart/dataset.rs) | 1 | v1.1 version, `sampled`, `breaks`, reserved-name refusal, no right-edge downgrade for sampled metrics |
| [`src/chart/reduce.rs`](../../src/chart/reduce.rs) | 1 | `GroupedSeries` carries `breaks` |
| [`src/chart_mapping.rs`](../../src/chart_mapping.rs) | 1 | parent guard (P-1); `sampled: false` in the `MetricDescriptor` literal |
| [`src/chart_view.rs`](../../src/chart_view.rs), [`model.rs`](../../src/chart_view/model.rs), [`rank.rs`](../../src/chart_view/rank.rs), [`format.rs`](../../src/chart_view/format.rs), [`a11y.rs`](../../src/chart_view/a11y.rs) | 1 | `present_card`; model becomes a function of facts; sampled display |
| `tests/chart_facts.rs` | 1 | **new** |
| [`tests/chart_mapping.rs`](../../tests/chart_mapping.rs), [`chart_foundation.rs`](../../tests/chart_foundation.rs), [`chart_view.rs`](../../tests/chart_view.rs) | 1 | append tests only |
| `tests/fixtures/charts/native/sampled-*.json` | 1 | **new** v1.1 fixtures |
| [`docs/reference/command-charts/dataset-and-mapping.md`](../reference/command-charts/dataset-and-mapping.md), [`native-charts.md`](../reference/command-charts/native-charts.md) | 1 | v1.1, reserved names, parent guard, sampled display |
| [`docs/qa/command-charts.md`](../qa/command-charts.md) | 1 | append a dated section recording defect P-1; do not edit history |
| `examples/charts/quota-estimate.py`, `quota-observed.py` | 2 | **new**, executable |
| `examples/charts/agent-activity-filtered.toml`, `agent-activity-quota.toml` | 2 | **new** opt-in profiles |
| [`examples/charts/README.md`](../../examples/charts/README.md) | 2 | file rows and caveats |
| `tests/fixtures/charts/upstream/*` | 2 | **new**: twelve imported fixtures, capabilities copy without quota columns, provenance README |
| `tests/fixtures/charts/quota/*`, `tests/fixtures/charts/producers/*.sh` | 2 | **new**: invented answers; fake producer executables |
| `tests/chart_quota_wrappers.rs`, `tests/chart_profiles.rs` | 2 | **new** |
| [`flake.nix`](../../flake.nix), [`scripts/doctor.sh`](../../scripts/doctor.sh) | 2 | `python3` in the dev shell and the build prerequisites |
| `src/chart_cli.rs` and `src/chart_cli/{args,report,schema,check,replay,run,capture,text}.rs` | 3 | **new** |
| `src/chart/schema.rs` (new), [`config.rs`](../../src/chart/config.rs), [`error.rs`](../../src/chart/error.rs) | 3 | descriptor tables; structured config issue; stable error ids |
| [`src/chart_runtime.rs`](../../src/chart_runtime.rs), [`scheduler.rs`](../../src/chart_runtime/scheduler.rs), [`src/process.rs`](../../src/process.rs) | 3 | one-shot policy; raw stdout bytes (P-6) |
| [`src/lib.rs`](../../src/lib.rs), [`src/cli.rs`](../../src/cli.rs), [`src/main.rs`](../../src/main.rs) | 3 | root export, `Command::Charts`, dispatch, help line |
| `tests/charts_cli_{usage,replay,run,capture}.rs`, `tests/common/charts.rs` | 3 | **new** |
| `tests/chart_compat.rs`, `tests/chart_cookbook.rs` | 4 | **new** |
| `docs/chart-cookbook.md`, `docs/reference/command-charts/key-matrix.md` | 4 | **new** (matrix generated) |
| [`docs/guide.md`](../guide.md), [`docs/reference/command-charts/README.md`](../reference/command-charts/README.md), [`command-runtime.md`](../reference/command-charts/command-runtime.md), [`README.md`](../../README.md), [`Justfile`](../../Justfile) | 4 | see Task 4 |
| [`roadmap.md`](../../roadmap.md), this file | 4 | plan stage to BUILT only when graduation is owed |

## Reuse before you write

- Normalization stays the single path: [`chart_mapping::normalize_output`](../../src/chart_mapping.rs) (bound as `Normalizer`), `parse_native_snapshot`, `map_columns`. Replay calls them; nothing new parses producer output in Rust.
- Requests: `resolve_request_at(source, now, revision)` in [`chart/config.rs`](../../src/chart/config.rs) returns `Deferred` or `Invalid`; `ResolvedRequest.window.end` equals the whole-second request instant, so `clocks.request_at` is `window.end`.
- Execution: `process::run_with_stderr` with `CaptureLimits { stdout: STDOUT_LIMIT_BYTES, .. }`; `chart_runtime::execute` (private, ~`:453`) already maps failures through `failure_error`. Extract, do not duplicate.
- Pure scheduler: [`scheduler.rs`](../../src/chart_runtime/scheduler.rs) takes injected `Duration`; test the one-shot policy there with no process.
- Dataset equality: `ChartDataset` derives `PartialEq` but retains `request_window`; compare `series()`, `intervals()`, `range()`, `metrics()`, `revision()` when two independent requests are involved.
- Test scratch: `env!("CARGO_TARGET_TMPDIR")` as in [`tests/chart_integration.rs`](../../tests/chart_integration.rs) `counting_wrapper`. Fake producers: sibling of [`tests/fixtures/charts/runtime/*.sh`](../../tests/fixtures/charts/runtime/emit.sh). Existing helpers to mirror: `inline_config`/`map`/`answer`/`group`/`observed` in [`tests/chart_mapping.rs`](../../tests/chart_mapping.rs); `request()`, `snapshot_json`, `card()` in [`tests/chart_view.rs`](../../tests/chart_view.rs).
- Decimal text rules: [`chart/decimal.rs`](../../src/chart/decimal.rs) (24 integer, 18 fraction digits, no exponent, `to_plain_string` normalizes).

## House style and traps

- **Never add a field to** `ChartCardConfig`, `ChartSourceConfig`, `SourceInfo`, `SourceSnapshot` or `Cell`: existing tests build them as literals (`tests/chart_view.rs` `card()`, `info()`, `tests/chart_foundation.rs` `cell()`), and "preserve all existing regression tests" forbids repairing them. New data goes in new types. Adding a field to `MetricDescriptor`, `Series`, `GroupedSeries`, `DatasetParts` is safe (constructed only under `src/`).
- Baseline at `5e61849`, `cargo test` (all must stay green and no test may be removed or loosened): lib 89, bin 30, `chart_foundation` 41, `chart_integration` 4, `chart_mapping` 33, `chart_runtime` 41, `chart_view` 41, `chart_view_repair` 11, `cli` 8.
- GUI table order is group-major today (`time_rows`); the design's Details order is interval start then tuple. Facts hold the design order; `present_card` reorders for the unchanged GUI table.
- `ChartDataset::new` downgrades `Complete` to `Partial` for intervals ending after the request end. Sampled metrics must be exempt.
- [`fake-ledger.sh`](../../examples/charts/fake-ledger.sh) rejects `--columns` and `--channel` (exits 2 "unknown argument"). Do not point the new profiles at it; do not change it.
- `tests/chart_integration.rs` rewrites `"agent-activity", "ledger", "series"` inside the legacy profile. Keep that text of [`agent-activity.toml`](../../examples/charts/agent-activity.toml) byte-stable.
- Serde ignores unknown keys, so "reserved field present" needs presence-preserving parsing (`null` counts as present). Advice: parse the two reserved keys as `Option<serde_json::Value>` with a null-preserving deserializer; a plain `Option<bool>` cannot tell absent from `null`.
- Run commands for red/green: `./scripts/native-env.sh cargo test --test NAME`; format with `just format`; gates `just lint` and `just test`. `just check-ci` is the read-only gate. No git hook is installed here; never bypass one if present. `node_modules/` at the root is untracked and not yours: never stage it.
- Evidence and scratch go in the repository (`target/`, ignored) or the durable build directory named by the supervisor; never `/tmp`. Tracked Markdown must not cite durable paths.

## Acceptance coverage

| Design criterion | Task | Where it is pinned |
| :--- | :--- | :--- |
| 1 schema lists every key, both ways | 3 | `tests/charts_cli_usage.rs` schema tests |
| 2 check exit codes and `not-run`/`not-evaluated` | 3 | `tests/charts_cli_usage.rs` |
| 3 replay byte-identical | 3 | `tests/charts_cli_replay.rs` |
| 4 fixtures through replay; GUI and report share facts | 1 (facts equality), 4 (fixtures) | `tests/chart_facts.rs`, `tests/chart_compat.rs` |
| 5 one mismatched card, siblings plotted, exit 1 | 3 | `tests/charts_cli_replay.rs` |
| 6 `capacity` vs `no-observations` typed | 1, 3 | `tests/chart_facts.rs`, replay tests |
| 7 run spawns exactly once; preconditions before spawn | 3 | `tests/charts_cli_run.rs` |
| 8 no GUI, other modes build no runtime | 3 | fake Wayland socket test, existing `starts_chart_runtime` unit test |
| 9 no raw stdout, argv only with flag, no file without `--capture` | 3 | run and usage tests |
| 10 Details paging and subtotal arithmetic | 1, 3 | `tests/chart_facts.rs`, `tests/charts_cli_replay.rs` |
| 11 filtered and legacy profiles pass check; filtered `recent` plotted | 2 (parse), 4 (replay) | `tests/chart_profiles.rs`, `tests/chart_compat.rs` |
| 12 cookbook commands as written | 4 | `tests/chart_cookbook.rs` |
| 13 one JSON document; empty scope exit 2; `report-too-large` | 3 | usage tests |
| 14 four distinct empties; sum/gauge summary rules | 1 | `tests/chart_facts.rs` |
| 15 coverage unknown, never required; no aggregate freshness pass on failure | 3 | run tests |
| 16 capture replay equality (retained evidence only) | 3 | `tests/charts_cli_capture.rs` |
| 17 findings not undetermined; dependency `not-run` | 3 | run tests |
| 18 late-correction replacement through runtime | 4 | `tests/chart_compat.rs` |
| 19 wrapper exactness; `bounded_percent` card mismatch | 2, 4 | `tests/chart_quota_wrappers.rs`, compat |
| 20 estimate and observed cases through conversion and replay | 2, 4 | wrapper tests; compat |
| 21 v1.1 sampled, reserved names, versions | 1 (+3 schema lists versions) | `tests/chart_foundation.rs`, `tests/chart_facts.rs` |
| 22 selector outcomes; observed headline | 2 (output), 1 (headline), 4 (outcome) | wrapper tests, facts, compat |
| 23 wrapper bound refusals | 2 | `tests/chart_quota_wrappers.rs` |
| 24 legacy and filtered contain no quota | 2 | `tests/chart_profiles.rs` |

---

## Task 1 — core: card facts, sampled gauges, mapper guard

**Independent.** One commit, one worktree.

### Settled (do not re-open)

- One typed result per card; the GUI model is a function of it and the CLI serializes it. Exact, display and drawing numbers are three kinds: exact are `ChartDecimal`, display are strings, drawing are `f64` marked non-authoritative.
- Four distinct empty outcomes: empty series list = `NoObservations`; series exist but no interval in the card's range = `NoIntervals`; cells exist and every one is null = `NoKnownValues`; all-zero data = `Plotted`.
- Sum summary with any missing input has `exact = None` and `known_subtotal` holding the present-value sum (`None` only if no value is present; a real zero is `Some(0)`). Gauge `last` takes the latest non-null cell with that cell's own quality; earlier nulls do not invalidate it; a newer null sets `newest_is_null` without changing the value. P-4 fixes `bound`.
- v1.1: payload `version` is `sidescope-chart-v1` or `sidescope-chart-v1.1`; any other is `UnsupportedVersion`. `sampled` and `breaks` are reserved: a payload labeled v1 containing either (even `null`, even `false`) is `InvalidDataset`; every other unknown optional field is still ignored. Under v1.1 any failure refuses the whole answer: `sampled` is a boolean on a metric, `true` only with `quantity = "gauge"`; `breaks` appears only beside one series' `values` and `quality` for a sampled metric, integers, strictly increasing, each from 1 to interval count minus 1; `null`, floats, strings, duplicates, 0 and the interval count are invalid.
- Sampled display: each value is a reading at its interval's **start** instant; the interval end only orders samples. Lines plot at the start instant; readouts, Details and accessible text give the instant; no right-edge downgrade, no "so far"; `time-bars` refuses a sampled metric as a card mismatch. Headline for a sampled gauge: label `Last observed` (plus ` · incomplete` if the cell is not complete), note `as of <local instant>` always, plus ` · newest reading was not reported` when `newest_is_null`. It never says "current". `breaks` split line segments and set `break_before` on the Details row. The existing mark-cap logic in [`draw.rs`](../../src/chart_view/draw.rs) (`MAX_MARKED_POINTS`, single-point segments always marked) already gives the required "marks at or below the cap, segments plus isolated marks above it": do not change `draw.rs`.

### Preconditions

- `git rev-parse --short HEAD` is `5e61849` or a descendant that did not touch the Map's Task 1 paths; `git status --short` shows only `node_modules`.
- `./scripts/native-env.sh cargo test` is green with the baseline counts above. Otherwise stop.

### Frozen interfaces (names are contracts; field additions are cheap, renames are not)

```rust
// src/chart/facts.rs — GTK-free, no display strings, no dependency on chart_view
pub enum DataOrigin { Live, Replay }
pub fn reduce_card(card: &ChartCardConfig, snapshot: &SourceSnapshot,
                   now: Timestamp, origin: DataOrigin) -> CardFacts;

pub struct CardFacts {
    pub intent: CardIntent,              // id, source, kind, metric, title, resolved summary, group_by, stack_by,
                                         // top_n (Some(effective, default 5) only for ranked-bars), bounded_percent
    pub source: SourceFacts,             // origin, standing, snapshot (clone; dataset is an Arc)
    pub metric: Option<MetricDescriptor>,
    pub outcome: Outcome,
    pub summary: Option<SummaryFacts>,   // None unless Plotted/no-known with a card-level figure
    pub intervals: Option<IntervalFacts>,
    pub groups: Option<GroupFacts>,
    pub rows: Vec<GroupRow>,             // plotted rows, at most 16 including Other and every Unknown
    pub details: DetailFacts,
}
pub enum Standing { Live(SourceStatus), Replayed }   // Replayed iff origin is Replay and a dataset exists
pub enum Outcome {
    Plotted, NoObservations, NoIntervals, NoKnownValues,
    Refused(Refusal),
    Unavailable { source_error: Option<ChartErrorKind> },  // None = still loading (GUI only)
}
pub enum Refusal { CardMismatch(Mismatch), Capacity(CapacityRefusal) }
pub struct Mismatch { pub subject: MismatchSubject, pub wanted: Option<String>, pub available: Vec<String> }
pub enum MismatchSubject { Metric, Dimension, Summary, BoundedPercent, Grouping, Stacking, Sampled, GaugeMerge }
pub struct CapacityRefusal { pub limit: CapacityLimit, pub count: Option<usize>, pub maximum: Option<usize> }
pub enum CapacityLimit { UnknownSeries, GaugeSeries, NumericRange }
pub struct SummaryFacts {
    pub reduction: Reduction, pub exact: Option<ChartDecimal>, pub quality: Quality, pub bound: Bound,
    pub known_subtotal: Option<ChartDecimal>,   // sums only; None for gauges
    pub signed: bool,                           // any negative present value
    pub newest_is_null: bool,                   // gauges only
    pub observed: Option<Observation>,          // gauges: interval index and its start instant
}
pub enum Bound { Exact, AtLeast, Incomplete }
pub struct IntervalFacts { pub total: usize, pub complete: usize, pub partial: usize, pub missing: usize,
                           pub open_right_edge: bool }  // per interval, worst quality across all series; sampled: always false
pub struct GroupFacts { pub by: Vec<String>, pub total: usize, pub named_shown: usize, pub other_members: usize,
                        pub unknown_shown: usize, pub capacity: Option<usize>,  // the named limit, None when <= 16 groups
                        pub legend_shown: usize, pub legend_overflow: usize }
pub enum RowRole { Named, Other, Unknown }
pub struct GroupRow {  // value is the known subtotal; exact() is None when quality is Missing
    pub group: Option<Vec<(String, String)>>,   // None for Other
    pub role: RowRole, pub members: usize, pub value: Option<ChartDecimal>, pub quality: Quality,
    pub observed: Option<Observation>, pub cells: Vec<Cell>, pub breaks: Vec<usize>, /* + whatever drawing needs */ }
pub enum DetailGrain { IntervalGroup, Group }
pub struct DetailFacts { pub grain: Option<DetailGrain>, pub rows: Vec<DetailRow> }  // full set, before top-N folding
pub struct DetailRow {
    pub interval: Interval, pub sampled: bool, pub group: Vec<(String, String)>, pub shown_as: Option<RowRole>,
    pub known_subtotal: Option<ChartDecimal>,  // the present known value; for a Missing merged cell it still carries the known part
    pub quality: Quality, pub unit: String, pub break_before: bool,
}
impl DetailRow { pub fn exact(&self) -> Option<&ChartDecimal> /* known_subtotal unless quality is Missing */ }
impl GroupRow { pub fn known_subtotal(&self) -> Option<&ChartDecimal> /* = value */ }
// src/chart_view.rs additionally exports
pub fn present_card(facts: &CardFacts) -> ChartCardModel;   // the only constructor of the model
pub fn build_model(card, snapshot, now) -> ChartCardModel;  // unchanged signature: present_card(&reduce_card(.., Live))
// src/chart/dataset.rs
pub struct PayloadVersion { pub id: &'static str, pub adds: &'static [&'static str] }
pub const NATIVE_PAYLOAD_VERSIONS: [PayloadVersion; 2];     // v1 adds []; v1.1 adds ["sampled", "breaks"]
// MetricDescriptor gains `sampled: bool`; Series gains `breaks: BTreeMap<String, Vec<usize>>`
```

**Known-value encoding (must):** every row, ranked or time, stores one typed `known_subtotal` (the exact sum or selected observation of the values actually present, `None` only when none is present, a real zero is `Some(0)`); `exact` is derived by the accessor and is `None` exactly when quality is `Missing`. Report `rows[].exact`, `rows[].known_subtotal`, `details.rows[].exact` and `.known_subtotal` are copied from those accessors, so the CLI never sums, never subtracts and never infers a subtotal from `exact`. For a ranked `Partial` row the two are equal; for a `Missing` row `exact` is null and `known_subtotal` may be non-null.

Row semantics (must): time cards' `rows` are every plotted series (not legend-limited); ranked cards' `rows` are shown rows. `Details.rows` order for time cards is interval start then retained tuple, for ranked cards full value order with stable-id tie-break; `shown_as` is `Named`/`Other`/`Unknown` (Unknown iff any tuple value is empty, never folded into Other) or `None` when the card refused to plot. Ranked `grain = Group`: one row per retained group, a gauge row uses its actual selected observation interval. Capacity refusals keep their Details rows (as `capacity_refusal` does today); card-mismatch refusals have none. The tuple is the `collapse_series` key (card `group_by`, or `stack_by` for stacked bars). Evaluation order is today's, with `NoIntervals` inserted after `NoObservations`: mismatches, `NoObservations`, `NoIntervals`, capacity refusals, `NoKnownValues`, `Plotted`. `present_card` must reproduce every existing GUI message byte-for-byte from typed fields (inventory the sites in [`model.rs`](../../src/chart_view/model.rs): metric missing `:~300`, `build_dataset` summary and `bounded_percent` `:468`, `collapse` dimension/metric/gauge-merge `:425`, bars `group_by`/stacking `:~870`, `capacity_refusal` `:618`, `too_many` `:631`, overflow texts in `rank.rs` and `build_ranked`, three `No known values` sites). The new `Sampled` mismatch and the `Replayed` banner (`Replayed data · not live`, Info line `Origin: replayed from saved output; no command ran`) are new strings. Ranking (order, Other folding, Unknown) moves to the pure side (advice: `chart/facts` owns it; `chart_view/rank.rs` shrinks to label and swatch attachment) because facts cannot depend on `chart_view`.

### Tests first

Order: the mapper regression, then v1.1, then facts. Each red run is recorded in the commit message.

**1. Mapper parent guard — append to `tests/chart_mapping.rs`** (witnessed red at planning: `objects layout was drawn: Ok(1)`).

```rust
const NESTED: &str = r#"
[[chart_sources]]
id = "ledger"
command = ["example-ledger"]
format = "columns-v1"
window = "3h"
timezone = "UTC"
[chart_sources.mapping]
buckets = "/buckets"
interval_start = "/start"
interval_end = "/end"
groups = "/groups"
dimensions = [{ name = "project", pointer = "/keys/0" }]
[[chart_sources.mapping.metrics]]
id = "cost"
label = "Cost"
unit = "USD"
quantity = "interval_total"
values = "/columns/knownCostUSD"
quality_counter = "/columns/unpriced"
omitted = "zero"
"#;

fn nested(group: Value) -> Value {
    json!({"buckets": minute_buckets(1), "groups": [group]})
}

#[test]
fn an_objects_layout_group_is_refused_not_drawn_as_zero() {
    let objects = nested(json!({"keys": ["claude", "subscription"], "values": [{"knownCostUSD": "4"}]}));
    refused(NESTED, &objects, ChartErrorKind::InvalidMapping);
}

#[test]
fn a_missing_null_or_scalar_parent_is_refused_and_a_real_empty_container_is_zero() {
    for parent in [json!(null), json!(7), json!("x")] {
        refused(NESTED, &nested(json!({"keys": ["a"], "columns": parent})), ChartErrorKind::InvalidMapping);
    }
    let dataset = map(NESTED, &nested(json!({"keys": ["a"], "columns": {}}))).unwrap();
    assert_eq!(observed(&dataset, 0, "cost"), cells(&[(Some("0"), Complete)]));
    // Empty array parent: only a canonical numeric leaf is an omitted index.
    let indexed = NESTED.replace("/columns/knownCostUSD", "/columns/0").replace("/columns/unpriced", "/quality/0");
    let dataset = map(&indexed, &nested(json!({"keys": ["a"], "columns": [], "quality": []}))).unwrap();
    assert_eq!(observed(&dataset, 0, "cost"), cells(&[(Some("0"), Complete)]));
    // A string key against an array is a wrong shape, never an omitted leaf.
    refused(NESTED, &nested(json!({"keys": ["a"], "columns": []})), ChartErrorKind::InvalidMapping);
    for bad in ["-1", "01", "1.5", "+1"] {
        let config = NESTED.replace("/columns/knownCostUSD", &format!("/columns/{bad}"));
        refused(&config, &nested(json!({"keys": ["a"], "columns": [], "unpriced": []})), ChartErrorKind::InvalidMapping);
    }
}

#[test]
fn the_guard_applies_to_the_counter_and_keeps_explicit_null_and_root_parents() {
    // value present under an existing parent, counter parent missing is impossible (same parent): use two parents
    let split = NESTED.replace("/columns/unpriced", "/quality/unpriced");
    refused(&split, &nested(json!({"keys": ["a"], "columns": {"knownCostUSD": ["1"]}})), ChartErrorKind::InvalidMapping);
    let ok = map(&split, &nested(json!({"keys": ["a"], "columns": {"knownCostUSD": ["1"]}, "quality": {}}))).unwrap();
    assert_eq!(observed(&ok, 0, "cost"), cells(&[(Some("1"), Complete)]));
    let null_leaf = map(NESTED, &nested(json!({"keys": ["a"], "columns": {"knownCostUSD": null, "unpriced": [0]}}))).unwrap();
    assert_eq!(observed(&null_leaf, 0, "cost"), cells(&[(None, Missing)]));
    // omitted = "missing" never needed a parent: unchanged.
    let default = NESTED.replace("omitted = \"zero\"", "");
    let missing = map(&default, &nested(json!({"keys": ["a"]}))).unwrap();
    assert_eq!(observed(&missing, 0, "cost"), cells(&[(None, Missing)]));
}
```

Also: the empty object key (`/columns/` on `{}`) zero-fills. Add one case with an escaped token pointer (`/a~1b/leaf`, parent `/a~1b`) and keep the existing root-parent tests (`/cost`) untouched. `./scripts/native-env.sh cargo test --test chart_mapping` red, then green after the guard.

**2. v1.1 — append to `tests/chart_foundation.rs`** (witnessed red at planning: `UnsupportedVersion: snapshot version is not sidescope-chart-v1`). Fixture `tests/fixtures/charts/native/sampled-gauge.json` (new) is:

```json
{"version":"sidescope-chart-v1.1","generated_at":"2026-11-20T11:59:00Z",
 "range":{"start":"2026-11-20T11:00:00Z","end":"2026-11-20T12:00:00Z","timezone":"UTC"},
 "intervals":[{"start":"2026-11-20T11:00:00Z","end":"2026-11-20T11:15:00Z"},{"start":"2026-11-20T11:15:00Z","end":"2026-11-20T12:00:00Z"}],
 "metrics":{"observed_percent":{"label":"Observed","unit":"percent","quantity":"gauge","estimate":false,"sampled":true}},
 "series":[{"id":"[]","dimensions":{},"metrics":{"observed_percent":{"values":["12.5","13"],"quality":["complete","complete"],"breaks":[1]}}}]}
```

Tests (use the existing `request()`-style helper; request window ends `12:00:00Z`): the fixture parses, `metric("observed_percent").sampled` is true and the last cell stays `Complete`; the same text labeled `sidescope-chart-v1` is `InvalidDataset`; the same with `"sampled":null` in v1 is `InvalidDataset`; a v1 payload with `"sampled": false` is `InvalidDataset`; a v1 payload with an unrelated `"future_field": 1` is accepted; `sidescope-chart-v1.2` is `UnsupportedVersion`. Invalid v1.1 forms, each `InvalidDataset`: `sampled` on an `interval_total` metric; `sampled` as `"true"` or `null`; `breaks` beside a non-sampled metric; `breaks: null`; `[0]`; `[2]` (interval count); `[1,1]`; `[2,1]`; `[1.0]`; `["1"]`; `[-1]`. Also: `NATIVE_PAYLOAD_VERSIONS` lists both ids.

**3. Facts — new `tests/chart_facts.rs`** (red = does not compile: `reduce_card`, `present_card` and `CardFacts` are absent; `./scripts/native-env.sh cargo test --test chart_facts`). Consumer interface only: `parse_native_snapshot` or `map_columns`, a `SourceSnapshot` literal as in `tests/chart_view.rs`, then `reduce_card`. Cases, each with exact literals taken from the fixture JSON written in the test:

| Case | Expected facts |
| :--- | :--- |
| `series: []` | `Outcome::NoObservations`, `summary` None |
| series, `intervals: []` (range kept) | `Outcome::NoIntervals` |
| one series, cells all `null`/`missing` | `Outcome::NoKnownValues`, `intervals.missing == total` |
| one series, values `"0","0"`, complete | `Outcome::Plotted`, `summary.exact == Some("0")`, `known_subtotal == Some("0")`, `bound == Exact` |
| sum cells `"1.5"` complete, `"2"` partial, metric `nonnegative` | `quality Partial`, `bound AtLeast`, `exact Some("3.5")`, `known_subtotal Some("3.5")` |
| same, metric `unknown` contribution | `bound Incomplete`, `exact Some("3.5")` |
| sum with one missing cell and `"4"` complete | `quality Missing`, `exact None`, `known_subtotal Some("4")` |
| gauge `["41", null]` | `summary.exact Some("41")`, `newest_is_null true`, `observed` index 0 |
| gauge sampled fixture above, `time-series`, `now` after range | `summary.exact Some("13")`, `observed.at` = `11:15:00Z`, `open_right_edge false`, `rows[0].breaks == [1]`, `details.rows[1].break_before == true`, `details.rows[0].sampled == true` |
| sampled gauge on `time-bars` | `Refused(CardMismatch{subject: Sampled})` |
| `bounded_percent` on unit `percentage points` | `Refused(CardMismatch{subject: BoundedPercent})` |
| 17 groups, ranked `top_n = 16`, one Unknown | `rows.len() <= 16`, `Other` row role, Unknown row kept apart, `groups.capacity` equals the named limit (see note) |
| 17 empty-valued groups on a time card | `Refused(Capacity{limit: UnknownSeries})` with `details.rows` present |
| 17 gauge series | `Refused(Capacity{limit: GaugeSeries, count: Some(17), maximum: Some(16)})` |
| Details arithmetic (sum card, 3 groups, top_n 1) | sum of `details.rows[*].known_subtotal` equals `summary.known_subtotal`; `Other.value` equals the sum of its members' rows; Other is not counted with its members |
| Unknown membership | the Unknown group row has `shown_as Some(Unknown)` and its tuple is `[("project","")]`, never `Other` |
| Replay origin with dataset | `source.standing == Replayed`; `present_card(..).banner() == Some("Replayed data · not live")` |
| Equality | for every file in `tests/fixtures/charts/native/` and each card kind, `present_card(&reduce_card(c, s, now, Live))` equals `build_model(c, s, now)` (`ChartCardModel: PartialEq`) |

Note for the 17-group ranked row: the capacity is `16 - (unknown + 1)`; the implementer computes the literal from `named_limit` and asserts it; it is cheap and not a design question.

Also add to `tests/chart_view.rs` (appended): the sampled headline strings above, the Details table showing the instant (not `interval_text`), and the `Sampled` card mismatch message starting `Card mismatch`.

### Change

`dataset.rs`: accept both versions; keep `NATIVE_VERSION` text for v1; parse `sampled`/`breaks` with presence tracking; validate in `ChartDataset::new` (breaks only for sampled metrics, range and ordering); skip the right-edge downgrade (`:216` loop) for sampled metrics. `reduce.rs`: `collapse_series` copies `breaks` for a sampled single-member group (gauges are never merged). `chart_mapping.rs` `column()`: add the parent guard as one small helper used by both value and counter paths; no new config key. `facts.rs`/`chart_view/*`: split `build_model` into `reduce_card` then `present_card`; keep every existing public getter and `build_model`'s signature. Sampled placement: x from `TimeAxis::fraction(interval.start)`, `Axes.centers` likewise, segments break at `breaks` and not on contiguity.

### Gates

`just format`; `just lint`; `just test` (all baseline counts unchanged or higher; no existing test edited); the three new test files green. Docs: update the two reference pages (version row `Exactly sidescope-chart-v1`, "Unknown optional fields are ignored", the downgrade rule, gauge/line placement, objects-layout statement) and append the QA section naming defect P-1, the witnessed red, and the limit that the guard does not prove author intent.

### Fences

Must not touch: anything under `examples/`, `tests/fixtures/charts/upstream|quota|producers`, `scripts/`, `flake.nix`, `src/chart_runtime*`, `src/process.rs`, `src/cli.rs`, `src/main.rs`, `src/lib.rs`, `src/chart/config.rs`, `src/chart/error.rs`, `src/chart/state.rs`, `src/chart_view/draw.rs`. Tier: the path list is a tier-3 rule (remembered); the structural backstop is that Task 2's tests would fail to compile against any rename here only if frozen names change, so do not rename them. Stop and ask if a change needs a new field on a "never add" type.

### Commit message

`feat(charts): typed card facts, v1.1 sampled gauges, and a mapper parent guard`

### Report

The three red commands and outputs, the baseline-vs-final test counts per binary, whether `present_card` messages needed any text change (there should be none except the two new strings), and anything the tree forced.

---

## Task 2 — adapters: bounded exact-decimal quota wrappers, fixtures, profiles

**Independent of Task 1.** One commit, one worktree. Executable Python 3 standard library only.

### Settled (do not re-open)

- Two wrappers print a native answer; each runs exactly one producer command as its own child, no shell, no other query, **no new session or process group** (the child stays in the owned group), `stdin` null, producer `stderr` inherited (flows to the outer runner's existing discard or bounded capture). Reading **producer stdout while it arrives, capped at 1 MiB (1,048,576 bytes) before JSON decoding**: on overflow kill the child, reap it and exit non-zero with nothing printed. The pure conversion mode reads standard input with the same cap. Output over 1 MiB is also refused by the wrapper. Decimals are read with `json.loads(parse_float=decimal.Decimal)`; no value passes through a binary float.
- Wrappers **refuse rather than trim**: more than 2048 readings or buckets, 256 series or 100000 cells; a notice over 512 characters or more than 32 notices; invalid or misaligned input; producer `truncated = true` is a notice, not a refusal (each reading stays `complete`).
- Quota is not money and an estimate is not a reading. The metric unit for estimates is `percentage points` (so `bounded_percent = true` is a card mismatch), the observed unit is `percent` with `bounded_percent` left false. Percent text never becomes dollars. A model with no weight is `partial`, never a complete zero. An omitted count column is zero; an explicit null is missing.
- Money caveat of the original mapped profile carries into the filtered profile unchanged: list-price estimate of metered responses, not an invoice; unpriced raises `partial` and "incomplete"; flat-rate subscription has no per-response dollar figure and charts as counts or quota. The producer's own statement is kept: a Claude plan spending chart shows zero dollars, so chart it in quota percent. Claude Code used with a direct API key is classified `subscription` upstream and may be counted in the estimate; the wrappers do not correct it.
- Profiles never merge: legacy [`agent-activity.toml`](../../examples/charts/agent-activity.toml) stays byte-identical; the filtered and quota profiles are new files; neither legacy nor filtered contains a quota source or card (id, command, metric id or substring `quota` or `Percent`).

### Preconditions

Task 1's files are not needed. `python3 --version` runs inside `./scripts/native-env.sh` (3.9+ expected; Python 3.14 in this jail). If `python3` is absent in the nix shell, stop and report. The producers' source is read-only; fixtures are copied from its published fixture directory (upstream `docs/reference/ledger-v1-fixtures`, provenance commits below). No live account, credential, collector or provider is used.

### Wrapper contracts (frozen)

Both: arguments `--from INSTANT --to INSTANT --timezone ZONE` (the profile's whole-argument placeholders), `--producer EXE` (default `agent-activity`, used only by tests and owners; a literal executable name or path), `--convert-stdin` (pure mode: no process runs; same flags; reads one producer answer on stdin). Exit 2 = usage error, 1 = any refusal or producer failure, 0 = success; one plain stderr line with no data values. Output is one JSON object, compact, UTF-8, followed by a newline. Series ids use compact JSON of the tuple with `ensure_ascii=False` and separators `(",", ":")`.

**`examples/charts/quota-estimate.py`** — extra flags `--quota weekly|five-hour` (required), `--bucket minute|5min|15min|hour|day` (required), `--group-by none|model|project` (required; `project` is the producer's `workspace`). Producer command: `agent-activity ledger series --format json --layout columns --bucket B --group-by G --channel subscription --columns <P>,<U> --from F --to T --timezone Z` with `<P>,<U>` = `subscriptionWeeklyPercent,subscriptionWeeklyUnweighted` or `subscriptionFiveHourPercent,subscriptionFiveHourUnweighted`. Output `version` `sidescope-chart-v1`; `generated_at` = producer `generatedAt`; `revision` = text of `generation`; `range` = producer `range.from/to` with the `--timezone`; intervals = producer `buckets` (start/end as given); one metric `weekly_quota_points` or `five_hour_quota_points` (labels `Estimated weekly quota contribution` / `Estimated five-hour quota contribution`, unit `percentage points`, `interval_total`, `estimate: true`, `unknown_contribution: nonnegative`); one series per producer group, dimension `model` (with `(unknown)` shown as the empty string) or `project`, none for `none`. Cell value = exact plain decimal text. **Never call `Decimal.normalize()`, arithmetic or `quantize` for formatting**: they use the thread's default 28-digit context and silently round legal 24+18-digit numbers (42 significant digits). Advice: use `decimal.Context(prec=80)` only for `Decimal(text)` construction (construction itself is exact), or parse the JSON number's original text (`parse_float=str`-style capture) and format from `as_tuple()` digits and exponent into plain notation, then trim trailing fractional zeros and a bare `.` by string; negative zero is `0`; more than 24 integer or 18 fraction digits after trimming refuses. No binary float anywhere; an absent percent column is `"0"` complete; an explicit null is `missing`; the matching count column absent means zero, an integer above zero makes the cell `partial`, an explicit null makes it `missing`. A missing `quotaWeights` object is exit 1. Notices (at most four, in this order, these exact texts): `Quota weights generated {generated}; fitted from {fittedFrom} through {fittedThrough}.` (null shown as `unknown`; the fitted-through age is never judged); `Weights come from a plan priced at {seatPriceUSDPerMonth} USD per month.`; only when `--group-by model` and at least one answer group's model has no weight (key absent or null for the chosen bar): `{n} grouped model(s) have no fitted {weekly|five-hour} weight; their usage is a lower bound.`; `Estimated points are fitted, not published by the provider, and exclude resets and other clients.`

**`examples/charts/quota-observed.py`** — extra flags: exactly one of `--account-key K` (preferred) or `--account-label L`; `--window five_hour|seven_day` (required, passed to the producer); `--max-gap-minutes N` (1 to 1440, default 45). Producer command: `agent-activity quota series --format json --from F --to T --window W`. Output `version` `sidescope-chart-v1.1`; `generated_at` = producer `generatedAt`; `range` = exactly `--from`/`--to`; metric `observed_percent` (label `Observed quota utilization`, unit `percent`, `gauge`, `estimate: false`, `sampled: true`); at most one series: id `["<accountKey>","<window>"]`, no dimensions. Intervals `[at[i], at[i+1])`, the last `[at[n-1], to)`. **Instants keep RFC 3339 nanoseconds exactly**: parse every instant (`at`, `--from`, `--to`, resets) with a hand-written parser into integer epoch seconds plus integer nanoseconds (offset applied as integers), compare as `(seconds, nanos)` tuples, and print by formatting those integers back; Python `datetime` (microsecond truncation) and float timestamps are forbidden because they would merge distinct readings. The range is the design's inclusive-from, exclusive-to with no widening: a reading equal to `--from` is in range, one equal to `--to` is outside (exit 1). A gap break is listed when `next - previous > max_gap_minutes` in integer nanoseconds (strictly greater; exactly equal does not break) Cells: percent text complete, null `missing`. `breaks` (sorted, unique, only for indexes 1 to n-1): before the reading at each `resets` entry after the first, and before any reading more than `--max-gap-minutes` after the previous. Selector table (first match wins): `accounts = []` → success, no series; selector matches nothing in a non-empty list → exit 1; label or key matches more than one account → exit 1; account found but window entry absent → success, no series, notice `The account reported no readings for window {w}.`; window entry with empty `at` → one series, no intervals; otherwise one series (all-null readings stay `missing`). Exit 1 on duplicate or non-increasing instants, an instant outside `[from, to)`, a percent that is neither null nor a non-negative number, a reset index out of range or not increasing, array lengths that differ. Notices (only when the count is above zero, this order): truncated `Earlier readings exist but were not returned; the journal holds more than was read.`; `{n} reset-time change(s) in this range; the line is broken there and no change is computed across one.`; one `{n} recorded {kind} edge(s) in this range.` per kind among `limit_changed`, `availability_changed`, `window_added`, `window_removed` (sorted by kind); `{n} gap break(s): readings more than {m} minutes apart are not joined; that spacing rule is this wrapper's setting, not producer data.`. `observed_through` is omitted: neither `generatedAt` nor the newest reading is a collector watermark.

### Profiles (frozen ids)

- `agent-activity-filtered.toml`: sources `recent` (the design's block verbatim, `window = "3h"`), `daily`, `models`, `subscription`, `copilot-cost`; day sources add `--columns` for what they map (`knownCostUSD,unpriced`; `subscriptionResponses`; `subscriptionKnownCostUSD,subscriptionUnpriced` with `--channel subscription:github-copilot --group-by none`); cards `recent-cost`, `daily-cost`, `project-ranking`, `model-ranking`, `subscription-activity`, `copilot-estimate` with the design's titles ("list-price estimate"), `stack_by` and `group_by` both set on time bars. No `subscription-estimate` card.
- `agent-activity-quota.toml`: sources/cards `quota-estimate` (weekly, `day`, `model`, `calendar_days = 5`), `quota-estimate-projects` (optional), `quota-estimate-recent` (five-hour, `5min`, `none`, `window = "5h"`); cards `quota-weekly-daily`, `quota-weekly-by-model`, `quota-weekly-by-project`, `quota-five-hour-recent`; observed example sources and cards `quota-observed-primary-five_hour` (`window = "24h"`) and `quota-observed-primary-seven_day` (`window = "7d"`), `summary = "last"`, `group_by = []`, with `--account-key REPLACE-WITH-ACCOUNT-KEY`. Wrapper paths are placeholders the owner replaces (comment says so). Header comments: authority, the feature checks the author runs by hand (capabilities column descriptors; `agent-activity quota series --format json --window five_hour`), the money caveat, the API-key caveat, "estimated points are not the bar", "no account link is known".
- Fixtures: copy the twelve upstream files into `tests/fixtures/charts/upstream/` unchanged, plus `capabilities-without-quota.json` (the copy minus the four quota descriptors) and a `README.md` stating provenance: upstream `9a15507`, except `series-quota-percent.json` from `73a5063`, all unchanged at `49f89cc`, every value invented, no account data. New invented answers under `tests/fixtures/charts/quota/`: `estimate-*.json` and `observed-*.json` for each case in design [§8.1](chart-authoring-and-debugging.md#81-compatibility-tests) (list below).

### Tests first

`tests/chart_quota_wrappers.rs` drives the real executables with `std::process::Command` and `python3`, writing scratch under `CARGO_TARGET_TMPDIR`; `tests/chart_profiles.rs` parses profiles with `sidescope::parse_config` and checks fixtures. Red: neither the files nor the profiles exist (`./scripts/native-env.sh cargo test --test chart_quota_wrappers --test chart_profiles`).

```rust
#[test]
fn estimate_conversion_keeps_exact_decimal_text() {
    let out = run_wrapper("quota-estimate.py",
        &["--quota", "weekly", "--bucket", "5min", "--group-by", "model",
          "--from", "2026-11-16T12:00:00Z", "--to", "2026-11-16T12:15:00Z",
          "--timezone", "UTC", "--convert-stdin"],
        &fixture("upstream/series-quota-percent.json"));
    assert_eq!(out.status, 0, "{}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["version"], "sidescope-chart-v1");
    let cells = &v["series"][0]["metrics"]["weekly_quota_points"];
    assert_eq!(cells["values"], serde_json::json!(["0", "0.5", "0"]));
    assert_eq!(cells["quality"], serde_json::json!(["complete", "complete", "complete"]));
    assert_eq!(v["metrics"]["weekly_quota_points"]["unit"], "percentage points");
    assert_eq!(v["notices"].as_array().unwrap().len(), 3);   // weights basis, price basis, fixed statement
}
```

Required cases (each a named test; values written by hand from the fixture text, not copied from output): estimate five-hour gives `["0","2","0"]`; a model with a null weight and a positive `...Unweighted` count is `partial` and the no-weight notice appears with `1 grouped model(s)`; a window sum above 100 converts without error and the unit is not `percent`; missing `quotaWeights` exits 1 with empty stdout; an old `fittedThrough` is printed as given; `generated` longer than 512 characters exits 1; 257 groups, 2049 buckets and a cell product above 100000 each exit 1 (generate inputs in the test); a decimal with 19 fraction digits exits 1; a 24-integer-plus-18-fraction-digit value (42 significant digits, e.g. `123456789012345678901234.123456789012345678`) converts to identical text, and a 30-significant-digit value is not rounded; observed: two accounts × two windows never merge (key selects one; an ambiguous label exits 1; a miss in a non-empty list exits 1; `accounts: []` succeeds with `series == []`; window absent succeeds with the notice; `at: []` gives one series and `intervals == []`; all-null gives missing cells); a second reset entry produces `breaks`; a 46-minute gap breaks at the default and not at `--max-gap-minutes 60`; a gap of exactly 45 minutes does not break and 45 minutes plus 1 ns does; two readings 1 ns apart stay distinct intervals (`…:00.000000001Z` and `…:00.000000002Z`, first interval 1 ns long) and identical ones exit 1; a reading at `--from` is kept and one at `--to` exits 1; an estimate answer's `.123456789Z` bucket bounds are copied unchanged; `truncated: true` adds the notice and keeps `complete`; newest reading null stays `missing` at the last index; each exit-1 input listed above; `--max-gap-minutes 0` and `1441` exit 2; `--account-key` with `--account-label` exits 2; no selector exits 2.

Bound tests with **fake producer executables** (shell scripts in `tests/fixtures/charts/producers/`): `answer.sh FILE` prints a file and records its argv to `$FAKE_RECORD`; `flood.sh` writes 3 MiB then `sleep 30` after recording its pid in `$FAKE_PID`; `fail.sh` exits 3. Tests assert: argv recorded exactly once with the frozen producer flags and nothing else spawned; the flood case makes the wrapper exit 1 within 10 s, stdout empty, and the recorded pid is no longer a running process; stdin at exactly 1,048,576 bytes of valid JSON is accepted and at 1,048,577 exits 1; producer exit 3 gives wrapper exit 1 with no stdout. The wrapper never spawns a second process (the recorder has one line).

Profile tests: both new profiles parse; legacy file bytes are unchanged (`git`-independent: compare a checked-in length and a substring set, or hash it in the test); legacy and filtered contain none of `quota`, `Percent`, `Weighted`; the filtered `recent` source command equals the design's argv; `copilot-estimate` uses `--channel subscription:github-copilot`; every card id and source id above exists; wrappers have the executable bit; capabilities fixture has the four quota descriptors and `capabilities-without-quota.json` does not (documented check: the four names are keys of `.series.columns`); every mapped metric of the filtered profile agrees with the capabilities descriptors: `estimate` equal, quality-counter column equals `completenessColumn`, unit equal for `USD` metrics; the quota profile's estimate metrics deliberately say `percentage points` against descriptor `percent` (assert the divergence on purpose).

### Change

Create the files in the Map. Wrapper structure advice: one shared module is not possible (two single-file executables, no packaging); duplicate the small bounded-read and decimal helpers (about 40 lines) rather than import across files, because each file is copied alone by an owner. `flake.nix`: add `python3` to the dev shell `packages`; `scripts/doctor.sh`: add `check_command python3 'quota wrapper tests'` in the native build block. Update `examples/charts/README.md` with the new files, the profile choice (probe by hand; Sidescope never probes or falls back), and the caveats above.

### Gates

`just format`; `just lint`; `just test`; `bash scripts/doctor.sh --build` shows `python3`. Wrapper tests must **fail loudly** when `python3` is absent (assert on spawn error), never skip.

### Fences

Must not touch: `src/`, any `tests/*.rs` other than the two new files, `tests/fixtures/charts/native/`, `tests/fixtures/charts/ledger/`, `tests/fixtures/charts/runtime/`, `examples/charts/agent-activity.toml`, `fake-ledger.sh`, `docs/` (Task 4 owns docs; the examples README is the only Markdown here). Tier-3 rule for paths; the legacy-bytes test is the tier-1 backstop for the profile.

### Commit message

`feat(examples): bounded quota wrappers, synthetic fixtures and opt-in profiles`

### Report

Red and green commands, fixture provenance line, the wrapper byte-size, and the flood test's measured duration.

---

## Task 3 — CLI: `sidescope charts schema|check|replay|run`

**After Task 1.** One commit, one worktree. Needs `reduce_card`, `present_card`, v1.1 and `NATIVE_PAYLOAD_VERSIONS` from Task 1; touches no Task 2 file.

### Settled (do not re-open)

Quote of the design contract, in force verbatim: the command table ([§5.1](chart-authoring-and-debugging.md#51-command-table)), clock semantics ([§5.2](chart-authoring-and-debugging.md#52-clock-semantics)), report shape and enumerations ([§5.3](chart-authoring-and-debugging.md#53-report-shape) and [§5.4](chart-authoring-and-debugging.md#54-enumerations-and-exit-codes)), bounds, privacy and authority ([§5.5](chart-authoring-and-debugging.md#55-bounds-privacy-and-authority)) and criteria 1 to 3, 5 to 10, 13, 15 to 17. Highlights an implementer must not lose: execution only when named, once, unsandboxed, existing runner; **everything is resolved before anything happens** (arguments, configuration, selectors, data files, capture directory), any failure then means no process spawned and no file written; an explicit missing `--config` is `invalid`, only the default path may be absent; an empty scope (no source or no card) is `invalid` `empty-scope`; raw output is never in a report; no file without `--capture`; exactly one JSON document under the default format even for usage errors; a report over 1 MiB becomes a minimal `report-too-large` report, exit 2; replay has no default clock and its `freshness` is always `not-run`; `appearance` is always `not-run`.

Pinned here (planner altitude; each is a mapping of the design, not a new product choice):

| Topic | Rule |
| :--- | :--- |
| Arguments | `charts schema [--section NAME]`; `check [--config F] [--at I] [--card ID]…`; `replay [--config F] (--at I --data SRC=FILE… \| --from-capture DIR) [--card ID]… [--details CARD [--offset N] [--limit N]]`; `run [--config F] (--source ID… \| --all-sources) [--capture DIR] [--card ID]… [--details …]`; all verbs `--show-argv`, `--format json\|text`. `--data` splits at the first `=`. `--offset` ≥ 0 (default 0), `--limit` 1 to 5000 (default 500); they need `--details`. |
| Selection errors | repeated `--source`, `--card` or `--data` id, a bad `--at`, `--at` on `run`, `--offset/--limit` without `--details`, a flag a verb lacks, an unknown `--section`: `usage`. `--source` with `--all-sources`, `--data` with `--from-capture`, `--at` with `--from-capture`, a known card whose source is not selected or has no data: `conflicting-selection`. Unknown source in `--source`/`--data`/a capture manifest: `unknown-source`; unknown card or `--details` target: `unknown-card`. `--at` is floored to whole seconds and printed as UTC `Z`. |
| Scope | `check` covers every configured source (narrowed by `--card` to those cards' sources). `run` executes all selected sources even if `--card` hides all their cards. Scope needs ≥ 1 source and ≥ 1 card, else `empty-scope`. |
| Resolution order | args → config (read; structured issue with `key`) → selectors → replay data files (readable, ≤ 1,048,576 bytes: else `data-unreadable`; non-UTF-8 is a normal `parse` source finding) or capture manifest (missing, unreadable or malformed: `data-unreadable`; a source with `captured: false` and outcome `ok` cannot replay: `data-unreadable`) → capture path preflight (`capture-path`: the directory is creatable or already exists, and neither `manifest.json` nor any `<id>.stdout` exists; this step writes nothing) → create the directory owner-only (0700; files 0600, `create_new`) → spawn. |
| Statuses | `invalid` iff the report has a **precondition** error (`usage, config-unreadable, invalid-config, unknown-source, unknown-card, conflicting-selection, empty-scope, data-unreadable, capture-path, report-too-large`; all arise before any spawn except `report-too-large`, which replaces the report); a `capture-write` error arises **after execution**, is carried in `errors[]` and gives status `findings`, exit 1, with the full report still printed; else `findings` if any check is `fail`; else `undetermined` if a **required** check is `unknown`; else `ok`. Exit 0, 1, 2, 3. `required` is true for `config`, `request`, `execution`, `dataset`, `cards`, `freshness`; false for `coverage`, `appearance`. |
| Check chain (per source) | `request` ← `config`; `execution` ← `request` (replay/check: `not-run`, `replay does not run commands`); `dataset` ← `execution` (run) or the saved file (replay); `cards` evaluates after `request` passes (`fail` iff any card outcome is `refused` or `unavailable`); `freshness` ← `execution`, `pass` only when this invocation's attempt succeeded and the source is `Fresh` at `model_at`; `coverage` ← `dataset` (`pass` iff the dataset sets `observed_through`, else `unknown`; replay and check `not-run`). A skipped check is `not-run` with `why` starting `dependency failed: <check>` (or `dependency unknown: <check>` for a deferred request). All eight ids appear in the top-level `checks` and in each `sources[].result`, in this order: `config, request, execution, dataset, cards, freshness, coverage, appearance`. A source whose execution failed never has `freshness` `pass`, and the aggregate cannot be `pass` while any source failed. Aggregate: `fail` if any source failed, else `unknown` if any unknown, else `not-run` if any did not run, else `pass`. |
| Source blocks | `data_origin` (`none` check, `replay`, `run`); `clocks {request_at, model_at}` (run: `request_at` = resolved `window.end`, `model_at` = wall instant the attempt became terminal; replay: `--at` or the capture's recorded pair); `request {window, calendar_days, from, to, timezone, executable, argc[, argv]}`; `attempt {latency_ms, exit}` or `null` (`exit` is the process status or null when it did not exit normally); `dataset {status: valid\|absent\|refused, range, intervals, series, dimensions, metrics[{id,label,unit,quantity,estimate,unknown_contribution,sampled}], generated_at, observed_through, revision, notices_count, notices[]}`; `error {kind, text, stderr_excerpt}`; `result`. `error.kind` ids: `invalid-config, missing-executable, spawn, nonzero-exit, timeout, output-bound, cancelled, parse, unsupported-version, invalid-mapping, invalid-dataset` (add `pub fn id(self) -> &'static str` to `ChartErrorKind`; `card-mismatch` is per card). |
| Card blocks | `id, source, kind, intent, outcome {kind[, reason, subject, wanted, available, limit, count, maximum, source_error, why]}, summary {reduction, exact, quality, bound, known_subtotal, signed, newest_is_null, observed {at, end}}, intervals, groups {by, total, named_shown, other_members, unknown_shown, capacity, legend_shown, legend_overflow}, rows[{group (values array, null for Other), role, members, exact, known_subtotal, quality}], details {grain, rows_total, included[, offset, rows_returned, complete, rows[{interval {start,end}, sampled, group, shown_as, exact, known_subtotal, quality, unit, break_before}]]}, display {title, headline, badges, banner, message, footnote, accessible_name}, drawing {authoritative: false, axis {min,max,ticks}}`. Decimals are strings; `exact` is null when quality is `missing`. A card that did not plot has `rows: []`, `summary: null` unless facts carry one. All numbers a consumer asserts are in `intent`/`outcome`/`summary`/`rows`/`details`, never parsed out of `display`. |
| Serialization | `serde` derive structs (declaration order), `serde_json::to_string_pretty` plus a trailing newline, UTF-8; size bound measured on those bytes. `report-too-large` names the first card, in report order, at which the running total passes 1 MiB. |
| `errors[]` | `{id, text, key?, card?, source?}`; ids exactly: `usage, config-unreadable, invalid-config, unknown-source, unknown-card, conflicting-selection, empty-scope, data-unreadable, capture-path, capture-write, report-too-large`. `invalid-config.key` is the structured key path (`chart_sources[0].refresh_seconds`) from a new `ChartConfigIssue { key, message }` returned by an internal `validate_chart_config_detailed`; `validate_chart_config` keeps its string output byte-for-byte. A TOML parse error has no key and its text is flattened and bounded to 400 characters. Invalid reports still contain `schema`, `command` (null when no verb), `status`, `errors`, and empty `checks`, `sources`, `cards`. |
| `schema` | Prints one JSON document (not a report) `{schema: "sidescope-chart-schema-v1", report_schema, sections}`; `--section` accepts `source, card, mapping, mapping_dimension, mapping_metric, payload, limits, pointer_scopes, error_kinds, report` and prints only it; `--format text` prints the Markdown key matrix (the committed matrix is that text). Reads no configuration. |
| Text | First line `sidescope charts <verb>: <status> (exit <n>)`, then one line per check, source and card (`id: outcome — headline value`); built from the report value, never from a second computation. |
| One-shot | `ChartRuntime::start_one_shot(sources, normalizer)`; `OneShot::recv() -> Option<Attempt>` (blocks for the next terminal source, `None` when all are terminal); `OneShot::finish(self)` stops owned groups and reaps. `Attempt { source_id, request: Option<ResolvedRequest>, ended_at, latency_ms, exit: Option<i32>, stdout: Option<Vec<u8>> (exit 0 within the bound), stderr_excerpt, still_deferred, result: Result<Arc<ChartDataset>, ChartError> }`. `Scheduler::new_with_policy(sources, max_concurrent, RunPolicy::{Continuous, OneShot})`: under `OneShot`, a source gets no next due after its first terminal attempt; a deferral is retried once after 1 s, a second deferral is terminal (`still_deferred`). `execute()` is refactored into one function used by both policies. Process runner: add a raw-bytes entry (P-6) and keep `run_with_stderr` behavior. |
| Capture | Per design [§5.5](chart-authoring-and-debugging.md#55-bounds-privacy-and-authority). `manifest.json` (`sidescope-chart-capture-v1`): `{schema, complete, invocation_at, sources[{id, window {from,to}, timezone, clocks {request_at, model_at}, bytes, captured, outcome}]}`; `outcome` is `ok` or the error id; `bytes` is 0 when not captured. A stdout file is written when its source becomes terminal, byte-exact, with `create_new`; the manifest last. A write or exclusive-create failure is `capture-write`, status `findings`, nothing retried or deleted, manifest says `complete: false` and `captured: false` for that source. |

### Preconditions

Task 1 has landed (`rg -n "pub fn reduce_card" src/chart/facts.rs` finds it). `./scripts/native-env.sh cargo test` green. Baseline counts at least those listed above plus Task 1's. Stop if `reduce_card` has changed shape from the frozen interface.

### Tests first (public CLI, drive the binary)

Shared helper `tests/common/charts.rs`: `binary()` = `env!("CARGO_BIN_EXE_sidescope")`; scratch dir under `CARGO_TARGET_TMPDIR`; `write_config(dir, toml)`; `fake_producer(dir, name, script_body)` writing an executable `sh` script that appends `$$` to a marker file; `run(args, env) -> (code, stdout_json, stderr)`; `semantic_facts(report)` returning, per source, `dataset` and `error.kind`, and per card `outcome, intent, summary, intervals, groups, rows` (equality excludes `display`, `attempt`, `clocks`, `checks`, `data_origin`).

**No GUI start** — one fake Wayland socket for every verb:

```rust
#[test]
fn no_verb_touches_a_display() {
    let dir = scratch("nogui");
    let socket = dir.join("wayland-0");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    for args in [vec!["charts", "schema"], vec!["charts", "check", "--config", &cfg], /* replay, run */] {
        run_with_env(&args, &[("WAYLAND_DISPLAY", "wayland-0"),
                              ("XDG_RUNTIME_DIR", dir.to_str().unwrap()), ("DISPLAY", "")]);
    }
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
}
```

**Explicit execution and preconditions**: a config with sources `a` and `b`, each a fake producer appending to its own marker. `run --source a` leaves `b`'s marker absent and `a`'s with exactly one line. Table test over invalid invocations, each asserting exit 2, JSON `errors[0].id`, **both markers absent**, and no file under the capture directory (the directory itself absent): no selector; `--source a --all-sources`; `--source nope`; `--source a --source a`; `--card nope`; `--card` of `b` with `--source a`; `--at …` on run; capture directory containing `a.stdout`; capture directory containing `manifest.json`; capture parent missing; invalid config (names `chart_sources[0].refresh_seconds`); missing explicit `--config` (`config-unreadable`); empty config (`empty-scope`); `--details nope`; `--limit 0`; `--limit 5001`; `--offset` without `--details`. Fast+slow: `run --all-sources` with `a` instant and `b` sleeping 6.5 s, both `refresh_seconds = 5`: each marker has exactly one line.

**Status and case matrix** (replay with `--data` files and `--at 2026-11-20T15:00:30Z`, assert on the typed fields): empty series → `no-observations` exit 0; intervals none → `no-intervals`; all null → `no-known-values`; all zero → `plotted` with `summary.exact == "0"`; sum with a missing cell → `exact: null` and `known_subtotal` present; gauge newest null → `newest_is_null`; mismatched metric card with a plotted sibling → `refused` `card-mismatch` `subject: "metric"`, sibling plotted, source `valid`, exit 1; 17 gauge series → `refused` `capacity`; Details paging fields consistent across pages (`rows_total`, `offset`, `rows_returned`, `complete`) and the sum of `known_subtotal` over all pages equals `summary.known_subtotal` while Other is not double counted; byte-identical stdout on two runs; replay `freshness` `not-run`; unreadable and 1,048,577-byte data files `data-unreadable`, a 1,048,576-byte valid one accepted; check on a deferred request (`calendar_days = 1`, `timezone = "America/New_York"`, `--at 2026-11-20T05:00:00Z`) → `request: unknown`, exit 3; check exit 0 with `dataset` and `cards` `not-run` and every card `not-evaluated`; schema ignores an invalid default config; schema test both ways: every key named by TOML's own "expected one of" error for an unknown key in each table equals the descriptor key set, and every descriptor key with its `example` parses; boundary values accepted and one beyond rejected for each numeric range.

**Capture** (the contract's hard cases): producer printing `{not json` with exit 0 → run exit 1, source `error.kind == "parse"`, `<id>.stdout` holds exactly those bytes, manifest `captured: true`, `outcome: "parse"`; producer printing non-UTF-8 bytes with exit 0 → same, bytes exact (P-6); producer exit 2, a timeout (`timeout_seconds = 1`, `sleep 5`), output over 1 MiB, a non-executable file and a missing executable write **no** `.stdout` and the manifest says `captured: false` with the error id; `0600` file and `0700` new directory modes; a multi-source capture then `replay --from-capture DIR` equals the run's `semantic_facts` for sources whose stdout was retained, replays the recorded-failed source as `unavailable` with the recorded error id (not no data), reports `data_origin: "replay"`, `attempt: null`, `execution`/`freshness` `not-run`; a producer that creates `<dir>/b.stdout` after the preflight makes `b` a `capture-write` finding without overwriting that file and with `complete: false`; a producer that makes the directory read-only leaves earlier files intact, no manifest, status `findings`, and `--from-capture` of it exits 2 `data-unreadable`; a manifest naming a source absent from the config is `unknown-source`. Equality is asserted only over evidence actually retained.

`report-too-large`: a dataset of 5000 Details rows with 500-character group values plus `--details`, expect exit 2, `errors[0].id == "report-too-large"`, `errors[0].card`, and no `cards` content.

Unit tests (in `scheduler.rs`): under `OneShot` a finished source is never started again after `Tick` at +10 minutes; one deferral retries, the second is terminal; `Continuous` behavior is unchanged (existing tests untouched).

Red: `./scripts/native-env.sh cargo test --test charts_cli_usage` fails with exit 2 `unknown argument: charts` for every verb (witnessed at planning with `sidescope charts schema`).

### Change

`src/cli.rs`: `Command::Charts(Vec<String>)` for a first argument `charts`; top-level help gains one line and a pointer; `main.rs`: match `Command::Charts` **before** any GTK or config code and `std::process::exit(chart_cli::run(args))` (this is what keeps GUI initialization impossible; `doctor`, `--demo`, `--report`, `--browser` unchanged). `lib.rs`: `pub mod chart_cli;` plus a detailed config entry point (`parse_config_detailed`). `chart/schema.rs`: `KeyDescriptor` tables for source, card, mapping, dimension, metric keys (name, type, required, default, range or enum, scope, example, help), referencing the same constants validation uses (`MIN_REFRESH_SECONDS`, the timeout and top-n bounds, `MAX_CALENDAR_DAYS`); `help_text()` and `charts --help` read the same tables. `chart_runtime.rs`, `scheduler.rs`, `process.rs` as pinned. Reports are built only from `reduce_card`/`present_card`, `SourceSnapshot` and `Attempt`.

### Gates

`just format`; `just lint`; `just test`. `--help` for `charts` and each verb lists the tables, the authority warning on `run`, and the cookbook path `docs/chart-cookbook.md` (Task 4 writes it; the help test asserts the text, not the file).

### Fences

Must not touch: `src/chart/dataset.rs`, `src/chart/facts.rs`, `src/chart/reduce.rs`, `src/chart_view/*`, `src/chart_mapping.rs` (Task 1 owns them; a needed change is a stop-and-report), `examples/`, `tests/fixtures/charts/upstream|quota|producers`, any Task 1 or 2 test file, `docs/`. The root exports are yours alone: `src/lib.rs` gains only `pub mod chart_cli;` and the detailed parser; `src/chart/mod.rs` gains only `pub mod schema;`. Shared module ownership after this task: `chart::facts` and `chart_view` = Task 1; `chart_cli`, `chart::schema`, `chart_runtime`, `process` = Task 3.

### Commit message

`feat(cli): headless charts schema, check, replay and one-shot run with capture`

### Report

Red output, green gates, `git diff --stat`, list of descriptor tables, the measured wall time of the fast+slow test, and any report field you had to add (additive fields only).

---

## Task 4 — compatibility tests, cookbook, key matrix, guide

**After Tasks 1, 2 and 3.** One commit.

### Settled

Design [§8.1](chart-authoring-and-debugging.md#81-compatibility-tests) table, criteria 4, 11, 12, 18 to 20, 22 and the documentation set [§8.2](chart-authoring-and-debugging.md#82-documentation-set). The fixtures are the imported and invented ones from Task 2; tests drive `charts replay` and the wrappers' `--convert-stdin` mode, then `charts replay --data` on the converted file, through the real binary.

### Tests first

`tests/chart_compat.rs`, one test per row of design [§8.1](chart-authoring-and-debugging.md#81-compatibility-tests), with the expectation written by hand (jq over the fixture in a comment as the derivation). Use `--at 2026-11-16T15:04:05Z` unless the case needs another. Pinned:

| Fixture | Assertion |
| :--- | :--- |
| `series-empty.json` | `outcome.kind == "no-observations"`, exit 0 |
| `series-zero-only.json` | `plotted`, `summary.exact == "0"`, quality `complete` |
| `series-unpriced.json` (copilot metric) | quality `partial`, `bound == "at-least"`, `known_subtotal == "0.02131"` |
| `series-subscription.json` | metered `summary.exact == "0.0005"` complete; subscription responses stacked by channel unaffected by it |
| `series-open-bucket.json` | `open_right_edge == true`, `intervals.partial == 2`, `complete == 1` |
| `series-dst-23/25-hour-day.json` | Details rows: second interval is 82,800 s (23 h) / 90,000 s (25 h) from start to end |
| `series-objects.json` | `dataset.status == "refused"`, `error.kind == "invalid-mapping"` (P-1), exit 1, never `plotted` |
| `error-too-many-buckets.json` as stdout | `invalid-mapping`; as a fake producer exiting 2 with that text on stderr, `nonzero-exit` |
| `capabilities.json` vs profiles | Task 2's agreement assertions, plus the copy without quota descriptors fails the documented jq check |
| quota percent through the estimate wrapper | exact `"0.5"` and `"2"` in Details, notices present; `bounded_percent = true` on that card is `refused`/`card-mismatch`/`subject: "bounded-percent"` |
| estimate and observed cases | each stated outcome through conversion then replay: model with no weight is `partial`/`at-least`; window sum above 100 plotted with no 0 to 100 axis; missing `quotaWeights` is a non-zero wrapper exit; observed: two accounts × windows unmerged, reset breaks and no delta, nulls missing, long gap breaks, truncated notice, empty journal `no-observations`, `at: []` window `no-intervals`, all-null `no-known-values`, newest-null headline keeps the last reading with `newest_is_null` |
| late correction | a fake source answers "before" then "after" (counter file) under `ChartRuntime` with `refresh_now`; the published snapshot's `series()`, `intervals()`, `range()`, `metrics()` and `revision()` equal those parsed from "after" alone |
| filtered `recent` | against a group-none fixture: `plotted`, `groups.total == 1`, `check` passes for legacy and filtered profiles |

`tests/chart_cookbook.rs` extracts the fenced `bash` blocks marked `<!-- cookbook: run -->` from `docs/chart-cookbook.md`, runs each as written in a scratch directory with the binary on `PATH` and `jq` present (fail loudly if `jq` is missing: it is in the dev shell), and asserts the output lines the cookbook prints beneath each block. A second test regenerates `charts schema --format text` and compares it to `docs/reference/command-charts/key-matrix.md` after its fixed generated header. Red: the cookbook and matrix do not exist.

### Change

`docs/chart-cookbook.md` (worked three times: the money dashboard, the quota cards with wrappers/feature checks/limits, and `synthetic-latency.sh` as a gauge with `summary = "last"`), the troubleshooting flow (start from `status` and `outcome.kind`, branch by error kind to one next command), profile notes (legacy vs filtered vs quota, what a successful probe does not establish, why the shared subscription estimate is not carried forward, points vs readings, the fitted-weights basis, the API-key caveat, R7 project meaning, "adoption is copying tables by hand", captures stay out of version control), and the generated key matrix. Overlapping shipped references with claims now false or incomplete; rewrite to the new behavior, do not patch around: [`guide.md`](../guide.md) section 7 last paragraph ("not implemented yet") and the `doctor` sentence; [`README.md`](../../README.md) line 14 and 83 (authoring path, `charts run` authority); [`reference/command-charts/README.md`](../reference/command-charts/README.md) "Not … an agent-activity integration" and "Verified against" stamps; [`command-runtime.md`](../reference/command-charts/command-runtime.md) (one-shot policy, `doctor` sentence, module list line 114); [`tests/cli.rs`](../../tests/cli.rs) help assertions are appended, not edited. `Justfile`: recipe `charts-matrix` regenerating the matrix file. Plan frontmatter: set `stage: BUILT` only when every task has landed; keep the roadmap link until graduation is done.

### Gates, fences, commit

`just format`; `just lint`; `just test`; `uvx vantage-check --strict` on every Markdown file changed. Fences: no `src/` changes (a needed change is a stop-and-report to the owning task); no edits to Task 2 fixtures except by adding files. Commit: `docs(charts): authoring cookbook, key matrix and compatibility tests`.

---

## Ships with

- **Tests by level**: unit (`scheduler.rs` one-shot, facts cases), integration (`tests/chart_facts.rs`, `chart_compat.rs`, `chart_quota_wrappers.rs`, `chart_profiles.rs`), public CLI (`charts_cli_*.rs`), documentation (`chart_cookbook.rs`). The tests that would catch the likely regressions end to end: the fake-Wayland-socket test (GUI creep), the marker-file counts (hidden execution), the capture byte tests (silent truncation), the `series-objects.json` replay (missing read as zero).
- **Tests that must not be repaired to green**: none are expected to break. If an existing assertion about `Latest`/`Last known` gauge headlines, `table_rows()` order or refusal wording fails, the behavior changed by mistake: fix the code.
- **Docs now describing the old thing**: listed in Task 1 (reference pages) and Task 4 (guide, README, reference README and runtime page, help text in [`cli.rs`](../../src/cli.rs)).
- **Surfaces**: `config.example.toml` gains one comment line pointing to `sidescope charts check`; help text; the Justfile recipe; `flake.nix` and `doctor.sh` for `python3`; CLI error ids and the report schema (additive-only within v1).
- **Cheap and yours**: internal module layout under `src/chart_cli/`; names of private helpers; whether `GroupRow` carries extra drawing fields; wording of `text` output lines beyond the first.
- **Stop and ask**: any change to a frozen name, a design enumeration, a field on a "never add" type, or the report/capture/schema ids; any need to run a real producer, credential or collector.

## Don't

- Don't make `doctor`, `--demo`, `--report` or `--browser` execute anything, and don't add automatic capability probing or query rewriting.
- Don't recompute a reduction in the CLI; don't parse display strings; don't put raw stdout in a report.
- Don't widen the mapper beyond the parent guard (P-1) or teach it quota or ledger field names; wrappers are the place for producer-specific code.
- Don't merge the quota cards into the legacy or filtered profile, overlay estimates on readings, or infer an account for an estimate.
- Don't use `/tmp`; don't commit captures or account data; don't edit anything under the producer's checkout.

## Blockers

None. The one discrepancy found (P-1) was ruled by the supervisor and is recorded above; every other pin is a mapping of the design.
