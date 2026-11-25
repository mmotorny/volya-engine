# UI layout data

Functional specification of the data from which S.T.A.L.K.E.R.: Shadow of Chernobyl builds its 2D
user-interface screens, and of how that data is interpreted:

1. the UI layout XML files `config\ui\*.xml` (document syntax, element rectangles, textures and
   texture rectangles, stretching, alignment, rotation, text attributes, colours, lists), the
   1024 × 768 UI space and its mapping to the screen at any resolution and aspect ratio, and the
   wide-screen `_16` variant rule;
2. the texture descriptions (`[texture_desc] files` in `config\system.ltx`): texture ids → texture
   file and pixel rectangle;
3. animated texture sequences (`textures\*.seq`);
4. how UI textures are drawn (blending, alpha, colour modulation, filtering, addressing) as far as
   the data determines it.

Everything the title menu (approved spec `docs/behavior/title-menu.md`, section 10) needs is covered
completely; the general rules are stated so that other screens can be built from the same data.
Further element types are described only briefly (section 9).

Paths are paths in the combined game-data tree (archive specification); where a file exists in
several archives the copy that wins under archive precedence is meant. Text files are
Windows-1251. "Unit" means a unit of the virtual UI space (section 3).

Which elements a screen uses, in which order it attaches them and how it reacts to input is decided
by the code or script that builds the screen (for the title menu: `docs/behavior/title-menu.md`).
This document specifies what the data means once an element is read.

## 1. Files and loading

"The game stops" in this section means: the error text named (where one is named) is written to the
game's log, a crash report is produced and the program ends; the game does not carry on in any form,
and no reader gets a default in its place [EP-E0]. **Disputed in review:** only part of this is established. The error report never returns to the code that raised it, so no reader gets a default and the operation does not continue. That the program then ends is not established: all the game's binaries are built so that catch-all exception handlers also catch the trap raised by the error report (and access faults), they contain many such handlers, and it has not been shown that none of them is active above the point of the error.

### 1.1 Layout files

- Layout files live in `config\ui\`. A screen builder names a file (e.g. `ui_mm_main.xml`) and the
  loader reads `config\ui\<name>` (path alias `$game_config$`, sub-directory `ui`).
- **Wide-screen variant (`_16`) rule.** Let *W* × *H* be the current output resolution. When
  *W* / *H* (computed in single-precision floating point) is **greater than 1.3433334**, the loader
  first forms a variant name: the requested name with its last extension (from the last `.`)
  replaced by `_16.xml` (if the name has no `.`, `_16` is appended and no extension is added). If
  `config\ui\<variant>` exists, it is loaded instead of the requested file. Otherwise, and always
  when the ratio is not greater than 1.3433334, the requested name is used; if that name has no
  `.`, `.xml` is appended.
  - The rule applies to every file loaded from the `ui` sub-directory (directory name compared
    exactly as `ui` or `UI`), including texture-description files (section 6) and `#include`d
    files whose path starts with `ui\` (section 1.2). It is evaluated each time a file is loaded,
    with the resolution current at that moment.
  - Test vectors: 1024 × 768 (1.333), 1280 × 1024 (1.25), 1600 × 1200 (1.333) → normal files;
    1280 × 800 (1.6), 1680 × 1050 (1.6), 1920 × 1080 (1.778), 1366 × 768 (1.779) → `_16` variants.
  - At 1920 × 1080: `maingame.xml` → `config\ui\maingame_16.xml` (exists); `ui_mm_main.xml` →
    `config\ui\ui_mm_main_16.xml` does not exist → `config\ui\ui_mm_main.xml`.
  - The Russian edition has 24 `_16` layout files (`af_params_16`, `car_panel_16`,
    `carbody_item_16`, `carbody_new_16`, `inventory_item_16`, `inventory_new_16`, `maingame_16`,
    `map_desc_16`, `motion_icon_16`, `pda_dialog_character_16`, `skin_selector_16`, `spawn_16`,
    `stalkers_ranking_character_16`, `stats_16`, `talk_16`, `trade_16`, `trade_character_16`,
    `trade_item_16`, `ui_game_ahunt_16`, `ui_game_dm_16`, `ui_game_tdm_16`, `ui_mm_mp_gs_ext_16`,
    `voting_category_16`, `zone_map_16`), each next to its normal file. None of the title-menu
    files and none of the texture-description files has a `_16` variant.
  - The variants keep the 1024 × 768 space (it is stretched to the wide screen, section 3) and
    compensate by narrower authoring: x positions and widths are multiplied by about 0.75 and
    `stretch="1"` is added. Example, `motion_icon.xml` → `motion_icon_16.xml`: `background`
    (0, 640, 125 × 125) → (0, 640, 94 × 125, `stretch="1"`); `state_normal` (27, 22, 65 × 75) →
    (20, 22, 50 × 75, `stretch="1"`); `power_progress` (32, 94, 57 × 7) → (24, 94, 43 × 7).
- **A layout file that can't be opened** (neither the `_16` variant nor the requested name is in the
  data tree): opening it makes no report; the document is left without a root element [EP-L1, EP-L4].
  What follows depends on the builder, but in every case the game stops before the screen is shown:
  - Screens built by the game's scripts open their file through a call that ignores the result, so
    they stop at the first element they read (section 1.4) [EP-L6, EP-L6b, EP-L7]. These are
    `ui_mm_main.xml` (title menu), `ui_mm_opt.xml` (options), `ui_mm_load_dlg.xml`,
    `ui_mm_save_dlg.xml`, `ui_mm_mp.xml`, `ui_numpad_wnd.xml` and `ui_game_over.xml`; each script
    reads an element right after opening its file [EP-L7b].
  - Most screens built by the game's own code check the result and stop the game at once with an
    error text, in most cases "xml file not found" and the file name, among them `message_box.xml` (message
    boxes) and `color_defs.xml` (named colours) [EP-L9, EP-L9b, EP-M1]. The others don't check and
    stop at their first lookup, e.g. `ui_keybinding.xml` (the key-binding list of the options
    screen) [EP-L9, EP-L9c].
  - A file that opens but contains no element (e.g. the three 0-byte files of the Evidence summary)
    behaves like a file that can't be opened once anything is read from it [EP-L4, EP-L5]. **Disputed in review:** this holds for the files that contain only comments, not for the 0-byte files: an empty file is a parse error ("Error document empty."), so the game stops when the file is opened, as in section 1.3.
  - A file that is in the data tree but whose bytes can't be read stops the game while it is read
    (archive specification section 9).

### 1.2 `#include` lines

