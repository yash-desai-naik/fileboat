//! A text file's start, decoded: UTF-8, UTF-16 (by its byte order mark), or a legacy encoding
//! guessed from the bytes (Windows-1252, Shift JIS…). A 2 GB log reads only its first part.

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};
use serde::Serialize;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    pub text: String,
    /// "UTF-8", "UTF-8 with BOM", "UTF-16 LE", "windows-1252"…
    pub encoding: String,
    /// "LF", "CRLF", "CR", "Mixed", or "" for a single line.
    pub line_ending: String,
    /// Only the start was read (`limit`); the text ends at a whole line.
    pub truncated: bool,
    /// The file's size.
    pub total: u64,
}

/// Read up to `limit` bytes of `path` as text.
pub fn read_text(path: &Path, limit: u64) -> Result<Text, String> {
    let file = crate::osfs::open(path).map_err(|e| crate::fs::describe(path, &e))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut bytes = Vec::with_capacity(limit.min(total) as usize);
    file.take(limit).read_to_end(&mut bytes).map_err(|e| crate::fs::describe(path, &e))?;
    let truncated = (bytes.len() as u64) < total;
    Ok(decode(&bytes, truncated, total))
}

fn decode(bytes: &[u8], truncated: bool, total: u64) -> Text {
    let (encoding, label, body): (&'static Encoding, &str, &[u8]) = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (UTF_8, "UTF-8 with BOM", rest)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (UTF_16LE, "UTF-16 LE", rest)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        (UTF_16BE, "UTF-16 BE", rest)
    } else if utf8_up_to_a_cut(bytes, truncated) {
        (UTF_8, "UTF-8", bytes)
    } else {
        let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
        detector.feed(bytes, !truncated);
        let guess = detector.guess(None, chardetng::Utf8Detection::Allow);
        (guess, guess.name(), bytes)
    };
    let (decoded, _) = encoding.decode_without_bom_handling(body);
    let mut text = decoded.into_owned();
    // A cut in the middle of a line (or a character) ends at the last whole line.
    if truncated && let Some(i) = text.rfind('\n') {
        text.truncate(i + 1);
    }
    Text { line_ending: line_ending(&text).into(), encoding: label.to_string(), text, truncated, total }
}

/// Valid UTF-8, allowing a character cut in half at the end of a partial read.
fn utf8_up_to_a_cut(bytes: &[u8], truncated: bool) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(e) => truncated && e.error_len().is_none(),
    }
}

fn line_ending(text: &str) -> &'static str {
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    let b = text.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'\n' if i > 0 && b[i - 1] == b'\r' => crlf += 1,
            b'\n' => lf += 1,
            b'\r' if b.get(i + 1) != Some(&b'\n') => cr += 1,
            _ => {}
        }
    }
    match (crlf > 0, lf > 0, cr > 0) {
        (false, false, false) => "",
        (true, false, false) => "CRLF",
        (false, true, false) => "LF",
        (false, false, true) => "CR",
        _ => "Mixed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings() {
        assert_eq!(decode("héllo\n".as_bytes(), false, 7).encoding, "UTF-8");
        let bom = decode(&[0xEF, 0xBB, 0xBF, b'h', b'i'], false, 5);
        assert_eq!((bom.encoding.as_str(), bom.text.as_str()), ("UTF-8 with BOM", "hi"));
        let utf16 = decode(&[0xFF, 0xFE, b'h', 0, b'i', 0], false, 6);
        assert_eq!((utf16.encoding.as_str(), utf16.text.as_str()), ("UTF-16 LE", "hi"));
        // "café" in Windows-1252: not UTF-8, so guessed.
        let legacy = decode(b"caf\xe9 au lait, cr\xe8me br\xfbl\xe9e\n", false, 30);
        assert_eq!(legacy.encoding, "windows-1252");
        assert!(legacy.text.starts_with("café"));
    }

    #[test]
    fn a_partial_read_ends_at_a_whole_line() {
        // "é" is two bytes; the read stops between them.
        let bytes = "one\ntwo\nthré".as_bytes();
        let cut = &bytes[..bytes.len() - 1];
        let t = decode(cut, true, 100);
        assert_eq!((t.encoding.as_str(), t.text.as_str(), t.truncated), ("UTF-8", "one\ntwo\n", true));
    }

    #[test]
    fn line_endings() {
        assert_eq!(line_ending("a\r\nb\r\n"), "CRLF");
        assert_eq!(line_ending("a\nb"), "LF");
        assert_eq!(line_ending("a\rb"), "CR");
        assert_eq!(line_ending("a\r\nb\n"), "Mixed");
        assert_eq!(line_ending("one line"), "");
    }
}
