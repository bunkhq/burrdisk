---
name: Burrdisk
description: A native macOS and Windows disk space visualizer drawn as burrs and a sunburst you dive through.
colors:
  ember: "#f97316"
  ember-hi: "#fdba74"
  ember-lo: "#ea580c"
  ember-soft: "#f9731624"
  ember-ink: "#1f0b00"
  trash-red: "#ef4444"
  trash-red-hi: "#f87171"
  trash-red-lo: "#dc2626"
  data-dir-root: "oklch(74% 0.15 52)"
  data-file-root: "oklch(58% 0.055 52)"
  data-hidden: "oklch(48% 0.04 300)"
  data-smaller: "oklch(40% 0.012 50)"
  fullness-calm: "#40dd7f"
  fullness-warn: "#8fdd40"
  fullness-full: "#dd5c40"
  graphite-ground: "#141416"
  graphite-surface: "#1c1c1f"
  graphite-hover: "#232327"
  graphite-raised: "#2a2a2f"
  stone-top: "#34302d"
  stone-bottom: "#2a2725"
  menu-ground: "#262220"
  seed-core: "#26201d"
  hairline: "#ffffff12"
  hairline-strong: "#ffffff24"
  track: "#ffffff14"
  text: "#f2f2f3"
  text-muted: "#a1a1a8"
  text-faint: "#6b6b73"
typography:
  display:
    fontFamily: "Mona Sans (bundled), sans-serif"
    fontSize: "34px"
    fontWeight: 700
    lineHeight: "40px"
  figure:
    fontFamily: "Mona Sans (bundled), sans-serif"
    fontSize: "24px"
    fontWeight: 700
    lineHeight: 1
  headline:
    fontFamily: "Mona Sans (bundled), sans-serif"
    fontSize: "18px"
    fontWeight: 600
  title:
    fontFamily: "Mona Sans (bundled), sans-serif"
    fontSize: "16px"
    fontWeight: 600
  body-lead:
    fontFamily: "Mona Sans (bundled), sans-serif"
    fontSize: "17px"
    fontWeight: 400
  body:
    fontFamily: "Mona Sans (bundled), sans-serif"
    fontSize: "14px"
    fontWeight: 400
  label:
    fontFamily: "Mona Sans (bundled), sans-serif"
    fontSize: "12px"
    fontWeight: 400
rounded:
  xs: "6px"
  sm: "8px"
  md: "9px"
  lg: "10px"
  row: "11px"
  xl: "14px"
  full: "9999px"
spacing:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  2xl: "20px"
  page-x: "44px"
  section: "52px"
components:
  button-primary:
    backgroundColor: "{colors.ember}"
    textColor: "{colors.ember-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "32px"
  button-secondary:
    backgroundColor: "{colors.stone-top}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "32px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "32px"
  button-ghost-hover:
    backgroundColor: "{colors.graphite-hover}"
    textColor: "{colors.text}"
  button-danger:
    backgroundColor: "{colors.trash-red}"
    textColor: "#ffffff"
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "32px"
  button-icon:
    backgroundColor: "transparent"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.sm}"
    size: "28px"
  button-icon-hover:
    backgroundColor: "{colors.graphite-raised}"
    textColor: "{colors.text}"
  menu:
    backgroundColor: "{colors.menu-ground}"
    textColor: "{colors.text}"
    rounded: "{rounded.lg}"
    padding: "4px 0"
    width: "210px"
  menu-item-hover:
    backgroundColor: "{colors.ember}"
    textColor: "{colors.ember-ink}"
    rounded: "{rounded.xs}"
    height: "26px"
  sidebar-row:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    rounded: "{rounded.row}"
    height: "50px"
  sidebar-row-compact:
    backgroundColor: "transparent"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.sm}"
    height: "26px"
  mode-switch-segment-on:
    backgroundColor: "{colors.ember-soft}"
    textColor: "{colors.ember-hi}"
    rounded: "7px"
    height: "24px"
  tray-chip:
    backgroundColor: "{colors.graphite-raised}"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    height: "30px"
  image-row-hover:
    backgroundColor: "{colors.graphite-surface}"
    rounded: "{rounded.lg}"
    height: "40px"
---

# Design System: Burrdisk

## Overview

**Creative North Star: "Quiet Chrome, Loud Data"**

