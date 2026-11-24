// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// On-screen touch controls in a level: our design (the original game has no
// touch support), mapped by the engine onto the game's actions
// (crates/volya-engine/src/controls.rs). Page elements over the canvas, shown
// on touch devices while the level's controls are live:
//
// - the left half is a move stick: it centres where a finger lands (resting
//   at the bottom left) and sends its deflection, −1 … +1 per axis;
// - the right half looks: drags send their motion;
// - Jump, Crouch and Sprint at the bottom right (Crouch and Sprint are
//   toggles, shown pressed while on), Menu at the top left (as Escape).
//
// Touches on the controls go to them alone, not to input.js. Positions are
// in the controls' own frame, which is turned with the canvas on upright
// phones (style.css).

/** Stick radius, CSS pixels: a full deflection (matches style.css). */
const RADIUS = 56;

/** The position of `event` in `box`'s own frame, turned like the canvas. */
function local(box, event) {
  const rect = box.getBoundingClientRect();
  if (rect.width < rect.height !== box.clientWidth < box.clientHeight) {
    return [event.clientY - rect.top, rect.right - event.clientX];
  }
  return [event.clientX - rect.left, event.clientY - rect.top];
}

function element(tag, className, text) {
  const node = document.createElement(tag);
  node.className = className;
  if (text) node.textContent = text;
  return node;
}

/**
 * The controls; `send(message)` posts one input message to the engine.
 * `update({ level, crouch, sprint })` follows the engine's state. `touch`
 * says the device has touch without waiting for one (UX audit page).
 */
export function createTouchControls(send, { touch = false } = {}) {
  const root = element("div", "touch-controls");
  root.id = "touch-controls";
  root.hidden = true;
  const move = element("div", "touch-move");
  const ring = element("div", "touch-stick");
  const knob = element("div", "touch-knob");
  ring.append(knob);
  move.append(ring);
  const look = element("div", "touch-look");
  const button = (name, label) => {
    const b = element("button", `touch-button ${name}`, label);
    b.type = "button";
    b.dataset.button = name;
    return b;
  };
  const menu = button("menu", "Menu");
  const jump = button("jump", "Jump");
  const crouch = button("crouch", "Crouch");
  const sprint = button("sprint", "Sprint");
  crouch.setAttribute("aria-pressed", "false");
  sprint.setAttribute("aria-pressed", "false");
  const actions = element("div", "touch-actions");
  actions.append(sprint, crouch, jump);
  root.append(move, look, menu, actions);

  // Every pointer event on the controls stays with them.
  const own = (target, handlers) => {
    for (const [type, handler] of Object.entries(handlers)) {
      target.addEventListener(
        type,
        (event) => {
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

  // The move stick.
  let stick = null; // { id, origin }
  const showStick = (origin, [x, y]) => {
    ring.style.left = `${origin[0]}px`;
    ring.style.top = `${origin[1]}px`;
    ring.classList.add("held");
    knob.style.transform = `translate(${x * RADIUS}px, ${y * RADIUS}px)`;
  };
  const restStick = () => {
    ring.style.left = "";
    ring.style.top = "";
    ring.classList.remove("held");
    knob.style.transform = "";
  };
  const deflection = (event) => {
    const [px, py] = local(root, event);
    let [x, y] = [(px - stick.origin[0]) / RADIUS, (py - stick.origin[1]) / RADIUS];
    const length = Math.hypot(x, y);
    if (length > 1) [x, y] = [x / length, y / length];
    return [x, y];
  };
  const endStick = (event) => {
    if (stick?.id !== event.pointerId) return;
    stick = null;
    restStick();
    send(["stick", 0, 0]);
  };
  own(move, {
    pointerdown: (event) => {
      if (stick) return;
      capture(event);
      stick = { id: event.pointerId, origin: local(root, event) };
      showStick(stick.origin, [0, 0]);
      send(["stick", 0, 0]);
    },
    pointermove: (event) => {
      if (stick?.id !== event.pointerId) return;
      const value = deflection(event);
      showStick(stick.origin, value);
      send(["stick", ...value]);
    },
    pointerup: endStick,
    pointercancel: endStick,
  });

  // The look area: each finger's motion.
  const looking = new Map(); // pointerId → last position
  const endLook = (event) => looking.delete(event.pointerId);
  own(look, {
    pointerdown: (event) => {
      capture(event);
      looking.set(event.pointerId, local(root, event));
    },
    pointermove: (event) => {
      const last = looking.get(event.pointerId);
      if (!last) return;
      const at = local(root, event);
      looking.set(event.pointerId, at);
      send(["look", at[0] - last[0], at[1] - last[1]]);
    },
    pointerup: endLook,
    pointercancel: endLook,
  });

  // Buttons: held from down to up.
  for (const b of [menu, jump, crouch, sprint]) {
    const held = new Set();
    const release = (event) => {
      if (!held.delete(event.pointerId) || held.size > 0) return;
      b.classList.remove("down");
      send(["tbutton", b.dataset.button, false]);
    };
    own(b, {
      pointerdown: (event) => {
        capture(event);
        held.add(event.pointerId);
        if (held.size > 1) return;
        b.classList.add("down");
        send(["tbutton", b.dataset.button, true]);
      },
      pointerup: release,
      pointercancel: release,
    });
  }

  // Shown on touch devices: a coarse pointer, or once a touch is seen.
  let touched = touch || matchMedia("(pointer: coarse)").matches;
  let live = false;
  const render = () => {
    const shown = live && touched;
    if (root.hidden === !shown) return;
    root.hidden = !shown;
    if (!shown) {
      // Nothing stays held when the controls go away.
      if (stick) send(["stick", 0, 0]);
      stick = null;
      restStick();
      looking.clear();
    }
  };
  addEventListener(
    "pointerdown",
    (event) => {
      if (event.pointerType !== "touch" || touched) return;
      touched = true;
      render();
    },
    { capture: true },
  );

  return {
    element: root,
    update({ level, crouch: crouched, sprint: sprinting }) {
      live = level;
      crouch.setAttribute("aria-pressed", String(Boolean(crouched)));
      sprint.setAttribute("aria-pressed", String(Boolean(sprinting)));
      render();
    },
    /** The stick held at `origin` (CSS pixels) with `value` (UX audit page). */
    showStick,
  };
}
