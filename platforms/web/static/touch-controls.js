// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// On-screen touch controls in a level: our design (the original game has no
// touch support), after the controls of mobile shooters (Rainbow Six Mobile,
// Call of Duty Mobile, PUBG Mobile), mapped by the engine onto the game's
// actions (crates/volya-engine/src/controls.rs). Page elements over the
// canvas, shown on touch devices while the level's controls are live:
//
// - the whole picture moves and looks (#772, an experiment): a finger sends
//   where it is, and the engine tells a tap from a drag. A tap is
//   tap-to-move: the character runs to where it meets the level, a double
//   tap sprinting, and a plain ring marks the destination while the trip
//   is under way (TapMove); a drag keeps the scene under the finger and
//   lets a flick coast on (TouchLookDrag);
// - Jump and Crouch (a toggle, shown on, with its own icon in the low
//   crouch) at the bottom right act on their press; a drag begun on either
//   then looks too, and Crouch swiped down is the low crouch. Menu at the
//   top left does what Escape does; Inventory at the top right opens the
//   inventory screen (#110), both clear of the thumbs' zones; Console,
//   right of Menu, opens the console as its key does and raises the
//   device's keyboard for it (#642, console-touch.js);
// - PDA left of Inventory opens the PDA as the key bound to `active_jobs`
//   does (#730). The PDA has no close button of its own (the original
//   closes it by key), so while it is open over the level this button
//   alone shows, at its place, and closes it the same way; touches
//   elsewhere reach the touchpad (touchpad.js);
// - Torch and Night vision (toggles, shown on) in a row left of PDA along
//   the top edge (#363), clear of the thumbs' zones too, give the
//   `torch` and `night_vision` commands. Each shows only while its command
//   would do something (the engine says): Torch while a torch is attached,
//   Night vision while that torch's night vision is on or the worn outfit
//   has it, so neither sits idle for most of the game.
//
// They look like instruments of the game's HUD, drawn from its own UI
// textures (touch-art.js; design A of #105). No text on the controls: names
// are aria-labels. Touches on the controls go to them alone, not to
// input.js. Positions are in the controls' own frame, which is turned with
// the canvas on upright phones (style.css).

import { raiseOnTap } from "./console-touch.js";
import { approximateArt, gameArt, paintDisc, paintGlyph, vectorGlyph } from "./touch-art.js";

/** Sizes in CSS pixels (match style.css): the buttons. */
const SIZE = { menu: 44, console: 44, inventory: 44, pda: 44, torch: 44, nightVision: 44, jump: 80, crouch: 64 };
/** A touch on Jump or Crouch moved this far (CSS pixels) drags (our choice). */
const DRAG = 12;
/** A drag from Crouch within this angle of straight down is a swipe down
 * (our choice: 35°, its cosine). */
const SWIPE_CONE = Math.cos((35 * Math.PI) / 180);
/** The controls fade after this long untouched, milliseconds (our choice). */
const IDLE_MS = 4000;

const SVG = "http://www.w3.org/2000/svg";

/** A decorative line drawing of `paths` (`[d, className]` or `d`) in a
 * `size` × `size` frame. */
function drawing(className, size, paths) {
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
  return svg;
}

/** The gestures a held button takes, as chevrons at its edges (a 100 × 100
 * frame), shown only while it is held (our design: no permanent arrows):
 * dragging sideways looks, Crouch swiped down is the low crouch. */
const EDGES = { left: "M11 42 4 50l7 8", right: "M89 42l7 8-7 8", down: "M42 89l8 7 8-7" };

const edges = (sides) => drawing("touch-edges", 100, sides.map((side) => [EDGES[side], side]));

/** A canvas the art is drawn into. */
function artCanvas(className) {
  const c = document.createElement("canvas");
  c.className = className;
  c.setAttribute("aria-hidden", "true");
  return c;
}

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

/**
 * The controls; `send(message)` posts one input message to the engine, and
 * `call(op, args)` runs an engine op (for the game's textures; without it
 * the approximations stay). `update({ level, crouch, low, torch,
 * nightVision, pda })` follows the engine's state (`torch` and
 * `nightVision` on or off, or null to hide their buttons), and
 * `trip(at)` the destination's place in the picture (from its centre, in
 * half its height) or null. `touch` says the device has touch without
 * waiting for one, and `fade: false` keeps the controls from fading when
 * idle (UX audit page). `onConsole()` runs where a tap on Console may
 * raise the device's keyboard (raiseOnTap, console-touch.js).
 */