Burrdisk is a dark, near-monochrome Mac app in which the only real color comes from what is on the disk. The chrome is graphite: a near-black ground, two slightly lighter surfaces, hairline borders of white at low alpha, and three text greys. Against it, the data glows. Disks are drawn as burrs (a seed core with tapered spines). Folders are rings of the sunburst, colored in an OKLCH wheel that keeps every hue at equal brightness. Brand orange marks the primary action and the "this disk has a cached scan" state. Red is reserved for moving things to the Trash.

Density is that of a desktop utility: a 32px control height, 14px body text, compact 12px labels. The UI has no web-style hero sections and no card grids. Home lays the disks on the bare ground with no containers. The explorer is a full-bleed chart with a 360px sidebar and an 80px collection tray. Every surface moves. Sections rise in, staggered. Spines grow out clockwise. Hovers ease in and out instead of snapping.

The direction contract asked for a warm "loam" ground. The shipped ground is a cool graphite (`graphite-ground`), and only some pieces run warm: the seed core, the stone secondary button, and the menu ground. This file records the shipped values.

**Key Characteristics:**
- Graphite chrome, color only from data, brand orange, and Trash red.
- The burr is the signature form: a filled seed core with tapered spines that end in a small bead, and that are always data-true.
- Filled controls are lit from above: a vertical gradient and a halo in their own color. Buttons never draw an outline; a soft light follows the cursor inside them.
- Depth comes from colored halos plus a 1px contact shadow, never hard offsets.
- One bundled typeface, Mona Sans, on every platform. Weight and size carry hierarchy, and figures are set big and bold with a smaller muted unit.

## Colors

A graphite ground with one warm brand voice (orange), one alarm voice (red), and a perceptually even OKLCH data wheel that owns everything else.

### Primary
- **Ember** (`ember`): the brand accent. Used for the primary button's tone, the hover fill of popup-menu items, the Collector's count badge, the drag-over outline, the 1.5px rim around a burr core whose disk has a cached scan, the scanned-dot on disk-image rows, and the all-orange wordmark burr.
- **Ember Hi / Ember Lo** (`ember-hi`, `ember-lo`): the top and bottom of every orange gradient. Ember Hi also colors small state text on the dark ground ("Scanned 3 min ago", the go-up label in the chart center, the previewing folder icon).
- **Ember Soft** (`ember-soft`): a 14% orange wash used for drop targets, the pressed icon button, and the Full Disk Access hint panel.
- **Ember Ink** (`ember-ink`): a near-black brown used for text and icons that sit on orange. It is never plain black.

### Secondary
- **Trash Red** (`trash-red`, with `trash-red-hi` / `trash-red-lo` for its gradient): the Move to Trash button, the count badge while it counts down, and error toasts.

### Tertiary (data)
- **Data Wheel** (`data-dir-root`, `data-file-root`): folder and file colors come from `color_for`. Hue = 52° + position × 300°, so it runs around the circle. Folders start at L 0.74 / C 0.15, and files are quieter at L 0.58 / C 0.055. Each ring deeper drops lightness (0.045 for folders, 0.03 for files) and chroma (0.014 for folders, 0.006 for files). The wheel is rotated so that the middle of the largest child sits at the start hue. The palette is a product commitment and stays.
- **Hidden Violet** (`data-hidden`): space the scan cannot see.
- **Smaller Dust** (`data-smaller`): the "smaller items" remainder, which gets slightly darker with each ring deeper.
- **Type Hues**: in the File Types chart each category keeps one hue on the same OKLCH ramp (L 0.74 / C 0.15 for the category, 0.68 / 0.125 for its extensions fanned ±18° around it, 0.57 / 0.06 for files): Documents 52°, Archives 88°, Images 150°, Audio 200°, Programs 255°, Video 318°. Other files stay near-grey (C 0.05). This is the one chart where hue means kind rather than position, so The Biggest Is Orange Rule does not apply there.
- **Fullness** (`fullness-calm` → `fullness-warn` → `fullness-full`): the lit spines of an unscanned burr. The color is HSL with S 0.70 and L 0.56. It stays green below 70% full, sweeps through yellow-green to red-orange between 70% and 90%, and is red-orange above 90%.

