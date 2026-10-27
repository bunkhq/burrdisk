//! Smooth hover states. GPUI's `.hover()` styles switch instantly, so elements that
//! should ease in and out record their hover here and read back a 0..1 value
//! while rendering. Everything runs on the UI thread, so the store is thread-local.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{App, ElementId, Hsla, Window};

const IN: Duration = Duration::from_millis(130);
const OUT: Duration = Duration::from_millis(260);

struct Tween {
    target: bool,
    from: f32,
    since: Instant,
}

impl Tween {
    fn value(&self) -> f32 {
        let duration = if self.target { IN } else { OUT };
        let t = (self.since.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let to = if self.target { 1.0 } else { 0.0 };
        self.from + (to - self.from) * eased
    }

    fn settled(&self) -> bool {
        self.since.elapsed() >= if self.target { IN } else { OUT }
    }
}

thread_local! {
    static TWEENS: RefCell<HashMap<ElementId, Tween>> = RefCell::new(HashMap::new());
}

/// Current 0..1 hover amount for `key`.
pub fn hover(key: impl Into<ElementId>) -> f32 {
    let key = key.into();
    TWEENS.with(|t| t.borrow().get(&key).map_or(0.0, Tween::value))
}

/// Points `key` at `target`, starting from wherever it is now. Returns the current value.
pub fn track(key: impl Into<ElementId>, target: bool) -> f32 {
    let key = key.into();
    TWEENS.with(|t| {
        let mut map = t.borrow_mut();
        let current = map.get(&key).map_or(0.0, Tween::value);
        match map.get_mut(&key) {
            Some(tween) if tween.target == target => {}
            Some(tween) => {
                *tween = Tween { target, from: current, since: Instant::now() };
            }
            None if target => {
                map.insert(key, Tween { target, from: 0.0, since: Instant::now() });
            }
            None => {}
        }
        current
    })
}

/// An `on_hover` handler that drives the tween for `key` and repaints.
pub fn on_hover(key: impl Into<ElementId>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let key = key.into();
    move |hovered, window, _| {
        track(key.clone(), *hovered);
        window.refresh();
    }
}

/// Whether any tween is still moving; the root view keeps requesting frames until not.
/// Settled, un-hovered entries are dropped here.
pub fn animating() -> bool {
    TWEENS.with(|t| {
        let mut map = t.borrow_mut();
        map.retain(|_, tween| tween.target || !tween.settled());
        map.values().any(|tween| !tween.settled())
    })
}

pub fn mix(a: impl Into<Hsla>, b: impl Into<Hsla>, t: f32) -> Hsla {
    let (a, b) = (a.into(), b.into());
    // Fully transparent ends take the other side's hue so fades don't shift color.
    let a = if a.a == 0.0 { Hsla { a: 0.0, ..b } } else { a };
    let b = if b.a == 0.0 { Hsla { a: 0.0, ..a } } else { b };
    crate::sunburst::lerp_hsla(a, b, t)
}

/// A soft light under the cursor, clipped to a control's rounded shape, like a key
/// backlit from wherever the pointer rests. Add it as the control's first child, pass
/// the control's hover amount and corner radius, and repaint on mouse moves with
/// [`on_move`]. Off the control, the light waits at the nearest edge while it fades.
pub fn shine(t: f32, color: impl Into<Hsla>, radius: gpui::Pixels) -> impl gpui::IntoElement {
    use gpui::{BoxShadow, Bounds, ContentMask, Corners, Styled, canvas, point, px, size};
    let color = color.into();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if t <= 0.001 {
                return;
            }
            let mouse = window.mouse_position();
            let at = point(
                mouse.x.clamp(bounds.left(), bounds.right()),
                mouse.y.clamp(bounds.top(), bounds.bottom()),
            );
            // A round light wider than a button is tall. GPUI's blur radius is the
            // Gaussian's sigma, so this peaks near two thirds of `color`'s alpha under
            // the cursor, half that 25px away, and a fifth at 40px.
            let spot = crate::ui::px(52.);
            let glow = [BoxShadow {
                color: color.opacity(color.a * t),
                offset: point(px(0.), px(0.)),
                blur_radius: crate::ui::px(18.),
                spread_radius: px(0.),
            }];
            // Content masks are square, so the rounded corners are traced in one-pixel
            // rows; the rest of the control is a single band.
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let r = f32::from(radius).min(w / 2.).min(h / 2.).max(0.);
            let rows = r.ceil() as usize;
            let mut bands = Vec::with_capacity(rows * 2 + 1);
            for row in 0..rows {
                let dy = r - (row as f32 + 0.5);
                let inset = r - (r * r - dy * dy).max(0.).sqrt();
                let top = row as f32;
                bands.push((inset, top, 1.0_f32.min(r - top)));
                bands.push((inset, h - top - 1.0_f32.min(r - top), 1.0_f32.min(r - top)));
            }
            bands.push((0., r, h - 2. * r));
            for (inset, top, height) in bands {
                if height <= 0. {
                    continue;
                }
                let clip = Bounds {
                    origin: bounds.origin + point(px(inset), px(top)),
                    size: size(px(w - 2. * inset), px(height)),
                };
                window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                    window.paint_shadows(
                        Bounds::centered_at(at, size(spot, spot)),
                        Corners::all(spot / 2.),
                        &glow,
                    );
                });
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A mouse-move handler that repaints, so a [`shine`] keeps up with the cursor.
pub fn on_move(_: &gpui::MouseMoveEvent, window: &mut Window, _: &mut App) {
    window.refresh();
}
