//! Desktop-shell-only state - deliberately minimal. The Rust core
//! (`brute`) remains the sole authoritative store for anything
//! persistent (library index, runtime profiles, calibration records);
//! this struct holds only the in-memory cancellation flags a running
//! tuning session or local-generation session needs, for exactly as
//! long as one is active. See docs/desktop-architecture.md.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct AppState {
    /// `Some` only while `tune_run` is actively executing. Stage 4
    /// supports one tuning session at a time, matching the CLI's own
    /// single-run model (`brute tune status`/`cancel` also assume one
    /// active run) - not a limitation introduced here.
    pub tune_cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// `Some` only while `local_run_generate` is actively streaming.
    pub run_cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// `Some` only while `download_model` is actively downloading.
    pub download_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

/// A small cooperative-cancellation handle - `true` once cancellation
/// has been requested. Mirrors the core engine's own
/// `TickAction`/`is_cancelled` closure pattern (see
/// `runtime::process::TickAction`, `tuning::runner::execute_plan`)
/// rather than inventing a new cancellation vocabulary for the desktop
/// layer.
pub type CancelFlag = Arc<AtomicBool>;

pub fn new_cancel_flag() -> CancelFlag {
    Arc::new(AtomicBool::new(false))
}

pub fn is_cancelled(flag: &CancelFlag) -> bool {
    flag.load(Ordering::SeqCst)
}

pub fn request_cancel(flag: &CancelFlag) {
    flag.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_flag_starts_uncancelled() {
        let flag = new_cancel_flag();
        assert!(!is_cancelled(&flag));
    }

    #[test]
    fn request_cancel_is_observed_through_every_clone() {
        let flag = new_cancel_flag();
        let clone = flag.clone();
        assert!(!is_cancelled(&clone));

        request_cancel(&flag);

        assert!(is_cancelled(&flag));
        assert!(
            is_cancelled(&clone),
            "a cloned Arc must see the same cancellation state"
        );
    }

    #[test]
    fn app_state_starts_with_no_active_session() {
        let state = AppState::default();
        assert!(state.tune_cancel.lock().unwrap().is_none());
        assert!(state.run_cancel.lock().unwrap().is_none());
        assert!(state.download_cancel.lock().unwrap().is_none());
    }
}
