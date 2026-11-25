# Save picture

Functional specification of the picture S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) stores next
to a saved game and shows on the load screen: which frame it holds and what is drawn in it, the colour
correction applied, how the frame is reduced to 128 × 128 and stored, what happens to an older picture when
a save writes none, and how the load screen shows a picture, including one rewritten during the session.

Built on the approved specifications, which are not repeated here:

- `docs/formats/sav.md` (section 1.1: which saves write a picture; section 13: the picture file);
- `docs/behavior/save-load-screens.md` (section 5.2: the load screen's picture element; 8.3: the save
  message; 8.4: the menu during a capture; 10: automatic saves; 12: lifetime of the screens);
- `docs/formats/ui-layout.md` (section 6.3: how a texture path is resolved);
- `docs/formats/ltx-config.md` (the `user.ltx` console lines), `docs/formats/db-archive.md`.

Scope (proposal SL-1 of `docs/behavior/narrowed-specs-scope.md`): whether the HUD and the save message
are in the captured frame; the gamma applied; how the frame is reduced to 128 × 128; what happens to an
older picture when a save writes none; whether a picture rewritten under a name already shown in the
session is shown fresh. **Settles:** sav.md section 13 (believed: an older `<user>_autosave.dds` is left as
it is) and save-load-screens section 14 (whether the HUD and the save message appear in the picture;
whether a rewritten picture is shown fresh).

