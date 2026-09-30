use reqwest::blocking::Client;
use serde_json::json;

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
    let safe_text = sanitize_input(jp_text);

    // Must match make_prompt() format used during fine-tuning training
    let prompt = format!(
        "<bos><start_of_turn>user\n\
        You are a professional Japanese (ja) to Korean (ko) translator. \
        Your goal is to accurately convey the meaning and nuances of the original Japanese text \
        while adhering to Korean grammar, vocabulary, and cultural sensitivities.\n\
        The input may contain placeholders such as [P0], [P1], [P2], etc. \
        These represent protected terms. Copy them verbatim into the translation at the correct position.\n\
        Example: '今日は[P0]と[P1]で行く' → '오늘은 [P0]와 [P1]에서 가'\n\
        Produce only the Korean translation, without any additional explanations or commentary. \
        Please translate the following Japanese text into Korean:\n\
        {}<end_of_turn>\n\
        <start_of_turn>model\n",
        safe_text
    );

    let payload = json!({
        "prompt": prompt,
        "stream": false,
        "temperature": 0.1,
        "max_tokens": 512,
        "stop": ["<end_of_turn>", "<eos>"]
    });

    // Use /completion endpoint (llama.cpp native, not OpenAI-compatible)
    let endpoint = format!("{}/completion", server_url);

    let response = client
        .post(&endpoint)
        .json(&payload)
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

fn sanitize_input(text: &str) -> String {
    text.replace("<start_of_turn>", "")
        .replace("<end_of_turn>", "")
        .replace("<bos>", "")
        .replace("<eos>", "")
        .replace("</start_of_turn>", "")
        .replace("</end_of_turn>", "")
}

pub fn contains_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        let u = c as u32;
        // Hiragana: 0x3040 - 0x309F
        // Katakana: 0x30A0 - 0x30FF
        // CJK Unified Ideographs (Kanji): 0x4E00 - 0x9FAF
        (0x3040..=0x309F).contains(&u)
            || (0x30A0..=0x30FF).contains(&u)
            || (0x4E00..=0x9FAF).contains(&u)
    })
}
