use crate::config::AppConfig;
use crossbeam_channel::Sender;
use parking_lot::{Mutex, RwLock};
use resonance_core::history::ChatHistory;
use resonance_core::text::Dictionary;
use serde::{Deserialize, Serialize};

pub use resonance_types::{
    ChatMessage, NetworkInterface, ServiceStates, SnifferState, SnifferStatePayload, SystemMessage,
    TranslationResult, TranslatorState, TranslatorStatePayload,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

/// Locks are parking_lot's: they do not poison, so one panicking thread does
/// not make every later command that touches the same state panic too.
pub struct AppState {
    /// The live config. The file on disk is read once at start-up and
    /// written by `save_config`; everything else reads this copy.
    pub config: RwLock<AppConfig>,
    /// Held while a config change is applied (see `save_config`).
    pub config_lock: Mutex<()>,
    pub chat_history: Mutex<ChatHistory>,
    pub system_history: Mutex<VecDeque<SystemMessage>>,
    pub next_pid: AtomicU64,
    pub nickname_cache: Mutex<HashMap<String, String>>,
    /// Swapped whole when the dictionary is synced or edited; the translator
    /// takes a cheap `Arc` clone per job.
    pub dictionary: RwLock<Arc<Dictionary>>,
    pub translator_tx: Mutex<Option<Sender<crate::services::translator::TranslationJob>>>,
    /// Japanese messages of this run still owed a translation; caught up at
    /// each translator start (see `translator::catch_up`).
    pub translation_ledger: Mutex<resonance_core::workers::TranslationLedger>,
    pub data_factory_tx: Mutex<Option<Sender<crate::io::DataFactoryJob>>>,
    pub sniffer_tx: Mutex<Option<crate::services::sniffer::SnifferHandle>>,
    /// The last state each service emitted (`get_service_states`): a UI that
    /// was not listening yet still learns it.
    pub service_states: Mutex<ServiceStates>,
    pub blocked_users: Mutex<HashMap<u64, String>>,
    /// What the global shortcuts are bound to (see `shortcut.rs`).
    pub shortcuts: Mutex<crate::shortcut::GlobalShortcuts>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SystemLogLevel {
    Info,    // Normal initialization logs
    Warning, // Sniffer not active, GPU memory low
    Error,   // Driver init failed, Sidecar crashed
    Success, // Dictionary updated, Model ready
    Debug,   // high-frequency, technical events
    Trace,   // extremely-frequency
}

#[derive(Deserialize)]
pub struct ExportMessage {
    pub channel: String,
    pub nickname: String,
    pub message: String,
    pub translated: Option<String>,
    pub timestamp: u64,
}
