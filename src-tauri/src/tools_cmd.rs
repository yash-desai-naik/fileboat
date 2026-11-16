//! "Open in editor" and "Terminal here": the editors and terminals found, and opening them.

use crate::settings::SettingsStore;
use fileboat_core::tools::{self, Tool};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub editors: Vec<Tool>,
    pub terminals: Vec<Tool>,
}

#[tauri::command]
pub fn tools_list() -> Tools {
    Tools { editors: tools::editors(), terminals: tools::terminals() }
}

/// Open paths in an editor: `editor` if given, else the one chosen in Settings, else the first found.
#[tauri::command]
pub fn open_in_editor(settings: State<'_, SettingsStore>, paths: Vec<String>, editor: Option<String>) -> Result<(), String> {
    let editors = tools::editors();
    let id = editor.unwrap_or_else(|| settings.current().editor);
    let tool = tools::pick(&editors, &id).ok_or("No editor found. Install VS Code, Cursor or another editor.")?;
    tools::open_in_editor(tool, &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>())
}

/// Open a terminal in a folder: `terminal` if given, else the one chosen in Settings, else the first found.
#[tauri::command]
pub fn open_terminal(settings: State<'_, SettingsStore>, dir: String, terminal: Option<String>) -> Result<(), String> {
    let terminals = tools::terminals();
    let id = terminal.unwrap_or_else(|| settings.current().terminal);
    let tool = tools::pick(&terminals, &id).ok_or("No terminal found.")?;
    tools::open_terminal(tool, &PathBuf::from(dir))
}
