# Artefact idle effects and lights

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of what an artefact shows by itself: for every artefact section (class id `ARTEFACT` or
`SCRPTART`) and every `spawns\all.spawn` record of one, whether and how the artefact shows the effect its section names
(key `particles`): where it is placed, when it starts and stops, lying or carried; and its light (keys
`lights_enabled`, `trail_light_color`, `trail_light_range`): when it is on, its colour, range and position. It also
states the artefact's states that the sounds of its activation (specification S16 of
`docs/behavior/level-sound-scope.md`) follow, and the activation's timing; the sounds themselves are S16's. It is
specification P10 of `docs/behavior/particles-and-lights-scope.md`. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/behavior/particle-lifecycle.md` (P2): an effect instance, its placement by a matrix *M*, a velocity **w** and a
  mode (section 5), play and stop (section 7), its update with elapsed milliseconds (section 8).
- `docs/behavior/particle-groups.md` (P6): the same for a group; every effect an artefact names is a group (section
  9.1); a group without a time limit stays playing until its owner stops it (P6 5.6).
- `docs/behavior/zone-idle-effects.md` (P9), section 4.5: the game's **effect object** that owns one renderer
  instance, made from a name (effects first, then groups), played with an immediate 33 ms update, updated by its own
  schedule and when drawn. The artefact's effect objects are made with the same creation settings (section 4.2).
- `docs/behavior/level-placed-effects.md` (P8), section 5.4: the visibility sphere an effect object registers after a
  placement.
- `docs/behavior/dynamic-lights.md` (P16): the light record (section 2: default type point, default range 8, the
  rules by which a stored value changes, colour given as (R, G, B) storing A = 1), which lights are drawn and how they
  light the world. `docs/behavior/glows.md` (P15): the sources of glows (artefacts are not among them, section 5.1).
- `docs/formats/lanims-xr.md` (P12): not used; an artefact's light has no colour animation (section 5.1).
- `docs/behavior/level-objects.md`: which objects are online at the start (section 2, including the scripted
  day-and-night rule for three `SCRPTART` sections, 2.5), the object transform *X* (3.4), visual types (4.1), bones
  and bind matrices (4.4), the current model-space bone matrix *T* and the pose at rest (4.8).
- `docs/formats/skeletal-motions.md`: a bone's *T* under a motion at a time.
- `docs/formats/ogf.md`: the visual header with its bounding sphere.
- `docs/formats/sav.md`: the record layouts; `docs/formats/ltx-config.md`: key lookup with inheritance, booleans,
  reading a missing key is fatal.
- `docs/behavior/player-state.md` section 3.3: where an item the player takes is placed (slots, belt, backpack).

Conventions: matrices in the row-vector convention of level-objects.md (row 3 of *X* is the position **p**; a point
**q** is placed as [**q** 1] · *X*). Arithmetic is IEEE single precision (`f32`) unless stated. "ms" are milliseconds;
the clock is the device clock of zone-idle-effects.md (*C* in whole milliseconds). *E* is the camera position of the
frame. "Single-player game" is the game type `single` (the only one this specification needs; section 4.6 says what
other game types change). Every fact carries the name of its check, a script in `checks/artefact-idle-effects/` of
the dirty workspace; `run_all.py` runs them all (`ghidra.py`, run first, prepares the static analysis the `bin_*.py`
checks read; `data.py` reads the game data; `model.py` computes the worked values from this text).

## 1. Sections and records

### 1.1 Class ids

29 sections have class id `ARTEFACT` (25) or `SCRPTART` (4) (`data.py` D1). The two ids have two in-game behaviours
that differ only in the per-update work of section 6.4 and in one key (`jump_height`, read by `ARTEFACT` only)
(`bin_artefact.py` A1, A2, A3):

- `ARTEFACT`: the **jumping** behaviour (section 6.4).
- `SCRPTART`: the **plain** behaviour. The program registers this id itself only for a dedicated server; in a
  normal game the shipped `scripts\class_registrator.script` registers it, with the name under which the program
  exposes the plain behaviour to scripts and with the server-side script object of `se_artefact.script` (A1;
  `data.py` D8). For the three sections named in level-objects.md 2.5 that script object decides when the record is
  online; it does not touch the effect or the light.

Everything else in this specification holds for both ids.

### 1.2 Keys read

When the artefact object is made from its section (A3):

| Key | Kind | Read | Meaning |
|---|---|---|---|
| `particles` | string | if present | the idle effect: a name of `particles.xr` |
| `lights_enabled` | boolean | always (missing: fatal) | whether the artefact has a light |
| `trail_light_color` | three numbers separated by commas | when `lights_enabled` is on | the light's red, green and blue |
| `trail_light_range` | float | when `lights_enabled` is on | the light's range (m) |
| `jump_height` | float | `ARTEFACT` only, if present (else 0) | section 6.4 |

The artefact also notes whether its section is a key of `[artefact_spawn_zones]` (`config\misc\artefacts.ltx`); this
matters only for activation (section 7). No shipped script names `lights_enabled`, `trail_light_color`,
`trail_light_range` or `artefact_activation_seq` (D8).

### 1.3 The sections

Every section, with its records in `spawns\all.spawn` (all levels), the last part of its effect name (all are
`anomaly2\artefact\…`), its light (`trail_light_color`, `trail_light_range`, or off), its model
(`physics\anomaly\…`), the model's visual type, the model-space position of the model's bone 0 (section 4.1) and
`jump_height` (D11; the rows reproduce `table.md5`):

| Section | Class | Records | Effect | Light (R, G, B, range) | Model | Type | Bone 0 position | `jump_height` |
|---|---|---|---|---|---|---|---|---|
| `af_medusa` | `ARTEFACT` | 7 | `artefact_gravi` | off | `artefact_blackdrip` | 10 | (0.0005, 0.0390, -0.0001) | .5 |
| `af_cristall_flower` | `ARTEFACT` | 6 | `artefact_gravi` | 0.6, 0.7, 0.5, 0.5 | `artefact_blackdrip_2` | 10 | (0.0005, 0.0390, -0.0001) | .4 |
| `af_night_star` | `ARTEFACT` | 2 | `artefact_gravi` | 0.6, 0.7, 0.5, 3.0 | `artefact_blackdrip_1` | 10 | (0.0005, 0.0390, -0.0001) | .2 |
| `af_vyvert` | `ARTEFACT` | 11 | `artefact_gravi` | off | `artefact_gravy1` | 3 | (0.0005, 0.0390, -0.0001) | .3 |
| `af_gravi` | `ARTEFACT` | 2 | `artefact_gravi` | 0.6, 0.7, 0.5, 0.5 | `artefact_gravy2` | 3 | (0.0005, 0.0390, -0.0001) | .3 |
| `af_gold_fish` | `ARTEFACT` | 2 | `artefact_gravi` | 0.6, 0.7, 0.5, 2.0 | `artefact_gravy3` | 3 | (0.0005, 0.0390, -0.0001) | .2 |
| `af_drops` | `ARTEFACT` | 0 | `artefact_zharka` | 0.6, 0.7, 0.5, 0.5 | `artefact_kaply` | 10 | (-0.0179, 0.0612, 0.1001) | .1 |
| `af_fireball` | `ARTEFACT` | 1 | `artefact_zharka` | 0.9, 0.5, 0.5, 3.0 | `artefact_fire` | 3 | (0, 0, 0) | — (0) |
| `af_cristall` | `ARTEFACT` | 0 | `artefact_zharka` | 0.9, 0.5, 0.5, 3.0 | `artefact_cristall` | 10 | (-0.0065, 0.3792, -0.1212) | .1 |
| `af_blood` | `ARTEFACT` | 6 | `artefact_mincer` | off | `artefact_myasorubka1` | 3 | (0.0005, 0.0390, -0.0001) | .01 |
| `af_mincer_meat` | `ARTEFACT` | 1 | `artefact_mincer` | off | `artefact_myasorubka2` | 3 | (0.0005, 0.0390, -0.0001) | .1 |
| `af_soul` | `ARTEFACT` | 0 | `artefact_mincer` | off | `artefact_myasorubka3` | 3 | (0.0005, 0.0390, -0.0001) | .1 |
| `af_electra_sparkler` | `SCRPTART` | 5 | `artefact_electra` | 0.5, 0.5, 0.9, 0.50 | `artefact_electra4` | 3 | (0.0005, 0.0390, -0.0001) | (.5, not read) |
| `af_electra_flash` | `SCRPTART` | 6 | `artefact_electra` | 0.6, 0.7, 0.9, 2.5 | `artefact_electra3` | 3 | (0, 0.1019, 0) | (.1, not read) |
| `af_electra_moonlight` | `SCRPTART` | 4 | `artefact_electra` | 0.6, 0.7, 0.9, 1.0 | `artefact_electra1` | 3 | (0, 0.1485, -0.0011) | (.1, not read) |
| `af_ameba_slime` | `ARTEFACT` | 0 | `artefact_ameba` | off | `artefact_ameba3` | 3 | (0.0005, 0.0390, -0.0001) | .0 |
| `af_ameba_slug` | `ARTEFACT` | 1 | `artefact_ameba` | 0.6, 0.6, 0.5, 1.0 | `artefact_ameba2` | 3 | (0.0005, 0.0390, -0.0001) | .1 |
| `af_ameba_mica` | `ARTEFACT` | 1 | `artefact_ameba` | 0.6, 0.7, 0.6, 1 | `artefact_ameba1` | 3 | (0.0005, 0.0390, -0.0001) | .1 |
| `af_dummy_spring` | `ARTEFACT` | 3 | — | off | `artefact_pustishka` | 3 | (0.0005, 0.0390, -0.0001) | .2 |
| `af_dummy_dummy` | `ARTEFACT` | 3 | — | off | `artefact_pustishka1` | 3 | (0.0005, 0.0390, -0.0001) | .0 |
| `af_dummy_battery` | `ARTEFACT` | 3 | `artefact_gravi` | off | `artefact_battery` | 3 | (0, 0, 0) | .2 |
| `af_dummy_pellicle` | `ARTEFACT` | 1 | `artefact_ameba` | off | `artefact_plenka` | 10 | (-0.3000, 0, 0) | .2 |
| `af_dummy_glassbeads` | `ARTEFACT` | 1 | `artefact_mincer` | off | `artefact_spiral` | 10 | (-0.2094, 0.0965, 0.0127) | .2 |
| `af_rusty_thorn` | `ARTEFACT` | 0 | — | off | `artefact_needles1` | 3 | (0, 0.2369, 0) | .3 |
| `af_rusty_kristall` | `ARTEFACT` | 0 | — | off | `artefact_needles2` | 3 | (0, 0.2369, 0) | .4 |
| `af_rusty_sea-urchin` | `ARTEFACT` | 1 | — | off | `artefact_rusty_hairs` | 3 | (0, 0.2369, 0) | .5 |
| `af_fuzz_kolobok` | `ARTEFACT` | 2 | `artefact_puxx` | off | `artefact_rusty_hairs` | 3 | (0, 0.2369, 0) | .5 |
| `af_blood_tutorial` | `ARTEFACT` | 0 | `artefact_mincer` | off | `artefact_myasorubka1` | 3 | (0.0005, 0.0390, -0.0001) | .01 |
| `mp_af_electra_flash` | `SCRPTART` | 0 | `artefact_electra` | 0.6, 0.7, 0.9, 2.5 | `artefact_electra3` | 3 | (0, 0.1019, 0) | (.1, not read) |

Sections without lights carry no `trail_light_*` key (D1). The 69 records of 21 sections (D2) have no parent: every
artefact of `all.spawn` lies in the world when it is created.

## 2. The artefact's states

An artefact object is in one of these states (the states S16 and other specifications refer to):

- **lying**: online and without a parent. Its idle effect plays (section 4) and, when `lights_enabled` is on, its
  light is on (section 5).
- **carried**: online with a parent (in an inventory: belt, backpack, or in hands). In a single-player game it has
  no idle effect and its light is off (sections 4.6, 5.2).
- **activating**: from the start of an activation until the artefact's destruction (section 7). It begins from the
  carried state; its light is off (A15); the phases of section 7.2 run.
- **absent**: not online (never created, switched offline, or destroyed). Nothing of it exists.

Changes: created → lying (every shipped record, section 3.1); lying → carried when taken (4.6); carried → lying when
released by its parent (4.7); carried → activating (7.1); any → absent on destruction (3.2).

## 3. Coming online and going

### 3.1 Creation

When the artefact object is created from its record (it comes online), in this order (`bin_artefact.py` A4):

1. the general creation steps of a physical item: among them the object transform *X* from the record
   (level-objects.md 3.4) and a computation of the model's bone matrices (believed: the rest pose, every bone's *T*
   its bind matrix *B*, level-objects.md 4.8; no motion plays yet);
2. if the effect name is not empty: the **effect start** of section 4.2;
3. a light record is created (always, also when `lights_enabled` is off) and its shadow setting set on;
4. **lights on** (section 5.2);
5. if the model is animated (visual type 3), it plays the motion `idle` as a looped cycle (how a cycle advances is not
   specified here; skeletal-motions.md section 7);
6. the fast flag of section 6.2 is cleared.

The object is then processed every frame (`bin_engine.py` E6) until the physics simulation makes it sleep (section
6.1).

### 3.2 Destruction

When the artefact object is destroyed (switched offline, the level ends, or after an activation): every effect of
its effect list is destroyed (section 4.6), **lights off** (section 5.2), the light record is released, and an
activation in progress (with its own light) is released (A8).

## 4. The idle effect

### 4.1 The effect list and its point

The artefact holds its effects in an **effect list**: a list of points of its model, each with the effect objects
started at it. The list starts with exactly one point: bone 0 of the model with offset (0, 0, 0) (`bin_player.py` P1).
A model user-data section `particle_bones` would replace it, but only two other kinds of objects read that section;
the artefact never does, and no artefact model has one (P2; D5). So an artefact's effects are always at bone 0.

The point's position is **q** = **o** · *T*₀ · *X* with **o** = (0, 0, 0): row 3 of bone 0's current model-space
matrix *T*₀, placed by *X* (P3). For the 7 models of visual type 10 *T*₀ is the bind matrix (they have no motions);
for the 19 of type 3, every frame of their one motion `idle` gives bone 0 the same position (D5), the one of section
1.3. The effect start at creation (3.1) comes before `idle` plays, so it uses the rest pose (believed): the same
position for every model but `artefact_rusty_hairs`, whose bone 0 is at (0, 0.2300, 0) at rest and (0, 0.2369, 0)
under `idle` (D5). Whether the physics simulation changes a bone's current matrix is **unestablished** (believed not; the column
of section 1.3 assumes it does not).

### 4.2 Effect start

An effect start with a name, a direction **d**, a life and an auto-remove switch does, for every point of the list
(P3):

1. the **start matrix** *S* is built from **d**: rows 3 and the fourth column of the identity; row 2 = **d** /
   |**d**|; if |*d*ₓ| < |*d*ᵧ| (as for every start of this specification), row 1 = (0, *d*_z, −*d*ᵧ) / |(*d*ᵧ, *d*_z)|
   (of the normalised **d**); row 0 = row 1 × row 2 (P3);
2. the effect object of that name at this point is found, or, if there is none, made from the name (zone-idle-
   effects.md 4.5, with the same creation settings: no automatic removal) (P3);
3. its **life** is set: −1 (none) unless auto-remove is on, else the given life in ms;
4. three **angles** are taken from *S* and kept with the effect object (section 4.3);
5. its instance is placed (P2 5.1 / P6 3) in world mode with velocity (0, 0, 0) by *S* with row 3 replaced by the
   point's position **q**; then the effect object's visibility sphere is updated (P8 5.4);
6. if its instance is not playing, the effect object is played (zone-idle-effects.md 4.5: play and an immediate 33 ms
   update).

The idle effect is started with the section's `particles` name, **d** = (0, 1, 0), life −1 and auto-remove off, at
creation (3.1) and on release (4.7) (A4, A7). So *S* has rows (1, 0, 0), (0, 0, −1), (0, 1, 0) (`model.py` M1): the
effect's own z axis points up and its own y axis points along world −z, whatever the artefact's orientation.

### 4.3 The kept angles and the update rotation

From *S* the angles (*h*, *p*, *b*) are kept: with *w* = √(*S*[0][1]² + *S*[1][1]²): if *w* > 1.9073486 × 10⁻⁶ (`f32`
0x36000000) they follow the general case (not reached here); otherwise *h* = −atan2(−*S*[0][2], *S*[0][0]),
*p* = −atan2(−*S*[2][1], *w*), *b* = 0 (P3). For **d** = (0, 1, 0): *h* = 0, *p* = π/2 (`f32` 0x3FC90FDB), *b* = 0 (M1).

Later placements (section 4.4) use the **update rotation** *U* built from the kept angles in the form of
level-objects.md 3.4 with *x* = *p*, *y* = *h*, *z* = *b*, each sine and cosine taken of the `f32` angle (P4). For the
idle effect *U* has rows (1, 0, 0), (0, *c*, −1), (0, 1, *c*) with *c* = cos(*p*) = −4.371139 × 10⁻⁸ (`f32`
0xB33BBD2E) (M1), so the first placement (by *S*) and the later ones (by *U*) differ only by *c*.

Every `source` of the 14 effects that the six artefact groups' members name has action flag bit 0x2 and parent motion
0 (D4): their position and velocity regions are placed by the full rotation (P2 5.2), so a source region's local
point (0.1, 0.2, 0.3) lies at **q** + (0.1, 0.3, −0.2) (M1), and the placement velocity has no effect.

### 4.4 Placement at every list update

The effect list is updated by the artefact's client update (section 6.1) and after every destruction by name (4.6).
An update does nothing unless a start or an earlier update found something playing; otherwise, for every point and
every effect object at it (P4, P7):

1. the instance is placed in world mode by *U* with row 3 = the point's current position **q** (section 4.1, with
   the current *X*), with the list's **stored velocity** (section 6.3: the parent's velocity when carried, else
   (0, 0, 0)); then the visibility sphere is updated (P8 5.4);
2. if its life is not −1: when the frame's elapsed milliseconds are less than the life, the life decreases by them;
   otherwise the instance is stopped deferred (P2 7 / P6 3; E7) and the life becomes −1;
3. if the instance is playing (E7), the list stays active; otherwise the effect object is destroyed (section 4.6)
   and removed from the list.

The idle effect is a group without a time limit (D4), so it stays playing (P6 5.6) and is never removed by step 3;
it moves with the artefact. Live particles are not moved by a placement (P2 5.1).

### 4.5 The effect object's own updates

Between placements the effect object updates its instance by itself: at its own scheduled updates and when drawn,
with the clock's elapsed milliseconds (zone-idle-effects.md 4.5; its schedule is 30 … 525 ms by the camera distance
of its sphere's centre). This continues while the artefact object is not processed (section 6.1).

### 4.6 Taken by a parent; destruction of effects

When the artefact is taken by a parent (A6):

1. the general steps of an item being taken;
2. **lights off** (section 5.2);
3. in a single-player game (game type `single` = 1, A6), if the effect name is not empty: every effect object of that
   name in the list is **destroyed**, and the list is updated (4.4) (P5).

In other game types the effect is not destroyed (it stays in the list and follows the artefact); they are outside
this specification.

**Destroying** an effect object (P5, P6; `bin_engine.py` E2, E3): it is marked dead and handed to the engine's list
of effects to delete; a dead effect object is no longer updated by its schedule; the engine's per-frame step deletes
the listed effect objects (taking them from the end of the list, stopping at one that reports itself locked, which an
effect object never does: its lock field is set to 0 at creation and play). Its particles disappear with the
deletion; drawing does not test the dead mark, so they are drawn in at most the frames before the per-frame step runs
(believed: none; the order of that step and drawing within a frame is **unestablished**). The instance is not stopped
first: no death children are made (P6 5.4).

### 4.7 Released by its parent

When the artefact leaves its parent (dropped, or its holder's inventory releases it otherwise) (A7):

1. the general steps of an item being released;
2. **lights on** (section 5.2);
3. if the effect name is not empty: the effect start of 4.2 with **d** = (0, 1, 0), life −1, auto-remove off. In a
   single-player game the old effect object was destroyed when taken, so a new one is made and played at the
   artefact's current position.

### 4.8 Sections without an effect

The 5 sections without `particles` (`af_dummy_spring`, `af_dummy_dummy`, `af_rusty_thorn`, `af_rusty_kristall`,
`af_rusty_sea-urchin`; D1) start no effect; their list stays empty.

## 5. The light

### 5.1 The record

Created at the artefact object's creation (3.1, A4), released at its destruction (3.2). Its shadow setting is on (no
effect in the first renderer, P16 2.5). Its type is never set, so it is a point light (P16 2.1); the artefact sets
nothing on it but the colour (as R, G, B), the range, the position and the active state (A14). It has no colour
animation and the artefact creates no glow (the light record is its only renderer request, A14).

### 5.2 Lights on, lights off

If `lights_enabled` is off, lights on and lights off do nothing: the record is never made active, so the artefact
never lights anything (A5). If it is on (A5; E5):

- **Lights on**: colour (R, G, B) = the three numbers of `trail_light_color` (`f32`; alpha 1); range = 
  `trail_light_range`; position = **p** (row 3 of *X*, not bone 0, no height); then active. The stored values change by
  P16 2.2; the first range set always differs from the default 8 by at least 0.8 and is taken (M4).
- **Lights off**: made inactive.

Lights on runs at creation (3.1) and on release (4.7); lights off when taken (4.6), at the start of an activation
(7.1) and at destruction (3.2). So the light is on exactly while the artefact is lying.

### 5.3 Following the artefact

At every workload (section 6.3), if `lights_enabled` is on and the record is active, its position is set to **p**
(A5). Colour and range never change after lights on.

## 6. Updates

### 6.1 The client update

While the artefact object is processed, its client update runs in each frame's pass over the game's objects (not
while the game is paused) (E6), in this order (A9):

1. if it has no parent and its physics shell is active: *X* is taken from the physics simulation;
2. the general object update;
3. the effect-list update of section 4.4 (with this *X*);
4. general item work (of no concern here);
5. the workload of section 6.3, if the fast flag (6.2) is set or an activation is in progress.

Every object is processed after its creation; the physics simulation turns processing off when the object comes to
rest and on again when it moves (E6; the rule is the physics simulation's and is **unestablished** here). While not
processed the effect stays where it was last placed (the artefact does not move then) and keeps animating (4.5).

### 6.2 The scheduled update and the fast flag

The artefact also has scheduled updates: interval minimum 20 ms, maximum 50 ms (A3); with the general rule of
level-placed-effects.md 5.2 the period is clamp(30 + trunc(495 · *σ*), 30, 525) ms, *σ* = 0.005 (`f32` 0x3BA3D70A) ·
|*E* − **p**| (A10, E4): 30, 54, 153, 277, 525, 525 ms at 0, 10, 50, 100, 200, 400 m (M3). Each scheduled update
(A10):

1. the general scheduled work;
2. the **fast flag** is set when the artefact has no parent and either |*E* − **c**| − *r* < 50 m, (**c**, *r*) the
   model's sphere (its visual header's bounding sphere, believed, ogf.md) with its centre placed by *X* (E4), or the
   device's frame number equals a frame number the artefact keeps (set to 0 at creation and, by a static search,
   never changed: believed to matter only in frame 0, A10); otherwise it is cleared;
3. if the fast flag is clear, the workload of 6.3 runs here.

So a lying artefact near the camera runs its workload in every processed frame, one farther away or carried at its
scheduled updates.

### 6.3 The workload

In this order (A11):

1. the effect list's stored velocity becomes the parent's velocity if the artefact has a parent, else (0, 0, 0)
   (used by the next list update, 4.4);
2. the light follows (5.3);
3. without an activation: the behaviour's own work (6.4); with one: the activation update (7.2).

### 6.4 The behaviours' own work

- **Plain** (`SCRPTART`): with a parent, *X* becomes the parent's transform (A13).
- **Jumping** (`ARTEFACT`): with a parent (and the artefact's physics shell not in charge), *X* becomes the parent's
  transform; otherwise, if `jump_height` ≠ 0: a ray is cast from **p** straight down for `jump_height` m; if it hits
  something, the physics shell gets an upward impulse of the shell's mass × the frame's elapsed seconds × a constant
  (A13). How the physics simulation moves the artefact is not specified here; the effect and the light follow the
  resulting *X* (4.4, 5.3).

## 7. Activation (shared with S16)

### 7.1 Start

An activation starts only when the artefact's holder receives the artefact's activation message; the artefact sends
it, and nothing else does, when the activate animation of its in-hand item state ends; that state is entered when
`wpn_fire` is pressed while the artefact is the item in its holder's hands and its section is a key of
`[artefact_spawn_zones]` (A15). 14 sections are keys there (D7). The player never holds an artefact in hands: an item
reaches the hands only from a slot (player-state.md 3.3), and no artefact section with records has a `slot` key (only
`mp_af_electra_flash`, slot 10, which has no record) (D6). So the player's artefacts never activate; whether any other
holder (a character) ever puts an artefact in its hands is **unestablished**.

At the start (A15): the artefact's light is turned off (lights off, 5.2), the phase becomes 1, the phase timer 0, the
object is processed (counted once more), the activation's own light record (created with the activation, shadow
setting on) is made active, and phase 1 starts.

### 7.2 Phases

The phases are read from the section that the artefact's `artefact_activation_seq` names (15 sections name one of
four: `af_activation_bold`, `af_activation_electra`, `af_activation_gravi`, `af_activation_mincer`; D7): phase 1
`starting`, 2 `flying`, 3 `idle_before_spawning`, 4 `spawning`; each value lists the time (s), a sound, the light's red,
green and blue, the light's range, an effect name and an animation name (A15).

- **Phase start**: the phase's sound (S16), the activation light's range and colour, the phase's effect (an effect
  start of 4.2 at the artefact's effect list, life = time × 1000 ms, auto-remove on; its direction is not examined),
  the phase's animation (A15).
  The activation light and effects are owned by the follow-up specification F1 of the scope.
- **Update** (at every workload while activating, 6.3): the timer grows by the frame's elapsed seconds; when it reaches
  the phase's time, the next phase starts with the timer at 0. The activating artefact has a parent, so its fast
  flag is clear (6.2) and the workload runs both at every client update (6.1, step 5) and at every scheduled update
  (believed: a carried artefact keeps its scheduled updates); each run adds the frame's elapsed seconds, so in real
  time a phase can end before its listed time, by an amount that depends on the frame and scheduled-update rates. At the start of phase 4, when the game runs its own
  server, the zone of the section's `[artefact_spawn_zones]` entry is created (F1). When phase 4 ends: the extra
  processing count is removed and the artefact asks the game's server, once, to destroy it (A15); it becomes absent
  when the server's destruction arrives (3.2).

Phase times (s) (D7; M5): `af_activation_gravi` 5, 2, 5, 1; `af_activation_bold` 5, 2, 2, 0.5;
`af_activation_mincer` 5, 2, 5, 0; `af_activation_electra` 5, 2, 0, 2. A phase of time 0 ends at its first update.
Counted in accumulated timer seconds (ignoring the steps' rounding), the bold phases end at 5, 7, 9 and 9.5 s, the
electra phases at 5, 7, 7 and 9 s.

## 8. The first level at the start

- 6 artefact records (D3); online at the start only `esc_af_medusa` (`af_medusa`, `ARTEFACT`): `esc_af_blood` and
  `esc_af_cristall_flower` are offline by distance, the three `SCRPTART` records follow the day-and-night rule
  (level-objects.md 2.5) (D3).
- `esc_af_medusa` shows the group `anomaly2\artefact\artefact_gravi`; its light is off (`lights_enabled` false) (D3).
  Its *X* has row 3 (−244.8165, −13.1585, −33.6452); its effect point (bone 0 at rest of `artefact_blackdrip`, type 10)
  is (−244.8057, −13.1224, −33.6349) (M2). Its model sphere is centred at (0.0062, 0.1543, −0.0049), radius 0.23
  (D10), placed at (−244.7696, −13.0158, −33.6089); it is fast while the camera is within 50.23 m of that centre
  (M2).
- Four records of the level have lights (`esc_af_cristall_flower`, `esc_af_electra_flash`, `esc_af_electra_sparkler`,
  `esc_af_electra_sparkler_0000`); none is online at the start (M4). No artefact light is on at the start.

## 9. Verification facts

### 9.1 All levels

- 29 sections (`ARTEFACT` 25, `SCRPTART` 4); 69 records of 21 sections, none with a parent; per level: `l02_garbage`
  27, `l03u_agr_underground` 9, `l06_rostok` 7, `l01_escape` 6, `l10_radar` 5, `l11_pripyat` 5, `l07_military` 4,
  `l04u_labx18` 2, `l08_yantar` 2, `l03_agroprom` 1, `l04_darkvalley` 1 (D1, D2).
- 62 records name an effect, 30 have lights; 13 sections have lights (D1, D2).
- Six effect names, all groups of `particles.xr` (1,394,071 bytes, CRC-32 0x4E93C6F1), each with time limit 0:
  `artefact_gravi` (7 sections), `artefact_mincer` (5), `artefact_electra` (4), `artefact_ameba` (4), `artefact_zharka`
  (3), `artefact_puxx` (1); their 16 members have flags 0x4 (15) and 0x44 (1, in `artefact_electra`: death children
  `anomaly2\artefact\artefact_electra_idle_00`); they name 14 effects (D4, D9).
- 26 models: 19 of visual type 3 (each with its own motions, one of them `idle`), 7 of type 10; none has a
  `particle_bones` user-data section (D5).

### 9.2 Worked values

Sections 4.2, 4.3, 6.2 and 8 give them (M1 – M5).

## 10. Unestablished

- Whether the physics simulation changes the bones' current matrices of a lying artefact (4.1), and its rule for
  making an object sleep and wake (6.1).
- The order, within a frame, of the deletion of destroyed effect objects and drawing (4.6).
- Whether a character other than the player ever holds an artefact in hands and activates it (7.1).
- Whether a carried artefact keeps its scheduled updates (believed: yes), and so how much faster than listed the
  activation phases run in real time (7.2).
- For an artefact created with a parent (no shipped record; such artefacts are made during play), whether its creation
  starts the effect and the light before it is taken (believed: yes, and the taking ends them at once).
- The frame number the fast test compares with (believed: 0 for good, 6.2).
- How the physics simulation responds to the jumping impulse (6.4).

## 11. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`;
  binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`, `bin\xrRender_R1.dll`.
- Data examined: every record of `spawns\all.spawn` (decoded per sav.md) and the levels of `game.graph`; the artefact
  and activation sections of `config\misc\artefacts.ltx` through `system.ltx` with inheritance; the groups and effects
  of `particles.xr` that the sections name; the 26 artefact models (header, bones, motions, user data); the shipped
  scripts.
- Analysis: static analysis of the shipped binaries (Ghidra 12.2, headless decompilation, instruction listings and
  data reads): the artefact object's loading, creation, taking, release, destruction, client and scheduled updates,
  light handling, per-behaviour work and activation; the game's object effect list (its points, start, placement,
  destruction); the effect object's creation, play and destruction and the engine's deletion of destroyed effects;
  the general object schedule scale, centre and radius; the renderer's light colour setting; the class registration
  and the game-type names. Parsing of the data with throwaway tools written from the approved specifications and this
  text (`model.py` implements sections 4.2, 4.3 and 6.2). No execution or debugging of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/artefact-idle-effects/` reproduce every fact.

## Gaps found in review

- The start-matrix rule for a direction with |*d*ₓ| ≥ |*d*ᵧ| (4.2) and the general case of the kept angles (4.3) are
  not given; an activation phase effect (7.2, direction not examined) may need them.
- How the frame's elapsed milliseconds used by the life countdown (4.4 step 2) are obtained (clock, rounding) is not
  stated; it matters only for effects with a life, i.e. the activation phase effects.
