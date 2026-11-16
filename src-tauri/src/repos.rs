//! Git status, live, for the repositories open folders are in. Each repository has one status
//! worker and one recursive watch on its working tree: a change anywhere in it (or to `.git/HEAD`,
//! the index or refs) runs `git status` again. Bursts are coalesced, a newer request waits for
//! the running one, a busy repository rests between runs, and changes inside folders git ignores
//! (a build writing to `target/`) don't count. Results go into the open folders' models, and the
//! UI hears `git-changed` for the branch and the git panel.

use crate::folders::Folders;
use crate::sizes::path_key;
use crate::watch::{Changes, Watch};
use fileboat_core::git::{self, Head, Mark, Repo, Status, StatusIndex};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use tauri::Emitter;

/// What the last status run found.
pub struct Snapshot {
    pub status: Status,
    pub index: StatusIndex,
    pub head: Head,
    pub state: Option<&'static str>,
}

#[derive(Default)]
struct Run {
    running: bool,
    again: bool,
    /// Not before then (a rest after a slow run).
    next: Option<Instant>,
}

pub struct RepoState {
    pub repo: Repo,
    users: Mutex<usize>,
    alive: AtomicBool,
    run: Mutex<Run>,
    pub snapshot: Mutex<Option<Arc<Snapshot>>>,
    /// Why status couldn't be read (git missing…), if so.
    pub error: Mutex<Option<String>>,
}

/// Changes coming close together run status once.
const SETTLE: Duration = Duration::from_millis(150);

/// The rest after a run: twice its length, 300 ms to 10 s.
fn rest_after(took: Duration) -> Duration {
    (took * 2).clamp(Duration::from_millis(300), Duration::from_secs(10))
}

/// Files in `.git` whose change means the status, branch or state may have changed.
fn interesting_in_git_dir(rel: &str) -> bool {
    if rel.ends_with(".lock") {
        return false;
    }
    matches!(rel, "HEAD" | "index" | "packed-refs" | "MERGE_HEAD" | "REBASE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD" | "BISECT_LOG" | "FETCH_HEAD")
        || rel.starts_with("refs/")
        || rel.starts_with("rebase-merge")
        || rel.starts_with("rebase-apply")
}

pub struct Repos {
    open: Mutex<HashMap<String, Arc<RepoState>>>,
    watch: Option<Watch>,
    folders: Folders,
}

impl Repos {
    pub fn start(folders: Folders) -> Arc<Repos> {
        Arc::new_cyclic(|me: &Weak<Repos>| {
            let me = me.clone();
            let watch = Watch::start(move |changes| {
                if let Some(repos) = me.upgrade() {
                    repos.on_changes(changes);
                }
            })
            .ok();
            Repos { open: Mutex::new(HashMap::new()), watch, folders }
        })
    }

    /// A folder at `path` opened: its repository (if any), watched and with status on the way.
    pub fn attach(self: &Arc<Self>, path: &Path) -> Option<Arc<RepoState>> {
        let repo = git::discover(path)?;
        // Inside `.git` itself there's no status to show.
        if path_key(path).starts_with(&format!("{}{}", path_key(&repo.git_dir), std::path::MAIN_SEPARATOR)) || path_key(path) == path_key(&repo.git_dir) {
            return None;
        }
        let key = path_key(&repo.root);
        let mut open = self.open.lock().unwrap();
        let state = open
            .entry(key)
            .or_insert_with(|| {
                if let Some(w) = &self.watch {
                    w.watch_tree(&repo.root);
                }
                let s = Arc::new(RepoState {
                    repo,
                    users: Mutex::new(0),
                    alive: AtomicBool::new(true),
                    run: Mutex::default(),
                    snapshot: Mutex::new(None),
                    error: Mutex::new(None),
                });
                self.trigger(&s);
                s
            })
            .clone();
        *state.users.lock().unwrap() += 1;
        Some(state)
    }

