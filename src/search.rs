//! Finding items in a scan by name, extension, size and age.
//!
//! A query is words separated by spaces, and an item must match all of them:
//! - plain words match anywhere in the name, ignoring case: `cache`, `node_modules`
//! - `.ext` or `*.ext` matches the extension: `.mp4`, `*.dmg`
//! - `>500mb`, `<1gb` (also `> 1 GB`) match by size on disk; sizes alone match files only
//! - `>1y`, `>6mo`, `<2w`, `>30d` (also `> 1 year`) match by how long ago an item last
//!   changed, a folder by the newest change inside it
//! - `is:safe` matches what's safe to move to the Trash: caches, build output,
//!   dependencies of projects not touched in two weeks
//! - `is:dup` matches files with a copy elsewhere in the scan, once their contents
//!   are compared
//! - `is:grown` matches the folders that grew most since an earlier scan
//! - `is:synced` matches files a cloud service keeps on this device, which could be
//!   online only instead
//!
//! Results are the outermost matches: a matching folder stands for everything in it

use std::collections::HashSet;
use std::rc::Rc;

use crate::dupes::Dupes;
use crate::history::Growth;

use crate::scan::{NodeId, NodeKind, Tree};

const DAY: u32 = 86_400;

#[derive(Debug, Default, PartialEq)]
pub struct Query {
    /// Lowercased words the name must contain
    words: Vec<String>,
    /// Lowercased extensions, with their dot; the name must end with one of them
    exts: Vec<String>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    /// Unchanged for at least this many seconds
    older_than: Option<u32>,
    /// Changed within this many seconds
    newer_than: Option<u32>,
    /// Only what Burrdisk suggests clearing
    safe: bool,
    /// Only files with a copy elsewhere
    pub dupes: bool,
    /// Only folders that grew since an earlier scan
    pub grown: bool,
    /// Only files kept on this device in a cloud folder
    synced: bool,
}

/// The query for suggestions
pub const SAFE: &str = "is:safe";
/// The query for duplicates
pub const DUPES: &str = "is:dup";
/// The query for what grew
pub const GROWN: &str = "is:grown";
/// The query for files that could be online only
pub const SYNCED: &str = "is:synced";
/// Queries the field shows as chips; Backspace takes each away in one press
pub const CHIPS: [&str; 4] = [SAFE, DUPES, GROWN, SYNCED];

/// What some queries need beyond the scan: copies found by comparing contents, and
/// what grew since an earlier scan
#[derive(Clone, Copy, Default)]
pub struct Compared<'a> {
    pub dupes: Option<&'a Rc<Dupes>>,
    pub growth: Option<&'a Rc<Growth>>,
    /// The cloud folders in the scan
    pub clouds: &'a [NodeId],
    /// What suggestions leave out, with everything inside
    pub ignored: &'a [NodeId],
}

enum Unit {
    Bytes(u64),
    Seconds(u32),
}

fn unit(text: &str) -> Option<Unit> {
    Some(match text {
        "b" | "byte" | "bytes" => Unit::Bytes(1),
        "k" | "kb" => Unit::Bytes(1_000),
        "m" | "mb" => Unit::Bytes(1_000_000),
        "g" | "gb" => Unit::Bytes(1_000_000_000),
        "t" | "tb" => Unit::Bytes(1_000_000_000_000),
        "d" | "day" | "days" => Unit::Seconds(DAY),
        "w" | "wk" | "week" | "weeks" => Unit::Seconds(7 * DAY),
        "mo" | "mon" | "month" | "months" => Unit::Seconds(30 * DAY),
        "y" | "yr" | "year" | "years" => Unit::Seconds(365 * DAY),
        _ => return None,
    })
}

/// Splits a filter like `>1.5gb` into its comparison, number and unit
fn filter(token: &str) -> Option<(bool, f64, Unit)> {
    let (greater, rest) = match token.as_bytes().first()? {
        b'>' => (true, &token[1..]),
        b'<' => (false, &token[1..]),
        _ => return None,
    };
    let rest = rest.trim_start_matches('=');
    let split = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
    let number: f64 = rest[..split].parse().ok()?;
    // A bare number is a size in bytes
    let unit = if split == rest.len() { Unit::Bytes(1) } else { unit(&rest[split..])? };
    Some((greater, number, unit))
}