### Neutral
- **Graphite Ground** (`graphite-ground`): the window background behind everything.
- **Graphite Surface** (`graphite-surface`): the sidebar, the top of the tray's gradient, the hover fill of disk-image rows.
- **Graphite Hover / Raised** (`graphite-hover`, `graphite-raised`): hover and pressed fills for ghost and icon buttons, the toast body, and the top of tray chips.
- **Stone** (`stone-top`, `stone-bottom`): the warm gradient of the secondary button (hover `#433d38`→`#36312d`, pressed `#262321`).
- **Menu Ground** (`menu-ground`): the popup menu body.
- **Seed Core** (`seed-core`): the warm dark disc at the center of every data burr.
- **Hairlines** (`hairline`, `hairline-strong`, `track`): borders, dividers, the dim stubs of unused burr spines, and the empty track behind bars.
- **Text / Muted / Faint** (`text`, `text-muted`, `text-faint`): primary copy, secondary copy (units, details, capacity), and tertiary copy (counts, hints, disabled menu items).

### Named Rules
**The Data Owns Color Rule.** Chrome is graphite and grey. Saturated color comes only from the data wheel, fullness, Ember (brand, primary action, cached-scan state), or Trash Red (destruction). If an element is not data, an action, or a state, it is grey.

**The Biggest Is Orange Rule.** The data wheel rotates so the largest child of the focused folder takes the wheel's start hue, near the brand orange. The biggest thing on screen therefore always reads as Burrdisk's color.

**The Red Means Trash Rule.** Trash Red appears only where something is about to move to the Trash, or where a Trash operation failed. The one other red is the Windows close button's hover, which is the platform's own convention, not Burrdisk's.

## Typography

**Display Font:** Mona Sans (SIL OFL, bundled in `assets/fonts/`: Regular, Medium, SemiBold, Bold)
**Body Font:** the same family
**Label/Mono Font:** none distinct

**Character:** One sturdy, warm grotesque, bundled so Burrdisk looks like itself on macOS and Windows alike instead of falling back to each platform's default. Hierarchy comes from weight (bold for names and figures, semibold for headings and buttons, medium for row names) and size, not from a second family. Figures are proportional: Mona Sans' tabular set uses a slashed zero, which reads as code in a consumer utility, so `tnum` stays off.

### Hierarchy
- **Display** (700, 34px / 40px, scaled ×0.82–1.12 with window width around 1180px): the startup disk's name on home. Only one appears per screen.
- **Figure** (700, 22–30px, line height equal to size): amounts. Examples are the sidebar's focused size (24px), the tray total (22px), the live scanning total (30px), the burr-center percent (26px × scale),. An amount pairs its bold number with a smaller medium-weight muted unit (14px or 13px). The unit sits on the number's baseline, lifted by 0.145 × the size difference.
- **Headline** (600, 18px): the "Scanning <name>" title.
- **Title** (600, 16px): section headings on home ("Other drives", "Disk images" followed by a faint count) and the sidebar's focused folder name.
- **Body Lead** (17px × scale): the capacity sentence under the startup disk's name, "<used> used of <total>". The used figure is semibold and the rest is muted.
- **Body** (14px): button labels (600), row names (500), menu items, and toast text.
- **Label** (12px): drive capacity lines, chip names and sizes, item counts, hints, and footer rows. These are usually muted or faint.

### Named Rules
**The Sentence Not Stat Block Rule.** Capacity is written as one plain line ("120 GB used of 500 GB", "40 GB free of 1 TB", details joined with " · "), never as a grid of labeled stat tiles.

**The Number Then Unit Rule.** Sizes are a bold number with a smaller muted unit on a shared baseline. A percent sign sits snug against its number. Word units like "GB" get 0.12 em of air.

## Layout

The app is one window with a drawn titlebar row (56px tall) and screen-specific content below. On macOS it keeps a 96px left inset to clear the traffic lights. On Windows the bar runs from the left edge, its empty space drags the window (double-click maximizes), and drawn minimize / maximize-or-restore / close buttons sit flush at the right end, each 46px wide and the bar's full height, with 10px 1px-stroke glyphs. Their hover is a 6% white wash, except Close, which takes Windows' own close red (`#c42b1c`) with a white glyph; glyphs go faint while the window is inactive.

**Windows scale.** Every design size on Windows is drawn 1.2× (`ui::UI_SCALE`, applied by `ui::px` and the rem size; 1px hairlines stay 1px), because a Windows pixel at 100% scaling is far smaller than a macOS point. The numbers in this file are design units before that scale.

