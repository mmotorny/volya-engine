// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Two fingers on the picture as the mouse wheel wherever the game shows
// its cursor in the standard mobile mode, where the on-screen stick and
// cursor buttons drive the cursor (touch-controls.js, #856); our design:
//
// - two fingers swiped up or down scroll, one wheel step per SCROLL_STEP of
//   their travel, content following the fingers; pinched, one wheel step
//   per PINCH_STEP of the change of their distance, apart as wheel down
//   (the PDA map's zoom in, #743): whichever the fingers first do past
//   TAP_SLOP, they go on doing while both stay down;
// - one finger does nothing: a touch never clicks where it lands.
//
// The recognizer is pure: positions in CSS pixels. Each call returns the
// input messages of src/input.rs to send: ["wheel", steps].

/** Two fingers move at least this far before they scroll or pinch, CSS
 * pixels (our choice; Android's touch slop is 8 dp, iOS's about 10 pt). */
export const TAP_SLOP = 10;
/** Two fingers' travel per wheel step, CSS pixels (our choice). */
export const SCROLL_STEP = 40;
/** The change of two fingers' distance per wheel step, CSS pixels (our
 * choice). */
export const PINCH_STEP = 40;

/** A two-finger wheel recognizer. */
export function createTouchWheel() {
  // pointerId → [x, y]
  const fingers = new Map();
  // "idle": fewer than two fingers yet; "two": two fingers, scrolling or
  // pinching; "done": nothing until every finger lifts.
  let mode = "idle";
  let scroll = 0;
  // null until the fingers scroll or pinch, then which; their distance as
  // last seen and its change not yet sent.
  let gesture = null;
  let spread = 0;
  let pinch = 0;
  const distance = () => {
    const [a, b] = [...fingers.values()];
    return Math.hypot(a[0] - b[0], a[1] - b[1]);
  };
  const lifted = () => {
    mode = fingers.size === 0 ? "idle" : "done";
  };

  return {
    down(id, x, y) {
      if (fingers.has(id)) return [];
      fingers.set(id, [x, y]);
      if (fingers.size === 2 && mode === "idle") {
        mode = "two";
        scroll = 0;
        gesture = null;
        spread = distance();
        pinch = 0;
      } else if (fingers.size > 2) {
        mode = "done";
      }
      return [];
    },

    move(id, x, y) {
      const finger = fingers.get(id);
      if (!finger) return [];
      const dy = y - finger[1];
      fingers.set(id, [x, y]);
      if (mode !== "two") return [];
      // The fingers' mean travel; up is toward the content's end (wheel
      // down), as content follows the fingers.
      scroll += dy / 2;
      const d = distance();
      pinch += d - spread;
      spread = d;
      if (gesture === null) {
        if (Math.max(Math.abs(scroll), Math.abs(pinch)) <= TAP_SLOP) return [];
        // A pinch changes the distance more than twice the mean travel
        // (one finger moving straight away from a still one is a scroll).
        gesture = Math.abs(pinch) > 2 * Math.abs(scroll) ? "pinch" : "scroll";
      }
      if (gesture === "pinch") {
        const steps = Math.trunc(pinch / PINCH_STEP);
        if (steps === 0) return [];
        pinch -= steps * PINCH_STEP;
        return [["wheel", steps]];
      }
      const steps = Math.trunc(scroll / SCROLL_STEP);
      if (steps === 0) return [];
      scroll -= steps * SCROLL_STEP;
      return [["wheel", -steps]];
    },

    up(id, x, y) {
      if (!fingers.has(id)) return [];
      const out = this.move(id, x, y);
      fingers.delete(id);
      lifted();
      return out;
    },

    /** The platform took the touch: forgotten. */
    cancel(id) {
      if (!fingers.delete(id)) return [];
      lifted();
      return [];
    },

    /** Forgets every touch (they go on unheard). */
    reset() {
      fingers.clear();
      mode = "idle";
      return [];
    },
  };
}
