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
  (flanked by two cog wheels that turn as the bar slides). There is no per-item hover, pressed or
  disabled visual; all feedback is the bar's position.
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
  `back_movie` texture rectangle (0, 0, 876, 400) therefore maps exactly to the decoded frame.
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
aligned (`align="l"`), font `graffiti19` (section 5), colour RGB (135, 123, 116), opaque. The version
string is the value `InstallVers` of the Windows registry key
`HKEY_LOCAL_MACHINE\Software\GSC Game World\STALKER-SHOC\` (at most 128 bytes) if present, otherwise
the built-in `1.0006`.

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
   - `magnifire` (76, 22, 232 × 36), texture `ui\ui_magnifier`: **invisible**. It is never drawn; it
     only defines the click area (section 4.4). It coincides with the glass window.
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
the row, drawn in font `graffiti22`, opaque black. (Vertical placement within the row follows the
generic text rules; not specified here.) Captions have no background of their own; they appear on the
painted slats of the background picture.

### 4.3 Selection and the sliding bar

Exactly one item is selected. Selecting item *i* starts a **move** of the bar:

- **Target.** The bar's y (relative to `shniaga_wnd`) moves to
  *y₁* = (row y of item *i*) − (y of `magnifire` within the bar) + `y_offset` = 30·*i* − 22.
  The glass window then covers absolute y 418 + 30·*i* … 452 + 30·*i*, i.e. the selected caption.
  Absolute bar position: (599, 397 + 30·*i*).
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
its click area is disabled (the invisible glass area is moved off-screen to x = 1025). When the
sub-screen returns, the menu window is reopened and the click area restored; the list, the selection
and the bar position are kept (the menu screen is not rebuilt).

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

- The cursor is visible while the menu is shown. Its image is the animated texture `ui\ui_ani_cursor`
  rendered with the shader `hud\cursor`.
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
  by default (whether it is configurable was not established). The initial position was not
  established.

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
- `textures\ui\ui_magnifier.dds` (256 × 32 colour gradient) is loaded for `magnifire` but never shown.
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

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build), archives
  `gamedata.db0`…`gamedata.dbd`; `mods\mp_military_2.xdb0` not used.
- Data examined: the files in section 12 and the related configuration, script, texture, video and
  sound files of the combined archive tree.
- Analysis performed: reading of game data (XML, LTX, scripts, string tables); inspection of media
  with ffprobe/ffmpeg (stream parameters, frames, texture alpha); static analysis with Ghidra of the
  shipped 32-bit binaries (`xrGame.dll`, `XR_3DA.exe`, `xrSound.dll`, `xrGameSpy.dll`) to establish
  list selection, input handling, bar motion, music handling, cursor and video-texture behaviour,
  resolution mapping and the startup flow. The binaries were not executed. No leaked source code,
  derivative engines or third-party documentation of the engine were used.
- Not established: vertical caption placement within a row, cursor draw scaling and initial position,
  the screen direction of positive rotation, configurability of cursor sensitivity, whether the
  unmodified install loads `mods\`.
