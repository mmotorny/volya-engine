# Static shading (first renderer)

Behaviour-and-format specification of how the static world of the first single-player level
(`levels\l01_escape\`) is shaded by the game's first (default, static-lighting) renderer: the shader library
`shaders.xr` as far as these shaders need it, the effect of the scripted definitions `shaders\r1\*.s`, how
the level's shader table entries resolve to passes, textures, sampler and render states, and the colour
equation of every pass written as mathematics over named inputs. Where a fact could not be established it
is marked **unestablished**.

Conventions:

- Builds on the scoping specification (`docs/behavior/level-loading.md`, sections 3, 5.2–5.4, 5.12, 5.13,
  6), the archive specification and the LTX specification. Visual types and vertex decoding are specified in
  `ogf.md` and `level-geom.md`; this document refers to their vertex attributes by meaning (position, packed
  normal, base texture coordinate, light-map coordinate, vertex colour) and names the Direct3D 9 vertex
  element usage only to identify them.
- Numbers are little-endian. "u32", "i32", "f32" are 4-byte unsigned, signed, IEEE float. `zstring` is a
  zero-terminated Windows-1251 string. A "fixed string(n)" is n bytes holding a zstring padded with
  arbitrary bytes.
- Vectors are rows; `·` is the dot product; `sat(x)` = clamp(x, 0, 1) per component; `lerp(a,b,t)` =
  a + (b − a)t. Colours are RGB triples in [0, 1] unless stated. All arithmetic is on stored texel values
  directly: no sRGB decoding of textures and no sRGB encoding of the output (the renderer never enables
  either).
- Render and sampler state values are given by meaning; where useful the Direct3D 9 public enumeration value
  is given in brackets so that the facts can be checked against the public documentation.
- "Weather inputs" are values of the current blended environment (section 9); "camera inputs" come from the
  viewer.

## 1. Overview of the process

1. Each non-empty entry of `level` chunk 2 is `<engine shader>/<texture list>`. The engine shader name
   resolves to a **scripted definition** (a Lua module from `shaders\r1\*.s`) if one exists for it,
   otherwise to a **library definition** in `shaders.xr` (section 3). The texture list becomes the list
   T[0], T[1], … (section 4).
2. A resolved shader produces up to five **elements** (section 5). Element 0 and element 1 are the two
   variants of the normal opaque/translucent pass (near and far), elements 2 and 3 are additive dynamic
   point/spot light passes, element 4 is a special pass (for the library classes of section 7 except
   `D_TREE  ` and `S_SET   `, and for `selflight`, it draws into the projected light texture of objects,
   `docs/behavior/object-projected-light.md` section 4.4). Each element here has exactly one pass.
3. A pass = vertex program + pixel program (HLSL files in `shaders\r1\`), up to four textures bound to
   named samplers with sampler states, and render states (depth, blend, alpha test, fog). The program
   files define the colour equation (sections 7, 8).
4. Per frame the renderer picks, per visible static visual, which element to draw (section 6), groups them
   into draw phases and orders them (section 10).

## 2. Inputs read

| File | Use |
|---|---|
| `shaders.xr` (dba) | library definitions (section 3) |
| `shaders\r1\.s` and `shaders\r1\<module>.s` | scripted definitions (section 3.4) |
| `shaders\r1\<program>.vs`, `.ps`, `common.h`, `shaders\shared\common.h` | program sources (HLSL; read as data to establish the equations below) |
| `textures\textures.ltx` | `[association]`: detail texture and scale per base texture (section 4.3) |
| texture files | section 4.2 |
| `config\system.ltx` and `user.ltx` console values | renderer settings (section 11) |
| weather sections and `level.env_mod` | weather inputs (section 9) |

## 3. `shaders.xr`

### 3.1 Container

Chunked (scoping 5.1). Top-level chunks, in file order:

| id | Size (bytes) | Content |
|---|---|---|
| 0 | 0 | empty |
| 1 | 3,277 | texture-coordinate transform records (section 3.3) |
| 2 | 155,525 | 183 sub-chunks (ids 0 … 182), one definition each (section 3.2) |
| 3 | 3,531 | u32 count = 183, then 183 zstrings: the definition names, sorted |

### 3.2 Definition record (chunk 2 sub-chunk)

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | class tag, stored byte-reversed (stored `      ML` = tag `LM      `) |
| 8 | 128 | fixed string(128): definition name (the engine shader name, e.g. `def_shaders\def_vertex`) |
| 136 | 32 | fixed string(32): author machine name (ignored) |
| 168 | 4 | u32 time stamp (ignored) |
| 172 | 2 | u16 class version |
| 174 | 2 | padding (ignored) |
| 176 | … | property stream until the end of the sub-chunk |

Property stream: a sequence of properties, each `u32 type`, `zstring name`, then a value whose layout
depends on the type:

| Type | Meaning | Value |
|---|---|---|
| 0 | marker (group heading) | none |
| 1 | transform reference | fixed string(64): name of a chunk-1 record or `$null` |
| 2 | constant reference | fixed string(64) |
| 3 | texture reference | fixed string(64): texture name or `$base0` … `$base7` / `$null` (section 4.1) |
| 4 | integer | i32 value, i32 minimum, i32 maximum (12 bytes) |
| 6 | boolean | u32 (0 = false, otherwise true) |
| 7 | token choice | u32 value, u32 count n, then n × (u32 token id, fixed string(64) token name) |

Type 5 does not occur. Counts in the whole library: type 0: 451, 1: 268, 2: 2, 3: 544, 4: 269, 6: 497,
7: 56. Parsing every sub-chunk with these rules consumes it exactly.

Every definition starts with the same five properties (the **common block**):

| # | Type | Name | Use |
|---|---|---|---|
| 1 | 0 | `General` | — |
| 2 | 4 | `Priority` | draw priority p ∈ 0…3 (value field) |
| 3 | 6 | `Strict sorting` | if true: drawn sorted back to front (section 10) |
| 4 | 0 | `Base Texture` | — |
| 5 | 3 | `Name` | base texture reference (always `$base0` in this library for the classes below) |
| 6 | 1 | `Transform` | base texture transform (`$null`; unused by the first renderer's programmable passes) |

Class-specific properties follow, in this order:

| Tag | Version(s) | Properties after the common block |
|---|---|---|
| `LM      ` | 0 | none |
| `V       ` | 0 | none |
| `LM_AREF ` | 0 | integer `Alpha ref` (alpha-blend = false) |
|  | ≥ 1 | integer `Alpha ref`, boolean `Alpha-blend` |
| `V_AREF  ` | as `LM_AREF ` | |
| `D_TREE  ` | 0 | boolean `Alpha-blend` (object-LOD = false) |
|  | ≥ 1 | boolean `Alpha-blend`, boolean `Object LOD` |
| `BmmDold ` | < 3 | marker `Detail map`, texture `Name` (detail texture), transform `Transform` |
|  | ≥ 3 | as above, then textures `R2-R`, `R2-G`, `R2-B`, `R2-A` (second renderer only; ignored) |
| `S_SET   ` | 4 | token `Blending` (10 tokens: 0 `SET`, 1 `BLEND`, 2 `ADD`, 3 `MUL`, 4 `MUL_2X`, 5 `ALPHA-ADD`, 6 `MUL_2X (B^D)`, 7 `SET (2r)`, 8 `BLEND (2r)`, 9 `BLEND (4r)`), boolean `Texture clamp`, integer `Alpha ref`, booleans `Z-test`, `Z-write`, `Lighting`, `Fog` |

(`S_SET   ` versions 2 and 3 differ only in property order; none occur in this library: all 51 are version
4.) Class versions in the library: `BmmDold ` 3 (69), `D_TREE  ` 1 (3), `LM      ` 0 (1), `LM_AREF ` 1 (2),
`V       ` 0 (3), `V_AREF  ` 1 (5), `S_SET   ` 4 (51). Other tags (`MODEL   `, `MODELEbB`, `LmEbB   `,
`PARTICLE`, `D_STILL `, `E_SEL   `, `E_WIRE  `, `SH_WORLD`) are not used by the first level's static world.

Values for the first level's library shaders:

| Definition | Tag | Priority | Strict | Class values |
|---|---|---|---|---|
| `default` | `LM      ` | 1 | no | — |
| `def_shaders\def_vertex` | `V       ` | 1 | no | — |
| `def_shaders\def_aref` | `LM_AREF ` | 1 | no | alpha ref 33, alpha-blend no |
| `def_shaders\def_trans` | `LM_AREF ` | 2 | yes | alpha ref 0, alpha-blend yes |
| `def_shaders\def_aref_v` | `V_AREF  ` | 1 | no | alpha ref 33, alpha-blend no |
| `def_shaders\def_trans_v` | `V_AREF  ` | 2 | yes | alpha ref 0, alpha-blend yes |
| `flora\leaf_wave` | `D_TREE  ` | 1 | no | alpha-blend yes, object LOD no |
| `flora\trunk_wave` | `D_TREE  ` | 1 | no | alpha-blend no, object LOD no |
| `def_shaders\def_objects_lod` | `D_TREE  ` | 1 | no | alpha-blend no, object LOD yes |
| `levels\escape_grass` | `BmmDold ` | 1 | no | detail `detail\detail_grnd_grass`, transform `0001` |
| `levels\escape_asfalt`, `…_asfalt_noblend` | `BmmDold ` | 1 | no | detail `detail\detail_grnd_asphalt`, transform `0001` |
| `levels\escape_sand` | `BmmDold ` | 1 | no | detail `detail\detail_grnd_earth`, transform `0001` |
| `effects\glow` | `S_SET   ` | 1 | no | blending 5 (`ALPHA-ADD`), clamp yes, alpha ref 0, Z-test yes, Z-write no, lighting no, fog no |

### 3.3 Transform records (chunk 1)

A sequence (no sub-chunks; the chunk scanner of scoping 5.1 must not be applied) of records: `zstring name`
(e.g. `0000`, `0001`), then 108 bytes: u32 (1 or 3), u32 flags, then five waves of 20 bytes each
(u32 function, f32 × 4 arguments), for scale-U, scale-V, rotation, scroll-U, scroll-V. 29 records; they
consume the chunk exactly. Record `0001` has flags 1 and constant waves scale-U = scale-V = 144.

The first renderer's programmable passes never use these transforms (the detail scale comes from
`textures.ltx`, section 4.3); they are listed so that a parser can consume the chunk. The full semantics of
flags and wave functions are **unestablished**.

### 3.4 Scripted definitions (`shaders\r1\*.s`)

At start-up every file `shaders\r1\*.s` is loaded as a Lua module named after the file name without `.s`;
the file named exactly `.s` is loaded into the global scope (it defines helper functions shared by
modules). An engine shader name selects a module by replacing every `\` with `_` (`effects\wallmarkblend` →
module `effects_wallmarkblend`). The module **defines** the shader if it exists and provides a function
`normal` or a function `l_special`; then the library definition is not consulted. Otherwise the library
definition of that exact name is used; a name present in neither is fatal ("not found in library").

Element functions (each called with `(shader, t_base, t_second, t_detail)`; t_base = T[0], t_second = T[1],
t_detail = the associated detail texture name of T[0] (section 4.3); a missing value is the string `null`,
which binds no texture):

| Element | Function used |
|---|---|
| 0 | `normal_hq` if the module has it and T[0] has a detail association, else `normal` |
| 1 | `normal` |
| 2 | `l_point` |
| 3 | `l_spot` |
| 4 | `l_special` |

An absent function leaves the element empty. None of the first level's modules defines `normal_hq`.

Effect of the script vocabulary:

| Call | Effect on the pass |
|---|---|
| `shader:begin(vs, ps)` | starts the pass with programs `vs`, `ps` and defaults: fog on, depth test on (less-or-equal), depth write on, blending off, alpha test off |
| `:fog(b)` | fog on/off |
| `:zb(test, write)` | depth test (less-or-equal) or always-pass; depth write |
| `:blend(on, src, dst)` | blending with factors; `blend(true, one, zero)` counts as off |
| `:aref(on, ref)` | alpha test on/off with reference ref (clamped to 255); test passes when alpha·255 > ref |
| `:sorting(p, strict)` | priority p, strict-sorting flag (defaults: p = 1, strict = false) |
| `shader:sampler(name)` | binds the sampler named `name` in the programs, default states: wrap addressing, linear minification, linear mip, linear magnification — except that for samplers named `s_base` and `s_detail` linear min/mag become **anisotropic** |
| `:texture(t)` | binds texture t |
| `:clamp()`, `:wrap()`, `:mirror()` | addressing U, V, W |
| `:f_anisotropic()` | min aniso, mip linear, mag aniso |
| `:f_trilinear()` | linear, linear, linear |
| `:f_bilinear()` | linear, mip point, linear |
| `:f_linear()` | linear, no mip, linear |
| `:f_none()` | point, no mip, point |
| `:project(b)` | projective texture coordinates |
| `blend.<name>` | factors `zero` 1, `one` 2, `srccolor` 3, `invsrccolor` 4, `srcalpha` 5, `invsrcalpha` 6, `destalpha` 7, `invdestalpha` 8, `destcolor` 9, `invdestcolor` 10, `srcalphasat` 11 |

The global helpers in `.s` build the point and spot light passes (programs `<vs>`/`add_point` or
`add_spot`, fog off, depth test on, depth write off, additive blend one/one, alpha test with ref given or
0, samplers `s_base` = base, `s_lmap` and `s_att` = attenuation textures
`internal\internal_light_attpoint` (point) or `internal\internal_light_att` (projective) and
`internal\internal_light_attclip` (spot), clamp addressing, linear filtering without mip).

## 4. Textures

### 4.1 Texture list and names

The part of a chunk-2 entry after the first `/` is split at every `,` (no trimming). Each name is
lower-cased and, if it ends in `.tga`, `.dds`, `.bmp` or `.ogm` (case-insensitive), that extension is
removed. A texture reference `$base<k>` in a library definition selects T[k] (k = 0 … 7); `$null` and the
empty string mean "no texture"; any other reference names a texture directly. Library-definition samplers
take the texture positions given in section 7 regardless of the reference.

### 4.2 Locating texture files

A name n is looked up as `$level$n.dds`, then `$game_saves$n.dds`, then `$game_textures$n.dds`; not found
is fatal (`Can't find texture`). (Names containing `_bump` have a separate rule; none are used here.) For
`l01_escape` all names resolve; the level directory supplies `lmap#1_1` … `lmap#5_2`,
`terrain\terrain_escape`, `terrain\terrain_escape_lm`, `level_lods`, `level_lods_nm`.

Files are loaded with the format and mip-level count stored in the DDS file (block-compressed formats stay
compressed). With the default `texture_lod 0` no mip levels are dropped. The light maps (`lmap#*`) have one
level, so they are sampled without mipmaps.

### 4.3 Detail association (`textures\textures.ltx`)

Section `[association]` (LTX specification) maps a base texture name to a value parsed as
`<detail name>,<scale>` (text up to the first comma, then a float) and a usage flag found by substring:
`usage[diffuse_or_bump]` → 3, else `usage[bump]` → 2, else `usage[diffuse]` → 1, else 0. The table has 970
keys. A texture "has a diffuse detail" when it has an association with usage bit 0 set (1 or 3).

Detail lookup uses T[0] (the texture that the definition's base reference selects). When the shader class
can take detail (all classes in section 7 except `S_SET   `) and T[0] has an association:

- detail texture D = the association's detail name (except for `BmmDold `, which binds its own detail
  texture, section 7.4);
- the constant **dt_params** = (s, s, s, 0.02), s = the association's scale;
- the flag **use_detail** = usage bit 0.

Without an association use_detail is false and dt_params is not provided.

Verification (l01, 395 non-empty entries): 213 `default`, 33 `def_objects_lod`, 32 `def_vertex`,
3 `def_aref`, 1 `def_trans`, 1 `def_trans_v`, 5 `trunk_wave` entries and all 4 terrain entries have a
diffuse detail; across all entries with a diffuse detail there are 28 distinct detail textures; their scales
occur as 1 (×1), 2 (×19), 3 (×52), 4 (×44), 6 (×72), 8 (×96), 12 (×4), 144 (×4, the terrain entries,
association of `terrain\terrain_escape`).

### 4.4 Sampler states

Library-definition samplers use: base-type samplers (`s_base`, `s_lmap`, `s_detail`) wrap addressing,
linear min/mip/mag, with the `s_base`/`s_detail` rule turning min and mag into anisotropic; `s_hemi` clamp
addressing, linear min, **no mip**, linear mag. On every sampler the maximum anisotropy is
clamp(`r__tf_aniso`, 1, 16) (default 16) and the mip LOD bias is `r1_tf_mipbias` (default 0).

## 5. Elements and the near/far choice

| Element | Role |
|---|---|
| 0 | normal pass, near variant (may use detail) |
| 1 | normal pass, far variant (never detail) |
| 2, 3 | additive dynamic point / spot light pass (section 12) |
| 4 | special pass: the LOD billboards' opaque cut-out pass (section 8.4); for `LM      `, `LM_AREF `, `V       `, `V_AREF  `, `BmmDold ` and `selflight` the pass that draws the static geometry's baked light into the projected light texture of objects (`object-projected-light.md` 4.4); `D_TREE  ` and `S_SET   ` have none |

For a static mesh visual with bounding-sphere centre C and radius r, and camera position E, the normal
phase draws element 0 if |E − C| − r < 44 (metres) and element 1 otherwise.

## 6. Shared constants (named inputs)

| Name | Value |
|---|---|
| VP | the view-projection matrix (static geometry is in world coordinates) |
| E | camera position |
| L_ambient | (ambient_r, ambient_g, ambient_b, t) — weather ambient; w = section blend factor t (unused here) |
| L_hemi_color | weather hemi_color (rgb; w unused here) |
| L_sun_color | weather sun_color |
| L_sun_dir | weather sun direction (unit vector, the direction the light travels) |
| fog_plane | section 9.3 |
| fog_color | weather fog_color (used through the fixed fog unit, section 9.3) |
| dt_params | section 4.3 |
| time | global time in seconds (trees) |

Helper terms (N = unpacked vertex normal, as specified by the geometry specification: 2·c − 1 of the
colour-packed bytes):

- hemi term H = L_hemi_color.rgb (it does **not** depend on N);
- sun term S(N) = L_sun_color · max(0, N · (−L_sun_dir));
- detail fade, from the world position P: δ = min((|P − E| · dt_params.w)², 1); dt_mul = 1 − δ,
  dt_add = δ / 2;
- fog factor F = P · fog_plane (P with w = 1).

Precision and clamping rules that are part of the behaviour (Direct3D 9 shader models used: vertex
programs `vs_2_0` on capable hardware, pixel programs `ps_1_1`):

- Every vertex-program colour output (the values called c0, c1 below) is clamped to [0, 1] per component
  before interpolation; texture-coordinate outputs are not.
- Texel values are in [0, 1]; the pixel result is clamped to [0, 1] when written.
- Pixel-program constants (L_ambient in the pixel programs) are clamped to [−1, 1] (no effect for the
  shipped weather values).

## 7. Library-defined shaders: passes and colour equations

Notation per pass: B = texel of the base sampler at the base texture coordinate u_b (decoded per the
geometry specification), Lm = light-map texel (rgb = baked light, a = sun visibility) at the light-map
coordinate u_l, Hm = hemi texel, Dt = detail texel. Final colour C, alpha A; then fog (9.3), alpha test,
blending.

### 7.1 `LM      ` (`default`) and `LM_AREF ` (`def_aref`, `def_trans`)

Texture list: T[0] base, T[1] light map (`lmap#N_1`), T[2] hemi map (`lmap#N_2`); fewer than 3 textures is
fatal ("Not enought textures for shader"). Vertex layout: two texture coordinates (layout C).

| | Element 0 | Element 1 |
|---|---|---|
| programs | `lmap_dt`/`lmap_dt` if use_detail, else `lmap`/`lmap` | `lmap`/`lmap` |
| samplers | `s_base` = T[0], `s_lmap` = T[1], `s_hemi` = T[2], `s_detail` = D (detail variant) | `s_base`, `s_lmap`, `s_hemi` |

States (both elements): fog on; depth test on, depth write on.
- `LM      `: blending off, alpha test off.
- `LM_AREF `: alpha test on, ref = `Alpha ref`; if `Alpha-blend`: blending source-alpha / inverse-source-alpha
  [5/6], else off (factors one/zero).

Vertex stage: c0 = sat(H), c1 = sat(S(N)); detail variant also c0.a = dt_mul, c1.a = dt_add and detail
coordinate u_d = u_b · dt_params.xy.

Pixel stage, with h = (Hm.r + Hm.g + Hm.b)/3 (Hm sampled at u_l):

  light = L_ambient.rgb + Lm.rgb + c1 · Lm.a + c0 · h
  C = 2 · light · B.rgb
  detail variant: C = 2 · (2 · light · B.rgb) · (Dt.rgb · c0.a + c1.a)
  A = B.a

### 7.2 `V       ` (`def_vertex`) and `V_AREF  ` (`def_aref_v`, `def_trans_v`)

Texture list: T[0] base. Vertex layout: vertex colour (layout B); vertex colour (r, g, b, a) =
(baked rgb light, sun visibility), normal w = hemi visibility (geometry specification).

| | Element 0 | Element 1 |
|---|---|---|
| programs | `vert_dt`/`vert_dt` if use_detail, else `vert`/`vert` | `vert`/`vert` |
| samplers | `s_base` = T[0] (+ `s_detail` = D) | `s_base` |

States as 7.1 (`V       ` like `LM      `, `V_AREF  ` like `LM_AREF `).

Vertex stage: c0.rgb = sat(color.rgb + H · N.w + S(N) · color.a + L_ambient.rgb); detail variant
c0.a = dt_mul, c1 = dt_add (all four components), u_d = u_b · dt_params.xy.

Pixel stage: C = 2 · B.rgb · c0.rgb; detail: C = 2 · (2 · B.rgb · c0.rgb) · (Dt.rgb · c0.a + c1.a);
A = B.a.

### 7.3 `D_TREE  ` (`flora\leaf_wave`, `flora\trunk_wave`, `def_shaders\def_objects_lod`)

Texture list: T[0] base. Vertex layout A (four-component texture coordinate (u, v, frac, unused) and normal
with w = hemi/base light, geometry specification). Each tree visual's chunk 12 (104 bytes) supplies, as
read by the renderer: a 4 × 4 f32 matrix X (64 bytes, model to world, row-vector convention; X's
translation row is element (4, 1…3)), then 5 f32 **scale** (σ_r, σ_g, σ_b, σ_h, σ_s), then 5 f32 **bias**
(β_r, β_g, β_b, β_h, β_s); every scale and bias value is multiplied by 0.5 at load.

Per-draw constants (K = 1.5):

- c_scale = K·(σ_r, σ_g, σ_b, σ_h); c_bias = (K·β_r + ambient_r, K·β_g + ambient_g, K·β_b + ambient_b,
  K·β_h); c_sun = (K·σ_s, K·β_s);
- consts = (1/2048, 1/2048, 0, 0);
- wave = (0.1, 0.01, 0.11, time) / (2π);
- wind = 0.005 · normalize(sin(0.6283·time), 0, cos(0.6283·time)), w = 0.

| | Element 0 | Element 1 |
|---|---|---|
| programs (object LOD = no) | `tree_w_dt`/`vert_dt` if use_detail, else `tree_w`/`vert` | `tree_s`/`vert` |
| programs (object LOD = yes) | `tree_s_dt`/`vert_dt` if use_detail, else `tree_s`/`vert` | `tree_s`/`vert` |
| samplers | `s_base` = T[0] (+ `s_detail` = D) | `s_base` |

States (both): fog on; depth test and write on; alpha test on with ref 200 (object LOD = no) or 0 (object
LOD = yes); blending source-alpha / inverse-source-alpha if `Alpha-blend`, else off.

Vertex stage, with p = (position, 1)·X (world position), y₀ = X's translation y, m = texcoord:
- frac = m.z · consts.x; H_v = p.y − y₀; φ = wave.w + p·wave.xyz; c(φ) = (2√2·fract(φ) − √2)² − 1;
- programs with `_w` (wave): p.x += wind.x · H_v · c(φ) · frac, p.z += wind.z · H_v · c(φ) · frac;
  programs with `_s` (still): p unchanged;
- N_w = unpacked normal transformed by X's rotation (not renormalised);
- l = normal.w; U = c_scale · l + c_bias;
- c0.rgb = sat(U.rgb + L_hemi_color.rgb · U.w + L_sun_color · (0.25 + 0.75 · N_w·(−L_sun_dir)) ·
  (l · c_sun.x + c_sun.y));
- base coordinate = (m.x, m.y) · consts.xy; detail variant: u_d = base coordinate · dt_params.xy,
  c0.a = dt_mul, c1 = dt_add; fog from the moved position p.

Pixel stage: `vert`/`vert_dt` of 7.2.

### 7.4 `BmmDold ` (terrain: `levels\escape_*`)

Texture list: T[0] terrain base (`terrain\terrain_escape`; its alpha = hemi visibility), T[1] terrain light
map (`terrain\terrain_escape_lm`; rgb = baked light, a = sun visibility); fewer than 2 is fatal. Layout C.

Elements 0 and 1: programs `impl_dt`/`impl_dt`; samplers `s_base` = T[0], `s_lmap` = T[1], `s_detail` =
the definition's own `Detail map` texture (e.g. `detail\detail_grnd_grass`); dt_params from the association
of T[0] (`terrain\terrain_escape` → scale 144). States: fog on, depth test/write on, blending off, alpha
test off.

Vertex stage: c0 = sat(H), c1 = sat(S(N)); the light map is sampled at the **base** coordinate u_b;
u_d = u_b · dt_params.xy.

Pixel stage:
  light = L_ambient.rgb + Lm.rgb + c1 · Lm.a + c0 · B.a
  C = 2 · (2 · light · B.rgb) · Dt.rgb,  A = 1
(no distance fade of the detail texture for terrain).

### 7.5 `S_SET   ` (`effects\glow`)

Fixed-function single pass (no programs): colour = texel × vertex colour, alpha = texel alpha × vertex
alpha; addressing clamp if `Texture clamp`, else wrap; linear min/mip/mag. Blending per token: 0 off; 1, 8
source-alpha/inverse-source-alpha with alpha test (ref); 2 one/one; 3 zero/source-colour; 4, 6
destination-colour/source-colour; 5 source-alpha/one with alpha test (ref); 7 off with alpha test ref 0;
9 source-alpha/inverse-source-alpha with alpha test. Depth test, depth write, fog per properties. For
`effects\glow`: source-alpha / one, alpha test ref 0, clamp, depth test on, write off, fog off. The glow
sprites' geometry, size and fading (level chunk 7) are **unestablished**.

## 8. Script-defined shaders of the first level

### 8.1 `selflight` (entries with T[0] only; layout B)

Elements 0, 1 (`normal`): programs `simple`/`simple`, fog on, depth test/write on, no blend, no alpha test;
sampler `s_base` = T[0] (wrap, anisotropic). C = B.rgb, A = B.a (no lighting).
Element 4 (`l_special`): same without fog. Elements 2, 3: light passes (section 3.4).

### 8.2 `effects\lightplanes` (layout B)

Elements 0, 1: programs `base_lplanes`/`base_lplanes`; fog off; depth test on, write off; blending
source-alpha / one; alpha test ref 0; priority 2, not strict. Sampler `s_base` = T[0].
Vertex: projected with VP (identity world); fade φ = |v̂ · n̂| with v̂ the normalised view-space position and
n̂ the normalised view-space normal. Pixel: C = B.rgb, A = B.a · φ.

### 8.3 `effects\wallmarkblend` (layout B)

Elements 0, 1: programs `wmark`/`vert`; priority 2, not strict; blending source-alpha / inverse-source-alpha;
alpha test ref 0; depth test on, write off; fog on. Sampler `s_base` = T[0].
Vertex: position pushed off the surface: P' = P + 0.007·N, then
P' −= normalize(D_e + normalize(P' − E)) · s with D_e the camera's forward direction and
s = lerp(0.003, 0.011, |E − P|) (the lerp factor is the distance itself, not clamped); lighting c0 as in
7.2 (no detail); fog from P'. Pixel: `vert` (C = 2 · B.rgb · c0, A = B.a).

