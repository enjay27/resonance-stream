use crossbeam_channel::{unbounded, Receiver, Sender};
use reqwest::blocking::Client;
use serde_json::json;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

use crate::io::save_to_data_factory;
use crate::protocol::types::{ChatMessage, SystemLogLevel};
use crate::services::processor::{load_dictionary, postprocess_text, preprocess_text};
use crate::{inject_system_message, store_and_emit};

pub struct TranslationJob {
    pub chat: ChatMessage,
}

// Ensure the background server dies when this thread/app closes
struct ServerGuard(Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

#[derive(serde::Serialize, Clone)]
struct TranslationUpdate {
    pid: u64,
    translated: String,
}

pub fn translate_text(client: &Client, jp_text: &str) -> String {
    let system_prompt = "Blue Protocol Star Resonance 일본어 채팅 로그를 자연스러운 한국어 구어체로 번역하세요.\n\
        직역을 피하고, 원본에 없는 주어/목적어를 임의로 추가하지 마십시오.\n\
        클래스 및 파티 모집 약어(T, H, D, 狂, 響, NM, EH, M16 등)는 일본 서버 컨텍스트에 맞게 그대로 유지하십시오.\n\
        특히 게임 고유 용어 및 은어(예: ファスト -> 속공, 器用 -> 숙련, 完凸 -> 풀돌, 消化 -> 숙제)는\n\
        한국 유저들이 실제 사용하는 로컬라이징 용어로 엄격하게 번역하십시오.";

    let payload = json!({
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": jp_text}
        ],
        "temperature": 0.1,
        "max_tokens": 256
    });

    match client.post("http://127.0.0.1:8080/v1/chat/completions")
        .json(&payload)
        .send()
    {
        Ok(response) => {
            println!("Response {:?}", response);
            if let Ok(json) = response.json::<serde_json::Value>() {
                if let Some(content) = json["choices"][0]["message"]["content"].as_str() {
                    return content.trim().to_string();
                }
            }
            "[Translation Parse Error]".to_string()
        }
        Err(_) => "[AI Server Not Responding]".to_string(),
    }
}

