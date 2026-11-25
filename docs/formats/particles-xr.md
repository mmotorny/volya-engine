# `particles.xr` (format)

Format specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the particle effect library
`particles.xr`: the container, every chunk of the effect and group records, the effect flag word, the action
list with the record layouts of all 22 action types the library uses (and the sizes of the other ids the game
accepts), the 11 region kinds, and the group records with every member field and member flag value. It is
specification P1 of `docs/behavior/particles-and-lights-scope.md`. What the actions, regions, flags and groups
do over time is left to that spec's behaviour specifications (P2 – P7), which this text names where a field's
effect belongs to one of them. What could not be established is marked **unestablished**; what is inferred
rather than read from the data or the program is marked **believed**.

Builds on (not repeated): `docs/formats/chunks.md` (chunk = `u32` id word, `u32` size, payload; sequences,
lookup by id, lists of sub-chunks 0, 1, 2, …), `docs/formats/db-archive.md` (archive paths).

Conventions: little-endian; `u16`, `u32`, `i32`, `f32` (IEEE single); `float3` = 3 × `f32` (x, y, z);
`stringZ` = bytes up to and including a terminating zero byte, Windows-1251. Offsets are from the start of the
payload or record named. "Region" names the 68-byte records of section 5 (the data holds no word for them;
scoping spec). Action type names are the words the effects' own chunk 36 pairs with each type id
(section 4.5). Every fact carries the name of the check that reproduces it, a script in
`checks/particles-xr/` of the dirty workspace; `run_all.py` runs them all (`archive.py` extracts the file,
`bin_setup.py` prepares the static analysis that the `bin_*.py` checks read; `pxr.py` is a decoder written from
this text that the other checks use).

## 1. The file

`particles.xr` is stored in `gamedata.db4` and `gamedata.dba` and in no other of the 15 archives (the 14
`gamedata.db*` and `mods\mp_military_2.xdb0`); both copies are 1,394,071 bytes with CRC-32 0x4E93C6F1 and are
identical (`archive.py`). No chunk at any level of the file is compressed and no record contains a duplicate
chunk id (`container.py`).

The file is a chunk sequence with three top-level chunks, stored in this order (`container.py`):

| id | Size | Content |
|---|---|---|
| 1 | 2 | `u16` library version = 1 |
| 3 | 1,299,914 | effect records: sub-chunks with ids 0, 1, 2, …, one effect each (section 2); 1,075 of them, ids 0 – 1,074 in stored order |
| 4 | 94,131 | group records: sub-chunks with ids 0, 1, 2, …, one group each (section 6); 252 of them, ids 0 – 251 in stored order |

How the game reads the file (first renderer; `bin_loaders.py`):

- Chunk 1 must be present (otherwise an assertion failure). If its `u16` is not 1, no effect and no group is
  loaded and the game logs `PS Library: Unsupported version.`
- The effects are read from the sub-chunks of chunk 3 by looking up id 0, 1, 2, … until an id is not found
  (chunks.md section 2); the groups likewise from chunk 4.
- A record that fails to load (section 7) is discarded and ends its list: no further record of that list is
  read.
- After loading, the effects and the groups are each ordered by name, comparing names as unsigned bytes. The
  stored order of the effects is not name order; the stored order of the groups is (`container.py`).

