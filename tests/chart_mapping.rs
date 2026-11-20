//! Public-boundary tests for the generic `columns-v1` mapper, the opt-in
//! usage-cost profile and the synthetic example commands. Fixtures are
//! hand-written synthetic answers; no ledger, profile or account is read.

use serde_json::{Value, json};
use sidescope::chart::config::{ChartSourceConfig, ResolvedRequest, resolve_request};
use sidescope::chart::dataset::{ChartDataset, Quality, parse_native_snapshot};
use sidescope::chart::decimal::ChartDecimal;
use sidescope::chart::error::{ChartError, ChartErrorKind};
use sidescope::chart::time::{Timestamp, parse_instant};
use sidescope::chart_mapping::{map_columns, normalize_output};
use sidescope::parse_config;
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn ts(text: &str) -> Timestamp {
    parse_instant(text).unwrap()
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/{relative}")).unwrap()
}

fn replaced(base: &str, old: &str, new: &str) -> String {
    assert!(base.contains(old), "test pattern {old:?} not in base");
    base.replacen(old, new, 1)
}

/// One-metric inline mapping. `extra_mapping` lines land in the mapping
/// table, `metric_lines` in the single metric.
fn inline_config(dimensions: &str, extra_mapping: &str, metric_lines: &str) -> String {
    format!(
        r#"
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
dimensions = {dimensions}
{extra_mapping}

[[chart_sources.mapping.metrics]]
id = "cost"
label = "Cost"
unit = "USD"
quantity = "interval_total"
values = "/cost"
{metric_lines}
"#
    )
}

const ONE_DIMENSION: &str = r#"[{ name = "project", pointer = "/keys/0" }]"#;
const COUNTER: &str = "quality_counter = \"/unpriced\"";

fn plain() -> String {
    inline_config(ONE_DIMENSION, "", "")
}

fn with_counter() -> String {
    inline_config(ONE_DIMENSION, "", COUNTER)
}

fn source_of(config: &str, id: &str) -> ChartSourceConfig {
    parse_config(config)
        .unwrap()
        .chart_sources
        .into_iter()
        .find(|source| source.id == id)
        .unwrap()
}

fn request_for(source: &ChartSourceConfig, now: &str) -> ResolvedRequest {
    resolve_request(source, ts(now), 1).unwrap()
}

fn map(config: &str, raw: &Value) -> Result<ChartDataset, ChartError> {
    let source = source_of(config, "ledger");
    let request = request_for(&source, "2026-11-20T12:00:00Z");
    map_columns(raw, source.mapping.as_ref().unwrap(), &request)
}

fn minute_buckets(count: usize) -> Vec<Value> {
    (0..count)
        .map(|i| {
            json!({
                "start": format!("2026-11-20T{:02}:{:02}:00Z", 9 + i / 60, i % 60),
                "end": format!("2026-11-20T{:02}:{:02}:00Z", 9 + (i + 1) / 60, (i + 1) % 60),
            })
        })
        .collect()
}

fn answer(groups: Value, buckets: usize) -> Value {
    json!({"buckets": minute_buckets(buckets), "groups": groups})
}

fn group(name: &str, columns: Value) -> Value {
    let mut group = json!({"keys": [name]});
    for (key, value) in columns.as_object().unwrap() {
        group[key] = value.clone();
    }
    group
}

type Observed = Vec<(Option<String>, Quality)>;

fn observed(dataset: &ChartDataset, series: usize, metric: &str) -> Observed {
    dataset.series()[series].metrics[metric]
        .iter()
        .map(|cell| {
            (
                cell.value.as_ref().map(ChartDecimal::to_plain_string),
                cell.quality,
            )
        })
        .collect()
}

fn cells(rows: &[(Option<&str>, Quality)]) -> Observed {
    rows.iter()
        .map(|(value, quality)| (value.map(str::to_string), *quality))
        .collect()
}

fn refused(config: &str, raw: &Value, kind: ChartErrorKind) -> ChartError {
    let error = map(config, raw).unwrap_err();
    assert_eq!(error.kind, kind, "{error}");
    error
}

use Quality::{Complete, Missing, Partial};

// ---------- omitted-zero policy and missing cells ----------

#[test]
fn an_omitted_column_is_missing_by_default_and_zero_only_when_opted_in() {
    let raw = answer(json!([{"keys": ["Example A"]}]), 2);
    let default = map(&with_counter(), &raw).unwrap();
    assert_eq!(
        observed(&default, 0, "cost"),
        cells(&[(None, Missing), (None, Missing)])
    );
    let zero = map(
        &inline_config(ONE_DIMENSION, "", &format!("{COUNTER}\nomitted = \"zero\"")),
        &raw,
    )
    .unwrap();
    assert_eq!(
        observed(&zero, 0, "cost"),
        cells(&[(Some("0"), Complete), (Some("0"), Complete)])
    );
}

