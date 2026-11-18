//! Folders that sync with a cloud service, and making their files online-only.
//!
//! Files there count only the space they take on this device, so a file kept online
//! only takes none. Freeing up space asks the service to keep a file online only:
//! nothing is deleted, and the file downloads again when it's opened. On Windows
//! that covers every service built on the Cloud Files API (OneDrive, Dropbox, Google
//! Drive, iCloud and others); on macOS, iCloud Drive

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::scan::{NodeId, NodeKind, Tree};

/// A folder a cloud service syncs, and the service's name
#[derive(Clone, Debug)]
pub struct Root {
    pub path: PathBuf,
    pub service: String,
}

/// The cloud folders on this computer, found once
pub fn roots() -> &'static [Root] {
    static ROOTS: OnceLock<Vec<Root>> = OnceLock::new();
    ROOTS.get_or_init(|| find().into_iter().filter(|r| r.path.is_dir()).collect())
}

/// The cloud folder that holds `path`, if any
pub fn root_of(path: &Path) -> Option<&'static Root> {
    roots().iter().find(|r| path.starts_with(&r.path))
}

/// The nodes in a scan that are cloud folders, or the scan's root when it's inside one
pub fn in_tree(tree: &Tree) -> Vec<NodeId> {
    let scanned = tree.path(Tree::ROOT);
    if root_of(&scanned).is_some() {
        return vec![Tree::ROOT];
    }
    roots()
        .iter()
        .filter_map(|r| tree.deepest(&r.path).filter(|id| tree.path(*id) == r.path))
        .collect()
}

/// Whether a file is kept on this device in a cloud folder, so it could be online only.
/// `clouds` are the cloud folders in the tree, from `in_tree`
pub fn synced(tree: &Tree, clouds: &[NodeId], id: NodeId) -> bool {
    let node = tree.node(id);
    node.kind == NodeKind::File && node.size > 0 && clouds.iter().any(|c| tree.is_ancestor(*c, id))
}

/// Whether anything inside a cloud folder could be made online only
pub fn can_free_up(tree: &Tree, clouds: &[NodeId], id: NodeId) -> bool {
    let node = tree.node(id);
    node.kind != NodeKind::Hidden && node.size > 0 && clouds.iter().any(|c| tree.is_ancestor(*c, id))
}

/// Asks the cloud service to keep a file, or everything in a folder, online only
#[cfg(windows)]
pub fn free_up(path: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Unpinned (+U) and not pinned (-P) is what "Free up space" in File Explorer sets
    let attrib = |args: &[&std::ffi::OsStr]| -> std::io::Result<()> {
        let status = std::process::Command::new("attrib").args(args).creation_flags(CREATE_NO_WINDOW).status()?;
        if status.success() { Ok(()) } else { Err(std::io::Error::other("the cloud service didn\u{2019}t take the change")) }
    };
    attrib(&["+U".as_ref(), "-P".as_ref(), path.as_os_str()])?;
    if path.is_dir() {
        let inside = path.join("*");
        attrib(&["+U".as_ref(), "-P".as_ref(), inside.as_os_str(), "/S".as_ref(), "/D".as_ref()])?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn free_up(path: &Path) -> std::io::Result<()> {
    let status = std::process::Command::new("brctl").arg("evict").arg(path).status()?;
    if status.success() { Ok(()) } else { Err(std::io::Error::other("iCloud didn\u{2019}t take the change")) }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn free_up(_path: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// Sync roots registered with the Cloud Files API, under
/// `SyncRootManager\<Service>!<SID>!<Account>\UserSyncRoots`
#[cfg(windows)]
fn find() -> Vec<Root> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ, RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW,
    };
    const BASE: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager";
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let open = |parent: HKEY, sub: &str| -> Option<HKEY> {
        let mut key = std::ptr::null_mut();
        (unsafe { RegOpenKeyExW(parent, wide(sub).as_ptr(), 0, KEY_READ, &mut key) } == 0).then_some(key)
    };
    let mut found = Vec::new();
    let Some(base) = open(HKEY_LOCAL_MACHINE, BASE) else { return found };
    let mut ix = 0;
    loop {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        let status = unsafe {
            RegEnumKeyExW(base, ix, name.as_mut_ptr(), &mut len, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
        };
        if status != 0 {
            break;
        }
        ix += 1;
        let key_name = String::from_utf16_lossy(&name[..len as usize]);
        let service = key_name.split('!').next().unwrap_or_default().to_string();
        let Some(roots) = open(base, &format!(r"{key_name}\UserSyncRoots")) else { continue };
        let mut vx = 0;
        loop {
            let mut value = [0u16; 256];
            let mut value_len = value.len() as u32;
            let mut data = [0u16; 1024];
            let mut data_len = (data.len() * 2) as u32;
            let mut kind = 0u32;
            let status = unsafe {
                RegEnumValueW(
                    roots,
                    vx,
                    value.as_mut_ptr(),
                    &mut value_len,
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr().cast(),
                    &mut data_len,
                )
            };
            if status != 0 {
                break;
            }
            vx += 1;
            if kind == REG_SZ {
                let text = String::from_utf16_lossy(&data[..(data_len as usize / 2)]);
                let path = PathBuf::from(text.trim_end_matches('\0'));
                found.push(Root { path, service: service.clone() });
            }
        }
        unsafe { RegCloseKey(roots) };
    }
    unsafe { RegCloseKey(base) };
    found
}

/// iCloud Drive, which holds the Desktop and Documents too when they sync
#[cfg(target_os = "macos")]
fn find() -> Vec<Root> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return Vec::new() };
    vec![Root { path: home.join("Library/Mobile Documents"), service: "iCloud".into() }]
}

#[cfg(not(any(windows, target_os = "macos")))]
fn find() -> Vec<Root> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "depends on the cloud services set up on this machine"]
    fn lists_roots() {
        for root in roots() {
            println!("{} {}", root.service, root.path.display());
        }
    }
}
