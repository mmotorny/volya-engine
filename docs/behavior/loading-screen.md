# Loading screen (New Game to the first in-game frame)

Functional specification of what S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) shows and plays from
choosing a difficulty under New Game until the level is visible: the loading screen's elements, their
textures, rectangles at any resolution, the caption texts and font, the progress strip, the stage
sequence, the end of loading, audio, input and the new-game intro trigger. It refines the level-loading
scoping specification (`docs/behavior/level-loading.md`, sections 2.3–2.4) and replaces its loading-screen
details where they differ (section 9 lists the corrections).

Conventions:

- Paths are stored paths in the combined game-data tree (archive specification); `\` separates
  directories. Sizes are in bytes. Text files are Windows-1251.
- Referenced specifications: `docs/formats/ui-layout.md` (UI space, shader `hud\default`),
  `docs/formats/text-and-fonts.md` (string tables, fonts, drawing a string at a pixel position, shader
  `font`), `docs/behavior/title-menu.md`, `docs/behavior/startup.md` (movie sequences),
  `docs/behavior/level-loading.md` (what New Game executes, the level chosen, files per level).
- *W* × *H* is the output resolution in pixels. *s*ₓ = *W* / 1024, *s*ᵧ = *H* / 768 (UI-layout
  specification, section 3: independent axes, no letterboxing, at every aspect ratio).
- Screen positions in this document use the **Direct3D 9 convention**: the centre of pixel (*i*, *j*) is at
  (*i*, *j*), so a quad covering pixels 0 … *W*−1 exactly spans −0.5 … *W*−0.5. An implementation whose
  pixel centres are at (*i* + 0.5, *j* + 0.5) (Direct3D 10+, Vulkan, Metal, WebGPU) adds 0.5 to every
  position given here. Text positions (section 4.4) are integer pen positions as in the text-and-fonts
  specification and need no conversion.
- All vertex colours are given as 8-bit ARGB.

## 1. Overview

From the moment New Game's difficulty is chosen until the level is shown, the screen shows a static
loading image that is replaced by a new image each time a new **stage caption** begins. Each image is,
bottom to top: a full-screen background, a horizontal progress strip, the current caption, and — once the
level is known — a picture of the level. There are no hints, tips, animations, fades or "press a key"
prompts. After the last stage the image stays on top of the running level for 30 more rendered frames
with sound muted, then disappears at once; for a new game the intro movie sequence has started by then.

## 2. Timeline

1. The difficulty choice executes the console commands of the scoping specification, section 2.1
   (`g_game_difficulty …`, `start server(all/single/alife/new) client(localhost)`, `main_menu off`).
2. On the next processed frame the engine closes the title menu (`main_menu off` again), hides the
   console, resets the video device (visible effect not established; no mode change happens) and starts
   loading. Until the first caption begins nothing new is drawn (the last menu frame stays on screen).
   The caption `st_prefetching_objects` (when shown, section 5) is drawn during this frame's processing;
   afterwards the same frame is still rendered and presented the ordinary way (without the loading
   image) before the next caption's work starts. So one ordinary frame is presented between caption 1 and
   caption 2 on a first New Game, or before caption 1 on a later one. What that frame shows is **not
   established** (section 12).
3. Loading runs through the stages of section 5. Each caption, when it begins, immediately produces one
   new image (section 4). Between stages the image is not animated. (In addition, the image is redrawn
   unchanged after each internal loading step; the number of such redraws is not significant, as they
   are indistinguishable from a static image.)
4. After the last stage (`st_client_synchronising`) the level runs normally, but on each of the next 30
   rendered frames the loading image (with the last caption) is drawn over the rendered frame, and the
   sound output is muted (section 6). The background is opaque over the whole screen (section 8.1), so the
   level is not visible during these frames. (In these frames the engine also turns the camera through
   a full circle over the 30 frames and adds a temporary light, to warm up rendering; both are hidden by
   the opaque background and have no visible effect.)
5. During those frames, as soon as the level is loaded and at most 2 of the 30 overlay frames remain, the
   new-game intro sequence starts (section 7).
6. After the 30th overlay frame the engine re-applies the display gamma ramp from the user's gamma,
   brightness and contrast settings (to be specified with the video settings; with the default settings
   no change is expected, not verified), sound is un-muted, and the loading image is gone with its
   textures released. The next frame shows the intro movie (or, with `-nointro`, the level from the player's view).

There is no wait for input at any point, no minimum display time, and no fade-in or fade-out: images
replace each other at once and the overlay simply stops being drawn.

## 3. Coordinates of the elements

Element rectangles are defined in the 1024 × 768 UI space and scaled per axis. A half-pixel offset of
−0.5 is part of each rectangle; for the background and the picture it is applied **before** scaling (so it
is scaled too), for the strip **after** scaling (exactly −0.5 pixel). With *s*ₓ, *s*ᵧ as above:

| Element | Screen left … right (x) | Screen top … bottom (y) |
|---|---|---|
| Background | (0 − 0.5)·*s*ₓ … (1024 − 0.5)·*s*ₓ | (0 − 0.5)·*s*ᵧ … (768 − 0.5)·*s*ᵧ |
| Progress strip | 379·*s*ₓ − 0.5 … 647·*s*ₓ − 0.5 | 726·*s*ᵧ − 0.5 … 763·*s*ᵧ − 0.5 |
| Level picture | (257 − 0.5)·*s*ₓ … (769 − 0.5)·*s*ₓ | (369 − 0.5)·*s*ᵧ … (625 − 0.5)·*s*ᵧ |

So in UI units: background 0, 0, 1024 × 768; strip 379, 726, 268 × 37; picture 257, 369, 512 × 256. The
caption is placed in pixels (section 4.4). All quads are pre-transformed screen-space vertices drawn
without depth test or depth write (depth values are irrelevant). Every element is stretched with the
screen at other aspect ratios; there is no wide-screen variant.

## 4. Elements

Draw order within one image: background, strip, caption, picture (the picture is drawn last, on top; the
caption does not overlap it).

### 4.1 Background

- Texture `textures\ui\ui_load.dds` (shader texture name `ui\ui_load`), shader `hud\default`
  (UI-layout specification, section 8: texture × vertex colour, ordinary alpha blending, clamp, bilinear).
- Vertex colour 0xFFFFFFFF at all four corners.
- Texture coordinates: u = 0 … 1024 / *T*w, v = 0 … 768 / *T*h, where *T*w × *T*h is the texture's size
  (1024 × 1024, so u 0 … 1, v 0 … 0.75: the top 1024 × 768 texels). The left/top screen edge gets u/v 0.
- If the background's shader could not be created, the colour buffer is instead cleared to black
  (colour 0; depth is not cleared) and nothing else of the image is drawn. This does not happen with the shipped data.

### 4.2 Progress strip

**Geometry.** A band of 41 vertex columns *i* = 0 … 40, evenly spaced across the strip rectangle, each
with a top and a bottom vertex, forming 40 quads (80 triangles) side by side. With the strip's screen
left *x*₀ = 379·*s*ₓ and right *x*₁ = 647·*s*ₓ (before the −0.5):

- column *i*: x = *x*₀ + *i* · (*x*₁ − *x*₀) / 40 − 0.5; top y = 726·*s*ᵧ − 0.5; bottom y = 763·*s*ᵧ − 0.5;
- texture coordinates: u = (*i* · 268 / 40) / *T*w; top v = 768 / *T*h; bottom v = 805 / *T*h (texels
  x 0 … 268, y 768 … 805 of `ui_load.dds`, the band below the background part);
- texture and shader: the same as the background (`ui\ui_load`, `hud\default`).

**Colour.** Both vertices of column *i* get colour R = G = B = 255 and alpha *a*ᵢ; colour and alpha are
interpolated linearly between columns. The texture is multiplied by it and blended over the background,
so the strip shows the texture band faded by *a*ᵢ / 255:

- *i*′ = *i* if *i* ≤ 20, else 40 − *i* (the profile is mirrored about the centre);
- *c* = 20 · (*n* + 1) / *N*, where *n* is the number of captions begun so far in this loading (1 for the
  first caption) and *N* the expected total (below);
- *t* = 1 / (1 + e^(0.5 · (*i*′ − *c*))) (a logistic step centred at column distance *c* from each end);
- *a*ᵢ = floor(255 · *t*), clamped to 0 … 255.

So the strip fills from **both ends towards the centre** as captions advance, with a soft edge about 8
columns wide; full brightness is 254 (*t* < 1). (Single-precision and double-precision evaluation give the
same integers for every case of this edition.)

**Expected total *N*.** When a caption begins: *N* = 17 if the current game type is single-player and the
program's own command line contains the text `alife` anywhere; otherwise *N* = 14. For New Game from the
menu the command line does not contain `alife`, so ***N* = 14** throughout. (With such a command line, the
very first caption of a run, `st_prefetching_objects`, still uses 14, because the game type becomes
single-player only after it.) With *N* = 14 and 17 captions the strip is practically full from caption 13
on (section 10.4).

### 4.3 Caption text

- The displayed text is the string-table translation of the caption id (text-and-fonts specification,
  section 1.4: a missing key shows the id itself) followed by three full stops `...` (three bytes 0x2E).
  For the connecting stage it is the translation of `st_client_connecting_to`, a space and the client
  address followed by `...` (section 5); that composed text is itself looked up (it is not a key, so it is
  shown unchanged) and then gets the `...`.
- The text is at most 255 bytes (longer is not supported; all shipped captions are ≤ 35 bytes). No
  shipped caption contains `%`; behaviour for one that does is not specified.
- Only the current caption is shown; earlier ones are not listed.

### 4.4 Caption font, colour and position

- Font: section `[ui_font_graffiti19_russian]` of `config\fonts.ltx` (`shader = font`, `texture =
  ui\ui_font_graff_19_1024`, `texture800 = …_800`, `texture1600 = …_1600`), with the resolution-dependent
  variant, metric files and drawing rules of the text-and-fonts specification (sections 2–3). The variant
  is chosen from *H* (≤ 600: `_800`; 601–900: `_1024`; ≥ 901: `_1600`). It has no `size` or `interval`
  key, so the glyph height is the metric file's `height` (17, 19 and 30) and the advance factor is 1.
  Glyphs are not scaled with the screen.
- Colour: 0xFF9D8C78 (A 255, R 157, G 140, B 120) for every glyph.
- Alignment: centred.
- Pen position: *x* = floor(*W* · 0.5), *y* = floor((0.815 + 1) · *H* · 0.5) = floor(0.9075 · *H*),
  computed in single precision in the order shown. With the text-and-fonts rules the first glyph's left
  edge is at floor(*x*) − floor(*S* / 2) (*S* = sum of the glyph widths) and the glyph tops are at *y*.
  Examples: *H* = 600 → *y* = 544; 720 → 653; 768 → 696; 900 → 816; 1024 → 929; 1080 → 980; 1200 → 1089.
- Shader `font` (text-and-fonts specification, section 3.5).

### 4.5 Level picture

- Texture: `textures\intro\intro_<level>.dds`, where `<level>` is the level directory name (without the
  trailing `\`) of the level being loaded, here `textures\intro\intro_l01_escape.dds`. If no file of that
  name exists, `textures\intro\intro_no_start_picture.dds` is used instead.
- Shader `font` (blending as for glyphs; alpha test reference 32), vertex colour 0xFFFFFFFF.
- Texture coordinates: the whole texture, u 0 … 1 left to right, v 0 … 1 top to bottom, stretched over
  the picture rectangle (section 3) whatever the texture size.
- **When it is shown:** from the first image drawn after the level has been determined. On a New Game
  the level is first determined during the work of caption 4 (`st_creating_new_game`): when the game's
  objects are created from `spawns\all.spawn`, the creation of the player object (the record whose spawn
  flags have bit 0x08 set, section 8.4) makes the life simulation select the level of the player's
  game-graph vertex (scoping specification, section 2.2: `l01_escape`), and this selects the picture. So
  the picture is drawn from caption 5 (`st_server_connecting`) onwards, including the 30 overlay frames;
  captions 1–4 are shown without it (the area shows the background). When the client later starts loading
  the level (caption 7) the same picture is selected again; nothing changes on screen. Nothing removes the
  picture before the end of the overlay frames.

## 5. Stage sequence

The captions of a New Game from the title menu, in order. *n* is the caption number used by the strip.
Russian texts are those of `config\text\rus\ui_st_other.xml` with the `...` added.

| *n* | Caption id | Displayed text | What is being done while this caption is shown |
|---|---|---|---|
| 1 | `st_prefetching_objects` | Кэширование объектов... | Pre-creating game objects and loading models for later use (skipped work with `-noprefetch`; the caption is still shown). Only when the game type changes, see below. |
| 2 | `st_server_starting` | Сервер: Старт... | The local server is created for the game type `single`. |
| 3 | `st_loading_alife_simulator` | Сервер: Загрузка симуляции жизни... | The life simulation is set up. |
| 4 | `st_creating_new_game` | Создание новой игры... | A new game is created from `spawns\all.spawn` and `game.graph` (scoping specification, section 2.2); the level and its picture are determined here (section 4.5). |
| 5 | `st_server_connecting` | Сервер: Соединяемся... | The server finishes creating the game. |
| 6 | `st_client_connecting_to` + address | Клиент: Соединение с localhost... | The client connects to the local server. The address is the client parameter of the start command up to its first `/` (`localhost`; at most 63 bytes). |
| 7 | `st_opening_stream` | Открытие потока... | The file `level` is opened (`levels\l01_escape\level.ltx` was read just before, under caption 6). |
| 8 | `st_loading_cform` | Загрузка формы объектов... | `level.cform` (collision). |
| 9 | `st_loading_shaders` | Загрузка шейдеров... | The level's shader list (from `level`). |
| 10 | `st_loading_geometry` | Загрузка геометрии... | `level.geom` (and `level.geomx` with the second renderer). |
| 11 | `st_loading_spatial_db` | Загрузка базы пространств... | The spatial index of the level's static visuals is built. |
| 12 | `st_loading_details` | Загрузка детальных объектов... | `level.details` (if present). |
| 13 | `st_loading_sectors_portals` | Загрузка секторов и порталов... | Sectors, portals, `level.hom`, glows; afterwards `level.env_mod`. |
| 14 | `st_loading_ai_objects` | Загрузка ИИ объектов... | `level.ai` and `level.game` (if present), then the level's objects. |
| 15 | `st_client_spawning` | Клиент: Создание... | The client creates the objects of the game. |
| 16 | `st_loading_textures` | Загрузка текстур... | All texture files needed by the level are read and uploaded. |
| 17 | `st_client_synchronising` | Клиент: Синхронизация... | Last caption; then the 30 overlay frames (section 2, step 4). |

Rules:

- The caption count *n* starts at 0 when loading starts and is never reset during one loading.
- Caption 1 is shown only if the game type of the new game (`single`) differs from the game type of the
  previous game started in this run; the remembered game type is empty at program start and is not
  cleared when a game is left. So the **first** New Game after launch shows 17 captions; a later New Game
  in the same run (after a single-player game) shows 16, starting with `st_server_starting` as *n* = 1, and
  every later caption's *n* is one lower than in the table.
- Captions 10–12 are skipped only on a dedicated server (not applicable here).
- Stages 2–17 take place in this order for every level; the level-dependent work (7–16) depends only on
  which of the files named above exist.
- The time each caption stays on screen is the time its work takes; nothing is shown for a minimum time.

## 6. Audio

- The loading screen plays no sound of its own and no sound files are associated with it.
- During each of the 30 overlay frames (section 2, step 4) the sound system's master (listener) gain is
  set to 0; after the last overlay frame it is set to 1. Everything started during loading or in those
  frames (level ambience, the intro's sound pair) is therefore silent until the overlay disappears.
- What happens to the title-menu music while the loading stages run (it is stopped when the menu screen
  is discarded "a few frames" after closing, title-menu specification section 2, and no regular frames
  are processed during the stages) is **not established**.

## 7. The intro on New Game

- While the overlay frames run, a per-frame check starts the intro as soon as all of these hold: the
  command line does not contain `-nointro`; a level is loaded; at most 2 overlay frames remain; and the
  fourth server parameter is `new` (compared case-insensitively). The check then stops; it is made once
  per started game.
- It starts the movie sequence `<intro_game>` of `config\ui\ui_movies.xml` (scoping specification,
  section 2.3; startup specification for the sequence semantics and skipping). Its first frames are under
  the last (at most 2) overlay frames and its sound starts muted for those frames.
- With `-nointro`, no intro is started and the first frame after the overlay shows the level.
- With the command-line switch `-gpu_ref` the overlay frames are disabled (0 instead of 30), so the
  loading image disappears with the last stage.

## 8. Data facts

### 8.1 `textures\ui\ui_load.dds`

db8, 1,048,704 bytes, DDS 1024 × 1024 DXT5, one mip level, SHA-256
`55aeb78a97c7d9472b868fd7a24d0217b1cbd4066ba51d51f9f9aa15ccd82ad9`.

| Texel region (x, y, width × height) | Alpha | Use |
|---|---|---|
| 0, 0, 1024 × 768 | 255 everywhere | background (opaque) |
| 0, 768, 268 × 37 | 0 … 255, mean ≈ 134.1 ± 0.1 | strip band |
| 268, 768, 756 × 256 and 0, 805, 268 × 219 | 0 everywhere (RGB 0) | unused |

(Mean RGB of the background region ≈ (31.7, 27.3, 21.4); of the strip band ≈ (61.5, 41.1, 24.1), ±0.5.
Values from a standard DXT5 decoder: colour endpoints expanded from 5:6:5 by bit replication, colour
interpolants (2·c0 + c1) / 3 and (c0 + 2·c1) / 3; alpha interpolants ((7 − k)·a0 + k·a1) / 7 for k = 1 … 6
when a0 > a1, else ((5 − k)·a0 + k·a1) / 5 for k = 1 … 4 plus 0 and 255; integer division truncating. The
strip-band alpha mean is 134.07 with truncation and 134.10 with exact or rounded interpolation.)

### 8.2 Level pictures

- 30 directories under `levels\` contain all four of `level`, `level.ltx`, `level.geom`, `level.cform`
  (the loadable levels, scoping specification section 2.2). Every one has its
  `textures\intro\intro_<name>.dds` (all in db6), so the fallback `intro_no_start_picture` is never used
  in this edition.
- All 30 pictures are 512 × 256 with one mip level: 25 DXT1 (65,664 bytes) and 5 DXT3 (131,200 bytes:
  `testers_mp_darkvalley`, `testers_mp_lost_village`, `testers_mp_pool`, `testers_mp_railroad`,
  `testers_mp_rostok`).
- `textures\intro\intro_l01_escape.dds`: db6, 65,664 bytes, 512 × 256 DXT1, fully opaque, SHA-256
  `6b57bedbdbfad5c2512cb2700e4fdcbdd86a2e0ead61da82b7858dbf8aa52e5b`.
- `textures\intro\intro_no_start_picture.dds`: db6, 65,664 bytes, 512 × 256 DXT1, fully opaque, SHA-256
  `55272cc6b7a00bf294c9e55890446f0e83f32b024a0a36a8afc69c2013bbfe62`.
- Other `textures\intro\intro_*.dds` files (`intro_back`, `intro_fog_net`, `intro_k01_darkscape`,
  `intro_l06_rostok_demo`, `intro_l09_deadcity`, `intro_map_pic_*`, …) belong to no loadable level and are
  not used by the loading screen of New Game.

### 8.3 Strings

All 17 caption ids are in `config\text\rus\ui_st_other.xml` (dbb, 6,609 bytes) and nowhere else in the
string tables. The file also holds `st_loading_saved_game` (Загрузка игры) and `st_validating_cdkey`, which
New Game does not show.

### 8.4 The player object in `spawns\all.spawn`

Of the 8,641 spawn records (start-position specification, section 3), exactly one has bit 0x08 set in its
spawn-record flags: the actor (index 63, section `actor`, flags `0x0029`); all others have `0x0021`
(8,629) or `0x0023` (11). Its game-graph vertex 4 lies on level id 1, `l01_escape`.

## 9. Corrections to the scoping specification

- The strip has 41 vertex **columns** forming 40 cells, not "41 segments", and it fills from both ends
  towards the centre (section 4.2).
- The caption is followed by `...` (section 4.3).
- The positions of the two captions left open there are established: `st_prefetching_objects` is the
  first caption (only when the game type changes), `st_client_connecting_to` comes right after
  `st_server_connecting` and carries the client address.
- "Precaches for 30 frames before the loading screen ends" means: the loading image is drawn over the
  first 30 rendered frames of the running level with sound muted (section 2).
- The intro starts when at most 2 of those frames remain (section 7), rather than "within the first three
  rendered frames".

## 10. Verification

### 10.1 Rectangles (Direct3D 9 convention, section 3)

| *W* × *H* | Background x / y | Strip x / y (column step) | Picture x / y |
|---|---|---|---|
| 800 × 600 | −0.390625 … 799.609375 / −0.390625 … 599.609375 | 295.59375 … 504.96875 / 566.6875 … 595.59375 (5.234375) | 200.390625 … 600.390625 / 287.890625 … 487.890625 |
| 1024 × 768 | −0.5 … 1023.5 / −0.5 … 767.5 | 378.5 … 646.5 / 725.5 … 762.5 (6.7) | 256.5 … 768.5 / 368.5 … 624.5 |
| 1280 × 1024 | −0.625 … 1279.375 / −0.6667 … 1023.3333 | 473.25 … 808.25 / 967.5 … 1016.8333 (8.375) | 320.625 … 960.625 / 491.3333 … 832.6667 |
| 1920 × 1080 | −0.9375 … 1919.0625 / −0.703125 … 1079.296875 | 710.125 … 1212.625 / 1020.4375 … 1072.46875 (12.5625) | 480.9375 … 1440.9375 / 518.203125 … 878.203125 |

### 10.2 Captions

Width *S* (pixels) and first-glyph left edge *x*₀ for the 17 captions of section 5 with the metric files
`textures\ui\ui_font_graff_19_<variant>.ini` (db7); pen (*x*, *y*) per resolution in the header.

| *n* | Bytes | 800 × 600 `_800` (400, 544): *S* / *x*₀ | 1024 × 768 `_1024` (512, 696): *S* / *x*₀ | 1920 × 1080 `_1600` (960, 980): *S* / *x*₀ |
|---|---|---|---|---|
| 1 | 23 | 154 / 323 | 176 / 424 | 257 / 832 |
| 2 | 16 | 98 / 351 | 113 / 456 | 165 / 878 |
| 3 | 35 | 230 / 285 | 265 / 380 | 388 / 766 |
| 4 | 22 | 145 / 328 | 168 / 428 | 241 / 840 |
| 5 | 22 | 142 / 329 | 165 / 430 | 239 / 841 |
| 6 | 33 | 208 / 296 | 240 / 392 | 344 / 788 |
| 7 | 18 | 116 / 342 | 129 / 448 | 194 / 863 |
| 8 | 26 | 175 / 313 | 198 / 413 | 291 / 815 |
| 9 | 20 | 130 / 335 | 148 / 438 | 218 / 851 |
| 10 | 21 | 134 / 333 | 150 / 437 | 222 / 849 |
| 11 | 28 | 181 / 310 | 200 / 412 | 299 / 811 |
| 12 | 30 | 198 / 301 | 223 / 401 | 330 / 795 |
| 13 | 31 | 202 / 299 | 228 / 398 | 334 / 793 |
| 14 | 23 | 151 / 325 | 171 / 427 | 249 / 836 |
| 15 | 19 | 120 / 340 | 138 / 443 | 200 / 860 |
| 16 | 19 | 117 / 342 | 129 / 448 | 195 / 863 |
| 17 | 24 | 154 / 323 | 177 / 424 | 258 / 831 |

Glyph heights 17 / 19 / 30. At 1280 × 1024 the pen is (640, 929) with the `_1600` variant (same *S* as
1920 × 1080).

### 10.3 Strip alpha, *N* = 14

*a* for *i*′ = 0 … 20 (columns *i* and 40 − *i* are equal) and Σ = sum over all 41 columns:

| *n* | *a*(0) … *a*(20) | Σ |
|---|---|---|
| 0 | 171 141 109 79 55 36 23 14 9 5 3 2 1 0 0 0 0 0 0 0 0 | 1296 |
| 1 | 205 182 154 122 92 65 43 28 18 11 6 4 2 1 0 0 0 0 0 0 0 | 1866 |
| 2 | 228 213 193 167 136 104 75 52 34 22 13 8 5 3 1 1 0 0 0 0 0 | 2510 |
| 5 | 251 249 245 240 231 218 199 175 145 113 83 58 38 25 15 9 6 3 2 1 0 | 4612 |
| 7 | 254 253 252 251 248 245 239 229 216 196 171 141 109 79 55 36 23 14 9 5 3 | 6053 |
| 10 | 254 254 254 254 254 253 253 251 249 246 241 232 220 202 179 150 118 87 61 41 26 | 8132 |
| 14 | 254 254 254 254 254 254 254 254 254 254 254 253 252 251 248 245 239 229 216 196 171 | 10017 |
| 17 | 254 254 254 254 254 254 254 254 254 254 254 254 254 254 254 253 253 251 249 246 241 | 10365 |

Σ for *n* = 0 … 17: 1296, 1866, 2510, 3190, 3898, 4612, 5333, 6053, 6764, 7465, 8132, 8753, 9289, 9715,
10017, 10203, 10312, 10365. With *N* = 17: Σ for *n* = 1 is 1656, for *n* = 17 is 9971.

### 10.4 Invariants

- Over all 30 loadable levels the picture rectangle, texture size (512 × 256) and caption sequence are the
  same; only the picture differs.
- The caption sequence of a first New Game has 17 entries; *n* reaches 17 = *N* + 3 with *N* = 14.

## 11. Input during loading

- Nothing reacts to input on the loading screen: it cannot be skipped, cancelled or paused, and it waits
  for no key. Keyboard, mouse and gamepad are not read while the loading stages run (the window's system
  messages are still handled between stages).
- During the 30 overlay frames the level is running and receives input under the normal game rules (out
  of scope); the intro, once started, is skippable as specified for movie sequences.
- Touch and pointer input have no loading-screen function either, so desktop and mobile behave alike.

## 12. Not established

- Visible effect of the video-device reset when loading starts.
- Title-menu music behaviour during the loading stages (section 6).
- Whether keys or buttons pressed during the loading stages are delivered to the game after loading
  (section 11).
- How a `%` in a caption would be shown.
- What the one ordinary frame presented right after the first caption's processing shows (section 2,
  step 2): the menu is closed and the level is not yet loaded at that point.

## Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrRender_R1.dll`,
  `bin\xrRender_R2.dll`, `bin\xrSound.dll` (32-bit Windows).
- Data examined: `textures\ui\ui_load.dds`, all `textures\intro\intro_*.dds` of the 30 loadable levels and
  `intro_no_start_picture.dds` (headers parsed, DXT blocks decoded, regions measured, SHA-256 of the files),
  `config\fonts.ltx`, `config\text\rus\ui_st_other.xml`, `textures\ui\ui_font_graff_19_{800,1024,1600}.ini`,
  the merged archive listing (level directories and picture existence). Caption widths, positions,
  rectangles and strip values were recomputed from this document's rules with a throwaway program.
- Binaries: static analysis (Ghidra 12.2, headless) of the loading-screen drawing, the caption
  formatting, the stage order and conditions, the overlay-frame count and its sound muting, the intro
  start condition and the font and texture selection. Constants (rectangles, colour, totals, curve
  parameters, frame count) were read from the binaries' data. No execution or debugging was performed.
- No leaked source code, derived projects or third-party documentation of the engine were used. Public
  references: DDS/DXT block compression, the Direct3D 9 pixel-centre convention, OpenAL listener gain.
