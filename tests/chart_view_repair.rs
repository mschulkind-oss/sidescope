//! Regression tests for the chart-view review findings. Each builds a small
//! synthetic native dataset and asserts on the pure card model.

use serde_json::{Value, json};
use sidescope::chart::config::{ChartCardConfig, ChartKind, resolve_request};
use sidescope::chart::dataset::parse_native_snapshot;
use sidescope::chart::state::{SourceInfo, SourceSnapshot};
use sidescope::chart::time::{Timestamp, parse_instant};
use sidescope::chart_view::{ChartCardModel, PlotModel, RowKind, build_model};
use sidescope::parse_config;
use std::sync::Arc;

const NOW: &str = "2026-11-20T12:00:00Z";

fn ts(text: &str) -> Timestamp {
    parse_instant(text).unwrap()
}

struct Spec {
    unit: &'static str,
    quantity: &'static str,
    unknown_contribution: Option<&'static str>,
}

const USD: Spec = Spec {
    unit: "USD",
    quantity: "interval_total",
    unknown_contribution: None,
};

/// Dimension pairs and one value per interval (None = missing).
type Row<'a> = (Vec<(&'a str, &'a str)>, Vec<Option<&'a str>>);

/// `series`: dimension pairs and one value per interval (None = missing).
fn json_of(spec: &Spec, intervals: &[(&str, &str)], series: &[Row]) -> String {
    let mut metric = json!({
        "label": "Amount", "unit": spec.unit, "quantity": spec.quantity, "estimate": false
    });
    if let Some(value) = spec.unknown_contribution {
        metric["unknown_contribution"] = json!(value);
    }
    let items: Vec<Value> = series
        .iter()
        .enumerate()
        .map(|(index, (dims, values))| {
            let dimensions: serde_json::Map<String, Value> = dims
                .iter()
                .map(|(name, value)| (name.to_string(), json!(value)))
                .collect();
            let quality: Vec<&str> = values
                .iter()
                .map(|v| if v.is_some() { "complete" } else { "missing" })
                .collect();
            json!({
                "id": format!("s{index}"),
                "dimensions": dimensions,
                "metrics": {"amount": {"values": values, "quality": quality}}
            })
        })
        .collect();
    json!({
        "version": "sidescope-chart-v1",
        "generated_at": "2026-11-20T11:59:50Z",
        "observed_through": null,
        "range": {"start": intervals[0].0, "end": intervals[intervals.len() - 1].1, "timezone": "UTC"},
        "intervals": intervals.iter().map(|(s, e)| json!({"start": s, "end": e})).collect::<Vec<_>>(),
        "metrics": {"amount": metric},
        "series": items,
        "notices": []
    })
    .to_string()
}

