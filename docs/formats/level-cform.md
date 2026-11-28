# `level.cform`, game materials and the player's collision with the level

Functional specification of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition):

1. the static collision mesh `levels\<level>\level.cform`: every field;
2. the game material library `gamemtl.xr` as far as collision needs it, how the triangles' material ids
   map to it, and which material properties decide whether the player is blocked, walks on a surface or
   passes through it;
3. the collision body of the player (the on-foot actor) and the observable rules by which its movement is
   resolved against the level geometry: shape and dimensions per posture, ground detection, walls,
   slopes, steps and the posture-change test.

Movement speeds, accelerations, jumping, gravity, postures and controls are the subject of the
player-movement specification (`player-movement`) and are referred to, not repeated. Coordinates, units
(metres) and the +y up axis are those of the world-coordinates specification (`world-coordinates`, section 1).
The original resolves the player's motion with a general rigid-body simulation (a public physics library
with contact constraints); this document gives the body, the contact properties and the extra rules the
game adds, with their numbers, and states which overall outcomes (step height, steepest climbable slope)
could not be established as single numbers.

Conventions:

- Integers are little-endian and unsigned unless stated. `u16`/`u32` unsigned integers, `float` an IEEE 754
  single, `float3` three floats (x, y, z). Sizes are in bytes. `stringZ` is a zero-terminated byte string
  (Windows-1251).