#[test]
fn explicit_nulls_stay_missing_even_when_omitted_columns_mean_zero() {
    let config = inline_config(ONE_DIMENSION, "", &format!("{COUNTER}\nomitted = \"zero\""));
    let raw = answer(
        json!([
            group(
                "Example A",
                json!({"cost": ["0.5", null], "unpriced": [0, 0]})
            ),
            group("Example B", json!({"cost": null, "unpriced": [0, 0]})),
            group(
                "Example C",
                json!({"cost": ["1", "2"], "unpriced": [0, null]})
            ),
        ]),
        2,
    );
    let dataset = map(&config, &raw).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[(Some("0.5"), Complete), (None, Missing)])
    );
    assert_eq!(
        observed(&dataset, 1, "cost"),
        cells(&[(None, Missing), (None, Missing)])
    );
    assert_eq!(
        observed(&dataset, 2, "cost"),
        cells(&[(Some("1"), Complete), (None, Missing)])
    );
}

#[test]
fn a_positive_unpriced_counter_makes_a_partial_cell_that_keeps_the_known_portion() {
    let raw = answer(
        json!([group(
            "Example A",
            json!({"cost": ["0.5", "1.25", "2"], "unpriced": [0, 2, 0]})
        )]),
        3,
    );
    let dataset = map(&with_counter(), &raw).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[
            (Some("0.5"), Complete),
            (Some("1.25"), Partial),
            (Some("2"), Complete)
        ])
    );
}

#[test]
fn values_without_their_quality_counter_cannot_claim_completeness_unless_zero_is_declared() {
    let raw = answer(json!([group("Example A", json!({"cost": ["0.5"]}))]), 1);
    let dataset = map(&with_counter(), &raw).unwrap();
    assert_eq!(observed(&dataset, 0, "cost"), cells(&[(None, Missing)]));
    let zero = map(
        &inline_config(ONE_DIMENSION, "", &format!("{COUNTER}\nomitted = \"zero\"")),
        &raw,
    )
    .unwrap();
    assert_eq!(
        observed(&zero, 0, "cost"),
        cells(&[(Some("0.5"), Complete)])
    );
}

#[test]
fn a_metric_without_a_quality_counter_is_complete_where_it_has_values() {
    let raw = answer(json!([group("Example A", json!({"cost": ["3", null]}))]), 2);
    let dataset = map(&plain(), &raw).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[(Some("3"), Complete), (None, Missing)])
    );
}

#[test]
fn empty_groups_and_all_null_data_are_successful_no_observations() {
    let empty = map(&plain(), &answer(json!([]), 2)).unwrap();
    assert!(empty.series().is_empty());
    assert_eq!(empty.intervals().len(), 2);

    let nulls = answer(
        json!([group("Example A", json!({"cost": [null, null]}))]),
        2,
    );
    let dataset = map(&plain(), &nulls).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[(None, Missing), (None, Missing)])
    );

    let no_buckets = map(&plain(), &answer(json!([]), 0)).unwrap();
    assert!(no_buckets.intervals().is_empty());
    assert!(no_buckets.series().is_empty());
}

// ---------- shapes, numbers, ordering ----------

#[test]
fn columns_that_do_not_align_with_the_buckets_refuse_the_whole_answer() {
    for cost in [json!(["1"]), json!(["1", "2", "3"])] {
        let raw = answer(json!([group("Example A", json!({"cost": cost}))]), 2);
        refused(&plain(), &raw, ChartErrorKind::InvalidDataset);
    }
    let short_counter = answer(
        json!([group(
            "Example A",
            json!({"cost": ["1", "2"], "unpriced": [0]})
        )]),
        2,
    );
    refused(
        &with_counter(),
        &short_counter,
        ChartErrorKind::InvalidDataset,
    );
}

#[test]
fn numbers_are_decimal_strings_or_integers_never_floats_or_exponents() {
    let ok = answer(
        json!([group("Example A", json!({"cost": [3, "0.25", -2]}))]),
        3,
    );
    let dataset = map(&plain(), &ok).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[
            (Some("3"), Complete),
            (Some("0.25"), Complete),
            (Some("-2"), Complete)
        ])
    );
    for bad in [
        json!(1.5),
        json!("1e3"),
        json!("+1"),
        json!(" 1"),
        json!(true),
        json!("0.1234567890123456789"),
    ] {
        let raw = answer(json!([group("Example A", json!({"cost": [bad]}))]), 1);
        refused(&plain(), &raw, ChartErrorKind::InvalidDataset);
    }
    for bad in [json!(-1), json!(1.0), json!("1"), json!(true)] {
        let raw = answer(
            json!([group(
                "Example A",
                json!({"cost": ["1"], "unpriced": [bad]})
            )]),
            1,
        );
        refused(&with_counter(), &raw, ChartErrorKind::InvalidDataset);
    }
}

