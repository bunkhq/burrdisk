use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Context, ElementId, FontWeight, Hsla, IntoElement,
    MouseButton, MouseMoveEvent, Pixels, Point, Render, Window, canvas, div,
    ease_out_quint, linear_color_stop, linear_gradient, prelude::*, px, relative,
};

use crate::app::{Burrdisk, ChartDrag, DragItem, Explorer, Hover, NOTICE_DURATION, Notice, Screen};
use crate::format;
use crate::scan::{NodeId, NodeKind, Tree};
use crate::sunburst::{self, Geometry, Hit, PaintState, Segment, Target, View};
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, amount, button, glow, icon, icon_button, icon_label_button};
use crate::ui::fx;

/// Rows listed in the sidebar before the rest are summarized.
const MAX_ROWS: usize = 200;
const SIDEBAR_WIDTH: f32 = 360.0;
/// Rows that animate in when a folder opens; the rest appear immediately.
const ANIMATED_ROWS: usize = 18;

fn row_color(explorer: &Explorer, id: NodeId) -> Hsla {
    explorer
        .segment_color(id)
        .unwrap_or_else(|| gpui::hsla(0.07, 0.05, 0.40, 1.0))
}

fn format_percent(ratio: f32) -> String {
    match ratio * 100.0 {
        p if p >= 10.0 => format!("{p:.0}%"),
        p if p >= 0.1 => format!("{p:.1}%"),
        _ => "<0.1%".into(),
    }
}

pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let Screen::Explorer(explorer) = &mut app.screen else {
        return div().into_any_element();
    };
    if explorer.tick() {
        window.request_animation_frame();
    }
    let explorer = &*explorer;
    if explorer.notice.is_some() {
        // Keeps the toast's fade-out moving.
        window.request_animation_frame();
    }

    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .child(titlebar(explorer, cx))
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(chart(explorer, cx))
                .child(sidebar(explorer, cx)),
        )
        .child(tray(explorer, cx))
        .children(explorer.notice.clone().map(toast))
        .into_any_element()
}

/// A pill floating above the tray with the result of the last Trash operation.
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
                .border_1()
                .border_color(tone.opacity(0.45))
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
                // Fade back out over the last half second.
                let out = (left / 0.5).clamp(0.0, 1.0);
                el.opacity(t * out).mb(px(-12. * (1. - t)))
            },
        )
        .into_any_element()
}

fn titlebar(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let lineage = explorer.tree.lineage(explorer.focus);
    let last = lineage.len() - 1;
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .h(px(56.))
        .pl(px(92.))
        .pr(px(16.))
        .border_b_1()
        .border_color(theme::border())
        .child(icon_button(
            "home",
            Icon::ChevronLeft,
            cx.listener(|this, _, _, cx| this.go_home(cx)),
        ))
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_0p5()
                .overflow_hidden()
                .children(lineage.into_iter().enumerate().flat_map(|(ix, id)| {
                    let name = explorer.tree.node(id).name.to_string();
                    let is_last = ix == last;
                    let t = fx::hover(("crumb", ix));
                    let crumb = div()
                        .id(("crumb", ix))
                        .on_hover(fx::on_hover(("crumb", ix)))
                        .px_2p5()
                        .py_1()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(gpui::transparent_black())
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
                                .border_color(Hsla::from(theme::accent()).opacity(0.22))
                        })
                        .when(!is_last, |s| {
                            s.text_color(fx::mix(theme::text_muted(), theme::text(), t))
                                .bg(fx::mix(gpui::transparent_black(), theme::raised(), t))
                                .cursor_pointer()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(explorer) = this.explorer() {
                                explorer.set_focus(id);
                                cx.notify();
                            }
                        }))
                        .child(name)
                        .into_any_element();
                    let separator = (ix > 0).then(|| icon(Icon::ChevronRight, 12.0, theme::text_faint()).into_any_element());
                    separator.into_iter().chain(std::iter::once(crumb))
                })),
        )
        .child(icon_button(
            "rescan",
            Icon::Refresh,
            cx.listener(|this, _, _, cx| this.rescan(cx)),
        ))
}

