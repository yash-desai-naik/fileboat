//! Searching the index. A query is words and filters:
//!
//! - words match names fuzzily (`mainrs` finds `main.rs`); `'word` must appear as typed,
//!   `^word` starts the name, `word$` ends it, `!word` excludes; `/regex/` matches names by a
//!   regular expression; a word with a slash (`src/main`) also needs its folders on the path;
//! - `ext:rs,ts` · `kind:file|folder|image|code|doc|archive|audio|video|font` ·
//!   `size:>10mb` `size:<1k` `size:1mb..5mb` · `modified:<7d` (within 7 days) `modified:>1y`
//!   (older) `modified:today` · `in:src` (somewhere under a folder whose name has "src").
//!
//! Filters go first (cheap), then the words, over every candidate name in parallel.

use super::{Index, KIND_FOLDER};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cmp {
    Less(f64),
    More(f64),
    Between(f64, f64),
}

impl Cmp {
    fn holds(self, v: f64) -> bool {
        match self {
            Cmp::Less(x) => v < x,
            Cmp::More(x) => v > x,
            Cmp::Between(a, b) => v >= a && v <= b,
        }
    }
}

#[derive(Debug, Default)]
pub struct Query {
    /// Words for the fuzzy matcher (frizbee's syntax).
    words: Vec<String>,
    /// Folders a word with slashes needs on the path.
    path_parts: Vec<String>,
    regex: Option<regex::Regex>,
    ext: Vec<String>,
    kinds: Vec<String>,
    size: Option<Cmp>,
    /// Age in milliseconds.
    age: Option<Cmp>,
    today: bool,
    within: Vec<String>,
    /// Filters this index can't answer (git:…), to say so.
    pub unsupported: Vec<String>,
}

fn bytes(s: &str) -> Option<f64> {
    let s = s.trim().to_lowercase();
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let n: f64 = s[..split].parse().ok()?;
    let mult = match &s[split..] {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024f64.powi(4),
        _ => return None,
    };
    Some(n * mult)
}

fn duration(s: &str) -> Option<f64> {
    let s = s.trim().to_lowercase();
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let n: f64 = s[..split].parse().ok()?;
    let ms = match &s[split..] {
        "min" | "mins" => 60_000.0,
        "h" | "hr" | "hours" => 3_600_000.0,
        "d" | "" | "days" => 86_400_000.0,
        "w" | "weeks" => 7.0 * 86_400_000.0,
        "mo" | "months" => 30.0 * 86_400_000.0,
        "y" | "years" => 365.0 * 86_400_000.0,
        _ => return None,
    };
    Some(n * ms)
}

fn compare(s: &str, unit: fn(&str) -> Option<f64>) -> Option<Cmp> {
    if let Some(rest) = s.strip_prefix(">=").or_else(|| s.strip_prefix('>')) {
        return unit(rest).map(Cmp::More);
    }
    if let Some(rest) = s.strip_prefix("<=").or_else(|| s.strip_prefix('<')) {
        return unit(rest).map(Cmp::Less);
    }
    if let Some((a, b)) = s.split_once("..") {
        return Some(Cmp::Between(unit(a)?, unit(b)?));
    }
    let v = unit(s.strip_prefix('=').unwrap_or(s))?;
    Some(Cmp::Between(v, v))
}

