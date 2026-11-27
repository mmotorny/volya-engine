// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Touch as a touchpad wherever the game shows its mouse cursor: a Volya
// Engine addition, decided by the user (#108). A touch never clicks where it
// lands; the cursor moves and clicks as on a laptop's touchpad (after
// libinput's and Android's tap and long-press gestures):
//
// - a swipe anywhere moves the cursor by its motion (the engine applies the
//   acceleration and the menu's sensitivity: touchpad_gain in
//   crates/volya-engine-core/src/input.rs);
// - a tap (lifted within LONG_PRESS_MS, moved at most TAP_SLOP) clicks at
//   the cursor;
// - a touch beginning less than DOUBLE_TAP_MS after a tap's lift is a
//   second touch: lifted as a tap, a double click; moved past TAP_SLOP or
//   held LONG_PRESS_MS, it presses (a single press, never a double click's)
//   and holds until lifted: double-tap and hold, then move, drags (sliders,
//   scroll bars, list thumbs);
// - two fingers swiped up or down scroll, one wheel step per SCROLL_STEP of
//   their travel, content following the fingers; pinched, one wheel step
//   per PINCH_STEP of the change of their distance, apart as wheel down
//   (the PDA map's zoom in, #743): whichever the fingers first do past
//   TAP_SLOP, they go on doing while both stay down;
// - a long press (LONG_PRESS_MS within TAP_SLOP) is a right click, when the
//   time is reached or at the lift, whichever comes first.
//
// The recognizer is pure: positions in CSS pixels, times in page
// milliseconds (Event.timeStamp). Each call returns what to send, input
// messages of src/input.rs: ["cursor_motion", dx, dy, speed],
// ["cursor_button", "left" | "right", pressed, time, double] and
// ["wheel", steps]. The caller runs `tick(now)` at `deadline()`.

/** A tap moves at most this far from where it began, CSS pixels (our
 * choice; Android's touch slop is 8 dp, iOS's about 10 pt). */
export const TAP_SLOP = 10;
/** A touch held this long within TAP_SLOP is a long press, milliseconds
 * (Android's long-press timeout); lifted earlier, a tap. */
export const LONG_PRESS_MS = 500;
/** A touch beginning less than this after a tap's lift is a second touch,
 * milliseconds (the engine's double-click time, DOUBLE_CLICK_MS). */
export const DOUBLE_TAP_MS = 250;
/** Two fingers' travel per wheel step, CSS pixels (our choice). */
export const SCROLL_STEP = 40;
/** The change of two fingers' distance per wheel step, CSS pixels (our
 * choice). */
export const PINCH_STEP = 40;
/** A swipe's speed is smoothed over about this long, milliseconds (as the
 * level's look drags, touch-controls.js). */
export const SPEED_SMOOTH_MS = 75;

