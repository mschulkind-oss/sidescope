//! Binary-private glue between the dashboard window and the chart modules:
//! where chart cards sit in the card stacks, how they are updated in place, and
//! the synthetic demo scenarios. Chart cards never start work; the runtime owned
//! by `main` is the only producer of snapshots.

use gtk::prelude::*;
use serde_json::{Value, json};
use sidescope::chart::config::{ChartCardConfig, ChartSourceConfig, resolve_request};
use sidescope::chart::dataset::parse_native_snapshot;
use sidescope::chart::error::{ChartError, ChartErrorKind};
use sidescope::chart::state::{SourceInfo, SourceSnapshot};
use sidescope::chart::time::Timestamp;
use sidescope::chart_view::{
    CardLayout, ChartPalette, build_model, card_widget, css, update_card_widget,
};
use sidescope::theme::Theme;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

const SECTION_NAME: &str = "chart-section";

/// One configured card and the newest snapshot of its source.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartEntry {
    pub card: ChartCardConfig,
    pub snapshot: Option<SourceSnapshot>,
    /// Fixed clock for synthetic scenarios; live cards use the system clock.
    pub now: Option<Timestamp>,
}

/// Marks refresh-button emissions the application itself makes (startup and
/// the quota timer). The scheduler already queries every chart source on its
/// own, so only a user's click may ask it for another query.
#[derive(Default)]
pub struct AutomaticRefresh(std::cell::Cell<bool>);

impl AutomaticRefresh {
    pub fn run(&self, emit: impl FnOnce()) {
        self.0.set(true);
        emit();
        self.0.set(false);
    }

    pub fn requests_chart_refresh(&self) -> bool {
        !self.0.get()
    }
}

/// Charts are polled and shown only in live mode with at least one source.
pub fn starts_chart_runtime(demo: bool, sources: &[ChartSourceConfig]) -> bool {
    !demo && !sources.is_empty()
}

pub fn entries_from_snapshots(
    cards: &[ChartCardConfig],
    snapshots: &[SourceSnapshot],
) -> Vec<ChartEntry> {
    cards
        .iter()
        .map(|card| ChartEntry {
            card: card.clone(),
            snapshot: snapshots
                .iter()
                .find(|snapshot| snapshot.source_id == card.source)
                .cloned(),
            now: None,
        })
        .collect()
}

/// Replaces the snapshot of every card on a changed source.
pub fn apply_snapshots(entries: &mut [ChartEntry], changed: &[SourceSnapshot]) {
    for entry in entries {
        if let Some(snapshot) = changed
            .iter()
            .find(|snapshot| snapshot.source_id == entry.card.source)
        {
            entry.snapshot = Some(snapshot.clone());
        }
    }
}

struct Section {
    widget: gtk::Box,
    cards: Vec<(String, gtk::Widget)>,
    last: Option<(Vec<ChartEntry>, String, i64)>,
}

thread_local! {
    static SECTIONS: RefCell<HashMap<usize, Section>> = RefCell::new(HashMap::new());
    static CHART_CSS: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

/// Loads the chart provider once and keeps it current with the active theme.
pub fn install_css(theme: &Theme) {
    CHART_CSS.with_borrow_mut(|slot| {
        let provider = slot.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &gtk::gdk::Display::default().expect("display"),
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 3,
            );
            provider
        });
        provider.load_from_data(&css(theme));
    });
}

pub fn reload_css(theme: &Theme) {
    if CHART_CSS.with_borrow(Option::is_some) {
        install_css(theme);
    }
}

fn heading(count: usize) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    section.add_css_class("section-heading");
    let title = gtk::Label::new(Some("CHARTS"));
    title.add_css_class("section-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let counter = gtk::Label::new(Some(&format!("{count:02} CARDS")));
    counter.add_css_class("section-count");
    section.append(&title);
    section.append(&counter);
    section
}

