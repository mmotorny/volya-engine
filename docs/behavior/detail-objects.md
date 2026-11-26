# Detail objects in the first renderer (behaviour)

Behaviour specification of how the first (static-lighting) renderer, `renderer_r1`, turns the slot grid
of `level.details` into placed instances of the detail models (grass, weeds, flat litter) and draws them:
the cache of slots around the camera and when slots are built and rebuilt, the placement inside a slot,
the effect of `r__detail_density`, the distance and size limits, the sway caused by the wind, the shader
programs and states, and the place of the pass in the frame. It is proposed specification 3 of
`docs/behavior/r1-settings-scope.md` (section 2.4 there). What could not be established is marked
**unestablished**; what is inferred but not shown is marked **believed**, with the reason.

Conventions:

- Abbreviations of specifications: LD `docs/formats/level-details.md` (the file format: header, models,
  slot records; its field names are used here), CF `docs/formats/level-cform.md`, SS
  `docs/behavior/static-shading.md`, WA `docs/behavior/weather-ambient-effects.md`, DL
  `docs/behavior/dynamic-lights.md`, LL `docs/behavior/level-loading.md`, RS
  `docs/behavior/r1-settings-scope.md`.
- Positions are in metres in the level's world frame, y up. A **cell** (*cx*, *cz*) is the world square
  x ∈ [2*cx*, 2*cx* + 2], z ∈ [2*cz*, 2*cz* + 2]; its slot record is found as in LD 4.1 (record
  (*cx* + offset_x, *cz* + offset_z); outside the grid, a record whose four ids are 63).
- `f32` is IEEE single precision. "floor" and "ceil" are the usual roundings toward −∞ and +∞; "trunc"
  rounds toward zero. *E* is the camera position.
- A **generator** is a 32-bit state *q* advanced as *q* ← *q* · 214,013 + 2,531,011 (mod 2³²), each draw
  returning (*q* ≫ 16) AND 32,767, a value 0 … 32,767 [DO-5]. "*r*" denotes one draw. "The shared
  generator" is one used by the whole program, whose state is not reproducible (an implementation may use
  any uniform source for it).
- "Frame" counts the engine's frames; "the frame number" increases by one per frame.

**Checks.** Every claim carries a tag [DO-*n*]. The checks are one script in the dirty workspace,
`checks/detail-objects/run.py` (with `model.py`, a placement model written from this text, and `vec.py`,
the test vectors of section 8). It reads saved read-only static-analysis output of the shipped
`xrRender_R1.dll`, `XR_3DA.exe`, `xrCDB.dll` and `xrGame.dll`, constants read directly from the binaries,
and the extracted game data (`level.details`, `level.cform`, `shaders.xr`, the shader program files,
`config\rspec_*.ltx`, the shipped `user.ltx`). All 50 checks pass.

## 1. Overview

1. When a level loads, its `level.details` is read (LD) and a square **cache** of 49 × 49 cells centred on
   the camera's cell is set up; every cell of it is marked **pending** (section 2). **Disputed in review:**
   it is set up centred on cell (0, 0) and moved to the camera's cell by the first update (2.2).
2. A pending cell is **built**: its instances are placed by sampling a regular grid of positions,
   thinned by the slot's densities through a fixed dither matrix, and dropped onto the level's collision
   triangles (section 4). Builds happen a few per frame, nearest first (section 3).
3. When the camera moves into another cell, the row or column of the cache that falls behind is reused for
   the new cells ahead, which become pending (section 2.2).
4. Every frame the cells in view within 47.5 m are selected; each instance is kept or dropped by its
   projected size, and is drawn shrunken with distance, either still or with one of two sway motions
   driven by the wind (sections 5, 6).

## 2. The cache

### 2.1 Loading

- The file is optional: without it the level has no detail objects and the pass of section 6 never runs
  [DO-1].
- `level.details` is read in the `st_loading_details` stage of level loading (LL) and released when the
  level is unloaded [DO-1]. At the same time the ten sway values of `config\system.ltx` section `[details]`
  are read (WA 5.2) and the cache is filled (2.2) [DO-1].
- Nothing of this is redone on a video restart (`vid_restart`): the file, the cache and the built
  instances survive it [DO-1].

