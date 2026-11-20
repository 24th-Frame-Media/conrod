//! Video-memory politeness between the scan's GPU models and the decision model.
//!
//! On a card of 8 GB or less the 8-bit decision model (about 6.5 GB) cannot sit beside
//! the detector and the face models. A scan's GPU stages hold a shared [`stage`] guard for
//! as long as its sessions are loaded; a model call that would crowd the card takes the
//! [`exclusive`] guard and so waits for them, and they wait for it. Nothing else is
//! gated: on bigger cards, or at 4 bits, the two coexist and no call ever waits.
//!
//! No cycle is possible: the scan's guard is released when its sessions drop, and a scan
//! never waits on a moment judgement (the judge waits for the scan, never the reverse).

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

/// Cards up to this much video memory are "small" (an "8 GB" card reports a little under
/// 8 GiB; a quarter GiB of slack).
pub const SMALL_CARD: u64 = 8 * (1 << 30) + (1 << 28);

pub struct Gate(RwLock<()>);

/// Held by a scan while it has GPU sessions open. Any number may be held at once.
pub struct Stage<'a>(#[allow(dead_code)] RwLockReadGuard<'a, ()>);

/// Held for one decision-model call on a small card.
pub struct Exclusive<'a>(#[allow(dead_code)] RwLockWriteGuard<'a, ()>);

impl Gate {
    pub const fn new() -> Gate {
        Gate(RwLock::new(()))
    }

    pub fn stage(&self) -> Stage<'_> {
        Stage(self.0.read().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn exclusive(&self) -> Exclusive<'_> {
        Exclusive(self.0.write().unwrap_or_else(|e| e.into_inner()))
    }

    /// Whether a model call would have to wait right now.
    pub fn busy(&self) -> bool {
        matches!(self.0.try_write(), Err(TryLockError::WouldBlock))
    }
}

impl Default for Gate {
    fn default() -> Self {
        Gate::new()
    }
}

static GATE: Gate = Gate::new();

/// The process-wide gate: a scan holds this while its GPU sessions are loaded.
pub fn stage() -> Stage<'static> {
    GATE.stage()
}

/// The process-wide gate, for one model call on a small card.
pub fn exclusive() -> Exclusive<'static> {
    GATE.exclusive()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn a_model_call_waits_for_running_stages_and_releases_when_done() {
        static LOCAL: Gate = Gate::new();
        assert!(!LOCAL.busy());
        let first = LOCAL.stage();
        let second = LOCAL.stage();
        assert!(LOCAL.busy(), "stages share the card, a model call does not");
        let entered = Arc::new(AtomicBool::new(false));
        let flag = entered.clone();
        let call = std::thread::spawn(move || {
            let _gate = LOCAL.exclusive();
            flag.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !entered.load(Ordering::SeqCst),
            "the call waits while a scan holds its sessions"
        );
        drop(first);
        std::thread::sleep(Duration::from_millis(50));
        assert!(!entered.load(Ordering::SeqCst), "one stage is still open");
        drop(second);
        call.join().unwrap();
        assert!(entered.load(Ordering::SeqCst));
        assert!(!LOCAL.busy(), "released when the call returns");
        let _again = LOCAL.stage();
    }
}
