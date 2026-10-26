use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Context, ElementId, FontWeight, Hsla, IntoElement,
    SharedString, Window, canvas, div, linear_color_stop, linear_gradient,
    prelude::*, px, relative,
};

use crate::app::Burrdisk;
use crate::format;
use crate::sunburst;
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, bar, glow, icon, icon_label_button, section_label};

const GAUGE_FILL: Duration = Duration::from_millis(1100);

/// Fades and lifts a section into place, `order` beats after the page appears.
fn rise(el: impl IntoElement, id: impl Into<ElementId>, order: usize) -> AnyElement {
    let delay = order as f32 * 0.07;
    let total = delay + 0.45;
    div()
        .relative()
        .child(el.into_any_element())
        .with_animation(id, Animation::new(Duration::from_secs_f32(total)), move |el, t| {
            let local = sunburst::ease_out_cubic(((t * total - delay) / 0.45).clamp(0.0, 1.0));
            el.opacity(local).top(px(14. * (1. - local)))
        })
        .into_any_element()
}
use crate::volumes::{Volume, VolumeKind};

const COLLAPSED_IMAGES: usize = 6;

pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let fill_t = (app.home_since.elapsed().as_secs_f32() / GAUGE_FILL.as_secs_f32()).min(1.0);
    if fill_t < 1.0 {
        window.request_animation_frame();
    }
    let fill_t = 1.0 - (1.0 - fill_t).powi(4);
    let startup = app.volumes.iter().find(|v| v.kind == VolumeKind::Startup).cloned();
    let drives: Vec<Volume> = app
        .volumes
        .iter()
        .filter(|v| matches!(v.kind, VolumeKind::Internal | VolumeKind::External | VolumeKind::Network))
        .cloned()
        .collect();
    let images: Vec<Volume> = app
        .volumes
        .iter()
        .filter(|v| v.kind == VolumeKind::DiskImage)
        .cloned()
        .collect();

    let mut body = div().flex().flex_col().gap(px(36.)).w_full().max_w(px(920.)).mx_auto();

    if let Some(volume) = startup {
        body = body.child(rise(hero(volume, fill_t, cx), "rise-hero", 0));
    } else if app.volumes_loaded {
        body = body.child(
            div()
                .text_color(theme::text_muted())
                .child("No startup disk found."),
        );
    }

    if !drives.is_empty() {
        body = body.child(rise(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(section_label("Drives"))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_3()
                        .children(drives.into_iter().enumerate().map(|(ix, v)| drive_card(ix, v, cx))),
                ),
            "rise-drives",
            1,
        ));
    }

    if !images.is_empty() {
        let total = images.len();
        let expanded = app.show_all_images || total <= COLLAPSED_IMAGES + 1;
        let shown: Vec<Volume> = if expanded {
            images
        } else {
            images.into_iter().take(COLLAPSED_IMAGES).collect()
        };
        body = body.child(rise(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .items_baseline()
                                .child(section_label("Disk images"))
                                .child(div().text_xs().text_color(theme::text_faint()).child(total.to_string())),
                        )
                        .when(total > COLLAPSED_IMAGES + 1, |this| {
                            let label = if app.show_all_images { "Show less" } else { "Show all" };
                            this.child(
                                div()
                                    .id("toggle-images")
                                    .text_xs()
                                    .text_color(theme::text_muted())
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(theme::text()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_all_images = !this.show_all_images;
                                        cx.notify();
                                    }))
                                    .child(label),
                            )
                        }),
                )
                .child(
                    div()
                        .grid()
                        .grid_cols(2)
                        .gap_2()
                        .children(shown.into_iter().enumerate().map(|(ix, v)| image_row(ix, v, cx))),
                ),
            "rise-images",
            2,
        ));
    }

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(titlebar(cx))
        .child(
            div()
                .id("home-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px(px(40.))
                .pt(px(8.))
                .pb(px(48.))
                .child(body),
        )
}

fn titlebar(cx: &mut Context<Burrdisk>) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .h(px(56.))
        .pl(px(96.))
        .pr(px(20.))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .size(px(14.))
                        .rounded_full()
                        .border_3()
                        .border_color(theme::accent())
                        .shadow(glow(theme::accent(), 1.2)),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme::text())
                        .child("Burrdisk"),
                ),
        )
        .child(icon_label_button(
            "scan-folder",
            Icon::FolderSearch,
            "Scan Folder…",
            ButtonStyle::Secondary,
            cx.listener(|this, _, _, cx| this.pick_folder(cx)),
        ))
}

