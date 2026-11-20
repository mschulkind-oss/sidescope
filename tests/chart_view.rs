//! Public-API tests for the native chart view: the pure card model, Cairo
//! drawing into an image surface (no display) and hit testing. Widgets need a
//! display and are exercised by the integration stage.

use serde_json::{Value, json};
use sidescope::chart::config::{ChartCardConfig, ChartKind, ResolvedRequest, resolve_request};
use sidescope::chart::dataset::{ChartDataset, Quality, parse_native_snapshot};
use sidescope::chart::error::{ChartError, ChartErrorKind};
use sidescope::chart::reduce::Reduction;
use sidescope::chart::state::{SourceInfo, SourceSnapshot};
use sidescope::chart::time::{Timestamp, parse_instant};
use sidescope::chart_view::{
    ChartCardModel, ChartPalette, PlotModel, RowKind, Swatch, build_model, css, draw_plot,
    hit_test, plot_frame, swatch_for,
};
use sidescope::parse_config;
use sidescope::theme::THEMES;
use std::sync::Arc;

use gtk::cairo::{Context, Format, ImageSurface};

const NOW: &str = "2026-11-20T12:00:00Z";

fn ts(text: &str) -> Timestamp {
    parse_instant(text).unwrap()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/charts/native/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

const SOURCE_CONFIG: &str = r#"
[[chart_sources]]
id = "example-metrics"
command = ["example-chart-data", "--from", "{from}", "--to", "{to}"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"
"#;

fn request() -> ResolvedRequest {
    let source = parse_config(SOURCE_CONFIG).unwrap().chart_sources.remove(0);
    resolve_request(&source, ts(NOW), 1).unwrap()
}

fn dataset_from(json: &str) -> Result<ChartDataset, ChartError> {
    parse_native_snapshot(json, &request())
}

fn info() -> SourceInfo {
    SourceInfo {
        executable: "example-chart-data".to_string(),
        last_attempt: Some(ts(NOW)),
        last_success: Some(ts(NOW)),
        next_retry: Some(ts("2026-11-20T12:00:30Z")),
        latency_ms: Some(42),
        revision: 3,
        retry_suspended: false,
        request_window: None,
        timezone: "UTC".to_string(),
        stale_after_seconds: 90,
    }
}

fn snapshot_of(dataset: ChartDataset) -> SourceSnapshot {
    SourceSnapshot {
        source_id: "example-metrics".to_string(),
        dataset: Some(Arc::new(dataset)),
        last_error: None,
        stderr_excerpt: None,
        info: info(),
    }
}

fn snapshot_json(json: &str) -> SourceSnapshot {
    snapshot_of(dataset_from(json).unwrap())
}

fn fixture_snapshot(name: &str) -> SourceSnapshot {
    snapshot_json(&fixture(name))
}

fn card(kind: ChartKind, metric: &str) -> ChartCardConfig {
    ChartCardConfig {
        id: "card".to_string(),
        source: "example-metrics".to_string(),
        kind,
        metric: metric.to_string(),
        title: None,
        summary: None,
        group_by: Vec::new(),
        top_n: None,
        stack_by: None,
        bounded_percent: false,
    }
}

fn grouped(mut config: ChartCardConfig, dimension: &str) -> ChartCardConfig {
    config.group_by = vec![dimension.to_string()];
    config
}

fn model_at(config: &ChartCardConfig, snapshot: &SourceSnapshot) -> ChartCardModel {
    build_model(config, snapshot, ts(NOW))
}

struct Group<'a> {
    id: &'a str,
    project: &'a str,
    values: Vec<Option<&'a str>>,
    quality: Vec<&'a str>,
}

fn group<'a>(project: &'a str, values: &[Option<&'a str>], quality: &[&'a str]) -> Group<'a> {
    Group {
        id: project,
        project,
        values: values.to_vec(),
        quality: quality.to_vec(),
    }
}

fn unknown<'a>(values: &[&'a str]) -> Group<'a> {
    Group {
        id: "unknown",
        ..complete("", values)
    }
}

fn complete<'a>(project: &'a str, values: &[&'a str]) -> Group<'a> {
    group(
        project,
        &values.iter().map(|v| Some(*v)).collect::<Vec<_>>(),
        &vec!["complete"; values.len()],
    )
}

const DAYS: [(&str, &str); 5] = [
    ("2026-11-16T00:00:00Z", "2026-11-17T00:00:00Z"),
    ("2026-11-17T00:00:00Z", "2026-11-18T00:00:00Z"),
    ("2026-11-18T00:00:00Z", "2026-11-19T00:00:00Z"),
    ("2026-11-19T00:00:00Z", "2026-11-20T00:00:00Z"),
    ("2026-11-20T00:00:00Z", "2026-11-21T00:00:00Z"),
];

struct Spec<'a> {
    quantity: &'a str,
    unit: &'a str,
    unknown_contribution: Option<&'a str>,
    estimate: bool,
}

fn money() -> Spec<'static> {
    Spec {
        quantity: "interval_total",
        unit: "USD",
        unknown_contribution: None,
        estimate: false,
    }
}

fn build_json(
    spec: &Spec,
    intervals: &[(&str, &str)],
    groups: &[Group],
    dimension: &str,
) -> String {
    let mut metric = json!({
        "label": "Spend", "unit": spec.unit, "quantity": spec.quantity, "estimate": spec.estimate
    });
    if let Some(value) = spec.unknown_contribution {
        metric["unknown_contribution"] = json!(value);
    }
    let series: Vec<Value> = groups
        .iter()
        .map(|g| {
            json!({
                "id": g.id,
                "dimensions": {dimension: g.project},
                "metrics": {"amount": {"values": g.values, "quality": g.quality}}
            })
        })
        .collect();
    let items: Vec<Value> = intervals
        .iter()
        .map(|(start, end)| json!({"start": start, "end": end}))
        .collect();
    json!({
        "version": "sidescope-chart-v1",
        "generated_at": "2026-11-20T11:59:50Z",
        "observed_through": null,
        "range": {
            "start": intervals[0].0,
            "end": intervals[intervals.len() - 1].1,
            "timezone": "UTC"
        },
        "intervals": items,
        "metrics": {"amount": metric},
        "series": series,
        "notices": []
    })
    .to_string()
}

fn daily(groups: &[Group]) -> SourceSnapshot {
    snapshot_json(&build_json(&money(), &DAYS, groups, "project"))
}

fn minutes(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            (
                format!("2026-11-20T{:02}:{:02}:00Z", 9 + i / 60, i % 60),
                format!("2026-11-20T{:02}:{:02}:00Z", 9 + (i + 1) / 60, (i + 1) % 60),
            )
        })
        .collect()
}

fn minute_snapshot(spec: &Spec, groups: &[Group], count: usize) -> SourceSnapshot {
    let owned = minutes(count);
    let borrowed: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    snapshot_json(&build_json(spec, &borrowed, groups, "project"))
}

fn palette() -> ChartPalette {
    ChartPalette::from_theme(&THEMES[0])
}

