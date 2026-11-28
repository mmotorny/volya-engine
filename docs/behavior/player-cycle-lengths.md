# Animation cycle lengths of the player's model

Format-and-behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of S10 of
`docs/behavior/level-sound-scope.md`: which motion file the player's model uses, how a cycle of it is
found by name, and how long a cycle lasts: its length, its speed, its duration in seconds, and how its
time runs while it plays. No bone data is needed. Which cycle the player plays when, and how footsteps
use the duration, are not part of it (section 7). What could not be established is marked
**unestablished**.

Builds on (not repeated here):

- `docs/formats/skeletal-motions.md`: the motion-file list (chunk 19, section 1.2), motion files (1.3),
  the motion definitions of chunk 15 with their flags (bit 0: fx, otherwise cycle; bit 1) and the 16-bit
  value *q* of the stored speed (section 2), the motions of chunk 14 with their key count *N*
  (section 3), motion names (4.2) and partitions (4.3), and the evaluation at a time *t*, which repeats
  beyond the motion's length (5.1).
- `docs/behavior/player-state.md` section 2: the player's model `actors\hero\stalker_novice`.
- `docs/behavior/object-visual-and-transform.md` 1: model-file lookup. `docs/formats/chunks.md`: chunks.

Conventions as in skeletal-motions: little-endian; f32(*x*) is *x* rounded to single precision;
floor(*x*); min(*a*, *b*). Times in seconds unless given in milliseconds (ms).

Bracketed ids ([P1] …) name the check that backs the statement next to them: the functions of
`checks.py` in the dirty workspace's `checks/player-cycle-lengths/` (data extracted by `extract.py`,
binary evidence exported by `export.py`, decoder `pcl.py` written from this text).

A reader needs only chunk 15 and, from chunk 14, sub-chunk 0 and the start of each sub-chunk *k* + 1
(name `stringZ`, then the `u32` key count *N*); the tracks after it can be skipped, since every motion is
its own sub-chunk found by id.

## 1. The player's model and its motion file

