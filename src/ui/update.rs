use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Context, FontWeight, Hsla, SharedString, Window, div, ease_out_quint,
    linear_color_stop, linear_gradient, prelude::*, rgb, rgba,
};

use crate::app::{Burrdisk, UpdatePhase};
use crate::format;
use crate::platform;
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, button, glow, icon, icon_label_button, shadow};
use crate::ui::{fx, px};
use crate::update;

const CARD_WIDTH: f32 = 328.;

/// The titlebar's update button, with its card below it when open. `None` while
/// there's nothing to say
pub fn button_and_card(app: &Burrdisk, window: &Window, cx: &mut Context<Burrdisk>) -> Option<AnyElement> {
    let updater = &app.updater;
    let release = updater.release.as_ref();
    let (label, dot, clickable): (SharedString, Option<Hsla>, bool) = match (&updater.phase, release) {
        (UpdatePhase::Checking, _) => ("Checking for updates\u{2026}".into(), None, false),
        (UpdatePhase::UpToDate(_), _) => ("Burrdisk is up to date".into(), None, false),
        (UpdatePhase::Downloading(progress), _) => {
            (format!("Downloading\u{2026} {}%", percent(progress)).into(), Some(theme::accent().into()), true)
        }
        (UpdatePhase::Installing, _) => ("Installing\u{2026}".into(), Some(theme::accent().into()), true),
        (UpdatePhase::Failed(_), None) => ("Couldn\u{2019}t check for updates".into(), Some(theme::text_faint().into()), true),
        (UpdatePhase::Failed(_), Some(_)) => ("Update didn\u{2019}t finish".into(), Some(theme::text_faint().into()), true),
        (_, Some(release)) => (format!("Update to {}", release.version).into(), Some(theme::accent().into()), true),
        (_, None) => return None,
    };

    let id = "update-button";
    let t = if clickable { fx::hover(id) } else { 0.0 };
    let open = updater.open && clickable;
    let ink = if open { theme::text().into() } else { fx::mix(theme::text_muted(), theme::text(), t) };
    let pill = div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .h(px(28.))
        .px(px(10.))
        .relative()
        .rounded(px(9.))
        .bg(if open { theme::raised().into() } else { fx::mix(gpui::transparent_black(), theme::surface_hover(), t) })
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .child(fx::shine(t, gpui::white().opacity(0.34), px(9.)))
        .text_color(if clickable { ink } else { theme::text_faint().into() })
        .when(matches!(updater.phase, UpdatePhase::UpToDate(_)), |s| s.child(icon(Icon::Check, 13.0, theme::text_faint())))
        .when_some(dot, |s, dot| {
            s.child(div().flex_none().size(px(7.)).rounded_full().bg(dot).when(dot != theme::text_faint().into(), |s| {
                s.shadow(vec![shadow(dot.opacity(0.7), 0.0, 6.0, 0.0)])
            }))
        })
        .child(label)
        .when(clickable, |s| {
            s.cursor_pointer()
                .on_hover(fx::on_hover(id))
                .on_mouse_move(fx::on_move)
                .on_click(cx.listener(|this, _, _, cx| this.toggle_update_card(cx)))
        })
        // Fades in where it appears, so a new release arrives without a jolt
        .with_animation(
            SharedString::from(format!("update-button-in-{:?}", updater.since)),
            Animation::new(Duration::from_millis(420)).with_easing(ease_out_quint()),
            |el, t| el.opacity(t),
        );

    Some(
        div()
            .relative()
            .flex_none()
            .occlude()
            .child(pill)
            .when(open, |s| s.child(card(app, window, cx)))
            .into_any_element(),
    )
}

fn percent(progress: &update::Progress) -> u64 {
    let total = progress.total.load(Ordering::Relaxed).max(1);
    (progress.done.load(Ordering::Relaxed) * 100 / total).min(100)
}

