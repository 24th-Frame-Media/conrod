//! The hand-off between the OCR and Vision stages when they run side by side.
//!
//! Vision resolves a subject's number from the model's read, the entry list
//! and OCR's evidence (roundels, text tokens, the burst tally). If Vision
//! reached a subject before OCR had finished it, it resolved without that
//! evidence and OCR then patched the result in, so the stored number depended
//! on thread timing. The gate removes the race: a Vision worker waits until OCR
//! has finished (or given up on) *that* subject before it asks the model, so
//! the resolver always sees OCR's pass and the answer is the same every run.
//!
//! The gate never blocks for ever. It opens early when the OCR stage ends for
//! any reason (finished, failed, stopped, skipped), and a waiter also gives up
//! when the run is stopped or its own stage is skipped.

use crate::lock;
use std::collections::HashSet;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
struct State {
    /// Detections OCR has finished with this run (read, skipped or failed).
    done: HashSet<i64>,
    /// The OCR stage is over: nothing more will be marked.
    closed: bool,
}

#[derive(Default)]
pub(crate) struct OcrGate {
    state: Mutex<State>,
    changed: Condvar,
}

impl OcrGate {
    /// A gate with nothing to wait for: OCR does not run beside Vision.
    pub(crate) fn closed() -> OcrGate {
        let gate = OcrGate::default();
        gate.close();
        gate
    }

    /// OCR is finished with detection `id`.
    pub(crate) fn done(&self, id: i64) {
        lock(&self.state).done.insert(id);
        self.changed.notify_all();
    }

    /// The OCR stage ended: every waiter goes on with what is stored.
    pub(crate) fn close(&self) {
        lock(&self.state).closed = true;
        self.changed.notify_all();
    }

    /// Wait until OCR has finished with `id`, the stage has ended, or
    /// `give_up()` says the waiter should leave. True when OCR's pass for `id`
    /// is settled; false when the waiter left early.
    pub(crate) fn wait(&self, id: i64, give_up: impl Fn() -> bool) -> bool {
        let mut state = lock(&self.state);
        loop {
            if state.closed || state.done.contains(&id) {
                return true;
            }
            if give_up() {
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

/// Marks a detection done when dropped, so every exit from the OCR loop body
/// (a skip, an error, a stop) releases the Vision worker waiting on it.
pub(crate) struct DoneOnDrop<'a> {
    pub(crate) gate: &'a OcrGate,
    pub(crate) id: i64,
}

impl Drop for DoneOnDrop<'_> {
    fn drop(&mut self) {
        self.gate.done(self.id);
    }
}

/// Closes the gate when the OCR stage function returns by any path.
pub(crate) struct CloseOnDrop<'a>(pub(crate) &'a OcrGate);

impl Drop for CloseOnDrop<'_> {
    fn drop(&mut self) {
        self.0.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn vision_waits_for_ocr_to_finish_its_subject() {
        let gate = Arc::new(OcrGate::default());
        let ocr_finished = Arc::new(AtomicBool::new(false));
        let waiter = {
            let (gate, flag) = (gate.clone(), ocr_finished.clone());
            std::thread::spawn(move || {
                let settled = gate.wait(7, || false);
                // OCR's write must be visible by the time the wait returns.
                (settled, flag.load(Ordering::Acquire))
            })
        };
        std::thread::sleep(Duration::from_millis(80));
        // A different subject finishing does not release this one.
        gate.done(8);
        std::thread::sleep(Duration::from_millis(80));
        assert!(!waiter.is_finished(), "still waiting for subject 7");
        ocr_finished.store(true, Ordering::Release);
        gate.done(7);
        assert_eq!(waiter.join().unwrap(), (true, true));
    }

    #[test]
    fn a_finished_ocr_stage_opens_the_gate_for_everything() {
        let gate = OcrGate::default();
        gate.close();
        assert!(gate.wait(1, || false));
        assert!(gate.wait(2, || false));
        assert!(OcrGate::closed().wait(3, || false));
    }

    #[test]
    fn a_waiter_leaves_when_told_to_instead_of_hanging() {
        let gate = OcrGate::default();
        let begun = Instant::now();
        let stop = AtomicBool::new(false);
        std::thread::scope(|s| {
            let waiter = s.spawn(|| gate.wait(1, || stop.load(Ordering::Relaxed)));
            std::thread::sleep(Duration::from_millis(100));
            stop.store(true, Ordering::Relaxed);
            assert!(!waiter.join().unwrap(), "left without OCR's pass");
        });
        assert!(begun.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn dropping_the_guards_releases_the_waiters() {
        let gate = OcrGate::default();
        {
            let _done = DoneOnDrop { gate: &gate, id: 4 };
            assert!(!gate.wait(4, || true));
        }
        assert!(gate.wait(4, || false));
        {
            let _close = CloseOnDrop(&gate);
        }
        assert!(gate.wait(99, || false));
    }
}
