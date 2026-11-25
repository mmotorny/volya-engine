# Inventory screen

Functional specification of the single-player inventory screen of S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): how it opens and closes, what is drawn, where every element sits and what it shows,
how the player's items are laid out on it, and how mouse and keyboard actions on it change the player
character's inventory.

It builds on these approved specifications and does not repeat them:

- `docs/formats/ui-layout.md` (cited as `layout`): layout XML, rectangles, textures, texture ids,
  `stretch`, `alignment`, text attributes, colours, the 1024 × 768 space, the `_16` rule, scroll views
  (section 9.1);
- `docs/formats/text-and-fonts.md` (cited as `text`): string tables, fonts, simple and complex text
  (colour codes `%c[…]`);
- `docs/behavior/title-menu.md` (cited as `title`): the menu cursor (section 7), the 250 ms double click
  (section 4.4);
- `docs/behavior/options-screens.md` (cited as `options`): actions and keys (section 11), frame windows
  and frame lines (section 4.9), the scroll bar (section 4.8);
- `docs/behavior/save-load-screens.md` (cited as `saveload`): push buttons (section 3.2), opening the
  main menu with the `quit` key while an in-game window is open (section 2);
- `docs/behavior/player-state.md` (cited as `pstate`): the inventory's slots, belt and backpack
  (section 3.3), the active slot, carried weight and carry limit (3.4), eating (5), the `[inventory]`
  values (1).

Conventions:

- Paths are paths in the combined game-data tree; text files are Windows-1251. "Configuration" is the
  `system.ltx` tree. Coordinates are units of the 1024 × 768 UI space (`layout` section 3).
- "The screen" is the inventory screen; "the character" is the player character; "the inventory" is the
  character's inventory of `pstate`.
- An **item section** is the configuration section of an item.
- Statements are static (read from the program and the data; the game was not run). What was inferred
  rather than read is marked **believed**; what could not be established is marked **unestablished**;
  what needs a system that has no specification yet is marked **dependent** and described only
  structurally.

## 1. Opening and closing

### 1.1 Opening

- The key(s) bound to the action `inventory` (`options` section 11; default `bind inventory kI`, DIK 23,
  in `config\default_controls.ltx`) open the screen when all of these hold: a level is running, the game
  is not paused, the character is alive (health > 0), and no other in-game window (PDA, dialogue, trade,
  body search, …) is open. The key is handled on key press.
- Nothing else in the single-player game opens it: the only script that creates such a screen,
  `scripts\game_last_cl.script`, belongs to a multiplayer game type; the item-transfer windows are other
  screens.

### 1.2 Closing

The screen closes on any of:

1. a press of a key bound to `inventory` while the screen is the topmost in-game window (the same key
   toggles it);
2. a press of a key bound to `quit` (default Esc, DIK 1): the screen does not use this key, so it is not
   consumed and the in-game window is closed instead of opening the main menu (`saveload` section 2:
   "the key is given to that window first"); the main menu does not open on this press;
3. activation of `exit_button` (section 6.2);
4. the character's death (health ≤ 0): the screen (like the PDA, dialogue and body-search windows) is
   closed by the in-game interface's periodic update.

### 1.3 What opening and closing do

- **The game is not paused**: game time, the world, the character's condition (`pstate` section 4) and
  sounds continue. The level is rendered behind the screen as usual (section 3.1).
- **Character control.** Keyboard keys that the screen does not use (section 6.1) are passed on to the
  character's controls exactly as without the screen, so the character can walk, run, crouch, jump and
  use the other bound actions while the screen is open (static; the screen does not ask to stop the
  character's movement). Mouse buttons, mouse movement and the wheel are taken by the screen and never
  reach the character's controls (exception: mouse buttons 4–8, if bound, are handled like keyboard
  keys). On opening, the actions `wpn_fire` and `wpn_zoom` are released (as if their keys were let go).
- **Cursor.** The menu cursor of `title` section 7 is shown while the screen is open.
- **HUD.** While the screen is open the game's on-screen indicators are hidden and one further HUD
  display flag is cleared (its meaning is **unestablished**); both are restored to their previous values
  on closing.
- **Rebuild.** Opening rebuilds the screen's content from the inventory (section 4.3), clears the
  selection (section 5.1) and updates the per-frame values (section 4.7) at once.
- **Sounds.** Opening plays `snd_open`, closing `snd_close` (section 7).
- **Information portions.** In the single-player game, opening gives the character the information
  portion `ui_inventory` and closing gives `ui_inventory_hide` (`config\gameplay\info_portions.xml`: each
  disables the other). Scripts read them (e.g. `xr_conditions.script` `can_send_tutorial` returns false
  while the character has `ui_inventory`); their effects are **dependent** (scripts).
- **Active item.** Opening and closing do not change the item in the hands, except after an addon was
  attached to or detached from the weapon in the hands (section 5.8): that weapon is taken out of the
  hands when the addon action happens, and closing the screen makes its slot the active slot again.

### 1.4 Pause and console

- The key bound to `pause` still pauses the game while the screen is open. While the game is paused the
  screen ignores all keyboard and mouse input (it stays drawn).
- The console and the screenshot key work as without the screen.

## 2. Files

| File | Use |
|---|---|
| `config\ui\inventory_new.xml` (normal) / `inventory_new_16.xml` (`_16` rule, `layout` 1.1) | the screen (section 3) and the context menu (`properties_box`, section 5.6) |
| `config\ui\inventory_item.xml` / `inventory_item_16.xml` (includes `ui\af_params.xml` / `ui\af_params_16.xml`) | the item information panel (section 4.6) |
| `config\ui\ui_icons_npc.xml` | texture ids of the character pictures (section 4.4) |
| `textures\ui\ui_icon_equipment.dds` | item icons (1024 × 2048 px, DXT5, one mip level) |
| `textures\ui\ui_grid.dds` | the cell background (128 × 64 px, DXT5, one mip level) |
| `sounds\interface\inv_*.ogg` | the screen's sounds (section 7) |

The layout file (the variant chosen by the `_16` rule at that moment) is loaded once per level, when the
level's in-game interface is created; the screen then lives as long as the level. The layout is not
re-read when the screen opens. The attachment order of the elements is fixed (section 3.1), whatever the
order in the file.

## 3. Layout

### 3.1 The screen

The screen is the window `main` (0, 0, 1024 × 768; it draws nothing itself). It covers the whole UI
space, but only its elements are drawn; where none is drawn the rendered level shows through (in the
normal layout: the band y 0–80 above `belt_slots`, and the areas outside the pictures).

Elements are attached in the order of the table below; later ones are drawn on top of earlier ones
(`layout` 4.3). "Abs." is the absolute top-left. All statics follow `layout` section 5 (picture) and 7
(text); "drawn" is the picture size in units (texture pixels, or the element size when `stretch` is on).

Normal layout (`inventory_new.xml`):

