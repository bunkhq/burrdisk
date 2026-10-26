use gpui::{
    AnyElement, Context, FontWeight, IntoElement, SharedString, Window, canvas, div, prelude::*,
    px, relative,
};

use crate::app::Burrdisk;
use crate::format;
use crate::sunburst;
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, bar, button, icon, section_label};
use crate::volumes::{Volume, VolumeKind};

const COLLAPSED_IMAGES: usize = 6;

pub fn render(app: &mut Burrdisk, _window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
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
        body = body.child(hero(volume, cx));
    } else if app.volumes_loaded {
        body = body.child(
            div()
                .text_color(theme::text_muted())
                .child("No startup disk found."),
        );
    }

    if !drives.is_empty() {
        body = body.child(
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
        );
    }

    if !images.is_empty() {
        let total = images.len();
        let expanded = app.show_all_images || total <= COLLAPSED_IMAGES + 1;
        let shown: Vec<Volume> = if expanded {
            images
        } else {
            images.into_iter().take(COLLAPSED_IMAGES).collect()
        };
        body = body.child(
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
        );
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
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_muted())
                .child("Burrdisk"),
        )
        .child(button(
            "scan-folder",
            "Scan Folder…",
            ButtonStyle::Secondary,
            cx.listener(|this, _, _, cx| this.pick_folder(cx)),
        ))
}

fn hero(volume: Volume, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let ratio = volume.fill_ratio();
    let fill = theme::fullness(ratio);
    let (used_num, used_unit) = format::bytes_parts(volume.total.saturating_sub(volume.available));
    let percent = format!("{:.0}%", ratio * 100.0);
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
        .rounded(px(20.))
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border())
        .cursor_pointer()
        .hover(|s| s.border_color(theme::border_strong()))
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
                            sunburst::paint_gauge(window, bounds, ratio, 16.0, fill, theme::track().into());
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
                                .text_size(px(34.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(percent),
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
                button(
                    "scan-startup",
                    "Scan",
                    ButtonStyle::Primary,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.scan_volume(scan_volume.clone(), cx)
                    }),
                )
                .h(px(40.))
                .px(px(22.))
                .text_base()
                .rounded(px(10.)),
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
    div()
        .id(("drive", ix))
        .flex()
        .flex_col()
        .gap_3()
        .w(relative(0.325))
        .min_w(px(220.))
        .p_4()
        .rounded(px(14.))
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border())
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_hover()).border_color(theme::border_strong()))
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
                        .size(px(32.))
                        .rounded(px(8.))
                        .bg(theme::raised())
                        .child(icon(glyph, 16.0, theme::text_muted())),
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
        .child(bar(ratio, theme::fullness(ratio), 4.0))
        .child(div().text_xs().text_color(theme::text_muted()).child(free))
        .into_any_element()
}

fn image_row(ix: usize, volume: Volume, cx: &mut Context<Burrdisk>) -> AnyElement {
    let target = volume.clone();
    div()
        .id(("image", ix))
        .flex()
        .items_center()
        .gap_3()
        .h(px(44.))
        .px_3()
        .rounded(px(10.))
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface()))
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
        .child(icon(Icon::Disc, 15.0, theme::text_faint()))
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
        .into_any_element()
}