fn render(model: &ChartCardModel, width: i32, selected: Option<usize>) -> ImageSurface {
    let height = model.preferred_plot_height().max(1.0).ceil() as i32;
    let mut surface = ImageSurface::create(Format::ARgb32, width, height).unwrap();
    {
        let cr = Context::new(&surface).unwrap();
        draw_plot(
            &cr,
            model,
            &palette(),
            f64::from(width),
            f64::from(height),
            selected,
        );
    }
    maybe_capture(&mut surface, width);
    surface
}

fn maybe_capture(surface: &mut ImageSurface, width: i32) {
    let Ok(dir) = std::env::var("SIDESCOPE_CHART_CAPTURE_DIR") else {
        return;
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(&dir);
    assert!(path.starts_with(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target")));
    std::fs::create_dir_all(&path).unwrap();
    let test = std::thread::current()
        .name()
        .unwrap_or("test")
        .replace("::", "-");
    let name = format!("{test}-{width}.png");
    let (w, h, data) = pixels(surface);
    std::fs::write(path.join(name), encode_png(w, h, &data)).unwrap();
}

/// Minimal PNG writer (stored deflate blocks): the cairo build has no PNG
/// feature. Cairo's premultiplied BGRA is composited over the card color.
fn encode_png(width: usize, height: usize, bgra: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in 0..height {
        raw.push(0);
        for x in 0..width {
            let px = &bgra[(row * width + x) * 4..(row * width + x) * 4 + 4];
            let background = 30u16;
            for channel in [px[2], px[1], px[0]] {
                raw.push((u16::from(channel) + background * (255 - u16::from(px[3])) / 255) as u8);
            }
        }
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    let mut zlib = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65_535).peekable();
    while let Some(chunk) = chunks.next() {
        zlib.push(u8::from(chunks.peek().is_none()));
        zlib.extend((chunk.len() as u16).to_le_bytes());
        zlib.extend((!(chunk.len() as u16)).to_le_bytes());
        zlib.extend(chunk);
    }
    zlib.extend(((b << 16) | a).to_be_bytes());
    fn crc(bytes: &[u8]) -> u32 {
        let mut value = 0xffff_ffffu32;
        for byte in bytes {
            value ^= u32::from(*byte);
            for _ in 0..8 {
                value = if value & 1 == 1 {
                    (value >> 1) ^ 0xedb8_8320
                } else {
                    value >> 1
                };
            }
        }
        !value
    }
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], body: &[u8]| {
        png.extend((body.len() as u32).to_be_bytes());
        let mut data = kind.to_vec();
        data.extend(body);
        png.extend(&data);
        png.extend(crc(&data).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend((width as u32).to_be_bytes());
    header.extend((height as u32).to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    png
}

fn pixels(surface: &mut ImageSurface) -> (usize, usize, Vec<u8>) {
    surface.flush();
    let (w, h) = (surface.width() as usize, surface.height() as usize);
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let mut out = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        out.extend_from_slice(&data[row * stride..row * stride + w * 4]);
    }
    (w, h, out)
}

fn opaque(rgba: &[u8]) -> bool {
    rgba[3] > 200
}

/// Distinct opaque colors inside a pixel rectangle.
fn colors_in(surface: &mut ImageSurface, x0: usize, x1: usize, y0: usize, y1: usize) -> usize {
    let (w, _, data) = pixels(surface);
    let mut seen = std::collections::BTreeSet::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let px = &data[(y * w + x) * 4..(y * w + x) * 4 + 4];
            if opaque(px) {
                seen.insert([px[0], px[1], px[2]]);
            }
        }
    }
    seen.len()
}

fn opaque_count(surface: &mut ImageSurface, x0: usize, x1: usize, y0: usize, y1: usize) -> usize {
    let (w, _, data) = pixels(surface);
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if opaque(&data[(y * w + x) * 4..(y * w + x) * 4 + 4]) {
                count += 1;
            }
        }
    }
    count
}

fn bars_of(model: &ChartCardModel) -> &[sidescope::chart_view::Bar] {
    match model.plot() {
        PlotModel::Bars { bars, .. } => bars,
        other => panic!("expected bars, got {other:?}"),
    }
}

fn rows_of(model: &ChartCardModel) -> &[sidescope::chart_view::RankedRow] {
    match model.plot() {
        PlotModel::Ranked { rows, .. } => rows,
        other => panic!("expected ranked rows, got {other:?}"),
    }
}

// ---------- banners and freshness ----------

#[test]
fn banners_label_every_status_and_keep_prior_data() {
    let config = card(ChartKind::TimeSeries, "amount");
    let fresh = fixture_snapshot("minimal.json");
    let model = model_at(&config, &fresh);
    assert_eq!(model.banner(), None);

    let stale = build_model(&config, &fresh, ts("2026-11-20T12:05:00Z"));
    assert_eq!(stale.banner(), Some("Stale"));
    assert!(matches!(stale.plot(), PlotModel::Lines { .. }));

    let mut failed = fresh.clone();
    failed.last_error = Some(ChartError::new(ChartErrorKind::Timeout, "took too long"));
    let model = model_at(&config, &failed);
    assert_eq!(model.banner(), Some("Refresh failed"));
    assert!(
        matches!(model.plot(), PlotModel::Lines { .. }),
        "prior data retained"
    );
    assert!(
        model
            .info_lines()
            .iter()
            .any(|line| line.contains("timeout"))
    );
    let failed_stale = build_model(&config, &failed, ts("2026-11-20T12:05:00Z"));
    assert_eq!(failed_stale.banner(), Some("Refresh failed · stale"));

    let mut unavailable = fresh.clone();
    unavailable.dataset = None;
    unavailable.last_error = Some(ChartError::new(ChartErrorKind::MissingExecutable, "x"));
    let model = model_at(&config, &unavailable);
    assert_eq!(
        model.banner(),
        Some("Source unavailable: missing executable")
    );
    assert_eq!(model.plot(), &PlotModel::Empty);
    assert!(model.message().is_some());

    let mut loading = fresh;
    loading.dataset = None;
    let model = model_at(&config, &loading);
    assert_eq!(model.banner(), Some("Loading"));
    assert!(model.message().is_some());
}

// ---------- headlines ----------

#[test]
fn complete_sums_and_last_values_have_honest_headlines() {
    let snapshot = daily(&[complete("A", &["1", "2", "3", "4", "5"])]);
    // The last day is open at NOW, so make a past-only dataset for a complete sum.
    let past = snapshot_json(&build_json(
        &money(),
        &DAYS[..3],
        &[complete("A", &["1", "2", "3"])],
        "project",
    ));
    let headline = model_at(&card(ChartKind::TimeBars, "amount"), &past)
        .headline()
        .cloned()
        .unwrap();
    assert_eq!(headline.label, "Total");
    assert_eq!(headline.value, "6.00 USD");
    assert_eq!(headline.note, None);
    drop(snapshot);

    let gauge = fixture_snapshot("gauge.json");
    let mut config = grouped(card(ChartKind::TimeSeries, "latency"), "service");
    config.summary = Some(Reduction::Last);
    let model = model_at(&config, &gauge);
    assert_eq!(model.headline().unwrap().value, "2 series");
}

