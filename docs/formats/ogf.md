# Visual structure (`.ogf`) of the level's static world

Format specification of the visual structure used for the static visuals stored in chunk 3 of the `level`
file (`level.md` section 4), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): every visual type
the static world uses, the hierarchy, bounding volumes, references into `level.geom` / `level.geomx`,
the per-visual shader reference, progressive (LOD) meshes, distant billboards, tree instances with their
transforms and per-instance lighting, and which visuals the first renderer draws for a frame.

Related specifications: chunk container `chunks.md` (scoping specification `level-loading.md` section 5.1);
vertex/index buffers and vertex decoding `level-geom.md`; `level.md`.

Conventions as in `level.md`: little-endian; `u8`/`u16`/`u32`/`f32`; `float3` = 3 × f32; a chunk is
`u32 id`, `u32 size`, payload, found by id (chunk order inside a visual has no meaning; in the shipped
data chunk 1 is stored last in meshes, progressive meshes and tree instances except type 11 (order 21, 12,
1, 20), and first in groups and billboard groups (orders 1, 10 and 1, 10, 11)). "VB *k*" / "IB *k*" = vertex buffer / index buffer *k* of
`level.geom` chunk 9 / chunk 10 (`level-geom.md`); "window record *k*" = record *k* of `level.geom`
chunk 11.

## 1. Visual types

A visual is a chunk sequence. Its type is byte 1 of its header (chunk 1). Types used by the static world of
every shipped level (counts for `l01_escape` / all 30 levels):

| Type | Name used here | Chunks | `l01_escape` | All levels |
|---|---|---|---|---|
| 0 | mesh | 1, 21, optional 22 | 2,861 | 55,807 |
| 1 | group | 1, 10 | 1,917 | 22,407 |
| 2 | progressive mesh | 1, 21, 6, optional 22 | 411 | 6,770 |
| 6 | billboard group (distant LOD) | 1, 10, 11 | 1,839 | 15,236 |
| 7 | tree instance | 1, 21, 12 | 2,822 | 18,644 |
| 11 | progressive tree instance | 1, 21, 12, 20 | 1,688 | 14,802 |

No other chunk ids occur in level visuals. Type values 3, 4, 5, 8, 9 and 10 exist for standalone model files
(characters, objects, particles); they do not occur in the level's visuals and are out of scope. Any value
above 11 is fatal ("Unknown visual type").

**Standalone `.ogf` files.** The static world needs none: every visual drawn for the static world is
embedded in the `level` file, and level visuals refer to other visuals only by index. The model files in the
level directory (`meshes\brkbl#*.ogf`, all of type 10, chunks 1, 18, 9, 13, 16) are breakable objects
spawned as objects, out of scope. A standalone file uses the same header (chunk 1).

## 2. Header (chunk 1, 44 bytes; every visual)

| Offset | Type | Meaning |
|---|---|---|
| 0 | u8 | format version; must be 4 (checked) |
| 1 | u8 | visual type (section 1) |
| 2 | u16 | shader index into `level` chunk 2; 0 = none |
| 4 | float3 | bounding box minimum |
| 16 | float3 | bounding box maximum |
| 28 | float3 | bounding sphere centre |
| 40 | f32 | bounding sphere radius |

Bounding volumes are in world coordinates for every type (for tree instances: of the transformed tree).
The box and the sphere are stored independently: the sphere centre is in general not the box centre and
the radius is not the half-diagonal. In all shipped levels the box of a mesh, progressive mesh or tree
instance is the exact box (within 0.01) of the vertices its index range uses (transformed, for trees), and
every child's box lies inside its parent's box. Culling uses the sphere and the box (section 8).

Shader index use by type, in all shipped levels: groups (1) always 0; every other type non-zero. Billboard
groups (6) always reference an entry with engine shader `details\lod` (`details\lod/level_lods` in
`l01_escape`); their children's own shaders are used for the children. Tree instances (7, 11) use the
engine shaders `flora\leaf_wave`, `flora\trunk_wave` or `def_shaders\def_objects_lod`.

## 3. Geometry reference (chunk 21, 24 bytes; types 0, 2, 7, 11)

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | vertex buffer index *vb* (into `level.geom` chunk 9) |
| 4 | u32 | first vertex *v0* |
| 8 | u32 | vertex count *vc* |
| 12 | u32 | index buffer index *ib* (into `level.geom` chunk 10) |
| 16 | u32 | first index *i0* |
| 20 | u32 | index count *ic* (a multiple of 3) |

