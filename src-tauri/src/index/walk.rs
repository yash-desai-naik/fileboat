//! Filling the index: a parallel walk (the `ignore` crate, so `.gitignore`, `.ignore` and
//! `.fbignore` are honoured when the rules say so) that applies the rules as it goes, never
//! follows links or junctions, and lists cloud folders without entering them (nothing is
//! downloaded). Reconciling one folder walks just that folder's own entries the same way.

use super::{Index, Meta, Rules, FLAG_UNLISTED, KIND_FILE, KIND_FOLDER, NONE};
use crate::preview::not_downloaded;
use ignore::{WalkBuilder, WalkState};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

struct Found {
    path: PathBuf,
    meta: Meta,
    /// A folder whose contents weren't walked (cloud-only).
    unlisted: bool,
}

fn millis(t: std::io::Result<std::time::SystemTime>) -> i64 {
    t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(windows)]
fn hidden_attr(m: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes() & 0x2 != 0
}

#[cfg(not(windows))]
fn hidden_attr(_m: &std::fs::Metadata) -> bool {
    false
}

fn meta_of(name: &str, m: &std::fs::Metadata) -> Meta {
    Meta {
        kind: if m.is_dir() { KIND_FOLDER } else { KIND_FILE },
        hidden: name.starts_with('.') || hidden_attr(m),
        size: if m.is_dir() { 0 } else { m.len() },
        mtime: millis(m.modified()),
    }
}

/// Walk `start` (depth 0 is `start` itself), calling `found` for each entry in an order where a
/// folder comes before what's in it. `max_depth` limits how deep.
fn walk(start: &Path, rules: &Rules, max_depth: Option<usize>, cancelled: &(dyn Fn() -> bool + Sync), mut found: impl FnMut(Found)) {
    let (tx, rx) = mpsc::channel::<Found>();
    let rules = Arc::new(rules.clone());
    let mut wb = WalkBuilder::new(start);
    wb.hidden(false)
        .follow_links(false)
        .max_depth(max_depth)
        .parents(rules.gitignore)
        .ignore(rules.gitignore)
        .git_ignore(rules.gitignore)
        .git_global(rules.gitignore)
        .git_exclude(rules.gitignore);
    if rules.gitignore {
        wb.add_custom_ignore_filename(".fbignore");
    }
    {
        let (rules, tx) = (rules.clone(), tx.clone());
        wb.filter_entry(move |e| {
            if e.depth() == 0 {
                return true;
            }
            let Some(ft) = e.file_type() else { return false };
            if ft.is_symlink() {
                return false;
            }
            let Ok(m) = e.metadata() else { return false };
            if !rules.hidden && hidden_attr(&m) {
                return false;
            }
            if rules.skip(e.path(), ft.is_dir()) {
                return false;
            }
            if ft.is_dir() && not_downloaded(&m) {
                // Listed, never entered: entering would fetch it from the cloud.
                let name = e.file_name().to_string_lossy();
                let _ = tx.send(Found { path: e.path().to_path_buf(), meta: meta_of(&name, &m), unlisted: true });
                return false;
            }
            true
        });
    }
    let walker = wb.build_parallel();
    // The builder holds a sender (in the filter): drop it, or the receiving loop never ends.
    drop(wb);
    std::thread::scope(|s| {
        s.spawn(move || {
            walker.run(|| {
                let tx = tx.clone();
                Box::new(move |res| {
                    if cancelled() {
                        return WalkState::Quit;
                    }
                    let Ok(e) = res else { return WalkState::Continue };
                    let meta = if e.depth() == 0 {
                        match std::fs::metadata(e.path()) {
                            Ok(m) => meta_of("", &m),
                            Err(_) => return WalkState::Quit,
                        }
                    } else {
                        match e.metadata() {
                            Ok(m) => meta_of(&e.file_name().to_string_lossy(), &m),
                            Err(_) => return WalkState::Continue,
                        }
                    };
                    let _ = tx.send(Found { path: e.into_path(), meta, unlisted: false });
                    WalkState::Continue
                })
            });
            drop(tx);
        });
        for f in rx {
            found(f);
        }
    });
}

