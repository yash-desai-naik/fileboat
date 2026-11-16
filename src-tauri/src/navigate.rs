//! The address bar: what a typed path is, and folders to complete it with.

use fileboat_core::fs;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    /// The absolute path, as shown.
    pub path: String,
    /// "folder", "file" or "missing".
    pub kind: &'static str,
    /// For a file: its folder and name, so the view can open the folder and select it.
    pub parent: Option<String>,
    pub name: Option<String>,
}

/// What a typed or pasted path points to (`~`, `%VAR%`, `$VAR`, relative to `base`…).
#[tauri::command]
pub fn path_resolve(input: String, base: Option<String>) -> Resolved {
    let path = fs::resolve(&input, base.as_deref().map(Path::new));
    let kind = match std::fs::metadata(&path) {
        Ok(m) if m.is_dir() => "folder",
        Ok(_) => "file",
        Err(_) => "missing",
    };
    Resolved {
        parent: path.parent().filter(|_| kind == "file").map(|p| p.display().to_string()),
        name: path.file_name().filter(|_| kind == "file").map(|n| n.to_string_lossy().into_owned()),
        path: path.display().to_string(),
        kind,
    }
}

/// Folders completing a partly typed path (or every subfolder of a path ending in a separator).
#[tauri::command]
pub fn path_complete(input: String, base: Option<String>, limit: Option<usize>) -> Vec<String> {
    fs::complete(&input, base.as_deref().map(Path::new), limit.unwrap_or(50))
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}
