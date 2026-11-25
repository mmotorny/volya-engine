# Particle drawing

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of how the particles of an effect of `particles.xr` become sprites on the screen:
texture frames and their animation (effect chunk 6 and flag bits 0x400, 0x800, 0x1000, 0x2000), the engine shader
and texture (chunk 7), alignment (chunk 37, bits 0x8000, 0x100000, 0x200000), stretching with speed (chunk 34, bit
0x40000), culling (bits 0x400000, 0x800000), the 6 engine shaders the effects name, including the distortion
effects, and the order and visibility of effects within a frame. It is specification P7 of
`docs/behavior/particles-and-lights-scope.md`. What could not be established is marked **unestablished**; what is
inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/formats/particles-xr.md` (P1): the effect record, its flag word *F* and chunks 6, 7, 34, 37.
- P2, *Particle effect lifecycle and emission* (`particle-lifecycle.md`): an effect instance, its particles
  (position **x**, velocity **w**ₚ, size **s**, rotation **ρ**, colour, frame number, frame flags), the particle
  list and its order, the shared random generator (its section 2.1), world and local placement (its section 5.1,
  with the kept matrix *M*), the update and its steps (its section 8) and the bounding sphere (its section 8.3).
- `docs/behavior/static-shading.md`: shader resolution (its sections 1, 3.4, 4), samplers (4.4), the script
  vocabulary (3.4), fog (9.3), fixed states (10.1), the frame order (10.3), the settings (11).
- `docs/formats/level.md` section 9: sectors, portals and the screen threshold *T*\_min (its section 9.4).
- `docs/behavior/level-objects.md`: the row-vector matrix convention.

Conventions: vectors are rows; a point **p** is mapped by a matrix *M* as [**p** 1] · *M*, a direction **d** as
[**d** 0] · *M* (rows 0 – 2 of *M* are the mapped axes, row 3 the translation). · is the dot product, × the cross
product, |**a**| the length. Arithmetic is IEEE single precision (`f32`); sines and cosines are taken of `f32`
angles in radians and rounded to `f32`. floor(*x*) is the largest integer ≤ *x*; *a* mod *b* and *a* div *b* are
the remainder and quotient of integer division (both operands are ≥ 0 here). FLT_MIN is 1.1754944 × 10⁻³⁸.
Direct3D 9 render-state values are given by meaning, with the public enumeration value in brackets.
"The camera" has position **E**, forward **F**, up **U** and right **R** (section 4.1). Every fact carries the
name of the check that reproduces it, a script in `checks/particle-drawing/` of the dirty workspace; `run_all.py`
runs them all (`archive.py` extracts the data, `bin_setup.py` prepares the static analysis the `bin_*.py` checks
read, `pdata.py` decodes the data from the approved specifications, `sim.py` is a throwaway implementation of
sections 3 and 4 written from this text).

## 1. Overview

An effect is drawn as one batch of sprites, one quad per live particle, built anew from the particles' current
values and the current camera every time the effect is drawn. The effect's engine shader and texture (chunk 7)
give the passes; sections 3 and 4 give the quads; section 5 the shaders; section 6 the distortion effects;
section 7 when and in which order effects are drawn.

## 2. When an effect draws nothing

An effect draws nothing in a frame when its flag word lacks bit 0x1 (it has no sprite) or it has no live particle
(`bin_draw.py` D2). The five effects without bit 0x1 (P1 section 3) are never drawn.

## 3. Texture frames

### 3.1 The frame rectangle

Each particle shows the texture rectangle from (*u*₀, *v*₀) to (*u*₁, *v*₁) (`bin_draw.py` D3):

- Without bit 0x400: (0, 0) to (1, 1).
- With bit 0x400: from chunk 6 (P1 section 2.1), with *t*ᵤ the frame width, *t*ᵥ the frame height and *n*ᵣ the
  frames per row: the frame index *i* = floor(*f* · 0.003921569) of the particle's frame number *f* (the
  constant is the `f32` 0x3B808081, just above 1/255), and
  *u*₀ = (*i* mod *n*ᵣ) · *t*ᵤ, *v*₀ = (*i* div *n*ᵣ) · *t*ᵥ, *u*₁ = *u*₀ + *t*ᵤ, *v*₁ = *v*₀ + *t*ᵥ.

The frame count does not limit the rectangle; a rectangle reaching beyond 1 samples the texture with the
sampler's addressing (section 5): 72 effects have such frames (`data_effects.py` E9).

Every conversion of a value to an integer in this section is floor(), also for negative values
(`bin_frames.py` F0).

### 3.2 The frame number at birth

A new particle starts with frame number 0 and frame flags 0 (P2). Then, when it is born (`bin_frames.py` F1):

- with bit 0x1000: *k* is drawn from the shared generator (P2 section 2.1) and the frame number becomes
  floor((*k* mod *n*) · 255), *n* the frame count (chunk 6; 16 when the effect has no chunk 6, which only happens
  without bit 0x400, where the frame number is not used);
- then, with bits 0x800 and 0x2000 both set: a further *k* is drawn; if it is odd, frame flag 0x1 is set: the
  particle plays its frames backwards.

Nothing else of the particle is changed at birth by the drawing.

### 3.3 The frame animation

When the flag word has both 0x400 and 0x800, step 3 of every update step (P2 section 8.3) changes every live
particle (`bin_frames.py` F2): with *r* the frame rate (chunk 6), *n* the frame count, and *σ* = −1 if the
particle's frame flag 0x1 is set, else 1:

1. *g* = *f* · 0.003921569 + *σ* · (*r* · 0.033), *f* the frame number;
2. if *g* > *n*, *g* ← *g* − *n*; then, if *g* < 0, *g* ← *g* + *n* (each at most once);
3. the frame number becomes floor(*g* · 255) (kept as 16 bits).

The animation does not run with 0x800 alone (P1 section 3).

## 4. The sprite of a particle

### 4.1 The camera

The camera's forward **F**, up **U** and right **R** are those of the view the frame is drawn with: the camera
sets **R** = **U** × **F** from its unit forward and up vectors and then **U** = **F** × **R**
(`bin_draw.py` D14). **E** is the camera position.

### 4.2 Half sizes and stretching

With **s** the particle's size (`bin_draw.py` D4):

- *w* = **s**ₓ · 0.5, *h* = **s**ᵧ · 0.5 (the third size component is not used by the drawing);
- with bit 0x40000, with *v* = |**w**ₚ| the particle's speed and (*k*ₓ, *k*ᵧ) the x and y of the velocity
  scale (chunk 34): *w* ← *w* + *k*ₓ · *v*, *h* ← *h* + *k*ᵧ · *v*.

### 4.3 Orientation

The quad is built from a centre **X**, the half sizes *w*, *h*, the angle *α* = **ρ**ₓ (the first rotation
component, radians) and two vectors **T** (the sprite's up) and **S** (its right) (`bin_draw.py` D5, D6):

- **A** = (**S** cos *α* + **T** sin *α*) · *w*, **B** = (**T** cos *α* − **S** sin *α*) · *h*.

**X**, **T** and **S** are chosen as follows, with *v* = |**w**ₚ| and the alignment angles **a** = (*a*ₓ, *a*ᵧ,
*a*_z) (chunk 37; default (−1.5707964, 0, 0), P1) (`bin_draw.py` D7 – D10):

| Case | Flag word | **T** | **S** |
|---|---|---|---|
| facing | without 0x8000 | **U** | **R** |
| fixed at rest | 0x8000, *v* < 10⁻⁷, 0x100000 | row 2 of *Q*(**a**) | row 0 of *Q*(**a**) |
| axis at rest | 0x8000, *v* < 10⁻⁷, without 0x100000 | **D** = **d**(**a**) | **D** × **F**, see below |
| across motion | 0x8000, *v* ≥ 10⁻⁷, 0x200000 | unit(**k** × **S**) | unit(**e** × **k**) |
| along motion | 0x8000, *v* ≥ 10⁻⁷, without 0x200000 | **D** = **w**ₚ · (1/*v*) | **D** × **F**, see below |

(10⁻⁷ is the `f32` 1.0000000e−7; a case with *v* ≥ 10⁻⁷ ignores 0x100000, a case with *v* < 10⁻⁷ ignores
0x200000.)

- *Q*(**a**), with s₀ = sin *a*ₓ, c₀ = cos *a*ₓ, s₁ = sin *a*ᵧ, c₁ = cos *a*ᵧ, s₂ = sin *a*_z, c₂ = cos *a*_z:
  row 0 = (c₂c₁ − s₂s₁s₀, −s₂c₀, s₂c₁s₀ + c₂s₁); row 2 = (−c₀s₁, s₀, c₀c₁) (`bin_draw.py` D8). All three
  angles are read here, so P1's statement that the drawing does not read the third alignment angle holds only
  for the other cases; all 239 chunks 37 have *a*_z = 0 (P1).
- **d**(**a**) = (sin *a*ᵧ · cos *a*ₓ, −sin *a*ₓ, cos *a*ᵧ · cos *a*ₓ), computed as (−sin(−*a*ᵧ) · cos(−*a*ₓ),
  sin(−*a*ₓ), cos(−*a*ᵧ) · cos(−*a*ₓ)) (`bin_draw.py` D9).
- For the two axis cases, **S** = **D** × **F**, divided by its length only when its squared length is greater
  than FLT_MIN; otherwise it is used as it is (a sprite seen along its axis degenerates) (`bin_draw.py` D6). The
  sprite stretches along **D** and turns about it toward the camera.
- Across motion: **k** = **w**ₚ · (1/*v*); **e** = (0, 1, 0) if |**k**ᵧ| ≤ 0.99, else (0, 0, 1); unit(**a**) here
  is **a** · (1/√(**a** · **a**)) with no guard. The sprite lies in the plane across the motion
  (`bin_draw.py` D10).

**X** is the particle position **x**. In local mode (P2 section 5.1) the particles are in the effect's own space
and are drawn mapped by the kept matrix *M*: **X** = [**x** 1] · *M*, and **T**, **S** of the fixed, axis and
across cases are mapped as directions, [**T** 0] · *M*, [**S** 0] · *M* (the axis **D** is mapped before **S** is
formed from it; no vector is normalised again); the facing case's **U** and **R** are not mapped. *v*, **k** and
the case are taken from the particle's own (unmapped) velocity (`bin_draw.py` D11). In world mode nothing is
mapped.

### 4.4 The quad

The quad's four vertices, in this order, with their texture coordinates (`bin_draw.py` D5, D6):

| Vertex | Position | (u, v) |
|---|---|---|
| 0 | **X** − **A** − **B** | (*u*₀, *v*₁) |
| 1 | **X** − **A** + **B** | (*u*₀, *v*₀) |
| 2 | **X** + **A** − **B** | (*u*₁, *v*₁) |
| 3 | **X** + **A** + **B** | (*u*₁, *v*₀) |

Each vertex carries the particle's colour as a Direct3D colour (a 32-bit word holding A in its top byte, then R,
G and B, read by the programs as (R, G, B, A) / 255). The quad is the two triangles (0, 1, 2) and (3, 2, 1) (`bin_draw.py` D13). The
vertices are world positions: the effect is drawn with the identity world matrix (`bin_draw.py` D12). A vertex
is 24 bytes: position (3 `f32`), colour, u, v (`bin_draw.py` D2).

All quads of an effect are drawn as one indexed triangle list, the particles in the order of the particle list
(P2 section 3.2): a later particle is drawn over an earlier one; there is no sorting within an effect
(`bin_draw.py` D12).

### 4.5 Culling

During an effect's draw, face culling is set from the flag word (`bin_draw.py` D12): without 0x400000 no culling
[1]; with 0x400000 alone triangles appearing clockwise on screen are culled [2]; with 0x400000 and 0x800000
counter-clockwise ones [3]. 0x800000 without 0x400000 has no effect (its one effect lacks 0x400000,
`data_effects.py` E6). After the effect the culling returns to counter-clockwise [3], the fixed state of
static-shading.md section 10.1.

## 5. Shaders

### 5.1 Resolution

An effect's shader is made at library load from chunk 7's engine shader name and texture string exactly as a
level shader entry is (static-shading.md sections 1, 3.4, 4: texture list T[0], T[1], …; scripted definitions
take precedence) (`bin_shader.py` H8). The six engine shaders the effects name (`data_effects.py` E1,
`data_shaders.py` Q1, Q2):

| Engine shader | Effects | Resolves to |
|---|---|---|
| `particles\add` | 383 | library definition, class `PARTICLE` (section 5.2) |
| `particles\blend` | 322 | library definition, class `PARTICLE` |
| `particles\alpha_add` | 213 | library definition, class `PARTICLE` |
| `particles\set` | 1 (`vehiclefx\skid_dirt_start`) | library definition, class `PARTICLE` |
| `particles\xadd` | 49 | scripted definition `shaders\r1\particles_xadd.s` (section 5.3) |
| `particles\xdistort` | 102 | scripted definition `shaders\r1\particles_xdistort.s` |

`shaders.xr` also holds `S_SET   ` definitions named `particles\xadd` and `particles\xdistort`, which the scripted
ones replace, and `particles\dark` (class `PARTICLE`, Blending 3) and `particles\xblend`, which no effect names
(`data_shaders.py` Q1).

### 5.2 Class `PARTICLE`

Stored as the tag `PARTICLE` (byte-reversed in the file, static-shading.md 3.2), version 0; after the common
block the properties token `Blending` (tokens 0 `SET`, 1 `BLEND`, 2 `ADD`, 3 `MUL`, 4 `MUL_2X`, 5 `ALPHA-ADD`),
boolean `Texture clamp`, integer `Alpha ref` (`bin_shader.py` H1, `data_shaders.py` Q1). Values:

| Definition | Priority | Strict sorting | Blending | Texture clamp | Alpha ref |
|---|---|---|---|---|---|
| `particles\add` | 3 | no | 2 `ADD` | yes | 32 |
| `particles\blend` | 3 | yes | 1 `BLEND` | yes | 32 |
| `particles\alpha_add` | 3 | no | 5 `ALPHA-ADD` | yes | 32 |
| `particles\set` | 1 | no | 0 `SET` | yes | 32 |

The class makes the same single pass for every element of the shader, elements 0 – 5 (static-shading.md
section 5; it does not depend on the element), so a library particle shader has identical elements 0 – 4
(`bin_shader.py` H7). The pass (`bin_shader.py` H2, H4):

- programs `particle` / `particle` (5.4); lighting off; depth test on (less-or-equal);
- per `Blending`:

| Blending | Blend (source / destination factor) | Depth write | Fog | Alpha test |
|---|---|---|---|---|
| 0 `SET` | off | on | on | passes when A · 255 > 200 |
| 1 `BLEND` | source alpha / inverse source alpha [5 / 6] | off | off | A · 255 > 0 |
| 2 `ADD` | one / one [2 / 2] | off | off | A · 255 > 0 |
| 3 `MUL` | destination colour / zero [9 / 1] | off | off | A · 255 > 0 |
| 4 `MUL_2X` | destination colour / source colour [9 / 3] | off | off | A · 255 > 0 |
| 5 `ALPHA-ADD` | source alpha / one [5 / 2] | off | off | A · 255 > 0 |

  (other values make no pass); the property `Alpha ref` is not used;
- one sampler, `s_base` = T[0], addressing clamp when `Texture clamp` is set (all four), else wrap; mip filter
  linear; minification and magnification anisotropic (a sampler named `s_base` or `s_detail` given linear
  filtering gets anisotropic minification and magnification, as for the script vocabulary, static-shading.md
  3.4); maximum anisotropy and mip bias as static-shading.md 4.4 (`bin_shader.py` H3, H5). The class reads no
  other texture: a second name in the texture string (7 effects of these classes have one,
  `data_effects.py` E2) is not used.

The priority is the low two bits of each element's flags and strict sorting the next bit; they decide where the
effect is queued (section 7) (`bin_shader.py` H6).

### 5.3 The scripted particle shaders

The script word `distort(b)` sets an element flag (0x10) that marks the element for the distortion target; it
is not in static-shading.md's vocabulary table (`bin_shader.py` H6). `particles_xadd.s` defines `normal` and
`l_special`, `particles_xdistort.s` only `l_special` (`data_shaders.py` Q2); with the element rules of
static-shading.md 3.4 (no `normal_hq`, no `l_point`, `l_spot`):

| Shader | Elements 0 and 1 | Element 4 |
|---|---|---|
| `particles\xadd` | programs `particle` / `particle`; priority 3, not strict; blend one / one; alpha test off; depth test on, write off; fog off; distortion flag set (no effect on elements 0 and 1); sampler `s_base` = T[0] | programs `particle` / `particle_distort`; priority 3, not strict; blend source alpha / inverse source alpha; alpha test off (the default); depth test on, write off; fog off; distortion flag set; samplers `s_base` = T[0] (not read by the program), `s_distort` = T[1] |
| `particles\xdistort` | none (not drawn in the normal passes) | as `particles\xadd`, with `s_distort` = T[0] |

The samplers take the script defaults: wrap addressing; `s_base` anisotropic minification and magnification
with linear mip, `s_distort` linear, linear, linear (static-shading.md 3.4). Elements 2 and 3 are empty. 45 of
the 49 `particles\xadd` effects name two textures, the second always one whose name starts with `pfx\pfx_dist`
(17 of them `pfx\pfx_distortion`); `anomaly2\group_items\pe02`, `damage_fx\body_burn_01`, `damage_fx\body_burn_02` and
`explosions\group_items\expl_mushroom_flash` name one, so their T[1] is `null` and `s_distort` has no texture
(static-shading.md 3.4); what Direct3D 9 returns for such a sampler is **unestablished** (believed (0, 0, 0, 1)).
All 102 `particles\xdistort` effects name one texture (`data_effects.py` E2).

### 5.4 Programs and colour

`particle.vs` maps the world position with the view-projection matrix, passes the texture coordinate and the
vertex colour **c** = (R, G, B, A)/255 unchanged, and gives the fog factor of static-shading.md 9.3 for the
position (used only by passes with fog on, i.e. `SET`) (`data_shaders.py` Q3). With *B* the texel of `s_base`
and *D* the texel of `s_distort` at the vertex texture coordinate (`data_shaders.py` Q3):

- `particle.ps`: colour = **c**.rgb · *B*.rgb, alpha = **c**.a · *B*.a;
- `particle_distort.ps`: colour = *D*.rgb, alpha = *D*.a · ((**c**.r + **c**.g + **c**.b) · 0.33).

Then the alpha test, fog (`SET` only) and blending of the pass. Clamping and precision as static-shading.md
section 6.

## 6. Distortion

### 6.1 When it is used

Distortion is available when the device's pixel-shader version, as major · 10 + minor, is at least 14 (1.4) and
the command line does not hold `-nodistort` (`bin_order.py` O8). Without it nothing goes to the distortion
target: `particles\xdistort` effects draw nothing at all and `particles\xadd` effects only their elements 0 and 1.

### 6.2 The distortion target

Element 4 of an effect's shader, when it exists and has the distortion flag (0x10) and distortion is available,
is queued in the distortion list of the frame, with the key |**E** − **C**′|² + 10⁻⁵ (section 7.3)
(`bin_order.py` O3). Of the particle shaders only those of `particles\xadd` and `particles\xdistort` qualify:
the library particle shaders have an element 4 but without the flag.

At the end of the frame, after everything else of the world (section 7.4), when distortion is available and the
list is not empty, a distortion target the size of the scene's render target (static-shading.md 10.2), in the
same format, is made current with the scene's depth buffer (`bin_order.py` O9):

1. it is cleared, colour only, to (127, 127, 127, 127) / 255 (depth is kept: distortion sprites are depth-tested
   against the scene);
2. the queued elements are drawn in decreasing key order (back to front; equal keys in queue order), each with
   its own pass (5.3; source alpha / inverse source alpha over the target), the sprites built as in section 4;
3. the list is emptied.

Each queued element is drawn by setting its pass and asking the effect to build and draw its quads
(`bin_order.py` O10).

When the user interface draws its own distortion over the game (outside this specification), the world's
distortion list is not drawn that frame (`bin_order.py` O9).

### 6.3 Composition

The frame is then presented with the post-process program `postprocess_d` instead of `postprocess` (when the
distortion pass ran) (`bin_order.py` O9): with *D* the distortion target's texel at the first texture
coordinate **t**₀ of the post-process quad (sampled clamped, linear), the offset
**o** = (*D*.r − 0.5, *D*.g − 0.5) · 0.05 is added to both of the program's scene look-ups (**t**₀ + **o** and
**t**₁ + **o**); everything else is identical to `postprocess` (`data_shaders.py` Q4). The post-process quad's
coordinates and parameters are those of the presentation, which static-shading.md 10.2 leaves **unestablished**.
A target texel left at the cleared value gives an offset of about −0.0001 on each axis (believed negligible).

When the scene was drawn straight to the screen (no post-process that frame, static-shading.md 10.2), the
distortion of that frame is drawn but not shown; a frame following one with a distortion pass is drawn into the
render target (`bin_order.py` O11), so the next frame shows its distortion (believed).

## 7. Order and visibility within the frame

### 7.1 Effect objects

The game draws effects through effect objects (their owners, P8 – P11 and F1, say which exist). Each frame, with
dynamic objects shown (a device flag that is on by default and that no console command changes), the objects
whose culling sphere (below) meets the view frustum are considered; an object is drawn only if it lies in a
sector visited this frame (level.md 9.2) and its sphere lies inside one of the frustums that sector was reached
with, and it is not hidden by the occlusion mesh `level.hom` (out of scope as in level.md 9.2; an object found
visible is tested again after 10 – 24 frames, a hidden one every frame) (`bin_object.py` B5). **Disputed in
review:** the sphere need not lie inside the frustum: the test rejects it only when, for some plane of that
frustum, the centre lies outside the plane by more than the radius; a sphere partly inside passes. Which sector an
object is in is **unestablished** here; a renderer that ignores sectors and occlusion draws a superset.

An object's culling sphere is its instance's bounding sphere (P2 8.3) with the centre mapped by the object's
transform *T*. *T* is the identity for a new object and becomes *M* whenever the object places its instance in
local mode with *M* (P2 5.1); a world-mode placement leaves it unchanged (`bin_object.py` B2). The culling sphere
is renewed only when the object is played, placed or given its scheduled update, and only when the mapped centre
moved by at least 0.01 on some axis or the radius changed by at least 0.15; otherwise the earlier one is kept
(`bin_object.py` B3). How often an object gets its scheduled update is outside this specification.

Drawing an object (`bin_object.py` B1):

1. its instance is advanced (P2 8.2) by *e* = the device's millisecond clock now minus at the object's last
   advance, when *e* ≠ 0 (the same clock frame-wide, so an object drawn again in the same frame is not advanced
   again);
2. each of its effects is queued (7.2): the instance itself, or for a group each effect the group holds (P6),
   every one with the centre **C**′ = [**C** 1] · *T* of its own bounding sphere (**C**, *r*) as it is after the
   advance (`bin_order.py` O1).

Effect objects cast no shadows (`bin_object.py` B4). Effects drawn as part of the first-person view (weapons'
effects) are queued in that view's own lists (`bin_order.py` O4) and are outside this specification.

### 7.2 Queuing an effect

An effect is queued at most once per traversal of the scene (`bin_order.py` O2). It is not queued when its screen
measure *m* = *r* / (|**E** − **C**′|² + 10⁻⁵) is not above *T*\_min of level.md 9.4 (12.25 / *S*)
(`bin_order.py` O2). Otherwise, in the normal (camera) pass (`bin_order.py` O3, O4):

- element 4 goes to the distortion list when it qualifies (6.2);
- element 0 or 1 (identical for every particle shader) is taken; an effect whose shader has neither
  (`particles\xdistort`) is not queued further;
- an element with strict sorting goes to the frame's strict-sorted list with the key |**E** − **C**′|² + 10⁻⁵;
- any other goes to the frame's group of non-sorted dynamic passes of its priority half (priorities 0 – 1 or
  2 – 3), grouped there by pass state.

### 7.3 Sorted lists

The strict-sorted list and the distortion list are drawn in decreasing key order: the farthest first (back to
front); an element queued with a key equal to one already queued is drawn after it (`bin_order.py` O5). Each
queued effect is drawn with its own pass, its sprites built at that moment (section 4) (`bin_order.py` O10).

### 7.4 The frame

Effects take these places in the frame order of static-shading.md 10.3 (`bin_order.py` O6, O7):

1. Non-sorted passes of priorities 0 – 1 (step 1 there): the static ones, then the dynamic ones, among them
   `particles\set` effects (priority 1: opaque, depth-writing, alpha-tested).
2. LOD billboards, sky and clouds (steps 2 – 4 there).
3. The dynamic light passes (spot lights, then point lights; P16): only elements of priority 0 – 1 are queued in
   them, and only for effects already queued in this frame. Effects with the shaders of priority 3 are never
   drawn in them; a `particles\set` effect inside a light's volume is drawn again with element 2 or 3, the same
   pass as before, which leaves the image unchanged (believed: same vertices, colour and depth).
4. Non-sorted passes of priorities 2 – 3 (step 5 there): the static ones, then the dynamic ones: effects of
   `particles\add`, `particles\alpha_add` and `particles\xadd` (elements 0 / 1). These only add to the frame, so
   their order among themselves does not change the result (believed: apart from rounding).
5. The strict-sorted list (step 6 there), together with the static strict-sorted passes: effects of
   `particles\blend`, back to front (7.3).
6. Glows, flares and the environment's final drawing (step 7 there).
7. The distortion pass and the post-process (section 6).

Between 4 and 5 the renderer draws one more set of geometry, believed to be the portal fades of level.md 9.2
rule 4 (`bin_order.py` O6).

## 8. Verification

### 8.1 Facts on the data

- 1,070 effects have a sprite; their engine shaders as in 5.1 (`data_effects.py` E1). Their texture strings name
  136 distinct textures (after the rules of static-shading.md 4.1), every one present as `textures\<name>.dds`
  in the archives; 1,018 strings name one texture and 52 two (`data_effects.py` E2, E3).
- Orientation cases over the 1,070 (by flag word; the at-rest and moving cases are chosen per particle):
  833 without 0x8000; with 0x8000: 115 with neither 0x100000 nor 0x200000, 88 with 0x200000 only, 13 with
  0x100000 only, 21 with both; every one with 0x8000 has a chunk 37 (`data_effects.py` E5).
- Culling: 1,057 without 0x400000 and 0x800000, 12 with 0x400000 only, 1 with 0x800000 only
  (`hit_fx\group_items\hit_sparks_glow_inside_1`, not culled) (`data_effects.py` E6).
- Frames: 422 with 0x400; 228 animated (0x400 and 0x800); 252 with 0x400 and 0x1000; 23 with 0x400, 0x800 and
  0x2000; 113 stretched (0x40000) (`data_effects.py` E7).
- The first level's `level.ps_static` effect `tests\muhi` (scoping spec 1.2): `particles\blend`, texture
  `pfx\pfx_leaves_01` (stored `pfx\pfx_leaves_01.tga`), flag word 0x1401 (sprite, frames, random start frame, not
  animated, facing), frames 0.5 × 0.5, 2 per row, 4 frames (`data_effects.py` E8): each leaf shows one of the four
  quarters of the texture, chosen at birth, and is drawn back to front with the other strict-sorted passes.

- The 22 effects of the first level's start named by the `level.ps_static`, zone and artefact roots of the
  scoping spec 1.2 and every member slot of their groups (the weather's ambient effects are not among them) (`data_l01.py` L1 – L4): engine shaders `particles\add` 7,
  `particles\alpha_add` 6, `particles\blend` 5, `particles\xdistort` 4 (`anomaly2\artefact\artefact_gravi_dist`,
  `anomaly2\group_items\gravity_idle_dist_00`, `anomaly2\group_items\gravity_shield_dist`,
  `static\fire_distort`: drawn only into the distortion target); 16 facing, 5 aligned with neither 0x100000 nor
  0x200000, 1 with both; 7 with frames, 4 of them animated; 2 stretched; 1 culled
  (`anomaly2\group_items\electra_disk`, flag word 0x708001: clockwise faces culled).

### 8.2 Test vectors

Computed by `sim.py`, an implementation of sections 3 and 4 written from this text, not measured in the game.
The camera: **E** = (0, 0, 0), **F** = (0, 0, 1), **U** = (0, 1, 0), so **R** = (1, 0, 0). Vertices are listed
as position; (u, v).

1. Facing, no frames: **x** = (1, 2, 10), **s** = (2, 1, 1), **ρ**ₓ = 0: (0, 1.5, 10); (0, 1) —
   (0, 2.5, 10); (0, 0) — (2, 1.5, 10); (1, 1) — (2, 2.5, 10); (1, 0).
2. As 1 with **ρ**ₓ = π/2 (`f32`): **A** ≈ (−4.4 × 10⁻⁸, 1, 0), **B** ≈ (−0.5, −2.2 × 10⁻⁸, 0): vertex 0 ≈
   (1.5, 1, 10), vertex 3 ≈ (0.5, 3, 10).
3. Frames: frame number 765, 2 per row, 0.5 × 0.5: index 3, rectangle (0.5, 0.5) – (1, 1).
4. Animation: frame number 0, 4 frames, rate 24 (0.792 frames per step): the frame numbers after steps 1 – 6
   are 201, 402, 603, 804, 1005, 186 (frames 0, 1, 2, 3, 3, 0; the sixth passes 4 and wraps). Backwards from
   0: −0.792 < 0 → 3.208 → 818 (frame 3).
5. Along motion with stretching: **x** = (0, 0, 10), **s** = (1, 1, 1), **w**ₚ = (0, 3, 0), velocity scale (0, 0.5),
   **ρ**ₓ = 0: *w* = 0.5, *h* = 0.5 + 1.5 = 2, **D** = (0, 1, 0), **S** = **D** × **F** = (1, 0, 0): vertices
   (−0.5, −2, 10), (−0.5, 2, 10), (0.5, −2, 10), (0.5, 2, 10).
6. Across motion: **w**ₚ = (0, 0, 2): **k** = (0, 0, 1), **e** = (0, 1, 0), **S** = (1, 0, 0), **T** = (0, 1, 0).
7. Fixed at rest with **a** = (−1.5707964, 0, 0): **T** = row 2 ≈ (0, −1, −4.4 × 10⁻⁸), **S** = row 0 = (1, 0, 0);
   axis at rest with the same **a**: **D** ≈ (0, 1, −4.4 × 10⁻⁸).
8. Axis along the view: **w**ₚ = (0, 0, 5): **D** = **F**, **S** = (0, 0, 0) (not normalised); the quad
   collapses onto the line **X** ± **B**.

## 9. Unestablished

- The texel Direct3D 9 gives for a sampler without a texture (5.3; four `particles\xadd` effects).
- The coordinates and parameters of the post-process quad (6.3; static-shading.md 10.2).
- Which sector an effect object is in, and the occlusion test (7.1).
- The geometry drawn between steps 4 and 5 of 7.4 (believed the portal fades).
- The distortion target's pixel format beyond being the scene target's.

## 10. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`;
  binaries `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrParticles.dll`.
