//! The live search index: loaded from its file at start-up and reconciled with the disk (only
//! folders whose modified time changed are re-listed), built from scratch when there's no file
//! or the roots or rules changed, kept current by a recursive watch on each root, and saved now
//! and then. All of it runs on one background thread at low priority; searches take a read
//! lock and run in parallel.

use crate::watch::{Changes, Watch};
use fileboat_core::index::{self, Index, Query, Rules, SearchResult, FLAG_UNLISTED, KIND_FOLDER};
use fileboat_core::prefs::Settings;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};
use tauri::Emitter;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// "off" | "loading" | "building" | "reconciling" | "ready" | "paused"
    pub phase: &'static str,
    pub entries: usize,
    /// Entries walked so far while building.
    pub progress: usize,
    pub bytes: usize,
    pub roots: Vec<String>,
    pub error: Option<String>,
}

/// What the index is built from.
#[derive(Clone, PartialEq)]
struct Config {
    enabled: bool,
    roots: Vec<PathBuf>,
    rules_off: Vec<String>,
    excludes: Vec<String>,
    includes: Vec<String>,
    hidden: bool,
    gitignore: bool,
}

impl Config {
    fn from(s: &Settings) -> Config {
        let mut roots: Vec<PathBuf> = s.index_roots.iter().map(PathBuf::from).filter(|p| p.is_absolute()).collect();
        if roots.is_empty() {
            roots.extend(dirs::home_dir());
        }
        Config {
            enabled: s.index_enabled,
            roots,
            rules_off: s.index_rules_off.clone(),
            excludes: s.index_excludes.clone(),
            includes: s.index_includes.clone(),
            hidden: s.index_hidden,
            gitignore: s.index_gitignore,
        }
    }

    fn rules(&self) -> Rules {
        Rules::new(&self.rules_off, &self.excludes, &self.includes, self.hidden, self.gitignore)
    }
}

pub struct Indexer {
    index: RwLock<Index>,
    status: Mutex<Status>,
    config: Mutex<Option<Config>>,
    /// A newer configuration or a rebuild cancels the running build.
    generation: AtomicU64,
    paused: AtomicBool,
    dirty: AtomicBool,
    progress: AtomicUsize,
    watch: Mutex<Option<Watch>>,
    app: tauri::AppHandle,
    me: Weak<Indexer>,
    /// Folders searched that aren't in the index, walked once and kept briefly.
    scratch: Mutex<Option<(PathBuf, Instant, Arc<Index>)>>,
}

fn file() -> Option<PathBuf> {
    let dir = fileboat_core::paths::cache_dir().ok()?.join("index");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("index.bin"))
}

