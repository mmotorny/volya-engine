# Sound occlusion

Specification S6 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`): how the
level's geometry between a sound and the listener lowers the sound's volume, and the format of
`level.som`. It defines the **occlusion target** *o\** that the volume step of S2
(`docs/behavior/sound-positional.md` section 5.3, step 4) moves each emitter's occlusion *o* towards, and
the same geometry's effect on the loudness reported to AI hearers. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- S2, `docs/behavior/sound-positional.md`: emitters, 3D and 2D, the listener position *L* (the camera's eye
  position), the emitter position *p*, the max distance *M*, the volume step, the occlusion *o* (rate 1 per
  second) and the output volume *V*; S1, `docs/formats/sound-ogg-params.md` (reports to the AI).
- `docs/formats/level-cform.md` sections 2.1 – 2.3 (`level.cform`: vertices, triangles, the front normal)
  and 3.3 (the load-time material mapping).
- `docs/formats/chunks.md` (the chunk container).
- `docs/formats/ltx-config.md` (`config\system.ltx`; reading a missing key is fatal).
- `docs/behavior/level-loading.md` (`$level$` is the loaded level's directory).

Conventions: little-endian; `u32` unsigned 32-bit integer, `f32` IEEE 754 single; `float3` three `f32`
(x, y, z). Positions are world coordinates in metres. For vectors, · is the dot product and × the cross
product. A **face** is a triangle (*v*₀, *v*₁, *v*₂) with the **face normal** *N* = (*v*₁ − *v*₀) ×
(*v*₂ − *v*₀) (not normalised; it points to the same side as the front normal of `level-cform.md`
section 2.3).

**Checks.** Every factual claim carries a tag. The checks are scripts in the dirty workspace,
`checks/sound-occlusion/`: `export.py` regenerates the evidence in `ev/` (headless Ghidra exports of the
shipped program files); `run_all.py` runs `check_bin.py` (tags B1 – B15: the program files),
`check_data.py` (D1 – D4: the archives, decoded with `som.py`, a reader written from section 2) and
`model.py` (M1 – M5: an implementation of sections 3 – 5 written from this text, reproducing the worked
examples of section 7).

## 1. Overview

- Two meshes occlude sound [B6, B4]:
  - the **level mesh**: every triangle of the level's `level.cform`;
  - the **sound occlusion mesh**: the faces of the level's `level.som`, each with its own **transmission
    value**; most levels have no such file (section 6).
- For each 3D emitter the occlusion target is computed along one straight segment from the listener to a
  point near the emitter (section 4). Any crossing of the level mesh multiplies the target by the
  **occlusion scale** (0.4 in this edition) once; each crossed face of the sound occlusion mesh multiplies
  it by that face's transmission value [B10, B12, D1].
- The occlusion only scales the volume (through S2); it does not filter, delay or redirect the sound
  [B8, B10, B12].

## 2. `level.som`

### 2.1 Layout

`levels\<level>\level.som` is a chunked file (`chunks.md`) [B1]:

| Chunk | Payload |
|---|---|
| 0 | `u32` at offset 0, read and not used. The chunk must exist: a file without it is a fatal error [B1] |
| 1 | a sequence of 44-byte **polygon records**, read from the start of the payload to its end [B1] |

No other chunk is read [B1]. Polygon record [B1]:

| Offset | Size | Field |
|---|---|---|
| 0 | 12 | `float3` corner *a* |
| 12 | 12 | `float3` corner *b* |
| 24 | 12 | `float3` corner *c* |
| 36 | 4 | `u32` **two-sided**: 0 = one-sided, any other value = two-sided |
| 40 | 4 | `f32` **transmission value** *t* |

- Every polygon gives the face (*a*, *b*, *c*) with value *t*; a two-sided polygon also gives the face
  (*c*, *b*, *a*) with the same *t*, in this order (the polygon's face first) [B1]. So a two-sided polygon
  has one face whose normal points to each side.
- What the program does with a missing chunk 1, a chunk 1 whose length is not a multiple of 44, or a
  chunk 0 shorter than 4 bytes is **unestablished**; no shipped file has one [D3].

### 2.2 Building the mesh

- The faces' corners are merged into a vertex list in face order, corner order *a*, *b*, *c* of each face:
  a corner whose x, y and z each differ by less than 0.01 from those of a vertex already in the list uses
  the first such vertex; otherwise it is appended [B2]. The faces are kept in order, none removed
  [B2, B3]. Merging moves a corner by less than 0.01 on each axis; it happens in two shipped files (section
  7) [D2].
- The mesh needs at least 4 vertices (after merging) and at least 2 faces; otherwise loading is a fatal
  error [B3]. Every shipped file meets this [D3].

## 3. When the meshes are present

- **Level mesh.** When a level's collision is loaded from its `level.cform`, the sound system is given that
  mesh: all its triangles, regardless of material, with the vertex positions as stored (the material
  mapping of `level-cform.md` section 3.3 does not change positions or remove triangles) [B6, B3]. When the
  level's collision is destroyed (the level is left), the sound system has no level mesh [B6]. Objects of
  the level (doors, physics objects, characters) are not part of it [B6].
- **Sound occlusion mesh.** When a level is loaded and `$level$level.som` exists, it is read (section 2)
  and replaces the previous sound occlusion mesh [B4, B1]. If the file does not exist, nothing is changed.
  The sound occlusion mesh is otherwise released only when the sound system itself ends [B5]. So after a
  level without `level.som` is loaded, the sound occlusion mesh of the last level that had one stays in
  effect (believed consequence: not executed). An implementation that wants the original's behaviour keeps
  it; clearing it on leaving a level differs only in that case.
- **Occlusion scale** *k*: read once at engine start-up from `config\system.ltx` section `[sound]`, key
  `occlusion_scale`, and limited to 0.1 … 0.5 (a value below 0.1 becomes 0.1, above 0.5 becomes 0.5);
  a missing key is fatal (`ltx-config.md`) [B7]. No console command, script or other code sets it [B7].
  This edition sets 0.4, so *k* = 0.4 [D1].

## 4. The occlusion target

### 4.1 When it is computed

- At every volume step of a 3D emitter whose distance to the listener is at most its max distance *M*
  (S2 section 5.3, step 4); not for an emitter beyond *M*, and not for a 2D emitter [B8].
- At the start of every emitter (S2 section 4.2), from its position *p* [B8]; for a 2D emitter that value has
  no effect (S2 uses *o* = 1 for it).
- Each computation is independent and random (section 4.2), so *o\** may change from update to update
  near the edge of an obstacle; *o* follows at 1 per second (S2), which smooths it [B8, B9].

### 4.2 The segment

1. Two pseudo-random integers *r*₁, *r*₂, each uniform in 0 … 32,767, are drawn from the program's shared
   generator (any uniform source will do; the original's sequence depends on all other users of the
   generator) [B9]. With θ = *r*₁ · π / 32,767 and φ = *r*₂ · 2π / 32,767, the **jitter** is
   *j* = 0.2 · (cos φ · sin θ, sin φ · sin θ, cos θ), where sin θ = √(1 − cos² θ) [B9]. (θ is uniform in
   0 … π, so the jitter's directions are not uniform over the sphere.)
2. The **target point** is *x* = *p* + *j*; the segment runs from *L* to *x*; its length is *D* = |*x* − *L*|
   and its direction *d* = (*x* − *L*) / *D* [B9].

### 4.3 Crossings

A face (*v*₀, *v*₁, *v*₂) with normal *N* is **crossed** by the segment when the ray *L* + *s* · *d*
meets the face's plane at a point inside the face, edges included, with 0 < *s* ≤ *D* [B11]. The point is
inside when, with *e*₁ = *v*₁ − *v*₀, *e*₂ = *v*₂ − *v*₀ and the determinant *g* = *e*₁ · (*d* × *e*₂)
(= −*d* · *N*), the point's barycentric coordinates *u* (along *e*₁) and *w* (along *e*₂) satisfy *u* ≥ 0,
*w* ≥ 0, *u* + *w* ≤ 1 (the standard ray–triangle intersection of Möller and Trumbore, 1997) [B11]. The
determinant also decides which faces can be crossed [B11]:

- **two-sided test** (level mesh): |*g*| ≥ 10⁻⁵; a face nearly parallel to the segment is never crossed;
- **one-sided test** (sound occlusion mesh): *g* ≥ 10⁻⁵, i.e. the segment runs against the face normal:
  only when the listener is on the side the normal points to.

### 4.4 The value

    o* = (k if the segment crosses at least one triangle of the level mesh, else 1)
         × the product of t over every face of the sound occlusion mesh that it crosses (one-sided test)

[B10, B12, B11]

- Without a level mesh the first factor is 1; without a sound occlusion mesh the product is 1 [B10, B12].
- The level-mesh factor is applied once however many triangles are crossed [B10]. Each crossed face of the
  sound occlusion mesh counts once; a segment through a shared edge of two faces crosses both (edges are
  included) and so multiplies by *t* twice (believed consequence, not executed) [B11, B12].
- For a two-sided polygon only one of its two faces passes the one-sided test for a given segment, so it
  multiplies by *t* once from either side [B1, B11]. A one-sided polygon multiplies by *t* only when the
  listener is on its normal's side.
- *t* = 0 silences the sound through that polygon: *o* falls to 0 at 1 per second and the emitter becomes
  inaudible (S2) [D2].

**The remembered triangle (an optimisation).** The original keeps, per emitter, the last level-mesh
triangle a full search found crossed (none for a new emitter) [B13, B8]. Each computation first tests that
triangle with the one-sided test and 0 < *s* < *D*; if it is crossed, the full search of the level mesh is
skipped and the factor *k* applies; otherwise the level mesh is searched (two-sided test), and a crossed
triangle, if found, becomes the remembered one [B13]. A crossing of the remembered triangle under these
conditions is also a crossing under section 4.3, so the result equals section 4.4 (believed, up to
floating-point rounding at the limits); an implementation may omit it.

## 5. Effects

- **Volume.** S2 uses *o*: *V* = 0.1 · (*b* · *v* · *c* · *φ* · *o*) + 0.9 · *V* at each volume step, and
  *o* = *o\** at the start [B8]. So an occluded sound is quieter, has a lower voice priority (S2 section 6)
  and, below *V* = 0.01, is inaudible. The occlusion does not change the distance attenuation of S2
  section 7.4.
- **Timing of the AI reports.** The report timing and range of S2 section 5.5 do not use the occlusion
  [B14].
- **Loudness heard by AI hearers.** For each AI hearer that a report reaches (S24), the loudness it is
  given is multiplied by the sound occlusion mesh's value along a segment from the hearer's position to
  the sound's position with a fresh jitter of radius 0.2 (section 4.2, with the hearer in place of *L*): the
  product of *t* over the crossed faces (one-sided test), 1 without a sound occlusion mesh [B15]. The level
  mesh and the occlusion scale take no part [B15]. A hearer is told of the sound only if the loudness so
  multiplied is above 10⁻⁷ [B15]. The loudness before this factor, and what hearers do, are S24's.

## 6. Which levels have `level.som`

8 of the 30 level directories have `level.som`; every level has `level.cform` [D2]:
`l01_escape`, `l04_darkvalley`, `l04u_labx18`, `l05_bar`, `l06_rostok`, `l10u_bunker`, `l11_pripyat`,
`testers_mp_factory`.

## 7. Verification facts

**All shipped `level.som` files** [D2, D3]: chunk 0 (4 bytes, value 0) and chunk 1 only, in that order,
neither compressed; chunk 1 is (file size − 20) bytes, a multiple of 44; two-sided flags are 0 or 1;
transmission values 0 … 0.75; no polygon repeats a corner.

| Level | Size | SHA-256 (first 16 hex digits) | Polygons | Two-sided | Values *t* | Faces | Vertices after merging | Corners moved by merging |
|---|---|---|---|---|---|---|---|---|
| `l01_escape` | 1,384 | `e8af487e6869eeb4` | 31 | 0 | 0.6 | 31 | 20 | 0 |
| `l04_darkvalley` | 1,956 | `32693f9728227dd2` | 44 | 44 | 0, 0.6 | 88 | 86 | 6 |
| `l04u_labx18` | 32,536 | `37469c922b3c1c4d` | 739 | 739 | 0.75 | 1,478 | 524 | 6 |
| `l05_bar` | 4,244 | `7c06daee65c2e83e` | 96 | 96 | 0, 0.6 | 192 | 93 | 0 |
| `l06_rostok` | 768 | `8601501ceeff66d4` | 17 | 17 | 0.05 | 34 | 18 | 0 |
| `l10u_bunker` | 76,756 | `983a8e0cf8508818` | 1,744 | 2 | 0, 0.75 | 1,746 | 867 | 0 |
| `l11_pripyat` | 1,076 | `be389d6ee3d4e226` | 24 | 24 | 0, 0.6 | 48 | 48 | 0 |
| `testers_mp_factory` | 15,860 | `a7fa77fc81368fc7` | 360 | 360 | 0.6 | 720 | 318 | 0 |

(Values are the stored `f32`, e.g. 0.6 is 0.60000002.)

**`levels\l01_escape\level.som`** [D4]: 31 one-sided polygons, all *t* = 0.6, forming a box without a
floor: bounds (−296.52, −67.47, −164.98) … (−229.65, −18.09, −93.28); every face normal points out of the
box; 19 roof faces (normalised normal y > 0.8), none facing down; total area 18,036.31 m². The start eye
position (−246.72630, −23.263261, −134.43387) (`start-position.md`) lies within the bounds. So a sound
inside this box heard from outside is multiplied by 0.6 (on top of any level-mesh crossing); a sound
outside heard from inside is not affected by it.

**Worked examples** on `l01_escape`, computed by `model.py` from sections 4 and 5 with *k* = 0.4 and the
jitter *j* = (0, 0, 0.2) (*r*₁ = 0, any *r*₂) unless stated [M1 – M5]:

| Case | Result |
|---|---|
| jitter for *r*₁ = 16,383, *r*₂ = 8,191 | *j* ≈ (0.0000288, 0.2, 0.0000096) [M1] |
| listener at the start eye position, emitter `ambient\sparks1` of `level.snd_static` at (−245.3441, −23.2075, −135.9194) | 2 level triangles crossed, no face: *o\** = 0.4 [M2] |
| listener (−246.7, 0, −134.4) above the box, same emitter | 6 level triangles, one roof face: *o\** = 0.4 · 0.6 = 0.24 [M2] |
| listener (−246.7, 10, −134.4), emitter (−246.7, 0, −200) | nothing crossed: *o\** = 1 [M2] |
| listener (−200, −40, −110), emitter (−260, −40, −110) | no level triangle; one side face, entering the box: *o\** = 0.6 [M3] |
| listener (−260, −40, −110), emitter (−200, −40, −110) | the side face is passed from behind: *o\** = 1 [M3] |
| AI hearer query, hearer (−200, −40, −110), sound (−260, −40, −110); and hearer (−246.7, 0, −134.4), sound at `sparks1` | 0.6 each (no level-mesh factor) [M4] |
| `l04_darkvalley` polygon 0 (two-sided, *t* = 0), segment through its centroid along its normal, 2 m each side, no jitter | 0 from both sides [M4] |
| *o* from 1 towards *o\** = 0.24 with *dt* = 0.016 s | 0.984 after 1 update, 0.248 after 47, 0.24 from update 48 on [M5] |

## 8. Unestablished

- The handling of a `level.som` without chunk 1, with a chunk 1 length not a multiple of 44, or with a short
  chunk 0 (section 2.1).
- That the previous level's sound occlusion mesh stays in effect on a level without `level.som` is believed
  from static analysis, not executed (section 3).
- That the remembered triangle never changes the result is believed (section 4.4).
- Double counting at shared edges is believed (section 4.4).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives
  `gamedata.db0` … `gamedata.dbd`, and the shipped program files of `bin\`.
- Data examined: every `level.som` and `level.cform` of the archives' level directories,
  `config\system.ltx` and every other `config\*.ltx` file (for the key `occlusion_scale`).
- Analysis: static analysis (decompilation and disassembly with Ghidra, headless) of the shipped sound
  library, collision library, engine executable and game library; reading constants from them. No program
  was executed. Public documents used: T. Möller and B. Trumbore, "Fast, Minimum Storage Ray/Triangle
  Intersection", Journal of Graphics Tools 2(1), 1997.

## Gaps found in review

- The occlusion target when the target point coincides with the listener position (segment length 0) is
  unestablished.
- Whether an emitter's remembered level-mesh triangle (section 4.4) is kept when the level mesh is
  replaced, so that it could apply *k* without a crossing of the new mesh, is unestablished; an
  implementation that omits the remembered triangle follows section 4.4 exactly.