| # | Element (parent) | Abs. rectangle | Picture | Text |
|---|---|---|---|---|
| 1 | `belt_slots` (`main`) | 0, 80, 1024 × 172 | id `ui_slots_belt` (`ui\ui_inventory2` 0, 749, 1024 × 172), drawn 1024 × 172 | — |
| 2 | `back` | 10, 243, 1005 × 483 | id `ui_inventory_main` (`ui\ui_inventory` 0, 0, 1005 × 483), drawn 1005 × 483 | — |
| 3 | `bottom_static` | 0, 736, 1024 × 32 | path `ui\ui_bottom_background` (whole texture, 1024 × 32 px), drawn 1024 × 32 | — |
| 4 | `bag_static` | 15, 240, 350 × 480 | — | weight line (section 4.5): offset (30, 8), `graffiti22`, (231, 153, 22), complex mode |
| 5 | `money_static` | 744, 530, 108 × 33 | — | money (4.7): `graffiti22`, (238, 153, 26), centred horizontally and vertically |
| 6 | `descr_static` | 370, 253, 335 × 475 | — | `ui_inv_description` «Описание»: offset (25, 0), `graffiti22`, (238, 155, 23); holds the information panel (4.6) |
| 7 | `character_frame_window` | 730, 260, 268 × 460 | frame without texture: draws nothing (believed) | title static `title` at (80, −8) relative, 240 × 60 (abs. 810, 252): `ui_inv_personal` «Костюм», `graffiti22`, (231, 153, 22) |
| 8 | `static_personal` (`character_frame_window`) | 731, 545, 100 × 260 | — (nothing drawn) | — |
| 9 | `progress_background` | 739, 340, 59 × 215 | id `ui_scale_three` (`ui\ui_inventory2` 210, 39, 59 × 179), drawn 59 × 179 | — |
| 10 | `progress_bar_health` (`progress_background`) | 747, 362, 10 × 124 | bar (4.7), picture id `ui_scale_red_1` (307, 41, 9 × 128) | — |
| 11 | `progress_bar_psy` (same) | 765, 362, 10 × 124 | bar, id `ui_scale_blue_1` (334, 41, 9 × 128) | — |
| 12 | `progress_bar_radiation` (same) | 784, 362, 10 × 124 | bar, id `ui_scale_yellow_1` (316, 41, 9 × 128) | — |
| 13 | `outfit_info` | 740, 550, 260 × 180 | protection list (4.8) | caption static at (140, 0) relative: `ui_st_params` «Параметры», `graffiti22`, colour `UI_orange` (238, 153, 26), simple mode |
| 14 | `dragdrop_bag` (`bag_static`) | 37, 273, 310 × 442 | backpack list (4.1) | — |
| 15 | `dragdrop_belt` | 645, 136, 410 × 60 | belt list | — |
| 16 | `dragdrop_outfit` | 858, 290, 104 × 260 | outfit list (4.4) | — |
| 17 | `dragdrop_pistol` | 38, 118, 156 × 104 | slot-1 list | — |
| 18 | `dragdrop_automatic` | 237, 118, 312 × 104 | slot-2 list | — |
| 19 | context menu (section 5.6) | (shown on demand) | | |
| 20 | `time_static` | 15, 730, 200 × 30 | path `ui\ui_dg_time` (256 × 64 px) with texture rectangle (0, 0, 190 × 36), drawn 190 × 36 | `ui_inv_time` «Время»: offset (20, 8), `letterica16`, (240, 217, 182) |
| 21 | `time_static_str` (`time_static`) | 130, 737, 107 × 30 | — | game time (4.7): `letterica16`, (240, 217, 182), left, top |
| 22 | `exit_button` | 870, 725, 157 × 48 | push button, looks `ui_button_main02_e` (0, 662), `_h` (0, 758), `_t` (0, 710), `_d` (0, 614), each 157 × 48 in `ui\ui_common` | `ui_inv_exit` «Выход», `graffiti22`; colours `e` (240, 217, 182), `t` (0, 0, 0), `h` (0, 0, 0), `d` (135, 123, 116) |

Notes:

- The element texts are string keys (`text` section 1). `bag_static`'s own text `ui_inv_bag`
  («Рюкзак») is replaced by the weight line whenever the screen is rebuilt (section 4.5), so it is never
  seen. `money_static`'s and `time_static_str`'s texts are set every frame (4.7).
- The `text` colour of `exit_button` (238, 153, 26) is overridden by its `text_color e`.
- Multiplayer-only elements (`progress_back_rank`, `progress_bar_rank`, `rank`, `rank:pic`) exist in the
  file but are built only in multiplayer game types; they are not part of this screen.
- The `action_sounds` element is read for sounds (section 7); `properties_box` for the context menu.

Wide-screen layout (`inventory_new_16.xml`, chosen by the `_16` rule), differences only (everything else
as above):

| Element | Abs. rectangle | Other differences |
|---|---|---|
| `belt_slots` | 127, 80, 771 × 172 | `stretch` on: drawn 771 × 172 |
| `back` | 8, 243, 1000 × 483 | `stretch` on: drawn 1000 × 483 |
| `bottom_static` | 0, 736, 1024 × 32 | `stretch` on (the element is written later in the file; the attachment order is still that of the table above, which does not depend on the file) |
| `bag_static` | 11, 240, 400 × 400 | `stretch` on (no picture); text offset (22, 8), `graffiti19` |
| `money_static` | 744, 530, 81 × 33 | `graffiti19` |
| `descr_static` | 370, 253, 330 × 475 | text offset (19, 0), `graffiti19` |
| `character_frame_window` | 730, 260, 202 × 460 | title (50, −8) relative, 100 × 60, `complex_mode` 1, `graffiti19` |
| `static_personal` | 731, 545, 95 × 260 | |
| `progress_background` | 739, 337, 44 × 179 | `stretch` on: drawn 44 × 179 |
| `progress_bar_health` / `_psy` / `_radiation` | 745 / 759 / 773, 359, 8 × 124 | |
| `outfit_info` | 740, 550, 260 × 180 | caption static at (130, 0); scroll view (8, 33, 236 × 147); protection statics 226 × 15, text offset x 13 |
| `dragdrop_bag` | 33, 286, 295 × 415 | 9 × 10 cells of 31 × 41 |
| `dragdrop_belt` | 600, 136, 225 × 60 | 5 × 1 cells of 45 × 60 |
| `dragdrop_outfit` | 858, 290, 78 × 260 | 4 × 4 cells of 43 × 57 |
| `dragdrop_pistol` | 150, 118, 117 × 104 | 3 × 2 cells of 39 × 52 |
| `dragdrop_automatic` | 300, 118, 235 × 104 | 6 × 2 cells of 39 × 52 |
| `time_static` | 11, 730, 151 × 30 | `stretch` on: picture drawn 151 × 30; text offset (15, 6) |
| `time_static_str` | 107, 736, 80 × 30 | |
| `exit_button` | 850, 725, 157 × 48 | `graffiti19` |

### 3.2 The item lists

Each `dragdrop_*` element is an **item list**: a grid of cells. Attributes (`layout` 9.2 names them):
`cell_width`, `cell_height` (units), `cols_num`, `rows_num`, `unlimited` (default 0), `group_similar`
(default 0), `custom_placement` (default **1**).