pub fn start_translator_worker(app: AppHandle, model_path: PathBuf) -> Sender<TranslationJob> {
    let (tx, rx): (Sender<TranslationJob>, Receiver<TranslationJob>) = unbounded();
    let config = crate::config::load_config(app.clone());

    thread::spawn(move || {
        inject_system_message(&app, SystemLogLevel::Info, "Translator", "Initializing HTTP AI Backend...");

        let server_path = app.path()
            .app_data_dir()
            .unwrap()
            .join("bin")
            .join("llama-server.exe");

        // Launch the Vulkan Server Process
        let mut server_cmd = Command::new(server_path);
        server_cmd.arg("-m").arg(&model_path);
        server_cmd.arg("--port").arg("8080");
        server_cmd.arg("--log-disable"); // Prevents terminal spam

        if config.compute_mode.to_lowercase() == "vulkan" || config.compute_mode.to_lowercase() == "cuda" {
            server_cmd.arg("-ngl").arg("99");
            inject_system_message(&app, SystemLogLevel::Info, "Translator", format!("Compute Mode: {} (GPU Offloading Enabled)", config.compute_mode.to_uppercase()));
        } else {
            server_cmd.arg("-ngl").arg("0");
            inject_system_message(&app, SystemLogLevel::Info, "Translator", "Compute Mode: CPU (System RAM)");
        }

        let n_ctx_size = match config.tier.to_lowercase().as_str() {
            "low" => "512",
            "middle" => "1024",
            "high" => "2048",
            "extreme" => "4096",
            _ => "1024",
        };
        server_cmd.arg("-c").arg(n_ctx_size);

        inject_system_message(&app, SystemLogLevel::Info, "Translator", format!("Performance Tier: {} (Context: {})", config.tier.to_uppercase(), n_ctx_size));

        // 0x08000000 = CREATE_NO_WINDOW (Hides the server terminal from the user)
        server_cmd.creation_flags(0x08000000);

        let server_process = match server_cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                inject_system_message(&app, SystemLogLevel::Error, "Translator", format!("Failed to start llama-server.exe. Is it in the root folder? ({})", e));
                return;
            }
        };

        let _server_guard = ServerGuard(server_process);

        inject_system_message(&app, SystemLogLevel::Info, "Translator", "Loading model into memory... (This takes a few seconds)");
        thread::sleep(Duration::from_secs(5));

        let client = Client::new();
        let dict_path = app.path().app_data_dir().unwrap().join("custom_dict.json");
        let custom_dict = load_dictionary(&dict_path);

        inject_system_message(&app, SystemLogLevel::Success, "Translator", "AI Server running! Ready for translation.");

        // The exact same Watchdog Batching loop as before!
        while let Ok(first_job) = rx.recv() {
            let mut batch = vec![first_job.chat];
            let start_time = Instant::now();
            let timeout = Duration::from_millis(1000);

            while batch.len() < 5 {
                let elapsed = start_time.elapsed();
                if elapsed >= timeout { break; }
                match rx.recv_timeout(timeout - elapsed) {
                    Ok(job) => batch.push(job.chat),
                    Err(_) => break,
                }
            }

            inject_system_message(&app, SystemLogLevel::Debug, "Translator", format!("Translating batch of {} messages sequentially...", batch.len()));

            for mut chat in batch {
                let shield = preprocess_text(&chat.message, &custom_dict, chat.nickname_romaji.as_deref(), Some(&chat.nickname));
                let raw_translation = translate_text(&client, &shield.masked_text);
                let final_str = postprocess_text(&raw_translation, &shield);

                chat.translated = Some(final_str.clone());
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
    use crate::services::downloader::{MODEL_FILENAME, MODEL_FOLDER};
    use crate::{AI_SERVER_FILENAME, AI_SERVER_FOLDER};
    use reqwest::blocking::Client;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::Instant;

    #[test]
    fn evaluate_translation() {
        // 1. Resolve model path (using the exact same logic from your old test)
        let appdata = std::env::var("APPDATA").expect("Could not find APPDATA environment variable");
        let mut model_path = PathBuf::from(appdata.clone());
        // Replace this with your actual Tauri bundle identifier if it changed
        model_path.push("com.enjay.bpsr.resonance-stream");
        model_path.push("models");
        model_path.push(MODEL_FOLDER);
        model_path.push(MODEL_FILENAME);

        println!("Looking for model at: {:?}", model_path);

        let mut ai_server_path = PathBuf::from(appdata.clone());
        // Replace this with your actual Tauri bundle identifier if it changed
        ai_server_path.push("com.enjay.bpsr.resonance-stream");
        ai_server_path.push("bin");
        ai_server_path.push(AI_SERVER_FOLDER);
        ai_server_path.push(AI_SERVER_FILENAME);

        println!("Looking for ai server at: {:?}", ai_server_path);

        // 2. Start the AI Server in the background for the test
        println!("Starting llama-server.exe...");
        let mut server_process = Command::new(&ai_server_path)
            .arg("-m").arg(&model_path)
            .arg("--port").arg("8080")
            .arg("-ngl").arg("99") // Force GPU for the test
            .arg("--log-disable")
            .spawn()
            .expect("Failed to start llama-server.exe. Is it in the src-tauri folder?");

        // Give the server time to load the 1.8GB model into VRAM
        println!("Waiting 5 seconds for model to load into VRAM...");
        std::thread::sleep(std::time::Duration::from_secs(5));

        let client = Client::new();

        // 3. The Japanese text you want to test
        let test_jp = "NM出ました！TとH募集します。よろしくお願いします！";

        println!("-----------------------------------");
        println!("[Input JA]: {}", test_jp);

        // 4. Run and time the translation
        let start_time = Instant::now();
        let result_ko = translate_text(&client, test_jp);
        let elapsed = start_time.elapsed();

        println!("[Output KO]: {}", result_ko);
        println!("[Time]: {:.2?}", elapsed);
        println!("-----------------------------------");

        // 5. Cleanup: Kill the server so it doesn't stay running in the background
        let _ = server_process.kill();
    }
}