impl Query {
    pub fn parse(text: &str) -> Self {
        let lower = text.to_lowercase();
        // Glue "> 1 gb" back into ">1gb"
        let mut tokens: Vec<String> = Vec::new();
        for word in lower.split_whitespace() {
            let glue = tokens.last().is_some_and(|last: &String| {
                let filter_head = matches!(last.as_str(), ">" | "<" | ">=" | "<=");
                let number_tail = (last.starts_with('>') || last.starts_with('<'))
                    && last.ends_with(|c: char| c.is_ascii_digit())
                    && unit(word).is_some();
                filter_head || number_tail
            });
            match tokens.last_mut() {
                Some(last) if glue => last.push_str(word),
                _ => tokens.push(word.to_string()),
            }
        }
        let mut query = Query::default();
        for token in tokens {
            if let Some((greater, number, unit)) = filter(&token) {
                match unit {
                    Unit::Bytes(scale) => {
                        let bytes = (number * scale as f64) as u64;
                        if greater { query.min_size = Some(bytes) } else { query.max_size = Some(bytes) }
                    }
                    Unit::Seconds(scale) => {
                        let secs = (number * scale as f64).min(u32::MAX as f64) as u32;
                        if greater { query.older_than = Some(secs) } else { query.newer_than = Some(secs) }
                    }
                }
            } else if token == SAFE {
                query.safe = true;
            } else if token == DUPES {
                query.dupes = true;
            } else if token == GROWN {
                query.grown = true;
            } else if token == SYNCED {
                query.synced = true;
            } else if let Some(ext) = token.strip_prefix("*.").or_else(|| token.strip_prefix('.')).filter(|e| !e.is_empty()) {
                query.exts.push(format!(".{ext}"));
            } else if token != ">" && token != "<" {
                query.words.push(token);
            }
        }
        query
    }

    /// Whether the query asks for suggestions, which leave out what's ignored
    pub fn suggests(&self) -> bool {
        self.safe || self.dupes || self.grown || self.synced
    }

    pub fn is_empty(&self) -> bool {
        *self == Query::default()
    }

    /// Whether only a size bounds the query, which then looks at files alone: every
    /// big file sits in folders at least as big
    fn files_only(&self) -> bool {
        self.words.is_empty()
            && self.exts.is_empty()
            && self.older_than.is_none()
            && self.newer_than.is_none()
            && !self.safe
            && !self.dupes
            && !self.grown
            && !self.synced
    }

    /// Whether nothing inside an item can match, as nothing inside is bigger or
    /// changed more recently than the item itself
    fn rules_out_inside(&self, node: crate::scan::Node, now: u32) -> bool {
        self.min_size.is_some_and(|min| node.size < min)
            || self.newer_than.is_some_and(|n| node.modified != 0 && now.saturating_sub(node.modified) > n)
    }

    fn matches(&self, tree: &Tree, id: NodeId, now: u32, compared: Compared) -> bool {
        let node = tree.node(id);
        if node.kind == NodeKind::Hidden || (node.kind == NodeKind::Dir && self.files_only()) {
            return false;
        }
        if self.min_size.is_some_and(|min| node.size < min) || self.max_size.is_some_and(|max| node.size > max) {
            return false;
        }
        if self.older_than.is_some() || self.newer_than.is_some() {
            if node.modified == 0 {
                return false;
            }
            let age = now.saturating_sub(node.modified);
            if self.older_than.is_some_and(|o| age < o) || self.newer_than.is_some_and(|n| age > n) {
                return false;
            }
        }
        if !self.exts.is_empty() && !self.exts.iter().any(|e| ends_with_ci(node.name, e)) {
            return false;
        }
        if !self.words.iter().all(|w| contains_ci(node.name, w)) {
            return false;
        }
        if self.dupes && !compared.dupes.is_some_and(|d| d.has_copy(tree, id)) {
            return false;
        }
        if self.grown && !compared.growth.is_some_and(|g| g.grew(id).is_some()) {
            return false;
        }
        if self.synced && !crate::cloud::synced(tree, compared.clouds, id) {
            return false;
        }
        !self.safe || crate::insight::suggested(tree, id, now)
    }
}

