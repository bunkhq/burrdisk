use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use gpui::{
    Bounds, Context, FocusHandle, MouseButton, NavigationDirection, PathPromptOptions, Pixels, Point,
    Render, SharedString, Subscription, Task, Window, actions, div, prelude::*,
};

use crate::filetype::Breakdown;
use crate::scan::{self, NodeId, NodeKind, Progress, Tree};
use crate::sunburst::{self, Measure, Segment, Target, Zoom};
use crate::theme;
use crate::update::{self, Release};
use crate::volumes::{self, Volume};

actions!(burrdisk, [GoUp, GoBack, GoForward, Preview, Collect, Quit, CheckForUpdates]);

#[derive(Clone)]
pub struct ScanTarget {
    pub path: PathBuf,
    pub name: String,
    /// Set when scanning a whole volume; enables free/hidden space accounting.
    pub volume: Option<Volume>,
    /// Scan as root through an authorized helper process, to see folders the user can't.
    pub admin: bool,
}

pub enum Screen {
    Home,
    /// Asks for Full Disk Access before a scan that would otherwise miss protected folders.
    Access(ScanTarget),
    Scanning(Scanning),
    Explorer(Explorer),
}

/// How the chart divides the folder in focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChartMode {
    /// Folders and files sized by the space they take.
    Size,
    /// Folders and files sized by how many files they hold.
    Count,
    /// Everything inside, grouped by file type and extension.
    Types,
}

/// Where an update stands.
pub enum UpdatePhase {
    Idle,
    /// A check the user asked for, from the app menu.
    Checking,
    /// A check the user asked for found nothing newer; shown briefly.
    UpToDate(Instant),
    Available,
    Downloading(Arc<update::Progress>),
    /// Unpacking and swapping in the new version, just before the restart.
    Installing,
    Failed(String),
}

pub struct Updater {
    pub phase: UpdatePhase,
    /// The newer release, once one is found.
    pub release: Option<Release>,
    /// Whether the update card is open below the titlebar's Update button.
    pub open: bool,
    /// The version the card last opened for by itself, so it does so once.
    prompted: Option<update::Version>,
    /// When the Update button appeared, for its entrance.
    pub since: Instant,
    _checks: Option<Task<()>>,
    _job: Option<Task<()>>,
}

pub struct Scanning {
    pub target: ScanTarget,
    pub progress: Arc<Progress>,
    pub started: Instant,
    _task: Task<()>,
}

pub struct CachedScan {
    pub target: ScanTarget,
    pub tree: Tree,
    pub scanned_at: Instant,
    /// The chart's top ring, kept so the home screen can draw the disk's burr.
    pub lobes: Vec<Lobe>,
}

/// One top-level slice of a scanned disk: its span in turns and chart color.
#[derive(Clone, Copy)]
pub struct Lobe {
    pub start: f32,
    pub end: f32,
    pub color: gpui::Hsla,
}

pub struct Explorer {
    pub target: ScanTarget,
    pub scanned_at: Instant,
    pub tree: Tree,
    pub focus: NodeId,
    pub segments: Rc<Vec<Segment>>,
    pub hover: Option<Hover>,
    /// What was hovered before, kept briefly so highlights cross-fade.
    pub prev_hover: Option<Hover>,
    pub hover_since: Instant,
    /// Folder the sidebar shows while the chart is hovered, instead of the focus.
    pub preview: Option<NodeId>,
    pub chart_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// What a drag starting on the chart would carry, captured at mouse-down.
    pub chart_drag: ChartDrag,
    /// Mouse is held down on the chart; hover stays frozen until release.
    pub pressing: bool,
    /// When the chart first appeared, for its entrance sweep.
    pub revealed: Instant,
    /// Camera move between a folder and its parent or child, while it runs.
    pub zoom: Option<Zoom>,
    /// Deepest folder visited along the current path; Forward walks back down toward it.
    pub trail: NodeId,
    /// Item whose right-click menu is open, and where the click was.
    pub context: Option<(NodeId, Point<Pixels>)>,
    pub collected: Vec<NodeId>,
    /// The Collector tile being dragged and its preview, whose spot on the shelf
    /// shows as an empty slot for as long as the preview lives.
    pub shelf_drag: Option<(NodeId, gpui::AnyWeakView)>,
    /// How far the shelf is scrolled, where it's easing to, and when it last moved.
    pub shelf_scroll: f32,
    pub shelf_target: f32,
    pub shelf_moved: Instant,
    /// Until when the shelf keeps scrolling to its end, so a new tile is in view.
    pub shelf_follow: Option<Instant>,
    /// The shelf's visible width and its tiles' width; set while painting.
    pub shelf_widths: Rc<Cell<(f32, f32)>>,
    /// Where the tray sits, so a dragged tile knows whether it's still over it.
    pub tray_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// When "Move to Trash" was pressed; the move starts once the countdown runs out.
    pub countdown: Option<Instant>,
    pub trashing: bool,
    /// The move to the Trash under way, or just finished and about to close.
    pub trash_job: Option<TrashJob>,
    /// Keeps the item being moved in view in the manifest.
    pub manifest_scroll: gpui::ScrollHandle,
    pub notice: Option<Notice>,
    pub mode: ChartMode,
    /// The focus folder grouped by file type, while the chart shows types.
    pub breakdown: Option<Rc<Breakdown>>,
    /// Type or extension the sidebar shows while the type chart is hovered.
    pub group_preview: Option<u32>,
    /// Type or extension the type chart is opened into, like a folder.
    pub group_focus: Option<u32>,
}

/// One item in a move to the Trash, as it was when the move began.
pub struct TrashItem {
    pub name: String,
    /// The folder it's in.
    pub place: String,
    pub size: u64,
    pub kind: NodeKind,
    pub color: Option<gpui::Hsla>,
}

/// Where each item of a move to the Trash stands, shared with the thread moving them.
pub const TRASH_WAITING: u8 = 0;
pub const TRASH_MOVING: u8 = 1;
pub const TRASH_MOVED: u8 = 2;
pub const TRASH_FAILED: u8 = 3;