/** A touchpad recognizer. */
export function createTouchpad() {
  // pointerId → { start, at }
  const fingers = new Map();
  // "pending": one finger, a tap, a long press or a move yet; "second": the
  // touch after a tap, a double click or a drag yet; "moving": it moves the
  // cursor; "spent": a long press fired, it may still move; "drag": the
  // lead finger drags; "scroll": two fingers; "done": nothing until every
  // finger lifts.
  let mode = "idle";
  // The left button held by a drag, apart from the mode: whatever ends the
  // lead finger, cancels a finger or resets releases it.
  let held = false;
  // The finger moving the cursor or dragging, and its smoothed speed.
  let lead = null;
  let speed = 0;
  let pending = 0;
  let last = 0;
  let downAt = 0;
  // When the last tap lifted, for the second touch.
  let tapped = null;
  let scroll = 0;
  // Two fingers: null until they scroll or pinch, then which; their
  // distance as last seen and its change not yet sent.
  let gesture = null;
  let spread = 0;
  let pinch = 0;
  const distance = () => {
    const [a, b] = [...fingers.values()];
    return Math.hypot(a.at[0] - b.at[0], a.at[1] - b.at[1]);
  };

  const button = (name, pressed, time, double = false) => ["cursor_button", name, pressed, time, double];
  const click = (name, time, double = false) => [button(name, true, time, double), button(name, false, time)];
  const press = (time) => {
    held = true;
    mode = "drag";
    return button("left", true, time);
  };
  const release = (time) => {
    if (!held) return [];
    held = false;
    return [button("left", false, time)];
  };
  const motion = (dx, dy, time) => {
    pending += Math.hypot(dx, dy);
    const dt = time - last;
    if (dt > 0) {
      const weight = 1 - Math.exp(-dt / SPEED_SMOOTH_MS);
      speed += weight * ((pending / dt) * 1000 - speed);
      pending = 0;
      last = time;
    }
    return ["cursor_motion", dx, dy, speed];
  };
  const leadGone = () => (fingers.size === 0 ? "idle" : "done");

  return {
    down(id, x, y, time) {
      if (fingers.has(id)) return [];
      fingers.set(id, { start: [x, y], at: [x, y] });
      if (fingers.size === 1) {
        lead = id;
        downAt = time;
        speed = 0;
        pending = 0;
        last = time;
        mode = tapped !== null && time - tapped < DOUBLE_TAP_MS ? "second" : "pending";
        tapped = null;
        return [];
      }
      if (fingers.size === 2 && ["pending", "second", "moving", "spent"].includes(mode)) {
        mode = "scroll";
        scroll = 0;
        gesture = null;
        spread = distance();
        pinch = 0;
      }
      return [];
    },

    move(id, x, y, time) {
      const finger = fingers.get(id);
      if (!finger) return [];
      const [dx, dy] = [x - finger.at[0], y - finger.at[1]];
      if (dx === 0 && dy === 0) return [];
      finger.at = [x, y];
      if (mode === "scroll") {
        // The fingers' mean travel; up is toward the content's end (wheel
        // down), as content follows the fingers.
        scroll += dy / fingers.size;
        if (fingers.size === 2) {
          const d = distance();
          pinch += d - spread;
          spread = d;
        }
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
      }
      if (id !== lead) return [];
      if (mode === "pending" || mode === "spent" || mode === "second") {
        const [sx, sy] = [x - finger.start[0], y - finger.start[1]];
        if (Math.hypot(sx, sy) <= TAP_SLOP) return [];
        // Past the slop: a move (a drag after a tap), from where it began.
        if (mode === "second") return [press(time), motion(sx, sy, time)];
        mode = "moving";
        return [motion(sx, sy, time)];
      }
      if (mode === "moving" || mode === "drag") return [motion(dx, dy, time)];
      return [];
    },

    up(id, x, y, time) {
      if (!fingers.has(id)) return [];
      const out = this.move(id, x, y, time);
      fingers.delete(id);
      if (id !== lead) {
        // Another finger: a drag goes on; a scroll ends.
        if (mode !== "drag") mode = leadGone();
        return out;
      }
      const quick = time - downAt < LONG_PRESS_MS;
      if (mode === "pending" && quick) {
        out.push(...click("left", time));
        tapped = time;
      } else if (mode === "pending") {
        // Held still past the long press, its timer not yet run.
        out.push(...click("right", time));
      } else if (mode === "second" && quick) {
        out.push(...click("left", time, true));
      } else if (mode === "second") {
        out.push(press(time));
      }
      out.push(...release(time));
      mode = leadGone();
      return out;
    },

    /** The platform took the touch: released, without acting. */
    cancel(id, time) {
      if (!fingers.has(id)) return [];
      fingers.delete(id);
      const out = release(time);
      mode = leadGone();
      return out;
    },

    /** The long press or the held second touch, once `deadline()` has passed. */
    tick(time) {
      if (time - downAt < LONG_PRESS_MS) return [];
      if (mode === "pending") {
        mode = "spent";
        return click("right", time);
      }
      if (mode === "second") return [press(time)];
      return [];
    },

    /** When `tick` may act next, or null. */
    deadline() {
      return mode === "pending" || mode === "second" ? downAt + LONG_PRESS_MS : null;
    },

    /** Forgets every touch (they go on unheard); a held drag is released. */
    reset(time) {
      const out = release(time);
      fingers.clear();
      mode = "idle";
      tapped = null;
      return out;
    },
  };
}
