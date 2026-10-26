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
