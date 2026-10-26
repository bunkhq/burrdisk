use std::time::{Duration, Instant};

use gpui::{
    Animation, AnimationExt, AnyElement, BoxShadow, Context, ElementId, FontWeight, Hsla,
    IntoElement, SharedString, Window, canvas, div, linear_color_stop, linear_gradient, point,
    prelude::*, px, relative,
};

use crate::app::Burrdisk;
use crate::format;
use crate::sunburst;
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, amount, bar, glow, icon, icon_label_button, section_label};
use crate::ui::fx;
use crate::volumes::{Volume, VolumeKind};

const COLLAPSED_IMAGES: usize = 6;
const GAUGE_FILL: Duration = Duration::from_millis(1100);
const IMAGES_TOGGLE: Duration = Duration::from_millis(380);
const IMAGE_ROW: f32 = 46.0;
const IMAGE_GAP: f32 = 8.0;

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

fn ago(at: Instant) -> String {
    match at.elapsed().as_secs() {
        s if s < 60 => "Scanned just now".into(),
        s if s < 3600 => format!("Scanned {} min ago", s / 60),
        s => format!("Scanned {} h ago", s / 3600),
    }
}

pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let fill_t = (app.home_since.elapsed().as_secs_f32() / GAUGE_FILL.as_secs_f32()).min(1.0);
    if fill_t < 1.0 {
        window.request_animation_frame();
    }
    let fill_t = 1.0 - (1.0 - fill_t).powi(4);
    // Display type follows the window a little, so the hero neither crowds nor floats.
    let scale = (f32::from(window.viewport_size().width) / 1180.0).clamp(0.8, 1.15);

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

    let mut body = div().flex().flex_col().gap(px(36.)).w_full().max_w(px(940.)).mx_auto();

    body = body.child(rise(intro(app), "rise-intro", 0));

    if let Some(volume) = startup {
        let scanned = app.scanned_at(&volume.mount);
        body = body.child(rise(hero(volume, scanned, fill_t, scale, cx), "rise-hero", 1));
    } else if app.volumes_loaded {
        body = body.child(div().text_color(theme::text_muted()).child("No startup disk found."));
    }

    if !drives.is_empty() {
        let cards: Vec<AnyElement> = drives
            .into_iter()
            .enumerate()
            .map(|(ix, v)| {
                let scanned = app.scanned_at(&v.mount);
                drive_card(ix, v, scanned, cx)
            })
            .collect();
        body = body.child(rise(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(section_label("Drives"))
                .child(div().flex().flex_wrap().gap_3().children(cards)),
            "rise-drives",
            2,
        ));
    }

    if !images.is_empty() {
        body = body.child(rise(image_section(app, images, window, cx), "rise-images", 3));
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
                .pt(px(4.))
                .pb(px(48.))
                .child(body),
        )
}

