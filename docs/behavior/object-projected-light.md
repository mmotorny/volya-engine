# Projected light of objects (first renderer)

Behaviour specification of the **projected light texture** of the game's first (static-lighting)
renderer: which objects receive it, what each object's part of it shows and when it is redrawn, the
per-object **approximate colour** it is cleared with (the light estimate whose mean is the object's
brightness value), and how the model programs sample it. It completes `docs/behavior/level-objects.md`
sections 5.3 – 5.5, which define the element choice and the colour equation of element 0 of the model
shaders but leave the texture's content unestablished. Where a fact could not be established it is marked
**unestablished**.

Conventions:

- Builds on `docs/behavior/static-shading.md` (here "SS": shader resolution, elements, weather inputs,
  fog, render-state vocabulary, light-map and vertex-colour inputs), `docs/behavior/level-objects.md`
  ("LO": objects, receivers, the model colour equations, per-object visibility values),
  `docs/behavior/dynamic-lights.md` ("DL": light records, the directional records A and B),
  `docs/formats/ogf.md` (visual types) and `docs/formats/level.md` (sectors).
- Vectors are rows, `·` is the dot product, `sat(x)` = clamp(x, 0, 1) per component, `trunc` rounds
  toward zero. Positions are world positions in metres; y is up. E is the camera position. "Frame" is one
  drawn image; "frame time" is the game's frame time in seconds (0 while paused); "time" is the game's
  global clock in milliseconds.
- Every fact carries the name of its check, a script in `checks/object-light/` of the dirty workspace
  (`run_all.py` runs them all; the binary checks read the disassembly listings of
  `checks/dynamic-lights/out/`, produced by that directory's `dump.py`).

## 1. Overview

1. The renderer keeps one 512 × 512 texture with 8 bits per channel (alpha, red, green, blue), divided into
   100 **cells** of 51 × 51 pixels (section 2).
2. Each frame, objects close enough to the camera become **receivers** (LO 5.3). Each receiver owns a cell;
   a cell is redrawn only when its receiver is new to it, has moved out of the volume the cell was drawn
   for, or after a random 30 … 90 s (section 3).
3. Drawing a cell clears it to the receiver's **approximate colour** (section 5) and then draws, seen from
   high above the object, the static level geometry of the object's sector under it with a special pass
   that writes the geometry's **baked light** (light map or vertex light, plus ambient and hemisphere) in
   colour and its **sun visibility** in alpha (section 4).
4. A receiver's meshes are drawn with element 0 of their model shader: each vertex looks up its cell
   straight below itself, and the pixel takes that light plus the sun term weighted by the looked-up sun
   visibility, blended with the object's own estimate (LO 5.5; section 6).

## 2. The texture and its cells

- One render target, 512 × 512 pixels, format with 8-bit alpha, red, green, blue (Direct3D 9 format
  21), created with each level (OL-1). Its resource name, by which the model shaders bind it, is
  `$user$projector`.
- 100 slots; slot *i* (0 … 99) owns the cell whose top-left pixel is (51 · (*i* mod 10),
  51 · ⌊*i* / 10⌋), 51 × 51 pixels (OL-1, OL-5). The remaining 2 columns and rows of pixels (510, 511) belong
  to no cell.
- Each slot stores: its receiver, the frame it was last used in, the recorded centre *C*′ and **recorded
  box** of section 3, a re-draw time, the texture mapping and the clamp box of section 6 (OL-3, OL-6, OL-7).

## 3. Receivers and slots

### 3.1 Receivers

During the camera's pass, as each object with a model is taken for drawing, it becomes a receiver of the
frame when all of these hold (OL-2):

1. fewer than 100 receivers have been taken in this frame;
2. the object answers that it receives shadows: the engine's default answer is yes (OL-2); of the object kinds
   checked only crows (`AI_CROW`) answer no; every other kind checked (physics objects, breakables,
   destroyable objects, hanging lamps, inventory boxes, items, food, ammunition, weapons, grenades,
   artefacts, zones, restrictors, the level changer, stalkers, the trader, monsters, the helicopter, the
   actor) answers yes (OL-14);
3. it is not already a receiver in this frame;
4. |*E* − *C*| + *R* < 50 · *R*, with *C*, *R* the model's header sphere centre transformed by the object's
   transform and its radius (as LO 5.3);
5. the object lies in a sector (an object outside every sector logs `! Invalid object '<name>' position.
   Outside of sector structure.` and is not a receiver).

Receivers are taken in the order the frame's traversal reaches the objects (the order among objects of
one sector is **unestablished**).

