# Title menu: the green top row over the panorama video

What the original draws in the top screen row (and the right screen column) where the title menu's
panorama video (`back_movie`, `textures\ui\ui_mm_main_back_crop.ogm`) shows through the background, why it
is green there, whether it repeats run to run, and what a reimplementation must reproduce. Answers the
question of GitHub issue 800 (the top row of the original's title-menu frame is green-tinted in columns
462–1919 at 1920 × 1080).

Builds on the approved specifications `docs/behavior/title-menu.md` (TM: layers, 3.1–3.2),
`docs/behavior/startup.md` (ST: video frames as textures, 6.6), `docs/formats/text-and-fonts.md` (TF: the main menu's layout surface, 4.4), `docs/formats/ui-layout.md` (UL: picture
geometry 5.3, shaders 8) and `docs/behavior/frame-presentation.md` (FP: target size 1.1, presentation 3.5,
title-menu state 5.1). Pixel centres follow Direct3D 9 (pixel (i, j) has its centre at (i, j), FP
conventions). Every claim carries a tag [GR-n]; all checks are run by `greenrow/checks/run.py` in the
dirty workspace.

## 1. Answer

The green comes from **texture addressing and filtering at the video's top edge reaching texels the game
never writes**, which hold zero:

- The video texture is 1024 × 512 with the 876 × 400 picture in its top-left corner (ST 6.6); the texels
  outside the picture are never written (ST 6.6). The texture is created by the engine with one level, the
  32-bit A8R8G8B8 format and the managed pool, and the game sets no initial content [GR-8].
- In the captured runs those unwritten texels hold **0 in every channel** [GR-5, GR-6]. Through the video
  shader (ST 6.6, pixel-shader path: the texture holds Y′, Cb, Cr unconverted) a texel of Y′ = Cb = Cr = 0
  converts to RGB (0, 135, 0) before clamping, i.e. green with strongly negative red and blue
  (R ≈ −222, B ≈ −275 on the 0–255 scale) (ST 6.6).
- `hud\movie` samples with **wrap** addressing and linear filtering (ST 6.6). The `back_movie` quad starts
  half a pixel above the target's top row (UL 5.3: top = floor(0 · sy) − 0.5) with texture coordinate v = 0
  there. When one UI unit is more than one target pixel tall (sy > 1, the usual case), the top target row's
  sample point lies above the centre of texel row 0, so linear filtering blends in texel row 511, the
  wrapped neighbour, which is never written (section 2.1). Conversion happens after filtering (ST 6.6), so
  the blend is a blend of Y′Cb′Cr′ toward zero, which after conversion lowers red and blue far more than
  green: the green tint.
- The background (`ui_menu_backgraund`) covers the video wherever it is opaque. In its texture row 0
  (`textures\ui\ui_mainmenu.dds`) alpha is 239–255 for texels 0–241 and below 255 for every texel 242–1023
  (68–247) [GR-4]; at 1920 × 1080 texel 242 lies at screen column ≈ 454. So the video, with its blended
  top row, shows through along the whole top edge from about column 454 to the right edge; left of it the
  opaque background hides it. (This band of partial transparency along row 0 is wider than the "faint
  area" described in TM 3.1.)
- The background itself does **not** wrap: `hud\default` clamps (UL 8). Under wrap its row 1023 (alpha 0
  outside texels 399–634) would also reach row 0 and change every pixel of the row; the capture rules this
  out [GR-5].

It is **not** the video's own first row: in all 110 frames, coded rows 0–1 inside the picture columns 0–875
hold Y′ 30–239, Cb 109–131, Cr 127–137 (ordinary picture content; values below 16 occur only in coded
columns 876–879, which never reach the texture, ST 6.6) [GR-3]. It is not the menu's frame (no element of
`ui_mm_main.xml` other than `back_movie` and `background` covers the top row, TM 3) and not an uninitialised
render target (the row is fully determined by the model of section 2 to within ±3 levels) [GR-5].

## 2. The behaviour to reproduce

### 2.1 Weight of the never-written texels at the top edge

For `back_movie` one texture texel is one UI unit (rectangle 0, 0, 876, 400 drawn 876 × 400 units, UL 5.3).
With sy the UI scale of the surface the menu is drawn on (target height H_t / 768 while the menu draws into
the off-screen target, FP 5.1 and TF 4.4), the centre of target row 0 is at texel position
v = 0.5 / sy − 0.5 (texel centres at integer positions). With linear filtering and wrap addressing:

