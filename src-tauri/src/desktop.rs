//! Fileboat's own folders, for the About box, and opening things with the desktop.

use fileboat_core::paths::{cache_dir, data_dir};

/// Open a file, folder or web address with the desktop: Explorer, Finder or xdg-open.
pub fn open_with_desktop(arg: &std::path::Path) -> Result<(), String> {
    let program = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(arg).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Where Fileboat keeps things, for the About box.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    /// Settings, tab and window state.
    data_dir: String,
    /// Things that can be rebuilt: the search index, icon caches.
    cache_dir: String,
    /// The running program.
    exe: String,
}

#[tauri::command]
pub fn app_paths() -> Result<AppPaths, String> {
    Ok(AppPaths {
        data_dir: data_dir()?.display().to_string(),
        cache_dir: cache_dir()?.display().to_string(),
        exe: std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
    })
}

/// Open one of Fileboat's folders in the file manager ("data" or "cache").
#[tauri::command]
pub fn open_app_folder(which: String) -> Result<(), String> {
    let dir = match which.as_str() {
        "data" => data_dir()?,
        "cache" => cache_dir()?,
        _ => return Err("Unknown folder.".into()),
    };
    open_with_desktop(&dir)
}

/// Fileboat's releases on GitHub, in the default browser (a fixed address, nothing from the page).
#[tauri::command]
pub fn open_releases_page() -> Result<(), String> {
    open_with_desktop(std::path::Path::new("https://github.com/yash-desai-naik/fileboat/releases"))
}

/// Open a link from the preview (a Markdown link) in the browser or mail app: http, https and
/// mailto only, never a program or a file.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|_| "That isn't a link.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Err("Only web and mail links open from the preview.".into());
    }
    open_with_desktop(std::path::Path::new(parsed.as_str()))
}

/// Open a file with its default app (or run it, for programs), as a double-click in Explorer or
/// Finder does. On Windows a type with no app gets the system's "Open with" dialog.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    if !path.exists() {
        return Err(format!("{} doesn't exist.", path.display()));
    }
    shell_open(&path)
}

#[cfg(windows)]
fn shell_open(path: &std::path::Path) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PCWSTR, w};
    // ShellExecute returns a value above 32 on success; 31 means no app is registered.
    const SE_ERR_NOASSOC: isize = 31;
    let file = HSTRING::from(path.as_os_str());
    let dir = path.parent().map(|p| HSTRING::from(p.as_os_str())).unwrap_or_default();
    let run = |verb: PCWSTR| unsafe { ShellExecuteW(None, verb, &file, PCWSTR::null(), &dir, SW_SHOWNORMAL).0 as isize };
    match run(w!("open")) {
        r if r > 32 => Ok(()),
        SE_ERR_NOASSOC if run(w!("openas")) > 32 => Ok(()),
        r => Err(format!("Windows couldn't open {} (error {r}).", path.display())),
    }
}

#[cfg(not(windows))]
fn shell_open(path: &std::path::Path) -> Result<(), String> {
    let status = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
        .arg(path)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("Couldn't open {}.", path.display())) }
}

/// Show an item in Explorer (selected) or Finder (revealed).
#[tauri::command]
pub fn reveal_in_file_manager(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        // `/select,` must be glued to the path in one argument, as Explorer parses it itself.
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{path}\""))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("open").arg("-R").arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// The system's Properties dialog (Windows) or Get Info window (macOS) for an item.
#[tauri::command]
pub fn show_properties(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::{SHOP_FILEPATH, SHObjectProperties};
        use windows::core::{HSTRING, PCWSTR};
        let ok = unsafe { SHObjectProperties(None, SHOP_FILEPATH, &HSTRING::from(path.as_str()), PCWSTR::null()) };
        if ok.as_bool() { Ok(()) } else { Err(format!("Couldn't show the properties of {path}.")) }
    }
    #[cfg(not(windows))]
    {
        let script = format!(
            "tell application \"Finder\" to open information window of (POSIX file \"{}\" as alias)",
            path.replace('\\', "\\\\").replace('"', "\\\"")
        );
        std::process::Command::new("osascript").arg("-e").arg(script).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Windows' "How do you want to open this file?" dialog (choose an app, optionally always).
#[tauri::command]
pub fn open_with_dialog(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{HSTRING, PCWSTR, w};
        let r = unsafe { ShellExecuteW(None, w!("openas"), &HSTRING::from(path.as_str()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
        if r.0 as isize > 32 { Ok(()) } else { Err(format!("Couldn't show \"Open with\" for {path}.")) }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("\"Open with\" isn't available on macOS yet: use Finder's Open With.".into())
    }
}
