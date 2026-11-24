// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// On-screen touch controls in a level: our design (the original game has no
// touch support), after the controls of mobile shooters (Rainbow Six Mobile,
// Call of Duty Mobile, PUBG Mobile), mapped by the engine onto the game's
// actions (crates/volya-engine/src/controls.rs). Page elements over the
// canvas, shown on touch devices while the level's controls are live:
//
// - the left half is a move stick: it appears where a thumb lands (a faint
//   hint rests at the bottom left) and sends its reach in stick radii, past
//   the rim too. Pushed forward past the rim it sprints, and a lock target
//   shows above it: let go there, the sprint locks hands-free until the
//   stick is touched again (the engine decides, SprintLock);
// - the right half looks: drags send their motion;
// - Jump and Crouch (a toggle, shown on, and lower in the low crouch) at
//   the bottom right act on their press; a drag begun on either then looks
//   too, and Crouch swiped down is the low crouch. Menu at the top left
//   does what Escape does.
//
// No text on the controls: glyphs are ours, names are aria-labels. Touches
// on the controls go to them alone, not to input.js. Positions are in the
// controls' own frame, which is turned with the canvas on upright phones
// (style.css).

/** Stick radius, CSS pixels: a full deflection (matches style.css). */
const RADIUS = 56;
// The sprint lock's geometry in stick radii, as the engine's (controls.rs:
// SPRINT_RIM, SPRINT_RIM_EXIT, SPRINT_CONE, LOCK_REACH, LOCK_RADIUS,
// LOCK_MIN_REACH).
const SPRINT_RIM = 1.3;
const SPRINT_RIM_EXIT = 1.15;
const SPRINT_CONE = Math.SQRT1_2;
const LOCK_REACH = 2.1;
const LOCK_RADIUS = 0.55;
const LOCK_MIN_REACH = SPRINT_RIM + LOCK_RADIUS;
/** The reach the page reports at most, per axis (STICK_REACH in the engine). */
const REACH = 3;
/** A touch on Jump or Crouch moved this far (CSS pixels) drags (our choice). */
const DRAG = 12;
/** A drag from Crouch within this angle of straight down is a swipe down
 * (our choice: 35°, its cosine). */
const SWIPE_CONE = Math.cos((35 * Math.PI) / 180);
/** The controls fade after this long untouched, milliseconds (our choice). */
const IDLE_MS = 4000;

const SVG = "http://www.w3.org/2000/svg";

/** A decorative line drawing of `paths` (`[d, className]` or `d`) and
 * `circles` in a `size` × `size` frame. */
function drawing(className, size, paths, circles = []) {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", `0 0 ${size} ${size}`);
  svg.setAttribute("class", className);
  svg.setAttribute("aria-hidden", "true");
  for (const path of paths) {
    const [d, name] = Array.isArray(path) ? path : [path];
    const p = document.createElementNS(SVG, "path");
    p.setAttribute("d", d);
    if (name) p.setAttribute("class", name);
    svg.append(p);
  }
  for (const [cx, cy, r] of circles) {
    const c = document.createElementNS(SVG, "circle");
    c.setAttribute("cx", cx);
    c.setAttribute("cy", cy);
    c.setAttribute("r", r);
    svg.append(c);
  }
  return svg;
}

// Our glyphs (24 × 24, drawn with round 2-unit strokes in style.css).
/** Jump: an arrow up off a ground line. */
const JUMP = [["M12 17V4", "M6.5 9.5 12 4l5.5 5.5", "M5 20.5h14"]];
/** Crouch: a figure on one knee. */
const CROUCH = [["M11 8.5 10 14", "M10.8 10.5l3.7 2.3", "M10 14l5 .5V20", "M10 14l-1.5 6H5"], [[11.5, 5, 2]]];
/** Low crouch: the figure lower and leaning forward. */
const LOW_CROUCH = [["M13 11.5 8.5 15", "M12 12.8l3.2 2.4", "M8.5 15l4.5 1.5V20", "M8.5 15 7 20H3.5"], [[15, 9.5, 2]]];
/** Menu: pause bars. */
const MENU = [["M9 6v12", "M15 6v12"]];
/** The gesture chevrons at a button's edges (a 100 × 100 frame): dragging
 * sideways looks, Crouch swiped down is the low crouch. */
