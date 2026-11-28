# Shadow-casting lights and projected-texture lights (first renderer)

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of two specifications of the approved `docs/behavior/particles-and-lights-scope.md`
(cited as `scope`):

- **Part A, Shadow-casting lights** (P17): what the hanging-lamp light-flag bit 0x02 and every other shadow
  setting change in the first renderer. The light records' own shadow setting changes nothing (section 2);
  the first renderer's only shadows are **object shadows**: dark patches that game objects cast, from the
  lights near them, onto the level's static surfaces (sections 3 – 7). Part A states which settings and data
  decide them and specifies them in full.
- **Part B, Projected-texture lights** (P18): how a light's projected texture (the lamps' light and ambient
  textures, the searchlight's and the torch's `spot_texture`) changes what the light lights in the first
  renderer: it changes nothing (section 8).

Builds on (not repeated): `docs/behavior/dynamic-lights.md` (cited as `DL`: the light record, its fields,
flag bits, bounding sphere and updates (section 2), the chunk-6 records and the two directional records A
and B (3.1), the lights game objects create (3.2), the camera's pass (4) and the light passes (5, 6)),
`docs/behavior/hanging-lamps.md` (cited as `HL`), `docs/behavior/torch.md` (cited as `TO`),
`docs/behavior/static-shading.md` (cited as `SS`: shader resolution and elements (3), drawing order
(10.3), the projected-size measure and ssa_min (10.2)), `docs/behavior/offline-simulation.md` (which
objects are online (6.5), those of the first level at the start (9.1)), `docs/behavior/object-visual-and-transform.md`
(the object transform *X*, 4), `docs/formats/skeleton-ogf.md` (cited as `SK`: model files and
their bounding spheres (2)), `docs/behavior/model-shading.md` (cited as `MS`: shading of objects, element choice
(5)), `docs/behavior/object-projected-light.md` (cited as `OPL`: the projected light texture (2), the sun
visibility *s* (5)), `docs/behavior/object-detail-level.md` (cited as `ODL`: detail level of a part (4)), `docs/formats/level-cform.md` (cited as
`CF`: `level.cform`, the loaded triangle bits (3.3), `gamemtl.xr`), `docs/formats/level.md` (sectors),
`docs/behavior/options-screens.md` (cited as `OS`), `docs/behavior/hud-scope.md`,
`docs/formats/sav.md` (spawn records and their class ids), `docs/formats/ogf.md`.

Conventions: as `DL` (vectors are rows; `·` dot, `×` cross product; `sat(x)` = clamp to [0, 1];
`normalize(v)` = v / |v|; a matrix M maps a point p by (p, 1)·M). E is the camera position, "frame" one
drawn image, Δt the frame time in seconds (as `OPL` 5.3). A light record's position is P, direction D,
range r, colour (c_r, c_g, c_b); its **type** is directional, point or spot (`DL` 2.1). The records A and B
of `DL` 3.1 are directional; every other record is point or spot. An object's **model sphere** is the
bounding sphere of its model (`SK` 2) with centre transformed by the object transform *X*: centre C,
radius R. Every fact carries the name of its check, a script in `checks/light-shadows-projection/` of the
dirty workspace (`run_all.py` runs them all; `gen.py`, run first, produces the decompilations the binary
checks read). Facts marked **believed** are inferred rather than read from the program or the data.

## 1. Overview

1. Shadow settings of light records (section 2): the shadow setting a lamp (bit 0x02), a torch, a
   searchlight, an artefact or a zone gives its light record is stored and never read. The settings
   `r2_sun`, `r2_sun_details`, `r2_ls_squality` and the other `r2_*` options of the video screens change
   nothing in the first renderer. No setting switches object shadows on or off.
2. Object shadows (sections 3 – 7), each frame:
   1. during the camera's pass, the objects drawn whose class casts shadows become **casters**, each with
      the list of its **model parts** that have an element 4 (section 3);
   2. each object keeps a **light list**: the lights near it that can see it, with an energy, refreshed now
      and then (section 4); every light record counts, the level's chunk-6 point lights and the sun record
      B included;
   3. after the camera's pass, for each caster and each light of its list with enough energy, at most 36
      per frame, the caster's parts are drawn (black, for model shaders) into one 85 × 85 tile of a white
      512 × 512 target, seen from the light; the target is then blurred into the **shadow texture**
      (section 5);
   4. later in the frame each such shadow is projected from its light onto the triangles of
      `level.cform` it reaches and multiplies the image there by the shadow texture plus a fade
      (sections 6, 7).
3. Projected textures (section 8): the name is handed to a setting of the light record that stores
   nothing; spot lights always project the fixed attenuation texture of `DL` 6.4.

## 2. The light records' shadow setting and the shadow-related options

### 2.1 The shadow setting

A light record's shadow setting is one bit of its flag word; setting it stores the bit and nothing else, and
no part of the first renderer ever tests it (the only flag it reads back is the active state). It therefore
changes neither the light passes of `DL` 5 – 6 nor object shadows (sections 3 – 7), which use lights
whatever their shadow setting. This holds for every source that sets it (`DL` 3.2): the hanging lamp's main
light (light-flag bit 0x02, `HL` section on the light flags) and ambient light (off), the searchlight (on),
the torch's main light (on) and second light (off), artefact and zone lights (on), the chunk-6 records
(off) and lights of other game objects. Checks: `bin_settings.py` LS-S1, LS-S2.

Data (`data.py` LS-D1): 627 of the 745 hanging-lamp records, on 18 levels, have bit 0x02. Of the 36 lamps
for the first renderer (light-flag bit 0x04, `HL`), 12 have it: `lights_alarm_1_0000` of `l03_agroprom`,
`l08u_brainlab`, `l10u_bunker` and `l12u_sarcofag`; `lights_alarm_1_0000` … `_0003` of
`l03u_agr_underground`; `lights_alarm_1_0001` of `l08u_brainlab`; `trader_light_0003` of `l12_stancia_2`;
`light_alarm_glass_0027` of `l12u_control_monolith`; `light_sharnir_1_0001` of `l12u_sarcofag`. The five
first-renderer lamps of `l01_escape` have it clear. With or without it, a lamp's light behaves alike.

### 2.2 Options

- `r2_sun`, `r2_sun_details` and the other on/off `r2_*` options are bits of one flag word that the first
  renderer registers as console values; it reads the word only in its console commands, never for drawing.
  `r2_ls_squality` (the options screen's shadow-quality slider, `OS`) is registered and never read. Each
  of these changes nothing in the first renderer (`bin_settings.py` LS-S3). The shipped `user.ltx` has
  `r2_sun on`, `r2_sun_details off`, `r2_ls_squality 1.` (`OS`).
- The first renderer has no option, console value or command-line switch for object shadows; the only
  strings of it that name shadows are the resource names of section 5 (`bin_settings.py` LS-S5).
- Object shadows depend on these settings and data only: the HUD flags 0x04 (`hud_weapon`) and 0x40 for
  the actor in first-person view (section 3.2); the materials with flag 0x400 for the receiving surfaces
  (section 6.2). `r1_dlights` and `r1_dlights_clip` do not take part: nothing of sections
  3 – 7 reads them (`bin_track.py` LS-T10).

## 3. Casters

### 3.1 Objects drawn in the camera's pass

During the camera's pass (`DL` 4, `level.md` section 9), the renderer handles the objects the camera's
query returns, sorted by the distance of their spatial sphere centres from E, nearest first (the order of
equal distances is **unestablished**). Before them it calls the two first-person callbacks of section 3.2
and updates light lists (section 4.1). Then, for each object that passes the visibility tests of `DL` 4
(sector, frustum, occlusion), it marks the object as the one being drawn, draws it, and clears the mark.
Marking an object while not in first-person mode makes it a **caster candidate** (`bin_casters.py` LS-C1,
LS-C4). A candidate becomes a caster when all of these hold (`bin_casters.py` LS-C2):

1. its class casts shadows (section 3.3);
2. the renderer is not in first-person mode (the mode used for the first-person items, `DL` 5.5);
3. it has not been a caster yet in this frame;
4. with C, R its model sphere: D = |C − E| + R satisfies (4 / (R + 10⁻⁵)) · D / 48 < 1, i.e.
   D < 12 · (R + 10⁻⁵). Example: R = 1 casts while D < 12.00012, R = 0.5 while D < 6.00012
   (`numeric.py` LS-N1).

A caster records C, D and the object, and is the **current caster** until the mark is cleared or the next
candidate is marked; a candidate that fails a test ends any current caster.

### 3.2 The actor in first-person view

The game's HUD calls two callbacks during the camera's pass, before the objects of 3.1
(`bin_casters.py` LS-C4, LS-C5):

- The first, when all of these hold: HUD flags 0x04 and 0x40 are both set; the HUD has been loaded (always
  during play); the level's current entity exists and is the actor; the actor is in first-person view (the
  camera follows the actor, the actor's camera 0, configured by the section `actor_firsteye_cam`, is active
  (1 and 2 are `actor_look_cam` and `actor_free_cam`), and it is not attached to a vehicle or mounted
  weapon, or the one it is attached to answers yes to two tests of its own; which holders pass them is
  **unestablished**). Then the renderer is put in **invisible mode**, the actor is marked as the
  object being drawn (so it is a caster candidate as in 3.1) and drawn, and invisible mode is left. In
  invisible mode a model part is recorded for the current caster (3.4) and then not drawn. So the actor's
  body casts its shadow without being seen.
- The second draws the first-person items in first-person mode; nothing it draws is a caster or a part.

HUD flag 0x04 is `hud_weapon` (on in the shipped `user.ltx`; the game also sets it when it registers its
console commands). Flag 0x40 is set in the initial HUD flags (0x6E0) and again when the actor object is
created; the game never clears either bit with a mask (the engine overwrites the whole flag word only in
three places, one of them the writing of capture images; **believed** outside normal play). So in play the
actor's first-person shadow is shown exactly while `hud_weapon` is on (`bin_settings.py` LS-S6). Whether
the camera's pass also draws the actor's body in first-person view is outside this specification
(**unestablished** here); if it does, the actor is a caster through 3.1 only once per frame (rule 3), its
parts having been recorded by the first callback.

### 3.3 Which classes cast

An object's class answers the "cast shadow" question; game objects answer yes by default, other drawn
things (particle effects) no (`bin_casters.py` LS-C6). By the class ids of the spawn records (`sav.md`;
class ids registered by `scripts\class_registrator.script` are taken to behave as the class the
registration names as client class, **believed**) (`data.py` LS-D5):

| Class ids | Cast |
|---|---|
| `AI_CROW`; `C_HLCPTR`, `C_HLCP_S` (helicopter); `O_BRKBL` (breakable objects); every artefact id: `ARTEFACT`, `SCRPTART`, `AF_MBALL`, `AF_BDROP`, `AF_NEEDL`, `AF_BAST`, `AF_BGRAV`, `AF_DUMMY`, `AF_ZUDA`, `AF_THORN`, `AF_FBALL`, `AF_EBALL`, `AF_RHAIR`, `AF_GALAN`, `AF_GRAVI` | no |
| `O_ACTOR` | yes while not attached to a vehicle or mounted weapon |
| every other class id of the object factory (monsters, characters, items, weapons, outfits, physical objects, hanging lamps, zones, searchlights, vehicles, …) | yes |

Over all levels the spawn records count 7,565 of casting classes, 699 of non-casting ones, the actor, and
376 `SMRTTRRN` and `RE_SPAWN` records without a model. On `l01_escape`: 708 casting, 111 not (`AI_CROW` 55,
`O_BRKBL` 49, `ARTEFACT` 3, `SCRPTART` 3, `C_HLCP_S` 1), the actor, 50 without a model (which of them are
online at the start: `offline-simulation.md` 9.1). An object without a model is never drawn and never casts.

### 3.4 Parts

While a caster is current and the renderer is not in first-person mode, every model part the object draws
is recorded for the caster with the object matrix current at that moment, if (`bin_casters.py` LS-C3):

1. it passes the tests by which the camera's pass draws a part: at most once per traversal, projected-size
   measure above ssa_min (`SS` 10.2), and its shader has an element for the pass (`MS` 5) in an enabled
   priority group; and
2. its shader has an element 4.

Element 4 by shader (`bin_gen.py` LS-G8; `SS` 3 for resolution and the scripted modules' `l_special`):

| Shader | Element 4 |
|---|---|
| library `MODEL   `, `MODELEbB` | the shadow element (5.5) |
| library `LM      `, `LM_AREF `, `LmEbB   ` | programs `lmap_l` / `lmap_l` |
| library `V       `, `V_AREF  ` | programs `vert_l` / `vert_l` |
| library `BmmDold ` | programs `impl_l` / `impl_l` |
| library `PARTICLE`, `S_SET   ` | the class's only pass (the same for every element) |
| library `D_TREE  `, `D_STILL ` | none |
| scripted module with `l_special` (`shaders\r1\*.s`: `details_lod`, `effects_water`, `effects_waterryaska`, `effects_waterstuden`, `models_pautina`, `models_xanomaly`, `models_xdistort`, `models_xdistortcolor`, `models_xdistortcolorl`, `models_xdistortcolorlinv`, `models_xdistortinv`, `models_xglass`, `models_xglass2`, `models_xlens`, `models_xmonolith`, `models_xwindows`, `particles_xadd`, `particles_xdistort`, `selflight`) | that function's pass |
| scripted module without `l_special` | none |

The scripted modules with `l_special` are listed by `data.py` LS-D6. Every element 0 – 5 of a library
definition is compiled (`bin_gen.py` LS-G8); a part drawn into a shadow
uses its element 4 as it is, so a part with a scripted distortion or glass element is drawn into the shadow
with that element's programs and states.

## 4. The light list of an object

Every object with a model keeps a list of (light record, energy, colour), sorted by energy, largest first
(the order of equal energies is **unestablished**). Casters use it in section 5. It is updated by the
**full update** below; between full updates it is kept as it is.

### 4.1 When the full update runs

Only in the camera's pass (`bin_track.py` LS-T1; `bin_casters.py` LS-C4), at most once per frame per
object:

- for the level's current entity (in play, the actor), every frame, before the objects are handled;
- for one other object per frame: the object at position (frame counter mod N) of the camera query's
  sorted result list (N its length, counting every entry; the counter advances by 1 each frame), if that
  entry is an object that passes the visibility tests of 3.1.

So an object's list follows the lights only when it is the actor or every N-th frame.

### 4.2 The full update

With C, R the object's model sphere, the **sample point** is C′ = C + (0, 0.3 R, 0) (`bin_track.py`
LS-T2). The update first refreshes the object's sky and sun visibility as `OPL` 5.2 (the sun visibility *s*
used below). The list is rebuilt only when the object's class answers yes to "cast shadow" or to
"receive shadow" (every class except `AI_CROW` and the particle effects answers yes to the latter,
`bin_casters.py` LS-C6) and the object's light flag is set (object flags start as 7 and the game sets only 7)
(`bin_track.py` LS-T3):

