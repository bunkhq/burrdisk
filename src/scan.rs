use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rayon::prelude::*;

#[cfg(windows)]
mod mft;

pub type NodeId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Dir,
    File,
    /// Space the volume reports as used that the scan couldn't attribute to any file
    /// (snapshots, system files we can't read, other APFS volumes in the container)
    Hidden,
}

/// One item of a [`Tree`], borrowed from it
#[derive(Clone, Copy, Debug)]
pub struct Node<'a> {
    pub name: &'a str,
    pub size: u64,
    /// Files inside (1 for a file); folders themselves aren't counted
    pub items: u64,
    pub kind: NodeKind,
    /// When it last changed, in seconds since 1970; for a folder, the newest change
    /// to anything inside. 0 when unknown
    pub modified: u32,
    pub parent: Option<NodeId>,
    /// Sorted largest first
    pub children: &'a [NodeId],
}

const NO_PARENT: u32 = u32::MAX;

/// How a node is stored: 40 bytes, with its name in `Tree::names` and its children's
/// ids in `Tree::kids`, so a scan of millions of files makes no allocation per file
struct Slot {
    size: u64,
    items: u64,
    name: u32,
    parent: u32,
    /// The node's children are `kids[first..first + count]`
    first: u32,
    count: u32,
    modified: u32,
    /// File names are at most 255 characters, so their UTF-8 fits in 16 bits
    name_len: u16,
    kind: NodeKind,
}

pub struct Tree {
    slots: Vec<Slot>,
    names: String,
    kids: Vec<NodeId>,
    root_path: PathBuf,
    /// How long the scan took
    pub duration: Duration,
}

impl Tree {
    pub const ROOT: NodeId = 0;

