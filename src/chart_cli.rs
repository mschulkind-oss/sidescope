//! `sidescope charts schema|check|replay|run`: headless chart authoring and
//! verification. Entered before any GUI or configuration code, so no verb can
//! initialise a display. Execution is the existing runtime's one-shot policy,
//! normalization is `chart_mapping::normalize_output`, reduction is
//! `chart::facts::reduce_card`; this module resolves, sequences and reports.

mod args;
mod assemble;
mod capture;
mod eval;
mod help;
mod report;
mod select;
mod text;

use crate::chart::config::ChartSourceConfig;
use crate::chart::error::ChartErrorKind;
use crate::chart::schema::{SchemaDocument, key_matrix};
use crate::chart::time::{Interval, Timestamp, parse_instant};
use crate::chart_mapping::normalize_output;
use crate::chart_runtime::{Attempt, ChartRuntime};
use args::{Format, Invocation, Parsed, Verb};
use assemble::{Assembly, assemble};
use capture::{CaptureWriter, Manifest, ManifestClocks, ManifestSource, ManifestWindow};
use eval::{ReplayInput, SourceEval, check_source, replay_source, run_source};
use report::{ClockBlock, Report, ReportError};
use select::{Loaded, select_scope};
use std::io::Write;
use std::path::Path;

/// Runs one `charts` invocation and returns the process exit status.
pub fn run(args: Vec<String>) -> i32 {
    let (text, code) = execute(&args);
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
    code
}

fn execute(args: &[String]) -> (String, i32) {
    match args::parse(args) {
        Err(usage) => finish(
            Report::invalid(
                usage.verb.map(Verb::name),
                None,
                vec![ReportError::new("usage", usage.text)],
            ),
            usage.format,
        ),
        Ok(Parsed::Help(verb)) => (help::text(verb), 0),
        Ok(Parsed::Invocation(invocation)) if invocation.verb == Verb::Schema => {
            (schema_text(&invocation), 0)
        }
        Ok(Parsed::Invocation(invocation)) => {
            let report = match select::load_config(&invocation) {
                Err(failure) => {
                    let (config, error) = *failure;
                    Report::invalid(Some(invocation.verb.name()), config, vec![error])
                }
                Ok(loaded) => {
                    let block = loaded.block.clone();
                    dispatch(&invocation, &loaded).unwrap_or_else(|error| {
                        let mut block = block;
                        block.valid = true;
                        Report::invalid(Some(invocation.verb.name()), Some(block), vec![error])
                    })
                }
            };
            finish(report, invocation.format)
        }
    }
}

fn finish(report: Report, format: Format) -> (String, i32) {
    let report = report.bounded();
    let code = report.status.exit_code();
    let text = match format {
        Format::Json => report.to_json(),
        Format::Text => text::render(&report),
    };
    (text, code)
}

fn schema_text(invocation: &Invocation) -> String {
    match invocation.format {
        Format::Text => key_matrix(),
        Format::Json => {
            let document = SchemaDocument {
                only: invocation.section.clone(),
            };
            let mut text = serde_json::to_string_pretty(&document).unwrap_or_default();
            text.push('\n');
            text
        }
    }
}

fn dispatch(invocation: &Invocation, loaded: &Loaded) -> Result<Report, ReportError> {
    match invocation.verb {
        Verb::Check => check_report(invocation, loaded),
        Verb::Replay => replay_report(invocation, loaded),
        Verb::Run => run_report(invocation, loaded),
        Verb::Schema => unreachable!("schema reads no configuration"),
    }
}

fn whole_second(at: Timestamp) -> Timestamp {
    Timestamp::from_second(at.as_second()).unwrap_or(at)
}

fn conflict(text: &str) -> ReportError {
    ReportError::new("conflicting-selection", text)
}

fn check_report(invocation: &Invocation, loaded: &Loaded) -> Result<Report, ReportError> {
    let scope = select_scope(invocation, &loaded.config, None)?;
    let (at, origin) = match invocation.at {
        Some(at) => (at, "explicit"),
        None => (whole_second(Timestamp::now()), "wall"),
    };
    let evals = scope
        .sources
        .iter()
        .map(|source| check_source(source, at))
        .collect();
    Ok(assemble(Assembly {
        verb: Verb::Check,
        clock: ClockBlock {
            at: at.to_string(),
            origin,
        },
        config: loaded.block.clone(),
        scope: &scope,
        evals,
        show_argv: invocation.show_argv,
        details: None,
        errors: Vec::new(),
    }))
}

