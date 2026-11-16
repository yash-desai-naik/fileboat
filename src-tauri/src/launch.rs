//! A folder (or file) named on the command line: `fileboat D:\projects`, `fileboat .`, a shortcut
//! or "Open with". The first launch opens it in a new tab; a second launch hands it to the running
//! app, which opens it in a new tab of its window.

use fileboat_core::fs::path_arg;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// The path given when Fileboat started, for the page to take once (`take_launch_path`).
#[derive(Default)]
pub struct LaunchPath(pub Mutex<Option<String>>);

#[tauri::command]
pub fn take_launch_path(launch: tauri::State<'_, LaunchPath>) -> Option<String> {
    launch.0.lock().unwrap().take()
}

/// A second launch: bring the window forward and open its path, if any, in a new tab.
pub fn second_launch(app: &AppHandle, args: Vec<String>, cwd: String) {
    let Some(window) = app.webview_windows().values().next().cloned() else { return };
    let _ = window.unminimize();
    let _ = window.set_focus();
    if let Some(path) = path_arg(&args, Path::new(&cwd)) {
        let _ = window.emit("open-path", path);
    }
}
