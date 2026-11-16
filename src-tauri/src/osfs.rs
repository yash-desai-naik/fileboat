//! `std::fs` for paths the user's files can have: each path goes through [`os`] first, so names
//! Windows' ordinary API rewrites or refuses (a trailing dot or space, a device name like
//! `con.txt`, made by WSL, git or a Mac) are read, copied, renamed and deleted like any other.
//! On macOS these are `std::fs` itself.

use std::borrow::Cow;
use std::fs::{File, Metadata, OpenOptions, Permissions, ReadDir};
use std::io;
use std::path::Path;

/// Device names Windows reserves, with or without an extension ("con", "nul.txt").
pub const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A name Windows' ordinary API can't reach as it is.
pub fn awkward_name(name: &str) -> bool {
    if name == "." || name == ".." {
        return false;
    }
    if name.ends_with(['.', ' ']) {
        return true;
    }
    let stem = name.split('.').next().unwrap_or("").trim_end().to_lowercase();
    RESERVED.contains(&stem.as_str())
}

/// The form of `path` to hand to the OS. On Windows, an absolute path with an awkward name in it
/// becomes a `\\?\` path (`\\?\UNC\server\share\…` for a share), which Windows takes literally;
/// every other path is returned as it is.
#[cfg(windows)]
pub fn os(path: &Path) -> Cow<'_, Path> {
    use std::path::{Component, Prefix};
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with(r"\\?\") || !path.is_absolute() {
        return Cow::Borrowed(path);
    }
    if !path.components().any(|c| matches!(c, Component::Normal(n) if awkward_name(&n.to_string_lossy()))) {
        return Cow::Borrowed(path);
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut parts: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => match p.kind() {
                Prefix::Disk(d) => out = format!(r"\\?\{}:", d as char),
                Prefix::UNC(server, share) => out = format!(r"\\?\UNC\{}\{}", server.to_string_lossy(), share.to_string_lossy()),
                _ => return Cow::Borrowed(path),
            },
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(n) => parts.push(n.to_string_lossy().into_owned()),
        }
    }
    for p in parts {
        out.push('\\');
        out.push_str(&p);
    }
    if out.ends_with(':') {
        out.push('\\');
    }
    Cow::Owned(out.into())
}

#[cfg(not(windows))]
pub fn os(path: &Path) -> Cow<'_, Path> {
    Cow::Borrowed(path)
}

