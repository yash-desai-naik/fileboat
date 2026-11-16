//! Folders: listing them, the folder model views read from, and paths.

pub mod entry;
pub mod group;
pub mod model;
pub mod natural;

pub use entry::{Entry, Kind};
pub use group::{Group, GroupBy};
pub use model::{Folder, SortKey, View};

use std::path::{Path, PathBuf};

/// List a folder, handing entries over in batches (the first screen shows before a huge folder
/// is fully read). Entries that vanish or can't be read while listing are skipped. Stops early
/// when `on_batch` returns false (the folder was closed).
pub fn list(path: &Path, batch_size: usize, mut on_batch: impl FnMut(Vec<Entry>) -> bool) -> Result<(), String> {
    let read = std::fs::read_dir(path).map_err(|e| describe(path, &e))?;
    let mut batch = Vec::with_capacity(batch_size);
    for item in read {
        let Ok(item) = item else { continue };
        if let Some(entry) = Entry::from_dir_entry(&item) {
            batch.push(entry);
        }
        if batch.len() >= batch_size && !on_batch(std::mem::replace(&mut batch, Vec::with_capacity(batch_size))) {
            return Ok(());
        }
    }
    on_batch(batch);
    Ok(())
}

/// The name a path really has on disk. Windows and macOS ignore case, so "readme.txt" can name
/// a file stored as "README.txt"; after a case-only rename, events mention both spellings.
#[cfg(windows)]
pub fn real_name(path: &Path) -> Option<String> {
    use windows::Win32::Storage::FileSystem::{FindClose, FindFirstFileW, WIN32_FIND_DATAW};
    use windows::core::HSTRING;
    let mut data = WIN32_FIND_DATAW::default();
    let handle = unsafe { FindFirstFileW(&HSTRING::from(path.as_os_str()), &mut data) }.ok()?;
    let _ = unsafe { FindClose(handle) };
    let end = data.cFileName.iter().position(|&c| c == 0).unwrap_or(data.cFileName.len());
    Some(String::from_utf16_lossy(&data.cFileName[..end]))
}

/// The name a path really has on disk (see the Windows version): found in its folder ignoring case.
#[cfg(not(windows))]
pub fn real_name(path: &Path) -> Option<String> {
    let wanted = path.file_name()?.to_string_lossy().to_lowercase();
    std::fs::read_dir(path.parent()?)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.to_lowercase() == wanted)
}

/// A folder error in plain words, naming the folder.
pub fn describe(path: &Path, e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    let shown = path.display();
    match e.kind() {
        NotFound => format!("{shown} doesn't exist."),
        PermissionDenied => format!("You don't have permission to open {shown}."),
        NotADirectory => format!("{shown} is a file, not a folder."),
        _ => format!("Couldn't open {shown}: {e}"),
    }
}

/// Folders matching a partly typed path, for the address bar: the folders in its parent whose
/// names start with the last part (any case), or every folder in it when it ends with a
/// separator. Hidden folders only when the typed part starts with a dot. Natural order.
pub fn complete(input: &str, base: Option<&Path>, limit: usize) -> Vec<PathBuf> {
    let ends_with_sep = input.ends_with(['/', '\\']);
    let resolved = resolve(input, base);
    let (dir, prefix) = if ends_with_sep || input.trim().is_empty() {
        (resolved, String::new())
    } else {
        match (resolved.parent(), resolved.file_name()) {
            (Some(parent), Some(name)) => (parent.to_path_buf(), name.to_string_lossy().to_lowercase()),
            _ => (resolved, String::new()),
        }
    };
    let Ok(read) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut found: Vec<Entry> = read
        .filter_map(|e| Entry::from_dir_entry(&e.ok()?))
        .filter(|e| e.kind == Kind::Folder && e.key.starts_with(&prefix) && (!e.hidden || prefix.starts_with('.')))
        .collect();
    found.sort_by(|a, b| natural::compare(&a.key, &b.key));
    found.into_iter().take(limit).map(|e| dir.join(&e.name)).collect()
}

/// A folder or file named on the command line (`fileboat .`, `fileboat D:\x`): the first argument
/// after the program that isn't a flag, made absolute against the folder it was typed in.
pub fn path_arg(args: &[String], cwd: &Path) -> Option<String> {
    let arg = args.iter().skip(1).find(|a| !a.starts_with('-'))?;
    Some(resolve(arg, Some(cwd)).display().to_string())
}

/// A path typed or pasted by the user, made absolute: `~` is the home folder, `%VAR%` / `$VAR`
/// are expanded, `/` works on Windows, and `.`/`..` are resolved without following links.
pub fn resolve(input: &str, base: Option<&Path>) -> PathBuf {
    let trimmed = input.trim().trim_matches('"');
    let expanded = expand_vars(trimmed);
    let p = crate::paths::expand_home(&expanded);
    let p = match base {
        Some(base) if !p.is_absolute() => base.join(p),
        _ => p,
    };
    normalize(&p)
}

