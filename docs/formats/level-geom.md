# `level.geom` and `level.geomx`

Functional specification of the static level geometry files `levels\<level>\level.geom` and
`levels\<level>\level.geomx` of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): their chunks,
the vertex declarations, vertex and index buffers, the progressive-mesh ("sliding window") level of
detail records, the exact decoding of every packed vertex attribute into floating-point values as
the first renderer's vertex programs consume them, and the role of `level.geomx`.

Conventions: integers are little-endian; `u16`/`u32` unsigned, `i16` signed (two's complement);
`f32` is an IEEE-754 single. Sizes are in bytes. Both files are chunk sequences as defined by the
chunk-container specification (`chunks`); "chunk n" means the first top-level chunk with id n.
Element type and usage codes are those of Direct3D 9's public vertex-declaration API
(`D3DVERTEXELEMENT9`, `D3DDECLTYPE`, `D3DDECLUSAGE`); everything needed from it is restated here.

## 1. Role of the two files

- `level.geom` holds every vertex and index of the level's static visuals (the visuals of `level`
  chunk 3) with full vertex attributes. The **first renderer** (the default configuration) reads
  only this file.
- `level.geomx` holds a second copy of the same static geometry with **positions only**. Only the
  **second renderer** reads it (in addition to `level.geom`); it draws a visual from this copy,
  instead of from `level.geom`, in the rendering phase that uses the shaders' shadow-map element
  (the element whose programs, e.g. `shaders\r2\shadow_direct_base.vs`, take only a position). Visuals
  that need texture coordinates in that phase (transparent, trees, wallmarks, nearly all
  alpha-tested ones; see section 7) have no `level.geomx` geometry and are drawn from
  `level.geom`.
- A level directory is offered as a level only if `level`, `level.ltx`, `level.geom` and
  `level.cform` all exist; `level.geomx` is not part of that check.

Both files are read as streams, so none of their chunks may be compressed (`chunks`, section 5);
no shipped one is.

## 2. Chunks

Every shipped `level.geom` and `level.geomx` (30 levels plus `mp_military_2` in
`mods\mp_military_2.xdb0`) consists of exactly these four chunks in this order, filling the file:

| Id | Content | Required |
|---|---|---|
| 1 | header: `u16` version, `u16` (meaning **unestablished**) | not read by either renderer |
| 9 | vertex buffers (section 3) | yes: missing is fatal |
| 10 | index buffers (section 5) | first renderer: optional (no index buffers); second renderer: required |
| 11 | progressive-mesh records (section 6) | optional (first renderer and second renderer read it from `level.geom` only) |

Header values in the shipped files: the version is 14 in every file. The second value is one of 0,
18, 19, 45, 53 in `level.geom` and one of 0, 69, 252, 261, 267, 270, 482 in `level.geomx`
(`l01_escape`: 18 and 267). Nothing in either renderer depends on the header; a reader may accept
any header and should not require the chunk.

In `level.geomx`, chunk 11 always holds just a zero record count (4 bytes).

## 3. Chunk 9: vertex buffers

```
u32   buffer_count
repeat buffer_count times:
    declaration: 8-byte elements, up to and including the end marker (section 4)
    u32   vertex_count
    vertex_count × stride bytes of vertex data
```

The buffers follow each other without padding and fill the chunk exactly. Buffers are numbered
0, 1, 2, … in stored order; references to "vertex buffer i" (section 7) use this number.

`stride` is the vertex size of stream 0 implied by the declaration: the maximum over its elements
of `offset + size(type)` (sizes in section 4). In every shipped file the elements are contiguous
from offset 0, so this equals the sum of the element sizes. A declaration has at most 64 elements
before its end marker.

Every buffer has at most 65,535 vertices (indices are 16-bit). A buffer's vertices are not ordered
in any way a reader needs to know about.

## 4. Vertex declarations

Each element is 8 bytes:

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | stream (`u16`); `0xFF` marks the end of the declaration |
| 2 | 2 | offset of the attribute within the vertex (`u16`) |
| 4 | 1 | type |
| 5 | 1 | method (0 in all data: default) |
| 6 | 1 | usage |
| 7 | 1 | usage index |

The end marker is the element (stream `0xFF`, offset 0, type 17 = unused, method 0, usage 0,
usage index 0); every shipped declaration ends with exactly these 8 bytes. All other elements of
all shipped declarations use stream 0 and method 0.