fn intro(app: &Burrdisk) -> impl IntoElement {
    let disks = app.volumes.iter().filter(|v| v.kind != VolumeKind::DiskImage).count();
    let free: u64 = app
        .volumes
        .iter()
        .filter(|v| v.kind != VolumeKind::DiskImage)
        .map(|v| v.available)
        .sum();
    let summary = if app.volumes_loaded {
        format!(
            "{disks} {} · {} available",
            if disks == 1 { "disk" } else { "disks" },
            format::bytes(free)
        )
    } else {
        "Looking for disks…".into()
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(px(28.))
                .font_weight(FontWeight::BOLD)
                .whitespace_nowrap()
                .child("Where did the space go?"),
        )
        .child(div().text_sm().text_color(theme::text_muted()).whitespace_nowrap().child(summary))
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

fn hero(
    volume: Volume,
    scanned: Option<Instant>,
    fill_t: f32,
    scale: f32,
    cx: &mut Context<Burrdisk>,
) -> impl IntoElement {
    let ratio = volume.fill_ratio();
    let shown_ratio = ratio * fill_t;
    let fill = theme::fullness(ratio);
    let accent: Hsla = theme::accent().into();
    let t = fx::hover("hero");
    let (used_num, used_unit) = format::bytes_parts(volume.total.saturating_sub(volume.available));
    let percent = format!("{:.0}", shown_ratio * 100.0);
    let details: SharedString = format!("{} · {}", volume.subtitle(), volume.file_system.to_uppercase()).into();
    let scan_volume = volume.clone();
    let card_volume = volume.clone();
    let gauge = 168.0 * scale;

    div()
        .id("hero")
        .relative()
        .flex()
        .items_center()
        .gap(px(36. * scale))
        .p(px(30. * scale))
        .rounded(px(22.))
        .bg(linear_gradient(
            135.,
            linear_color_stop(fx::mix(gpui::rgb(0x2a1f18), gpui::rgb(0x33241a), t), 0.),
            linear_color_stop(theme::surface(), 0.6),
        ))
        .border_1()
        .border_color(accent.opacity(0.16 + 0.22 * t))
        .shadow(vec![
            BoxShadow {
                color: gpui::black().opacity(0.4),
                offset: point(px(0.), px(12.)),
                blur_radius: px(32.),
                spread_radius: px(-8.),
            },
            BoxShadow {
                color: accent.opacity(0.06 + 0.12 * t),
                offset: point(px(0.), px(14.)),
                blur_radius: px(48.),
                spread_radius: px(-4.),
            },
        ])
        .cursor_pointer()
        .on_hover(fx::on_hover("hero"))
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(card_volume.clone(), cx)))
        .child(
            div()
                .relative()
                .flex_none()
                .size(px(gauge))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            sunburst::paint_gauge(window, bounds, shown_ratio, 16.0 * scale, fill, theme::track().into());
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
                        .gap_1()
                        .child(amount(percent, "%", 36.0 * scale, 16.0 * scale))
                        .child(div().text_xs().text_color(theme::text_muted()).child("full")),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(18. * scale))
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
                                        .min_w_0()
                                        .text_size(px(24. * scale))
                                        .font_weight(FontWeight::BOLD)
                                        .whitespace_nowrap()
                                        .truncate()
                                        .child(volume.name.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .text_color(theme::text_muted())
                                .whitespace_nowrap()
                                .child(details)
                                .when_some(scanned, |s, at| s.child(scanned_badge(at))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap_2()
                        .whitespace_nowrap()
                        .child(amount(used_num, used_unit, 40.0 * scale, 18.0 * scale))
                        .child(
                            div()
                                .mb(px(5. * scale))
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
                            this.child(stat("Purgeable", format::bytes(volume.purgeable()), theme::text_faint().into()))
                        }),
                ),
        )
        .child(
            div().flex_none().child(
                icon_label_button(
                    "scan-startup",
                    if scanned.is_some() { Icon::ArrowUpRight } else { Icon::Scan },
                    if scanned.is_some() { "Open" } else { "Scan" },
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

fn scanned_badge(at: Instant) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(1.))
        .rounded_full()
        .bg(theme::accent_soft())
        .text_xs()
        .text_color(theme::accent_hi())
        .child(div().size(px(5.)).rounded_full().bg(theme::accent()))
        .child(ago(at))
}

fn stat(label: &'static str, value: String, dot: Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .whitespace_nowrap()
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

fn drive_card(ix: usize, volume: Volume, scanned: Option<Instant>, cx: &mut Context<Burrdisk>) -> AnyElement {
    let ratio = volume.fill_ratio();
    let key = ("drive", ix);
    let t = fx::hover(key);
    let accent: Hsla = theme::accent().into();
    let glyph = match volume.kind {
        VolumeKind::Network => Icon::Network,
        VolumeKind::External => Icon::Usb,
        _ => Icon::HardDrive,
    };
    let free = format!("{} free of {}", format::bytes(volume.available), format::bytes(volume.total));
    let target = volume.clone();
    div()
        .id(key)
        .flex()
        .flex_col()
        .gap_3()
        .w(relative(0.325))
        .min_w(px(220.))
        .p_4()
        .rounded(px(16.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(fx::mix(theme::surface_hover(), gpui::rgb(0x2b2622), t), 0.),
            linear_color_stop(theme::surface(), 1.),
        ))
        .border_1()
        .border_color(fx::mix(theme::border(), accent.opacity(0.35), t))
        .when(t > 0.0, |s| s.shadow(glow(accent, 0.8 * t)))
        .cursor_pointer()
        .on_hover(fx::on_hover(key))
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2p5()
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(34.))
                        .rounded(px(10.))
                        .bg(fx::mix(theme::raised(), theme::accent_soft(), t))
                        .child(icon(glyph, 16.0, fx::mix(theme::text_muted(), theme::accent_hi(), t))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .truncate()
                                .child(volume.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme::text_faint())
                                .whitespace_nowrap()
                                .child(volume.subtitle()),
                        ),
                )
                .when_some(scanned, |s, _| s.child(div().size(px(7.)).rounded_full().bg(theme::accent()))),
        )
        .child(bar(("drive-bar", ix), ratio, theme::fullness(ratio), 5.0))
        .child(
            div()
                .text_xs()
                .text_color(theme::text_muted())
                .whitespace_nowrap()
                .truncate()
                .child(free),
        )
        .into_any_element()
}

