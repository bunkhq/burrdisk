use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Context, ElementId, FontWeight, Hsla, IntoElement,
    Bounds, MouseButton, MouseDownEvent, MouseMoveEvent, ScrollWheelEvent, SharedString, Pixels, Point, Render, RenderImage, Window, WindowControlArea,
    canvas, div, ease_out_quint, img, linear_color_stop, linear_gradient, prelude::*, relative,
};
use crate::ui::px;

use crate::appicon;
use crate::app::{
    self, Burrdisk, ChartDrag, ChartMode, DragItem, Explorer, Hover, NOTICE_DURATION, Notice, Screen, ShelfItem,
    TRASH_COUNTDOWN,
};
use crate::filetype::{self, Breakdown, Category};
use crate::format;
use crate::platform;
use crate::scan::{NodeId, NodeKind, Tree};
use crate::sunburst::{self, Geometry, Hit, PaintState, Segment, Target, View};
use crate::theme;
use crate::ui::components::{
    ButtonStyle, Icon, MenuItem, amount, button, context_menu, glow, icon, icon_button, icon_label_button, shadow,
    window_controls,
};
use crate::ui::fx;
use crate::volumes;

/// Rows listed in the sidebar before the rest are summarized
const MAX_ROWS: usize = 200;
/// Narrowest the sidebar can be dragged, and the least room it leaves the chart
const SIDEBAR_MIN: f32 = 280.0;
const CHART_MIN: f32 = 360.0;

/// Drag payload for the sidebar's resize edge
#[derive(Clone)]
struct SidebarResize;

/// What follows the pointer while resizing: nothing
struct NoPreview;

impl Render for NoPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
/// Rows that animate in when a folder opens; the rest appear immediately
const ANIMATED_ROWS: usize = 18;

/// An item's chart color, drained toward gray once it sits in the Collector
fn row_color(explorer: &Explorer, id: NodeId) -> Hsla {
    let color = explorer
        .segment_color(id)
        .unwrap_or_else(|| gpui::hsla(0.07, 0.05, 0.40, 1.0));
    if explorer.is_collected(id) { pending(color) } else { color }
}

fn pending(color: Hsla) -> Hsla {
    Hsla { s: color.s * 0.12, l: color.l * 0.85, ..color }
}

fn format_percent(ratio: f32) -> String {
    match ratio * 100.0 {
        p if p >= 10.0 => format!("{p:.0}%"),
        p if p >= 0.1 => format!("{p:.1}%"),
        _ => "<0.1%".into(),
    }
}

pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let full_disk_access = app.full_disk_access;
    // Keep the chart some room however the window is sized
    let sidebar_width =
        app.sidebar_width.min(f32::from(window.viewport_size().width) / crate::ui::UI_SCALE - CHART_MIN).max(SIDEBAR_MIN);
    let update = crate::ui::update::button_and_card(app, window, cx);
    let Screen::Explorer(explorer) = &mut app.screen else {
        return div().into_any_element();
    };
    if explorer.tick() {
        window.request_animation_frame();
    }
    let explorer = &*explorer;
    if explorer.notice.is_some() {
        // Keeps the toast's fade-out moving
        window.request_animation_frame();
    }

    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .on_drop(cx.listener(|this, item: &ShelfItem, _, cx| {
            if let Some(explorer) = this.explorer() {
                explorer.uncollect(item.0.id);
                cx.notify();
            }
        }))
        .on_drag_move(cx.listener(|this, event: &gpui::DragMoveEvent<SidebarResize>, window, cx| {
            // In design units, which `px` scales up on Windows
            let viewport = f32::from(window.viewport_size().width) / crate::ui::UI_SCALE;
            let width = viewport - f32::from(event.event.position.x) / crate::ui::UI_SCALE;
            this.sidebar_width = width.clamp(SIDEBAR_MIN, (viewport - CHART_MIN).max(SIDEBAR_MIN));
            cx.notify();
        }))
        .child(titlebar(explorer, update, window, cx))
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(chart(explorer, cx))
                .child(sidebar(explorer, full_disk_access, sidebar_width, cx)),
        )
        .child(tray(explorer, cx))
        .children(manifest(explorer, cx))
        .children(explorer.notice.clone().map(toast))
        .children(item_menu(explorer, window, cx))
        .into_any_element()
}

/// Opens the right-click menu for `id` where the mouse went down
fn open_item_menu(this: &mut Burrdisk, id: NodeId, event: &MouseDownEvent, cx: &mut Context<Burrdisk>) {
    let Some(explorer) = this.explorer() else { return };
    if explorer.tree.node(id).kind == NodeKind::Hidden {
        return;
    }
    explorer.context = Some((id, event.position));
    cx.notify();
}

fn item_menu(explorer: &Explorer, window: &Window, cx: &mut Context<Burrdisk>) -> Option<AnyElement> {
    let (id, position) = explorer.context?;
    let node = explorer.tree.node(id);
    let name = node.name.to_string();
    let path = explorer.tree.path(id);
    let entity = cx.entity();
    let on_explorer = |f: fn(&mut Explorer, NodeId)| {
        let entity = entity.clone();
        move |_: &mut Window, cx: &mut gpui::App| {
            entity.update(cx, |this, cx| {
                if let Some(explorer) = this.explorer() {
                    f(explorer, id);
                    cx.notify();
                }
            })
        }
    };
    let mut items = Vec::new();
    if node.kind == NodeKind::Dir {
        items.push(MenuItem::new(format!("Expand \u{201c}{name}\u{201d}"), on_explorer(Explorer::set_focus)));
    } else {
        let open = path.clone();
        items.push(MenuItem::new("Open", move |_, cx| cx.open_with_system(&open)));
    }
    let (preview, reveal, terminal) = (path.clone(), path.clone(), path);
    if cfg!(not(windows)) {
        items.push(MenuItem::new("Preview", move |_, _| app::quick_look(&preview)).shortcut("Space"));
    }
    items.push(MenuItem::new(format!("Show in {}", platform::FILE_MANAGER), move |_, cx| cx.reveal_path(&reveal)));
    items.push(MenuItem::new("Open in Terminal", move |_, _| app::open_in_terminal(&terminal)));
    items.push(
        MenuItem::new(format!("Move \u{201c}{name}\u{201d} to Collector"), on_explorer(Explorer::collect))
            .shortcut(platform::COLLECT_KEYS)
            .disabled(explorer.is_collected(id)),
    );
    Some(context_menu(&format!("item-{id}"), items, position, window, move |_, cx| {
        entity.update(cx, |this, cx| {
            if let Some(explorer) = this.explorer() {
                explorer.context = None;
                cx.notify();
            }
        })
    }))
}

/// A pill floating above the tray with the result of the last Trash operation
fn toast(notice: Notice) -> AnyElement {
    let tone: Hsla = if notice.error { theme::danger().into() } else { gpui::hsla(0.40, 0.6, 0.55, 1.0) };
    let glyph = if notice.error { Icon::X } else { Icon::Trash };
    let left = NOTICE_DURATION.as_secs_f32() - notice.shown.elapsed().as_secs_f32();
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(96.))
        .flex()
        .justify_center()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2p5()
                .max_w(px(640.))
                .min_w_0()
                .pl_2()
                .pr_4()
                .py_2()
                .rounded(px(14.))
                .bg(theme::raised())
                .shadow(glow(tone, 1.2))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(26.))
                        .rounded_full()
                        .bg(tone.opacity(0.2))
                        .child(icon(glyph, 13.0, tone)),
                )
                .child(div().text_sm().min_w_0().child(notice.text.clone())),
        )
        .with_animation(
            ElementId::Name(format!("toast-{:?}", notice.shown).into()),
            Animation::new(Duration::from_millis(320)).with_easing(ease_out_quint()),
            move |el, t| {
                // Fade back out over the last half second
                let out = (left / 0.5).clamp(0.0, 1.0);
                el.opacity(t * out).mb(px(-12. * (1. - t)))
            },
        )
        .into_any_element()
}