1. **Gather**: every active light record whose bounding sphere's cube (centre ± radius on each axis,
   `DL` 2.3) overlaps the cube C′ ± R (bounds inclusive) and whose position satisfies |C′ − P| < r + R
   is **tracked**; a light already tracked is kept, a new one starts with t = 0 and e = 0. Every active
   record counts, point and spot alike, including the chunk-6 point records (`DL` 3.1): the query and this
   test do not look at the excluded mark, `r1_dlights` or the shadow setting (`bin_track.py` LS-T4). The
   directional records A and B are never active (`DL` 3.1); the sun enters only by step 4.
2. **Visibility**: for each tracked light, a random unit vector u is drawn: z = cos(π k₁ / 32767),
   x = cos(2π k₂ / 32767) · √(1 − z²), y = sin(2π k₂ / 32767) · √(1 − z²), with k₁ then k₂ the next two
   integers 0 … 32767 of the renderer's random generator (s ← 214013 s + 2531011 modulo 2³², value =
   (s >> 16) & 0x7FFF; its state and seed are **unestablished**). A ray from P towards C′ + 0.5 R · u, as
   long as the distance to that point, is tested against the static collision (as the sky rays of `OPL`
   5.2, the object itself excluded). Then t ← clamp(t + (free ? 4 : −2) · Δt, −0.5, 1) and
   e ← 0.9 e + 0.1 t (`bin_track.py` LS-T5).
