# Glows

Functional specification of glows in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's
first (static-lighting) renderer: the glows stored in the levels (`level` chunk 7) and the glows of hanging
lamps, searchlights and torches; which glows are considered in a frame, how each glow fades in and out, how
it is tested for occlusion (including the setting `r1_glows_per_frame`), and how it is drawn as a sprite. It is
specification P15 of `docs/behavior/particles-and-lights-scope.md` (cited as `scope`).

It builds on:

- `docs/formats/level.md` (cited as `level`): chunk 2 (section 3), chunk 7 (section 8), the sector of a point
  and the sector traversal (sections 9.1, 9.2);
- `docs/behavior/static-shading.md` (cited as `SS`): the `S_SET   ` pass states (section 7.5), the `shaders.xr`
  definitions (section 3.2), the frame order (section 10.3);
- `docs/behavior/view-projection.md` (cited as `VP`): the view matrix and projection (sections 1.1, 2), the far
  plane (section 2.1), the frame time (section 1);
- `docs/behavior/sky.md`: the sprite triangle order (section 7.4) and the flares drawn after the glows (section 10);
- `docs/behavior/hanging-lamps.md`: the hanging-lamp light placement and on/off rule (sections 4.2, 5);
  `docs/formats/skeleton-ogf.md`: bones (section 4); `docs/behavior/object-visual-and-transform.md`: the object
  transform *X* (section 4); `docs/behavior/offline-simulation.md`: the objects online at the start (section 9.1);
- `docs/formats/sav.md`: the hanging-lamp block (glow texture, glow radius; section 7.4.2);
- `docs/formats/level-cform.md`: the collision triangles of `level.cform`;
- `docs/behavior/options-screens.md`: the `rspec_*.ltx` scripts (section 8).

Conventions:

- Vectors are world-space, metres. *E* is the camera position, **d** the camera's view direction, **u** its up
  vector and **r** its right vector (`VP` section 1.1). *Z* is the far plane of `VP` section 2.1 (the target
  value, including environment modifiers and `rs_vis_distance`, not the smoothed value of `VP` 2.2) [X1, X9].
  *dt* is the frame time in seconds (0 while the game is paused) [X1].
- "Frame" is one rendered frame; each frame has a number that increases by one per frame [X1].
- Statements are static (read from the program and the data; the game was not run). Where the program was not
  followed completely the statement is marked **believed**; what could not be established is marked
  **unestablished**.
