use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

pub struct Assets;

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")))),*]
    };
}

const ICONS: &[(&str, &[u8])] = icons![
    "arrow-up-right",
    "chevron-left",
    "chevron-right",
    "disc",
    "file",
    "folder",
    "folder-open",
    "hard-drive",
    "inbox",
    "network",
    "plus",
    "refresh",
    "usb",
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
