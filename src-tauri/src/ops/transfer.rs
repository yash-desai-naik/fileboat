//! Copying and moving files and folders: the work behind paste and drag and drop.
//!
//! A transfer reports progress, asks what to do when a name is taken (through `Control`), can be
//! cancelled at any point (a half-copied file is removed), and returns what it made or moved so
//! it can be undone. Moves within a drive are renames (instant); across drives they copy, then
//! delete the source only once the copy succeeded.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Copy,
    Move,
}

/// What to do when the destination already has an item of that name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// The old one goes to the Recycle Bin / Trash, the new one takes its place.
    Replace,
    Skip,
    /// Both stay: the new one gets a free name ("notes - Copy.txt").
    KeepBoth,
}

/// A name clash, for the user to decide.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub source: PathBuf,
    pub target: PathBuf,
    pub source_size: u64,
    pub target_size: u64,
    pub source_modified: f64,
    pub target_modified: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_items: u64,
    pub total_items: u64,
    /// The item being worked on, by name.
    pub current: String,
}

/// How a running transfer talks to whoever started it.
pub trait Control {
    fn progress(&self, p: &Progress);
    /// None cancels the whole transfer.
    fn conflict(&self, c: &Conflict) -> Option<Choice>;
    fn cancelled(&self) -> bool;
}

/// What a finished transfer did (for undo and the summary).
#[derive(Debug, Default)]
pub struct Outcome {
    /// Copy: the top-level items made (undo removes them).
    pub created: Vec<PathBuf>,
    /// Move: (from, to) of each top-level item moved.
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub skipped: usize,
}

const CANCELLED: &str = "Cancelled.";
/// Files bigger than this are copied in chunks, so progress moves and cancel works mid-file.
const CHUNKED: u64 = 16 * 1024 * 1024;

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn millis(m: &fs::Metadata) -> f64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// The name a kept-both copy gets: "notes - Copy.txt", "notes - Copy (2).txt" (Windows) or
/// "notes copy.txt", "notes copy 2.txt" (macOS), the first one free in `dir`.
pub fn copy_name(dir: &Path, name: &str) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let candidate = |n: u32| -> String {
        if cfg!(target_os = "macos") {
            if n == 1 { format!("{stem} copy{ext}") } else { format!("{stem} copy {n}{ext}") }
        } else if n == 1 {
            format!("{stem} - Copy{ext}")
        } else {
            format!("{stem} - Copy ({n}){ext}")
        }
    };
    (1..).map(candidate).find(|c| fs::symlink_metadata(dir.join(c)).is_err()).expect("a free name")
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Is `inner` the folder `outer` or somewhere inside it?
fn inside(inner: &Path, outer: &Path) -> bool {
    match (fs::canonicalize(inner), fs::canonicalize(outer)) {
        (Ok(i), Ok(o)) => i.starts_with(o),
        _ => false,
    }
}

/// Count what a copy will go through: (bytes, files and folders). Links aren't followed.
fn measure(p: &Path, ctl: &dyn Control) -> (u64, u64) {
    let Ok(meta) = fs::symlink_metadata(p) else { return (0, 0) };
    if meta.is_dir() {
        let mut total = (0, 1);
        for e in fs::read_dir(p).into_iter().flatten().flatten() {
            if ctl.cancelled() {
                break;
            }
            let (b, n) = measure(&e.path(), ctl);
            total.0 += b;
            total.1 += n;
        }
        total
    } else {
        (meta.len(), 1)
    }
}

fn is_cross_device(e: &std::io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE on Windows, EXDEV on macOS and Linux.
    e.kind() == std::io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(if cfg!(windows) { 17 } else { 18 })
}

struct Run<'a> {
    ctl: &'a dyn Control,
    mode: Mode,
    progress: Progress,
    skipped: usize,
}

