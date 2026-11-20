//! The pure card model. `build_model` is total: every source status and every
//! dataset shape yields a model, and card-level problems become a visible
//! message instead of a failure.

use super::a11y::{accessible_name, age_text, freshness};
use super::format::{
    amount_text, axis_time, day_label, exact_text, full_label, instant_text, interval_text,
    join_unit, local_text, range_text, share_text, tick_text, unit_text,
};
use super::palette::Swatch;
use super::rank::{Entry, shown_entries};
use crate::chart::config::{ChartCardConfig, ChartKind};
use crate::chart::dataset::{
    ChartDataset, MAX_VISIBLE_SERIES, MetricDescriptor, Quality, Quantity, UnknownContribution,
};
use crate::chart::decimal::ChartDecimal;
use crate::chart::facts::{
    CapacityLimit, CapacityRefusal, CardFacts, DataOrigin, DetailGrain, MAX_LEGEND_ENTRIES,
    Mismatch, MismatchSubject, Outcome, Refusal, Standing, SummaryFacts, reduce_card,
};
use crate::chart::geometry::{ScaleKind, TimeAxis, ValueScale, value_scale};
use crate::chart::reduce::Reduction;
use crate::chart::state::{SourceSnapshot, SourceStatus};
use crate::chart::time::{ChartZone, Interval, Timestamp};

pub const TABLE_HEADER: [&str; 5] = ["Interval", "Group", "Value", "Quality", "Unit"];
const MAX_READOUT_GROUPS: usize = 5;
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

/// The live model of one card: `present_card` over the card's facts.
pub fn build_model(
    card: &ChartCardConfig,
    snapshot: &SourceSnapshot,
    now: Timestamp,
) -> ChartCardModel {
    present_card(&reduce_card(card, snapshot, now, DataOrigin::Live))
}