/// Brings the chart section of `stack` up to date and places it last. Existing
/// card widgets are updated in place, so an open Details popover and keyboard
/// focus survive; nothing is rebuilt unless the card list changed.
pub fn sync_charts(stack: &gtk::Box, entries: &[ChartEntry], layout: CardLayout, theme: &Theme) {
    let key = stack.as_ptr() as usize;
    if entries.is_empty() {
        SECTIONS.with_borrow_mut(|sections| {
            if let Some(section) = sections.remove(&key)
                && section.widget.parent().is_some()
            {
                stack.remove(&section.widget);
            }
        });
        return;
    }
    let clock = |entry: &ChartEntry| entry.now.unwrap_or_else(Timestamp::now);
    let bucket = entries
        .first()
        .map_or(0, |entry| clock(entry).as_second().div_euclid(5));
    SECTIONS.with_borrow_mut(|sections| {
        let section = sections.entry(key).or_insert_with(|| {
            let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
            widget.set_widget_name(SECTION_NAME);
            Section {
                widget,
                cards: Vec::new(),
                last: None,
            }
        });
        let attached = section.widget.parent().is_some();
        if !attached {
            stack.append(&section.widget);
        } else if stack.last_child().as_ref() != Some(section.widget.upcast_ref()) {
            stack.reorder_child_after(&section.widget, stack.last_child().as_ref());
        }
        if section
            .last
            .as_ref()
            .is_some_and(|(last, id, seen)| last == entries && id == theme.id && *seen == bucket)
        {
            return;
        }
        let palette = ChartPalette::from_theme(theme);
        let models: Vec<_> = entries
            .iter()
            .map(|entry| {
                entry
                    .snapshot
                    .as_ref()
                    .map(|snapshot| build_model(&entry.card, snapshot, clock(entry)))
            })
            .collect();
        let same_cards = section.cards.len() == entries.len()
            && section
                .cards
                .iter()
                .zip(entries)
                .all(|((id, _), entry)| *id == entry.card.id);
        if !same_cards {
            while let Some(child) = section.widget.first_child() {
                section.widget.remove(&child);
            }
            section.widget.append(&heading(entries.len()));
            section.cards.clear();
            for (entry, model) in entries.iter().zip(&models) {
                let widget = model
                    .as_ref()
                    .map(|model| card_widget(model, &palette, layout))
                    .unwrap_or_else(|| gtk::Label::new(Some("Waiting for chart data")).upcast());
                section.widget.append(&widget);
                section.cards.push((entry.card.id.clone(), widget));
            }
        } else {
            for ((_, widget), model) in section.cards.iter().zip(&models) {
                if let Some(model) = model {
                    update_card_widget(widget, model, &palette);
                }
            }
        }
        section.last = Some((entries.to_vec(), theme.id.to_string(), bucket));
    });
}

// ---- synthetic scenarios ---------------------------------------------------

const DEMO_CONFIG: &str = r#"
[[chart_sources]]
id = "recent"
command = ["demo-recent"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_sources]]
id = "daily"
command = ["demo-daily"]
format = "sidescope-chart-v1"
calendar_days = 5
timezone = "America/New_York"

[[chart_sources]]
id = "latency"
command = ["demo-latency"]
format = "sidescope-chart-v1"
window = "3h"
timezone = "UTC"

[[chart_cards]]
id = "demo-spend"
source = "recent"
kind = "time-series"
metric = "amount"
title = "Spend, last 3 hours"
group_by = ["project"]

[[chart_cards]]
id = "demo-daily"
source = "daily"
kind = "time-bars"
metric = "amount"
title = "Spend per day"
stack_by = "project"

[[chart_cards]]
id = "demo-ranking"
source = "daily"
kind = "ranked-bars"
metric = "amount"
title = "Spend by project"
group_by = ["project"]

[[chart_cards]]
id = "demo-latency"
source = "latency"
kind = "time-series"
metric = "latency"
title = "Latency"
group_by = ["service"]
"#;

const LIVE_NOW: &str = "2026-11-20T12:00:00Z";
const DAILY_NOW: &str = "2026-11-02T20:00:00Z";
const DAY_BOUNDS: [&str; 6] = [
    "2026-10-29T04:00:00Z",
    "2026-10-30T04:00:00Z",
    "2026-10-31T04:00:00Z",
    "2026-11-01T04:00:00Z",
    "2026-11-02T05:00:00Z",
    "2026-11-03T05:00:00Z",
];

fn instant(text: &str) -> Timestamp {
    text.parse().expect("scenario instants are valid")
}

/// Minute-spaced boundaries `[start, start + step, ...]` as RFC 3339 text.
fn minute_bounds(count: usize, step_minutes: i64) -> Vec<String> {
    let end = instant(LIVE_NOW).as_second();
    (0..=count as i64)
        .map(|index| {
            let second = end - (count as i64 - index) * step_minutes * 60;
            Timestamp::from_second(second)
                .expect("in range")
                .to_string()
        })
        .collect()
}

