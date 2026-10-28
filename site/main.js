// Burrdisk's site: the burrs and the sunburst are drawn here, from an example
// disk, with the app's own color rules (DESIGN.md) and motion.
"use strict";

const TAU = Math.PI * 2;
const NS = "http://www.w3.org/2000/svg";
const REDUCED = matchMedia("(prefers-reduced-motion: reduce)").matches;
const REPO = "bunkhq/burrdisk";

const easeCubic = (t) => 1 - Math.pow(1 - t, 3);
const easeQuint = (t) => 1 - Math.pow(1 - t, 5);
const lerp = (a, b, t) => a + (b - a) * t;
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const el = (name, attrs = {}) => {
  const node = document.createElementNS(NS, name);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  return node;
};

// One tween at a time per key; a new one starts from wherever the old one was.
const tweens = new Map();
function tween(key, duration, ease, step, done) {
  cancelAnimationFrame(tweens.get(key));
  if (REDUCED || duration <= 0) {
    step(1);
    done && done();
    return;
  }
  const start = performance.now();
  const frame = (now) => {
    const t = clamp((now - start) / duration, 0, 1);
    step(ease(t));
    if (t < 1) tweens.set(key, requestAnimationFrame(frame));
    else done && done();
  };
  tweens.set(key, requestAnimationFrame(frame));
}

// Fullness: green below 70% full, through yellow-green to red-orange by 90%.
function fullness(f) {
  const t = clamp((f - 0.7) / 0.2, 0, 1);
  const hue = t < 0.5 ? lerp(144, 90, t / 0.5) : lerp(90, 11, (t - 0.5) / 0.5);
  return `hsl(${hue.toFixed(1)} 70% 56%)`;
}

// A spine: a tapered quad from a root buried in the core to a thin tip, and a bead.
function spinePath(a, r0, r1, rootHalf, tipHalf) {
  const dx = Math.sin(a), dy = -Math.cos(a), nx = -dy, ny = dx;
  const p = (r, s) => `${(r * dx + s * nx).toFixed(2)},${(r * dy + s * ny).toFixed(2)}`;
  return `M${p(r0, -rootHalf)}L${p(r1, -tipHalf)}L${p(r1, tipHalf)}L${p(r0, rootHalf)}Z`;
}

/* ---------- The wordmark burr: 14 Ember spines, the one ornamental burr ---------- */
for (const svg of document.querySelectorAll("[data-wordmark]")) {
  for (let i = 0; i < 14; i++) {
    const a = (i / 14) * TAU;
    svg.append(el("path", { d: spinePath(a, 3, 10.4, 1.1, 0.35), fill: "#f97316" }));
    svg.append(el("circle", { cx: (10.4 * Math.sin(a)).toFixed(2), cy: (-10.4 * Math.cos(a)).toFixed(2), r: 1.05, fill: "#fdba74" }));
  }
  svg.append(el("circle", { r: 4.2, fill: "#f97316" }));
}

/* ---------- The example disk ---------- */
// [name, GB, kind, children]: d folder, f file, h hidden space, s smaller items.
const DISK = ["Macintosh HD", 432, "d", [
  ["Users", 182, "d", [
    ["you", 160, "d", [
      ["Library", 62, "d", [
        ["Developer", 31, "d", [
          ["Xcode", 26, "d", [["DerivedData", 18.4, "d"], ["iOS DeviceSupport", 7.6, "d"]]],
          ["CoreSimulator", 5, "d"],
        ]],
        ["Caches", 14, "d"], ["Application Support", 11, "d"], ["Containers", 6, "d"],
      ]],
      ["Movies", 34, "d", [["Final Cut Library", 24, "d"], ["Exports", 6.9, "d"], ["Screen Recording.mov", 3.1, "f"]]],
      ["Downloads", 29, "d", [["Backup 2023.zip", 9.6, "f"], ["Installers", 8.4, "d"], ["Photos export", 6, "d"], ["Smaller items", 5, "s"]]],
      ["Developer", 21, "d", [
        ["web-app", 8, "d", [["node_modules", 2.4, "d"], [".next", 3.1, "d"], ["src", 2.5, "d"]]],
        ["models", 7, "f"], ["burrdisk", 6, "d"],
      ]],
      ["Documents", 14, "d"],
    ]],
    ["Shared", 14, "d"], ["Guest", 8, "d"],
  ]],
  ["Applications", 64, "d", [["Xcode.app", 11.2, "f"]]],
  ["Library", 58, "d"], ["Hidden space", 38, "h"], ["System", 41, "d"], ["private", 27, "d"], ["Smaller items", 22, "s"],
]];
const COLLECT = ["DerivedData", "Backup 2023.zip", "Screen Recording.mov", "node_modules"];
const ICON = {
  d: '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
  zip: '<rect width="20" height="5" x="2" y="3" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/>',
  mov: '<path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5"/><rect x="2" y="6" width="14" height="12" rx="2"/>',
};