Before XML parsing, the file is processed line by line (a line ends at CR or LF; runs of CR/LF are
one line break, so empty lines disappear). A line whose **first character is `#`** and which
contains `#include` is replaced by the complete (recursively processed) contents of the file named
between the first pair of double quotes. All other lines are kept as they are.

- If the included path starts with `ui\`, the remainder is passed through the `_16` rule (as a file
  of the `ui` directory) and the result is opened as `ui\<result>` relative to the same root as the
  including file (`config\`). If that fails, or the path does not start with `ui\`, the path is
  opened literally relative to that root. If that fails too, the game stops with an error text
  naming the layout file and the include path [EP-L3].
- Evidence: 24 include lines in the Russian edition, all at column 0 and all of the form
  `#include "ui\<file>.xml"`, e.g. `map_spots.xml` includes ten `ui\map_spots_*.xml` files,
  `ui_mm_mp.xml` includes `ui\ui_mm_mp_tabclient.xml`, `…_tabserver.xml`, `…_taboptions.xml`,
  `…_gs_ext.xml`; `carbody_item.xml` includes `ui\af_params.xml` while `carbody_item_16.xml`
  includes `ui\af_params_16.xml`.

### 1.3 XML syntax accepted

The processed text is parsed by a lenient, non-validating XML parser. Requirements evidenced by the
shipped data (a strict parser rejects 18 of the 154 `config\ui\*.xml` files):

- Attribute values may be in double or single quotes, or **unquoted**; an unquoted value ends at
  white space, CR, LF, `/` or `>` (`<main x=0 y=0 width=1024 height=768/>` in `inventory_new.xml`,
  `trade.xml`, `pda.xml`, `carbody_new.xml`, `inventorymp.xml`).
- Attributes need not be separated by white space (`<text  align="l"x="0" y="0" .../>` in
  `hint_item.xml`; `b="23"complex_mode="0"` in `trade_item.xml`).
- A comment ends at the first `-->`; `--` inside a comment is allowed
  (`<!-- ------------------------------------------------------------------------- -->`).
