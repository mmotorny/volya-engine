# Particle collision (effect chunk 33 and flag bits 0x10000, 0x20000, 0x80000)

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of what effect chunk 33 (the collision values) and the effect flag bits 0x10000,
0x20000 and 0x80000 of `particles.xr` change in a playing effect: the collision step of every update step, the ray
it casts for each particle against the level's static collision mesh (`level.cform`) and, with bit 0x80000, against
movable objects, the bounce and the removal of particles that hit, and what this changes in the particle list, the
bounding box and the end of a deferred stop. It also establishes, for the review gap of the scoping specification,
the meaning of the effect flag bits 0x800, 0x1000, 0x2000, 0x100000, 0x200000, 0x400000 and 0x800000 and the
approved specification that covers each (section 6). It is specification P5 of
`docs/behavior/particles-and-lights-scope.md`. What could not be established is marked **unestablished**; what is
inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/formats/particles-xr.md` (P1): the effect record, its flag word *F*, chunk 33 (section 2.3), the action list.
- `docs/behavior/particle-lifecycle.md` (P2): an effect instance, its particles (position **x**, previous position
  **x**ₚ, velocity **w**ₚ; P2 3.1; 18 words each, P3 1), the particle list with its birth and death notices and removal
  (P2 3.2), the update and its step (P2 8.3, whose step 4 is this specification's), the bounding box (P2 8.3 step 5),
  the deferred stop (P2 7, 8.3 step 6).
- `docs/behavior/particle-motion-actions.md` (P3): `move` (P3 2), the only action that sets **x**ₚ.
- `docs/behavior/particle-groups.md` (P6): group members are effect instances updated as P2 says; death children
  (P6 6.2 – 6.4).
- `docs/behavior/particle-drawing.md` (P7): frames, alignment and culling (section 6 here).
- `docs/formats/level-cform.md`: the triangles and vertices of `level.cform` and a triangle's front normal (its
  section 2.3).
- `docs/behavior/glows.md` section 5.1: which objects have a collision form of the skeleton kind, and their bone
  shapes.

Conventions: as P2. · between vectors is the dot product, × the cross product, |**a**| the length; **a** *k*
scales a vector. Arithmetic is IEEE single precision (`f32`); a square root is taken on the x87 unit of the
original and rounded to `f32` where stated (believed: an implementation rounding every operation to `f32` agrees;
the checks compare at a relative 10⁻⁴). Δ = 0.033 (the `f32` 0x3D072B02, the step time of P2 8.3). FLT_MIN is
1.1754944 × 10⁻³⁸. Chunk 33's three values are named as in P1 2.3: the tangential factor *ψ* (offset 0), the
resilience *β* (offset 4) and the cutoff *κ* (offset 8). Every fact carries the name of the check that reproduces
it, a script in `checks/particle-collision/` of the dirty workspace; `run_all.py` runs them all (`archive.py`
extracts `particles.xr`, `bin_setup.py` prepares the static analysis the `bin_*.py` checks read, `pxr.py` is a
decoder written from P1's text, `ref.py` is a throwaway implementation written from this text, `emu.py` interprets
the shipped instructions for the `emu_*.py` checks).

## 1. Overview

When the flag word has 0x10000, every update step of the effect ends its actions and frame animation with a
**collision step** (2): each live particle's motion of the step, the segment from **x**ₚ to **x**, is tested with a
ray (3). A particle whose ray hits something either bounces (its velocity is reflected and damped by the chunk-33
values, and it is moved again from **x**ₚ by the new velocity; up to two tests per step) or, with 0x20000, is
removed (4). Without 0x80000 the ray tests only the triangles of `level.cform`; with 0x80000 also movable objects
that have a collision form of the skeleton kind (3.3). Chunk 33 and the bits 0x20000 and 0x80000 change nothing
without 0x10000 (2).

## 2. When the collision step runs

- The collision step runs in every update step (P2 8.3) of an effect instance whose record's flag word has bit
  0x10000, as step 4: after the time-limit test, all actions of the list and the frame animation, before the
  bounding box (`bin_collision.py` C1). It runs for every effect instance, alone or as a member of a group (P6 updates
  members as P2 says).
- It visits the particles live after the actions, *n* of them, from the last (*i* = *n* − 1) to the first (*i* = 0),
  each once (`bin_collision.py` C2).
- The values it reads are chunk 33's (*ψ*, *β*, *κ*; P1 2.3: present exactly when the flag word has 0x10000) and the
  bits 0x20000 and 0x80000 (`bin_collision.py` C4, C7, C8). It tests no other bit of the flag word (`gap_bits.py`).
- Bits 0x20000 and 0x80000 are tested only by the collision step, so they have no effect without 0x10000
  (`gap_bits.py` G0: the renderer's only other tests of these values read a fixed global word, not an effect
  record). In the library 9 effects have 0x20000 without 0x10000 (`data.py` D2).
- The program has a slot for a notice called on a particle's first hit in a step; it is empty in the shipped game
  (the renderer only ever sets it to empty), so nothing else happens on a hit (`bin_collision.py` C6; believed: no
  other part of the game sets it).

## 3. The ray

### 3.1 The query

Each test casts a ray from a start **o** along a unit direction **d** with a length *L* (section 4 gives them) and a
**mask**: 2 when the flag word lacks 0x80000, 3 when it has it (`bin_collision.py` C4; `emu_step.py` checks the
mask the shipped step passes). **Note:** P1 section 3's row for 0x80000 gives the two masks the other way round; the
mask is 3 with the bit.

The query's answer is the nearest hit with a distance *t* < *L*, and whether it is a triangle of the static mesh
(which one) or an object (`bin_raypick.py` R1, R2):

1. The nearest distance starts as *L*, with no hit.
2. Mask bit 0x2 (both masks): the static mesh (3.2). Its nearest front-side hit at distance *t* with 0 < *t* ≤ *L*
   is taken when *t* < *L*: the nearest distance becomes *t*, the hit is that triangle.
3. Mask bit 0x1 (mask 3 only): objects (3.3), each tested against the current nearest distance; an object hit at a
   distance below it becomes the hit and the nearest distance.

So a static hit wins a tie with an object; a hit at exactly *L* (the segment's end) is not a hit. When two triangles
are hit at exactly the same distance, which one is taken depends on the order of the collision tree the game builds
(**unestablished**); it matters only for their normals.

### 3.2 The static mesh

The static mesh is every triangle and vertex of the level's `level.cform` (level-cform.md 2.1), in file order: the
game reads the file at level load and keeps its vertices and triangles as stored (`bin_raypick.py` R5). No triangle
is left out by its material: pass-through (`bush`, `water`, `fake`, …), solid and every other material are tested
alike (`bin_raypick.py` R6: the triangle test reads only the three vertex indices, and the attribute word with
the material is only copied into the result, never tested).

A triangle with vertices **v**₀, **v**₁, **v**₂ (in its index order) is hit by the ray at distance *t* when, with
**e**₁ = **v**₁ − **v**₀, **e**₂ = **v**₂ − **v**₀, **p** = **d** × **e**₂ and *D* = **e**₁ · **p**
(`bin_raypick.py` R6; `emu_tri.py` runs the shipped test):

- *D* ≥ 10⁻⁵ (the `f32` 0x3727C5AC); since *D* = −**d** · (**e**₁ × **e**₂), and **e**₁ × **e**₂ points along the
  triangle's front normal (level-cform.md 2.3), this holds only when the ray travels against the front normal: a
  triangle is hit only from its front side;
- with **s** = **o** − **v**₀, *u* = **s** · **p**: 0 ≤ *u* ≤ *D*;
- with **q** = **s** × **e**₁, *v* = **d** · **q**: 0 ≤ *v* and *u* + *v* ≤ *D*;
- *t* = (**e**₂ · **q**) · (1 / *D*), and 0 < *t* ≤ *L* (3.1).

This is the public ray–triangle test of Möller and Trumbore with back faces culled; the game has a second variant
without the culling that the collision step does not use (`emu_tri.py`: the two variants differ exactly by hits from
the back side). Of several hit triangles the nearest is kept; a later one replaces a kept one only when strictly
nearer (`bin_raypick.py` R6).

### 3.3 Objects (bit 0x80000)

With mask 3 the ray also tests objects (`bin_raypick.py` R2 – R4):

- **Which objects:** objects whose collision form is of the skeleton kind (glows.md 5.1 says which objects have one:
  those whose configuration section has a `cform` line, and those the game gives one; the shipped configuration
  has 228 `cform` lines, all `cform = skeleton`, `cform_lines.py`), while they are collidable: an object becomes
  collidable when the game enables it and it has a collision form, and stops being collidable when the game
  disables it (`bin_raypick.py` R3). No object is excluded, the player's own included (section `actor` of
  `config\creatures\actor.ltx` has `cform = skeleton`, `cform_lines.py`). Objects whose collision form is of the
  game's other kind (not skeleton) are not tested with mask 3. Which objects are enabled at a given moment is
  **unestablished** here (believed: those online and not hidden).
- **The test:** the ray must pass the form's bounding sphere; then each bone element with a shape (box, sphere or
  cylinder, in the current pose of the model; elements without a shape or without a bone are skipped; glows.md 5.1
  and the bone data it names) is tested, and an element counts only when the ray **enters** its shape from outside:
  a ray that starts inside a shape does not hit that shape. The hit distance is the distance from **o** to the entry
  point, and counts when 0 ≤ *t* and *t* is below the current nearest distance; the nearest is kept. The exact
  ray–box, ray–sphere and ray–cylinder computations are the usual ones (believed; their rounding is
  **unestablished**).
- Which objects the game's spatial pre-selection hands to these tests is **unestablished** (believed: every
  collidable object whose region the ray passes, so that it excludes no object whose shapes the ray enters).

## 4. The collision step for one particle

For particle *i* (2), with its position **x**, previous position **x**ₚ and velocity **w** (= **w**ₚ of P2), up to
two tests are made; each test (`bin_collision.py` C2 – C8; `emu_step.py` runs the shipped step on 1,836 particles
and compares every word of every particle slot with `ref.py`):

1. **Segment.** **δ** = **x** − **x**ₚ, *L* = √(|**δ**|²) (|**δ**|² in `f32`; the root compared before rounding,
   then rounded to `f32`). If *L* < 10⁻⁵ (the `f32` 0x3727C5AC), or *L* is not a number: **x** ← **x**ₚ, and the
   particle is done for this step (no ray).
2. **Ray** (3.1) from **o** = **x**ₚ along **d** = **δ** · (1 / *L*) with length *L* and the mask of 3.1. No hit:
   the particle is done (it keeps **x** and **w**).
3. **Normal.** For a triangle: its front normal **n** = (**v**₁ − **v**₀) × (**v**₂ − **v**₁) (level-cform.md 2.3),
   multiplied by √(1 / |**n**|²) when |**n**|² > FLT_MIN (always the case for a triangle that can be hit, 3.2). For
   an object: **n** = (0, 1, 0), whatever the shape or where it is hit (`bin_collision.py` C5).
4. **Removal** (flag 0x20000): the particle is removed (section 5) and is done; its values are not changed first
   (`bin_collision.py` C7). The particle that the removal moves into place *i* (P2 3.2) is not tested in this step:
   it was tested before, at its earlier place.
5. **Bounce** (no 0x20000): *w*ₙ = **w** · **n**; **t** = **w** − **n** *w*ₙ; unless *κ* ≥ |**t**|², **t** ← **t** *ψ*
   (so the tangential part is scaled when its squared length exceeds the cutoff, or is not a number); then **w** ←
   **t** − (**n** *w*ₙ) *β* and **x** ← **x**ₚ + **w** Δ (`bin_collision.py` C8). **x**ₚ is not changed: the particle
   restarts from its previous position, not from the hit point, and moves a full step Δ with the new velocity. The
   hit point itself is not used.
6. After the first test's bounce the particle is tested again from 1 (with the new **x**); after the second test's
   bounce it is done, whatever lies on its new segment (it can end beyond a surface).

So in one step a particle is tested at most twice, and moved by at most two bounces. The resilience *β* = 0 makes
the particle slide (no normal component kept), *β* = 1 reflects the normal component fully; *ψ* = 1 keeps the
tangential part, *ψ* = 0 removes it above the cutoff.

The segment is the step's whole motion only as far as **x**ₚ was set by `move` in the same step (P3 2): in all 71
colliding effects of the library the list has `move`, and no `randomdisplace` or `vortex` (the other actions that
change positions, P3 2) stands after it, so the segment is exactly `move`'s displacement **w** Δ with the velocity
`move` used; in 7 of them further actions follow `move` (P3 and P4 say what they change), and the bounce uses the
velocity as the whole list left it (`data.py` D4).

## 5. Removal, the particle list and the bounding box

- A removal is P2 3.2's: the death notice with the particle's values at that moment (for a group member, P6 6.2: a
  death child is made at its position **x**, which is the step's end position beyond the surface it hit, with
  velocity (**x** − **x**ₚ) · (1 / 0.033)), then the count shrinks by one and the then-last particle is copied into
  place *i* (`bin_collision.py` C7). The vacated slot (the old last place) keeps its contents: the particle that was
  moved out of it, or the removed particle itself when it was the last (`emu_step.py` compares all slots).
- The bounding box (P2 8.3 step 5) and the end of a deferred stop (P2 8.3 step 6) use the same particle count *n*
  as the collision step, the count before it, and the same storage (`bin_collision.py` C1). After removals the box
  therefore spans the first *n* slots of the storage, including the slots at and beyond the new count with the
  contents just described: positions of removed particles (as at their removal) and duplicates of moved ones count.
  A stopping effect whose last particles are all removed by the collision step is not ended in that step (*n* > 0)
  but in the next one, whose count is 0. This settles P2 section 10's **unestablished** item on particles removed by
  collision.
- A bounce changes only **x** and **w** of the particle (and step 1 only **x**); all other words are unchanged
  (`emu_step.py`).

## 6. The effect flag bits 0x800, 0x1000, 0x2000, 0x100000, 0x200000, 0x400000, 0x800000

None of these bits is tested by the collision step (`gap_bits.py`). Each is tested by the particle birth, the frame
animation or the drawing of the first renderer, and its behaviour is specified by the approved
`docs/behavior/particle-drawing.md` (P7), in the section named (`gap_bits.py` G1 – G3):

| Bit | Effects | Meaning | Specified in |
|---|---|---|---|
| 0x800 | 421 | the frames are animated over time, only together with 0x400 (the animation runs when both are set); together with 0x2000 it also enables the backwards choice at birth | P7 3.3; P7 3.2 |
| 0x1000 | 339 | each new particle starts on a random frame (drawn from the shared generator at birth) | P7 3.2 |
| 0x2000 | 95 | with 0x800: each new particle plays its frames backwards when a further number drawn from the shared generator at birth is odd; without 0x800 no effect | P7 3.2, 3.3 |
| 0x100000 | 51 | with 0x8000: a particle at rest takes the fixed orientation of the alignment angles (chunk 37); without 0x8000, or for a moving particle, no effect | P7 4.3 |
| 0x200000 | 126 | with 0x8000: a moving particle's sprite lies across its motion; without 0x8000, or for a particle at rest, no effect | P7 4.3 |
| 0x400000 | 12 | face culling while the effect is drawn: clockwise faces culled | P7 4.5 |
| 0x800000 | 1 | with 0x400000: counter-clockwise faces culled instead; without 0x400000 (its one effect) no effect | P7 4.5 |

(Counts over the 1,075 effects, as in P1 3.) The meanings agree with P1 section 3's table; P7 states the exact
rules.

## 7. Verification

### 7.1 Facts on `particles.xr`

(`data.py`, decoding with `pxr.py`.)

- 71 effects have 0x10000 (all with chunk 33); 70 of them have 0x1 (drawn); `alexmx\test1_old00` collides but is
  never drawn (D1).
- 31 of the 71 also have 0x20000 (removed on a hit), 40 bounce; 9 effects have 0x20000 without 0x10000 (no effect)
  (D2).
- 0x80000: 2 effects, both with 0x10000: `anomaly2\group_items\studen_idle_00` (flag word 0xF1001, also 0x20000:
  removed on hitting the mesh or an object) and `explosions\group_items\expl_mushroom_fire_small` (0x94C01:
  bounces off the mesh and objects, *ψ* = 0.72, *β* = 0, *κ* = 0) (D2).
- Chunk 33 of the 40 bouncing effects: (*ψ*, *β*, *κ*) = (1, 0, 0) in 8, (1, 0.2, 0), (1, 0.5, 0) in 3 each; only
  two have *κ* > 0: `alexmx\test1_old00` (0.91, 1, 1) and `anomaly2\group_items\body_tear_blood_03` (0.5, 0.3,
  0.25) (D3). In all 71, *ψ* and *β* lie in 0 … 1 and *κ* ≥ 0 (D3; P1 2.3).
- Every colliding effect has `move`; none has `randomdisplace` or `vortex` after it; in 7 `move` is not the last
  action (D4).
- Group members (P1 6.1) name colliding effects 73 times as the effect, 11 times as the death child, once each as
  per-particle and birth child; 17 colliding effects are named by no group. 9 members whose effect removes
  particles on a hit have a death child (bit 0x40), so their particles' hits start death children, e.g.
  `anomaly2\studen_idle_up` (its `studen_idle_00` starts `anomaly2\group_items\hit_studen_hit_distort_00`) and
  `explosions\expl_car` (`expl_flying_flames` starts `expl_mushroom_fire_small`) (D5).
- The 22 effects of the first level's start (scoping spec 1.2: the `level.ps_static`, zone and artefact roots with
  every member slot of their groups) include no colliding effect. Of the weather's ambient effects of the start
  (scoping spec 1.2), the group `nature\fog_stormy_01` plays `nature\group_items\fog_stormy_dust_02` (0x31001:
  removed on a hit) and `nature\group_items\fog_stormy_dust` (0x11401: bounces, (0.95, 0.25, 0)); the members of
  `nature\fog_foggy_00` do not collide (D6). Whether and when these play is P11's.

### 7.2 Test vectors

Computed by `ref.py`, an implementation of sections 3 – 5 written from this text, not measured in the game; a
particle is given as **x**ₚ and **w**, with **x** = **x**ₚ + **w** Δ (`f32`) unless stated.

Synthetic (`vectors.py`): one triangle (−50, 0, −50), (50, 0, 50), (50, 0, −50), front normal (0, 1, 0).

| # | Flag word | (*ψ*, *β*, *κ*) | Particle | Result |
|---|---|---|---|---|
| S1 | 0x10000 | (1, 0.5, 0) | **x**ₚ (0, 0.5, 0), **w** (3, −30, 0) | hit; **w** (3, 15, 0), **x** (0.099, 0.995, 0); the second test (upwards) hits nothing |
| S2 | 0x10000 | (0.5, 0.5, 0) / (0.5, 0.5, 10) / (0.5, 0.5, 9) | as S1 | **w** (1.5, 15, 0) / (3, 15, 0) / (3, 15, 0) (\|**t**\|² = 9 is not above 9) |
| S2d | 0x10000 | (1, 0, 0) | as S1 | **w** (3, 0, 0), **x** (0.099, 0.5, 0) |
| S3 | 0x10000 | (1, 0.5, 0) | **x**ₚ (0, 0.5, 0), **w** (0, −0.0001, 0) | *L* < 10⁻⁵: **x** = **x**ₚ |
| S4 | 0x10000 | (1, 0.5, 0) | **x**ₚ (0, −0.5, 0), **w** (0, 30, 0) | crosses the triangle from its back: no hit, **x** (0, 0.49, 0) |
| S5 | 0x30000 | (1, 0.5, 0) | three particles: 0 **x**ₚ (1, 2, 1) **w** (0, 1, 0); 1 **x**ₚ (0, 0.5, 0) **w** (0, −30, 0); 2 **x**ₚ (−1, 2, −1) **w** (0, 2, 0) | particle 1 removed, count 2; slot 1 holds particle 2, slot 2 still holds particle 2 |
| S6 | 0x30000 | (1, 0.5, 0) | particles 0 and 1 of S5 | particle 1 (the last) removed, count 1; slot 1 still holds it, **x** (0, −0.49, 0) |
| S7 | 0x90000 | (1, 0.5, 0) | an object's shape on the segment from **x**ₚ to **x**; **w** (4, −2, 3) | **n** = (0, 1, 0): **w** (4, 1, 3); with 0x10000 alone the object is not tested and **w** is unchanged |

On `levels\l01_escape\level.cform` (`level_vectors.py`), at the actor's start position (start-position.md;
level-cform.md 7.1: on its vertical line the floor triangle 424,895, front normal (−0.00437, 0.99999, −0.00007),
and the ceiling triangle 424,977, front normal y −0.99341), with **x**ₚ = (−246.72630, −23.793261, −134.43387),
1 m above the stored position:

| # | Flag word | (*ψ*, *β*, *κ*) | **w** | Result |
|---|---|---|---|---|
| V1 | 0x10000 | (1, 0.5, 0) | (0, −40, 0) | hits triangle 424,895 at *t* = 0.999868; **w** (−0.26231, 19.99886, −0.0044), **x** (−246.73495, −23.1333, −134.43402) |
| V2 | 0x30000 | (1, 0.5, 0) | (0, −40, 0) | removed; count 0 |
| V3 | 0x10000 | (0.5, 0.5, 0) | (3, 100, 0) | first test hits the ceiling 424,977 at *t* = 2.177107 (**Disputed in review:** with **x** = **x**ₚ + **w** Δ as stated, the ray is tilted and hits 424,977 at *t* = 2.178059; 2.177107 is the distance along a vertical segment; the resulting **w** and **x** are unchanged) and sends the particle down; the second test hits the floor 424,895; **w** (0.51653, 24.35004, 5.68958), **x** (−246.70926, −22.98971, −134.24611) |
| V4 | 0x10000 | (1, 0.5, 0) | (0, −20, 0) | the segment (0.66 m) ends above the floor: no hit |
| V5 | 0x10000 | (1, 0.5, 0) | (0, −0.0001, 0) | *L* < 10⁻⁵: **x** = **x**ₚ |

## 8. Corrections to approved specifications

- `docs/formats/particles-xr.md` section 3, row 0x80000: the query mask is 3 **with** the bit and 2 without it
  (the row says the reverse); with the bit the ray also tests movable objects with a collision form of the skeleton
  kind (3.3). This settles P1 section 9's **unestablished** item on 0x80000 (`bin_collision.py` C4,
  `bin_raypick.py` R2).
- `docs/behavior/particle-lifecycle.md` section 8.3 step 5 and section 10: settled in section 5 here.

## 9. Unestablished

- Which of several triangles hit at exactly the same distance is taken (3.1).
- Which objects are enabled, and what the spatial pre-selection hands to the object test (3.3).
- The exact computations and rounding of the ray tests against bone boxes, spheres and cylinders (3.3; believed the
  usual entering tests).
- The x87 precision of the square roots (conventions; believed `f32` per operation suffices).

## 10. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): `particles.xr` from `gamedata.db4` (identical
  copy in `gamedata.dba`), `levels\l01_escape\level.cform` from `gamedata.db0`, the configuration (`cform` lines);
  binaries `bin\xrRender_R1.dll` (MD5 `2bf95c8376b735daa6d82b8944894be0`), `bin\xrParticles.dll` (MD5
  `22bad4d258e6211c6ea7cc8dc8d7ca55`), `bin\XR_3DA.exe` (MD5 `ba3ad0127bff6619ed991a7c82d8b946`), `bin\xrCDB.dll`
  (MD5 `28fef3c1e4a3de265ae707ea3212731a`).
- Data examined: every effect and group record of `particles.xr` (decoded with a throwaway decoder written from P1's
  text); the first level's collision mesh around the start position.
- Analysis: static analysis (Ghidra 12.2, headless; decompilation and instruction listings; the system's LLVM
  disassembler for a scan of stores) of the first renderer's effect update and collision step, the particle
  library's particle removal, the engine's ray query and its object part (collision form kinds, the skeleton form's
  ray test), the level's collision mesh load and the collision library's ray query and ray–triangle tests. The
  shipped collision step and ray–triangle tests were run by interpreting their instructions on prepared input and
  compared with a throwaway implementation written from this text (`ref.py`). No execution or debugging of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/particle-collision/` reproduce every fact.

## Gaps found in review

- The ray uses the particle's stored position and previous position as world coordinates with no transform; for an instance in local mode (P2 5.1), whose particles stay in the effect's own space, the text does not say that these effect-space values are tested against the world mesh and objects as they are.
- How an object's collision-form bounding sphere (3.3) is formed and the exact rule by which the ray passes it are not stated.
