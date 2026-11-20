//! Card facts: one typed, display-free result per chart card.
//!
//! `reduce_card` is the only place a card's exact reductions, ranking,
//! capacity and empty-state decisions are made. The GUI model is a function of
//! these facts (`chart_view::present_card`) and the command line serializes
//! them; nothing here knows a display string, a drawing surface or a command.
//! Exact numbers are `ChartDecimal`; drawing numbers elsewhere are `f64` and
//! never authoritative.

use super::config::{ChartCardConfig, ChartKind};
use super::dataset::{
    Cell, ChartDataset, MAX_VISIBLE_SERIES, MetricDescriptor, Quality, Quantity,
    UnknownContribution,
};
use super::decimal::ChartDecimal;
use super::error::ChartErrorKind;
use super::reduce::{GroupedSeries, Reduction, collapse_series, last_known, sum_cells};
use super::state::{SourceSnapshot, SourceStatus};
use super::time::{Interval, Timestamp};
use std::cmp::Ordering;

pub const MAX_LEGEND_ENTRIES: usize = 5;
pub const DEFAULT_TOP_N: usize = 5;

/// Where the dataset behind the facts came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataOrigin {
    Live,
    Replay,
}

/// How the source stands for this card: a live status, or a replay of saved
/// output that ran no command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Live(SourceStatus),
    Replayed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardIntent {
    pub id: String,
    pub source: String,
    pub kind: ChartKind,
    pub metric: String,
    pub title: Option<String>,
    /// The card's reduction, or the metric's default; `None` only when the
    /// card names no summary and the metric is not known.
    pub summary: Option<Reduction>,
    pub group_by: Vec<String>,
    pub stack_by: Option<String>,
    /// Ranked bars only: the effective limit and its default of five.
    pub top_n: Option<usize>,
    pub bounded_percent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceFacts {
    pub origin: DataOrigin,
    pub standing: Standing,
    pub snapshot: SourceSnapshot,
    pub now: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchSubject {
    Metric,
    Dimension,
    Summary,
    BoundedPercent,
    Grouping,
    Stacking,
    Sampled,
    GaugeMerge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub subject: MismatchSubject,
    pub wanted: Option<String>,
    pub available: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityLimit {
    UnknownSeries,
    GaugeSeries,
    NumericRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityRefusal {
    pub limit: CapacityLimit,
    pub count: Option<usize>,
    pub maximum: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    CardMismatch(Mismatch),
    Capacity(CapacityRefusal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Plotted,
    NoObservations,
    NoIntervals,
    NoKnownValues,
    Refused(Refusal),
    /// `None` means the source has not answered yet (GUI only).
    Unavailable {
        source_error: Option<ChartErrorKind>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Exact,
    AtLeast,
    Incomplete,
}

/// The interval a gauge reading was taken in; a sampled reading is at `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub index: usize,
    pub interval: Interval,
    /// False when a newer interval has no value.
    pub is_latest: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryFacts {
    pub reduction: Reduction,
    /// `None` when quality is `Missing`: the known amount is then only in
    /// `known_subtotal`.
    pub exact: Option<ChartDecimal>,
    pub quality: Quality,
    pub bound: Bound,
    /// Sums only: the exact sum of the values actually present (`None` when
    /// no value is present; a real zero is `Some(0)`).
    pub known_subtotal: Option<ChartDecimal>,
    /// Any negative present value among the card's groups.
    pub signed: bool,
    /// Gauges only: the newest interval has no value, so `exact` is older.
    pub newest_is_null: bool,
    pub observed: Option<Observation>,
    /// Sums only: the newest interval ends after the request and has a value.
    pub includes_open_interval: bool,
}

/// Per interval, the worst quality across every series of the card's metric.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntervalFacts {
    pub total: usize,
    pub complete: usize,
    pub partial: usize,
    pub missing: usize,
    /// The newest interval ends after the request. Always false for a sampled
    /// metric: a reading at an instant is not an open bucket.
    pub open_right_edge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupFacts {
    pub by: Vec<String>,
    pub total: usize,
    pub named_shown: usize,
    pub other_members: usize,
    pub unknown_shown: usize,
    /// The named-group limit; `None` when there are at most 16 groups.
    pub capacity: Option<usize>,
    /// Entries the GUI legend shows (none for a single unlabeled group).
    pub legend_shown: usize,
    pub legend_overflow: usize,
    /// Any negative present value among the card's groups.
    pub signed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowRole {
    Named,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupRow {
    /// `None` for Other.
    pub group: Option<Vec<(String, String)>>,
    pub role: RowRole,
    pub members: usize,
    /// The known subtotal, or the selected observation of a gauge.
    pub value: Option<ChartDecimal>,
    pub quality: Quality,
    pub observed: Option<Observation>,
    pub cells: Vec<Cell>,
    /// Sampled lines do not connect across these interval indices.
    pub breaks: Vec<usize>,
}

impl GroupRow {
    pub fn known_subtotal(&self) -> Option<&ChartDecimal> {
        self.value.as_ref()
    }

    /// `None` exactly when the quality is `Missing`.
    pub fn exact(&self) -> Option<&ChartDecimal> {
        (self.quality != Quality::Missing)
            .then_some(self.value.as_ref())
            .flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailGrain {
    IntervalGroup,
    Group,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailRow {
    pub interval: Interval,
    /// The row is a reading at `interval.start`.
    pub sampled: bool,
    pub group: Vec<(String, String)>,
    /// How the row appears in the plot; `None` when it is not drawn (a
    /// refused card, or a gauge row cut by `top_n`).
    pub shown_as: Option<RowRole>,
    /// The present known value; for a `Missing` merged cell it still carries
    /// the known part.
    pub known_subtotal: Option<ChartDecimal>,
    pub quality: Quality,
    pub unit: String,
    pub break_before: bool,
    /// Group grain gauge rows: a newer interval has no value.
    pub last_known: bool,
}

impl DetailRow {
    /// `None` exactly when the quality is `Missing`.
    pub fn exact(&self) -> Option<&ChartDecimal> {
        (self.quality != Quality::Missing)
            .then_some(self.known_subtotal.as_ref())
            .flatten()
    }
}

/// The full set of rows, before any top-N folding.
#[derive(Debug, Clone, PartialEq)]
pub struct DetailFacts {
    pub grain: Option<DetailGrain>,
    pub rows: Vec<DetailRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardFacts {
    pub intent: CardIntent,
    pub source: SourceFacts,
    pub metric: Option<MetricDescriptor>,
    pub outcome: Outcome,
    pub summary: Option<SummaryFacts>,
    pub intervals: Option<IntervalFacts>,
    pub groups: Option<GroupFacts>,
    /// Plotted rows, at most 16 including Other and every Unknown. Time
    /// cards carry every plotted series; ranked cards the shown rows.
    pub rows: Vec<GroupRow>,
    pub details: DetailFacts,
    /// Every collapsed group key in key order, before ranking: display
    /// labels depend on the whole set.
    pub group_keys: Vec<Vec<(String, String)>>,
}

struct Parts {
    outcome: Outcome,
    summary: Option<SummaryFacts>,
    intervals: Option<IntervalFacts>,
    groups: Option<GroupFacts>,
    rows: Vec<GroupRow>,
    details: DetailFacts,
    group_keys: Vec<Vec<(String, String)>>,
}

impl Parts {
    fn of(outcome: Outcome) -> Self {
        Self {
            outcome,
            summary: None,
            intervals: None,
            groups: None,
            rows: Vec::new(),
            details: DetailFacts {
                grain: None,
                rows: Vec::new(),
            },
            group_keys: Vec::new(),
        }
    }

    fn refused(refusal: Refusal) -> Self {
        Self::of(Outcome::Refused(refusal))
    }
}

fn refused_box(refusal: Refusal) -> Box<Parts> {
    Box::new(Parts::refused(refusal))
}

fn mismatch(subject: MismatchSubject, wanted: Option<&str>, available: Vec<String>) -> Refusal {
    Refusal::CardMismatch(Mismatch {
        subject,
        wanted: wanted.map(str::to_string),
        available,
    })
}

fn numeric_range() -> Refusal {
    Refusal::Capacity(CapacityRefusal {
        limit: CapacityLimit::NumericRange,
        count: None,
        maximum: None,
    })
}

fn reduction_name(reduction: Reduction) -> &'static str {
    match reduction {
        Reduction::Sum => "sum",
        Reduction::Last => "last",
    }
}

fn default_reduction(metric: &MetricDescriptor) -> Reduction {
    match metric.quantity {
        Quantity::IntervalTotal => Reduction::Sum,
        Quantity::Gauge => Reduction::Last,
    }
}

/// Pure: the same card, snapshot, instant and origin always give the same facts.
pub fn reduce_card(
    card: &ChartCardConfig,
    snapshot: &SourceSnapshot,
    now: Timestamp,
    origin: DataOrigin,
) -> CardFacts {
    let dataset = snapshot.dataset.as_deref();
    let metric = dataset
        .and_then(|dataset| dataset.metric(&card.metric))
        .cloned();
    let standing = match (origin, dataset) {
        (DataOrigin::Replay, Some(_)) => Standing::Replayed,
        _ => Standing::Live(snapshot.status(now)),
    };
    let intent = CardIntent {
        id: card.id.clone(),
        source: card.source.clone(),
        kind: card.kind,
        metric: card.metric.clone(),
        title: card.title.clone(),
        summary: card
            .summary
            .or_else(|| metric.as_ref().map(default_reduction)),
        group_by: card.group_by.clone(),
        stack_by: card.stack_by.clone(),
        top_n: (card.kind == ChartKind::RankedBars)
            .then(|| card.top_n.map_or(DEFAULT_TOP_N, usize::from)),
        bounded_percent: card.bounded_percent,
    };
    let parts = match (dataset, &metric) {
        (None, _) => Parts::of(Outcome::Unavailable {
            source_error: snapshot.last_error.as_ref().map(|error| error.kind),
        }),
        (Some(dataset), None) => Parts::refused(mismatch(
            MismatchSubject::Metric,
            Some(&card.metric),
            dataset
                .metrics()
                .iter()
                .map(|metric| metric.id.clone())
                .collect(),
        )),
        (Some(dataset), Some(metric)) => evaluate(card, dataset, metric),
    };
    CardFacts {
        intent,
        source: SourceFacts {
            origin,
            standing,
            snapshot: snapshot.clone(),
            now,
        },
        metric,
        outcome: parts.outcome,
        summary: parts.summary,
        intervals: parts.intervals,
        groups: parts.groups,
        rows: parts.rows,
        details: parts.details,
        group_keys: parts.group_keys,
    }
}

fn evaluate(card: &ChartCardConfig, dataset: &ChartDataset, metric: &MetricDescriptor) -> Parts {
    let summary = card.summary.unwrap_or_else(|| default_reduction(metric));
    if !summary.permitted(metric.quantity) {
        return Parts::refused(mismatch(
            MismatchSubject::Summary,
            Some(reduction_name(summary)),
            vec![reduction_name(default_reduction(metric)).to_string()],
        ));
    }
    if card.bounded_percent && metric.unit != "percent" {
        return Parts::refused(mismatch(
            MismatchSubject::BoundedPercent,
            Some("percent"),
            vec![metric.unit.clone()],
        ));
    }
    if card.kind == ChartKind::TimeBars && metric.sampled {
        return Parts::refused(mismatch(
            MismatchSubject::Sampled,
            None,
            vec!["time-series".to_string(), "ranked-bars".to_string()],
        ));
    }
    if dataset.series().is_empty() {
        return Parts::of(Outcome::NoObservations);
    }
    let context = Context {
        card,
        dataset,
        metric,
        summary,
    };
    let result = match card.kind {
        ChartKind::TimeSeries => time_card(&context, false),
        ChartKind::TimeBars => time_card(&context, true),
        ChartKind::RankedBars => ranked_card(&context),
    };
    let mut parts = match result {
        Ok(parts) => parts,
        Err(parts) => *parts,
    };
    parts.intervals = Some(interval_facts(dataset, metric));
    parts
}

struct Context<'a> {
    card: &'a ChartCardConfig,
    dataset: &'a ChartDataset,
    metric: &'a MetricDescriptor,
    summary: Reduction,
}

fn interval_facts(dataset: &ChartDataset, metric: &MetricDescriptor) -> IntervalFacts {
    let mut facts = IntervalFacts {
        total: dataset.intervals().len(),
        complete: 0,
        partial: 0,
        missing: 0,
        open_right_edge: !metric.sampled
            && dataset
                .intervals()
                .last()
                .is_some_and(|interval| interval.end > dataset.request_window().end),
    };
    for index in 0..facts.total {
        let worst = dataset
            .series()
            .iter()
            .filter_map(|series| series.metrics.get(&metric.id))
            .map(|cells| cells[index].quality)
            .max()
            .unwrap_or(Quality::Complete);
        match worst {
            Quality::Complete => facts.complete += 1,
            Quality::Partial => facts.partial += 1,
            Quality::Missing => facts.missing += 1,
        }
    }
    facts
}

fn collapse(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    dimensions: &[String],
) -> Result<Vec<GroupedSeries>, Refusal> {
    let groups =
        collapse_series(dataset, &metric.id, dimensions).map_err(|error| match error.kind {
            ChartErrorKind::CardMismatch => missing_dimension(dataset, metric, dimensions),
            _ => numeric_range(),
        })?;
    if metric.quantity == Quantity::Gauge && groups.len() < dataset.series().len() {
        return Err(mismatch(MismatchSubject::GaugeMerge, None, Vec::new()));
    }
    Ok(groups)
}

fn missing_dimension(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    dimensions: &[String],
) -> Refusal {
    let available: Vec<String> = dataset
        .dimension_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let wanted = dimensions
        .iter()
        .find(|name| !available.contains(name))
        .cloned();
    if wanted.is_none() {
        // Only the metric itself can be absent here.
        return mismatch(
            MismatchSubject::Metric,
            Some(&metric.id),
            dataset
                .metrics()
                .iter()
                .map(|metric| metric.id.clone())
                .collect(),
        );
    }
    mismatch(MismatchSubject::Dimension, wanted.as_deref(), available)
}

fn is_unknown(key: &[(String, String)]) -> bool {
    key.iter().any(|(_, value)| value.is_empty())
}

fn unknown_count(groups: &[GroupedSeries]) -> usize {
    groups.iter().filter(|group| is_unknown(&group.key)).count()
}

fn has_negative(groups: &[GroupedSeries]) -> bool {
    groups
        .iter()
        .flat_map(|group| &group.cells)
        .any(|cell| cell.value.as_ref().is_some_and(|value| value.is_negative()))
}

/// The number of named groups the plot can show beside Other and every
/// Unknown; `None` when everything fits.
fn named_limit(groups: &[GroupedSeries]) -> Result<Option<usize>, CapacityRefusal> {
    if groups.len() <= MAX_VISIBLE_SERIES {
        return Ok(None);
    }
    let unknown = unknown_count(groups);
    MAX_VISIBLE_SERIES
        .checked_sub(unknown.saturating_add(1))
        .map(Some)
        .ok_or(CapacityRefusal {
            limit: CapacityLimit::UnknownSeries,
            count: Some(unknown),
            maximum: Some(MAX_VISIBLE_SERIES),
        })
}

fn too_many_gauges(count: usize) -> Refusal {
    Refusal::Capacity(CapacityRefusal {
        limit: CapacityLimit::GaugeSeries,
        count: Some(count),
        maximum: Some(MAX_VISIBLE_SERIES),
    })
}

fn bound_of(quality: Quality, metric: &MetricDescriptor) -> Bound {
    match quality {
        Quality::Complete => Bound::Exact,
        Quality::Partial if metric.unknown_contribution == UnknownContribution::Nonnegative => {
            Bound::AtLeast
        }
        _ => Bound::Incomplete,
    }
}

fn observation(dataset: &ChartDataset, index: usize, is_latest: bool) -> Observation {
    Observation {
        index,
        interval: dataset.intervals()[index],
        is_latest,
    }
}

fn so_far(dataset: &ChartDataset, metric: &MetricDescriptor, interval: &Interval) -> bool {
    !metric.sampled && interval.end > dataset.request_window().end
}

/// Ranked entries: named by value, an Other for the cut named groups (sums
/// only), and every Unknown kept apart and never folded.
struct Ranking {
    shown: Vec<GroupRow>,
    /// Every named row in rank order, then Unknown; Other never repeats them.
    full: Vec<GroupRow>,
    named_shown: usize,
}

fn order(a: &GroupRow, b: &GroupRow) -> Ordering {
    match (&a.value, &b.value) {
        (Some(x), Some(y)) => y.cmp(x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| a.group.cmp(&b.group))
}

fn reduced_row(
    dataset: &ChartDataset,
    group: GroupedSeries,
    reduction: Reduction,
) -> Result<GroupRow, Refusal> {
    let (value, quality, observed) = match reduction {
        Reduction::Sum => {
            let sum = sum_cells(&group.cells).map_err(|_| numeric_range())?;
            (sum.known, sum.quality, None)
        }
        Reduction::Last => match last_known(&group.cells) {
            Some(last) => (
                Some(last.value),
                last.quality,
                Some(observation(dataset, last.index, last.is_latest)),
            ),
            None => (None, Quality::Missing, None),
        },
    };
    Ok(GroupRow {
        role: if is_unknown(&group.key) {
            RowRole::Unknown
        } else {
            RowRole::Named
        },
        group: Some(group.key),
        members: 1,
        value,
        quality,
        observed,
        cells: group.cells,
        breaks: group.breaks,
    })
}

/// `named_limit` of `None` keeps every named row. Cutting named rows creates
/// an Other only for sums; gauges cannot be aggregated honestly.
fn rank(
    dataset: &ChartDataset,
    groups: Vec<GroupedSeries>,
    reduction: Reduction,
    named_limit: Option<usize>,
) -> Result<Ranking, Refusal> {
    let mut named = Vec::new();
    let mut unknown = Vec::new();
    for group in groups {
        let row = reduced_row(dataset, group, reduction)?;
        if row.role == RowRole::Unknown {
            unknown.push(row);
        } else {
            named.push(row);
        }
    }
    named.sort_by(order);
    unknown.sort_by(order);
    let mut full = named.clone();
    full.extend(unknown.iter().cloned());

    let mut shown = named;
    let mut rest = Vec::new();
    if let Some(limit) = named_limit
        && shown.len() > limit
    {
        rest = shown.split_off(limit);
    }
    let named_shown = shown.len();
    if !rest.is_empty() && reduction == Reduction::Sum {
        let pseudo: Vec<Cell> = rest
            .iter()
            .map(|row| Cell {
                value: row.value.clone(),
                quality: row.quality,
            })
            .collect();
        let total = sum_cells(&pseudo).map_err(|_| numeric_range())?;
        let width = rest[0].cells.len();
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            let sum =
                sum_cells(rest.iter().map(|row| &row.cells[index])).map_err(|_| numeric_range())?;
            cells.push(Cell {
                value: sum.known,
                quality: sum.quality,
            });
        }
        shown.push(GroupRow {
            group: None,
            role: RowRole::Other,
            members: rest.len(),
            value: total.known,
            quality: total.quality,
            observed: None,
            cells,
            breaks: Vec::new(),
        });
    }
    shown.extend(unknown);
    Ok(Ranking {
        shown,
        full,
        named_shown,
    })
}

fn legend_len(shown: &[GroupRow]) -> usize {
    if shown.len() == 1 && shown[0].group.as_ref().is_some_and(Vec::is_empty) {
        0
    } else {
        shown.len()
    }
}

fn group_facts(
    by: &[String],
    groups: &[GroupedSeries],
    limit: Option<usize>,
    ranking: &Ranking,
    legend: usize,
    signed: bool,
) -> GroupFacts {
    GroupFacts {
        by: by.to_vec(),
        total: groups.len(),
        named_shown: ranking.named_shown,
        other_members: ranking
            .shown
            .iter()
            .filter(|row| row.role == RowRole::Other)
            .map(|row| row.members)
            .sum(),
        unknown_shown: ranking
            .shown
            .iter()
            .filter(|row| row.role == RowRole::Unknown)
            .count(),
        capacity: limit,
        legend_shown: legend.min(MAX_LEGEND_ENTRIES),
        legend_overflow: legend.saturating_sub(MAX_LEGEND_ENTRIES),
        signed,
    }
}

/// Interval-major, then retained tuple. `roles` says how each group appears;
/// `None` for every row means the card did not plot.
fn time_details(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    groups: &[GroupedSeries],
    ranking: Option<&Ranking>,
) -> DetailFacts {
    let role_of = |key: &Vec<(String, String)>| {
        let ranking = ranking?;
        if is_unknown(key) {
            return Some(RowRole::Unknown);
        }
        let named = ranking
            .shown
            .iter()
            .any(|row| row.role == RowRole::Named && row.group.as_ref() == Some(key));
        Some(if named {
            RowRole::Named
        } else {
            RowRole::Other
        })
    };
    let roles: Vec<Option<RowRole>> = groups.iter().map(|group| role_of(&group.key)).collect();
    let mut rows = Vec::with_capacity(groups.len() * dataset.intervals().len());
    for (index, interval) in dataset.intervals().iter().enumerate() {
        for (group, role) in groups.iter().zip(&roles) {
            let cell = &group.cells[index];
            rows.push(DetailRow {
                interval: *interval,
                sampled: metric.sampled,
                group: group.key.clone(),
                shown_as: *role,
                known_subtotal: cell.value.clone(),
                quality: cell.quality,
                unit: metric.unit.clone(),
                break_before: group.breaks.contains(&index),
                last_known: false,
            });
        }
    }
    DetailFacts {
        grain: Some(DetailGrain::IntervalGroup),
        rows,
    }
}

fn time_card(context: &Context, bars: bool) -> Result<Parts, Box<Parts>> {
    let Context {
        card,
        dataset,
        metric,
        ..
    } = *context;
    let stack_dims: Vec<String> = card.stack_by.iter().cloned().collect();
    let dimensions: &[String] = if bars { &stack_dims } else { &card.group_by };
    if bars && !card.group_by.is_empty() && card.group_by != stack_dims {
        return Err(refused_box(mismatch(
            MismatchSubject::Grouping,
            Some("stack_by"),
            stack_dims,
        )));
    }
    let groups = collapse(dataset, metric, dimensions).map_err(refused_box)?;
    let signed = has_negative(&groups);
    if bars && card.stack_by.is_some() && (metric.quantity != Quantity::IntervalTotal || signed) {
        return Err(refused_box(mismatch(
            MismatchSubject::Stacking,
            None,
            Vec::new(),
        )));
    }
    let keys: Vec<Vec<(String, String)>> = groups.iter().map(|group| group.key.clone()).collect();
    if dataset.intervals().is_empty() {
        let mut parts = Parts::of(Outcome::NoIntervals);
        parts.group_keys = keys;
        return Ok(parts);
    }
    let limit = match named_limit(&groups) {
        Ok(limit) => limit,
        Err(capacity) => {
            let mut parts = Parts::refused(Refusal::Capacity(capacity));
            parts.details = time_details(dataset, metric, &groups, None);
            parts.group_keys = keys;
            return Err(Box::new(parts));
        }
    };
    if limit.is_some() && metric.quantity == Quantity::Gauge {
        return Err(refused_box(too_many_gauges(groups.len())));
    }
    let reduction = default_reduction(metric);
    let ranking = rank(dataset, groups.clone(), reduction, limit).map_err(refused_box)?;
    let total_cells: Option<Vec<Cell>> = if dimensions.is_empty() || reduction == Reduction::Last {
        groups.first().map(|group| group.cells.clone())
    } else {
        let all = collapse(dataset, metric, &[]).map_err(refused_box)?;
        all.into_iter().next().map(|group| group.cells)
    };
    let summary = match &total_cells {
        Some(cells) if groups.len() == 1 || reduction == Reduction::Sum => {
            Some(time_summary(dataset, metric, cells, reduction, signed).map_err(refused_box)?)
        }
        _ => None,
    };
    let has_value = ranking
        .shown
        .iter()
        .flat_map(|row| &row.cells)
        .any(|cell| cell.value.is_some());
    let legend = if !bars || card.stack_by.is_some() {
        legend_len(&ranking.shown)
    } else {
        0
    };
    let mut parts = Parts::of(if has_value {
        Outcome::Plotted
    } else {
        Outcome::NoKnownValues
    });
    parts.summary = summary;
    parts.groups = Some(group_facts(
        dimensions, &groups, limit, &ranking, legend, signed,
    ));
    parts.details = time_details(dataset, metric, &groups, Some(&ranking));
    parts.group_keys = keys;
    parts.rows = ranking.shown;
    Ok(parts)
}

/// The card-level figure of a time card: the sum over all series, or the one
/// gauge's last reading.
fn time_summary(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    cells: &[Cell],
    reduction: Reduction,
    signed: bool,
) -> Result<SummaryFacts, Refusal> {
    if reduction == Reduction::Last {
        return Ok(match last_known(cells) {
            Some(last) => SummaryFacts {
                reduction,
                exact: Some(last.value),
                quality: last.quality,
                bound: bound_of(last.quality, metric),
                known_subtotal: None,
                signed,
                newest_is_null: !last.is_latest,
                observed: Some(observation(dataset, last.index, last.is_latest)),
                includes_open_interval: false,
            },
            None => SummaryFacts {
                reduction,
                exact: None,
                quality: Quality::Missing,
                bound: Bound::Incomplete,
                known_subtotal: None,
                signed,
                newest_is_null: !cells.is_empty(),
                observed: None,
                includes_open_interval: false,
            },
        });
    }
    let sum = sum_cells(cells).map_err(|_| numeric_range())?;
    let open = dataset
        .intervals()
        .last()
        .is_some_and(|interval| so_far(dataset, metric, interval))
        && cells.last().is_some_and(|cell| cell.value.is_some());
    Ok(sum_summary(metric, sum.known, sum.quality, signed, open))
}

fn sum_summary(
    metric: &MetricDescriptor,
    known: Option<ChartDecimal>,
    quality: Quality,
    signed: bool,
    open: bool,
) -> SummaryFacts {
    SummaryFacts {
        reduction: Reduction::Sum,
        exact: (quality != Quality::Missing)
            .then(|| known.clone())
            .flatten(),
        quality,
        bound: bound_of(quality, metric),
        known_subtotal: known,
        signed,
        newest_is_null: false,
        observed: None,
        includes_open_interval: open,
    }
}

fn ranked_card(context: &Context) -> Result<Parts, Box<Parts>> {
    let Context {
        card,
        dataset,
        metric,
        summary,
    } = *context;
    let groups = collapse(dataset, metric, &card.group_by).map_err(refused_box)?;
    if summary == Reduction::Last && groups.len() > MAX_VISIBLE_SERIES {
        return Err(refused_box(too_many_gauges(groups.len())));
    }
    let keys: Vec<Vec<(String, String)>> = groups.iter().map(|group| group.key.clone()).collect();
    if dataset.intervals().is_empty() {
        let mut parts = Parts::of(Outcome::NoIntervals);
        parts.group_keys = keys;
        return Ok(parts);
    }
    let top_n = card.top_n.map_or(DEFAULT_TOP_N, usize::from);
    // Every visible row, Other and Unknown included, must fit the plot: the
    // named limit reserves a slot for Other and for each Unknown group.
    let (capacity, limit, refusal) = match named_limit(&groups) {
        Ok(capacity) => (capacity, capacity.map_or(top_n, |c| c.min(top_n)), None),
        Err(refusal) => (None, top_n, Some(refusal)),
    };
    let ranking = rank(dataset, groups.clone(), summary, Some(limit)).map_err(refused_box)?;
    let signed = has_negative(&groups);
    let details = ranked_details(dataset, metric, &ranking, summary, refusal.is_none());
    if let Some(refusal) = refusal {
        let mut parts = Parts::refused(Refusal::Capacity(refusal));
        parts.details = details;
        parts.group_keys = keys;
        return Err(Box::new(parts));
    }
    let pseudo: Vec<Cell> = ranking
        .shown
        .iter()
        .map(|row| Cell {
            value: row.value.clone(),
            quality: row.quality,
        })
        .collect();
    let subtotal = sum_cells(&pseudo).map_err(|_| refused_box(numeric_range()))?;
    let summary_facts = (summary == Reduction::Sum)
        .then(|| sum_summary(metric, subtotal.known, subtotal.quality, signed, false));
    let has_value = ranking.shown.iter().any(|row| row.value.is_some());
    let mut parts = Parts::of(if has_value {
        Outcome::Plotted
    } else {
        Outcome::NoKnownValues
    });
    parts.summary = summary_facts;
    parts.groups = Some(group_facts(
        &card.group_by,
        &groups,
        capacity,
        &ranking,
        0,
        signed,
    ));
    parts.details = details;
    parts.group_keys = keys;
    parts.rows = ranking.shown;
    Ok(parts)
}

/// One row per retained group, in value order with the stable tie-break.
fn ranked_details(
    dataset: &ChartDataset,
    metric: &MetricDescriptor,
    ranking: &Ranking,
    summary: Reduction,
    drawn: bool,
) -> DetailFacts {
    let rows = ranking
        .full
        .iter()
        .enumerate()
        .map(|(position, row)| {
            let shown_as = if !drawn {
                None
            } else if row.role == RowRole::Unknown {
                Some(RowRole::Unknown)
            } else if position < ranking.named_shown {
                Some(RowRole::Named)
            } else if summary == Reduction::Sum {
                Some(RowRole::Other)
            } else {
                None
            };
            DetailRow {
                interval: row
                    .observed
                    .map_or_else(|| dataset.range(), |observed| observed.interval),
                sampled: metric.sampled && row.observed.is_some(),
                group: row.group.clone().unwrap_or_default(),
                shown_as,
                known_subtotal: row.value.clone(),
                quality: row.quality,
                unit: metric.unit.clone(),
                break_before: false,
                last_known: row.observed.is_some_and(|observed| !observed.is_latest),
            }
        })
        .collect();
    DetailFacts {
        grain: Some(DetailGrain::Group),
        rows,
    }
}
