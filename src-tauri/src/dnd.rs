//! Drag and drop. Dragging rows out starts a system drag with the files, so they drop into
//! Explorer, editors, browsers and chat apps (as a copy). Anything dropped on Fileboat (from those
//! apps, the desktop, or Fileboat's own drags) arrives as a system file drop; the page works out
//! the folder under the cursor and asks `job_drop` to copy or move there.

use crate::jobs::{self, Jobs};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

/// What the drag shows under the cursor.
const DRAG_IMAGE: &[u8] = include_bytes!("../icons/64x64.png");

/// Start dragging `paths` out of the window. Runs on the main thread (Windows needs the window's
/// own thread for a drag) and returns once it has started.
#[tauri::command]
pub fn drag_start(window: tauri::WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    let files: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).filter(|p| p.exists()).collect();
    if files.is_empty() {
        return Ok(());
    }
    let w = window.clone();
    window
        .run_on_main_thread(move || {
            // Other apps get a copy: a drag out of Fileboat never moves files by surprise.
            let options = drag::Options { skip_animatation_on_cancel_or_failure: true, mode: drag::DragMode::Copy };
            let _ = drag::start_drag(&w, drag::DragItem::Files(files), drag::Image::Raw(DRAG_IMAGE.to_vec()), |_, _| {}, options);
        })
        .map_err(|e| e.to_string())
}

/// Ctrl (copy) and Shift (move) held right now: a page gets no key events during a system drag.
#[cfg(windows)]
fn modifiers() -> (bool, bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};
    let down = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0;
    (down(VK_CONTROL.0), down(VK_SHIFT.0))
}

#[cfg(not(windows))]
fn modifiers() -> (bool, bool) {
    (false, false)
}

/// Are both paths on the same drive (where a move is just a rename)?
fn same_volume(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        // The drive or network share: "C:" or "\\server\share".
        let root = |p: &Path| match p.components().next() {
            Some(std::path::Component::Prefix(prefix)) => prefix.as_os_str().to_string_lossy().to_lowercase(),
            _ => String::new(),
        };
        root(a) == root(b) && !root(a).is_empty()
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev(),
            _ => false,
        }
    }
}

/// Files dropped on a folder: moved if they're on the same drive, copied from another drive, as
/// Explorer does; holding Ctrl copies and Shift moves.
#[tauri::command]
pub fn job_drop(app: AppHandle, jobs: State<'_, Jobs>, sources: Vec<String>, dest: String) -> Result<String, String> {
    let dest = fileboat_core::fs::resolve(&dest, None);
    let (ctrl, shift) = modifiers();
    let same = sources.first().is_some_and(|s| same_volume(Path::new(s), &dest));
    let mode = if ctrl {
        "copy"
    } else if shift || same {
        "move"
    } else {
        "copy"
    };
    jobs::job_transfer(app, jobs, sources, dest.display().to_string(), mode.into())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn volumes() {
        use std::path::Path;
        assert!(super::same_volume(Path::new("C:\\a\\b"), Path::new("c:\\x")));
        assert!(!super::same_volume(Path::new("C:\\a"), Path::new("D:\\a")));
        assert!(super::same_volume(Path::new("\\\\nas\\share\\a"), Path::new("\\\\NAS\\share\\b")));
        assert!(!super::same_volume(Path::new("\\\\nas\\one\\a"), Path::new("\\\\nas\\two\\b")));
    }
}
