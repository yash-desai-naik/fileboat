//! A small MCP server inside Fileboat (from Tableboat), so Claude Code can see and work with what
//! the user is browsing.
//!
//! Streamable-HTTP JSON-RPC on 127.0.0.1 (random port) at `/mcp/<window>`, protected by a random
//! bearer token and an Origin check. Tools: get_context, search_files, list_folder, git_status;
//! reveal, select, open_in_app, list_app_commands, run_app_command (the window); get_settings,
//! update_settings; create_folder, create_file, rename, move, trash (these ask first and can be
//! undone); and approve, Claude Code's permission prompt, answered in the pane.

use crate::claude::{AiContext, ASKS_ITSELF, READ_TOOLS};
use crate::ops::undo::Op;
use crate::prefs::Settings;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where the tools run: the app, one host per window.
pub trait Host: Send + Sync {
    /// What the user has open.
    fn context(&self) -> AiContext;
    /// Ask the user to allow something (Allow / Deny in the pane). "Always" is remembered by the host.
    fn ask(&self, tool: &str, input: Value) -> Result<bool, String>;
    /// Ask the window to do something (reveal, select, run a command…) and return its answer.
    fn ui(&self, action: &str, args: Value) -> Result<Value, String>;
    /// Search the index (`scope`: a folder to search under).
    fn search(&self, query: &str, scope: Option<&str>, limit: usize) -> Result<String, String>;
    fn settings(&self) -> Result<Settings, String>;
    fn save_settings(&self, settings: &Settings) -> Result<(), String>;
    /// Remember a change for Ctrl+Z.
    fn undoable(&self, op: Op, description: String);
}

/// Finds the host for a request's route (`/mcp/<route>`).
pub type Resolver = Arc<dyn Fn(&str) -> Option<Arc<dyn Host>> + Send + Sync>;

pub struct McpServer {
    pub port: u16,
    pub token: String,
}

const MAX_OUTPUT: usize = 60_000;

impl McpServer {
    pub fn start(resolve: Resolver) -> Result<Self, String> {
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| format!("Could not start the MCP server: {e}"))?;
        let port = server.server_addr().to_ip().map(|a| a.port()).ok_or("The MCP server has no port")?;
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let tok = token.clone();
        std::thread::Builder::new()
            .name("fileboat-mcp".into())
            .spawn(move || {
                for request in server.incoming_requests() {
                    let resolve = resolve.clone();
                    let tok = tok.clone();
                    // Each request on its own thread: `approve` waits for the user.
                    std::thread::spawn(move || handle(&resolve, &tok, request));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { port, token })
    }
}

fn respond(request: tiny_http::Request, status: u16, body: Option<Value>) {
    let mut response = match body {
        Some(v) => tiny_http::Response::from_string(v.to_string()).with_status_code(status).boxed(),
        None => tiny_http::Response::empty(status).boxed(),
    };
    if let Ok(h) = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        response.add_header(h);
    }
    let _ = request.respond(response);
}

fn header<'a>(request: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn handle(resolve: &Resolver, token: &str, mut request: tiny_http::Request) {
    let authorised = header(&request, "Authorization") == Some(&format!("Bearer {token}"));
    // A web page in some browser can reach 127.0.0.1 too; real MCP clients send no Origin.
    let foreign_origin = header(&request, "Origin").is_some_and(|o| !o.starts_with("http://127.0.0.1"));
    if !authorised || foreign_origin {
        respond(request, 401, None);
        return;
    }
    let window = request.url().strip_prefix("/mcp/").unwrap_or("").split('?').next().unwrap_or("").to_string();
    if window.is_empty() || !window.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        respond(request, 404, None);
        return;
    }
    let Some(host) = resolve(&window) else {
        respond(request, 404, None);
        return;
    };
    if *request.method() != tiny_http::Method::Post {
        respond(request, 405, None);
        return;
    }
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        respond(request, 400, None);
        return;
    }
    let Ok(message) = serde_json::from_str::<Value>(&body) else {
        respond(request, 400, Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "Parse error"}})));
        return;
    };
    let replies: Vec<Value> = match &message {
        Value::Array(batch) => batch.iter().filter_map(|m| rpc(host.as_ref(), m)).collect(),
        single => rpc(host.as_ref(), single).into_iter().collect(),
    };
    match (message.is_array(), replies.len()) {
        (_, 0) => respond(request, 202, None),
        (false, _) => respond(request, 200, replies.into_iter().next()),
        (true, _) => respond(request, 200, Some(Value::Array(replies))),
    }
}

