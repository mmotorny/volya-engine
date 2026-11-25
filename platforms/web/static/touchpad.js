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
// - a touch within DOUBLE_TAP_MS of a tap's lift presses at once and holds
//   until lifted: double-tap and hold, then move, drags (sliders, scroll
//   bars); a quick second tap is a double click;
// - two fingers swiped up or down scroll, one wheel step per SCROLL_STEP of
//   their travel, content following the fingers;
// - a long press (LONG_PRESS_MS within TAP_SLOP) is a right click.
//
// The recognizer is pure: positions in CSS pixels, times in page
// milliseconds (Event.timeStamp). Each call returns what to send, input
// messages of src/input.rs: ["cursor_motion", dx, dy, speed],
// ["cursor_button", "left" | "right", pressed, time] and ["wheel", steps].
// The caller runs `tick(now)` at `deadline()` (the long press).

/** A tap moves at most this far from where it began, CSS pixels (our
 * choice; Android's touch slop is 8 dp, iOS's about 10 pt). */
export const TAP_SLOP = 10;
/** A touch held this long within TAP_SLOP is a long press, milliseconds
 * (Android's long-press timeout); lifted earlier, a tap. */
export const LONG_PRESS_MS = 500;
/** A touch beginning this soon after a tap's lift drags, milliseconds (the
 * engine's double-click time, so a quick second tap is a double click). */
export const DOUBLE_TAP_MS = 250;
/** Two fingers' travel per wheel step, CSS pixels (our choice). */
export const SCROLL_STEP = 40;
/** A swipe's speed is smoothed over about this long, milliseconds (as the
 * level's look drags, touch-controls.js). */
export const SPEED_SMOOTH_MS = 75;

/** A touchpad recognizer. */
export function createTouchpad() {
  // pointerId → { start, at, time }
  const fingers = new Map();
  // "pending": one finger, a tap or a long press yet; "moving": it moves
  // the cursor; "spent": a long press fired, it may still move; "drag": the
  // left button held; "scroll": two fingers; "done": nothing until every
  // finger lifts.
  let mode = "idle";
  // The finger moving the cursor or dragging, and its smoothed speed.
  let lead = null;
  let speed = 0;
  let pending = 0;
  let last = 0;
  let downAt = 0;
  // When the last tap lifted, for the double tap.
  let tapped = null;
  let scroll = 0;

  const button = (name, pressed, time) => ["cursor_button", name, pressed, time];
  const click = (name, time) => [button(name, true, time), button(name, false, time)];
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
  const startMotion = (time) => {
    speed = 0;
    pending = 0;
    last = time;
  };

  return {
    down(id, x, y, time) {
      if (fingers.has(id)) return [];
      fingers.set(id, { start: [x, y], at: [x, y] });
      if (fingers.size === 1) {
        lead = id;
        downAt = time;
        startMotion(time);
        if (tapped !== null && time - tapped <= DOUBLE_TAP_MS) {
          tapped = null;
          mode = "drag";
          return [button("left", true, time)];
        }
        tapped = null;
        mode = "pending";
        return [];
      }
      if (fingers.size === 2 && ["pending", "moving", "spent"].includes(mode)) {
        mode = "scroll";
        scroll = 0;
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
        const steps = Math.trunc(scroll / SCROLL_STEP);
        if (steps === 0) return [];
        scroll -= steps * SCROLL_STEP;
        return [["wheel", -steps]];
      }
      if (id !== lead) return [];
      if (mode === "pending" || mode === "spent") {
        const [sx, sy] = [x - finger.start[0], y - finger.start[1]];
        if (Math.hypot(sx, sy) <= TAP_SLOP) return [];
        // Past the slop: a move, from where it began.
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
      if (mode === "pending" && time - downAt < LONG_PRESS_MS) {
        out.push(...click("left", time));
        tapped = time;
      } else if (mode === "drag" && id === lead) {
        out.push(button("left", false, time));
      }
      mode = fingers.size === 0 ? "idle" : "done";
      return out;
    },

    /** The platform took the touch: released, without acting. */
    cancel(id, time) {
      if (!fingers.has(id)) return [];
      fingers.delete(id);
      const out = mode === "drag" && id === lead ? [button("left", false, time)] : [];
      mode = fingers.size === 0 ? "idle" : "done";
      return out;
    },

    /** The long press, once `deadline()` has passed. */
    tick(time) {
      if (mode !== "pending" || time - downAt < LONG_PRESS_MS) return [];
      mode = "spent";
      return click("right", time);
    },

    /** When `tick` may act next, or null. */
    deadline() {
      return mode === "pending" ? downAt + LONG_PRESS_MS : null;
    },

    /** Forgets every touch (they go on unheard); a held drag is released. */
    reset(time) {
      const out = mode === "drag" ? [button("left", false, time)] : [];
      fingers.clear();
      mode = "idle";
      tapped = null;
      return out;
    },
  };
}
