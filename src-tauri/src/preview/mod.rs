//! What the preview pane needs from a file: which viewer shows it (by name, then by content),
//! its text (any encoding, only the start of big files), bytes for the hex view, an archive's
//! file list, YAML and TOML as JSON for the tree, and a folder's summary.
//!
//! Nothing here opens a cloud file whose contents aren't on this computer: `info` says so first.

mod archive;
mod data;
mod text;

pub use archive::{list_archive, ArchiveEntry, ArchiveList};
pub use data::structured;
pub use text::{read_text, Text};

use serde::Serialize;
use std::io::Read;
use std::path::Path;

/// The viewer that shows a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Viewer {
    Text,
    Markdown,
    Json,
    Yaml,
    Toml,
    Csv,
    Image,
    /// Shown as an image (scripts never run), with its source a click away.
    Svg,
    Audio,
    Video,
    Pdf,
    Font,
    Archive,
    /// Anything else: the hex view.
    Binary,
    Folder,
    /// A cloud file whose contents aren't on this computer: not read, so not downloaded.
    Cloud,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub path: String,
    pub name: String,
    pub viewer: Viewer,
    /// For the media elements and the preview scheme ("" when not needed).
    pub mime: String,
    pub size: u64,
    /// Milliseconds since 1970 (0 when unknown).
    pub modified: f64,
    pub created: f64,
    pub accessed: f64,
    pub readonly: bool,
    pub hidden: bool,
    pub link: bool,
}

/// How much of a file is looked at to tell text from binary and to recognise formats.
const SNIFF: usize = 8192;

/// Extension → viewer and MIME type, for the formats the preview knows by name.
fn by_extension(name: &str) -> Option<(Viewer, &'static str)> {
    let lower = name.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return Some((Viewer::Archive, "application/gzip"));
    }
    if lower == "cargo.lock" || lower == "poetry.lock" || lower == "uv.lock" {
        return Some((Viewer::Toml, ""));
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e)?;
    use Viewer::*;
    Some(match ext {
        "md" | "markdown" | "mdx" => (Markdown, ""),
        "json" | "geojson" | "webmanifest" | "har" => (Json, ""),
        "yml" | "yaml" => (Yaml, ""),
        "toml" => (Toml, ""),
        "csv" => (Csv, ""),
        "tsv" => (Csv, ""),
        "png" | "apng" => (Image, "image/png"),
        "jpg" | "jpeg" | "jfif" => (Image, "image/jpeg"),
        "gif" => (Image, "image/gif"),
        "webp" => (Image, "image/webp"),
        "avif" => (Image, "image/avif"),
        "bmp" => (Image, "image/bmp"),
        "ico" => (Image, "image/x-icon"),
        "svg" => (Svg, "image/svg+xml"),
        "mp3" => (Audio, "audio/mpeg"),
        "wav" => (Audio, "audio/wav"),
        "ogg" | "oga" => (Audio, "audio/ogg"),
        "opus" => (Audio, "audio/ogg"),
        "flac" => (Audio, "audio/flac"),
        "m4a" => (Audio, "audio/mp4"),
        "aac" => (Audio, "audio/aac"),
        "weba" => (Audio, "audio/webm"),
        "mp4" | "m4v" => (Video, "video/mp4"),
        "webm" => (Video, "video/webm"),
        "mov" => (Video, "video/quicktime"),
        "ogv" => (Video, "video/ogg"),
        "pdf" => (Pdf, "application/pdf"),
        "ttf" => (Font, "font/ttf"),
        "otf" => (Font, "font/otf"),
        "woff" => (Font, "font/woff"),
        "woff2" => (Font, "font/woff2"),
        "zip" | "jar" | "war" | "ear" | "apk" | "aar" | "vsix" | "nupkg" | "whl" | "xpi" => (Archive, "application/zip"),
        "tar" => (Archive, "application/x-tar"),
        _ => return None,
    })
}

/// Text or not, from the first bytes: a UTF-16 byte order mark is text; a NUL byte, or many
/// control characters, is binary.
pub fn looks_like_text(head: &[u8]) -> bool {
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    if head.contains(&0) {
        return false;
    }
    let control = head.iter().filter(|&&b| b < 0x09 || ((0x0E..0x20).contains(&b) && b != 0x1B)).count();
    control * 10 <= head.len()
}

/// Recognised by content (a file without a known extension).
fn by_content(head: &[u8]) -> Option<(Viewer, &'static str)> {
    let kind = infer::get(head)?;
    let mime = kind.mime_type();
    use Viewer::*;
    let viewer = match kind.matcher_type() {
        infer::MatcherType::Image => Image,
        infer::MatcherType::Audio => Audio,
        infer::MatcherType::Video => Video,
        infer::MatcherType::Font => Font,
        _ => match mime {
            "application/pdf" => Pdf,
            "application/zip" | "application/x-tar" => Archive,
            _ => return None,
        },
    };
    Some((viewer, mime))
}

/// Contents not on this computer (a OneDrive or iCloud placeholder): reading would download it.
#[cfg(windows)]
pub fn not_downloaded(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x4_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    meta.file_attributes() & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(not(windows))]
pub fn not_downloaded(_meta: &std::fs::Metadata) -> bool {
    false
}

