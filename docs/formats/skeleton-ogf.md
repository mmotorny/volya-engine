# Skeleton models (`.ogf` model files)

Format specification of the standalone model files (`.ogf`) of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition) that the game's objects use as their visuals: the visual types of these files, skeleton models and
their chunks, bones (names, hierarchy, the bind pose), child meshes, the skinned vertex formats, and the
invariants that hold over every shipped file. How a visual name finds its file is in
`docs/behavior/object-visual-and-transform.md`; how the first renderer stores, skins and shades these meshes
is in `docs/behavior/model-shading.md`; the detail window a skinned progressive mesh draws is in
`docs/behavior/object-detail-level.md` section 4. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/formats/ogf.md`: the visual header (chunk 1, section 2) and the progressive detail windows (section 6).
- `docs/formats/chunks.md`: chunk sequences, lookup by id.
- `docs/formats/bone-physics.md`: the meaning of every field of chunk 16 beyond the layout and the bind pose
  given here.
- `docs/formats/skeletal-motions.md`: chunks 14, 15 and 19 (motions).

Conventions: little-endian; `u8`/`u16`/`u32`, `s16`, `f32`; `float3` = 3 × `f32`; `stringZ` zero-terminated
Windows-1251; "chunk" = `u32 id`, `u32 size`, payload, found by id; matrices in the row-vector convention (a
point `p` is transformed as `[p 1] · M`; `M[r][c]`, rows and columns counted from 0); angles in radians.
"Model space" uses the world's axes and units (`docs/behavior/world-coordinates.md` section 1).

**Checks.** Bracketed ids [SK-*n*] name the checks of the dirty workspace's
`checks/level-objects-split/run_all.py` (decoders `ogf.py`, `refs.py`, `named.py` beside it), which reads every
`.ogf` file of the archives (merged view: later archives win) and the shipped binaries.

## 1. Visual types

A model file is a chunk sequence in the visual structure of `ogf.md` (header chunk 1, 44 bytes, version 4;
type in byte 1). Each chunk id occurs at most once per level of nesting [SK-8]. Types in the game data (all
1,581 `.ogf` files of the archives, merged view) [SK-1, SK-4]:

| Type | Name used here | Top-level chunks | Files | Referenced by records |
|---|---|---|---|---|
| 10 | skeleton model | 1, 9, 13, 16, 18, optional 17 | 1,249 | 664 |
| 3 | animated skeleton model | 1, 9, 13, 16, 18, optional 17, and either 19 or both 14 and 15 | 330 | 155 |
| 1 | group | 1, 9, 18 | 1 | 0 |
| 0 | mesh | 1, 2, 3, 4 | 1 | 0 |

("Referenced by records": the files named by the visual names of `spawns\all.spawn`,
`object-visual-and-transform.md` section 1.)

Child meshes (inside chunk 9) [SK-1, SK-2]:

| Type | Name used here | Chunks | Vertex formats |
|---|---|---|---|
| 5 | skinned mesh | 1, 2, 3, 4 | 1-link, 2-link |
| 4 | skinned progressive mesh | 1, 2, 3, 4, 6 | 1-link, 2-link |
| 0 | mesh | 1, 2, 3, 4 | fixed layout 0x112 (only in the type-1 file) |

Visual types 3, 4, 5 and 10 are what these files add to `ogf.md`. The header's shader index (bytes 2–3) is 0
in every model file and is not used: standalone files name their shader in chunk 2 [SK-1]. The type-1 and
top-level type-0 files are referenced by no record and are not needed [SK-1, SK-4].

## 2. Header boxes

Boxes and spheres are in model space (bind pose). In all 2,480 child meshes of all files the header box is
exactly the box of the mesh's vertex positions, and the header sphere contains every position (within 2e−7).
The header box of a skeleton model contains the boxes of all its children [SK-2].

## 3. Skeleton models (types 10 and 3)

| Chunk | Content |
|---|---|
| 1 | header |
| 9 | child meshes: sub-chunks with ids 0, 1, 2, … in order, each a complete visual (section 5) |
| 13 | bone names and hierarchy (section 4) |
| 16 | per-bone physics and bind data (section 4) |
| 17 | user data: LTX text (e.g. `[disable]`, `[icon]`); not needed for drawing |
| 18 | description (source file name, author, times); not needed |
| 19 | (type 3) comma-separated list of motion files; animation (`skeletal-motions.md`) |
| 14, 15 | (type 3) embedded motions and motion parameters; animation (`skeletal-motions.md`) |

[SK-1, SK-8]. Every child of a type-3 or type-10 model has type 4 or 5 [SK-1].

## 4. Bones

**Chunk 13**: `u32` bone count *n*, then *n* records:

| Field | Type |
|---|---|
| bone name | `stringZ` |
| parent name | `stringZ` (empty for the root) |
| bounding box | 15 × `f32`: 3 × 3 rotation (rows), `float3` centre, `float3` half-size, in the bone's own bind space |

Bone index = position in this list (0-based). Every name in the data is lower-case. Exactly one bone has an
empty parent name (the root); every other parent name names a bone of the list. Bone counts: 1 … 61 in all
files [SK-1, SK-3].

**Chunk 16**: one record per bone, in the order of chunk 13:

| Field | Type | Use here |
|---|---|---|
| version | `u32` | 1 in all files |
| game material | `stringZ` | physics |
| shape | 112 bytes (`u16` shape type, `u16` flags, a box of 15 `f32`, a sphere of 4 `f32`, a cylinder of 8 `f32`) | physics |
| joint type | `u32` | physics |
| joint limits | 12 × `f32` | physics |
| spring, damping | 2 × `f32` | physics |
| joint flags | `u32` | physics |
| break force, break torque | 2 × `f32` | physics |
| friction | `f32` | present only if version ≠ 0 |
| **bind rotation** | `float3` (*r*₀, *r*₁, *r*₂) | bind pose |
| **bind position** | `float3` **t** | bind pose |
| mass | `f32` | physics |
| centre of mass | `float3` | physics |

Parsing every chunk 16 of every file with this layout consumes it exactly [SK-1, SK-8]. The physics fields
are specified in `bone-physics.md`.

**Bind pose.** Each bone's local bind matrix is *L* = [[R(−*r*₀, −*r*₁, −*r*₂), 0], [**t**, 1]] with R the
rotation of `object-visual-and-transform.md` section 4 evaluated at the **negated** angles (sx = sin(−*r*₀),
…). The bone's model-space bind matrix is *B* = *L* · *B*(parent) (row-vector convention: the child's local
matrix first), *B*(root) = *L*(root). Evidence: for the 51 bones of `actors\trader\trader.ogf` that own at
least 5 vertices (1-link: its bone; 2-link: *b*₁ if *w* ≥ 0.5, else *b*₀), the centre of the chunk-13 box
transformed by *B* lies on average 0.035 m (max 0.158 m) from the mean of the bone's vertices; with
non-negated angles or the other multiplication order the averages are 1.04 … 1.58 m [SK-7]. Drawing at rest
does not need the bind pose (`model-shading.md` section 2).

## 5. Child meshes

| Chunk | Content |
|---|---|
| 1 | header (type 5, 4 or 0) |
| 2 | two `stringZ`: texture name, then engine shader name (e.g. `wood\wood_walls8`, `models\model`) |
| 3 | vertices: `u32` format, `u32` count *vc*, then *vc* records (section 6) |
| 4 | indices: `u32` count *ic* (a multiple of 3), then *ic* × `u16`; a triangle list |
| 6 | (type 4 only) detail windows, in the layout of `ogf.md` section 6 (4 reserved `u32` = 0, `u32` *n*, *n* × {`u32` offset, `u16` triangles, `u16` vertices}; offset counted in indices) |

Indices are 0-based into this mesh's own vertices. In every child mesh of every file every index is < *vc*,
the smallest index is 0, and for type 4 the windows satisfy every statement of `ogf.md` section 6 (level 0
ends at *ic* and uses all *vc* vertices; triangles and vertices never increase; every window lies inside the
index list and every index of a window is below its vertex count) [SK-1].

## 6. Vertex formats

The vertex formats of a skinned mesh (types 4 and 5) are exactly the two below; any other value there is
fatal ("Invalid vertex type in skinned model '%s'") [SK-1, SK-5]. Format 0x112 occurs only in type-0 meshes
[SK-2].

**1-link (format 0x12071980)**, 60 bytes:

| Offset | Type | Meaning |
|---|---|---|
| 0 | `float3` | position **P** (model space, bind pose) |
| 12 | `float3` | normal **N** |
| 24 | `float3` | tangent |
| 36 | `float3` | binormal |
| 48 | 2 × `f32` | texture coordinate (u, v) |
| 56 | `u32` | bone index *b* (into chunk 13 of the containing model) |

**2-link (format 0x240E3300)**, 64 bytes:

| Offset | Type | Meaning |
|---|---|---|
| 0 | `u16` | bone index *b*₀ |
| 2 | `u16` | bone index *b*₁ |
| 4 | `float3` | position **P** |
| 16 | `float3` | normal **N** |
| 28 | `float3` | tangent |
| 40 | `float3` | binormal |
| 52 | `f32` | weight *w* of *b*₁ (0 … 1) |
| 56 | 2 × `f32` | texture coordinate (u, v) |

**Fixed layout (format 0x112)**, 32 bytes: `float3` position, `float3` normal, 2 × `f32` texture coordinate.

Texture coordinates address the texture with the Direct3D convention (v grows downward), as for the level's
surfaces. Normals: in the referenced files 931,504 of 931,510 have length 1 within 0.01 and 6 have length 0
[SK-4].

Bone indices are < the model's bone count in every vertex of every file; all 2-link weights lie in 0 … 1
(referenced files: 307,655 vertices with *w* = 0, 123 with *w* = 1, 281,684 in between) [SK-1, SK-4].

## 7. Verification facts

### 7.1 Invariants over the model files

Over the 819 files referenced by the records of all levels — 664 of type 10, 155 of type 3 (117 with chunk
19, 38 with chunks 14 and 15); child meshes: 581 type 5 with 1-link, 31 type 5 with 2-link, 378 type 4 with
1-link, 250 type 4 with 2-link; 931,510 vertices and 16,298,541 indices in total — and over all 1,581 `.ogf`
files of the archives (2,146,420 vertices, 32,591,853 indices) [SK-1, SK-2, SK-4]:

- every header has version 4 and shader index 0; top-level types and chunk sets are those of section 1;
  child chunk sets and vertex formats are those of sections 1 and 6;
- chunk 9's sub-chunk ids are 0, 1, 2, … in order; every child of a type-3/10 model has type 4 or 5;
- chunk 13 has exactly one root and every parent name exists; chunk 16 has one record per bone, version 1,
  and is consumed exactly by the layout of section 4;
- vertex chunks have size 8 + 60 *vc* (1-link), 8 + 64 *vc* (2-link) or 8 + 32 *vc* (0x112); index chunks
  4 + 2 *ic*; *ic* a multiple of 3; every index < *vc* and the smallest is 0;
- every bone index of every vertex is below the model's bone count; 2-link weights lie in 0 … 1;
- each child header box equals the box of its stored positions; child boxes lie inside the model's header
  box;
- type-4 windows satisfy `ogf.md` section 6.

### 7.2 Worked examples: model files of `l01_escape` objects

Sums are over the stored vertices of the child [SK-6]:

- `meshes\physics\door\door_trader.ogf` (96,656 bytes): type 10, 3 bones (`link` root, `door`, `lock`; bind
  positions in model space (0, 1.9743, 0), (0, 1.3675, 0), (1.3061, 0.7045, 0)), 1 child of type 4, 1-link,
  1,177 vertices, 12,285 indices, 19 detail levels, texture `door\door_trader`, shader `models\weapons`.
  Sums over its vertices: position (1078.4926, 1716.3511, 25.1468), normal (42.6894, −8.6118, 30.0575),
  (u, v) (583.1458, 295.4061).
- `meshes\physics\light\lampa_01_on.ogf` (54,863 bytes): type 10, 7 bones; 2 children of type 5 (1-link):
  699 vertices / 2,268 indices (`prop\prop_iten4`, `models\model`) and 85 / 288 (`prop\prop_iten4`,
  `models\selflight`).
- `levels\l01_escape\meshes\brkbl#0.ogf` (11,622 bytes): type 10, 9 bones, 1 child (type 5, 134 vertices, 282
  indices, `glas\glas_dirt`, `models\window`).
