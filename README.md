<img src="assets/icon.svg" width="64" height="64" alt="">

# Burrdisk

A fast, friendly disk space visualizer for macOS, written in Rust with [GPUI](https://gpui.rs).

- **Pick a disk.** Your startup disk front and center, other drives below, mounted disk images tucked away.
- **Explore.** An interactive sunburst: click a ring to dive into a folder, click the center to go back. The sidebar lists everything in the current folder, synced with the chart.
- **Collect, then clean.** Drag files and folders (from the chart or the list) into the tray at the bottom. Review the pile, then move it all to the Trash in one go.

## Build

Requires Rust 1.88+ and Xcode with the Metal Toolchain (`xcodebuild -downloadComponent MetalToolchain`).

```sh
cargo run --release              # open the disk picker
cargo run --release -- ~/Library # scan a folder directly
```

Without Xcode, build with `--features runtime_shaders` to compile Metal shaders at launch instead.

## Keyboard

| Key | Action |
| --- | --- |
| `Esc`, `⌘[`, `⌫` | Go up a folder (or back to the disk list) |
| `⌘Q` | Quit |

## Notes

- Sizes are allocated bytes on disk, so sparse and compressed files count correctly. Hard links are counted once.
- On the startup disk, space the scan can't see (snapshots, protected system files) shows as **Hidden space**. Give Burrdisk Full Disk Access in System Settings to see more.
- Deleting moves items to the Trash. Nothing is removed permanently.

Windows support is planned; the scanner and volume code are already cross-platform.

Created in [T3 Code](https://t3.codes).