### 8.4 `details\lod` (distant billboards of LOD group visuals, type 6; texture `level_lods`)

Samplers (script defaults, i.e. wrap, linear/linear/linear): `s_base0`, `s_base1` = T[0] (`level_lods`),
`s_hemi0`, `s_hemi1` = T[0] + `_nm` (`level_lods_nm`).
- Element 4 (`l_special`): programs `lod`/`lod`; blending off; alpha test ref 200; depth test and write on;
  fog on.
- Elements 0, 1 (`normal`): programs `lod`/`lod`; blending source-alpha / inverse-source-alpha; alpha test
  ref 8; depth test on, write off; fog on.

Billboard data (chunk 11 of a type-6 visual, 896 bytes) = 8 facets × 4 vertices × 28 bytes: f32 × 3
position, f32 × 2 texture coordinate, u32 colour read as bytes (B, G, R, A) = baked rgb light and hemi (A),
u8 sun, 3 unused bytes. At load each facet gets a normal n_f = −normalize(¼ Σ of the four unit corner
normals), corner normal k = normalize((v_{k+1} − v_k) × (v_{k+2} − v_{k+1})) (indices mod 4; a corner
normal of squared length ≤ 1.2·10⁻³⁸ is left unnormalised).

Per frame, for each LOD visual drawn (selection in section 10.2) with sphere (C, r):
- d̂ = normalize(C − E); sort the facets by d̂ · n_f ascending; facet b = largest, facet a = second largest,
  facet c = third; blend factor
  w = trunc(clamp((1 − (q_a − q_c)/(q_b − q_c) + 1) · 0.5 · 255.5, 0, 255)), q = d̂ · n_f;
