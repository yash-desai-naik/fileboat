//! The edge cases from docs/PLAN.md §8 that the core can check without a window.

use fileboat_core::fs::{list, tree_size};
use fileboat_core::index::{self as walk, Rules};
use fileboat_core::ops::{self, delete, transfer};
use fileboat_core::preview::{self, read_text};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fileboat-edge-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    list(dir, 100, |batch| {
        names.extend(batch.into_iter().map(|e| e.name));
        true
    })
    .unwrap();
    names.sort();
    names
}

struct Quiet {
    cancel: Cell<bool>,
}
impl transfer::Control for Quiet {
    fn progress(&self, _: &transfer::Progress) {}
    fn conflict(&self, _: &transfer::Conflict) -> Option<transfer::Choice> {
        Some(transfer::Choice::KeepBoth)
    }
    fn cancelled(&self) -> bool {
        self.cancel.get()
    }
}
fn quiet() -> Quiet {
    Quiet { cancel: Cell::new(false) }
}

fn rules() -> Rules {
    Rules::new(&[], &[], &[], true, true)
}

/// Paths over 260 characters: listed, read, sized, copied, indexed and deleted.
#[test]
fn long_paths() {
    let root = temp_dir("long");
    let mut deep = root.clone();
    for i in 0..12 {
        deep.push(format!("a-fairly-long-folder-name-number-{i:02}"));
    }
    assert!(deep.as_os_str().len() > 300);
    fs::create_dir_all(&deep).unwrap();
    let file = deep.join("notes.txt");
    fs::write(&file, "hello\n").unwrap();

    assert_eq!(names_in(&deep), vec!["notes.txt"]);
    assert_eq!(read_text(&file, 1024).unwrap().text, "hello\n");
    assert!(preview::info(&file).is_ok());
    let size = tree_size(&root, || false).unwrap();
    assert_eq!((size.bytes, size.files, size.partial), (6, 1, false));

    let dest = temp_dir("long-dest");
    transfer::transfer(&[root.join("a-fairly-long-folder-name-number-00")], &dest, transfer::Mode::Copy, &quiet()).unwrap();
    assert!(dest.join(deep.strip_prefix(&root).unwrap()).join("notes.txt").exists());

    let idx = walk::build(std::slice::from_ref(&root), &rules(), &|| false, &mut |_| {});
    assert!(idx.find(&file).is_some(), "indexed");

    let renamed = ops::rename(&file, "renamed.txt").unwrap();
    assert!(renamed.exists());
    delete::delete_permanently(&[root.clone(), dest.clone()]).unwrap();
    assert!(!root.exists() && !dest.exists());
}

/// Names Windows' usual APIs can't make (a trailing dot or space, a device name) but that exist,
/// made by WSL, git or a Mac: listed, read, copied, renamed and deleted all the same.
#[cfg(windows)]
#[test]
fn awkward_windows_names() {
    let root = temp_dir("names");
    let verbatim = |p: &Path| PathBuf::from(format!(r"\\?\{}", p.display()));
    let names = ["dot.", "space ", "con.txt"];
    for n in names {
        fs::write(verbatim(&root.join(n)), n).unwrap();
    }
    fs::create_dir(verbatim(&root.join("folder."))).unwrap();
    fs::write(verbatim(&root.join("folder.").join("inside.txt")), "x").unwrap();

    assert_eq!(names_in(&root), vec!["con.txt", "dot.", "folder.", "space "]);
    for n in names {
        assert_eq!(read_text(&root.join(n), 1024).map(|t| t.text), Ok(n.to_string()), "read {n:?}");
    }
    assert_eq!(names_in(&root.join("folder.")), vec!["inside.txt"]);
    assert_eq!(tree_size(&root, || false).unwrap().files, 4);

    let dest = temp_dir("names-dest");
    transfer::transfer(&names.iter().map(|n| root.join(n)).chain([root.join("folder.")]).collect::<Vec<_>>(), &dest, transfer::Mode::Copy, &quiet()).unwrap();
    assert_eq!(names_in(&dest), vec!["con.txt", "dot.", "folder.", "space "]);

    let fixed = ops::rename(&root.join("dot."), "dot.txt").unwrap();
    assert_eq!(fs::read_to_string(fixed).unwrap(), "dot.");
    delete::delete_permanently(&[root.join("space "), root.join("folder.")]).unwrap();
    assert_eq!(names_in(&root), vec!["con.txt", "dot.txt"]);
    delete::delete_permanently(&[root, dest]).unwrap();
}

