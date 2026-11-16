use std::path::{Path, PathBuf};

/// `%APPDATA%\Fileboat` on Windows, `~/Library/Application Support/Fileboat` on macOS, created if
/// missing: settings, tab and window state, favourites, icon themes.
///
/// `FILEBOAT_DATA_DIR` overrides it (tests and a portable copy can use their own folder).
pub fn data_dir() -> Result<PathBuf, String> {
    let dir = match std::env::var_os("FILEBOAT_DATA_DIR").filter(|v| !v.is_empty()) {
        Some(custom) => PathBuf::from(custom),
        None => dirs::config_dir().ok_or("Could not find the settings folder.")?.join("Fileboat"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// `%LOCALAPPDATA%\Fileboat` on Windows, `~/Library/Caches/Fileboat` on macOS, created if missing:
/// things that can be rebuilt (the search index, icon caches), kept out of the roaming profile.
///
/// With `FILEBOAT_DATA_DIR` set, it is the `cache` folder inside it.
pub fn cache_dir() -> Result<PathBuf, String> {
    let dir = match std::env::var_os("FILEBOAT_DATA_DIR").filter(|v| !v.is_empty()) {
        Some(custom) => PathBuf::from(custom).join("cache"),
        None => dirs::cache_dir().ok_or("Could not find the cache folder.")?.join("Fileboat"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Write to a temporary file, then rename over the target, so a crash mid-write
/// never leaves a half-written file.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Expand a leading `~` to the user's home folder.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~").filter(|r| r.is_empty() || r.starts_with(['/', '\\'])) {
        if let Some(home) = home_dir() {
            return home.join(rest.trim_start_matches(['/', '\\']));
        }
    }
    PathBuf::from(path)
}

/// The user's home folder: `%USERPROFILE%` on Windows, `$HOME` on macOS.
pub fn home_dir() -> Option<PathBuf> {
    let preferred = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let other = if cfg!(windows) { "HOME" } else { "USERPROFILE" };
    std::env::var_os(preferred).or_else(|| std::env::var_os(other)).map(PathBuf::from)
}

/// Seconds since 1970.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home_only_touches_a_leading_tilde() {
        let home = home_dir().expect("a home folder");
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/projects"), home.join("projects"));
        assert_eq!(expand_home("~\\projects"), home.join("projects"));
        assert_eq!(expand_home("~other/x"), PathBuf::from("~other/x"));
        assert_eq!(expand_home("a/~/b"), PathBuf::from("a/~/b"));
    }
}
