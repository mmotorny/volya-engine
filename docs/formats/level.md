# `level` file (level directory main file)

Format specification of the file `levels\<name>\level` of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): header, shader/texture table, static visuals, portals, lights, glows and sectors, as needed to
show the level's static world with the game's first renderer. The visuals themselves (chunk 3) are specified
in `ogf.md`; the vertex and index buffers they reference in `level-geom.md`; the chunk container in
`chunks.md` (scoping specification `level-loading.md`, section 5.1).

Conventions:

- Numbers are little-endian; `u8`/`u16`/`u32` unsigned, `i32` signed, `f32` IEEE-754 single precision.
- `float3` = 3 × f32 (x, y, z); world coordinates are the game's (y is up, as the sun direction and the
  downward sector ray below show).
- "Chunk" = the container of `chunks.md`: `u32 id`, `u32 size`, `size` bytes of payload, repeated without
  padding. A chunk is found by its id; the order of chunks in the file has no meaning.
- Strings are zero-terminated, Windows-1251.
- "Visual *i*" = the visual stored in sub-chunk *i* of chunk 3.

## 1. Top-level chunks

| id | Content | Required by the first renderer |
|---|---|---|
| 1 | header (section 2) | yes |
| 2 | shader/texture table (section 3) | yes; absent is fatal ("Level doesn't builded correctly.") |
| 3 | visuals (section 4) | yes |
| 4 | portals (section 5) | yes (may be empty) |
| 6 | lights (section 7) | yes |
| 7 | glows (section 8) | yes; must exist and be non-empty |
| 8 | sectors (section 6) | yes |

No other top-level ids occur in any shipped level. In every shipped level the chunks are stored in the order
1, 4, 6, 7, 3, 2, 8.

## 2. Header (chunk 1, 4 bytes)

| Offset | Type | Meaning |
|---|---|---|
| 0 | u16 | format version; must be 14 (any other value is fatal) |
| 2 | u16 | meaning **unestablished**; 1 or 2 in the shipped levels (2 in `l08u_brainlab`, `l10u_bunker`, `l12_stancia_2`, `l12u_control_monolith`, `l12u_sarcofag`, `testers_mp_bath`, `testers_mp_lost_village`; 1 in the other 23). Not needed to show the static world |

## 3. Shader/texture table (chunk 2)

```
u32     count
count × string
```

The strings fill the chunk exactly. Entry *k* (0-based) is the "shader index" *k* used by visual headers
(`ogf.md`) and glows (section 8).

- An empty string means "no shader": the index is valid but nothing is created for it. Entry 0 is empty in
  every shipped level and no other entry is empty. Index 0 is used only by group visuals (type 1), which
  draw nothing themselves.
- A non-empty entry has the form `<engine shader>/<texture list>`: it is split at the **first** `/`. Every
  non-empty entry in the shipped levels contains a `/`.
  - `<engine shader>` is the name of a shader definition (scripted `shaders\r1\*.s` or a `shaders.xr`
    definition; resolution per the scoping specification section 5.13 and the shader specification), e.g.
    `default`, `def_shaders\def_vertex`, `levels\escape_grass`, `details\lod`, `effects\glow`.
  - `<texture list>` is the rest of the string, passed unchanged to that shader definition. It is a
    comma-separated list of texture names without extension, e.g. `mtl\mtl_bochka_02,lmap#5_1,lmap#5_2`
    (base texture, then the two light-map pages), `terrain\terrain_escape,terrain\terrain_escape_lm`,
    `level_lods`. How each named texture is used is for the shader specification. Each name resolves
    to a file by the texture name resolution of the scoping specification (section 4): `$level$<name>.dds`
    first (the level directory), then the saved-games folder, then `$game_textures$<name>.dds`.
- Each entry is created once, when the level is loaded, in table order. No two entries of a shipped
  level are equal.

## 4. Visuals (chunk 3)