#[test]
fn a_single_gauge_headline_says_latest_or_last_known() {
    let spec = Spec {
        quantity: "gauge",
        unit: "ms",
        unknown_contribution: None,
        estimate: false,
    };
    let latest = minute_snapshot(&spec, &[complete("only", &["41", "44.5"])], 2);
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &latest);
    assert_eq!(model.headline().unwrap().label, "Latest");
    assert_eq!(model.headline().unwrap().value, "44.50 ms");

    let last_known = minute_snapshot(
        &spec,
        &[group("only", &[Some("41"), None], &["complete", "missing"])],
        2,
    );
    let headline = model_at(&card(ChartKind::TimeSeries, "amount"), &last_known)
        .headline()
        .cloned()
        .unwrap();
    assert_eq!(headline.label, "Last known");
    assert_eq!(headline.value, "41.00 ms");
    assert!(headline.note.unwrap().starts_with("observed "));
}

#[test]
fn the_open_current_day_is_marked_so_far() {
    let snapshot = daily(&[complete("A", &["1", "2", "3", "4", "5"])]);
    let model = model_at(&card(ChartKind::TimeBars, "amount"), &snapshot);
    let bars = bars_of(&model);
    assert_eq!(bars.iter().filter(|bar| bar.so_far).count(), 1);
    assert!(bars[4].so_far);
    assert_eq!(
        bars[4].quality,
        Quality::Partial,
        "generic downgrade of the open bucket"
    );
    assert!(model.x_labels().last().unwrap().1.ends_with('*'));
    assert!(model.footnote().unwrap().contains("so far"));
    assert!(model.badges().iter().any(|badge| badge == "so far"));
    let headline = model.headline().unwrap();
    assert_eq!(headline.label, "Known amount · incomplete");
    assert!(headline.note.as_ref().unwrap().contains("so far"));
    let last_row = model.table_rows().last().unwrap();
    assert_eq!(last_row[3], "partial · so far");
}

#[test]
fn incomplete_totals_are_never_a_bare_total() {
    let snapshot = fixture_snapshot("minimal.json");
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    let headline = model.headline().unwrap();
    assert_eq!(headline.label, "Known amount · incomplete");
    assert_eq!(headline.value, "0.150 USD");
    assert!(
        !headline.value.starts_with("At least"),
        "producer did not declare nonnegative"
    );
    assert!(model.badges().iter().any(|badge| badge == "incomplete"));
    assert!(model.badges().iter().any(|badge| badge == "estimate"));
}

#[test]
fn at_least_requires_declared_nonnegative_unknowns() {
    let spec = Spec {
        unknown_contribution: Some("nonnegative"),
        ..money()
    };
    let snapshot = minute_snapshot(
        &spec,
        &[group(
            "A",
            &[Some("1"), Some("2")],
            &["complete", "partial"],
        )],
        2,
    );
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    assert_eq!(model.headline().unwrap().value, "At least 3.00 USD");
    assert_eq!(model.headline().unwrap().label, "Known amount · incomplete");
}

#[test]
fn a_missing_cell_makes_the_sum_incomplete_with_its_known_subtotal() {
    let snapshot = minute_snapshot(
        &money(),
        &[group("A", &[Some("1"), None], &["complete", "missing"])],
        2,
    );
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    let headline = model.headline().unwrap();
    assert_eq!(headline.label, "Known amount · incomplete");
    assert_eq!(headline.value, "1.00 USD");
}

// ---------- time series drawing ----------

#[test]
fn a_gap_breaks_the_line_into_segments_without_interpolation() {
    let snapshot = minute_snapshot(
        &money(),
        &[group(
            "A",
            &[Some("1"), Some("2"), None, Some("3"), Some("4")],
            &["complete", "complete", "missing", "complete", "complete"],
        )],
        5,
    );
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    let PlotModel::Lines { series, .. } = model.plot() else {
        panic!("lines expected");
    };
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].segments.len(), 2);
    assert_eq!(series[0].segments[0].len(), 2);
    assert_eq!(series[0].segments[1].len(), 2);
}

#[test]
fn a_gap_between_intervals_also_breaks_the_line() {
    let intervals = [
        ("2026-11-20T09:00:00Z", "2026-11-20T09:01:00Z"),
        ("2026-11-20T09:05:00Z", "2026-11-20T09:06:00Z"),
    ];
    let snapshot = snapshot_json(&build_json(
        &money(),
        &intervals,
        &[complete("A", &["1", "2"])],
        "project",
    ));
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    let PlotModel::Lines { series, .. } = model.plot() else {
        panic!();
    };
    assert_eq!(series[0].segments.len(), 2);
}

#[test]
fn a_single_point_draws_a_mark() {
    let snapshot = minute_snapshot(&money(), &[complete("A", &["2"])], 1);
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    let PlotModel::Lines { series, .. } = model.plot() else {
        panic!();
    };
    assert_eq!(
        series[0].segments,
        vec![vec![series[0].segments[0][0].clone()]]
    );
    let mut surface = render(&model, 640, None);
    let (left, top, width, height) = plot_frame(640.0, model.preferred_plot_height());
    let x = (left + series[0].segments[0][0].x * width) as usize;
    let marks = opaque_count(
        &mut surface,
        x - 4,
        x + 5,
        top as usize,
        (top + height) as usize,
    );
    assert!(
        marks >= 12,
        "a single observation must draw a visible mark, got {marks}"
    );
}

#[test]
fn partial_points_carry_a_hollow_marker_and_identical_colors() {
    let complete_snapshot =
        minute_snapshot(&money(), &[complete("A", &["1", "3", "2", "4", "3"])], 5);
    let partial_snapshot = minute_snapshot(
        &money(),
        &[group(
            "A",
            &[Some("1"), Some("3"), Some("2"), Some("4"), Some("3")],
            &["complete", "complete", "partial", "complete", "complete"],
        )],
        5,
    );
    let config = card(ChartKind::TimeSeries, "amount");
    let (a, b) = (
        model_at(&config, &complete_snapshot),
        model_at(&config, &partial_snapshot),
    );
    let (PlotModel::Lines { series: sa, .. }, PlotModel::Lines { series: sb, .. }) =
        (a.plot(), b.plot())
    else {
        panic!();
    };
    assert_eq!(sa[0].swatch, sb[0].swatch, "marking never recolors");
    assert_eq!(sb[0].segments[0][2].quality, Quality::Partial);
    let (mut image_a, mut image_b) = (render(&a, 640, None), render(&b, 640, None));
    let (pa, pb) = (pixels(&mut image_a).2, pixels(&mut image_b).2);
    assert_ne!(pa, pb, "hollow marker must change pixels");
}

#[test]
fn all_zero_data_draws_a_labeled_baseline_with_a_nonzero_span() {
    let snapshot = minute_snapshot(&money(), &[complete("A", &["0", "0", "0"])], 3);
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    let PlotModel::Lines { axes, .. } = model.plot() else {
        panic!();
    };
    assert!(axes.zero_baseline);
    assert!(axes.scale.max > axes.scale.min);
    assert_eq!(axes.y_labels[0], "0");
    let mut surface = render(&model, 640, None);
    let (left, top, width, height) = plot_frame(640.0, model.preferred_plot_height());
    let bottom = (top + height) as usize;
    let on_baseline = opaque_count(
        &mut surface,
        left as usize + 4,
        (left + width) as usize - 4,
        bottom - 2,
        bottom + 2,
    );
    assert!(on_baseline > 100, "baseline pixels: {on_baseline}");
}

