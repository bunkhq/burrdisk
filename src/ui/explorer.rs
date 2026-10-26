use gpui::{
    AnyElement, Context, FontWeight, IntoElement, MouseButton, MouseMoveEvent, Pixels, Point, Render,
    SharedString, Window, canvas, div, prelude::*, px, relative,
};

use crate::app::{Burrdisk, ChartDrag, DragItem, Explorer, Hover, Screen, TRANSITION};
use crate::format;
use crate::scan::{NodeId, NodeKind, Tree};
use crate::sunburst::{self, Geometry, Hit, PaintState, Target};
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, button, icon, icon_button};

/// Rows listed in the sidebar before the rest are summarized.
const MAX_ROWS: usize = 200;
const SIDEBAR_WIDTH: f32 = 340.0;

pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let Screen::Explorer(explorer) = &app.screen else {
        return div().into_any_element();
    };
    let animating = explorer.transition.elapsed() < TRANSITION;
    if animating {
        window.request_animation_frame();
    }

    div()
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
                    let crumb = div()
                        .id(("crumb", ix))
                        .px_2()
                        .py_1()
                        .rounded(px(6.))
                        .text_sm()
                        .flex_none()
                        .max_w(px(220.))
                        .truncate()
                        .when(is_last, |s| s.text_color(theme::text()).font_weight(FontWeight::MEDIUM))
                        .when(!is_last, |s| {
                            s.text_color(theme::text_muted())
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::surface_hover()).text_color(theme::text()))
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
    let segments = explorer.segments.clone();
    let bounds_cell = explorer.chart_bounds.clone();
    let hovered_ix = match explorer.hover {
        Some(Hover::Target(target)) => segments.iter().position(|s| s.target == target),
        _ => None,
    };
    let collected: Vec<bool> = segments
        .iter()
        .map(|s| match s.target {
            Target::Node(id) => explorer.is_collected(id),
            Target::Smaller(parent) => explorer.is_collected(parent),
        })
        .collect();
    let progress = (explorer.transition.elapsed().as_secs_f32() / TRANSITION.as_secs_f32()).min(1.0);

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
            (node.name.to_string(), node.size, "Click to go back")
        }
        _ => {
            let node = explorer.tree.node(explorer.focus);
            (node.name.to_string(), node.size, "")
        }
    };
    let center_width = explorer
        .chart_bounds
        .get()
        .map(|b| Geometry::new(b).inner * 1.6)
        .unwrap_or(150.0);

    let chart_drag = explorer.chart_drag.clone();
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
                explorer.hover = hover;
                cx.notify();
            }
        }))
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            if let Some(explorer) = this.explorer()
                && !hovered
                && !explorer.pressing
            {
                explorer.hover = None;
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
                    explorer.hover = None;
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
                        hovered: hovered_ix,
                        collected,
                        progress,
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
                        .child(
                            div()
                                .w_full()
                                .text_center()
                                .text_sm()
                                .text_color(theme::text_muted())
                                .truncate()
                                .child(label_name),
                        )
                        .child(
                            div()
                                .text_size(px(24.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format::bytes(label_size)),
                        )
                        .when(!label_hint.is_empty(), |s| {
                            s.child(div().text_xs().text_color(theme::text_faint()).child(label_hint))
                        }),
                ),
        )
}