- The `<?xml … encoding="…"?>` declaration is ignored for decoding: files declaring `utf-8` contain
  Windows-1251 bytes (in comments). No entity references occur in `config\ui\`.
- Leading and trailing white space of element text is not part of the value (inferred from data
  such as `<texture> ui\ui_dg_info</texture>`).
- Not established: how a mismatched end tag is handled (`ingame_msglog_mp.xml` and
  `ingame_msglog_sp.xml` open `<_window>` and close `</window>`). A parse error in a loaded file
  stops the game with an error text naming the file and the parser's description [EP-L2].

### 1.4 Document model and lookup

- The **root** is the first element of the document after `#include` expansion (comments,
  `<?…?>` and `<!…>` declarations are not elements). Its own name is not significant. Counted this
  way over the 154 files of `config\ui\` (each file counted once, as loaded on its own): `<window>`
  in 70, `<ui_texture>` in 23 (exactly the 23 texture descriptions, section 6.1), `<w>` in 22, 29
  other names (`<colors>`, `<map_spots>`, …) in 33, and 6 files have no element at all (section
  "Evidence summary").
- An element is addressed by a **path** of element names separated by `:` relative to the root
  (e.g. `shniaga_wnd:shniaga:magnifire`), plus an **index** that selects the *n*-th (0-based)
  element with the first name among the root's children; every further path component takes the
  first child of that name. Names are compared exactly (case-sensitive).
- The **value** of an element is its text content; when its first child is not text (e.g. it is
  empty or starts with an element), the value is absent and the reader's default applies.
- Attributes are looked up by exact name. Numeric attributes and values are read like C `atoi`
  (integers: leading digits, `"false"` → 0, `"1.5"` → 1) or `atof` (floats); an absent attribute
  gives the default stated for it. A flag attribute is "on" when its integer value is non-zero,
  unless stated otherwise.
- **Required elements.** A builder creates each element it needs (window, static, button, edit
  box, list, scroll view, frame, …) by reading it from its path. If no element is found at that
  path, the game stops with the error text "XML node not found" and the path; this holds for every
  kind of element, whether the screen is built by the game's code or by a script [EP-L8, EP-L8b,
  EP-L6c]. Optional child elements (`texture`, `text`, `font`, `texture_offset`, …) are looked up
  first by the code that reads them and simply skipped when absent; an absent attribute or value
  gives its default without a report.
- **A document without a root** (its file couldn't be opened, or has no element, section 1.1):
  every element, attribute, value or child-count lookup from the root stops the game with an error
  text naming the file; it never yields a default [EP-L4, EP-L5, EP-L5b].
- **Message boxes.** A message box is built by name from `config\ui\message_box.xml`, whose root
  has one child per box (33 boxes, e.g. `message_box_quit_windows`, `select_map`) [EP-M7].
  - If the file can't be loaded, the game stops ("xml file not found") [EP-M1].
  - The first element read is `<name>:picture`, as a required static element: a name missing from
    the file stops the game with "XML node not found" and the path `<name>:picture`
    [EP-M2, EP-M3].
  - `<name>:message_text` is optional [EP-M4]. The attribute `type` of `<name>` is required (absent:
    the game stops) [EP-M5]. Its value is compared without regard to case with `ok`, `yes_no`,
    `yes_no_cancel`, `direct_ip`, `password`, `quit_windows`, `quit_game` and `info`; any other
    value is treated as `ok` [EP-M6]. The buttons a type uses are required elements, e.g.
    `<name>:button_ok` for `ok` and `button_yes`, `button_no`, `button_cancel` for
    `yes_no_cancel` [EP-M8].
  - All 10 box names the shipped scripts use exist in the file [EP-M7].

## 2. Colours

A colour is 8-bit ARGB, read from an element's attributes:

- If the element has a `color` attribute, its value names a colour defined in
  `config\ui\color_defs.xml` (root `<colors>`, children `<color name="…" r g b [a]>`, `a`
  default 255, `r`/`g`/`b` default 0); e.g. `ui_clr_1` = (240, 217, 182). An undefined name gives
  0 (transparent black) — not otherwise established.
- Otherwise the colour is `a`, `r`, `g`, `b` (integers, used modulo 256). `a` defaults to 255; the
  default of `r`, `g`, `b` depends on where the colour is read: 255 for text and font colours and
  for texture colours (so white, opaque by default); 0 for list text-state colours.
- Usage counts in `config\ui\*.xml`: 1,164 elements with `r`/`g`/`b`, 234 with `a`, 704 with
  `color`.

## 3. UI space and screen mapping

- Layouts use a virtual space of **1024 × 768 units**, origin top-left, x to the right, y down.
  Fractional values are allowed (floats).
- The space is mapped to the output of *W* × *H* pixels by independent scale factors
  **sx = W / 1024, sy = H / 768**. The UI always fills the whole screen; at other aspect ratios it
  is stretched (no letterboxing). Wide screens are handled only by the `_16` files (section 1.1)
  and the rotated-quad correction (section 5.4).
- Test vectors: 1024 × 768 → (1, 1); 800 × 600 → (0.78125, 0.78125); 1280 × 1024 →
  (1.25, 1.3333); 1920 × 1080 → (1.875, 1.40625).

## 4. Windows (common to all elements)

### 4.1 Rectangle and position

- `x`, `y`, `width`, `height` (floats, default 0): the element rectangle in units, **relative to the
  top-left corner of its parent's rectangle** (the parent being the element it is attached to by
  the screen builder; top-level windows are relative to the UI space).
- `alignment`: if the value contains the letter `c` anywhere, (`x`, `y`) is the **centre** of the
  rectangle instead of its top-left corner, i.e. the rectangle's top-left is
  (x − width/2, y − height/2) relative to the parent. Other values have no effect. (Used 130 times;
  `alignment="c"` on the title menu's cog wheels.)
- An `align` attribute on the element itself (as opposed to on its `text` child, section 7) has no
  effect on the position.
- **Absolute rectangle** = parent's absolute top-left + own (possibly centre-corrected) top-left,
  same width/height. Example (title menu): `shniaga_wnd` (590, 419) → `shniaga` (9, 0) →
  `left_grating` (0, 4): absolute top-left (599, 423) while the bar is at y = 0.

### 4.2 Optional children read for every window

- `font` child: `font` and colour attributes (section 7.2) give the window's font and font colour
  (used by elements that draw text without a `text` child, e.g. lists).
- `window_name` child: its value becomes the window's name (used by code to find windows).
- `auto_static` children: each `auto_static` child (any number, in document order) is built as a
  static element (section 5) with paths relative to the owning element and is attached to the
  owning window, so it is drawn as one of its children. Example, `car_panel.xml`:
  `<auto_static x="3" y="12" width="19" height="18"><texture>ui_hud_icon_car</texture></auto_static>`
  inside `car_health_static`.

### 4.3 Drawing order and clipping

- A window draws its own content, then its children in attachment order (later on top).
- A static element draws, in this order: its texture, its children, its text. So a static's text
  is on top of its children.
- `clipper="1"`: the static's texture, children and text are clipped to its own absolute
  rectangle. Without it nothing is clipped except to the screen (and to any clip rectangle set by an
  enclosing clipping element, e.g. a list's view area).

## 5. Static elements (picture and/or text)

A static element is a window (section 4) with an optional texture and an optional text. Statics are
the most common element; all title-menu layers except the item list are statics.

### 5.1 Attributes and children

| Where | Name | Type, default | Meaning |
|---|---|---|---|
| element | `x y width height alignment` | section 4.1 | rectangle |
| child | `texture` | value: texture id or texture path | picture (5.2) |
| `texture` | `x y width height` | floats, 0 | texture rectangle in texture pixels (5.2) |
| `texture` | `color` / `a r g b` | 255 each | texture colour, multiplies the texels (5.3, 8) |
| element | `stretch` | int, 0 | non-zero: draw the picture at the element's size (5.3) |
| child | `texture_offset` | `x`, `y` floats, 0 | picture position relative to the element's top-left |
| element | `heading` | int, 0 | non-zero: the picture is drawn rotatable (5.4) |
| element | `clipper` | int, 0 | section 4.3 |
| child | `text` | section 7 | caption |
| element | `complex_mode` | int, 0 | non-zero: forces multi-line text (7.4); 0 here does not switch it off |
| element | `light_anim`, `la_cyclic` (1), `la_text` (1), `la_texture` (1), `la_alpha` (0) | | colour animation by name from the colour-animation library, applied to text and/or texture colour (not specified here) |
| element | `xform_anim`, `xform_anim_cyclic` (1) | | transform animation (not specified here) |
| element | `highlight_text`, `hA hR hG hB` (255) | | highlighted text mode and colour (not specified here) |

Survey of `config\ui\*.xml` (Russian edition): 3,983 elements with `x`, 3,694 with `width`;
1,157 `texture` children with a value: 484 by id without rectangle, 1 by id with rectangle
(`mp_buy_menu.xml`: `<texture x="0" y="0" width="1024" height="768">ui_bm_back</texture>`), 558 by
path without rectangle, 114 by path with rectangle (`ui_mm_save_dlg.xml`:
`<texture x="0" y="0" width="1024" height="430">ui\ui_mm_window_back_crop</texture>`); 237
`stretch`, 18 `heading`, 165 `complex_mode`. A `stretch` attribute on the `texture` child (once in
the data) is ignored.

### 5.2 Which texture and which part of it

- The `texture` value is first looked up as a **texture id** among the texture descriptions
  (section 6; exact, case-sensitive match). If found, the picture uses that description's texture
  file and its pixel rectangle.
- Otherwise the value is a **texture path** (relative to `textures\`, without extension, e.g.
  `ui\ui_mm_main_back_crop`), resolved as in section 6.3, and no rectangle is set.
- If the `texture` child has both `width` and `height` non-zero, its `x`, `y`, `width`, `height`
  set the texture rectangle (pixels), overriding the description's rectangle. If either is 0 the
  attributes are ignored.
- A picture without any rectangle uses the whole texture (0, 0, texture width, texture height) —
  for a video, the whole power-of-two texture in which the frame sits at the top-left.

### 5.3 Picture geometry

Let the element's absolute top-left be (*X*, *Y*) units, its size *w* × *h*, the texture offset
(*ox*, *oy*), the texture rectangle (*u0*, *v0*, *tw*, *th*) in pixels and the texture *TW* × *TH*
pixels (the image's own size; all UI textures in the edition are powers of two and have a single
mip level).

- **Size in units.** With `stretch` on: *w* × *h*. With `stretch` off: *tw* × *th*, i.e. **one
  texture pixel = one unit** (if the texture rectangle is empty, the whole texture's size). The
  picture is not clipped to the element (section 4.3).
- **Screen quad** (non-rotated): left = floor((X + ox) · sx), top = floor((Y + oy) · sy); the quad
  spans from (left − 0.5, top − 0.5) to (left + size_x · sx − 0.5, top + size_y · sy − 0.5) in
  screen pixels. Only the top-left corner is floored; the size is not rounded.
- **Texture coordinates**: (u0 / TW, v0 / TH) at the top-left to ((u0 + tw) / TW, (v0 + th) / TH)
  at the bottom-right.
- **Vertex colour** = the texture colour (section 5.1), used as in section 8.
- Test vectors:
  - `background` of `ui_mm_main.xml` at 1024 × 768: (0, 0, 1024 × 768), id `ui_menu_backgraund`
    → `textures\ui\ui_mainmenu.dds` 1024 × 1024, rectangle (0, 0, 1024, 768); quad
    (−0.5, −0.5)–(1023.5, 767.5), texture coordinates (0, 0)–(1, 0.75). At 1920 × 1080: quad
    (−0.5, −0.5)–(1919.5, 1079.5).
  - `left_grating` (no `stretch`), id `ui_menu_grating_l` (865, 878, 46, 70): drawn 46 × 70 units
    although the element is 55 × 67; texture coordinates (0.8447266, 0.8574219)–(0.8896484,
    0.9257813). With the bar at y = 8 (absolute top-left (599, 431)): at 1024 × 768 quad
    (598.5, 430.5)–(644.5, 500.5); at 1920 × 1080 floor(1123.125) = 1123, floor(606.094) = 606,
    quad (1122.5, 605.5)–(1208.75, 703.9375); at 800 × 600 quad (466.5, 335.5)–(502.4375,
    390.1875).
  - `back_movie` (148, 0, 876 × 400), path `ui\ui_mm_main_back_crop` (video, 1024 × 512 texture),
    rectangle (0, 0, 876, 400): texture coordinates (0, 0)–(0.8554688, 0.78125); drawn 876 × 400.
  - `fire_movie` (525, 459, 128 × 256), path `ui\ui_mm_fire`, no rectangle: whole 128 × 256
    texture, drawn 128 × 256.

### 5.4 Rotatable pictures (`heading`)

When `heading` is on, the picture has a rotation angle *a* (radians, initially 0, set by code) and is
drawn as follows (stretch rules for the size *s_w* × *s_h* as in 5.3):

- The pivot is the centre of the picture, (*s_w*/2, *s_h*/2) relative to its top-left
  *P* = (X + ox, Y + oy) (code can set another pivot).
- Each corner *c* ∈ {(0, 0), (s_w, 0), (s_w, s_h), (0, s_h)} with *d* = *c* − pivot is placed at
  - x = P.x + pivot.x + k · (d.x · cos *a* + d.y · sin *a*)
  - y = P.y + pivot.y + (d.y · cos *a* − d.x · sin *a*)

  in units, then multiplied by (sx, sy) **without** flooring or the −0.5 shift. A positive angle
  turns the picture **counter-clockwise** as seen on screen.
- *k* = 0.8333 when *W* / *H* > 1.3433334 (same test as the `_16` rule), else 1: on wide screens a
  rotatable picture is narrowed horizontally around its pivot (at every angle, including 0).
- Texture coordinates are as in 5.3 plus half a texel: (u0 + 0.5) / TW, (v0 + 0.5) / TH, etc.
- Test vectors (`left_anim`: centre (43, 37) in the bar, 55 × 55, `stretch="1"`, id
  `ui_menu_bigroll_l` (723, 878, 86, 86); bar at (599, 419) so P = (614.5, 428.5), pivot
  (27.5, 27.5)):
  - texture coordinates (0.7065430, 0.8579102)–(0.7905273, 0.9418945);
  - 1024 × 768, *a* = π/2: corners (0,0) → (614.5, 483.5), (55,0) → (614.5, 428.5),
    (55,55) → (669.5, 428.5), (0,55) → (669.5, 483.5) — the top-left corner moved to the bottom
    left;
  - 1024 × 768, *a* = 0.3: (607.601, 437.855), (660.145, 421.601), (676.399, 474.145),
    (623.855, 490.399);
  - 1920 × 1080, *a* = π/2 (*k* = 0.8333): (1160.783, 679.922), (1160.783, 602.578),
    (1246.717, 602.578), (1246.717, 679.922) screen pixels.

## 6. Texture descriptions

### 6.1 File list

`config\system.ltx` section `[texture_desc]`, key `files`: a comma-separated list of names (items
trimmed of surrounding white space). Each name gets `.xml` appended and is loaded as a `ui` file
(section 1.1, so the `_16` rule applies). The Russian edition lists 23 files: `ui_mp_icon_rank,
ui_icons_map, ui_common, ui_old_textures, ui_ingame, ui_map_description, ui_models_multiplayer,
ui_team_logo, ui_team_logo_small, ui_mainmenu, ui_mp_main, ui_statistics, ui_hud, ui_numpad,
ui_inventory, ui_inventory2, ui_iconstotal, ui_pda, ui_icons_npc, ui_npc_monster, ui_npc_unique,
ui_buy_menu, ui_asus_intro`. All 23 files exist. The texture named by `ui_mp_icon_rank`
(`ui\ui_mp_icon_rank`, ids `rank_0`…`rank_4`, referenced by `ui_game_dm.xml` and its `_16` variant)
is **absent** from this edition: no `textures\ui\ui_mp_icon_rank` file of any kind (`.dds`, `.ogm`,
`.avi`, `.seq`) exists in any of the 14 archives (nor in the multiplayer map archive under `mods\`);
the textures of the other 22 files all exist. If the section does not exist, no descriptions are
loaded. The files are loaded once, when the main-menu object is created (before any screen is built).

### 6.2 File format

```xml
<ui_texture>
    <file_name>ui\ui_mainMenu</file_name>
    <texture id="ui_menu_backgraund" x="0" y="0" width="1024" height="768" />
    ...
