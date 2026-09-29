use crate::config::AppConfig;
use crossbeam_channel::Sender;
use parking_lot::{Mutex, RwLock};
use resonance_core::history::ChatHistory;
use resonance_core::text::Dictionary;
use serde::{Deserialize, Serialize};

pub use resonance_types::{
    ChatMessage, NetworkInterface, SnifferStatePayload, SystemMessage, TranslationResult,
    TranslatorStatePayload,
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
    pub chat_history: Mutex<ChatHistory>,
    pub system_history: Mutex<VecDeque<SystemMessage>>,
    pub next_pid: AtomicU64,
    pub nickname_cache: Mutex<HashMap<String, String>>,
    /// Swapped whole when the dictionary is synced or edited; the translator
    /// takes a cheap `Arc` clone per job.
    pub dictionary: RwLock<Arc<Dictionary>>,
    pub translator_tx: Mutex<Option<Sender<crate::services::translator::TranslationJob>>>,
    pub data_factory_tx: Mutex<Option<Sender<crate::io::DataFactoryJob>>>,
    pub sniffer_tx: Mutex<Option<Sender<()>>>,
    pub blocked_users: Mutex<HashMap<u64, String>>,
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
