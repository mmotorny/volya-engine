// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The ergonomic mobile mode (the app's setting, stored as "experimental",
// #802) on the screens that show the game's cursor (the user's design,
// #807), in place of the standard mode's on-screen stick and cursor
// buttons (touch-controls.js, #856):
//
// - a touch is a direct mouse click where it lands: down presses the left
//   button there, a moving finger drags it, the lift releases it (the game
//   then draws no cursor: the engine's `touch_direct`). The press waits
//   until the finger moves past TAP_SLOP, lifts, or has been down
//   PRESS_DELAY_MS, so a second finger landing in that time makes a pinch
//   instead of a click (as a scroll view delays its content's touches; our
//   value, shorter than UIScrollView's believed 150 ms);
// - two fingers pinch the whole screen's picture, magnifying it about
//   them up to MAX_ZOOM, and drag it while magnified: the picture's point
//   first under their midpoint stays under it, and the picture always
//   covers the screen (an edge stops it until the fingers come back). A finger joining a press already made, or a third finger,
//   does nothing; once a pinch has begun, its fingers click nothing.
//
// The recognizer is pure: positions are the page's (client) CSS pixels,
// times page milliseconds (Event.timeStamp), the screen `size()` CSS
// pixels. Each call returns what to do: ["press" | "drag" | "release",
// id, x, y, time] for the left button at a page position, and ["zoom",
// zoom] for the picture's new magnification, `{ scale, x, y }`: a page
// point p of the unmagnified picture shows at (x, y) + scale · p. The
// caller runs `tick(now)` at `deadline()`.

import { TAP_SLOP } from "./touch-wheel.js";

/** A press waits this long for a second finger, milliseconds (our choice). */
export const PRESS_DELAY_MS = 100;
/** The most a pinch magnifies the picture (our choice). */
export const MAX_ZOOM = 4;
/** The picture unmagnified. */
export const NO_ZOOM = Object.freeze({ scale: 1, x: 0, y: 0 });

/** The CSS transform that shows `zoom` (transform-origin at the top left). */
export const zoomTransform = ({ scale, x, y }) => (scale === 1 && x === 0 && y === 0 ? "" : `translate(${x}px, ${y}px) scale(${scale})`);

/** Where page point `[x, y]` of the unmagnified picture shows under `zoom`. */
export const zoomed = ({ scale, x, y }, [px, py]) => [x + scale * px, y + scale * py];

/** A recognizer; `size()` is the screen's `[width, height]`, CSS pixels. */
export function createDirectTouch(size) {
  // pointerId → [x, y]
  const fingers = new Map();
  // "idle"; "pending": one finger, its press not yet made; "pressed": it
  // holds the left button; "pinch": two fingers zoom; "done": nothing
  // until every finger lifts.
  let mode = "idle";
  let lead = null;
  let start = [0, 0];
  let downAt = 0;
  let pair = [];
  let zoom = NO_ZOOM;
  // The pinch's start: the fingers' distance, the magnification, and the
  // picture's point under their midpoint.
  let anchor = null;

  const press = () => {
    mode = "pressed";
    return ["press", lead, ...start, downAt];
  };
  const rest = () => (fingers.size === 0 ? "idle" : "done");

  const gap = ([a, b]) => Math.hypot(a[0] - b[0], a[1] - b[1]);
  const mid = ([a, b]) => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
  /** The pinch starts with the pair at `points`. */
  const begin = (points) => {
    const m = mid(points);
    anchor = { gap: gap(points), scale: zoom.scale, at: [(m[0] - zoom.x) / zoom.scale, (m[1] - zoom.y) / zoom.scale] };
  };
  /** The pair now at `points`: magnified by their distance over the
   * start's, the start's point under their midpoint, from the pinch's
   * start (so the edges, which stop the picture, don't shift it). */
  const pinch = (points) => {
    const [w, h] = size();
    const d = gap(points);
    const scale = anchor.gap > 0 && d > 0 ? Math.min(MAX_ZOOM, Math.max(1, (anchor.scale * d) / anchor.gap)) : zoom.scale;
    const m = mid(points);
    const clamp = (v, extent) => Math.min(0, Math.max(extent * (1 - scale), v));
    const next = { scale, x: clamp(m[0] - scale * anchor.at[0], w), y: clamp(m[1] - scale * anchor.at[1], h) };
    if (next.scale === zoom.scale && next.x === zoom.x && next.y === zoom.y) return [];
    zoom = next.scale === 1 ? NO_ZOOM : Object.freeze(next);
    return [["zoom", zoom]];
  };

  return {
    /** The picture's magnification now. */
    get zoom() {
      return zoom;
    },

    down(id, x, y, time) {
      if (fingers.has(id)) return [];
      fingers.set(id, [x, y]);
      if (fingers.size === 1) {
        mode = "pending";
        lead = id;
        start = [x, y];
        downAt = time;
      } else if (fingers.size === 2 && mode === "pending") {
        mode = "pinch";
        pair = [lead, id];
        begin(pair.map((f) => fingers.get(f)));
      }
      return [];
    },

    move(id, x, y, time) {
      const before = fingers.get(id);
      if (!before || (before[0] === x && before[1] === y)) return [];
      if (mode === "pinch" && pair.includes(id)) {
        fingers.set(id, [x, y]);
        return pinch(pair.map((f) => fingers.get(f)));
      }
      fingers.set(id, [x, y]);
      if (id !== lead) return [];
      if (mode === "pending") {
        if (Math.hypot(x - start[0], y - start[1]) <= TAP_SLOP) return [];
        return [press(), ["drag", id, x, y, time]];
      }
      if (mode === "pressed") return [["drag", id, x, y, time]];
      return [];
    },

    up(id, x, y, time) {
      if (!fingers.has(id)) return [];
      const out = this.move(id, x, y, time);
      fingers.delete(id);
      if (mode === "pinch" && pair.includes(id)) {
        mode = rest();
      } else if (id === lead && mode === "pending") {
        out.push(press(), ["release", id, x, y, time]);
        mode = rest();
      } else if (id === lead && mode === "pressed") {
        out.push(["release", id, x, y, time]);
        mode = rest();
      } else if (fingers.size === 0) {
        mode = "idle";
      }
      return out;
    },

    /** The platform took the touch: a press made is released where the
     * finger was, a pending one never made. */
    cancel(id, time) {
      const at = fingers.get(id);
      if (!at) return [];
      fingers.delete(id);
      const out = id === lead && mode === "pressed" ? [["release", id, ...at, time]] : [];
      if (id === lead || (mode === "pinch" && pair.includes(id))) mode = rest();
      else if (fingers.size === 0) mode = "idle";
      return out;
    },

    /** The press held still past PRESS_DELAY_MS, once `deadline()` passed. */
    tick(time) {
      if (mode !== "pending" || time - downAt < PRESS_DELAY_MS) return [];
      return [press()];
    },

    /** When `tick` may act next, or null. */
    deadline() {
      return mode === "pending" ? downAt + PRESS_DELAY_MS : null;
    },

    /** Forgets every touch; a press made is released. The zoom stays. */
    reset(time) {
      const at = fingers.get(lead) ?? start;
      const out = mode === "pressed" ? [["release", lead, ...at, time]] : [];
      fingers.clear();
      mode = "idle";
      return out;
    },

    /** The picture back to its own size. */
    unzoom() {
      if (zoom === NO_ZOOM) return [];
      zoom = NO_ZOOM;
      return [["zoom", zoom]];
    },
  };
}
