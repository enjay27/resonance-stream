use std::io::{BufRead, BufReader};
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
use crate::{inject_system_message, store_and_emit, AI_SERVER_FILENAME, AI_SERVER_FOLDER};

pub const AI_SERVER_URL: &str = "http://127.0.0.1:8080";

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
    let system_prompt =
        "당신은 '블루 프로토콜: 스타 레조넌스' 일본 서버 전문 번역 엔진입니다.
        사용자가 입력하는 일본어 채팅 로그를 다음 규칙에 따라 한국어 구어체로 번역하십시오.

        1. **출력 형식**: 번역 결과만 출력하십시오. 설명, 인사, 따옴표 등 부가적인 텍스트는 절대 포함하지 마십시오.
        2. **로컬라이징 용어**: 한국 유저들의 실제 게임 용어를 엄격히 사용하십시오.
           - 火力 -> 딜러 / ファスト -> 속공 / 器用 -> 숙련 / リキャスト -> 쿨타임
           - 完凸 -> 풀돌 / 消化 -> 숙제 / 寄生 -> 버스
        3. **약어 유지**: 다음 약어는 일본 서버 컨텍스트 유지를 위해 번역하지 않고 그대로 둡니다.
           - 클래스 및 역할: T, H, D, DPS
           - 콘텐츠 및 모집: NM, EH, M16, EX, k
        4. **번역 스타일**:
           - 문어체가 아닌 자연스러운 한국어 구어체(채팅 스타일)를 사용하십시오.
           - 원문에 없는 주어/목적어를 임의로 추측하여 추가하지 마십시오.
           - 직역보다는 게임 내 상황에 맞는 의역을 우선하되, 원문의 의도를 해치지 마십시오.";

    let payload = json!({
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": jp_text}
        ],
        "stream": true, // Enable SSE
        "temperature": 0.1
    });

    // 1. Send the request and get a streaming response
    let response = match client.post("http://127.0.0.1:8080/v1/chat/completions")
        .json(&payload)
        .send()
    {
        Ok(res) => res,
        Err(_) => return "[AI Server Connection Error]".to_string(),
    };

    let mut full_translated_text = String::new();
    let reader = BufReader::new(response);

    // 2. Parse the stream line-by-line
    for line in reader.lines() {
        if let Ok(l) = line {
            if l.starts_with("data: ") {
                let json_str = &l[6..];
                if json_str.trim() == "[DONE]" { break; }

                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                    // Extract the "delta" (the new piece of text)
                    if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                        full_translated_text.push_str(content);
                    }
                }
            }
        }
    }

    full_translated_text.trim().to_string()
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
            .join(AI_SERVER_FOLDER)
            .join(AI_SERVER_FILENAME);

        // Launch the Vulkan Server Process
        let mut server_cmd = Command::new(server_path);
        server_cmd.arg("-m").arg(&model_path);
        server_cmd.arg("--port").arg("8080");
        server_cmd.arg("--log-disable");

        if config.compute_mode.to_lowercase() == "gpu" {
            // For a GTX 1060, 10 to 15 layers is usually the sweet spot to leave VRAM for the game.
            // Ideally, tie this to your config.tier!
            let gpu_layers = match config.tier.to_lowercase().as_str() {
                "low" => "10",      // Safe for 3GB 1060
                "middle" => "15",   // Safe for 6GB 1060
                "high" => "25",
                "extreme" => "99",
                _ => "15",
            };
            server_cmd.arg("-ngl").arg(gpu_layers);
            inject_system_message(&app, SystemLogLevel::Info, "Translator", format!("GPU Offloading: {} Layers", gpu_layers));
        } else {
            server_cmd.arg("-ngl").arg("0");
            inject_system_message(&app, SystemLogLevel::Info, "Translator", "Compute Mode: CPU");
        }

        // --- Extreme Low-End Memory Optimizations ---

        // 1. Minimum context for 140 char limits
        server_cmd.arg("-c").arg("512");

        // 2. Tiny batches
        server_cmd.arg("-b").arg("16");
        server_cmd.arg("-ub").arg("16");

        // 3. CPU Thread limiting (Crucial when GPU offload is partial)
        server_cmd.arg("-t").arg("4"); // Prevents the translation from freezing the game

        // 4. Safe memory handling (REMOVED --mlock)
        server_cmd.arg("--parallel").arg("1");
        server_cmd.arg("--cont-batching");
        server_cmd.arg("--no-mmap");

        inject_system_message(&app, SystemLogLevel::Info, "Translator", format!("Performance Tier: {}", config.tier.to_uppercase()));

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

        if server_health_check(&app) { return; }

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

