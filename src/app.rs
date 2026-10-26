use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    Bounds, Context, FocusHandle, PathPromptOptions, Pixels, Render, SharedString, Task,
    Window, actions, div, prelude::*,
};

use crate::scan::{self, NodeId, NodeKind, Progress, Tree};
use crate::sunburst::{self, Segment, Target, Zoom};
use crate::theme;
use crate::volumes::{self, Volume};

actions!(burrdisk, [GoUp, Quit]);

#[derive(Clone)]
pub struct ScanTarget {
    pub path: PathBuf,
    pub name: String,
    /// Set when scanning a whole volume; enables free/hidden space accounting.
    pub volume: Option<Volume>,
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
    pub collected: Vec<NodeId>,
    pub confirm_armed: bool,
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
            this.start_scan(ScanTarget { path, name, volume: None }, cx);
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

    /// When `path` was last scanned this session, if it was.
    pub fn scanned_at(&self, path: &std::path::Path) -> Option<Instant> {
        self.scans.get(path).map(|scan| scan.scanned_at)
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
            },
            cx,
        );
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
                this.open(ScanTarget { path, name, volume: None }, cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn start_scan(&mut self, target: ScanTarget, cx: &mut Context<Self>) {
        let progress = Arc::new(Progress::default());
        let job = {
            let progress = progress.clone();
            let path = target.path.clone();
            let name = target.name.clone();
            cx.background_executor()
                .spawn(async move { scan::scan(&path, &name, &progress) })
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
                                return;
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
            collected: Vec::new(),
            confirm_armed: false,
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
            self.scans.insert(
                explorer.target.path.clone(),
                CachedScan {
                    target: explorer.target,
                    tree: explorer.tree,
                    scanned_at: explorer.scanned_at,
                },
            );
        }
        self.home_since = Instant::now();
        self.refresh_volumes(cx);
        cx.notify();
    }

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
            Screen::Home => {}
            Screen::Scanning(_) => self.cancel_scan(cx),
            Screen::Explorer(explorer) => {
                if explorer.confirm_armed {
                    explorer.confirm_armed = false;
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
        explorer.confirm_armed = false;
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
                explorer.trashing = false;
                let text = match (failed.len(), reason) {
                    (0, _) => format!("Moved {} to the Trash", crate::format::bytes(freed)),
                    (n, Some(reason)) => {
                        let what = if n == 1 { failed[0].clone() } else { format!("{n} items") };
                        format!("Couldn't move {what} to the Trash: {reason}")
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

    pub fn arm_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(explorer) = self.explorer() else { return };
        explorer.confirm_armed = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            this.update(cx, |this, cx| {
                if let Some(explorer) = this.explorer() {
                    explorer.confirm_armed = false;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

impl Explorer {
    pub fn set_focus(&mut self, id: NodeId) {
        if self.tree.node(id).kind != NodeKind::Dir || id == self.focus {
            return;
        }
        self.zoom = Zoom::new(&self.tree, self.focus, id);
        self.focus = id;
        self.hover = None;
        self.prev_hover = None;
        self.preview = None;
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
        self.confirm_armed = false;
    }

    pub fn uncollect(&mut self, id: NodeId) {
        self.collected.retain(|c| *c != id);
        self.confirm_armed = false;
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
        items
            .into_iter()
            .map(|(id, path)| (id, trash::delete(&path).map_err(|error| error.to_string())))
            .collect()
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
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_family(".SystemUIFont")
            .child(content)
    }
}