/// Disk images: the first few always, the rest sliding open below them.
fn image_section(app: &Burrdisk, images: Vec<Volume>, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let total = images.len();
    let collapsible = total > COLLAPSED_IMAGES + 1;
    let expanded = app.show_all_images || !collapsible;

    // 0 = collapsed, 1 = fully open, eased between while toggling.
    let progress = match app.images_toggled {
        Some(at) if at.elapsed() < IMAGES_TOGGLE => {
            window.request_animation_frame();
            let t = sunburst::ease_in_out_cubic(at.elapsed().as_secs_f32() / IMAGES_TOGGLE.as_secs_f32());
            if app.show_all_images { t } else { 1.0 - t }
        }
        _ => {
            if expanded { 1.0 } else { 0.0 }
        }
    };

    let mut images = images.into_iter().enumerate();
    let head: Vec<AnyElement> = images
        .by_ref()
        .take(if collapsible { COLLAPSED_IMAGES } else { total })
        .map(|(ix, v)| {
            let scanned = app.scanned_at(&v.mount).is_some();
            image_row(ix, v, scanned, cx)
        })
        .collect();
    let rest: Vec<(usize, Volume)> = images.collect();
    let rest_rows = rest.len().div_ceil(2) as f32;
    let rest_height = rest_rows * IMAGE_ROW + (rest_rows - 1.0).max(0.0) * IMAGE_GAP;
    let rest_count = rest.len();
    let rest: Vec<AnyElement> = rest
        .into_iter()
        .enumerate()
        .map(|(n, (ix, v))| {
            // Rows further down arrive a little later, like a drawer unrolling.
            let local = ((progress * (1.0 + rest_count as f32 * 0.03)) - n as f32 * 0.03).clamp(0.0, 1.0);
            div()
                .opacity(local)
                .child(image_row(ix, v.clone(), app.scanned_at(&v.mount).is_some(), cx))
                .into_any_element()
        })
        .collect();

    let label = if app.show_all_images { "Show less" } else { "Show all" };
    let toggle_t = fx::hover("toggle-images");

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
                        .items_center()
                        .child(section_label("Disk images"))
                        .child(
                            div()
                                .px_1p5()
                                .rounded_full()
                                .bg(theme::raised())
                                .text_xs()
                                .text_color(theme::text_muted())
                                .child(total.to_string()),
                        ),
                )
                .when(collapsible, |this| {
                    this.child(
                        div()
                            .id("toggle-images")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_0p5()
                            .rounded(px(7.))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .bg(fx::mix(gpui::transparent_black(), theme::raised(), toggle_t))
                            .text_color(fx::mix(theme::text_muted(), theme::text(), toggle_t))
                            .cursor_pointer()
                            .on_hover(fx::on_hover("toggle-images"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_all_images = !this.show_all_images;
                                this.images_toggled = Some(Instant::now());
                                cx.notify();
                            }))
                            .child(label)
                            .child(
                                // The chevron turns from pointing right to pointing down.
                                div().size(px(12.)).child(icon(
                                    if progress > 0.5 { Icon::ChevronDown } else { Icon::ChevronRight },
                                    12.0,
                                    fx::mix(theme::text_muted(), theme::text(), toggle_t),
                                )),
                            ),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(div().grid().grid_cols(2).gap(px(IMAGE_GAP)).children(head))
                .when(!rest.is_empty() && progress > 0.0, |s| {
                    s.child(
                        div()
                            .h(px((rest_height + IMAGE_GAP) * progress))
                            .pt(px(IMAGE_GAP))
                            .overflow_hidden()
                            .child(div().grid().grid_cols(2).gap(px(IMAGE_GAP)).children(rest)),
                    )
                }),
        )
}

fn image_row(ix: usize, volume: Volume, scanned: bool, cx: &mut Context<Burrdisk>) -> AnyElement {
    let target = volume.clone();
    let key = ("image", ix);
    let t = fx::hover(key);
    div()
        .id(key)
        .flex()
        .items_center()
        .gap_3()
        .h(px(IMAGE_ROW))
        .px_2()
        .rounded(px(12.))
        .border_1()
        .border_color(fx::mix(gpui::transparent_black(), theme::border(), t))
        .bg(fx::mix(gpui::transparent_black(), theme::surface(), t))
        .cursor_pointer()
        .on_hover(fx::on_hover(key))
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(30.))
                .rounded(px(9.))
                .bg(fx::mix(theme::surface(), theme::accent_soft(), t))
                .child(icon(Icon::Disc, 14.0, fx::mix(theme::text_faint(), theme::accent_hi(), t))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .whitespace_nowrap()
                .truncate()
                .child(volume.name.clone()),
        )
        .when(scanned, |s| s.child(div().flex_none().size(px(6.)).rounded_full().bg(theme::accent())))
        .child(
            div()
                .w(px(72.))
                .flex_none()
                .text_xs()
                .text_right()
                .whitespace_nowrap()
                .text_color(theme::text_muted())
                .child(format::bytes(volume.total)),
        )
        .child(div().flex_none().opacity(t).child(icon(Icon::ChevronRight, 12.0, theme::text_muted())))
        .into_any_element()
}