/// The card: what's new, what restarting means, and the one action
fn card(app: &Burrdisk, window: &Window, cx: &mut Context<Burrdisk>) -> AnyElement {
    let updater = &app.updater;
    let release = updater.release.as_ref();
    let blocker = update::blocker();

    let heading = match (&updater.phase, release) {
        (UpdatePhase::Failed(_), None) => "Couldn\u{2019}t check for updates".to_string(),
        (_, Some(release)) => format!("Burrdisk {}", release.version),
        _ => "Burrdisk".to_string(),
    };
    let detail = match release {
        Some(release) => {
            let mut parts = vec![format!("You have {}", update::Version::current())];
            if let Some(asset) = &release.asset {
                parts.push(format!("{} download", format::bytes(asset.size)));
            }
            parts.join(" \u{b7} ")
        }
        None => format!("You have {}", update::Version::current()),
    };

    let head = div()
        .flex()
        .items_start()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(30.))
                .rounded_full()
                .bg(theme::accent_soft())
                .child(icon(Icon::Download, 15.0, theme::accent_hi())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .gap(px(3.))
                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).line_height(px(20.)).child(heading))
                .child(div().text_xs().text_color(theme::text_faint()).child(detail)),
        );

    let notes = release.filter(|r| !r.summary.is_empty()).map(|release| {
        let page = release.page.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_size(px(13.)).line_height(px(19.)).text_color(theme::text_muted()).child(release.summary.clone()))
            .child(link("update-notes", "What\u{2019}s New", move |_, _, cx| cx.open_url(&page)))
    });

    // What restarting costs, said before the button rather than discovered after
    let mut cautions: Vec<(Icon, String)> = Vec::new();
    if let Some(why) = &blocker {
        cautions.push((Icon::ShieldAlert, format!("Burrdisk can\u{2019}t update itself here: {why}")));
    } else if release.is_some_and(|r| r.asset.is_none()) {
        cautions.push((Icon::ShieldAlert, "This release has no download for this computer yet".into()));
    } else if release.is_some() && !matches!(updater.phase, UpdatePhase::Failed(_)) {
        if app.update_loses_scans() {
            cautions.push((Icon::Refresh, "Burrdisk restarts to update, so scans will need to run again".into()));
        }
        if cfg!(target_os = "macos") && app.full_disk_access {
            cautions.push((Icon::ShieldCheck, "macOS will ask for Full Disk Access again after the update".into()));
        }
    }
    if let UpdatePhase::Failed(err) = &updater.phase {
        cautions.push((Icon::X, sentence(err)));
    }
    let cautions = (!cautions.is_empty()).then(|| {
        div().flex().flex_col().gap(px(6.)).children(cautions.into_iter().map(|(glyph, text)| {
            div()
                .flex()
                .items_start()
                .gap(px(8.))
                .child(div().pt(px(2.)).child(icon(glyph, 12.0, theme::text_faint())))
                .child(div().flex_1().min_w_0().text_xs().line_height(px(17.)).text_color(theme::text_muted()).child(text))
        }))
    });

    let page = release.map_or(update::RELEASES_PAGE.to_string(), |r| r.page.clone());
    let can_install = blocker.is_none() && release.is_some_and(|r| r.asset.is_some());
    let trash_busy = app.update_blocked_by_trash();
    let foot = match &updater.phase {
        UpdatePhase::Downloading(progress) => progress_foot(progress, cx),
        UpdatePhase::Installing => installing_foot(),
        UpdatePhase::Failed(_) if release.is_none() => actions(
            button("update-close", "Close", ButtonStyle::Ghost, cx.listener(|this, _, _, cx| this.close_update_card(cx))),
            button("update-retry", "Try Again", ButtonStyle::Primary, cx.listener(|this, _, _, cx| this.check_for_updates(cx))),
        ),
        UpdatePhase::Failed(_) if can_install => actions(
            button("update-page", "Download Manually", ButtonStyle::Ghost, move |_, _, cx| cx.open_url(&page)),
            button("update-retry", "Try Again", ButtonStyle::Primary, cx.listener(|this, _, _, cx| this.install_update(cx))),
        ),
        _ if can_install => actions(
            button("update-later", "Later", ButtonStyle::Ghost, cx.listener(|this, _, _, cx| this.close_update_card(cx))),
            if trash_busy {
                // A Trash move can't be cut short by a restart
                button("update-go", format!("Waiting for {}\u{2026}", platform::TRASH), ButtonStyle::Secondary, |_, _, _| {})
                    .opacity(0.6)
                    .cursor_default()
            } else {
                icon_label_button(
                    "update-go",
                    Icon::Download,
                    "Update and Restart",
                    ButtonStyle::Primary,
                    cx.listener(|this, _, _, cx| this.install_update(cx)),
                )
            },
        ),
        _ => actions(
            button("update-later", "Later", ButtonStyle::Ghost, cx.listener(|this, _, _, cx| this.close_update_card(cx))),
            icon_label_button("update-page", Icon::ArrowUpRight, "Download", ButtonStyle::Primary, move |_, _, cx| {
                cx.open_url(&page)
            }),
        ),
    };

    let viewport = window.viewport_size();
    let body = div()
        .id("update-card")
        .mt(px(8.))
        .w(px(CARD_WIDTH))
        .flex()
        .flex_col()
        .gap(px(14.))
        .p(px(16.))
        .rounded(px(14.))
        .bg(rgb(0x262220))
        .border_1()
        .border_color(rgba(0xffffff1f))
        .shadow(vec![
            shadow(gpui::black().opacity(0.45), 14.0, 36.0, -8.0),
            shadow(gpui::black().opacity(0.3), 1.0, 3.0, 0.0),
        ])
        .occlude()
        .cursor_default()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(head)
        .children(notes)
        .children(cautions)
        .child(foot)
        .with_animation(
            "update-card-in",
            Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()),
            |el, t| el.opacity(t).mt(px(14. - 6. * t)),
        );

    div()
        .absolute()
        .top_full()
        .right_0()
        .child(
            // Clicking anywhere else puts the card away, as "Later" does
            gpui::deferred(
                gpui::anchored().position(gpui::point(px(0.), px(0.))).child(
                    div()
                        .id("update-backdrop")
                        .w(viewport.width)
                        .h(viewport.height)
                        .occlude()
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.close_update_card(cx))),
                ),
            )
            .with_priority(1),
        )
        .child(
            gpui::deferred(
                gpui::anchored().anchor(gpui::Corner::TopRight).snap_to_window_with_margin(px(8.)).child(body),
            )
            .with_priority(2),
        )
        .into_any_element()
}

