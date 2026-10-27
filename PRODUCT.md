# Product

<!-- impeccable:product-schema 1 -->

## Platform

macos

Native macOS desktop app built with GPUI (Rust, Metal). Not a web app; Windows is planned later.

## Users

A mix: Mac power users who know where caches and build folders hide, everyday Mac owners who just got a "disk almost full" warning, and the author as a daily personal tool. The job is the same for all of them: find out what is eating the disk and safely clear it.

## Product Purpose

Burrdisk scans a disk or folder and shows where the space went, then lets the user collect what they want gone and move it to the Trash in one go. Success is a user going from "my disk is full" to freed space in a few minutes without fear of deleting the wrong thing.

## Positioning

An interactive sunburst you dive through (click a ring to open a folder, the center to go back), synced with a sidebar list, plus a collection tray: collect first, review the pile, then clean. Deletion only ever moves to the Trash.

## Operating Context

Home lists disks: the startup disk first, other drives below, mounted disk images tucked away. Scanning shows live progress. The explorer is the sunburst, sidebar, and tray. Scans are cached for the session. Full Disk Access changes how much of the startup disk the scan can see.

## Capabilities and Constraints

- Sizes are allocated bytes on disk; hard links counted once.
- Space the scan cannot see on the startup disk shows as Hidden space.
- Deleting moves items to the Trash; nothing is removed permanently.
- Keyboard: Esc, ⌘[, ⌫ go up a folder; ⌘Q quits.

## Brand Commitments

- Name: Burrdisk, after the burr, the prickly seed that sticks to things. The sunburst chart reads as one.
- Orange is the brand accent and stays.
- The sunburst's data palette stays.
- Everything else about the home screen's look is free to replace.

## Evidence on Hand

App icon masters in `assets/icon/` (a scanned burr, drawn by `script/icons/make_icons.py`); icon set in `assets/icons/`. No testimonials, benchmarks, or user counts exist; do not invent any.

## Product Principles

1. Show the space, don't describe it: the visualization is the product.
2. Safe by default: every destructive step is reversible or interruptible.
3. Fast and direct: the disk you want is one click away.
4. Quiet chrome, loud data: color belongs to what is on the disk.