</ui_texture>
```

- `file_name` (child of the root): the texture path (section 6.3) shared by all entries of the file.
- Each `texture` child of the root: `id` (string), `x`, `y`, `width`, `height` (floats, default 0)
  — a rectangle in pixels of that texture. Ids are matched exactly (case-sensitive).
- If an id is defined more than once, the first definition loaded wins (list order of 6.1, then
  document order). The Russian edition defines 1,131 ids with no duplicates.
- Evidence, `config\ui\ui_mainmenu.xml` (48 entries, `file_name` `ui\ui_mainMenu` →
  `textures\ui\ui_mainmenu.dds`, 1024 × 1024 DXT5): `ui_menu_backgraund` (0, 0, 1024, 768),
  `ui_menu_move` (7, 951, 392, 73), `ui_menu_bigroll_l` (723, 878, 86, 86), `ui_menu_bigroll_r`
  (635, 878, 86, 86), `ui_menu_grating_l` (865, 878, 46, 70), `ui_menu_grating_r` (810, 878, 55, 67),
  `ui_menu_roll_L_01` (0, 768, 55, 55) … Entries per file: ui_common 478, ui_iconstotal 121,
  ui_npc_unique 109, ui_old_textures 95, ui_hud 90, ui_mainmenu 48, ui_buy_menu 29, ui_numpad 22,
  ui_inventory2 21, ui_npc_monster 19, ui_models_multiplayer 18, ui_map_description 16,
  ui_inventory 16, ui_icons_npc 16, ui_statistics 13, ui_mp_icon_rank 5, ui_mp_main 5, others 1–2.

### 6.3 Texture paths and file types

A texture path names a file under `textures\` without extension; matching of file names is
case-insensitive (archive specification).

- **Kind of texture**, in order of preference: `textures\<path>.ogm` (Ogg Theora video, see the
  title-menu spec section 3.2), `.avi` (video), `.seq` (frame sequence, section 6.4), otherwise a
  still image.
- **Still images**: a trailing extension `.tga`, `.dds`, `.bmp` or `.ogm` (case-insensitive) is
  removed from the name, then `<name>.dds` is searched in the current level's directory, then in
  the saved-games directory, then in `textures\`; the first found is used (on the title screen no
  level exists, so effectively `textures\<name>.dds`). A texture found under none of these names
  stops the game with the error text "Can't find texture '<name>'". A texture whose file is found
  but can't be read (archive specification section 9), or whose bytes don't decode as an image,
  stops the game too [EP-R7].
- **When a texture is read.** Either when the texture is first created (when the element that uses
  it is built), or, when the engine defers loading, the first time it is used for drawing (or in a
  batch when a game starts); so in the original a texture file can be read while a frame is drawn.
  Which of the two applies depends on engine state that was not established [EP-R8, EP-R9].
- Shaders: a static's picture uses the shader `hud\default`, except that a path for which
  `textures\<path>.ogm` exists uses `hud\movie` (on hardware with pixel shader model 2.0 or higher;
  a reimplementation can assume this). Section 8.

### 6.4 Animated sequences (`.seq`)

A text file of lines (CR, LF or CRLF; empty lines are skipped):

1. optional line `cycled` (case-insensitive): back-and-forth playback;
2. the frame rate *f* (frames per second, an integer read like `atoi`);
3. one frame texture path per line (surrounding white space trimmed; empty lines ignored). Each
   frame is a still image resolved as in 6.3 (so `fx\fx_wood_fire_1.tga` →
   `textures\fx\fx_wood_fire_1.dds`).

Playback: frame period *p* = 1000 / *f* milliseconds (integer division). With *t* the engine's
continuous millisecond clock (the same clock as the title menu's animations) and *n* frames,
*k* = floor(*t* / *p*):

- normal: frame index *k* mod *n* (0-based, in file order; frames may repeat);
- `cycled`: *m* = *k* mod 2*n*; index *m* if *m* < *n*, else *n* − 1 − (*m* mod *n*) — i.e.
  0, 1, …, *n*−1, *n*−1, …, 1, 0, 0, 1, … (end frames shown twice).

There is no start offset: all sequences run in lockstep with the clock. A sequence is used wherever
a texture path is (UI or not).

Evidence: 11 `.seq` files, none `cycled`:

| File | f | p (ms) | Frames |
|---|---|---|---|
| `textures\ui\ui_ani_cursor.seq` | 19 | 52 | 8: `ui\ui_ani_cursor_01` … `_08` |
| `textures\ui\ui_noise.seq` | 25 | 40 | 11: `ui\ui_noise_` 00 01 02 03 04 03 01 02 00 03 04 |
| `textures\fx\fx_stalker.seq` | 5 | 200 | 30: `fx\fx_stalker_001.tga` … `_030.tga` |
| `textures\fx\fx_wood_fire.seq` | 15 | 66 | 20, with `.tga` extensions |
| `textures\water\water_normal.seq` | 27 | 37 | 29 |
| `textures\artifact\artifact_hell1.seq` | 35 | 28 | 10 |

(also `fx_fire1`, `fx_lavahell1`, `water_dudv`, `water_pool1`, `water_pool2`; some lack a final line
break.) Test vectors, `ui_ani_cursor`: *t* = 0…51 → `_01`; 52 → `_02`; 415 → `_08`; 416 → `_01`;
1000 → *k* = 19 → `_04`. A hypothetical `cycled` sequence of 3 frames shows indices
0, 1, 2, 2, 1, 0, 0, … for *k* = 0, 1, 2, …

## 7. Text

### 7.1 The `text` child

A static's caption is described by its `text` child:

| Attribute / value | Meaning |
|---|---|
| value (element text) | a string-table key; the shown text is its translation (string-table spec). No value: no text set by the data (code may set it). |
| `font` | font name (7.2); absent: the text keeps its current font (default `letterica16`) |
| `color` / `a r g b` | text colour (section 2, default 255 each = opaque white); always applied |
| `align` | `l` left, `r` right, `c` centre (exact one-letter values); other/absent: unchanged (default left) |
| `vert_align` | `t` top, `c` centre, `b` bottom; other/absent: unchanged (default top) |
| `x`, `y` | text offset in units from the element's top-left (default 0) |
| `complex_mode` | 1: multi-line text with colour codes and `\n` (7.4); 0: single line, drawn literally |

Further optional children of the element (not of `text`) give state colours for buttons:
`text_color` with children `e` (enabled), `d` (disabled), `t` (touched), `h` (highlighted), each a
colour (section 2).

Survey: 1,424 `text` children with `font`, 918 with `align`, 82 with `vert_align`, 74 with
`complex_mode`.

### 7.2 Font names

Accepted values of `font` (exact match), 11 in all: `graffiti19`, `graffiti22`, `graffiti32`,
`graffiti50`, `arial_14`, `medium`, `small`, `letterica16`, `letterica18`, `letterica25`, `di`
(`di` is accepted but no layout file of this edition uses it). An absent or empty `font` leaves the
font unchanged. Any other non-empty value is a **fatal error** in the shipped game (an error report
"unknown font" naming the value; the game stops). The data contains three such values, `header`,
`normal` and `graffiti18`, only in `carbody.xml`, `inventory.xml`, `inventorymp_new.xml`,
`inventorymp_new2.xml`, `jobs_new.xml`, `maingame_new.xml`, `map.xml` and `trade_new.xml`; no code,
script or include names these eight files, so they are never loaded (all 18 occurrences are on
`text` children). The fonts themselves (their `config\fonts.ltx` sections, glyph sizes,
per-resolution textures) are specified with the font system; this document uses only a font's line
height and character advance widths, both in screen pixels.

### 7.3 Placement (single line)

Let the element's absolute top-left be (*X*, *Y*), its size *w* × *h*, the text offset (*tx*, *ty*),
and the line height in units *lh* = (font line height in pixels) / sy.

- Horizontal anchor *ax* = X + tx + {0 for left, *w* for right, *w*/2 for centre}; the string is
  drawn left-aligned, right-aligned or centred on that anchor respectively.
- Vertical: *ay* = Y + ty + {0 for top, (*h* − *H_t*)/2 for centre, *h* − *H_t* for bottom}, where
  *H_t* = *lh* for single-line text.
- The font draws at screen position (*ax* · sx, *ay* · sy) (no rounding at this level).
- Text is not clipped to the element unless `clipper` is on.
- Examples: `static_version` (15, 740, 100 × 30), `<text align="l" font="graffiti19" r="135"
  g="123" b="116" complex_mode="0"/>`: anchor (15, 740), left aligned, top, colour ARGB
  (255, 135, 123, 116), at 1920 × 1080 screen (28.125, 1040.625).
- Title-menu item row *i* (700, 419 + 30*i*, 164 × 30): the menu code sets every item caption to
  single-line mode, **horizontal centre and vertical centre** (the layout data gives only their
  font and colour), so the anchor is (782, 419 + 30*i* + (30 − *lh*)/2). With `graffiti22` at
  1024 × 768 (line height 22 px, *lh* = 22): (782, 423 + 30*i*). At 800 × 600 (`_800` variant,
  21 px, *lh* = 26.88): *ay* = 419 + 30*i* + 1.56; for item 0 the font draws at screen y 328.5625.
  The resulting pixels are worked out in the text-and-fonts specification, section 5.

### 7.4 Multi-line (complex) mode

This mode is used only when `complex_mode` is 1. The complete rules (colour codes, line breaks,
wrapping, line placement) are in the text-and-fonts specification
(`docs/formats/text-and-fonts.md`), section 4.3; in brief:

- The text is cut at every colour code and at every literal two-character sequence `\n`
  (backslash, `n`), which ends a line. The resulting pieces (segments) are word-wrapped to the
  element width *w*: a character is accepted while (line width so far + its width + width of the
  letter `o`) ≤ *w* (widths in units = the character's advance in pixels, including the font's
  interval factor, divided by sx). When a character is not accepted, the line is
  broken after the most recent white-space character **of the current segment** (not at the
  segment's position 0); if the segment has none, the line is broken **after the rejected character,
  which stays on the line**. The exact end-of-segment cases are in the text-and-fonts specification.
- Lines are laid out top to bottom with pitch *lh* + interval (interval 0 unless set by code);
  *H_t* = *n* · (*lh* + interval) − interval for *n* lines. Horizontal alignment applies to each
  line separately; vertical alignment uses *H_t*.
- Colour codes are interpreted **only in this mode** (in single-line mode they are drawn
  literally, as are `\n` sequences): `%c[default]` restores the element's text colour,
  `%c[<name>]` selects a colour of `color_defs.xml`, `%c[a,r,g,b]` gives the colour numerically
  (four decimal integers, in that order; the alpha always stays the element's text alpha). The
  shipped string tables do not use them.

## 8. How UI textures are drawn

Shader `hud\default` is defined in `shaders.xr` (`gamedata.dba`) as a "basic (simple)" screen
blender with the properties: Blending = `BLEND`, Texture clamp = on, Alpha ref = 0 (range 0–255),
Z-test off, Z-write off, Lighting off, Fog off, base texture `$base0` (the element's texture). Its
effect:

- **Colour** = texel colour × vertex colour; **alpha** = texel alpha × vertex alpha (vertex colour
  = the texture colour of section 5.1, white opaque by default; colour animations change it).
- **Blending**: result = source · source alpha + destination · (1 − source alpha) (ordinary
  non-premultiplied alpha blending).
- **Alpha test** enabled with reference 0 (the comparison function was not established; with
  reference 0 it can only drop fully transparent texels, which contribute nothing under this
  blending, so it has no visible effect).
- **Filtering**: bilinear minification and magnification (and linear between mip levels, though
  UI textures have one level). **Addressing**: clamp to edge.
- No depth test or write; drawn in UI order (section 4.3) over whatever is already on screen.

Other `hud\*` shaders in `shaders.xr`, same blender type: `hud\cursor` = same as `hud\default`
(the title-menu cursor uses it); `hud\set` = Blending `SET` (no blending: result = texel × vertex
colour, no alpha test); `hud\add` = Blending `ADD` (result = source + destination, no alpha test);
`hud\fog_of_war` = `BLEND` without clamp; `hud\hitmarker` = `BLEND`; `hud\seta` = `SET (2r)`
(not specified).

Shader `hud\movie` (video pictures) is the script `shaders\r1\hud_movie.s` (`gamedata.dba`;
`shaders\r2\hud_movie.s` for the other renderer): no fog, no depth test/write, blending source
alpha / 1 − source alpha, pixel program `yuv2rgb` (`shaders\r1\yuv2rgb.ps`), which converts the
texture's Y, U, V channels to RGB as R = 1.16406 Y + 1.59765 V − 0.86961,
G = 1.16406 Y − 0.390625 U − 0.8125 V + 0.53076, B = 1.16406 Y + 2.01562 U − 1.0786 (values 0–1)
and outputs **alpha 1**. So video pictures are drawn opaque and the vertex (texture) colour does
not affect them. Its sampler filtering was not established.

## 9. Lists

### 9.1 Vertical scroll list (used for the title menu's `buttons_region`)

| Attribute | Default | Meaning |
|---|---|---|
| `x y width height alignment` | | view rectangle (section 4.1) |
| `right_ident` | 0 | x of every item within the list (sic: despite its name it is the left offset) |
| `left_ident` | 0 | read but no layout effect was found |
| `top_indent` | 0 | space above the first item |
| `bottom_indent` | 0 | space below the last item |
| `vert_interval` | 0 | gap before each item (including the first) |
| `inverse_dir` | 0 | 1: the item block is aligned to the bottom of the view |
| `flip_vert` | 0 | 1: items are laid out in reverse order |
| `always_show_scroll` | 1 | the scroll bar is shown always if the value is exactly 1; otherwise only when the items are taller than the view (`"false"`, `"0"` → hidden while the content fits) |
| `can_select` | 0 | 1: items are selectable |
| `scroll_profile` | `default` | scroll-bar look (from `scroll_bar.xml`; not specified here) |
| `text` children | | each becomes a multi-line text item (section 7) whose height is its text height |

Layout: item *j* (0-based, in list order) is placed at x = `right_ident`,
y = `top_indent` + Σ over previous items (height + `vert_interval`) + `vert_interval`; the content
height is `top_indent` + `bottom_indent` + Σ (height + `vert_interval`). Items are drawn clipped to
the view rectangle shortened by `top_indent` at the top and `bottom_indent` at the bottom; only
items intersecting it are drawn.

Title menu: `<buttons_region x="110" y="0" width="164" height="280" always_show_scroll="false"/>`
→ all indents and the interval 0, scroll bar hidden (6–8 items × 30 units ≤ 280); items at
(0, 30*j*).

### 9.2 Other list attributes (brief)

Older list windows read `x`, `y`, `width`, `height`, `item_height`, `active_bg`, a `font` child,
`scroll_profile`, `always_show_scroll` (default **0** for this list type), `always_hide_scroll`,
`flip_vert`. Combo/option lists read `list_length` (default 4), `always_show_scroll` (default 1),
`list_font`, `list_font_s`, `text_color` `e`/`d` (colour defaults 0). Drag-and-drop cell lists read
`cell_width`, `cell_height`, `rows_num`, `cols_num`, `unlimited`, `group_similar`,
`custom_placement` (default 1). Buttons additionally read `accel`, `accel_ext`, `shadow_offset_x/y`,
`push_off_x/y`, `hint`, `check_mode`, `sound_h`/`sound_t` children and an `idention` child
(`push_x/y`, `over_x/y`). Semantics beyond these names were not established.

## 10. Title-menu checklist

What `ui_mm_main.xml` needs, by the rules above:

| Element | Rules applied |
|---|---|
| `background` | 4.1 rectangle; 5.2 id `ui_menu_backgraund`; 5.3 (no stretch, 1024 × 768 px = units); 8 `hud\default` (its alpha lets the sky video show through) |
| `back_movie` | 5.2 path + texture rectangle (0, 0, 876, 400); 6.3 `.ogm` → `hud\movie`, opaque |
| `fire_movie` | 5.2 path, whole 128 × 256 texture; `hud\movie` |
| `shniaga_wnd`, `buttons_region`, `shniaga` | 4.1 relative rectangles; 9.1 list with `always_show_scroll="false"`; `shniaga` texture id `ui_menu_move` 392 × 73 drawn at its pixel size |
| `magnifire` | rectangle and `stretch="1"`, path `ui\ui_magnifier`; `y_offset` value read by code (1.4) |
| `left_anim`, `right_anim` | `alignment="c"` (4.1), `stretch="1"` 55 × 55 from 86 × 86 px, `heading="1"` (5.4) |
| `left_grating`, `right_grating` | no stretch: drawn 46 × 70 and 55 × 67 units from the top-left (5.3) |
| `menu_*` lists | attributes `btn_height`, `font`, `a r g b` and `btn` children `name`/`caption` are read by the menu code (title-menu spec section 4); each caption is drawn single-line, centred horizontally and vertically in its 164 × 30 row (7.3) |
| `menu_sound` | `random` attribute and `menu_music` values read by the menu code |
| `static_version` | 7.1–7.3: `graffiti19`, left, top, ARGB (255, 135, 123, 116), single line |

## 11. Not established

- Handling of malformed XML beyond the tolerances of section 1.3 (mismatched end tags).
- Undefined `color` names (assumed 0), comparison function of the UI alpha test, sampler filtering
  of `hud\movie`.
- The effect of `left_ident` in the scroll list; colour/transform animations (`light_anim`,
  `xform_anim`), highlighted text, tiling and mirroring of pictures (only reachable from code).
- Behaviour on hardware without pixel shader 2.0.

## Evidence summary (Russian edition)

| File | Archive | Notes |
|---|---|---|
| `config\system.ltx` | `gamedata.dbc` | `[texture_desc] files` (23 names) |
| `config\ui\ui_mm_main.xml` | `gamedata.dbb` | title-menu layout |
| `config\ui\ui_mainmenu.xml` | `gamedata.dbb` (identical in `db0`, `dba`) | 48 texture ids |
| `config\ui\color_defs.xml` | `gamedata.dbb` | named colours |
| `config\ui\maingame_16.xml` etc. | `gamedata.dbb` | 24 `_16` variants |
| `shaders.xr` | `gamedata.dba` | `hud\*` shader properties |
| `shaders\r1\hud_movie.s`, `shaders\r1\yuv2rgb.ps` | `gamedata.dba` | video shader |
| `textures\ui\ui_ani_cursor.seq` | `gamedata.db7` | `19`, 8 frames |
| `textures\ui\ui_noise.seq`, `textures\water\*.seq` | `gamedata.db8` | |
| `textures\fx\*.seq`, `textures\artifact\artifact_hell1.seq` | `gamedata.db5` | `.tga` frame names |
| `textures\ui\ui_mainmenu.dds`, `ui_common.dds`, `ui_hud.dds`, `ui_iconstotal.dds`, `ui_inventory.dds` | | 1024 × 1024 DXT5, 1 mip level |
| `textures\ui\ui_magnifier.dds` 256 × 32, `ui_ani_cursor_01.dds` 64 × 64, `ui_noise_00.dds` 256 × 256 | | power-of-two, 1 mip level |

`config\ui\` contains 154 `.xml` files. Counting the first element of each file after `#include`
expansion (comments and declarations skipped; the expansion changes no file's first element): 70
`window`, 23 `ui_texture`, 22 `w`, 33 files with 29 other names, and 6 files with no element:
`follower_character.xml`, `inventory_character.xml` and `pda_dialog.xml` are empty (0 bytes in every
archive that has them), `map_spots_darkvalley.xml`, `map_spots_military.xml` and
`map_spots_rostok.xml` contain only comments.

## Provenance

- Date: 2026-11-23. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG
  distribution) owned by the user; data from `gamedata.db0`–`gamedata.dbd`, resolved by archive
  precedence with the project's archive tool.
