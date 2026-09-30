//! End to end: the mock game server's packets through capture, the
//! translator step and resonance-llama to a mock llama-server, and back.

mod support;

use resonance_core::text::Dictionary;
use resonance_core::workers::{ServerSupervisor, SupervisorAction};
use resonance_llama::health_ok;
use std::time::Instant;
use support::game_server::*;
use support::harness::Harness;
use support::llama_server::{MockLlama, Reply};

fn packets(frames: &[Vec<u8>]) -> Vec<Vec<u8>> {
    Connection::to_default_client().packets(frames)
}

fn translations(rows: &[resonance_types::ChatMessage]) -> Vec<(&str, Option<&str>)> {
    rows.iter()
        .map(|r| (r.message.as_str(), r.translated.as_deref()))
        .collect()
}

#[test]
fn a_burst_comes_out_translated_in_order_and_only_japanese_is_sent() {
    let llama = MockLlama::start();
    llama.content("안녕하세요").content("같이 가요");
    let burst = [
        chat(1, 100, "たろう", "こんにちは").frame(),
        chat(2, 101, "Bob", "hello all").frame(),
        chat(3, 102, "はなこ", "一緒に行こう").frame(),
        me_frame(Channel::Party, "ok"),
    ]
    .concat();
    // Coalesced, then cut in the middle of the third line.
    let cut = burst.len() - 30;
    let rows = Harness::new(&llama).run(&packets(&cut_at(&burst, &[cut])));

    assert_eq!(
        translations(&rows),
        [
            ("こんにちは", Some("안녕하세요")),
            ("hello all", None),
            ("一緒に行こう", Some("같이 가요")),
            ("ok", None),
        ]
    );
    assert_eq!(llama.prompted_lines(), ["こんにちは", "一緒に行こう"]);
}

#[test]
fn names_and_dictionary_terms_go_out_masked_and_come_back() {
    let llama = MockLlama::start();
    llama.content("[P0]님, [P1] 가자");
    let mut harness = Harness::new(&llama);
    harness.translator.dictionary =
        Dictionary::from_json_str(r#"{"boss": {"銀なぽ": "은나포"}}"#).unwrap();

    // たろう is known by name once one of their lines has been seen.
    let rows = harness.run(&packets(&[
        chat(1, 100, "たろう", "gg").frame(),
        chat(2, 101, "はなこ", "たろうさん、銀なぽ行こう").frame(),
    ]));

    assert_eq!(llama.prompted_lines(), ["[P0]さん、[P1]行こう"]);
    assert_eq!(rows[1].translated.as_deref(), Some("Tarou님, 은나포 가자"));
}

#[test]
fn model_noise_is_cleaned_from_the_translation() {
    let llama = MockLlama::start();
    // Leaked turn tags, a re-cased and re-spaced placeholder.
    llama.content("<think>hmm</think> [ p0 ]님 안녕 <end_of_turn>");
    let rows = Harness::new(&llama).run(&packets(&[
        chat(1, 100, "たろう", "gg").frame(),
        chat(2, 101, "はなこ", "たろうさんこんにちは").frame(),
    ]));
    assert_eq!(rows[1].translated.as_deref(), Some("Tarou님 안녕"));
}

#[test]
fn a_line_repeated_in_world_chat_is_asked_for_once() {
    let llama = MockLlama::start();
    llama.content("레이드 모집 중!");
    let recruit = "レイド募集中！";
    let rows = Harness::new(&llama).run(&packets(&[
        chat(1, 100, "Bob", recruit).frame(),
        chat(2, 101, "Ann", recruit).frame(),
        chat(3, 102, "Cid", recruit).frame(),
    ]));
    assert!(rows
        .iter()
        .all(|r| r.translated.as_deref() == Some("레이드 모집 중!")));
    assert_eq!(llama.completions().len(), 1);
}

#[test]
fn a_failed_line_is_left_untranslated_and_asked_for_again_next_time() {
    let llama = MockLlama::start();
    llama.reply([Reply::Status(503)]).content("다시 해 봐");
    let mut harness = Harness::new(&llama);
    let first = harness.run(&packets(&[chat(1, 100, "Bob", "もう一回").frame()]));
    assert_eq!(first[0].translated, None);

    // The same words in a new line: the failure was not cached.
    let second = harness.run(&packets(&[chat(2, 100, "Bob", "もう一回").frame()]));
    assert_eq!(second[0].translated.as_deref(), Some("다시 해 봐"));
    assert_eq!(llama.completions().len(), 2);
}

#[test]
fn a_duplicate_from_the_game_is_neither_shown_nor_translated_twice() {
    let llama = MockLlama::start();
    llama.content("한 번만");
    let line = chat(5, 100, "Bob", "一回だけ");
    let rows = Harness::new(&llama).run(&packets(&[line.frame(), line.frame()]));
    assert_eq!(rows.len(), 1);
    assert_eq!(llama.completions().len(), 1);
}

#[test]
fn translation_waits_for_the_model_to_load() {
    let llama = MockLlama::start();
    llama.health([503, 503, 503]).content("준비 완료");
    let mut harness = Harness::new(&llama);
    let polls = (1..=10)
        .find(|_| health_ok(&harness.translator.client, &llama.url))
        .expect("server became healthy");
    assert_eq!(polls, 4);
    let rows = harness.run(&packets(&[chat(1, 100, "Bob", "準備OK").frame()]));
    assert_eq!(rows[0].translated.as_deref(), Some("준비 완료"));
}

#[test]
fn a_server_that_keeps_failing_is_restarted_then_given_up_on() {
    // The translator worker's decisions, fed by real replies: three
    // failures in a row restart the server; a success in between resets
    // the count; after too many restarts it gives up.
    let llama = MockLlama::start();
    let mut harness = Harness::new(&llama);
    let mut supervisor = ServerSupervisor::default();
    let now = Instant::now();
    let mut seq = 0;
    let mut outcome = |harness: &mut Harness, supervisor: &mut ServerSupervisor| {
        seq += 1;
        let rows = harness.run(&packets(&[
            chat(seq, 100, "Bob", &format!("行{seq}")).frame()
        ]));
        match rows[0].translated {
            Some(_) => {
                supervisor.on_job_ok();
                SupervisorAction::Continue
            }
            None => supervisor.on_job_failed(now),
        }
    };

    llama.reply([
        Reply::Status(500),
        Reply::Status(500),
        Reply::Content("ok".into()),
    ]);
    llama.reply([Reply::Status(500), Reply::Status(500)]);
    let steps: Vec<_> = (0..5)
        .map(|_| outcome(&mut harness, &mut supervisor))
        .collect();
    assert!(
        steps.iter().all(|a| *a == SupervisorAction::Continue),
        "{steps:?}"
    );

    // The mock answers 500 from here on.
    let mut restarts = 0;
    let give_up = loop {
        match outcome(&mut harness, &mut supervisor) {
            SupervisorAction::Continue => {}
            SupervisorAction::Restart(_) => restarts += 1,
            SupervisorAction::GiveUp => break true,
        }
        if restarts > 10 {
            break false;
        }
    };
    assert!(give_up);
    assert_eq!(restarts, resonance_core::workers::MAX_RESTARTS);
}