3. **List**: each tracked light with (c_r + c_g + c_b) / 3 · e > 10⁻⁵ is listed with energy 0.5 e and
   colour 0.5 e · (c_r, c_g, c_b); for a light without the excluded mark (every light created by a game
   object, `DL` 3.2) energy and colour are halved again (0.25 e). Lights that the gathering did not find
   are dropped with their t and e (`bin_track.py` LS-T6, LS-T9).
4. **Sun**: if (c_r + c_g + c_b) / 3 · *s* > 10⁻⁵ for record B's colour, B is listed with energy *s* and
   colour 0.5 *s* · its colour (`bin_track.py` LS-T7).
5. The list is sorted by energy, largest first (`bin_track.py` LS-T8).

Example: a light seen at once, updated every frame at 60 frames/s, reaches the shadow threshold of 5.2
(energy ≥ 0.05) at the 9th update if created by a game object, at the 6th if a chunk-6 light
(`numeric.py` LS-N7).

## 5. Making the shadows of a frame

### 5.1 When

After the camera's pass has handled every object (and before the objects' projected light texture of
`OPL` 2 is prepared), once per frame. The two render targets used are made when a level loads, whatever
the settings: `$user$temp` and `$user$shadow`, 512 × 512, 32-bit A8R8G8B8 (`bin_gen.py` LS-G1).

