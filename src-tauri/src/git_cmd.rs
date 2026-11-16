//! Git commands for the UI: the repository a folder is in (branch, ahead/behind, state, counts),
//! the panel's files, and the writes (stage, commit, branches, stash, fetch / pull / push), which
//! run off the main thread and refresh the status when done.

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
    /// Seconds the last status took, when so slow that it only runs when asked.
    pub paused: Option<u32>,
}

/// The repository `path` is in, with what its last status said; None outside one.
#[tauri::command]
pub fn git_info(folders: State<'_, Folders>, path: String) -> Option<RepoInfo> {
    let repo = git::discover(&PathBuf::from(&path))?;
    let state = folders.repos().get(&repo.root);
    let snap = state.as_ref().and_then(|s| s.snapshot.lock().unwrap().clone());
    let error = state.as_ref().and_then(|s| s.error.lock().unwrap().clone());
    let paused = state.as_ref().and_then(|s| *s.paused.lock().unwrap());
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
        paused,
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

/// The repository at (or above) `path`.
fn repo_of(path: &str) -> Result<git::Repo, String> {
    git::discover(&PathBuf::from(path)).ok_or_else(|| "That isn't inside a git repository.".to_string())
}

/// Paths as git wants them: relative to the repository root.
fn rels(repo: &git::Repo, paths: &[String]) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|p| git::relative(repo, &PathBuf::from(p)).filter(|r| !r.is_empty()).ok_or_else(|| format!("{p} isn't in this repository.")))
        .collect()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Run status now (a paused repository's status bar asks for it).
#[tauri::command]
pub fn git_status_run(folders: State<'_, Folders>, path: String) {
    if let Some(repo) = git::discover(&PathBuf::from(&path)) {
        refresh(&folders, &repo);
    }
}

/// After a write: status again now (the watcher would notice too, a moment later).
fn refresh(folders: &Folders, repo: &git::Repo) {
    if let Some(state) = folders.repos().get(&repo.root) {
        folders.repos().trigger(&state);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub root: String,
    /// Changed, staged, untracked and conflicted files (not ignored ones).
    pub files: Vec<git::FileStatus>,
}

/// The files the git panel lists, from the last status.
#[tauri::command]
pub fn git_status(folders: State<'_, Folders>, path: String) -> Option<RepoStatus> {
    let repo = git::discover(&PathBuf::from(&path))?;
    let snap = folders.repos().get(&repo.root)?.snapshot.lock().unwrap().clone()?;
    Some(RepoStatus { root: repo.root.display().to_string(), files: snap.status.files.iter().filter(|f| !f.ignored).cloned().collect() })
}

/// stage, unstage or discard (tracked files' unstaged changes) these paths.
#[tauri::command]
pub async fn git_paths(folders: State<'_, Folders>, op: String, paths: Vec<String>) -> Result<(), String> {
    let first = paths.first().ok_or("Nothing selected.")?.clone();
    let repo = repo_of(&first)?;
    let rel = rels(&repo, &paths)?;
    let r = repo.clone();
    blocking(move || match op.as_str() {
        "stage" => git::stage(&r, &rel),
        "unstage" => git::unstage(&r, &rel),
        "discard" => git::discard(&r, &rel),
        _ => Err("Unknown git action.".into()),
    })
    .await?;
    refresh(&folders, &repo);
    Ok(())
}

/// Add these files and folders to the repository's .gitignore.
#[tauri::command]
pub async fn git_ignore(folders: State<'_, Folders>, paths: Vec<String>) -> Result<usize, String> {
    let first = paths.first().ok_or("Nothing selected.")?.clone();
    let repo = repo_of(&first)?;
    let rel = rels(&repo, &paths)?;
    let dirs: Vec<(String, bool)> = rel.into_iter().zip(paths.iter()).map(|(r, p)| (r, std::path::Path::new(p).is_dir())).collect();
    let r = repo.clone();
    let n = blocking(move || git::ignore(&r, &dirs)).await?;
    refresh(&folders, &repo);
    Ok(n)
}

#[tauri::command]
pub async fn git_commit(folders: State<'_, Folders>, root: String, message: String, amend: bool) -> Result<String, String> {
    let repo = repo_of(&root)?;
    let r = repo.clone();
    let out = blocking(move || git::commit(&r, &message, amend)).await?;
    refresh(&folders, &repo);
    Ok(out)
}

#[tauri::command]
pub async fn git_branches(root: String) -> Result<Vec<git::BranchInfo>, String> {
    let repo = repo_of(&root)?;
    blocking(move || git::branches(&repo)).await
}

/// switch to a branch, create one (and switch), stash, or pop the newest stash.
#[tauri::command]
pub async fn git_branch_op(folders: State<'_, Folders>, root: String, op: String, name: String) -> Result<(), String> {
    let repo = repo_of(&root)?;
    let r = repo.clone();
    blocking(move || match op.as_str() {
        "switch" => git::switch(&r, &name),
        "create" => git::create_branch(&r, &name),
        "stash" => git::stash(&r, &name),
        "pop" => git::stash_pop(&r),
        _ => Err("Unknown git action.".into()),
    })
    .await?;
    refresh(&folders, &repo);
    Ok(())
}

#[tauri::command]
pub async fn git_log(root: String, count: usize) -> Result<Vec<git::Commit>, String> {
    let repo = repo_of(&root)?;
    blocking(move || git::log(&repo, count.min(500))).await
}

/// Repositories with a fetch, pull or push running (one at a time each).
#[derive(Default)]
pub struct Syncing(std::sync::Mutex<std::collections::HashSet<String>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    root: String,
    op: String,
    line: String,
}

/// fetch, pull or push; progress arrives as `git-progress` events; git's closing message comes back.
#[tauri::command]
pub async fn git_sync(app: tauri::AppHandle, folders: State<'_, Folders>, syncing: State<'_, Syncing>, root: String, op: String) -> Result<String, String> {
    use tauri::Emitter;
    let repo = repo_of(&root)?;
    let which = match op.as_str() {
        "fetch" => git::Sync::Fetch,
        "pull" => git::Sync::Pull,
        "push" => git::Sync::Push,
        _ => return Err("Unknown git action.".into()),
    };
    let key = crate::sizes::path_key(&repo.root);
    if !syncing.0.lock().unwrap().insert(key.clone()) {
        return Err("A fetch, pull or push is already running in this repository.".into());
    }
    let r = repo.clone();
    let root_s = repo.root.display().to_string();
    let result = blocking(move || {
        let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
        git::sync(
            &r,
            which,
            |line| {
                // At most ten updates a second.
                if last.elapsed() >= std::time::Duration::from_millis(100) {
                    last = std::time::Instant::now();
                    let _ = app.emit("git-progress", Progress { root: root_s.clone(), op: op.clone(), line: line.to_string() });
                }
            },
            || false,
        )
    })
    .await;
    syncing.0.lock().unwrap().remove(&key);
    refresh(&folders, &repo);
    result
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffTexts {
    /// The file before (HEAD, or the staged version); None for a new file.
    pub before: Option<String>,
    /// The file after (on disk, or staged); None when deleted.
    pub after: Option<String>,
}

/// The two sides of a file's diff: "worktree" (staged or committed → on disk) or "staged"
/// (committed → staged).
#[tauri::command]
pub async fn git_diff(path: String, mode: String) -> Result<DiffTexts, String> {
    let repo = repo_of(&path)?;
    let rel = git::relative(&repo, &PathBuf::from(&path)).ok_or("That file isn't in the repository.")?;
    blocking(move || {
        let text = |b: Option<Vec<u8>>| b.map(|b| String::from_utf8_lossy(b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&b)).into_owned());
        if mode == "staged" {
            Ok(DiffTexts { before: text(git::show(&repo, "HEAD", &rel)?), after: text(git::show(&repo, "", &rel)?) })
        } else {
            // Against what's staged, which is HEAD unless something was staged.
            let before = match git::show(&repo, "", &rel)? {
                Some(b) => Some(b),
                None => git::show(&repo, "HEAD", &rel)?,
            };
            let after = std::fs::read(&path).ok();
            Ok(DiffTexts { before: text(before), after: text(after) })
        }
    })
    .await
}

/// A web page for `path` on GitHub, GitLab or Bitbucket: at the branch, or (permalink) at the commit.
#[tauri::command]
pub async fn git_web_url(path: String, permalink: bool) -> Result<String, String> {
    let repo = repo_of(&path)?;
    blocking(move || {
        let remote = git::remote_url(&repo).ok_or("This repository has no remote.")?;
        let base = web_base(&remote).ok_or("The remote isn't a web host Fileboat knows.")?;
        let rel = git::relative(&repo, &PathBuf::from(&path)).unwrap_or_default();
        let at = if permalink {
            git::log(&repo, 1)?.first().map(|c| c.hash.clone()).ok_or("There are no commits yet.")?
        } else {
            git::head(&repo).branch.ok_or("HEAD isn't on a branch.")?
        };
        let kind = if std::path::Path::new(&path).is_dir() { "tree" } else { "blob" };
        let sep = if base.contains("gitlab") { "/-/" } else { "/" };
        let page = if base.contains("bitbucket.org") { format!("{base}/src/{at}/{rel}") } else { format!("{base}{sep}{kind}/{at}/{rel}") };
        Ok(page.trim_end_matches('/').to_string())
    })
    .await
}

/// "git@github.com:me/app.git" or "https://github.com/me/app.git" → "https://github.com/me/app".
fn web_base(remote: &str) -> Option<String> {
    let r = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    let (host, path) = if let Some(rest) = r.strip_prefix("git@") {
        rest.split_once(':')?
    } else if let Some(rest) = r.strip_prefix("ssh://") {
        let rest = rest.split_once('@').map_or(rest, |(_, h)| h);
        let (host, path) = rest.split_once('/')?;
        (host.split(':').next()?, path)
    } else {
        let rest = r.strip_prefix("https://").or_else(|| r.strip_prefix("http://"))?;
        let rest = rest.split_once('@').map_or(rest, |(_, h)| h);
        rest.split_once('/')?
    };
    Some(format!("https://{host}/{path}"))
}

#[cfg(test)]
mod tests {
    use super::web_base;

    #[test]
    fn remotes_to_web_pages() {
        assert_eq!(web_base("git@github.com:me/app.git").as_deref(), Some("https://github.com/me/app"));
        assert_eq!(web_base("https://github.com/me/app.git").as_deref(), Some("https://github.com/me/app"));
        assert_eq!(web_base("https://user@gitlab.com/g/sub/app").as_deref(), Some("https://gitlab.com/g/sub/app"));
        assert_eq!(web_base("ssh://git@host.example:2222/team/app.git").as_deref(), Some("https://host.example/team/app"));
        assert_eq!(web_base("/local/bare.git"), None);
    }
}
