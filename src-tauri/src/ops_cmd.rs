//! File operations the views ask for. The open folder's watcher shows the result; these only do
//! the work and report the outcome (or a plain reason why not).

use fileboat_core::ops;
use std::path::PathBuf;

/// A new folder in `dir`, named `name` or the first free "New folder (n)". Returns its name.
#[tauri::command]
pub fn op_new_folder(dir: String, name: Option<String>) -> Result<String, String> {
    let dir = PathBuf::from(dir);
    let name = name.unwrap_or_else(|| ops::unique_name(&dir, "New folder"));
    ops::create_folder(&dir, &name)?;
    Ok(name)
}

/// A new empty file in `dir`, named `name` or the first free "New file (n).txt". Returns its name.
#[tauri::command]
pub fn op_new_file(dir: String, name: Option<String>) -> Result<String, String> {
    let dir = PathBuf::from(dir);
    let name = name.unwrap_or_else(|| ops::unique_name(&dir, "New file.txt"));
    ops::create_file(&dir, &name)?;
    Ok(name)
}

/// Rename a file or folder in place. Returns the new full path.
#[tauri::command]
pub fn op_rename(path: String, new_name: String) -> Result<String, String> {
    ops::rename(&PathBuf::from(path), new_name.trim()).map(|p| p.display().to_string())
}

/// Is this a usable name? (Checked as you type in the rename box.)
#[tauri::command]
pub fn op_check_name(name: String) -> Option<String> {
    ops::check_name(name.trim()).err()
}
