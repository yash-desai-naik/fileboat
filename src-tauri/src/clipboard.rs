//! Copy, cut and paste of files, shared with Explorer on Windows: the clipboard gets the file list
//! (CF_HDROP), whether it was a cut ("Preferred DropEffect", as Explorer writes it) and the paths
//! as text (pasting into a terminal or editor gives the paths). Files copied or cut in Explorer
//! paste into Fileboat the same way.
//!
//! macOS: the clipboard is Fileboat's own for now (Finder's pasteboard needs native code that
//! isn't written yet); the paths still go to the system clipboard as text.

use serde::Serialize;
use std::sync::Mutex;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contents {
    pub paths: Vec<String>,
    /// Cut (move on paste) rather than copy.
    pub cut: bool,
}

/// What Fileboat last put on the clipboard: used where the system clipboard can't hold files
/// (macOS for now), and to tell the views which items are cut (shown faded).
#[derive(Default)]
pub struct AppClipboard(pub Mutex<Contents>);

#[tauri::command]
pub fn clipboard_set(window: tauri::WebviewWindow, app_clip: tauri::State<'_, AppClipboard>, paths: Vec<String>, cut: bool) -> Result<(), String> {
    system::write(&window, &paths, cut)?;
    *app_clip.0.lock().unwrap() = Contents { paths, cut };
    Ok(())
}

/// The files on the clipboard (copied here or in Explorer), if any.
#[tauri::command]
pub fn clipboard_get(app_clip: tauri::State<'_, AppClipboard>) -> Contents {
    match system::read() {
        Some(c) => {
            // Copied elsewhere since: what Fileboat remembered no longer applies.
            let mut mine = app_clip.0.lock().unwrap();
            if mine.paths != c.paths {
                *mine = Contents::default();
            }
            c
        }
        None if cfg!(windows) => Contents::default(),
        None => app_clip.0.lock().unwrap().clone(),
    }
}

/// After a cut is pasted, the clipboard is emptied (as in Explorer: the files have moved).
#[tauri::command]
pub fn clipboard_clear_cut(app_clip: tauri::State<'_, AppClipboard>) {
    let mut mine = app_clip.0.lock().unwrap();
    if mine.cut {
        *mine = Contents::default();
        system::clear();
    }
}

#[cfg(windows)]
mod system {
    use super::Contents;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
    use windows::Win32::UI::Shell::{DROPFILES, DragQueryFileW, HDROP};
    use windows::core::w;

    const DROPEFFECT_COPY: u32 = 1;
    const DROPEFFECT_MOVE: u32 = 2;

    /// Another program may hold the clipboard for a moment: try a few times. Writing needs an owner
    /// window (without one, Explorer and other OLE readers don't see the files).
    fn open(owner: Option<HWND>) -> Result<(), String> {
        for _ in 0..10 {
            if unsafe { OpenClipboard(owner) }.is_ok() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Err("The clipboard is busy (another program is using it). Try again.".into())
    }

    /// Copy `bytes` into a movable global block for the clipboard.
    fn global(bytes: &[u8]) -> Result<HGLOBAL, String> {
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|e| e.to_string())?;
            let p = GlobalLock(h) as *mut u8;
            if p.is_null() {
                return Err("Out of memory for the clipboard.".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            let _ = GlobalUnlock(h);
            Ok(h)
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    pub fn write(window: &tauri::WebviewWindow, paths: &[String], cut: bool) -> Result<(), String> {
        // DROPFILES header, then the paths as UTF-16, each ending in 0, the list ending in another 0.
        let header = DROPFILES { pFiles: std::mem::size_of::<DROPFILES>() as u32, fWide: true.into(), ..Default::default() };
        let mut bytes: Vec<u8> = unsafe {
            std::slice::from_raw_parts(&header as *const DROPFILES as *const u8, std::mem::size_of::<DROPFILES>()).to_vec()
        };
        for p in paths {
            for u in wide(p).into_iter().chain([0]) {
                bytes.extend(u.to_le_bytes());
            }
        }
        bytes.extend([0, 0]);
        let effect = if cut { DROPEFFECT_MOVE } else { DROPEFFECT_COPY };
        let text: Vec<u8> = wide(&paths.join("\r\n")).into_iter().chain([0]).flat_map(|u| u.to_le_bytes()).collect();

        open(window.hwnd().ok())?;
        let result = (|| unsafe {
            EmptyClipboard().map_err(|e| e.to_string())?;
            SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(global(&bytes)?.0))).map_err(|e| e.to_string())?;
            let fmt = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            SetClipboardData(fmt, Some(HANDLE(global(&effect.to_le_bytes())?.0))).map_err(|e| e.to_string())?;
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(global(&text)?.0))).map_err(|e| e.to_string())?;
            Ok(())
        })();
        let _ = unsafe { CloseClipboard() };
        result
    }

    pub fn read() -> Option<Contents> {
        if unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32) }.is_err() {
            return None;
        }
        open(None).ok()?;
        let contents = unsafe {
            let mut paths = Vec::new();
            if let Ok(h) = GetClipboardData(CF_HDROP.0 as u32) {
                let drop = HDROP(h.0);
                let count = DragQueryFileW(drop, u32::MAX, None);
                for i in 0..count {
                    let len = DragQueryFileW(drop, i, None) as usize;
                    let mut buf = vec![0u16; len + 1];
                    DragQueryFileW(drop, i, Some(&mut buf));
                    paths.push(String::from_utf16_lossy(&buf[..len]));
                }
            }
            let fmt = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            let mut cut = false;
            if let Ok(h) = GetClipboardData(fmt) {
                let g = HGLOBAL(h.0);
                let p = GlobalLock(g) as *const u8;
                if !p.is_null() && GlobalSize(g) >= 4 {
                    let effect = u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]);
                    cut = effect & DROPEFFECT_MOVE != 0;
                }
                let _ = GlobalUnlock(g);
            }
            Contents { paths, cut }
        };
        let _ = unsafe { CloseClipboard() };
        Some(contents).filter(|c| !c.paths.is_empty())
    }

    pub fn clear() {
        if open(None).is_ok() {
            let _ = unsafe { EmptyClipboard() };
            let _ = unsafe { CloseClipboard() };
        }
    }
}

#[cfg(not(windows))]
mod system {
    use super::Contents;

    /// The paths as text on the system clipboard (pbcopy), so they paste into a terminal.
    pub fn write(_window: &tauri::WebviewWindow, paths: &[String], _cut: bool) -> Result<(), String> {
        use std::io::Write;
        let mut child = std::process::Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        child.stdin.take().ok_or("No clipboard.")?.write_all(paths.join("\n").as_bytes()).map_err(|e| e.to_string())?;
        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Finder's file pasteboard isn't read yet: Fileboat's own clipboard is used.
    pub fn read() -> Option<Contents> {
        None
    }

    pub fn clear() {}
}
