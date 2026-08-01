use log::info;
use std::io::{Read, Write};
use std::path::PathBuf;
use tauri::Manager as TauriManager;
use zip::write::FileOptions;
use zip::ZipWriter;

/// Return the last `max_len` bytes of `s`, adjusted to start at a valid UTF-8
/// character boundary so the returned slice is always valid Unicode.
fn safe_tail(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }

    // Find the first byte index at or after (len - max_len) that is a char
    // boundary. The loop must terminate because s.len() itself is a boundary.
    let mut start = s.len() - max_len;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// Export a bug report ZIP to the user's Desktop.
///
/// # Arguments
/// * `context_json` — JSON string with user description + game context
/// * `save_json` — JSON string with the full serialized save/game state
///
/// Returns the path to the created .zip file, or an error string.
#[tauri::command]
pub fn export_bug_report(
    app_handle: tauri::AppHandle,
    context_json: String,
    save_json: String,
) -> Result<String, String> {
    info!("[cmd] export_bug_report");
    let desktop = dirs::desktop_dir().ok_or("Could not find Desktop directory")?;
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("bugreport-{}.zip", timestamp);
    let zip_path: PathBuf = desktop.join(&filename);
    info!("[cmd] export_bug_report: creating {}", zip_path.display());

    // Try to also include Tauri log files (last 50KB of last 3 logs)
    let logs_dir = app_handle.path().app_data_dir().map(|p| p.join("logs"));
    let log_contents = match &logs_dir {
        Ok(dir) if dir.exists() => {
            let mut combined = String::new();
            if let Ok(entries) = std::fs::read_dir(dir) {
                let mut log_files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map(|ext| ext == "log")
                            .unwrap_or(false)
                    })
                    .collect();
                log_files.sort_by_key(|e| e.path());
                for entry in log_files.iter().rev().take(3) {
                    if let Ok(mut f) = std::fs::File::open(entry.path()) {
                        let mut content = String::new();
                        if f.read_to_string(&mut content).is_ok() {
                            let log_name = entry.file_name().to_string_lossy().to_string();
                            combined.push_str(&format!("\n\n===== {} =====\n", log_name));
                            combined.push_str(safe_tail(&content, 50_000));
                        }
                    }
                }
            }
            combined
        }
        _ => String::new(),
    };

    let file =
        std::fs::File::create(&zip_path).map_err(|e| format!("Failed to create zip: {}", e))?;
    let mut zip = ZipWriter::new(file);

    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // context.json
    zip.start_file("context.json", options)
        .map_err(|e| format!("Failed to write context.json: {}", e))?;
    zip.write_all(context_json.as_bytes())
        .map_err(|e| format!("Failed to write context data: {}", e))?;

    // save.json
    zip.start_file("save.json", options)
        .map_err(|e| format!("Failed to write save.json: {}", e))?;
    zip.write_all(save_json.as_bytes())
        .map_err(|e| format!("Failed to write save data: {}", e))?;

    // logs.txt (if available)
    if !log_contents.is_empty() {
        zip.start_file("logs.txt", options)
            .map_err(|e| format!("Failed to write logs.txt: {}", e))?;
        zip.write_all(log_contents.as_bytes())
            .map_err(|e| format!("Failed to write logs: {}", e))?;
    }

    zip.finish()
        .map_err(|e| format!("Failed to finalize zip: {}", e))?;

    info!("[cmd] export_bug_report: done at {}", zip_path.display());

    Ok(zip_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::safe_tail;

    #[test]
    fn safe_tail_returns_full_string_when_below_max() {
        assert_eq!(safe_tail("hello", 10), "hello");
    }

    #[test]
    fn safe_tail_trims_ascii_to_last_bytes() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        assert_eq!(safe_tail(text, 5), "vwxyz");
    }

    #[test]
    fn safe_tail_does_not_split_multi_byte_characters() {
        // "αβγδε" is 10 bytes in UTF-8 (2 bytes each). A naive byte slice of
        // the last 3 bytes would start in the middle of "δ" and panic.
        let text = "αβγδε";
        assert_eq!(safe_tail(text, 3), "ε");
    }

    #[test]
    fn safe_tail_handles_empty_string() {
        assert_eq!(safe_tail("", 10), "");
    }

    #[test]
    fn safe_tail_handles_exact_length() {
        let text = "abcde";
        assert_eq!(safe_tail(text, 5), text);
    }
}
