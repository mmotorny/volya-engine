# Title menu

Functional specification of the main menu that S.T.A.L.K.E.R.: Shadow of Chernobyl shows after
startup ("title menu"): what is seen and heard, which data files supply each element, where elements
sit on screen, how the player navigates it, and what each item does at the level of "opens screen X".
Other screens (load, options, multiplayer, credits, message boxes), the intro movies, and the generic
formats involved (UI layout XML, texture descriptions, string tables, fonts, LTX, Ogg Theora/Vorbis
playback, the sound system) are specified elsewhere; this document states only what the menu needs
from them.

All file paths are paths in the combined game-data tree (see the archive specification). Where a file
exists in several archives, the copy that wins under archive precedence is the one described; the
archive holding it is noted in section 12. Text files are Windows-1251.

## 1. Overview

- **Technique.** The title menu is entirely **2D**: a stack of textured rectangles and text in a fixed
  virtual UI space, plus two **looping Ogg Theora videos used as textures** (a storm/power-plant
  panorama and a camp fire). No 3D scene is rendered behind it: on the title screen no game level
  exists; when the menu is opened during a game, the level is removed from rendering while the menu is
  shown.
- **Virtual space.** Layout coordinates are in a 1024 × 768 unit space with the origin at the top-left,
  x to the right and y downward. The menu window covers the whole space (0, 0, 1024, 768).
- **Selection widget.** The menu items are rows of plain black text drawn on metal slats painted into
  the background. The current item is marked by a sliding metal bar with a translucent glass window
  (flanked by two cog wheels that turn as the bar slides). The glass window is also a **lens**: it
  magnifies whatever lies under it, so the selected caption appears larger than the others (section
  4.6). There is no per-item hover, pressed or disabled visual; all feedback is the bar's position and
  the lens that moves with it.
- **Audio.** One music track (a stereo pair of mono files) plays while the menu is open. The data
  configures no interface sounds for the menu.

## 2. When the menu appears and disappears

- **After startup.** On the first rendered frame the game plays the startup movie sequence defined by
  the `<intro_logo>` block of `config\ui\ui_movies.xml` (specified elsewhere). When that sequence has
  finished, the title menu opens. This happens only when no level is loaded (and not on a dedicated
  server).
- **`-nointro` command-line switch.** If the command line contains `-nointro`, the sequence is
  skipped: the console is shown and the menu opens at once (opening the menu hides the console; it is
  shown again when the menu closes, see below).
- **Console commands.** `main_menu on` opens the menu, `main_menu off` closes it (other data, e.g.
  `scripts\xr_effects.script`, issues `main_menu on` as well).
- **What opening does (observable).**
  - Every sound that is already playing is paused (game sounds); the menu's own sounds, created after
    this, play normally.
  - If a level is running and the game was not already paused, the game time is paused as well, and the
    usual "pause" caption is suppressed while the menu is up (restored afterwards).
  - The console is hidden; whether it was visible is remembered and restored on close.
  - The mouse cursor becomes visible (section 7); its previous visibility is restored on close.
  - The menu screen (sections 3–5) is built anew each time the menu opens, so the item list
    (section 4.1) is chosen and the music (section 6) starts at every opening.
- **What closing does.** The pauses are released, the console/cursor states are restored, and the menu
  screen is discarded a few frames later; discarding it stops the menu music. If the video mode was
  changed while the menu was open, the video subsystem is restarted on close (console command
  `vid_restart`).

## 3. Visual layers

Element names in this section are the element names of `config\ui\ui_mm_main.xml`; coordinates are
in UI units. Elements are drawn in the order listed (later ones on top).

| # | Element | Rectangle (x, y, w, h) | Content |
|---|---|---|---|
| 1 | `back_movie` | 148, 0, 876, 400 | video `textures\ui\ui_mm_main_back_crop.ogm`, texture rectangle (0, 0, 876, 400) |
| 2 | `background` | 0, 0, 1024, 768 | texture id `ui_menu_backgraund` → `textures\ui\ui_mainmenu.dds`, pixels (0, 0)–(1024, 768) |
| 3 | `fire_movie` | 525, 459, 128, 256 | video `textures\ui\ui_mm_fire.ogm`, whole frame |
| 4 | `shniaga_wnd` | 590, 419, 392, 280 | the item list and the sliding bar (section 4) |
| 5 | `static_version` | 15, 740, 100, 30 | version caption (section 3.4) |

The XML file also contains a `menu_sound` block (section 6) and the item-list blocks (section 4.1).

### 3.1 Background picture

`textures\ui\ui_mainmenu.dds` is a 1024 × 1024 DXT5 texture. Its texture description
`config\ui\ui_mainmenu.xml` (listed in `[texture_desc] files` of `config\system.ltx`) names the file
`ui\ui_mainMenu` (case-insensitive) and defines the regions used by the menu:

| Texture id | x, y, w, h (pixels) | Use |
|---|---|---|
| `ui_menu_backgraund` | 0, 0, 1024, 768 | full-screen background |
| `ui_menu_move` | 7, 951, 392, 73 | the sliding bar |
| `ui_menu_bigroll_l` | 723, 878, 86, 86 | left cog wheel |
| `ui_menu_bigroll_r` | 635, 878, 86, 86 | right cog wheel |
| `ui_menu_grating_l` | 865, 878, 46, 70 | left grating |
| `ui_menu_grating_r` | 810, 878, 55, 67 | right grating |

The file also defines `ui_menu_roll_L_01`…`_20`, `ui_menu_roll_R_01`…`_20` (55 × 55 cells) and
`ui_menu_smallroll_l/_r`; the title menu does not use them.

The background picture shows a brick wall with the game logo, a rifle and backpack on the left, a
camp fire near the centre, and on the right a rack of nine horizontal metal slats (roughly x 700–860,
y 415–700) on which the item captions appear. Its alpha channel is fully opaque except an irregular
sky region in the upper right (approximately x ≥ 650 and y ≤ 330, plus a faint area around
x 450–650, y < 80). That region lies inside `back_movie`, so the panorama video shows through it as
the sky. Everything outside `back_movie` is covered by the opaque background.

