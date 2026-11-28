# Thunderbolts: the lightning model, its flash and its drawing (first renderer)

Specification P21 of the approved scoping specification `docs/behavior/particles-and-lights-scope.md`:
how a lightning bolt of the weather is drawn in the game's first (static-lighting) renderer (its
lightning model, its two glow sprites, its colour animation) and the light it gives. It also specifies
the `.dm` model format, which the lightning models use, over every `.dm` file of the data (section 2).
When bolts come, where they are aimed and how far away they are, and their thunder, are the approved
`docs/behavior/sound-rain-thunder.md` (here **S8**) and are not repeated. What could not be established
is marked **unestablished**; what is inferred rather than read from the program or the data is marked
**believed**.

Builds on (not repeated here):

- S8: the bolt schedule (section 5.2), a bolt's start with its life λ, description, direction **d**,
  distance *s* and point **p** (section 5.3), the blended far plane *F* (section 2.3), the order of a
  bolt's random draws (section 6), the clocks *T* and *dt* (section 3.1), when the environment is updated
  (section 3.2), the bolt sections and `[thunderbolt_common]` of `config\weathers\thunderbolt.ltx`
  (sections 2.2, 7.2).
- `docs/formats/lanims-xr.md` (here **LA**): finding a colour animation by name (section 4), its values
  (section 5) and its colour at a time (section 6.2).
- `docs/behavior/sky.md`: the sky box colour (section 5.3), the sprite quads (section 7.4), the shader
  `effects\sun` (section 7.5), the lens flares (section 7).
- `docs/behavior/static-shading.md` (here **SS**): shader and texture resolution (sections 3, 4), the
  shared constant L_sun_color (section 6), the blended weather values and the fog (sections 9.2, 9.3),
  the order of a frame (section 10.3), the fixed states (section 10.1).
- `docs/behavior/view-projection.md` (here **VP**): the camera's eye *E*, right vector **r** and up vector
  **u′** (section 1.1), the projection and the far plane (sections 2, 2.1);
  `docs/behavior/world-coordinates.md` (here **WC**): world coordinates (section 1).
- `docs/formats/level-cform.md`: the level's static collision mesh; `docs/behavior/model-shading.md`
  section 7: objects' lighting; `docs/behavior/object-projected-light.md` section 5.3: the view object;
  `docs/formats/chunks.md`; `docs/formats/ltx-config.md`.

Conventions: as S8. U(*a*, *b*) is a new draw of S8's shared generator mapped to *a* … *b* (S8
conventions); "r mod 2" uses a new draw *r* (0 … 32,767) of the same generator. Vectors are rows; a point
**v** is transformed by a 4 × 4 matrix *M* as (**v**, 1) · *M*. All arithmetic is in 32-bit float.
Positions are world coordinates (metres, +y up, WC section 1). "Byte of *x*" is trunc(255 · *x*) as in
sky.md.

**Checks.** Every factual claim carries a tag. The checks are scripts in the dirty workspace,
`checks/thunderbolts/`: `extract.py` extracts the files from the archives; `export.py` regenerates the
evidence in `ev/` from the shipped program files (headless Ghidra); `run_all.py` runs `check_dm.py`
(tags F1 – F6: every `.dm` file and every `level.details`, decoded by a reader written from section 2),
`check_data.py` (D1 – D6: the configuration, `lanims.xr`, `shaders.xr`, textures), `check_bin.py`
(B1 – B11: the program files) and `model.py` (M1 – M6: an implementation of sections 3 – 6 written from
this text, reproducing section 8).

## 1. Overview

- Each bolt description (a bolt section of `thunderbolt.ltx`) names a **lightning model** (a `.dm` file),
  a **colour animation** and two glow **sprites**, top and centre (section 3).
- When a bolt starts (S8 section 5.3) its picture gets a random **tilt** and a **length**: the distance
  from the bolt's point **p** along its tilted axis to the first obstacle, at most 2 *F* (section 4).
- While the bolt runs, every environment update adds a **flash** colour, taken from the colour animation
  over the bolt's life, to the frame's blended sky, sun and fog colours (section 5). That flash is all
  the light a bolt gives: it creates no dynamic light [B10].
- Every rendered frame while the bolt runs, after the rain, the model is drawn additively with a
  scrolling or flickering texture, and the two sprites additively, fading in (section 6).