The visual's triangles are the triangle list formed by indices *i0* … *i0*+*ic*−1 of IB *ib*; each index is
**relative to *v0***: index value *j* means vertex *v0* + *j* of VB *vb*. In all shipped levels every
referenced range lies inside its buffer, every index is < *vc*, and the smallest index of each visual is 0.
The first renderer draws exactly *ic*/3 triangles for a mesh (type 0) and a tree instance (type 7);
progressive types draw a sub-range (section 6).

Several visuals may reference the same ranges: in `l01_escape` the 7,782 chunk-21 visuals use 3,448
distinct references (trees of the same species share one mesh and differ by their transform, section 5).

Vertex layouts in use (stride 32 for all; element decoding in `level-geom.md`):

| Visual types | Elements (offset: type, usage) |
|---|---|
| 7, 11 | 0: FLOAT3 position; 12: D3DCOLOR normal; 16: D3DCOLOR tangent; 20: D3DCOLOR binormal; 24: SHORT4 texcoord |
| 0, 2 | as above to 20; then either 24: D3DCOLOR colour, 28: SHORT2 texcoord, or 24: SHORT2 texcoord, 28: SHORT2 texcoord |

Mesh and progressive-mesh positions are world coordinates; tree positions are in the tree's own space
(section 5).

**Chunk 22 (second renderer only).** A chunk sequence holding a nested chunk 21 (and, for type 2, a nested
chunk 6 of section 6) with the same layout, but whose buffer indices refer to `level.geomx` chunks 9 and 10
instead. The first renderer ignores chunk 22.

## 4. Groups (chunk 10; types 1 and 6)

```
u32   n
n × u32   child visual indices (into level chunk 3)
```

A group draws nothing itself; drawing it means drawing its children (subject to section 8). Groups nest
(group children of groups). In every shipped level a child index is smaller than the parent's own index,
and no visual has more than one parent. In `l01_escape` type-1 groups have 2 … 49 children (709
with 2, 485 with 3, 312 with 4).

Billboard groups (type 6) have 1 … 10 children (see section 9), all of them tree instances (type 7 or 11); every tree
instance of every shipped level is the child of exactly one billboard group.

## 5. Tree instances (chunk 12, 104 bytes; types 7 and 11)

| Offset | Type | Meaning |
|---|---|---|
| 0 | 16 × f32 | transform *M*, row-major 4 × 4, row-vector convention |
| 64 | 5 × f32 | lighting scale: *s*\_r, *s*\_g, *s*\_b, *s*\_hemi, *s*\_sun |
| 84 | 5 × f32 | lighting bias: *b*\_r, *b*\_g, *b*\_b, *b*\_hemi, *b*\_sun |

**Transform.** A tree vertex position (*x*, *y*, *z*) maps to world as (*x*, *y*, *z*, 1) × *M*:
world.x = *x*·M[0] + *y*·M[4] + *z*·M[8] + M[12], world.y = *x*·M[1] + *y*·M[5] + *z*·M[9] + M[13],
world.z = *x*·M[2] + *y*·M[6] + *z*·M[10] + M[14] (M[k] = the *k*-th f32). The fourth column (M[3], M[7],
M[11], M[15]) is (0, 0, 0, 1) in all shipped data. Normals transform by the upper 3 × 3 part in the same
way (the vertex programs do not renormalize). The world height of the tree's base, used by the wind sway,
is M[13].

**Per-instance lighting.** The first renderer passes these values to the tree vertex programs
(`shaders\r1\tree_*.vs`, game data) as constants, after multiplying every stored value by 0.75:

- `c_scale` = 0.75 × (*s*\_r, *s*\_g, *s*\_b, *s*\_hemi)
- `c_bias` = 0.75 × (*b*\_r, *b*\_g, *b*\_b, *b*\_hemi) + (*a*\_r, *a*\_g, *a*\_b, 0), where *a* is the
  current environment's ambient colour