// Fill out folders the example doesn't spell out, so every ring reads as a disk.
function grow(node, depth) {
  const [name, size, kind, kids] = node;
  const out = { name, size, kind, depth, children: [] };
  let list = kids ? kids.slice() : [];
  if (!kids && kind === "d" && depth < 6 && size > 1.2) {
    const parts = [0.42, 0.24, 0.14, 0.09];
    list = parts.map((p, i) => [`${name}/${i}`, size * p, i === 3 ? "f" : "d"]);
    list.push([`${name}/s`, size * 0.11, "s"]);
  }
  const named = list.reduce((sum, k) => sum + k[1], 0);
  if (kids && size - named > 0.05) list.push([`${name}/rest`, size - named, "s"]);
  list.sort((a, b) => (a[2] === "s") - (b[2] === "s") || b[1] - a[1]);
  out.children = list.map((k) => grow(k, depth + 1));
  return out;
}
const root = grow(DISK, 0);
const nodes = [];
(function place(n, x0, parent) {
  n.x0 = x0;
  n.x1 = x0 + n.size / root.size;
  n.parent = parent;
  n.mid = (n.x0 + n.x1) / 2;
  nodes.push(n);
  let at = x0;
  for (const c of n.children) { place(c, at, n); at = c.x1; }
})(root, 0, null);
const find = (name) => nodes.find((n) => n.name === name);
const you = find("you");
const collectNodes = COLLECT.map(find);
const collectedGB = collectNodes.reduce((s, n) => s + n.size, 0);

// color_for: hue by position in the focused folder, rotated so its biggest child
// takes the start hue; each ring deeper is a little darker and calmer.
function colorFor(n, focus, collected) {
  const k = Math.max(1, n.depth - focus.depth) - 1;
  if (n.kind === "h") return "oklch(48% 0.04 300)";
  if (n.kind === "s") return `oklch(${40 - k * 2}% 0.012 50)`;
  const span = focus.x1 - focus.x0;
  const first = focus.children[0];
  const firstMid = (first.mid - focus.x0) / span;
  const pos = clamp((n.mid - focus.x0) / span, 0, 1);
  const hue = 52 + ((((pos - firstMid) % 1) + 1) % 1) * 300;
  let [l, c] = n.kind === "f" ? [0.58 - k * 0.03, 0.055 - k * 0.006] : [0.74 - k * 0.045, 0.15 - k * 0.014];
  if (collected) c *= 0.12;
  return `oklch(${(l * 100).toFixed(1)}% ${Math.max(c, 0.01).toFixed(3)} ${hue.toFixed(1)})`;
}

