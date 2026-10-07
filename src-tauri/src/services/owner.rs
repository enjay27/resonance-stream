//! The one owner of the background services' lifetime (review R-1, W-10): who is running, and the
//! only code that starts, stops or restarts them. Every path that used to take the slot by hand --
//! start-up, the translator command, a settings change, the model swap -- goes through here, so the
//! rules (an old worker is dropped before a new one starts, a stopped worker is retired) live in one
//! place instead of four slightly different copies.
//!
//! The translator is here; the sniffer follows (it needs the wait for the old capture thread).

use crate::services::translator::{self, TranslationJob};
use crossbeam_channel::Sender;
use parking_lot::Mutex;
use tauri::AppHandle;

#[derive(Default)]
pub struct Services {
    /// The running translator's job queue, or `None`. Dropping it ends the worker.
    translator: Mutex<Option<Sender<TranslationJob>>>,
}

impl Services {
    /// Starts the translator unless one is running (idempotent). Returns whether it started.
    pub fn start_translator(&self, app: &AppHandle) -> bool {
        let mut slot = self.translator.lock();
        if slot.is_some() {
            return false;
        }
        *slot = Some(translator::start_translator_worker(
            app.clone(),
            crate::get_model_path(app),
        ));
        true
    }

    /// Drops the running translator, if any, and starts a fresh one (new settings, a swapped
    /// model). The old worker's queue closes first, so it ends; the new one supersedes it.
    pub fn restart_translator(&self, app: &AppHandle) {
        let mut slot = self.translator.lock();
        *slot = None;
        *slot = Some(translator::start_translator_worker(
            app.clone(),
            crate::get_model_path(app),
        ));
    }

    /// Stops the translator: drops its queue and retires every worker still starting up, so none of
    /// them reports on, or relaunches, a server that is no longer theirs. Returns whether one was
    /// running.
    pub fn stop_translator(&self) -> bool {
        let was_running = self.translator.lock().take().is_some();
        translator::retire_translator_workers();
        was_running
    }

    /// A handle to the running translator's queue, for the sniffer's per-message dispatch.
    pub fn translator_sender(&self) -> Option<Sender<TranslationJob>> {
        self.translator.lock().clone()
    }
}