### 2.2 Cells of the cache and how it follows the camera

- The camera's cell is (*c*ₓ, *c*_z) = (floor((*E*.x + 1) · 0.5), floor((*E*.z + 1) · 0.5)) [DO-2]. (So the
  camera may stand in that cell or in the one before it: cell *c* covers x from 2*c* − 1 to 2*c* + 1 for this
  purpose.)
- The cache holds 49 columns × 49 rows. Column *k* (0 … 48) holds cell x = *c*ₓ − 24 + *k*; row *m* (0 … 48)
  holds cell z = *c*_z + 24 − *m* [DO-2]. At load the cache is centred on the camera cell of that moment
  and every entry is initialised (2.3). **Disputed in review:** at load the cache is centred on cell (0, 0),
  whatever the camera's position, and every entry is initialised for that centre; the first update then
  steps the centre to the camera cell as below. Because every entry is already pending, all 2,401 are still
  built in that first update; but if the camera cell is (0, 0) at that update the centre does not change,
  so the groups (2.4) keep their initial state (empty) until the camera first changes cell.
- When the camera cell differs from the cache's centre, the centre moves toward it one step at a time
  (first along x until equal, then along z). Each step of +1 in x reuses column 0 as the new column 48
  (cells x = new *c*ₓ + 24), −1 in x column 48 as the new column 0; likewise in z (+1: row 48 becomes row 0,
  holding z = new *c*_z + 24; −1: row 0 becomes row 48). Every entry of the reused column or row is
  re-initialised for its new cell (2.3) [DO-2]. All other entries keep their built instances.
- Hence a cell's instances, once built, are kept for as long as the cell stays within the 49 × 49 square;
  they are built again only after the cell has left the square and come back [DO-2, DO-3].

### 2.3 Initialising an entry

For the cell (*cx*, *cz*) with record words w0 … w3 (LD 4.2) [DO-3]:

- its **box** is x from 2*cx* − 0.001 to 2*cx* + 2.001, z from 2*cz* − 0.001 to 2*cz* + 2.001, y from
  *y*_low − 0.001 to *y*_top + 0.001, with *y*_low = base · 0.2 − 200 and *y*_top = *y*_low + height · 0.1
  (`f32` constants 0.2, 200, 0.1, 0.001);
- it is **empty** if all four part ids are 63 (cells outside the grid are empty);
- its instances are discarded;
- unless it is already pending it is appended to the **pending list** and marked pending, empty or not
  (an empty entry is taken off the list without building, 4.1).

A fresh entry (before the first initialisation) counts as empty, not pending, and due for its first
refresh (5.3) at once [DO-3]. Initialising an entry does not change the frame at which its next refresh
(5.3) is due, nor the lists of instances it handed out for drawing at its last refresh (5.3).

### 2.4 Groups

For culling, the cache's columns 0 … 47 and rows 0 … 47 are split into 12 × 12 **groups** of 4 × 4
entries; column 48 and row 48 belong to no group and are therefore never drawn [DO-2]. (Those cells lie
47 m or more from the camera along one axis.) **Disputed in review:** column 48 (the +x edge) lies 47 m or
more away along x, but row 48 (the −z edge, cells z = *c*_z − 24) lies only 45 m or more away along z,
so cells of row 48 whose centres are within 47.5 m (5.3) are also never drawn. Whenever the camera cell has changed in an update, each
group's box is recomputed as the union of its 16 entries' boxes (in the state they have at that moment),
its sphere as that box's centre and half diagonal, and the group counts as empty when all 16 entries are
empty [DO-4]. A group's box is not updated when an entry of it is built later without a cell change.

## 3. When entries are built

In every update of the cache (section 5.1), after the camera-cell steps of 2.2 [DO-4]:

- if all 2,401 entries are pending, all are built;
- otherwise at most 7 are built: each time the pending entry whose box centre is nearest to *E* (squared
  distance; the first in list order on a tie) is built and removed from the list, the others keeping their
  order.

So after a level load the whole cache is built in the first update; after a camera move of one cell, the
49 new entries are built over 7 frames, nearest first. A jump of 49 cells or more along x or z re-initialises
every entry, so all 2,401 are pending and are built at once; after a smaller jump the re-initialised entries
are built 7 per frame, nearest first [DO-2, DO-4].