- fade alpha α = trunc(clamp((1 − (ssa − ssa_B)/max(ssa_A − ssa_B, 10⁻⁷)) · 255, 0, 255)) (ssa terms in
  section 10.2);
- offset o = −0.5 · r · d̂;
- emit 4 vertices taking facet-vertex indices 3, 0, 2, 1 in that order; vertex k: position0 = a.v[k].pos + o,
  position1 = b.v[k].pos + o, normal0 = n_a, normal1 = n_b, colour = bytes (A = w, R = a.v[k].sun,
  G = b.v[k].sun, B = α), texcoord0 = a.v[k].uv, texcoord1 = b.v[k].uv, texcoord2 = a.v[k] colour,
  texcoord3 = b.v[k] colour; drawn as two triangles (0, 1, 2), (3, 2, 1) of the emitted four.

Vertex stage (colour inputs as Direct3D colour: x = R, y = G, z = B, w = A, each /255):
f = colour.w; P = lerp(position0, position1, f); n = normalize(lerp(normal0, normal1, f)); n.y += 1;
n = normalize(n); g = lerp(tc2, tc3, f) · 1.55; σ = lerp(colour.x, colour.y, f) · 1.55;
c.rgb = sat(g.rgb + L_hemi_color.rgb · g.w + L_sun_color · σ · (1 + 0.7 · n · L_sun_dir) + L_ambient.rgb);
c.a = colour.z; fog from P.
Pixel stage: base = lerp(B0, B1, f) · c (rgba), h = lerp(H0, H1, f).a,
C = 2 · base.rgb · (0.5 + 0.5·h), A = base.a.