fn titlebar(explorer: &Explorer, update: Option<AnyElement>, window: &Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let lineage = explorer.tree.lineage(explorer.focus);
    // An opened file type continues the path past the folder: "Video › .mp4"
    let groups: Vec<(u32, String)> = match (explorer.breakdown.as_deref(), explorer.group_focus) {
        (Some(breakdown), Some(g)) => {
            let mut chain = vec![g];
            while let Some(parent) = breakdown.group(*chain.last().unwrap()).parent.filter(|p| *p != Breakdown::ROOT) {
                chain.push(parent);
            }
            chain.into_iter().rev().map(|g| (g, breakdown.group(g).name.clone())).collect()
        }
        _ => Vec::new(),
    };
    let crumbs: Vec<(Option<NodeId>, Option<u32>, String)> = lineage
        .iter()
        .map(|id| (Some(*id), None, explorer.tree.node(*id).name.to_string()))
        .chain(groups.into_iter().map(|(g, name)| (None, Some(g), name)))
        .collect();
    let last = crumbs.len() - 1;
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .h(px(56.))
        .pl(px(platform::TITLEBAR_INSET + 16.))
        .pr(px(platform::TITLEBAR_END))
        .border_b_1()
        .border_color(theme::border())
        .window_control_area(WindowControlArea::Drag)
        // Keeps the root's focus-on-click from claiming presses here, which would stop
        // Windows from moving (or, on double-click, maximizing) the window
        .occlude()
        .child(
            icon_button(
                "back",
                Icon::ChevronLeft,
                cx.listener(|this, _, _, cx| this.go_back(cx)),
            )
            .occlude(),
        )
        .child({
            let enabled = explorer.next_folder().is_some();
            icon_button(
                "forward",
                Icon::ChevronRight,
                cx.listener(|this, _, _, cx| this.go_forward(cx)),
            )
            .when(!enabled, |s| s.opacity(0.3).cursor_default())
            .mr_1()
            .occlude()
        })
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_0p5()
                .overflow_hidden()
                .child({
                    // The root of every path: the disk list
                    let t = fx::hover("crumb-home");
                    div()
                        .id("crumb-home")
                        .on_hover(fx::on_hover("crumb-home"))
                        .on_mouse_move(fx::on_move)
                        .relative()
                        .flex_none()
                        .px_2p5()
                        .py_1()
                        .rounded(px(7.))
                        .child(fx::shine(t, gpui::white().opacity(0.34), px(7.)))
                        .text_sm()
                        .whitespace_nowrap()
                        .text_color(fx::mix(theme::text_muted(), theme::text(), t))
                        .bg(fx::mix(gpui::transparent_black(), theme::raised(), t))
                        .cursor_pointer()
                        .occlude()
                        .on_click(cx.listener(|this, _, _, cx| this.go_home(cx)))
                        .child("Disks and Folders")
                })
                .children(crumbs.into_iter().enumerate().flat_map(|(ix, (id, group, name))| {
                    let is_last = ix == last;
                    let t = fx::hover(("crumb", ix));
                    let crumb = div()
                        .id(("crumb", ix))
                        .on_hover(fx::on_hover(("crumb", ix)))
                        .on_mouse_move(fx::on_move)
                        .relative()
                        .px_2p5()
                        .py_1()
                        .rounded(px(7.))
                        .text_sm()
                        .flex_shrink()
                        .min_w(px(36.))
                        .max_w(px(220.))
                        .whitespace_nowrap()
                        .truncate()
                        .when(is_last, |s| {
                            s.text_color(theme::accent_hi())
                                .font_weight(FontWeight::SEMIBOLD)
                                .bg(theme::accent_soft())
                        })
                        .when(!is_last, |s| {
                            s.text_color(fx::mix(theme::text_muted(), theme::text(), t))
                                .bg(fx::mix(gpui::transparent_black(), theme::raised(), t))
                                .cursor_pointer()
                                .child(fx::shine(t, gpui::white().opacity(0.34), px(7.)))
                        })
                        .occlude()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(explorer) = this.explorer() {
                                match id {
                                    // The open folder's own crumb closes any opened type
                                    Some(id) if id == explorer.focus => explorer.open_group(None),
                                    Some(id) => explorer.set_focus(id),
                                    None => explorer.open_group(group),
                                }
                                cx.notify();
                            }
                        }))
                        .child(name)
                        .into_any_element();
                    let separator = icon(Icon::ChevronRight, 12.0, theme::text_faint()).into_any_element();
                    [separator, crumb]
                })),
        )
        .children(update.map(|u| div().mr(px(8.)).child(u)))
        .child(mode_switch(explorer, cx))
        .child(
            icon_button(
                "rescan",
                Icon::Refresh,
                cx.listener(|this, _, _, cx| this.rescan(cx)),
            )
            .occlude(),
        )
        .when(cfg!(windows), |s| s.child(div().w(px(8.))).child(window_controls(window)))
}

/// Chart modes, as the switch in the titlebar names them
const MODES: [(ChartMode, &str); 3] = [
    (ChartMode::Size, "Size"),
    (ChartMode::Count, "File Count"),
    (ChartMode::Types, "File Types"),
];

/// A segmented switch for what the chart's slices stand for
fn mode_switch(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .mr_2()
        .rounded(px(9.))
        .bg(theme::surface())
        .children(MODES.iter().enumerate().map(|(ix, &(mode, label))| {
            let on = explorer.mode == mode;
            let t = fx::hover(("mode", ix));
            div()
                .id(("mode", ix))
                .on_hover(fx::on_hover(("mode", ix)))
                .on_mouse_move(fx::on_move)
                .relative()
                .flex()
                .items_center()
                .h(px(24.))
                .px_2p5()
                .rounded(px(7.))
                .text_size(px(13.))
                .whitespace_nowrap()
                .when(on, |s| {
                    s.text_color(theme::accent_hi())
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(theme::accent_soft())
                })
                .when(!on, |s| {
                    s.text_color(fx::mix(theme::text_muted(), theme::text(), t))
                        .bg(fx::mix(gpui::transparent_black(), theme::raised(), t))
                        .cursor_pointer()
                        .child(fx::shine(t, gpui::white().opacity(0.34), px(7.)))
                })
                // Without this, Windows treats a press here as one on the titlebar's
                // drag area and the click never arrives
                .occlude()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(explorer) = this.explorer() {
                        explorer.set_mode(mode);
                        cx.notify();
                    }
                }))
                .child(label)
        }))
}

fn files(n: u64) -> String {
    if n == 1 { "1 file".into() } else { format!("{} files", format::count(n)) }
}

/// An item's figure in the current mode: how much space it takes, or how many files it holds
fn figure(explorer: &Explorer, id: NodeId) -> String {
    let node = explorer.tree.node(id);
    match explorer.mode {
        ChartMode::Count => files(node.items),
        _ => format::bytes(node.size),
    }
}

/// The icon for an item, by what kind of file it is
fn item_icon(name: &str, kind: NodeKind) -> Icon {
    category_icon(filetype::category(name, kind))
}

fn category_icon(category: Category) -> Icon {
    match category {
        Category::Folder => Icon::Folder,
        Category::Archive => Icon::Archive,
        Category::Video => Icon::Video,
        Category::Program => Icon::AppWindow,
        Category::Audio => Icon::Music,
        Category::Image => Icon::Image,
        Category::Document => Icon::FileText,
        Category::Other => Icon::File,
        Category::Hidden => Icon::Disc,
    }
}

/// A breakdown group's own name: ".mp4 files" for an extension, "Video" for a category
fn group_name(breakdown: &Breakdown, g: u32) -> String {
    let group = breakdown.group(g);
    match group.parent {
        Some(parent) if parent != Breakdown::ROOT => {
            if group.name.starts_with('.') { format!("{} files", group.name) } else { "Files without an extension".into() }
        }
        _ => group.name.clone(),
    }
}