| List | Shows | Grid (normal) | `unlimited` | `group_similar` | `custom_placement` |
|---|---|---|---|---|---|
| `dragdrop_bag` | the backpack, plus the item in slot 3 (grenades) | 7 × 10 of 41 × 41 | 1 | 1 | 1 |
| `dragdrop_belt` | the belt | 5 × 1 of 60 × 60 | 0 | 0 | 1 |
| `dragdrop_outfit` | slot 6 (outfit) | 4 × 4 of 57 × 57 | 0 | 0 | 0 |
| `dragdrop_pistol` | slot 1 | 3 × 2 of 52 × 52 | 0 | 0 | 0 |
| `dragdrop_automatic` | slot 2 | 6 × 2 of 52 × 52 | 0 | 0 | 0 |

The items in slots 0 (knife), 4 (binoculars), 5 (bolt), 7 (PDA), 8 (detector), 9 (torch) and 10 are not
shown anywhere on the screen and cannot be reached through it. Slot 3's item is shown in the backpack list
but stays in slot 3 (section 5.5).

- **Grid area.** The cells form a block of `cols_num · cell_width` × `rows · cell_height` units whose
  top-left is the list's top-left, where *rows* is `rows_num` at a rebuild and may grow (below). Cell
  (*c*, *r*) (column, row, 0-based) has its top-left at (*c* · `cell_width`, *r* · `cell_height`).
- **Growth.** When an item has to be placed and no free room exists: if `unlimited` is 1, one row is
  added and the search is repeated; otherwise all items of the list are removed and placed again one by
  one (in their current internal order, **unestablished**), and if there is still no room the original
  stops with a fatal error. (With the shipped data the second case cannot happen: slot lists hold one
  item and the belt test of section 5.4 keeps the belt within its 5 cells.)
- **Scrolling.** A vertical scroll bar (`options` 4.8, profile `default` of `config\ui\scroll_bar.xml`) at
  the list's right edge, inside the list rectangle, is shown only while the grid block is taller than the
  list. The block is drawn clipped to the list rectangle and moves up as it is scrolled; the mouse wheel
  over the list scrolls it while the bar is shown. The scroll step is `cell_height` / 3 (integer
  division) — **believed** (the value given to the bar; its per-notch use was not verified). Only the
  backpack list can need it: 10 rows are 410 units, within its 442 units; from 11 rows (451) the bar is
  shown.
- **Cell backgrounds.** Every cell of the visible part of the grid block is drawn as a quad of the cell's
  size with the left half of `textures\ui\ui_grid.dds` (texture coordinates (0, 0)–(0.5, 1)), shader
  `hud\fog_of_war` (`layout` section 8), vertex colour opaque white; the quad's screen corners are the
  cell's corners mapped to pixels (`layout` 3), rounded to whole pixels, minus 0.5. The outfit list draws
  no cell backgrounds (section 4.4).
- **Items** are drawn over the cell backgrounds (section 4.2), in placement order.

## 4. Content

### 4.1 Which items each list shows

At a rebuild every list is emptied and filled from the inventory:

1. slot 1's item (if any) → `dragdrop_pistol`;
2. slot 2's item → `dragdrop_automatic`;
3. slot 6's item → `dragdrop_outfit` (section 4.4);
4. each belt item, in belt order (`pstate` 3.3) → `dragdrop_belt`;
5. the backpack items, sorted (section 4.3) → `dragdrop_bag`;
6. slot 3's item (if any) → `dragdrop_bag`, after the backpack items.

Each item becomes a **cell item** placed as in section 4.3.

### 4.2 Item icons and cell items

Every item section has the integer keys `inv_grid_x`, `inv_grid_y`, `inv_grid_width`,
`inv_grid_height` (read as unsigned 32-bit; a missing key is a fatal error). An item's **grid size** is
*w* × *h* = (`inv_grid_width`, `inv_grid_height`) cells; its **icon** is the rectangle

    (inv_grid_x · 50, inv_grid_y · 50, w · 50, h · 50)  pixels of textures\ui\ui_icon_equipment.dds

(50 pixels per grid unit), drawn with shader `hud\default`.

A cell item occupies *w* × *h* cells and has the size *w* · `cell_width` × *h* · `cell_height` units; its
icon is stretched to that size (texture coordinates as in `layout` 5.3). Examples (normal layout):
`novice_outfit` (0, 18, 2 × 2) → pixels (0, 900, 100 × 100); `wpn_ak74` (5, 5, 5 × 2) → pixels
(250, 250, 250 × 100), drawn 260 × 104 in `dragdrop_automatic`; `medkit` (6, 14, 2 × 1) → pixels
(300, 700, 100 × 50), drawn 82 × 41 in the backpack.

- **Weapon addons on the icon.** A weapon cell item additionally draws, for each addon whose status is 2
  (attachable) and whose installed bit is set (`pstate` 3.1), the addon's icon over its own (permanent,
  status-1 addons are part of the weapon's own icon): the icon of the item section named by the weapon's `scope_name`, `silencer_name` or
  `grenade_launcher_name` (believed: the addon sections are the ones named there), at the weapon's
  offset `scope_x`/`scope_y`, `silencer_x`/`silencer_y` or `grenade_launcher_x`/`grenade_launcher_y`
  (pixels of the weapon's icon). Offsets and sizes are scaled from icon pixels to units by the cell item's
  size divided by the weapon icon's pixel size, per axis. Order: silencer, scope, grenade launcher. The
  addons themselves are **dependent** (weapons).
- **Count text.** A cell item's text is drawn with the default text attributes (`letterica16`, opaque
  white, left, top, offset 0, simple mode; static: no other attributes are set for it):
  - a cell that groups *n* ≥ 2 items (section 4.3) shows `x`*n* (e.g. `x2`); a single item shows nothing;
  - an ammunition cell (item class `AMMO`) shows the sum of the current rounds of its boxes (all grouped
    boxes included), as a decimal number, also for a single box (a box's rounds start at its section's
    `box_size`; how rounds change is **dependent**). Believed: a further condition under which the text
    stays empty exists in the program but was not established; it is not met by ordinary boxes.
- **No highlight.** No highlight of the selected or hovered cell was found.

### 4.3 Placement, grouping and order