pub struct TrashJob {
    pub items: Vec<TrashItem>,
    pub states: Arc<Vec<AtomicU8>>,
    /// The progress bar's eased share of bytes processed.
    pub shown: f32,
    pub finished: Option<Instant>,
}

/// How long the finished list stays up before it closes and the result shows.
pub const TRASH_LINGER: Duration = Duration::from_millis(900);

impl TrashJob {
    pub fn state(&self, ix: usize) -> u8 {
        self.states[ix].load(Ordering::Relaxed)
    }

    pub fn total(&self) -> u64 {
        self.items.iter().map(|i| i.size).sum()
    }

    /// Items and bytes dealt with so far, moved or not.
    pub fn processed(&self) -> (usize, u64) {
        self.items
            .iter()
            .enumerate()
            .filter(|(ix, _)| self.state(*ix) >= TRASH_MOVED)
            .fold((0, 0), |(n, bytes), (_, item)| (n + 1, bytes + item.size))
    }

    pub fn current(&self) -> Option<&TrashItem> {
        self.items.iter().enumerate().find(|(ix, _)| self.state(*ix) == TRASH_MOVING).map(|(_, item)| item)
    }
}

/// A path as people write it, without the `\\?\` prefix a canonical Windows path carries.
pub fn display_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
}

/// The explorer sidebar's starting width.
pub const SIDEBAR_WIDTH: f32 = 360.0;

/// A short message shown above the tray, e.g. the result of moving items to the Trash.
#[derive(Clone)]
pub struct Notice {
    pub text: SharedString,
    pub error: bool,
    pub shown: Instant,
}

pub const NOTICE_DURATION: Duration = Duration::from_secs(6);
/// Grace period between pressing "Move to Trash" and anything being moved.
pub const TRASH_COUNTDOWN: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hover {
    Center,
    Target(Target),
}

/// Payload carried when dragging a file or folder toward the collection tray.
#[derive(Clone)]
pub struct DragItem {
    pub id: NodeId,
    pub name: SharedString,
    pub size: u64,
    pub kind: NodeKind,
}

/// Payload carried when dragging a tile on the Collector's shelf: back out to put it
/// back, or onto another tile to reorder.
#[derive(Clone)]
pub struct ShelfItem(pub DragItem);

/// Drag payload for the chart. The chart is one element, so the item under the
/// cursor is resolved when the button goes down rather than at render time.
#[derive(Clone, Default)]
pub struct ChartDrag(pub Rc<std::cell::RefCell<Option<DragItem>>>);

pub struct Burrdisk {
    pub focus_handle: FocusHandle,
    pub volumes: Vec<Volume>,
    pub volumes_loaded: bool,
    pub show_all_images: bool,
    /// When "Show all" was last toggled, for the expand/collapse animation.
    pub images_toggled: Option<Instant>,
    /// Finished scans kept for this session, by root path.
    pub scans: HashMap<PathBuf, CachedScan>,
    /// When the disk list was last shown, for its entrance animation.
    pub home_since: Instant,
    pub screen: Screen,
    /// Mount point whose actions menu is open on the home screen.
    pub menu: Option<PathBuf>,
    /// Whether macOS lets Burrdisk read protected folders; checked again on activation.
    pub full_disk_access: bool,
    /// Width of the explorer's sidebar, as last dragged.
    pub sidebar_width: f32,
    /// The user chose to scan without Full Disk Access this session.
    pub access_skipped: bool,
    /// When the access step was opened, for its entrance.
    pub access_since: Instant,
    pub updater: Updater,
    _refresh: Option<Task<()>>,
    _activation: Option<Subscription>,
}

/// How long "Up to date" stays in the titlebar after a check from the menu.
const UP_TO_DATE_FOR: Duration = Duration::from_secs(4);

pub const HOVER_FADE: Duration = Duration::from_millis(160);

