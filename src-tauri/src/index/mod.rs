//! The search index: names and metadata (never contents) of everything under the chosen roots,
//! minus what the rules leave out. Compact: one string arena for the names and parallel arrays
//! for the rest (about 45 bytes per entry, plus a lookup table rebuilt on load), saved to one
//! file and loaded at start-up. A child always has a higher id than its folder, so whole
//! subtrees are removed in one pass; removed entries stay as tombstones until `compact`.

mod query;
mod rules;
mod walk;

pub use query::{search, Hit, Query, SearchResult};
pub use rules::{all_rules, RuleInfo, Rules};
pub use walk::{build, list_into, walk_into};

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// No folder above (a root).
pub const NONE: u32 = u32::MAX;

pub const KIND_FILE: u8 = 0;
pub const KIND_FOLDER: u8 = 1;

pub const FLAG_DELETED: u8 = 1;
pub const FLAG_HIDDEN: u8 = 2;
/// A folder whose contents aren't listed (cloud-only, or couldn't be read).
pub const FLAG_UNLISTED: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meta {
    pub kind: u8,
    pub hidden: bool,
    pub size: u64,
    /// Milliseconds since 1970.
    pub mtime: i64,
}

#[derive(Default)]
pub struct Index {
    parent: Vec<u32>,
    name_at: Vec<u32>,
    name_len: Vec<u16>,
    arena: String,
    kind: Vec<u8>,
    flags: Vec<u8>,
    size: Vec<u64>,
    mtime: Vec<i64>,
    /// The roots, with their ids.
    roots: Vec<(PathBuf, u32)>,
    /// (folder, lower-case name) → id, for updates by path. Not saved.
    lookup: HashMap<u64, u32>,
    deleted: usize,
    /// The rules' fingerprint when built: other rules mean a rebuild.
    pub rules: u64,
}

fn child_key(parent: u32, name: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parent.hash(&mut h);
    for c in name.chars().flat_map(char::to_lowercase) {
        c.hash(&mut h);
    }
    h.finish()
}

/// A path's parts as the index compares them.
fn same(a: &str, b: &str) -> bool {
    if cfg!(any(windows, target_os = "macos")) { a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase() } else { a == b }
}

impl Index {
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Entries in the index (not counting removed ones).
    pub fn live(&self) -> usize {
        self.parent.len() - self.deleted
    }

    pub fn roots(&self) -> &[(PathBuf, u32)] {
        &self.roots
    }

    /// Roughly the memory the index takes.
    pub fn bytes(&self) -> usize {
        self.parent.len() * (4 + 4 + 2 + 1 + 1 + 8 + 8) + self.arena.len() + self.lookup.len() * 16
    }

    pub fn name(&self, id: u32) -> &str {
        let i = id as usize;
        let at = self.name_at[i] as usize;
        &self.arena[at..at + self.name_len[i] as usize]
    }

    pub fn parent(&self, id: u32) -> u32 {
        self.parent[id as usize]
    }

    pub fn kind(&self, id: u32) -> u8 {
        self.kind[id as usize]
    }

    pub fn flags(&self, id: u32) -> u8 {
        self.flags[id as usize]
    }

    pub fn size(&self, id: u32) -> u64 {
        self.size[id as usize]
    }

    pub fn mtime(&self, id: u32) -> i64 {
        self.mtime[id as usize]
    }

    pub fn is_live(&self, id: u32) -> bool {
        self.flags[id as usize] & FLAG_DELETED == 0
    }

    /// The full path of an entry.
    pub fn path(&self, id: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut at = id;
        loop {
            let p = self.parent[at as usize];
            if p == NONE {
                let root = self.roots.iter().find(|(_, r)| *r == at).map(|(p, _)| p.clone()).unwrap_or_default();
                let mut out = root;
                for part in parts.iter().rev() {
                    out.push(part);
                }
                return out;
            }
            parts.push(self.name(at));
            at = p;
        }
    }

    /// How many folders deep below its root.
    pub fn depth(&self, id: u32) -> u32 {
        let mut d = 0;
        let mut at = self.parent[id as usize];
        while at != NONE {
            d += 1;
            at = self.parent[at as usize];
        }
        d
    }

    /// Whether `id` is `folder` or inside it.
    pub fn within(&self, id: u32, folder: u32) -> bool {
        let mut at = id;
        while at != NONE {
            if at == folder {
                return true;
            }
            at = self.parent[at as usize];
        }
        false
    }