fn pairs(bounds: &[String]) -> Vec<(String, String)> {
    bounds
        .windows(2)
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect()
}

struct Line {
    id: &'static str,
    dimension: &'static str,
    label: &'static str,
    values: Vec<Option<String>>,
    quality: Vec<&'static str>,
}

fn line(
    id: &'static str,
    dimension: &'static str,
    label: &'static str,
    values: Vec<Option<String>>,
    partial_last: bool,
) -> Line {
    let mut quality: Vec<&'static str> = values
        .iter()
        .map(|value| {
            if value.is_some() {
                "complete"
            } else {
                "missing"
            }
        })
        .collect();
    if partial_last
        && let Some(last) = quality.last_mut()
        && *last == "complete"
    {
        *last = "partial";
    }
    Line {
        id,
        dimension,
        label,
        values,
        quality,
    }
}

fn native_json(
    intervals: &[(String, String)],
    timezone: &str,
    metric: (&str, &str, &str, &str, bool),
    lines: &[Line],
) -> String {
    let (key, label, unit, quantity, estimate) = metric;
    let series: Vec<Value> = lines
        .iter()
        .map(|line| {
            json!({
                "id": line.id,
                "dimensions": {line.dimension: line.label},
                "metrics": {key: {"values": line.values, "quality": line.quality}}
            })
        })
        .collect();
    json!({
        "version": "sidescope-chart-v1",
        "generated_at": intervals[intervals.len() - 1].1,
        "observed_through": null,
        "revision": "synthetic-scenario",
        "range": {
            "start": intervals[0].0,
            "end": intervals[intervals.len() - 1].1,
            "timezone": timezone
        },
        "intervals": intervals.iter().map(|(start, end)| json!({"start": start, "end": end})).collect::<Vec<_>>(),
        "metrics": {key: {"label": label, "unit": unit, "quantity": quantity, "estimate": estimate}},
        "series": series
    })
    .to_string()
}