### 3.2 Keeping or redrawing a slot

After the traversal, before the frame's geometry is drawn, every receiver is examined (OL-3, OL-11):

- It **keeps** its slot if it had one in the previous use, that slot still names it, its current world
  box (the model's header box transformed by the object's transform, as the axis-aligned box of the
  transformed box) lies inside the slot's recorded box on all three axes, and the time is not past the
  slot's re-draw time. The slot is then marked used in this frame. Nothing is drawn.
- Otherwise it is **queued**.

Then the slots are visited in order 0 … 99; each slot **not** marked used in this frame is given to the
next receiver taken from the end of the queue (the last queued first), until the queue is empty or the
slots run out; a receiver left without a slot draws with stale constants (**unestablished** what it shows).
A slot that is given a receiver records (OL-3):

- the recorded centre *C*′ = *C* + (0, 0.1 *R*, 0);
- the recorded box: the receiver's current world box grown on each side by 0.1 of its size along that
  axis;
- the re-draw time = time + 30,000 + (*n* mod 60,000) ms, *n* a value of the game's random generator
  (so 30 … 90 s later);

and its cell is drawn (section 4). (A sanity check of the camera of section 4.1 that would leave the slot unused never fails for a finite
*C*′.)

### 3.3 When

Section 3.2 runs every frame in which the camera is inside a sector, after the visibility traversal and
before any geometry of the frame is drawn, so a receiver's cell is ready in the frame it is first needed
(OL-11).

## 4. Drawing a cell

### 4.1 Camera

With *h* = 1.1 *R* (OL-4):

- eye at *C*′ + (0, 200, 0), looking straight down (direction (0, −1, 0)), up vector (0, 0, 1); image
  x grows with world +x, image y with world +z;
- perspective projection with tan(half angle) = *h* / 200 horizontally and vertically, near plane at depth
  199.999 and far plane at depth 207 + *h*; so the depth window runs from 0.001 m above *C*′ down to
  7 + *h* below it, and at the depth of *C*′ the image spans *C*′ ± *h* in x and z;
- for a point **P**, with *d* = *C*′.y + 200 − **P**.y: x_n = (**P**.x − *C*′.x) · 200 / (*d* · *h*),
  y_n = (**P**.z − *C*′.z) · 200 / (*d* · *h*), depth z_n = *q* (*d* − 199.999) / *d*,
  *q* = (207 + *h*) / (207 + *h* − 199.999) (OL-N1, OL-N2);
- viewport: the slot's cell (section 2), depth range 0 … 1; world transform identity.

With Direct3D 9 rasterisation (pixel centres at integer coordinates; documented behaviour) a point with
(x_n, y_n) lands at texture pixel coordinates (51 · (*i* mod 10) + 25.5 · (1 + x_n), 51 · ⌊*i*/10⌋ +
25.5 · (1 − y_n)); the lookup of section 6 then hits the texel drawn for that point (v offset by 0.05
pixel).

### 4.2 Clear

The cell (only the cell: viewport-limited clear of the colour, OL-5) is cleared to the receiver's
approximate colour **A** = (A_r, A_g, A_b) of section 5, as computed at the receiver's last full update
(bringing *h* and *s* to the current frame before the clear does not change **A**; section 5.3):

- red, green, blue bytes = clamp(trunc(255 · A_r), 0, 255), … (OL-5, OL-N3);
- alpha byte = clamp(trunc(255 · (A_r + A_g + A_b) / 4), 0, 255).

The depth buffer used with the texture is cleared to 1 (and its stencil, if any, to 0) once per frame,
before the first cell of that frame is drawn, in frames that draw a cell (OL-5). **Disputed in review:** the
depth clear happens in every frame in which at least one receiver is queued (3.2), before slots are
given out, also when no free slot remains and no cell is drawn. Pixels of the cell that no geometry covers keep the clear colour.

### 4.3 What is drawn

The query box *B*_q = [*C*′.x − *h*, *C*′.x + *h*] × [*C*′.y − *h* − 7, *C*′.y] × [*C*′.z − *h*,
*C*′.z + *h*] (OL-7). Starting from the root visual of the **receiver's own sector** only (`level.md`
sector; no portals are followed), visuals are processed (OL-8):

- groups (type 1) and billboard groups (type 6): each child whose header box overlaps *B*_q (closed
  intervals on all three axes) is processed; the billboard itself is never drawn here;
