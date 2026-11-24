# Save and load screens

Functional specification of how S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) lets the player
save and load games: opening the menu during a game and returning to it, the save-game screen, the
load-game screen, their message boxes, quick save and quick load, the automatic saves, and how the
remembered last save is kept. It states what is seen, heard and done, and which data files supply each
element. The save file itself — names, location, contents, the picture file, the remembered last save
and what the load menu reads from a save — is specified in `docs/formats/sav.md` and referred to here,
not restated.

Built on the approved specifications, which are not repeated here:

- `docs/behavior/title-menu.md` (the menu: item lists including the in-game lists §4.1, activation §4.5,
  opening/closing effects §2, cursor §7, the "valid last save" test §4.1);
- `docs/formats/ui-layout.md` (layout XML, rectangles, statics, texture ids and paths, `.seq`, drawing,
  colours, single-line text placement, the 1024 × 768 space);
- `docs/formats/text-and-fonts.md` (string tables, missing-key rule, fonts, complex mode with `\n`);
- `docs/formats/sav.md` (saved games: §1.1 location and names, §1.2 level-change save, §1.3 last
  save, §2 validity, §11.2 loading, §11.4 load-menu data, §13 picture file);
- `docs/formats/ltx-config.md`, `docs/formats/db-archive.md`, `docs/behavior/startup.md` (key bindings:
  `config\default_controls.ltx` with `user.ltx` on top), `docs/behavior/loading-screen.md`.

Conventions: paths are paths in the combined game-data tree, winning copy by archive precedence
(section 15 lists the archives); text files are Windows-1251. Coordinates are UI units. "Absolute"
rectangles are in the 1024 × 768 space. Keys are given by their key names in the binding files and their
DirectInput scan codes. Each fact is established from the data or from static analysis of the shipped
binaries unless marked *believed* (inferred, not traced to the end) or listed in section 14.

## 1. Overview

- The save and load screens are full-screen 2D sub-screens of the menu. They are opened from the menu
  items `btn_save` ("Сохранить игру", in-game list only) and `btn_load` ("Загрузить игру", title screen
  and in game). While a sub-screen is open the menu's own window is closed (title-menu §4.5); the
  sub-screen returns to it with Cancel, Esc, or after saving.
- Both screens are built from layout files (`config\ui\ui_mm_save_dlg.xml`,
  `config\ui\ui_mm_load_dlg.xml`) by the game's script data (`scripts\ui_save_dialog.script`,
  `scripts\ui_load_dialog.script`, shared Esc handling in `scripts\ui_base_dialog.script`).
- A screen is created the first time it is opened after the menu was opened, and then kept until the
  menu closes (the menu screen is rebuilt at every opening, title-menu §2). Within one menu opening a
  screen therefore keeps its list, selection, scroll position, typed name and information area between
  visits (section 12).
- Saves live in the saved-games folder (path alias `$game_saves$`): `<name>.sav` and, optionally, a
  picture `<name>.dds`.
- No interface sound is configured for these screens or their message boxes.

## 2. Opening the menu during a game and returning to it

- **Opening.** While a single-player level is running, pressing the key bound to the action `quit`
  (default Esc, scan code 1; `bind quit kESCAPE`) toggles the menu (console command `main_menu` without
  argument, which opens the menu when closed and closes it when open; `main_menu on`/`1` opens,
  `main_menu off`/`0` closes, any other argument opens). If an in-game window (inventory, PDA, trade,
  dialogue, …) is open, the key is given to that window first and the menu does not open (what such a
  window does with the key is outside this specification). The key works whether or not the game is
  paused and whether or not the player character is alive.
