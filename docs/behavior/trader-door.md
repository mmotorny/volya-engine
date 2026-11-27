# The trader's door at the start

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the door of the
trader's bunker on the first level (`trader_door`, `l01_escape`) from a New Game's start: what locks it and
how it is held while locked, what unlocks it and when, how it stops the player's body, and the hint "Дверь
заперта" (the door is locked). It is the part of `physics-scope.md` PH-20 (scripted physics schemes) that
the start of the game needs, with the parts of PH-7 (joints) and of the player's contacts with movable bodies
(`physics-scope.md` 2.1, PH-8) that this door uses; PH-9 (hit impulses on shells) is not needed (section 3.4).

Builds on (not repeated): `docs/behavior/physics-scope.md` (cited as `scope`; terms *body*, *shell*,
*joint*, *step*, *scripts*, *logic*), `docs/formats/bone-physics.md` (cited as `bone`), `docs/formats/level-cform.md`
(cited as `cform`; materials, contact properties, the player's body and its contact rules),
`docs/behavior/physics-world.md` (cited as `world`), `docs/behavior/physics-library.md` (cited as `library`),
`docs/behavior/level-objects.md`, `docs/formats/ui-layout.md`, `docs/formats/colour-animations.md` (cited as
`lanim`), `docs/behavior/hud-scope.md`, `docs/behavior/trader-and-crows-scope.md`, `docs/formats/sav.md`.

Conventions:

- The **door object** is the spawn record `trader_door`; the **door** is its moving leaf, the model bone
  `door`; the **frame** is the bone `link`. "The player's body" is the collision body of `cform` 5.
- Angles are in radians, the library's units are those of `library` (believed: metres, kilograms, seconds,
  newtons, newton-metres).
- "Script" names (`ph_door`, `xr_logic`, `set_max_force_and_velocity`, …) are the shipped scripts' and the
  script interface's own names, which the game data uses.
- Statements are static (read from the data and the shipped binaries; the game was not run) unless marked
  otherwise. Inferred statements are marked **believed**; what could not be established is marked
  **unestablished**.
- Bracketed ids are the checks reproducing a statement: functions of `checks.py` in the dirty workspace's
  `checks/trader-door/` (`python3 run_all.py` runs them all; `export.py` re-creates their evidence).

## 1. The door object

- Record: `spawns\all.spawn`, level `l01_escape`, section `physic_object` (class `O_PHYS_S`), physics type 3
  (a shell from the model's bone data, `scope` 1.2), fixed bones `link`, model `physics\door\door_trader`.
  Its custom data is a `[logic]` section naming the logic file `scripts\esc_trader_door.ltx` and a
  `[collide]` section with `ignore_static` [D1]. It stands 1.7 m from the player's start
  (`level-objects.md`, object 79; `scope` 1.2).
- Model `meshes\physics\door\door_trader.ogf` (`bone` for every field) [D2, D2b]:

| Bone | Parent | Shape | Joint | Other |
|---|---|---|---|---|
| `link` (the frame, root) | — | none (type 0) | rigid | fixed by the record |
| `door` | `link` | box: centre (0.703, 0, −0.013), half sizes (0.690, 1.322, 0.125) on the bone's axes | type 2 with x and z limits 0, y limits −1.9199 … 0: a hinge about the bone's y axis (`bone` 3.2) | mass 100 |
| `lock` | `door` | none (type 0, no-physics flag) | rigid | — |

  All three bones have the material `objects\large_metal_trash` [D2]. So the only collision shape of the door
  object is the door's box (a leaf 1.38 m wide, 2.645 m high, 0.25 m thick in its bone's frame).
