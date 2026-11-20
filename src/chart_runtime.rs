//! Bounded command runtime for chart sources: one scheduler thread owns the
//! pure `scheduler::Scheduler`, runs each command on its own worker thread and
//! is the only publisher of accepted datasets. No GUI toolkit is involved.

pub mod scheduler;

use crate::chart::config::{
    ChartSourceConfig, MAX_CONCURRENT_COMMANDS, Normalizer, RequestError, ResolvedRequest,
    STDERR_LIMIT_BYTES, STDOUT_LIMIT_BYTES, resolve_request_at,
};
use crate::chart::dataset::ChartDataset;
use crate::chart::error::{ChartError, ChartErrorKind, sanitize_text};
use crate::chart::state::{SourceInfo, SourceSnapshot};
use crate::chart::time::{Interval, Timestamp};
use crate::process::{CancelToken, CaptureLimits, RunFailure, run_raw};
use scheduler::{Action, Event, RunPolicy, Scheduler};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// What one finished command produced, held until the scheduler accepts it.
struct Answer {
    request_window: Option<Interval>,
    request: Option<ResolvedRequest>,
    latency: Duration,
    /// The wall instant the attempt became terminal.
    ended_at: Timestamp,
    /// Process status; `None` when it did not exit normally or never ran.
    exit: Option<i32>,
    /// Exit-0 stdout within the bound, kept only for a one-shot run.
    stdout: Option<Vec<u8>>,
    stderr_excerpt: Option<String>,
    /// Set when the request window is not open yet: nothing ran, nothing is
    /// published and the scheduler retries after this delay.
    deferred: Option<Duration>,
    result: Result<ChartDataset, ChartError>,
}

impl Answer {
    /// An attempt that never reached a process.
    fn bare(
        latency: Duration,
        deferred: Option<Duration>,
        result: Result<ChartDataset, ChartError>,
    ) -> Self {
        Answer {
            request_window: None,
            request: None,
            latency,
            ended_at: Timestamp::now(),
            exit: None,
            stdout: None,
            stderr_excerpt: None,
            deferred,
            result,
        }
    }
}

/// What a one-shot run reports for one source once it is terminal.
#[derive(Debug)]
pub struct Attempt {
    pub source_id: String,
    /// `None` when no request could be built (still deferred, or invalid).
    pub request: Option<ResolvedRequest>,
    pub ended_at: Timestamp,
    pub latency_ms: u64,
    pub exit: Option<i32>,
    /// Exit-0 stdout within the bound, byte-exact, even if it failed to parse.
    pub stdout: Option<Vec<u8>>,
    pub stderr_excerpt: Option<String>,
    /// The request window was still not open after the single retry.
    pub still_deferred: bool,
    pub result: Result<Arc<ChartDataset>, ChartError>,
}

/// A running one-shot: each selected source is attempted once.
pub struct OneShot {
    attempts: Receiver<Attempt>,
    remaining: usize,
    runtime: ChartRuntime,
}

impl OneShot {
    /// Blocks for the next terminal source; `None` when every source is terminal.
    pub fn recv(&mut self) -> Option<Attempt> {
        if self.remaining == 0 {
            return None;
        }
        let attempt = self.attempts.recv().ok()?;
        self.remaining -= 1;
        Some(attempt)
    }

