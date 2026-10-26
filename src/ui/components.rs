use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Div, ElementId, Hsla, IntoElement, Rgba, SharedString,
    Stateful, Svg, Window, div, linear_color_stop, linear_gradient, point, prelude::*, px, rgb,
    rgba, svg,
};

use std::time::Duration;

use gpui::{Animation, AnimationExt, ease_out_quint};

use crate::theme;
use crate::ui::fx;

#[derive(Clone, Copy)]
pub enum Icon {
    ArrowUpRight,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    CornerUpLeft,
    Disc,
    File,
    Folder,
    FolderOpen,
    FolderSearch,
    HardDrive,
    Inbox,
    Network,
    Plus,
    Refresh,
    Scan,
    Trash,
    Usb,
    X,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Icon::ArrowUpRight => "icons/arrow-up-right.svg",
            Icon::ChevronDown => "icons/chevron-down.svg",
            Icon::ChevronLeft => "icons/chevron-left.svg",
            Icon::ChevronRight => "icons/chevron-right.svg",
            Icon::CornerUpLeft => "icons/corner-up-left.svg",
            Icon::Disc => "icons/disc.svg",
            Icon::File => "icons/file.svg",
            Icon::Folder => "icons/folder.svg",
            Icon::FolderOpen => "icons/folder-open.svg",
            Icon::FolderSearch => "icons/folder-search.svg",
            Icon::HardDrive => "icons/hard-drive.svg",
            Icon::Inbox => "icons/inbox.svg",
            Icon::Network => "icons/network.svg",
            Icon::Plus => "icons/plus.svg",
            Icon::Refresh => "icons/refresh.svg",
            Icon::Scan => "icons/scan.svg",
            Icon::Trash => "icons/trash.svg",
            Icon::Usb => "icons/usb.svg",
            Icon::X => "icons/x.svg",
        }
    }
}

pub fn icon(icon: Icon, size: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .path(icon.path())
        .size(px(size))
        .flex_none()
        .text_color(color.into())
}

#[derive(Clone, Copy, PartialEq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

fn shadow(color: Hsla, y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
    }
}

/// A soft colored halo under a raised element.
pub fn glow(color: impl Into<Hsla>, strength: f32) -> Vec<BoxShadow> {
    let color = color.into();
    vec![
        shadow(color.opacity(0.28 * strength), 6.0, 18.0, -4.0),
        shadow(gpui::black().opacity(0.35), 1.0, 2.0, 0.0),
    ]
}

/// Filled buttons are lit from above: a vertical gradient, a brighter top edge
/// and a halo in their own color that grows on hover.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    style: ButtonStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    styled_button(id, None, label.into(), style, on_click)
}

fn styled_button(
    id: impl Into<ElementId>,
    glyph: Option<Icon>,
    label: SharedString,
    style: ButtonStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let t = fx::hover(id.clone());
    let fg: Hsla = match style {
        ButtonStyle::Primary => rgb(0x1f0b00).into(),
        ButtonStyle::Danger => gpui::white(),
        ButtonStyle::Secondary => theme::text().into(),
        ButtonStyle::Ghost => fx::mix(theme::text_muted(), theme::text(), t),
    };
    let base = div()
        .id(id.clone())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap_1p5()
        .h(px(32.))
        .px(px(14.))
        .rounded(px(9.))
        .border_1()
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .cursor_pointer()
        .on_hover(fx::on_hover(id))
        .on_click(on_click)
        .when_some(glyph, |s, glyph| s.pl(px(11.)).child(icon(glyph, 14.0, fg)))
        .child(label);
    match style {
        ButtonStyle::Primary => filled(base, theme::accent_hi(), theme::accent_lo(), fg, theme::accent(), t),
        ButtonStyle::Danger => filled(base, rgb(0xf87171), rgb(0xdc2626), fg, theme::danger(), t),
        ButtonStyle::Secondary => base
            .bg(linear_gradient(
                180.,
                linear_color_stop(fx::mix(rgb(0x34302d), rgb(0x433d38), t), 0.),
                linear_color_stop(fx::mix(rgb(0x2a2725), rgb(0x36312d), t), 1.),
            ))
            .border_color(fx::mix(rgba(0xffffff1a), rgba(0xffffff33), t))
            .text_color(fg)
            .shadow(vec![shadow(gpui::black().opacity(0.3 + 0.1 * t), 1.0 + t, 2.0 + 4.0 * t, 0.0)])
            .active(|s| s.bg(rgb(0x262321))),
        ButtonStyle::Ghost => base
            .bg(fx::mix(gpui::transparent_black(), theme::surface_hover(), t))
            .border_color(gpui::transparent_black())
            .text_color(fg)
            .active(|s| s.bg(theme::raised())),
    }
}