    pub fn node(&self, id: NodeId) -> Node<'_> {
        let slot = &self.slots[id as usize];
        let name = slot.name as usize;
        let first = slot.first as usize;
        Node {
            name: &self.names[name..name + slot.name_len as usize],
            size: slot.size,
            items: slot.items,
            kind: slot.kind,
            modified: slot.modified,
            parent: (slot.parent != NO_PARENT).then_some(slot.parent),
            children: &self.kids[first..first + slot.count as usize],
        }
    }

    /// Where the scan started
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut parts = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            if current == Self::ROOT {
                break;
            }
            let node = self.node(current);
            parts.push(node.name);
            cursor = node.parent;
        }
        let mut path = self.root_path.clone();
        for part in parts.into_iter().rev() {
            path.push(part);
        }
        path
    }

    /// Ancestors from the root down to (and including) `id`
    pub fn lineage(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut cursor = self.node(id).parent;
        while let Some(parent) = cursor {
            out.push(parent);
            cursor = self.node(parent).parent;
        }
        out.reverse();
        out
    }

    pub fn is_ancestor(&self, ancestor: NodeId, mut id: NodeId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            match self.node(id).parent {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }

    /// Whether a node is still reachable from the root. It isn't once it, or any
    /// folder it was in, is removed
    pub fn is_attached(&self, mut id: NodeId) -> bool {
        // Detaching a node cuts its link to its parent, so the way up from a detached
        // node ends short of the root
        loop {
            match self.slots[id as usize].parent {
                NO_PARENT => return id == Self::ROOT,
                parent => id = parent,
            }
        }
    }

    /// Detaches a node after its files were deleted, shrinking every ancestor
    pub fn remove(&mut self, id: NodeId) {
        let Some(parent) = self.node(id).parent else {
            return;
        };
        let (size, items) = (self.node(id).size, self.node(id).items);
        let (first, count) = {
            let slot = &self.slots[parent as usize];
            (slot.first as usize, slot.count as usize)
        };
        if let Some(pos) = self.kids[first..first + count].iter().position(|c| *c == id) {
            self.kids.copy_within(first + pos + 1..first + count, first + pos);
            self.slots[parent as usize].count -= 1;
        }
        let mut cursor = Some(parent);
        while let Some(current) = cursor {
            let slot = &mut self.slots[current as usize];
            slot.size = slot.size.saturating_sub(size);
            slot.items = slot.items.saturating_sub(items);
            cursor = self.node(current).parent;
        }
        // Every folder up the chain shrank, so each may now rank lower among its siblings
        let mut cursor = self.node(parent).parent;
        while let Some(current) = cursor {
            self.sort_children(current);
            cursor = self.node(current).parent;
        }
        self.slots[id as usize].parent = NO_PARENT;
    }

    fn sort_children(&mut self, id: NodeId) {
        let slot = &self.slots[id as usize];
        let (first, count) = (slot.first as usize, slot.count as usize);
        let slots = &self.slots;
        self.kids[first..first + count].sort_by(|a, b| slots[*b as usize].size.cmp(&slots[*a as usize].size));
    }

    /// The deepest folder in the tree on the way to `path`, or `None` when the path
    /// is outside what was scanned
    pub fn deepest(&self, path: &Path) -> Option<NodeId> {
        let inside = path.strip_prefix(&self.root_path).ok()?;
        let mut id = Self::ROOT;
        for part in inside.components() {
            let part = part.as_os_str().to_string_lossy();
            let next = self.node(id).children.iter().copied().find(|c| {
                let c = self.node(*c);
                c.kind == NodeKind::Dir && c.name == part
            });
            match next {
                Some(next) => id = next,
                None => break,
            }
        }
        Some(id)
    }

    /// The names of a folder's subfolders, which a listing of it again needn't walk
    pub fn subfolders(&self, id: NodeId) -> HashSet<String> {
        self.node(id)
            .children
            .iter()
            .map(|c| self.node(*c))
            .filter(|c| c.kind == NodeKind::Dir)
            .map(|c| c.name.to_string())
            .collect()
    }

    /// Brings a folder up to date with a fresh listing of it. Items still there keep
    /// their ids, so anything pointing at them stays valid; items gone are detached
    /// as `remove` does. Returns whether anything changed
    pub fn apply(&mut self, relisted: Relisted) -> bool {
        let Relisted { id, entries } = relisted;
        if self.node(id).kind != NodeKind::Dir || !self.is_attached(id) {
            return false;
        }
        let old: Vec<NodeId> = self.node(id).children.to_vec();
        let old_sum: u64 = old.iter().map(|c| self.node(*c).size).sum();
        let own = self.node(id).size.saturating_sub(old_sum);
        let mut by_name: HashMap<String, NodeId> = old.iter().map(|c| (self.node(*c).name.to_string(), *c)).collect();
        let mut kids = Vec::with_capacity(entries.len() + 1);
        // Items replaced by something else of the same name
        let mut gone = Vec::new();
        let mut changed = false;
        for Entry { name, size, modified, kind } in entries {
            let existing = by_name.remove(&name);
            let existing_kind = existing.map(|c| self.node(c).kind);
            match (kind, existing) {
                (EntryKind::Known, Some(c)) if existing_kind == Some(NodeKind::Dir) => kids.push(c),
                // Changed again since it was listed; the next listing catches up
                (EntryKind::Known, _) => gone.extend(existing),
                (EntryKind::File { linked }, Some(c)) if existing_kind == Some(NodeKind::File) => {
                    let slot = &mut self.slots[c as usize];
                    // Another link to a file is counted where the scan first met it
                    let size = if linked { slot.size } else { size };
                    if slot.size != size || slot.modified != modified {
                        slot.size = size;
                        slot.modified = modified;
                        changed = true;
                    }
                    kids.push(c);
                }
                (kind, existing) => {
                    gone.extend(existing);
                    // A new link to a file already counted takes no more space
                    let size = if matches!(kind, EntryKind::File { linked: true }) { 0 } else { size };
                    changed = true;
                    kids.push(self.add_child(id, &name, size, modified, kind));
                }
            }
        }
        for c in by_name.into_values().chain(gone) {
            // Hidden space isn't a file, so it's never listed
            if self.node(c).kind == NodeKind::Hidden {
                kids.push(c);
            } else {
                self.slots[c as usize].parent = NO_PARENT;
                changed = true;
            }
        }
        if !changed {
            return false;
        }

        let slots = &self.slots;
        kids.sort_by(|a, b| slots[*b as usize].size.cmp(&slots[*a as usize].size));
        let slot = &self.slots[id as usize];
        let (first, count) = (slot.first as usize, slot.count as usize);
        let first = if kids.len() <= count {
            self.kids[first..first + kids.len()].copy_from_slice(&kids);
            first
        } else {
            let at = self.kids.len();
            self.kids.extend_from_slice(&kids);
            at
        };
        let size = own + kids.iter().map(|c| self.node(*c).size).sum::<u64>();
        let items: u64 = kids.iter().map(|c| self.node(*c).items).sum();
        let newest = kids.iter().map(|c| self.node(*c).modified).max().unwrap_or(0);
        let slot = &mut self.slots[id as usize];
        let (size_delta, items_delta) = (size as i64 - slot.size as i64, items as i64 - slot.items as i64);
        slot.first = first as u32;
        slot.count = kids.len() as u32;
        slot.size = size;
        slot.items = items;
        slot.modified = slot.modified.max(newest);

        let mut cursor = self.node(id).parent;
        while let Some(current) = cursor {
            let slot = &mut self.slots[current as usize];
            slot.size = slot.size.saturating_add_signed(size_delta);
            slot.items = slot.items.saturating_add_signed(items_delta);
            slot.modified = slot.modified.max(newest);
            self.sort_children(current);
            cursor = self.node(current).parent;
        }
        true
    }

    fn add_child(&mut self, parent: NodeId, name: &str, size: u64, modified: u32, kind: EntryKind) -> NodeId {
        let id = self.slots.len() as NodeId;
        let (items, node_kind, dir) = match kind {
            EntryKind::New(dir) => (dir.items, NodeKind::Dir, Some(dir)),
            EntryKind::File { .. } => (1, NodeKind::File, None),
            EntryKind::Known => (0, NodeKind::Dir, None),
        };
        self.slots.push(Slot {
            size,
            items,
            name: self.names.len() as u32,
            name_len: name.len() as u16,
            parent,
            first: self.kids.len() as u32,
            count: 0,
            modified,
            kind: node_kind,
        });
        self.names.push_str(name);
        if let Some(dir) = dir {
            place(*dir, id, self);
        }
        id
    }

    /// Adds a pseudo-entry for space the scan could not see
    pub fn add_hidden_space(&mut self, size: u64) {
        const NAME: &str = "Hidden space";
        let id = self.slots.len() as NodeId;
        self.slots.push(Slot {
            size,
            items: 0,
            name: self.names.len() as u32,
            name_len: NAME.len() as u16,
            parent: Self::ROOT,
            first: 0,
            count: 0,
            modified: 0,
            kind: NodeKind::Hidden,
        });
        self.names.push_str(NAME);
        // The root's children move to the end of `kids`, where there is room for one more
        let root = &self.slots[Self::ROOT as usize];
        let (first, count) = (root.first as usize, root.count as usize);
        let moved = self.kids.len();
        self.kids.extend_from_within(first..first + count);
        self.kids.push(id);
        let root = &mut self.slots[Self::ROOT as usize];
        root.size += size;
        root.first = moved as u32;
        root.count += 1;
        self.sort_children(Self::ROOT);
    }
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub cancelled: AtomicBool,
    pub current: Mutex<String>,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// A folder's listing, built during the parallel walk and flattened into a `Tree`
/// afterwards. Its children's names share one string, so a file costs a 24-byte
/// record instead of a record plus an allocation of its own
#[derive(Default)]
struct Dir {
    names: String,
    children: Vec<Child>,
    /// Totals of the children's sizes and file counts
    size: u64,
    items: u64,
    /// The newest change among the children
    modified: u32,
}

struct Child {
    /// Where this child's name ends in its folder's `names`. It starts where the
    /// previous child's ends
    name_end: u32,
    /// For a folder, the newest change inside it, or its own when it's empty
    modified: u32,
    /// For a folder, its own size plus everything inside
    size: u64,
    dir: Option<Box<Dir>>,
}

impl Child {
    fn items(&self) -> u64 {
        self.dir.as_ref().map_or(1, |d| d.items)
    }
}

struct Walker<'a> {
    // Device and hard link checks need inode numbers, which only Unix exposes
    #[cfg_attr(not(unix), allow(dead_code))]
    devices: Vec<u64>,
    skip: Vec<PathBuf>,
    #[cfg_attr(not(unix), allow(dead_code))]
    hard_links: Mutex<HashSet<(u64, u64)>>,
    progress: &'a Progress,
}