fn chart(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let bounds_cell = explorer.chart_bounds.clone();
    let rings = explorer.rings();
    let (segments, view) = match &explorer.zoom {
        Some(zoom) => (zoom.segments.clone(), zoom.view()),
        None => (explorer.segments.clone(), View::revealing(explorer.revealed)),
    };
    let collected: Vec<bool> = segments
        .iter()
        .map(|s| match s.target {
            Target::Node(id) => explorer.is_collected(id),
            Target::Smaller(parent) => explorer.is_collected(parent),
            Target::Group(_) | Target::GroupRest(_) => false,
        })
        .collect();
    let (highlight, dim, center_hover) = hover_weights(explorer, &segments);

    // What the center disc describes: the hovered item, or the folder in focus
    let breakdown = explorer.breakdown.as_deref();
    let (label_name, label_value, label_hint): (String, String, String) = match (explorer.hover, breakdown) {
        (Some(Hover::Target(Target::Node(id))), _) => {
            let node = explorer.tree.node(id);
            let hint = if breakdown.is_some() { hint_for(NodeKind::File) } else { hint_for(node.kind) };
            (node.name.to_string(), figure(explorer, id), hint.into())
        }
        (Some(Hover::Target(Target::Smaller(parent))), _) => {
            let (count, value) = smaller_summary(explorer, parent);
            (format!("{} smaller items", format::count(count as u64)), value, String::new())
        }
        (Some(Hover::Target(Target::Group(g))), Some(breakdown)) => {
            let group = breakdown.group(g);
            (group_name(breakdown, g), format::bytes(group.size), format!("{} \u{b7} Click to open", files(group.count)))
        }
        (Some(Hover::Target(Target::GroupRest(g))), Some(breakdown)) => {
            let (count, size, what) = group_rest(explorer, breakdown, g);
            (format!("{} smaller {what}", format::count(count)), format::bytes(size), String::new())
        }
        (Some(Hover::Center), Some(breakdown)) if explorer.group_focus.is_some() => {
            let g = explorer.group_root();
            let back = match breakdown.group(g).parent {
                Some(parent) if parent != Breakdown::ROOT => group_name(breakdown, parent),
                _ => explorer.tree.node(explorer.focus).name.to_string(),
            };
            (format!("Back to {back}"), format::bytes(breakdown.group(g).size), String::new())
        }
        (None, Some(breakdown)) if explorer.group_focus.is_some() => {
            let g = explorer.group_root();
            let group = breakdown.group(g);
            (group_name(breakdown, g), format::bytes(group.size), files(group.count))
        }
        (Some(Hover::Center), _) if explorer.focus != Tree::ROOT => {
            let parent = explorer.tree.node(explorer.focus).parent.unwrap_or(Tree::ROOT);
            (format!("Back to {}", explorer.tree.node(parent).name), figure(explorer, explorer.focus), String::new())
        }
        _ => {
            let node = explorer.tree.node(explorer.focus);
            (node.name.to_string(), figure(explorer, explorer.focus), String::new())
        }
    };
    // The label lives inside the center disc, so it scales with the chart.
    // Measured in window pixels, so it is brought back to design units for `px`
    let inner =
        explorer.chart_bounds.get().map_or(80.0, |b| Geometry::new(b, rings).inner) / crate::ui::UI_SCALE;
    let center_width = inner * 1.55;
    let name_size = (inner * 0.13).clamp(11.0, 15.0);
    let value_size = (inner * 0.25).clamp(17.0, 34.0) * if explorer.mode == ChartMode::Count { 0.8 } else { 1.0 };
    let hint_size = (inner * 0.105).clamp(10.0, 12.5);

    let chart_drag = explorer.chart_drag.clone();
    let can_go_up = explorer.focus != Tree::ROOT || explorer.group_focus.is_some();
    let going_up = explorer.hover == Some(Hover::Center) && can_go_up;
    let label_key = format!("{}-{:?}-{:?}", explorer.focus, explorer.mode, explorer.hover);
    let types = explorer.mode == ChartMode::Types;
    let pointer = (!types && matches!(explorer.hover, Some(Hover::Target(Target::Node(_)))))
        || matches!(explorer.hover, Some(Hover::Target(Target::Group(_))))
        || going_up;

    div()
        .id("chart")
        .relative()
        .flex_1()
        .min_w_0()
        .m(px(28.))
        .when(pointer, |s| s.cursor_pointer())
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
            let Some(explorer) = this.explorer() else { return };
            if event.pressed_button.is_some() {
                return;
            }
            let rings = explorer.rings();
            let hover = explorer.chart_bounds.get().and_then(|bounds| {
                match sunburst::hit_test(&explorer.segments, &Geometry::new(bounds, rings), event.position)? {
                    Hit::Center => Some(Hover::Center),
                    Hit::Segment(ix) => Some(Hover::Target(explorer.segments[ix].target)),
                }
            });
            if hover != explorer.hover {
                explorer.set_chart_hover(hover);
                cx.notify();
            }
        }))
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            if let Some(explorer) = this.explorer()
                && !hovered
                && !explorer.pressing
            {
                explorer.set_chart_hover(None);
                cx.notify();
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, _| {
                let Some(explorer) = this.explorer() else { return };
                explorer.pressing = true;
                let item = explorer.hovered_node().and_then(|id| explorer.drag_item(id));
                *explorer.chart_drag.0.borrow_mut() = item;
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, _| {
                if let Some(explorer) = this.explorer() {
                    explorer.pressing = false;
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if let Some(id) = this.explorer().and_then(|e| e.hovered_node()) {
                    open_item_menu(this, id, event, cx);
                }
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                if let Some(explorer) = this.explorer()
                    && explorer.pressing
                {
                    explorer.pressing = false;
                    explorer.set_chart_hover(None);
                    cx.notify();
                }
            }),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let Some(explorer) = this.explorer() else { return };
            match explorer.hover {
                Some(Hover::Center) => {
                    if !explorer.group_up()
                        && let Some(parent) = explorer.tree.node(explorer.focus).parent
                    {
                        explorer.set_focus(parent);
                    }
                }
                // File types open like folders; the files inside them don't
                Some(Hover::Target(Target::Group(g))) => explorer.open_group(Some(g)),
                Some(Hover::Target(Target::Node(id))) if explorer.mode != ChartMode::Types => explorer.set_focus(id),
                _ => return,
            }
            cx.notify();
        }))
        .on_drag(chart_drag, |drag: &ChartDrag, offset: Point<Pixels>, _, cx| {
            let item = drag.0.borrow().clone();
            cx.new(|_| DragPreview { item, offset })
        })
        .child(
            canvas(
                move |bounds, _, _| bounds_cell.set(Some(bounds)),
                move |bounds, _, window, _| {
                    let state = PaintState {
                        view,
                        highlight,
                        dim,
                        collected,
                        center_hover,
                        center_tint: theme::accent().into(),
                        rings,
                    };
                    sunburst::paint(&segments, bounds, &state, window);
                },
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .w(px(center_width))
                        .gap_0p5()
                        .when(going_up, |s| s.child(icon(Icon::CornerUpLeft, 14.0, theme::accent_hi())))
                        .child(
                            div()
                                .w_full()
                                .text_center()
                                .text_size(px(name_size))
                                .text_color(if going_up { theme::accent_hi() } else { theme::text_muted() })
                                .whitespace_nowrap()
                                .truncate()
                                .child(label_name),
                        )
                        .child(
                            div()
                                .text_size(px(value_size))
                                .font_weight(FontWeight::BOLD)
                                .whitespace_nowrap()
                                .child(label_value),
                        )
                        .when(!label_hint.is_empty(), |s| {
                            s.child(
                                div()
                                    .w_full()
                                    .text_center()
                                    .text_size(px(hint_size))
                                    .text_color(theme::text_faint())
                                    .whitespace_nowrap()
                                    .truncate()
                                    .child(label_hint),
                            )
                        })
                        .with_animation(
                            ElementId::Name(format!("center-{label_key}").into()),
                            Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()),
                            |el, t| el.opacity(t).mt(px(6. * (1. - t))),
                        ),
                ),
        )
}

/// Per-segment highlight, background dim and center glow, cross-fading between the
/// previous and current hover so moving across the chart feels continuous. A hovered
/// slice lights up together with everything drawn outside it, within its span
fn hover_weights(explorer: &Explorer, segments: &[Segment]) -> (Vec<f32>, f32, f32) {
    if explorer.zoom.is_some() || (explorer.hover.is_none() && explorer.prev_hover.is_none()) {
        return (Vec::new(), 0.0, 0.0);
    }
    let t = explorer.hover_progress();
    let can_go_up = explorer.focus != Tree::ROOT || explorer.group_focus.is_some();
    let span = |hover: Option<Hover>| match hover {
        Some(Hover::Target(target)) => segments.iter().find(|s| s.target == target).map(|s| (s.depth, s.start, s.end)),
        _ => None,
    };
    let belongs = |span: Option<(usize, f32, f32)>, segment: &Segment| {
        span.is_some_and(|(depth, start, end)| {
            segment.depth >= depth && segment.start >= start - 1e-6 && segment.end <= end + 1e-6
        }) as u8 as f32
    };
    let is_target = |hover: Option<Hover>| matches!(hover, Some(Hover::Target(_))) as u8 as f32;
    let is_center = |hover: Option<Hover>| (can_go_up && hover == Some(Hover::Center)) as u8 as f32;
    let (cur, prev) = (explorer.hover, explorer.prev_hover);
    let (cur_span, prev_span) = (span(cur), span(prev));
    let highlight = segments
        .iter()
        .map(|s| belongs(cur_span, s) * t + belongs(prev_span, s) * (1.0 - t))
        .collect();
    let dim = is_target(cur) * t + is_target(prev) * (1.0 - t);
    let center = is_center(cur) * t + is_center(prev) * (1.0 - t);
    (highlight, dim, center)
}

fn hint_for(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Dir => "Click to open",
        NodeKind::File => "Drag into the Collector below",
        NodeKind::Hidden => "Not visible to the scan",
    }
}

/// How many of a folder's children are folded into its "smaller items" slice, and their figure
fn smaller_summary(explorer: &Explorer, parent: NodeId) -> (usize, String) {
    let shown: Vec<NodeId> = explorer
        .segments
        .iter()
        .filter_map(|s| match s.target {
            Target::Node(id) if explorer.tree.node(id).parent == Some(parent) => Some(id),
            _ => None,
        })
        .collect();
    let rest: Vec<NodeId> = explorer
        .tree
        .node(parent)
        .children
        .iter()
        .copied()
        .filter(|c| !shown.contains(c))
        .collect();
    let value = match explorer.mode {
        ChartMode::Count => files(rest.iter().map(|c| explorer.tree.node(*c).items).sum()),
        _ => format::bytes(rest.iter().map(|c| explorer.tree.node(*c).size).sum()),
    };
    (rest.len(), value)
}