## 4. Building an entry: placement

### 4.1 Inputs

Building clears the pending mark. An empty entry is not built. Otherwise [DO-5]:

- The **triangles** are all triangles of the level's collision form (CF 2) that intersect the entry's box
  (2.3), the box and triangle being closed sets (the exact separating-axis test: the box's three axes, the
  triangle's plane and the nine cross products of box axes and triangle edges) [DO-5]. **Believed**: the
  collision form used is the whole of `level.cform` as loaded (CF). If there is no such triangle, nothing
  is placed and the entry keeps its box.
- **Count** *n* = ceil(`f32`(2 / *ρ*)), *ρ* = the current value of `r__detail_density` (section 7)
  [DO-5]. For the values 0.2, 0.3, 0.4, 0.5, 0.6: *n* = 10, 7, 5, 4, 4 [DO-11].
- **Jitter** *J* = *ρ* · 0.58823526 (`f32` 0x3F169696) [DO-5].
- **Seed** *s* = (*cx* · *cz*) XOR 0x12071980 (32-bit product of the signed cell coordinates). Four
  generators all start with state *s*: the position generator *G*ₚ, the choice generator *G*_c, the scale
  generator *G*ₛ and the rotation generator *G*ᵣ [DO-5].
- For each part *k* = 0 … 3 with id ≠ 63, its four density values d0 … d3 (LD 4.3), each multiplied by 17.

### 4.2 Positions

For *j* = 0 … *n* (along z, outer) and, within it, *i* = 0 … *n* (along x, inner), (*n* + 1)² positions in
all [DO-5]:

1. Draw *r*₁ = *G*ₚ mod 16, then *r*₂ = *G*ₚ mod 16.
2. Let *i*′ = min(*i*, *n* − 1), *j*′ = min(*j*, *n* − 1), *a* = *i*′/*n*, *b* = *j*′/*n*. For each part *k* with
   id ≠ 63: its density *D* = bilinear blend of 17·d0 … 17·d3 at (*a*, *b*) as in LD 4.3 (0 … 255); *v* =
   clamp(floor(*D* + 0.5), 0, 255). The part **qualifies** if *v* > *T*[(*i*′ + *r*₁) mod 16][(*j*′ + *r*₂)
   mod 16], *T* the threshold matrix of 4.3 [DO-5].
3. If no part qualifies, the position is skipped (nothing further is drawn from any generator for it).
4. If one part qualifies it is chosen; if several, the chosen one is the (*G*_c mod *count*)-th of the
   qualifying parts in part order [DO-5].
5. Two jitters, first for z then for x: *t* = −*J* + *r* · 2*J* / 32,767 with *r* drawn from *G*ₚ [DO-5].
6. The point is *x* = *x*_min + 2*i*/*n* + *t*ₓ, *z* = *z*_min + 2*j*/*n* + *t*_z, where *x*_min, *z*_min are the
   box's minima (2*cx* − 0.001, 2*cz* − 0.001). It may lie outside the cell by up to *J*.
7. **Ground**: a ray goes straight down from (*x*, box top, *z*). Only triangles whose (v[i1] − v[i0]) ×
   (v[i2] − v[i0]) has a y component of at least 0.00001 count (the front side faces up, CF 2.3); hits at
   or below the start are taken (distance ≥ 0, the triangle's edges included). The ground height *y* is
   the highest hit, starting from box bottom − 5 when there is none [DO-5].
8. If *y* is below the box bottom (*y*_low − 0.001), nothing is placed (no further draws).
9. Otherwise an **instance** of the chosen part's model *M* is placed at (*x*, *y*, *z*):
   - scale *σ* = lo + *r* · (hi − lo) / 32,767 with *r* from *G*ₛ, lo = 0.5 · min scale, hi = 0.9 · max scale
     of *M* (LD 3.1) [DO-5];
   - angle *θ* = *r* · 0.00019175345 (2π / 32,767) with *r* from *G*ᵣ; the instance's rotation turns model
     space about y by *θ*: a model point (*p*ₓ, *p*_y, *p*_z) goes to (*p*ₓ cos *θ* + *p*_z sin *θ*, *p*_y,
     −*p*ₓ sin *θ* + *p*_z cos *θ*) before scaling by *σ* and translating by (*x*, *y*, *z*) [DO-5];
   - **variant**: 0 (still) when the device has no vertex programs (section 6.5) or the model's flags bit 0
     is set; otherwise one draw *r* of the shared generator gives variant 1 if *r* mod 3 ≠ 0 and variant 2
     if *r* mod 3 = 0 [DO-5];
   - **light factors**: sun factor / 15 and hemisphere factor / 15 of the record (LD 4.2, 4.4) [DO-5].

The instances of each part are kept in placement order [DO-5].

### 4.3 The threshold matrix

With Q = (0, 14, 3, 13, 11, 5, 8, 6, 12, 2, 15, 1, 7, 9, 4, 10), for *a*, *b*, *c*, *d* each 0 … 3:
*T*[4*c* + *a*][4*d* + *b*] = trunc(Q[4*c* + *d*] · 15.875 · 0.0625 + Q[4*a* + *b*] · 15.875 + 0.5) [DO-6].
(*T* is built once; its values range 0 … 253, row 0 begins 0, 222, 48, 206, 14, 236, 62, 220, and the 256
values sum to 32,386 [DO-6].) The first index is the x index of 4.2, the second the z index.

### 4.4 The entry after building

The entry's box becomes the union of its instances' boxes, each instance box being the model's vertex
bounds (LD 3.1 positions) transformed by the instance's rotation, scale and translation (the axis-aligned
box of the transformed bounding box); its sphere is that box's centre and half its diagonal [DO-5]. Each
model's own radius *R* is half the diagonal of its vertex bounds [DO-5]. (An entry that placed nothing
keeps an empty box, so it is never in view.)

## 5. Per-frame selection

### 5.1 When the selection runs

The detail pass (section 6) runs in a frame when the level has detail objects and the engine's device flag
0x40 is set [DO-8]. That flag is set from the start (the flags' built-in value is 0x1C641), no console
command changes it (the only mask commands on the flags are `rs_v_sync` 4, `rs_fullscreen` 1,
`rs_refresh_60hz` 0x80 and `rs_stats` 0x20) and the game library never changes it; it is cleared only
while the engine takes its developer level-map pictures [DO-8]. So in play the pass runs every frame of a
level with detail objects. (This settles the condition WA 5.2 left unestablished.)

At the start of the pass, if the selection has not yet run in this frame and the pass ran in the previous
frame, the cache is updated (sections 2.2, 3) and the selection of 5.2–5.4 runs; otherwise neither runs
[DO-8]. The drawing lists are emptied after every pass (6.3), so in a frame where the selection does not
run nothing is drawn: the first frame of the pass after a frame without it draws no detail objects
(**believed** for the very first frame after loading: whether the record of the previous pass counts as
"previous frame" then depends on a value the loader does not set).

### 5.2 Culling

Each frame [DO-7]:

1. A frustum is made of the view-projection matrix with the left, right, top, bottom and **far** planes
   (no near plane).
2. Each non-empty group (2.4) whose sphere and box are not outside the frustum is examined; if the group
   is only partly inside, each of its entries is tested again by its own sphere and box.
3. Occlusion: an entry is then tested against the level's occlusion geometry (`level.hom`: the box test of
   `docs/behavior/occlusion-tests.md` 2.2 with the entry's box, cached as its section 5; its section 6) when such
   a test is due and occlusion testing is enabled: after a passed test the next
   is due after 10 + (shared generator mod 15) frames, after a failed test in the next frame; between tests
   the entry counts as passed; a failed entry is skipped this frame. During the detail pass, "enabled" is
   the state the previous frame left (the occlusion data are drawn later in the frame, 6.1) [DO-7]. A
   renderer without occlusion culling draws a superset.

Empty entries are skipped.

### 5.3 Refresh of an entry's lists

An entry hands out, for each of its parts, three lists of its instances: still, sway 1 and sway 2. They are
recomputed (a **refresh**) only when the frame number is greater than the entry's due frame; otherwise the
lists of its last refresh are handed out again, whatever the distance [DO-7]. At a refresh:

1. *d*² = |*E* − *C*|², *C* the entry's sphere centre (4.4). If *d*² > 2,256.25 (47.5²) the entry is skipped
   this frame, its lists and due frame unchanged (so it is retried every frame) [DO-7].
2. The **fade** *f* = 1 − (*d*² − 1) · 0.00044340981 (`f32` 0x39E87975, about 1/2,255.25) if *d*² ≥ 1, else
   1; it falls linearly in *d*² from 1 at 1 m to about 0 at 47.5 m [DO-7].
3. The next refresh is due at the current frame + 15 + (shared generator mod 15) [DO-7].
4. For each instance (scale *σ*, model radius *R*): its drawing scale *σ*′ = *σ* · *f* is stored, and its
   size measure *q* = (*σ*′)² · *R*² / *d*². With *T*_s = 12.25 · *k*, the value *ssa*_min of SS 10.2 for this
   frame (it depends on `r__geometry_lod`, the field of view and the target size): the instance is dropped
   if *q* < *T*_s; it goes to the still list if *q* ≤ 16 · *T*_s, otherwise to the list of its variant
   (0 still, 1 sway 1, 2 sway 2) [DO-7].

So `r__geometry_lod` changes which detail instances are drawn, at once (through *T*_s) [DO-7]; distant and
small instances stop swaying (between *T*_s and 16 *T*_s) before they vanish.

Consequences for an implementation: an instance's drawn size and list are updated only every 15 to 29
frames per entry; computing them every frame changes the picture only by that lag (**believed**: the
difference is a slightly later shrink and sway change). Because an entry re-initialised by a camera move
(2.2) keeps handing out the lists of its last refresh until its due frame, the original can for up to 29
frames draw stale instances of the cell that previously used that cache entry; such entries are at the
far edge of the cache, where *f* ≈ 0 (**Disputed in review:** the instances discarded at initialisation
(2.3) return to one pool shared by all entries, and a later build that takes one from the pool resets it,
drawing scale included, to zero; so a stale list refers to the previous cell's instances only until they
are reused, and from then to zero-size instances of whichever entry reused them) (**believed**: no visible effect; an implementation should hand out
nothing for an entry built after its last refresh).

### 5.4 Collecting

The lists of every selected entry are appended, per model, to three drawing lists (still, sway 1,
sway 2), in group order (groups row by row, 12 per row), entry order inside the group (row by row) and
part order [DO-7]. The order has no visible effect (the pass is alpha-tested and writes depth, 6.2;
**believed**).

## 6. Drawing

### 6.1 Place in the frame and fixed states

- The detail pass comes after step 1 of SS 10.3 (non-sorted passes of priorities 0–1) and before step 2
  (LOD billboards), so before the sky and before the occlusion data of 5.2 are drawn [DO-9, DO-7].
- During the pass culling is off (both faces drawn); afterwards back-face culling of counter-clockwise
  triangles (SS 10.1) is restored. The world transform is the identity: instance positions are world
  positions [DO-9].
- Order: the sway-1 list of every model (model id order), then the sway-2 list, then the still list
  [DO-9].
- Dynamic lights (DL) are not applied to detail objects (**believed**: the pass draws only the two
  elements of 6.2).

### 6.2 Shader and states

Every shipped model uses the shader `details\blend` with texture `build_details` (LD 3.1). `details\blend`
is a `shaders.xr` definition of the detail-object kind (tag `D_STILL `) with the property `Alpha-blend`
= 1 (`details\set`, the other definition of the kind, has 0) [DO-9]. Its two elements [DO-9]:

| Element | Used for | Vertex program | Pixel program |
|---|---|---|---|
| 0 | sway 1, sway 2 | `detail_wave` | `detail` |
| 1 | still | `detail_still` | `detail` |

States of both: depth test less-or-equal, depth write on, no blending (source one, destination zero),
fixed-function fog **off**, lighting off, alpha test on when `Alpha-blend` is 1 with reference 200 (passes
when alpha · 255 > 200, SS 10.1), off otherwise [DO-9]. Sampler `s_base` (texture `build_details`):
wrap addressing on all axes, anisotropic minification and magnification, linear mip filter (the
anisotropy level as in SS 4.4) [DO-9]. So detail objects are not fogged and are cut out, not blended.

### 6.3 Per-instance inputs

For each drawn instance (rotation rows *m*, translation (*x*, *y*, *z*), drawing scale *σ*′, light factors
*s*_f = sun factor/15 and *h*_f = hemisphere factor/15) the programs receive [DO-9]:

- three rows A₀, A₁, A₂: A_r = (*σ*′ · *m*_r, translation component *r*), where *m*_r is row *r* of the
  rotation of 4.2 (x′ = A₀ · (*p*, 1), etc.);
- a colour *c* = (`ambient` + `hemi_color`.rgb · *h*_f + `sun_color` · 0.5 · *s*_f, 1), with the current
  blended weather values L_ambient, L_hemi_color, L_sun_color of SS 9.2 (LD 4.4) [DO-9].

Per vertex of the model (LD 3.1): its position (*p*, 1), its texture coordinate stored as
floor(u · 16384) / 16384, floor(v · 16384) / 16384, and its **height fraction** *φ* = floor(*p*_y / *h* ·
16384) / 16384 with *h* = the model's vertex-bounds height (each stored value clamped to −32,768 … 32,767
before the division) [DO-10]. The drawing lists are emptied after the pass [DO-9].

### 6.4 Program equations

From the shipped programs `shaders\r1\detail_wave.vs`, `detail_still.vs`, `detail.ps` and
`shaders\shared\common.h`, read as data [DO-10]:

- World position *P* = (A₀ · (*p*, 1), A₁ · (*p*, 1), A₂ · (*p*, 1)).
- **Still** (`detail_still`): clip position = VP · *P* (SS 6); vertex colour C = *c*.
- **Sway** (`detail_wave`) with the constants *w* (wave) and **δ** (dir2D) of 6.5 and (κ_x, κ_y, κ_z, κ_w) =
  (1/16384, 1/16384, 0.25, 0.9):
  - phase *g* = *P* · *w*.xyz + *w*.w; cyclic value *γ* = (2.8284271 · frac(*g*) − 1.4142136)² − 1 (−1 … 1),
    frac(*g*) = *g* − floor(*g*);
  - *H* = *P*.y − *y*_inst, where *y*_inst = A₁.w is the instance's ground height;
  - *P*.x ← *P*.x + **δ**.x · *H* · *γ* · *φ*, *P*.z ← *P*.z + **δ**.z · *H* · *γ* · *φ* (y unchanged);
  - clip position = VP · *P*; vertex colour C = *c* · (0.9 + 0.25 · max(0, *γ*) · *φ*).
- Vertex colours are clamped to [0, 1] before interpolation (SS 6).
- **Pixel** (`detail`): with the texel *t* of `s_base` at the texture coordinate: rgb = *t*.rgb · C.rgb · 2,
  alpha = *t*.a (then the alpha test of 6.2); the result is clamped to [0, 1] (SS 6).

### 6.5 The wind's constants

Each pass, the five current sway values (amp1, amp2, rot1, rot2, speed) are the blend of WA 5.2 by the wind
strength *W* [DO-9 with WA]. With *t* the engine's global time in seconds (**believed** to be the clock of
WA 4.3) [DO-9]:

| | sway 1 | sway 2 |
|---|---|---|
| *w*.xyz | (1/(10π), 1/(14π), 1/(6π)) = (0.031830989, 0.022736421, 0.053051647) | (1/(6π), 1/(14π), 1/(10π)) |
| *w*.w | *t* · speed / (2π) | the same |
| **δ** | amp1 · (sin(2π*t*/rot1), 0, cos(2π*t*/rot1)) | amp2 · (sin(2π*t*/rot2), 0, cos(2π*t*/rot2)) |

