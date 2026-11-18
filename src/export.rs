//! Lists of items as CSV, for the explorer's Export button and `burrdisk --list --csv`

use std::fmt::Write as _;

use crate::app::display_path;
use crate::scan::{NodeId, NodeKind, Tree};
use crate::search::Results;

/// One line per item: its full path, kind, size on disk in bytes, files inside, when
/// it last changed, and with search results for duplicates or growth, the number of
/// copies or bytes grown
pub fn csv(tree: &Tree, ids: &[NodeId], results: Option<&Results>) -> String {
    let dupes = results.and_then(|r| r.dupes.as_ref());
    let growth = results.and_then(|r| r.growth.as_ref());
    let mut out = String::from("Path,Kind,Size,Files,Last changed");
    if dupes.is_some() {
        out.push_str(",Copies");
    }
    if growth.is_some() {
        out.push_str(",Grew");
    }
    out.push_str("\r\n");
    for &id in ids {
        let node = tree.node(id);
        let kind = match node.kind {
            NodeKind::Dir => "Folder",
            NodeKind::File => "File",
            NodeKind::Hidden => "Hidden space",
        };
        let path = if node.kind == NodeKind::Hidden { node.name.to_string() } else { display_path(&tree.path(id)) };
        let changed = if node.modified == 0 { String::new() } else { crate::format::date(node.modified) };
        let _ = write!(out, "{},{kind},{},{},{changed}", field(&path), node.size, node.items);
        if let Some(dupes) = dupes {
            let _ = write!(out, ",{}", dupes.copies(tree, id).map_or(0, |c| c.len()));
        }
        if let Some(growth) = growth {
            let _ = write!(out, ",{}", growth.grew(id).unwrap_or(0));
        }
        out.push_str("\r\n");
    }
    out
}

/// Quotes a field that holds a comma, quote or line break. A leading `=`, `+`, `-`
/// or `@` gets a quote mark too, so spreadsheets don't run a file name as a formula
fn field(text: &str) -> String {
    let formula = text.starts_with(['=', '+', '-', '@']);
    if formula || text.contains([',', '"', '\n', '\r']) {
        let prefix = if formula { "'" } else { "" };
        format!("\"{prefix}{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_fields() {
        assert_eq!(field(r"C:\a\b.txt"), r"C:\a\b.txt");
        assert_eq!(field("a, \"b\""), "\"a, \"\"b\"\"\"");
        assert_eq!(field("=cmd"), "\"'=cmd\"");
    }
}