/// What a group's "smaller" slice holds: how many members, their size, and what to call them
fn group_rest(explorer: &Explorer, breakdown: &Breakdown, g: u32) -> (u64, u64, String) {
    let shown = |target: Target| explorer.segments.iter().any(|s| s.target == target);
    let group = breakdown.group(g);
    if group.groups.is_empty() {
        // An extension: the files beyond its biggest few
        let drawn: Vec<NodeId> = group.files.iter().copied().filter(|f| shown(Target::Node(*f))).collect();
        let size = group.size - drawn.iter().map(|f| explorer.tree.node(*f).size).sum::<u64>();
        let name = if group.name.starts_with('.') { format!("{} files", group.name) } else { "files".into() };
        return (group.count - drawn.len() as u64, size, name);
    }
    let rest: Vec<u32> = group.groups.iter().copied().filter(|c| !shown(Target::Group(*c))).collect();
    let size = rest.iter().map(|c| breakdown.group(*c).size).sum();
    (rest.len() as u64, size, "file types".into())
}

/// One line of the sidebar list: a folder, file, file type or extension
struct Row {
    key: (&'static str, usize),
    target: Target,
    name: String,
    /// Shown faintly after the name, e.g. an extension's file type
    detail: Option<&'static str>,
    glyph: Icon,
    /// Extension whose app icon stands in for `glyph`, once known
    app: Option<String>,
    color: Hsla,
    /// Share of the listed whole, by the chart's measure
    ratio: f32,
    figure: String,
    /// A real item, which can be opened, dragged and right-clicked
    node: Option<NodeId>,
    /// Clicking opens it in the chart
    opens: bool,
    collected: bool,
    muted: bool,
}

/// Rows smaller than this share of the list are drawn as one compact line
const COMPACT_RATIO: f32 = 0.001;

fn row(explorer: &Explorer, row: Row, ix: usize, list_key: &str, previewing: bool, cx: &mut Context<Burrdisk>) -> AnyElement {
    let Row { key, target, name, detail, glyph, app, color, ratio, figure, node, opens, collected, muted } = row;
    let app = app.and_then(|ext| appicon::for_extension(&ext, cx));
    // A row stays lit while its right-click menu is open
    let is_hovered = explorer.hover == Some(Hover::Target(target))
        || node.is_some_and(|id| explorer.context.is_some_and(|(c, _)| c == id));
    let t = fx::track(key, is_hovered);
    let compact = ratio < COMPACT_RATIO;
    let drag = node.and_then(|id| explorer.drag_item(id));
    let tint = Hsla { l: (color.l + 0.10 + 0.08 * t).min(0.92), ..color };

    let base = div()
        .id(key)
        .relative()
        .flex()
        .items_center()
        .mx_2()
        .bg(linear_gradient(
            90.,
            linear_color_stop(color.opacity(0.16 * t), 0.),
            linear_color_stop(color.opacity(0.03 * t), 1.),
        ))
        .when(opens, |s| s.cursor_pointer())
        .when(collected, |s| s.opacity(0.4))
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            let Some(explorer) = this.explorer() else { return };
            let next = if *hovered {
                Some(Hover::Target(target))
            } else if explorer.hover == Some(Hover::Target(target)) {
                None
            } else {
                return;
            };
            explorer.set_hover(next);
            cx.notify();
        }))
        .when(opens, |s| {
            s.on_click(cx.listener(move |this, _, _, cx| {
                if let Some(explorer) = this.explorer() {
                    match target {
                        Target::Node(id) => explorer.set_focus(id),
                        Target::Group(g) => explorer.open_group(Some(g)),
                        _ => return,
                    }
                    cx.notify();
                }
            }))
        })
        .when_some(node, |s, id| {
            s.on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| open_item_menu(this, id, event, cx)),
            )
        })
        .when_some(drag, |s, item| {
            s.on_drag(item, |item: &DragItem, offset, _, cx| {
                let item = Some(item.clone());
                cx.new(|_| DragPreview { item, offset })
            })
        });

    let name_el = |size: f32, weight: FontWeight| {
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_baseline()
            .gap_1p5()
            .child(
                div()
                    .min_w_0()
                    .text_size(px(size))
                    .font_weight(weight)
                    .truncate()
                    .when(muted, |s| s.text_color(theme::text_muted()))
                    .child(name.clone()),
            )
            .when_some(detail, |s, detail| {
                s.child(div().flex_none().text_xs().text_color(theme::text_faint()).child(detail))
            })
    };
    let chevron = div().w(px(12.)).flex_none().when(opens, |s| {
        s.child(icon(Icon::ChevronRight, if compact { 10.0 } else { 12.0 }, fx::mix(theme::text_faint(), theme::text(), t)))
    });

    let el = if compact {
        // Crumbs: one short line, so a long tail of tiny items stays out of the way
        base.gap_2p5()
            .h(px(26.))
            .pl(px(14.))
            .pr_2p5()
            .rounded(px(8.))
            .child(self::glyph(app, glyph, 13.0, tint.opacity(0.8 + 0.2 * t), collected))
            .child(
                name_el(13.0, FontWeight::NORMAL)
                    .text_color(fx::mix(theme::text_muted(), theme::text(), t)),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(fx::mix(theme::text_faint(), theme::text_muted(), t))
                    .child(figure),
            )
            .child(chevron)
    } else {
        base.gap_3()
            .h(px(50.))
            .pl(px(12.))
            .pr_2p5()
            .rounded(px(11.))
            .child(div().flex().flex_none().justify_center().w(px(20.)).child(self::glyph(app, glyph, 18.0, tint, collected)))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .child(name_el(14.0, FontWeight::MEDIUM))
                    .child(
                        div()
                            .h(px(3.))
                            .w_full()
                            .rounded_full()
                            .bg(theme::track())
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(color.opacity(0.75 + 0.25 * t))
                                    .w(relative(ratio.clamp(0.015, 1.0))),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .items_end()
                    .min_w(px(70.))
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).whitespace_nowrap().child(figure))
                    .child(div().text_xs().text_color(theme::text_faint()).child(format_percent(ratio))),
            )
            .child(chevron)
    };

    // The first screenful cascades in each time the list changes.
    // Peeking via the chart swaps lists often, so it uses a quicker, flatter fade
    if ix < ANIMATED_ROWS {
        let (step, length, rise) = if previewing { (0.012, 0.18, 4.0) } else { (0.024, 0.30, 10.0) };
        let delay = ix as f32 * step;
        let total = delay + length;
        el.with_animation(
            ElementId::Name(format!("row-in-{list_key}-{}-{}", key.0, key.1).into()),
            Animation::new(Duration::from_secs_f32(total)),
            move |el, t| {
                let local = sunburst::ease_out_cubic(((t * total - delay) / length).clamp(0.0, 1.0));
                // Collected rows settle at their dimmed opacity, not full
                el.opacity(local * if collected { 0.4 } else { 1.0 }).top(px(rise * (1. - local)))
            },
        )
        .into_any_element()
    } else {
        el.into_any_element()
    }
}

/// A file or folder row in the folder charts, or an item row in the type chart
fn node_row(explorer: &Explorer, id: NodeId, whole: u64, color: Hsla) -> Row {
    let node = explorer.tree.node(id);
    let types = explorer.mode == ChartMode::Types;
    let part = match explorer.mode {
        ChartMode::Count => node.items,
        _ => node.size,
    };
    Row {
        key: ("row", id as usize),
        target: Target::Node(id),
        name: node.name.to_string(),
        detail: None,
        glyph: item_icon(node.name, node.kind),
        app: appicon::extension_for(node.name, node.kind),
        color,
        ratio: part as f32 / whole.max(1) as f32,
        figure: figure(explorer, id),
        node: (node.kind != NodeKind::Hidden).then_some(id),
        opens: node.kind == NodeKind::Dir && !types,
        collected: explorer.is_collected(id),
        muted: node.kind == NodeKind::Hidden,
    }
}

fn group_row(breakdown: &Breakdown, g: u32, whole: u64, detail: bool) -> Row {
    let group = breakdown.group(g);
    let color = sunburst::type_color(group.category, breakdown.level(g), group.spread);
    Row {
        key: ("group-row", g as usize),
        target: Target::Group(g),
        name: group.name.clone(),
        detail: detail.then(|| group.category.label()),
        glyph: category_icon(group.category),
        // An extension's row shows the app that opens that kind of file
        app: (breakdown.level(g) == 1 && group.category != Category::Program)
            .then(|| group.name.strip_prefix('.').map(str::to_string))
            .flatten(),
        color,
        ratio: group.size as f32 / whole.max(1) as f32,
        figure: format::bytes(group.size),
        node: None,
        opens: true,
        collected: false,
        muted: false,
    }
}

/// What the sidebar lists, and how its header describes the whole
struct Listing {
    key: String,
    title: String,
    title_color: Hsla,
    title_icon: Icon,
    /// The header's big figure, as number and unit
    figure: (String, String),
    /// The header's secondary figure
    aside: String,
    rows: Vec<Row>,
    /// Items beyond the listed rows
    remaining: usize,
    /// A real folder the header acts on: collect it, reveal it
    folder: Option<NodeId>,
}