Types used, with their stored size and the value a vertex program receives (a 4-component vector;
missing components are filled as shown):

| Code | Name | Size | Stored as | Program input `(x, y, z, w)` |
|---|---|---|---|---|
| 2 | FLOAT3 | 12 | three `f32` a, b, c | (a, b, c, 1) |
| 4 | D3DCOLOR | 4 | four bytes `b0 b1 b2 b3` | (b2/255, b1/255, b0/255, b3/255) — note the swap of bytes 0 and 2 |
| 6 | SHORT2 | 4 | two `i16` a, b | (a, b, 0, 1), integers converted to float without scaling |
| 7 | SHORT4 | 8 | four `i16` a, b, c, d | (a, b, c, d), integers converted to float without scaling |

Usages used: 0 = position, 3 = normal, 5 = texture coordinate, 6 = tangent, 7 = binormal,
10 = colour.

### 4.1 The layouts in the data

`level.geom` uses exactly three declarations, all with a 32-byte stride (offset: type, usage, usage
index):

| Layout | Elements | Used for |
|---|---|---|
| **A** (trees) | 0: FLOAT3 position; 12: D3DCOLOR normal; 16: D3DCOLOR tangent; 20: D3DCOLOR binormal; 24: SHORT4 texcoord 0 | tree and bush visuals (`level` visual types 7 and 11) |
| **B** (vertex-lit) | 0: FLOAT3 position; 12: D3DCOLOR normal; 16: D3DCOLOR tangent; 20: D3DCOLOR binormal; 24: D3DCOLOR colour 0; 28: SHORT2 texcoord 0 | meshes with per-vertex lighting (engine shaders `def_shaders\def_vertex`, `…\def_aref_v`, `…\def_trans_v`, `selflight`, `effects\lightplanes`, `effects\wallmarkblend`, …) |
| **C** (light-mapped) | 0: FLOAT3 position; 12: D3DCOLOR normal; 16: D3DCOLOR tangent; 20: D3DCOLOR binormal; 24: SHORT2 texcoord 0; 28: SHORT2 texcoord 1 | meshes with a light map (`default`, `def_shaders\def_aref`, `…\def_trans`) and terrain (`levels\<level>_grass`, `…_asfalt`, …) |

`level.geomx` uses exactly one declaration, **P**: 0: FLOAT3 position (stride 12).

The 70 + 122 + 243 vertex buffers of all shipped `level.geom` files use layouts A, B and C
respectively; all 197 vertex buffers of all `level.geomx` files use P. A layout is recognised by its
exact element list; a reader may also simply interpret the elements generically.

## 5. Chunk 10: index buffers

```
u32   buffer_count
repeat buffer_count times:
    u32   index_count
    index_count × u16 indices
```

No padding; the buffers fill the chunk. Buffers are numbered 0, 1, 2, … in stored order. Indices
describe triangle lists (three indices per triangle) and are **relative to the first vertex of the
visual that uses them** (section 7), not to the start of the vertex buffer. Every shipped index
count is a multiple of 3.

## 6. Chunk 11: progressive-mesh records

Used by visuals with continuous level of detail whose detail data is stored here rather than in the
visual itself (in `level`, visual type 11: progressive trees; visual type 2 carries a record of the
same layout in its own data, see the visuals specification).

```
u32   record_count
repeat record_count times:
    u32 × 4   reserved (0 in every shipped record; read and ignored)
    u32       step_count
    step_count × 8-byte steps:
        u32   index_offset   first index of this step, relative to the visual's first index
        u16   triangle_count
        u16   vertex_count   number of vertices (from the visual's first vertex) this step uses
```

Records are numbered 0, 1, 2, … and are referenced by number from the visuals. Steps are ordered
from most to least detailed. Observed in every shipped record, relative to the visual that
references it (visual with vertex count `V` and index count `I`):

- step 0 has `index_offset + 3 × triangle_count = I` and `vertex_count = V` (full detail);
- triangle counts and index offsets are non-increasing from step to step (equal offsets occur),
  and the last step's index offset is 0;
- every index read by a step is below that step's `vertex_count`.