/// `%USERPROFILE%` (Windows) and `$HOME` / `${HOME}` (both) from the environment; unknown
/// names are left as typed.
fn expand_vars(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if c == '%' {
            if let Some(end) = rest[1..].find('%') {
                let name = &rest[1..1 + end];
                if let Some(v) = std::env::var_os(name).filter(|_| !name.is_empty()) {
                    out.push_str(&v.to_string_lossy());
                    rest = &rest[end + 2..];
                    continue;
                }
            }
        } else if c == '$' {
            let (name, len) = if let Some(braced) = rest[1..].strip_prefix('{') {
                match braced.find('}') {
                    Some(end) => (&braced[..end], end + 3),
                    None => ("", 0),
                }
            } else {
                let n = rest[1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').count();
                (&rest[1..1 + n], n + 1)
            };
            if let Some(v) = std::env::var_os(name).filter(|_| !name.is_empty()) {
                out.push_str(&v.to_string_lossy());
                rest = &rest[len..];
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Resolve `.` and `..` lexically (no file system access, links not followed).
fn normalize(p: &Path) -> PathBuf {
    use std::path::Component::*;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            CurDir => {}
            ParentDir => {
                if !matches!(out.components().next_back(), None | Some(RootDir | Prefix(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    // "C:" alone means the current folder on that drive; Fileboat means its root.
    let s = out.as_os_str().to_string_lossy();
    if cfg!(windows) && s.len() == 2 && s.ends_with(':') {
        out.push("\\");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_paths() {
        let home = crate::paths::home_dir().unwrap();
        assert_eq!(resolve("~", None), home);
        assert_eq!(resolve("  \"~/a/./b/../c\" ", None), home.join("a").join("c"));
        let base = home.join("x");
        assert_eq!(resolve("..", Some(&base)), home);
        assert_eq!(resolve("y", Some(&base)), home.join("x").join("y"));
        // SAFETY: tests in this module are the only ones touching this variable.
        unsafe { std::env::set_var("FILEBOAT_TEST_DIR", home.join("t")) };
        assert_eq!(resolve("$FILEBOAT_TEST_DIR/q", None), home.join("t").join("q"));
        assert_eq!(resolve("${FILEBOAT_TEST_DIR}", None), home.join("t"));
        assert_eq!(resolve("%FILEBOAT_TEST_DIR%", None), home.join("t"));
        assert_eq!(expand_vars("100% sure, $NOPE_NOT_SET"), "100% sure, $NOPE_NOT_SET");
    }

    #[cfg(windows)]
    #[test]
    fn windows_roots() {
        assert_eq!(resolve("C:", None), PathBuf::from("C:\\"));
        assert_eq!(resolve("C:/Users/../Windows", None), PathBuf::from("C:\\Windows"));
        assert_eq!(resolve("C:\\..", None), PathBuf::from("C:\\"));
    }

    #[test]
    fn real_names_keep_their_case() {
        let dir = std::env::temp_dir().join(format!("fileboat-realname-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("README.txt"), "x").unwrap();
        assert_eq!(real_name(&dir.join("README.txt")).as_deref(), Some("README.txt"));
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(real_name(&dir.join("readme.txt")).as_deref(), Some("README.txt"));
        }
        assert_eq!(real_name(&dir.join("nope.txt")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_line_path() {
        let cwd = Path::new(if cfg!(windows) { "C:\\work" } else { "/work" });
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(path_arg(&args(&["fileboat"]), cwd), None);
        assert_eq!(path_arg(&args(&["fileboat", "--flag"]), cwd), None);
        assert_eq!(path_arg(&args(&["fileboat", "."]), cwd), Some(cwd.display().to_string()));
        assert_eq!(path_arg(&args(&["fileboat", "-x", "src"]), cwd), Some(cwd.join("src").display().to_string()));
    }

    #[test]
    fn completes_folders() {
        let dir = std::env::temp_dir().join(format!("fileboat-complete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["src", "Scripts", "docs", "src-tauri", "s10", "s9"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("setup.py"), "").unwrap();
        let names = |v: Vec<PathBuf>| v.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        let base = dir.display().to_string();
        assert_eq!(names(complete(&format!("{base}/s"), None, 10)), vec!["s9", "s10", "Scripts", "src", "src-tauri"]);
        assert_eq!(names(complete(&format!("{base}/SR"), None, 10)), vec!["src", "src-tauri"]);
        assert_eq!(names(complete(&format!("{base}/"), None, 2)), vec!["docs", "s9"]);
        assert_eq!(names(complete("sc", Some(&dir), 10)), vec!["Scripts"]);
        assert!(complete(&format!("{base}/nope/x"), None, 10).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_in_batches() {
        let dir = std::env::temp_dir().join(format!("fileboat-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("f{i}.txt")), "x".repeat(i)).unwrap();
        }
        let mut batches = Vec::new();
        list(&dir, 2, |b| {
            batches.push(b.len());
            true
        })
        .unwrap();
        assert_eq!(batches, vec![2, 2, 2, 0]);
        let mut f = Folder::new(dir.clone(), View::default());
        list(&dir, 100, |b| {
            f.extend(b);
            true
        })
        .unwrap();
        let rows: Vec<(String, u64)> = f.rows(0, 10).map(|e| (e.name.clone(), e.size)).collect();
        assert_eq!(rows[0].0, "sub");
        assert_eq!(rows[5], ("f4.txt".to_string(), 4));
        assert!(list(&dir.join("nope"), 10, |_| true).unwrap_err().contains("doesn't exist"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
