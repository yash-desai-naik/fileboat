//! A file's outline for "Go to symbol" (`name@symbol` in the command bar, Ctrl+Shift+O): its
//! functions, classes, types, methods, headings… found line by line with a few patterns per
//! language, as ctags does. Not a parser: fast, never runs anything, good enough to jump to.

use crate::preview::read_text;
use regex::Regex;
use serde::Serialize;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// "function", "method", "class", "interface", "type", "enum", "struct", "trait", "module",
    /// "constant", "variable", "heading", "selector", "macro", "impl".
    pub kind: &'static str,
    /// 1-based.
    pub line: u32,
    /// 1-based column of the name.
    pub column: u32,
    /// Nesting by indentation (0 at the top), or the heading level minus one.
    pub depth: u32,
}

/// The outline of the file at `path` (at most the first 8 MB). Unknown languages have none.
pub fn symbols(path: &Path) -> Result<Vec<Symbol>, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let Some(lang) = language(&ext) else { return Ok(Vec::new()) };
    let text = read_text(path, 8 << 20)?.text;
    Ok(outline(lang, &text))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Js,
    Rust,
    Python,
    Go,
    CLike,
    Kotlin,
    Swift,
    Ruby,
    Php,
    C,
    Markdown,
    Css,
    Shell,
}

pub fn language(ext: &str) -> Option<Lang> {
    Some(match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => Lang::Js,
        "rs" => Lang::Rust,
        "py" | "pyw" | "pyi" => Lang::Python,
        "go" => Lang::Go,
        "java" | "cs" | "scala" | "dart" => Lang::CLike,
        "kt" | "kts" => Lang::Kotlin,
        "swift" => Lang::Swift,
        "rb" | "rake" => Lang::Ruby,
        "php" => Lang::Php,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "m" | "mm" => Lang::C,
        "md" | "markdown" | "mdx" => Lang::Markdown,
        "css" | "scss" | "less" | "sass" => Lang::Css,
        "sh" | "bash" | "zsh" | "fish" | "ps1" => Lang::Shell,
        _ => return None,
    })
}

