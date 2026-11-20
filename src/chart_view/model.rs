//! The pure card model. `build_model` is total: every source status and every
//! dataset shape yields a model, and card-level problems become a visible
//! message instead of a failure.

use super::a11y::{accessible_name, age_text, freshness};
use super::format::{
    amount_text, axis_time, day_label, exact_text, full_label, interval_text, join_unit,
    local_text, range_text, share_text, tick_text, unit_text,
};
use super::palette::Swatch;
use super::rank::{Entry, Ranking, rank};
use crate::chart::config::{ChartCardConfig, ChartKind};
use crate::chart::dataset::{
    Cell, ChartDataset, MAX_VISIBLE_SERIES, MetricDescriptor, Quality, Quantity,
    UnknownContribution,
};
use crate::chart::geometry::{ScaleKind, TimeAxis, ValueScale, value_scale};
use crate::chart::reduce::{GroupedSeries, Reduction, Sum, collapse_series, last_known, sum_cells};
use crate::chart::state::{SourceSnapshot, SourceStatus};
use crate::chart::time::{ChartZone, Interval, Timestamp};

pub const TABLE_HEADER: [&str; 5] = ["Interval", "Group", "Value", "Quality", "Unit"];
const MAX_LEGEND_ENTRIES: usize = 5;
const MAX_READOUT_GROUPS: usize = 5;
const DEFAULT_TOP_N: usize = 5;
const PLOT_HEIGHT: f64 = 120.0;
const RANKED_ROW_HEIGHT: f64 = 22.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Headline {
    pub label: String,
    pub value: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegendEntry {
    pub label: String,
    pub full_label: String,
    pub swatch: Swatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlotPoint {
    /// Fraction of the returned range, by real time.
    pub x: f64,
    pub value: f64,
    pub quality: Quality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineSeries {
    pub label: String,
    pub swatch: Swatch,
    /// A gap in the data or between intervals starts a new segment.
    pub segments: Vec<Vec<PlotPoint>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarSegment {
    pub swatch: Swatch,
    pub low: f64,
    pub high: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    /// Index of the interval this bar stands for.
    pub index: usize,
    pub x0: f64,
    pub x1: f64,
    /// Empty when the interval has no known value.
    pub segments: Vec<BarSegment>,
    pub quality: Quality,
    pub so_far: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Named,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankedRow {
    pub label: String,
    pub full_label: String,
    pub swatch: Swatch,
    pub kind: RowKind,
    pub value: Option<f64>,
    pub value_text: String,
    /// Share of the known subtotal; absent for signed metrics.
    pub share: Option<String>,
    pub quality: Quality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Axes {
    pub scale: ValueScale,
    pub y_labels: [String; 3],
    /// Fraction and text; three for compact timelines.
    pub x_labels: Vec<(f64, String)>,
    /// True when every value is zero: a labeled baseline, no trend.
    pub zero_baseline: bool,
    /// Interval centers as range fractions, by interval index.
    pub centers: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlotModel {
    Empty,
    Lines {
        series: Vec<LineSeries>,
        axes: Axes,
    },
    Bars {
        bars: Vec<Bar>,
        axes: Axes,
    },
    Ranked {
        rows: Vec<RankedRow>,
        scale: ValueScale,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartCardModel {
    card_id: String,
    kind: ChartKind,
    title: String,
    subtitle: String,
    headline: Option<Headline>,
    badges: Vec<String>,
    banner: Option<String>,
    message: Option<String>,
    footnote: Option<String>,
    plot: PlotModel,
    legend: Vec<LegendEntry>,
    legend_overflow: usize,
    table_rows: Vec<Vec<String>>,
    readouts: Vec<String>,
    info_lines: Vec<String>,
    accessible_name: String,
}

impl ChartCardModel {
    pub fn card_id(&self) -> &str {
        &self.card_id
    }
    pub fn kind(&self) -> ChartKind {
        self.kind
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn subtitle(&self) -> &str {
        &self.subtitle
    }
    pub fn headline(&self) -> Option<&Headline> {
        self.headline.as_ref()
    }
    pub fn badges(&self) -> &[String] {
        &self.badges
    }
    pub fn accessible_name(&self) -> &str {
        &self.accessible_name
    }
    /// Header: exact interval, group, unrounded value, quality, unit.
    pub fn table_header(&self) -> [&'static str; 5] {
        TABLE_HEADER
    }
    pub fn table_rows(&self) -> &[Vec<String>] {
        &self.table_rows
    }
    pub fn info_lines(&self) -> &[String] {
        &self.info_lines
    }
    /// "Refresh failed", "Stale", "Source unavailable: <reason>", "Loading".
    pub fn banner(&self) -> Option<&str> {
        self.banner.as_deref()
    }
    /// Card-level text shown instead of or beside the plot.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn footnote(&self) -> Option<&str> {
        self.footnote.as_deref()
    }
    pub fn plot(&self) -> &PlotModel {
        &self.plot
    }
    /// At most five named entries; the rest are in the details table.
    pub fn legend(&self) -> &[LegendEntry] {
        &self.legend
    }
    pub fn legend_overflow(&self) -> usize {
        self.legend_overflow
    }
    pub fn x_labels(&self) -> &[(f64, String)] {
        match &self.plot {
            PlotModel::Lines { axes, .. } | PlotModel::Bars { axes, .. } => &axes.x_labels,
            _ => &[],
        }
    }
    /// Number of keyboard-selectable positions: intervals, or ranked rows.
    pub fn selection_count(&self) -> usize {
        self.readouts.len()
    }
    /// Exact interval, group, unrounded value, quality and unit for one position.
    pub fn selection_readout(&self, index: usize) -> Option<&str> {
        self.readouts.get(index).map(String::as_str)
    }
    pub fn preferred_plot_height(&self) -> f64 {
        match &self.plot {
            PlotModel::Ranked { rows, .. } => {
                (rows.len() as f64 * RANKED_ROW_HEIGHT).max(RANKED_ROW_HEIGHT)
            }
            PlotModel::Empty => 0.0,
            _ => PLOT_HEIGHT,
        }
    }
}

#[derive(Default)]
struct Built {
    headline: Option<Headline>,
    plot: Option<PlotModel>,
    legend: Vec<LegendEntry>,
    rows: Vec<Vec<String>>,
    readouts: Vec<String>,
    badges: Vec<String>,
    footnote: Option<String>,
    message: Option<String>,
}

fn refusal(message: impl Into<String>) -> Built {
    Built {
        message: Some(message.into()),
        ..Built::default()
    }
}

fn quality_text(quality: Quality, so_far: bool) -> String {
    let text = match quality {
        Quality::Complete => "complete",
        Quality::Partial => "partial",
        Quality::Missing => "missing",
    };
    if so_far && quality != Quality::Complete {
        format!("{text} · so far")
    } else {
        text.to_string()
    }
}

fn worst(a: Quality, b: Quality) -> Quality {
    a.max(b)
}

pub fn build_model(
    card: &ChartCardConfig,
    snapshot: &SourceSnapshot,
    now: Timestamp,
) -> ChartCardModel {
    let status = snapshot.status(now);
    let dataset = snapshot.dataset.as_deref();
    let metric = dataset.and_then(|dataset| dataset.metric(&card.metric));
    let title = card
        .title
        .clone()
        .or_else(|| metric.map(|metric| metric.label.clone()))
        .unwrap_or_else(|| card.metric.clone());
    let banner = banner_text(snapshot, status);

    let mut built = match (dataset, metric) {
        (None, _) => refusal(match status {
            SourceStatus::Loading => "Waiting for the first answer",
            _ => "No data to show",
        }),
        (Some(_), None) => refusal("Card mismatch: the source does not provide this metric"),
        (Some(dataset), Some(metric)) => {
            build_dataset(card, dataset, metric).unwrap_or_else(refusal)
        }
    };

    let subtitle = match (dataset, metric) {
        (Some(dataset), Some(metric)) => format!(
            "{} · {} · {}",
            metric.label,
            unit_text(&metric.unit),
            range_text(&dataset.range(), dataset.timezone())
        ),
        _ => String::new(),
    };
    if let Some(metric) = metric
        && metric.estimate
        && built.plot.is_some()
    {
        built.badges.insert(0, "estimate".to_string());
    }
    let legend_overflow = built.legend.len().saturating_sub(MAX_LEGEND_ENTRIES);
    built.legend.truncate(MAX_LEGEND_ENTRIES);
    let access = accessible_name(
        card.kind,
        &title,
        metric.map(|metric| unit_text(&metric.unit)),
        dataset,
        &freshness(snapshot, status, now),
    );
    ChartCardModel {
        card_id: card.id.clone(),
        kind: card.kind,
        title,
        subtitle,
        headline: built.headline,
        badges: built.badges,
        banner,
        message: built.message,
        footnote: built.footnote,
        plot: built.plot.unwrap_or(PlotModel::Empty),
        legend: built.legend,
        legend_overflow,
        table_rows: built.rows,
        readouts: built.readouts,
        info_lines: info_lines(snapshot, dataset, now),
        accessible_name: access,
    }
}

fn banner_text(snapshot: &SourceSnapshot, status: SourceStatus) -> Option<String> {
    match status {
        SourceStatus::Fresh => None,
        SourceStatus::Loading => Some("Loading".to_string()),
        SourceStatus::Stale => Some("Stale".to_string()),
        SourceStatus::RefreshFailed { stale: false } => Some("Refresh failed".to_string()),
        SourceStatus::RefreshFailed { stale: true } => Some("Refresh failed · stale".to_string()),
        SourceStatus::Unavailable => Some(format!(
            "Source unavailable: {}",
            snapshot
                .last_error
                .as_ref()
                .map_or("unknown reason", |error| error.kind.label())
        )),
    }
}

fn info_lines(
    snapshot: &SourceSnapshot,
    dataset: Option<&ChartDataset>,
    now: Timestamp,
) -> Vec<String> {
    let info = &snapshot.info;
    let zone = dataset
        .map(|dataset| dataset.timezone().clone())
        .or_else(|| ChartZone::parse(&info.timezone).ok())
        .or_else(|| ChartZone::parse("UTC").ok());
    let when = |instant: Option<Timestamp>| match (instant, &zone) {
        (Some(instant), Some(zone)) => local_text(instant, zone),
        _ => "never".to_string(),
    };
    let mut lines = vec![
        format!("Source: {}", snapshot.source_id),
        format!("Command: {}", info.executable),
        format!("Last attempt: {}", when(info.last_attempt)),
        format!(
            "Last success: {}{}",
            when(info.last_success),
            info.last_success
                .map(|success| format!(" ({})", age_text(now.as_second() - success.as_second())))
                .unwrap_or_default()
        ),
    ];
    lines.push(if info.retry_suspended {
        "Next retry: suspended until settings change or a retry is requested".to_string()
    } else {
        match info.next_retry {
            Some(retry) => format!("Next retry: {}", when(Some(retry))),
            None => "Next retry: not scheduled".to_string(),
        }
    });
    lines.push(match info.latency_ms {
        Some(ms) => format!("Latency: {ms} ms"),
        None => "Latency: unknown".to_string(),
    });
    lines.push(format!("Request revision: {}", info.revision));
    lines.push(format!("Stale after: {} s", info.stale_after_seconds));
    if let (Some(window), Some(zone)) = (&info.request_window, &zone) {
        lines.push(format!("Requested window: {}", interval_text(window, zone)));
    }
    if let Some(dataset) = dataset {
        lines.push(format!(
            "Returned range: {}",
            interval_text(&dataset.range(), dataset.timezone())
        ));
        lines.push(format!("Time zone: {}", dataset.timezone().name()));
        lines.push(format!(
            "Answer generated: {}",
            local_text(dataset.generated_at(), dataset.timezone())
        ));
        lines.push(match dataset.observed_through() {
            Some(through) => format!(
                "Collected through: {}",
                local_text(through, dataset.timezone())
            ),
            None => "Collected through: unknown".to_string(),
        });
        lines.push(match dataset.revision() {
            Some(revision) => format!("Source revision: {revision}"),
            None => "Source revision: none".to_string(),
        });
        for notice in dataset.notices() {
            lines.push(format!("Notice: {notice}"));
        }
    }
    if let Some(error) = &snapshot.last_error {
        lines.push(format!("Last error: {error}"));
    }
    if let Some(excerpt) = &snapshot.stderr_excerpt {
        lines.push(format!("Diagnostics: {excerpt}"));
    }
    lines
}

fn collapse(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    dims: &[String],
) -> Result<Vec<GroupedSeries>, String> {
    let groups = collapse_series(dataset, &metric.id, dims)
        .map_err(|error| format!("{}: {}", error.kind.label(), error.message()))?;
    if metric.quantity == Quantity::Gauge && groups.len() < dataset.series().len() {
        return Err(
            "Gauge series are never summed; group by a dimension that separates them".to_string(),
        );
    }
    Ok(groups)
}

fn unknown_count(groups: &[GroupedSeries]) -> usize {
    groups
        .iter()
        .filter(|group| group.key.iter().any(|(_, value)| value.is_empty()))
        .count()
}

fn so_far(dataset: &ChartDataset, interval: &Interval) -> bool {
    interval.end > dataset.request_window().end
}

fn has_negative(groups: &[GroupedSeries]) -> bool {
    groups
        .iter()
        .flat_map(|group| &group.cells)
        .any(|cell| cell.value.as_ref().is_some_and(|value| value.is_negative()))
}

fn scale_kind(card: &ChartCardConfig, negative: bool) -> ScaleKind {
    if card.bounded_percent {
        ScaleKind::Percent0To100
    } else if negative {
        ScaleKind::Signed
    } else {
        ScaleKind::Nonnegative
    }
}

fn build_dataset(
    card: &ChartCardConfig,
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
) -> Result<Built, String> {
    let summary = card.summary.unwrap_or(match metric.quantity {
        Quantity::IntervalTotal => Reduction::Sum,
        Quantity::Gauge => Reduction::Last,
    });
    if !summary.permitted(metric.quantity) {
        return Err("Card mismatch: this metric does not support that summary".to_string());
    }
    if card.bounded_percent && metric.unit != "percent" {
        return Err("Card mismatch: bounded_percent needs a percent metric".to_string());
    }
    if dataset.series().is_empty() {
        return Ok(refusal("No observations"));
    }
    match card.kind {
        ChartKind::TimeSeries => build_lines(card, dataset, metric),
        ChartKind::TimeBars => build_bars(card, dataset, metric),
        ChartKind::RankedBars => build_ranked(card, dataset, metric, summary),
    }
}

fn sum_headline(sum: &Sum, metric: &MetricDescriptor, note: Option<String>) -> Headline {
    if sum.quality == Quality::Complete && sum.known.is_some() {
        return Headline {
            label: "Total".to_string(),
            value: sum
                .known
                .as_ref()
                .map_or_else(String::new, |known| amount_text(known, &metric.unit)),
            note,
        };
    }
    let value = match &sum.known {
        Some(known) if metric.unknown_contribution == UnknownContribution::Nonnegative => {
            format!("At least {}", amount_text(known, &metric.unit))
        }
        Some(known) => amount_text(known, &metric.unit),
        None => "No known amount".to_string(),
    };
    Headline {
        label: "Known amount · incomplete".to_string(),
        value,
        note,
    }
}

fn gauge_headline(
    cells: &[Cell],
    intervals: &[Interval],
    zone: &ChartZone,
    metric: &MetricDescriptor,
) -> Headline {
    match last_known(cells) {
        Some(last) => {
            let mut label = if last.is_latest {
                "Latest"
            } else {
                "Last known"
            }
            .to_string();
            if last.quality != Quality::Complete {
                label.push_str(" · incomplete");
            }
            Headline {
                label,
                value: amount_text(&last.value, &metric.unit),
                note: (!last.is_latest)
                    .then(|| format!("observed {}", interval_text(&intervals[last.index], zone))),
            }
        }
        None => Headline {
            label: "Latest".to_string(),
            value: "No known value".to_string(),
            note: None,
        },
    }
}

/// Header for time charts: sum over all groups, or the single gauge's last.
fn time_headline(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    total: Option<&GroupedSeries>,
    group_count: usize,
) -> Result<Option<Headline>, String> {
    let Some(total) = total else {
        return Ok(None);
    };
    if metric.quantity == Quantity::Gauge {
        if group_count != 1 {
            return Ok(Some(Headline {
                label: "Series".to_string(),
                value: format!("{group_count} series"),
                note: None,
            }));
        }
        return Ok(Some(gauge_headline(
            &total.cells,
            dataset.intervals(),
            dataset.timezone(),
            metric,
        )));
    }
    let sum = sum_cells(&total.cells)
        .map_err(|_| "Totals exceed the supported numeric range".to_string())?;
    let current = dataset
        .intervals()
        .last()
        .is_some_and(|interval| so_far(dataset, interval))
        && total.cells.last().is_some_and(|cell| cell.value.is_some());
    Ok(Some(sum_headline(
        &sum,
        metric,
        current.then(|| "includes the current interval · so far".to_string()),
    )))
}

fn legend_of(entries: &[Entry]) -> Vec<LegendEntry> {
    if entries.len() == 1 && entries[0].key.is_empty() {
        return Vec::new();
    }
    entries
        .iter()
        .map(|entry| LegendEntry {
            label: entry.label.clone(),
            full_label: entry.full_label.clone(),
            swatch: entry.swatch,
        })
        .collect()
}

fn named_limit(groups: &[GroupedSeries]) -> Result<Option<usize>, String> {
    if groups.len() <= MAX_VISIBLE_SERIES {
        return Ok(None);
    }
    let unknown = unknown_count(groups);
    MAX_VISIBLE_SERIES
        .checked_sub(unknown.saturating_add(1))
        .map(Some)
        .ok_or_else(|| {
            format!(
                "Too many Unknown series: {unknown} groups with an empty value exceed the {MAX_VISIBLE_SERIES} the plot can show; every row is in Details"
            )
        })
}

fn capacity_refusal(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    groups: &[GroupedSeries],
    message: String,
) -> Built {
    Built {
        message: Some(message),
        rows: time_rows(dataset, metric, groups),
        ..Built::default()
    }
}

fn too_many(count: usize) -> String {
    format!("Too many series: {count} gauge series cannot be combined; group by fewer dimensions")
}

fn time_rows(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    groups: &[GroupedSeries],
) -> Vec<Vec<String>> {
    let mut rows = Vec::with_capacity(groups.len() * dataset.intervals().len());
    for group in groups {
        let label = full_label(&group.key);
        for (interval, cell) in dataset.intervals().iter().zip(&group.cells) {
            rows.push(vec![
                interval_text(interval, dataset.timezone()),
                label.clone(),
                cell.value
                    .as_ref()
                    .map_or_else(|| "no value".to_string(), exact_text),
                quality_text(cell.quality, so_far(dataset, interval)),
                unit_text(&metric.unit).to_string(),
            ]);
        }
    }
    rows
}

fn badges_for(groups: &[GroupedSeries], current: bool) -> Vec<String> {
    let mut badges = Vec::new();
    if groups
        .iter()
        .flat_map(|group| &group.cells)
        .any(|cell| cell.quality != Quality::Complete)
    {
        badges.push("incomplete".to_string());
    }
    if current {
        badges.push("so far".to_string());
    }
    badges
}

fn axes_for(
    dataset: &ChartDataset,
    scale: ValueScale,
    values: &[f64],
    x_labels: Vec<(f64, String)>,
) -> Axes {
    let axis = TimeAxis::new(dataset.range());
    Axes {
        scale,
        y_labels: scale.ticks().map(tick_text),
        x_labels,
        zero_baseline: values.iter().all(|value| *value == 0.0),
        centers: dataset
            .intervals()
            .iter()
            .map(|interval| axis.center(interval))
            .collect(),
    }
}

fn line_x_labels(dataset: &ChartDataset) -> Vec<(f64, String)> {
    let range = dataset.range();
    let span = range.seconds();
    let middle = Timestamp::from_second(range.start.as_second() + span / 2).unwrap_or(range.start);
    [(0.0, range.start), (0.5, middle), (1.0, range.end)]
        .into_iter()
        .map(|(x, instant)| (x, axis_time(instant, dataset.timezone(), span)))
        .collect()
}

fn selection_readouts(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    shown: &[Entry],
) -> Vec<String> {
    dataset
        .intervals()
        .iter()
        .enumerate()
        .map(|(index, interval)| {
            let mut parts = vec![interval_text(interval, dataset.timezone())];
            for entry in shown.iter().take(MAX_READOUT_GROUPS) {
                let cell = &entry.cells[index];
                let value = cell.value.as_ref().map_or_else(
                    || "no value".to_string(),
                    |value| join_unit(&exact_text(value), &metric.unit),
                );
                let group = if entry.key.is_empty() && shown.len() == 1 {
                    String::new()
                } else {
                    format!("{}: ", entry.full_label)
                };
                parts.push(format!(
                    "{group}{value} · {}",
                    quality_text(cell.quality, so_far(dataset, interval))
                ));
            }
            if shown.len() > MAX_READOUT_GROUPS {
                parts.push(format!(
                    "+{} more in Details",
                    shown.len() - MAX_READOUT_GROUPS
                ));
            }
            parts.join(" · ")
        })
        .collect()
}

fn build_lines(
    card: &ChartCardConfig,
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
) -> Result<Built, String> {
    let groups = collapse(dataset, metric, &card.group_by)?;
    let limit = match named_limit(&groups) {
        Ok(limit) => limit,
        Err(message) => return Ok(capacity_refusal(dataset, metric, &groups, message)),
    };
    if limit.is_some() && metric.quantity == Quantity::Gauge {
        return Err(too_many(groups.len()));
    }
    let reduction = if metric.quantity == Quantity::Gauge {
        Reduction::Last
    } else {
        Reduction::Sum
    };
    let ranking = rank(groups.clone(), reduction, limit)?;
    let total = if card.group_by.is_empty() || metric.quantity == Quantity::Gauge {
        groups.first().cloned()
    } else {
        collapse(dataset, metric, &[])?.into_iter().next()
    };
    let headline = time_headline(dataset, metric, total.as_ref(), groups.len())?;
    let axis = TimeAxis::new(dataset.range());
    let intervals = dataset.intervals();
    let mut values = Vec::new();
    let series: Vec<LineSeries> = ranking
        .shown
        .iter()
        .map(|entry| {
            let mut segments: Vec<Vec<PlotPoint>> = Vec::new();
            let mut open = false;
            for (index, cell) in entry.cells.iter().enumerate() {
                let contiguous = index > 0 && intervals[index - 1].end == intervals[index].start;
                match &cell.value {
                    Some(value) => {
                        let value = value.to_f64();
                        values.push(value);
                        if !(open && contiguous) {
                            segments.push(Vec::new());
                        }
                        segments
                            .last_mut()
                            .expect("segment exists")
                            .push(PlotPoint {
                                x: axis.center(&intervals[index]),
                                value,
                                quality: cell.quality,
                            });
                        open = true;
                    }
                    None => open = false,
                }
            }
            LineSeries {
                label: entry.label.clone(),
                swatch: entry.swatch,
                segments,
            }
        })
        .collect();
    let current = headline.as_ref().is_some_and(|headline| {
        headline
            .note
            .as_deref()
            .is_some_and(|n| n.contains("so far"))
    });
    let kind = scale_kind(card, groups_negative(&groups));
    let Some(scale) = value_scale(&values, kind) else {
        return Ok(Built {
            headline,
            message: Some("No known values in this range".to_string()),
            rows: time_rows(dataset, metric, &groups),
            badges: badges_for(&groups, current),
            ..Built::default()
        });
    };
    let axes = axes_for(dataset, scale, &values, line_x_labels(dataset));
    Ok(Built {
        headline,
        plot: Some(PlotModel::Lines { series, axes }),
        legend: legend_of(&ranking.shown),
        rows: time_rows(dataset, metric, &groups),
        readouts: selection_readouts(dataset, metric, &ranking.shown),
        badges: badges_for(&groups, current),
        footnote: None,
        message: None,
    })
}

fn groups_negative(groups: &[GroupedSeries]) -> bool {
    has_negative(groups)
}

fn bar_x_labels(dataset: &ChartDataset, bars: &[Bar]) -> Vec<(f64, String)> {
    let intervals = dataset.intervals();
    let zone = dataset.timezone();
    let daily = intervals
        .iter()
        .all(|interval| interval.seconds() >= 20 * 3_600);
    let picks: Vec<usize> = if intervals.len() <= 3 {
        (0..intervals.len()).collect()
    } else {
        vec![0, intervals.len() / 2, intervals.len() - 1]
    };
    picks
        .into_iter()
        .map(|index| {
            let interval = &intervals[index];
            let mut label = if daily {
                day_label(interval.start, zone)
            } else {
                axis_time(interval.start, zone, dataset.range().seconds())
            };
            if bars[index].so_far {
                label.push('*');
            }
            (((bars[index].x0 + bars[index].x1) / 2.0), label)
        })
        .collect()
}

fn build_bars(
    card: &ChartCardConfig,
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
) -> Result<Built, String> {
    let stack_dims: Vec<String> = card.stack_by.iter().cloned().collect();
    if !card.group_by.is_empty() && card.group_by != stack_dims {
        return Err(
            "Card mismatch: time bars stack by one dimension; use stack_by, group_by is not supported"
                .to_string(),
        );
    }
    let groups = collapse(dataset, metric, &stack_dims)?;
    if card.stack_by.is_some() && metric.quantity != Quantity::IntervalTotal {
        return Err("Stacking is only available for nonnegative interval totals".to_string());
    }
    let negative = has_negative(&groups);
    if card.stack_by.is_some() && negative {
        return Err("Stacking is only available for nonnegative interval totals".to_string());
    }
    let limit = match named_limit(&groups) {
        Ok(limit) => limit,
        Err(message) => return Ok(capacity_refusal(dataset, metric, &groups, message)),
    };
    if limit.is_some() && metric.quantity == Quantity::Gauge {
        return Err(too_many(groups.len()));
    }
    let reduction = if metric.quantity == Quantity::Gauge {
        Reduction::Last
    } else {
        Reduction::Sum
    };
    let ranking = rank(groups.clone(), reduction, limit)?;
    let total = if stack_dims.is_empty() || metric.quantity == Quantity::Gauge {
        groups.first().cloned()
    } else {
        collapse(dataset, metric, &[])?.into_iter().next()
    };
    let headline = time_headline(dataset, metric, total.as_ref(), groups.len())?;
    let axis = TimeAxis::new(dataset.range());
    let mut totals = Vec::new();
    let bars: Vec<Bar> = dataset
        .intervals()
        .iter()
        .enumerate()
        .map(|(index, interval)| {
            let (x0, x1) = axis.span(interval);
            let mut segments = Vec::new();
            let mut quality = Quality::Complete;
            let mut level = 0.0;
            let mut any = false;
            for entry in &ranking.shown {
                let cell = &entry.cells[index];
                quality = worst(quality, cell.quality);
                let Some(value) = cell.value.as_ref().map(|value| value.to_f64()) else {
                    continue;
                };
                any = true;
                if stack_dims.is_empty() {
                    segments.push(BarSegment {
                        swatch: entry.swatch,
                        low: 0.0,
                        high: value,
                    });
                    totals.push(value);
                } else {
                    segments.push(BarSegment {
                        swatch: entry.swatch,
                        low: level,
                        high: level + value,
                    });
                    level += value;
                }
            }
            if !stack_dims.is_empty() && any {
                totals.push(level);
            }
            if !any {
                segments.clear();
            }
            Bar {
                index,
                x0,
                x1,
                segments,
                quality,
                so_far: so_far(dataset, interval),
            }
        })
        .collect();
    let current = bars
        .last()
        .is_some_and(|bar| bar.so_far && !bar.segments.is_empty());
    let table_dims: Vec<String> =
        card.group_by
            .iter()
            .chain(card.stack_by.iter())
            .fold(Vec::new(), |mut dims, name| {
                if !dims.contains(name) {
                    dims.push(name.clone());
                }
                dims
            });
    let table_groups = collapse(dataset, metric, &table_dims)?;
    let kind = scale_kind(card, negative);
    let Some(scale) = value_scale(&totals, kind) else {
        return Ok(Built {
            headline,
            message: Some("No known values in this range".to_string()),
            rows: time_rows(dataset, metric, &table_groups),
            badges: badges_for(&groups, current),
            ..Built::default()
        });
    };
    let x_labels = bar_x_labels(dataset, &bars);
    let axes = axes_for(dataset, scale, &totals, x_labels);
    Ok(Built {
        headline,
        plot: Some(PlotModel::Bars { bars, axes }),
        legend: if stack_dims.is_empty() {
            Vec::new()
        } else {
            legend_of(&ranking.shown)
        },
        rows: time_rows(dataset, metric, &table_groups),
        readouts: selection_readouts(dataset, metric, &ranking.shown),
        badges: badges_for(&groups, current),
        footnote: current.then(|| "* so far: the current interval is still open".to_string()),
        message: None,
    })
}

fn is_stale_observation(entry: &Entry) -> bool {
    matches!(entry.observed, Some((_, false)))
}

fn build_ranked(
    card: &ChartCardConfig,
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    summary: Reduction,
) -> Result<Built, String> {
    let groups = collapse(dataset, metric, &card.group_by)?;
    if summary == Reduction::Last && groups.len() > MAX_VISIBLE_SERIES {
        return Err(too_many(groups.len()));
    }
    let top_n = card.top_n.map_or(DEFAULT_TOP_N, usize::from);
    // Every visible row, Other and Unknown included, must fit the plot: the
    // named limit reserves a slot for Other and for each Unknown group.
    let (limit, refusal) = match named_limit(&groups) {
        Ok(capacity) => (capacity.map_or(top_n, |capacity| capacity.min(top_n)), None),
        Err(message) => (top_n, Some(message)),
    };
    let ranking: Ranking = rank(groups.clone(), summary, Some(limit))?;
    let negative = has_negative(&groups);
    let pseudo: Vec<Cell> = ranking
        .shown
        .iter()
        .map(|entry| Cell {
            value: entry.value.clone(),
            quality: entry.quality,
        })
        .collect();
    let subtotal =
        sum_cells(&pseudo).map_err(|_| "Totals exceed the supported numeric range".to_string())?;
    let share_base = (!negative && summary == Reduction::Sum)
        .then(|| subtotal.known.as_ref().map(|known| known.to_f64()))
        .flatten()
        .filter(|base| *base > 0.0);
    let current = dataset
        .intervals()
        .last()
        .is_some_and(|interval| so_far(dataset, interval));
    let rows: Vec<RankedRow> = ranking
        .shown
        .iter()
        .map(|entry| RankedRow {
            label: entry.label.clone(),
            full_label: entry.full_label.clone(),
            swatch: entry.swatch,
            kind: entry.kind,
            value: entry.value.as_ref().map(|value| value.to_f64()),
            value_text: match &entry.value {
                Some(value) => format!(
                    "{}{}{}",
                    amount_text(value, &metric.unit),
                    if entry.quality == Quality::Complete {
                        ""
                    } else {
                        "*"
                    },
                    if is_stale_observation(entry) {
                        " · last known"
                    } else {
                        ""
                    }
                ),
                None => "no known value".to_string(),
            },
            share: share_base.and_then(|base| {
                entry
                    .value
                    .as_ref()
                    .map(|value| share_text(value.to_f64() / base))
            }),
            quality: entry.quality,
        })
        .collect();
    let values: Vec<f64> = rows.iter().filter_map(|row| row.value).collect();
    let kind = scale_kind(card, negative);
    let range_label = interval_text(&dataset.range(), dataset.timezone());
    let table_rows = ranking
        .full
        .iter()
        .map(|entry| {
            // A gauge row shows the interval it was actually observed in.
            let interval = match entry.observed {
                Some((index, _)) => interval_text(&dataset.intervals()[index], dataset.timezone()),
                None => range_label.clone(),
            };
            let mut quality = quality_text(entry.quality, summary == Reduction::Sum && current);
            if is_stale_observation(entry) {
                quality.push_str(" · last known");
            }
            vec![
                interval,
                entry.full_label.clone(),
                entry
                    .value
                    .as_ref()
                    .map_or_else(|| "no value".to_string(), exact_text),
                quality,
                unit_text(&metric.unit).to_string(),
            ]
        })
        .collect();
    if let Some(message) = refusal {
        return Ok(Built {
            message: Some(message),
            rows: table_rows,
            ..Built::default()
        });
    }
    let readouts = rows
        .iter()
        .zip(&ranking.shown)
        .map(|(row, entry)| {
            let value = entry.value.as_ref().map_or_else(
                || "no known value".to_string(),
                |value| join_unit(&exact_text(value), &metric.unit),
            );
            let share = row
                .share
                .as_ref()
                .map(|share| format!(" · {share} of known subtotal"))
                .unwrap_or_default();
            format!(
                "{}: {value} · {}{}{share}",
                entry.full_label,
                quality_text(entry.quality, false),
                match entry.observed {
                    Some((index, false)) => format!(
                        " · last known, observed {}",
                        interval_text(&dataset.intervals()[index], dataset.timezone())
                    ),
                    _ => String::new(),
                }
            )
        })
        .collect();
    let headline = (summary == Reduction::Sum).then(|| {
        let mut headline = sum_headline(&subtotal, metric, None);
        if headline.label == "Total" {
            headline.label = "Known subtotal".to_string();
        }
        headline
    });
    let mut notes = Vec::new();
    if rows.iter().any(|row| row.quality != Quality::Complete) {
        notes.push("* incomplete: known amount only");
    }
    if share_base.is_some() {
        notes.push("shares use known amounts, not unknown remainders");
    }
    let footnote = (!notes.is_empty()).then(|| notes.join(" · "));
    let Some(scale) = value_scale(&values, kind) else {
        return Ok(Built {
            headline,
            message: Some("No known values in this range".to_string()),
            rows: table_rows,
            ..Built::default()
        });
    };
    Ok(Built {
        headline,
        plot: Some(PlotModel::Ranked { rows, scale }),
        legend: Vec::new(),
        rows: table_rows,
        readouts,
        badges: badges_for(&groups, current && summary == Reduction::Sum),
        footnote,
        message: None,
    })
}
