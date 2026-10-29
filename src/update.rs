//! Updates from GitHub Releases: find the latest published release, download the
//! build for this platform, check its SHA-256 against the digest GitHub reports,
//! and install it.
//!
//! macOS swaps the new `Burrdisk.app` in beside the running one, then relaunches
//! once this process has quit. Windows hands the downloaded installer the job:
//! it runs silently into the same install (per user or for all users) and
//! starts Burrdisk again when done.
//!
//! `BURRDISK_UPDATE_FEED` points the check at another release JSON (in the shape
//! of GitHub's `releases/latest`), for testing the whole flow against a local
//! server. It also turns updates on in builds that would otherwise skip them

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

const FEED: &str = "https://api.github.com/repos/bunkhq/burrdisk/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/bunkhq/burrdisk/releases";
/// How often a running Burrdisk looks for a new release
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// No release is anywhere near this; a bigger download is refused
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// Parses "1.2.3" or "v1.2.3"; anything after a `-` or `+` is ignored
    pub fn parse(text: &str) -> Option<Self> {
        let core = text.trim().trim_start_matches('v').split(['-', '+']).next()?;
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        let major = parts.next()??;
        let minor = parts.next().unwrap_or(Some(0))?;
        let patch = parts.next().unwrap_or(Some(0))?;
        Some(Self(major, minor, patch))
    }

    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("package version")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    /// The release notes' opening paragraph, as plain text
    pub summary: String,
    pub page: String,
    /// The build for this platform, if the release has one
    pub asset: Option<Asset>,
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lowercase hex SHA-256, from the digest GitHub computes on upload
    pub sha256: Option<String>,
}

fn feed_override() -> Option<String> {
    std::env::var("BURRDISK_UPDATE_FEED").ok().filter(|s| !s.is_empty())
}

/// Whether this copy looks for updates at all: installed release builds do;
/// `cargo run`, the dev app and debug builds don't
pub fn enabled() -> bool {
    feed_override().is_some() || (!cfg!(debug_assertions) && install_kind().is_some())
}

/// Why this copy can't replace itself, worded to follow "Burrdisk can't update
/// itself here: ". `None` when it can
pub fn blocker() -> Option<String> {
    match install_kind() {
        None => Some("it isn\u{2019}t running as an installed app".into()),
        Some(Install::Bundle(app)) => {
            let path = app.to_string_lossy();
            if path.contains("/AppTranslocation/") {
                return Some("macOS is running it from a temporary copy. Move it to Applications first".into());
            }
            let parent = app.parent()?;
            if !writable(parent) {
                return Some(format!("it can\u{2019}t write to {}", parent.display()));
            }
            None
        }
        Some(Install::Portable) => Some("this copy wasn\u{2019}t installed with the installer".into()),
        Some(Install::Installer(_)) => None,
    }
}

enum Install {
    /// macOS: the running `Burrdisk.app`
    Bundle(PathBuf),
    /// Windows: installed by the installer, which can run again over it. Holds
    /// whether it went in for all users (and so needs an administrator)
    Installer(bool),
    /// Windows: an exe run from anywhere else
    Portable,
}

fn install_kind() -> Option<Install> {
    let exe = std::env::current_exe().ok()?;
    if cfg!(target_os = "macos") {
        // .../Burrdisk.app/Contents/MacOS/burrdisk
        let app = exe.parent()?.parent()?.parent()?;
        if app.extension()? != "app" {
            return None;
        }
        // The dev app shares the bundle identifier; its name tells it apart
        let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
        if !plist.contains("<key>CFBundleName</key><string>Burrdisk</string>") {
            return None;
        }
        Some(Install::Bundle(app.to_path_buf()))
    } else if cfg!(windows) {
        let dir = exe.parent()?;
        if !dir.join("unins000.exe").exists() {
            return Some(Install::Portable);
        }
        let all_users = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|var| std::env::var_os(var))
            .any(|root| exe.starts_with(root));
        Some(Install::Installer(all_users))
    } else {
        None
    }
}

#[cfg(unix)]
fn writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(not(unix))]
fn writable(_: &Path) -> bool {
    true
}

