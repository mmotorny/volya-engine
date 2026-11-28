# Drawing and shading of object models (first renderer)

Behaviour specification of how the game's first (static-lighting) renderer of S.T.A.L.K.E.R.: Shadow of
Chernobyl (Russian edition) draws the skeleton models (`docs/formats/skeleton-ogf.md`) of in-game objects:
how a skinned mesh's vertices are stored for drawing, how they are skinned and where they are drawn at rest,
how a mesh's shader resolves, which element is drawn, the per-object shader constant, and the colour
equations of the model shaders. What could not be established is marked **unestablished**; what is inferred
rather than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/formats/skeleton-ogf.md`: model files, bones and the bind pose (section 4), child meshes (section 5),
  vertex formats (section 6).
- `docs/behavior/object-visual-and-transform.md`: the object's visual and its transform *X* (section 4).
- `docs/behavior/static-shading.md` (here "SS"): `shaders.xr`, scripted shader definitions, texture lists and
  lookup, weather inputs, fog, render-state vocabulary, the precision rules (section 6), drawing order
  (section 10).
- `docs/behavior/object-detail-level.md` (here "ODL"): which of a model's meshes are skipped as too small
  (section 3) and which detail window a skinned progressive mesh draws (section 4).
- `docs/behavior/object-projected-light.md` (here "OPL"): receivers (section 3.1), the per-object sky and sun
  visibility values *h* and *s* (section 5), the projected light texture and its sampling (section 6).
- `docs/behavior/dynamic-lights.md` (here "DL"): dynamic light passes.

Conventions: matrices in the row-vector convention (a point `p` is transformed as `[p 1] · M`); floor(*x*) is
the largest integer ≤ *x*; clamp(*x*, *a*, *b*) limits *x* to *a* … *b*; `lerp(a, b, t)` = a + (b − a)t.

**Checks.** Bracketed ids [MS-*n*] name the checks of the dirty workspace's
`checks/level-objects-split/run_all.py`, which reads `shaders.xr`, the shader programs and scripts of
`shaders\r1\` and the shipped binaries.

## 1. How skinned vertices are stored

When a skinned mesh (types 4 and 5) is loaded, its vertices are converted for drawing in one of these modes:

- **single bone**: a 1-link mesh whose vertices all name the same bone;
- **hardware 1-link**: other 1-link meshes;
- **hardware 2-link**: 2-link meshes;
- **software**: instead of either hardware mode, a mesh whose largest bone index exceeds (*K* − 22) / 3
  (integer division), *K* the number of vertex shader constants the graphics device reports; the vertices
  stay as floats and are skinned on the CPU. With *K* = 256 (believed: any device the renderer runs its
  programs on) the limit is 78 and no shipped model, at most 61 bones (skeleton-ogf.md section 4), uses this
  mode.

In the three hardware modes every vertex is stored quantized; the drawn values are [MS-5]:

| Value | Stored as | Drawn value |
|---|---|---|
| position component *p* | `s16` *q* = clamp(floor(*p* · 2730.5833), −32768, 32767) | *q* · 12 / 32768 |
| normal (also tangent, binormal), first normalised if its squared length exceeds 1.1754944e−38 | per component `u8` *c* = clamp(floor((*n* + 1) · 127.5), 0, 255) | 2 · *c* / 255 − 1 |
| texture coordinate *t* | `s16` *q* = clamp(floor(*t* · 2047.9375), −32768, 32767) | *q* · 16 / 32768 |
| 2-link weight *w* | `u8` *c* = clamp(floor(*w* · 255 + 0.5), 0, 255) | *c* / 255 |

(2730.5833 = 32767 / 12 and 2047.9375 = 32767 / 16, as stored in single precision.) So positions are drawn
within ±12 m of the model origin and texture coordinates within ±16; a component outside is clamped. Among
the files referenced by records only `meshes\equipments\projector.ogf` (positions up to 16.87 m) and
`meshes\physics\anomaly\artefact_ameba2.ogf` (a texture coordinate of 16.08) exceed these ranges; neither is
used on the first level. The vertex programs read the decoded values; a zero normal is drawn as the direction
of (−1, −1, −1) after normalisation. The component order inside the stored normal is believed to be x, y, z
as decoded (the device decodes the packed value as a colour, red = x). Tangent and binormal are not used by
the first renderer's model programs.

## 2. Skinning and the pose at rest

Each bone has a **render matrix** *K* = *B*⁻¹ · *T* where *B* is the bone's bind matrix (skeleton-ogf.md
section 4) and *T* the bone's current model-space matrix (row-vector convention: the bind matrix's inverse
first). A 1-link vertex is drawn at **P** · *K*[*b*] (normal **N** · rotation part of *K*[*b*]); a 2-link
vertex with the blended matrix (1 − *w*) · *K*[*b*₀] + *w* · *K*[*b*₁] (the matrices are blended, not the
transformed points; element 1 of section 4 uses *K*[*b*₀] alone for 2-link vertices). Then the object's
transform *X* applies.

**At rest** every bone's current matrix equals its bind matrix (*T* = *B*), so every render matrix is the
identity and **every vertex is drawn at its stored position**: world = **P** · *X* (with the precision of
section 1). The bone data (chunks 13, 16) is therefore not needed to draw a model at rest. An animated
skeleton model (type 3) draws exactly like a skeleton model (type 10) at rest.

What the running game shows beyond this differs from the rest pose and is outside this specification:
characters, monsters and crows play motions from the first frame (`docs/behavior/motion-playback.md`);
physics objects settle under the physics simulation; hanging lamps play the motion `idle` if their model has
one, and objects with a Skeleton block may play its startup animation (`$editor` means none).

## 3. What is drawn

Each child mesh of an online object's model is drawn with its own shader (chunk 2). Very small meshes are
skipped by the small-part test of ODL section 3; a skinned progressive mesh (type 4) draws the window of its
chunk 6 that ODL section 4 selects (window 0, the full-detail mesh, at full detail).

## 4. Shader resolution

The chunk-2 pair (texture *t*, engine shader *s*) is resolved exactly like an entry `<s>/<t>` of the level's
shader table (SS sections 1 – 4): a scripted definition `shaders\r1\<s with \ → _>.s` if it defines `normal`
or `l_special`, else the `shaders.xr` definition named *s*; texture list T[0] = *t*; texture lookup as SS 4.2.
The model definitions use two library classes not covered by SS. Their sampler `s_base` takes T[0] by
position, as SS 4.1 states for library definitions, whatever the definition's own `Base Texture` name is
(e.g. `models\antigas_glass` names `act\act_glass_universal`; its meshes still sample T[0]) [MS-1].

**Class `MODEL   `** (properties after the common block: boolean `Use alpha-channel` *α*, integer
`Alpha ref` *ref*):

| Element | Programs (vertex / pixel) | Samplers | States |
|---|---|---|---|
| 0 | `model_def_hq` / `model_def_hq` | `s_base` = T[0], `s_lmap` = the projected light texture (section 7) | fog on; depth test and write on; if *α*: blending source-alpha / inverse-source-alpha and alpha test with *ref*, else no blending, no alpha test |
| 1 | `model_def_lq` / `model_def_lq` | `s_base` = T[0] | as element 0 |
| 2, 3 | `model_def_point` / `add_point`, `model_def_spot` / `add_spot` | `s_base`, attenuation textures | dynamic light passes (section 8) |

**Class `MODELEbB`** (environment-mapped; properties: texture `Name` = cube environment map *E*, transform,
boolean `Alpha-Blend` *β*):

| Element | Programs | Samplers | States |
|---|---|---|---|
| 0 | `model_env_hq` / `model_env_hq` | `s_base` = T[0], `s_env` = *E* (clamp), `s_lmap` = projected light texture | fog on; depth test on; depth write = not *β*; if *β*: blending source-alpha / inverse-source-alpha with alpha test ref 0 |
| 1 | `model_env_lq` / `model_env_lq` | `s_base`, `s_env` | as element 0 |
| 2, 3 | as `MODEL   ` | | |

[MS-6] Definitions of `shaders.xr` used by the referenced files: `models\model` (`MODEL`, *α* off, ref 32),
`models\model_aref` (*α* on, ref 128), `models\model_fur` (*α* on, 32), `models\transparent` (*α* on, ref 0,
priority 2, strict sorting); `models\weapons`, `models\artefact`, `models\artefact2`, `models\antigas_glass`,
`models\mirror`, `models\water` (`MODELEbB`, *β* off) and `models\window` (`MODELEbB`, *β* on); *E* is the
definition's own cube map, e.g. `sky\sky_cube_weapons` for `models\weapons` and `models\window` [MS-1].
Scripted (`shaders\r1\models_*.s`): `models\selflight`, `models\selflightl` (`model_def_lqs` / `simple`:
colour = base texel, fog on, no lighting), `models\lightplanes` (`model_def_lplanes` / `base_lplanes`:
additive source-alpha / one, depth write off, fade by |view direction · normal| as in SS 8.2),
`models\xwindows`, `models\pautina`, `models\xdistort`, `models\xdistortcolor`, `models\xdistortcolorl`,
`models\xanomaly`, `models\xmonolith` (effects with distortion or blending, drawn with the scripts' passes;
their appearance is outside this specification) [MS-2].

## 5. Element choice

An object's meshes are drawn with element 0 if the object is a **projected-light receiver** in the frame
(OPL 3.1: among its conditions, the object has a model, answers that it receives shadows, |camera − *C*| + *R*
< 50 · *R* with *C*, *R* the model's header sphere centre transformed by *X* and its radius, and fewer than
100 receivers have been taken in the frame), else element 1.

## 6. The per-object constant

The shader constant **L_dynamic_props** = (0.5 *s*, 0.5 *s*, 0.5 *s*, 0.5 *h*), with *h* and *s* the object's
smoothed sky and sun visibility (OPL section 5) brought to the current frame when the object is drawn (OPL
5.3).

## 7. Colour equations

With **n** = normalize(**N** transformed by the object's rotation), the weather inputs of SS section 9
(L_ambient, L_hemi_color, L_sun_color, L_sun_dir) and the game's programs `shaders\r1\model_def_*.vs/.ps`,
`model_env_*`, `common.h` [MS-3]:

- sun term S(**n**) = L_sun_color · max(0, **n** · (−L_sun_dir));
- **model light** *Q* = (**n**.y · 0.5 + 0.5) · L_dynamic_props.w · L_hemi_color.rgb + L_ambient.rgb +
  L_dynamic_props.xyz · S(**n**); a vertex-stage colour output, so the precision rules of SS section 6 apply.

Element 1 (`model_def_lq`): C = 2 · *Q* · B.rgb, A = B.a (B = base texel at (u, v)) [MS-3].
Environment-mapped (`model_env_lq`): base = lerp(E, B.rgb, B.a) with E the cube texel at the reflection of
the view direction (`calc_reflection` of `common.h`) about **n**; C = 2 · *Q* · base, A = B.a [MS-3, MS-4].

Element 0 (`model_def_hq`): with *t* the texel of the projected light texture at the object's projected
coordinate, light = lerp(*t*.rgb + S(**n**) · *t*.a, *Q*, φ), C = 2 · light · B.rgb, A = B.a (env variant:
base as above). The fade φ = c + (1 − c) · *g*², *g* = (|camera − *C*′| + *R*) / (50 *R*), *C*′ the
receiver's recorded centre *C* + (0, 0.1 *R*, 0), c = console value `r1_lmodel_lerp` (range 0 … 0.333; 0.1
in the shipped `user.ltx` and by default) [MS-6, MS-7]. The projected light texture (a 512 × 512 render
target `$user$projector`), the projected coordinate (the vertex's world position clamped into a box and
mapped into the receiver's cell) and *C*′ are specified in OPL sections 2 – 4 and 6: *t* is the baked light
(rgb) and sun visibility (a) of the static geometry below the object. Drawing receivers with element 1
instead replaces that baked light by *Q*, which indoors (*h* = *s* = 0) is the ambient alone.

Fog, alpha test, blending and drawing order follow SS sections 9.3 and 10 (objects with priority 2 or strict
sorting are drawn with the sorted passes).

## 8. Dynamic lights

With `r1_dlights on` (default) the hanging lamps' lights (`docs/behavior/hanging-lamps.md`) and the other
game-created lights are added to models with elements 2 and 3; which lights reach which object, and the
passes, are specified in DL (sections 4 – 6; the level's chunk-6 point lights are never drawn).

## 9. Unestablished

- The number of vertex shader constants of the device (believed 256, section 1) and the component order of
  the stored normal (believed, section 1).
- The poses shown in the running game (motions, physics): outside this specification (section 2).

## 10. Provenance

- Date: 2026-11-24 (as sections 4.7 – 4.9 and 5 of `level-objects.md`; revised 2026-11-26 after
  object-projected-light.md and dynamic-lights.md); moved into this specification unchanged on 2026-11-27
  when that specification was split by system. Sections 3 and 6 now point to the specifications that settled
  the small-part test, the detail value and the per-object visibility values.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`; default `__support\userdocs\user.ltx`.
- Data examined: `shaders.xr`, `shaders\r1\*` (model programs, `skin.h`, `common.h`, `models_*.s`), every
  `.ogf` file under `meshes\` and `levels\*\meshes\`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation) of the model-file
  loader's skinning modes, the renderer's conversion of skinned vertices (scale constants, rounding, clamping,
  vertex declarations), the model shader classes, receiver selection and fade; reading of the game's shader
  programs and scripts as data.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Section 5 lists only some receiver conditions; `object-projected-light.md` 3.1 adds that an object is taken at most once per frame and only when it lies in a sector (an object outside every sector is not a receiver).