### 5.2 Which shadows

Casters are taken in the order they became casters (3.1, 3.2); casters without parts are skipped. For each,
the entries of its object's light list are taken in list order; an entry gives a **shadow** when its energy
is at least 0.05 and the light frustum of 5.3 exists. Shadows are numbered i = 0, 1, … in the order they
are made; once 36 exist, no more are made this frame (`bin_gen.py` LS-G2).

### 5.3 The light's frustum

With C, R the caster's model sphere (C as recorded in 3.1) (`bin_gen.py` LS-G3 – LS-G5):

- **Light point L and range ρ**: a directional record: L = C − 100 · D, ρ = 120. Otherwise L = P and
  ρ = r; while |C − L| ≤ 0.001, L.y is raised by 0.01; then, if d = |C − L| < R + 0.1, L is moved away from C
  along the line C – L to distance exactly R + 0.1.
- With d = |C − L|: α = R / d, n = d − R − 0.002, f = min(ρ, d + max(R, 4.5)). The shadow exists only if
  n ≥ 0.002, n + 0.002 ≤ f, α ≤ 0.9 and α ≥ 0.01.
- **View V**: the view of `DL` 5.3 with eye L, forward F = normalize(C − L) and up hint U = (0, 1, 0), or
  (1, 0, 0) when 1 − |F.y| < 10⁻⁵ (rows (X.x, Y.x, F.x, 0), (X.y, Y.y, F.y, 0), (X.z, Y.z, F.z, 0),
  (−X·L, −Y·L, −F·L, 1) with Y = normalize(U − (U·F) F), X = Y × F).