fn hero(volume: Volume, fill_t: f32, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let ratio = volume.fill_ratio();
    let shown_ratio = ratio * fill_t;
    let fill = theme::fullness(ratio);
    let (used_num, used_unit) = format::bytes_parts(volume.total.saturating_sub(volume.available));
    let percent = format!("{:.0}", shown_ratio * 100.0);
    let details: SharedString = format!(
        "{} · {}",
        volume.subtitle(),
        volume.file_system.to_uppercase()
    )
    .into();
    let scan_volume = volume.clone();
    let card_volume = volume.clone();

    div()
        .id("hero")
        .flex()
        .items_center()
        .gap(px(40.))
        .p(px(32.))
        .rounded(px(22.))
        .bg(linear_gradient(
            135.,
            linear_color_stop(gpui::rgb(0x2a1f18), 0.),
            linear_color_stop(theme::surface(), 0.55),
        ))
        .border_1()
        .border_color(Hsla::from(theme::accent()).opacity(0.16))
        .shadow(vec![gpui::BoxShadow {
            color: gpui::black().opacity(0.4),
            offset: gpui::point(px(0.), px(12.)),
            blur_radius: px(32.),
            spread_radius: px(-8.),
        }])
        .cursor_pointer()
        .hover(|s| {
            s.border_color(Hsla::from(theme::accent()).opacity(0.38)).shadow(vec![gpui::BoxShadow {
                color: Hsla::from(theme::accent()).opacity(0.16),
                offset: gpui::point(px(0.), px(12.)),
                blur_radius: px(40.),
                spread_radius: px(-6.),
            }])
        })
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(card_volume.clone(), cx)))
        .child(
            div()
                .relative()
                .flex_none()
                .size(px(168.))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            sunburst::paint_gauge(window, bounds, shown_ratio, 16.0, fill, theme::track().into());
                        },
                    )
                    .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .child(div().text_size(px(38.)).font_weight(FontWeight::BOLD).child(percent))
                                .child(div().text_base().text_color(theme::text_muted()).child("%")),
                        )
                        .child(div().text_xs().text_color(theme::text_muted()).child("full")),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(20.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(Icon::HardDrive, 18.0, theme::text_muted()))
                                .child(
                                    div()
                                        .text_size(px(26.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .truncate()
                                        .child(volume.name.clone()),
                                ),
                        )
                        .child(div().text_sm().text_color(theme::text_muted()).child(details)),
                )
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_1p5()
                        .child(div().text_size(px(40.)).font_weight(FontWeight::SEMIBOLD).child(used_num))
                        .child(div().text_size(px(18.)).text_color(theme::text_muted()).child(used_unit))
                        .child(
                            div()
                                .ml_2()
                                .text_sm()
                                .text_color(theme::text_muted())
                                .child(format!("used of {}", format::bytes(volume.total))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(28.))
                        .child(stat("Available", format::bytes(volume.available), fill))
                        .when(volume.purgeable() > 0, |this| {
                            this.child(stat(
                                "Of which purgeable",
                                format::bytes(volume.purgeable()),
                                theme::text_faint().into(),
                            ))
                        }),
                ),
        )
        .child(
            div().flex_none().child(
                icon_label_button(
                    "scan-startup",
                    Icon::Scan,
                    "Scan",
                    ButtonStyle::Primary,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.scan_volume(scan_volume.clone(), cx)
                    }),
                )
                .h(px(44.))
                .pl(px(18.))
                .pr(px(22.))
                .text_base()
                .rounded(px(12.)),
            ),
        )
}

fn stat(label: &'static str, value: String, dot: gpui::Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(div().size(px(6.)).rounded_full().bg(dot))
                .child(div().text_xs().text_color(theme::text_faint()).child(label)),
        )
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(value))
}

fn drive_card(ix: usize, volume: Volume, cx: &mut Context<Burrdisk>) -> AnyElement {
    let ratio = volume.fill_ratio();
    let glyph = match volume.kind {
        VolumeKind::Network => Icon::Network,
        VolumeKind::External => Icon::Usb,
        _ => Icon::HardDrive,
    };
    let free = format!("{} free of {}", format::bytes(volume.available), format::bytes(volume.total));
    let target = volume.clone();
    let group: SharedString = format!("drive-{ix}").into();
    div()
        .id(("drive", ix))
        .group(group.clone())
        .flex()
        .flex_col()
        .gap_3()
        .w(relative(0.325))
        .min_w(px(220.))
        .p_4()
        .rounded(px(16.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(theme::surface_hover(), 0.),
            linear_color_stop(theme::surface(), 1.),
        ))
        .border_1()
        .border_color(theme::border())
        .cursor_pointer()
        .hover(|s| {
            s.border_color(Hsla::from(theme::accent()).opacity(0.35))
                .shadow(glow(theme::accent(), 0.8))
        })
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2p5()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(34.))
                        .rounded(px(10.))
                        .bg(theme::raised())
                        .group_hover(group.clone(), |s| s.bg(theme::accent_soft()))
                        .child(
                            icon(glyph, 16.0, theme::text_muted())
                                .group_hover(group, |s| s.text_color(theme::accent_hi())),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).truncate().child(volume.name.clone()))
                        .child(div().text_xs().text_color(theme::text_faint()).child(volume.subtitle())),
                ),
        )
        .child(bar(("drive-bar", ix), ratio, theme::fullness(ratio), 5.0))
        .child(div().text_xs().text_color(theme::text_muted()).child(free))
        .into_any_element()
}

fn image_row(ix: usize, volume: Volume, cx: &mut Context<Burrdisk>) -> AnyElement {
    let target = volume.clone();
    let group: SharedString = format!("image-{ix}").into();
    div()
        .id(("image", ix))
        .group(group.clone())
        .flex()
        .items_center()
        .gap_3()
        .h(px(46.))
        .px_2()
        .rounded(px(12.))
        .border_1()
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface()).border_color(theme::border()))
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(30.))
                .rounded(px(9.))
                .bg(theme::surface())
                .group_hover(group.clone(), |s| s.bg(theme::accent_soft()))
                .child(
                    icon(Icon::Disc, 14.0, theme::text_faint())
                        .group_hover(group.clone(), |s| s.text_color(theme::accent_hi())),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .child(volume.name.clone()),
        )
        .child(
            div()
                .w(px(72.))
                .flex_none()
                .text_xs()
                .text_right()
                .text_color(theme::text_muted())
                .child(format::bytes(volume.total)),
        )
        .child(
            icon(Icon::ChevronRight, 12.0, theme::text_faint())
                .invisible()
                .group_hover(group, |s| s.visible()),
        )
        .into_any_element()
}
