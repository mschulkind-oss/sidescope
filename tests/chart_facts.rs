//! Public-boundary tests for card facts: exact reductions, the four distinct
//! empty outcomes, capacity, the full pre-fold Details rows, sampled gauges
//! and the equality of the GUI model with the facts it is presented from.

use serde_json::{Value, json};
use sidescope::chart::config::{ChartCardConfig, ChartKind, ResolvedRequest, resolve_request};
use sidescope::chart::dataset::{Quality, parse_native_snapshot};
use sidescope::chart::decimal::ChartDecimal;
use sidescope::chart::error::{ChartError, ChartErrorKind};
use sidescope::chart::facts::{
    Bound, CapacityLimit, CardFacts, DataOrigin, DetailGrain, MismatchSubject, Outcome, Refusal,
    RowRole, Standing, reduce_card,
};
use sidescope::chart::reduce::Reduction;
use sidescope::chart::state::{SourceInfo, SourceSnapshot};
use sidescope::chart::time::{Timestamp, parse_instant};
use sidescope::chart_view::{build_model, present_card};
use sidescope::parse_config;
use std::sync::Arc;

const NOW: &str = "2026-11-20T12:00:00Z";

fn ts(text: &str) -> Timestamp {
    parse_instant(text).unwrap()
}

fn request() -> ResolvedRequest {
    let source = parse_config(
        r#"
[[chart_sources]]
id = "example-metrics"
command = ["example-chart-data"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"
"#,
    )
    .unwrap()
    .chart_sources
    .remove(0);
    resolve_request(&source, ts(NOW), 1).unwrap()
}

fn info() -> SourceInfo {
    SourceInfo {
        executable: "example-chart-data".to_string(),
        last_attempt: Some(ts(NOW)),
        last_success: Some(ts(NOW)),
        next_retry: None,
        latency_ms: Some(1),
        revision: 1,
        retry_suspended: false,
        request_window: None,
        timezone: "UTC".to_string(),
        stale_after_seconds: 90,
    }
}

fn snapshot_of(json: &str) -> SourceSnapshot {
    let dataset = parse_native_snapshot(json, &request()).unwrap();
    SourceSnapshot {
        source_id: "example-metrics".to_string(),
        dataset: Some(Arc::new(dataset)),
        last_error: None,
        stderr_excerpt: None,
        info: info(),
    }
}

fn card(kind: ChartKind) -> ChartCardConfig {
    ChartCardConfig {
        id: "card".to_string(),
        source: "example-metrics".to_string(),
        kind,
        metric: "amount".to_string(),
        title: None,
        summary: None,
        group_by: Vec::new(),
        top_n: None,
        stack_by: None,
        bounded_percent: false,
    }
}

fn grouped(kind: ChartKind, dimensions: &[&str]) -> ChartCardConfig {
    ChartCardConfig {
        group_by: dimensions.iter().map(|name| name.to_string()).collect(),
        ..card(kind)
    }
}

struct Spec {
    quantity: &'static str,
    unit: &'static str,
    unknown_contribution: Option<&'static str>,
}

const SUM: Spec = Spec {
    quantity: "interval_total",
    unit: "USD",
    unknown_contribution: None,
};
const AT_LEAST: Spec = Spec {
    quantity: "interval_total",
    unit: "USD",
    unknown_contribution: Some("nonnegative"),
};
const GAUGE: Spec = Spec {
    quantity: "gauge",
    unit: "ms",
    unknown_contribution: None,
};

/// One series: its dimensions, then values and qualities per interval.
type Row = (
    Vec<(&'static str, String)>,
    Vec<Option<&'static str>>,
    Vec<&'static str>,
);

fn row(
    dimensions: &[(&'static str, &str)],
    values: &[Option<&'static str>],
    quality: &[&'static str],
) -> Row {
    (
        dimensions
            .iter()
            .map(|(k, v)| (*k, v.to_string()))
            .collect(),
        values.to_vec(),
        quality.to_vec(),
    )
}

fn project(name: &str, values: &[&'static str]) -> Row {
    row(
        &[("project", name)],
        &values.iter().map(|v| Some(*v)).collect::<Vec<_>>(),
        &vec!["complete"; values.len()],
    )
}

/// Minute intervals from 11:00; the request window ends at 12:00.
fn payload(spec: &Spec, intervals: usize, rows: &[Row]) -> String {
    let mut metric = json!({
        "label": "Amount", "unit": spec.unit, "quantity": spec.quantity, "estimate": false
    });
    if let Some(value) = spec.unknown_contribution {
        metric["unknown_contribution"] = json!(value);
    }
    let items: Vec<Value> = (0..intervals)
        .map(|i| {
            json!({
                "start": format!("2026-11-20T11:{:02}:00Z", i),
                "end": format!("2026-11-20T11:{:02}:00Z", i + 1),
            })
        })
        .collect();
    let series: Vec<Value> = rows
        .iter()
        .enumerate()
        .map(|(index, (dimensions, values, quality))| {
            let dims: serde_json::Map<String, Value> = dimensions
                .iter()
                .map(|(k, v)| (k.to_string(), json!(v)))
                .collect();
            json!({
                "id": format!("s{index}"),
                "dimensions": dims,
                "metrics": {"amount": {"values": values, "quality": quality}},
            })
        })
        .collect();
    json!({
        "version": "sidescope-chart-v1",
        "generated_at": "2026-11-20T11:59:50Z",
        "range": {"start": "2026-11-20T11:00:00Z", "end": "2026-11-20T12:00:00Z", "timezone": "UTC"},
        "intervals": items,
        "metrics": {"amount": metric},
        "series": series,
        "notices": []
    })
    .to_string()
}

fn facts(config: &ChartCardConfig, json: &str) -> CardFacts {
    reduce_card(config, &snapshot_of(json), ts(NOW), DataOrigin::Live)
}

fn text(decimal: Option<&ChartDecimal>) -> Option<String> {
    decimal.map(ChartDecimal::to_plain_string)
}

fn sampled_fixture() -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/charts/native/sampled-gauge.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

// ---------- the four empties ----------

#[test]
fn an_empty_series_list_is_no_observations() {
    let facts = facts(&card(ChartKind::TimeSeries), &payload(&SUM, 2, &[]));
    assert_eq!(facts.outcome, Outcome::NoObservations);
    assert!(facts.summary.is_none() && facts.rows.is_empty());
}

#[test]
fn series_without_intervals_are_no_intervals_not_no_values() {
    for kind in [
        ChartKind::TimeSeries,
        ChartKind::TimeBars,
        ChartKind::RankedBars,
    ] {
        let json = payload(&SUM, 0, &[project("a", &[])]);
        let facts = facts(&card(kind), &json);
        assert_eq!(facts.outcome, Outcome::NoIntervals, "{kind:?}");
        assert_eq!(facts.intervals.as_ref().unwrap().total, 0);
    }
}

#[test]
fn every_cell_null_is_no_known_values_with_the_missing_count() {
    let json = payload(
        &SUM,
        2,
        &[row(
            &[("project", "a")],
            &[None, None],
            &["missing", "missing"],
        )],
    );
    let facts = facts(&card(ChartKind::TimeSeries), &json);
    assert_eq!(facts.outcome, Outcome::NoKnownValues);
    let intervals = facts.intervals.unwrap();
    assert_eq!(
        (intervals.total, intervals.missing, intervals.complete),
        (2, 2, 0)
    );
    let summary = facts.summary.unwrap();
    assert_eq!((summary.exact, summary.known_subtotal), (None, None));
    assert_eq!(summary.quality, Quality::Missing);
}

#[test]
fn all_zero_data_is_plotted_and_an_exact_zero() {
    let facts = facts(
        &card(ChartKind::TimeSeries),
        &payload(&SUM, 2, &[project("a", &["0", "0"])]),
    );
    assert_eq!(facts.outcome, Outcome::Plotted);
    let summary = facts.summary.unwrap();
    assert_eq!(text(summary.exact.as_ref()).as_deref(), Some("0"));
    assert_eq!(text(summary.known_subtotal.as_ref()).as_deref(), Some("0"));
    assert_eq!(summary.bound, Bound::Exact);
}

// ---------- sums ----------

#[test]
fn a_partial_sum_is_at_least_only_for_a_nonnegative_unknown_contribution() {
    let rows = [row(
        &[("project", "a")],
        &[Some("1.5"), Some("2")],
        &["complete", "partial"],
    )];
    let at_least = facts(&card(ChartKind::TimeSeries), &payload(&AT_LEAST, 2, &rows))
        .summary
        .unwrap();
    assert_eq!(at_least.quality, Quality::Partial);
    assert_eq!(at_least.bound, Bound::AtLeast);
    assert_eq!(text(at_least.exact.as_ref()).as_deref(), Some("3.5"));
    assert_eq!(
        text(at_least.known_subtotal.as_ref()).as_deref(),
        Some("3.5")
    );
    let unknown = facts(&card(ChartKind::TimeSeries), &payload(&SUM, 2, &rows))
        .summary
        .unwrap();
    assert_eq!(unknown.bound, Bound::Incomplete);
    assert_eq!(text(unknown.exact.as_ref()).as_deref(), Some("3.5"));
}

#[test]
fn a_sum_with_a_missing_cell_has_no_exact_but_keeps_its_known_subtotal() {
    let rows = [row(
        &[("project", "a")],
        &[None, Some("4")],
        &["missing", "complete"],
    )];
    let summary = facts(&card(ChartKind::TimeSeries), &payload(&SUM, 2, &rows))
        .summary
        .unwrap();
    assert_eq!(summary.quality, Quality::Missing);
    assert_eq!(summary.exact, None);
    assert_eq!(text(summary.known_subtotal.as_ref()).as_deref(), Some("4"));
    assert_eq!(summary.bound, Bound::Incomplete);
}

#[test]
fn a_merged_missing_cell_keeps_its_known_part_in_details_but_has_no_exact() {
    let rows = [
        project("a", &["4"]),
        row(&[("project", "b")], &[None], &["missing"]),
    ];
    let facts = facts(&card(ChartKind::TimeSeries), &payload(&SUM, 1, &rows));
    let detail = &facts.details.rows[0];
    assert_eq!(detail.quality, Quality::Missing);
    assert_eq!(text(detail.known_subtotal.as_ref()).as_deref(), Some("4"));
    assert_eq!(detail.exact(), None);
    assert_eq!(facts.rows[0].exact(), None);
    assert_eq!(text(facts.rows[0].known_subtotal()).as_deref(), Some("4"));
}

#[test]
fn a_negative_value_marks_the_summary_signed() {
    let signed = facts(
        &card(ChartKind::TimeSeries),
        &payload(&SUM, 2, &[project("a", &["-1", "2"])]),
    );
    assert!(signed.summary.unwrap().signed);
    let plain = facts(
        &card(ChartKind::TimeSeries),
        &payload(&SUM, 2, &[project("a", &["1", "2"])]),
    );
    assert!(!plain.summary.unwrap().signed);
}

#[test]
fn intervals_are_counted_by_their_worst_quality_across_series() {
    let json = payload(
        &SUM,
        3,
        &[
            row(
                &[("project", "a")],
                &[Some("1"), Some("1"), Some("1")],
                &["complete", "partial", "partial"],
            ),
            row(
                &[("project", "b")],
                &[Some("1"), None, Some("1")],
                &["complete", "missing", "complete"],
            ),
        ],
    );
    let intervals = facts(&card(ChartKind::TimeBars), &json).intervals.unwrap();
    assert_eq!(
        (intervals.complete, intervals.partial, intervals.missing),
        (1, 1, 1)
    );
    assert!(!intervals.open_right_edge);
}

#[test]
fn an_interval_ending_after_the_request_is_an_open_right_edge() {
    let json = std::fs::read_to_string(format!(
        "{}/tests/fixtures/charts/native/partial-current.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let facts = facts(&card(ChartKind::TimeBars), &json);
    let intervals = facts.intervals.unwrap();
    assert!(intervals.open_right_edge);
    assert_eq!(
        (intervals.complete, intervals.partial, intervals.total),
        (1, 1, 2)
    );
    assert!(facts.summary.unwrap().includes_open_interval);
}

// ---------- gauges ----------

#[test]
fn a_gauge_takes_its_last_non_null_cell_and_reports_a_null_newest() {
    let rows = [row(
        &[("project", "a")],
        &[Some("41"), None],
        &["complete", "missing"],
    )];
    let summary = facts(&card(ChartKind::TimeSeries), &payload(&GAUGE, 2, &rows))
        .summary
        .unwrap();
    assert_eq!(text(summary.exact.as_ref()).as_deref(), Some("41"));
    assert!(summary.newest_is_null);
    assert_eq!(summary.observed.unwrap().index, 0);
    assert_eq!(summary.known_subtotal, None);
}

#[test]
fn a_gauge_keeps_the_quality_of_its_own_cell_and_earlier_nulls_do_not_invalidate_it() {
    let rows = [row(
        &[("project", "a")],
        &[None, Some("5"), Some("6")],
        &["missing", "complete", "partial"],
    )];
    let summary = facts(&card(ChartKind::TimeSeries), &payload(&GAUGE, 3, &rows))
        .summary
        .unwrap();
    assert_eq!(text(summary.exact.as_ref()).as_deref(), Some("6"));
    assert_eq!(summary.quality, Quality::Partial);
    assert!(!summary.newest_is_null);
    assert_eq!(summary.observed.unwrap().index, 2);
}

#[test]
fn a_sampled_gauge_is_positioned_at_its_start_instant_with_breaks_and_no_right_edge() {
    let snapshot = snapshot_of(&sampled_fixture());
    let config = ChartCardConfig {
        metric: "observed_percent".to_string(),
        ..card(ChartKind::TimeSeries)
    };
    let facts = reduce_card(
        &config,
        &snapshot,
        ts("2026-11-20T12:30:00Z"),
        DataOrigin::Live,
    );
    assert_eq!(facts.outcome, Outcome::Plotted);
    let summary = facts.summary.as_ref().unwrap();
    assert_eq!(text(summary.exact.as_ref()).as_deref(), Some("13"));
    assert_eq!(
        summary.observed.unwrap().interval.start,
        ts("2026-11-20T11:15:00Z")
    );
    assert!(!facts.intervals.as_ref().unwrap().open_right_edge);
    assert_eq!(facts.rows[0].breaks, vec![1]);
    assert!(facts.details.rows[0].sampled && !facts.details.rows[0].break_before);
    assert!(facts.details.rows[1].break_before);
}

#[test]
fn sampled_gauges_refuse_time_bars_but_not_ranked_bars() {
    let snapshot = snapshot_of(&sampled_fixture());
    let bars = ChartCardConfig {
        metric: "observed_percent".to_string(),
        ..card(ChartKind::TimeBars)
    };
    let bars = reduce_card(&bars, &snapshot, ts(NOW), DataOrigin::Live);
    let Outcome::Refused(Refusal::CardMismatch(mismatch)) = bars.outcome else {
        panic!("{:?}", bars.outcome);
    };
    assert_eq!(mismatch.subject, MismatchSubject::Sampled);
    let ranked = ChartCardConfig {
        metric: "observed_percent".to_string(),
        ..card(ChartKind::RankedBars)
    };
    let ranked = reduce_card(&ranked, &snapshot, ts(NOW), DataOrigin::Live);
    assert_eq!(ranked.outcome, Outcome::Plotted);
    let detail = &ranked.details.rows[0];
    assert!(detail.sampled);
    assert_eq!(detail.interval.start, ts("2026-11-20T11:15:00Z"));
}

// ---------- mismatches ----------

#[test]
fn bounded_percent_needs_a_percent_unit() {
    let config = ChartCardConfig {
        bounded_percent: true,
        ..card(ChartKind::TimeSeries)
    };
    let spec = Spec {
        unit: "percentage points",
        ..GAUGE
    };
    let facts = facts(&config, &payload(&spec, 1, &[project("a", &["5"])]));
    let Outcome::Refused(Refusal::CardMismatch(mismatch)) = facts.outcome else {
        panic!("{:?}", facts.outcome);
    };
    assert_eq!(mismatch.subject, MismatchSubject::BoundedPercent);
}

#[test]
fn a_missing_metric_lists_what_the_source_offers() {
    let config = ChartCardConfig {
        metric: "nope".to_string(),
        ..card(ChartKind::TimeSeries)
    };
    let facts = facts(&config, &payload(&SUM, 1, &[project("a", &["1"])]));
    let Outcome::Refused(Refusal::CardMismatch(mismatch)) = facts.outcome else {
        panic!("{:?}", facts.outcome);
    };
    assert_eq!(mismatch.subject, MismatchSubject::Metric);
    assert_eq!(mismatch.wanted.as_deref(), Some("nope"));
    assert_eq!(mismatch.available, vec!["amount".to_string()]);
}

#[test]
fn a_missing_dimension_and_a_gauge_merge_are_typed_mismatches() {
    let facts_dim = facts(
        &grouped(ChartKind::TimeSeries, &["team"]),
        &payload(&SUM, 1, &[project("a", &["1"])]),
    );
    let Outcome::Refused(Refusal::CardMismatch(mismatch)) = facts_dim.outcome else {
        panic!("dimension");
    };
    assert_eq!(mismatch.subject, MismatchSubject::Dimension);
    assert_eq!(mismatch.available, vec!["project".to_string()]);
    let merged = facts(
        &card(ChartKind::TimeSeries),
        &payload(&GAUGE, 1, &[project("a", &["1"]), project("b", &["2"])]),
    );
    let Outcome::Refused(Refusal::CardMismatch(mismatch)) = merged.outcome else {
        panic!("merge");
    };
    assert_eq!(mismatch.subject, MismatchSubject::GaugeMerge);
}

// ---------- capacity ----------

fn numbered(count: usize, values: &[&'static str]) -> Vec<Row> {
    (0..count)
        .map(|i| project(&format!("p{i:02}"), values))
        .collect()
}

#[test]
fn seventeen_empty_valued_groups_refuse_with_the_details_kept() {
    let rows: Vec<Row> = (0..17)
        .map(|i| {
            row(
                &[("team", ""), ("project", &format!("p{i:02}"))],
                &[Some("1")],
                &["complete"],
            )
        })
        .collect();
    let facts = facts(
        &grouped(ChartKind::TimeSeries, &["team", "project"]),
        &payload(&SUM, 1, &rows),
    );
    let Outcome::Refused(Refusal::Capacity(capacity)) = &facts.outcome else {
        panic!("{:?}", facts.outcome);
    };
    assert_eq!(capacity.limit, CapacityLimit::UnknownSeries);
    assert_eq!(capacity.count, Some(17));
    assert_eq!(facts.details.rows.len(), 17);
    assert!(facts.details.rows.iter().all(|row| row.shown_as.is_none()));
}

#[test]
fn seventeen_gauge_series_are_a_typed_capacity_refusal() {
    let facts = facts(
        &grouped(ChartKind::TimeSeries, &["project"]),
        &payload(&GAUGE, 1, &numbered(17, &["1"])),
    );
    let Outcome::Refused(Refusal::Capacity(capacity)) = facts.outcome else {
        panic!("{:?}", facts.outcome);
    };
    assert_eq!(capacity.limit, CapacityLimit::GaugeSeries);
    assert_eq!((capacity.count, capacity.maximum), (Some(17), Some(16)));
}

#[test]
fn a_sum_past_the_numeric_range_is_a_typed_capacity_refusal() {
    let big = "999999999999999999999999";
    let facts = facts(
        &card(ChartKind::TimeSeries),
        &payload(&SUM, 2, &[project("a", &[big, big])]),
    );
    let Outcome::Refused(Refusal::Capacity(capacity)) = facts.outcome else {
        panic!("{:?}", facts.outcome);
    };
    assert_eq!(capacity.limit, CapacityLimit::NumericRange);
}

#[test]
fn ranked_rows_fit_sixteen_with_other_and_a_separate_unknown() {
    let mut rows = numbered(16, &["1"]);
    rows.push(project("", &["1"]));
    let config = ChartCardConfig {
        top_n: Some(16),
        ..grouped(ChartKind::RankedBars, &["project"])
    };
    let facts = facts(&config, &payload(&SUM, 1, &rows));
    assert_eq!(facts.outcome, Outcome::Plotted);
    assert!(facts.rows.len() <= 16);
    let groups = facts.groups.as_ref().unwrap();
    assert_eq!(groups.total, 17);
    // Sixteen slots, one for Other and one for the single Unknown group.
    assert_eq!(groups.capacity, Some(14));
    assert_eq!(groups.named_shown, 14);
    assert_eq!(groups.other_members, 2);
    assert_eq!(groups.unknown_shown, 1);
    assert_eq!(
        facts
            .rows
            .iter()
            .filter(|row| row.role == RowRole::Other)
            .count(),
        1
    );
    let unknown = facts
        .rows
        .iter()
        .find(|row| row.role == RowRole::Unknown)
        .unwrap();
    assert_eq!(
        unknown.group,
        Some(vec![("project".to_string(), String::new())])
    );
}

// ---------- Details ----------

#[test]
fn ranked_details_hold_every_group_before_folding_and_the_arithmetic_adds_up() {
    let rows = [
        project("a", &["5"]),
        project("b", &["3"]),
        project("c", &["2"]),
    ];
    let config = ChartCardConfig {
        top_n: Some(1),
        ..grouped(ChartKind::RankedBars, &["project"])
    };
    let facts = facts(&config, &payload(&SUM, 1, &rows));
    assert_eq!(facts.details.grain, Some(DetailGrain::Group));
    assert_eq!(facts.details.rows.len(), 3);
    assert_eq!(facts.rows.len(), 2);
    let shown: Vec<_> = facts.details.rows.iter().map(|row| row.shown_as).collect();
    assert_eq!(
        shown,
        [
            Some(RowRole::Named),
            Some(RowRole::Other),
            Some(RowRole::Other)
        ]
    );
    let known: i64 = facts
        .details
        .rows
        .iter()
        .map(|row| {
            row.known_subtotal
                .as_ref()
                .unwrap()
                .to_plain_string()
                .parse::<i64>()
                .unwrap()
        })
        .sum();
    assert_eq!(known, 10);
    assert_eq!(
        text(facts.summary.as_ref().unwrap().known_subtotal.as_ref()).as_deref(),
        Some("10")
    );
    let other = facts
        .rows
        .iter()
        .find(|row| row.role == RowRole::Other)
        .unwrap();
    assert_eq!(text(other.known_subtotal()).as_deref(), Some("5"));
    assert_eq!(other.members, 2);
    assert_eq!(other.group, None);
}

#[test]
fn time_details_are_interval_major_and_other_is_not_counted_with_its_members() {
    let rows = numbered(18, &["1", "2"]);
    let facts = facts(
        &grouped(ChartKind::TimeSeries, &["project"]),
        &payload(&SUM, 2, &rows),
    );
    assert_eq!(facts.outcome, Outcome::Plotted);
    assert_eq!(facts.details.grain, Some(DetailGrain::IntervalGroup));
    assert_eq!(facts.details.rows.len(), 36);
    assert_eq!(
        facts.details.rows[0].interval.start,
        ts("2026-11-20T11:00:00Z")
    );
    assert_eq!(
        facts.details.rows[17].interval.start,
        ts("2026-11-20T11:00:00Z")
    );
    assert_eq!(
        facts.details.rows[18].interval.start,
        ts("2026-11-20T11:01:00Z")
    );
    let sum = |rows: &mut dyn Iterator<Item = Option<&ChartDecimal>>| -> i64 {
        rows.map(|value| value.unwrap().to_plain_string().parse::<i64>().unwrap())
            .sum()
    };
    let details = sum(&mut facts
        .details
        .rows
        .iter()
        .map(|row| row.known_subtotal.as_ref()));
    assert_eq!(details, 54);
    assert_eq!(
        text(facts.summary.as_ref().unwrap().known_subtotal.as_ref()).as_deref(),
        Some("54")
    );
    // The plotted rows (Other included) add up to the same total.
    let plotted = sum(&mut facts.rows.iter().map(GroupRowValue::value));
    assert_eq!(plotted, 54);
    let other = facts
        .rows
        .iter()
        .find(|row| row.role == RowRole::Other)
        .unwrap();
    assert_eq!(other.members, 3);
    assert_eq!(text(other.known_subtotal()).as_deref(), Some("9"));
    let folded = facts
        .details
        .rows
        .iter()
        .filter(|row| row.shown_as == Some(RowRole::Other))
        .count();
    assert_eq!(folded, 6);
}

trait GroupRowValue {
    fn value(&self) -> Option<&ChartDecimal>;
}

impl GroupRowValue for sidescope::chart::facts::GroupRow {
    fn value(&self) -> Option<&ChartDecimal> {
        self.known_subtotal()
    }
}

#[test]
fn an_unknown_group_is_kept_apart_and_never_folded_into_other() {
    let mut rows = numbered(17, &["1"]);
    rows.push(project("", &["7"]));
    let facts = facts(
        &grouped(ChartKind::TimeSeries, &["project"]),
        &payload(&SUM, 1, &rows),
    );
    let unknown: Vec<_> = facts
        .details
        .rows
        .iter()
        .filter(|row| row.shown_as == Some(RowRole::Unknown))
        .collect();
    assert_eq!(unknown.len(), 1);
    assert_eq!(
        unknown[0].group,
        vec![("project".to_string(), String::new())]
    );
    let unknown_row = facts
        .rows
        .iter()
        .find(|row| row.role == RowRole::Unknown)
        .unwrap();
    assert_eq!(text(unknown_row.known_subtotal()).as_deref(), Some("7"));
}

#[test]
fn ties_rank_by_the_stable_group_key() {
    let rows = [
        project("b", &["2"]),
        project("a", &["2"]),
        project("c", &["3"]),
    ];
    let facts = facts(
        &grouped(ChartKind::RankedBars, &["project"]),
        &payload(&SUM, 1, &rows),
    );
    let order: Vec<_> = facts
        .details
        .rows
        .iter()
        .map(|row| row.group[0].1.clone())
        .collect();
    assert_eq!(order, ["c", "a", "b"]);
}

// ---------- origin, availability and the GUI model ----------

#[test]
fn replayed_data_has_its_own_standing_and_banner() {
    let snapshot = snapshot_of(&payload(&SUM, 1, &[project("a", &["1"])]));
    let facts = reduce_card(
        &card(ChartKind::TimeSeries),
        &snapshot,
        ts(NOW),
        DataOrigin::Replay,
    );
    assert_eq!(facts.source.standing, Standing::Replayed);
    let model = present_card(&facts);
    assert_eq!(model.banner(), Some("Replayed data · not live"));
    assert!(
        model
            .info_lines()
            .contains(&"Origin: replayed from saved output; no command ran".to_string())
    );
    let live = reduce_card(
        &card(ChartKind::TimeSeries),
        &snapshot,
        ts(NOW),
        DataOrigin::Live,
    );
    assert!(matches!(live.source.standing, Standing::Live(_)));
    assert_eq!(present_card(&live).banner(), None);
}

#[test]
fn a_source_without_a_dataset_is_unavailable_or_still_loading() {
    let mut snapshot = snapshot_of(&payload(&SUM, 1, &[project("a", &["1"])]));
    snapshot.dataset = None;
    let loading = reduce_card(
        &card(ChartKind::TimeSeries),
        &snapshot,
        ts(NOW),
        DataOrigin::Replay,
    );
    assert_eq!(loading.outcome, Outcome::Unavailable { source_error: None });
    assert!(matches!(loading.source.standing, Standing::Live(_)));
    snapshot.last_error = Some(ChartError::new(ChartErrorKind::Timeout, "slow"));
    let failed = reduce_card(
        &card(ChartKind::TimeSeries),
        &snapshot,
        ts(NOW),
        DataOrigin::Live,
    );
    assert_eq!(
        failed.outcome,
        Outcome::Unavailable {
            source_error: Some(ChartErrorKind::Timeout)
        }
    );
}

#[test]
fn the_gui_model_is_exactly_present_card_of_the_facts_for_every_fixture_and_kind() {
    let directory = format!(
        "{}/tests/fixtures/charts/native",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut seen = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let json = std::fs::read_to_string(&path).unwrap();
        let Ok(dataset) = parse_native_snapshot(&json, &request()) else {
            continue;
        };
        let metric = dataset.metrics()[0].id.clone();
        let snapshot = SourceSnapshot {
            source_id: "example-metrics".to_string(),
            dataset: Some(Arc::new(dataset)),
            last_error: None,
            stderr_excerpt: None,
            info: info(),
        };
        for kind in [
            ChartKind::TimeSeries,
            ChartKind::TimeBars,
            ChartKind::RankedBars,
        ] {
            for summary in [None, Some(Reduction::Sum), Some(Reduction::Last)] {
                let config = ChartCardConfig {
                    metric: metric.clone(),
                    summary,
                    ..card(kind)
                };
                let facts = reduce_card(&config, &snapshot, ts(NOW), DataOrigin::Live);
                assert_eq!(
                    present_card(&facts),
                    build_model(&config, &snapshot, ts(NOW)),
                    "{path:?} {kind:?}"
                );
                seen += 1;
            }
        }
    }
    assert!(seen >= 8 * 9, "fixtures seen: {seen}");
}

#[test]
fn the_reported_legend_counts_are_what_the_gui_legend_shows() {
    let cases = [
        (
            grouped(ChartKind::TimeSeries, &[]),
            payload(&SUM, 1, &[project("a", &["1"])]),
        ),
        (
            grouped(ChartKind::TimeSeries, &["project"]),
            payload(&SUM, 1, &[project("a", &["1"])]),
        ),
        (
            grouped(ChartKind::TimeSeries, &["project"]),
            payload(&SUM, 1, &numbered(8, &["1"])),
        ),
        (
            grouped(ChartKind::RankedBars, &["project"]),
            payload(&SUM, 1, &numbered(8, &["1"])),
        ),
    ];
    for (config, json) in cases {
        let facts = facts(&config, &json);
        let model = present_card(&facts);
        let groups = facts.groups.unwrap();
        assert_eq!(groups.legend_shown, model.legend().len());
        assert_eq!(groups.legend_overflow, model.legend_overflow());
    }
    // A single unlabeled group has no legend; eight named groups overflow by three.
    let single = facts(
        &grouped(ChartKind::TimeSeries, &[]),
        &payload(&SUM, 1, &[project("a", &["1"])]),
    );
    assert_eq!(single.groups.unwrap().legend_shown, 0);
    let many = facts(
        &grouped(ChartKind::TimeSeries, &["project"]),
        &payload(&SUM, 1, &numbered(8, &["1"])),
    );
    let many = many.groups.unwrap();
    assert_eq!((many.legend_shown, many.legend_overflow), (5, 3));
}
