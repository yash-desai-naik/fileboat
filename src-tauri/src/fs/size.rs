//! A folder's total size: every file under it, read from directory listings only (no file is
//! opened). Links and junctions are never followed, so a loop can't trap the walk and nothing is
//! counted twice through one; cloud folders whose contents aren't on this computer are not
//! entered, so sizing never downloads anything.

use serde::Serialize;
use std::fs::Metadata;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeSize {
    pub bytes: u64,
    pub files: u64,
    pub folders: u64,
    /// Something wasn't counted: a folder that couldn't be read, or cloud contents not here.
    pub partial: bool,
}

/// Contents not on this computer: entering the folder would fetch its listing from the cloud.
#[cfg(windows)]
fn remote(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x4_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    meta.file_attributes() & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(not(windows))]
fn remote(_meta: &Metadata) -> bool {
    false
}

/// How often (in entries) `cancelled` is asked.
const CHECK_EVERY: u64 = 2048;

/// Add up everything under `root`. `cancelled` is asked now and then; when it says yes the walk
/// stops and returns None.
pub fn tree_size(root: &Path, mut cancelled: impl FnMut() -> bool) -> Option<TreeSize> {
    let mut total = TreeSize::default();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut seen: u64 = 0;
    while let Some(dir) = stack.pop() {
        let Ok(read) = crate::osfs::read_dir(&dir) else {
            total.partial = true;
            continue;
        };
        for item in read {
            seen += 1;
            if seen.is_multiple_of(CHECK_EVERY) && cancelled() {
                return None;
            }
            let Ok(item) = item else {
                total.partial = true;
                continue;
            };
            // The listing's own metadata (on Windows it comes with the listing): a link's is the
            // link's, so links count as nothing and are never followed.
            let Ok(meta) = item.metadata() else {
                total.partial = true;
                continue;
            };
            let kind = meta.file_type();
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                total.folders += 1;
                if remote(&meta) {
                    total.partial = true;
                } else {
                    stack.push(item.path());
                }
            } else if kind.is_file() {
                total.files += 1;
                total.bytes += meta.len();
            }
        }
    }
    if cancelled() { None } else { Some(total) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fileboat-size-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn adds_up_nested_files() {
        let root = tmp("nested");
        std::fs::write(root.join("a.txt"), vec![0u8; 100]).unwrap();
        std::fs::create_dir_all(root.join("x/y")).unwrap();
        std::fs::write(root.join("x/b.bin"), vec![0u8; 250]).unwrap();
        std::fs::write(root.join("x/y/c.bin"), vec![0u8; 50]).unwrap();
        let size = tree_size(&root, || false).unwrap();
        assert_eq!(size, TreeSize { bytes: 400, files: 3, folders: 2, partial: false });
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_folder_is_partial_and_cancelling_stops() {
        let root = tmp("missing");
        let size = tree_size(&root.join("nope"), || false).unwrap();
        assert!(size.partial && size.bytes == 0);
        assert_eq!(tree_size(&root, || true), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let root = tmp("links");
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::write(root.join("real/f"), vec![0u8; 10]).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("again")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("real/loop")).unwrap();
        let size = tree_size(&root, || false).unwrap();
        assert_eq!((size.bytes, size.files, size.folders), (10, 1, 1));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn junctions_are_not_followed() {
        let root = tmp("junction");
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::write(root.join("real/f"), vec![0u8; 10]).unwrap();
        // A junction back to the root: a loop if it were followed. mklink /J needs no privileges.
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("real").join("loop"))
            .arg(&root)
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(made, "mklink /J failed");
        let size = tree_size(&root, || false).unwrap();
        assert_eq!((size.bytes, size.files, size.folders), (10, 1, 1));
        let _ = std::fs::remove_dir(root.join("real").join("loop"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