impl Query {
    pub fn parse(text: &str) -> Result<Query, String> {
        let mut q = Query::default();
        for token in text.split_whitespace() {
            if token.len() > 2 && token.starts_with('/') && token.ends_with('/') {
                let re = regex::RegexBuilder::new(&token[1..token.len() - 1]).case_insensitive(true).build().map_err(|e| format!("That regular expression doesn't work: {e}"))?;
                q.regex = Some(re);
                continue;
            }
            if let Some((key, value)) = token.split_once(':') {
                let v = value.to_lowercase();
                match key.to_lowercase().as_str() {
                    "ext" => {
                        q.ext.extend(v.split(',').map(|e| e.trim_start_matches('.').to_string()).filter(|e| !e.is_empty()));
                        continue;
                    }
                    "kind" | "type" => {
                        q.kinds.extend(v.split([',', '|']).map(str::to_string).filter(|k| !k.is_empty()));
                        continue;
                    }
                    "size" => {
                        q.size = Some(compare(&v, bytes).ok_or_else(|| format!("“{token}”: try size:>10mb, size:<1k or size:1mb..5mb"))?);
                        continue;
                    }
                    "modified" | "mod" | "date" => {
                        if v == "today" {
                            q.today = true;
                        } else {
                            // "modified:<7d" means within the last 7 days: an age below 7 days.
                            q.age = Some(compare(&v, duration).ok_or_else(|| format!("“{token}”: try modified:<7d, modified:>1y or modified:today"))?);
                        }
                        continue;
                    }
                    "in" => {
                        q.within.push(v);
                        continue;
                    }
                    "name" => {
                        q.words.push(value.to_string());
                        continue;
                    }
                    "git" => {
                        q.unsupported.push(token.to_string());
                        continue;
                    }
                    // A drive letter ("c:\…") or anything else: a plain word.
                    _ => {}
                }
            }
            if token.contains(['/', '\\']) {
                let parts: Vec<&str> = token.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
                if let Some((last, dirs)) = parts.split_last() {
                    q.path_parts.extend(dirs.iter().map(|d| d.to_lowercase()));
                    q.words.push((*last).to_string());
                }
                continue;
            }
            q.words.push(token.to_string());
        }
        Ok(q)
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.regex.is_none() && self.ext.is_empty() && self.kinds.is_empty() && self.size.is_none() && self.age.is_none() && !self.today && self.within.is_empty() && self.path_parts.is_empty()
    }
}

const IMAGE: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "ico", "svg", "tif", "tiff", "heic", "psd"];
const CODE: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts", "c", "h", "cc", "cpp", "hpp", "cs", "fs", "rb", "php", "swift", "m", "mm", "scala", "dart", "lua", "sh",
    "bash", "zsh", "ps1", "sql", "html", "css", "scss", "less", "vue", "svelte", "json", "yaml", "yml", "toml", "xml", "gradle", "ex", "exs", "erl", "hs", "clj", "r", "jl", "zig", "nim",
];
const DOC: &[&str] = &["md", "markdown", "txt", "pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "csv", "ppt", "pptx", "odp", "epub"];
const ARCHIVE: &[&str] = &["zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "zst", "jar", "war", "whl", "nupkg"];
const AUDIO: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "aac", "opus", "wma"];
const VIDEO: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "m4v", "wmv"];
const FONT: &[&str] = &["ttf", "otf", "woff", "woff2"];

fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(0) | None => "",
        Some(i) => &name[i + 1..],
    }
}

