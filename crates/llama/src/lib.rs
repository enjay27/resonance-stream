//! Talks to the local llama.cpp server (llama-server) over HTTP. Builds and
//! tests on any OS; launching and supervising the server stays in the app.

use reqwest::blocking::Client;
use resonance_core::text::completion_request;
use std::time::Duration;

/// llama-server is on 127.0.0.1: a connection that takes longer is not coming.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// One translation, or one health check. Well under
/// `resonance_core::workers::MAX_TRANSLATION_WAIT`, so a hung server fails
/// jobs (and is restarted) while the lines are still worth translating.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The client the translator uses: requests give up after
/// [`REQUEST_TIMEOUT`] instead of waiting on a hung server forever.
pub fn client() -> Client {
    client_with(REQUEST_TIMEOUT)
}

/// A client for llama-server whose requests give up after `request`.
pub fn client_with(request: Duration) -> Client {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT.min(request))
        .timeout(request)
        .build()
        .expect("HTTP client builds (Client::new() panics on the same failure)")
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
    let content = json_body["content"]
        .as_str()
        .ok_or_else(|| "AI server reply has no content".to_string())?
        .trim();
    if content.is_empty() {
        return Err("AI server reply is empty".to_string());
    }
    Ok(content.to_string())
}

/// `true` once `GET /health` answers with a success status (llama-server
/// answers 503 while it loads the model).
pub fn health_ok(client: &Client, server_url: &str) -> bool {
    client
        .get(format!("{}/health", server_url))
        .send()
        .is_ok_and(|res| res.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::blocking::Client;
    use std::time::{Duration, Instant};

    #[test]
    fn test_full_translator_flow_with_mock() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        // 1. Create a mock server on a random available port
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind mock server");
        let port = listener.local_addr().unwrap().port();
        let mock_url = format!("http://127.0.0.1:{}", port);

        // 2. Spawn a thread to act as the "llama-server"
        std::thread::spawn(move || {
            // Loop to handle multiple incoming requests (health check + translation)
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    let mut buffer = [0; 4096];
                    if let Ok(bytes_read) = stream.read(&mut buffer) {
                        let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                        // Route 1: Mock the Health Check endpoint
                        if request.starts_with("GET /health") {
                            let body = r#"{"status":"ok"}"#;
                            // FIXED: Added Content-Length and Connection: close
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(), body
                            );
                            let _ = stream.write_all(response.as_bytes());
                        }
                        // Route 2: Mock llama.cpp's native /completion endpoint,
                        // which translate_text() calls (not the OpenAI-style one)
                        else if request.starts_with("POST /completion") {
                            let body = r#"{"content": " 116 정찰 우측 은나포 "}"#;
                            // FIXED: Added Content-Length and Connection: close
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(), body
                            );
                            let _ = stream.write_all(response.as_bytes());
                        }
                    }
                }
            }
        });

        let client = Client::new();

        // 3. Test the Health Check polling logic against the mock
        let mut is_ready = false;
        let start_wait = Instant::now();

        // We use a much shorter timeout (2 seconds) since the mock is instant
        while start_wait.elapsed().as_secs() < 2 {
            if health_ok(&client, &mock_url) {
                is_ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        assert!(is_ready, "Mock server failed the health check loop!");

        // 4. Test the actual Translation pipeline against the mock
        let test_jp = "116　偵察右　銀なぽ";
        let result_ko = translate_text(&client, &mock_url, test_jp).unwrap();

        assert_eq!(result_ko, "116 정찰 우측 은나포");
    }
}
