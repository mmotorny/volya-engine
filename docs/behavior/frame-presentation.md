# Frame presentation (first renderer)

How the first (static-lighting) renderer, `renderer_r1`, of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition) gets the drawn world onto the screen: the off-screen target and its size from `r__supersample`,
when a frame is drawn into it, and the presentation pass that reduces it to the screen through the program
`postprocess` or `postprocess_d`, with the screen-effect parameters (blur, gray, duality, noise, colours),
`c_brightness`, `r1_pps_u` and `r1_pps_v`. It also names and scopes the game-driven effects that send a frame
through the target. This is item 4 of the scoping specification `docs/behavior/r1-settings-scope.md`
(here RS), including both of its review gaps.

Builds on: `docs/behavior/static-shading.md` (SS: scripted definitions 3.4, sampler states, pixel-program
precision rules, target size 10.2), `docs/behavior/particle-drawing.md` (PD: the distortion pass, section 6),
`docs/behavior/options-screens.md` (OS: video restart 5.4, back-buffer format 9),
`docs/behavior/start-position.md` (SP: frame time, 7.2), `docs/behavior/hud-scope.md` (HS: proposed
specifications 19 `ppe-format`, 20 `screen-effects`, 21 `hud-night-vision`), `docs/formats/db-archive.md`.

Conventions:

- "The engine" is `XR_3DA.exe`, "the renderer" `xrRender_R1.dll`, "the game library" `xrGame.dll`, all
  in `bin\` of the shipped edition.
- W × H is the screen (back-buffer) size in pixels (`vid_mode`, OS 9); W_t × H_t the target size.
- floor, ceil: the usual integer functions on single-precision (32-bit) floating-point values; every
  arithmetic step below is single precision unless stated.
- A **frame** is one world frame of the renderer; it runs only while a level is loaded: it is driven by
  the level's per-frame drawing [FP-3]. Without a level nothing here happens.
- Pixel (i, j) is column i, row j, from 0 at the top left. Screen positions follow Direct3D 9, where the
  centre of pixel (i, j) is at screen position (i, j) (**documented** Direct3D 9 behaviour, "directly
  mapping texels to pixels"; an implementation whose pixel centres are at (i + 0.5, j + 0.5) shifts
  positions by half a pixel to keep the same sampling).
- Every claim carries a tag [FP-n]; the checks are one script in the dirty workspace,
  `fpres/checks/run.py` (helpers in the same folder). All 108 checks pass.

## 1. The off-screen target

### 1.1 Size

With s = `r__supersample` (integer 1 … 4, RS 1), per axis [FP-2, FP-1]:

    n = floor(N · √s + 0.5), clamped to 128 … 2048, then reduced by 1 if odd

with N = W for the width and N = H for the height. This refines SS 10.2 ("scaled by √s, clamped, made
even"): the rounding is to nearest (halves up) and "made even" is a decrement.

| Screen | s | Target W_t × H_t |
|---|---|---|
| 1024 × 768 | 1 | 1024 × 768 |
| 1024 × 768 | 2 | 1448 × 1086 |
| 1024 × 768 | 3 | 1774 × 1330 |
| 1024 × 768 | 4 | 2048 × 1536 |
| 800 × 600 | 2 | 1130 × 848 |
| 1280 × 1024 | 2 | 1810 × 1448 |
| 1920 × 1080 | 2 | 2048 × 1526 |
| 2560 × 1440 | 1 | 2048 × 1440 |

[FP-2]. The last two rows show that the clamp can make the target smaller than the screen on one axis.

**Shipped default.** `user.ltx` holds `r__supersample 2` [FP-15], so at 1024 × 768 the world is drawn at
1448 × 1086 pixels (2.0 times the pixel count, factor 1.4140625 per axis) and reduced as in 3.4.

### 1.2 What is built

When the target is built the renderer creates, all of size W_t × H_t [FP-2]:

- the colour target `$user$rendertarget` and the distortion target `$user$distort` (PD 6.2), both in the
  format of the screen's back buffer (OS 9);
- a depth-stencil buffer: when W_t = W and H_t = H the screen's own depth buffer is used; otherwise a new
  one of size W_t × H_t in the device's depth format;
- the presentation pass `postprocess` (always) and `postprocess_d` (only when distortion is available,
  PD 6.1: pixel-shader version ≥ 1.4 and no `-nodistort` on the command line; this is decided before the
  target is built) [FP-2, FP-13];
- a quad geometry of four vertices (section 3.2).

The target is **usable** only when both colour textures were created; otherwise every frame is drawn
straight to the screen [FP-2]. The engine log receives `* SSample: <W_t>x<H_t>` and then
`* SSample: enabled` or `* SSample: disabled` [FP-2].

The target is rebuilt at device creation and at every device reset (video restart, OS 5.4) [FP-2]; a new
value of `r__supersample` therefore changes the size only then (RS 2.7). Rebuilding also resets the
renderer's copy of the screen-effect parameters to its own defaults (2.3).

## 2. Screen-effect parameters

### 2.1 The parameter set

The presentation is controlled by ten values, named here as the game scripts name them (`effector_params`
with `blur`, `gray`, `dual.h`, `dual.v`, `noise.intensity`, `noise.grain`, `noise.fps`, `color_base`,
`color_gray`, `color_add`, `lua_help.script`) [FP-18, FP-9, FP-10]:

| Value | Kind | Renderer's own default | Identity (engine) |
|---|---|---|---|
| blur | real | 0 | 0 |
| gray | real | 0 | 0 |
| dual.h | real, stored as its absolute value | 0 | 0 |
| dual.v | real, stored as its absolute value | 0 | 0 |
| noise.intensity | real | 0 | 0 |
| noise.grain | real | 1 | 1 |
| noise.fps | real, stored as \|value\| + 10⁻⁷ | 25 | 30 |
| color_base | colour, bytes R, G, B | 127, 127, 127 | 0.5, 0.5, 0.5 → bytes 128, 128, 128 |
| color_gray | colour, bytes R, G, B | 85, 85, 85 | 0.333, 0.333, 0.333 → bytes 85, 85, 85 |
| color_add | colour, bytes R, G, B | 0, 0, 0 | 0, 0, 0 → bytes 0, 0, 0 |

[FP-2, FP-5, FP-7, FP-10].

### 2.2 Combination and hand-over (engine)

The engine keeps the set for the player's view. On each camera update with a level running [FP-10]:

1. start from the identity;
2. for every active screen effect (section 4), the effect produces a full set of ten values (colours as
   three reals); the effect's values minus the identity are added to the running set; an effect that
   reports that it has finished is removed instead (how effects produce values, weights and end is the
   specification `screen-effects`, HS 20);
3. when no effect contributed, the set is the identity;
4. a negative noise.grain is replaced by the identity's 1;
5. the set is handed to the renderer, noise.grain clamped to 0.001 … 1000, each colour channel c converted
   to the byte clamp(floor(c · 255 + 0.5), 0, 255) [FP-1, FP-10].

Whether step 5 runs on a given camera update depends on the camera (an active camera effect can take over
the view); its rules belong to the camera's own specification (**unestablished** here) [FP-10].

While the main menu is shown, step 5 hands over the identity instead of the combined set; opening the main
menu while a level is loaded also hands over the identity at once [FP-10, FP-4].

Only these two engine steps (the hand-over and the identity hand-over) set the renderer's values; nothing
else in the engine or the game library does, and the renderer never sets them itself [FP-9, FP-11].

### 2.3 Values in force

The renderer uses the values last handed over. After the target is (re)built and before the first
hand-over, the renderer's own defaults apply (left column of 2.1); note color_base 127 there against 128
for the identity [FP-2, FP-10].

### 2.4 When effects count as active

The effects count as **active** unless all of the following hold [FP-8]:

- blur ≤ 0.001, gray ≤ 0.001, noise.intensity ≤ 0.001;
- dual.h ≤ 0.001 and dual.v ≤ 0.001 (absolute values, 2.1);
- each of the R, G, B bytes of color_base is within 2 of 127 (125 … 129);
- each of the R, G, B bytes of color_add is ≤ 2;
- the user interface does not ask for the post-process (2.5).

color_gray, noise.grain and noise.fps never make the effects active. The identity (bytes 128 and 0) and
the renderer's defaults are both inactive [FP-8, FP-10].

### 2.5 The user interface's request

The user interface asks for the post-process while the main menu is shown and not hidden, and a flag is
set that opening the main menu sets and one of the main menu's own windows also sets or clears (which
window, and when, is **unestablished**) [FP-4, FP-10]. While it asks:
the main menu draws itself into the current target at the start of the presentation (3.1 step 1), and its
own distortion into the distortion target during the distortion pass (3.1 step 3); the world's
distortion list is not drawn then (PD 6.2) [FP-4]. What the main menu draws is outside this specification.

## 3. Per frame

### 3.1 Order

At the start of a frame [FP-3]:

1. the frame is drawn into the off-screen target when the target is usable **and** at least one of:
   the effects are active (2.4); `r__supersample` > 1; the distortion pass ran in the previous frame.
   Otherwise it is drawn straight to the screen (back buffer and its depth buffer);
2. the chosen depth buffer is cleared to 1.0 (and its stencil to 0, when the device has one: **believed**, the
   stencil bit comes from a device property not examined here); the
   colour is not cleared (the device flag that would also clear it is off from the start and no console
   variable sets it) [FP-3].

Then the world is drawn (SS 10.3, sky, flares…) [FP-3]. At the end of the frame, the presentation [FP-4]:

1. the user interface's main post-process drawing (2.5), into the current target;
2. "use the target" is evaluated again exactly as in step 1 of the frame start;
3. when distortion is available and (the world distortion list is non-empty or the interface asks, 2.5),
   the distortion pass of PD 6.2 runs — this records the frame as one with a distortion pass; it runs
   even when the frame went straight to the screen (PD 6.3);
4. the screen becomes the current target;
5. when the target was used (step 2), the presentation quad (3.2–3.4) is drawn on the screen with
   `postprocess_d` if step 3 ran, else `postprocess`. When the target was not used, nothing more is drawn:
   the picture already on the screen is the frame.

Drawing after the world frame (the interface over the level) therefore goes straight to the screen, not
through the target, and is neither supersampled nor affected by the screen effects (**believed**: the
presentation leaves the screen current and only the frame start and the distortion pass make another
target current; the interface's order relative to the world frame is outside the renderer) [FP-3, FP-4].

### 3.2 The quad

Four vertices, given directly in screen coordinates (no vertex program: the scripted definition names the
vertex program `null`, which means none) [FP-5, FP-12]. With U = `r1_pps_u`, V = `r1_pps_v` (reals −1 … 1, in
pixels; 0 before set; the shipped `user.ltx` sets both to 0) [FP-15]:

| Corner | Position (x, y) | tc0 | tc1 | tc2 |
|---|---|---|---|---|
| left bottom | (U, V + H) | (a₀, b₁′) | (a₀′, b₁) | (x₀, y₁) |
| left top | (U, V) | (a₀, b₀) | (a₀′, b₀′) | (x₀, y₀) |
| right bottom | (U + W, V + H) | (a₁′, b₁′) | (a₁, b₁) | (x₁, y₁) |
| right top | (U + W, V) | (a₁′, b₀) | (a₁, b₀′) | (x₁, y₀) |

with depth z = 10⁻⁷ and 1/w = 1 for all four; drawn as two triangles covering the rectangle [FP-5, FP-4]
(every coordinate depends on x alone or on y alone, so the split does not matter). The vertex colours
are in 3.3.

Scene coordinates [FP-5]: let β = blur, except that β = 1 whenever H_t ≠ H (the renderer also stores 1 as
its blur value then); d_h = dual.h, d_v = dual.v. Then

    a₀ = (0.5 + 0.5·β) / W_t,  a₁ = 1 + a₀,  b₀ = (0.5 + 0.5·β) / H_t,  b₁ = 1 + b₀
    a₁′ = a₁ − d_h/2,  b₁′ = b₁ − d_v/2,  a₀′ = a₀ + d_h/2,  b₀′ = b₀ + d_v/2

so tc0 spans the scene from (a₀, b₀) to (a₁ − d_h/2, b₁ − d_v/2), and tc1 the same span shifted by
(d_h/2, d_v/2).

Noise coordinates [FP-7]: the noise cell n = ceil(noise.grain · 256 + 10⁻⁷). A countdown T (0 when the
target is built) is lowered by the frame time in seconds (the value of SP 7.2) each time the quad is drawn
(3.1 step 5); when
it becomes negative, two offsets o_x = r₁ mod max(n, 1) and o_y = r₂ mod max(n, 1) are drawn from the
engine's shared generator (state ← state · 214013 + 2531011 mod 2³², value = (state >> 16) & 0x7FFF; two
draws, x first), and 1/noise.fps is added to T until T ≥ 0 [FP-7]. Then

    x₀ = (o_x + 0.5) / n,  y₀ = (o_y + 0.5) / n
    x₁ = ⌊W / n⌋ + x₀ + 1,  y₁ = ⌊H / n⌋ + y₀ + 1     (⌊W / n⌋ is integer division)

[FP-7]. With noise.grain < 1/256, n is 0 and the integer division is by zero; the behaviour then is
**unestablished** (the engine's grain clamp allows it).

### 3.3 Vertex colours and constant

- colour 0 (all four vertices): R, G, B = the color_base bytes, A = clamp(floor((1 − noise.intensity) · 255),
  0, 255) [FP-6, FP-1];
- colour 1: R, G, B = the color_gray bytes, A = clamp(floor((1 − gray) · 255), 0, 255) [FP-6];
- constant `c_brightness` = (R/255, G/255, B/255, 0) of the color_add bytes [FP-6].

Colours reach the pixel program as bytes / 255.

### 3.4 The pixel program

Pass states (`shaders\r1\postprocess.s`, vocabulary of SS 3.4): no fog, depth test off (always pass),
depth write off, no blending; samplers `s_base0` and `s_base1` = `$user$rendertarget`, clamped, linear
minification and magnification without mip; `s_noise` = `fx\fx_noise2`, wrapped, linear without mip
[FP-12]. `fx\fx_noise2` is `textures\fx\fx_noise2.dds` (`gamedata.db5`, 174,890 bytes): 256 × 256, 9 levels,
16-bit 5:6:5 colour without alpha (only the top level is sampled) [FP-12].

The program (`shaders\r1\postprocess.ps`, pixel-shader model 1.1) computes, with C0, C1 the vertex
colours (3.3), t0 = scene at tc0, t1 = scene at tc1, N = noise texel at tc2 [FP-12]:

    image = (t0.rgb + t1.rgb) / 2
    g     = image · C1.rgb                     (dot product)
    image = g + (image − g) · C1.a             (gray: C1.a = 1 leaves the image)
    image = image + (image · N.rgb · 2 − image) · (1 − C0.a)   (noise: C0.a = 1 leaves the image)
    out   = ((image · C0.rgb) + c_brightness.rgb) · 2,  alpha 1

and the result is clamped to [0, 1] when written (SS precision rules). `postprocess_d` (`postprocess_d.ps`,
model 1.4) is identical except that both scene look-ups add the distortion offset of PD 6.3 (from
`$user$distort` at tc0, scale 0.05, `common.h` `def_distort`) [FP-12]. Clamping of intermediate values to
the range of model-1.x registers, and their precision, are **unestablished** (they matter only when
image · N · 2 exceeds 1).

### 3.5 What reaches the screen

Linear filtering with clamped addressing samples the target at position, in target texels whose centres
are at integer positions (texel k covers k − 0.5 … k + 0.5):

    p_x = (a + (i − U) · (span_u) / W) · W_t − 0.5,   p_y likewise with j, V, H, H_t

where a and span_u are the tc's left value and width. With no duality and U = V = 0 this is
p_x = 0.5·β + i · W_t / W, p_y = 0.5·β + j · H_t / H [FP-5].

- **`r__supersample` 1** (target the screen's size, β = blur = 0 without effects): p = (i, j), exactly one
  target texel per pixel; the presentation copies the target (then the colour steps below).
- **`r__supersample` > 1** (β forced to 1): p_x = 0.5 + i · W_t/W, p_y = 0.5 + j · H_t/H, one bilinear
  sample of the four nearest texels per pixel (no wider filter). With the shipped settings at
  1024 × 768: p = (0.5 + 1.4140625 · i, 0.5 + 1.4140625 · j); pixel (0, 0) is the average of the target's
  texels (0..1, 0..1); positions beyond the last texel centre (i = 1023: p_x ≈ 1447.07) take the edge texel.

With both tc equal (no duality), t0 = t1, so with the identity in force (bytes 128, alpha 255, gray and
noise off, add 0) every presented pixel is min(1, S · 256/255) of the sampled scene colour S; with the
renderer's own defaults (base 127) it is S · 254/255 [FP-6, FP-10, FP-12].

**Duality** draws the scene twice, the two copies offset by (d_h, d_v) of the texture's size and averaged
(each copy also covers 1 − d/2 of the scene, so the picture is slightly magnified). **Disputed in review:** by
the corners of 3.2 the two copies are offset by (d_h/2, d_v/2) of the texture's size, not (d_h, d_v). **Blur** with the
target the screen's size shifts both look-ups by β/2 texel diagonally, blending each pixel with its
right/lower neighbours. **Gray** mixes towards the dot product with color_gray; **noise** multiplies by
twice the noise texture, tiled roughly every n pixels and jumping to a new random offset noise.fps times a
second; **color_base** scales (0.5 is neutral), **color_add** adds twice its value [FP-6, FP-7, FP-12].

## 4. What sends a frame through the target (game-driven effects)

With `r__supersample` > 1 (the shipped setting) every frame goes through the target, whatever the effects
(3.1). With `r__supersample` 1 a frame goes through it only when (a) the combined set of 2.2 is active by
2.4, (b) the main menu asks (2.5), or (c) the previous frame had a distortion pass (PD 6). Any effect whose
values move the set off the identity beyond the margins of 2.4 does (a); an effect that changes only
color_gray, noise.grain or noise.fps does not.

The game library has 12 kinds of screen effect and adds them in 17 places; the engine adds none itself
[FP-14]. Named by their data and triggers [FP-14, FP-17]; the owner of each place is established from the
binary, the trigger wording is a reading of the names involved (**believed**, to be established by HS 20):

| Source | Trigger | Data |
|---|---|---|
| Animated effect files | loaded from a `.ppe` file and played | 32 `anims\*.ppe` (HS 19) |
| Section effects of `config\misc\postprocess.ltx` | a section's `pp_eff_name` (a `.ppe` file) and `pp_eff_cyclic`, together with its camera animation | 39 sections, 8 active `pp_eff_name` lines [FP-14] |
| Player condition | alcohol and low psy health | `[effector_alcohol]`, `[effector_psy_health]` |
| Player hits | hit from a direction; sound shock | `[effector_fire_hit_0]` … `_7`, `[snd_shock_effector]` |
| Monster hits on the player | a creature's attack | `actor_hit_effect` key and creature attack sections (HS 20) |
| Explosions | an explosion (believed: near the player) | explosion effect sections (HS 20) |
| Creature abilities | bloodsucker feeding and its mind control, psy-dog and its phantoms, burer, pseudogiant strike, poltergeist flame, controller aura | creature sections (HS 20), `[effector_controller_aura]`, `[effector_controller_aura2]` |
| Anomaly zones | the player within a zone's radius, weighted by distance (believed) | `ppe_file`, `radius_min`, `radius_max` (`[postprocess_*]`, zone files) |
| Night vision, used items, outfits | the torch's night-vision mode; using an item; outfits (which sections each uses: believed) | `[effector_nightvision*]`, item sections (HS 21) |
| Scripts | `level.add_pp_effector` (a `.ppe` file), `level.add_complex_effector`, classes derived from `effector` filled through `effector_params`; weights through `set_pp_effector_factor` | `scripts\*.script` (HS 20) |
| Main menu | shown (2.5) | — |
| Distortion | world distortion sprites (PD 6) | — |

How each effect computes its values over time, how effects are weighted and stopped, is for the proposed
specifications `ppe-format` (HS 19), `screen-effects` (HS 20) and `hud-night-vision` (HS 21); the main
menu's drawing for the main-menu specification. This specification gives what the renderer does with
the combined values.

## 5. Verification facts

- Shipped `user.ltx`: `r__supersample 2`, `r1_pps_u 0.`, `r1_pps_v 0.` [FP-15].
- Target sizes: table of 1.1 [FP-2].
- Default in-level presentation at 1024 × 768: target 1448 × 1086, sample positions
  0.5 + 1.4140625 · i and 0.5 + 1.4140625 · j, colour gain 256/255 clamped at 1 [FP-2, FP-5, FP-10].
- `shaders\r1\.s` defines `t_rt = "$user$rendertarget"`, `t_distort = "$user$distort"`,
  `t_noise = "fx\\fx_noise2"` [FP-12].
- `fx_noise2.dds`: 174,890 bytes, 256 × 256, 9 levels, 5:6:5 [FP-12].
- `config\misc\postprocess.ltx`: 39 sections, 8 active `pp_eff_name` lines; 32 `anims\*.ppe` files [FP-14].

## 6. Unestablished

- Which camera updates hand the set to the renderer when a camera effect controls the view (2.2).
- Which main-menu window sets the flag of the interface's request (2.5).
- Behaviour with noise.grain below 1/256 (3.2).
- Range and precision of intermediate values in the model-1.x pixel programs (3.4).
- Nothing was executed: timing statements come from the order of operations in the binaries.

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archive entries resolved with the project's archive tool (`gamedata.db*`); the shipped
  `__support\userdocs\user.ltx`.
- Data analysis: `shaders\r1\.s`, `postprocess.s`, `postprocess.ps`, `postprocess_d.s`, `postprocess_d.ps`,
  `common.h`; `textures\fx\fx_noise2.dds` header; `config\misc\postprocess.ltx`; `anims\*.ppe` (count);
  `scripts\*.script`, including `lua_help.script`.
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only) of `XR_3DA.exe`, `xrRender_R1.dll`
  and `xrGame.dll` (target creation, frame start, presentation, distortion pass, screen-effect
  combination and hand-over, the main menu's request, effect sources); constants, import, export and
  run-time type tables read directly from the files; the integer-conversion instruction idioms evaluated
  on sample values. No execution or debugging.
- Approved specifications of the repository read for the parts built on.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- 2.2 step 2: before an effect produces its values, its set starts from all zeros (not from the identity), so a
  value the effect leaves unset contributes minus the identity's value; which values each effect sets belongs to
  `screen-effects` (HS 20).
