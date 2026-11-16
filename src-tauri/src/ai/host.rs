//! The app's side of the MCP tool server: one host per window. Permission questions become
//! Allow / Deny cards in that window's Claude pane; window actions go to its UI
//! (src/lib/agentBridge.ts).

use super::ClaudeHub;
use fileboat_core::claude::AiContext;
use fileboat_core::mcp::{Host, Resolver};
use fileboat_core::ops::undo::{Op, UndoLog};
use fileboat_core::prefs::Settings;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub struct GuiHost {
    app: AppHandle,
    window: String,
}

/// Requests to `/mcp/<window>` are served for that window.
pub fn resolver(app: AppHandle) -> Resolver {
    Arc::new(move |window: &str| {
        // Only for windows that exist.
        app.get_webview_window(window)?;
        let host: Arc<dyn Host> = Arc::new(GuiHost { app: app.clone(), window: window.to_string() });
        Some(host)
    })
}

impl Host for GuiHost {
    fn context(&self) -> AiContext {
        self.app.state::<ClaudeHub>().context(&self.window)
    }

    /// Show an Allow / Deny card in the window's Claude pane and wait for the answer.
    fn ask(&self, tool: &str, input: Value) -> Result<bool, String> {
        let hub = self.app.state::<ClaudeHub>();
        if hub.is_always_allowed(&self.window, tool) {
            return Ok(true);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        hub.approvals.lock().unwrap().insert(id.clone(), tx);
        let _ = self.app.emit_to(&self.window, "claude:permission", json!({ "id": id, "toolName": tool, "input": input }));
        match rx.recv_timeout(Duration::from_secs(15 * 60)) {
            Ok((allowed, always)) => {
                if allowed && always {
                    hub.allow_always(&self.window, tool);
                }
                Ok(allowed)
            }
            Err(_) => {
                hub.approvals.lock().unwrap().remove(&id);
                Ok(false)
            }
        }
    }

    /// Ask this window's UI to do something and wait for its answer.
    fn ui(&self, action: &str, args: Value) -> Result<Value, String> {
        let hub = self.app.state::<ClaudeHub>();
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        hub.ui_calls.lock().unwrap().insert(id.clone(), tx);
        self.app.emit_to(&self.window, "ui:request", json!({ "id": id, "action": action, "args": args })).map_err(|e| e.to_string())?;
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(result) => result,
            Err(_) => {
                hub.ui_calls.lock().unwrap().remove(&id);
                Err("The window didn't answer (is it still open?).".into())
            }
        }
    }

    fn search(&self, query: &str, scope: Option<&str>, limit: usize) -> Result<String, String> {
        let indexer = self.app.state::<Arc<crate::indexer::Indexer>>();
        let r = indexer.search(query, scope.map(std::path::Path::new), limit)?;
        let status = indexer.status();
        let mut out = format!("{} match{}{}", r.total, if r.total == 1 { "" } else { "es" }, if r.total > r.hits.len() { format!(", the best {} shown", r.hits.len()) } else { String::new() });
        if scope.is_none() {
            out.push_str(&format!(" (index of {}; state: {})", status.roots.join(", "), status.phase));
        }
        out.push('\n');
        if !r.unsupported.is_empty() {
            out.push_str(&format!("Not supported here: {}\n", r.unsupported.join(", ")));
        }
        for h in &r.hits {
            out.push_str(&format!("{}{}\n", h.path, if h.folder { std::path::MAIN_SEPARATOR_STR } else { "" }));
        }
        Ok(out)
    }

    fn settings(&self) -> Result<Settings, String> {
        Ok(self.app.state::<crate::settings::SettingsStore>().current())
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        self.app.state::<crate::settings::SettingsStore>().replace(&self.app, settings.clone())
    }

    fn undoable(&self, op: Op, description: String) {
        self.app.state::<UndoLog>().push(op, description);
    }
}