fn chart(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let bounds_cell = explorer.chart_bounds.clone();
    let (segments, view) = match &explorer.zoom {
        Some(zoom) => (zoom.segments.clone(), zoom.view()),
        None => (explorer.segments.clone(), View::revealing(explorer.revealed)),
    };
    let collected: Vec<bool> = segments
        .iter()
        .map(|s| match s.target {
            Target::Node(id) => explorer.is_collected(id),
            Target::Smaller(parent) => explorer.is_collected(parent),
        })
        .collect();
    let (highlight, dim, center_hover) = hover_weights(explorer, &segments);

    // What the center disc describes: the hovered item, or the folder in focus.
    let (label_name, label_size, label_hint) = match explorer.hover {
        Some(Hover::Target(Target::Node(id))) => {
            let node = explorer.tree.node(id);
            (node.name.to_string(), node.size, hint_for(node.kind))
        }
        Some(Hover::Target(Target::Smaller(parent))) => {
            let (count, size) = smaller_summary(explorer, parent);
            (format!("{count} smaller items"), size, "")
        }
        Some(Hover::Center) if explorer.focus != Tree::ROOT => {
            let node = explorer.tree.node(explorer.focus);
            let parent = explorer.tree.node(explorer.focus).parent.unwrap_or(Tree::ROOT);
            (format!("Back to {}", explorer.tree.node(parent).name), node.size, "")
        }
        _ => {
            let node = explorer.tree.node(explorer.focus);
            (node.name.to_string(), node.size, "")
        }
    };
    // The label lives inside the center disc, so it scales with the chart.
    let inner = explorer.chart_bounds.get().map_or(80.0, |b| Geometry::new(b).inner);
    let center_width = inner * 1.55;
    let name_size = (inner * 0.13).clamp(11.0, 15.0);
    let value_size = (inner * 0.25).clamp(17.0, 34.0);
    let hint_size = (inner * 0.105).clamp(10.0, 12.5);

    let chart_drag = explorer.chart_drag.clone();
    let going_up = explorer.hover == Some(Hover::Center) && explorer.focus != Tree::ROOT;
    let label_key = format!("{}-{:?}", explorer.focus, explorer.hover);
    let pointer = matches!(explorer.hover, Some(Hover::Target(Target::Node(_))))
        || (matches!(explorer.hover, Some(Hover::Center)) && explorer.focus != Tree::ROOT);

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
            let hover = explorer.chart_bounds.get().and_then(|bounds| {
                match sunburst::hit_test(&explorer.segments, &Geometry::new(bounds), event.position)? {
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
                    if let Some(parent) = explorer.tree.node(explorer.focus).parent {
                        explorer.set_focus(parent);
                    }
                }
                Some(Hover::Target(Target::Node(id))) => explorer.set_focus(id),
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
                                .child(format::bytes(label_size)),
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
/// previous and current hover so moving across the chart feels continuous.
fn hover_weights(explorer: &Explorer, segments: &[Segment]) -> (Vec<f32>, f32, f32) {
    if explorer.zoom.is_some() || (explorer.hover.is_none() && explorer.prev_hover.is_none()) {
        return (Vec::new(), 0.0, 0.0);
    }
    let tree = &explorer.tree;
    let t = explorer.hover_progress();
    let can_go_up = explorer.focus != Tree::ROOT;
    let belongs = |hover: Option<Hover>, segment: &Segment| -> bool {
        match hover {
            Some(Hover::Target(Target::Node(id))) => match segment.target {
                Target::Node(sid) => tree.is_ancestor(id, sid),
                Target::Smaller(parent) => tree.is_ancestor(id, parent),
            },
            Some(Hover::Target(smaller)) => segment.target == smaller,
            _ => false,
        }
    };
    let is_target = |hover: Option<Hover>| matches!(hover, Some(Hover::Target(_))) as u8 as f32;
    let is_center = |hover: Option<Hover>| (can_go_up && hover == Some(Hover::Center)) as u8 as f32;
    let (cur, prev) = (explorer.hover, explorer.prev_hover);
    let highlight = segments
        .iter()
        .map(|s| belongs(cur, s) as u8 as f32 * t + belongs(prev, s) as u8 as f32 * (1.0 - t))
        .collect();
    let dim = is_target(cur) * t + is_target(prev) * (1.0 - t);
    let center = is_center(cur) * t + is_center(prev) * (1.0 - t);
    (highlight, dim, center)
}

fn hint_for(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Dir => "Click to open",
        NodeKind::File => "Drag to the tray to collect",
        NodeKind::Hidden => "Not visible to the scan",
    }
}

fn smaller_summary(explorer: &Explorer, parent: NodeId) -> (usize, u64) {
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
    (rest.len(), rest.iter().map(|c| explorer.tree.node(*c).size).sum())
}

fn sidebar(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let listed = explorer.listed();
    let previewing = explorer.preview.is_some();
    let focus = explorer.tree.node(listed);
    let focus_size = focus.size.max(1);
    let hovered = explorer.hovered_node();
    let children: Vec<NodeId> = focus.children.iter().copied().take(MAX_ROWS).collect();

    let remaining = focus.children.len().saturating_sub(MAX_ROWS);
    let focus_path = explorer.tree.path(listed);
    let items = focus.items;
    let (focus_num, focus_unit) = format::bytes_parts(focus.size);

    let rows = children.iter().copied().enumerate().map(|(ix, id)| {
        let node = explorer.tree.node(id);
        let color = row_color(explorer, id);
        let ratio = node.size as f32 / focus_size as f32;
        let is_hovered = hovered == Some(id);
        let t = fx::track(("row", id as usize), is_hovered);
        let collected = explorer.is_collected(id);
        let kind = node.kind;
        let name = node.name.to_string();
        let size = format::bytes(node.size);
        let percent = format_percent(ratio);
        let drag = explorer.drag_item(id);
        let path = (kind != NodeKind::Hidden).then(|| explorer.tree.path(id));
        let glyph = match kind {
            NodeKind::Dir => Icon::Folder,
            NodeKind::File => Icon::File,
            NodeKind::Hidden => Icon::Disc,
        };

        let row = div()
            .id(("row", id as usize))
            .group("row")
            .relative()
            .flex()
            .items_center()
            .gap_3()
            .h(px(50.))
            .pl_2()
            .pr_2p5()
            .mx_2()
            .rounded(px(11.))
            .border_1()
            .border_color(color.opacity(0.28 * t))
            .bg(linear_gradient(
                90.,
                linear_color_stop(color.opacity(0.16 * t), 0.),
                linear_color_stop(color.opacity(0.03 * t), 1.),
            ))
            .when(kind == NodeKind::Dir, |s| s.cursor_pointer())
            .when(collected, |s| s.opacity(0.4))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let Some(explorer) = this.explorer() else { return };
                let next = if *hovered {
                    Some(Hover::Target(Target::Node(id)))
                } else if explorer.hover == Some(Hover::Target(Target::Node(id))) {
                    None
                } else {
                    return;
                };
                explorer.set_hover(next);
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(explorer) = this.explorer() {
                    explorer.set_focus(id);
                    cx.notify();
                }
            }))
            .when_some(drag, |s, item| {
                s.on_drag(item, |item: &DragItem, offset, _, cx| {
                    let item = Some(item.clone());
                    cx.new(|_| DragPreview { item, offset })
                })
            })
            // Colored tile, brighter when the row is hovered.
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(32.))
                    .rounded(px(9.))
                    .bg(color.opacity(0.16 + 0.14 * t))
                    .border_1()
                    .border_color(color.opacity(0.30))
                    .when(t > 0.0, |s| s.shadow(glow(color, 1.2 * t)))
                    .child(icon(glyph, 15.0, Hsla { l: (color.l + 0.12).min(0.9), ..color })),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .when(kind == NodeKind::Hidden, |s| s.text_color(theme::text_muted()))
                            .child(name),
                    )
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
                    )
                    .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(-4.))
                    .flex()
                    .gap_0p5()
                    .p_0p5()
                    .rounded(px(9.))
                    .bg(theme::surface())
                    .border_1()
                    .border_color(theme::border())
                    .shadow_md()
                    .opacity(t)
                    .when(t < 0.05, |s| s.invisible())
                    .when_some(path, |s, path| {
                        s.child(
                            icon_button(("reveal", id as usize), Icon::ArrowUpRight, move |_, _, cx| {
                                cx.stop_propagation();
                                cx.reveal_path(&path);
                            })
                            .size(px(26.)),
                        )
                        .when(!collected, |s| {
                            s.child(
                                icon_button(
                                    ("collect", id as usize),
                                    Icon::Plus,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        if let Some(explorer) = this.explorer() {
                                            explorer.collect(id);
                                            cx.notify();
                                        }
                                    }),
                                )
                                .size(px(26.)),
                            )
                        })
                    }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .items_end()
                    .w(px(70.))
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(size))
                    .child(div().text_xs().text_color(theme::text_faint()).child(percent)),
            )
            .child(div().w(px(12.)).flex_none().when(kind == NodeKind::Dir, |s| {
                s.child(icon(Icon::ChevronRight, 12.0, fx::mix(theme::text_faint(), theme::text(), t)))
            }));

        // The first screenful cascades in each time the folder changes.
        // Peeking via the chart swaps lists often, so it uses a quicker, flatter fade.
        if ix < ANIMATED_ROWS {
            let (step, length, rise) = if previewing { (0.012, 0.18, 4.0) } else { (0.024, 0.30, 10.0) };
            let delay = ix as f32 * step;
            let total = delay + length;
            row.with_animation(
                ElementId::Name(format!("row-in-{listed}-{id}").into()),
                Animation::new(Duration::from_secs_f32(total)),
                move |el, t| {
                    let local = sunburst::ease_out_cubic(((t * total - delay) / length).clamp(0.0, 1.0));
                    el.opacity(local).top(px(rise * (1. - local)))
                },
            )
            .into_any_element()
        } else {
            row.into_any_element()
        }
    });

    // Composition strip: the biggest items side by side, in their chart colors.
    let strip: Vec<AnyElement> = children
        .iter()
        .take(14)
        .filter_map(|&id| {
            let ratio = explorer.tree.node(id).size as f32 / focus_size as f32;
            (ratio >= 0.004).then(|| {
                div()
                    .h_full()
                    .w(relative(ratio))
                    .bg(row_color(explorer, id))
                    .when(hovered == Some(id), |s| s.opacity(1.0))
                    .when(hovered.is_some() && hovered != Some(id), |s| s.opacity(0.35))
                    .into_any_element()
            })
        })
        .collect();

    let footer = (listed == Tree::ROOT)
        .then(|| explorer.target.volume.clone())
        .flatten()
        .map(|volume| {
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .px_5()
                .py_4()
                .border_t_1()
                .border_color(theme::border())
                .child(footer_row("Available", format::bytes(volume.available)))
                .when(volume.purgeable() > 0, |s| {
                    s.child(footer_row("Purgeable", format::bytes(volume.purgeable())))
                })
                .child(footer_row("Capacity", format::bytes(volume.total)))
        });

    let hidden_hint = cfg!(target_os = "macos")
        && listed == Tree::ROOT
        && explorer.target.path == std::path::Path::new("/")
        && explorer
            .tree
            .node(Tree::ROOT)
            .children
            .iter()
            .any(|c| {
                let n = explorer.tree.node(*c);
                n.kind == NodeKind::Hidden && n.size > explorer.tree.node(Tree::ROOT).size / 5
            });

    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(SIDEBAR_WIDTH))
        .border_l_1()
        .border_color(theme::border())
        .bg(theme::surface())
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
                        .h(px(14.))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::accent_hi())
                        .when(previewing, |s| s.child("PEEKING INSIDE"))
                        .when(!previewing, |s| s.text_color(theme::text_faint()).child("CURRENT FOLDER")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(
                            Icon::FolderOpen,
                            16.0,
                            if previewing { theme::accent_hi() } else { theme::text_muted() },
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .truncate()
                                .child(focus.name.to_string()),
                        )
                        .child(
                            icon_button("reveal-focus", Icon::ArrowUpRight, move |_, _, cx| {
                                cx.reveal_path(&focus_path)
                            })
                            .size(px(24.)),
                        ),
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
                                .child(format!("{} items", format::count(items))),
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
        .when(hidden_hint, |s| {
            s.child(
                div()
                    .id("fda-hint")
                    .mx_3()
                    .mb_2()
                    .p_3()
                    .rounded(px(10.))
                    .bg(theme::accent_soft())
                    .text_xs()
                    .text_color(theme::text_muted())
                    .cursor_pointer()
                    .on_click(|_, _, cx| {
                        cx.open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
                    })
                    .child(
                        div()
                            .text_color(theme::text())
                            .font_weight(FontWeight::MEDIUM)
                            .child("Lots of hidden space?"),
                    )
                    .child("Give Burrdisk Full Disk Access in System Settings, then rescan."),
            )
        })
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

fn footer_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .text_xs()
        .child(div().text_color(theme::text_faint()).child(label))
        .child(div().text_color(theme::text_muted()).child(value))
}

