use gpui::{Hsla, Rgba, rgb, rgba};

// A quiet, warm-neutral dark palette. Color is reserved for data
pub fn bg() -> Rgba {
    rgb(0x141416)
}
pub fn surface() -> Rgba {
    rgb(0x1c1c1f)
}
pub fn surface_hover() -> Rgba {
    rgb(0x232327)
}
pub fn raised() -> Rgba {
    rgb(0x2a2a2f)
}
pub fn border() -> Rgba {
    rgba(0xffffff12)
}
pub fn border_strong() -> Rgba {
    rgba(0xffffff24)
}
pub fn text() -> Rgba {
    rgb(0xf2f2f3)
}
pub fn text_muted() -> Rgba {
    rgb(0xa1a1a8)
}
pub fn text_faint() -> Rgba {
    rgb(0x6b6b73)
}
pub fn accent() -> Rgba {
    rgb(0xf97316)
}
pub fn accent_hi() -> Rgba {
    rgb(0xfdba74)
}
pub fn accent_lo() -> Rgba {
    rgb(0xea580c)
}
pub fn accent_soft() -> Rgba {
    rgba(0xf9731624)
}
pub fn danger() -> Rgba {
    rgb(0xef4444)
}
pub fn track() -> Rgba {
    rgba(0xffffff14)
}

/// Fill level color: calm when there's room, warmer as the disk fills
pub fn fullness(ratio: f32) -> Hsla {
    let hue = if ratio < 0.7 {
        0.40
    } else if ratio < 0.9 {
        0.40 - (ratio - 0.7) / 0.2 * 0.30
    } else {
        0.03
    };
    gpui::hsla(hue, 0.70, 0.56, 1.0)
}
