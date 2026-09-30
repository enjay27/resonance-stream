//! resonance-llama against a mock llama-server: what the server can answer,
//! and what the client makes of it.

mod support;

use reqwest::blocking::Client;
use resonance_core::text::output_token_limit;
use resonance_llama::{health_ok, translate_text};
use std::time::Duration;
use support::llama_server::{MockLlama, Reply};

#[test]
fn a_reply_is_trimmed() {
    let llama = MockLlama::start();
    llama.content("  116 정찰 우측 은나포 \n");
    assert_eq!(
        translate_text(&Client::new(), &llama.url, "116　偵察右　銀なぽ").as_deref(),
        Ok("116 정찰 우측 은나포")
    );
}

#[test]
fn the_request_is_a_completion_with_the_prompt_and_an_output_limit() {
    let llama = MockLlama::start();
    llama.content("안녕");
    translate_text(&Client::new(), &llama.url, "[P0]こんにちは").unwrap();

    let requests = llama.completions();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    let body = requests[0].json();
    let prompt = body["prompt"].as_str().unwrap();
    assert!(prompt.starts_with("<bos><start_of_turn>user\n"));
    assert!(prompt.ends_with("[P0]こんにちは<end_of_turn>\n<start_of_turn>model\n"));
    assert_eq!(body["n_predict"], output_token_limit("[P0]こんにちは"));
    assert_eq!(llama.prompted_lines(), ["[P0]こんにちは"]);
}

#[test]
fn unicode_in_and_out_survives_the_round_trip() {
    let llama = MockLlama::start();
    llama.content("좋아요 👍 「확인」");
    assert_eq!(
        translate_text(&Client::new(), &llama.url, "いいね👍「確認」").as_deref(),
        Ok("좋아요 👍 「확인」")
    );
    assert_eq!(llama.prompted_lines(), ["いいね👍「確認」"]);
}

#[test]
fn one_client_serves_many_requests_in_a_row() {
    let llama = MockLlama::start();
    llama.content("하나").content("둘").content("셋");
    let client = Client::new();
    let got: Vec<_> = ["一", "二", "三"]
        .iter()
        .map(|t| translate_text(&client, &llama.url, t).unwrap())
        .collect();
    assert_eq!(got, ["하나", "둘", "셋"]);
}

#[test]
fn an_error_status_is_an_error() {
    // llama-server's error body carries no `content`.
    for status in [500, 503] {
        let llama = MockLlama::start();
        llama.reply([Reply::Status(status)]);
        let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
        assert!(err.contains("no content"), "{status}: {err}");
    }
}

#[test]
fn a_reply_that_is_not_json_is_an_error() {
    let llama = MockLlama::start();
    llama.reply([Reply::Body("<html>Bad Gateway</html>".into())]);
    let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
    assert!(err.contains("unreadable"), "{err}");
}

#[test]
fn a_reply_without_string_content_is_an_error() {
    for body in [
        r#"{"stop":true}"#,
        r#"{"content":null}"#,
        r#"{"content":42}"#,
        "[]",
    ] {
        let llama = MockLlama::start();
        llama.reply([Reply::Body(body.into())]);
        let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
        assert!(err.contains("no content"), "{body}: {err}");
    }
}

#[test]
fn a_reply_cut_off_mid_body_is_an_error() {
    let llama = MockLlama::start();
    llama.reply([Reply::CutOff]);
    assert!(translate_text(&Client::new(), &llama.url, "テスト").is_err());
}

#[test]
fn a_connection_closed_without_a_reply_is_an_error() {
    let llama = MockLlama::start();
    llama.reply([Reply::Close]);
    let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
    assert!(err.contains("connection error"), "{err}");
}

#[test]
fn a_server_that_is_not_running_is_an_error() {
    // Bind and drop: nothing listens on that port any more.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let err = translate_text(
        &Client::new(),
        &format!("http://127.0.0.1:{port}"),
        "テスト",
    )
    .unwrap_err();
    assert!(err.contains("connection error"), "{err}");
}

#[test]
fn a_slow_reply_still_arrives() {
    let llama = MockLlama::start();
    llama.reply([Reply::Slow(Duration::from_millis(300), "늦었다".into())]);
    assert_eq!(
        translate_text(&Client::new(), &llama.url, "遅い").as_deref(),
        Ok("늦었다")
    );
}

#[test]
fn health_is_not_ok_while_the_model_loads() {
    let llama = MockLlama::start();
    llama.health([503, 503]);
    let client = Client::new();
    assert!(!health_ok(&client, &llama.url));
    assert!(!health_ok(&client, &llama.url));
    assert!(health_ok(&client, &llama.url));
}

#[test]
fn health_is_not_ok_without_a_server() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert!(!health_ok(
        &Client::new(),
        &format!("http://127.0.0.1:{port}")
    ));
}
