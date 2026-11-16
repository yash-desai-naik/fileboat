//! What the index leaves out: folders a developer never wants to search (dependencies, build
//! output, environments and caches, version control internals, the system), each a rule with an
//! id that can be switched off. Some only apply in context: `vendor` beside a `composer.json` or
//! `go.mod`, `bin`/`obj` beside a .NET project file, `env` when it holds a Python environment,
//! `build`/`dist`/`out` inside a project.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// One rule, as the settings list it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: &'static str,
    pub group: &'static str,
    /// When it applies, if not always.
    pub note: &'static str,
}

/// When a named folder is left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    Always,
    /// Beside one of these files in the same folder.
    Beside(&'static [&'static str]),
    /// Beside a file with one of these extensions (`.csproj`…).
    BesideExt(&'static [&'static str]),
    /// When it contains this file.
    Contains(&'static str),
}

const PROJECT: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "go.mod",
    "pyproject.toml",
    "setup.py",
    "CMakeLists.txt",
    "Makefile",
    "composer.json",
    "pubspec.yaml",
    "mix.exs",
    "deno.json",
];
const DOTNET: &[&str] = &["csproj", "fsproj", "vbproj", "sln"];

/// (id, group, when). The id is the folder's name, or `parent/name` for nested ones.
const NAMED: &[(&str, &str, When)] = &[
    // Dependencies and build output
    ("node_modules", "Dependencies and build output", When::Always),
    ("bower_components", "Dependencies and build output", When::Always),
    ("jspm_packages", "Dependencies and build output", When::Always),
    ("vendor", "Dependencies and build output", When::Beside(&["composer.json", "go.mod"])),
    ("target", "Dependencies and build output", When::Beside(&["Cargo.toml", "pom.xml"])),
    ("build", "Dependencies and build output", When::Beside(PROJECT)),
    ("dist", "Dependencies and build output", When::Beside(PROJECT)),
    ("out", "Dependencies and build output", When::Beside(PROJECT)),
    ("bin", "Dependencies and build output", When::BesideExt(DOTNET)),
    ("obj", "Dependencies and build output", When::BesideExt(DOTNET)),
    (".next", "Dependencies and build output", When::Always),
    (".nuxt", "Dependencies and build output", When::Always),
    (".svelte-kit", "Dependencies and build output", When::Always),
    (".astro", "Dependencies and build output", When::Always),
    (".turbo", "Dependencies and build output", When::Always),
    (".parcel-cache", "Dependencies and build output", When::Always),
    (".vite", "Dependencies and build output", When::Always),
    (".angular", "Dependencies and build output", When::Always),
    ("Pods", "Dependencies and build output", When::Always),
    ("DerivedData", "Dependencies and build output", When::Always),
    (".gradle", "Dependencies and build output", When::Always),
    (".dart_tool", "Dependencies and build output", When::Always),
    (".pub-cache", "Dependencies and build output", When::Always),
    (".terraform", "Dependencies and build output", When::Always),
    (".serverless", "Dependencies and build output", When::Always),
    ("coverage", "Dependencies and build output", When::Beside(PROJECT)),
    (".nyc_output", "Dependencies and build output", When::Always),
    // Environments and caches
    (".venv", "Environments and caches", When::Always),
    ("venv", "Environments and caches", When::Always),
    ("env", "Environments and caches", When::Contains("pyvenv.cfg")),
    ("__pycache__", "Environments and caches", When::Always),
    (".tox", "Environments and caches", When::Always),
    (".nox", "Environments and caches", When::Always),
    (".mypy_cache", "Environments and caches", When::Always),
    (".pytest_cache", "Environments and caches", When::Always),
    (".ruff_cache", "Environments and caches", When::Always),
    (".ipynb_checkpoints", "Environments and caches", When::Always),
    (".cache", "Environments and caches", When::Always),
    (".eslintcache", "Environments and caches", When::Always),
    (".stack-work", "Environments and caches", When::Always),
    (".cargo/registry", "Environments and caches", When::Always),
    (".rustup", "Environments and caches", When::Always),
    (".npm", "Environments and caches", When::Always),
    (".yarn/cache", "Environments and caches", When::Always),
    (".pnpm-store", "Environments and caches", When::Always),
    (".m2", "Environments and caches", When::Always),
    (".nuget/packages", "Environments and caches", When::Always),
    ("pkg/mod", "Environments and caches", When::Always),
    (".bun/install", "Environments and caches", When::Always),
    (".deno", "Environments and caches", When::Always),
    (".nvm", "Environments and caches", When::Always),
    (".conda/pkgs", "Environments and caches", When::Always),
    (".android", "Environments and caches", When::Always),
    (".vscode/extensions", "Environments and caches", When::Always),
    (".vscode-insiders/extensions", "Environments and caches", When::Always),
    (".cursor/extensions", "Environments and caches", When::Always),
    (".windsurf/extensions", "Environments and caches", When::Always),
    // Version control internals
    (".git", "Version control internals", When::Always),
    (".svn", "Version control internals", When::Always),
    (".hg", "Version control internals", When::Always),
    // System (by name, anywhere)
    ("$Recycle.Bin", "System", When::Always),
    ("System Volume Information", "System", When::Always),
    (".Trashes", "System", When::Always),
    (".Spotlight-V100", "System", When::Always),
    (".fseventsd", "System", When::Always),
];

