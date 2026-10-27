use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    Bounds, Context, FocusHandle, PathPromptOptions, Pixels, Point, Render, SharedString, Task,
    Window, actions, div, prelude::*,
};

use crate::scan::{self, NodeId, NodeKind, Progress, Tree};
use crate::sunburst::{self, Segment, Target, Zoom};
use crate::theme;
use crate::volumes::{self, Volume};

actions!(burrdisk, [GoUp, GoBack, GoForward, Preview, Collect, Quit]);

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
    Scanning(Scanning),
    Explorer(Explorer),
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
    /// When "Move to Trash" was pressed; the move starts once the countdown runs out.
    pub countdown: Option<Instant>,
    pub trashing: bool,
    pub notice: Option<Notice>,
}

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
    _refresh: Option<Task<()>>,
}

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
            _refresh: None,
        };
        this.refresh_volumes(cx);
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

    fn open_explorer(&mut self, target: ScanTarget, tree: Tree, scanned_at: Instant, cx: &mut Context<Self>) {
        let segments = Rc::new(sunburst::layout(&tree, Tree::ROOT));
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
            countdown: None,
            trashing: false,
            notice: None,
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
            let lobes = sunburst::layout(&explorer.tree, Tree::ROOT)
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
        cx.notify();
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
            Screen::Scanning(_) => self.cancel_scan(cx),
            Screen::Explorer(explorer) => {
                if explorer.context.is_some() {
                    explorer.context = None;
                    cx.notify();
                } else if explorer.countdown.is_some() {
                    explorer.countdown = None;
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
        let Some(explorer) = self.explorer() else { return };
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
        let job = cx.background_executor().spawn(async move { move_to_trash(items) });
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
                let lineage = explorer.tree.lineage(explorer.focus);
                let detached = lineage
                    .windows(2)
                    .any(|w| !explorer.tree.node(w[0]).children.contains(&w[1]));
                if detached {
                    explorer.focus = Tree::ROOT;
                    explorer.zoom = None;
                }
                if !explorer.tree.is_attached(explorer.trail) {
                    explorer.trail = explorer.focus;
                }
                explorer.trashing = false;
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
                this.show_notice(text, !failed.is_empty(), cx);
                cx.notify();
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
        if self.trail != self.focus && self.tree.is_ancestor(self.focus, self.trail) {
            let lineage = self.tree.lineage(self.trail);
            let at = lineage.iter().position(|id| *id == self.focus)?;
            return lineage.get(at + 1).copied();
        }
        self.only_subfolder()
    }

    pub fn set_focus(&mut self, id: NodeId) {
        if self.tree.node(id).kind != NodeKind::Dir || id == self.focus {
            return;
        }
        self.zoom = Zoom::new(&self.tree, self.focus, id);
        self.focus = id;
        // Going up keeps the trail; going anywhere off it starts a new one.
        if !self.tree.is_ancestor(id, self.trail) {
            self.trail = id;
        }
        self.hover = None;
        self.prev_hover = None;
        self.preview = None;
        self.context = None;
        self.relayout();
    }

    /// Hover coming from the chart also previews the hovered folder in the sidebar
    /// (or, for a file, the folder it's in).
    pub fn set_chart_hover(&mut self, hover: Option<Hover>) {
        self.set_hover(hover);
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
        self.zoom.is_some()
            || self.countdown.is_some()
            || self.revealed.elapsed() < sunburst::REVEAL_DURATION
            || self.hover_since.elapsed() < HOVER_FADE
    }

    pub fn relayout(&mut self) {
        self.segments = Rc::new(sunburst::layout(&self.tree, self.focus));
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
        // Changing the pile mid-countdown stops it, so nothing moves that wasn't reviewed.
        self.countdown = None;
    }

    pub fn uncollect(&mut self, id: NodeId) {
        self.collected.retain(|c| *c != id);
        self.countdown = None;
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
fn move_to_trash(items: Vec<(NodeId, PathBuf)>) -> Vec<(NodeId, Result<(), String>)> {
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut context = trash::TrashContext::default();
        context.set_delete_method(DeleteMethod::NsFileManager);
        let (mut done, refused): (Vec<_>, Vec<_>) = items
            .into_iter()
            .map(|(id, path)| {
                let ok = context.delete(&path).is_ok();
                (id, path, ok)
            })
            .partition(|(_, _, ok)| *ok);
        let mut results: Vec<(NodeId, Result<(), String>)> = done.drain(..).map(|(id, _, _)| (id, Ok(()))).collect();
        if !refused.is_empty() {
            context.set_delete_method(DeleteMethod::Finder);
            let paths: Vec<&PathBuf> = refused.iter().map(|(_, path, _)| path).collect();
            let outcome = context.delete_all(paths).map_err(|error| match error {
                trash::Error::Unknown { description } => description
                    .rsplit_once("failed: ")
                    .map_or(description.clone(), |(_, tail)| tail.to_string()),
                other => other.to_string(),
            });
            for (id, path, _) in refused {
                // Finder works on the batch as a whole; check what actually left.
                let result = match &outcome {
                    Ok(()) => Ok(()),
                    Err(_) if !path.exists() => Ok(()),
                    Err(error) => Err(error.clone()),
                };
                results.push((id, result));
            }
        }
        results
    }
    #[cfg(not(target_os = "macos"))]
    {
        let ids: Vec<NodeId> = items.iter().map(|(id, _)| *id).collect();
        let run = move || -> Vec<(NodeId, Result<(), String>)> {
            items
                .into_iter()
                .map(|(id, path)| (id, trash::delete(&path).map_err(|error| error.to_string())))
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
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_family("Mona Sans")
            .child(content)
    }
}
