//! The Trash (Recycle Bin on Windows) itself: how much it holds, opening it, and
//! putting back what Burrdisk moved there. Burrdisk never empties it

use std::path::PathBuf;

/// Whether what Burrdisk moved can be put back from here. macOS has no way to
/// restore from the Trash in code; Finder's Put Back does it
pub const CAN_PUT_BACK: bool = cfg!(windows);

/// Bytes and items in the Trash, or `None` when it can't be read
#[cfg(windows)]
pub fn size() -> Option<(u64, u64)> {
    use windows_sys::Win32::UI::Shell::{SHQUERYRBINFO, SHQueryRecycleBinW};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    // No drive given means every drive's bin
    let status = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    (status == 0).then(|| (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64))
}

/// The home folder's Trash, which macOS only lets apps with Full Disk Access read
#[cfg(not(windows))]
pub fn size() -> Option<(u64, u64)> {
    let trash = PathBuf::from(std::env::var_os("HOME")?).join(".Trash");
    std::fs::read_dir(&trash).ok()?;
    let tree = crate::scan::scan(&trash, "Trash", &crate::scan::Progress::default())?;
    let root = tree.node(crate::scan::Tree::ROOT);
    Some((root.size, root.items))
}

/// Opens the Trash in File Explorer or Finder
pub fn open() {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer.exe").arg("shell:RecycleBinFolder").spawn();
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        let _ = std::process::Command::new("open").arg(PathBuf::from(home).join(".Trash")).spawn();
    }
}

/// Puts back items moved to the Trash at or after `since` (seconds since 1970) from
/// these paths, the latest of each. Returns how many came back
#[cfg(windows)]
pub fn put_back(paths: &[PathBuf], since: i64) -> Result<usize, String> {
    // `trash` sets up single-threaded COM and panics on a thread that already runs
    // multithreaded COM, as the thread pool behind background tasks does. A thread
    // of its own always starts clean
    let paths = paths.to_vec();
    std::thread::spawn(move || restore(&paths, since))
        .join()
        .unwrap_or_else(|_| Err("the Recycle Bin couldn\u{2019}t be read".into()))
}

#[cfg(windows)]
fn restore(paths: &[PathBuf], since: i64) -> Result<usize, String> {
    let wanted: std::collections::HashSet<&PathBuf> = paths.iter().collect();
    let mut items: Vec<trash::TrashItem> = trash::os_limited::list()
        .map_err(|e| e.to_string())?
        .into_iter()
        // A little slack for clocks that round differently
        .filter(|item| item.time_deleted >= since - 5 && wanted.contains(&item.original_path()))
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.time_deleted));
    items.dedup_by_key(|item| item.original_path());
    let count = items.len();
    if count == 0 {
        return Err("they\u{2019}re no longer in the Recycle Bin".into());
    }
    trash::os_limited::restore_all(items).map_err(|error| match error {
        trash::Error::RestoreCollision { path, .. } => {
            format!("something new is already at {}", crate::app::display_path(&path))
        }
        other => other.to_string(),
    })?;
    Ok(count)
}

#[cfg(not(windows))]
pub fn put_back(_paths: &[PathBuf], _since: i64) -> Result<usize, String> {
    Err("use Put Back in the Trash in Finder".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "moves a file through this machine's Recycle Bin"]
    fn puts_back() {
        let dir = std::env::temp_dir().join(format!("burrdisk-putback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("keep me.txt");
        std::fs::write(&file, b"hello").unwrap();
        let since = crate::format::unix_now() as i64;
        trash::delete(&file).unwrap();
        assert!(!file.exists());
        println!("bin holds {:?}", size());
        assert_eq!(put_back(std::slice::from_ref(&file), since), Ok(1));
        assert_eq!(std::fs::read(&file).unwrap(), b"hello");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
