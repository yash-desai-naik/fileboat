//! Git, read: where a folder's repository is, its branch and what it's in the middle of (merge,
//! rebase…), read from `.git` directly; and its status from the user's own `git`
//! (`status --porcelain=v2`), so fsmonitor, the untracked cache, `core.autocrlf`, sparse
//! checkouts, submodules and worktrees all behave exactly as in their terminal.
//!
//! Git runs without a console window and with `--no-optional-locks`: reading status never takes
//! the index lock away from a `git` command the user runs at the same moment.

mod status;

pub use status::{parse_status, Branch, FileStatus, Mark, Status, StatusIndex};

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// A repository's places on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// The working tree (what `git rev-parse --show-toplevel` says).
    pub root: PathBuf,
    /// Its `.git` folder (for a linked worktree, `.git/worktrees/<name>` in the main one).
    pub git_dir: PathBuf,
    /// Where refs and config live (the main `.git` for a linked worktree).
    pub common_dir: PathBuf,
}

/// The repository `path` is in, if any: the nearest folder up with a `.git` folder, or a `.git`
/// file pointing elsewhere (worktrees, submodules). Bare repositories aren't working trees.
pub fn discover(path: &Path) -> Option<Repo> {
    let mut here = Some(path);
    while let Some(dir) = here {
        let dot = dir.join(".git");
        if let Ok(meta) = std::fs::metadata(&dot) {
            let git_dir = if meta.is_dir() {
                dot
            } else {
                // "gitdir: <path>", relative to the folder holding the file.
                let text = std::fs::read_to_string(&dot).ok()?;
                let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
                let p = PathBuf::from(target);
                if p.is_absolute() { p } else { dir.join(p) }
            };
            if !git_dir.join("HEAD").is_file() {
                return None;
            }
            let common_dir = std::fs::read_to_string(git_dir.join("commondir"))
                .ok()
                .map(|c| {
                    let p = PathBuf::from(c.trim());
                    if p.is_absolute() { p } else { git_dir.join(p) }
                })
                .unwrap_or_else(|| git_dir.clone());
            return Some(Repo { root: dir.to_path_buf(), git_dir, common_dir });
        }
        here = dir.parent();
    }
    None
}

/// What HEAD points at: a branch, or a commit (detached).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Head {
    pub branch: Option<String>,
    /// The first 7 characters of the commit, when detached.
    pub detached: Option<String>,
}

pub fn head(repo: &Repo) -> Head {
    let text = std::fs::read_to_string(repo.git_dir.join("HEAD")).unwrap_or_default();
    let text = text.trim();
    match text.strip_prefix("ref: ") {
        Some(r) => Head { branch: Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_string()), detached: None },
        None => Head { branch: None, detached: Some(text.chars().take(7).collect()) },
    }
}

/// What the repository is in the middle of, if anything.
pub fn in_progress(repo: &Repo) -> Option<&'static str> {
    let g = &repo.git_dir;
    if g.join("rebase-merge").is_dir() || g.join("rebase-apply").is_dir() {
        Some("rebase")
    } else if g.join("MERGE_HEAD").is_file() {
        Some("merge")
    } else if g.join("CHERRY_PICK_HEAD").is_file() {
        Some("cherry-pick")
    } else if g.join("REVERT_HEAD").is_file() {
        Some("revert")
    } else if g.join("BISECT_LOG").is_file() {
        Some("bisect")
    } else {
        None
    }
}

/// `git` in `dir`, with no console window and no locks taken that a user's own `git` would wait on.
pub fn command(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir).arg("--no-optional-locks").stdin(Stdio::null());
    // Messages in English whatever the system language: Fileboat reads some of them.
    c.env("LC_ALL", "C").env("LANGUAGE", "C");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Run `cmd` to the end and return its output; `cancelled` is asked every 20 ms and kills it.
pub fn run(mut cmd: Command, mut cancelled: impl FnMut() -> bool) -> Result<Vec<u8>, String> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Git isn't installed (or not on PATH). Install it from https://git-scm.com to use git in Fileboat.".to_string()
        } else {
            format!("Couldn't run git: {e}")
        }
    })?;
    // Read both pipes on their own threads so a big output never blocks git.
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let reading = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let reading_err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err.read_to_end(&mut buf);
        buf
    });
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled.".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = reading.join().unwrap_or_default();
    let stderr = reading_err.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        let msg = String::from_utf8_lossy(&stderr).trim().to_string();
        Err(if msg.is_empty() { format!("git failed ({status}).") } else { msg })
    }
}