## 9. Weather inputs

### 9.1 Section values

From each hourly weather section (LTX): `ambient` (3 floats), `hemi_color` (4), `sun_color` (3),
`sun_dir` (2: altitude α°, longitude λ°), `fog_color` (3), `fog_distance`, `fog_density`, `far_plane`.
Direction: L_sun_dir = (−sin λ·cos α, sin α, cos λ·cos α). (`lmap_color` is not read.)

### 9.2 Blending

With game time τ (seconds of the day) between the bracketing sections A (time τ_A) and B (τ_B), the weight
t = clamp((τ − τ_A)/(τ_B − τ_A), 0, 1) (times wrap at 86,400 s). Environment modifiers (`level.env_mod`, 19
floats each: position (3), radius ρ, power π_m, far_plane, fog_color (3), fog_density, ambient (3),
sky_color (3), hemi_color (3)) contribute, for camera distance δ < ρ, weight ω = (1 − δ/ρ)·π_m; M = Σ ω·field,
W = Σ ω (zero when none in range). Then:

- ambient = (lerp(A, B, t) + M.ambient)/(1 + W); hemi_color.rgb likewise (w = lerp only);
  fog_color likewise; fog_density = (lerp + M.fog_density)/(1 + W);
- sun_color = lerp; L_sun_dir = normalize(lerp(dir_A, dir_B, t)); fog_distance = lerp;
- fog_near = (1 − fog_density) · fog_distance · 0.85; fog_far = 0.99 · fog_distance;
- far_plane = (lerp + M.far_plane) · `rs_vis_distance` / (1 + W).

