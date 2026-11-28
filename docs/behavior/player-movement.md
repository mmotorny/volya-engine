# Player movement and first-person controls

Functional specification of how S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) moves the on-foot
player character and its first-person view in a level (the first level, `levels\l01_escape\`, is the case
examined): the movement actions and their default key bindings, how held and toggled actions become a
movement mode (walk, run, sprint, crouch, low crouch, jump), the movement speeds and where each value comes
from, how speed changes are applied by the character physics (acceleration, speed cap, gravity, jumping,
movement in the air), eye height per posture, mouse and keyboard looking, and the camera motions caused by
moving (bobbing, landing).

It builds on:

- `docs/formats/ltx-config.md` (configuration files, sections, keys, value syntax);
- `docs/behavior/world-coordinates.md` (world coordinate system, units, +y up, left-handed; the camera's
  **yaw** and **pitch**, positive yaw turning to the viewer's right, **positive pitch looking down**);
- `docs/behavior/view-projection.md` (the view matrix built from the camera's direction and up, and camera
  inertia, its section 1.1; the field of view, its section 3.1);
- `docs/formats/sav.md` (the spawn packet, its section 7.2, and the player's record in
  `spawns\all.spawn`, its section 10);
- `docs/behavior/startup.md` section 4 (the effective key bindings are those of
  `config\default_controls.ltx` with `user.ltx` applied on top);
- `docs/formats/level-cform.md` (cited as `level-cform`): the player's collision body and its boxes, the
  contacts and their friction (the traction `T`), the ground contact and walkable ground, the simulation
  step, step assistance, the posture-change test, the airborne and landing tests and the impact speed.

What `level-cform` specifies is not repeated here; where movement depends on it this document names the
section.

Conventions:

- Units: metres, seconds, radians, kilograms. Vectors are `(x, y, z)` in world coordinates.
- `|v|` is the length of `v`; `û` is `u / |u|`; `clamp(x, a, b)` limits `x` to `[a, b]`.
- "Frame" is one rendered frame of the game. Two frame times are used, `dt_m` and `dt_f` (section 3.2).
  "Physics step" is one step of the fixed-rate physics simulation (section 6.1).
- Configuration values are given as written in the data and as the number they mean. Key lookups follow the
  LTX specification (inherited sections included).

## 1. Configuration sources

| File (winning copy) | Size | Used for |
|---|---|---|
| `config\creatures\actor.ltx` (`gamedata.dbb`) | 11,359 | section `[actor]`: speeds, jump, air control, mass, collision boxes, eye height factor |
| `config\system.ltx` (`gamedata.dbc`) | 22,829 | section `[actor_firsteye_cam]`: pitch/yaw limits, keyboard turn rate |
| `config\misc\effectors.ltx` (`gamedata.dbb`, included by `system.ltx`) | 1,964 | section `[bobbing_effector]` |
| `config\default_controls.ltx` (`gamedata.dbb`) | 1,111 | default key bindings |
| `__support\userdocs\user.ltx` (game root, copied to the user folder by the installer) | 5,028 | console settings and bindings applied on top |

The `[actor]` section is the one named by the actor's spawn record (`sav.md` sections 7.2 and 10.1). Its parent section `common_ph_friction_params_on_npc_death` (`config\defines.ltx`) holds none of the keys
below.

### 1.1 `[actor]` keys

| Key | Value in data | Meaning here | Section |
|---|---|---|---|
| `walk_accel` | `17` | base acceleration magnitude `A` | 5 |
| `run_coef` | `2.1` | factor when running forward or sideways | 5 |
| `run_back_coef` | `1.8` | factor when running with "back" in effect | 5 |
| `walk_back_coef` | `0.8` | factor when walking with "back" in effect | 5 |
| `crouch_coef` | `0.45` | factor in either crouch | 5 |
| `climb_coef` | `0.7` | factor while climbing (out of scope, section 10) | 5 |
| `sprint_koef` | `2.6` | factor while sprinting | 5 |
| `walk_strafe_coef` | absent → 1.0 | factor when walking with a sideways component | 5 |
| `run_strafe_coef` | absent → 1.0 | factor when running with a sideways component | 5 |
| `jump_speed` | `6.` | vertical speed at take-off | 6.6 |
| `air_control_param` | `0.1` | airborne input force factor | 6.5 |
| `ph_mass` | `80` | mass of the character body | 6.1 |
| `ph_box0_center`, `ph_box0_size` | `0.0,0.9,0.0`, `0.35,0.9,0.35` | standing collision box | 7 |
| `ph_box1_center`, `ph_box1_size` | `0.0,0.75,0.0`, `0.35,0.75,0.35` | crouch collision box | 7 |
| `ph_box2_center`, `ph_box2_size` | `0.0,0.6,0.0`, `0.35,0.6,0.35` | low-crouch collision box | 7 |
| `camera_height_factor` | `0.85` | eye height / box height | 7 |
| `ph_crash_speed_min`, `ph_crash_speed_max` | `14`, `30` | impact speeds for landing classification and the landing effect | 6.7, 9.2 |

When `walk_strafe_coef` or `run_strafe_coef` is present in a section it is read as a float; absent, the
value is 1.0. Neither is present in `[actor]`, so sideways movement has no speed difference of its own.

### 1.2 Console settings

Values the game holds as console variables. "Built-in" is the value before any configuration file is
applied; `user.ltx` lines have the form `<name> <value>`.

| Variable | Built-in | `user.ltx` | Meaning |
|---|---|---|---|
| `g_always_run` | on | `g_always_run on` | run is the default mode; the "accel" action switches to walk (section 4.1) |
| `g_backrun` | — | `g_backrun on` | no effect on movement was found |
| `mouse_sens` | 0.12 (range 0.05 … 0.6) | `mouse_sens 0.12` | mouse sensitivity (section 8.1) |
| `mouse_invert` | off | `mouse_invert off` | vertical mouse inversion (section 8.1) |
| `ph_frequency` | step 0.01 s | `ph_frequency 100.00000` | physics rate; the step is `1 / ph_frequency` = 0.01 s (`level-cform` section 6.1) |
| `cam_inert` | 0.7 | `cam_inert 0.` | camera inertia (`view-projection.md` section 1.1) |

## 2. Actions and default bindings

### 2.1 Binding format

Bindings are console lines in `config\default_controls.ltx` and `user.ltx`:

    bind <action> <key>

`unbindall` (first binding-related line of `user.ltx`) removes all bindings made so far; `bind_sec` binds a
secondary key in the same way. Key names are the letter `k` followed by the key's name (`kW` is the W key,
`kSPACE` the space bar, `kLCONTROL` the left Control key, `kLSHIFT` the left Shift key, `kUP`, `kDOWN`,
`kLEFT`, `kRIGHT` the arrow keys), and `mouse1` … `mouse8` for mouse buttons (`mouse1` is the left
button, `mouse2` the right). Keyboard and mouse are the only input devices: the original has no gamepad
or joystick support (its key-name table contains keyboard and mouse names only).

### 2.2 Default bindings of the movement and look actions

Both `default_controls.ltx` and `user.ltx` contain 58 `bind` lines with identical action-to-key pairs, so the
effective binding of every action is the same whether or not `user.ltx` is present.

| Action | Key | Kind | Effect |
|---|---|---|---|
| `forward` | W | held | move forward |
| `back` | S | held | move backward |
| `lstrafe` | A | held | move left |
| `rstrafe` | D | held | move right |
| `accel` | left Shift | held | invert the run/walk default (walk while held, section 4.1) |
| `crouch` | left Control | held | crouch while held (section 4.2) |
| `crouch_toggle` | not bound | press | crouch toggle (section 4.2) |
| `sprint_toggle` | X | press | toggle sprint (section 4.3) |
| `jump` | Space | press | jump (section 4.4) |
| `llookout`, `rlookout` | Q, E | held | lean left / right (section 10; forces walking, section 4.1) |
| `left`, `right` | ←, → | held | turn (section 8.2) |
| `up`, `down` | ↑, ↓ | held | look up / down (section 8.2) |

"Held" actions are re-applied on every frame while their key is down and lapse in the frame after the key is
released. "Press" actions act once when the key goes down (and `jump` also on release, section 4.4). Other
bound actions (weapons, inventory, use, …) are outside this specification; one of them affects movement: the
moment `wpn_fire` (left mouse button) is pressed, a requested sprint is cancelled (section 4.3).

## 3. Movement request and movement state

Each frame the game keeps two sets of movement flags:

- the **request**, built from the actions of section 2: forward, back, left, right, accel, crouch, jump,
  sprint, lean left, lean right;
- the **state**, which is what the character actually does: the same direction flags, accel, crouch
  (with the crouch height, section 4.2), sprint, jump, and the airborne-related flags *falling* and
  *landing* (section 6.7), and *climbing* (ladders, section 10).

Held actions set their request flag on every frame the key is down; after each movement update the held
flags (direction, accel, lean, and crouch unless the crouch toggle is engaged) are cleared again, so a
released key stops acting in the next frame. Sprint and jump are request flags set and cleared by key
presses (sections 4.3, 4.4).

Per frame, the movement update (sections 4 and 5) runs once, with the movement time `dt_m` (section 3.2).

### 3.1 Frames in the air

Whether a frame is an *air frame* is decided by the airborne state of section 6.5 as left by the previous
frame (the physics of the previous frame). Each frame's movement update does, in this order:

1. the falling countdown (section 6.7);
2. the unable-to-move check (section 5, item 4);
3. the local direction from the request (section 5, item 1, without the opposite-key cancelling).

On an air frame it then **skips everything else** of sections 4.2 (entering a crouch, changing crouch
height), 4.3, 4.4 (starting a jump, counting down the jump cooldown) and 5 (items 1's cancelling, 2), and
only turns the raw local direction by the heading (section 5, item 3): `a` has components ±1, i.e. length
1 (or √2 diagonally), not normalized and not scaled. On an air frame the state's direction, accel and sprint
flags are not taken from the request; they keep their previous values.

What still happens on every frame, air frames included: the clearing of held request flags (section 3), the
lean flags (section 10), the landing classification (section 6.7), the stand-up attempt (section 4.2), the
clearing of the state's direction flags when the speed is below 0.2 m/s (section 9.1), the switch of the
state to walking when the character is unable to run (section 4.1), and the heading update (section 5).

### 3.2 Frame times

Two different frame times are used. While the game is paused the movement update does not run and `dt_f`
is 0.

- **Movement time `dt_m`.** The game keeps a clock in whole milliseconds. `dt_m` is the number of
  milliseconds by which that clock advanced since the previous frame's movement update, capped at 100, times
  0.001 s (so it is a multiple of 1 ms and at most 0.1 s). It is used by the movement update (sections 3–5),
  the jump cooldown (section 4.4) and the falling countdown (section 6.7).
- **Display frame time `dt_f`.** With `t` the real time in seconds since the previous frame,
  `dt_f = min(0.9 · t + 0.1 · dt_f′, 0.1)`, where `dt_f′` is the previous frame's value (a smoothed frame
  time, capped at 0.1 s; 0 while paused; a device setting outside this specification can replace it by a
  fixed 0.02 s). It is used by the physics simulation (section 6.1), the keyboard
  look (section 8.2), the bobbing and the landing effect (section 9), and the per-time damage of a deadly
  ground material (section 6.7).

## 4. Movement modes

### 4.1 Run or walk

A set of flags *runs* when

    (g_always_run XOR accel)  AND NOT ( lean-or-aim AND NOT exempt )

where `accel` is the flag of that set, `lean-or-aim` is true when lean left or lean right is in the set or
the player is aiming down a weapon's sights, and `exempt` is true when the set has crouch, jump, a landing
flag or climbing. With the default `g_always_run on`, the character runs unless left Shift is held; leaning
or aiming makes it walk, except while crouched, jumping, landing or climbing.

The character's *running for speed purposes* additionally requires that it is able to run; conditions that
make it unable to run (and which then also switch its state to walking) belong to the character's condition
(health, stamina) and are out of scope (section 10).

### 4.2 Crouch and low crouch

There are three postures, each with its own collision box (section 7):

| Posture | Box | Entered |
|---|---|---|
| standing | 0 | default |
| crouch | 1 | crouch requested while the request runs (section 4.1) |
| low crouch | 2 | crouch requested while the request walks |

With the defaults: left Control alone gives the crouch, left Control with left Shift the low crouch.

- **Entering:** when the request has crouch and the state has not (and the state is not climbing), the game
  switches the collision box to box 1 if the request runs, else box 2; only if the switch succeeds does the
  state become crouched. While climbing, a crouch request is ignored.
- **Changing crouch height:** while crouched, if the state does not run but the request does, the game
  switches to box 1; if the state runs but the request does not, to box 2. A failed switch leaves the
  posture as it was. After a failed switch to box 1 the state's accel flag is also kept unchanged in that
  frame (the state keeps walking in box 2 and the switch is retried on later frames). After a failed switch
  to box 2 the state's accel flag is nevertheless taken from the request as usual, so the state then walks
  while still in box 1 (crouch speed of the walk mode, 0.765 m/s forward) and, since the state no longer
  runs, no further switch is attempted until the request runs again.
- **Standing up:** when the state is crouched and the request no longer has crouch (or the character starts
  climbing), the game switches to box 0; only on success does the state stop being crouched. If the switch
  fails the character stays crouched and retries on later frames.
- Whether a box switch succeeds is decided by the posture-change test of `level-cform` section 5.3
  (including its rule that a refused target is not retried within 500 ms unless the body has moved at
  least 0.05 m; "retries on later frames" above is subject to it). A failed switch to a taller box under a
  low ceiling is the usual case; a switch to a lower box is believed there always to succeed, so the failed
  switch to box 2 described above is a corner case.
- **Crouch toggle:** the game keeps a toggle state, initially *released*. Pressing `crouch_toggle` flips it;
  when it becomes *engaged*, the crouch request is set and stays set across frames. Releasing the `crouch`
  key sets the toggle back to *released*. While released, the crouch request lasts only while `crouch` is
  held. (`crouch_toggle` is not bound by default.)

### 4.3 Sprint

- Pressing `sprint_toggle` toggles the sprint request.
- Pressing `wpn_fire` clears the sprint request.
- Each frame the sprint request takes effect only if the character can sprint: it can run (section 4.1,
  ability), it is not aiming, not leaning, the current state has neither left nor right, the character's
  condition allows sprinting (stamina; out of scope, section 10), and two further checks pass: one made by
  the game type and one made by the character itself; what they test was not established (section 11). A
  first version may treat them as passing. Otherwise the state does not
  sprint in that frame but the request stays (sprint resumes, for example, when a sideways key is
  released).
- The sprint request and state are both cancelled when the state has none of forward, left, right, or is
  crouched, or is climbing, or when the request does not run (section 4.1). So sprinting needs forward
  movement in run mode; stopping, crouching, walking (Shift) or moving only backward ends it, and it must be
  toggled on again.

### 4.4 Jump

- Pressing `jump` sets the jump request; releasing the key clears it.
- A jump starts in a frame that is not an air frame (section 3.1) when all of these hold: the jump request
  is set; no direction key is being ignored by the unable-to-move check (section 5, item 4); the state is not already
  jumping; the jump cooldown has expired; the jump key has been released since the previous jump (a
  held key does not repeat); the player is not aiming.
- At the start of a jump the state gets the jump flag, the cooldown is set to 0.3 s and the take-off is passed
  to the physics (section 6.6). The cooldown counts down by `dt_m` on frames that are
  not air frames only.
- The jump flag is cleared when the state starts falling (section 6.7), or on a frame on which the character
  is not airborne (on the ground or climbing) when more than 0.1 s of cooldown has elapsed (0.3 − cooldown
  > 0.1, strictly; the cooldown only elapses on non-air frames); clearing it this way sets the cooldown to
  0.3 s again. Landing (section 6.7) also sets the cooldown to 0.3 s and requires the jump key
  to be released before the next jump.
- One further condition for a jump (and for running) is an internal flag of the character's movement that
  is clear when the character is created; its meaning was not established (section 11).

## 5. Desired acceleration

Every frame the movement update produces an acceleration vector `a` that drives the character physics
(section 6). On frames that are not air frames (section 3.1) it is built from the state of section 4 after
that frame's update as follows; on air frames see section 3.1.

1. **Local direction.** `z = (forward ? 1 : 0) − (back ? 1 : 0)`, `x = (right ? 1 : 0) − (left ? 1 : 0)`,
   `y = 0`, taken from the request. If `|z| < 0.00001` the state's forward and back flags are cleared; if
   `|x| < 0.00001` its left and right flags are cleared (opposite keys cancel). If the state has none of
   forward, back, left, right, or the direction has length ≤ 0.00001, `a = 0`.
2. **Magnitude.** With `running` = the state runs (section 4.1, including ability) and `back` = the state's
   back flag:

       |a| = A × M × C × S × T
       A = walk_accel
       M = running ? (back ? run_back_coef : run_coef) : (back ? walk_back_coef : 1)
       C = crouched (either height) ? crouch_coef : 1
       S = sprinting ? sprint_koef : 1
       T = (left or right) and not crouched ? (running ? run_strafe_coef : walk_strafe_coef) : 1

   (and `× climb_coef` while climbing). The local direction is normalized before scaling, so diagonal
   movement is as fast as straight movement.
3. **World direction.** The local vector is turned by the character's heading `θ`:

       a = |a| × ( x̂·cos θ + ẑ·sin θ,  0,  −x̂·sin θ + ẑ·cos θ )

   where `(x̂, ẑ)` is the normalized local direction. `θ` is the heading left by the previous frame: after
   each frame's movement update, if the state then has any direction flag, the heading is set to that
   frame's camera yaw (`world-coordinates.md` section 2); otherwise the heading is the body's own
   heading, which follows the camera only once they differ by more than π/4 (body turning, out of scope,
   section 10). So while moving "forward" is `(sin yaw, 0, cos yaw)` and "right" is `(cos yaw, 0, −sin yaw)`
   with the yaw of the previous frame, both horizontal regardless of pitch; on the first moving frame after
   standing still the direction can differ from the camera's by up to π/4.
4. **Unable to move.** The player is unable to move at all when any of three conditions outside this
   specification holds, tested in this order: the character's power (stamina) is too low to walk, the
   carried weight is too high to walk, or a third condition tested on another component attached to the
   character (what it tests was not established, section 11). If the player is unable to move and the
   request has a direction key, the request's and the state's direction flags are cleared for that frame and
   the jump request is ignored in that frame (no jump starts), so `a = 0`; for the first two conditions the
   game also shows a message (interface, out of scope). A first version that leaves the character's condition
   out (section 10) may treat the player as always able to move.

The physics caps the ground speed at `0.1 × |a|` (section 6.3). Resulting maximum ground speeds with the
shipped values (`A = 17`):

| Mode | `|a|` | Maximum speed (m/s) |
|---|---|---|
| walk forward / sideways / diagonal | 17 | 1.70 |
| walk backward | 13.6 | 1.36 |
| run forward / sideways / diagonal (default) | 35.7 | 3.57 |
| run backward | 30.6 | 3.06 |
| sprint | 92.82 | 9.282 |
| crouch (run mode), forward | 16.065 | 1.6065 |
| crouch (run mode), backward | 13.77 | 1.377 |
| low crouch (walk mode), forward | 7.65 | 0.765 |
| low crouch (walk mode), backward | 6.12 | 0.612 |

"Backward" means the back flag is in effect (back held without forward), with or without a sideways
component; e.g. back + right while running is capped at 3.06 m/s.

## 6. Character physics

### 6.1 Body and simulation

- The character is a physics body of mass `m = ph_mass = 80` (its shapes: `level-cform` section 5)
  simulated at a fixed step `h` of **0.01 s** (`level-cform` section 6.1). Each frame the display frame time
  `dt_f` (section 3.2) is added to an accumulator; as many whole steps as fit are run (at most 10 per frame,
  since `dt_f ≤ 0.1`), and the remainder carries over to the next frame.
- **Gravity:** 19.62 m/s² downward (−y). It is a constant of the game; no configuration key sets it.
- The character's contacts with the level and objects, their normals, and the step and obstacle handling
  are specified in `level-cform` (sections 4–6). This section uses these of their results: the *ground
  contact* and its *ground normal* `n` (the contact whose normal has the largest y component), *walkable
  ground* (a ground contact with `n.y > 0.70710677`, ground no steeper than 45°; `level-cform` section
  6.2), and the step's friction factor `f`, which is the traction `T` of `level-cform` section 5.4 (section
  6.4).

**Hand-over of the acceleration.** Each frame, after the movement update, its acceleration `a` (section 5,
or section 3.1 on an air frame) is offered to the body. Before that, if an impulse has been applied to the
character by another game object since the previous frame (for example a monster's attack; out of scope),
a pending vector derived from it is added to `a` once and then cleared; without such impulses (ordinary
movement) nothing is added. Two things are then handed over:

- the **speed limit** `v_max = 0.1 × |a|` of section 6.3 is set from this offered `a` on **every** frame,
  unconditionally;
- the acceleration **vector** is filtered as follows.

The body holds the last value it accepted, `a_k` (zero
when the character is created), and accepts the new value, which then replaces `a_k`, unless the new value
is close to it. With `â_new` and `â_k` the directions of the new value and of `a_k` (each vector divided by
its length; a vector shorter than 0.0000001 is used as it is, i.e. as practically the zero vector), the new
value is **ignored** when all three components satisfy `|â_new − â_k| < 0.05` and in addition
`| |a_new| − |a_k| | < 0.5`; otherwise it is accepted. Everything in sections 6.2–6.6 that uses the
acceleration vector — the *moving* test `|a_k| > 0.1`, the driving-force direction, the airborne input
force, the take-off direction `â` of section 6.6 and the jump assistance `a_h` — uses `a_k`; only the speed
limit uses the offered `|a|`.

This refines `level-cform` section 6.2, whose "requested acceleration ... replaces the previous value" is,
precisely, this filtered `a_k`: the choice of the obstacle contact (`level-cform` 6.2) and the
step-replacement tests on `â` (`level-cform` 6.3) read `a_k` (checked); the other "movement is requested"
(`|a| > 0.1`) tests and uses of `â` in `level-cform` sections 5.4, 6.2 and 6.3 are believed to read it as
well (they take the acceleration vector from the body, whose only stored acceleration vector is `a_k`).

Consequences:

- while moving and turning the camera slowly, the movement direction (on the ground and in the air)
  follows the heading in steps, not continuously: a new direction is taken only once some component of the
  unit direction has changed by 0.05, i.e. after a turn of between about 2.9° and 3.9° depending on the
  heading (2.9° when the direction is along a world axis, 3.9° when it is diagonal to them);
- a change of `|a|` smaller than 0.5 with an unchanged direction leaves `a_k` unchanged, but the speed limit
  follows at once; e.g. switching between walking backward standing (`|a|` = 13.6) and backward in the
  crouch of run mode (13.77) keeps the old vector, and the limit becomes 1.377 m/s in that frame;
- stopping (`a = 0`), a new direction key, and the change between a scaled ground value and the unscaled air
  value of section 3.1 always differ enough and are accepted at once.

A take-off (section 6.6) is handed over separately.

Each step the following forces act on the body (in addition to gravity and contact forces), and after the
step its velocity is limited (section 6.3). After the character's forces of a step have been added, a total
force on the body larger than `5 · m / h` (40,000 N for 80 kg and `h` = 0.01 s) is scaled down to that
magnitude, keeping its direction; this limit is far above the forces of ordinary movement and never acts
there.

### 6.2 Driving force on the ground

Let `moving` be `|a| > 0.1`, and `f` the step's friction factor (section 6.4). When moving and not airborne:

- on walkable ground: `F = 25 · f · m · d̂`, where `d̂` is the unit vector of the horizontal input direction
  `(a.x, 0, a.z)` projected onto the ground plane (perpendicular to `n`) along the vertical;
- with contacts but no walkable ground (only walls or slopes steeper than 45°): `F = 37.5 · f · m · d̂_h`,
  `d̂_h` the horizontal unit input direction;
- with no contact at all, `f = 0` and so `F = 0`.

The driving force does not depend on `|a|`; `|a|` only sets the speed cap. On flat ground with `f = 1` the
character therefore accelerates at 25 m/s² until it reaches the cap (0.143 s from standing to 3.57 m/s).

While moving and not airborne a sideways damping force also acts:
`F_side = −500 · f · (v · ŝ) · ŝ`, where `v` is the body's velocity and `ŝ` the horizontal unit vector
perpendicular to the driving force; it removes velocity across the movement direction (for an 80 kg body,
at a rate of `6.25 · f` per second). In steps where the character is moving and not airborne but has no
contact, an extra downward force of `50 · m` acts (it keeps the character on the ground over small bumps).

During step assistance (stairs and ledges) the driving and side forces are changed as specified in
`level-cform` sections 6.2 and 6.3 (drive along the obstacle plane, × 4, side coefficient 700, side force
also while airborne).

### 6.3 Speed cap and stopping

After each step the body's velocity `v` is limited to a speed `v_max`:

- `v_max = 0.1 × |a|` when moving and not airborne, with `|a|` the length of the acceleration offered by
  the latest movement update (section 6.1, not `a_k`) and "moving" meaning `|a_k| > 0.1`; `v_max = 150` m/s
  otherwise (not moving, or airborne). On an air frame the offered `|a|` is 1 (√2 diagonally), so if the
  character lands with a direction key held, the limit in the steps until the next movement update is
  0.1 m/s (0.1414 m/s diagonally).
- If `|v| ≤ v_max` nothing changes. If `|v_max| < 0.0000001` (cannot happen while moving) the velocity is set
  to 0.
- Otherwise the reduction depends on a *loose* flag, which is set for the step when the character is
  airborne, or is not on walkable ground (section 6.1) and is not climbing:
  - if the flag is set and `−150 < v.y < 0` (descending), only the horizontal components are scaled:
    `v.x` and `v.z` are multiplied by `v_max / |v|`, `v.y` is kept (the resulting speed stays above
    `v_max`); so walking off a step or edge does not limit the descent speed;
  - otherwise the whole vector is scaled to length `v_max` (`v' = v · v_max / |v|`), and the body's position
    is corrected by `(v' − v) × 0.01 s` (the position it would have had with the limited velocity in this
    step).
- In addition, only in a step in which the velocity was over the limit (after either branch above) and the
  character is moving and not airborne, the body's position for the step is then set to its position after
  the previous step plus the limited velocity × 0.01 s. Steps with `|v| ≤ v_max` keep the solver's position.
- A lower `|a|` (e.g. pressing Shift while running, or crouching) therefore reduces the speed within one
  step; a higher `|a|` raises it at the driving acceleration of section 6.2.
- When not moving (no direction key, `|a| ≤ 0.1`) there is no driving force and only the 150 m/s limit; the character is
  slowed by the friction of its contacts (section 6.4), which normally stops it on the ground.
- After each step a weak drag acts in every state: `F = −k · v`, `k = min(0.0002 · |v|, m / 0.01)`
  (negligible at walking speeds).

### 6.4 Contact friction

The friction factor `f` of sections 6.2 and 6.3 is the step's final traction `T` of `level-cform` section
5.4 (the largest contact friction value of the step, 0 without contacts; 1 on the shipped solid surfaces
with the player's default material). The friction of the contacts themselves is also specified there: zero
while moving (`|a_k| > 0.1`), so the driving force and, on walkable ground, the speed cap alone determine
the motion; when not moving, contact friction holds and stops the character. The resulting deceleration
emerges from the contact solver and is not given as a number here (section 11).

### 6.5 Airborne movement

The character is **airborne** from a take-off (section 6.6), or after leaving the ground without a jump by
the rule of `level-cform` section 6.4 (more than 0.5 m from the departure point and more than 0.1 m from it
vertically), until the airborne state ends by the rule of that section (a ground contact with
`n.y > 0.35355338`, or practically zero speed) or the character starts climbing.

While airborne:

- the force from the input is `F = m · air_control_param · a_k`; on air frames `a` is the unscaled direction
  of section 3.1, so this is an acceleration of **0.1 m/s²** (0.1414 m/s² diagonally) in the input
  direction, whatever the mode. (On the frame on which a jump starts, which is not an air frame, `a` is the
  full scaled vector; it is replaced by the unscaled one from the first air frame on.) Only the 150 m/s
  limit of section 6.3 applies;
- gravity, the drag of section 6.3 and contact forces act as usual (contact friction as in section 6.4).

### 6.6 Jump take-off

A take-off passed by the movement update (section 4.4) is evaluated in that same frame: it is accepted only
if the character is not airborne and the most upward contact normal recorded at the last physics step that
had contacts has `n.y > 0.5` (or the character is climbing, out of scope). That recorded normal is not
cleared when contact is lost, so a jump can still be taken after walking off an edge until the character
becomes airborne (section 6.5), and is refused on slopes steeper than 60°. A refused
take-off is not retried (the state's jump flag is set anyway, section 4.4). For an accepted take-off the
velocity `v` is read in that frame and the new velocity is applied at the next physics step, which **sets**
the body's velocity to

    v' = ( 1.2·v.x + 0.2·â.x,   jump_speed,   1.2·v.z + 0.2·â.z )

where `v` is the velocity read in the take-off frame and `â = a_k / max(|a_k|, 1)`, `a_k` the acceleration
held by the body (section 6.1): the unit input direction when moving, 0 when not. The character becomes airborne. With `jump_speed = 6` and gravity 19.62 m/s², a jump on
flat ground rises 6² / (2 × 19.62) = **0.917 m** and returns to the take-off height after
2 × 6 / 19.62 = **0.612 s**; a running jump keeps 1.2 times the horizontal speed (4.28 m/s at 3.57 m/s, plus
0.2 m/s in the input direction), and on landing with a direction key held the speed is capped again
(section 6.3, including its remark on the limit right after landing).

**Jump assistance.** From the take-off until the character is next on walkable ground (section 6.1; or has a
climbing contact), with `P0` the body position at take-off, `a_h = (a_k.x, 0, a_k.z)` and `L = |a_h|`, two extra
horizontal forces act in each step while `L > 0`:

- while the horizontal progress `(a_h / L) · (p − P0)` of the body position `p` from `P0` along the input
  direction is less than 0.3 m: `F = 1000 · a_h / L` (newtons, not scaled by mass: 12.5 m/s² for 80 kg);
- while the horizontal velocity `v_h` points against the input (`a_h · v_h < 0`):
  `F = 3000 · (a_h · v_h) / (L · |v_h|²) · v_h`, which brakes the backward motion.

### 6.7 Falling and landing states

These flags affect the run rule (section 4.1) and the bobbing (section 9.1), not the speed:

- **Falling:** a countdown runs by `dt_m` on frames when the character is airborne and the
  state is not falling; when it reaches zero the state gets the falling flag and loses the jump flag, and
  the countdown restarts at 0.2 s. It is not reset otherwise (so a jump normally starts falling 0.2 s after
  take-off, or sooner if an earlier short airborne phase used part of the countdown). Its value when the
  character is created was not established.
- **Landing:** on the step the character first touches something while airborne, if the state is falling
  and the impact speed `v_i` is above 4 m/s, the state gets a landing flag for 0.1 s — 0.3 s (the hard
  landing) when the impact's damage fraction is non-zero, i.e. when `v_i` exceeds `ph_crash_speed_min = 14`
  (fraction `(v_i − 14) / (30 − 14)`), or when the ground material is **deadly** (its damage rate × `dt_f`
  is added to the fraction; `level-cform` section 4.4). Falling and jump flags are cleared on that contact
  in any case, the jump cooldown (section 4.4) is set to 0.3 s, and a jump key still held must be released
  before the next jump.
- `v_i` is the impact speed of `level-cform` section 4.4 (the largest over the contacts of the material's
  impact factor times the speed into the contact); as noted there, that it is this value that reaches the
  landing rules is believed, not fully traced.

## 7. Eye height and posture transitions

The eye is directly above the character's position (its feet):

    eye = position + (0, h, 0),   h = H × camera_height_factor

- `camera_height_factor` is read from the character's configuration section, i.e. the section named in its
  spawn record (`actor`, in `config\creatures\actor.ltx`, dbb): `camera_height_factor = 0.85` [E1].
- `H` is the height of the character's current collision box. The section defines three boxes, `ph_box0_*`,
  `ph_box1_*`, `ph_box2_*`, each by `ph_boxN_center` and `ph_boxN_size` (float3); the box spans
  `center − size` … `center + size`, so its height is `2 × size.y`. At spawn the box with index 0 is made
  current (boxes 1 and 2 are only selected later, by the movement, section 4.2), so `H = 2 × ph_box0_size.y`.
- Values: `ph_box0_center = 0.0, 0.9, 0.0`, `ph_box0_size = 0.35, 0.9, 0.35` → `H = 1.8`, `h = 1.53` [E1].

The vertical offset is not rotated by the character's heading (the heading turns about the y axis only).

All three boxes have their bottom at the feet (`center.y − size.y = 0`):

| Posture | Box | `H` | Eye height `h` |
|---|---|---|---|
| standing | 0 | 1.8 | 1.53 |
| crouch | 1 | 1.5 | 1.275 |
| low crouch | 2 | 1.2 | 1.02 |

A posture change takes effect in the frame its box switch succeeds (section 4.2), and the eye height changes
to the new value in that same frame: there is no smoothing of posture changes. The only smoothing of the eye
height is the lag when the character's position rises (stairs, steps): the camera's height follows with a
lag, at most 0.2 m behind, catching up at 1.5 m/s [B3]; none while airborne or descending; it does not
apply at spawn. A leaning offset is zero at spawn. Camera bobbing and the landing effect (section 9) are
added on top.

### 7.1 Worked example: the start of a New Game

The player is placed at the stored position of its record in `spawns\all.spawn` (`sav.md` section 10.2),
(−246.72630, −24.793261, −134.43387), the position of its feet: the first collision surface of
`level.cform` below the eye, on the vertical line through the stored position, lies 0.00013 m above the
stored y, i.e. 1.52987 m below the eye [W1, E2]; the stored position is that close to the floor. Standing
(box 0, `h = 1.53`), the start eye position is **(−246.72630, −23.263261, −134.43387)** [E1].

## 8. Looking

### 8.1 Mouse

Each frame the relative mouse motion of the frame (the sum of all horizontal and vertical motion reports,
in the input device's counts; positive `dx` to the right, positive `dy` toward the user) changes the
first-person camera's yaw and pitch:

    s     = mouse_sens × K × 0.02 × (F_view / 67.5) / c
    yaw   = yaw   + dx × s
    pitch = pitch + dy × s × 0.75 × (mouse_invert ? −1 : 1)

- `K` is an engine sensitivity scale with the constant value 1.0 (nothing changes it).
- `F_view` is the camera's current vertical field of view in degrees: 67.5 when not aiming
  (`view-projection.md` section 3.1), so the factor is 1 then; it is smaller while aiming through a
  zooming weapon (out of scope).
- `c` is the control-inertia factor of the active inventory item (its configuration key
  `control_inertion_factor`, 1.0 when absent), and 1.0 when no item is active. Which item is active at New
  Game belongs to the inventory (out of scope).
- With the defaults (`mouse_sens 0.12`, not aiming, `c = 1`): 0.0024 rad (0.1375°) of yaw and 0.0018 rad
  (0.1031°) of pitch per count. Moving the mouse right turns right; moving it toward the user looks down
  (`mouse_invert off`).
- **Limits:** pitch is clamped to `lim_pitch` of `[actor_firsteye_cam]`, −1.5 … 1.5 rad (about ±85.9°)
  after every change. `lim_yaw = 0, 0` means yaw is not limited (it is not wrapped by the camera either).
- `mouse_invert` is a console flag (`mouse_invert on|off`); `mouse_sens` accepts 0.05 … 0.6.

### 8.2 Keyboard look

While `left` / `right` is held, yaw decreases / increases by `rot_speed.x × dt_f / c` per frame; while `up` /
`down` is held, pitch decreases / increases (looks up / down) by `rot_speed.y × dt_f / c`, with the pitch
limit as above. `rot_speed = 3.14, 3.14, 0` in `[actor_firsteye_cam]`: π rad/s.

## 9. Camera motion

### 9.1 Bobbing

A bobbing motion is applied to the player's camera each frame, after the eye position and orientation are
computed. It keeps:

- a time `τ` that grows by the display frame time `dt_f` (section 3.2) on every frame;
- a fade `w`: on frames when the state has any of forward, back, left, right, `w` becomes `w + 5·dt_f` if
  `w < 1`, else 1; on other frames, if `w > 0` it decreases by `5·dt_f` (and may thereby become slightly negative), else it is
  set to 0. A negative `w` in such a frame produces a tiny downward bounce (the formulas below use `w` with
  its sign). (The state's direction flags
  are cleared on frames when the character's speed is below 0.2 m/s and it is neither jumping nor falling, so
  bobbing fades out when the character is blocked.)

When `|w| ≥ 0.00001`, with `k = 0.75` if the state is crouched (either height) and 1 otherwise, and
amplitude and rate chosen from `[bobbing_effector]`:

| Condition | Amplitude `B` | Rate `R` |
|---|---|---|
| the state runs (section 4.1, ignoring ability) | `run_amplitude = 0.0075` | `run_speed = 10.0` |
| else, limping (out of scope) | `limp_amplitude = 0.011` | `limp_speed = 6.0` |
| else | `walk_amplitude = 0.005` | `walk_speed = 7.0` |

let `φ = R · k · τ`, `b = B · k`, `δ = |sin φ · b| · w` and `ρ = cos φ · w · b`. The camera's position is
raised by `δ` (`p.y += δ`), and with the camera's direction `d`, up `u` and `r = u × d` (its right vector,
`view-projection.md` section 1.1), the direction and up vector become

    d' = cos ρ·cos δ · d − sin ρ·cos δ · r + sin δ · u
    u' = (sin²ρ − cos²ρ·sin δ) · d + cos ρ·cos δ · u + (cos ρ·sin ρ·sin δ + sin ρ·cos ρ) · r

(exactly these combinations; they are not renormalized; the view construction of `view-projection.md`,
section 1.1, then re-orthogonalizes `u'` against `d'`). The effect is a vertical bounce of at most `b` (7.5 mm running, 5 mm walking) with a slight sway, twice per period `2π / (R·k)` for the bounce.

### 9.2 Landing effect

On the step the player character first touches something while airborne (section 6.7), a landing effect
starts with strength

    p = clamp(v_i / ph_crash_speed_max, 0, 1)²        (ph_crash_speed_max = 30)

It keeps a time `t` from 0; each frame `t` grows by `3.5 · dt_f`, and while `t < 1` the camera position's y is
changed by `− sin((t + 1)·π) · p · 0.15`, i.e. raised by `0.15 · p · sin(π·t)`, which rises and returns
over 1/3.5 ≈ 0.286 s. When `t` reaches 1 the effect ends. For an ordinary jump landing (`v_i` ≈ 6 m/s,
`p` ≈ 0.04) the offset peaks at about 6 mm.

## 10. Out of scope

Listed so that a first version can ignore them knowingly:

- **Leaning** (`llookout` Q, `rlookout` E): the state's lean flags follow the request except while jumping,
  falling or landing; the lean angle moves toward −π/4 (left), +π/4 (right) or 0 (none or both)
  at 2π rad/s and offsets and tilts the eye. The exact eye offset and camera tilt are not specified here.
  Its effect on movement (forcing walk, preventing sprint) is in sections 4.1 and 4.3.
- **Aiming** through weapon sights (forces walk, prevents sprint and jump, changes the field of view and
  the mouse factor).
- **Climbing** ladders (`climb_coef`), vehicles, and scripted control of the player.
- **Condition:** health, power (stamina) and carried weight. The `[actor_condition]` section of
  `actor.ltx` has keys about power use while walking, running, sprinting and jumping
  (`walk_power`, `accel_k`, `sprint_k`, `jump_power`, …), limping thresholds (`limping_*`), and thresholds
  below which the character cannot sprint or walk (`cant_sprint_power_*`, `cant_walk_power_*`), and
  `max_walk_weight` / `max_item_mass` limit the carried weight. They do not change the speeds of section 5;
  they can disable running, sprinting or moving, and their rules were not established here.
- Slowdown after being hit, fall damage, footstep sounds, the character's body animation and its turning
  while standing (the movement direction uses the camera yaw, section 5).
- Third-person cameras.

## 11. Unestablished

- The stopping deceleration on actual surfaces as a number (it emerges from the contact friction of
  `level-cform` section 5.4 and the contact solver; section 6.4).
- The rules of the character's condition that disable running, sprinting or moving (power, carried weight),
  the third unable-to-move condition of section 5 item 4 (tested on another component attached to the
  character), and the initial condition at New Game (section 10).
- Which inventory item is active at New Game, and so the mouse factor `c` at the start (section 8.1).
- The two further sprint checks (game type, character) of section 4.3.
- A rarely set special case of the speed limit of section 6.3 (an additional limit derived from a stored
  direction, applied only when an internal flag is set; it is clear in ordinary movement), not described.
- The meaning of the internal movement flag that must be clear for running and jumping (section 4.4), and of
  a time stamp before which running is disabled (part of the ability to run, section 4.1), and that time
  stamp's value at New Game.
- Which of the constants of section 6 written with 0.01 s follow the step `h` when `ph_frequency` is not
  100 (the force limit of section 6.1 does), and the effect of `ph_iterations` (`level-cform` section 6.1).

## 12. Verification facts

Configuration (values as read with the LTX rules from the winning copies of section 1):

- `[actor]`: `walk_accel 17`, `jump_speed 6.`, `run_coef 2.1`, `run_back_coef 1.8`, `walk_back_coef 0.8`,
  `crouch_coef 0.45`, `climb_coef 0.7`, `sprint_koef 2.6`, `air_control_param 0.1`,
  `camera_height_factor 0.85`, `ph_mass 80`, `ph_crash_speed_min 14`, `ph_crash_speed_max 30`;
  `walk_strafe_coef` and `run_strafe_coef` absent; boxes as in section 1.1 (all bottoms at 0).
- `[actor_firsteye_cam]`: `lim_pitch -1.5,1.5`, `lim_yaw 0,0`, `rot_speed 3.14,3.14,0`.
- `[bobbing_effector]`: `run_amplitude 0.0075`, `walk_amplitude 0.005`, `limp_amplitude 0.011`,
  `run_speed 10.0`, `walk_speed 7.0`, `limp_speed 6.0`.
- `default_controls.ltx` and `user.ltx`: 58 `bind` lines each, identical action/key pairs; the 14
  movement and look actions of section 2.2 bound as listed, `crouch_toggle` unbound in both.
- `user.ltx`: `g_always_run on`, `g_backrun on`, `mouse_sens 0.12`, `mouse_invert off`,
  `ph_frequency 100.00000`, `ph_iterations 38`, `cam_inert 0.`.

Checkable relations:

- Maximum speeds (section 5 table) are `0.1 × walk_accel ×` the products of coefficients: walk 1.7, run
  3.57, sprint 9.282, crouch 1.6065, low crouch 0.765 m/s forward.
- Eye heights 1.53 / 1.275 / 1.02 m = `0.85 × 1.8 / 1.5 / 1.2`; the standing value gives the start eye
  of section 7.1.
- Jump apex 0.9174 m and flat-ground air time 0.6116 s from `jump_speed 6` and gravity 19.62.
- Mouse: 0.0024 rad yaw and 0.0018 rad pitch per count at the default sensitivity.
- Air control: `air_control_param × 1` = 0.1 m/s² straight and `0.1 × √2` = 0.1414 m/s² diagonally on air
  frames, independent of the mode.
- Force limit `5 × ph_mass / 0.01` = 40,000 N; at most 10 physics steps per frame (`dt_f ≤ 0.1`).
- Acceleration hand-over (section 6.1): with `|a|` unchanged, a horizontal direction along a world axis must
  turn by 0.05 rad (2.86°), one diagonal to the axes by 0.0684 rad (3.92°), before it is accepted; a vector change 13.6 → 13.77 (`|Δ|` = 0.17 < 0.5) is ignored while the speed
  limit changes 1.36 → 1.377 m/s; 13.6 → 35.7 is accepted.

## 13. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); archives `gamedata.db0` …
  `gamedata.dbd`; `__support\userdocs\user.ltx`; binaries `bin\XR_3DA.exe` and `bin\xrGame.dll`.
- Data examined: `config\creatures\actor.ltx`, `config\system.ltx`, `config\misc\effectors.ltx`,
  `config\defines.ltx`, `config\default_controls.ltx`, `config\weapons\*.ltx` (for
  `control_inertion_factor`), `user.ltx`.
- Analysis: static analysis of the binaries (disassembly and decompilation with Ghidra) to establish the
  input actions and their handling, the run/walk, crouch, sprint and jump rules, the acceleration and speed
  computation, the character physics forces, caps, gravity, take-off and airborne control, the eye height,
  the mouse and keyboard look mapping, the console variables and their defaults, and the bobbing and
  landing camera effects, the hand-over of the acceleration to the character body, the force limit, and
  the two frame times (the game's millisecond clock and scheduler, the display frame-time smoothing and
  the physics step accumulator); the configuration values and bindings were read from a fresh extraction of
  the winning copies with an independent throwaway LTX reader written from the LTX specification, which
  also recomputed every derived number of section 12. Consistency with the approved `level-cform`
  and `ltx-config` specifications and with the start view's camera facts (now in `world-coordinates.md`
  and `view-projection.md`) was checked section by section. The program was not
  executed.
- The eye-height rule at spawn, the rising-eye lag constants and the worked example of section 7 were
  established on 2026-11-23 in the investigation of the start view (static analysis of the actor's eye
  height; checked against `config\creatures\actor.ltx` and `levels\l01_escape\level.cform` with
  independent throwaway parsers) and moved here on 2026-11-27 without new investigation. Their checks are
  `checks/start-view/` of the dirty workspace (`run.py`): `data.py` [E…, W…] reads the game data,
  `binary_notes.py` [B…] checks constants in the original investigation's static-analysis outputs; all
  pass.
