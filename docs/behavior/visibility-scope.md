# Visibility in the first renderer (scoping specification)

Scoping specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of how the first
(static-lighting) renderer `bin\xrRender_R1.dll`, the one the game uses at its default settings, decides
each frame which parts of the level and which objects it does not draw because they cannot be seen.
Section 1 maps the area and what approved specifications already state; sections 2 – 8 give, per topic,
what is established so far; section 9 proposes a split into small specifications, one format or one
behaviour each, with scope, size and order; section 10 lists what is unestablished. It states behaviour
only as far as needed to fix scope.

Builds on (not repeated): `docs/formats/level.md` (here "LV": portals section 5, sectors section 6,
the visibility rules section 9), `docs/formats/ogf.md` (here "OGF", section 8), `docs/formats/level-cform.md`,
`docs/behavior/level-loading.md` (section 5.6), `docs/behavior/glows.md` (here "GL"),
`docs/behavior/dynamic-lights.md` (here "DL"), `docs/behavior/particle-drawing.md` (here "PD", section 7.1),
`docs/behavior/detail-objects.md` (here "DO", section 5.2), `docs/behavior/object-detail-level.md`
(here "ODL"), `docs/behavior/r1-settings-scope.md` (here "RS"), `docs/behavior/static-shading.md`.

Conventions: "the camera" is the frame's eye position **E**; "view depth" is the distance from **E** along
the view direction. The **view-projection** is the frame's combined view and projection matrix; for a point it
gives clip coordinates (x, y, z, w) with w the view depth. "Visited sector" and "the frusta a sector was
reached with" are those of LV 9.2. Every fact carries the id of the check that reproduces it; the checks
are scripts in `checks/visibility-scope/` of the dirty workspace (`run.py` runs them all): `hom_data.py`
[H…], `data_sectors.py` [S…], `data_cfg.py` [C…] read the game data (merged archive view); `binary_r1.py`
[B…] and `binary_exe.py` [E…] read the shipped binaries by static analysis (read-only decompilation; outputs
kept in `out/`); `model.py` [M…] is arithmetic. All checks pass.

## 1. The area

Per frame the renderer runs one **visibility pass** for the camera. In order [B2, B7, B8, B9]:

1. It derives the size thresholds (LV 9.4, ODL 1) and builds the **view frustum** from the
   view-projection (section 4).
2. If the level has an occlusion mesh, it draws the mesh's triangles into a small depth grid (section 3).
3. It finds the **camera sector** (LV 9.1).
4. If there is a camera sector: it traverses sectors through portals (LV 9.2), gathers the static visuals
   of every visited sector (LV 9.3, OGF 8), and gathers the dynamic items returned by a spatial query
   (objects, lights, glows; section 5). Without a camera sector it gathers nothing (section 6).

Separate passes, specified elsewhere, make their own visibility decisions: detail objects (DO 5.2),
dynamic-light passes (DL 5.3 – 5.4), projected object shadows (`docs/behavior/light-shadows-projection.md`).

| Topic | Already stated in | Section here |
|---|---|---|
| Sectors, portals: format | LV 5, 6 | 2 |
| Camera sector | LV 9.1 | 2, 6 |
| Traversal through portals, frustum narrowing, portal size rule | LV 9.2, 9.4 | 2 |
| Portal fade (darkening) | named only (LV 9.2 rule 4; PD 7.4 "believed") | 2 |
| Occlusion mesh `level.hom` and its tests | named only, "out of scope" (LV 9.2, OGF 8.1, DL 4, DO 5.2, PD 7.1) | 3 |
| View frustum, near and far planes, field of view | partly (DO 5.2 planes; RS 2.2 far plane; LV 11 fov unestablished) | 4 |
| Static visuals: frustum and size rejection | LV 9.3, 9.4; OGF 8 | 4 |
| Objects: sector, frustum, occlusion | partly (PD 7.1; ODL for sizes) | 5 |
| Lights, glows, particles, detail objects | DL 2.4, 4; GL 3, 5; PD 7.1; DO 5.2 | 5 |
| Edge cases | partly (LV 9.1, 9.2 rule 2) | 6 |
| Settings | RS (table) | 7 |

## 2. Spatial subdivision and traversal

- The level's subdivision for visibility is its sectors (`level` chunk 8) joined by convex portal polygons
  (chunk 4) (LV 5, 6). The camera's sector comes from a downward ray against portals and collision triangles
  (LV 9.1). These are established and approved.
