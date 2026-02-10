use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

#[tauri::command]
pub fn start_translator_sidecar(app: AppHandle, use_gpu: bool) -> Result<String, String> {
    let model_path = crate::model_manager::get_model_path(&app);
    let model_path_str = model_path.to_string_lossy().to_string();

    if !model_path.exists() {
        return Err("Model file missing".to_string());
    }

    let gpu_arg = if use_gpu { "-1" } else { "0" };

    // 1. Configure the Sidecar Command
    let sidecar_command = app.shell()
        .sidecar("translator")
        .map_err(|e| e.to_string())?
        .args(&["--model", &model_path_str])
        .args(&["--gpu_layers", gpu_arg]);

    // 2. Spawn and Listen
    let (mut rx, mut _child) = sidecar_command
        .spawn()
        .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

    // 3. Handle Output (Stdout from Python)
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let CommandEvent::Stdout(line_bytes) = event {
                let line = String::from_utf8_lossy(&line_bytes);
                println!("[PY] {}", line); // Debug log

                // Parse JSON from Python and send to Frontend
                // We assume Python sends: {"type": "...", ...}
                app.emit("translator-event", line.to_string()).unwrap_or(());
            }
        }
    });

    Ok("Translator Started".to_string())
}