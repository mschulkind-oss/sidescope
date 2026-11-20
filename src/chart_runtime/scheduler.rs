//! Pure scheduling state machine: no threads, no clock, no I/O. Time is the
//! monotonic `Duration` the caller injects with every step.

use crate::chart::config::ChartSourceConfig;
use crate::chart::error::ChartError;
use std::time::Duration;

/// Seconds to wait after the first, second, third and later consecutive
/// failures; never sooner than the source's own refresh interval.
const BACKOFF_SECONDS: [u64; 4] = [30, 60, 120, 300];

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Tick,
    RefreshRequested {
        source: String,
    },
    RetryRequested {
        source: String,
    },
    Reconfigured {
        sources: Vec<ChartSourceConfig>,
    },
    Finished {
        source: String,
        revision: u64,
        outcome: Result<(), ChartError>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Start {
        source: String,
        revision: u64,
    },
    Cancel {
        source: String,
        revision: u64,
    },
    /// The finished answer carries the current revision: apply it.
    Publish {
        source: String,
    },
}

#[derive(Debug)]
struct Entry {
    config: ChartSourceConfig,
    revision: u64,
    running: Option<u64>,
    due: Option<Duration>,
    failures: u32,
    suspended: bool,
    queued_refresh: bool,
}

pub struct Scheduler {
    entries: Vec<Entry>,
    /// Cancelled runs whose workers have not reported yet; they still hold a
    /// concurrency slot and block a new run of the same source.
    draining: Vec<(String, u64)>,
    max_concurrent: usize,
    next_revision: u64,
}

impl Scheduler {
    pub fn new(sources: &[ChartSourceConfig], max_concurrent: usize) -> Scheduler {
        let mut scheduler = Scheduler {
            entries: Vec::new(),
            draining: Vec::new(),
            max_concurrent: max_concurrent.max(1),
            next_revision: 1,
        };
        for source in sources {
            let entry = scheduler.fresh_entry(source.clone(), Duration::ZERO);
            scheduler.entries.push(entry);
        }
        scheduler
    }

    fn fresh_entry(&mut self, config: ChartSourceConfig, due: Duration) -> Entry {
        let revision = self.next_revision;
        // Revisions never repeat, so a late answer from a removed or replaced
        // source cannot match a newer one.
        self.next_revision += 1;
        Entry {
            config,
            revision,
            running: None,
            due: Some(due),
            failures: 0,
            suspended: false,
            queued_refresh: false,
        }
    }

    pub fn revision(&self, source: &str) -> Option<u64> {
        self.find(source).map(|entry| entry.revision)
    }

    /// Monotonic time of the next scheduled run; `None` when suspended or running.
    pub fn next_due(&self, source: &str) -> Option<Duration> {
        self.find(source).and_then(|entry| entry.due)
    }

    pub fn is_suspended(&self, source: &str) -> bool {
        self.find(source).is_some_and(|entry| entry.suspended)
    }

    fn find(&self, source: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.config.id == source)
    }

    fn find_mut(&mut self, source: &str) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.config.id == source)
    }

    fn slots_in_use(&self) -> usize {
        self.draining.len()
            + self
                .entries
                .iter()
                .filter(|entry| entry.running.is_some())
                .count()
    }

    pub fn step(&mut self, now: Duration, event: Event) -> Vec<Action> {
        let mut actions = Vec::new();
        match event {
            Event::Tick => {}
            Event::RefreshRequested { source } | Event::RetryRequested { source } => {
                if let Some(entry) = self.find_mut(&source) {
                    entry.suspended = false;
                    if entry.running.is_some() {
                        entry.queued_refresh = true;
                    } else {
                        entry.due = Some(now);
                    }
                }
            }
            Event::Reconfigured { sources } => self.reconfigure(now, sources, &mut actions),
            Event::Finished {
                source,
                revision,
                outcome,
            } => self.finished(now, &source, revision, outcome, &mut actions),
        }
        self.start_due(now, &mut actions);
        actions
    }

    fn reconfigure(
        &mut self,
        now: Duration,
        sources: Vec<ChartSourceConfig>,
        actions: &mut Vec<Action>,
    ) {
        let mut old = std::mem::take(&mut self.entries);
        for source in sources {
            let position = old.iter().position(|entry| entry.config.id == source.id);
            let previous = position.map(|index| old.remove(index));
            match previous {
                Some(entry) if entry.config == source => self.entries.push(entry),
                Some(entry) => {
                    self.retire(entry, actions);
                    let fresh = self.fresh_entry(source, now);
                    self.entries.push(fresh);
                }
                None => {
                    let fresh = self.fresh_entry(source, now);
                    self.entries.push(fresh);
                }
            }
        }
        for entry in old {
            self.retire(entry, actions);
        }
    }

    fn retire(&mut self, entry: Entry, actions: &mut Vec<Action>) {
        if let Some(revision) = entry.running {
            actions.push(Action::Cancel {
                source: entry.config.id.clone(),
                revision,
            });
            self.draining.push((entry.config.id, revision));
        }
    }

    fn finished(
        &mut self,
        now: Duration,
        source: &str,
        revision: u64,
        outcome: Result<(), ChartError>,
        actions: &mut Vec<Action>,
    ) {
        if let Some(index) = self
            .draining
            .iter()
            .position(|(id, rev)| id == source && *rev == revision)
        {
            self.draining.remove(index);
            return;
        }
        let Some(entry) = self.find_mut(source) else {
            return;
        };
        if entry.running != Some(revision) {
            return;
        }
        entry.running = None;
        let refresh = entry.config.refresh();
        match outcome {
            Ok(()) => {
                entry.failures = 0;
                entry.suspended = false;
                entry.due = Some(now + refresh);
            }
            Err(error) => {
                entry.failures = entry.failures.saturating_add(1);
                if error.kind.suspends_retry() {
                    entry.suspended = true;
                    entry.due = None;
                } else {
                    let step = (entry.failures as usize - 1).min(BACKOFF_SECONDS.len() - 1);
                    let backoff = Duration::from_secs(BACKOFF_SECONDS[step]);
                    entry.due = Some(now + backoff.max(refresh));
                }
            }
        }
        if std::mem::take(&mut entry.queued_refresh) {
            entry.suspended = false;
            entry.due = Some(now);
        }
        actions.push(Action::Publish {
            source: source.to_string(),
        });
    }

    fn start_due(&mut self, now: Duration, actions: &mut Vec<Action>) {
        let mut candidates: Vec<(Duration, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.running.is_none()
                    && !entry.suspended
                    && !self.draining.iter().any(|(id, _)| *id == entry.config.id)
            })
            .filter_map(|(index, entry)| {
                entry.due.filter(|due| *due <= now).map(|due| (due, index))
            })
            .collect();
        candidates.sort();
        for (_, index) in candidates {
            if self.slots_in_use() >= self.max_concurrent {
                break;
            }
            let entry = &mut self.entries[index];
            entry.running = Some(entry.revision);
            entry.due = None;
            actions.push(Action::Start {
                source: entry.config.id.clone(),
                revision: entry.revision,
            });
        }
    }

    /// Earliest monotonic time something can start; `None` when nothing is
    /// scheduled or every slot is busy (a `Finished` event wakes the caller).
    pub fn next_wake(&self) -> Option<Duration> {
        if self.slots_in_use() >= self.max_concurrent {
            return None;
        }
        self.entries
            .iter()
            .filter(|entry| {
                entry.running.is_none()
                    && !entry.suspended
                    && !self.draining.iter().any(|(id, _)| *id == entry.config.id)
            })
            .filter_map(|entry| entry.due)
            .min()
    }
}