- skeleton visuals (types 3, 10): as groups;
- any other visual (mesh, progressive mesh, tree instance): drawn with **every pass of element 4** of its
  shader (SS section 5), if the shader has an element 4; a progressive mesh draws the detail level it was
  last drawn with (**unestablished** before its first draw).

The root visual itself is processed without a box test; a leaf's own box is not tested.

### 4.4 Element 4 of the level's shaders

Element 4 of the library classes (OL-9) and of scripted definitions with `l_special` (SS 3.4):

| Class / definition | Programs | Samplers | Colour written (rgb, a) |
|---|---|---|---|
| `LM      `, `LM_AREF ` | `lmap_l` / `lmap_l` | `s_lmap` = T[1], `s_hemi` = T[2] (`s_base` = T[0] bound, unused) | rgb = L_ambient.rgb + Lm.rgb + sat(L_hemi_color.rgb) · h, h = (Hm.r + Hm.g + Hm.b)/3; a = Lm.a — Lm and Hm sampled at the light-map coordinate (as SS 7.1) |
| `V       `, `V_AREF  ` | `vert_l` / `vert_l` | `s_base` = T[0] (unused) | rgb = sat(color.rgb + L_hemi_color.rgb · N.w + L_ambient.rgb); a = sat(color.a) — the vertex inputs of SS 7.2 |
| `BmmDold ` (terrain) | `impl_l` / `impl_l` | `s_base` = T[0], `s_lmap` = T[1] | rgb = L_ambient.rgb + Lm.rgb + sat(L_hemi_color.rgb) · B.a; a = Lm.a — both sampled at the base coordinate (as SS 7.4) |
| `selflight` (script) | `simple` / `simple` | `s_base` = T[0] | rgb = B.rgb, a = B.a (SS 8.1) |
| `details\lod` (script) | `lod` / `lod` | — | never reached here (billboards are not drawn, 4.3) |

No sun term, no detail texture and no fog in any of them: the sun is applied later, by the model, through
the alpha channel (section 6). The sampler states are those of SS 4.4 (`s_hemi` clamp, linear, no mip;
`s_lmap`, `s_base` wrap, linear, with mip levels where the texture has them).

States of every element-4 pass of the table (OL-9): fog off; depth test (less-or-equal) and depth write
on; blending off; **alpha test off** (also for `LM_AREF ` and `V_AREF `, whose cut-out holes are drawn
solid, and for `def_trans`-like definitions with alpha blending in their normal elements); the usual
back-face culling (SS 10.1). Colour outputs are clamped to [0, 1] and stored as bytes.

Classes without element 4 draw nothing here (OL-9): `D_TREE  ` (all trees, `flora\*` and
`def_shaders\def_objects_lod`), `S_SET   ` (glows), `effects\lightplanes`, `effects\wallmarkblend`.
(Element 4 of the model classes is a shadow pass; models are not part of a sector's static geometry and
are never drawn here.)

## 5. The approximate colour

### 5.1 Inputs kept per object

Every object with a model keeps (LO 5.4): the sky visibility target and smoothed value *h*, the sun
visibility target and smoothed value *s*, a list of **tracked lights** (each with a visibility energy *e*
and a smoothed visibility *σ*), and the approximate colour **A**. Initial values (OL-12): *h* = 0.5,
*s* = 0.2, **A** = (0, 0, 0), no tracked lights. Three tracking options, all on initially: lights (1),
sun (2), sky (4); whether any game object turns them off is **unestablished**.

### 5.2 A full update

A **full update** of an object (OL-12), with **o** = *C* + (0, 0.3 *R*, 0) (*C*, *R* as in 3.1):

1. sun visibility target (option 2) and sky visibility target (option 4): exactly as LO 5.4 (the sun ray
   after a random 6 … 12 full updates; five sky directions per full update); at the first full update the
   smoothed *h* is set to its target.
