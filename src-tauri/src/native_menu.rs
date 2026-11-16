//! "Show more options": the real Explorer context menu for files or a folder's background, with
//! everything shell extensions add (7-Zip, TortoiseGit, antivirus, Send to…). Windows only.

/// Show the Explorer menu at the mouse for `paths` (all in one folder), or for `folder`'s
/// background when `paths` is empty, and run what's chosen.
#[tauri::command]
pub fn native_menu(window: tauri::WebviewWindow, paths: Vec<String>, folder: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        // Menus belong to the window's own thread.
        window
            .run_on_main_thread(move || {
                if let Err(e) = imp::show(hwnd, &paths, &folder) {
                    eprintln!("native menu: {e}");
                }
            })
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = (window, paths, folder);
        Err("The system context menu is only available on Windows.".into())
    }
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        CMF_EXTENDEDVERBS, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX, DefSubclassProc,
        IContextMenu, IContextMenu3, IShellFolder, RemoveWindowSubclass, SEE_MASK_UNICODE, SHBindToParent, SHParseDisplayName, SetWindowSubclass,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, GetCursorPos, SW_SHOWNORMAL, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_DRAWITEM, WM_INITMENUPOPUP,
        WM_MEASUREITEM, WM_MENUCHAR,
    };
    use windows::core::{HSTRING, Interface, PCSTR, PCWSTR};

    const FIRST_ID: u32 = 1;
    const LAST_ID: u32 = 0x7FFF;
    const SUBCLASS_ID: usize = 0xF1EB;

    thread_local! {
        /// The menu being shown, for drawing its owner-drawn items and submenus.
        static ACTIVE: RefCell<Option<IContextMenu3>> = const { RefCell::new(None) };
    }

    /// While the popup is open, the window passes these messages to the shell's menu (icons,
    /// "Send to" and other submenus are filled and drawn by it).
    unsafe extern "system" fn forward(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
            let handled = ACTIVE.with(|a| {
                a.borrow().as_ref().and_then(|cm| {
                    let mut result = LRESULT(0);
                    unsafe { cm.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result)) }.ok().map(|_| result)
                })
            });
            if let Some(result) = handled {
                return result;
            }
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// An absolute item list for a path (freed with CoTaskMemFree).
    fn pidl(path: &str) -> Result<*mut ITEMIDLIST, String> {
        let mut p: *mut ITEMIDLIST = std::ptr::null_mut();
        unsafe { SHParseDisplayName(&HSTRING::from(path), None, &mut p, 0, None) }.map_err(|e| format!("{path}: {e}"))?;
        Ok(p)
    }

    /// The context menu of items (all in one folder), or of a folder's background.
    fn context_menu(hwnd: HWND, paths: &[String], folder: &str) -> Result<(IContextMenu, Vec<*mut ITEMIDLIST>), String> {
        let mut owned = Vec::new();
        if paths.is_empty() {
            // The background: the folder's own view object.
            let p = pidl(folder)?;
            owned.push(p);
            let mut last: *mut ITEMIDLIST = std::ptr::null_mut();
            let parent: IShellFolder = unsafe { SHBindToParent(p, Some(&mut last)) }.map_err(|e| e.to_string())?;
            let shell_folder: IShellFolder = unsafe { parent.BindToObject(last, None) }.map_err(|e| e.to_string())?;
            let cm: IContextMenu = unsafe { shell_folder.CreateViewObject(hwnd) }.map_err(|e| e.to_string())?;
            return Ok((cm, owned));
        }
        let mut parent: Option<IShellFolder> = None;
        let mut children: Vec<*const ITEMIDLIST> = Vec::new();
        for path in paths {
            let p = pidl(path)?;
            owned.push(p);
            let mut last: *mut ITEMIDLIST = std::ptr::null_mut();
            let f: IShellFolder = unsafe { SHBindToParent(p, Some(&mut last)) }.map_err(|e| e.to_string())?;
            parent.get_or_insert(f);
            children.push(last);
        }
        let parent = parent.ok_or("Nothing to show a menu for.")?;
        let cm: IContextMenu = unsafe { parent.GetUIObjectOf(hwnd, &children, None) }.map_err(|e| e.to_string())?;
        Ok((cm, owned))
    }

    pub fn show(hwnd: isize, paths: &[String], folder: &str) -> Result<(), String> {
        let hwnd = HWND(hwnd as *mut _);
        let (cm, owned) = context_menu(hwnd, paths, folder)?;
        let result = (|| {
            let menu = unsafe { CreatePopupMenu() }.map_err(|e| e.to_string())?;
            // Shift shows the extended verbs (Copy as path, Open PowerShell here…), as in Explorer.
            let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
            let flags = CMF_NORMAL | if shift { CMF_EXTENDEDVERBS } else { 0 };
            unsafe { cm.QueryContextMenu(menu, 0, FIRST_ID, LAST_ID, flags) }.ok().map_err(|e| e.to_string())?;
            let mut pt = POINT::default();
            unsafe { GetCursorPos(&mut pt) }.map_err(|e| e.to_string())?;

            // A popup menu only shows (and closes on an outside click) for the foreground window.
            let _ = unsafe { SetForegroundWindow(hwnd) };
            ACTIVE.with(|a| *a.borrow_mut() = cm.cast::<IContextMenu3>().ok());
            let subclassed = unsafe { SetWindowSubclass(hwnd, Some(forward), SUBCLASS_ID, 0) }.as_bool();
            let chosen = unsafe { TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, pt.x, pt.y, hwnd, None) }.0 as u32;
            if subclassed {
                let _ = unsafe { RemoveWindowSubclass(hwnd, Some(forward), SUBCLASS_ID) };
            }
            ACTIVE.with(|a| *a.borrow_mut() = None);

            if chosen >= FIRST_ID {
                let offset = (chosen - FIRST_ID) as usize;
                let info = CMINVOKECOMMANDINFOEX {
                    cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                    // CMIC_MASK_UNICODE has the same value as SEE_MASK_UNICODE.
                    fMask: SEE_MASK_UNICODE | CMIC_MASK_PTINVOKE,
                    hwnd,
                    // The command by number (MAKEINTRESOURCE), not by verb name.
                    lpVerb: PCSTR(offset as *const u8),
                    lpVerbW: PCWSTR(offset as *const u16),
                    nShow: SW_SHOWNORMAL.0,
                    ptInvoke: pt,
                    ..Default::default()
                };
                let r = unsafe { cm.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO) };
                let _ = unsafe { DestroyMenu(menu) };
                r.map_err(|e| e.to_string())?;
            } else {
                let _ = unsafe { DestroyMenu(menu) };
            }
            Ok(())
        })();
        for p in owned {
            unsafe { CoTaskMemFree(Some(p as *const _)) };
        }
        result
    }
}