- 709 sectors in all 30 levels [S1]. Three levels (`l02_garbage`, `l12_stancia_2`, `testers_mp_factory`)
  have one sector and no portals [S1]; a level without portals also has no portal geometry for the
  camera-sector ray, which then uses only collision triangles [B8]. Seven sectors in five levels with several
  sectors have no portal at all: one each in `l04u_labx18`, `l05_bar`, `l08u_brainlab`, `l10u_bunker`, three
  in `l12u_control_monolith` [S1]; by the traversal rules (LV 9.2) such a sector is visited only when the
  camera is in it, and from it no other sector is reached.
- The camera pass traverses with three of the traversal's four options on: the portal size rule, the
  collection of portals for the fade, and the occlusion test of the clipped portal polygon (LV 9.2 rule 6);
  the fourth option (a screen-rectangle mode) is off in the camera pass [B7]. Which options the light passes
  use, and what the fourth does, belongs to the traversal's specification (V8, section 9).
- The portal fade: a portal whose screen measure is below *T*\_A is collected for a darkening fade [B7]
  (LV 9.2 rule 4). How it is drawn is **unestablished** (spec V9).

## 3. Occlusion

### 3.1 The occlusion mesh `level.hom`

- Read from the level directory when the level loads; when the file is missing a warning is logged and the
  level has no occlusion [B1]. 25 of the 30 level directories have the file; `l10u_bunker`, `l12u_sarcofag`,
  `testers_mp_bath`, `testers_mp_military_2`, `testers_mp_pool` do not [H1].
- Chunked (`chunks.md`): chunk 0 holds a u32 = 0 in every file; chunk 1 is a sequence of 40-byte records:
  three float3 vertices, then a u32 [H2, B1]. No other chunks occur [H2]. 17,583 records in all (904 in
  `l01_escape`) [H3].
- The u32 is 0 or 1 in every record (16,118 are 1, 1,465 are 0) [H3]. With 1 the triangle occludes from
  both sides; with 0 only while the camera is on its front side, the side of the normal
  normalize((**v**₀ − **v**₁) × (**v**₀ − **v**₂)), plane offset *d* = −**n**·**v**₀, front when
  **n**·**E** + *d* > 0 [B1, B3].
- At load, vertices closer than 0.01 are merged and the triangles' neighbours are found; triangles with area
  below 0.001 are reported (no shipped triangle is) [B1, H3]. Whether merging changes any shipped triangle
  is **unestablished**.

### 3.2 The depth grid

- When the level has an occlusion mesh, occlusion is on for the pass; the grid is drawn at the start of
  each visibility pass [B2]. A test made before a pass has drawn the grid draws it first, except while a
  game-wide state reports itself active (**believed** the main menu: which state it is is **unestablished**)
  [B2].
- The grid is 64 × 64 cells over the screen (**believed**: x and y mapped from −1 … 1 to 0 … 64), held in a 68 × 68
  buffer; coarser grids of 32, 16 and 8 cells are reduced from it by taking the farthest of each 2 × 2
  [B3, B4, M2]. Depths are clamped to −1.99 … 1.99 and stored as fixed-point (× 2³⁰) [B4]. What the
  coarser grids are used for is **unestablished**.
- Triangles drawn: those of the mesh that meet the view frustum, sorted by a measure of distance from the
  camera (the exact key and direction are **unestablished**), each clipped at the plane w + z = 0 of the view-projection only [B3, E4]; with the near plane 0.2 that is
  view depth 0.2 · *f* / (2*f* − 0.2) ≈ 0.1 (*f* the far plane) [M1]. A triangle that is back-facing (u32 = 0),
  clipped away, or covers no cell is skipped for the next 3 … 9 frames (a random count) [B3].
- How a triangle is rasterised (cell coverage, depth per cell, the use of neighbour triangles at shared
  edges) is **unestablished** (spec V3).

### 3.3 The tests

- Three shapes are tested against the 64-cell grid: an axis-aligned box (by its 8 corners), a polygon (the
  clipped portal), and a screen rectangle given directly with a depth [B5]. A box or polygon with any corner at w < 10⁻⁵ is
  visible without a test [B5]. **Disputed in review:** the coordinate compared with 10⁻⁵ is the corner's clip-space
  depth z (the third clip coordinate), not w; with the projection's depth mapping (section 4) a corner is
  untested-visible when it lies nearer than about the near plane (view depth below ≈ 0.2) or behind the eye.
  The shape's depth used in the test is the smallest z / w of its corners. Otherwise the shape's screen rectangle (clamped to cells 0 … 63) is visible
  when any cell in it stores an occluder depth beyond the shape's nearest depth [B5]. The exact depth
  comparison and the rounding of rectangle edges are **unestablished** (spec V4).
- Results are cached per item: after a visible result the next test is due after 10 + (a random 0 … 14)
  frames, after a hidden result in the next frame; between tests the item counts as visible [B6]. Items
  cached this way: static visuals [B10], objects [B9], lights [B9] (DL 4), detail-object groups [B10]
  (DO 5.2). The box test reached from outside the renderer treats a box that contains the camera as
  visible [B6].
