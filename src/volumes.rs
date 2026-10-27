use std::path::{Path, PathBuf};

use sysinfo::Disks;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VolumeKind {
    Startup,
    Internal,
    External,
    Network,
    DiskImage,
}

#[derive(Clone, Debug)]
pub struct Volume {
    pub name: String,
    pub mount: PathBuf,
    pub kind: VolumeKind,
    pub file_system: String,
    pub total: u64,
    /// Space the OS can hand out right now, including purgeable space.
    pub available: u64,
    /// Strictly free blocks, excluding purgeable space.
    pub free: u64,
}

impl Volume {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.free)
    }

    pub fn purgeable(&self) -> u64 {
        self.available.saturating_sub(self.free)
    }

    pub fn fill_ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.total.saturating_sub(self.available) as f32 / self.total as f32
        }
    }

}

pub fn list() -> Vec<Volume> {
    let images = disk_image_mounts();
    let disks = Disks::new_with_refreshed_list();
    let mut volumes: Vec<Volume> = disks
        .list()
        .iter()
        .filter_map(|disk| {
            let mount = disk.mount_point().to_path_buf();
            if !is_user_facing(&mount) || disk.total_space() == 0 {
                return None;
            }
            let file_system = disk.file_system().to_string_lossy().to_string();
            let kind = if is_startup(&mount) {
                VolumeKind::Startup
            } else if images.iter().any(|m| m == &mount) {
                VolumeKind::DiskImage
            } else if is_network_fs(&file_system) {
                VolumeKind::Network
            } else if disk.is_removable() {
                VolumeKind::External
            } else {
                VolumeKind::Internal
            };
            let mut name = disk.name().to_string_lossy().to_string();
            if name.is_empty() {
                name = mount
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| mount.display().to_string());
            }
            let available = disk.available_space();
            let free = strict_free(&mount).unwrap_or(available).min(available);
            Some(Volume {
                name,
                mount,
                kind,
                file_system,
                total: disk.total_space(),
                available,
                free,
            })
        })
        .collect();
    volumes.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.total.cmp(&a.total)));
    volumes.dedup_by(|a, b| a.mount == b.mount);
    volumes
}

fn is_startup(mount: &Path) -> bool {
    if cfg!(windows) {
        let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        mount
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with(&system_drive.to_ascii_uppercase())
    } else {
        mount == Path::new("/")
    }
}

fn is_user_facing(mount: &Path) -> bool {
    if cfg!(target_os = "macos") {
        mount == Path::new("/") || mount.starts_with("/Volumes")
    } else if cfg!(windows) {
        true
    } else {
        mount == Path::new("/")
            || mount.starts_with("/home")
            || mount.starts_with("/media")
            || mount.starts_with("/mnt")
            || mount.starts_with("/run/media")
    }
}

fn is_network_fs(fs: &str) -> bool {
    matches!(
        fs.to_ascii_lowercase().as_str(),
        "smbfs" | "afpfs" | "nfs" | "webdav" | "cifs" | "smb2" | "fuse.sshfs"
    )
}

#[cfg(unix)]
fn strict_free(mount: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(mount.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(unix))]
fn strict_free(_mount: &Path) -> Option<u64> {
    None
}

/// Mount points of attached disk images, so they can be grouped away from real drives.
#[cfg(target_os = "macos")]
fn disk_image_mounts() -> Vec<PathBuf> {
    let Ok(output) = std::process::Command::new("hdiutil").arg("info").output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mount = line.split('\t').next_back()?.trim();
            mount.starts_with('/').then(|| PathBuf::from(mount))
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn disk_image_mounts() -> Vec<PathBuf> {
    Vec::new()
}

/// Opens Finder's Get Info window for `path`, or its Properties dialog on Windows.
pub fn get_info(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "tell application \"Finder\"",
                "-e",
                "activate",
                "-e",
                "open information window of (POSIX file (item 1 of argv) as alias)",
                "-e",
                "end tell",
                "-e",
                "end run",
            ])
            .arg(path)
            .spawn();
    }
    #[cfg(windows)]
    crate::platform::shell_execute("properties", path, None, false);
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = path;
}