/// The repository's status: branch, upstream, ahead/behind, and every changed, untracked and
/// (with `ignored`) ignored path. Untracked and ignored folders come as one entry each.
pub fn status(repo: &Repo, ignored: bool, cancelled: impl FnMut() -> bool) -> Result<Status, String> {
    let mut cmd = command(&repo.root);
    cmd.args(["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=normal"]);
    if ignored {
        cmd.arg("--ignored=traditional");
    }
    let out = run(cmd, cancelled)?;
    Ok(parse_status(&out))
}

/// `path` relative to the repository root, with `/` (git's form): "" for the root itself, None
/// when outside it. Case doesn't matter on Windows and macOS.
pub fn relative(repo: &Repo, path: &Path) -> Option<String> {
    let p = path.to_string_lossy().replace('\\', "/");
    let r = repo.root.to_string_lossy().replace('\\', "/");
    let (p_cmp, r_cmp) = if cfg!(any(windows, target_os = "macos")) { (p.to_lowercase(), r.to_lowercase()) } else { (p.clone(), r.clone()) };
    let p_cmp = p_cmp.trim_end_matches('/');
    let r_cmp = r_cmp.trim_end_matches('/');
    if p_cmp == r_cmp {
        return Some(String::new());
    }
    let cut = r.trim_end_matches('/').len() + 1;
    (p_cmp.starts_with(r_cmp) && p_cmp[r_cmp.len()..].starts_with('/') && p.is_char_boundary(cut))
        .then(|| p[cut..].trim_end_matches('/').to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh repository with one commit, or None when git isn't installed.
    pub fn repo(name: &str) -> Option<PathBuf> {
        let dir = std::env::temp_dir().join(format!("fileboat-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            command(&dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"]).args(args).output()
        };
        if git(&["init", "-q"]).is_err() {
            return None;
        }
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        std::fs::create_dir_all(dir.join("src/deep")).unwrap();
        std::fs::write(dir.join("src/deep/lib.rs"), "fn x() {}\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();
        git(&["add", "."]).unwrap();
        git(&["commit", "-q", "-m", "first"]).unwrap();
        Some(dir)
    }

    #[test]
    fn discovers_heads_and_states() {
        let Some(dir) = repo("discover") else { return };
        let r = discover(&dir.join("src/deep")).unwrap();
        assert_eq!(r.root, dir);
        assert_eq!(head(&r), Head { branch: Some("main".into()), detached: None });
        assert_eq!(in_progress(&r), None);
        std::fs::write(r.git_dir.join("MERGE_HEAD"), "x").unwrap();
        assert_eq!(in_progress(&r), Some("merge"));
        assert_eq!(relative(&r, &dir.join("src").join("deep")).as_deref(), Some("src/deep"));
        assert_eq!(relative(&r, &dir).as_deref(), Some(""));
        assert!(discover(&std::env::temp_dir()).is_none_or(|o| o.root != dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn status_marks_files_and_folders() {
        let Some(dir) = repo("status") else { return };
        let r = discover(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "two\n").unwrap();
        std::fs::write(dir.join("src/deep/lib.rs"), "fn y() {}\n").unwrap();
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();
        std::fs::create_dir_all(dir.join("fresh/inner")).unwrap();
        std::fs::write(dir.join("fresh/inner/f"), "x").unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/out"), "x").unwrap();
        std::fs::write(dir.join("run.log"), "x").unwrap();
        command(&dir).args(["add", "new.txt"]).output().unwrap();

        let s = status(&r, true, || false).unwrap();
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        let idx = StatusIndex::new(&s);
        assert_eq!(idx.mark("a.txt", false), Some((Mark::Modified, false)));
        assert_eq!(idx.mark("new.txt", false), Some((Mark::Added, true)));
        assert_eq!(idx.mark("src", true), Some((Mark::Changed, false)));
        assert_eq!(idx.mark("src/deep", true), Some((Mark::Changed, false)));
        assert_eq!(idx.mark("fresh", true), Some((Mark::Untracked, false)));
        assert_eq!(idx.mark("fresh/inner/f", false), Some((Mark::Untracked, false)));
        assert_eq!(idx.mark("target", true), Some((Mark::Ignored, false)));
        assert_eq!(idx.mark("target/out", false), Some((Mark::Ignored, false)));
        assert_eq!(idx.mark("run.log", false), Some((Mark::Ignored, false)));
        assert_eq!(idx.mark(".gitignore", false), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
