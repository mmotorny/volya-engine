# Script API G3: window description files, fonts and textures

What the window-description names of the game scripts' interface do in S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): the class `CScriptXmlInit` (a reader of one window description file that builds windows from
it, with the unlisted method `InitProgressBar`), the classes `CGameFont` and `TEX_INFO`, the global functions
`GetFontSmall`, `GetFontMedium`, `GetFontDI`, `GetFontGraffiti19Russian`, `GetFontGraffiti22Russian`,
`GetFontGraffiti32Russian`, `GetFontGraffiti50Russian`, `GetFontLetterica16Russian`, `GetFontLetterica18Russian`,
`GetFontLetterica25`, `GetTextureName`, `GetTextureRect`, `GetTextureInfo` and `GetARGB`, and the function
`game.translate_string`. This is spec G3 of the approved scoping specification `docs/behavior/script-api-scope.md`
(section 6, order 12), which assigns these names to it; its scope rule applies: this spec owns every name of these
classes and functions as the binary registers them, listed or not.

Builds on (not repeated here):

- `docs/formats/ui-layout.md` (here **UL**): window description files (`config\ui\*.xml`), the `_16` rule, the
  document model, paths and indices, required elements and "XML node not found" (1.4), colours (2), the
  window, static, texture and text attributes (4, 5, 7), texture descriptions (6), scroll lists (9). "The game
  stops" has UL section 1's meaning, with its review note.
- `docs/formats/text-and-fonts.md` (here **TF**): the string table and its lookup (1.2, 1.4), the 11 UI fonts
  and their `config\fonts.ltx` sections (2.1).
- `docs/behavior/title-menu.md` section 4 (the item list `shniaga_wnd`).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched
  and what a mismatched call does). Where a statement needs A's ground it says **depends on A**.
- Spec E, `docs/behavior/script-api-value-types-and-math.md`: `Frect` (5.1), f32 and its rounding (2.2).
- The window classes the methods return (`CUIWindow`, `CUIStatic`, `CUIButton`, … ) are the subjects of specs G1
  and G2 (scoping spec section 6); this spec says which object a method makes, from which description, and where
  it is attached, not what the window then does.

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`); also examined:
  `bin\xrLUA.dll` (290,816 bytes, MD5 `a65ba53867962afc92e0f17409e9cd1f`), `bin\xrXMLParser.dll` (57,344 bytes, MD5
  `28c2beacb3a956b46ce464fb31709cbb`), `bin\xrCore.dll` (856,064 bytes, MD5 `eab71f90f01575eca3500e623873e57c`)
  [WF-1].
- **The listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established
  from the binary's registration code.
- A **reader** is an object made by `CScriptXmlInit()`; **its document** is the window description file it last
  parsed (UL 1.4). A **path** and an **index** address an element of the document as in UL 1.4. A **text** is a Lua
  string; a **number** the runtime's double.
- **The texture registry** is the set of texture descriptions of UL 6 (id → texture file and rectangle).
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (read
  from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was
  run.

**Checks.** Every claim carries a tag [WF-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-window-files/run.py` (`python3 -I run.py`): it reads the binaries' bytes, the evidence of
read-only headless static analysis of them (produced by `gh.py` beside it into `bin/`), the listing, the shipped
scripts and the configuration data, and the scoping spec's own check modules for the item counts. All checks pass
(`ALL PASS 103`, output in `run.out`).

## 1. What is registered

The scoping spec gives this spec 50 listed items: the classes `CScriptXmlInit` (27 methods), `CGameFont` (3
constants) and `TEX_INFO` (2 methods), the 14 global functions above and `game.translate_string`; the loadable
scripts use 38 of these names (with the unlisted `InitProgressBar`), in 26 loadable scripts [WF-2]. The binary
registers exactly these, and one method more than the listing shows [WF-3]:

