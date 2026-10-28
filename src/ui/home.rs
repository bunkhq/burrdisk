use std::time::{Duration, Instant};

use gpui::{
    Animation, AnimationExt, AnyElement, Context, ElementId, FontWeight, Hsla, IntoElement,
    SharedString, Window, WindowControlArea, canvas, div, prelude::*,
};
use crate::ui::px;

use crate::app::{Burrdisk, Lobe};
use crate::format;
use crate::platform;
use crate::sunburst::{self, Spine};
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, MenuItem, icon, icon_button, icon_label_button, popup_menu, split_button, window_controls};
use crate::ui::fx;
use crate::volumes::{self, Volume, VolumeKind};

const COLLAPSED_IMAGES: usize = 6;
const GROW: Duration = Duration::from_millis(1300);
const IMAGES_TOGGLE: Duration = Duration::from_millis(380);
const IMAGE_ROW: f32 = 40.0;
const IMAGE_GAP: f32 = 4.0;

/// Fades and lifts a section into place, `order` beats after the page appears.
pub fn rise(el: impl IntoElement, id: impl Into<ElementId>, order: usize) -> AnyElement {
    let delay = order as f32 * 0.08;
    let total = delay + 0.5;
    div()
        .relative()
        .child(el.into_any_element())
        .with_animation(id, Animation::new(Duration::from_secs_f32(total)), move |el, t| {
            let local = sunburst::ease_out_cubic(((t * total - delay) / 0.5).clamp(0.0, 1.0));
            el.opacity(local).top(px(10. * (1. - local)))
        })
        .into_any_element()
}

fn ago(at: Instant, took: Duration, admin: bool) -> String {
    let who = if admin { " as administrator" } else { "" };
    let when = match at.elapsed().as_secs() {
        s if s < 60 => "just now".to_string(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s => format!("{} h ago", s / 3600),
    };
    format!("Scanned{who} {when} in {}", format::duration(took))
}

/// What the home screen knows about a disk beyond its volume figures.
#[derive(Clone)]
struct Known {
    scanned: Option<Instant>,
    /// How long the cached scan took.
    took: Duration,
    admin: bool,
    lobes: Vec<Lobe>,
}

fn known(app: &Burrdisk, volume: &Volume) -> Known {
    let scan = app.scans.get(&volume.mount);
    Known {
        scanned: scan.map(|s| s.scanned_at),
        took: scan.map_or(Duration::ZERO, |s| s.tree.duration),
        admin: scan.is_some_and(|s| s.target.admin),
        lobes: scan.map(|s| s.lobes.clone()).unwrap_or_default(),
    }
}

/// The spines of a disk's burr. A scanned disk grows one cluster of spines per
/// top-level folder, in its chart color, so the burr previews what's inside. An
/// unscanned one shows how full it is: lit spines for used space, stubs for free.
fn spines(volume: &Volume, known: &Known, count: usize) -> Vec<Spine> {
    if !known.lobes.is_empty() {
        let widest = known.lobes.iter().map(|l| l.end - l.start).fold(0.0f32, f32::max).max(1e-3);
        let mut out = Vec::new();
        for lobe in &known.lobes {
            let span = lobe.end - lobe.start;
            let n = ((span * count as f32).round() as usize).max(1);
            let reach = 0.5 + 0.5 * (span / widest).sqrt();
            for j in 0..n {
                let u = (j as f32 + 0.5) / n as f32;
                // Each folder bulges in its middle, so neighbors read as separate lobes.
                let profile = 0.72 + 0.28 * (u * std::f32::consts::PI).sin();
                out.push(Spine { at: lobe.start + span * u, len: reach * profile, color: lobe.color });
            }
        }
        return out;
    }
    let ratio = volume.fill_ratio();
    let lit = theme::fullness(ratio);
    let dim: Hsla = theme::border_strong().into();
    (0..count)
        .map(|i| {
            let at = (i as f32 + 0.5) / count as f32;
            if at < ratio {
                Spine { at, len: 0.92 + 0.08 * ((i * 7 % 5) as f32 / 4.0), color: lit }
            } else {
                Spine { at, len: 0.34, color: dim }
            }
        })
        .collect()
}

/// A disk's burr as it looks before a scan: lit spines for the space in use.
pub fn fill_burr(id: impl Into<ElementId>, volume: &Volume, size: f32, count: usize, grow: f32) -> impl IntoElement {
    let known = Known { scanned: None, took: Duration::ZERO, admin: false, lobes: Vec::new() };
    let spines = spines(volume, &known, count);
    let core = gpui::hsla(0.07, 0.14, 0.13, 1.0);
    div().id(id).flex_none().size(px(size)).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| sunburst::paint_burr(window, bounds, &spines, core, None, grow, 0.0),
        )
        .size_full(),
    )
}