    /// A folder in `state`'s repository closed: the last one stops its watch and status.
    pub fn detach(&self, state: &Arc<RepoState>) {
        let mut users = state.users.lock().unwrap();
        *users = users.saturating_sub(1);
        if *users > 0 {
            return;
        }
        state.alive.store(false, Ordering::Relaxed);
        self.open.lock().unwrap().remove(&path_key(&state.repo.root));
        if let Some(w) = &self.watch {
            w.unwatch(&state.repo.root);
        }
    }

    /// The open repository at `root`, if any folder in it is open.
    pub fn get(&self, root: &Path) -> Option<Arc<RepoState>> {
        self.open.lock().unwrap().get(&path_key(root)).cloned()
    }

    /// Run status again (now, or once the running one and the rest after it are over).
    pub fn trigger(self: &Arc<Self>, state: &Arc<RepoState>) {
        {
            let mut run = state.run.lock().unwrap();
            if run.running {
                run.again = true;
                return;
            }
            run.running = true;
        }
        let (me, state) = (self.clone(), state.clone());
        std::thread::spawn(move || me.status_loop(&state));
    }

    fn status_loop(&self, state: &Arc<RepoState>) {
        loop {
            let wait = state.run.lock().unwrap().next.map_or(SETTLE, |n| n.saturating_duration_since(Instant::now()).max(SETTLE));
            std::thread::sleep(wait);
            if !state.alive.load(Ordering::Relaxed) {
                break;
            }
            let started = Instant::now();
            match git::status(&state.repo, true, || !state.alive.load(Ordering::Relaxed)) {
                Ok(status) => {
                    let snap = Snapshot {
                        index: StatusIndex::new(&status),
                        head: git::head(&state.repo),
                        state: git::in_progress(&state.repo),
                        status,
                    };
                    *state.snapshot.lock().unwrap() = Some(Arc::new(snap));
                    *state.error.lock().unwrap() = None;
                }
                Err(e) if e == "Cancelled." => break,
                Err(e) => *state.error.lock().unwrap() = Some(e),
            }
            self.folders.git_changed(state);
            if let Some(app) = self.folders.app() {
                let _ = app.emit("git-changed", state.repo.root.display().to_string());
            }
            let mut run = state.run.lock().unwrap();
            run.next = Some(Instant::now() + rest_after(started.elapsed()));
            if run.again {
                run.again = false;
                continue;
            }
            run.running = false;
            break;
        }
        // Cancelled: let a later attach start afresh.
        state.run.lock().unwrap().running = false;
    }

    fn on_changes(self: &Arc<Self>, changes: Changes) {
        let open: Vec<Arc<RepoState>> = self.open.lock().unwrap().values().cloned().collect();
        for state in open {
            if changes.rescan || changes.paths.iter().any(|p| matters(&state, p)) {
                self.trigger(&state);
            }
        }
    }
}

/// Whether a change at `path` can change `state`'s status: anything in the working tree that
/// git doesn't ignore, and the files in `.git` that hold HEAD, the index and refs.
fn matters(state: &RepoState, path: &Path) -> bool {
    let Some(rel) = git::relative(&state.repo, path) else { return false };
    if let Some(inside) = rel.strip_prefix(".git/") {
        return interesting_in_git_dir(inside);
    }
    if rel == ".git" || rel.is_empty() {
        return false;
    }
    let snap = state.snapshot.lock().unwrap().clone();
    // Writing build output into an ignored folder changes nothing git shows.
    !snap.is_some_and(|s| s.index.mark(&rel, false) == Some((Mark::Ignored, false)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_git_files_matter() {
        assert!(interesting_in_git_dir("HEAD"));
        assert!(interesting_in_git_dir("index"));
        assert!(interesting_in_git_dir("refs/heads/main"));
        assert!(interesting_in_git_dir("rebase-merge/done"));
        assert!(!interesting_in_git_dir("index.lock"));
        assert!(!interesting_in_git_dir("objects/ab/cdef"));
        assert!(!interesting_in_git_dir("logs/HEAD"));
        assert_eq!(rest_after(Duration::from_millis(20)), Duration::from_millis(300));
        assert_eq!(rest_after(Duration::from_secs(2)), Duration::from_secs(4));
    }
}
