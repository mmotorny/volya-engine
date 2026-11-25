# `level.details` (detail objects of a level)

Functional specification of the file `levels\<level>\level.details` of S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): the small models scattered over the ground (grass, weeds, flat litter), and the grid of
2 × 2 m cells ("slots") that says which models grow where, at what height and how densely. It covers the
whole file: header, models, slot grid and every bit of a slot record. How slots are turned into placed
instances and drawn (sampling positions, random scale and rotation, `r__detail_density`, distances, sway)
belongs to the `detail-objects` behaviour specification; it is described here only as far as it fixes the
meaning of a stored value.

Conventions: integers are little-endian, unsigned unless stated otherwise; reals are IEEE 754 single
precision (`f32`). Sizes are in bytes. Positions are in metres in the level's world frame, whose vertical
axis is y. Strings are zero-terminated, Windows-1251 (all shipped strings are ASCII). "Bits a–b" of a 32-bit
word count from bit 0, the least significant. "Assertion failure" means the game stops with an error, as in
CH. Abbreviations of approved specifications: CH `docs/formats/chunks.md`, LV `docs/formats/level.md`,
LL `docs/behavior/level-loading.md`, CF `docs/formats/level-cform.md`, SS `docs/behavior/static-shading.md`.

**Checks.** Every claim carries a tag [LD-*n*]. The checks are one script in the dirty workspace,
`checks/level-details/run.py`, after `extract.py` (all 24 files from the archives with the project's archive
tool) and `extract_cform.py` (the `level.cform` headers). It reads the extracted files, the archives'
listings, the shipped binaries' bytes (constants, strings) and saved read-only static-analysis output of
the same binaries. All 68 checks pass.

## 1. Where the file is and who reads it

- The file is `level.details` in the level directory. 24 copies exist in the edition, one per level: 23 in
  `gamedata.db0` … `gamedata.db4` and `levels\mp_military_2\level.details` in `mods\mp_military_2.xdb0`.
  Of the 31 level directories (those with a `level.cform`), 7 have none: `l03u_agr_underground`,
  `l04u_labx18`, `l08u_brainlab`, `l10u_bunker`, `l12u_control_monolith`, `l12u_sarcofag`,
  `testers_mp_pool` [LD-1].
- Sizes range from 85,796 bytes (`testers_mp_workshop`) to 10,474,756 (`l12_stancia`, `l12_stancia_2`);
  `l01_escape` has 8,392,388 [LD-1].
- Only the two renderers name the file; no other shipped binary does [LD-17]. The first renderer reads it in
  the `st_loading_details` stage (LL 2.4).
- The file is optional: if it does not exist the level simply has no detail objects [LD-4].
- Every level that has the file also has `levels\<level>\build_details.dds` (349,672 bytes), the texture
  its models name (section 3); no level without the file has one [LD-16].

## 2. Top-level structure

The file is a chunk sequence (CH). Three chunks are used, looked up by id (CH 3):

| Chunk id | Size | Content |
|---|---|---|
| 0 | 24 | header (section 2.1) |
| 1 | varies | models: a chunk sequence, one sub-chunk per model (section 3) |
| 2 | 16 × size_x × size_z | slot grid (section 4) |

In every shipped file the chunks are stored in the order 1, 2, 0, there are no others, none is
compressed, and they fill the file exactly [LD-2].

### 2.1 Header (chunk 0)

Read as a fixed 24-byte record (a different size is an assertion failure, CH 3) [LD-4]:

| Offset | Size | Field | Meaning |
|---|---|---|---|
| 0 | 4 | version | must be 3; any other value is an assertion failure [LD-4]. 3 in every file [LD-3] |
| 4 | 4 | model count | number of models in chunk 1 [LD-4] |
| 8 | 4 | offset_x | signed; grid column of world cell x = 0 (section 4.1) |
| 12 | 4 | offset_z | signed; grid row of world cell z = 0 |
| 16 | 4 | size_x | number of grid columns (along world x) |
| 20 | 4 | size_z | number of grid rows (along world z) |

Shipped values: version 3 everywhere; model count 21 in 21 files and 20 in 3 (`mp_military_2`,
`testers_mp_atp`, `testers_mp_factory`); offsets are negative in some files (offset_x in `testers_mp_atp` and
`testers_mp_darkvalley`, offset_z in `mp_military_2` and `testers_mp_military_2`), so they must be read as
signed; grids range from 66 × 65 (`testers_mp_workshop`) to 817 × 800 (`l12_stancia`) [LD-3].

