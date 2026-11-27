// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The art of the level's touch controls (touch-controls.js; design A of
// #105): instruments in the style of the game's HUD, built from the game's
// own UI textures, which the engine decodes from the player's imported
// game (the `ui_images` op); nothing of the game is shipped. Until they
// arrive (or where there is no game, as on the UX audit page) close vector
// approximations in the same palette stand in.
//
// - Discs: the HUD panels' grainy gunmetal (`ui\ui_hud_frame_back`, tiled
//   at TEXEL CSS pixels per texel) over a dark backing for legibility over
//   bright scenes. Their thin rims are drawn by style.css at the weight of
//   the minimap's ring (`ui\ui_round_map_area`).
// - Glyphs: the HUD's posture icons (`ui_hud_soldier_crouch`, `_creep`)
//   as silhouettes (their alpha with the one-texel scanline gaps
//   closed by the engine) and our own Jump arrow (after the proportions of
//   `ui\ui_map_arrow_02`), Menu bars, Inventory backpack (the UI textures
//   have no inventory icon), Console prompt (#642) and Torch beam (the
//   torch's one picture, its icon, is Night vision's), redrawn with
//   scanlines at device pixels (every third row dimmed), so they can't
//   beat against the screen's pixels.
// - PDA's glyph: the HUD's PDA icon (`ui_hud_icon_PDA`, the flashing
//   icon of hud-scope.md spec 2) as a silhouette (#730).
// - Night vision's glyph: the inventory icon of the torch (`device_torch`,
//   which has the night vision, torch.md 1.1; `ui\ui_icon_equipment`)
//   engraved: its brightness as alpha, so its lenses show where a
//   silhouette would be one blot.
// - Colours: teal, the crouch icon's mean texel colour; grey, the
//   crosshair's (`[hud_cursor] cross_color`).
//
// Canvases are drawn at devicePixelRatio and redrawn when it changes.

/** CSS pixels per texel of the tiled gunmetal (our choice). */
export const TEXEL = 0.75;
/** Scanlines: every PERIOD-th device-pixel row kept at DIM (our design,
 * after the HUD icons' look). */
const PERIOD = 3;
const DIM = 0.2;

/** What the controls take from the game, in `art` order. */
const REQUESTS = [
  ["frame", { path: "ui\\ui_hud_frame_back" }],
  ["crouch", { id: "ui_hud_soldier_crouch", silhouette: true }],
  ["creep", { id: "ui_hud_soldier_creep", silhouette: true }],
  ["torchIcon", { item: "device_torch", engraving: true }],
  ["pda", { id: "ui_hud_icon_PDA", silhouette: true }],
];
/** Glyph sources whose transparent margins are trimmed. */
const TRIMMED = new Set(["crouch", "creep", "torchIcon", "pda"]);

/** The approximations' palette, close to the game's (our choice). */
const APPROXIMATE_TEAL = [36, 178, 162];
const APPROXIMATE_GREY = [178, 178, 178];

/** A canvas of `width` × `height` pixels, with its 2D context. */
function canvas(width, height) {
  const c = document.createElement("canvas");
  c.width = Math.max(1, width);
  c.height = Math.max(1, height);
  return [c, c.getContext("2d")];
}

/** `[r, g, b]` as a CSS colour, with `alpha`. */
export const css = ([r, g, b], alpha = 1) => `rgba(${r}, ${g}, ${b}, ${alpha})`;

/** `color` lifted toward white by `amount` (0–1): the bright teal. */
const lift = (color, amount) => color.map((c) => Math.round(c + (255 - c) * amount));

/** The bounding box of texels with alpha above `threshold` in `image`. */
function trim(image, threshold = 8) {
  const { width, height, data } = image;
  let [x0, y0, x1, y1] = [width, height, -1, -1];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (data[(y * width + x) * 4 + 3] <= threshold) continue;
      x0 = Math.min(x0, x);
      y0 = Math.min(y0, y);
      x1 = Math.max(x1, x);
      y1 = Math.max(y1, y);
    }
  }
  return x1 < 0 ? [0, 0, width, height] : [x0, y0, x1 - x0 + 1, y1 - y0 + 1];
}

