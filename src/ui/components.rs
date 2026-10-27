use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Div, ElementId, Hsla, IntoElement, Rgba, SharedString,
    Stateful, Svg, Window, WindowControlArea, div, linear_color_stop, linear_gradient, point, prelude::*, rgb,
    rgba, svg,
};
use crate::ui::px;

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
    WindowClose,
    WindowMaximize,
    WindowMinimize,
    WindowRestore,
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
            Icon::WindowClose => "icons/window-close.svg",
            Icon::WindowMaximize => "icons/window-maximize.svg",
            Icon::WindowMinimize => "icons/window-minimize.svg",
            Icon::WindowRestore => "icons/window-restore.svg",
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

/// Minimize, maximize and close, drawn at the right end of a title bar on Windows,
/// where Burrdisk draws its own title bar. The system handles the clicks (and shows
/// Snap Layouts over maximize); these only paint. Empty on macOS, which keeps its
/// traffic lights.
pub fn window_controls(window: &Window) -> AnyElement {
    if !cfg!(windows) {
        return div().into_any_element();
    }
    let active = window.is_window_active();
    let maximize = if window.is_maximized() { Icon::WindowRestore } else { Icon::WindowMaximize };
    let button = |id: &'static str, glyph: Icon, area: WindowControlArea| {
        let t = fx::hover(id);
        let close = area == WindowControlArea::Close;
        let (fill, ink) = if close {
            (Hsla::from(rgb(0xc42b1c)), Hsla::from(rgb(0xffffff)))
        } else {
            (Hsla::from(rgba(0xffffff0f)), theme::text().into())
        };
        let rest = if active { theme::text_muted() } else { theme::text_faint() };
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(46.))
            .h_full()
            .occlude()
            .window_control_area(area)
            .bg(fx::mix(gpui::transparent_black(), fill, t))
            .active(move |s| s.bg(fill.opacity(if close { 0.8 } else { 0.6 })))
            .on_hover(fx::on_hover(id))
            .child(icon(glyph, 10.0, fx::mix(rest, ink, t)))
    };
    div()
        .flex()
        .flex_none()
        .h_full()
        .child(button("window-min", Icon::WindowMinimize, WindowControlArea::Min))
        .child(button("window-max", maximize, WindowControlArea::Max))
        .child(button("window-close", Icon::WindowClose, WindowControlArea::Close))
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

/// A button with a chevron segment beside it that opens a menu of related actions.
pub fn split_button(
    id: &str,
    glyph: Icon,
    label: impl Into<SharedString>,
    style: ButtonStyle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_menu: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let main = styled_button(SharedString::from(format!("{id}-main")), Some(glyph), label.into(), style, on_click)
        .rounded_r(px(0.));
    let chevron = styled_button(SharedString::from(format!("{id}-menu")), None, SharedString::default(), style, on_menu)
        .w(px(28.))
        .px_0()
        .gap_0()
        .rounded_l(px(0.))
        .border_l_0()
        .child(icon(Icon::ChevronDown, 13.0, if style == ButtonStyle::Primary { rgb(0x1f0b00) } else { theme::text_muted() }));
    // A hairline in the button's own ink separates the two halves.
    let seam: Hsla = if style == ButtonStyle::Primary { rgba(0x1f0b0040).into() } else { rgba(0xffffff1f).into() };
    div()
        .flex()
        .flex_none()
        .child(main)
        .child(div().w(px(1.)).bg(seam))
        .child(chevron)
}

pub struct MenuItem {
    pub label: SharedString,
    /// Key hint shown at the right edge, like "Space" or "⌘⌫".
    pub shortcut: Option<SharedString>,
    pub enabled: bool,
    pub action: Option<Box<dyn Fn(&mut Window, &mut App)>>,
}

impl MenuItem {
    pub fn new(label: impl Into<SharedString>, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self { label: label.into(), shortcut: None, enabled: true, action: Some(Box::new(action)) }
    }

    pub fn shortcut(mut self, keys: impl Into<SharedString>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.enabled = !disabled;
        self
    }

    pub fn separator() -> Self {
        Self { label: SharedString::default(), shortcut: None, enabled: false, action: None }
    }
}

