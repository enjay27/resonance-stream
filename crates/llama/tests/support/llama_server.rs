//! Mock llama-server: answers `/health` and `/completion` on 127.0.0.1 from
//! a script, one reply per request, and records every request it gets.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// What the mock does with one `/completion` request.
#[derive(Clone, Debug)]
pub enum Reply {
    /// 200 with `{"content": ...}`, as llama-server answers.
    Content(String),
    /// 200 with this body as it is (malformed or unexpected JSON).
    Body(String),
    /// This status with a JSON error body, e.g. 503 while the model loads.
    Status(u16),
    /// Reads the request, then never answers (a hung server).
    Hang,
    /// Answers `Content` after a delay.
    Slow(Duration, String),
    /// Sends the headers and part of the body, then closes the connection.
    CutOff,
    /// Closes the connection without a word.
    Close,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
}

impl Request {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

#[derive(Default)]
struct Script {
    completions: VecDeque<Reply>,
    /// Statuses `/health` answers first; 200 once they run out.
    health: VecDeque<u16>,
    requests: Vec<Request>,
}

pub struct MockLlama {
    pub url: String,
    script: Arc<Mutex<Script>>,
}

impl MockLlama {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock llama-server");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let script = Arc::new(Mutex::new(Script::default()));
        let shared = script.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let script = shared.clone();
                thread::spawn(move || serve(stream, &script));
            }
        });
        Self { url, script }
    }

    /// Queues replies for the next `/completion` requests, in order. With
    /// the queue empty the mock answers 500.
    pub fn reply(&self, replies: impl IntoIterator<Item = Reply>) -> &Self {
        self.script.lock().unwrap().completions.extend(replies);
        self
    }

    pub fn content(&self, text: &str) -> &Self {
        self.reply([Reply::Content(text.to_string())])
    }

    /// `/health` answers these statuses first (503 = still loading, 0 =
    /// never answers).
    pub fn health(&self, statuses: impl IntoIterator<Item = u16>) -> &Self {
        self.script.lock().unwrap().health.extend(statuses);
        self
    }

    pub fn requests(&self) -> Vec<Request> {
        self.script.lock().unwrap().requests.clone()
    }

    pub fn completions(&self) -> Vec<Request> {
        self.requests()
            .into_iter()
            .filter(|r| r.path == "/completion")
            .collect()
    }

    /// The masked lines the model was asked to translate: what follows the
    /// instructions in each prompt, up to the end of the user turn.
    pub fn prompted_lines(&self) -> Vec<String> {
        self.completions()
            .iter()
            .map(|r| {
                let prompt = r.json()["prompt"].as_str().unwrap_or_default().to_string();
                let line = prompt.rsplit_once("into Korean:\n").map_or("", |(_, l)| l);
                line.split("<end_of_turn>")
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }
}

fn serve(stream: TcpStream, script: &Mutex<Script>) {
    // The client may keep the connection alive for several requests.
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut stream = stream;
    while let Some(request) = read_request(&mut reader) {
        let is_completion = request.path == "/completion";
        let reply = {
            let mut script = script.lock().unwrap();
            script.requests.push(request.clone());
            if is_completion {
                script.completions.pop_front().unwrap_or(Reply::Status(500))
            } else if request.path == "/health" {
                match script.health.pop_front() {
                    Some(200) | None => Reply::Body(r#"{"status":"ok"}"#.into()),
                    Some(0) => Reply::Hang,
                    Some(status) => Reply::Status(status),
                }
            } else {
                Reply::Status(404)
            }
        };
        let keep_open = match reply {
            Reply::Content(text) => {
                let body = serde_json::json!({ "content": text, "stop": true }).to_string();
                respond(&mut stream, 200, &body)
            }
            Reply::Body(body) => respond(&mut stream, 200, &body),
            Reply::Status(status) => {
                let body = serde_json::json!({
                    "error": { "code": status, "message": "Loading model", "type": "unavailable_error" }
                })
                .to_string();
                respond(&mut stream, status, &body)
            }
            Reply::Slow(delay, text) => {
                thread::sleep(delay);
                let body = serde_json::json!({ "content": text }).to_string();
                respond(&mut stream, 200, &body)
            }
            Reply::Hang => {
                thread::sleep(Duration::from_secs(600));
                false
            }
            Reply::CutOff => {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 64\r\n\r\n{\"content\": \"\xEC\x95\x88",
                );
                false
            }
            Reply::Close => false,
        };
        if !keep_open {
            return;
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) -> bool {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).is_ok()
}

/// One HTTP/1.1 request, or `None` once the client closed the connection.
fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}