/// A junction that points back up never loops the index or folder sizes.
#[cfg(windows)]
#[test]
fn junction_loops() {
    let root = temp_dir("loop");
    fs::create_dir_all(root.join("a").join("b")).unwrap();
    fs::write(root.join("a").join("b").join("f.txt"), "12345").unwrap();
    let link = root.join("a").join("b").join("up");
    let ok = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&root)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "mklink /J");

    let size = tree_size(&root, || false).unwrap();
    assert_eq!((size.bytes, size.files), (5, 1));
    let idx = walk::build(std::slice::from_ref(&root), &rules(), &|| false, &mut |_| {});
    assert!(idx.live() < 10, "{} entries", idx.live());
    assert!(idx.find(&root.join("a").join("b").join("f.txt")).is_some());
    assert!(idx.find(&link.join("a")).is_none(), "not followed");
    fs::remove_dir(&link).unwrap();
    fs::remove_dir_all(&root).unwrap();
}

/// A huge file is previewed from its start only, quickly; binary that looks like text is hex.
#[test]
fn huge_and_binary_files() {
    let root = temp_dir("big");
    let log = root.join("huge.log");
    {
        let f = fs::File::create(&log).unwrap();
        use std::io::Write;
        // A megabyte of real lines, then the rest of 2 GB (sparse, so the test is quick).
        let lines: String = (0..20_000).map(|i| format!("2026-11-16 12:00:00 INFO request {i} done\n")).collect();
        (&f).write_all(lines.as_bytes()).unwrap();
        f.set_len(2 * 1024 * 1024 * 1024 + 7).unwrap();
    }
    let started = std::time::Instant::now();
    let t = read_text(&log, 512 * 1024).unwrap();
    assert!(t.truncated && t.total > 2_000_000_000);
    assert!(t.text.starts_with("2026-11-16 12:00:00 INFO request 0 done\n"));
    assert!(t.text.len() <= 512 * 1024 && t.text.ends_with('\n'));
    assert!(started.elapsed().as_millis() < 2000, "{:?}", started.elapsed());
    assert!(matches!(preview::info(&log).unwrap().viewer, preview::Viewer::Text));

    // Text with NULs in it, and a PNG-looking header named .txt.
    let bin = root.join("data.txt");
    fs::write(&bin, b"abc\0\0\0\x01\x02def\0ghi").unwrap();
    assert!(!preview::looks_like_text(&fs::read(&bin).unwrap()));
    let png = root.join("image.txt");
    fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
    assert!(!preview::looks_like_text(&fs::read(&png).unwrap()));
    fs::remove_dir_all(&root).unwrap();
}

/// UTF-16 with a byte-order mark reads as text, with the encoding named.
#[test]
fn utf16_with_bom() {
    let root = temp_dir("utf16");
    let file = root.join("w.txt");
    let mut bytes = vec![0xFF, 0xFE];
    for u in "héllo\r\nwörld".encode_utf16() {
        bytes.extend(u.to_le_bytes());
    }
    fs::write(&file, &bytes).unwrap();
    let t = read_text(&file, 1024).unwrap();
    assert_eq!(t.text, "héllo\r\nwörld");
    assert!(t.encoding.to_lowercase().contains("utf-16"), "{}", t.encoding);
    assert!(preview::info(&file).unwrap().viewer != preview::Viewer::Binary);
    fs::remove_dir_all(&root).unwrap();
}

/// A file another program holds open: the message says which program, when Windows knows.
#[cfg(windows)]
#[test]
fn locked_files_name_the_program() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = temp_dir("locked");
    fs::create_dir(root.join("folder")).unwrap();
    let file = root.join("folder").join("report.docx");
    fs::write(&file, "x").unwrap();
    // Open with no sharing at all, as Word does.
    let held = fs::OpenOptions::new().read(true).write(true).share_mode(0).open(&file).unwrap();

    let err = delete::delete_permanently(std::slice::from_ref(&root.join("folder"))).unwrap_err();
    assert!(err.contains("\"report.docx\" in it is open in "), "{err}");
    let err = ops::rename(&file, "renamed.docx").unwrap_err();
    assert!(err.contains("it's open in "), "{err}");
    let dest = temp_dir("locked-dest");
    let err = transfer::transfer(std::slice::from_ref(&file), &dest, transfer::Mode::Copy, &quiet()).unwrap_err();
    assert!(err.contains("it's open in "), "{err}");

    drop(held);
    delete::delete_permanently(&[root, dest]).unwrap();
}