#[test]
fn all_missing_data_draws_no_baseline() {
    let snapshot = fixture_snapshot("all-null.json");
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    assert_eq!(model.plot(), &PlotModel::Empty);
    assert_eq!(model.message(), Some("No known values in this range"));
    let headline = model.headline().unwrap();
    assert_eq!(headline.value, "No known amount");
    let mut surface = ImageSurface::create(Format::ARgb32, 640, 120).unwrap();
    draw_plot(
        &Context::new(&surface).unwrap(),
        &model,
        &palette(),
        640.0,
        120.0,
        None,
    );
    assert_eq!(opaque_count(&mut surface, 0, 640, 0, 120), 0);
}

#[test]
fn empty_series_is_no_observations_not_zero() {
    let model = model_at(
        &card(ChartKind::TimeSeries, "amount"),
        &fixture_snapshot("empty-series.json"),
    );
    assert_eq!(model.message(), Some("No observations"));
    assert_eq!(model.headline(), None);
    assert_eq!(model.plot(), &PlotModel::Empty);
}

#[test]
fn signed_values_include_zero_and_both_extrema() {
    let snapshot = minute_snapshot(&money(), &[complete("A", &["-3", "5", "2"])], 3);
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    let PlotModel::Lines { axes, .. } = model.plot() else {
        panic!();
    };
    assert!(axes.scale.min <= -3.0 && axes.scale.max >= 5.0);
    assert_eq!(
        axes.y_labels[1],
        tick_of((axes.scale.min + axes.scale.max) / 2.0)
    );
}

fn tick_of(value: f64) -> String {
    // Mirrors the compact tick format used by the model for small values.
    let mut text = format!("{value:.1}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    text
}

#[test]
fn three_x_labels_and_at_most_five_legend_entries_at_both_widths() {
    let groups: Vec<Group> = (0..8)
        .map(|i| Group {
            id: Box::leak(format!("g{i}").into_boxed_str()),
            project: Box::leak(format!("Project {i}").into_boxed_str()),
            values: vec![Some("1"); 6],
            quality: vec!["complete"; 6],
        })
        .collect();
    let snapshot = minute_snapshot(&money(), &groups, 6);
    let config = grouped(card(ChartKind::TimeSeries, "amount"), "project");
    let model = model_at(&config, &snapshot);
    assert_eq!(model.x_labels().len(), 3);
    assert_eq!(model.legend().len(), 5);
    assert_eq!(model.legend_overflow(), 3);
    // Both layouts draw the same model; labels must stay inside the surface.
    for width in [640, 378] {
        let mut surface = render(&model, width, None);
        let (w, _, data) = pixels(&mut surface);
        assert_eq!(w, width as usize);
        assert!(data.iter().any(|b| *b != 0));
    }
    let days = daily(&[complete("A", &["1", "2", "3", "4", "5"])]);
    assert_eq!(
        model_at(&card(ChartKind::TimeBars, "amount"), &days)
            .x_labels()
            .len(),
        3
    );
}

// ---------- bars ----------

#[test]
fn partial_bars_carry_hatching_with_the_same_color() {
    let full = snapshot_json(&build_json(
        &money(),
        &DAYS[..3],
        &[complete("A", &["4", "4", "4"])],
        "project",
    ));
    let part = snapshot_json(&build_json(
        &money(),
        &DAYS[..3],
        &[group(
            "A",
            &[Some("4"), Some("4"), Some("4")],
            &["complete", "partial", "complete"],
        )],
        "project",
    ));
    let config = card(ChartKind::TimeBars, "amount");
    let (a, b) = (model_at(&config, &full), model_at(&config, &part));
    assert_eq!(
        bars_of(&a)[1].segments[0].swatch,
        bars_of(&b)[1].segments[0].swatch
    );
    assert_eq!(bars_of(&a)[1].quality, Quality::Complete);
    assert_eq!(bars_of(&b)[1].quality, Quality::Partial);
    let height = a.preferred_plot_height();
    let (left, top, width, plot_h) = plot_frame(640.0, height);
    let bar = &bars_of(&a)[1];
    let (x0, x1) = (
        (left + bar.x0 * width) as usize + 3,
        (left + bar.x1 * width) as usize - 3,
    );
    let (y0, y1) = (
        (top + plot_h * 0.35) as usize,
        (top + plot_h * 0.9) as usize,
    );
    let mut ia = render(&a, 640, None);
    let mut ib = render(&b, 640, None);
    assert_eq!(
        colors_in(&mut ia, x0, x1, y0, y1),
        1,
        "complete bar is one flat color"
    );
    assert!(
        colors_in(&mut ib, x0, x1, y0, y1) >= 2,
        "partial bar must be hatched"
    );
}

#[test]
fn bar_width_follows_real_duration_across_the_dst_day() {
    let snapshot = fixture_snapshot("dst-five-days.json");
    let model = model_at(&card(ChartKind::TimeBars, "amount"), &snapshot);
    let bars = bars_of(&model);
    let widths: Vec<f64> = bars.iter().map(|bar| bar.x1 - bar.x0).collect();
    assert!(
        widths[3] > widths[2] * 1.03,
        "25 h day is wider than a 24 h day: {widths:?}"
    );
    assert!((widths[3] / widths[2] - 25.0 / 24.0).abs() < 1e-9);
}

#[test]
fn table_rows_carry_the_exact_interval_with_offsets_including_the_25_hour_day() {
    let snapshot = fixture_snapshot("dst-five-days.json");
    let model = model_at(&card(ChartKind::TimeBars, "amount"), &snapshot);
    assert_eq!(
        model.table_header(),
        ["Interval", "Group", "Value", "Quality", "Unit"]
    );
    let rows = model.table_rows();
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows[3],
        vec![
            "2026-11-01 00:00:00 -04:00 – 2026-11-02 00:00:00 -05:00",
            "All",
            "4.25",
            "complete",
            "USD"
        ]
    );
    assert_eq!(rows[2][2], "0", "exact zero stays exact");
    assert!(model.selection_readout(3).unwrap().contains("4.25 USD"));
}

#[test]
fn table_values_are_unrounded_decimals() {
    let snapshot = minute_snapshot(&money(), &[complete("A", &["0.1234567", "1.10"])], 2);
    let model = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    assert_eq!(model.table_rows()[0][2], "0.1234567");
    assert_eq!(model.table_rows()[1][2], "1.1");
}

