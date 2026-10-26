use std::f32::consts::TAU;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{Bounds, Hsla, PathBuilder, Pixels, Point, Window, point, px};

use crate::scan::{NodeId, NodeKind, Tree};

/// Rings drawn beyond the center disc.
pub const RINGS: usize = 5;
/// Segments thinner than this (in turns) are folded into "smaller objects".
const MIN_SPAN: f32 = 0.004;
const CENTER_RATIO: f32 = 0.27;

pub const ZOOM_DURATION: Duration = Duration::from_millis(620);
pub const REVEAL_DURATION: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Node(NodeId),
    /// Aggregate of children too small to draw, keyed by their parent.
    Smaller(NodeId),
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub target: Target,
    pub depth: usize,
    /// Angles in turns, clockwise from 12 o'clock.
    pub start: f32,
    pub end: f32,
    pub color: Hsla,
    /// Color this segment takes once a zoom settles. Equal to `color` outside a zoom.
    pub alt: Hsla,
}

pub enum Hit {
    Center,
    Segment(usize),
}

pub struct Geometry {
    pub center: Point<Pixels>,
    pub inner: f32,
    pub ring: f32,
}

impl Geometry {
    pub fn new(bounds: Bounds<Pixels>) -> Self {
        let radius = (f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 2.0 - 12.0)
            .max(40.0);
        let inner = radius * CENTER_RATIO;
        Self {
            center: bounds.center(),
            inner,
            ring: (radius - inner) / RINGS as f32,
        }
    }
}

pub fn layout(tree: &Tree, focus: NodeId) -> Vec<Segment> {
    let mut cx = LayoutCx { tree, zoom: None, origin: hue_origin(tree, focus), out: Vec::new() };
    cx.lay_children(focus, 0, 0.0, 1.0, MIN_SPAN, false);
    cx.out
}

/// An animated move between a folder and one of its descendants. Both views are
/// laid out in the ancestor's coordinates; the camera then slides between them, so
/// the chosen slice opens up into the full circle (or folds back into its parent).
pub struct Zoom {
    pub segments: Rc<Vec<Segment>>,
    /// Span of the descendant in the ancestor's view.
    a: f32,
    b: f32,
    /// How many rings the descendant sits below the ancestor.
    k: f32,
    zoom_in: bool,
    started: Instant,
}

impl Zoom {
    pub fn new(tree: &Tree, from: NodeId, to: NodeId) -> Option<Self> {
        let (base, inner, zoom_in) = if tree.is_ancestor(from, to) {
            (from, to, true)
        } else if tree.is_ancestor(to, from) {
            (to, from, false)
        } else {
            return None;
        };
        let lineage = tree.lineage(inner);
        let base_ix = lineage.iter().position(|id| *id == base)?;
        let path = lineage[base_ix + 1..].to_vec();
        let k = path.len();
        if k == 0 {
            return None;
        }
        let mut cx = LayoutCx {
            tree,
            zoom: Some(ZoomCx { inner, path, extra: k, span: None, origin: hue_origin(tree, inner) }),
            origin: hue_origin(tree, base),
            out: Vec::new(),
        };
        cx.lay_children(base, 0, 0.0, 1.0, MIN_SPAN, false);
        let (a, b) = cx.zoom.as_ref()?.span?;
        Some(Self {
            segments: Rc::new(cx.out),
            a,
            b,
            k: k as f32,
            zoom_in,
            started: Instant::now(),
        })
    }

    pub fn finished(&self) -> bool {
        self.started.elapsed() >= ZOOM_DURATION
    }

    pub fn view(&self) -> View {
        let t = (self.started.elapsed().as_secs_f32() / ZOOM_DURATION.as_secs_f32()).min(1.0);
        let e = ease_in_out_cubic(t);
        let v = if self.zoom_in { e } else { 1.0 - e };
        View {
            lo: self.a * v,
            hi: 1.0 + (self.b - 1.0) * v,
            shift: self.k * v,
            mix: v,
            reveal: 1.0,
        }
    }
}