/// Outcomes whose exit-0 output the capture keeps and replay re-normalizes.
const RETAINED_OUTCOMES: [&str; 5] = [
    "ok",
    "parse",
    "unsupported-version",
    "invalid-mapping",
    "invalid-dataset",
];

fn unreadable(text: String) -> ReportError {
    ReportError::new("data-unreadable", text)
}

fn instant(text: &str) -> Result<Timestamp, ReportError> {
    parse_instant(text).map_err(|_| unreadable(format!("{text:?} is not an instant")))
}

fn replay_report(invocation: &Invocation, loaded: &Loaded) -> Result<Report, ReportError> {
    if invocation.from_capture.is_some() && !invocation.data.is_empty() {
        return Err(conflict("--data and --from-capture cannot be combined"));
    }
    if invocation.from_capture.is_some() && invocation.at.is_some() {
        return Err(conflict(
            "--from-capture replays the recorded clocks; --at cannot be combined with it",
        ));
    }
    let manifest: Option<(std::path::PathBuf, Manifest)> = match &invocation.from_capture {
        Some(dir) => {
            let dir = Path::new(dir);
            Some((
                dir.to_path_buf(),
                capture::load_manifest(dir).map_err(unreadable)?,
            ))
        }
        None => None,
    };
    let chosen: Vec<String> = match &manifest {
        Some((_, manifest)) => manifest.sources.iter().map(|s| s.id.clone()).collect(),
        None => invocation.data.iter().map(|(id, _)| id.clone()).collect(),
    };
    let scope = select_scope(invocation, &loaded.config, Some(&chosen))?;
    let mut evals = Vec::new();
    let clock;
    match &manifest {
        Some((dir, manifest)) => {
            clock = ClockBlock {
                at: manifest.invocation_at.clone(),
                origin: "recorded",
            };
            for source in &scope.sources {
                let entry = manifest
                    .sources
                    .iter()
                    .find(|entry| entry.id == source.id)
                    .expect("scope sources come from the manifest");
                evals.push(replay_capture_entry(dir, source, entry)?);
            }
        }
        None => {
            let at = invocation.at.expect("replay --data requires --at");
            clock = ClockBlock {
                at: at.to_string(),
                origin: "explicit",
            };
            for source in &scope.sources {
                let file = invocation
                    .data
                    .iter()
                    .find(|(id, _)| *id == source.id)
                    .map(|(_, file)| file)
                    .expect("scope sources come from --data");
                let bytes = capture::read_bounded(Path::new(file))
                    .map_err(|why| unreadable(format!("{file}: {why}")))?;
                evals.push(replay_source(
                    source,
                    Some(at),
                    at,
                    ReplayInput::Bytes(bytes),
                ));
            }
        }
    }
    Ok(assemble(Assembly {
        verb: Verb::Replay,
        clock,
        config: loaded.block.clone(),
        scope: &scope,
        evals,
        show_argv: invocation.show_argv,
        details: invocation.details.as_ref(),
        errors: Vec::new(),
    }))
}

fn replay_capture_entry<'a>(
    dir: &Path,
    source: &'a ChartSourceConfig,
    entry: &ManifestSource,
) -> Result<SourceEval<'a>, ReportError> {
    let id = &source.id;
    let retained = RETAINED_OUTCOMES.contains(&entry.outcome.as_str());
    let input = if entry.captured {
        if !retained {
            return Err(unreadable(format!(
                "{id}: the manifest records outcome {} yet a kept stdout",
                entry.outcome
            )));
        }
        let bytes = capture::read_bounded(&capture::stdout_path(dir, id))
            .map_err(|why| unreadable(format!("{id}.stdout: {why}")))?;
        if bytes.len() != entry.bytes {
            return Err(unreadable(format!(
                "{id}.stdout holds {} bytes; the manifest records {}",
                bytes.len(),
                entry.bytes
            )));
        }
        ReplayInput::Bytes(bytes)
    } else if retained {
        return Err(unreadable(format!(
            "{id}: the capture kept no stdout, so outcome {} cannot be replayed",
            entry.outcome
        )));
    } else {
        match ChartErrorKind::from_source_id(&entry.outcome) {
            Some(kind) => ReplayInput::Recorded(kind),
            None => {
                return Err(unreadable(format!(
                    "{id}: unknown recorded outcome {:?}",
                    entry.outcome
                )));
            }
        }
    };
    let request_at = entry
        .clocks
        .request_at
        .as_deref()
        .map(instant)
        .transpose()?;
    let model_at = instant(&entry.clocks.model_at)?;
    let eval = replay_source(source, request_at, model_at, input);
    // The recorded window must be what this configuration would request.
    let recorded = entry
        .window
        .as_ref()
        .map(|window| Ok::<_, ReportError>((instant(&window.from)?, instant(&window.to)?)))
        .transpose()?;
    let rebuilt = eval
        .request
        .as_ref()
        .map(|request| (request.window.start, request.window.end));
    if recorded != rebuilt || (rebuilt.is_some() && entry.timezone != source.timezone) {
        return Err(unreadable(format!(
            "{id}: the capture was recorded for a different request window or timezone than this configuration resolves"
        )));
    }
    Ok(eval)
}

