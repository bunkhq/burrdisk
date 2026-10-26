use std::sync::atomic::Ordering;

use gpui::{Context, FontWeight, IntoElement, Window, canvas, div, prelude::*, px};

use crate::app::{Burrdisk, Screen};
use crate::format;
use crate::sunburst;
use crate::theme;
use crate::ui::components::{ButtonStyle, button};

pub fn render(app: &mut Burrdisk, window: &mut Window, cx: &mut Context<Burrdisk>) -> impl IntoElement {
    let Screen::Scanning(scanning) = &app.screen else {
        return div();
    };
    let files = scanning.progress.files.load(Ordering::Relaxed);
    let bytes = scanning.progress.bytes.load(Ordering::Relaxed);
    let current = scanning.progress.current.lock().map(|c| c.clone()).unwrap_or_default();
    let expected = scanning.target.volume.as_ref().map(|v| v.used()).filter(|u| *u > 0);
    let elapsed = scanning.started.elapsed().as_secs_f32();
    let name = scanning.target.name.clone();
    window.request_animation_frame();

    // Known volume size gives a real progress ring; folders get a gentle spinner.
    let (start, end) = match expected {
        Some(total) => (0.0, (bytes as f32 / total as f32).clamp(0.02, 0.98)),
        None => {
            let head = (elapsed * 0.9).fract();
            (head, head + 0.22)
        }
    };
    let ring_color = gpui::Hsla::from(theme::accent());

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(div().h(px(56.)).flex_none())
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(28.))
                .pb(px(56.))
                .child(
                    div()
                        .relative()
                        .size(px(200.))
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    sunburst::paint_arc(window, bounds, start, end, 10.0, ring_color, theme::track().into());
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
                                        .text_size(px(28.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format::bytes(bytes)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme::text_muted())
                                        .child(format!("{} files", format::count(files))),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Scanning {name}")),
                        )
                        .child(
                            div()
                                .w(px(520.))
                                .text_xs()
                                .text_center()
                                .text_color(theme::text_faint())
                                .truncate()
                                .child(current),
                        ),
                )
                .child(button(
                    "cancel-scan",
                    "Cancel",
                    ButtonStyle::Secondary,
                    cx.listener(|this, _, _, cx| this.cancel_scan(cx)),
                )),
        )
}