fn millis(t: std::io::Result<std::time::SystemTime>) -> f64 {
    t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

#[cfg(windows)]
fn is_hidden(_name: &str, meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x2 != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _meta: &std::fs::Metadata) -> bool {
    name.starts_with('.')
}

/// What a path is and which viewer shows it.
pub fn info(path: &Path) -> Result<Info, String> {
    let link_meta = std::fs::symlink_metadata(path).map_err(|e| crate::fs::describe(path, &e))?;
    let meta = std::fs::metadata(path).map_err(|e| crate::fs::describe(path, &e))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    let (viewer, mime) = if meta.is_dir() {
        (Viewer::Folder, "")
    } else if not_downloaded(&meta) {
        (Viewer::Cloud, "")
    } else {
        let mut head = Vec::with_capacity(SNIFF);
        if meta.len() > 0 {
            std::fs::File::open(path)
                .and_then(|f| f.take(SNIFF as u64).read_to_end(&mut head))
                .map_err(|e| crate::fs::describe(path, &e))?;
        }
        match by_extension(&name) {
            // Named like text but isn't (a ".txt" full of NULs): the hex view.
            Some((v, m)) if matches!(v, Viewer::Markdown | Viewer::Json | Viewer::Yaml | Viewer::Toml | Viewer::Csv) => {
                if looks_like_text(&head) { (v, m) } else { (Viewer::Binary, "") }
            }
            Some(found) => found,
            None if looks_like_text(&head) => (Viewer::Text, ""),
            None => by_content(&head).unwrap_or((Viewer::Binary, "")),
        }
    };
    Ok(Info {
        path: path.display().to_string(),
        viewer,
        mime: mime.to_string(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: millis(meta.modified()),
        created: millis(meta.created()),
        accessed: millis(meta.accessed()),
        readonly: meta.permissions().readonly(),
        hidden: is_hidden(&name, &link_meta),
        link: link_meta.file_type().is_symlink(),
        name,
    })
}

/// The MIME type to serve a file with (by name; "application/octet-stream" when unknown).
pub fn mime_of(path: &Path) -> &'static str {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match by_extension(&name) {
        Some((_, m)) if !m.is_empty() => m,
        _ => "application/octet-stream",
    }
}

/// Up to `len` bytes from `offset` (the hex view), at most 64 KB.
pub fn read_bytes(path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(|e| crate::fs::describe(path, &e))?;
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(len.min(65536));
    f.take(len.min(65536) as u64).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// A folder at a glance: what's in it and its README.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    pub files: u64,
    pub folders: u64,
    /// The README's name and text (Markdown or plain), if there is one.
    pub readme: Option<(String, String)>,
}

/// Count a folder's items and read its README (the first of README.md, README, README.txt…).
pub fn folder_summary(path: &Path) -> Result<FolderSummary, String> {
    let read = std::fs::read_dir(path).map_err(|e| crate::fs::describe(path, &e))?;
    let (mut files, mut folders) = (0, 0);
    let mut readmes: Vec<(u8, String)> = Vec::new();
    for item in read.flatten() {
        let is_dir = item.file_type().is_ok_and(|t| t.is_dir());
        if is_dir {
            folders += 1;
            continue;
        }
        files += 1;
        let name = item.file_name().to_string_lossy().into_owned();
        let rank = match name.to_lowercase().as_str() {
            "readme.md" => 0,
            "readme.markdown" => 1,
            "readme" => 2,
            "readme.txt" => 3,
            "readme.rst" => 4,
            _ => continue,
        };
        readmes.push((rank, name));
    }
    readmes.sort();
    let readme = readmes.into_iter().find_map(|(_, name)| {
        let p = path.join(&name);
        let meta = std::fs::metadata(&p).ok()?;
        if not_downloaded(&meta) {
            return None;
        }
        let text = read_text(&p, 256 * 1024).ok()?;
        Some((name, text.text))
    });
    Ok(FolderSummary { files, folders, readme })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fileboat-preview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn viewers_by_name_and_by_content() {
        let dir = tmp("viewers");
        let cases: Vec<(&str, Vec<u8>, Viewer)> = vec![
            ("notes.md", b"# Hi".to_vec(), Viewer::Markdown),
            ("main.rs", b"fn main() {}".to_vec(), Viewer::Text),
            ("Makefile", b"all:\n\techo hi\n".to_vec(), Viewer::Text),
            ("data.json", b"{}".to_vec(), Viewer::Json),
            ("Cargo.lock", b"version = 3".to_vec(), Viewer::Toml),
            ("fake.json", vec![0, 1, 2, 3, 0, 0], Viewer::Binary),
            ("photo", vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R'], Viewer::Image),
            ("blob.bin", vec![0, 159, 146, 150, 0, 1], Viewer::Binary),
            ("empty.txt", Vec::new(), Viewer::Text),
            ("logo.svg", b"<svg/>".to_vec(), Viewer::Svg),
            ("a.tar.gz", vec![0x1f, 0x8b], Viewer::Archive),
            ("utf16.txt", vec![0xFF, 0xFE, b'h', 0, b'i', 0], Viewer::Text),
        ];
        for (name, bytes, viewer) in cases {
            std::fs::write(dir.join(name), bytes).unwrap();
            assert_eq!(info(&dir.join(name)).unwrap().viewer, viewer, "{name}");
        }
        assert_eq!(info(&dir).unwrap().viewer, Viewer::Folder);
        assert_eq!(info(&dir.join("photo")).unwrap().mime, "image/png");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_summary_finds_the_readme() {
        let dir = tmp("summary");
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(dir.join("readme.txt"), "plain").unwrap();
        std::fs::write(dir.join("README.md"), "# Project").unwrap();
        std::fs::write(dir.join("main.rs"), "").unwrap();
        let s = folder_summary(&dir).unwrap();
        assert_eq!((s.files, s.folders), (3, 1));
        assert_eq!(s.readme, Some(("README.md".to_string(), "# Project".to_string())));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bytes_from_an_offset() {
        let dir = tmp("bytes");
        std::fs::write(dir.join("f"), b"0123456789").unwrap();
        assert_eq!(read_bytes(&dir.join("f"), 3, 4).unwrap(), b"3456");
        assert_eq!(read_bytes(&dir.join("f"), 8, 100).unwrap(), b"89");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
