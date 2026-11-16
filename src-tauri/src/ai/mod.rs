//! The Claude pane (from Tableboat): runs the user's own Claude Code (`claude`), talking
//! stream-JSON over stdin/stdout. No Node.js and no adapter.
//!
//! Claude Code works in the repository root of the folder the window is in (see
//! `fileboat_core::claude`). Moving into another repository doesn't touch a running chat: the
//! pane offers to switch, and the next new chat starts there.
//!
//! The process only exists while the pane is open. Fileboat's MCP server (`fileboat_core::mcp`,
//! served for each window by `host.rs`) gives Claude the app's tools; every other permission
//! question is routed to the pane's Allow / Deny buttons through `--permission-prompt-tool`.

pub mod host;

use fileboat_core::claude::{self, AiContext};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};

struct Proc {
    child: Child,
    /// Shared so a write never holds the window-state lock across an await.
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
}

#[derive(Default)]
struct WindowState {
    proc: Option<Proc>,
    /// Claude Code's session id, for --resume after a stop.
    claude_session: Option<String>,
    /// Tools the user allowed for the rest of this chat.
    always_allowed: HashSet<String>,
    context: AiContext,
    /// Where this chat runs (fixed for the chat; None: the next chat picks it from the context).
    folder: Option<PathBuf>,
}

#[derive(Default)]
pub struct ClaudeHub {
    windows: Mutex<HashMap<String, WindowState>>,
    pub mcp: Mutex<Option<fileboat_core::mcp::McpServer>>,
    /// Permission questions waiting for the user: id → answer channel.
    pub approvals: Mutex<HashMap<String, std::sync::mpsc::Sender<(bool, bool)>>>,
    /// Claude's window actions waiting for the window to do them: id → result channel.
    pub ui_calls: Mutex<HashMap<String, std::sync::mpsc::Sender<Result<Value, String>>>>,
}

/// Where a chat would run for this context: the repository of the folder, or the folder (or home).
fn wanted_folder(ctx: &AiContext) -> PathBuf {
    let folder = ctx.folder.as_deref().map(PathBuf::from).filter(|p| p.is_dir()).or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
    claude::working_folder(&folder)
}

impl ClaudeHub {
    pub fn context(&self, window: &str) -> AiContext {
        self.windows.lock().unwrap().get(window).map(|w| w.context.clone()).unwrap_or_default()
    }

    pub fn set_context(&self, window: &str, ctx: AiContext) {
        self.windows.lock().unwrap().entry(window.to_string()).or_default().context = ctx;
    }

    /// The chat's folder: the running one's, or where a new one would start.
    pub fn folder(&self, window: &str) -> PathBuf {
        let windows = self.windows.lock().unwrap();
        let w = windows.get(window);
        w.and_then(|w| w.folder.clone()).unwrap_or_else(|| wanted_folder(&w.map(|w| w.context.clone()).unwrap_or_default()))
    }

    pub fn is_always_allowed(&self, window: &str, tool: &str) -> bool {
        self.windows.lock().unwrap().get(window).is_some_and(|w| w.always_allowed.contains(tool))
    }

    pub fn allow_always(&self, window: &str, tool: &str) {
        self.windows.lock().unwrap().entry(window.to_string()).or_default().always_allowed.insert(tool.to_string());
    }
}