| Name | Kind | Arguments → result | Section |
|---|---|---|---|
| `CScriptXmlInit` | global class | constructor `CScriptXmlInit()` | 2.1 |
| | methods | `ParseFile(name)`, `ParseShTexInfo(name)` → nothing | 2.2, 5.4 |
| | | `InitWindow(path, index, window)` → nothing | 3.3 |
| | | `InitAutoStaticGroup(path, parent)` → nothing | 3.3 |
| | | 24 methods `Init…(path, parent)` → a new window (table of 3.2), among them the unlisted `InitProgressBar` | 3 |
| `CGameFont` | global class | no constructor, no method; constants `alLeft` 0, `alRight` 1, `alCenter` 2 | 4.2 |
| `GetFontSmall()` … `GetFontLetterica25()` (10 functions) | global | → a `CGameFont` | 4.1 |
| `GetARGB(a, r, g, b)` | global | → number | 4.3 |
| `GetTextureName(id)` | global | → text | 5.1 |
| `GetTextureRect(id)` | global | → a new `Frect` | 5.1 |
| `GetTextureInfo(id, default_id)` | global | → a new `TEX_INFO` | 5.2 |
| `TEX_INFO` | global class | no constructor; methods `get_file_name()` → text, `get_rect()` → a new `Frect` | 5.2 |
| `game.translate_string(key)` | function in `game` | → text | 6 |

The listing lacks only `InitProgressBar` [WF-3]. No script can make a `CGameFont` or a `TEX_INFO` itself; it gets
them only from the functions above [WF-3]. Every argument named path, name, id, key above accepts only an
argument of Lua type string (a number does not match); a number argument only a number; a window argument an object
of `CUIWindow` or a class derived from it, or nil (a const object does not match) [WF-4, WF-8]. Which overload a
call selects and what a mismatched call does **depend on A**.

## 2. The reader and its document

### 2.1 `CScriptXmlInit()`

