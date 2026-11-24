# Start position and first-person camera at New Game

Functional specification of how S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) places the player's
first-person camera when a New Game reaches its first in-game frame: which parts of `spawns\all.spawn` hold
the player's start position and orientation and how to read them (everything else in that file only to the
depth needed to skip it), how the stored angles become the camera's heading and pitch, the eye height above
the stored position, the world coordinate system shared by the level geometry and the camera, and the
camera's projection (field of view and its dependence on the screen's aspect ratio, near and far planes,
and where their values come from).

It resolves the camera items left **unestablished** in the level-loading scoping specification
(`docs/behavior/level-loading.md`, sections 2.5, 5.10, 5.11, 7 and 9) and is item 6 of its section 8.

Conventions:

- Paths are stored paths in the combined game-data tree (archive specification); `\` separates directories.
- Sizes are in bytes. Numbers are little-endian. `u8`/`u16`/`u32` are unsigned integers, `float` is an IEEE
  754 single, `float3` is three floats (x, y, z). `stringZ` is a zero-terminated byte string
  (Windows-1251).
- "Chunked" means the chunk container of the scoping specification, section 5.1 (`u32 id`, `u32 size`,
  payload; no padding).
- Angles are in radians unless stated. `sin`, `cos`, `tan` are the usual functions.
- Matrices are written in the row-vector convention: a point `p` (as a row `[x y z 1]`) is transformed as
  `p · M`; `M[r][c]` is row `r`, column `c`, both counted from 1.

`game.graph` is **not needed** for anything here. (The level to load, `levels\l01_escape\`, is derived in
the scoping specification, section 2.2; the actor's game-graph vertex is located in section 3.3 below only
for that cross-check.)

## 1. World coordinate system

One coordinate system is shared by the level geometry (`level`, `level.geom`, `level.cform`, …), the
objects in `spawns\all.spawn`, the environment modifiers (`level.env_mod`) and the camera:

- **Units:** metres. The actor's configured collision box is 1.8 units tall (section 4.2); at the start
  position, on the vertical line through the stored position, the collision surface of `level.cform` lies
  0.00013 units above the stored y (the floor; no collision triangle lies below the stored position on that
  line) and 3.18 units above it (the ceiling).
- **Up axis:** +y. The camera's up vector is +y for zero pitch (section 5); gravity-bound objects stand on
  surfaces below them in y.
- **Handedness: left-handed.** The camera's right vector is computed as `right = up × direction` (the
  ordinary cross product of the components, section 6) and the projection maps view-space +x to the right of
  the screen, +y to the top and +z (the viewing direction) into the screen (section 7). So, seen from a
  viewer at the origin looking along +z with +y up, +x is on the viewer's right. Seen from above (looking
  down −y), with +z pointing to the top of the page, +x points to the right.
- Coordinates in the spawn file are the level's own coordinates, the same as those of the level's geometry
  files (no per-level offset is applied; the `offset` values of `game.graph`'s level table / `game_levels.ltx`
  are not involved).

A right-handed consumer can convert by negating one axis (e.g. z) of every position and direction; this
reverses the winding of every triangle. Which winding the original renderer culls is **unestablished**
here (it belongs to the geometry/rendering specifications).

## 2. `spawns\all.spawn`: locating the actor's record

The spawn file is `spawns\all.spawn` (dbb, 4,474,073 bytes; scoping specification section 2.2). It is
chunked. Only top-level chunk 1 is needed; chunks 0 (header), 2 and 3 can be skipped. (Chunk 0 is 44 bytes
and begins with `u32 8`; the scoping specification, section 5.10, describes it.)

Chunk 1 is itself chunked:

| Sub-chunk id | Size | Content |
|---|---|---|
| 0 | 4 | `u32` object count (8,641) |
| 1 | 2,552,742 | the objects: one sub-chunk per object, ids 0, 1, 2, … in order (object index) |
| 2 | 0 | empty |

Each object's sub-chunk (id = object index `i`) is chunked and contains exactly two chunks:

| Chunk id | Size | Content |
|---|---|---|
| 0 | 2 | `u16`, equal to `i` in every object |
| 1 | varies | chunked; exactly two chunks: id 0 = **spawn packet**, id 1 = update packet |

Both packets are stored the same way: `u16 n`, then `n` bytes of packet data (`n + 2` equals the chunk
size in all 17,282 packets). The update packet is not needed.

**The actor** is the one object whose spawn packet's section name (section 3.1) is `actor`. Exactly one
such object exists; it is object index 63 (object name `level_prefix_actor_0001`).

## 3. Spawn packet layout

### 3.1 Common part (every object)

Read in order from the start of the packet data:

| Field | Type | Notes |
|---|---|---|
| message type | `u16` | must be 1 (all 8,641 records) |
| section name | `stringZ` | name of the object's configuration section (an LTX section in the `system.ltx` tree); `actor` for the actor |
| object name | `stringZ` | e.g. `level_prefix_actor_0001` |
| two bytes | `u8`, `u8` | meaning **unestablished**; `0x00, 0xFE` in all records |
| position | `float3` | world position (section 1) |
| angles | `float3` | orientation angles `a0, a1, a2` (section 5 gives the actor's use) |
| four values | 4 × `u16` | meaning **unestablished**; `0, 0xFFFF, 0xFFFF, 0xFFFF` in all records |
| flags | `u16` | bit 0x20: a version field follows |
| version | `u16` | present only if flags bit 0x20 is set (it is set in all records) |
| … | | version-dependent fields, below |
| data size | `u16` | `s` |
| object-specific data | `s − 2` bytes | the data size counts its own two bytes; the object-specific data ends exactly at the end of the packet in all records |

Version-dependent fields between the version and the data size, in this order:

1. If version > 69: one `u16` (meaning **unestablished**; 6 in all records).
2. If version ≥ 71: a block of client data: its length is a `u8` if version < 94, else a `u16`; that many
   bytes follow (length 0 in all records).
3. If version > 79: one `u16`, equal to the object index `i` in all records.

A record whose version is 0, or whose flags lack bit 0x20, is invalid. Versions below 112 have further
legacy fields between items 3 and the data size that are not described here; all records of the shipped
file have version 118, so a reader may reject versions below 112.

To skip an object, skip its whole chunk (chunk sizes). To skip the object-specific data within a packet,
skip `s − 2` bytes.

### 3.2 The actor's packet

For the actor, the common part reads: message type 1, section `actor`, name `level_prefix_actor_0001`, bytes
`00 FE`, position and angles as in section 4.1, the four values `0, 0xFFFF, 0xFFFF, 0xFFFF`, flags `0x0029`,
version 118, `u16 6`, client-data length `u16 0`, `u16 63`, data size 222. The spawn packet is 298 bytes
(its chunk 300).

### 3.3 Object-specific data (only as far as the actor's first fields)

Not needed for the camera. For a cross-check of the level choice, the actor's object-specific data begins
with:

| Field | Type | Actor's value |
|---|---|---|
| game-graph vertex | `u16` | 4 |
| float | `float` | 0.7 (`33 33 33 3F`) |
| u32 | `u32` | 1 |
| level-graph node | `u32` | 11,713 |
| flags | `u32` | `0xFFFFFFBF` |
| custom data | `stringZ` | text beginning `[dont_spawn_character_supplies]` |
| two values | 2 × `u32` | `0xFFFFFFFF`, `0xFFFFFFFF` |
| visual | `stringZ` | `actors\hero\stalker_novice` |

Further fields are out of scope. Vertex 4 is resolved against `game.graph` as in the scoping
specification, sections 2.2 and 5.11.

## 4. Start position and eye height

### 4.1 Stored values (actor, object index 63)

| Field | Value | Bytes |
|---|---|---|
| position x | −246.72630 | `EF B9 76 C3` |
| position y | −24.793261 | `99 58 C6 C1` |
| position z | −134.43387 | `12 6F 06 C3` |
| angle a0 | 0.0043165367 | `BC 71 8D 3B` |
| angle a1 | −1.3962544 | `77 B8 B2 BF` |
| angle a2 | −0.00068671 | `5D 04 34 BA` |

The actor is placed at the stored position; it is the position of its feet (the first collision surface of
`level.cform` below the eye, on the vertical line through the stored position, lies 0.00013 m above the
stored y, i.e. 1.52987 m below the eye; the stored position is that close to the floor).

### 4.2 Eye height

The eye is directly above the actor's position:

    eye = position + (0, h, 0),   h = H × camera_height_factor

- `camera_height_factor` is read from the actor's configuration section, i.e. the section named in the
  record (`actor`, in `config\creatures\actor.ltx`, dbb): `camera_height_factor = 0.85`.
- `H` is the height of the actor's current collision box. The actor's section defines three boxes,
  `ph_box0_*`, `ph_box1_*`, `ph_box2_*`, each by `ph_boxN_center` and `ph_boxN_size` (float3); the box spans
  `center − size` … `center + size`, so its height is `2 × size.y`. At spawn the box with index 0 is made
  current (boxes 1 and 2 are only selected later, by the actor's movement), so `H = 2 × ph_box0_size.y`.
- Values: `ph_box0_center = 0.0, 0.9, 0.0`, `ph_box0_size = 0.35, 0.9, 0.35` → `H = 1.8`, `h = 1.53`.

So the start eye position is **(−246.72630, −23.263261, −134.43387)**.

The vertical offset is not rotated by the actor's heading (the actor's heading turns about the y axis only).
Two refinements do not apply at the start and are listed for completeness: when the actor rises (e.g. on
stairs) the camera's height follows with a lag (at most 0.2 m behind, catching up at 1.5 m/s); a leaning
offset is zero at spawn.

## 5. Orientation: stored angles → camera heading and pitch

The first-person camera keeps three angles, **yaw**, **pitch** and **roll**. At spawn they are set from the
actor's stored angles as:

    yaw   = −a1
    pitch =  a0
    roll  =  0        (a2 is not used by the camera)

Actor: yaw = +1.3962544 (79.999°), pitch = +0.0043165 (0.247°), roll = 0.

(The first-person camera's configuration section `[actor_firsteye_cam]` in `config\system.ltx` limits pitch
to `lim_pitch = -1.5, 1.5`; `lim_yaw = 0, 0` means yaw is unlimited. The start pitch is within the limit.)

**Camera basis from the angles.** The camera's orientation is the rotation

    R = Ry(yaw) · Rx(pitch) · Rz(roll)

applied to column vectors, where `Ry`, `Rx`, `Rz` are the standard rotations about the y, x and z axes:

    Ry(t) = [[ cos t, 0, sin t], [0, 1, 0], [−sin t, 0, cos t]]
    Rx(t) = [[1, 0, 0], [0, cos t, −sin t], [0, sin t, cos t]]
    Rz(t) = [[cos t, −sin t, 0], [sin t, cos t, 0], [0, 0, 1]]

The camera's viewing direction is `R · (0, 0, 1)`, its up vector `R · (0, 1, 0)`. With roll 0:

    direction = ( cos(pitch)·sin(yaw), −sin(pitch), cos(pitch)·cos(yaw) )
    up        = ( sin(pitch)·sin(yaw),  cos(pitch), sin(pitch)·cos(yaw) )

Consequences: yaw 0, pitch 0 looks along +z; positive yaw turns toward +x (clockwise seen from above, i.e.
to the viewer's right); **positive pitch looks down**.

Actor at start:

    direction = (0.984797, −0.0043165, 0.173655)
    up        = (0.0042509, 0.999991, 0.00074959)

Consistency evidence: the trader object (`escape_trader`, section `m_trader`, object index 84, position
(−244.693, −24.958, −133.960)) stands 2.09 m (horizontally) in front of the eye, 3.1° off the horizontal
viewing direction, i.e. the player starts facing the trader.

## 6. View transform

From the eye position `e`, direction `d` and up vector `u` (section 5) the view matrix is built as:

1. `u' = normalize(u − d·(u·d))` (up made perpendicular to the direction),
2. `r = u' × d`, i.e. `r = (u'.y·d.z − u'.z·d.y, u'.z·d.x − u'.x·d.z, u'.x·d.y − u'.y·d.x)`,
3. the view matrix (row-vector convention)

        V = [[ r.x,     u'.x,     d.x,    0 ],
             [ r.y,     u'.y,     d.y,    0 ],
             [ r.z,     u'.z,     d.z,    0 ],
             [ −e·r,    −e·u',    −e·d,   1 ]]

   so view-space x = (p − e)·r, y = (p − e)·u', z = (p − e)·d.

Actor at start: `r = (0.173657, 0.0, −0.984806)`, last row `(−89.5456, 24.4126, 266.2201, 1)`.

With the default user configuration (`cam_inert 0.` in `__support\userdocs\user.ltx`) the camera position
and direction are used as computed. A non-zero `cam_inert` (the engine's value when the line is absent is
0.7) blends each frame's position and direction with the previous frame's: `new = old × cam_inert + computed
× (1 − cam_inert)`.

## 7. Projection

The projection matrix (row-vector convention, view space as in section 6) is

    P = [[ A/T, 0,   0,              0 ],
         [ 0,   1/T, 0,              0 ],
         [ 0,   0,   Z/(Z − N),      1 ],
         [ 0,   0,   −N·Z/(Z − N),   0 ]]

with `T = tan(F/2)`, where:

- **F — vertical field of view: 67.5°.** It is a fixed value of the game (not read from configuration; no
  console variable changes it). It applies whenever the player is not aiming a zoomable weapon, so always at
  the start.
- **A — aspect factor** = `(screen height ÷ 2) / (screen width ÷ 2)`, each half rounded down to an integer
  (pixels of the render target), times the camera's own aspect value, which is 1.0 for the first-person
  camera. The horizontal field of view therefore follows from the vertical one:
  `horizontal FOV = 2·atan(T / A)`. Examples: 1024×768 and 800×600 → A = 0.75, `A/T = 1.122454`, horizontal
  83.40°; 1920×1080 → A = 0.5625, `A/T = 0.841841`, horizontal 99.82°; 1280×1024 → A = 0.8, horizontal
  79.74°. `1/T = 1.496606`.
- **N — near plane: 0.2 m.** It is 0.01 m instead in a frame in which the axis-aligned cube of half-size
  0.2 m centred on the eye intersects a triangle of the level's static collision (`level.cform`) or the
  bounds of a physically simulated object.
- **Z — far plane**: the current environment's far plane (weather), section 7.1.

After division by w (= view-space z), x and y are in −1 … 1 (+x right, +y up) and depth `z/w` runs from 0 at
the near plane to 1 at the far plane.

### 7.1 Far plane

    Z = ( (1 − t)·far₁ + t·far₂ + Σ wᵢ·farᵢ ) × rs_vis_distance / (1 + Σ wᵢ)

- `far₁`, `far₂`: the `far_plane` keys of the two hourly weather sections currently being blended, `t` the
  blend weight between them (how sections and `t` are chosen is for the weather specification). At the
  start (weather `default`, 05:30; scoping specification section 2.6) both sections have
  `far_plane = 350`, so the blended value is 350 whatever `t` is.
- `rs_vis_distance`: console variable, range 0.4 … 1.5, default 1.0 (`rs_vis_distance 1.` in the default
  `user.ltx`).
- Environment modifiers (`levels\<level>\level.env_mod`, one 76-byte chunk per modifier, 19 floats in the
  order: float3 position, radius, power, far plane, float3, float, float3, float3, float3): a modifier with
  centre `c`, radius `ρ`, power `π` and far plane `f` contributes when the distance `δ` from the camera
  position most recently applied to the view (in steady state, the eye) to `c` is less than `ρ`, with
  weight `w = (1 − δ/ρ) × π`, adding `w·f` to the sum and `w` to `Σ wᵢ`. The other fields modify fog and
  colours (environment specification).

At the start eye position two modifiers of `l01_escape` apply: chunk 1 (centre (−246.421, −23.526,
−133.742), radius 4, power 2, far plane 150; δ = 0.800, w = 1.59990) and chunk 6 (centre (−246.366,
−24.036, −134.415), radius 4, power 2, far plane 300; δ = 0.853, w = 1.57357). Hence
**Z = (350 + 712.056) / (1 + 3.17347) = 254.478 m** with `rs_vis_distance = 1`.

### 7.2 Smoothing of field of view, aspect and far plane

The values used in P are not set directly but follow their targets (F, A and Z above) each frame:

    value ← value + k·(target − value),   k = clamp(10 × frame time in seconds, 0, 1)

starting, when the player's camera is created at spawn, from 90° (field of view), 1.0 (aspect factor) and
100 m (far plane). After a fraction of a second (or after any frame of 0.1 s or longer) the values equal
their targets; an implementation may use the targets directly. The frame time of the first in-game frame is
**unestablished**, so the exact first-frame values are too.

## 8. Verification facts

`spawns\all.spawn` (dbb, 4,474,073 bytes):

- Top-level chunks: ids 0, 1, 2, 3 of sizes 44, 2,552,770, 850,944, 1,070,283. Chunk 1's sub-chunks: ids
  0, 1, 2 of sizes 4, 2,552,742, 0; sub-chunk 0 holds 8,641.
- 8,641 object sub-chunks, ids 0 … 8,640 in order; each has chunk 0 (2 bytes, value = id) and chunk 1 with
  exactly two chunks (ids 0, 1); in all 17,282 packets the `u16` length + 2 = chunk size.
- In all 8,641 spawn packets: message type 1; the two bytes after the names are `00 FE`; the four `u16`
  values are `0, 0xFFFF, 0xFFFF, 0xFFFF`; flags have bit 0x20 (values: `0x0021` × 8,629, `0x0023` × 11,
  `0x0029` × 1 (the actor)); version 118; the following `u16` is 6; client-data length 0; the next `u16`
  equals the object index; the object-specific data ends exactly at the end of the packet.
- Exactly one record has section `actor`: index 63, name `level_prefix_actor_0001`, values of section 4.1.
- Sums over all 8,641 records (in file order, double precision): positions (592,183.146, −29,943.946,
  71,704.991); angles (537.2033, 259.2581, −902.9700).

Derived values for the start: eye (−246.72630, −23.263261, −134.43387); yaw 1.3962544, pitch 0.0043165;
direction (0.984797, −0.0043165, 0.173655); far plane 254.478 m (default `user.ltx`, weather `default` at
05:30).

## 9. Unestablished

- Meanings of the two bytes after the object name, the four `u16` values after the angles, the `u16` after
  the version, and the `u16` equal to the object index (beyond that equality); the legacy fields of versions
  below 112.
- The triangle winding (culling) convention of the level geometry.
- The frame time of the first in-game frame (exact first-frame field of view, aspect and far plane, section
  7.2) and whether a dynamically simulated object is within 0.2 m of the start eye (near plane 0.01 m instead
  of 0.2 m for that frame; no static collision triangle is).
- Scripted camera effects at the start of the game (out of scope: game scripts and camera animations may
  move or animate the view after spawn).

## 10. Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrRender_R1.dll`; default
  `__support\userdocs\user.ltx`.
- Data examined: `spawns\all.spawn`, `config\creatures\actor.ltx`, `config\system.ltx`,
  `config\weathers\*.ltx`, `levels\l01_escape\level.env_mod`, `levels\l01_escape\level.cform`.
- Analysis: static analysis of the binaries (disassembly and decompilation with Ghidra) to establish the
  reading of spawn records, the actor's eye height and initial camera angles, the camera basis, the view and
  projection construction, the field-of-view constant, near and far planes and their smoothing; all facts
  were checked against the game data with independent throwaway parsers (record layout invariants over all
  8,641 objects, the actor's values, the floor below the start position in `level.cform`, the environment
  modifiers at the start eye, and the trader's position relative to the view direction).