The player's model `actors\hero\stalker_novice` is named by the player's record of `spawns\all.spawn`
[P1a] (player-state section 2). Its file `meshes\actors\hero\stalker_novice.ogf` has type 3 and a chunk 19
whose bytes are exactly `actors\stalker_animation` followed by one zero byte: one item [P1b]. Its only
motion file (file 0) is therefore `meshes\actors\stalker_animation.omf`; no `.omf` file exists under
`levels\` [P1c].

The starting outfit `novice_outfit` (`config\misc\outfit.ltx`) has
`actor_visual = actors\hero\stalker_novice.ogf`, the same model [P2a]. For reference: `outfit.ltx` has
16 distinct `actor_visual` values; the models of 14 of them list only `actors\stalker_animation`;
`actors\hero\stalker_exoskeleton.ogf` lists `actors\stalker_animation`, `actors\exoskeleton_animation`,
`actors\critical_hit_grup_2`, and `actors\dolg\stalker_do_nauchniy.ogf` lists
`actors\stalker_animation`, `actors\critical_hit_grup_6` [P2b].

`meshes\actors\stalker_animation.omf` has 1,569 motion definitions: 1,545 cycles and 24 fx motions; all
names are distinct [P3]. The player's model thus has 1,545 cycle names [P3b].

## 2. Finding a cycle by name

- When motion files are decoded, the definitions with flags bit 0 clear (cycles) are entered into a
  table of cycle names, and the others (fx) into a separate table; a cycle name therefore never finds an
  fx motion [P4a].
- A cycle is looked up by name in the model's **last** motion file first, then in each earlier one; the
  first file that has a cycle of that name gives it [P4b, P4c]. This is the rule of skeletal-motions 4.2
  applied to cycles only.
- Names are compared byte for byte with the stored names, which are lower-cased when read; the name
  asked for is **not** lower-cased [P4a, P4b]. A name that matches no cycle finds nothing; what follows
  from that is up to the user of the name.

On the player's model every cycle name refers to a definition of `stalker_animation.omf`.

## 3. Length, speed and duration

For a cycle with key count *N* and stored speed *x* (chunk 15):

- **Length** *L* = f32(*N*′ · f32(1/30)), f32(1/30) = 0.033333335, where *N*′ is *N* reduced to its low
  24 bits (the program keeps the count in 24 bits) [P5a, P5b, P5c]. The largest *N* in all motion sets
  is 3,140, so *N*′ = *N* in the data [P5d]. This is the length of skeletal-motions 5.1.
- **Speed** *s* = f32(*q*(*x*) · f32(1/655.35)), f32(1/655.35) = 0.0015259022, with *q* as in
  skeletal-motions section 2 [P6a, P6b, P6c]. A stored speed of 1 gives *q* = 655 and
  *s* = 0.99946594 [P6d].
- **Duration** *D* = f32(*L* / *s*): the time one pass of the cycle takes while it runs (section 4). For
  a stored speed of 1, *D* = 1.000534 · *L* (to 7 digits) [P6d].

A cycle is started with these values: its time is set to 0 and its speed to *s* [P6e]. The game code
starts cycles only in the forms that take the definition's own parameters (a form taking an explicit
speed exists in the program but is not used by the game code) [P6f]; the player's animation starts its
cycles that way [P6g].

## 4. The time of a running cycle

A started cycle has a time *t* (0 at its start, section 3) and is **running** from its start. The pose
of the model is evaluated at *t* (skeletal-motions section 5).

**Advancing.** The program advances the cycles of a model as follows [P7a, P7b, P7c]:

1. Let *c* be the current value of the frame clock (below) in ms, and *c*₀ the value recorded for the
   model at its previous advance. If *c* = *c*₀, nothing happens.
2. Otherwise Δ = *c* − *c*₀ (unsigned 32-bit difference), *dt* = f32(min(Δ, 66) · f32(0.001)) seconds,
   and *c* is recorded for the model.
3. Every cycle of the model that has not yet been advanced in the current frame is advanced once: if it
   is running, *t* becomes f32(*t* + f32(*s* · *dt*)) (single-precision arithmetic).

So, while every frame takes at most 66 ms, a running cycle's *t* grows by *s* per second of the frame
clock, and, because the evaluation repeats beyond *L*, a looping cycle repeats every *D* seconds. When
more than 66 ms pass between advances, the cycle falls behind the clock.

**The frame clock** counts elapsed milliseconds of the program's main timer and is read once per frame,
when the frame number is also increased [P8a]. No part of the program changes the clock's rate (the only
means to do so is used nowhere) [P8b], so it runs at real time (believed; its behaviour while the game
is paused is not covered).

**When advances happen.** The program advances a model whenever it computes the model's bones and
whenever a cycle is started on it (before the new cycle is set up) [P10a]; game code may also request
it. Whether the player's model is advanced on every frame is **unestablished**.

**Changes of speed.** No change of the speed of the player's leg cycles after their start was found: a
search of the game code for stores into the player's current leg cycle finds only the one setting its
time when the player's animation starts it [P9]. Whether any code changes the speed of the player's
cycles is **unestablished** (believed not).

**Stopping at the end.** A cycle whose definition has flags bit 1 (value 2) set **stops at the end**
[P6a]. After each advance of such a cycle that is not fading out, if *t* > f32(*L* − f32(1/30)), *t* is
set to f32(*L* − f32(1/30)) and the cycle stops running, so *t* no longer changes [P11a]. A cycle with
bit 1 clear never stops by itself.

**Fading out.** Starting a new cycle on a partition with blending makes the cycles already on that
partition fade out; without blending they are removed [P11c]. The new cycle starts fading in and running
[P11d]. A cycle that stops at the end stops running as soon as it begins fading out; any other cycle
keeps running while it fades out [P11b]. How the weights change while fading is playback (section 7).

## 5. Verification facts

### 5.1 The animations of `[stalker_step_manager]`

The section `[stalker_step_manager]` of `config\creatures\m_stalker.ltx` (used by the player through
`step_params = stalker_step_manager` of `[actor]` in `config\creatures\actor.ltx` [P12c], S11) has 38 lines with 37 distinct names (`norm_escape_00` occurs twice); every one is a
cycle of the player's model [P12a]. Their values (*L* and *D* rounded to 6 decimals) [P12a]:

| Name | *N* | Flags | Stored speed | *L* | *D* |
|---|---|---|---|---|---|
| `dmg_norm_escape_0` | 20 | 0 | 1 | 0.666667 | 0.667023 |
| `dmg_norm_walk_fwd_1` | 40 | 0 | 1 | 1.333333 | 1.334046 |
| `dmg_norm_run_fwd_1` | 40 | 8 | 1 | 1.333333 | 1.334046 |
| `dmg_norm_walk_fwd_0` | 40 | 0 | 1.5 | 1.333333 | 0.888912 |
| `dmg_norm_walk_back_0` | 40 | 0 | 1 | 1.333333 | 1.334046 |
| `dmg_norm_walk_ls_0`, `dmg_norm_walk_rs_0` | 40 | 0 | 1.5 | 1.333333 | 0.888912 |
| `dmg_norm_run_fwd_0` | 20 | 8 | 1 | 0.666667 | 0.667023 |
| `dmg_norm_run_back_0` | 20 | 0 | 1 | 0.666667 | 0.667023 |
| `dmg_norm_run_ls_0`, `dmg_norm_run_rs_0` | 20 | 8 | 1 | 0.666667 | 0.667023 |
| `dmg_norm_turn_left_0`, `dmg_norm_turn_right_0` | 25 | 2 | 1 | 0.833333 | 0.833779 |
| `norm_escape_0` | 20 | 0 | 1 | 0.666667 | 0.667023 |
| `norm_escape_00` | 20 | 8 | 1 | 0.666667 | 0.667023 |
| `norm_walk_fwd_1` | 40 | 8 | 1 | 1.333333 | 1.334046 |
| `norm_jump_end` | 10 | 0 | 1 | 0.333333 | 0.333511 |
| `norm_walk_fwd_0`, `norm_walk_back_0`, `norm_walk_ls_0`, `norm_walk_rs_0` | 26 | 8 | 1 | 0.866667 | 0.867130 |
| `norm_run_fwd_0`, `norm_run_back_0`, `norm_run_ls_0`, `norm_run_rs_0` | 26 | 8 | 1 | 0.866667 | 0.867130 |
| `norm_turn_left_0`, `norm_turn_right_0` | 30 | 0 | 1 | 1.000000 | 1.000534 |
| `cr_walk_fwd_0`, `cr_walk_back_0` | 55 | 8 | 1 | 1.833333 | 1.834313 |
| `cr_walk_ls_0`, `cr_walk_rs_0` | 39 | 8 | 1 | 1.300000 | 1.300695 |
| `cr_run_fwd_0`, `cr_run_back_0`, `cr_run_ls_0`, `cr_run_rs_0` | 26 | 8 | 1 | 0.866667 | 0.867130 |
| `cr_turn_left_0`, `cr_turn_right_0` | 30 | 0 | 1 | 1.000000 | 1.000534 |

Example in full precision: `norm_walk_fwd_0`, *L* = 0.8666667342, *D* = 0.8671298 [P12b]. The five
names of the scoping specification (`norm_walk_fwd_0`, `norm_run_fwd_0`, `norm_escape_00`,
`cr_walk_fwd_0`, `norm_jump_end`) are among them [P13e].

### 5.2 All cycles of the player's model

Over the 1,545 cycles [P13a – P13d]: the sum of *N* is 125,350; the sum of *L* is 4178.3335 and of *D*
4139.9268 (sums in double precision of the single-precision values, within 1e−3). Flags: 0 in 270
cycles, 2 in 1,055, 6 in 1, 8 in 219 (so 1,056 stop at the end). *q*(speed) is 655 in 1,482 cycles,
1,310 in 26, 983 in 22, and at least 327 in all. *N* ranges over 1 … 3,140; the shortest *D* is that of
`$editor` (0.033351), the longest that of `doc_agr_` (104.7226).

### 5.3 Invariants over all motion sets

In all 156 motion sets (skeletal-motions 1.3), the 8,983 cycles all have *N* ≥ 1 and *q*(speed) ≥ 19,
so every cycle has a positive length and a finite duration [P14].

## 6. Summary for users of the durations

Given a cycle name of the player's model: look it up among the cycles of `stalker_animation.omf`
(section 2), read its *N*, flags and stored speed, and compute *L*, *s* and *D* (section 3). A looping
cycle that has run without interruption for a frame-clock time τ (frames at most 66 ms apart, every
frame advanced) has *t* = τ · *s* up to rounding, that is, τ / *D* passes [P7c].

## 7. Not covered

- Which cycle the player's model plays in which movement state, and the time the player's animation
  gives a newly started leg cycle (the only value it stores into one, section 4).
- How footstep timing uses *D* (S11).
- Playback weights: fading in and out, the power, accrue and falloff parameters, and blending several
  cycles; flags bits 2 and 3; the time of fx motions.
- The frame clock while the game is paused.

## 8. Unestablished

- Whether the player's model is advanced on every frame (section 4).
- Whether any code other than the player's animation changes the speed of the player's cycles
  (section 4).
- The value recorded for a model before its first advance, and so the first Δ after the model is
  created.

## 9. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (later archives win); binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, and the import lists
  of the other shipped libraries.
- Data examined: `spawns\all.spawn`, `config\misc\outfit.ltx`, `config\creatures\m_stalker.ltx`, every
  `.ogf` and `.omf` file of the archives (chunks 14, 15, 19), the archive listing.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation with
  Ghidra 12.2) for the cycle-name tables and their lookup, the length and speed given to a started
  cycle, the advancing of a model's cycles, the frame clock, stopping at the end and fading out, and the
  starting of the player's cycles; constants read from the binary. No execution or debugging. A
  throwaway decoder written from this text computes every value of section 5.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Section 2 leaves open what a lookup of a missing cycle name does: the program also has a lookup form that reports "! MODEL: can't find cycle: %s" for a name that matches no cycle; whether that stops the game, and which form the users of these names rely on, is not stated.
- Section 4 does not say that, when a cycle that stops at the end stops, it may also begin fading out and notify the code that started it, depending on how it was started; which of the player's cycles do so is not stated.
