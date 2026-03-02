use std::io::{BufRead, BufReader};
use crossbeam_channel::{unbounded, Receiver, Sender};
use reqwest::blocking::Client;
use serde_json::json;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::io::save_to_data_factory;
use crate::protocol::types::{ChatMessage, SystemLogLevel};
use crate::services::processor::{load_dictionary, postprocess_text, preprocess_text};
use crate::{inject_system_message, kill_orphaned_servers, store_and_emit, AI_SERVER_FILENAME, AI_SERVER_FOLDER};

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
        "당신은 '블루 프로토콜: 스타 레조넌스' 일본 서버 전문 번역 엔진입니다. 일본어 채팅을 한국어 게임 용어(한국 서버 공식 명칭)로 번역하는 것이 유일한 임무입니다.

        # 출력 규칙 (매우 중요)
        - 번역 결과만 출력하십시오. 설명, 인사, 따옴표 등 일체의 부가 텍스트는 출력하지 마십시오.
        - 원문이 길거나 특수문자가 포함되어 있어도 절대 중간에 끊지 말고 끝까지 번역하십시오.
        - 일본어 잔류 및 혼용 절대 금지: 히라가나, 가타카나, 한자는 결과에 단 하나도 남기지 마십시오. 한자를 그대로 복사하여 한국어 문법과 섞어 쓰는 행위를 엄격히 금지합니다. (예: 暇인 분 -> 한가한 분)
        - 괄호 병기 절대 금지: '번역어(원문)' 형태로 괄호 안에 일본어나 발음을 남기는 행위를 엄격히 금지합니다.

        # 영단어(알파벳) 처리 (절대 규칙)
        알파벳(A-Z, a-z)으로 표기된 영단어는 절대 한글로 음역하지 마십시오. 무조건 알파벳 원문 그대로 출력하십시오.
        - 잘못된 예: CLANNAD -> 클랜나드
        - 올바른 예: CLANNAD -> CLANNAD / discord check -> discord check

        # 가타카나 번역 우선순위
        가타카나 용어 번역 시 아래 순서를 엄격히 따르십시오.
        1순위: 아래 '로컬라이징 용어' 목록에 있는 경우 → 목록의 번역어를 사용하십시오.
        2순위: 목록에 없는 게임 고유명사(몬스터명, 스킬명, 아이템명 등) → 한국어 음역으로 변환하십시오.
        3순위: 목록에 없는 일반 가타카나 표현 → 한국어 음역 또는 자연스러운 의역을 사용하십시오.
        ※ 어떤 경우에도 가타카나를 결과에 그대로 출력하거나 의미를 임의로 창작하지 마십시오.
        - 잘못된 예: レインボーパン -> 리조노 펑크 (환각/창작 오류)
        - 올바른 예: レインボーパン -> 레인보우 빵

        # 고유명사 환각 및 임의 변환 금지
        1. 몬스터, 상태 이상, 아이템 이름을 서양 판타지 용어로 마음대로 바꾸거나 지어내지 마십시오.
           - 잘못된 예: 鬼를 '고블린'으로 임의 변환
        2. 일상적인 표현을 엉뚱한 상황으로 창작하거나, 한자를 번역하지 않고 그대로 방치하지 마십시오.
           - 잘못된 예 1 (창작): お暇な方 -> '공부가 끝난 분'
           - 잘못된 예 2 (방치): お暇な方 -> '暇인 분'
           - 올바른 예: お暇な方 -> '한가한 분'

        # 로컬라이징 용어 (반드시 아래 용어로 번역)
        - 火力 → 딜러
        - ファスト → 속공
        - 器用 → 숙련
        - リキャスト → 쿨타임
        - 完凸 → 풀돌
        - 消化 → 숙제
        - 寄生 → 버스
        - 盾 → 탱커
        - 杖 → 법사
        - 弓 → 궁수
        - シャドハン (또는 シャドウハンター) → 그림자 사냥꾼

        # 약어 유지 (절대 번역 금지)
        - 클래스/역할: T, H, D, DPS
        - 콘텐츠/모집: NM, EH, M16, EX, k, @ (@은 모집 인원 표기로 사용)

        # 번역 스타일
        - 문어체가 아닌 자연스러운 한국어 구어체(채팅 스타일)를 사용하십시오.
        - 직역보다 게임 상황에 맞는 의역을 우선하되, 원문의 의미를 자의적으로 왜곡하지 마십시오.
        - 원문에 없는 주어/목적어를 임의로 추측하여 추가하지 마십시오.

        # 번역 불가 처리
        - 의미가 불분명하거나 맥락을 알 수 없는 경우에도 최선을 다해 번역하십시오.
        - 빈 출력, 원문 그대로 복사, 또는 '번역할 수 없습니다' 등의 메시지 출력을 엄격히 금지합니다.";

    let payload = json!({
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": jp_text}
        ],
        "stream": true, // Enable SSE
        "temperature": 0.1,
        "max_tokens": 512
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
        emit_translator_state(&app, "Starting", "Initializing AI Backend..."); // STATE 1

        kill_orphaned_servers(&app);

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

        // Dynamically scale Layers, Parallel slots, and Context based on hardware tier
        let gpu_layers = if config.compute_mode.to_lowercase() == "gpu" {
            match config.tier.to_lowercase().as_str() {
                "low" => "12",       // Safe for 4GB/6GB GPUs playing heavy games
                "middle" => "24",    // Sweet spot for 8GB GPUs
                "high" => "32",      // Good for 12GB GPUs
                "very high" => "99", // Full offload for 16GB+ GPUs
                _ => "24",
            }
        } else {
            "0" // CPU Mode
        };

        server_cmd.arg("-ngl").arg(gpu_layers);

        // 2. Strict Memory Constraints (Optimized for single-chat processing)
        server_cmd.arg("-c").arg("1536"); // Fixed 1024 context window (plenty for chat)

        // Remove --parallel entirely, letting it default to 1 (sequential)
        // Remove --cont-batching, as it requires extra memory overhead for multi-user generation

        // 3. Batching & Cache Settings

        server_cmd.arg("-b").arg("64");
        server_cmd.arg("-ub").arg("64");
        server_cmd.arg("-t").arg("4");
        server_cmd.arg("--parallel").arg("1");

        inject_system_message(&app, SystemLogLevel::Info, "Translator", format!("Performance Tier: {}", config.tier.to_uppercase()));

        // Log the final command construction for deep debugging
        inject_system_message(&app, SystemLogLevel::Trace, "Translator", format!("Server Launch Command: {:?}", server_cmd));

        server_cmd.creation_flags(0x08000000);

        let server_process = match server_cmd.spawn() {
            Ok(child) => {
                inject_system_message(&app, SystemLogLevel::Trace, "Translator", format!("Server process successfully spawned with OS PID: {}", child.id()));
                child
            },
            Err(e) => {
                let err_msg = format!("Failed to start llama-server.exe. Is it in the root folder? ({})", e);
                inject_system_message(&app, SystemLogLevel::Error, "Translator", &err_msg);
                emit_translator_state(&app, "Error", &err_msg); // ERROR STATE
                return;
            }
        };

        let _server_guard = ServerGuard(server_process);

        emit_translator_state(&app, "Loading Model", "Loading AI weights into VRAM..."); // STATE 2

        if server_health_check(&app) {
            emit_translator_state(&app, "Error", "AI Engine failed to start (OOM or missing model)."); // ERROR STATE
            return;
        }

        let client = Client::new();
        let dict_path = app.path().app_data_dir().unwrap().join("custom_dict.json");
        let custom_dict = load_dictionary(&dict_path);

        inject_system_message(&app, SystemLogLevel::Success, "Translator", "AI Server running! Ready for translation.");
        emit_translator_state(&app, "Active", "AI Engine Ready");

        // Sequential Processing Loop (No Batching, No Parallelism)
        while let Ok(job) = rx.recv() {
            let chat = job.chat;
            let pid = chat.pid;

            inject_system_message(&app, SystemLogLevel::Debug, "Translator", format!("[PID {}] Input JA: {}", pid, chat.message));

            // 1. Preprocess
            let shield = preprocess_text(&chat.message, &custom_dict, chat.nickname_romaji.as_deref(), Some(&chat.nickname));
            inject_system_message(&app, SystemLogLevel::Trace, "Translator", format!("[PID {}] Preprocessed (Masked): {}", pid, shield.masked_text));

            // 2. HTTP Request (Blocking)
            let req_start = Instant::now();
            let raw_translation = translate_text(&client, &shield.masked_text);

            inject_system_message(&app, SystemLogLevel::Trace, "Translator", format!("[PID {}] Raw AI Response ({}ms): {}", pid, req_start.elapsed().as_millis(), raw_translation));

            // 3. Postprocess
            let final_str = postprocess_text(&raw_translation, &shield);
            inject_system_message(&app, SystemLogLevel::Debug, "Translator", format!("[PID {}] Final Output KO: {}", pid, final_str));

            // --- 4. DATA LOGGER DISPATCH ---
            let state = app.state::<crate::AppState>();
            if let Some(df_tx) = state.data_factory_tx.lock().unwrap().as_ref() {
                let _ = df_tx.send(crate::io::DataFactoryJob {
                    pid: chat.pid,
                    original: chat.message.clone(),
                    translated: Some(final_str.clone()),
                });
            }

            // --- 5. UPDATE MEMORY HISTORY ---
            {
                let mut history = state.chat_history.lock().unwrap();
                if let Some(existing_chat) = history.get_mut(&chat.pid) {
                    existing_chat.translated = Some(final_str.clone());
                }
            }

            // --- 6. EMIT TO UI ---
            let result = crate::protocol::types::TranslationResult {
                pid: chat.pid,
                translated: final_str,
            };
            let _ = app.emit("translation-event", &result);
        }
    });

    tx
}

