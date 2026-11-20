//! Per-source snapshot published by the runtime and consumed by views.

use super::dataset::ChartDataset;
use super::error::ChartError;
use super::time::{Interval, Timestamp};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    /// Name only, never a path.
    pub executable: String,
    pub last_attempt: Option<Timestamp>,
    pub last_success: Option<Timestamp>,
    pub next_retry: Option<Timestamp>,
    pub latency_ms: Option<u64>,
    pub revision: u64,
    pub retry_suspended: bool,
    pub request_window: Option<Interval>,
    pub timezone: String,
    pub stale_after_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub source_id: String,
    /// Last accepted answer, replaced whole.
    pub dataset: Option<Arc<ChartDataset>>,
    /// Error of the latest attempt; `None` after a success.
    pub last_error: Option<ChartError>,
    /// Present only when the source opted into `capture_stderr`.
    pub stderr_excerpt: Option<String>,
    pub info: SourceInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Loading,
    Fresh,
    Stale,
    RefreshFailed { stale: bool },
    Unavailable,
}

impl SourceSnapshot {
    pub fn status(&self, now: Timestamp) -> SourceStatus {
        let Some(_) = &self.dataset else {
            return if self.last_error.is_some() {
                SourceStatus::Unavailable
            } else {
                SourceStatus::Loading
            };
        };
        let stale = match self.info.last_success {
            Some(success) => {
                now.as_second().saturating_sub(success.as_second())
                    > i64::try_from(self.info.stale_after_seconds).unwrap_or(i64::MAX)
            }
            None => true,
        };
        match (self.last_error.is_some(), stale) {
            (true, stale) => SourceStatus::RefreshFailed { stale },
            (false, true) => SourceStatus::Stale,
            (false, false) => SourceStatus::Fresh,
        }
    }
}