/// System places, by path (case doesn't matter), as `system:<path>` rules.
fn system_paths() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let home = dirs::home_dir();
    if cfg!(windows) {
        let win = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        out.push(("Windows".into(), win));
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"] {
            if let Some(p) = std::env::var_os(var) {
                out.push((var.into(), PathBuf::from(p)));
            }
        }
        if let Some(h) = &home {
            out.push(("AppData".into(), h.join("AppData")));
        }
    } else {
        for p in ["/System", "/Library", "/private", "/Volumes", "/dev", "/proc", "/sys"] {
            out.push((p.trim_start_matches('/').into(), PathBuf::from(p)));
        }
        if let Some(h) = &home {
            out.push(("Library/Caches".into(), h.join("Library/Caches")));
            out.push(("Library/Containers".into(), h.join("Library/Containers")));
            out.push((".Trash".into(), h.join(".Trash")));
        }
    }
    out
}

/// Files left out by name.
const FILES: &[&str] = &["pagefile.sys", "hiberfil.sys", "swapfile.sys", ".DS_Store", "Thumbs.db", "desktop.ini"];

/// Every rule, for the settings.
pub fn all_rules() -> Vec<RuleInfo> {
    let note = |w: When| match w {
        When::Always => "",
        When::Beside(f) if f == PROJECT => "inside a project",
        When::Beside(&["composer.json", "go.mod"]) => "in PHP and Go projects",
        When::Beside(&["Cargo.toml", "pom.xml"]) => "in Rust and Maven projects",
        When::Beside(_) => "in projects",
        When::BesideExt(_) => "in .NET projects",
        When::Contains(_) => "when it holds a Python environment",
    };
    let mut out: Vec<RuleInfo> = NAMED.iter().map(|&(id, group, when)| RuleInfo { id, group, note: note(when) }).collect();
    out.push(RuleInfo { id: "system", group: "System", note: "Windows, Program Files, ProgramData, AppData; /System, /Library, ~/Library/Caches on macOS" });
    out.push(RuleInfo { id: "system-files", group: "System", note: "pagefile.sys, hiberfil.sys, .DS_Store, Thumbs.db, desktop.ini" });
    out
}

/// The rules in force: the defaults minus the ones switched off, plus the user's own excludes.
#[derive(Debug, Clone)]
pub struct Rules {
    named: Vec<(String, Option<String>, When)>,
    system: Vec<String>,
    system_files: bool,
    /// Extra folder or file names (case-insensitive) and absolute paths to leave out.
    excluded_names: HashSet<String>,
    excluded_paths: Vec<String>,
    /// Paths always indexed even if a rule would skip them.
    included_paths: Vec<String>,
    /// Files and folders the system marks hidden (dotfiles are indexed either way).
    pub hidden: bool,
    /// Honour .gitignore, .ignore and .fbignore inside projects.
    pub gitignore: bool,
}

fn key(p: &Path) -> String {
    let s = p.to_string_lossy();
    let s = if cfg!(windows) { s.replace('/', "\\") } else { s.into_owned() };
    let s = s.trim_end_matches(['\\', '/']).to_string();
    if cfg!(any(windows, target_os = "macos")) { s.to_lowercase() } else { s }
}

fn within(path_key: &str, base: &str) -> bool {
    path_key == base || (path_key.starts_with(base) && path_key[base.len()..].starts_with(['\\', '/']))
}

impl Rules {
    /// `off`: rule ids switched off; `excludes`: names or absolute paths; `includes`: absolute paths.
    pub fn new(off: &[String], excludes: &[String], includes: &[String], hidden: bool, gitignore: bool) -> Rules {
        let off: HashSet<String> = off.iter().map(|s| s.to_lowercase()).collect();
        let named = NAMED
            .iter()
            .filter(|(id, ..)| !off.contains(&id.to_lowercase()))
            .map(|&(id, _, when)| match id.split_once('/') {
                Some((parent, name)) => (name.to_lowercase(), Some(parent.to_lowercase()), when),
                None => (id.to_lowercase(), None, when),
            })
            .collect();
        let system = if off.contains("system") { Vec::new() } else { system_paths().into_iter().map(|(_, p)| key(&p)).collect() };
        let (paths, names): (Vec<&String>, Vec<&String>) = excludes.iter().filter(|e| !e.trim().is_empty()).partition(|e| Path::new(e.as_str()).is_absolute());
        Rules {
            named,
            system,
            system_files: !off.contains("system-files"),
            excluded_names: names.into_iter().map(|n| n.trim().to_lowercase()).collect(),
            excluded_paths: paths.into_iter().map(|p| key(Path::new(p))).collect(),
            included_paths: includes.iter().map(|p| key(Path::new(p))).collect(),
            hidden,
            gitignore,
        }
    }

