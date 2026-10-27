//! Names and helpers that differ between macOS and Windows.

/// The system file manager, as in "Show in Finder".
pub const FILE_MANAGER: &str = if cfg!(windows) { "File Explorer" } else { "Finder" };

/// Where deleted items go.
pub const TRASH: &str = if cfg!(windows) { "Recycle Bin" } else { "Trash" };

/// The menu item that opens the system's info window for a disk.
pub const GET_INFO: &str = if cfg!(windows) { "Properties" } else { "Get Info" };

/// Key hint for moving the hovered item to the Collector.
pub const COLLECT_KEYS: &str = if cfg!(target_os = "macos") { "\u{2318}\u{232b}" } else { "Ctrl+Backspace" };

/// Room at the left of a title bar for the macOS window buttons. Windows draws
/// its caption buttons at the right instead (see `window_controls`).
pub const TITLEBAR_INSET: f32 = if cfg!(target_os = "macos") { 76. } else { 0. };

/// Padding at the right end of a title bar. Windows' caption buttons sit flush
/// against the window edge, as the system's own do.
pub const TITLEBAR_END: f32 = if cfg!(windows) { 0. } else { 16. };

/// Runs `verb` on `path` through the Windows shell, as a double-click or a context
/// menu item in File Explorer would. Returns the started process, if any.
#[cfg(windows)]
pub fn shell_execute(
    verb: &str,
    path: &std::path::Path,
    params: Option<&std::ffi::OsStr>,
    wait: bool,
) -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let verb = wide(verb.as_ref());
    let file = wide(path.as_os_str());
    let params = params.map(wide);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_INVOKEIDLIST | SEE_MASK_NOASYNC | if wait { SEE_MASK_NOCLOSEPROCESS } else { 0 };
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ref().map_or(std::ptr::null(), |p| p.as_ptr());
    info.nShow = SW_SHOWNORMAL;
    let ok = unsafe { ShellExecuteExW(&mut info) } != 0;
    (ok && !info.hProcess.is_null()).then_some(info.hProcess)
}
