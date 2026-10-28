use std::collections::HashSet;
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
    name_len: u32,
    parent: u32,
    /// The node's children are `kids[first..first + count]`
    first: u32,
    count: u32,
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
            parent: (slot.parent != NO_PARENT).then_some(slot.parent),
            children: &self.kids[first..first + slot.count as usize],
        }
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
    pub fn is_attached(&self, id: NodeId) -> bool {
        let lineage = self.lineage(id);
        lineage[0] == Self::ROOT && lineage.windows(2).all(|w| self.node(w[0]).children.contains(&w[1]))
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

    /// Adds a pseudo-entry for space the scan could not see
    pub fn add_hidden_space(&mut self, size: u64) {
        const NAME: &str = "Hidden space";
        let id = self.slots.len() as NodeId;
        self.slots.push(Slot {
            size,
            items: 0,
            name: self.names.len() as u32,
            name_len: NAME.len() as u32,
            parent: Self::ROOT,
            first: 0,
            count: 0,
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
}

struct Child {
    /// Where this child's name ends in its folder's `names`. It starts where the
    /// previous child's ends
    name_end: u32,
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
            out.children.push(Child { name_end: out.names.len() as u32, size, dir: None });
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
            child.dir = Some(Box::new(dir));
        }
        out.size = out.children.iter().map(|c| c.size).sum();
        out.items = out.children.iter().map(Child::items).sum();
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
                name_len: text.len() as u32,
                parent: id,
                first: 0,
                count: 0,
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

    let (mut nodes, mut names) = (1, name.len());
    measure(&root, &mut nodes, &mut names);
    let mut tree = Tree {
        slots: Vec::with_capacity(nodes),
        names: String::with_capacity(names),
        kids: Vec::with_capacity(nodes),
        root_path,
        duration: Duration::ZERO,
    };
    tree.slots.push(Slot {
        size: own + root.size,
        items: root.items,
        name: 0,
        name_len: name.len() as u32,
        parent: NO_PARENT,
        first: 0,
        count: 0,
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

const TREE_MAGIC: &[u8; 4] = b"BDT2";

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
            let parent = u32::from_le_bytes(u32_buf);
            if parent != NO_PARENT && parent as usize >= id {
                return Err(Error::new(ErrorKind::InvalidData, "parent after child"));
            }
            input.read_exact(&mut u32_buf)?;
            name.resize(u32::from_le_bytes(u32_buf) as usize, 0);
            input.read_exact(&mut name)?;
            let start = names.len();
            names.push_str(&String::from_utf8_lossy(&name));
            slots.push(Slot {
                size,
                items,
                name: start as u32,
                name_len: (names.len() - start) as u32,
                parent,
                first: 0,
                count: 0,
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
