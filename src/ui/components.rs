use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Div, ElementId, Hsla, IntoElement, Rgba, SharedString,
    Stateful, Svg, Window, div, linear_color_stop, linear_gradient, point, prelude::*, px, rgb,
    rgba, svg,
};

use std::time::Duration;

use gpui::{Animation, AnimationExt, ease_out_quint};

use crate::theme;

#[derive(Clone, Copy)]
pub enum Icon {
    ArrowUpRight,
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
    let fg: Hsla = match style {
        ButtonStyle::Primary => rgb(0x1f0b00).into(),
        ButtonStyle::Danger => gpui::white(),
        ButtonStyle::Secondary => theme::text().into(),
        ButtonStyle::Ghost => theme::text_muted().into(),
    };
    let base = div()
        .id(id.into())
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
        .cursor_pointer()
        .on_click(on_click)
        .when_some(glyph, |s, glyph| s.pl(px(11.)).child(icon(glyph, 14.0, fg)))
        .child(label);
    match style {
        ButtonStyle::Primary => filled(base, theme::accent_hi(), theme::accent_lo(), rgb(0x1f0b00), theme::accent()),
        ButtonStyle::Danger => filled(base, rgb(0xf87171), rgb(0xdc2626), rgb(0xffffff), theme::danger()),
        ButtonStyle::Secondary => base
            .bg(linear_gradient(180., linear_color_stop(rgb(0x34302d), 0.), linear_color_stop(rgb(0x2a2725), 1.)))
            .border_color(rgba(0xffffff1a))
            .text_color(theme::text())
            .shadow(vec![shadow(gpui::black().opacity(0.3), 1.0, 2.0, 0.0)])
            .hover(|s| {
                s.bg(linear_gradient(180., linear_color_stop(rgb(0x3d3834), 0.), linear_color_stop(rgb(0x312d2a), 1.)))
                    .border_color(rgba(0xffffff2c))
            })
            .active(|s| s.bg(rgb(0x262321))),
        ButtonStyle::Ghost => base
            .border_color(rgba(0x00000000))
            .text_color(theme::text_muted())
            .hover(|s| s.bg(theme::surface_hover()).text_color(theme::text()))
            .active(|s| s.bg(theme::raised())),
    }
}

fn filled(base: Stateful<Div>, hi: Rgba, lo: Rgba, fg: Rgba, tone: Rgba) -> Stateful<Div> {
    let hi = Hsla::from(hi);
    let lo = Hsla::from(lo);
    let lift = |c: Hsla, by: f32| Hsla { l: (c.l + by).min(1.0), ..c };
    base.bg(linear_gradient(180., linear_color_stop(hi, 0.), linear_color_stop(lo, 1.)))
        .border_color(lift(hi, 0.08).opacity(0.7))
        .text_color(fg)
        .shadow(glow(tone, 1.0))
        .hover(move |s| {
            s.bg(linear_gradient(180., linear_color_stop(lift(hi, 0.05), 0.), linear_color_stop(lift(lo, 0.05), 1.)))
                .shadow(glow(tone, 1.8))
        })
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
    let group: SharedString = format!("icon-button-{id}").into();
    div()
        .id(id)
        .group(group.clone())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(28.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgba(0x00000000))
        .cursor_pointer()
        .hover(|s| s.bg(theme::raised()).border_color(theme::border()))
        .active(|s| s.bg(theme::accent_soft()))
        .on_click(on_click)
        .child(
            svg()
                .path(glyph.path())
                .size(px(15.))
                .text_color(theme::text_muted())
                .group_hover(group, |s| s.text_color(theme::text())),
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

pub fn section_label(text: impl Into<SharedString>) -> Div {
    div()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme::text_faint())
        .child(text.into().to_uppercase())
}
