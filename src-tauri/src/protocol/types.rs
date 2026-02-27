use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::AtomicU64;
use crossbeam_channel::Sender;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub struct AppState {
    pub batch_data: Arc<(Mutex<(Vec<MessageRequest>, u64)>, Condvar)>,
    pub chat_history: Mutex<IndexMap<u64, ChatMessage>>,
    pub system_history: Mutex<VecDeque<SystemMessage>>,
    pub next_pid: AtomicU64,
    pub nickname_cache: Mutex<HashMap<String, String>>,
    pub translator_tx: Mutex<Option<Sender<crate::services::translator::TranslationJob>>>,
    pub data_factory_tx: Mutex<Option<Sender<crate::io::DataFactoryJob>>>,
    pub sniffer_tx: Mutex<Option<Sender<()>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub pid: u64,
    pub channel: String,
    pub nickname: String,
    pub message: String,
    pub timestamp: u64,
    pub uid: u64,
    pub class_id: u64,
    pub level: u64,
    pub sequence_id: u64,
    // --- Translation Support ---
    #[serde(default)]
    pub translated: Option<String>,
    #[serde(default)]
    pub nickname_romaji: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemMessage {
    pub pid: u64,             // Unique ID for Leptos 'For' loop keys
    pub timestamp: u64,       // Milliseconds for sorting
    pub level: String,        // "info", "warn", "error", "success"
    pub source: String,       // "Backend", "Sniffer", "Translator"
    pub message: String,      // The actual log text
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
    pub cmd: String,          // Always "translate"
    pub pid: u64,
    pub text: String,         // The Japanese message
}

// 2. Lobby Recruitment: Detailed recruitment board data
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LobbyRecruitment {
    pub party_id: u64,
    pub leader_nickname: String,
    pub description: String,      // The full Japanese description
    pub recruit_id: String,       // "ID:XXXXX" extracted from description
    pub member_count: u32,
    pub max_members: u32,
    pub timestamp: u64,
    // --- Translation Support ---
    #[serde(default)]
    pub translated: Option<String>,      // Translated party description
    #[serde(default)]
    pub nickname_romaji: Option<String>, // Leader's name in Romaji
}

// 3. Profile Assets: Player thumbnails and full renders
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAsset {
    pub uid: u64,
    pub snapshot_url: String,   // Thumbnail URL
    pub halflength_url: String, // Full render URL
    pub status_text: String,    // Original Japanese status
    pub timestamp: u64,
    // --- Translation Support ---
    #[serde(default)]
    pub translated: Option<String>, // Translated status/title
}

#[derive(Debug)]
pub struct SplitPayload {
    pub channel: String,
    pub chat_blocks: Vec<(u32, Vec<u8>)>,
    pub recruit_asset_blocks: Vec<Vec<u8>>,
}

#[derive(Serialize)]
pub struct NicknameRequest {
    pub cmd: String,          // Always "nickname_only"
    pub pid: u64,
    pub nickname: String,     // Required for this request type
}

// --- RESPONSE: Python -> Rust ---
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NicknameResponse {
    pub pid: u64,
    pub nickname: String,
    pub romaji: String, // Flat string, no object
}

// --- For Full translation requests ---
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageResponse {
    pub pid: u64,
    pub translated: String,
}

#[derive(Serialize)]
pub struct BatchMessageRequest {
    pub cmd: String, // "batch_translate" or "translate_and_save"
    pub messages: Vec<MessageRequest>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TranslationResult {
    pub pid: u64,
    pub translated: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchMessageResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub results: Vec<TranslationResult>,
}

#[derive(Deserialize)]
pub struct ExportMessage {
    pub channel: String,
    pub nickname: String,
    pub message: String,
    pub translated: Option<String>,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SnifferStatePayload {
    pub state: String,   // "Starting", "Firewall", "Binding", "Active", "Error", "Off"
    pub message: String, // Context or Error message
}