//! Folder sizes, worked out in the background for open folders that want them (the "Show folder
//! sizes" setting, or "Calculate Folder Sizes" on one folder). Two threads walk folders; results
//! are cached (sizes of folders seen recently) and handed to the open folders in batches, so a
//! folder with hundreds of subfolders is re-sorted a few times, not once per subfolder.
//! File system events and F5 forget cached sizes that may have changed; a folder that keeps
//! changing (a build writing to `target`, `npm install`) is walked again only after a rest.

use crate::folders::Folders;
use fileboat_core::fs::{tree_size, TreeSize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const WORKERS: usize = 2;
/// Folder sizes kept; the least recently used go first.
const CACHE_LIMIT: usize = 20_000;
/// Results arriving within this long reach the views together.
const BATCH: Duration = Duration::from_millis(150);

/// How long before a folder just walked is walked again: ten times the walk, 3 s to a minute.
fn rest_after(walk: Duration) -> Duration {
    (walk * 10).clamp(Duration::from_secs(3), Duration::from_secs(60))
}

/// A path as compared: without case on Windows and macOS (their usual file systems ignore it),
/// `/` as `\` on Windows, and without trailing separators.
pub fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy();
    let s = if cfg!(windows) { s.replace('/', "\\") } else { s.into_owned() };
    let s = s.trim_end_matches(['\\', '/']);
    if cfg!(any(windows, target_os = "macos")) { s.to_lowercase() } else { s.to_string() }
}

use path_key as key;

#[derive(Default)]
struct Cache {
    map: HashMap<String, (TreeSize, u64)>,
    tick: u64,
}

impl Cache {
    fn get(&mut self, k: &str) -> Option<TreeSize> {
        self.tick += 1;
        let tick = self.tick;
        self.map.get_mut(k).map(|(size, used)| {
            *used = tick;
            *size
        })
    }

    fn put(&mut self, k: String, size: TreeSize) {
        self.tick += 1;
        self.map.insert(k, (size, self.tick));
        if self.map.len() > CACHE_LIMIT {
            // Drop the least recently used tenth in one go.
            let mut ticks: Vec<u64> = self.map.values().map(|(_, t)| *t).collect();
            let cut = CACHE_LIMIT / 10;
            ticks.select_nth_unstable(cut);
            let below = ticks[cut];
            self.map.retain(|_, (_, t)| *t > below);
        }
    }
}

#[derive(Default)]
struct Queue {
    /// Folders to walk, each with when it may start.
    waiting: VecDeque<(PathBuf, Instant)>,
    /// Waiting or being walked (so a folder is walked once however many times it's asked for).
    busy: HashSet<String>,
    /// Folders walked lately: not again before then.
    resting: HashMap<String, Instant>,
}

pub struct Sizer {
    cache: Mutex<Cache>,
    queue: Mutex<Queue>,
    ready: Condvar,
}

impl Sizer {
    /// Start the walkers and the thread handing results to `folders`.
    pub fn start(folders: Folders) -> Arc<Sizer> {
        let sizer = Arc::new(Sizer { cache: Mutex::default(), queue: Mutex::default(), ready: Condvar::new() });
        let (done_tx, done_rx) = mpsc::channel::<(PathBuf, TreeSize)>();
        for _ in 0..WORKERS {
            let (sizer, folders, done) = (sizer.clone(), folders.clone(), done_tx.clone());
            std::thread::spawn(move || sizer.work(&folders, &done));
        }
        std::thread::spawn(move || {
            while let Ok(first) = done_rx.recv() {
                std::thread::sleep(BATCH);
                let mut batch = vec![first];
                batch.extend(done_rx.try_iter());
                folders.apply_sizes(&batch);
            }
        });
        sizer
    }

