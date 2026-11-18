# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Native desktop app for macOS (Apple silicon) and Windows (x64), built with GPUI (Rust; Metal on macOS, DirectX on Windows). It is not a web app, but it draws its own UI with no platform widgets, so web-style guidance applies to its surfaces. Platform conventions still hold where they show: Finder vs File Explorer, Trash vs Recycle Bin, ⌘ vs Ctrl, drawn window controls on Windows.

## Users

A mix: power users and developers who know where caches and build folders hide, everyday owners who just got a "disk almost full" warning, and the author as a daily personal tool. The job is the same for all of them: find out what is eating the disk, decide what can go, and safely clear it.

## Product Purpose

Burrdisk scans a disk or folder and shows where the space went, then lets the user collect what they want gone and move it to the Trash in one go. Success is a user going from "my disk is full" to freed space in a few minutes without fear of deleting the wrong thing.

## Positioning

An interactive sunburst you dig through (click a ring to open a folder, the center to go back), synced with a sidebar list, plus a collection tray: collect first, review the pile, then clean. Deletion only ever moves to the Trash. Free, open source, and on both platforms, where its closest rivals are Mac-only and paid (DaisyDisk), unsigned (SquirrelDisk), or delete permanently (CleanMyMac).

## Operating Context

Home lists disks: the startup disk first, other drives below, mounted disk images tucked away. Scanning shows live progress. The explorer is the sunburst, sidebar, and tray. Scans are cached for the session. Full Disk Access (macOS) and Rescan as Administrator (both; on Windows it reads the NTFS file table directly) change how much the scan can see. Burrdisk updates itself from GitHub Releases; an Update button in the titlebar and a card below it ask before anything is downloaded, and restarting is the user's call.

## Capabilities and Constraints

- Sizes are allocated bytes on disk; hard links counted once on macOS. On Windows, OneDrive online-only files count only what they keep locally.
- Space the scan cannot see on the startup disk shows as Hidden space.
- Deleting moves items to the Trash (Recycle Bin); nothing is removed permanently.
- Keyboard: Esc, ⌘[, ⌫ go up a folder; ⌘Q quits (Ctrl and Alt equivalents on Windows). The mouse's back and forward buttons step back and forward like the titlebar arrows.
- Lightweight is a feature: a scan of millions of files stays in a compact in-memory tree, the app opens instantly, and new capabilities must not add background services, accounts, or telemetry.

## Roadmap

From a survey of 30+ disk tools (DaisyDisk, WizTree, WinDirStat, TreeSize, CleanMyMac, GrandPerspective, SquirrelDisk, Radix, ncdu, gdu, dua, npkill, Czkawka and others), in order:

1. Record when files last changed, and show age: a chart mode colored by it, and ages in the list
2. Search and filter by name, extension, size and age
3. Explain Hidden space and well-known system files (snapshots, purgeable, hiberfil.sys, pagefile.sys, WinSxS, WSL and Docker disks) in plain words, with the safe fix
4. "Safe to clear" and developer-junk suggestions (caches, old Downloads, node_modules, target, DerivedData) that feed the Collector, with recently used projects left alone
5. Saved scans and "what grew since last time"
6. Duplicates by content
7. Export and a command line

Every addition goes through the Collector and the Trash, never a one-click delete, and asks for no more than one extra click over what it replaces.

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
5. Answer "what can I delete?", not just "what is big?": surface age, purpose and safety next to size.