### 9.3 Fog

fog_plane is derived each frame from the view-projection matrix VP in row-vector convention (clip = (P,1)·VP):
let q = third column + fourth column of VP (a 4-vector), ℓ = |q.xyz|; then
fog_plane = (−q.xyz / (ℓ (fog_far − fog_near)), 1 − (q.w/ℓ − fog_near)/(fog_far − fog_near)),
i.e. F = 1 − (d − fog_near)/(fog_far − fog_near) with d the signed distance of P from the plane
q·(P,1) = 0 (a plane parallel to the image plane near the camera). F is computed per vertex, clamped to
[0, 1], interpolated, and applied after the pixel stage by the fixed fog unit of passes with fog on:
C ← F·C + (1 − F)·fog_rgb, alpha unchanged, where fog_rgb = trunc(clamp(255·fog_color, 0, 255))/255.
(Table/pixel fog is never enabled.)

### 9.4 Values for the first level at 05:30 (no modifier in range)

Sections `default_weather_05`/`_06`, t = 0.5: L_ambient.rgb = (0.0775, 0.0800, 0.0825), L_hemi_color =
(0.775, 0.800, 0.825, 1.0), L_sun_color = (0.759, 0.380, 0.102), L_sun_dir ≈ (0.9132, −0.2079, 0.3505),
fog_color = (0.2965, 0.304, 0.357) → bytes (75, 77, 91), fog_near = 29.75, fog_far = 346.5, far_plane = 350.

