//! One file or folder as Fileboat lists it: everything a view shows, read in one pass.

use serde::Serialize;
use std::fs::{DirEntry, Metadata};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Folder,
    File,
    /// A device, pipe, socket, or a link whose target is gone.
    Other,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    /// Bytes for files; for folders 0, or everything under them once sized (`sized`).
    pub size: u64,
    /// A folder whose `size` has been worked out.
    pub sized: bool,
    /// Milliseconds since 1970, or 0 when unknown.
    pub modified: f64,
    pub created: f64,
    /// Lower-case extension without the dot ("rs", "gz"); empty for folders and dotfiles like ".env".
    pub ext: String,
    /// Hidden by the system: the hidden attribute on Windows; a leading dot or the hidden flag on macOS.
    pub hidden: bool,
    /// A symbolic link or junction (shown with its target's kind).
    pub link: bool,
    pub readonly: bool,
    /// Lower-case name, for sorting and type-ahead (computed once).
    #[serde(skip)]
    pub key: Box<str>,
}

fn millis(t: std::io::Result<SystemTime>) -> f64 {
    t.ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// "archive.tar.gz" → "gz"; ".env" and "Makefile" → "".
pub fn extension_of(name: &str) -> String {
    match name.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) if i + 1 < name.len() => name[i + 1..].to_lowercase(),
        Some(_) => String::new(),
    }
}

#[cfg(windows)]
fn hidden(_name: &str, meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(target_os = "macos")]
fn hidden(name: &str, meta: &Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const UF_HIDDEN: u32 = 0x8000;
    name.starts_with('.') || meta.st_flags() & UF_HIDDEN != 0
}

#[cfg(not(any(windows, target_os = "macos")))]
fn hidden(name: &str, _meta: &Metadata) -> bool {
    name.starts_with('.')
}

impl Entry {
    /// From a name and its metadata. `link` metadata is the link itself; `target` is what it
    /// points to (None when the link is broken).
    fn build(name: String, link_meta: &Metadata, target: Option<&Metadata>) -> Entry {
        let link = link_meta.file_type().is_symlink();
        let meta = target.unwrap_or(link_meta);
        let kind = if link && target.is_none() {
            Kind::Other
        } else if meta.is_dir() {
            Kind::Folder
        } else if meta.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        let ext = if kind == Kind::Folder { String::new() } else { extension_of(&name) };
        Entry {
            key: name.to_lowercase().into_boxed_str(),
            kind,
            size: if kind == Kind::File { meta.len() } else { 0 },
            sized: false,
            modified: millis(meta.modified()),
            created: millis(meta.created()),
            ext,
            hidden: hidden(&name, link_meta),
            link,
            readonly: meta.permissions().readonly(),
            name,
        }
    }

    /// From a directory listing. On Windows the metadata comes with the listing (no extra
    /// system call per file), except for links, whose target is looked up.
    pub fn from_dir_entry(e: &DirEntry) -> Option<Entry> {
        let name = e.file_name().to_string_lossy().into_owned();
        // A directory entry's metadata is the link's own, never its target's.
        let meta = e.metadata().ok()?;
        if meta.file_type().is_symlink() {
            let target = std::fs::metadata(e.path()).ok();
            Some(Entry::build(name, &meta, target.as_ref()))
        } else {
            Some(Entry::build(name, &meta, Some(&meta)))
        }
    }

    /// From a path (a single changed file, reported by a file system event).
    pub fn from_path(path: &std::path::Path) -> Option<Entry> {
        let name = path.file_name()?.to_string_lossy().into_owned();
        let meta = std::fs::symlink_metadata(path).ok()?;
        if meta.file_type().is_symlink() {
            let target = std::fs::metadata(path).ok();
            Some(Entry::build(name, &meta, target.as_ref()))
        } else {
            Some(Entry::build(name, &meta, Some(&meta)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert_eq!(extension_of("main.RS"), "rs");
        assert_eq!(extension_of("archive.tar.gz"), "gz");
        assert_eq!(extension_of(".env"), "");
        assert_eq!(extension_of("Makefile"), "");
        assert_eq!(extension_of("trailing."), "");
        assert_eq!(extension_of(".config.json"), "json");
    }
}
