use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

pub struct Assets;

/// Mona Sans (SIL Open Font License, see `assets/fonts/OFL.txt`), the app's one typeface
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/MonaSans-Regular.ttf"),
    include_bytes!("../assets/fonts/MonaSans-Medium.ttf"),
    include_bytes!("../assets/fonts/MonaSans-SemiBold.ttf"),
    include_bytes!("../assets/fonts/MonaSans-Bold.ttf"),
];

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")))),*]
    };
}

const ICONS: &[(&str, &[u8])] = icons![
    "app-window",
    "archive",
    "arrow-up-right",
    "check",
    "chevron-down",
    "chevron-left",
    "chevron-right",
    "corner-up-left",
    "disc",
    "download",
    "file",
    "file-text",
    "folder",
    "folder-open",
    "folder-search",
    "hard-drive",
    "image",
    "inbox",
    "loader-circle",
    "music",
    "network",
    "plus",
    "refresh",
    "scan",
    "shield-alert",
    "shield-check",
    "trash",
    "usb",
    "video",
    "window-close",
    "window-maximize",
    "window-minimize",
    "window-restore",
    "x",
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}
