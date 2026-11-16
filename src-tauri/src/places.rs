//! Places for the sidebar: the user's home folders and the drives (volumes) with their space.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub path: String,
    /// "home", "desktop", "documents", "downloads", "pictures", "music", "videos"
    pub kind: &'static str,
}

/// The home folder and its usual subfolders, those that exist.
pub fn home_places() -> Vec<Place> {
    let list: [(&str, &'static str, Option<PathBuf>); 7] = [
        ("Home", "home", crate::paths::home_dir()),
        ("Desktop", "desktop", dirs::desktop_dir()),
        ("Documents", "documents", dirs::document_dir()),
        ("Downloads", "downloads", dirs::download_dir()),
        ("Pictures", "pictures", dirs::picture_dir()),
        ("Music", "music", dirs::audio_dir()),
        ("Videos", "videos", dirs::video_dir()),
    ];
    list.into_iter()
        .filter_map(|(name, kind, path)| {
            let path = path.filter(|p| p.is_dir())?;
            Some(Place { name: name.into(), path: path.display().to_string(), kind })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    /// "C:\" or "/Volumes/USB".
    pub path: String,
    /// The volume's label ("Data"), or a description ("Local Disk") when it has none.
    pub label: String,
    /// "fixed", "removable", "network", "optical" or "other".
    pub kind: &'static str,
    /// Bytes, when the drive could tell in time (an empty card reader or an unreachable network
    /// drive can't).
    pub free: Option<u64>,
    pub total: Option<u64>,
}

/// The drives, quickly: their roots, kinds and labels. Free space is asked separately
/// (`space`), because a network drive whose server is off can take half a minute to answer.
#[cfg(windows)]
pub fn drives() -> Vec<Drive> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW};
    use windows::core::{HSTRING, PCWSTR};
    // GetDriveTypeW results.
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;

    let mut buf = [0u16; 512];
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
    let roots: Vec<String> = String::from_utf16_lossy(&buf[..n.min(buf.len())])
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    roots
        .into_iter()
        .map(|root| {
            let wide = HSTRING::from(root.as_str());
            let kind = match unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) } {
                DRIVE_FIXED => "fixed",
                DRIVE_REMOVABLE => "removable",
                DRIVE_REMOTE => "network",
                DRIVE_CDROM => "optical",
                _ => "other",
            };
            let label = if kind == "network" {
                // From the drive mapping: no need to reach the server.
                network_share(&root).map(|unc| share_label(&unc)).unwrap_or_default()
            } else {
                let mut label = [0u16; 261];
                let named = unsafe { GetVolumeInformationW(PCWSTR(wide.as_ptr()), Some(&mut label), None, None, None, None) }.is_ok();
                if named { String::from_utf16_lossy(&label).trim_end_matches('\0').to_string() } else { String::new() }
            };
            let fallback = match kind {
                "fixed" => "Local Disk",
                "removable" => "Removable Disk",
                "network" => "Network Drive",
                "optical" => "CD/DVD Drive",
                _ => "Drive",
            };
            Drive { path: root, label: if label.is_empty() { fallback.into() } else { label }, kind, free: None, total: None }
        })
        .collect()
}

/// The UNC path a mapped drive points to ("\\server\share"), from Windows' own record of it.
#[cfg(windows)]
fn network_share(root: &str) -> Option<String> {
    use windows::Win32::NetworkManagement::WNet::WNetGetConnectionW;
    use windows::core::{HSTRING, PWSTR};
    let local = HSTRING::from(root.trim_end_matches('\\'));
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let r = unsafe { WNetGetConnectionW(&local, Some(PWSTR(buf.as_mut_ptr())), &mut len) };
    if r.0 != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// "\\192.168.1.6\work" → "work on 192.168.1.6".
#[cfg(windows)]
fn share_label(unc: &str) -> String {
    let mut parts = unc.trim_start_matches('\\').split('\\');
    match (parts.next(), parts.next()) {
        (Some(server), Some(share)) if !share.is_empty() => format!("{share} on {server}"),
        _ => unc.to_string(),
    }
}

/// Free and total bytes of the volume holding `root` (may block on a network drive: call it
/// off the main thread, with a time limit).
#[cfg(windows)]
pub fn space(root: &str) -> Option<(u64, u64)> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::{HSTRING, PCWSTR};
    let wide = HSTRING::from(root);
    let (mut free, mut total) = (0u64, 0u64);
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut free), Some(&mut total), None) }.ok()?;
    Some((free, total))
}

#[cfg(unix)]
pub fn drives() -> Vec<Drive> {
    let mut out = vec![Drive {
        path: "/".into(),
        label: if cfg!(target_os = "macos") { "Macintosh HD".into() } else { "/".into() },
        kind: "fixed",
        free: None,
        total: None,
    }];
    // macOS lists every mounted volume in /Volumes; the start-up disk appears there as a link to "/".
    if let Ok(read) = std::fs::read_dir("/Volumes") {
        for e in read.flatten() {
            let path = e.path();
            if std::fs::read_link(&path).is_ok() {
                continue;
            }
            out.push(Drive {
                label: e.file_name().to_string_lossy().into_owned(),
                path: path.display().to_string(),
                kind: "removable",
                free: None,
                total: None,
            });
        }
    }
    out
}

/// Free and total bytes of the volume holding `root` (statvfs).
#[cfg(unix)]
pub fn space(root: &str) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(std::ffi::OsStr::new(root).as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let unit = s.f_frsize as u64;
    Some((s.f_bavail as u64 * unit, s.f_blocks as u64 * unit))
}

#[cfg(test)]
mod tests {
    #[test]
    fn home_and_drives() {
        let home = super::home_places();
        assert_eq!(home.first().map(|p| p.kind), Some("home"));
        let drives = super::drives();
        assert!(!drives.is_empty());
        let first = &drives[0];
        assert!(!first.label.is_empty());
        // The first drive is local (C:\ or /): it answers at once.
        let (free, total) = super::space(&first.path).expect("free space of the first drive");
        assert!(free <= total && total > 0);
    }

    #[cfg(windows)]
    #[test]
    fn share_labels() {
        assert_eq!(super::share_label(r"\\192.168.1.6\work"), "work on 192.168.1.6");
        assert_eq!(super::share_label(r"\\nas"), r"\\nas");
    }
}
