//! Bounded command runtime for chart sources: one scheduler thread owns the
//! pure `scheduler::Scheduler`, runs each command on its own worker thread and
//! is the only publisher of accepted datasets. No GUI toolkit is involved.

pub mod scheduler;

use crate::chart::config::{
    ChartSourceConfig, MAX_CONCURRENT_COMMANDS, Normalizer, RequestError, STDERR_LIMIT_BYTES,
    STDOUT_LIMIT_BYTES, resolve_request_at,
};
use crate::chart::dataset::ChartDataset;
use crate::chart::error::{ChartError, ChartErrorKind, sanitize_text};
use crate::chart::state::{SourceInfo, SourceSnapshot};
use crate::chart::time::{Interval, Timestamp};
use crate::process::{CancelToken, CaptureLimits, RunFailure, run_with_stderr};
use scheduler::{Action, Event, Scheduler};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// What one finished command produced, held until the scheduler accepts it.
struct Answer {
    request_window: Option<Interval>,
    latency: Duration,
    stderr_excerpt: Option<String>,
    /// Set when the request window is not open yet: nothing ran, nothing is
    /// published and the scheduler retries after this delay.
    deferred: Option<Duration>,
    result: Result<ChartDataset, ChartError>,
}

enum Message {
    Event(Event),
    Done {
        source: String,
        revision: u64,
        answer: Box<Answer>,
    },
    Shutdown,
}

struct SourceState {
    config: ChartSourceConfig,
    snapshot: SourceSnapshot,
    changed: bool,
}

#[derive(Default)]
struct Shared {
    sources: Vec<SourceState>,
}

pub struct ChartRuntime {
    shared: Arc<Mutex<Shared>>,
    sender: Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn executable_name(source: &ChartSourceConfig) -> String {
    let name = source.executable();
    name.rsplit('/').next().unwrap_or(name).to_string()
}

fn fresh_state(config: &ChartSourceConfig, revision: u64) -> SourceState {
    SourceState {
        snapshot: SourceSnapshot {
            source_id: config.id.clone(),
            dataset: None,
            last_error: None,
            stderr_excerpt: None,
            info: SourceInfo {
                executable: executable_name(config),
                last_attempt: None,
                last_success: None,
                next_retry: None,
                latency_ms: None,
                revision,
                retry_suspended: false,
                request_window: None,
                timezone: config.timezone.clone(),
                stale_after_seconds: config.stale_after().as_secs(),
            },
        },
        config: config.clone(),
        changed: true,
    }
}

impl ChartRuntime {
    /// Spawns the scheduler thread. Does not run any command for an empty source list.
    pub fn start(sources: &[ChartSourceConfig], normalizer: Normalizer) -> ChartRuntime {
        let scheduler = Scheduler::new(sources, MAX_CONCURRENT_COMMANDS);
        let shared = Arc::new(Mutex::new(Shared {
            sources: sources
                .iter()
                .map(|config| fresh_state(config, scheduler.revision(&config.id).unwrap_or(0)))
                .collect(),
        }));
        let (sender, receiver) = mpsc::channel();
        let mut driver = Driver {
            scheduler,
            shared: Arc::clone(&shared),
            normalizer,
            sender: sender.clone(),
            origin: Instant::now(),
            running: HashMap::new(),
            pending: HashMap::new(),
        };
        let thread = std::thread::Builder::new()
            .name("chart-scheduler".into())
            .spawn(move || driver.run(receiver))
            .ok();
        ChartRuntime {
            shared,
            sender,
            thread,
        }
    }

    /// Snapshots changed since the previous call, in source order.
    pub fn poll(&self) -> Vec<SourceSnapshot> {
        let mut shared = lock(&self.shared);
        let mut changed = Vec::new();
        for state in &mut shared.sources {
            if std::mem::take(&mut state.changed) {
                changed.push(state.snapshot.clone());
            }
        }
        changed
    }

    /// Latest snapshot of every source, in config order.
    pub fn snapshots(&self) -> Vec<SourceSnapshot> {
        lock(&self.shared)
            .sources
            .iter()
            .map(|state| state.snapshot.clone())
            .collect()
    }

    /// Coalesces: at most one refresh stays queued behind a running command.
    pub fn refresh_now(&self, source_id: &str) {
        let _ = self.sender.send(Message::Event(Event::RefreshRequested {
            source: source_id.to_string(),
        }));
    }

    /// Resumes a source whose scheduled retry was suspended.
    pub fn retry(&self, source_id: &str) {
        let _ = self.sender.send(Message::Event(Event::RetryRequested {
            source: source_id.to_string(),
        }));
    }

    /// Cancels in-flight commands of changed sources, bumps their revision and
    /// clears their cache. An identical source entry is left untouched.
    pub fn reconfigure(&self, sources: &[ChartSourceConfig]) {
        let _ = self.sender.send(Message::Event(Event::Reconfigured {
            sources: sources.to_vec(),
        }));
    }