fn burr(
    id: impl Into<ElementId>,
    volume: &Volume,
    known: &Known,
    size: f32,
    count: usize,
    grow: f32,
    bristle: f32,
) -> impl IntoElement {
    let spines = spines(volume, known, count);
    let rim = known.scanned.map(|_| Hsla::from(theme::accent()));
    let core = gpui::hsla(0.07, 0.14, 0.13, 1.0);
    div().id(id).flex_none().size(px(size)).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| sunburst::paint_burr(window, bounds, &spines, core, rim, grow, bristle),
        )
        .size_full(),
    )
}

pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let grow = (app.home_since.elapsed().as_secs_f32() / GROW.as_secs_f32()).min(1.0);
    let update = crate::ui::update::button_and_card(app, window, cx);
    if grow < 1.0 {
        window.request_animation_frame();
    }
    // Display type follows the window a little, so the lead disk neither crowds nor floats.
    let scale = (f32::from(window.viewport_size().width) / (1180.0 * crate::ui::UI_SCALE)).clamp(0.82, 1.12);

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

    let mut body = div().flex().flex_col().gap(px(52.)).w_full().max_w(px(960.)).mx_auto();

    if let Some(volume) = startup {
        let known = known(app, &volume);
        body = body.child(rise(lead(app, volume, known, grow, scale, window, cx), "rise-lead", 0));
    } else if app.volumes_loaded {
        body = body.child(div().text_color(theme::text_muted()).child("No startup disk found."));
    }

    if !drives.is_empty() {
        let items: Vec<AnyElement> = drives
            .into_iter()
            .enumerate()
            .map(|(ix, v)| {
                let known = known(app, &v);
                drive(app, ix, v, known, grow, window, cx)
            })
            .collect();
        body = body.child(rise(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(heading("Other drives", None))
                .child(div().flex().flex_wrap().gap_x(px(28.)).gap_y_5().children(items)),
            "rise-drives",
            1,
        ));
    }

    if !images.is_empty() {
        body = body.child(rise(image_section(app, images, window, cx), "rise-images", 2));
    }

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(titlebar(grow, update, window, cx))
        .child(
            div()
                .id("home-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px(px(44.))
                .pt(px(12.))
                .pb(px(56.))
                .child(body),
        )
}

fn heading(text: &'static str, count: Option<usize>) -> impl IntoElement {
    div()
        .flex()
        .items_baseline()
        .gap_2()
        .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(text))
        .when_some(count, |s, n| s.child(div().text_sm().text_color(theme::text_faint()).child(n.to_string())))
}

fn titlebar(grow: f32, update: Option<AnyElement>, window: &Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    // The wordmark's own little burr, all brand orange.
    let mark: Vec<Spine> = (0..14)
        .map(|i| Spine { at: i as f32 / 14.0, len: 0.95, color: theme::accent().into() })
        .collect();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .h(px(56.))
        .pl(px(platform::TITLEBAR_INSET + 20.))
        .pr(px(if cfg!(windows) { platform::TITLEBAR_END } else { 20. }))
        .window_control_area(WindowControlArea::Drag)
        // Keeps the root's focus-on-click from claiming presses here, which would stop
        // Windows from moving (or, on double-click, maximizing) the window.
        .occlude()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div().size(px(20.)).child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                sunburst::paint_burr(window, bounds, &mark, theme::accent().into(), None, grow, 0.0)
                            },
                        )
                        .size_full(),
                    ),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme::text())
                        .child("Burrdisk"),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .h_full()
                .gap(px(12.))
                .children(update)
                .child(
                    icon_label_button(
                        "scan-folder",
                        Icon::FolderSearch,
                        "Scan Folder…",
                        ButtonStyle::Secondary,
                        cx.listener(|this, _, _, cx| this.pick_folder(cx)),
                    )
                    .occlude(),
                )
                .child(window_controls(window)),
        )
}