/* ---------- Burrs ---------- */
// Unscanned: spines all round, the used fraction lit in Fullness, the rest dim stubs.
function unscannedSpines(count, used, R, coreRatio = 0.34) {
  const r0 = R * coreRatio, tone = fullness(used), out = [];
  for (let i = 0; i < count; i++) {
    const f = (i + 0.5) / count, lit = f < used;
    const jitter = 0.94 + 0.06 * Math.sin(i * 2.399);
    out.push({ a: f * TAU, reach: lit ? (R - r0) * jitter : (R - r0) * 0.34, color: lit ? tone : "#ffffff24", bead: lit });
  }
  return out;
}
// Scanned: one lobe per top-level folder, its size by share, its reach by √share.
function dataSpines(count, R) {
  const r0 = R * 0.34, out = [], top = root.children, max = top[0].size;
  let i = 0;
  for (const n of top) {
    const share = n.x1 - n.x0, k = Math.max(1, Math.round(share * count));
    for (let j = 0; j < k; j++, i++) {
      const lf = (j + 0.5) / k;
      const a = (n.x0 + share * lf) * TAU;
      const reach = (R - r0) * (0.48 + 0.52 * Math.sqrt(n.size / max)) * (0.8 + 0.2 * Math.sin(lf * Math.PI));
      out.push({ a, reach, color: colorFor(n, root, false), bead: true });
    }
  }
  return out;
}
function drawBurr(group, spines, R, progress, rim, core = true, coreRatio = 0.34) {
  const r0 = R * coreRatio, rootHalf = R * 0.014, beadR = R * 0.0135;
  if (group.childElementCount !== spines.length * 2 + (core ? 1 : 0)) {
    group.replaceChildren();
    for (const s of spines) {
      group.append(el("path", { fill: s.color }));
      group.append(el("circle", { r: beadR, fill: s.color, opacity: s.bead ? 1 : 0 }));
    }
    if (core) group.append(el("circle", { r: r0, fill: "#26201d", stroke: rim ? "#f97316" : "none", "stroke-width": 3 }));
  }
  const kids = group.children;
  spines.forEach((s, i) => {
    // Spines grow out sweeping clockwise: each starts a little after the last.
    const local = clamp((progress - (i / spines.length) * 0.45) / 0.55, 0, 1);
    const r1 = r0 + s.reach * easeCubic(local);
    kids[i * 2].setAttribute("d", spinePath(s.a, r0 - 3, Math.max(r1, r0 - 2), rootHalf, 0.9));
    kids[i * 2].setAttribute("fill", s.color);
    kids[i * 2 + 1].setAttribute("cx", (r1 * Math.sin(s.a)).toFixed(2));
    kids[i * 2 + 1].setAttribute("cy", (-r1 * Math.cos(s.a)).toFixed(2));
    kids[i * 2 + 1].setAttribute("fill", s.color);
    kids[i * 2 + 1].setAttribute("opacity", s.bead && local > 0.05 ? 1 : 0);
  });
}

/* ---------- Hero: the warning, and the burr bristling behind it ---------- */
const heroSvg = document.querySelector("[data-hero-burr]");
if (heroSvg) {
  heroSvg.classList.add("data");
  const used = 432 / 460;
  const R = 380;
  const glow = el("radialGradient", { id: "heroGlow" });
  glow.append(el("stop", { offset: 0.2, "stop-color": fullness(used), "stop-opacity": 0.2 }));
  glow.append(el("stop", { offset: 1, "stop-color": fullness(used), "stop-opacity": 0 }));
  const defs = el("defs");
  defs.append(glow);
  heroSvg.append(defs, el("circle", { r: 400, fill: "url(#heroGlow)" }));
  const g = el("g");
  heroSvg.append(g);
  // The full bar bristles into the burr: its spines sweep out once the bar has filled.
  // A small hub, so the burr's center stays hidden behind the warning.
  const spines = unscannedSpines(84, used, R, 0.18);
  drawBurr(g, spines, R, 0, false, false, 0.18);
  setTimeout(() => tween("hero", 1300, (t) => t, (t) => drawBurr(g, spines, R, t, false, false, 0.18)), REDUCED ? 0 : 1100);
  // Scrolling away, the spines bristle and the burr turns, as if dug into.
  let ticking = false;
  addEventListener("scroll", () => {
    if (ticking || REDUCED) return;
    ticking = true;
    requestAnimationFrame(() => {
      const t = clamp(scrollY / innerHeight, 0, 1);
      g.setAttribute("transform", `rotate(${(t * 24).toFixed(2)}) scale(${(1 + t * 0.07).toFixed(3)})`);
      ticking = false;
    });
  }, { passive: true });
}
for (const bar of document.querySelectorAll(".bar span")) {
  bar.style.setProperty("--tone", fullness(parseFloat(bar.style.getPropertyValue("--fill"))));
}