- Data analysis: all `config\ui\*.xml`, `config\system.ltx`, `config\text\rus\*.xml` (search for
  colour codes), all 11 `.seq` files, DDS headers of UI textures, `shaders.xr` and the `r1` shader
  scripts; attribute and element surveys with small scripts; comparison of `_16` files with their
  normal counterparts.
- Binary analysis: static analysis with Ghidra 12.2 (headless) of the shipped 32-bit binaries
  `xrGame.dll`, `xrXMLParser.dll`, `XR_3DA.exe`, `xrRender_R1.dll` and `xrCore.dll` (string
  references, decompilation and disassembly of the routines that read the attributes above, load
  texture descriptions and sequences, and draw textured quads and text). No execution or debugging.
- Prior approved specs relied on: `docs/formats/db-archive.md`, `docs/formats/ltx-config.md`,
  `docs/behavior/startup.md`, `docs/behavior/title-menu.md`.
- No leaked source code, derived projects or third-party documentation of the engine was used.
- **Revision 2026-11-23** (reconciliation with the text-and-fonts specification; same edition and
  tools, plus: a survey of all `font` attribute values; a count of the first element of every
  `config\ui\` file after include expansion; a check against the archive listings of every
  texture-description texture and of the sizes of the files without elements; a search of the game
  library, the engine executable, all scripts and all include lines for the layout files that are
  loaded; and a re-reading in the game library of the font-name lookup and its callers, the error
  path, the single-line drawing, the wrapping and the code that builds the title-menu items). Changes
  against the previously approved text:
  - 1.4 and Evidence summary: the root-element counts are corrected (70 `window`, 23 `ui_texture`,
    22 `w`, 29 other names in 33 files, 6 files without any element, three of them 0 bytes and three
    containing only comments; was 60 `window`, 17 `w`) and the counting rule is stated.
  - 6.1: the texture of `ui_mp_icon_rank` (`ui\ui_mp_icon_rank`) is stated to be absent from the
    edition; the other 22 description textures exist.
  - 7.1, 7.2: `di` added (11 accepted names, was 10); an unknown non-empty name is a fatal error;
    an absent or empty `font` keeps the current font; the three unused values `header`, `normal`,
    `graffiti18` and the eight never-loaded files that contain them are listed.
  - 7.3: the title-menu example is corrected: item captions are centred **vertically** as well as
    horizontally by the menu code (was "top"), anchor y 423 + 30*i* at 1024 × 768 (was 419 + 30*i*);
    the same fact is added to the checklist of section 10.
  - 7.1, 7.4: colour codes and `\n` are interpreted only in complex mode (was "both modes"); the
    no-whitespace break is after the rejected character, which stays on the line (was "before" it);
    break points are searched only in the current segment, never at its position 0 (was "the
    line"); the character widths are stated to include the font's interval factor; the full rule is
    referenced to the text-and-fonts specification.
- **Revision 2026-11-25** (what the original does when a layout can't be opened or built, a message
  box is missing, or a texture can't be read; same edition and tools, plus a static reading of the
  error-report path of the core library and the process's exception handling, of the script binding
  layer of the script library, of every place in the game library that opens an XML file, of the
  element initialisers, the message-box builder, the script call that opens layout files, and the
  texture creation and loading of the engine executable and the renderer; and a survey of the
  shipped scripts and of `message_box.xml`). Bracketed ids [EP-…] name the checks in the
  investigation workspace that reproduce each statement from the binaries and data. Changes against
  the previously approved text:
  - 1: "the game stops" defined.
  - 1.1: what happens when a layout file can't be opened (added).
  - 1.2: a missing include file stops the game (was "an error").
  - 1.3: a parse error stops the game (was "fatal"; same meaning).
  - 1.4: required elements, documents without a root and message boxes (was "missing → error").
  - 6.3: what a missing or unreadable texture does, and when texture files are read (added).

## Gaps found in review

- Whether a catch-all exception handler of the engine, active above the point of an error report, keeps the game running after a layout, message-box or texture error (what "the game stops" means at run time; section 1).
- A layout document that already holds a loaded file keeps that file's root when a later open of another file fails (the failed open leaves the root unchanged); section 1.1 describes a document that has not loaded a file before. Whether any builder reuses a document this way is not established.