fn snapshot(json: &str) -> SourceSnapshot {
    let source = parse_config(
        r#"
[[chart_sources]]
id = "example-metrics"
command = ["example-chart-data", "--from", "{from}", "--to", "{to}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"
"#,
    )
    .unwrap()
    .chart_sources
    .remove(0);
    let request = resolve_request(&source, ts(NOW), 1).unwrap();
    let dataset = parse_native_snapshot(json, &request).unwrap();
    SourceSnapshot {
        source_id: "example-metrics".to_string(),
        dataset: Some(Arc::new(dataset)),
        last_error: None,
        stderr_excerpt: None,
        info: SourceInfo {
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
        },
    }
}

fn card(kind: ChartKind, metric: &str, group_by: &[&str]) -> ChartCardConfig {
    ChartCardConfig {
        id: "card".to_string(),
        source: "example-metrics".to_string(),
        kind,
        metric: metric.to_string(),
        title: None,
        summary: None,
        group_by: group_by.iter().map(|s| s.to_string()).collect(),
        top_n: None,
        stack_by: None,
        bounded_percent: false,
    }
}

fn model(card: &ChartCardConfig, json: &str) -> ChartCardModel {
    build_model(card, &snapshot(json), ts(NOW))
}

const TWO: [(&str, &str); 2] = [
    ("2026-11-20T09:00:00Z", "2026-11-20T09:01:00Z"),
    ("2026-11-20T09:01:00Z", "2026-11-20T09:02:00Z"),
];

#[test]
fn unknown_groups_beyond_plot_capacity_refuse_visibly_and_keep_every_detail_row() {
    for count in [17, 40] {
        let names: Vec<String> = (0..count).map(|i| format!("model-{i:02}")).collect();
        let series: Vec<_> = names
            .iter()
            .map(|name| {
                (
                    vec![("project", ""), ("model", name.as_str())],
                    vec![Some("1"), Some("2")],
                )
            })
            .collect();
        let card = card(ChartKind::TimeSeries, "amount", &["project", "model"]);
        let model = model(&card, &json_of(&USD, &TWO, &series));
        let message = model.message().unwrap_or_default();
        assert!(
            message.contains("Unknown") && message.contains(&count.to_string()),
            "{count}: {message:?}"
        );
        assert!(matches!(model.plot(), PlotModel::Empty));
        assert_eq!(model.table_rows().len(), count * 2, "details are retained");
    }
}

#[test]
fn a_few_unknown_groups_still_plot_beside_named_ones() {
    let mut series: Vec<Row> = Vec::new();
    let names: Vec<String> = (0..20).map(|i| format!("m{i:02}")).collect();
    for name in &names {
        series.push((vec![("model", name.as_str())], vec![Some("1"), Some("2")]));
    }
    series.push((vec![("model", "")], vec![Some("1"), Some("1")]));
    let card = card(ChartKind::TimeSeries, "amount", &["model"]);
    let model = model(&card, &json_of(&USD, &TWO, &series));
    let PlotModel::Lines { series, .. } = model.plot() else {
        panic!("{:?}", model.message());
    };
    assert!(series.len() <= 16);
}

#[test]
fn native_bounded_percent_requires_a_percent_metric_without_rejecting_the_source() {
    let json = json_of(
        &USD,
        &TWO,
        &[(vec![("p", "a")], vec![Some("150"), Some("2")])],
    );
    let mut bounded = card(ChartKind::TimeSeries, "amount", &[]);
    bounded.bounded_percent = true;
    let model = model(&bounded, &json);
    let message = model.message().unwrap_or_default();
    assert!(message.starts_with("Card mismatch"), "{message:?}");
    assert!(matches!(model.plot(), PlotModel::Empty));
    // The same source still serves an unbounded card.
    let plain = card(ChartKind::TimeSeries, "amount", &[]);
    assert!(matches!(
        self::model(&plain, &json).plot(),
        PlotModel::Lines { .. }
    ));
    // A percent metric is accepted.
    let percent = Spec {
        unit: "percent",
        quantity: "gauge",
        unknown_contribution: None,
    };
    let json = json_of(
        &percent,
        &TWO,
        &[(vec![("p", "a")], vec![Some("50"), Some("60")])],
    );
    assert!(matches!(
        self::model(&bounded, &json).plot(),
        PlotModel::Lines { .. }
    ));
}

const GAUGE: Spec = Spec {
    unit: "ms",
    quantity: "gauge",
    unknown_contribution: None,
};

#[test]
fn ranked_gauges_name_the_last_known_observation_and_its_interval() {
    let json = json_of(
        &GAUGE,
        &TWO,
        &[
            (vec![("service", "api")], vec![Some("41"), Some("44")]),
            (vec![("service", "db")], vec![Some("12"), None]),
        ],
    );
    let model = model(&card(ChartKind::RankedBars, "amount", &["service"]), &json);
    let db = model
        .table_rows()
        .iter()
        .find(|row| row[1].contains("db"))
        .expect("db row");
    assert!(
        db[0].contains("09:00:00") && db[0].contains("09:01:00"),
        "{db:?}"
    );
    assert!(
        !db[0].contains("09:02:00"),
        "must not claim the whole range: {db:?}"
    );
    assert!(db[3].contains("last known"), "{db:?}");
    let api = model
        .table_rows()
        .iter()
        .find(|row| row[1].contains("api"))
        .unwrap();
    assert!(api[0].contains("09:02:00"), "{api:?}");
    assert!(!api[3].contains("last known"), "{api:?}");
    let PlotModel::Ranked { rows, .. } = model.plot() else {
        panic!()
    };
    assert!(rows.iter().any(|row| row.value_text.contains("last known")));
    let readouts: Vec<&str> = (0..model.selection_count())
        .filter_map(|index| model.selection_readout(index))
        .collect();
    assert!(
        readouts
            .iter()
            .any(|r| r.contains("db") && r.contains("last known")),
        "{readouts:?}"
    );
}

#[test]
fn a_producer_unknown_below_the_top_n_cut_stays_its_own_row_not_other() {
    let names = ["a", "b", "c", "d", "e", "f", "g"];
    let mut series: Vec<Row> = names
        .iter()
        .map(|name| (vec![("model", *name)], vec![Some("10"), Some("10")]))
        .collect();
    series.push((vec![("model", "")], vec![Some("1"), Some("1")]));
    let model = model(
        &card(ChartKind::RankedBars, "amount", &["model"]),
        &json_of(&USD, &TWO, &series),
    );
    let PlotModel::Ranked { rows, .. } = model.plot() else {
        panic!()
    };
    let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == RowKind::Unknown).count(), 1);
    assert_eq!(kinds.iter().filter(|k| **k == RowKind::Other).count(), 1);
    let other = rows.iter().find(|row| row.kind == RowKind::Other).unwrap();
    assert_eq!(other.value, Some(40.0), "Other holds only named remainder");
}

