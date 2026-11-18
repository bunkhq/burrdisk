//! Remembering past scans, to show what grew since.
//!
//! After each scan Burrdisk saves the size of every folder of at least `MIN_SIZE`,
//! by its path inside what was scanned. A scan replaces any saved in the last
//! `FRESH` seconds, and the newest `KEEP` are kept. The explorer compares with the
//! newest one older than `FRESH`, so scanning again right after clearing space
//! still compares with the days before

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::scan::{NodeId, NodeKind, Tree};

/// Folders smaller than this aren't saved
const MIN_SIZE: u64 = 50_000_000;
/// Growth smaller than this isn't worth pointing at
pub const MIN_GROWTH: u64 = 100_000_000;
/// A child folder that holds this share of its folder's growth stands for it
const EXPLAINS: f64 = 0.8;
const FRESH: u32 = 12 * 3600;
const KEEP: usize = 12;
const HEADER: &str = "burrdisk-history 1";

/// Folder sizes from one scan, by path inside what was scanned ("" for the root,
/// names joined by '/')
pub struct Snapshot {
    pub taken: u32,
    sizes: HashMap<String, u64>,
}

impl Snapshot {
    /// Takes the folder sizes from a scan
    pub fn of(tree: &Tree, taken: u32) -> Self {
        let mut sizes = HashMap::new();
        let mut stack = vec![(Tree::ROOT, String::new())];
        while let Some((id, path)) = stack.pop() {
            let node = tree.node(id);
            sizes.insert(path.clone(), node.size);
            for &child in node.children {
                let c = tree.node(child);
                // Children are sorted largest first
                if c.size < MIN_SIZE {
                    break;
                }
                if c.kind == NodeKind::Dir && !c.name.contains('\n') {
                    stack.push((child, join(&path, c.name)));
                }
            }
        }
        Snapshot { taken, sizes }
    }

    fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        let mut text = format!("{HEADER}\t{}\n", self.taken);
        for (path, size) in &self.sizes {
            let _ = writeln!(text, "{size}\t{path}");
        }
        out.write_all(text.as_bytes())
    }

    fn read(file: &Path) -> Option<Self> {
        let mut lines = BufReader::new(std::fs::File::open(file).ok()?).lines();
        let first = lines.next()?.ok()?;
        let taken = first.strip_prefix(HEADER)?.trim().parse().ok()?;
        let mut sizes = HashMap::new();
        for line in lines {
            let line = line.ok()?;
            let (size, path) = line.split_once('\t')?;
            sizes.insert(path.to_string(), size.parse().ok()?);
        }
        Some(Snapshot { taken, sizes })
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() { name.to_string() } else { format!("{parent}/{name}") }
}

/// Where scans are remembered
fn dir() -> Option<PathBuf> {
    Some(crate::platform::data_dir()?.join("History"))
}

/// Saved scans of one folder or disk, newest first, as (taken, file)
fn saved(dir: &Path, root: &Path) -> Vec<(u32, PathBuf)> {
    let prefix = format!("{}-", key(root));
    let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let taken = name.strip_prefix(&prefix)?.strip_suffix(".tsv")?.parse().ok()?;
            Some((taken, entry.path()))
        })
        .collect();
    found.sort_by_key(|(taken, _)| std::cmp::Reverse(*taken));
    found
}

