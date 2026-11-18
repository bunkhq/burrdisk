//! Burrdisk without its window, for scripts and terminals:
//!
//! ```text
//! burrdisk --list <path> [--find <query>] [--limit <n>] [--csv]
//! ```
//!
//! Scans the path and prints what's inside it, largest first, or with `--find`, what
//! a search for the query finds there, in the explorer's search syntax

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use crate::app::display_path;
use crate::scan::{self, NodeId, Tree};
use crate::search::{self, Compared, Query};

const USAGE: &str = "Usage: burrdisk --list <path> [--find <query>] [--limit <n>] [--csv]

Scans <path> and prints what's inside it, largest first.

  --find <query>  Print what a search finds instead, e.g. \".mov >1gb\", \"is:safe\",
                  \"is:dup\", \"is:grown\" or \"is:synced\"
  --limit <n>     Print at most <n> items (50 by default, all with --csv)
  --csv           Print CSV: path, kind, size in bytes, files, last changed

burrdisk <path>   Opens Burrdisk and scans <path>";

struct Options {
    path: PathBuf,
    find: Option<String>,
    limit: Option<usize>,
    csv: bool,
}

/// Runs the command line when the arguments ask for it; `None` opens the window
pub fn run(args: &[OsString]) -> Option<ExitCode> {
    let first = args.first()?.to_str()?;
    if !matches!(first, "--list" | "--help" | "-h") {
        return None;
    }
    attach_console();
    if first != "--list" {
        println!("{USAGE}");
        return Some(ExitCode::SUCCESS);
    }
    let options = match parse(&args[1..]) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return Some(ExitCode::from(2));
        }
    };
    Some(list(options))
}

fn parse(args: &[OsString]) -> Result<Options, String> {
    let mut path = None;
    let mut find = None;
    let mut limit = None;
    let mut csv = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next().and_then(|v| v.to_str()).map(str::to_string).ok_or(format!("{name} needs a value"))
        };
        match arg.to_str() {
            Some("--find") => find = Some(value("--find")?),
            Some("--limit") => {
                limit = Some(value("--limit")?.parse().map_err(|_| "--limit needs a number".to_string())?)
            }
            Some("--csv") => csv = true,
            Some(flag) if flag.starts_with("--") => return Err(format!("Unknown option {flag}")),
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err("Give one path".into()),
        }
    }
    Ok(Options { path: path.ok_or("Give a path to scan")?, find, limit, csv })
}

fn list(options: Options) -> ExitCode {
    let path = options.path.canonicalize().map(|p| PathBuf::from(display_path(&p))).unwrap_or(options.path);
    if !path.is_dir() {
        eprintln!("{} isn't a folder", path.display());
        return ExitCode::FAILURE;
    }
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().to_string());
    let Some(tree) = scan::scan(&path, &name, &scan::Progress::default()) else {
        eprintln!("Couldn't scan {}", path.display());
        return ExitCode::FAILURE;
    };
    let now = crate::format::unix_now();

    let results = options.find.as_deref().map(|text| {
        let query = Query::parse(text);
        let dupes = query
            .dupes
            .then(|| crate::dupes::find(crate::dupes::candidates(&tree), &crate::dupes::Progress::default()))
            .flatten()
            .map(Rc::new);
        let growth = query
            .grown
            .then(|| crate::history::previous(&path, now))
            .flatten()
            .map(|past| Rc::new(crate::history::Growth::compare(&tree, &past)));
        if query.grown && growth.is_none() {
            eprintln!("No earlier scan of {} to compare with yet", path.display());
        }
        let clouds = crate::cloud::in_tree(&tree);
        let ignored = crate::ignore::in_tree(&tree, &crate::ignore::load());
        let compared = Compared { dupes: dupes.as_ref(), growth: growth.as_ref(), clouds: &clouds, ignored: &ignored };
        search::run(&tree, Tree::ROOT, &query, now, compared)
    });
    let ids: &[NodeId] = match &results {
        Some(results) => &results.matches,
        None => tree.node(Tree::ROOT).children,
    };
    let limit = options.limit.unwrap_or(if options.csv { usize::MAX } else { 50 });
    let ids = &ids[..ids.len().min(limit)];

    let mut out = std::io::stdout().lock();
    let written = if options.csv {
        out.write_all(crate::export::csv(&tree, ids, results.as_ref()).as_bytes())
    } else {
        let growth = results.as_ref().and_then(|r| r.growth.clone());
        let text: String = ids
            .iter()
            .map(|&id| {
                let node = tree.node(id);
                let figure = match growth.as_ref().and_then(|g| g.grew(id)) {
                    Some(grew) => format!("+{}", crate::format::bytes(grew)),
                    None => crate::format::bytes(node.size),
                };
                let place = tree.path(id);
                let place = place.strip_prefix(&path).unwrap_or(&place);
                let shown = if place.as_os_str().is_empty() { node.name.into() } else { place.display().to_string() };
                format!("{figure:>10}  {shown}\n")
            })
            .collect();
        out.write_all(text.as_bytes())
    };
    // A closed pipe, as when piping into `head`, isn't a failure
    match written {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => ExitCode::FAILURE,
        _ => ExitCode::SUCCESS,
    }
}

/// Release builds on Windows are windowed apps, which start without a console; this
/// borrows the terminal's, so printing reaches it
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(not(windows))]
fn attach_console() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>();
        let o = parse(&args(&["C:\\", "--find", ".mov >1gb", "--csv", "--limit", "5"])).unwrap();
        assert_eq!(o.path, PathBuf::from("C:\\"));
        assert_eq!(o.find.as_deref(), Some(".mov >1gb"));
        assert_eq!(o.limit, Some(5));
        assert!(o.csv);
        assert!(parse(&args(&["--csv"])).is_err());
        assert!(parse(&args(&["a", "--nope"])).is_err());
        assert!(parse(&args(&["a", "--limit", "x"])).is_err());
    }
}