/// Whether `name` contains `needle`, which is already lowercase
fn contains_ci(name: &str, needle: &str) -> bool {
    if name.is_ascii() && needle.is_ascii() {
        let (hay, needle) = (name.as_bytes(), needle.as_bytes());
        return needle.is_empty() || hay.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle));
    }
    name.to_lowercase().contains(needle)
}

fn ends_with_ci(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name.is_char_boundary(name.len() - suffix.len())
        && name[name.len() - suffix.len()..].to_lowercase() == suffix
}

/// What a search found inside one folder
pub struct Results {
    /// The outermost matches, largest first
    pub matches: Vec<NodeId>,
    matched: HashSet<NodeId>,
    pub total_size: u64,
    /// Folders that hold a match, so the chart can keep the way to them visible
    pub holders: HashSet<NodeId>,
    /// The copies the matches are among, when the query asks for duplicates
    pub dupes: Option<Rc<Dupes>>,
    /// How much each match grew, when the query asks what grew. `total_size` is
    /// then the growth
    pub growth: Option<Rc<Growth>>,
}

impl Results {
    /// How much a chart slice for `id` belongs to the results: 1 if it is or sits in
    /// a match, 0.5 if it holds one, else 0
    pub fn presence(&self, tree: &Tree, id: NodeId, root: NodeId) -> f32 {
        if self.holders.contains(&id) {
            return 0.5;
        }
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            if self.matched.contains(&current) {
                return 1.0;
            }
            if current == root {
                break;
            }
            cursor = tree.node(current).parent;
        }
        0.0
    }
}