- **Projection Q**: rows (1/α, 0, 0, 0), (0, 1/α, 0, 0), (0, 0, k, 1), (0, 0, −k n, 0) with k = f / (f − n).
- The shadow's matrix is M = V · Q.

A spot light is handled like a point light here and in section 7: only its position and range are used,
not its direction or cone (`bin_gen.py` LS-G3, `bin_recv.py` LS-P5: the only test is for the directional
type).

A directional light is always at d = 100, so only casters with R ≥ 1 get a sun shadow. Examples: d = 5,
R = 0.5, ρ = 10: α = 0.1, n = 4.498, f = 9.5; directional, R = 1.5: α = 0.015, n = 98.498, f = 104.5
(`numeric.py` LS-N2).

### 5.4 Drawing a shadow

All shadow drawing is done with depth testing off (`bin_gen.py` LS-G6). Before the first shadow of a
frame, `$user$temp` becomes the target and is cleared to white (0xFFFFFFFF). Shadow i is drawn into the
viewport x = 85 · (i mod 6), y = 85 · ⌊i / 6⌋, 85 × 85 pixels, depth range 0 – 1, with view V and projection Q:
each recorded part of the caster with its recorded object matrix as world matrix and its shader's
element 4 (`bin_gen.py` LS-G6, LS-G7). A type-4 (progressive) mesh is drawn with the detail argument −1,
whose meaning is **unestablished** (`ODL` 4).

### 5.5 The shadow element

