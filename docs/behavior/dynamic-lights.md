# Dynamic lights on geometry (first renderer)

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of specification P16 of the approved `particles-and-lights-scope`: which dynamic
lights exist, which of them are drawn, which static geometry and which objects each one lights, the point
and spot passes with their programs, constants and attenuation textures, and the settings `r1_dlights` and
`r1_dlights_clip`. It also states what the renderer does with every record of the level's chunk 6,
including the directional (sun) record, and settles the statements that `static-shading.md` section 12,
`level-objects.md` section 5.6 and `level.md` section 7 leave unestablished or state otherwise (section 11).
What could not be established is marked **unestablished**.

Builds on (not repeated): `docs/formats/level.md` (chunk 6 layout, section 9 camera sector and traversal),
`docs/behavior/static-shading.md` (shader resolution, elements, program inputs, fog, precision rules,
drawing order, weather inputs), `docs/behavior/level-objects.md` (objects present at the start, object
transform, model shader classes, per-object light inputs, hanging-lamp structure),
`docs/formats/ogf.md`, `docs/formats/sav.md`, `docs/formats/ltx-config.md`,
`docs/behavior/options-screens.md` (quality presets). The colour a light shows over time comes from its
owner (hanging lamps P13, zones P9, searchlights P14, torches P20, artefacts P10) through `lanims.xr`
(P12, `lanims-xr.md`); this specification takes a light's colour as last set.

Conventions: vectors are rows; `·` is the dot product, `×` the cross product; `sat(x)` = clamp(x, 0, 1)
per component; `normalize(v)` = v / |v|. A matrix M maps a point p by (p, 1)·M (row-vector convention, as
in `static-shading.md`). E is the camera position. "Frame" is one drawn image. Every fact carries the name
of its check, a script in `checks/dynamic-lights/` of the dirty workspace (`run_all.py` runs them all;
`dump.py`, run first, produces the disassembly listings the binary checks read).

## 1. Overview

1. Every light is a **light record** of the renderer (section 2). The level's chunk 6 creates records at
   load (section 3.1); game objects create records for lamps, searchlights, torches, artefacts, zones and
   other objects (section 3.2).
2. Each frame, during the camera's pass, the renderer collects the lights that are seen and eligible into
   two lists, point and spot (section 4). Records made from chunk 6 are never eligible.
3. After the sky and clouds are drawn, every listed spot light, then every listed point light, gets one
   **light pass** (section 5): a traversal from the light's sector with the light's own frustum selects the
   static geometry and objects to draw again, additively, with the light's element (2 point, 3 spot) of
   their shaders (section 6).

## 2. The light record

### 2.1 Fields and defaults

| Field | Default for a new record | Notes |
|---|---|---|
| type | point | point, spot or directional; only chunk 6 makes directional records (section 3) |
| position P | (0, −1000, 0) | |
| direction D | (0, −1, 0) | stored normalised |
| a second ("right") vector | (0, 0, 0) | stored normalised; not used by the passes |
| range r | 8 | |
| cone angle θ | 1.0471976 (60°) | full apex angle in radians; spot lights only |
| colour (R, G, B, A) | (1, 1, 1, 1) | |
| active | off | |
| shadow setting | on (records created by game objects) | stored; no effect on the passes (section 2.5) |
| excluded | off (records created by game objects) | set for every chunk-6 record (section 3.1) |

Checks: `dl_r1.py` DL-R1.

### 2.2 Updating a field

- Position: the new value is ignored when it differs from the stored one by less than 0.001 on every axis.
- Range: the new value r′ is ignored when |r′ − r| < max(0.1·r, 0.001).
- Cone angle: ignored when it differs by less than 0.00001.
- Direction, the second vector, colour, type, active and shadow: always stored.
- The colour may be given as (R, G, B, A) or as (R, G, B), which stores A = 1.
- Two further settings are accepted and have no effect in the first renderer: a projected texture name
  (lamps pass their light texture, `level-objects.md` section 6) and one other value (meaning
  **unestablished**).

Checks: `dl_r1.py` DL-R2, `dl_game.py` DL-G1, DL-G2.

### 2.3 Bounding sphere