fn folder_listing(explorer: &Explorer) -> Listing {
    let listed = explorer.listed();
    let previewing = explorer.preview.is_some();
    let node = explorer.tree.node(listed);
    let count_mode = explorer.mode == ChartMode::Count;
    let mut children = node.children.to_vec();
    if count_mode {
        children.sort_by(|a, b| explorer.tree.node(*b).items.cmp(&explorer.tree.node(*a).items));
    }
    let whole = if count_mode { node.items } else { node.size };
    let remaining = children.len().saturating_sub(MAX_ROWS);
    let rows = children
        .iter()
        .take(MAX_ROWS)
        .map(|&id| node_row(explorer, id, whole, row_color(explorer, id)))
        .collect();
    let figure = if count_mode {
        (format::count(node.items), if node.items == 1 { "file" } else { "files" }.into())
    } else {
        format::bytes_parts(node.size)
    };
    let aside = if count_mode { format::bytes(node.size) } else { files(node.items) };
    Listing {
        key: format!("{listed}-{:?}", explorer.mode),
        title: node.name.to_string(),
        // While peeking, the header folder takes the color of its slice in the chart
        title_color: if previewing { row_color(explorer, listed) } else { theme::text_muted().into() },
        title_icon: Icon::FolderOpen,
        figure,
        aside,
        rows,
        remaining,
        folder: Some(listed),
    }
}

fn type_listing(explorer: &Explorer, breakdown: &Breakdown) -> Listing {
    let focus = explorer.tree.node(explorer.focus);
    let Some(g) = explorer.group_preview.or(explorer.group_focus) else {
        // Every extension in the folder, biggest first, each marked with its type
        let mut exts: Vec<u32> = (0..breakdown.groups.len() as u32)
            .filter(|g| breakdown.group(*g).groups.is_empty() && *g != Breakdown::ROOT)
            .collect();
        exts.sort_by(|a, b| breakdown.group(*b).size.cmp(&breakdown.group(*a).size));
        let hidden = breakdown.hidden.map(|id| explorer.tree.node(id).size).unwrap_or(0);
        let whole = breakdown.group(Breakdown::ROOT).size + hidden;
        let mut rows: Vec<Row> = Vec::new();
        if let Some(id) = breakdown.hidden {
            rows.push(node_row(explorer, id, whole, row_color(explorer, id)));
        }
        rows.extend(exts.iter().take(MAX_ROWS).map(|g| group_row(breakdown, *g, whole, true)));
        rows.sort_by(|a, b| b.ratio.total_cmp(&a.ratio));
        return Listing {
            key: format!("{}-types", explorer.focus),
            title: focus.name.to_string(),
            title_color: theme::text_muted().into(),
            title_icon: Icon::FolderOpen,
            figure: format::bytes_parts(focus.size),
            aside: files(breakdown.group(Breakdown::ROOT).count),
            rows,
            remaining: exts.len().saturating_sub(MAX_ROWS),
            folder: Some(explorer.focus),
        };
    };
    let group = breakdown.group(g);
    let rows: Vec<Row> = if group.groups.is_empty() {
        group
            .files
            .iter()
            .map(|&id| node_row(explorer, id, group.size, row_color(explorer, id)))
            .collect()
    } else {
        group.groups.iter().take(MAX_ROWS).map(|c| group_row(breakdown, *c, group.size, false)).collect()
    };
    let remaining = if group.groups.is_empty() {
        (group.count as usize).saturating_sub(rows.len())
    } else {
        group.groups.len().saturating_sub(MAX_ROWS)
    };
    let title_color = sunburst::type_color(group.category, breakdown.level(g), group.spread);
    Listing {
        key: format!("{}-group-{g}", explorer.focus),
        title: group_name(breakdown, g),
        title_color,
        title_icon: category_icon(group.category),
        figure: format::bytes_parts(group.size),
        aside: files(group.count),
        rows,
        remaining,
        folder: None,
    }
}

/// The sidebar's left edge, which drags to resize it. A soft line shows on hover
fn resize_edge() -> impl IntoElement {
    let t = fx::hover("sidebar-edge");
    div()
        .id("sidebar-edge")
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(-4.))
        .w(px(8.))
        .flex()
        .justify_center()
        .cursor_col_resize()
        .on_hover(fx::on_hover("sidebar-edge"))
        .on_drag(SidebarResize, |_, _, window, cx| {
            cx.set_active_drag_cursor_style(gpui::CursorStyle::ResizeLeftRight, window);
            cx.new(|_| NoPreview)
        })
        .child(div().w(px(2.)).h_full().bg(Hsla::from(theme::accent()).opacity(0.6 * t)))
}

fn sidebar(explorer: &Explorer, full_disk_access: bool, width: f32, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let listing = match explorer.breakdown.as_deref() {
        Some(breakdown) => type_listing(explorer, breakdown),
        None => folder_listing(explorer),
    };
    let previewing = explorer.preview.is_some() || explorer.group_preview.is_some();
    let Listing { key: list_key, title, title_color, title_icon, figure: (focus_num, focus_unit), aside, rows, remaining, folder } =
        listing;
    let hovered = explorer.hover;

    // Composition strip: the biggest rows side by side, in their chart colors
    let strip: Vec<AnyElement> = rows
        .iter()
        .take(14)
        .filter(|r| r.ratio >= 0.004)
        .map(|r| {
            let lit = hovered == Some(Hover::Target(r.target));
            div()
                .h_full()
                .w(relative(r.ratio))
                .bg(r.color)
                .when(hovered.is_some() && !lit, |s| s.opacity(0.35))
                .into_any_element()
        })
        .collect();

    let rows: Vec<AnyElement> = rows
        .into_iter()
        .enumerate()
        .map(|(ix, r)| row(explorer, r, ix, &list_key, previewing, cx))
        .collect();

    let at_root = folder == Some(Tree::ROOT);
    let addable = folder.is_some_and(|f| f != Tree::ROOT && !explorer.is_collected(f));
    let reveal_path = folder.map(|f| explorer.tree.path(f));

    let footer = at_root.then(|| {
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .px_5()
            .py_4()
            .border_t_1()
            .border_color(theme::border())
            .when_some(explorer.target.volume.clone(), |s, volume| {
                s.child(footer_row("Available", format::bytes(volume.available)))
                    .when(volume.purgeable() > 0, |s| {
                        s.child(footer_row("Purgeable", format::bytes(volume.purgeable())))
                    })
                    .child(footer_row("Capacity", format::bytes(volume.total)))
            })
            .child(footer_row("Scan time", format::duration(explorer.tree.duration)))
    });

    // Without Full Disk Access, macOS hides whole app libraries from the scan
    let access_hint = at_root && !full_disk_access && volumes::needs_full_disk_access(&explorer.target.path);

    div()
        .flex()
        .flex_col()
        .relative()
        .flex_none()
        .w(px(width))
        // Its contents truncate rather than holding it wider than dragged
        .min_w_0()
        .border_l_1()
        .border_color(theme::border())
        .bg(theme::surface())
        .child(resize_edge())
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_5()
                .pt_5()
                .pb_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(title_icon, 16.0, title_color))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .truncate()
                                .child(title),
                        )
                        .when_some(folder.filter(|_| addable), |s, listed| {
                            s.child(
                                icon_button(
                                    "collect-focus",
                                    Icon::Plus,
                                    cx.listener(move |this, _, _, cx| {
                                        if let Some(explorer) = this.explorer() {
                                            explorer.collect(listed);
                                            cx.notify();
                                        }
                                    }),
                                )
                                .size(px(24.)),
                            )
                        })
                        .when_some(reveal_path, |s, path| {
                            s.child(
                                icon_button("reveal-focus", Icon::ArrowUpRight, move |_, _, cx| cx.reveal_path(&path))
                                    .size(px(24.)),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_end()
                        .child(amount(focus_num, focus_unit, 24.0, 14.0))
                        .child(
                            div()
                                .ml_auto()
                                .mb(px(4.))
                                .text_xs()
                                .text_color(theme::text_faint())
                                .whitespace_nowrap()
                                .child(aside),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(2.))
                        .mt_2()
                        .h(px(6.))
                        .w_full()
                        .rounded_full()
                        .overflow_hidden()
                        .bg(theme::track())
                        .children(strip),
                ),
        )
        .when(access_hint, |s| s.child(access_panel()))
        .child(
            div()
                .id("rows")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .pb_2()
                .children(rows)
                .when(remaining > 0, |s| {
                    s.child(
                        div()
                            .px_5()
                            .py_2()
                            .text_xs()
                            .text_color(theme::text_faint())
                            .child(format!("and {} smaller items", format::count(remaining as u64))),
                    )
                }),
        )
        .children(footer)
}