/* ---------- The dig: burr, sunburst, dive, collect, trash ---------- */
const chart = document.querySelector("[data-chart]");
if (chart) {
  chart.classList.add("data");
  const R = 370, INNER = 100, RING = 66, RINGS = 4;
  const burrG = el("g");
  const ringsG = el("g");
  chart.append(ringsG, burrG);
  const paths = new Map();
  for (const n of nodes) {
    if (n.depth === 0) continue;
    const p = el("path", { stroke: "#141416", "stroke-width": 1.6, "stroke-linejoin": "round" });
    p.style.transition = "fill 620ms cubic-bezier(.22,1,.36,1), opacity 420ms";
    ringsG.append(p);
    paths.set(n, p);
  }
  const center = document.querySelector("[data-center]");
  const crumb = document.querySelector("[data-crumb]");
  const tray = document.querySelector("[data-tray]");
  const shelf = document.querySelector("[data-shelf]");
  const badge = document.querySelector("[data-badge]");
  const count = document.querySelector("[data-count]");
  const amount = document.querySelector("[data-amount] strong");
  const ring = document.querySelector("[data-ring]");
  const toast = document.querySelector("[data-toast]");
  const trashBtn = document.querySelector("[data-trash-btn]");

  // The view: which slice of the disk fills the circle, how deep, how far swept.
  const view = { x0: 0, x1: 1, d: 0, sweep: 0, burr: 0 };
  let focus = root;
  let collected = new Set();
  let gone = new Set();

  function arcPath(r0, r1, a0, a1) {
    const p = (r, a) => `${(r * Math.sin(a)).toFixed(2)},${(-r * Math.cos(a)).toFixed(2)}`;
    const large = a1 - a0 > Math.PI ? 1 : 0;
    if (a1 - a0 >= TAU - 1e-4) a1 = a0 + TAU - 1e-4;
    return `M${p(r0, a0)}L${p(r1, a0)}A${r1},${r1} 0 ${large} 1 ${p(r1, a1)}L${p(r0, a1)}A${r0},${r0} 0 ${large} 0 ${p(r0, a0)}Z`;
  }
  function drawRings() {
    const span = view.x1 - view.x0;
    for (const [n, p] of paths) {
      const k = n.depth - view.d;
      const a0 = clamp((n.x0 - view.x0) / span, 0, 1) * TAU * view.sweep;
      const a1 = clamp((n.x1 - view.x0) / span, 0, 1) * TAU * view.sweep;
      if (k < 0.6 || k > RINGS + 0.4 || a1 - a0 < 0.002 || view.sweep === 0) {
        p.setAttribute("d", "");
        continue;
      }
      const fade = clamp(RINGS + 0.4 - k, 0, 1) * clamp(k - 0.6, 0, 1) * 2;
      const r0 = INNER + (k - 1) * RING;
      p.setAttribute("d", arcPath(Math.max(r0, INNER * 0.6), r0 + RING - 1.5, a0, a1));
      p.style.opacity = gone.has(n) || [...gone].some((g) => isInside(n, g)) ? 0 : Math.min(1, fade);
    }
  }
  const isInside = (n, g) => n.x0 >= g.x0 && n.x1 <= g.x1 && n.depth > g.depth;
  function paint() {
    for (const [n, p] of paths) {
      const c = [...collected].some((g) => g === n || isInside(n, g));
      p.style.fill = colorFor(n, focus, c);
    }
  }

  const scanned = dataSpines(84, R);
  const before = unscannedSpines(scanned.length, 432 / 460, R);
  // A scan: the warning's red spines retract and grow back as the disk's folders.
  let morph = 1;
  function drawBurrAt(t) {
    const spines = scanned.map((s, i) => {
      const b = before[i];
      const m = clamp((morph - (i / scanned.length) * 0.5) / 0.5, 0, 1);
      const reach = m < 0.5 ? lerp(b.reach, b.reach * 0.15, easeCubic(m * 2)) : lerp(s.reach * 0.15, s.reach, easeCubic((m - 0.5) * 2));
      return { a: s.a, reach, color: m < 0.5 ? b.color : s.color, bead: m < 0.5 ? b.bead : true };
    });
    drawBurr(burrG, spines, R, t, true);
  }

  function setCenter(value, unit) {
    center.querySelector("strong").textContent = value;
    center.querySelector("span").textContent = unit;
  }
  function setCrumb(parts) {
    crumb.innerHTML = parts.map((p, i) => (i === parts.length - 1 ? `<b>${p}</b>` : p)).join("<i>›</i>");
  }
  function to(target, duration, ease = easeQuint) {
    const from = { ...view };
    tween("view", duration, ease, (t) => {
      for (const k of Object.keys(target)) view[k] = lerp(from[k], target[k], t);
      drawRings();
    });
  }
  function showBurr(on) {
    const from = view.burr;
    const target = on ? 1 : 0;
    if (from === target) return;
    tween("burr", on ? 1300 : 380, on ? (t) => t : easeCubic, (t) => {
      view.burr = lerp(from, target, t);
      drawBurrAt(view.burr);
    });
  }
  const fmt = (gb) => (Math.round(gb * 10) / 10).toString();

  // The tray, as the app draws it.
  let timers = [];
  const later = (ms, fn) => timers.push(setTimeout(fn, REDUCED ? 0 : ms));
  function clearTimers() { timers.forEach(clearTimeout); timers = []; }
  function flare() { badge.classList.remove("flare"); void badge.offsetWidth; badge.classList.add("flare"); }
  function trayState(items) {
    shelf.replaceChildren();
    tray.classList.toggle("has", items.length > 0);
    tray.classList.remove("counting");
    count.textContent = items.length || "";
    amount.textContent = fmt(items.reduce((s, n) => s + n.size, 0));
    trashBtn.lastChild.textContent = "Move to Trash";
    for (const n of items) addTile(n, false);
  }
  function addTile(n, animate) {
    const icon = n.name.endsWith(".zip") ? ICON.zip : n.name.endsWith(".mov") ? ICON.mov : ICON.d;
    const tile = document.createElement("div");
    tile.className = "tile";
    if (!animate) tile.style.animation = "none";
    const tint = colorFor(n, focus, false);
    tile.innerHTML = `<b><svg viewBox="0 0 24 24" fill="none" stroke="${tint}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${icon}</svg>${fmt(n.size)} GB</b><span>${n.name}</span>`;
    shelf.append(tile);
  }
  function collectAll(animate) {
    if (!animate) {
      collected = new Set(collectNodes);
      paint();
      trayState(collectNodes);
      return;
    }
    collectNodes.forEach((n, i) => later(260 + i * 420, () => {
      collected.add(n);
      paint();
      tray.classList.add("has");
      addTile(n, true);
      count.textContent = collected.size;
      amount.textContent = fmt([...collected].reduce((s, m) => s + m.size, 0));
      flare();
    }));
  }

  const steps = {
    scan() {
      focus = root; collected.clear(); gone.clear();
      tray.style.opacity = 0; tray.classList.add("off");
      center.classList.remove("off");
      setCenter("94", "%");
      setCrumb(["Macintosh HD"]);
      to({ x0: 0, x1: 1, d: 0, sweep: 0 }, 300, easeCubic);
      cancelAnimationFrame(tweens.get("burr"));
      morph = 0;
      view.burr = 1;
      tween("morph", 1600, (t) => t, (t) => { morph = t; drawBurrAt(view.burr); });
      trayState([]);
    },
    explore() {
      focus = root; collected.clear(); gone.clear();
      tray.style.opacity = 1; tray.classList.add("off");
      setCenter("432", "GB");
      setCrumb(["Macintosh HD"]);
      paint();
      showBurr(false);
      to({ x0: 0, x1: 1, d: 0, sweep: 1 }, view.sweep < 1 ? 900 : 620);
      trayState([]);
    },
    dive() {
      focus = you; collected.clear(); gone.clear();
      tray.style.opacity = 1; tray.classList.add("off");
      setCenter("160", "GB");
      setCrumb(["Macintosh HD", "Users", "you"]);
      paint();
      showBurr(false);
      to({ x0: you.x0, x1: you.x1, d: you.depth, sweep: 1 }, 620);
      trayState([]);
    },
    collect(fromTrash) {
      focus = you; gone.clear();
      tray.style.opacity = 1; tray.classList.remove("off");
      setCenter("160", "GB");
      setCrumb(["Macintosh HD", "Users", "you"]);
      showBurr(false);
      to({ x0: you.x0, x1: you.x1, d: you.depth, sweep: 1 }, 620);
      if (fromTrash || collected.size === collectNodes.length) collectAll(false);
      else { collected.clear(); paint(); trayState([]); collectAll(true); }
    },
    trash() {
      steps.collect(true);
      // Five seconds, the badge red, a white ring draining clockwise.
      tray.classList.add("counting");
      trashBtn.lastChild.textContent = "Stop";
      let left = 5;
      count.textContent = left;
      ring.style.transition = "none";
      ring.style.strokeDashoffset = 0;
      void ring.getBoundingClientRect();
      ring.style.transition = REDUCED ? "none" : "stroke-dashoffset 5s linear";
      ring.style.strokeDashoffset = 169.6;
      for (let s = 1; s <= 4; s++) later(s * 1000, () => { count.textContent = 5 - s; flare(); });
      later(5000, () => {
        tray.classList.remove("counting");
        for (const t of shelf.children) t.classList.add("out");
        gone = new Set(collectNodes);
        drawRings();
        setCenter(fmt(you.size - collectedGB), "GB");
      });
      later(5500, () => {
        trayState([]);
        toast.classList.add("on");
      });
      later(9500, () => toast.classList.remove("on"));
    },
  };

  let current = null;
  function go(name) {
    if (name === current) return;
    const prev = current;
    current = name;
    clearTimers();
    toast.classList.remove("on");
    if (name === "collect" && prev === "trash") steps.collect(true);
    else steps[name]();
  }

  drawRings();
  const stepEls = document.querySelectorAll("[data-step]");
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      stepEls.forEach((s) => s.classList.toggle("on", s === e.target));
      go(e.target.dataset.step);
    }
  }, { rootMargin: matchMedia("(max-width: 900px)").matches ? "-58% 0px -40% 0px" : "-48% 0px -48% 0px" });
  stepEls.forEach((s) => io.observe(s));
  stepEls[0].classList.add("on");
  // Until the Scan step is reached, the drawing waits as the warning's red burr.
  morph = 0;
  view.burr = 1;
  tray.style.opacity = 0;
  tray.classList.add("off");
  setCenter("94", "%");
  drawBurrAt(1);
}