export function createTouchControls(send, { call = null, touch = false, fade = true, onConsole = () => {} } = {}) {
  const root = element("div", "touch-controls");
  root.id = "touch-controls";
  root.hidden = true;
  const look = element("div", "touch-look");
  // The trip's destination: a plain ring, placed by trip().
  const destination = element("div", "touch-destination");
  destination.hidden = true;
  const button = (name, label, sides, glyphs = [""]) => {
    const b = element("button", `touch-button ${name}`);
    b.type = "button";
    b.dataset.button = name;
    b.setAttribute("aria-label", label);
    b.append(artCanvas("touch-disc"));
    for (const variant of glyphs) b.append(artCanvas(`touch-glyph ${variant}`.trim()));
    if (sides) b.append(edges(sides));
    return b;
  };
  const menu = button("menu", "Menu");
  const consoleButton = button("console", "Console");
  const inventory = button("inventory", "Inventory");
  const pda = button("pda", "PDA");
  const jump = button("jump", "Jump", ["left", "right"]);
  // Crouch: the upright crouch icon; brighter when on; the creep icon in
  // the low crouch.
  const crouch = button("crouch", "Crouch", ["left", "right", "down"], ["", "on", "low"]);
  crouch.setAttribute("aria-pressed", "false");
  // Torch and Night vision: their glyphs, brighter when on.
  const torch = button("torch", "Torch", null, ["", "on"]);
  const nightVision = button("night-vision", "Night vision", null, ["", "on"]);
  nightVision.dataset.button = "night_vision";
  for (const b of [torch, nightVision]) {
    b.setAttribute("aria-pressed", "false");
    b.hidden = true;
  }
  const actions = element("div", "touch-actions");
  actions.append(crouch, jump);
  root.append(look, destination, menu, consoleButton, inventory, pda, torch, nightVision, actions);

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

  // Looking and moving: each finger on the look area (a tap or a drag,
  // the engine tells them apart) or dragged from a button, where it is in
  // the picture (from its centre, in half its height) at its event's time,
  // with half the picture's height in CSS pixels.
  const looking = new Set(); // pointerIds
  const sendLook = (event, phase, at = local(root, event)) => {
    const half = root.clientHeight / 2;
    const x = (at[0] - root.clientWidth / 2) / half;
    const y = (at[1] - half) / half;
    send(["look", x, y, phase, event.timeStamp, event.pointerId, half]);
  };
  /** A finger starts: put down on the look area, or (`phase` "drag")
   * dragged from a button past DRAG. */
  const startLook = (event, at, phase = "down") => {
    looking.add(event.pointerId);
    sendLook(event, phase, at);
  };
  const lookTo = (event) => {
    if (looking.has(event.pointerId)) sendLook(event, "move");
  };
  /** A finger stops looking: lifted (it may fling) or `cancelled`. */
  const endLook = (event, cancelled) => {
    if (!looking.delete(event.pointerId)) return;
    sendLook(event, cancelled ? "cancel" : "up");
  };
  /** Every finger stops looking without acting (the controls hiding). */
  const cancel = (id) => send(["look", 0, 0, "cancel", performance.now(), id, root.clientHeight / 2]);
  const cancelLooks = () => {
    for (const id of looking) cancel(id);
    looking.clear();
  };
  own(look, {
    pointerdown: (event) => {
      capture(event);
      startLook(event, local(root, event));
    },
    pointermove: lookTo,
    pointerup: (event) => endLook(event, false),
    pointercancel: (event) => endLook(event, true),
  });

  // Buttons: held from down to up. A touch on Jump or Crouch that moves
  // past DRAG then looks from there (our design, as mobile shooters' fire
  // buttons do), and on Crouch, if that first move is down, is the low
  // crouch too. Crouch while crouched waits (our choice): a tap stands up
  // on release, a drag aside stands up and looks, a swipe down only goes
  // to the low crouch, so no swipe stands up for a moment first; a
  // cancelled touch does nothing.
  let crouched = false;
  const releases = new Map(); // button → its release
  for (const b of [menu, inventory, pda, torch, nightVision, jump, crouch]) {
    const held = new Map(); // pointerId → { start, waiting }
    const drags = b === jump || b === crouch;
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
    releases.set(b, () => {
      for (const id of held.keys()) {
        if (looking.delete(id)) cancel(id);
      }
      held.clear();
      lift();
    });
    const release = (event, cancelled) => {
      const touch = held.get(event.pointerId);
      if (!touch) return;
      if (touch.waiting && !cancelled) press();
      held.delete(event.pointerId);
      endLook(event, cancelled);
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
        startLook(event, at, "drag");
      },
      pointerup: (event) => release(event, false),
      pointercancel: (event) => release(event, true),
    });
  }

  // Console presses the engine's button on its tap's release, and raises
  // the device's keyboard where a tap may (console-touch.js).
  raiseOnTap(consoleButton, () => onConsole());
  own(consoleButton, {
    pointerdown: (event) => {
      capture(event);
      consoleButton.classList.add("down");
    },
    pointerup: () => {
      if (!consoleButton.classList.contains("down")) return;
      consoleButton.classList.remove("down");
      send(["tbutton", "console", true]);
      send(["tbutton", "console", false]);
    },
    pointercancel: () => consoleButton.classList.remove("down"),
  });

  // The art: approximations until the game's textures arrive (asked for
  // once the level's controls are first live), redrawn at a new
  // devicePixelRatio. For tests: `data-art` says which is drawn.
  let art = approximateArt();
  const paint = () => {
    const { teal, tealHi, grey } = art;
    for (const [b, size] of [
      [menu, SIZE.menu],
      [consoleButton, SIZE.console],
      [inventory, SIZE.inventory],
      [pda, SIZE.pda],
      [torch, SIZE.torch],
      [nightVision, SIZE.nightVision],
      [jump, SIZE.jump],
      [crouch, SIZE.crouch],
    ]) {
      paintDisc(b.querySelector(".touch-disc"), art, size);
    }
    const k = devicePixelRatio || 1;
    const menuGlyph = SIZE.menu * 0.5;
    paintGlyph(menu.querySelector(".touch-glyph"), vectorGlyph("menu", Math.round(menuGlyph * k)), menuGlyph, menuGlyph, grey);
    const promptGlyph = SIZE.console * 0.56;
    paintGlyph(consoleButton.querySelector(".touch-glyph"), vectorGlyph("console", Math.round(promptGlyph * k)), promptGlyph, promptGlyph, grey);
    // The game's UI textures have no inventory or backpack icon: our
    // backpack, drawn as the Menu bars are.
    const bagGlyph = SIZE.inventory * 0.56;
    paintGlyph(inventory.querySelector(".touch-glyph"), vectorGlyph("inventory", Math.round(bagGlyph * k)), bagGlyph, bagGlyph, grey);
    // The HUD's PDA icon (`ui_hud_icon_PDA`), twice as wide as high.
    const pdaGlyph = SIZE.pda * 0.7;
    paintGlyph(pda.querySelector(".touch-glyph"), art.pda, pdaGlyph, pdaGlyph / 2, grey);
    const torchGlyph = SIZE.torch * 0.58;
    const [torchNormal, torchOn] = torch.querySelectorAll(".touch-glyph");
    paintGlyph(torchNormal, vectorGlyph("torch", Math.round(torchGlyph * k)), torchGlyph, torchGlyph, grey);
    paintGlyph(torchOn, vectorGlyph("torch", Math.round(torchGlyph * k)), torchGlyph, torchGlyph, tealHi);
    // The icon is twice as wide as high.
    const nightGlyph = SIZE.nightVision * 0.74;
    const [nightNormal, nightOn] = nightVision.querySelectorAll(".touch-glyph");
    paintGlyph(nightNormal, art.torchIcon, nightGlyph, nightGlyph / 2, grey);
    paintGlyph(nightOn, art.torchIcon, nightGlyph, nightGlyph / 2, tealHi);
    const jumpGlyph = SIZE.jump * 0.56;
    paintGlyph(jump.querySelector(".touch-glyph"), vectorGlyph("jump", Math.round(jumpGlyph * k)), jumpGlyph, jumpGlyph, teal);
    const crouchGlyph = SIZE.crouch * 0.62;
    const [normal, on, low] = crouch.querySelectorAll(".touch-glyph");
    paintGlyph(normal, art.crouch, crouchGlyph, crouchGlyph, teal);
    paintGlyph(on, art.crouch, crouchGlyph, crouchGlyph, tealHi);
    paintGlyph(low, art.creep, crouchGlyph, crouchGlyph, tealHi);
    for (const [name, color] of Object.entries({ teal, "teal-hi": tealHi, grey })) {
      root.style.setProperty(`--hud-${name}`, color.join(", "));
    }
    root.dataset.art = art.game ? "game" : "approximate";
  };
  paint();
  (function watchPixelRatio() {
    matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener(
      "change",
      () => {
        paint();
        watchPixelRatio();
      },
      { once: true },
    );
  })();
  let asked = false;
  const askArt = () => {
    if (asked || !call) return;
    asked = true;
    gameArt(call).then(
      (game) => {
        art = game;
        paint();
      },
      (error) => {
        // The approximations stay; asked again when the controls are next
        // live.
        asked = false;
        console.warn(`touch controls: the game's textures: ${error.message}`);
      },
    );
  };

  // Shown on touch devices: a coarse pointer, or once a touch is seen in
  // the level.
  let touched = touch || matchMedia("(pointer: coarse)").matches;
  let live = false;
  let pdaOpen = false;
  const render = () => {
    // Over the open PDA, its button alone (style.css).
    const overPda = pdaOpen && !live;
    const shown = (live || overPda) && touched;
    const mode = root.classList.contains("pda-open");
    if (root.hidden === !shown && mode === overPda) return;
    root.hidden = !shown;
    root.classList.toggle("pda-open", overPda);
    if (shown && !overPda) {
      wake();
    } else {
      // Nothing stays held when the controls go away, and nothing acts:
      // a finger down is cancelled, not lifted, so it neither taps nor
      // flings.
      cancelLooks();
      for (const [b, release] of releases) if (b !== pda || !shown) release();
      consoleButton.classList.remove("down");
      clearTimeout(idleTimer);
      root.classList.remove("idle");
    }
  };
  addEventListener(
    "pointerdown",
    (event) => {
      if (event.pointerType !== "touch" || touched || !(live || pdaOpen)) return;
      touched = true;
      render();
    },
    { capture: true },
  );

  return {
    element: root,
    update({ level, crouch: on, low, torch: torchOn, nightVision: nightOn, pda: overPda }) {
      crouched = Boolean(on);
      live = level;
      pdaOpen = Boolean(overPda);
      if (level) askArt();
      // For tests: whether the engine's message says the controls are live.
      root.dataset.live = String(Boolean(level));
      crouch.setAttribute("aria-pressed", String(crouched));
      crouch.classList.toggle("low", crouched && Boolean(low));
      crouch.setAttribute("aria-label", crouched && low ? "Crouch, low" : "Crouch");
      // Torch and Night vision: hidden (null) while their command would do
      // nothing; a press on one going away acts no further.
      for (const [b, value] of [
        [torch, torchOn],
        [nightVision, nightOn],
      ]) {
        const hidden = value === null || value === undefined;
        if (hidden && !b.hidden) releases.get(b)();
        b.hidden = hidden;
        b.setAttribute("aria-pressed", String(Boolean(value)));
      }
      render();
    },
    /** The trip's destination at `at` in the picture (from its centre, in
     * half its height, +y down), or none (null): the marker there, hidden
     * off the picture. */
    trip(at) {
      const [width, height] = [root.clientWidth, root.clientHeight];
      const [x, y] = at ? [width / 2 + (at[0] * height) / 2, height / 2 + (at[1] * height) / 2] : [-1, -1];
      destination.hidden = !(x >= 0 && x <= width && y >= 0 && y <= height);
      destination.style.transform = `translate(${x}px, ${y}px)`;
    },
    /** The controls faded as when idle (UX audit page). */
    fadeNow: () => root.classList.add("idle"),
  };
}
