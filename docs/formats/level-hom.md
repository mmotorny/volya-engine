# `level.hom` (occlusion mesh of a level)

Functional specification of the file `levels\<level>\level.hom` of S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): a small triangle mesh, separate from the drawn geometry, that the first renderer draws
each frame into a coarse depth grid to skip what is hidden behind it. It covers the whole file, what the game
does with it when a level loads, and facts of every shipped copy. How the mesh is drawn into the grid and how
items are tested against it belong to the `occlusion-grid` and `occlusion-tests` specifications; they are
stated here only as far as they fix the meaning of a stored value.

Conventions: integers are little-endian, unsigned; reals are IEEE 754 single precision (`f32`); a `float3`
is three `f32` (x, y, z). Sizes are in bytes. Positions are in metres in the level's world frame (vertical
axis y). Abbreviations of approved specifications: CH `docs/formats/chunks.md`, VS
`docs/behavior/visibility-scope.md` (the scoping specification of this area).

**Checks.** Every claim carries a tag [D*n*] or [L*n*]. The checks are scripts in `checks/level-hom/` of the
dirty workspace: `extract.py` extracts every `level.hom` from all archives of the edition (`gamedata.db*`
and `mods\*`) with the project's archive tool; `data.py` [D…] reads the extracted files (and compares them
with the merged archive view); `binary.py` [L…] reads saved read-only static-analysis output of
`bin\xrRender_R1.dll` and `bin\xrCDB.dll` and constants from the binaries; `run.py` runs both. All checks
pass.

## 1. Where the file is and who reads it

- The file is `level.hom` in the level directory. 26 copies exist in the edition: 25 in `gamedata.db0` …
  `gamedata.db4` and `levels\mp_military_2\level.hom` in `mods\mp_military_2.xdb0` [D1]. The 25 copies
  are those of the merged archive view counted in VS 3.1 [D1].
- Of the 31 level directories (those with a `level.cform`), 5 have none: `l10u_bunker`, `l12u_sarcofag`,
  `testers_mp_bath`, `testers_mp_military_2`, `testers_mp_pool` [D1].
- The first renderer reads it from the level directory when the level loads. When the file does not exist,
  a warning naming the path is logged and the level has no occlusion mesh: nothing is occlusion-tested for
  that level (VS 3.2, 7) [L1].

## 2. Layout

The file is a sequence of chunks (CH): an 8-byte header (u32 id, u32 size) followed by `size` bytes. Every
shipped file has exactly two chunks, in this order, uncompressed, and nothing after them, so its size is
20 + 40 *n* for *n* records [D2]:

| Chunk | Size | Content |
|---|---|---|
| 0 | 4 | u32, 0 in every file [D2]. The game never reads this chunk [L1]. |
| 1 | 40 *n* | *n* triangle records [D2, L2]. |

Only chunk 1 is opened [L1]. Its records are read one after another while bytes remain in the chunk [L2].
What happens with a chunk 1 whose size is not a multiple of 40 is **unestablished** (no shipped file has
one [D2]).

### 2.1 Triangle record (40 bytes)

| Offset | Type | Meaning |
|---|---|---|
| 0 | float3 | vertex **v**₀ |
| 12 | float3 | vertex **v**₁ |
| 24 | float3 | vertex **v**₂ |
| 36 | u32 | **two-sided** flag |

[L2, D2]. All shipped coordinates are finite [D3].

The two-sided flag: with any non-zero value the triangle occludes whichever side the camera is on; with 0 it
occludes only while the camera is on its **front** side [L6]. The front side is the side the normal **n**
points to, with **n** = normalize((**v**₀ − **v**₁) × (**v**₀ − **v**₂)) and plane offset *d* = −**n**·**v**₀;
the camera at **E** is on the front side when **n**·**E** + *d* > 0 (VS 3.1) [L5, L6]. Shipped values are 0
and 1 only [D3].

## 3. Loading

When the level loads, the records are turned into the level's occlusion mesh [L2 – L5]:

1. **Vertex merging.** The vertices are taken in record order (**v**₀, **v**₁, **v**₂ of record 0, then of
   record 1, …). Each is compared with the vertices kept so far, in the order they were kept; the first kept
   vertex whose x, y and z each differ from it by less than 0.01 (strictly) is used in its place. If there is
   none, the vertex is kept as a new one. A triangle is then three indices into the kept vertices, plus the
   flag [L2, L3]. A vertex is therefore replaced by the **position** of the first kept vertex near it, not
   by an average.
2. **Neighbours.** Each triangle has three edges: edge 0 (**v**₀, **v**₁), edge 1 (**v**₁, **v**₂), edge 2
   (**v**₂, **v**₀), each identified by its two merged vertex indices regardless of direction. All edges of
   all triangles are sorted by those indices; wherever two edges next to each other in that order have the
   same two indices, the two triangles become each other's neighbour across that edge. An edge without a
   match has no neighbour [L4]. The indices are compared on their low 16 bits only [L4] (no shipped file
   has more than 2,192 merged vertices, so this never matters [D4]). When three or more triangles share an
   edge, each pair adjacent in the sorted order is linked, a later link replacing an earlier one in the same
   slot; the order among equal edges, and so which triangles end up linked, is **unestablished** [L4].
   **Disputed in review:** the order is established: edges are sorted by their lower index, then their higher
   index, then the index of their triangle (its position in record order), so equal edges are in ascending
   triangle order. For an edge shared by triangles *t*₁ < *t*₂ < … < *t*ₖ, each consecutive pair is linked in
   that order: *t*₁ ends up linked to *t*₂, *t*ₖ to *t*ₖ₋₁, and every triangle between to the next one.
