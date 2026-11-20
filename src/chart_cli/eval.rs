//! What one source contributed to a report: its request, attempt, normalized
//! answer and the checks that follow from them. Execution, normalization and
//! card reduction stay in their own modules; this only gathers their results.

use crate::chart::config::{ChartSourceConfig, RequestError, ResolvedRequest, resolve_request_at};
use crate::chart::dataset::ChartDataset;
use crate::chart::error::{ChartError, ChartErrorKind};
use crate::chart::state::{SourceInfo, SourceSnapshot};
use crate::chart::time::{Interval, Timestamp};
use crate::chart_mapping::normalize_output;
use crate::chart_runtime::Attempt;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Step {
    pub status: &'static str,
    pub why: Option<String>,
}

impl Step {
    pub fn pass() -> Self {
        Self {
            status: "pass",
            why: None,
        }
    }

    pub fn fail(why: impl Into<String>) -> Self {
        Self {
            status: "fail",
            why: Some(why.into()),
        }
    }

    pub fn unknown(why: impl Into<String>) -> Self {
        Self {
            status: "unknown",
            why: Some(why.into()),
        }
    }

    pub fn not_run(why: impl Into<String>) -> Self {
        Self {
            status: "not-run",
            why: Some(why.into()),
        }
    }

    /// `Some(not-run)` naming the earlier check when it did not pass.
    pub fn blocks(&self, id: &str) -> Option<Step> {
        match self.status {
            "pass" => None,
            "fail" => Some(Step::not_run(format!("dependency failed: {id}"))),
            "unknown" => Some(Step::not_run(format!("dependency unknown: {id}"))),
            // Downstream of a skipped check: keep naming the original cause.
            _ => match &self.why {
                Some(why) if why.starts_with("dependency ") => Some(self.clone()),
                _ => Some(Step::not_run(format!("dependency not run: {id}"))),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Clocks {
    pub request_at: Option<Timestamp>,
    pub model_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct ErrorView {
    pub kind: ChartErrorKind,
    pub text: String,
    pub stderr_excerpt: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct AttemptView {
    pub latency_ms: u64,
    pub exit: Option<i32>,
}

pub struct SourceEval<'a> {
    pub config: &'a ChartSourceConfig,
    /// `none`, `replay` or `run`.
    pub data_origin: &'static str,
    pub clocks: Clocks,
    pub request: Option<ResolvedRequest>,
    pub request_step: Step,
    pub attempt: Option<AttemptView>,
    pub execution: Step,
    pub dataset: Step,
    pub freshness: Step,
    pub coverage: Step,
    /// `valid`, `absent` or `refused`.
    pub dataset_status: &'static str,
    pub dataset_value: Option<Arc<ChartDataset>>,
    pub error: Option<ErrorView>,
    /// Present only when the request passed and there is something to build
    /// cards from (an answer or an error).
    pub snapshot: Option<SourceSnapshot>,
}

pub fn executable_name(config: &ChartSourceConfig) -> String {
    let name = config.executable();
    name.rsplit('/').next().unwrap_or(name).to_string()
}

enum Resolved {
    Request(Box<ResolvedRequest>),
    Deferred,
    Invalid(String),
}

fn resolve(config: &ChartSourceConfig, at: Timestamp) -> Resolved {
    match resolve_request_at(config, at, 0) {
        Ok(request) => Resolved::Request(Box::new(request)),
        Err(RequestError::Deferred { .. }) => Resolved::Deferred,
        Err(RequestError::Invalid(error)) => Resolved::Invalid(error.message().to_string()),
    }
}

fn request_step(resolved: &Resolved) -> Step {
    match resolved {
        Resolved::Request(_) => Step::pass(),
        Resolved::Deferred => Step::unknown("the request window is not open yet at this instant"),
        Resolved::Invalid(message) => Step::fail(message.clone()),
    }
}

fn base<'a>(
    config: &'a ChartSourceConfig,
    data_origin: &'static str,
    clocks: Clocks,
) -> SourceEval<'a> {
    SourceEval {
        config,
        data_origin,
        clocks,
        request: None,
        request_step: Step::pass(),
        attempt: None,
        execution: Step::not_run("check does not run commands"),
        dataset: Step::not_run("check reads no data"),
        freshness: Step::not_run("check does not judge age"),
        coverage: Step::not_run("coverage is judged only for a live run"),
        dataset_status: "absent",
        dataset_value: None,
        error: None,
        snapshot: None,
    }
}

/// `charts check`: the request only.
pub fn check_source<'a>(config: &'a ChartSourceConfig, at: Timestamp) -> SourceEval<'a> {
    let resolved = resolve(config, at);
    let mut eval = base(
        config,
        "none",
        Clocks {
            request_at: Some(at),
            model_at: at,
        },
    );
    eval.request_step = request_step(&resolved);
    if let Resolved::Request(request) = resolved {
        eval.request = Some(*request);
    }
    eval
}

/// Normalizes exit-0 bytes exactly as the runtime does.
fn normalize(
    config: &ChartSourceConfig,
    bytes: &[u8],
    request: &ResolvedRequest,
) -> Result<ChartDataset, ChartError> {
    match std::str::from_utf8(bytes) {
        Ok(text) => normalize_output(config, text, request),
        Err(_) => Err(ChartError::new(
            ChartErrorKind::ParseFailure,
            "the command output is not UTF-8",
        )),
    }
}

pub fn blank_snapshot(config: &ChartSourceConfig) -> SourceSnapshot {
    snapshot(config, None, None, None, None)
}

fn snapshot(
    config: &ChartSourceConfig,
    window: Option<Interval>,
    dataset: Option<Arc<ChartDataset>>,
    error: Option<ChartError>,
    stderr_excerpt: Option<String>,
) -> SourceSnapshot {
    SourceSnapshot {
        source_id: config.id.clone(),
        dataset,
        last_error: error,
        stderr_excerpt,
        info: SourceInfo {
            executable: executable_name(config),
            last_attempt: None,
            last_success: None,
            next_retry: None,
            latency_ms: None,
            revision: 0,
            retry_suspended: false,
            request_window: window,
            timezone: config.timezone.clone(),
            stale_after_seconds: config.stale_after().as_secs(),
        },
    }
}

fn view(error: &ChartError, stderr_excerpt: Option<String>) -> ErrorView {
    ErrorView {
        kind: error.kind,
        text: error.message().to_string(),
        stderr_excerpt,
    }
}

/// Saved output for one source.
pub enum ReplayInput {
    Bytes(Vec<u8>),
    /// The capture recorded this failure and kept no output.
    Recorded(ChartErrorKind),
}

/// `charts replay`: normalize saved output at the given clocks; nothing runs.
pub fn replay_source<'a>(
    config: &'a ChartSourceConfig,
    request_at: Option<Timestamp>,
    model_at: Timestamp,
    input: ReplayInput,
) -> SourceEval<'a> {
    let resolved = match request_at {
        Some(at) => resolve(config, at),
        None => Resolved::Deferred,
    };
    let mut eval = base(
        config,
        "replay",
        Clocks {
            request_at,
            model_at,
        },
    );
    eval.request_step = request_step(&resolved);
    eval.execution = Step::not_run("replay does not run commands");
    eval.freshness = Step::not_run("replayed data has no live age");
    eval.coverage = Step::not_run("coverage is judged only for a live run");
    let Resolved::Request(request) = resolved else {
        eval.dataset = eval
            .request_step
            .blocks("request")
            .unwrap_or_else(|| Step::not_run("no request"));
        return eval;
    };
    let window = request.window;
    let (result, retained) = match &input {
        ReplayInput::Bytes(bytes) => (normalize(config, bytes, &request), true),
        ReplayInput::Recorded(kind) => (
            Err(ChartError::new(
                *kind,
                "recorded in the capture; no output was kept",
            )),
            false,
        ),
    };
    eval.request = Some(*request);
    match result {
        Ok(dataset) => {
            let dataset = Arc::new(dataset);
            eval.dataset = Step::pass();
            eval.dataset_status = "valid";
            eval.snapshot = Some(snapshot(
                config,
                Some(window),
                Some(dataset.clone()),
                None,
                None,
            ));
            eval.dataset_value = Some(dataset);
        }
        Err(error) => {
            eval.dataset = Step::fail(error.kind.id());
            eval.dataset_status = if retained { "refused" } else { "absent" };
            eval.error = Some(view(&error, None));
            eval.snapshot = Some(snapshot(config, Some(window), None, Some(error), None));
        }
    }
    eval
}

