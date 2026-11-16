//! Fileboat's own folders, for the About box, and opening things with the desktop.

use fileboat_core::paths::{cache_dir, data_dir};

/// Open a file, folder or web address with the desktop: Explorer, Finder or xdg-open.
pub fn open_with_desktop(arg: &std::path::Path) -> Result<(), String> {
    let program = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(arg).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Where Fileboat keeps things, for the About box.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    /// Settings, tab and window state.
    data_dir: String,
    /// Things that can be rebuilt: the search index, icon caches.
    cache_dir: String,
    /// The running program.
    exe: String,
}

#[tauri::command]
pub fn app_paths() -> Result<AppPaths, String> {
    Ok(AppPaths {
        data_dir: data_dir()?.display().to_string(),
        cache_dir: cache_dir()?.display().to_string(),
        exe: std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
    })
}

/// Open one of Fileboat's folders in the file manager ("data" or "cache").
#[tauri::command]
pub fn open_app_folder(which: String) -> Result<(), String> {
    let dir = match which.as_str() {
        "data" => data_dir()?,
        "cache" => cache_dir()?,
        _ => return Err("Unknown folder.".into()),
    };
    open_with_desktop(&dir)
}

/// Fileboat's releases on GitHub, in the default browser (a fixed address, nothing from the page).
#[tauri::command]
pub fn open_releases_page() -> Result<(), String> {
    open_with_desktop(std::path::Path::new("https://github.com/yash-desai-naik/fileboat/releases"))
}
