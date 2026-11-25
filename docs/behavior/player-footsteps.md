# Footsteps and landings of the player

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of S11 of
`docs/behavior/level-sound-scope.md`: when the player's footsteps sound and which sound plays. It covers
which leg cycle the player's model plays in each movement state (including the landing cycle), the step
table that places steps within a leg cycle, the timing of the steps, the choice of the step sound from the
material pair of the player and the ground, and how the sound is played. Any collision sound of the
player's body, including the impact of a landing, is S13's. What could not be established is marked
**unestablished**; what is inferred rather than shown by the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/behavior/player-cycle-lengths.md` (S10): the player's model and its motion file, finding a cycle
  by name (section 2), a cycle's length *L*, speed *s*, duration *D* = f32(*L* / *s*) and its flag
  "stops at the end" (sections 3, 4).
- `docs/formats/material-pairs.md` (S9): the pair of two materials (section 5), its **step** list and the
  sounds made from it when the library loads (sections 2 – 4; effects category).
- `docs/behavior/sound-positional.md` (S2): playing a sound independently (section 3.2), the play volume
  *v* (section 3.3), the effects category volume (section 5.4).
- `docs/formats/sound-ogg-params.md` (S1): sound files, their distances and base volume, the reports to
  the AI.
- `docs/behavior/player-movement.md`: the movement state and its flags (section 3), the run rule (4.1),
  crouch and low crouch (4.2), sprint (4.3), jump (4.4), falling and landing (6.7), body turning and
  climbing (section 10), the frame times (3.2).
- `docs/formats/level-cform.md`: the player's own position (its feet, section 5.1), the **ground
  material** (section 4.4), the airborne state (section 6.4), the materials and their indices
  (section 3).
- `docs/formats/ltx-config.md`: configuration files and value lists.

Conventions: f32(*x*) is *x* rounded to single precision; trunc(*x*) is *x* rounded toward zero to an
integer. Times are whole milliseconds (ms) of the step clock (section 3.1) unless stated; additions and
comparisons of clock values are unsigned 32-bit. "The state" is the movement state of
`player-movement.md` section 3 after that frame's movement update.

**Checks.** Every claim carries a tag. The checks are scripts in the dirty workspace,
`checks/player-footsteps/`: `extract.py` extracts the data into `x/`, `export.py` regenerates the
binary evidence in `ev/` (headless Ghidra exports of the shipped program files); `run_all.py` runs
`check_data.py` (tags D1 – D9: the game data), `check_bin.py` (B1 – B20: the program files) and
`model.py` (M1 – M6: an implementation of sections 3 and 4 written from this text).

## 1. Configuration

- The player's configuration section `[actor]` of `config\creatures\actor.ltx` has
  `material = creatures\actor`, `step_params = stalker_step_manager` and `LegsCount = 2` [D1].
  `material` names the player's **own material**: the material of that name in `gamemtl.xr`
  (`creatures\actor`: id 90, index 53) [D1, B13]. A missing `material` line is a fatal error
  (assertion failure) [B13].
- `LegsCount` is the number of **legs** *n*, read as an unsigned 8-bit integer [B1]. Room is kept for at
  most 4 legs; a larger value is not supported [B1].
- `step_params` names the **step table**, a section that must exist for the player to have footsteps at
  all (without it, no step ever sounds) [B1]. Its lines are read in order; each line's key is a cycle
  name and its value a list (commas, items trimmed): item 0 is the **pass count** *C* (an integer; 0 is
  a fatal error, assertion failure), and for each leg *k* = 0 … *n* − 1, item 2*k* + 1 is the leg's
  **step time** *t*ₖ and item 2*k* + 2 its **step volume** *p*ₖ (decimal numbers) [B1]. The key is
  looked up as a cycle name of the player's model (S10 section 2); a line whose name is no cycle is
  skipped; a line whose cycle already has an entry replaces it [B1]. The comment of the shipped
  section says the legs are in the order left, right [D2] (believed: it only names the two steps; nothing
  else depends on the order).

Shipped table (section `[stalker_step_manager]` of `config\creatures\m_stalker.ltx`): 38 lines with 37
distinct names (`norm_escape_00` twice, with the same values), all of them cycles of the player's
model; every line has 5 items and *C* = 1 [D2]. The lines of the cycles the player plays (section 2;
the others belong to other characters) [D2, D3]:

| Cycles | *C* | *t*₀, *p*₀ | *t*₁, *p*₁ |
|---|---|---|---|
| `norm_walk_fwd_0`, `norm_walk_back_0`, `norm_walk_ls_0`, `norm_walk_rs_0` | 1 | 0.2, 0.2 | 0.65, 0.2 |
| `norm_run_fwd_0`, `norm_run_back_0`, `norm_run_ls_0`, `norm_run_rs_0` | 1 | 0.2, 1 | 0.65, 1 |
| `cr_walk_fwd_0`, `cr_walk_back_0`, `cr_walk_ls_0`, `cr_walk_rs_0`, `cr_run_fwd_0`, `cr_run_back_0`, `cr_run_ls_0`, `cr_run_rs_0` | 1 | 0.2, 0.1 | 0.65, 0.1 |
| `norm_escape_00` | 1 | 0.2, 1.3 | 0.65, 1.3 |
| `norm_jump_end` | 1 | 0.2, 1.3 | 0.21, 1.3 |

## 2. The leg cycle

### 2.1 Choice by movement state

The player's model plays one **leg cycle** at a time. Its name is chosen from the state at every
movement update of the player (each frame, `player-movement.md` section 3), and also when the player's
model changes [B2, B3]. While the player's health is 0 or less no leg cycle is chosen (health is not
covered) [B2].

The name is *P*`_`*X*, with the **prefix** *P* [B2, B4]:

- while crouched (either height): `cr`;
- else while climbing: the climbing set (below);
- else `norm`.

and *X* the first that applies of [B2, B4]:

| State | *X* |
|---|---|
| landing (the landing flag of `player-movement.md` section 6.7) | `jump_end` |
| hard landing (the 0.3 s landing flag of section 6.7) | `jump_end_1` |
| turning the body while standing (section 10), not climbing | `turn` |
| falling (section 6.7) | `jump_idle` |
| jumping (section 4.4) | `jump_begin` |
| forward | *M*`_fwd_0` |
| back | *M*`_back_0` |
| left | *M*`_ls_0` |
| right | *M*`_rs_0` |
| none of these | idle (below) |

*M* is `run` when the state **runs** by the rule of `player-movement.md` section 4.1 (`g_always_run`
inverted by accel, and walking forced by leaning or aiming unless crouched, jumping, landing or climbing), else `walk` [B2, B5]. So with the defaults a crouch with Shift held (the low crouch) plays
`cr_walk_*` and a crouch without it `cr_run_*`.

- **Idle:** `norm_idle_0`; crouched: `cr_idle_0` when the state runs, else `cr_idle_1` [B2, B4].
- **Sprint** replaces the choice while the state sprints (section 4.3): forward gives `norm_escape_00`,
  else left `norm_escape_ls_00`, else right `norm_escape_rs_00`, whatever the prefix; with only back the
  choice above stands [B2, B4].
- **Climbing set:** idle `cl_idle_1`; every direction, walking or running, `cl_run_fwd_0`,
  `cl_run_back_0`, `cl_run_ls_0`, `cl_run_rs_0`; the jump, landing and turn names are those of `norm`
  [B4].

All these names are cycles of the player's model [D4]. Of them, only the walk, run, crouched walk and
run, `norm_escape_00` and `norm_jump_end` lines are in the step table (section 1); the idle, jump,
falling, hard-landing, turn, sideways-sprint and climbing cycles and every `cr_jump_*` cycle are not
[D4]. None of the player's leg cycles in the step table stops at the end (S10 section 4: flags bit 1
clear in all of them) [D3], so their passes always restart (section 3.3, item 4), and what happens when a
cycle stops at the end (its fading out and the notice to the code that started it) never concerns the
footsteps.

### 2.2 Starting a leg cycle

A leg cycle is started only when the chosen cycle differs from the one playing [B3]. It is started in
the form that takes the cycle's own parameters (S10 section 3), and then given a time [B3]:

- if both the state at the start of the frame's movement update and the state after it have a direction
  (forward, back, left or right) and a leg cycle was playing: the fraction of a pass that the previous
  cycle had reached (its time modulo its length, divided by its length; believed: the remainder is the
  ordinary floating-point remainder) times the new cycle's length *L*;
- else if only the state after the update has a direction: 0 or *L* / 2, chosen by one draw *r* of the
  generator of section 4.2 (*L* / 2 when *r* is odd);
- else 0.

The steps do not depend on this time (section 3) [B6].

## 3. Step timing

### 3.1 The step clock

The **step clock** is a millisecond clock of the engine, read once per frame [B7]: the elapsed time of a
timer that is stopped while the game is paused (it is one of the timers the pause stops) and whose rate
factor is 1 and never changed (nothing in the shipped program files sets it) [B7, B8]. With the device
setting of a fixed frame time it advances by 20 ms per frame instead [B7]. It is not the frame clock that
advances the animation (S10 section 4), which keeps running during a pause [B7].

### 3.2 Starting

When a leg cycle starts (section 2.2), the steps start with it: the **start time** *T* is the step clock
now; if the cycle has a line in the step table, its *C*, *t*ₖ and *p*ₖ are taken, the **pass** *c* = 1,
and every leg's step is pending; otherwise there are no steps until the next leg cycle starts [B6].

### 3.3 Each frame

Once per frame (believed: in the player's frame update, which runs every frame), while the player's
health is above 0, the steps of the current leg cycle are updated, with
*now* the step clock and *D* the cycle's duration (S10 section 3) [B2, B9]:

1. If the player's own material and the ground material have no pair (S9 section 5), nothing happens in
   this frame [B9, B10].
2. With the pass duration *d* = f32(*D* / *C*), leg *k*'s step of pass *c* is **due** at
   *T* + trunc(((*c* − 1) + *t*ₖ) · *d* · 1000) (computed in at least double precision, then truncated)
   [B9]. Each leg whose step of the current pass is pending and due (*now* ≥ due time) **steps**: the step
   sound of section 4 plays, subject to its conditions, and that leg's step of pass *c* is no longer
   pending [B9]. A step whose sound is suppressed (section 4) is consumed all the same [B9].
3. If *c* < *C*: *c* becomes trunc((*now* − *T*) / (*d* · 1000)) + 1 [B9].
4. If the cycle does not stop at the end and *now* > *T* + trunc(*L* / *s* · 1000) (the cycle's duration in
   at least double precision): *T* increases by trunc(*L* / *s* · 1000), *c* = 1, and every leg's step is
   pending again [B9].

So with the shipped table (*C* = 1) each leg steps once per pass of the cycle, at *t*ₖ · *D* after the
pass began, the passes following each other every trunc(*D* · 1000) ms from the cycle's start; a step is
heard at the first frame at or after its due time; a cycle change restarts the timing from the moment of
the change. Due times (ms after the start of a pass) and pass lengths of the table's cycles [D3]:

| Cycles | *D* (s) | Leg 0 | Leg 1 | Pass |
|---|---|---|---|---|
| `norm_walk_*`, `norm_run_*`, `cr_run_*` | 0.8671298 | 173 | 563 | 867 |
| `cr_walk_fwd_0`, `cr_walk_back_0` | 1.834313 | 366 | 1,192 | 1,834 |
| `cr_walk_ls_0`, `cr_walk_rs_0` | 1.3006947 | 260 | 845 | 1,300 |
| `norm_escape_00` | 0.6670229 | 133 | 433 | 667 |
| `norm_jump_end` | 0.3335115 | 66 | 70 | 333 |

## 4. The step sound

### 4.1 Conditions and choice

- **Pair.** The sound comes from the step list of the pair of the player's own material and the
  **ground material** (`level-cform.md` section 4.4: the material most recently touched by the player's
  contacts, a pass-through material taking precedence), as indices (S9 section 5) [B10, B11]. When the
  player's material record is set up, the ground material is `default` (index 0), until the body first
  touches something [B12] (believed: the record is set up when the player is spawned; its caller was not
  traced). The pair is looked
  up anew at each frame (section 3.3) [B9].
- **On the ground.** The sound plays only if the player is not airborne (`level-cform.md` section 6.4,
  `player-movement.md` section 6.5) by the state the movement physics recorded at its last update
  [B14, B15]. Otherwise the step passes silently.
- **A sound.** If the pair's step list is empty, nothing plays [B9]. Otherwise sound *r* mod *m* of the
  list plays, *m* its number of sounds and *r* one draw of the generator of section 4.2 [B9].

### 4.2 The generator

The draws come from the game's global pseudo-random generator, which other parts of the game draw from
too: a 32-bit state *x*,
each draw *x* ← (*x* · 214,013 + 2,531,011) mod 2³², result (*x* div 65,536) mod 32,768 (0 … 32,767)
[B16]. Its initial state and its other users are **unestablished**, so the sequence in a given play is not
reproducible; a uniform choice among the list's sounds is a faithful substitute.

### 4.3 Playing

The sound is played independently (S2 section 3.2): 3D, not looped, no delay, at the player's position
(its feet) raised by 0.5 m, (*x*, *y* + 0.5, *z*), taken at the moment of the step, with play volume
*v* = *p*ₖ of the stepping leg, the file's frequency and range, and the player as the sound's owner (for
the reports to the AI, S1) [B9, B17, B18]. The sound was created with the effects category when the
material library loaded (S9 section 4), so its volume is *b* · *p*ₖ · `snd_volume_eff` · *F* at the
listener before attenuation (S2 sections 5.3, 5.4); the gain sent to the device is limited to 1
(S2 section 7.1). The emitter does not follow the player afterwards (an independent play cannot be
addressed) [B17].

Each step also starts a particle effect chosen from the pair's collide particle list at the stepping
leg's foot bone, when that list is not empty [B19]; it makes no sound and is not covered here.

## 5. Landing

Landing plays no step of its own: it gives the leg cycle `norm_jump_end` (standing) while the landing flag
lasts (section 2.1), and that cycle's two steps (volume 1.3 each) are due 66 and 70 ms after it starts
[D3]. The landing flag lasts 0.1 s (`player-movement.md` section 6.7), so both steps are due before the
cycle is replaced (believed: both play at ordinary frame rates; at frame intervals above about 30 ms the
landing cycle may be replaced first). They sound only if the player is no longer airborne (section 4.1).
A hard landing (`norm_jump_end_1`) and a crouched landing (`cr_jump_end`, `cr_jump_end_1`) have no step
table line and so no step sound [D4]. Take-off, jumping and falling have none either [D4].

## 6. Not covered

- Collision sounds of the player's body, including the landing impact (S13); climbing sounds (S12): the
  climbing cycles have no step table line, so no footsteps sound while climbing (section 2.1) [D4]. **Disputed in review:** "no footsteps
  while climbing" holds only while neither landing flag is set: by section 2.1 a climbing state with a
  landing flag chooses `norm_jump_end`, whose step table line gives two steps; whether a landing flag can be
  set while climbing is not established.
- The particle effects of steps (section 4.3), the torso and head cycles, and the playback weights of the
  leg cycles.
- Footsteps of other characters (other lines of the step table).
- The game also has a timed way of playing a material pair's step list for a moving object, kept with each
  living object's material record; no use of it was found for the player (believed: it does not play for
  the player) [B20].

## 7. Verification facts

### 7.1 Configuration and table

- `[actor]`: `material = creatures\actor`, `step_params = stalker_step_manager`, `LegsCount = 2` [D1].
- Step table: 38 lines, 37 names, 5 items each, *C* = 1, values as in section 1 [D2]; durations and due
  times as in section 3.3 [D3].
- `creatures\actor` is material id 90 at index 53; `default` is id 0 at index 0 [D1].

### 7.2 Pairs

- The 57 pairs with `creatures\actor` name 40 distinct step sounds, all existing as
  `sounds\<name>.ogg`; 49 step lists have 4 sounds, 4 have 2, and 4 are empty: `objects\bullet`, `materials\fake_ladders`, `creatures\phantom`, `materials\earth_death`
  [D5, D6].
- The pair of `creatures\actor` and `default` (the ground material before any contact) is pair 1204,
  with the step list `material\actor\step\default1` … `default4` [D7].
- Step lists for the 22 materials of `levels\l01_escape\level.cform` (pair id; sounds, all under
  `material\`) [D8]:

| Material (id) | Pair | Step sounds |
|---|---|---|
| `default` (0), `materials\concrete` (3), `materials\asphalt` (4), `materials\metal` (5), `materials\glass` (6), `materials\flooring_tile` (12), `materials\bricks` (18), `materials\shifer` (49), `materials\stucco` (53), `materials\fake` (80), `materials\cloth` (81) | 1204, 1207, 1208, 1209, 1210, 1215, 1219, 1228, 1229, 1249, 1250 | `actor\step\default1` … `default4` |
| `materials\wood` (2), `materials\wooden_board` (19), `materials\tree_trunk` (39) | 1206, 1220, 1226 | `actor\step\new_wood1` … `new_wood4` |
| `materials\earth` (7), `materials\dirt` (10) | 1211, 1213 | `actor\step\earth1` … `earth4` |
| `materials\gravel` (8) | 1212 | `actor\step\gravel1`, `gravel2` |
| `materials\metal_plate` (13), `materials\metal_pipe` (34) | 1216, 1223 | `actor\step\metal_plate1` … `metal_plate4` |
| `materials\bush` (37) | 1224 | `actor\step\grass1` … `grass4` |
| `materials\setka_rabica` (48) | 1227 | `human\step\metall01gr` … `metall04gr` |
| `materials\tin` (60) | 1231 | `actor\step\tin1` … `tin4` |

- Of the 22 pairs only `materials\bush`'s has a collide particle list (1 item) [D9].

### 7.3 Worked examples

Computed by `model.py` from sections 3 and 4 (on the ground, a pair with a 4-sound list, the cycle started
at step clock 10,000) [M1 – M6]:

| Case | Result |
|---|---|
| `norm_walk_fwd_0`, frames every 16 ms from 10,000 | steps at 10,176 (leg 0), 10,576 (leg 1), 11,040, 11,440, 11,920, 12,304 ms; volume 0.2 [M1] |
| the same, frames every 1 ms | 10,173, 10,563, 11,040, 11,430, 11,907, 12,297 ms (passes restart at 10,867 and 11,734) [M2] |
| `norm_jump_end`, 16 ms frames | both steps (due +66, +70) in the frame at +80 ms, volume 1.3; none while airborne [M3] |
| `cr_walk_fwd_0`, 1 ms frames | +366, +1,192 ms [M4] |
| `norm_escape_00`, 1 ms frames | +133, +433, +800, +1,100 ms [M4] |
| `norm_idle_0` (no table line); a walk with no pair | no steps; without a pair not even the pass restarts [M5] |
| generator from state 0, 4-sound list | sounds 2, 3, 2, 1 [M6] |

## 8. Unestablished and believed

- The generator's initial state and its other users (section 4.2).
- The order, within a frame, of the leg-cycle choice and the step update (a one-frame difference at most).
- Believed: the step volumes' legs are left and right (section 1); the remainder used for the phase
  (section 2.2); both landing steps play at ordinary frame rates (section 5); the timed per-object way of
  section 6 does not play for the player.

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives
  `gamedata.db0` … `gamedata.dbd` (later archives win); binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`,
  `bin\xrCore.dll`, `bin\xrSound.dll`.
- Data examined: `config\creatures\actor.ltx`, `config\creatures\m_stalker.ltx`, `gamemtl.xr`,
  `meshes\actors\hero\stalker_novice.ogf`, `meshes\actors\stalker_animation.omf`,
  `levels\l01_escape\level.cform`, the archive listing (existence of sound files).
- Analysis: static analysis (headless decompilation and disassembly with Ghidra 12.2) of the game
  library's step table loading, step timing and step sound, the player's leg-cycle choice and the names
  it builds, the material record of living objects and the ground material, the airborne state seen by
  the steps, the engine's clocks and pause, the core library's timers and the sound library's independent
  play; constants read from the binaries. No execution or debugging. A model written from sections 3
  and 4 reproduces section 7.3.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- When the state's flag for turning the body while standing is set and cleared is not specified here, and
  `player-movement.md` section 10 leaves it out of scope; it decides between the `turn` cycle (no steps) and
  the movement cycles.
- What happens when `[actor]` has no `step_params` or no `LegsCount` line is not stated.
- What a step table line with fewer than 2*n* + 1 items gives for the missing step times and volumes is not
  stated.
- The ground material when the player stands on something other than the level geometry (a physical object)
  is not stated; `level-cform.md` section 4.4 covers only contacts with the level geometry.