### 3.2 Videos

Both videos are Ogg Theora streams used as animated textures (generic behaviour, specified with the
texture system; what the menu relies on):

- A texture name with a `.ogm` file in `textures\` is a video; it is preferred over `.avi`, `.seq`
  and still images of the same name. If a companion `<name>#alpha.ogm` exists it supplies the alpha
  channel; neither menu video has one, so both are opaque.
- The texture is allocated at the next power of two in each dimension and the frame occupies its
  top-left corner (876 × 400 in a 1024 × 512 texture for the panorama; 128 × 256 for the fire). The
  `back_movie` texture rectangle (0, 0, 876, 400) therefore maps exactly to the picture. The panorama's coded
  Theora frame is 880 × 400 with an 876 × 400 picture; the texture receives the leftmost 876 columns. Which
  part of a coded frame becomes the texture, what the uncovered texels hold, and the Y′CbCr → RGB conversion
  are specified in `docs/behavior/startup.md` §6.6; the sizes in the table below are picture sizes.
- Playback starts when the texture is loaded, runs in real time on the engine's continuous
  millisecond clock (the same clock the sliding bar uses; inferred not to stop while the game is
  paused, since the menu animates while it pauses the game) and **loops** indefinitely: when the elapsed time
  reaches the stream length, it restarts from the first frame.

| File | Size | Rate | Frames | Length | Content |
|---|---|---|---|---|---|
| `textures\ui\ui_mm_main_back_crop.ogm` | 876 × 400 | 24 fps | 110 | 4.583 s | rainy storm sky over the power plant, fire on the reactor building |
| `textures\ui\ui_mm_fire.ogm` | 128 × 256 | 33 fps | 118 | 3.576 s | the camp fire and the wall/pipe around it |

The fire video is drawn over the background (it is opaque and matches the painted scene around it),
animating the fire painted at that spot.

### 3.3 Item list and sliding bar

See section 4.

### 3.4 Version caption

`static_version` (15, 740, 100, 30) shows the text `ver. ` followed by the game version string, left
aligned (`align="l"`), font `graffiti19` (section 5), colour RGB (135, 123, 116), opaque. The text is set
once, when the title-menu window is built (`ui_main_menu.script`: `"ver. " .. mm:GetGSVer()`), and is
always `ver. 1.0006`: the game's version string is the built-in `1.0006`, whatever the registry holds.