fn server_health_check(app: &AppHandle) -> bool {
    let client = Client::new();
    let mut is_ready = false;
    let start_wait = Instant::now();

    inject_system_message(&app, SystemLogLevel::Info, "Translator", "Waiting for AI Engine to warm up...");

    // Poll the health endpoint for up to 30 seconds
    while start_wait.elapsed().as_secs() < 30 {
        if let Ok(res) = client.get("http://127.0.0.1:8080/health").send() {
            if res.status().is_success() {
                is_ready = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(1000));
    }

    if !is_ready {
        inject_system_message(&app, SystemLogLevel::Error, "Translator", "AI Engine failed to initialize within 30s.");
        return true;
    }
    false
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
    use std::time::{Instant, Duration};
    use std::process::Command;
    use reqwest::blocking::Client;
    use crate::{AI_SERVER_FILENAME, AI_SERVER_FOLDER};
    use crate::services::downloader::{MODEL_FILENAME, MODEL_FOLDER};

    #[test]
    fn evaluate_translation() {
        // 1. Resolve paths (pointing to your AppData/bin folder)
        let appdata = std::env::var("APPDATA").expect("Could not find APPDATA environment variable");
        let base_path = PathBuf::from(appdata).join("com.enjay.bpsr.resonance-stream");

        let model_path = base_path.join("models").join(MODEL_FOLDER).join(MODEL_FILENAME);
        let server_path = base_path.join("bin").join(AI_SERVER_FOLDER).join(AI_SERVER_FILENAME);

        println!("Looking for model at: {:?}", model_path);
        println!("Looking for server at: {:?}", server_path);

        // 2. Start the AI Server
        println!("Starting llama-server.exe...");
        let mut server_process = Command::new(server_path)
            .arg("-m").arg(&model_path)
            .arg("--port").arg("8080")
            .arg("-ngl").arg("99") // Force GPU for the test
            .arg("--log-disable")
            .spawn()
            .expect("Failed to start llama-server.exe. Is it downloaded to AppData/bin?");

        // 3. Smart Health Check (Polled every 500ms)
        let client = Client::new();
        let mut is_ready = false;
        let start_wait = Instant::now();

        println!("Waiting for AI Engine to warm up...");
        while start_wait.elapsed().as_secs() < 30 {
            if let Ok(res) = client.get("http://127.0.0.1:8080/health").send() {
                if res.status().is_success() {
                    is_ready = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(1000));
        }

        if !is_ready {
            let _ = server_process.kill();
            panic!("AI Engine failed to initialize within 30s (Model too large or GPU OOM?)");
        }

        // 4. Run the test translation using SSE logic
        let test_jp = "116　偵察右　銀なぽ";
        println!("-----------------------------------");
        println!("[Input JA]: {}", test_jp);

        let start_time = Instant::now();

        // This now calls the updated translate_text that uses SSE streaming
        let result_ko = translate_text(&client, test_jp);

        let elapsed = start_time.elapsed();

        println!("[Output KO]: {}", result_ko);
        println!("[Time]: {:.2?}", elapsed);
        println!("-----------------------------------");

        // 5. Cleanup
        let _ = server_process.kill();
    }
}