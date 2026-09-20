use crate::{
    error::{AppError, Result},
    recovery::{InputFile, RecoveredFile},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
pub(crate) struct Session {
    pub inputs: Vec<InputFile>,
    pub destination: Option<PathBuf>,
    pub results: HashMap<String, RecoveredFile>,
    pub last_outputs: Vec<PathBuf>,
}

#[derive(Clone, Default)]
pub(crate) struct AppState {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    session: Mutex<Session>,
    occupied: AtomicBool,
    recovering: AtomicBool,
    cancel: AtomicBool,
}

impl AppState {
    pub fn session(&self) -> Result<MutexGuard<'_, Session>> {
        self.shared
            .session
            .lock()
            .map_err(|_| AppError::SessionUnavailable)
    }

    // Reserve the whole operation, including dialogs and blocking work. A check
    // followed by a later lock would allow selection/reset to race with recovery.
    pub fn begin(&self) -> Result<OperationGuard> {
        self.shared
            .occupied
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| AppError::Busy)?;
        Ok(OperationGuard {
            state: self.clone(),
        })
    }

    pub fn begin_recovery(&self) -> Result<OperationGuard> {
        let guard = self.begin()?;
        self.shared.cancel.store(false, Ordering::SeqCst);
        self.shared.recovering.store(true, Ordering::SeqCst);
        Ok(guard)
    }

    pub fn is_recovering(&self) -> bool {
        self.shared.recovering.load(Ordering::SeqCst)
    }

    pub fn cancel(&self) {
        if self.is_recovering() {
            self.shared.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn cancellation(&self) -> &AtomicBool {
        &self.shared.cancel
    }
}

pub(crate) struct OperationGuard {
    state: AppState,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.state.shared.recovering.store(false, Ordering::SeqCst);
        self.state.shared.occupied.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_mutations_share_one_reservation() {
        let state = AppState::default();
        let selection = state.begin().unwrap();
        assert!(matches!(state.begin_recovery(), Err(AppError::Busy)));
        assert!(matches!(state.begin(), Err(AppError::Busy)));
        drop(selection);
        let recovery = state.begin_recovery().unwrap();
        assert!(state.is_recovering());
        assert!(matches!(state.begin(), Err(AppError::Busy)));
        drop(recovery);
        assert!(!state.is_recovering());
        assert!(state.begin().is_ok());
    }

    #[test]
    fn cancellation_is_scoped_to_the_current_recovery() {
        let state = AppState::default();
        let first = state.begin_recovery().unwrap();
        state.cancel();
        assert!(state.cancellation().load(Ordering::SeqCst));
        drop(first);
        let _second = state.begin_recovery().unwrap();
        assert!(!state.cancellation().load(Ordering::SeqCst));
    }

    #[test]
    fn unwinding_releases_the_operation() {
        let state = AppState::default();
        let _ = std::panic::catch_unwind(|| {
            let _guard = state.begin_recovery().unwrap();
            panic!("simulated worker failure");
        });
        assert!(!state.is_recovering());
        assert!(state.begin().is_ok());
    }
}
