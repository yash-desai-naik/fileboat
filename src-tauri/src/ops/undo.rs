//! Undo for file operations: each one done in Fileboat is remembered (for this session) with
//! what it takes to reverse it. Ctrl+Z reverses the most recent, wherever it happened.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub enum Op {
    /// A new file or folder.
    Created(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    /// Moved to the Recycle Bin / Trash at `at` (seconds since 1970).
    Trashed { paths: Vec<PathBuf>, at: i64 },
    /// Moved: each (from, to).
    Moved(Vec<(PathBuf, PathBuf)>),
    /// Copies made (they are what undo removes).
    Copied(Vec<PathBuf>),
}

const LIMIT: usize = 100;

#[derive(Default)]
pub struct UndoLog(Mutex<Vec<(Op, String)>>);

impl UndoLog {
    /// Remember an operation and how it's described ("Rename “a” to “b”").
    pub fn push(&self, op: Op, description: String) {
        let mut log = self.0.lock().unwrap();
        log.push((op, description));
        if log.len() > LIMIT {
            log.remove(0);
        }
    }

    /// Reverse the most recent operation. Ok(None) when there's nothing to undo; the description of
    /// what was undone otherwise. If it can't be undone it is forgotten, with the reason.
    pub fn undo(&self) -> Result<Option<String>, String> {
        let Some((op, description)) = self.0.lock().unwrap().pop() else { return Ok(None) };
        reverse(&op).map(|()| Some(description))
    }
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn reverse(op: &Op) -> Result<(), String> {
    match op {
        Op::Created(path) => remove_new(path),
        Op::Renamed { from, to } => {
            if !to.exists() {
                return Err(format!("\"{}\" isn't there any more, so the rename can't be undone.", name_of(to)));
            }
            super::rename(to, &name_of(from)).map(|_| ())
        }
        Op::Trashed { paths, at } => restore_from_trash(paths, *at),
        Op::Moved(pairs) => {
            for (from, to) in pairs.iter().rev() {
                if std::fs::symlink_metadata(from).is_ok() {
                    return Err(format!("\"{}\" exists again where it was moved from, so it isn't moved back.", name_of(from)));
                }
                std::fs::rename(to, from).map_err(|e| format!("Couldn't move \"{}\" back: {e}", name_of(to)))?;
            }
            Ok(())
        }
        Op::Copied(created) => super::delete::to_trash(&created.iter().filter(|p| p.exists()).cloned().collect::<Vec<_>>()),
    }
}

/// Undo "new": an untouched new item (empty folder, empty file) is removed; one with content
/// since then goes to the Recycle Bin / Trash, so nothing written into it is lost.
fn remove_new(path: &Path) -> Result<(), String> {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return Ok(()) };
    let untouched = if meta.is_dir() {
        std::fs::read_dir(path).map(|mut d| d.next().is_none()).unwrap_or(false)
    } else {
        meta.len() == 0
    };
    if untouched {
        let r = if meta.is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) };
        r.map_err(|e| format!("Couldn't remove \"{}\": {e}", name_of(path)))
    } else {
        super::delete::to_trash(&[path.to_path_buf()])
    }
}

/// Put items back from the Recycle Bin: for each path, the newest item deleted from there since
/// `at` (a little earlier, for clock rounding).
#[cfg(windows)]
fn restore_from_trash(paths: &[PathBuf], at: i64) -> Result<(), String> {
    let items = trash::os_limited::list().map_err(|e| format!("Couldn't read the Recycle Bin: {e}"))?;
    let mut chosen = Vec::new();
    for p in paths {
        let (Some(parent), Some(name)) = (p.parent(), p.file_name()) else { continue };
        let parent = real_folder(parent);
        let same: Vec<&trash::TrashItem> =
            items.iter().filter(|i| i.name == name && same_path(&real_folder(&i.original_parent), &parent)).collect();
        // The one deleted by this operation (a little leeway for clock rounding), else the newest.
        let best = same
            .iter()
            .copied()
            .filter(|i| i.time_deleted >= at - 60)
            .max_by_key(|i| i.time_deleted)
            .or_else(|| same.iter().copied().max_by_key(|i| i.time_deleted));
        match best {
            Some(item) => chosen.push(item.clone()),
            None => return Err(format!("\"{}\" isn't in the Recycle Bin any more.", name_of(p))),
        }
    }
    trash::os_limited::restore_all(chosen).map_err(|e| match e {
        trash::Error::RestoreCollision { path, .. } => {
            format!("\"{}\" exists again where it was, so it stays in the Recycle Bin.", name_of(&path))
        }
        other => format!("Couldn't restore from the Recycle Bin: {other}"),
    })
}

/// A folder's real, full path: "C:\Users\YASHDE~1" (a short 8.3 name) and "C:\Users\Yash Desai"
/// are the same folder, and the Recycle Bin records the long form.
#[cfg(windows)]
fn real_folder(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

#[cfg(windows)]
fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

#[cfg(not(windows))]
fn restore_from_trash(paths: &[PathBuf], _at: i64) -> Result<(), String> {
    let what = if paths.len() == 1 { format!("\"{}\"", name_of(&paths[0])) } else { format!("{} items", paths.len()) };
    Err(format!("Fileboat can't take things out of the Trash yet: open the Trash in Finder, select {what} and choose Put Back."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fileboat-undo-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn undo_new_rename_and_move() {
        let d = temp_dir("basic");
        let log = UndoLog::default();
        assert_eq!(log.undo().unwrap(), None);

        let folder = super::super::create_folder(&d, "New folder").unwrap();
        log.push(Op::Created(folder.clone()), "New folder".into());
        let renamed = super::super::rename(&folder, "src").unwrap();
        log.push(Op::Renamed { from: folder.clone(), to: renamed.clone() }, "Rename".into());
        std::fs::create_dir(d.join("dest")).unwrap();
        let moved = d.join("dest").join("src");
        std::fs::rename(&renamed, &moved).unwrap();
        log.push(Op::Moved(vec![(renamed.clone(), moved.clone())]), "Move".into());

        assert_eq!(log.undo().unwrap().as_deref(), Some("Move"));
        assert!(renamed.is_dir() && !moved.exists());
        assert_eq!(log.undo().unwrap().as_deref(), Some("Rename"));
        assert!(folder.is_dir() && !renamed.exists());
        assert_eq!(log.undo().unwrap().as_deref(), Some("New folder"));
        assert!(!folder.exists(), "an untouched new folder is removed");
        assert_eq!(log.undo().unwrap(), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn undo_rename_when_the_file_is_gone_explains() {
        let d = temp_dir("gone");
        let log = UndoLog::default();
        log.push(Op::Renamed { from: d.join("a"), to: d.join("b") }, "Rename".into());
        assert!(log.undo().unwrap_err().contains("isn't there any more"));
        assert_eq!(log.undo().unwrap(), None, "a failed undo is forgotten");
        let _ = std::fs::remove_dir_all(&d);
    }
}