- `[collide] ignore_static` makes the door object's shell skip collision with the level mesh: its collision
  pass tests other physics objects (the player's body included, section 3) but not the level [B9, B11]
  (`scope` 1.3 lists `ignore_static` among the keys read; that it sets the bit tested here is **believed**).

## 2. The lock

### 2.1 The logic at the start

The door object is bound by the physics-object script binder (`scope` 1.2). At the object's first update in
which the player exists, the binder starts its logic; from then on every update of the object passes an
"update" to the active scheme [D5]. When the first update happens after a New Game is **believed** to be the
first frame of play (the object's update schedule is not stated by an approved spec).

The logic file `esc_trader_door.ltx` starts in the section `ph_door@locked` of the door scheme `ph_door`
(`scripts\ph_door.script`) [D3]:

- `locked = true`; `closed` is absent and defaults to **true**; `show_tips` defaults to true; `no_force`
  defaults to false [D3, D4, D4b];
- the locked hint id is read from the key `tip_open`, absent, so it is the default `tip_door_locked` [D4];
- `snd_open_start = trader_door_unlock`;
- `on_info = {+esc_trader_can_leave} ph_door@closed %=play_snd(device\door_servomotor)%` (section 2.4).

### 2.2 What starting the locked section does

On start the scheme takes the door's joint (the joint of bone `door`) and its limits on axis 0 through the
script interface (`get_limits`, section 2.3), then, because the section is closed [D4c]:

1. **Close**: the joint's motor on axis 0 is set to maximum force 200 and velocity 3
   (`set_max_force_and_velocity(200, 3, 0)`); a per-step check (a "fast call") is registered; the door is
   marked "to block". **Disputed in review:** the scheme registers the same fast call three times, so three
   entries are added; the first to run at a step where the door is closed acts and ends, and the other two,
   finding the mark cleared, return false and stay registered, acting again whenever a later closing sets the
   mark. No sound plays the first time the scheme starts for the object (and none when the door
   is already closed).
2. **Hint**: since the section is locked and the locked hint id is set, the object's hint becomes
   `tip_door_locked` (`set_tip_text`) [D4c].
3. The object is made usable only through its script (`set_nonscript_usable(false)`) [D4c]. **Disputed in
   review:** D4c shows only that the call is made; what clearing this setting changes for the player's use
   of the object is read from the name and is unestablished.

The **fast call** runs in the physics world's per-step calls until it returns true [B5]. Each step: if the
door is marked "to block" and closed (section 2.3), it sets the motor to maximum force **10000** and velocity
**1** (`set_max_force_and_velocity(10000, 1, 0)`), clears the mark, plays nothing for the first, silent
closing, and ends [D4c].

### 2.3 How the joint and the script's values relate

- `set_max_force_and_velocity(force, velocity, axis)` wakes the object's shell if it rests, then stores both
  values for that axis; for a hinge the library gets them as the hinge's motor: parameter 3 (maximum motor
  force, here a torque) and parameter 2 (motor target velocity) [B1, B1b, B1c, B1d]. A hinge made from the
  bone data gets a motor at creation only when its stored maximum force is above 0 [B1e]; whether the door's
  is above 0 then is **unestablished** (believed 0: no motor until the script sets one).
- `get_axis_angle(n)` returns, for a hinge, the library's hinge angle, whatever `n` (the script passes 90)
  [B2]. The library measures it from the bodies' relative rotation when the hinge's axis was set, at the
  hinge's creation [B2b, B4c]; the door object's shell is made from the model's bind pose at the record's
  transform, so angle 0 is the door as placed (**believed**: the door as placed is closed). **Disputed in
  review:** no check backs that the shell is built from the bind pose (the record names no startup animation;
  the pose a shell is built in is PH-3's); treat it as believed.
- The hinge's stops are the bone's limits: low stop −1.9199, high stop 0 (`bone` 3.2) [B3c, B1e]. So the
  door turns from its placed pose (0) to −1.9199 (about 110°) in one direction only.
- `get_limits(axis)` returns the stored (low, high) when the joint's first element is not fixed and has a
  body; otherwise it returns (−high, −low) [B3b, B3d]. For this door the first element is **believed** to be
  the frame's (the parent bone's), which is fixed (section 2.5), so the script receives low = −0 and
  high = 1.9199.
- The scheme's **closed** test is "hinge angle ≤ the low value received" [D4c]. With the values above it
  holds whenever the angle is at most 0, i.e. at any angle between the stops (except a momentary overshoot
  past the high stop). So the fast call ends at its first step and the 10000 / 1 motor is set one physics
  step after the scheme starts (**believed**, from the values above).

### 2.4 How the door is held while locked

- **Not fixed: limited and driven.** The door body is jointed to the frame by the hinge of 2.3, limited to
  −1.9199 … 0, and the hinge's motor drives it at target velocity +1 rad/s (toward the high stop, angle 0,
  the placed pose) with at most 10000 of torque. A positive motor velocity turning the hinge angle upward is
  the library's (public ODE 0.5.0's) motor convention (**believed**; `library` 5.1 states the motor rows are
  public 0.5.0's). The door is thus pressed against its closed stop; pushing it toward opening is resisted by
  up to 10000 of torque, and pushing it further closed by the stop.
- Before the scheme starts (2.1) the hinge has no motor (**believed**, 2.3): the door is free between its
  stops for that time.
- The **frame** (`link`, the record's fixed bone) is fixed when the shell is built: its element is marked
  fixed and its body is given a mass of 10⁸ (a sphere mass adjusted to that total), gravity off, and zero
  linear and angular velocity, force and torque; forces applied to a fixed element are ignored [B4, B4b].
  **Disputed in review:** the checks show what fixing an element does, not that `link` has an element or body
  of its own: `link` has shape type 0, which adds no body (`bone` 2.1), and `physics-scope` 1.2 and `bone` 8
  state that a shell whose root bone has no used shape is fatal. Which element the fixed bone `link` names,
  and so what is fixed, is unestablished.
  The shell is built and activated (its joints made) before the fixed bones are fixed [B4c, B4d], so the hinge
  joins the door's body to the frame's body (**believed**). Whether anything else keeps the frame's body in
  place over time is **unestablished** here (PH-3, PH-5).
- The door's own mass is 100 (bone `door`; `lock` has no physics and adds no mass, `bone` 5) [D2].
- The bone data's joint spring and damping (1 and 1) and limit factors (1, 1) make the hinge as stiff as the
  world (`bone` 3.3) [D2].

### 2.5 Using the door while locked

The use action on the door object (the player's use key while the object is under the crosshair, believed:
the use input is `player-movement`'s / PS specs') reaches the scheme's use callback [D5]: since the section is
locked, it plays the section's `snd_open_start` sound set `trader_door_unlock`, which is `device\door_servomotor`
[D4d, D4e], at the door's position [D4d]; then it switches section only by an `on_use` condition, which the locked
section does not have. The door does not move and nothing else happens [D4d].

## 3. The door stopping the player

### 3.1 Which shapes meet

- The player's body (`cform` 5.1) keeps its four shapes — bottom sphere, torso cylinder, head sphere and
  outer sphere — in one collision group, and that group is what other physics objects are collided with
  [B8]. Every shape of one group is tested against every shape of the other (the library's group collision,
  public 0.5.0) [B9d].
- The outer sphere is marked not to meet the level mesh (`cform` 5.1: it does not collide with the level),
  but it does meet other objects' shapes; its contacts never become contacts (no force): each one only sets
  the body's per-step "blocked" flag (`cform` 6.3) [B8, B8b, B8c].
- The door object's only shape is the door's box (section 1). So, against the door: the bottom sphere, the
  torso cylinder and the head sphere give contacts with force, and the outer sphere (radius 0.7, centre
  1.225 m above the feet for the standing box, `cform` 7.4) marks the step blocked as soon as it overlaps the
  box.

### 3.2 When the pair is collided

Each physics step the player's body's collision pass takes every other active physics object whose bounding
box overlaps the body's (the physics objects' spatial index), skips a pair that the two objects' collision
classes exclude, and collides the two groups; then the level, unless the body's "ignore static" bit is set;
then the body leaves the active set for the rest of that step's pass, so a pair is collided once per step
[B9]. Whether the pair is collided in the player's pass or the door object's, and so the order of the two
shapes in each contact (which decides the sign of the library's contact normal), is **unestablished**
(believed: whichever of the two objects the world takes first, `world` 4.1). That the collision classes do not
exclude this pair is **believed** (the shipped game stops the player at this door, issue #866).

### 3.3 The contact

For every contact between two objects' shapes [B9b, B9c]:

- library contact mode: friction pyramid in both tangent directions, soft ERP and soft CFM;
- friction `μ = A.friction × B.friction`; softness from the two materials' springs and dampings, bounce
  only if both materials are bouncy, exactly as `cform` 4.2 states for the level. Here: the player's shape
  material is material index 0 `default` at creation (`cform` 5.1; friction, damping, spring 1, not bouncy)
  and the door's is `objects\large_metal_trash` (friction, damping, spring 1, bounce 0, bouncy, dynamic, not
  pass-through, not an actor obstacle) [D12]: μ = 1, the world's softness, no bounce;
- each shape's own contact callback runs (the outer sphere's, 3.1), then each object's;
- a contact still kept is made a contact joint attached to **both** bodies: the door receives the reaction.

The player's object callback hands each contact of its four shapes to the body's contact rules [B10]. They
are `cform` 5.4 and 6.3's rules, with these differences for a contact whose two shapes are both on bodies (a
movable body, such as the door) [B10b, B10c]:

- **Normal-direction softening**: with `e` the contact's ERP so far and `q = 20.1 − 19.1·nᵧ` (`n` the
  library's contact normal as generated for the pair, `cform` 5.4), `f = 1 / ((1 − e)·q + 5·e)`, CFM × `f`,
  ERP = `5·f·e` (the level uses 0.5 where this uses 5). Example, a horizontal normal, all factors 1, step
  0.01 s: ERP 0.12987, CFM 1.082 × 10⁻⁷ (a level wall: ERP 0.014706) [B12].
- **Traction**: the contact's value is 1 (as `cform` 5.4 already states for a movable body).
- An **impact** is computed from the two bodies' velocities and masses and recorded for collision damage
  (PH-12) [B10d].
- Unchanged: the bottom sphere's downward contacts are dropped (unless the step replacement is active); the
  head sphere's contacts are frictionless and softened first as `cform` 5.4; while movement is requested
  every contact's friction is 0, otherwise `μ × T × (4 during step assistance, else 1)`; the step replacement
  of `cform` 6.3 acts on these contacts under its conditions, and the outer sphere's overlap with the door
  sets the "blocked" flag that suppresses it in the next step [B10c, B8c].

So the walking player meets the leaf as a frictionless wall (friction 0 while moving) whose push the motor
and stop of 2.4 resist; how far the leaf yields under the player's movement forces (`player-movement` 6) is
the solver's result and is **unestablished** statically.