    /// A short fingerprint: the index is rebuilt when the rules change.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (n, p, _) in &self.named {
            (n, p).hash(&mut h);
        }
        self.system.hash(&mut h);
        self.system_files.hash(&mut h);
        let mut names: Vec<&String> = self.excluded_names.iter().collect();
        names.sort();
        names.hash(&mut h);
        self.excluded_paths.hash(&mut h);
        self.included_paths.hash(&mut h);
        (self.hidden, self.gitignore).hash(&mut h);
        h.finish()
    }

    /// Whether `path` is somewhere the user asked to index anyway.
    fn included(&self, k: &str) -> bool {
        self.included_paths.iter().any(|i| within(k, i) || within(i, k))
    }

    /// Whether a folder (or file, `is_dir` false) at `path` is left out.
    pub fn skip(&self, path: &Path, is_dir: bool) -> bool {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { return false };
        let k = key(path);
        if self.included(&k) {
            return false;
        }
        if self.excluded_paths.iter().any(|p| within(&k, p)) || self.excluded_names.contains(&name) {
            return true;
        }
        if !is_dir {
            return self.system_files && FILES.iter().any(|f| f.eq_ignore_ascii_case(&name));
        }
        if self.system.contains(&k) {
            return true;
        }
        let parent = path.parent();
        let parent_name = parent.and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_lowercase());
        self.named.iter().any(|(n, p, when)| {
            *n == name
                && p.as_ref().is_none_or(|p| parent_name.as_deref() == Some(p.as_str()))
                && match when {
                    When::Always => true,
                    When::Beside(files) => parent.is_some_and(|d| files.iter().any(|f| d.join(f).is_file())),
                    When::BesideExt(exts) => parent.is_some_and(|d| {
                        std::fs::read_dir(d).is_ok_and(|rd| {
                            rd.flatten().any(|e| {
                                let n = e.file_name().to_string_lossy().to_lowercase();
                                exts.iter().any(|x| n.ends_with(&format!(".{x}")))
                            })
                        })
                    }),
                    When::Contains(file) => path.join(file).is_file(),
                }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fileboat-rules-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn named_and_conditional_rules() {
        let d = tmp("cond");
        let rules = Rules::new(&[], &[], &[], false, true);
        std::fs::create_dir_all(d.join("web/node_modules")).unwrap();
        std::fs::create_dir_all(d.join("web/dist")).unwrap();
        std::fs::create_dir_all(d.join("photos/dist")).unwrap();
        std::fs::write(d.join("web/package.json"), "{}").unwrap();
        std::fs::create_dir_all(d.join("php/vendor")).unwrap();
        std::fs::create_dir_all(d.join("notes/vendor")).unwrap();
        std::fs::write(d.join("php/composer.json"), "{}").unwrap();
        std::fs::create_dir_all(d.join("py/env")).unwrap();
        std::fs::write(d.join("py/env/pyvenv.cfg"), "").unwrap();
        std::fs::create_dir_all(d.join("letters/env")).unwrap();
        std::fs::create_dir_all(d.join("app/bin")).unwrap();
        std::fs::write(d.join("app/App.csproj"), "").unwrap();
        std::fs::create_dir_all(d.join("home/.cargo/registry")).unwrap();
        std::fs::create_dir_all(d.join("home/other/registry")).unwrap();

        assert!(rules.skip(&d.join("web/node_modules"), true));
        assert!(rules.skip(&d.join("web/dist"), true), "dist inside a project");
        assert!(!rules.skip(&d.join("photos/dist"), true), "dist elsewhere is yours");
        assert!(rules.skip(&d.join("php/vendor"), true));
        assert!(!rules.skip(&d.join("notes/vendor"), true));
        assert!(rules.skip(&d.join("py/env"), true));
        assert!(!rules.skip(&d.join("letters/env"), true));
        assert!(rules.skip(&d.join("app/bin"), true));
        assert!(rules.skip(&d.join("home/.cargo/registry"), true));
        assert!(!rules.skip(&d.join("home/other/registry"), true));
        assert!(rules.skip(&d.join("Thumbs.db"), false));
        assert!(!rules.skip(&d.join("node_modules"), false), "a file named like a rule folder stays");

        // Switched off, excluded by the user, included anyway.
        let off = Rules::new(&["node_modules".into()], &["photos".into(), d.join("letters").display().to_string()], &[d.join("web/dist").display().to_string()], false, true);
        assert!(!off.skip(&d.join("web/node_modules"), true));
        assert!(off.skip(&d.join("photos"), true));
        assert!(off.skip(&d.join("letters/env"), true));
        assert!(!off.skip(&d.join("web/dist"), true));
        assert_ne!(off.fingerprint(), rules.fingerprint());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn every_rule_is_listed_once() {
        let all = all_rules();
        let ids: HashSet<&str> = all.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), all.len());
        assert!(ids.contains("node_modules") && ids.contains("system"));
    }
}
