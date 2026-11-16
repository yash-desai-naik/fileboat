//! Icon themes: VS Code icon themes dropped into Fileboat's `icon-themes` folder (read in VS
//! Code's own format, their files served by the `fbicon` scheme from inside that folder only),
//! the system's own icons (Windows: the shell's icon for each type, or a program's own), and
//! pictures chosen in Change icon….

use base64::Engine as _;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::http::{header, Request, Response, StatusCode};

/// `<data dir>/icon-themes`, created if missing.
pub fn themes_dir() -> Result<PathBuf, String> {
    let dir = fileboat_core::paths::data_dir()?.join("icon-themes");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInfo {
    /// "<folder>::<theme id>"
    pub id: String,
    pub label: String,
}

/// A theme's JSON files in a folder: (id, label, json path). A VS Code extension's package.json
/// lists them (`contributes.iconThemes`); otherwise any top-level JSON with `iconDefinitions`.
fn themes_in(folder: &Path) -> Vec<(String, String, PathBuf)> {
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(folder.join("package.json"))
        && let Ok(pkg) = serde_json::from_str::<Value>(&text)
        && let Some(list) = pkg.pointer("/contributes/iconThemes").and_then(Value::as_array)
    {
        for t in list {
            let (Some(id), Some(path)) = (t.get("id").and_then(Value::as_str), t.get("path").and_then(Value::as_str)) else { continue };
            let label = t.get("label").and_then(Value::as_str).unwrap_or(id);
            out.push((format!("{name}::{id}"), label.to_string(), folder.join(path)));
        }
    }
    if out.is_empty()
        && let Ok(rd) = std::fs::read_dir(folder)
    {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json")
                && std::fs::read_to_string(&p).is_ok_and(|t| t.contains("\"iconDefinitions\""))
            {
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                out.push((format!("{name}::{stem}"), format!("{name} ({stem})"), p));
            }
        }
    }
    out
}

/// The VS Code icon themes in the icon-themes folder.
#[tauri::command]
pub fn icon_themes_list() -> Result<Vec<ThemeInfo>, String> {
    let dir = themes_dir()?;
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        if e.path().is_dir() {
            out.extend(themes_in(&e.path()).into_iter().map(|(id, label, _)| ThemeInfo { id, label }));
        }
    }
    out.sort_by_key(|t| t.label.to_lowercase());
    Ok(out)
}

/// A theme as the UI uses it: icon ids to files (relative to the theme's folder) and the maps.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IconTheme {
    id: String,
    label: String,
    folder: String,
    defs: HashMap<String, String>,
    #[serde(flatten)]
    maps: Maps,
    light: Option<Maps>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Maps {
    file: Option<String>,
    folder_icon: Option<String>,
    folder_expanded: Option<String>,
    file_extensions: HashMap<String, String>,
    file_names: HashMap<String, String>,
    folder_names: HashMap<String, String>,
    folder_names_expanded: HashMap<String, String>,
}

fn string_map(v: Option<&Value>) -> HashMap<String, String> {
    v.and_then(Value::as_object).map(|m| m.iter().filter_map(|(k, v)| Some((k.to_lowercase(), v.as_str()?.to_string()))).collect()).unwrap_or_default()
}

fn maps(o: &Map<String, Value>) -> Maps {
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
    Maps {
        file: s("file"),
        folder_icon: s("folder"),
        folder_expanded: s("folderExpanded"),
        file_extensions: string_map(o.get("fileExtensions")),
        file_names: string_map(o.get("fileNames")),
        folder_names: string_map(o.get("folderNames")),
        folder_names_expanded: string_map(o.get("folderNamesExpanded")),
    }
}