/// Explains Hidden space and leads straight to the setting that shrinks it
fn access_panel() -> impl IntoElement {
    let t = fx::hover("access-panel");
    div()
        .id("access-panel")
        .on_hover(fx::on_hover("access-panel"))
        .flex()
        .items_start()
        .gap_2p5()
        .mx_3()
        .mb_2()
        .p_3()
        .rounded(px(10.))
        .bg(fx::mix(theme::accent_soft(), gpui::rgba(0xf9731638), t))
        .cursor_pointer()
        .on_click(|_, _, cx| cx.open_url(volumes::FULL_DISK_ACCESS_SETTINGS))
        .child(div().pt(px(1.)).child(icon(Icon::ShieldAlert, 16.0, theme::accent_hi())))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .text_xs()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::text())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Full Disk Access is off"),
                )
                .child(
                    div()
                        .text_color(theme::text_muted())
                        .child("macOS is hiding app data from the scan, so it piles up as Hidden space."),
                )
                .child(
                    div()
                        .mt_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fx::mix(theme::accent_hi(), theme::accent(), t))
                        .child("Open System Settings, then rescan"),
                ),
        )
}

fn footer_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .text_xs()
        .child(div().text_color(theme::text_faint()).child(label))
        .child(div().text_color(theme::text_muted()).child(value))
}

/// An item's glyph: the icon of the app that opens it when one is known, else its
/// type glyph in `tint`. Drained items lose the app icon's color too
fn glyph(app: Option<Arc<RenderImage>>, fallback: Icon, size: f32, tint: Hsla, drained: bool) -> AnyElement {
    match app {
        // App icons carry a little transparent margin, so they draw a touch larger
        Some(image) => img(image).size(px(size + 2.)).flex_none().grayscale(drained).into_any_element(),
        None => icon(fallback, size, tint).into_any_element(),
    }
}

/// Height of a tile on the Collector's shelf; its width follows its name
const TILE_H: f32 = 58.;
/// Names longer than this truncate
const TILE_NAME_MAX: f32 = 168.;
/// How far the shelf's edge fades reach
const SHELF_FADE: f32 = 36.;

fn shelf_tint(explorer: &Explorer, id: NodeId) -> Hsla {
    let color = explorer.segment_color(id).unwrap_or_else(|| gpui::hsla(0.07, 0.05, 0.40, 1.0));
    Hsla { l: (color.l + 0.12).min(0.9), ..color }
}

/// A tile's fill, lifting toward a brighter top as `t` goes to 1
fn tile_ground(t: f32) -> gpui::Background {
    linear_gradient(
        180.,
        linear_color_stop(fx::mix(theme::raised(), gpui::rgb(0x36363c), t), 0.),
        linear_color_stop(fx::mix(theme::surface(), theme::raised(), t), 1.),
    )
}

/// A shelf tile's face: its icon and size over its name
fn face(glyph: AnyElement, size: u64, name: SharedString) -> gpui::Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .justify_center()
        .gap(px(5.))
        .min_w(px(76.))
        .h(px(TILE_H))
        .px(px(10.))
        .rounded(px(10.))
        .bg(tile_ground(0.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                // Leaves room for the X that shows on hover
                .mr(px(16.))
                .child(glyph)
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text_muted())
                        .whitespace_nowrap()
                        .child(format::bytes(size)),
                ),
        )
        .child(div().max_w(px(TILE_NAME_MAX)).text_size(px(11.5)).text_color(theme::text()).truncate().child(name))
}

fn tile_face(explorer: &Explorer, id: NodeId, cx: &mut gpui::App) -> gpui::Div {
    let node = explorer.tree.node(id);
    // App icons keep their own colors; the fallback glyph takes the item's chart color
    let app = appicon::for_item(node.name, node.kind, cx);
    let glyph = glyph(app, item_icon(node.name, node.kind), 15.0, shelf_tint(explorer, id), false);
    face(glyph, node.size, node.name.to_string().into())
}

/// One collected item on the shelf. It can be picked up and dropped outside the
/// tray to put it back, or onto another tile to move it there
fn tile(explorer: &Explorer, id: NodeId, cx: &mut Context<Burrdisk>) -> AnyElement {
    if explorer.shelf_dragging() == Some(id) {
        // Where the lifted tile came from, as wide as the tile
        return div()
            .relative()
            .flex_none()
            .child(tile_face(explorer, id, cx).opacity(0.))
            .child(
                div().absolute().inset_0().rounded(px(10.)).bg(theme::track()),
            )
            .into_any_element();
    }
    let key = ElementId::from(("tile", id as usize));
    let t = fx::hover(key.clone());
    let entity = cx.entity();
    let tray = explorer.tray_bounds.clone();
    let item = explorer.drag_item(id).map(ShelfItem);
    let tint = shelf_tint(explorer, id);
    tile_face(explorer, id, cx)
        .id(key.clone())
        .cursor_grab()
        .bg(tile_ground(t))
        .on_hover(fx::on_hover(key))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| open_item_menu(this, id, event, cx)),
        )
        .when_some(item, |s, item| {
            s.on_drag(item, move |item: &ShelfItem, _, _, cx| {
                let (item, tray) = (item.clone(), tray.clone());
                let id = item.0.id;
                let view = cx.new(|_| ShelfPreview { item, tint, tray });
                let weak = gpui::AnyView::from(view.clone()).downgrade();
                entity.update(cx, |this, cx| {
                    if let Some(explorer) = this.explorer() {
                        explorer.shelf_drag = Some((id, weak));
                    }
                    cx.notify();
                });
                view
            })
        })
        .on_drop(cx.listener(move |this, item: &ShelfItem, _, cx| {
            if let Some(explorer) = this.explorer() {
                explorer.move_collected(item.0.id, Some(id));
                cx.notify();
            }
        }))
        .drag_over::<ShelfItem>(|s, _, _, _| {
            s.bg(linear_gradient(
                180.,
                linear_color_stop(fx::mix(theme::raised(), theme::accent(), 0.28), 0.),
                linear_color_stop(fx::mix(theme::surface(), theme::accent(), 0.16), 1.),
            ))
        })
        .child(
            div().absolute().top(px(4.)).right(px(4.)).opacity(t).child(
                icon_button(("uncollect", id as usize), Icon::X, cx.listener(move |this, _, _, cx| {
                    if let Some(explorer) = this.explorer() {
                        explorer.uncollect(id);
                        cx.notify();
                    }
                }))
                .size(px(18.)),
            ),
        )
        .with_animation(
            ("tile-in", id as usize),
            Animation::new(Duration::from_millis(280)).with_easing(ease_out_quint()),
            |el, t| el.opacity(t).top(px(8. * (1. - t))),
        )
        .into_any_element()
}

/// The row of tiles. It scrolls sideways with eased motion, and fades out at
/// whichever edge has more beyond it
fn shelf(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> AnyElement {
    let tiles: Vec<AnyElement> = explorer.collected.clone().into_iter().map(|id| tile(explorer, id, cx)).collect();
    let scroll = explorer.shelf_scroll;
    let room = explorer.shelf_room();
    // The fades are the tray's own ground, which darkens downward, so each is drawn
    // as thin strips that match it top to bottom and thin out toward the tiles
    let inset = (80. - (TILE_H + 12.)) / 2. / 80.;
    let (top, bottom) = (fx::mix(theme::surface(), theme::bg(), inset), fx::mix(theme::surface(), theme::bg(), 1. - inset));
    let fade = |left: bool, amount: f32| {
        const STRIPS: usize = 18;
        let strips = (0..STRIPS).map(move |i| {
            let reach = (i as f32 + 0.5) / STRIPS as f32;
            let alpha = amount * (1. - reach) * (1. - reach);
            div().flex_none().w(px(SHELF_FADE / STRIPS as f32)).h_full().bg(linear_gradient(
                180.,
                linear_color_stop(top.opacity(alpha), 0.),
                linear_color_stop(bottom.opacity(alpha), 1.),
            ))
        });
        let el = div().absolute().top_0().bottom_0().flex().w(px(SHELF_FADE));
        if left { el.left_0().children(strips) } else { el.right_0().flex_row_reverse().children(strips) }
    };
    let widths = explorer.shelf_widths.clone();
    let content = explorer.shelf_widths.clone();
    div()
        .id("shelf")
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(TILE_H + 12.))
        .overflow_hidden()
        .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
            if let Some(explorer) = this.explorer() {
                let delta = event.delta.pixel_delta(px(48.));
                // A plain wheel scrolls the shelf too
                let step = if delta.x.abs() > delta.y.abs() { delta.x } else { delta.y };
                explorer.scroll_shelf(-f32::from(step));
                cx.notify();
            }
        }))
        .child(
            canvas(
                move |b, _, _| widths.set((f32::from(b.size.width), widths.get().1)),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(-scroll))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    canvas(
                        move |b, _, _| content.set((content.get().0, f32::from(b.size.width))),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .children(tiles),
        )
        .child(fade(true, (scroll / SHELF_FADE).clamp(0., 1.)))
        .child(fade(false, ((room - scroll) / SHELF_FADE).clamp(0., 1.)))
        .into_any_element()
}