/// The disk actions menu, shared by every disk's chevron.
fn disk_menu(app: &Burrdisk, volume: &Volume, known: &Known, window: &Window, cx: &mut Context<Burrdisk>) -> Option<AnyElement> {
    if app.menu.as_ref() != Some(&volume.mount) {
        return None;
    }
    let entity = cx.entity();
    let act = |f: fn(&mut Burrdisk, Volume, &mut Context<Burrdisk>)| {
        let entity = entity.clone();
        let volume = volume.clone();
        move |_: &mut Window, cx: &mut gpui::App| {
            let volume = volume.clone();
            entity.update(cx, |this, cx| f(this, volume, cx));
        }
    };
    let mount = volume.mount.clone();
    let info = volume.mount.clone();
    // A disk that hasn't been scanned yet gets scanned, not rescanned.
    let verb = if known.scanned.is_some() { "Rescan" } else { "Scan" };
    let items = vec![
        MenuItem::new(verb, act(|this, v, cx| {
            let admin = this.scanned_as_admin(&v.mount);
            this.rescan_volume(v, admin, cx)
        })),
        if known.admin {
            MenuItem::new(format!("{verb} as Normal User"), act(|this, v, cx| this.rescan_volume(v, false, cx)))
        } else {
            MenuItem::new(format!("{verb} as Administrator"), act(|this, v, cx| this.rescan_volume(v, true, cx)))
        },
        MenuItem::separator(),
        MenuItem::new("Forget Scan Result", act(|this, v, cx| this.forget_scan(&v.mount, cx)))
            .disabled(known.scanned.is_none()),
        MenuItem::separator(),
        MenuItem::new(format!("Show in {}", platform::FILE_MANAGER), move |_, cx| cx.open_with_system(&mount)),
        MenuItem::new(platform::GET_INFO, move |_, _| volumes::get_info(&info)),
    ];
    let dismiss = entity.clone();
    Some(popup_menu(
        &format!("disk-{}", volume.mount.display()),
        items,
        window,
        move |_, cx| dismiss.update(cx, |this, cx| this.close_menu(cx)),
    ))
}

/// Scan or Open, with the chevron for the rest of the disk's actions.
fn scan_button(
    app: &Burrdisk,
    volume: &Volume,
    known: &Known,
    style: ButtonStyle,
    window: &Window,
    cx: &mut Context<Burrdisk>,
) -> impl IntoElement {
    let id = format!("scan-{}", volume.mount.display());
    let open = volume.clone();
    let mount = volume.mount.clone();
    let (glyph, label) = if known.scanned.is_some() { (Icon::ArrowUpRight, "Open") } else { (Icon::Scan, "Scan") };
    div()
        .relative()
        .flex_none()
        .child(split_button(
            &id,
            glyph,
            label,
            style,
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.scan_volume(open.clone(), cx)
            }),
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(mount.clone(), cx)
            }),
        ))
        .children(disk_menu(app, volume, known, window, cx))
}

/// The startup disk: a big burr beside its name, capacity and scan button.
fn lead(
    app: &Burrdisk,
    volume: Volume,
    known: Known,
    grow: f32,
    scale: f32,
    window: &Window,
    cx: &mut Context<Burrdisk>,
) -> impl IntoElement {
    let ratio = volume.fill_ratio();
    let bristle = fx::hover("lead-burr");
    let used = volume.total.saturating_sub(volume.available);
    let mut details = vec![format!("{} free", format::bytes(volume.available))];
    if volume.purgeable() > 0 {
        details.push(format!("{} purgeable", format::bytes(volume.purgeable())));
    }
    details.push(volume.file_system.to_uppercase());
    let target = volume.clone();
    let size = 300.0 * scale;
    let percent = format!("{:.0}%", ratio * 100.0 * sunburst::ease_out_cubic(grow));

    div()
        .flex()
        .items_center()
        .gap(px(44. * scale))
        .child(
            div()
                .id("lead-burr")
                .relative()
                .flex_none()
                .cursor_pointer()
                .on_hover(fx::on_hover("lead-burr"))
                .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
                .child(burr("lead-burr-art", &volume, &known, size, 84, grow, bristle))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(26. * scale))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme::text())
                        .child(percent),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(34. * scale))
                        .line_height(px(40. * scale))
                        .font_weight(FontWeight::BOLD)
                        .whitespace_nowrap()
                        .truncate()
                        .child(volume.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_1p5()
                        .text_size(px(17. * scale))
                        .whitespace_nowrap()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(format::bytes(used)))
                        .child(
                            div()
                                .text_color(theme::text_muted())
                                .child(format!("used of {}", format::bytes(volume.total))),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::text_muted())
                        .whitespace_nowrap()
                        .truncate()
                        .child(details.join("  ·  ")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .mt(px(18.))
                        .child(scan_button(app, &volume, &known, ButtonStyle::Primary, window, cx))
                        .when_some(known.scanned, |s, at| {
                            s.child(div().text_xs().text_color(theme::accent_hi()).whitespace_nowrap().child(ago(at, known.took, known.admin)))
                        }),
                )
                .when(!app.full_disk_access && volumes::needs_full_disk_access(&volume.mount), |s| {
                    s.child(access_notice())
                }),
        )
}