fn key(root: &Path) -> String {
    let digest = Sha256::digest(crate::app::display_path(root).to_lowercase().as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Remembers a scan, replacing any from the last `FRESH` seconds
pub fn save(root: &Path, snapshot: &Snapshot) -> std::io::Result<()> {
    let dir = dir().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{}-{}.tsv", key(root), snapshot.taken));
    let partial = file.with_extension("part");
    snapshot.write(&mut std::io::BufWriter::new(std::fs::File::create(&partial)?))?;
    std::fs::rename(&partial, &file)?;
    for (ix, (taken, old)) in saved(&dir, root).into_iter().enumerate() {
        let fresh = taken != snapshot.taken && snapshot.taken.saturating_sub(taken) < FRESH;
        if fresh || ix >= KEEP {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

/// The scan to compare with: the newest older than `FRESH`
pub fn previous(root: &Path, now: u32) -> Option<Snapshot> {
    let dir = dir()?;
    saved(&dir, root)
        .into_iter()
        .filter(|(taken, _)| now.saturating_sub(*taken) >= FRESH)
        .find_map(|(_, file)| Snapshot::read(&file))
}

/// What grew since a past scan
pub struct Growth {
    pub since: u32,
    /// How much the whole scan grew; negative when it shrank
    pub total: i64,
    /// The folders that grew by `MIN_GROWTH` or more, each as deep as still explains
    /// most of the growth, and by how much
    grew: HashMap<NodeId, u64>,
}

impl Growth {
    pub fn compare(tree: &Tree, past: &Snapshot) -> Self {
        let size = |path: &str| past.sizes.get(path).copied().unwrap_or(0);
        let mut grew = HashMap::new();
        let mut stack = vec![(Tree::ROOT, String::new())];
        while let Some((id, path)) = stack.pop() {
            for &child in tree.node(id).children {
                let c = tree.node(child);
                if c.size < MIN_GROWTH {
                    break;
                }
                if c.kind != NodeKind::Dir {
                    continue;
                }
                let child_path = join(&path, c.name);
                let delta = c.size.saturating_sub(size(&child_path));
                if delta < MIN_GROWTH {
                    continue;
                }
                // Look deeper when one folder inside holds most of the growth
                let deeper = c.children.iter().any(|&g| {
                    let g = tree.node(g);
                    g.kind == NodeKind::Dir
                        && g.size.saturating_sub(size(&join(&child_path, g.name))) as f64 >= delta as f64 * EXPLAINS
                });
                if deeper {
                    stack.push((child, child_path));
                } else {
                    grew.insert(child, delta);
                }
            }
        }
        let total = tree.node(Tree::ROOT).size as i64 - size("") as i64;
        Growth { since: past.taken, total, grew }
    }

    /// How much a folder grew, if it's one that stands out
    pub fn grew(&self, id: NodeId) -> Option<u64> {
        self.grew.get(&id).copied()
    }
}

/// How long since the past scan, as a label: "Since yesterday", "Over 3 days"
pub fn since(taken: u32, now: u32) -> String {
    let over = |n: u32, one: &str| if n == 1 { format!("Over a {one}") } else { format!("Over {n} {one}s") };
    match now.saturating_sub(taken) / 86_400 {
        0 => "Since earlier today".into(),
        1 => "Since yesterday".into(),
        n @ 2..=13 => over(n, "day"),
        n @ 14..=59 => over(n / 7, "week"),
        n => over(n / 30, "month"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{self, Progress};

    #[test]
    fn finds_what_grew() {
        let dir = std::env::temp_dir().join(format!("burrdisk-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("videos/raw")).unwrap();
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("docs/a.bin"), vec![1u8; 60_000_000]).unwrap();
        let before = scan::scan(&dir, "t", &Progress::default()).unwrap();
        let past = Snapshot::of(&before, 1_000);

        // Round trip through the file format
        let mut bytes = Vec::new();
        past.write(&mut bytes).unwrap();
        let file = dir.join("snapshot.tsv");
        std::fs::write(&file, bytes).unwrap();
        let past = Snapshot::read(&file).unwrap();
        assert_eq!(past.taken, 1_000);
        assert_eq!(past.sizes.get("docs").copied(), Some(before.node(before.node(Tree::ROOT).children[0]).size));
        std::fs::remove_file(&file).unwrap();

        std::fs::write(dir.join("videos/raw/take.mov"), vec![1u8; 150_000_000]).unwrap();
        let after = scan::scan(&dir, "t", &Progress::default()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let growth = Growth::compare(&after, &past);
        let grew: Vec<String> = growth.grew.keys().map(|id| after.node(*id).name.to_string()).collect();
        // The deepest folder that explains the growth stands for it
        assert_eq!(grew, vec!["raw"]);
        assert!(growth.total >= 150_000_000);
    }
}