/// When `e` says a file is in use, the programs that have it open, as Windows knows them
/// ("Microsoft Word", "Notepad"), else None. Folders aren't tracked by Windows this way.
#[cfg(windows)]
pub fn in_use(path: &Path, e: &io::Error) -> Option<Vec<String>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::RestartManager::{RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY, RM_PROCESS_INFO};
    use windows::core::{PCWSTR, PWSTR};
    // Sharing violation, lock violation, or access denied (what a delete of an open file gets).
    if !matches!(e.raw_os_error(), Some(32 | 33 | 5)) || std::fs::symlink_metadata(os(path)).is_ok_and(|m| m.is_dir()) {
        return None;
    }
    let wide: Vec<u16> = os(path).as_os_str().encode_wide().chain(Some(0)).collect();
    let mut session = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    unsafe {
        if RmStartSession(&mut session, None, PWSTR(key.as_mut_ptr())) != ERROR_SUCCESS {
            return None;
        }
        let mut names = Vec::new();
        if RmRegisterResources(session, Some(&[PCWSTR(wide.as_ptr())]), None, None) == ERROR_SUCCESS {
            let mut infos = vec![RM_PROCESS_INFO::default(); 16];
            let (mut needed, mut count, mut reasons) = (0u32, infos.len() as u32, 0u32);
            if RmGetList(session, &mut needed, &mut count, Some(infos.as_mut_ptr()), &mut reasons) == ERROR_SUCCESS {
                for info in &infos[..count as usize] {
                    let end = info.strAppName.iter().position(|&c| c == 0).unwrap_or(info.strAppName.len());
                    let name = String::from_utf16_lossy(&info.strAppName[..end]);
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        let _ = RmEndSession(session);
        (!names.is_empty()).then_some(names)
    }
}

#[cfg(not(windows))]
pub fn in_use(_path: &Path, _e: &io::Error) -> Option<Vec<String>> {
    None
}

/// "Word", "Word and Notepad", "Word, Excel and Notepad".
pub fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Why `e` happened to `path`, in words: the programs that have it open when Windows knows,
/// "it's open in another program" for a sharing violation, else the system's message.
pub fn explain(path: &Path, e: &io::Error) -> String {
    match in_use(path, e) {
        Some(programs) => format!("it's open in {}", and_list(&programs)),
        None if matches!(e.raw_os_error(), Some(32 | 33)) && cfg!(windows) => "it's open in another program".into(),
        None => e.to_string(),
    }
}

pub fn metadata<P: AsRef<Path>>(p: P) -> io::Result<Metadata> {
    std::fs::metadata(os(p.as_ref()))
}
pub fn symlink_metadata<P: AsRef<Path>>(p: P) -> io::Result<Metadata> {
    std::fs::symlink_metadata(os(p.as_ref()))
}
pub fn read_dir<P: AsRef<Path>>(p: P) -> io::Result<ReadDir> {
    std::fs::read_dir(os(p.as_ref()))
}
pub fn open<P: AsRef<Path>>(p: P) -> io::Result<File> {
    File::open(os(p.as_ref()))
}
pub fn create<P: AsRef<Path>>(p: P) -> io::Result<File> {
    File::create(os(p.as_ref()))
}
pub fn open_with<P: AsRef<Path>>(options: &OpenOptions, p: P) -> io::Result<File> {
    options.open(os(p.as_ref()))
}
pub fn read<P: AsRef<Path>>(p: P) -> io::Result<Vec<u8>> {
    std::fs::read(os(p.as_ref()))
}
pub fn read_to_string<P: AsRef<Path>>(p: P) -> io::Result<String> {
    std::fs::read_to_string(os(p.as_ref()))
}
pub fn write<P: AsRef<Path>>(p: P, contents: impl AsRef<[u8]>) -> io::Result<()> {
    std::fs::write(os(p.as_ref()), contents)
}
pub fn create_dir<P: AsRef<Path>>(p: P) -> io::Result<()> {
    std::fs::create_dir(os(p.as_ref()))
}
pub fn create_dir_all<P: AsRef<Path>>(p: P) -> io::Result<()> {
    std::fs::create_dir_all(os(p.as_ref()))
}
pub fn remove_file<P: AsRef<Path>>(p: P) -> io::Result<()> {
    std::fs::remove_file(os(p.as_ref()))
}
pub fn remove_dir<P: AsRef<Path>>(p: P) -> io::Result<()> {
    std::fs::remove_dir(os(p.as_ref()))
}
pub fn remove_dir_all<P: AsRef<Path>>(p: P) -> io::Result<()> {
    std::fs::remove_dir_all(os(p.as_ref()))
}
pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    std::fs::rename(os(from.as_ref()), os(to.as_ref()))
}
pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64> {
    std::fs::copy(os(from.as_ref()), os(to.as_ref()))
}
pub fn set_permissions<P: AsRef<Path>>(p: P, perm: Permissions) -> io::Result<()> {
    std::fs::set_permissions(os(p.as_ref()), perm)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn only_awkward_paths_change() {
        assert_eq!(os(Path::new(r"C:\a\b.txt")), Path::new(r"C:\a\b.txt"));
        assert_eq!(os(Path::new(r"rel\dot.")), Path::new(r"rel\dot."));
        assert_eq!(os(Path::new(r"C:\a\dot.")).as_os_str(), r"\\?\C:\a\dot.");
        assert_eq!(os(Path::new(r"C:/a/./x/../con.txt")).as_os_str(), r"\\?\C:\a\con.txt");
        assert_eq!(os(Path::new(r"\\srv\share\d\space ")).as_os_str(), r"\\?\UNC\srv\share\d\space ");
        assert_eq!(os(Path::new(r"\\?\C:\a\dot.")).as_os_str(), r"\\?\C:\a\dot.");
        assert!(!awkward_name("..") && !awkward_name("console.log") && awkward_name("NUL") && awkward_name("aux.c"));
    }
}
