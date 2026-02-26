use crossbeam_channel::{unbounded, Receiver, Sender};
use llama_cpp_2::context::LlamaContext;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::llama_batch::LlamaBatch;

use crate::protocol::types::{ChatMessage, SystemLogLevel};
use crate::{inject_system_message, store_and_emit};
use crate::io::save_to_data_factory;
use crate::services::processor::{load_dictionary, postprocess_text, preprocess_text};

pub struct TranslationJob {
    pub chat: ChatMessage,
}

#[derive(serde::Serialize, Clone)]
struct TranslationUpdate {
    pid: u64,
    translated: String,
}

pub fn translate_text(model: &LlamaModel, ctx: &mut LlamaContext, jp_text: &str) -> String {
    let prompt = format!(
        "<|im_start|>system\n\
        다음 Blue Protocol (스타레조) 채팅 로그를 일본어에서 중립적인 한국어로 번역하세요. \
        명사를 임의로 추가하지 말고, 게임 용어(T, H, D, 狂, 響, NM, EH, M16)는 유지하십시오.<|im_end|>\n\
        <|im_start|>user\n\
        {}<|im_end|>\n\
        <|im_start|>assistant\n",
        jp_text
    );

    let tokens = model.str_to_token(&prompt, AddBos::Always).unwrap();

    let mut batch = LlamaBatch::new(512, 1);
    let last_index = tokens.len() - 1;
    for (i, &token) in tokens.iter().enumerate() {
        batch.add(token, i as i32, &[0], i == last_index).unwrap();
    }

    ctx.decode(&mut batch).expect("Failed to decode prompt");

    let mut translated_text = String::new();
    let mut n_cur = batch.n_tokens();

    while n_cur <= 256 {
        let candidates = ctx.candidates_ith(batch.n_tokens() - 1);

        let new_token_id = candidates
            .max_by(|a, b| a.logit().partial_cmp(&b.logit()).unwrap())
            .expect("Failed to find token")
            .id();

        let token_str = model.token_to_str(new_token_id, Special::Tokenize).unwrap_or_default();
        translated_text.push_str(&token_str);

        if translated_text.contains("<|im_end|>") {
            translated_text = translated_text.replace("<|im_end|>", "");
            break;
        }

        if new_token_id == model.token_eos() {
            break;
        }

        batch.clear();
        batch.add(new_token_id, n_cur, &[0], true).unwrap();
        ctx.decode(&mut batch).expect("Failed to decode token");

        n_cur += 1;
    }

    // Clean up the context so it's fresh for the next message!
    ctx.clear_kv_cache();

    translated_text.trim().to_string()
}

pub fn start_translator_worker(app: AppHandle, model_path: PathBuf) -> Sender<TranslationJob> {
    let (tx, rx): (Sender<TranslationJob>, Receiver<TranslationJob>) = unbounded();

    thread::spawn(move || {
        inject_system_message(&app, SystemLogLevel::Info, "Translator", "Initializing GGUF Backend...");

        let backend = LlamaBackend::init().expect("Failed to initialize Llama backend");
        let model_params = LlamaModelParams::default();

        let model = match LlamaModel::load_from_file(&backend, &model_path, &model_params) {
            Ok(m) => m,
            Err(e) => {
                inject_system_message(&app, SystemLogLevel::Error, "Translator", format!("GGUF Load Failed: {}", e));
                return;
            }
        };

        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(1024));
        let mut ctx = model.new_context(&backend, ctx_params).expect("Failed to create context");

        // Load Dictionary once into memory
        let dict_path = app.path().app_data_dir().unwrap().join("custom_dict.json");
        let custom_dict = load_dictionary(&dict_path); // You'll need a quick helper function to read your JSON file into a HashMap<String, String>

        inject_system_message(&app, SystemLogLevel::Success, "Translator", "Native Model loaded! Ready for translation.");

        // 0. The Background Loop
        while let Ok(first_job) = rx.recv() { // Blocks until the FIRST message arrives

            let mut batch = vec![first_job.chat];
            let start_time = Instant::now();
            let timeout = Duration::from_millis(1000); // 1000ms Watchdog

            // 1. Watchdog Collection Phase (Keep this! It's great for network efficiency)
            while batch.len() < 5 {
                let elapsed = start_time.elapsed();
                if elapsed >= timeout { break; }

                match rx.recv_timeout(timeout - elapsed) {
                    Ok(job) => batch.push(job.chat),
                    Err(_) => break,
                }
            }

            // 2. High-Quality Iterative Translation Phase
            inject_system_message(&app, SystemLogLevel::Debug, "Translator", format!("Translating batch of {} messages sequentially...", batch.len()));

            for mut chat in batch {
                // 1. PREPROCESS (Mask the terms)
                // Pass the romaji nickname if you have it available in your chat struct
                let shield = preprocess_text(&chat.message, &custom_dict, chat.nickname_romaji.as_deref(), Some(&chat.nickname));

                // 2. TRANSLATE (Give the LLM the masked text)
                let raw_translation = translate_text(&model, &mut ctx, &shield.masked_text);

                // 3. POSTPROCESS (Restore terms & fix Josa)
                let final_str = postprocess_text(&raw_translation, &shield);

                chat.translated = Some(final_str.clone());

                // Save to Data Factory & Emit
                let _ = save_to_data_factory(&app, chat.pid, &chat.message, &final_str);
                store_and_emit(&app, chat);
            }
        }
    });

    tx
}

pub fn contains_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        let u = c as u32;
        // Hiragana: 0x3040 - 0x309F
        // Katakana: 0x30A0 - 0x30FF
        // CJK Unified Ideographs (Kanji): 0x4E00 - 0x9FAF
        (0x3040..=0x309F).contains(&u) ||
            (0x30A0..=0x30FF).contains(&u) ||
            (0x4E00..=0x9FAF).contains(&u)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Instant;
    use crate::{MODEL_FILENAME, MODEL_FOLDER};

    #[test]
    fn evaluate_translation() {
        // 1. Put the hardcoded path to your GGUF model here for testing
        let appdata = std::env::var("APPDATA").expect("Could not find APPDATA environment variable");
        let mut model_path = PathBuf::from(appdata);
        model_path.push("com.enjay.bpsr.resonance-stream");
        model_path.push("models");
        model_path.push(MODEL_FOLDER);
        model_path.push(MODEL_FILENAME);

        println!("Looking for model at: {:?}", model_path);

        println!("Loading model for evaluation...");
        let backend = LlamaBackend::init().unwrap();
        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params).unwrap();

        let ctx_params = LlamaContextParams::default().with_n_ctx(std::num::NonZeroU32::new(1024));
        let mut ctx = model.new_context(&backend, ctx_params).unwrap();

        // 2. The Japanese text you want to test
        let test_jp = "NM出ました！TとH募集します。よろしくお願いします！";

        println!("-----------------------------------");
        println!("[Input JA]: {}", test_jp);

        // 3. Run and time the translation
        let start_time = Instant::now();
        let result_ko = translate_text(&model, &mut ctx, test_jp);
        let elapsed = start_time.elapsed();

        println!("[Output KO]: {}", result_ko);
        println!("[Time]: {:.2?}", elapsed);
        println!("-----------------------------------");
    }
}