Names (`container.py`): the 1,075 effect names are distinct, the 252 group names are distinct, and no name is
both. All names are lower case, 10 – 57 characters, from the set `a`–`z`, `0`–`9`,
`\`, `_`, `-`, `(`, `)`, `!` and space; `q` does not occur, and `j` only in group names.

## 2. Effect record

The payload of a chunk-3 sub-chunk is itself a chunk sequence. Its chunks:

| id | Size | Content | Presence (in the 1,075 effects) |
|---|---|---|---|
| 1 | 2 | `u16` effect version = 1 | all |
| 2 | n | `stringZ` effect name; the chunk holds exactly the name and its terminator | all |
| 3 | 4 | `u32` maximum number of live particles | all |
| 4 | n | action list (section 4) | all |
| 5 | 4 | `u32` effect flag word (section 3) | all |
| 7 | n | sprite: two `stringZ`, engine shader name and texture name (section 2.2) | exactly when flag bit 0x1 is set: 1,070 |
| 6 | 28 | texture frames (section 2.1) | exactly when flag bit 0x400 is set: 423 |
| 8 | 4 | `f32` time limit (seconds) | exactly when flag bit 0x4000 is set: 623 |
| 33 | 12 | collision values (section 2.3) | exactly when flag bit 0x10000 is set: 71 |
| 34 | 12 | `float3` velocity scale (section 2.4) | exactly when flag bit 0x40000 is set: 113 |
| 37 | 12 | `float3` alignment angles (section 2.4) | exactly when flag bit 0x8000 is set: 239 |
| 35 | n | editor stamp (section 2.5); not read by the game | all |
| 36 | n | editor description of the actions (section 4.5); not read by the game | all |

(`effect_chunks.py`, `container.py`.) In every effect the chunks are stored with 1, 2, 3, 4, 5 first, in that
order, and 35, 36 last; the optional chunks between them always follow the order 7, 6, 8, 33, 34, 37; 32
distinct sequences occur (`effect_chunks.py`). Chunk 4 is 400 – 2,448 bytes, chunk 2 11 – 58 bytes. The
maximum particle counts range from 1 to 5,000 and sum to 57,143 (`effect_chunks.py`).

How the game reads an effect record (`bin_loaders.py`):

- Chunks 1, 2 and 3 must be present (otherwise an assertion failure); if the version is not 1, the record
  fails to load.
- Chunk 4 must be present and non-empty (otherwise an assertion failure); its bytes are kept as they are and
  decoded as section 4 describes.
- Chunk 5 is copied into the flag word (believed 0 if the chunk is absent; the word starts as 0).
- Chunk 7 is read when the flag word has bit 0x1, chunk 6 when it has 0x400, chunk 8 when it has 0x4000,
  chunk 33 when it has 0x10000, chunk 34 when it has 0x40000; for each of these the chunk must then be present
  (otherwise an assertion failure). Chunk 37 is read when the flag word has 0x8000 and the chunk is present;
  without it the default below is kept.
- Chunks 35 and 36 are not looked up anywhere in the first renderer.

Values kept when a chunk is not read (`bin_loaders.py`): texture frames: width 0.125, height 0.5, frames per
row 8, frame count 16, frame rate 24 (offsets 8 and 12 are left unset); time limit 0; collision values
(1, 0, 0); velocity scale (0, 0, 0); alignment angles (−1.5707964, 0, 0), i.e. −π/2 in `f32`. The game reports an
effect's length as −1 when the flag word lacks 0x4000, else as the chunk-8 time limit (`bin_loaders.py`).

### 2.1 Chunk 6: texture frames (28 bytes)

| Offset | Type | Field |
|---|---|---|
| 0 | `f32` | frame width, in texture coordinates |
| 4 | `f32` | frame height, in texture coordinates |
| 8 | `f32` | not read by the game (believed) |
| 12 | `f32` | not read by the game (believed) |
| 16 | `i32` | frames per row |
| 20 | `i32` | frame count |
| 24 | `f32` | frame rate, frames per second |

The drawing reads the width, height and frames per row; the frame animation reads the frame count and the
rate; neither reads the values at offsets 8 and 12 (`bin_flags.py`). How frames are chosen and animated is P7's.
In the 423 chunks (`effect_values.py`): frames per row 1 – 16; frame count 1 – 200; frame rate 0 – 144; width
× frames per row = 1 (to 0.002) in 418; height × ⌈frame count / frames per row⌉ = 1 (to 0.002) in 342; the
values at offsets 8 and 12 are (0, 0) in 107; the most common other pairs are (128, 128) (76), (128, 146) (60) and (64, 64) (52).

### 2.2 Chunk 7: sprite

Two `stringZ` filling the chunk: the engine shader name and the texture name, resolved as in
`docs/behavior/static-shading.md`. The 1,070 sprites name 6 engine shaders: `particles\add` (383),
`particles\blend` (322), `particles\alpha_add` (213), `particles\xdistort` (102), `particles\xadd` (49),
`particles\set` (1). No texture name is empty; 52 hold a comma (a list of textures); 191 distinct texture
strings occur (`effect_values.py`).

### 2.3 Chunk 33: collision values (12 bytes)

| Offset | Type | Field |
|---|---|---|
| 0 | `f32` | tangential factor: multiplies a colliding particle's velocity component along the surface, when that component's squared length exceeds the cutoff |
| 4 | `f32` | resilience: the component along the surface normal is reversed and multiplied by it |
| 8 | `f32` | cutoff, compared with a squared speed |

(`bin_flags.py`; the collision step is P5's.) In the 71 chunks the first two values lie in 0 … 1 and the third
is ≥ 0; the most common triples are (1, 0, 0) (23 effects) and (0.5, 0.5, 0) (9) (`effect_values.py`).

### 2.4 Chunks 34 and 37

- Chunk 34, velocity scale (`float3`): the drawing reads x and y, not z (`bin_flags.py`); z is 0 in 110 of the
  113 chunks; x ranges over 0 … 7 and y over 0 … 75 (`effect_values.py`). Its use is P7's.
- Chunk 37, alignment angles (`float3`, radians): the drawing reads x and y, not z (`bin_flags.py`); z is 0 in
  all 239; the most common values are (−π/2, 0, 0) (173 effects), (0, 0, 0) (35), (−1.57, 0, 0) (12) and
  (π/2, 0, 0) (11) (`effect_values.py`). Its use is P7's.

### 2.5 Chunk 35 (and group chunk 6): editor stamp

Two `stringZ` followed by two `u32`, filling the chunk; group chunk 6 has the same layout (`editor.py`). The
strings look like network computer and user names (889 of the 1,075 effect stamps and 222 of the 252 group
stamps start both strings with `\\`); the two `u32` read as Unix times of 2003-04-24 … 2007-02-12 in the
effects (1972-03-16 … 2007-02-12 in the groups), the first not later than the second in 1,073 effects and 251
groups (`editor.py`). The game does not read these chunks (`bin_loaders.py`).

## 3. Effect flag word (chunk 5)

The union of the 1,075 flag words is 0xFFFC01; 133 distinct words occur (`flags.py`). The game tests exactly
the bits of this table (`bin_flags.py`):

| Bit | Effects | Meaning (owner of the behaviour) |
|---|---|---|
| 0x1 | 1,070 | the effect has a sprite (chunk 7); without the bit the effect is not drawn (P7) |
| 0x400 | 423 | the sprite uses texture frames (chunk 6) (P7) |
| 0x800 | 421 | the frames are animated over time; the animation runs only when 0x400 is also set (229 of the 421) (P7) |
| 0x1000 | 339 | each new particle starts on a random frame (P7); set without 0x400 in 86 effects |
| 0x2000 | 95 | with 0x800: each new particle plays its frames backwards with probability ½ (P7); set without 0x800 in 10 |
| 0x4000 | 623 | the effect has a time limit (chunk 8) (P2) |
| 0x8000 | 239 | sprites are aligned to the particle's motion, using the alignment angles (chunk 37) (P7) |
| 0x10000 | 71 | particles collide with the level (chunk 33) (P5) |
| 0x20000 | 40 | with 0x10000: a particle is removed when it collides (P5); set without 0x10000 in 9 |
| 0x40000 | 113 | sprite size grows with speed (chunk 34) (P7) |
| 0x80000 | 2 | with 0x10000: changes which geometry the collision test considers (a query mask of 2 instead of 3); which geometry is **unestablished** (P5) |
| 0x100000 | 51 | with 0x8000: a particle at rest takes a fixed orientation from the alignment angles (P7); set without 0x8000 in 17 |
| 0x200000 | 126 | with 0x8000: a moving particle's sprite lies across its motion (P7); set without 0x8000 in 17 |
| 0x400000 | 12 | face culling on (clockwise faces culled); without it no culling (P7). The game sets Direct3D 9 render state 22 (cull mode) to 2 with the bit, 3 with both bits, 1 without; in the public Direct3D documentation 1 is no culling, 2 clockwise, 3 counter-clockwise |
| 0x800000 | 1 | with 0x400000: counter-clockwise faces are culled instead; its one effect lacks 0x400000 (P7) |

(Counts and co-occurrence: `flags.py`; the tests: `bin_flags.py`.) The five effects without 0x1 are
`alexmx\ps_00`, `alexmx\test1`, `alexmx\test1_00`, `alexmx\test1_00_old00`, `alexmx\test1_old00`
(`flags.py`). The meanings above name what each bit selects; those of 0x8000, 0x100000, 0x200000 and 0x40000 are believed
from the drawing's use of them. The behaviour itself, including the exact orientation and size rules, is the
named spec's.

## 4. Action list (chunk 4)

### 4.1 Layout

| Offset | Type | Field |
|---|---|---|
| 0 | `u32` | record count *n* |
| 4 | … | *n* action records, back to back |

An action record is a 12-byte header followed by the fields of its type (section 4.3):

| Offset | Type | Field |
|---|---|---|
| 0 | `u32` | type id *t*; selects the record's layout |
| 4 | `u32` | action flag word (section 4.2) |
| 8 | `u32` | type id again |

The game reads the count, then for each record the first type id, which selects the layout, and then the flag
word, the second type id and the fields (`bin_actions.py`); the second id is the one kept. In every record of
the library the two ids are equal (`actions.py`). The count is read only when the chunk is non-empty
(`bin_actions.py`). With the sizes of section 4.3 every chunk 4 of the library is consumed exactly
(`pxr.py`, `actions.py`).

The library holds 6,921 action records, 2 – 12 per effect (`actions.py`):

| *t* | Name | Records | *t* | Name | Records |
|---|---|---|---|---|---|
| 1 | `bounce` | 15 | 17 | `randomvelocity` | 2 |
| 4 | `damping` | 14 | 19 | `sink` | 89 |
| 6 | `follow` | 2 | 21 | `source` | 1,087 |
| 7 | `gravitate` | 17 | 22 | `speedlimit` | 36 |
| 8 | `gravity` | 310 | 23 | `targetcolor` | 976 |
| 9 | `jet` | 1 | 24 | `targetsize` | 747 |
| 10 | `killold` | 1,070 | 25 | `targetrotate` | 543 |
| 12 | `move` | 1,070 | 27 | `targetvelocity` | 494 |
| 14 | `orbitpoint` | 174 | 29 | `vortex` | 133 |
| 15 | `randomaccel` | 35 | 30 | `turbulence` | 58 |
| 16 | `randomdisplace` | 33 | 31 | `scatter` | 15 |

1,067 effects have one `source`, 7 have two and 1 has six; 6 effects have no `move` (`actions.py`).

### 4.2 Action flag word

The values in the library are 0x0 (4,767 records), 0x2 (829), 0x3 (238), and, only on `source` records (every
one of which has bit 0x80000000), 0x80000000 (43), 0x80000001 (1), 0x80000002 (492), 0x80000003 (139),
0xA0000000 (7), 0xA0000002 (376), 0xA0000003 (29) (`actions.py`; per type in `actions.py`'s output). No
record outside `source` has a bit above 0x3; no record has 0x40000000.

| Bit | Records | Meaning |
|---|---|---|
| 0x1 | 407 | not tested by the game (`bin_flags.py`); no effect on the game (believed: an editor setting) |
| 0x2 | 1,067 + 1,036 `source` | the action's stored positions and directions are given in the effect's own space and follow the effect's full transform when the effect is placed; without it only the effect's position applies (P2) |
| 0x20000000 | 412 (`source` only) | `source`: a new particle's size is uniform: its three components take the first one generated (P2) |
| 0x40000000 | 0 | `source`: the source is switched off; set and cleared by the game at run time (believed: set while the effect is stopped, cleared when it plays), never set in the data (P2) |
| 0x80000000 | 1,087 (`source` only) | tested only by `source`; with and without it, a new particle is made from the same fields in the same way: no effect on the simulation (`bin_flags.py`) |

(The game tests exactly 0x2 (when placing), 0x20000000, 0x40000000 and 0x80000000, and sets and clears
0x40000000; `bin_flags.py`. The game runs every action of a list in list order without testing the flag word;
`bin_flags.py`.) Counts: bit 0x2 is set in 829 + 238 records of other types and 492 + 139 + 376 + 29 `source`
records; bit 0x1 in 238 + 1 + 139 + 29 records (`actions.py`).

### 4.3 Field layouts of the 22 types

Offsets are after the 12-byte header; "R" is a 68-byte region (section 5). The field names are labels for the
role each value plays, believed from static analysis; every listed field is read by the game when its action
runs or when the effect is placed (`bin_roles.py`), and the stored order is the order the game reads them
(`bin_roles.py`). The behaviour is P2's (`source`, placement, regions), P3's (motion) and P4's (`killold`,
`sink`, `targetcolor`, `targetsize`, `targetrotate`). Record sizes are those the game reads for each id
(`bin_actions.py`).

| *t* | Name | Size | Fields (offset: type label) |
|---|---|---|---|
| 1 | `bounce` | 80 | 0: R surface region; 68: `f32` tangential factor; 72: `f32` resilience; 76: `f32` cutoff (squared speed) |
| 4 | `damping` | 20 | 0: `float3` damping factor per axis; 12: `f32` lower bound, 16: `f32` upper bound, both compared with the squared speed |
| 6 | `follow` | 12 | 0: `f32` magnitude; 4: `f32` epsilon; 8: `f32` maximum radius |
| 7 | `gravitate` | 12 | 0: `f32` magnitude; 4: `f32` epsilon; 8: `f32` maximum radius |
| 8 | `gravity` | 12 | 0: `float3` acceleration |
| 9 | `jet` | 92 | 0: `float3` centre; 12: R acceleration region; 80: `f32` magnitude; 84: `f32` epsilon; 88: `f32` maximum radius |
| 10 | `killold` | 8 | 0: `f32` age limit; 4: `u32` kill-younger switch (0 or not) |
| 12 | `move` | 0 | none |
| 14 | `orbitpoint` | 24 | 0: `float3` centre; 12: `f32` magnitude; 16: `f32` epsilon; 20: `f32` maximum radius |
| 15 | `randomaccel` | 68 | 0: R acceleration region |
| 16 | `randomdisplace` | 68 | 0: R displacement region |
| 17 | `randomvelocity` | 68 | 0: R velocity region |
| 19 | `sink` | 72 | 0: `u32` kill-inside switch; 4: R region |
| 21 | `source` | 372 | 0: R position region; 68: R velocity region; 136: R rotation region; 204: R size region; 272: R colour region; 340: `f32` alpha; 344: `f32` rate (particles per second); 348: `f32` age; 352: `f32` age spread; 356: `float3` parent velocity; 368: `f32` parent motion |
| 22 | `speedlimit` | 8 | 0: `f32` minimum speed; 4: `f32` maximum speed |
| 23 | `targetcolor` | 20 | 0: `float3` colour; 12: `f32` alpha; 16: `f32` scale |
| 24 | `targetsize` | 24 | 0: `float3` size; 12: `float3` scale per axis |
| 25 | `targetrotate` | 16 | 0: `float3` rotation; 12: `f32` scale |
| 27 | `targetvelocity` | 16 | 0: `float3` velocity; 12: `f32` scale |
| 29 | `vortex` | 36 | 0: `float3` centre; 12: `float3` axis; 24: `f32` magnitude; 28: `f32` epsilon; 32: `f32` maximum radius |
| 30 | `turbulence` | 28 | 0: `f32` frequency; 4: `i32` octave count; 8: `f32` magnitude; 12: `f32` sampling offset; 16: `float3` drift |
| 31 | `scatter` | 24 | 0: `float3` centre; 12: `f32` magnitude; 16: `f32` epsilon; 20: `f32` maximum radius |

Notes on the stored values (`actions.py`):

- `source`: the parent velocity is (0, 0, 0) in all 1,087 records; the game replaces it at run time from the
  effect's velocity and the parent motion (`bin_roles.py`; P2). The age is non-zero in 15 records (values −1,
  0.1, 1) and the age spread in 1 (value 1). The rate ranges from 0.5 to 777,777,792; alpha from 0 to 1;
  parent motion from 0 to 2.01.
- `killold`: the kill-younger switch is 0 in 1,069 records and 1 in one; the age limits sum to 115,395.31.
- `sink`: the kill-inside switch is 1 in 55 records and 0 in 34.
- `turbulence`: octave counts 1 (22 records), 2 (24), 3 (2), 4 (10).
- `damping`: the bounds are squared speeds (e.g. 16 and 10,000; 0 and 10³²).
- `speedlimit`: the minimum speed can be negative (−1 in 13 of the 36 records, −2 in one).

### 4.4 Other type ids

The game accepts type ids 0 – 31 except 2; the id selects the layout with no range test, so ids above 31 and
id 2 are not valid (what happens then is **unestablished**; believed a crash) (`bin_actions.py`). The ids the
library does not use, with the record sizes after the header that the game reads for them
(`bin_actions.py`): 0 (80), 3 (4), 5 (32), 11 (12), 13 (36), 18 (4), 20 (72), 26 (16), 28 (16). Ids 26 and 28
make the same kind of record as 25 (`targetrotate`) and 27 (`targetvelocity`) respectively, read and run alike (`bin_actions.py`). The only
word the data gives for any of them is `explosion` for id 5 (section 4.5). Their fields and behaviour are not
specified; no effect uses them.

### 4.5 Chunk 36: editor description of the actions

Not read by the game (`bin_loaders.py`). Layout (`pxr.py`, `editor.py`):

| Field | Type |
|---|---|
| entry count *m* | `u32` |
| *m* entries, back to back: | |
| type id | `u32` |
| (always 0) | `u32` |
| action name | `stringZ` |
| editor flags | `u32` (values 0, 1, 2, 3) |
| parameter block | bytes, a fixed size per type id |

Parameter block sizes per type id: 1: 53, 4: 20, 5: 33, 6: 12, 7: 12, 8: 13, 9: 65, 10: 5, 12: 0, 14: 25,
15: 41, 16: 41, 17: 41, 19: 42, 21: 222, 22: 8, 23: 20, 24: 24, 25: 16, 27: 17, 29: 37, 30: 28, 31: 25. With
them every chunk 36 is consumed exactly; the 1,075 chunks hold 7,100 entries (`editor.py`). The contents of
the parameter blocks are **unestablished** (the game does not read them).

The action name, after removing trailing digits and underscores (e.g. `orbitpoint_1`, `targetsize00`), is one
word per type id, the names of section 4.1, and `explosion` for id 5 (`editor.py`).

Relation to chunk 4 (`editor.py`): in every one of the 1,075 effects, the entries whose editor flags have bit
0x1 are, in order, exactly the action records of chunk 4 by type. The other 171 + 8 entries (editor flags 0 or
2), in 124 effects, have no record in chunk 4; among them is the one entry of id 5 (`explosion`, in
`alexmx\fontan`, editor flags 2). Chunk 4 alone determines the effect's actions.

## 5. Regions

### 5.1 Layout (68 bytes)

| Offset | Type | Field |
|---|---|---|
| 0 | `u32` | kind (0 – 10) |
| 4 | `float3` | **p**₁ |
| 16 | `float3` | **p**₂ |
| 28 | `float3` | **u** |
| 40 | `float3` | **v** |
| 52 | `f32` | *r*₁ |
| 56 | `f32` | *r*₂ |
| 60 | `f32` | *r*₃ |
| 64 | `f32` | *r*₄ |

The derived values (relations below) are stored, not recomputed when the record is read (`bin_actions.py`: a
region is read as 68 bytes). Fields not listed for a kind are not part of its definition; in the data they hold
leftover values (earlier values of other kinds, zeros or arbitrary bytes; `regions.py`). Whether the game reads
them anywhere is P2's. Regions occur in `bounce`, `jet`,
`randomaccel`, `randomdisplace`, `randomvelocity`, `sink` (one each) and `source` (five) (`regions.py`).

### 5.2 Kinds

The kind names describe the shape the game draws points from (`bin_regions.py`); how points are generated,
tested for containment and transformed is P2's. The relations hold for every region of the library (to a
relative 10⁻⁴; `regions.py`); a normal is left as stored when its squared length is at most 1.1754944 × 10⁻³⁸
(it is then (0, 0, 0)).

| Kind | Name | Regions | Meaningful fields and stored relations |
|---|---|---|---|
| 0 | point | 2,608 | **p**₁ the point |
| 1 | line | 1,722 | **p**₁ start; **p**₂ = end − start |
| 2 | triangle | 13 | **p**₁ first corner; **u**, **v** the two edges from it; **p**₂ = unit(**u** × **v**); *r*₁ = −**p**₁ · **p**₂; *r*₃ = \|**u**\|; *r*₄ = \|**v**\| |
| 3 | plane | 67 | **p**₁ a point of the plane; **p**₂ unit normal (or zero: 15 regions); *r*₁ = −**p**₁ · **p**₂ |
| 4 | box | 75 | **p**₁ the minimum corner, **p**₂ the maximum corner (each component of **p**₁ ≤ that of **p**₂) |
| 5 | sphere | 489 | **p**₁ centre; *r*₁ outer radius ≥ *r*₂ inner radius; *r*₃ = *r*₁²; *r*₄ = *r*₂² |
| 6 | cylinder | 48 | **p**₁ centre of one end; **p**₂ axis (other end − **p**₁); *r*₁ outer radius ≥ *r*₂ inner radius; *r*₃ = *r*₁²; *r*₄ = 1 / \|**p**₂\|² (0 when **p**₂ is zero); **u**, **v** the basis below |
| 7 | cone | 349 | as kind 6, with **p**₁ the apex and **p**₂ the axis to the centre of the base |
| 8 | Gaussian cloud | 21 | **p**₁ centre; *r*₁ = σ (standard deviation, > 0); *r*₂ = 0.39894229 / σ (that is 1 / (σ√(2π))); *r*₄ = −0.5 / σ² |
| 9 | disc | 217 | **p**₁ centre; **p**₂ unit normal (or zero: 5 regions); *r*₁ outer radius ≥ *r*₂ inner radius; *r*₃ = −**p**₁ · **p**₂; **u**, **v** the basis below, with **n** = **p**₂ |
| 10 | rectangle | 1 | **p**₁ corner; **u**, **v** the two edges from it; **p**₂ = unit(**u** × **v**); *r*₁ = −**p**₁ · **p**₂; *r*₃ = \|**u**\|; *r*₄ = \|**v**\| |

Basis of kinds 6, 7 and 9, with **n** the unit axis (kind 9: **n** = **p**₂): **e** = (0, 1, 0) if \|*n*ₓ\| >
0.999, else (1, 0, 0); **u** = unit(**e** − (**e** · **n**) **n**); **v** = **n** × **u** (`regions.py`;
the threshold is the program's constant, `bin_regions.py`). unit(**a**) is **a** / \|**a**\|, or **a** itself
when \|**a**\|² ≤ 1.1754944 × 10⁻³⁸. Radii may be negative: in 10 regions (kinds 5, 6, 7) *r*₂ < 0, and in 9
of them also *r*₁ < 0 (e.g. *r*₁ = *r*₂ = −3) (`regions.py`). The game generates (0, 0, 0) from a kind
above 10 (`bin_regions.py`).

Kinds by place (`regions.py`):

| Place | Kinds (regions) |
|---|---|
| `source` position | 0 (335), 1 (141), 3 (15), 4 (27), 5 (334), 6 (33), 7 (18), 8 (3), 9 (181) |
| `source` velocity | 0 (488), 1 (125), 4 (24), 5 (68), 6 (9), 7 (325), 8 (18), 9 (30) |
| `source` rotation | 0 (286), 1 (799), 5 (2) |
| `source` size | 0 (523), 1 (549), 2 (1), 4 (14) |
| `source` colour | 0 (975), 1 (99), 2 (11), 5 (2) |
| `bounce` | 2 (1), 3 (1), 5 (6), 9 (6), 10 (1) |
| `jet` | 0 (1) |
| `randomaccel` | 1 (6), 5 (29) |
| `randomdisplace` | 1 (1), 5 (32) |
| `randomvelocity` | 1 (2) |
| `sink` | 3 (51), 4 (10), 5 (16), 6 (6), 7 (6) |

## 6. Group record

The payload of a chunk-4 sub-chunk is a chunk sequence. Every group has chunks 1 – 6, stored in that order
(`groups.py`):

| id | Size | Content |
|---|---|---|
| 1 | 2 | `u16` group version = 3 |
| 2 | n | `stringZ` group name; the chunk holds exactly the name and its terminator |
| 3 | 4 | `u32` group flag word; 0 in all 252 groups; not read by the group's update (`bin_flags.py`); meaning **unestablished** |
| 4 | n | members (section 6.1) |
| 5 | 4 | `f32` group time limit (seconds); 0 means none (89 groups); the game stops the group when its time exceeds a limit greater than 0 (`bin_flags.py`; P6); the 252 values sum to 5,088.15, the largest is 900 |
| 6 | n | editor stamp (section 2.5); not read by the game |

How the game reads a group record (`bin_loaders.py`): chunk 1 must be present (otherwise an assertion
failure); if its version is not 3 the game logs `!Unsupported PG version. Load failed.` and the record fails
to load. Chunk 2 must be present (otherwise an assertion failure). Chunk 3 is copied into the flag word when
present. Members are read from chunk 4; if chunk 4 is absent they are read from chunk 7, whose members have
only the first two strings below (no shipped group has chunk 7; if neither exists, an assertion failure).
Chunk 5 is read when present. Chunk 6 is not read.

### 6.1 Members (chunk 4)

`u32` member count, then per member (`pxr.py`, `groups.py`; order of fields: `bin_loaders.py`):

| Field | Type |
|---|---|
| effect name | `stringZ` |
| per-particle child effect name | `stringZ` (empty: none) |
| birth child effect name | `stringZ` (empty: none) |
| death child effect name | `stringZ` (empty: none) |
| start time (seconds) | `f32` |
| stop time (seconds) | `f32` |
| member flag word | `u32` |

The chunk is consumed exactly by the members in every group. The 252 groups have 1,047 members (0 in 3 groups;
most often 2, 3, 5 or 6) (`groups.py`). Start times range over 0 … 10, stop times over 0 … 900; the stop time
is 0 in 926 members and never between 0 and the start time (`groups.py`).

Names (`groups.py`): no member slot names a group. Every effect name slot is non-empty; 20 members, all with
member flag 0x4, name an effect that is not in the library, 5 in each of the groups
`explosions\old\explosion-02`, `-021`, `-031` and `-042` (e.g. `explosions\expl_04_flame`); one death child (`static\buble_distort01`, in `static\bubble`) is not in
the library either. What the game does with a name it does not find is P6's.

### 6.2 Member flag word

The game tests exactly the bits 0x2, 0x4, 0x10, 0x20 and 0x40 of the member flag word (`bin_flags.py`); what
each does over time is P6's:

| Bit | Members | Meaning |
|---|---|---|
| 0x1 | 46 | not tested; no effect (believed: an editor setting) |
| 0x2 | 39 | each live particle of the member's effect carries a per-particle child effect (the second name): created at the particle's birth, removed at its death (believed from those steps) |
| 0x4 | 993 | the member is used: only members with this bit are played by the group |
| 0x10 | 3 | with 0x2: a per-particle child that is no longer playing is played again (believed: the call is the play request) |
| 0x20 | 33 | each new particle of the member's effect starts a birth child (the third name); a looped effect may not be one (the game reports `Can't use looped effect '%s' as 'On Birth' child for group.`) |
| 0x40 | 18 | each particle that dies starts a death child (the fourth name), by the same step that starts birth children |
| 0x2000, 0x4000, 0x8000, 0x10000, 0x20000, 0x200000 | 1 each | not tested; all six only in the one member of `destroy_fx\destroy_small_box_00` (value 0x23E004, stop time 4.48 × 10⁻⁴⁴) |

