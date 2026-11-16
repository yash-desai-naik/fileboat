//! Changing files: creating, renaming (and, in the following modules, deleting, copying, moving).
//! Every function checks names first and answers in plain words.

use std::path::{Path, PathBuf};

/// Names Windows reserves for devices, with or without an extension ("nul.txt" too).
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Is `name` usable for a file or folder here? Checks what the OS would refuse (or accept but
/// make awkward), so the user gets a clear reason instead of an OS error code.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Enter a name.".into());
    }
    if name == "." || name == ".." {
        return Err(format!("\"{name}\" can't be used as a name."));
    }
    if name.len() > 255 {
        return Err("That name is too long (at most 255 characters).".into());
    }
    if name.contains(['/', '\0']) || (cfg!(windows) && name.contains('\\')) {
        return Err("A name can't contain a slash.".into());
    }
    if cfg!(windows) {
        if let Some(c) = name.chars().find(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || (*c as u32) < 32) {
            let shown = if (c as u32) < 32 { "control characters".to_string() } else { format!("{c}") };
            return Err(format!("A name can't contain {shown}. Windows doesn't allow < > : \" | ? *"));
        }
        if name.ends_with(['.', ' ']) {
            return Err("On Windows a name can't end with a dot or a space.".into());
        }
        let stem = name.split('.').next().unwrap_or("").trim_end().to_lowercase();
        if RESERVED.contains(&stem.as_str()) {
            return Err(format!("\"{name}\" is reserved by Windows for a device. Choose another name."));
        }
    }
    Ok(())
}

/// A name in `dir` not taken yet: "New folder", then "New folder (2)", "New folder (3)"…; for a
/// file the number goes before the extension: "New file (2).txt".
pub fn unique_name(dir: &Path, wanted: &str) -> String {
    if !dir.join(wanted).exists() {
        return wanted.to_string();
    }
    let (stem, ext) = match wanted.rfind('.') {
        Some(i) if i > 0 => (&wanted[..i], &wanted[i..]),
        _ => (wanted, ""),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !dir.join(candidate).exists())
        .expect("some number is free")
}

fn io_error(what: &str, path: &Path, e: std::io::Error) -> String {
    use std::io::ErrorKind::*;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    match e.kind() {
        AlreadyExists => format!("\"{name}\" already exists here."),
        PermissionDenied => format!("You don't have permission to {what} \"{name}\". It may be open in another program, or read-only."),
        NotFound => format!("\"{name}\" doesn't exist (any more)."),
        _ => format!("Couldn't {what} \"{name}\": {e}"),
    }
}

/// Make a new folder in `dir`. Fails if the name is taken.
pub fn create_folder(dir: &Path, name: &str) -> Result<PathBuf, String> {
    check_name(name)?;
    let path = dir.join(name);
    std::fs::create_dir(&path).map_err(|e| io_error("create", &path, e))?;
    Ok(path)
}

/// Make a new, empty file in `dir`. Fails if the name is taken (never overwrites).
pub fn create_file(dir: &Path, name: &str) -> Result<PathBuf, String> {
    check_name(name)?;
    let path = dir.join(name);
    std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| io_error("create", &path, e))?;
    Ok(path)
}

/// Rename `path` to `new_name` in the same folder. Returns the new path. A name that differs only
/// in case is renamed through a temporary name (Windows and macOS see both names as one file).
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    check_name(new_name)?;
    let old_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if old_name == new_name {
        return Ok(path.to_path_buf());
    }
    let dir = path.parent().ok_or("Can't rename a drive or the root.")?;
    let target = dir.join(new_name);
    let case_only = old_name.to_lowercase() == new_name.to_lowercase();
    if case_only {
        let temp = dir.join(unique_name(dir, &format!("{old_name}.fileboat-rename")));
        std::fs::rename(path, &temp).map_err(|e| io_error("rename", path, e))?;
        if let Err(e) = std::fs::rename(&temp, &target) {
            // Put it back as it was.
            let _ = std::fs::rename(&temp, path);
            return Err(io_error("rename", path, e));
        }
        return Ok(target);
    }
    // std::fs::rename replaces an existing file on some systems: never do that silently.
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(format!("\"{new_name}\" already exists here."));
    }
    std::fs::rename(path, &target).map_err(|e| io_error("rename", path, e))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fileboat-ops-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names() {
        assert!(check_name("main.rs").is_ok());
        assert!(check_name(".env").is_ok());
        assert!(check_name("  ").is_err());
        assert!(check_name("..").is_err());
        assert!(check_name("a/b").is_err());
        if cfg!(windows) {
            assert!(check_name("a:b").unwrap_err().contains(":"));
            assert!(check_name("what?").is_err());
            assert!(check_name("trailing.").is_err());
            assert!(check_name("trailing ").is_err());
            assert!(check_name("CON").unwrap_err().contains("reserved"));
            assert!(check_name("nul.txt").unwrap_err().contains("reserved"));
            assert!(check_name("console.log").is_ok());
            assert!(check_name("com10").is_ok());
        }
    }

    #[test]
    fn unique_names() {
        let d = temp_dir("unique");
        assert_eq!(unique_name(&d, "New folder"), "New folder");
        std::fs::create_dir(d.join("New folder")).unwrap();
        std::fs::create_dir(d.join("New folder (2)")).unwrap();
        assert_eq!(unique_name(&d, "New folder"), "New folder (3)");
        std::fs::write(d.join("notes.txt"), "").unwrap();
        assert_eq!(unique_name(&d, "notes.txt"), "notes (2).txt");
        std::fs::write(d.join(".env"), "").unwrap();
        assert_eq!(unique_name(&d, ".env"), ".env (2)");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn create_and_rename() {
        let d = temp_dir("rename");
        let f = create_folder(&d, "src").unwrap();
        assert!(f.is_dir());
        assert!(create_folder(&d, "src").unwrap_err().contains("already exists"));
        let file = create_file(&d, "readme.md").unwrap();
        std::fs::write(&file, "hello").unwrap();
        assert!(create_file(&d, "readme.md").unwrap_err().contains("already exists"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello", "never overwritten");

        // Case-only rename works and keeps the content.
        let renamed = rename(&file, "README.md").unwrap();
        let listed: Vec<String> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(listed.contains(&"README.md".to_string()), "{listed:?}");
        assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "hello");

        // Renaming onto an existing name is refused, not a silent overwrite.
        create_file(&d, "other.md").unwrap();
        assert!(rename(&renamed, "other.md").unwrap_err().contains("already exists"));
        assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "hello");
        let moved = rename(&renamed, "docs.md").unwrap();
        assert!(moved.exists() && !renamed.exists());
        assert!(rename(&moved, "bad|name").is_err() || !cfg!(windows));
        let _ = std::fs::remove_dir_all(&d);
    }
}