fn filled(base: Stateful<Div>, hi: Rgba, lo: Rgba, fg: Hsla, tone: Rgba, t: f32) -> Stateful<Div> {
    let hi = Hsla::from(hi);
    let lo = Hsla::from(lo);
    let lift = |c: Hsla, by: f32| Hsla { l: (c.l + by).min(1.0), ..c };
    base.bg(linear_gradient(
        180.,
        linear_color_stop(lift(hi, 0.05 * t), 0.),
        linear_color_stop(lift(lo, 0.06 * t), 1.),
    ))
    .border_color(lift(hi, 0.08 + 0.06 * t).opacity(0.7))
    .text_color(fg)
    .shadow(glow(tone, 1.0 + 1.2 * t))
    .active(move |s| {
        s.bg(linear_gradient(180., linear_color_stop(lo, 0.), linear_color_stop(hi, 1.)))
            .shadow(glow(tone, 0.6))
    })
}

/// A button with a leading icon.
pub fn icon_label_button(
    id: impl Into<ElementId>,
    glyph: Icon,
    label: impl Into<SharedString>,
    style: ButtonStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    styled_button(id, Some(glyph), label.into(), style, on_click)
}

pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: Icon,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let t = fx::hover(id.clone());
    div()
        .id(id.clone())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(28.))
        .rounded(px(8.))
        .border_1()
        .border_color(fx::mix(gpui::transparent_black(), theme::border(), t))
        .bg(fx::mix(gpui::transparent_black(), theme::raised(), t))
        .cursor_pointer()
        .active(|s| s.bg(theme::accent_soft()))
        .on_hover(fx::on_hover(id))
        .on_click(on_click)
        .child(
            svg()
                .path(glyph.path())
                .size(px(15.))
                .text_color(fx::mix(theme::text_muted(), theme::text(), t)),
        )
}

/// A thin horizontal capacity bar that fills in when it first appears.
pub fn bar(id: impl Into<ElementId>, ratio: f32, color: impl Into<Hsla>, height: f32) -> AnyElement {
    let color = color.into();
    let ratio = ratio.clamp(0.0, 1.0);
    let dim = Hsla { l: (color.l - 0.12).max(0.0), ..color };
    div()
        .w_full()
        .h(px(height))
        .rounded_full()
        .bg(theme::track())
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(linear_gradient(90., linear_color_stop(dim, 0.), linear_color_stop(color, 1.)))
                .shadow(vec![shadow(color.opacity(0.45), 0.0, 8.0, -1.0)])
                .with_animation(
                    id,
                    Animation::new(Duration::from_millis(900)).with_easing(ease_out_quint()),
                    move |el, t| el.w(gpui::relative(ratio * t)),
                ),
        )
        .into_any_element()
}

/// A number with a smaller unit after it ("38.0 GB"), sharing one baseline.
/// Both boxes get a line height equal to their font size and are bottom-aligned;
/// the unit is then lifted by the difference in descent below the baseline.
pub fn amount(number: impl Into<SharedString>, unit: impl Into<SharedString>, size: f32, unit_size: f32) -> Div {
    const DESCENT: f32 = 0.145;
    let unit = unit.into();
    // A percent sign sits snug against its number; words like "GB" need air.
    let gap = if unit == "%" { size * 0.03 } else { size * 0.12 };
    div()
        .flex()
        .items_end()
        .gap(px(gap))
        .whitespace_nowrap()
        .child(
            div()
                .text_size(px(size))
                .line_height(px(size))
                .font_weight(gpui::FontWeight::BOLD)
                .child(number.into()),
        )
        .child(
            div()
                .text_size(px(unit_size))
                .line_height(px(unit_size))
                .mb(px(DESCENT * (size - unit_size)))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme::text_muted())
                .child(unit),
        )
}

pub fn section_label(text: impl Into<SharedString>) -> Div {
    div()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme::text_faint())
        .child(text.into().to_uppercase())
}
