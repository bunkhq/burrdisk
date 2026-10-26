use gpui::{
    AnyElement, ClickEvent, Div, ElementId, Hsla, IntoElement, Rgba, SharedString, Stateful,
    Svg, Window, App, div, prelude::*, px, svg,
};

use crate::theme;

#[derive(Clone, Copy)]
pub enum Icon {
    ArrowUpRight,
    ChevronLeft,
    ChevronRight,
    Disc,
    File,
    Folder,
    FolderOpen,
    HardDrive,
    Inbox,
    Network,
    Plus,
    Refresh,
    Usb,
    X,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Icon::ArrowUpRight => "icons/arrow-up-right.svg",
            Icon::ChevronLeft => "icons/chevron-left.svg",
            Icon::ChevronRight => "icons/chevron-right.svg",
            Icon::Disc => "icons/disc.svg",
            Icon::File => "icons/file.svg",
            Icon::Folder => "icons/folder.svg",
            Icon::FolderOpen => "icons/folder-open.svg",
            Icon::HardDrive => "icons/hard-drive.svg",
            Icon::Inbox => "icons/inbox.svg",
            Icon::Network => "icons/network.svg",
            Icon::Plus => "icons/plus.svg",
            Icon::Refresh => "icons/refresh.svg",
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

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    style: ButtonStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (bg, hover, fg, border): (Rgba, Rgba, Rgba, Rgba) = match style {
        ButtonStyle::Primary => (theme::accent(), gpui::rgb(0xfb8a3c), gpui::rgb(0x1a0d03), theme::accent()),
        ButtonStyle::Secondary => (theme::raised(), gpui::rgb(0x34343a), theme::text(), theme::border_strong()),
        ButtonStyle::Ghost => (gpui::rgba(0x00000000), theme::surface_hover(), theme::text_muted(), gpui::rgba(0x00000000)),
        ButtonStyle::Danger => (theme::danger(), theme::danger_hover(), gpui::rgb(0xffffff), theme::danger()),
    };
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap_1p5()
        .h(px(32.))
        .px(px(14.))
        .rounded(px(8.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .text_sm()
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .active(|s| s.opacity(0.85))
        .on_click(on_click)
        .child(label.into())
}

pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: Icon,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(28.))
        .rounded(px(7.))
        .cursor_pointer()
        .text_color(theme::text_muted())
        .hover(|s| s.bg(theme::surface_hover()).text_color(theme::text()))
        .active(|s| s.opacity(0.8))
        .on_click(on_click)
        .child(
            svg()
                .path(glyph.path())
                .size(px(15.))
                .text_color(theme::text_muted()),
        )
}

/// A thin horizontal capacity bar.
pub fn bar(ratio: f32, color: impl Into<Hsla>, height: f32) -> AnyElement {
    div()
        .w_full()
        .h(px(height))
        .rounded_full()
        .bg(theme::track())
        .child(
            div()
                .h_full()
                .rounded_full()
                .w(gpui::relative(ratio.clamp(0.0, 1.0)))
                .bg(color.into()),
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
