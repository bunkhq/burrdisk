use std::f32::consts::TAU;

use gpui::{Bounds, Hsla, PathBuilder, Pixels, Point, Window, point, px};

use crate::scan::{NodeId, NodeKind, Tree};

/// Rings drawn beyond the center disc.
pub const RINGS: usize = 5;
/// Segments thinner than this (in turns) are folded into "smaller objects".
const MIN_SPAN: f32 = 0.004;
const CENTER_RATIO: f32 = 0.27;

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
        let radius = (f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 2.0 - 8.0)
            .max(40.0);
        let inner = radius * CENTER_RATIO;
        Self {
            center: bounds.center(),
            inner,
            ring: (radius - inner) / RINGS as f32,
        }
    }

    pub fn ring_radii(&self, depth: usize) -> (f32, f32) {
        let r0 = self.inner + depth as f32 * self.ring;
        (r0, r0 + self.ring)
    }
}

pub fn layout(tree: &Tree, focus: NodeId) -> Vec<Segment> {
    let mut out = Vec::new();
    lay_children(tree, focus, 0, 0.0, 1.0, &mut out);
    out
}

fn lay_children(tree: &Tree, parent: NodeId, depth: usize, start: f32, end: f32, out: &mut Vec<Segment>) {
    let node = tree.node(parent);
    if depth >= RINGS || node.size == 0 {
        return;
    }
    let span = end - start;
    let mut cursor = start;
    let mut rest = 0u64;
    for &child in &node.children {
        let size = tree.node(child).size;
        let child_span = span * size as f32 / node.size as f32;
        if child_span < MIN_SPAN || rest > 0 {
            rest += size;
            continue;
        }
        let kind = tree.node(child).kind;
        out.push(Segment {
            target: Target::Node(child),
            depth,
            start: cursor,
            end: cursor + child_span,
            color: color_for(kind, cursor + child_span / 2.0, depth),
        });
        if kind == NodeKind::Dir {
            lay_children(tree, child, depth + 1, cursor, cursor + child_span, out);
        }
        cursor += child_span;
    }
    let rest_span = span * rest as f32 / node.size as f32;
    if rest_span >= MIN_SPAN / 2.0 {
        out.push(Segment {
            target: Target::Smaller(parent),
            depth,
            start: cursor,
            end: cursor + rest_span,
            color: gpui::hsla(0.0, 0.0, 0.32 - depth as f32 * 0.02, 1.0),
        });
    }
}

/// Hue follows the position around the circle so a folder and its contents share a
/// family of colors; deeper rings and plain files are quieter.
pub fn color_for(kind: NodeKind, mid: f32, depth: usize) -> Hsla {
    let hue = (0.58 + mid * 0.86).fract();
    let d = depth as f32;
    match kind {
        NodeKind::Dir => gpui::hsla(hue, 0.72 - d * 0.05, 0.64 - d * 0.045, 1.0),
        NodeKind::File => gpui::hsla(hue, 0.30 - d * 0.03, 0.50 - d * 0.035, 1.0),
        NodeKind::Hidden => gpui::hsla(0.80, 0.30, 0.42, 1.0),
    }
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

pub struct PaintState {
    pub hovered: Option<usize>,
    /// Segments already collected for deletion are drawn faded out.
    pub collected: Vec<bool>,
    /// 0..1 entrance progress after drilling in or out.
    pub progress: f32,
}

pub fn paint(segments: &[Segment], bounds: Bounds<Pixels>, state: &PaintState, window: &mut Window) {
    let geometry = Geometry::new(bounds);
    let t = state.progress;
    for (ix, segment) in segments.iter().enumerate() {
        // Rings unfold outward one after another.
        let local = ((t * 1.6) - segment.depth as f32 * 0.14).clamp(0.0, 1.0);
        let local = 1.0 - (1.0 - local).powi(3);
        if local <= 0.0 {
            continue;
        }
        let (r0, r1) = geometry.ring_radii(segment.depth);
        let grow = 0.82 + 0.18 * local;
        let r0 = r0 * grow;
        let r1 = r1 * grow;

        let mut color = segment.color;
        if state.hovered == Some(ix) {
            color.l = (color.l + 0.12).min(0.92);
            color.s = (color.s + 0.05).min(1.0);
        }
        if state.collected.get(ix).copied().unwrap_or(false) {
            color.a = 0.22;
        }
        color.a *= local;
        annular_sector(window, geometry.center, r0 + 1.0, r1 - 1.0, segment.start, segment.end, color);
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
    if oe <= os || ie <= is {
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

/// A thick ring showing how full a volume is.
pub fn paint_gauge(window: &mut Window, bounds: Bounds<Pixels>, ratio: f32, thickness: f32, fill: Hsla, track: Hsla) {
    let center = bounds.center();
    let r1 = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0;
    let r0 = r1 - thickness;
    annular_sector_plain(window, center, r0, r1, 0.0, 1.0, track);
    if ratio > 0.0 {
        annular_sector_plain(window, center, r0, r1, 0.0, ratio.min(1.0), fill);
    }
}

fn annular_sector_plain(
    window: &mut Window,
    center: Point<Pixels>,
    r0: f32,
    r1: f32,
    start: f32,
    end: f32,
    color: Hsla,
) {
    let at = |r: f32, turn: f32| {
        let a = turn * TAU;
        point(center.x + px(r * a.sin()), center.y - px(r * a.cos()))
    };
    let n = (((end - start) * 180.0).ceil() as usize).max(2);
    let mut builder = PathBuilder::fill();
    builder.move_to(at(r1, start));
    for i in 1..=n {
        builder.line_to(at(r1, start + (end - start) * i as f32 / n as f32));
    }
    for i in 0..=n {
        builder.line_to(at(r0, end - (end - start) * i as f32 / n as f32));
    }
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// A ring track with a highlighted arc from `start` to `end` (in turns).
pub fn paint_arc(window: &mut Window, bounds: Bounds<Pixels>, start: f32, end: f32, thickness: f32, fill: Hsla, track: Hsla) {
    let center = bounds.center();
    let r1 = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0;
    let r0 = r1 - thickness;
    annular_sector_plain(window, center, r0, r1, 0.0, 1.0, track);
    annular_sector_plain(window, center, r0, r1, start, end, fill);
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
}