Chunk 3 is a sequence of sub-chunks with ids 0, 1, 2, … *n*−1 (consecutive, in that order; loading stops at
the first missing id). Sub-chunk *i* is visual *i*, a chunk sequence in the visual structure of `ogf.md`
(header in its chunk 1; the visual type is the header's byte 1).

Visuals refer to other visuals only by index into this list (group children, `ogf.md` section 4), and in
every shipped level a child's index is smaller than its parent's index, so visuals can be decoded in index
order with all children already present. Every visual has at most one parent. The visuals that are no
visual's child are exactly the sector root visuals (section 6), one per sector. In `l01_escape` all roots are groups (type 1); over all
shipped levels 707 of the 709 roots are groups, one is a mesh (type 0) and one a progressive mesh (type 2).

## 5. Portals (chunk 4)

The chunk size is a multiple of 80 (checked; otherwise fatal); it may be 0 (three shipped levels have no
portals and a single sector). Records of 80 bytes:

| Offset | Type | Meaning |
|---|---|---|
| 0 | u16 | sector A (index into chunk 8) |
| 2 | u16 | sector B (index into chunk 8) |
| 4 | 6 × float3 | polygon vertices; only the first *count* are used |
| 76 | u32 | *count*, number of vertices (3 … 6 occur) |

Unused vertex slots hold arbitrary values. The polygon is convex and planar (by data).

Derived values (used by the visibility rules of section 9):

- **Plane.** For *i* = 1 … *count*−2 form the triangle normal **t**ᵢ = normalize((**v**ᵢ − **v**₀) × (**v**ᵢ₊₁ − **v**ᵢ)),
  skipping triangles whose cross product has length ≤ 1e−7 (if none remains the portal is invalid: fatal
  "Invalid portal detected"). The plane normal **n** is the normalized average of the **t**ᵢ, and
  *d* = −**n**·**v**₀. Sector A lies on the side **n**·**p** + *d* > 0, sector B on the other side (by data:
  for 1,288 of the 2,100 portals of all levels only A's root bounding-sphere centre is on the positive side,
  for 3 only B's).
- **Bounding sphere.** Centre **c** = midpoint of the axis-aligned box of the *count* vertices; radius =
  distance from **c** to that box's maximum corner.

Every portal is listed in the portal lists (section 6) of both its sectors.

## 6. Sectors (chunk 8)

Chunk 8 is a sequence of sub-chunks with ids 0, 1, 2, … (consecutive; loading stops at the first missing
id). Sub-chunk *s* is sector *s* and is itself a chunk sequence:

| Sub-chunk id | Content |
|---|---|
| 1 | portal list: *size*/2 × u16, indices into chunk 4. Size must be even (checked). May be empty |
| 2 | root visual: one u32, index into chunk 3. Size must be 4 (checked) |

The root visual of a sector holds (through its group hierarchy) all static visuals of that sector; every
visual of chunk 3 is reachable from exactly one sector root.

## 7. Lights (chunk 6)

Records of 108 bytes (chunk size / 108 records):

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | 1 for the sun record, 2 for all others in the shipped levels; not used by the first renderer (meaning **unestablished**) |
| 4 | 104 bytes | a light in the layout of Direct3D 9's public `D3DLIGHT9` structure: u32 type (1 point, 2 spot, 3 directional), float4 diffuse (r, g, b, a), float4 specular, float4 ambient, float3 position, float3 direction, f32 range, f32 falloff, f32 attenuation0, attenuation1, attenuation2, f32 theta, f32 phi |

Exactly one record per shipped level has type 3 (directional): the sun. A level without a directional light
is fatal ("Where is sun?"). All other records of the shipped levels have type 1 (point).

The first renderer uses only these fields: type; for the sun its direction (record offset 68); for a
point light its diffuse colour (record offset 8), position (56) and range (80). All other fields hold
arbitrary values in the shipped data (often denormal or garbage bit patterns) and must be ignored. These
lights are not part of the stored static lighting (which is in light maps and vertex data, see the shader
specification); they serve dynamic lighting of objects, out of scope for the static world. How the sun
record relates to the environment's sun is for the environment specification (**unestablished** here).

## 8. Glows (chunk 7)

The chunk must exist, have a non-zero size and a size that is a multiple of 18 (each checked; fatal
otherwise). Records of 18 bytes:

| Offset | Type | Meaning |
|---|---|---|
| 0 | float3 | position (world) |
| 12 | f32 | radius |
| 16 | u16 | shader index into chunk 2 |

In the shipped levels 1,165 of the 1,166 glows use an entry with engine shader `effects\glow`, one
uses `effects\flare`. Drawing of glows
(fading, occlusion tests) is for a later specification.

## 9. Which visuals the first renderer draws from a camera position

This section states, as observable rules, how the stored data selects what is drawn for one frame. Rules
whose inputs are out of scope are named as such.

### 9.1 Camera sector

A vertical ray is cast from the camera position straight down (direction (0, −1, 0)), at most 500 units
long, against both the portal polygons (section 5) and the level's collision triangles (`level.cform`).
The nearest hit decides:

- a collision triangle: the camera is in that triangle's sector (the collision triangle record carries a
  sector index; for `l01_escape` the u16 at byte 14 of each 16-byte triangle record of `level.cform` takes
  exactly the values 0 … 23, one per sector; the field belongs to the collision specification);
- a portal (taken when no triangle is nearer than the portal by more than 1e−5): the camera is in sector A
  of that portal if **n**·**p** + *d* > 0 for the camera position **p**, else in sector B;
- nothing within 500 units: the camera sector of the previous frame is kept.

The sector is re-determined only when the camera has moved (by more than 1e−7 on any axis). **Disputed in
review:** the condition is a move by at least 1e−7 on some axis (|Δ| ≥ 1e−7).

There is **no camera sector** after a level is loaded (and after it is unloaded) until a ray finds one; a ray that
finds nothing leaves it none. When a level is unloaded the remembered camera position is set to the largest
single-precision value, so the first visibility pass after the next load casts the ray. While there is no camera
sector the first renderer's visibility pass does nothing of section 9.2 – 9.3 and gathers no dynamic item: no
sector is traversed, no static visual, object, effect object, glow or light is drawn or listed, and the game's two
HUD callbacks (which draw the first-person items, `docs/behavior/light-shadows-projection.md` 3.2) are not called;
the occlusion grid is still drawn (`docs/behavior/visibility-scope.md` 6, `docs/behavior/object-visibility.md` 1).

### 9.2 Sector traversal through portals

Starting from the camera sector with the full view frustum, a sector is entered through each of its portals
unless one of the following holds (rules applied in this order):

1. The portal was already passed this frame.
2. Target sector: for a portal that the camera touches (an axis-aligned box of half-size 0.002 around the
   camera intersects the portal polygon) the target is the portal's other sector than the current one;
   otherwise the target is sector A if **n**·**p** + *d* ≤ 0, else sector B — and the portal is skipped if
   that target is the current sector or the sector the current one was entered from.
3. The portal's bounding sphere is outside the current frustum.
4. Portal size: with *r* the portal radius, *D* the distance from the camera to the portal centre and
   cos the absolute cosine between **n** and the direction from the camera to the centre, the portal's
   screen measure is *m* = *r*² / *D*² × cos. The portal is skipped if *m* < *T*\_min, and also if
   *m* < *T*\_B (thresholds in section 9.4). Portals with *m* < *T*\_A are additionally drawn as a
   darkening fade (not part of the stored world; out of scope).
5. The portal polygon, clipped against the current frustum, is empty.
6. The portal is hidden by the occlusion mesh `level.hom`: the clipped portal polygon of rule 5 is tested by the
   polygon test of `docs/behavior/occlusion-tests.md` 2.3 (not cached; visible without a test while occlusion is
   off, which it is for a level without `level.hom`, `occlusion-tests.md` 4); this rule is applied by the camera's
   traversal only (`docs/behavior/traversal-options.md` 4). A renderer without occlusion culling draws a
   superset.

The target sector is then visited with the current frustum narrowed to the clipped portal polygon (the
planes through the camera and the polygon's edges), and the same rules apply recursively from it. A sector
may be reached through several portals; it keeps every frustum it was reached with.

A renderer that ignores sections 9.1 and 9.2 and visits every sector with the full view frustum draws a
superset of what the original draws (the original draws no static visual outside the visited sectors).

### 9.3 Visuals of a visited sector

For each frustum a sector was reached with, its root visual is tested and descended (the visual hierarchy,
types and LOD selection are in `ogf.md` section 8). A visual reached twice in one frame is drawn once.

### 9.4 Screen thresholds

The size thresholds are derived once per frame from the screen and three console settings:

- *S* = (`r__geometry_lod` + 1e−7) × (90 / *fov*)² × *W* × *H*, where *W* × *H* is the render-target
  size in pixels and *fov* the **vertical** field-of-view angle in degrees of the camera the frame is drawn with
  (`docs/behavior/view-projection.md` 1 – 3; during play 67.5, smaller while aiming). `r__geometry_lod` defaults
  to 1.0 (range 0.1 … 1.2).
- *T*\_min = 3.5² / *S* = 12.25 / *S*.
- *T*\_A = (`r1_ssa_lod_a` / 3)² / *S*, *T*\_B = (`r1_ssa_lod_b` / 3)² / *S*; `r1_ssa_lod_a` defaults to 64
  (range 16 … 96), `r1_ssa_lod_b` to 48 (range 16 … 64).
- *P*\_lo = (16 / 0.75)² / *S* ≈ 455.11 / *S* and *P*\_hi = (64 / 0.75)² / *S* ≈ 7281.78 / *S*
  (progressive-mesh detail, `ogf.md` section 8.3).

## 10. Verification facts

`levels\l01_escape\level` (4,094,604 bytes):

- Header (14, 1). Chunk sizes: 1: 4, 2: 17,864, 3: 4,065,388, 4: 5,520, 6: 4,428, 7: 396, 8: 948.
- Chunk 2: 396 entries, entry 0 empty, 395 non-empty, 18 distinct engine shaders;
  entry 1 is `effects\glow/glow\glow_04`, entry 395 is
  `def_shaders\def_objects_lod/act\act_monolit_corp`; the 395 texture lists name 917 textures in total
  (counting repeats).
- Chunk 3: 11,538 visuals.
- Chunk 4: 69 portals with 4 (63), 5 (1) and 6 (5) vertices. Portal 0: sectors A = 16,
  B = 23, plane normal (−1, 0, 0), *d* = −215.882, bounding-sphere centre (−215.882, −18.394, −167.358),
  radius 0.881.
- Chunk 8: 24 sectors; root visuals 9621, 9622, 9626, 9631 … 9643 (13
  consecutive), 9647, 9648, 9649, 9652, 9656, 9662, 9665, 11537 (in sector order); 138 portal-list entries in
  total (each portal twice); sector 0 lists portals 25 and 57.
- Chunk 6: 41 lights (1 directional with direction (0.840, −0.423, 0.340), 40 point).
- Chunk 7: 22 glows with shader indices 1 (12 glows), 2 (4), 3 (6), all `effects\glow`;
  sum of radii 17.4; glow 0 at (30.484, 19.756, 679.066), radius 1.0.

All 30 shipped levels (`levels\*\level`, archives db0 … db4):

Per level (header second u16, chunk-2 entries, visuals, portals, sectors, lights, glows; the last two
columns are defined in `ogf.md` section 9):

| Level | hdr u16 | shader entries | visuals | portals | sectors | lights | glows | triangles (full detail) | distinct geometry refs |
|---|---|---|---|---|---|---|---|---|---|
| `l01_escape` | 1 | 396 | 11,538 | 69 | 24 | 41 | 22 | 3,047,891 | 3,448 |
| `l02_garbage` | 1 | 290 | 7,663 | 0 | 1 | 19 | 2 | 1,955,697 | 2,071 |
| `l03_agroprom` | 1 | 469 | 7,148 | 130 | 33 | 43 | 41 | 1,952,857 | 2,481 |
| `l03u_agr_underground` | 1 | 168 | 1,085 | 26 | 18 | 99 | 92 | 603,148 | 899 |
| `l04_darkvalley` | 1 | 459 | 8,471 | 242 | 55 | 75 | 41 | 1,906,791 | 3,041 |
| `l04u_labx18` | 1 | 158 | 1,255 | 32 | 31 | 131 | 130 | 551,673 | 1,076 |
| `l05_bar` | 1 | 428 | 6,108 | 131 | 27 | 112 | 99 | 1,473,702 | 3,208 |
| `l06_rostok` | 1 | 430 | 5,928 | 27 | 9 | 26 | 15 | 1,347,420 | 3,241 |
| `l07_military` | 1 | 522 | 6,638 | 159 | 50 | 71 | 40 | 2,530,087 | 3,200 |
| `l08_yantar` | 1 | 394 | 5,914 | 69 | 26 | 73 | 10 | 1,509,536 | 2,297 |
| `l08u_brainlab` | 2 | 274 | 1,352 | 29 | 21 | 112 | 101 | 497,277 | 1,095 |
| `l10_radar` | 1 | 324 | 9,885 | 80 | 23 | 53 | 24 | 3,224,640 | 2,804 |
| `l10u_bunker` | 2 | 276 | 1,889 | 32 | 27 | 123 | 112 | 839,686 | 1,644 |
| `l11_pripyat` | 1 | 425 | 14,446 | 419 | 159 | 129 | 7 | 3,242,330 | 6,470 |
| `l12_stancia` | 1 | 245 | 7,275 | 1 | 2 | 13 | 11 | 1,970,434 | 4,367 |
| `l12_stancia_2` | 2 | 374 | 9,696 | 0 | 1 | 39 | 9 | 2,186,584 | 7,519 |
| `l12u_control_monolith` | 2 | 144 | 948 | 45 | 27 | 79 | 77 | 402,434 | 829 |
| `l12u_sarcofag` | 2 | 210 | 1,820 | 29 | 25 | 112 | 98 | 788,351 | 1,552 |
| `testers_mp_agroprom` | 1 | 172 | 2,607 | 19 | 4 | 15 | 14 | 640,689 | 1,025 |
| `testers_mp_atp` | 1 | 160 | 1,697 | 32 | 7 | 13 | 1 | 601,319 | 595 |
| `testers_mp_bath` | 2 | 206 | 1,583 | 43 | 11 | 27 | 26 | 392,116 | 1,013 |
| `testers_mp_darkvalley` | 1 | 174 | 1,813 | 72 | 13 | 37 | 16 | 401,624 | 928 |
| `testers_mp_factory` | 1 | 229 | 1,748 | 0 | 1 | 13 | 1 | 391,109 | 1,175 |
| `testers_mp_lost_village` | 2 | 207 | 1,949 | 93 | 30 | 17 | 12 | 532,325 | 1,216 |
| `testers_mp_military_1` | 1 | 225 | 1,699 | 58 | 23 | 37 | 28 | 531,828 | 1,137 |
| `testers_mp_military_2` | 1 | 175 | 1,327 | 32 | 4 | 6 | 6 | 519,073 | 624 |
| `testers_mp_pool` | 1 | 75 | 276 | 34 | 22 | 53 | 48 | 63,865 | 226 |
| `testers_mp_railroad` | 1 | 262 | 3,248 | 53 | 13 | 10 | 1 | 909,140 | 2,143 |
| `testers_mp_rostok` | 1 | 240 | 4,568 | 63 | 8 | 61 | 40 | 1,244,000 | 2,069 |
| `testers_mp_workshop` | 1 | 234 | 2,092 | 81 | 14 | 43 | 42 | 498,977 | 1,328 |

Invariants holding in every level: header version 14; top-level ids exactly {1, 2, 3, 4, 6, 7, 8};
shader entry 0 empty and every other entry non-empty, containing `/`, and unique; portal chunk a
multiple of 80 and glow chunk a non-empty multiple of 18; every portal listed by both its sectors; sector
roots = the parentless visuals; exactly one directional light, all others point lights. Totals: 2,100
portals, 709 sectors, 1,682 lights, 1,166 glows.

## 11. Unestablished

- Meaning of the header's second u16 and of the lights' leading u32.

## 12. Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe` and `bin\xrRender_R1.dll`.
- Data examined: the `level`, `level.geom` and `level.geomx` files of all 30 level directories (merged
  archive view, later archive wins), `levels\l01_escape\level.cform`, the game's own vertex programs in
  `shaders\r1\` and `shaders\shared\common.h`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation) to establish
  what the first renderer reads and checks and how it uses each field; statistical checks over the data of
  all levels (index ranges, uniqueness, side-of-plane tests, value ranges); a throwaway parser written only
  from this text and `ogf.md`, run over all 30 levels, reproduces every number in section 10.
- Section 9 revised 2026-11-25 (field of view, no camera sector, portal occlusion) by further static analysis of
  `bin\XR_3DA.exe` and `bin\xrRender_R1.dll`.

## Gaps found in review

- Section 9.2 rule 4 still calls the darkening fade of small portals out of scope; it is specified in
  `docs/behavior/portal-fade.md`, which section 9 does not reference.
