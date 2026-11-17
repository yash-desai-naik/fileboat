//! Editors and terminals installed on this computer, for "Open in editor" and "Terminal here".
//! Found by where they install (no registry scans, no PATH walking), so detection is instant.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    program: PathBuf,
    /// Arguments before the path(s); `{dir}` in one is replaced by the folder (terminals).
    #[serde(skip)]
    args: Vec<String>,
    /// A console program that needs a window of its own (Windows), started in the folder.
    #[serde(skip)]
    console: bool,
    /// A batch script (JetBrains Toolbox): run without a console window flashing.
    #[serde(skip)]
    script: bool,
}

fn tool(id: &str, name: &str, program: PathBuf, args: &[&str]) -> Tool {
    Tool { id: id.into(), name: name.into(), program, args: args.iter().map(|s| s.to_string()).collect(), console: false, script: false }
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

#[cfg(windows)]
pub fn editors() -> Vec<Tool> {
    let local = env_path("LOCALAPPDATA").unwrap_or_default();
    let pf = env_path("ProgramFiles").unwrap_or_else(|| PathBuf::from("C:\\Program Files"));
    let pf86 = env_path("ProgramFiles(x86)").unwrap_or_else(|| PathBuf::from("C:\\Program Files (x86)"));
    let candidates: Vec<(&str, &str, Vec<PathBuf>)> = vec![
        ("vscode", "VS Code", vec![local.join("Programs\\Microsoft VS Code\\Code.exe"), pf.join("Microsoft VS Code\\Code.exe")]),
        (
            "vscode-insiders",
            "VS Code Insiders",
            vec![local.join("Programs\\Microsoft VS Code Insiders\\Code - Insiders.exe"), pf.join("Microsoft VS Code Insiders\\Code - Insiders.exe")],
        ),
        ("cursor", "Cursor", vec![local.join("Programs\\cursor\\Cursor.exe")]),
        ("windsurf", "Windsurf", vec![local.join("Programs\\Windsurf\\Windsurf.exe")]),
        ("zed", "Zed", vec![local.join("Programs\\Zed\\Zed.exe"), pf.join("Zed\\Zed.exe")]),
        ("sublime", "Sublime Text", vec![pf.join("Sublime Text\\sublime_text.exe"), pf.join("Sublime Text 3\\sublime_text.exe")]),
        ("notepad++", "Notepad++", vec![pf.join("Notepad++\\notepad++.exe"), pf86.join("Notepad++\\notepad++.exe")]),
    ];
    let mut out: Vec<Tool> = candidates
        .into_iter()
        .filter_map(|(id, name, paths)| paths.into_iter().find(|p| p.is_file()).map(|p| tool(id, name, p, &[])))
        .collect();
    // JetBrains IDEs installed with the Toolbox app, through its launcher scripts.
    let toolbox = local.join("JetBrains\\Toolbox\\scripts");
    let mut ides: Vec<Tool> = std::fs::read_dir(&toolbox)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "cmd" {
                return None;
            }
            let stem = path.file_stem()?.to_string_lossy().to_lowercase();
            let name = jetbrains_name(&stem)?;
            Some(Tool { script: true, ..tool(&format!("jetbrains-{stem}"), name, path, &[]) })
        })
        .collect();
    ides.sort_by(|a, b| a.name.cmp(&b.name));
    out.extend(ides);
    if let Some(windir) = env_path("SystemRoot") {
        out.push(tool("notepad", "Notepad", windir.join("System32\\notepad.exe"), &[]));
    }
    out
}

fn jetbrains_name(stem: &str) -> Option<&'static str> {
    Some(match stem {
        "idea" | "idea64" => "IntelliJ IDEA",
        "webstorm" | "webstorm64" => "WebStorm",
        "pycharm" | "pycharm64" => "PyCharm",
        "rustrover" | "rustrover64" => "RustRover",
        "goland" | "goland64" => "GoLand",
        "rider" | "rider64" => "Rider",
        "clion" | "clion64" => "CLion",
        "phpstorm" | "phpstorm64" => "PhpStorm",
        "datagrip" | "datagrip64" => "DataGrip",
        "rubymine" | "rubymine64" => "RubyMine",
        "fleet" => "Fleet",
        _ => return None,
    })
}

