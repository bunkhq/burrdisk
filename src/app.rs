use std::cell::Cell;
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

pub struct Explorer {
    pub target: ScanTarget,
    pub tree: Tree,
    pub focus: NodeId,
    pub segments: Rc<Vec<Segment>>,
    pub hover: Option<Hover>,
    /// What was hovered before, kept briefly so highlights cross-fade.
    pub prev_hover: Option<Hover>,
    pub hover_since: Instant,
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
    pub notice: Option<SharedString>,
}

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

    pub fn scan_volume(&mut self, volume: Volume, cx: &mut Context<Self>) {
        self.start_scan(
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
                this.start_scan(ScanTarget { path, name, volume: None }, cx);
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
        let segments = Rc::new(sunburst::layout(&tree, Tree::ROOT));
        self.screen = Screen::Explorer(Explorer {
            target,
            tree,
            focus: Tree::ROOT,
            segments,
            hover: None,
            prev_hover: None,
            hover_since: Instant::now(),
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
        self.screen = Screen::Home;
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
        let job = cx.background_executor().spawn(async move {
            let mut context = trash::TrashContext::default();
            #[cfg(target_os = "macos")]
            {
                use trash::macos::{DeleteMethod, TrashContextExtMacos};
                context.set_delete_method(DeleteMethod::NsFileManager);
            }
            items
                .into_iter()
                .map(|(id, path)| (id, context.delete(&path).map_err(|e| e.to_string())))
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = job.await;
            this.update(cx, |this, cx| {
                let Some(explorer) = this.explorer() else { return };
                let mut freed = 0;
                let mut failed = Vec::new();
                for (id, result) in results {
                    match result {
                        Ok(()) => {
                            freed += explorer.tree.node(id).size;
                            explorer.collected.retain(|c| *c != id);
                            explorer.tree.remove(id);
                        }
                        Err(_) => failed.push(explorer.tree.node(id).name.to_string()),
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
                explorer.notice = Some(if failed.is_empty() {
                    format!("Moved {} to the Trash", crate::format::bytes(freed)).into()
                } else {
                    format!("Couldn't move {} to the Trash", failed.join(", ")).into()
                });
                explorer.relayout();
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
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
        self.relayout();
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
        self.notice = None;
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
