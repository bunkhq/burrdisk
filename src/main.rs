// Release builds on Windows are GUI apps, so no console window opens beside them
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod appicon;
mod assets;
mod filetype;
mod format;
mod platform;
mod scan;
mod sunburst;
mod theme;
mod ui;
mod update;
mod volumes;

use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, SystemMenuType, TitlebarOptions,
    WindowBounds, WindowOptions, point, prelude::*, size,
};
use ui::px;

use app::{Burrdisk, CheckForUpdates, Collect, GoBack, GoForward, GoUp, Preview, Quit};

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--scan-helper") {
        return scan::run_helper(&args[1..]);
    }
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("escape", GoUp, None),
                KeyBinding::new("secondary-[", GoBack, None),
                KeyBinding::new("secondary-]", GoForward, None),
                KeyBinding::new("secondary-backspace", Collect, None),
                KeyBinding::new("secondary-up", GoUp, None),
                KeyBinding::new("backspace", GoUp, None),
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("alt-left", GoBack, None),
                KeyBinding::new("alt-right", GoForward, None),
            ]);
            // Quick Look is macOS only. Opening the hovered item instead would make
            // Space launch programs on Windows
            if cfg!(not(windows)) {
                cx.bind_keys([KeyBinding::new("space", Preview, None)]);
            }
            cx.on_action(|_: &Quit, cx| cx.quit());
            let mut items = Vec::new();
            if update::enabled() {
                items.extend([MenuItem::action("Check for Updates\u{2026}", CheckForUpdates), MenuItem::separator()]);
            }
            cx.set_menus(vec![Menu {
                name: "Burrdisk".into(),
                items: items
                    .into_iter()
                    .chain([
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit Burrdisk", Quit),
                    ])
                    .collect(),
            }]);

            // Mona Sans, bundled so Burrdisk reads the same on every platform
            cx.text_system()
                .add_fonts(assets::FONTS.iter().map(|font| std::borrow::Cow::Borrowed(*font)).collect())
                .expect("bundled fonts load");

            let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(860.), px(600.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Burrdisk".into()),
                        // Burrdisk draws its own title bar; Windows gets drawn caption buttons
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(18.), px(20.))),
                    }),
                    focus: true,
                    ..Default::default()
                },
                |window, cx| {
                    // Text and spacing in rems follow the same scale as `ui::px` sizes
                    window.set_rem_size(px(16.));
                    cx.new(|cx| Burrdisk::new(window, cx))
                },
            )
            .expect("failed to open window");
            cx.on_window_closed(|cx| cx.quit()).detach();
            cx.activate(true);
        });
    std::process::ExitCode::SUCCESS
}