#[cfg(windows)]
pub fn terminals() -> Vec<Tool> {
    let local = env_path("LOCALAPPDATA").unwrap_or_default();
    let pf = env_path("ProgramFiles").unwrap_or_else(|| PathBuf::from("C:\\Program Files"));
    let sys = env_path("SystemRoot").unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("System32");
    let console = |t: Tool| Tool { console: true, ..t };
    let candidates = vec![
        tool("windows-terminal", "Windows Terminal", local.join("Microsoft\\WindowsApps\\wt.exe"), &["-d", "{dir}"]),
        console(tool("pwsh", "PowerShell", pf.join("PowerShell\\7\\pwsh.exe"), &["-NoExit"])),
        console(tool("powershell", "Windows PowerShell", sys.join("WindowsPowerShell\\v1.0\\powershell.exe"), &["-NoExit"])),
        console(tool("cmd", "Command Prompt", sys.join("cmd.exe"), &["/K"])),
        tool("git-bash", "Git Bash", pf.join("Git\\git-bash.exe"), &["--cd={dir}"]),
        // Git installed for the current user only.
        tool("git-bash", "Git Bash", local.join("Programs\\Git\\git-bash.exe"), &["--cd={dir}"]),
        console(tool("wsl", "WSL", sys.join("wsl.exe"), &["--cd", "{dir}"])),
    ];
    let mut found: Vec<Tool> = candidates.into_iter().filter(|t| t.program.is_file()).collect();
    // One entry per terminal, even if it's installed in two places.
    let mut seen = std::collections::HashSet::new();
    found.retain(|t| seen.insert(t.id.clone()));
    found
}

/// macOS: apps opened with `open -a`.
#[cfg(not(windows))]
fn mac_apps(list: &[(&str, &str, &str)]) -> Vec<Tool> {
    let home_apps = crate::paths::home_dir().map(|h| h.join("Applications")).unwrap_or_default();
    list.iter()
        .filter_map(|(id, name, app)| {
            [PathBuf::from("/Applications"), PathBuf::from("/System/Applications/Utilities"), PathBuf::from("/System/Applications"), home_apps.clone()]
                .into_iter()
                .map(|dir| dir.join(app))
                .find(|p| p.exists())
                .map(|p| tool(id, name, PathBuf::from("open"), &["-a", &p.to_string_lossy()]))
        })
        .collect()
}

#[cfg(not(windows))]
pub fn editors() -> Vec<Tool> {
    mac_apps(&[
        ("vscode", "VS Code", "Visual Studio Code.app"),
        ("vscode-insiders", "VS Code Insiders", "Visual Studio Code - Insiders.app"),
        ("cursor", "Cursor", "Cursor.app"),
        ("windsurf", "Windsurf", "Windsurf.app"),
        ("zed", "Zed", "Zed.app"),
        ("sublime", "Sublime Text", "Sublime Text.app"),
        ("jetbrains-idea", "IntelliJ IDEA", "IntelliJ IDEA.app"),
        ("jetbrains-webstorm", "WebStorm", "WebStorm.app"),
        ("jetbrains-pycharm", "PyCharm", "PyCharm.app"),
        ("jetbrains-rustrover", "RustRover", "RustRover.app"),
        ("jetbrains-goland", "GoLand", "GoLand.app"),
        ("textedit", "TextEdit", "TextEdit.app"),
    ])
}

#[cfg(not(windows))]
pub fn terminals() -> Vec<Tool> {
    mac_apps(&[
        ("terminal", "Terminal", "Terminal.app"),
        ("iterm", "iTerm", "iTerm.app"),
        ("ghostty", "Ghostty", "Ghostty.app"),
        ("warp", "Warp", "Warp.app"),
        ("wezterm", "WezTerm", "WezTerm.app"),
    ])
}

/// The tool with this id, else the first one found.
pub fn pick<'a>(tools: &'a [Tool], id: &str) -> Option<&'a Tool> {
    tools.iter().find(|t| t.id == id).or_else(|| tools.first())
}

