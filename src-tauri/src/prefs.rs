//! User settings, stored as `settings.json` in the data folder (see `paths::data_dir`).
//!
//! The file is meant to be editable by hand. Unknown or missing keys fall back to
//! defaults; a file that can't be parsed is moved aside (never silently overwritten).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The Details view's columns (widths in `columnWidths`).
const COLUMNS: [&str; 4] = ["name", "modified", "type", "size"];
const DEFAULT_LIGHT_THEME: &str = "fileboat-light";
const DEFAULT_DARK_THEME: &str = "fileboat-dark";

/// Is `id` a theme of this mode ("light" or "dark")?
fn theme_fits(id: &str, mode: &str) -> bool {
    crate::themes::mode_of(id) == Some(mode)
}

/// Refuse a `lightTheme` / `darkTheme` value that isn't a theme id of that mode, naming the
/// valid ones (rather than quietly falling back to the default, as a hand-edited file does).
fn check_theme_ids(patch: &Value) -> Result<(), String> {
    for (key, mode) in [("lightTheme", "light"), ("darkTheme", "dark")] {
        let Some(asked) = patch.get(key) else { continue };
        let id = asked.as_str().unwrap_or("");
        if theme_fits(id, mode) {
            continue;
        }
        let why = match crate::themes::mode_of(id) {
            Some(other) => format!("\"{id}\" is a {other} theme"),
            None => format!("there is no theme {asked}"),
        };
        let valid = crate::themes::ids_for(mode).join(", ");
        return Err(format!("\"{key}\" must be a {mode} theme id ({why}). {mode} themes: {valid}"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// The colour theme used in light mode: a light theme id from `src/themes/themes.json`.
    pub light_theme: String,
    /// The colour theme used in dark mode: a dark theme id from `src/themes/themes.json`.
    pub dark_theme: String,
    /// "compact" | "default" | "comfortable"
    pub density: String,
    /// Whole-UI zoom factor (Ctrl+= / Ctrl+-).
    pub ui_zoom: f64,
    pub show_sidebar: bool,
    pub sidebar_width: u32,
    /// The preview pane on the right.
    pub show_preview: bool,
    pub preview_width: u32,
    /// How folders show: "details" | "list".
    pub view_mode: String,
    /// "name" | "size" | "modified" | "created" | "type"
    pub sort_by: String,
    pub sort_descending: bool,
    /// Folders before files whatever the sort.
    pub folders_first: bool,
    /// Show files the system marks hidden (and dotfiles on macOS).
    pub show_hidden: bool,
    /// "none" | "type" | "modified" | "size" | "letter"
    pub group_by: String,
    /// Details view column widths in pixels, by column ("name", "modified", "type", "size").
    pub column_widths: BTreeMap<String, u32>,
    /// Folders pinned in the sidebar, in order.
    pub favourites: Vec<String>,
    /// "Open in editor": an editor id ("vscode", "cursor", "zed"…), "" for the first one found.
    pub editor: String,
    /// "Terminal here": a terminal id ("windows-terminal", "pwsh", "iterm"…), "" for the first found.
    pub terminal: String,
    /// Look for a new version on GitHub at start-up and once a day.
    pub check_updates: bool,
    /// A version the user chose to skip (no notice for it; a manual check still shows it).
    pub skipped_update: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            light_theme: DEFAULT_LIGHT_THEME.into(),
            dark_theme: DEFAULT_DARK_THEME.into(),
            density: "default".into(),
            ui_zoom: 1.0,
            show_sidebar: true,
            sidebar_width: 240,
            show_preview: false,
            preview_width: 360,
            view_mode: "details".into(),
            sort_by: "name".into(),
            sort_descending: false,
            folders_first: true,
            show_hidden: false,
            group_by: "none".into(),
            column_widths: BTreeMap::new(),
            favourites: Vec::new(),
            editor: String::new(),
            terminal: String::new(),
            check_updates: true,
            skipped_update: String::new(),
        }
    }
}

impl Settings {
    /// Clamp values a hand edit could have broken.
    pub fn sanitized(mut self) -> Self {
        let d = Settings::default();
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = d.theme;
        }
        if !theme_fits(&self.light_theme, "light") {
            self.light_theme = d.light_theme;
        }
        if !theme_fits(&self.dark_theme, "dark") {
            self.dark_theme = d.dark_theme;
        }
        if !matches!(self.density.as_str(), "compact" | "default" | "comfortable") {
            self.density = d.density;
        }
        if !self.ui_zoom.is_finite() {
            self.ui_zoom = d.ui_zoom;
        }
        self.ui_zoom = self.ui_zoom.clamp(0.5, 3.0);
        self.sidebar_width = self.sidebar_width.clamp(160, 900);
        self.preview_width = self.preview_width.clamp(240, 1600);
        if !matches!(self.view_mode.as_str(), "details" | "list") {
            self.view_mode = d.view_mode;
        }
        if !matches!(self.sort_by.as_str(), "name" | "size" | "modified" | "created" | "type") {
            self.sort_by = d.sort_by;
        }
        if !matches!(self.group_by.as_str(), "none" | "type" | "modified" | "size" | "letter") {
            self.group_by = d.group_by;
        }
        self.column_widths.retain(|k, _| COLUMNS.contains(&k.as_str()));
        for w in self.column_widths.values_mut() {
            *w = (*w).clamp(40, 2000);
        }
        let mut seen = std::collections::HashSet::new();
        self.favourites.retain(|p| !p.trim().is_empty() && seen.insert(p.to_lowercase()));
        self.favourites.truncate(200);
        self
    }

    /// Merge a partial settings object (`{"theme": "dark"}`) into these settings.
    /// Unknown keys and wrong types are refused; values out of range are clamped.
    pub fn merged(&self, patch: &Value) -> Result<Settings, String> {
        let mut merged = serde_json::to_value(self).map_err(|e| e.to_string())?;
        let (Some(target), Some(changes)) = (merged.as_object_mut(), patch.as_object()) else {
            return Err("settings patch must be an object".into());
        };
        for (key, value) in changes {
            if !target.contains_key(key) {
                return Err(format!("unknown setting \"{key}\""));
            }
            target.insert(key.clone(), value.clone());
        }
        let next: Settings = serde_json::from_value(merged).map_err(|e| format!("invalid setting: {e}"))?;
        Ok(next.sanitized())
    }
}