Makes a new reader with an empty document (no file parsed; UL 1.4 "a document without a root") [WF-5]. The reader
is owned by the script side and destroyed, with its document, when it is collected (**believed**: the binding
library's constructor path gives the object to Lua, and the registered destructor frees it) [WF-5]. Destroying a
reader does not destroy the windows it made (3.1).

### 2.2 `ParseFile(name)`

Loads `config\ui\<name>` into the reader's document as a window description file, with the `_16` rule, include
lines and parsing of UL 1.1–1.3 [WF-5]. A file that cannot be opened is not reported: the call returns and the
document is left as it was (UL 1.1; the game stops at the first element read from a document without a root, UL
1.4); a file that opens but does not parse makes the game stop with an error text naming the file and the parser's
description (UL 1.3) [WF-5].

Calling `ParseFile` a second time on one reader: **unestablished** (the parser was not shown to discard the earlier
document; UL's second review gap). No loadable script does it: each of its 6 calls is on a reader of its own
[WF-11].

## 3. Building windows

### 3.1 What an `Init…` method does

Each of the 24 methods of the table in 3.2 does, in this order [WF-5]:

1. makes a **new** window of the kind in the table;
2. reads its description from the reader's document at *path*, index 0 (the first element of that name; UL 1.4),
   with the reading of 3.2; an element that is not there makes the game stop with "XML node not found" and the
   path (UL 1.4), so these methods never return nil for a missing element;
3. marks the new window as owned by the window it is attached to (its "auto-delete" mark: the parent destroys it
   with itself) [WF-5];
4. attaches it (below);
5. returns it, as an object of the class in the table (nil only if no window could be made) [WF-4].

**Attaching** [WF-5]:

- parent nil: the window is not attached to anything (it is still marked as owned by its parent, so nothing
  destroys it; **believed** to stay allocated until the program ends);
- parent a `CUIScrollView` (or a class derived from it): the window becomes an item of the scroll view's content,
  added last, and the scroll view is marked to lay its items out again (UL 9.1);
- otherwise: the window becomes the parent's last child (UL 4.3: drawn after the earlier children).

Exception: `InitSpinText` always attaches to the parent itself, also when the parent is a scroll view (then the
spin box is a child of the scroll view, not an item of its content) [WF-5].

The returned object is the window itself: scripts that keep it change the attached window. Using it after its
parent destroyed it is **unestablished** (**believed** to touch freed memory).

### 3.2 The 24 methods

"Window reading" (**W**) is UL 4: `x`, `y`, `width`, `height`, `alignment`, the optional `font` and `window_name`
children and the `auto_static` children (UL 4.2; 3.3 here) [WF-6]. "Static reading" (**S**) is W plus the static
attributes of UL 5.1 and 7.1: the `text` child (with `align`, `vert_align`, `complex_mode`, `x`, `y`, `font` and
colour), the optional `text_color` child with `e`, `d`, `t`, `h` colours, the `texture` child (value, rectangle,
colour), `stretch`, `texture_offset`, `heading`, `light_anim`, `la_cyclic`, `la_text`, `la_texture`, `la_alpha`,
`xform_anim`, `xform_anim_cyclic`, `highlight_text` with `hA` `hR` `hG` `hB`, `clipper`, `complex_mode` [WF-6].
"Options item" (**O**): an optional child `options_item` whose `entry` and `group` attributes bind the window to an
options entry (what that binding does belongs to the options spec, G5) [WF-6]. Colour attributes are read as UL 2;
the default of `r`, `g`, `b` is given per item (`a` defaults to 255 everywhere) [WF-6].

| Method | Makes a | Returned as | Description read (beyond the elements named, all children of *path*) |
|---|---|---|---|
| `InitStatic` | static | `CUIStatic` | S |
| `InitAnimStatic` | animated static | `CUIStatic` | S, then `x_offset`, `y_offset` (floats, 0), `frames`, `duration`, `columns`, `frame_width`, `frame_height`, `cyclic`, `autoplay` (integers, 0) |
| `InitCheck` | check button | `CUICheckButton` | S, O |
| `InitButton` | button | `CUIButton` | S, then `accel`, `accel_ext` (key names), `shadow_offset_x`, `shadow_offset_y` (0), `push_off_x` (2), `push_off_y` (3), `hint` (a string-table key) |
| `Init3tButton` | three-state button | `CUI3tButton` | W, `text` and `text_color` as in S, the state textures `texture`, `texture_e`, `texture_t`, `texture_d`, `texture_h`, `texture_offset`, the sounds `sound_h`, `sound_t`, `accel`, `accel_ext`, `shadow_offset_x`, `shadow_offset_y`, an optional `hint` child read as S, `check_mode` (−1: unchanged; 1: on; other: off), `hint` attribute; no `push_off_…` |
| `InitEditBox` | edit box | `CUIEditBox` | W, `text` (font, `align`, key), `text_color` `e`, `d`, `cursor` (r g b default 0), `db_click`, `numonly`, `float` (read only when `numonly` is non-zero), `password`, the `text` child's `x`, `y` as a text offset, `max_symb_count` (0: unchanged), the `texture` value, O |
| `InitCDkey` | key-entry box (an edit box whose class scripts cannot name) | `CUIEditBox` | as `InitEditBox` |
| `InitFrame` | frame window | `CUIFrameWindow` | W, the `texture` value (the frame's texture name), an optional `title` child read as S |
| `InitFrameLine` | frame line | `CUIFrameLineWnd` | `x`, `y`, `width`, `height`, `alignment`, `vertical` (0: horizontal), the `texture` value and its colour (default 255), an optional `title` child read as S |
| `InitLabel` | label | `CUILabel` | as `InitFrameLine`, then its `text` child (font, `align`, key; colour default 255) and that child's `x`, `y` as a text offset (applied when non-zero) |
| `InitSpinNum` | integer spin box | `CUISpinNum` | W, O, `text_color` `e` and `d` (default 0) |
| `InitSpinFlt` | number spin box | `CUISpinFlt` | as `InitSpinNum` |
| `InitSpinText` | text spin box | `CUISpinText` | as `InitSpinNum` (attached as in 3.1, exception) |
| `InitComboBox` | combo box | `CUIComboBox` | `list_length` (4), W, O, `always_show_scroll` (1; on when exactly 1), the `list_font` and `list_font_s` children (font and colour), `text_color` `e`, `d` (default 0) |
| `InitList` | list window | `CUIListWnd` | `x`, `y`, `width`, `height`, `alignment`, `item_height` (0), `active_bg` (0), a `font` child, `scroll_profile` (`default`), `always_show_scroll` (0), `always_hide_scroll` (0), `flip_vert` (0; on when exactly 1) (UL 9.2) |
| `InitScrollView` | scroll view | `CUIScrollView` | W, the scroll-list attributes of UL 9.1 (`right_ident`, `left_ident`, `top_indent`, `bottom_indent`, `vert_interval`, `scroll_profile`, and the flags `inverse_dir` (0), `flip_vert` (0), `always_show_scroll` (1), `can_select` (0), each on when exactly 1), each `text` child made a text item |
| `InitTab` | tab control | `CUITabControl` | W, O, `radio` (0); each `button` child, in document order, made a three-state button read as `Init3tButton` reads (with that child's index), a radio button when `radio` is non-zero, and added to the tab control |
| `InitTrackBar` | track bar | `CUITrackBar` | W, `is_integer` (0: the bar's values are numbers, otherwise integers), O, `invert` (0), `step` (0.1) |
| `InitProgressBar` | progress bar | `CUIProgressBar` | `x`, `y`, `width`, `height`, `alignment`, `horz` (horizontal when exactly 1), `min`, `max`, `pos` (floats, 0; a `pos` below `min` becomes `min`, one above `max` becomes `max`), `inertion` (0); a `progress` child read as S (if absent, nothing more is read: no background, no colours); an optional `background` child (S); an optional `min_color` child (colour, default 255) and, only when `min_color` is present, a `max_color` child read without a presence test (if absent, its attributes are absent and give r g b 255, a 255; UL 1.4) |
| `InitMMShniaga` | the title menu's item list | `CUIMMShniaga` | W, then `shniaga:magnifire`, `shniaga`, `shniaga:left_anim`, `shniaga:right_anim`, `shniaga:left_grating`, `shniaga:right_grating` (each read as S), `buttons_region` (read as `InitScrollView` reads), the value of `shniaga:magnifire:y_offset`, then the item lists of title-menu.md 4.1 |
| `InitKeyBinding` | key-binding list | `CUIWindow` | W, a scroll view at `scroll_view`, a frame at `frame`, labels at `header_1`, `header_2`, `header_3`, then items from `config\ui\ui_keybinding.xml` read with the elements `scroll_view:item_group` and `scroll_view:item_key` of the description |
| `InitServerList` | multiplayer server list | `CServerList` | W, lists (read as `InitList` reads) at `list`, `list_server_properties`, `list_players_list` (and `height2` of `list`), frames at `frame`, `list_server_properties:frame`, `list_players_list:frame`, the font of `list_item:text`, an edit box at `edit_gs_filter` (and its `y2`), labels at `cap_server_properties`, `cap_players_list`, `cap_frags`, `cap_death`, the widths `icon`, `server`, `map`, `game`, `players`, `ping`, `version` of `sizes`, and 7 column headers (three-state buttons at `header`, frame lines at `header_frames`) |
| `InitMapList` | multiplayer map list | `CUIMapList` | W, labels at `header_1`, `header_2`, frames at `frame_1`, `frame_2`, lists at `list_1`, `list_2` (each read as a scroll view with `font`, `font_s` and `align`), three-state buttons at `btn_left`, `btn_right`, `btn_up`, `btn_down` |
| `InitMapInfo` | multiplayer map information | `CUIMapInfo` | W only |

Numbers in parentheses are defaults (UL 1.4: an absent attribute gives its default). A flag attribute not marked
"on when exactly 1" is on when its integer value is non-zero; one so marked is on only for the value 1 (any other
value, including 2 or −1, leaves it off). The readers of the window, static, frame, frame line, list,
button, three-state button, tab control, progress bar, scroll view and animated static each require their element
(UL 1.4); elements described above as optional are skipped when absent [WF-6]. What each window then does with what
it read (state textures and colours, sounds, accelerators, options binding, spin and track ranges, the menu's
behaviour) belongs to G1, G2, G5 and title-menu.md, not to this spec.

Three of the kinds have no class that scripts can name: the animated static, the key-entry box and the key-binding
list are returned as `CUIStatic`, `CUIEditBox` and `CUIWindow` respectively, so a script sees only those classes'
members on them [WF-4, WF-5].

### 3.3 `InitWindow` and `InitAutoStaticGroup`

- `InitWindow(path, index, window)` applies the window reading W to an **existing** window, from the element at
  *path* and *index* (the number truncated toward zero to an integer) [WF-4, WF-5]: it sets the window's
  rectangle (UL 4.1), font and window name (UL 4.2), and attaches the element's `auto_static` children to it (as
  below). It makes no new window and returns nothing. A missing element makes the game stop (UL 1.4). Window nil
  matches the call (1) but is **believed** to crash (the reading writes to the window). Example: `ui_game_over.script`
  (not loadable) calls `xml:InitWindow("main", 0, self)`, giving its script window the rectangle of `main` [WF-11].
- `InitAutoStaticGroup(path, parent)` makes one static for each `auto_static` child of the element at *path*
  (index 0), in document order, each read as S with its own index, names it `auto_static_<n>` (*n* from 0), marks
  it owned and makes it the parent's last child (never through a scroll view's content), and returns nothing
  [WF-5, WF-6]. With no such child it does nothing. Parent nil: **believed** to crash on the first child (the
  attachment calls the parent).

### 3.4 Worked example

`ui_game_over.script` (not one of the 250 loadable scripts) calls `xml:InitStatic("item_" .. n, nil)` for *n* = 0
to 30 and stops at the first nil result [WF-11]. By 3.1 the result is never nil: `config\ui\ui_game_over.xml` has
`item_0`, `item_1` and `item_2`, so the call for `item_3` makes the game stop with "XML node not found" and the path
`item_3`; the three statics made before are attached to nothing (parent nil) and are added to a scroll view by the
script itself [WF-11]. No loadable script passes nil as a parent [WF-11].

## 4. Fonts and colours

### 4.1 `GetFont…`

Each function returns the UI font of one fixed `config\fonts.ltx` section (TF 2.1), the same font that a layout
`font` name selects (UL 7.2) [WF-7]:

| Function | Layout font name | `fonts.ltx` section |
|---|---|---|
| `GetFontSmall()` | `small` | `stat_font` |
| `GetFontMedium()` | `medium` | `hud_font_medium` |
| `GetFontDI()` | `di` | `hud_font_di` |
| `GetFontGraffiti19Russian()` | `graffiti19` | `ui_font_graffiti19_russian` |
| `GetFontGraffiti22Russian()` | `graffiti22` | `ui_font_graffiti22_russian` |
| `GetFontGraffiti32Russian()` | `graffiti32` | `ui_font_graff_32` |
| `GetFontGraffiti50Russian()` | `graffiti50` | `ui_font_graff_50` |
| `GetFontLetterica16Russian()` | `letterica16` | `ui_font_letterica16_russian` |
| `GetFontLetterica18Russian()` | `letterica18` | `ui_font_letterica18_russian` |
| `GetFontLetterica25()` | `letterica25` | `ui_font_letter_25` |

No function returns the font of `arial_14` (`ui_font_arial_14`) [WF-7]. The result is a `CGameFont` object that
refers to the font (it does not own it; nil if the font does not exist) [WF-7]; scripts only pass it to window
methods that take a font (G2). The fonts are created with the game's UI (TF 2.1); calling these functions before
that is **unestablished** (**believed** to read through an absent object and crash). Every call returns a new Lua
object for the same font (**believed**, from the binding library's push of a pointer).

### 4.2 `CGameFont`

A class with no constructor and no method; its constants `CGameFont.alLeft` = 0, `CGameFont.alRight` = 1,
`CGameFont.alCenter` = 2 are the horizontal alignments that window methods take (e.g.
`btn:SetTextAlign(CGameFont.alCenter)`, 5 loadable scripts) [WF-2, WF-3].

### 4.3 `GetARGB(a, r, g, b)`

Each argument is rounded to an integer (by the x87 unit in its current rounding mode, **believed** round to nearest,
E 2.2) and its low 16 bits are kept; the result is

    ((a mod 256) · 2²⁴ + (r mod 256) · 2¹⁶ + (g mod 256) · 2⁸ + (b mod 256)), read as a signed 32-bit integer,

returned as a number [WF-8]. So an alpha of 128 or more gives a negative number. Examples: `GetARGB(255, 255, 255,
255)` = −1; `GetARGB(255, 0, 255, 0)` = −16,711,936; `GetARGB(255, 128, 128, 128)` = −8,355,712; `GetARGB(0, 1, 2, 3)` =
66,051; `GetARGB(127, 0, 0, 0)` = 2,130,706,432; `GetARGB(256, 255, 0, 0)` = 16,711,680 (alpha 0) [WF-8]. The value
is the colour form of UL 2 that window colour methods take (G2).

## 5. Texture descriptions

### 5.1 `GetTextureName(id)`, `GetTextureRect(id)`

They look *id* up in the texture registry (UL 6; ids exact and case-sensitive) [WF-9]:

- `GetTextureName(id)` returns the description's texture file (the `file_name` of its file, e.g.
  `ui\ui_iconsTotal`);
- `GetTextureRect(id)` returns a new `Frect` (E 5.1) of the description's rectangle: x1 = `x`, y1 = `y`,
  x2 = `x` + `width`, y2 = `y` + `height`, each sum rounded to f32 when the description was read [WF-9].

An id not in the registry makes the game stop with an error report "Can't find texture" naming the id [WF-9].

### 5.2 `GetTextureInfo(id, default_id)` and `TEX_INFO`

Returns a new `TEX_INFO` holding the texture file and rectangle of *id* if the registry has it, otherwise of
*default_id*; if neither is in the registry, the game stops with an error report naming *id* [WF-9].
`info:get_file_name()` returns its texture file as a text (nil if it holds none, **believed**: the method returns no
text for an empty file name); `info:get_rect()` returns a new `Frect` with its rectangle as in 5.1 [WF-9].

Worked example (measured): `config\ui\ui_iconstotal.xml` (`file_name` `ui\ui_iconsTotal`) describes
`ui_iconsTotal_found_thing` as x 0, y 94, width 83, height 47, so `GetTextureInfo("ui_iconsTotal_found_thing",
"ui_iconsTotal_found_thing")` holds `ui\ui_iconsTotal` and the rectangle (0, 94, 83, 141); `_g.script`'s
`get_texture_info` turns that into (0, 94, 83, 47) by subtracting x1 and y1 [WF-9].

### 5.3 The registry

The registry is the one the main menu fills from `[texture_desc] files` (UL 6.1) [WF-9]. When it is cleared is
**unestablished** here. A lookup before the main menu has loaded the descriptions finds only what
`ParseShTexInfo` added.

### 5.4 `ParseShTexInfo(name)`

Loads `config\ui\<name>` (with the `_16` rule) as a texture description file (UL 6.2) into the registry: for
each `texture` child of the root, in document order, the id, the file's `file_name` and the rectangle of 5.1 [WF-5,
WF-9]. An id already in the registry keeps its earlier description (UL 6.2: the first definition wins) [WF-9]. No
loadable script calls it, `GetTextureRect` or `GetTextureName` [WF-11].

## 6. `game.translate_string(key)`

Returns the string-table text of *key* (TF 1.4): the table's text when *key* is an entry, otherwise *key* itself
(also for the empty key, which gives the empty text) [WF-10]. The table is built on the first lookup if no earlier
lookup built it (TF 1.2) [WF-10]. A missing non-empty key also writes the line `[string table] '<key>' has no entry`
to the log when a setting of the string table is on; which setting that is and whether it is on in the shipped game
is **unestablished** [WF-10]. The text is the one stored, with the `$$ACTION_…$$` substitution of TF 1.6 already
made; `%c[…]` and `\n` in it are interpreted only where a window draws it in complex mode (TF 4.3). Example: the
loadable `news_manager.script` builds news texts as `"%c[255,160,160,160]" .. game.translate_string("st_tip") ..
"\\n" .. "%c[default]" .. …` [WF-2].

## 7. Use by the loadable scripts (measured, [WF-2])

Of the names of this spec, the loadable scripts use: `InitStatic` (14 scripts), `Init3tButton` (10), `GetFontMedium`
(10), `InitFrame` (9), `InitCheck` (8), `CScriptXmlInit`, `ParseFile`, `CGameFont` (6 each), `CGameFont.alCenter`
(5), `InitLabel`, `InitTab`, `InitTrackBar`, `game.translate_string` (4 each), `CGameFont.alLeft`, `GetARGB`,
`GetFontLetterica16Russian`, `GetFontLetterica18Russian`, `InitComboBox`, `InitEditBox` (3 each), `InitFrameLine`,
`InitList`, `InitSpinNum`, `InitProgressBar` (2 each), and once each `GetFontDI`, `GetFontLetterica25`,
`GetTextureInfo`, `get_file_name`, `get_rect`, `InitAnimStatic`, `InitCDkey`, `InitKeyBinding`, `InitMMShniaga`,
`InitMapInfo`, `InitMapList`, `InitScrollView`, `InitServerList`, `InitSpinFlt`, `InitSpinText`. Not used by them:
`InitWindow`, `InitAutoStaticGroup`, `ParseShTexInfo`, `GetTextureName`, `GetTextureRect`, `GetFontSmall`,
`GetFontGraffiti19Russian`, `GetFontGraffiti22Russian`, `GetFontGraffiti32Russian`, `GetFontGraffiti50Russian`,
`CGameFont.alRight`.

## 8. Verification facts

- Items: 50 listed, 38 used names, 26 loadable scripts [WF-2]; 28 methods of `CScriptXmlInit` (the listing lacks
  `InitProgressBar`), 10 `GetFont…` functions, `CGameFont` constants 0, 1, 2 [WF-3].
- Results: the table of 3.2 [WF-4]; 24 control-making methods set the owned mark, 23 attach through the
  scroll-view-aware rule, `InitSpinText` directly [WF-5].
- Fonts: the table of 4.1, all 11 layout names resolved [WF-7]. `GetARGB` examples of 4.3 [WF-8]. Texture example of
  5.2 [WF-9].

## 9. Unestablished

- A second `ParseFile` on one reader (2.2).
- What happens to windows made with a nil parent, and use of a returned window after its parent destroyed it (3.1).
- `InitWindow` with a nil window and `InitAutoStaticGroup` with a nil parent (believed crashes, 3.3).
- `GetFont…` before the UI fonts exist; whether each call gives a distinct Lua object (4.1).
- When the texture registry is cleared (5.3).
- The setting that enables the "has no entry" log line (6).
- The behaviour of each window after it is built (the subject of G1, G2, G5 and title-menu.md).

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); binaries as
  identified above; scripts and configuration as the winning copies of `gamedata.db0` … `gamedata.dbd`, extracted
  with the project's archive tool.
- Analysis: static analysis with Ghidra (headless, read-only) of the game library (the registration code of these
  names, the binding layer's argument matching and result conversion for them, the functions they call: the
  window-description readers, the attachment, the font table, the texture registry and the string table), of the
  XML parser library (file loading), of the core library (its shared-string store) and of the runtime library (string
  push); byte reads of strings and constants; a survey of the shipped scripts (uses, worked examples) and of
  `config\ui\` files. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- `InitAutoStaticGroup` when the reader's document has no element at *path*: not stated (whether it does nothing or stops the game).
- `ParseShTexInfo` with a file that cannot be opened, or a file without a `file_name` child: what is added to the registry is not stated.
