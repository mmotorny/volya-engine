# Particle groups

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of a group of `particles.xr` over time: which members play, their start and stop times,
the group time limit (group chunk 5), the effects a member names in its other three slots and when they start, and
what every member flag value of the library does. It is specification P6 of
`docs/behavior/particles-and-lights-scope.md`. What a single effect does over time (its update in 33 ms steps, its
placement, play and stop, its particles' births and deaths) is P2's; what its actions do is P3's and P4's; drawing
is P7's. Who creates, places, plays and updates a group (a zone, an artefact, the weather, …) and how often is the
owners' specifications'. What could not be established is marked **unestablished**; what is inferred rather than read
from the program or the data is marked **believed**.

Builds on (not repeated): `docs/formats/particles-xr.md` (P1: the group record, section 6, with its member fields
and member flag word; "member", "effect name", "per-particle child name", "birth child name", "death child name",
"start time", "stop time" and "time limit" below are P1's), the particle lifecycle specification P2 (an effect
instance: placement by a matrix *M*, a velocity **w** and a mode, world or local, P2 5.1; play, deferred and immediate
stop, playing, length, P2 7; the update with *e* ms, P2 8; the particle list, its birth and death notices and the
particle's position **x** and previous position **x**ₚ, P2 3), `docs/behavior/object-visual-and-transform.md` (matrix
convention).

Conventions: matrices in the row-vector convention of object-visual-and-transform.md (a point **p** is placed as [**p** 1] · *M*;
rows 0 – 2 are the axes, row 3 the translation). Arithmetic is IEEE single precision (`f32`) unless stated. Times are
in seconds, *e* in whole milliseconds. "Member *i*" is the *i*-th member of the record (0-based, stored order). Every
fact carries the name of the check that reproduces it, a script in `checks/particle-groups/` of the dirty workspace;
`run_all.py` runs them all (`archive.py` extracts `particles.xr`, `bin_setup.py` prepares the static analysis the
`bin_*.py` checks read, `pg.py` decodes the library from P1's text, `sim.py` is a throwaway implementation of
sections 3 – 5 written from this text that `data.py` uses for section 9).

## 1. Overview

A group instance is made from one group record. For every member it holds one effect instance of the member's
effect (the **member effect**), a list of **per-particle children** and a list of **free children** (both effect
instances, section 6). It also holds a state (playing, stopping), a group time *t*, a position and a bounding box.
Its owner places it (section 3), plays and stops it (section 3) and updates it with the elapsed real time (section 5);
the group passes the same placement, stop and elapsed time on to its effects.

## 2. Making a group instance

### 2.1 The member effects

When an instance gets its record (`bin_create.py`):

- one member effect is made for **every** member, whatever its flag word (members without bit 0x4 also get one),
  by looking up the member's effect name among the library's effects (exact byte comparison; groups are not
  searched). A name that is not found, or an empty one, is an assertion failure with the text
  `Particle effect doesn't exist` (2.3);
- each member effect gets its record as an effect alone does (P2 8.1), except that its particle-birth and
  particle-death notices are the group's (section 6), told which member they belong to;
- a new instance is not playing and not stopping, and its position is (0, 0, 0) (`bin_create.py`).

### 2.2 Names given by owners

An owner that makes a particle object by a name gets an effect when the name is an effect of the library, otherwise
a group when it is a group; when it is neither, an assertion failure with the text
`Particle effect or group doesn't exist` (`bin_create.py`). No name is both (P1 1).

### 2.3 Assertion failures and fatal errors

An assertion failure and the fatal error of 6.4 both end in the program's error reporting, which hands the text to
the crash reporter and then raises a debug-break exception (`bin_create.py`). Believed: without a debugger the game
ends there with its error report; this was not observed by running the game.

## 3. Placement, play and stop

(`bin_state.py`.)

- **Placement** with *M*, **w** and a mode: the group's position becomes *M*'s row 3, and every member effect is
  placed with the same *M*, **w** and mode (P2 5.1). The children are not placed by this request (they are placed
  from particles, section 6).
- **Play**: *t* = 0, stopping cleared, playing set. Nothing else: the member effects are started by the update
  (5.2), and member effects still playing from before keep playing.
- **Stop, deferred**: stopping set; then every member is stopped deferred (5.4).
- **Stop, immediate**: playing cleared; then every member is stopped immediately (5.4). Stopping is left as it was.
- **Playing** is the playing state.

## 4. Length and looping

A group's **length** is its time limit (chunk 5; 0 for none). An effect or group is **looped** when its length is
below 0 (`bin_state.py`); an effect's length is −1 without a time limit (P2 7), so an effect without a time limit is
looped and one with a time limit is not, and a group is never looped.

## 5. Update

The owner updates the instance with *e* ms (`bin_update.py`).

### 5.1 Not playing

The bounding box becomes the position ± 0.001 on each axis, the sphere the box's centre and half its diagonal.
Nothing else happens: *t* does not advance and no member effect or child is updated.

### 5.2 Playing

In this order:

1. **Δ** = *e* · 0.001 (the `f32` constant 0.0010000000474974513), rounded to `f32`; *e* is taken as unsigned
   (*e* + 2³² when the 32-bit value is negative). E.g. *e* = 33 gives Δ = 0.032999999821186066, *e* = 16 gives
   0.01600000075995922 (`data.py`).
2. **Start and stop tests**: for each member in member order that has flag bit 0x4 (members without it are skipped),
   with *t* the group time before this update and *t* + Δ summed in `f32`:
   - if its member effect is playing: when *t* ≤ stop time ≤ *t* + Δ, the member is stopped (5.4): deferred when the
     member flag word has bit 0x1, immediately when it has not;
   - if its member effect is not playing: when the group is not stopping and *t* ≤ start time ≤ *t* + Δ, the member
     effect is played (P2 7). Nothing else happens to it here.
3. **Time**: *t* ← *t* + Δ (`f32`).
4. **Time limit**: if *t* > the time limit, the time limit > 0 and the group is not stopping, the group is stopped
   deferred (section 3). This step does not end the update.
5. **Members**: every member in member order (all members, whatever their flags) is updated (5.3) with *e*,
   collecting a bounding box and whether anything is still playing.
6. **End of a deferred stop**: if the group is stopping and nothing was playing in 5, playing and stopping are
   cleared.
7. **Bounding box**: if the collected box is not empty on any axis, it becomes the group's box, the sphere its
   centre and half its diagonal; otherwise box and sphere are left as they were (5.5).

### 5.3 Updating a member

With *e* (`bin_update.py`):

1. The member effect is updated with *e* (P2 8.2). If it is then playing, something is playing, and its bounding box
   is merged into the collected box (5.5). Only if it is playing, and only when the member flag word has bit 0x2
   and the per-particle child name is not empty: each per-particle child *i* is placed at live particle *i* of the
   member effect (6.3, world mode, by the particle's position without the member effect's local matrix).
2. Each per-particle child, in list order, is updated with *e*. If it is then playing, something is playing and its
   box is merged; if it is not playing and the member flag word has bit 0x10, it is played again (P2 7).
3. Each free child, in list order, is updated with *e*. If it is then playing, something is playing and its box is
   merged; if it is not playing, it is destroyed and removed from the list (the order of the others is kept).

Children made during step 1 (by the member effect's particle notices, section 6) are thus updated with the full *e*
in the same update.

### 5.4 Stopping a member

Stopping a member deferred or immediately stops its member effect, every per-particle child and every free child
in the same way (P2 7). An immediate stop then destroys all per-particle and free children and empties both lists
(`bin_update.py`). An immediate stop of the member effect removes its particles without death notices (P2 7), so it
starts no death children.

### 5.5 Bounding box

The collected box starts empty (minimum +3.4028235 × 10³⁸, maximum −3.4028235 × 10³⁸ on each axis). Merging a box
takes the minimum of the minima and the maximum of the maxima on each axis; a box is merged only when it is not empty
(minimum ≤ maximum on all three axes). The effects' boxes are P2's (8.2, 8.3) (`bin_update.py`).

### 5.6 Consequences

(From 5.2 – 5.4; `sim.py`, `data.py`.)

- **Which members play**: only members with flag bit 0x4; the others' effects are made and placed but never played.
- **Start**: a used member's effect is played in the first update whose interval [*t*, *t* + Δ] contains its start
  time, if it is not playing then and the group is not stopping; a start time of 0 is in the first update after
  play. A member effect that has ended (by its own time limit, P2, or a stop) is not played again unless an
  interval contains its start time again (a replay of the group restarts *t* at 0). No member starts after its
  group's deferred stop.
- **Stop time**: a playing used member is stopped when an interval contains its stop time. A stop time of 0 lies in
  an interval only while *t* = 0: in the first update after play the member effects of start time 0 are not playing
  yet, so they are not stopped then; they are stopped by an update with *t* = 0 that finds them playing (an update
  with *e* = 0 right after play, or the first update after a replay while they still play).
- **Time limit**: when *t* first exceeds a positive time limit, the group is stopped deferred: no member starts any
  more, the effects and children run on until they have ended, and the stop-time tests still apply (an immediate
  member stop cuts that member's particles). The group ends (5.2 step 6) in the first update in which nothing of it
  is playing. A group without a time limit (limit 0) stays playing, even when all its effects have ended, until its
  owner stops it.

## 6. Child effects

### 6.1 Birth of a particle of a member effect

When a member effect adds a particle (P2 3.2, 6), in this order (`bin_children.py`):

1. the effect's own birth notice (frame values, P2 3.2, P7);
2. if the member flag word has bit 0x20: a **birth child** is made from the birth child name at the new particle
   (6.4);
3. if the member flag word has bit 0x2: a **per-particle child** is made from the per-particle child name at the new
   particle (6.3) and appended to the per-particle children.

### 6.2 Death of a particle of a member effect

When a member effect removes particle *i* (P2 3.2; reported before the last particle moves into place *i*), in this
order (`bin_children.py`):

1. if the member flag word has bit 0x2: per-particle child *i* is stopped deferred and appended to the free children;
   the last per-particle child moves into place *i* and the list shortens by one, so per-particle child *i* stays the
   child of particle *i*;
2. if the member flag word has bit 0x40: a **death child** is made from the death child name at the dying particle
   (6.4).

The steps that use a child name (6.1, 6.2, 5.3) each run only with that name's bit, so a name in a slot whose bit
is clear is not used (`bin_children.py`, `bin_update.py`).

### 6.3 Making a child at a particle

A child effect is made by looking up its name among the library's effects as in 2.1 (an empty or unknown name is the
assertion failure `Particle effect doesn't exist`) (`bin_children.py`). From the particle's position **x** and
previous position **x**ₚ:

- velocity **v** = (**x** − **x**ₚ) · *q*, *q* = 1 / 0.033 in `f32` (30.303030), per component; a newborn particle
  has **x** = **x**ₚ (P2 6), so a child made at birth gets **v** = (0, 0, 0);
- when the member effect is in world mode: *M*′ = the identity with row 3 = (**x**, 1);
- when the member effect is in local mode with kept matrix *K* (P2 5.1): **v** ← [**v** 0] · *K*, and *M*′ = *K* with
  the first three entries of row 3 replaced by [**x** 1] · *K*.

The child is played (P2 7) and then placed by *M*′ and **v** in world mode (P2 5.1). The source parent velocity of
its `source` actions thus comes from **v** (P2 5.2).

Afterwards (5.3 step 1), per-particle children are placed again at every update in which the member effect is
playing: per-particle child *i* by the identity with row 3 = (**x**ᵢ, 1) and **v**ᵢ = (**x**ᵢ − **x**ₚᵢ) · *q* of
live particle *i*, in world mode, without *K* also when the member effect is in local mode. Free children are never
placed again.

### 6.4 Birth and death children

A birth or death child is made as in 6.3 at the particle, but before it is played it is tested: if it is looped
(section 4: an effect without a time limit), the game reports the fatal error
`Can't use looped effect '%s' as 'On Birth' child for group.` (with the child's name, also for a death child) and
makes no child (2.3). Otherwise it is played, placed, and appended to the free children (`bin_children.py`). A free
child runs until it is no longer playing (its time limit and the end of its particles, P2) and is then destroyed
(5.3 step 3).

## 7. Member flag values

The program's group handling tests exactly the member flag bits 0x1, 0x2, 0x4, 0x10, 0x20 and 0x40
(`bin_children.py`): 0x4 selects the members that play (5.2), 0x1 makes the stop at the stop time deferred instead
of immediate (5.2), 0x2 per-particle children (6.1 – 6.3, 5.3), 0x10 their replay (5.3), 0x20 birth children (6.1,
6.4), 0x40 death children (6.2, 6.4). Correction to P1 6.2, which lists 0x1 as not tested: it is tested here.
Without 0x2 there are no per-particle children, so 0x10 alone does nothing. The other bits of 0x23E004 are not tested.

What each of the 17 values of the library does (counts of members: P1 6.2, `data.py`):

| Value | Members | Plays | At its stop time | Children |
|---|---|---|---|---|
| 0x0 | 48 | no | — | none |
| 0x4 | 878 | yes | immediate stop | none |
| 0x5 | 44 | yes | deferred stop | none |
| 0x6 | 26 | yes | immediate stop | per-particle |
| 0x7 | 2 | yes | deferred stop | per-particle |
| 0x14 | 1 | yes | immediate stop | none (0x10 without 0x2) |
| 0x16 | 1 | yes | immediate stop | per-particle, replayed when ended |
| 0x20 | 5 | no | — | none (no particles are born) |
| 0x24 | 18 | yes | immediate stop | birth |
| 0x26 | 4 | yes | immediate stop | per-particle, birth |
| 0x36 | 1 | yes | immediate stop | per-particle replayed when ended, birth |
| 0x40 | 1 | no | — | none |
| 0x44 | 10 | yes | immediate stop | death |
| 0x46 | 2 | yes | immediate stop | per-particle, death |
| 0x64 | 2 | yes | immediate stop | birth, death |
| 0x66 | 3 | yes | immediate stop | per-particle, birth, death |
| 0x23E004 | 1 | yes | immediate stop | none |

"At its stop time" applies only when the stop time lies in an interval while the member effect plays (5.6).

## 8. Members and children in the data

(`data.py`, decoding with P1's text.)

- Members with bit 0x4: 993 of 1,047. The 3 groups without members (`light_fx\light_clear_01---`,
  `light_fx\light_rainy_01---`, `light_fx\projector_clear_01---`) are the only groups in which no member plays.
- Of the 993: 212 have a start time above 0; 118 a stop time above 0 (21 of them with bit 0x1); none has a stop time
  above 0 and not above its start time; none starts after its group's time limit (when the limit is above 0); 7
  have a stop time above their group's positive time limit (so they can be stopped while the group drains); 349
  name an effect without a time limit (played until stopped) and 624 one with a time limit; 20 name an effect that
  is not in the library (all 0x4, the members of `explosions\old\explosion-02`, `-021`, `-031`, `-042`, P1 6.1), so
  making one of these four groups is an assertion failure (2.1).
- No member slot names a group. 89 groups have no time limit.
- Per-particle children (bit 0x2, all 39 such members play): 38 name an effect of the library (14 of them without a
  time limit, which is allowed for these); one, in `anomaly2\pux_blast00` (value 0x6), has an empty name, so the
  first particle its member effect adds is an assertion failure (6.3).
- Birth children (bit 0x20 on a member that plays: 28): all name an effect of the library with a time limit. 5
  members with 0x20 do not play.
- Death children (bit 0x40 on a member that plays: 17): 16 name an effect of the library with a time limit; the one
  of `static\bubble` (`static\buble_distort01`) is not in the library, so the first particle that dies in that
  member is an assertion failure (6.3). One member with 0x40 does not play.
- The names of the six groups above that cannot be shown without an error (the four `explosions\old\…` groups,
  `anomaly2\pux_blast00`, `static\bubble`) occur in no file of the merged archives other than `particles.xr` and in
  none of the shipped binaries (`bin\`), as lower-case text.

## 9. Verification

### 9.1 Timeline vectors

(`data.py`, with `sim.py`.) A group is played and then updated with *e* = 33 ms each time unless stated; update
numbers start at 1; members are numbered from 0. "Started" means the member effect is played by 5.2 step 2. Stops are
listed only for member effects without a time limit, which certainly still play then; when an effect with a time limit
ends depends on its particles (P2 – P4).

- `explosions\campfire_05` (the fires of the first level; P1 8): members 0, 3, 4, 5, 6 start in update 1, member 2
  (`campfire_02_sparks`, start time 0.5) in update 16; member 1 (`campfire_03_smoke`, value 0x0) never. No time
  limit.
- `anomaly2\electra_idle`: members 0 and 1 start in update 1; member 1 (value 0x24) makes a birth child
  `anomaly2\group_items\electra_disk_00` (an effect with a time limit) for each particle its effect adds; member 0
  (value 0x4) names a per-particle and a birth child, which are not used.
- `anomaly2\teleport` (time limit 1.1): starts: update 1 members 0, 5, 7, 12; 13: 4; 19: 3; 26: 8; 28: 1, 6, 9; 29:
  10; 31: 11. Member 0 (no time limit, stop time 1.0, value 0x4) is stopped immediately in update 31. The group is
  stopped deferred in update 34. With *e* = 16 instead: starts in updates 1 (0, 5, 7, 12), 25 (4), 38 (3), 54 (8),
  57 (1, 6, 9), 60 (10), 63 (11); member 0 stopped in 63; the group stopped in 69.
- `destroy_fx\destroy_small_box_00` (time limit 1; member 0 value 0x23E004 with stop time 4.48 × 10⁻⁴⁴, member 1
  stop time 0): with *e* = 33: both start in update 1, the group is stopped deferred in update 31. With *e* = 0, 33,
  33: both start in update 1, and in update 2 (*t* = 0, interval [0, 0.033]) both are stopped immediately (their
  effects, with time limits 1 and 1.5, still play then).
- Over the 248 groups that can be made (all but the four of section 8), *e* = 33: all 973 used members start, in
  updates summing to 6,843; the 159 groups with a time limit are stopped deferred in updates summing to 153,160;
  the others never by their time limit.

## 10. Unestablished

- What follows an assertion failure or a fatal error in the running game (2.3; believed: the game ends).
- The meaning of the group flag word (P1 6); the group's handling does not read it (`bin_state.py`).
- How often owners update, place, play and stop their groups, and when they destroy them (the owners'
  specifications).

## 11. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): `particles.xr` from `gamedata.db4`
  (identical copy in `gamedata.dba`); binaries `bin\xrRender_R1.dll` (MD5 `2bf95c8376b735daa6d82b8944894be0`),
  `bin\xrParticles.dll` (MD5 `22bad4d258e6211c6ea7cc8dc8d7ca55`), `bin\XR_3DA.exe` (MD5
  `ba3ad0127bff6619ed991a7c82d8b946`; the looped test only) and `bin\xrCore.dll` (MD5
  `eab71f90f01575eca3500e623873e57c`; the error reporting only).
- Data examined: every group and member of `particles.xr` and the effects they name, decoded with a throwaway parser
  written from P1's text; a text search of every non-media file of the merged archives and of the shipped binaries
  for six group names.
- Analysis: static analysis (Ghidra 12.2, headless) of the first renderer's group instance (making it and its member
  effects, placement, play, stop, the update, the member update and stop, the member effects' particle notices and
  the child effects), of the shared looped test in `XR_3DA.exe` and of the error reporting in `xrCore.dll`. The
  timeline was reproduced by a throwaway implementation written from this text (`sim.py`). No execution or debugging
  of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/particle-groups/` reproduce every fact.

## Gaps found in review

- What destroying a group instance does to its member effects, per-particle children and free children is not stated.
- What giving a record to an instance that already has members and children does (whether the old member effects and children are stopped, destroyed or kept) is not stated.
- Child effects (6.3, 6.4) are not stated to have the notices of an effect alone (P2 3.2), i.e. that their own particles' births and deaths start no further children.
- `docs/formats/particles-xr.md` 6.2 describes a per-particle child as removed at its particle's death; 6.2 here has it stopped deferred and kept as a free child until it is no longer playing, and particles-xr.md's row does not say so (besides the bit 0x1 correction of section 7).