fn decimals(count: usize, base: usize, spread: usize, seed: usize) -> Vec<Option<String>> {
    (0..count)
        .map(|index| {
            let cents = base + (index * (7 + seed) + seed * 13) % spread;
            Some(format!("{}.{:02}", cents / 100, cents % 100))
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Complete,
    Partial,
}

struct Scenario {
    recent: String,
    daily: String,
    latency: String,
}

fn scenario_data(shape: Shape) -> Scenario {
    let minute = pairs(&minute_bounds(36, 5));
    let mut alpha = decimals(36, 40, 260, 0);
    let mut beta = decimals(36, 15, 140, 3);
    if shape == Shape::Partial {
        alpha[11] = None;
        alpha[12] = None;
        beta[20] = None;
    }
    let recent = native_json(
        &minute,
        "UTC",
        ("amount", "Estimated spend", "USD", "interval_total", true),
        &[
            line("alpha", "project", "Example A", alpha, true),
            line(
                "beta",
                "project",
                "Example B",
                beta,
                shape == Shape::Partial,
            ),
        ],
    );
    let days: Vec<(String, String)> = DAY_BOUNDS
        .windows(2)
        .map(|pair| (pair[0].to_string(), pair[1].to_string()))
        .collect();
    let mut gamma = decimals(5, 120, 300, 5);
    if shape == Shape::Partial {
        gamma[2] = None;
    }
    let daily = native_json(
        &days,
        "America/New_York",
        ("amount", "Estimated spend", "USD", "interval_total", true),
        &[
            line(
                "alpha",
                "project",
                "Example A",
                decimals(5, 300, 900, 1),
                true,
            ),
            line(
                "beta",
                "project",
                "Example B",
                decimals(5, 150, 500, 2),
                shape == Shape::Partial,
            ),
            line("gamma", "project", "Example C", gamma, false),
        ],
    );
    let latency = native_json(
        &minute,
        "UTC",
        ("latency", "Latency", "ms", "gauge", false),
        &[
            line(
                "api",
                "service",
                "api",
                decimals(36, 2000, 3000, 1)
                    .into_iter()
                    .map(|v| v.map(|s| s.replace('.', "")))
                    .collect(),
                false,
            ),
            line(
                "db",
                "service",
                "db",
                decimals(36, 800, 1500, 4)
                    .into_iter()
                    .map(|v| v.map(|s| s.replace('.', "")))
                    .collect(),
                false,
            ),
        ],
    );
    Scenario {
        recent,
        daily,
        latency,
    }
}

fn many_series_daily() -> String {
    include_str!("../tests/fixtures/charts/native/many-series.json").to_string()
}

fn demo_snapshot(
    source: &ChartSourceConfig,
    json: Option<&str>,
    now: Timestamp,
    last_success: Timestamp,
    error: Option<ChartError>,
) -> SourceSnapshot {
    let request = resolve_request(source, now, 1).expect("demo sources resolve");
    let dataset = json.map(|json| {
        Arc::new(parse_native_snapshot(json, &request).expect("scenario data is valid"))
    });
    let succeeded = dataset.is_some();
    SourceSnapshot {
        source_id: source.id.clone(),
        dataset,
        last_error: error,
        stderr_excerpt: None,
        info: SourceInfo {
            executable: format!("demo-{}", source.id),
            last_attempt: Some(now),
            last_success: succeeded.then_some(last_success),
            next_retry: Some(Timestamp::from_second(now.as_second() + 30).expect("in range")),
            latency_ms: succeeded.then_some(180),
            revision: 1,
            retry_suspended: false,
            request_window: Some(request.window),
            timezone: source.timezone.clone(),
            stale_after_seconds: 90,
        },
    }
}

/// Synthetic chart entries for a `charts-*` visual scenario, never live data.
pub fn demo_entries(scenario: &str) -> Option<Vec<ChartEntry>> {
    let config = sidescope::parse_config(DEMO_CONFIG).expect("demo chart config is valid");
    let now = instant(LIVE_NOW);
    let (shape, kind) = match scenario {
        "charts-full" | "charts-details" => (Shape::Complete, "fresh"),
        "charts-partial" => (Shape::Partial, "fresh"),
        "charts-stale" => (Shape::Complete, "stale"),
        "charts-failed" => (Shape::Complete, "failed"),
        "charts-unavailable" => (Shape::Complete, "unavailable"),
        "charts-dst" => (Shape::Partial, "dst"),
        "charts-many-series" => (Shape::Complete, "many"),
        _ => return None,
    };
    let daily_now = instant(if kind == "many" { LIVE_NOW } else { DAILY_NOW });
    let data = scenario_data(shape);
    let source = |id: &str| {
        config
            .chart_sources
            .iter()
            .find(|source| source.id == id)
            .expect("demo source")
    };
    let error = |kind| Some(ChartError::new(kind, "synthetic scenario failure"));
    let stale_success = Timestamp::from_second(now.as_second() - 900).expect("in range");
    let daily_json = if kind == "many" {
        many_series_daily()
    } else {
        data.daily.clone()
    };
    let (recent, daily, latency) = match kind {
        "stale" => (
            demo_snapshot(
                source("recent"),
                Some(&data.recent),
                now,
                stale_success,
                None,
            ),
            demo_snapshot(
                source("daily"),
                Some(&daily_json),
                daily_now,
                stale_success,
                None,
            ),
            demo_snapshot(
                source("latency"),
                Some(&data.latency),
                now,
                stale_success,
                None,
            ),
        ),
        "failed" => (
            demo_snapshot(
                source("recent"),
                Some(&data.recent),
                now,
                stale_success,
                error(ChartErrorKind::Timeout),
            ),
            demo_snapshot(
                source("daily"),
                Some(&daily_json),
                daily_now,
                now,
                error(ChartErrorKind::NonzeroExit),
            ),
            demo_snapshot(source("latency"), Some(&data.latency), now, now, None),
        ),
        "unavailable" => (
            demo_snapshot(
                source("recent"),
                None,
                now,
                now,
                error(ChartErrorKind::MissingExecutable),
            ),
            demo_snapshot(
                source("daily"),
                None,
                daily_now,
                now,
                error(ChartErrorKind::ParseFailure),
            ),
            demo_snapshot(source("latency"), None, now, now, None),
        ),
        _ => (
            demo_snapshot(source("recent"), Some(&data.recent), now, now, None),
            demo_snapshot(
                source("daily"),
                Some(&daily_json),
                daily_now,
                daily_now,
                None,
            ),
            demo_snapshot(source("latency"), Some(&data.latency), now, now, None),
        ),
    };
    let snapshots = [recent, daily, latency];
    let mut entries = entries_from_snapshots(&config.chart_cards, &snapshots);
    for entry in &mut entries {
        entry.now = Some(if entry.card.source == "daily" {
            daily_now
        } else {
            now
        });
    }
    if kind == "dst" {
        entries.retain(|entry| entry.card.source == "daily");
    }
    if kind == "many" {
        entries.retain(|entry| entry.card.kind == sidescope::chart::config::ChartKind::RankedBars);
    }
    Some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidescope::chart::config::ChartKind;

    #[test]
    fn automatic_refreshes_never_ask_the_scheduler_for_another_query() {
        let automatic = AutomaticRefresh::default();
        assert!(automatic.requests_chart_refresh(), "a user click does");
        let mut seen = None;
        automatic.run(|| seen = Some(automatic.requests_chart_refresh()));
        assert_eq!(seen, Some(false), "startup and timer emissions do not");
        assert!(automatic.requests_chart_refresh(), "the guard is released");
    }

    #[test]
    fn charts_start_only_for_live_mode_with_sources() {
        let config = sidescope::parse_config(DEMO_CONFIG).unwrap();
        assert!(starts_chart_runtime(false, &config.chart_sources));
        assert!(!starts_chart_runtime(true, &config.chart_sources));
        assert!(!starts_chart_runtime(false, &[]));
    }

    #[test]
    fn every_scenario_builds_valid_entries_for_every_card() {
        for scenario in [
            "charts-full",
            "charts-details",
            "charts-partial",
            "charts-stale",
            "charts-failed",
            "charts-unavailable",
            "charts-dst",
            "charts-many-series",
        ] {
            let entries = demo_entries(scenario).unwrap_or_else(|| panic!("{scenario}"));
            assert!(!entries.is_empty(), "{scenario}");
            for entry in &entries {
                let snapshot = entry.snapshot.as_ref().expect("snapshot");
                build_model(&entry.card, snapshot, entry.now.unwrap());
            }
        }
        assert!(demo_entries("charts-unknown").is_none());
        assert!(demo_entries("full").is_none());
    }

    #[test]
    fn scenarios_cover_the_distinguishable_source_states() {
        let status = |name: &str, source: &str| {
            let entries = demo_entries(name).unwrap();
            let entry = entries.iter().find(|e| e.card.source == source).unwrap();
            let snapshot = entry.snapshot.as_ref().unwrap();
            snapshot.status(entry.now.unwrap())
        };
        use sidescope::chart::state::SourceStatus as S;
        assert_eq!(status("charts-full", "recent"), S::Fresh);
        assert_eq!(status("charts-stale", "recent"), S::Stale);
        assert_eq!(
            status("charts-failed", "recent"),
            S::RefreshFailed { stale: true }
        );
        assert_eq!(
            status("charts-failed", "daily"),
            S::RefreshFailed { stale: false }
        );
        assert_eq!(status("charts-unavailable", "recent"), S::Unavailable);
        assert_eq!(status("charts-unavailable", "latency"), S::Loading);
    }

    #[test]
    fn dst_scenario_shows_only_daily_sources_and_many_series_only_rankings() {
        let dst = demo_entries("charts-dst").unwrap();
        assert!(dst.iter().all(|entry| entry.card.source == "daily"));
        let many = demo_entries("charts-many-series").unwrap();
        assert!(
            many.iter()
                .all(|entry| entry.card.kind == ChartKind::RankedBars)
        );
    }

    #[test]
    fn chart_card_ids_never_collide_with_dashboard_card_ids() {
        let config = sidescope::parse_config(DEMO_CONFIG).unwrap();
        for card in &config.chart_cards {
            assert_ne!(
                format!("chart-card-{}", card.id),
                format!("card-{}", card.id)
            );
        }
        for id in ["ai-usage", "cpu", "weather", "air"] {
            assert!(config.chart_cards.iter().all(|card| card.id != id));
        }
    }

    #[test]
    fn apply_snapshots_replaces_only_cards_of_the_changed_source() {
        let mut entries = demo_entries("charts-full").unwrap();
        let before = entries.clone();
        let mut replacement = entries
            .iter()
            .find(|entry| entry.card.source == "latency")
            .and_then(|entry| entry.snapshot.clone())
            .unwrap();
        replacement.info.revision = 99;
        apply_snapshots(&mut entries, std::slice::from_ref(&replacement));
        for (old, new) in before.iter().zip(&entries) {
            if new.card.source == "latency" {
                assert_eq!(new.snapshot.as_ref().unwrap().info.revision, 99);
            } else {
                assert_eq!(old, new);
            }
        }
    }
}