/// A native-feeling popup menu, positioned below the element it is placed in.
/// `on_dismiss` runs for a click anywhere outside it.
pub fn popup_menu(
    id: &str,
    items: Vec<MenuItem>,
    window: &Window,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    menu(id, items, None, window, on_dismiss)
}

/// A right-click menu opening at `position`, in window coordinates.
pub fn context_menu(
    id: &str,
    items: Vec<MenuItem>,
    position: gpui::Point<gpui::Pixels>,
    window: &Window,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    menu(id, items, Some(position), window, on_dismiss)
}

fn menu(
    id: &str,
    items: Vec<MenuItem>,
    position: Option<gpui::Point<gpui::Pixels>>,
    window: &Window,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_dismiss = std::rc::Rc::new(on_dismiss);
    let dismiss = on_dismiss.clone();
    let viewport = window.viewport_size();
    let rows = items.into_iter().enumerate().map(|(ix, item)| {
        if item.action.is_none() {
            return div().mx_2().my_1().h(px(1.)).bg(theme::border()).into_any_element();
        }
        let key = SharedString::from(format!("{id}-item-{ix}"));
        let action = item.action.unwrap();
        let dismiss = on_dismiss.clone();
        div()
            .id(key.clone())
            .flex()
            .items_center()
            .h(px(26.))
            .px_2p5()
            .mx_1()
            .rounded(px(6.))
            .text_sm()
            .whitespace_nowrap()
            .gap_6()
            .group(key.clone())
            .when(!item.enabled, |s| s.text_color(theme::text_faint()))
            .when(item.enabled, |s| {
                s.text_color(theme::text())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::accent()).text_color(rgb(0x1f0b00)))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        dismiss(window, cx);
                        action(window, cx);
                    })
            })
            .child(div().flex_1().child(item.label))
            .when_some(item.shortcut, |s, keys| {
                s.child(
                    div()
                        .text_color(theme::text_faint())
                        .group_hover(key.clone(), |s| s.text_color(rgba(0x1f0b00b3)))
                        .child(keys),
                )
            })
            .into_any_element()
    });
    let anchored = match position {
        Some(at) => gpui::anchored().position(at).anchor(gpui::Corner::TopLeft),
        None => gpui::anchored().anchor(gpui::Corner::TopRight),
    };
    div()
        .when(position.is_none(), |s| s.absolute().top_full().right_0())
        .child(
            // A transparent backdrop catches the click that dismisses the menu, so it
            // never also lands on whatever is underneath.
            gpui::deferred(
                gpui::anchored().position(gpui::point(px(0.), px(0.))).child(
                    div()
                        .id(SharedString::from(format!("{id}-backdrop")))
                        .w(viewport.width)
                        .h(viewport.height)
                        .occlude()
                        .on_mouse_down(gpui::MouseButton::Left, {
                            let dismiss = dismiss.clone();
                            move |_, window, cx| dismiss(window, cx)
                        })
                        .on_mouse_down(gpui::MouseButton::Right, {
                            move |_, window, cx| {
                                cx.stop_propagation();
                                dismiss(window, cx)
                            }
                        }),
                ),
            )
            .with_priority(1),
        )
        .child(
            gpui::deferred(
                anchored
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id(SharedString::from(format!("{id}-menu")))
                            .mt_1p5()
                            .flex()
                            .flex_col()
                            .min_w(px(210.))
                            .py_1()
                            .rounded(px(10.))
                            .bg(rgb(0x262220))
                            .border_1()
                            .border_color(rgba(0xffffff1f))
                            .shadow(vec![
                                shadow(gpui::black().opacity(0.45), 10.0, 28.0, -6.0),
                                shadow(gpui::black().opacity(0.3), 1.0, 3.0, 0.0),
                            ])
                            .occlude()
                            .children(rows)
                            .with_animation(
                                SharedString::from(format!("{id}-menu-in")),
                                Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                                |el, t| el.opacity(t).mt(px(6. - 2. * t)),
                            ),
                    ),
            )
            .with_priority(2),
        )
        .into_any_element()
}