- Bracketed ids name the checks that reproduce the statement, all in the dirty workspace's `checks/glows/`:
  `R…` in `binary_r1.py` (the first renderer), `X…` in `binary_x3.py` (the engine and the collision library),
  `G…` in `binary_game.py` (the game library), `D…` in `data.py` (game data), `M…` in `model.py` (numbers
  computed from this text), `X11`–`X14` in `binary_ray.py` (what the occlusion ray tests), `D8` in
  `data_ray.py` (the occlusion ray on `levels\l01_escape\`); `run.py` runs them all.

## 1. What a glow is

A glow has these values:

| Value | Meaning |
|---|---|
| position *P* | world position of the sprite's centre |
| radius *ρ* | half-size of the square sprite, metres |
| direction **g** | optional unit vector; a glow without one is seen from all sides |
| shader | the pass that draws the sprite: an engine shader with its texture |
| on | whether the glow takes part in frames at all |
| fade *f* | 1 … 255; the glow's current brightness |
| occluded | the result of the latest occlusion test (section 5) |
| sector | the level sector the glow is in (`level` section 9.1), or none |

The sphere of a glow, used for culling, has centre *P* and radius *ρ* [R4].

A glow also stores a colour (set by lamps, searchlights and torches from their colour animations); the colour
is never used when drawing (section 6), so it has no visible effect [R4, R7].

## 2. Sources of glows

### 2.1 Level glows (`level` chunk 7)

Each 18-byte record of chunk 7 (`level` section 8) is one glow, created when the level is loaded, in record
order [R2]:

- *P* = the record's position, *ρ* = its radius, no direction;
- shader = the chunk-2 entry the record's shader index names (`level` section 3): its engine shader with its
  texture list. In the shipped levels 1,165 glows use `effects\glow` and one (in `l11_pripyat`) uses
  `effects\flare`; every glow entry names exactly one texture [D1];
- on from loading; *f* = 255; occluded = true; sector none (found at the first gather, section 3) [R2, X5].

Level glows stay on for the whole level; nothing is known to switch them (believed) [R2].

### 2.2 Glows created by objects

An object glow starts off, with *f* = 1, occluded = false, *ρ* = 0.1, no direction, *P* = (0, 0, 0) and shader
`effects\glow` with the texture the object names [R3, R4]. Changes made by its object:

- position: taken only when some axis differs from the current position by at least 0.001; a smaller change
  is ignored [R4];
- radius: taken only when it differs from the current radius by at least 10⁻⁵ [R4];
- direction: normalised; a vector whose squared length is not above the smallest normal single-precision
  value (1.1755·10⁻³⁸) is ignored [R4];
- on/off as below. Switching on, or a taken position or radius change, marks the glow's sector for
  re-detection (section 3) [X5].

**Hanging lamps** (`hanging-lamps.md`). A lamp gets a glow only if its glow texture is non-empty; the glow's
texture is the lamp's glow texture and *ρ* its glow radius (sav.md 7.4.2) [G1]. The glow is on exactly while the
lamp's lights are on (lights on at spawn when health > 0; switched off and on with them) [G1]. While the lamp's
main light is on, each frame the glow's position is set to the main light's position: the translation row of
the light's matrix *M* (`hanging-lamps.md` section 4.2: it is row 3) [G2].
A lamp glow has no direction [G2]. **Disputed in review:** the per-frame placement also requires the lamp's health to be above 0 at that frame, not only the main light being on; while health is 0 the glow keeps its last position. In the data, 20 of the 745 lamp records have a glow texture: 9 with
`glow\glow_ghost` radius 0.3, 7 with `glow\glow_orange` radius 1.0, 4 with `glow\glow_orange` radius 0.3; all
20 exist for the first renderer (light-flag bit 0x04) [D4].

**Searchlights** (the `O_SEARCH` object, `scope` 1.3). The glow's texture and radius are the `glow_texture`
and `glow_radius` of the `projector_definition` section in the model's user data [G3]; for
`equipments\projector.ogf` these are `glow\glow_torch_r2` and 1.0 [D5]. The glow is on exactly while the
searchlight's light is on; the light is switched on at spawn [G3]. While the light is on, each frame, with
*M* = (the current model matrix of the bone named `guide_bone`) · *X*: *P* = row 3 of *M* (translation) and
**g** = row 2 of *M* (the light direction) [G4]. (`equipments\camera.ogf` also has a `projector_definition`, with
`lights\lights_flashlight2` and 0.2 [D5]; whether any object uses it belongs to specification P14.)

**Torches.** The glow's texture and radius are the `glow_texture` and `glow_radius` of the `torch_definition`
section the torch's model supplies [G5]; the shipped `torch_definition` (`config\models\objects\light_night.ltx`)
has `glow\glow_torch_r2` and 0.3 [D5] (how the model reaches that section is **unestablished**, `scope`
section 4). The glow is on exactly while the torch is switched on, also when its owner's settings withhold the
torch's light [G5]. While switched on, each frame [G6]:

- a torch without owner: under a condition of the torch's state that is **unestablished** it is switched off
  (its glow too); otherwise the glow keeps its placement;
- a torch whose owner is a character other than the player (believed: the program tests whether the owner
  is the player's actor): if the owner is less than 100 m from *E*
  (squared distance below 10,000), or a level-wide condition (**unestablished**) does not hold, *M* =
  (current model matrix of the torch model's `guide_bone`) · (the torch's transform); otherwise *M* = the owner's
  transform with its translation replaced by the owner's centre raised by 2/3 of the owner's radius along y.
  *P* = row 3 of *M*, **g** = row 2 of *M* [G6];
- the player's torch: *M* as in the near case above (**Disputed in review:** the player's torch uses the same near/far rule as the case above, decided before the player test: *M* is the owner-transform form when the player is at least 100 m from *E* and the level-wide condition holds; the near form otherwise, which is the usual case); *P* = row 3 of *M* (the torch's light itself sits at
  *P* − 0.2 · row 0 + 0.1 · row 1 − 0.3 · row 2 of *M*); **g** = (−sin *h* · cos *p*, sin *p*, cos *h* · cos *p*), *h* and
  *p* being the torch's smoothed heading and pitch, which follow the player's camera angles with angular speeds
  between 0.5 and 7.5 rad/s over a range of π/6 (the exact smoothing law and the heading offset are
  **unestablished**) [G6].

**Other glows.** Vehicles' headlights also create glows (section `lights`, key `headlights` of the vehicle's
model data) [G7]; they belong to the follow-up scoping specification F1 (`scope` section 3). They are drawn by
the rules below.

## 3. Gathering (each frame)

During the frame's visibility pass, if the camera sector is known (`level` section 9.1) [R8]:

1. Candidates are the glows that are on and whose sphere is not entirely outside the full view frustum
   (believed: a conservative pre-selection), taken in order of increasing squared distance of *P* from *E* [R8].
2. A candidate whose sector is marked for re-detection gets its sector from *P* by the rule of `level`
   section 9.1 (the downward ray; when nothing is found the previous sector is kept, initially none) [R8, X5].
3. A glow with no sector, or whose sector was not visited this frame by the traversal of `level` section 9.2,
   is skipped [R8].
4. Otherwise the glow is **considered** if its sphere is not outside at least one of the frusta its sector
   was reached with. A sphere is outside a frustum when, for some plane (outward normal **n**, offset *k*),
   **n** · *P* + *k* > *ρ* [R8, X7].

The dynamic-object switch that gates this pass is on by default and no shipped console command changes it
[X8].

A considered glow is processed at most once per frame [R1]. With *D*² = |*E* − *P*|²:

- if *D*² < *Z*²: *f* ← min(255, *f* + 4096 · *dt*) if the glow is not occluded, else *f* ← max(1, *f* − 1024 · *dt*);
  the glow is appended to the frame's **glow list**;
- otherwise *f* ← max(1, *f* − 1024 · *dt*) and the glow is not listed [R1].

A glow that is not considered keeps its fade. A full rise takes 0.062 s, a full fall 0.248 s [M2].

## 4. Where glows sit in the frame

Glows are drawn in step 7 of `SS` section 10.3: after the strict-sorted passes, before the lens flares and the
sun's gradient (`sky.md` section 10) [R9]. In that step, if the glow list is not empty: the occlusion tests of
section 5 run, then the glows are drawn (section 6), then the list is emptied [R5, R6].

## 5. Occlusion tests and `r1_glows_per_frame`

A counter *k*, set to 0 when the level is loaded and never reset, cycles through the list. With *L* the length
of this frame's glow list (in gathering order) and *N* = `r1_glows_per_frame`, up to *N* times [R5]:

1. take the glow at list position *k* mod *L*; if it was already tested this frame, stop;
2. if *D* = |*P* − *E*| > 10⁻⁷: occluded ← (the ray from *E* towards *P*, length *D*, hits something, section 5.1);
   otherwise the occluded value is kept;
3. *k* ← *k* + 1.

So at most min(*N*, *L*) glows are tested per frame, each at most once, and the new result affects the fade
from the next frame's gathering on [R1, R5].

`r1_glows_per_frame` is an integer console variable, range 2 … 32, default 16 [R10]; all six `rspec_*.ltx`
scripts set it to 16 [D7].

### 5.1 The ray

The ray hits when either holds [X3]:

- **objects**: it hits the collision form of an object, other than the camera's current view entity [X2], whose
  collision form is of the skeleton kind. Objects get such a form when their configuration section has a
  `cform` line, and the game's breakable objects, physics objects, hanging lamps, torches and physics skeleton
  objects create one [X4, G8]. A skeleton form is hit when the ray passes its bounding sphere and hits the shape of
  one of its bone elements (box, sphere or cylinder, in the current pose; elements without a shape are
  skipped) from either side [X4]. The bone shapes and their placement are those of the model's bones
  (`skeleton-ogf.md` section 4); the intersection tests are the usual ray–box, ray–sphere and ray–cylinder tests
  (believed).

  Only objects in the level's spatial index that are marked collidable are considered [X13]: an object enters
  the index when it is spawned on the client (comes online) and leaves it when destroyed (goes offline); it
  also leaves it while attached to a parent object (e.g. an item carried in an inventory or held in hand) and
  re-enters when detached [X13]. It is marked collidable while it is enabled and has a collision form, and
  unmarked while disabled [X13]. Being hidden (not drawn) does not affect this [X13]. Which game objects are
  disabled, and when, is decided per object kind by the game library and is **unestablished** [X14]. The
  material flags play no part in the object test (a skeleton form has no materials) [X4];
- **static**: it hits a triangle of the level's collision mesh (`level.cform`) at a distance *t* with
  0 < *t* ≤ *D*, from either side (no back-face culling) [X3, X10].

**Every triangle of `level.cform` is tested; none is skipped, by material or otherwise** [X11, X12]. The mesh the
ray is cast against is built at level load from all the file's vertices and triangles; the only change made to
it is the material mapping of `level-cform` section 3.3 (which rewrites the attribute bits 0–15 and removes no
triangle) [X11, X12]. The triangle test does not read the material or any attribute bit; in particular the
triangles of materials with the **pass-through** flag 0x80 (`materials\water`, `materials\bush`,
`materials\fake`, `materials\death`), with the **actor obstacle** flag 0x1000 (`materials\fake`), or with the
flags 0x400 / 0x800 (attribute bits 14 / 15) occlude glows like any other triangle [X12]. So the static test,
expressed against `gamemtl.xr`'s flags, is: a triangle occludes whatever its material's flags are.

On `levels\l01_escape\` the camera at the New Game start (`VP` 2.3: eye (−246.72630, −23.263261, −134.43387)) is
close to `materials\fake` triangles: rays from it to the level glows cross coincident pairs of `fake` triangles
0.75 … 1.69 m from the eye for 12 of the 22 level glows. The static part alone gives, for that eye (section 7,
[D8]): 20 of the 22 level glows hit, 2 not hit (glows 7 and 20); of the 20, glows 6 and 21 are hit only by
`fake` triangles, the other 18 also by triangles of other materials. Glows are therefore not all occluded once
tested at the start (level glows start occluded, section 2.1): glows 7 and 20 have a clear static ray, and whether they are then occluded depends only on the
object part.

Each glow remembers its last static test: ray start, direction, length, result, and the triangle hit. When the
start differs by less than 0.001 on every axis, |1 − **dir** · **dir**ₚᵣₑᵥ| < 10⁻⁵ and the length differs by less
than 10⁻⁵, the remembered static result is reused; otherwise if the remembered triangle is hit again at
0 < *t* < *D* the result is "hit"; otherwise the full test is made and remembered [X3]. Doing the full test
every time differs only within those tolerances. The object part is tested every time [X3].

## 6. Drawing

The listed glows are drawn sorted by shader (glows sharing a shader in one batch); with the additive blending
below the order does not change the result [R6]. For each listed glow with *f* > 1 [R6]:

1. *c* = 1 if the glow has no direction (or its squared length ≤ 10⁻⁵); otherwise *c* = **g** · (*E* − *P*) / |*E* − *P*|.
   The glow is skipped unless *f* · *c* > 1.
2. *z* = |**d̂** · (*P* − *E*)| (**d̂** = **d** normalised); *t* = clamp((*z* − 0.2) · 20/3, 0, 1): 0 up to 0.2 m in
   front of or behind the camera, 1 from 0.35 m [R6, M3]. The glow is skipped unless *f* · *t* · *c* > 1.
3. *a* = (1 − *D*² / *Z*²) · *f* · *t* · *c*; the byte *b* = the integer part of *a* (0 … 255) [R7, M1].

The sprite is a square of half-size *ρ* centred at *P*, facing the camera [R7]:

| Corner | Position | Texture coordinate |
|---|---|---|
| 0 | *P* − *ρ***u** − *ρ***r** | (0, 1) |
| 1 | *P* + *ρ***u** − *ρ***r** | (0, 0) |
| 2 | *P* − *ρ***u** + *ρ***r** | (1, 1) |
| 3 | *P* + *ρ***u** + *ρ***r** | (1, 0) |

drawn as the triangles (0, 1, 2) and (3, 2, 1) (clockwise on screen, so not removed by the default culling)
[R7, X6, M4], world matrix identity, with the camera's view and projection; every vertex has the colour
(*b*, *b*, *b*, *b*) / 255 in all four channels [R7].

The pass is the glow's shader (`SS` section 7.5): colour = texel · *b*/255, alpha *A* = texel alpha · *b*/255;
blending source-alpha / one, i.e. the frame gains texel.rgb · *b*/255 · *A*; texture clamped, linear filtering;
no depth write, no fog. For `effects\glow`: alpha-test reference 0 (pixels with *A* · 255 ≤ 0 discarded), depth
test on. For `effects\flare`: reference 2, depth test off [D3]. Glows are thus hidden per pixel by nearer
geometry (for `effects\glow`) in addition to fading out when occluded.

## 7. Verification facts

All 30 shipped levels [D1]:

- 1,166 glows; every chunk 7 is a non-empty multiple of 18 bytes; engine shaders `effects\glow` 1,165,
  `effects\flare` 1 (`l11_pripyat`, texture `glow\fx_volume1`); radii 0.1 … 5.0.
- The 11 textures named: `glow\glow_white` (495 glows), `glow\glow_red2` (375), `glow\glow_04` (275),
  `fx\fx_volumefog1` (8), `glow\glow_orange` (3), `glow\fx_volume1` (3 with `effects\glow`, 1 with
  `effects\flare`), `glow\glow_blue` (2), `glow\glow_ghost`, `glow\glow_green`, `flare\flare_08`,
  `glow\glow_orange_bright` (1 each).
- Every glow texture (level, lamp, searchlight, torch) is in `textures\`, none in a level directory; the
  winning copies are in `gamedata.dbb`. `glow\glow_*`, `glow\fx_volume1`: 256 × 256 DXT5 with 9 mip levels;
  `fx\fx_volumefog1` 128 × 128 DXT5; `flare\flare_08` 32 × 32 DXT1; `lights\lights_flashlight2` 8 × 8 DXT5
  [D6].

`levels\l01_escape\` [D2, D4]:

- Static occlusion ray (section 5.1) from the New Game eye (−246.72630, −23.263261, −134.43387) to each of the
  22 level glows (in chunk 7 order, numbered from 0), all triangles tested [D8]: not hit for glows 7 and 20
  (both at (−249.74, −21.28, −134.44), *D* = 3.6); hit only by `materials\fake` triangles (a coincident pair at
  *t* = 0.87) for glows 6 and 21 (both at (−244.53, −21.92, −134.42), *D* = 2.6); the nearest hit is a
  `materials\fake` pair for glows 2, 3, 6, 8, 9, 10, 13, 14, 15, 16, 17, 21; the other 18 hit glows are also hit
  by non-`fake` triangles. The level has 1,516 `materials\fake` triangles [D8].

- 22 level glows: `effects\glow/glow\glow_04` 12 (radius 1.0), `effects\glow/glow\glow_red2` 6 (0.5),
  `effects\glow/glow\glow_white` 4 (two 0.7, two 0.5); sum of radii 17.4; glow 0 at (30.484, 19.756, 679.066),
  radius 1.0.
- The 5 lamps online at the start (`offline-simulation.md` 9.1) all have glows: the 3 `physics\light\lampa_01_on` with
  `glow\glow_orange` radius 0.3, the 2 `physics\decor\tv_1` with `glow\glow_orange` radius 1.0.

`shaders.xr` [D3]: `effects\glow` — `S_SET   `, blending 5, clamp, alpha ref 0, Z-test on, Z-write off, fog off;
`effects\flare` — `S_SET   `, blending 5, clamp, alpha ref 2, Z-test off, Z-write off, fog off.

## 8. Unestablished

- How the torch model reaches `torch_definition`; the condition switching off an owner-less torch; the
  level-wide condition of the far torch case; the player's torch heading offset and aim smoothing law
  (section 2.2).
- Whether the full-frustum pre-selection of section 3 step 1 can drop a glow that step 4 would keep (believed
  not); the exact ray–shape tests for object bone shapes; which objects the game disables, and when (which
  removes them from the object part of the ray) (section 5.1).
- Whether any object uses `equipments\camera.ogf`'s `projector_definition` (specification P14).

## 9. Provenance

- Date: 2026-11-24; section 5.1 (which triangles and objects the ray tests) revised 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`;
  binaries `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`, `bin\xrCDB.dll`, `bin\xrGame.dll`.
- Data examined: `level` (chunks 2 and 7) of all 30 shipped levels; `levels\l01_escape\level.cform` and
  `gamemtl.xr` (the occlusion ray at the start); `shaders.xr`; the hanging-lamp records of
  `spawns\all.spawn`; the user data of `meshes\equipments\projector.ogf` and `camera.ogf`;
  `config\models\objects\light_night.ltx`; `config\rspec_*.ltx`; the archive listings and DDS headers of the
  glow textures.
- Analysis: static analysis (Ghidra, headless) of the four binaries; parsing of the data with throwaway tools
  written from the approved specifications listed above and from this text. The checks in `checks/glows/`
  reproduce every statement.

## Gaps found in review

- Section 5.1: an object is marked collidable when it is enabled while it has a collision form; whether a collision form given to an object that is already enabled also marks it is not stated.
