use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::scan::{NodeId, NodeKind, Tree};

/// What kind of thing an item is, judged by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Folder,
    Archive,
    Video,
    Program,
    Audio,
    Image,
    Document,
    Other,
    Hidden,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Folder => "Folders",
            Category::Archive => "Archives",
            Category::Video => "Video",
            Category::Program => "Programs",
            Category::Audio => "Audio",
            Category::Image => "Images",
            Category::Document => "Documents",
            Category::Other => "Other files",
            Category::Hidden => "Hidden space",
        }
    }
}

const VIDEO: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "mkv", "webm", "wmv", "flv", "mpg", "mpeg", "3gp", "mts", "m2ts", "vob",
    "braw", "r3d", "mxf", "fcpbundle",
];
const IMAGE: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "heic", "heif", "webp", "tif", "tiff", "bmp", "svg", "raw", "cr2", "cr3",
    "nef", "arw", "dng", "orf", "rw2", "raf", "psd", "ai", "ico", "icns", "exr", "hdr", "avif", "jxl",
    "photoslibrary",
];
const AUDIO: &[&str] = &[
    "mp3", "m4a", "m4b", "m4p", "aac", "wav", "aif", "aiff", "flac", "ogg", "opus", "wma", "caf", "mid",
    "midi", "logicx", "band",
];
const ARCHIVE: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "lz4", "lzma", "dmg", "iso", "img",
    "sparseimage", "sparsebundle", "pkg", "xip", "cab",
];
const PROGRAM: &[&str] = &[
    "app", "appex", "framework", "kext", "plugin", "bundle", "exe", "msi", "dll", "so", "dylib", "bin",
    "jar", "apk", "ipa", "deb", "rpm", "appimage", "wasm", "o", "a",
];
const DOCUMENT: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "key", "pages", "numbers", "txt", "md", "rtf",
    "odt", "ods", "odp", "epub", "csv", "tex",
];

/// The lowercased extension of a file name. Dotfiles like `.zshrc` have none.
pub fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty() && ext.len() <= 16).then(|| ext.to_ascii_lowercase())
}

fn category_of_ext(ext: &str) -> Category {
    let is = |list: &[&str]| list.contains(&ext);
    if is(VIDEO) {
        Category::Video
    } else if is(IMAGE) {
        Category::Image
    } else if is(AUDIO) {
        Category::Audio
    } else if is(ARCHIVE) {
        Category::Archive
    } else if is(PROGRAM) {
        Category::Program
    } else if is(DOCUMENT) {
        Category::Document
    } else {
        Category::Other
    }
}

/// Folders are folders unless their extension says they're an app, a library or
/// some other package Finder shows as a single item.
pub fn category(name: &str, kind: NodeKind) -> Category {
    let by_ext = extension(name).map_or(Category::Other, |ext| category_of_ext(&ext));
    match kind {
        NodeKind::Hidden => Category::Hidden,
        NodeKind::Dir if by_ext == Category::Other || by_ext == Category::Document => Category::Folder,
        _ => by_ext,
    }
}

/// Biggest files kept per extension; the rest are summed into "smaller".
const TOP_FILES: usize = 200;

/// A bucket in the file type breakdown: the whole folder, a category, or one extension.
pub struct Group {
    pub name: String,
    pub category: Category,
    pub size: u64,
    pub count: u64,
    pub parent: Option<u32>,
    /// Sub-groups, largest first: categories under the root, extensions under a category.
    pub groups: Vec<u32>,
    /// For an extension, its largest items, largest first.
    pub files: Vec<NodeId>,
    /// Where an extension sits within its category's slice, -0.5..0.5, which fans
    /// its hue a little off the category's.
    pub spread: f32,
}

/// Everything under a folder, grouped by category and then extension.
pub struct Breakdown {
    pub groups: Vec<Group>,
    /// Space the scan couldn't see, shown beside the categories.
    pub hidden: Option<NodeId>,
}

impl Breakdown {
    pub const ROOT: u32 = 0;

    pub fn group(&self, ix: u32) -> &Group {
        &self.groups[ix as usize]
    }