async fn spawn(app: &AppHandle, window: &str, resume: Option<String>) -> Result<Proc, String> {
    let claude = claude::find_claude().ok_or(claude::NOT_INSTALLED)?;
    let hub = app.state::<ClaudeHub>();
    let folder = {
        let mut windows = hub.windows.lock().unwrap();
        let w = windows.entry(window.to_string()).or_default();
        let f = w.folder.clone().unwrap_or_else(|| wanted_folder(&w.context));
        w.folder = Some(f.clone());
        f
    };

    // Fileboat's MCP server, started on first use.
    let (port, token) = {
        let mut guard = hub.mcp.lock().unwrap();
        if guard.is_none() {
            *guard = Some(fileboat_core::mcp::McpServer::start(host::resolver(app.clone()))?);
        }
        let s = guard.as_ref().expect("started");
        (s.port, s.token.clone())
    };
    let config_path = fileboat_core::paths::data_dir()?.join(format!("mcp-{window}.json"));
    claude::write_mcp_config(&config_path, port, &token, window)?;

    let model = app.state::<crate::settings::SettingsStore>().current().claude_model;
    let mut cmd = Command::new(&claude);
    cmd.current_dir(&folder)
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
        .arg("--include-partial-messages")
        .args(claude::base_args(&config_path, &claude::pane_prompt(), &model))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if let Some(id) = resume {
        cmd.args(["--resume", &id]);
    }
    #[cfg(windows)]
    {
        // No console window.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Could not start Claude Code: {e}"))?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;

    // stdout: one JSON message per line, forwarded to the window as is.
    let app2 = app.clone();
    let win = window.to_string();
    tauri::async_runtime::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(id) = value.get("session_id").and_then(Value::as_str)
                && let Some(w) = app2.state::<ClaudeHub>().windows.lock().unwrap().get_mut(&win)
            {
                w.claude_session = Some(id.to_string());
            }
            let _ = app2.emit_to(&win, "claude:event", value);
        }
    });
    // stderr: keep the tail for error messages.
    let app3 = app.clone();
    let win3 = window.to_string();
    tauri::async_runtime::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            tail.push(line);
            if tail.len() > 20 {
                tail.remove(0);
            }
        }
        let _ = app3.emit_to(&win3, "claude:exit", json!({ "stderr": tail.join("\n") }));
    });

    Ok(Proc { child, stdin: Arc::new(tokio::sync::Mutex::new(stdin)) })
}

// ---------- commands ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    installed: bool,
    path: Option<String>,
    running: bool,
    session: Option<String>,
    /// Where the chat runs.
    folder: String,
    /// Where a new chat would run now (another repository than `folder` after moving on).
    here: String,
    /// The model Claude Code uses when Fileboat doesn't pick one (~/.claude/settings.json).
    default_model: Option<String>,
}

#[tauri::command]
pub fn claude_status(window: tauri::Window, hub: tauri::State<'_, ClaudeHub>) -> ClaudeStatus {
    let path = claude::find_claude();
    let label = window.label();
    let here = wanted_folder(&hub.context(label));
    let windows = hub.windows.lock().unwrap();
    let w = windows.get(label);
    ClaudeStatus {
        installed: path.is_some(),
        path: path.map(|p| p.display().to_string()),
        running: w.is_some_and(|w| w.proc.is_some()),
        session: w.and_then(|w| w.claude_session.clone()),
        folder: w.and_then(|w| w.folder.clone()).unwrap_or_else(|| here.clone()).display().to_string(),
        here: here.display().to_string(),
        default_model: claude::default_model(),
    }
}

/// Send a message, starting Claude Code (resuming this window's chat) if it isn't running.
#[tauri::command]
pub async fn claude_send(app: AppHandle, window: tauri::Window, hub: tauri::State<'_, ClaudeHub>, text: String, context: AiContext, images: Option<Vec<ImageInput>>) -> Result<(), String> {
    let label = window.label().to_string();
    hub.set_context(&label, context);
    let needs_start = {
        let mut windows = hub.windows.lock().unwrap();
        let w = windows.entry(label.clone()).or_default();
        // A process that exited (crash, sign-out) is replaced.
        if let Some(p) = w.proc.as_mut()
            && p.child.try_wait().map(|s| s.is_some()).unwrap_or(true)
        {
            w.proc = None;
        }
        if w.proc.is_none() { Some(w.claude_session.clone()) } else { None }
    };
    if let Some(resume) = needs_start {
        let proc = spawn(&app, &label, resume).await?;
        hub.windows.lock().unwrap().entry(label.clone()).or_default().proc = Some(proc);
    }
    let mut content = vec![json!({ "type": "text", "text": text })];
    for img in images.unwrap_or_default().into_iter().take(20) {
        if !IMAGE_TYPES.contains(&img.media_type.as_str()) {
            return Err(format!("Images must be PNG, JPEG, GIF or WebP (got {}).", img.media_type));
        }
        content.push(json!({ "type": "image", "source": { "type": "base64", "media_type": img.media_type, "data": img.data } }));
    }
    let line = json!({ "type": "user", "message": { "role": "user", "content": content } });
    let stdin = {
        let windows = hub.windows.lock().unwrap();
        windows.get(&label).and_then(|w| w.proc.as_ref()).map(|p| p.stdin.clone()).ok_or("Claude Code stopped.")?
    };
    let mut stdin = stdin.lock().await;
    stdin.write_all(format!("{line}\n").as_bytes()).await.map_err(|e| format!("Could not send to Claude Code: {e}"))?;
    stdin.flush().await.map_err(|e| format!("Could not send to Claude Code: {e}"))
}