    /// Stops owned process groups and reaps them; `Drop` does the same.
    pub fn shutdown(self) {}
}

impl Drop for ChartRuntime {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Driver {
    scheduler: Scheduler,
    shared: Arc<Mutex<Shared>>,
    normalizer: Normalizer,
    sender: Sender<Message>,
    origin: Instant,
    running: HashMap<(String, u64), (CancelToken, JoinHandle<()>)>,
    pending: HashMap<String, (u64, Box<Answer>)>,
}

impl Driver {
    fn run(&mut self, receiver: mpsc::Receiver<Message>) {
        self.handle(Event::Tick);
        loop {
            let now = self.origin.elapsed();
            let message = match self.scheduler.next_wake() {
                Some(wake) => match receiver.recv_timeout(wake.saturating_sub(now)) {
                    Ok(message) => Some(message),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => Some(Message::Shutdown),
                },
                None => Some(receiver.recv().unwrap_or(Message::Shutdown)),
            };
            match message {
                None => self.handle(Event::Tick),
                Some(Message::Event(event)) => self.handle(event),
                Some(Message::Done {
                    source,
                    revision,
                    answer,
                }) => {
                    if let Some((_, handle)) = self.running.remove(&(source.clone(), revision)) {
                        let _ = handle.join();
                    }
                    let outcome = answer.result.as_ref().map(|_| ()).map_err(Clone::clone);
                    let source_id = source.clone();
                    let deferred = answer.deferred;
                    self.pending.insert(source.clone(), (revision, answer));
                    if let Some(retry_after) = deferred {
                        self.pending.remove(&source_id);
                        self.handle(Event::Deferred {
                            source,
                            revision,
                            retry_after,
                        });
                        continue;
                    }
                    self.handle(Event::Finished {
                        source,
                        revision,
                        outcome,
                    });
                    // Accepted answers were taken by `publish`; the rest are obsolete.
                    self.pending.remove(&source_id);
                }
                Some(Message::Shutdown) => break,
            }
        }
        for (cancel, _) in self.running.values() {
            cancel.cancel();
        }
        for (_, (_, handle)) in self.running.drain() {
            let _ = handle.join();
        }
    }

    fn handle(&mut self, event: Event) {
        let reconfigured = match &event {
            Event::Reconfigured { sources } => Some(sources.clone()),
            _ => None,
        };
        let actions = self.scheduler.step(self.origin.elapsed(), event);
        if let Some(sources) = reconfigured {
            self.rebuild_sources(&sources);
        }
        for action in actions {
            match action {
                Action::Start { source, revision } => self.start(&source, revision),
                Action::Cancel { source, revision } => {
                    if let Some((cancel, _)) = self.running.get(&(source, revision)) {
                        cancel.cancel();
                    }
                }
                Action::Publish { source } => self.publish(&source),
            }
        }
    }

    /// Unchanged entries keep their snapshot; new or changed ones restart empty.
    fn rebuild_sources(&mut self, sources: &[ChartSourceConfig]) {
        let mut shared = lock(&self.shared);
        let mut previous = std::mem::take(&mut shared.sources);
        for config in sources {
            let kept = previous
                .iter()
                .position(|state| state.config.id == config.id && state.config == *config)
                .map(|index| previous.remove(index));
            let state = kept.unwrap_or_else(|| {
                fresh_state(config, self.scheduler.revision(&config.id).unwrap_or(0))
            });
            shared.sources.push(state);
        }
        drop(shared);
        // A stashed answer of a replaced source can never match again.
        let ids: Vec<&str> = sources.iter().map(|config| config.id.as_str()).collect();
        self.pending.retain(|id, _| ids.contains(&id.as_str()));
    }

    fn start(&mut self, source: &str, revision: u64) {
        let Some(config) = lock(&self.shared)
            .sources
            .iter()
            .find(|state| state.config.id == source)
            .map(|state| state.config.clone())
        else {
            return;
        };
        let now = Timestamp::now();
        self.with_state(source, |state| {
            state.snapshot.info.last_attempt = Some(now);
        });
        let cancel = CancelToken::new();
        let worker_cancel = cancel.clone();
        let sender = self.sender.clone();
        let normalizer = self.normalizer;
        let id = source.to_string();
        let spawned = std::thread::Builder::new()
            .name("chart-command".into())
            .spawn(move || {
                let started = Instant::now();
                let answer = catch_unwind(AssertUnwindSafe(|| {
                    execute(&config, revision, normalizer, &worker_cancel, started)
                }))
                .unwrap_or_else(|_| Answer {
                    request_window: None,
                    latency: started.elapsed(),
                    stderr_excerpt: None,
                    deferred: None,
                    result: Err(ChartError::new(
                        ChartErrorKind::ParseFailure,
                        "reading the command output failed internally",
                    )),
                });
                let _ = sender.send(Message::Done {
                    source: id,
                    revision,
                    answer: Box::new(answer),
                });
            });
        match spawned {
            Ok(handle) => {
                self.running
                    .insert((source.to_string(), revision), (cancel, handle));
            }
            Err(_) => {
                let answer = Answer {
                    request_window: None,
                    latency: Duration::ZERO,
                    stderr_excerpt: None,
                    deferred: None,
                    result: Err(ChartError::new(
                        ChartErrorKind::SpawnFailed,
                        "cannot start a worker thread",
                    )),
                };
                let _ = self.sender.send(Message::Done {
                    source: source.to_string(),
                    revision,
                    answer: Box::new(answer),
                });
            }
        }
    }