A record's bounding sphere (C, R) follows its type. It is recomputed when the position, range or cone is
updated (2.2), and when a new direction differs from the stored one by 1 − D_new·D_old ≥ 0.00001 (a smaller
change is stored without recomputing):

- point: C = P, R = r;
- spot with θ < π/2: R = r / (2·cos²(θ/2)), C = P + D·R;
- spot with θ ≥ π/2: C = P + D·r, R = r·tan(θ/2).

Examples: θ = 60°, r = 8: R = 5.3333, C 5.3333 ahead of P; θ = 120°, r = 8: C 8 ahead, R = 13.8564
(`dl_numeric.py` DL-N3). Checks: `dl_r1.py` DL-R3.

### 2.4 Active state and sector

An active record takes part in the spatial queries of section 4 and of section 9; an inactive one in none.
Its **sector** is the sector containing its position P, found by the rule of `level.md` section 9.1 (the
previous sector is kept when the rule finds none; a record that has never been in a sector has none),
re-determined after P changes. Checks: `dl_r1.py` DL-R4.

### 2.5 Settings without effect

The shadow setting and the projected texture name do not change which geometry a light reaches or how
it is drawn: spot passes always project the texture `internal\internal_light_att` (section 6.4). The
shadow setting was found tested by none of the renderer's users of the light record (`dl_r1.py` DL-R17,
`dl_game.py` DL-G1); what else it may change is P17's.

## 3. Light sources

### 3.1 Chunk 6 of `level`

Every record of chunk 6 (108 bytes, `level.md` section 7; count = chunk size / 108) becomes a light record
with the **excluded** mark and the shadow setting off. By record type (the u32 at record offset 4):

- **point (1)**: type point; position from record offset 56, range from offset 80, colour (R, G, B, A) =
  the four floats at offset 8; direction (0, 0, −1); made active. Being excluded, it is never drawn
  (section 4); it is tracked for the brightness value of section 9.
- **directional (3, the sun)**: two directional records, A and B, both excluded and never made active,
  both first aimed along the record's direction (offset 68). No other field of the record is used.
- A level without a directional record is fatal ("Where is sun?").

Each frame, before any object's light inputs are updated (`level-objects.md` section 5.4), A and B are set
from the weather (`static-shading.md` section 9; L_sun_dir, sun_color):

- A: direction L_sun_dir, position E − 500·L_sun_dir, colour sun_color, range 600.
- B: direction d_B = normalize(v) with v = L_sun_dir − (0, 0.75, 0) (if |v| < 0.001, L_sun_dir is added
  to v again, at most 10 times, before normalising); position E − 500·d_B; colour sun_color ×
  `r2_sun_lumscale` (section 8); range 600.

So the record's own direction is overwritten before it is ever used. B's direction is the sun direction of
the objects' sun-visibility ray: the ray of `level-objects.md` section 5.4 goes in direction −d_B. At the
first level's start (L_sun_dir ≈ (0.9132, −0.2079, 0.3505), altitude 12.0°) d_B ≈ (0.6670, −0.6997,
0.2560), altitude 44.4° (`dl_numeric.py` DL-N1). B also enters the brightness value (section 9). A is
used by nothing in the first renderer.

Checks: `dl_r1.py` DL-R5, DL-R6; `dl_data.py` DL-D1.

### 3.2 Lights created by game objects

Game objects create records through the renderer and set their fields (`dl_game.py` DL-G6). The record
type each source sets (the owners specify when the light is active and what colour, range and position it
has):

| Source (data) | Records | Type | Shadow setting | Owner |
|---|---|---|---|---|
| hanging lamp (`O_HLAMP` record, `level-objects.md` section 6) | main light | spot if light-flag bit 0x10, else point | light-flag bit 0x02 | P13 |
| | ambient light (light-flag bit 0x20) | point | off | P13 |
| searchlight (`projector_definition`) | one | spot | on | P14 |
| torch (`torch_definition`) | main light | spot | on | P20 |
| | second light (`omni_range`, `omni_color`) | point | off | P20 |
| artefact (`lights_enabled`, `trail_light_color`, `trail_light_range`) | one | point (believed) | on | P10 |
| zone (`idle_light`, `idle_light_*`; `light_color`, `light_range`, `light_time`) | idle and other lights | point (believed) | on | P9 |

