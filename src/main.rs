mod app;
mod assets;
mod format;
mod scan;
mod sunburst;
mod theme;
mod ui;
mod volumes;

use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, SystemMenuType, TitlebarOptions,
    WindowBounds, WindowOptions, point, prelude::*, px, size,
};

use app::{Burrdisk, GoUp, Quit};

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("escape", GoUp, None),
                KeyBinding::new("cmd-[", GoUp, None),
                KeyBinding::new("cmd-up", GoUp, None),
                KeyBinding::new("backspace", GoUp, None),
                KeyBinding::new("cmd-q", Quit, None),
            ]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.set_menus(vec![Menu {
                name: "Burrdisk".into(),
                items: vec![
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit Burrdisk", Quit),
                ],
            }]);

            let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(860.), px(600.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Burrdisk".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(18.), px(20.))),
                    }),
                    focus: true,
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Burrdisk::new(window, cx)),
            )
            .expect("failed to open window");
            cx.on_window_closed(|cx| cx.quit()).detach();
            cx.activate(true);
        });
}
