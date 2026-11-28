# Frame presentation (first renderer)

How the first (static-lighting) renderer, `renderer_r1`, of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition) gets the drawn world onto the screen: the off-screen target and its size from `r__supersample`,
when a frame is drawn into it, and the presentation pass that reduces it to the screen through the program
`postprocess` or `postprocess_d`, with the screen-effect parameters (blur, gray, duality, noise, colours),
`c_brightness`, `r1_pps_u` and `r1_pps_v`. It also names and scopes the game-driven effects that send a frame
through the target. This is item 4 of the scoping specification `docs/behavior/r1-settings-scope.md`
(here RS), including both of its review gaps. Section 5 gives the path and values in force in each state of
the program: the title menu and its other screens before any level, the loading screen, a running level,
the menu over a level, and the title menu after a level was unloaded.

Builds on: `docs/behavior/static-shading.md` (SS: scripted definitions 3.4, sampler states, pixel-program
precision rules, target size 10.2), `docs/behavior/particle-drawing.md` (PD: the distortion pass, section 6),
`docs/behavior/options-screens.md` (OS: video restart 5.4, back-buffer format 9),
`docs/behavior/view-projection.md` (VP: frame time, 1 step 2), `docs/behavior/hud-scope.md` (HS: proposed
specifications 19 `ppe-format`, 20 `screen-effects`, 21 `hud-night-vision`), `docs/formats/db-archive.md`,
`docs/behavior/display-gamma.md` (DG: the display gamma ramp), `docs/behavior/loading-screen.md` (LS),
`docs/behavior/title-menu.md` (TM: the lens, 4.6), `docs/formats/text-and-fonts.md` (TF: the main menu's
layout surface, 4.4).

Conventions:

- "The engine" is `XR_3DA.exe`, "the renderer" `xrRender_R1.dll`, "the game library" `xrGame.dll`, all
  in `bin\` of the shipped edition.
- W × H is the screen (back-buffer) size in pixels (`vid_mode`, OS 9); W_t × H_t the target size.
- floor, ceil: the usual integer functions on single-precision (32-bit) floating-point values; every
  arithmetic step below is single precision unless stated.
- A **frame** is one world frame of the renderer. It is run by exactly two callers: the per-frame drawing
  of a loaded level, and the per-frame drawing of the main menu while the menu is shown, with or without a
  level (section 3.6) [FP-3, FP-19]. Frame start (3.1) and presentation (3.1, 3.2–3.5) happen only inside a
  frame; what is drawn without one (the loading screen) goes straight to the screen (5.3).
- Pixel (i, j) is column i, row j, from 0 at the top left. Screen positions follow Direct3D 9, where the
  centre of pixel (i, j) is at screen position (i, j) (**documented** Direct3D 9 behaviour, "directly
  mapping texels to pixels"; an implementation whose pixel centres are at (i + 0.5, j + 0.5) shifts
  positions by half a pixel to keep the same sampling).
- Every claim carries a tag [FP-n]; the checks are two scripts in the dirty workspace:
  `fpres/checks/run.py` (FP-1 … FP-18, helpers in the same folder) and `fp576/checks/run.py` (FP-19 …
  FP-26 and the black-box check FP-B1, revision of 2026-11-26).

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

Hand-overs belong to a level: in the engine the hand-over is called only by the camera update; the game
library calls it (after a camera update) only for objects of a level (the player's view, two kinds of
mounted weapon, the vehicle) and, from its own per-frame update, only while a loaded level exists [FP-23]. Without a level
nothing hands values over. The identity hand-over is made in exactly three cases [FP-23, FP-20]:

- while the main menu is shown, in place of step 5 (above);
- when the main menu is opened while a level exists (above);
- when a level is unloaded: destroying the level hands over the identity, after its drawing is
  unregistered.

### 2.3 Values in force

The renderer uses the values last handed over. After the target is (re)built and before the first
hand-over, the renderer's own defaults apply (left column of 2.1); note color_base 127 there against 128
for the identity [FP-2, FP-10].

So (section 5 for each state): from the start of the program until the first level hands values over,
the defaults are in force (no hand-over happens without a level, 2.2; the device is created, and the
target built, at startup); after a level is unloaded the identity is in force until the next hand-over
or the next rebuild of the target (device reset, 1.2: a video restart, a lost device, or starting the next
level, DG 4), which brings back the defaults [FP-2, FP-23]. Measured: at the first title screen the
picture is darker than the menu texture, matching the defaults' gain 254/255 and not 1 or 256/255 [FP-B1].
**Disputed in review:** the measurement separates the gains only under an assumption the text does not state:
that the reference renderer truncates when it converts to 8 bits (then any gain below 1 shows each
non-zero channel one level lower). With rounding, gain 254/255 changes almost none of the background's
levels (mostly below 128) and leaves mean signed differences of −0.76 and −1.35 at `r__supersample` 1 and
4, close to the −0.88 and −1.47 of gain 1; so the black-box result is **believed** support, the values
in force rest on the static analysis (2.2, FP-23).

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

The user interface asks for the post-process while all of these hold [FP-4, FP-10, FP-21]:

- the main menu is shown;
- no saved-game picture is pending (below);
- the **request flag** is set. Opening the main menu sets it. Otherwise only the script method
  `SetVisibleMagnifier` of the title menu's button list writes it, with its argument; the menu's script
  (`scripts\ui_main_menu.script`, function `Show`) calls it with false when the load, save, options or
  multiplayer screen (or the spawn dialog) opens, and the screens call `Show(true)` when they close.
  The menu's message boxes (quit, quit to the title, last save) leave it set (TM 4.6) [FP-21].

**Saved-game picture.** Saving (the autosave and the save command) asks for a picture of the next frame:
the main menu marks a picture pending and, when a level is shown under it, puts the level's own drawing
back for that frame; once the frame counter has passed the next frame, the picture is taken, the mark
cleared and the level's drawing removed again [FP-22]. While the mark is set the main menu runs no frame and draws nothing (3.6).

While the interface asks: the main menu draws its interface into the current target at the start of the
presentation (3.1 step 1), laid out on the target's size (TF 4.4), and its own distortion into the
distortion target during the distortion pass (3.1 step 3); the world's distortion list is not drawn then
(PD 6.2) [FP-4, FP-21, FP-25]. While the menu is shown but does not ask (request flag clear), it draws its
interface after the frame, straight onto the screen, laid out on the screen's size (3.6) [FP-21]. What the
main menu draws is outside this specification (TM, TF).

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
This includes the main menu's interface when the menu does not ask for the post-process (2.5, 3.6) and the
loading screen (5.3) [FP-21, FP-24].

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
target is built) is lowered by the frame time in seconds (the value of VP 1 step 2) each time the quad is drawn
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
  texels (0..1, 0..1); positions beyond the last texel centre (i = 1023: p_x = 1447.086) take the edge texel
  [FP-26].

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

### 3.6 Who runs a frame

The device keeps a list of per-frame drawings and calls them on each device frame [FP-20]. Two of them run
a frame [FP-19, FP-20]:

- **A loaded level.** Its drawing is registered when loading ends and unregistered when the level is
  destroyed. It computes the view, then runs the frame.
- **The main menu.** Its drawing is registered when the menu is shown and unregistered when it is hidden.
  Opening the menu while a level exists also unregisters the level's drawing (and, conditionally, its
  per-frame update); hiding the menu registers them again. So while the menu is shown exactly one frame
  per device frame is run, by the menu. On each device frame the menu's drawing, unless a saved-game
  picture is pending (2.5; then it does nothing):
  1. computes the level's view, when a level exists;
  2. runs the frame (3.1), with or without a level;
  3. when the interface does not ask for the post-process (request flag clear, 2.5), draws the menu's
     interface onto the screen; otherwise the interface was drawn during step 2, at presentation step 1.

The renderer's view computation and frame are run by nothing else: in the engine only the level's drawing
calls them, in the game library only the main menu's [FP-19]. Without a level, frame start, the choice of
target, the presentation and its colour step run exactly as in 3.1–3.5; what the world part of such a
frame draws is outside this specification (the title menu's opaque background covers the screen, TM).

## 4. What sends a frame through the target (game-driven effects)

With `r__supersample` > 1 (the shipped setting) every frame goes through the target, whatever the effects
(3.1). With `r__supersample` 1 a frame goes through it only when (a) the combined set of 2.2 is active by
2.4, (b) the main menu asks (2.5; true on every frame of the title menu itself, 5.1), or (c) the previous
frame had a distortion pass (PD 6). Any effect whose
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

## 5. States of the program

In every state the display gamma ramp of DG (from `rs_c_gamma`, `rs_c_brightness`, `rs_c_contrast`; set at
device creation and again in the cases of DG 3) acts on everything presented, whatever path drew it; the
presentation pass has no gamma step of its own (3.4) [FP-12]. With the shipped settings the ramp is the
identity (DG 2.1). The states below differ only in the path and in the screen-effect values.

### 5.1 Title menu before any level

The title menu, its New Game list and its message boxes, from startup until the first level is loaded
[FP-19, FP-21, FP-8, FP-B1]:

- **Frames:** run by the main menu, one per device frame (3.6).
- **Path:** the request flag is set (2.5), so the interface asks for the post-process and the effects are
  active (2.4): every frame is drawn into the off-screen target whenever it is usable, at every
  `r__supersample`, 1 included (3.1). Measured with `r__supersample` 1 and 4 [FP-B1]. **Disputed in review:**
  at `r__supersample` 1 the frames tell the target path from the direct one only under the truncation
  assumption of 2.3; the path rests on 2.4, 2.5 and 3.1.
- **Target size:** 1.1; with `r__supersample` 1 the screen's size unless an axis exceeds 2048 (2560 × 1440
  gives 2048 × 1440).
- **Interface:** drawn into the target at presentation step 1, laid out on W_t × H_t (TF 4.4).
- **Distortion:** when distortion is available (PD 6.1) the distortion pass runs every frame, because the
  interface asks (3.1 step 3): the distortion target is cleared to 127 / 255, the menu draws its lens into
  it (TM 4.6), and the frame is presented with `postprocess_d`. Where the cleared value is left both
  look-ups move by (127/255 − 0.5) · 0.05 = −9.80 × 10⁻⁵ of the target per axis, i.e. −9.80 × 10⁻⁵ · W_t
  and −9.80 × 10⁻⁵ · H_t texels. Without distortion `postprocess` is used and nothing moves.
- **Values:** the renderer's defaults (2.3), as no level has handed values over: blur 0, gray 0, duality 0,
  noise intensity 0, color_base (127, 127, 127), color_gray (85, 85, 85), color_add 0. The quad's colours
  are C0 = (127, 127, 127, 255), C1 = (85, 85, 85, 255), `c_brightness` = (0, 0, 0, 0) (3.3); gray and
  noise have no effect (alphas 1); the noise offsets are still drawn from the shared generator (3.2).
  Every presented pixel is S · 254/255 of the scene colour S sampled from the target (3.5), so white is
  shown as 254/255.
- **Placement and filtering:** 3.5, with β = 1 whenever H_t ≠ H (any `r__supersample` > 1 below the clamp,
  and the clamped cases), else β = blur = 0.

### 5.2 Other title-menu screens

The load, save, options and multiplayer screens (and the spawn dialog), opened from the title menu
[FP-21]:

- **Frames:** still run by the main menu (3.6).
- **Path:** the request flag is clear, so the interface does not ask; the values (defaults or identity)
  are inactive (2.4). With `r__supersample` > 1 every frame goes through the target and is presented as in
  5.1, with `postprocess` (no distortion pass: the interface does not ask, and the world's distortion list
  is filled only by a level's drawing, PD 6.2; **believed** empty without a level). With
  `r__supersample` 1 the frame goes straight to the screen, except the first frame after one with a
  distortion pass (the frame after the screen opened, when distortion is available), which goes through
  the target (3.1).
- **Interface:** drawn after the frame, straight onto the screen, laid out on W × H (TF 4.4): it is not
  resampled and not scaled by the colour values; only what the world part of the frame left on the screen
  under it went through the presentation.

Closing the screen sets the request flag again and returns to 5.1.

### 5.3 Loading screen

From the choice of New Game or a saved game until the level is shown (LS 2) [FP-20, FP-24]:

- **Frames:** none while loading: the main menu is hidden when loading starts (LS 2 step 2), unregistering
  its drawing, and the level's drawing is registered only when loading ends (3.6).
- **Path:** each loading image is drawn between the device's begin and end of a frame, without changing
  the render target and without the renderer's frame or presentation [FP-24]; it is drawn at the screen's
  size (LS 3) onto the target current at that moment, which is the screen (**believed**: the last
  presentation left the screen current, 3.1 step 4, and nothing in between makes another target
  current). No screen-effect value applies to it.
- **Overlay frames:** during the 30 frames after loading (LS 2 step 4) the level runs its frames (5.4) and
  the loading image is drawn at the end of each device frame, after the presentation, onto the screen;
  after the 30th the ramp is set again (DG 3.3) [FP-24].
- **Values:** starting a level resets the device (DG 4), which rebuilds the target and restores the
  renderer's defaults (1.2) until the level's first hand-over.

### 5.4 Level running

Frames are run by the level (3.6) and the target is used as in 3.1 and section 4. The values are those of
the last hand-over (2.2): the identity when no effect runs, so every presented pixel is
min(1, S · 256/255) (3.5); **Disputed in review:** only for frames that go through the target; with
`r__supersample` 1, no active effect and no distortion pass in the previous frame the frame goes straight
to the screen (3.1) and is shown unscaled; the defaults only between the device reset of 5.3 and the first hand-over. The
interface over the level goes straight to the screen (3.1).

### 5.5 Main menu over a level

Opening the menu hands over the identity and moves the frame to the menu, which also computes the level's
view (2.2, 3.6). With the title menu's request set the path is that of 5.1 and with a screen of 5.2 open
that of 5.2, with the identity's values in place of the defaults: gain 256/255, clamped. While a saved-game
picture is pending the level's own drawing runs that frame (2.5).

### 5.6 Title menu after a level was unloaded

Destroying the level hands over the identity (2.2). The title menu and its screens then follow 5.1 and 5.2
with the identity's values (C0 = (128, 128, 128, 255), gain min(1, S · 256/255)) instead of the defaults,
until the next rebuild of the target (a video restart, for example from the options screen, a lost device,
or the next level's start), which restores the defaults (2.3) [FP-23, FP-2].

### 5.7 Worked examples

All sample positions are in target texels as in 3.5 (texel centres at whole numbers), with U = V = 0 and no
duality [FP-26].

**1920 × 1080, `r__supersample` 4, title menu before any level (5.1).**

- Target: 1920 · 2 + 0.5 → 3840 and 1080 · 2 + 0.5 → 2160, both clamped to 2048: 2048 × 2048. The menu is
  laid out on 2048 × 2048 (TF 4.4).
- Every frame goes through the target; H_t ≠ H, so β = 1.
- Pixel (i, j) samples p_x = 0.5 + 1.0666667 · i, p_y = 0.5 + 1.8962963 · j, one bilinear sample (clamped):
  pixel (0, 0) averages texels (0..1, 0..1); the last column (p_x = 2047.433) takes texel column 2047;
  the last row (p_y = 2046.604) mixes rows 2046 and 2047 as 0.396 : 0.604.
- With distortion available, outside the lens both positions are lower by 9.80 × 10⁻⁵ · 2048 = 0.2008:
  p_x = 0.2992 + 1.0666667 · i, p_y = 0.2992 + 1.8962963 · j.
- Colour: S · 254/255; a sampled (200, 100, 50) / 255 is shown as (199.216, 99.608, 49.804) / 255, white as
  254 / 255. After a level was unloaded (5.6): (200.784, 100.392, 50.196) / 255, and every channel of 254
  or more as 255.
- Other title-menu screens (5.2): the world part of the frame is presented the same way; the screen's
  interface is drawn afterwards at 1920 × 1080, one-to-one.

**1024 × 768, `r__supersample` 2 (the shipped setting).**

- Target: 1024 · 1.4142135 + 0.5 → 1448 (1449 made even), 768 · 1.4142135 + 0.5 → 1086: 1448 × 1086
  (1.1). The title menu is laid out on 1448 × 1086.
- β = 1; p_x = 0.5 + 1.4140625 · i, p_y = 0.5 + 1.4140625 · j; the last column and row (1447.086, 1085.086)
  take the edge texels.
- With distortion available, outside the lens: p_x = 0.3580 + 1.4140625 · i, p_y = 0.3935 + 1.4140625 · j
  (offsets 0.1420 and 0.1065 texels).
- Colour as in the first example: defaults before any level (S · 254/255), identity in a level and after one
  (min(1, S · 256/255)).

**1024 × 768, `r__supersample` 1, title menu (5.1).** The target is 1024 × 768 and used (the menu asks),
β = 0: p = (i, j), one texel per pixel, shown at S · 254/255. With distortion available, outside the lens
p = (i − 0.1004, j − 0.0753): each pixel takes 0.1004 of its left and 0.0753 of its upper neighbour
(bilinear weights). On the other title-menu screens (5.2) the frame goes straight to the screen.

## 6. Verification facts

- Shipped `user.ltx`: `r__supersample 2`, `r1_pps_u 0.`, `r1_pps_v 0.` [FP-15].
- Target sizes: table of 1.1 [FP-2].
- Default in-level presentation at 1024 × 768: target 1448 × 1086, sample positions
  0.5 + 1.4140625 · i and 0.5 + 1.4140625 · j, colour gain 256/255 clamped at 1 [FP-2, FP-5, FP-10].
- `shaders\r1\.s` defines `t_rt = "$user$rendertarget"`, `t_distort = "$user$distort"`,
  `t_noise = "fx\\fx_noise2"` [FP-12].
- `fx_noise2.dds`: 174,890 bytes, 256 × 256, 9 levels, 5:6:5 [FP-12].
- `config\misc\postprocess.ltx`: 39 sections, 8 active `pp_eff_name` lines; 32 `anims\*.ppe` files [FP-14].
- Title menu before any level, 1024 × 768, `r__supersample` 1 and 4: the presented background is on average
  darker than `textures\ui\ui_mainmenu.dds` by what the gain 254/255 predicts (mean signed difference
  +0.09 and −0.49 levels against the model, −0.88 and −1.47 for gain 1 or 256/255) [FP-B1]. **Disputed in
  review:** these figures assume a truncating conversion to 8 bits in the reference renderer (2.3); with
  rounding the gain-254/255 model gives −0.76 and −1.35.
- Worked numbers of 5.7 [FP-26].

## 7. Unestablished

- Which camera updates hand the set to the renderer when a camera effect controls the view (2.2).
- Behaviour with noise.grain below 1/256 (3.2).
- Range and precision of intermediate values in the model-1.x pixel programs (3.4).
- Timing statements come from the order of operations in the binaries; the only executions were black-box
  runs of the title screen (FP-B1).
- What the world part of a frame draws without a level (3.6).

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
- **Revision 2026-11-26** (frames without a level, issue 576; same edition and tools). Static analysis of
  the three binaries: every caller of the renderer's view computation and frame, the device's list of
  per-frame drawings and who registers in it (level loading and destruction, showing and hiding the main
  menu), the main menu's per-frame drawing, its presentation hooks and the request flag's writers (the
  script method and its binding), the saved-game picture request, every caller of the camera hand-over
  and of the identity hand-over and the owning types of the callers, the loading-screen drawing and the
  device's frame begin and end. Scripts: `ui_main_menu.script`, `ui_base_dialog.script`,
  `ui_load_dialog.script`, `ui_mm_opt_main.script`, `ui_mp_main.script`. Black-box: frames of the unmodified
  game at its first title screen (headless, through the project's runner of the original), 1024 × 768
  windowed with `r__supersample` 1 and 4, compared with the background texture. The archive comparison
  inside FP-12 was not rerun (the project's archive tool was not built on this machine); the other checks
  of `fpres/checks/run.py` were rerun and pass. Changes against the previously approved text:
  - conventions: a frame is run by a loaded level or by the main menu, with or without a level (was "only
    while a level is loaded… Without a level nothing here happens");
  - 2.2: hand-overs need a level; the identity is also handed over when a level is unloaded;
  - 2.3: values in force before the first level (defaults) and after an unload (identity), measured gain;
  - 2.5: the request flag's writer (`SetVisibleMagnifier`, from the menu's `Show`) and the screens that clear
    it (was **unestablished**); the pending saved-game picture; where the interface draws when it does not
    ask;
  - 3.1: the main menu's interface without the request, and the loading screen, go straight to the screen;
  - 3.5: the last sample position at 1024 × 768 is 1447.086 (was ≈ 1447.07);
  - 3.6 (new): who runs a frame; 4: the title menu always asks;
  - 5 (new): path, target and values per state, worked examples; former 5 and 6 renumbered 6 and 7.

## Gaps found in review

- 2.2 step 2: before an effect produces its values, its set starts from all zeros (not from the identity), so a
  value the effect leaves unset contributes minus the identity's value; which values each effect sets belongs to
  `screen-effects` (HS 20).
- Section 5: the pictures shown without a level outside the main menu's own screens are not covered: the
  credits sequence the title menu's credits button starts (`credits_seq`, through the game's tutorial
  sequences) and the logo sequence before the first title screen (`intro_logo`, skipped with `-nointro`);
  for each, whether the main menu stays shown, the path and the values in force.
