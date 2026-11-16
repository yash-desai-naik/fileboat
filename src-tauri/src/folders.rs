//! Open folders, one model per view, kept in Rust. The view asks for the rows on screen
//! (`folder_rows`) and hears about changes on its channel; a 200,000-file folder never sits in
//! the WebView.

use fileboat_core::fs::{self, Entry, Folder, View};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::State;

/// What a view hears about its folder.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FolderEvent {
    /// More entries were read (`done` once the listing is complete).
    Loaded { count: usize, total: usize, done: bool },
    /// The folder changed on disk (or its view was re-sorted): rows may have moved.
    Changed { count: usize, total: usize },
    /// The folder couldn't be read, or was deleted while open.
    Error { message: String },
}

pub struct OpenFolder {
    pub folder: Folder,
    /// Label of the window that owns it; closed together with that window.
    pub window: String,
    pub channel: Channel<FolderEvent>,
    /// False once the view closed it: the listing thread stops.
    pub alive: bool,
}

impl OpenFolder {
    pub fn changed(&self) {
        let _ = self.channel.send(FolderEvent::Changed { count: self.folder.len(), total: self.folder.total() });
    }
}

pub type Shared = Arc<Mutex<OpenFolder>>;

#[derive(Default)]
pub struct Folders {
    next: AtomicU64,
    open: Mutex<HashMap<String, Shared>>,
}

impl Folders {
    pub fn get(&self, id: &str) -> Result<Shared, String> {
        self.open.lock().unwrap().get(id).cloned().ok_or_else(|| "That folder is no longer open.".into())
    }

    fn close(&self, id: &str) {
        if let Some(f) = self.open.lock().unwrap().remove(id) {
            f.lock().unwrap().alive = false;
        }
    }

    /// A window closed: forget its folders.
    pub fn close_window(&self, window: &str) {
        let ids: Vec<String> =
            self.open.lock().unwrap().iter().filter(|(_, f)| f.lock().unwrap().window == window).map(|(k, _)| k.clone()).collect();
        for id in ids {
            self.close(&id);
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub id: String,
    /// The folder's absolute path, as shown.
    pub path: String,
    pub name: String,
    pub parent: Option<String>,
}

/// The name shown for a folder: its name, or the drive / root itself.
pub fn display_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Open a folder for a view: checks it, then lists it in the background. The first entries
/// arrive within milliseconds; the rest follow in batches (`Loaded` events).
#[tauri::command]
pub fn folder_open(
    window: tauri::Window,
    folders: State<'_, Folders>,
    path: String,
    base: Option<String>,
    view: View,
    on_event: Channel<FolderEvent>,
) -> Result<Opened, String> {
    let path = fs::resolve(&path, base.as_deref().map(std::path::Path::new));
    let meta = std::fs::metadata(&path).map_err(|e| fs::describe(&path, &e))?;
    if !meta.is_dir() {
        return Err(format!("{} is a file, not a folder.", path.display()));
    }
    let id = format!("f{}", folders.next.fetch_add(1, Ordering::Relaxed));
    let shared: Shared = Arc::new(Mutex::new(OpenFolder {
        folder: Folder::new(path.clone(), view),
        window: window.label().to_string(),
        channel: on_event,
        alive: true,
    }));
    folders.open.lock().unwrap().insert(id.clone(), shared.clone());

    let listing = path.clone();
    std::thread::spawn(move || {
        // The first batch goes out at once (a screenful); then batches at most every 300 ms, so
        // a huge folder is sorted a few times while loading rather than once per batch.
        let mut pending: Vec<Entry> = Vec::new();
        let mut last: Option<Instant> = None;
        let flush = |pending: &mut Vec<Entry>, done: bool| -> bool {
            let mut f = shared.lock().unwrap();
            if !f.alive {
                return false;
            }
            f.folder.extend(std::mem::take(pending));
            let _ = f.channel.send(FolderEvent::Loaded { count: f.folder.len(), total: f.folder.total(), done });
            true
        };
        let result = fs::list(&listing, 500, |batch| {
            pending.extend(batch);
            if last.is_none_or(|t| t.elapsed() >= Duration::from_millis(300)) {
                last = Some(Instant::now());
                return flush(&mut pending, false);
            }
            true
        });
        match result {
            Ok(()) => {
                flush(&mut pending, true);
            }
            Err(message) => {
                let _ = shared.lock().unwrap().channel.send(FolderEvent::Error { message });
            }
        }
    });

    Ok(Opened {
        id,
        name: display_name(&path),
        parent: path.parent().map(|p| p.display().to_string()),
        path: path.display().to_string(),
    })
}

/// Rows `start..end` of the view (fewer at the end).
#[tauri::command]
pub fn folder_rows(folders: State<'_, Folders>, id: String, start: usize, end: usize) -> Result<Vec<Entry>, String> {
    let f = folders.get(&id)?;
    let f = f.lock().unwrap();
    Ok(f.folder.rows(start, end).cloned().collect())
}

/// Change the sort or hidden files; the view hears `Changed` and re-reads its rows.
#[tauri::command]
pub fn folder_view(folders: State<'_, Folders>, id: String, view: View) -> Result<usize, String> {
    let f = folders.get(&id)?;
    let mut f = f.lock().unwrap();
    f.folder.set_view(view);
    f.changed();
    Ok(f.folder.len())
}

/// The row of a name (to keep the selection when rows move), if shown.
#[tauri::command]
pub fn folder_row_of(folders: State<'_, Folders>, id: String, name: String) -> Result<Option<usize>, String> {
    let f = folders.get(&id)?;
    let f = f.lock().unwrap();
    Ok(f.folder.row_of(&name))
}

/// Type-ahead: the first row at or after `from` (wrapping) whose name starts with `prefix`.
#[tauri::command]
pub fn folder_seek(folders: State<'_, Folders>, id: String, prefix: String, from: usize) -> Result<Option<usize>, String> {
    let f = folders.get(&id)?;
    let f = f.lock().unwrap();
    let prefix = prefix.to_lowercase();
    let n = f.folder.len();
    let rows: Vec<&Entry> = f.folder.rows(0, n).collect();
    Ok((0..n).map(|i| (from + i) % n.max(1)).find(|&i| rows[i].key.starts_with(&prefix)))
}

#[tauri::command]
pub fn folder_close(folders: State<'_, Folders>, id: String) {
    folders.close(&id);
}