/// Patterns: (kind, regex with the name in group "n"). The first that matches a line wins.
type Rules = Vec<(&'static str, Regex)>;

fn rules(lang: Lang) -> &'static Rules {
    static CACHE: OnceLock<Vec<(Lang, Rules)>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        let r = |list: &[(&'static str, &str)]| -> Rules { list.iter().map(|(k, p)| (*k, Regex::new(p).expect("a valid pattern"))).collect() };
        const ID: &str = r"[A-Za-z_$][\w$]*";
        vec![
            (
                Lang::Js,
                r(&[
                    ("function", &format!(r"^\s*(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:async\s+)?function\s*\*?\s*(?P<n>{ID})")),
                    ("class", &format!(r"^\s*(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:abstract\s+)?class\s+(?P<n>{ID})")),
                    ("interface", &format!(r"^\s*(?:export\s+)?(?:declare\s+)?interface\s+(?P<n>{ID})")),
                    ("type", &format!(r"^\s*(?:export\s+)?(?:declare\s+)?type\s+(?P<n>{ID})\s*(?:<.*>)?\s*=")),
                    ("enum", &format!(r"^\s*(?:export\s+)?(?:declare\s+)?(?:const\s+)?enum\s+(?P<n>{ID})")),
                    ("module", &format!(r"^\s*(?:export\s+)?(?:declare\s+)?namespace\s+(?P<n>{ID})")),
                    // const f = (…) => / async x => / function (…)
                    ("function", &format!(r"^\s*(?:export\s+)?(?:const|let|var)\s+(?P<n>{ID})\s*(?::[^=]+)?=\s*(?:async\s+)?(?:function\b|\([^)]*\)\s*(?::[^=]+)?=>|{ID}\s*=>)")),
                    ("constant", &format!(r"^(?:export\s+)?const\s+(?P<n>{ID})\b")),
                    // Methods and object functions: name(args) { / async name(args): T {
                    (
                        "method",
                        &format!(r"^\s+(?:(?:public|private|protected|static|readonly|async|override|abstract|get|set|\*)\s+)*(?P<n>{ID})\s*(?:<[^>]*>)?\s*\([^)]*\)?\s*(?::\s*[^={{]+)?\{{\s*$"),
                    ),
                ]),
            ),
            (
                Lang::Rust,
                r(&[
                    ("function", r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?fn\s+(?P<n>\w+)"#),
                    ("struct", r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(?P<n>\w+)"),
                    ("enum", r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(?P<n>\w+)"),
                    ("trait", r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+(?P<n>\w+)"),
                    ("type", r"^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+(?P<n>\w+)"),
                    ("module", r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?P<n>\w+)"),
                    ("constant", r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+(?:mut\s+)?(?P<n>[A-Z_][A-Z0-9_]*)\s*:"),
                    ("macro", r"^\s*macro_rules!\s*(?P<n>\w+)"),
                    ("impl", r"^\s*(?:unsafe\s+)?impl(?:\s*<[^>]*>)?\s+(?P<n>[\w:<>, ]+?(?:\s+for\s+[\w:<>, ]+?)?)\s*(?:where\b.*)?\{?\s*$"),
                ]),
            ),
            (
                Lang::Python,
                r(&[("class", r"^\s*class\s+(?P<n>\w+)"), ("function", r"^\s*(?:async\s+)?def\s+(?P<n>\w+)"), ("constant", r"^(?P<n>[A-Z_][A-Z0-9_]*)\s*(?::[^=]+)?=")]),
            ),
            (
                Lang::Go,
                r(&[
                    ("method", r"^func\s+\([^)]*\)\s*(?P<n>\w+)"),
                    ("function", r"^func\s+(?P<n>\w+)"),
                    ("struct", r"^type\s+(?P<n>\w+)\s+struct\b"),
                    ("interface", r"^type\s+(?P<n>\w+)\s+interface\b"),
                    ("type", r"^type\s+(?P<n>\w+)\b"),
                ]),
            ),
            (
                Lang::CLike,
                r(&[
                    ("class", r"^\s*(?:@\w+\s+)*(?:(?:public|private|protected|internal|static|abstract|final|sealed|partial|data|open)\s+)*(?:class|record)\s+(?P<n>\w+)"),
                    ("interface", r"^\s*(?:(?:public|private|protected|internal|static|sealed)\s+)*interface\s+(?P<n>\w+)"),
                    ("enum", r"^\s*(?:(?:public|private|protected|internal|static)\s+)*enum\s+(?P<n>\w+)"),
                    ("struct", r"^\s*(?:(?:public|private|protected|internal|static|readonly|ref)\s+)*struct\s+(?P<n>\w+)"),
                    (
                        "method",
                        r"^\s*(?:(?:public|private|protected|internal|static|final|abstract|override|virtual|async|synchronized|native|extern|unsafe|new)\s+)+[\w<>\[\],.?]+(?:\s*<[^>]*>)?\s+(?P<n>\w+)\s*(?:<[^>]*>)?\s*\(",
                    ),
                ]),
            ),
            (
                Lang::Kotlin,
                r(&[
                    ("class", r"^\s*(?:(?:public|private|protected|internal|abstract|open|sealed|data|enum|annotation|inner)\s+)*class\s+(?P<n>\w+)"),
                    ("interface", r"^\s*(?:(?:public|private|internal|sealed|fun)\s+)*interface\s+(?P<n>\w+)"),
                    ("module", r"^\s*(?:(?:public|private|internal|companion|data)\s+)*object\s+(?P<n>\w+)"),
                    ("function", r"^\s*(?:(?:public|private|protected|internal|override|open|suspend|inline|operator|infix|tailrec|abstract)\s+)*fun\s+(?:<[^>]*>\s*)?(?:[\w.]+\.)?(?P<n>\w+)\s*\("),
                ]),
            ),
            (
                Lang::Swift,
                r(&[
                    ("class", r"^\s*(?:(?:public|private|fileprivate|internal|open|final)\s+)*class\s+(?P<n>\w+)"),
                    ("struct", r"^\s*(?:(?:public|private|fileprivate|internal)\s+)*struct\s+(?P<n>\w+)"),
                    ("enum", r"^\s*(?:(?:public|private|fileprivate|internal|indirect)\s+)*enum\s+(?P<n>\w+)"),
                    ("interface", r"^\s*(?:(?:public|private|fileprivate|internal)\s+)*protocol\s+(?P<n>\w+)"),
                    ("module", r"^\s*(?:(?:public|private|fileprivate|internal)\s+)*extension\s+(?P<n>\w+)"),
                    ("function", r"^\s*(?:(?:public|private|fileprivate|internal|open|static|class|override|mutating|@\w+)\s+)*func\s+(?P<n>\w+)"),
                ]),
            ),
            (
                Lang::Ruby,
                r(&[("class", r"^\s*class\s+(?P<n>[\w:]+)"), ("module", r"^\s*module\s+(?P<n>[\w:]+)"), ("method", r"^\s*def\s+(?:self\.)?(?P<n>[\w?!=]+)")]),
            ),
            (
                Lang::Php,
                r(&[
                    ("class", r"^\s*(?:(?:abstract|final|readonly)\s+)*class\s+(?P<n>\w+)"),
                    ("interface", r"^\s*interface\s+(?P<n>\w+)"),
                    ("trait", r"^\s*trait\s+(?P<n>\w+)"),
                    ("function", r"^\s*(?:(?:public|private|protected|static|abstract|final)\s+)*function\s+&?(?P<n>\w+)"),
                ]),
            ),
            (
                Lang::C,
                r(&[
                    ("macro", r"^\s*#\s*define\s+(?P<n>\w+)"),
                    ("class", r"^\s*(?:template\s*<[^>]*>\s*)?class\s+(?:\w+\s+)?(?P<n>\w+)\s*(?:final\s*)?(?::[^;]*)?\{?\s*$"),
                    ("struct", r"^\s*(?:typedef\s+)?struct\s+(?P<n>\w+)\s*\{?\s*$"),
                    ("enum", r"^\s*(?:typedef\s+)?enum\s+(?:class\s+)?(?P<n>\w+)"),
                    ("module", r"^\s*namespace\s+(?P<n>\w+)"),
                    // A definition starting at the line's start: type name(args) {  (not a call, not a declaration ending in ;)
                    (
                        "function",
                        r"^(?:(?:static|inline|extern|virtual|constexpr|explicit|unsigned|signed|const|struct)\s+)*[\w:<>]+[\s*&]+(?P<n>~?\w+(?:::~?\w+)*)\s*\([^;]*\)\s*(?:const\s*)?(?:noexcept\s*)?(?:override\s*)?\{?\s*$",
                    ),
                ]),
            ),
            (Lang::Markdown, r(&[("heading", r"^(?P<level>#{1,6})\s+(?P<n>.+?)\s*#*\s*$")])),
            (Lang::Css, r(&[("selector", r"^(?P<n>[.#:\w\[\]\-*>+~ ,()=]+?)\s*\{")])),
            (Lang::Shell, r(&[("function", r"^\s*(?:function\s+)?(?P<n>[\w-]+)\s*\(\)\s*\{"), ("function", r"^\s*function\s+(?P<n>[\w-]+)")])),
        ]
    });
    &all.iter().find(|(l, _)| *l == lang).expect("every language has rules").1
}

const NOT_NAMES: &[&str] = &["if", "for", "while", "switch", "catch", "return", "function", "else", "do", "try", "with", "new", "typeof", "await", "match", "loop", "sizeof", "constructor"];

/// The outline of `text` in `lang`.
pub fn outline(lang: Lang, text: &str) -> Vec<Symbol> {
    let rules = rules(lang);
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut in_block_comment = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if lang == Lang::Markdown {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fence = !in_fence;
            }
            if in_fence {
                continue;
            }
        } else {
            // Skip comments (enough to not list commented-out code).
            if in_block_comment {
                if line.contains("*/") {
                    in_block_comment = false;
                }
                continue;
            }
            if trimmed.starts_with("/*") && !matches!(lang, Lang::Python | Lang::Ruby | Lang::Shell) {
                in_block_comment = !trimmed.contains("*/");
                continue;
            }
            let hash_comment = matches!(lang, Lang::Python | Lang::Ruby | Lang::Shell) || (lang == Lang::Php && trimmed.starts_with("# "));
            if trimmed.starts_with("//") || trimmed.starts_with('*') || (hash_comment && trimmed.starts_with('#')) {
                continue;
            }
        }
        for (kind, re) in rules {
            let Some(c) = re.captures(line) else { continue };
            let Some(n) = c.name("n") else { continue };
            let name = n.as_str().trim();
            if name.is_empty() || NOT_NAMES.contains(&name) {
                break;
            }
            let indent = line.len() - trimmed.len();
            let depth = match c.name("level") {
                Some(level) => level.as_str().len() as u32 - 1,
                None => (line[..indent].replace('\t', "    ").len() / 2).min(16) as u32,
            };
            // An indented function in a class-like language is a method.
            let kind = if *kind == "function" && depth > 0 && matches!(lang, Lang::Python | Lang::Kotlin | Lang::Swift | Lang::Php | Lang::Rust) { "method" } else { kind };
            out.push(Symbol { name: name.to_string(), kind, line: i as u32 + 1, column: line[..n.start()].chars().count() as u32 + 1, depth });
            break;
        }
        if out.len() >= 5000 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(lang: Lang, text: &str) -> Vec<String> {
        outline(lang, text).into_iter().map(|s| format!("{} {}:{}", s.kind, s.name, s.line)).collect()
    }

    #[test]
    fn typescript() {
        let ts = "import x from 'y';\nexport interface Log { at: Date }\nexport type Level = 'info' | 'warn';\nexport async function dailyLogs(day: Date): Promise<Log[]> {\n  if (x) {\n    return [];\n  }\n}\nconst parse = (s: string) => s.trim();\nexport const LIMIT = 10;\nexport class Store extends Base {\n  private async load(id: string): Promise<void> {\n    await go();\n  }\n}\n// function commented() {}\nenum Kind { A }\n";
        assert_eq!(
            names(Lang::Js, ts),
            [
                "interface Log:2",
                "type Level:3",
                "function dailyLogs:4",
                "function parse:9",
                "constant LIMIT:10",
                "class Store:11",
                "method load:12",
                "enum Kind:17"
            ]
        );
    }

    #[test]
    fn rust_python_go() {
        let rs = "pub struct Index {\n    n: u32,\n}\nimpl Index {\n    pub fn len(&self) -> usize { 0 }\n}\nimpl Display for Index {\n}\npub(crate) async fn build() {}\nconst MAX: usize = 3;\nmacro_rules! m { () => {} }\n";
        assert_eq!(names(Lang::Rust, rs), ["struct Index:1", "impl Index:4", "method len:5", "impl Display for Index:7", "function build:9", "constant MAX:10", "macro m:11"]);
        let py = "import os\n\nTIMEOUT = 3\n\nclass Store:\n    def load(self):\n        pass\n\nasync def main():\n    # def not_this(): pass\n    pass\n";
        assert_eq!(names(Lang::Python, py), ["constant TIMEOUT:3", "class Store:5", "method load:6", "function main:9"]);
        let go = "package main\n\ntype Server struct {\n}\n\nfunc (s *Server) Start() error {\n}\n\nfunc main() {\n}\n";
        assert_eq!(names(Lang::Go, go), ["struct Server:3", "method Start:6", "function main:9"]);
    }

    #[test]
    fn markdown_headings_skip_code() {
        let md = "# Title\n\nText\n\n## Install\n\n```sh\n# not a heading\n```\n\n### Windows ###\n";
        let s = outline(Lang::Markdown, md);
        assert_eq!(s.iter().map(|s| (s.name.as_str(), s.line, s.depth)).collect::<Vec<_>>(), [("Title", 1, 0), ("Install", 5, 1), ("Windows", 11, 2)]);
    }

    #[test]
    fn columns_and_languages() {
        let s = outline(Lang::Js, "export function go() {}\n");
        assert_eq!((s[0].line, s[0].column), (1, 17));
        assert_eq!(language("tsx"), Some(Lang::Js));
        assert_eq!(language("txt"), None);
    }
}
