# Weather ambient effects

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of the particle effects of the weather's **ambient effects**: what each of the 10 effect
sections of `config\weathers\env_ambient.ltx` shows (its `particles`), where it is placed (`offset`), how its
particles live from its start to its removal (the timing of `effect_period` and `life_time` being S4's), and what its
wind (`wind_gust_factor`) changes. It is specification P11 of `docs/behavior/particles-and-lights-scope.md`, and it
settles the gap of `docs/behavior/particle-drawing.md` on the drawing facts of `nature\fog_foggy_00` and
`nature\fog_stormy_01`, the two ambient effects of the first level's start, and the point S4 leaves to P11, when
stopped effect particles count as no longer playing. What could not be established is marked **unestablished**;
what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- S4, `docs/behavior/sound-weather-ambient.md`: the effect sections and their keys (its section 2.3, values in its
  section 5.3), the sets, the clock "now", the shared generator (its section 3.2), the outdoors test, and the
  per-frame rule that chooses an effect, starts it at the eye position *E* plus its `offset`, plays its sound, ends
  it and removes it (its section 4.6). "The rule" below is that rule; "start step", "end step" and "removal step"
  are its three parts.
- P2, `docs/behavior/particle-lifecycle.md`: an effect instance; placement by a matrix *M*, a velocity **w** and a
  mode (its section 5); play, deferred stop and playing (its section 7); the update with *e* ms (its section 8).
- P6, `docs/behavior/particle-groups.md`: the same operations on a group instance (its sections 3 – 5).
- P7, `docs/behavior/particle-drawing.md`: how an effect's particles are drawn and when effect objects are drawn.
- P8, `docs/behavior/level-placed-effects.md`: the particle object the level makes for a record, its creation by
  name (its section 2), its elapsed-time, scheduled and drawing updates and its visibility sphere (its section 5).
- P3 (`particle-motion-actions.md`) and P4 (`particle-appearance-actions.md`): what the actions do in a step.
- `docs/behavior/sky.md`: the current sections A, B and the blend weight *t*, the global time *T* in seconds;
  `docs/formats/particles-xr.md` (P1): effect and group records, the effect flag word.
- `docs/formats/ltx-config.md`: reading the files.

Conventions: matrices in the row-vector convention of `docs/behavior/object-visual-and-transform.md` (rows 0 – 2 are the axes,
row 3 the translation). Arithmetic is IEEE single precision (`f32`) unless stated. Positions are world positions of
the level (metres, +y up). "The effect object" is the particle object the rule makes for one ambient effect.
*W* is the **wind strength** (section 4). Every fact carries a tag naming its check, in
`checks/weather-ambient-effects/` of the dirty workspace: [D*n*] in `data.py` (game data extracted by
`extract.py`, decoded by `pxr.py`, a decoder written from P1's text, and `ltx.py`), [B*n*] in `bin.py`
(static-analysis evidence exported by `export.py`, and the shipped binaries' bytes), [M*n*] in `model.py` (an
implementation of sections 3 – 5 written from this text); `run_all.py` runs them all.

## 1. Overview

- The `particles` key of all 10 effect sections names one of 8 **groups** of `particles.xr`; no ambient effect is a
  single effect record (section 2).
- When the rule starts an ambient effect, it makes an effect object for that group, places it once at *E* +
  `offset` in world axes and plays it (section 3.2). The object is then updated like P8's objects (section 3.3); its
  particles stay where they were born: the effect does not follow the camera.
- When the rule ends the effect, the group is stopped with a deferred stop: no more particles are born, the live
  ones run out (section 3.4). As soon as nothing of it plays, the rule asks for its removal; it is deleted at the
  start of the next frame (section 3.5).
- While an effect is active, its `wind_gust_factor` is the **gust** in force (S4). The gust sets how fast a slowly
  varying noise moves; the noise gives the wind strength *W* (0 … 1), which tilts falling rain drops and blends the
  sway of the level's detail objects (grass) between a calm and a windy set of values (sections 4, 5). The wind
  does not act on particles.

## 2. The effects

### 2.1 Groups

The 10 effect sections (S4 section 5.3) give 8 distinct `particles` names; each is a group, none an effect, and no
group has a time limit (group chunk 5 = 0) [D2]. Their members (P1 section 6.1; names after
`nature\group_items\`), all with start time 0, stop time 0 and no child names [D2]:

| Group | Effect sections | Members (member flag word) | Live particles at most |
|---|---|---|---|
| `nature\fog_foggy_00` | `ae0_effect_4` | `fog_foggy_dust` (0x4) | 18 |
| `nature\fog_stormy` | `ae0_effect_0` | `fog_stormy_dust_01` (0x4), `fog_stormy_leaves_01` (0x4) | 26 |
| `nature\fog_stormy_00` | `ae0_effect_8` | `fog_stormy_dust_00` (0x4), `fog_stormy_leaves_02a_00` (0x4) | 76 |
| `nature\fog_stormy_01` | `ae0_effect_6`, `ae0_effect_9` | `fog_stormy_dust_02` (0x4), `fog_stormy_dust` (0x4), `fog_stormy_dust_02` (0x0) | 26 |
| `nature\fog_stormy_02` | `ae0_effect_1`, `ae0_effect_7` | `fog_stormy_dust_02` (0x4), `fog_stormy_leaves_02` (0x4), `fog_stormy_leaves_02a` (0x4) | 74 |
| `nature\fog_tornado_00` | `ae0_effect_3` | `tornado_01_dust_01` (0x4), `tornado_01_leaves_01` (0x4), `vortex_dust_01` (0x4) | 64 |
| `nature\fog_tornado_01` | `ae0_effect_2` | `vortex_dust_01` (0x4), `vortex_leaves_01` (0x4) | 46 |
| `nature\vortex_01` | `ae0_effect_5` | `vortex_dust_01` (0x4), `vortex_leaves_01` (0x4) | 46 |

("Live particles at most" sums the particle maxima of the members with flag bit 0x4 [D9].) By P6: every member
with bit 0x4 starts in the group's first update after play; the third member of `nature\fog_stormy_01` (flag word
0) is made and placed but never played (P6 section 7); with stop time 0 and the start's immediate 33 ms update
(section 3.2) no member is stopped by its stop time (P6 section 5.6); a group without a time limit plays until it is
stopped.

### 2.2 Member effects

The 13 distinct member effects (P1 sections 2, 3; names after `nature\group_items\`) [D3, D5]:

| Effect | Flag word | Maximum | Texture | Frames (width, height, per row, count) | Actions in list order |
|---|---|---|---|---|---|
| `fog_foggy_dust` | 0x1001 | 18 | `pfx\pfx_smoke_b` | — | source, killold, sink, targetcolor, targetrotate, move |
| `fog_stormy_dust` | 0x11401 | 8 | `pfx\pfx_stonedebris2` | 0.25, 0.25, 4, 16 | source, killold, sink, targetrotate, targetcolor, gravity, move |
| `fog_stormy_dust_00` | 0x1001 | 48 | `pfx\pfx_smoke_b` | — | source, killold, sink, targetcolor, targetrotate, vortex, targetsize, move |
| `fog_stormy_dust_01` | 0x1001 | 8 | `pfx\pfx_smoke_b` | — | source, killold, sink, targetcolor, targetrotate, vortex, move |
| `fog_stormy_dust_02` | 0x31001 | 18 | `pfx\pfx_smoke_b` | — | source, killold, sink, targetcolor, targetrotate, vortex, move |
| `fog_stormy_leaves_01` | 0x20401 | 18 | `pfx\pfx_leaves_01` | 0.5, 0.5, 2, 4 | source, killold, sink, targetsize, targetrotate, vortex, targetcolor, move |
| `fog_stormy_leaves_02` | 0x20401 | 28 | `pfx\pfx_leaves_01` | 0.5, 0.5, 2, 4 | source, killold, sink, targetsize, targetrotate, vortex, targetcolor, move |
| `fog_stormy_leaves_02a` | 0x21401 | 28 | `pfx\pfx_leaves_01.tga` | 0.5, 0.5, 2, 4 | source, killold, sink, targetsize, targetrotate, vortex, move |
| `fog_stormy_leaves_02a_00` | 0x21401 | 28 | `pfx\pfx_leaves_01.tga` | 0.5, 0.5, 2, 4 | source, killold, sink, targetsize, targetrotate, vortex, move |
| `tornado_01_dust_01` | 0x3401 | 18 | `pfx\pfx_smokepuffs2` | 0.25, 0.5, 4, 8 | source, killold, sink, targetcolor, targetrotate, vortex, move |
| `tornado_01_leaves_01` | 0x1401 | 28 | `pfx\pfx_leaves_01` | 0.5, 0.5, 2, 4 | source, killold, sink, targetrotate, vortex, move |
| `vortex_dust_01` | 0x1001 | 18 | `pfx\pfx_smoke_b` | — | source, killold, targetcolor, targetrotate, randomaccel, targetrotate, vortex, move |
| `vortex_leaves_01` | 0x1401 | 28 | `pfx\pfx_leaves_01.tga` | 0.5, 0.5, 2, 4 | source, killold, targetcolor, randomaccel, targetrotate, vortex, move |

- None has a time limit (flag bit 0x4000 clear), so each is looped (P2 section 7) and ends only by a stop [D3].
- Each has one `source` (action flag word 0x80000002 or 0xA0000002, so it follows the effect's placement, P2 section
  5.2) and one `killold` with kill-younger 0 and age limit 5 s, except `fog_stormy_dust` and `fog_stormy_dust_00`
  with 2 s; `move` is last in every list; every `sink` and `vortex` has action flag bit 0x2 [D3]. The frame rate of
  every chunk 6 is 24 [D3].
- The four textures exist as `textures\pfx\pfx_leaves_01.dds`, `pfx_smoke_b.dds`, `pfx_smokepuffs2.dds` and
  `pfx_stonedebris2.dds` (the `.tga` ending is dropped as in static-shading.md section 4.1) [D4].

### 2.3 Drawing facts

By P7 and the flag words of 2.2 [D3, D5]:

- All 13 have a sprite and the engine shader `particles\blend`: one pass blending source alpha over inverse source
  alpha, no depth write, no fog, alpha test A · 255 > 0 (P7 section 5.2); each member effect is queued in the
  frame's strict-sorted list and drawn back to front with the other strict-sorted passes (P7 sections 7.2 – 7.4).
- None has flag bit 0x8000, 0x40000, 0x400000 or 0x800000: every sprite faces the camera (P7 section 4.3, case
  "facing"), is not stretched with speed and is drawn without face culling.
- None has bit 0x800, so no frame animation runs. With frames (bit 0x400): `fog_stormy_dust`,
  `fog_stormy_leaves_02a`, `fog_stormy_leaves_02a_00`, `tornado_01_dust_01`, `tornado_01_leaves_01` and
  `vortex_leaves_01` also have 0x1000 and start each particle on a random frame (P7 section 3.2);
  `fog_stormy_leaves_01` and `fog_stormy_leaves_02` have frames without 0x1000 and show frame 0, the rectangle
  (0, 0) – (0.5, 0.5), throughout. The others show the whole texture. Bit 0x2000 of `tornado_01_dust_01` does nothing without
  0x800, and bit 0x20000 without 0x10000 (in the four leaves effects) does nothing (P1 section 3).
- `fog_stormy_dust` (chunk 33: 0.95, 0.25, 0) and `fog_stormy_dust_02` (chunk 33: 1, 0, 0, and bit 0x20000) collide
  with the level (P1 section 2.3); what collision does is specification P5's, not covered by an approved
  specification (**unestablished** here).

The start's two effects (S4 section 5.1; set `ambient_env_tuman`: `ae0_effect_4` and `ae0_effect_6`, set
`ambient_env_morning`: `ae0_effect_4`, both with offset (0, 0, 0)) [D8]: `nature\fog_foggy_00` is one smoke effect
(`fog_foggy_dust`, `pfx\pfx_smoke_b`, whole texture, up to 18 sprites); `nature\fog_stormy_01` is two played smoke
and debris effects (`fog_stormy_dust_02`, `pfx\pfx_smoke_b`, whole texture, up to 18, colliding and removed on
collision; `fog_stormy_dust`, `pfx\pfx_stonedebris2`, 16 frames of 0.25 × 0.25 in 4 rows of 4 with a random frame
each, up to 8, colliding).

## 3. The effect object

### 3.1 Making it

At the start step (S4 section 4.6, its step 5) the rule makes one particle object of the same kind as P8's, from the
effect's `particles` name: the name is looked up among the library's effects, then among its groups (P8 section 2;
all shipped names are groups, section 2.1) [B1, B4, D2]. It is made with automatic removal off, so the object never
removes itself when a length runs out; it removes itself only by the removal of section 3.5 [B1, B4]. On being
made it is registered with the engine's scheduler exactly as P8's objects (P8 section 5.2) [B4].

### 3.2 The start

Right after it is made, in the same start step [B2, B3]:

1. The group is placed (P6 section 3) by the matrix *M* with rows 0 – 2 the identity and row 3 the position
   *E* + `offset` (S4 section 4.6), with velocity **w** = (0, 0, 0), in world mode. So the group's position becomes
   *E* + `offset` and every member effect is placed by *M*: the regions and centres of its actions with action flag
   bit 0x2 (all `source`, `sink` and `vortex` actions here, section 2.2) are moved by *E* + `offset` and keep their
   stored directions (P2 section 5.2).
2. The group is played (P6 section 3).
3. Its last-update time is set to the clock minus 33 and it is updated at once with 33 ms (P8 section 5.1): with
   start time 0, every member with bit 0x4 is played and simulated for one step in this update (P6 section 5.2).
   Its visibility sphere is then registered (P8 section 5.4).

The object is never placed again [B2]: its particles are born where the group was placed and move only by their
actions (P3); a player who walks away leaves the fog behind.

### 3.3 Updates

The effect object is updated exactly as P8's objects: the elapsed-time update of P8 section 5.1, run by the
scheduler with the period of P8 section 5.2 (30 ms near the camera, up to 525 ms at 200 m, by the distance from the
camera to the centre of the group's bounding sphere), and by drawing in every frame in which it is drawn (P8 section
5.3; which frames draw it: P7 section 7.1), with its visibility sphere kept as in P8 section 5.4 [B4]. A group far
from the camera and not in view is therefore simulated more slowly than real time (P8 section 5.2).

### 3.4 The end

At the end step (S4 section 4.6), in every frame in which it applies while the object exists, the group is stopped
with a deferred stop (P6 section 3: stopping set, every member stopped deferred, so their `source` actions are
switched off and their live particles run on) [B5, B3]. A repeated deferred stop changes nothing observable
(believed from P2 section 7 and P6 section 3). The
particles then end by their actions: by `killold` once their age reaches 5 s (2 s in `fog_stormy_dust` and
`fog_stormy_dust_00`), earlier by `sink` (P4 section 4) or by collision (P5) (believed from P4 and the data of 2.2;
the actual times depend on the updates of 3.3).

### 3.5 No longer playing, and removal

The group stops playing in the first update (scheduled or by drawing, section 3.3) in which no member effect and no
child of it is still playing (P6 section 5.2 step 6); a member effect stops playing in the update in which its last
particle is removed after its deferred stop (P2 section 8.3 step 6). Since no group or member effect of the ambient
effects has a time limit (section 2), an ambient effect never stops playing before the end step; this settles
S4's "when stopped effect particles count as no longer playing" [B3, B6, D2, D3].

At the removal step (S4 section 4.6), in every frame, the rule tests whether the object's group is playing; if it is
not, the rule asks the object to be destroyed and forgets it, so a new effect may start in a later frame [B6]. The
request marks the object destroyed (its scheduled updates no longer update the group) and queues it; queued objects
are deleted (unregistered from the scheduler, their sphere removed, their group destroyed) at the start of the next
frame's game update, before the scheduler runs and before the rule [B4, B6]. An object is deleted unless it is
locked, and the start (3.2) leaves it unlocked [B6].

### 3.6 Order within a frame

Each frame of a running level, in this order [B6]: (1) the environment is updated (not while paused, section 4.3),
including the wind strength, and queued effect objects are deleted (3.5); (2) when the game is not paused, the
scheduler runs, updating effect objects that are due (3.3); (3) when the game is not paused, the rule runs (start,
end, removal). The drawing of the frame, with its drawing updates, follows (P7, P8 section 5.3) (believed: the
drawing is not part of the examined frame update).

## 4. Wind

### 4.1 The gust

The gust in force is a value of the environment, 0 when the environment is made [B7]. At the start step the rule
sets it to the effect's `wind_gust_factor`; at the end step it sets it to 0 (S4 section 4.6) [B7, B5]. The 10
effect sections hold 0.01 … 0.03 (S4 section 5.3).

### 4.2 The wind noise

The environment keeps one one-dimensional gradient noise (the public algorithm known as Perlin noise) with this
state, set when the environment is made [B8]:

| State | Value when made |
|---|---|
| seed *s* | *r* mod 65,535, *r* one draw of the shared generator (S4 section 3.2), so *s* = *r* (0 … 32,767) |
| built | no |
| tables *P* (256 integers) and *G* (256 `f32`) | not yet built |
| octave offsets *o*₀, *o*₁ (`f32`) | 0, 0 |
| frequency φ (`f32`) | 1 (replaced in every environment update, 4.3) |
| amplitude *A* (`f32`) | 0.66666 (`f32` 0x3F2AAA3B) |
| last time τ (`f32`) | 0 |

**Building the tables**, at the first evaluation of the noise [B9]: the C runtime library's generator
(`msvcr80.dll`; its state *q*, 32 bits: `srand(x)` sets *q* = *x*; each `rand()` sets *q* ← *q* · 214,013 +
2,531,011 mod 2³² and returns (*q* ≫ 16) AND 32,767) is seeded with *s*; then

1. for *i* = 0 … 255: *P*[*i*] = *i* and *G*[*i*] = ((`rand()` mod 512) − 256) · 0.00390625, a value −1 … 0.99609375;
2. for *i* = 255 down to 1: *j* = `rand()` mod 256; *P*[*i*] and *P*[*j*] are exchanged.

The C runtime's generator is shared with other users in the program (e.g. P2 section 2.2); seeding it here changes
their later values too. **Disputed in review:** the C runtime's `rand` and `srand` keep their state per thread (both exports
first fetch the calling thread's data block and read or write the state there), so seeding here changes the later
values only of users on the same thread; which users share the environment's thread is unestablished. Since the seed comes from the shared generator, whose start differs from run to run (S4
section 3.2), the tables differ from run to run: an implementation may use any seed and any uniform generator
(believed: nothing else depends on the exact tables).

**One noise value** *n*(*x*) [B10]: *y* = *x* + 4096 (`f32`); *i* = *y* truncated toward zero to an integer;
*f* = *y* − *i* (`f32`); *i* ← *i* mod 256; *g*₀ = *G*[*P*[*i*]], *g*₁ = *G*[*P*[(*i* + 1) mod 256]];
*u* = *g*₀ · *f* (`f32`), *v* = (*f* − 1) · *g*₁, *h* = (3 − 2*f*) · *f*²; *n* = *u* + *h* · (*v* − *u*) (the products
other than *u* in the processor's extended precision). So *n* = 0 at every integer *x*, and |*n*| ≤ 0.5 [M2].

**The continuous value** *V*(*T*) at time *T* [B11]: *d* = φ · (*T* − τ) (`f32`; while τ = 0 the program uses *T*
itself, which is the same); τ ← *T*; then for octave *k* = 0, 1: *o*ₖ ← *o*ₖ + *d* (`f32`); the sum grows by
*A*ₖ · *n*(*o*ₖ), with *A*₀ = *A* and *A*₁ = *A* · 0.5, and is rounded to `f32`; *d* ← 2*d*. *V* is the sum.

### 4.3 The wind strength

In every environment update (section 3.6; only while a level is loaded, and not while the game is paused unless an
engine flag is set, what sets it being **unestablished**) [B12]:

1. φ ← gust · 0.03 (`f32` 0x3CF5C28F), the gust being the one in force then, i.e. as the previous frame's rule left
   it;
2. *W* ← clamp(*V*(*T*) + 0.5, 0, 1), with *T* the global time in seconds (`docs/behavior/sky.md`; the seconds of the
   clock whose milliseconds are S4's "now").

*W* is 0 when the environment is made [B7]; nothing else of this specification changes it (zones, section 4.4).

Consequences [M3, M4]:

- The noise moves only while a gust is in force: octave 0's offset advances by 0.03 · gust per second, octave 1's
  twice as fast. With the gust 0 the offsets, and so *W*, stay where they are.
- Before the first gust of a run the offsets are 0 and *n*(0) = 0, so *W* = 0.5 exactly; at the first level's start
  *W* is 0.5.
- The shipped gusts are small: the start's 15 s effect `ae0_effect_4` (gust 0.015) advances octave 0 by 0.00675,
  changing *W* by at most about 0.01 (from the formulas of 4.2; 0.0014 for seed 12345, M3); *W* drifts noticeably only over hours of play with effects (M4: one hour at gust
  0.03 keeps *W* within 0.41 … 0.73 for one seed).
- *W* is kept for the whole run, across levels (believed: the environment is made once per run).

### 4.4 What uses the wind strength

Besides the environment's own update (4.3), of all the program's code that reaches the environment through the
engine's global game object only these read or write *W* or the gust [B13, B16]:

| Use | Specification |
|---|---|
| the rule sets the gust | S4, section 4.1 |
| a falling rain drop's direction at its birth reads *W* | section 5.1 |
| the detail objects' sway values read *W* | section 5.2 |
| anomaly zones write *W* in their own update | F1 of the scoping specification (what they write is outside this specification) |

The particle library imports nothing but the core library and the runtime and reaches no environment value: wind
does not move particles [B13].

## 5. The wind's consumers

### 5.1 Rain drops

Each new rain drop gets, when it is born, the mean direction **d** [B14]:

- *V*ₐ = the wind velocity of the environment: `wind_velocity` of A and B blended as *V*_A · (1 − *t*) + *V*_B · *t*,
  without the environment modifiers; *w*_d = `wind_direction` of A and B (degrees × 0.017453292, then radians)
  blended the same way;
- σ = clamp(*W* · 0.1 · *V*ₐ · 0.05, 0, 1) (`f32`, in that order);
- *a* = 0.17453294 · σ − 1.5707964 (radians, so 10° · σ above straight down);
- **d** = (−sin *w*_d · cos *a*, sin *a*, cos *w*_d · cos *a*).

So the drops fall straight down at σ = 0 and lean by up to 10° toward the horizontal direction (−sin *w*_d, 0,
cos *w*_d), +z for direction 0. The drop's direction is then varied randomly around **d** with the angle
0.05235988 rad (3°) [B14]; that variation and everything else of the rain's drawing are outside this
specification (no approved specification covers rain drawing; `docs/behavior/sky.md` section 11).

Data [D6]: of the 123 hourly sections, `wind_velocity` is 0 in 59, 20 in 58, 10 in 5 and 100 in 1 (`stancia2`,
12:00); `wind_direction` is 0 in all. Both start sections have velocity 0: at the start σ = 0. Rain falls with wind
in 9 sections (`default` 12:00, 16:00; `rain` 05:45, 06:15, 09:00, 10:00, 12:00, 14:00, 22:00); with *W* = 0.5 and
velocity 20, σ = 0.05: a tilt of 0.5° toward +z [M5].

### 5.2 Detail objects

Section `[details]` of the configuration holds two sets of five sway values (keys `swing_normal_*`, the calm set,
and `swing_fast_*`, the windy set) [B15, D7]:

| Value | `swing_normal_` | `swing_fast_` | At *W* = 0.5 |
|---|---|---|---|
| `amp1` | 0.1 | 0.35 | 0.225 |
| `amp2` | 0.05 | 0.20 | 0.125 |
| `rot1` | 30.0 | 5 | 17.5 |
| `rot2` | 1.0 | 0.5 | 0.75 |
| `speed` | 2.0 | 0.5 | 1.25 |

(The shipped lines of the fast set hold a second number after a `;`, removed as a comment, ltx-config.md.) Each
time the first renderer runs its detail-object pass (when the level has detail objects and the device flag 0x40 is
set; what sets that flag is **unestablished**; that this pass draws the detail objects is believed from the keys it
uses), each of the five values in use becomes *normal* · (1 − *W*) + *fast* · *W* [B15, M6].
How the five values move the detail objects is the detail objects' drawing, which no approved specification covers
(**unestablished** here).

## 6. Sounds

An ambient effect's sound starts at the effect's start, at the same position *E* + `offset`, and plays to its own
end, unaffected by the end and removal of the effect object (S4 sections 4.6, 6).

## 7. Verification

### 7.1 Data

- `particles.xr` in `gamedata.db4` and `gamedata.dba`: 1,394,071 bytes, CRC-32 0x4E93C6F1, identical [D1].
- The tables of sections 2.1 and 2.2, decoded from `particles.xr` [D2, D3, D5, D9]; the textures [D4]; the start's
  sets and effects [D8]; the wind keys [D6]; `[details]` [D7].
- Source rates (particles per second): `fog_foggy_dust` 8, `fog_stormy_dust` 12, `fog_stormy_dust_00` 18,
  `fog_stormy_dust_01` 37, `fog_stormy_dust_02` 37, `fog_stormy_leaves_01` 50, `fog_stormy_leaves_02` 18,
  `fog_stormy_leaves_02a` 28, `fog_stormy_leaves_02a_00` 28, `tornado_01_dust_01` 49, `tornado_01_leaves_01` 150,
  `vortex_dust_01` 17, `vortex_leaves_01` 17 [D3].

### 7.2 Test vectors

Computed by `model.py` from sections 3 – 5 (not measured in the game) [M1 – M7]:

- Tables for seed 12345 (C runtime generator seeded with 12345): *P*[0 … 7] = 140, 206, 209, 109, 73, 1, 184, 167;
  *G*[*P*[0 … 7]] = −0.14453125, 0.26953125, 0.46484375, −0.9375, −0.90625, −0.140625, 0.984375, 0.28515625;
  *G* ranges over −0.98828125 … 0.984375; SHA-256 of the text `P[0],…,P[255]|G[0],…,G[255]` (decimal integers,
  shortest round-trip decimal floats, comma-separated):
  `7337863be3df8ecbcab4c614324240af319d092fc0440393281b9dbdcbf5895a` [M1].
- With those tables, *n*(*x*) for *x* = 0, 0.25, 0.5, 1, 1.5, 2.75: 0, −0.062073, −0.103516, 0, −0.048828, 0.252228
  [M2].
- Frames every 20 ms from *T* = 100 s (*T* as `f32`), seed 12345: 50 frames without gust, 750 frames with the gust
  0.015 in force, then 250 without: *W* = 0.5 through the 51st frame; 0.498609 at the end of the gust; 0.498609
  in every later frame; offsets 0.00675 and 0.0135 [M3].
- The same with the gust 0.03 for 180,000 frames (one hour): *W* within 0.4144 … 0.7338; offsets 3.336 and 6.672
  (the `f32` sums of the small steps) [M4].
- Rain direction (5.1): *W* 0.5, velocity 20, direction 0: σ = 0.05, **d** = (0, −0.999962, 0.008726); *W* 1,
  velocity 100, direction 90°: σ = 0.5, **d** = (−0.087156, −0.996195, 0), a 5° tilt toward −x; velocity 0:
  **d** = (0, −1, 0) [M5].
- Detail values (5.2): the column "At *W* = 0.5"; at *W* = 0 the calm set, at 1 the windy set [M6].
- The start's first effect, `nature\fog_foggy_00` at *E* = (−246.72630, −23.263261, −134.43387) with offset 0:
  `fog_foggy_dust`'s `source` position box becomes (−256.7263, −23.2633, −136.4339) … (−236.7263, −21.2633,
  −119.4339), and its `sink` plane (kill inside) gets *r*₁ = −24.01326, so its particles at *y* ≤ −24.01326 (0.75 m
  below the eye) are removed (P2 sections 4.2, 4.3, 5.2; P4 section 4) [M7].

## 8. Unestablished

- What collision does to the particles of `fog_stormy_dust` and `fog_stormy_dust_02` (P5; section 2.3).
- The random variation of a rain drop's direction around **d** and the rest of the rain's drawing (5.1); how the
  sway values move the detail objects, and what sets the device flag 0x40 (5.2).
- What the zones write into *W* (F1; 4.4).
- The engine flag that lets the environment update while paused (4.3).
- Whether the gust is reset when a level is left while an effect is active (4.1; believed not, since only the rule
  changes it and the rule runs only with a level); that the environment, and so the noise, is made once per run
  (4.3, believed).
- The exact times at which the stopped particles end (3.4), which follow from the updates of 3.3 and P2 – P5.

## 9. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives `gamedata.db0` … `gamedata.dbd`
  (`particles.xr` from `gamedata.db4`, identical in `gamedata.dba`; every `config\*.ltx` file of the merged view);
  binaries `bin\XR_3DA.exe` (MD5 `ba3ad0127bff6619ed991a7c82d8b946`), `bin\xrGame.dll`
  (`19938b4b9fa11e3b4b6462ce7e7a12ac`), `bin\xrRender_R1.dll` (`2bf95c8376b735daa6d82b8944894be0`),
  `bin\xrParticles.dll` (`22bad4d258e6211c6ea7cc8dc8d7ca55`; import table and a text search only), `bin\msvcr80.dll`
  (the bytes of its `rand` and `srand` exports).
- Data examined: the 10 effect sections and 11 sets of `env_ambient.ltx`, the 123 hourly sections of the weathers,
  `[details]` of `system.ltx`, the 8 groups and 13 member effects named, decoded with throwaway parsers written from
  P1's and ltx-config.md's text; the archive listing for the textures.
- Analysis: static analysis (Ghidra 12.2, headless; decompiles and instruction listings) of the ambient rule's
  effect object (creation, placement, play, stop, playing test, removal), the particle object's scheduled and drawing
  updates and deletion, the game's frame order, the environment's construction and frame update, the noise, the
  loading of the effect sections and of the wind keys, the rain drop's birth, the detail objects' sway values, the
  group and effect interfaces of the first renderer, and a scan of every function that reaches the environment
  through the engine's global game object in the three programs; reading constants. The noise and its consequences were
  reproduced by a throwaway implementation written from this text (`model.py`). No program was executed.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/weather-ambient-effects/` reproduce every fact.

## Gaps found in review

- The deletion of queued effect objects (3.5) stops at the first locked object in the queue: that object and those
  after it stay queued for a later frame; the ambient effect objects are never locked, so this affects them only
  when queued behind another owner's locked object.