struct ZoomCx {
    inner: NodeId,
    /// Nodes from just below the base down to `inner`; never folded away.
    path: Vec<NodeId>,
    extra: usize,
    span: Option<(f32, f32)>,
    /// Hue origin of the descendant's own view.
    origin: f32,
}

struct LayoutCx<'a> {
    tree: &'a Tree,
    zoom: Option<ZoomCx>,
    origin: f32,
    out: Vec<Segment>,
}

impl LayoutCx<'_> {
    /// `inside` marks the zoomed folder's subtree, which is drawn deeper and with
    /// a finer fold threshold so it matches that folder's own layout exactly.
    fn lay_children(&mut self, parent: NodeId, depth: usize, start: f32, end: f32, min_span: f32, inside: bool) {
        let tree = self.tree;
        let node = tree.node(parent);
        let extra = self.zoom.as_ref().map_or(0, |z| z.extra);
        if depth >= RINGS + extra || node.size == 0 {
            return;
        }
        let limit = if inside { RINGS + extra } else { RINGS };
        let span = end - start;
        let mut cursor = start;
        let mut rest = 0u64;
        for &child in &node.children {
            let size = tree.node(child).size;
            let child_span = span * size as f32 / node.size as f32;
            let on_path = self.zoom.as_ref().is_some_and(|z| z.path.contains(&child));
            if !on_path && (child_span < min_span || rest > 0) {
                rest += size;
                continue;
            }
            if !on_path && depth >= limit {
                cursor += child_span;
                continue;
            }
            let kind = tree.node(child).kind;
            let mid = cursor + child_span / 2.0;
            let color = color_for(kind, mid - self.origin, depth);
            let alt = self.alt_color(inside, color, |a, b, k, origin| {
                color_for(kind, (mid - a) / (b - a) - origin, depth.saturating_sub(k))
            });
            self.out.push(Segment {
                target: Target::Node(child),
                depth,
                start: cursor,
                end: cursor + child_span,
                color,
                alt,
            });
            let is_inner = self.zoom.as_ref().is_some_and(|z| z.inner == child);
            if is_inner && let Some(zoom) = &mut self.zoom {
                zoom.span = Some((cursor, cursor + child_span));
            }
            if kind == NodeKind::Dir {
                let (next_min, next_inside) = if is_inner {
                    (MIN_SPAN * child_span, true)
                } else {
                    (min_span, inside)
                };
                self.lay_children(child, depth + 1, cursor, cursor + child_span, next_min, next_inside);
            }
            cursor += child_span;
        }
        let rest_span = span * rest as f32 / node.size as f32;
        if rest_span >= min_span / 2.0 && depth < limit {
            let color = smaller_color(depth);
            let alt = self.alt_color(inside, color, |_, _, k, _| smaller_color(depth.saturating_sub(k)));
            self.out.push(Segment {
                target: Target::Smaller(parent),
                depth,
                start: cursor,
                end: cursor + rest_span,
                color,
                alt,
            });
        }
    }

    fn alt_color(&self, inside: bool, color: Hsla, f: impl FnOnce(f32, f32, usize, f32) -> Hsla) -> Hsla {
        match &self.zoom {
            Some(ZoomCx { span: Some((a, b)), extra, origin, .. }) if inside => f(*a, *b, *extra, *origin),
            _ => color,
        }
    }
}

fn smaller_color(depth: usize) -> Hsla {
    oklch(0.40 - depth as f32 * 0.025, 0.012, 50.0)
}

/// Puts the middle of a folder's largest child at the start of the hue wheel, so
/// the biggest thing on screen always wears the brand orange.
fn hue_origin(tree: &Tree, focus: NodeId) -> f32 {
    let node = tree.node(focus);
    match node.children.first() {
        Some(&first) if node.size > 0 => tree.node(first).size as f32 / node.size as f32 / 2.0,
        _ => 0.0,
    }
}