- **Placing a cell item** (at a rebuild, or when an item is put into a list without a position): if the
  list groups similar items (`group_similar` 1) and it already holds a cell item that is similar
  (below), the new item joins that cell (the cell's count grows; no room is used). Otherwise the item
  goes to the first free room: the rows are scanned from the top, and in each row the columns from the
  left; the first top-left cell (*c*, *r*) with *c* + *w* ≤ columns, *r* + *h* ≤ rows and all *w* × *h*
  cells free is used (growth of section 3.2 if none).
- **Placing at a position** (a drop, section 5.3): the target cell is the cell containing the top-left
  corner of the dragged picture: *c* = trunc((*X* − *GX*) / `cell_width`), *r* likewise, where *X* is the
  picture's absolute top-left and *GX* the grid block's absolute top-left (scrolling included); if that
  cell is outside the grid or the *w* × *h* room there is not free, the item is placed as without a
  position. Grouping is tried first, as above.
- **Similar items:** same item section, and |condition difference| < 0.01; weapons additionally need the
  same installed-addon byte and the same place (`pstate` 6: slot, belt, backpack); ammunition boxes
  additionally pass one further equality test (**unestablished**; believed always true for boxes of one
  section).
- **Backpack order at a rebuild.** The backpack items are sorted before placing; item *a* comes before
  *b* when:
  1. *a*'s grid width is greater; if equal,
  2. *a*'s grid height is greater; if equal,
  3. both have the same item section: *a*'s object id is greater (higher ids first);
  4. different sections: by an order of internal identities that does not depend on the data
     (**unestablished**; a reimplementation may order by section name).

  The sort is not stable (irrelevant: no two different items compare equal except by rule 4).
- **Example** (normal layout, 7 columns). Backpack: `wpn_ak74` (5 × 2, id 100), `medkit` (2 × 1, id 101,
  condition 1), `medkit` (id 102, condition 1), `bread` (1 × 1, id 103). Sorted: `wpn_ak74`, `medkit` 102,
  `medkit` 101, `bread`. Placed: `wpn_ak74` at cell (0, 0) (cells 0–4 × 0–1); `medkit` 102 at (5, 0); `medkit`
  101 joins it (text `x2`); `bread` at (5, 1). Absolute top-lefts: (37, 273), (242, 273), (242, 314).
- **After moves.** Moving items (section 5) places or removes single cell items; the lists are not
  re-sorted until the next rebuild.
- **Rebuilds** happen when the screen opens and, while it is open, on the first frame after an item
  enters or leaves the inventory through the game's ownership changes (taking, dropping; believed also
  for items used up or attached as addons). A rebuild also clears the selection.

### 4.4 Outfit list

`dragdrop_outfit` shows a **character picture** instead of drawing its cells: a static covering the whole
list (0, 0, 104 × 260), stretched, whose texture id is the worn outfit's `full_icon_name` (`config\misc\
outfit.ltx`; e.g. `npc_icon_novice_outfit` → `ui\ui_icons_npc` (0, 704, 128 × 320)), or
`npc_icon_without_outfit` (`ui\ui_icons_npc` 384, 384, 128 × 320) when no outfit is worn. Only this
picture is drawn by the list: neither cell backgrounds nor the outfit's cell item are drawn, but the cell
item exists (at cell (0, 0), *w* × *h* cells, e.g. 114 × 114 units for `novice_outfit`) and receives the
mouse like any cell item, so the outfit can be selected, dragged, double-clicked and right-clicked over
that area.

### 4.5 Weight line

At every rebuild and after every drop (section 5.7) the text of `bag_static` (complex mode) is set to

    <L><C><M as %3.1f> %c[UI_orange]/<W as %5.1f>

- `<L>` = `%c[default]` + the translation of `ui_inv_weight` («Вес») + one space;
- `<C>` = `%c[red]` if *M* > *W*, else `%c[UI_orange]`;
- *M* = the carried weight, *W* = the carry limit (`max_weight` + the worn outfit's
  `additional_inventory_weight2`), both of `pstate` 3.4; `%3.1f` and `%5.1f` are C formats (one decimal,
  minimum widths 3 and 5, padded with spaces on the left).

So "Вес " is drawn in the element's colour (231, 153, 22) and the numbers in `UI_orange` (238, 153, 26),
the carried weight in red (255, 0, 0) when over the limit. New Game: `%c[default]Вес %c[UI_orange]4.3
%c[UI_orange]/ 50.0`, shown as «Вес 4.3 / 50.0».

### 4.6 Item information panel

The panel is a window covering `descr_static` (0, 0, 335 × 475 relative to it; 330 × 475 wide) built from
`inventory_item.xml` (no `main_frame` element exists, so the panel keeps that rectangle). It is drawn only
while an item is selected (section 5.1); its parts are attached in this order (relative to
`descr_static`; wide values in brackets):

| Part | Rectangle | Content when an item is selected |
|---|---|---|
| `static_name` | 23, 188, 185 × 30 [139 × 30], `stretch` | the translation of the item's `inv_name`; `letterica18`, (238, 155, 23), complex mode (word-wrapped to the width), left, vertically centred |
| `static_weight` | 23, 222, 90 × 20 [67 × 20] | the item's weight formatted `%3.2f kg` (two decimals, Latin `kg`), e.g. `0.30 kg`; `letterica18` [`letterica16`], (135, 128, 122), left |
| `static_cost` | 215, 188, 110 × 30 [83 × 30] | `%d RU` of the item section's `cost` (always, in the single-player game); `letterica18`, (238, 155, 23), centred horizontally and vertically |
| `static_condition` | 113, 222, 95 × 20 [72 × 20] | fixed caption `st_condition` «состояние»; `letterica18` [`letterica16`], (135, 128, 122), right aligned |
| `condition_progress` | 215, 227, 107 × 8 [210, 227] | bar (4.7) of value condition · 100 + 1 − 0.00001, range 0–100, horizontal; picture id `ui_mg_progress_efficiency_full` (`ui\ui_hud` 161, 381, 120 × 17); colour from `min_color` (255, 0, 0) to `max_color` (0, 255, 0) |
| `descr_list` | 24, 245, 300 × 193 [20, 245] | a scroll view (`layout` 9.1; `left_ident` 16, other indents 0, scroll bar only when needed) holding, top to bottom: the weapon parameter block `wpn_params` (only for weapons; **dependent**), the artefact parameter block `af_params` (only for artefacts; **dependent**), and the item's description text |
| `image_static` | centre (170, 100), maximum 300 × 160 [226 × 180] | the item's icon (4.2), stretched to its own pixel size (*w* · 50 × *h* · 50 units; width × 0.75301206 when *W*/*H* > 1.3433334), but at most the maximum size per axis; the static stays centred on (170, 100) |

- The **weight** is the item's own weight: `inv_weight` (a weapon includes its installed addons and
  magazine as in `pstate` 3.4).
- The **condition** is the item's condition 0–1 (`pstate` 3.1, Item block).
- The **description text** is the translation of the item section's `description` key (if the key is
  absent, the text is empty), drawn in complex mode (wrapping at the width), font `letterica16`, colour
  (240, 217, 182) (the `descr_list:font` attributes), placed at x 0 of the scroll view (the
  `right_ident` 0, `layout` 9.1), width = 300 − 0 − 16 (list width minus the scroll bar's width when
  shown, minus `right_ident` and `left_ident`); its height is the text's height. The list is emptied
  and refilled each time the selection changes.
- Example, `medkit`: name «Аптечка» (translation of `Medkit`), `0.30 kg`, `150 RU`, condition bar full
  (green), icon 100 × 50 units centred on (170, 100).

### 4.7 Values updated every frame

While the screen is open, every frame:

- **Bars** `progress_bar_health`, `progress_bar_psy`, `progress_bar_radiation` (range 0–100, vertical)
  take the character's health × 100, psy health × 100 and radiation × 100 (`pstate` 2.3), clamped to the
  range.