fn server_health_check(app: &AppHandle) -> bool {
    let client = Client::new();
    let mut is_ready = false;
    let start_wait = Instant::now();

    inject_system_message(app, SystemLogLevel::Info, "Translator", "Waiting for AI Engine to warm up...");

    // Poll the health endpoint for up to 30 seconds
    while start_wait.elapsed().as_secs() < 30 {
        inject_system_message(app, SystemLogLevel::Trace, "Translator", "Polling http://127.0.0.1:8080/health...");

        if let Ok(res) = client.get("http://127.0.0.1:8080/health").send() {
            if res.status().is_success() {
                inject_system_message(app, SystemLogLevel::Trace, "Translator", format!("Health check passed after {}ms", start_wait.elapsed().as_millis()));
                is_ready = true;
                break;
            } else {
                inject_system_message(app, SystemLogLevel::Trace, "Translator", format!("Health check returned status: {}", res.status()));
            }
        }
        thread::sleep(Duration::from_millis(1000));
    }

    if !is_ready {
        inject_system_message(app, SystemLogLevel::Error, "Translator", "AI Engine failed to initialize within 30s.");
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

pub fn emit_translator_state(app: &tauri::AppHandle, state: &str, message: &str) {
    let _ = app.emit("translator-state", crate::protocol::types::TranslatorStatePayload {
        state: state.to_string(),
        message: message.to_string(),
    });
}