- `meshes\physics\equipment_cache\equipment_box_01_case.ogf` (2,183 bytes): type 10, 1 bone, 1 child (24
  vertices, 36 indices, `glow\glow_ghost`, `models\model_aref`).
- `meshes\actors\trader\trader.ogf` (417,583 bytes): type 3 (chunk 19: `actors\trader_animation`), 55 bones,
  root `bip01_pelvis`; 1 child of type 4, 2-link, 3,338 vertices, 87,048 indices, 1,362 detail levels,
  `act\act_barman`, `models\model`; sum of the 2-link weights 670.5939. Bind positions: `bip01_pelvis` (0,
  0.8264, −0.1119), `bip01_head` (−0.0001, 1.5192, −0.0752).
- `meshes\physics\vehicles\mi24\veh_mi24_u_01.ogf`: type 3, 29 bones, 3 children (type 4, 1-link: 9,833
  vertices / 270,786 indices with `models\weapons`, 92 / 192 and 418 / 5,340 with `models\transparent`).

## 8. Unestablished

Nothing of the layout. The meaning of chunks 17 and 18 beyond their text, which drawing does not need.

## 9. Provenance

- Date: 2026-11-24 (as part of an earlier specification of the level's objects); moved into this specification
  unchanged on 2026-11-27 when that specification was split by system.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrRender_R1.dll`.
- Data examined: every `.ogf` file under `meshes\` and `levels\*\meshes\`; `spawns\all.spawn` for the
  referenced files.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation) of the model-file
  loader (vertex formats); a throwaway decoder written from this text reads every model file and reproduces
  every count and invariant of section 7; the bind-pose convention was checked against the data as described
  in section 4.
- No leaked source code, derived projects or third-party documentation of the engine were used.
