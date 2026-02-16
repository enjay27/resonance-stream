use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // --- 1. Resource Sync Logic ---
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    // Path to where the .exe is being built (e.g., target/debug)
    let target_dir = Path::new(&manifest_dir).join("target").join(profile);

    // Your 'bin' folder relative to src-tauri
    let files = [
        "bin/WinDivert.dll",
        "bin/WinDivert64.sys",
        "resources/models.json"
    ];

    let files = [
        "bin/WinDivert.dll",
        "bin/WinDivert64.sys",
        "resources/models.json" // [ADD THIS] Sync models for dev mode
    ];

    for file in files {
        let src = Path::new(&manifest_dir).join(file);

        // Maintain the path: 'bin/WinDivert.dll' or 'resources/models.json'
        let dest = target_dir.join(file);

        if src.exists() {
            // [FIX] Create subdirectories (bin/ or resources/) in target folder
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).ok();
            }
            let _ = fs::copy(&src, &dest);
        }
    }

    // --- 2. Existing Manifest Logic ---
    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("app.manifest"));

    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(windows)
    )
        .expect("failed to run build script");
}