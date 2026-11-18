//! Finding files with the same contents, for the `is:dup` search.
//!
//! Only files of at least `MIN_SIZE` are compared, as smaller ones free little. Files
//! are grouped by size, then by a hash of their first and last blocks, and only those
//! still alike are read whole, so most files are never read at all. Cloud placeholders
//! are skipped, as reading them would download them, and so are extra hard links to
//! one file, as moving a link to the Trash frees nothing. Clones on APFS share their
//! blocks too, but nothing cheap tells them apart, so they count as copies

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::scan::{NodeId, NodeKind, Tree};

pub const MIN_SIZE: u64 = 1_000_000;
/// How much of each end of a file the first pass reads
const BLOCK: u64 = 64 * 1024;

type Hash = [u8; 32];

pub struct Candidate {
    id: NodeId,
    path: PathBuf,
    /// Shallower paths and shorter names first, so a set's first member is the one
    /// most likely to be the original
    rank: (usize, usize),
}

#[derive(Default)]
pub struct Progress {
    /// Bytes read whole so far, of `total`, which is known once the first pass is done
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancelled: AtomicBool,
}

impl Progress {
    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Sets of files with the same contents
#[derive(Default)]
pub struct Dupes {
    /// Each file's set and its place in it
    set_of: HashMap<NodeId, (u32, u32)>,
    /// Each set's members, the likely original first
    sets: Vec<Vec<NodeId>>,
}

impl Dupes {
    /// The copies of the file still in the scan, itself included, or `None` when it
    /// has no copy left
    pub fn copies(&self, tree: &Tree, id: NodeId) -> Option<Vec<NodeId>> {
        if !self.has_copy(tree, id) {
            return None;
        }
        let set = &self.sets[self.set_of[&id].0 as usize];
        Some(set.iter().copied().filter(|m| tree.is_attached(*m)).collect())
    }

    /// Whether the file is still in the scan with a copy that is too
    pub fn has_copy(&self, tree: &Tree, id: NodeId) -> bool {
        let Some(&(set, _)) = self.set_of.get(&id) else { return false };
        tree.is_attached(id) && self.sets[set as usize].iter().any(|m| *m != id && tree.is_attached(*m))
    }

    /// Which set a file is in and its place there, for listing copies side by side
    pub fn order(&self, id: NodeId) -> (u32, u32) {
        self.set_of.get(&id).copied().unwrap_or((u32::MAX, 0))
    }

    /// Every file that had a copy when the comparison ran
    pub fn files(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.set_of.keys().copied()
    }
}

/// Files in the scan big enough to compare that share their size with another
pub fn candidates(tree: &Tree) -> Vec<Candidate> {
    let mut by_size: HashMap<u64, Vec<NodeId>> = HashMap::new();
    let mut stack = vec![Tree::ROOT];
    while let Some(id) = stack.pop() {
        let node = tree.node(id);
        match node.kind {
            NodeKind::File if node.size >= MIN_SIZE => by_size.entry(node.size).or_default().push(id),
            // Children are sorted largest first, so the rest are too small to matter
            NodeKind::Dir => stack.extend(node.children.iter().take_while(|c| tree.node(**c).size >= MIN_SIZE)),
            _ => {}
        }
    }
    by_size
        .into_values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .map(|id| {
            let path = tree.path(id);
            let rank = (path.components().count(), tree.node(id).name.len());
            Candidate { id, path, rank }
        })
        .collect()
}

/// Compares the candidates' contents. `None` when cancelled. Runs on a few threads
/// of its own at background priority, so the rest of the computer stays responsive
/// while it reads
pub fn find(candidates: Vec<Candidate>, progress: &Progress) -> Option<Dupes> {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get() / 4).clamp(1, 3);
    match rayon::ThreadPoolBuilder::new().num_threads(threads).start_handler(|_| background_priority()).build() {
        Ok(pool) => pool.install(|| compare(candidates, progress)),
        Err(_) => compare(candidates, progress),
    }
}

/// Lowers the thread's CPU and disk priority below what the user is doing
fn background_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
    #[cfg(target_os = "macos")]
    unsafe {
        libc::setpriority(libc::PRIO_DARWIN_THREAD, 0, libc::PRIO_DARWIN_BG);
    }
}

fn compare(candidates: Vec<Candidate>, progress: &Progress) -> Option<Dupes> {
    // First pass: length and both ends
    let sampled: Vec<(Candidate, Sample)> =
        candidates.into_par_iter().filter_map(|c| sample(&c.path, progress).map(|s| (c, s))).collect();
    if progress.cancelled() {
        return None;
    }
    let mut alike: HashMap<(u64, Hash), Vec<(Candidate, Sample)>> = HashMap::new();
    for (candidate, sample) in sampled {
        alike.entry((sample.len, sample.ends)).or_default().push((candidate, sample));
    }
    let groups: Vec<(u64, Vec<Candidate>)> =
        alike.into_iter().filter_map(|((len, _), group)| Some((len, distinct_files(group)?))).collect();

    // Second pass: whole contents, unless the ends already covered them
    let whole = |len: u64| len > 2 * BLOCK;
    let total: u64 = groups.iter().filter(|(len, _)| whole(*len)).map(|(len, g)| len * g.len() as u64).sum();
    progress.total.store(total, Ordering::Relaxed);
    let mut sets: Vec<Vec<Candidate>> = Vec::new();
    for (len, group) in groups {
        if !whole(len) {
            sets.push(group);
            continue;
        }
        let hashed: Vec<(Hash, Candidate)> =
            group.into_par_iter().filter_map(|c| hash_whole(&c.path, progress).map(|h| (h, c))).collect();
        if progress.cancelled() {
            return None;
        }
        let mut same: HashMap<Hash, Vec<Candidate>> = HashMap::new();
        for (hash, candidate) in hashed {
            same.entry(hash).or_default().push(candidate);
        }
        sets.extend(same.into_values().filter(|s| s.len() > 1));
    }

    let mut dupes = Dupes::default();
    for mut set in sets {
        set.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.path.cmp(&b.path)));
        let ix = dupes.sets.len() as u32;
        for (rank, c) in set.iter().enumerate() {
            dupes.set_of.insert(c.id, (ix, rank as u32));
        }
        dupes.sets.push(set.into_iter().map(|c| c.id).collect());
    }
    Some(dupes)
}