#[test]
fn stacking_allows_nonnegative_interval_totals_and_marks_missing_components() {
    let snapshot = minute_snapshot(
        &money(),
        &[
            complete("A", &["1", "2"]),
            group("B", &[Some("1"), None], &["complete", "missing"]),
        ],
        2,
    );
    let mut config = card(ChartKind::TimeBars, "amount");
    config.stack_by = Some("project".to_string());
    let model = model_at(&config, &snapshot);
    let bars = bars_of(&model);
    assert_eq!(bars[0].segments.len(), 2);
    assert_eq!(bars[0].segments[1].low, 1.0);
    assert_eq!(bars[0].segments[1].high, 2.0);
    assert_eq!(bars[0].quality, Quality::Complete);
    assert_eq!(
        bars[1].quality,
        Quality::Missing,
        "a missing component makes the stack incomplete"
    );
    assert_eq!(bars[1].segments.len(), 1);
    assert_eq!(model.legend().len(), 2);
}

#[test]
fn stacking_is_refused_for_signed_and_gauge_metrics() {
    let mut config = card(ChartKind::TimeBars, "amount");
    config.stack_by = Some("project".to_string());
    let signed = minute_snapshot(&money(), &[complete("A", &["-1", "2"])], 2);
    let model = model_at(&config, &signed);
    assert_eq!(model.plot(), &PlotModel::Empty);
    assert!(model.message().unwrap().contains("Stacking"));

    let spec = Spec {
        quantity: "gauge",
        unit: "ms",
        unknown_contribution: None,
        estimate: false,
    };
    let gauge = minute_snapshot(&spec, &[complete("A", &["1", "2"])], 2);
    let model = model_at(&config, &gauge);
    assert!(model.message().unwrap().contains("Stacking"));
}

// ---------- ranked bars ----------

fn ranked_snapshot() -> SourceSnapshot {
    daily(&[
        complete("Example A", &["1", "1", "1", "1", "1"]),
        complete("Example B", &["2", "2", "2", "2", "2"]),
        complete("Example C", &["3", "3", "3", "3", "3"]),
        complete("Example D", &["4", "4", "4", "4", "4"]),
        complete("Example E", &["5", "5", "5", "5", "5"]),
        complete("Example F", &["6", "6", "6", "6", "6"]),
        group(
            "Example G",
            &[Some("7"), Some("7"), Some("7"), Some("7"), Some("7")],
            &["complete", "complete", "complete", "complete", "partial"],
        ),
        unknown(&["8", "8", "8", "8", "8"]),
    ])
}

fn past_ranked(groups: &[Group]) -> SourceSnapshot {
    snapshot_json(&build_json(&money(), &DAYS[..2], groups, "project"))
}

#[test]
fn ranked_bars_show_top_five_other_and_a_separate_unknown() {
    let model = model_at(
        &grouped(card(ChartKind::RankedBars, "amount"), "project"),
        &ranked_snapshot(),
    );
    let rows = rows_of(&model);
    let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Example G",
            "Example F",
            "Example E",
            "Example D",
            "Example C",
            "Other",
            "Unknown"
        ]
    );
    assert_eq!(rows[5].kind, RowKind::Other);
    assert_eq!(rows[6].kind, RowKind::Unknown);
    // Other = A + B over five days = 15; Unknown stays out of it.
    assert_eq!(rows[5].value, Some(15.0));
    assert_eq!(rows[6].value, Some(40.0));
    assert_eq!(rows[5].full_label, "Other (2 more)");
    // Details keep the full ranking, Other never repeating its members.
    assert_eq!(model.table_rows().len(), 8);
}

#[test]
fn other_aggregates_quality_like_named_bars() {
    let snapshot = past_ranked(&[
        complete("Example A", &["1", "1"]),
        group(
            "Example B",
            &[Some("1"), Some("1")],
            &["complete", "partial"],
        ),
        complete("Example C", &["9", "9"]),
    ]);
    let mut config = grouped(card(ChartKind::RankedBars, "amount"), "project");
    config.top_n = Some(1);
    let model = model_at(&config, &snapshot);
    let rows = rows_of(&model);
    assert_eq!(rows[1].label, "Other");
    assert_eq!(rows[1].quality, Quality::Partial);
    assert!(rows[1].value_text.ends_with('*'), "{}", rows[1].value_text);
    assert_eq!(rows[0].quality, Quality::Complete);
}

#[test]
fn ties_rank_by_stable_id() {
    let snapshot = past_ranked(&[
        complete("Zed", &["2", "2"]),
        complete("Alpha", &["2", "2"]),
        complete("Mid", &["2", "2"]),
    ]);
    let mut config = grouped(card(ChartKind::RankedBars, "amount"), "project");
    config.top_n = Some(2);
    let model = model_at(&config, &snapshot);
    let labels: Vec<&str> = rows_of(&model)
        .iter()
        .map(|row| row.label.as_str())
        .collect();
    assert_eq!(labels, ["Alpha", "Mid", "Other"]);
}

#[test]
fn shares_use_the_known_subtotal_and_are_omitted_for_signed_metrics() {
    let snapshot = past_ranked(&[
        complete("A", &["3", "3"]),
        group("B", &[Some("1"), None], &["complete", "missing"]),
    ]);
    let model = model_at(
        &grouped(card(ChartKind::RankedBars, "amount"), "project"),
        &snapshot,
    );
    let rows = rows_of(&model);
    // Known amounts 6 and 1: shares of the known subtotal 7.
    assert_eq!(rows[0].share.as_deref(), Some("86%"));
    assert_eq!(rows[1].share.as_deref(), Some("14%"));
    assert!(model.footnote().unwrap().contains("known amounts"));

    let signed = past_ranked(&[complete("A", &["3", "-1"]), complete("B", &["-5", "1"])]);
    let model = model_at(
        &grouped(card(ChartKind::RankedBars, "amount"), "project"),
        &signed,
    );
    let rows = rows_of(&model);
    assert_eq!(rows[0].label, "A", "signed values rank descending");
    assert_eq!(rows[0].value, Some(2.0));
    assert_eq!(rows[1].value, Some(-4.0));
    assert!(rows.iter().all(|row| row.share.is_none()));
    let PlotModel::Ranked { scale, .. } = model.plot() else {
        panic!()
    };
    assert!(
        scale.min < 0.0 && scale.max > 0.0,
        "bars extend on both sides of zero"
    );
    // Negative bar is drawn left of the zero line, positive right of it.
    let mut surface = render(&model, 640, None);
    let (w, h, data) = pixels(&mut surface);
    let row_height = h / 2;
    let find = |row: usize| -> Vec<usize> {
        (0..w)
            .filter(|x| {
                let y = row * row_height + row_height / 2;
                let px = &data[(y * w + x) * 4..(y * w + x) * 4 + 4];
                opaque(px)
            })
            .collect()
    };
    assert!(!find(0).is_empty() && !find(1).is_empty());
}

#[test]
fn ranked_gauges_are_never_aggregated() {
    let gauge = fixture_snapshot("gauge.json");
    let mut config = grouped(card(ChartKind::RankedBars, "latency"), "service");
    config.summary = Some(Reduction::Last);
    config.top_n = Some(1);
    let model = model_at(&config, &gauge);
    let rows = rows_of(&model);
    assert_eq!(rows.len(), 1, "no Other for gauges");
    assert_eq!(rows[0].label, "api");
    assert_eq!(model.table_rows().len(), 2, "full ranking stays in details");
    assert_eq!(model.headline(), None);
}

