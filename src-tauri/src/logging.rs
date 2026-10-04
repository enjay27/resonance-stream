use resonance_core::test_env::Tee;
use std::io::Write;
use std::path::Path;

/// Console logger: coloured level, everything from this crate down to Trace,
/// other crates at Warn. With `log_file` (a test run's `--log-file`) the log is
/// also written there, without colours -- a release exe has no console to read.
pub fn init_logger(log_file: Option<&Path>) {
    let mut builder = env_logger::builder();
    builder
        .format(|buf, record| {
            // 1. Get the default ANSI style for the log level (Info=Green, Warn=Yellow, etc.)
            let level_style = buf.default_level_style(record.level());

            // 2. Apply the styles using the 0.11 `{style}text{style:#}` pattern
            writeln!(
                buf,
                "[{timestamp} {level_style}{level}{level_style:#}] {message}",
                timestamp = buf.timestamp(),
                level_style = level_style, // Turns level color ON
                level = record.level(),
                // {level_style:#} magically turns the color OFF
                message = record.args()
            )
        })
        .filter_level(log::LevelFilter::Warn) // Keep other crates quiet
        .filter_module("resonance_stream_lib", log::LevelFilter::Trace); // Show your debugs

    if let Some(path) = log_file {
        match open_log_file(path) {
            Ok(file) => {
                builder.write_style(env_logger::WriteStyle::Never).target(
                    env_logger::Target::Pipe(Box::new(Tee(std::io::stderr(), file))),
                );
            }
            Err(e) => eprintln!(
                "resonance-stream: cannot open log file {}: {e}",
                path.display()
            ),
        }
    }
    builder.init();
}

fn open_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
}
