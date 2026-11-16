//! Searching inside files, as ripgrep does (its own crates): a parallel walk of a folder that
//! honours `.gitignore` and the index's rules (no `node_modules`), skipping binary files, files
//! over 50 MB and cloud files that aren't on this computer; matching lines stream out as found.

use crate::index::Rules;
use crate::preview::not_downloaded;
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkMatch};
use ignore::{WalkBuilder, WalkState};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Default)]
pub struct GrepOptions {
    /// The pattern is a regular expression (otherwise plain text).
    pub regex: bool,
    /// Match case exactly (otherwise only when the pattern has capitals).
    pub case: bool,
    pub whole_word: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    pub path: String,
    /// 1-based.
    pub line: u64,
    /// The line, trimmed to about 300 characters around the first match.
    pub text: String,
    /// Character ranges in `text` that matched.
    pub ranges: Vec<(u32, u32)>,
}

const MAX_FILE: u64 = 50 << 20;
const MAX_LINE: usize = 300;

/// The line as shown, and where the matches are in it (in characters).
fn excerpt(line: &str, byte_ranges: &[(usize, usize)]) -> (String, Vec<(u32, u32)>) {
    let line = line.trim_end_matches(['\r', '\n']);
    // Start a little before the first match when the line is long.
    let first = byte_ranges.first().map_or(0, |r| r.0);
    let mut start = if line.len() > MAX_LINE && first > 40 { first - 40 } else { 0 };
    while !line.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + MAX_LINE).min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let shown = &line[start..end];
    let lead = shown.len() - shown.trim_start().len();
    let text = shown.trim_start();
    let to_chars = |b: usize| text[..b.min(text.len())].chars().count() as u32;
    let ranges: Vec<(u32, u32)> = byte_ranges
        .iter()
        .filter(|(a, b)| *a >= start + lead && *b <= end)
        .map(|(a, b)| (to_chars(a - start - lead), to_chars(b - start - lead)))
        .collect();
    let mut out = text.to_string();
    if start > 0 {
        out.insert(0, '…');
        // The ranges move along with the text.
        return (out, ranges.into_iter().map(|(a, b): (u32, u32)| (a + 1, b + 1)).collect());
    }
    (out, ranges)
}

struct Collect<'a, M: Matcher> {
    matcher: &'a M,
    path: &'a str,
    found: &'a mut Vec<LineMatch>,
}

impl<M: Matcher> Sink for Collect<'_, M> {
    type Error = std::io::Error;

    fn matched(&mut self, _s: &grep_searcher::Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        let bytes = m.bytes();
        let mut ranges = Vec::new();
        let _ = self.matcher.find_iter(bytes, |r| {
            ranges.push((r.start(), r.end()));
            ranges.len() < 50
        });
        let line = String::from_utf8_lossy(bytes);
        let (text, ranges) = excerpt(&line, &ranges);
        self.found.push(LineMatch { path: self.path.to_string(), line: m.line_number().unwrap_or(0), text, ranges });
        // At most 100 lines from one file.
        Ok(self.found.len() < 100)
    }
}