/// Capitalizes an error from the updater and drops any final period: "Couldn't reach GitHub"
fn sentence(err: &str) -> String {
    let mut chars = err.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    let text = format!("{first}{}", chars.as_str());
    text.strip_suffix('.').map(str::to_string).unwrap_or(text)
}

fn actions(secondary: impl IntoElement, primary: impl IntoElement) -> AnyElement {
    div().flex().items_center().justify_end().gap_2().pt(px(2.)).child(secondary).child(primary).into_any_element()
}

fn progress_foot(progress: &update::Progress, cx: &mut Context<Burrdisk>) -> AnyElement {
    let done = progress.done.load(Ordering::Relaxed);
    let total = progress.total.load(Ordering::Relaxed);
    let fraction = if total == 0 { 0.0 } else { (done as f32 / total as f32).clamp(0.0, 1.0) };
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .pt(px(2.))
        .child(bar(fraction))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::text_muted())
                        .child(format!("Downloading {} of {}", format::bytes(done), format::bytes(total))),
                )
                .child(
                    button("update-cancel", "Cancel", ButtonStyle::Ghost, cx.listener(|this, _, _, cx| this.cancel_update(cx)))
                        .h(px(26.))
                        .px(px(10.)),
                ),
        )
        .into_any_element()
}

fn installing_foot() -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .pt(px(2.))
        .child(bar(1.0))
        .child(
            div()
                .h(px(26.))
                .flex()
                .items_center()
                .text_xs()
                .text_color(theme::text_muted())
                .child("Installing. Burrdisk will reopen in a moment\u{2026}"),
        )
        .into_any_element()
}

/// A 6px bar in Ember over the track, lit like the filled buttons
fn bar(fraction: f32) -> impl IntoElement {
    div().h(px(6.)).w_full().rounded_full().bg(theme::track()).child(
        div()
            .h_full()
            .w(gpui::relative(fraction.max(0.02)))
            .rounded_full()
            .bg(linear_gradient(90., linear_color_stop(theme::accent_lo(), 0.), linear_color_stop(theme::accent_hi(), 1.)))
            .shadow(glow(theme::accent(), 0.6)),
    )
}

/// An Ember Hi text link that leaves the app, styled like home's "Turn On…"
fn link(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let t = fx::hover(id);
    let color = fx::mix(theme::accent_hi(), theme::accent(), t);
    div().flex().child(
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_1()
            .ml(px(-6.))
            .px_1p5()
            .py(px(2.))
            .rounded(px(6.))
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color)
            .bg(fx::mix(gpui::transparent_black(), theme::accent_soft(), t))
            .cursor_pointer()
            .on_hover(fx::on_hover(id))
            .on_click(on_click)
            .child(label)
            .child(icon(Icon::ArrowUpRight, 12.0, color)),
    )
}
