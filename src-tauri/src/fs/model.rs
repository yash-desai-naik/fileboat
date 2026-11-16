//! A folder as a view shows it: its entries, kept here (in Rust) rather than in the WebView, and
//! an order (sorted, hidden files filtered) the view reads a window of rows from.

use super::entry::{Entry, Kind};
use super::group::{self, Clock, Group, GroupBy};
use super::natural;
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
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
#[serde(rename_all = "camelCase", default)]
pub struct View {
    pub sort: SortKey,
    pub descending: bool,
    /// Folders before files whatever the sort (direction only applies within each group).
    pub folders_first: bool,
    pub show_hidden: bool,
    pub group: GroupBy,
    /// The viewer's clock, for date groups: now (ms since 1970) and the local offset (minutes).
    pub now: i64,
    pub tz_offset_min: i32,
    /// Work out folder sizes (in the background, by the app).
    pub folder_sizes: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            sort: SortKey::Name,
            descending: false,
            folders_first: true,
            show_hidden: false,
            group: GroupBy::None,
            now: 0,
            tz_offset_min: 0,
            folder_sizes: false,
        }
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
    /// The groups, in order (empty when not grouping).
    groups: Vec<Group>,
    /// Keys of collapsed groups: their rows aren't in `order`.
    collapsed: HashSet<String>,
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
        Folder { path, entries: Vec::new(), by_name: HashMap::new(), order: Vec::new(), view, groups: Vec::new(), collapsed: HashSet::new() }
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
            if view.group != self.view.group {
                self.collapsed.clear();
            }
            // Turning folder sizes on alone moves no rows; turning them off forgets them.
            let mut resort = View { folder_sizes: view.folder_sizes, ..self.view } != view;
            if self.view.folder_sizes && !view.folder_sizes {
                for e in self.entries.iter_mut().filter(|e| e.sized) {
                    e.size = 0;
                    e.sized = false;
                }
                resort = true;
            }
            self.view = view;
            if resort {
                self.reorder();
            }
        }
    }

    /// Folders (not links to them) whose size isn't known yet.
    pub fn unsized_folders(&self) -> Vec<String> {
        self.entries.iter().filter(|e| e.kind == Kind::Folder && !e.sized && !e.link).map(|e| e.name.clone()).collect()
    }

    /// Set folder sizes (name, bytes); re-orders if that moves rows. Returns whether any changed.
    pub fn set_sizes<'a>(&mut self, sizes: impl IntoIterator<Item = (&'a str, u64)>) -> bool {
        let mut changed = false;
        for (name, bytes) in sizes {
            let Some(&i) = self.by_name.get(name) else { continue };
            let e = &mut self.entries[i as usize];
            if e.kind == Kind::Folder && (!e.sized || e.size != bytes) {
                e.size = bytes;
                e.sized = true;
                changed = true;
            }
        }
        // Sizes show in the Size column and group totals; rows move only when sorted or grouped by size.
        if changed {
            self.reorder();
        }
        changed
    }

    /// The groups in order (empty when not grouping).
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// Collapse or expand a group.
    pub fn set_collapsed(&mut self, key: &str, collapsed: bool) {
        let changed = if collapsed { self.collapsed.insert(key.to_string()) } else { self.collapsed.remove(key) };
        if changed {
            self.reorder();
        }
    }

    fn reorder(&mut self) {
        let show_hidden = self.view.show_hidden;
        let entries = &self.entries;
        let mut order: Vec<u32> = (0..entries.len() as u32).filter(|&i| show_hidden || !entries[i as usize].hidden).collect();
        let view = self.view;
        if view.group == GroupBy::None {
            order.sort_by(|&a, &b| compare(&entries[a as usize], &entries[b as usize], &view));
            self.order = order;
            self.groups.clear();
            return;
        }
        // Each entry's group, worked out once; rows sort by group, then as usual within it.
        let clock = Clock { now: view.now, tz_offset_min: view.tz_offset_min };
        let mut keyed: Vec<(group::GroupOf, u32)> =
            order.into_iter().map(|i| (group::group_of(&entries[i as usize], view.group, &clock).expect("grouping"), i)).collect();
        keyed.sort_by(|(ga, a), (gb, b)| {
            ga.rank
                .cmp(&gb.rank)
                .then_with(|| natural::compare(&ga.sub, &gb.sub))
                .then_with(|| compare(&entries[*a as usize], &entries[*b as usize], &view))
        });
        let mut groups: Vec<Group> = Vec::new();
        let mut shown: Vec<u32> = Vec::with_capacity(keyed.len());
        for (g, i) in keyed {
            if groups.last().is_none_or(|last| last.key != g.key) {
                let collapsed = self.collapsed.contains(&g.key);
                groups.push(Group { key: g.key, label: g.label, start: shown.len(), count: 0, bytes: 0, collapsed });
            }
            let last = groups.last_mut().expect("a group");
            last.count += 1;
            let e = &entries[i as usize];
            if e.kind != Kind::Folder || e.sized {
                last.bytes += e.size;
            }
            if !last.collapsed {
                shown.push(i);
            }
        }
        self.order = shown;
        self.groups = groups;
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
            sized: false,
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
        f.set_view(View { sort: SortKey::Size, descending: true, folders_first: true, show_hidden: true, ..View::default() });
        assert_eq!(names(&f), vec!["src", "docs", ".git", "b.txt", "a9.rs", "A10.rs"]);
        f.set_view(View { sort: SortKey::Modified, descending: false, folders_first: false, show_hidden: false, ..View::default() });
        assert_eq!(names(&f), vec!["src", "A10.rs", "b.txt", "a9.rs", "docs"]);
        f.set_view(View { sort: SortKey::Type, ..View::default() });
        assert_eq!(names(&f), vec!["docs", "src", "a9.rs", "A10.rs", "b.txt"]);
    }

    #[test]
    fn groups_by_type_with_collapsing() {
        let mut f = sample();
        f.set_view(View { group: GroupBy::Type, show_hidden: true, ..View::default() });
        let summary: Vec<(String, usize, usize)> = f.groups().iter().map(|g| (g.label.clone(), g.start, g.count)).collect();
        assert_eq!(
            summary,
            vec![("Folders".into(), 0, 3), ("rs".into(), 3, 2), ("txt".into(), 5, 1)],
            "folders, then files by extension"
        );
        assert_eq!(names(&f), vec![".git", "docs", "src", "a9.rs", "A10.rs", "b.txt"]);
        assert_eq!(f.groups()[1].bytes, 30);

        f.set_collapsed("ext:rs", true);
        assert_eq!(names(&f), vec![".git", "docs", "src", "b.txt"]);
        let rs = &f.groups()[1];
        assert_eq!((rs.collapsed, rs.start, rs.count), (true, 3, 2));
        assert_eq!(f.groups()[2].start, 3, "the next group starts where the collapsed one would");
        f.set_collapsed("ext:rs", false);
        assert_eq!(f.len(), 6);

        // Changing how to group starts with every group open.
        f.set_collapsed("folder", true);
        f.set_view(View { group: GroupBy::Letter, show_hidden: true, ..View::default() });
        assert!(f.groups().iter().all(|g| !g.collapsed));
        let letters: Vec<&str> = f.groups().iter().map(|g| g.label.as_str()).collect();
        assert_eq!(letters, vec!["A", "B", "D", "G", "S"]);
    }

    #[test]
    fn folder_sizes_sort_and_add_to_group_totals() {
        let mut f = sample();
        assert_eq!(f.unsized_folders().len(), 3);
        f.set_view(View { sort: SortKey::Size, descending: true, ..View::default() });
        assert!(f.set_sizes([("docs", 5000), ("src", 10)]));
        assert_eq!(&names(&f)[..2], ["docs", "src"]);
        assert!(!f.set_sizes([("docs", 5000)]), "the same size again changes nothing");
        assert!(!f.set_sizes([("b.txt", 1)]), "files keep their own size");
        assert_eq!(f.unsized_folders(), vec![".git"]);
        f.set_view(View { group: GroupBy::Type, show_hidden: true, ..View::default() });
        assert_eq!(f.groups()[0].bytes, 5010, "sized folders count; .git isn't sized yet");

        // Turning sizes off forgets them.
        f.set_view(View { folder_sizes: true, ..View::default() });
        assert!(f.unsized_folders() == vec![".git"]);
        f.set_view(View::default());
        assert_eq!(f.unsized_folders().len(), 3);
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
