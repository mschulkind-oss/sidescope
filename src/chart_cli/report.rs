//! The report document: serde types in declaration order, built only from card
//! facts, source snapshots and attempts. Nothing here reduces, sums or parses
//! display text.

use super::args::DetailsRequest;
use crate::chart::config::{ChartCardConfig, ChartKind};
use crate::chart::dataset::{ChartDataset, Quality, Quantity, UnknownContribution};
use crate::chart::facts::{
    Bound, CapacityLimit, CardFacts, CardIntent, DetailGrain, MismatchSubject, Outcome, Refusal,
    RowRole,
};
use crate::chart::reduce::Reduction;
use crate::chart::schema::{CHECKS, REPORT_LIMIT_BYTES, REPORT_SCHEMA_ID};
use crate::chart_view::{PlotModel, present_card};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    Findings,
    Invalid,
    Undetermined,
}

impl Status {
    pub fn exit_code(self) -> i32 {
        match self {
            Status::Ok => 0,
            Status::Findings => 1,
            Status::Invalid => 2,
            Status::Undetermined => 3,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Findings => "findings",
            Status::Invalid => "invalid",
            Status::Undetermined => "undetermined",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClockBlock {
    pub at: String,
    pub origin: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigBlock {
    pub path: String,
    pub origin: &'static str,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportError {
    pub id: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl ReportError {
    pub fn new(id: &'static str, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            key: None,
            card: None,
            source: None,
        }
    }
}

/// One check outcome, top level or per source.
#[derive(Debug, Clone, Serialize)]
pub struct CheckBlock {
    pub id: &'static str,
    pub required: bool,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClocksBlock {
    pub request_at: Option<String>,
    pub model_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestBlock {
    pub window: Option<String>,
    pub calendar_days: Option<u16>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub timezone: String,
    pub executable: String,
    pub argc: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttemptBlock {
    pub latency_ms: u64,
    pub exit: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RangeBlock {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricBlock {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub quantity: &'static str,
    pub estimate: bool,
    pub unknown_contribution: &'static str,
    pub sampled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidDataset {
    pub status: &'static str,
    pub range: RangeBlock,
    pub intervals: usize,
    pub series: usize,
    pub dimensions: Vec<String>,
    pub metrics: Vec<MetricBlock>,
    pub generated_at: String,
    pub observed_through: Option<String>,
    pub revision: Option<String>,
    pub notices_count: usize,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum DatasetBlock {
    Valid(Box<ValidDataset>),
    Unread { status: &'static str },
}

impl DatasetBlock {
    pub fn from_dataset(dataset: &ChartDataset) -> Self {
        DatasetBlock::Valid(Box::new(ValidDataset {
            status: "valid",
            range: RangeBlock {
                start: dataset.range().start.to_string(),
                end: dataset.range().end.to_string(),
            },
            intervals: dataset.intervals().len(),
            series: dataset.series().len(),
            dimensions: dataset
                .dimension_names()
                .into_iter()
                .map(str::to_string)
                .collect(),
            metrics: dataset
                .metrics()
                .iter()
                .map(|metric| MetricBlock {
                    id: metric.id.clone(),
                    label: metric.label.clone(),
                    unit: metric.unit.clone(),
                    quantity: match metric.quantity {
                        Quantity::IntervalTotal => "interval_total",
                        Quantity::Gauge => "gauge",
                    },
                    estimate: metric.estimate,
                    unknown_contribution: match metric.unknown_contribution {
                        UnknownContribution::Nonnegative => "nonnegative",
                        UnknownContribution::Unknown => "unknown",
                    },
                    sampled: metric.sampled,
                })
                .collect(),
            generated_at: dataset.generated_at().to_string(),
            observed_through: dataset.observed_through().map(|at| at.to_string()),
            revision: dataset.revision().map(str::to_string),
            notices_count: dataset.notices().len(),
            notices: dataset.notices().to_vec(),
        }))
    }

    pub fn unread(status: &'static str) -> Self {
        DatasetBlock::Unread { status }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBlock {
    pub kind: &'static str,
    pub text: String,
    pub stderr_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceBlock {
    pub id: String,
    pub data_origin: &'static str,
    pub clocks: Option<ClocksBlock>,
    pub request: RequestBlock,
    pub attempt: Option<AttemptBlock>,
    pub dataset: DatasetBlock,
    pub error: Option<ErrorBlock>,
    pub result: Vec<CheckBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentBlock {
    pub metric: String,
    pub title: Option<String>,
    pub summary: Option<&'static str>,
    pub group_by: Vec<String>,
    pub stack_by: Option<String>,
    pub top_n: Option<usize>,
    pub bounded_percent: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutcomeBlock {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wanted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_error: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

impl OutcomeBlock {
    fn of(kind: &'static str) -> Self {
        Self {
            kind,
            reason: None,
            subject: None,
            wanted: None,
            available: None,
            limit: None,
            count: None,
            maximum: None,
            source_error: None,
            why: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservedBlock {
    pub at: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SummaryBlock {
    pub reduction: &'static str,
    pub exact: Option<String>,
    pub quality: &'static str,
    pub bound: &'static str,
    pub known_subtotal: Option<String>,
    pub signed: bool,
    pub newest_is_null: bool,
    pub observed: Option<ObservedBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntervalsBlock {
    pub total: usize,
    pub complete: usize,
    pub partial: usize,
    pub missing: usize,
    pub open_right_edge: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupsBlock {
    pub by: Vec<String>,
    pub total: usize,
    pub named_shown: usize,
    pub other_members: usize,
    pub unknown_shown: usize,
    pub capacity: Option<usize>,
    pub legend_shown: usize,
    pub legend_overflow: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RowBlock {
    pub group: Option<Vec<String>>,
    pub role: &'static str,
    pub members: usize,
    pub exact: Option<String>,
    pub known_subtotal: Option<String>,
    pub quality: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntervalBlock {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetailRowBlock {
    pub interval: IntervalBlock,
    pub sampled: bool,
    pub group: Vec<String>,
    pub shown_as: Option<&'static str>,
    pub exact: Option<String>,
    pub known_subtotal: Option<String>,
    pub quality: &'static str,
    pub unit: String,
    pub break_before: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetailsBlock {
    pub grain: Option<&'static str>,
    pub rows_total: usize,
    pub included: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows_returned: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<DetailRowBlock>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadlineBlock {
    pub label: String,
    pub value: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DisplayBlock {
    pub title: String,
    pub subtitle: String,
    pub headline: Option<HeadlineBlock>,
    pub badges: Vec<String>,
    pub banner: Option<String>,
    pub message: Option<String>,
    pub footnote: Option<String>,
    pub accessible_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AxisBlock {
    pub min: f64,
    pub max: f64,
    pub ticks: [f64; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct DrawingBlock {
    pub authoritative: bool,
    pub axis: Option<AxisBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardBlock {
    pub id: String,
    pub source: String,
    pub kind: &'static str,
    pub intent: IntentBlock,
    pub outcome: OutcomeBlock,
    pub summary: Option<SummaryBlock>,
    pub intervals: Option<IntervalsBlock>,
    pub groups: Option<GroupsBlock>,
    pub rows: Vec<RowBlock>,
    pub details: Option<DetailsBlock>,
    pub display: Option<DisplayBlock>,
    pub drawing: Option<DrawingBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub command: Option<&'static str>,
    pub status: Status,
    pub clock: Option<ClockBlock>,
    pub config: Option<ConfigBlock>,
    pub errors: Vec<ReportError>,
    pub checks: Vec<CheckBlock>,
    pub sources: Vec<SourceBlock>,
    pub cards: Vec<CardBlock>,
}

impl Report {
    /// A report that only says why nothing was done.
    pub fn invalid(
        command: Option<&'static str>,
        config: Option<ConfigBlock>,
        errors: Vec<ReportError>,
    ) -> Self {
        Report {
            schema: REPORT_SCHEMA_ID,
            command,
            status: Status::Invalid,
            clock: None,
            config,
            errors,
            checks: Vec::new(),
            sources: Vec::new(),
            cards: Vec::new(),
        }
    }

    /// Pretty UTF-8 JSON with a trailing newline.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        text.push('\n');
        text
    }

    /// Replaces an over-bound report with the minimal one naming the first
    /// card at which the running size passes the bound.
    pub fn bounded(self) -> Report {
        let whole = self.to_json();
        if whole.len() <= REPORT_LIMIT_BYTES {
            return self;
        }
        let mut running = Report {
            cards: Vec::new(),
            ..self.clone()
        }
        .to_json()
        .len();
        let mut culprit = self.cards.first().map(|card| card.id.clone());
        for card in &self.cards {
            let pretty = serde_json::to_string_pretty(card).unwrap_or_default();
            // Each card sits two levels deep: four spaces on every line.
            running += pretty.len() + 4 * (pretty.matches('\n').count() + 1) + 2;
            if running > REPORT_LIMIT_BYTES {
                culprit = Some(card.id.clone());
                break;
            }
        }
        let mut error = ReportError::new(
            "report-too-large",
            format!(
                "the report would exceed {REPORT_LIMIT_BYTES} bytes; narrow it with --card or page --details"
            ),
        );
        error.card = culprit;
        Report::invalid(self.command, None, vec![error])
    }
}

pub fn check(id: &'static str, status: &'static str, why: Option<String>) -> CheckBlock {
    let required = CHECKS
        .iter()
        .find(|(name, _)| *name == id)
        .is_some_and(|(_, required)| *required);
    CheckBlock {
        id,
        required,
        status,
        why,
    }
}

fn kind_name(kind: ChartKind) -> &'static str {
    match kind {
        ChartKind::TimeSeries => "time-series",
        ChartKind::TimeBars => "time-bars",
        ChartKind::RankedBars => "ranked-bars",
    }
}

fn reduction_name(reduction: Reduction) -> &'static str {
    match reduction {
        Reduction::Sum => "sum",
        Reduction::Last => "last",
    }
}

fn quality_name(quality: Quality) -> &'static str {
    match quality {
        Quality::Complete => "complete",
        Quality::Partial => "partial",
        Quality::Missing => "missing",
    }
}

fn bound_name(bound: Bound) -> &'static str {
    match bound {
        Bound::Exact => "exact",
        Bound::AtLeast => "at-least",
        Bound::Incomplete => "incomplete",
    }
}

fn role_name(role: RowRole) -> &'static str {
    match role {
        RowRole::Named => "named",
        RowRole::Other => "other",
        RowRole::Unknown => "unknown",
    }
}

fn subject_name(subject: MismatchSubject) -> &'static str {
    match subject {
        MismatchSubject::Metric => "metric",
        MismatchSubject::Dimension => "dimension",
        MismatchSubject::Summary => "summary",
        MismatchSubject::BoundedPercent => "bounded-percent",
        MismatchSubject::Grouping => "grouping",
        MismatchSubject::Stacking => "stacking",
        MismatchSubject::Sampled => "sampled",
        MismatchSubject::GaugeMerge => "gauge-merge",
    }
}

fn limit_name(limit: CapacityLimit) -> &'static str {
    match limit {
        CapacityLimit::UnknownSeries => "unknown-series",
        CapacityLimit::GaugeSeries => "gauge-series",
        CapacityLimit::NumericRange => "numeric-range",
    }
}

fn outcome_block(outcome: &Outcome) -> OutcomeBlock {
    match outcome {
        Outcome::Plotted => OutcomeBlock::of("plotted"),
        Outcome::NoObservations => OutcomeBlock::of("no-observations"),
        Outcome::NoIntervals => OutcomeBlock::of("no-intervals"),
        Outcome::NoKnownValues => OutcomeBlock::of("no-known-values"),
        Outcome::Unavailable { source_error } => OutcomeBlock {
            source_error: source_error.map(|kind| kind.id()),
            ..OutcomeBlock::of("unavailable")
        },
        Outcome::Refused(Refusal::CardMismatch(mismatch)) => OutcomeBlock {
            reason: Some("card-mismatch"),
            subject: Some(subject_name(mismatch.subject)),
            wanted: mismatch.wanted.clone(),
            available: Some(mismatch.available.clone()),
            ..OutcomeBlock::of("refused")
        },
        Outcome::Refused(Refusal::Capacity(capacity)) => OutcomeBlock {
            reason: Some("capacity"),
            limit: Some(limit_name(capacity.limit)),
            count: capacity.count,
            maximum: capacity.maximum,
            ..OutcomeBlock::of("refused")
        },
    }
}

fn intent_block(intent: &CardIntent) -> IntentBlock {
    IntentBlock {
        metric: intent.metric.clone(),
        title: intent.title.clone(),
        summary: intent.summary.map(reduction_name),
        group_by: intent.group_by.clone(),
        stack_by: intent.stack_by.clone(),
        top_n: intent.top_n,
        bounded_percent: intent.bounded_percent,
    }
}

fn decimal(value: Option<&crate::chart::decimal::ChartDecimal>) -> Option<String> {
    value.map(|decimal| decimal.to_plain_string())
}

fn values(group: &[(String, String)]) -> Vec<String> {
    group.iter().map(|(_, value)| value.clone()).collect()
}

fn details_block(facts: &CardFacts, page: Option<&DetailsRequest>) -> DetailsBlock {
    let total = facts.details.rows.len();
    let grain = facts.details.grain.map(|grain| match grain {
        DetailGrain::IntervalGroup => "interval-group",
        DetailGrain::Group => "group",
    });
    let Some(page) = page else {
        return DetailsBlock {
            grain,
            rows_total: total,
            included: false,
            offset: None,
            rows_returned: None,
            complete: None,
            rows: None,
        };
    };
    let rows: Vec<DetailRowBlock> = facts
        .details
        .rows
        .iter()
        .skip(page.offset)
        .take(page.limit)
        .map(|row| DetailRowBlock {
            interval: IntervalBlock {
                start: row.interval.start.to_string(),
                end: row.interval.end.to_string(),
            },
            sampled: row.sampled,
            group: values(&row.group),
            shown_as: row.shown_as.map(role_name),
            exact: decimal(row.exact()),
            known_subtotal: decimal(row.known_subtotal.as_ref()),
            quality: quality_name(row.quality),
            unit: row.unit.clone(),
            break_before: row.break_before,
        })
        .collect();
    DetailsBlock {
        grain,
        rows_total: total,
        included: true,
        offset: Some(page.offset),
        rows_returned: Some(rows.len()),
        // The page holds every row only when it starts at the first one and reaches the last.
        complete: Some(page.offset == 0 && rows.len() == total),
        rows: Some(rows),
    }
}

fn axis(model: &crate::chart_view::ChartCardModel) -> Option<AxisBlock> {
    let scale = match model.plot() {
        PlotModel::Empty => return None,
        PlotModel::Lines { axes, .. } | PlotModel::Bars { axes, .. } => axes.scale,
        PlotModel::Ranked { scale, .. } => *scale,
    };
    Some(AxisBlock {
        min: scale.min,
        max: scale.max,
        ticks: scale.ticks(),
    })
}

/// The report block of an evaluated card: typed facts, then the model's text.
pub fn card_block(facts: &CardFacts, page: Option<&DetailsRequest>) -> CardBlock {
    let model = present_card(facts);
    CardBlock {
        id: facts.intent.id.clone(),
        source: facts.intent.source.clone(),
        kind: kind_name(facts.intent.kind),
        intent: intent_block(&facts.intent),
        outcome: outcome_block(&facts.outcome),
        summary: facts.summary.as_ref().map(|summary| SummaryBlock {
            reduction: reduction_name(summary.reduction),
            exact: decimal(summary.exact.as_ref()),
            quality: quality_name(summary.quality),
            bound: bound_name(summary.bound),
            known_subtotal: decimal(summary.known_subtotal.as_ref()),
            signed: summary.signed,
            newest_is_null: summary.newest_is_null,
            observed: summary.observed.as_ref().map(|observation| ObservedBlock {
                at: observation.interval.start.to_string(),
                end: observation.interval.end.to_string(),
            }),
        }),
        intervals: facts.intervals.as_ref().map(|intervals| IntervalsBlock {
            total: intervals.total,
            complete: intervals.complete,
            partial: intervals.partial,
            missing: intervals.missing,
            open_right_edge: intervals.open_right_edge,
        }),
        groups: facts.groups.as_ref().map(|groups| GroupsBlock {
            by: groups.by.clone(),
            total: groups.total,
            named_shown: groups.named_shown,
            other_members: groups.other_members,
            unknown_shown: groups.unknown_shown,
            capacity: groups.capacity,
            legend_shown: groups.legend_shown,
            legend_overflow: groups.legend_overflow,
        }),
        rows: facts
            .rows
            .iter()
            .map(|row| RowBlock {
                group: row.group.as_deref().map(values),
                role: role_name(row.role),
                members: row.members,
                exact: decimal(row.exact()),
                known_subtotal: decimal(row.known_subtotal()),
                quality: quality_name(row.quality),
            })
            .collect(),
        details: Some(details_block(facts, page)),
        display: Some(DisplayBlock {
            title: model.title().to_string(),
            subtitle: model.subtitle().to_string(),
            headline: model.headline().map(|headline| HeadlineBlock {
                label: headline.label.clone(),
                value: headline.value.clone(),
                note: headline.note.clone(),
            }),
            badges: model.badges().to_vec(),
            banner: model.banner().map(str::to_string),
            message: model.message().map(str::to_string),
            footnote: model.footnote().map(str::to_string),
            accessible_name: model.accessible_name().to_string(),
        }),
        drawing: Some(DrawingBlock {
            authoritative: false,
            axis: axis(&model),
        }),
    }
}

/// A card nothing evaluated: only what the configuration declares.
pub fn not_evaluated_block(card: &ChartCardConfig, intent: &CardIntent, why: String) -> CardBlock {
    CardBlock {
        id: card.id.clone(),
        source: card.source.clone(),
        kind: kind_name(card.kind),
        intent: intent_block(intent),
        outcome: OutcomeBlock {
            why: Some(why),
            ..OutcomeBlock::of("not-evaluated")
        },
        summary: None,
        intervals: None,
        groups: None,
        rows: Vec::new(),
        details: None,
        display: None,
        drawing: None,
    }
}

/// `true` for an outcome that fails the `cards` check.
pub fn outcome_fails(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Refused(_) | Outcome::Unavailable { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::schema::{
        BOUNDS, CAPACITY_LIMITS, MISMATCH_SUBJECTS, OUTCOME_KINDS, QUALITIES, ROW_ROLES,
    };

    #[test]
    fn every_spelled_name_is_in_the_schema_enumerations() {
        for subject in [
            MismatchSubject::Metric,
            MismatchSubject::Dimension,
            MismatchSubject::Summary,
            MismatchSubject::BoundedPercent,
            MismatchSubject::Grouping,
            MismatchSubject::Stacking,
            MismatchSubject::Sampled,
            MismatchSubject::GaugeMerge,
        ] {
            assert!(MISMATCH_SUBJECTS.contains(&subject_name(subject)));
        }
        assert_eq!(MISMATCH_SUBJECTS.len(), 8);
        for limit in [
            CapacityLimit::UnknownSeries,
            CapacityLimit::GaugeSeries,
            CapacityLimit::NumericRange,
        ] {
            assert!(CAPACITY_LIMITS.contains(&limit_name(limit)));
        }
        for quality in [Quality::Complete, Quality::Partial, Quality::Missing] {
            assert!(QUALITIES.contains(&quality_name(quality)));
        }
        for bound in [Bound::Exact, Bound::AtLeast, Bound::Incomplete] {
            assert!(BOUNDS.contains(&bound_name(bound)));
        }
        for role in [RowRole::Named, RowRole::Other, RowRole::Unknown] {
            assert!(ROW_ROLES.contains(&role_name(role)));
        }
        for outcome in [
            Outcome::Plotted,
            Outcome::NoObservations,
            Outcome::NoIntervals,
            Outcome::NoKnownValues,
            Outcome::Unavailable { source_error: None },
        ] {
            assert!(OUTCOME_KINDS.contains(&outcome_block(&outcome).kind));
        }
    }
}