To draw a visual at step `k`: draw `triangle_count[k]` triangles from the visual's index buffer
starting at index `first_index + index_offset[k]`, with indices relative to the visual's first
vertex as usual, using `vertex_count[k]` vertices. With a detail factor `f` in [0, 1] (1 = full
detail), the first renderer chooses `k = floor((1 − f) × (step_count − 1) + 0.5)`; how `f` is
computed is outside this specification (**unestablished**). Drawing step 0 always gives the
complete mesh.

## 7. How visuals reference the geometry

The reference itself is part of each visual's data in `level` chunk 3 (visuals specification): a
24-byte record of six `u32`

| Field | Meaning |
|---|---|
| vertex buffer | number of the vertex buffer in chunk 9 |
| first vertex | index of the visual's first vertex within that buffer |
| vertex count | number of vertices the visual uses |
| index buffer | number of the index buffer in chunk 10 |
| first index | index of the visual's first index within that buffer |
| index count | number of indices (3 per triangle) |

Triangle `t` of the visual has corners `VB[first_vertex + IB[first_index + 3t + j]]`, j = 0, 1, 2.
The first renderer draws the visual's `index count / 3` triangles exactly so.

For every visual of every shipped level: the vertex and index ranges lie inside their buffers;
all indices are below the vertex count; and the visual's indices use every one of its vertices
(the smallest index is 0, the distinct indices are exactly 0 … vertex count − 1).

A visual that also has `level.geomx` geometry has a second record of the same layout (nested in
the visual's data, visuals specification) whose numbers refer to `level.geomx`'s chunks 9 and 10.
In the shipped data (all levels, 59,913 such visuals): its vertex count is never larger than the
`level.geom` one (positions shared by several full vertices are stored once), and the set of
distinct positions it uses equals that of its `level.geom` range except in 3 visuals, where it
lacks 1 or 2 positions. For visuals without continuous level of detail, its triangles, taken as
ordered triples of positions (so with the same winding), are the same collection as the
`level.geom` ones, possibly in a different order (all but 1 of 53,469 visuals). For visual type 2
it is indexed differently and comes with its own level-of-detail steps. Visuals of types 7 and 11, the `effects\…`
shaders and those with transparent engine shaders (`…\def_trans`, `…\def_trans_v`) never have
`level.geomx` geometry, and those with alpha-tested ones (`…\def_aref`, `…\def_aref_v`) have it
only in 8 visuals across all levels.

## 8. Decoding the vertex attributes

The decoding below is what the first renderer's vertex programs (game data:
`shaders\shared\common.h`, `shaders\r1\common.h`, `shaders\r1\lmap*.vs`, `impl*.vs`, `vert*.vs`,
`tree_*.vs`) do with the program inputs of section 4. Throughout, for a D3DCOLOR element with
stored bytes `b0 b1 b2 b3`:

- `rgb(e) = (b2, b1, b0) / 255` and `alpha(e) = b3 / 255`;
- `vec(e) = 2 × rgb(e) − 1`, i.e. `x = 2·b2/255 − 1`, `y = 2·b1/255 − 1`, `z = 2·b0/255 − 1`.

### 8.1 Common to layouts A, B and C