- Which items use which shape, and whether the random counts come from one shared generator, belong to
  spec V4 (DO 5.2 states a shared generator for detail objects).

## 4. View volume, near and far planes, size rejection

- The view frustum of the camera pass has the left, right, top, bottom and far planes and **no near plane**
  [B7, E4] (as DO 5.2 states for detail objects).
- Projection: the vertical scale is 1 / tan(*fov* / 2), *fov* in degrees, the horizontal scale
  *a* / tan(*fov* / 2) with *a* = (render height / render width) × a per-camera aspect factor [E1, E2].
  So *fov* is the **vertical** angle (settles LV 11 and LV 9.4); the per-camera factor's values are
  **unestablished** (believed 1 for the player's view). Depth maps *f* / (*f* − *n*) with near *n* and far *f*
  in the usual way [E1].
- The engine's camera starts with *fov* 90 and far 100 [E2]; the renderer reads the current *fov* each frame
  for its thresholds [E1]. The near plane is 0.2 wherever the engine or the game applies a camera, except at
  one place in the game that passes a computed value (**unestablished** which camera that is) [E3].
- The far plane is the weather's far value × `rs_vis_distance` (RS 2.2); the player's *fov* during play,
  and its change while aiming (weapons set `scope_zoom_factor`, 38 keys in 35 files, values 7.5 … 55,
  commented as the zoom field of view [C1]), are **unestablished** here (spec V5). A smaller *fov* raises
  *S* (LV 9.4) and so lowers every size threshold.
- Static geometry: portal size rule and visual size thresholds (LV 9.2 rule 4, 9.4; OGF 8); frustum tests
  of visuals by sphere and box (OGF 8.1) [B10]. Dynamic objects: their models' small-part tests (ODL 3).

## 5. Dynamic items

- The camera pass queries the engine's spatial index with the view frustum for renderable items and lights,
  and takes the results in order of distance (GL 3) [B9]. Each item: its sector is re-determined if it moved
  (from its sector point by the LV 9.1 rule; by default the sector point is the item's bounding-sphere
  centre; the sector is kept when none is found) [B9, E5]; the item is skipped unless its sector was visited
  this frame and its bounding sphere is not outside one of the frusta that sector was reached with [B9].
- Renderable objects are then occlusion-tested (cached) with the axis-aligned box of their visual's bounding
  box under the object's transform, and drawn [B9]. Particle effects are renderable objects (PD 7.1).
  Which items override the sector point (e.g. characters) is **unestablished** (spec V6).
- Lights: same sector and frustum rule, then the occlusion test of their box, then listing (DL 4) [B9].
- Glows: gathered in the same loop (GL 3), occluded by rays rather than by the grid (GL 5) [B9].
- Detail objects: own frustum test and cached occlusion test (DO 5.2) [B10].

## 6. Edge cases

- **No camera sector.** The camera sector is none after a level loads or unloads until the first ray that
  finds one [B8]; when a later ray finds nothing the previous sector is kept (LV 9.1) [B8]. While it is
  none the pass gathers no static visual, object, light or glow [B8] (what else the frame draws, e.g. the
  sky, is outside this pass; **believed** drawn as usual).
- **Camera outside every sector after being inside:** the last sector stays the camera sector, and the
  traversal starts from it (LV 9.1) [B8].
- **Camera at or inside geometry.** With no near plane in the view frustum nothing is rejected for being
  too close by the pass itself; the projection's near plane 0.2 still clips what is drawn (**believed**:
  standard clipping of the graphics interface). A portal the camera touches is handled by LV 9.2 rule 2.
  A box with a corner behind the eye (w < 10⁻⁵) is never hidden by occlusion, so a box around the camera
  always passes [B5, B6]; **Disputed in review:** the condition is a corner with clip-space z < 10⁻⁵, i.e. nearer
  than about the near plane 0.2 or behind the eye (section 3.3); the conclusion for a box around the camera holds; occluder triangles are clipped at view depth ≈ 0.1 [M1].
- **Sectors without portals** and one-sector levels: section 2 [S1].
- **Levels without `level.hom`:** no occlusion at all [B1, H1].

## 7. Settings

| Setting | Default | Effect on visibility |
|---|---|---|
| `r__geometry_lod` | 1.0 | size thresholds *S* (LV 9.4, ODL 1) |
| `r1_ssa_lod_a`, `r1_ssa_lod_b` | 64, 48 | portal fade and portal size rule, billboard cross-fade (LV 9.4) |
| `rs_vis_distance` | RS | far plane (RS 2.2) |
| `r1_dlights` | DL 8 | lights listed (DL 4) |
| `r1_glows_per_frame` | 16 | glow ray tests per frame (GL 5) |
| `r__detail_density` | DO 7 | detail objects (DO 7) |
| field of view (game camera, weapon zoom) | 90 at engine start [E2] | thresholds *S*; projection (section 4) |