The 17 values (`groups.py`): 0x0 (48 members), 0x4 (878), 0x5 (44), 0x6 (26), 0x7 (2), 0x14 (1), 0x16 (1),
0x20 (5), 0x24 (18), 0x26 (4), 0x36 (1), 0x40 (1), 0x44 (10), 0x46 (2), 0x64 (2), 0x66 (3), 0x23E004 (1).
Child names and bits (`groups.py`): 71 members name a per-particle child (38 of them with 0x2), 53 a birth
child (33 with 0x20), 30 a death child (18 with 0x40); one member (`anomaly2\pux_blast00`, value 0x6) has 0x2
with an empty per-particle child name. A child name on a member without its bit is not used (P6).

## 7. Load failures (summary)

From sections 1, 2 and 6 (`bin_loaders.py`): an effect record fails to load when its version is not 1, a group
record when its version is not 3; a failed record is discarded and ends its list. A missing chunk the reader
requires (library chunk 1; effect chunks 1, 2, 3, 4 and the flag-gated chunks 7, 6, 8, 33, 34; group chunks 1,
2, and 4 or 7) is an assertion failure. No shipped record fails (`container.py`, `effect_chunks.py`,
`groups.py`).

## 8. Verification

On `particles.xr` (either copy):

- Counts: 1,075 effects, 252 groups, 6,921 action records, 1,047 members; action counts per type (4.1), region
  counts per kind and place (5.2), chunk counts (2), flag-bit counts (3), member flag values (6.2).