| Attribute | Decoding |
|---|---|
| position | the three `f32` as stored (x, y, z) |
| normal | `vec(normal)`; not renormalised by the light-mapped and vertex-lit programs (stored normals have length 1 within ±0.013) |
| tangent, binormal | `vec(tangent)`, `vec(binormal)` (not used by the first renderer's static-geometry programs; used by the second renderer) |

Positions of layouts B and C are in level (world) coordinates and are used directly. Positions,
normals, tangents and binormals of layout A are in the tree's own coordinates and are transformed by
the visual's transform (stored with the tree visual, visuals specification).

Measured in `l01_escape` over the first 100 triangles of every light-mapped and vertex-lit visual:
for 98–99 % of the corners, the decoded normal points the same way (cosine above 0.5) as the
triangle normal `(P1 − P0) × (P2 − P0)` of its corners `P0, P1, P2` in stored order and stored
coordinates (with the D3DCOLOR byte order above; reading the bytes unswapped matches only about
65–80 %). Tangent and binormal are each within 12° of perpendicular to the normal for over 90 % of
the vertices.

### 8.2 Layout C (light-mapped)

Let `s0, s1` be the two `i16` of texcoord 0 and `l0, l1` the two `i16` of texcoord 1.

- **Base texture coordinate** `(u, v) = ((s0 + alpha(tangent)) × 32 / 32768, (s1 + alpha(binormal))
  × 32 / 32768)`, i.e. `(s + a/255) / 1024`; the alpha bytes of tangent and binormal are fractional
  extensions of the stored integers.
- **Light-map coordinate** `(u, v) = (l0 / 32768, l1 / 32768)`. Shipped values are 0 … 32,767, so
  the light-map coordinates are in [0, 1).
- `alpha(normal)` is a hemisphere-occlusion term; the light-mapped programs do not use it.
- Programs: `lmap*.vs` (engine shaders of class "lightmap", e.g. `default`) sample the light map
  (the level's `lmap#…_1` / `lmap#…_2` textures named by the shader) at the light-map coordinate.
  `impl*.vs` (terrain, e.g. `levels\escape_grass`) ignore texcoord 1 and sample the base texture
  and the terrain light map (`…_lm`) both at the base coordinate.

### 8.3 Layout B (vertex-lit)

Let `s0, s1` be the two `i16` of texcoord 0.

- **Base texture coordinate** as for layout C: `((s0 + alpha(tangent)) × 32 / 32768,
  (s1 + alpha(binormal)) × 32 / 32768)`.
- **Colour 0**: `rgb(colour)` is precomputed light (added as is); `alpha(colour)` is the sun
  visibility; `alpha(normal)` is the hemisphere (sky) visibility.
- The first renderer's `vert*.vs` combine them as
  `light = rgb(colour) + hemi_colour × alpha(normal) + sun_colour × max(0, dot(n, −sun_dir)) ×
  alpha(colour) + ambient`, where `n` is the decoded normal and the colours, sun direction and
  ambient come from the current environment (environment specification).

### 8.4 Layout A (trees)

Let `s0, s1, s2, s3` be the four `i16` of texcoord 0.

- **Texture coordinate** `(u, v) = (s0 / 2048, s1 / 2048)` (the first renderer sets the scale
  constant to 1/2048 for both components). The tangent and binormal alpha bytes are 0 and are not
  used for trees.
- **Wind rigidity** `s2 / 2048` (shipped values 16 … 3,335): the fraction of the wind sway applied
  to the vertex (tree animation; outside a static render).
- `s3` is 0 in every shipped vertex and is unused.
- `alpha(normal)` is the vertex's single light level `L`; the tree programs turn it into colour
  with per-visual scale and bias constants stored with the visual (visuals specification), roughly
  `rgb = scale.rgb × L + bias.rgb`, hemisphere `= scale.w × L + bias.w`.

### 8.5 Layout P (`level.geomx`)

Position only: the three `f32` as stored, in level coordinates.

## 9. Test vectors

`levels\l01_escape\level.geom` (56,995,500 bytes, `gamedata.db0`):

- chunk 1: `0E 00 12 00` (version 14, second value 18).
- chunk 9: 19 vertex buffers, 1,165,444 vertices in total. Layout and vertex count by buffer:
  0 A 65,532; 1 A 65,525; 2 A 65,535; 3 A 65,466; 4 A 17,208; 5 B 65,533; 6 C 65,534; 7 B 65,534;
  8 B 65,531; 9 C 65,535; 10 C 65,535; 11 B 46,595; 12 C 65,534; 13 C 65,533; 14 C 65,535;
  15 C 65,532; 16 C 65,533; 17 C 65,533; 18 C 53,181. Per layout: A 279,266, B 243,193,
  C 642,985 vertices.
- chunk 10: 10 index buffers with 1,048,575; 1,048,572; 1,048,572; 1,048,572; 1,048,572;
  1,048,572; 1,048,575; 1,048,572; 1,048,575; 228,285 indices (9,665,442 in total). The sum of all
  indices of all buffers is 20,158,230,477.
- chunk 11: 97 records with 45,910 steps in total, all reserved words 0; the sum of all steps'
  triangle counts is 206,758,914. Record 0 has 1,999 steps, the first being (index offset 138,876,
  7,441 triangles, 6,232 vertices) and the last (0, 3,445, 4,234).
- Position bounding box over all vertices of all buffers (tree vertices included, in their own
  coordinates): x −706.2949 … 791.9193, y −96.8817 … 115.4424, z −1132.7742 … 1284.6455.
- Sums of decoded values over all vertices, buffers in stored order, accumulated in double
  precision (agreement to about 6 significant digits is expected):

  | Layout | Σx | Σy | Σz | Σ normal x | Σ normal y | Σ normal z | Σu | Σv |
  |---|---|---|---|---|---|---|---|---|
  | A | 1,543.857 | 858,158.181 | 21,295.825 | −1,306.682 | 4,057.522 | 1,909.765 | 137,907.235 | 151,770.555 |
  | B | −15,952,682.922 | −1,189,764.774 | −15,682,451.543 | −288.678 | 57,189.588 | −3,061.337 | 123,441.981 | 128,703.135 |
  | C | −18,071,883.845 | −2,306,859.350 | 7,820,393.116 | 198.145 | 77,973.369 | −2,080.129 | 315,949.164 | 321,140.995 |

  Further sums: layout C light-map Σu 322,198.036, Σv 318,530.206; layout B colour Σr 9,400.282,
  Σg 7,924.125, Σb 5,268.024, Σalpha 31,010.314; Σ alpha(normal): A 84,581.314, B 56,213.275,
  C 90,685.224; layout A Σ wind rigidity 170,410.061.

`levels\l01_escape\level.geomx` (13,401,518 bytes, `gamedata.db0`):

- chunk 1: `0E 00 0B 01` (14, 267); chunk 11: `00 00 00 00`.
- chunk 9: 8 buffers of layout P with 65,533; 65,534; 65,534; 65,535; 65,535; 65,534; 65,535;
  14,030 vertices (472,770 in total); Σx −11,284,761.953, Σy −499,529.358, Σz 5,738,854.187;
  same bounding box as `level.geom`'s.
- chunk 10: 4 buffers with 1,048,575; 1,048,575; 1,048,575; 718,302 indices; sum of all indices
  892,427,944.

Visual references (`levels\l01_escape\level`): 7,782 visuals reference `level.geom`, 2,967 of them
also `level.geomx`; the 1,688 visuals of type 11 reference all 97 progressive-mesh records.

Invariants holding for all 31 shipped levels (30 in `gamedata.db*`, plus `mp_military_2`):

- chunks 1, 9, 10, 11 in this order fill each file; version 14;
- vertex counts ≤ 65,535; strides 32 (`level.geom`) and 12 (`level.geomx`); only layouts A, B, C
  (`level.geom`) and P (`level.geomx`);
- all index counts are multiples of 3;
- `level.geomx` chunk 11 holds 0 records; `level.geom` chunk 11 holds 0 records in `l04u_labx18`,
  `l10u_bunker`, `l12u_control_monolith`, `l12u_sarcofag` and `testers_mp_pool`;
- the visual-reference properties of sections 6 and 7.

## Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` and `mods\mp_military_2.xdb0`; binaries `bin\xrRender_R1.dll`,
  `bin\xrRender_R2.dll`, `bin\XR_3DA.exe`, `bin\xrCore.dll`.
- Data examined: `level.geom`, `level.geomx` and `level` of all 31 levels; the shipped shader
  sources `shaders\shared\common.h`, `shaders\r1\*.vs`, `shaders\r1\common.h`,
  `shaders\r2\shadow_direct_*.vs` read as data. Measurements: chunk walks, declaration census,
  buffer and record sizes, cross-checks of every visual's geometry reference against the buffers
  and progressive-mesh records, `level.geom`/`level.geomx` triangle comparison by position,
  normal-versus-triangle-orientation statistics (which established the D3DCOLOR byte order),
  value ranges of every packed attribute, and visual-to-layout-to-engine-shader census.
- Binaries: static analysis (Ghidra 12.2, headless) of both renderers' loading of these files
  (which chunks are read and required, the declaration and buffer reading, use of chunk 11),
  of how visuals select and draw their geometry ranges and level-of-detail steps, of the tree
  texture-coordinate scale constant, of when the second renderer uses `level.geomx`, and of the
  level-directory check. No execution or debugging.
- A throwaway parser written from this text decoded every buffer of all 62 files and reproduced the
  test vectors above.
- No leaked source code, derived projects or third-party documentation of the engine were used.
  Public references: Direct3D 9 vertex-declaration types and their conversion to vertex-program
  inputs.