/// The only constructor of a card model: every string is derived from typed
/// facts and nothing is reduced or ranked here.
pub fn present_card(facts: &CardFacts) -> ChartCardModel {
    let snapshot = &facts.source.snapshot;
    let now = facts.source.now;
    let dataset = snapshot.dataset.as_deref();
    let metric = facts.metric.as_ref();
    let title = facts
        .intent
        .title
        .clone()
        .or_else(|| metric.map(|metric| metric.label.clone()))
        .unwrap_or_else(|| facts.intent.metric.clone());
    let banner = banner_text(facts);

    let mut built = match (dataset, metric) {
        (Some(dataset), Some(metric)) => build_dataset(facts, dataset, metric),
        _ => refusal(message_of(&facts.outcome).unwrap_or_default()),
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
    let reading = match (dataset, facts.summary.as_ref()) {
        (Some(dataset), Some(summary)) if metric.is_some_and(|metric| metric.sampled) => summary
            .observed
            .map(|observed| instant_text(observed.interval.start, dataset.timezone())),
        _ => None,
    };
    let freshness = match facts.source.standing {
        Standing::Replayed => "replayed, not live".to_string(),
        Standing::Live(status) => freshness(snapshot, status, now),
    };
    let access = accessible_name(
        facts.intent.kind,
        &title,
        metric.map(|metric| unit_text(&metric.unit)),
        dataset,
        reading.as_deref(),
        &freshness,
    );
    ChartCardModel {
        card_id: facts.intent.id.clone(),
        kind: facts.intent.kind,
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
        info_lines: info_lines(facts, dataset),
        accessible_name: access,
    }
}

fn banner_text(facts: &CardFacts) -> Option<String> {
    let snapshot = &facts.source.snapshot;
    match facts.source.standing {
        Standing::Replayed => Some("Replayed data · not live".to_string()),
        Standing::Live(SourceStatus::Fresh) => None,
        Standing::Live(SourceStatus::Loading) => Some("Loading".to_string()),
        Standing::Live(SourceStatus::Stale) => Some("Stale".to_string()),
        Standing::Live(SourceStatus::RefreshFailed { stale: false }) => {
            Some("Refresh failed".to_string())
        }
        Standing::Live(SourceStatus::RefreshFailed { stale: true }) => {
            Some("Refresh failed · stale".to_string())
        }
        Standing::Live(SourceStatus::Unavailable) => Some(format!(
            "Source unavailable: {}",
            snapshot
                .last_error
                .as_ref()
                .map_or("unknown reason", |error| error.kind.label())
        )),
    }
}

fn mismatch_text(mismatch: &Mismatch) -> String {
    match mismatch.subject {
        MismatchSubject::Metric => "Card mismatch: the source does not provide this metric",
        MismatchSubject::Dimension => {
            "card mismatch: the source does not provide a requested dimension"
        }
        MismatchSubject::Summary => "Card mismatch: this metric does not support that summary",
        MismatchSubject::BoundedPercent => "Card mismatch: bounded_percent needs a percent metric",
        MismatchSubject::Grouping => {
            "Card mismatch: time bars stack by one dimension; use stack_by, group_by is not supported"
        }
        MismatchSubject::Stacking => "Stacking is only available for nonnegative interval totals",
        MismatchSubject::Sampled => {
            "Card mismatch: sampled readings are instants, not interval totals; draw them as a time series"
        }
        MismatchSubject::GaugeMerge => {
            "Gauge series are never summed; group by a dimension that separates them"
        }
    }
    .to_string()
}

fn capacity_text(capacity: &CapacityRefusal) -> String {
    match capacity.limit {
        CapacityLimit::UnknownSeries => format!(
            "Too many Unknown series: {} groups with an empty value exceed the {} the plot can show; every row is in Details",
            capacity.count.unwrap_or_default(),
            capacity.maximum.unwrap_or(MAX_VISIBLE_SERIES)
        ),
        CapacityLimit::GaugeSeries => format!(
            "Too many series: {} gauge series cannot be combined; group by fewer dimensions",
            capacity.count.unwrap_or_default()
        ),
        CapacityLimit::NumericRange => "Totals exceed the supported numeric range".to_string(),
    }
}

/// The card-level text of an outcome that is not a plot; `None` for
/// `Plotted`.
fn message_of(outcome: &Outcome) -> Option<String> {
    Some(match outcome {
        Outcome::Plotted => return None,
        Outcome::NoObservations => "No observations".to_string(),
        Outcome::NoIntervals => "No intervals in this range".to_string(),
        Outcome::NoKnownValues => "No known values in this range".to_string(),
        Outcome::Refused(Refusal::CardMismatch(mismatch)) => mismatch_text(mismatch),
        Outcome::Refused(Refusal::Capacity(capacity)) => capacity_text(capacity),
        Outcome::Unavailable { source_error: None } => "Waiting for the first answer".to_string(),
        Outcome::Unavailable {
            source_error: Some(_),
        } => "No data to show".to_string(),
    })
}

fn info_lines(facts: &CardFacts, dataset: Option<&ChartDataset>) -> Vec<String> {
    let snapshot = &facts.source.snapshot;
    let now = facts.source.now;
    let info = &snapshot.info;
    let zone = dataset
        .map(|dataset| dataset.timezone().clone())
        .or_else(|| ChartZone::parse(&info.timezone).ok())
        .or_else(|| ChartZone::parse("UTC").ok());
    let when = |instant: Option<Timestamp>| match (instant, &zone) {
        (Some(instant), Some(zone)) => local_text(instant, zone),
        _ => "never".to_string(),
    };
    let mut lines = Vec::new();
    if facts.source.standing == Standing::Replayed {
        lines.push("Origin: replayed from saved output; no command ran".to_string());
    }
    lines.extend([
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
    ]);
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

/// An interval total is "so far" while its interval is still open; a sampled
/// reading is an instant and never is.
fn so_far(dataset: &ChartDataset, metric: &MetricDescriptor, interval: &Interval) -> bool {
    !metric.sampled && interval.end > dataset.request_window().end
}

fn scale_kind(bounded_percent: bool, negative: bool) -> ScaleKind {
    if bounded_percent {
        ScaleKind::Percent0To100
    } else if negative {
        ScaleKind::Signed
    } else {
        ScaleKind::Nonnegative
    }
}

fn is_signed(facts: &CardFacts) -> bool {
    facts.groups.as_ref().is_some_and(|groups| groups.signed)
}

fn build_dataset(facts: &CardFacts, dataset: &ChartDataset, metric: &MetricDescriptor) -> Built {
    match facts.outcome {
        Outcome::Plotted | Outcome::NoKnownValues => match facts.intent.kind {
            ChartKind::TimeSeries => build_lines(facts, dataset, metric),
            ChartKind::TimeBars => build_bars(facts, dataset, metric),
            ChartKind::RankedBars => build_ranked(facts, dataset, metric),
        },
        _ => Built {
            message: message_of(&facts.outcome),
            rows: table_rows(facts, dataset, metric),
            ..Built::default()
        },
    }
}

/// The Details table as the GUI shows it: time cards group-major, ranked
/// cards in value order.
fn table_rows(
    facts: &CardFacts,
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
) -> Vec<Vec<String>> {
    let zone = dataset.timezone();
    let when = |interval: &Interval, sampled: bool| {
        if sampled {
            instant_text(interval.start, zone)
        } else {
            interval_text(interval, zone)
        }
    };
    let value_text =
        |value: Option<&ChartDecimal>| value.map_or_else(|| "no value".to_string(), exact_text);
    match facts.details.grain {
        None => Vec::new(),
        Some(DetailGrain::IntervalGroup) => {
            let mut ordered: Vec<_> = facts.details.rows.iter().collect();
            ordered.sort_by(|a, b| a.group.cmp(&b.group));
            ordered
                .into_iter()
                .map(|row| {
                    vec![
                        when(&row.interval, row.sampled),
                        full_label(&row.group),
                        value_text(row.known_subtotal.as_ref()),
                        quality_text(row.quality, so_far(dataset, metric, &row.interval)),
                        unit_text(&row.unit).to_string(),
                    ]
                })
                .collect()
        }
        Some(DetailGrain::Group) => {
            let open = facts
                .intervals
                .as_ref()
                .is_some_and(|intervals| intervals.open_right_edge);
            let sum = facts.intent.summary == Some(Reduction::Sum);
            facts
                .details
                .rows
                .iter()
                .map(|row| {
                    let mut quality = quality_text(row.quality, sum && open);
                    if row.last_known {
                        quality.push_str(" · last known");
                    }
                    vec![
                        when(&row.interval, row.sampled),
                        full_label(&row.group),
                        value_text(row.known_subtotal.as_ref()),
                        quality,
                        unit_text(&row.unit).to_string(),
                    ]
                })
                .collect()
        }
    }
}

fn sum_headline(
    known: Option<&ChartDecimal>,
    quality: Quality,
    metric: &MetricDescriptor,
    note: Option<String>,
) -> Headline {
    if quality == Quality::Complete && known.is_some() {
        return Headline {
            label: "Total".to_string(),
            value: known.map_or_else(String::new, |known| amount_text(known, &metric.unit)),
            note,
        };
    }
    let value = match known {
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

fn gauge_headline(summary: &SummaryFacts, zone: &ChartZone, metric: &MetricDescriptor) -> Headline {
    let Some((value, observed)) = summary.exact.as_ref().zip(summary.observed) else {
        return Headline {
            label: if metric.sampled {
                "Last observed"
            } else {
                "Latest"
            }
            .to_string(),
            value: "No known value".to_string(),
            note: None,
        };
    };
    let incomplete = summary.quality != Quality::Complete;
    if metric.sampled {
        let mut label = "Last observed".to_string();
        if incomplete {
            label.push_str(" · incomplete");
        }
        let mut note = format!("as of {}", instant_text(observed.interval.start, zone));
        if summary.newest_is_null {
            note.push_str(" · newest reading was not reported");
        }
        return Headline {
            label,
            value: amount_text(value, &metric.unit),
            note: Some(note),
        };
    }
    let mut label = if observed.is_latest {
        "Latest"
    } else {
        "Last known"
    }
    .to_string();
    if incomplete {
        label.push_str(" · incomplete");
    }
    Headline {
        label,
        value: amount_text(value, &metric.unit),
        note: (!observed.is_latest)
            .then(|| format!("observed {}", interval_text(&observed.interval, zone))),
    }
}

/// Header for time charts: the sum over all groups, or the single gauge's
/// last reading.
fn time_headline(
    facts: &CardFacts,
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
) -> Option<Headline> {
    let groups = facts.groups.as_ref()?;
    if metric.quantity == Quantity::Gauge {
        if groups.total != 1 {
            return Some(Headline {
                label: "Series".to_string(),
                value: format!("{} series", groups.total),
                note: None,
            });
        }
        return facts
            .summary
            .as_ref()
            .map(|summary| gauge_headline(summary, dataset.timezone(), metric));
    }
    let summary = facts.summary.as_ref()?;
    Some(sum_headline(
        summary.known_subtotal.as_ref(),
        summary.quality,
        metric,
        summary
            .includes_open_interval
            .then(|| "includes the current interval · so far".to_string()),
    ))
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

fn badges_for(facts: &CardFacts, current: bool) -> Vec<String> {
    let mut badges = Vec::new();
    if facts
        .intervals
        .as_ref()
        .is_some_and(|intervals| intervals.partial + intervals.missing > 0)
    {
        badges.push("incomplete".to_string());
    }
    if current {
        badges.push("so far".to_string());
    }
    badges
}

/// Where an interval's reading sits on the axis: the center of an interval
/// total, the start instant of a sampled reading.
fn position(axis: &TimeAxis, metric: &MetricDescriptor, interval: &Interval) -> f64 {
    if metric.sampled {
        axis.fraction(interval.start)
    } else {
        axis.center(interval)
    }
}

fn axes_for(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
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
            .map(|interval| position(&axis, metric, interval))
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
            let when = if metric.sampled {
                instant_text(interval.start, dataset.timezone())
            } else {
                interval_text(interval, dataset.timezone())
            };
            let mut parts = vec![when];
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
                    quality_text(cell.quality, so_far(dataset, metric, interval))
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

fn build_lines(facts: &CardFacts, dataset: &ChartDataset, metric: &MetricDescriptor) -> Built {
    let shown = shown_entries(facts);
    let headline = time_headline(facts, dataset, metric);
    let axis = TimeAxis::new(dataset.range());
    let intervals = dataset.intervals();
    let mut values = Vec::new();
    let series: Vec<LineSeries> = shown
        .iter()
        .map(|entry| {
            let mut segments: Vec<Vec<PlotPoint>> = Vec::new();
            let mut open = false;
            for (index, cell) in entry.cells.iter().enumerate() {
                // Interval totals join only across touching intervals; sampled
                // readings join unless the answer declares a break.
                let connected = index > 0
                    && if metric.sampled {
                        !entry.breaks.contains(&index)
                    } else {
                        intervals[index - 1].end == intervals[index].start
                    };
                match &cell.value {
                    Some(value) => {
                        let value = value.to_f64();
                        values.push(value);
                        if !(open && connected) {
                            segments.push(Vec::new());
                        }
                        segments
                            .last_mut()
                            .expect("segment exists")
                            .push(PlotPoint {
                                x: position(&axis, metric, &intervals[index]),
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
    let current = facts
        .summary
        .as_ref()
        .is_some_and(|summary| summary.includes_open_interval);
    let kind = scale_kind(facts.intent.bounded_percent, is_signed(facts));
    let rows = table_rows(facts, dataset, metric);
    let Some(scale) = value_scale(&values, kind) else {
        return Built {
            headline,
            message: message_of(&Outcome::NoKnownValues),
            rows,
            badges: badges_for(facts, current),
            ..Built::default()
        };
    };
    let axes = axes_for(dataset, metric, scale, &values, line_x_labels(dataset));
    Built {
        headline,
        plot: Some(PlotModel::Lines { series, axes }),
        legend: legend_of(&shown),
        rows,
        readouts: selection_readouts(dataset, metric, &shown),
        badges: badges_for(facts, current),
        footnote: None,
        message: None,
    }
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

fn build_bars(facts: &CardFacts, dataset: &ChartDataset, metric: &MetricDescriptor) -> Built {
    let stacked = facts.intent.stack_by.is_some();
    let shown = shown_entries(facts);
    let headline = time_headline(facts, dataset, metric);
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
            for entry in &shown {
                let cell = &entry.cells[index];
                quality = worst(quality, cell.quality);
                let Some(value) = cell.value.as_ref().map(|value| value.to_f64()) else {
                    continue;
                };
                any = true;
                if !stacked {
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
            if stacked && any {
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
                so_far: so_far(dataset, metric, interval),
            }
        })
        .collect();
    let current = bars
        .last()
        .is_some_and(|bar| bar.so_far && !bar.segments.is_empty());
    let kind = scale_kind(facts.intent.bounded_percent, is_signed(facts));
    let rows = table_rows(facts, dataset, metric);
    let Some(scale) = value_scale(&totals, kind) else {
        return Built {
            headline,
            message: message_of(&Outcome::NoKnownValues),
            rows,
            badges: badges_for(facts, current),
            ..Built::default()
        };
    };
    let x_labels = bar_x_labels(dataset, &bars);
    let axes = axes_for(dataset, metric, scale, &totals, x_labels);
    Built {
        headline,
        plot: Some(PlotModel::Bars { bars, axes }),
        legend: if stacked {
            legend_of(&shown)
        } else {
            Vec::new()
        },
        rows,
        readouts: selection_readouts(dataset, metric, &shown),
        badges: badges_for(facts, current),
        footnote: current.then(|| "* so far: the current interval is still open".to_string()),
        message: None,
    }
}

fn is_stale_observation(entry: &Entry) -> bool {
    matches!(entry.observed, Some((_, false)))
}

fn build_ranked(facts: &CardFacts, dataset: &ChartDataset, metric: &MetricDescriptor) -> Built {
    let shown = shown_entries(facts);
    let summary = facts.intent.summary.unwrap_or(Reduction::Sum);
    let negative = is_signed(facts);
    let subtotal = facts.summary.as_ref();
    let share_base = (!negative && summary == Reduction::Sum)
        .then(|| {
            subtotal
                .and_then(|subtotal| subtotal.known_subtotal.as_ref())
                .map(|known| known.to_f64())
        })
        .flatten()
        .filter(|base| *base > 0.0);
    let current = facts
        .intervals
        .as_ref()
        .is_some_and(|intervals| intervals.open_right_edge);
    let zone = dataset.timezone();
    let rows: Vec<RankedRow> = shown
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
    let kind = scale_kind(facts.intent.bounded_percent, negative);
    let table = table_rows(facts, dataset, metric);
    let readouts = rows
        .iter()
        .zip(&shown)
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
                        if metric.sampled {
                            instant_text(dataset.intervals()[index].start, zone)
                        } else {
                            interval_text(&dataset.intervals()[index], zone)
                        }
                    ),
                    _ => String::new(),
                }
            )
        })
        .collect();
    let headline = subtotal.map(|subtotal| {
        let mut headline = sum_headline(
            subtotal.known_subtotal.as_ref(),
            subtotal.quality,
            metric,
            None,
        );
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
        return Built {
            headline,
            message: message_of(&Outcome::NoKnownValues),
            rows: table,
            ..Built::default()
        };
    };
    Built {
        headline,
        plot: Some(PlotModel::Ranked { rows, scale }),
        legend: Vec::new(),
        rows: table,
        readouts,
        badges: badges_for(facts, current && summary == Reduction::Sum),
        footnote,
        message: None,
    }
}
