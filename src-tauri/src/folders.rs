//! Open folders, one model per view, kept in Rust. The view asks for the rows on screen
//! (`folder_rows`) and hears about changes on its channel; a 200,000-file folder never sits in
//! the WebView. Open folders are watched, so changes made by other programs show at once.

use fileboat_core::fs::{self, Entry, Folder, View};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::State;

use crate::repos::{RepoState, Repos};
use crate::sizes::Sizer;
use crate::watch::{Changes, Watch};

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
    /// "Calculate Folder Sizes" was chosen here (sizes are worked out even with the setting off).
    pub sizes_requested: bool,
    /// The git repository it's in, if any.
    pub repo: Option<Arc<RepoState>>,
}

impl OpenFolder {
    /// Whether its subfolders' sizes are worked out.
    fn wants_sizes(&self) -> bool {
        self.alive && (self.folder.view().folder_sizes || self.sizes_requested)
    }
}

impl OpenFolder {
    pub fn changed(&self) {
        let _ = self.channel.send(FolderEvent::Changed { count: self.folder.len(), total: self.folder.total() });
    }
}

pub type Shared = Arc<Mutex<OpenFolder>>;

/// The app's open folders. Cloning is cheap (shared), so the watcher's thread holds one too.
#[derive(Clone, Default)]
pub struct Folders(Arc<Inner>);

#[derive(Default)]
struct Inner {
    next: AtomicU64,
    open: Mutex<HashMap<String, Shared>>,
    /// Started with the first folder; None if the OS refused (folders then aren't live).
    watch: OnceLock<Option<Watch>>,
    /// Started when a folder first wants sizes.
    sizer: OnceLock<Arc<Sizer>>,
    /// Started with the first folder.
    repos: OnceLock<Arc<Repos>>,
    /// For events (git changes); set at start-up.
    app: OnceLock<tauri::AppHandle>,
}

/// The same folder (see `sizes::path_key`).
fn same_path(a: &Path, b: &Path) -> bool {
    a == b || crate::sizes::path_key(a) == crate::sizes::path_key(b)
}

impl Folders {
    pub fn get(&self, id: &str) -> Result<Shared, String> {
        self.0.open.lock().unwrap().get(id).cloned().ok_or_else(|| "That folder is no longer open.".into())
    }

    fn watch(&self) -> Option<&Watch> {
        self.0
            .watch
            .get_or_init(|| {
                let folders = self.clone();
                Watch::start(move |changes| folders.apply(changes)).ok()
            })
            .as_ref()
    }

    pub fn set_app(&self, app: tauri::AppHandle) {
        let _ = self.0.app.set(app);
    }

    pub fn app(&self) -> Option<&tauri::AppHandle> {
        self.0.app.get()
    }

    pub fn repos(&self) -> &Arc<Repos> {
        self.0.repos.get_or_init(|| Repos::start(self.clone()))
    }

    /// Show the repository's last status on a folder's rows (or no marks outside a repository).
    fn fill_git(&self, shared: &Shared) {
        let mut f = shared.lock().unwrap();
        if !f.alive {
            return;
        }
        let Some(state) = f.repo.clone() else { return };
        let snap = state.snapshot.lock().unwrap().clone();
        let base = fileboat_core::git::relative(&state.repo, &f.folder.path);
        let changed = match (snap, base) {
            (Some(snap), Some(base)) => f.folder.set_git(|e| {
                let rel = if base.is_empty() { e.name.clone() } else { format!("{base}/{}", e.name) };
                snap.index.mark(&rel, e.kind == fs::Kind::Folder)
            }),
            _ => f.folder.set_git(|_| None),
        };
        if changed {
            f.changed();
        }
    }

    /// A repository's status changed: its open folders show it.
    pub fn git_changed(&self, state: &Arc<RepoState>) {
        let open: Vec<Shared> = self.0.open.lock().unwrap().values().cloned().collect();
        for shared in open {
            let mine = shared.lock().unwrap().repo.as_ref().is_some_and(|r| Arc::ptr_eq(r, state));
            if mine {
                self.fill_git(&shared);
            }
        }
    }