- `c_sun` = 0.75 × (*s*\_sun, *b*\_sun), 0, 0
- `m_xform` = *M* (as above)
- `consts` = (1/2048, 1/2048, 0, 0): the SHORT4 texture coordinate of a tree vertex is (u, v, sway
  rigidity, unused) as signed 16-bit values; u and v times 1/2048 give the texture coordinate, the third
  value times 1/2048 the sway rigidity, which is not limited to 0 … 1. In `l01_escape` visual 3272
  (type 11) u, v lie in [−0.0009765625, 1.0] and the rigidity in [0.1699, 1.0547]; visual 3277 (type 7):
  u, v in [0.00049, 0.99609], rigidity in [0.5815, 0.6919]; the fourth value is 0 in both.

With these, the vertex programs compute per vertex, with *L* = the alpha component (0 … 1) of the vertex's
D3DCOLOR normal element: light = `c_scale`·*L* + `c_bias` (rgb part = precomputed colour, fourth part
scales the environment's hemisphere colour), sun factor = *L*·`c_sun`.x + `c_sun`.y (scales the
environment's sun colour). The exact combination is in the game's own `tree_s.vs`/`tree_w.vs`; which
program a tree uses comes from its shader (shader specification).

Wind sway (animation, not needed for a still image at rest): `wave` = (0.1, 0.01, 0.11, *t*) / (2π) and
`wind` = 0.005 × normalize(sin(0.2π·*t*), 0, cos(0.2π·*t*)), *t* = the engine's global time in seconds
(which clock exactly is **unestablished**).

In the data (test: any of the three rgb values exactly non-zero) the scale rgb part and the bias rgb part
are non-zero for the same instances: 728 of the 18,644 type-7 instances (15 in `l01_escape`) and 967 of
the 14,802 type-11 instances (15 in `l01_escape`); for all others both rgb parts are exactly zero. Example:
`l08_yantar` visual 4785, scale (0.6025, 0.4636, 0.1926, 0.2670, 0.0), bias (0.1071, 0.0824, 0.0341,
0.1175, 0.0) (rounded).

## 6. Progressive meshes (types 2 and 11)

A progressive mesh is stored as one index range (chunk 21) that contains a sliding window of triangle
sets; a detail level selects a window.

**Window list.** Type 2: chunk 6. Type 11: chunk 20 (4 bytes, one u32 = index *k* of a window record in
`level.geom` chunk 11). Both have the same layout:

```
4 × u32   reserved (0 in all shipped data)
u32       n            number of detail levels (≥ 1)
n × { u32 offset; u16 triangles; u16 vertices }
```

Level *j* (0 = most detailed) draws `triangles` triangles from the indices starting at *i0* + `offset`
(relative to *v0* as in section 3) and uses only the first `vertices` vertices of the range (every index
of the window is < `vertices`). In all shipped data: level 0 uses all *vc* vertices and ends exactly at the
end of the range (`offset` + 3·`triangles` = *ic*); triangles and vertices never increase with *j*; every
window lies inside the range.

`level.geom` chunk 11 (layout owned by `level-geom.md`): u32 record count, then the records back to back,
each in the layout above.

**Level selection.** Given the detail value *q* ∈ [0, 1] of section 8.3, the level drawn is
*j* = floor((1 − *q*) × (*n* − 1) + 0.5).

## 7. Billboards (chunk 11, 896 bytes; type 6)

8 faces × 4 vertices × 28 bytes (face *f*, vertex *k* at byte 112·*f* + 28·*k*):

| Offset | Type | Meaning |
|---|---|---|
| 0 | float3 | position (world) |
| 12 | 2 × f32 | texture coordinate (into the billboard atlas named by the visual's shader entry, e.g. `level_lods`) |
| 20 | u32 | D3DCOLOR (bytes B, G, R, A in file order): precomputed light colour r, g, b and hemisphere factor in A |
| 24 | u8 | sun factor (0 … 255) |
| 25 | 3 bytes | padding (arbitrary values; not used) |

The 8 faces are views of the group's trees from 8 directions. The face normal used for selection is
**N**\_f = −normalize(¼ Σ **t**ᵢ) over the four corner normals **t**₁ = normalize((**v**₁−**v**₀)×(**v**₂−**v**₁)),
**t**₂ = normalize((**v**₂−**v**₁)×(**v**₃−**v**₂)), **t**₃ = normalize((**v**₃−**v**₂)×(**v**₀−**v**₃)),
**t**₄ = normalize((**v**₀−**v**₃)×(**v**₁−**v**₀)) (a corner normal whose cross product has squared length
≤ 1.1754944e−38 is used unnormalized).

**Coverage factor.** Per billboard group a factor *k* is computed from its header: with *R* the sphere
radius and *m* the middle value of the three box half-extents (max − min)/2 sorted ascending,
*k* = 2 (*R*² asin(*m*/*R*) + *m* √(*R*² − *m*²)) / (π *R*²).

**Drawn quad.** When a billboard is drawn (section 8.2), with **e** = normalize(sphere centre − camera
position):

1. For each face, *w*\_f = **e** · **N**\_f. Sort the 8 faces by *w* ascending; let *F*₁ be the last (largest
   *w*), *F*₂ the second-to-last and *F*₃ the third-to-last.
2. Blend factor *β* = floor(((1 − (*w*(*F*₂) − *w*(*F*₃)) / (*w*(*F*₁) − *w*(*F*₃))) + 1) × 0.5 × 255.5),
   clamped to 0 … 255.
3. Fade *α* = floor((1 − (*σ* − *T*\_B) / max(*T*\_A − *T*\_B, 1e−7)) × 255), clamped to 0 … 255, with *σ*
   the group's screen measure (section 8.2) and *T*\_A, *T*\_B from `level.md` section 9.4.
4. One quad is emitted. Its vertex *q* = 0 … 3 is built from face vertex *k* = (3, 0, 2, 1)[*q*] of *F*₂
   ("0" values) and of *F*₁ ("1" values): position0/1 = the faces' vertex positions + **e** × (−0.5 *R*)
   (moved towards the camera by half the radius); normal0/1 = **N**\_{F₂} / **N**\_{F₁}; texcoord0/1;
   rgbh0/1 = the faces' D3DCOLOR; and one colour holding sun0 = *F*₂'s sun byte, sun1 = *F*₁'s sun byte,
   *α* and *β*. The quad's triangles are (*q*0, *q*1, *q*2) and (*q*3, *q*2, *q*1).
5. The game's own `shaders\r1\lod.vs`/`lod.ps` (game data; scripted shader `details_lod.s`) consume this
   vertex: input COLOR0 = (sun0, sun1, *α*, *β*)/255, positions and colours blended by *β*/255 towards
   *F*₁. The vertex layout handed to those programs, for reference: 0: FLOAT3 position0; 12: FLOAT3
   position1; 24: FLOAT3 normal0; 36: FLOAT3 normal1; 48: D3DCOLOR (A = *β*, R = sun0, G = sun1, B = *α*);
   52: FLOAT2 texcoord0; 60: FLOAT2 texcoord1; 68: D3DCOLOR rgbh0; 72: D3DCOLOR rgbh1 (76 bytes).

## 8. Which visuals are drawn

For every sector visited (`level.md` section 9) the sector's root visual is processed against each frustum
the sector was reached with. Processing a visual:

### 8.1 Culling

A visual whose bounding sphere/box is outside the frustum is not processed further (nothing below it is
drawn). Frustum culling only removes what is outside the view: a renderer that tests every visual, or only
the groups, produces the same image. The original also tests visuals against the occlusion mesh
`level.hom` (out of scope; a renderer without it draws a superset of hidden geometry). Then by type:

### 8.2 Groups and billboard groups

- Type 1: its children are processed.
- Type 6: with *D*² the squared distance from the camera position to the sphere centre and *R* the sphere
  radius, the screen measure is *σ* = *R* / (*D*² + 1e−5) × *k* (*k* from section 7). Then:
  - *σ* < *T*\_min: nothing is drawn (neither billboard nor trees);
  - *T*\_min ≤ *σ* < *T*\_A: the billboard is drawn (section 7), faded by *α*;
  - *σ* > *T*\_B: the children (trees) are processed.
  Between *T*\_B and *T*\_A both are drawn (cross-fade); with the defaults *T*\_B < *T*\_A.

### 8.3 Meshes and tree instances (types 0, 2, 7, 11)

The screen measure is *σ* = *R* / (*D*² + 1e−5) (as above, without *k*). The visual is drawn only if
*σ* > *T*\_min. For the progressive types (2, 11) the detail value is
*q* = √(clamp((*σ* − *P*\_lo) / (*P*\_hi − *P*\_lo), 0, 1)) and the level of section 6 is drawn; types 0
and 7 draw their whole range. Each visual is drawn with its own shader entry (header) and, for trees, the
constants of section 5.

The thresholds *T*\_min, *T*\_A, *T*\_B, *P*\_lo, *P*\_hi are defined in `level.md` section 9.4.

A static frame drawn without the camera-dependent parts (drawing every type-0/2 visual at level 0 and every
tree instance, and no billboard) shows the full-detail world; drawing both trees and billboards everywhere
is not what the original shows at any camera position closer than the cross-fade band.

## 9. Verification facts

`levels\l01_escape\level` chunk 3 (with `level.geom` of the same directory):

- 11,538 visuals: type 0: 2,861; 1: 1,917; 2: 411; 6: 1,839; 7: 2,822; 11: 1,688. All headers have
  version 4. SHA-256 of the 11,538 44-byte headers concatenated in index order:
  `a9c3107c8cf69a74a10781f79e503a3076d69b6ec76b1ce00b44266dd8026045`.
- 7,782 visuals have chunk 21, with 3,448 distinct (24-byte) references; the sum of their index counts is
  29,743,374.
- Triangles drawn when every type-0 and type-7 visual is drawn whole and every progressive visual at level 0:
  3,047,891. Detail levels: 38,599 in the 411 chunk-6 lists; the 1,688 type-11 visuals reference window
  records whose lists hold 309,021 levels in total (counting a shared record once per referencing visual).
- Billboard groups have 1 (109 groups), 2 (1,173), 3 (179), 4 (377) or 10 (1) children.
- Visual 0: type 2, shader index 4, chunk 21 = (5, 31041, 128, 5, 583479, 1611); chunk 6 has 49 levels,
  level 0 = (offset 1131, 160 triangles, 128 vertices), level 48 = (0, 64, 80).
- Visual 3272: type 11, shader index 344, chunk 21 = (0, 0, 6232, 0, 0, 161199), window record 0 (1,999
  levels); transform translation (M[12], M[13], M[14]) = (22.2026, 15.8762, 664.0648); scale =
  (0, 0, 0, 0.6078, 0.5493), bias = (0, 0, 0, 0.2493, 0.1848) (rounded to 4 decimals).
- Visual 3273: type 6, shader index 345 (`details\lod/level_lods`), one child (3272); coverage factor
  *k* = 0.77375; face 0 normal **N**₀ = (−0.9067, −0.0120, −0.4215). For a camera at the sphere centre +
  (100, 0, 0): *F*₁ = face 7, *F*₂ = face 0, *β* = 135.

All 30 shipped levels:

- Visual counts by type as in section 1 (133,666 visuals); every header version 4.
- 96,023 visuals have chunk 21, with 64,721 distinct references per level summed over levels.
- Full-detail triangle count (definition as for `l01_escape`) summed over all levels: 36,756,603.
- Detail levels: 1,186,900 in chunk-6 lists; 3,287,550 via window records (per referencing visual).
- Groups (type 1) have 2 … 124 children; billboard groups 1 … 5 or 10 children (2,759 with 1, 8,870 with
  2, 1,507 with 3, 2,094 with 4, 2 with 5, 4 with 10).
- Invariants holding everywhere: every chunk-21 range and every index inside its buffer and below *vc*,
  minimum index 0, *ic* a multiple of 3, stride 32; header boxes equal (within 0.01) the boxes of the used,
  transformed vertices; child index < parent index, one parent per visual, child boxes inside parent boxes;
  billboard children are tree instances; tree transforms have fourth column (0, 0, 0, 1); window lists
  satisfy all statements of section 6; shader index 0 exactly for groups.

## 10. Unestablished

- Exact rules of occlusion culling with `level.hom` (section 8.1).
- Which clock drives the wind-sway time *t* (section 5).
- Meaning of the four reserved u32 of the window lists (0 everywhere).

## 11. Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe` and `bin\xrRender_R1.dll`.
- Data examined: `level`, `level.geom` and `level.geomx` of all 30 level directories (merged archive view),
  `levels\l01_escape\meshes\*.ogf`, the game's vertex/pixel programs `shaders\r1\lod.vs`, `lod.ps`,
  `tree_s.vs`, `tree_w.vs`, `details_lod.s` and `shaders\shared\common.h`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation) for what the
  first renderer reads, checks and computes per visual type; data consistency checks over all levels
  (index ranges, parent/child order, bounding boxes against decoded and transformed vertices, window lists);
  a throwaway parser written only from this text, `level.md` and the vertex-buffer layout reproduces every
  number in section 9.