The game does not compare chunk 2's size with size_x × size_z; it uses chunk 2's data in place
[LD-4]. In every shipped file the size is exactly 16 × size_x × size_z [LD-3]. An implementation should
treat a shorter chunk 2 as a malformed file.

## 3. Models (chunk 1)

Chunk 1 is a chunk sequence. For *i* = 0 … model count − 1 the game opens the sub-chunk with id *i* (lookup
by id, CH 3) and reads model *i* from it [LD-4]. In every shipped file chunk 1 holds exactly the sub-chunks
0, 1, … count − 1 in that order and nothing else [LD-5].

The game keeps the models in a table of 64 entries [LD-4]; slot records name models by a 6-bit id in which
63 means "none" (section 4.2), so ids 0–62 are usable. Files with more than 63 models are not supported;
what the game does with a count above 64 is **unestablished**.

### 3.1 Model record

Each model sub-chunk holds, back to back [LD-6]:

| Field | Size | Meaning |
|---|---|---|
| shader | string | name of an engine shader definition, as the `<engine shader>` part of an LV 3 entry |
| texture | string | texture list passed to that shader, as the `<texture list>` part of an LV 3 entry |
| flags | 4 | bit 0: the model never sways (section 3.2); other bits unused |
| min scale | 4 (`f32`) | smaller end of the model's size range (use: `detail-objects`) |
| max scale | 4 (`f32`) | larger end of the size range |
| vertex count | 4 | *V* |
| index count | 4 | *I*; must be a multiple of 3, otherwise an assertion failure |
| vertices | 20 × *V* | per vertex five `f32`: position x, y, z (model space, metres), texture u, v |
| indices | 2 × *I* | `u16` vertex indices; every three form one triangle (an indexed triangle list) |

The game reads each string into a 256-byte buffer, so a string longer than 255 characters is not supported
[LD-6]. The shader is created from the pair (shader, texture) [LD-6]; the texture name resolves to a file
as in LV 3 (level directory first), which gives `levels\<level>\build_details.dds` for the shipped name
[LD-16]. The record fills its sub-chunk exactly in every shipped file [LD-6]. Vertex positions and
texture coordinates are copied as stored; the drawing transforms the position and keeps u, v [LD-6].

Shipped values [LD-7]:

- every model has shader `details\blend` (a definition in `shaders.xr`) and texture `build_details`;
- flags are 0 or 1 only; the flags-1 models are flat (|y| < 0.03 m for every vertex), every flags-0 model
  rises above y = 0.3 m;
- 0 < min scale ≤ max scale;
- every index is < *V*, every vertex is used by some index;
- texture coordinates lie within [−0.01, 1.01].

### 3.2 Flags bit 0 (first renderer)

When the first renderer places an instance of a model whose flags bit 0 is set, the instance uses the
still (non-swaying) variant of the model's shader; when the bit is clear, each instance randomly gets one
of two swaying variants. On hardware without vertex programs every instance is still [LD-8]. The sway
itself is for `detail-objects`.

## 4. Slot grid (chunk 2)

Chunk 2 is size_x × size_z records of 16 bytes, row by row: record index = size_x × *iz* + *ix* for column
*ix* (0 ≤ *ix* < size_x) and row *iz* (0 ≤ *iz* < size_z) [LD-10].

### 4.1 Placement of a slot in the world

Record (*ix*, *iz*) is the slot of world cell (*cx*, *cz*) = (*ix* − offset_x, *iz* − offset_z). That slot
covers world x from 2·*cx* to 2·*cx* + 2 and world z from 2·*cz* to 2·*cz* + 2 [LD-10]. A cell outside the
grid behaves as a slot whose four model ids are all 63, i.e. it has no detail objects [LD-10].

Evidence: in `l01_escape` the grid spans x −680 … 768 and z −634 … 812 and encloses the collision box of
`level.cform` (CF; x −678.90 … 766.57, z −633.79 … 811.67) within one slot on every side; with x and z
exchanged it would not [LD-11]. (In 9 of the 24 files the grid encloses the collision box this tightly; in
the others the two differ [LD-11].)

### 4.2 Slot record

Four 32-bit words w0, w1, w2, w3 (bytes 0–3, 4–7, 8–11, 12–15). A slot holds up to four **parts**, each a
model id with its density values.