    fn sizer(&self) -> &Arc<Sizer> {
        self.0.sizer.get_or_init(|| Sizer::start(self.clone()))
    }

    /// A folder's size, if the sizer has it.
    pub fn cached_size(&self, path: &Path) -> Option<fs::TreeSize> {
        self.0.sizer.get()?.cached(path)
    }

    /// Whether an open folder at `dir` wants its subfolders sized.
    pub fn wants_sizes(&self, dir: &Path) -> bool {
        let open: Vec<Shared> = self.0.open.lock().unwrap().values().cloned().collect();
        open.iter().any(|f| {
            let f = f.lock().unwrap();
            f.wants_sizes() && same_path(&f.folder.path, dir)
        })
    }

    /// Show the sizes already known for a folder's subfolders, and work out the rest.
    fn fill_sizes(&self, shared: &Shared) {
        let missing = {
            let mut f = shared.lock().unwrap();
            if !f.wants_sizes() {
                return;
            }
            let sizer = self.sizer();
            let path = f.folder.path.clone();
            let mut known = Vec::new();
            let mut missing = Vec::new();
            for name in f.folder.unsized_folders() {
                let p = path.join(&name);
                match sizer.cached(&p) {
                    Some(size) => known.push((name, size.bytes)),
                    None => missing.push(p),
                }
            }
            if f.folder.set_sizes(known.iter().map(|(n, b)| (n.as_str(), *b))) {
                f.changed();
            }
            missing
        };
        self.sizer().queue(missing);
    }

    /// Fileboat changed what's under these paths (a copy or move finished): sizes that include
    /// them are worked out again. Changes deeper than an open folder's own items aren't seen by
    /// the watcher, so this is how dropping files onto a subfolder updates its size.
    pub fn sizes_changed(&self, paths: &[PathBuf]) {
        let Some(sizer) = self.0.sizer.get() else { return };
        for p in paths {
            sizer.forget_up(p);
        }
        let open: Vec<Shared> = self.0.open.lock().unwrap().values().cloned().collect();
        let mut again = Vec::new();
        for shared in open {
            let f = shared.lock().unwrap();
            if !f.wants_sizes() {
                continue;
            }
            for p in paths {
                // The folder's own item that holds `p` (`p` itself when it's one).
                let item = p.ancestors().find(|a| a.parent().is_some_and(|d| same_path(d, &f.folder.path)));
                if let Some(item) = item
                    && f.folder.get(&item.file_name().unwrap_or_default().to_string_lossy()).is_some_and(|e| e.kind == fs::Kind::Folder)
                {
                    again.push(item.to_path_buf());
                }
            }
        }
        sizer.queue(again);
    }

    /// Sizes worked out (by the sizer): give them to the open folders they're in.
    pub fn apply_sizes(&self, done: &[(PathBuf, fs::TreeSize)]) {
        let open: Vec<Shared> = self.0.open.lock().unwrap().values().cloned().collect();
        for shared in open {
            let mut f = shared.lock().unwrap();
            if !f.wants_sizes() {
                continue;
            }
            let path = f.folder.path.clone();
            let mine = done
                .iter()
                .filter(|(p, _)| p.parent().is_some_and(|d| same_path(d, &path)))
                .filter_map(|(p, size)| Some((p.file_name()?.to_str()?, size.bytes)));
            if f.folder.set_sizes(mine) {
                f.changed();
            }
        }
    }

    fn close(&self, id: &str) {
        let Some(f) = self.0.open.lock().unwrap().remove(id) else { return };
        let (path, repo) = {
            let mut f = f.lock().unwrap();
            f.alive = false;
            (f.folder.path.clone(), f.repo.take())
        };
        if let Some(w) = self.watch() {
            w.unwatch(&path);
        }
        if let Some(repo) = repo {
            self.repos().detach(&repo);
        }
    }

