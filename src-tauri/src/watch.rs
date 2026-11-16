//! One file system watcher for the whole app: open folders are watched (not their subfolders),
//! and changes arrive as events from the OS (ReadDirectoryChangesW, FSEvents). Nothing polls.
//!
//! Events are collected until the folder has been quiet for a moment (or for at most half a
//! second during a burst such as `npm install`), then handed over in one batch.

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Quiet time that ends a batch, and the longest a batch waits during a burst.
const QUIET: Duration = Duration::from_millis(80);
const MAX_WAIT: Duration = Duration::from_millis(500);

/// A batch of changes: the paths that changed (created, modified, renamed or removed), and
/// whether the OS lost events (its buffer overflowed), in which case every folder is re-read.
pub struct Changes {
    pub paths: Vec<PathBuf>,
    pub rescan: bool,
}

pub struct Watch {
    watcher: Mutex<RecommendedWatcher>,
    /// How many views watch each folder: one OS watch per folder however many tabs show it.
    counts: Mutex<HashMap<PathBuf, usize>>,
}

impl Watch {
    /// Start watching; `on_changes` runs on the watcher's own thread for each batch.
    pub fn start(on_changes: impl Fn(Changes) + Send + 'static) -> Result<Watch, String> {
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let watcher = notify::recommended_watcher(tx).map_err(|e| format!("Couldn't watch for changes: {e}"))?;
        std::thread::Builder::new()
            .name("fileboat-watch".into())
            .spawn(move || batches(rx, on_changes))
            .map_err(|e| e.to_string())?;
        Ok(Watch { watcher: Mutex::new(watcher), counts: Mutex::new(HashMap::new()) })
    }

    pub fn watch(&self, path: &Path) {
        let mut counts = self.counts.lock().unwrap();
        let n = counts.entry(path.to_path_buf()).or_insert(0);
        if *n == 0 {
            // A folder that can't be watched (some network shares) still opens; it just isn't live.
            let _ = self.watcher.lock().unwrap().watch(path, RecursiveMode::NonRecursive);
        }
        *n += 1;
    }

    pub fn unwatch(&self, path: &Path) {
        let mut counts = self.counts.lock().unwrap();
        if let Some(n) = counts.get_mut(path) {
            *n -= 1;
            if *n == 0 {
                counts.remove(path);
                let _ = self.watcher.lock().unwrap().unwatch(path);
            }
        }
    }
}

fn batches(rx: mpsc::Receiver<notify::Result<notify::Event>>, on_changes: impl Fn(Changes)) {
    // Blocks until the first event of a batch: no work at all while nothing changes.
    while let Ok(first) = rx.recv() {
        let started = Instant::now();
        let mut changes = Changes { paths: Vec::new(), rescan: false };
        add(&mut changes, first);
        loop {
            let left = MAX_WAIT.saturating_sub(started.elapsed());
            match rx.recv_timeout(QUIET.min(left)) {
                Ok(event) => add(&mut changes, event),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            if started.elapsed() >= MAX_WAIT {
                break;
            }
        }
        changes.paths.sort();
        changes.paths.dedup();
        on_changes(changes);
    }
}

fn add(changes: &mut Changes, event: notify::Result<notify::Event>) {
    match event {
        Ok(e) => {
            if e.need_rescan() {
                changes.rescan = true;
            }
            // Access events (a file was read) change nothing a view shows.
            if !matches!(e.kind, EventKind::Access(_)) {
                changes.paths.extend(e.paths);
            }
        }
        // An error from the OS watcher (e.g. its buffer overflowed): re-read to be safe.
        Err(_) => changes.rescan = true,
    }
}