impl Run<'_> {
    fn check(&self) -> Result<(), String> {
        if self.ctl.cancelled() { Err(CANCELLED.into()) } else { Ok(()) }
    }

    fn tick(&mut self, current: &Path, bytes: u64, items: u64) {
        self.progress.done_bytes += bytes;
        self.progress.done_items += items;
        self.progress.current = name_of(current);
        self.ctl.progress(&self.progress);
    }

    /// The target exists: ask, and return the path to write to (None = skip).
    fn resolve(&mut self, source: &Path, target: &Path) -> Result<Option<PathBuf>, String> {
        let (sm, tm) = (fs::metadata(source).ok(), fs::metadata(target).ok());
        let conflict = Conflict {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            source_size: sm.as_ref().map_or(0, |m| m.len()),
            target_size: tm.as_ref().map_or(0, |m| m.len()),
            source_modified: sm.as_ref().map_or(0.0, millis),
            target_modified: tm.as_ref().map_or(0.0, millis),
        };
        match self.ctl.conflict(&conflict).ok_or(CANCELLED)? {
            Choice::Skip => {
                self.skipped += 1;
                Ok(None)
            }
            Choice::KeepBoth => {
                let dir = target.parent().ok_or("No folder to copy into.")?;
                Ok(Some(dir.join(copy_name(dir, &name_of(target)))))
            }
            Choice::Replace => {
                // The replaced item goes to the bin (or, where there's none, is deleted). Tests
                // don't fill the real Recycle Bin.
                let old = vec![target.to_path_buf()];
                if cfg!(test) {
                    super::delete::delete_permanently(&old)?;
                } else {
                    super::delete::to_trash(&old).or_else(|_| super::delete::delete_permanently(&old))?;
                }
                Ok(Some(target.to_path_buf()))
            }
        }
    }

    /// Copy `src` to `dst` (a file, or a folder merged into an existing one).
    fn copy(&mut self, src: &Path, dst: &Path) -> Result<bool, String> {
        self.check()?;
        let meta = fs::symlink_metadata(src).map_err(|e| format!("Couldn't read \"{}\": {e}", name_of(src)))?;
        if meta.is_dir() {
            match fs::symlink_metadata(dst) {
                Ok(m) if m.is_dir() => {} // an existing folder of that name: merge into it
                Ok(_) => {
                    let Some(dst) = self.resolve(src, dst)? else { return Ok(false) };
                    return self.copy(src, &dst);
                }
                Err(_) => fs::create_dir(dst).map_err(|e| format!("Couldn't create \"{}\": {e}", name_of(dst)))?,
            }
            self.tick(src, 0, 1);
            let mut children: Vec<PathBuf> = fs::read_dir(src)
                .map_err(|e| format!("Couldn't read \"{}\": {e}", name_of(src)))?
                .flatten()
                .map(|e| e.path())
                .collect();
            children.sort();
            for child in children {
                self.copy(&child, &dst.join(child.file_name().unwrap_or_default()))?;
            }
            return Ok(true);
        }
        let dst = if fs::symlink_metadata(dst).is_ok() {
            match self.resolve(src, dst)? {
                Some(p) => p,
                None => {
                    self.tick(src, meta.len(), 1);
                    return Ok(false);
                }
            }
        } else {
            dst.to_path_buf()
        };
        self.copy_file(src, &dst, meta.len())?;
        Ok(true)
    }

    fn copy_file(&mut self, src: &Path, dst: &Path, len: u64) -> Result<(), String> {
        let fail = |e: std::io::Error| format!("Couldn't copy \"{}\": {e}", name_of(src));
        if len < CHUNKED {
            // The system's own copy (CopyFileEx on Windows): fast, keeps dates and attributes.
            fs::copy(src, dst).map_err(fail)?;
            self.tick(src, len, 1);
            return Ok(());
        }
        let result = (|| -> Result<(), String> {
            let mut from = fs::File::open(src).map_err(fail)?;
            let mut to = fs::OpenOptions::new().write(true).create_new(true).open(dst).map_err(fail)?;
            let mut buf = vec![0u8; 1024 * 1024];
            loop {
                self.check()?;
                let n = from.read(&mut buf).map_err(fail)?;
                if n == 0 {
                    break;
                }
                to.write_all(&buf[..n]).map_err(fail)?;
                self.tick(src, n as u64, 0);
            }
            // Keep the modified date and read-only flag, as the system copy does.
            let meta = fs::metadata(src).map_err(fail)?;
            if let Ok(t) = meta.modified() {
                let _ = to.set_modified(t);
            }
            drop(to);
            let _ = fs::set_permissions(dst, meta.permissions());
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.tick(src, 0, 1);
                Ok(())
            }
            Err(e) => {
                // Never leave half a file behind.
                let _ = fs::remove_file(dst);
                Err(e)
            }
        }
    }
}