/// Stop the running answer (and the process). The next message resumes the same chat.
#[tauri::command]
pub async fn claude_stop(window: tauri::Window, hub: tauri::State<'_, ClaudeHub>) -> Result<(), String> {
    stop_window(&hub, window.label()).await;
    Ok(())
}

pub async fn stop_window(hub: &ClaudeHub, label: &str) {
    let proc = hub.windows.lock().unwrap().get_mut(label).and_then(|w| w.proc.take());
    if let Some(mut p) = proc {
        let _ = p.child.kill().await;
    }
    // Unblock any permission question still waiting.
    hub.approvals.lock().unwrap().clear();
}

/// Start a fresh chat where the window is now (or switch to a past one with `resume`).
#[tauri::command]
pub async fn claude_new(window: tauri::Window, hub: tauri::State<'_, ClaudeHub>, resume: Option<String>) -> Result<(), String> {
    stop_window(&hub, window.label()).await;
    let mut windows = hub.windows.lock().unwrap();
    let w = windows.entry(window.label().to_string()).or_default();
    // A past chat continues in its folder; a new one starts where the window is.
    if resume.is_none() {
        w.folder = None;
    }
    w.claude_session = resume;
    w.always_allowed.clear();
    Ok(())
}

/// What the window shows (for get_context and where the next chat runs).
#[tauri::command]
pub fn claude_set_context(window: tauri::Window, hub: tauri::State<'_, ClaudeHub>, context: AiContext) {
    hub.set_context(window.label(), context);
}

#[tauri::command]
pub fn claude_permission_reply(hub: tauri::State<'_, ClaudeHub>, id: String, allow: bool, always: bool) {
    if let Some(tx) = hub.approvals.lock().unwrap().remove(&id) {
        let _ = tx.send((allow, always));
    }
}

/// The window's answer to one of Claude's actions (see `host::GuiHost::ui`).
#[tauri::command]
pub fn ui_reply(hub: tauri::State<'_, ClaudeHub>, id: String, ok: bool, value: Option<Value>, error: Option<String>) {
    if let Some(tx) = hub.ui_calls.lock().unwrap().remove(&id) {
        let _ = tx.send(if ok { Ok(value.unwrap_or(Value::Null)) } else { Err(error.unwrap_or_else(|| "The window couldn't do that.".into())) });
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntry {
    id: String,
    title: String,
    modified: u64,
}

fn first_user_text(v: &Value) -> Option<String> {
    if v.get("type")?.as_str()? != "user" {
        return None;
    }
    let content = v.get("message")?.get("content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().find_map(|p| (p.get("type")?.as_str()? == "text").then(|| p.get("text")?.as_str().map(str::to_string)).flatten())?,
        _ => return None,
    };
    // Fileboat's attached context isn't part of the title.
    let t = text.split("<fileboat-context>").next().unwrap_or("").trim();
    (!t.is_empty()).then(|| t.chars().take(90).collect())
}

/// Past chats of the chat's project (the repository), newest first: the same ones the terminal shows.
#[tauri::command]
pub fn claude_sessions(window: tauri::Window, hub: tauri::State<'_, ClaudeHub>) -> Vec<SessionEntry> {
    let Some(dir) = claude::sessions_store(&hub.folder(window.label())) else { return vec![] };
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
    let mut out: Vec<SessionEntry> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            let path = e.path();
            let modified = e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            let file = std::fs::File::open(&path).ok()?;
            let title = BufReader::new(file).lines().take(200).map_while(Result::ok).filter_map(|l| serde_json::from_str::<Value>(&l).ok()).find_map(|v| first_user_text(&v))?;
            Some(SessionEntry { id: path.file_stem()?.to_string_lossy().into_owned(), title, modified })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.modified));
    out.truncate(50);
    out
}

