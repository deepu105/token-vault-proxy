use colored::Colorize;
use serde_json::json;
use std::io::Write;

/// Whether JSON output mode is active.
/// Resolved from --json flag or TV_PROXY_OUTPUT=json env var.
pub fn is_json_mode(json_flag: bool) -> bool {
    json_flag || std::env::var("TV_PROXY_OUTPUT").map_or(false, |v| v == "json")
}

/// Print command output. In JSON mode, writes structured JSON to stdout.
/// Otherwise prints human-friendly text to stdout.
pub fn output(data: serde_json::Value, human_text: &str, json_mode: bool) {
    if json_mode {
        let mut stdout = std::io::stdout().lock();
        let _ = serde_json::to_writer_pretty(&mut stdout, &data);
        let _ = writeln!(stdout);
    } else {
        println!("{human_text}");
    }
}

/// Print an error. In JSON mode writes { "error": ... } to stdout so agents
/// can parse it; otherwise writes human text to stderr.
pub fn output_error(code: &str, message: &str, json_mode: bool) {
    if json_mode {
        let data = json!({
            "error": {
                "code": code,
                "message": message,
            }
        });
        let mut stdout = std::io::stdout().lock();
        let _ = serde_json::to_writer_pretty(&mut stdout, &data);
        let _ = writeln!(stdout);
    } else {
        eprintln!("{} {message}", "Error:".red().bold());
    }
}