/// Scans `root` and returns its size tree, or `None` if cancelled
pub fn scan(root: &Path, display_name: &str, progress: &Progress) -> Option<Tree> {
    let started = Instant::now();
    let walker = Walker {
        devices: devices_for(root),
        skip: skipped_paths(root),
        hard_links: Mutex::default(),
        progress,
    };
    let dir = walker.walk(root);
    if progress.is_cancelled() {
        return None;
    }
    let own = fs::symlink_metadata(root).map(|m| allocated(&m)).unwrap_or(0);
    let mut tree = flatten(dir, display_name, own, root.to_path_buf());
    tree.duration = started.elapsed();
    Some(tree)
}

impl Walker<'_> {
    fn walk(&self, dir: &Path) -> Dir {
        let mut out = Dir::default();
        let Ok(read_dir) = fs::read_dir(dir) else {
            return out;
        };
        if let Ok(mut current) = self.progress.current.try_lock() {
            use std::fmt::Write;
            current.clear();
            let _ = write!(current, "{}", dir.display());
        }

        // Subfolders to walk next: where each sits in `children`, its path, its own size
        let mut subdirs = Vec::new();
        let (mut files, mut bytes) = (0, 0);
        for item in read_dir.flatten() {
            if self.progress.is_cancelled() {
                return Dir::default();
            }
            // On Unix this is an lstat, so symlinks are never followed
            let Ok(meta) = item.metadata() else { continue };
            let size = if meta.file_type().is_dir() {
                let path = item.path();
                if !self.same_device(&meta) || self.skip.iter().any(|s| s == &path) {
                    continue;
                }
                subdirs.push((out.children.len(), path, allocated(&meta)));
                0
            } else {
                files += 1;
                if self.first_link(&meta) { file_allocated(&meta, &item) } else { 0 }
            };
            bytes += size;
            out.names.push_str(&item.file_name().to_string_lossy());
            out.children.push(Child { name_end: out.names.len() as u32, modified: modified(&meta), size, dir: None });
        }
        self.progress.files.fetch_add(files, Ordering::Relaxed);
        self.progress.bytes.fetch_add(bytes, Ordering::Relaxed);

        let walked: Vec<(usize, u64, Dir)> = subdirs
            .into_par_iter()
            .map(|(ix, path, own)| {
                let dir = self.walk(&path);
                self.progress.bytes.fetch_add(own, Ordering::Relaxed);
                (ix, own, dir)
            })
            .collect();
        for (ix, own, dir) in walked {
            let child = &mut out.children[ix];
            child.size = own + dir.size;
            if dir.modified != 0 {
                child.modified = dir.modified;
            }
            child.dir = Some(Box::new(dir));
        }
        out.total_up();
        // Growing buffers leave up to half their room empty; hand it back before the
        // whole walk is held in memory at once
        out.names.shrink_to_fit();
        out.children.shrink_to_fit();
        out
    }

    #[cfg(unix)]
    fn same_device(&self, meta: &fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        self.devices.is_empty() || self.devices.contains(&meta.dev())
    }

    #[cfg(not(unix))]
    fn same_device(&self, _meta: &fs::Metadata) -> bool {
        true
    }

    /// Hard-linked files are counted once, at the first path we meet them
    #[cfg(unix)]
    fn first_link(&self, meta: &fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() <= 1 {
            return true;
        }
        self.hard_links
            .lock()
            .unwrap()
            .insert((meta.dev(), meta.ino()))
    }

    #[cfg(not(unix))]
    fn first_link(&self, _meta: &fs::Metadata) -> bool {
        true
    }
}