/// What each setting accepts (for Claude and, later, `fb settings`).
pub const HELP: &str = "theme: \"system\" | \"light\" | \"dark\" (\"system\" follows your system setting)
lightTheme: colour theme used in light mode: \"fileboat-light\" (default) | \"catppuccin-latte\" | \"github-light\" | \"one-light\" | \"rose-pine-dawn\" | \"gruvbox-light\" | \"kanagawa-lotus\" | \"hc-light\"
darkTheme: colour theme used in dark mode: \"fileboat-dark\" (default) | \"catppuccin-mocha\" | \"tokyo-night\" | \"github-dark\" | \"one-dark\" | \"rose-pine\" | \"gruvbox-dark\" | \"kanagawa-wave\" | \"nord\" | \"dracula\" | \"hc-dark\"
density: \"compact\" | \"default\" | \"comfortable\"
uiZoom: 0.5 to 3.0 (whole window zoom)
showSidebar, showPreview (the preview pane): true | false
sidebarWidth: 160 to 900; previewWidth: 240 to 1600 (pixels)
viewMode: \"details\" | \"list\" (how folders show)
sortBy: \"name\" | \"size\" | \"modified\" | \"created\" | \"type\"; sortDescending, foldersFirst, showHidden: true | false
groupBy: \"none\" | \"type\" | \"modified\" | \"size\" | \"letter\" (group rows under headers)
favourites: [\"C:\\projects\", \"~/work\"] (folders pinned in the sidebar)
editor: \"\" (the first found) | \"vscode\" | \"vscode-insiders\" | \"cursor\" | \"windsurf\" | \"zed\" | \"sublime\" | \"notepad++\" | \"jetbrains-idea\" (and other JetBrains IDEs) | \"notepad\" | \"textedit\"
terminal: \"\" (the first found) | \"windows-terminal\" | \"pwsh\" | \"powershell\" | \"cmd\" | \"git-bash\" | \"wsl\" | \"terminal\" | \"iterm\" | \"ghostty\" | \"warp\" | \"wezterm\"
columnWidths: {\"name\": 320, \"modified\": 150, \"type\": 140, \"size\": 90} (Details view, 40 to 2000 pixels each)
checkUpdates: look for a new Fileboat version at start-up and once a day
skippedUpdate: a version not to be told about again (\"\" = none)";

/// One changed setting: key, old value, new value.
pub type Change = (String, Value, Value);

impl Settings {
    /// Apply `patch` strictly: unknown keys, wrong types and values that would be clamped are
    /// refused (the app's own controls clamp quietly; a typed or asked-for value should not).
    /// Returns the new settings and what actually changes.
    pub fn checked_change(&self, patch: &Value) -> Result<(Settings, Vec<Change>), String> {
        check_theme_ids(patch)?;
        let next = self.merged(patch)?;
        let before = serde_json::to_value(self).map_err(|e| e.to_string())?;
        let after = serde_json::to_value(&next).map_err(|e| e.to_string())?;
        let same = |a: &Value, b: &Value| match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() < 1e-9,
            _ => a == b,
        };
        let mut changes = Vec::new();
        for (key, asked) in patch.as_object().into_iter().flatten() {
            let now = &after[key];
            if !same(asked, now) {
                return Err(format!("\"{key}\" can't be {asked} (allowed values would make it {now}). Allowed:\n{HELP}"));
            }
            if !same(&before[key], now) {
                changes.push((key.clone(), before[key].clone(), now.clone()));
            }
        }
        Ok((next, changes))
    }
}

/// `settings.json` in the data folder.
pub fn path() -> Result<PathBuf, String> {
    Ok(crate::paths::data_dir()?.join("settings.json"))
}

/// Read the settings (defaults when there's no file). A file that can't be parsed is moved
/// aside as `settings.invalid-<time>.json` and defaults are used.
pub fn load(path: &Path) -> Settings {
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s.sanitized(),
            Err(_) => {
                let invalid = path.with_file_name(format!("settings.invalid-{}.json", crate::paths::now_secs()));
                let _ = fs::rename(path, invalid);
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

/// Save atomically (a crash mid-write can't leave a half-written file).
pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    crate::paths::write_atomic(path, &text).map_err(|e| format!("could not save settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// `settings.json` with `text`, alone in a new temporary folder.
    fn temp_file(text: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fileboat-prefs-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn theme_ids_default_to_fileboat() {
        let d = Settings::default();
        assert_eq!((d.light_theme.as_str(), d.dark_theme.as_str()), ("fileboat-light", "fileboat-dark"));
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["lightTheme"], "fileboat-light");
        assert_eq!(v["darkTheme"], "fileboat-dark");
    }

    #[test]
    fn partial_files_and_bad_theme_ids_load_with_defaults() {
        let p = temp_file(r#"{ "theme": "dark", "sidebarWidth": 300 }"#);
        let s = load(&p);
        assert_eq!((s.theme.as_str(), s.sidebar_width), ("dark", 300));
        assert_eq!((s.light_theme.as_str(), s.dark_theme.as_str()), ("fileboat-light", "fileboat-dark"));
        // Hand-edited ids that don't exist or have the wrong mode fall back; good ones stay.
        fs::write(&p, r#"{ "lightTheme": "catppuccin-mocha", "darkTheme": "nope" }"#).unwrap();
        let s = load(&p);
        assert_eq!((s.light_theme.as_str(), s.dark_theme.as_str()), ("fileboat-light", "fileboat-dark"));
        fs::write(&p, r#"{ "lightTheme": "github-light", "darkTheme": "tokyo-night" }"#).unwrap();
        let s = load(&p);
        assert_eq!((s.light_theme.as_str(), s.dark_theme.as_str()), ("github-light", "tokyo-night"));
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn unreadable_file_is_moved_aside() {
        let p = temp_file("{ not json");
        let s = load(&p);
        assert_eq!(s.theme, "system");
        assert!(!p.exists(), "the broken file is renamed, not overwritten");
        let dir = p.parent().unwrap();
        let kept: Vec<String> = fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(kept.len() == 1 && kept[0].starts_with("settings.invalid-"), "{kept:?}");
        assert_eq!(fs::read_to_string(dir.join(&kept[0])).unwrap(), "{ not json");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn values_are_clamped_and_unknown_keys_refused() {
        let s = Settings::default();
        assert_eq!(s.merged(&json!({ "sidebarWidth": 5 })).unwrap().sidebar_width, 160);
        assert_eq!(s.merged(&json!({ "uiZoom": 9.0 })).unwrap().ui_zoom, 3.0);
        assert!(s.merged(&json!({ "nope": 1 })).unwrap_err().contains("unknown setting"));
        assert!(s.merged(&json!({ "showSidebar": "yes" })).is_err());
        // Strict changes refuse what would be clamped.
        assert!(s.checked_change(&json!({ "sidebarWidth": 5 })).is_err());
        let (_, changes) = s.checked_change(&json!({ "showPreview": true, "density": "default" })).unwrap();
        assert_eq!(changes, vec![("showPreview".to_string(), json!(false), json!(true))]);
    }

    #[test]
    fn view_settings_are_checked() {
        let s = Settings::default();
        assert_eq!((s.view_mode.as_str(), s.sort_by.as_str(), s.folders_first, s.show_hidden), ("details", "name", true, false));
        let next = s.merged(&json!({ "viewMode": "grid", "sortBy": "colour", "columnWidths": { "name": 5, "nope": 100, "size": 120 } })).unwrap();
        assert_eq!((next.view_mode.as_str(), next.sort_by.as_str()), ("details", "name"));
        assert_eq!(next.column_widths, BTreeMap::from([("name".to_string(), 40), ("size".to_string(), 120)]));
        assert!(s.checked_change(&json!({ "sortBy": "size", "sortDescending": true })).is_ok());
    }

    #[test]
    fn theme_ids_are_checked_by_mode() {
        let s = Settings::default();
        let (next, changes) = s.checked_change(&json!({ "darkTheme": "catppuccin-mocha" })).unwrap();
        assert_eq!(next.dark_theme, "catppuccin-mocha");
        assert_eq!(changes, vec![("darkTheme".to_string(), json!("fileboat-dark"), json!("catppuccin-mocha"))]);

        let err = s.checked_change(&json!({ "darkTheme": "catppuccin-latte" })).unwrap_err();
        assert!(err.contains("\"catppuccin-latte\" is a light theme") && err.contains("fileboat-dark, catppuccin-mocha"), "{err}");
        assert!(!err.contains("fileboat-light"), "only dark ids are listed: {err}");
        let err = s.checked_change(&json!({ "lightTheme": "solarized" })).unwrap_err();
        assert!(err.contains("no theme \"solarized\"") && err.contains("fileboat-light, catppuccin-latte"), "{err}");
        assert!(s.checked_change(&json!({ "lightTheme": 5 })).is_err());
    }

    #[test]
    fn help_lists_every_theme_id() {
        for (key, mode) in [("lightTheme", "light"), ("darkTheme", "dark")] {
            let line = HELP.lines().find(|l| l.starts_with(&format!("{key}:"))).expect("a HELP line");
            let listed: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
            assert_eq!(listed, crate::themes::ids_for(mode), "HELP's {key} line must list the {mode} themes of themes.json");
        }
    }

    #[test]
    fn help_names_every_setting() {
        let keys = serde_json::to_value(Settings::default()).unwrap();
        for key in keys.as_object().unwrap().keys() {
            assert!(HELP.contains(key.as_str()), "HELP doesn't describe \"{key}\"");
        }
    }
}