"Believed": the creating code sets no type, so the record keeps the default (point); later changes of type
were not searched for. Other game objects (weapon shots, explosives, rockets, the helicopter, vehicles)
also create lights; they belong to F1 and are drawn by the same rules. Checks: `dl_game.py` DL-G2 … DL-G5.

### 3.3 At the first level's start

The five lamps online at the start (`level-objects.md` section 6) have health > 0 and give seven records:
five spot main lights (cone 120°, ranges 1, 1.5, 2, 3, 3; shadow bit clear) and two point ambient lights
of the television sets (radius 1, power 0.2) (`dl_lamps.py` DL-L1). Zone idle lights are off and the
artefact's lights disabled (scoping section 1.3). Whether the actor's torch and the helicopter's light are
on at the start is owned by P20 and F1 (**unestablished** here). The 40 chunk-6 point lights are never
drawn.

## 4. Lights chosen each frame

During the camera's pass, the spatial query of the camera frustum also returns the active light records
whose bounding sphere intersects it. A returned record is listed when all of the following hold, in this
order:

1. It has a sector, that sector was visited by the camera's traversal this frame (`level.md` section 9.2),
   and the bounding sphere intersects one of the frusta the sector was reached with.
2. Occlusion: its box (C ± R on each axis) passes the occlusion test of `level.hom` (out of scope, as in
   `level.md`). The test is repeated only when due: after a passed test the next one is due after
   10 + (a random integer 0 … 14) frames, after a failed test in the next frame; between tests the record
   counts as passed. A renderer without occlusion culling lists a superset.
3. It was not already listed this frame.
4. It is not excluded (section 3.1), `r1_dlights` is on, its range is at least 0.1, and it has a sector.

A listed point record goes to the point list; a record of any other type to the spot list. The lists are
emptied after the frame's light passes. Checks: `dl_r1.py` DL-R7.

## 5. Light passes

### 5.1 Place in the frame

The light passes follow the sky and clouds (step 3 of `static-shading.md` section 10.3) and precede the
blended billboard pass (step 4) and the priority 2–3 passes (step 5): first one pass per spot light, in
list order, then one per point light. All light passes add to the image with saturation (section 6.2) and
every contribution is non-negative, so their order and the order inside a pass do not change the result.
Checks: `dl_r1.py` DL-R8.

### 5.2 Fade and skipping

With clip = `r1_dlights_clip` (section 8) and the record's bounding sphere (C, R):

  g = 1 − (|E − C| − R) / clip

The light is skipped when g < 0.00001; a point light is also skipped when r < 0.01. Otherwise its colour is
multiplied by f = sat(g) (section 6.3). Example: R = 3, clip 75: f = 0.3733 at |E − C| = 50; skipped at 80
(`dl_numeric.py` DL-N4). Checks: `dl_r1.py` DL-R9.

### 5.3 The light's frustum

Each pass uses a view V and a perspective projection Q_L, both of the Direct3D left-handed kind:

- V: eye O, forward axis F, up hint U; X = normalize(U × F), Y = F × X; (p, 1)·V = ((p − O)·X, (p − O)·Y,
  (p − O)·F, 1).
- Q_L with scale h, near n and far z_f, k = z_f / (z_f − n): a view point (x, y, z) gives clip coordinates
  (h·x, h·y, k·z − k·n, z).

| | O | F | U | h | n | z_f |
|---|---|---|---|---|---|---|
| point light | P + (0, 300, 0) | (0, −1, 0) | (0, 0, 1) | 1 / tan(1°) | 300 − r | 300 + r |
| spot light | P | D | (0, 1, 0), or (0, 0, 1) when \|D.y\| > 0.99 | 1 / tan(θ/2) | 0.1 | r + 10⁻⁷ |

The light's frustum is the clip volume of M = V·Q_L (−w ≤ x ≤ w, −w ≤ y ≤ w, 0 ≤ z ≤ w for clip (x, y, z,
w)). For a point light it is a narrow column seen from 300 above: at the light's height its half-width is
300·tan(1°) = 5.2365 (for r = 10: 5.0620 at its top, 5.4110 at its bottom; `dl_numeric.py` DL-N2).
Checks: `dl_r1.py` DL-R9.

### 5.4 Traversal