    /// Add an entry (a child of `parent`, or a root with `NONE` and its full path as the name).
    pub fn push(&mut self, parent: u32, name: &str, meta: Meta) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(parent);
        self.name_at.push(self.arena.len() as u32);
        self.name_len.push(name.len().min(u16::MAX as usize) as u16);
        self.arena.push_str(name);
        self.kind.push(meta.kind);
        self.flags.push(if meta.hidden { FLAG_HIDDEN } else { 0 });
        self.size.push(meta.size);
        self.mtime.push(meta.mtime);
        if parent != NONE {
            self.lookup.insert(child_key(parent, name), id);
        }
        id
    }

    /// Add a root folder (`path` must be absolute); returns its id.
    pub fn add_root(&mut self, path: &Path, meta: Meta) -> u32 {
        let id = self.push(NONE, &path.to_string_lossy(), meta);
        self.roots.push((path.to_path_buf(), id));
        id
    }

    pub fn set_flag(&mut self, id: u32, flag: u8, on: bool) {
        let f = &mut self.flags[id as usize];
        *f = if on { *f | flag } else { *f & !flag };
    }

    pub fn update(&mut self, id: u32, meta: Meta) {
        let i = id as usize;
        self.kind[i] = meta.kind;
        self.size[i] = meta.size;
        self.mtime[i] = meta.mtime;
        self.set_flag(id, FLAG_HIDDEN, meta.hidden);
    }

    /// The live child of `parent` called `name`.
    pub fn child(&self, parent: u32, name: &str) -> Option<u32> {
        let id = *self.lookup.get(&child_key(parent, name))?;
        (self.is_live(id) && self.parent[id as usize] == parent && same(self.name(id), name)).then_some(id)
    }

    /// The live children of `folder` (a scan: for reconciling one folder at a time).
    pub fn children(&self, folder: u32) -> Vec<u32> {
        (folder + 1..self.len() as u32).filter(|&i| self.parent[i as usize] == folder && self.is_live(i)).collect()
    }

    /// The entry at `path`, if indexed (either slash on Windows).
    pub fn find(&self, path: &Path) -> Option<u32> {
        let k = path.to_string_lossy();
        let k = if cfg!(windows) { k.replace('/', "\\") } else { k.into_owned() };
        for (root, id) in &self.roots {
            let r = root.to_string_lossy();
            let r = if cfg!(windows) { r.replace('/', "\\") } else { r.into_owned() };
            let rest = if same(&k, &r) {
                ""
            } else if k.len() > r.len() && same(&k[..r.len()], &r) && k[r.len()..].starts_with(['\\', '/']) {
                &k[r.len() + 1..]
            } else if r.ends_with(['\\', '/']) && k.len() >= r.len() && same(&k[..r.len()], &r) {
                &k[r.len()..]
            } else {
                continue;
            };
            let mut at = *id;
            for part in rest.split(['\\', '/']).filter(|p| !p.is_empty()) {
                at = self.child(at, part)?;
            }
            return Some(at);
        }
        None
    }

    /// Remove an entry and everything under it.
    pub fn remove(&mut self, id: u32) {
        if !self.is_live(id) {
            return;
        }
        self.set_flag(id, FLAG_DELETED, true);
        self.deleted += 1;
        if self.kind(id) == KIND_FOLDER {
            // Children come after their folder.
            for i in id + 1..self.len() as u32 {
                let p = self.parent[i as usize];
                if p != NONE && !self.is_live(p) && self.is_live(i) {
                    self.set_flag(i, FLAG_DELETED, true);
                    self.deleted += 1;
                }
            }
        }
    }

    /// Whether enough is removed to be worth compacting.
    pub fn wants_compact(&self) -> bool {
        self.deleted > 10_000 && self.deleted * 5 > self.len()
    }

    /// Drop removed entries (ids change).
    pub fn compact(&mut self) {
        let mut out = Index { rules: self.rules, ..Index::default() };
        let mut new_id = vec![NONE; self.len()];
        for i in 0..self.len() as u32 {
            if !self.is_live(i) {
                continue;
            }
            let p = self.parent[i as usize];
            let np = if p == NONE { NONE } else { new_id[p as usize] };
            if p != NONE && np == NONE {
                continue;
            }
            let meta = Meta { kind: self.kind(i), hidden: self.flags(i) & FLAG_HIDDEN != 0, size: self.size(i), mtime: self.mtime(i) };
            let id = out.push(np, self.name(i), meta);
            out.flags[id as usize] = self.flags(i);
            new_id[i as usize] = id;
        }
        out.roots = self.roots.iter().filter(|(_, id)| new_id[*id as usize] != NONE).map(|(p, id)| (p.clone(), new_id[*id as usize])).collect();
        *self = out;
    }

    // ---------- saving and loading ----------

    const MAGIC: &'static [u8; 8] = b"FBIDX\x00\x00\x02";

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        let mut copy;
        let me = if self.deleted > 0 {
            copy = Index { rules: self.rules, ..Index::default() };
            copy.clone_from_live(self);
            &copy
        } else {
            self
        };
        let tmp = file.with_extension("tmp");
        {
            let mut w = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            w.write_all(Self::MAGIC)?;
            w.write_all(&me.rules.to_le_bytes())?;
            w.write_all(&(me.len() as u64).to_le_bytes())?;
            w.write_all(&(me.arena.len() as u64).to_le_bytes())?;
            w.write_all(&(me.roots.len() as u32).to_le_bytes())?;
            for (p, id) in &me.roots {
                let s = p.to_string_lossy();
                w.write_all(&(s.len() as u32).to_le_bytes())?;
                w.write_all(s.as_bytes())?;
                w.write_all(&id.to_le_bytes())?;
            }
            for v in &me.parent {
                w.write_all(&v.to_le_bytes())?;
            }
            for v in &me.name_at {
                w.write_all(&v.to_le_bytes())?;
            }
            for v in &me.name_len {
                w.write_all(&v.to_le_bytes())?;
            }
            w.write_all(&me.kind)?;
            w.write_all(&me.flags)?;
            for v in &me.size {
                w.write_all(&v.to_le_bytes())?;
            }
            for v in &me.mtime {
                w.write_all(&v.to_le_bytes())?;
            }
            w.write_all(me.arena.as_bytes())?;
            w.flush()?;
        }
        std::fs::rename(&tmp, file)
    }

    fn clone_from_live(&mut self, other: &Index) {
        let mut c = Index { rules: other.rules, ..Index::default() };
        c.parent.clone_from(&other.parent);
        c.name_at.clone_from(&other.name_at);
        c.name_len.clone_from(&other.name_len);
        c.arena.clone_from(&other.arena);
        c.kind.clone_from(&other.kind);
        c.flags.clone_from(&other.flags);
        c.size.clone_from(&other.size);
        c.mtime.clone_from(&other.mtime);
        c.roots.clone_from(&other.roots);
        c.deleted = other.deleted;
        c.compact();
        *self = c;
    }

    pub fn load(file: &Path) -> std::io::Result<Index> {
        let mut r = std::io::BufReader::new(std::fs::File::open(file)?);
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "not a Fileboat index");
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if &magic != Self::MAGIC {
            return Err(bad());
        }
        let mut b8 = [0u8; 8];
        let mut b4 = [0u8; 4];
        r.read_exact(&mut b8)?;
        let rules = u64::from_le_bytes(b8);
        r.read_exact(&mut b8)?;
        let n = u64::from_le_bytes(b8) as usize;
        r.read_exact(&mut b8)?;
        let arena_len = u64::from_le_bytes(b8) as usize;
        r.read_exact(&mut b4)?;
        let nroots = u32::from_le_bytes(b4) as usize;
        if n > 200_000_000 || arena_len > 8 << 30 || nroots > 10_000 {
            return Err(bad());
        }
        let mut roots = Vec::with_capacity(nroots);
        for _ in 0..nroots {
            r.read_exact(&mut b4)?;
            let mut s = vec![0u8; u32::from_le_bytes(b4) as usize];
            r.read_exact(&mut s)?;
            r.read_exact(&mut b4)?;
            roots.push((PathBuf::from(String::from_utf8(s).map_err(|_| bad())?), u32::from_le_bytes(b4)));
        }
        let mut read_vec = |size: usize| -> std::io::Result<Vec<u8>> {
            let mut v = vec![0u8; n * size];
            r.read_exact(&mut v)?;
            Ok(v)
        };
        let parent: Vec<u32> = read_vec(4)?.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        let name_at: Vec<u32> = read_vec(4)?.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        let name_len: Vec<u16> = read_vec(2)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let kind = read_vec(1)?;
        let flags = read_vec(1)?;
        let size: Vec<u64> = read_vec(8)?.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c)).collect();
        let mtime: Vec<i64> = read_vec(8)?.as_chunks::<8>().0.iter().map(|c| i64::from_le_bytes(*c)).collect();
        let mut arena = vec![0u8; arena_len];
        r.read_exact(&mut arena)?;
        let arena = String::from_utf8(arena).map_err(|_| bad())?;
        let mut idx = Index { parent, name_at, name_len, arena, kind, flags, size, mtime, roots, lookup: HashMap::new(), deleted: 0, rules };
        // Check the arrays point where they should before trusting them.
        for i in 0..n {
            let at = idx.name_at[i] as usize;
            let end = at + idx.name_len[i] as usize;
            let p = idx.parent[i];
            if end > idx.arena.len() || !idx.arena.is_char_boundary(at) || !idx.arena.is_char_boundary(end) || (p != NONE && p as usize >= i) {
                return Err(bad());
            }
        }
        idx.lookup.reserve(n);
        for i in 0..n as u32 {
            let p = idx.parent[i as usize];
            if p != NONE {
                let k = child_key(p, idx.name(i));
                idx.lookup.insert(k, i);
            }
        }
        Ok(idx)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn file(size: u64) -> Meta {
        Meta { kind: KIND_FILE, hidden: false, size, mtime: 1000 }
    }
    pub fn folder() -> Meta {
        Meta { kind: KIND_FOLDER, hidden: false, size: 0, mtime: 1000 }
    }

    pub fn sample() -> Index {
        let mut idx = Index::default();
        let root = idx.add_root(Path::new(r"C:\Users\me"), folder());
        let proj = idx.push(root, "projects", folder());
        let app = idx.push(proj, "app", folder());
        idx.push(app, "main.rs", file(100));
        idx.push(app, "README.md", file(2000));
        let src = idx.push(app, "src", folder());
        idx.push(src, "lib.rs", file(50));
        idx.push(root, "notes.txt", file(10));
        idx
    }

    #[test]
    fn paths_lookup_and_removal() {
        let mut idx = sample();
        let lib = idx.find(Path::new(r"C:\Users\me\projects\app\src\lib.rs")).unwrap();
        assert_eq!(idx.name(lib), "lib.rs");
        assert_eq!(idx.path(lib), PathBuf::from(r"C:\Users\me").join("projects").join("app").join("src").join("lib.rs"));
        assert_eq!(idx.depth(lib), 4);
        assert_eq!(idx.find(Path::new(r"c:\users\ME\Projects\APP\readme.md")).map(|i| idx.name(i)), Some("README.md"));
        let app = idx.find(Path::new(r"C:\Users\me\projects\app")).unwrap();
        #[cfg(windows)]
        assert_eq!(idx.find(Path::new("C:/Users/me/projects/app")), Some(app), "forward slashes");
        assert!(idx.within(lib, app));
        assert_eq!(idx.children(app).len(), 3);
        idx.remove(app);
        assert!(idx.find(Path::new(r"C:\Users\me\projects\app\src\lib.rs")).is_none());
        assert_eq!(idx.live(), 3, "root, projects, notes.txt");
        idx.compact();
        assert_eq!(idx.len(), 3);
        assert!(idx.find(Path::new(r"C:\Users\me\notes.txt")).is_some());
    }

    #[test]
    fn save_and_load() {
        let mut idx = sample();
        idx.rules = 42;
        let gone = idx.find(Path::new(r"C:\Users\me\notes.txt")).unwrap();
        idx.remove(gone);
        let f = std::env::temp_dir().join(format!("fileboat-index-{}.bin", std::process::id()));
        idx.save(&f).unwrap();
        let back = Index::load(&f).unwrap();
        assert_eq!((back.len(), back.rules), (7, 42));
        let lib = back.find(Path::new(r"C:\Users\me\projects\app\src\lib.rs")).unwrap();
        assert_eq!(back.size(lib), 50);
        assert!(back.find(Path::new(r"C:\Users\me\notes.txt")).is_none());
        std::fs::write(&f, b"garbage").unwrap();
        assert!(Index::load(&f).is_err());
        std::fs::remove_file(&f).unwrap();
    }
}