/// Hue follows the position around the circle so a folder and its contents share a
/// family of colors; deeper rings and plain files are quieter. Colors are picked in
/// OKLCH so every hue reads equally bright, with no muddy olives or neon greens.
pub fn color_for(kind: NodeKind, position: f32, depth: usize) -> Hsla {
    let hue = (52.0 + position * 300.0).rem_euclid(360.0);
    let d = depth as f32;
    match kind {
        NodeKind::Dir => oklch(0.74 - d * 0.045, 0.15 - d * 0.014, hue),
        NodeKind::File => oklch(0.58 - d * 0.03, 0.055 - d * 0.006, hue),
        NodeKind::Hidden => oklch(0.48, 0.04, 300.0),
    }
}

/// OKLCH (lightness 0..1, chroma, hue in degrees) to an sRGB color.
pub fn oklch(l: f32, c: f32, h: f32) -> Hsla {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let r = 4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_;
    let g = -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_;
    let bl = -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_;
    let encode = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
    };
    gpui::Rgba { r: encode(r), g: encode(g), b: encode(bl), a: 1.0 }.into()
}

pub fn hit_test(segments: &[Segment], geometry: &Geometry, position: Point<Pixels>) -> Option<Hit> {
    let dx = f32::from(position.x - geometry.center.x);
    let dy = f32::from(position.y - geometry.center.y);
    let r = (dx * dx + dy * dy).sqrt();
    if r < geometry.inner {
        return Some(Hit::Center);
    }
    let depth = ((r - geometry.inner) / geometry.ring) as usize;
    if depth >= RINGS {
        return None;
    }
    let turn = (dx.atan2(-dy) / TAU).rem_euclid(1.0);
    segments
        .iter()
        .position(|s| s.depth == depth && turn >= s.start && turn < s.end)
        .map(Hit::Segment)
}

/// Where the camera sits: which slice of the angle range fills the circle, how
/// many rings it has moved inward, and how far the entrance sweep has got.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub lo: f32,
    pub hi: f32,
    pub shift: f32,
    /// 0 uses `Segment::color`, 1 uses `Segment::alt`.
    pub mix: f32,
    /// 0..1 entrance sweep after a scan finishes.
    pub reveal: f32,
}

impl View {
    pub fn settled() -> Self {
        Self { lo: 0.0, hi: 1.0, shift: 0.0, mix: 0.0, reveal: 1.0 }
    }

    pub fn revealing(started: Instant) -> Self {
        let reveal = (started.elapsed().as_secs_f32() / REVEAL_DURATION.as_secs_f32()).min(1.0);
        Self { reveal, ..Self::settled() }
    }
}

pub struct PaintState {
    pub view: View,
    /// Per segment, 0..1: how strongly it belongs to the hovered item.
    pub highlight: Vec<f32>,
    /// 0..1: how much everything outside the hovered item recedes.
    pub dim: f32,
    /// Segments already collected for deletion are drawn faded out.
    pub collected: Vec<bool>,
    /// 0..1 hover glow on the center disc.
    pub center_hover: f32,
    pub center_tint: Hsla,
}