The pass's two internal switches for static geometry and for dynamic items are on by default and no console
command changes them [E6]; only the demo recorder's level-map capture mode replaces the switch word (it leaves
static geometry on and dynamic items off) [E6]. The first renderer has no command-line switch and no console
variable for occlusion or portals [B11, E6]: occlusion is on exactly when `level.hom` exists.

## 8. What the original draws

A renderer that ignores sectors, portals and occlusion and keeps the frustum and size rules draws a superset
of what the original draws (LV 9.2, OGF 8.1); occlusion only removes, it never adds. The exceptions to
"superset" are the cases of section 6 where the original draws nothing (no camera sector) and the portal
fade, which adds darkening geometry (section 2). **Disputed in review:** drawing nothing when there is no camera
sector is not an exception to "superset" (any image is a superset of nothing); it is the case where the two
differ most. The only exception named here is the portal fade.

## 9. Proposed specifications

| # | Name | Kind | Scope | Builds on | Size |
|---|---|---|---|---|---|
| V1 | `level-hom` | format | `level.hom`: chunks, record, the u32 (two-sided), load-time merging and checks, data facts of all 25 files | 3.1 | small |
| V2 | `view-projection` | behaviour | projection from *fov*, aspect, near, far; the frustum planes of each pass; *fov* sources during play (player camera, weapon zoom, other cameras); the computed near value | 4 | small |
| V3 | `occlusion-grid` | behaviour | drawing the occluders each frame: selection, order, clipping, facing, rasterisation, depth encoding, neighbour edges, skip counts, coarser grids, when it is (re)drawn | 3.2, V1, V2 | medium |
| V4 | `occlusion-tests` | behaviour | box, polygon, rectangle tests; depth comparison; caching and its random counts; which item uses which test | 3.3, V3 | small |
| V5 | targeted fix of `level.md` 9 | format | *fov* is vertical (LV 11); camera sector none after load and its effect; portal occlusion rule pointing to V4 | 4, 6 | small |
| V6 | `object-visibility` | behaviour | the camera pass's dynamic items: spatial query, sector point per item kind, sector and frusta test, occlusion box, order, the per-frame round-robin item | 5, V4 | small |
| V7 | targeted fixes | — | replace "out of scope" occlusion statements in OGF 8.1, DL 4, DO 5.2, PD 7.1 by references to V4 | V4 | small |
| V8 | traversal options | behaviour | the four traversal options and which pass uses which (camera, lights, shadows) | 2, LV 9.2 | small |
| V9 | `portal-fade` | behaviour | the darkening fade of small portals: what is drawn, colour and order | 2, LV 9.2 | small |

Order: V1, V2 first (independent); V3 after V1 and V2; V4 after V3; V5 after V2 and V4; V6 after V4; V7 after
V4; V8 and V9 independent. A renderer needs none of V1 – V4 to draw a correct image (section 8); they matter
for matching the original's cost and its few visible occlusion effects.

## 10. Unestablished

- Which game-wide state suppresses drawing the occlusion grid early (believed: the main menu) (3.2).
- Rasterisation details of the grid and the use of the coarser grids (3.2, V3).
- The exact depth comparison and rectangle rounding of the tests (3.3, V4).
- Whether load-time vertex merging changes any shipped occluder (3.1).
- The per-camera aspect factor's values; the player's *fov*; the camera that passes a computed near value (4).
- Which items override the default sector point (5).
- How the portal fade is drawn (2); the fourth traversal option (2).
- Whether the frame's other parts (sky, HUD) are drawn while there is no camera sector (6).

## 11. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrRender_R1.dll`,
  `bin\XR_3DA.exe`, `bin\xrGame.dll`; archives `gamedata.db0` … `gamedata.dbd` (merged view).
- Data examined: `levels\*\level.hom` (25 files), `levels\*\level` chunk 8 (30 files), `config\**\*.ltx`.
- Analysis: static analysis of the shipped binaries (headless decompilation, constants read from the
  binaries' data); statistical checks over the level data; arithmetic.

## Gaps found in review

- The box test reached from outside the renderer reports visible without testing under one further condition
  besides the box containing the camera; what that condition is is not stated (spec V4).
- Whether the frustum tests of dynamic items' bounding spheres (against each frustum their sector was reached
  with) and of static visuals use any margin or tolerance is not stated, though margins are in the given scope
  (specs V4, V6).