- sy > 1: target row 0 takes weight **w = 0.5 − 0.5 / sy** from texel row 511 (zero) and 1 − w from texel
  row 0; the rows below are unaffected (row 1 is at v = 1.5 / sy − 0.5 ≥ 0 for every sy ≤ 3).
- sy ≤ 1: no weight (v ≥ 0).

| Screen, `r__supersample` | Target W_t × H_t (FP 1.1) | sy | w |
|---|---|---|---|
| 1920 × 1080, 4 (the capture) | 2048 × 2048 | 2.6667 | 0.3125 |
| 1920 × 1080, 1 | 1920 × 1080 | 1.40625 | 0.1444 |
| 1024 × 768, 4 | 2048 × 1536 | 2 | 0.25 |
| 1024 × 768, 2 (shipped default) | 1448 × 1086 | 1.4141 | 0.1464 |
| 1024 × 768, 1 | 1024 × 768 | 1 | 0 |

The texel is the same in every column (row 511 is entirely outside the picture). The blended texel goes
through the shader's conversion (ST 6.6) and then the background is blended over it (source alpha /
inverse source alpha, UL 8). The presentation pass (FP 3.5) then reduces the target to the screen; with
`r__supersample` > 1 screen row 0 is a bilinear mix of target rows 0 and 1 (FP 3.5, with the distortion
offset of FP 5.1), so the green is diluted but stays in screen row 0 only [GR-2, GR-5].

### 2.2 The same at the right edge

The video quad's right edge coincides with the target's right edge (UI x 148 + 876 = 1024). With
sx = W_t / 1024 and L = floor(148 · sx) − 0.5 the quad's left edge (UL 5.3), the centre of the last target
column (W_t − 1) is at texel position u = (W_t − 1 − L) / sx − 0.5; the part of u beyond 875 is the weight
of texel column 876, which is outside the picture and holds zero. At 1920 × 1080 with `r__supersample` 4
(sx = 2): u = 875.25, weight 0.25; with `r__supersample` 1 (sx = 1.875): u = 875.5, weight 0.5; at
1024 × 768 with `r__supersample` 1: weight 0. **Disputed in review:** at 1920 × 1080 with `r__supersample` 1
the quad's right edge is at 277 + 876 · 1.875 − 0.5 = 1919.0 (UL 5.3: only the top-left corner is floored), so
the centre of the last target column lies exactly on the quad's right edge, which Direct3D 9's top-left fill
rule (documented) leaves uncovered: that column receives no video there, and the weight 0.5 does not apply;
what it shows instead was not established. The background's alpha in its column 1023 is below 255 in the
sky (TM 3.1), so the right screen column of the sky is green-tinted the same way: in the capture, screen
column 1919 over rows 2–559 has a mean green excess G − (R + B)/2 of 17.1, column 1918 of 2.5, column 1917
of −0.5, and the model with zero texels matches column 1919 (every third row) to within ±3 levels [GR-6].

The left edge (UI x 148) and the bottom edge (UI y 400) of the video lie under opaque background (TM 3.1),
so their blends are hidden. **Disputed in review:** only the bottom edge is fully covered (background alpha 255
in texel rows 399–400 over columns 148–1023). At the left edge the background's texel columns 147–149 have alpha
218–254 in about 20 of rows 0–400 (rows 0–10 and around rows 66–80), and the left edge's sample point lies left
of texel column 0's centre whenever sx > 1 (weight 0.25 at the capture's sx = 2), blending in texel column 1023
through wrap, which is never written; so that blend shows partly there. It was not compared with the capture
(the model was checked from screen column 280).

### 2.3 What a reimplementation must do