/// Drops extra hard links to one file, leaving the group only if two remain
fn distinct_files(group: Vec<(Candidate, Sample)>) -> Option<Vec<Candidate>> {
    let mut seen = std::collections::HashSet::new();
    let files: Vec<Candidate> =
        group.into_iter().filter(|(_, s)| s.file.is_none_or(|f| seen.insert(f))).map(|(c, _)| c).collect();
    (files.len() > 1).then_some(files)
}

struct Sample {
    len: u64,
    ends: Hash,
    /// The file's identity on its volume, which hard links share
    file: Option<(u64, u64)>,
}

fn sample(path: &std::path::Path, progress: &Progress) -> Option<Sample> {
    if progress.cancelled() {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    if in_cloud(&meta) {
        return None;
    }
    let len = meta.len();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BLOCK as usize];
    let head = read_up_to(&mut file, &mut buf)?;
    hasher.update(&buf[..head]);
    if len > BLOCK {
        file.seek(SeekFrom::Start(len.saturating_sub(BLOCK).max(BLOCK))).ok()?;
        let tail = read_up_to(&mut file, &mut buf)?;
        hasher.update(&buf[..tail]);
    }
    Some(Sample { len, ends: hasher.finalize().into(), file: file_id(&file, &meta) })
}

fn hash_whole(path: &std::path::Path, progress: &Progress) -> Option<Hash> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if progress.cancelled() {
            return None;
        }
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        hasher.update(&buf[..n]);
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    Some(hasher.finalize().into())
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> Option<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    Some(filled)
}

/// Whether the file's contents live in the cloud, so reading it would download it
#[cfg(windows)]
fn in_cloud(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    };
    meta.file_attributes() & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN)
        != 0
}

/// iCloud Drive's evicted files are "dataless"
#[cfg(target_os = "macos")]
fn in_cloud(meta: &std::fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const SF_DATALESS: u32 = 0x4000_0000;
    meta.st_flags() & SF_DATALESS != 0
}

#[cfg(not(any(windows, target_os = "macos")))]
fn in_cloud(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn file_id(_file: &File, meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.nlink() > 1).then(|| (meta.dev(), meta.ino()))
}

#[cfg(windows)]
fn file_id(file: &File, _meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    (ok != 0 && info.nNumberOfLinks > 1).then(|| {
        (u64::from(info.dwVolumeSerialNumber), (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{self, Progress as ScanProgress};

    #[test]
    fn finds_copies() {
        let dir = std::env::temp_dir().join(format!("burrdisk-dupes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/deeper")).unwrap();
        let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let mut other = big.clone();
        // Same size and same ends, different in the middle
        other[1_500_000] ^= 1;
        std::fs::write(dir.join("film.mov"), &big).unwrap();
        std::fs::write(dir.join("a/deeper/film copy.mov"), &big).unwrap();
        std::fs::write(dir.join("a/edited.mov"), &other).unwrap();
        std::fs::write(dir.join("a/small.txt"), b"tiny").unwrap();
        std::fs::write(dir.join("small copy.txt"), b"tiny").unwrap();
        let tree = scan::scan(&dir, "t", &ScanProgress::default()).unwrap();

        let found = find(candidates(&tree), &Progress::default()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found.sets.len(), 1);
        let names = |ids: Vec<NodeId>| ids.iter().map(|id| tree.node(*id).name.to_string()).collect::<Vec<_>>();
        let film = found.set_of.keys().copied().find(|id| tree.node(*id).name == "film.mov").unwrap();
        // The shallower copy comes first
        assert_eq!(names(found.copies(&tree, film).unwrap()), vec!["film.mov", "film copy.mov"]);

        // The search waits for the comparison, then lists copies side by side
        let query = crate::search::Query::parse("is:dup .mov");
        let now = crate::format::unix_now();
        let none = crate::search::Compared::default();
        assert!(crate::search::run(&tree, Tree::ROOT, &query, now, none).matches.is_empty());
        let found = std::rc::Rc::new(found);
        let compared = crate::search::Compared { dupes: Some(&found), ..none };
        let results = crate::search::run(&tree, Tree::ROOT, &query, now, compared);
        assert_eq!(names(results.matches.clone()), vec!["film.mov", "film copy.mov"]);
    }
}
