//! Natural order for names, as Explorer and Finder sort them: "file2" before "file10",
//! case-insensitive, with a stable tie-break so equal-looking names keep one order.

use std::cmp::Ordering;

/// Compare two lower-case names naturally: runs of digits compare by value.
pub fn compare(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    loop {
        match (x.first(), y.first()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let (nx, rx) = split_digits(x);
                let (ny, ry) = split_digits(y);
                // Compare by value without parsing (numbers may be longer than u64):
                // ignore leading zeros, then the longer number is bigger, then digit by digit.
                let (tx, ty) = (trim_zeros(nx), trim_zeros(ny));
                let ord = tx.len().cmp(&ty.len()).then_with(|| tx.cmp(ty));
                if ord != Ordering::Equal {
                    return ord;
                }
                // "01" and "1" are equal in value: fewer leading zeros first.
                if nx.len() != ny.len() {
                    return nx.len().cmp(&ny.len());
                }
                x = rx;
                y = ry;
            }
            (Some(_), Some(_)) => {
                // Compare one character (UTF-8 sequence) at a time.
                let cx = char_len(x[0]);
                let cy = char_len(y[0]);
                let ord = x[..cx.min(x.len())].cmp(&y[..cy.min(y.len())]);
                if ord != Ordering::Equal {
                    return ord;
                }
                x = &x[cx.min(x.len())..];
                y = &y[cy.min(y.len())..];
            }
        }
    }
}

fn split_digits(s: &[u8]) -> (&[u8], &[u8]) {
    let n = s.iter().take_while(|c| c.is_ascii_digit()).count();
    s.split_at(n)
}

fn trim_zeros(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|&&c| c == b'0').count();
    &s[n.min(s.len().saturating_sub(1))..]
}

fn char_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::compare;
    use std::cmp::Ordering::*;

    fn sorted(mut v: Vec<&str>) -> Vec<&str> {
        v.sort_by(|a, b| compare(a, b));
        v
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(sorted(vec!["file10", "file2", "file1"]), vec!["file1", "file2", "file10"]);
        assert_eq!(sorted(vec!["v1.10.0", "v1.9.2", "v1.2.0"]), vec!["v1.2.0", "v1.9.2", "v1.10.0"]);
        assert_eq!(compare("a01", "a1"), Greater, "same value: fewer leading zeros first");
        assert_eq!(compare("a0", "a00"), Less);
        assert_eq!(compare("x99999999999999999999999", "x100000000000000000000000"), Less, "longer than u64");
    }

    #[test]
    fn plain_text_and_prefixes() {
        assert_eq!(compare("abc", "abd"), Less);
        assert_eq!(compare("ab", "abc"), Less);
        assert_eq!(compare("", ""), Equal);
        assert_eq!(sorted(vec!["b", "a10", "a9", "a"]), vec!["a", "a9", "a10", "b"]);
        assert_eq!(compare("é1", "é2"), Less);
        assert_eq!(compare("日本10", "日本9"), Greater);
    }
}