impl Dir {
    /// Sums the children's sizes and file counts and finds their newest change
    fn total_up(&mut self) {
        self.size = self.children.iter().map(|c| c.size).sum();
        self.items = self.children.iter().map(Child::items).sum();
        self.modified = self.children.iter().map(|c| c.modified).max().unwrap_or(0);
    }
}

/// When a file last changed, in whole seconds since 1970 (0 if unknown or earlier).
/// 32 bits last until 2106
fn modified(meta: &fs::Metadata) -> u32 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
}

/// Bytes actually allocated on disk, so sparse and compressed files count correctly
#[cfg(unix)]
fn allocated(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn allocated(meta: &fs::Metadata) -> u64 {
    meta.len()
}

#[cfg(not(windows))]
fn file_allocated(meta: &fs::Metadata, _item: &fs::DirEntry) -> u64 {
    allocated(meta)
}

/// A file's size on disk. Most files take their length; cloud placeholders (OneDrive
/// files kept online only), compressed and sparse files ask NTFS what they really use
#[cfg(windows)]
fn file_allocated(meta: &fs::Metadata, item: &fs::DirEntry) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
        FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_SPARSE_FILE, GetCompressedFileSizeW,
        INVALID_FILE_SIZE,
    };
    const SPECIAL: u32 = FILE_ATTRIBUTE_COMPRESSED
        | FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_SPARSE_FILE;
    if meta.file_attributes() & SPECIAL == 0 {
        return meta.len();
    }
    let path: Vec<u16> = item.path().as_os_str().encode_wide().chain(Some(0)).collect();
    let mut high = 0u32;
    let low = unsafe { GetCompressedFileSizeW(path.as_ptr(), &mut high) };
    if low == INVALID_FILE_SIZE && std::io::Error::last_os_error().raw_os_error() != Some(0) {
        return meta.len();
    }
    (u64::from(high) << 32) | u64::from(low)
}

/// Devices the scan may enter. On macOS the startup disk is split into a read-only
/// system volume and a data volume joined by firmlinks; both belong to one scan
#[cfg(unix)]
fn devices_for(root: &Path) -> Vec<u64> {
    use std::os::unix::fs::MetadataExt;
    let dev = |p: &str| fs::metadata(p).map(|m| m.dev()).ok();
    let Some(root_dev) = fs::metadata(root).map(|m| m.dev()).ok() else {
        return Vec::new();
    };
    let mut devices = vec![root_dev];
    if cfg!(target_os = "macos") {
        let system = [dev("/"), dev("/System/Volumes/Data")];
        if system.contains(&Some(root_dev)) {
            devices.extend(system.into_iter().flatten());
        }
    }
    devices
}

#[cfg(not(unix))]
fn devices_for(_root: &Path) -> Vec<u64> {
    Vec::new()
}

