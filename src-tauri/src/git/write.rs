//! Git, written: everything that changes a repository runs the user's own `git`, so hooks,
//! signing, credential helpers, LFS and config behave exactly as in their terminal. Each
//! operation is a fixed command; paths go after `--` and branch names are checked by git first.

use super::{command, run, Repo};
use serde::Serialize;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// `git` for a write: like `command`, plus no terminal prompts (a missing credential fails
/// with a message instead of waiting for input nobody can type).
fn write_command(repo: &Repo) -> Command {
    let mut c = command(&repo.root);
    c.env("GIT_TERMINAL_PROMPT", "0");
    c
}

fn text(out: Vec<u8>) -> String {
    String::from_utf8_lossy(&out).trim().to_string()
}

fn with_paths(mut c: Command, rels: &[String]) -> Command {
    c.arg("--");
    c.args(rels);
    c
}

/// Stage these paths (new, changed or deleted).
pub fn stage(repo: &Repo, rels: &[String]) -> Result<(), String> {
    let mut c = write_command(repo);
    c.args(["add", "-A"]);
    run(with_paths(c, rels), || false).map(drop)
}

/// Unstage these paths (keeping the changes in the files).
pub fn unstage(repo: &Repo, rels: &[String]) -> Result<(), String> {
    let mut c = write_command(repo);
    c.args(["restore", "--staged"]);
    match run(with_paths(c, rels), || false) {
        Ok(_) => Ok(()),
        // Before the first commit there's no HEAD to restore from.
        Err(_) => {
            let mut c = write_command(repo);
            c.args(["rm", "--cached", "-r", "-q"]);
            run(with_paths(c, rels), || false).map(drop)
        }
    }
}

/// Throw away the unstaged changes to these tracked files (what's staged stays).
pub fn discard(repo: &Repo, rels: &[String]) -> Result<(), String> {
    let mut c = write_command(repo);
    c.arg("restore");
    run(with_paths(c, rels), || false).map(drop)
}

/// Commit what's staged; the commit's first line comes back.
pub fn commit(repo: &Repo, message: &str, amend: bool) -> Result<String, String> {
    if message.trim().is_empty() && !amend {
        return Err("Write a commit message first.".into());
    }
    let mut c = write_command(repo);
    c.args(["commit", "-m", message]);
    if amend {
        c.arg("--amend");
    }
    run(c, || false).map(|out| text(out).lines().next().unwrap_or_default().to_string())
}

/// A branch name git accepts (`git check-ref-format --branch`), or why not.
pub fn check_branch_name(repo: &Repo, name: &str) -> Result<(), String> {
    if name.is_empty() || name.starts_with('-') {
        return Err("That isn't a valid branch name.".into());
    }
    let mut c = command(&repo.root);
    c.args(["check-ref-format", "--branch", name]);
    run(c, || false).map(drop).map_err(|_| format!("“{name}” isn't a valid branch name."))
}

/// Switch to a branch (a remote one gets a local branch tracking it, as `git switch` does).
pub fn switch(repo: &Repo, name: &str) -> Result<(), String> {
    check_branch_name(repo, name)?;
    let mut c = write_command(repo);
    c.args(["switch", name]);
    run(c, || false).map(drop)
}

/// Create a branch from HEAD and switch to it.
pub fn create_branch(repo: &Repo, name: &str) -> Result<(), String> {
    check_branch_name(repo, name)?;
    let mut c = write_command(repo);
    c.args(["switch", "-c", name]);
    run(c, || false).map(drop)
}

/// Stash every change, untracked files included.
pub fn stash(repo: &Repo, message: &str) -> Result<(), String> {
    let mut c = write_command(repo);
    c.args(["stash", "push", "-u"]);
    if !message.trim().is_empty() {
        c.args(["-m", message]);
    }
    run(c, || false).map(drop)
}

/// Apply the newest stash and drop it.
pub fn stash_pop(repo: &Repo) -> Result<(), String> {
    let mut c = write_command(repo);
    c.args(["stash", "pop"]);
    run(c, || false).map(drop)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub remote: bool,
    pub current: bool,
    pub upstream: Option<String>,
    /// Seconds since 1970 of its last commit.
    pub date: i64,
}

