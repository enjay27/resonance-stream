use reqwest::blocking::Client;
use serde_json::json;

pub const AI_SERVER_URL: &str = "http://127.0.0.1:8080";

pub fn translate_text(client: &Client, server_url: &str, jp_text: &str) -> String {
    // Must match make_prompt() format used during fine-tuning training
    let prompt = format!(
        "<bos><start_of_turn>user\n\
        You are a professional Japanese (ja) to Korean (ko) translator. \
        Your goal is to accurately convey the meaning and nuances of the original Japanese text \
        while adhering to Korean grammar, vocabulary, and cultural sensitivities.\n\
        Produce only the Korean translation, without any additional explanations or commentary. \
        Please translate the following Japanese text into Korean:\n\
        {}<end_of_turn>\n\
        <start_of_turn>model\n",
        jp_text
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

    let response = match client.post(&endpoint).json(&payload).send() {
        Ok(res) => res,
        Err(_) => return "[AI Server Connection Error]".to_string(),
    };

    if let Ok(json_body) = response.json::<serde_json::Value>() {
        if let Some(content) = json_body["content"].as_str() {
            return content.trim().to_string();
        }
    }

    "[AI Server Parsing Error]".to_string()
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
