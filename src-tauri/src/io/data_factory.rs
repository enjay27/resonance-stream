use crate::{inject_system_message, ChatMessage, SystemLogLevel};
use chrono::Local;
use crossbeam_channel::{unbounded, Receiver, Sender};
use resonance_core::history::chat_log_file_name;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

pub struct DataFactoryJob {
    /// The message as shown, with its translation when it has one.
    pub chat: ChatMessage,
}

/// The archive lives in the app data folder -- the one "앱 데이터 폴더 열기"
/// opens. (It used to be written three levels up, into the user's home.)
fn dataset_path(app: &AppHandle) -> std::io::Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("dataset_raw.jsonl"))
}

/// Daily chat logs (full messages), reloaded as history on the next start.
pub fn chat_logs_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("Failed to resolve AppData directory")
        .join("chat_logs")
}

/// Appends to today's chat log, switching files when the date changes.
struct DailyLog {
    dir: PathBuf,
    day: String,
    writer: Option<BufWriter<File>>,
}

impl DailyLog {
    fn write(&mut self, chat: &ChatMessage) -> std::io::Result<()> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        if self.writer.is_none() || self.day != today {
            self.flush();
            std::fs::create_dir_all(&self.dir)?;
            let path = self.dir.join(chat_log_file_name(&today));
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            self.writer = Some(BufWriter::new(file));
            self.day = today;
        }
        let line = serde_json::to_string(chat).map_err(std::io::Error::other)?;
        match self.writer.as_mut() {
            Some(writer) => writeln!(writer, "{}", line),
            None => Ok(()),
        }
    }

    fn flush(&mut self) {
        if let Some(writer) = self.writer.as_mut() {
            let _ = writer.flush();
        }
    }
}

pub fn start_data_factory_worker(app: AppHandle) -> Sender<DataFactoryJob> {
    let (tx, rx): (Sender<DataFactoryJob>, Receiver<DataFactoryJob>) = unbounded();

    thread::spawn(move || {
        let opened = dataset_path(&app).and_then(|path| {
            let file = OpenOptions::new().create(true).append(true).open(&path)?;
            Ok((path, file))
        });
        let (path, file) = match opened {
            Ok(opened) => opened,
            Err(e) => {
                inject_system_message(
                    &app,
                    SystemLogLevel::Error,
                    "DataFactory",
                    format!("Cannot open the chat archive: {}", e),
                );
                return;
            }
        };
        inject_system_message(
            &app,
            SystemLogLevel::Info,
            "DataFactory",
            format!("Archiving chat to {}", path.display()),
        );

        // Two archives, as before the merge of both designs: the training
        // pairs (dataset_raw.jsonl) and the full daily chat logs. Files stay
        // open for the worker's life and are flushed whenever the queue runs
        // dry, so a burst is one write and nothing waits long on disk.
        let mut dataset = BufWriter::new(file);
        let mut daily = DailyLog {
            dir: chat_logs_dir(&app),
            day: String::new(),
            writer: None,
        };
        while let Ok(job) = rx.recv() {
            let chat = job.chat;
            let entry = serde_json::json!({
                "pid": chat.pid,
                "original": chat.message,
                "translated": chat.translated, // Some("text") or null
                "timestamp": now_ms()
            });
            let _ = writeln!(dataset, "{}", entry);
            if let Err(e) = daily.write(&chat) {
                log::warn!("[DataFactory] Chat log write failed: {}", e);
            }
            if rx.is_empty() {
                let _ = dataset.flush();
                daily.flush();
            }
        }
        let _ = dataset.flush();
        daily.flush();
    });

    tx
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