#[test]
fn unordered_overlapping_and_inverted_buckets_are_refused_by_dataset_validation() {
    let at = |a: &str, b: &str| json!({"start": a, "end": b});
    let cases = [
        vec![
            at("2026-11-20T09:01:00Z", "2026-11-20T09:02:00Z"),
            at("2026-11-20T09:00:00Z", "2026-11-20T09:01:00Z"),
        ],
        vec![
            at("2026-11-20T09:00:00Z", "2026-11-20T09:02:00Z"),
            at("2026-11-20T09:01:00Z", "2026-11-20T09:03:00Z"),
        ],
        vec![at("2026-11-20T09:02:00Z", "2026-11-20T09:01:00Z")],
        vec![at("not a time", "2026-11-20T09:01:00Z")],
    ];
    for buckets in cases {
        let raw = json!({"buckets": buckets, "groups": []});
        refused(&plain(), &raw, ChartErrorKind::InvalidDataset);
    }
}

#[test]
fn answers_beyond_the_shared_limits_are_refused_before_mapping() {
    let raw = json!({"buckets": minute_buckets_wide(2_049), "groups": []});
    refused(&plain(), &raw, ChartErrorKind::InvalidDataset);
    let groups: Vec<Value> = (0..257)
        .map(|i| group(&format!("g{i}"), json!({})))
        .collect();
    refused(
        &plain(),
        &answer(json!(groups), 1),
        ChartErrorKind::InvalidDataset,
    );
}

fn minute_buckets_wide(count: usize) -> Vec<Value> {
    (0..count as i64)
        .map(|i| {
            let at = |n: i64| {
                Timestamp::from_second(ts("2026-11-18T00:00:00Z").as_second() + n * 60)
                    .unwrap()
                    .to_string()
            };
            json!({"start": at(i), "end": at(i + 1)})
        })
        .collect()
}

// ---------- identity, corrections, partial current bucket ----------