/// Run the current thread at background priority (disk and CPU), as the indexer should.
fn background_priority() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Indexer {
    pub fn start(app: tauri::AppHandle, settings: &Settings) -> Arc<Indexer> {
        let me = Arc::new_cyclic(|me: &Weak<Indexer>| Indexer {
            index: RwLock::new(Index::default()),
            status: Mutex::new(Status { phase: "loading", entries: 0, progress: 0, bytes: 0, roots: Vec::new(), error: None }),
            config: Mutex::new(None),
            generation: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            progress: AtomicUsize::new(0),
            watch: Mutex::new(None),
            app,
            me: me.clone(),
            scratch: Mutex::new(None),
        });
        me.configure(settings);
        // Save now and then while changes come in.
        let weak = Arc::downgrade(&me);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(120));
            let Some(me) = weak.upgrade() else { break };
            me.save_if_dirty();
        });
        me
    }

    fn set_phase(&self, phase: &'static str) {
        let status = {
            let idx = self.index.read().unwrap();
            let cfg = self.config.lock().unwrap();
            let mut s = self.status.lock().unwrap();
            s.phase = phase;
            s.entries = idx.live();
            s.bytes = idx.bytes();
            s.progress = self.progress.load(Ordering::Relaxed);
            s.roots = cfg.as_ref().map(|c| c.roots.iter().map(|r| r.display().to_string()).collect()).unwrap_or_default();
            s.clone()
        };
        let _ = self.app.emit("index-status", status);
    }

    pub fn status(&self) -> Status {
        let mut s = self.status.lock().unwrap().clone();
        s.progress = self.progress.load(Ordering::Relaxed);
        s
    }

    /// The settings changed (or start-up): rebuild if what's indexed changed.
    pub fn configure(&self, settings: &Settings) {
        let next = Config::from(settings);
        {
            let mut cfg = self.config.lock().unwrap();
            if cfg.as_ref() == Some(&next) {
                return;
            }
            *cfg = Some(next);
        }
        self.restart(false);
    }

    /// Start over: load (or build, with `rebuild` or when the file doesn't match) and watch.
    pub fn restart(&self, rebuild: bool) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(cfg) = self.config.lock().unwrap().clone() else { return };
        // The old watch goes; a new one is made once the index is ready.
        *self.watch.lock().unwrap() = None;
        if !cfg.enabled {
            *self.index.write().unwrap() = Index::default();
            self.set_phase("off");
            return;
        }
        let Some(me) = self.me.upgrade() else { return };
        std::thread::Builder::new()
            .name("fileboat-index".into())
            .spawn(move || {
                background_priority();
                me.run(&cfg, generation, rebuild);
            })
            .ok();
    }

    fn cancelled(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) != generation
    }

    fn run(&self, cfg: &Config, generation: u64, rebuild: bool) {
        let rules = cfg.rules();
        let stale = |me: &Indexer| me.cancelled(generation);
        // The saved index, if it was built from the same roots and rules.
        let loaded = (!rebuild).then(file).flatten().and_then(|f| Index::load(&f).ok()).filter(|idx| {
            idx.rules == rules.fingerprint() && idx.roots().iter().map(|(p, _)| p.clone()).collect::<Vec<_>>() == cfg.roots
        });
        match loaded {
            Some(idx) => {
                *self.index.write().unwrap() = idx;
                self.set_phase("reconciling");
                self.reconcile(&rules, &|| stale(self));
            }
            None => {
                self.progress.store(0, Ordering::Relaxed);
                self.set_phase("building");
                let mut last = Instant::now();
                let built = index::build(&cfg.roots, &rules, &|| stale(self), &mut |n| {
                    self.progress.store(n, Ordering::Relaxed);
                    if last.elapsed() > Duration::from_millis(500) {
                        last = Instant::now();
                        let mut s = self.status.lock().unwrap().clone();
                        s.progress = n;
                        let _ = self.app.emit("index-status", s);
                    }
                });
                if stale(self) {
                    return;
                }
                *self.index.write().unwrap() = built;
                self.dirty.store(true, Ordering::Relaxed);
            }
        }
        if stale(self) {
            return;
        }
        self.save_if_dirty();
        self.watch_roots(cfg);
        self.set_phase(if self.paused.load(Ordering::Relaxed) { "paused" } else { "ready" });
    }

    /// Re-list every indexed folder whose modified time changed since the index was saved
    /// (adding, removing or renaming an entry changes its folder's time).
    fn reconcile(&self, rules: &Rules, stale: &dyn Fn() -> bool) {
        let folders: Vec<(u32, PathBuf, i64)> = {
            let idx = self.index.read().unwrap();
            (0..idx.len() as u32)
                .filter(|&i| idx.is_live(i) && idx.kind(i) == KIND_FOLDER && idx.flags(i) & FLAG_UNLISTED == 0)
                .map(|i| (i, idx.path(i), idx.mtime(i)))
                .collect()
        };
        for (id, path, saved) in folders {
            if stale() {
                return;
            }
            let now = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
            let mut idx = self.index.write().unwrap();
            if !idx.is_live(id) {
                continue;
            }
            match now {
                None => idx.remove(id),
                Some(t) if t != saved => {
                    index::list_into(&mut idx, id, &path, rules);
                }
                _ => continue,
            }
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    fn watch_roots(&self, cfg: &Config) {
        let weak = self.me.clone();
        let Ok(watch) = Watch::start(move |changes| {
            if let Some(me) = weak.upgrade() {
                me.apply(changes);
            }
        }) else {
            return;
        };
        for r in &cfg.roots {
            watch.watch_tree(r);
        }
        *self.watch.lock().unwrap() = Some(watch);
    }

    /// File system events: add, update or remove what changed.
    fn apply(&self, changes: Changes) {
        if self.paused.load(Ordering::Relaxed) {
            return;
        }
        let Some(cfg) = self.config.lock().unwrap().clone() else { return };
        let rules = cfg.rules();
        if changes.rescan {
            let generation = self.generation.load(Ordering::SeqCst);
            self.reconcile(&rules, &|| self.cancelled(generation));
            return;
        }
        let mut idx = self.index.write().unwrap();
        let mut changed = false;
        for p in &changes.paths {
            let meta = std::fs::symlink_metadata(p).ok();
            match (idx.find(p), meta) {
                (Some(id), None) => {
                    idx.remove(id);
                    changed = true;
                }
                (Some(id), Some(m)) => {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
                    let meta = index::Meta { kind: if m.is_dir() { KIND_FOLDER } else { index::KIND_FILE }, hidden: name.starts_with('.'), size: if m.is_dir() { 0 } else { m.len() }, mtime };
                    // Its entries are kept current by these same events, so its time can follow.
                    idx.update(id, meta);
                    changed = true;
                }
                (None, Some(m)) => {
                    // New: only inside a folder that's indexed (so nothing under excluded or
                    // ignored folders), and not left out by the rules itself.
                    let Some(parent) = p.parent().and_then(|d| idx.find(d)) else { continue };
                    if m.file_type().is_symlink() || rules.skip(p, m.is_dir()) {
                        continue;
                    }
                    if m.is_dir() {
                        index::walk_into(&mut idx, parent, p, &rules);
                    } else {
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
                        idx.push(parent, &name, index::Meta { kind: index::KIND_FILE, hidden: name.starts_with('.'), size: m.len(), mtime });
                    }
                    changed = true;
                }
                (None, None) => {}
            }
        }
        if changed {
            if idx.wants_compact() {
                idx.compact();
            }
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    pub fn save_if_dirty(&self) {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return;
        }
        let Some(f) = file() else { return };
        let idx = self.index.read().unwrap();
        if let Err(e) = idx.save(&f) {
            self.status.lock().unwrap().error = Some(format!("Couldn't save the index: {e}"));
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        if !paused {
            // Catch up on what changed meanwhile.
            self.restart(false);
        } else {
            self.set_phase("paused");
        }
    }

    /// Search the index, or (`scope` outside it) that folder walked on the spot.
    pub fn search(&self, query: &str, scope: Option<&Path>, limit: usize) -> Result<SearchResult, String> {
        let q = Query::parse(query)?;
        let now = now_ms();
        if let Some(scope) = scope {
            let idx = self.index.read().unwrap();
            if let Some(id) = idx.find(scope).filter(|&id| idx.kind(id) == KIND_FOLDER && idx.flags(id) & FLAG_UNLISTED == 0) {
                return Ok(index::search(&idx, &q, Some(id), limit, now));
            }
            drop(idx);
            // Not indexed (excluded, ignored, or outside the roots): walk it, without the
            // .gitignore rule (you're looking right at it), and keep it for the next keystrokes.
            let scratch = {
                let mut s = self.scratch.lock().unwrap();
                match &*s {
                    Some((p, at, idx)) if p == scope && at.elapsed() < Duration::from_secs(30) => idx.clone(),
                    _ => {
                        let cfg = self.config.lock().unwrap().clone();
                        let rules = cfg.map(|c| Rules::new(&c.rules_off, &c.excludes, &c.includes, c.hidden, false)).unwrap_or_else(|| Rules::new(&[], &[], &[], false, false));
                        let started = Instant::now();
                        let idx = Arc::new(index::build(&[scope.to_path_buf()], &rules, &|| started.elapsed() > Duration::from_secs(5), &mut |_| {}));
                        *s = Some((scope.to_path_buf(), Instant::now(), idx.clone()));
                        idx
                    }
                }
            };
            return Ok(index::search(&scratch, &q, None, limit, now));
        }
        let idx = self.index.read().unwrap();
        Ok(index::search(&idx, &q, None, limit, now))
    }
}

// ---------- commands ----------

#[tauri::command]
pub async fn index_search(indexer: tauri::State<'_, Arc<Indexer>>, query: String, scope: Option<String>, limit: Option<usize>) -> Result<SearchResult, String> {
    let me = indexer.inner().clone();
    tauri::async_runtime::spawn_blocking(move || me.search(&query, scope.as_deref().map(Path::new), limit.unwrap_or(200).min(2000))).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn index_status(indexer: tauri::State<'_, Arc<Indexer>>) -> Status {
    indexer.status()
}

#[tauri::command]
pub fn index_rebuild(indexer: tauri::State<'_, Arc<Indexer>>) {
    indexer.restart(true);
}

#[tauri::command]
pub fn index_pause(indexer: tauri::State<'_, Arc<Indexer>>, paused: bool) {
    indexer.set_paused(paused);
}

#[tauri::command]
pub fn index_rules() -> Vec<index::RuleInfo> {
    index::all_rules()
}