/// Local and remote branches, newest first.
pub fn branches(repo: &Repo) -> Result<Vec<BranchInfo>, String> {
    let mut c = command(&repo.root);
    c.args(["for-each-ref", "--sort=-committerdate", "--format=%(refname)%00%(HEAD)%00%(upstream:short)%00%(committerdate:unix)", "refs/heads", "refs/remotes"]);
    let out = text(run(c, || false)?);
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\0');
            let full = f.next()?;
            let current = f.next()? == "*";
            let upstream = f.next().filter(|u| !u.is_empty()).map(str::to_string);
            let date = f.next().and_then(|d| d.parse().ok()).unwrap_or(0);
            let (name, remote) = match full.strip_prefix("refs/heads/") {
                Some(n) => (n.to_string(), false),
                None => (full.strip_prefix("refs/remotes/")?.to_string(), true),
            };
            // "origin/HEAD" points at another remote branch.
            (!name.ends_with("/HEAD")).then_some(BranchInfo { name, remote, current, upstream, date })
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub author: String,
    /// Seconds since 1970.
    pub date: i64,
    pub subject: String,
}

/// The newest `n` commits on HEAD.
pub fn log(repo: &Repo, n: usize) -> Result<Vec<Commit>, String> {
    let mut c = command(&repo.root);
    c.args(["log", &format!("-n{n}"), "--format=%H%x00%h%x00%an%x00%at%x00%s"]);
    // An empty repository has no log.
    let out = run(c, || false).map(text).unwrap_or_default();
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\0');
            Some(Commit {
                hash: f.next()?.into(),
                short: f.next()?.into(),
                author: f.next()?.into(),
                date: f.next()?.parse().unwrap_or(0),
                subject: f.next().unwrap_or_default().into(),
            })
        })
        .collect())
}

/// A file as committed (`rev` = "HEAD") or as staged (`rev` = ""), for the diff; None when it
/// isn't there (a new file).
pub fn show(repo: &Repo, rev: &str, rel: &str) -> Result<Option<Vec<u8>>, String> {
    if rel.starts_with('-') {
        return Ok(None);
    }
    let mut c = command(&repo.root);
    c.arg("show").arg(format!("{rev}:{rel}"));
    match run(c, || false) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.contains("does not exist") || e.contains("exists on disk, but not in") || e.contains("invalid object name") || e.contains("bad revision") => Ok(None),
        Err(e) => Err(e),
    }
}

/// Add these paths (relative to the root) to the repository's top `.gitignore`, as patterns
/// anchored at the root ("/build/"), skipping ones already there. Returns how many were added.
pub fn ignore(repo: &Repo, rels: &[(String, bool)]) -> Result<usize, String> {
    let file = repo.root.join(".gitignore");
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    let have: std::collections::HashSet<&str> = existing.lines().map(str::trim).collect();
    let mut add = Vec::new();
    for (rel, dir) in rels {
        // Characters with a meaning in patterns are escaped.
        let escaped: String = rel.chars().flat_map(|c| if matches!(c, '*' | '?' | '[' | '!' | '#') { vec!['\\', c] } else { vec![c] }).collect();
        let pattern = format!("/{escaped}{}", if *dir { "/" } else { "" });
        if !have.contains(pattern.as_str()) && !add.contains(&pattern) {
            add.push(pattern);
        }
    }
    if add.is_empty() {
        return Ok(0);
    }
    let mut text = existing.clone();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for p in &add {
        text.push_str(p);
        text.push('\n');
    }
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(add.len())
}

/// `origin`, or the first remote; None without remotes.
fn remote_name(repo: &Repo) -> Option<String> {
    let mut c = command(&repo.root);
    c.arg("remote");
    let names = text(run(c, || false).ok()?);
    let names: Vec<&str> = names.lines().collect();
    if names.contains(&"origin") { Some("origin".into()) } else { names.first().map(|n| n.to_string()) }
}

/// The URL of `origin` (or the only remote).
pub fn remote_url(repo: &Repo) -> Option<String> {
    let remote = remote_name(repo)?;
    let mut c = command(&repo.root);
    c.args(["remote", "get-url", &remote]);
    run(c, || false).ok().map(text)
}

/// Fetch, pull or push.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sync {
    Fetch,
    Pull,
    Push,
}