const EDGES = { left: "M11 42 4 50l7 8", right: "M89 42l7 8-7 8", down: "M42 89l8 7 8-7" };
/** The move stick's rim chevrons (its 112 × 112 frame). */
const RIM = ["M50 13l6-6 6 6", "M50 99l6 6 6-6", "M13 50l-6 6 6 6", "M99 50l6 6-6 6"];
/** Sprint ahead: a double chevron up, between the rim and the lock target. */
const SPRINT = ["M7 13l5-5 5 5", "M7 19l5-5 5 5"];

const glyph = (className, paths, circles) => drawing(className, 24, paths, circles);
const edges = (sides) => drawing("touch-edges", 100, sides.map((side) => [EDGES[side], side]));

/** The position of `event` in `box`'s own frame, turned like the canvas. */
function local(box, event) {
  const rect = box.getBoundingClientRect();
  if (rect.width < rect.height !== box.clientWidth < box.clientHeight) {
    return [event.clientY - rect.top, rect.right - event.clientX];
  }
  return [event.clientX - rect.left, event.clientY - rect.top];
}

function element(tag, className) {
  const node = document.createElement(tag);
  node.className = className;
  return node;
}

/** Whether a stick at `[x, y]` (stick radii) is past the rim, given whether
 * it was (the engine's hysteresis). */
function pastRim([x, y], was) {
  const length = Math.hypot(x, y);
  return -y >= SPRINT_CONE * length && length >= (was ? SPRINT_RIM_EXIT : SPRINT_RIM);
}

const onTarget = ([x, y], reach) => Math.hypot(x, y + reach) <= LOCK_RADIUS;

/**
 * The controls; `send(message)` posts one input message to the engine.
 * `update({ level, crouch, low, sprint, lock })` follows the engine's
 * state. `touch` says the device has touch without waiting for one, and
 * `fade: false` keeps the controls from fading when idle (UX audit page).
 */
