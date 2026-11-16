//! `git status --porcelain=v2 -z --branch` read into a status, and an index that answers
//! "what does a view show for this path?" quickly for every row of a folder.

use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// What a view shows for a file or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mark {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Ignored,
    Conflicted,
    /// A folder with changes somewhere inside.
    Changed,
}

/// The branch line of the status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    /// The branch, or None when detached.
    pub head: Option<String>,
    /// The commit (None before the first commit).
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// One path in the status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    /// Relative to the root, with `/`; folders (untracked or ignored as a whole) end with `/`.
    pub path: String,
    /// Staged change: `M`, `A`, `D`, `R`, `C`, `T`, `U` or `.` for none.
    pub index: char,
    /// Unstaged change, the same letters.
    pub worktree: char,
    /// The old path of a rename or copy.
    pub orig: Option<String>,
    pub conflicted: bool,
    pub untracked: bool,
    pub ignored: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub branch: Branch,
    pub files: Vec<FileStatus>,
}

fn field(s: &str, n: usize) -> Option<&str> {
    s.splitn(n + 1, ' ').nth(n)
}

/// Parse the output of `git status --porcelain=v2 -z --branch`.
pub fn parse_status(out: &[u8]) -> Status {
    let mut st = Status::default();
    let mut records = out.split(|&b| b == 0).map(|r| String::from_utf8_lossy(r).into_owned());
    while let Some(rec) = records.next() {
        if rec.is_empty() {
            continue;
        }
        let tail = |n| field(&rec, n).unwrap_or_default().to_string();
        match rec.as_bytes()[0] {
            b'#' => {
                if let Some(v) = rec.strip_prefix("# branch.oid ") {
                    st.branch.oid = (v != "(initial)").then(|| v.to_string());
                } else if let Some(v) = rec.strip_prefix("# branch.head ") {
                    st.branch.head = (v != "(detached)").then(|| v.to_string());
                } else if let Some(v) = rec.strip_prefix("# branch.upstream ") {
                    st.branch.upstream = Some(v.to_string());
                } else if let Some(v) = rec.strip_prefix("# branch.ab ") {
                    let mut it = v.split(' ');
                    st.branch.ahead = it.next().and_then(|a| a.trim_start_matches('+').parse().ok()).unwrap_or(0);
                    st.branch.behind = it.next().and_then(|b| b.trim_start_matches('-').parse().ok()).unwrap_or(0);
                }
            }
            b'1' | b'2' | b'u' => {
                let xy: Vec<char> = field(&rec, 1).unwrap_or("..").chars().collect();
                let (index, worktree) = (xy.first().copied().unwrap_or('.'), xy.get(1).copied().unwrap_or('.'));
                let (path, orig, conflicted) = match rec.as_bytes()[0] {
                    // 1 XY sub mH mI mW hH hI path
                    b'1' => (tail(8), None, false),
                    // 2 XY sub mH mI mW hH hI Xscore path, then the old path as its own record
                    b'2' => (tail(9), records.next(), false),
                    // u XY sub m1 m2 m3 mW h1 h2 h3 path
                    _ => (tail(10), None, true),
                };
                st.files.push(FileStatus { path, index, worktree, orig, conflicted, untracked: false, ignored: false });
            }
            b'?' => st.files.push(FileStatus { path: rec[2..].to_string(), index: '.', worktree: '?', orig: None, conflicted: false, untracked: true, ignored: false }),
            b'!' => st.files.push(FileStatus { path: rec[2..].to_string(), index: '.', worktree: '!', orig: None, conflicted: false, untracked: false, ignored: true }),
            _ => {}
        }
    }
    st
}

/// A changed path's mark and whether (any of) its change is staged.
fn mark_of(f: &FileStatus) -> (Mark, bool) {
    if f.conflicted {
        return (Mark::Conflicted, false);
    }
    if f.untracked {
        return (Mark::Untracked, false);
    }
    if f.ignored {
        return (Mark::Ignored, false);
    }
    let staged = f.index != '.';
    // The worktree change shows if there is one; otherwise the staged one.
    let letter = if f.worktree != '.' { f.worktree } else { f.index };
    let mark = match letter {
        'A' => Mark::Added,
        'D' => Mark::Deleted,
        'R' | 'C' => Mark::Renamed,
        _ => Mark::Modified,
    };
    // A new file staged and then edited is still "added".
    (if f.index == 'A' { Mark::Added } else { mark }, staged)
}