    pub fn new(tree: &Tree, focus: NodeId) -> Self {
        struct Acc {
            category: Category,
            size: u64,
            count: u64,
            top: BinaryHeap<Reverse<(u64, NodeId)>>,
        }
        let mut by_ext: HashMap<String, Acc> = HashMap::new();
        let mut hidden = None;
        let mut stack = vec![focus];
        while let Some(id) = stack.pop() {
            let node = tree.node(id);
            let item = id != focus
                && match node.kind {
                    NodeKind::File => true,
                    NodeKind::Hidden => {
                        hidden = Some(id);
                        continue;
                    }
                    // Apps and other packages count as one item, not their insides.
                    NodeKind::Dir => category(&node.name, node.kind) != Category::Folder,
                };
            if !item {
                stack.extend(node.children.iter().copied());
                continue;
            }
            let ext = extension(&node.name).unwrap_or_default();
            let acc = by_ext.entry(ext).or_insert_with_key(|ext| Acc {
                category: if ext.is_empty() { Category::Other } else { category_of_ext(ext) },
                size: 0,
                count: 0,
                top: BinaryHeap::new(),
            });
            acc.size += node.size;
            acc.count += 1;
            acc.top.push(Reverse((node.size, id)));
            if acc.top.len() > TOP_FILES {
                acc.top.pop();
            }
        }

        let mut groups = vec![Group {
            name: tree.node(focus).name.to_string(),
            category: Category::Folder,
            size: 0,
            count: 0,
            parent: None,
            groups: Vec::new(),
            files: Vec::new(),
            spread: 0.0,
        }];
        let mut categories: HashMap<Category, u32> = HashMap::new();
        let mut exts: Vec<(String, Acc)> = by_ext.into_iter().collect();
        exts.sort_by(|a, b| b.1.size.cmp(&a.1.size).then_with(|| a.0.cmp(&b.0)));
        for (ext, acc) in exts {
            let cat = *categories.entry(acc.category).or_insert_with(|| {
                groups.push(Group {
                    name: acc.category.label().into(),
                    category: acc.category,
                    size: 0,
                    count: 0,
                    parent: Some(Self::ROOT),
                    groups: Vec::new(),
                    files: Vec::new(),
                    spread: 0.0,
                });
                (groups.len() - 1) as u32
            });
            let mut files: Vec<(u64, NodeId)> = acc.top.into_iter().map(|Reverse(f)| f).collect();
            files.sort_by(|a, b| b.cmp(a));
            groups.push(Group {
                name: if ext.is_empty() { "No extension".into() } else { format!(".{ext}") },
                category: acc.category,
                size: acc.size,
                count: acc.count,
                parent: Some(cat),
                groups: Vec::new(),
                files: files.into_iter().map(|(_, id)| id).collect(),
                spread: 0.0,
            });
            let ix = (groups.len() - 1) as u32;
            let parent = &mut groups[cat as usize];
            parent.size += acc.size;
            parent.count += acc.count;
            parent.groups.push(ix);
        }
        let mut cats: Vec<u32> = categories.into_values().collect();
        cats.sort_by(|a, b| groups[*b as usize].size.cmp(&groups[*a as usize].size));
        for &cat in &cats {
            let total = groups[cat as usize].size.max(1) as f32;
            let mut cursor = 0u64;
            for ext in groups[cat as usize].groups.clone() {
                let size = groups[ext as usize].size;
                groups[ext as usize].spread = (cursor as f32 + size as f32 / 2.0) / total - 0.5;
                cursor += size;
            }
        }
        let (size, count) = cats.iter().fold((0, 0), |(s, n), c| {
            let g = &groups[*c as usize];
            (s + g.size, n + g.count)
        });
        let root = &mut groups[Self::ROOT as usize];
        root.size = size;
        root.count = count;
        root.groups = cats;
        Self { groups, hidden }
    }

    /// 0 for a category, 1 for an extension.
    pub fn level(&self, g: u32) -> usize {
        match self.group(g).parent {
            Some(Self::ROOT) | None => 0,
            Some(_) => 1,
        }
    }

    /// Finds a group by its category and, for an extension, its name; for carrying
    /// an open group over to a rebuilt breakdown.
    pub fn find(&self, category: Category, name: Option<&str>) -> Option<u32> {
        let cat = self.group(Self::ROOT).groups.iter().copied().find(|g| self.group(*g).category == category)?;
        match name {
            None => Some(cat),
            Some(name) => self.group(cat).groups.iter().copied().find(|g| self.group(*g).name == name),
        }
    }

    /// The group a hovered item belongs to, for previewing in the sidebar.
    pub fn group_of_file(&self, id: NodeId) -> Option<u32> {
        self.groups.iter().position(|g| g.files.contains(&id)).map(|ix| ix as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_names() {
        assert_eq!(extension("Movie.MOV").as_deref(), Some("mov"));
        assert_eq!(extension(".zshrc"), None);
        assert_eq!(extension("Makefile"), None);
        assert_eq!(category("a.mp4", NodeKind::File), Category::Video);
        assert_eq!(category("Safari.app", NodeKind::Dir), Category::Program);
        assert_eq!(category("v1.2", NodeKind::Dir), Category::Folder);
        assert_eq!(category("notes.txt", NodeKind::Dir), Category::Folder);
        assert_eq!(category("song.flac", NodeKind::File), Category::Audio);
    }
}
