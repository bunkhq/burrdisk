//! Plain-language notes on well-known space hogs: what an item is, whether it's
//! safe to move to the Trash, and the right way to clear it when it isn't

use std::path::{Component, Path};

use crate::scan::{NodeId, NodeKind, Tree};

/// What to do with an item
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing is lost that can't come back by itself
    Safe,
    /// Might hold something worth keeping; look first
    Review,
    /// Trashing it won't work or won't help; clear it the way it says instead
    Elsewhere,
    /// The system needs it. The Collector won't take it
    Keep,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Safe => "Safe to move to the Trash",
            Verdict::Review => "Look inside before you clear it",
            Verdict::Elsewhere => "Clear it another way",
            Verdict::Keep => "Leave it where it is",
        }
    }
}

/// Something the note's button does
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Copy a command to run in a terminal
    Copy(&'static str),
    /// Open a settings page or other link
    Open(&'static str),
    /// Start a system tool
    Launch(&'static str),
    /// Scan the disk again as an administrator
    RescanAsAdmin,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Copy(_) => "Copy Command",
            Action::Open(_) => "Open Settings",
            Action::Launch(_) => "Open Tool",
            Action::RescanAsAdmin => "Rescan as Administrator",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Insight {
    /// A short name for what it is, shown after the item's name in lists
    pub tag: &'static str,
    pub about: &'static str,
    pub verdict: Verdict,
    /// How to clear it, when there's more to say than the verdict
    pub how: Option<&'static str>,
    pub action: Option<Action>,
    /// Belongs to a project, so suggestions skip it while the project is in use
    pub project: bool,
}

/// How recently a project's build output or dependencies changed for suggestions to
/// leave them be
pub const IN_USE: u32 = 14 * 86_400;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Os {
    Any,
    Mac,
    Windows,
}

impl Os {
    fn here(self) -> bool {
        match self {
            Os::Any => true,
            Os::Mac => cfg!(target_os = "macos"),
            Os::Windows => cfg!(windows),
        }
    }
}

struct Rule {
    os: Os,
    /// The end of the item's path, lowercase, closest folder last: `["library", "caches"]`
    path: &'static [&'static str],
    /// The path must be exactly this, from a disk's root
    at_root: bool,
    /// Only a folder, or only a file
    kind: Option<NodeKind>,
    /// Something that must sit next to it, lowercase, e.g. the project file that made it
    beside: Option<&'static str>,
    insight: Insight,
}

const fn rule(os: Os, path: &'static [&'static str], kind: Option<NodeKind>, insight: Insight) -> Rule {
    Rule { os, path, at_root: false, kind, beside: None, insight }
}

const DIR: Option<NodeKind> = Some(NodeKind::Dir);
const FILE: Option<NodeKind> = Some(NodeKind::File);

const fn note(tag: &'static str, about: &'static str, verdict: Verdict) -> Insight {
    Insight { tag, about, verdict, how: None, action: None, project: false }
}

const fn project(mut insight: Insight) -> Insight {
    insight.project = true;
    insight
}

const fn how(mut insight: Insight, how: &'static str) -> Insight {
    insight.how = Some(how);
    insight
}

const fn act(mut insight: Insight, action: Action) -> Insight {
    insight.action = Some(action);
    insight
}

const fn at_root(mut rule: Rule) -> Rule {
    rule.at_root = true;
    rule
}

const fn beside(mut rule: Rule, file: &'static str) -> Rule {
    rule.beside = Some(file);
    rule
}

use Verdict::*;

/// Checked in order; the first match wins, so specific paths come before general ones
const RULES: &[Rule] = &[
    // Developer folders, on every platform
    rule(Os::Any, &["node_modules"], DIR, project(how(
        note("Dependencies", "Packages installed for a JavaScript project", Safe),
        "Your package manager installs them again from the project's package.json",
    ))),
    beside(rule(Os::Any, &["target"], DIR, project(how(
        note("Build output", "What Cargo compiled for a Rust project", Safe),
        "cargo build makes it again",
    ))), "cargo.toml"),
    beside(rule(Os::Any, &[".next"], DIR, project(how(
        note("Build output", "Next.js build files and cache", Safe),
        "The next build makes it again",
    ))), "package.json"),
    rule(Os::Any, &["__pycache__"], DIR, note("Bytecode cache", "Compiled Python files. Python makes them again when it needs them", Safe)),
    rule(Os::Any, &[".gradle"], DIR, note("Build cache", "Gradle's downloads and build cache. Gradle fetches what it needs again", Safe)),
    rule(Os::Any, &[".npm", "_cacache"], DIR, act(
        note("Download cache", "Packages npm keeps so installs go faster", Safe),
        Action::Copy("npm cache clean --force"),
    )),
    rule(Os::Any, &[".cargo", "registry"], DIR, note("Download cache", "Crates Cargo downloaded. It fetches them again when a project needs them", Safe)),
    rule(Os::Any, &["steamapps", "common"], DIR, how(
        note("Game library", "Games installed by Steam", Elsewhere),
        "Uninstall games in Steam, so it knows they're gone",
    )),
    // macOS
    rule(Os::Mac, &["developer", "xcode", "deriveddata"], DIR, project(how(
        note("Xcode build data", "Indexes and build products for your Xcode projects", Safe),
        "Xcode makes it again on the next build",
    ))),
    rule(Os::Mac, &["developer", "xcode", "ios devicesupport"], DIR, how(
        note("Device support", "Debug symbols for each iOS version you've connected", Safe),
        "Xcode copies them again when you connect a device",
    )),
    rule(Os::Mac, &["developer", "xcode", "archives"], DIR, how(
        note("Xcode archives", "Builds you archived to ship", Review),
        "Keep the ones you may need to read crash reports for",
    )),
    rule(Os::Mac, &["developer", "coresimulator", "devices"], DIR, act(
        note("Simulators", "iPhone and iPad simulators and everything installed on them", Elsewhere),
        Action::Copy("xcrun simctl delete unavailable"),
    )),
    rule(Os::Mac, &["docker.raw"], FILE, how(
        note("Docker disk", "The disk Docker Desktop keeps its images, containers and volumes on", Elsewhere),
        "Remove what you don't need inside Docker, with docker system prune or Docker Desktop's Troubleshoot page",
    )),
    rule(Os::Mac, &["mobilesync", "backup"], DIR, how(
        note("Device backups", "Backups of your iPhone and iPad", Review),
        "Manage them in Finder: select the device, then Manage Backups",
    )),
    rule(Os::Mac, &["mail downloads"], DIR, note("Mail attachments", "Attachments you opened in Mail. The originals stay in your mail", Safe)),
    rule(Os::Mac, &["caches", "homebrew"], DIR, act(
        note("Download cache", "Homebrew's downloads and old versions", Safe),
        Action::Copy("brew cleanup --prune=all"),
    )),
    rule(Os::Mac, &["library", "caches"], DIR, how(
        note("Caches", "Files apps keep to start and load faster", Safe),
        "Apps make them again as they need them, so some may run slower once",
    )),
    rule(Os::Mac, &[".trash"], DIR, how(
        note("Trash", "What you've already moved to the Trash", Elsewhere),
        "Empty the Trash in Finder to free this space",
    )),
    at_root(rule(Os::Mac, &["system"], DIR, note("System", "macOS itself", Keep))),
    at_root(rule(Os::Mac, &["usr"], DIR, note("System", "Programs and libraries macOS needs", Keep))),
    at_root(rule(Os::Mac, &["private"], DIR, note("System", "macOS's own data, including virtual memory and sleep images", Keep))),
    // Windows
    at_root(rule(Os::Windows, &["hiberfil.sys"], FILE, act(
        how(
            note("Hibernation file", "Windows saves memory here to hibernate and for Fast Startup", Elsewhere),
            "Turn hibernation off to remove it. Run this as administrator",
        ),
        Action::Copy("powercfg /h off"),
    ))),
    at_root(rule(Os::Windows, &["pagefile.sys"], FILE, note("Virtual memory", "Memory Windows moves to disk when RAM runs short. Windows manages its size", Keep))),
    at_root(rule(Os::Windows, &["swapfile.sys"], FILE, note("Virtual memory", "Memory Windows uses for Store apps. Windows manages its size", Keep))),
    at_root(rule(Os::Windows, &["windows.old"], DIR, act(
        how(
            note("Previous Windows", "Your last Windows version, kept so you can go back after an upgrade", Elsewhere),
            "Remove it in Settings, under Storage, then Temporary files and Previous Windows installation(s)",
        ),
        Action::Open("ms-settings:storagesense"),
    ))),
    at_root(rule(Os::Windows, &["system volume information"], DIR, act(
        how(
            note("Restore points", "System restore points and shadow copies", Elsewhere),
            "Limit their space in System Protection: choose the drive, then Configure",
        ),
        Action::Launch("SystemPropertiesProtection.exe"),
    ))),
    at_root(rule(Os::Windows, &["$recycle.bin"], DIR, how(
        note("Recycle Bin", "What you've already moved to the Recycle Bin", Elsewhere),
        "Empty the Recycle Bin to free this space",
    ))),
    rule(Os::Windows, &["windows", "winsxs"], DIR, act(
        how(
            note("Component store", "Windows components and updates. It looks bigger than it is, because most of its files are also in System32", Keep),
            "Disk Cleanup trims it: Clean up system files, then Windows Update Cleanup",
        ),
        Action::Launch("cleanmgr.exe"),
    )),
    rule(Os::Windows, &["windows", "softwaredistribution"], DIR, act(
        how(
            note("Update downloads", "Files Windows Update downloaded", Elsewhere),
            "Disk Cleanup clears the ones already installed: Clean up system files, then Windows Update Cleanup",
        ),
        Action::Launch("cleanmgr.exe"),
    )),
    rule(Os::Windows, &["windows", "installer"], DIR, note("App installers", "Windows needs these to repair, update and uninstall your apps", Keep)),
    rule(Os::Windows, &["system32", "driverstore"], DIR, note("Drivers", "Every driver Windows can install", Keep)),
    rule(Os::Windows, &["deliveryoptimization"], DIR, act(
        how(
            note("Update sharing", "Update files Windows shares with other PCs", Elsewhere),
            "Disk Cleanup clears it: Delivery Optimization Files",
        ),
        Action::Launch("cleanmgr.exe"),
    )),
    rule(Os::Windows, &["appdata", "local", "temp"], DIR, note("Temporary files", "Files apps left behind while they worked. Files still in use can't be moved and stay put", Safe)),
    rule(Os::Windows, &["windows", "temp"], DIR, note("Temporary files", "Files Windows and installers left behind. Files still in use can't be moved and stay put", Safe)),
    rule(Os::Windows, &["crashdumps"], DIR, note("Crash reports", "Memory dumps saved when apps crashed", Safe)),
    rule(Os::Windows, &["ext4.vhdx"], FILE, how(
        note("Linux disk", "A WSL distribution's whole disk. It grows but never shrinks by itself", Elsewhere),
        "Delete files inside Linux, run wsl --shutdown, then compact it with Optimize-VHD or diskpart's compact vdisk",
    )),
    rule(Os::Windows, &["docker_data.vhdx"], FILE, how(
        note("Docker disk", "The disk Docker Desktop keeps its images, containers and volumes on", Elsewhere),
        "Remove what you don't need inside Docker (docker system prune), then use Troubleshoot to reclaim the space",
    )),
    at_root(rule(Os::Windows, &["windows"], DIR, note("System", "Windows itself", Keep))),
    at_root(rule(Os::Windows, &["program files"], DIR, act(
        how(
            note("Apps", "Where most apps are installed", Elsewhere),
            "Uninstall apps in Settings, so nothing is left half-installed",
        ),
        Action::Open("ms-settings:appsfeatures"),
    ))),
    at_root(rule(Os::Windows, &["program files (x86)"], DIR, act(
        how(
            note("Apps", "Where 32-bit apps are installed", Elsewhere),
            "Uninstall apps in Settings, so nothing is left half-installed",
        ),
        Action::Open("ms-settings:appsfeatures"),
    ))),
];

/// Space the scan couldn't see, on the startup disk
const HIDDEN: Insight = if cfg!(windows) {
    act(
        note(
            "Hidden space",
            "Space in use that the scan couldn't read: restore points, protected system folders and the disk's own file table",
            Elsewhere,
        ),
        Action::RescanAsAdmin,
    )
} else {
    act(
        note(
            "Hidden space",
            "Space in use that the scan couldn't read: Time Machine snapshots, purgeable space macOS frees by itself, and folders only an administrator can open",
            Elsewhere,
        ),
        Action::RescanAsAdmin,
    )
};

/// The lowercase folder and file names along a path, from the top
fn names(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

/// What Burrdisk knows about an item, if anything
pub fn explain(tree: &Tree, id: NodeId) -> Option<Insight> {
    let node = tree.node(id);
    if node.kind == NodeKind::Hidden {
        return Some(HIDDEN);
    }
    let path = names(&tree.path(id));
    RULES
        .iter()
        .find(|rule| {
            rule.os.here()
                && rule.kind.is_none_or(|k| k == node.kind)
                && path.len() >= rule.path.len()
                && path[path.len() - rule.path.len()..].iter().zip(rule.path).all(|(a, b)| a == b)
                && (!rule.at_root || path.len() == rule.path.len())
                && rule.beside.is_none_or(|file| {
                    node.parent.is_some_and(|p| {
                        tree.node(p).children.iter().any(|c| tree.node(*c).name.eq_ignore_ascii_case(file))
                    })
                })
        })
        .map(|rule| rule.insight)
}

/// Whether an item's name is one a rule ends with, so it may have a note. Much
/// cheaper than [`explain`], for checking every item in a scan
pub fn candidate(name: &str) -> bool {
    RULES
        .iter()
        .any(|rule| rule.os.here() && rule.path.last().is_some_and(|last| name.eq_ignore_ascii_case(last)))
}

/// Whether an item is a suggestion: safe to move to the Trash, and not part of a
/// project changed in the last two weeks
pub fn suggested(tree: &Tree, id: NodeId, now: u32) -> bool {
    candidate(tree.node(id).name)
        && explain(tree, id).is_some_and(|note| {
            note.verdict == Safe && !(note.project && now.saturating_sub(tree.node(id).modified) < IN_USE)
        })
}

/// Whether the Collector should refuse an item, because the system needs it
pub fn protected(tree: &Tree, id: NodeId) -> bool {
    explain(tree, id).is_some_and(|i| i.verdict == Keep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{self, Progress};

    #[test]
    fn explains_known_folders() {
        let dir = std::env::temp_dir().join(format!("burrdisk-insight-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("web/node_modules/left-pad")).unwrap();
        std::fs::create_dir_all(dir.join("rust/target/debug")).unwrap();
        std::fs::create_dir_all(dir.join("other/target")).unwrap();
        std::fs::write(dir.join("rust/Cargo.toml"), b"[package]").unwrap();
        std::fs::write(dir.join("web/node_modules/left-pad/index.js"), b"x").unwrap();
        let tree = scan::scan(&dir, "t", &Progress::default()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let child = |parent: NodeId, name: &str| {
            *tree.node(parent).children.iter().find(|c| tree.node(**c).name == name).unwrap()
        };
        let find = |parent: &str, name: &str| child(child(Tree::ROOT, parent), name);
        assert_eq!(explain(&tree, find("web", "node_modules")).unwrap().tag, "Dependencies");
        // A folder called target is only build output beside a Cargo.toml
        assert_eq!(explain(&tree, find("rust", "target")).unwrap().verdict, Safe);
        assert!(explain(&tree, find("other", "target")).is_none());
        assert!(!protected(&tree, find("web", "node_modules")));
    }
}
