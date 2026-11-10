//! The Job queue: priorities and scheduled Jobs.
//!
//! Queued Jobs start in priority order: Jobs from the Agora ahead of Jobs created
//! from issues, first come first served within the same origin. A [`Scheduler`]
//! turns a [`Timetable`] into a new queued Job at each of its times, so every run
//! of a scheduled Job (a daily report, say) is a Job of its own. Time comes from a
//! [`Clock`], so tests drive it with a [`ManualClock`].

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::locks::mutex;

/// Where a Job came from; it decides how soon the Job starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobOrigin {
    /// Submitted by a person in the Agora; starts first.
    Agora,
    /// Created from an issue.
    Issue,
}

/// Identifies one queued Job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

/// Identifies a scheduled Job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduleId(pub u64);

/// A Job waiting in the queue, carrying the State it will start with.
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedJob<S> {
    pub id: JobId,
    pub origin: JobOrigin,
    /// The initial State of the Job.
    pub state: S,
    /// The scheduled Job this run belongs to, if any.
    pub schedule: Option<ScheduleId>,
}

struct QueueInner<S> {
    next_id: u64,
    waiting: BTreeMap<(JobOrigin, u64), QueuedJob<S>>,
}

/// A priority queue of Jobs. Clones share the same queue.
pub struct JobQueue<S> {
    inner: Arc<Mutex<QueueInner<S>>>,
}

impl<S> Clone for JobQueue<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> Default for JobQueue<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> JobQueue<S> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(QueueInner {
                next_id: 0,
                waiting: BTreeMap::new(),
            })),
        }
    }

    /// Queues a Job and returns its id.
    pub fn submit(&self, origin: JobOrigin, state: S) -> JobId {
        self.push(origin, state, None)
    }

    fn push(&self, origin: JobOrigin, state: S, schedule: Option<ScheduleId>) -> JobId {
        let mut inner = mutex(&self.inner);
        let seq = inner.next_id;
        inner.next_id += 1;
        let id = JobId(seq);
        inner.waiting.insert(
            (origin, seq),
            QueuedJob {
                id,
                origin,
                state,
                schedule,
            },
        );
        id
    }

    /// Takes the Job that should start next, if any.
    pub fn next(&self) -> Option<QueuedJob<S>> {
        mutex(&self.inner).waiting.pop_first().map(|(_, job)| job)
    }

    /// How many Jobs are waiting.
    pub fn len(&self) -> usize {
        mutex(&self.inner).waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A source of the current time, as the time since the Unix epoch.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}

/// The real time.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
    }
}

/// A clock that only moves when told to. Clones share the same time.
#[derive(Debug, Clone, Default)]
pub struct ManualClock {
    now: Arc<Mutex<Duration>>,
}

impl ManualClock {
    /// A clock starting at time zero.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn advance(&self, by: Duration) {
        *mutex(&self.now) += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *mutex(&self.now)
    }
}

/// When a scheduled Job runs: repeatedly, one fixed interval apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timetable {
    interval: Duration,
}

impl Timetable {
    /// Runs once per `interval`, the first run one interval after scheduling.
    ///
    /// # Panics
    /// Panics if `interval` is zero.
    pub fn every(interval: Duration) -> Self {
        assert!(!interval.is_zero(), "a timetable interval must not be zero");
        Self { interval }
    }
}

/// Why a schedule could not be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error("unknown schedule {0:?}")]
    Unknown(ScheduleId),
}

struct Entry<S> {
    timetable: Timetable,
    origin: JobOrigin,
    template: S,
    next_due: Duration,
    paused: bool,
}

/// Queues a new Job for each time of every scheduled Job that has come due.
///
/// Call [`Scheduler::tick`] regularly; it is the only thing that reads the clock.
pub struct Scheduler<S> {
    clock: Arc<dyn Clock>,
    queue: JobQueue<S>,
    inner: Mutex<SchedulerInner<S>>,
}

struct SchedulerInner<S> {
    next_id: u64,
    entries: HashMap<ScheduleId, Entry<S>>,
}

impl<S: Clone> Scheduler<S> {
    pub fn new(clock: impl Clock + 'static, queue: JobQueue<S>) -> Self {
        Self {
            clock: Arc::new(clock),
            queue,
            inner: Mutex::new(SchedulerInner {
                next_id: 0,
                entries: HashMap::new(),
            }),
        }
    }

    /// Schedules a Job; each run starts with a copy of `state`.
    pub fn schedule(&self, timetable: Timetable, origin: JobOrigin, state: S) -> ScheduleId {
        let now = self.clock.now();
        let mut inner = mutex(&self.inner);
        let id = ScheduleId(inner.next_id);
        inner.next_id += 1;
        inner.entries.insert(
            id,
            Entry {
                timetable,
                origin,
                template: state,
                next_due: now + timetable.interval,
                paused: false,
            },
        );
        id
    }

    /// Stops queueing runs until [`Scheduler::resume`].
    pub fn pause(&self, id: ScheduleId) -> Result<(), ScheduleError> {
        let mut inner = mutex(&self.inner);
        let entry = inner
            .entries
            .get_mut(&id)
            .ok_or(ScheduleError::Unknown(id))?;
        entry.paused = true;
        Ok(())
    }

    /// Starts queueing runs again from the next time after now; runs missed while
    /// paused are not replayed.
    pub fn resume(&self, id: ScheduleId) -> Result<(), ScheduleError> {
        let now = self.clock.now();
        let mut inner = mutex(&self.inner);
        let entry = inner
            .entries
            .get_mut(&id)
            .ok_or(ScheduleError::Unknown(id))?;
        if entry.paused {
            entry.paused = false;
            while entry.next_due <= now {
                entry.next_due += entry.timetable.interval;
            }
        }
        Ok(())
    }

    /// Queues one Job per scheduled time that has passed; returns their ids.
    pub fn tick(&self) -> Vec<JobId> {
        let now = self.clock.now();
        let mut inner = mutex(&self.inner);
        let mut ids: Vec<_> = inner.entries.keys().copied().collect();
        ids.sort();
        let mut queued = Vec::new();
        for id in ids {
            let entry = inner.entries.get_mut(&id).expect("id from keys");
            if entry.paused {
                continue;
            }
            while entry.next_due <= now {
                queued.push(
                    self.queue
                        .push(entry.origin, entry.template.clone(), Some(id)),
                );
                entry.next_due += entry.timetable.interval;
            }
        }
        queued
    }
}