    fn with_state(&self, source: &str, apply: impl FnOnce(&mut SourceState)) {
        if let Some(state) = lock(&self.shared)
            .sources
            .iter_mut()
            .find(|state| state.config.id == source)
        {
            apply(state);
        }
    }

    fn publish(&mut self, source: &str) {
        let Some((revision, answer)) = self.pending.remove(source) else {
            return;
        };
        if self.scheduler.revision(source) != Some(revision) {
            return;
        }
        let next_due = self.scheduler.next_due(source);
        let suspended = self.scheduler.is_suspended(source);
        let elapsed = self.origin.elapsed();
        let now = Timestamp::now();
        self.with_state(source, |state| {
            let info = &mut state.snapshot.info;
            info.latency_ms = Some(u64::try_from(answer.latency.as_millis()).unwrap_or(u64::MAX));
            info.request_window = answer.request_window;
            info.revision = revision;
            info.retry_suspended = suspended;
            info.next_retry = next_due.and_then(|due| {
                let wait = due.saturating_sub(elapsed);
                now.checked_add(jiff::SignedDuration::try_from(wait).ok()?)
                    .ok()
            });
            state.snapshot.stderr_excerpt = answer.stderr_excerpt;
            match answer.result {
                Ok(dataset) => {
                    info.last_success = Some(now);
                    state.snapshot.dataset = Some(Arc::new(dataset));
                    state.snapshot.last_error = None;
                }
                Err(error) => state.snapshot.last_error = Some(error),
            }
            state.changed = true;
        });
    }
}

fn failure_error(failure: RunFailure) -> ChartError {
    let (kind, message) = match failure {
        RunFailure::MissingExecutable => (
            ChartErrorKind::MissingExecutable,
            "the command executable was not found".to_string(),
        ),
        RunFailure::SpawnFailed => (
            ChartErrorKind::SpawnFailed,
            "the command could not be started".to_string(),
        ),
        RunFailure::Timeout => (
            ChartErrorKind::Timeout,
            "the command exceeded its timeout".to_string(),
        ),
        RunFailure::OutputBound => (
            ChartErrorKind::OutputBound,
            "the command output exceeded the size limit".to_string(),
        ),
        RunFailure::NonzeroExit(Some(code)) => (
            ChartErrorKind::NonzeroExit,
            format!("the command exited with status {code}"),
        ),
        RunFailure::NonzeroExit(None) => (
            ChartErrorKind::NonzeroExit,
            "the command was ended by a signal".to_string(),
        ),
        RunFailure::NotUtf8 => (
            ChartErrorKind::ParseFailure,
            "the command output is not UTF-8".to_string(),
        ),
        RunFailure::Cancelled => (
            ChartErrorKind::Cancelled,
            "the command was cancelled".to_string(),
        ),
    };
    ChartError::new(kind, message)
}

fn stderr_excerpt(text: &str) -> Option<String> {
    let flattened: String = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let clean = sanitize_text(flattened.trim(), STDERR_LIMIT_BYTES);
    (!clean.is_empty()).then_some(clean)
}

fn execute(
    config: &ChartSourceConfig,
    revision: u64,
    normalizer: Normalizer,
    cancel: &CancelToken,
    started: Instant,
) -> Answer {
    let request = match resolve_request_at(config, Timestamp::now(), revision) {
        Ok(request) => request,
        Err(RequestError::Deferred { retry_after }) => {
            return Answer {
                request_window: None,
                latency: started.elapsed(),
                stderr_excerpt: None,
                deferred: Some(retry_after),
                result: Err(ChartError::new(
                    ChartErrorKind::Cancelled,
                    "the request window is not open yet",
                )),
            };
        }
        Err(RequestError::Invalid(error)) => {
            return Answer {
                request_window: None,
                latency: started.elapsed(),
                stderr_excerpt: None,
                deferred: None,
                result: Err(error),
            };
        }
    };
    let mut command = Command::new(&request.argv[0]);
    command.args(&request.argv[1..]);
    let limits = CaptureLimits {
        stdout: STDOUT_LIMIT_BYTES,
        stderr: if config.capture_stderr {
            STDERR_LIMIT_BYTES
        } else {
            0
        },
        timeout: config.timeout(),
    };
    let mut stderr_bytes = Vec::new();
    let (excerpt, result) = match run_with_stderr(&mut command, &limits, cancel, &mut stderr_bytes)
    {
        Ok(captured) => (
            config
                .capture_stderr
                .then(|| stderr_excerpt(&captured.stderr))
                .flatten(),
            normalizer(config, &captured.stdout, &request),
        ),
        Err(failure) => (
            config
                .capture_stderr
                .then(|| stderr_excerpt(&String::from_utf8_lossy(&stderr_bytes)))
                .flatten(),
            Err(failure_error(failure)),
        ),
    };
    Answer {
        request_window: Some(request.window),
        latency: started.elapsed(),
        stderr_excerpt: excerpt,
        deferred: None,
        result,
    }
}
