//! An archive's file list: zip (and the formats built on it: jar, apk, vsix, whl…) from its
//! central directory, without extracting anything; tar and tar.gz by reading through the headers.

use serde::Serialize;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveList {
    pub entries: Vec<ArchiveEntry>,
    /// Entries in the archive (when known: a tar that was cut short doesn't say).
    pub total: Option<u64>,
    /// Bytes once extracted (the listed entries).
    pub unpacked: u64,
    /// More entries than listed (`limit`, or a tar too big to read through quickly).
    pub truncated: bool,
}

/// How long reading through a big tar.gz may take before the list stops.
const TAR_TIME: Duration = Duration::from_secs(2);

/// List up to `limit` entries of the archive at `path`.
pub fn list_archive(path: &Path, limit: usize) -> Result<ArchiveList, String> {
    let lower = path.to_string_lossy().to_lowercase();
    let file = crate::osfs::open(path).map_err(|e| crate::fs::describe(path, &e))?;
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        list_tar(flate2::read::GzDecoder::new(BufReader::new(file)), limit)
    } else if lower.ends_with(".tar") {
        list_tar(BufReader::new(file), limit)
    } else {
        list_zip(file, limit)
    }
}

fn list_zip(file: std::fs::File, limit: usize) -> Result<ArchiveList, String> {
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| format!("Not a readable zip archive ({e})."))?;
    let total = zip.len();
    let mut entries = Vec::with_capacity(total.min(limit));
    let mut unpacked = 0;
    for i in 0..total.min(limit) {
        // The raw entry: its header only, never decompressed.
        let Ok(e) = zip.by_index_raw(i) else { continue };
        unpacked += e.size();
        entries.push(ArchiveEntry { name: e.name().map(|n| n.into_owned()).unwrap_or_default(), size: e.size(), dir: e.is_dir() });
    }
    Ok(ArchiveList { entries, total: Some(total as u64), unpacked, truncated: total > limit })
}

fn list_tar(reader: impl Read, limit: usize) -> Result<ArchiveList, String> {
    let mut archive = tar::Archive::new(reader);
    let started = Instant::now();
    let mut entries = Vec::new();
    let mut unpacked = 0;
    let mut truncated = false;
    for item in archive.entries().map_err(|e| format!("Not a readable tar archive ({e})."))? {
        if entries.len() >= limit || started.elapsed() > TAR_TIME {
            truncated = true;
            break;
        }
        let e = item.map_err(|e| format!("Not a readable tar archive ({e})."))?;
        let size = e.header().size().unwrap_or(0);
        let name = e.path().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = e.header().entry_type().is_dir();
        unpacked += size;
        entries.push(ArchiveEntry { name, size, dir });
    }
    let total = if truncated { None } else { Some(entries.len() as u64) };
    Ok(ArchiveList { entries, total, unpacked, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn zip_and_tar_gz_lists() {
        let dir = super::super::tests::tmp("archives");
        // A zip with a folder and two files.
        let zip_path = dir.join("a.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            w.add_directory("src/", opts).unwrap();
            w.start_file("src/main.rs", opts).unwrap();
            w.write_all(b"fn main() {}").unwrap();
            w.start_file("README.md", opts).unwrap();
            w.write_all(b"# hi").unwrap();
            w.finish().unwrap();
        }
        let list = list_archive(&zip_path, 100).unwrap();
        let names: Vec<&str> = list.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["src/", "src/main.rs", "README.md"]);
        assert_eq!((list.total, list.unpacked, list.truncated), (Some(3), 16, false));
        assert!(list_archive(&zip_path, 2).unwrap().truncated);

        // A tar.gz with one file.
        let tgz = dir.join("b.tar.gz");
        {
            let gz = flate2::write::GzEncoder::new(std::fs::File::create(&tgz).unwrap(), flate2::Compression::fast());
            let mut t = tar::Builder::new(gz);
            let mut h = tar::Header::new_gnu();
            h.set_size(5);
            h.set_cksum();
            t.append_data(&mut h, "notes.txt", &b"hello"[..]).unwrap();
            t.into_inner().unwrap().finish().unwrap();
        }
        let list = list_archive(&tgz, 100).unwrap();
        assert_eq!((list.entries[0].name.as_str(), list.entries[0].size, list.total), ("notes.txt", 5, Some(1)));

        std::fs::write(dir.join("bad.zip"), b"not a zip").unwrap();
        assert!(list_archive(&dir.join("bad.zip"), 10).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
