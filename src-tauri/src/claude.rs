//! Claude Code helpers for the Claude pane (from Tableboat): finding `claude`, the arguments it
//! runs with, Fileboat's prompt and which of Fileboat's tools ask first.
//!
//! Claude Code runs in the repository root of the folder you're in (or the folder itself
//! outside a repository), so its project, `CLAUDE.md`, chats and memory are the same ones your
//! terminal and IDE use. Fileboat never writes anything into that folder.

use serde_json::json;
use std::path::{Path, PathBuf};

/// What Fileboat tells Claude about what the user has open (the `get_context` tool).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiContext {
    /// The active tab's folder.
    pub folder: Option<String>,
    /// The selected items' full paths (the first 200).
    pub selection: Vec<String>,
    /// The file the preview pane shows.
    pub preview: Option<String>,
    /// How the folder shows ("details", "list", "columns").
    pub view: Option<String>,
    /// What's typed in the search box, if a search is open.
    pub search: Option<String>,
}

/// Where Claude Code works for `folder`: its repository's root, or the folder itself.
pub fn working_folder(folder: &Path) -> PathBuf {
    crate::git::discover(folder).map(|r| r.root).unwrap_or_else(|| folder.to_path_buf())
}

/// The Claude Code program on PATH or in a usual install folder.
pub fn find_claude() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    if let Some(path) = std::env::var_os("PATH")
        && let Some(found) = find_in(&path, names)
    {
        return Some(found);
    }
    // A macOS app started from the Dock or Finder gets the system's minimal PATH, not the
    // user's: ask their login shell where things really are (once per run).
    #[cfg(target_os = "macos")]
    if let Some(path) = login_shell_path()
        && let Some(found) = find_in(&path, names)
    {
        return Some(found);
    }
    let home = dirs::home_dir();
    let exe = names[0];
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = &home {
        candidates.push(home.join(".local/bin").join(exe));
        candidates.push(home.join(".claude/local").join(exe));
        candidates.push(home.join(".bun/bin").join(exe));
        candidates.push(home.join(".volta/bin").join(exe));
    }
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(exe));
        candidates.push(PathBuf::from("/usr/local/bin").join(exe));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn find_in(path: &std::ffi::OsStr, names: &[&str]) -> Option<PathBuf> {
    std::env::split_paths(path).find_map(|dir| names.iter().map(|n| dir.join(n)).find(|p| p.is_file()))
}

#[cfg(target_os = "macos")]
fn login_shell_path() -> Option<std::ffi::OsString> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Option<std::ffi::OsString>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            let out = std::process::Command::new(shell).args(["-l", "-c", "printf %s \"$PATH\""]).output().ok()?;
            let path = String::from_utf8(out.stdout).ok().filter(|p| out.status.success() && !p.trim().is_empty())?;
            Some(std::ffi::OsString::from(path))
        })
        .clone()
}

pub const NOT_INSTALLED: &str = "Claude Code (claude) wasn't found. Install it from https://claude.com/claude-code and sign in once in a terminal.";

/// Claude Code keeps a project's chats in ~/.claude/projects/<path with non-alphanumerics as '-'>.
pub fn sessions_store(folder: &Path) -> Option<PathBuf> {
    let encoded: String = folder.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    Some(dirs::home_dir()?.join(".claude").join("projects").join(encoded))
}