#[test]
fn ordered_key_tuples_form_series_ids_without_delimiter_collisions() {
    let config = inline_config(
        r#"[{ name = "a", pointer = "/keys/0" }, { name = "b", pointer = "/keys/1" }]"#,
        "",
        "",
    );
    let raw = answer(
        json!([
            {"keys": ["a|b", "c"], "cost": ["1"]},
            {"keys": ["a", "b|c"], "cost": ["2"]},
            {"keys": ["a", "b", "c"], "cost": ["3"]},
        ]),
        1,
    );
    let dataset = map(&config, &raw).unwrap();
    let ids: Vec<&str> = dataset.series().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        3
    );
    assert_eq!(dataset.series()[0].dimensions["a"], "a|b");
    assert_eq!(dataset.series()[1].dimensions["b"], "b|c");
    let again = map(&config, &raw).unwrap();
    assert_eq!(
        ids,
        again
            .series()
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn two_groups_with_one_dimension_tuple_are_refused() {
    let raw = answer(
        json!([
            group("Example A", json!({"cost": ["1"]})),
            group("Example A", json!({"cost": ["2"]}))
        ]),
        1,
    );
    refused(&plain(), &raw, ChartErrorKind::InvalidDataset);
}

#[test]
fn a_later_answer_with_a_corrected_old_bucket_replaces_rather_than_accumulates() {
    let before = answer(json!([group("Example A", json!({"cost": ["1", "2"]}))]), 2);
    let after = answer(json!([group("Example A", json!({"cost": ["1", "5"]}))]), 2);
    let first = map(&plain(), &before).unwrap();
    let second = map(&plain(), &after).unwrap();
    assert_eq!(
        observed(&first, 0, "cost"),
        cells(&[(Some("1"), Complete), (Some("2"), Complete)])
    );
    assert_eq!(
        observed(&second, 0, "cost"),
        cells(&[(Some("1"), Complete), (Some("5"), Complete)])
    );
}

#[test]
fn a_bucket_ending_after_the_request_instant_is_partial() {
    let late = map(
        &plain(),
        &json!({
            "buckets": [
                {"start": "2026-11-20T11:59:00Z", "end": "2026-11-20T12:00:00Z"},
                {"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:01:00Z"},
            ],
            "groups": [group("Example A", json!({"cost": ["1", "2"]}))],
        }),
    )
    .unwrap();
    assert_eq!(
        observed(&late, 0, "cost"),
        cells(&[(Some("1"), Complete), (Some("2"), Partial)])
    );
}

#[test]
fn daylight_saving_buckets_keep_their_real_duration() {
    let spring = json!({
        "buckets": [
            {"start": "2026-03-07T05:00:00Z", "end": "2026-03-08T05:00:00Z"},
            {"start": "2026-03-08T05:00:00Z", "end": "2026-03-09T04:00:00Z"},
        ],
        "groups": [group("Example A", json!({"cost": ["1", "2"]}))],
    });
    let seconds: Vec<i64> = map(&plain(), &spring)
        .unwrap()
        .intervals()
        .iter()
        .map(|i| i.seconds())
        .collect();
    assert_eq!(seconds, [86_400, 82_800]);
}

// ---------- metadata ----------

#[test]
fn range_and_metadata_pointers_feed_the_dataset_and_default_sensibly() {
    let defaults = map(&plain(), &answer(json!([]), 2)).unwrap();
    assert_eq!(defaults.range().start, ts("2026-11-20T09:00:00Z"));
    assert_eq!(defaults.range().end, ts("2026-11-20T09:02:00Z"));
    assert_eq!(defaults.generated_at(), ts("2026-11-20T12:00:00Z"));
    assert_eq!(defaults.observed_through(), None);
    assert_eq!(defaults.revision(), None);
    assert!(defaults.notices().is_empty());

    let config = inline_config(
        ONE_DIMENSION,
        r#"range_start = "/range/from"
range_end = "/range/to"
generated_at = "/generatedAt"
observed_through = "/seen"
revision = "/generation"
notices = ["/note"]"#,
        "",
    );
    let mut raw = answer(json!([]), 2);
    raw["range"] = json!({"from": "2026-11-20T08:59:00.000Z", "to": "2026-11-20T09:05:00.000Z"});
    raw["generatedAt"] = json!("2026-11-20T11:59:57.250Z");
    raw["seen"] = json!(null);
    raw["generation"] = json!(4021);
    raw["note"] = json!("Synthetic note.");
    let dataset = map(&config, &raw).unwrap();
    assert_eq!(dataset.range().start, ts("2026-11-20T08:59:00Z"));
    assert_eq!(dataset.range().end, ts("2026-11-20T09:05:00Z"));
    assert_eq!(dataset.generated_at(), ts("2026-11-20T11:59:57.250Z"));
    assert_eq!(dataset.observed_through(), None);
    assert_eq!(dataset.revision(), Some("4021"));
    assert_eq!(dataset.notices(), ["Synthetic note."]);
    assert_eq!(dataset.timezone().name(), "UTC");
    raw["seen"] = json!("2026-11-20T11:58:00Z");
    assert_eq!(
        map(&config, &raw).unwrap().observed_through(),
        Some(ts("2026-11-20T11:58:00Z"))
    );
}

#[test]
fn metric_descriptors_come_from_the_declaration() {
    let config = inline_config(
        ONE_DIMENSION,
        "",
        "estimate = true\nunknown_contribution = \"nonnegative\"",
    );
    let dataset = map(&config, &answer(json!([]), 1)).unwrap();
    let metric = dataset.metric("cost").unwrap();
    assert_eq!(
        (metric.unit.as_str(), metric.label.as_str()),
        ("USD", "Cost")
    );
    assert!(metric.estimate);
    assert_eq!(
        metric.unknown_contribution,
        sidescope::chart::dataset::UnknownContribution::Nonnegative
    );
}

// ---------- pointers ----------

#[test]
fn pointers_decode_escapes_and_resolve_relative_to_their_scope() {
    let config = replaced(
        &plain(),
        "buckets = \"/buckets\"",
        "buckets = \"/a~1b/c~0d\"",
    );
    let raw = json!({
        "a/b": {"c~d": minute_buckets(1)},
        "groups": [group("Example A", json!({"cost": ["1"]}))],
    });
    let dataset = map(&config, &raw).unwrap();
    assert_eq!(dataset.series().len(), 1);
    // A group-relative pointer written as if absolute finds nothing in the group.
    let absolute = replaced(&plain(), "\"/keys/0\"", "\"/groups/0/keys/0\"");
    let error = refused(
        &absolute,
        &answer(json!([group("Example A", json!({"cost": ["1"]}))]), 1),
        ChartErrorKind::InvalidMapping,
    );
    assert!(error.message().contains("mapping.dimensions"), "{error}");
}

#[test]
fn bad_pointer_targets_are_mapping_errors_that_name_the_key_but_no_data() {
    let secret = "SECRET-424242";
    let with_secret = |mut raw: Value| {
        raw["note"] = json!(secret);
        raw
    };
    let cases: [(&str, Value, &str); 5] = [
        (
            "buckets = \"/nope\"",
            with_secret(answer(json!([]), 1)),
            "mapping.buckets",
        ),
        (
            "buckets = \"/buckets\"",
            with_secret(json!({"buckets": {"x": secret}, "groups": []})),
            "mapping.buckets",
        ),
        (
            "groups = \"/groups\"",
            with_secret(json!({"buckets": minute_buckets(1), "groups": secret})),
            "mapping.groups",
        ),
        (
            "interval_start = \"/start\"",
            with_secret(
                json!({"buckets": [{"end": "2026-11-20T09:01:00Z", "x": secret}], "groups": []}),
            ),
            "mapping.interval_start",
        ),
        (
            "dimensions = [{ name = \"project\", pointer = \"/keys/0\" }]",
            with_secret(
                json!({"buckets": minute_buckets(1), "groups": [{"keys": [424242], "cost": ["1"]}]}),
            ),
            "mapping.dimensions",
        ),
    ];
    for (declaration, raw, key) in cases {
        let config = if declaration == "buckets = \"/nope\"" {
            replaced(&plain(), "buckets = \"/buckets\"", declaration)
        } else {
            plain()
        };
        let error = refused(&config, &raw, ChartErrorKind::InvalidMapping);
        assert!(error.message().contains(key), "{error} should name {key}");
        assert!(!error.to_string().contains(secret), "{error}");
        assert!(!error.to_string().contains("424242"), "{error}");
    }
}

#[test]
fn a_values_pointer_that_is_present_but_not_an_array_is_a_mapping_error() {
    let raw = answer(json!([group("Example A", json!({"cost": "7"}))]), 1);
    let error = refused(&plain(), &raw, ChartErrorKind::InvalidMapping);
    assert!(
        error.message().contains("mapping.metrics.cost.values"),
        "{error}"
    );
}

// ---------- normalize_output ----------

#[test]
fn normalize_output_parses_columns_output_and_reports_malformed_json() {
    let source = source_of(&plain(), "ledger");
    let request = request_for(&source, "2026-11-20T12:00:00Z");
    let raw = answer(json!([group("Example A", json!({"cost": ["1"]}))]), 1);
    let text = serde_json::to_string(&raw).unwrap();
    let via_output = normalize_output(&source, &text, &request).unwrap();
    assert_eq!(
        via_output,
        map_columns(&raw, source.mapping.as_ref().unwrap(), &request).unwrap()
    );
    let error = normalize_output(&source, "{\"buckets\": [SECRET", &request).unwrap_err();
    assert_eq!(error.kind, ChartErrorKind::ParseFailure);
    assert!(!error.to_string().contains("SECRET"));
    let empty = normalize_output(&source, "", &request).unwrap_err();
    assert_eq!(empty.kind, ChartErrorKind::ParseFailure);
}

#[test]
fn normalize_output_delegates_native_sources_unchanged() {
    let config = r#"
[[chart_sources]]
id = "native"
command = ["example-chart-data"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"
"#;
    let source = source_of(config, "native");
    let request = request_for(&source, "2026-11-20T12:02:10Z");
    let text = read("tests/fixtures/charts/native/minimal.json");
    assert_eq!(
        normalize_output(&source, &text, &request).unwrap(),
        parse_native_snapshot(&text, &request).unwrap()
    );
    let error = normalize_output(&source, "[", &request).unwrap_err();
    assert_eq!(error.kind, ChartErrorKind::ParseFailure);
}

// ---------- the opt-in usage-cost profile ----------

const PROFILE: &str = "examples/charts/agent-activity.toml";

fn profile() -> String {
    read(PROFILE)
}

/// Profile rule: every USD metric names the counter that makes a cell partial.
fn metrics_without_a_quality_counter(config: &str) -> Vec<String> {
    parse_config(config)
        .unwrap()
        .chart_sources
        .iter()
        .flat_map(|source| source.mapping.iter().flat_map(|m| m.metrics.iter()))
        .filter(|metric| metric.unit == "USD" && metric.quality_counter.is_none())
        .map(|metric| metric.id.clone())
        .collect()
}

#[test]
fn the_profile_parses_declares_pricing_quality_and_opts_into_omitted_zero() {
    let config = parse_config(&profile()).unwrap();
    let ids: Vec<&str> = config.chart_sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["recent", "daily", "models", "subscription"]);
    assert_eq!(config.chart_cards.len(), 6);
    assert!(metrics_without_a_quality_counter(&profile()).is_empty());
    let stripped = replaced(&profile(), "quality_counter = \"/columns/unpriced\"\n", "");
    assert_eq!(
        metrics_without_a_quality_counter(&stripped),
        ["metered_cost"]
    );
    for source in &config.chart_sources {
        let mapping = source.mapping.as_ref().unwrap();
        for metric in &mapping.metrics {
            assert_eq!(
                metric.omitted,
                sidescope::chart::config::OmittedPolicy::Zero,
                "{} declares omitted columns are zero",
                metric.id
            );
        }
    }
    let by_id = |id: &str| config.chart_sources.iter().find(|s| s.id == id).unwrap();
    let metered = &by_id("recent").mapping.as_ref().unwrap().metrics[0];
    assert!(metered.label.contains("Metered list-price estimate"));
    assert!(metered.estimate);
    let subscription = &by_id("subscription").mapping.as_ref().unwrap().metrics;
    assert_eq!(subscription[0].quality_counter, None);
    assert_eq!(
        subscription[1].quality_counter.as_deref(),
        Some("/columns/subscriptionUnpriced")
    );
}

#[test]
fn the_profile_queries_fit_the_upstream_limits_and_the_two_presets_remain_valid() {
    let config = parse_config(&profile()).unwrap();
    for source in &config.chart_sources {
        let flag = |name: &str| {
            let at = source.command.iter().position(|a| a == name).unwrap();
            source.command[at + 1].as_str()
        };
        let buckets = match (flag("--bucket"), &source.window, source.calendar_days) {
            ("minute", Some(window), None) => {
                window.trim_end_matches('h').parse::<u32>().unwrap() * 60
            }
            ("day", None, Some(days)) => u32::from(days),
            other => panic!("unexpected query {other:?}"),
        };
        assert!(buckets <= 400, "{} asks for {buckets} buckets", source.id);
        assert_eq!(flag("--layout"), "columns");
        assert_eq!(flag("--format"), "json");
        assert_eq!(flag("--from"), "{from}");
        assert_eq!(flag("--to"), "{to}");
        assert_eq!(flag("--timezone"), "{timezone}");
    }
    let two_hours = replaced(&profile(), "window = \"3h\"", "window = \"2h\"");
    assert_eq!(
        source_of(&two_hours, "recent").window.as_deref(),
        Some("2h")
    );
    let three_days = profile().replace("calendar_days = 5", "calendar_days = 3");
    assert_eq!(source_of(&three_days, "daily").calendar_days, Some(3));
}

fn run_fake_ledger(source: &ChartSourceConfig, request: &ResolvedRequest) -> String {
    assert_eq!(source.executable(), "agent-activity");
    let output = Command::new("sh")
        .arg(format!("{ROOT}/examples/charts/fake-ledger.sh"))
        .args(&request.argv[1..])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    String::from_utf8(output.stdout).unwrap()
}

fn profile_dataset(id: &str, now: &str) -> ChartDataset {
    let source = source_of(&profile(), id);
    let request = request_for(&source, now);
    let text = run_fake_ledger(&source, &request);
    normalize_output(&source, &text, &request).unwrap()
}

#[test]
fn the_profile_maps_the_synthetic_minute_answer() {
    let dataset = profile_dataset("recent", "2026-11-20T12:00:30Z");
    assert_eq!(dataset.intervals().len(), 180);
    assert_eq!(dataset.revision(), Some("4021"));
    let names: Vec<&str> = dataset
        .series()
        .iter()
        .map(|s| s.dimensions["project"].as_str())
        .collect();
    assert_eq!(names, ["Example A", "Example B", "Example C"]);
    let a = observed(&dataset, 0, "metered_cost");
    assert_eq!(
        a[40].1, Partial,
        "an unpriced response makes the minute partial"
    );
    assert_eq!(a[41].1, Partial);
    assert_eq!(a[39].1, Complete);
    // Example B has no unpriced column and Example C no cost column at all.
    assert!(
        observed(&dataset, 1, "metered_cost")
            .iter()
            .all(|c| c.1 == Complete)
    );
    assert!(
        observed(&dataset, 2, "metered_cost")
            .iter()
            .all(|c| *c == (Some("0".into()), Complete))
    );
}

#[test]
fn the_profile_maps_five_calendar_days_across_the_fall_back_day() {
    let dataset = profile_dataset("daily", "2026-11-03T20:00:00Z");
    let seconds: Vec<i64> = dataset.intervals().iter().map(|i| i.seconds()).collect();
    assert_eq!(seconds, [86_400, 86_400, 90_000, 86_400, 86_400]);
    assert_eq!(dataset.timezone().name(), "America/New_York");
    let a = observed(&dataset, 0, "metered_cost");
    assert_eq!(
        a,
        cells(&[
            (Some("1.5"), Complete),
            (Some("2.25"), Complete),
            (Some("5.125"), Partial),
            (Some("0"), Complete),
            (Some("1.1"), Partial),
        ])
    );
    let total = a
        .iter()
        .filter_map(|c| c.0.as_deref())
        .map(|v| ChartDecimal::parse(v).unwrap())
        .try_fold(ChartDecimal::zero(), |sum, v| sum.checked_add(&v))
        .unwrap();
    assert_eq!(total.to_plain_string(), "9.975");
    let b = observed(&dataset, 1, "metered_cost");
    assert_eq!(b[4], (Some("0.4".into()), Partial), "today is provisional");
    assert!(b[..4].iter().all(|c| c.1 == Complete));
}

#[test]
fn the_profile_keeps_subscription_activity_apart_from_metered_cost() {
    let dataset = profile_dataset("subscription", "2026-11-03T20:00:00Z");
    let metrics: Vec<&str> = dataset.metrics().iter().map(|m| m.id.as_str()).collect();
    assert_eq!(metrics, ["subscription_responses", "subscription_cost"]);
    let keys: Vec<&str> = dataset
        .series()
        .iter()
        .map(|s| s.dimensions["channel"].as_str())
        .collect();
    assert_eq!(
        keys,
        ["metered", "subscription", "subscription:example-provider"]
    );
    // Metered responses never leak into the subscription count.
    assert!(
        observed(&dataset, 0, "subscription_responses")[..4]
            .iter()
            .all(|c| c.0.as_deref() == Some("0"))
    );
    // A plain subscription has no dollar figure: its cost is zero known, not a price.
    assert_eq!(
        observed(&dataset, 1, "subscription_cost")[0],
        (Some("0".into()), Complete)
    );
    let provider = observed(&dataset, 2, "subscription_cost");
    assert_eq!(provider[1], (Some("0.09".into()), Complete));
    assert_eq!(provider[2], (Some("0.04".into()), Partial));
}

#[test]
fn the_profile_models_source_keeps_the_unknown_model_as_its_own_series() {
    let dataset = profile_dataset("models", "2026-11-03T20:00:00Z");
    // The producer's placeholder is normalized by the profile's own mapping
    // rule into an empty (Unknown) value; the series identity keeps raw text.
    let names: Vec<&str> = dataset
        .series()
        .iter()
        .map(|s| s.dimensions["model"].as_str())
        .collect();
    assert_eq!(names, ["model-one", ""]);
    assert_eq!(dataset.series()[1].id, "[\"(unknown)\"]");
    assert_eq!(
        observed(&dataset, 1, "metered_cost")[2],
        (Some("2.625".into()), Partial)
    );
}

#[test]
fn unknown_value_rule_is_explicit_per_dimension_and_keeps_distinct_identities() {
    let mut config = parse_config(
        r#"
[[chart_sources]]
id = "s"
command = ["x"]
format = "columns-v1"
window = "3h"
timezone = "UTC"
[chart_sources.mapping]
buckets = "/b"
interval_start = "/s"
interval_end = "/e"
groups = "/g"
dimensions = [{ name = "model", pointer = "/k/0", unknown_value = "?" }, { name = "team", pointer = "/k/1" }]
[[chart_sources.mapping.metrics]]
id = "m"
label = "M"
unit = "USD"
quantity = "interval_total"
values = "/v"
"#,
    )
    .unwrap();
    let source = config.chart_sources.remove(0);
    let request = request_for(&source, "2026-11-20T12:00:00Z");
    let raw = json!({
        "b": [{"s": "2026-11-20T09:00:00Z", "e": "2026-11-20T10:00:00Z"}],
        "g": [
            {"k": ["?", "?"], "v": [1]},
            {"k": ["a", "?"], "v": [2]}
        ]
    });
    let dataset = map_columns(&raw, source.mapping.as_ref().unwrap(), &request).unwrap();
    let mut shown: Vec<(String, String)> = dataset
        .series()
        .iter()
        .map(|s| (s.dimensions["model"].clone(), s.dimensions["team"].clone()))
        .collect();
    shown.sort();
    // Only the dimension that opted in is normalized; "?" stays literal elsewhere.
    assert_eq!(
        shown,
        [
            (String::new(), "?".to_string()),
            ("a".to_string(), "?".to_string())
        ]
    );
}

#[test]
fn every_profile_source_maps_the_coverage_statement_to_a_notice() {
    let statement =
        "Synthetic coverage statement: usage may be absent; no answer claims complete coverage.";
    for (id, now) in [
        ("recent", "2026-11-20T12:00:30Z"),
        ("daily", "2026-11-03T20:00:00Z"),
        ("models", "2026-11-03T20:00:00Z"),
        ("subscription", "2026-11-03T20:00:00Z"),
    ] {
        let dataset = profile_dataset(id, now);
        assert_eq!(dataset.notices(), [statement], "{id}");
    }
}

// ---------- synthetic direct-contract commands ----------

fn run_example(script: &str, from: &str, to: &str) -> String {
    let output = Command::new("sh")
        .arg(format!("{ROOT}/examples/charts/{script}"))
        .args(["--from", from, "--to", to, "--timezone", "UTC"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{script}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn the_unrelated_synthetic_commands_drive_the_same_native_contract() {
    let config = r#"
[[chart_sources]]
id = "example-metrics"
command = ["example-chart-data"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"
"#;
    let source = source_of(config, "example-metrics");
    let request = request_for(&source, "2026-11-20T12:00:00Z");
    let cases = [
        ("synthetic-spend.sh", "amount", "USD"),
        ("synthetic-latency.sh", "latency_ms", "ms"),
    ];
    for (script, metric, unit) in cases {
        let text = run_example(script, "2026-11-20T09:00:30Z", "2026-11-20T12:00:00Z");
        assert_eq!(
            text,
            run_example(script, "2026-11-20T09:00:30Z", "2026-11-20T12:00:00Z"),
            "{script} is deterministic"
        );
        let dataset = normalize_output(&source, &text, &request).unwrap();
        assert_eq!(dataset.intervals().len(), 180, "{script}");
        assert_eq!(dataset.series().len(), 2);
        let descriptor = dataset.metric(metric).unwrap();
        assert_eq!(descriptor.unit, unit);
        for banned in ["ledger", "knownCost", "unpriced", "agent-activity"] {
            assert!(!text.contains(banned), "{script} mentions {banned}");
        }
    }
    let long = run_example(
        "synthetic-spend.sh",
        "2026-11-17T12:00:00Z",
        "2026-11-20T12:00:00Z",
    );
    let dataset = normalize_output(&source, &long, &request).unwrap();
    assert_eq!(
        dataset.intervals().len(),
        360,
        "longer windows coarsen the step"
    );
}

#[test]
fn the_synthetic_commands_refuse_missing_arguments() {
    for script in ["synthetic-spend.sh", "synthetic-latency.sh"] {
        let status = Command::new("sh")
            .arg(format!("{ROOT}/examples/charts/{script}"))
            .args(["--from", "2026-11-20T09:00:00Z"])
            .output()
            .unwrap()
            .status;
        assert!(!status.success(), "{script}");
    }
}

// ---------- the renderer-side tree stays vendor-free ----------

#[test]
fn source_tree_carries_no_ledger_vocabulary() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for word in ["knownCostUSD", "unpriced", "agent-activity"] {
                    if text.contains(word) {
                        hits.push(format!("{}: {word}", path.display()));
                    }
                }
            }
        }
    }
    let mut hits = Vec::new();
    walk(&std::path::Path::new(ROOT).join("src"), &mut hits);
    assert!(hits.is_empty(), "{hits:?}");
}

// ---- parent-container guard for omitted = "zero" ----

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
    let objects =
        nested(json!({"keys": ["claude", "subscription"], "values": [{"knownCostUSD": "4"}]}));
    refused(NESTED, &objects, ChartErrorKind::InvalidMapping);
}