fn skipped_paths(root: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") && root == Path::new("/") {
        // `/System/Volumes/Data` is reached through firmlinks; walking it directly
        // would count everything twice
        ["/System/Volumes", "/Volumes", "/dev", "/Network"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else if cfg!(target_os = "linux") && root == Path::new("/") {
        ["/proc", "/sys", "/dev", "/run"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        Vec::new()
    }
}

/// A folder listed again after something inside it changed, for `Tree::apply`
pub struct Relisted {
    id: NodeId,
    entries: Vec<Entry>,
}

struct Entry {
    name: String,
    size: u64,
    modified: u32,
    kind: EntryKind,
}

enum EntryKind {
    /// `linked` when other paths share the file, which the scan counts only once
    File { linked: bool },
    /// A subfolder the tree already has, which keeps what it holds
    Known,
    /// A subfolder new since the scan, walked whole
    New(Box<Dir>),
}

/// Lists folder `id` at `path` again, walking subfolders the tree doesn't have yet,
/// with the same rules as the scan of `root`. `None` when it can't be read
pub fn relist(root: &Path, path: &Path, id: NodeId, known: &HashSet<String>) -> Option<Relisted> {
    let progress = Progress::default();
    let walker = Walker {
        devices: devices_for(root),
        skip: skipped_paths(root),
        hard_links: Mutex::default(),
        progress: &progress,
    };
    let mut entries = Vec::new();
    for item in fs::read_dir(path).ok()?.flatten() {
        let Ok(meta) = item.metadata() else { continue };
        let name = item.file_name().to_string_lossy().to_string();
        let mut modified = modified(&meta);
        let (kind, size) = if meta.file_type().is_dir() {
            let sub = item.path();
            if !walker.same_device(&meta) || walker.skip.iter().any(|s| s == &sub) {
                continue;
            }
            if known.contains(&name) {
                (EntryKind::Known, 0)
            } else {
                let dir = walker.walk(&sub);
                if dir.modified != 0 {
                    modified = dir.modified;
                }
                let size = allocated(&meta) + dir.size;
                (EntryKind::New(Box::new(dir)), size)
            }
        } else {
            (EntryKind::File { linked: hard_linked(&meta) }, file_allocated(&meta, &item))
        };
        entries.push(Entry { name, size, modified, kind });
    }
    Some(Relisted { id, entries })
}

#[cfg(unix)]
fn hard_linked(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() > 1
}

/// The walk on Windows counts hard links at every path, so a listing does too
#[cfg(not(unix))]
fn hard_linked(_meta: &fs::Metadata) -> bool {
    false
}

/// Adds a walked folder's contents under node `id`, its children as one run of `kids`
fn place(dir: Dir, id: NodeId, tree: &mut Tree) {
    let Dir { names, children, .. } = dir;
    let mut start = 0;
    let mut children: Vec<(u32, Child)> = children
        .into_iter()
        .map(|child| (std::mem::replace(&mut start, child.name_end), child))
        .collect();
    children.sort_unstable_by_key(|(_, child)| std::cmp::Reverse(child.size));

    let first_id = tree.slots.len() as NodeId;
    let slot = &mut tree.slots[id as usize];
    slot.first = tree.kids.len() as u32;
    slot.count = children.len() as u32;
    for (i, (start, child)) in children.iter().enumerate() {
        let text = &names[*start as usize..child.name_end as usize];
        tree.slots.push(Slot {
            size: child.size,
            items: child.items(),
            name: tree.names.len() as u32,
            name_len: text.len() as u16,
            parent: id,
            first: 0,
            count: 0,
            modified: child.modified,
            kind: if child.dir.is_some() { NodeKind::Dir } else { NodeKind::File },
        });
        tree.names.push_str(text);
        tree.kids.push(first_id + i as NodeId);
    }
    drop(names);
    for (i, (_, child)) in children.into_iter().enumerate() {
        if let Some(sub) = child.dir {
            place(*sub, first_id + i as NodeId, tree);
        }
    }
}

/// Lays the walk out as a `Tree`. Each folder's children get consecutive ids, so
/// its child list is one run of `kids`, and each folder's names and records are freed
/// as soon as they are copied
fn flatten(root: Dir, name: &str, own: u64, root_path: PathBuf) -> Tree {
    fn measure(dir: &Dir, nodes: &mut usize, names: &mut usize) {
        *nodes += dir.children.len();
        *names += dir.names.len();
        for child in &dir.children {
            if let Some(sub) = &child.dir {
                measure(sub, nodes, names);
            }
        }
    }

    let (mut nodes, mut names) = (1, name.len());
    measure(&root, &mut nodes, &mut names);
    let mut tree = Tree {
        slots: Vec::with_capacity(nodes),
        names: String::with_capacity(names),
        kids: Vec::with_capacity(nodes),
        root_path,
        duration: Duration::ZERO,
    };
    let mut end = name.len().min(u16::MAX as usize);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    let name = &name[..end];
    tree.slots.push(Slot {
        size: own + root.size,
        items: root.items,
        name: 0,
        name_len: name.len() as u16,
        parent: NO_PARENT,
        first: 0,
        count: 0,
        modified: root.modified,
        kind: NodeKind::Dir,
    });
    tree.names.push_str(name);
    place(root, Tree::ROOT, &mut tree);
    tree
}

/// Files an admin scan uses to talk to the app: the finished tree, live progress
/// counters, and a marker the app creates to ask the helper to stop
#[derive(Clone)]
pub struct HelperFiles {
    pub tree: PathBuf,
    progress: PathBuf,
    cancel: PathBuf,
}

impl HelperFiles {
    pub fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        Self::at(std::env::temp_dir().join(format!("burrdisk-{}-{stamp}", std::process::id())))
    }

    fn at(base: PathBuf) -> Self {
        let with = |ext: &str| base.with_extension(ext);
        Self { tree: with("tree"), progress: with("progress"), cancel: with("cancel") }
    }

    pub fn cancel(&self) {
        let _ = fs::write(&self.cancel, b"");
    }

    pub fn read_progress(&self, progress: &Progress) {
        let Ok(text) = fs::read_to_string(&self.progress) else { return };
        let mut lines = text.lines();
        let mut number = || lines.next().and_then(|l| l.parse::<u64>().ok());
        if let (Some(files), Some(bytes)) = (number(), number()) {
            progress.files.store(files, Ordering::Relaxed);
            progress.bytes.store(bytes, Ordering::Relaxed);
        }
        if let Some(current) = text.lines().nth(2)
            && let Ok(mut slot) = progress.current.try_lock()
        {
            *slot = current.to_string();
        }
    }

    fn remove(&self) {
        for path in [&self.tree, &self.progress, &self.cancel] {
            let _ = fs::remove_file(path);
        }
    }
}

/// Scans `root` as root. macOS asks for an administrator's password or Touch ID, then
/// runs this same executable in helper mode; `None` if that was refused or cancelled
#[cfg(target_os = "macos")]
pub fn scan_as_admin(root: &Path, display_name: &str, files: &HelperFiles) -> Option<Tree> {
    let exe = std::env::current_exe().ok()?;
    let base = files.tree.with_extension("");
    let status = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "do shell script (quoted form of item 1 of argv) & \" --scan-helper \" & (quoted form of item 2 of argv) & \" \" & (quoted form of item 3 of argv) & \" \" & (quoted form of item 4 of argv) with prompt \"Burrdisk wants to scan folders only an administrator can read.\" with administrator privileges",
            "-e",
            "end run",
        ])
        .arg(&exe)
        .arg(&base)
        .arg(root)
        .arg(display_name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    let tree = status
        .success()
        .then(|| fs::File::open(&files.tree).ok())
        .flatten()
        .and_then(|file| Tree::read(std::io::BufReader::new(file), root.to_path_buf()).ok());
    files.remove();
    tree
}

