//! The preview pane's commands (reading runs off the main thread) and the `fbpreview` scheme,
//! which serves images, media, PDFs and fonts to the WebView with Range support.
//!
//! The scheme serves only files the preview asked for: `preview_url` hands out a token for a
//! path, and a request for anything else is refused, so the WebView never gets the whole disk.

use crate::folders::Folders;
use fileboat_core::fs::{tree_size, TreeSize};
use fileboat_core::preview::{self, ArchiveList, FolderSummary, Info, Text};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::http::{header, Request, Response, StatusCode};
use tauri::State;

/// Run file reading on a worker thread (commands without `async` run on the main thread).
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn preview_info(path: String) -> Result<Info, String> {
    blocking(move || preview::info(&PathBuf::from(path))).await
}

/// The first `limit` bytes as text (at most 64 MB; "Load more" asks for more).
#[tauri::command]
pub async fn preview_text(path: String, limit: u64) -> Result<Text, String> {
    blocking(move || preview::read_text(&PathBuf::from(path), limit.min(64 << 20))).await
}

#[tauri::command]
pub async fn preview_bytes(path: String, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    blocking(move || preview::read_bytes(&PathBuf::from(path), offset, len)).await
}

#[tauri::command]
pub async fn preview_archive(path: String) -> Result<ArchiveList, String> {
    blocking(move || preview::list_archive(&PathBuf::from(path), 5000)).await
}

/// A JSON, YAML or TOML file as JSON, for the tree (files over 8 MB are shown as text only).
#[tauri::command]
pub async fn preview_structured(path: String, format: String) -> Result<serde_json::Value, String> {
    blocking(move || {
        let text = preview::read_text(&PathBuf::from(path), 8 << 20)?;
        if text.truncated {
            return Err("Too big to show as a tree.".into());
        }
        preview::structured(&text.text, &format)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPreview {
    #[serde(flatten)]
    pub summary: FolderSummary,
    /// Everything under it, if known or quick to work out.
    pub size: Option<TreeSize>,
}

/// How long the preview spends adding up a folder it has no size for.
const FOLDER_SIZE_TIME: Duration = Duration::from_millis(1500);

#[tauri::command]
pub async fn preview_folder(folders: State<'_, Folders>, path: String) -> Result<FolderPreview, String> {
    let path = PathBuf::from(path);
    let cached = folders.cached_size(&path);
    blocking(move || {
        let summary = preview::folder_summary(&path)?;
        let size = cached.or_else(|| {
            let started = Instant::now();
            tree_size(&path, || started.elapsed() > FOLDER_SIZE_TIME)
        });
        Ok(FolderPreview { summary, size })
    })
    .await
}

/// Paths the preview may load through the scheme, by token (the most recent few).
#[derive(Default)]
pub struct PreviewFiles(Mutex<(HashMap<String, PathBuf>, VecDeque<String>)>);

const TOKENS_KEPT: usize = 64;

/// A token the WebView can load `path` with (`fbpreview://localhost/<token>`).
#[tauri::command]
pub fn preview_url(files: State<'_, PreviewFiles>, path: String) -> String {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write(path.as_bytes());
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let token = format!("{:016x}{:016x}", h.finish(), std::collections::hash_map::RandomState::new().build_hasher().finish());
    let mut guard = files.0.lock().unwrap();
    let (map, order) = &mut *guard;
    map.insert(token.clone(), PathBuf::from(path));
    order.push_back(token.clone());
    while order.len() > TOKENS_KEPT {
        if let Some(old) = order.pop_front() {
            map.remove(&old);
        }
    }
    token
}

/// What a media element gets per request when it doesn't say how much (it asks again).
const CHUNK: u64 = 4 << 20;
/// Files bigger than this are only served in ranges.
const WHOLE_MAX: u64 = 256 << 20;

/// Serve a request to `fbpreview://localhost/<token>`.
pub fn serve(files: &PreviewFiles, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let token = request.uri().path().trim_start_matches('/').to_string();
    let path = files.0.lock().unwrap().0.get(&token).cloned();
    let Some(path) = path else { return status(StatusCode::NOT_FOUND) };
    match serve_file(&path, request.headers().get(header::RANGE).and_then(|v| v.to_str().ok())) {
        Ok(r) => r,
        Err(code) => status(code),
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(code).body(Vec::new()).expect("a response")
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n` → (start, end inclusive), clamped to `len`.
fn parse_range(range: &str, len: u64) -> Option<(u64, u64)> {
    let spec = range.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            (len.saturating_sub(n), len.checked_sub(1)?)
        }
        (a, "") => {
            let a: u64 = a.parse().ok()?;
            (a, (a + CHUNK - 1).min(len.checked_sub(1)?))
        }
        (a, b) => (a.parse().ok()?, b.parse::<u64>().ok()?.min(len.checked_sub(1)?)),
    };
    (start <= end).then_some((start, end))
}

fn serve_file(path: &std::path::Path, range: Option<&str>) -> Result<Response<Vec<u8>>, StatusCode> {
    let mut file = std::fs::File::open(path).map_err(|_| StatusCode::NOT_FOUND)?;
    let len = file.metadata().map_err(|_| StatusCode::NOT_FOUND)?.len();
    let base = Response::builder()
        .header(header::CONTENT_TYPE, preview::mime_of(path))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header("X-Content-Type-Options", "nosniff")
        // Nothing served here may run scripts, even opened on its own.
        .header(header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; sandbox");
    let (start, end) = match range {
        Some(r) => parse_range(r, len).ok_or(StatusCode::RANGE_NOT_SATISFIABLE)?,
        None if len > WHOLE_MAX => (0, CHUNK.min(len) - 1),
        None if len == 0 => return base.status(StatusCode::OK).body(Vec::new()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
        None => (0, len - 1),
    };
    file.seek(SeekFrom::Start(start)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut body = Vec::with_capacity((end - start + 1) as usize);
    file.take(end - start + 1).read_to_end(&mut body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let partial = range.is_some() || len > WHOLE_MAX;
    let r = if partial {
        base.status(StatusCode::PARTIAL_CONTENT).header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
    } else {
        base.status(StatusCode::OK)
    };
    r.body(body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=0-", 100 << 20), Some((0, CHUNK - 1)), "open ranges come in chunks");
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=2000-", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }

    #[test]
    fn only_issued_tokens_are_served() {
        let dir = std::env::temp_dir().join(format!("fileboat-scheme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.png");
        std::fs::write(&file, b"0123456789").unwrap();
        let files = PreviewFiles::default();
        files.0.lock().unwrap().0.insert("tok".into(), file.clone());
        let get = |uri: &str, range: Option<&str>| {
            let mut b = Request::builder().uri(uri);
            if let Some(r) = range {
                b = b.header(header::RANGE, r);
            }
            serve(&files, &b.body(Vec::new()).unwrap())
        };
        let whole = get("fbpreview://localhost/tok", None);
        assert_eq!((whole.status(), whole.body().as_slice()), (StatusCode::OK, &b"0123456789"[..]));
        assert_eq!(whole.headers()[header::CONTENT_TYPE], "image/png");
        let part = get("http://fbpreview.localhost/tok", Some("bytes=2-4"));
        assert_eq!((part.status(), part.body().as_slice()), (StatusCode::PARTIAL_CONTENT, &b"234"[..]));
        assert_eq!(part.headers()[header::CONTENT_RANGE], "bytes 2-4/10");
        assert_eq!(get("fbpreview://localhost/other", None).status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
