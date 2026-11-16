//! The app's settings store and commands (the settings themselves are in `fileboat_core::prefs`).
//!
//! Claude (and, later, `fb settings`) can change settings.json while the app runs:
//! the store re-reads the file when it changed on disk, and the windows reload their copy when
//! they get focus or hear `settings:changed`.

use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;
use fileboat_core::prefs;
use tauri::State;

pub use prefs::Settings;

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<(Settings, Option<SystemTime>)>,
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl SettingsStore {
    pub fn load() -> Result<Self, String> {
        let path = prefs::path()?;
        let settings = prefs::load(&path);
        prefs::save(&path, &settings)?;
        let stamp = modified(&path);
        Ok(Self { path, current: Mutex::new((settings, stamp)) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (Settings, Option<SystemTime>)> {
        let mut guard = self.current.lock().unwrap_or_else(|e| e.into_inner());
        let now = modified(&self.path);
        if now.is_some() && now != guard.1 {
            *guard = (prefs::load(&self.path), now);
        }
        guard
    }

    pub fn current(&self) -> Settings {
        self.lock().0.clone()
    }
}

#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.current()
}

/// Merge `patch` (a partial settings object) into the current settings and save.
#[tauri::command]
pub fn update_settings(app: tauri::AppHandle, store: State<'_, SettingsStore>, patch: Value) -> Result<Settings, String> {
    use tauri::Manager;
    let mut guard = store.lock();
    let next = guard.0.merged(&patch)?;
    prefs::save(&store.path, &next)?;
    *guard = (next.clone(), modified(&store.path));
    drop(guard);
    // The index follows its settings (it rebuilds only when what's indexed changed).
    if let Some(indexer) = app.try_state::<std::sync::Arc<crate::indexer::Indexer>>() {
        indexer.configure(&next);
    }
    Ok(next)
}

/// Open settings.json in the user's default editor for .json files (Notepad / TextEdit if none).
#[tauri::command]
pub fn open_settings_file(store: State<'_, SettingsStore>) -> Result<String, String> {
    let path = &store.path;
    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .status();
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg("-t").arg(path).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(path).status();
    match status {
        Ok(s) if s.success() => Ok(path.display().to_string()),
        _ => {
            #[cfg(windows)]
            let fallback = std::process::Command::new("notepad.exe").arg(path).spawn();
            #[cfg(not(windows))]
            let fallback = std::process::Command::new("open").arg(path).spawn();
            fallback.map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
    }
}