## 2. The `.dm` format

### 2.1 Files

The data holds 13 `.dm` files, all in `meshes\dm\`, each stored once, in `gamedata.db4` [F1]. Users
name them relative to `meshes\` [B2]:

- the bolt sections' `lightning_model` (section 3; 7 of the files) [F6, B2];
- the rain, whose model `dm\rain.dm` is named by the program itself; the program names no other `.dm`
  file [F6]. The rain's drawing is not specified here.

The same record is also the layout of every model sub-chunk of `level.details` chunk 1 (read by the same
reader) [F5, B11]; that file's other contents and the detail objects belong to the `level.details`
specification.

### 2.2 Layout

Little-endian; no header or padding; the record fills the file exactly [F2, B11]:

| Field | Size | Content |
|---|---|---|
| shader | *n*₁ + 1 | zero-terminated text (Windows-1251): an engine shader name |
| texture | *n*₂ + 1 | zero-terminated text: a texture name |
| flags | 4 | 32-bit unsigned |
| min scale | 4 | 32-bit float |
| max scale | 4 | 32-bit float |
| vertex count *N*v | 4 | 32-bit unsigned |
| index count *N*i | 4 | 32-bit unsigned |
| vertices | 20 · *N*v | per vertex five 32-bit floats: position x, y, z, texture u, v |
| indices | 2 · *N*i | 16-bit unsigned vertex numbers, a triangle list: triangle *k* is indices 3*k*, 3*k* + 1, 3*k* + 2 |

The strings are read into 256-byte buffers [B11]; every shipped string is at most 27 bytes long [F4].

### 2.3 Meaning and reading rules

- **shader**, **texture**: one shader is made from the pair, the shader name as the engine shader and the
  texture as its texture, as in SS sections 3 and 4 [B11]. Every lightning model names `effects\lightning`
  (an `S_SET` definition, section 6.2); `rain.dm` names `effects\rain`; the `level.details` records name
  `details\blend` [F4, F5, D4].
- **positions** are in the model's own coordinates; how a user places them is the user's (section 4 for
  the bolt). **u**, **v** are the texture coordinates of the vertex.
- *N*i must be a multiple of 3; otherwise the game stops with an assertion [B11]. Indices are not checked
  against *N*v (an index ≥ *N*v is **unestablished**; none occurs [F2]).
- **flags**, **min scale**, **max scale** are read and stored [B11]. The two users of `.dm` files do not
  use them [B9, B11]; their meaning for detail objects belongs to the `level.details` specification. In
  every `.dm` file they are 0, 0.5 and 2.0 [F3].
- After reading, the program computes the model's bounding box and sphere (used by neither `.dm` user
  [B11]) and may reorder the indices and vertices when a measure of vertex-cache cost decreases [B11]. The
  measure and the reordering are **unestablished**; that the reordered model draws the same triangles with
  the same vertex values is **believed**. For the bolt the order cannot change the picture, since its
  blending is additive (section 6.2).

### 2.4 Drawing a model

A user hands a 4 × 4 matrix *M*, a colour *c*, texture offsets Δu, Δv and a first vertex number *b*: every
vertex becomes the position (x, y, z, 1) · *M*, the colour *c*, the texture coordinate (u + Δu, v + Δv),
and every index *i* becomes *b* + *i*; the triangles are drawn as a list [B11]. (A second form without
texture offsets, Δu = Δv = 0, also exists [B11].)

### 2.5 The shipped files

| File | Bytes | Shader | Texture | *N*v | *N*i | Triangles | Position range | Texture range | CRC-32 |
|---|---|---|---|---|---|---|---|---|---|
| `dm_lightning-01-glow.dm` | 44,929 | `effects\lightning` | `fx\fx_lightning-glow` | 1,753 | 4,905 | 1,635 | x −0.290…0.278, y −0.994…0.036, z −0.165…0.288 | u 0…1, v 0.010…0.419 | `353A41DD` |
| `dm_lightning-01.dm` | 42,360 | `effects\lightning` | `fx\fx_lightning` | 1,644 | 4,713 | 1,571 | x −0.252…0.242, y −0.954…0.001, z −0.126…0.257 | u 0…1, v 0.010…0.419 | `A7592A69` |
| `dm_lightning-02-glow.dm` | 32,194 | `effects\lightning` | `fx\fx_lightning` | 1,250 | 3,570 | 1,190 | x −0.113…0.260, y −0.929…0.033, z −0.290…0.205 | u 0…1, v 0.045…0.477 | `0A328B5D` |
| `dm_lightning-02.dm` | 30,108 | `effects\lightning` | `fx\fx_lightning` | 1,167 | 3,357 | 1,119 | x −0.092…0.227, y −0.907…0.005, z −0.266…0.180 | u 0…1, v 0.045…0.466 | `E46F141C` |
| `dm_lightning-03-glow.dm` | 30,916 | `effects\lightning` | `fx\fx_lightning` | 1,196 | 3,471 | 1,157 | x −0.538…0.335, y −0.853…0.094, z −0.299…0.181 | u 0…1, v 0.010…0.444 | `FDFCBA99` |
| `dm_lightning-03.dm` | 30,942 | `effects\lightning` | `fx\fx_lightning` | 1,197 | 3,474 | 1,158 | x −0.506…0.289, y −0.817…0.035, z −0.263…0.147 | u 0…1, v 0.010…0.444 | `6D945B37` |
| `dm_lightning-04-glow.dm` | 44,614 | `effects\lightning` | `fx\fx_lightning` | 1,733 | 4,950 | 1,650 | x −0.174…0.326, y −0.794…0.079, z −0.590…0.688 | u 0…1.009, v 0.025…0.483 | `902638A1` |
| `dm_lightning-04.dm` | 44,898 | `effects\lightning` | `fx\fx_lightning` | 1,743 | 4,992 | 1,664 | x −0.165…0.292, y −0.766…0.042, z −0.548…0.646 | u 0…1.009, v 0.025…0.483 | `F421BEB2` |
| `dm_lightning_stancia_01.dm` | 23,346 | `effects\lightning` | `pfx\pfx_lighting_stancia_01` | 774 | 3,900 | 1,300 | x −0.500…0.500, y −0.131…0.126, z −0.500…0.500 | u 0…1, v −0.002…0.413 | `BFB8B695` |
| `dm_lightning_stancia_02.dm` | 8,266 | `effects\lightning` | `pfx\pfx_lighting_stancia_01` | 272 | 1,380 | 460 | x −0.500…0.500, y −0.046…0.014, z −0.500…0.500 | u 0…1, v −0.002…0.174 | `BFFEA5A0` |
| `dm_lightning_stancia_03.dm` | 12,966 | `effects\lightning` | `pfx\pfx_lighting_stancia_02` | 429 | 2,160 | 720 | x −0.477…0.504, y −0.036…0.312, z −0.500…0.503 | u 0…1, v 0.003…0.242 | `D364963C` |
| `dm_lightning_stancia_05.dm` | 43,722 | `effects\lightning` | `fx\fx_lightning` | 1,710 | 4,734 | 1,578 | x −0.330…0.460, y −0.030…0.403, z −0.469…0.401 | u 0…1, v 0.010…0.245 | `1086B4D7` |
| `rain.dm` | 423 | `effects\rain` | `fx\fx_rainsplash1` | 15 | 36 | 12 | x −0.090…0.090, y 0…0.220, z −0.090…0.090 | u 0…1, v 0…1 | `C8EEE52D` |

(Ranges rounded to 0.001 [F4].) Invariants [F2 – F4]: every index count is a multiple of 3, every index is
below the vertex count and every vertex is used; all 13 files have flags 0 and scales 0.5, 2.0; 14,883
vertices and 45,642 indices in all. The eight `dm_lightning-*` models hang below their origin (y −1 … 0.1)
and keep v below 0.5; the four stancia models are flat in y (extent below 0.45) and also keep v below 0.5.
`rain.dm` vertex 0 is (0.02, 0.22, 0.02, u 1, v 0) and its first triangles are (4, 0, 3), (4, 3, 2) [F4].
Named by the configuration: `dm_lightning-01` … `-04` and `dm_lightning_stancia_01` … `_03` (section 7);
the four `-glow` models and `dm_lightning_stancia_05` are named nowhere [F6].

## 3. The bolt description: picture keys

Besides `lightning_model` and `sound` (S8 section 2.2), a bolt section is read, in this order, for
[B2]:

| Key | Read as | Use |
|---|---|---|
| `color_anim` | text: a `lanims.xr` entry name | the flash and the sprites' fade (section 5) |
| `gradient_top_shader`, `gradient_top_texture` | texts | one shader from the pair (SS sections 3, 4): the top sprite |
| `gradient_top_radius` | two floats (*a*ₜ, *b*ₜ) | the top sprite's half width and half height per metre of bolt length |
| `gradient_top_opacity` | float *o* | the opacity of **both** sprites |
| `gradient_center_shader`, `gradient_center_texture` | texts | the centre sprite's shader |
| `gradient_center_radius` | two floats (*a*c, *b*c) | the centre sprite's half sizes per metre |
| `gradient_center_opacity` | float | read and never used [B2] |

Then `lightning_model` is opened and read as a `.dm` model (section 2) [B2]. The colour animation is
found by name (LA section 4) [B2]; what happens when no entry has the name is **unestablished**
(believed: the game fails). Once found, the entry's frame rate is **set to its frame count** in the
library itself, so its length (LA section 6.2) becomes 1 s for every later user of the entry [B2]. (In
this edition only the bolt sections name the entry it uses, and its rate already equals its frame count
[D3].)

From `[thunderbolt_common]` (read once when the program starts, S8 section 2.2) [B3]:

| Key | Read as |
|---|---|
| `tilt` | float, degrees, converted to radians (× 0.017453292): τ, the largest tilt of a bolt |
| `sky_color`, `sun_color`, `fog_color` | floats *k*sky, *k*sun, *k*fog: how much of the flash colour each value gets |

## 4. A bolt's shape

### 4.1 Tilt

When a bolt starts, after its distance *s* (S8 section 5.3), three numbers are drawn, in this order: θ₀ =
U(−τ, τ), θ₁ = *r* × 1.9175345 × 10⁻⁴ for a new draw *r* (so 0 … 2π), θ₂ = U(−τ, τ) [B3]. These are S8
section 6's "three numbers for the picture"; then come the ray test (section 4.2) and S8's sound decision
[B3].

With *s*ₖ = sin(−θₖ) and *c*ₖ = cos(−θₖ), the bolt's rotation has the rows [B4]:

- **i** = (*c*₂*c*₁ − *s*₂*s*₁*s*₀, −*s*₂*c*₀, *s*₂*c*₁*s*₀ + *c*₂*s*₁)
- **j** = (*c*₂*s*₁*s*₀ + *s*₂*c*₁, *c*₀*c*₂, *s*₂*s*₁ − *c*₂*c*₁*s*₀)
- **k** = (−*c*₀*s*₁, *s*₀, *c*₀*c*₁)

(an orthonormal basis with **k** = **i** × **j** [M5]). With no tilt, **j** = (0, 1, 0). The bolt's
**axis** is **a** = −**j**, pointing down from the bolt's point when untilted [B5].

### 4.2 Length

The length *L* starts at 2 *F* [B5]. A ray from **p** along **a**, at most *L* long, is tested
[B5]:

1. Against the level's static collision mesh (`level-cform.md`) and its objects, except the view object
   (the player in normal play). A triangle (**v**₀, **v**₁, **v**₂) of the static mesh is hit only from
   its front: when **a** · ((**v**₁ − **v**₀) × (**v**₂ − **v**₀)) ≤ −10⁻⁵ (the determinant of the
   ray-triangle test is at least 10⁻⁵), at a distance greater than 0 and at most *L*, inside the
   triangle; the nearest hit is kept [B5]. How an object answers the ray is **unestablished** (no
   approved specification covers objects' collision forms); an implementation that tests only the static
   mesh differs only when a bolt's axis meets an object. If anything is hit, *L* becomes the distance of
   the nearest hit and step 2 is skipped.
2. Otherwise, against the plane *y* = 0 of the world (normal (0, −1, 0)): with *q* = −*a*ᵧ, if
   |*q*| < 10⁻⁷ the plane is not met; otherwise its distance is *t* = **p**ᵧ / *q*, and the plane counts
   when *t* > 0 or |*t*| < 10⁻⁷; then, if *t* ≤ *L*, *L* becomes *t* [B5].

So a bolt reaches down to the ground (the static mesh, from above), to the plane *y* = 0 when it sees no
ground, or 2 *F* at most. *L* is fixed for the bolt's life.

### 4.3 Placement

The model's matrix *M* has the rows *L* · **i**, *L* · **j**, *L* · **k** (each with fourth component 0)
and the fourth row (**p**, 1): the model is scaled uniformly by *L*, rotated, and its origin put at **p**
[B6]. The model's (0, −1, 0) lands at **p** + *L* · **a**; the shipped `dm_lightning-*` models reach
down to y = −0.77 … −0.99 (section 2.5), so their drawn bolt ends 77 – 99 % of the way there [F4].

The two sprites are centred on the **top point** **p** and the **centre point** **m** = **p** +
(*L* / 2) · **a** [B6]. *M*, **p** and **m** do not change while the bolt runs; the sprites turn to face
the camera every frame (section 6.3). The thunder's position and range (S8 section 5.5) use *s* and **p**,
not *L* (S8 section 5.6).

## 5. The flash

### 5.1 Each environment update

S8 section 5.2 step 3 is, in full, for a running bolt [B7]: if its elapsed time *e* > λ it stops running;
*e* increases by *dt*; then, with the new *e*, whether or not it just stopped:

- *t* = *e* / λ; the flash colour is the colour animation's colour at time *t* (LA section 6.2), with the
  frame rate as replaced in section 3 (so frame floor(frac(*t*) · frame count)); its red, green and blue
  channels (LA section 5: bits 16–23, 8–15, 0–7) times the 32-bit float nearest to 1/255 give
  **c** = (*c*r, *c*g, *c*b) in 0 … 1; its alpha is not used [B7];
- the bolt's **phase** φ = 1.5 · *t*, limited to 0 … 1 [B7];
- the blended **sky colour** gains *k*sky · **c**, the blended **sun colour** *k*sun · **c**, the blended
  **fog colour** *k*fog · **c** [B7].

The blended values are those of this update (S8 section 2.3; SS section 9.2; sky.md section 3.3: sky and
fog colours after the environment modifiers, the sun colour without) [B8]. The update order is: the blend,
the lens flares' own update, the thunder (S8 section 5.2, with this flash), the rain [B8]. Because the
blend writes these values afresh on every update, a flash lasts one update and never accumulates [B8].
(While the environment is not updated, e.g. while the game is paused, the last values are kept,
**believed**: S8 section 3.2.)

So a bolt adds a flash on every update from the one it starts on to the one it stops on, which is one
update more than it is drawn (section 6.1) [B7, M1].

### 5.2 Who sees the flash (first renderer)

A frame's environment update happens before that frame is drawn [B8]. Then:

- the **sky box** colour (sky.md section 5.3) is made from the sky colour when the sky is drawn, so it
  includes the flash; its bytes are not clamped (sky.md's byte of *x*) [B8]. Without environment
  modifiers no sky colour component exceeds 0.6 with the flash, since no section stores more than 0.5
  [D6]; with modifiers a component of 1.0039 or more would wrap (**believed** not to occur; not examined);
- the **fog colour** state (SS section 9.3) is set from the fog colour after the thunder and the rain, as
  clamped bytes [B8];
- **L_sun_color** (SS section 6) is the sun colour, read once per frame when a pass first uses it, with
  w = 0 [B8]; so every pass and object lit by it (SS sections 7.3, 8.4; `model-shading.md` section 7) sees the flash;
- **not** affected: the sun sprite, the lens flares and their gradient (sky.md section 7), which take the
  sun colour in their own update, before the flash [B8]; the clouds, ambient and hemi colours (no flash
  is added to them) [B7]; the sun direction. With a renderer other than the first, the sun direction is
  replaced by the bolt's (−**d**) while it runs; the first renderer reports a different identity, so this
  does not happen here [B1].
  **Disputed in review:** the replacement is tied to one particular renderer identity, not to every
  renderer other than the first; which identity the second renderer reports was not examined, so that it
  happens there is unbacked. For the first renderer the statement stands.

The bolt creates no dynamic light and changes no light of the level: the thunderbolt uses the renderer
only to make its model and to ask its identity [B10]. Its light on the world is the flash alone.

## 6. Drawing

### 6.1 When

In each rendered frame, after the lens flares (SS section 10.3 step 7), the rain is drawn and then the
bolt, if a level is loaded and the bolt is running (S8 section 5.1) at that moment; then the frame's final
pass follows (SS section 10.2) [B9]. The bolt's elapsed time and phase are those of the last update. All
positions are already world positions (world matrix identity) [B9].

### 6.2 The model

The model (section 2.4) is drawn with *M* (section 4.3), colour 0xFFFFFFFF (white, opaque), Δu = 0 and
[B9]:

- Δv = φ / 2 while φ ≤ 0.5;
- Δv = 0.5 · (*r* mod 2), for a new draw *r* each drawn frame, once φ > 0.5 (so 0 or 0.5 at random).

So the texture slides by up to a quarter while φ rises to 0.5 (the first third of the life), then jumps
at random between its lower and upper halves (the shipped lightning models keep v below 0.5, section
2.5) [B9, F4]. These draws are made by the drawing, in addition to S8 section 6's.

States: culling off for the model, then back to the default (SS section 10.1) for the sprites [B9]; the
rest from the model's shader. `effects\lightning` is an `S_SET` definition with the same flags as
`effects\sun` (sky.md section 7.5): blending 5 (source alpha, one: additive), alpha test reference 2,
texture clamp, depth test on, depth write off, fog off [D4]. With white vertices the model adds its
texel colour × texel alpha to the frame (sky.md section 7.5's colour equation) wherever it passes the depth
and alpha tests; it does not fade (only the sprites do) [B9, D4].

### 6.3 The sprites

Two quads, the top sprite first and then the centre sprite, each with its own shader (section 3) [B9]. For
centre **C**, half width *h*x and half height *h*y, with the frame's camera right **r** and up **u′** (VP
section 1.1) [B9]:

| Corner | Position | Texture coordinate |
|---|---|---|
| 0 | **C** + *h*x **r** + *h*y **u′** | (0, 0) |
| 1 | **C** + *h*x **r** − *h*y **u′** | (0, 1) |
| 2 | **C** − *h*x **r** + *h*y **u′** | (1, 0) |
| 3 | **C** − *h*x **r** − *h*y **u′** | (1, 1) |

drawn as the triangles (0, 1, 2), (3, 2, 1), like sky.md section 7.4's quads (*h*x = *h*y there) [B9].

- Top sprite: **C** = **p**, *h*x = *a*ₜ · *L*, *h*y = *b*ₜ · *L*.
- Centre sprite: **C** = **m**, *h*x = *a*c · *L*, *h*y = *b*c · *L*.
- Both: every vertex colour has all four bytes (A, R, G, B) equal to the byte of *o* · φ (*o* the top
  opacity) [B2, B9].

With `effects\sun` (sky.md section 7.5) and κ = (byte of *o* · φ) / 255, each sprite adds texel colour ×
κ × texel alpha × κ where it passes the tests, so the sprites fade in over the first two thirds of the life and stay
at full strength after.

### 6.4 Visibility

The model and sprites are tested against the depth of everything drawn before (depth write off, so they
hide nothing) and clipped by the frame's projection and far plane (VP sections 2, 2.1) [B9, D4]. The sky,
drawn at depth 0.99999 … 1 (sky.md section 5.5), does not hide them.

## 7. Data of this edition

`[thunderbolt_common]`: `tilt = 17` (τ = 17°), `sky_color = 0.1`, `sun_color = 0.9`, `fog_color = 0.1`
[D1]. Every bolt section uses `color_anim = weathers\thunderbolt_00` and both sprite shaders
`effects\sun` [D2]:

| Bolt section | `lightning_model` | top *o* | top radius | top texture | centre opacity (unused) | centre radius | centre texture |
|---|---|---|---|---|---|---|---|
| `sect_thunderbolt_00` | `dm\dm_lightning-01.dm` | 0.6 | 0.5, 0.25 | `fx\fx_gradient` | 0.6 | 2, 1 | `fx\fx_gradient` |
| `sect_thunderbolt_01` | `dm\dm_lightning-02.dm` | 0.6 | 0.5, 0.25 | `fx\fx_gradient` | 0.6 | 2, 1 | `fx\fx_gradient` |
| `sect_thunderbolt_02` | `dm\dm_lightning-03.dm` | 0.6 | 0.5, 0.25 | `fx\fx_gradient` | 0.6 | 2, 1 | `fx\fx_gradient` |
| `sect_thunderbolt_03` | `dm\dm_lightning-04.dm` | 0.6 | 0.5, 0.25 | `fx\fx_gradient` | 0.6 | 2, 1 | `fx\fx_gradient` |
| `sect_thunderbolt_stancia_00` | `dm\dm_lightning_stancia_01.dm` | 0.75 | 2.5, 1.25 | `pfx\pfx_anomaly_8` | 1 | 2, 1 | `fx\fx_gradient` |
| `sect_thunderbolt_stancia_01` | `dm\dm_lightning_stancia_02.dm` | 1 | 3, 1.75 | `fx\fx_gradient` | 1 | 2, 1 | `fx\fx_gradient` |
| `sect_thunderbolt_stancia_02` | `dm\dm_lightning_stancia_03.dm` | 0.8 | 2, 1 | `fx\fx_gradient` | 1 | 2, 1 | `fx\fx_gradient` |
| `sect_thunderbolt_stancia_03` | `dm\dm_lightning-01.dm` | 0.6 | 0.5, 0.25 | `fx\fx_gradient` | 0.6 | 2, 1 | `fx\fx_gradient` |

`weathers\thunderbolt_00` (LA section 7, entry 38): frame rate 50, 50 frames (the rate stays 50 after
section 3), keys at frames 0, 27, 31, 42, 48, 49 with the colours 0x000000, 0xFFFFFF, 0x000000,
0xFFFFFF, 0xFFFFFF, 0x000000 (alpha 0): black rising to white over 27 frames, a fall to black, a second
rise to white held to frame 48 [D3]. The flash is grey (equal channels); its strongest values add 0.1 to
the sky and fog colours and 0.9 to the sun colour [M2]. The configuration and scripts name the entry 8
times, all in these sections [D3].

Textures (all exist) [D5]: `fx\fx_lightning` 64 × 512, 10 levels, uncompressed; `pfx\pfx_lighting_stancia_01`
and `_02` 256 × 512, 10 levels, DXT5; `pfx\pfx_anomaly_8` 256 × 256, 9 levels, DXT5; `fx\fx_gradient`
64 × 64, 7 levels, DXT5; (unused by bolts: `fx\fx_lightning-glow` 64 × 64 DXT3, `fx\fx_rainsplash1`
128 × 128 DXT5).

## 8. Worked examples

Computed by `model.py` from sections 3 – 6 and LA [M1 – M6].

### 8.1 One bolt's flash

Life λ = 0.25 s, *dt* = 0.02 s, the bolt starting on update 0 [M1]:

| Update | *e* (s) | *t* | Frame | Colour | φ | Δv | Sky and fog gain | Sun gain | Drawn |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 0.02 | 0.08 | 4 | 0x252525 | 0.12 | 0.06 | 0.0145 | 0.1306 | yes |
| 1 | 0.04 | 0.16 | 8 | 0x4B4B4B | 0.24 | 0.12 | 0.0294 | 0.2647 | yes |
| 2 | 0.06 | 0.24 | 12 | 0x717171 | 0.36 | 0.18 | 0.0443 | 0.3988 | yes |
| 3 | 0.08 | 0.32 | 16 | 0x979797 | 0.48 | 0.24 | 0.0592 | 0.5329 | yes |
| 4 | 0.10 | 0.40 | 19 | 0xB3B3B3 | 0.60 | 0 or 0.5 | 0.0702 | 0.6318 | yes |
| 5 | 0.12 | 0.48 | 23 | 0xD9D9D9 | 0.72 | 0 or 0.5 | 0.0851 | 0.7659 | yes |
| 6 | 0.14 | 0.56 | 27 | 0xFFFFFF | 0.84 | 0 or 0.5 | 0.1 | 0.9 | yes |
| 7 | 0.16 | 0.64 | 31 | 0x000000 | 0.96 | 0 or 0.5 | 0 | 0 | yes |
| 8 | 0.18 | 0.72 | 35 | 0x5C5C5C | 1 | 0 or 0.5 | 0.0361 | 0.3247 | yes |
| 9 | 0.20 | 0.80 | 39 | 0xB9B9B9 | 1 | 0 or 0.5 | 0.0725 | 0.6529 | yes |
| 10 | 0.22 | 0.88 | 43 | 0xFFFFFF | 1 | 0 or 0.5 | 0.1 | 0.9 | yes |
| 11 | 0.24 | 0.96 | 47 | 0xFFFFFF | 1 | 0 or 0.5 | 0.1 | 0.9 | yes |
| 12 | 0.26 | 1.04 | 1 | 0x090909 | 1 | 0 or 0.5 | 0.0035 | 0.0318 | yes |
| 13 | 0.28 | 1.12 | 5 | 0x2F2F2F | 1 | — | 0.0184 | 0.1659 | no (stops) |

(*e* is summed in 32-bit float: at update 4 it is just below 0.1, so frame 19, not 20.) The bolt is drawn
13 times and flashes 14 times; on its last two updates *t* > 1 and the colour starts again from the
animation's beginning [M1]. The sprite bytes for *o* = 0.6 are 18 at φ = 0.12, 73 at φ = 0.48 and 153 at
φ = 1 [M6].

### 8.2 Shape

The eye at the origin, *F* = 350 m, the bolt of S8 section 8.2 (heading 111°, altitude 15°, **d** =
(−0.90177, 0.25882, −0.34616)), *s* = 329 m, so **p** = (−296.682, 85.152, −113.887); no collision
triangle or object on the axis [M3, M4]:

- No tilt (θ = 0, 0, 0): **a** = (0, −1, 0); the plane *y* = 0 is 85.152 m below, so *L* = 85.152 m;
  **m** = (−296.682, 42.576, −113.887); the top sprite is 42.576 m × 21.288 m (half sizes), the centre
  sprite 170.304 m × 85.152 m [M3].
- Tilt θ = (10°, 90°, −5°): **i** = (−0.01513, −0.08583, −0.99619), **j** = (0.17299, 0.98106, −0.08716),
  **k** = (0.98481, −0.17365, 0); **a** = (−0.17299, −0.98106, 0.08716); *L* = 86.796 m; **m** =
  (−304.190, 42.576, −110.104); the model's (0, −1, 0) lands on the plane, at (−311.697, 0, −106.322) [M4].

## 9. Verification facts

- `.dm` files: section 2.5 (sizes, shaders, textures, counts, ranges, CRC-32, invariants) [F1 – F4]; the
  481 models of the 23 `level.details` files (all with shader `details\blend`, flags 0 in 294 and 1 in
  187) decode with section 2.2, each filling its sub-chunk exactly [F5].
- Bolt sections, `[thunderbolt_common]`, the colour animation and the textures: section 7 [D1 – D5].
- `effects\lightning` and `effects\sun` in `shaders.xr`: `S_SET`, blending 5, alpha reference 2, texture
  clamp, depth test, no depth write, fog off, not strict-sorted [D4].
- The worked examples of section 8 [M1 – M6].

## 10. Unestablished

- How an object (other than the static mesh) answers the bolt's ray (section 4.2).
- What happens when `color_anim` names no entry (believed: the game fails) (section 3).
- The vertex-cache measure and reordering after reading a `.dm` file; that it keeps the same triangles is
  believed (section 2.3).
- An index at or above the vertex count (section 2.3; none occurs).
- Whether environment modifiers ever push a flashed sky colour component to 1.0039 or more, where its byte
  would wrap (section 5.2).
- That the last environment values (with any flash) persist while the environment is not updated
  (section 5.1, believed).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`, and the shipped program files of `bin\`.
- Data examined: every `.dm` file and every stored copy of it; every `level.details` (chunks 0 and 1);
  `config\weathers\thunderbolt.ltx`, `environment.ltx` and the weather files it includes; every
  configuration and script file (searched for names); `lanims.xr`; `shaders.xr`; the textures the bolt
  and `.dm` files name; the program files (searched for `.dm` names).
- Analysis: static analysis (decompilation and disassembly with Ghidra, headless) of the engine
  executable, the first renderer and the collision library; constants read from the program files. No
  program was executed. Independent throwaway readers written from this text and from the approved
  specifications decode every `.dm` file and `level.details` model, the configuration, `lanims.xr` and
  `shaders.xr`; a model written from this text reproduces section 8.

## Gaps found in review

- What happens when a bolt section's `lightning_model` is empty or names a missing file, or when one of the
  section 3 keys is missing, is not stated (none occurs in this edition).
- What happens when a `.dm` shader or texture string is 256 bytes or longer, beyond the 256-byte buffers of
  section 2.2, is not stated (none occurs).
