pub mod components;
pub mod explorer;
pub mod home;
pub mod scanning;
pub mod update;
pub mod fx;
pub mod access;

/// How much larger than its macOS point sizes the UI is drawn. Windows at 100%
/// scaling shows a point as one small pixel, so everything runs a size up there
pub const UI_SCALE: f32 = if cfg!(windows) { 1.2 } else { 1.0 };

/// A design size in pixels, scaled by [`UI_SCALE`]. Hairlines stay one pixel
pub fn px(value: f32) -> gpui::Pixels {
    gpui::px(if value.abs() <= 1.5 { value } else { value * UI_SCALE })
}
