//! A folder as a view shows it: its entries, kept here (in Rust) rather than in the WebView, and
//! an order (sorted, hidden files filtered) the view reads a window of rows from.

use super::entry::{Entry, Kind};
use super::natural;
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Modified,
    Created,
    /// The extension, then the name.
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub sort: SortKey,
    pub descending: bool,
    /// Folders before files whatever the sort (direction only applies within each group).
    pub folders_first: bool,
    pub show_hidden: bool,
}

impl Default for View {
    fn default() -> Self {
        View { sort: SortKey::Name, descending: false, folders_first: true, show_hidden: false }
    }
}

pub struct Folder {
    pub path: PathBuf,
    entries: Vec<Entry>,
    /// Name → index in `entries`, for updates from file system events.
    by_name: HashMap<String, u32>,
    /// What the view shows, in order: indexes into `entries`.
    order: Vec<u32>,
    view: View,
}

fn compare(a: &Entry, b: &Entry, view: &View) -> Ordering {
    if view.folders_first {
        let (fa, fb) = (a.kind == Kind::Folder, b.kind == Kind::Folder);
        if fa != fb {
            return fb.cmp(&fa);
        }
    }
    let by_name = || natural::compare(&a.key, &b.key).then_with(|| a.name.cmp(&b.name));
    let ord = match view.sort {
        SortKey::Name => by_name(),
        SortKey::Size => a.size.cmp(&b.size).then_with(by_name),
        SortKey::Modified => a.modified.total_cmp(&b.modified).then_with(by_name),
        SortKey::Created => a.created.total_cmp(&b.created).then_with(by_name),
        SortKey::Type => a.ext.cmp(&b.ext).then_with(by_name),
    };
    if view.descending { ord.reverse() } else { ord }
}

impl Folder {
    pub fn new(path: PathBuf, view: View) -> Folder {
        Folder { path, entries: Vec::new(), by_name: HashMap::new(), order: Vec::new(), view }
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Rows the view shows.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Every entry, hidden ones included.
    pub fn total(&self) -> usize {
        self.entries.len()
    }

    /// Rows `start..end` of the view (clamped).
    pub fn rows(&self, start: usize, end: usize) -> impl Iterator<Item = &Entry> {
        let end = end.min(self.order.len());
        let start = start.min(end);
        self.order[start..end].iter().map(|&i| &self.entries[i as usize])
    }

    /// The row of a name in the view, if shown.
    pub fn row_of(&self, name: &str) -> Option<usize> {
        let i = *self.by_name.get(name)?;
        self.order.iter().position(|&o| o == i)
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name).map(|&i| &self.entries[i as usize])
    }

    /// Add entries from the listing (or replace ones with the same name), then re-order.
    pub fn extend(&mut self, batch: Vec<Entry>) {
        for e in batch {
            self.put(e);
        }
        self.reorder();
    }

    fn put(&mut self, e: Entry) {
        match self.by_name.get(&e.name) {
            Some(&i) => self.entries[i as usize] = e,
            None => {
                self.by_name.insert(e.name.clone(), self.entries.len() as u32);
                self.entries.push(e);
            }
        }
    }

    /// Apply changes reported by file system events: entries added or changed, names removed.
    pub fn apply(&mut self, upserts: Vec<Entry>, removed: &[String]) {
        for e in upserts {
            self.put(e);
        }
        if !removed.is_empty() {
            let gone: std::collections::HashSet<&str> = removed.iter().map(String::as_str).collect();
            self.entries.retain(|e| !gone.contains(e.name.as_str()));
            self.by_name = self.entries.iter().enumerate().map(|(i, e)| (e.name.clone(), i as u32)).collect();
        }
        self.reorder();
    }

    /// Replace everything (a full re-listing).
    pub fn replace(&mut self, entries: Vec<Entry>) {
        self.entries.clear();
        self.by_name.clear();
        self.extend(entries);
    }

    pub fn set_view(&mut self, view: View) {
        if view != self.view {
            self.view = view;
            self.reorder();
        }
    }

    fn reorder(&mut self) {
        let show_hidden = self.view.show_hidden;
        let entries = &self.entries;
        let mut order: Vec<u32> = (0..entries.len() as u32).filter(|&i| show_hidden || !entries[i as usize].hidden).collect();
        let view = self.view;
        order.sort_by(|&a, &b| compare(&entries[a as usize], &entries[b as usize], &view));
        self.order = order;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: Kind, size: u64, modified: f64, hidden: bool) -> Entry {
        Entry {
            name: name.into(),
            key: name.to_lowercase().into(),
            kind,
            size,
            modified,
            created: modified,
            ext: if kind == Kind::File { super::super::entry::extension_of(name) } else { String::new() },
            hidden,
            link: false,
            readonly: false,
        }
    }

    fn names(f: &Folder) -> Vec<&str> {
        f.rows(0, usize::MAX).map(|e| e.name.as_str()).collect()
    }

    fn sample() -> Folder {
        let mut f = Folder::new(PathBuf::from("x"), View::default());
        f.extend(vec![
            entry("b.txt", Kind::File, 30, 3.0, false),
            entry("src", Kind::Folder, 0, 1.0, false),
            entry("A10.rs", Kind::File, 10, 2.0, false),
            entry("a9.rs", Kind::File, 20, 4.0, false),
            entry(".git", Kind::Folder, 0, 5.0, true),
            entry("docs", Kind::Folder, 0, 6.0, false),
        ]);
        f
    }

    #[test]
    fn folders_first_natural_names_hidden_filtered() {
        let f = sample();
        assert_eq!(names(&f), vec!["docs", "src", "a9.rs", "A10.rs", "b.txt"]);
        assert_eq!((f.len(), f.total()), (5, 6));
    }

    #[test]
    fn sorting_and_showing_hidden() {
        let mut f = sample();
        f.set_view(View { sort: SortKey::Size, descending: true, folders_first: true, show_hidden: true });
        assert_eq!(names(&f), vec!["src", "docs", ".git", "b.txt", "a9.rs", "A10.rs"]);
        f.set_view(View { sort: SortKey::Modified, descending: false, folders_first: false, show_hidden: false });
        assert_eq!(names(&f), vec!["src", "A10.rs", "b.txt", "a9.rs", "docs"]);
        f.set_view(View { sort: SortKey::Type, ..View::default() });
        assert_eq!(names(&f), vec!["docs", "src", "a9.rs", "A10.rs", "b.txt"]);
    }

    #[test]
    fn events_add_change_and_remove() {
        let mut f = sample();
        f.apply(vec![entry("a1.rs", Kind::File, 1, 9.0, false), entry("b.txt", Kind::File, 99, 9.0, false)], &["src".into()]);
        assert_eq!(names(&f), vec!["docs", "a1.rs", "a9.rs", "A10.rs", "b.txt"]);
        assert_eq!(f.get("b.txt").unwrap().size, 99);
        assert_eq!(f.row_of("a9.rs"), Some(2));
        assert_eq!(f.row_of("src"), None);
        assert_eq!(f.row_of(".git"), None, "hidden");
        assert_eq!(f.rows(3, 100).count(), 2);
        assert_eq!(f.rows(10, 20).count(), 0);
    }
}