export function createTouchControls(send, { touch = false, fade = true } = {}) {
  const root = element("div", "touch-controls");
  root.id = "touch-controls";
  root.hidden = true;
  const move = element("div", "touch-move");
  move.setAttribute("role", "group");
  move.setAttribute("aria-label", "Move");
  const ring = element("div", "touch-stick");
  const target = element("div", "touch-lock");
  const knob = element("div", "touch-knob");
  const hint = glyph("touch-sprint-hint", SPRINT);
  ring.append(drawing("touch-rim", 112, RIM), hint, target, knob);
  move.append(ring);
  const look = element("div", "touch-look");
  const button = (name, label, art, sides) => {
    const b = element("button", `touch-button ${name}`);
    b.type = "button";
    b.dataset.button = name;
    b.setAttribute("aria-label", label);
    if (sides) b.append(edges(sides));
    b.append(glyph("touch-glyph", ...art));
    return b;
  };
  const menu = button("menu", "Menu", MENU);
  const jump = button("jump", "Jump", JUMP, ["left", "right"]);
  const crouch = button("crouch", "Crouch", CROUCH, ["left", "right", "down"]);
  crouch.append(glyph("touch-glyph low", ...LOW_CROUCH));
  crouch.setAttribute("aria-pressed", "false");
  const actions = element("div", "touch-actions");
  actions.append(crouch, jump);
  root.append(move, look, menu, actions);

  // Every touch or pen contact on the controls stays with them (a pen
  // can't use pointer lock); a mouse passes through to input.js (pointer
  // lock, firing) on a device with both.
  const own = (target, handlers) => {
    for (const [type, handler] of Object.entries(handlers)) {
      target.addEventListener(
        type,
        (event) => {
          if (event.pointerType === "mouse") return;
          event.stopPropagation();
          event.preventDefault();
          handler(event);
        },
        { passive: false },
      );
    }
  };
  // Moves and the lift reach the control the finger began on (touch has
  // this implicitly; a pointer that is no longer down can't be captured).
  const capture = (event) => {
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Not an active pointer.
    }
  };
  root.addEventListener("contextmenu", (event) => event.preventDefault());

  // Idle: the controls fade after a while untouched (our choice), and come
  // back with any touch on them.
  let idleTimer = 0;
  const wake = () => {
    root.classList.remove("idle");
    clearTimeout(idleTimer);
    if (fade) idleTimer = setTimeout(() => root.classList.add("idle"), IDLE_MS);
  };
  root.addEventListener("pointerdown", wake, { capture: true });

  // The move stick.
  let stick = null; // { id, origin, value, past }
  let sprinting = false;
  let lockOrigin = null; // where the stick was let go into the lock
  let lockReach = LOCK_REACH;
  /** The lock target for a stick centred at `origin` (our choice): the
   * default reach, or lower where its whole drop area wouldn't fit below
   * the safe area's top edge (where Menu's top is), but past the rim. The
   * engine is told, so it accepts a let-go where the target is drawn. */
  const aim = (origin) => {
    const room = (origin[1] - menu.offsetTop) / RADIUS - LOCK_RADIUS;
    lockReach = Math.min(LOCK_REACH, Math.max(LOCK_MIN_REACH, room));
    // Centres in the ring's frame: 56 px down its 112-pixel box, inside a
    // 2-pixel border; the target is 44 px, the chevrons 22 px.
    const centre = 56 - lockReach * RADIUS;
    target.style.top = `${centre - 24}px`;
    hint.style.top = `${(centre + 22) / 2 - 13}px`;
  };
  const place = (origin) => {
    ring.style.left = origin ? `${origin[0]}px` : "";
    ring.style.top = origin ? `${origin[1]}px` : "";
  };
  /** The knob at `[x, y]` stick radii: on the rim at most, except forward
   * past it, where it follows the thumb up to the lock target. */
  const knobAt = ([x, y], past) => {
    const length = Math.hypot(x, y);
    const most = past ? lockReach : 1;
    if (length > most) [x, y] = [(x / length) * most, (y / length) * most];
    knob.style.transform = `translate(${x * RADIUS}px, ${y * RADIUS}px)`;
  };
  const showStick = (origin, value, past = pastRim(value, false)) => {
    if (!stick) aim(origin);
    place(origin);
    ring.classList.add("held");
    ring.classList.toggle("past", past);
    ring.classList.toggle("on-target", past && onTarget(value, lockReach));
    ring.classList.remove("locked");
    knobAt(value, past);
  };
  const showLocked = (origin) => {
    if (origin !== lockOrigin && origin) aim(origin);
    lockOrigin = origin;
    place(origin);
    ring.classList.remove("held", "past", "on-target");
    ring.classList.add("locked");
    knobAt([0, -lockReach], true);
  };
  const restStick = () => {
    lockOrigin = null;
    place(null);
    ring.classList.remove("held", "past", "on-target", "locked");
    knob.style.transform = "";
  };
  const reach = (event) => {
    const [px, py] = local(root, event);
    const clamp = (v) => Math.max(-REACH, Math.min(REACH, v));
    return [clamp((px - stick.origin[0]) / RADIUS), clamp((py - stick.origin[1]) / RADIUS)];
  };
  const endStick = (event) => {
    if (stick?.id !== event.pointerId) return;
    const value = reach(event);
    const past = pastRim(value, stick.past);
    // Where it was let go, then the let-go: the engine locks on the target.
    send(["stick", ...value]);
    send(["stick", 0, 0]);
    const origin = stick.origin;
    stick = null;
    if (past && onTarget(value, lockReach) && sprinting) showLocked(origin);
    else restStick();
  };
  own(move, {
    pointerdown: (event) => {
      if (stick) return;
      capture(event);
      const origin = local(root, event);
      aim(origin);
      stick = { id: event.pointerId, origin, value: [0, 0], past: false };
      showStick(origin, [0, 0]);
      send(["stick_lock", lockReach]);
      send(["stick", 0, 0]);
    },
    pointermove: (event) => {
      if (stick?.id !== event.pointerId) return;
      const value = reach(event);
      stick.value = value;
      stick.past = pastRim(value, stick.past);
      showStick(stick.origin, value, stick.past);
      send(["stick", ...value]);
    },
    pointerup: endStick,
    pointercancel: endStick,
  });

  // Looking: each finger's motion, on the look area or dragged from a
  // button.
  const looking = new Map(); // pointerId → last position
  const lookTo = (event) => {
    const last = looking.get(event.pointerId);
    if (!last) return;
    const at = local(root, event);
    looking.set(event.pointerId, at);
    send(["look", at[0] - last[0], at[1] - last[1]]);
  };
  const endLook = (event) => looking.delete(event.pointerId);
  own(look, {
    pointerdown: (event) => {
      capture(event);
      looking.set(event.pointerId, local(root, event));
    },
    pointermove: lookTo,
    pointerup: endLook,
    pointercancel: endLook,
  });

  // Buttons: held from down to up. A touch on Jump or Crouch that moves
  // past DRAG then looks from there (our design, as mobile shooters' fire
  // buttons do), and on Crouch, if that first move is down, is the low
  // crouch too. Crouch while crouched waits (our choice): a tap stands up
  // on release, a drag aside stands up and looks, a swipe down only goes
  // to the low crouch, so no swipe stands up for a moment first.
  let crouched = false;
  const releases = [];
  for (const b of [menu, jump, crouch]) {
    const held = new Map(); // pointerId → { start, waiting }
    const drags = b !== menu;
    let pressed = false; // the press sent, its release not yet
    const press = () => {
      if (pressed) return;
      pressed = true;
      send(["tbutton", b.dataset.button, true]);
    };
    const lift = () => {
      b.classList.remove("down");
      if (!pressed) return;
      pressed = false;
      send(["tbutton", b.dataset.button, false]);
    };
    releases.push(() => {
      for (const id of held.keys()) looking.delete(id);
      held.clear();
      lift();
    });
    const release = (event) => {
      const touch = held.get(event.pointerId);
      if (!touch) return;
      if (touch.waiting) press();
      held.delete(event.pointerId);
      looking.delete(event.pointerId);
      if (held.size === 0) lift();
    };
    own(b, {
      pointerdown: (event) => {
        capture(event);
        const waiting = b === crouch && crouched;
        held.set(event.pointerId, { start: local(root, event), waiting });
        b.classList.add("down");
        if (!waiting) press();
      },
      pointermove: (event) => {
        if (!drags) return;
        if (looking.has(event.pointerId)) return lookTo(event);
        const touch = held.get(event.pointerId);
        if (!touch) return;
        const at = local(root, event);
        const [dx, dy] = [at[0] - touch.start[0], at[1] - touch.start[1]];
        const length = Math.hypot(dx, dy);
        if (length < DRAG) return;
        if (b === crouch && dy >= SWIPE_CONE * length) {
          send(["tbutton", "crouch_low", true]);
          send(["tbutton", "crouch_low", false]);
        } else if (touch.waiting) {
          press();
        }
        touch.waiting = false;
        looking.set(event.pointerId, at);
      },
      pointerup: release,
      pointercancel: release,
    });
  }

  // Shown on touch devices: a coarse pointer, or once a touch is seen in
  // the level.
  let touched = touch || matchMedia("(pointer: coarse)").matches;
  let live = false;
  const render = () => {
    const shown = live && touched;
    if (root.hidden === !shown) return;
    root.hidden = !shown;
    if (shown) {
      wake();
    } else {
      // Nothing stays held when the controls go away.
      if (stick) send(["stick", 0, 0]);
      stick = null;
      restStick();
      looking.clear();
      for (const release of releases) release();
      clearTimeout(idleTimer);
    }
  };
  // input.js releases everything on blur and on losing pointer lock (its
  // listeners come first): a held stick's value is sent again. Our choice:
  // held buttons count as released until pressed again, like keys (sending
  // them again would be a fresh press).
  const resend = () => {
    if (root.hidden) return;
    if (stick?.value) send(["stick", ...stick.value]);
  };
  addEventListener("blur", resend);
  document.addEventListener("pointerlockchange", resend);
  addEventListener(
    "pointerdown",
    (event) => {
      if (event.pointerType !== "touch" || touched || !live) return;
      touched = true;
      render();
    },
    { capture: true },
  );

  return {
    element: root,
    update({ level, crouch: on, low, sprint, lock }) {
      crouched = Boolean(on);
      live = level;
      sprinting = Boolean(sprint);
      // For tests: whether the engine's message says the controls are live.
      root.dataset.live = String(Boolean(level));
      crouch.setAttribute("aria-pressed", String(Boolean(crouched)));
      crouch.classList.toggle("low", Boolean(crouched && low));
      if (lock && !stick) showLocked(lockOrigin);
      else if (!lock && ring.classList.contains("locked")) restStick();
      render();
    },
    /** The stick held at `origin` (CSS pixels) with `value` (stick radii;
     * UX audit page). */
    showStick,
    /** The stick locked after being let go at `origin` (UX audit page). */
    showLocked,
    /** The controls faded as when idle (UX audit page). */
    fadeNow: () => root.classList.add("idle"),
  };
}
