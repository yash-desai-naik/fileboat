//! Small JSON files the UI keeps between runs (open tabs and their history), in
//! `<data dir>/state/`. Nothing secret is stored here.

use fileboat_core::paths::{data_dir, write_atomic};

fn file(name: &str) -> Result<std::path::PathBuf, String> {
    // Names are fixed words or window labels: never paths.
    if name.is_empty() || name.len() > 80 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("Invalid state name.".into());
    }
    let dir = data_dir()?.join("state");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("{name}.json")))
}

#[tauri::command]
pub fn state_read(name: String) -> Result<Option<String>, String> {
    match std::fs::read_to_string(file(&name)?) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn state_write(name: String, contents: String) -> Result<(), String> {
    if contents.len() > 5_000_000 {
        return Err("State too large to save.".into());
    }
    write_atomic(&file(&name)?, &contents).map_err(|e| e.to_string())
}
