<img src="assets/icon.svg" width="64" height="64" alt="">

# Burrdisk

A fast, friendly disk space visualizer for macOS, written in Rust with [GPUI](https://gpui.rs).

- **Pick a disk.** Every disk is drawn as a burr. Before a scan its spines show how full it is; after one, they grow into its biggest folders. The menu beside each Scan button can rescan (also as administrator), forget a scan, show the disk in Finder or open Get Info.
- **Explore.** An interactive sunburst: click a ring to dive into a folder, click the center to go back. The sidebar lists everything in the current folder, synced with the chart.
- **Collect, then clean.** Drag files and folders (from the chart or the list) into the Collector at the bottom, or right-click one and pick Move to Collector. Review the pile, then move it all to the Trash in one go. A 5-second countdown runs first; Stop or Esc cancels it.

## Install

Download `Burrdisk-<version>-arm64.zip` from [Releases](https://github.com/bunkhq/burrdisk/releases), unzip it and move `Burrdisk.app` to Applications. It's signed ad hoc rather than notarized, so the first launch needs right-click > Open (or System Settings > Privacy & Security > Open Anyway). Apple silicon only for now.

## Build

Requires Rust 1.88+ and Xcode with the Metal Toolchain (`xcodebuild -downloadComponent MetalToolchain`).

```sh
cargo run --release              # open the disk picker
cargo run --release -- ~/Library # scan a folder directly
```

For development with Full Disk Access, use `script/dev-app.sh` instead. It builds a `Burrdisk Dev.app` signed with a local certificate (created once in your login keychain), so the Full Disk Access grant survives rebuilds. Run `script/dev-app.sh --grant` the first time and add the app in System Settings.

`script/bundle.sh` builds the release `Burrdisk.app` and zips it into `dist/`.

Without Xcode, build with `--features runtime_shaders` to compile Metal shaders at launch instead.

## Keyboard

| Key | Action |
| --- | --- |
| `⌘[`, `⌘]` | Back up to the parent folder (or the disk list), and forward down the path you came back from |
| `Esc`, `⌫`, `⌘↑` | Go up a folder (or back to the disk list); stops a Trash countdown |
| `Space` | Quick Look the hovered item |
| `⌘⌫` | Move the hovered item to the Collector |
| `⌘Q` | Quit |

## Notes

- Sizes are allocated bytes on disk, so sparse and compressed files count correctly. Hard links are counted once.
- On the startup disk, space the scan can't see (snapshots, protected system files) shows as **Hidden space**. Give Burrdisk Full Disk Access in System Settings to see more.
- Deleting moves items to the Trash. Nothing is removed permanently.

Windows support is planned; the scanner and volume code are already cross-platform.

Created in [T3 Code](https://t3.codes).