So the sway's direction turns around the vertical once every rot1 (rot2) seconds, its amplitude is amp1
(amp2) per metre of height at the top of the model (*φ* = 1), and it pulses as a wave travelling across the
ground with speed set by `speed`. The weather's wind direction and velocity do not enter [DO-9]. With the
shipped values and *W* = 0.5 (WA 5.2): amp1 0.225, amp2 0.125, rot1 17.5 s, rot2 0.75 s, speed 1.25.

### 6.6 Devices without vertex programs

When the device reports no vertex programs (vertex-shader major version 0), every instance is still
(4.2), only the still lists are drawn, positions are transformed on the processor, the vertex colour is
white, and the shader's fixed-function form is used (texture × diffuse, lighting and fog switched on)
[DO-5, DO-12]. Its exact colour result is **unestablished**. The project's targets always have vertex
programs; this path is described only for completeness.

## 7. `r__detail_density`

- Real, 0.2 … 0.6, built-in value 0.3; presets: `minimum` 0.4, `low` 0.4, `default` 0.3, `high` 0.2,
  `extreme` 0.2, `maximum` 0.2; the shipped `user.ltx` 0.2 [DO-11].
- It is read only when an entry is built (4.1) [DO-11]: a new value applies to entries built afterwards,
  i.e. to cells entering the cache through camera movement (2.2) and to every cell at the next level load.
  Entries already built keep their instances; nothing rebuilds them on a set or on a video restart
  [DO-1, DO-11]. (This settles the rebuild rule RS 2.4 left open.)