/// The "model" in Claude Code's user settings, if any.
pub fn default_model() -> Option<String> {
    let text = std::fs::read_to_string(dirs::home_dir()?.join(".claude").join("settings.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let model = value.get("model")?.as_str()?.trim();
    (!model.is_empty()).then(|| model.to_string())
}

/// For the Claude pane.
pub fn pane_prompt() -> String {
    "You are running inside Fileboat, a file explorer for developers, in a side pane next to the folder the user \
     is browsing. Your working directory is that folder's git repository (or the folder itself). Use the fileboat \
     MCP tools to see what the user sees and to work in the app: get_context (the folder, the selected items, the \
     previewed file, the view and any search), search_files (Fileboat's index of file and folder names, in \
     milliseconds; filters ext:rs,ts kind:image size:>10mb modified:<7d in:src; scope it to a folder), list_folder \
     (a folder's items with sizes and dates), git_status, reveal (show a file in Fileboat, selected), select (select \
     items in the folder shown), list_app_commands and run_app_command (everything in Fileboat's command bar), \
     get_settings and update_settings. create_folder, create_file, rename, move, trash and open_in_app change \
     things and ask the user first (they can be undone with Ctrl+Z), so call them directly instead of asking in the \
     chat. Your own Read, Glob and Grep tools work as usual; Edit, Write and Bash ask the user. When you mention \
     files, give their paths so the user can find them. Keep answers short and practical."
        .into()
}

/// Tools Claude may use without asking: they only read, or only change what the window shows.
pub const READ_TOOLS: &[&str] = &[
    "mcp__fileboat__get_context",
    "mcp__fileboat__search_files",
    "mcp__fileboat__list_folder",
    "mcp__fileboat__git_status",
    "mcp__fileboat__reveal",
    "mcp__fileboat__select",
    "mcp__fileboat__list_app_commands",
    "mcp__fileboat__get_settings",
    "Read",
    "Glob",
    "Grep",
    "LS",
];

/// Tools that change something: each asks the user itself (with a readable summary), so
/// `approve` lets them through without asking twice.
pub const ASKS_ITSELF: &[&str] = &[
    "mcp__fileboat__create_folder",
    "mcp__fileboat__create_file",
    "mcp__fileboat__rename",
    "mcp__fileboat__move",
    "mcp__fileboat__trash",
    "mcp__fileboat__open_in_app",
    "mcp__fileboat__run_app_command",
    "mcp__fileboat__update_settings",
];

/// The MCP config file that points Claude Code at Fileboat's tool server.
pub fn write_mcp_config(path: &Path, port: u16, token: &str, route: &str) -> Result<(), String> {
    let config = json!({
        "mcpServers": {
            "fileboat": {
                "type": "http",
                "url": format!("http://127.0.0.1:{port}/mcp/{route}"),
                "headers": { "Authorization": format!("Bearer {token}") }
            }
        }
    });
    std::fs::write(path, config.to_string()).map_err(|e| e.to_string())
}

/// The arguments for every Claude Code run from Fileboat: only Fileboat's MCP server, read tools
/// pre-allowed, every other permission question sent to `approve`, and the model if chosen.
pub fn base_args(config_path: &Path, system_prompt: &str, model: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--mcp-config".into(),
        config_path.display().to_string(),
        // Only Fileboat's server: the user's other servers would cost memory and start-up time.
        "--strict-mcp-config".into(),
        "--allowedTools".into(),
        READ_TOOLS.join(" "),
        "--permission-prompt-tool".into(),
        "mcp__fileboat__approve".into(),
        // Ask for everything not pre-allowed, whatever the user's own default mode is: the
        // questions come to the pane.
        "--permission-mode".into(),
        "manual".into(),
        "--append-system-prompt".into(),
        system_prompt.into(),
    ];
    let model = model.trim();
    if !model.is_empty() && model.chars().all(|c| c.is_ascii_alphanumeric() || "-._[]".contains(c)) {
        args.push("--model".into());
        args.push(model.into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works_in_the_repository_root() {
        let dir = std::env::temp_dir().join(format!("fileboat-claude-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo/.git")).unwrap();
        std::fs::write(dir.join("repo/.git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::create_dir_all(dir.join("repo/src/deep")).unwrap();
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        assert_eq!(working_folder(&dir.join("repo/src/deep")), dir.join("repo"));
        assert_eq!(working_folder(&dir.join("plain")), dir.join("plain"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn arguments_route_permissions_to_the_pane() {
        let args = base_args(Path::new("cfg.json"), "prompt", "opus; rm -rf /");
        assert!(args.windows(2).any(|w| w[0] == "--permission-prompt-tool" && w[1] == "mcp__fileboat__approve"));
        assert!(!args.contains(&"--model".to_string()), "a model name with odd characters is ignored");
        assert!(base_args(Path::new("c"), "p", "claude-opus-5-5").ends_with(&["--model".into(), "claude-opus-5-5".into()]));
    }
}