/// A shelf tile in hand. Off the tray it says what letting go will do
struct ShelfPreview {
    item: ShelfItem,
    tint: Hsla,
    tray: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Render for ShelfPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let over_tray = self.tray.get().is_some_and(|b| b.contains(&window.mouse_position()));
        // The preview's origin is the tile's own corner, so it lifts from where it was
        let item = &self.item.0;
        let app = appicon::for_item(&item.name, item.kind, cx);
        let glyph = glyph(app, item_icon(&item.name, item.kind), 15.0, self.tint, false);
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(face(glyph, item.size, item.name.clone()).bg(tile_ground(1.)).shadow(vec![
                shadow(gpui::black().opacity(0.6), 12.0, 30.0, -8.0),
                shadow(gpui::black().opacity(0.3), 1.0, 3.0, 0.0),
            ]))
            .when(!over_tray, |s| {
                s.child(
                    div()
                        .px(px(7.))
                        .py(px(3.))
                        .rounded(px(6.))
                        .bg(theme::raised())
                        .border_1()
                        .border_color(theme::border_strong())
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .whitespace_nowrap()
                        .child("Drop to put back"),
                )
            })
    }
}

/// One line of the manifest: what's going, from where, and how it's doing
struct ManifestLine {
    name: String,
    place: String,
    size: u64,
    kind: NodeKind,
    color: Option<Hsla>,
    /// Absent during the countdown, before anything has started
    state: Option<u8>,
}

/// The list that rises above the tray once Move to Trash is pressed: everything that
/// will go, while the countdown runs, then each item's progress as it moves
fn manifest(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> Option<AnyElement> {
    let remaining = explorer
        .countdown
        .map(|at| 1.0 - (at.elapsed().as_secs_f32() / TRASH_COUNTDOWN.as_secs_f32()).min(1.0));
    let job = explorer.trash_job.as_ref();
    if remaining.is_none() && job.is_none() {
        return None;
    }
    let lines: Vec<ManifestLine> = match job {
        Some(job) => job
            .items
            .iter()
            .enumerate()
            .map(|(ix, item)| ManifestLine {
                name: item.name.clone(),
                place: item.place.clone(),
                size: item.size,
                kind: item.kind,
                color: item.color,
                state: Some(job.state(ix)),
            })
            .collect(),
        None => explorer
            .collected
            .iter()
            .map(|&id| {
                let node = explorer.tree.node(id);
                ManifestLine {
                    name: node.name.to_string(),
                    place: explorer.tree.path(id).parent().map(app::display_path).unwrap_or_default(),
                    size: node.size,
                    kind: node.kind,
                    color: explorer.segment_color(id),
                    state: None,
                }
            })
            .collect(),
    };
    let total: u64 = lines.iter().map(|l| l.size).sum();
    if let Some(ix) = lines.iter().position(|l| l.state == Some(app::TRASH_MOVING)) {
        explorer.manifest_scroll.scroll_to_item(ix);
    }
    // Seconds since start, for the spinner and the moving item's sweep
    let clock = explorer.revealed.elapsed().as_secs_f32();

    let (bar, figure): (AnyElement, String) = match (job, remaining) {
        (Some(job), _) => {
            let done = (job.shown * total as f32) as u64;
            (
                div()
                    .h_full()
                    .w(relative(job.shown.clamp(0., 1.)))
                    .rounded_full()
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(theme::accent(), 0.),
                        linear_color_stop(theme::accent_hi(), 1.),
                    ))
                    .into_any_element(),
                format!("{} of {}", format::bytes(done.min(total)), format::bytes(total)),
            )
        }
        (None, Some(remaining)) => {
            let left = (remaining * TRASH_COUNTDOWN.as_secs_f32()).ceil().max(1.0) as u32;
            (
                div()
                    .h_full()
                    .w(relative(remaining))
                    .rounded_full()
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(gpui::rgb(0xb91c1c), 0.),
                        linear_color_stop(gpui::rgb(0xf87171), 1.),
                    ))
                    .into_any_element(),
                format!("in {left} s"),
            )
        }
        (None, None) => unreachable!(),
    };

    let rows: Vec<AnyElement> = lines
        .into_iter()
        .enumerate()
        .map(|(ix, line)| {
            let state = line.state;
            let moving = state == Some(app::TRASH_MOVING);
            let settled = matches!(state, Some(app::TRASH_MOVED) | Some(app::TRASH_FAILED));
            let tint = line
                .color
                .map(|c| Hsla { l: (c.l + 0.12).min(0.9), ..c })
                .unwrap_or_else(|| theme::text_muted().into());
            let app = appicon::for_item(&line.name, line.kind, cx);
            let status: Option<AnyElement> = state.map(|state| {
                let (glyph, label, color): (Option<AnyElement>, &str, Hsla) = match state {
                    app::TRASH_MOVING => (
                        Some(
                            icon(Icon::Loader, 13.0, theme::accent_hi())
                                .with_transformation(gpui::Transformation::rotate(gpui::radians(
                                    clock * std::f32::consts::TAU * 1.2,
                                )))
                                .into_any_element(),
                        ),
                        "Moving",
                        theme::accent_hi().into(),
                    ),
                    app::TRASH_MOVED => (Some(icon(Icon::Check, 13.0, SUCCESS).into_any_element()), "Moved", SUCCESS),
                    app::TRASH_FAILED => {
                        (Some(icon(Icon::X, 13.0, theme::danger()).into_any_element()), "Couldn't move", theme::danger().into())
                    }
                    _ => (None, "Waiting", theme::text_faint().into()),
                };
                div()
                    .flex()
                    .flex_none()
                    .justify_end()
                    .items_center()
                    .gap(px(6.))
                    .w(px(108.))
                    .text_size(px(11.5))
                    .text_color(color)
                    .children(glyph)
                    .child(label)
                    .into_any_element()
            });
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(11.))
                .h(px(40.))
                .px(px(10.))
                .rounded(px(9.))
                .when(moving, |s| s.bg(theme::surface_hover()))
                .when(settled, |s| s.opacity(0.55))
                .child(div().flex().flex_none().justify_center().w(px(18.)).child(glyph(
                    app,
                    item_icon(&line.name, line.kind),
                    15.0,
                    tint,
                    false,
                )))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_baseline()
                        .gap_2()
                        .child(div().flex_none().max_w(px(220.)).text_size(px(13.)).truncate().child(line.name))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(11.5))
                                .text_color(theme::text_faint())
                                .truncate()
                                .child(line.place),
                        ),
                )
                .when(moving, |s| {
                    // A single item gives no progress of its own, so it just sweeps
                    let sweep = (clock * 0.9 + ix as f32 * 0.37).fract();
                    s.child(
                        div()
                            .relative()
                            .flex_none()
                            .w(px(96.))
                            .h(px(4.))
                            .rounded_full()
                            .overflow_hidden()
                            .bg(theme::track())
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left(relative(sweep * 1.4 - 0.4))
                                    .w(relative(0.4))
                                    .rounded_full()
                                    .bg(theme::accent_hi()),
                            ),
                    )
                })
                .child(
                    div()
                        .flex_none()
                        .w(px(64.))
                        .flex()
                        .justify_end()
                        .text_size(px(12.))
                        .text_color(theme::text_muted())
                        .child(format::bytes(line.size)),
                )
                .children(status)
                .into_any_element()
        })
        .collect();

    // The list fades away once everything has moved
    let closing = job
        .and_then(|j| j.finished)
        .map_or(1., |at| 1. - ((at.elapsed().as_secs_f32() - 0.55) / 0.3).clamp(0., 1.));
    let title = format!("Moving to the {}", platform::TRASH);
    Some(
        div()
            .absolute()
            .left(px(16.))
            .bottom(px(80. + 10.))
            .w(px(560.))
            .max_w(relative(0.9))
            .opacity(closing)
            .child(
                div()
                    .id("manifest")
                    .flex()
                    .flex_col()
                    .p(px(8.))
                    .rounded(px(14.))
                    .bg(gpui::rgb(0x211f1e))
                    .shadow(vec![
                        shadow(gpui::black().opacity(0.5), 14.0, 36.0, -6.0),
                        shadow(gpui::black().opacity(0.3), 1.0, 3.0, 0.0),
                    ])
                    .occlude()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px(px(10.))
                            .pt(px(6.))
                            .pb(px(10.))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(div().flex_1().h(px(6.)).rounded_full().bg(theme::track()).overflow_hidden().child(bar))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.))
                                    .text_color(theme::text_muted())
                                    .child(figure),
                            ),
                    )
                    .child(
                        div()
                            .id("manifest-rows")
                            .flex()
                            .flex_col()
                            .max_h(px(240.))
                            .overflow_y_scroll()
                            .track_scroll(&explorer.manifest_scroll)
                            .children(rows),
                    )
                    .with_animation(
                        "manifest-in",
                        Animation::new(Duration::from_millis(240)).with_easing(ease_out_quint()),
                        |el, t| el.opacity(t).top(px(10. * (1. - t))),
                    ),
            )
            .into_any_element(),
    )
}