- **Money:** `money_static` shows `%d RU` of the character's money (`pstate` 2.2; New Game «40 RU»).
- **Time:** `time_static_str` shows the current game time of day as `HH:MM` (two digits each, `:`
  between; game time of `pstate`'s conventions).
- **Outfit protections:** section 4.8.

**Bar drawing** (all bars of this screen). A bar has a range *min*–*max* (attributes `min`, `max`,
default 0), a current drawn value *v* and a direction (`horz` 1 horizontal, otherwise vertical). Its
picture is the bar's `progress` child (a static, sized to the whole bar, picture at its pixel size unless
`stretch`), drawn clipped to the filled part: length *L* = (bar length along its direction) ·
*v* / (*max* − *min*) (if *max* − *min* is below 0.00001, *max* is raised by 0.00001 first); horizontal
bars fill from the left (clip rectangle: left part of length *L*, full height), vertical bars from the
bottom (clip rectangle: bottom part of length *L*, full width). Nothing is drawn when *L* ≤ 0. A bar with
`min_color`/`max_color` tints its picture with the colour interpolated linearly per channel (alpha
included) between them by *v*/(*max* − *min*). A bar with a `background` child draws it first, unclipped
(none here).

- *Animation:* when a new value is set, if the bar was drawn in the previous frame its drawn value moves
  towards the new one by at most (1 − `inertion`) · (*max* − *min*) · (frame time in seconds) per frame
  (`inertion` default 0 here: the whole range per second); otherwise it jumps to the new value. So the
  bars jump to their values when the screen opens and then follow changes smoothly. (Believed: read from
  the program; the frame bookkeeping was not run.)

### 4.8 Outfit protections (`outfit_info`)

`outfit_info` holds the caption static (3.1 row 13) and a scroll view `outfit_info:scroll_view`
(10, 33, 240 × 147 relative; abs. 750, 583; `always_show_scroll` 0, indents 0). Nine **protection lines**
*k* = 0…8 are defined; line *k* uses:

| *k* | Static (`outfit_info:static_…`) | Label key (text) | Outfit key |
|---|---|---|---|
| 0 | `static_burn_immunity` | `ui_inv_outfit_burn_protection` «Ожог» | `burn_protection` |
| 1 | `static_strike_immunity` | `ui_inv_outfit_shock_protection` «Электрошок» | `shock_protection` |
| 2 | `static_shock_immunity` | `ui_inv_outfit_strike_protection` «Удар» | `strike_protection` |
| 3 | `static_wound_immunity` | `ui_inv_outfit_wound_protection` «Разрыв» | `wound_protection` |
| 4 | `static_radiation_immunity` | `ui_inv_outfit_radiation_protection` «Радиация» | `radiation_protection` |
| 5 | `static_telepatic_immunity` | `ui_inv_outfit_telepatic_protection` «Телепат.» | `telepatic_protection` |
| 6 | `static_chemical_burn_immunity` | `ui_inv_outfit_chemical_burn_protection` «Хим. ожог» | `chemical_burn_protection` |
| 7 | `static_explosion_immunity` | `ui_inv_outfit_explosion_protection` «Взрыв» | `explosion_protection` |
| 8 | `static_fire_wound_immunity` | `ui_inv_outfit_fire_wound_protection` «Пулестойкость» | `fire_wound_protection` |

(Lines 1 and 2 use each other's statics, so the shock line shows the strike icon
`ui_inv_icon_strike_immunity` and the strike line the shock icon; the label and the value agree.)

Each static is 230 × 15 [226 × 15], complex mode, picture its `ui_inv_icon_…_immunity` id (18 × 18 px of
`ui\ui_hud`, drawn 18 × 18 at the static's top-left), text offset (17, 0) [(13, 0)], `letterica16`,
colour `ui_clr_1` (240, 217, 182).

Every frame, for each *k* in order:

- *p* = the worn outfit's protection value for *k* × the outfit's condition (0 without an outfit);
- *a* = the character's protection factor for *k* from artefacts on the belt (**dependent**: artefacts;
  1 without artefacts);
- if |*p*| ≥ 0.00001 or |1 − *a*| ≥ 0.00001 the line is shown with the text
  `<label> ` + `<c1> <p·100 as %+3.0f>%` and, if |1 − *a*| ≥ 0.00001, `<c2> <(1 − a)·100 as %+3.0f>%`
  appended, where *c1* is `%c[green]` if *p* > 0 else `%c[red]`, *c2* likewise for 1 − *a*;
  otherwise the line is not shown.
- A line that becomes shown is appended at the end of the scroll view (if it is not already in it); a line
  that becomes hidden is removed from it. Lines are stacked from the top with no gaps (`layout` 9.1), so
  the order is the order in which the lines first appeared (the fixed order above when the screen is first
  shown with an outfit).

New Game (`novice_outfit`, condition 1, no artefacts): seven lines, at y 0, 15, …, 90 of the scroll view:
«Ожог +10%», «Электрошок +10%», «Удар +10%», «Разрыв +10%», «Хим. ожог +10%», «Взрыв +10%»,
«Пулестойкость +10%» (text `Ожог %c[green] +10%` etc.; the number in green); radiation and telepathic
protection are 0.0 and not shown.

## 5. Interactions

### 5.1 Selection

The **selected item** is one cell item (or none). It is set by a left press, a double click, a right press
or the start of a drag on a cell item, and cleared by a rebuild, by eating, dropping or attaching through
the screen (5.7, 5.8). Selecting an item fills the information panel (4.6) with it (only when the
selection changes).

### 5.2 Mouse events on cell items

- **Left press** on a cell item: selects it.
- **Double click** (a second left press within 250 ms of the previous left press on the screen, once per
  frame; `title` 4.4): see 5.5.
- **Mouse movement with the left button held** over a cell item: selects it and starts dragging it.
- **Right press**: selects it and opens the context menu (5.6).
- **Wheel** over a list: scrolls it (3.2).

### 5.3 Dragging

- While dragging, a copy of the cell item's picture follows the cursor: drawn at the cursor position plus
  the offset from the cursor to the cell item's top-left at the moment the drag started (offset truncated
  to whole units), at the cell item's size, with vertex colour (255, 255, 255) and alpha 170 (0xAA). It is
  drawn above the screen (believed). Whether the original cell stays visible during the drag is
  **unestablished**.