#[test]
fn time_bars_reject_a_group_by_they_would_otherwise_silently_ignore() {
    let json = json_of(
        &USD,
        &TWO,
        &[
            (vec![("project", "a")], vec![Some("1"), Some("1")]),
            (vec![("project", "b")], vec![Some("2"), Some("2")]),
        ],
    );
    let grouped = card(ChartKind::TimeBars, "amount", &["project"]);
    let model = model(&grouped, &json);
    let message = model.message().unwrap_or_default();
    assert!(
        message.contains("stack_by") && message.contains("group_by"),
        "{message:?}"
    );
    assert!(matches!(model.plot(), PlotModel::Empty));
    // The working profile shape (stack_by only) is unchanged.
    let mut stacked = card(ChartKind::TimeBars, "amount", &[]);
    stacked.stack_by = Some("project".to_string());
    assert!(matches!(
        self::model(&stacked, &json).plot(),
        PlotModel::Bars { .. }
    ));
    // Naming the same dimension in both keys is not a conflict.
    let mut same = card(ChartKind::TimeBars, "amount", &["project"]);
    same.stack_by = Some("project".to_string());
    assert!(matches!(
        self::model(&same, &json).plot(),
        PlotModel::Bars { .. }
    ));
}

#[test]
fn a_tiny_negative_headline_keeps_its_sign() {
    let negative = json_of(
        &USD,
        &TWO,
        &[(vec![("p", "a")], vec![Some("-0.00001"), None])],
    );
    let positive = json_of(
        &USD,
        &TWO,
        &[(vec![("p", "a")], vec![Some("0.00001"), None])],
    );
    let card = card(ChartKind::TimeSeries, "amount", &[]);
    let neg = model(&card, &negative).headline().unwrap().value.clone();
    let pos = model(&card, &positive).headline().unwrap().value.clone();
    assert_ne!(neg, pos);
    assert!(neg.contains('-'), "{neg}");
    assert!(!pos.contains('-'), "{pos}");
}

#[test]
fn exact_details_keep_nonzero_fractional_seconds() {
    let fractional = [("2026-11-20T09:00:00.100Z", "2026-11-20T09:00:00.900Z")];
    let model = model(
        &card(ChartKind::TimeSeries, "amount", &[]),
        &json_of(&USD, &fractional, &[(vec![("p", "a")], vec![Some("1")])]),
    );
    let row = &model.table_rows()[0][0];
    assert!(
        row.contains("09:00:00.1") && row.contains("09:00:00.9"),
        "{row}"
    );
    // Whole-second intervals stay uncluttered.
    let whole = self::model(
        &card(ChartKind::TimeSeries, "amount", &[]),
        &json_of(
            &USD,
            &TWO,
            &[(vec![("p", "a")], vec![Some("1"), Some("1")])],
        ),
    );
    assert!(!whole.table_rows()[0][0].contains('.'));
}

