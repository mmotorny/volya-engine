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
//   shows above it (closer, or turned aside, where a thumb lands too high
//   for it): let go there, the sprint locks hands-free until the stick is
//   touched again (the engine decides, SprintLock);
// - the right half looks: a drag sends where the finger is, and the engine
//   keeps the scene under it and lets a flick coast on (TouchLookDrag);
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
import { approximateArt, gameArt, paintDisc, paintGlyph, paintPicture, vectorGlyph } from "./touch-art.js";

/** Stick radius, CSS pixels: a full deflection (matches style.css). */
const RADIUS = 56;
/** Sizes in CSS pixels (match style.css): the buttons, the knob, the lock
 * target, and the segmented ring around the stick. */
const SIZE = { menu: 44, console: 44, inventory: 44, pda: 44, torch: 44, nightVision: 44, jump: 80, crouch: 64 };
const KNOB = 48;
const TARGET = 44;
const SEGMENTS = 2 * RADIUS + 6;
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
/** Sprint ahead: a double chevron up, between the rim and the lock target. */
const SPRINT = ["M7 13l5-5 5 5", "M7 19l5-5 5 5"];

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

/** Whether a stick at `[x, y]` (stick radii) is past the rim toward the
 * lock target at `centre`, given whether it was (the engine's rule and
 * hysteresis). */
function pastRim([x, y], centre, was) {
  const length = Math.hypot(x, y);
  const reach = Math.hypot(...centre);
  const ahead = (x * centre[0] + y * centre[1]) / reach >= SPRINT_CONE * length;
  return ahead && length >= (was ? SPRINT_RIM_EXIT : SPRINT_RIM);
}

const onTarget = ([x, y], centre) => Math.hypot(x - centre[0], y - centre[1]) <= LOCK_RADIUS;

/**
 * The controls; `send(message)` posts one input message to the engine, and
 * `call(op, args)` runs an engine op (for the game's textures; without it
 * the approximations stay). `update({ level, crouch, low, sprint, lock,
 * torch, nightVision })` follows the engine's state (`torch` and
 * `nightVision` on or off, or null to hide their buttons). `touch` says the device has touch without
 * waiting for one, and `fade: false` keeps the controls from fading when
 * idle (UX audit page). `onConsole()` runs where a tap on Console may
 * raise the device's keyboard (raiseOnTap, console-touch.js).
 */