#[test]
fn a_missing_null_or_scalar_parent_is_refused_and_a_real_empty_container_is_zero() {
    for parent in [json!(null), json!(7), json!("x")] {
        refused(
            NESTED,
            &nested(json!({"keys": ["a"], "columns": parent})),
            ChartErrorKind::InvalidMapping,
        );
    }
    let dataset = map(NESTED, &nested(json!({"keys": ["a"], "columns": {}}))).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[(Some("0"), Complete)])
    );
    // Empty array parent: only a canonical numeric leaf is an omitted index.
    let indexed = NESTED
        .replace("/columns/knownCostUSD", "/columns/0")
        .replace("/columns/unpriced", "/quality/0");
    let dataset = map(
        &indexed,
        &nested(json!({"keys": ["a"], "columns": [], "quality": []})),
    )
    .unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[(Some("0"), Complete)])
    );
    // A string key against an array is a wrong shape, never an omitted leaf.
    refused(
        NESTED,
        &nested(json!({"keys": ["a"], "columns": []})),
        ChartErrorKind::InvalidMapping,
    );
    for bad in ["-1", "01", "1.5", "+1", ""] {
        let config = NESTED
            .replace("/columns/knownCostUSD", &format!("/columns/{bad}"))
            .replace("quality_counter = \"/columns/unpriced\"", "");
        refused(
            &config,
            &nested(json!({"keys": ["a"], "columns": []})),
            ChartErrorKind::InvalidMapping,
        );
    }
}