/// The release asset built for this platform, as the release scripts name it
fn asset_suffix() -> String {
    if cfg!(target_os = "macos") {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x86_64" };
        format!("-{arch}.zip")
    } else {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        format!("-windows-{arch}-setup.exe")
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .user_agent(format!("Burrdisk/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The latest release, if it's newer than this copy. Blocks on the network
pub fn check() -> Result<Option<Release>> {
    let url = feed_override().unwrap_or_else(|| FEED.to_string());
    let body = agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("couldn\u{2019}t reach GitHub")?
        .body_mut()
        .read_to_string()
        .context("couldn\u{2019}t read the release")?;
    let json: serde_json::Value = serde_json::from_str(&body).context("the release didn\u{2019}t make sense")?;
    let release = parse_release(&json).ok_or_else(|| anyhow!("the release didn\u{2019}t make sense"))?;
    Ok((release.version > Version::current()).then_some(release))
}

fn parse_release(json: &serde_json::Value) -> Option<Release> {
    let version = Version::parse(json["tag_name"].as_str()?)?;
    let suffix = asset_suffix();
    let asset = json["assets"].as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        if !(name.starts_with("Burrdisk-") && name.ends_with(&suffix)) {
            return None;
        }
        Some(Asset {
            name: name.to_string(),
            url: a["browser_download_url"].as_str()?.to_string(),
            size: a["size"].as_u64().unwrap_or(0),
            sha256: a["digest"]
                .as_str()
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(|hex| hex.to_ascii_lowercase()),
        })
    });
    Some(Release {
        version,
        summary: summary(json["body"].as_str().unwrap_or_default()),
        page: json["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string(),
        asset,
    })
}

/// The first paragraph of Markdown release notes, with the markup taken out
fn summary(notes: &str) -> String {
    let paragraph = notes
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty() && !p.starts_with('#') && !p.starts_with('-') && !p.starts_with('*'))
        .unwrap_or_default();
    let mut text = String::new();
    let mut rest = paragraph;
    // [label](url) keeps its label
    while let Some(open) = rest.find('[') {
        text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find("](").and_then(|mid| after[mid..].find(')').map(|end| (mid, mid + end))) {
            Some((mid, end)) => {
                text.push_str(&after[..mid]);
                rest = &after[end + 1..];
            }
            None => {
                text.push('[');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    let text: String = text.replace(['*', '`'], "").split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > 240 {
        let cut: String = text.chars().take(237).collect();
        format!("{}\u{2026}", cut.trim_end_matches(|c: char| !c.is_whitespace()).trim_end())
    } else if text.trim_end_matches('.').contains(". ") {
        text
    } else {
        text.strip_suffix('.').map(str::to_string).unwrap_or(text)
    }
}

/// Progress of a download, shared with the UI
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

/// Where downloads and the unpacked app wait to be installed
fn staging() -> Result<PathBuf> {
    let dir = match install_kind() {
        // Beside the app, so the swap is a rename on one volume
        Some(Install::Bundle(app)) => app.parent().unwrap().join(".Burrdisk-update"),
        _ => std::env::temp_dir().join("Burrdisk-update"),
    };
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("couldn\u{2019}t write to {}", dir.display()))?;
    Ok(dir)
}

/// Downloads the release's asset and checks its digest. Blocks on the network
pub fn download(asset: &Asset, progress: Arc<Progress>) -> Result<PathBuf> {
    let dir = staging()?;
    let path = dir.join(&asset.name);
    let mut response = agent()
        .get(&asset.url)
        .config()
        .timeout_recv_body(Some(Duration::from_secs(600)))
        .build()
        .call()
        .context("couldn\u{2019}t reach GitHub")?;
    let total = response.body().content_length().unwrap_or(asset.size);
    if total > MAX_DOWNLOAD {
        bail!("the download is unexpectedly large");
    }
    progress.total.store(total, Ordering::Relaxed);
    let mut reader = response.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut file = std::fs::File::create(&path).context("couldn\u{2019}t save the download")?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if progress.cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_dir_all(&dir);
            bail!("cancelled");
        }
        let n = reader.read(&mut buf).context("the download was interrupted")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).context("couldn\u{2019}t save the download")?;
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    file.sync_all().ok();
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    match &asset.sha256 {
        Some(expected) if *expected != actual => {
            let _ = std::fs::remove_dir_all(&dir);
            bail!("the download didn\u{2019}t match its checksum")
        }
        // Every asset uploaded to GitHub since mid-2025 has a digest. One without
        // can't be checked, so it isn't installed
        None => {
            let _ = std::fs::remove_dir_all(&dir);
            bail!("the release has no checksum to check the download against")
        }
        Some(_) => Ok(path),
    }
}

/// What's left to do once Burrdisk has quit
pub struct Relaunch(Box<dyn FnOnce() -> Result<()> + Send>);

impl Relaunch {
    pub fn run(self) -> Result<()> {
        (self.0)()
    }
}

/// Installs a verified download as far as it can while Burrdisk runs. The
/// returned step relaunches; run it, then quit
pub fn install(release: &Release, download: &Path) -> Result<Relaunch> {
    match install_kind() {
        Some(Install::Bundle(app)) => install_bundle(&app, release.version, download),
        Some(Install::Installer(all_users)) => Ok(run_installer(download.to_path_buf(), all_users)),
        _ => bail!("this copy can\u{2019}t update itself"),
    }
}

fn run(cmd: &mut std::process::Command, what: &str) -> Result<String> {
    let out = cmd.output().with_context(|| format!("couldn\u{2019}t {what}"))?;
    if !out.status.success() {
        bail!("couldn\u{2019}t {what}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn install_bundle(app: &Path, version: Version, zip: &Path) -> Result<Relaunch> {
    use std::process::Command;
    let stage = zip.parent().unwrap().to_path_buf();
    let unpacked = stage.join("new");
    run(Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(zip).arg(&unpacked), "unpack the update")?;
    let new = unpacked.join("Burrdisk.app");
    let plist = new.join("Contents/Info.plist");
    let key = |key: &str| {
        run(
            Command::new("/usr/bin/plutil").args(["-extract", key, "raw", "-o", "-"]).arg(&plist),
            "read the update",
        )
    };
    if key("CFBundleIdentifier")? != "dev.burrdisk.Burrdisk" {
        bail!("the download isn\u{2019}t Burrdisk");
    }
    if Version::parse(&key("CFBundleShortVersionString")?) != Some(version) {
        bail!("the download is a different version than expected");
    }
    run(
        Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(&new),
        "verify the update\u{2019}s signature",
    )?;
    // Downloaded by Burrdisk itself, so there should be no quarantine flag; clear
    // any, so the relaunch isn't stopped by Gatekeeper's first-open check
    let _ = Command::new("/usr/bin/xattr").args(["-dr", "com.apple.quarantine"]).arg(&new).output();

    // Swap: the running app moves aside (it keeps running from there) and the new
    // one takes its place. Put the old one back if the second step fails
    let old = stage.join("Burrdisk-old.app");
    std::fs::rename(app, &old).context("couldn\u{2019}t move the current app aside")?;
    if let Err(err) = std::fs::rename(&new, app) {
        let _ = std::fs::rename(&old, app);
        return Err(err).context("couldn\u{2019}t put the new app in place");
    }

    let app = app.to_path_buf();
    Ok(Relaunch(Box::new(move || {
        // Waits for this process to exit, clears the staging folder, opens the new app
        Command::new("/bin/sh")
            .arg("-c")
            .arg(r#"while kill -0 "$1" 2>/dev/null; do sleep 0.1; done; rm -rf "$2"; open "$3""#)
            .arg("sh")
            .arg(std::process::id().to_string())
            .arg(&stage)
            .arg(&app)
            .spawn()
            .context("couldn\u{2019}t relaunch")?;
        Ok(())
    })))
}

fn run_installer(installer: PathBuf, all_users: bool) -> Relaunch {
    Relaunch(Box::new(move || start_installer(&installer, all_users)))
}

/// Inno Setup: a progress window only, into the existing install, then
/// `/relaunch=1` starts Burrdisk again (see installer/burrdisk.iss). It waits
/// for this process to close, and closes it if it hasn't
#[cfg(windows)]
fn start_installer(installer: &Path, all_users: bool) -> Result<()> {
    let scope = if all_users { "/ALLUSERS" } else { "/CURRENTUSER" };
    let params = format!("/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /relaunch=1 {scope}");
    // Asks for the process handle only to learn that it started; Burrdisk quits next
    let started = crate::platform::shell_execute("open", installer, Some(std::ffi::OsStr::new(&params)), true);
    started.map(|_| ()).ok_or_else(|| anyhow!("couldn\u{2019}t start the installer"))
}

#[cfg(not(windows))]
fn start_installer(_: &Path, _: bool) -> Result<()> {
    bail!("the installer only runs on Windows")
}

/// Clears what an earlier update left behind (a failed or interrupted one)
pub fn clean_up() {
    if let Some(Install::Bundle(app)) = install_kind() {
        let _ = std::fs::remove_dir_all(app.parent().unwrap().join(".Burrdisk-update"));
    } else {
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("Burrdisk-update"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(Version::parse("v0.2.0"), Some(Version(0, 2, 0)));
        assert_eq!(Version::parse("1.10"), Some(Version(1, 10, 0)));
        assert_eq!(Version::parse("2.0.1-beta.1"), Some(Version(2, 0, 1)));
        assert_eq!(Version::parse("latest"), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
    }

    #[test]
    fn release_json() {
        let json = serde_json::json!({
            "tag_name": "v9.1.0",
            "html_url": "https://github.com/bunkhq/burrdisk/releases/tag/v9.1.0",
            "body": "Burrdisk now runs on **Windows**, see [the notes](https://x.y).\n\n- **Windows.** The full app.\n",
            "assets": [
                { "name": "Burrdisk-9.1.0-arm64.dmg", "browser_download_url": "a", "size": 1, "digest": "sha256:AA" },
                { "name": "Burrdisk-9.1.0-arm64.zip", "browser_download_url": "b", "size": 2, "digest": "sha256:BB" },
                { "name": "Burrdisk-9.1.0-x86_64.zip", "browser_download_url": "c", "size": 3, "digest": "sha256:CC" },
                { "name": "Burrdisk-9.1.0-windows-x64-setup.exe", "browser_download_url": "d", "size": 4, "digest": "sha256:DD" },
            ],
        });
        let release = parse_release(&json).unwrap();
        assert_eq!(release.version, Version(9, 1, 0));
        assert_eq!(release.summary, "Burrdisk now runs on Windows, see the notes");
        let asset = release.asset.unwrap();
        let expect = if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") { ("b", "bb") } else { ("c", "cc") }
        } else {
            ("d", "dd")
        };
        assert_eq!((asset.url.as_str(), asset.sha256.as_deref().unwrap()), expect);
    }
}