- **Home:** a single scrolling column, max 960px wide and centered, with 44px side padding and 56px bottom padding. Sections are 52px apart. The startup disk is a two-column lead: a 300px burr (× scale) on the left, a 44px gap, then name, capacity, details, and the split button on the right. Other drives wrap as 296px-wide items (a 104px burr plus text) with a 28px column gap. Disk images form a two-column grid of 40px rows with a 24px column gap and a 4px row gap. Lists longer than 7 collapse to 6, and the rest slide open like a drawer.
- **Explorer:** a full-bleed sunburst, a 360px sidebar on the right with a left hairline, and an 80px tray across the bottom that has a top hairline and a surface-to-ground gradient. Toasts float 96px above the bottom edge, centered, max 640px wide.
- **Rhythm:** spacing follows a 4px grid. Steps of 6, 8, 12, 16, and 20px handle most inner gaps and padding.

## Elevation & Depth

Depth is a hybrid. Surfaces are tonal layers (ground → surface → raised). Lifted elements carry a **halo in their own color** plus a tight black contact shadow, rather than a neutral drop shadow. Halos grow on hover and flare on change.

### Shadow Vocabulary
- **Glow** (`0 6px 18px -4px <tone @ 0.28 × strength>, 0 1px 2px rgba(0,0,0,0.35)`): filled buttons (strength 1.0, rising to 2.2 on hover), the tray count badge (1.4, flaring to 4.4 when the count changes), toasts (1.2, tone = result color), and hovered sidebar-row tiles (up to 1.2 in the row's data color).
- **Contact** (`0 1px 2px rgba(0,0,0,0.3)`, growing to `0 2px 6px rgba(0,0,0,0.4)` on hover): secondary buttons.
- **Float** (`0 10px 28px -6px rgba(0,0,0,0.45), 0 1px 3px rgba(0,0,0,0.3)`): popup menus.

### Named Rules
**The Halo Not Drop Rule.** Raised colored elements cast light in their own hue. Shadows are always centered and soft, with no hard offsets.

## Shapes

Corners are gently rounded and tighten as elements get smaller: 6px on menu items and chip tiles, 8px on icon buttons, 9px on buttons, chips, and row tiles, 10px on menus, image rows and hint panels, 11px on sidebar rows, and 14px on toasts and the tray badge. Full pills are used for status dots, toast icon wells, and bars. Bars are 3px (row share) and 6px (sidebar composition strip, which has 2px gaps between segments). Borders are 1px hairlines. The empty tray badge uses a 2px dashed `hairline-strong` outline.

The recurring silhouette is round: burr cores, sunburst rings, and spines radiating from a center. Spines are tapered quads, wide at a root buried 2px inside the core and 0.9px wide at the tip, each capped with a bead slightly lighter than the spine.

## Components

### Buttons
Tactile and lit, like small backlit keys.
- **Shape:** gently rounded (9px), 32px tall, 14px side padding (11px before a leading 14px icon), no border, 14px semibold label. No button, crumb or segment draws an outline; fill, gradient and glow carry the edge.
- **Cursor light:** every button, icon button, crumb, inactive chart-mode segment and the Update button carries a round white light under the cursor (a 52px disc with an 18px Gaussian blur: it peaks near two thirds of its alpha under the cursor, half that 25px away, a fifth at 40px) and clipped to the control's rounded shape. It fades with the hover tween and, when the pointer leaves, waits at the nearest edge while it fades. It is meant to be seen on the dark controls, about 22% white at the cursor: 34% on Secondary, Ghost and every quiet control. On Primary (20%) and Danger (18%) it only warms the color. Drawn by `fx::shine`, repainted by `fx::on_move`. The pie chart never carries it.
- **Primary:** an Ember gradient (Ember Hi at the top to Ember Lo at the bottom) with Ember Ink text and an Ember glow.
- **Danger:** the same construction in the Trash Red gradient, with white text.
- **Hover:** gradient stops lighten (+0.05 and +0.06 L) and the glow strength rises from 1.0 to 2.2. **Active:** the gradient flips (low at top) and the glow drops to 0.6.
- **Secondary:** the Stone gradient with a contact shadow. Used for Scan Folder…, buttons on drives that are not the lead, Stop, and Moving….
- **Ghost:** transparent; fades to Graphite Hover with text going from muted to full.
- **Icon button:** a 28px square (resized to 22–26px in dense spots) with 8px corners and a 15px icon. On hover it fades in a Raised fill. When pressed it shows Ember Soft.
- **Split button:** a button plus a chevron segment, with square inner corners and a 1px seam in the button's own ink (Ember Ink at 25% on primary, white at 12% otherwise). It holds Scan/Open on the left, and the chevron opens the disk menu.

### Popup Menu
- A Menu Ground panel at least 210px wide, with 10px corners, a white 12% hairline, and the Float shadow. It anchors to the top-right of its trigger and snaps 8px inside the window.
- Items are 26px tall with 6px corners. Hovering fills an item with Ember and Ember Ink. Disabled items are faint. Separators are 1px hairlines.
- Enter: a 140ms fade with a 6px→4px settle (ease-out quint).

### Sidebar Rows
- 50px tall with 11px corners. An 18px icon with no tile, in the row's chart color lightened +0.10 L (+0.08 more on hover), sits beside the name (14px medium) and a 3px share bar in the data color over `track`.
- The icon's shape says what the item is: folder, archive, video, program (apps and other packages), audio, image, document, or plain file; Hidden space gets the disc. The color always matches the item's slice in the chart.
- **Compact rows:** items under 0.1% of the list collapse to 26px lines with 8px corners: a 13px icon, a 13px regular muted name, the figure in 12px faint, no bar or percent.
- Hover tints the whole row with a horizontal gradient of its data color (16%→3%), with no border. Collected rows dim to 40%.

### Chart Modes
- A segmented switch in the explorer titlebar, left of Rescan: **Size**, **File Count**, **File Types**. A 2px-padded Graphite Surface well with 9px corners and no border; segments are 24px tall with 7px corners and 13px labels. The active segment wears the current-crumb style (Ember Soft fill, Ember Hi semibold text, no border); the others ease from muted to a Raised fill on hover.
- **Size:** slices by bytes on disk. **File Count:** slices and rows by files inside (folders themselves don't count); figures read "1,234 files" and the header leads with the count. **File Types:** three thicker rings over everything inside the focus folder: category, extension, then the biggest real items of each (apps count as one item). The sidebar lists every extension with its type faintly after the name; hovering a type in the chart lists its extensions, hovering an extension lists its files. Types and extensions open like folders (chart slice, sidebar row, or crumb): the path continues past the folder ("Downloads › Video › .mp4"), the chart redraws with fewer, thicker rings (never fewer than two ring widths), and the center, Back, Esc and the mouse's back button step out one level. An opened extension lists up to 200 of its biggest items, which can be collected, dragged and right-clicked.

### Full Disk Access
- Scanning the startup disk or a folder above the home folder without Full Disk Access first opens a step screen: the disk's unscanned burr (180px) beside a 26px bold heading, one muted paragraph, three numbered steps in 22px hairline circles, then Open System Settings (primary) and Scan Without Access (ghost). Once access is detected on window activation, the actions become a single Scan button and an Ember Hi shield line confirms it.
- On home, a single line under the startup disk's buttons (shield icon, muted sentence, an Ember Hi "Turn On…" link) stays until access is granted. In the explorer, a borderless Ember Soft panel tops the disk's root list; hover deepens its wash to 22% Ember.

### Updates
- **Update button:** in the home and explorer titlebars (left of Scan Folder… and of the chart modes), only while there is something to say. 28px tall, 9px corners, 13px semibold, ghost-like (muted, easing to Graphite Hover and text on hover; Raised while its card is open). A 7px status dot leads: Ember with a soft Ember halo while an update is available or installing, faint grey after a failure. Labels: "Update to 0.3.0", "Downloading… 42%", "Installing…", "Update didn't finish"; after a check from the macOS app menu, a faint, unclickable "Checking for updates…" and then a check mark with "Burrdisk is up to date" for 4 seconds. It fades in over 420ms.
- **Update card:** opens below the button, right-aligned, 328px wide: Menu Ground, 14px corners, white 12% hairline, a slightly deeper Float shadow (14px / 36px). It enters with a 220ms fade and an 8px settle. Inside, 16px padding and 14px gaps: a 30px Ember Soft well with the download icon in Ember Hi beside the 16px semibold "Burrdisk 0.3.0" and a faint "You have 0.2.0 · 4.7 MB download"; the release notes' first paragraph in 13px muted; a "What's New ↗" link styled like home's Turn On…; then faint-iconed 12px cautions that say what restarting costs (scans need to run again; on macOS, Full Disk Access must be turned on again, because the ad hoc signature changes); then Later (ghost) and Update and Restart (primary). Downloading swaps the buttons for a 6px Ember bar on `track` with its own soft glow, "Downloading 2.1 of 4.7 MB" and Cancel; installing fills the bar and says Burrdisk will reopen.
- **When it opens by itself:** once per new version, and only on the disk list. Found while scanning or exploring, the button appears without the card. Clicking outside the card puts it away like Later.
- **Errors** are neutral, never Trash Red: the reason as a muted sentence beside a faint X, with Try Again (primary) and Download Manually (ghost). A copy that can't replace itself (macOS App Translocation, a folder it can't write to, a Windows copy not installed by the installer) says why and offers Download, which opens the release page.

### Collection Tray
- An 80px bar. When empty, it shows a 48px dashed badge with the inbox icon and two lines of muted and faint guidance. Once items are collected, it shows an orange count badge with a glow that flares when the count changes, the total as an amount, a horizontally scrolling row of chips, and Move to Trash.
- **Chips:** 30px tall with 9px corners, a Raised→Surface gradient, a border in the item's data color at 35%, a 20px tinted icon tile, a 12px name (max 160px) and size, and a 22px X icon button. They enter with a 280ms fade and an 8px rise.
- **Countdown:** 5 seconds. The count badge turns Trash Red and shows the seconds left inside a white ring that drains clockwise. Each new number drops into place and the badge's halo flares. Stop replaces the action; Esc also stops it.
- **Collected items** drain to near-gray (saturation × 0.12) in the chart, the sidebar and the Collector chips.
- **Item menu:** right-click a row or chart slice for Expand (or Open), Preview (Space, macOS only), Show in Finder (File Explorer on Windows), Open in Terminal and Move to Collector (⌘⌫ / Ctrl+Backspace). Rows carry no hover buttons.

### Toast
- A Raised pill with 14px corners and a border in its tone at 45%. It has a glow and a 26px round icon well that holds the tone at 20%. It enters over 320ms rising 12px, and fades out over its last 0.5s (shown for 6s).

### Burr (signature)
- A data drawing, not decoration. The core is a filled Seed Core disc at 34% of the radius. When the disk has a cached scan, the core gets a 1.5px Ember rim.
- **Unscanned:** spines go around the full circle (84 on the lead disk, 40 on others). The used fraction is lit in Fullness color at full length with slight jitter. The free fraction is dim `hairline-strong` stubs at 34% length.
- **Scanned:** spines become the disk's top-level folders in their data colors, one cluster per folder. Cluster size is proportional to the folder's share, reach scales with √share, and each cluster bulges in its middle so neighboring folders read as separate lobes.
- **Motion:** spines grow out over 1300ms, sweeping clockwise (ease-out cubic). On hover the spines bristle, extending 7%. Clicking a burr scans or opens its disk.
- The lead burr carries the fill percent in its core (26px bold × scale). The wordmark carries a 20px, 14-spine, all-Ember burr: the brand mark, the one non-data burr.

### Disk Image Rows
- 40px rows with no resting container. On hover they fade in a Surface fill with 10px corners, the disc icon brightens within its own hue (each image gets one, 137.5° apart, so the fade never crosses other hues), and the chevron menu button goes from 35% to full opacity.

### Motion (shared)
- **Hover tween:** 130ms in and 260ms out, ease-out cubic, driven by `fx::hover`. Never use an instant swap on custom controls.
- **Rise:** home sections fade and lift 10px over 500ms, staggered 80ms.
- **Explorer:** chart zoom 620ms, reveal 900ms, center label 220ms (ease-out quint).

## Do's and Don'ts

### Do:
- **Do** take every non-grey color from the data wheel, Fullness, Ember, or Trash Red (The Data Owns Color Rule).
- **Do** build filled controls lit from above: a vertical hi→lo gradient, a same-hue glow that grows on hover, an inverted gradient when pressed, and the cursor light inside.
- **Do** put Ember Ink (`#1f0b00`), not black or white, on anything orange.
- **Do** ease hovers through `fx::hover` (130ms in / 260ms out) and enter new elements with a short fade-and-rise.
- **Do** keep every burr spine data-true: fill state before a scan, top-level folders after.
- **Do** write sizes as a bold number with a smaller muted unit on one baseline.

### Don't:
- **Don't** wrap home's disks in card containers. They sit on the ground, and rows get a fill only on hover.
- **Don't** replace a disk's burr with a ring gauge or a block of stat chips.
- **Don't** use Trash Red for anything but moving to the Trash, its countdown, or its errors.
- **Don't** cast neutral or hard-offset drop shadows under colored elements. Use the colored Glow.
- **Don't** outline buttons. A hairline around a control is the generic look Burrdisk avoids; separate with fill and light.
- **Don't** add a second typeface. Hierarchy comes from Mona Sans' weight and size.
- **Don't** add spines, rings, or colors that do not map to bytes on disk. The wordmark burr is the only ornamental burr.
