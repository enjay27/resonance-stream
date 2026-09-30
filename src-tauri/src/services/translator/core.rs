use reqwest::blocking::Client;
use resonance_core::text::completion_request;

use std::sync::atomic::{AtomicU16, Ordering};

/// Port tried first for llama-server; another is picked if it is taken.
pub const PREFERRED_SERVER_PORT: u16 = 8080;
static SERVER_PORT: AtomicU16 = AtomicU16::new(PREFERRED_SERVER_PORT);

/// Base URL of the running llama-server (the port is chosen at launch).
pub fn server_url() -> String {
    format!("http://127.0.0.1:{}", SERVER_PORT.load(Ordering::Relaxed))
}

pub(crate) fn set_server_port(port: u16) {
    SERVER_PORT.store(port, Ordering::Relaxed);
}

/// `Err` carries a reason for the log; it is never shown as a translation.
pub fn translate_text(client: &Client, server_url: &str, jp_text: &str) -> Result<String, String> {
    // Use /completion endpoint (llama.cpp native, not OpenAI-compatible)
    let endpoint = format!("{}/completion", server_url);

    let response = client
        .post(&endpoint)
        .json(&completion_request(jp_text))
        .send()
        .map_err(|e| format!("AI server connection error: {e}"))?;

    let json_body = response
        .json::<serde_json::Value>()
        .map_err(|e| format!("AI server reply unreadable: {e}"))?;
    json_body["content"]
        .as_str()
        .map(|content| content.trim().to_string())
        .ok_or_else(|| "AI server reply has no content".to_string())
}
