use crossbeam_channel::Sender;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub use resonance_types::{
    ChatMessage, NetworkInterface, SnifferStatePayload, SystemMessage, TranslationResult,
    TranslatorStatePayload,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Condvar, Mutex};

pub struct AppState {
    pub batch_data: Arc<(Mutex<(Vec<MessageRequest>, u64)>, Condvar)>,
    pub chat_history: Mutex<IndexMap<u64, ChatMessage>>,
    pub system_history: Mutex<VecDeque<SystemMessage>>,
    pub next_pid: AtomicU64,
    pub nickname_cache: Mutex<HashMap<String, String>>,
    pub translator_tx: Mutex<Option<Sender<crate::services::translator::TranslationJob>>>,
    pub data_factory_tx: Mutex<Option<Sender<crate::io::DataFactoryJob>>>,
    pub sniffer_tx: Mutex<Option<Sender<()>>>,
    pub dedup_cache: Mutex<HashMap<(u64, u64, u64), u64>>,
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

#[derive(Serialize)]
pub struct MessageRequest {
    pub cmd: String, // Always "translate"
    pub pid: u64,
    pub text: String, // The Japanese message
}

#[derive(Deserialize)]
pub struct ExportMessage {
    pub channel: String,
    pub nickname: String,
    pub message: String,
    pub translated: Option<String>,
    pub timestamp: u64,
}
