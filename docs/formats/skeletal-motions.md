# Skeletal motions and the pose at a moment

Format-and-behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the
motions of animated skeleton models (visual type 3 of `docs/formats/skeleton-ogf.md` 1): where a
model's motions come from (its motion-file list, chunk 19, or its own chunks 14 and 15; the `.omf`
files), the motion parameters (chunk 15) and keys (chunk 14), how a motion's tracks are matched to the
model's bones (chunks 13 and 16), and the evaluation of one motion at a time *t* (*t* = 0 is the first
frame) into every bone's current model-space matrix *T*, which gives the render matrices of
model-shading 2. Playback over time (advancing *t*, speed, what happens at the end of a motion,
fading in and out), blending several motions on one bone, and which motion an object plays are not part
of it (section 7). What could not be established is marked **unestablished**.

Builds on (not repeated here):

- `docs/formats/skeleton-ogf.md`: visual types and chunk sets (1), skeleton models (3), bones,
  bind matrices *B* and their convention (4), child meshes and vertex formats (5, 6).
- `docs/behavior/model-shading.md`: skinning with render matrices *K* = *B*⁻¹ · *T* (2).
- `docs/behavior/object-visual-and-transform.md`: model-file lookup and case-insensitive paths (1).
- `docs/formats/ogf.md`: the visual header (chunk 1). `docs/formats/chunks.md`: chunks, nesting,
  lookup by id. `docs/formats/ltx-config.md` section 10: the path aliases `$level$` and
  `$game_meshes$`.

Conventions as in skeleton-ogf: little-endian; `u8`/`u16`/`u32`, `s8`/`s16`, `f32`; `float3` =
3 × `f32`; `stringZ` zero-terminated Windows-1251; matrices in the row-vector convention (a point **p**
is transformed as [**p** 1] · *M*; *M*[*r*][*c*], rows and columns from 0; row 3 is the translation).
floor(*x*) is the largest integer ≤ *x*; clamp(*x*, *a*, *b*) limits *x* to *a* … *b*; *a* mod *n* is
the remainder in 0 … *n* − 1; f32(*x*) is *x* rounded to single precision. Times are in seconds.

Bracketed ids ([M1] …) name the check that backs the statement next to them: the functions of
`checks.py` in the dirty workspace's `checks/skeletal-motions/` (data extracted by `extract.py`, binary
evidence exported by `export.py`, decoder `sm.py` written from this text).

## 1. Where a model's motions come from

### 1.1 Models with motions

Only type-3 models have motions. Of the 1,581 `.ogf` files of the archives, 330 have type 3: 206 have a
**motion-file list** (chunk 19) and 124 have **embedded motions** (chunks 14 and 15 of the model file
itself); none has both, and no file of another type has any of chunks 14, 15 or 19 [M1].

If chunk 19 is present, the model's motions are those of the listed files (section 1.2), and any
chunks 14 and 15 of the model file would not be read; otherwise the model file's own chunks 14 and 15
are its single motion set [M2c].

### 1.2 The motion-file list (chunk 19)

Chunk 19 holds one `stringZ` (in all 206 files exactly the chunk's bytes, ending with the single zero
byte) [M2]. It is split into items at commas and each item is trimmed of leading and trailing bytes
≤ 0x20 [M2b]. More than 3 items is an error [M2b]. In the data every list has 1, 2 or 3 items (80, 82
and 44 lists) and no item is empty or has surrounding spaces [M2].