    /// A window closed: forget its folders.
    pub fn close_window(&self, window: &str) {
        let ids: Vec<String> =
            self.0.open.lock().unwrap().iter().filter(|(_, f)| f.lock().unwrap().window == window).map(|(k, _)| k.clone()).collect();
        for id in ids {
            self.close(&id);
        }
    }

    /// Changes from the watcher: re-read the changed names in each open folder they belong to
    /// (outside the lock), apply them, and tell the views.
    fn apply(&self, changes: Changes) {
        // Sizes that include what changed are out of date.
        if let Some(sizer) = self.0.sizer.get() {
            for p in &changes.paths {
                sizer.forget_up(p);
            }
        }
        let open: Vec<Shared> = self.0.open.lock().unwrap().values().cloned().collect();
        for shared in open {
            let path = shared.lock().unwrap().folder.path.clone();
            if changes.rescan {
                if let Some(sizer) = self.0.sizer.get() {
                    sizer.forget_under(&path);
                }
                relist(self, &shared, &path);
                continue;
            }
            let inside: Vec<&PathBuf> = changes.paths.iter().filter(|p| p.parent().is_some_and(|d| same_path(d, &path))).collect();
            // The folder itself may be gone: Windows reports its files being deleted, not the
            // watched folder, so check whenever anything in or about it changed.
            let touched = !inside.is_empty() || changes.paths.iter().any(|p| same_path(p, &path));
            if touched && !path.is_dir() {
                let message = format!("{} was deleted or moved.", path.display());
                let _ = shared.lock().unwrap().channel.send(FolderEvent::Error { message });
                continue;
            }
            let mut upserts = Vec::new();
            let mut removed = Vec::new();
            for p in inside {
                let Some(name) = p.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
                // On Windows and macOS an old spelling still finds the file after a case-only
                // rename: only the name it really has on disk is an entry.
                let real = if cfg!(any(windows, target_os = "macos")) { fs::real_name(p) } else { Some(name.clone()) };
                match real {
                    Some(real) if real == name => upserts.extend(Entry::from_path(p)),
                    Some(real) => {
                        removed.push(name);
                        upserts.extend(Entry::from_path(&path.join(real)));
                    }
                    None => removed.push(name),
                }
            }
            if upserts.is_empty() && removed.is_empty() {
                continue;
            }
            {
                let mut f = shared.lock().unwrap();
                if !f.alive {
                    continue;
                }
                f.folder.apply(upserts, &removed);
                f.changed();
            }
            self.fill_git(&shared);
            self.fill_sizes(&shared);
        }
    }
}

/// Read a folder again from scratch (the OS lost track of its changes).
fn relist(folders: &Folders, shared: &Shared, path: &Path) {
    let mut all = Vec::new();
    let result = fs::list(path, 4096, |batch| {
        all.extend(batch);
        true
    });
    {
        let mut f = shared.lock().unwrap();
        match result {
            Ok(()) => {
                f.folder.replace(all);
                f.changed();
            }
            Err(message) => {
                let _ = f.channel.send(FolderEvent::Error { message });
                return;
            }
        }
    }
    folders.fill_git(shared);
    folders.fill_sizes(shared);
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
    let id = format!("f{}", folders.0.next.fetch_add(1, Ordering::Relaxed));
    let repo = folders.repos().attach(&path);
    let shared: Shared = Arc::new(Mutex::new(OpenFolder {
        folder: Folder::new(path.clone(), view),
        window: window.label().to_string(),
        channel: on_event,
        alive: true,
        sizes_requested: false,
        repo,
    }));
    folders.0.open.lock().unwrap().insert(id.clone(), shared.clone());
    if let Some(w) = folders.watch() {
        w.watch(&path);
    }

    let listing = path.clone();
    let folders = folders.inner().clone();
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
                if flush(&mut pending, true) {
                    folders.fill_git(&shared);
                    folders.fill_sizes(&shared);
                }
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

/// What a selection holds, for the status bar: how many files and folders, and the total size
/// of the files and of the folders already sized.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionInfo {
    pub files: usize,
    pub folders: usize,
    pub bytes: u64,
}