- Data examined: `particles.xr`, `shaders.xr`, `shaders\r1\.s`, `common.h`, `particle.vs`, `particle.ps`,
  `particle_distort.ps`, `particles_xadd.s`, `particles_xdistort.s`, `postprocess.s`, `postprocess_d.s`,
  `postprocess.ps`, `postprocess_d.ps` (each identical in db4 and dba), the archive listings for the textures.
- Analysis: parsing of the data with throwaway decoders written from the approved specifications; static
  analysis (Ghidra 12.2, decompilation and instruction listings) of the shipped binaries; emulation of one
  instruction sequence (the float-to-integer conversion). No code was executed. The checks in
  `checks/particle-drawing/` reproduce every fact.

## Gaps found in review

- `docs/formats/particles-xr.md` 2.4 says the drawing does not read chunk 37's z; section 4.3 here shows the fixed-at-rest case reads it, and particles-xr.md is not corrected.
- The view and projection the distortion pass (6.2) draws its sprites with are not stated (believed the scene's).
- How a frame number is stored when floor(*g* · 255) of 3.3 falls outside 0 … 65535 is not stated; no shipped animated effect reaches that case (frame rate · 0.033 never exceeds its frame count).
- The drawing facts of the weather's ambient effects present at the first level's start (`nature\fog_foggy_00`, `nature\fog_stormy_01`; scoping spec 1.2) are not given in 8.1.
