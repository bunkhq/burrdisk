//! Items people asked Burrdisk not to suggest again. Suggestions (`is:safe`,
//! `is:dup`, `is:grown`, `is:synced`) skip them and everything inside them; plain
//! searches and the chart still show them. Kept as one path per line in the app's
//! data folder

use std::path::{Path, PathBuf};

use crate::scan::{NodeId, Tree};

fn file() -> Option<PathBuf> {
    Some(crate::platform::data_dir()?.join("Ignored.txt"))
}

pub fn load() -> Vec<PathBuf> {
    let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else { return Vec::new() };
    text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()
}

fn save(paths: &[PathBuf]) -> std::io::Result<()> {
    let file = file().ok_or(std::io::ErrorKind::NotFound)?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
    std::fs::write(file, text)
}

/// Adds or takes away a path, and returns the list as it now stands
pub fn set(path: &Path, ignored: bool) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = load();
    paths.retain(|p| p != path);
    if ignored {
        paths.push(path.to_path_buf());
    }
    save(&paths)?;
    Ok(paths)
}

/// The ignored items in a scan
pub fn in_tree(tree: &Tree, paths: &[PathBuf]) -> Vec<NodeId> {
    paths.iter().filter_map(|p| tree.deepest(p).filter(|id| tree.path(*id) == *p).or_else(|| file_in(tree, p))).collect()
}

/// A file's node, which `deepest` stops short of as it only follows folders
fn file_in(tree: &Tree, path: &Path) -> Option<NodeId> {
    let parent = tree.deepest(path.parent()?)?;
    let name = path.file_name()?.to_string_lossy();
    tree.node(parent).children.iter().copied().find(|c| tree.node(*c).name == name && tree.path(*c) == path)
}
