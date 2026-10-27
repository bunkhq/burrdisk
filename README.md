<img src="assets/icon/macos-256.png" width="96" height="96" alt="">

# Burrdisk

A fast, friendly disk space visualizer for macOS and Windows, written in Rust with [GPUI](https://gpui.rs).

- **Pick a disk.** Every disk is drawn as a burr. Before a scan its spines show how full it is; after one, they grow into its biggest folders. The menu beside each Scan button can rescan (also as administrator), forget a scan, show the disk in Finder or open Get Info.
- **Explore.** An interactive sunburst: click a ring to dive into a folder, click the center to go back. The sidebar lists everything in the current folder, synced with the chart.
- **Collect, then clean.** Drag files and folders (from the chart or the list) into the Collector at the bottom, or right-click one and pick Move to Collector. Review the pile, then move it all to the Trash in one go. A 5-second countdown runs first; Stop or Esc cancels it.

## Install

Download from [Releases](https://github.com/bunkhq/burrdisk/releases):

- **macOS:** open `Burrdisk-<version>-arm64.dmg` and drag Burrdisk to Applications. It's signed ad hoc rather than notarized, so the first launch needs right-click > Open (or System Settings > Privacy & Security > Open Anyway). Apple silicon only for now.
- **Windows:** run `Burrdisk-<version>-windows-x64-setup.exe`. It installs for your user without an administrator prompt (or for everyone, if you choose), adds Burrdisk to the Start menu and can be removed from Settings > Apps. It isn't code signed, so SmartScreen may ask first (More info > Run anyway).

After that, Burrdisk updates itself. It checks GitHub Releases a few seconds after launch and every six hours (on macOS, also from **Burrdisk > Check for Updates…**). When a new version is out, an Update button appears in the title bar. **Update and Restart** downloads it, checks it against the SHA-256 digest GitHub records for the file, installs it and reopens Burrdisk. On macOS the new app replaces the old one in place. Because the app is signed ad hoc, Full Disk Access has to be turned on again after each update. On Windows the new installer runs silently over the existing install.

## Build

Requires Rust 1.88+ and Xcode with the Metal Toolchain (`xcodebuild -downloadComponent MetalToolchain`).

```sh
cargo run --release              # open the disk picker
cargo run --release -- ~/Library # scan a folder directly
```

For development with Full Disk Access, use `script/dev-app.sh` instead. It builds a `Burrdisk Dev.app` signed with a local certificate (created once in your login keychain), so the Full Disk Access grant survives rebuilds. Run `script/dev-app.sh --grant` the first time and add the app in System Settings.

Builds that aren't installed (`cargo run`, `Burrdisk Dev.app`) don't look for updates. To try the updater, point a bundled `Burrdisk.app` at a release JSON of your own, in the shape of GitHub's `releases/latest` API: `BURRDISK_UPDATE_FEED=http://127.0.0.1:8000/release.json Burrdisk.app/Contents/MacOS/burrdisk`.

`script/bundle.sh` builds the release `Burrdisk.app` and packs it into a disk image and a zip in `dist/`.

The app icons are drawn by `script/icons/make_icons.py`; `script/icons/build.sh` renders them and packs `assets/macos/Burrdisk.icns` and `assets/windows/burrdisk.ico` (needs Python 3 and `cargo install resvg`).

Without Xcode, build with `--features runtime_shaders` to compile Metal shaders at launch instead.

On Windows, `cargo run --release` needs Visual Studio's C++ build tools and the Windows SDK. `script\bundle.ps1` builds the release `Burrdisk.exe`, its installer (with [Inno Setup 6](https://jrsoftware.org/isinfo.php)) and a portable zip into `dist\`.

Pushing a `v*` tag runs `.github/workflows/release.yml`, which builds the disk image and the installer on GitHub's runners and attaches them to a draft release.

## Keyboard

| macOS | Windows | Action |
| --- | --- | --- |
| `⌘[`, `⌘]` | `Ctrl+[`, `Ctrl+]`, `Alt+←`, `Alt+→` | Back up to the parent folder (or the disk list), and forward down the path you came back from |
| `Esc`, `⌫`, `⌘↑` | `Esc`, `Backspace`, `Ctrl+↑` | Go up a folder (or back to the disk list); stops a Trash countdown |
| `Space` | | Quick Look the hovered item (macOS only) |
| `⌘⌫` | `Ctrl+Backspace` | Move the hovered item to the Collector |
| `⌘Q` | `Alt+F4` | Quit |

## Notes

- Sizes are allocated bytes on disk, so sparse and compressed files count correctly. Hard links are counted once.
- On the startup disk, space the scan can't see (snapshots, protected system files) shows as **Hidden space**. Give Burrdisk Full Disk Access in System Settings to see more.
- Deleting moves items to the Trash (the Recycle Bin on Windows). Nothing is removed permanently.
- On Windows, files OneDrive keeps online only count as the space they use locally. Hard links (common under `C:\Windows\WinSxS`) are counted at every path, so that folder can read larger than it is. Rescan as Administrator goes through a UAC prompt.

Created in [T3 Code](https://t3.codes).