- **What opening does** is title-menu §2: game sounds pause, game time pauses (unless already paused),
  the console hides, the cursor shows, the menu is built with the in-game item list (title-menu §4.1:
  `menu_main_single`, or `menu_main_single_dead` when the player character's health is ≤ 0).
- **What is drawn behind.** Nothing of the game: while the menu is open the level is not rendered
  (title-menu §1); the menu and both screens cover the whole screen with opaque layers (section 4.1).
  The one exception is the moment a save picture is taken (section 8.4).
- **Sound.** Game sounds are paused. The menu music starts when the menu opens and, during a game,
  plays once without restarting (title-menu §6). It is not stopped by opening the save or load screen.
  Buttons, lists, the edit field and message boxes make no sound.
- **Returning to the game.** Menu item "Возврат к игре" (`btn_ret`) or a press of Esc (scan code 1, tested
  directly, not through the `quit` binding) on the menu's main page while a level is running and the
  player character is alive (title-menu §4.4) closes the menu; pauses and console/cursor
  states are restored. Starting a new game or loading a save also closes the menu (loading: sections 5.3 and 9.3; a new game:
  title-menu §4.5).
  On the save and load screens the `quit` key does not return to the game; it returns to the menu
  (section 3.7).
- **Pausing.** The menu pauses the game for its whole duration, including while the save or load screen
  and their message boxes are shown.

## 3. Behaviour common to both screens

### 3.1 Input routing

- Only the topmost screen receives input: the save or load screen, or, while one is shown, its message
  box. The menu itself (the item list) is not visible and gets no input while a sub-screen is open.
- **Per-frame update of every shown screen.** Once per frame every screen that is open and shown is
  updated, not only the topmost one. While a message box is shown, the save or load screen behind it is
  still shown and therefore still updated: its elements keep re-evaluating their cursor-over flags
  (section 3.2), its list keeps following the cursor (focused row, section 3.4), and its buttons keep
  choosing their looks from those flags. Only input events (presses, releases, movements, wheel, holds,
  keys) are withheld from it. Within a screen, only elements that are shown are updated; list rows
  outside the visible range (3.4) are hidden and are not evaluated.
- Before any screen gets a key: the key(s) bound to the console open the console; F12 (scan code 88)
  takes a screenshot (title-menu §4.4). Everything else goes to the topmost screen.
- Mouse: left button presses and releases, cursor movement and the wheel go to the element under the
  cursor (later-attached elements first), and also to an element the cursor has only just left
  (section 3.2). While the left button is held, a *hold* is also reported to the topmost screen in
  frames after the press (section 3.5). The cursor is the menu cursor (title-menu §7).
- **Double click.** A left-button press less than 250 ms (engine continuous clock) after the previous
  left-button press on the same screen is delivered to the elements as a *double click* instead of a
  press (at most one per frame). How the elements treat it: push buttons, list rows and scroll-bar
  arrows like a press (sections 3.2, 3.5); the load screen's list additionally runs its double-click
  action (section 5.3); the name field and the scroll-bar thumb **ignore** it (no focus, no drag;
  sections 4.4, 3.5); all other elements ignore presses anyway. Example: click the down arrow and
  grab the thumb within 250 ms: the thumb does not follow the cursor.
- No gamepad input exists.

### 3.2 Push buttons (`btn_*`, message-box buttons)

A button is a rectangle with a texture *base name* `B` (the `texture` value) whose four looks are the
texture ids `B_e` (normal), `B_h` (cursor over), `B_t` (pressed) and `B_d`
(disabled; no button on these screens is ever disabled). Each look is drawn at its pixel size from the
button's top-left (no stretching). The caption (the `text` child: string key, font) is single-line,
centred horizontally **and** vertically in the button rectangle. Its colour per look comes from the
`text_color` children: `e` normal, `h` cursor over, `t` pressed, `d` disabled; a look without a
`text_color` entry uses the normal colour; the normal colour, if `text_color:e` is absent, is the
colour of the `text` child (UI-layout §7.1 defaults: opaque white).

Behaviour (mouse only; no keyboard shortcut is configured on these screens). A button has a
*cursor-over* flag and one of three states, *idle* (initial), *pressed* and *returning*:

- **Cursor-over flag.** Set when the cursor is inside the button's rectangle (edges included),
  re-evaluated once per frame in the per-frame update (3.1), purely from the cursor position — also
  while the button's screen is behind a message box, and regardless of anything drawn over it. Between
  two evaluations it keeps its value, so the first cursor movement that takes the cursor out of the
  button still finds it set. An element that is not evaluated (a hidden list row) keeps its flag.
  When the flag turns on, the element reports *cursor entered* to its owner; when it turns off,
  *cursor left* (only the list uses these reports, 3.4).
- **Which events a button receives.** A left press, left release, double click or cursor movement
  reaches the button when the cursor is inside its rectangle at that event, or when its cursor-over
  flag is still set (the first movement out of it, or a release in that same frame). Otherwise the
  button does not see the event; there is no mouse capture.
- **Transitions** (only for events the button receives; any other event changes nothing):

  | State | Event | New state | Effect |
  |---|---|---|---|
  | idle | left press or double click | pressed | the button reports *pressed* to its owner (only the scroll-bar arrows react, 3.5) |
  | pressed | left release, cursor-over flag set | idle | the button is **activated** (its action runs) |
  | pressed | left release, flag clear | idle | none |
  | pressed | cursor movement, flag clear (the cursor has just come back over it) | returning | none |
  | returning | cursor movement, flag set | pressed | none |
  | returning | left release | idle | none |

- **Looks.** Chosen in every per-frame update from the state and the flag: pressed: look `t` and the
  `t` caption colour, whether or not the cursor is over the button; idle or returning: look `h` and the
  `h` colour while the cursor-over flag is set, else look `e` and the normal colour. So while a message
  box is shown, a button of the screen behind it under the cursor is drawn with look `h` and the `h`
  caption colour (e.g. the save screen's Cancel at absolute (792, 595)–(949, 643), which lies outside
  the box's rectangle), although it receives no input.
- **Consequences.** A normal click (press and release over the button) activates it. Pressing,
  moving off and releasing elsewhere does not activate it and leaves it *pressed* (look `t`), because
  the release does not reach it. When the cursor comes back over a pressed button, its first movement
  there makes it *returning* (look `h`) and the next one *pressed* again; a release over it then
  activates it (also the release of a new click, since a press changes nothing in this state), while
  a release in the *returning* state only makes it idle. Whether the state is reset when the screen is
  closed and opened again within one menu opening was not established (section 14).

### 3.3 Frames and lines

- **Frame (`list_frame`).** The `texture` value is a base name `B`; the frame is drawn from nine texture
  ids `B_lt`, `B_t`, `B_rt`, `B_l`, `B_back`, `B_r`, `B_lb`, `B_b`, `B_rb`: the four corners at their
  pixel size in the frame's corners, the edges repeated at pixel size along the sides between the
  corners, the `_back` region repeated at pixel size over the interior; where a repetition does not fit,
  the last copy is cut off (cropped, not scaled). (If `textures\<B>.ini` existed it would describe the
  frame instead; none exists for the bases used here.)
- **Line (edit-field background, list highlight).** A horizontal line of length *L* from base name `B`:
  `B_b` at its pixel size at the left end, `B_e` at its pixel size ending at the right end, `B_back`
  repeated at pixel size between them, the last copy cropped. Drawn at the parts' own pixel height from
  the line's top.

### 3.4 Save list

Both screens have a list `form:list` inside a frame `form:list_frame`.

- **Rows.** Every row is `item_height` = 18 units tall and as wide as the list minus the scroll bar
  width (429 − 16 = 413); it has no background of its own. Row *k* counted from the first visible row is
  at (0, 18·*k*) in the list. The number of visible rows is floor(list height / 18): **15** on the save
  screen (287 / 18), **10** on the load screen (180 / 18). Rows outside the visible range are not drawn.
- **Row content** (two single-line, left/top-aligned texts, neither clipped):
  - the save name: the file's name without the last four characters (`.sav`), at row position
    (0, 0), field 230 × 22, font `letterica18`, colour ARGB (255, 216, 186, 140);
  - the file time: `[` + modification time + `]` at row position (250, 0), field 150 × 22, font
    `letterica16`, same colour. The time is the file's last-modification time in local time, formatted
    `DD:MM:YYYY HH:MM` (day, month and year separated by colons; two-digit day, month, hour and minute
    with leading zeros; four-digit year), e.g. `[24:11:2026 14:05]`.
- **Names are lower case.** The game's file index stores every path in lower case and the game writes
  every file with a lower-cased name (section 8.2), so all names shown in the list are lower case
  regardless of how they were typed.
- **Which files.** Every file of the saved-games folder whose name matches `*.sav`. The **load** screen
  lists only files directly in the folder; the **save** screen also lists `.sav` files in sub-folders,
  shown with their relative path (`sub\name`). Files with other extensions (the `.dds` pictures, any
  `.sav` copies elsewhere) are not listed.
- **Order.** Most recently modified first (descending modification time). The order of files with equal
  times was not established.
- **When filled.** Once, when the screen is created (section 1); it is not refreshed while the menu stays
  open, except that deleting removes a row (section 3.6, 4.3, 5.3) and the load screen drops rows whose
  file has vanished (section 5.2).
- **Selection.** Exactly zero or one row is selected; initially none. A row is selected when it is
  activated as a push button (section 3.2: each row behaves as a button without visible looks; a
  normal click is a left press and release on the same row). The selected row is marked by a highlight line (section 3.3) with base name `ui_listline`
  (`ui_listline_b` 4 × 18, `ui_listline_back` 28 × 18, `ui_listline_e` 4 × 18 in
  `textures\ui\ui_common.dds`), 18 units tall, at the selected row's position, as long as the list width
  (429) or, while the scroll bar is shown, the list width minus 16; it is drawn behind the row texts and
  clipped to the list rectangle (so it disappears when the selected row is scrolled out of view). There
  is **no** hover highlight: the list draws one only when its layout element has a non-zero
  `active_bg` attribute (neither list has one) or when a script sets a highlighted row explicitly
  (these screens' scripts never do).
- **Focused row.** Besides the selected index the list keeps a *focused* index, initially *none*. It
  is set to a row's index (its position in the whole list, 0 = first row) when that row reports *cursor entered* (its cursor-over flag turning on in
  the per-frame update, 3.2), and reset to *none* when the row whose index equals it reports *cursor
  left*; a *cursor left* of any other row does not change it. Clicking does not set it, and removing a
  row does not change it (the stored number stays while the rows after the removed one are renumbered
  down by one). In practice it is the index of the row under the cursor as of the last per-frame
  update, including while a message box covers the list (3.1). Only the save screen uses it (4.3).
- **Re-delivered movement.** When a row reports *cursor left*, the list, at the start of its next
  per-frame update, delivers to itself one cursor movement at the last cursor position it received,
  routed to its rows and scroll bar as any movement (3.1, 3.2). Rows and arrows have no distinct looks,
  so this has no immediate visible effect; it can only move a row or arrow between the *pressed* and *returning*
  states of 3.2.
- **Keyboard.** The list has no keyboard control (no arrow keys, Enter, Delete or Page keys).
- **Wheel.** Each wheel notch with the cursor over the list (rows, empty part or scroll bar) scrolls one
  row: wheel forward (away from the user) towards the first row, backward towards the last; scrolling
  stops at the ends. Scrolling never changes the selection.
- **Removing a row** (deletion, vanished file — always the selected row) never changes the selected
  *index*; it is written only when a row is clicked. The row that moves into that position becomes the
  selected one (highlighted). When the removed row was the last row, the index points one past the end
  of the list: a **stale selection**. No highlight is drawn (no row has that index), but the screens
  still treat it as a selection (sections 3.6, 4.3, 5.3). It ends when a row is clicked, and with the
  screen (next menu opening).

### 3.5 Scroll bar

- Shown only while the list has more rows than fit (count > 15 resp. 10); hidden otherwise.
- Position: the rightmost 16 units of the list, full list height: absolute (903, 274, 16 × 287) on the
  save screen, (903, 399, 16 × 180) on the load screen. Its look comes from profile `default` of
  `config\ui\scroll_bar.xml`: up arrow 16 × 16 at the top (texture id `ui_scroll_btn_up`), down arrow
  16 × 16 at the bottom (`ui_scroll_btn_down`), track between them (`ui_scroll_back`), thumb
  (`ui_scroll_box`); these regions are 15 × 16 pixels of `textures\ui\ui_common.dds`. The arrows are
  drawn once each at pixel size (15 × 16 units at the arrow's top-left).
- **Track and thumb drawing.** Each is drawn by repeating its region, unstretched, along the bar: for a
  length *L* (the track: *T*, from 16 units below the bar's top; the thumb: its own length, from the
  thumb's top), *n* = floor(*L* / 16) copies of 15 × 16 units are placed one below the other starting
  at the top-left, plus one more copy when *L* − 16·*n* > 0; everything is clipped to the rectangle
  15 units wide and *L* units tall at that top-left (also to any enclosing clip), so the last copy is
  cropped at the bottom, not scaled. Across the 16-unit bar there is one copy, 15 units wide from the
  bar's left edge; the bar's rightmost unit is not covered by the track or the thumb. The top-left is
  converted to screen pixels and floored as for any picture (UI-layout §5.3); the copies follow at
  multiples of 16 units. Examples: the save screen's track (*T* = 255) is 16 copies, the last showing
  its top 15 rows; the load screen's (*T* = 148) 10 copies, the last showing 4 rows; a 131-unit thumb
  9 copies, the last showing 3 rows.
- Scroll range: the first visible row *f* runs from 0 to count − visible rows.
- **Arrows** are push buttons (section 3.2) whose looks all use the normal texture. Absolute arrow
  rectangles: save screen up (903, 274)–(919, 290), down (903, 545)–(919, 561); load screen up
  (903, 399)–(919, 415), down (903, 563)–(919, 579). A *step* scrolls one row: the up arrow's towards
  the first row, the down arrow's towards the last; a step at an end of the range does nothing. Steps
  happen:
  1. when an arrow becomes *pressed* (left press or double click on it, 3.2);
  2. when an arrow is *activated* (release over it, 3.2);
  3. **while the left button is held:** in every frame after the press frame in which the left button
     is down both before and after that frame's mouse input (so neither in the press frame nor in the
     release frame), if the cursor is inside the up or the down arrow's absolute rectangle (edges
     included) and more than 100 ms have passed (engine continuous clock, sampled once per frame) since
     the last hold step of any scroll bar (one time shared by all scroll bars, 0 at program start), one
     step of that arrow is made and the 100 ms period restarts. This depends only on where the cursor
     is, not on where the button was pressed: a press elsewhere (a row, the track) followed by moving
     the cursor onto an arrow scrolls too. Holds go only to the topmost screen, so there are no hold
     steps while a message box is shown.

  Results: a click whose release comes in the frame after the press (or in the same frame) moves two
  rows; a click spanning three frames moves three (press, hold step, release) unless a hold step
  happened less than 100 ms before; holding the button on an arrow then adds one row each time more
  than 100 ms have passed since the previous hold step.
- **Thumb.** Track length *T* = list height − 32 (255 on the save screen, 148 on the load screen).
  Thumb length = *T* · (visible rows) / (count − 1), but at least 16 and at most *T*. Thumb top =
  16 + *f* · (*T* − thumb length) / max(1, count − visible rows) (thumb length truncated to an integer,
  integer division), measured from the scroll bar's top. **Dragging:** after a left press on the thumb
  (not one delivered as a double click, 3.1, which does nothing),
  the thumb receives all mouse events until the release; each cursor movement during that time moves
  the thumb vertically by the cursor's vertical movement, but only if after the movement the cursor is
  within 10 units of the thumb's rectangle (from 10 units left of / above it, inclusive, to 10 units
  right of / below it, exclusive); the thumb is kept between the arrows, and the list scrolls to the
  position corresponding to the thumb (exact rounding not established). Clicking the track outside the
  thumb does nothing.

### 3.6 Deleting a save (both screens)

The Delete button (`btn_delete`, "Удалить") does nothing while the list is empty or no row has been
selected since the screen was created. Otherwise — including with a stale selection (3.4) — it opens the message box `message_box_delete_file_name` ("Вы действительно хотите удалить
файл?", Yes/No; section 6). "Нет" closes the box. "Да" closes the box, deletes `<name>.sav` and, if it
exists, `<name>.dds` from the saved-games folder (`<name>` is the row's name), removes the row from
the list (section 3.4) and then refreshes as described per screen (4.3, 5.3). With a stale selection,
"Да" deletes nothing: the screen's script fails when it asks the list for the selected row (a script
runtime error; its further observable consequence — error message, ignored, or game stop — was not
established, section 14). Deleting the remembered
last save (section 11) makes the title screen's "Последн. сохран." item disappear the next time the
menu is built (its validity test fails).

**Volya Engine (deliberate deviation, not the original's behaviour):** treat a stale selection as no
selection — clear the selection when the selected row is removed and no row moves into its position,
so Delete, Load and the double click then do nothing, as with no selection.

### 3.7 Esc and Enter

- The key bound to `quit` (Esc) returns from the save or load screen to the menu, exactly like Cancel,
  whatever has keyboard focus. The trigger is **any press or any release** of that key that reaches the
  screen (the menu passes both key presses and key releases to the topmost screen, and the screens'
  shared Esc handling does not distinguish them). Normally the press closes the screen and the release
  then goes to the menu, which ignores key releases, so nothing more happens. A release alone also
  closes the screen when its press went elsewhere, for example:
  - press Esc while a message box is shown (the box ignores it), keep holding it, close the box with
    the mouse (with "Нет", or "ОК"), then release Esc: the screen returns to the menu;
  - open the console on one of these screens and press Esc: the press hides the console, and the
    release, which then reaches the screen, returns it to the menu.
- On the save screen, if the name field has focus and is not empty, an Esc **press** also clears the
  field (section 4.4); the field ignores releases, so a screen closed by a release keeps its text.
- Neither a press nor a release is handled by the screen while a message box is shown (the box is the
  topmost screen).
- **Enter** has no effect on either screen, except that on the save screen it ends typing in the name
  field (section 4.4). (The script data contains Enter handlers for both screens, but they test for an
  event value that the engine never delivers for keys, so they never run.)
- Message boxes ignore the keyboard entirely (section 6).

## 4. The save-game screen

### 4.1 Layout

Built from `config\ui\ui_mm_save_dlg.xml`; the screen window is (0, 0, 1024, 768). Elements in drawing
order (later on top); "Abs" is the absolute rectangle (form children are relative to `form`, which is at
(415, 168)):

| # | Element | Abs (x, y, w, h) | Content |
|---|---|---|---|
| 1 | `back_video` | 0, 0, 1024, 430 | video `textures\ui\ui_mm_window_back_crop.ogm`, texture rectangle (0, 0, 1024, 430): rainy night village with a burning barrel; drawn 1024 × 430 |
| 2 | `background` | 0, 0, 1024, 768 | `textures\ui\ui_static_mm_back_04.dds` (1024 × 1024 DXT5), no rectangle and no stretch, so drawn 1024 × 1024 from (0, 0); only its top 768 rows are on screen. Its alpha is transparent in the sky and window panes (upper part), through which `back_video` shows |
| 3 | `newspaper_video` | 137, 512, 256, 256 | video `textures\ui\ui_mm_newspaper.ogm` (opaque, matches the painted floor with papers and a teddy bear) |
| 4 | `form` | 415, 168, 560, 460 | texture id `ui_menu_options_dlg` → `ui\ui_common` (164, 232, 566, 461), drawn 566 × 461 (a dark metal-mesh panel) |
| 5 | `form:caption` | 480, 178, 500, 25 | text key `ui_mm_save_game` "Сохранить игру", font `graffiti22`, ARGB (255, 184, 168, 131), single line, left, top |
| 6 | `form:edit` | 480, 223, 257, 23 | name field (section 4.4): line base `ui_linetext_e` (`_b` 9 × 24, `_back` 33 × 24, `_e` 9 × 24), text font `letterica18`, ARGB (255, 240, 217, 182) |
| 7 | `form:list_frame` | 480, 264, 449, 307 | frame base `ui_tablist_textbox` (nine 32 × 32 parts) |
| 8 | `form:list` | 490, 274, 429, 287 | save list, `item_height` 18 (section 3.4) |
| 9 | `form:btn_save` | 480, 595, 157, 48 | button base `ui_button_main01`, caption `ui_mm_save` "Сохранить" |
| 10 | `form:btn_delete` | 636, 595, 157, 48 | same base, caption `ui_mm_delete` "Удалить" |
| 11 | `form:btn_cancel` | 792, 595, 157, 48 | same base, caption `ui_mm_cancel` "Отмена" |

All three buttons: caption font `graffiti22`; `text_color` `e` (227, 199, 178), `t` (180, 153, 155),
`d` (106, 95, 91), `h` (0, 0, 0) — i.e. light caption normally, black when hovered, dimmer while
pressed. Button looks: `ui_button_main01_e` (0, 470, 157, 48), `_h` (0, 566, 157, 48), `_t`
(0, 518, 157, 48), `_d` (0, 422, 157, 48) in `textures\ui\ui_common.dds`.

The file also contains `_back_video`, and the form contains `_texture` and `_texture_offset`; these
are not used (no screen element is built from them).

### 4.2 Initial state

Name field empty and without focus; no row selected; list scrolled to the top; scroll bar shown if more
than 15 saves.

### 4.3 Actions

| Action | Effect |
|---|---|
| Click a row | The row becomes selected (3.4). Then the name field's text is replaced by the name (lower case, without `.sav`) of the row at the list's **focused** index (3.4), not the selected one; if the focused index is *none* or no row has it, the field keeps its text. For a normal click the two are the same row (the row under the cursor). The field does not get focus. |
| Save (`btn_save`) | Let *N* be the name field's text. If *N* is empty: message box `message_box_empty_file_name` ("Имя файла пустое", OK); the screen stays. Else if the saved-games folder already holds `<N>.sav` (names compared case-insensitively): message box `message_box_file_already_exist` ("Файл с таким именем уже существует. Перезаписать?", Yes/No); "Да" saves under *N* and returns to the menu, "Нет" just closes the box. Otherwise: saves under *N* (section 8) and returns to the menu. |
| Delete (`btn_delete`) | Section 3.6. After a confirmed deletion (row removed) the screen applies the same name rule as a click: if the list is now empty, nothing else changes; otherwise, if a row exists at the focused index (3.4), its name replaces the field's text, else the field keeps its text. The focused index is that of the save-screen row that was under the cursor, behind the box, when "Да" was released (3.1); after the removal that index holds the next row down when the deleted row was above it. Example: the box's "Да" is at absolute (450, 404)–(567, 433) and the save list's visible rows 7 and 8 (counted from 0 at the first visible row) span x 490–903, y 400–418 and 418–436; releasing "Да" at x ≥ 490 over one of them, when that row exists, writes a name into the field; releasing it at x < 490 leaves the field unchanged. Apart from the list (3.4), no other element changes. |
| Cancel (`btn_cancel`), Esc | Returns to the menu without saving. |

"Returns to the menu" means: the save screen closes and the menu window reappears with its list,
selection and bar position unchanged (title-menu §4.5). The menu stays open and the game stays paused;
the player returns to the game with "Возврат к игре" or Esc.

A name the save action refuses (section 8.1) also returns to the menu, but no save is made and nothing
is shown.

### 4.4 The name field

- **Focus.** The field gets keyboard focus when a left press that is not a double click (3.1) is
  made inside it while it does not have focus; the caret is then placed after the last character. A
  press delivered as a double click does nothing (the layout has no `db_click` attribute, which would
  let a double click focus it too). Example: click a row (its name goes into the field), then within
  250 ms press inside the field: the field does not get focus, and typing does nothing until the next
  press inside it that is not a double click. A press while it has focus does not move the caret. It
  loses focus on Enter (scan code 28) or numpad Enter (156) and on Esc while the text is empty.
  Whether it keeps focus when the screen is closed and opened again within the same menu opening was
  not established (section 14). Without focus the field ignores all keys (presses and releases).
- **Look.** Background: line `ui_linetext_e` of length 257 from the field's top-left (left cap 9 units,
  right cap ending at x + 257, middle repeated, section 3.3; 24 units tall, one more than the field).
  Text: single line, left aligned, `letterica18`, ARGB (255, 240, 217, 182), starting 3 units right of
  the field's left edge and vertically centred in the field: its line's top is at field top +
  (23 − font line height in units) / 2, the same height as the caret's (no `x`/`y` on the `text`
  child shifts it; no `align` is given). While focused, a caret is
  drawn: the character `_` in the same font, placed immediately after the text up to the caret
  position, vertically centred in the field (y = field top + (23 − font line height in units) / 2). The
  caret does not blink.
- **Characters.** Typing is defined by key scan codes, independent of the keyboard layout (Cyrillic
  cannot be typed). The field keeps one *shift* flag: it is set by a **press** of left or right Shift
  (42, 54) and cleared by a **release** of either, both only while the field has focus; gaining or
  losing focus does not change it. So Shift pressed before the field got focus types unshifted
  characters until Shift is pressed again with focus; Shift released while the field had no focus
  leaves the flag set, and the field types shifted characters without Shift held until a Shift
  release with focus; with both Shift keys held, releasing one clears the flag. Flag clear → flag
  set:
  - letter keys `A`…`Z` → lower case `a`…`z` / upper case `A`…`Z` (Caps Lock is ignored);
  - digit keys `1`…`9`, `0` → the digit / `!` `@` `#` `$` `%` `^` `&` `*` `(` `)`;
  - `-` → `-` / `_`; `=` → `=` / `+`; `[` → `[` / `{`; `]` → `]` / `}`; `;` → `;` / `:`;
    `'` → `'` / `"`; `\` → `\` / `|`; `,` → `,` / `<`; `.` → `.` / `>`; `/` → `/` / `?`;
  - Space (57) → space.
  Numpad digits, Tab and all other keys type nothing.
- **Editing keys.** Backspace (14) deletes the character before the caret; Delete (211) deletes the
  character after it; Left (203) / Right (205) move the caret by one character within the text; Esc
  (1, on its press) clears the text if it is not empty (and the screen then closes, section 3.7),
  otherwise removes focus. Key releases do nothing in the field. There is no Home/End, selection, or clipboard.
- **Key repeat** (real-time clock, ms; checked once per frame while the field has focus). The field
  keeps a *repeat key* (none when it gains focus) and a time mark *M*:
  - in each frame with focus and no repeat key, *M* is set to the current time;
  - every key press with focus (any key, Shift included) is applied once and makes that key the
    repeat key, in its *delay* phase; *M* is not changed;
  - in the delay phase, in the first frame in which more than 400 ms have passed since *M*, the
    repeating phase starts and *M* is set to the current time (nothing is applied in that frame);
  - in the repeating phase, in each frame in which more than 100 ms have passed since *M*, the key is
    applied again and *M* is set to the current time;
  - a release of the repeat key (with focus) ends repeating; releases of other keys do not; Enter,
    and Esc on an empty field, end repeating with the focus.
  Consequences: only the most recently pressed key repeats; with no key repeating before, the first
  repeat comes a little over 500 ms after the last frame before the press; holding `a` and then
  pressing and holding Shift makes Shift the repeat key (no visible effect) and `a` stops repeating;
  holding `a` and pressing and releasing `b` stops all repeating although `a` is still held; a key
  pressed while another key is repeating starts its delay from that key's last mark, so its first
  repeat can come sooner.
- **Length limit.** A typed character is accepted only while the width of the text already in the field
  (in UI units, current font) is at most 256 (field width − 1); there is no character-count limit.
- **Initial content** is empty (no default name is proposed).

## 5. The load-game screen

### 5.1 Layout

Built from `config\ui\ui_mm_load_dlg.xml`; elements in drawing order (form at (415, 168)):

| # | Element | Abs (x, y, w, h) | Content |
|---|---|---|---|
| 1 | `back_video` | 0, 0, 1024, 430 | as on the save screen |
| 2 | `background` | 0, 0, 1024, 768 | `textures\ui\ui_static_mm_back_04.dds`, texture rectangle (0, 0, 1024, 768) |
| 3 | `newspaper_video` | 137, 512, 256, 256 | as on the save screen |
| 4 | `form` | 415, 168, 560, 460 | as on the save screen |
| 5 | `form:caption` | 480, 178, 500, 25 | text key `ui_mm_load_game` "Загрузить игру", `graffiti22`, ARGB (255, 184, 168, 131) |
| 6 | `form:picture` | 520, 239, 128, 128 | save picture (section 5.2), `clipper="1"`; initially texture path `ui\ui_noise` |
| 7 | `form:file_info` | 462, 223, 497, 171 | `textures\ui\ui_static_mm_back_04.dds` rectangle (0, 768, 497, 171): a metal frame around a square window (transparent where the picture shows through) and a mesh panel; drawn over the picture |
| 8 | `form:file_caption` | 714, 277, 512, 20 | save name, font `letterica18`, ARGB (255, 238, 155, 23), `complex_mode="1"` |
| 9 | `form:file_data` | 714, 297, 512, 50 | save details (section 5.2), `letterica18`, ARGB (255, 216, 186, 140), `complex_mode="1"` |
| 10 | `form:list_frame` | 480, 389, 449, 200 | frame base `ui_tablist_textbox` |
| 11 | `form:list` | 490, 399, 429, 180 | save list, `item_height` 18; the list's `font` child (`letterica16`, (216, 186, 140)) has no visible effect, the rows set their own fonts |
| 12 | `form:btn_load` | 480, 595, 157, 48 | button base `ui_button_main01`, caption `ui_mm_load` "Загрузить" |
| 13 | `form:btn_delete` | 636, 595, 157, 48 | caption `ui_mm_delete` "Удалить" |
| 14 | `form:btn_cancel` | 792, 595, 157, 48 | caption `ui_mm_cancel` "Отмена" |

Buttons as on the save screen (4.1). `file_caption` and `file_data` are left/top aligned and not
clipped (their texts may extend past the form). Unused data: `_back_video`, `_texture`,
`_texture_offset` (as in 4.1), and `file_info`'s `_texture`.

### 5.2 Information area

Initially: `file_caption` and `file_data` empty, picture `ui\ui_noise`. Whenever a row is clicked
(selected), and after deletions (5.3), the area shows the selected row's save *N*:

- **`file_caption`:** *N* (as shown in the list).
- **`file_data`:** three lines, joined by line breaks (complex mode):
  1. `Уровень: ` + level name — translation of string key `st_level` ("Уровень"), `: `, and the
     translation of the save's level name (sav §11.4, e.g. `l01_escape` → "Кордон"; a key without
     translation shows the key itself);
  2. `Время: ` + in-game time — key `ui_inv_time` ("Время"), `: `, then the save's game time (sav
     §11.4) as
     `HH:MM MM/DD/YYYY` (hour, minute, month, day with two digits and leading zero; four-digit year;
     note month before day). The fields come from the game's own split of the game-time value *t*
     (milliseconds, sav §12), which is **not** an exact inverse of the sav §12 date formula; implement
     it as stated, with unsigned 64-bit integer arithmetic and integer (floor) division:
     - milliseconds = *t* mod 1000, then *t* ← *t* div 1000; seconds = *t* mod 60, *t* ← *t* div 60;
       minutes = *t* mod 60, *t* ← *t* div 60; hours = *t* mod 24; the day count *D* = *t* div 24;
     - year *Y* = *D* · 400 div 146097 + 1;
     - day of year *r* = *D* − 365 · (*Y* − 1) − *Y* div 4 + *Y* div 100 − *Y* div 400 + 1 (note: the
       leap corrections use *Y*, not *Y* − 1);
     - month: start at 1; while the month is January … November and *r* is greater than that month's
       length (31, February 28 or 29 when *Y* is a leap year — divisible by 4 and not by 100, or by
       400 —, 31, 30, 31, 30, 31, 31, 30, 31, 30), subtract the length from *r* and go to the next
       month; December takes whatever remains, with no upper limit; the day is the final *r*.
     Consequences for dates encoded by sav §12: 01.05.2012 05:30 shows `05:30 05/01/2012`; 31.12.2012
     00:00 shows `00:00 01/01/2013`; 01.01.2011 and 01.01.2014 show day 32 of December of the
     previous year (`12/32/2010`, `12/32/2013`); every other day of 2011–2014 shows its own date.
  3. `Здоровье NN%` — key `ui_inv_health` ("Здоровье"), one space, the player character's health
     (sav §11.4) as a percentage, the fraction × 100 truncated to an integer (believed: the script's integer
     conversion truncates), immediately followed by `%` (no colon).
  If the `.sav` file no longer exists, `file_data` is `no file data` (literal, untranslated) and then the
  row is removed from the list (3.4); the caption keeps the vanished name and the picture is unchanged.
  For a file that exists but is not valid (sav §2) the area shows the defaults of sav §11.4: the
  configured New Game start time (`config\alife.ltx` `[alife]` `start_date` 01.05.2012, `start_time`
  05:30:00, shown "05:30 05/01/2012"), health 100 %, and the level named there.
- **Picture:** if `<N>.dds` exists in the saved-games folder, the picture element shows that file
  (texture path `<N>`, resolved per UI-layout §6.3: the current level's folder, then the saved-games
  folder, then `textures\`); it is a 128 × 128 image drawn at pixel size, filling the element exactly.
  Otherwise the element shows `ui\ui_noise`: the 11-frame `.seq` animation of 256 × 256 noise textures
  (25 fps), drawn at pixel size and clipped to the 128 × 128 element, so its top-left quarter is seen.
- After a confirmed **deletion** that leaves a stale selection (3.4: the deleted row was the last row
  and other rows remain), `file_caption` and `file_data` are cleared and the picture returns to
  `ui\ui_noise`. Dropping a vanished file's row (above) can also leave a stale selection (when it was
  the last row); the area then stays as described above (vanished name, `no file data`, picture
  unchanged).

### 5.3 Actions

| Action | Effect |
|---|---|
| Click a row | Selects it and fills the information area (5.2). |
| Load (`btn_load`), or double click in the list | Nothing if the list is empty or no row has been selected since the screen was created. With a stale selection (3.4) the script fails when it asks for the selected row (script runtime error, consequence not established, section 14); nothing is loaded. Otherwise, with *N* the selected name: if *N* is not valid (section 7, sav §2), message box `message_box_invalid_saved_game` ("Не совпадает версия сохранённой игры или сохранённая игра испорчена.", OK), which only closes. If valid and **no single-player game is running** (title screen): loads at once — console commands `disconnect` then `start server(N/single/alife/load) client(localhost)`; the menu closes as the level starts loading (loading screen, `docs/behavior/loading-screen.md`). If valid and **a single-player game is running**: message box `message_box_confirm_load_save` ("Загрузка игры приведет к потере несохраненных данных. Желаете продолжить?", Yes/No); "Да" executes `load N` (section 9.3), "Нет" closes the box. |
| Delete (`btn_delete`) | Section 3.6. After a confirmed deletion: if the list became empty nothing else changes (the information area keeps showing the deleted save); otherwise the information area is refreshed for the row now at the selected index, or cleared (5.2) if the selection became stale. |
| Cancel (`btn_cancel`), Esc | Returns to the menu (as 4.3). |

**Double click** means: the second left-button press of a double click (3.1) with the cursor anywhere
over the list rectangle — rows, empty part or scroll bar. The first click of a double click on a row
selects that row (on its release), so double-clicking a row loads it.

## 6. Message boxes

The boxes are defined in `config\ui\message_box.xml` (33 definitions; this section uses five). A box is
shown over the current screen, which stays drawn and keeps its per-frame update behind it (cursor-over
flags, button looks, the list's focused row; section 3.1) but receives no input until the box closes.
The box's own window covers the whole screen, so clicks outside the box's rectangle do nothing. Boxes
react to the mouse only: Enter, Esc and all other keys do nothing (the console and F12 keys still work,
3.1); a release of Esc that comes after the box has closed reaches the screen (3.7). Clicking a box button closes the box and then performs the screen's reaction (sections 3.6, 4.3,
5.3).

Common layout of the five boxes (absolute; the box is at (200, 250)):

| Element | Abs (x, y, w, h) | Content |
|---|---|---|
| box | 200, 250, 510, 206 | texture id `ui_frame_error` → `ui\ui_common` (213, 0, 510, 206) |
| `picture` | 235, 315, 77, 77 | sign: `ui_frame_error_sign_red` (800, 0, 77, 77) or `ui_frame_error_sign_alarm` (723, 0, 77, 77) |
| `message_text` | 370, 290, 310, 60 | text key, font `letterica18`, ARGB (255, 216, 186, 140), complex mode (wrapped to width 310, text-and-fonts §4.3), left, top |
| `button_yes` | 450, 404, 117, 29 | button base `ui_button_ordinary`, caption `ui_st_btn_yes` "Да" |
| `button_no` / `button_ok` | 570, 404, 117, 29 | caption `ui_st_btn_no` "Нет" / `ui_st_btn_ok` "ОК" |

Button captions: font `letterica16`, ARGB (255, 216, 186, 140) in every look (no `text_color`); looks
`ui_button_ordinary_e` (0, 115, 117, 29), `_h` (0, 144, 117, 29), `_t` (0, 173, 117, 29), `_d`
(0, 86, 117, 29). `check_mode="0"` (ordinary push buttons).

| Box | `type` | Sign | Text key | Text | Buttons |
|---|---|---|---|---|---|
| `message_box_empty_file_name` | `ok` | red | `ui_st_file_name_is_empty` | Имя файла пустое | ОК |
| `message_box_file_already_exist` | `yes_no` | red | `ui_st_file_exist_owerwrite_q` | Файл с таким именем уже существует. Перезаписать? | Да, Нет |
| `message_box_delete_file_name` | `yes_no` | red | `ui_mm_delete_existing_file_q` | Вы действительно хотите удалить файл? | Да, Нет |
| `message_box_confirm_load_save` | `yes_no` | alarm | `ui_st_load_game_confirm` | Загрузка игры приведет к потере несохраненных данных. Желаете продолжить? | Да, Нет |
| `message_box_invalid_saved_game` | `ok` | alarm | `ui_st_invalid_saved_game` | Не совпадает версия сохранённой игры или сохранённая игра испорчена. | ОК |

(`message_box_confirm_load_save` is also used by the in-game "Последн. сохран." item, title-menu §4.5.)

## 7. What the screens use from a save

- A save named *N* is `<N>.sav` in the saved-games folder `$game_saves$`, with an optional picture
  `<N>.dds` next to it (sav §1.1, §13). On disk both names are lower case (8.2); the list shows the
  lower-case name.
- The list needs, per file, only the name and the last-modification time (local time, 3.4).
- The information area shows the level, the game time and the player character's health that the
  load menu reads from the save (sav §11.4), plus the picture file (5.2).
- *Valid* means the test of sav §2 (the same as title-menu §4.1's "valid last save"); it gates loading
  from the load screen, quick load and `load`.

## 8. Saving

### 8.1 The save action

Every manual save — the save screen, quick save — is the console command `save N` (quick save: `save`
with no name). It does nothing (only a console log line, nothing on screen) when:

- the game is not single-player;
- the player character does not exist or its health is ≤ 0;
- *N* contains any of the characters `/ \ : * ? " < > |`.

Otherwise the game is saved under *N* (an empty *N* means the quick-save name, 9.1).

### 8.2 Files written

- `<N>.sav` in the saved-games folder (sav §1.1), with the name **lower-cased** (the game lower-cases
  every path it writes). Neither the `save` command nor quick saves and autosaves ask before replacing
  an existing file of that name; the save screen asks first (4.3). That the old file is overwritten is
  believed (sav §1.1).
- `<N>.dds`, the picture (8.4, sav §13), also lower-cased.
- The file index is updated at once, so the new save appears in lists created afterwards (not in an
  already created list, 3.4).

### 8.3 Message

After a manual save, the HUD shows the text `<translation of st_game_saved>: <N>` — "игра сохранена: "
followed by the name as given (original case; for a quick save the quick-save name) — in the HUD
element `game_saved` of `config\ui\ui_custom_msgs.xml`: absolute (100, 730, 800, 30), font `graffiti22`,
ARGB (255, 240, 217, 182), centred horizontally. It disappears 3 seconds later on the game clock, which
does not advance while the game is paused; a save made from the menu therefore shows its message for
3 s after the player returns to the game (believed: the HUD is not drawn while the menu is open). A
new save message replaces the previous one and restarts the 3 s.

### 8.4 Picture capture

- The picture file, when it is taken and when it is not (windowed mode, level-change and New Game
  saves) are those of sav §13. The image is scaled to 128 × 128 regardless of aspect ratio.
- If the request comes while the menu is open, the menu is not drawn and the game view (the level and
  its HUD, frozen by the pause) is drawn instead until the picture has been taken, so it shows the game, not
  the menu; the player sees the game view for those frames, then the menu again. The console, if shown,
  is hidden for the capture and restored afterwards. Opening or closing the menu is not possible while a
  capture is pending.
- **Windowed mode:** no picture is written (sav §13); the save itself is made. Such a save shows the
  noise picture on the load screen, or the picture of an earlier save of the same name if its
  `<N>.dds` is still there (believed: no code removing an old picture was found).

## 9. Quick save and quick load

### 9.1 Quick-save name

`<user>_quicksave`, where `<user>` is the operating-system user (log-on) name, e.g. user `Andy` →
save name `Andy_quicksave`, files `andy_quicksave.sav` / `andy_quicksave.dds`, message "игра сохранена:
Andy_quicksave". There is one quick-save slot; each quick save overwrites it.

### 9.2 Keys

Default bindings (`config\default_controls.ltx` and the default `user.ltx`): `bind quick_save kF6`,
`bind quick_load kF7` — F6 (scan code 64) and F7 (65). The same keys are also bound to the multiplayer
voting actions `vote` and `vote_yes`; bindings of single-player-only and multiplayer-only actions do not
conflict, and in single player F6/F7 mean quick save/quick load.

Both keys act only while playing a single-player level: the menu is closed, the game is not paused, no
in-game window takes the key first, and player input is not disabled by the game's scripts (believed:
e.g. during scripted scenes). Otherwise they do nothing.

