use std::io::Write;

/// Console logger: coloured level, everything from this crate down to Trace,
/// other crates at Warn.
pub fn init_logger() {
    env_logger::builder()
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
        .filter_module("resonance_stream_lib", log::LevelFilter::Trace) // Show your debugs
        .init();
}