- Its effect: (*n* + 1)² candidate positions per cell (121 at 0.2, 64 at 0.3, 36 at 0.4, 25 at 0.5 and
  0.6) and the jitter *J*; the densities of the cell, through the dither matrix, decide which positions
  are used. A smaller value means more instances.

## 8. Verification

### 8.1 Facts of the binaries and data

The constants of sections 2–7 [DO-1 … DO-12]; the threshold matrix of 4.3 [DO-6].

### 8.2 Placement vectors (`levels\l01_escape\`)

Computed by `vec.py` from the rules of section 4 alone, applied to the 16 × 16 cells *cx*, *cz* = −8 … 7
(world x, z from −16 to 16) with the Russian edition's `level.details` and `level.cform`. They are not
measurements of the original (nothing was executed); they fix the reading of the rules. The model uses
double precision; values on a rounding boundary may differ from the original's single-precision
arithmetic in rare positions [DO-13]:

| *ρ* | *n* | instances | sum of ground heights *y* | sum of scales *σ* |
|---|---|---|---|---|
| 0.2 | 10 | 3,526 | −1,279.7 | 3,393.87 |
| 0.3 | 7 | 1,981 | −684.6 | 1,893.56 |
| 0.4 | 5 | 1,059 | −354.4 | 993.49 |
| 0.6 | 4 | 633 | −232.8 | 599.86 |