export function createTouchControls(send, { call = null, touch = false, fade = true, onConsole = () => {} } = {}) {
  const root = element("div", "touch-controls");
  root.id = "touch-controls";
  root.hidden = true;
  const move = element("div", "touch-move");
  move.setAttribute("role", "group");
  move.setAttribute("aria-label", "Move");
  const ring = element("div", "touch-stick");
  const target = element("div", "touch-lock");
  const knob = element("div", "touch-knob");
  const hint = drawing("touch-sprint-hint", 24, SPRINT);
  const base = artCanvas("touch-disc");
  const rim = element("div", "touch-rim");
  const segments = artCanvas("touch-segments");
  const knobDisc = artCanvas("touch-disc");
  const spot = artCanvas("touch-spot");
  const figure = artCanvas("touch-figure");
  const figureLit = artCanvas("touch-figure lit");
  knob.append(knobDisc);
  target.append(spot, figure, figureLit);
  ring.append(base, rim, segments, target, knob, hint);
  move.append(ring);
  const look = element("div", "touch-look");
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
  root.append(move, look, menu, consoleButton, inventory, pda, torch, nightVision, actions);

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
  let stick = null; // { id, origin, centre, value, past }
  let sprinting = false;
  let letGo = null; // { origin, centre } of the latest let-go, for the lock
  /**
   * The lock target's centre (stick radii) for a stick centred at
   * `origin` (our choice): straight ahead at the default reach, or closer,
   * down to all of it past the rim, where its drop area wouldn't fit
   * inside the safe area (whose top edge is Menu's top, its right and
   * bottom edges the action buttons'); else turned clockwise, toward the
   * screen's middle, in 15° steps up to 135°. The engine is told, so it
   * accepts a let-go where the target is drawn.
   */
  const aim = ([x0, y0]) => {
    const hit = LOCK_RADIUS * RADIUS;
    const top = menu.offsetTop;
    const right = actions.offsetLeft + actions.offsetWidth;
    const bottom = actions.offsetTop + actions.offsetHeight;
    const fits = ([x, y]) => y - hit >= top && y + hit <= bottom && x + hit <= right;
    for (let degrees = 0; degrees <= 135; degrees += 15) {
      const turn = (degrees * Math.PI) / 180;
      const toward = [Math.sin(turn), -Math.cos(turn)];
      for (let reach = LOCK_REACH; reach >= LOCK_MIN_REACH - 1e-6; reach -= 0.05) {
        const centre = [toward[0] * reach, toward[1] * reach];
        if (fits([x0 + centre[0] * RADIUS, y0 + centre[1] * RADIUS])) return centre;
      }
    }
    const turn = (135 * Math.PI) / 180;
    return [Math.sin(turn) * LOCK_MIN_REACH, -Math.cos(turn) * LOCK_MIN_REACH];
  };
  /** Draws the target and the sprint chevrons toward `centre`. */
  const drawTarget = (centre) => {
    // In the ring's frame: its centre 56 px into the 112-pixel box; the
    // target is 44 px, the chevrons 22 px.
    const reach = Math.hypot(...centre);
    const toward = [centre[0] / reach, centre[1] / reach];
    const at = (r) => [56 + toward[0] * r, 56 + toward[1] * r];
    const [tx, ty] = at(reach * RADIUS);
    target.style.left = `${tx - 22}px`;
    target.style.top = `${ty - 22}px`;
    // Between the rim and the target's near edge.
    const [hx, hy] = at((RADIUS + reach * RADIUS - 22) / 2);
    hint.style.left = `${hx - 11}px`;
    hint.style.top = `${hy - 11}px`;
    hint.style.transform = `rotate(${Math.atan2(toward[0], -toward[1])}rad)`;
  };
  const place = (origin) => {
    ring.style.left = origin ? `${origin[0]}px` : "";
    ring.style.top = origin ? `${origin[1]}px` : "";
  };
  /** The knob at `[x, y]` stick radii: on the rim at most, except past it
   * toward the target, where it follows the thumb up to the target. */
  const knobAt = ([x, y], most) => {
    const length = Math.hypot(x, y);
    if (length > most) [x, y] = [(x / length) * most, (y / length) * most];
    knob.style.transform = `translate(${x * RADIUS}px, ${y * RADIUS}px)`;
  };
  const showHeld = (origin, centre, value, past) => {
    drawTarget(centre);
    place(origin);
    ring.classList.add("held");
    ring.classList.toggle("past", past);
    ring.classList.toggle("on-target", past && onTarget(value, centre));
    ring.classList.remove("locked");
    knobAt(value, past ? Math.hypot(...centre) : 1);
  };
  const drawLocked = ({ origin, centre }) => {
    drawTarget(centre);
    place(origin);
    ring.classList.remove("held", "past", "on-target");
    ring.classList.add("locked");
    knobAt(centre, Infinity);
  };
  const restStick = () => {
    place(null);
    ring.classList.remove("held", "past", "on-target", "locked");
    knob.style.transform = "";
  };
  const reachOf = (event) => {
    const [px, py] = local(root, event);
    const clamp = (v) => Math.max(-REACH, Math.min(REACH, v));
    return [clamp((px - stick.origin[0]) / RADIUS), clamp((py - stick.origin[1]) / RADIUS)];
  };
  /** The stick's touch ends: let go where it is, or, `cancelled` (by the
   * system or the controls hiding), without acting there, so no lock. */
  const endStick = (cancelled, value) => {
    const { origin, centre, past } = stick;
    stick = null;
    if (cancelled) {
      send(["stick_cancel"]);
      letGo = null;
      restStick();
      return;
    }
    // Where it was let go, then the let-go: the engine locks on the target.
    send(["stick", ...value]);
    send(["stick", 0, 0]);
    letGo = { origin, centre };
    // Drawn at once while the page knows of the sprint; otherwise when
    // the engine says it locked (update).
    if (pastRim(value, centre, past) && onTarget(value, centre) && sprinting) drawLocked(letGo);
    else restStick();
  };
  own(move, {
    pointerdown: (event) => {
      if (stick) return;
      capture(event);
      const origin = local(root, event);
      const centre = aim(origin);
      stick = { id: event.pointerId, origin, centre, value: [0, 0], past: false };
      showHeld(origin, centre, [0, 0], false);
      send(["stick_lock", ...centre]);
      send(["stick", 0, 0]);
    },
    pointermove: (event) => {
      if (stick?.id !== event.pointerId) return;
      const value = reachOf(event);
      stick.value = value;
      stick.past = pastRim(value, stick.centre, stick.past);
      showHeld(stick.origin, stick.centre, value, stick.past);
      send(["stick", ...value]);
    },
    pointerup: (event) => {
      if (stick?.id === event.pointerId) endStick(false, reachOf(event));
    },
    pointercancel: (event) => {
      if (stick?.id === event.pointerId) endStick(true);
    },
  });

  // Looking: each finger on the look area or dragged from a button, where
  // it is in the picture (from its centre, in half its height) at its
  // event's time; the engine keeps the scene under it and flings on
  // release.
  const looking = new Set(); // pointerIds
  const sendLook = (event, phase, at = local(root, event)) => {
    const half = root.clientHeight / 2;
    const x = (at[0] - root.clientWidth / 2) / half;
    const y = (at[1] - half) / half;
    send(["look", x, y, phase, event.timeStamp, event.pointerId]);
  };
  const startLook = (event, at) => {
    looking.add(event.pointerId);
    sendLook(event, "down", at);
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
  const cancelLooks = () => {
    for (const id of looking) send(["look", 0, 0, "cancel", performance.now(), id]);
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
        if (looking.delete(id)) send(["look", 0, 0, "cancel", performance.now(), id]);
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
        startLook(event, at);
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
    const { teal, tealHi, grey, pale } = art;
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
    paintDisc(base, art, 2 * RADIUS, { backing: 0.3, opacity: 0.75 });
    paintPicture(segments, art.segments, SEGMENTS, teal);
    paintDisc(knobDisc, art, KNOB, { texel: 0.6, backing: 0.55, opacity: 1 });
    paintPicture(spot, art.spot, TARGET);
    paintGlyph(figure, art.sprint, TARGET * 0.55, TARGET * 0.55, teal);
    paintGlyph(figureLit, art.sprint, TARGET * 0.55, TARGET * 0.55, pale);
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
      // a held stick is cancelled, not let go, so it can't lock.
      if (stick) endStick(true);
      letGo = null;
      restStick();
      cancelLooks();
      for (const [b, release] of releases) if (b !== pda || !shown) release();
      consoleButton.classList.remove("down");
      clearTimeout(idleTimer);
      root.classList.remove("idle");
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
      if (event.pointerType !== "touch" || touched || !(live || pdaOpen)) return;
      touched = true;
      render();
    },
    { capture: true },
  );

  return {
    element: root,
    update({ level, crouch: on, low, sprint, lock, torch: torchOn, nightVision: nightOn, pda: overPda }) {
      crouched = Boolean(on);
      live = level;
      pdaOpen = Boolean(overPda);
      if (level) askArt();
      sprinting = Boolean(sprint);
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
      // The lock where the thumb let go, whatever the page knew then.
      if (lock && !stick) drawLocked(letGo ?? { origin: null, centre: [0, -LOCK_REACH] });
      else if (!lock && ring.classList.contains("locked")) restStick();
      render();
    },
    /** The stick held at `origin` (CSS pixels) with `value` (stick radii,
     * or "target" for the lock target's place; UX audit page). */
    showStick(origin, value) {
      const centre = aim(origin);
      const at = value === "target" ? centre : value;
      showHeld(origin, centre, at, pastRim(at, centre, false));
    },
    /** The stick locked after being let go at `origin` (UX audit page). */
    showLocked(origin) {
      letGo = { origin, centre: aim(origin) };
      drawLocked(letGo);
    },
    /** The controls faded as when idle (UX audit page). */
    fadeNow: () => root.classList.add("idle"),
  };
}