/** A canvas holding one `ui_images` answer (`trimmed` of its margins). */
function pictureCanvas({ width, height, rgba }, trimmed) {
  const bytes = Uint8ClampedArray.from(atob(rgba), (ch) => ch.charCodeAt(0));
  const image = new ImageData(bytes, width, height);
  const [x, y, w, h] = trimmed ? trim(image) : [0, 0, width, height];
  const [c, g] = canvas(w, h);
  g.putImageData(image, -x, -y);
  return c;
}

/** The colours shared by every element, from the teal and the grey. */
function palette(teal, grey) {
  return { teal, tealHi: lift(teal, 0.35), grey, pale: lift(teal, 0.85) };
}

let loaded = null;

/**
 * The controls' art from the imported game, once per session: `call` runs
 * an engine op. A picture the engine can't give is replaced by its
 * approximation (named in `missing`), so the rest is kept and nothing is
 * decoded again. Rejects (and is tried again next time) only when the op
 * itself fails.
 */
export function gameArt(call) {
  loaded ??= (async () => {
    const started = performance.now();
    const answer = await call("ui_images", { images: REQUESTS.map(([, request]) => request) });
    // For measuring: the engine's decoding time and the whole wait.
    const art = { game: true, ms: answer.ms, missing: [] };
    let approximate = null;
    const images = {};
    REQUESTS.forEach(([name], i) => {
      const image = answer.images?.[i];
      if (image && !image.error) {
        images[name] = image;
        art[name] = pictureCanvas(image, TRIMMED.has(name));
      } else {
        approximate ??= approximateArt();
        art[name] = approximate[name];
        art.missing.push(name);
        console.warn(`touch controls: the game's ${name} picture: ${image?.error ?? "missing"}`);
      }
    });
    const teal = images.crouch?.mean ?? APPROXIMATE_TEAL;
    const cross = answer.cross_color;
    const grey = cross ? cross.slice(0, 3).map((c) => Math.round(c * 255)) : APPROXIMATE_GREY;
    art.totalMs = performance.now() - started;
    return Object.assign(art, palette(teal, grey));
  })().catch((error) => {
    loaded = null;
    throw error;
  });
  return loaded;
}

/** A repeatable pseudo-random sequence (the approximations stay the same
 * from one drawing to the next). */