    fn work(&self, folders: &Folders, done: &mpsc::Sender<(PathBuf, TreeSize)>) {
        loop {
            let path = {
                let mut q = self.queue.lock().unwrap();
                loop {
                    let now = Instant::now();
                    if let Some(i) = q.waiting.iter().position(|(_, at)| *at <= now) {
                        break q.waiting.remove(i).expect("a waiting folder").0;
                    }
                    q = match q.waiting.iter().map(|(_, at)| *at).min() {
                        Some(at) => self.ready.wait_timeout(q, at - now).unwrap().0,
                        None => self.ready.wait(q).unwrap(),
                    };
                }
            };
            // Only while a folder showing it still wants sizes (it may have closed meanwhile).
            let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let started = Instant::now();
            let size = if folders.wants_sizes(&parent) { tree_size(&path, || !folders.wants_sizes(&parent)) } else { None };
            let k = key(&path);
            if let Some(size) = size {
                self.cache.lock().unwrap().put(k.clone(), size);
                let _ = done.send((path, size));
            }
            let mut q = self.queue.lock().unwrap();
            q.busy.remove(&k);
            let now = Instant::now();
            if q.resting.len() > 1000 {
                q.resting.retain(|_, until| *until > now);
            }
            q.resting.insert(k, now + rest_after(started.elapsed()));
        }
    }

    /// A cached size, if known.
    pub fn cached(&self, path: &Path) -> Option<TreeSize> {
        self.cache.lock().unwrap().get(&key(path))
    }

    /// Work these out (those not already waiting or being walked).
    pub fn queue(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let mut q = self.queue.lock().unwrap();
        let now = Instant::now();
        for p in paths {
            let k = key(&p);
            let at = q.resting.get(&k).copied().filter(|until| *until > now).unwrap_or(now);
            if q.busy.insert(k) {
                q.waiting.push_back((p, at));
            }
        }
        self.ready.notify_all();
    }

    /// Something at `path` changed: its size and every folder's above it are out of date.
    pub fn forget_up(&self, path: &Path) {
        let mut cache = self.cache.lock().unwrap();
        for p in path.ancestors() {
            cache.map.remove(&key(p));
        }
    }

    /// Forget every size under `path` (F5: sizes are worked out again).
    pub fn forget_under(&self, path: &Path) {
        let base = key(path);
        let mut cache = self.cache.lock().unwrap();
        cache.map.retain(|k, _| !(k.len() > base.len() && k.starts_with(&base) && k[base.len()..].starts_with(['\\', '/'])));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgetting_up_and_under() {
        let sizer = Sizer { cache: Mutex::default(), queue: Mutex::default(), ready: Condvar::new() };
        let size = TreeSize { bytes: 1, ..TreeSize::default() };
        let root = std::env::temp_dir().join("fb-sizes");
        for p in ["a", "a/b", "a/b/c", "a2", "x"] {
            sizer.cache.lock().unwrap().put(key(&root.join(p)), size);
        }
        sizer.forget_up(&root.join("a/b/c/file.txt"));
        assert!(sizer.cached(&root.join("a")).is_none() && sizer.cached(&root.join("a/b/c")).is_none());
        assert!(sizer.cached(&root.join("x")).is_some());

        sizer.cache.lock().unwrap().put(key(&root.join("a/b")), size);
        sizer.forget_under(&root.join("a"));
        assert!(sizer.cached(&root.join("a/b")).is_none());
        assert!(sizer.cached(&root.join("a2")).is_some(), "a sibling whose name starts the same stays");
    }

    #[cfg(windows)]
    #[test]
    fn keys_ignore_case_slashes_and_trailing_separators() {
        assert_eq!(path_key(Path::new("C:/Users/Me/")), path_key(Path::new(r"c:\users\me")));
        assert_eq!(path_key(Path::new(r"C:\")), "c:");
    }

    #[test]
    fn busy_folders_rest_longer() {
        assert_eq!(rest_after(Duration::from_millis(5)), Duration::from_secs(3));
        assert_eq!(rest_after(Duration::from_secs(2)), Duration::from_secs(20));
        assert_eq!(rest_after(Duration::from_secs(30)), Duration::from_secs(60));
    }

    #[test]
    fn the_cache_drops_the_least_recently_used() {
        let mut cache = Cache::default();
        let size = TreeSize::default();
        for i in 0..CACHE_LIMIT {
            cache.put(format!("k{i}"), size);
        }
        assert!(cache.get("k0").is_some()); // used: kept
        cache.put("new".into(), size);
        assert!(cache.map.len() <= CACHE_LIMIT);
        assert!(cache.get("k0").is_some() && cache.get("new").is_some());
        assert!(cache.get("k1").is_none());
    }
}
