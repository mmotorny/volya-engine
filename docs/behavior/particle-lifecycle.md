# Particle effect lifecycle and emission

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of an effect of `particles.xr` over time: an effect instance and its particles,
playing and stopping, the time limit (effect chunk 8), the update and its fixed step, placement of the effect by
a transform and a velocity (including, per action type, which stored fields follow the transform), particle birth
by `source` (action type 21), and the point generation, containment test and transform of all 11 region kinds.
It is specification P2 of `docs/behavior/particles-and-lights-scope.md`. What the other actions do in a step is
P3's (motion) and P4's (lifetime and appearance); frames and drawing are P7's; collision (chunk 33) is P5's;
groups are P6's. What could not be established is marked **unestablished**; what is inferred rather than read from
the program or the data is marked **believed**.

Builds on (not repeated): `docs/formats/particles-xr.md` (P1: the effect record, flag word, action list, action
flag word, field layouts of section 4.3, regions of section 5; "field" and "region" below are P1's),
`docs/behavior/object-visual-and-transform.md` (matrix convention).

Conventions: matrices in the row-vector convention of object-visual-and-transform.md: a point **p** is placed as [**p** 1] · *M*
(rows 0 – 2 of *M* are the transformed axes, row 3 the translation), a direction **d** as [**d** 0] · *M*. ·
between vectors is the dot product, × the cross product, |**a**| the length. Arithmetic is IEEE single precision
(`f32`) unless stated. floor(*x*) is the largest integer ≤ *x*; trunc(*x*) drops the fraction toward zero. Times
are in seconds, "ms" in milliseconds. Region fields are P1's names: kind, **p**₁, **p**₂, **u**, **v**, *r*₁ – *r*₄.
FLT_MIN is 1.1754944 × 10⁻³⁸. Every fact carries the name of the check that reproduces it, a script in
`checks/particle-lifecycle/` of the dirty workspace; `run_all.py` runs them all (`archive.py` extracts
`particles.xr`, `bin_setup.py` prepares the static analysis the `bin_*.py` checks read, `sim.py` is a throwaway
implementation written from this text that `data.py` uses for section 9).

## 1. Overview

An effect instance is made from one effect record. It holds its own copy of the record's action list, a list of
live particles, a state (playing, stopping, local placement), a remaining time for the time limit, a millisecond
accumulator, a position, a placement matrix and a bounding box. Its owner (a level effect, a zone, a group, …; the
owners' specs say which) places it (section 5), plays and stops it (section 7) and updates it with the elapsed
real time (section 8). Each update runs whole steps of 33 ms; a step runs the time-limit test and every action of
the list in list order with step length Δ = 0.033 (section 8.3). `source` actions add particles (section 6);
other actions change or remove them (P3, P4).

## 2. Random numbers

### 2.1 Uniform numbers

Every random choice of the particle actions and regions takes values from one generator, an object of the
program's core library that the particle library uses rather than a state of its own (`bin_random.py`): a state *s* (32 bits) is advanced as *s* ← *s* · 214013 + 2531011 (mod 2³²) and
gives *k* = (*s* >> 16) & 0x7FFF (0 … 32767). From *k*:

- U = *k* · (1/32767) (the `f32` nearest to 1/32767), a number in 0 … 1, both ends included;
- an angle θ = *k* · (2π/32767) (the `f32` constant), in 0 … 2π.

Each U or θ below takes a fresh *k*, in the order written. The state's starting value and which other parts of
the program draw from it are **unestablished**; since they draw from the same sequence, the game's exact values
cannot be reproduced, and any uniform generator gives the same distributions (believed: nothing else observable
depends on the sequence). The generator is stated for test vectors (section 9.2).

### 2.2 Normal deviate g(σ)

Used by region kind 8 (section 4.1) and by the age spread of `source` (section 6) (`bin_random.py`):

- if σ = 0, g = 0 and nothing is drawn;
- otherwise: *y* = −ln U₁, U₂ drawn; this is repeated (fresh U₁, U₂) while U₂ > exp(−(*y* − 1)²/2); then
  g = 1.2539185 · σ · *y* when a second generator (the C runtime's `rand()`, not the one of 2.1) gives an odd
  value, else −1.2539185 · σ · *y*.

*y* is thus distributed as the absolute value of a standard normal variable; g is symmetric about 0 with
standard deviation 1.2539185 σ (believed: derived from the rule, not measured).

## 3. Particles

### 3.1 A particle

Each particle has (`bin_source.py`, `bin_manager.py`): position **x**, previous position **x**ₚ, velocity
**w**ₚ, size **s** (3 components), rotation **ρ** (3 components), colour (four 8-bit channels A, R, G, B), age *a*,
a frame number (16 bits, P7) and frame flags (16 bits, P7). What reads the previous position, the rotation and the
frame values is P3's and P7's.

### 3.2 The particle list

The live particles form a list of at most *N* entries, *N* = the effect's maximum (chunk 3) (`bin_manager.py`,
`bin_effect.py`):

- a new particle is appended at the end (section 6); nothing is appended when the list is full;
- removing particle *i* (by an action of P3, P4 or P5) moves the last particle into place *i* and shortens the
  list by one; the list order is therefore not birth order. The removal of a particle is reported to the owner's
  death notice before the move (used by groups, P6; an effect alone does nothing on it);
- each birth is reported to the birth notice after the particle is written and before the count grows; for an
  effect alone the notice sets the frame values (P7) and nothing else (`bin_effect.py`).

## 4. Regions

The actions use their regions in three ways: points are drawn from a region (4.1), positions are tested against a
region (4.2), and a region is placed by a matrix (4.3). Fields not named for a kind are not read in that use.

### 4.1 Point generation

The point generated from a region, per kind (`bin_generate.py`, `bin_random.py`):

| Kind | Point |
|---|---|
| 0 point, 3 plane | **p**₁ (nothing drawn) |
| 1 line | **p**₁ + U **p**₂ (one U for all components) |
| 2 triangle | *a* = U₁, *b* = U₂; if *a* + *b* ≥ 1 then *a* ← 1 − *a*, *b* ← 1 − *b*; **p**₁ + *a* **u** + *b* **v** |
| 4 box | per axis, in the order x, y, z: *p*₁ + U (*p*₂ − *p*₁) |
| 5 sphere | **d** = (U₃ − ½, U₂ − ½, U₁ − ½) (the first U gives z, the third x); if \|**d**\|² > FLT_MIN, **d** ← **d** / \|**d**\|; ℓ = *r*₁ if *r*₁ = *r*₂, else *r*₂ + U₄ (*r*₁ − *r*₂) (U₄ only then); **p**₁ + ℓ **d** |
| 6 cylinder | *h* = U₁, θ, ℓ = *r*₂ + U₃ (*r*₁ − *r*₂); **p**₁ + *h* **p**₂ + ℓ cos θ **u** + ℓ sin θ **v** |
| 7 cone | as 6, with ℓ multiplied by *h* |
| 8 Gaussian cloud | **p**₁ + (g(*r*₁), g(*r*₁), g(*r*₁)), x first (2.2) |
| 9 disc | θ, ℓ = *r*₂ + U₂ (*r*₁ − *r*₂); **p**₁ + ℓ cos θ **u** + ℓ sin θ **v** (**p**₂ not read) |
| 10 rectangle | *a* = U₁, *b* = U₂; **p**₁ + *b* **u** + *a* **v** |
| above 10 | (0, 0, 0) |

The sphere's direction comes from a cube, so it is not uniform over directions; radii are used as stored, also
when negative (P1 5.2).

### 4.2 Containment

Whether a position **x** is inside a region (`bin_within.py`); **d** = **x** − **p**₁:

| Kind | Inside when |
|---|---|
| 0, 1, 2, 9, 10, and kinds above 10 | never |
| 3 plane | **p**₂ · **x** ≥ −*r*₁ (the side the normal points to, the plane included) |
| 4 box | *p*₁ ≤ *x* ≤ *p*₂ on each of the three axes (bounds included) |
| 5 sphere | *r*₄ ≤ \|**d**\|² ≤ *r*₃ |
| 6 cylinder | with *t* = (**d** · **p**₂) *r*₄ and *q* = \|**d** − *t* **p**₂\|²: 0 ≤ *t* ≤ 1 and *r*₂² ≤ *q* ≤ *r*₃ |
| 7 cone | with *t* and *q* as for 6: 0 ≤ *t* ≤ 1 and (*r*₂ *t*)² ≤ *q* ≤ (*r*₁ *t*)² |
| 8 Gaussian cloud | *r*₂ · exp(*r*₄ \|**d**\|²) > U, a fresh U per test: a random answer, more likely near **p**₁ |

The test reads the derived values *r*₃, *r*₄ as they stand (stored, or kept by placement, 4.3); the derived values
are not recomputed.

### 4.3 Placement of a region by a matrix

A region is placed by a matrix *M* by computing new fields from the stored region (`bin_transform.py`). Fields not
listed keep their stored values (in particular *r*₁ – *r*₄ except where stated; the derived values of P1 5.2 are
not recomputed, and directions are not renormalised):

| Kind | Placed fields |
|---|---|
| 0 point, 5 sphere, 8 Gaussian cloud | **p**₁ as a point |
| 1 line | **p**₁ as a point, **p**₂ as a direction |
| 2 triangle, 6 cylinder, 7 cone, 9 disc, 10 rectangle | **p**₁ as a point; **p**₂, **u**, **v** as directions |
| 3 plane | **p**₁ as a point, **p**₂ as a direction; then *r*₁ = −(**p**₁′ · **p**₂′) from the placed values |
| 4 box | the axis-aligned box of the 8 placed corners: start with **p**₁′ = **p**₂′ = [**p**₁ 1] · *M*; for each row *i* = 0, 1, 2 and column *j*: *t* = *M*[*i*][*j*] · (*p*₂ − *p*₁)ᵢ; add *t* to *p*₁′ⱼ if *t*'s sign bit is set (negative, or −0), else to *p*₂′ⱼ |
| above 10 | **unestablished** (the kind indexes past the program's table; believed a crash) |

"Placing a region without translation" below means placing it by *M* with row 3 replaced by (0, 0, 0, 1): its
points then turn with *M* but do not move with *M*'s translation.

## 5. Placing an effect

### 5.1 The placement request

The owner places an instance with a matrix *M*, a velocity **w** and a choice of mode (`bin_effect.py`):

- **World mode** (the usual one): the instance's position becomes *M*'s row 3, and its action list is placed by
  *M* and **w** (5.2). The particles keep their values: already live particles are not moved.
- **Local mode**: nothing is placed and **w** is not used; *M* is kept, and the particles, which stay in the
  effect's own space, are drawn transformed by *M* (P7). The instance's position is not changed by a local-mode
  placement. The bounding box and sphere (8.2, 8.3) are then in the effect's own space too: the instance computes
  them from the particles' stored positions (or its position) and never maps them by *M* (`bin_local_box.py`).
  *M* is applied by their users: the effect object's culling sphere and the queuing of the effect for drawing take
  the sphere's centre mapped as a point by the object's transform, which a local-mode placement sets to *M*, and
  the radius as it is, not scaled by *M* (P7 7.1; `bin_local_box.py`). Neither of these reads the box
  (`bin_local_box.py`).
  **Disputed in review:** the box is read and mapped too: the occlusion test of a drawn effect object (P7 7.1;
  `docs/behavior/occlusion-tests.md` 6, "An object's box") takes the instance's bounding box as its visual's box and
  uses the smallest axis-aligned box around that box transformed by the object's transform (in local mode *M*).

A world-mode placement clears local mode; a local-mode placement sets it. A new instance has position (0, 0, 0),
the identity as its kept matrix, and its action list as stored, which acts as a placement by the identity with
**w** = (0, 0, 0) (`bin_effect.py`, `bin_placement.py`).

### 5.2 Placing the action list

Every action of the list is placed in list order, each starting from its own stored values (the values read from
the record, which placement never changes), so placing again replaces the previous placement
(`bin_placement.py`). An action whose action flag word has bit 0x2 is placed by *M*′ = *M*; any other by *M*′ =
the identity with *M*'s row 3 (translation only). Then, for every `source` (whatever its bit 0x2), the parent
velocity becomes **w** · *q* (*q* = its parent motion field, P1 4.3).

The fields each action type places, by P1's field names and offsets (`bin_placement.py`; the steps of P3 and P4
read the placed values, never the stored ones of these fields):

| Type | Placed by *M*′ | Not placed |
|---|---|---|
| 1 `bounce` | surface region (0): placed (4.3) | the three values |
| 9 `jet` | centre (0) as a point; acceleration region (12) placed without translation | magnitude, epsilon, maximum radius |
| 14 `orbitpoint` | centre (0) as a point | the three values |
| 15 `randomaccel`, 16 `randomdisplace`, 17 `randomvelocity` | region (0) placed without translation | — |
| 19 `sink` | region (4) placed | kill-inside switch |
| 21 `source` | position region (0) placed; velocity region (68) placed without translation | rotation, size and colour regions, alpha, rate, age, age spread, parent motion; the parent velocity is set from **w** as above |
| 27 `targetvelocity` | velocity (0) as a direction | scale |
| 29 `vortex` | centre (0) as a point; axis (12) as a direction (not renormalised) | magnitude, epsilon, maximum radius |
| 31 `scatter` | centre (0) as a point | the three values |
| 4 `damping`, 6 `follow`, 7 `gravitate`, 8 `gravity`, 10 `killold`, 12 `move`, 22 `speedlimit`, 23 `targetcolor`, 24 `targetsize`, 25 `targetrotate`, 30 `turbulence` | nothing | all fields (e.g. `gravity`'s acceleration and `turbulence`'s drift stay in world axes) |

With bit 0x2 clear, *M*′ has no rotation, so the directions and the regions placed without translation keep their
stored values, and points move by *M*'s translation only. With bit 0x2 set they follow *M* fully, including any
scale in *M* (radii do not scale).

## 6. Particle birth (`source`)

A `source` is switched off while its action flag word has bit 0x40000000 (set and cleared at run time, section 7).
Each step (Δ = 0.033), a switched-on `source` (`bin_source.py`):

1. *z* = rate · Δ; *n* = floor(*z*) (as an integer); one U is drawn, and *n* ← *n* + 1 if *z* − *n* > U.
2. If count + *n* > *N*, *n* ← *N* − count (count: the live particles at that moment, including those added by
   earlier actions of the same step). Nothing happens when *n* ≤ 0.
3. For each new particle, in this order, using the placed regions of 5.2 and generation of 4.1:
   1. **x** = **x**ₚ = a point of the position region;
   2. **s** = a point of the size region; with action flag bit 0x20000000, *s*ᵧ = *s*_z = *s*ₓ;
   3. **ρ** = a point of the rotation region;
   4. **w**ₚ = a point of the velocity region + the parent velocity;
   5. **c** = a point of the colour region;
   6. *a* = age + g(age spread) (2.2);
   7. colour: R, G, B = clamp(trunc(255 *c*ₓ), 0, 255), clamp(trunc(255 *c*ᵧ), 0, 255),
      clamp(trunc(255 *c*_z), 0, 255); A = clamp(trunc(255 · alpha), 0, 255); stored as one 32-bit value
      A · 2²⁴ + R · 2¹⁶ + G · 2⁸ + B;
   8. frame number 0 and frame flags 0; then the birth notice (3.2) and the count grows by one.

The action flag bit 0x80000000 does not change any of this (P1 4.2; `bin_source.py`). A particle is added with
these values only; ageing and motion are other actions' (`move`, P3).

## 7. Playing and stopping

(`bin_effect.py`, `bin_manager.py`.)

- **Play**: clears stopping, sets playing, and switches every `source` of the list on (clears bit 0x40000000);
  for every `turbulence` (30) it also resets a run-time value of that action to 0 (its meaning is P3's). Live
  particles, the accumulator and the remaining time are kept.
- **Stop, deferred**: switches every `source` off and sets stopping; the particles live on and the effect keeps
  playing until none is left (8.3).
- **Stop, immediate**: switches every `source` off, removes every particle at once (the count becomes 0, without
  death notices) and clears playing. Stopping is left as it was.
- **Playing** is the playing state; an instance without a record is updated as not playing (8.2).
- The effect's **length** is −1 without flag bit 0x4000, else the chunk-8 time limit *T* (P1); an effect with
  length −1 is looped (P6 uses this).

## 8. Update

### 8.1 Giving the instance its record

When an instance gets its record (`bin_effect.py`): the record's action list is read into the instance's own list
(P1 4), the particle maximum becomes *N* (chunk 3), the birth and death notices are set, and, when the flag word
has 0x4000, the remaining time becomes *T*. A new instance's remaining time and accumulator are 0 and it is not
playing. Raising *N* keeps the live particles; lowering it below the count drops the particles beyond the new
maximum (`bin_manager.py`).

A new instance's bounding box is empty, its minimum +3.4028235 × 10³⁸ and its maximum −3.4028235 × 10³⁸ on each
axis (the largest finite `f32` and its negative), and its sphere has centre (0, 0, 0) and radius 0
(`bin_new_box.py`). Giving it its record, playing, stopping and placing it (sections 5, 7) change neither; only an
update does (8.2, 8.3). So box and sphere keep these values until the first update while not playing, or, while
playing, until the first step with live particles.

### 8.2 An update with *e* ms

The owner updates the instance with the elapsed real time *e*, a whole number of milliseconds (`bin_effect.py`):

- **Not playing**: the bounding box becomes the position ± 0.001 on each axis, the sphere is the box's centre and
  half its diagonal; nothing else happens (the accumulator is not changed).
- **Playing**: the accumulator *A* grows by *e*. If *A* < 33, nothing else happens. Otherwise *n* = floor(*A* / 33)
  and *A* ← *A* mod 33; *n* is limited to 3 (the time of further steps is dropped); then up to *n* steps (8.3) are
  run, fewer when a step ends the update.

### 8.3 A step

In this order (`bin_effect.py`, `bin_manager.py`):

1. **Time limit**: when the flag word has 0x4000 and the instance is not stopping, the remaining time decreases by
   0.033 (`f32`); if it is then below 0, it is set back to *T*, the effect is stopped (deferred, section 7) and the
   update ends: this step's actions and any further steps are not run.
2. **Actions**: every action of the list, in list order, with Δ = 0.033 (a `source` as in section 6; the others
   P3, P4).
3. **Frames**: when the flag word has both 0x400 and 0x800, the frame animation (P7).
4. **Collision**: when the flag word has 0x10000 (P5).
5. **Bounding box**: if particles are live, the box spans the minimum and maximum of their positions on each axis,
   enlarged on every side by the largest size component of any live particle (not less than 0); the sphere is the
   box's centre and half its diagonal. If none is live, box and sphere are left as they were (for an instance that
   has had no live particle and no update while not playing, the empty box and the sphere of 8.1). The particle count
   used here is the one after step 2; whether particles removed by step 4 are left out is **unestablished**.
6. **End of a deferred stop**: if the instance is stopping and no particle was counted in 5, playing and stopping
   are cleared and the update ends.

Consequences: the remaining time does not run while stopping, and survives a stop and replay that the time limit
did not cause; an effect stopped by its time limit replays with the full *T*. A time-limited effect, played and
updated without stopping, is stopped by the time limit in step *k*, the first *k* with the remaining time
*T* − *k* · 0.033 (each subtraction in `f32`) below 0; its actions have then run *k* − 1 times
(`data.py`, `sim.py`).

## 9. Verification

### 9.1 Facts on `particles.xr`

(`data.py`, decoding with P1's text and simulating with `sim.py`.)

- The 623 time-limited effects (no limit is 0) stop in step 2 … 60,636; the stop steps sum to 314,659. The most
  common limits: 0.1 (98 effects, step 4), 0.2 (83, step 7), 1 (80, step 31), 0.5 (68, step 16).
- Of the 1,087 `source` records, 392 have rate · 0.033 < 1 (at most one birth per step) and 395 have
  floor(rate · 0.033) ≥ their effect's *N* (the list is filled in the first step); 16 have a non-zero age or age
  spread.
- Placed types: records (of them with action flag bit 0x2): `bounce` 15 (14), `jet` 1 (1), `orbitpoint` 174 (165),
  `randomaccel` 35 (16), `randomdisplace` 33 (33), `randomvelocity` 2 (2), `sink` 89 (84), `source` 1,087 (1,036),
  `targetvelocity` 494 (411), `vortex` 133 (109), `scatter` 15 (15). 1,037 effects hold a placed action with bit
  0x2.

### 9.2 Test vectors

(`data.py`.) *M* below is the rotation by 90° about y with translation (10, 2, −3): rows (0, 0, −1, 0),
(0, 1, 0, 0), (1, 0, 0, 0), (10, 2, −3, 1).

- Update schedule of a playing instance (no time limit, no stop) given *e* = 16, 16, 16, 16, 200, 0, 40 ms: steps
  0, 0, 1, 0, 3, 0, 1.
- `tests\muhi` (P1 8) placed by *M* with **w** = (1, 2, 3): `source` (bit 0x2) position disc **p**₁ (10, 2, −3),
  **p**₂ (0, 1, 0), **u** (0, 0, −1), **v** (−1, 0, 0), *r*₁ – *r*₃ kept; velocity cone unchanged
  (**p**₁ (0, 0, 0), **p**₂ (0, 0.1, 0); its **u**, **v** turn to (0, 0, −1), (−1, 0, 0)); parent velocity
  (0, 0, 0) (parent motion 0); `orbitpoint` centres (10, 2, −3) and (10.3, 2, −3.3); `randomdisplace` sphere
  **p**₁ (0, 0, 0) (without translation); `vortex` centre (10, 5, −3), axis (0, 1, 0).
- Box region **p**₁ (−1, 0, −2), **p**₂ (3, 1, 2) placed by *M*: **p**₁′ (8, 2, −6), **p**₂′ (12, 3, −2); placed by the
  rotation of 0.7853982 rad about y (rows (cos, 0, −sin, 0), (0, 1, 0, 0), (sin, 0, cos, 0), (0, 0, 0, 1)):
  (−2.12132, 0, −3.535534) … (3.535534, 1, 2.12132).
- Containment: sphere **p**₁ (0, 0, 0), *r*₃ 0.09, *r*₄ 0: (0, 0, 0) and (0.3, 0, 0) inside, (0.31, 0, 0) not;
  `tests\muhi`'s velocity cone (*r*₁ 0.3, *r*₂ 0, *r*₄ 99.999992): (0, 0.05, 0), (0.14, 0.05, 0) and the apex
  (0, 0, 0) inside, (0.16, 0.05, 0) and (0, 0.11, 0) not.
- Generator (2.1) from state 1: *k* = 41, 18467, 6334, 26500, 19169.
- `tests\muhi` unplaced (identity, **w** = 0), empty list, generator from state 1, one step of its `source`: *z* =
  0.561, U = 41/32767 gives *n* = 1; the particle: **x** = **x**ₚ = (−0.178081, 0, 0.075190), **w**ₚ =
  (−0.060986, 0.058501, −0.007754), **ρ** = (84.5949, 0.6175, 0.6175), **s** = (0.01, 0.01, 0), colour
  0xFF7F7F7F, age 0; generator state 752,224,798 after these seven draws. The effect's birth notice (3.2; flag
  word 0x1401, 4 frames, P7 3.2) then draws an eighth: *k* = 29358, frame number floor((29358 mod 4) · 255) =
  510, frame flags 0; generator state afterwards 1,924,036,713 (`data.py`, `bin_notice.py`).

## 10. Unestablished

- The starting state of the shared generator and the other draws from it (2.1).
- Placement of a region of a kind above 10 (4.3).
- Whether the bounding box leaves out particles removed by collision in the same step (8.3).
- Whether anything outside this specification and P7 reads an instance's bounding box, which in local mode is in
  the effect's own space (5.1).
  **Disputed in review:** the occlusion test of effect objects (`docs/behavior/occlusion-tests.md` 6) reads it,
  mapped by the object's transform (5.1); whether anything else reads it is unestablished.
- How often owners update and place their instances (the owners' specs).

## 11. Provenance

- Date: 2026-11-24; a new instance's box and sphere (8.1) added 2026-11-26; the box and sphere in local mode (5.1)
  added 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): `particles.xr` from `gamedata.db4`
  (identical copy in `gamedata.dba`); binaries `bin\xrParticles.dll` (MD5 `22bad4d258e6211c6ea7cc8dc8d7ca55`) and
  `bin\xrRender_R1.dll` (MD5 `2bf95c8376b735daa6d82b8944894be0`); for a new instance's box and sphere (8.1),
  `bin\XR_3DA.exe` (MD5 `ba3ad0127bff6619ed991a7c82d8b946`); for the box and sphere in local mode (5.1),
  `bin\xrGame.dll` (MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`).
- Data examined: every effect record of `particles.xr`, decoded with a throwaway parser written from P1's text.
- Analysis: static analysis (Ghidra 12.2, headless) of the particle library (the random numbers, region
  generation, containment and placement, the placement of action lists and of each action type, particle birth,
  addition and removal, play and stop) and of the first renderer's effect instance (its record, placement, play,
  stop and update) and of the engine executable's routines that set a new instance's box and sphere, and, for the
  box and sphere in local mode (5.1), of the first renderer's traversal of drawn objects and of the game library's
  effect object (`bin\xrGame.dll`); the region placement was checked by emulating the program's instructions on
  random input.
  Behaviour was then reproduced by a throwaway implementation written from this text (`sim.py`). No execution or
  debugging of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/particle-lifecycle/` reproduce every fact.
