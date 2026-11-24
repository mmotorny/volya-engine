# Sky and environment around the static world (first renderer, New Game start)

Behaviour-and-data specification of what surrounds the static world of the first single-player level
(`levels\l01_escape\`) when a New Game reaches its first in-game frame in the game's first (default,
static-lighting) renderer: the sky's shape, size and placement, its textures and how they are chosen,
blended and sampled, its colour, the cloud layer, the sun and its lens flares, how fog and the far plane
meet the sky, the effect of `level.env_mod` on that view, and the drawing order. Where a fact could not be
established it is marked **unestablished**.

Conventions:

- Builds on the scoping specification (`docs/behavior/level-loading.md`, §2.6, §5.8, §5.15), the
  static-shading specification (`docs/behavior/static-shading.md`, here "SS"; its §9 weather inputs,
  hourly blending and fog are used, not repeated), the start-position specification
  (`docs/behavior/start-position.md`, here "SP"; eye, camera basis, projection, far plane) and the LTX and
  archive specifications.
- Coordinates are world (level) metres, +y up, left-handed (SP §1). Vectors are rows; `·` is the dot
  product, `×` the ordinary cross product of components; `lerp(a,b,t) = a + (b − a)t`;
  `sat(x) = clamp(x, 0, 1)`; `trunc` rounds toward zero.
- "Byte of x" means `trunc(255·x)` stored in 8 bits. Where this document says "clamped byte" it means
  `clamp(trunc(255·x), 0, 255)`. A Direct3D colour (D3DCOLOR) packs bytes as A (bits 24–31), R (16–23),
  G (8–15), B (0–7); a program receiving it as a colour input sees (R, G, B, A)/255 in (x, y, z, w).
- Matrices act on row vectors (`p · M`). "WVP" is world × view × projection; view and projection are those
  of SP §6, §7.
- Render and sampler states are named by meaning, with the Direct3D 9 public enumeration value in brackets
  where useful.
- E is the eye position, F the camera viewing direction, r the camera right vector and u' the camera up
  vector of SP §6 (all unit vectors); T is the global time in seconds (the same clock as SS §6 `time`); dt
  is the frame time in seconds.

## 1. The start: time, weather, sections, blend weight

| Item | Value | Source |
|---|---|---|
| Game time of day | 05:30:00 (τ = 19,800 s) | scoping §2.6 (`config\alife.ltx`) |
| Weather | `default` → cycle section `[sect_default_weather]` (24 hourly sections) | scoping §2.6 |
| Bracketing sections | A = `[default_weather_05]` (05:00:00), B = `[default_weather_06]` (06:00:00) | `config\weathers\weather_default.ltx` |
| Blend weight | t = 0.5 at 05:30:00 exactly (SS §9.2); t grows by 1/3,600 per game second | |
| Eye | (−246.72630, −23.263261, −134.43387) | SP §4 |
| View direction F | (0.984797, −0.0043165, 0.173655) | SP §5 |
| Environment modifiers in range | records 1 and 6 of `levels\l01_escape\level.env_mod`, weights 1.59990 and 1.57357, Σw = W = 3.17347 | §9; SP §7.1 |

Every frame the environment is blended from A, B, t and the modifiers in range (SS §9.2, extended by §3
below). All facts below that depend on the start use these inputs.

## 2. Inputs read

| File | Use |
|---|---|
| `config\weathers\environment.ltx` and its includes (`weather_*.ltx`, `flares.ltx`, …) | weather cycles and hourly sections (§3), lens-flare sections (§7) |
| `textures\sky\<name>.dds` named by `sky_texture` | sky cube maps (§5) |
| `textures\sky\<name>#small.dds` | loaded for every section, not drawn by the first renderer's sky (§3.2) |
| `textures\sky\sky_oblaka.dds` named by `clouds_texture` | cloud layers (§6) |
| `textures\fx\*.dds` named by lens-flare sections | sun sprite, gradient, flares (§7) |
| `shaders\r1\sky2.vs`, `sky2.ps` | sky programs (§5.4) |
| `shaders\r1\clouds.s`, `clouds.vs`, `clouds.ps`, `shared\cloudconfig.h`, `common.h` | cloud pass (§6.3) |
| `shaders.xr` definitions `effects\sun`, `effects\flare` | sun sprite and flare passes (§7.5) |
| `levels\<level>\level.env_mod` | environment modifiers (§9) |

The program and script files are game data (HLSL / Lua) and are read as the definition of the equations
below.

## 3. Per-section sky values and their blending

### 3.1 Keys read from each hourly section

In addition to the keys of SS §9.1:

| Key | Format | Stored value |
|---|---|---|
| `sky_texture` | texture name | S, the sky cube map; also S + `#small` (§3.2) |
| `sky_color` | 3 floats | **0.5 ×** the three values |
| `sky_rotation` | 1 float, degrees; optional (absent = 0) | θ = value × π/180 (radians) |
| `clouds_texture` | texture name | the cloud texture |
| `clouds_color` | 5 floats `r, g, b, a, k` (`k` absent = 0) | colour (r·k/2, g·k/2, b·k/2), opacity a |
| `flares` | name of a lens-flare section, or empty | §7 (empty: no sun sprite, gradient or flares) |
| `rain_density` | float, clamped to [0, 1] | rain (§11) |
| `thunderbolt` | name, or empty | thunderbolts (§11) |

A message (not an error) is logged when `sky_color` (as stored, after × 0.5) or the stored clouds colour
leaves [0, 2] per component; nothing else changes.

Texture names are resolved like every other texture name (SS §4.1, §4.2).

### 3.2 Textures created per section

For every hourly section of every weather cycle listed in `[weathers]` and every weather effect listed in
`[weather_effects]` of `environment.ltx` (123 sections in this edition), three textures are created: the
sky cube map S, the cube map S + `#small`, and the cloud texture. The `#small` cube maps are not bound by
any pass described here; any other use by the first renderer is **unestablished** (they need not be read
for the start view).

### 3.3 Blending (extends SS §9.2)

With A, B, t of §1 and modifiers summed as in SS §9.2 (M = Σ ω·field, W = Σ ω):

- sky colour C_sky = (lerp(A.sky_color, B.sky_color, t) + M.sky_color) / (1 + W), with the stored (× 0.5)
  section values and the modifiers' raw `sky_color` field (§9.1; modifiers are **not** halved);
- sky rotation θ_sky = lerp(A.θ, B.θ, t) (no modifier);
- clouds colour and opacity: component-wise lerp of the stored values (no modifier);
- textures are not blended at this stage: the sky pass binds A's cube map and B's cube map, the cloud
  pass binds A's cloud texture and B's cloud texture (§5, §6);
- the lens-flare section in effect is A's if t < 0.5, otherwise B's (§7.2).

## 4. Values at the start (first frame)

Sections A and B (as written in `weather_default.ltx`):

| Key | A `default_weather_05` | B `default_weather_06` |
|---|---|---|
| `sky_texture` | `sky\sky_11_cube` | `sky\sky_2_cube` |
| `sky_color` | 0.8, 0.8, 0.8 | 1.0, 1.0, 1.0 |
| `sky_rotation` | 0 | 0 |
| `clouds_texture` | `sky\sky_oblaka` | `sky\sky_oblaka` |
| `clouds_color` | 0.353, 0.333, 0.420, 0.5, 1.7 | 0.698, 0.447, 0.322, 0.5, 1.7 |
| `flares` | `flares_sun_rise` | `flares_sun_rise` |
| `sun_dir`, `sun_color` | −10, 291; 0.700, 0.284, 0.096 | −14.0, 291; 0.818, 0.476, 0.108 |
| `rain_density`, `thunderbolt` | 0.0, empty | 0.0, empty |

Blended at t = 0.5 with the two modifiers of §1:

| Quantity | Value |
|---|---|
| C_sky | (0.68043, 0.69522, 0.71001) → bytes (173, 177, 181); without modifiers it would be (0.45, 0.45, 0.45) → (114, 114, 114) |
| sky blend byte (§5.3) | trunc(255 × 0.5) = 127 |
| θ_sky | 0 |
| clouds colour, opacity | (0.44667, 0.33150, 0.31535), 0.5 → bytes (113, 84, 80), 127 |
| L_sun_dir (SS §9.1, §9.2) | (0.91318, −0.20791, 0.35054); sun direction D_s = −L_sun_dir |
| sun colour | (0.759, 0.380, 0.102) |
| far plane | 254.478 m (SP §7.1) |
| fog colour | (0.31374, 0.31702, 0.32380) → fog bytes (80, 80, 82) |
| fog density, near, far | 0.97604; fog_near = 7.1284 m, fog_far = 346.5 m (SS §9.2) |
| ambient | (0.04814, 0.05170, 0.05525) |
| hemi colour (rgb) | (0.37496, 0.39573, 0.41801) |

(SS §9.4 gives the same quantities for a camera with no modifier in range; at the actual start eye the two
modifiers apply and the values above hold.)

## 5. The sky box

### 5.1 Geometry

A fixed box of 12 vertices and 20 triangles built into the program (no file is read). Each vertex has a
position p and a three-component texture coordinate c:

| # | p | c |
|---|---|---|
| 0 | (−1, −1, −1) | (−1, −1.01, −1) |
| 1 | (1, −1, −1) | (1, −1.01, −1) |
| 2 | (−1, −1, 1) | (−1, −1.01, 1) |
| 3 | (1, −1, 1) | (1, −1.01, 1) |
| 4 | (−1, 1, −1) | (−1, 1, −1) |
| 5 | (1, 1, −1) | (1, 1, −1) |
| 6 | (−1, 1, 1) | (−1, 1, 1) |
| 7 | (1, 1, 1) | (1, 1, 1) |
| 8 | (−1, 0, −1) | (−1, −1, −1) |
| 9 | (1, 0, −1) | (1, −1, −1) |
| 10 | (1, 0, 1) | (1, −1, 1) |
| 11 | (−1, 0, 1) | (−1, −1, 1) |

Triangles (vertex indices, a triangle list):
(0,2,3) (3,1,0) (4,5,7) (7,6,4) (0,1,9) (9,8,0) (8,9,5) (5,4,8) (1,3,10) (10,9,1) (9,10,7) (7,5,9)
(3,2,11) (11,10,3) (10,11,6) (6,7,10) (2,0,8) (8,11,2) (11,8,4) (4,6,11).

So the box is the cube [−1, 1]³ with each side split at height 0: the bottom face (2 triangles), the top
face (2), four lower side quads between heights −1 and 0 and four upper side quads between 0 and 1 (2
triangles each). Every triangle is wound so that its normal (b − a) × (c − a) points toward the box's
centre; with the renderer's default culling (SS §10.1) all of them are front-facing for a viewer inside.

### 5.2 Placement

World matrix (rotation by θ_sky about +y, unit scale, translation to the eye):

    W_sky = [[ cos θ, 0, −sin θ, 0 ],
             [ 0,     1,  0,     0 ],
             [ sin θ, 0,  cos θ, 0 ],
             [ E.x,   E.y, E.z,  1 ]]

The box therefore always surrounds the eye at 1 m (faces) to √3 m (corners), moves with the camera and
does not rotate with it. With θ_sky = 0 (every shipped section, §13) it is axis-aligned. Its size has no
visible effect because of the depth range of §5.5.

### 5.3 Vertex colour

All 12 vertices carry one colour per frame: R, G, B = bytes of C_sky (§3.3), A = byte of t (the hourly
blend weight). The texture coordinates are passed unchanged; the program never transforms them by W_sky,
so the cube map turns with θ_sky together with the box.

### 5.4 Pass and colour equation

- Programs `sky2.vs` / `sky2.ps` (first renderer). Vertex stage: position × WVP; colour and both texture
  coordinates copied (texture coordinate 0 = texture coordinate 1 = c).
- Samplers: `s_sky0` = A's cube map (stage 0), `s_sky1` = B's cube map (stage 1); both clamp addressing
  [3], linear minification, **no mip**, linear magnification. (The cube maps have one level anyway, §13.)
- States: depth test on (less-or-equal), depth write **off**, blending off, alpha test off, fog **off**,
  default culling.

Pixel colour, with v = vertex colour as (R, G, B, A)/255 and s0, s1 the cube-map texels at c:

    C = sat( v.rgb · lerp(s0.rgb, s1.rgb, v.a) · 2 ),   alpha 1

At the start v.rgb = (173, 177, 181)/255 and v.a = 127/255: the texels of the two cube maps are mixed
0.502 : 0.498 and multiplied by 1.35686, 1.38824, 1.41961 (= 2·173/255, 2·177/255, 2·181/255) per
channel, then clamped.

### 5.5 Depth range

The sky box (and the cloud dome, §6) is drawn with the viewport depth range set to [0.99999, 1]
(minimum 0.99999, maximum 1), and the range is reset to [0, 1] right after each of them. Combined with the
depth test and no depth write, the sky is visible exactly where no geometry drawn before it is nearer — in
practice everywhere the static world does not cover — regardless of the box's 1-m size, and it never
occludes anything drawn later.

### 5.6 Which cube-map direction each screen direction shows

Because the attributes are interpolated perspective-correctly across planar triangles and each face's
texture coordinates are an affine function of the position on that face, the cube-map lookup direction c
for a viewing direction d (in the box's model space, i.e. after undoing θ_sky) is found by intersecting
the ray from the centre along d with the cube, at the point p = d / max(|d.x|, |d.y|, |d.z|):

| Part of the box hit | c |
|---|---|
| top face (p.y = 1) | p |
| upper sides (0 ≤ p.y ≤ 1, on a side face) | (p.x, 2·p.y − 1, p.z) |
| lower sides (−1 ≤ p.y < 0, on a side face) | (p.x, −1 + 0.01·p.y, p.z) |
| bottom face (p.y = −1) | (p.x, −1.01, p.z) |

Consequences, for viewing directions d (model space):

- Above the horizon, directions that hit an upper side quad show that side face of the cube map (|c.x| or
  |c.z| = 1) with its full height stretched: c.y runs from −1 at the horizon to +1 where the side quad meets
  the top face (45° elevation on a face's centre line, 35.3° toward the box's vertical edges). On a face's
  centre line c.y = 0, the middle of the side face, is shown at 26.6° elevation (p.y = 0.5). Steeper
  directions hit the top face and show the +y face exactly as a direct cube-map lookup would (c = p).
- At the horizon itself (p.y = 0) the lookup is (p.x, −1, p.z): the edge between a side face and the −y
  face, i.e. the border texels of the −y face.
- Below the horizon, directions that hit a lower side quad (shallower than the bottom face's boundary:
  |d.y| < max(|d.x|, |d.z|)) get c.y between −1 and −1.01 with |c.x| or |c.z| = 1, i.e. practically the
  same −y border texels as the horizon directly above them.
- Directions that hit the bottom face (|d.y| ≥ max(|d.x|, |d.z|)) get c = (p.x, −1.01, p.z) with p.x, p.z
  spanning [−1, 1]: the whole −y face, its face coordinates scaled by 1/1.01 (e.g. d = (0, −1, 0.2) shows
  a texel near the centre of the −y face). This part of the box must be drawn; it is not a continuation
  of the horizon colour.

The cube map is sampled with the public Direct3D cube-map face selection and face coordinates; the DDS
faces are in the public DDS order (+x, −x, +y, −y, +z, −z).

## 6. Clouds

### 6.1 Drawn or not

The cloud dome is drawn in a frame only if |blended clouds opacity| ≥ 0.001. At the start the opacity is
0.5: drawn. (31 of the 123 shipped sections have opacity 0, §13.)

### 6.2 Geometry and placement

A fixed dome of 91 vertices (positions only) and 160 triangles built into the program (no file is read).
It is exactly the upper half of a twice-subdivided icosahedron on the unit sphere:

1. Icosahedron: (0, 1, 0), (0, −1, 0), five vertices (c·cos(2πk/5), s, c·sin(2πk/5)) and five vertices
   (c·cos(2πk/5 + π/5), −s, c·sin(2πk/5 + π/5)), k = 0 … 4, with s = 1/√5, c = 2/√5; 20 faces: for each k,
   (top, U_k, U_{k+1}), (bottom, L_{k+1}, L_k), (U_k, L_k, U_{k+1}), (U_{k+1}, L_k, L_{k+1}) (indices mod 5).
2. Subdivide every triangle twice into four (edge midpoints, shared between neighbouring triangles,
   pushed out to the unit sphere): 162 vertices, 320 triangles.
3. Keep the 160 triangles whose three vertices all have y ≥ 0; they use 91 vertices, 20 of them on y = 0.

The built-in table equals this construction to within 6·10⁻⁶ per coordinate; vertex order and triangle
order are irrelevant to the result. Every triangle is wound so that its normal (b − a) × (c − a) points
toward the centre (front-facing from inside, default culling).

World matrix (horizontal scale 10, vertical scale 0.4, rotation θ_sky, translation to the eye):

    W_cl = [[ 10·cos θ, 0,   −10·sin θ, 0 ],
             [ 0,        0.4, 0,         0 ],
             [ 10·sin θ, 0,   10·cos θ,  0 ],
             [ E.x,      E.y, E.z,       1 ]]

So the clouds form a flattened dome over the eye (10 m across, 0.4 m high, rim at eye height); like the
sky it is drawn with the depth range [0.99999, 1] (§5.5), so its size is invisible.

### 6.3 Vertex data, pass and equations

Each vertex carries its dome position p (model space, as in the table, the program receives it as the
position), and two colours, the same for all vertices:

- colour 0 ("directions"): R, G, B, A = bytes of (½·w₀.x + ½, ½·w₀.z + ½, ½·w₁.z + ½, ½·w₁.x + ½), with
  w₀ = (−sin 45°, 0, cos 45°) and w₁ = (−sin 67.5°, 0, cos 67.5°): bytes R 37, G 217, B 176, A 9.
- colour 1: R, G, B = bytes of the blended clouds colour, A = byte of the blended clouds opacity
  (start: 113, 84, 80, 127).

Pass from the game's `shaders\r1\clouds.s`: programs `clouds.vs` / `clouds.ps`; fog off; depth test on
(less-or-equal), depth write off; blending source-alpha / inverse-source-alpha [5/6]; samplers `s_clouds0`
= A's cloud texture (stage 0) and `s_clouds1` = B's cloud texture (stage 1), wrap addressing, anisotropic
minification and magnification, linear mip (maximum anisotropy and mip bias as SS §4.4).

Vertex stage (constants from `shared\cloudconfig.h`; `timers.z` = T · 0.1):

    position  = p · W_cl · VP
    k0 = colour0.xy · 2 − 1,   k1 = colour0.wz · 2 − 1
    uv0 = p.xz · 0.7 + k0 · (0.1·T) · 0.1
    uv1 = p.xz · 2.8 + k1 · (0.1·T) · 0.05
    col = colour1,  col.a = colour1.a · (p.y)^25

(k0 = (37, 217)/255·2 − 1 = (−0.70980, 0.70196), k1 = (9, 176)/255·2 − 1 = (−0.92941, 0.38039). The
first layer drifts 0.01 texture units per second along k0, the second 0.005 along k1. The texture
coordinates follow the model-space position, so the clouds move with the eye.)

Pixel stage, with s0 = `s_clouds0` at uv0 and s1 = `s_clouds1` at uv1:

    C = sat( col.rgb · (s0.rgb + s1.rgb) ),   A = col.a

The textures' alpha is not used; the opacity is the per-vertex col.a only. Because of the (p.y)^25 factor
the clouds are visible only near the zenith: on the dome's vertex rings col.a is multiplied by 1 (y = 1),
0.379 (y = 0.96194), 0.0615 (y = 0.89442), 0.0175 (y = 0.85066) and practically 0 below.

## 7. Sun and lens flares

### 7.1 Direction and the start view

The sun is in direction D_s = −L_sun_dir (unit; SS §9.2), i.e. toward where the light comes from. At the
start D_s = (−0.91318, 0.20791, −0.35054): 12.0° above the horizontal, horizontal heading −111.0° (from
+z toward +x), while the player looks along heading +80.0°. F · D_s = −0.96107: **the sun is behind the
player and nothing of §7 is drawn in the start view.** The rest of §7 describes what appears when the view
turns toward the sun.

### 7.2 Lens-flare section in effect

The section named by `flares` of A (t < 0.5) or B (t ≥ 0.5) — at the start `[flares_sun_rise]` of
`config\weathers\flares.ltx`. Keys:

| Key | Meaning |
|---|---|
| `source` (bool) | draw the sun sprite; then `source_shader`, `source_texture`, `source_radius`, `source_ignore_color` are read |
| `flares` (bool) | draw flares; then `flare_shader`, and comma-separated lists `flare_textures`, `flare_radius`, `flare_opacity`, `flare_position` (one flare per item of `flare_textures`, the other lists indexed alike) |
| `gradient` (bool) | draw the glow; then `gradient_shader`, `gradient_texture`, `gradient_radius`, `gradient_opacity` |
| `blend_rise_time`, `blend_down_time` | seconds (negative → 0); fade rates λ↑ = 1/(`blend_rise_time` + 10⁻⁷), λ↓ = 1/(`blend_down_time` + 10⁻⁷) |

`[flares_sun_rise]`: source on (`fx\fx_sun_rise`, shader `effects\sun`, radius 0.03, colour not ignored);
flares on, six flares (texture, radius, opacity, position): (`fx\fx_flare1`, 0.08, 0.34, 1.3),
(`fx\fx_flare2`, 0.12, 0.26, 1.0), (`fx\fx_flare2`, 0.04, 0.50, 0.5), (`fx\fx_flare2`, 0.08, 0.42, −0.3),
(`fx\fx_flare3`, 0.12, 0.26, −0.6), (`fx\fx_flare1`, 0.30, 0.26, −1.0), shader `effects\flare`; gradient on
(`fx\fx_gradient`, shader `effects\flare`, radius 0.4, opacity 0.85); rise and down times 300 s. Texture
names written with `.tga` lose the extension (SS §4.1).

### 7.3 Visibility and placement

Every frame, with D = far plane (§4; SP §7.1), q = 0.75·D and c = F · D_s:

- If c ≤ 0.01, nothing of §7 is drawn this frame.
- Otherwise the axis point is P_a = E + q·F and the sun point is P_s = E + (q / c)·D_s (the point in
  direction D_s at view depth q). L = normalize(P_s − P_a) (the screen line from the centre to the sun).
- Occlusion value v (0 … 1, starts at 0): each visible frame it moves toward a target v* by at most 4·dt.
  v* = 1 when a ray from E along D_s, 1,000 m long, hits no static collision triangle (`level.cform`) or
  object; when it hits, v* is the product of the hit surfaces' transmission factors from the game-material
  library (0 for opaque surfaces). The material factors, the object test and the caching of the last hit
  triangle are **unestablished** in detail; a static implementation may take v* = 0 if the ray hits the
  level's collision mesh, else 1.
- Fade a (0 … 1, starts at 0), updated every frame whether or not the sun is visible: on the first frame
  of a level the section in effect is taken and a stays 0; then a rises by λ↑·dt·k_t per frame until it
  reaches 1. k_t is how many times faster game time runs than real time; it is not environment data but
  comes from the game's clock, which passes it to the environment together with the game time (before the
  game first does so the value is 12). For a New Game it is presumed to be `time_factor = 10` of
  `config\alife.ltx` (not traced). When the section in effect changes, a first falls by the old section's
  λ↓·dt·k_t to 0, then the new section is taken and a rises again. With `[flares_sun_rise]` and k_t = 10
  the sun, flares and gradient fade in over 30 real seconds after the level appears.

### 7.4 Sprites

All sprites are quads of four vertices (position, colour, texture coordinate) drawn as the two triangles
(0, 1, 2), (3, 2, 1), world matrix identity, normal depth range [0, 1]. With a centre C, a half-size h and
two unit axes X, Y the corners are: 0 = C + hX − hY, texcoord (0, 0); 1 = C + hX + hY, (0, 1);
2 = C − hX − hY, (1, 0); 3 = C − hX + hY, (1, 1).

| Sprite | Drawn when | C | h | X, Y | Colour (clamped bytes) |
|---|---|---|---|---|---|
| sun source | `source` on | P_s | `source_radius` · q | r, r × F | rgb = sun colour (or 1, 1, 1 if `source_ignore_color`); A = a |
| flare i | `flares` on and v ≥ 0.001 | P_a + `flare_position`ᵢ · (P_s − P_a) | `flare_radius`ᵢ · q | L, L × F | rgb = sun colour · f, A = f, f = `flare_opacity`ᵢ · v · a |
| gradient | `gradient` on, v ≥ 0.001 and g ≥ 0.001 | P_s | `gradient_radius` · g · q | r, r × F | rgb = sun colour · a·g, A = a·g |

(r × F equals −u'.) The gradient strength is g = `gradient_opacity` · a · v · e_x · e_y, where, with
(x, y) = the sun point's normalized device coordinates (clip x/w, clip y/w): e_x = 1 if |x| ≤ 0.5, else
(2.5 − |x|)/2; e_y likewise; g = 0 if |x| > 2.5 or |y| > 2.5.

Because every size is proportional to q and the sprite lies at view depth q, the sprites' angular sizes do
not depend on the far plane: the sun source's half-size is `source_radius` × the distance to its centre
along the view axis.

### 7.5 Sprite passes

Both shaders are library definitions (`shaders.xr`, class `S_SET   `, SS §7.5): `effects\sun` — blending
`ALPHA-ADD` (source-alpha / one, alpha test ref 2), texture clamp, depth test on, depth write off, fog off;
`effects\flare` — the same but depth test **off**. Colour = texel × vertex colour, alpha = texel alpha ×
vertex alpha. The sun source is therefore hidden by any geometry nearer than q; flares and gradient are
drawn over everything and fade with v instead.

## 8. Fog, far plane and the sky

- The sky box, the cloud dome and all sun/flare sprites are drawn with fog off; the fog of SS §9.3 applies
  only to the geometry passes that have fog on.
- Geometry beyond the far plane (start: 254.478 m) is clipped; there the sky shows. Fog is not tied to the
  far plane: at the start fog runs from 7.13 m to 346.5 m, so geometry at the far plane keeps
  F = 1 − (254.478 − 7.128)/(346.5 − 7.128) = 0.271 of its own colour (73 % fog colour (80, 80, 82)/255)
  and is cut off against the unfogged sky.
- The far plane does not limit the sky: its depth is forced into [0.99999, 1] (§5.5), and the sun sprite
  lies at 0.75 × the far plane (§7.3).

## 9. `level.env_mod` and the start view

### 9.1 Format

Chunked (scoping §5.1); the 19-float record layout and field meanings are those of SS §9.2. New here: the
file is optional (no file: no modifiers); chunks are read by id 0, 1, 2, … in order until an id is missing
(a chunk with a later id after a gap is not read); every chunk is 76 bytes.

### 9.2 `levels\l01_escape\level.env_mod` (588 bytes, 7 chunks)

| id | position | radius | power | far | fog colour | fog dens. | ambient | sky colour | hemi |
|---|---|---|---|---|---|---|---|---|---|
| 0 | (−256.52, −22.73, −133.74) | 3 | 2 | 150 | 0.5020 ×3 | 1 | 0, 0, 0 | 1, 1, 1 | 0, 0.0157, 0.6431 |
| 1 | (−246.42, −23.53, −133.74) | 4 | 2 | 150 | 0.5020 ×3 | 1 | 0, 0, 0 | 1, 1, 1 | 0, 0, 0.0039 |
| 2 | (−76.58, 0, 162.54) | 20 | 1 | 300 | 0.5020 ×3 | 1 | 0.0078 ×3 | 0.4235 ×3 | 0, 0, 0.0588 |
| 3 | (−71.28, 0, 175.20) | 15 | 1 | 300 | 0.5020 ×3 | 1 | 0.0078 ×3 | 0.4235 ×3 | 0, 0, 0.0039 |
| 4 | (−78.48, 0, 139.52) | 15 | 1 | 300 | 0.5020 ×3 | 1 | 0.0078 ×3 | 0.4235 ×3 | 0, 0.0235, 0.9412 |
| 5 | (−251.92, −24.04, −134.14) | 4 | 2 | 300 | 0.1333, 0.1373, 0.1216 | 1 | 0.0784, 0.0863, 0.0941 | 0.5020, 0.5412, 0.5804 | 0.5020, 0.5412, 0.5804 |
| 6 | (−246.37, −24.04, −134.41) | 4 | 2 | 300 | 0.1333, 0.1373, 0.1216 | 1 | 0.0784, 0.0863, 0.0941 | 0.5020, 0.5412, 0.5804 | 0.5020, 0.5412, 0.5804 |

(Positions rounded to 0.01, colours to 0.0001, each correctly rounded from the stored f32 values.)

### 9.3 Effect at the start eye

Distances from the start eye: record 1 0.8002 m (weight 1.59990), record 6 0.8529 m (1.57357); every
other record is out of range. Their effect on the start view (values in §4): the far plane drops from 350
to 254.478 m (SP §7.1); the sky colour rises from 0.45 to about 0.68–0.71 (record 1's sky colour is 1); fog
density rises to 0.976 (fog starts at 7.13 m instead of 29.75 m); fog colour, ambient and hemi colour
change as listed in §4 (these feed SS §7, §9.3). Sun direction and colour, clouds and the sky textures are
not affected by modifiers.

## 10. Drawing order

Within SS §10.3, step 3 ("sky and clouds") is, in this order:

1. sky box (§5), depth range [0.99999, 1];
2. sun source sprite (§7.4, if visible), depth range [0, 1];
3. cloud dome (§6), depth range [0.99999, 1].

The flares and the gradient (§7.4) are drawn in SS §10.3 step 7 after the glows, flares first (in list
order), gradient last. Opaque world geometry is drawn before the sky, translucent geometry (SS §10.3 steps
4–6) after it, over the sky.

## 11. Other weather effects at the start

Rain is drawn only when the blended `rain_density` ≥ 0.001; at the start it is 0. Neither start section
names a thunderbolt. Neither effect contributes to the start view; their rendering is out of scope.

## 12. What the start view shows (summary)

Wherever the static world (clipped at 254.478 m, fogged toward (80, 80, 82)) does not cover the screen: the
blend (t = 0.498 in the byte) of `sky\sky_11_cube` and `sky\sky_2_cube`, mapped by §5.6, multiplied per
channel by 1.357, 1.388, 1.420 and clamped; near the zenith, faint cloud layers of `sky\sky_oblaka`
(colour (113, 84, 80)/255 times the sum of two texel layers, opacity 0.498 × y²⁵); no sun, flares or
gradient (sun behind the player; also still faded out on the first frames).

## 13. Verification facts

Weather data (`config\weathers\environment.ltx` and includes, dbb):

- 13 weather cycles in `[weathers]`, 2 weather effects in `[weather_effects]`; 123 hourly sections in all;
  `sect_default_weather` has 24.
- Every section has `sky_rotation = 0`; every `clouds_texture` is `sky\sky_oblaka`; 17 distinct
  `sky_texture` values; `flares` values are `flares_sun_rise`, `flares_default10`, `flares_gradient`,
  `flares_gradient1` or empty; clouds opacity ranges 0 … 1 with 31 sections below 0.001.
- All 17 sky cube maps exist in db7 as 786,560-byte DDS files: 512 × 512, DXT1, mip count field 0 (one
  level), all six cube faces (caps2 0xFE00); their `#small` companions are 24,704 bytes (32 × 32
  uncompressed, six faces) except `sky_yantar_cube#small` (3,200 bytes, DXT1).
- `textures\sky\sky_11_cube.dds` MD5 d8059a4c4932919c1420ca48f840f6f0; `sky_2_cube.dds` MD5
  e1eb8c54af2838141ed12da3de5e7b4f; `sky_oblaka.dds` 1,398,256 bytes, 1024 × 1024, DXT5, 11 levels, MD5
  6ec89de67a2c1998bfc12864fe3bb349.
- Flare textures `textures\fx\fx_sun_rise.dds`, `fx_flare1/2/3.dds`, `fx_gradient.dds` exist (dbb).

Environment modifiers:

- 15 shipped levels have `level.env_mod` (l01_escape, l02_garbage, l03_agroprom, l03u_agr_underground,
  l04u_labx18, l05_bar, l08_yantar, l08u_brainlab, l10u_bunker, l11_pripyat, l12_stancia_2,
  l12u_control_monolith, l12u_sarcofag, testers_mp_military_1, testers_mp_pool) with 38 records in all;
  every chunk is 76 bytes and ids run 0 … n−1. Sizes 588, 840, 84, 84, 84, 252, 84, 84, 84, 336, 168,
  168, 168, 84, 84 in that order.
- l01_escape: 7 records; exactly records 1 and 6 are within range of the start eye (§9.3).

Geometry (built-in, reproducible from §5.1 and §6.2):

- Sky box: 12 vertices, 20 triangles, all 20 facing the centre; Σ of all texture-coordinate y = −4.04
  (vertices 0–3: −1.01 each, 4–7: +1, 8–11: −1); Σ of all position y = 0.
- Cloud dome: 91 vertices, 160 triangles, 20 vertices on y = 0, Σ y over the 91 vertices = 40.1546, all
  vertices at radius 1, all triangles facing the centre.

Start values: §4 (C_sky bytes (173, 177, 181, 127), clouds bytes (113, 84, 80, 127), direction bytes
(37, 217, 176, 9), F · D_s = −0.96107).

## 14. Unestablished

- Use of the `#small` cube maps by the first renderer (not by any pass here).
- Occlusion of the sun by the world in detail (material transmission factors, object tests, caching);
  the time factor k_t in the flare fade is presumed to be `time_factor` = 10.
- The value range of intermediate pixel-program results on the original hardware (affects only the cloud
  sum s0 + s1 before the final clamp).
- Rain and thunderbolt rendering (not drawn at the start).
- What the screen shows where neither the world nor the sky draws (does not occur: the sky box encloses
  the eye).

## 15. Provenance

Date 2026-11-23. Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`, binaries
`bin\XR_3DA.exe`, `bin\xrRender_R1.dll`. Data examined: `config\weathers\*.ltx`, `config\alife.ltx`, the
`level.env_mod` files of all 15 levels that have one, the 80 `textures\sky\*cube*.dds` files,
`textures\sky\sky_oblaka.dds`, the `textures\fx` flare textures, `shaders.xr`, and the game's own shader
sources and scripts in `shaders\r1\` (read as data). Analysis: static analysis of the two binaries
(Ghidra, headless) to establish the keys read per weather section and their scaling, the blending of the
sky values, the sky and cloud geometry tables and transforms, vertex colours, pass states, depth range,
draw order, the lens-flare placement, colours and fading, and the reading of `level.env_mod`; constant
tables in the binaries were read as data. Independent throwaway tools written from this text decode the
weather sections and modifier files, regenerate the cloud dome and the sky box, and reproduce every number
of §4, §9.3 and §13. No execution or debugging; no leaked source code or derived projects were used.
Public references: Direct3D 9 render states, cube-map addressing and D3DCOLOR layout; DDS.
