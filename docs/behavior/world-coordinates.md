# World coordinates and orientation angles

Functional specification of the world coordinate system of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition), shared by the level geometry, the objects of the spawn file and the camera (units, up axis,
handedness), and of how an object's three stored orientation angles become the player camera's yaw, pitch and
roll and the camera's direction and up vectors in world coordinates. The start of a New Game is its worked
example (section 4).

It builds on:

- `docs/formats/sav.md` (cited as SAV): the spawn packet and its position and angles fields (section 7.2), and
  the spawn file's object list and the player's record in it (section 10);
- `docs/behavior/view-projection.md` (cited as VP): the view matrix built from the direction and up vectors
  (section 1.1) and the projection (section 2);
- `docs/behavior/player-movement.md` (cited as PM): the eye height above the stored position (section 7) and
  the pitch limit (section 8.1).

Conventions:

- Angles are in radians unless stated. `sin`, `cos`, `tan` are the usual functions.
- Vectors are `(x, y, z)`; `×` is the ordinary cross product of the components.
- Rotations are written as matrices applied to column vectors (this section only; the view and projection
  matrices of VP use the row-vector convention).

**Checks.** Every claim carries a tag. The checks are scripts in `checks/start-view/` of the dirty
workspace (`run.py` runs them all): `data.py` [W…, S…, E…] reads the game data (`spawns\all.spawn`,
`config\creatures\actor.ltx`, `config\system.ltx`, `levels\l01_escape\level.cform`); the static-analysis facts
are those of the original investigation of the player camera (read-only decompilation of `bin\xrGame.dll` and
`bin\XR_3DA.exe`), whose camera-manager and projection parts are checked by VP's checks [X…, G…], run through
`run_vp.py`. All checks pass.

## 1. World coordinate system

One coordinate system is shared by the level geometry (`level`, `level.geom`, `level.cform`, …), the
objects in `spawns\all.spawn`, the environment modifiers (`level.env_mod`) and the camera:

- **Units:** metres. The actor's configured collision box is 1.8 units tall (PM section 7) [W5]; at the
  start position, on the vertical line through the stored position, the collision surface of `level.cform`
  lies 0.00013 units above the stored y (the floor; no collision triangle lies below the stored position on
  that line) and 3.18 units above it (the ceiling) [W1].
- **Up axis:** +y. The camera's up vector is +y for zero pitch (section 3) [W2]; gravity-bound objects stand
  on surfaces below them in y.
- **Handedness: left-handed.** The camera's right vector is computed as `right = up × direction` (VP
  section 1.1) and the projection maps view-space +x to the right of the screen, +y to the top and +z (the
  viewing direction) into the screen (VP section 2) [X1]. So, seen from a viewer at the origin looking along
  +z with +y up, +x is on the viewer's right. Seen from above (looking down −y), with +z pointing to the top
  of the page, +x points to the right.
- Coordinates in the spawn file are the level's own coordinates, the same as those of the level's geometry
  files (no per-level offset is applied; the `offset` values of `game.graph`'s level table /
  `game_levels.ltx` are not involved).

A right-handed consumer can convert by negating one axis (e.g. z) of every position and direction; this
reverses the winding of every triangle. Which winding the original renderer culls is **unestablished** here
(it belongs to the geometry/rendering specifications).

## 2. Stored angles → camera yaw, pitch and roll

A spawn packet stores an object's orientation as three angles `a0, a1, a2` (SAV section 7.2). The
first-person camera keeps three angles, **yaw**, **pitch** and **roll**. When the player's object is spawned
they are set from its stored angles as:

    yaw   = −a1
    pitch =  a0
    roll  =  0        (a2 is not used by the camera)

[W2]

The first-person camera's configuration section `[actor_firsteye_cam]` in `config\system.ltx` limits pitch
to `lim_pitch = -1.5, 1.5`; `lim_yaw = 0, 0` means yaw is unlimited [W4] (PM section 8.1).

## 3. Camera basis from the angles

The camera's orientation is the rotation

    R = Ry(yaw) · Rx(pitch) · Rz(roll)

applied to column vectors, where `Ry`, `Rx`, `Rz` are the standard rotations about the y, x and z axes:

    Ry(t) = [[ cos t, 0, sin t], [0, 1, 0], [−sin t, 0, cos t]]
    Rx(t) = [[1, 0, 0], [0, cos t, −sin t], [0, sin t, cos t]]
    Rz(t) = [[cos t, −sin t, 0], [sin t, cos t, 0], [0, 0, 1]]

The camera's viewing direction is `R · (0, 0, 1)`, its up vector `R · (0, 1, 0)`. With roll 0:

    direction = ( cos(pitch)·sin(yaw), −sin(pitch), cos(pitch)·cos(yaw) )
    up        = ( sin(pitch)·sin(yaw),  cos(pitch), sin(pitch)·cos(yaw) )

[W2]

Consequences: yaw 0, pitch 0 looks along +z; positive yaw turns toward +x (clockwise seen from above, i.e. to
the viewer's right); **positive pitch looks down** [W2].

The view matrix built from these vectors and the eye position is VP section 1.1.

## 4. Worked example: the start of a New Game

The player's record in `spawns\all.spawn` (object index 63, section `actor`; SAV section 10.2) stores
position (−246.72630, −24.793261, −134.43387) and angles a0 = 0.0043165367, a1 = −1.3962544,
a2 = −0.00068671 [S4]. The eye is 1.53 m above the stored position, at (−246.72630, −23.263261, −134.43387)
(PM section 7.1) [E1].

- yaw = +1.3962544 (79.999°), pitch = +0.0043165 (0.247°), roll = 0 [W2]; the start pitch is within the
  limit of section 2.
- direction = (0.984797, −0.0043165, 0.173655), up = (0.0042509, 0.999991, 0.00074959) [W2].

Consistency evidence: the trader object (`escape_trader`, section `m_trader`, object index 84, position
(−244.693, −24.958, −133.960)) [S7] stands 2.09 m (horizontally) in front of the eye, 3.1° off the horizontal
viewing direction, i.e. the player starts facing the trader [W3].

## 5. Unestablished

- The triangle winding (culling) convention of the level geometry (section 1).

## 6. Provenance

- Date: 2026-11-23 (established in the investigation of the start view); moved into this specification
  2026-11-27 without new investigation.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrRender_R1.dll`.
- Data examined: `spawns\all.spawn`, `config\creatures\actor.ltx`, `config\system.ltx`,
  `levels\l01_escape\level.cform`.
- Analysis: static analysis of the binaries (disassembly and decompilation with Ghidra) to establish the
  initial camera angles and the camera basis; the facts were checked against the game data with independent
  throwaway parsers (the actor's values, the floor and ceiling above and below the start position in
  `level.cform`, and the trader's position relative to the view direction).