3. **Per triangle**, from the merged vertices [L5]:
   - the flag;
   - the plane (**n**, *d*) of section 2.1;
   - the area, by Heron's formula from the three edge lengths. A triangle with area below 0.001 is reported
     in the log as invalid but is kept and used like any other;
   - the centre, the mean of the three vertices;
   - a skip counter, starting at 0 (its use: `occlusion-grid`).
4. A search structure over the merged mesh is built; the renderer uses it to find the triangles that meet the
   view frustum each frame (`occlusion-grid`). Occlusion is then available for the level [L5].

What the neighbour links and the centre are used for belongs to `occlusion-grid` (VS 3.2).

## 4. Facts of the shipped files

18,373 records in the 26 files (17,583 in the 25 files of the base archives, as VS 3.1) [D5]. 16,908 records
are two-sided and 1,465 one-sided; one-sided triangles occur in three files only: `l05_bar` (168),
`l07_military` (204) and `testers_mp_workshop` (all 1,093) [D3, D5].

Vertex merging (section 3, step 1) [D4]:

- It moves 27 vertex uses in 4 files: `l08_yantar` 4, `l11_pripyat` 10, `mp_military_2` 11,
  `testers_mp_factory` 2, each by at most 0.0097 on any axis. In every other file it only joins vertices with
  identical coordinates.
- No triangle loses a vertex to it (no two corners of a triangle merge), and no triangle has an area below
  0.001 before or after it, so nothing is reported as invalid.

Over all files, 14,051 edges belong to one triangle only and 66 edges to more than two [D6].

Per file [D7] (records; of them two-sided, one-sided; vertices after merging; file size in bytes; edges
belonging to one triangle; edges belonging to three or more):

| Level | Records | Two-sided | One-sided | Vertices | Bytes | Single edges | Edges of 3+ |
|---|---|---|---|---|---|---|---|
| `l01_escape` | 904 | 904 | 0 | 918 | 36,180 | 862 | 16 |
| `l02_garbage` | 446 | 446 | 0 | 504 | 17,860 | 539 | 26 |
| `l03_agroprom` | 519 | 519 | 0 | 547 | 20,780 | 501 | 4 |
| `l03u_agr_underground` | 680 | 680 | 0 | 371 | 27,220 | 82 | 0 |
| `l04_darkvalley` | 444 | 444 | 0 | 677 | 17,780 | 694 | 0 |
| `l04u_labx18` | 1,136 | 1,136 | 0 | 780 | 45,460 | 472 | 0 |
| `l05_bar` | 1,114 | 946 | 168 | 1,013 | 44,580 | 852 | 0 |
| `l06_rostok` | 794 | 794 | 0 | 885 | 31,780 | 834 | 0 |
| `l07_military` | 993 | 789 | 204 | 1,146 | 39,740 | 1,010 | 3 |
| `l08_yantar` | 1,350 | 1,350 | 0 | 930 | 54,020 | 400 | 2 |
| `l08u_brainlab` | 210 | 210 | 0 | 223 | 8,420 | 220 | 0 |
| `l10_radar` | 289 | 289 | 0 | 374 | 11,580 | 388 | 9 |
| `l11_pripyat` | 1,992 | 1,992 | 0 | 2,192 | 79,700 | 2,221 | 1 |
| `l12_stancia` | 303 | 303 | 0 | 327 | 12,140 | 307 | 0 |
| `l12_stancia_2` | 1,021 | 1,021 | 0 | 972 | 40,860 | 687 | 0 |
| `l12u_control_monolith` | 708 | 708 | 0 | 344 | 28,340 | 0 | 0 |
| `mp_military_2` | 790 | 790 | 0 | 637 | 31,620 | 602 | 0 |
| `testers_mp_agroprom` | 190 | 190 | 0 | 178 | 7,620 | 162 | 0 |
| `testers_mp_atp` | 321 | 321 | 0 | 324 | 12,860 | 325 | 0 |
| `testers_mp_darkvalley` | 88 | 88 | 0 | 80 | 3,540 | 52 | 0 |
| `testers_mp_factory` | 161 | 161 | 0 | 208 | 6,460 | 194 | 1 |
| `testers_mp_lost_village` | 922 | 922 | 0 | 662 | 36,900 | 360 | 0 |
| `testers_mp_military_1` | 301 | 301 | 0 | 393 | 12,060 | 393 | 0 |
| `testers_mp_railroad` | 836 | 836 | 0 | 770 | 33,460 | 686 | 0 |
| `testers_mp_rostok` | 768 | 768 | 0 | 536 | 30,740 | 423 | 2 |
| `testers_mp_workshop` | 1,093 | 0 | 1,093 | 890 | 43,740 | 785 | 2 |

## 5. Unestablished

- The behaviour for a chunk 1 whose size is not a multiple of 40 (section 2).
- Which triangles are linked as neighbours across an edge shared by three or more triangles (section 3,
  step 2); it affects the 66 such edges of section 4 only. **Disputed in review:** established (section 3,
  step 2).

## 6. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); archives `gamedata.db0` …
  `gamedata.dbd` and `mods\mp_military_2.xdb0`; binaries `bin\xrRender_R1.dll`, `bin\xrCDB.dll`.
- Data examined: all 26 `levels\*\level.hom` files and the list of level directories (`level.cform`) of all
  archives.
- Analysis: structural and statistical checks over the files (chunk layout, record values, a reimplementation
  of the merging and edge matching described here); static analysis of the shipped binaries (headless
  decompilation, read-only; constants read from the binaries' data).

## Gaps found in review

- A file without chunk 1, or with an empty chunk 1, is not covered: the reader uses the chunk without testing
  whether it was found (**believed**: the game crashes).