fn tray(explorer: &Explorer, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let count = explorer.collected.len();
    let total = explorer.collected_size();
    let empty = count == 0;

    let chips = explorer.collected.iter().map(|&id| {
        let node = explorer.tree.node(id);
        let glyph = if node.kind == NodeKind::Dir { Icon::Folder } else { Icon::File };
        let color = row_color(explorer, id);
        div()
            .id(("chip", id as usize))
            .flex()
            .flex_none()
            .items_center()
            .gap_1p5()
            .h(px(30.))
            .pl_2()
            .pr_1()
            .rounded(px(9.))
            .bg(linear_gradient(
                180.,
                linear_color_stop(theme::raised(), 0.),
                linear_color_stop(theme::surface(), 1.),
            ))
            .border_1()
            .border_color(color.opacity(0.35))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.))
                    .rounded(px(6.))
                    .bg(color.opacity(0.22))
                    .child(icon(glyph, 11.0, Hsla { l: (color.l + 0.12).min(0.9), ..color })),
            )
            .child(div().max_w(px(160.)).text_xs().truncate().child(node.name.to_string()))
            .child(div().text_xs().text_color(theme::text_faint()).child(format::bytes(node.size)))
            .child(
                icon_button(("uncollect", id as usize), Icon::X, cx.listener(move |this, _, _, cx| {
                    if let Some(explorer) = this.explorer() {
                        explorer.uncollect(id);
                        cx.notify();
                    }
                }))
                .size(px(22.)),
            )
            .with_animation(
                ("chip-in", id as usize),
                Animation::new(Duration::from_millis(280)).with_easing(ease_out_quint()),
                |el, t| el.opacity(t).top(px(8. * (1. - t))),
            )
    });

    let (num, unit) = format::bytes_parts(total);
    let action: AnyElement = if explorer.trashing {
        button("trash", "Moving…", ButtonStyle::Secondary, |_, _, _| {}).into_any_element()
    } else if explorer.confirm_armed {
        icon_label_button(
            "trash",
            Icon::Trash,
            format!("Move {} to Trash?", format::bytes(total)),
            ButtonStyle::Danger,
            cx.listener(|this, _, _, cx| this.move_collected_to_trash(cx)),
        )
        .into_any_element()
    } else {
        icon_label_button(
            "trash",
            Icon::Trash,
            "Move to Trash",
            ButtonStyle::Danger,
            cx.listener(|this, _, _, cx| this.arm_confirm(cx)),
        )
        .into_any_element()
    };

    div()
        .id("tray")
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
        .drag_over::<DragItem>(|s, _, _, _| s.bg(theme::accent_soft()).border_color(theme::accent()))
        .drag_over::<ChartDrag>(|s, _, _, _| s.bg(theme::accent_soft()).border_color(theme::accent()))
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
                .when(!empty, |s| {
                    s.bg(linear_gradient(
                        180.,
                        linear_color_stop(theme::accent_hi(), 0.),
                        linear_color_stop(theme::accent_lo(), 1.),
                    ))
                    .shadow(glow(theme::accent(), 1.4))
                    .text_color(gpui::rgb(0x1f0b00))
                    .child(div().text_base().font_weight(FontWeight::BOLD).child(count.to_string()))
                })
                .with_animation(("badge-pop", count), Animation::new(Duration::from_millis(420)), move |el, t| {
                    // The halo flares briefly each time the count changes.
                    let flare = (t * std::f32::consts::PI).sin() * (1. - t);
                    if empty { el } else { el.shadow(glow(theme::accent(), 1.4 + flare * 3.0)) }
                }),
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
                            .child("Collect what you want gone, then move it all to the Trash at once."),
                    ),
            )
        })
        .when(!empty, |s| {
            s.child(amount(num, unit, 22.0, 13.0).flex_none())
            .child(
                div()
                    .id("chips")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1p5()
                    .overflow_x_scroll()
                    .children(chips),
            )
            .child(button(
                "clear",
                "Clear",
                ButtonStyle::Ghost,
                cx.listener(|this, _, _, cx| {
                    if let Some(explorer) = this.explorer() {
                        explorer.collected.clear();
                        explorer.confirm_armed = false;
                        cx.notify();
                    }
                }),
            ))
            .child(action)
        })
}

struct DragPreview {
    item: Option<DragItem>,
    offset: Point<Pixels>,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let Some(item) = &self.item else {
            return div();
        };
        let glyph = if item.kind == NodeKind::Dir { Icon::Folder } else { Icon::File };
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
                .child(icon(glyph, 14.0, theme::text_muted()))
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