## 10. Drawing order and selection

### 10.1 Fixed states

Unless a pass says otherwise: depth function less-or-equal; back-face culling of triangles that appear
counter-clockwise on screen [cull mode 3]; alpha-test comparison "greater" (passes when alpha·255 > ref);
fixed lighting off; dithering irrelevant. LOD billboards (8.4) are drawn with culling disabled.

### 10.2 Selection

For a static mesh with sphere (C, r), the projected-size measure ssa = r/(|E − C|² + 10⁻⁵). With the camera
vertical field of view φ (degrees) and the render-target size w × h pixels, k = 1/((g + 10⁻⁷)(90/φ)² w h),
g = `r__geometry_lod` (default 1). Thresholds: ssa_min = 12.25·k, ssa_B = (r1_ssa_lod_b/3)²·k,
ssa_A = (r1_ssa_lod_a/3)²·k (defaults 48 and 64).
- A mesh with ssa ≤ ssa_min is not drawn.
- A LOD group (type 6) uses ssa = r/(|E − C|² + 10⁻⁵) · κ with κ = 2(r² asin(m/r) + m·√(r² − m²))/(π r²),
  m = the median of the half-extents of its bounding box. If ssa < ssa_A and ssa ≥ ssa_min its billboard is
  drawn (fade α of 8.4); if ssa > ssa_B its children are drawn; between ssa_B and ssa_A both.