/// Search the files under `root` for `pattern`, calling `found` with each file's matching lines
/// (from several threads, one file at a time). Stops after `limit` lines or when `cancelled`.
/// Returns how many files were searched.
pub fn grep(root: &Path, pattern: &str, opts: &GrepOptions, rules: &Rules, limit: usize, cancelled: &(dyn Fn() -> bool + Sync), found: &(dyn Fn(Vec<LineMatch>) + Sync)) -> Result<usize, String> {
    if pattern.is_empty() {
        return Ok(0);
    }
    let pat = if opts.regex { pattern.to_string() } else { regex::escape(pattern) };
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(!opts.case && !pattern.chars().any(char::is_uppercase))
        .case_smart(false)
        .word(opts.whole_word)
        .build(&pat)
        .map_err(|e| format!("That pattern doesn't work: {e}"))?;
    let mut wb = WalkBuilder::new(root);
    wb.hidden(false).follow_links(false).git_ignore(true).ignore(true).git_global(true).git_exclude(true).parents(true);
    wb.add_custom_ignore_filename(".fbignore");
    let rules = rules.clone();
    wb.filter_entry(move |e| e.depth() == 0 || !(e.file_type().is_some_and(|t| t.is_symlink()) || rules.skip(e.path(), e.file_type().is_some_and(|t| t.is_dir()))));
    let walker = wb.build_parallel();
    let files = AtomicUsize::new(0);
    let lines = AtomicUsize::new(0);
    walker.run(|| {
        let matcher = matcher.clone();
        let mut searcher = SearcherBuilder::new().line_number(true).binary_detection(BinaryDetection::quit(0)).build();
        let (files, lines) = (&files, &lines);
        Box::new(move |res| {
            if cancelled() || lines.load(Ordering::Relaxed) >= limit {
                return WalkState::Quit;
            }
            let Ok(e) = res else { return WalkState::Continue };
            if !e.file_type().is_some_and(|t| t.is_file()) {
                return WalkState::Continue;
            }
            let Ok(meta) = e.metadata() else { return WalkState::Continue };
            if meta.len() > MAX_FILE || meta.len() == 0 || not_downloaded(&meta) {
                return WalkState::Continue;
            }
            files.fetch_add(1, Ordering::Relaxed);
            let path = e.path().display().to_string();
            let mut out = Vec::new();
            let _ = searcher.search_path(&matcher, e.path(), Collect { matcher: &matcher, path: &path, found: &mut out });
            if !out.is_empty() {
                lines.fetch_add(out.len(), Ordering::Relaxed);
                found(out);
            }
            WalkState::Continue
        })
    });
    Ok(files.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn finds_lines_honouring_rules_and_gitignore() {
        let d = std::env::temp_dir().join(format!("fileboat-grep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::create_dir_all(d.join("node_modules/x")).unwrap();
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join(".gitignore"), "*.log\n").unwrap();
        std::fs::write(d.join("src/main.rs"), "fn main() {\n    println!(\"Hello, TODO world\");\n}\n// todo: more\n").unwrap();
        std::fs::write(d.join("node_modules/x/index.js"), "// TODO hidden in deps\n").unwrap();
        std::fs::write(d.join("run.log"), "TODO in a log\n").unwrap();
        std::fs::write(d.join("blob.bin"), [b'T', b'O', b'D', b'O', 0, 1, 2]).unwrap();
        let rules = Rules::new(&[], &[], &[], false, true);
        let all = Mutex::new(Vec::new());
        let n = grep(&d, "todo", &GrepOptions::default(), &rules, 1000, &|| false, &|m| all.lock().unwrap().extend(m)).unwrap();
        let mut all = all.into_inner().unwrap();
        all.sort_by_key(|m| m.line);
        let lines: Vec<(u64, &str)> = all.iter().map(|m| (m.line, m.text.as_str())).collect();
        assert_eq!(lines, [(2, "println!(\"Hello, TODO world\");"), (4, "// todo: more")]);
        assert_eq!(all[0].ranges, [(17, 21)], "TODO in the trimmed line");
        assert!(n >= 2);
        // Case: capitals in the pattern make it exact.
        let caps = Mutex::new(Vec::new());
        grep(&d, "TODO", &GrepOptions::default(), &rules, 1000, &|| false, &|m| caps.lock().unwrap().extend(m)).unwrap();
        assert_eq!(caps.into_inner().unwrap().len(), 1);
        // Regex and whole words.
        let re = Mutex::new(Vec::new());
        grep(&d, r"fn \w+\(", &GrepOptions { regex: true, ..Default::default() }, &rules, 1000, &|| false, &|m| re.lock().unwrap().extend(m)).unwrap();
        assert_eq!(re.into_inner().unwrap()[0].line, 1);
        assert!(grep(&d, "(", &GrepOptions { regex: true, ..Default::default() }, &rules, 10, &|| false, &|_| {}).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn long_lines_are_cut_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
        let (text, ranges) = excerpt(&line, &[(500, 506)]);
        assert!(text.starts_with('…'));
        assert!(text.chars().count() <= MAX_LINE + 1);
        let (a, b) = ranges[0];
        assert_eq!(text.chars().skip(a as usize).take((b - a) as usize).collect::<String>(), "needle");
    }
}