Instances per model id at *ρ* = 0.3: 1: 28, 4: 14, 5: 9, 6: 3, 8: 326, 9: 457, 10: 3, 12: 251, 13: 218,
14: 26, 15: 172, 16: 45, 17: 32, 18: 10, 19: 244, 20: 143 [DO-13].

## 9. Not established

- Which shared-generator values the original draws (variants, occlusion and refresh delays): not
  reproducible, and any uniform choice is equivalent.
- Whether the first frame after loading draws detail objects (5.1).
- The exact colour result of the fixed-function path (6.6).
- Nothing was executed: all timing statements come from the order of operations in the binaries.

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archives `gamedata.db0` … `gamedata.dbd`, extracted with the project's archive tool; the shipped
  `user.ltx`.
- Data analysis: `levels\l01_escape\level.details` and `level.cform`; `shaders.xr` (`details\blend`,
  `details\set`); the program files `shaders\r1\detail_wave.vs`, `detail_still.vs`, `detail.ps`,
  `common.h`, `shaders\shared\common.h`; `config\system.ltx` `[details]`; `config\rspec_*.ltx`.
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only) of `xrRender_R1.dll` (loading,
  the cache and its updates, the slot build including a recovered jump table, the threshold matrix,
  selection and refresh, both drawing paths, the detail shader's passes, the frame order), `XR_3DA.exe`
  (pass and sampler setup, frustum planes, device flags and their commands, the weather section's key
  layout, device capabilities), `xrCDB.dll` (the box query) and `xrGame.dll` (uses of the device flags);
  constants read directly from the files. No execution or debugging.
- A placement model written from this text reproduces section 8.2.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The order within one update of the group recomputation (2.4) and the builds (3): the groups are recomputed
  after that update's builds, so their boxes include the boxes of the entries built in it.