The sectors are traversed as in `level.md` section 9.2, with these differences:

- It starts in the light record's sector (section 2.4) with the light's frustum.
- The eye of the traversal (portal sides, frusta through clipped portals) is O of section 5.3.
- A spot light treats every portal that an axis-aligned box of half-size 0.02 around its position touches
  as rule 2's touched portal (the camera's box is 0.002); a point light marks none.
- Rule 4 (portal size) and rule 6 (occlusion) are not applied.

Checks: `dl_r1.py` DL-R11.

### 5.5 What a pass draws

The pass submits, for each sector the traversal visited and each frustum the sector was reached with:

- **Static geometry**: the sector's visual hierarchy, descended as in the camera's pass (`level.md`
  section 9.3, `ogf.md` section 8), tested against that frustum.
- **Objects**: every object whose spatial bounding sphere the light's frustum query returns, whose sector
  was visited and whose sphere intersects one of that sector's frusta, draws its model (its visuals are
  tested against that frustum).
- **First-person models**: when a sphere of radius 2 around E intersects the light's frustum, the
  first-person models (the items drawn in the player's hands) are submitted too, and drawn after the rest
  of the pass with the first-person projection, as the frame draws them.

A submitted visual (static leaf or model part) is drawn in the pass only if all of these hold:

1. It was already submitted earlier in the same frame (in practice: by the camera's pass). Geometry the
   camera's pass did not reach is never lit.
2. It was not already submitted in this pass.
3. Its projected-size measure, computed from the camera as in the camera's pass (`static-shading.md`
   section 10.2), is above ssa_min.
4. Its shader has the element of the pass — element 2 for point lights, element 3 for spot lights —
   (section 6.1), and that element's priority is 0 or 1.

The drawn visuals are grouped and drawn as the frame's priority 0–1 passes are. Checks: `dl_r1.py`
DL-R9, DL-R11, DL-R12; `dl_game.py` DL-G7.

## 6. Pass shaders

### 6.1 Which shaders have light elements

A library definition (`shaders.xr`) gives elements 2 and 3 by class tag:

| Class tag | Vertex programs (point / spot) | Alpha reference | Condition |
|---|---|---|---|
| `LM      ` | `lmap_point` / `lmap_spot` | 0 | — |
| `LM_AREF ` | `lmap_point` / `lmap_spot` | `Alpha ref` | only when `Alpha-blend` is false (otherwise no light elements) |
| `LmEbB   ` | `lmap_point` / `lmap_spot` | 0 | — |
| `V       ` | `vert_point` / `vert_spot` | 0 | — |
| `V_AREF  ` | `vert_point` / `vert_spot` | `Alpha ref` | — |
| `BmmDold ` | `impl_point` / `impl_spot` | 0 | — |
| `D_TREE  ` | `tree_s_point` / `tree_s_spot` if `Object LOD`, else `tree_w_point` / `tree_w_spot` | 0 | — |
| `MODEL   ` | `model_def_point` / `model_def_spot` | `Alpha ref` if `Use alpha-channel`, else 0 | — |
| `MODELEbB` | `model_def_point` / `model_def_spot` | 0 | — |

No other class has light elements (in particular `S_SET   `, particles and detail objects). An element's
priority is the definition's `Priority` (`static-shading.md` section 3.2), so definitions of priority 2 or
3 (e.g. `def_shaders\def_trans_v`, `models\transparent`) are never lit (section 5.5 rule 4).

A scripted definition (`shaders\r1\<module>.s`, `static-shading.md` section 3.4) has element 2 if the
module defines `l_point` and element 3 if it defines `l_spot`. In the shipped modules these call the
global helpers of `shaders\r1\.s` with the vertex program named in the call: `wmark_point` / `wmark_spot`
(`effects_wallmarkblend`, `effects_wallmarkmult`, `effects_wallmarkset`), `model_def_point` /
`model_def_spot` (`models_selflight`, `models_selflightl`, `models_xanomaly`, `models_xglass`,
`models_xglass2`, `models_xlens`, `models_xmonolith`, `models_xwindows`) and `simple_point` /
`simple_spot` (`selflight`); the helpers set alpha reference 0 and no priority (default 1). The other 22
modules have no light elements.

Checks: `dl_shaders.py` DL-S1 … DL-S4.

### 6.2 Pass states

Every light element is one pass with pixel program `add_point` (element 2) or `add_spot` (element 3):
fog off; depth test on (less-or-equal), depth write off; blending one / one (the result is added to the
image and clamped); alpha test on, passing when alpha·255 > the alpha reference of 6.1. Samplers:

| Sampler | Point pass | Spot pass |
|---|---|---|
| `s_base` | T[0], as in the shader's normal elements | same |
| `s_lmap` | `internal\internal_light_attpoint` | `internal\internal_light_att`, projective coordinates |
| `s_att` | `internal\internal_light_attpoint` | `internal\internal_light_attclip` |

The two attenuation samplers use clamp addressing and linear minification and magnification without
mipmaps (so only the textures' first level is used). Checks: `dl_shaders.py` DL-S1, DL-S2, DL-S4.

The 20 program files `shaders\r1\*_point.*` and `*_spot.*` are these 18 plus `tree_wave_point.vs` and
`tree_wave_spot.vs`, which nothing uses (`dl_shaders.py` DL-S5).

### 6.3 Constants and colour equation

Per pass, with f of section 5.2 and the light record's fields:

- **L_dynamic_pos** = (P, 0.5 / r) in point passes, (P, 1 / r) in spot passes.
- **L_dynamic_color** = (f·R, f·G, f·B, 1) (the record's colour alpha is not used).
- **L_dynamic_xform** = M·T, with M of section 5.3 and T mapping clip (x, y, z, w) to
  (0.5·x + 0.515625·w, −0.5·y + 0.515625·w, z / r, w).

Checks: `dl_r1.py` DL-R9, DL-R10.

Vertex stage, with the world position p and world normal n of the program (6.5) and the fog factor F of
`static-shading.md` section 9.3 at p:

- l = normalize(p − P), s = n·(−l);
- c = sat(L_dynamic_color · s · sat(F)) — a colour output, clamped per component (the precision rules of
  `static-shading.md` section 6);
- point pass: q = (p − P)·L_dynamic_pos.w + (0.5, 0.5, 0.5); coordinates t₁ = (q.x, q.z), t₂ = (q.x, q.y);
- spot pass: σ = (p, 1)·L_dynamic_xform; coordinates t₁ = (σ.x / σ.w, σ.y / σ.w) (projective),
  t₂ = (σ.z, σ.z).

For a spot light σ.z = k·(d − 0.1) / r with d the depth along D, so t₂ runs from 0 at depth 0.1 to 1 at depth
r (for r = 8: 0.4937 at depth 4; `dl_numeric.py` DL-N5).

Pixel stage, with B the `s_base` texel (r, g, b, a), a₁ the `s_lmap` texel at t₁ and a₂ the `s_att` texel
at t₂ (all four components each):

  C.rgb = 2 · B.rgb · B.a · a₁.rgb · a₂.rgb · c.rgb,  A = 2 · B.a · a₁.a · a₂.a · c.a

clamped to [0, 1], then the alpha test, then added to the image. Checks: `dl_data.py` DL-D3.

### 6.4 Attenuation textures

The three textures are game files (`textures\internal\`, in db5 and dbb, identical) used exactly as
stored; each is 32-bit A8R8G8B8 (bytes B, G, R, A), a 128-byte header, the top level first:

| File | Size | CRC-32 | Texels | Levels | SHA-256 of the top level's bytes |
|---|---|---|---|---|---|
| `internal_light_att.dds` | 349,652 | 2F237132 | 256 × 256 | 9 (only the first used) | 3b115f19f9298e610ef391e798b6d906d3ce551b5667da93931753d1e243e072 |
| `internal_light_attclip.dds` | 1,152 | 12E20637 | 128 × 2 | 1 | f4c7a038de813686052e0bf6432bf80996d63e7a5012a9f87f7b4512f8e68662 |
| `internal_light_attpoint.dds` | 16,512 | 01E9896B | 64 × 64 | 1 | 7ffc00e88bcdfedbfcf2728e6efb81bbc6eb6be5f46d3325ae5d08abe33857cc |

Content: `attpoint` is a round falloff, zero on its border and outside the inscribed circle, peak red 252
at texel (32, 31) (x, y); the point attenuation is the product of two of its lookups. `attclip` has two
identical rows; texel 0 and texel 127 are zero and the red channel falls from 254 at texel 1 without
rising: a depth falloff that is zero behind the near plane. `att` is the spot light's projected pattern,
zero on its border (red sum 4,820,473). The channels of a texel are not all equal. Checks: `dl_data.py`
DL-D4.

### 6.5 Position, normal and base coordinate per program

| Programs | p | n | Base coordinate |
|---|---|---|---|
| `lmap_*`, `impl_*`, `vert_*`, `simple_*` | vertex position (static geometry is in world coordinates) | unpacked vertex normal (not normalised) | as the shader's normal elements |
| `wmark_*` | the wall-mark position shift of `static-shading.md` section 8.3 | unpacked normal | as normal elements |
| `tree_w_*` | the waving position of `static-shading.md` section 7.3 (`_w` programs) | normalize(normal transformed by X's rotation) | (m.x, m.y)·consts.xy |
| `tree_s_*` | the unmoved position (`_s`) | as `tree_w_*` | as `tree_w_*` |
| `model_def_*` | the skinned model position (`level-objects.md` section 4.8) transformed by the object's world matrix | normalize(normal transformed likewise) | the vertex texture coordinate |

The model programs come in the four skinning variants of the normal model programs. Unlike the trees'
normal passes (`static-shading.md` 7.3: N_w not renormalised), the tree light programs normalise n; and a
tree drawn with its far element (`tree_s`, unmoved) gets a waving light pass when its definition is not
`Object LOD`, so the lit surface can differ from the drawn one where the waving moves vertices. Checks:
`dl_data.py` DL-D3; `dl_shaders.py` DL-S1.

## 7. Side effects of light passes

- **Billboards drawn again**: a LOD group visual (type 6) reached by a light's traversal is queued for the
  billboards by the camera's rule (`static-shading.md` section 10.2: measure below ssa_A and not below
  ssa_min) without rule 1 of section 5.5. The queue is drawn by the cut-out billboard pass (step 2 of
  `static-shading.md` 10.3, before the lights) and the blended one (step 4, after them) and emptied only
  after step 4, so each such light adds one more drawing of that billboard in step 4.
- **Distortion elements**: a visual whose shader's element 4 is a distortion element is queued for
  distortion again by every light pass that submits it, but only if that element's priority is 0 or 1.
  All 27 distortion elements of the shipped scripts have priority 2 or 3, so this does not happen with
  them; whether a library definition has one of priority 0 or 1 is **unestablished**.

Checks: `dl_r1.py` DL-R13, DL-R15; `dl_data.py` DL-D5.

## 8. Settings

| Setting | Kind | Built-in initial value | Range | Shipped `user.ltx` | Presets (`rspec_*.ltx`) |
|---|---|---|---|---|---|
| `r1_dlights` | on/off | on | — | on | on in all six |
| `r1_dlights_clip` | float | 30 | 10 … 150 | 75 | minimum 30, low 30, default 40, high 50, extreme 75, maximum 60 |
| `r2_sun_lumscale` | float | 1 | −1 … 3 | 1 | — |

`r1_dlights` off lists no light (section 4 rule 4); `r1_dlights_clip` is the clip of section 5.2;
`r2_sun_lumscale` scales B's colour (section 3.1). Checks: `dl_r1.py` DL-R14; `dl_data.py` DL-D2.

## 9. Brightness value of objects

For each object, the renderer tracks the active light records around it (spatial query of lights within
a box around the object, keeping those whose distance is below their range plus the object's radius),
including the chunk-6 point records, and adds directional record B weighted by the object's sun
visibility; from them, with ray tests and smoothing, it keeps a colour whose mean of the three components
it reports to the game as the object's brightness. This value is not used for drawing. How the lights are
weighted and smoothed is **unestablished**. Checks: `dl_r1.py` DL-R16.

## 10. Verification facts

### 10.1 First level at the start

- Chunk 6 of `levels\l01_escape\level`: 41 records; record 0 directional, direction (0.840, −0.423,
  0.340); 40 point records, ranges 7 … 12 (sum 366). None is drawn.
- Lights that can be drawn: the seven lamp records of section 3.3 (and P20's and F1's, if on).
- Chunk 2: 380 of the 395 non-empty entries have light elements: `lmap_*` 250 (alpha reference 0) and 4
  (33), `vert_*` 54 (0) and 3 (33), `tree_s_*` 37, `tree_w_*` 14, `wmark_*` 11, `impl_*` 4, `simple_*` 3;
  without: `def_shaders\def_trans` 5 (alpha blending), `def_shaders\def_trans_v` 5 (priority 2),
  `effects\glow` 3, `effects\lightplanes` 1, `details\lod` 1.

Checks: `dl_data.py` DL-D1, `dl_shaders.py` DL-S6, `dl_lamps.py` DL-L1.

### 10.2 All 30 shipped levels

- Chunk 6: 1,682 records; exactly one directional per level, always the first record; the leading u32 is 1
  for it and 2 for every point record.
- Chunk 2: 8,047 of the 8,315 non-empty entries have light elements; the 268 without: `effects\glow` 81,
  `def_shaders\def_trans` 61, `def_shaders\def_trans_v` 56, `effects\lightplanes` 35, `details\lod` 25,
  `effects\water` 4, `effects\waterstuden` 4, `effects\waterryaska` 1, `effects\flare` 1.
- No `shaders.xr` definition has strict sorting with priority 0 or 1.

Checks: `dl_data.py` DL-D1, `dl_shaders.py` DL-S6.

### 10.3 Worked values

Sections 2.3, 3.1, 5.2, 5.3 and 6.3 give examples (`dl_numeric.py` DL-N1 … DL-N5).

## 11. Statements of earlier specifications settled here

- `static-shading.md` section 12, "the level's 40 point lights (chunk 6) become active dynamic lights when
  `r1_dlights` is on": they are made active (registered) but excluded; they are never drawn (sections 3.1,
  4). Which lights reach which geometry: sections 4 and 5; the attenuation textures: section 6.4.
- `level-objects.md` section 5.6, "the level's point lights and the hanging lamps' lights are added to
  models": the lamps' lights are (with every other game-created light) by sections 4 and 5; the level's
  point lights are not. Which object a light reaches: section 5.5.
- `level-objects.md` section 5.4: the sun-visibility ray goes in direction −d_B (section 3.1), not
  −L_sun_dir.
- `level.md` section 7, chunk-6 lights "serve dynamic lighting of objects": they light nothing; the point
  records enter only the brightness value (section 9); "how the sun record relates to the environment's
  sun": section 3.1.

## 12. Unestablished

- The meaning of the second ignored setting of section 2.2.
- Whether artefact and zone lights ever change type after creation (section 3.2).
- Whether the actor's torch and the helicopter's light are on at the first level's start (section 3.3).
- Library definitions with a distortion element of priority 0 or 1 (section 7).
- The weighting and smoothing of the brightness value (section 9).
- The occlusion test with `level.hom` (section 4 rule 2), as in `level.md`.

## 13. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`, `bin\xrGame.dll`; default
  `__support\userdocs\user.ltx`.
- Data examined: chunks 2 and 6 of the `level` files of all 30 levels; `shaders.xr`; `shaders\r1\*.s`,
  the light programs `shaders\r1\*_point.*`, `*_spot.*`, `common.h`, `tree_w.vs`; the three attenuation
  textures; `config\rspec_*.ltx`; the hanging-lamp records of `spawns\all.spawn` for `l01_escape`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation): the light
  record and its updates, the loading of chunk 6, the per-frame sun update, the per-frame choice of lights,
  the light passes (fade, frusta, traversal, submission rules, constants), the shader classes' light
  elements, the settings, the per-object light tracking, and the light set-up of the game objects; reading
  of the game's shader programs and scripts as data; parsing of the data with throwaway tools written from
  the approved specifications and this text (`classify.py` implements section 6.1 and reproduces the counts
  of section 10). The checks in `checks/dynamic-lights/` reproduce every fact.

## Gaps found in review

- Section 5.5 draws the first-person models in a light pass "with the first-person projection, as the frame draws them"; that projection and which models count are defined by no approved specification (`hud-scope.md` item 26 owns them), so this part of the pass cannot be implemented from the approved specifications alone.