- The **target list** is the list whose absolute rectangle contains the cursor (none if none).
- **Left release** ends the drag and acts by the source list *S* (the dragged item's list) and target *T*.
  Lists are classed as *backpack* (`dragdrop_bag`), *belt* (`dragdrop_belt`) or *slot*
  (`dragdrop_pistol`, `dragdrop_automatic`, `dragdrop_outfit`):

  1. *T* is none: nothing happens.
  2. *T* = *S*: if the list has `custom_placement` 1 (backpack, belt), the cell item (with all the items
     grouped in it) is moved to the drop position in the same list (placement at a position, 4.3; the
     inventory is not changed); otherwise nothing happens.
  3. *T* ≠ *S* but of the same class (slot list to slot list): nothing happens.
  4. *T* is the backpack: **to backpack** (5.4) at the drop position.
  5. *T* is the belt: **to belt** (5.4) at the drop position.
  6. *T* is a slot list: if *T* is the list of the item's own slot (`slot` key: 1 → pistol list, 2 →
     automatic list, 6 → outfit list, and only when that slot is not persistent, `pstate` 3.3 —
     slots 1, 2, 6 are not), **to slot with swap** (5.4).

  Then, in cases 4–6 (whether or not the move succeeded), one further effect is tried with the selected
  (dragged) item:
  - if *T* is `dragdrop_outfit`: if the item is eatable (food, drink, medkit, bandage, antiradiation
    drug) it is **used** (5.7) — dragging food onto the character picture eats it;
  - otherwise, if *T* holds at least one cell item and the first cell item in *T*'s placement order is a
    weapon that can take the dragged item as an addon, the addon is **attached** to that weapon (5.8). In
    practice only addons dragged from the backpack onto `dragdrop_pistol` or `dragdrop_automatic` reach
    this (addons have no slot and cannot go on the belt).

### 5.4 Moves between places

The screen moves an item with these operations; each changes the inventory as `pstate` 3.3 defines the
places, moves the cell item between lists and plays a sound (section 7). "Fails" means nothing changes.

- **To backpack** (with or without a drop position): fails if an item with this object id is already in the
  backpack. Otherwise the item leaves its slot or the belt (if it was the item in the hands, it is put
  away: the active slot becomes none) and goes to the end of the backpack, place 3; the carried weight is
  unchanged. The cell item is removed from its list and placed in `dragdrop_bag` (at the drop position if
  given, else first free room or grouped). Sound `snd_item_to_ruck`.
- **To belt** (with or without a drop position): fails unless the item **can go on the belt**:
  1. no item with this object id is on the belt;
  2. the item section has `belt = true`;
  3. the belt holds fewer than `max_belt` (5) items;
  4. the belt items and the new item, sorted by the order of 4.3 (rules 1–4), can all be placed by
     first-free placement in one row of `max_belt` cells (an item of height ≥ 2 never fits).

  (This is the further belt test `pstate` 3.3 left unestablished; for the shipped belt items, all 1 × 1,
  it reduces to rule 3.) Otherwise the item leaves its slot (put away if in the hands) or the backpack and
  is appended to the belt, place 2. The cell item goes to `dragdrop_belt`. Sound `snd_item_to_belt`.
- **To slot** (without swap): fails unless the item **can go to its slot**: its section has a `slot`
  number of an existing slot and that slot is empty. If that slot is 3 (grenades, shown in no slot list),
  nothing further happens, but the operation counts as done (so the alternatives of 5.5 are not tried);
  for slots 0, 4, 5, 7–10 the outcome is **unestablished** (not reachable in ordinary play: their items
  reach this only from the backpack while their own slot is empty). Otherwise (slots 1, 2, 6) the item
  is put into the slot (place 1) and removed from the backpack and the belt; the cell item goes to the
  slot's list. Then the slot is
  requested to become the **active slot** (as `pstate` 3.3 "Active slot", and: if another slot is active,
  its item is put away first; slots that cannot be active ignore the request): the item is taken into the
  hands. Sound `snd_item_to_slot`.
- **To slot with swap:** if the item can go to its slot, as "to slot". Otherwise, if the item's slot is a
  slot whose list exists (1, 2, 6) and is not persistent, the item currently in that slot is first moved
  **to backpack** (without position) and then the move is retried once without swap. Otherwise fails.
  (Example: dragging a second rifle onto the automatic list puts the old rifle into the backpack and takes
  the new one into the hands.) The slot-3 item, shown in the backpack, never moves: slot 3 is persistent
  and it is not in the backpack.

### 5.5 Double click

On a double click on a cell item *I* (after selecting it):

1. if *I*'s item is eatable: it is **used** (5.7);
2. else if *I* is in `dragdrop_bag`: **to slot** (no swap); if that fails, **to belt** (no position); if
   that fails, **to slot with swap**;
3. else (*I* is on the belt or in a slot list): **to backpack** (no position).

### 5.6 Context menu

A right press on a cell item selects it and builds the context menu for its item. Entries, in this order,
each only if its condition holds (string key → shown text):

| # | Entry | Condition | Action |
|---|---|---|---|
| 1 | `st_move_to_slot` «переместить в слот» | not an outfit; the section has a `slot`, that slot is not persistent, and the item can go to its slot (5.4) | to slot with swap |
| 2 | `st_move_on_belt` «повесить на пояс» | `belt = true` and the item can go on the belt | to belt |
| 3 | `st_move_to_bag` «убрать в рюкзак», or `st_undress_outfit` «снять костюм» for an outfit | the item is not in the backpack, it may go to the backpack (every item but the bolt), and it has no persistent slot | to backpack |
| 4 | `st_dress_outfit` «одеть костюм» | an outfit and entry 3 was not added (i.e. the outfit is in the backpack) | to slot with swap |
| 5 | `st_detach_gl` «отсоединить гранатомет» | a weapon whose grenade-launcher status is 2 and whose launcher bit is set | detach (5.8) |
| 6 | `st_detach_scope` «отсоединить прицел» | likewise for the scope | detach |
| 7 | `st_detach_silencer` «отсоединить глушитель» | likewise for the silencer | detach |
| 8 | `st_unload_magazine` «разрядить» | a magazine weapon whose magazine holds rounds, or any weapon grouped in its cell does | unload (5.8) |
| 9 | `st_attach_scope_to_pistol` «прицепить прицел к пистолету» | a scope that the item in slot 1 can take | attach to slot 1's weapon |
| 10 | `st_attach_scope_to_rifle` «прицепить прицел к винтовке» | a scope that slot 2's item can take | attach to slot 2's weapon |
| 9′ | `st_attach_silencer_to_pistol` «прицепить глушитель к пистолету» | a silencer that slot 1's item can take | attach |
| 10′ | `st_attach_silencer_to_rifle` «прицепить глушитель к винтовке» | a silencer that slot 2's item can take | attach |
| 10″ | `st_attach_gl_to_rifle` «прицепить гранатомет к винтовке» | a grenade launcher that slot 2's item can take | attach |
| 11 | `st_use` «использовать» (medkits and bandages, class ids `II_MEDKI`, `II_BANDG`; antiradiation drugs `II_ANTIR`), `st_drink` «выпить» (drinks, `II_BOTTL`), `st_eat` «съесть» (other eatables, e.g. `II_FOOD`) | the item is eatable | use (5.7) |
| 12 | `st_drop` «выбросить» | the section does not have `quest_item = true` | drop (5.7) |
| 13 | `st_drop_all` «выбросить все» | as 12, and the cell groups more than one item | drop all |

(Rows 9–10″ are the addon kinds: an item is at most one of scope, silencer, grenade launcher; a grenade
launcher has no pistol entry.) The texts are string keys of `ui_st_inventory.xml` (`st_drink`:
`ui_st_other.xml`). If no entry applies the menu is not shown (an item with `quest_item = true` and no
other entry gets no menu).

**Look and placement.**