/// One JSON-RPC message. Notifications (no id) get no reply.
fn rpc(host: &dyn Host, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "fileboat", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call_tool(host, name, &args) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                Err(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": true }),
            })
        }
        _ => Err(json!({ "code": -32601, "message": format!("Method not found: {method}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    })
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn tool_list() -> Value {
    let path = json!({ "type": "string", "description": "A full path, or relative to the folder the user is in" });
    let paths = json!({ "type": "array", "items": { "type": "string" }, "description": "Full paths, or relative to the folder the user is in" });
    json!([
        { "name": "get_context", "description": "What the user has open in Fileboat: the folder, the selected items, the file in the preview pane, the view, and the search box's text.", "inputSchema": obj(json!({}), &[]) },
        { "name": "search_files", "description": "Search Fileboat's index of file and folder names (fast, the whole indexed disk or one folder). Words match names fuzzily; filters: ext:rs,ts kind:file|folder|image|code|doc|archive|audio|video|font size:>10mb size:1mb..5mb modified:<7d modified:today in:src (under a folder named like that) a/b (folders on the path) /regex/. Returns paths with size and date.", "inputSchema": obj(json!({ "query": { "type": "string" }, "folder": { "type": "string", "description": "Search only under this folder (default: everywhere indexed)" }, "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 50 } }), &["query"]) },
        { "name": "list_folder", "description": "A folder's items (default: the folder the user is in) with kind, size and modified date, folders first. Hidden items only with hidden: true.", "inputSchema": obj(json!({ "path": path, "hidden": { "type": "boolean", "default": false } }), &[]) },
        { "name": "git_status", "description": "The git repository of a folder (default: the user's folder): branch, upstream, ahead/behind, a merge or rebase in progress, and every changed, staged, untracked and conflicted file.", "inputSchema": obj(json!({ "path": path }), &[]) },
        { "name": "reveal", "description": "Show a file or folder in Fileboat: open its folder in the current tab with it selected (the preview shows it).", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "select", "description": "Select items by name or path in the folder the user is in.", "inputSchema": obj(json!({ "paths": paths }), &["paths"]) },
        { "name": "open_in_app", "description": "Open a file with its default app (or a folder in a new Fileboat tab). Asks the user first.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "list_app_commands", "description": "Every command in Fileboat's command bar (views, sorting, panes, theme, git, file operations…) with its id.", "inputSchema": obj(json!({}), &[]) },
        { "name": "run_app_command", "description": "Run a command bar command by id (see list_app_commands). Asks the user first.", "inputSchema": obj(json!({ "id": { "type": "string" } }), &["id"]) },
        { "name": "get_settings", "description": "Fileboat's settings and what each accepts.", "inputSchema": obj(json!({}), &[]) },
        { "name": "update_settings", "description": "Change Fileboat settings: pass only the keys to change (see get_settings). Asks the user first.", "inputSchema": obj(json!({ "changes": { "type": "object" } }), &["changes"]) },
        { "name": "create_folder", "description": "Create a folder. Asks the user first; Ctrl+Z undoes it.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "create_file", "description": "Create a file with optional text content (never replaces an existing file). Asks the user first; Ctrl+Z undoes it.", "inputSchema": obj(json!({ "path": path, "content": { "type": "string" } }), &["path"]) },
        { "name": "rename", "description": "Rename a file or folder (a new name, not a path). Asks the user first; Ctrl+Z undoes it.", "inputSchema": obj(json!({ "path": path, "new_name": { "type": "string" } }), &["path", "new_name"]) },
        { "name": "move", "description": "Move files or folders into a folder on the same drive. Asks the user first; Ctrl+Z undoes it.", "inputSchema": obj(json!({ "paths": paths, "to": { "type": "string", "description": "The destination folder" } }), &["paths", "to"]) },
        { "name": "trash", "description": "Move files or folders to the Recycle Bin / Trash. Asks the user first; Ctrl+Z puts them back.", "inputSchema": obj(json!({ "paths": paths }), &["paths"]) },
        { "name": "approve", "description": "Permission prompt handler used by Fileboat (not for direct use).", "inputSchema": obj(json!({ "tool_name": { "type": "string" }, "input": { "type": "object" }, "tool_use_id": { "type": "string" } }), &["tool_name", "input"]) }
    ])
}

pub(crate) fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… (cut at {max} characters)", &s[..end])
}

/// A path from Claude: absolute, or relative to the user's folder.
fn resolve(host: &dyn Host, p: &str) -> Result<PathBuf, String> {
    let p = p.trim();
    if p.is_empty() {
        return Err("A path is needed.".into());
    }
    let base = host.context().folder.map(PathBuf::from);
    Ok(crate::fs::resolve(p, base.as_deref()))
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| format!("`{key}` is required."))
}

fn arg_paths(host: &dyn Host, args: &Value, key: &str) -> Result<Vec<PathBuf>, String> {
    let list = args.get(key).and_then(Value::as_array).ok_or_else(|| format!("`{key}` must be a list of paths."))?;
    if list.is_empty() || list.len() > 500 {
        return Err(format!("`{key}`: between 1 and 500 paths."));
    }
    list.iter().map(|v| v.as_str().ok_or_else(|| "Paths must be strings.".to_string()).and_then(|s| resolve(host, s))).collect()
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn fmt_size(n: u64) -> String {
    match n {
        n if n < 1024 => format!("{n} B"),
        n if n < 1 << 20 => format!("{:.1} KB", n as f64 / 1024.0),
        n if n < 1 << 30 => format!("{:.1} MB", n as f64 / (1u64 << 20) as f64),
        n => format!("{:.1} GB", n as f64 / (1u64 << 30) as f64),
    }
}

fn fmt_date(ms: f64) -> String {
    // UTC, as text the model reads easily.
    let secs = (ms / 1000.0) as i64;
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil(days);
    let t = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", t / 3600, (t % 3600) / 60)
}

/// Days since 1970 → (year, month, day).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn call_tool(host: &dyn Host, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "approve" => approve(host, args),
        "get_context" => {
            let c = host.context();
            let mut out = String::new();
            match &c.folder {
                Some(f) => {
                    out.push_str(&format!("Folder: {f}\n"));
                    if let Some(repo) = crate::git::discover(Path::new(f)) {
                        let head = crate::git::head(&repo);
                        out.push_str(&format!("Git repository: {} (branch {})\n", repo.root.display(), head.branch.or(head.detached).unwrap_or_default()));
                    }
                }
                None => out.push_str("No folder is open.\n"),
            }
            if let Some(v) = &c.view {
                out.push_str(&format!("View: {v}\n"));
            }
            if c.selection.is_empty() {
                out.push_str("Nothing is selected.\n");
            } else {
                out.push_str(&format!("Selected ({}):\n", c.selection.len()));
                for s in c.selection.iter().take(200) {
                    out.push_str(&format!("- {s}\n"));
                }
            }
            if let Some(p) = &c.preview {
                out.push_str(&format!("In the preview pane: {p}\n"));
            }
            if let Some(s) = c.search.as_deref().filter(|s| !s.trim().is_empty()) {
                out.push_str(&format!("Searching for: {s}\n"));
            }
            Ok(out)
        }
        "search_files" => {
            let query = arg_str(args, "query")?;
            let folder = args.get("folder").and_then(Value::as_str).map(|f| resolve(host, f)).transpose()?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50).clamp(1, 500) as usize;
            host.search(query, folder.as_deref().map(|p| p.to_str().unwrap_or_default()), limit).map(|s| clip(&s, MAX_OUTPUT))
        }
        "list_folder" => {
            let path = match args.get("path").and_then(Value::as_str) {
                Some(p) => resolve(host, p)?,
                None => host.context().folder.map(PathBuf::from).ok_or("No folder is open; pass `path`.")?,
            };
            let hidden = args.get("hidden").and_then(Value::as_bool).unwrap_or(false);
            let mut entries = Vec::new();
            crate::fs::list(&path, 1000, |batch| {
                entries.extend(batch);
                entries.len() < 5000
            })?;
            let skipped = entries.iter().filter(|e| e.hidden && !hidden).count();
            entries.retain(|e| hidden || !e.hidden);
            entries.sort_by(|a, b| (b.kind == crate::fs::Kind::Folder).cmp(&(a.kind == crate::fs::Kind::Folder)).then_with(|| crate::fs::natural::compare(&a.key, &b.key)));
            let mut out = format!("{} ({} items{})\n", path.display(), entries.len(), if skipped > 0 { format!(", {skipped} hidden not shown") } else { String::new() });
            for e in entries.iter().take(1000) {
                let kind = if e.kind == crate::fs::Kind::Folder { "folder" } else { "file" };
                let size = if e.kind == crate::fs::Kind::Folder { String::new() } else { format!("  {}", fmt_size(e.size)) };
                out.push_str(&format!("{}{}  [{kind}{size}]  {}\n", e.name, if e.kind == crate::fs::Kind::Folder { "/" } else { "" }, fmt_date(e.modified)));
            }
            if entries.len() > 1000 {
                out.push_str("… (the first 1000 shown)\n");
            }
            Ok(clip(&out, MAX_OUTPUT))
        }
        "git_status" => {
            let path = match args.get("path").and_then(Value::as_str) {
                Some(p) => resolve(host, p)?,
                None => host.context().folder.map(PathBuf::from).ok_or("No folder is open; pass `path`.")?,
            };
            let repo = crate::git::discover(&path).ok_or_else(|| format!("{} isn't in a git repository.", path.display()))?;
            let st = crate::git::status(&repo, false, || false)?;
            let b = &st.branch;
            let mut out = format!(
                "Repository: {}\nBranch: {}\nUpstream: {}{}\n",
                repo.root.display(),
                b.head.clone().unwrap_or_else(|| "(detached)".into()),
                b.upstream.clone().unwrap_or_else(|| "none".into()),
                if b.ahead + b.behind > 0 { format!(" (ahead {}, behind {})", b.ahead, b.behind) } else { String::new() }
            );
            if let Some(s) = crate::git::in_progress(&repo) {
                out.push_str(&format!("In progress: {s}\n"));
            }
            if st.files.is_empty() {
                out.push_str("No changes.\n");
            }
            for f in st.files.iter().take(2000) {
                let what = if f.conflicted {
                    "conflict".to_string()
                } else if f.untracked {
                    "untracked".to_string()
                } else {
                    format!("staged {} / unstaged {}", f.index, f.worktree)
                };
                out.push_str(&format!("{}  ({what}){}\n", f.path, f.orig.as_ref().map(|o| format!(" from {o}")).unwrap_or_default()));
            }
            Ok(clip(&out, MAX_OUTPUT))
        }
        "reveal" => {
            let path = resolve(host, arg_str(args, "path")?)?;
            if !path.exists() {
                return Err(format!("{} doesn't exist.", path.display()));
            }
            host.ui("reveal", json!({ "path": path }))?;
            Ok(format!("Showing {} in Fileboat.", path.display()))
        }
        "select" => {
            let paths = arg_paths(host, args, "paths")?;
            let names: Vec<String> = paths.iter().map(|p| name_of(p)).collect();
            let n = host.ui("select", json!({ "names": names }))?;
            Ok(format!("Selected {}.", n.as_u64().unwrap_or(0)))
        }
        "open_in_app" => {
            let path = resolve(host, arg_str(args, "path")?)?;
            if !path.exists() {
                return Err(format!("{} doesn't exist.", path.display()));
            }
            let summary = if path.is_dir() { format!("Open {} in a new tab", path.display()) } else { format!("Open {} with its app", path.display()) };
            if !host.ask("mcp__fileboat__open_in_app", json!({ "question": "Open this?", "summary": summary }))? {
                return Err("The user said no.".into());
            }
            host.ui("open", json!({ "path": path }))?;
            Ok("Opened.".into())
        }
        "list_app_commands" => {
            let list = host.ui("commands", json!({}))?;
            Ok(clip(&serde_json::to_string_pretty(&list).unwrap_or_default(), MAX_OUTPUT))
        }
        "run_app_command" => {
            let id = arg_str(args, "id")?;
            let list = host.ui("commands", json!({}))?;
            let title = list.as_array().and_then(|l| l.iter().find(|c| c.get("id").and_then(Value::as_str) == Some(id))).and_then(|c| c.get("title")).and_then(Value::as_str).map(str::to_string);
            let title = title.ok_or_else(|| format!("There's no command \"{id}\" (see list_app_commands)."))?;
            if !host.ask("mcp__fileboat__run_app_command", json!({ "question": "Run this command?", "summary": title }))? {
                return Err("The user said no.".into());
            }
            host.ui("run", json!({ "id": id }))?;
            Ok(format!("Ran \"{title}\"."))
        }
        "get_settings" => {
            let s = host.settings()?;
            Ok(format!("{}\n\nWhat each accepts:\n{}", serde_json::to_string_pretty(&s).unwrap_or_default(), crate::prefs::HELP))
        }
        "update_settings" => {
            let changes = args.get("changes").cloned().ok_or("`changes` is required.")?;
            let current = host.settings()?;
            let (next, diff) = current.checked_change(&changes)?;
            if diff.is_empty() {
                return Ok("Nothing to change.".into());
            }
            let summary = diff.iter().map(|(k, a, b)| format!("{k}: {a} → {b}")).collect::<Vec<_>>().join("\n");
            if !host.ask("mcp__fileboat__update_settings", json!({ "question": "Change these settings?", "summary": summary }))? {
                return Err("The user said no.".into());
            }
            host.save_settings(&next)?;
            Ok(format!("Changed:\n{summary}"))
        }
        "create_folder" | "create_file" => {
            let folder = name == "create_folder";
            let path = resolve(host, arg_str(args, "path")?)?;
            let dir = path.parent().ok_or("That path has no folder.")?;
            let file_name = name_of(&path);
            crate::ops::check_name(&file_name)?;
            if path.exists() {
                return Err(format!("{} already exists.", path.display()));
            }
            if !dir.is_dir() {
                return Err(format!("{} isn't a folder.", dir.display()));
            }
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            let summary = if folder || content.is_empty() {
                path.display().to_string()
            } else {
                format!("{}\n\n{}", path.display(), clip(content, 2000))
            };
            let tool = if folder { "mcp__fileboat__create_folder" } else { "mcp__fileboat__create_file" };
            let question = if folder { "Create this folder?" } else { "Create this file?" };
            if !host.ask(tool, json!({ "question": question, "summary": summary }))? {
                return Err("The user said no.".into());
            }
            let made = if folder {
                std::fs::create_dir(&path).map_err(|e| crate::fs::describe(&path, &e))?;
                path.clone()
            } else {
                // create_new: never replaces a file that appeared meanwhile.
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| crate::fs::describe(&path, &e))?;
                f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
                path.clone()
            };
            host.undoable(Op::Created(made.clone()), format!("New {} “{file_name}” (Claude)", if folder { "folder" } else { "file" }));
            Ok(format!("Created {}.", made.display()))
        }
        "rename" => {
            let path = resolve(host, arg_str(args, "path")?)?;
            let new_name = arg_str(args, "new_name")?;
            crate::ops::check_name(new_name)?;
            if !host.ask("mcp__fileboat__rename", json!({ "question": "Rename this?", "summary": format!("{}\n→ {new_name}", path.display()) }))? {
                return Err("The user said no.".into());
            }
            let to = crate::ops::rename(&path, new_name)?;
            host.undoable(Op::Renamed { from: path.clone(), to: to.clone() }, format!("Rename “{}” to “{new_name}” (Claude)", name_of(&path)));
            Ok(format!("Renamed to {}.", to.display()))
        }
        "move" => {
            let paths = arg_paths(host, args, "paths")?;
            let to = resolve(host, arg_str(args, "to")?)?;
            if !to.is_dir() {
                return Err(format!("{} isn't a folder.", to.display()));
            }
            for p in &paths {
                if !p.exists() {
                    return Err(format!("{} doesn't exist.", p.display()));
                }
                if to.starts_with(p) {
                    return Err(format!("Can't move {} into itself.", p.display()));
                }
                if to.join(name_of(p)).exists() {
                    return Err(format!("{} already has a “{}”.", to.display(), name_of(p)));
                }
            }
            let list = paths.iter().map(|p| format!("- {}", p.display())).collect::<Vec<_>>().join("\n");
            if !host.ask("mcp__fileboat__move", json!({ "question": format!("Move {} into {}?", if paths.len() == 1 { "this".to_string() } else { format!("these {}", paths.len()) }, to.display()), "summary": list }))? {
                return Err("The user said no.".into());
            }
            let mut done = Vec::new();
            for p in &paths {
                let dest = to.join(name_of(p));
                if let Err(e) = std::fs::rename(p, &dest) {
                    if !done.is_empty() {
                        host.undoable(Op::Moved(done.clone()), format!("Move {} items (Claude)", done.len()));
                    }
                    return Err(format!("Couldn't move {} ({e}). Moving to another drive isn't possible from here; the user can drag it in Fileboat.", p.display()));
                }
                done.push((p.clone(), dest));
            }
            let n = done.len();
            host.undoable(Op::Moved(done), format!("Move {n} item{} into “{}” (Claude)", if n == 1 { "" } else { "s" }, name_of(&to)));
            Ok(format!("Moved {n} into {}.", to.display()))
        }
        "trash" => {
            let paths = arg_paths(host, args, "paths")?;
            if let Some(missing) = paths.iter().find(|p| !p.exists()) {
                return Err(format!("{} doesn't exist.", missing.display()));
            }
            let list = paths.iter().map(|p| format!("- {}", p.display())).collect::<Vec<_>>().join("\n");
            let bin = if cfg!(windows) { "the Recycle Bin" } else { "the Trash" };
            if !host.ask("mcp__fileboat__trash", json!({ "question": format!("Move {} to {bin}?", if paths.len() == 1 { "this".to_string() } else { format!("these {}", paths.len()) }), "summary": list }))? {
                return Err("The user said no.".into());
            }
            let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            crate::ops::delete::to_trash(&paths)?;
            let n = paths.len();
            host.undoable(Op::Trashed { paths, at }, format!("Delete {n} item{} (Claude)", if n == 1 { "" } else { "s" }));
            Ok(format!("Moved {n} to {bin}."))
        }
        other => Err(format!("Unknown tool {other}")),
    }
}

/// Claude Code's permission prompt: the host asks the user and waits.
fn approve(host: &dyn Host, args: &Value) -> Result<String, String> {
    let tool = args.get("tool_name").and_then(Value::as_str).unwrap_or("").to_string();
    let input = args.get("input").cloned().unwrap_or(json!({}));
    let allow = |input: Value| json!({ "behavior": "allow", "updatedInput": input }).to_string();
    // Tools that change something ask the user inside the tool itself, so they aren't asked twice.
    if READ_TOOLS.contains(&tool.as_str()) || ASKS_ITSELF.contains(&tool.as_str()) {
        return Ok(allow(input));
    }
    if host.ask(&tool, input.clone())? {
        Ok(allow(input))
    } else {
        Ok(json!({ "behavior": "deny", "message": "The user denied this in Fileboat." }).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        folder: PathBuf,
        allow: bool,
        asked: Mutex<Vec<String>>,
        undo: Mutex<Vec<String>>,
    }

    impl Host for Fake {
        fn context(&self) -> AiContext {
            AiContext { folder: Some(self.folder.display().to_string()), selection: vec!["a.txt".into()], ..Default::default() }
        }
        fn ask(&self, tool: &str, _input: Value) -> Result<bool, String> {
            self.asked.lock().unwrap().push(tool.to_string());
            Ok(self.allow)
        }
        fn ui(&self, _action: &str, _args: Value) -> Result<Value, String> {
            Err("no window".into())
        }
        fn search(&self, query: &str, _scope: Option<&str>, _limit: usize) -> Result<String, String> {
            Ok(format!("searched {query}"))
        }
        fn settings(&self) -> Result<Settings, String> {
            Ok(Settings::default())
        }
        fn save_settings(&self, _s: &Settings) -> Result<(), String> {
            Ok(())
        }
        fn undoable(&self, _op: Op, description: String) {
            self.undo.lock().unwrap().push(description);
        }
    }

    fn fake(name: &str, allow: bool) -> Fake {
        let folder = std::env::temp_dir().join(format!("fileboat-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        Fake { folder, allow, asked: Mutex::default(), undo: Mutex::default() }
    }

    #[test]
    fn file_tools_ask_first_and_can_be_undone() {
        let h = fake("files", true);
        let made = call_tool(&h, "create_file", &json!({ "path": "notes.md", "content": "# hi" })).unwrap();
        assert!(made.contains("notes.md"));
        assert_eq!(std::fs::read_to_string(h.folder.join("notes.md")).unwrap(), "# hi");
        assert!(call_tool(&h, "create_file", &json!({ "path": "notes.md" })).is_err(), "never replaces");
        call_tool(&h, "create_folder", &json!({ "path": "docs" })).unwrap();
        call_tool(&h, "move", &json!({ "paths": ["notes.md"], "to": "docs" })).unwrap();
        assert!(h.folder.join("docs/notes.md").is_file());
        call_tool(&h, "rename", &json!({ "path": "docs/notes.md", "new_name": "README.md" })).unwrap();
        assert!(h.folder.join("docs/README.md").is_file());
        assert!(call_tool(&h, "move", &json!({ "paths": ["docs"], "to": "docs" })).is_err(), "not into itself");
        assert_eq!(h.asked.lock().unwrap().len(), 4);
        assert_eq!(h.undo.lock().unwrap().len(), 4);

        let list = call_tool(&h, "list_folder", &json!({})).unwrap();
        assert!(list.contains("docs/") && list.contains("[folder]"));
        assert_eq!(call_tool(&h, "search_files", &json!({ "query": "main" })).unwrap(), "searched main");
        assert!(call_tool(&h, "get_context", &json!({})).unwrap().contains("a.txt"));
        std::fs::remove_dir_all(&h.folder).unwrap();
    }

    #[test]
    fn a_no_changes_nothing() {
        let h = fake("deny", false);
        assert!(call_tool(&h, "create_folder", &json!({ "path": "x" })).is_err());
        assert!(!h.folder.join("x").exists());
        assert!(h.undo.lock().unwrap().is_empty());
        std::fs::remove_dir_all(&h.folder).unwrap();
    }

    #[test]
    fn approve_passes_read_tools_and_asks_for_the_rest() {
        let h = fake("approve", false);
        let read = approve(&h, &json!({ "tool_name": "Read", "input": { "file_path": "x" } })).unwrap();
        assert!(read.contains("\"allow\""));
        let edit = approve(&h, &json!({ "tool_name": "Edit", "input": {} })).unwrap();
        assert!(edit.contains("\"deny\""));
        assert_eq!(*h.asked.lock().unwrap(), ["Edit"]);
        std::fs::remove_dir_all(&h.folder).unwrap();
    }

    #[test]
    fn dates() {
        assert_eq!(fmt_date(0.0), "1970-01-01 00:00 UTC");
        assert_eq!(fmt_date(1_700_000_000_000.0), "2023-11-14 22:13 UTC");
        assert_eq!(fmt_date(951_782_400_000.0), "2000-02-29 00:00 UTC");
    }
}