#[test]
fn many_interval_total_series_become_fifteen_plus_other() {
    let model = model_at(
        &grouped(card(ChartKind::TimeSeries, "amount"), "project"),
        &fixture_snapshot("many-series.json"),
    );
    let PlotModel::Lines { series, .. } = model.plot() else {
        panic!();
    };
    assert_eq!(series.len(), 16);
    assert_eq!(series[15].label, "Other");
    assert_eq!(
        model.table_rows().len(),
        20 * 2,
        "details keep every series"
    );
}

#[test]
fn seventeen_gauge_series_are_refused_visibly_naming_the_count() {
    let spec = Spec {
        quantity: "gauge",
        unit: "ms",
        unknown_contribution: None,
        estimate: false,
    };
    let groups: Vec<Group> = (0..17)
        .map(|i| Group {
            id: Box::leak(format!("g{i}").into_boxed_str()),
            project: Box::leak(format!("P{i}").into_boxed_str()),
            values: vec![Some("1"), Some("2")],
            quality: vec!["complete"; 2],
        })
        .collect();
    let snapshot = minute_snapshot(&spec, &groups, 2);
    for kind in [
        ChartKind::TimeSeries,
        ChartKind::TimeBars,
        ChartKind::RankedBars,
    ] {
        let mut config = grouped(card(kind, "amount"), "project");
        config.summary = Some(Reduction::Last);
        if kind == ChartKind::TimeBars {
            config.group_by = vec![];
            config.stack_by = Some("project".to_string());
        }
        let model = model_at(&config, &snapshot);
        assert_eq!(model.plot(), &PlotModel::Empty, "{kind:?}");
        let message = model.message().unwrap();
        assert!(
            message.contains("17") || message.contains("Stacking"),
            "{kind:?}: {message}"
        );
    }
    let mut config = grouped(card(ChartKind::TimeSeries, "amount"), "project");
    config.summary = Some(Reduction::Last);
    assert!(
        model_at(&config, &snapshot)
            .message()
            .unwrap()
            .contains("17")
    );
}

#[test]
fn unrelated_series_of_a_gauge_are_not_summed() {
    let gauge = fixture_snapshot("gauge.json");
    let model = model_at(&card(ChartKind::TimeSeries, "latency"), &gauge);
    assert!(model.message().unwrap().contains("never summed"));
}

// ---------- stable colors and labels ----------

#[test]
fn colors_follow_the_dimension_tuple_not_the_rank() {
    let key = vec![("project".to_string(), "Example A".to_string())];
    assert_eq!(swatch_for(&key), swatch_for(&key));
    assert_eq!(swatch_for(&[]), Swatch::Accent);
    let first = model_at(
        &grouped(card(ChartKind::RankedBars, "amount"), "project"),
        &past_ranked(&[
            complete("Example A", &["9", "9"]),
            complete("Example B", &["1", "1"]),
        ]),
    );
    let second = model_at(
        &grouped(card(ChartKind::RankedBars, "amount"), "project"),
        &past_ranked(&[
            complete("Example A", &["1", "1"]),
            complete("Example B", &["9", "9"]),
        ]),
    );
    let swatch = |model: &ChartCardModel, label: &str| {
        rows_of(model)
            .iter()
            .find(|row| row.label == label)
            .unwrap()
            .swatch
    };
    assert_eq!(swatch(&first, "Example A"), swatch(&second, "Example A"));
    assert_eq!(swatch(&first, "Example B"), swatch(&second, "Example B"));
    assert_eq!(swatch(&first, "Example A"), swatch_for(&key));
    // The same tuple draws the same color on a line chart.
    let lines = model_at(
        &grouped(card(ChartKind::TimeSeries, "amount"), "project"),
        &minute_snapshot(&money(), &[complete("Example A", &["1", "2"])], 2),
    );
    let PlotModel::Lines { series, .. } = lines.plot() else {
        panic!()
    };
    assert_eq!(series[0].swatch, swatch_for(&key));
    let p = palette();
    assert_eq!(p.color(swatch_for(&key)), p.color(swatch_for(&key)));
}

#[test]
fn colliding_shortened_labels_gain_parent_context_but_accessible_text_keeps_full_labels() {
    let snapshot = past_ranked(&[
        complete("/work/one/app", &["2", "2"]),
        complete("/work/two/app", &["1", "1"]),
        complete("/work/solo/tool", &["3", "3"]),
    ]);
    let model = model_at(
        &grouped(card(ChartKind::RankedBars, "amount"), "project"),
        &snapshot,
    );
    let rows = rows_of(&model);
    let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["tool", "one/app", "two/app"]);
    assert_eq!(rows[1].full_label, "/work/one/app");
    assert!(
        model
            .selection_readout(1)
            .unwrap()
            .starts_with("/work/one/app:")
    );
    assert!(
        model
            .table_rows()
            .iter()
            .any(|row| row[1] == "/work/two/app")
    );
}

// ---------- card-level mismatches ----------

#[test]
fn a_card_mismatch_is_card_level_and_leaves_other_cards_unaffected() {
    let snapshot = fixture_snapshot("minimal.json");
    let mut absent_metric = card(ChartKind::TimeSeries, "no-such-metric");
    absent_metric.id = "a".to_string();
    let model = model_at(&absent_metric, &snapshot);
    assert_eq!(model.plot(), &PlotModel::Empty);
    assert!(model.message().unwrap().starts_with("Card mismatch"));

    let absent_dimension = grouped(card(ChartKind::TimeSeries, "amount"), "nope");
    let model = model_at(&absent_dimension, &snapshot);
    assert!(model.message().unwrap().contains("card mismatch"));

    let mut forbidden = card(ChartKind::TimeSeries, "amount");
    forbidden.summary = Some(Reduction::Last);
    let model = model_at(&forbidden, &snapshot);
    assert!(model.message().unwrap().starts_with("Card mismatch"));

    let healthy = model_at(&card(ChartKind::TimeSeries, "amount"), &snapshot);
    assert!(matches!(healthy.plot(), PlotModel::Lines { .. }));
    assert_eq!(healthy.message(), None);
    assert_eq!(healthy.banner(), None);
}

// ---------- info, accessibility, selection ----------

#[test]
fn info_lines_describe_the_source_without_arguments() {
    let model = model_at(
        &card(ChartKind::TimeSeries, "amount"),
        &fixture_snapshot("minimal.json"),
    );
    let lines = model.info_lines().join("\n");
    for expected in [
        "Source: example-metrics",
        "Command: example-chart-data",
        "Last attempt: 2026-11-20 12:00:00 +00:00",
        "Last success: 2026-11-20 12:00:00 +00:00 (0 s ago)",
        "Next retry: 2026-11-20 12:00:30 +00:00",
        "Latency: 42 ms",
        "Source revision: sample-7",
        "Collected through: unknown",
        "Returned range: 2026-11-20 12:00:00 +00:00 – 2026-11-20 12:02:00 +00:00",
        "Notice: Synthetic example; the second interval has an unknown remainder.",
    ] {
        assert!(lines.contains(expected), "missing {expected:?} in\n{lines}");
    }
    assert!(!lines.contains("--from"), "no arguments in Info");
}