| Word | Bits | Field | Meaning |
|---|---|---|---|
| w0 | 0–11 | base | lowest y of the slot = base × 0.2 − 200 m [LD-9] |
| w0 | 12–19 | height | top y of the slot = lowest y + height × 0.1 m [LD-9] |
| w0 | 20–25 | id of part 0 | model id; 63 = no part [LD-9] |
| w0 | 26–31 | id of part 1 | |
| w1 | 0–5 | id of part 2 | |
| w1 | 6–11 | id of part 3 | |
| w1 | 12–15 | sun factor | 0–15, used as value ÷ 15 (section 4.4) [LD-13] |
| w1 | 16–19 | hemisphere factor | 0–15, used as value ÷ 15 (section 4.4) [LD-13] |
| w1 | 20–23, 24–27, 28–31 | three further 4-bit values | read but not used for drawing (section 4.4) |
| w2 | 0–15 | densities of part 0 | four 4-bit values (section 4.3) [LD-12] |
| w2 | 16–31 | densities of part 1 | |
| w3 | 0–15 | densities of part 2 | |
| w3 | 16–31 | densities of part 3 | |

The scale factors are the single-precision values 0.2, 200, 0.1 and 1/15 [LD-9].

- A part whose id is 63 places nothing; a slot whose four ids are all 63 is treated as empty and never
  built [LD-9].
- The same model id may appear in two parts of one slot (it does in the shipped data); each part is
  handled on its own [LD-14].
- A part with id 63 may still have non-zero densities in the data; they are ignored [LD-9, LD-14].
- An id that is neither 63 nor below the model count has **unestablished** behaviour; no shipped slot has
  one [LD-14].

### 4.3 Densities

The 16-bit density word of a part holds four values d0 (bits 0–3), d1 (bits 4–7), d2 (bits 8–11) and d3
(bits 12–15) [LD-12]. They are the part's density at the four corners of the slot:

| Value | Corner |
|---|---|
| d0 | smallest x, smallest z |
| d1 | largest x, smallest z |
| d2 | smallest x, largest z |
| d3 | largest x, largest z |

At a point of the slot at fraction *a* of its width along x and *b* along z (0 ≤ *a*, *b* ≤ 1, measured from
the smallest-x, smallest-z corner) the density is the bilinear blend
d0·(1−*a*)(1−*b*) + d1·*a*(1−*b*) + d2·(1−*a*)*b* + d3·*a**b*, and the game scales it by 17, so 15
corresponds to 255 [LD-12]. Which points are sampled and how the density decides whether an instance is
placed there are for `detail-objects`.

Evidence from the data: in `l02_garbage`, for neighbouring slots that contain the same model, the values on
the shared edge agree (d1 of a slot with d0 of the next slot in x, d3 with d2; d2 with d0 of the next slot
in z, d3 with d1) in 49 % and 48 % of the comparisons, against 38 % for the reversed pairing [LD-12].

### 4.4 Light factors

When the first renderer draws an instance placed from a slot, it gives the instance the colour
`ambient` + `hemi_color`.rgb × (hemisphere factor ÷ 15) + `sun_color` × 0.5 × (sun factor ÷ 15), where
`ambient`, `hemi_color` and `sun_color` are the current weather values (SS 9) [LD-13]. **Disputed in review:**
this holds only on hardware with vertex programs. Without vertex programs the first renderer draws
through a different path that writes the same constant vertex colour (all 32 bits set) for every vertex
of every instance, so the sun and hemisphere factors have no effect there. (The renderer reads
the three weather values in the same relative layout in which the engine stores the weather section's
`ambient`, `hemi_color` and `sun_color`; the names are established from that layout [LD-13].)

The first renderer copies the three further 4-bit values of w1 bits 20–31 (each ÷ 15) into each placed
instance but never uses them; the second renderer does not decode them at all [LD-13]. Their meaning is
**unestablished**. They are 0 in all but about 0.03 % of their occurrences in the shipped slots, but not 0 everywhere in the edition [LD-14].

### 4.5 Shipped values

