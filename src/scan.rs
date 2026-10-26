use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;

pub type NodeId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Dir,
    File,
    /// Space the volume reports as used that the scan couldn't attribute to any file
    /// (snapshots, system files we can't read, other APFS volumes in the container).
    Hidden,
}

#[derive(Debug)]
pub struct Node {
    pub name: Box<str>,
    pub size: u64,
    pub items: u64,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    /// Sorted largest first.
    pub children: Vec<NodeId>,
}

pub struct Tree {
    nodes: Vec<Node>,
    root_path: PathBuf,
}

impl Tree {
    pub const ROOT: NodeId = 0;

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut parts = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            if current == Self::ROOT {
                break;
            }
            let node = self.node(current);
            parts.push(&*node.name);
            cursor = node.parent;
        }
        let mut path = self.root_path.clone();
        for part in parts.into_iter().rev() {
            path.push(part);
        }
        path
    }

    /// Ancestors from the root down to (and including) `id`.
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

    /// Detaches a node after its files were deleted, shrinking every ancestor.
    pub fn remove(&mut self, id: NodeId) {
        let Some(parent) = self.node(id).parent else {
            return;
        };
        let (size, items) = (self.node(id).size, self.node(id).items);
        self.nodes[parent as usize].children.retain(|c| *c != id);
        let mut cursor = Some(parent);
        while let Some(current) = cursor {
            let node = &mut self.nodes[current as usize];
            node.size = node.size.saturating_sub(size);
            node.items = node.items.saturating_sub(items);
            cursor = node.parent;
        }
        let parent_node = &self.nodes[parent as usize];
        let mut children = parent_node.children.clone();
        children.sort_by(|a, b| self.node(*b).size.cmp(&self.node(*a).size));
        self.nodes[parent as usize].children = children;
        self.nodes[id as usize].parent = None;
    }

    /// Adds a pseudo-entry for space the scan could not see.
    pub fn add_hidden_space(&mut self, size: u64) {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(Node {
            name: "Hidden space".into(),
            size,
            items: 0,
            kind: NodeKind::Hidden,
            parent: Some(Self::ROOT),
            children: Vec::new(),
        });
        let root = &mut self.nodes[Self::ROOT as usize];
        root.size += size;
        root.children.push(id);
        let mut children = self.nodes[Self::ROOT as usize].children.clone();
        children.sort_by(|a, b| self.node(*b).size.cmp(&self.node(*a).size));
        self.nodes[Self::ROOT as usize].children = children;
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

/// A subtree built during the parallel walk, flattened into a `Tree` afterwards.
struct Entry {
    name: Box<str>,
    size: u64,
    items: u64,
    is_dir: bool,
    children: Vec<Entry>,
}

struct Walker<'a> {
    devices: Vec<u64>,
    skip: Vec<PathBuf>,
    hard_links: Mutex<HashSet<(u64, u64)>>,
    progress: &'a Progress,
}

/// Scans `root` and returns its size tree, or `None` if cancelled.
pub fn scan(root: &Path, display_name: &str, progress: &Progress) -> Option<Tree> {
    let walker = Walker {
        devices: devices_for(root),
        skip: skipped_paths(root),
        hard_links: Mutex::default(),
        progress,
    };
    let children = walker.walk(root);
    if progress.is_cancelled() {
        return None;
    }
    let own = fs::symlink_metadata(root).map(|m| allocated(&m)).unwrap_or(0);
    let entry = Entry {
        name: display_name.into(),
        size: own + children.iter().map(|c| c.size).sum::<u64>(),
        items: children.iter().map(|c| c.items).sum(),
        is_dir: true,
        children,
    };
    let mut nodes = Vec::new();
    flatten(entry, None, &mut nodes);
    Some(Tree {
        nodes,
        root_path: root.to_path_buf(),
    })
}

impl Walker<'_> {
    fn walk(&self, dir: &Path) -> Vec<Entry> {
        let Ok(read_dir) = fs::read_dir(dir) else {
            return Vec::new();
        };
        if let Ok(mut current) = self.progress.current.try_lock() {
            *current = dir.display().to_string();
        }

        let mut entries = Vec::new();
        let mut subdirs = Vec::new();
        let mut bytes = 0;
        for item in read_dir.flatten() {
            if self.progress.is_cancelled() {
                return Vec::new();
            }
            // On Unix this is an lstat, so symlinks are never followed.
            let Ok(meta) = item.metadata() else { continue };
            let name: Box<str> = item.file_name().to_string_lossy().into();
            let file_type = meta.file_type();
            if file_type.is_dir() {
                let path = item.path();
                if !self.same_device(&meta) || self.skip.iter().any(|s| s == &path) {
                    continue;
                }
                subdirs.push((path, name, allocated(&meta)));
            } else {
                let size = if self.first_link(&meta) { allocated(&meta) } else { 0 };
                bytes += size;
                entries.push(Entry {
                    name,
                    size,
                    items: 1,
                    is_dir: false,
                    children: Vec::new(),
                });
            }
        }
        self.progress
            .files
            .fetch_add(entries.len() as u64, Ordering::Relaxed);
        self.progress.bytes.fetch_add(bytes, Ordering::Relaxed);

        let dirs: Vec<Entry> = subdirs
            .into_par_iter()
            .map(|(path, name, own)| {
                let children = self.walk(&path);
                self.progress.bytes.fetch_add(own, Ordering::Relaxed);
                Entry {
                    name,
                    size: own + children.iter().map(|c| c.size).sum::<u64>(),
                    items: 1 + children.iter().map(|c| c.items).sum::<u64>(),
                    is_dir: true,
                    children,
                }
            })
            .collect();
        entries.extend(dirs);
        entries
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

    /// Hard-linked files are counted once, at the first path we meet them.
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

/// Bytes actually allocated on disk, so sparse and compressed files count correctly.
#[cfg(unix)]
fn allocated(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn allocated(meta: &fs::Metadata) -> u64 {
    meta.len()
}

/// Devices the scan may enter. On macOS the startup disk is split into a read-only
/// system volume and a data volume joined by firmlinks; both belong to one scan.
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
        // would count everything twice.
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

fn flatten(entry: Entry, parent: Option<NodeId>, nodes: &mut Vec<Node>) -> NodeId {
    let id = nodes.len() as NodeId;
    nodes.push(Node {
        name: entry.name,
        size: entry.size,
        items: entry.items,
        kind: if entry.is_dir { NodeKind::Dir } else { NodeKind::File },
        parent,
        children: Vec::new(),
    });
    let mut children = entry.children;
    children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    let ids: Vec<NodeId> = children
        .into_iter()
        .map(|child| flatten(child, Some(id), nodes))
        .collect();
    nodes[id as usize].children = ids;
    id
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let root = tree.node(Tree::ROOT);
        assert_eq!(root.children.len(), 2);
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

        fs::remove_dir_all(&dir).unwrap();
    }
}