Conventions: the original game was not run. Every statement is **static** (read from the shipped binaries
or data) unless marked **documented** (taken from Microsoft's public Direct3D 9 / D3DX 9 documentation),
**believed** (inferred, not traced to the end) or **unestablished**. The bracketed ids name the check that
backs a statement: `B…` in `bin.py`, `D…` in `data.py`, `L…` in `lut.py`, all in the dirty workspace's
`checks/save-picture/`; `run.py` regenerates the analysis dumps (`ghidra.py`) and runs all three.
Statements taken from an approved specification name its section instead. Colours are 8-bit channels
R, G, B, A.

## 1. Which saves write a picture

As sav.md section 1.1 and 13: the console command `save` (the save screen and quick save) and the
periodic automatic save request a picture; the level-change save and the New Game save do not. The
request names the file `$game_saves$` + save name + `.dds` [B18, B19].

Order within the request (static):

- `save`: the save is handed to the simulation, then the HUD message `game_saved` (save-load-screens
  8.3) is added with an end time 3.0 s after the current value of the engine's global clock, then the
  picture is requested [B18]. The layout entry has no `ttl` attribute, so 3.0 s applies [D5].
- periodic automatic save: the save is handed to the simulation, the picture is requested, then the HUD
  message `autosave` is added with the same 3.0 s end time [B19, D5].

A request replaces any request still pending: the pending mark is set again, the stored file name and
frame number are overwritten, with no test for an earlier request [B20]. If two saves are requested before
the first capture is made, only the last named file receives a picture; the earlier save gets none and
keeps whatever older picture it had (section 5).

## 2. When the frame is captured

- Let *F* be the value of the engine's frame counter when the picture is requested. The counter is
  incremented by one at the start of every frame, before the frame's per-frame updates; the frame is drawn
  and presented after them [B14, B15].
- The request stores *F* + 1 [B20]. In each frame's per-frame update (also while the menu is closed, as quick and automatic saves have pictures, sav.md 13) the game checks for a pending
  request whose stored value is less than the current counter; the first frame that passes is *F* + 2, and
  the capture runs there, before that frame is drawn [B21]. **Disputed in review:** that the per-frame check runs in every frame while the menu is closed is not backed by a check; it is inferred from quick and automatic saves having pictures (sav.md 13). Treat it as believed. A capture therefore copies what is on screen
  after frame *F* + 1 was presented: the image of frame *F* + 1 (**believed**: the copy reads the
  device's front buffer, which holds the most recently presented image (**documented**); a display queue
  that delays presentation would give an older frame).
- When the capture runs it clears the pending mark first; it is made once per request, whether or not a
  file results [B21].
- **Full-screen only.** If the display is not in full-screen mode (console variable `rs_fullscreen`, bit 1
  of the device flags, `rs_fullscreen on` in the default `user.ltx`), the capture writes nothing and logs
  `~ Can't capture screen while in windowed mode...` [B1, B13, D6]. This is sav.md section 13; the save
  itself is made.
- Both shipped renderers (`xrRender_R1.dll`, `xrRender_R2.dll`) make the capture identically [B5].

## 3. What the frame contains

The picture is the **whole frame as presented**: every layer drawn in frame *F* + 1, at the render
resolution, with nothing removed or added for the capture, except the following changes the request makes
for the frames in between.

- **Console:** the request remembers whether the console was shown and hides it; the capture shows it again
  if it was shown [B20, B21]. The console is never in the picture.
- **Menu open (a save from the save screen):** if the menu is shown when the request is made, the game view
  is drawn again for the frames until the capture (save-load-screens 8.4): the level's per-frame update and
  its drawing are re-enabled, the menu draws nothing at all, and after the capture the level's update and
  drawing are disabled again [B20, B21, B22]. While the menu is open the game is paused (save-load-screens
  4.3); the paused game draws its pause text only when the pause-text flag is set, and opening the menu in a
  single-player game clears that flag, so no pause text is in the picture [B24, B26].
- **HUD:** the level's drawing is the 3D view followed by the HUD interface [B23]. So the picture shows the
  HUD exactly as it is drawn in that frame; what the HUD shows is the HUD specification's (#101), not this
  one's.
- **Save message:** `game_saved` ("игра сохранена: <name>", save-load-screens 8.3) and `autosave`
  ("Автосохранение...", save-load-screens 10) belong to the HUD's game-specific message layer, which is
  drawn only while the HUD indicators are shown; the script functions `hide_indicators` and
  `show_indicators` (called by shipped scripts, e.g. `scripts\avi_start.script`,
  `scripts\xr_effects.script`) turn them off and on [B24, D7]. Both messages are added before the capture
  frame is drawn (section 1) and last 3.0 s of the global clock, which does not advance while the game is
  paused (save-load-screens 8.3). Consequently:
  - quick save (F6) and the save screen: the picture shows `игра сохранена: <name>` when the indicators are
    shown;
  - periodic automatic save: the picture shows `Автосохранение...` when the indicators are shown;
  - with the indicators hidden, neither message nor the rest of that layer is in the picture.
  (**believed**: the message could be missing if more than 3.0 s of the global clock passed between the
  request and the drawing of frame *F* + 1, e.g. if writing the save in between took that long; whether the
  save is written before frame *F* + 1 is drawn is **unestablished**.)
- **Mouse cursor:** the cursor is drawn by a drawing step of its own, after the level and the menu (drawing
  steps run in descending priority: the level's step has the normal priority, the cursor's a lower one),
  in every frame in which it is visible; the menu's drawing has a priority between the two [B16, B25, B32]. The capture does not hide it. So whenever the cursor is
  visible — in particular for a save from the save screen, where the menu shows it — it is in the picture
  at its position on screen (static; that the menu leaves the cursor visible for the capture frames is
  **believed**: no code hiding it during a capture was found).
- **Other in-game windows** open at that moment (they are part of the HUD interface) are in the picture as
  drawn.

## 4. From frame to file

1. **Copy.** The presented image is copied at the render resolution (width × height of the game's display
   mode) into 32-bit pixels with 8-bit A, R, G, B [B2] (**documented**: the copy is converted to that
   format).
2. **Colour correction.** Every pixel's R, G and B are replaced through the game's colour table (below);
   A is set to 255 [B3]. The same table is the one the game loads into the display's hardware gamma ramp
   [B12], so the picture shows the colours the player sees. (**believed**: the copy itself does not include
   the hardware ramp; that is why the game applies the table.) The pass treats the copy as width × height
   contiguous pixels (**believed**: the copy has no row padding at the usual sizes; with padding, the last
   rows would be left uncorrected).
3. **Reduction and compression.** The corrected image is converted in one step into a 128 × 128 image in
   DXT1 (BC1) block compression with a single mip level, regardless of the frame's aspect ratio, by the
   D3DX 9 library (`d3dx9_34.dll`) with its default filter and no colour key [B4, B6]
   (**documented**: the default filter is the triangle filter with dithering; the library converts to
   compressed formats). The exact filter weights, the dithering and the DXT1 encoder's choices are those of
   that library and are **unestablished**; a reimplementation that resamples with a triangle (tent) filter
   over the source area of each destination pixel and encodes all blocks opaque meets the description but
   is not bit-identical. All source pixels are opaque, so no block needs transparency.
4. **File.** The texture is stored as a DirectDraw Surface (`.dds`, public format) by the same library and
   written to `$game_saves$` + name + `.dds`, replacing any file of that name [B4]. Header flags written by
   the library are **unestablished**; a 128 × 128 DXT1 surface with one level has 8 192 bytes of block data
   (32 × 32 blocks of 8 bytes), and with the 4-byte signature and 124-byte header the file is
   **believed** to be 8 320 bytes (save-load-screens 13 item 8).
5. If any step fails (the copy, the texture creation, the conversion, the encoding), no file is written and
   no error is shown [B30].

### 4.1 Colour table

From the three display settings *gamma*, *brightness* and *contrast* (console variables `rs_c_gamma`,
`rs_c_brightness`, `rs_c_contrast`, each 0.5 … 1.5, default 1.0; the default `user.ltx` has `1.` for each)
[B9, B11, D6] and a per-channel balance that is always 1.0 (no shipped code changes it) [B29], the game
computes for every channel *c* and every input value *i* = 0 … 255 a 16-bit entry:

- *g* = 1 / (*gamma* + 0.00001);
- *v* = (*i* · (1/255))^*g* · (*contrast* · 0.5 + 0.5) · 65535 + (*brightness* · 0.5 − 0.5) · 32768 −
  *contrast* · 0.5 · 32768 + 16384, rounded to a 32-bit float;
- entry = *v* · balance, rounded down to an integer and clamped to 0 … 65535 [B8, B10].

The picture uses the high byte of each entry: output = ⌊entry / 256⌋ [B3]. At the defaults (1.0, 1.0,
1.0) the table is the identity (output = input for all 256 values) [L1], so a default-configured game
stores the frame's colours unchanged. Test vectors (same for R, G and B; inputs 0, 64, 128, 192, 255)
[L2]:

| gamma, brightness, contrast | outputs |
|---|---|
| 1.5, 1.0, 1.0 | 0, 101, 161, 211, 255 |
| 0.5, 1.0, 1.0 | 0, 16, 64, 145, 255 |
| 1.0, 1.5, 1.0 | 32, 96, 160, 224, 255 |
| 1.0, 0.5, 1.0 | 0, 32, 96, 160, 223 |
| 1.0, 1.0, 1.5 | 0, 48, 128, 208, 255 |
| 1.0, 1.0, 0.5 | 32, 80, 128, 176, 223 |

## 5. Saves that write no picture

No picture is written by the level-change save, the New Game save (sav.md 1.1, 13), a save made in a
windowed mode (section 2), a save whose request was replaced (section 1), or when a capture step fails
(section 4). In every case the existing `<name>.dds`, if any, is left as it is: the only code that removes
a picture is the save and load screens' delete action, which removes `<name>.sav` and then `<name>.dds` if
it exists [D2, D3]; no other code of the game, the engine or the renderers removes or renames a file in the
saved-games folder [B28]. Consequences:

- after a level change, `<user>_autosave.sav` is the level-change save and `<user>_autosave.dds`, if
  present, is the picture of the last periodic automatic save (settles sav.md 13's believed statement,
  static);
- a save made under an existing name in a windowed mode keeps the old save's picture;
- `all.dds` never exists unless placed there by other means.

## 6. How the load screen shows a picture

The element and its placement are those of save-load-screens 5.2; the screen sets the element's texture to
`<N>` when `<N>.dds` exists and to `ui\ui_noise` otherwise [D1].

- **Resolution.** The texture name is resolved as in ui-layout 6.3 (current level's folder, then the
  saved-games folder, then `textures\`).
- **No reduction.** The texture-quality setting (`texture_lod`, and the list `[reduce_lod_texture_list]`)
  only drops mip levels while more than one level remains, so a one-level picture is always loaded at
  128 × 128 [B7]. A file with a different size or format is loaded as stored (the loader keeps the file's
  dimensions and format) [B31].
- **Textures are shared by name.** Loading a texture whose name matches a texture currently loaded returns
  that texture without reading the file again; a texture is unloaded the moment nothing refers to it any
  more [B17]. Setting the element's texture always builds the element's new drawing state from the name and
  releases the previous one only afterwards, with no comparison of the names [B17, B27].

**Rewritten pictures.** Therefore `<N>.dds` is read from disk when the element selects `<N>` while no
texture named `<N>` is loaded, and is not read again while one is. A picture rewritten under a name is
shown fresh unless the texture of that name is still in use at the moment the element selects it:

- Example (static): within one menu opening, the player opens the load screen and clicks the row `moy_sejv`
  (its picture is shown), returns to the menu, saves under `Moy_Sejv` from the save screen (the menu stays
  open, save-load-screens 4.3), and opens the load screen again. The load screen is the same one, its
  element still holds the texture `moy_sejv` (save-load-screens 12), so clicking `moy_sejv` again shows the
  **old** picture. Clicking another row first releases `moy_sejv` (if nothing else holds it), and clicking
  `moy_sejv` afterwards shows the new picture.
- Across menu openings the screens are rebuilt (save-load-screens 12) and the element's texture is released
  with the old screen, so the next opening shows the current file (**believed**: nothing else in the game
  refers to a texture named after a save).

## 7. Verification facts

1. Colour table at the default settings is the identity; the vectors of section 4.1 [L1, L2].
2. Default `user.ltx`: `rs_c_gamma 1.`, `rs_c_brightness 1.`, `rs_c_contrast 1.`, `rs_fullscreen on`
   [D6].
3. `config\ui\ui_custom_msgs.xml`: `game_saved` and `autosave` have no `ttl`; their messages last 3.0 s
   [D5, B18, B19].
4. `scripts\ui_load_dialog.script` deletes `<N>.dds` together with `<N>.sav`, and is the only shipped
   script that deletes files [D2, D3].
5. A picture written by the game: 128 × 128, DXT1, one mip level (sav.md 13, [B4]); 8 192 bytes of block
   data; file size believed 8 320.
6. Request in frame *F*, capture at the start of frame *F* + 2 of the image of frame *F* + 1 [B20, B21].

## 8. Unestablished

- The exact pixels: the D3DX 9 triangle filter's weights and dithering and its DXT1 encoder; the DDS header
  flags written.
- Whether the copied frame can be older than *F* + 1 (display queue), and whether it includes the hardware
  gamma ramp (believed not).
- Whether the save is written before frame *F* + 1 is drawn, and so whether a slow save can remove the save
  message before the capture.
- That the cursor stays visible during a capture from the menu (believed).

## Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives `gamedata.db0` …
  `gamedata.dbd`, `__support\userdocs\user.ltx`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`,
  `bin\xrRender_R1.dll`, `bin\xrRender_R2.dll` (SHA-256 prefixes `c5d0008f`, `60f6b66b`, `1fe7a5bc`,
  `2a91c9bb`).
- Analysis: static analysis with Ghidra 12.2 (headless, disassembly and decompilation) of the save and
  automatic-save commands, the picture request and its per-frame handling, the frame loop, the menu's
  drawing, the level's and the HUD's drawing, the cursor, the colour-correction table and its console
  variables, the renderers' frame capture and texture loading, the engine's texture sharing and release,
  and every file-deletion call in the four binaries. Reading of `config\ui\ui_custom_msgs.xml`, the shipped
  scripts and `user.ltx`, extracted with the project's archive tool. The colour table was recomputed from
  the text of section 4.1 by a throwaway program (check `lut.py`). The binaries were not executed; no saved
  game or picture produced by the game was available.
- Public documentation: Microsoft Learn pages for the Direct3D 9 front-buffer copy, the gamma ramp and the
  D3DX surface conversion (default filter, colour key).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What the load screen's picture element shows when `<N>.dds` exists but cannot be loaded as a texture (damaged or not a DDS file).
