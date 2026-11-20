//! Public-API tests for the shared chart contracts: decimals, time, config,
//! datasets, reductions, geometry, state and errors.

use serde_json::{Value, json};
use sidescope::chart::config::{
    ChartSourceConfig, ResolvedRequest, SourceFormat, resolve_request, validate_chart_config,
};
use sidescope::chart::dataset::{ChartDataset, Quality, Quantity, parse_native_snapshot};
use sidescope::chart::decimal::{ChartDecimal, DecimalError};
use sidescope::chart::error::{ChartError, ChartErrorKind, sanitize_text};
use sidescope::chart::geometry::{ScaleKind, TimeAxis, value_scale};
use sidescope::chart::reduce::{Reduction, collapse_series, last_known, sum_cells};
use sidescope::chart::state::{SourceInfo, SourceSnapshot, SourceStatus};
use sidescope::chart::time::{
    ChartZone, Interval, Timestamp, WindowSpec, format_utc, parse_instant, resolve_window,
};
use sidescope::parse_config;
use std::sync::Arc;

fn ts(text: &str) -> Timestamp {
    parse_instant(text).unwrap()
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/charts/native/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

const NATIVE_CONFIG: &str = r#"
[[chart_sources]]
id = "example-metrics"
command = ["example-chart-data", "--from", "{from}", "--to", "{to}", "--timezone", "{timezone}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"
refresh_seconds = 30

[[chart_cards]]
id = "recent-amount"
source = "example-metrics"
kind = "time-series"
metric = "amount"
summary = "sum"
"#;

const COLUMNS_CONFIG: &str = r#"
[[chart_sources]]
id = "ledger"
command = ["example-ledger"]
format = "columns-v1"
calendar_days = 5
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
quality_counter = "/unpriced"
values = "/cost"

[[chart_sources.mapping.metrics]]
id = "load"
label = "Load"
unit = "percent"
quantity = "gauge"
values = "/load"

[[chart_cards]]
id = "cost-card"
source = "ledger"
kind = "ranked-bars"
metric = "cost"
group_by = ["project"]
"#;

fn replaced(base: &str, old: &str, new: &str) -> String {
    assert!(base.contains(old), "test pattern {old:?} not in base");
    base.replacen(old, new, 1)
}

fn refused(config: &str, expect: &str) {
    let error = parse_config(config).unwrap_err();
    assert!(error.contains(expect), "{error:?} should name {expect:?}");
}

fn native_source() -> ChartSourceConfig {
    parse_config(NATIVE_CONFIG).unwrap().chart_sources.remove(0)
}

fn request_at(now: &str) -> ResolvedRequest {
    resolve_request(&native_source(), ts(now), 1).unwrap()
}

fn parse_fixture(name: &str, now: &str) -> Result<ChartDataset, ChartError> {
    parse_native_snapshot(&fixture(name), &request_at(now))
}

fn refused_snapshot(json: &str, kind: ChartErrorKind) {
    let error = parse_native_snapshot(json, &request_at("2026-11-20T12:02:10Z")).unwrap_err();
    assert_eq!(error.kind, kind, "{error}");
}

fn mutated(name: &str, old: &str, new: &str) -> String {
    replaced(&fixture(name), old, new)
}

// ---------- decimal ----------

#[test]
fn decimal_accepts_the_plain_grammar() {
    for text in ["0", "-1.5", "10", "0.000001"] {
        assert!(ChartDecimal::parse(text).is_ok(), "{text}");
    }
    let extreme = format!("-{}.{}", "9".repeat(24), "9".repeat(18));
    assert!(ChartDecimal::parse(&extreme).is_ok());
}

#[test]
fn decimal_refuses_everything_else() {
    for text in [
        "1e3", "+1", ".5", "5.", "", " 1", "1 ", "007", "-", "NaN", "1,5", "-.5",
    ] {
        assert_eq!(
            ChartDecimal::parse(text),
            Err(DecimalError::Syntax),
            "{text:?}"
        );
    }
    assert_eq!(
        ChartDecimal::parse(&"1".repeat(25)),
        Err(DecimalError::TooManyIntegerDigits)
    );
    assert_eq!(
        ChartDecimal::parse(&format!("0.{}", "1".repeat(19))),
        Err(DecimalError::TooManyFractionDigits)
    );
}

#[test]
fn decimal_sums_exactly_and_refuses_overflow() {
    let (a, b, c) = (
        ChartDecimal::parse("0.1").unwrap(),
        ChartDecimal::parse("0.2").unwrap(),
        ChartDecimal::parse("0.3").unwrap(),
    );
    assert_eq!(a.checked_add(&b).unwrap(), c);
    let nines = ChartDecimal::parse(&"9".repeat(24)).unwrap();
    let one = ChartDecimal::parse("1").unwrap();
    assert_eq!(nines.checked_add(&one), Err(DecimalError::Overflow));
    let negative = ChartDecimal::parse(&format!("-{}", "9".repeat(24))).unwrap();
    assert_eq!(
        negative.checked_add(
            &one.checked_add(&one)
                .unwrap()
                .checked_add(&negative)
                .unwrap()
        ),
        Err(DecimalError::Overflow)
    );
}

#[test]
fn decimal_plain_string_normalizes() {
    let plain = |text: &str| ChartDecimal::parse(text).unwrap().to_plain_string();
    assert_eq!(plain("1.50"), "1.5");
    assert_eq!(plain("-0"), "0");
    assert_eq!(plain("0.000"), "0");
    assert_eq!(plain("100"), "100");
    assert_eq!(plain("-12.340"), "-12.34");
    assert!(ChartDecimal::zero().is_zero());
    assert!(ChartDecimal::parse("-1").unwrap().is_negative());
    assert!(!ChartDecimal::parse("-0").unwrap().is_negative());
    assert_eq!(ChartDecimal::parse("0.5").unwrap().to_f64(), 0.5);
}

#[test]
fn decimal_fixed_rounds_half_away_from_zero_without_altering_the_value() {
    let fixed = |text: &str, places| ChartDecimal::parse(text).unwrap().format_fixed(places);
    assert_eq!(fixed("2.345", 2), "2.35");
    assert_eq!(fixed("-2.345", 2), "-2.35");
    assert_eq!(fixed("2.5", 0), "3");
    assert_eq!(fixed("-2.5", 0), "-3");
    assert_eq!(fixed("0.004", 2), "0.00");
    assert_eq!(fixed("1.5", 2), "1.50");
    let value = ChartDecimal::parse("2.345").unwrap();
    let _ = value.format_fixed(2);
    assert_eq!(value.to_plain_string(), "2.345");
}

// ---------- time and windows ----------

#[test]
fn instants_require_an_offset_and_format_in_utc() {
    assert!(parse_instant("2026-11-20T12:00:00").is_err());
    assert!(parse_instant("2026-11-20").is_err());
    assert!(parse_instant("not a time").is_err());
    let with_offset = parse_instant("2026-11-20T07:00:00.250-05:00").unwrap();
    assert_eq!(format_utc(with_offset), "2026-11-20T12:00:00Z");
    assert!(Interval::new(ts("2026-11-20T12:00:00Z"), ts("2026-11-20T12:00:00Z")).is_err());
    let interval = Interval::new(ts("2026-11-20T12:00:00Z"), ts("2026-11-20T12:01:00Z")).unwrap();
    assert_eq!(interval.seconds(), 60);
}

#[test]
fn zones_resolve_local_time_with_offsets() {
    assert!(ChartZone::parse("Not/AZone").is_err());
    assert!(ChartZone::parse("").is_err());
    let zone = ChartZone::parse("America/New_York").unwrap();
    assert_eq!(zone.name(), "America/New_York");
    let local = zone.local(ts("2026-11-01T05:30:00Z"));
    assert_eq!(
        (local.month, local.day, local.hour, local.minute),
        (11, 1, 1, 30)
    );
    assert_eq!(local.offset_seconds, -4 * 3600);
    let repeated = zone.local(ts("2026-11-01T06:30:00Z"));
    assert_eq!((repeated.hour, repeated.offset_seconds), (1, -5 * 3600));
}

#[test]
fn fixed_windows_parse_and_resolve() {
    assert_eq!(
        WindowSpec::parse_fixed("3h"),
        Ok(WindowSpec::Fixed { minutes: 180 })
    );
    assert_eq!(
        WindowSpec::parse_fixed("90m"),
        Ok(WindowSpec::Fixed { minutes: 90 })
    );
    assert_eq!(
        WindowSpec::parse_fixed("30d"),
        Ok(WindowSpec::Fixed { minutes: 43_200 })
    );
    for bad in [
        "0m",
        "30d1m",
        "90s",
        "1.5h",
        "",
        "h",
        "-1h",
        "31d",
        "720h1",
        "99999999999999999999m",
    ] {
        assert!(WindowSpec::parse_fixed(bad).is_err(), "{bad:?}");
    }
    let now = ts("2026-11-20T12:00:00Z");
    let window = resolve_window(
        &WindowSpec::Fixed { minutes: 180 },
        &ChartZone::parse("UTC").unwrap(),
        now,
    )
    .unwrap();
    assert_eq!(window.start, ts("2026-11-20T09:00:00Z"));
    assert_eq!(window.end, now);
}

fn calendar_source(days: &str) -> ChartSourceConfig {
    let text = replaced(
        NATIVE_CONFIG,
        "window = \"3h\"",
        &format!("calendar_days = {days}"),
    );
    let text = replaced(
        &text,
        "timezone = \"UTC\"",
        "timezone = \"America/New_York\"",
    );
    parse_config(&text).unwrap().chart_sources.remove(0)
}

#[test]
fn calendar_windows_start_at_local_midnight() {
    let request = resolve_request(&calendar_source("5"), ts("2026-11-02T20:00:00Z"), 1).unwrap();
    assert_eq!(request.window.start, ts("2026-10-29T04:00:00Z"));
    assert_eq!(request.window.end, ts("2026-11-02T20:00:00Z"));
    assert_eq!(request.timezone, "America/New_York");
    let one = resolve_request(&calendar_source("1"), ts("2026-11-02T20:00:00Z"), 1).unwrap();
    assert_eq!(one.window.start, ts("2026-11-02T05:00:00Z"));
}

#[test]
fn dst_day_buckets_keep_their_real_duration() {
    let november = parse_fixture("dst-five-days.json", "2026-11-10T00:00:00Z").unwrap();
    let hours: Vec<i64> = november
        .intervals()
        .iter()
        .map(|i| i.seconds() / 3600)
        .collect();
    assert_eq!(hours, [24, 24, 24, 25, 24]);
    let spring = parse_fixture("dst-spring.json", "2026-11-10T00:00:00Z").unwrap();
    let hours: Vec<i64> = spring
        .intervals()
        .iter()
        .map(|i| i.seconds() / 3600)
        .collect();
    assert_eq!(hours, [24, 23]);
    let axis = TimeAxis::new(november.range());
    let (left, right) = axis.span(&november.intervals()[3]);
    let (short_left, short_right) = axis.span(&november.intervals()[2]);
    let ratio = (right - left) / (short_right - short_left);
    assert!((ratio - 25.0 / 24.0).abs() < 1e-12);
}

#[test]
fn calendar_day_counts_and_exclusivity_are_validated() {
    for days in ["0", "401"] {
        let text = replaced(
            NATIVE_CONFIG,
            "window = \"3h\"",
            &format!("calendar_days = {days}"),
        );
        refused(&text, "calendar_days");
    }
    let both = replaced(
        NATIVE_CONFIG,
        "window = \"3h\"",
        "window = \"3h\"\ncalendar_days = 5",
    );
    refused(&both, "mutually exclusive");
    let neither = replaced(NATIVE_CONFIG, "window = \"3h\"\n", "");
    refused(&neither, "window or calendar_days");
    refused(&replaced(NATIVE_CONFIG, "\"3h\"", "\"31d\""), "window");
    refused(
        &replaced(NATIVE_CONFIG, "\"UTC\"", "\"Mars/Base\""),
        "timezone",
    );
}

// ---------- configuration ----------

#[test]
fn the_design_sketch_parses_and_absent_keys_leave_vectors_empty() {
    let config = parse_config(NATIVE_CONFIG).unwrap();
    assert_eq!(config.chart_sources.len(), 1);
    assert_eq!(config.chart_sources[0].format, SourceFormat::NativeV1);
    assert_eq!(config.chart_cards.len(), 1);
    assert_eq!(config.chart_cards[0].summary, Some(Reduction::Sum));
    let plain = parse_config("theme = \"paper\"\n").unwrap();
    assert!(plain.chart_sources.is_empty() && plain.chart_cards.is_empty());
    assert!(parse_config(COLUMNS_CONFIG).is_ok());
}

#[test]
fn source_defaults_follow_the_design() {
    let source = native_source();
    assert_eq!(source.executable(), "example-chart-data");
    assert_eq!(source.refresh().as_secs(), 30);
    assert_eq!(source.timeout().as_secs(), 10);
    assert_eq!(source.stale_after().as_secs(), 90);
    assert!(!source.capture_stderr);
    let slow = replaced(
        NATIVE_CONFIG,
        "refresh_seconds = 30",
        "refresh_seconds = 60",
    );
    assert_eq!(
        parse_config(&slow).unwrap().chart_sources[0]
            .stale_after()
            .as_secs(),
        120
    );
    let explicit = replaced(
        NATIVE_CONFIG,
        "refresh_seconds = 30",
        "refresh_seconds = 30\nstale_after_seconds = 60",
    );
    assert_eq!(
        parse_config(&explicit).unwrap().chart_sources[0]
            .stale_after()
            .as_secs(),
        60
    );
}

#[test]
fn invalid_source_settings_are_refused_naming_the_key() {
    let cases = [
        (
            "refresh_seconds = 30",
            "refresh_seconds = 4",
            "refresh_seconds",
        ),
        (
            "refresh_seconds = 30",
            "timeout_seconds = 0",
            "timeout_seconds",
        ),
        (
            "refresh_seconds = 30",
            "timeout_seconds = 61",
            "timeout_seconds",
        ),
        (
            "refresh_seconds = 30",
            "stale_after_seconds = 59",
            "stale_after_seconds",
        ),
        ("refresh_seconds = 30", "mystery = 1", "mystery"),
        ("id = \"example-metrics\"", "id = \"Bad Id\"", "id"),
        ("\"example-chart-data\"", "\"~/bin/x\"", "command"),
        ("\"example-chart-data\"", "\"a|b\"", "command"),
        ("\"example-chart-data\"", "\"$HOME/x\"", "command"),
        ("\"example-chart-data\"", "\"\"", "command"),
        ("\"--from\"", "\"--from={from}\"", "command"),
        ("\"{timezone}\"", "\"{when}\"", "command"),
        ("\"{to}\"", "\"{to\"", "command"),
        (
            "format = \"sidescope-chart-v1\"",
            "format = \"columns-v1\"",
            "mapping",
        ),
        (
            "format = \"sidescope-chart-v1\"",
            "format = \"invalid\"",
            "format",
        ),
    ];
    for (old, new, key) in cases {
        refused(&replaced(NATIVE_CONFIG, old, new), key);
    }
    refused(
        "[[chart_sources]]\nid = \"x\"\ncommand = []\nformat = \"sidescope-chart-v1\"\nwindow = \"3h\"\ntimezone = \"UTC\"\n",
        "command",
    );
}

#[test]
fn the_error_for_a_bad_card_names_the_card_key() {
    let cases = [
        (
            "source = \"example-metrics\"",
            "source = \"missing\"",
            "chart_cards[0].source",
        ),
        (
            "id = \"recent-amount\"",
            "id = \"BAD\"",
            "chart_cards[0].id",
        ),
        ("summary = \"sum\"", "top_n = 5", "top_n"),
        ("summary = \"sum\"", "stack_by = \"project\"", "stack_by"),
        ("summary = \"sum\"", "group_by = [\"a\", \"a\"]", "group_by"),
        ("summary = \"sum\"", "color = \"red\"", "color"),
    ];
    for (old, new, key) in cases {
        refused(&replaced(NATIVE_CONFIG, old, new), key);
    }
    let ranked = replaced(NATIVE_CONFIG, "\"time-series\"", "\"ranked-bars\"");
    for top_n in ["0", "17"] {
        refused(
            &replaced(&ranked, "summary = \"sum\"", &format!("top_n = {top_n}")),
            "top_n",
        );
    }
    assert!(parse_config(&replaced(&ranked, "summary = \"sum\"", "top_n = 16")).is_ok());
    let bars = replaced(NATIVE_CONFIG, "\"time-series\"", "\"time-bars\"");
    assert!(
        parse_config(&replaced(
            &bars,
            "summary = \"sum\"",
            "stack_by = \"project\""
        ))
        .is_ok()
    );
}

#[test]
fn duplicate_ids_are_refused() {
    let doubled_source = format!(
        "{NATIVE_CONFIG}{}",
        NATIVE_CONFIG.split("[[chart_cards]]").next().unwrap()
    );
    refused(&doubled_source, "duplicate source id");
    let doubled_card = format!(
        "{NATIVE_CONFIG}[[chart_cards]]{}",
        NATIVE_CONFIG.split("[[chart_cards]]").nth(1).unwrap()
    );
    refused(&doubled_card, "duplicate card id");
}

#[test]
fn mapping_is_required_exactly_for_columns_sources() {
    let no_mapping = COLUMNS_CONFIG.replace(
        &COLUMNS_CONFIG[COLUMNS_CONFIG.find("[chart_sources.mapping]").unwrap()
            ..COLUMNS_CONFIG.find("[[chart_cards]]").unwrap()],
        "",
    );
    refused(&no_mapping.replace("cost-card", "x"), "mapping");
    let native_with_mapping = format!(
        "{}{}",
        NATIVE_CONFIG.split("[[chart_cards]]").next().unwrap(),
        &COLUMNS_CONFIG[COLUMNS_CONFIG.find("[chart_sources.mapping]").unwrap()
            ..COLUMNS_CONFIG.find("[[chart_cards]]").unwrap()]
    );
    refused(&native_with_mapping, "mapping");
}

#[test]
fn mapping_pointers_and_declarations_are_validated() {
    let cases = [
        (
            "buckets = \"/buckets\"",
            "buckets = \"buckets\"",
            "mapping.buckets",
        ),
        (
            "groups = \"/groups\"",
            "groups = \"/a~2b\"",
            "mapping.groups",
        ),
        (
            "interval_end = \"/end\"",
            "interval_end = \"end\"",
            "mapping.interval_end",
        ),
        (
            "values = \"/cost\"",
            "values = \"~cost\"",
            "mapping.metrics.values",
        ),
        (
            "quality_counter = \"/unpriced\"",
            "quality_counter = \"unpriced\"",
            "quality_counter",
        ),
        (
            "pointer = \"/keys/0\"",
            "pointer = \"keys\"",
            "mapping.dimensions.pointer",
        ),
        ("id = \"load\"", "id = \"cost\"", "duplicate metric id"),
        ("quantity = \"gauge\"", "quantity = \"rate\"", "quantity"),
        ("unit = \"USD\"", "unit = \"\"", "mapping.metrics.unit"),
    ];
    for (old, new, key) in cases {
        refused(&replaced(COLUMNS_CONFIG, old, new), key);
    }
    let escaped = replaced(
        COLUMNS_CONFIG,
        "groups = \"/groups\"",
        "groups = \"/a~0b/c~1d\"",
    );
    assert!(parse_config(&escaped).is_ok());
    let twin = replaced(
        COLUMNS_CONFIG,
        "dimensions = [{ name = \"project\", pointer = \"/keys/0\" }]",
        "dimensions = [{ name = \"project\", pointer = \"/keys/0\" }, { name = \"project\", pointer = \"/keys/1\" }]",
    );
    refused(&twin, "duplicate dimension");
}

#[test]
fn columns_cards_are_checked_against_the_mapping() {
    let cases = [
        (
            "metric = \"cost\"",
            "metric = \"absent\"",
            "chart_cards[0].metric",
        ),
        (
            "group_by = [\"project\"]",
            "group_by = [\"model\"]",
            "chart_cards[0].group_by",
        ),
        (
            "group_by = [\"project\"]",
            "summary = \"last\"",
            "chart_cards[0].summary",
        ),
        (
            "group_by = [\"project\"]",
            "bounded_percent = true",
            "chart_cards[0].bounded_percent",
        ),
    ];
    for (old, new, key) in cases {
        refused(&replaced(COLUMNS_CONFIG, old, new), key);
    }
    let stacked = replaced(COLUMNS_CONFIG, "\"ranked-bars\"", "\"time-bars\"");
    refused(
        &replaced(&stacked, "group_by = [\"project\"]", "stack_by = \"model\""),
        "stack_by",
    );
    let bounded = replaced(COLUMNS_CONFIG, "metric = \"cost\"", "metric = \"load\"");
    let bounded = replaced(
        &bounded,
        "group_by = [\"project\"]",
        "bounded_percent = true\nsummary = \"last\"",
    );
    assert!(parse_config(&bounded).is_ok());
    // Native sources cannot decide these before data arrives (R4).
    let native = replaced(
        NATIVE_CONFIG,
        "summary = \"sum\"",
        "summary = \"last\"\nbounded_percent = true",
    );
    assert!(parse_config(&native).is_ok());
}

#[test]
fn validate_chart_config_is_callable_directly() {
    let config = parse_config(NATIVE_CONFIG).unwrap();
    assert!(validate_chart_config(&config.chart_sources, &config.chart_cards).is_ok());
    assert!(validate_chart_config(&[], &config.chart_cards).is_err());
}

#[test]
fn requests_substitute_placeholders_one_argument_each_without_running_anything() {
    let source = native_source();
    let request = resolve_request(&source, ts("2026-11-20T12:00:00.987Z"), 7).unwrap();
    assert_eq!(
        request.argv,
        [
            "example-chart-data",
            "--from",
            "2026-11-20T09:00:00Z",
            "--to",
            "2026-11-20T12:00:00Z",
            "--timezone",
            "UTC"
        ]
    );
    assert_eq!(request.window.end, ts("2026-11-20T12:00:00Z"));
    assert_eq!(
        (request.source_id.as_str(), request.revision),
        ("example-metrics", 7)
    );
    assert_eq!(request.timezone, "UTC");
    // The executable does not exist; resolving must still succeed.
    assert_eq!(source.executable(), "example-chart-data");
}

#[test]
fn request_errors_never_contain_argument_values() {
    let mut source = native_source();
    source.command.push("secret-argument-value".into());
    source.timezone = "Secret/Zone".into();
    let error = resolve_request(&source, ts("2026-11-20T12:00:00Z"), 1).unwrap_err();
    assert_eq!(error.kind, ChartErrorKind::ConfigInvalid);
    assert!(!error.message().contains("secret-argument-value"));
    assert!(!error.message().contains("Secret/Zone"));
}

// ---------- dataset ----------

#[test]
fn fixtures_parse_into_valid_datasets() {
    let minimal = parse_fixture("minimal.json", "2026-11-20T12:02:10Z").unwrap();
    assert_eq!(minimal.revision(), Some("sample-7"));
    assert_eq!(minimal.observed_through(), None);
    assert_eq!(minimal.timezone().name(), "UTC");
    assert_eq!(minimal.intervals().len(), 2);
    assert_eq!(minimal.dimension_names(), ["project"]);
    assert_eq!(minimal.notices().len(), 1);
    assert_eq!(
        minimal.metric("amount").unwrap().quantity,
        Quantity::IntervalTotal
    );
    assert!(minimal.metric("absent").is_none());
    let cells = &minimal.series()[0].metrics["amount"];
    assert_eq!(cells[0].quality, Quality::Complete);
    assert_eq!(cells[1].quality, Quality::Partial);
    assert_eq!(cells[1].value.as_ref().unwrap().to_plain_string(), "0.03");
    assert_eq!(minimal.request_window().end, ts("2026-11-20T12:02:10Z"));
    assert_eq!(minimal.generated_at(), ts("2026-11-20T12:02:10Z"));

    let all_null = parse_fixture("all-null.json", "2026-11-20T12:02:10Z").unwrap();
    assert!(
        all_null.series()[0].metrics["amount"]
            .iter()
            .all(|c| c.quality == Quality::Missing && c.value.is_none())
    );

    let empty = parse_fixture("empty-series.json", "2026-11-20T12:02:10Z").unwrap();
    assert!(empty.series().is_empty() && empty.dimension_names().is_empty());

    let gauge = parse_fixture("gauge.json", "2026-11-20T12:02:10Z").unwrap();
    assert_eq!(gauge.metric("latency").unwrap().quantity, Quantity::Gauge);

    let many = parse_fixture("many-series.json", "2026-11-20T12:02:10Z").unwrap();
    assert_eq!(many.series().len(), 20);
}

#[test]
fn intervals_ending_after_the_request_downgrade_complete_to_partial() {
    let current = parse_fixture("partial-current.json", "2026-11-20T15:00:00Z").unwrap();
    let cells = &current.series()[0].metrics["amount"];
    assert_eq!(cells[0].quality, Quality::Complete);
    assert_eq!(cells[1].quality, Quality::Partial);
    assert_eq!(cells[1].value.as_ref().unwrap().to_plain_string(), "2");
    let finished = parse_fixture("partial-current.json", "2026-11-22T00:00:00Z").unwrap();
    assert!(
        finished.series()[0].metrics["amount"]
            .iter()
            .all(|c| c.quality == Quality::Complete)
    );
    // Missing cells stay missing.
    let missing = mutated(
        "all-null.json",
        "\"end\": \"2026-11-20T12:02:00Z\"",
        "\"end\": \"2026-11-20T12:02:00Z\"",
    );
    let dataset = parse_native_snapshot(&missing, &request_at("2026-11-20T12:00:30Z")).unwrap();
    assert!(
        dataset.series()[0].metrics["amount"]
            .iter()
            .all(|c| c.quality == Quality::Missing)
    );
}

#[test]
fn malformed_and_wrong_version_snapshots_are_refused() {
    refused_snapshot("not json", ChartErrorKind::ParseFailure);
    refused_snapshot("", ChartErrorKind::ParseFailure);
    refused_snapshot(
        "{\"version\": \"sidescope-chart-v1\"",
        ChartErrorKind::ParseFailure,
    );
    refused_snapshot(
        &mutated("minimal.json", "sidescope-chart-v1", "sidescope-chart-v2"),
        ChartErrorKind::UnsupportedVersion,
    );
    refused_snapshot("{}", ChartErrorKind::InvalidDataset);
    refused_snapshot(
        "{\"version\": \"sidescope-chart-v1\"}",
        ChartErrorKind::InvalidDataset,
    );
    refused_snapshot(
        &mutated(
            "minimal.json",
            "\"estimate\": true",
            "\"estimate\": \"yes\"",
        ),
        ChartErrorKind::InvalidDataset,
    );
}

#[test]
fn invariant_violations_reject_the_whole_snapshot() {
    let invalid = ChartErrorKind::InvalidDataset;
    let cases: Vec<(&str, String)> = vec![
        ("duplicate series id", {
            let mut value: Value = serde_json::from_str(&fixture("many-series.json")).unwrap();
            value["series"][1]["id"] = value["series"][0]["id"].clone();
            value["series"][1]["dimensions"]["project"] = json!("Other");
            value.to_string()
        }),
        ("duplicate metric id", {
            let text = fixture("minimal.json");
            replaced(
                &text,
                "\"metrics\": {\n    \"amount\": {\n      \"label\"",
                "\"metrics\": {\n    \"amount\": {\"label\": \"x\", \"unit\": \"USD\", \"quantity\": \"gauge\", \"estimate\": false},\n    \"amount\": {\n      \"label\"",
            )
        }),
        (
            "values shorter",
            mutated(
                "minimal.json",
                "\"values\": [\n            \"0.12\",\n            \"0.03\"\n          ]",
                "\"values\": [\"0.12\"]",
            ),
        ),
        (
            "quality shorter",
            mutated(
                "minimal.json",
                "\"complete\",\n            \"partial\"",
                "\"complete\"",
            ),
        ),
        (
            "overlap",
            mutated(
                "minimal.json",
                "\"start\": \"2026-11-20T12:01:00Z\"",
                "\"start\": \"2026-11-20T12:00:30Z\"",
            ),
        ),
        ("unordered", {
            let mut value: Value = serde_json::from_str(&fixture("minimal.json")).unwrap();
            value["intervals"].as_array_mut().unwrap().reverse();
            value.to_string()
        }),
        (
            "zero length",
            mutated(
                "minimal.json",
                "\"end\": \"2026-11-20T12:01:00Z\"",
                "\"end\": \"2026-11-20T12:00:00Z\"",
            ),
        ),
        (
            "outside range",
            mutated(
                "minimal.json",
                "\"end\": \"2026-11-20T12:02:00Z\"\n    }\n  ],",
                "\"end\": \"2026-11-20T12:03:00Z\"\n    }\n  ],",
            ),
        ),
        (
            "complete with null",
            mutated(
                "all-null.json",
                "\"missing\",\n            \"missing\"",
                "\"complete\",\n            \"missing\"",
            ),
        ),
        (
            "missing with value",
            mutated(
                "minimal.json",
                "\"complete\",\n            \"partial\"",
                "\"missing\",\n            \"partial\"",
            ),
        ),
        (
            "unknown quality",
            mutated("minimal.json", "\"complete\"", "\"fine\""),
        ),
        (
            "bad decimal",
            mutated("minimal.json", "\"0.12\"", "\"1e3\""),
        ),
        ("float value", mutated("minimal.json", "\"0.12\"", "0.12")),
        (
            "bad timestamp",
            mutated(
                "minimal.json",
                "\"generated_at\": \"2026-11-20T12:02:10Z\"",
                "\"generated_at\": \"yesterday\"",
            ),
        ),
        (
            "timestamp without offset",
            mutated(
                "minimal.json",
                "\"generated_at\": \"2026-11-20T12:02:10Z\"",
                "\"generated_at\": \"2026-11-20T12:02:10\"",
            ),
        ),
        (
            "bad zone",
            mutated(
                "minimal.json",
                "\"timezone\": \"UTC\"",
                "\"timezone\": \"Mars/Base\"",
            ),
        ),
        (
            "range reversed",
            mutated(
                "minimal.json",
                "\"start\": \"2026-11-20T12:00:00Z\",\n    \"end\": \"2026-11-20T12:02:00Z\"",
                "\"start\": \"2026-11-20T12:02:00Z\",\n    \"end\": \"2026-11-20T12:00:00Z\"",
            ),
        ),
        (
            "series lacks a declared metric",
            mutated(
                "minimal.json",
                "\"amount\": {\n          \"values\"",
                "\"other\": {\n          \"values\"",
            ),
        ),
        (
            "control characters",
            mutated(
                "minimal.json",
                "\"Estimated amount\"",
                "\"Estimated\\u001b[31m amount\"",
            ),
        ),
        (
            "control characters in dimension",
            mutated("minimal.json", "\"Example\"", "\"Exam\\nple\""),
        ),
        (
            "duplicate dimension key",
            mutated(
                "minimal.json",
                "\"project\": \"Example\"",
                "\"project\": \"Example\", \"project\": \"Again\"",
            ),
        ),
        (
            "empty series id",
            mutated("minimal.json", "\"sample-project\"", "\"\""),
        ),
        (
            "missing required field",
            mutated("minimal.json", "\"unit\": \"USD\",", ""),
        ),
    ];
    for (name, json) in cases {
        let error =
            parse_native_snapshot(&json, &request_at("2026-11-20T12:02:10Z")).expect_err(name);
        assert_eq!(error.kind, invalid, "{name}: {error}");
    }
}

#[test]
fn series_must_share_dimension_names_and_tuples() {
    let mut value: Value = serde_json::from_str(&fixture("many-series.json")).unwrap();
    value["series"][1]["dimensions"] = json!({"project": "Example 00"});
    refused_snapshot(&value.to_string(), ChartErrorKind::InvalidDataset);
    let mut value: Value = serde_json::from_str(&fixture("many-series.json")).unwrap();
    value["series"][1]["dimensions"] = json!({"team": "Example 01"});
    refused_snapshot(&value.to_string(), ChartErrorKind::InvalidDataset);
    let mut value: Value = serde_json::from_str(&fixture("many-series.json")).unwrap();
    value["series"][1]["dimensions"] = json!({"project": "Example 01", "team": "x"});
    refused_snapshot(&value.to_string(), ChartErrorKind::InvalidDataset);
}

#[test]
fn unknown_optional_fields_are_ignored() {
    let mut value: Value = serde_json::from_str(&fixture("minimal.json")).unwrap();
    value["future"] = json!({"anything": [1, 2, 3]});
    value["series"][0]["note"] = json!("extra");
    value["metrics"]["amount"]["tooltip"] = json!("extra");
    assert!(parse_native_snapshot(&value.to_string(), &request_at("2026-11-20T12:02:10Z")).is_ok());
}

fn sized_snapshot(intervals: usize, series: usize, metrics: usize) -> String {
    let start = 1_700_000_000i64;
    let at = |second: i64| Timestamp::from_second(second).unwrap().to_string();
    let interval_list: Vec<Value> = (0..intervals as i64)
        .map(|i| json!({"start": at(start + i), "end": at(start + i + 1)}))
        .collect();
    let metric_map: serde_json::Map<String, Value> = (0..metrics)
        .map(|m| (format!("m{m}"), json!({"label": "M", "unit": "count", "quantity": "interval_total", "estimate": false})))
        .collect();
    let series_list: Vec<Value> = (0..series)
        .map(|s| {
            let per_metric: serde_json::Map<String, Value> = (0..metrics)
                .map(|m| (format!("m{m}"), json!({"values": vec!["1"; intervals], "quality": vec!["complete"; intervals]})))
                .collect();
            json!({"id": format!("s{s}"), "dimensions": {"n": format!("{s}")}, "metrics": per_metric})
        })
        .collect();
    json!({
        "version": "sidescope-chart-v1",
        "generated_at": at(start + intervals as i64 + 5),
        "range": {"start": at(start), "end": at(start + intervals as i64), "timezone": "UTC"},
        "intervals": interval_list,
        "metrics": metric_map,
        "series": series_list,
    })
    .to_string()
}

#[test]
fn size_limits_refuse_instead_of_truncating() {
    let request = request_at("2026-11-20T12:02:10Z");
    let accepted = parse_native_snapshot(&sized_snapshot(2_048, 1, 1), &request).unwrap();
    assert_eq!(accepted.intervals().len(), 2_048);
    assert!(parse_native_snapshot(&sized_snapshot(256, 1, 32), &request).is_ok());
    for (intervals, series, metrics) in [(2_049, 1, 1), (1, 257, 1), (1, 1, 33), (13, 256, 32)] {
        assert!(intervals * series * metrics > 0);
        let error = parse_native_snapshot(&sized_snapshot(intervals, series, metrics), &request)
            .unwrap_err();
        assert_eq!(
            error.kind,
            ChartErrorKind::InvalidDataset,
            "{intervals}x{series}x{metrics}"
        );
    }
}

// ---------- reductions ----------

fn cell(value: Option<&str>, quality: Quality) -> sidescope::chart::dataset::Cell {
    sidescope::chart::dataset::Cell {
        value: value.map(|v| ChartDecimal::parse(v).unwrap()),
        quality,
    }
}

#[test]
fn sums_track_missing_and_partial_cells() {
    let complete = sum_cells(&[
        cell(Some("0.1"), Quality::Complete),
        cell(Some("0.2"), Quality::Complete),
    ])
    .unwrap();
    assert_eq!(complete.quality, Quality::Complete);
    assert_eq!(complete.known.unwrap().to_plain_string(), "0.3");
    assert_eq!(
        (complete.missing, complete.partial, complete.cells),
        (0, 0, 2)
    );

    let partial = sum_cells(&[
        cell(Some("1"), Quality::Complete),
        cell(Some("2"), Quality::Partial),
    ])
    .unwrap();
    assert_eq!((partial.quality, partial.partial), (Quality::Partial, 1));
    assert_eq!(partial.known.unwrap().to_plain_string(), "3");

    let missing = sum_cells(&[
        cell(Some("1"), Quality::Partial),
        cell(None, Quality::Missing),
    ])
    .unwrap();
    assert_eq!((missing.quality, missing.missing), (Quality::Missing, 1));
    assert_eq!(missing.known.unwrap().to_plain_string(), "1");

    let none = sum_cells(&[cell(None, Quality::Missing), cell(None, Quality::Missing)]).unwrap();
    assert_eq!((none.known, none.quality), (None, Quality::Missing));
}

#[test]
fn sum_overflow_is_an_error() {
    let nines = "9".repeat(24);
    let cells = [
        cell(Some(&nines), Quality::Complete),
        cell(Some("1"), Quality::Complete),
    ];
    assert_eq!(sum_cells(&cells).unwrap_err(), DecimalError::Overflow);
}

#[test]
fn reductions_are_permitted_per_quantity() {
    assert!(Reduction::Sum.permitted(Quantity::IntervalTotal));
    assert!(!Reduction::Sum.permitted(Quantity::Gauge));
    assert!(Reduction::Last.permitted(Quantity::Gauge));
    assert!(!Reduction::Last.permitted(Quantity::IntervalTotal));
}

#[test]
fn last_known_reports_whether_it_is_the_latest_cell() {
    let latest = last_known(&[
        cell(Some("1"), Quality::Complete),
        cell(Some("2"), Quality::Partial),
    ])
    .unwrap();
    assert_eq!(
        (latest.index, latest.is_latest, latest.quality),
        (1, true, Quality::Partial)
    );
    let stale = last_known(&[
        cell(Some("1"), Quality::Complete),
        cell(None, Quality::Missing),
    ])
    .unwrap();
    assert_eq!(
        (stale.index, stale.is_latest, stale.quality),
        (0, false, Quality::Complete)
    );
    assert_eq!(stale.value.to_plain_string(), "1");
    assert!(last_known(&[cell(None, Quality::Missing)]).is_none());
    assert!(last_known(&[]).is_none());
}

fn two_dimension_dataset() -> ChartDataset {
    let text = json!({
        "version": "sidescope-chart-v1",
        "generated_at": "2026-11-20T12:02:10Z",
        "range": {"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:03:00Z", "timezone": "UTC"},
        "intervals": [
            {"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:01:00Z"},
            {"start": "2026-11-20T12:01:00Z", "end": "2026-11-20T12:02:00Z"},
            {"start": "2026-11-20T12:02:00Z", "end": "2026-11-20T12:03:00Z"}
        ],
        "metrics": {"amount": {"label": "Amount", "unit": "USD", "quantity": "interval_total", "estimate": false}},
        "series": [
            {"id": "b-two", "dimensions": {"project": "B", "model": "two"}, "metrics": {"amount": {"values": ["1", null, "1"], "quality": ["complete", "missing", "partial"]}}},
            {"id": "a-one", "dimensions": {"project": "A", "model": "one"}, "metrics": {"amount": {"values": ["2", "2", "2"], "quality": ["complete", "complete", "complete"]}}},
            {"id": "a-two", "dimensions": {"project": "A", "model": "two"}, "metrics": {"amount": {"values": ["3", "3", "3"], "quality": ["partial", "complete", "complete"]}}}
        ]
    })
    .to_string();
    parse_native_snapshot(&text, &request_at("2026-11-20T12:10:00Z")).unwrap()
}

#[test]
fn collapsing_merges_series_by_retained_dimensions() {
    let dataset = two_dimension_dataset();
    let by_project = collapse_series(&dataset, "amount", &["project".into()]).unwrap();
    assert_eq!(by_project.len(), 2);
    assert_eq!(
        by_project[0].key,
        [("project".to_string(), "A".to_string())]
    );
    assert_eq!(
        by_project[1].key,
        [("project".to_string(), "B".to_string())]
    );
    let a = &by_project[0].cells;
    assert_eq!(
        (a[0].value.as_ref().unwrap().to_plain_string(), a[0].quality),
        ("5".into(), Quality::Partial)
    );
    assert_eq!(
        (a[1].value.as_ref().unwrap().to_plain_string(), a[1].quality),
        ("5".into(), Quality::Complete)
    );
    assert_eq!(by_project[1].cells[1].quality, Quality::Missing);
    assert!(by_project[1].cells[1].value.is_none());

    let everything = collapse_series(&dataset, "amount", &[]).unwrap();
    assert_eq!(everything.len(), 1);
    assert!(everything[0].key.is_empty());
    assert_eq!(everything[0].cells[1].quality, Quality::Missing);
    assert_eq!(
        everything[0].cells[1]
            .value
            .as_ref()
            .unwrap()
            .to_plain_string(),
        "5"
    );
    assert_eq!(
        everything[0].cells[0]
            .value
            .as_ref()
            .unwrap()
            .to_plain_string(),
        "6"
    );

    let both = collapse_series(&dataset, "amount", &["project".into(), "model".into()]).unwrap();
    let keys: Vec<_> = both.iter().map(|g| g.key.clone()).collect();
    assert_eq!(keys.len(), 3);
    assert_eq!(keys[0][1].1, "one");
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    let reversed =
        collapse_series(&dataset, "amount", &["model".into(), "project".into()]).unwrap();
    assert_eq!(reversed[0].key[0].0, "model");
    assert_eq!(
        both,
        collapse_series(&dataset, "amount", &["project".into(), "model".into()]).unwrap()
    );
}

#[test]
fn collapsing_refuses_absent_names_and_tolerates_empty_datasets() {
    let dataset = two_dimension_dataset();
    let error = collapse_series(&dataset, "amount", &["team".into()]).unwrap_err();
    assert_eq!(error.kind, ChartErrorKind::CardMismatch);
    let error = collapse_series(&dataset, "absent", &[]).unwrap_err();
    assert_eq!(error.kind, ChartErrorKind::CardMismatch);
    let empty = parse_fixture("empty-series.json", "2026-11-20T12:02:10Z").unwrap();
    assert!(
        collapse_series(&empty, "amount", &["project".into()])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn merged_overflow_is_reported_not_wrapped() {
    let nines = "9".repeat(24);
    let text = json!({
        "version": "sidescope-chart-v1",
        "generated_at": "2026-11-20T12:02:10Z",
        "range": {"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:01:00Z", "timezone": "UTC"},
        "intervals": [{"start": "2026-11-20T12:00:00Z", "end": "2026-11-20T12:01:00Z"}],
        "metrics": {"amount": {"label": "Amount", "unit": "USD", "quantity": "interval_total", "estimate": false}},
        "series": [
            {"id": "a", "dimensions": {"p": "a"}, "metrics": {"amount": {"values": [nines], "quality": ["complete"]}}},
            {"id": "b", "dimensions": {"p": "b"}, "metrics": {"amount": {"values": ["1"], "quality": ["complete"]}}}
        ]
    })
    .to_string();
    let dataset = parse_native_snapshot(&text, &request_at("2026-11-20T12:02:10Z")).unwrap();
    let error = collapse_series(&dataset, "amount", &[]).unwrap_err();
    assert_eq!(error.kind, ChartErrorKind::InvalidDataset);
}

// ---------- geometry ----------

#[test]
fn time_axis_maps_instants_by_exact_duration() {
    let range = Interval::new(ts("2026-11-20T00:00:00Z"), ts("2026-11-21T00:00:00Z")).unwrap();
    let axis = TimeAxis::new(range);
    assert_eq!(axis.fraction(ts("2026-11-20T00:00:00Z")), 0.0);
    assert_eq!(axis.fraction(ts("2026-11-20T12:00:00Z")), 0.5);
    assert_eq!(axis.fraction(ts("2026-11-22T00:00:00Z")), 1.0);
    assert_eq!(axis.fraction(ts("2026-11-19T00:00:00Z")), 0.0);
    let hour = Interval::new(ts("2026-11-20T06:00:00Z"), ts("2026-11-20T07:00:00Z")).unwrap();
    assert!((axis.center(&hour) - 6.5 / 24.0).abs() < 1e-12);
    let (left, right) = axis.span(&hour);
    assert!(((right - left) - 1.0 / 24.0).abs() < 1e-12);
}

#[test]
fn value_scales_include_zero_and_round_outward() {
    let scale = value_scale(&[3.0, 7.0], ScaleKind::Nonnegative).unwrap();
    assert_eq!((scale.min, scale.max), (0.0, 10.0));
    let scale = value_scale(&[0.12, 0.03], ScaleKind::Nonnegative).unwrap();
    assert_eq!((scale.min, scale.max), (0.0, 0.2));
    let scale = value_scale(&[11.0], ScaleKind::Nonnegative).unwrap();
    assert_eq!(scale.max, 20.0);
    let scale = value_scale(&[-3.0, 7.0], ScaleKind::Signed).unwrap();
    assert_eq!((scale.min, scale.max), (-5.0, 10.0));
    let scale = value_scale(&[4.0, 6.0], ScaleKind::Signed).unwrap();
    assert_eq!(scale.min, 0.0);
    let scale = value_scale(&[-4.0, -6.0], ScaleKind::Signed).unwrap();
    assert_eq!((scale.min, scale.max), (-10.0, 0.0));
    let zero = value_scale(&[0.0, 0.0], ScaleKind::Nonnegative).unwrap();
    assert!(zero.max > zero.min);
    assert!(value_scale(&[], ScaleKind::Signed).is_none());
    assert!(value_scale(&[f64::NAN], ScaleKind::Nonnegative).is_none());
    let percent = value_scale(&[12.0], ScaleKind::Percent0To100).unwrap();
    assert_eq!((percent.min, percent.max), (0.0, 100.0));
    assert_eq!(
        value_scale(&[3.3, 1.1], ScaleKind::Nonnegative),
        value_scale(&[3.3, 1.1], ScaleKind::Nonnegative)
    );
}

#[test]
fn scale_fractions_and_ticks() {
    let scale = value_scale(&[3.0, 7.0], ScaleKind::Nonnegative).unwrap();
    assert_eq!(scale.fraction(5.0), 0.5);
    assert_eq!(scale.fraction(50.0), 1.0);
    assert_eq!(scale.fraction(-1.0), 0.0);
    assert_eq!(scale.ticks(), [0.0, 5.0, 10.0]);
}

// ---------- state and errors ----------

fn snapshot(dataset: bool, error: bool, last_success: Option<&str>) -> SourceSnapshot {
    SourceSnapshot {
        source_id: "example-metrics".into(),
        dataset: dataset
            .then(|| Arc::new(parse_fixture("minimal.json", "2026-11-20T12:02:10Z").unwrap())),
        last_error: error.then(|| ChartError::new(ChartErrorKind::Timeout, "too slow")),
        stderr_excerpt: None,
        info: SourceInfo {
            executable: "example-chart-data".into(),
            last_attempt: None,
            last_success: last_success.map(ts),
            next_retry: None,
            latency_ms: None,
            revision: 1,
            retry_suspended: false,
            request_window: None,
            timezone: "UTC".into(),
            stale_after_seconds: 90,
        },
    }
}

#[test]
fn status_distinguishes_loading_unavailable_failed_stale_and_fresh() {
    let now = ts("2026-11-20T12:10:00Z");
    assert_eq!(
        snapshot(false, false, None).status(now),
        SourceStatus::Loading
    );
    assert_eq!(
        snapshot(false, true, None).status(now),
        SourceStatus::Unavailable
    );
    let fresh = Some("2026-11-20T12:09:00Z");
    let old = Some("2026-11-20T12:08:29Z");
    assert_eq!(
        snapshot(true, false, fresh).status(now),
        SourceStatus::Fresh
    );
    assert_eq!(
        snapshot(true, false, Some("2026-11-20T12:08:30Z")).status(now),
        SourceStatus::Fresh
    );
    assert_eq!(snapshot(true, false, old).status(now), SourceStatus::Stale);
    assert_eq!(
        snapshot(true, true, fresh).status(now),
        SourceStatus::RefreshFailed { stale: false }
    );
    assert_eq!(
        snapshot(true, true, old).status(now),
        SourceStatus::RefreshFailed { stale: true }
    );
}

#[test]
fn errors_are_sanitized_and_classified() {
    assert_eq!(
        sanitize_text("a\u{1b}[31mred\nline\t!", 50),
        "a[31mredline!"
    );
    assert_eq!(sanitize_text("abcdefghij", 5), "abcd…");
    assert_eq!(sanitize_text("abc", 3), "abc");
    let error = ChartError::new(ChartErrorKind::ParseFailure, "bad\u{1b}[0m\nthing");
    assert!(!error.message().contains('\u{1b}') && !error.message().contains('\n'));
    assert!(error.to_string().starts_with("parse failure"));
    let long = ChartError::new(ChartErrorKind::Timeout, "x".repeat(1_000));
    assert!(long.message().chars().count() <= 200);
    let all = [
        ChartErrorKind::ConfigInvalid,
        ChartErrorKind::MissingExecutable,
        ChartErrorKind::SpawnFailed,
        ChartErrorKind::NonzeroExit,
        ChartErrorKind::Timeout,
        ChartErrorKind::OutputBound,
        ChartErrorKind::Cancelled,
        ChartErrorKind::ParseFailure,
        ChartErrorKind::UnsupportedVersion,
        ChartErrorKind::InvalidMapping,
        ChartErrorKind::InvalidDataset,
        ChartErrorKind::CardMismatch,
    ];
    let suspending: Vec<_> = all
        .into_iter()
        .filter(|kind| kind.suspends_retry())
        .collect();
    assert_eq!(
        suspending,
        [
            ChartErrorKind::ConfigInvalid,
            ChartErrorKind::MissingExecutable
        ]
    );
    let mut labels: Vec<_> = all.iter().map(|kind| kind.label()).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), 12);
    assert!(all.iter().all(|kind| !kind.label().is_empty()));
}
