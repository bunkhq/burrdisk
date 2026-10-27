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

    /// Whether a node is still reachable from the root (it isn't once removed).
    pub fn is_attached(&self, id: NodeId) -> bool {
        self.lineage(id).windows(2).all(|w| self.node(w[0]).children.contains(&w[1]))
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

/// Files an admin scan uses to talk to the app: the finished tree, live progress
/// counters, and a marker the app creates to ask the helper to stop.
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
/// runs this same executable in helper mode; `None` if that was refused or cancelled.
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

#[cfg(not(target_os = "macos"))]
pub fn scan_as_admin(root: &Path, display_name: &str, _files: &HelperFiles) -> Option<Tree> {
    scan(root, display_name, &Progress::default())
}

/// Entry point for `burrdisk --scan-helper <base> <root> <name>`, running as root.
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
                // Write then rename, so the app never reads half a file.
                let partial = files.progress.with_extension("progress-new");
                if fs::write(&partial, text).is_ok() {
                    let _ = fs::rename(&partial, &files.progress);
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        let tree = scan(Path::new(root), &name.to_string_lossy(), &progress);
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

const TREE_MAGIC: &[u8; 4] = b"BDT1";

impl Tree {
    /// Serializes the tree for handing it from the admin helper to the app.
    pub fn write(&self, out: &mut impl std::io::Write) -> std::io::Result<()> {
        out.write_all(TREE_MAGIC)?;
        out.write_all(&(self.nodes.len() as u32).to_le_bytes())?;
        for node in &self.nodes {
            let kind = match node.kind {
                NodeKind::Dir => 0u8,
                NodeKind::File => 1,
                NodeKind::Hidden => 2,
            };
            out.write_all(&[kind])?;
            out.write_all(&node.size.to_le_bytes())?;
            out.write_all(&node.items.to_le_bytes())?;
            out.write_all(&node.parent.unwrap_or(u32::MAX).to_le_bytes())?;
            out.write_all(&(node.name.len() as u32).to_le_bytes())?;
            out.write_all(node.name.as_bytes())?;
        }
        Ok(())
    }

    /// Reads a tree written by [`Tree::write`]. Children are listed in id order,
    /// which is the largest-first order they were written in.
    pub fn read(mut input: impl std::io::Read, root_path: PathBuf) -> std::io::Result<Tree> {
        use std::io::{Error, ErrorKind};
        let mut u32_buf = [0u8; 4];
        let mut u64_buf = [0u8; 8];
        let mut magic = [0u8; 4];
        input.read_exact(&mut magic)?;
        if &magic != TREE_MAGIC {
            return Err(Error::new(ErrorKind::InvalidData, "not a Burrdisk tree"));
        }
        input.read_exact(&mut u32_buf)?;
        let count = u32::from_le_bytes(u32_buf) as usize;
        let mut nodes: Vec<Node> = Vec::with_capacity(count);
        for id in 0..count {
            let mut kind = [0u8; 1];
            input.read_exact(&mut kind)?;
            input.read_exact(&mut u64_buf)?;
            let size = u64::from_le_bytes(u64_buf);
            input.read_exact(&mut u64_buf)?;
            let items = u64::from_le_bytes(u64_buf);
            input.read_exact(&mut u32_buf)?;
            let parent = Some(u32::from_le_bytes(u32_buf)).filter(|p| *p != u32::MAX);
            input.read_exact(&mut u32_buf)?;
            let mut name = vec![0u8; u32::from_le_bytes(u32_buf) as usize];
            input.read_exact(&mut name)?;
            if parent.is_some_and(|p| p as usize >= id) {
                return Err(Error::new(ErrorKind::InvalidData, "parent after child"));
            }
            if let Some(p) = parent {
                nodes[p as usize].children.push(id as NodeId);
            }
            nodes.push(Node {
                name: String::from_utf8_lossy(&name).into(),
                size,
                items,
                kind: match kind[0] {
                    0 => NodeKind::Dir,
                    1 => NodeKind::File,
                    _ => NodeKind::Hidden,
                },
                parent,
                children: Vec::new(),
            });
        }
        if nodes.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "empty tree"));
        }
        Ok(Tree { nodes, root_path })
    }
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

        let mut bytes = Vec::new();
        tree.write(&mut bytes).unwrap();
        let read = Tree::read(&bytes[..], dir.clone()).unwrap();
        assert_eq!(read.node(Tree::ROOT).size, tree.node(Tree::ROOT).size);
        assert_eq!(read.node(Tree::ROOT).children, tree.node(Tree::ROOT).children);
        assert_eq!(read.path(a), dir.join("a"));

        fs::remove_dir_all(&dir).unwrap();
    }
}