#[test]
fn accessible_names_state_kind_title_unit_range_and_freshness() {
    let mut config = card(ChartKind::RankedBars, "amount");
    config.title = Some("By project".to_string());
    let model = model_at(&config, &fixture_snapshot("minimal.json"));
    let name = model.accessible_name();
    for part in [
        "Ranked bars: By project",
        "unit USD",
        "range 2026-11-20 12:00–12:02 UTC",
        "fresh, updated 0 s ago",
    ] {
        assert!(name.contains(part), "{name:?} lacks {part:?}");
    }
    let stale = build_model(
        &config,
        &fixture_snapshot("minimal.json"),
        ts("2026-11-20T12:10:00Z"),
    );
    assert!(stale.accessible_name().contains("stale"));
}

#[test]
fn hit_testing_selects_intervals_and_ranked_rows() {
    let model = model_at(
        &card(ChartKind::TimeBars, "amount"),
        &daily(&[complete("A", &["1", "2", "3", "4", "5"])]),
    );
    let (left, _, width, _) = plot_frame(640.0, model.preferred_plot_height());
    let bars = bars_of(&model);
    let x = left + (bars[2].x0 + bars[2].x1) / 2.0 * width;
    assert_eq!(
        hit_test(&model, 640.0, model.preferred_plot_height(), x, 50.0),
        Some(2)
    );
    assert_eq!(model.selection_count(), 5);

    let ranked = model_at(
        &grouped(card(ChartKind::RankedBars, "amount"), "project"),
        &ranked_snapshot(),
    );
    let height = ranked.preferred_plot_height();
    assert_eq!(hit_test(&ranked, 378.0, height, 10.0, 1.0), Some(0));
    assert_eq!(
        hit_test(&ranked, 378.0, height, 10.0, height - 1.0),
        Some(6)
    );
    assert_eq!(ranked.selection_count(), 7);
    assert_eq!(
        hit_test(
            &model_at(
                &card(ChartKind::TimeSeries, "amount"),
                &fixture_snapshot("empty-series.json")
            ),
            640.0,
            120.0,
            1.0,
            1.0
        ),
        None
    );
}

#[test]
fn the_selected_position_is_visible_in_the_drawing() {
    let model = model_at(
        &card(ChartKind::TimeBars, "amount"),
        &daily(&[complete("A", &["1", "2", "3", "4", "5"])]),
    );
    let (mut plain, mut picked) = (render(&model, 640, None), render(&model, 640, Some(1)));
    assert_ne!(pixels(&mut plain).2, pixels(&mut picked).2);
}

#[test]
fn drawing_is_deterministic_and_fits_both_widths_for_every_kind() {
    let snapshots = [ranked_snapshot(), fixture_snapshot("dst-five-days.json")];
    for snapshot in &snapshots {
        for kind in [
            ChartKind::TimeSeries,
            ChartKind::TimeBars,
            ChartKind::RankedBars,
        ] {
            let config = if kind == ChartKind::RankedBars {
                grouped(card(kind, "amount"), "project")
            } else {
                card(kind, "amount")
            };
            let model = model_at(&config, snapshot);
            for width in [640, 378] {
                let (mut a, mut b) = (
                    render(&model, width, Some(0)),
                    render(&model, width, Some(0)),
                );
                assert_eq!(pixels(&mut a).2, pixels(&mut b).2);
            }
        }
    }
}

#[test]
fn every_theme_yields_a_palette_and_chart_only_css() {
    for theme in THEMES {
        let palette = ChartPalette::from_theme(theme);
        assert!(!palette.font.is_empty());
        let stylesheet = css(theme);
        assert!(stylesheet.contains(".chart-card"));
        for line in stylesheet.lines().filter(|line| line.contains('{')) {
            assert!(line.trim_start().starts_with(".chart-card"), "{line}");
        }
    }
}

#[test]
fn models_are_total_over_the_dataset_limits() {
    let intervals = minutes(120);
    let borrowed: Vec<(&str, &str)> = intervals
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let values: Vec<&str> = vec!["1"; 120];
    let groups: Vec<Group> = (0..40)
        .map(|i| Group {
            id: Box::leak(format!("g{i}").into_boxed_str()),
            project: Box::leak(format!("P{i}").into_boxed_str()),
            values: values.iter().map(|v| Some(*v)).collect(),
            quality: vec!["complete"; 120],
        })
        .collect();
    let snapshot = snapshot_json(&build_json(&money(), &borrowed, &groups, "project"));
    for kind in [
        ChartKind::TimeSeries,
        ChartKind::TimeBars,
        ChartKind::RankedBars,
    ] {
        let mut config = grouped(card(kind, "amount"), "project");
        if kind == ChartKind::TimeBars {
            config.stack_by = Some("project".to_string());
        }
        let model = model_at(&config, &snapshot);
        assert!(model.legend().len() <= 5);
        assert_eq!(
            model.table_rows().len(),
            if kind == ChartKind::RankedBars {
                40
            } else {
                40 * 120
            }
        );
        let _ = render(&model, 640, None);
    }
}

/// Named gallery for visual inspection; PNGs appear only when
/// `SIDESCOPE_CHART_CAPTURE_DIR` is set (under `target/`).
#[test]
fn capture_gallery_renders_every_kind_at_both_widths() {
    let mut stacked = card(ChartKind::TimeBars, "amount");
    stacked.stack_by = Some("project".to_string());
    let stack_data = daily(&[
        complete("Example A", &["1", "2", "3", "2", "1"]),
        group(
            "Example B",
            &[Some("2"), Some("1"), None, Some("2"), Some("2")],
            &["complete", "complete", "missing", "complete", "partial"],
        ),
        complete("Example C", &["1", "1", "1", "1", "1"]),
    ]);
    let signed = past_ranked(&[
        complete("A", &["3", "-1"]),
        complete("B", &["-5", "1"]),
        complete("C", &["1", "1"]),
    ]);
    let series_groups: Vec<Group> = (0..3)
        .map(|i| Group {
            id: Box::leak(format!("g{i}").into_boxed_str()),
            project: Box::leak(format!("Example {i}").into_boxed_str()),
            values: (0..30)
                .map(|n| Some(["1", "3", "2", "5", "4"][(n + i) % 5]))
                .collect(),
            quality: vec!["complete"; 30],
        })
        .collect();
    let mut series_values = series_groups;
    series_values[0].values[10] = None;
    series_values[0].quality[10] = "missing";
    series_values[1].quality[29] = "partial";
    let line_data = minute_snapshot(&money(), &series_values, 30);
    let mut failed = daily(&[complete("Example A", &["1", "2", "3", "4", "5"])]);
    failed.last_error = Some(ChartError::new(ChartErrorKind::Timeout, "x"));
    let cases: Vec<(&str, ChartCardModel)> = vec![
        (
            "time-series-grouped",
            model_at(
                &grouped(card(ChartKind::TimeSeries, "amount"), "project"),
                &line_data,
            ),
        ),
        (
            "daily-bars-so-far-partial",
            model_at(
                &card(ChartKind::TimeBars, "amount"),
                &daily(&[complete("A", &["4", "6", "3", "7", "5"])]),
            ),
        ),
        (
            "daily-bars-dst-25h",
            model_at(
                &card(ChartKind::TimeBars, "amount"),
                &fixture_snapshot("dst-five-days.json"),
            ),
        ),
        ("stacked-bars", model_at(&stacked, &stack_data)),
        (
            "ranked-other-unknown",
            model_at(
                &grouped(card(ChartKind::RankedBars, "amount"), "project"),
                &ranked_snapshot(),
            ),
        ),
        (
            "ranked-signed",
            model_at(
                &grouped(card(ChartKind::RankedBars, "amount"), "project"),
                &signed,
            ),
        ),
        (
            "refresh-failed",
            model_at(&card(ChartKind::TimeBars, "amount"), &failed),
        ),
        (
            "all-zero",
            model_at(
                &card(ChartKind::TimeSeries, "amount"),
                &minute_snapshot(&money(), &[complete("A", &["0", "0", "0"])], 3),
            ),
        ),
    ];
    for (name, model) in &cases {
        for width in [640, 378] {
            let mut surface = render(model, width, None);
            let height = surface.height() as usize;
            assert!(
                opaque_count(&mut surface, 0, width as usize, 0, height) > 0,
                "{name}"
            );
            capture_to(&mut surface, name, width);
        }
    }
}