- Paths are stored paths in the combined game-data tree (archive specification, `db-archive`); `\`
  separates directories.
- "Chunked" means the chunk container of the chunk-container specification (`chunks`): `u32 id`,
  `u32 size`, payload, no padding; "chunk n" is the chunk with id n.
- Vector operations are the ordinary component formulas: `a × b = (a.y·b.z − a.z·b.y, a.z·b.x − a.x·b.z,
  a.x·b.y − a.y·b.x)`, `a·b` the dot product.
- "Established" facts come from static analysis of the shipped binaries and are checked against the game
  data where the data can show them; facts marked **unestablished** could not be determined.

## 1. Files

| Path | Format | Role |
|---|---|---|
| `levels\<level>\level.cform` | own, header + tables, **not** chunked | the level's static collision mesh |
| `gamemtl.xr` (root of the game-data tree, `$game_data$`) | chunked | the game material library |

`level.cform` must exist (a level directory is offered as a level only if `level`, `level.ltx`,
`level.geom` and `level.cform` exist; scoping specification `level-loading`, section 2.2) and is read when
the level loads (`st_loading_cform` stage). Its bounding box sizes the engine's spatial structures.

`gamemtl.xr` is loaded once at game start. It is stored identically (same size 310,494 and CRC-32
`ca9561b8`) in `gamedata.db0` and `gamedata.dba`. If it is missing the game logs `! Can't find game
material file: ` with the path and has no materials (every level load then fails, section 3.3).

## 2. `level.cform`

### 2.1 Layout

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `u32` version; must be **4** (any other value is a fatal error) |
| 4 | 4 | `u32` vertex count `V` |
| 8 | 4 | `u32` triangle count `T` |
| 12 | 12 | `float3` bounding box minimum |
| 24 | 12 | `float3` bounding box maximum |
| 36 | 12·V | vertices: `V` × `float3` position |
| 36 + 12·V | 16·T | triangles: `T` × 16-byte record (section 2.2) |

The file is exactly `36 + 12·V + 16·T` bytes; nothing follows the triangles.

### 2.2 Triangle record

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `u32` vertex index `i0` |
| 4 | 4 | `u32` vertex index `i1` |
| 8 | 4 | `u32` vertex index `i2` |
| 12 | 4 | `u32` attribute word `w` |

The attribute word:

| Bits | Meaning |
|---|---|
| 0–13 (`w & 0x3FFF`) | **material id**: the id of a game material (section 3.2), not a list position |
| 14 | ignored in the file; replaced at load (section 3.3) |
| 15 | ignored in the file; replaced at load (section 3.3) |
| 16–31 (`w >> 16`) | **sector index**: index of a sector of the level (the sub-chunks of `level` chunk 8; `level` format specification, `docs/formats/level.md`, section 6). In every shipped level the largest value + 1 equals that level's sector count. Not used by collision |

Bits 14 and 15 are constant within each shipped file but differ between files (both 0, both 1, or one of
each); they carry no meaning in the file.

### 2.3 Triangle orientation (front side)

Each triangle's **front normal** is

    n = normalize( (v[i1] − v[i0]) × (v[i2] − v[i1]) )

The triangle collides only with shapes on its front side (section 4.1). Evidence: the floor triangle under
the start position has n.y = +0.99999 and the ceiling above it n.y = −0.99341 (section 7.1), i.e. both
front sides face the player. Vegetation and other thin surfaces are stored as two triangles with opposite
orientation (e.g. `l01_escape` triangles 906,292 and 906,293). In `l01_escape`, 706,926 triangles have
n.y > 0 and 550,975 have n.y < 0.

### 2.4 Degenerate data

Triangles with two equal vertex indices occur (4 in `l01_escape`, up to 2,362 in a level); they have zero
area. A reader must accept them; how the original's contact generation treats them is **unestablished**
(skipping them is the natural choice).

## 3. Game materials (`gamemtl.xr`)

### 3.1 File layout

Top-level chunks (in this order in the shipped file):

| Chunk | Size | Content |
|---|---|---|
| 0x1000 | 2 | `u16` library version; must be **1** (otherwise the game logs that the library has an invalid version and loads no materials) |
| 0x1001 | 8 | two `u32`: the next free material id (94) and the next free material-pair id (1,398); not needed |
| 0x1002 | 8,990 | the **materials**: one sub-chunk per material (sub-chunk ids 0, 1, 2, … in file order; 57 records) |
| 0x1003 | 301,462 | material **pairs** (1,247 sub-chunks): sounds, particles and effects for contacts between two materials. Not needed for collision; skip |

The materials are read in file order (sub-chunk iteration order); a material's **index** is its position
in that order, counting every record (index 0 is the first record).

Each material sub-chunk is itself chunked:

| Chunk | Required | Content |
|---|---|---|
| 0x1000 | yes | `u32` material **id**, then `stringZ` **name** (e.g. `materials\concrete`) |
| 0x1005 | no | `stringZ` description (Russian text; not needed); empty when absent |
| 0x1001 | yes | `u32` **flags** (section 3.2) |
| 0x1002 | yes | five `float`: **friction**, **damping**, **spring**, **bounce start speed**, **bounce** (section 4.2) |
| 0x1003 | yes | four `float`: factor 0 (meaning **unestablished**; not used by the collision code examined), factor 1 = **impact factor** (section 4.4), factors 2, 3 (meaning **unestablished**; not used by the collision code examined) |
| 0x1004 | no | one `float`, the **liquid factor**, 1.0 when absent: a body touching a **liquid** triangle (flag 0x80000000, section 3.2) gets the damping `1 − liquid factor`; 1.0 in all records except `materials\water` (0.8, damping 0.2) |
| 0x1006 | no | one `float`: **damage rate** of a deadly material (section 4.4), 0 when absent; 0 in all records except `materials\death` (30) and `materials\earth_death` (150) |

All 57 shipped records contain all seven chunks. A missing required chunk is a fatal error. A record starts
from these values before its chunks are read (so they are the values of absent optional chunks): id
0xFFFFFFFF, name `unknown`, empty description, flags 0, friction 1, damping 1, spring 1, bounce start speed
0, bounce 0.1, factors 0, 1, 0, 0, chunk-0x1004 value 1, damage rate 0.

### 3.2 Material flags

These are the flag bits tested by the collision code examined for this specification (load-time mapping,
contact generation against the level mesh, the player's contact handling and step probe). No other bit is
tested there; whether other bits are used elsewhere (sounds, effects, other objects) is **unestablished**.

| Bit | Name used here | Effect | Shipped materials with the bit |
|---|---|---|---|
| 0x80 | **pass-through** | no contact force between the level mesh and any body: shapes pass through such triangles (section 4.1). Its impact and damage rules still apply (section 4.4) | `materials\water`, `materials\bush`, `materials\fake`, `materials\death` |
| 0x1000 | **actor obstacle** | contacts with the player are kept even if the material is pass-through: the player is blocked, other bodies pass | `materials\fake` (invisible walls; 1,516 triangles in `l01_escape`) |
| 0x100 | **dynamic** | material of movable objects and creatures; must **not** occur in `level.cform` (section 3.3) | 29 materials (`creatures\*`, `objects\*`, `default_object`) |
| 0x4 | **bouncy** | contact bounce applies when both materials have it (section 4.2) | 12 materials, e.g. `materials\wood`, `materials\metal` |
| 0x10000000 | **deadly** | standing on / in it damages the player at the material's damage rate (section 4.4) | `materials\death` (also pass-through), `materials\earth_death` (solid) |
| 0x400 | — | copied into triangle bit 14 at load (section 3.3); not used by collision | `materials\bush`, `materials\fake`, `materials\fake_ladders`, `materials\death` |
| 0x800 | — | copied into triangle bit 15 at load; not used by collision | the four above and `materials\setka_rabica` |
| 0x80000000 | **liquid** | when a body touches such a triangle, the body is given the damping `1 − liquid factor` (chunk 0x1004), the largest such value being kept per body; applied only under conditions on the touching body's state (bit 0x200 of the material also takes part; no shipped liquid material has it) | `materials\water` (flags 0x80000082; id 11, used in shipped levels) |

Every other surface material is **solid**: it blocks the player and everything else. The bit 0x20 is set only
on `materials\fake_ladders`; ladder behaviour is out of scope (**unestablished** here). `materials\water`
is pass-through for collision; whether the liquid damping reaches the player, and any swimming or other
liquid behaviour, are out of scope (**unestablished** here).

### 3.3 Mapping triangle materials at load

When a level loads, each triangle's material id (`w & 0x3FFF`) is looked up among the materials whose
flags lack **dynamic** (0x100), by id. If it is not found the game stops with the fatal error
`Game material '<id>' not found`. Otherwise, in the loaded copy of the triangle:

- bits 0–13 become the material's **index** (section 3.1) instead of its id;
- bit 14 becomes 1 if the material has flag 0x400, else 0;
- bit 15 becomes 1 if the material has flag 0x800, else 0;
- bits 16–31 are kept.

All collision rules below use the material reached this way. In every shipped level every material id used
is present and not dynamic. Ids and indices differ: e.g. id 81 (`materials\cloth`) is index 46, id 80
(`materials\fake`) index 45, id 39 (`materials\tree_trunk`) index 22.

## 4. Contacts with the level geometry

### 4.1 Which contacts exist

- Contacts are generated between the player's collision shapes (section 5.1) and the triangles of
  `level.cform` near them. A shape collides with a triangle only from the triangle's **front side**
  (section 2.3): a shape whose centre is behind the triangle's plane, and was behind it at the end of the
  previous step, gets no contact from it; a shape whose centre crossed from the front to the back during
  the step does collide.
- A contact with a **pass-through** triangle exerts no force, except for the player when the material is
  an **actor obstacle** (section 3.2).
- Each contact records the triangle's material (its index, section 3.3).

### 4.2 Contact properties

For a contact between materials `A` and `B` (for the player's shapes: the material assigned to the
player's body, section 5.1, and the triangle's material), with physics step `h` (section 6):

- **friction coefficient** `μ = A.friction × B.friction`;
- **softness** (spring–damper): `k = h × A.spring × B.spring × S`, `d = A.damping × B.damping × D`; the
  contact's error-reduction parameter is `k / (k + d)` and its constraint-force mixing `1 / (k + d)` (the
  terms of the original's physics library: the fraction of penetration corrected per step and the
  contact's compliance). `S` and `D` are the world's spring and damping, derived from two stored constants
  `K = 24,000,002` and `D₀ = 400,000`: the world's error-reduction parameter is `E = K·h / (K·h + D₀)` and
  its constraint-force mixing `C = 1 / (K·h + D₀)`, and `S = E / (h·C)` = 24,000,002, `D = (1 − E) / C` =
  400,000 (single precision, any `h`). With all factors 1 and `h = 0.01`: error-reduction 0.375 and
  constraint-force mixing 1.5625 × 10⁻⁶.
- **bounce**: only if both materials are **bouncy** (0x4): bounce coefficient `min(A.bounce, B.bounce)`,
  applied above an approach speed of `max(A.bounce start speed, B.bounce start speed)`. Otherwise no bounce.
- Friction acts in two tangent directions independently (a friction pyramid).

The player's body adjusts its own contacts further (section 5.4).

### 4.3 Values of the solid surfaces in `l01_escape`

All 22 materials used by `l01_escape` have friction 1.0 and spring 1.0 except `materials\concrete`,
`materials\asphalt` (spring 0.8), `materials\flooring_tile` (damping 0.7, spring 0.7), `materials\fake`
(spring 0.7), `materials\bush` (friction 0.0); damping is 1.0 except `materials\flooring_tile` (0.7).
The `default` material (id 0, index 0): friction 1, damping 1, spring 1, bounce start speed 1.5, bounce 1.

### 4.4 Impact and deadly ground (inputs to damage)

The damage itself belongs to the health specification; what collision provides:

- **Impact speed** of a step: the largest, over the player's contacts, of `factor1 × s`, where for a solid
  material `s = max(|v·n|, friction × |v − (v·n)·n|)` (`v` the body's velocity, `n` the contact normal), and
  for a pass-through material `s = |v|`; `factor1` and `friction` are the touched material's (the other
  side's, not the player's). The player's movement code turns an impact speed `S` above
  `ph_crash_speed_min` of the actor's configuration section (`config\creatures\actor.ltx`: 14) into the
  damage fraction `(S − 14) / (ph_crash_speed_max − 14)` (`ph_crash_speed_max` = 30); that it is this
  impact speed that reaches that formula is believed (the hand-over passes through an indirection that was
  not followed in full).
- **Ground material**: the material most recently touched by the player's contacts, a pass-through material
  taking precedence over a solid one. If it is **deadly**, the damage fraction grows by
  `damage rate × frame time (s)` each frame (so standing in `materials\death`, which is pass-through, or on
  `materials\earth_death` hurts).

## 5. The player's collision body

### 5.1 Shapes

The player is one rigid body that never rotates (its rotation is reset to identity and its angular velocity
to zero every step) carrying four shapes. They are built from the actor's current box (section 5.2) of
extents `E = 2 × ph_boxN_size` (`ph_boxN_size` from the actor's section in `config\creatures\actor.ltx`):

    r = 0.5 × min(E.x, E.z)
    h = E.y − 2·r          (if negative: 0.01)
    s = 0.5 × r × sqrt(0.3055556)        (= 0.276385·r)

All positions are relative to the **body centre**, which lies `r` above the actor's position (the feet):
setting the actor's position `p` places the body centre at `p + (0, r, 0)`, and the actor's position is
read back as body centre − `(0, r, 0)`.

| Shape | Kind | Size | Centre (relative to body centre) | Collides with the level |
|---|---|---|---|---|
| bottom sphere | sphere | radius `r` | (0, 0, 0) | yes |
| torso cylinder | flat-ended cylinder, axis +y | radius `0.8333333·r`, length `h + s` | (0, `0.5·h − s`, 0) | yes |
| head sphere | sphere | radius `0.8333333·r` | (0, `h`, 0) | yes |
| outer sphere | sphere | radius `2·r` | (0, `2.5·r`, 0) | **no** (its use is out of scope) |

The body's mass is `ph_mass` (80). The material assigned to the four shapes is material index 0
(`default`) when the body is created; whether the actor is later given another material is
**unestablished** (the actor's own material, `creatures\actor`, has friction 0.8).

### 5.2 Boxes and postures

The actor section defines three boxes; the actor's configuration (`config\creatures\actor.ltx`, section
`actor`) gives:

| Box | `ph_boxN_center` | `ph_boxN_size` | Height `E.y` | Used when |
|---|---|---|---|---|
| 0 | 0, 0.9, 0 | 0.35, 0.9, 0.35 | 1.8 | standing (and at spawn) |
| 1 | 0, 0.75, 0 | 0.35, 0.75, 0.35 | 1.5 | crouching while the run/walk state is "run" |
| 2 | 0, 0.6, 0 | 0.35, 0.6, 0.35 | 1.2 | crouching while the run/walk state is "walk" (low crouch) |

The run/walk state is the console flag `g_always_run` inverted while the walk key is held (controls:
`player-movement`). Further posture conditions that select box 1 or 2 are for `player-movement`; the
boxes' centres are not used by the collision body (only the sizes are).

Resulting shapes (`r = 0.35` for all three boxes; heights above the feet):

| Box | `h` | torso cylinder spans | head sphere centre | top of the head sphere |
|---|---|---|---|---|
| 0 | 1.1 | 0.204898 … 1.401633 | 1.45 | 1.741667 |
| 1 | 0.8 | 0.204898 … 1.101633 | 1.15 | 1.441667 |
| 2 | 0.5 | 0.204898 … 0.801633 | 0.85 | 1.141667 |

The bottom sphere spans 0 … 0.7 above the feet in every box. Changing the box does not move the actor's
position (the feet stay where they are; `r` does not change).

### 5.3 Posture change test (standing up under a low ceiling)

A change to a **taller** box (crouch → stand, low crouch → crouch) is made only if the taller body fits:
the game grows the shapes from the current to the target box in steps while letting the body settle
against the level, and refuses the change if penetration remains. When refused, the actor keeps the lower
posture (and keeps trying while the request lasts); after a refusal the same target is not retried within
500 ms unless the body has moved at least 0.05 m. Changes between the two crouch boxes go through the same
test; a change to a lower box is believed always to succeed (the lower body fits where the taller one did). The exact
number of growth steps and the penetration threshold are **unestablished**.

### 5.4 Contact rules of the player's body

In addition to section 4:

- **Bottom sphere, downward-facing surfaces:** unless the step replacement of section 6.3 is active, a
  contact of the bottom sphere whose normal (pointing from the surface towards the body) has a negative y
  component is dropped (the bottom sphere is never pushed down). While the step replacement is active this
  rule is off; section 6.3 says what happens to such contacts then.
- **Head sphere:** its contacts have no friction and are softened: with `e` the contact's error-reduction
  parameter, `f = 1 / ((1 − e)·20.1 + e·0.5)`; the constraint-force mixing is multiplied by `f` and the
  error-reduction parameter becomes `f·0.5·e`.
- **All contacts with the level** are then softened by the normal's direction: with `n` pointing from the
  surface towards the body, `q = 20.1 + 19.1·n.y`, `f = 1 / ((1 − e)·q + e·0.5)`, constraint-force mixing ×
  `f`, error-reduction parameter = `f·0.5·e`. So `q` is 39.2 for a floor, 20.1 for a wall and 1 for a
  ceiling: floors get the softest contacts, ceilings the stiffest. (The original generates level contacts
  with the level mesh as the first and the player's shape as the second geometry, and this term uses the
  library's raw normal, which then points from the body towards the surface; the other rules of this
  specification correct for that and are stated with the toward-body normal. For contacts with movable
  objects the sign depends on the pair's order; out of scope.)
- **Traction** `T`: reset to 0 at the start of each step and raised contact by contact, as the contacts
  of the step are processed, to the contact's value if larger. A contact's value is its friction
  coefficient `μ` (section 4.2), or 1 for a contact with a movable body, for a head-sphere contact, and for
  every contact processed after the step's "blocked" flag (section 6.3) has been set in that step.
- **Friction while moving:** while the player requests movement (`|a| > 0.1`, `a` the requested
  acceleration of section 6.2), every contact's friction is **zero**: the player slides freely along
  walls. When no movement is requested, a contact's friction is `μ × T × (4 during step assistance, else
  1)`, with `T` the running value up to and including that contact (not the step's final value). (On the
  shipped data with the default player material all solid level materials have friction 1, so the running
  and final values coincide.)
- The step's final `T` (0 in a step without contacts) is the friction factor that scales the movement
  forces of `player-movement` (its sections 6.2 and 6.4).

## 6. Movement against the geometry

### 6.1 Simulation step

The body is simulated in fixed steps of `h = 1 / ph_frequency` seconds (console variable `ph_frequency`,
100 in the default `__support\userdocs\user.ltx`: `h = 0.01 s`; the game's own default is also 0.01 s).
The solver iteration count is the console variable `ph_iterations` (38 in the default `user.ltx`). Gravity
and the movement speeds are in `player-movement`.

### 6.2 Ground, walls and the movement forces

**Interface with `player-movement`.** Each update the actor's movement logic hands the body the
**requested acceleration** `a`, a world-space vector in m/s² (it replaces the previous value); its
computation, the speed cap derived from it, gravity, the movement forces (drive, side and down forces),
the airborne control force and the stopping behaviour are specified in `player-movement` (sections 5 and
6) and are not repeated here. Movement is **requested** when `|a| > 0.1`; `â = a / |a|`.

This specification provides the collision results those forces use. Each step, among the contacts with
collision force (section 4.1), with normals taken pointing from the surface towards the body:

- the **ground contact** is the one whose normal has the largest y component; its normal is the **ground
  normal**;
- the **obstacle contact** is the one whose normal is most opposed to the requested horizontal movement
  (smallest `n.x·a.x + n.z·a.z`).

The ground is **walkable** when a ground contact exists and its normal has **y > 0.70710677** (slopes up to
45°); on walkable ground the drive force of `player-movement` follows the ground plane, otherwise it is
horizontal. The traction `T` (section 5.4) is that specification's friction factor.

During **step assistance** (section 6.3) the movement forces change as follows (`m` the body's mass):

- if an obstacle contact exists, the drive force has magnitude `25·m·T` along `â` turned into the plane of
  the obstacle contact: the direction `(up × â) × obstacle normal`, normalised (the same construction as
  the ground-plane drive, with the obstacle normal instead of the ground normal);
- in any case the drive force (of whichever rule applies) is multiplied by 4, its vertical component is made
  non-negative and each horizontal component's sign is set to that of `â`'s;
- the side force's coefficient is 700 instead of 500, and the side force also acts while the body is
  airborne (the down force does not).

Consequences: walls are slid along without friction while moving; on ground steeper than 45° the drive
becomes horizontal and stops following the slope; when no movement is requested, contact friction holds
the body. Whether a given steeper slope can still be climbed, or makes the player slide down, emerges from
these forces together with gravity and the contact solver; the steepest climbable slope as a single number
is **unestablished**.

### 6.3 Step assistance (stairs and ledges)

Every step while movement is requested, the game probes the static geometry ahead of the actor with two
boxes centred 0.4 m ahead of the actor's position (the feet) along `â`, aligned with `â`, with half-sizes
`0.7·r` along `â`, `0.5·r` across it (0.245 m and 0.175 m) and vertical half-sizes as follows:

| Box | Centre above the feet | Vertical half-size | Spans above the feet |
|---|---|---|---|
| upper | 1.5 | `2·r` = 0.7 | 0.8 … 2.2 |
| lower | `r + 0.05` = 0.4 | `r` = 0.35 | 0.05 … 0.75 |

Only triangles that are not pass-through count. **Step assistance** is on for the step when a triangle
intersects the lower box and no triangle intersects the upper box (an obstacle low ahead with free space
above it); otherwise it is off. It is also off while the body is in one internal movement state whose
meaning is **unestablished** (believed not to occur for the player on foot). (The assignment of the 0.7·r and 0.5·r half-sizes to the along and across
directions is believed from the order of the box axes; the vertical extents and the 0.4 m offset are
established.) When the body touches a movable object, different rules apply (out of scope).

During step assistance the movement forces change as in section 6.2, and contact friction when no
movement is requested is 4 times larger (section 5.4).

**Step replacement.** The contact replacement below is active for a contact only when all of these hold:
movement is requested; step assistance is on; the body is not in either of two internal body states whose
meaning is **unestablished** (believed to concern movable objects); no head-sphere contact has been registered so far in the current step; the
"blocked" flag was not set in the previous step (that flag is set in a step by a head-sphere contact, by an
upper probe box hit, or by certain contacts with movable objects); no jump has been requested and not yet
started; and no jump is in progress (a jump is in progress from the step it is applied until the first step
afterwards with walkable ground; jump request and application are in `player-movement`). When it is
active, with `n` the contact normal pointing from the surface towards the body:

- a contact of the **bottom sphere** with `n.y ≤ 0` and `n.x·â.x + n.z·â.z ≤ 0` (a surface at or above the
  sphere's centre height that faces against the movement) is replaced by a contact with normal (0, 1, 0)
  and depth `r + (contact point y − body centre y)`: the edge pushes the body straight up until the bottom
  of the bottom sphere is at the edge's height;
- a contact of the **torso cylinder** whose contact point lies below the body centre and with
  `n.x·â.x + n.z·â.z ≤ 0` is replaced in the same way (whatever `n.y`);
- every other contact is kept unchanged; in particular bottom-sphere contacts with `n.y > 0` (floor, uphill
  ground, edges below the sphere's centre), and bottom-sphere contacts with `n.y < 0` that do not face
  against the movement, are kept (the drop rule of section 5.4 does not apply while the replacement is
  active).

When the replacement is not active, the drop rule of section 5.4 applies.

The replacement acts only on edges between 0.205 m (bottom of the torso cylinder) and 0.7 m (top of the
bottom sphere) above the feet, and the lower probe box reaches 0.75 m; lower edges meet the bottom sphere
below its centre and are climbed by the sphere's own shape. The maximum step height these rules produce in
play is therefore at most about 0.7 m; the actual maximum (which also depends on the contact solver) is
**unestablished**.

### 6.4 Airborne and landing

**Departure point.** At the first step in which the body has no contact at all after a step in which it
had a contact, the body's position (its centre) is recorded.

The body becomes **airborne** at a step without a ground contact when its centre is more than 0.5 m from
the departure point and more than 0.1 m from it vertically (`|Δy| > 0.1`). It stops being airborne at a step
with a ground contact whose normal y exceeds 0.35355338 (about 69°), or when its speed is practically zero
(squared speed below 10⁻⁷). Jumping, which also makes it airborne, is in `player-movement`; these are the two airborne tests that
specification's section 6.5 refers to.

## 7. Verification facts

### 7.1 `levels\l01_escape\level.cform`

- 30,383,712 bytes (stored in `gamedata.db0`, CRC-32 `920fce34`; SHA-256
  `78f3bf273fe13c96529e5e3da9b351f23c73ccda11f681f0e9fd9743072e856e`).
- Version 4, V = 698,013, T = 1,375,470; 36 + 698,013·12 + 1,375,470·16 = 30,383,712.
- Bounding box min (−678.89703, −38.667225, −633.79108), max (766.565, 113.87654, 811.67108); it equals the
  componentwise minimum and maximum of the vertices exactly.
- Sum of all vertex coordinates (double precision, file order): (−27,658,748.514199, −464,806.416943,
  14,863,741.775122). Sum of all 4,126,410 vertex indices: 1,505,709,882,650. Every vertex is used by at
  least one triangle; the largest index is 698,012.
- Bits 14 and 15 of the attribute word: 0 and 1 in every triangle (`w & 0xC000 = 0x8000`).
- Triangles per material id: 0: 1,290; 2: 10,514; 3: 62,749; 4: 1,963; 5: 85,042; 6: 206; 7: 50,962;
  8: 11,958; 10: 210; 12: 172; 13: 152,733; 18: 42,800; 19: 114,589; 34: 4,483; 37: 114,080; 39: 579,315;
  48: 22,544; 49: 6,765; 53: 10,667; 60: 31,359; 80: 1,516; 81: 69,553.
- Sector indices 0 … 23 (24 sectors, as `level` chunk 8); sector 23 has 1,302,201 triangles, sector 0 2,704.
- 4 triangles with repeated vertex indices (zero area). Total area 2,349,844.74 m²; 227,721 triangles
  (area 1,150,394.56 m²) have front normal y > 0.70710678.
- After the load-time mapping (section 3.3), bit 14 is set in 115,596 triangles and bit 15 in 138,140
  (those of `bush`, `fake` and, for bit 15, `setka_rabica`).
- 115,596 triangles are pass-through (materials 37 `bush` and 80 `fake`); 1,516 of them (`fake`) block the
  player.

**Start position.** On the vertical line through the actor's start position (−246.72630, −24.793261,
−134.43387) (`sav`, section 10.2) the mesh has seven triangles, all above the stored y; none lies
below it. The lowest is triangle **424,895** (vertices 250,888, 247,104, 247,153), material id 12
(`materials\flooring_tile`, index 11, solid), crossed about **0.00013 m above** the stored y (0.0001322 when the vertical line is intersected with the
triangle's plane in double precision from the stored floats; single-precision evaluation gives about
0.000133), front normal
(−0.00437, 0.99999, −0.00007): it faces up and is walkable (y > 0.70710677). The actor therefore starts
standing on it, its bottom sphere overlapping it by about 0.00013 m. The next is triangle 424,977
(`materials\concrete`) 3.17711 m above the stored y, front normal y −0.99341 (a ceiling facing down), above
the standing body's top (1.741667 m) and the upper step probe (2.2 m); the remaining three are 5.2036 m
(`materials\earth`, facing up) and 10.5715 m and 17.1344 m above (`materials\bush`, a two-sided pair each).

### 7.2 `gamemtl.xr`

- 310,494 bytes; chunks 0x1000 (2), 0x1001 (8), 0x1002 (8,990), 0x1003 (301,462), in this order.
- Version 1; 57 materials with ids 0 … 93 (not contiguous), all with chunks 0x1000, 0x1001, 0x1002,
  0x1003, 0x1004, 0x1005, 0x1006; 1,247 material pairs.
- Flag counts over the 57 materials: 0x80 × 4, 0x100 × 29, 0x1000 × 1, 0x4 × 20 (12 of them non-dynamic),
  0x10000000 × 2, 0x400 × 4, 0x800 × 5.
- Index 0 is `default` (id 0, flags `0x60000010`). `materials\fake`: index 45, id 80, flags `0x00001C80`,
  physics (1, 1, 0.7, 0, 0.01). `materials\earth_death`: index 56, id 93, flags `0x70000018`, factor 1 =
  100, damage rate 150.

### 7.3 Invariants over all 30 shipped `level.cform` files

(`levels\l01_escape` … `levels\l12u_sarcofag` and the 12 `levels\testers_mp_*`, in `gamedata.db0` …
`gamedata.db4`.)

- Version 4; size = 36 + 12·V + 16·T; bounding box = exact vertex min/max; every index < V; every vertex
  used.
- Every material id used exists in `gamemtl.xr` and is not dynamic; the ids used across all levels are 0,
  2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 15, 16, 18, 19, 34, 37, 39, 48, 49, 53, 60, 80, 81, 88, 93.
- Bits 14 and 15 are each constant within a file.
- The largest sector index + 1 equals the number of sub-chunks of chunk 8 of the same level's `level` file
  (e.g. 159 for `l11_pripyat`, 1 for `l02_garbage`).
- Smallest file: `testers_mp_pool` (V 21,564, T 41,467); largest: `l11_pripyat` (V 1,091,297, T 2,043,680).

### 7.4 Derived values

Body for box 0: bottom sphere radius 0.35 centred 0.35 above the feet; torso cylinder radius 0.291667,
length 1.196735, centred 0.803265 above the feet; head sphere radius 0.291667 centred 1.45 above the feet;
outer sphere radius 0.7 centred 1.225 above the feet; mass 80. Contact parameters on `l01_escape` concrete
with the default material at `h = 0.01`: friction 1.0, error-reduction 0.8·0.01·2.4 × 10⁷ /
(0.8·0.01·2.4 × 10⁷ + 4 × 10⁵) = 0.32432, constraint-force mixing 1.6892 × 10⁻⁶ (before the player's own
adjustments of section 5.4).

## 8. Unestablished

- The meaning of material factors 0, 2 and 3, uses of flag bits other than those in section 3.2 (including
  0x20, set only on `materials\fake_ladders`), and the use of triangle bits 14 and 15 outside collision.
- The exact body-state conditions of the liquid damping (section 3.2) and whether it reaches the player.
- Whether the actor's collision shapes are given a material other than index 0 after creation.
- Which of the step probe's horizontal half-sizes lies along the movement direction (section 6.3); the
  maximum step height and the steepest climbable slope as single numbers (they emerge from sections
  6.2–6.3 together with gravity and the contact solver); the step rules while touching movable objects.
- The number of growth steps and the penetration threshold of the posture change test.
- Ladders, water, collision with movable objects and other creatures (out of scope).

## 9. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrCDB.dll`; default
  `__support\userdocs\user.ltx`.
- Data examined: all 30 `levels\*\level.cform`, the 30 `levels\*\level` files (chunk 8 only), `gamemtl.xr`,
  `config\creatures\actor.ltx`, `config\system.ltx`, `spawns\all.spawn` (start position, via the approved
  `sav` specification, section 10.2).
- Analysis: static analysis of the binaries (disassembly and decompilation with Ghidra) to establish the
  reading of `level.cform` and `gamemtl.xr`, the load-time material mapping, the contact generation and
  contact parameters, the triangle front-side test, the actor's collision body and its sizing from the box
  configuration, the box selection and posture-change test, and the body's contact, ground, step and drive
  rules with their constants. All file facts were checked with an independent throwaway parser written from
  this text, over every shipped level; the facts on the start position with a ray cast through the decoded mesh.