fn spawn(t: &Tool, args: Vec<String>, cwd: &Path) -> Result<(), String> {
    let mut cmd = if t.script {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(&t.program);
        c
    } else {
        std::process::Command::new(&t.program)
    };
    cmd.args(args).current_dir(cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x10;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if t.console {
            cmd.creation_flags(CREATE_NEW_CONSOLE);
        } else if t.script {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Couldn't start {}: {e}", t.name))
}

/// Open files or folders in an editor.
pub fn open_in_editor(t: &Tool, paths: &[PathBuf]) -> Result<(), String> {
    let cwd = paths.first().and_then(|p| if p.is_dir() { Some(p.as_path()) } else { p.parent() }).unwrap_or(Path::new("."));
    let mut args = t.args.clone();
    args.extend(paths.iter().map(|p| p.display().to_string()));
    spawn(t, args, cwd)
}

/// Open one file at a line (and column) in an editor that can be told where: VS Code and its
/// kin, Zed, Sublime Text, Notepad++ and JetBrains IDEs. Others just open the file.
pub fn open_at_line(t: &Tool, path: &Path, line: u32, column: u32) -> Result<(), String> {
    let cwd = path.parent().unwrap_or(Path::new("."));
    let shown = path.display().to_string();
    let (line, column) = (line.max(1), column.max(1));
    let at: Option<Vec<String>> = match t.id.as_str() {
        "vscode" | "vscode-insiders" | "cursor" | "windsurf" | "vscodium" => {
            if cfg!(target_os = "macos") {
                // `open -a` can't pass arguments to an app that's already running: use its URL.
                let scheme = t.id.as_str();
                return std::process::Command::new("open")
                    .arg(format!("{scheme}://file{}:{line}:{column}", if shown.starts_with('/') { shown.clone() } else { format!("/{shown}") }))
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| format!("Couldn't start {}: {e}", t.name));
            }
            Some(vec!["-g".into(), format!("{shown}:{line}:{column}")])
        }
        "zed" | "sublime" if cfg!(windows) => Some(vec![format!("{shown}:{line}:{column}")]),
        "notepad++" => Some(vec![format!("-n{line}"), format!("-c{column}"), shown.clone()]),
        id if id.starts_with("jetbrains-") && cfg!(windows) => Some(vec!["--line".into(), line.to_string(), "--column".into(), column.to_string(), shown.clone()]),
        _ => None,
    };
    let mut args = t.args.clone();
    args.extend(at.unwrap_or_else(|| vec![shown]));
    spawn(t, args, cwd)
}

/// Open a terminal in a folder.
pub fn open_terminal(t: &Tool, dir: &Path) -> Result<(), String> {
    let shown = dir.display().to_string();
    let mut args: Vec<String> = t.args.iter().map(|a| a.replace("{dir}", &shown)).collect();
    if cfg!(not(windows)) {
        args.push(shown);
    }
    spawn(t, args, dir)
}

#[cfg(test)]
mod tests {
    #[test]
    fn detection_finds_a_default() {
        let editors = super::editors();
        if cfg!(windows) {
            assert!(editors.iter().any(|e| e.id == "notepad"), "Notepad is always there");
            assert!(super::terminals().iter().any(|t| t.id == "cmd"), "so is the Command Prompt");
        }
        let first = editors.first().map(|e| e.id.clone());
        assert_eq!(super::pick(&editors, "no-such-editor").map(|e| e.id.clone()), first);
        assert_eq!(super::jetbrains_name("rustrover64"), Some("RustRover"));
    }

    #[test]
    fn launching_starts_the_program_in_the_folder() {
        let dir = std::env::temp_dir();
        let (program, args) = if cfg!(windows) { ("cmd", vec!["/C", "exit"]) } else { ("true", vec![]) };
        let t = super::tool("test", "Test", std::path::PathBuf::from(program), &args);
        assert!(super::open_in_editor(&t, std::slice::from_ref(&dir)).is_ok());
        let missing = super::tool("nope", "Nope", dir.join("no-such-program.exe"), &[]);
        assert!(super::open_terminal(&missing, &dir).unwrap_err().contains("Couldn't start Nope"));
    }
}