/// Answers "what does this path show?" for the rows of a folder.
#[derive(Debug, Default)]
pub struct StatusIndex {
    files: HashMap<String, (Mark, bool)>,
    /// Folders with a change (not counting ignored files) somewhere inside.
    dirty: HashSet<String>,
    /// Folders untracked or ignored as a whole (without the trailing `/`).
    untracked_dirs: HashSet<String>,
    ignored_dirs: HashSet<String>,
}

impl StatusIndex {
    pub fn new(st: &Status) -> StatusIndex {
        let mut idx = StatusIndex::default();
        for f in &st.files {
            let (mark, staged) = mark_of(f);
            let path = f.path.trim_end_matches('/');
            if f.path.ends_with('/') {
                if f.untracked {
                    idx.untracked_dirs.insert(path.to_string());
                } else if f.ignored {
                    idx.ignored_dirs.insert(path.to_string());
                }
            }
            idx.files.insert(path.to_string(), (mark, staged));
            if !f.ignored {
                let mut p = path;
                while let Some(i) = p.rfind('/') {
                    p = &p[..i];
                    if !idx.dirty.insert(p.to_string()) {
                        break;
                    }
                }
            }
        }
        idx
    }

    /// The mark for `rel` (relative to the root, `/`-separated), if any.
    pub fn mark(&self, rel: &str, is_dir: bool) -> Option<(Mark, bool)> {
        if let Some(&m) = self.files.get(rel) {
            return Some(m);
        }
        // Inside a folder that's untracked or ignored as a whole.
        let mut p = rel;
        while let Some(i) = p.rfind('/') {
            p = &p[..i];
            if self.untracked_dirs.contains(p) {
                return Some((Mark::Untracked, false));
            }
            if self.ignored_dirs.contains(p) {
                return Some((Mark::Ignored, false));
            }
        }
        (is_dir && self.dirty.contains(rel)).then_some((Mark::Changed, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_v2() {
        let out = b"# branch.oid 1234567890abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaa bbb src/a.rs\0\
1 A. N... 000000 100644 100644 000 ccc new file.txt\0\
2 R. N... 100644 100644 100644 ddd ddd R100 docs/new.md\0docs/old.md\0\
u UU N... 100644 100644 100644 100644 e f g conflict.txt\0\
? notes/\0! target/\0";
        let s = parse_status(out);
        assert_eq!(
            s.branch,
            Branch { head: Some("main".into()), oid: Some("1234567890abcdef".into()), upstream: Some("origin/main".into()), ahead: 2, behind: 1 }
        );
        let paths: Vec<&str> = s.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/a.rs", "new file.txt", "docs/new.md", "conflict.txt", "notes/", "target/"]);
        assert_eq!(s.files[2].orig.as_deref(), Some("docs/old.md"));
        let idx = StatusIndex::new(&s);
        assert_eq!(idx.mark("src/a.rs", false), Some((Mark::Modified, false)));
        assert_eq!(idx.mark("new file.txt", false), Some((Mark::Added, true)));
        assert_eq!(idx.mark("docs/new.md", false), Some((Mark::Renamed, true)));
        assert_eq!(idx.mark("conflict.txt", false), Some((Mark::Conflicted, false)));
        assert_eq!(idx.mark("docs", true), Some((Mark::Changed, false)));
        assert_eq!(idx.mark("notes/todo.md", false), Some((Mark::Untracked, false)));
        assert_eq!(idx.mark("target/debug", true), Some((Mark::Ignored, false)));
        assert_eq!(idx.mark("README.md", false), None);
        // Before the first commit, and detached.
        let s = parse_status(b"# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!((s.branch.oid, s.branch.head), (None, None));
    }
}
