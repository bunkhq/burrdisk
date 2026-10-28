use gpui::{Context, FontWeight, IntoElement, Window, div, prelude::*, px};

use crate::app::{Burrdisk, Screen};
use crate::theme;
use crate::ui::components::{ButtonStyle, Icon, button, icon, icon_button, icon_label_button};
use crate::ui::home::{fill_burr, rise};
use crate::volumes;

/// The step before scanning a disk macOS partly hides: why Full Disk Access
/// matters, how to turn it on, and a way to scan regardless
pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let Screen::Access(target) = &app.screen else {
        return div().into_any_element();
    };
    let grow = (app.access_since.elapsed().as_secs_f32() / 1.3).min(1.0);
    if grow < 1.0 {
        window.request_animation_frame();
    }
    let granted = app.full_disk_access;
    let name = target.name.clone();

    let steps = [
        "Open Full Disk Access in System Settings.",
        "Turn on Burrdisk. If it isn\u{2019}t listed, click + and choose Burrdisk.",
        "When macOS asks, choose Quit & Reopen, then scan again.",
    ];
    let step_rows = steps.into_iter().enumerate().map(|(ix, text)| {
        div()
            .flex()
            .items_start()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(22.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::border_strong())
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text_muted())
                    .child((ix + 1).to_string()),
            )
            .child(div().pt(px(2.)).text_sm().text_color(theme::text()).child(text))
    });

    let actions = if granted {
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(icon_label_button(
                "access-scan",
                Icon::Scan,
                format!("Scan {name}"),
                ButtonStyle::Primary,
                cx.listener(|this, _, _, cx| this.scan_without_access(cx)),
            ))
    } else {
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(icon_label_button(
                "access-open",
                Icon::ShieldAlert,
                "Open System Settings",
                ButtonStyle::Primary,
                |_, _, cx| cx.open_url(volumes::FULL_DISK_ACCESS_SETTINGS),
            ))
            .child(button(
                "access-skip",
                "Scan Without Access",
                ButtonStyle::Ghost,
                cx.listener(|this, _, _, cx| this.scan_without_access(cx)),
            ))
    };

    let status = if granted {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .text_color(theme::accent_hi())
            .child(icon(Icon::ShieldCheck, 15.0, theme::accent_hi()))
            .child("Full Disk Access is on. Burrdisk can see the whole disk.")
    } else {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .text_color(theme::text_faint())
            .child("Burrdisk only reads sizes. Nothing is opened, uploaded or changed.")
    };

    let copy = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .max_w(px(500.))
        .gap_5()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(26.))
                        .line_height(px(32.))
                        .font_weight(FontWeight::BOLD)
                        .child("Let Burrdisk see the whole disk"),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .line_height(px(22.))
                        .text_color(theme::text_muted())
                        .child(
                            "macOS keeps Mail, Messages, Safari, Photos and other apps\u{2019} data \
                             out of view until you allow Full Disk Access. Without it, all of that \
                             lands in one Hidden space slice you can\u{2019}t open or clean up.",
                        ),
                ),
        )
        .child(div().flex().flex_col().gap_3().children(step_rows))
        .child(div().flex().flex_col().gap_4().mt_1().child(actions).child(status));

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .h(px(56.))
                .pl(px(92.))
                .pr(px(16.))
                .child(icon_button("access-back", Icon::ChevronLeft, cx.listener(|this, _, _, cx| this.go_home(cx))))
                .child(div().text_sm().text_color(theme::text_muted()).child("Disks and Folders")),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .px(px(44.))
                .pb(px(56.))
                .child(rise(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(48.))
                        .when_some(target.volume.as_ref(), |s, volume| {
                            s.child(fill_burr("access-burr", volume, 180.0, 60, grow))
                        })
                        .child(copy),
                    "rise-access",
                    0,
                )),
        )
        .into_any_element()
}