- Allocate the panorama video's texture at the power-of-two size of ST 6.6 and give every texel outside
  the picture the value 0 in all four channels (not the picture's edge texels, not white); upload frames
  only into the picture rectangle.
- Sample it with **wrap** addressing and linear filtering, with the UI quad placement of UL 5.3 at Direct3D
  9 pixel centres, and convert Y′CbCr to RGB **after** filtering (ST 6.6), clamping only the converted
  result.
- Draw the background over it with clamp addressing (UL 8).

With these rules the model of section 2.1 reproduces the capture's rows 0 and 1, columns 280–1919, every
pixel within 3 levels on every channel (mean largest-channel error 0.41 in row 0) [GR-5]. Example columns
(frame 95 of the video, see section 3), capture against the model: column 700 row 0 (24, 48, 17) against
(24, 48, 17), row 1 (48, 46, 44) against (49, 46, 44); column 1000 row 0 (21, 73, 19) against
(21, 73, 19) [GR-5]. With the edge row repeated instead of zero texels the capture's row 0 differs from that model by about
−27, +5, −26 levels (R, G, B) on average [GR-5].

The same applies to every video texture whose picture edge is sampled at a rectangle edge (ST 9 lists
this as unestablished for the startup videos; the zero content measured here is believed to hold for them
too, as their textures are created by the same code with the same arguments [GR-8]).

## 3. Run to run

**Established** for the two runs captured on the user's Windows PC (its real Direct3D 9 driver):

- the fidelity capture of 2026-11-27 (`r__supersample` 4, 1920 × 1080, renderer R1) [GR-1]: mean green
  excess 30.2 in row 0 over columns 470–1909, between −1.2 and −0.8 in rows 1–3 [GR-2]; the model with
  zero texels fits as above [GR-5]; the video frame shown is frame 95 (chosen by best fit of rows 4–12,
  mean error 0.16 levels) [GR-5];
- an earlier run (frames recorded 2026-11-26, `vid_mode 1024x768`, saved at 1920 × 1080 by the capture):
  with `r__supersample` 4 row 0 has mean green excess 11.5 over columns 300–1909 and rows 1–3 none; with
  `r__supersample` 1 no row has it [GR-7], as predicted by w = 0 at sy = 1 (section 2.1).

That the never-written texels hold zero on every machine is **believed**, not established: the game
does not fill them [GR-8, ST 6.6], so their content is the graphics runtime's; Direct3D 9 does not
document the initial content of a new managed texture, and fresh memory committed by Windows is zero
(documented for the operating system's page allocation), which is the believed cause. Other drivers or
runtimes (e.g. a translation layer) were not examined.

On hardware without pixel shader 2.0 (ST 6.6, CPU conversion) the never-written texels are not converted
and would be RGB black instead of green (believed, from the same zero content; not observed).
**Disputed in review:** on that path the window's normal shader is used (ST 6.6), which clamps (UL 8), so the
top row takes no wrapped texel and is not affected; only the right-edge blend with texel column 876 remains,
whose texels would then be black with alpha 0, lowering the video's colour and alpha there (not observed).

## 4. Not established

- The initial content of the never-written texels on other machines, drivers or Direct3D 9
  implementations (section 3).
- Whether `hud\movie`'s anisotropic filtering (ST 6.6) differs from bilinear filtering at these edges on
  other hardware; under the measured magnification the capture matches bilinear filtering.

## Checks

All in the dirty workspace, run by `greenrow/checks/run.py`:

- GR-1: the capture's recorded settings (`title-menu.user.ltx` next to the capture).
- GR-2: `othertint.py` on the capture, rows 0–3.
- GR-3: `videorow0.py`, the video's coded rows 0–1 over all frames (conformant decoder, cropping off).
- GR-4: alpha of `ui_mainmenu.dds` row 0 (decoded with FFmpeg).
- GR-5: `model.py` (frame choice, row comparisons, examples) and `fullrow.py` (rows 0–1, columns 280–1919;
  alternatives: texels 255, background wrapped).
- GR-6: `rightcol.py`, column 1919 rows 2–559.
- GR-7: `othertint.py` on the 2026-11-26 frames and their settings files.
- GR-8: `pool.py`, a byte search of `XR_3DA.exe` for the only texture creation with constant level count 1,
  usage 0, format A8R8G8B8 and the managed pool, made through the Direct3D 9 device's texture-creation
  method and followed by the video-stream error message.

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build), `bin\XR_3DA.exe`;
  `textures\ui\ui_mm_main_back_crop.ogm` and `textures\ui\ui_mainmenu.dds` as extracted from the archives.
- Black-box outputs: frames of the unmodified original captured on the user's Windows PC (the fidelity
  title-menu frame of 2026-11-27 and the title frames of 2026-11-26), analysed pixel by pixel.
- Data analysis: the video decoded with FFmpeg (cropping disabled), the DDS decoded with FFmpeg; a forward
  model of the drawing built only from the approved specifications named above and these data, compared
  with the capture.
- Binary analysis: static byte search of `XR_3DA.exe` for constant texture-creation arguments; earlier
  static analysis and emulated execution as recorded in ST 6.6. The game was not run for this
  specification.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- The title-menu fire video (`ui\ui_mm_fire`, texture 128 × 256 entirely filled by the picture, drawn opaque over the background) is sampled with the same wrap addressing, so its outermost pixels may blend in the opposite edge of its own picture; not examined.
- The video's left-edge blend where the background is partly transparent (section 2.2 review note) is not modelled or compared with the capture.