#[test]
fn the_guard_applies_to_the_counter_and_keeps_explicit_null_and_root_parents() {
    let split = NESTED.replace("/columns/unpriced", "/quality/unpriced");
    refused(
        &split,
        &nested(json!({"keys": ["a"], "columns": {"knownCostUSD": ["1"]}})),
        ChartErrorKind::InvalidMapping,
    );
    let ok = map(
        &split,
        &nested(json!({"keys": ["a"], "columns": {"knownCostUSD": ["1"]}, "quality": {}})),
    )
    .unwrap();
    assert_eq!(observed(&ok, 0, "cost"), cells(&[(Some("1"), Complete)]));
    let null_leaf = map(
        NESTED,
        &nested(json!({"keys": ["a"], "columns": {"knownCostUSD": null, "unpriced": [0]}})),
    )
    .unwrap();
    assert_eq!(observed(&null_leaf, 0, "cost"), cells(&[(None, Missing)]));
    // omitted = "missing" never needed a parent: unchanged.
    let default = NESTED.replace("omitted = \"zero\"", "");
    let missing = map(&default, &nested(json!({"keys": ["a"]}))).unwrap();
    assert_eq!(observed(&missing, 0, "cost"), cells(&[(None, Missing)]));
    // A pointer whose parent is the group itself is unchanged.
    let root = NESTED
        .replace("/columns/knownCostUSD", "/cost")
        .replace("quality_counter = \"/columns/unpriced\"", "");
    let zero = map(&root, &nested(json!({"keys": ["a"]}))).unwrap();
    assert_eq!(observed(&zero, 0, "cost"), cells(&[(Some("0"), Complete)]));
}

#[test]
fn the_empty_key_and_escaped_tokens_are_resolved_before_the_guard() {
    let empty_key = NESTED
        .replace("/columns/knownCostUSD", "/columns/")
        .replace("/columns/unpriced", "/columns/n");
    let dataset = map(&empty_key, &nested(json!({"keys": ["a"], "columns": {}}))).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[(Some("0"), Complete)])
    );
    // `~1` is a slash inside one token: the parent is the key "a/b".
    let escaped = NESTED
        .replace("/columns/knownCostUSD", "/a~1b/leaf")
        .replace("/columns/unpriced", "/a~1b/count");
    let dataset = map(&escaped, &nested(json!({"keys": ["a"], "a/b": {}}))).unwrap();
    assert_eq!(
        observed(&dataset, 0, "cost"),
        cells(&[(Some("0"), Complete)])
    );
    refused(
        &escaped,
        &nested(json!({"keys": ["a"], "a": {"b": {}}})),
        ChartErrorKind::InvalidMapping,
    );
}