/// The checkmark green for things that went through
const SUCCESS: Hsla = Hsla { h: 0.40, s: 0.6, l: 0.55, a: 1.0 };

fn tray(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let count = explorer.collected.len();
    let total = explorer.collected_size();
    let job = explorer.trash_job.as_ref();
    let moving = job.is_some();
    let empty = count == 0 && !moving;

    let shelf = (!empty && !moving).then(|| shelf(explorer, cx));
    // While moving: how far along, and what's going right now
    let progress = job.map(|job| {
        let (done, _) = job.processed();
        let percent = (job.shown * 100.).round() as u32;
        let current = job.current().map(|item| item.name.clone());
        (percent, done, job.items.len(), job.total(), current, job.finished.is_some())
    });

    let (num, unit) = format::bytes_parts(total);
    // 1 when the countdown starts, draining to 0 as the move begins
    let remaining = explorer
        .countdown
        .map(|at| 1.0 - (at.elapsed().as_secs_f32() / TRASH_COUNTDOWN.as_secs_f32()).min(1.0));
    let action: AnyElement = if explorer.trashing {
        button("trash", "Moving…", ButtonStyle::Secondary, |_, _, _| {}).into_any_element()
    } else if remaining.is_some() {
        icon_label_button(
            "stop-trash",
            Icon::X,
            "Stop",
            ButtonStyle::Secondary,
            cx.listener(|this, _, _, cx| this.stop_trash_countdown(cx)),
        )
        .into_any_element()
    } else {
        icon_label_button(
            "trash",
            Icon::Trash,
            format!("Move to {}", platform::TRASH),
            ButtonStyle::Danger,
            cx.listener(|this, _, _, cx| this.start_trash_countdown(cx)),
        )
        .into_any_element()
    };
    let mut action = Some(action);
    let seconds_left = remaining.map(|r| (r * TRASH_COUNTDOWN.as_secs_f32()).ceil().max(1.0) as u32);

    div()
        .id("tray")
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap_4()
        .h(px(80.))
        .px_5()
        .border_t_1()
        .border_color(theme::border())
        .bg(linear_gradient(
            180.,
            linear_color_stop(theme::surface(), 0.),
            linear_color_stop(theme::bg(), 1.),
        ))
        .on_drop(cx.listener(|this, item: &DragItem, _, cx| {
            if let Some(explorer) = this.explorer() {
                explorer.collect(item.id);
                cx.notify();
            }
        }))
        .on_drop(cx.listener(|this, drag: &ChartDrag, _, cx| {
            let item = drag.0.borrow_mut().take();
            if let (Some(explorer), Some(item)) = (this.explorer(), item) {
                explorer.collect(item.id);
                cx.notify();
            }
        }))
        .on_drop(cx.listener(|this, item: &ShelfItem, _, cx| {
            // Let go on the tray but not on a tile: it goes to the end of the shelf
            if let Some(explorer) = this.explorer() {
                explorer.move_collected(item.0.id, None);
                cx.notify();
            }
        }))
        .drag_over::<DragItem>(|s, _, _, _| s.bg(theme::accent_soft()).border_color(theme::accent()))
        .drag_over::<ChartDrag>(|s, _, _, _| s.bg(theme::accent_soft()).border_color(theme::accent()))
        .child({
            let bounds = explorer.tray_bounds.clone();
            canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {}).absolute().inset_0()
        })
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(48.))
                .rounded(px(14.))
                .when(empty, |s| {
                    s.border_2()
                        .border_dashed()
                        .border_color(theme::border_strong())
                        .child(icon(Icon::Inbox, 18.0, theme::text_faint()))
                })
                .when(!empty && !moving && remaining.is_none(), |s| {
                    s.bg(linear_gradient(
                        180.,
                        linear_color_stop(theme::accent_hi(), 0.),
                        linear_color_stop(theme::accent_lo(), 1.),
                    ))
                    .shadow(glow(theme::accent(), 1.4))
                    .text_color(gpui::rgb(0x1f0b00))
                    .child(div().text_base().font_weight(FontWeight::BOLD).child(count.to_string()))
                })
                .when_some(progress.clone(), |s, (percent, ..)| {
                    s.bg(linear_gradient(
                        180.,
                        linear_color_stop(theme::accent_hi(), 0.),
                        linear_color_stop(theme::accent_lo(), 1.),
                    ))
                    .shadow(glow(theme::accent(), 1.4))
                    .text_color(gpui::rgb(0x1f0b00))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::BOLD).child(format!("{percent}%")))
                })
                .when_some(remaining.zip(seconds_left), |s, (remaining, left)| {
                    // The count gives way to the countdown: a ring draining around
                    // the seconds left, each new number dropping into place
                    s.relative()
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(gpui::rgb(0xf87171), 0.),
                            linear_color_stop(gpui::rgb(0xb91c1c), 1.),
                        ))
                        .text_color(gpui::white())
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    let ring = Bounds { origin: bounds.origin - gpui::point(px(5.), px(5.)), size: bounds.size + gpui::size(px(10.), px(10.)) };
                                    sunburst::paint_arc(
                                        window,
                                        ring,
                                        0.0,
                                        remaining,
                                        2.5,
                                        gpui::white(),
                                        gpui::white().opacity(0.22),
                                        false,
                                    )
                                },
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .child(
                            div()
                                .text_size(px(19.))
                                .font_weight(FontWeight::BOLD)
                                .child(left.to_string())
                                .with_animation(
                                    ElementId::Name(format!("countdown-{left}").into()),
                                    Animation::new(Duration::from_millis(360)).with_easing(ease_out_quint()),
                                    |el, t| el.opacity(t).mt(px(-14. * (1. - t))),
                                ),
                        )
                })
                .with_animation(
                    ElementId::Name(format!("badge-pop-{count}-{seconds_left:?}").into()),
                    Animation::new(Duration::from_millis(420)),
                    move |el, t| {
                        // The halo flares each time the count changes, and on every second of a countdown
                        let flare = (t * std::f32::consts::PI).sin() * (1. - t);
                        let tone = if remaining.is_some() { theme::danger() } else { theme::accent() };
                        if empty { el } else { el.shadow(glow(tone, 1.4 + flare * 3.0)) }
                    },
                ),
        )
        .when(empty, |s| {
            s.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme::text_muted())
                            .whitespace_nowrap()
                            .truncate()
                            .child("Drop files and folders here"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::text_faint())
                            .whitespace_nowrap()
                            .truncate()
                            .child(format!("Collect what you want gone, then move it all to the {} at once.", platform::TRASH)),
                    ),
            )
        })
        .when(!empty && remaining.is_some(), |s| {
            s.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .child(format!("Moving {} to the {}", format::bytes(total), platform::TRASH)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::text_faint())
                            .whitespace_nowrap()
                            .truncate()
                            .child(format!(
                                "{count} {} · Press Stop or Esc to keep everything",
                                if count == 1 { "item" } else { "items" }
                            )),
                    ),
            )
            .children(action.take())
        })
        .when_some(progress, |s, (_, done, count, total, current, finished)| {
            let headline = match (finished, current) {
                (true, _) => format!("Moved to the {}", platform::TRASH),
                (false, Some(name)) => format!("Moving {name}"),
                (false, None) => format!("Moving to the {}", platform::TRASH),
            };
            s.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .truncate()
                            .child(headline),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::text_faint())
                            .whitespace_nowrap()
                            .child(format!("{done} of {count} · {}", format::bytes(total))),
                    ),
            )
        })
        .when(!empty && !moving && remaining.is_none(), |s| {
            s.child(amount(num, unit, 22.0, 13.0).flex_none())
            .children(shelf)
            .child(button(
                "clear",
                "Clear",
                ButtonStyle::Ghost,
                cx.listener(|this, _, _, cx| {
                    if let Some(explorer) = this.explorer() {
                        explorer.collected.clear();
                        explorer.countdown = None;
                        cx.notify();
                    }
                }),
            ))
            .children(action.take())
        })
}

struct DragPreview {
    item: Option<DragItem>,
    offset: Point<Pixels>,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(item) = &self.item else {
            return div();
        };
        let app = appicon::for_item(&item.name, item.kind, cx);
        let glyph = glyph(app, item_icon(&item.name, item.kind), 14.0, theme::text_muted().into(), false);
        div().pl(self.offset.x + px(10.)).pt(self.offset.y + px(10.)).child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(32.))
                .px_3()
                .rounded(px(9.))
                .bg(theme::raised())
                .border_1()
                .border_color(theme::border_strong())
                .shadow_lg()
                .text_color(theme::text())
                .child(glyph)
                .child(div().max_w(px(220.)).text_sm().truncate().child(item.name.clone()))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::text_muted())
                        .child(format::bytes(item.size)),
                ),
        )
    }
}