The render target is the screen size scaled by √`r__supersample` per axis (default 2), clamped to 128…2048
and made even; how it is presented (post-process pass) is **unestablished**.

### 10.3 Order per frame

1. Non-sorted passes of priority 0–1 (grouped by state; the order inside the group is irrelevant to the
   result except for blended ones, drawn roughly near to far by ssa).
2. LOD billboards, element 4 (opaque cut-out pass).
3. Sky and clouds (weather specification).
4. LOD billboards, element 1 (blended pass).
5. Non-sorted passes of priority 2–3 (e.g. light planes, wall marks).
6. Strict-sorted passes (e.g. `def_trans`, `def_trans_v`) in order of decreasing |E − C|² (back to front).
7. Glows, then lens flares.

## 11. Renderer settings involved (defaults from the shipped `user.ltx`)

`r__tf_aniso 16`, `r1_tf_mipbias 0`, `texture_lod 0`, `r__geometry_lod 1`, `r1_ssa_lod_a 64`,
`r1_ssa_lod_b 48`, `r__supersample 2`, `rs_vis_distance 1`, `rs_c_gamma/brightness/contrast 1` (the
resulting gamma ramp is the identity within 1/65535), `r1_dlights on`, `r1_dlights_clip 75`.

## 12. Dynamic light passes (elements 2, 3)