Element 4 of `MODEL   ` and `MODELEbB` definitions: programs `model_def_shadow` (`shaders\r1\`, the skinning
variants of the model programs; output: the position projected by the world-view-projection matrix and
colour 0) and `model_shadow` (output: that colour); no fog, depth compare always, no depth write, blending
zero / source colour, no alpha test (`bin_gen.py` LS-G8). Drawn into the white tile, every covered pixel
becomes 0 (black, alpha 0).

### 5.6 The blur

If at least one shadow was made this frame, the shadow texture `$user$shadow` becomes the target and one
quad is drawn over it with the program pair of `shaders\r1\blur4.s` (`null` / `avg4`: fog off, no depth
test or write, otherwise the script defaults of `SS` 3.4; four samplers on `$user$temp`, each clamped
with linear filtering), whose
pixel is the mean of its four samples (`bin_gen.py` LS-G10). The quad is given in pixel positions (no
transform) with corners (ε, 512), (ε, ε), (512, 512), (512, ε), ε = 10⁻⁵, and four coordinate pairs per
corner; at a pixel of position (x, y) the pairs are, with q = (x, y) / 512,

    q + (−1, −1)/2048,  q + (5, 5)/2048,  q + (5, −1)/2048,  q + (−1, 5)/2048.

Under the Direct3D 9 rule that a pixel's centre is at integer coordinates and texel k's centre at
(k + 0.5) / 512 (documented), the result at pixel (i, j) is the source filtered with weights 0.375, 0.25,
0.375 on texels i − 1, i, i + 1 per axis (2-D: corners 0.140625, edges 0.09375, centre 0.0625; source
clamped at the border) (`numeric.py` LS-N4). Pixel column 0 and row 0, whose centres lie outside the quad,
keep their previous content (**believed**, by the same rule); the shadow texture's content before the first
blur is **unestablished**. If no shadow is made in a frame, nothing is projected (section 7) and the
texture keeps its content.

### 5.7 What a shadow keeps

Each shadow keeps the object, i, C, M, the light record and the entry's energy (`bin_gen.py` LS-G9). The
light record's colour is read again when the shadow is projected.

## 6. Receiving triangles

### 6.1 The cache

For each (object, light record) pair the renderer keeps a set of receiving triangles with the object's
position (row 4 of its matrix) and the light's position P at the time it was built, and the time it was
last used. When a shadow is projected, the set of its pair is reused if both positions differ from the
kept ones by less than 0.001 on every axis; otherwise it is rebuilt (6.2) and the positions are kept anew.
Either way its time becomes now (the device's millisecond clock). After each projection pass, sets unused
for more than 30,000 ms are dropped (`bin_recv.py` LS-P4). A reused set keeps triangles chosen with an
earlier M; the coordinates of 7.2 always use the current M.

### 6.2 Building the set

The shadow's frustum is −w ≤ x ≤ w, −w ≤ y ≤ w, −w ≤ z ≤ w for (x, y, z, w) = (p, 1)·M (six planes)
(`bin_recv.py` LS-P2). The triangles of the level's static collision mesh (`level.cform` as loaded, `CF`
3.3) that the collision query returns for this frustum are taken, and for each triangle (vertices v0, v1,
v2 in file order) (`bin_recv.py` LS-P3):

1. it is skipped if its loaded bit 14 is set, i.e. its material has flag 0x400 (`CF` 3.2–3.3:
   `materials\bush`, `materials\death`, `materials\fake`, `materials\fake_ladders`);
2. N = (v0 − v1) × (v0 − v2); skipped if |N|² < 10⁻⁷; n = N / |N|;
3. skipped unless n · (P − v0) ≥ 0 (the light record's position P on the side n points to; for a
   directional record P is the position of `DL` 3.1);
4. it is clipped by the six planes (a convex polygon); if the polygon has k ≥ 3 vertices p0 … p(k−1), the
   triangles (p0, p(j−1), p(j)) for j = 2 … k − 1 are added, each with n.

The query is believed to return at least every triangle that intersects the frustum, so the clipped set does
not depend on how coarse it is (**believed**).

Data (`data.py` LS-D4): 115,596 of the 1,375,470 triangles of `l01_escape` never receive object shadows;
924,518 over all 30 levels; none in `l12u_control_monolith`.

## 7. Projecting the shadows

### 7.1 Place in the frame

After the sky and clouds, the light passes (`DL` 5.1) and the renderer's wall-mark pass (its marks are
outside this specification), before the blended billboard pass (step 4 of `SS` 10.3), the priority 2–3 passes and the flares; once per
frame, for every shadow of section 5 in order (`bin_recv.py` LS-P1).

### 7.2 Vertices

The world matrix is the identity; the camera's view and projection are used with the projection's
element at row 3, column 2 (the 15th value, row-major from 1) lowered by 0.002 for this pass, then restored
(a depth offset towards the camera) (`bin_recv.py` LS-P6). For shadow i with light record λ, energy w and
caster centre C, κ = (c_r + c_g + c_b) · w / 3 with λ's current colour. Every triangle of its set with
n · (E − p0) ≥ 0 is drawn; at each of its vertices p (`bin_recv.py` LS-P5):

- the light term: directional λ: ℓ = max(0, n · (−D)) · κ; otherwise, if |p − P|² ≤ r²:
  ℓ = (1 − 1 / (|p − P| + 1)) · n · normalize(P − p) · κ when positive, else 0 (normalising only when
  |P − p|² exceeds the smallest normal float); beyond the range, 0;
- a = sat(|E − p|² / 2304) (48 m), b = sat(|C − p|² / 20.25) (4.5 m);
- the shade v = 255 · (1 − (1 − b) · (1 − a) · ℓ · 1.5), converted to an integer (truncation toward zero for
  non-negative values; negative values end up as 32 through the clamp below);
- the triangle is skipped if all three shades are above 248; otherwise each shade is clamped to 32 … 255
  and the vertex colour is (v, v, v, v) / 255 (alpha too);
- the texture coordinates, with (x, y, z, w) = (p, 1)·M:
  u = (x / w + 1) · 85 / 1024 + (i mod 6) / 6 + 1 / 1024,
  v = (1 − y / w) · 85 / 1024 + ⌊i / 6⌋ / 6 + 1 / 1024.

The coordinates span 85 texels per shadow but start 0.5 + (i mod 6) / 3 texels to the right of (and
0.5 + ⌊i / 6⌋ / 3 below) the tile's first pixel, as drawn in 5.4 (`numeric.py` LS-N3). Triangles are sent
as lists in batches of 256 (`bin_recv.py` LS-P7).

Examples (`numeric.py` LS-N5): sun shining straight onto the surface, κ = 0.6, next to the camera and the
caster: v = 25, clamped to 32 (the darkest); the same 24 m from the camera and 1.5 m from C: 101; a white
lamp light of energy 0.25, 2 m away, facing the surface: 191; at 48 m from the camera or 4.5 m from C: 255.

### 7.3 Pixels

The shader is `effects\shadow_world` on the shadow texture (its library definition, class `SH_WORLD`,
priority 1, `data.py` LS-D7, compiles one fixed pass) (`bin_recv.py` LS-P6):

- depth test less-or-equal, no depth write; no lighting, no fog;
- colour = sat(T.rgb + c.rgb), alpha = sat(T.a + c.a), with T the shadow-texture texel at (u, v), linear
  minification, magnification and mip filtering (the address mode is not set by the shader:
  **unestablished**), and c the interpolated vertex colour;
- blending destination colour / zero: the image becomes image × colour.

Where the caster covered its tile (T = 0) the image is multiplied by c, at most darkening to 32/255 of
its value; where it did not (T = 1) the image is unchanged; the blurred border lies between
(`numeric.py` LS-N6). The pass sets no culling of its own (`SS` 10.1 applies, **believed**); the triangles
it draws face the camera and appear clockwise on screen, so that culling keeps them (`numeric.py` LS-N8).

### 7.4 At the first level's start

The lights that can enter light lists on `l01_escape` at the start are the 40 chunk-6 point records, the
seven lamp records (`DL` 3.3), the sun record B and the lights of P20 and F1 if on (`DL` 3.3). The five
lamps' shadow bit is clear (2.1); it would change nothing. Casters are the online objects of casting
classes (3.3) within the distance rule of 3.1, and the actor in first-person view with `hud_weapon` on.

## 8. Part B: projected textures

### 8.1 Where the names go

The game hands each of these names to the light record as its projected-texture setting (`DL` 2.2) (`bin_proj.py` LS-X1,
LS-X2):

| Source | Name | Record |
|---|---|---|
| hanging lamp (`HL`) | the record's light texture | main light |
| | the record's ambient texture | ambient light (only with light-flag bit 0x20) |
| torch (`TO`) | `torch_definition` `spot_texture` (`internal\internal_light_torch_r2`) | main light |
| searchlight | `projector_definition` `spot_texture` of `meshes\equipments\projector.ogf` (`internal\internal_light_torch_r2`) | its light |

In the first renderer that setting, and the other setting without effect of `DL` 2.2, do nothing: the name is not kept (`bin_settings.py` LS-S1,
`bin_proj.py` LS-X3). The light record's other settings set or read its type, active state, shadow
setting, position, direction, cone, range and colour, or release it, and one more does nothing; none
keeps a name (`bin_proj.py` LS-X4).

### 8.2 What a projected texture changes

Nothing. A spot light's pass projects `internal\internal_light_att` (with `internal\internal_light_attclip`
for depth) whatever the light (`DL` 6.2 – 6.4; every spot pass names these fixed textures, `bin_proj.py`
LS-X3); a point light's pass uses `internal\internal_light_attpoint`. Object shadows (sections 3 – 7) do
not use a texture of the light. The objects' projected light texture `$user$projector` (`OPL` 2) cannot
depend on a light's texture name, since none is kept. A spot light lights the same geometry, with the same
colour, as it would without a projected texture; so does a point light (a lamp's ambient light).

### 8.3 Data

(`data.py` LS-D2, LS-D3) 36 hanging-lamp records name a light texture, all `lights\lights_spot_wire_02`,
all with a spot main light; of the first-renderer lamps only `light_sharnir_1_0001` of `l12u_sarcofag`.
Four records name an ambient texture: `trader_light_0000` and `trader_light_0003` of `l01_escape` and
`trader_light_0003` of `l12_stancia_2` (`lights\lights_duolight02grey`), `trader_light_0005` of
`l01_escape` (`lights\lights_bspot01`); all four are first-renderer lamps, and none has an ambient light
(bit 0x20), so these names are not even handed over (`data.py` LS-D6). `meshes\equipments\camera.ogf`'s
`projector_definition` names `lights\lights_blamplight2`.

## 9. Statements of earlier specifications settled here

- `DL` 2.5 leaves to P17 "what else" the shadow setting may change: nothing (section 2.1).
- `DL` 9 states that the per-object light tracking feeds only a brightness value that is not used for
  drawing: the light list it builds (section 4) is also what object shadows are made from (sections 5 – 7);
  its weighting and smoothing are section 4.2.
- `scope` 2.1 P17 names the lamp bit "shadow-casting": the first renderer's shadows do not depend on it
  (sections 2, 3 – 7).

## 10. Verification facts

### 10.1 First level

- The five first-renderer lamps: bit 0x02 clear; three of them name an ambient texture but have no
  ambient light, none names a light texture (`data.py` LS-D1, LS-D2, LS-D6).
- Spawn records by cast answer (3.3): 708 cast, 111 do not, the actor, 50 without a model.
- `level.cform`: 115,596 triangles (materials 37 `bush` and 80 `fake`) receive no object shadow.

### 10.2 All levels

- 627 of 745 lamp records have bit 0x02; 12 of the 36 first-renderer lamps.
- Receiving triangles excluded: 924,518 over the 30 levels.
- 132 classes of objects and effects answer the cast question: 112 yes, 18 no, the actor (two classes,
  single-player and multiplayer) conditionally (`bin_casters.py` LS-C6).

### 10.3 Worked values

Sections 3.1, 4.2, 5.3, 5.6, 7.2 and 7.3 give examples (`numeric.py` LS-N1 … LS-N8).

## 11. Unestablished

- Tie orders: objects at equal distance (3.1), list entries of equal energy (4).
- Which vehicles and mounted weapons let the actor keep the first-person view when attached (3.2).
- Whether the camera's pass also draws the actor's body in first-person view (3.2).
- The state and seed of the renderer's random generator (4.2).
- The detail argument −1 for progressive meshes (5.4).
- The shadow texture's content before the first blur, and (**believed**) its row and column 0 (5.6).
- The address mode of the shadow texture in the projection (7.3), and (**believed**) that the projection
  pass is culled as `SS` 10.1.

## 12. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`, `bin\xrGame.dll`; default
  `__support\userdocs\user.ltx`.
- Data examined: the hanging-lamp records and the class ids of all records of `spawns\all.spawn`;
  `level.cform` of all 30 levels; `gamemtl.xr`; `shaders.xr`; `shaders\r1\*.s`, `blur4.s`,
  `model_def_shadow.vs`, `model_shadow.ps`, `avg4.ps`; the user data of
  `meshes\equipments\projector.ogf` and `camera.ogf`; `config\models\objects\light_night.ltx`;
  `scripts\class_registrator.script`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation): the light
  record's entries, the renderer's settings, the per-object light tracking, the choice of casters and parts,
  the making, blurring and projection of object shadows, the shader classes' elements, the game's object
  classes and their cast answers, the HUD's first-person callbacks and flags, the light set-up of lamps,
  torches and searchlights; reading of the shader programs and scripts as data; parsing of the data with
  throwaway tools written from the approved specifications and this text (`numeric.py` implements the
  formulas of sections 3 – 7). The checks in `checks/light-shadows-projection/` reproduce every fact.

## Gaps found in review

- Element 4 of the library classes `LM      `, `LM_AREF `, `LmEbB   `, `V       `, `V_AREF  `, `BmmDold ` (programs `lmap_l`, `vert_l`, `impl_l`) and the single pass of `PARTICLE` and `S_SET   ` (section 3.4): their pass states (blending, depth, alpha test, samplers) and what the programs output are given by no approved specification, so a model part with such a shader cannot be drawn into a shadow tile (5.4) without guessing.
- Section 4.1 picks the round-robin object by its position in the camera query's sorted result list, counting every entry; which kinds of entries that list holds besides objects (light records, effects) is not stated, so which object is refreshed in a given frame is not fully determined.