- The menu is a frame window (`options` 4.9) with base `ui_cb_listbox` (the `properties_box:texture`
  value; parts 10 × 10 px in `ui\ui_common` 723–753, 77–107) containing a list at (5, 5) relative to the
  menu.
- Rows are 20 units high, one per entry, in entry order, starting at the list's top (`top_indent` 0); row
  text: one line, the translated entry text, font `letterica16` (from `properties_box:list:font`), colour
  (131, 131, 131); the selected row's text colour is (218, 217, 213) (`font_s`) and it has a highlight line
  `ui_listline` (`saveload` 3.3: `_b` 0, 37, 4 × 18; `_back` 4, 37, 28 × 18; `_e` 32, 37, 4 × 18 in
  `ui\ui_common`) behind the text. The row under the cursor becomes the selected row (hover selects; believed — read as
  the rows' reaction to the cursor entering them).
  Horizontal text placement within the row and the exact highlight geometry are **unestablished**
  (believed: as in the save list of `saveload` 3.4).
- Size: height = *n* · 20 + `top_indent` (0) + `bottom_indent` (10) units for *n* entries; width =
  max(20, `left_ident` (20) + `right_ident` (10) + *T* + 2), where *T* is the widest entry text in units
  (its width in the font, `text` 3.1, divided by the horizontal UI scale *sx*). The list gets the same
  width and height (so it extends 5 units beyond the menu's right and bottom edges). The `item_height`
  attribute (18) of the data is not used by this list.
- Position: with the cursor at (*x*, *y*) and the menu size *w* × *h*, relative to the screen's rectangle
  (0, 0)–(1024, 768), the menu's top-left is the first of: (*x* − *w*, *y*) if *x* − *w* > 0 and
  *y* + *h* < 768; (*x* − *w*, *y* − *h*) if *x* − *w* > 0 and *y* − *h* > 0; (*x*, *y* − *h*) if
  *x* + *w* < 1024 and *y* − *h* > 0; otherwise (*x*, *y*). (The menu prefers to open to the left of the
  cursor.)
- Opening plays `snd_properties`.

**Input while the menu is open.** The menu takes all mouse input: a left press on a row selects it and
chooses it (the menu closes and the row's action runs on the selected item); a left press outside the
menu closes it without action; a right press anywhere closes it. Any key press or release closes it; the
key is then also handled as in section 6.1 (so Esc closes the menu and the screen).

### 5.7 Use, drop

- **Use** (menu entry 11, double click, drag onto the character picture): the selection is cleared; if
  the item is still usable (believed: it has portions left, `pstate` 5 item 8), the character uses it as
  `pstate` section 5 specifies, and `snd_item_use` is played. The item usually leaves the inventory, which
  triggers a rebuild (4.3).
- **Drop** (menu entry 12, or a press of a key bound to `drop` — default `bind drop kG`, DIK 34 — while an
  item is selected): nothing if the item's section has `quest_item = true`. Otherwise the item is marked as
  dropped by the player and released from the inventory: it leaves its place (put away if in the hands),
  the carried weight drops by its weight; `snd_drop_item` is played; the selection is cleared and the
  weight line is updated. What happens to the item in the world (it is placed into the level near the
  character and falls) is **dependent** (world objects and physics). The `drop` key also works with the
  context menu open (it closes the menu).
- **Drop all** (menu entry 13): the same for every item grouped in the cell and then for the cell's own
  item; `snd_drop_item` is played once per item.

### 5.8 Addons and unloading (dependent)

- **Attach** (menu entries 9–10″, or dropping an addon onto a slot list holding a weapon that can take
  it): `snd_attach_addon` is played; the addon item is installed on the weapon (it leaves the inventory,
  the weapon's installed-addon bit is set); if that weapon was in the hands it is put away and its slot is
  made active again when the screen closes (1.3); the selection is cleared.
- **Detach** (entries 5–7): `snd_detach_addon`; the addon is removed from the weapon and becomes an item
  in the inventory; the same put-away rule.
- **Unload** (entry 8): the rounds of the weapon's magazine (and of every weapon grouped in its cell)
  become ammunition items in the inventory.

Which weapons can take which addons, the addon items' creation and the ammunition boxes are
**dependent** (weapons).

### 5.9 Trading and other windows (dependent)

The trade and body-search windows (`trade.xml`, `carbody_new.xml`) are other screens with their own item
lists; this screen is not used by them. The screen never opens while one of them is open (1.1).

## 6. Keyboard and mouse summary

### 6.1 Keyboard

| Key | Effect |
|---|---|
| bound to `inventory` | closes the screen (1.2) |
| bound to `quit` (Esc) | closes the screen (and the context menu) |
| bound to `drop` (primary or secondary binding) | drops the selected item (5.7) |
| bound to `pause` | pauses the game; the screen then ignores input |
| any key while the context menu is open | closes the menu, then as above |
| any other key | passed to the character's controls (1.3) |

There is no keyboard navigation of the lists, no keyboard selection and no keyboard shortcut on
`exit_button` (it has no `accel`). There is no gamepad support.

### 6.2 Mouse

The cursor moves as in `title` section 7. Left press, movement with the left button held, left release,
double click, right press and wheel act as in sections 5.2–5.6; `exit_button` behaves as a push button of
`saveload` 3.2 (looks, caption colours, activation on release over it) and its activation closes the
screen.

## 7. Sounds

Read from `action_sounds` of the layout file; each is a sound file `sounds\<name>.ogg` played once, as a
listener-relative (2D) sound:

| Element | Value | Played when |
|---|---|---|
| `snd_open` | `interface\inv_open` | the screen opens |
| `snd_close` | `interface\inv_close` | the screen closes |
| `snd_item_to_slot` | `interface\inv_slot` | an item is moved to a slot (5.4) |
| `snd_item_to_belt` | `interface\inv_belt` | to the belt |
| `snd_item_to_ruck` | `interface\inv_ruck` | to the backpack |
| `snd_properties` | `interface\inv_properties` | the context menu opens |
| `snd_drop_item` | `interface\inv_drop` | each dropped item |
| `snd_attach_addon` | `interface\inv_attach_addon` | an addon is attached |
| `snd_detach_addon` | `interface\inv_detach_addon` | an addon is detached |
| `snd_item_use` | `interface\inv_slot` | an item is used |

All nine files exist (`gamedata.db5`). Dragging within a list, selecting and failed moves are silent.

## 8. Messages

The screen shows no message boxes and no hints: failed moves have no feedback (no sound, no text). The
only text that reports state is the weight line (4.5; red when over the carry limit).

## 9. Verification facts

From the game data (Russian edition):

- `config\ui\inventory_new.xml` and `inventory_new_16.xml` both exist; the elements of section 3.1 and their
  rectangles are as tabled; in the normal file `main` is `x=0 y=0 width=1024 height=768` with unquoted
  values. Absolute rectangles of the lists: normal bag (37, 273, 310 × 442), belt (645, 136, 410 × 60),
  outfit (858, 290, 104 × 260), pistol (38, 118, 156 × 104), automatic (237, 118, 312 × 104); wide bag
  (33, 286, 295 × 415).
- Grid blocks (normal): bag 287 × 410 units at New Game (7 × 41, 10 × 41); belt 300 × 60; outfit 228 × 228
  (not drawn); pistol 156 × 104; automatic 312 × 104.
- Texture ids: `ui_slots_belt` (`ui\ui_inventory2` 0, 749, 1024, 172), `ui_inventory_main`
  (`ui\ui_inventory` 0, 0, 1005, 483), `ui_scale_three` (210, 39, 59, 179), `ui_scale_red_1`
  (307, 41, 9, 128), `ui_scale_blue_1` (334, 41, 9, 128), `ui_scale_yellow_1` (316, 41, 9, 128),
  `ui_button_main02_e/_h/_t/_d` (0, 662 / 758 / 710 / 614, 157 × 48 in `ui\ui_common`),
  `ui_mg_progress_efficiency_full` (`ui\ui_hud` 161, 381, 120, 17), `ui_cb_listbox_*` 10 × 10 at
  x 723/733/743, y 77/87/97, `npc_icon_without_outfit` (`ui\ui_icons_npc` 384, 384, 128, 320),
  `npc_icon_novice_outfit` (0, 704, 128, 320). `ui_button_main02` itself is not an id.
- Texture files: `ui_icon_equipment.dds` 1024 × 2048 DXT5, `ui_grid.dds` 128 × 64 DXT5,
  `ui_icons_npc.dds` 2048 × 1024 DXT1, `ui_inventory.dds` and `ui_inventory2.dds` 1024 × 1024 DXT5,
  `ui_bottom_background.dds` 1024 × 32 DXT3, `ui_dg_time.dds` 256 × 64 DXT3.
- Item sections: 309 sections of the configuration tree have all four `inv_grid_*` keys with integer
  values (two further sections have only `inv_grid_width`/`_height`); grid sizes occurring:
  1 × 1 (123), 2 × 1 (80), 5 × 2 (29), 2 × 3 (25), 4 × 2 (18), 3 × 1 (11), 5 × 1 (9), 2 × 2 (6), 4 × 1 (3),
  6 × 2 (3), 6 × 1 (2), 1 × 2 (2). Every icon lies inside the 1024 × 2048 texture except `wpn_colt1911`
  and `mp_wpn_colt1911` ((19, 4, 2 × 1) → x 950–1050 > 1024; clamp addressing repeats the edge texels).
- Icons of the starting items: `novice_outfit` (0, 18, 2 × 2), `wpn_binoc` (16, 8, 2 × 1),
  `device_pda` (6, 13, 2 × 1), `device_torch` (16, 12, 2 × 1), `detector_simple` (9, 14, 1 × 1),
  `bolt` (0, 0, 1 × 1).
- String table texts (section 5.6 and 3.1), e.g. `ui_inv_weight` «Вес», `st_condition` «состояние»,
  `ui_inv_exit` «Выход», `st_drop_all` «выбросить все».
- `[inventory]` `max_belt` 5; `slot_persistent_2`, `_3`, `_7` false (slots 1, 2, 6), others true except
  slot 10 (no key, false).

Derived for a New Game (character state of `pstate` section 2–3, nothing taken since):

- Lists: bag, belt, pistol and automatic empty; the outfit list shows `npc_icon_novice_outfit` stretched to
  104 × 260 units; no scroll bar.
- Weight line `%c[default]Вес %c[UI_orange]4.3 %c[UI_orange]/ 50.0`; money «40 RU»; health and psy bars
  full (124 units), radiation bar empty.
- Seven outfit protection lines (section 4.8).
- Right-clicking the outfit (over the character picture) gives the menu «снять костюм», «выбросить» (two
  rows: 40 + 10 = 50 units high).

Invariants:

- Every list's grid block starts at the list's top-left; cell (*c*, *r*) top-left = (*c* · `cell_width`,
  *r* · `cell_height`).
- After a rebuild, the cell items made from backpack items (slot 3's item, placed last, excluded) are in
  non-increasing grid width, and within equal widths non-increasing grid height, in placement order.
- An item appears in at most one list; the items in slots 0, 4, 5, 7, 8, 9, 10 appear in none.
- The context menu height is 20 *n* + 10 for *n* ≥ 1 entries.

## 10. Unestablished and dependent

- Unestablished: the HUD flag cleared while the screen is open (1.3); the order of grouped-item re-placement
  in the growth fallback (3.2); the ammunition similarity test (4.3); the order of different-section items
  of equal size (4.3 rule 4); whether the dragged cell stays visible during a drag (5.3); the row text
  placement and highlight geometry of the context menu (5.6); the scroll step per wheel notch (3.2,
  believed `cell_height`/3); "to slot" for items of slots 0, 4, 5, 7–10 (5.4); a condition that blanks an
  ammunition cell's count (4.2).
- Believed: no frame drawn for `character_frame_window` (3.1); the drag picture is drawn above the screen;
  the bar animation details (4.7); "usable" means portions left (5.7); the addon sections named by
  `scope_name` etc. (4.2).
- Dependent: information portions and scripts (1.3); weapon addons, magazines and ammunition (4.2, 4.6
  `wpn_params`, 5.8); artefact parameters and belt artefact protection (4.6 `af_params`, 4.8); dropped
  items in the world (5.7); trading and body search (5.9); multiplayer elements.

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), owned by the user;
  data from `gamedata.db0`–`gamedata.dbd` resolved by archive precedence with the project's archive tool.
- Data examined: `config\ui\inventory_new.xml`, `inventory_new_16.xml`, `inventory_item.xml`,
  `inventory_item_16.xml`, `af_params.xml`, `color_defs.xml`, all texture-description files listed in
  `system.ltx` `[texture_desc]`, `config\system.ltx` and the item, weapon and outfit sections of its tree,
  `config\default_controls.ltx`, `config\localization.ltx` and the listed string tables,
  `config\gameplay\info_portions.xml`, all scripts (searched for the information portions and for code
  creating the screen: `game_last_cl.script`, `xr_conditions.script`), DDS headers of the
  textures named in section 9, the archive listings of `sounds\interface\`.
- Binary analysis: static analysis with Ghidra 12.2 (headless; disassembly and decompilation) of the
  shipped `bin\xrGame.dll`: the construction of the screen from the layout, its opening, closing, per-frame
  update and input handling, the in-game key dispatch (inventory, quit, pause, drop), the item lists
  (placement, grouping, growth, scrolling, cell drawing, dragging), the cell items and icons, the outfit
  picture, the weight line, the information panel, the outfit protection lines, the context menu (entries,
  conditions, size, placement, input), the item moves between places and the slot activation they request,
  the eating, dropping and addon requests, the bar drawing, the item and outfit configuration reading;
  `bin\XR_3DA.exe` for a HUD flag (without result). The program was not executed.
- Throwaway check: a separate script in the dirty workspace built the element list of section 3.1 (both
  variants), the list grids and the texture rectangles from the XML and texture descriptions using only
  this text and the approved specs, and recomputed the facts of section 9.
- No leaked source code, derived projects or third-party documentation of the engine were used.