- **Quick save (F6):** console command `save` (no name) → saves as `<user>_quicksave` with picture and
  message (section 8); nothing happens if the player character is dead.
- **Quick load (F7):** if `<user>_quicksave` is valid (section 7), executes `load <user>_quicksave` (9.3)
  without any confirmation; otherwise nothing happens and nothing is shown.

### 9.3 The `load` command

`load N` (from the load screen's confirmation, F7, or the console) requires a running single-player
game, a non-empty *N*, an existing and valid `<N>.sav`; otherwise it only logs. It closes the menu if
open, releases the pause, and loads the save as in sav §11.2 (loading screen:
`docs/behavior/loading-screen.md`).

## 10. Automatic saves

The automatic saves, their names and pictures are those of sav §1.1 and §1.2: the New Game save
`all` (no picture), the level-change save `<user>_autosave` (no picture) and the periodic automatic
save `<user>_autosave` (with picture), on the schedule and conditions given there
(`config\alife.ltx` `[alife]` `autosave_interval = 01:05:00`, `delay_autosave_interval = 00:00:30`).
What the player sees of them:

- **New game, level change:** no message. The level-change save's information area names the
  destination level (e.g. "Уровень: Свалка" after crossing from `l01_escape` to `l02_garbage`),
  because the player is moved there before it is written (sav §1.2). An older
  `<user>_autosave.dds` from a periodic automatic save is then shown with it (believed, sav §13).
- **Periodic automatic save:** the HUD text `st_autosave` "Автосохранение..." in the element
  `autosave` of `config\ui\ui_custom_msgs.xml` (absolute (100, 700, 800, 30), `graffiti22`, ARGB
  (255, 240, 217, 182), centred) for 3 s of game time, and a picture as in 8.4.
- The default `user.ltx` already contains `load_last_save all`.

## 11. The remembered last save

The remembered last save, when it is set, how `load_last_save` uses it and how it is persisted in
`user.ltx` are those of sav §1.3 (every save of sections 8–10 and every start from a save sets it);
title-menu §4.1 describes the menu item that uses it. Shown names keep the case in which the name was
given (e.g. typed `Moy_Sejv` → last save `Moy_Sejv`, file `moy_sejv.sav`).

## 12. Lifetime of the screens within one menu opening

- Each screen is created on its first opening after the menu opened and is reused until the menu
  closes; it is not rebuilt when re-entered.
- Consequences: after saving, the save screen's list does not show the new save if the screen is opened
  again before the menu is closed; the name field keeps its text; a load screen opened earlier in the
  same menu opening shows a stale list, one opened for the first time after the save shows it.
- The next opening of the menu builds fresh screens (empty field, no selection, current files).

## 13. Verification facts

1. `config\ui\ui_mm_save_dlg.xml` builds 11 elements and `config\ui\ui_mm_load_dlg.xml` 14, with the
   absolute rectangles, textures, fonts, colours and strings of tables 4.1 and 5.1.
2. Visible rows: 15 (save), 10 (load); row width 413; scroll bars at (903, 274, 16 × 287) and
   (903, 399, 16 × 180); track lengths 255 and 148; arrow rectangles as in 3.5; with 30 saves on the
   save screen the thumb is 131 units long, its top at 16 (first row on top) and 140 (*f* = 15); with
   11 saves on the load screen the thumb fills the track (148).
3. Message boxes: `config\ui\message_box.xml` has 33 box definitions; the five used ones have the
   rectangles of section 6; button rows at absolute y 404.
4. String keys and Russian texts: `ui_mm_save_game` "Сохранить игру", `ui_mm_load_game` "Загрузить
   игру", `ui_mm_save` "Сохранить", `ui_mm_load` "Загрузить", `ui_mm_delete` "Удалить", `ui_mm_cancel`
   "Отмена" (all in `config\text\rus\ui_st_mm.xml`); `ui_st_btn_yes` "Да", `ui_st_btn_no` "Нет",
   `ui_st_btn_ok` "ОК", `st_level` "Уровень" and the box texts of section 6 (`ui_st_other.xml`);
   `ui_inv_health` "Здоровье", `st_autosave` "Автосохранение...", `l01_escape` "Кордон", `l02_garbage`
   "Свалка" (`string_table_general.xml`); `ui_inv_time` "Время" (`ui_st_inventory.xml`);
   `st_game_saved` "игра сохранена" (`string_table_ui.xml`).
5. Texture ids (all in `config\ui\ui_common.xml`, file `ui\ui_common`, `textures\ui\ui_common.dds`
   1024 × 1024 DXT5, 1 048 704 bytes): `ui_menu_options_dlg` (164, 232, 566, 461); `ui_linetext_e_b`
   (41, 21, 9, 24), `_back` (50, 21, 33, 24), `_e` (82, 21, 9, 24); `ui_tablist_textbox_lt`
   (117, 45, 32, 32), `_t` (149, 45), `_rt` (181, 45), `_l` (117, 77), `_back` (149, 77), `_r`
   (181, 77), `_lb` (117, 109), `_b` (149, 109), `_rb` (181, 109), all 32 × 32; `ui_listline_b`
   (0, 37, 4, 18), `_back` (4, 37, 28, 18), `_e` (32, 37, 4, 18); `ui_scroll_btn_up` (124, 173, 15, 16),
   `ui_scroll_back` (124, 189, 15, 16), `ui_scroll_box` (124, 205, 15, 16), `ui_scroll_btn_down`
   (124, 221, 15, 16); button and box regions as in sections 4.1 and 6.
6. Media: `textures\ui\ui_mm_window_back_crop.ogm` 1 771 398 bytes, Theora, picture 1024 × 430 (coded
   1024 × 432), 30 fps, 113 frames, 3.767 s; `textures\ui\ui_mm_newspaper.ogm` 850 739 bytes, 256 × 256,
   33 fps, 149 frames, 4.515 s; `textures\ui\ui_static_mm_back_04.dds` 1 048 704 bytes, 1024 × 1024 DXT5;
   `textures\ui\ui_noise.seq` 180 bytes, rate 25, 11 frames (`ui_noise_` 00 01 02 03 04 03 01 02 00 03
   04), `ui_noise_00.dds` 32 896 bytes, 256 × 256 DXT1.
7. Test vectors: file time 2026-11-24 14:05 local → row text `[24:11:2026 14:05]`; save on `l01_escape`
   at in-game 2012-05-01 05:30 with health 0.857 → `file_data` lines "Уровень: Кордон", "Время: 05:30
   05/01/2012", "Здоровье 85%"; game time 31.12.2012 00:00 (sav §12 value 63,492,595,200,000) →
   "Время: 00:00 01/01/2013"; 01.01.2014 00:00 (63,524,131,200,000) → "Время: 00:00 12/32/2013"; user `Andy` → quick save `Andy_quicksave`, files `andy_quicksave.sav`
   and `andy_quicksave.dds`; typed name `Moy_Sejv` → files `moy_sejv.sav`/`.dds`, list row `moy_sejv`,
   last save `Moy_Sejv`, message "игра сохранена: Moy_Sejv".
8. Automatic-save interval 3 900 000 ms, retry delay 30 000 ms (sav §1.1); save picture 128 × 128 DXT1 (a DDS of 8 320
   bytes: 128-byte header + 8 192 bytes of blocks, *believed* from the format).
9. A new game writes `all.sav` and no `all.dds`.
10. `quit` key on these screens: Esc pressed and released on the save or load screen → one return to
    the menu (on the press). Esc pressed while the delete box is shown, box closed with "Нет" while Esc
    is held, Esc released → the screen returns to the menu on the release (the box's "Нет" itself
    leaves the screen open). Console opened on the load screen, then Esc pressed and released → the
    console hides on the press and the load screen returns to the menu on the release.
11. Name field: click a row, then press inside the field within 250 ms → no focus (typing `a` adds
    nothing); press again more than 250 ms later → focus, caret after the row's name, `a` is appended.
    With focus: Shift pressed, `a` → `A`; Shift released, `a` → `a`; Shift pressed, Enter (focus
    ends), Shift released, new focus, `a` → `A` (flag still set).
    Holding `a` with no other key: `a` once, then repeats starting a little over 500 ms later, about
    every 100 ms (one frame later at most); pressing and releasing `b` meanwhile: `b` once, then no
    more repeats.
12. Drawing: with more than 15 saves the save screen's track is 16 copies of `ui_scroll_back`, 15 units
    wide at absolute x 903, from y 290, clipped at y 545 (the 16th copy shows 15 of its 16 rows); the
    load screen's track 10 copies from y 415, clipped at y 563. The name field's text starts at
    absolute x 483 (field left 480 + 3), with its line top at 223 + (23 − line height of `letterica18`
    in units) / 2.
13. Focused row after deleting on the save screen: 20 saves, list scrolled to the top, row 2 clicked
    (selected; the field shows its name); Delete, then "Да" pressed and released with the cursor at
    absolute (500, 410) (over visible row 7, index 7) → files of row 2 deleted, the field shows the
    name formerly in row 8 (now index 7). Same with the cursor at (460, 410), over no row → the field
    keeps the deleted row's name. While the box is shown with the
    cursor at (850, 620), the save screen's Cancel button is drawn with `ui_button_main01_h` and a black
    caption.

## 14. Not established

- The observable consequence of the script runtime error raised by Delete-"Да", Load or the double
  click with a stale selection (3.4): whether an error is shown, the action is silently dropped, or the
  game stops.
- Order of list rows with equal modification times.
- The rounding that turns a dragged thumb position into a list position.
- The input-disable condition that blocks F6/F7 (what postpones a periodic automatic save is listed
  as unestablished in sav §1.1).
- Whether a renamed/overwritten picture is shown fresh when the same texture name was shown before in
  the same session (texture caching).
- Whether leading or trailing spaces in a typed name are kept by the `save` command.
- Whether the name field keeps keyboard focus (and its repeat key) when the save screen is closed
  and opened again within one menu opening.
- Whether the HUD (and the "игра сохранена" message) appears in the save picture.
- Whether a push button's state (3.2) is reset when its screen is closed and opened again within one
  menu opening.

## 15. Evidence (Russian edition)

| Path | Winning archive | Size | Notes |
|---|---|---|---|
| `config\ui\ui_mm_save_dlg.xml` | `gamedata.dbb` | 2 229 | identical in `db0`, `dba` |
| `config\ui\ui_mm_load_dlg.xml` | `gamedata.dbb` | 2 822 | identical in `db0`, `dba` |
| `config\ui\message_box.xml` | `gamedata.dbc` | 26 290 | 33 boxes; older copies in `db0`, `dba`, `dbb` |
| `config\ui\ui_custom_msgs.xml` | `gamedata.dbb` | 4 138 | `game_saved`, `autosave` |
| `config\ui\ui_common.xml` | `gamedata.dbb` | 40 013 | identical in `db0`, `dba` |
| `config\ui\scroll_bar.xml` | `gamedata.dbb` | 2 239 | profile `default` |
| `scripts\ui_save_dialog.script` | `gamedata.dbb` | 6 801 | older 6 818-byte copies in `db4`, `dba` |
| `scripts\ui_load_dialog.script` | `gamedata.dbb` | 9 540 | older 8 351-byte copies in `db4`, `dba` |
| `scripts\ui_base_dialog.script` | `gamedata.dbb` | 5 619 | Esc handling; older 6 063-byte copies in `db4`, `dba` |
| `scripts\ui_main_menu.script` | `gamedata.dbb` | 8 447 | opens the screens; older 8 493-byte copies in `db4`, `dba` |
| `config\alife.ltx` | `gamedata.dbb` | 8 758 | `[alife]` autosave intervals, start date/time |
| `config\default_controls.ltx` | `gamedata.dbb` | 1 111 | `quick_save kF6`, `quick_load kF7` |
| `config\text\rus\ui_st_mm.xml`, `ui_st_other.xml`, `string_table_general.xml`, `string_table_ui.xml`, `ui_st_inventory.xml` | `gamedata.dbb` | | strings |
| `textures\ui\ui_mm_window_back_crop.ogm`, `ui_mm_newspaper.ogm`, `ui_static_mm_back_04.dds`, `ui_noise.seq` | `gamedata.db8` | | media |
| `textures\ui\ui_common.dds` | `gamedata.db7` | 1 048 704 | |
| `__support\userdocs\user.ltx` (GOG install) | – | | `bind quick_save kF6`, `bind quick_load kF7`, `load_last_save all` |

SHA-256: `ui_mm_save_dlg.xml` 87b18545…faea5c9, `ui_mm_load_dlg.xml` f968a6f2…305d7b1,
`message_box.xml` a195109d…f635f5f.

Reproduction: extract the files with the project's archive tool, read XML/LTX/scripts as
Windows-1251, probe media with `ffprobe`; build the element tables of 4.1, 5.1 and 6 by composing each
element's `x`/`y` with the `form` (or box) position and looking texture ids up in the texture
descriptions.

## Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build), archives
  `gamedata.db0`…`gamedata.dbd`, and the GOG default user files in `__support\userdocs\`.
- Data examined: the files of section 15 and the related layout, texture-description, string-table,
  configuration and script files.
- Analysis performed: reading of game data (XML, LTX, scripts, string tables, `.seq`); inspection of
  media with ffprobe/ffmpeg (stream parameters, frames, texture alpha); static analysis with Ghidra 12.2
  (headless) of the shipped 32-bit binaries `xrGame.dll`, `XR_3DA.exe`, `xrRender_R1.dll` and
  `xrCore.dll` (console commands for saving, loading, the menu and the last save; key-action handling
  in a level; the UI widgets used by the screens: buttons, list, scroll bar, edit field, frames, message
  boxes; input routing and double-click detection; the save routine, file naming and lower-casing; the
  save-picture capture; the autosave timer and the level-transition and new-game saves; the script
  bindings used by the screens). The binaries were not executed. Script-language integer conversion is
  from the public Lua 5.1 semantics. No leaked source code, derivative engines or third-party
  documentation of the engine were used.
- Revision 2026-11-24 (after review): re-read in `xrGame.dll` every list routine that removes rows
  or reads the selection, and the screens' scripts; sections 3.4, 3.6, 5.2, 5.3 and 14 now describe the
  stale selection left by removing the last row (was "nothing is selected") and mark the Volya Engine
  handling as a deliberate deviation.
- Revision 2026-11-24 (second review): re-traced in `XR_3DA.exe` and `xrGame.dll` how mouse button
  state is reported per frame (press, release, hold), how the menu forwards it to the topmost screen
  and down the element tree, the push-button states and looks, the scroll bar's hold handling and thumb
  dragging, and the list's highlight conditions; sections 3.1, 3.2, 3.4, 3.5 and 5.2 revised (arrow
  hold steps, buttons left pressed after a release elsewhere, information area after dropping a
  vanished file's row).
- Revision 2026-11-24 (third review): re-read the shared Esc handling script and, in `xrGame.dll`
  and `XR_3DA.exe`, how key presses and releases reach the topmost screen, the edit field's key
  handling and the console's Esc handling; sections 3.7, 4.4 and fact 13.10 now state that a release of
  the `quit` key also closes the screen.
- Revision 2026-11-24 (fourth review): re-read in `xrGame.dll` the name field's mouse handling (focus
  only on a left press, double click only with `db_click`), its key handling (shift flag, repeat key
  and time mark, per-frame update while focused) and the thumb's mouse handling (drag only on a left
  press); sections 3.1, 3.5, 4.4, 13.11 and 14 revised.
- Revision 2026-11-24 (fifth review): re-read in `xrGame.dll` how the scroll bar draws its track and
  thumb (repeat counts, remainder, clip rectangle, unstretched region) and how the name field places
  its text (text position offset, the text-lines object's alignment as set at creation, its
  vertical-centre rule, the absent `align`/`x`/`y` on the `text` child); sections 3.5, 4.4, 13.12 and
  14 revised. Sections 7, 8, 10 and 11 now refer to the approved `docs/formats/sav.md` for the save
  files, picture, automatic saves and last save instead of restating them; statements that differed
  from it (picture timing, automatic-save clock and conditions, last-save persistence, invalid-file
  level) were removed in its favour.
- Revision 2026-11-24 (seventh review): re-read the save screen's row-click handler (focused, not
  selected, index) and, in `xrGame.dll`, the list's handling of its rows' cursor-entered/-left reports,
  its initial values, row removal and renumbering, its item lookup by index, its per-frame update (the
  re-delivered movement), the per-frame cursor-over evaluation and the push button's per-frame look
  choice, and the menu's per-frame update of every open and shown screen; sections 3.1, 3.2, 3.4, 4.3,
  6, 13.13 and 14 revised. Two weakly backed statements were removed or moved to section 14 (what an
  in-game window does with the `quit` key; button state across re-showing).
- Revision 2026-11-24 (eighth review): traced in `xrGame.dll` the split of a game-time value into
  year, month, day, hour, minute, second and millisecond that the script's game-time accessor uses,
  and checked its use by the load screen's script; section 5.2 now states it and 13.7 has edge dates
  (checked by the dirty-workspace script `saveload8/split_check.py` against the sav §12 formula for
  every day of 2011–2014). Section 2 now names Esc (scan code 1) for leaving the menu's main page, as
  the menu's script tests it.
- Throwaway check: a new script written only from this text and the approved specifications (dirty
  workspace, `saveload3/check3.py`), using freshly extracted winning copies, rebuilt the element lists
  of sections 4.1, 5.1 and 6, the scroll-bar geometry of 3.5, the string keys, tables and texts of
  section 13, the HUD message elements, the `alife.ltx` values and key bindings, and the test vectors
  of 13.7; all 150 comparisons with the values given here matched. Media facts were re-measured with
  ffprobe and the archive listings.