    /// Stops owned process groups and reaps them.
    pub fn finish(self) {
        drop(self.runtime);
    }
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
        Self::start_with(sources, normalizer, RunPolicy::Continuous, None)
    }

    /// Runs each source once under the one-shot policy; no refresh, backoff
    /// retry or queued manual run follows a terminal attempt.
    pub fn start_one_shot(sources: &[ChartSourceConfig], normalizer: Normalizer) -> OneShot {
        let (sender, attempts) = mpsc::channel();
        let runtime = Self::start_with(sources, normalizer, RunPolicy::OneShot, Some(sender));
        OneShot {
            attempts,
            remaining: sources.len(),
            runtime,
        }
    }

    fn start_with(
        sources: &[ChartSourceConfig],
        normalizer: Normalizer,
        policy: RunPolicy,
        attempts: Option<Sender<Attempt>>,
    ) -> ChartRuntime {
        let scheduler = Scheduler::new_with_policy(sources, MAX_CONCURRENT_COMMANDS, policy);
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
            attempts,
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
    /// Set for a one-shot run: terminal attempts go here, not into snapshots.
    attempts: Option<Sender<Attempt>>,
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
                        self.handle(Event::Deferred {
                            source,
                            revision,
                            retry_after,
                        });
                        // A terminal one-shot deferral was taken by `publish`.
                        self.pending.remove(&source_id);
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
        let keep_stdout = self.attempts.is_some();
        let id = source.to_string();
        let spawned = std::thread::Builder::new()
            .name("chart-command".into())
            .spawn(move || {
                let started = Instant::now();
                let answer = catch_unwind(AssertUnwindSafe(|| {
                    execute(
                        &config,
                        revision,
                        normalizer,
                        &worker_cancel,
                        started,
                        keep_stdout,
                    )
                }))
                .unwrap_or_else(|_| {
                    Answer::bare(
                        started.elapsed(),
                        None,
                        Err(ChartError::new(
                            ChartErrorKind::ParseFailure,
                            "reading the command output failed internally",
                        )),
                    )
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
                let answer = Answer::bare(
                    Duration::ZERO,
                    None,
                    Err(ChartError::new(
                        ChartErrorKind::SpawnFailed,
                        "cannot start a worker thread",
                    )),
                );
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
        if let Some(attempts) = &self.attempts {
            let answer = *answer;
            let _ = attempts.send(Attempt {
                source_id: source.to_string(),
                request: answer.request,
                ended_at: answer.ended_at,
                latency_ms: u64::try_from(answer.latency.as_millis()).unwrap_or(u64::MAX),
                exit: answer.exit,
                stdout: answer.stdout,
                stderr_excerpt: answer.stderr_excerpt,
                still_deferred: answer.deferred.is_some(),
                result: answer.result.map(Arc::new),
            });
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

/// One attempt for both run policies: resolve the request, run the command
/// under the bounds, normalize its exit-0 output.
fn execute(
    config: &ChartSourceConfig,
    revision: u64,
    normalizer: Normalizer,
    cancel: &CancelToken,
    started: Instant,
    keep_stdout: bool,
) -> Answer {
    let request = match resolve_request_at(config, Timestamp::now(), revision) {
        Ok(request) => request,
        Err(RequestError::Deferred { retry_after }) => {
            return Answer::bare(
                started.elapsed(),
                Some(retry_after),
                Err(ChartError::new(
                    ChartErrorKind::Cancelled,
                    "the request window is not open yet",
                )),
            );
        }
        Err(RequestError::Invalid(error)) => {
            return Answer::bare(started.elapsed(), None, Err(error));
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
    let mut exit = None;
    let mut stdout = None;
    let (excerpt, result) = match run_raw(&mut command, &limits, cancel, &mut stderr_bytes) {
        Ok(captured) => {
            exit = Some(0);
            let excerpt = config
                .capture_stderr
                .then(|| stderr_excerpt(&captured.stderr))
                .flatten();
            let result = match std::str::from_utf8(&captured.stdout) {
                Ok(text) => normalizer(config, text, &request),
                Err(_) => Err(failure_error(RunFailure::NotUtf8)),
            };
            if keep_stdout {
                stdout = Some(captured.stdout);
            }
            (excerpt, result)
        }
        Err(failure) => {
            if let RunFailure::NonzeroExit(code) = failure {
                exit = code;
            }
            (
                config
                    .capture_stderr
                    .then(|| stderr_excerpt(&String::from_utf8_lossy(&stderr_bytes)))
                    .flatten(),
                Err(failure_error(failure)),
            )
        }
    };
    Answer {
        request_window: Some(request.window),
        request: Some(request),
        latency: started.elapsed(),
        ended_at: Timestamp::now(),
        exit,
        stdout,
        stderr_excerpt: excerpt,
        deferred: None,
        result,
    }
}