fn hint_for(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Dir => "Click to open · drag to collect",
        NodeKind::File => "Drag to collect",
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
    let focus = explorer.tree.node(explorer.focus);
    let focus_size = focus.size.max(1);
    let hovered = explorer.hovered_node();
    let children: Vec<NodeId> = focus.children.iter().copied().take(MAX_ROWS).collect();
    let remaining = focus.children.len().saturating_sub(MAX_ROWS);
    let focus_path = explorer.tree.path(explorer.focus);
    let items = focus.items;

    let rows = children.into_iter().map(|id| {
        let node = explorer.tree.node(id);
        let color = explorer
            .segment_color(id)
            .unwrap_or_else(|| gpui::hsla(0.0, 0.0, 0.38, 1.0));
        let ratio = node.size as f32 / focus_size as f32;
        let is_hovered = hovered == Some(id);
        let collected = explorer.is_collected(id);
        let kind = node.kind;
        let name = node.name.to_string();
        let size = format::bytes(node.size);
        let drag = explorer.drag_item(id);
        let path = (kind != NodeKind::Hidden).then(|| explorer.tree.path(id));

        div()
            .id(("row", id as usize))
            .group("row")
            .relative()
            .flex()
            .items_center()
            .gap_2p5()
            .h(px(36.))
            .px_3()
            .mx_2()
            .rounded(px(8.))
            .overflow_hidden()
            .when(is_hovered, |s| s.bg(theme::surface_hover()))
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
                explorer.hover = next;
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
            // Proportional wash behind the row.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(relative(ratio.clamp(0.0, 1.0)))
                    .bg(color.opacity(0.07)),
            )
            .child(div().flex_none().size(px(8.)).rounded_full().bg(color))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .when(kind == NodeKind::Hidden, |s| s.text_color(theme::text_muted()))
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap_0p5()
                    .invisible()
                    .group_hover("row", |s| s.visible())
                    .when_some(path, |s, path| {
                        s.child(
                            icon_button(("reveal", id as usize), Icon::ArrowUpRight, move |_, _, cx| {
                                cx.stop_propagation();
                                cx.reveal_path(&path);
                            })
                            .size(px(24.)),
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
                                .size(px(24.)),
                            )
                        })
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(72.))
                    .text_right()
                    .text_sm()
                    .text_color(theme::text_muted())
                    .child(size),
            )
            .when(kind == NodeKind::Dir, |s| {
                s.child(icon(Icon::ChevronRight, 12.0, theme::text_faint()))
            })
            .when(kind != NodeKind::Dir, |s| s.child(div().w(px(12.)).flex_none()))
            .into_any_element()
    });

    let footer = (explorer.focus == Tree::ROOT)
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
        && explorer.focus == Tree::ROOT
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
                .pb_4()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(Icon::FolderOpen, 16.0, theme::text_muted()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
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
                        .text_xs()
                        .text_color(theme::text_faint())
                        .child(format!("{} · {} items", format::bytes(focus.size), format::count(items))),
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
        div()
            .id(("chip", id as usize))
            .flex()
            .flex_none()
            .items_center()
            .gap_1p5()
            .h(px(28.))
            .pl_2p5()
            .pr_1()
            .rounded(px(7.))
            .bg(theme::raised())
            .border_1()
            .border_color(theme::border())
            .child(icon(glyph, 12.0, theme::text_faint()))
            .child(div().max_w(px(160.)).text_xs().truncate().child(node.name.to_string()))
            .child(div().text_xs().text_color(theme::text_faint()).child(format::bytes(node.size)))
            .child(
                icon_button(("uncollect", id as usize), Icon::X, cx.listener(move |this, _, _, cx| {
                    if let Some(explorer) = this.explorer() {
                        explorer.uncollect(id);
                        cx.notify();
                    }
                }))
                .size(px(20.)),
            )
    });

    let (num, unit) = format::bytes_parts(total);
    let action: AnyElement = if explorer.trashing {
        button("trash", "Moving…", ButtonStyle::Secondary, |_, _, _| {}).into_any_element()
    } else if explorer.confirm_armed {
        button(
            "trash",
            format!("Move {} to Trash?", format::bytes(total)),
            ButtonStyle::Danger,
            cx.listener(|this, _, _, cx| this.move_collected_to_trash(cx)),
        )
        .into_any_element()
    } else {
        button(
            "trash",
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
        .h(px(76.))
        .px_5()
        .border_t_1()
        .border_color(theme::border())
        .bg(theme::bg())
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
        .drag_over::<DragItem>(|s, _, _, _| s.bg(theme::accent_soft()))
        .drag_over::<ChartDrag>(|s, _, _, _| s.bg(theme::accent_soft()))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(48.))
                .rounded_full()
                .border_2()
                .border_color(if empty { theme::border_strong() } else { theme::accent() })
                .when(empty, |s| s.child(icon(Icon::Inbox, 18.0, theme::text_faint())))
                .when(!empty, |s| {
                    s.child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(count.to_string()),
                    )
                }),
        )
        .when(empty, |s| {
            s.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(div().text_sm().text_color(theme::text_muted()).child(
                        explorer
                            .notice
                            .clone()
                            .unwrap_or_else(|| SharedString::from("Drop files and folders here")),
                    ))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::text_faint())
                            .child("Collect what you want gone, then move it all to the Trash at once."),
                    ),
            )
        })
        .when(!empty, |s| {
            s.child(
                div()
                    .flex()
                    .flex_none()
                    .items_baseline()
                    .gap_1()
                    .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child(num))
                    .child(div().text_sm().text_color(theme::text_muted()).child(unit)),
            )
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