/// Copy or move `sources` into the folder `dest`.
pub fn transfer(sources: &[PathBuf], dest: &Path, mode: Mode, ctl: &dyn Control) -> Result<Outcome, String> {
    if !dest.is_dir() {
        return Err(format!("\"{}\" isn't a folder.", dest.display()));
    }
    for s in sources {
        if s.is_dir() && inside(dest, s) {
            let verb = if mode == Mode::Copy { "copy" } else { "move" };
            return Err(format!("Can't {verb} \"{}\" into itself.", name_of(s)));
        }
    }
    let mut run = Run { ctl, mode, progress: Progress::default(), skipped: 0 };
    for s in sources {
        let (b, n) = measure(s, ctl);
        run.progress.total_bytes += b;
        run.progress.total_items += n;
    }
    ctl.progress(&run.progress);
    let mut out = Outcome::default();

    for s in sources {
        run.check()?;
        let name = s.file_name().ok_or_else(|| format!("Can't {} a drive.", if mode == Mode::Copy { "copy" } else { "move" }))?;
        let mut target = dest.join(name);
        let same_place = s.parent().is_some_and(|p| same_file(p, dest));

        if run.mode == Mode::Move {
            if same_place {
                continue; // moving into the folder it's already in: nothing to do
            }
            if fs::symlink_metadata(&target).is_ok() {
                match run.resolve(s, &target)? {
                    Some(t) => target = t,
                    None => continue,
                }
            }
            match fs::rename(s, &target) {
                Ok(()) => {
                    let (b, n) = measure(&target, ctl);
                    run.tick(&target, b, n);
                    out.moved.push((s.clone(), target));
                }
                Err(e) if is_cross_device(&e) => {
                    // Another drive: copy, then remove the source once the copy is complete.
                    if run.copy(s, &target)? {
                        super::delete::delete_permanently(std::slice::from_ref(s))?;
                        out.moved.push((s.clone(), target));
                    }
                }
                Err(e) => return Err(format!("Couldn't move \"{}\": {e}", name_of(s))),
            }
        } else {
            if same_place {
                // Copying into its own folder makes a second copy beside it.
                target = dest.join(copy_name(dest, &name.to_string_lossy()));
            }
            let existed = fs::symlink_metadata(&target).is_ok();
            if run.copy(s, &target)? && !existed {
                out.created.push(target);
            }
        }
    }
    out.skipped = run.skipped;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct Test {
        choice: Option<Choice>,
        asked: RefCell<Vec<String>>,
        cancel: Cell<bool>,
        last: RefCell<Progress>,
    }

    impl Test {
        fn new(choice: Option<Choice>) -> Test {
            Test { choice, asked: RefCell::new(Vec::new()), cancel: Cell::new(false), last: RefCell::new(Progress::default()) }
        }
    }

    impl Control for Test {
        fn progress(&self, p: &Progress) {
            *self.last.borrow_mut() = p.clone();
        }
        fn conflict(&self, c: &Conflict) -> Option<Choice> {
            self.asked.borrow_mut().push(name_of(&c.target));
            self.choice
        }
        fn cancelled(&self) -> bool {
            self.cancel.get()
        }
    }

    fn setup(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("fileboat-transfer-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (src, dst) = (root.join("src"), root.join("dst"));
        fs::create_dir_all(src.join("folder").join("deep")).unwrap();
        fs::write(src.join("a.txt"), "aaa").unwrap();
        fs::write(src.join("folder").join("b.txt"), "bb").unwrap();
        fs::write(src.join("folder").join("deep").join("c.txt"), "c").unwrap();
        fs::create_dir_all(&dst).unwrap();
        (root, src, dst)
    }

    #[test]
    fn copies_trees_with_progress() {
        let (root, src, dst) = setup("copy");
        let t = Test::new(None);
        let out = transfer(&[src.join("a.txt"), src.join("folder")], &dst, Mode::Copy, &t).unwrap();
        assert_eq!(fs::read_to_string(dst.join("folder").join("deep").join("c.txt")).unwrap(), "c");
        assert_eq!(out.created, vec![dst.join("a.txt"), dst.join("folder")]);
        let p = t.last.borrow();
        assert_eq!((p.done_bytes, p.total_bytes), (6, 6));
        assert_eq!(p.done_items, p.total_items);
        assert!(src.join("a.txt").exists(), "copy keeps the source");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn conflicts_skip_replace_keep_both() {
        let (root, src, dst) = setup("conflict");
        fs::write(dst.join("a.txt"), "old").unwrap();
        let skip = Test::new(Some(Choice::Skip));
        let out = transfer(&[src.join("a.txt")], &dst, Mode::Copy, &skip).unwrap();
        assert_eq!((out.skipped, fs::read_to_string(dst.join("a.txt")).unwrap().as_str()), (1, "old"));
        assert_eq!(*skip.asked.borrow(), vec!["a.txt"]);

        let both = Test::new(Some(Choice::KeepBoth));
        transfer(&[src.join("a.txt")], &dst, Mode::Copy, &both).unwrap();
        let copy = dst.join(if cfg!(target_os = "macos") { "a copy.txt" } else { "a - Copy.txt" });
        assert_eq!(fs::read_to_string(copy).unwrap(), "aaa");
        assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "old");

        let replace = Test::new(Some(Choice::Replace));
        transfer(&[src.join("a.txt")], &dst, Mode::Copy, &replace).unwrap();
        assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "aaa");

        // A folder that exists already is merged into; only clashing files are asked about.
        fs::create_dir_all(dst.join("folder")).unwrap();
        fs::write(dst.join("folder").join("b.txt"), "x").unwrap();
        let merge = Test::new(Some(Choice::Skip));
        transfer(&[src.join("folder")], &dst, Mode::Copy, &merge).unwrap();
        assert_eq!(*merge.asked.borrow(), vec!["b.txt"]);
        assert!(dst.join("folder").join("deep").join("c.txt").exists());

        // Cancelling at a clash stops the transfer.
        assert_eq!(transfer(&[src.join("a.txt")], &dst, Mode::Copy, &Test::new(None)).unwrap_err(), CANCELLED);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn same_folder_copy_keeps_both_and_moves_are_renames() {
        let (root, src, dst) = setup("move");
        let t = Test::new(None);
        transfer(&[src.join("a.txt")], &src, Mode::Copy, &t).unwrap();
        let expected = if cfg!(target_os = "macos") { "a copy.txt" } else { "a - Copy.txt" };
        assert!(src.join(expected).exists(), "duplicated beside the original");
        assert!(t.asked.borrow().is_empty());

        let out = transfer(&[src.join("folder")], &dst, Mode::Move, &t).unwrap();
        assert_eq!(out.moved, vec![(src.join("folder"), dst.join("folder"))]);
        assert!(!src.join("folder").exists() && dst.join("folder").join("deep").join("c.txt").exists());
        let out = transfer(&[src.join("a.txt")], &src, Mode::Move, &t).unwrap();
        assert!(out.moved.is_empty(), "moving into its own folder does nothing");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_into_itself_and_cancels_cleanly() {
        let (root, src, _dst) = setup("itself");
        let t = Test::new(None);
        let err = transfer(&[src.join("folder")], &src.join("folder").join("deep"), Mode::Copy, &t).unwrap_err();
        assert!(err.contains("into itself"), "{err}");
        let big = src.join("big.bin");
        fs::write(&big, vec![7u8; (CHUNKED + 1) as usize]).unwrap();
        let out_dir = root.join("out");
        fs::create_dir_all(&out_dir).unwrap();
        let cancel = Test::new(None);
        cancel.cancel.set(true);
        assert_eq!(transfer(&[big], &out_dir, Mode::Copy, &cancel).unwrap_err(), CANCELLED);
        assert!(!out_dir.join("big.bin").exists(), "no half-copied file left");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_names() {
        let d = std::env::temp_dir().join(format!("fileboat-copyname-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let first = copy_name(&d, "notes.txt");
        fs::write(d.join(&first), "").unwrap();
        let second = copy_name(&d, "notes.txt");
        if cfg!(target_os = "macos") {
            assert_eq!((first.as_str(), second.as_str()), ("notes copy.txt", "notes copy 2.txt"));
        } else {
            assert_eq!((first.as_str(), second.as_str()), ("notes - Copy.txt", "notes - Copy (2).txt"));
        }
        assert_eq!(copy_name(&d, ".env"), if cfg!(target_os = "macos") { ".env copy" } else { ".env - Copy" });
        let _ = fs::remove_dir_all(&d);
    }
}