The level's 40 point lights (chunk 6) become active dynamic lights when `r1_dlights` is on; they may be
drawn onto static geometry with elements 2 (point: programs `lmap_point`/`impl_point`/`vert_point`/
`tree_*_point` with `add_point`) and 3 (spot). The point pass: additive one/one, depth test on, write off,
fog off; pixel C = 2 · B.rgb · B.a · a₁ · a₂ · c.rgb (A = 2·B.a·a₁·a₂·c.a), a₁, a₂ = attenuation texels at
((P − L_pos)·L_pos.w + 0.5).xz and .xy, c = light colour · N·(−normalize(P − L_pos)) · sat(F).
Which lights are applied to which geometry, and the attenuation textures' content, are **unestablished**; a
first static render may omit these passes.

## 13. Verification facts (first level and all shipped levels)

- `shaders.xr`: 183 definitions, 183 names in chunk 3, 29 transform records; property-type counts as in
  3.2.
- `l01_escape` chunk 2: 396 entries (entry 0 empty), 18 distinct engine shaders, 4 of them scripted
  (`selflight`, `effects\lightplanes`, `effects\wallmarkblend`, `details\lod`).
- Element-0 program pairs over the 395 non-empty l01 entries: `lmap_dt` 217, `lmap` 42, `vert_dt` 33,
  `vert` 29, `tree_s_dt`/`vert_dt` 33, `tree_s`/`vert` 4, `tree_w_dt`/`vert_dt` 5, `tree_w`/`vert` 9,
  `impl_dt` 4, `wmark`/`vert` 11, `simple` 3, `base_lplanes` 1, `lod` 1, fixed-function (glow) 3.
- Visual-to-shader consistency (l01 chunk 3): `LM`/`LM_AREF`/`BmmDold` visuals use vertex layout C, `V`/
  `V_AREF` and scripted mesh shaders layout B, `D_TREE` layout A; all 1,839 type-6 visuals use
  `details\lod`.
- All 30 shipped levels (18 single-player, 12 `testers_mp_*`): 8,315 non-empty chunk-2 entries, every
  engine shader name resolves (scripted or library); other levels additionally use `effects\water`,
  `effects\waterryaska`, `effects\waterstuden`, `effects\flare` and `def_shaders\def_refl_alpha_heli`
  (`LmEbB   `), outside this specification.
- Weather inputs at the l01 start: section 9.4.

## 14. Unestablished (summary)

Transform-record semantics (3.3); glow sprite geometry and fading (7.5); presentation of the supersampled
render target and post-process (10.2); dynamic-light selection and attenuation textures (12); exact
in-group order of non-sorted blended passes (10.3).

## 15. Provenance

Date 2026-11-23; revised 2026-11-26: sections 1 and 5, element 4 of the library classes (after
object-projected-light.md). Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`. Data examined:
`shaders.xr`, `shaders\r1\*.s` and program sources, `textures\textures.ltx`, `config\weathers\
weather_default.ltx`, the shipped default `user.ltx`, `level` and `level.geom` of `l01_escape`, chunk 2 of
the `level` files of all 30 shipped levels. Analysis: parsing of the data files with throwaway tools;
static analysis (Ghidra) of the shipped `XR_3DA.exe` and `xrRender_R1.dll` to establish property layouts,
element selection, pass states, constant sources, weather blending and fog. A validator written from this
text reproduces the counts of section 13.
