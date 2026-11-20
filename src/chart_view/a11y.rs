//! Accessible names state kind, title, unit, range and freshness in words.

use super::format::range_text;
use crate::chart::config::ChartKind;
use crate::chart::dataset::ChartDataset;
use crate::chart::state::{SourceSnapshot, SourceStatus};
use crate::chart::time::Timestamp;

pub fn kind_name(kind: ChartKind) -> &'static str {
    match kind {
        ChartKind::TimeSeries => "Time series",
        ChartKind::TimeBars => "Time bars",
        ChartKind::RankedBars => "Ranked bars",
    }
}

pub fn age_text(seconds: i64) -> String {
    if seconds < 90 {
        format!("{} s ago", seconds.max(0))
    } else if seconds < 5_400 {
        format!("{} min ago", seconds / 60)
    } else {
        format!("{} h ago", seconds / 3_600)
    }
}

pub fn freshness(snapshot: &SourceSnapshot, status: SourceStatus, now: Timestamp) -> String {
    let updated = snapshot
        .info
        .last_success
        .map(|success| {
            format!(
                ", updated {}",
                age_text(now.as_second() - success.as_second())
            )
        })
        .unwrap_or_default();
    match status {
        SourceStatus::Loading => "loading".to_string(),
        SourceStatus::Fresh => format!("fresh{updated}"),
        SourceStatus::Stale => format!("stale{updated}"),
        SourceStatus::RefreshFailed { stale: false } => format!("refresh failed{updated}"),
        SourceStatus::RefreshFailed { stale: true } => {
            format!("refresh failed and stale{updated}")
        }
        SourceStatus::Unavailable => "source unavailable".to_string(),
    }
}

pub fn accessible_name(
    kind: ChartKind,
    title: &str,
    unit: Option<&str>,
    dataset: Option<&ChartDataset>,
    freshness: &str,
) -> String {
    let mut parts = vec![format!("{}: {title}", kind_name(kind))];
    if let Some(unit) = unit {
        parts.push(format!("unit {unit}"));
    }
    if let Some(dataset) = dataset {
        parts.push(format!(
            "range {}",
            range_text(&dataset.range(), dataset.timezone())
        ));
    }
    parts.push(freshness.to_string());
    parts.join(", ")
}