pub fn paint(segments: &[Segment], bounds: Bounds<Pixels>, state: &PaintState, window: &mut Window) {
    let geometry = Geometry::new(bounds);
    let view = state.view;
    let width = (view.hi - view.lo).max(1e-6);

    // Center disc, tinted by the folder in focus.
    let mut disc = lerp_hsla(gpui::hsla(0.07, 0.06, 0.13, 1.0), state.center_tint, 0.10 + 0.10 * state.center_hover);
    disc.a = 1.0;
    let disc_r = (geometry.inner - 4.0) * (0.9 + 0.1 * ease_out_cubic(view.reveal));
    ribbon(window, geometry.center, 0.0, disc_r, 0.0, 1.0, |_| disc);
    let mut rim = state.center_tint;
    rim.a = 0.18 + 0.32 * state.center_hover;
    ribbon(window, geometry.center, disc_r - 1.5, disc_r, 0.0, 1.0, |_| rim);

    // Highlighted segments go last so their glow sits on top of neighbours.
    for pass in 0..2 {
        for (ix, segment) in segments.iter().enumerate() {
            let highlight = state.highlight.get(ix).copied().unwrap_or(0.0);
            if (pass == 1) != (highlight > 0.0) {
                continue;
            }
            let rel = segment.depth as f32 - view.shift;
            let fade = (1.0 + rel).clamp(0.0, 1.0) * (RINGS as f32 - rel).clamp(0.0, 1.0);
            // Rings sweep in clockwise, each a beat after the one inside it.
            let local = ease_out_cubic((view.reveal * 1.45 - segment.depth as f32 * 0.09).clamp(0.0, 1.0));
            if fade <= 0.0 || local <= 0.0 {
                continue;
            }
            let start = ((segment.start - view.lo) / width).clamp(0.0, 1.0).min(local);
            let end = ((segment.end - view.lo) / width).clamp(0.0, 1.0).min(local);
            if end - start < 1e-5 {
                continue;
            }
            let r0 = (geometry.inner + rel * geometry.ring).max(0.0);
            let r1 = (r0 + geometry.ring) + 6.0 * highlight;

            let mut color = lerp_hsla(segment.color, segment.alt, view.mix);
            if highlight > 0.0 {
                color.l = (color.l + 0.10 * highlight).min(0.90);
                color.s = (color.s + 0.08 * highlight).min(1.0);
            }
            let mut alpha = fade * (0.35 + 0.65 * local);
            alpha *= 1.0 - 0.5 * state.dim * (1.0 - highlight);
            if state.collected.get(ix).copied().unwrap_or(false) {
                alpha *= 0.2;
            }
            color.a = alpha;

            if highlight > 0.0 {
                let mut glow = color;
                glow.a = 0.22 * highlight * alpha;
                annular_sector(window, geometry.center, r0 - 2.0, r1 + 5.0, start, end, glow);
            }
            annular_sector(window, geometry.center, r0 + 1.0, r1 - 1.0, start, end, color);
            // A lighter lip along the outer edge gives each ring a little depth.
            let mut lip = color;
            lip.l = (lip.l + 0.10).min(0.95);
            lip.a *= 0.55;
            annular_sector(window, geometry.center, r1 - 3.0, r1 - 1.0, start, end, lip);
        }
    }
}