Each time the version string is produced, the GameSpy library (`bin\xrGameSpy.dll`) opens the Windows
registry key `HKEY_LOCAL_MACHINE\Software\GSC Game World\STALKER-SHOC\` for reading and queries its value
`InstallVers` (up to 128 bytes, any value type), but the data read is not used: the key's or value's
presence, absence, type and content do not change the string. The key is opened without a registry-view
flag, so on 64-bit Windows the 32-bit program reads the 32-bit (`WOW6432Node`) view (Windows' documented
registry redirection). Observed under Wine (run of the unmodified game, 2026-11-26): with `InstallVers` =
`2.1.0.10` (string) in the 32-bit view the caption reads `ver. 1.0006`.

The GameSpy library is loaded with the game's main-menu component; if it cannot be loaded, or lacks the
version function, the game stops with a fatal error. There is no registry-dependent case to reproduce.

Checks (dirty workspace): `titlever/scripts/check.py` (the binaries and the script line),
`titlever/scripts/check_run.py` (the recorded run; the caption read from its frames).

## 4. The item list (`shniaga_wnd`)

`shniaga_wnd` is a 392 × 280 window at (590, 419). It contains, in drawing order:

1. **Item region** `buttons_region` at (110, 0) relative to the window, 164 × 280, a vertical list
   without visible scroll bar (`always_show_scroll="false"`; the list never exceeds the region). All
   list indents and the inter-item gap are 0.
2. **Bar** `shniaga` at (9, *y*) relative to the window, 392 × 73, texture `ui_menu_move`, drawn on
   top of the item captions. *y* moves (section 4.3); its initial value is the XML value 0. The bar
   texture is an opaque metal frame with a translucent, whitish, cracked glass window occupying about
   bar pixels x 80–311, y 21–55 (alpha about 18–45 %); the caption under the glass shows through it.
   The bar has these children (positions relative to the bar, drawn in this order):
   - `magnifire` (76, 22, 232 × 36), texture `ui\ui_magnifier`, `stretch="1"`: **hidden** in the menu
     picture: it is never drawn with the bar. It defines the click area (section 4.4) and the
     **lens** (section 4.6), whose picture is drawn only into the distortion target. It coincides
     with the glass window.
   - `left_grating` (0, 4, 55 × 67), texture `ui_menu_grating_l`, no stretching (drawn at the region's
     own 46 × 70 size from the window's top-left).
   - `right_grating` (337, 4, 55 × 67), texture `ui_menu_grating_r`.
   - `left_anim` 55 × 55, `alignment="c"` so (43, 37) is its **centre**, texture `ui_menu_bigroll_l`
     (86 × 86) stretched to 55 × 55 (`stretch="1"`), rotatable (`heading="1"`).
   - `right_anim` 55 × 55 centred at (340, 37), texture `ui_menu_bigroll_r`, same attributes.
   - `magnifire` contains `<y_offset>0</y_offset>`, used in section 4.3.

### 4.1 Which items are shown

The list shown depends on the game state when the menu opens. Each list is an element of
`ui_mm_main.xml` whose `btn` children give, in order, an item name (`name`) and a caption string key
(`caption`). All lists use `btn_height="30"`, `font="graffiti22"` and colour `a="255" r="0" g="0"
b="0"` (opaque black).

| Game state at opening | List element |
|---|---|
| No level loaded, valid last save exists (below) | `menu_main_last_save` |
| No level loaded, otherwise | `menu_main` |
| Single-player level running, player character exists and its health is ≤ 0 | `menu_main_single_dead` |
| Single-player level running, otherwise | `menu_main_single` |
| Any other (multiplayer) level running | `menu_main_mm` |

In addition, for the title screen (no level) the list `menu_new_game` is prepared as the second page
(section 4.5).

**Valid last save.** The game remembers the name of the most recently saved or loaded game. The name
is persisted in the user configuration file as a console command line `load_last_save <name>` and is
updated whenever a game is saved or loaded. The last save is valid when this name is non-empty and the
file `<name>.sav` exists in the saved-games directory (path alias `$game_saves$`), is at least 8 bytes
long, starts with the 32-bit little-endian value `0xFFFFFFFF`, and its next 32-bit value is greater
than 1 (the game itself writes 3).

Title-screen lists (Russian captions from `config\text\rus\ui_st_mm.xml`):

`menu_main`:

| # | Name | Caption key | Caption |
|---|---|---|---|
| 0 | `btn_new_game` | `ui_mm_newgame` | Новая игра |
| 1 | `btn_load` | `ui_mm_load_game` | Загрузить игру |
| 2 | `btn_multiplayer` | `ui_mm_multiplayer_n` | Сетевая игра |
| 3 | `btn_options` | `ui_mm_options_n` | Опции |
| 4 | `btn_credits` | `ui_mm_credits` | Титры |
| 5 | `btn_quit` | `ui_mm_quit2windows` | Выйти в Windows |

`menu_main_last_save`: the same, with `btn_lastsave` / `ui_mm_last_save` (Последн. сохран.) inserted
as item 2 (7 items: new game, load, last save, multiplayer, options, credits, quit).

`menu_new_game` (difficulty page):

| # | Name | Caption key | Caption |
|---|---|---|---|
| 0 | `btn_novice` | `ui_mm_novice` | Новичок |
| 1 | `btn_stalker` | `ui_mm_stalker` | Сталкер |
| 2 | `btn_veteran` | `ui_mm_veteran` | Ветеран |
| 3 | `btn_master` | `ui_mm_master` | Мастер |
| 4 | `btn_new_back` | `ui_mm_back` | Назад |

In-game lists, for completeness: `menu_main_single` = return to game (`btn_ret`, `ui_mm_return_game`
"Возврат к игре"), last save, load, save (`btn_save`, `ui_mm_save_game` "Сохранить игру"), options,
credits, quit game (`btn_quit_to_mm`, `ui_mm_quit_game` "Завершить игру"), quit to Windows.
`menu_main_single_dead` = last save, load, options, credits, quit game, quit to Windows.
`menu_main_mm` = return to game, options, multiplayer, credits, disconnect (`btn_quit_to_mm`,
`ui_mm_disconnect` "Отсоединиться"), quit to Windows.

All items are always enabled; the only conditional item is the last-save item, which is present or
absent as described.

### 4.2 Item rows

Item *i* (0-based) is a text row 164 × 30 at (0, 30·*i*) in the item region, i.e. at absolute
(700, 419 + 30·*i*). The caption is the string-table text of the caption key, horizontally centred in
the row, drawn in font `graffiti22`, opaque black. (Its placement within the row, vertical and
horizontal, is specified in `docs/formats/text-and-fonts.md`, sections 4.2, 4.4 and 5: the caption is
vertically centred, and where it lands depends on `r__supersample`, because the menu is laid out on the
renderer's off-screen target when frames go through it.) Captions have no background of their own; they appear on the
painted slats of the background picture.

### 4.3 Selection and the sliding bar

Exactly one item is selected. Selecting item *i* starts a **move** of the bar:

- **Target.** The bar's y (relative to `shniaga_wnd`) moves to
  *y₁* = (row y of item *i*) − (y of `magnifire` within the bar) + `y_offset` = 30·*i* − 22.
  The glass window then covers absolute y 418 + 30·*i* … 452 + 30·*i*, i.e. the selected caption.
  Absolute bar position: (599, 397 + 30·*i*). The lens (section 4.6) moves with the bar.
- **Duration.** With *d* = |*y₁* − *y₀*| (*y₀* = bar y when the move starts) and *H* = 280 (the height
  of `shniaga_wnd`): *T* = round(1000 · ln(*d* + 1) / ln(*H*)) milliseconds, but at least 100 ms.
  Examples: *d* = 22 → 556 ms, 30 → 609 ms, 60 → 730 ms, 90 → 801 ms, 120 → 851 ms, 150 → 890 ms,
  180 → 923 ms.
- **Motion.** At elapsed time *t* (0 ≤ *t* ≤ *T*) the bar y is
  *y₀* + (*y₁* − *y₀*) · ln(1 + 10·*t*/*T*) / ln 11 — a fast start that decelerates into the target.
  When *t* exceeds *T* the bar is placed exactly at *y₁* and the move ends. A new selection during a
  move starts a new move from the bar's current position. The bar's x never changes.
- **Clock.** Moves are timed by the engine's continuous millisecond clock (it keeps advancing while
  the menu has the game paused).
- **Cog wheels.** During a move, each frame, the two wheels are rotated as if rolling along the bar's
  path: with *y* the bar y at that frame and *h* = 55 (the wheel window height), the rotation angle is
  *a* = 2π · frac(*y* / (π·*h*)) (equivalently 2*y*/*h* radians modulo 2π). The right wheel gets
  rotation +*a*, the left wheel −*a*, so they turn in opposite directions. The wheels keep their angle
  when the bar is at rest. (Which screen direction a positive rotation of a rotatable static is, is a
  property of the generic UI renderer.)
- **Initial selection.** When the menu opens, and whenever the list page changes (section 4.5),
  item 0 is selected. Since the bar starts at y = 0, the opening move is a 22-unit slide upward
  (556 ms) onto item 0.
  **Measured** (one capture of the unmodified game, 130 s after a start without a level and without
  any input): item 0 (`btn_new_game`) is drawn selected, the bar at rest over its caption. A menu that
  shows no item selected after opening is not the original's behaviour.

### 4.4 Input

**Mouse.**
- **Hover:** when the visible cursor enters an item row, that item becomes selected (the bar slides to
  it). Entering the row of the item that is already selected selects it again (a move of at least
  100 ms toward the same target). Moving within a row has no effect.
- **Click:** a left-button press while the cursor is inside the glass area (`magnifire`, absolute
  (675, 419 + 30·*i*, 232 × 36) for the selected item *i* once the bar is at rest; it moves with the
  bar) **activates** the selected item. The second press of a quick double click (presses less than
  250 ms apart) is reported as a double click and does not activate. The press is not tied to a row:
  any press inside the glass activates whatever item is selected.
- No other mouse buttons, and no wheel action, are used by the list (the list never needs scrolling).

**Keyboard** (key-press events; DirectInput scan codes):
- **Up arrow** (200): selects the previous item if the selected index is greater than 0; no wrap.
- **Down arrow** (208): selects the next item if there is one; no wrap.
- **Enter** (28): activates the selected item.
- **Escape** (1): on the difficulty page, returns to the main page (section 4.5). On the main page it
  does nothing on the title screen; when a level is running and the player character is alive it
  closes the menu (`main_menu off`).
- **Q** (16): quits the game immediately, without confirmation (console command `quit`).
- **F12** (88): takes a screenshot (generic).
- The key(s) bound to opening the console open the console.

**Gamepad:** no gamepad or joystick support was found.

### 4.5 Activation

| Item | Effect |
|---|---|
| `btn_new_game` | Switches the list to the difficulty page `menu_new_game` (no other screen opens). |
| `btn_new_back` | Switches back to the main page (the list chosen in 4.1). |
| `btn_novice`, `btn_stalker`, `btn_veteran`, `btn_master` | Sets the difficulty with console command `g_game_difficulty gd_novice` / `gd_stalker` / `gd_veteran` / `gd_master`; if a single-player game is running, executes `disconnect`; then executes `start server(all/single/alife/new) client(localhost)` and `main_menu off`: a new game starts and the menu closes. |
| `btn_load` | Opens the **load-game screen**; the title menu is hidden while it is open and shown again when it returns. |
| `btn_lastsave` | On the title screen: executes `main_menu off` then `load_last_save` (loads the remembered save, section 4.1). During a game: first opens the confirmation box `message_box_confirm_load_save` of `config\ui\message_box.xml` (text key `ui_st_load_game_confirm`); "Yes" performs the same two commands. |
| `btn_multiplayer` | Opens the **multiplayer screen** (title menu hidden meanwhile) and executes `check_for_updates 0`. |
| `btn_options` | Opens the **options screen** (title menu hidden meanwhile). |
| `btn_credits` | Starts the **credits sequence** `credits_seq` defined in `config\ui\ui_credits.xml` (it has its own background, coal video and music `music\credits`); the menu is not closed by this action. |
| `btn_quit` | Opens the confirmation box `message_box_quit_windows` of `config\ui\message_box.xml` (text key `ui_mm_quit_windows_message` "Действительно ли вы хотите выйти в Windows?", buttons `ui_st_btn_yes` "Да" / `ui_st_btn_no` "Нет"). "Yes" executes `quit` (the game exits); "No" closes the box. |
| `btn_ret` (in game) | `main_menu off`. |
| `btn_save` (in game) | Opens the **save-game screen** (menu hidden meanwhile). |
| `btn_quit_to_mm` (in game) | Opens the confirmation box `message_box_quit_game`, with text key `ui_mm_quit_game_message` in single player or `ui_mm_disconnect_message` in multiplayer; "Yes" executes `disconnect`. |

The item behaviour is implemented in the game's script data, `scripts\ui_main_menu.script`, which
loads `ui_mm_main.xml`, builds layers 1–5 in the order of section 3, and binds the actions above to
the item names; the list widget delivers a "button clicked" event carrying the item name for every
item except `btn_new_game` and `btn_new_back`, which it handles itself.

When a sub-screen (load, save, options, multiplayer) is opened, the title menu window is closed, and
its click area is disabled (the hidden glass area is moved off-screen to x = 1025) and the lens is
switched off (section 4.6). When the sub-screen returns, the menu window is reopened and the click
area and the lens restored; the list, the selection
and the bar position are kept (the menu screen is not rebuilt).

### 4.6 The glass lens

The selected caption is not drawn in another font, size or colour: every caption is drawn as in
section 4.2 (`graffiti22`, opaque black). It looks larger because the glass window is a **lens**: a
screen-space distortion that magnifies whatever the menu has drawn under the window, the caption
together with the glass and slats behind it [GL-1 … GL-11].

**Where the lens comes from.** The `magnifire` window (section 4) is a child of the bar and moves with
it. It is hidden, so it never appears in the menu picture, and it is on the main menu's list of windows
that are drawn into the **distortion target** instead [GL-1, GL-3]. Its picture `textures\ui\ui_magnifier.dds`
is not an image but an offset map (below). No other window is on that list: the script function that
would add one (`SetPPMode`, listed in `scripts\lua_help.script`) is called by no shipped script [GL-8].

**When the lens is active.** The main menu asks for the post-process (`docs/behavior/frame-presentation.md`,
here FP, section 2.5) while it is shown and its **lens flag** is set [GL-2]. Opening the menu sets the flag
[GL-7]. The menu's script (`scripts\ui_main_menu.script`, its `Show` function, which calls
`SetVisibleMagnifier`) clears it when the load, save, options or multiplayer screen opens (and when the
spawn dialog opens) and sets it again when that screen returns; clearing it also moves the `magnifire`
window to x = 1025 (off the screen; see section 4.5), setting it puts the window back at its layout x
[GL-6, GL-8]. Nothing else changes the flag [GL-7]. Message boxes (quit and last-save confirmation)
leave it set, so the lens stays while they are shown [GL-8]. While the flag is set and distortion is
available (`docs/behavior/particle-drawing.md`, here PD, section 6.1: pixel shaders 1.4, no `-nodistort`),
every frame goes through the off-screen target (FP 2.4, 3.1) and the distortion pass runs (FP 3.1 step 3);
without distortion there is no lens.

**Drawing the offset map.** In the distortion pass (PD 6.2) the distortion target, *W*ₜ × *H*ₜ (the
off-screen target's size, FP 1.1–1.2), is cleared to (127, 127, 127, 127) / 255; the world's distortion
list is not drawn; then the `magnifire` picture is drawn into it [GL-4]:

- with the main menu's layout factors of the target, *s*ₓ = *W*ₜ / 1024, *s*ᵧ = *H*ₜ / 768 (as for the
  menu itself, `docs/formats/text-and-fonts.md` 4.4) [GL-2];
- as any layout picture (`docs/formats/ui-layout.md` 5.3, 8): `stretch="1"`, so 232 × 36 units at the
  window's absolute top-left (*X*, *Y*) = (675, 419 + *b* + 22), *b* being the bar's y within
  `shniaga_wnd` (section 4.3), which is (675, 419 + 30·*i*) for the selected item *i* when the bar is at
  rest. Quad from (floor(*X*·*s*ₓ) − 0.5, floor(*Y*·*s*ᵧ) − 0.5) to that point plus (232·*s*ₓ, 36·*s*ᵧ)
  target pixels, texture coordinates (0, 0) to (1, 1) of the whole 256 × 32 texture, shader `hud\default`
  (bilinear, clamped; white vertex colour) [GL-5, GL-9]. The texture is opaque (alpha 255 everywhere), so
  inside the quad the target holds the filtered texel, written as 8-bit values like any colour.

The texture (`gamedata.db8`, 256 × 32, uncompressed 32-bit ARGB, one level) has blue = alpha = 255
everywhere; red and green are the offsets [GL-10]:

- a border 3 texels wide is neutral (127 or 128 in red, 126 or 127 in green);
- in columns 3 … 252, rows 3 … 28, red falls from left to right and is nearly constant down each column
  (within 2): 253 at column 3, at least 253 to column 20, then 235 (64), 201 (96), 158 (128), 110 (160),
  64 (192), 26 (224), 11 (240), 2 (252) (column means over the rows);
- green falls from top to bottom and is nearly constant along each row (within 5): 150 at row 3, 139 (8),
  128 (13), 125 (14), 113 (18), 99 (23), 91 (28) (row means over the columns).

An implementation uses the texture itself; these values are only a description.

**Composition.** The frame is presented with `postprocess_d` (FP 3.1 step 5, 3.4; PD 6.3): a screen pixel
whose scene coordinate is **t** shows the scene, i.e. the menu as drawn into the target, at
**t** + **o**, with **o** = (*D*.r − 0.5, *D*.g − 0.5) · 0.05 and *D* the distortion target sampled
bilinearly at **t**. So a pixel near the glass's left edge (red ≈ 1) shows what lies 0.025 · *W* screen pixels
to its right, one near the right edge (red ≈ 0) what lies 0.025 · *W* to its left, one near the top
(green ≈ 0.59) what lies about 0.0044 · *H* below, one near the bottom (green ≈ 0.36) what lies
about 0.0072 · *H* above: the picture under the glass is spread outwards, i.e. magnified, about 1.45 times
across and 1.65 times down near the centre, and shifted slightly right (red 158 at the centre column).
Outside the quad the cleared value gives an offset of about −0.0001 (negligible, PD 6.3). The lens
samples the whole menu picture there: the caption, the glass and grating of the bar, the slats.

**Timing.** The `magnifire` window's position is derived from the bar's each frame, and the distortion
target is drawn in the same frame as the menu [GL-1, GL-4]. So the lens is always exactly at the glass:
there is no separate animation, fade or scale change on selection. When the selection changes the lens
slides with the bar (section 4.3): the old caption returns to its normal look as the
glass leaves it, the new one is magnified as the glass arrives, and a caption that the glass only
partly covers is distorted only where it is covered. The lens has the same strength at every position;
the offsets do not change over time.

**Measured** (black-box frames of the unmodified game at the title menu, item 0 selected, bar at rest,
at 1920 × 1080, 1280 × 1024, 1024 × 768 and 800 × 600 with `r__supersample` 1 and 4, and 1920 × 1080
with 2): the selected caption's appearance is reproduced by this model, with the caption laid out as in
`docs/formats/text-and-fonts.md` sections 4.4 and 5 and the menu frame resampled as in FP 3.5. Fitting
the frame's darkness under the glass to the model's caption coverage (per-row offsets plus one ink
factor) explains 35 – 49 % of the variance (the glass and slats behind the caption are not modelled), against at most 19 % with
the offsets scaled by 0.8, 0.9, 1.1 or 1.2 and under 1 % without the lens; in three of the runs, finer scaling (±2 %, ±4 %)
and whole-pixel shifts of up to 2 pixels do not fit better by more than 0.01 [GL-11, GL-12].

**Worked examples** (item 0 `Новая игра` selected, bar at rest, *b* = −22; pixel centres at whole
numbers as in FP; coverage ≥ 0.5 extents of the caption computed with the model) [GL-13]:

- **1920 × 1080, `r__supersample` 4.** Target 2048 × 2048, *s*ₓ = 2, *s*ᵧ = 2.6667. Lens quad on the
  distortion target: floor(675 · 2) = 1350, floor(419 · 2.6667) = 1117, from (1349.5, 1116.5), 464 × 96 target
  pixels. Distortion values on target row 1164 (lens centre): (128, 126) at x 1352, (235, 120) at 1466,
  (158, 121) at 1581, (65, 120) at 1697, (127, 126) at 1810. On screen row 614 the offsets, in screen
  pixels, are (+47.3, −1.6) at x 1274, (+48.0, −1.6) at 1309, (+40.5, −1.6) at 1374, (+11.3, −1.6) at
  1483, (−23.7, −1.6) at 1591, (−40.8, −1.6) at 1657, (−46.6, −1.6) at 1691; down screen column 1482
  the vertical offsets are −0.3 (y 591, outside the active rows), +4.8 (595), +2.0 (604), −1.6 (614),
  −5.4 (624), −7.7 (633), −0.3 (637). The caption covers x 1364 … 1527, y 605 … 633 (164 × 29 pixels)
  instead of x 1407 … 1524, y 607 … 627 (118 × 21) without the lens.
- **1024 × 768, `r__supersample` 1.** Target 1024 × 768 (the frame goes through it because the menu asks
  for the post-process), *s*ₓ = *s*ᵧ = 1. Lens quad (674.5, 418.5)–(906.5, 454.5). Distortion values on row
  436: (160, 125) at x 677 (edge texel blended with the border), (253, 122) at 678, (235, 121) at 733,
  (158, 122) at 790, (65, 121) at 848, (3, 121) at 903. Offsets on row 436: (+25.20, −0.98) at x 679,
  (+25.60, −0.98) at 698, (+21.79, −0.98) at 732, (+6.12, −0.83) at 790, (−12.55, −0.98) at 848,
  (−21.58, −0.98) at 883, (−25.00, −0.98) at 902; down column 790: −0.23 (y 420), +3.39 (423), +1.58
  (429), −0.83 (436), −3.84 (444), −5.50 (450), −0.23 (453). Pixel (790, 436) therefore shows the menu at
  (796.12, 435.17). The caption covers x 717 … 830, y 424 … 445 (114 × 22) instead of x 741 … 823,
  y 427 … 441 (83 × 15).
- **1024 × 768, `r__supersample` 2** (the shipped setting). Target 1448 × 1086, lens quad from
  (953.5, 591.5), 328.06 × 50.91 target pixels; the offsets in screen pixels are within 0.5 of the
  `r__supersample` 1 values at the same places (they are fractions of the screen); the caption covers x 716 … 829,
  y 427 … 448.

**Not established:** whether the cursor, drawn after the menu (section 7), passes through the lens; the
lens's look while the credits sequence runs over the menu (section 4.5); the in-game menu (section 4.1),
**believed** the same, since the lens does not depend on a level. With distortion available but the
target unusable (FP 1.2), the lens would be drawn but not shown (PD 6.3).

## 5. Fonts and strings

- `graffiti22` is the font defined by section `[ui_font_graffiti22_russian]` of `config\fonts.ltx`
  (texture `ui\ui_font_graff_22_1024`, with `texture800` / `texture1600` variants);
  `graffiti19` is `[ui_font_graffiti19_russian]` (`ui\ui_font_graff_19_*`). Font file format and
  resolution-dependent variant choice are specified with fonts. In `config\localization.ltx` the
  `font_prefix` value is commented out, so no prefix applies.
- Caption keys resolve through the string tables listed in `[string_table] files` of
  `config\localization.ltx` for language `rus`; all keys used by the menu are in
  `config\text\rus\ui_st_mm.xml` (message-box keys `ui_st_*` in other listed tables).

## 6. Music and sound

The `menu_sound` element of `ui_mm_main.xml` configures the menu's audio:

- The element has the attribute `random="0"` and one active child, `menu_music` with text
  `music\wasteland2`. Further children are commented out in the data: `whell_sound` `car\apc_run`,
  `whell_click` `car\test_car_stop`, and `menu_music` entries `music\guitar_2`, `music\guitar_1`,
  `music\guitar_3`.
- **Track list.** Every `menu_music` child is one track; this edition has exactly one,
  `music\wasteland2`.
- **Stereo pair.** A track name *N* is played from two files, `sounds\N_l.ogg` and `sounds\N_r.ogg`
  (here `sounds\music\wasteland2_l.ogg` and `_r.ogg`: Ogg Vorbis, mono, 44 100 Hz, 125.128 s each).
  Both start together, not looped, as listener-relative ("2D") sounds positioned at
  (−0.5, 0, 0.3) for `_l` and (+0.5, 0, 0.3) for `_r` in listener-relative coordinates (the axis
  convention is the sound system's; the files are evidently the left and right channels). Both belong to the **music** volume category (console
  variable `snd_volume_music`), not to effects.
- **Start.** The music starts when the menu screen is built (each time the menu opens).
- **Continuation.** Every frame while the menu window is open and the game is **not paused**, if
  either of the two sounds has stopped, a track is picked again and both channels restart. The pick
  is pseudo-random among the `menu_music` entries regardless of the `random` attribute (with one
  track, the same track simply repeats). On the title screen the menu does not pause the game, so the
  music normally repeats indefinitely. When the menu is opened during a game the game is paused (section 2), so the
  track plays once and is not restarted.
- **Stop.** The music stops when the menu screen is discarded (after the menu closes).
- **Bar sounds (optional).** `menu_sound` may name a `whell_sound` (played when a bar move starts,
  unless it is still playing, and stopped when the move ends) and a `whell_click` (played once per
  move, during the first tenth of its duration). Each is used only if `sounds\<name>.ogg` exists. In
  this edition both are commented out, so bar moves are **silent**.
- No ambient sound, hover sound or click sound is configured for the title menu.

## 7. Cursor

- The cursor is **visible** while the menu is shown: opening the menu starts its window as a dialog, and
  starting any dialog that asks for the cursor (the menu's does) makes the cursor visible [TM-5]. Its
  image is the animated texture `ui\ui_ani_cursor` rendered with the shader `hud\cursor`.
- It is drawn after the menu, i.e. over it: the cursor and the menu are both in the engine's per-frame
  render list, the cursor with priority 2 and the menu with priority 4, and the list is drawn in order
  of falling priority [TM-7, TM-8].
- "Visible" does not mean "seen": where the image lands depends on the cursor position (below), and
  at the menu's first frame after the startup sequence it lands entirely outside the screen.
- `textures\ui\ui_ani_cursor.seq` is a frame sequence: its first line `19` is the frame rate (a first
  line `cycled` would select back-and-forth playback; it is absent), followed by the 8 frame textures
  `ui\ui_ani_cursor_01` … `_08` (`textures\ui\ui_ani_cursor_0N.dds`, 64 × 64 DXT5: an arrow with a
  radiation sign). The frame period is 1000 / 19 = 52 ms (integer division); the shown frame is
  (continuous clock in ms / 52) mod 8, wrapping 8 → 1.
- The arrow tip is at the image's top-left, which is placed at the cursor position. (Draw size taken
  from the texture, 64 × 64; its scaling to screen pixels follows the generic UI rules and was not
  separately verified.)
- The cursor position is kept in normalised coordinates −1 … +1 on each axis; UI position =
  ((*n*ₓ + 1) · 512, (*n*ᵧ + 1) · 384). Mouse motion of (Δx, Δy) device pixels changes *n* by
  (Δx · *s* / screen width, Δy · *s* / screen height), clamped to −1 … +1, with sensitivity *s* = 1.5
  by default (whether it is configurable was not established).
- **Position when the menu first opens.** The position is (0, 0), the screen centre, when the program
  starts [TM-7]. It is then changed only by mouse motion (above) and by UI sequences: whenever an
  **image** item of a sequence (an `<item type="image">` of `config\ui\ui_movies.xml` or of the
  tutorial file that includes it) starts, the cursor is set to that item's `cursor_pos` attributes
  `x`, `y` in UI units, as *n* = (*x* · 2/1024 − 1, *y* · 2/768 − 1), each clamped to −1 … +1 [TM-2];
  an item without `cursor_pos` uses *x* = 1024, *y* = 768, i.e. *n* = (+1, +1) [TM-1]. Video items do not
  move the cursor [TM-3].
- The startup sequence `intro_logo` (`docs/behavior/startup.md` §6) ends with its two image items (the
  ASUS and NVIDIA pictures), and none of its items has `cursor_pos` [TM-4]. So when the title menu opens
  after it, the cursor is at *n* = (+1, +1): the image's top-left (its tip) is at UI (1024, 768), the
  bottom-right corner, and the whole 64 × 64 image lies outside the screen. **Until the first mouse
  motion no cursor is seen**, and the cursor lies in no item row, so no hover selection happens; the
  first motion up or left brings the tip onto the screen from that corner (motion down or right is
  clamped at +1 and changes nothing). Skipping with the quit key changes nothing here: each press ends
  only the current item and starts the next (`docs/behavior/startup.md` §6.5), so both image items
  start before the menu opens.
  **Disputed in review:** a motion up alone or left alone does not bring the tip onto the screen: the other
  coordinate stays at +1 (UI x 1024 or y 768, just past the last pixel column or row), so the image stays
  outside the screen. The tip comes onto the screen only once the motions so far have moved the cursor
  both left and up.
- With `-nointro` no sequence plays (section 2), so the cursor is at the centre, UI (512, 384), and is
  seen there from the menu's first frame.
- **Measured**: in the capture of section 4.3 no cursor is drawn anywhere on the screen.

## 8. Screen resolution and aspect ratio

- **Mapping.** The 1024 × 768 UI space is mapped to the output by independent scaling of each axis:
  screen x = UI x · W / 1024, screen y = UI y · H / 768 (W × H = output resolution). The menu always
  fills the screen; on non-4:3 screens it is stretched horizontally or vertically (no letterboxing).
- **Wide-screen layout files.** When W / H > 1.3433, the UI layout loader first looks for a variant
  file named with `_16` before the extension (here it would be `config\ui\ui_mm_main_16.xml`) and uses
  it if present. No such file exists in this edition, so the same layout is used at every aspect
  ratio. (That the menu's script-side loading uses this generic rule was inferred, not traced.)
- **Cog-wheel correction.** When the video device is reset (e.g. a resolution change) while the menu
  screen exists and W / H > 1.3433, the cog wheels' width is set to 55 × 0.7518797 ≈ 41.35 units
  (height stays 55) so that they look round on the stretched screen; at other aspect ratios it is set
  back to 55. This correction is not applied when the menu screen is first built.
- Font texture variants per resolution (`texture800`, `texture1600`) are a font-system matter.

## 9. Animations and transitions summary

| Element | Animation |
|---|---|
| Sky panorama video | continuous loop, 4.583 s period |
| Fire video | continuous loop, 3.576 s period |
| Sliding bar | logarithmic-ease moves on selection (section 4.3) |
| Lens | fixed to the glass window, moves with the bar; no animation of its own (section 4.6) |
| Cog wheels | rotate with bar travel only |
| Cursor | 8-frame loop at 19 fps |

No fades or other transitions were found when the menu opens or closes, when the list page changes
(the captions are replaced at once and the bar slides to item 0), or when sub-screens open.

## 10. Data the menu needs from generic formats

- **UI layout XML** (`config\ui\ui_mm_main.xml`): element rectangles (`x`, `y`, `width`, `height`),
  `texture` child (texture id or path, optional `x`/`y`/`width`/`height` texture rectangle),
  `stretch`, `heading`, `alignment="c"` (position is the centre), text attributes (`align`
  `l`/`r`/`c`, `font`, `r`/`g`/`b`[/`a`], `complex_mode`), list attributes (`always_show_scroll`,
  default indents and gap 0), and the menu-specific elements `shniaga_wnd`, `buttons_region`,
  `shniaga`, `magnifire`, `y_offset`, `left_anim`, `right_anim`, `left_grating`, `right_grating`,
  `menu_*` lists with `btn_height`, `font`, colour and `btn name/caption`, `menu_sound` with
  `random`, `menu_music`, `whell_sound`, `whell_click`.
- **Texture descriptions** (`config\ui\ui_mainmenu.xml` via `config\system.ltx [texture_desc]`):
  texture ids → file and pixel rectangle.
- **String tables:** caption keys → Russian text.
- **Fonts:** `config\fonts.ltx` sections named in section 5.
- **Textures:** DDS images; `.ogm` Theora video textures; `.seq` frame sequences.
- **Sounds:** Ogg Vorbis mono files, music category, listener-relative positioning.

## 11. Unused or superseded data

- `textures\ui\ui_btn_mm_*.dds`, `ui_mm_window_back_crop.ogm`, `ui_mm_rug02.ogm`,
  `ui_mm_newspaper.ogm`, `ui_vid_back_0N.ogm` are referenced by other screens, not by the title menu.
- `textures\ui\ui_magnifier.dds` (256 × 32) is never shown as a picture: it is the lens's offset map
  (section 4.6).
- Older copies of `config\ui\ui_mm_main.xml` (in `gamedata.db0`, `gamedata.dba`) and of
  `scripts\ui_main_menu.script` (in `gamedata.db4`, `gamedata.dba`) are overridden by the
  `gamedata.dbb` copies described here; the older layout lacks `menu_main_last_save`,
  `menu_main_single_dead` and `static_version`.

## 12. Evidence (Russian edition)

| Path | Winning archive | Notes |
|---|---|---|
| `config\ui\ui_mm_main.xml` | `gamedata.dbb` | 3 853 bytes in `db0`; dbb copy differs (see 11) |
| `config\ui\ui_mainmenu.xml` | `gamedata.dbb` | identical in `db0`, `dba`, `dbb` |
| `config\ui\message_box.xml` | `gamedata.dbc` | |
| `config\ui\ui_movies.xml`, `config\ui\ui_credits.xml` | `gamedata.dbb` | |
| `config\text\rus\ui_st_mm.xml` | `gamedata.dbb` | identical in `db9` |
| `config\localization.ltx` | `gamedata.db9` | |
| `config\fonts.ltx` | `gamedata.dbb` | |
| `config\system.ltx` | `gamedata.dbc` | `[texture_desc]` lists `ui_mainmenu` |
| `scripts\ui_main_menu.script` | `gamedata.dbb` | |
| `textures\ui\ui_mainmenu.dds` | `gamedata.db8` | 1 048 704 bytes, 1024 × 1024 DXT5 |
| `textures\ui\ui_magnifier.dds` | `gamedata.db8` | 256 × 32 uncompressed 32-bit |
| `textures\ui\ui_mm_main_back_crop.ogm` | `gamedata.db8` | 1 789 705 bytes |
| `textures\ui\ui_mm_fire.ogm` | `gamedata.db8` | 499 680 bytes |
| `sounds\music\wasteland2_l.ogg`, `_r.ogg` | `gamedata.db5` | 1 166 230 / 1 161 149 bytes |
| `textures\ui\ui_ani_cursor.seq`, `_01`…`_08.dds` | `gamedata.db7` | 172 bytes; 4 224 bytes each |

Reproduction: extract the files with the project's archive tool; inspect XML/LTX as Windows-1251
text; `ffprobe` gives the video and audio parameters above; converting `ui_mainmenu.dds` to an image
and extracting its alpha shows the transparent sky region and the bar's translucent glass.

## Provenance

- Date: 2026-11-23; section 3.4 revised 2026-11-26; sections 4.3 and 7 revised 2026-11-26 (issue 550).
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build), archives
  `gamedata.db0`…`gamedata.dbd`; `mods\mp_military_2.xdb0` not used.
- Data examined: the files in section 12 and the related configuration, script, texture, video and
  sound files of the combined archive tree.
- Analysis performed: reading of game data (XML, LTX, scripts, string tables); inspection of media
  with ffprobe/ffmpeg (stream parameters, frames, texture alpha); static analysis with Ghidra of the
  shipped 32-bit binaries (`xrGame.dll`, `XR_3DA.exe`, `xrSound.dll`, `xrGameSpy.dll`) to establish
  list selection, input handling, bar motion, music handling, cursor and video-texture behaviour,
  resolution mapping and the startup flow; for section 3.4 (2026-11-26), how `xrGame.dll` obtains the
  version string from `xrGameSpy.dll` and how that library reads the registry. The binaries were not
  executed, except that (2026-11-26) the unmodified game was run under Wine for section 3.4, and
  (2026-11-23) the
  video frame-to-texture conversion of `XR_3DA.exe` was run in a CPU emulator on the two menu videos (see
  `docs/behavior/startup.md` §6.6). No leaked source code,
  derivative engines or third-party documentation of the engine were used.
- Targeted fix (issue 550, 2026-11-26): static analysis with Ghidra of the cursor's creation, drawing and
  movement, the dialog start, the render-list registration of the cursor and of the menu, and the UI
  sequence's image and video items (their loading and start); `config\ui\ui_movies.xml` read for
  `intro_logo`. Checks `[TM-n]` are in `hud4/checks/run.py` of the dirty workspace. The measured
  statements come from one capture of the unmodified game made by the clean side (headless, Wine 11
  under box64, 1024 × 768).
- Targeted fix (issue 570, 2026-11-26): the caption's placement within its row is now specified in the
  text specification (sections 4.2, 4.4, 5), from black-box runs of the unmodified game at four
  resolutions with `r__supersample` 1, 2 and 4 and from static analysis; checks `[VC-n]` are in
  `vcentre/checks/` of the dirty workspace.
- Targeted fix (issue 571, 2026-11-26): section 4.6 (the lens) added; sections 1, 4, 4.3, 9 and 11
  revised for it. Static analysis with Ghidra (headless, instruction listings of `xrGame.dll` and
  `xrRender_R1.dll`) of the list widget's construction, the main menu's post-process drawing and its
  list of windows, the window and static drawing, the magnifier's visibility switch, the menu
  activation, and the renderer's distortion pass; reading of `config\ui\ui_mm_main.xml`, all game
  scripts, and `textures\ui\ui_magnifier.dds`; the model fitted to the selected caption in black-box
  frames of the unmodified game (the runs of the issue-570 fix: 1920 × 1080, 1280 × 1024, 1024 × 768,
  800 × 600 with `r__supersample` 1 and 4, and 1920 × 1080 with 2). Checks `[GL-n]` are in
  `magn/checks/run.py` of the dirty workspace.
- Not established: cursor draw scaling,
  the screen direction of positive rotation, configurability of cursor sensitivity, whether the
  unmodified install loads `mods\`.

## Gaps found in review

- Whether mouse motion during the startup sequence (after its image items have set the cursor to the bottom-right corner) moves the cursor, and so changes where it is when the menu opens, is not established.