fn kind_matches(kind: &str, is_dir: bool, ext: &str) -> bool {
    let in_list = |list: &[&str]| list.iter().any(|e| e.eq_ignore_ascii_case(ext));
    match kind {
        "folder" | "dir" | "directory" => is_dir,
        "file" => !is_dir,
        _ if is_dir => false,
        "image" | "img" => in_list(IMAGE),
        "code" | "source" => in_list(CODE),
        "doc" | "document" | "text" => in_list(DOC),
        "archive" => in_list(ARCHIVE),
        "audio" | "music" => in_list(AUDIO),
        "video" => in_list(VIDEO),
        "font" => in_list(FONT),
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub path: String,
    pub name: String,
    pub folder: bool,
    pub size: u64,
    pub modified: i64,
    /// Characters of the name that matched (for highlighting).
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<Hit>,
    /// How many matched (more than `hits` when capped).
    pub total: usize,
    pub unsupported: Vec<String>,
}

fn byte_to_char_indices(s: &str, bytes: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = bytes.iter().filter_map(|&b| s.char_indices().position(|(i, _)| i == b as usize).map(|p| p as u32)).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Search the index (under `scope` when given), best matches first, at most `limit`.
pub fn search(idx: &Index, q: &Query, scope: Option<u32>, limit: usize, now_ms: i64) -> SearchResult {
    let day_start = now_ms - now_ms.rem_euclid(86_400_000);
    let roots: Vec<u32> = idx.roots().iter().map(|(_, id)| *id).collect();
    let passes = |id: u32| -> bool {
        if !idx.is_live(id) || roots.contains(&id) {
            return false;
        }
        let name = idx.name(id);
        let is_dir = idx.kind(id) == KIND_FOLDER;
        let ext = if is_dir { "" } else { ext_of(name) };
        if !q.ext.is_empty() && (is_dir || !q.ext.iter().any(|e| e.eq_ignore_ascii_case(ext))) {
            return false;
        }
        if !q.kinds.is_empty() && !q.kinds.iter().any(|k| kind_matches(k, is_dir, ext)) {
            return false;
        }
        if let Some(c) = q.size
            && (is_dir || !c.holds(idx.size(id) as f64))
        {
            return false;
        }
        if let Some(c) = q.age
            && !c.holds((now_ms - idx.mtime(id)) as f64)
        {
            return false;
        }
        if q.today && idx.mtime(id) < day_start {
            return false;
        }
        if let Some(re) = &q.regex
            && !re.is_match(name)
        {
            return false;
        }
        if let Some(s) = scope
            && (id == s || !idx.within(id, s))
        {
            return false;
        }
        if !q.within.is_empty() || !q.path_parts.is_empty() {
            let mut ancestors = Vec::new();
            let mut at = idx.parent(id);
            while at != super::NONE {
                ancestors.push(idx.name(at).to_lowercase());
                at = idx.parent(at);
            }
            if !q.within.iter().all(|w| ancestors.iter().any(|a| a.contains(w.as_str()))) {
                return false;
            }
            if !q.path_parts.iter().all(|p| ancestors.iter().any(|a| a.contains(p.as_str()))) {
                return false;
            }
        }
        true
    };
    let candidates: Vec<u32> = (0..idx.len() as u32).filter(|&id| passes(id)).collect();

    // What a name is compared with for the boosts: the first plain word.
    let first = q.words.iter().find(|w| !w.starts_with(['!', '\'', '^'])).map(|w| w.trim_end_matches('$').to_lowercase()).unwrap_or_default();
    let boost = |id: u32| -> u8 {
        if first.is_empty() {
            return 0;
        }
        let name = idx.name(id).to_lowercase();
        let stem = match name.rfind('.') {
            Some(i) if i > 0 => &name[..i],
            _ => name.as_str(),
        };
        // "main" → main.rs before mainline.rs before domain.rs.
        if name == first || stem == first {
            2
        } else if name.starts_with(&first) {
            1
        } else {
            0
        }
    };
    let rank = |ids: &mut Vec<(u32, u16)>| {
        // Named as typed first; then better score; then shallower; then shorter names; then newer.
        ids.sort_by(|a, b| {
            let (ba, bb) = (boost(a.0), boost(b.0));
            // Among names exactly as typed, the shallower one (~/Downloads before
            // ~/.claude/downloads), whatever the letter case.
            let exact_depth = if ba == 2 && bb == 2 { idx.depth(a.0).cmp(&idx.depth(b.0)) } else { std::cmp::Ordering::Equal };
            bb.cmp(&ba)
                .then(exact_depth)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| idx.depth(a.0).cmp(&idx.depth(b.0)))
                .then_with(|| idx.name(a.0).len().cmp(&idx.name(b.0).len()))
                // Then what you worked on lately.
                .then_with(|| idx.mtime(b.0).cmp(&idx.mtime(a.0)))
        });
    };

    let (mut ranked, total): (Vec<(u32, u16)>, usize) = if q.words.is_empty() {
        let mut all: Vec<(u32, u16)> = candidates.iter().map(|&id| (id, 0)).collect();
        let total = all.len();
        // Without words: newest first.
        all.sort_by_key(|&(id, _)| std::cmp::Reverse(idx.mtime(id)));
        all.truncate(limit);
        (all, total)
    } else {
        let names: Vec<&str> = candidates.iter().map(|&id| idx.name(id)).collect();
        let config = frizbee::Config { max_typos: Some(0), ..frizbee::Config::default() };
        let mut matcher = frizbee::Matcher::from_query(&q.words.join(" "), &config);
        let matches = if names.len() > 50_000 { matcher.match_list_parallel(&names, 0) } else { matcher.match_list(&names) };
        let total = matches.len();
        // Re-rank the best few thousand with the tie-breakers.
        let mut top: Vec<(u32, u16)> = matches.iter().take(limit.max(1) * 20).map(|m| (candidates[m.index as usize], m.score)).collect();
        rank(&mut top);
        top.truncate(limit);
        (top, total)
    };

    // Highlights for the ones shown.
    let indices: Vec<Vec<u32>> = if q.words.is_empty() {
        vec![Vec::new(); ranked.len()]
    } else {
        let names: Vec<&str> = ranked.iter().map(|&(id, _)| idx.name(id)).collect();
        let config = frizbee::Config { max_typos: Some(0), ..frizbee::Config::default() };
        let mut matcher = frizbee::Matcher::from_query(&q.words.join(" "), &config);
        let mut out = vec![Vec::new(); names.len()];
        for m in matcher.match_list_indices(&names) {
            out[m.index as usize] = byte_to_char_indices(names[m.index as usize], &m.indices);
        }
        out
    };
    let hits = ranked
        .drain(..)
        .zip(indices)
        .map(|((id, _), indices)| Hit {
            path: idx.path(id).display().to_string(),
            name: idx.name(id).to_string(),
            folder: idx.kind(id) == KIND_FOLDER,
            size: idx.size(id),
            modified: idx.mtime(id),
            indices,
        })
        .collect();
    SearchResult { hits, total, unsupported: q.unsupported.clone() }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{file, folder};
    use super::super::{Index, Meta, KIND_FILE};
    use super::*;
    use std::path::Path;

    fn idx() -> Index {
        let mut idx = Index::default();
        let root = idx.add_root(Path::new("/home/me"), folder());
        let app = idx.push(root, "app", folder());
        let src = idx.push(app, "src", folder());
        idx.push(src, "main.rs", Meta { kind: KIND_FILE, hidden: false, size: 2_000, mtime: 10 * 86_400_000 });
        idx.push(src, "domain.rs", file(50));
        idx.push(app, "main.ts", Meta { kind: KIND_FILE, hidden: false, size: 20 << 20, mtime: 9 * 86_400_000 });
        idx.push(root, "photo.png", file(5 << 20));
        idx.push(root, "maintenance.txt", file(10));
        idx
    }

    fn names(r: &SearchResult) -> Vec<&str> {
        r.hits.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn fuzzy_with_filters() {
        let idx = idx();
        let now = 10 * 86_400_000 + 1000;
        let run = |q: &str| search(&idx, &Query::parse(q).unwrap(), None, 50, now);
        let r = run("main");
        assert_eq!(names(&r)[..2], ["main.ts", "main.rs"], "exact-start matches first, shallower first");
        assert!(names(&r).contains(&"maintenance.txt"));
        assert_eq!(r.hits[0].indices, vec![0, 1, 2, 3]);
        assert_eq!(names(&run("main ext:rs")), ["main.rs", "domain.rs"], "domain.rs has \"main\" in it");
        assert_eq!(names(&run("kind:image")), ["photo.png"]);
        assert_eq!(names(&run("size:>1mb kind:file")).len(), 2);
        assert_eq!(names(&run("main modified:<1d")), ["main.rs"]);
        assert_eq!(names(&run("in:src")).len(), 2);
        // "domain.rs" has m-a-i in order too: fuzzy finds it, after the better match.
        let in_src = run("src/mai");
        assert_eq!(names(&in_src)[0], "main.rs");
        assert!(!names(&in_src).contains(&"main.ts"), "main.ts isn't under a src folder");
        assert_eq!(names(&run("/^d.*n\\.rs$/")), ["domain.rs"]);
        assert_eq!(names(&run("kind:folder")).len(), 2);
        assert_eq!(run("git:modified main").unsupported, ["git:modified"]);
        assert!(Query::parse("size:lots").is_err());
        let app = idx.find(Path::new("/home/me/app")).unwrap();
        let scoped = search(&idx, &Query::parse("main").unwrap(), Some(app), 50, now);
        assert_eq!(scoped.total, 3, "only under app: main.rs, domain.rs, main.ts");
    }
}