fn annular_sector(
    window: &mut Window,
    center: Point<Pixels>,
    r0: f32,
    r1: f32,
    start: f32,
    end: f32,
    color: Hsla,
) {
    // A constant ~1.5px gap between neighbours, independent of radius.
    let gap_outer = (0.75 / r1) / TAU;
    let gap_inner = (0.75 / r0.max(1.0)) / TAU;
    let full = end - start >= 0.999;
    let (os, oe) = if full { (start, end) } else { (start + gap_outer, end - gap_outer) };
    let (is, ie) = if full { (start, end) } else { (start + gap_inner, end - gap_inner) };
    if oe <= os || ie <= is || r1 <= r0 {
        return;
    }

    let at = |r: f32, turn: f32| {
        let a = turn * TAU;
        point(center.x + px(r * a.sin()), center.y - px(r * a.cos()))
    };
    let steps = |span: f32, r: f32| ((span * TAU * r / 3.0).ceil() as usize).clamp(2, 720);

    let mut builder = PathBuilder::fill();
    let n = steps(oe - os, r1);
    builder.move_to(at(r1, os));
    for i in 1..=n {
        builder.line_to(at(r1, os + (oe - os) * i as f32 / n as f32));
    }
    let n = steps(ie - is, r0);
    for i in 0..=n {
        builder.line_to(at(r0, ie - (ie - is) * i as f32 / n as f32));
    }
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// An arc from `start` to `end` (in turns) whose color can vary along its length.
/// `color_at` receives 0..1 along the arc.
fn ribbon(
    window: &mut Window,
    center: Point<Pixels>,
    r0: f32,
    r1: f32,
    start: f32,
    end: f32,
    color_at: impl Fn(f32) -> Hsla,
) {
    if end <= start || r1 <= r0 {
        return;
    }
    let at = |r: f32, turn: f32| {
        let a = turn * TAU;
        point(center.x + px(r * a.sin()), center.y - px(r * a.cos()))
    };
    let first = color_at(0.0);
    let uniform = first == color_at(1.0) && first == color_at(0.5);
    let pieces = if uniform { 1 } else { (((end - start) * 96.0).ceil() as usize).max(2) };
    for p in 0..pieces {
        let s = start + (end - start) * p as f32 / pieces as f32;
        // Overlap a hair so seams between pieces don't show.
        let e = (start + (end - start) * (p + 1) as f32 / pieces as f32 + if p + 1 < pieces { 0.0015 } else { 0.0 }).min(end);
        let color = color_at((p as f32 + 0.5) / pieces as f32);
        let n = (((e - s) * 360.0).ceil() as usize).max(2);
        let mut builder = PathBuilder::fill();
        builder.move_to(at(r1, s));
        for i in 1..=n {
            builder.line_to(at(r1, s + (e - s) * i as f32 / n as f32));
        }
        for i in 0..=n {
            builder.line_to(at(r0, e - (e - s) * i as f32 / n as f32));
        }
        builder.close();
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

fn dot(window: &mut Window, center: Point<Pixels>, r: f32, turn: f32, radius: f32, color: Hsla) {
    let a = turn * TAU;
    let c = point(center.x + px(r * a.sin()), center.y - px(r * a.cos()));
    ribbon(window, c, 0.0, radius, 0.0, 1.0, |_| color);
}

/// A thick ring showing how full a volume is, with rounded ends and a soft glow.
pub fn paint_gauge(window: &mut Window, bounds: Bounds<Pixels>, ratio: f32, thickness: f32, fill: Hsla, track: Hsla) {
    let center = bounds.center();
    let r1 = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0 - 6.0;
    let r0 = r1 - thickness;
    let mid = (r0 + r1) / 2.0;
    ribbon(window, center, r0, r1, 0.0, 1.0, |_| track);
    let ratio = ratio.min(1.0);
    if ratio <= 0.0 {
        return;
    }
    let mut glow = fill;
    glow.a = 0.16;
    ribbon(window, center, r0 - 4.0, r1 + 4.0, 0.0, ratio, |_| glow);
    let shade = |u: f32| {
        let mut c = fill;
        c.l = (fill.l - 0.10 + 0.14 * u).clamp(0.0, 1.0);
        c
    };
    ribbon(window, center, r0, r1, 0.0, ratio, shade);
    dot(window, center, mid, 0.0, thickness / 2.0, shade(0.0));
    dot(window, center, mid, ratio, thickness / 2.0, shade(1.0));
}

/// A ring track with a highlighted arc from `start` to `end` (in turns). With
/// `tail`, the arc fades in from transparent like a comet.
pub fn paint_arc(window: &mut Window, bounds: Bounds<Pixels>, start: f32, end: f32, thickness: f32, fill: Hsla, track: Hsla, tail: bool) {
    let center = bounds.center();
    let r1 = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0 - 8.0;
    let r0 = r1 - thickness;
    let mid = (r0 + r1) / 2.0;
    ribbon(window, center, r0, r1, 0.0, 1.0, |_| track);
    let color_at = |u: f32| {
        let mut c = fill;
        if tail {
            c.a = u * u;
        } else {
            c.l = (fill.l - 0.12 + 0.16 * u).clamp(0.0, 1.0);
        }
        c
    };
    let mut glow = fill;
    glow.a = 0.14;
    ribbon(window, center, r0 - 5.0, r1 + 5.0, if tail { start + (end - start) * 0.5 } else { start }, end, |_| glow);
    ribbon(window, center, r0, r1, start, end, color_at);
    if !tail {
        dot(window, center, mid, start, thickness / 2.0, color_at(0.0));
    }
    dot(window, center, mid, end, thickness / 2.0, color_at(1.0));
}

pub fn lerp_hsla(a: Hsla, b: Hsla, t: f32) -> Hsla {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    // Take the short way around the hue wheel.
    let mut dh = b.h - a.h;
    if dh > 0.5 {
        dh -= 1.0;
    } else if dh < -0.5 {
        dh += 1.0;
    }
    Hsla {
        h: (a.h + dh * t).rem_euclid(1.0),
        s: a.s + (b.s - a.s) * t,
        l: a.l + (b.l - a.l) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{self, Progress};
    use gpui::{Bounds, point, size};

    #[test]
    fn layout_spans_and_hit_test() {
        let dir = std::env::temp_dir().join(format!("burrdisk-sun-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("big")).unwrap();
        std::fs::write(dir.join("big/a"), vec![1u8; 300_000]).unwrap();
        std::fs::write(dir.join("b"), vec![1u8; 100_000]).unwrap();
        let tree = scan::scan(&dir, "t", &Progress::default()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        let segments = layout(&tree, Tree::ROOT);
        let ring0: Vec<_> = segments.iter().filter(|s| s.depth == 0).collect();
        assert_eq!(ring0.len(), 2);
        assert!(ring0[0].end > 0.6 && ring0[0].start == 0.0);
        assert!(segments.iter().any(|s| s.depth == 1));

        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(400.)));
        let geometry = Geometry::new(bounds);
        assert!(matches!(hit_test(&segments, &geometry, point(px(200.), px(200.))), Some(Hit::Center)));
        // Just right of 12 o'clock on the first ring is the largest child.
        let y = 200.0 - geometry.inner - geometry.ring / 2.0;
        match hit_test(&segments, &geometry, point(px(205.), px(y))) {
            Some(Hit::Segment(ix)) => assert_eq!(segments[ix].target, ring0[0].target),
            _ => panic!("expected a segment"),
        }
        assert!(hit_test(&segments, &geometry, point(px(399.), px(1.))).is_none());
    }

    /// At the end of a zoom, the descendant's subtree must land exactly where its own
    /// layout puts it, so swapping to that layout doesn't jump.
    #[test]
    fn zoom_lands_on_child_layout() {
        let dir = std::env::temp_dir().join(format!("burrdisk-zoom-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (i, size) in [(0, 400_000), (1, 120_000), (2, 30_000), (3, 900)].iter() {
            std::fs::create_dir_all(dir.join("big/sub")).unwrap();
            std::fs::write(dir.join(format!("big/f{i}")), vec![1u8; *size]).unwrap();
            std::fs::write(dir.join(format!("big/sub/g{i}")), vec![1u8; *size / 2]).unwrap();
        }
        std::fs::write(dir.join("other"), vec![1u8; 200_000]).unwrap();
        let tree = scan::scan(&dir, "t", &Progress::default()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        let big = tree.node(Tree::ROOT).children.iter().copied().find(|c| &*tree.node(*c).name == "big").unwrap();
        let zoom = Zoom::new(&tree, Tree::ROOT, big).unwrap();
        let target = layout(&tree, big);
        let (a, b) = (zoom.a, zoom.b);
        for seg in target.iter() {
            let found = zoom
                .segments
                .iter()
                .find(|s| s.target == seg.target && s.depth == seg.depth + 1)
                .unwrap_or_else(|| panic!("{:?} missing from zoom layout", seg.target));
            let start = (found.start - a) / (b - a);
            let end = (found.end - a) / (b - a);
            assert!((start - seg.start).abs() < 1e-4 && (end - seg.end).abs() < 1e-4);
            assert!((found.alt.h - seg.color.h).abs() < 1e-3 && (found.alt.l - seg.color.l).abs() < 1e-3);
        }
        // Zooming back out is the same move in reverse.
        assert!(Zoom::new(&tree, big, Tree::ROOT).is_some());
    }
}