2. *h* and *s* are brought to the current frame (5.3).
3. If option 1 is on (for every receiver; for other objects only if they cast or receive shadows, which
   every kind of the game does, 3.1): **Disputed in review:** crows (`AI_CROW`) answer no both to casting and
   to receiving shadows, so for them this step never runs: no light is tracked and **A** has no
   contributions (ambient, hemisphere and sun terms only); whether the other kinds cast shadows was not
   checked, only that they receive them (which suffices for this step).
   - every active light record (DL 2.4) returned by a spatial query of the box **o** ± (*R*, *R*, *R*) (its
     bounding sphere overlaps the box) whose position **L** satisfies |**o** − **L**| < range + *R* is
     tracked (a newly tracked light starts
     with *e* = *σ* = 0); tracked lights not found again are dropped. The chunk-6 point records are active
     and are tracked; the sun records A and B are not active and never tracked.
   - for each tracked light: a ray from **L** towards **o** + *R*/2 · **j** (**j** a random unit vector,
     new for every ray), of the length of that distance, against the static collision; *e* ← clamp(*e* +
     (4 if the ray hits nothing, else −2) · frame time, −0.5, 1); *σ* ← 0.9 *σ* + 0.1 *e*;
   - the **contributions** are: each tracked light with mean(colour.rgb) · *σ* > 10⁻⁵, with weight
     *w* = *σ* / 2; and the sun record B (DL 3.1: colour = sun_color × `r2_sun_lumscale`, position
     *E* − 500 · d_B, range 600) when mean(B colour) · *s* > 10⁻⁵, with weight *w* = *s* / 2.
4. **A** = L_ambient.rgb + *h*′ · L_hemi_color.rgb + *s*′ · L_sun_color.rgb, with the current blended
   environment's values (SS 9.2), *h*′ = *h* if option 4 is on else 0.2, *s*′ = *s* if option 2 is on else
   0.2; then, if option 1 is on, for each contribution with colour **c**, position **L** and range *r*:
   **A** += *w* · **c** · clamp(1 − |**L** − **o**| / (*r* + 10⁻⁵), 0, 1); if option 1 is off, **A** =
   (0.1, 0.1, 0.1).

The two halvings of the original (a further × 1/2 for lights that are not excluded and a × 2 for them in
the sum) cancel; the result is as written (OL-12). The object's **brightness value** (DL 9) is
(A_r + A_g + A_b) / 3 (DL-R16).

### 5.3 When values update

- **Full updates** (OL-11): in each frame's camera pass, the level's current view entity (the actor in
  play) gets a full update; in addition exactly one entry of the frame's list of spatial entries found in
  the view frustum (objects, glows and lights, in the order of that list) is chosen: entry number
  *n* mod *N* where *N* is the length of the list and *n* a counter incremented every frame; if that entry
  is an object that lies in a sector visited in this frame, passes one of that sector's frusta and is not
  hidden by the occlusion test (`occlusion-tests.md`), it gets a full update. So with *N* entries in view each
  visible object gets one full update every *N* frames or so. The per-object "at most once per frame" of
  LO 5.4 holds (a second full update in the same frame does nothing).
- **Smoothing** (LO 5.4: *v* ← *k* · target + (1 − *k*) · *v*, *k* = clamp(frame time, 0, 1)) of *h* and
  *s* is applied at most once per frame, in each frame in which the values are used: when the object is
  drawn (its L_dynamic_props), when its cell is drawn, when its brightness is read, and in a full update.
- **A** changes only in full updates.

## 6. Sampling in the model programs

For a receiver drawn in a frame, the constants of its slot are set (OL-10):

- the **texture mapping**: a world point **P** maps to the cell coordinates
  u = (51 · (*i* mod 10) + 0.5)/512 + (51/512) · (0.5 + 0.5 · x_n),
  v = (51 · ⌊*i*/10⌋ + 0.5)/512 + (51/512) · (0.5 + 1/1024 − 0.5 · y_n),
  with x_n, y_n of section 4.1 for that slot's *C*′ and *h* (texture coordinates in [0, 1], v down). The
  vertex program delivers it as a projective coordinate divided per pixel (OL-N1, OL-13);
- the **clamp box**: [*C*′.x − *h* + 0.05, *C*′.x + *h* − 0.05] × [*C*′.y − *h* − 7 + 0.05,
  *C*′.y − 0.05] × [*C*′.z − *h* + 0.05, *C*′.z + *h* − 0.05] (OL-7); each vertex's world position is
  clamped into it per axis before mapping;
- the **factor** φ = c + (1 − c) · *g*², *g* = (|*E* − *C*′| + *R*) / (50 *R*), c = `r1_lmodel_lerp`
  (OL-10; this settles LO 5.5's "*C*′ believed *C*": *C*′ is the recorded centre of 3.2, taken at the
  slot's last drawing).

The sampler (OL-13): clamp addressing, linear minification and magnification, no mip levels.

Element 0 of `MODEL   ` / `MODELEbB` (LO 5.5, programs `model_def_hq`, `model_env_hq`; OL-D2): with *t*
the texel at the mapped coordinate of the clamped vertex position, S(**n**) the sun term (sat, a vertex
colour) and *Q* the model light of LO 5.5:

  light = lerp(*t*.rgb + S(**n**) · *t*.a, *Q*, φ),  C = 2 · light · B.rgb,  A = B.a

(the environment-mapped variant uses its base of LO 5.5). The sun term here is not multiplied by the
object's own sun visibility: the looked-up *t*.a (the sun visibility of the geometry below, or the clear
alpha) replaces it.