For each item, `.omf` is appended and the file is the first that exists of `$level$` + item + `.omf`
(the current level's directory) and `$game_meshes$` + item + `.omf` (`meshes\`); if neither exists the
game stops with "Can't find motion file '%s'." [M2b]. The positions of the items in the list (0, 1, 2)
are the model's **motion files** 0, 1, 2. A motion file is decoded once per name and the result is
shared by every model that lists the same name [M2c].

In the data the lists name 26 distinct files, all found under `meshes\`; no `.omf` file exists under
`levels\` [M3].

### 1.3 Motion files (`.omf`)

A motion file is a chunk sequence (chunks.md) of exactly chunk 14 followed by chunk 15, with the same
layouts as a model's embedded chunks 14 and 15 (sections 2 and 3). The archives hold 31 distinct `.omf`
paths and one `meshes\actors\stalker_animation.omf-`; all 32 files have this form [M4]. Five `.omf`
files are listed by no model: `meshes\actors\exoskeleton_animation_.omf`,
`meshes\actors\monolit_animation.omf`, `meshes\actors\stalker_animation_old.omf`,
`meshes\actors\stalker_zombi\stalker_zombi.omf`, `meshes\monsters\_dog_animation.omf` [M3].

A **motion set** is one pair of chunks 14 and 15: one per motion file, or a model's embedded pair. There
are 156 in the data (32 files and 124 models) [M5].

## 2. Motion parameters (chunk 15)

Chunk 15 is required: a motion set without it is fatal ("Old skinned model version unsupported! (%s)")
[M5]. Layout, read sequentially:

| Field | Type | Meaning |
|---|---|---|
| version | `u16` | 3 in every set; a value above 4 is an error ("Invalid OGF/OMF version:") [M5] |
| partition count *P* | `u16` | |
| *P* partitions | | each: name `stringZ` (lower-cased when read), bone-entry count `u16`, then that many **bone entries**: bone name `stringZ` (used as stored), track index `u32` (only its low 16 bits are used) [M6b] |
| definition count *D* | `u16` | |
| *D* motion definitions | | each as below |

Motion definition:

| Field | Type | Meaning |
|---|---|---|
| name | `stringZ` | lower-cased when read [M7b] |
| flags | `u32` | only the low 16 bits are kept; bit 0 (value 1): the motion is an **fx** motion, otherwise a **cycle** [M7b]; bits 1 – 3: playback options (section 7) |
| bone or partition | `u16` | cycle: a partition index, or 0xFFFF for all partitions; fx: a bone index of the model, or 0xFFFF for the root bone (section 4.3) [M7, M17] |
| motion index | `u16` | equals the definition's position in every set [M7]; not needed |
| speed, power, accrue, falloff | 4 × `f32` | playback parameters (section 7) |
| marks | | only if version ≥ 4: `u32` mark-set count, then per mark set a name (a line, see below), `u32` *m*, *m* × (`f32`, `f32`) intervals [M8, M9] |

Definition *k* (0-based, in stored order) describes the motion stored in sub-chunk *k* + 1 of chunk 14
(section 3).

The program keeps each of speed, power, accrue and falloff as a 16-bit value *q* = clamp(floor(f32(*x* ·
655.35)), 0, 65535) of the stored *x*; for a cycle whose *q*(falloff) ≥ *q*(accrue), *q*(falloff) becomes
*q*(accrue) − 1 [M8, M8b].

A mark-set name is read as a text line: the reading position moves past the first byte, on to the next
CR (0x0D) or LF (0x0A) byte or the end of the chunk, and then past every following CR or LF byte [M9].
No shipped set has version 4 (section 8).

Data facts [M6, M7]: 1 partition in 130 sets, 2 in 3, 3 in 23. In every set the track indices of all
bone entries together are 0 … *n* − 1, each once (*n* = number of bone entries), and the bone names are
unique and lower-case. 9,232 definitions in all; names are lower-case and unique within each set. Flags
values: 0 (1,780 definitions), 1 (185), 2 (6,170), 3 (64), 4 (5), 6 (42), 8 (986). Cycles: 4,762 with
0xFFFF, 4,221 with a partition index below *P*; fx: 30 with 0xFFFF, 219 with an index below *n*.

## 3. Motion keys (chunk 14)

Chunk 14 is a sequence of sub-chunks with ids 0, 1, …, *D* in order [M10]:

- **Sub-chunk 0**: `u32` motion count (low 16 bits used); equals *D* in every set [M10].
- **Sub-chunk *k* + 1** (*k* = 0 … count − 1), found by id: the motion of definition *k* [M10b]:

| Field | Type | Meaning |
|---|---|---|
| name | `stringZ` | not used; equals definition *k*'s name in every set [M10b, M11] |
| key count *N* | `u32` | ≥ 1 in the data (18 motions have 1 key) [M12] |
| tracks | | one track per bone (section 4.1), in track-index order 0, 1, … |

Track [M11b]:

| Field | Type | Present |
|---|---|---|
| track flags | `u8` | always |
| rotation keys | `u32` checksum, then *N* × 8 bytes | if bit 1 (value 2) of the flags is clear |
| single rotation key | 8 bytes | if bit 1 is set (no checksum) |
| translation keys | `u32` checksum, then *N* × 3 bytes | if bit 0 (value 1) is set |
| translation size **s** | `float3` | if bit 0 is set |
| translation base **o** | `float3` | always |

- A **rotation key** is 4 × `s16` (*x*, *y*, *z*, *w*); each component decodes as *c* · f32(1/32767)
  (single precision, 3.0518509e−05) [M14, M14b]. A track with bit 1 set has the same rotation at every key.
- A **translation key** is 3 × `s8` (*a*₀, *a*₁, *a*₂); its translation is (*a*ᵢ · **s**ᵢ + **o**ᵢ). A
  track with bit 0 clear has translation **o** at every key.
- Each checksum is the CRC-32 (the common polynomial 0xEDB88320, as in zlib) of the key bytes that
  follow it [M12]; the game uses it only to share identical key arrays in memory [M12c]. A reader may ignore it.

Parsing every motion of every set with this layout consumes each sub-chunk exactly [M11]. Data facts
[M12, M13]: track flags 0 in 219,394 tracks, 1 in 20,994, 2 in 173,967, 3 in 1,501; 19,234,168 rotation
keys, 1,849,418 translation keys and 262,883 checksums in all. The length of every stored rotation key
(decoded) lies in 0.999941 … 1.0000000; translation sizes are 0 … 0.0653 per component.

## 4. Tracks, bones and motion names

### 4.1 Matching tracks to bones

Track *j* of every motion of a set belongs to the bone entry of chunk 15 whose track index is *j*, and
through its bone name to the model bone (skeleton-ogf 4, chunk 13) of that exact name; tracks are
stored and found by bone name, never by bone position [M2c]. Partition membership plays no part in the
matching.

When a motion set is decoded, one track is read per bone of the model that decodes it [M15b]; because
a motion file is decoded once and shared (section 1.2), it is decoded with the bone count of the first
model that loads it. In the data this makes no difference: in all 500 pairs of a type-3 model and one
of its motion sets, the set's bone names are exactly the model's bone names (and so the counts are
equal) [M15]. In 496 pairs the track indices also follow the model's bone order; in the other 4
(`meshes\actors\protect\protect_suit1.ogf`, `protect_suit2.ogf`, `meshes\monsters\rat\rat_2.ogf`,
`rat_3.ogf`) they do not, so matching must be by name [M15].

### 4.2 Motion names

A motion of a model is identified by its motion file *p* (section 1.2; 0 for embedded motions) and its
definition index *k*. A motion name refers to the motion of that name in the **last** of the model's
motion files that has a definition of that name; names are compared byte for byte with the stored
(lower-case) names [M16]. Example: `actors\soldier\soldier_antigas` lists files of 1,569, 128 and 18
motions; all 128 names of the second and 7 of the third also occur in the first, so those names refer
to the second and third files; the model has 1,580 distinct motion names [M16].

### 4.3 Partitions

The partitions of a model are those of its motion file 0, whatever file a cycle comes from [M17]. When
a cycle is played on partition *i* (0 … 3), it applies to the bones named by partition *i*'s bone
entries; with 0xFFFF it is played on each partition. An fx motion is applied starting at the bone of its
definition, the root bone for 0xFFFF [M17]. Which bones below that bone an fx motion affects, and
everything else about playing motions, is playback (section 7).

## 5. The pose at time *t*

This section gives, for one motion (*p*, *k*) with *N* keys and a time *t* ≥ 0, every bone's local
matrix *L* and current model-space matrix *T*. A bone on which this one motion is applied takes the
evaluated values unchanged (no weighting) [M19c].

### 5.1 Key position

The motion runs at 30 keys per second [M14]. With *u* = f32(*t* · 30), the frame number is
*F* = floor(*u*) [M14c, M8b] and the fraction *f* = f32(*u* − *F*). The two keys used are
*k*₀ = *F* mod *N* and *k*₁ = (*F* + 1) mod *N* [M18]. The motion's length is *N* · f32(1/30)
(0.033333335) seconds [M14]; between the last key and the length the evaluation interpolates from the
last key to key 0, and beyond the length it repeats (whether playback ever evaluates such times is
section 7).

### 5.2 Rotation

If the track has a single rotation key, the rotation **q** is that key, decoded [M18]. Otherwise, with
**a** and **b** the decoded keys *k*₀ and *k*₁ (as 4-vectors (*x*, *y*, *z*, *w*)) and
*g* = clamp(*f*, 0, 1) [M18]:

- *c* = **a** · **b** (sum of the four products); if *c* < 0, *c* is replaced by −*c* and σ = −1,
  otherwise σ = 1.
- If 1 − *c* ≤ ε with ε = f32(1e−5) (9.99999975e−06): *w*ₐ = 1 − *g*, *w*_b = *g*.
- Otherwise the angle is θ = *h* − *c* · (((*A* · *c*² − *B*) · *c*² + *C*) · *c*² + *D*₀), with the
  single-precision constants *h* = 1.57079637 (π/2), *A* = 2.83893299, *B* = 3.85373497,
  *C* = 1.69320405, *D*₀ = 0.892399013 [M18b], and *w*ₐ = sin(θ − θ · *g*) / sin θ,
  *w*_b = sin(θ · *g*) / sin θ.
- **q** = *w*ₐ · **a** + σ · *w*_b · **b**. **q** is **not** normalised before use [M19].

The polynomial approximates arccos *c*; it differs from it by up to 0.0805 rad (near *c* = 0.9853), and
the weights it gives differ from those of exact spherical interpolation by at most 0.0060 [M18c].

### 5.3 Translation

If the track has translation keys, the translation **p** = (1 − *f*) · **p**₀ + *f* · **p**₁ per
component, **p**₀ and **p**₁ the translations of keys *k*₀ and *k*₁ (section 3); *f* is not clamped
here. Otherwise **p** = **o** [M18].

### 5.4 Local and current matrices

The bone's local matrix *L*, from **q** = (*x*, *y*, *z*, *w*) and **p** = (*p*₀, *p*₁, *p*₂) [M19]:

| Row | Column 0 | Column 1 | Column 2 | Column 3 |
|---|---|---|---|---|
| 0 | 1 − 2(*y*² + *z*²) | 2(*xy* − *wz*) | 2(*xz* + *wy*) | 0 |
| 1 | 2(*xy* + *wz*) | 1 − 2(*x*² + *z*²) | 2(*yz* − *wx*) | 0 |
| 2 | 2(*xz* − *wy*) | 2(*yz* + *wx*) | 1 − 2(*x*² + *y*²) | 0 |
| 3 | *p*₀ | *p*₁ | *p*₂ | 1 |

The bone's current matrix is *T* = *L* · *T*(parent) (the bone's own matrix first, as for the bind
matrices of skeleton-ogf 4); for the root bone *T* = *L* [M19b]. The render matrix of model-shading
2 is then *K* = *B*⁻¹ · *T*, and the vertices are skinned as described there.

Data check of this convention: for every sampled motion of `actors\neytral\stalker_neytral_balon_1`
(41 motions of `stalker_animation`) and `actors\trader\trader` (65 motions), skinning the model's
meshes with *K* at *t* = 0 changes the lengths of triangle edges that join vertices of different bones
less with the matrix and order above than with the conjugate rotation or the opposite multiplication
order [M20].

### 5.5 The first frame

At *t* = 0, *F* = 0 and *f* = 0, so every track evaluates to its key 0: the decoded rotation key 0 and
the translation of key 0 (or **o**) [M21] (in the program's arithmetic the weight sin θ / sin θ may
differ from 1 by rounding).

The program computes a bone only while its visibility bit is set, and game code can attach a callback
to a bone that follows or replaces the motion's result [M19d]; neither is part of this specification.

## 6. Verification facts

Values are computed in double precision from the stored single-precision data; positions agree within
2e−4 and sums within 1e−3 with a single-precision implementation of section 5.

### 6.1 Motion files

Motions and bone entries per file [M28]:

| File | Motions | Bone entries |
|---|---|---|
| `meshes\actors\bandits_animation.omf` | 131 | 45 |
| `meshes\actors\barman_animation.omf` | 886 | 45 |
| `meshes\actors\critical_hit_grup_1.omf` … `_7.omf` | 4, 16, 17, 17, 18, 18, 4 | 45 |
| `meshes\actors\exoskeleton_animation.omf`, `exoskeleton_animation_.omf` | 28, 28 | 45 |
| `meshes\actors\monolit_animation.omf` | 6 | 45 |
| `meshes\actors\mp_stalker.omf` | 289 | 45 |
| `meshes\actors\soldiers_animation.omf` | 128 | 45 |
| `meshes\actors\stalker_animation.omf` | 1,569 | 45 |
| `meshes\actors\stalker_animation.omf-` | 1,533 | 45 |
| `meshes\actors\stalker_animation_old.omf` | 1,532 | 45 |
| `meshes\actors\stalker_drunk.omf` | 2 | 45 |
| `meshes\actors\stalker_zombi\stalker_zombi.omf` | 192 | 45 |
| `meshes\actors\trader_animation.omf` | 65 | 55 |
| `meshes\actors\ucheniy.omf` | 22 | 45 |
| `meshes\actors\zombie_animation.omf` | 254 | 45 |
| `meshes\actors\zombie_wounded.omf` | 4 | 45 |
| `meshes\monsters\_dog_animation.omf` | 54 | 29 |
| `meshes\monsters\bloodsucker_animations.omf` | 50 | 61 |
| `meshes\monsters\burer_animation.omf` | 40 | 53 |
| `meshes\monsters\dog_animation.omf` | 55 | 33 |
| `meshes\monsters\kontroller.omf` | 98 | 54 |
| `meshes\monsters\psevdodog_animation.omf` | 86 | 35 |
| `meshes\monsters\psevdogigant_animation.omf` | 53 | 59 |
| `meshes\monsters\rat_animation.omf` | 13 | 29 |
| `meshes\monsters\zombi_animation.omf` | 69 | 41 |

Over all 156 motion sets: 9,232 motions with 679,906 keys (sum of *N*) [M28b].

### 6.2 Models of `l01_escape`

The type-3 models named in object-visual-and-transform 5.1 and skeleton-ogf 7.2 and their motions [M27]:
`actors\trader\trader` lists `actors\trader_animation`; `actors\novice\green_stalker_2` and
`green_stalker_10` list `actors\stalker_animation`, `actors\critical_hit_grup_3`;
`actors\soldier\soldier_antigas` lists `actors\stalker_animation`, `actors\soldiers_animation`,
`actors\critical_hit_grup_6`; `actors\soldier\soldier_beret_1` the same with `critical_hit_grup_5`;
`actors\neytral\stalker_neytral_balon_8` lists `actors\stalker_animation`, `actors\critical_hit_grup_4`;
`physics\vehicles\mi24\veh_mi24_u_01` has embedded motions.

Positions below are row 3 of *T* (model space, before the object transform).

- **`actors\trader\trader`, motion `idle`** (`trader_animation`, definition 24): 100 keys, flags 2,
  bone or partition 0xFFFF, stored speed 1, power 1, accrue 2, falloff 2 [M22]. Track 0 is
  `bip01_pelvis` (flags 2, single rotation key (−1570, 1570, −23116, 23116), base (−0.00001, 0.544627,
  −0.109624)); track 12 is `bip01_head` (flags 0, rotation key 0 (1602, −22009, −793, 24208), base
  x = 0.058879) [M22]; the sums of the head's 100 stored rotation keys are (154883, −2185962, −87106,
  2434353) [M29].
  At *t* = 0: `bip01_pelvis` (0, 0.5446, −0.1096), `bip01_head` (−0.0311, 1.1592, 0.0958); the sum of
  all 55 bone positions (−3.9849, 49.9574, 11.7492); the box of all skinned vertices (*K* of section
  5.4, model-shading 2, without the quantization of model-shading 1) (−0.3665, −0.0007, −0.2796) …
  (0.2698, 1.3697, 0.5612). At *t* = 1.25: head (−0.0309, 1.1632, 0.0897), sum (−3.9936, 50.0255,
  11.6602), box (−0.3630, 0.0016, −0.2812) … (0.2696, 1.3740, 0.5612) [M23].
- **`physics\vehicles\mi24\veh_mi24_u_01`** (29 bones): motions `idle` (479 keys) and `die` (150).
  `idle`: bone `right_gun2` at *t* = 0 (2.1956, 1.0904, 0.0212), at *t* = 0.5 (2.1968, 1.0681, 0.0147);
  sums of the bone positions (−1.2610, 60.5935, −2.6751) and (−1.1031, 60.2494, −2.6607) [M24].
- **`actors\novice\green_stalker_2`, motion `norm_idle_1_`** (`stalker_animation`, 60 keys): at
  *t* = 0 `bip01_pelvis` (0, 0.8731, 0), `bip01_head` (0.0717, 1.5003, 0.1498), sum (4.4558, 54.3430,
  13.4239); at *t* = 0.5 (0, 0.8772, 0), (0.0719, 1.5071, 0.1432), sum (4.4651, 54.6129, 13.2453)
  [M25].
- **`monsters\crow\crow`** (16 bones, 12 embedded motions), motion `norm_fly_fwd`: bone `right_leg2`
  at *t* = 0 (0.0380, −0.0227, −0.0537), at *t* = 0.5 (0.0380, 0.0047, −0.0537); root at *t* = 0.5
  (0, 0.0274, 0); sums (−0.0054, 0.7885, −0.0979) and (0.0020, 0.7121, −0.1300) [M26].

### 6.3 Invariants over all model and motion files

- Type-3 models have chunk 19 or chunks 14 and 15, never both (section 1.1) [M1].
- Every motion set has version 3, 1 – 3 partitions, a permutation of track indices, chunk 14 with
  sub-chunks 0 … *D*, *D* motions each consumed exactly by the track layout, *N* ≥ 1, and correct
  checksums [M5, M6, M10, M11, M12].
- Every listed motion file resolves, and every motion set's bone names equal its model's [M3, M15].

## 7. Not covered

- Playback: how *t* advances, the speed and power parameters, the flags bits 1 – 3, the accrue and
  falloff fades, what happens at a motion's end, the marks, and which bones below its bone an fx
  motion drives.
- Blending: several motions on one bone at once (their weights and channels).
- Which motion an object shows in the first frame (scoping proposal LO-2; characters, monsters and
  crows choose motions through AI and scripts).
- Bone visibility and bone callbacks (section 5.5).

## 8. Unestablished

- The exact name value of a version-4 mark set (section 2): the line rule is established, but no
  shipped set has version 4, so the decoding could not be checked against data.
- The meaning of flags bits 2 and 3 (values 4 and 8) of a motion definition.
- The result for *t* < 0.
- What the game does with a motion set whose bone names differ from the model's (no shipped pair
  differs, section 4.1).

## 9. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (later archives win); binaries `bin\XR_3DA.exe`, `bin\xrCore.dll`.
- Data examined: every `.ogf` file (1,581) and every `.omf` file (31, and `stalker_animation.omf-`) of
  the archives; the archive listing.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation with
  Ghidra 12.2) for the reading of the motion-file list and the motion chunks, the motion-name lookup,
  the playing of a motion on partitions and bones (only as far as stated in 4.3), the key
  evaluation, the bone matrix and the bone hierarchy; constants read from the binary. No execution or
  debugging. A throwaway decoder written from this text parses every motion set and evaluates poses;
  the matrix convention was checked against the data by skinning (section 5.4).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The local matrix *L* of a bone on which the evaluated motion is not applied (a cycle played on one partition leaves the other partitions' bones without it; an fx motion starts at its bone) is not given, so "every bone's current matrix" is specified only for bones the motion applies to.
