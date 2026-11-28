# View and projection: field of view, near plane, frusta

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the first renderer
(`bin\xrRender_R1.dll`), of the projection each frame is drawn with and the view volumes the renderer's passes
test against: how the field of view, aspect, near and far values reach the projection; the field of view of
every camera during play (on foot, aiming, in a vehicle, special effects); which camera gets a computed near
value; the projection of the first-person models; and the frustum planes of every pass that builds its own
(camera, portals, detail objects, occluders, dynamic lights, projected shadows); the view matrix and the far
value; the start of a New Game is its worked example (section 2.3). It is specification V2 of the scoping
specification VS and builds on its sections 3.1 and 4.

Conventions: matrices act on row vectors, (x, y, z, w) = (p, 1) · M; the view-projection **VP** = V · P is the
frame's view matrix V times its projection P; for a point, w is its view depth. Angles of view are in degrees;
"fov" is always the **vertical** field of view (VS 4). *W* × *H* is the render-target size in pixels.
Abbreviations of approved specifications: VS `docs/behavior/visibility-scope.md`, WC
`docs/behavior/world-coordinates.md` (world coordinates; the camera's direction and up vectors), LV
`docs/formats/level.md`, DL `docs/behavior/dynamic-lights.md`, LS `docs/behavior/light-shadows-projection.md`,
DO `docs/behavior/detail-objects.md`, PM `docs/behavior/player-movement.md`, RS
`docs/behavior/r1-settings-scope.md`, SAV `docs/formats/sav.md`.

**Checks.** Every claim carries a tag. The checks are scripts in `checks/view-projection/` of the dirty
workspace (`run.py` runs them all): `binary_x3.py` [X…] reads `bin\XR_3DA.exe`, `binary_r1.py` [R…]
`bin\xrRender_R1.dll`, `binary_game.py` [G…] `bin\xrGame.dll` (static analysis: read-only headless
decompilation and disassembly, constants read from the binaries' data); `model.py` [M…] is arithmetic;
`data.py` [C…] reads the configuration of the merged archive view. The checks of the view matrix, the far
value and the worked example are in `checks/start-view/` (`run.py` runs them and, through `run_vp.py`, the
checks above): `data.py` [V…, E…, S…, W…] reads `spawns\all.spawn`, `config\creatures\actor.ltx`,
`config\weathers\weather_default.ltx`, `levels\l01_escape\level.env_mod`, `levels\l01_escape\level.cform`
and the default `user.ltx`; `binary_notes.py` [B…] checks constants of the binaries in the original
investigation's static-analysis outputs. All checks pass.

## 1. From a camera to the frame's matrices

A **camera manager** holds the values the frame is drawn with: position, direction, up, fov, far value *f*
and aspect value *a*. It starts with fov 90, *f* 100 and *a* 1 [X4]. Two managers matter: the **level's**,
which applies itself (below), and the **player's**, which does not; the player's update applies it [X4, G14,
G5].

**Update from a camera.** Each frame a manager is updated from one camera, whose own values are a fov and an
**aspect factor** (a camera starts with fov 90 and factor 1 [X4]) [X4]:

1. Targets: the camera's fov; the environment's far value *Z* (section 2.1, which includes `rs_vis_distance`);
   and (*H* div 2) / (*W* div 2) × the camera's aspect factor (halves rounded down to integers) [X0, X3, X4].
2. Smoothing: fov, *f* and *a* each move toward their target by *k* · (target − value) with
   *k* = clamp(10 × frame time in seconds, 0, 1) [X3, B5]. At 60 frames per second a change from 90 to
   67.5 is within 0.1 of the target after 0.5 s and within 0.001 after 1 s; a frame of 0.1 s or longer jumps to
   the target [M7].
3. **Effects:** then every active camera effect of the manager, from the most recently added to the first,
   may change position, direction, up, fov, *f* and *a*; finished effects are removed [X3]. Which effects
   change fov, *f* or *a* is section 3.5.
4. A manager that applies itself (the level's) then **applies** its values with near value 0.2, unless an
   effect asks it not to (the demo recorder can; **believed** that no game effect does) [X3, X4].

**Apply.** Applying builds the view matrix V from position, direction and up (section 1.1) and the projection P from
fov, *a*, a near value *n* given by the caller, and *f* (section 2); it hands fov and *a* to the renderer (they
are the fov and aspect its size thresholds and the first-person projection use) [X1, R5] (VS 4). In the engine the only
caller is step 4 (near 0.2) [X8]; the game calls it at five places, four with 0.2 and one with a computed value
(section 3.1) [G11]. The renderer never applies a camera [R5].

**The frame's matrices.** Each frame the device computes **VP** = V · P (and its inverse) from the matrices
last applied [X2]. Every frustum of section 5 that is built "from **VP**" uses this matrix.

### 1.1 The view matrix

From the eye position `e`, direction `d` and up vector `u` (for the first-person camera: WC section 3) the
view matrix is built as:

1. `u' = normalize(u − d·(u·d))` (up made perpendicular to the direction),
2. `r = u' × d`, i.e. `r = (u'.y·d.z − u'.z·d.y, u'.z·d.x − u'.x·d.z, u'.x·d.y − u'.y·d.x)`,
3. the view matrix (row-vector convention)

        V = [[ r.x,     u'.x,     d.x,    0 ],
             [ r.y,     u'.y,     d.y,    0 ],
             [ r.z,     u'.z,     d.z,    0 ],
             [ −e·r,    −e·u',    −e·d,   1 ]]

   so view-space x = (p − e)·r, y = (p − e)·u', z = (p − e)·d.

[V1] With the default user configuration (`cam_inert 0.` in `__support\userdocs\user.ltx` [V4]) the camera
position and direction are used as computed. A non-zero `cam_inert` (the engine's value when the line is
absent is 0.7 [B1]) blends each frame's position and direction with the previous frame's: `new = old ×
cam_inert + computed × (1 − cam_inert)`.

## 2. The projection

    P = [[ a/T, 0,   0,            0 ],
         [ 0,   1/T, 0,            0 ],
         [ 0,   0,   f/(f − n),    1 ],
         [ 0,   0,   −n·f/(f − n), 0 ]]

with *T* = tan(fov × π/180 / 2); all other entries are 0 [X1]. After division by w, x and y run from −1 to 1
(+x right, +y up) and z/w from 0 at view depth *n* to 1 at *f*. The horizontal field of view is
2 · atan(*T* / *a*): at fov 67.5, 83.40° on a 4:3 target and 99.82° on 16:9 [M1].

The aspect value's target is (*H* div 2) / (*W* div 2) × the camera's aspect factor (section 1); for the
first-person camera (factor 1) at fov 67.5: 1024×768 and 800×600 → *a* = 0.75, *a*/*T* = 1.122454,
horizontal 83.40°; 1920×1080 → *a* = 0.5625, *a*/*T* = 0.841841, horizontal 99.82°; 1280×1024 → *a* = 0.8,
horizontal 79.74°; 1/*T* = 1.496606 [V6].

### 2.1 Far value

The environment's far value, the target *Z* of section 1, is

    Z = ( (1 − t)·far₁ + t·far₂ + Σ wᵢ·farᵢ ) × rs_vis_distance / (1 + Σ wᵢ)

- `far₁`, `far₂`: the `far_plane` keys of the two hourly weather sections currently being blended, `t` the
  blend weight between them (how sections and `t` are chosen is for the weather specification).
- `rs_vis_distance`: console variable, range 0.4 … 1.5, default 1.0 [B4] (`rs_vis_distance 1.` in the
  default `user.ltx` [V4]).
- Environment modifiers (`levels\<level>\level.env_mod`, one 76-byte chunk per modifier [V2], 19 floats in
  the order: float3 position, radius, power, far plane, float3, float, float3, float3, float3): a modifier
  with centre `c`, radius `ρ`, power `π` and far plane `f` contributes when the distance `δ` from the camera
  position most recently applied to the view (in steady state, the eye) to `c` is less than `ρ`, with
  weight `w = (1 − δ/ρ) × π`, adding `w·f` to the sum and `w` to `Σ wᵢ`. The other fields modify fog and
  colours (environment specification).

### 2.2 Starting values

The smoothed values (section 1, step 2) start, when the player's camera is created at spawn, from fov 90,
aspect value 1 and far value 100 [X4, B5]. After a fraction of a second (or after
any frame of 0.1 s or longer) the values equal their targets [M7]; an implementation may use the targets
directly. The frame time of the first in-game frame is **unestablished**, so the exact first-frame values are
too.

### 2.3 Worked example: the start of a New Game

At the first in-game frame of a New Game the player's first-person camera has:

- eye `e` = (−246.72630, −23.263261, −134.43387) (PM section 7.1) [E1]; direction
  `d` = (0.984797, −0.0043165, 0.173655) and up (0.0042509, 0.999991, 0.00074959) (WC section 4) [W2];
- view matrix (section 1.1): `r` = (0.173657, 0.0, −0.984806), last row (−89.5456, 24.4126, 266.2201, 1)
  [V1];
- fov 67.5 (section 3.1) and aspect factor 1, so on a 4:3 target the values of section 2;
- far value (section 2.1): at the start (weather `default`, 05:30; level-loading specification section 2.6)
  both hourly sections have `far_plane = 350` (as every hourly section of the default weather [V3]), so the
  blended value is 350 whatever `t` is. Two modifiers of `l01_escape` apply at the start eye: chunk 1 (centre
  (−246.421, −23.526, −133.742), radius 4, power 2, far plane 150; δ = 0.800, w = 1.59990) and chunk 6
  (centre (−246.366, −24.036, −134.415), radius 4, power 2, far plane 300; δ = 0.853, w = 1.57357). Hence
  **Z = (350 + 712.056) / (1 + 3.17347) = 254.478 m** with `rs_vis_distance = 1` [V2];
- near value (section 3.1): no triangle of `level.cform` meets the cube of half-size 0.2 centred on the
  start eye [V5], so it is 0.2 unless a physically simulated object is that close (**unestablished**).

## 3. Field of view, aspect factor and near value of each camera

### 3.1 The player on foot

The player has a first-person camera and two others (orbiting views); one is **active** [G5].

**Field of view.** Each frame the player's fov is [G1]:

- if the active camera is the first-person one and the active item is a weapon that is **zoomed** (aiming):
  its current **zoom factor** × 0.75 — except while that weapon has a scope picture (made from
  `scope_texture` when a scope is fitted [G2]), its current mode uses it, and its **zoom progress** is below 1:
  then 67.5;
- otherwise 67.5 [B2], a fixed value of the game (not read from configuration; no console variable changes it).

That fov is given to the active camera and, when another camera is active, to the first-person camera as well
[G5]. The aspect factor of every player camera stays 1: no method of the game's camera kinds sets fov or
aspect factor [G12] (the game's own aspect factor values are those of section 3.6).

How the zoom progress advances is **unestablished**; it is a value of the weapon compared with 1 here and when
zooming out (below) [G1, G4]. The mouse factor F_view / 67.5 of PM uses this fov.

**Zoom factor of a weapon** [G2]:

- With a scope fitted (the weapon's `scope_status` is 1, permanent, or 2, attachable, with the scope attached):
  the **scope factor**: `scope_zoom_factor` of the weapon's section (permanent) or of the attached scope's
  section (attachable).
- Otherwise the **sight factor**: `ironsight_zoom_factor` of the weapon's section, or 50 when it has none; but
  for a weapon with `zoom_enabled` true and no fitted scope, the weapon's own `scope_zoom_factor`.
- A weapon with a grenade launcher, in launcher mode (launcher fitted): the sight factor, and no scope
  picture is shown [G2].

No section of the shipped configuration has `ironsight_zoom_factor` [C1]. The shipped `scope_zoom_factor`
values are 7.5, 10, 15, 20, 30, 33.3, 50, 55 (38 keys) [C1], giving aiming fovs of 5.625 … 41.25 [M3].

**Zooming.** Zooming in makes the weapon zoomed and sets its current factor to its zoom factor; zooming out
makes it unzoomed and sets the current factor to 67.5 [G3]. The binoculars (permanent scope, scope factor
*s* = `scope_zoom_factor` 7.5 [C2]) differ [G4]:

- two zoom-step actions change the current factor by −(67.5 − *s*) × 0.7 / 3 and +(67.5 − *s*) × 0.7 / 3,
  kept within *s* … 67.5 − 0.3 (67.5 − *s*); for the shipped binoculars steps of 14 between 7.5 and 49.5, fov
  5.625 … 37.125 [M4]. Which input gives which step is **unestablished**;
- zooming out remembers the current factor (only when the zoom progress is at least 1), zooming in restores
  it; the remembered factor starts at *s* [G4].

The RPG-7 kind of weapon reads `max_zoom_factor` (5 in `w_rpg7.ltx`, the only such key) into its scope factor
when loaded; its section also has `scope_status` 1 and `scope_zoom_factor` 30 [C1, C2]. Which of the two
applies while aiming is **unestablished**.

**Managers and near value.** When the player is the level's view [G5]:

- the level's manager is updated from the active camera (and applies itself, near 0.2, section 1);
- the player's manager is updated from the first-person camera, or from the active camera when a game flag
  is set (which setting that flag reflects is **unestablished**);
- then, if the active camera is the first-person one and the level's manager has no effect of kind 0 (the
  demo playback and the demo recorder are of kind 0 [X5]; whether any other effect is, is **unestablished**),
  the player's manager is applied with the **computed near value**: 0.2, or 0.01 when an axis-aligned box of
  half-size 0.2 centred on the eye meets a triangle of the level's static collision or, failing that, the
  bounds of a physically simulated object [G5, B3]. This is the one place that passes a
  computed value (VS 4) [G11].

So with the first-person camera the frame uses the player's manager and the computed near value; with an
orbiting camera, or during demo playback or recording, the level's manager and near 0.2 [G5, X3]. The demo
recorder can ask the level's manager not to apply itself; what is applied then is **unestablished**.

### 3.2 Vehicles and mounted weapons

In a vehicle, the vehicle's camera is updated with the player's fov of section 3.1 and the player's manager is
applied from it with near 0.2 [G1, G6, G13]. The two kinds of mounted weapon ignore that fov: their own update
moves their camera with fov 67.5 and applies the player's manager with near 0.2 [G6, G13]. Aspect factor 1
[X4, G12].

### 3.3 Paused game

While the game is paused, the player's manager is updated from the active camera (or the vehicle's) and
applied with near 0.2 each frame [G6].

### 3.4 Viewing through a creature

Two creature kinds have a camera step of their own that updates the level's manager with targets fov = the
creature's `eye_fov`, far = its `eye_range` (possibly reduced, by a rule **unestablished**) and aspect factor
**0.75** [G7]. **Disputed in review:** the rule that may reduce the far value is given both the far value
and the fov to change, so the fov target is not established to be exactly `eye_fov` either. The manager then
applies itself with near 0.2 (section 1). **Believed**: this step runs when
the level's view is that creature; when that happens in play is **unestablished**.
Shipped `eye_fov` values are 80 … 170 [C3].

### 3.5 Effects that change the field of view

Of the camera effects, three change fov, far or aspect value [G8, G9, X5]:

| Effect | Change |
|---|---|
| Psy attack of the controller creature (added to the player's manager) | fov = 170 − 160 · *t* / *T*, *t* the time since the effect started, *T* its duration (170 at the start, 90 half-way, 10 at the end [M6]) [G8] |
| An effect added while a creature is made invisible | fov moves at 80° per second toward 70 + 105 · clamp(*v* / 15, 0, 1), *v* a speed value of the creature [G8]; which creature and which speed are **unestablished** |
| Demo recorder, cube-map capture | aspect value *a* = 1 [X5]; its fov is **unestablished** |

Demo playback changes only position and orientation; the demo recorder otherwise too [X5]. The demo recorder's
level-map capture writes its own projection (VS 7) [X5]. Every other camera effect found in the game (nine
processing steps) changes only position and orientation [G9].

### 3.6 Summary

| View | fov | Aspect factor | Near |
|---|---|---|---|
| Player, first-person camera | 67.5; zoom factor × 0.75 when aiming (3.1) | 1 | 0.2, or 0.01 near collision (3.1) |
| Player, orbiting camera | 67.5 | 1 | 0.2 |
| Demo playback or recording | the active camera's (the effects do not change it, 3.5) | 1 (cube-map capture: *a* = 1) | 0.2 |
| Vehicle | player's fov (3.1) | 1 | 0.2 |
| Mounted weapon | 67.5 | 1 | 0.2 |
| Creature view (3.4) | `eye_fov` | 0.75 | 0.2 |

The far value is always the smoothed environment value *Z* (section 2.1) except in the creature view.

## 4. The first-person projection

The first-person models (the items drawn in the player's hands) are drawn with their own projection [R5]:

- fov = the applied fov × 0.45 (30.375 at fov 67.5 [M5]); 0.45 is fixed: no console command or code changes
  it [G10, R5];
- aspect value: the applied *a*;
- near 0.2; far *F* = the environment's far value *Z* (the target, not the smoothed value);
- the matrix has the form of section 2 with these values.

While they are drawn, **VP** is replaced by V · P_first-person, then restored [R5]. This happens near the start
of the frame, before the sky, and in the dynamic-light passes (DL 5.5) [R5, R2].

## 5. Frusta

### 5.1 A frustum from a matrix

A frustum built from a matrix M has up to six planes, chosen by bits; for clip coordinates (x, y, z, w) =
(p, 1) · M a point is inside a plane when [X6]:

| Bit | Inside when | Plane |
|---|---|---|
| 1 | x ≥ −w | left |
| 2 | x ≤ w | right |
| 4 | y ≤ w | top |
| 8 | y ≥ −w | bottom |
| 0x20 | z ≤ w | far |
| 0x10 | z ≥ −w | near side |

Planes are stored in the order 1, 2, 4, 8, 0x20, 0x10 and normalised to unit normals, so plane values are
distances; a point is outside a plane when its value is positive [X6]. For a projection of the form of
section 2, the near-side plane z ≥ −w lies at view depth *n* · *f* / (2*f* − *n*) (about *n* / 2), not at
*n*: 0.1001 for *n* 0.2, *f* 100 [M2].

### 5.2 The passes

| Pass | Matrix | Planes | |
|---|---|---|---|
| Camera pass (static geometry, the spatial query of objects, lights and glows, the start of the traversal) | **VP** | 0x2f: left, right, top, bottom, far; no near plane | [R1] (VS 4) |
| Sectors reached through portals | the eye and the clipped portal | a plane through the eye and each edge of the portal polygon, the portal's own plane (the eye on its outer side), and the far plane (0x20) of the pass's matrix | [X7, R1] (LV 9.2) |
| Detail objects | **VP** | 0x2f | [R1] (DO 5.2) |
| Occluder triangles (clipping) | **VP** | 0x10 only | [R1] (VS 3.2) |
| Point and spot light passes | the light's matrix M = V · Q_L of DL 5.3 | all six (0x3f) | [R2] |
| Projected object shadows | the shadow's matrix M of LS 5.3 | all six (0x3f) | [R3] (LS 6.2) |

- A portal polygon of more than 6 vertices is simplified before its frustum is built; how is
  **unestablished** (belongs to specification V8 with the traversal options) [X7].
- Light passes: the frustum is used for the light's traversal and draws (DL 5.4 – 5.5), and a sphere of radius
  2 around the camera tested against it decides whether the first-person models are lit (DL 5.5) [R2]. Its
  near side is z ≥ −w, which DL 5.3 gives as 0 ≤ z: for a point light (near 300 − *r*, far 300 + *r*) with
  *r* = 10 the frustum reaches view depth 272.42, 27.58 from the light rather than 10; for a spot light (near
  0.1) about 0.05 from the light [M2].
- The occluder triangles drawn into the depth grid are those meeting the pass's frustum (VS 3.2); the drawing
  step that starts a pass early also builds a 0x2f frustum of **VP** [R1].
- One more frustum has the four side planes only (0xf); it is built around level collision triangles near a
  point, with a random rotation (**believed**: the placement of surface marks, not a pass) [R4].
- Whether the first-person models are tested against any frustum in the camera pass is **unestablished**.

## 6. Unestablished

- How a weapon's zoom progress advances (3.1).
- The setting behind the game flag that makes the player's manager follow the active camera (3.1).
- Which input gives which binocular zoom step; which zoom factor the RPG-7 uses while aiming (3.1).
- Effects of kind 0 other than the demo playback and recorder; what is applied when the demo recorder asks the
  level's manager not to apply itself (3.1).
- When a creature's own camera step runs in play, and the rule that may reduce its far value (3.4).
- Which creature and which speed drive the invisibility effect's fov; the fov of the demo recorder's
  cube-map capture (3.5).
- How a portal polygon of more than 6 vertices is simplified (5.2).
- Whether the first-person models are frustum-tested in the camera pass (5.2).
- The frame time of the first in-game frame, so the exact first-frame fov, aspect and far values (2.2), and
  whether a physically simulated object is within 0.2 m of the start eye (near value 0.01 instead of 0.2 for
  that frame; no static collision triangle is) (2.3).
- Scripted camera effects at the start of the game (out of scope: game scripts and camera animations may
  move or animate the view after spawn).

## 7. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\XR_3DA.exe`,
  `bin\xrRender_R1.dll`, `bin\xrGame.dll`; archives `gamedata.db0` … `gamedata.dbd` (merged view).
- Data examined: `config\**\*.ltx` (zoom, scope and creature keys); for sections 1.1 and 2.1 – 2.3
  `config\weathers\*.ltx`, `levels\l01_escape\level.env_mod`, `levels\l01_escape\level.cform`,
  `spawns\all.spawn`, `config\creatures\actor.ltx` and the default `__support\userdocs\user.ltx`.
- Analysis: static analysis of the shipped binaries (headless decompilation and disassembly, read-only;
  constants read from the binaries' data); arithmetic. Sections 1.1, 2.1 – 2.3 (the view matrix, camera
  inertia, the far value, the starting values and the worked example) were established on 2026-11-23 in the
  investigation of the start view (static analysis of the view and projection construction, the far value
  and the smoothing; checked against the game data with independent throwaway parsers) and moved here on
  2026-11-27 without new investigation.

## Gaps found in review

- Camera effects change the manager's own stored fov, far and aspect values, so the next frame's smoothing
  (section 1, step 2) starts from the values the effects left, not from the smoothed ones; when an effect
  that changes fov ends, the fov returns to its target by the smoothing. The draft does not state this.
- The fov the invisibility effect starts from (section 3.5) is not stated.
