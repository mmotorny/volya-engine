# Effects placed by the level (`level.ps_static`)

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of the particle effects a level places through its `level.ps_static` file: how the
file is read, what is made from each record (an effect or a group), where it is placed, when it starts, how it is
updated over time, and when it ends, for all 12 shipped files. It is specification P8 of
`docs/behavior/particles-and-lights-scope.md`, including that spec's review gap: 16 of the 22 names in these
files are groups, and section 6 states what the level does with a group name. What could not be established is
marked **unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/behavior/level-loading.md` 5.9 (the record: effect name and a 64-byte transform) and 2.3 (the loading
  image after loading);
- `docs/formats/particles-xr.md` (P1: effect and group records, the effect flag word, the group time limit,
  members);
- `docs/behavior/particle-lifecycle.md` (P2: an effect instance; its placement (section 5), play and stop
  (section 7), and update with an elapsed time in milliseconds (section 8));
- P6 of the scoping specification (particle groups: what a group instance does when placed, played and
  updated), P7 (drawing, including which effects are drawn in a frame);
- `docs/formats/chunks.md` (sections 4 and 5: sequential iteration, compressed chunks);
- `docs/behavior/autosave-timing.md` (section 1: the world clock; section 2.3: the engine's scheduler, its due
  rule and its per-frame budget);
- `docs/behavior/object-visual-and-transform.md` (matrix convention: row vectors, a point **p** placed as
  [**p** 1] · *M*; row 3 the translation, section 4).

Conventions: *M*[*r*][*c*] is row *r*, column *c* of a 4 × 4 matrix; the 16 stored floats are *M* row by row.
Arithmetic is IEEE single precision (`f32`) unless stated. "The clock" is the world clock of autosave-timing.md
section 1 (milliseconds, unsigned 32-bit; it stands still while the game is paused). "Instance" is the effect or
group instance made from a record (section 2). Every fact carries the name of the check that reproduces it, in
`checks/level-placed-effects/` of the dirty workspace: [B*n*] in `bin.py` (static analysis prepared by
`ghidra.py`), [D*n*] in `data.py` (game data extracted by `extract.py`, decoded by a throwaway parser written from
this text and P1's); `run_all.py` runs them all.

## 1. The file

### 1.1 When it is read

`level.ps_static` in the loaded level's directory (`$level$`) is read once each time a level is loaded [B1]. The
level-load sequence is: `level.ltx`, the `level` file, the collision form, the renderer's level data, the
environment modifiers, a game step (`level.ai`, `level.game`), the level's objects, then the step that reads this
file; the same step then reads `level.snd_static`, `level.som` and the random sounds [B1]. The file is optional:
when it does not exist, the level has no such effects and the step continues [B2].

### 1.2 Layout

The file is a chunk sequence read by sequential iteration (chunks.md section 4): every top-level chunk, in stored
order and whatever its id, is one **record**; a compressed chunk is decompressed first (chunks.md section 5)
[B2]. The record is the chunk's payload, fields back to back from offset 0 (level-loading.md 5.9):

| Field | Type | Meaning |
|---|---|---|
| **name** | zero-terminated string, Windows-1251 | effect or group name of `particles.xr` (section 2) |
| **transform** *M* | 16 × `f32` | 4 × 4 matrix, row by row (section 3) |

The name is read into a 256-byte buffer: a name of 256 bytes or more (without the terminator) fails an assertion
("Dest string less than needed."); the copy stops at the terminator or at the end of the payload [B2]. Bytes after
the 64 transform bytes are not read; what a payload too short for the transform gives is **unestablished**.

Shipped files [D1, D2]: the 12 levels `l01_escape` (83 bytes), `l03u_agr_underground` (772),
`l04u_labx18` (1,186), `l05_bar` (224), `l08_yantar` (89), `l08u_brainlab` (86), `l10u_bunker` (474),
`l11_pripyat` (372), `l12_stancia` (1,995), `l12_stancia_2` (7,190), `l12u_control_monolith` (204),
`l12u_sarcofag` (284) have one. Their chunks are numbered 0, 1, 2, … in order, none is compressed, and every
payload holds exactly the name, its terminator and the 64 transform bytes; the files hold 1, 8, 12, 2, 1, 1, 5, 4,
21, 73, 2 and 3 records, 133 in all.

## 2. What a record makes

For each record, in file order, one instance is made and kept by the level (section 7) [B3, B10]:

1. The name is looked up among the effects of `particles.xr`; if found, an **effect instance** is made from that
   effect record (P2) [B3].
2. Otherwise it is looked up among the groups; if found, a **group instance** is made from that group record
   (P6) [B3].
3. If it is neither, the program reports the assertion "Particle effect or group doesn't exist" (believed fatal,
   as other assertions); what follows is **unestablished** [B3].

Records are independent: two records with the same name, even byte-identical ones, make two instances, placed and
updated separately (section 8 lists the shipped duplicates) [B3, D6].

Shipped names [D3, D5]: 22 distinct, all in `particles.xr`, none both an effect and a group; the longest has 49
bytes. 6 are effects (54 records): `damage_fx\heli_smoke_01`, `static\fire_distort`, `static\fire_distort_big`,
`static\static_lava_02`, `static\static_stancia2`, `tests\muhi`. 16 are groups (79 records), with 63 members in all
(56 of them with member flag bit 0x4), every member naming an effect of the library.

## 3. Placement

Before placing, *M*[3][1] (the y of the translation) is increased by 0.01 (`f32` 0x3C23D70A), so the effect sits
1 cm higher than stored; nothing else of *M* is changed or checked [B2]. The instance is then placed once (P2
section 5.1 for an effect, P6 for a group) [B4]:

- by the matrix *M* (after the change above),
- with the velocity **w** = (0, 0, 0),
- in world mode (not local mode).

For an effect this sets its position to row 3 of *M* and places its action list by *M* (P2 5.1, 5.2): actions
with action flag bit 0x2 follow *M*'s rotation, the others only its translation. The level never places an
instance again [B10]. The instance is also drawn with an object matrix that is the identity and is never changed
on this path, so the particles of a world-mode effect are drawn where the effect put them (P7) [B8].

Shipped transforms [D4]: in every record column 3 is (0, 0, 0, 1) and rows 0 – 2 are orthonormal (largest
deviation 1.5 × 10⁻⁷) with determinant +1, so *M* is a rotation and a translation, without scale; 106 records hold
exactly the identity rotation and 27 a rotation.

## 4. Start

Right after placement, still in the load step of section 1.1, the instance is played and updated once [B4, B5]:

1. it is played (P2 section 7 for an effect; P6 for a group);
2. its last-update time is set to the clock minus 33, and it is updated at once with the elapsed time clock −
   last-update time = 33 ms (P2 section 8.2: for an effect, one step); the last-update time becomes the clock
   [B5];
3. its visibility sphere is updated (section 5.4).

The clock is the value taken at the start of the frame in which loading runs (autosave-timing.md section 1; believed:
the clock is not advanced during the load), so the time the rest of the load takes is added to the first update
after loading (section 5.3).

## 5. Updates

### 5.1 The elapsed-time update

Every update of an instance after the start (by the schedule, 5.2, or by drawing, 5.3) is the same [B5, B6, B8]:
*e* = clock − last-update time (unsigned 32-bit); if *e* = 0 nothing is updated; otherwise the instance is updated
with *e* ms (P2 section 8.2 for an effect, P6 for a group) and the last-update time becomes the clock. In both
cases the visibility sphere is then updated (5.4); after a drawing update the instance is drawn (P7).
**Disputed in review:** only the start update (section 4) and the scheduled update (5.2) update the visibility
sphere afterwards; the drawing update (5.3) does not: it updates the instance (when *e* ≠ 0) and draws it, and
leaves the registered sphere as it was. Since both
routes share the last-update time, the instance is given each elapsed millisecond of the clock once, at most one
update per clock value; how much of it an effect simulates follows from P2 8.2 (whole steps of 33 ms, at most
three per update, the rest of a longer time dropped).

### 5.2 The schedule

Each instance is a task of the engine's scheduler (autosave-timing.md section 2.3), registered when it is made
(section 2), in the scheduler's prioritised queue (not the queue of tasks run every frame) [B6]:

- It is first due at the clock at which its registration takes effect, and runs at the scheduler's first pass in a
  frame whose clock is past that time [B6].
- Each run: the scheduler computes the next period from a **scale** *s* taken just before, runs the update of 5.1
  (the scheduler's own elapsed time is not used), and sets the next due time to the clock of the pass plus the
  period [B6]. The task is never dropped by the scheduler [B6].
- The instance's configured update interval has a minimum of 20 ms and a maximum of 50 ms. With *lo* = max(20,
  30) = 30 and *hi* = (50 + 1000) div 2 = 525, the period is *lo* + trunc((*hi* − *lo*) · *s*) (the product
  rounded to `f32`, then truncated toward zero), clamped to 30 … 525 ms [B6].
- *s* = *d* · 0.005 (`f32` 0x3BA3D70A), where *d* is the distance from the camera position to the centre of the
  instance's bounding sphere (for an effect, P2 8.2 and 8.3: the box of its live particles, or its position ±
  0.001 when not playing; for a group, P6) [B6].

The period is thus 30 ms at the camera and grows by 2.475 ms per metre to 525 ms at 200 m and beyond (test
vectors in section 8). Combined with P2 8.2, an effect that is not drawn and is updated only by the schedule runs
in real time while the period is at most 99 ms (30.3 steps per second), and slower beyond: 3 steps per period, e.g.
19.6 steps per second at 153 ms and 5.71 at 525 ms; the rest of the clock time is dropped [D9]. Which frames run
the scheduler while the loading image of level-loading.md 2.3 is shown, and the scheduler's per-frame budget
(autosave-timing.md 2.3), are **unestablished**.

### 5.3 Drawing

In each frame in which the renderer draws the instance (which instances are drawn, and when: P7), the update of
5.1 runs first, then the instance is drawn [B8]. An instance in view is therefore updated every drawn frame, with
that frame's elapsed clock time. The first update after loading (by the schedule or by drawing) receives the whole
time since the start update of section 4, including the rest of the load, of which an effect simulates at most
three steps (P2 8.2) (believed; from 4 and 5.1).

### 5.4 The visibility sphere

After placement and after every update (**Disputed in review:** after placement, the start update and every
scheduled update; not after a drawing update, 5.1), the instance's bounding sphere (centre and radius, P2 8.3 for an effect,
P6 for a group) is placed by the object matrix (the identity, section 3) and handed to the renderer's spatial
index (believed: the index by which P7 finds what to draw) [B9]:

- nothing is done unless the three centre coordinates and the radius are each a normal number or zero (not
  infinite, not NaN, not denormal) [B9];
- the first time, the sphere is registered [B9];
- afterwards it is moved only when a centre coordinate has changed by at least 0.01 (`f32` 0x3C23D70B) or the
  radius by at least 0.15 (`f32` 0x3E19999A) since the last registered or moved sphere; otherwise the registered
  sphere is kept [B9].

## 6. Groups

A group name makes a group instance (section 2, step 2) where an effect name makes an effect instance, and the
level then does exactly the same with it: the same placement (section 3: *M* with y + 0.01, velocity 0, world
mode), the same play and 33 ms start update (section 4), the same scheduled and drawing updates with the elapsed
clock time (section 5), the same visibility sphere (5.4) and the same end (section 7) [B3, B4, B5, B6, B8]. The
effect and the group answer the same requests (place, update with *e* ms, play) [B4]. What a group instance does on
them — which members play, when they start and stop, the group time limit, the members' child effects, and its
bounding sphere — is P6's. The level never addresses a group's members itself.

## 7. End

The level never plays, stops, places or removes an instance again after section 4 [B10]. The length an effect or
group reports (P2 section 7; P6) only sets values that matter when an instance removes itself at the end of its
life, which these instances never do [B7]. An instance therefore ends only:

- **on its own**, if its effect or group stops by its own time limit (P2 8.3 for an effect, P6 for a group); it is
  then never played again by the level. No shipped name does: none of the 6 effects has flag bit 0x4000 (no time
  limit; flag words 0x1001, and 0x1401 for `tests\muhi`), and all 16 groups have time limit 0 [D5], so every
  shipped level effect runs from the load until the level is destroyed (inside a group, members follow P6);
- **with the level**: when the level is destroyed, every instance it made is destroyed, and its scheduled and
  drawing updates stop [B7, B10].

Each load of a level therefore starts its effects afresh (believed to include loading a saved game, which loads
the level anew). Whether a load that reloads the simulation in place keeps them is **unestablished**.

## 8. Verification

On the data [D1 – D8]:

- File sizes and MD5 prefixes (levels in the order of 1.2): `fbb89333`, `2cc3c057`, `9c7869da`, `a5caa24b`,
  `eeeb4561`, `588b2fde`, `d3457be2`, `b1d0bd70`, `0a32ed8f`, `320319c3`, `90c948a9`, `1d643f9e` [D1].
- `l01_escape`: one record, effect `tests\muhi`, identity rotation, row 3 (−255.712234, −18.832966, −127.245377)
  (`f32` bits 0xC37FB655, 0xC196A9EA, 0xC2FE7DA2); placed with row 3 (−255.712234, −18.822966, −127.245377)
  (y bits 0xC196956F) [D7].
- Records per name: `static\static_stancia2` 37, `explosions\campfire_05` 14, `explosions\gas_light` 12,
  `explosions\campfire_06` 11, `industrial_particles\exhaust_isparenie_blue_down` 10, `explosions\campfire_08` 9,
  `static\fire_distort` 8, `explosions\campfire_07` 7; 3 each `explosions\campfire_boar_grill`,
  `static\static_lava_02`, `damage_fx\heli_smoke_01`, `explosions\expl_car_smoke_big`; 2 each
  `industrial_particles\exhaust_isparenie_white_down`, `static\fire_distort_big`, `anomaly2\control_monolit_holo`;
  1 each `tests\muhi`, `industrial_particles\exhaust_isparenie_white_hor`, `static\fire_tube`, `anomaly2\mozg`,
  `anomaly2\generatory`, `static\static_lava_particles`, `anomaly2\monolith` [D3].
- Byte-identical records: only in `l11_pripyat`, records 0 and 2, and 1 and 3; each makes its own instance [D6].
- Sum of the placed translations (rows 3, after the y change) over all 133 records, in double precision:
  (37,576.653, 1,869.736, 12,318.863) [D8].
- All names are lower case [D3].

Test vectors [D9]:

- Scheduled period by distance *d* (section 5.2): 0 m → 30 ms, 10 m → 54, 50 m → 153, 100 m → 277, 150 m → 401,
  200 m → 525, 1,000 m → 525.
- Steps per second of an effect updated only by the schedule at a fixed period (P2 8.2, accumulator carried):
  30 ms → 30.2 (**Disputed in review:** 30.2 is the rate over a 10 s window; the long-run rate at a 30 ms period
  is 1000 / 33 = 30.3, as at 99 ms), 99 ms → 30.3, 153 ms → 19.6, 525 ms → 5.71.

## 9. Unestablished

- What a payload too short for the 64 transform bytes gives (1.2).
- What follows the assertion for a name that is neither an effect nor a group (2).
- Which frames run the scheduler while the post-load loading image is shown, and the scheduler's per-frame budget
  (5.2).
- Whether a load that reloads the simulation in place keeps the instances (7).
- Group behaviour itself (P6) and which frames draw an instance (P7) are outside this specification.

## 10. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): the 12 `levels\*\level.ps_static` files
  and `particles.xr` (merged archive view; `particles.xr` from `gamedata.db4`, identical in `gamedata.dba`);
  binaries `bin\xrGame.dll` (MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), `bin\XR_3DA.exe`
  (`ba3ad0127bff6619ed991a7c82d8b946`), `bin\xrRender_R1.dll` (`2bf95c8376b735daa6d82b8944894be0`),
  `bin\xrCore.dll` (`eab71f90f01575eca3500e623873e57c`).
- Data examined: every record of the 12 files; the effect and group records they name, decoded with a throwaway
  parser written from P1's text and this text.
- Analysis: static analysis (Ghidra 12.2, headless; decompiles, instruction listings, import and export tables)
  of the level-load sequence and the step that reads the file, the object made per record (creation by name,
  placement, play, scheduled and drawing updates, the scheduling scale, the visibility sphere, destruction with
  the level), the engine's scheduler (registration, period, due rule), and the string read of the core library.
  No execution or debugging of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/level-placed-effects/` reproduce every fact.

## Gaps found in review

- The name lookup of section 2 is not stated to be exact; it compares names byte for byte, case-sensitively, against the library's effect names and then its group names (established in review).
- The precision of the scale is not stated: the distance and *s* = *d* · 0.005 are computed in the processor's extended floating-point unit and are not rounded to `f32` before the product (*hi* − *lo*) · *s*, which alone is rounded to `f32`; the effect on periods at exact millisecond boundaries is not stated.