/// Scans `root` elevated. Windows shows its UAC prompt, then runs this same executable
/// in helper mode; `None` if that was refused or cancelled
#[cfg(windows)]
pub fn scan_as_admin(root: &Path, display_name: &str, files: &HelperFiles) -> Option<Tree> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};

    let exe = std::env::current_exe().ok()?;
    let base = files.tree.with_extension("");
    let mut params = std::ffi::OsString::from("--scan-helper");
    for arg in [base.as_os_str(), root.as_os_str(), display_name.as_ref()] {
        params.push(" ");
        params.push(quote_arg(arg));
    }
    let process = crate::platform::shell_execute("runas", &exe, Some(&params), true)?;
    let mut code = 1;
    unsafe {
        WaitForSingleObject(process, INFINITE);
        GetExitCodeProcess(process, &mut code);
        CloseHandle(process);
    }
    let tree = (code == 0)
        .then(|| fs::File::open(&files.tree).ok())
        .flatten()
        .and_then(|file| Tree::read(std::io::BufReader::new(file), root.to_path_buf()).ok());
    files.remove();
    tree
}

/// Quotes one argument the way `CommandLineToArgvW` (and Rust's `args_os`) splits them:
/// backslashes are literal except right before a quote, where they escape in pairs
#[cfg(windows)]
fn quote_arg(arg: &std::ffi::OsStr) -> std::ffi::OsString {
    let arg = arg.to_string_lossy();
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                backslashes = 0;
            }
        }
        if c != '\\' {
            out.push(c);
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out.into()
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn scan_as_admin(root: &Path, display_name: &str, _files: &HelperFiles) -> Option<Tree> {
    scan(root, display_name, &Progress::default())
}

/// What the elevated helper runs. On Windows a whole volume is read straight from its
/// file table when that works, far faster than listing every folder; otherwise, and
/// on other systems, it walks the folders
fn scan_elevated(root: &Path, name: &str, progress: &Progress) -> Option<Tree> {
    #[cfg(windows)]
    match mft::scan(root, name, progress) {
        Ok(tree) => return Some(tree),
        Err(mft::Failure::Cancelled) => return None,
        Err(mft::Failure::Unsupported) => {
            progress.files.store(0, Ordering::Relaxed);
            progress.bytes.store(0, Ordering::Relaxed);
        }
    }
    scan(root, name, progress)
}

/// Entry point for `burrdisk --scan-helper <base> <root> <name>`, running as root
pub fn run_helper(args: &[std::ffi::OsString]) -> std::process::ExitCode {
    use std::process::ExitCode;
    let [base, root, name] = args else { return ExitCode::FAILURE };
    let files = HelperFiles::at(PathBuf::from(base));
    let progress = Progress::default();
    let done = AtomicBool::new(false);
    let tree = std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if files.cancel.exists() {
                    progress.cancel();
                }
                let current = progress.current.lock().map(|c| c.clone()).unwrap_or_default();
                let text = format!(
                    "{}\n{}\n{current}\n",
                    progress.files.load(Ordering::Relaxed),
                    progress.bytes.load(Ordering::Relaxed)
                );
                // Write then rename, so the app never reads half a file
                let partial = files.progress.with_extension("progress-new");
                if fs::write(&partial, text).is_ok() {
                    let _ = fs::rename(&partial, &files.progress);
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        let tree = scan_elevated(Path::new(root), &name.to_string_lossy(), &progress);
        done.store(true, Ordering::Relaxed);
        tree
    });
    let Some(tree) = tree else { return ExitCode::FAILURE };
    let written = fs::File::create(&files.tree).and_then(|file| {
        let mut out = std::io::BufWriter::new(file);
        tree.write(&mut out)?;
        std::io::Write::flush(&mut out)
    });
    if written.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

const TREE_MAGIC: &[u8; 4] = b"BDT3";

impl Tree {
    /// Serializes the tree for handing it from the admin helper to the app
    pub fn write(&self, out: &mut impl std::io::Write) -> std::io::Result<()> {
        out.write_all(TREE_MAGIC)?;
        out.write_all(&(self.duration.as_millis() as u64).to_le_bytes())?;
        out.write_all(&(self.slots.len() as u32).to_le_bytes())?;
        for id in 0..self.slots.len() as NodeId {
            let node = self.node(id);
            let kind = match node.kind {
                NodeKind::Dir => 0u8,
                NodeKind::File => 1,
                NodeKind::Hidden => 2,
            };
            out.write_all(&[kind])?;
            out.write_all(&node.size.to_le_bytes())?;
            out.write_all(&node.items.to_le_bytes())?;
            out.write_all(&node.modified.to_le_bytes())?;
            out.write_all(&node.parent.unwrap_or(NO_PARENT).to_le_bytes())?;
            out.write_all(&(node.name.len() as u32).to_le_bytes())?;
            out.write_all(node.name.as_bytes())?;
        }
        Ok(())
    }

    /// Reads a tree written by [`Tree::write`]
    pub fn read(mut input: impl std::io::Read, root_path: PathBuf) -> std::io::Result<Tree> {
        use std::io::{Error, ErrorKind};
        let mut u32_buf = [0u8; 4];
        let mut u64_buf = [0u8; 8];
        let mut magic = [0u8; 4];
        input.read_exact(&mut magic)?;
        if &magic != TREE_MAGIC {
            return Err(Error::new(ErrorKind::InvalidData, "not a Burrdisk tree"));
        }
        input.read_exact(&mut u64_buf)?;
        let duration = Duration::from_millis(u64::from_le_bytes(u64_buf));
        input.read_exact(&mut u32_buf)?;
        let count = u32::from_le_bytes(u32_buf) as usize;
        if count == 0 {
            return Err(Error::new(ErrorKind::InvalidData, "empty tree"));
        }
        let mut slots: Vec<Slot> = Vec::with_capacity(count);
        let mut names = String::new();
        let mut name = Vec::new();
        for id in 0..count {
            let mut kind = [0u8; 1];
            input.read_exact(&mut kind)?;
            input.read_exact(&mut u64_buf)?;
            let size = u64::from_le_bytes(u64_buf);
            input.read_exact(&mut u64_buf)?;
            let items = u64::from_le_bytes(u64_buf);
            input.read_exact(&mut u32_buf)?;
            let modified = u32::from_le_bytes(u32_buf);
            input.read_exact(&mut u32_buf)?;
            let parent = u32::from_le_bytes(u32_buf);
            if parent != NO_PARENT && parent as usize >= id {
                return Err(Error::new(ErrorKind::InvalidData, "parent after child"));
            }
            input.read_exact(&mut u32_buf)?;
            let len = u32::from_le_bytes(u32_buf) as usize;
            // Invalid UTF-8 can grow threefold when replaced, and must still fit 16 bits
            if len > u16::MAX as usize / 3 {
                return Err(Error::new(ErrorKind::InvalidData, "name too long"));
            }
            name.resize(len, 0);
            input.read_exact(&mut name)?;
            let start = names.len();
            names.push_str(&String::from_utf8_lossy(&name));
            slots.push(Slot {
                size,
                items,
                name: start as u32,
                name_len: (names.len() - start) as u16,
                parent,
                first: 0,
                count: 0,
                modified,
                kind: match kind[0] {
                    0 => NodeKind::Dir,
                    1 => NodeKind::File,
                    _ => NodeKind::Hidden,
                },
            });
        }
        // Give each folder a run of `kids` sized to its children, then fill the runs
        // in id order
        for id in 1..count {
            let parent = slots[id].parent;
            if parent != NO_PARENT {
                slots[parent as usize].count += 1;
            }
        }
        let mut next = 0;
        for slot in &mut slots {
            slot.first = next;
            next += slot.count;
            slot.count = 0;
        }
        let mut kids = vec![0; next as usize];
        for id in 1..count {
            let parent = slots[id].parent;
            if parent != NO_PARENT {
                let slot = &mut slots[parent as usize];
                kids[(slot.first + slot.count) as usize] = id as NodeId;
                slot.count += 1;
            }
        }
        let mut tree = Tree { slots, names, kids, root_path, duration };
        for id in 0..count as NodeId {
            tree.sort_children(id);
        }
        Ok(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn quotes_helper_args() {
        let quote = |s: &str| quote_arg(s.as_ref()).into_string().unwrap();
        assert_eq!(quote(r"C:\"), r#""C:\\""#);
        assert_eq!(quote(r"C:\Program Files\x"), r#""C:\Program Files\x""#);
        assert_eq!(quote(r#"say "hi"\"#), r#""say \"hi\"\\""#);
        assert_eq!(quote(r#"a\"b"#), r#""a\\\"b""#);
    }

    #[test]
    fn scans_and_removes() {
        let dir = std::env::temp_dir().join(format!("burrdisk-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/big"), vec![1u8; 200_000]).unwrap();
        fs::write(dir.join("a/small"), vec![1u8; 10]).unwrap();
        fs::write(dir.join("top"), vec![1u8; 50_000]).unwrap();

        let progress = Progress::default();
        let mut tree = scan(&dir, "test", &progress).unwrap();
        // A node stays 40 bytes however many files a scan holds
        assert_eq!(std::mem::size_of::<Slot>(), 40);
        let root = tree.node(Tree::ROOT);
        assert_eq!(root.children.len(), 2);
        // Folders carry the newest change inside them
        let now = crate::format::unix_now();
        assert!(root.modified > now - 3600 && root.modified <= now + 60);
        let newest = root.children.iter().map(|c| tree.node(*c).modified).max().unwrap();
        assert_eq!(root.modified, newest);
        let a = root.children[0];
        assert_eq!(&*tree.node(a).name, "a");
        assert!(tree.node(a).size >= 200_000);
        assert_eq!(tree.path(a), dir.join("a"));

        let b = tree.node(a).children[0];
        let before = tree.node(Tree::ROOT).size;
        let b_size = tree.node(b).size;
        tree.remove(b);
        assert_eq!(tree.node(Tree::ROOT).size, before - b_size);
        assert!(tree.is_ancestor(Tree::ROOT, a));

        let mut bytes = Vec::new();
        tree.write(&mut bytes).unwrap();
        let read = Tree::read(&bytes[..], dir.clone()).unwrap();
        assert_eq!(read.node(Tree::ROOT).size, tree.node(Tree::ROOT).size);
        assert_eq!(read.node(Tree::ROOT).children, tree.node(Tree::ROOT).children);
        assert_eq!(read.node(a).modified, tree.node(a).modified);
        assert_eq!(read.path(a), dir.join("a"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn catches_up_with_changes() {
        let dir = std::env::temp_dir().join(format!("burrdisk-relist-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/big"), vec![1u8; 200_000]).unwrap();
        fs::write(dir.join("a/gone"), vec![1u8; 30_000]).unwrap();
        fs::write(dir.join("top"), vec![1u8; 50_000]).unwrap();
        let mut tree = scan(&dir, "test", &Progress::default()).unwrap();
        let a = tree.deepest(&dir.join("a/new/x")).unwrap();
        let b = tree.deepest(&dir.join("a/b")).unwrap();
        assert_eq!(tree.path(a), dir.join("a"));

        fs::write(dir.join("a/b/big"), vec![1u8; 400_000]).unwrap();
        fs::remove_file(dir.join("a/gone")).unwrap();
        fs::create_dir_all(dir.join("a/new/deeper")).unwrap();
        fs::write(dir.join("a/new/deeper/file"), vec![1u8; 70_000]).unwrap();
        for id in [b, a] {
            let relisted = relist(&dir, &tree.path(id), id, &tree.subfolders(id)).unwrap();
            assert!(tree.apply(relisted));
        }

        let fresh = scan(&dir, "test", &Progress::default()).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let sizes = |t: &Tree, id: NodeId| {
            let mut out: Vec<(String, u64, u64)> =
                t.node(id).children.iter().map(|c| (t.node(*c).name.to_string(), t.node(*c).size, t.node(*c).items)).collect();
            out.sort();
            out
        };
        assert_eq!(tree.node(Tree::ROOT).size, fresh.node(Tree::ROOT).size);
        assert_eq!(tree.node(Tree::ROOT).items, fresh.node(Tree::ROOT).items);
        assert_eq!(sizes(&tree, a), sizes(&fresh, fresh.node(Tree::ROOT).children[0]));
        // What was already there keeps its id, and the new folder is walked whole
        assert_eq!(tree.deepest(&dir.join("a/b")), Some(b));
        let deeper = tree.deepest(&dir.join("a/new/deeper")).unwrap();
        assert_eq!(tree.path(deeper), dir.join("a/new/deeper"));
        assert!(tree.node(deeper).size >= 70_000);
        // Nothing changed, so nothing to do
        let relisted = relist(&dir, &dir, Tree::ROOT, &tree.subfolders(Tree::ROOT));
        assert!(relisted.is_none());
    }
}
