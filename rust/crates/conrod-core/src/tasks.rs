//! Everything the app is doing, for the status area in the top right.
//!
//! Work is only visible if it says so, so every background operation takes a
//! [`Task`] from the [`TaskHub`] before it starts. The UI reads a snapshot
//! each frame; nothing here blocks on the UI.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Paused,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub id: u64,
    pub label: String,
    pub detail: String,
    pub state: State,
    pub done: u64,
    pub total: u64,
    pub elapsed: Duration,
    /// Linear from the rate so far; `None` until there is a rate.
    pub eta: Option<Duration>,
    pub error: Option<String>,
}

#[derive(Debug)]
struct Entry {
    id: u64,
    label: String,
    detail: String,
    state: State,
    done: u64,
    total: u64,
    started: Instant,
    ended: Option<Instant>,
    error: Option<String>,
}

#[derive(Default)]
struct Inner {
    tasks: Vec<Entry>,
    log: VecDeque<String>,
}

/// Shared by the engine and the UI.
#[derive(Clone, Default)]
pub struct TaskHub {
    inner: Arc<Mutex<Inner>>,
    next: Arc<AtomicU64>,
}

/// Finished tasks kept for the popover, and log lines kept.
const KEEP_FINISHED: usize = 20;
const KEEP_LOG: usize = 200;

impl TaskHub {
    pub fn new() -> TaskHub {
        TaskHub::default()
    }

    /// Announce a piece of work. `total` 0 means "don't know how much".
    pub fn start(&self, label: impl Into<String>, total: u64) -> Task {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let label = label.into();
        let mut inner = self.inner.lock().unwrap();
        inner.log_line(format!("started: {label}"));
        inner.tasks.push(Entry {
            id,
            label,
            detail: String::new(),
            state: State::Running,
            done: 0,
            total,
            started: Instant::now(),
            ended: None,
            error: None,
        });
        Task {
            hub: self.clone(),
            id,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Everything running, then the most recent finished, newest first.
    pub fn snapshot(&self) -> Vec<Snapshot> {
        let inner = self.inner.lock().unwrap();
        let mut out: Vec<Snapshot> = inner
            .tasks
            .iter()
            .map(|e| {
                let elapsed = e.ended.unwrap_or_else(Instant::now) - e.started;
                let eta = (e.state == State::Running && e.done > 0 && e.total > e.done)
                    .then(|| elapsed.mul_f64((e.total - e.done) as f64 / e.done as f64));
                Snapshot {
                    id: e.id,
                    label: e.label.clone(),
                    detail: e.detail.clone(),
                    state: e.state,
                    done: e.done,
                    total: e.total,
                    elapsed,
                    eta,
                    error: e.error.clone(),
                }
            })
            .collect();
        out.sort_by_key(|s| {
            (
                s.state != State::Running && s.state != State::Paused,
                std::cmp::Reverse(s.id),
            )
        });
        out
    }

    pub fn log(&self) -> Vec<String> {
        self.inner.lock().unwrap().log.iter().cloned().collect()
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut Entry, &mut VecDeque<String>)) {
        let mut inner = self.inner.lock().unwrap();
        let Inner { tasks, log } = &mut *inner;
        if let Some(e) = tasks.iter_mut().find(|e| e.id == id) {
            f(e, log);
        }
        // Drop the oldest finished tasks beyond the keep limit.
        let finished = tasks.iter().filter(|e| e.ended.is_some()).count();
        if finished > KEEP_FINISHED {
            if let Some(i) = tasks.iter().position(|e| e.ended.is_some()) {
                tasks.remove(i);
            }
        }
    }
}

impl Inner {
    fn log_line(&mut self, line: String) {
        push_log(&mut self.log, line);
    }
}

fn push_log(log: &mut VecDeque<String>, line: String) {
    if log.len() == KEEP_LOG {
        log.pop_front();
    }
    log.push_back(line);
}

/// One piece of work. Dropping it unfinished marks it failed, so a panic or
/// an early return can never leave the status area saying "working" forever.
pub struct Task {
    hub: TaskHub,
    id: u64,
    cancelled: Arc<AtomicBool>,
}

impl Task {
    pub fn progress(&self, done: u64, total: u64) {
        self.hub.update(self.id, |e, _| {
            e.done = done;
            e.total = total;
        });
    }

    pub fn detail(&self, text: impl Into<String>) {
        let text = text.into();
        self.hub.update(self.id, |e, _| e.detail = text);
    }

    pub fn paused(&self, paused: bool) {
        self.hub.update(self.id, |e, _| {
            e.state = if paused {
                State::Paused
            } else {
                State::Running
            };
        });
    }

    /// Asked to stop by the user; the worker checks and winds down.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn finish(self) {
        self.end(State::Done, None);
    }

    pub fn fail(self, error: impl Into<String>) {
        self.end(State::Failed, Some(error.into()));
    }

    fn end(&self, state: State, error: Option<String>) {
        self.hub.update(self.id, |e, log| {
            if e.ended.is_some() {
                return;
            }
            e.state = state;
            e.ended = Some(Instant::now());
            push_log(
                log,
                match &error {
                    Some(err) => format!("failed: {}: {err}", e.label),
                    None => format!("done: {}", e.label),
                },
            );
            e.error = error;
        });
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        self.end(State::Failed, Some("stopped without finishing".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_is_visible_until_it_ends_and_reports_how_it_ended() {
        let hub = TaskHub::new();
        let task = hub.start("Scanning", 10);
        task.progress(5, 10);
        let s = &hub.snapshot()[0];
        assert_eq!((s.state, s.done, s.total), (State::Running, 5, 10));
        task.finish();
        assert_eq!(hub.snapshot()[0].state, State::Done);
        assert!(hub.log().iter().any(|l| l == "done: Scanning"));
    }

    #[test]
    fn a_task_dropped_unfinished_says_so() {
        let hub = TaskHub::new();
        drop(hub.start("Writing XMP", 3));
        let s = &hub.snapshot()[0];
        assert_eq!(s.state, State::Failed);
        assert!(s.error.is_some());
    }

    #[test]
    fn running_work_is_listed_before_finished_work() {
        let hub = TaskHub::new();
        hub.start("old", 1).finish();
        let _running = hub.start("new", 1);
        assert_eq!(hub.snapshot()[0].label, "new");
    }
}