#[tauri::command]
pub fn folder_selection_info(folders: State<'_, Folders>, id: String, names: Vec<String>) -> Result<SelectionInfo, String> {
    let f = folders.get(&id)?;
    let f = f.lock().unwrap();
    let mut info = SelectionInfo { files: 0, folders: 0, bytes: 0 };
    for e in names.iter().filter_map(|n| f.folder.get(n)) {
        if e.kind == fs::Kind::Folder {
            info.folders += 1;
            if e.sized {
                info.bytes += e.size;
            }
        } else {
            info.files += 1;
            info.bytes += e.size;
        }
    }
    Ok(info)
}

/// The entries with these names (the selection, for a context menu), in the order given.
#[tauri::command]
pub fn folder_entries(folders: State<'_, Folders>, id: String, names: Vec<String>) -> Result<Vec<Entry>, String> {
    let f = folders.get(&id)?;
    let f = f.lock().unwrap();
    Ok(names.iter().filter_map(|n| f.folder.get(n).cloned()).collect())
}

/// Names of rows `start..end` (select all, Shift+click across rows not on screen).
#[tauri::command]
pub fn folder_names(folders: State<'_, Folders>, id: String, start: usize, end: usize) -> Result<Vec<String>, String> {
    let f = folders.get(&id)?;
    let f = f.lock().unwrap();
    Ok(f.folder.rows(start, end).map(|e| e.name.clone()).collect())
}

/// Change the sort or hidden files; the view hears `Changed` and re-reads its rows.
#[tauri::command]
pub fn folder_view(folders: State<'_, Folders>, id: String, view: View) -> Result<usize, String> {
    let shared = folders.get(&id)?;
    let len = {
        let mut f = shared.lock().unwrap();
        f.folder.set_view(view);
        f.changed();
        f.folder.len()
    };
    folders.fill_sizes(&shared);
    Ok(len)
}

/// "Calculate Folder Sizes": work out this folder's subfolder sizes (whatever the setting); the
/// view hears `Changed` as they arrive.
#[tauri::command]
pub fn folder_sizes(folders: State<'_, Folders>, id: String) -> Result<(), String> {
    let shared = folders.get(&id)?;
    shared.lock().unwrap().sizes_requested = true;
    folders.fill_sizes(&shared);
    Ok(())
}

/// The folder's groups, in order (empty when not grouping).
#[tauri::command]
pub fn folder_groups(folders: State<'_, Folders>, id: String) -> Result<Vec<fs::Group>, String> {
    let f = folders.get(&id)?;
    let f = f.lock().unwrap();
    Ok(f.folder.groups().to_vec())
}

/// Collapse or expand a group; the view hears `Changed`.
#[tauri::command]
pub fn folder_collapse(folders: State<'_, Folders>, id: String, key: String, collapsed: bool) -> Result<(), String> {
    let f = folders.get(&id)?;
    let mut f = f.lock().unwrap();
    f.folder.set_collapsed(&key, collapsed);
    f.changed();
    Ok(())
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

/// Read the folder again from disk (F5; or when its changes can't be watched, e.g. some network
/// shares). The view hears `Changed` when done.
#[tauri::command]
pub fn folder_refresh(folders: State<'_, Folders>, id: String) -> Result<(), String> {
    let shared = folders.get(&id)?;
    let path = shared.lock().unwrap().folder.path.clone();
    // Sizes are worked out again too.
    if let Some(sizer) = folders.0.sizer.get() {
        sizer.forget_under(&path);
    }
    let folders = folders.inner().clone();
    std::thread::spawn(move || relist(&folders, &shared, &path));
    Ok(())
}

/// The page (re)loaded: folders a previous page of this window left open are closed (a reload
/// doesn't destroy the window, so they would otherwise stay open and watched).
#[tauri::command]
pub fn folders_reset(window: tauri::Window, folders: State<'_, Folders>) {
    folders.close_window(window.label());
}

#[tauri::command]
pub fn folder_close(folders: State<'_, Folders>, id: String) {
    folders.close(&id);
}