- Total bytes of the 1,075 chunk-4 payloads: 582,240; their MD5, concatenated in stored order:
  `86c509f3d97270a1850655a0543916c1` (`vectors.py`).
- Sums: maximum particle counts 57,143; chunk-8 time limits 10,366.5 over 623 effects; `killold` age limits
  115,395.31; group time limits 5,088.15 (`effect_chunks.py`, `effect_values.py`, `actions.py`,
  `vectors.py`).
- Every effect's chunk 4 and chunk 36, and every group's chunk 4, consumed exactly; every region relation of
  5.2; the chunk-36 entries with editor bit 0x1 equal chunk 4's types in all effects.

Test vector: effect `tests\muhi` (sub-chunk id 934; named by the first level's `level.ps_static`, scoping spec 1.2): maximum
particles 7; flag word 0x1401; sprite `particles\blend`, `pfx\pfx_leaves_01.tga`; frames: width 0.5, height
0.5, offsets 8 and 12 hold 64 and 64, 2 per row, 4 frames, rate 24; no chunk 8; chunk 4 is 648 bytes, chunk 36
548 bytes. Its action records, in order (`vectors.py`):

| # | Type | Flag word | Values |
|---|---|---|---|
| 1 | `source` (21) | 0x80000002 | position: disc (9), **p**₁ (0, 0, 0), **p**₂ (0, 1, 0), **u** (1, 0, 0), **v** (0, 0, −1), *r*₁ 1, *r*₂ 0, *r*₃ 0; velocity: cone (7), **p**₁ (0, 0, 0), **p**₂ (0, 0.1, 0), **u** (1, 0, 0), **v** (0, 0, −1), *r*₁ 0.3, *r*₂ 0, *r*₃ 0.09, *r*₄ 99.999992; rotation: line (1), **p**₁ (−137, −1, −1), **p**₂ (274, 2, 2); size: point (0), **p**₁ (0.01, 0.01, 0); colour: point (0), **p**₁ (0.5, 0.5, 0.5); alpha 1; rate 17; age 0; age spread 0; parent velocity (0, 0, 0); parent motion 0 |
| 2 | `targetrotate` (25) | 0x0 | rotation (−11.092285, 0, 0); scale 0.03 |
| 3 | `orbitpoint` (14) | 0x2 | centre (0, 0, 0); magnitude 1; epsilon 0.001; maximum radius 1.0000000 × 10¹⁶ |
| 4 | `orbitpoint` (14) | 0x2 | centre (0.3, 0, 0.3); magnitude 1; epsilon 0.001; maximum radius 1.0000000 × 10¹⁶ |
| 5 | `randomdisplace` (16) | 0x2 | sphere (5), **p**₁ (0, 0, 0), *r*₁ 0.3, *r*₂ 0, *r*₃ 0.09, *r*₄ 0 |
| 6 | `vortex` (29) | 0x2 | centre (0, 3, 0); axis (0, 1, 0); magnitude 70; epsilon 50; maximum radius 170 |
| 7 | `killold` (10) | 0x0 | age limit 2; kill-younger 0 |
| 8 | `move` (12) | 0x0 | — |

Its chunk 36 has 8 entries, all with editor flags 1: `source`, `targetrotate`, `orbitpoint`, `orbitpoint`,
`randomdisplace`, `vortex`, `killold`, `move` (`vectors.py`).

Test vector: group `explosions\campfire_05` (sub-chunk id 92; the fires of the first level, scoping spec 1.2): flags 0, time
limit 0, 7 members, all child names empty (effect, start, stop, flags): `explosions\group_items\campfire_05_flame00`,
0, 0, 0x4; `explosions\group_items\campfire_03_smoke`, 0, 0, 0x0; `explosions\group_items\campfire_02_sparks`,
0.5, 0, 0x4; `static\fire_distort`, 0, 0, 0x4; `explosions\group_items\campfire_05_flame01`, 0, 0, 0x4;
`explosions\group_items\campfire_05_vacuum`, 0, 0, 0x4; `explosions\group_items\campfire_glow_05`, 0, 0, 0x4
(`vectors.py`).

## 9. Unestablished

- What happens with an action type id of 2 or above 31 (4.4); the fields and behaviour of the unused ids 0, 3,
  5, 11, 13, 18, 20 (4.4).
- The contents of the chunk-36 parameter blocks and the meaning of the chunk-36 editor flag bit 0x2 (4.5).
- Which geometry the effect flag bit 0x80000 excludes from the collision test (3).
- The meaning of the group flag word (6).
- The roles of the values at chunk-6 offsets 8 and 12, and of chunk 34's and chunk 37's z (believed unused; 2.1,
  2.4).

## 10. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives `gamedata.db0` …
  `gamedata.dbd` and `mods\mp_military_2.xdb0` (listings), `particles.xr` from `gamedata.db4` and
  `gamedata.dba`; binaries `bin\xrRender_R1.dll` (MD5 `2bf95c8376b735daa6d82b8944894be0`) and
  `bin\xrParticles.dll` (MD5 `22bad4d258e6211c6ea7cc8dc8d7ca55`).
- Data examined: every chunk of every effect and group record of `particles.xr`, decoded by a throwaway parser
  written from this text (`checks/particles-xr/pxr.py`), which consumes every action list, every chunk 36 and
  every member list exactly.
- Analysis: static analysis (Ghidra 12.2, headless) of how the first renderer reads the library, effect and
  group records and which bits of the effect and member flag words it tests, and of how the particle library
  reads action records (the record kind made for each id and the size read for each), which action flag bits it tests,
  which stored fields each action's step and placement read, and how it generates points from each region kind.
  No execution or debugging.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/particles-xr/` reproduce every fact.

## Gaps found in review

- Which fields of each action type are positions and which are directions under action flag bit 0x2 (4.2) is not stated per type; P2's placement must establish it.
- Bit 0x1 of the action flag word is "not tested" by a search for flag tests in the particle library only; whether any other part of the game reads it is not shown.