/// Read a theme: its icons (files only: font-glyph icons aren't supported) and its maps.
#[tauri::command]
pub fn icon_theme_load(id: String) -> Result<IconTheme, String> {
    let folder_name = id.split_once("::").ok_or("Not an icon theme id.")?.0.to_string();
    let folder_name = folder_name.as_str();
    if folder_name.contains(['/', '\\']) || folder_name.starts_with('.') {
        return Err("Not an icon theme id.".into());
    }
    let folder = themes_dir()?.join(folder_name);
    let (_, label, json_path) = themes_in(&folder).into_iter().find(|(i, ..)| *i == id).ok_or("That icon theme isn't in the icon-themes folder any more.")?;
    let text = std::fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("The theme's JSON can't be read: {e}"))?;
    let o = json.as_object().ok_or("The theme's JSON isn't an object.")?;
    let base = json_path.parent().unwrap_or(&folder);
    let mut defs = HashMap::new();
    for (k, v) in o.get("iconDefinitions").and_then(Value::as_object).into_iter().flatten() {
        let Some(icon_path) = v.get("iconPath").and_then(Value::as_str) else { continue };
        // Relative to the theme's folder, with forward slashes (for the icon URLs).
        let full = base.join(icon_path);
        if let Ok(rel) = normalize(&full).strip_prefix(normalize(&folder)) {
            defs.insert(k.clone(), rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(IconTheme {
        id,
        label,
        folder: folder_name.to_string(),
        defs,
        maps: maps(o),
        light: o.get("light").and_then(Value::as_object).map(maps),
    })
}

/// `a/./b/../c` → `a/c`, without touching the disk.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Serve `fbicon://localhost/<theme folder>/<path>`: a file inside the icon-themes folder only.
pub fn serve(request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let status = |code: StatusCode| Response::builder().status(code).body(Vec::new()).expect("a response");
    let Ok(dir) = themes_dir() else { return status(StatusCode::NOT_FOUND) };
    let raw = request.uri().path().trim_start_matches('/');
    let decoded = url::form_urlencoded::parse(format!("p={raw}").as_bytes()).find(|(k, _)| k == "p").map(|(_, v)| v.into_owned()).unwrap_or_default();
    let wanted = normalize(&dir.join(decoded.replace('\\', "/")));
    // Never outside the icon-themes folder (no "..", no links out of it).
    let (Ok(real), Ok(base)) = (std::fs::canonicalize(&wanted), std::fs::canonicalize(&dir)) else { return status(StatusCode::NOT_FOUND) };
    if !real.starts_with(&base) || !real.is_file() {
        return status(StatusCode::NOT_FOUND);
    }
    let mime = match real.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        _ => return status(StatusCode::NOT_FOUND),
    };
    match std::fs::read(&real) {
        Ok(body) => Response::builder()
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "max-age=3600")
            .header("X-Content-Type-Options", "nosniff")
            .header(header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; sandbox")
            .body(body)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

/// Open the icon-themes folder in a new tab (so a theme can be dropped in).
#[tauri::command]
pub fn icon_themes_folder() -> Result<String, String> {
    Ok(themes_dir()?.display().to_string())
}

/// A picture for Change icon… (SVG, PNG, ICO, JPEG, GIF, WebP up to 256 KB) as a data URL.
#[tauri::command]
pub fn icon_image(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let mime = match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return Err("Icons can be SVG, PNG, ICO, JPEG, GIF or WebP.".into()),
    };
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > 256 * 1024 {
        return Err("That picture is larger than 256 KB.".into());
    }
    let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// The system's icon for a file type (or a program's own, for .exe and shortcuts) as a PNG data URL.
#[tauri::command]
pub async fn system_icon(path: String, folder: bool) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || system::icon_png(&path, folder).map(|png| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
mod system {
    use windows::core::HSTRING;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES};
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    /// Files whose icon is their own (asked for by path); the rest by type only.
    fn own_icon(path: &str) -> bool {
        let lower = path.to_lowercase();
        [".exe", ".lnk", ".ico", ".url", ".appref-ms", ".msc", ".cpl", ".scr"].iter().any(|e| lower.ends_with(e))
    }

    pub fn icon_png(path: &str, folder: bool) -> Option<Vec<u8>> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let mut info = SHFILEINFOW::default();
            let (target, attrs, flags) = if folder {
                (HSTRING::from("folder"), FILE_ATTRIBUTE_DIRECTORY, SHGFI_USEFILEATTRIBUTES)
            } else if own_icon(path) {
                (HSTRING::from(path), FILE_ATTRIBUTE_NORMAL, Default::default())
            } else {
                // By type: never opens the file (cloud files aren't downloaded).
                let ext = std::path::Path::new(path).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_else(|| ".".into());
                (HSTRING::from(ext), FILE_ATTRIBUTE_NORMAL, SHGFI_USEFILEATTRIBUTES)
            };
            let ok = SHGetFileInfoW(&target, attrs, Some(&mut info), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON | flags);
            if ok == 0 || info.hIcon.is_invalid() {
                return None;
            }
            let png = to_png(info.hIcon);
            let _ = DestroyIcon(info.hIcon);
            png
        }
    }

    unsafe fn to_png(icon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<Vec<u8>> {
        unsafe {
            let mut ii = ICONINFO::default();
            GetIconInfo(icon, &mut ii).ok()?;
            let result = (|| {
                let mut bm = BITMAP::default();
                if GetObjectW(ii.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
                    return None;
                }
                let (w, h) = (bm.bmWidth, bm.bmHeight);
                let dc = CreateCompatibleDC(None);
                let mut bi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
                    ..Default::default()
                };
                let mut px = vec![0u8; (w * h * 4) as usize];
                let lines = GetDIBits(dc, ii.hbmColor, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
                // Old-style icons have no alpha: the mask says what's transparent.
                let mut mask = vec![0u8; (w * h * 4) as usize];
                let has_alpha = px.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
                if !has_alpha {
                    GetDIBits(dc, ii.hbmMask, 0, h as u32, Some(mask.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
                }
                let _ = DeleteDC(dc);
                if lines == 0 {
                    return None;
                }
                // BGRA → RGBA.
                for (i, p) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    p.swap(0, 2);
                    if !has_alpha {
                        p[3] = if mask[i * 4] == 0 { 255 } else { 0 };
                    }
                }
                let mut out = Vec::new();
                {
                    let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    let mut writer = enc.write_header().ok()?;
                    writer.write_image_data(&px).ok()?;
                }
                Some(out)
            })();
            let _ = DeleteObject(ii.hbmColor.into());
            let _ = DeleteObject(ii.hbmMask.into());
            result
        }
    }
}

#[cfg(not(windows))]
mod system {
    /// macOS system icons come later; until then Fileboat's minimal icons show.
    pub fn icon_png(_path: &str, _folder: bool) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizing_keeps_paths_inside() {
        assert_eq!(normalize(Path::new("a/b/../c/./d.svg")), PathBuf::from("a/c/d.svg"));
        let base = Path::new("themes/mat");
        assert!(normalize(&base.join("../../secret")).strip_prefix(base).is_err());
    }

    #[test]
    fn reads_a_vscode_theme_folder() {
        let dir = std::env::temp_dir().join(format!("fileboat-icons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dist/icons")).unwrap();
        std::fs::write(
            dir.join("package.json"),
            r#"{"contributes":{"iconThemes":[{"id":"mine","label":"My Icons","path":"./dist/theme.json"}]}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("dist/theme.json"),
            r#"{"iconDefinitions":{"_rs":{"iconPath":"./icons/rust.svg"},"_glyph":{"fontCharacter":"\\E001"}},"file":"_rs","fileExtensions":{"RS":"_rs"},"light":{"fileNames":{"x":"_rs"}}}"#,
        )
        .unwrap();
        let found = themes_in(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, "My Icons");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