- No slot has all four ids 63 [LD-14].
- A base of 0 (lowest y −200 m) always comes with a height of 0; a height of 0 with a non-zero base occurs
  in exactly 2 slots (one each in `testers_mp_military_1` and `testers_mp_railroad`) [LD-14]. Slots with
  base 0 lie where there is no ground (**believed**: in `l01_escape` all other slots lie within the
  collision box's height range, section 5).
- The sun factor takes the values 0–8, the hemisphere factor 0–13 [LD-14].

## 5. Test vectors (`levels\l01_escape\level.details`)

From `gamedata.db0` of the Russian GOG edition [LD-1, LD-3, LD-15]:

- 8,392,388 bytes; chunks in stored order 1 (17,108 bytes), 2 (8,375,232), 0 (24).
- MD5 of the chunk payloads: chunk 0 `d42cb184083b56d557086985c2d7053c`, chunk 1
  `48046c9c7488fc33157a94d9bad14584`, chunk 2 `3cee4ba6cda61198aac6b8d4bf598c43`.
- Header: version 3, 21 models, offset_x 340, offset_z 317, size_x 724, size_z 723 (523,452 slots).
- Models (id: flags, vertices, indices, min scale, max scale; sub-chunk size):

| Id | Flags | *V* | *I* | Min | Max | Size |
|---|---|---|---|---|---|---|
| 0 | 0 | 52 | 102 | 1.0 | 1.5 | 1,292 |
| 1 | 0 | 36 | 66 | 1.5 | 2.0 | 900 |
| 2 | 0 | 36 | 66 | 0.75 | 1.1 | 900 |
| 3 | 0 | 52 | 102 | 0.75 | 2.0 | 1,292 |
| 4 | 0 | 52 | 102 | 1.5 | 2.0 | 1,292 |
| 5 | 0 | 52 | 102 | 1.0 | 1.4 | 1,292 |
| 6 | 0 | 52 | 102 | 1.3 | 2.0 | 1,292 |
| 7 | 0 | 52 | 102 | 1.0 | 2.0 | 1,292 |
| 8 | 0 | 36 | 66 | 1.0 | 1.3 | 900 |
| 9 | 0 | 52 | 102 | 1.25 | 2.0 | 1,292 |
| 10 | 0 | 52 | 102 | 1.1 | 1.5 | 1,292 |
| 11 | 0 | 52 | 102 | 1.5 | 2.0 | 1,292 |
| 12 | 0 | 52 | 102 | 1.5 | 2.0 | 1,292 |
| 13–16 | 1 | 4 | 6 | 0.8 | 1.3 | 140 |
| 17 | 1 | 8 | 18 | 0.8 | 1.3 | 244 |
| 18–20 | 1 | 5 | 12 | 0.8 | 1.3 | 172 |

  (scales rounded to 4 decimals; in all, 667 vertices and 1,296 indices).
- Slots by number of parts with id ≠ 63: 1 part 30, 2 parts 365,446, 3 parts 5,099, 4 parts 152,877.
- 110,479 slots have at least one part with id ≠ 63 and a non-zero density word; 262,894 slots have base 0.
- Sums over all slots: base 281,470,427; height 1,890,395; sun factor 1,355,876; hemisphere factor
  2,341,486; all 16 density values of all slots 1,591,645.
- Over the slots with base ≠ 0 the lowest y is −37.6 m and the highest top is 83.3 m, inside the collision
  box height −38.67 … 113.88 m [LD-11].

## 6. Not established

- The meaning of w1 bits 20–31 (section 4.4).
- What the game does with a model count above 64, a missing model sub-chunk, a slot id that is neither 63
  nor below the model count, or a chunk 2 shorter than the grid.
- Whether base 0 is meant as "no ground" (section 4.5).

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archives `gamedata.db0` … `gamedata.dbd` and `mods\mp_military_2.xdb0`, read with the project's
  archive tool.
- Data analysis: all 24 `level.details` files decoded completely (header, every model, every slot); the
  headers of the 31 `level.cform` files; the archive listings for `build_details.dds`; `shaders.xr`.
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only) of `xrRender_R1.dll` (loading of
  the file, the model reader, slot lookup, slot building, the two drawing paths), `xrRender_R2.dll` (slot
  building, for which fields it decodes) and `XR_3DA.exe` (reading of the weather section's colour values);
  constants and strings read directly from the files. No execution or debugging.
- A throwaway parser written from this text decoded every file and reproduced section 5 and the
  invariants.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What the game does when a file exists but lacks chunk 0, 1 or 2, when a model sub-chunk is longer or
  shorter than its record, or when an index is not below the model's vertex count, is not stated.
