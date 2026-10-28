# AGENTS.md

## What this is

Burrdisk is a native disk space visualizer for macOS and Windows, written in Rust with GPUI. It scans a disk or folder, draws it as a sunburst you can dig through, and lets you collect what you want gone before moving it all to the Trash (Recycle Bin on Windows) in one go. Nothing is ever deleted permanently.

- `src/` is the app. `src/ui/` holds the screens (home, scanning, explorer, access, update), `src/scan.rs` and `src/scan/` the scanners (including the Windows MFT scan), `src/sunburst.rs` the chart
- `site/` is the project website, published to GitHub Pages from `main`
- `script/` holds the dev, bundle, icon and promo scripts; `installer/` the Windows Inno Setup script
- `PRODUCT.md` and `DESIGN.md` describe the product and the design system, read them before UI or copy changes
- Both platforms are supported, keep `cfg(target_os = ...)` paths in step and don't break the other platform's build

## Conventions

- Comment only when it helps contributors understand non-obvious behavior
- Use scoped Conventional Commits, e.g. `fix(explorer): ...`, `feat(scan): ...`, `chore(site): ...`
- Keep commit messages to the subject line, avoid commit bodies unless absolutely necessary, and keep them very short when used
- Always create new branches for features, never work on the same branch you start on unless it currently fits the work you are doing, and never work on `main`

## Text Conventions

- Do not put periods at the end of comments or user-facing strings

## Testing

- Preferably, keep testing to a minimum, don't add new tests unless asked, and don't keep test files around. `cargo build` (or `cargo check`) is usually enough, run `cargo test` only when touching code the existing unit tests cover
- Preferably, don't test your changes yourself by launching and driving the app or opening the site in a browser. The human can test them faster, has a better eye, and will tell you what is wrong. Only do so when prompted
