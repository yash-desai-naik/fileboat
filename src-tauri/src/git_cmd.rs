//! Git commands for the UI: the repository a folder is in (branch, ahead/behind, state, counts).

use crate::folders::Folders;
use fileboat_core::git;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub root: String,
    /// The repository's folder name.
    pub name: String,
    pub branch: Option<String>,
    /// The commit, when HEAD is detached.
    pub detached: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// "merge", "rebase", "cherry-pick", "revert" or "bisect".
    pub state: Option<&'static str>,
    pub staged: usize,
    pub changed: usize,
    pub untracked: usize,
    pub conflicted: usize,
    /// False until the first status has been read.
    pub ready: bool,
    /// Why status couldn't be read (git missing…).
    pub error: Option<String>,
}

/// The repository `path` is in, with what its last status said; None outside one.
#[tauri::command]
pub fn git_info(folders: State<'_, Folders>, path: String) -> Option<RepoInfo> {
    let repo = git::discover(&PathBuf::from(&path))?;
    let state = folders.repos().get(&repo.root);
    let snap = state.as_ref().and_then(|s| s.snapshot.lock().unwrap().clone());
    let error = state.as_ref().and_then(|s| s.error.lock().unwrap().clone());
    let head = snap.as_ref().map(|s| s.head.clone()).unwrap_or_else(|| git::head(&repo));
    let mut info = RepoInfo {
        root: repo.root.display().to_string(),
        name: repo.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        branch: head.branch,
        detached: head.detached,
        upstream: None,
        ahead: 0,
        behind: 0,
        state: git::in_progress(&repo),
        staged: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        ready: snap.is_some(),
        error,
    };
    if let Some(snap) = snap {
        info.upstream = snap.status.branch.upstream.clone();
        info.ahead = snap.status.branch.ahead;
        info.behind = snap.status.branch.behind;
        info.state = snap.state;
        for f in &snap.status.files {
            if f.conflicted {
                info.conflicted += 1;
            } else if f.untracked {
                info.untracked += 1;
            } else if !f.ignored {
                if f.index != '.' {
                    info.staged += 1;
                }
                if f.worktree != '.' {
                    info.changed += 1;
                }
            }
        }
    }
    Some(info)
}