#[test]
fn signed_input_cells_suppress_ranked_shares_even_when_net_totals_are_positive() {
    let json = json_of(
        &USD,
        &TWO,
        &[
            (vec![("p", "a")], vec![Some("-1"), Some("2")]),
            (vec![("p", "b")], vec![Some("1"), Some("1")]),
        ],
    );
    let model = model(&card(ChartKind::RankedBars, "amount", &["p"]), &json);
    let PlotModel::Ranked { rows, .. } = model.plot() else {
        panic!()
    };
    assert!(rows.iter().all(|row| row.share.is_none()), "{rows:?}");
    assert!(
        !model.footnote().unwrap_or_default().contains("shares use"),
        "no share note without shares"
    );
}

fn ranked_rows(model: &ChartCardModel) -> usize {
    match model.plot() {
        PlotModel::Ranked { rows, .. } => rows.len(),
        _ => 0,
    }
}

#[test]
fn seventeen_unknown_tuples_refuse_a_ranked_plot_and_keep_every_detail_row() {
    let models: Vec<String> = (0..17).map(|i| format!("model-{i:02}")).collect();
    let series: Vec<Row> = models
        .iter()
        .map(|name| {
            (
                vec![("project", ""), ("model", name.as_str())],
                vec![Some("1"), Some("2")],
            )
        })
        .collect();
    let card = card(ChartKind::RankedBars, "amount", &["project", "model"]);
    let model = model(&card, &json_of(&USD, &TWO, &series));
    let message = model.message().unwrap_or_default();
    assert!(
        message.contains("Unknown") && message.contains("17"),
        "{message:?}"
    );
    assert!(matches!(model.plot(), PlotModel::Empty));
    assert_eq!(
        model.table_rows().len(),
        17,
        "full ranking stays in Details"
    );
}

#[test]
fn ranked_plots_never_show_more_than_sixteen_rows_and_keep_the_full_ranking() {
    let names: Vec<String> = (0..20).map(|i| format!("m{i:02}")).collect();
    let named: Vec<Row> = names
        .iter()
        .map(|name| (vec![("model", name.as_str())], vec![Some("1"), Some("2")]))
        .collect();
    // (unknown groups, named groups, top_n)
    for (unknown, named_count, top_n) in [
        (0, 17, 16),
        (0, 20, 16),
        (10, 20, 16),
        (15, 5, 16),
        (15, 20, 5),
        (3, 14, 16),
    ] {
        let unknown_names: Vec<String> = (0..unknown).map(|i| format!("u{i:02}")).collect();
        let mut series: Vec<Row> = named[..named_count].to_vec();
        for name in &unknown_names {
            series.push((
                vec![("model", ""), ("team", name.as_str())],
                vec![Some("1"), Some("1")],
            ));
        }
        let with_team: Vec<Row> = series
            .into_iter()
            .map(|(mut dims, values)| {
                if dims.len() == 1 {
                    dims.push(("team", "x"));
                }
                (dims, values)
            })
            .collect();
        let mut card = card(ChartKind::RankedBars, "amount", &["model", "team"]);
        card.top_n = Some(top_n);
        let model = model(&card, &json_of(&USD, &TWO, &with_team));
        let shown = ranked_rows(&model);
        assert!(
            shown <= 16,
            "{unknown} unknown + {named_count} named, top {top_n}: {shown} rows"
        );
        assert!(
            shown > 0 || model.message().is_some(),
            "{unknown}/{named_count}: neither rows nor a refusal"
        );
        assert_eq!(
            model.table_rows().len(),
            unknown + named_count,
            "Details keeps every group"
        );
    }
}