/* ---------- Only the stage in view glows ---------- */
const lit = new IntersectionObserver((entries) => {
  for (const e of entries) e.target.classList.toggle("lit", e.isIntersecting);
}, { threshold: 0.18 });
document.querySelectorAll(".stage, .dig").forEach((s) => lit.observe(s));
document.querySelectorAll(".shot").forEach((s) => s.classList.add("data"));

/* ---------- Nav ---------- */
const nav = document.getElementById("nav");
addEventListener("scroll", () => nav.classList.toggle("solid", scrollY > 40), { passive: true });

/* ---------- The cursor light inside buttons ---------- */
for (const btn of document.querySelectorAll(".btn")) {
  btn.addEventListener("pointermove", (e) => {
    const r = btn.getBoundingClientRect();
    btn.style.setProperty("--mx", `${e.clientX - r.left}px`);
    btn.style.setProperty("--my", `${e.clientY - r.top}px`);
  });
}

/* ---------- Downloads: the right file for this computer ---------- */
(async function downloads() {
  const ua = navigator.userAgent;
  const os = /Windows/.test(ua) ? "win" : /Macintosh|Mac OS X/.test(ua) && !/iPhone|iPad/.test(ua) ? "mac" : null;
  const names = { mac: "macOS", win: "Windows" };
  if (os) {
    for (const label of document.querySelectorAll("[data-download-label]")) label.textContent = `Download for ${names[os]}`;
    for (const ic of document.querySelectorAll("[data-os-icon] use")) ic.setAttribute("href", os === "mac" ? "#i-apple" : "#i-windows");
  }
  let release;
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) return;
    release = await res.json();
  } catch { return; }
  const asset = (re) => release.assets && release.assets.find((a) => re.test(a.name));
  const files = { mac: asset(/arm64\.dmg$/), win: asset(/setup\.exe$/) };
  for (const a of document.querySelectorAll("[data-asset]")) {
    const f = files[a.dataset.asset];
    if (f) a.href = f.browser_download_url;
  }
  if (os && files[os]) {
    for (const a of document.querySelectorAll("[data-download]")) a.href = files[os].browser_download_url;
  }
  const other = os === "mac" ? "win" : "mac";
  const note = document.querySelector("[data-download-note]");
  if (note && release.tag_name) {
    const version = release.tag_name.replace(/^v/, "");
    note.innerHTML = os
      ? `Version ${version}${os === "mac" ? " for Apple silicon" : ""} · Also for <a href="${files[other] ? files[other].browser_download_url : release.html_url}">${names[other]}</a>`
      : `Version ${version}, for <a href="${files.mac ? files.mac.browser_download_url : release.html_url}">macOS</a> and <a href="${files.win ? files.win.browser_download_url : release.html_url}">Windows</a>`;
  }
})();