impl Burrdisk {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let mut this = Self {
            focus_handle,
            volumes: Vec::new(),
            volumes_loaded: false,
            show_all_images: false,
            images_toggled: None,
            scans: HashMap::new(),
            home_since: Instant::now(),
            screen: Screen::Home,
            menu: None,
            full_disk_access: volumes::has_full_disk_access(),
            sidebar_width: SIDEBAR_WIDTH,
            access_skipped: false,
            access_since: Instant::now(),
            updater: Updater {
                phase: UpdatePhase::Idle,
                release: None,
                open: false,
                prompted: None,
                since: Instant::now(),
                _checks: None,
                _job: None,
            },
            _refresh: None,
            _activation: None,
        };
        // Coming back from System Settings is the moment access may have changed.
        this._activation = Some(cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                let granted = volumes::has_full_disk_access();
                if granted != this.full_disk_access {
                    this.full_disk_access = granted;
                    cx.notify();
                }
            }
        }));
        this.refresh_volumes(cx);
        this.start_update_checks(cx);
        if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
            let path = path.canonicalize().unwrap_or(path);
            if let Some(volume) = volumes::list().into_iter().find(|v| v.mount == path) {
                this.scan_volume(volume, cx);
                return this;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            this.start_scan(ScanTarget { path, name, volume: None, admin: false }, cx);
        }
        this
    }

    pub fn refresh_volumes(&mut self, cx: &mut Context<Self>) {
        let list = cx.background_executor().spawn(async { volumes::list() });
        self._refresh = Some(cx.spawn(async move |this, cx| {
            let volumes = list.await;
            this.update(cx, |this, cx| {
                this.volumes = volumes;
                this.volumes_loaded = true;
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn scanned_as_admin(&self, path: &std::path::Path) -> bool {
        self.scans.get(path).is_some_and(|scan| scan.target.admin)
    }

    /// Opens a previous scan of the target if there is one, otherwise scans it.
    pub fn open(&mut self, target: ScanTarget, cx: &mut Context<Self>) {
        match self.scans.remove(&target.path) {
            Some(scan) => self.open_explorer(scan.target, scan.tree, scan.scanned_at, cx),
            None => self.start_scan(target, cx),
        }
    }

    pub fn scan_volume(&mut self, volume: Volume, cx: &mut Context<Self>) {
        self.open(
            ScanTarget {
                path: volume.mount.clone(),
                name: volume.name.clone(),
                volume: Some(volume),
                admin: false,
            },
            cx,
        );
    }

    /// Scans a volume from scratch, dropping any cached result, with the latest free space figures.
    pub fn rescan_volume(&mut self, volume: Volume, admin: bool, cx: &mut Context<Self>) {
        self.menu = None;
        self.scans.remove(&volume.mount);
        let volume = volumes::list().into_iter().find(|v| v.mount == volume.mount).unwrap_or(volume);
        self.start_scan(
            ScanTarget {
                path: volume.mount.clone(),
                name: volume.name.clone(),
                volume: Some(volume),
                admin,
            },
            cx,
        );
    }

    pub fn forget_scan(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        self.menu = None;
        self.scans.remove(path);
        cx.notify();
    }

    pub fn toggle_menu(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.menu = if self.menu.as_ref() == Some(&path) { None } else { Some(path) };
        cx.notify();
    }

    pub fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    pub fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Scan".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            this.update(cx, |this, cx| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                this.open(ScanTarget { path, name, volume: None, admin: false }, cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn start_scan(&mut self, target: ScanTarget, cx: &mut Context<Self>) {
        self.menu = None;
        if !self.full_disk_access && !self.access_skipped && volumes::needs_full_disk_access(&target.path) {
            self.access_since = Instant::now();
            self.screen = Screen::Access(target);
            cx.notify();
            return;
        }
        let progress = Arc::new(Progress::default());
        // An admin scan runs in a root helper that reports through files next to its output.
        let helper = target.admin.then(scan::HelperFiles::new);
        let job = {
            let progress = progress.clone();
            let path = target.path.clone();
            let name = target.name.clone();
            let helper = helper.clone();
            cx.background_executor().spawn(async move {
                match helper {
                    Some(files) => scan::scan_as_admin(&path, &name, &files),
                    None => scan::scan(&path, &name, &progress),
                }
            })
        };

        // Repaint progress while the walk runs, then hand the tree to the explorer.
        let ticker = {
            let progress = progress.clone();
            cx.spawn(async move |this, cx| {
                let mut job = std::pin::pin!(job);
                loop {
                    let tick = cx.background_executor().timer(Duration::from_millis(80));
                    match futures_select(&mut job, tick).await {
                        Some(result) => {
                            this.update(cx, |this, cx| this.finish_scan(result, cx)).ok();
                            return;
                        }
                        None => {
                            if progress.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                                if let Some(files) = &helper {
                                    files.cancel();
                                }
                                return;
                            }
                            if let Some(files) = &helper {
                                files.read_progress(&progress);
                            }
                            if this.update(cx, |_, cx| cx.notify()).is_err() {
                                return;
                            }
                        }
                    }
                }
            })
        };

        self.screen = Screen::Scanning(Scanning {
            target,
            progress,
            started: Instant::now(),
            _task: ticker,
        });
        cx.notify();
    }

    fn finish_scan(&mut self, tree: Option<Tree>, cx: &mut Context<Self>) {
        let Screen::Scanning(scanning) = &self.screen else { return };
        let Some(mut tree) = tree else {
            self.go_home(cx);
            return;
        };
        let target = scanning.target.clone();
        if let Some(volume) = &target.volume {
            let scanned = tree.node(Tree::ROOT).size;
            let hidden = volume.used().saturating_sub(scanned);
            if hidden > volume.used() / 100 {
                tree.add_hidden_space(hidden);
            }
        }
        self.open_explorer(target, tree, Instant::now(), cx);
    }

    /// Scans past the Full Disk Access step, for the rest of the session.
    pub fn scan_without_access(&mut self, cx: &mut Context<Self>) {
        if let Screen::Access(target) = &self.screen {
            let target = target.clone();
            self.access_skipped = true;
            self.start_scan(target, cx);
        }
    }

    fn open_explorer(&mut self, target: ScanTarget, tree: Tree, scanned_at: Instant, cx: &mut Context<Self>) {
        let segments = Rc::new(sunburst::layout(&tree, Tree::ROOT, Measure::Size));
        self.screen = Screen::Explorer(Explorer {
            target,
            scanned_at,
            tree,
            focus: Tree::ROOT,
            segments,
            hover: None,
            prev_hover: None,
            hover_since: Instant::now(),
            preview: None,
            chart_bounds: Rc::default(),
            chart_drag: ChartDrag::default(),
            pressing: false,
            revealed: Instant::now(),
            zoom: None,
            trail: Tree::ROOT,
            context: None,
            collected: Vec::new(),
            shelf_drag: None,
            shelf_scroll: 0.,
            shelf_target: 0.,
            shelf_moved: Instant::now(),
            shelf_follow: None,
            shelf_widths: Rc::default(),
            tray_bounds: Rc::default(),
            countdown: None,
            trashing: false,
            trash_job: None,
            manifest_scroll: gpui::ScrollHandle::new(),
            notice: None,
            mode: ChartMode::Size,
            breakdown: None,
            group_preview: None,
            group_focus: None,
        });
        cx.notify();
    }

    pub fn cancel_scan(&mut self, cx: &mut Context<Self>) {
        if let Screen::Scanning(scanning) = &self.screen {
            scanning.progress.cancel();
        }
        self.go_home(cx);
    }

    pub fn go_home(&mut self, cx: &mut Context<Self>) {
        // Keep the finished scan so returning to it this session is instant.
        if let Screen::Explorer(explorer) = std::mem::replace(&mut self.screen, Screen::Home) {
            let lobes = sunburst::layout(&explorer.tree, Tree::ROOT, Measure::Size)
                .into_iter()
                .filter(|s| s.depth == 0)
                .map(|s| Lobe { start: s.start, end: s.end, color: s.color })
                .collect();
            self.scans.insert(
                explorer.target.path.clone(),
                CachedScan {
                    target: explorer.target,
                    tree: explorer.tree,
                    scanned_at: explorer.scanned_at,
                    lobes,
                },
            );
        }
        self.home_since = Instant::now();
        self.refresh_volumes(cx);
        self.prompt_update();
        cx.notify();
    }

    /// Looks for a newer release a few seconds after launch, then every few hours.
    fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        if !update::enabled() {
            return;
        }
        update::clean_up();
        self.updater._checks = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            loop {
                let found = cx.background_executor().spawn(async { update::check() }).await;
                let ok = this.update(cx, |this, cx| this.update_checked(found, false, cx)).is_ok();
                if !ok {
                    break;
                }
                cx.background_executor().timer(update::CHECK_EVERY).await;
            }
        }));
    }

    /// Checks now, from the app menu, and says so even when there's nothing new.
    pub fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if !update::enabled() || !matches!(self.updater.phase, UpdatePhase::Idle | UpdatePhase::UpToDate(_)) {
            // Busy or already found: show what's there.
            self.updater.open = self.updater.release.is_some();
            cx.notify();
            return;
        }
        self.updater.phase = UpdatePhase::Checking;
        self.updater.since = Instant::now();
        cx.notify();
        self.updater._job = Some(cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async { update::check() }).await;
            this.update(cx, |this, cx| this.update_checked(found, true, cx)).ok();
        }));
    }

    fn update_checked(&mut self, found: anyhow::Result<Option<Release>>, manual: bool, cx: &mut Context<Self>) {
        let busy = matches!(self.updater.phase, UpdatePhase::Downloading(_) | UpdatePhase::Installing);
        if busy {
            return;
        }
        match found {
            Ok(Some(release)) => {
                let new = self.updater.release.as_ref().is_none_or(|r| r.version != release.version);
                if new || matches!(self.updater.phase, UpdatePhase::Checking) {
                    self.updater.phase = UpdatePhase::Available;
                }
                if new {
                    self.updater.since = Instant::now();
                }
                self.updater.release = Some(release);
                if manual {
                    self.updater.open = true;
                } else {
                    self.prompt_update();
                }
            }
            Ok(None) if manual => {
                self.updater.phase = UpdatePhase::UpToDate(Instant::now());
                self.updater.since = Instant::now();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(UP_TO_DATE_FOR).await;
                    this.update(cx, |this, cx| {
                        if let UpdatePhase::UpToDate(at) = this.updater.phase
                            && at.elapsed() >= UP_TO_DATE_FOR
                        {
                            this.updater.phase = UpdatePhase::Idle;
                            cx.notify();
                        }
                    })
                    .ok();
                })
                .detach();
            }
            // Background checks fail quietly; the next one may work.
            Ok(None) => {}
            Err(err) if manual => self.updater.phase = UpdatePhase::Failed(format!("{err:#}")),
            Err(_) => {}
        }
        cx.notify();
    }

    /// Opens the update card by itself once per version, and only on the disk
    /// list, never over a scan or the explorer.
    fn prompt_update(&mut self) {
        let Some(release) = &self.updater.release else { return };
        if matches!(self.screen, Screen::Home) && self.updater.prompted != Some(release.version) {
            self.updater.prompted = Some(release.version);
            self.updater.open = true;
        }
    }

    pub fn toggle_update_card(&mut self, cx: &mut Context<Self>) {
        self.updater.open = !self.updater.open;
        cx.notify();
    }

    pub fn close_update_card(&mut self, cx: &mut Context<Self>) {
        self.updater.open = false;
        cx.notify();
    }

    /// Downloads, checks and installs the update, then restarts into it.
    pub fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some(release) = self.updater.release.clone() else { return };
        let Some(asset) = release.asset.clone() else { return };
        let progress = Arc::new(update::Progress::default());
        progress.total.store(asset.size, std::sync::atomic::Ordering::Relaxed);
        self.updater.phase = UpdatePhase::Downloading(progress.clone());
        cx.notify();
        self.updater._job = Some(cx.spawn(async move |this, cx| {
            let job = cx.background_executor().spawn({
                let progress = progress.clone();
                async move { update::download(&asset, progress) }
            });
            // Moves the progress bar along while the download runs.
            let downloaded = loop {
                let tick = cx.background_executor().timer(Duration::from_millis(100));
                if progress.done.load(std::sync::atomic::Ordering::Relaxed)
                    >= progress.total.load(std::sync::atomic::Ordering::Relaxed)
                    || progress.cancel.load(std::sync::atomic::Ordering::Relaxed)
                {
                    break job.await;
                }
                tick.await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            };
            let installed = match downloaded {
                Ok(file) => {
                    let ok = this.update(cx, |this, cx| {
                        this.updater.phase = UpdatePhase::Installing;
                        cx.notify();
                    });
                    if ok.is_err() {
                        return;
                    }
                    let release = release.clone();
                    cx.background_executor().spawn(async move { update::install(&release, &file) }).await
                }
                Err(err) => Err(err),
            };
            this.update(cx, |this, cx| match installed.and_then(|relaunch| relaunch.run()) {
                Ok(()) => cx.quit(),
                Err(_) if progress.cancel.load(std::sync::atomic::Ordering::Relaxed) => {
                    this.updater.phase = UpdatePhase::Available;
                    cx.notify();
                }
                Err(err) => {
                    this.updater.phase = UpdatePhase::Failed(format!("{err:#}"));
                    this.updater.open = true;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    pub fn cancel_update(&mut self, cx: &mut Context<Self>) {
        if let UpdatePhase::Downloading(progress) = &self.updater.phase {
            progress.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        cx.notify();
    }

    /// Whether restarting now would throw something away: a scan underway or kept,
    /// or Trash work in flight.
    pub fn update_loses_scans(&self) -> bool {
        !self.scans.is_empty() || !matches!(self.screen, Screen::Home | Screen::Access(_))
    }

    pub fn update_blocked_by_trash(&self) -> bool {
        matches!(&self.screen, Screen::Explorer(e) if e.countdown.is_some() || e.trashing)
    }

    /// Rescans whatever the explorer is showing, the same way it was scanned before.
    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        if let Screen::Explorer(explorer) = &self.screen {
            let mut target = explorer.target.clone();
            if let Some(volume) = &target.volume {
                // Pick up the latest free space figures.
                if let Some(fresh) = volumes::list().into_iter().find(|v| v.mount == volume.mount) {
                    target.volume = Some(fresh);
                }
            }
            self.start_scan(target, cx);
        }
    }

    fn go_up(&mut self, _: &GoUp, _window: &mut Window, cx: &mut Context<Self>) {
        match &mut self.screen {
            Screen::Home => self.close_menu(cx),
            Screen::Access(_) => self.go_home(cx),
            Screen::Scanning(_) => self.cancel_scan(cx),
            Screen::Explorer(explorer) => {
                if explorer.context.is_some() {
                    explorer.context = None;
                    cx.notify();
                } else if explorer.countdown.is_some() {
                    explorer.countdown = None;
                    cx.notify();
                } else if explorer.group_up() {
                    cx.notify();
                } else if let Some(parent) = explorer.tree.node(explorer.focus).parent {
                    explorer.set_focus(parent);
                    cx.notify();
                } else {
                    self.go_home(cx);
                }
            }
        }
    }

    /// Steps up to the parent folder; from the disk's top folder, back to the disk list.
    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        if matches!(self.screen, Screen::Access(_)) {
            self.go_home(cx);
            return;
        }
        let Some(explorer) = self.explorer() else { return };
        if explorer.group_up() {
            cx.notify();
            return;
        }
        match explorer.tree.node(explorer.focus).parent {
            Some(parent) => {
                explorer.set_focus(parent);
                cx.notify();
            }
            None => self.go_home(cx),
        }
    }

    /// Steps one folder down, toward where Back came from, or into a lone subfolder.
    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        let Some(explorer) = self.explorer() else { return };
        if let Some(next) = explorer.next_folder() {
            explorer.set_focus(next);
            cx.notify();
        }
    }

    fn on_back(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        self.go_back(cx);
    }

    fn on_forward(&mut self, _: &GoForward, _: &mut Window, cx: &mut Context<Self>) {
        self.go_forward(cx);
    }

    /// The item Space and ⌘⌫ act on: the one with its menu open, else the one hovered.
    fn keyboard_target(&mut self) -> Option<NodeId> {
        let explorer = self.explorer()?;
        explorer.context.map(|(id, _)| id).or_else(|| explorer.hovered_node())
    }

    fn on_preview(&mut self, _: &Preview, _: &mut Window, _: &mut Context<Self>) {
        if let Some(id) = self.keyboard_target()
            && let Some(explorer) = self.explorer()
        {
            quick_look(&explorer.tree.path(id));
        }
    }

    fn on_collect(&mut self, _: &Collect, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.keyboard_target()
            && let Some(explorer) = self.explorer()
        {
            explorer.collect(id);
            explorer.context = None;
            cx.notify();
        }
    }

    pub fn explorer(&mut self) -> Option<&mut Explorer> {
        match &mut self.screen {
            Screen::Explorer(explorer) => Some(explorer),
            _ => None,
        }
    }

    pub fn move_collected_to_trash(&mut self, cx: &mut Context<Self>) {
        let Some(explorer) = self.explorer() else { return };
        if explorer.collected.is_empty() || explorer.trashing {
            return;
        }
        explorer.trashing = true;
        explorer.countdown = None;
        let items: Vec<(NodeId, PathBuf)> = explorer
            .collected
            .iter()
            .map(|id| (*id, explorer.tree.path(*id)))
            .collect();
        let states: Arc<Vec<AtomicU8>> = Arc::new(items.iter().map(|_| AtomicU8::new(TRASH_WAITING)).collect());
        explorer.trash_job = Some(TrashJob {
            items: items
                .iter()
                .map(|(id, path)| {
                    let node = explorer.tree.node(*id);
                    TrashItem {
                        name: node.name.to_string(),
                        place: path.parent().map(display_path).unwrap_or_default(),
                        size: node.size,
                        kind: node.kind,
                        color: explorer.segment_color(*id),
                    }
                })
                .collect(),
            states: states.clone(),
            shown: 0.,
            finished: None,
        });
        let job = cx.background_executor().spawn(async move { move_to_trash(items, &states) });
        cx.spawn(async move |this, cx| {
            let results = job.await;
            this.update(cx, |this, cx| {
                let Some(explorer) = this.explorer() else { return };
                let mut freed = 0;
                let mut failed = Vec::new();
                let mut reason = None;
                for (id, result) in results {
                    match result {
                        Ok(()) => {
                            freed += explorer.tree.node(id).size;
                            explorer.collected.retain(|c| *c != id);
                            explorer.tree.remove(id);
                        }
                        Err(error) => {
                            failed.push(explorer.tree.node(id).name.to_string());
                            reason.get_or_insert(error);
                        }
                    }
                }
                // Never leave the view focused on something that no longer exists.
                if !explorer.tree.is_attached(explorer.focus) {
                    explorer.focus = Tree::ROOT;
                    explorer.zoom = None;
                }
                if !explorer.tree.is_attached(explorer.trail) {
                    explorer.trail = explorer.focus;
                }
                explorer.trashing = false;
                if let Some(job) = &mut explorer.trash_job {
                    job.finished = Some(Instant::now());
                }
                let text = match (failed.len(), reason) {
                    (0, _) => format!("Moved {} to the {}", crate::format::bytes(freed), crate::platform::TRASH),
                    (n, Some(reason)) => {
                        let what = if n == 1 { failed[0].clone() } else { format!("{n} items") };
                        format!("Couldn't move {what} to the {}: {reason}", crate::platform::TRASH)
                    }
                    _ => unreachable!(),
                };
                explorer.zoom = None;
                explorer.relayout();
                cx.notify();
                // The result shows once the list of what moved has closed.
                let error = !failed.is_empty();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(TRASH_LINGER).await;
                    this.update(cx, |this, cx| this.show_notice(text, error, cx)).ok();
                })
                .detach();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn show_notice(&mut self, text: String, error: bool, cx: &mut Context<Self>) {
        let Some(explorer) = self.explorer() else { return };
        let shown = Instant::now();
        explorer.notice = Some(Notice { text: text.into(), error, shown });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_DURATION).await;
            this.update(cx, |this, cx| {
                if let Some(explorer) = this.explorer()
                    && explorer.notice.as_ref().is_some_and(|n| n.shown == shown)
                {
                    explorer.notice = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Starts the grace period before the collected items move to the Trash.
    pub fn start_trash_countdown(&mut self, cx: &mut Context<Self>) {
        let Some(explorer) = self.explorer() else { return };
        if explorer.collected.is_empty() || explorer.trashing {
            return;
        }
        let started = Instant::now();
        explorer.countdown = Some(started);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TRASH_COUNTDOWN).await;
            this.update(cx, |this, cx| {
                // Only the countdown that is still running gets to fire.
                if this.explorer().is_some_and(|e| e.countdown == Some(started)) {
                    this.move_collected_to_trash(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn stop_trash_countdown(&mut self, cx: &mut Context<Self>) {
        if let Some(explorer) = self.explorer() {
            explorer.countdown = None;
            cx.notify();
        }
    }
}

impl Explorer {
    /// The focused folder's single subfolder, when there is exactly one.
    pub fn only_subfolder(&self) -> Option<NodeId> {
        let mut dirs = self
            .tree
            .node(self.focus)
            .children
            .iter()
            .copied()
            .filter(|c| self.tree.node(*c).kind == NodeKind::Dir);
        let first = dirs.next()?;
        dirs.next().is_none().then_some(first)
    }

    /// The folder Forward opens: the next one down the trail, else a lone subfolder.
    pub fn next_folder(&self) -> Option<NodeId> {
        if self.group_focus.is_some() {
            return None;
        }
        if self.trail != self.focus && self.tree.is_ancestor(self.focus, self.trail) {
            let lineage = self.tree.lineage(self.trail);
            let at = lineage.iter().position(|id| *id == self.focus)?;
            return lineage.get(at + 1).copied();
        }
        self.only_subfolder()
    }

    pub fn measure(&self) -> Measure {
        match self.mode {
            ChartMode::Count => Measure::Count,
            _ => Measure::Size,
        }
    }

    /// Rings in the current chart.
    pub fn rings(&self) -> usize {
        match (self.mode, &self.breakdown, self.group_focus) {
            // An opened type has fewer levels below it; keep at least two ring widths.
            (ChartMode::Types, Some(breakdown), Some(g)) => (sunburst::TYPE_RINGS - 1 - breakdown.level(g)).max(2),
            (ChartMode::Types, ..) => sunburst::TYPE_RINGS,
            _ => sunburst::RINGS,
        }
    }

    /// The breakdown group the type chart starts from.
    pub fn group_root(&self) -> u32 {
        self.group_focus.unwrap_or(Breakdown::ROOT)
    }

    /// Opens a file type or extension in the type chart, like opening a folder.
    /// `None` returns to the whole folder.
    pub fn open_group(&mut self, group: Option<u32>) {
        let group = group.filter(|g| *g != Breakdown::ROOT);
        if group == self.group_focus {
            return;
        }
        self.group_focus = group;
        self.hover = None;
        self.prev_hover = None;
        self.group_preview = None;
        self.context = None;
        self.revealed = Instant::now();
        self.layout_groups();
    }

    /// Steps the type chart out to the enclosing type, or back to the whole folder.
    /// Returns false when no type is open.
    pub fn group_up(&mut self) -> bool {
        let Some(g) = self.group_focus else { return false };
        let parent = self.breakdown.as_ref().and_then(|b| b.group(g).parent);
        self.open_group(parent);
        true
    }

    fn layout_groups(&mut self) {
        if let Some(breakdown) = &self.breakdown {
            self.segments = Rc::new(sunburst::layout_types(&self.tree, breakdown, self.group_root()));
        }
    }

    pub fn set_mode(&mut self, mode: ChartMode) {
        if mode == self.mode {
            return;
        }
        self.mode = mode;
        self.zoom = None;
        self.hover = None;
        self.prev_hover = None;
        self.preview = None;
        self.group_preview = None;
        self.group_focus = None;
        self.context = None;
        self.revealed = Instant::now();
        self.relayout();
    }

    pub fn set_focus(&mut self, id: NodeId) {
        if self.tree.node(id).kind != NodeKind::Dir || id == self.focus {
            return;
        }
        self.group_focus = None;
        if self.mode == ChartMode::Types {
            // The type chart is rebuilt for the new folder and sweeps in again.
            self.zoom = None;
            self.revealed = Instant::now();
        } else {
            self.zoom = Zoom::new(&self.tree, self.focus, id, self.measure());
        }
        self.focus = id;
        // Going up keeps the trail; going anywhere off it starts a new one.
        if !self.tree.is_ancestor(id, self.trail) {
            self.trail = id;
        }
        self.hover = None;
        self.prev_hover = None;
        self.preview = None;
        self.group_preview = None;
        self.context = None;
        self.relayout();
    }

    /// Hover coming from the chart also previews the hovered folder in the sidebar
    /// (or, for a file, the folder it's in).
    pub fn set_chart_hover(&mut self, hover: Option<Hover>) {
        self.set_hover(hover);
        if let Some(breakdown) = &self.breakdown {
            // Types list their extensions, extensions their biggest items.
            self.group_preview = match hover {
                Some(Hover::Target(Target::Group(g) | Target::GroupRest(g))) => Some(g),
                Some(Hover::Target(Target::Node(id))) => breakdown.group_of_file(id),
                _ => None,
            }
            .filter(|g| *g != self.group_root());
            return;
        }
        let folder = match hover {
            Some(Hover::Target(Target::Node(id))) if self.tree.node(id).kind == NodeKind::Dir => Some(id),
            Some(Hover::Target(Target::Node(id))) => self.tree.node(id).parent,
            Some(Hover::Target(Target::Smaller(parent))) => Some(parent),
            _ => None,
        };
        self.preview = folder.filter(|f| *f != self.focus);
    }

    /// The folder whose contents the sidebar lists.
    pub fn listed(&self) -> NodeId {
        self.preview.unwrap_or(self.focus)
    }

    pub fn set_hover(&mut self, hover: Option<Hover>) {
        if hover == self.hover {
            return;
        }
        self.prev_hover = self.hover;
        self.hover = hover;
        self.hover_since = Instant::now();
    }

    /// 0..1 progress of the latest hover change.
    pub fn hover_progress(&self) -> f32 {
        sunburst::ease_out_cubic((self.hover_since.elapsed().as_secs_f32() / HOVER_FADE.as_secs_f32()).min(1.0))
    }

    /// Whether the chart still needs frames. Clears a finished zoom as a side effect.
    pub fn tick(&mut self) -> bool {
        if self.zoom.as_ref().is_some_and(|z| z.finished()) {
            self.zoom = None;
        }
        let shelf_moving = self.tick_shelf();
        let trash_moving = self.tick_trash();
        self.zoom.is_some()
            || shelf_moving
            || trash_moving
            || self.countdown.is_some()
            || self.revealed.elapsed() < sunburst::REVEAL_DURATION
            || self.hover_since.elapsed() < HOVER_FADE
    }

    pub fn relayout(&mut self) {
        // Group numbers belong to one breakdown; a rebuilt one invalidates them.
        self.group_preview = None;
        if self.mode == ChartMode::Types {
            // Keep an opened type open across a rebuild (after a move to the Trash), if it still exists.
            let opened = self.breakdown.as_ref().zip(self.group_focus).map(|(old, g)| {
                let group = old.group(g);
                let ext = (old.level(g) == 1).then(|| group.name.clone());
                (group.category, ext)
            });
            let breakdown = Breakdown::new(&self.tree, self.focus);
            self.group_focus = opened.and_then(|(category, ext)| {
                breakdown.find(category, ext.as_deref()).or_else(|| breakdown.find(category, None))
            });
            self.breakdown = Some(Rc::new(breakdown));
            self.layout_groups();
        } else {
            self.breakdown = None;
            self.group_focus = None;
            self.segments = Rc::new(sunburst::layout(&self.tree, self.focus, self.measure()));
        }
    }

    /// Adds an item to the tray, keeping the set free of nested duplicates.
    pub fn collect(&mut self, id: NodeId) {
        if self.tree.node(id).kind == NodeKind::Hidden || id == Tree::ROOT {
            return;
        }
        if self.collected.iter().any(|c| self.tree.is_ancestor(*c, id)) {
            return;
        }
        let tree = &self.tree;
        self.collected.retain(|c| !tree.is_ancestor(id, *c));
        self.collected.push(id);
        // Bring the new tile into view.
        if self.shelf_scroll == self.shelf_target {
            self.shelf_moved = Instant::now();
        }
        self.shelf_follow = Some(Instant::now() + Duration::from_millis(400));
        // Changing the pile mid-countdown stops it, so nothing moves that wasn't reviewed.
        self.countdown = None;
    }

    pub fn uncollect(&mut self, id: NodeId) {
        self.collected.retain(|c| *c != id);
        self.countdown = None;
    }

    /// Tile being dragged off the shelf, while its drag lasts.
    pub fn shelf_dragging(&self) -> Option<NodeId> {
        self.shelf_drag.as_ref().filter(|(_, view)| view.upgrade().is_some()).map(|(id, _)| *id)
    }

    /// How far the shelf can scroll.
    pub fn shelf_room(&self) -> f32 {
        let (visible, content) = self.shelf_widths.get();
        (content - visible).max(0.)
    }

    pub fn scroll_shelf(&mut self, by: f32) {
        if self.shelf_scroll == self.shelf_target {
            // Starting from rest; don't count the idle time as a frame.
            self.shelf_moved = Instant::now();
        }
        self.shelf_follow = None;
        self.shelf_target = (self.shelf_target + by).clamp(0., self.shelf_room());
    }

    /// Eases the move's progress bar, and closes the list once it has lingered.
    fn tick_trash(&mut self) -> bool {
        let Some(job) = &mut self.trash_job else { return false };
        if job.finished.is_some_and(|at| at.elapsed() >= TRASH_LINGER) {
            self.trash_job = None;
            return false;
        }
        let total = job.total().max(1);
        let target = if job.finished.is_some() { 1. } else { job.processed().1 as f32 / total as f32 };
        job.shown += (target - job.shown) * 0.18;
        true
    }

    /// Eases the shelf toward where it's headed. Whether it's still moving.
    fn tick_shelf(&mut self) -> bool {
        if self.shelf_follow.is_some_and(|until| Instant::now() < until) {
            self.shelf_target = self.shelf_room();
        } else {
            self.shelf_follow = None;
        }
        self.shelf_target = self.shelf_target.clamp(0., self.shelf_room());
        let dt = self.shelf_moved.elapsed().as_secs_f32().min(0.05);
        self.shelf_moved = Instant::now();
        let gap = self.shelf_target - self.shelf_scroll;
        self.shelf_scroll = if gap.abs() < 0.5 { self.shelf_target } else { self.shelf_target - gap * (-dt / 0.08).exp() };
        self.shelf_scroll != self.shelf_target || self.shelf_follow.is_some()
    }

    /// Moves a collected item to where `before` sits on the shelf, or to the end.
    pub fn move_collected(&mut self, id: NodeId, before: Option<NodeId>) {
        if before == Some(id) || !self.collected.contains(&id) {
            return;
        }
        self.collected.retain(|c| *c != id);
        let at = before.and_then(|b| self.collected.iter().position(|c| *c == b)).unwrap_or(self.collected.len());
        self.collected.insert(at, id);
    }

    pub fn is_collected(&self, id: NodeId) -> bool {
        self.collected.iter().any(|c| self.tree.is_ancestor(*c, id))
    }

    pub fn collected_size(&self) -> u64 {
        self.collected.iter().map(|id| self.tree.node(*id).size).sum()
    }

    pub fn drag_item(&self, id: NodeId) -> Option<DragItem> {
        let node = self.tree.node(id);
        (node.kind != NodeKind::Hidden).then(|| DragItem {
            id,
            name: node.name.to_string().into(),
            size: node.size,
            kind: node.kind,
        })
    }

    pub fn hovered_node(&self) -> Option<NodeId> {
        match self.hover {
            Some(Hover::Target(Target::Node(id))) => Some(id),
            _ => None,
        }
    }

    pub fn segment_color(&self, id: NodeId) -> Option<gpui::Hsla> {
        self.segments
            .iter()
            .find(|s| s.target == Target::Node(id))
            .map(|s| s.color)
    }
}

/// Opens a Quick Look panel for a file or folder, as Space does in Finder.
/// Windows has no Quick Look, so there Preview isn't offered at all.
pub fn quick_look(path: &std::path::Path) {
    #[cfg(windows)]
    let _ = path;
    #[cfg(not(windows))]
    std::process::Command::new("qlmanage")
        .arg("-p")
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok();
}

/// Opens Terminal in a folder, or in the folder holding a file.
pub fn open_in_terminal(path: &std::path::Path) {
    let dir = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x10;
        // Windows Terminal if it's installed, else a plain console.
        if std::process::Command::new("wt.exe").arg("-d").arg(dir).spawn().is_err() {
            std::process::Command::new("cmd.exe")
                .current_dir(dir)
                .creation_flags(CREATE_NEW_CONSOLE)
                .spawn()
                .ok();
        }
    }
    #[cfg(not(windows))]
    std::process::Command::new("open").args(["-a", "Terminal"]).arg(dir).spawn().ok();
}

/// Moves files and folders to the Trash. The Foundation API is tried first; whatever
/// it refuses (it does when ~/.Trash has the wrong owner, among other cases) is
/// handed to Finder in one batch, which is what a drag to the Trash would do.
/// Moves each item to the Trash, marking its spot in `states` as it goes.
fn move_to_trash(items: Vec<(NodeId, PathBuf)>, states: &Arc<Vec<AtomicU8>>) -> Vec<(NodeId, Result<(), String>)> {
    let mark = |ix: usize, state: u8| states[ix].store(state, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut context = trash::TrashContext::default();
        context.set_delete_method(DeleteMethod::NsFileManager);
        let (mut done, refused): (Vec<_>, Vec<_>) = items
            .into_iter()
            .enumerate()
            .map(|(ix, (id, path))| {
                mark(ix, TRASH_MOVING);
                let ok = context.delete(&path).is_ok();
                // Refused items wait for Finder to have a go.
                mark(ix, if ok { TRASH_MOVED } else { TRASH_WAITING });
                (ix, id, path, ok)
            })
            .partition(|(_, _, _, ok)| *ok);
        let mut results: Vec<(NodeId, Result<(), String>)> = done.drain(..).map(|(_, id, _, _)| (id, Ok(()))).collect();
        if !refused.is_empty() {
            context.set_delete_method(DeleteMethod::Finder);
            for (ix, ..) in &refused {
                mark(*ix, TRASH_MOVING);
            }
            let paths: Vec<&PathBuf> = refused.iter().map(|(_, _, path, _)| path).collect();
            let outcome = context.delete_all(paths).map_err(|error| match error {
                trash::Error::Unknown { description } => description
                    .rsplit_once("failed: ")
                    .map_or(description.clone(), |(_, tail)| tail.to_string()),
                other => other.to_string(),
            });
            for (ix, id, path, _) in refused {
                // Finder works on the batch as a whole; check what actually left.
                let result = match &outcome {
                    Ok(()) => Ok(()),
                    Err(_) if !path.exists() => Ok(()),
                    Err(error) => Err(error.clone()),
                };
                mark(ix, if result.is_ok() { TRASH_MOVED } else { TRASH_FAILED });
                results.push((id, result));
            }
        }
        results
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = mark;
        let ids: Vec<NodeId> = items.iter().map(|(id, _)| *id).collect();
        let states = states.clone();
        let run = move || -> Vec<(NodeId, Result<(), String>)> {
            items
                .into_iter()
                .enumerate()
                .map(|(ix, (id, path))| {
                    states[ix].store(TRASH_MOVING, Ordering::Relaxed);
                    let result = trash::delete(&path).map_err(|error| error.to_string());
                    states[ix].store(if result.is_ok() { TRASH_MOVED } else { TRASH_FAILED }, Ordering::Relaxed);
                    (id, result)
                })
                .collect()
        };
        // On Windows, `trash` sets up single-threaded COM and panics on a thread that
        // already runs multithreaded COM, as the thread pool behind background tasks
        // does. A thread of its own always starts clean.
        if cfg!(windows) {
            std::thread::spawn(run).join().unwrap_or_else(|_| {
                ids.into_iter().map(|id| (id, Err("the system refused".to_string()))).collect()
            })
        } else {
            run()
        }
    }
}

/// Resolves to `Some` when `job` finishes first, or `None` when `tick` does.
async fn futures_select<T>(
    job: &mut std::pin::Pin<&mut impl std::future::Future<Output = T>>,
    tick: Task<()>,
) -> Option<T> {
    use std::future::Future;
    use std::task::Poll;
    let mut tick = std::pin::pin!(tick);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(value) = job.as_mut().poll(cx) {
            return Poll::Ready(Some(value));
        }
        if tick.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}

impl Render for Burrdisk {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.screen {
            Screen::Home => crate::ui::home::render(self, window, cx).into_any_element(),
            Screen::Access(_) => crate::ui::access::render(self, window, cx).into_any_element(),
            Screen::Scanning(_) => crate::ui::scanning::render(self, window, cx).into_any_element(),
            Screen::Explorer(_) => crate::ui::explorer::render(self, window, cx).into_any_element(),
        };
        if crate::ui::fx::animating() {
            window.request_animation_frame();
        }
        div()
            .id("root")
            .key_context("Burrdisk")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::go_up))
            .on_action(cx.listener(Self::on_back))
            .on_action(cx.listener(Self::on_forward))
            .on_action(cx.listener(Self::on_preview))
            .on_action(cx.listener(Self::on_collect))
            .on_action(cx.listener(|this, _: &CheckForUpdates, _, cx| this.check_for_updates(cx)))
            // The mouse's side buttons (and two-finger swipes) step back and forward.
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, _, cx| this.go_back(cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, _, cx| this.go_forward(cx)),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_family("Mona Sans")
            .child(content)
    }
}
