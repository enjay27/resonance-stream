use serde_json::Value;

// --- THE LOGIC UNDER TEST ---
// This mimics the logic inside your sniffer.rs
// We test THIS function to ensure your algorithm handles packets correctly.
fn extract_game_text(payload: &[u8]) -> Option<String> {
    if payload.is_empty() { return None; }

    if let Ok(text) = std::str::from_utf8(payload) {
        let clean_text = text.trim();
        // The logic from your sniffer.rs
        if clean_text.len() > 1 && clean_text.chars().all(|c| !c.is_control()) {
            return Some(clean_text.to_string());
        }
    }
    None
}

// --- THE TESTS ---

#[test]
fn test_clean_japanese_text() {
    // 1. Simulate a clean packet containing "Hello" in Japanese
    let input = "こんにちは".as_bytes();

    // 2. Run Logic
    let result = extract_game_text(input);

    // 3. Assert
    assert_eq!(result, Some("こんにちは".to_string()));
}

#[test]
fn test_ignore_binary_junk() {
    // 1. Simulate binary garbage (Blue Protocol encrypted data often looks like this)
    let input = &[0xDE, 0xAD, 0xBE, 0xEF];

    // 2. Run Logic
    let result = extract_game_text(input);

    // 3. Assert (Should be None because it's not valid UTF-8)
    assert!(result.is_none());
}

#[test]
fn test_ignore_control_chars() {
    // 1. Simulate a packet that is technically text but just a control code (e.g., KeepAlive)
    let input = "\n".as_bytes();

    let result = extract_game_text(input);

    assert!(result.is_none());
}

#[test]
fn test_json_formatting() {
    // Verify that we can wrap the text in JSON correctly (Integration check)
    let clean_text = "TestMsg";
    let json_output = serde_json::json!({
        "text": clean_text
    }).to_string();

    assert_eq!(json_output, r#"{"text":"TestMsg"}"#);
}