/// Run a fetch, pull or push, calling `progress` with each line git reports ("Receiving objects:
/// 45% (450/1000)"); returns git's closing message. A branch without an upstream is pushed to
/// the remote and set to track it.
pub fn sync(repo: &Repo, op: Sync, mut progress: impl FnMut(&str), cancelled: impl Fn() -> bool) -> Result<String, String> {
    let mut c = write_command(repo);
    match op {
        Sync::Fetch => c.args(["fetch", "--prune", "--progress"]),
        Sync::Pull => c.args(["pull", "--progress"]),
        Sync::Push => {
            let tracked = {
                let mut q = command(&repo.root);
                q.args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]);
                run(q, || false).is_ok()
            };
            if tracked {
                c.args(["push", "--progress"])
            } else {
                let name = remote_name(repo).ok_or("This repository has no remote to push to.")?;
                c.args(["push", "--progress", "-u", &name, "HEAD"])
            }
        }
    };
    run_streaming(c, &mut progress, cancelled)
}

/// Run `cmd`, passing each progress line from stderr (split at `\r` and `\n`) to `progress`.
fn run_streaming(mut cmd: Command, progress: &mut impl FnMut(&str), cancelled: impl Fn() -> bool) -> Result<String, String> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null()).spawn().map_err(|e| format!("Couldn't run git: {e}"))?;
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let reading = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let (tx, rx) = mpsc::channel::<String>();
    let reading_err = std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        let mut line = Vec::new();
        while let Ok(n) = err.read(&mut chunk) {
            if n == 0 {
                break;
            }
            for &b in &chunk[..n] {
                if b == b'\r' || b == b'\n' {
                    if !line.is_empty() {
                        let _ = tx.send(String::from_utf8_lossy(&line).into_owned());
                        line.clear();
                    }
                } else {
                    line.push(b);
                }
            }
        }
        if !line.is_empty() {
            let _ = tx.send(String::from_utf8_lossy(&line).into_owned());
        }
    });
    let mut lines: Vec<String> = Vec::new();
    let status = loop {
        for l in rx.try_iter() {
            progress(&l);
            lines.push(l);
        }
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled.".into());
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let _ = reading_err.join();
    for l in rx.try_iter() {
        progress(&l);
        lines.push(l);
    }
    let stdout = text(reading.join().unwrap_or_default());
    // Progress lines ("Counting objects: 50%") aren't the message.
    let message: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|l| !l.contains('%') && !l.starts_with("remote: ") && !l.trim().is_empty())
        .collect();
    if status.success() {
        Ok(match stdout.lines().last() {
            Some(last) => last.to_string(),
            None => message.last().copied().unwrap_or("Done.").to_string(),
        })
    } else if message.is_empty() {
        Err(format!("git failed ({status})."))
    } else {
        Err(message.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{discover, status, tests::repo, StatusIndex};
    use super::*;

    #[test]
    fn stage_commit_branch_stash() {
        let Some(dir) = repo("write") else { return };
        let r = discover(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "two\n").unwrap();
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();
        stage(&r, &["a.txt".into(), "new.txt".into()]).unwrap();
        let idx = StatusIndex::new(&status(&r, false, || false).unwrap());
        assert_eq!(idx.mark("a.txt", false).map(|m| m.1), Some(true), "staged");
        unstage(&r, &["new.txt".into()]).unwrap();
        assert!(commit(&r, "", false).is_err());
        let first_line = commit(&r, "second", false).unwrap();
        assert!(first_line.contains("second"), "{first_line}");
        assert_eq!(log(&r, 5).unwrap()[0].subject, "second");
        assert_eq!(show(&r, "HEAD", "a.txt").unwrap().as_deref(), Some(&b"two\n"[..]));
        assert_eq!(show(&r, "HEAD", "new.txt").unwrap(), None);

        assert!(create_branch(&r, "bad..name").is_err());
        create_branch(&r, "feature/x").unwrap();
        assert!(branches(&r).unwrap().iter().any(|b| b.name == "feature/x" && b.current));
        switch(&r, "main").unwrap();

        std::fs::write(dir.join("a.txt"), "three\n").unwrap();
        discard(&r, &["a.txt".into()]).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "two\n");

        std::fs::write(dir.join("a.txt"), "stashed\n").unwrap();
        stash(&r, "wip").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "two\n");
        stash_pop(&r).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "stashed\n");
        assert!(sync(&r, Sync::Push, |_| {}, || false).is_err(), "no remote");

        assert_eq!(ignore(&r, &[("build".into(), true), ("notes [old].txt".into(), false)]).unwrap(), 2);
        assert_eq!(ignore(&r, &[("build".into(), true)]).unwrap(), 0, "already there");
        let text = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(text.ends_with("/build/\n/notes \\[old].txt\n"), "{text}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