fn capture_to(surface: &mut ImageSurface, name: &str, width: i32) {
    let Ok(dir) = std::env::var("SIDESCOPE_CHART_CAPTURE_DIR") else {
        return;
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    std::fs::create_dir_all(&path).unwrap();
    let (w, h, data) = pixels(surface);
    std::fs::write(
        path.join(format!("gallery-{name}-{width}.png")),
        encode_png(w, h, &data),
    )
    .unwrap();
}

// ---------- sampled gauges (sidescope-chart-v1.1) and new outcomes ----------

fn sampled_card(kind: ChartKind) -> ChartCardConfig {
    card(kind, "observed_percent")
}

fn sampled_with(replacements: &[(&str, &str)]) -> SourceSnapshot {
    let mut json = fixture("sampled-gauge.json");
    for (old, new) in replacements {
        assert!(json.contains(old), "{old}");
        json = json.replacen(old, new, 1);
    }
    snapshot_json(&json)
}

#[test]
fn a_sampled_gauge_headline_is_a_last_observed_instant_never_current() {
    let model = model_at(
        &sampled_card(ChartKind::TimeSeries),
        &fixture_snapshot("sampled-gauge.json"),
    );
    let headline = model.headline().unwrap();
    assert_eq!(headline.label, "Last observed");
    assert_eq!(headline.value, "13.00%");
    assert_eq!(
        headline.note.as_deref(),
        Some("as of 2026-11-20 11:15:00 +00:00")
    );
    let text = format!("{headline:?}{:?}", model.badges());
    assert!(!text.contains("so far") && !text.contains("Latest") && !text.contains("current"));
}

#[test]
fn a_sampled_headline_marks_an_incomplete_reading_and_a_null_newest() {
    let partial = sampled_with(&[(
        "\"quality\":[\"complete\",\"complete\"]",
        "\"quality\":[\"complete\",\"partial\"]",
    )]);
    let model = model_at(&sampled_card(ChartKind::TimeSeries), &partial);
    assert_eq!(
        model.headline().unwrap().label,
        "Last observed · incomplete"
    );
    let newest_null = sampled_with(&[
        ("\"values\":[\"12.5\",\"13\"]", "\"values\":[\"12.5\",null]"),
        (
            "\"quality\":[\"complete\",\"complete\"]",
            "\"quality\":[\"complete\",\"missing\"]",
        ),
    ]);
    let headline = model_at(&sampled_card(ChartKind::TimeSeries), &newest_null)
        .headline()
        .cloned()
        .unwrap();
    assert_eq!(headline.value, "12.50%");
    assert_eq!(
        headline.note.as_deref(),
        Some("as of 2026-11-20 11:00:00 +00:00 · newest reading was not reported")
    );
}

#[test]
fn sampled_lines_plot_at_the_start_instant_and_split_at_declared_breaks() {
    let model = model_at(
        &sampled_card(ChartKind::TimeSeries),
        &fixture_snapshot("sampled-gauge.json"),
    );
    let PlotModel::Lines { series, axes } = model.plot() else {
        panic!("expected lines");
    };
    // Range 11:00-12:00: the readings are at 11:00 and 11:15, not at midpoints.
    assert_eq!(axes.centers, vec![0.0, 0.25]);
    assert_eq!(series[0].segments.len(), 2, "the break splits the line");
    assert_eq!(series[0].segments[0][0].x, 0.0);
    assert_eq!(series[0].segments[1][0].x, 0.25);
    // Without the break the same readings are one connected segment.
    let joined = sampled_with(&[(",\"breaks\":[1]", "")]);
    let model = model_at(&sampled_card(ChartKind::TimeSeries), &joined);
    let PlotModel::Lines { series, .. } = model.plot() else {
        panic!("expected lines");
    };
    assert_eq!(series[0].segments.len(), 1);
}

#[test]
fn sampled_details_and_readouts_give_the_instant_and_never_so_far() {
    // The request ends before the second interval does; a reading is still complete.
    let model = model_at(
        &sampled_card(ChartKind::TimeSeries),
        &fixture_snapshot("sampled-gauge.json"),
    );
    let rows = model.table_rows();
    assert_eq!(rows[0][0], "2026-11-20 11:00:00 +00:00");
    assert_eq!(rows[1][0], "2026-11-20 11:15:00 +00:00");
    assert_eq!(rows[1][3], "complete");
    assert!(
        model
            .selection_readout(1)
            .unwrap()
            .starts_with("2026-11-20 11:15:00 +00:00")
    );
    assert!(
        model
            .accessible_name()
            .contains("last reading 2026-11-20 11:15:00 +00:00")
    );
}

#[test]
fn a_sampled_metric_cannot_be_drawn_as_time_bars() {
    let model = model_at(
        &sampled_card(ChartKind::TimeBars),
        &fixture_snapshot("sampled-gauge.json"),
    );
    assert!(model.message().unwrap().starts_with("Card mismatch"));
    assert_eq!(model.plot(), &PlotModel::Empty);
}

#[test]
fn series_without_intervals_say_so_instead_of_no_known_values() {
    let json = fixture("minimal.json");
    let value: Value = serde_json::from_str(&json).unwrap();
    let mut empty = value.clone();
    empty["intervals"] = json!([]);
    for series in empty["series"].as_array_mut().unwrap() {
        for metric in series["metrics"].as_object_mut().unwrap().values_mut() {
            metric["values"] = json!([]);
            metric["quality"] = json!([]);
        }
    }
    let snapshot = snapshot_json(&empty.to_string());
    let metric = value["metrics"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    for kind in [
        ChartKind::TimeSeries,
        ChartKind::TimeBars,
        ChartKind::RankedBars,
    ] {
        let model = model_at(&card(kind, &metric), &snapshot);
        assert_eq!(
            model.message(),
            Some("No intervals in this range"),
            "{kind:?}"
        );
    }
}