/// `charts run`: one terminal attempt of the existing runtime.
pub fn run_source<'a>(config: &'a ChartSourceConfig, attempt: &Attempt) -> SourceEval<'a> {
    let model_at = Timestamp::from_second(attempt.ended_at.as_second()).unwrap_or(attempt.ended_at);
    let request_at = attempt.request.as_ref().map(|request| request.window.end);
    let mut eval = base(
        config,
        "run",
        Clocks {
            request_at,
            model_at,
        },
    );
    eval.request = attempt.request.clone();
    eval.request_step = match (&attempt.request, attempt.still_deferred) {
        (Some(_), _) => Step::pass(),
        (None, true) => Step::unknown("the request window was still not open after one retry"),
        (None, false) => Step::fail(match &attempt.result {
            Err(error) => error.message().to_string(),
            Ok(_) => "the request could not be built".to_string(),
        }),
    };
    if let Some(blocked) = eval.request_step.blocks("request") {
        eval.execution = blocked.clone();
        eval.dataset = blocked.clone();
        eval.freshness = blocked.clone();
        eval.coverage = blocked;
        return eval;
    }
    eval.attempt = Some(AttemptView {
        latency_ms: attempt.latency_ms,
        exit: attempt.exit,
    });
    let window = attempt.request.as_ref().map(|request| request.window);
    let ran = attempt.stdout.is_some();
    match &attempt.result {
        Ok(dataset) => {
            eval.execution = Step::pass();
            eval.dataset = Step::pass();
            eval.dataset_status = "valid";
            eval.dataset_value = Some(dataset.clone());
            eval.coverage = if dataset.observed_through().is_some() {
                Step::pass()
            } else {
                Step::unknown("the producer asserts no observed_through watermark")
            };
            let mut snap = snapshot(
                config,
                window,
                Some(dataset.clone()),
                None,
                attempt.stderr_excerpt.clone(),
            );
            snap.info.last_attempt = Some(attempt.ended_at);
            snap.info.last_success = Some(attempt.ended_at);
            snap.info.latency_ms = Some(attempt.latency_ms);
            eval.snapshot = Some(snap);
        }
        Err(error) => {
            eval.execution = if ran {
                Step::pass()
            } else {
                Step::fail(error.kind.id())
            };
            eval.dataset = match eval.execution.blocks("execution") {
                Some(blocked) => blocked,
                None => Step::fail(error.kind.id()),
            };
            eval.coverage = match eval.dataset.blocks("dataset") {
                Some(blocked) => blocked,
                None => Step::not_run("no dataset"),
            };
            eval.dataset_status = if ran { "refused" } else { "absent" };
            eval.error = Some(view(error, attempt.stderr_excerpt.clone()));
            let mut snap = snapshot(
                config,
                window,
                None,
                Some(error.clone()),
                attempt.stderr_excerpt.clone(),
            );
            snap.info.last_attempt = Some(attempt.ended_at);
            snap.info.latency_ms = Some(attempt.latency_ms);
            eval.snapshot = Some(snap);
        }
    }
    eval.freshness = match eval.execution.blocks("execution") {
        Some(blocked) => blocked,
        None => {
            let age = model_at.as_second() - attempt.ended_at.as_second();
            if age <= i64::try_from(config.stale_after().as_secs()).unwrap_or(i64::MAX) {
                Step::pass()
            } else {
                Step::fail("the attempt is older than stale_after_seconds")
            }
        }
    };
    eval
}