/// A past chat's messages (the same shape as live events), shown before resuming it.
#[tauri::command]
pub fn claude_session_messages(window: tauri::Window, hub: tauri::State<'_, ClaudeHub>, id: String) -> Result<Vec<Value>, String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Invalid session id.".into());
    }
    let path = claude::sessions_store(&hub.folder(window.label())).ok_or("No home folder.")?.join(format!("{id}.jsonl"));
    let file = std::fs::File::open(&path).map_err(|e| format!("Could not open the chat: {e}"))?;
    let mut out: Vec<Value> = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
        .filter(|v| matches!(v.get("type").and_then(Value::as_str), Some("user") | Some("assistant")))
        .filter(|v| !v.get("isMeta").and_then(Value::as_bool).unwrap_or(false))
        .collect();
    if out.len() > 400 {
        out.drain(..out.len() - 400);
    }
    Ok(out)
}

// ---------- attachments and voice ----------

const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 256 * 1024;

/// An image for Claude (base64), from a file, the clipboard or a drop.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    media_type: String,
    data: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum Attachment {
    Text { name: String, path: String, text: String, truncated: bool, size: u64 },
    Image { name: String, path: String, media_type: String, data: String, size: u64 },
}

fn image_type(path: &std::path::Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Read a file to attach to a message: images as base64 (up to 5 MB), text up to 256 KB
/// (longer files are cut, and say so). Other binary files are refused.
#[tauri::command]
pub fn read_attachment(path: String) -> Result<Attachment, String> {
    use base64::Engine as _;
    use std::io::Read;
    let p = std::path::Path::new(&path);
    let meta = std::fs::metadata(p).map_err(|e| format!("Could not read {path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{path} isn't a file."));
    }
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
    if let Some(media_type) = image_type(p) {
        if meta.len() > MAX_IMAGE_BYTES {
            return Err(format!("{name} is larger than 5 MB."));
        }
        let bytes = std::fs::read(p).map_err(|e| format!("Could not read {name}: {e}"))?;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        return Ok(Attachment::Image { name, path, media_type: media_type.into(), data, size: meta.len() });
    }
    let mut bytes = Vec::new();
    std::fs::File::open(p).and_then(|f| f.take(MAX_TEXT_BYTES as u64 + 1).read_to_end(&mut bytes)).map_err(|e| format!("Could not read {name}: {e}"))?;
    let truncated = bytes.len() > MAX_TEXT_BYTES;
    bytes.truncate(MAX_TEXT_BYTES);
    if bytes.contains(&0) {
        return Err(format!("{name} isn't a text file (only text files and PNG/JPEG/GIF/WebP images can be attached)."));
    }
    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            let valid = e.utf8_error().valid_up_to();
            let bytes = e.into_bytes();
            if !truncated || bytes.len() - valid > 3 {
                return Err(format!("{name} isn't UTF-8 text."));
            }
            String::from_utf8_lossy(&bytes[..valid]).into_owned()
        }
    };
    Ok(Attachment::Text { name, path, text, truncated, size: meta.len() })
}

/// Windows voice typing (Win+H) for the focused chat input: built into Windows 10/11.
#[tauri::command]
pub fn voice_typing() -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_H, VK_LWIN};
        let key = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } };
        let inputs = [key(VK_LWIN, KEYBD_EVENT_FLAGS(0)), key(VK_H, KEYBD_EVENT_FLAGS(0)), key(VK_H, KEYEVENTF_KEYUP), key(VK_LWIN, KEYEVENTF_KEYUP)];
        if SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize != inputs.len() {
            return Err("Windows didn't accept the voice typing shortcut. Press Win+H yourself.".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("Use macOS dictation instead: turn it on in System Settings > Keyboard > Dictation, then press the microphone key twice.".into())
    }
}