fn run_report(invocation: &Invocation, loaded: &Loaded) -> Result<Report, ReportError> {
    if invocation.all_sources && !invocation.sources.is_empty() {
        return Err(conflict("--source and --all-sources cannot be combined"));
    }
    let chosen = (!invocation.all_sources).then_some(invocation.sources.as_slice());
    let scope = select_scope(invocation, &loaded.config, chosen)?;
    let ids: Vec<&str> = scope.sources.iter().map(|s| s.id.as_str()).collect();
    let mut writer = match &invocation.capture {
        Some(dir) => {
            capture::preflight(Path::new(dir), &ids)?;
            Some(CaptureWriter::create(Path::new(dir))?)
        }
        None => None,
    };

    // Nothing above spawned or wrote anything; below, each selected source runs once.
    let invocation_at = whole_second(Timestamp::now());
    let configs: Vec<ChartSourceConfig> = scope.sources.iter().map(|s| (*s).clone()).collect();
    let mut one_shot = ChartRuntime::start_one_shot(&configs, normalize_output);
    let mut attempts: Vec<(Attempt, bool)> = Vec::new();
    while let Some(attempt) = one_shot.recv() {
        let kept = match (&mut writer, &attempt.stdout) {
            (Some(writer), Some(bytes)) => writer.write_stdout(&attempt.source_id, bytes),
            _ => false,
        };
        attempts.push((attempt, kept));
    }
    one_shot.finish();

    let mut evals = Vec::new();
    let mut manifest_sources = Vec::new();
    for source in &scope.sources {
        let found = attempts.iter().position(|(a, _)| a.source_id == source.id);
        let (attempt, kept) = match found {
            Some(index) => attempts.swap_remove(index),
            None => (lost_attempt(&source.id), false),
        };
        let eval = run_source(source, &attempt);
        manifest_sources.push(ManifestSource {
            id: source.id.clone(),
            window: attempt
                .request
                .as_ref()
                .map(|request| window(request.window)),
            timezone: source.timezone.clone(),
            clocks: ManifestClocks {
                request_at: eval.clocks.request_at.map(|at| at.to_string()),
                model_at: eval.clocks.model_at.to_string(),
            },
            bytes: if kept {
                attempt.stdout.as_ref().map_or(0, Vec::len)
            } else {
                0
            },
            captured: kept,
            outcome: match &attempt.result {
                Ok(_) => "ok".to_string(),
                Err(error) => error.kind.id().to_string(),
            },
        });
        evals.push(eval);
    }
    let mut errors = Vec::new();
    if let Some(mut writer) = writer {
        writer.sources = manifest_sources;
        writer.write_manifest(invocation_at.to_string());
        errors = writer.failures;
    }
    Ok(assemble(Assembly {
        verb: Verb::Run,
        clock: ClockBlock {
            at: invocation_at.to_string(),
            origin: "wall",
        },
        config: loaded.block.clone(),
        scope: &scope,
        evals,
        show_argv: invocation.show_argv,
        details: invocation.details.as_ref(),
        errors,
    }))
}

fn window(window: Interval) -> ManifestWindow {
    ManifestWindow {
        from: window.start.to_string(),
        to: window.end.to_string(),
    }
}

/// The runtime ended before a source reported; say so rather than invent data.
fn lost_attempt(source: &str) -> Attempt {
    Attempt {
        source_id: source.to_string(),
        request: None,
        ended_at: Timestamp::now(),
        latency_ms: 0,
        exit: None,
        stdout: None,
        stderr_excerpt: None,
        still_deferred: false,
        result: Err(crate::chart::error::ChartError::new(
            ChartErrorKind::Cancelled,
            "the runtime ended before this source finished",
        )),
    }
}
