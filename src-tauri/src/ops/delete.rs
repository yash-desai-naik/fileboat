//! Deleting: to the Recycle Bin / Trash (the normal way), or permanently (asked first by the UI).

use std::path::{Path, PathBuf};

/// Move to the Recycle Bin (Windows) or the Trash (macOS). Fails as a whole with the system's
/// reason (for example a network drive with no Recycle Bin); nothing is deleted permanently here.
pub fn to_trash(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    trash::delete_all(paths).map_err(|e| {
        let bin = if cfg!(windows) { "the Recycle Bin" } else { "the Trash" };
        format!("Couldn't move to {bin}: {e}")
    })
}

/// Delete for good. Folders go with everything in them (links are removed, never followed), and
/// read-only files (git objects, for one) are made writable first, as Explorer does.
pub fn delete_permanently(paths: &[PathBuf]) -> Result<(), String> {
    for p in paths {
        remove(p).map_err(|(at, e)| {
            let name_of = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
            let name = name_of(p);
            // Inside a folder, name the item that couldn't go: "report.docx" in it is open in Word.
            let who = if at != *p { format!("\"{}\" in it is", name_of(&at)) } else { "it's".to_string() };
            if let Some(programs) = crate::osfs::in_use(&at, &e) {
                return format!("Couldn't delete \"{name}\": {who} open in {}.", crate::osfs::and_list(&programs));
            }
            match e.kind() {
                std::io::ErrorKind::PermissionDenied => format!("Couldn't delete \"{name}\": {who} in use by another program, or you don't have permission."),
                std::io::ErrorKind::NotFound if at == *p => format!("\"{name}\" doesn't exist (any more)."),
                _ => format!("Couldn't delete \"{name}\" ({}): {}.", name_of(&at), crate::osfs::explain(&at, &e)),
            }
        })?;
    }
    Ok(())
}

/// Remove `p` and everything in it; on failure, the item that couldn't be removed and why.
fn remove(p: &Path) -> Result<(), (PathBuf, std::io::Error)> {
    let here = |e| (p.to_path_buf(), e);
    let meta = crate::osfs::symlink_metadata(p).map_err(here)?;
    let ft = meta.file_type();
    if ft.is_symlink() {
        // A link to a folder (or a junction) is removed as a folder on Windows, a file elsewhere.
        return crate::osfs::remove_file(p).or_else(|_| crate::osfs::remove_dir(p)).map_err(here);
    }
    if ft.is_dir() {
        for child in crate::osfs::read_dir(p).map_err(here)? {
            remove(&child.map_err(here)?.path())?;
        }
        return retry_writable(p, &meta, |p| crate::osfs::remove_dir(p)).map_err(here);
    }
    retry_writable(p, &meta, |p| crate::osfs::remove_file(p)).map_err(here)
}

/// Run `op`; if it's refused and the item is read-only, make it writable and try once more.
fn retry_writable(p: &Path, meta: &std::fs::Metadata, op: impl Fn(&Path) -> std::io::Result<()>) -> std::io::Result<()> {
    match op(p) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && meta.permissions().readonly() => {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            crate::osfs::set_permissions(p, perms)?;
            op(p)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permanent_delete_handles_read_only_trees() {
        let d = std::env::temp_dir().join(format!("fileboat-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let objects = d.join("repo").join(".git").join("objects").join("ab");
        std::fs::create_dir_all(&objects).unwrap();
        let obj = objects.join("cdef");
        std::fs::write(&obj, "blob").unwrap();
        let mut perms = std::fs::metadata(&obj).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&obj, perms).unwrap();
        std::fs::write(d.join("loose.txt"), "x").unwrap();

        delete_permanently(&[d.join("repo"), d.join("loose.txt")]).unwrap();
        assert!(!d.join("repo").exists() && !d.join("loose.txt").exists());
        assert!(delete_permanently(&[d.join("gone")]).unwrap_err().contains("doesn't exist"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