/// Finds every outermost match under `root`, not counting `root` itself. A query for
/// duplicates or growth finds nothing until `compared` holds what it needs
pub fn run(tree: &Tree, root: NodeId, query: &Query, now: u32, compared: Compared) -> Results {
    let dupes = compared.dupes.filter(|_| query.dupes);
    let growth = compared.growth.filter(|_| query.grown);
    let waiting = (query.dupes && dupes.is_none()) || (query.grown && growth.is_none());
    let skip = if query.suggests() { compared.ignored } else { &[] };
    let mut matches = Vec::new();
    if let Some(d) = dupes {
        // Only files compared can match, which are far fewer than the scan's items
        let inside =
            |id: NodeId| id != root && tree.is_ancestor(root, id) && !skip.iter().any(|s| tree.is_ancestor(*s, id));
        // Copies the system keeps, like Windows' installer cache, can't be cleared, so
        // they don't list, though they still count as the copy a listed file has
        matches.extend(d.files().filter(|id| {
            inside(*id) && query.matches(tree, *id, now, compared) && !crate::insight::protected(tree, *id)
        }));
    } else if !waiting {
        // Folders that hold a cloud folder, through which a search for synced files
        // has to pass; everything else outside one is skipped whole
        let mut toward_cloud = HashSet::new();
        if query.synced {
            for &cloud in compared.clouds {
                let mut cursor = Some(cloud);
                while let Some(id) = cursor.filter(|id| toward_cloud.insert(*id)) {
                    cursor = tree.node(id).parent;
                }
            }
        }
        let in_cloud = |id: NodeId| compared.clouds.iter().any(|c| tree.is_ancestor(*c, id));
        let mut stack: Vec<(NodeId, bool)> =
            tree.node(root).children.iter().map(|c| (*c, query.synced && in_cloud(*c))).collect();
        while let Some((id, cloud)) = stack.pop() {
            if skip.contains(&id) {
                continue;
            }
            let cloud = cloud || (query.synced && compared.clouds.contains(&id));
            if query.synced && !cloud && !toward_cloud.contains(&id) {
                continue;
            }
            if query.matches(tree, id, now, compared) {
                matches.push(id);
            } else if !query.rules_out_inside(tree.node(id), now) {
                stack.extend(tree.node(id).children.iter().map(|c| (*c, cloud)));
            }
        }
    }
    let size = |id: &NodeId| match growth {
        Some(g) => g.grew(*id).unwrap_or(0),
        None => tree.node(*id).size,
    };
    match dupes {
        // Copies of one file side by side, the likely original first
        Some(d) => matches.sort_by_key(|id| (std::cmp::Reverse(size(id)), d.order(*id))),
        None => matches.sort_by_key(|id| std::cmp::Reverse(size(id))),
    }
    let matched = matches.iter().copied().collect();
    let total_size = matches.iter().map(size).sum();
    let mut holders = HashSet::new();
    for &id in &matches {
        let mut cursor = tree.node(id).parent;
        while let Some(parent) = cursor {
            if parent == root || !holders.insert(parent) {
                break;
            }
            cursor = tree.node(parent).parent;
        }
    }
    Results { matches, matched, total_size, holders, dupes: dupes.cloned(), growth: growth.cloned() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{self, Progress};

    #[test]
    fn parses_queries() {
        let q = Query::parse("Cache  .MP4 *.mov > 1.5 GB <2w >1 year");
        assert_eq!(q.words, vec!["cache"]);
        assert_eq!(q.exts, vec![".mp4", ".mov"]);
        assert_eq!(q.min_size, Some(1_500_000_000));
        assert_eq!(q.newer_than, Some(14 * DAY));
        assert_eq!(q.older_than, Some(365 * DAY));
        assert!(Query::parse("   ").is_empty());
        assert_eq!(Query::parse(">500mb").min_size, Some(500_000_000));
        assert_eq!(Query::parse("<6mo").newer_than, Some(180 * DAY));
        // A comparison that isn't a filter is just a word
        assert_eq!(Query::parse(">abc").words, vec![">abc"]);
    }

    #[test]
    fn finds_outermost_matches() {
        let dir = std::env::temp_dir().join(format!("burrdisk-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app/node_modules/x/node_modules")).unwrap();
        std::fs::create_dir_all(dir.join("films")).unwrap();
        std::fs::write(dir.join("app/node_modules/x/node_modules/a.js"), vec![1u8; 5_000]).unwrap();
        std::fs::write(dir.join("films/Trip.MP4"), vec![1u8; 300_000]).unwrap();
        std::fs::write(dir.join("films/notes.txt"), vec![1u8; 100]).unwrap();
        let tree = scan::scan(&dir, "t", &Progress::default()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let now = crate::format::unix_now();
        let names = |r: &Results| r.matches.iter().map(|id| tree.node(*id).name.to_string()).collect::<Vec<_>>();

        // The nested node_modules folds into the outer one
        let r = run(&tree, Tree::ROOT, &Query::parse("node_modules"), now, Compared::default());
        assert_eq!(names(&r), vec!["node_modules"]);
        assert_eq!(r.holders.len(), 1);

        let r = run(&tree, Tree::ROOT, &Query::parse(".mp4"), now, Compared::default());
        assert_eq!(names(&r), vec!["Trip.MP4"]);
        assert_eq!(r.presence(&tree, r.matches[0], Tree::ROOT), 1.0);

        // A size alone finds files, not the folders around them
        let r = run(&tree, Tree::ROOT, &Query::parse(">100kb"), now, Compared::default());
        assert_eq!(names(&r), vec!["Trip.MP4"]);

        // Fresh dependencies belong to a project in use, so they aren't suggested
        assert!(run(&tree, Tree::ROOT, &Query::parse(SAFE), now, Compared::default()).matches.is_empty());
        assert_eq!(run(&tree, Tree::ROOT, &Query::parse(SAFE), now + 30 * DAY, Compared::default()).matches.len(), 1);
        // What's ignored drops out of suggestions, not out of searches
        let app = tree.node(Tree::ROOT).children.iter().copied().find(|c| tree.node(*c).name == "app").unwrap();
        let ignoring = Compared { ignored: &[app], ..Compared::default() };
        assert!(run(&tree, Tree::ROOT, &Query::parse(SAFE), now + 30 * DAY, ignoring).matches.is_empty());
        assert_eq!(run(&tree, Tree::ROOT, &Query::parse("node_modules"), now, ignoring).matches.len(), 1);

        // Everything was just written, so nothing is a year old
        assert!(run(&tree, Tree::ROOT, &Query::parse(">1y"), now, Compared::default()).matches.is_empty());
        let r = run(&tree, Tree::ROOT, &Query::parse("<1d"), now, Compared::default());
        assert_eq!(r.matches.len(), 2);
    }
}