### 3.4 Not used here

- PH-9 (hit impulses on shells): hits on the door move it through PH-9; the scheme can also switch section on
  hits of named bones (`hit_on_bone`), which this logic does not set [D4c, D3].
- The approved `cform` 4 – 6 contacts are against the level only; the rules above extend them to the door.

## 4. The unlock

### 4.1 What unlocks it

Only the **info portion** `esc_trader_can_leave` held by the player switches the door: the locked section's
`on_info` is tried at every update of the scheme [D3, D4d]. In the shipped data it is given only by phrase
32111 of the trader's start dialog `escape_trader_start_dialog` [D6, D7]; it appears otherwise only in its
declaration, in the door's logic, in a commented-out line of another door's logic in `all.spawn`, and in
scripts whose `main` functions no configuration file, spawn record or reachable script calls (believed debug
scripts) [D6, D6b]. Nothing gives it on a timer: without the dialog the door stays locked for the whole game.

### 4.2 The dialog

The trader (`escape_trader`, `trader-and-crows-scope`) starts with `escape_trader_start_dialog` [D7b]. Its
logic at a New Game: 5000 ms waiting, then the move to the desk, then the first monologue
(`trader_monolog1`), with talking off in all three; the following section leaves talking at its default, on
[D8, D8b]
(timing of animations and sounds: `trader-and-crows-scope`). The player starts the talk (the use action on
the trader, believed). In the dialog [D7, D7c]:

- first talk during the tutorial (no `tutorial_end`; `tutorial_end` is given only later in the same dialog
  after the wounded-stalker task): phrase 0 → 12 → 3, the trader's question, with two answers:
  - 32: marks the choice made → 321 → 3211 → 32111, which gives `esc_trader_can_leave` (and
    `tutorial_wounded_start`) → 321111 ends the talk;
  - 31: marks the choice made, ends the talk and starts the PDA tutorial; a later talk goes 0 → 15 → 321 →
    3211 → 32111, as above;
- which speaker says each phrase and the texts are the dialog spec's (none approved); the path above is the
  data's.

### 4.3 What unlocking does

At the scheme's first update after the info is held [D3, D4d, D4f]:

- the effect `play_snd(device\door_servomotor)` plays that sound 2D at the player;
- the scheme switches to `ph_door@closed`: closed, unlocked; it closes the door again as 2.2 (the motor 200 / 3,
  then 10000 / 1 at the next step), with the hint now `tip_door_open` ("Нажмите ($$ACTION_USE$$), чтобы открыть
  дверь") [D4, D9]; using the door then switches to `ph_door@open`, whose opening sets the motor to 2100 / −3
  (toward the open stop) [D3, D4c]. The open and closed sections' sounds and the re-closing
  (`esc_close_door`) are PH-20's.

## 5. The hint "Дверь заперта"

### 5.1 Trigger

Neither touching the door nor the use action shows it: it is the door object's **hint**, shown while the
object is under the crosshair close enough [B7, B7b, B7c]:

- every frame the player's update reads the object the crosshair currently picks and its distance; the object
  counts only if its distance is **below** `[inventory] take_dist` = 2 (`config\system.ltx`) [B7, B7b, D11];
  nothing counts while one other player condition holds (**unestablished**; believed: while in a vehicle);
- in single play, if that object is usable and has a hint, the player's hint becomes the string table's text
  for the hint id; this comes before the character, item and box hints of `hud-scope` 13 [B7c];
- the hint id is `tip_door_locked` from the moment the locked section starts (2.2) [D4c, B6]; text "Дверь
  заперта" (`config\text\rus\string_table_ui.xml`) [D9].
- Which shapes the crosshair's pick tests, and from where, are `hud-scope` 14's (**unestablished** here).

### 5.2 Position and look

The HUD's `quick_info` static (`config\ui\maingame.xml`; the same element in `maingame_16.xml`) [D10, D10b, B7e]:
x 1, y 700, width 1022, height 50 in the 1024 × 768 layout space (`ui-layout`), multi-line text mode
(`complex_mode`), text centred, font `graffiti22`, colour (225, 225, 250, 255); colour animation `ui_btn_hint`
played once on the text's alpha (`la_cyclic` 0, `la_text` 1, `la_alpha` 1, `la_texture` 0): alpha from 0 at
frame 0 to 255 at frame 29, 30 frames at 30 fps (`lanims.xr` entry `ui_btn_hint`, keys 0 → 0x00000000 and
29 → 0xFF000000; evaluated by `lanim` 3 and 4.3) [D13].

### 5.3 Duration

No timer: every frame the static is shown exactly while the player has a hint, its text replaced when it
differs [B7d]. Whenever the picked object changes (to the door, or away from it), the static's colour start is
set to the current interface time plus its delay (`lanim` 4.2), so the fade-in replays each time the door comes
under the crosshair [B7d]. The hint disappears the frame the door stops being picked within 2 m.

## 6. Unestablished

- When, in frames from a New Game, the door object's first update starts its logic (2.1; believed the first
  frame of play), and whether the hinge has any motor before (2.3; believed none).
- That the door as placed is closed (angle 0) and that the joint's first element is the frame's (2.3).
- Whether anything else keeps the fixed frame's body in place (2.4).
- Which object's pass collides the door with the player, and so the sign of the contact normals (3.2); that
  the collision classes do not exclude the pair (3.2).
- How far the leaf yields under the player's push (3.3).
- The player condition that suppresses the crosshair's object, and which shapes the crosshair's pick tests
  (5.1).
- Whether the HUD's switches (`hud_draw`, `hud_info`; `hud-scope` 2) hide the hint static (5.2).
- Who speaks each dialog phrase and the dialog's texts (4.2).

## 7. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (merged view); binary `bin\xrGame.dll`.
- Data examined: `spawns\all.spawn` (the record `trader_door` with its physics-object block, `sav.md` 7.4.2;
  every record's custom data), `meshes\physics\door\door_trader.ogf` (bone data, `bone`), `gamemtl.xr`,
  `config\scripts\esc_trader_door.ltx`, `config\scripts\esc_trader.ltx`, `config\gameplay\dialogs_escape.xml`,
  `config\gameplay\info_l01escape.xml`, `config\gameplay\character_desc_escape.xml`,
  `config\text\rus\string_table_ui.xml`, `config\ui\maingame.xml`, `config\ui\maingame_16.xml`,
  `config\system.ltx`, `lanims.xr`, the shipped scripts (`ph_door`, `bind_physic_object`, `xr_effects`,
  `xr_sound`, `sound_theme`, `utils`, `escape_dialog` and every script naming the info portion); the public
  ODE 0.5.0 release's headers and sources for the library's parameter numbers, contact modes and group
  collision.
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and disassembly with Ghidra) of the
  script interface's joint and object functions used by the door scheme, the hinge's creation, fixing an
  element, the shell's build and activation order, the per-step script calls, the player's collision body and
  collision pass, the contact generation between objects and the player's contact rules, the player's
  per-frame hint and the HUD's hint static; constants read from the binary. No execution or debugging; the
  game was not run. The checks in `checks/trader-door/` reproduce every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- How a shell is built for this model, whose root bone `link` has no shape, given the fatal rule of `physics-scope` 1.2 and `bone` 8, and which bodies the door's hinge joins.
- What `set_nonscript_usable(false)` changes for the player's use of the door object (for example whether the player can take hold of it).
- Whether the door's shell goes to sleep while held closed at rest, and whether a sleeping shell is still collided with the player's body (3.2 takes only active objects) and woken by its contact.
- Whether the record's mass (10) has any effect on this type-3 shell, whose door bone carries mass 100.