function random(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

/** Approximate posture silhouettes in a 65 × 75 frame (our drawings): the
 * head `[x, y, r]`, then limbs as `[width, ...points]`, stroked round. */
const FIGURES = {
  // Upright crouch, aiming to the right.
  crouch: [
    [40, 10, 7],
    [14, [38, 18], [34, 40]],
    [5, [38, 21], [62, 19]],
    [7, [40, 24], [52, 27]],
    [12, [34, 40], [46, 50]],
    [10, [46, 50], [44, 69]],
    [12, [32, 42], [24, 56]],
    [10, [24, 56], [28, 70]],
    [6, [44, 71], [51, 72]],
    [6, [28, 72], [20, 72]],
  ],
  // Kneeling, aiming to the right.
  creep: [
    [34, 28, 7],
    [14, [32, 36], [28, 54]],
    [5, [34, 39], [62, 37]],
    [7, [36, 42], [50, 44]],
    [12, [28, 54], [44, 56]],
    [10, [44, 56], [44, 71]],
    [12, [28, 56], [20, 70]],
    [9, [20, 70], [6, 72]],
  ],
};

/** An approximate figure, drawn at `scale` × its frame. */
function figureCanvas([[hx, hy, hr], ...limbs], scale = 4) {
  const [c, g] = canvas(65 * scale, 75 * scale);
  g.scale(scale, scale);
  g.fillStyle = g.strokeStyle = "#fff";
  g.lineCap = "round";
  g.lineJoin = "round";
  for (const [width, ...points] of limbs) {
    g.lineWidth = width;
    g.beginPath();
    points.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
    g.stroke();
  }
  g.beginPath();
  g.arc(hx, hy, hr, 0, 2 * Math.PI);
  g.fill();
  const image = g.getImageData(0, 0, c.width, c.height);
  const [x, y, w, h] = trim(image);
  const [out, o] = canvas(w, h);
  o.putImageData(image, -x, -y);
  return out;
}

/** An approximate torch icon (our drawing of the headset, in its 100 × 50
 * frame): the housing faint, the two lenses' rims bright, as engraved. */
function torchIconCanvas(scale = 4) {
  const [c, g] = canvas(100 * scale, 50 * scale);
  g.scale(scale, scale);
  g.fillStyle = g.strokeStyle = "#fff";
  g.globalAlpha = 0.4;
  g.beginPath();
  [
    [12, 22],
    [30, 9],
    [56, 11],
    [95, 14],
    [94, 31],
    [58, 40],
    [42, 47],
    [12, 45],
  ].forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
  g.closePath();
  g.fill();
  g.lineWidth = 3;
  for (const [x, y, r] of [
    [17, 36, 8],
    [40, 38, 9.5],
  ]) {
    g.globalAlpha = 0.25;
    g.beginPath();
    g.arc(x, y, r, 0, 2 * Math.PI);
    g.fill();
    g.globalAlpha = 1;
    g.stroke();
  }
  return c;
}

/** An approximate PDA icon (our drawing of the handheld, in a 48 × 24
 * frame): its body with the screen cut out. */
function pdaIconCanvas(scale = 4) {
  const [c, g] = canvas(48 * scale, 24 * scale);
  g.scale(scale, scale);
  g.fillStyle = "#fff";
  g.beginPath();
  g.roundRect(4, 2, 40, 20, 4);
  g.roundRect(10, 5, 24, 14, 1);
  g.fill("evenodd");
  g.beginPath();
  g.arc(39, 12, 2.2, 0, 2 * Math.PI);
  g.fill();
  return c;
}

/** Close vector approximations of the game's art (no game data). */
export function approximateArt() {
  const [frame, g] = canvas(64, 64);
  const next = random(105);
  const image = g.createImageData(64, 64);
  for (let i = 0; i < image.data.length; i += 4) {
    const v = 128 + (next() - 0.5) * 50;
    image.data.set([v, v, v, 70 + next() * 45], i);
  }
  g.putImageData(image, 0, 0);
  return {
    game: false,
    frame,
    crouch: figureCanvas(FIGURES.crouch),
    creep: figureCanvas(FIGURES.creep),
    torchIcon: torchIconCanvas(),
    pda: pdaIconCanvas(),
    ...palette(APPROXIMATE_TEAL, APPROXIMATE_GREY),
  };
}

/** Sizes `c` to `width` × `height` CSS pixels at devicePixelRatio; returns
 * its context and the device pixels per CSS pixel. */
function fit(c, width, height) {
  const dpr = devicePixelRatio || 1;
  c.width = Math.max(1, Math.round(width * dpr));
  c.height = Math.max(1, Math.round(height * dpr));
  c.style.width = `${width}px`;
  c.style.height = `${height}px`;
  const g = c.getContext("2d");
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.clearRect(0, 0, c.width, c.height);
  g.imageSmoothingQuality = "high";
  return [g, c.width / width];
}

/** A gunmetal disc `size` CSS pixels across, its texels `texel` CSS pixels:
 * the dark backing at `backing`, the texture at `opacity`. */
export function paintDisc(c, art, size, { texel = TEXEL, backing = 0.4, opacity = 0.85 } = {}) {
  const [g, k] = fit(c, size, size);
  const r = (size / 2) * k;
  g.beginPath();
  g.arc(r, r, r, 0, 2 * Math.PI);
  g.fillStyle = `rgba(10, 12, 16, ${backing})`;
  g.fill();
  const pattern = g.createPattern(art.frame, "repeat");
  pattern.setTransform(new DOMMatrix().scale(texel * k));
  g.globalAlpha = opacity;
  g.fillStyle = pattern;
  g.fill();
}

/** `source` (a canvas) fitted in a `width` × `height` CSS-pixel box, its
 * alpha filled with `color`, scanlined. */
export function paintGlyph(c, source, width, height, color) {
  const [g, k] = fit(c, width, height);
  const scale = Math.min(c.width / source.width, c.height / source.height);
  const [w, h] = [source.width * scale, source.height * scale];
  g.drawImage(source, (c.width - w) / 2, (c.height - h) / 2, w, h);
  recolour(g, c, color);
  scanlines(g, c);
  return k;
}

function recolour(g, c, color) {
  g.globalCompositeOperation = "source-in";
  g.fillStyle = css(color);
  g.fillRect(0, 0, c.width, c.height);
  g.globalCompositeOperation = "source-over";
}

function scanlines(g, c) {
  g.globalCompositeOperation = "destination-out";
  g.fillStyle = `rgba(0, 0, 0, ${1 - DIM})`;
  for (let y = PERIOD - 1; y < c.height; y += PERIOD) g.fillRect(0, y, c.width, 1);
  g.globalCompositeOperation = "source-over";
}

/** Our Jump glyph in a 24 × 24 frame: a block arrow up (after the
 * proportions of `ui\ui_map_arrow_02`) over a ground line. */
export const JUMP = [
  [12, 2.5],
  [20.5, 11],
  [15.5, 11],
  [15.5, 18],
  [8.5, 18],
  [8.5, 11],
  [3.5, 11],
];
const GROUND = [4, 20.4, 16, 2.2];
/** Our Menu glyph: two bars (x, y, width, height in 24 × 24). */
const BARS = [
  [7.5, 5.5, 3, 13],
  [13.5, 5.5, 3, 13],
];

/** Our Inventory glyph in 24 × 24: a backpack's carry loop (x, y, width,
 * height, radius, and its hole), its bag, the flap's edge and a pocket
 * outline (cut, then its inside filled back). */
const BAG = {
  loop: [8.5, 1.5, 7, 5.5, 2],
  hole: [10.3, 3.2, 3.4, 3.8, 1],
  bag: [4.5, 5.5, 15, 16.5, 3.5],
  flap: [4.5, 11, 15, 1.6],
  pocket: [8, 14.4, 8, 5.2, 1.4],
  inside: [9.5, 15.9, 5, 2.2, 0.6],
};

/** Our Torch glyph in 24 × 24: a lamp (its body, x, y, width, height,
 * radius, and its face) and its beam, three rays spreading (stroked). */
const LAMP = { body: [2, 7.5, 7.5, 9, 2.2], face: [9, 5.5, 2.6, 13, 0.8] };
const RAYS = [
  [14, 8.6, 21.5, 5.4],
  [14, 12, 22, 12],
  [14, 15.4, 21.5, 18.6],
];

/** Our Console glyph in 24 × 24 (#642): a prompt, `>_`, its chevron
 * stroked (points) and its underscore a bar. */
const PROMPT = [
  [4.5, 6],
  [10.5, 12],
  [4.5, 18],
];
const UNDERSCORE = [12.5, 16.9, 8, 2.2];

/** A vector glyph source (our Jump arrow, Menu bars, Inventory backpack,
 * Torch beam or Console prompt) at `size` device pixels. */
export function vectorGlyph(name, size) {
  const [c, g] = canvas(size, size);
  g.scale(size / 24, size / 24);
  g.fillStyle = "#fff";
  if (name === "torch") {
    g.beginPath();
    g.roundRect(...LAMP.body);
    g.roundRect(...LAMP.face);
    g.fill();
    g.strokeStyle = "#fff";
    g.lineWidth = 2.2;
    g.lineCap = "round";
    for (const [x0, y0, x1, y1] of RAYS) {
      g.beginPath();
      g.moveTo(x0, y0);
      g.lineTo(x1, y1);
      g.stroke();
    }
  } else if (name === "console") {
    g.strokeStyle = "#fff";
    g.lineWidth = 2.4;
    g.lineCap = "round";
    g.lineJoin = "round";
    g.beginPath();
    PROMPT.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
    g.stroke();
    g.fillRect(...UNDERSCORE);
  } else if (name === "jump") {
    g.beginPath();
    JUMP.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
    g.closePath();
    g.fill();
    g.fillRect(...GROUND);
  } else if (name === "inventory") {
    g.beginPath();
    g.roundRect(...BAG.loop);
    g.roundRect(...BAG.hole);
    g.fill("evenodd");
    g.beginPath();
    g.roundRect(...BAG.bag);
    g.fill();
    g.globalCompositeOperation = "destination-out";
    g.fillRect(...BAG.flap);
    g.beginPath();
    g.roundRect(...BAG.pocket);
    g.fill();
    g.globalCompositeOperation = "source-over";
    g.beginPath();
    g.roundRect(...BAG.inside);
    g.fill();
  } else {
    for (const bar of BARS) g.fillRect(...bar);
  }
  return c;
}