/// One line under the startup disk while macOS keeps protected folders from the scan.
fn access_notice() -> impl IntoElement {
    let t = fx::hover("access-link");
    div()
        .flex()
        .items_center()
        .gap_2()
        .mt(px(10.))
        .text_sm()
        .child(icon(Icon::ShieldAlert, 15.0, theme::accent_hi()))
        .child(
            div()
                .text_color(theme::text_muted())
                .whitespace_nowrap()
                .truncate()
                .min_w_0()
                .child("Full Disk Access is off, so part of this disk will be hidden."),
        )
        .child(
            div()
                .id("access-link")
                .flex_none()
                .px_1p5()
                .rounded(px(6.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fx::mix(theme::accent_hi(), theme::accent(), t))
                .bg(fx::mix(gpui::transparent_black(), theme::accent_soft(), t))
                .cursor_pointer()
                .on_hover(fx::on_hover("access-link"))
                .on_click(|_, _, cx| cx.open_url(volumes::FULL_DISK_ACCESS_SETTINGS))
                .child("Turn On\u{2026}"),
        )
}

fn drive(
    app: &Burrdisk,
    ix: usize,
    volume: Volume,
    known: Known,
    grow: f32,
    window: &Window,
    cx: &mut Context<Burrdisk>,
) -> AnyElement {
    let key = ("drive", ix);
    let bristle = fx::hover(key);
    let glyph = match volume.kind {
        VolumeKind::Network => Icon::Network,
        VolumeKind::External => Icon::Usb,
        _ => Icon::HardDrive,
    };
    let target = volume.clone();
    div()
        .flex()
        .items_center()
        .gap_3()
        .w(px(296.))
        .child(
            div()
                .id(key)
                .cursor_pointer()
                .on_hover(fx::on_hover(key))
                .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
                .child(burr(("drive-burr", ix), &volume, &known, 104.0, 40, grow, bristle)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(icon(glyph, 13.0, theme::text_faint()))
                        .child(
                            div()
                                .min_w_0()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .truncate()
                                .child(volume.name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::text_muted())
                        .whitespace_nowrap()
                        .truncate()
                        .child(format!("{} free of {}", format::bytes(volume.available), format::bytes(volume.total))),
                )
                .child(div().mt_2().flex().child(scan_button(app, &volume, &known, ButtonStyle::Secondary, window, cx))),
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
            let known = known(app, &v);
            image_row(app, ix, v, known, window, cx)
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
            let known = known(app, &v);
            div()
                .opacity(local)
                .child(image_row(app, ix, v, known, window, cx))
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
                .child(heading("Disk images", Some(total)))
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
                .child(div().grid().grid_cols(2).gap_x(px(24.)).gap_y(px(IMAGE_GAP)).children(head))
                .when(!rest.is_empty() && progress > 0.0, |s| {
                    s.child(
                        div()
                            .h(px((rest_height + IMAGE_GAP) * progress))
                            .pt(px(IMAGE_GAP))
                            .overflow_hidden()
                            .child(div().grid().grid_cols(2).gap_x(px(24.)).gap_y(px(IMAGE_GAP)).children(rest)),
                    )
                }),
        )
}

/// Each disk image keeps a hue of its own. Hovering only raises its lightness and
/// chroma, so the icon never sweeps through other hues on the way.
fn image_color(ix: usize, t: f32) -> Hsla {
    let hue = (52.0 + ix as f32 * 137.5).rem_euclid(360.0);
    sunburst::oklch(0.58 + 0.16 * t, 0.035 + 0.11 * t, hue)
}

fn image_row(app: &Burrdisk, ix: usize, volume: Volume, known: Known, window: &Window, cx: &mut Context<Burrdisk>) -> AnyElement {
    let target = volume.clone();
    let mount = volume.mount.clone();
    let key = ("image", ix);
    let t = fx::hover(key);
    let menu_open = app.menu.as_ref() == Some(&volume.mount);
    let menu = disk_menu(app, &volume, &known, window, cx);
    let name: SharedString = volume.name.clone().into();
    div()
        .id(key)
        .flex()
        .items_center()
        .gap_3()
        .h(px(IMAGE_ROW))
        .pl_2()
        .pr_1()
        .rounded(px(10.))
        .bg(fx::mix(gpui::transparent_black(), theme::surface(), t))
        .cursor_pointer()
        .on_hover(fx::on_hover(key))
        .on_click(cx.listener(move |this, _, _, cx| this.scan_volume(target.clone(), cx)))
        .child(icon(Icon::Disc, 14.0, image_color(ix, t)))
        .child(div().flex_1().min_w_0().text_sm().whitespace_nowrap().truncate().child(name))
        .when(known.scanned.is_some(), |s| {
            s.child(div().flex_none().size(px(6.)).rounded_full().bg(theme::accent()))
        })
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
        .child(
            div()
                .relative()
                .flex_none()
                .opacity(if menu_open { 1.0 } else { 0.35 + 0.65 * t })
                .child(
                    icon_button(
                        SharedString::from(format!("image-menu-{ix}")),
                        Icon::ChevronDown,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_menu(mount.clone(), cx)
                        }),
                    )
                    .size(px(26.)),
                )
                .children(menu),
        )
        .into_any_element()
}
