//! File operations the views ask for. The open folder's watcher shows the result; these do the
//! work, remember how to undo it, and report the outcome (or a plain reason why not).

use fileboat_core::ops::{self, undo::Op, undo::UndoLog};
use std::path::{Path, PathBuf};
use tauri::State;

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// "“a.txt”" or "3 items", for undo descriptions.
fn what(paths: &[PathBuf]) -> String {
    match paths {
        [one] => format!("“{}”", name_of(one)),
        many => format!("{} items", many.len()),
    }
}

/// A new folder in `dir`, named `name` or the first free "New folder (n)". Returns its name.
#[tauri::command]
pub fn op_new_folder(undo: State<'_, UndoLog>, dir: String, name: Option<String>) -> Result<String, String> {
    let dir = PathBuf::from(dir);
    let name = name.unwrap_or_else(|| ops::unique_name(&dir, "New folder"));
    let path = ops::create_folder(&dir, &name)?;
    undo.push(Op::Created(path), format!("New folder “{name}”"));
    Ok(name)
}

/// A new file in `dir`, named `name` or the first free "New file (n).txt", empty or with
/// `content` (templates like .gitignore). Returns its name.
#[tauri::command]
pub fn op_new_file(undo: State<'_, UndoLog>, dir: String, name: Option<String>, content: Option<String>) -> Result<String, String> {
    let dir = PathBuf::from(dir);
    let name = name.unwrap_or_else(|| ops::unique_name(&dir, "New file.txt"));
    let path = ops::create_file(&dir, &name)?;
    if let Some(text) = content.filter(|t| !t.is_empty()) {
        std::fs::write(&path, text).map_err(|e| format!("Couldn't write \"{name}\": {e}"))?;
    }
    undo.push(Op::Created(path), format!("New file “{name}”"));
    Ok(name)
}

/// Rename a file or folder in place. Returns the new full path.
#[tauri::command]
pub fn op_rename(undo: State<'_, UndoLog>, path: String, new_name: String) -> Result<String, String> {
    let from = PathBuf::from(path);
    let to = ops::rename(&from, new_name.trim())?;
    if to != from {
        undo.push(Op::Renamed { from: from.clone(), to: to.clone() }, format!("Rename “{}” to “{}”", name_of(&from), name_of(&to)));
    }
    Ok(to.display().to_string())
}

/// Move to the Recycle Bin / Trash (in the background: a big folder can take a moment).
#[tauri::command]
pub async fn op_trash(undo: State<'_, UndoLog>, paths: Vec<String>) -> Result<(), String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let at = fileboat_core::paths::now_secs() as i64;
    let moved = paths.clone();
    tauri::async_runtime::spawn_blocking(move || ops::delete::to_trash(&moved)).await.map_err(|e| e.to_string())??;
    let description = format!("Delete {}", what(&paths));
    undo.push(Op::Trashed { paths, at }, description);
    Ok(())
}

/// Delete for good (the UI has asked first). Can't be undone.
#[tauri::command]
pub async fn op_delete(paths: Vec<String>) -> Result<(), String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    tauri::async_runtime::spawn_blocking(move || ops::delete::delete_permanently(&paths)).await.map_err(|e| e.to_string())?
}

/// Ctrl+Z: reverse the most recent operation. Returns what was undone, or None if nothing.
#[tauri::command]
pub async fn op_undo(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri::Manager;
    // Restoring from the Recycle Bin can take a moment: not on the main thread.
    tauri::async_runtime::spawn_blocking(move || app.state::<UndoLog>().undo()).await.map_err(|e| e.to_string())?
}

/// Is this a usable name? (Checked as you type in the rename box.)
#[tauri::command]
pub fn op_check_name(name: String) -> Option<String> {
    ops::check_name(name.trim()).err()
}