/// Put one walked entry into the index; false when its folder isn't there yet.
fn place(idx: &mut Index, f: &Found, start: &Path, base: u32, ids: &mut HashMap<PathBuf, u32>) -> bool {
    let (parent, name) = if f.path == start {
        if base == NONE {
            let id = idx.add_root(start, f.meta);
            ids.insert(f.path.clone(), id);
            return true;
        }
        (base, start.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    } else {
        let Some(parent) = f.path.parent().and_then(|p| ids.get(p)).copied() else { return false };
        (parent, f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    };
    let id = match idx.child(parent, &name) {
        Some(existing) => {
            idx.update(existing, f.meta);
            existing
        }
        None => idx.push(parent, &name, f.meta),
    };
    if f.unlisted {
        idx.set_flag(id, FLAG_UNLISTED, true);
    }
    if f.meta.kind == KIND_FOLDER {
        ids.insert(f.path.clone(), id);
    }
    true
}

/// Put walked entries into the index under `base` (the id of `start`'s folder, or `NONE` to
/// make `start` a root); returns `start`'s id. Entries whose folder hasn't arrived yet wait.
fn insert_all(idx: &mut Index, start: &Path, base: u32, rules: &Rules, max_depth: Option<usize>, cancelled: &(dyn Fn() -> bool + Sync), progress: &mut dyn FnMut(usize)) -> Option<u32> {
    let mut ids: HashMap<PathBuf, u32> = HashMap::new();
    let mut waiting: Vec<Found> = Vec::new();
    let mut n = 0usize;
    walk(start, rules, max_depth, cancelled, |f| {
        if !place(idx, &f, start, base, &mut ids) {
            waiting.push(f);
        }
        n += 1;
        if n.is_multiple_of(20_000) {
            progress(n);
        }
    });
    // Anything that arrived before its folder.
    loop {
        let before = waiting.len();
        waiting.retain(|f| !place(idx, f, start, base, &mut ids));
        if waiting.is_empty() || waiting.len() == before {
            break;
        }
    }
    progress(n);
    ids.get(start).copied()
}

/// Build an index of `roots` from scratch.
pub fn build(roots: &[PathBuf], rules: &Rules, cancelled: &(dyn Fn() -> bool + Sync), progress: &mut dyn FnMut(usize)) -> Index {
    let mut idx = Index { rules: rules.fingerprint(), ..Index::default() };
    for root in roots {
        if cancelled() {
            break;
        }
        if root.is_dir() {
            insert_all(&mut idx, root, NONE, rules, None, cancelled, progress);
        }
    }
    idx
}

/// A folder that appeared (or changed) under `parent`: walk it and add everything in it.
pub fn walk_into(idx: &mut Index, parent: u32, path: &Path, rules: &Rules) -> Option<u32> {
    insert_all(idx, path, parent, rules, None, &|| false, &mut |_| {})
}

/// Bring one indexed folder's own entries up to date with the disk (not its subfolders',
/// except new ones, which are walked whole). Returns whether anything changed.
pub fn list_into(idx: &mut Index, folder: u32, path: &Path, rules: &Rules) -> bool {
    let mut seen: Vec<(PathBuf, Meta)> = Vec::new();
    walk(path, rules, Some(1), &|| false, |f| {
        if f.path != path {
            seen.push((f.path, f.meta));
        }
    });
    let mut changed = false;
    let mut keep = std::collections::HashSet::new();
    for (p, meta) in seen {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match idx.child(folder, &name) {
            Some(id) => {
                keep.insert(id);
                let old = Meta { kind: idx.kind(id), hidden: idx.flags(id) & super::FLAG_HIDDEN != 0, size: idx.size(id), mtime: idx.mtime(id) };
                if old != meta {
                    idx.update(id, meta);
                    changed = true;
                }
            }
            None => {
                let id = if meta.kind == KIND_FOLDER { walk_into(idx, folder, &p, rules) } else { Some(idx.push(folder, &name, meta)) };
                if let Some(id) = id {
                    keep.insert(id);
                }
                changed = true;
            }
        }
    }
    for id in idx.children(folder) {
        if !keep.contains(&id) {
            idx.remove(id);
            changed = true;
        }
    }
    if let Ok(m) = std::fs::metadata(path) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        idx.update(folder, meta_of(&name, &m));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fileboat-walk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn builds_with_rules_and_reconciles() {
        let d = tmp("build");
        std::fs::create_dir_all(d.join("app/src")).unwrap();
        std::fs::create_dir_all(d.join("app/node_modules/left-pad")).unwrap();
        std::fs::write(d.join("app/package.json"), "{}").unwrap();
        std::fs::write(d.join("app/src/main.ts"), "x").unwrap();
        std::fs::write(d.join("app/node_modules/left-pad/index.js"), "x").unwrap();
        std::fs::write(d.join("notes.md"), "hello").unwrap();
        let rules = Rules::new(&[], &[], &[], false, true);
        let idx = build(std::slice::from_ref(&d), &rules, &|| false, &mut |_| {});
        let names: std::collections::HashSet<&str> = (1..idx.len() as u32).map(|i| idx.name(i)).collect();
        assert!(names.contains("main.ts") && names.contains("notes.md") && names.contains("package.json"));
        assert!(!names.contains("node_modules") && !names.contains("index.js"));
        let main = idx.find(&d.join("app/src/main.ts")).unwrap();
        assert_eq!(idx.size(main), 1);

        // Reconcile the app folder after changes on disk.
        let mut idx = idx;
        std::fs::write(d.join("app/new.txt"), "abc").unwrap();
        std::fs::create_dir_all(d.join("app/docs/deep")).unwrap();
        std::fs::write(d.join("app/docs/deep/x.md"), "x").unwrap();
        std::fs::remove_file(d.join("app/package.json")).unwrap();
        let app = idx.find(&d.join("app")).unwrap();
        assert!(list_into(&mut idx, app, &d.join("app"), &rules));
        assert!(idx.find(&d.join("app/new.txt")).is_some());
        assert!(idx.find(&d.join("app/docs/deep/x.md")).is_some(), "a new folder is walked whole");
        assert!(idx.find(&d.join("app/package.json")).is_none());
        assert!(!list_into(&mut idx, app, &d.join("app"), &rules), "nothing changed the second time");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn gitignore_is_honoured_inside_repositories() {
        let d = tmp("gitignore");
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join(".gitignore"), "secret/\n*.log\n").unwrap();
        std::fs::create_dir_all(d.join("secret")).unwrap();
        std::fs::write(d.join("secret/a.txt"), "x").unwrap();
        std::fs::write(d.join("run.log"), "x").unwrap();
        std::fs::write(d.join("kept.txt"), "x").unwrap();
        let on = build(std::slice::from_ref(&d), &Rules::new(&[], &[], &[], false, true), &|| false, &mut |_| {});
        assert!(on.find(&d.join("kept.txt")).is_some());
        assert!(on.find(&d.join("secret")).is_none() && on.find(&d.join("run.log")).is_none());
        let off = build(std::slice::from_ref(&d), &Rules::new(&[], &[], &[], false, false), &|| false, &mut |_| {});
        assert!(off.find(&d.join("run.log")).is_some());
        assert!(off.find(&d.join(".git")).is_none(), ".git is a rule of its own");
        std::fs::remove_dir_all(&d).unwrap();
    }
}

/// `cargo test --release --lib bench_home -- --ignored --nocapture`: build an index of the home
/// folder (names and metadata only) and time a few searches.
#[cfg(test)]
mod bench {
    use super::super::{search, Query};
    use super::*;

    #[test]
    #[ignore]
    fn bench_home() {
        let home = dirs::home_dir().unwrap();
        let rules = Rules::new(&[], &[], &[], false, true);
        let t = std::time::Instant::now();
        let idx = build(std::slice::from_ref(&home), &rules, &|| false, &mut |_| {});
        println!("built {} entries in {:?}, ~{} MB", idx.live(), t.elapsed(), idx.bytes() / 1_000_000);
        let f = std::env::temp_dir().join("fileboat-bench-index.bin");
        let t = std::time::Instant::now();
        idx.save(&f).unwrap();
        println!("saved in {:?} ({} MB on disk)", t.elapsed(), std::fs::metadata(&f).unwrap().len() / 1_000_000);
        let t = std::time::Instant::now();
        let back = Index::load(&f).unwrap();
        println!("loaded in {:?}", t.elapsed());
        std::fs::remove_file(&f).unwrap();
        for q in ["main", "readme", "cargo.toml", "ext:rs main", "kind:image", "size:>100mb", "zzzzqqq"] {
            let t = std::time::Instant::now();
            let r = search(&back, &Query::parse(q).unwrap(), None, 50, 0);
            println!("{q:>14}: {} matches in {:?}", r.total, t.elapsed());
        }
    }
}