## 7. Verification facts

- `levels\l01_escape\level` chunk 2 (395 non-empty entries) by element 4 (OL-D3): `lmap_l` 259
  (`LM      ` and `LM_AREF ` entries), `vert_l` 62, `impl_l` 4 (terrain), `selflight` 3; without element 4:
  51 `D_TREE  ` entries, 3 glows, 11 wall marks, 1 light-plane entry; `details\lod` 1 (not reached).
- Worked factors (OL-N4, c = 0.1): a receiver of radius 0.2 m at 1.5 m: *g* = 0.17, φ = 0.126 (the cell's
  light dominates); radius 0.5 m at 3 m: φ = 0.118; radius 0.2 m at 9.79 m: φ = 0.998 (at the receiver
  limit the object's own estimate *Q* takes over).
- Worked clear colour (OL-N4): an object with *h* = *s* = 0 and no contributions under the start's blended
  environment (sky.md 4: ambient (0.04814, 0.05170, 0.05525)) clears to bytes R, G, B = 12, 13, 14,
  A = 9.
- An object of radius 0.15 m is a receiver up to 7.35 m from the eye (OL-N5); near objects have φ near c,
  so their light comes almost entirely from the baked light of the surface below them (section 4).
  Drawn with element 1 instead, an object with *h* = *s* = 0 (indoors) under the start's environment gets
  light = L_ambient, C = (0.096, 0.103, 0.111) × B (OL-N5).

## 8. Statements of earlier specifications settled here

- `level-objects.md` 5.5: "how the projected light texture is filled is unestablished; a renderer without
  it can draw every object with element 1": filled as sections 2 – 4; drawing receivers with element 1
  instead replaces the baked light under the object by *Q* (ambient, hemisphere by sky visibility, sun by
  sun visibility), which indoors is near the ambient alone.
- `level-objects.md` 5.5, "*C*′ the receiver's recorded centre (believed: *C*)": *C*′ = *C* + (0, 0.1 *R*,
  0) as recorded when its cell was last drawn (section 3.2).
- `level-objects.md` 5.4, values "updated at most once per frame": full updates follow the schedule of
  5.3 (the actor every frame, other visible objects in turn); the smoothing happens in frames where the
  values are used.
- `dynamic-lights.md` 9, the brightness value "is not used for drawing; how the lights are weighted and
  smoothed is unestablished": the colour behind it is the clear colour of every cell (4.2); weighting and
  smoothing in 5.2.
- `static-shading.md` 5, element 4 "special pass (used by the LOD billboards)": the library classes
  `LM      `, `LM_AREF `, `V       `, `V_AREF  `, `BmmDold ` also have an element 4, used only for this
  texture (4.4).

## 9. Unestablished

- The order in which receivers are taken among objects of one sector (3.1).
- What a receiver without a slot shows (3.2).
- The detail level of a progressive mesh drawn in a cell before its first normal draw (4.3).
- Whether game objects change the three tracking options of an object (5.1).
- The order of the frustum list that the full-update turn counts in (5.3); it makes no difference once all
  entries have had their turn.
- The rounding of colour outputs to bytes in the render target (believed: as the device converts).

## 10. Provenance

- Date: 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`, `bin\xrGame.dll`.
- Data examined: `shaders.xr`; `shaders\r1\*.s`, `lmap_l.*`, `vert_l.*`, `impl_l.*`, `simple.*`,
  `model_def_hq.*`, `model_env_hq.*`, `common.h` (read as data); chunk 2 of `levels\l01_escape\level`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation): the texture's
  creation, the receiver test, the slot bookkeeping and the drawing of cells (camera, viewport, clear,
  texture mapping, clamp box, geometry selection), the library shader classes' element 4, the per-object
  light tracking and its update schedule, the constants set for receivers, the object classes' answer to
  the shadow-receive query; arithmetic of the conversions emulated in the checks. The checks in
  `checks/object-light/` reproduce every fact.

## Gaps found in review

- The depth buffer the cells are drawn with (its size, at least covering the 512 × 512 texture, and its format) is not stated.
