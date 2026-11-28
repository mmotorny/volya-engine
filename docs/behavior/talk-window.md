# Talk window layout

Functional specification of the window shown while the player character talks with another character, in
S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): its layout file and wide-screen variant, the elements
it reads and the order they are drawn in, the backgrounds and character pictures, the two vertical frame
lines with their titles, the two character panels, the answer log with its three kinds of entries, the
question list with its colours, the scrolling of both lists, and when the trade button is shown.

It is the spec DG-5 of `dialogue-scope` (section 4) and builds on, without repeating, `ui-layout`,
`text-and-fonts` (`docs/formats/`), `save-load-screens` (buttons), `options-screens` (scroll views and
scroll bars), `pda-controls` (frame lines with titles, buttons), `inventory-screen` and `dialogue-scope`
(`docs/behavior/`), cited by name and section. What a talk offers and who says what are `dialogue-scope`
DG-3 and DG-4; opening, closing and every input of the window DG-6; the lines scripts add (`give_talk_message`)
DG-7; the phrase voices DG-8; what the trade button does DG-10. The values the character panels show (names,
pictures, ranks, communities, relations) wait on the characters scope (#279, B-chars 3) and their texts on
`pda-scope` PD-14; this spec gives the panels as far as their layout goes.

Conventions:

- Every fact carries its check, a script in `checks/talk-window/` of the dirty workspace: `c1` =
  `c1_data.py` (game data), `c2` = `c2_binary.py` (static analysis of `bin\xrGame.dll`), followed by the
  label of the assertion (e.g. c2 B7). `run_all.py` runs both. File paths are archive paths of the winning
  copies; layout files are in `config\ui\`.
- "Element", "window", "static", the absolute rectangle and the drawing order are those of `ui-layout` 4
  and 5; a picture without `stretch` is drawn at its pixel size (`ui-layout` 5.3). Positions and sizes are in
  units of the 1024 × 768 space. "Absolute" gives a rectangle in that space.
- *t* is a font's line height in units (`text-and-fonts` 4.2, 3.6); at 1024 × 768 it is 18 for
  `letterica18`, 16 for `letterica16` and 22 for `graffiti22` (c1 D9). The worked heights below use these.
- "Player character" and "partner" are those of `dialogue-scope`. "Talk start" is the moment the window is
  set up for a new talk (which events lead to it is DG-3's and DG-6's).
- **Believed** marks a reading with its reason; **unestablished** what this work could not settle.

## 1. Files and creation

- The window is built once, when the level's single-player in-game interface is created, in the same step
  as the inventory screen (`inventory-screen` 2) (c2 B1). Its set-up loads `talk.xml` from the `ui` folder,
  so the `_16` rule of `ui-layout` 1.1 applies at that moment and `talk_16.xml` is loaded instead on wide
  screens; if the file cannot be loaded the game stops with "xml file not found" (c2 B1). The loaded document
  is kept: the entries of both lists (sections 6 and 7) read their templates from it each time one is added
  (c2 B1).
- Each character panel (section 5) loads `trade_character.xml` from the `ui` folder in the same way
  (`trade_character_16.xml` on wide screens), with the same stop if it fails (c2 B3).
- Pictures (c1 D2): `textures\ui\ui_top_background.dds` and `ui_bottom_background.dds` (1024 × 32 pixels
  each); the texture ids `ui_charinfo_left` (0, 0, 210 × 218) and `ui_charinfo_right` (813, 0, 211 × 218) of
  `ui\ui_inventory2` (1024 × 1024; `ui_inventory2.xml`); the frame-line parts of section 4 in `ui\ui_numpad`
  (1024 × 1024; `ui_numpad.xml`); the button looks of section 8 in `ui\ui_common` (1024 × 1024;
  `ui_common.xml`).

## 2. The window and its elements

The window is (0, 0) 1024 × 768 and draws nothing of its own, only its children (c2 B1, B2). It is a child of
the talk screen, also (0, 0) 1024 × 768, which holds the trade window as well (DG-10) (c2 B1).

`talk.xml` has 14 top elements (c1 D1); the window reads them in the following order, which is also the
order of attachment and so of drawing, whatever their order in the file (c2 B2). A window draws its shown
children in attachment order, later ones on top (c2 B2, `ui-layout` 4.3).

| # | Element | Read as | Attached to | Rectangle | Absolute |
|---|---|---|---|---|---|
| 1 | `top_background` | static | window | (0, 0) 1024 × 128 | picture (0, 0) 1024 × 32 |
| 2 | `bottom_background` | static | window | (0, 736) 1024 × 32 | (0, 736) 1024 × 32 |
| 3 | `left_character_icon` | static | window | (15, 32) 200 × 218 | picture (4, 32) 210 × 218 |
| 3a | left panel (`trade_character.xml`) | panel, section 5 | `left_character_icon` | (0, 0) 200 × 218 | (15, 32) |
| 4 | `right_character_icon` | static | window | (820, 32) 200 × 218 | picture (820, 32) 211 × 218 |
| 4a | right panel | panel, section 5 | `right_character_icon` | (0, 0) 200 × 218 | (820, 32) |
| 5 | first `frame_line_window` (frame line 0) | frame line, section 4 | window | (218, 42) 600 × 470 | (218, 42)–(818, 512) |
| 6 | second `frame_line_window` (frame line 1) | frame line | window | (218, 485) 600 × 250 | (218, 485)–(818, 735) |
| 5a | `answers_list` (the answer log) | scroll view, section 6 | frame line 0 | (33, 46) 544 × 377 | (251, 88) 544 × 377 |
| 6a | `questions_list` (the question list) | scroll view, section 7 | frame line 1 | (30, 100) 547 × 120 | (248, 585) 547 × 120 |
| 7 | `button` (the trade button) | button, section 8 | window | (437, 726) 157 × 48 | (437, 726)–(594, 774) |

(c1 D1, c2 B2.) The lists are read after both frame lines, the answer log first, but drawn as children of
their frame lines, after each line's band and title (section 4) (c2 B2, B7). Then the window reads the
`auto_static` children of the root (`ui-layout` 4.2; there are none, c1 D1) and two `font` elements of the
root (indices 0 and 1) as a font and colour; the files have none, and nothing of the window uses the result
(c1 D1, c2 B17). The item templates `question_item`, `other_answer_item`, `actor_answer_item`,
`iconed_answer_item`, `iconed_trade_info` are not read at set-up but each time an entry is added (sections 6,
7).

- **The coordinate space of the lists** (the disputed statement of `dialogue-scope` 1.2): `answers_list` is
  attached to frame line 0 and `questions_list` to frame line 1, so their rectangles are relative to the
  frame line's top-left, as the table gives (c2 B2).
- Consequences of the table (from the rules cited): frame line 1 is drawn after frame line 0 and its log
  and is drawn over frame line 0's bottom 27 units (y 485 … 512), where frame line 0's end part lies (section 4);
  the log itself ends at y 465 and is not covered. The trade button extends 6 units below the screen
  (to y 774) and is drawn over the bottom background and frame line 1. The right picture extends 7 units
  past the right edge (to x 1031).

## 3. Backgrounds and character pictures

All four are statics (`ui-layout` 5) without `stretch` in `talk.xml` (c1 D1):

- `top_background`, path `ui\ui_top_background`: the picture is drawn at its 1024 × 32 pixels at (0, 0),
  not at the element's height of 128 (c1 D1, D2).
- `bottom_background`, path `ui\ui_bottom_background`: 1024 × 32 at (0, 736) (c1 D1, D2).
- `left_character_icon`, id `ui_charinfo_left`, with `texture_offset` (−11, 0): drawn 210 × 218 from
  (4, 32). `right_character_icon`, id `ui_charinfo_right`, no offset: 211 × 218 from (820, 32) (c1 D1, D2).

## 4. Frame lines

Both `frame_line_window` elements are frame lines as `pda-controls` 3 reads them (window attributes,
`vertical`, the `texture` child's value as the base and its colour, an optional `title`) (c2 B2). Both give
`vertical="1"`, the bases `ui_talk_dialogue` and `ui_talk_dialogue2`, and no colour, so all parts are opaque
white (c1 D3).

- **Vertical band.** A non-zero `vertical` makes the line vertical (c2 B7). Its length *L* is the element's
  height (c2 B7). The parts are `<base>_b` (start), `<base>_back` (repeated) and `<base>_e` (end) (c2 B7):
  - `_b` at its pixel size at the line's top-left;
  - `_e` at its pixel size at the line's left, its bottom at the line's bottom (top at *L* − its height);
  - `_back` in one column starting just below `_b`: *n* = ⌊*r* / *h*⌋ whole copies of its height *h*,
    *r* = *L* − (height of `_b`) − (height of `_e`), then the remainder *r* − *n*·*h* (c2 B7). That the last
    copy is cut to the remainder at the bottom, not scaled, is **believed**: it is the same tiling as the
    horizontal line of `save-load-screens` 3.3 with the two directions exchanged (c2 B7).
  - The program reports an error if `_b` and `_e` differ in width or *r* is not positive (horizontal lines:
    if they differ in height or the remaining width is not positive) (c2 B7); the talk files meet both.
- **Parts** (c1 D2): frame line 0: `ui_talk_dialogue_b` (2, 369, 600 × 75), `_back` (2, 444, 600 × 96),
  `_e` (2, 540, 600 × 40); frame line 1: `ui_talk_dialogue2_b` (0, 580, 600 × 98), `_back` (0, 444,
  600 × 96), `_e` (0, 540, 600 × 40).
- **Worked bands** (c1 D3): frame line 0, *L* = 470: `_b` y 42 … 117, three copies of `_back` from 117, 213,
  309, a fourth cut to 67 rows from 405 to 472, `_e` from 472 to 512. Frame line 1, *L* = 250: `_b` 485 …
  583, one `_back` 583 … 679, a second cut to 16 rows from 679 to 695, `_e` 695 … 735. All 600 wide from
  x 218.
- **Drawing order.** A frame line draws its band, then its children: the title (attached first, when the
  line is made) and then its list (c2 B7). This settles for these lines what `pda-controls` 3 leaves as
  believed.
- **Titles** (c1 D3): statics read from each line's `title` child: frame line 0's at (25, 3) 190 × 22,
  absolute (243, 45); frame line 1's at (25, 60) 190 × 22, absolute (243, 545); both `graffiti22`
  (238, 153, 26), single line (`complex_mode` 0), left aligned, top, no text in the file.
- **Title texts** (c2 B8): at each talk start frame line 0's title gets the partner's name and frame line 1's
  the player character's name, as they are (not through the string table). The names are #279's (B-chars 3).
- The program also sets two further properties of both title statics (to 2 and 10) and of the panels' value
  statics (section 5; to 2 and 0, or 2 and 1); no part of the static's drawing or text handling reads them,
  so they are **believed** to have no effect (c2 B9).

## 5. Character panels

Each picture element carries a panel: a window at (0, 0) of the picture element's own width and height
(200 × 218; with `talk_16.xml` 150 × 220), attached to it, read from `trade_character.xml` (c2 B2, B3).
Its elements are therefore relative to the picture element's top-left (15, 32) or (820, 32), not to the
offset picture.

- **Elements read** (c2 B4), each only if the file has it, in this order: `icon_static`, `name_static`,
  `rank_static`, `rank_caption`, `community_static`, `community_caption`, `reputation_static`,
  `reputation_caption`, `relation_static`, `relation_caption` (statics) and `biography_list` (a scroll view).
  `trade_character.xml` has eight of them; it has no `reputation_*` and no `biography_list` (c1 D4), so a
  talk shows no reputation and no biography.
- **The file** (c1 D4):

  | Element | Rectangle | Absolute, left / right panel | Text |
  |---|---|---|---|
  | `icon_static` | (18, 35) 165 × 108 | (33, 67) / (838, 67) | — |
  | `name_static` | (23, 7) 153 × 22 | (38, 39) / (843, 39) | `graffiti22` (238, 155, 23), single line |
  | `rank_caption`, `rank_static` | (23, 150) 153 × 15 | (38, 182) / (843, 182) | caption `ui_st_rank` ("ранг") |
  | `community_caption`, `community_static` | (23, 164) 153 × 15 | (38, 196) / (843, 196) | caption `ui_st_community` ("группа") |
  | `relation_caption`, `relation_static` | (23, 177) 153 × 15 | (38, 209) / (843, 209) | caption `ui_st_relation` ("отношение") |

  Each caption is `letterica16` (216, 186, 140), left aligned; each value static, at the same rectangle as
  its caption, `letterica16` (240, 217, 182), right aligned, so caption and value share one line (c1 D4).
- **`icon_static`** clips its picture to its own rectangle (`ui-layout` 4.3 `clipper`) (c2 B4).
- **Filling** (c2 B5): at each talk start the left panel is filled with the player character and the right
  panel with the partner. A fill sets `name_static` to the character's name; `rank_static`,
  `community_static` (and `reputation_static`, were it present) to the string-table translation of the
  character's rank, community and reputation names; and `icon_static`'s picture to the character's icon
  texture (shader `hud\default`), stretched to `icon_static`'s 165 × 108 (c2 B5). The values and which
  texture is a character's icon are #279's (B-chars 3), the names' texts PD-14's.
- **Updating** (c2 B6): a panel that shows a character is refreshed at its first update after a fill and then
  on every frame whose number is a multiple of 100. A refresh first looks the character up; if it no longer
  exists, the panel forgets it and refreshes no more. Then:
  - for a character other than the player character, `relation_caption` and `relation_static` are shown,
    `relation_static`'s text is the translated name of the character's relation to the player character, and
    its colour is opaque (0, 255, 0) for relation kind 0, (192, 192, 192) for kind 1 and (255, 0, 0) for any
    other kind; for the player character both are hidden. So the left panel never shows a relation line and
    the right one does. The kinds and names are #279's and PD-14's.
  - the picture's texture colour becomes (255, 160, 160), opaque (multiplying the picture, `ui-layout` 8),
    when a value of the character is 0 or less, **believed** its health (the value is read from the
    character's condition) (c2 B6).
- **Wide screens** (`trade_character_16.xml`, c1 D4): `icon_static` (15, 35) 110 × 108, `name_static`
  (21, 7) 115 × 22, the three lines at (21, 150), (21, 164), (21, 177), each 100 × 15; the texts as above.

## 6. The answer log (`answers_list`)

A scroll view (`ui-layout` 9.1; `options-screens` 4.7 and 4.8 for the scroll bar, the wheel and dragging),
absolute (251, 88) 544 × 377, `right_ident` 0, `always_show_scroll` 0 (the bar only while the entries are
taller than the view), profile `default` (16-unit bar at the right edge, x 779 … 795) (c1 D5, c2 B2).

- **At talk start** the log is emptied and scrolled to its beginning (c2 B10).
- **Scrolling to the end** (c2 B10): after every entry the log is laid out again and its position set to
  *max* − *pg* + 1 (`options-screens` 4.8: *max* the entries' total height, *pg* the view height); the
  entries are drawn shifted up by that position when it is positive and not shifted otherwise. So while the
  entries fit, they stand from the top of the view; once they are taller, the last one ends one unit above
  the view's bottom edge.

### 6.1 Phrases

Each phrase said in the talk (which, and by whom, is DG-4's) adds one entry (c2 B11):

1. The template is `actor_answer_item` when the speaker's name equals the player character's name (an exact
   byte comparison), else `other_answer_item`. A partner whose name equals the player character's is
   therefore shown with the player character's template (c2 B11).
2. The entry is built from the template (below), `name_caption` gets the speaker's name and `content_text`
   the string-table translation of the phrase's text (c2 B11).
3. Its height is set (6.3), it is appended at the end of the log, and the log scrolls to its end (c2 B11).
4. The phrase also makes a record of the player character's news: the text `<name> >> <phrase text>`, with
   the picture and texture rectangle of the speaker's panel's `icon_static` (the left panel's for the player
   character's template, else the right one's) (c2 B11). What shows these records is `pda-scope`'s (PD-11,
   PD-12).

### 6.2 Iconed lines

Lines added by the script call of DG-7 are added only while the talk window is shown, from the template
that the call names, `iconed_answer_item` when it names none (the shipped data has that and `iconed_trade_info`)
(c2 B13). Such an entry is a phrase entry with one more static, `msg_icon`; its name is empty, its
`content_text` is the given text, and `msg_icon` shows the given rectangle (in texture pixels) of the given
texture, shader `hud\default`, stretched to `msg_icon`'s size (c2 B13). It is appended and the log scrolls to
its end; it makes no news record (c2 B13).

### 6.3 Entries

An entry is a window read from its template (`ui-layout` 4.1), then `min_height` (default 15) and
`bottom_footer` (default 0) of the template, its `content_text` and `name_caption` children as statics, and
for iconed entries `msg_icon` (c2 B12, B13). Its position in the log is the list's (`ui-layout` 9.1: x =
`right_ident`, stacked from the top).

- **Height** (c2 B12): the text of `content_text` is wrapped to that element's width (`text-and-fonts` 4.3)
  and `content_text`'s height becomes the text's height: *t* for a single-line text, *n* · *t* for *n*
  lines in complex mode (no line gap is set). The entry's height becomes `bottom_footer` + max(`content_text`
  y + its height, `min_height`). `name_caption` and `msg_icon` do not count. The template's own `height` and
  the children's heights given in the file are replaced.
- **Templates** (c1 D7):

  | Template | `min_height`, `bottom_footer` | `name_caption` | `content_text` | `msg_icon` |
  |---|---|---|---|---|
  | `other_answer_item` | 13, 5 | (0, 0) 520 × 10, `graffiti22` (238, 153, 26), single line | (20, 22) 510, `letterica18` (216, 186, 140), complex | — |
  | `actor_answer_item` | 13, 10 | (0, 0) 520 × 10, `graffiti22` (135, 183, 116), single line | (20, 22) 510, `letterica18` (255, 232, 208), complex | — |
  | `iconed_answer_item` | 30, 5 | (0, 0) 0 × 0 | (75, 0) 510, `letterica16` (255, 255, 255), complex | (20, 3) 49 × 29 |
  | `iconed_trade_info` | 20, 5 | (0, 0) 0 × 0 | (65, 2) 510, `letterica18` (216, 186, 140), complex | (20, 0) 34 × 20 |

  All four are 527 wide. Complex mode comes from `complex_mode="1"` on `content_text` itself (`ui-layout`
  5.1). The name's 22-unit line overflows its 10-unit element; nothing is clipped (`ui-layout` 7.3).
- **Worked heights** at 1024 × 768 (from the rules above): a one-line phrase of the partner 5 + max(22 + 18,
  13) = 45, two lines 63; of the player character 50 and 68; a one-line `iconed_answer_item` 5 + max(16, 30)
  = 35; a one-line `iconed_trade_info` 5 + max(2 + 18, 20) = 25.

## 7. The question list (`questions_list`)

A scroll view as in section 6, absolute (248, 585) 547 × 120, `right_ident` 5, `always_show_scroll` 0,
profile `default` (bar x 779 … 795) (c1 D5).

- **Filling** (c2 B14): at talk start the list is emptied with the log (c2 B10); each time the choices are
  refilled (when, and which choices, is DG-4's) the list is emptied, scrolled to its beginning, and one entry
  per choice is appended in order, its caption the string-table translation of the choice's text. Adding an
  entry does not scroll the list.
- **An entry** (c2 B14): a window read from `question_item` (527 × 20, `min_height` 13; c1 D7), whose
  `content_text` (0, 0) 520 × 20 is read as a button (`pda-controls` 4; the same kind as the trade button).
  It has no textures, so only its caption is drawn (`pda-controls` 4). Its caption: `letterica18`, colour
  (255, 232, 208), left aligned and centred vertically in the caption's own height (`pda-controls` 4), complex
  mode from `complex_mode="1"` on the `text` child (`text-and-fonts` 4.1; the button's text reader reads it,
  c2 B14) (c1 D7).
- **Colours** (c1 D7, `pda-controls` 4): `e` (255, 232, 208) normally, `h` (255, 255, 255) while the cursor is
  over the caption, `t` (231, 168, 12) while it is pressed; no `d` is given, so a disabled caption would use
  `e`. What a click does is DG-6's.
- **Height** (c2 B14): the caption is wrapped to 520, its height becomes the text's height (*t* or *n* · *t*),
  and the entry's height max(0 + that height, 13); there is no footer. At 1024 × 768 a one-line choice is
  18 high, two lines 36.

## 8. The trade button

`button` is a push button as `pda-controls` 4 reads it and `save-load-screens` 3.2 describes it (c2 B2,
B14): base `ui_button_main01`, so the looks `_e` (0, 470), `_t` (0, 518), `_d` (0, 422), `_h` (0, 566), each
157 × 48 of `ui\ui_common`, drawn at pixel size from (437, 726); caption `ui_st_trade` ("Торговать"),
`graffiti22` (238, 153, 26), centred horizontally and vertically; no `text_color`, so every look uses that
colour; no sounds, hint or accelerator (c1 D2, D4, D8).

- `highlight_text="1"` and `hR`, `hG`, `hB` on the element are not read: the button reader reads none of
  them (only statics do) (c1 D8, c2 B15).
- **When it is shown** (c2 B15): at each talk start the button is shown, unless the partner has the special
  mode of section 9, when it is hidden. Nothing else of the window shows or hides it. Whether a click does
  anything, and the disabled look, are DG-10's.

## 9. Partners with the special mode

A character whose object section gives `need_osoznanie_mode` true has the special mode (a boolean key;
absent: off) (c2 B16). In the data only the section `[m_osoznanie]` of `config\creatures\m_person.ltx` gives
it (true), and no section inherits from it (c1 D6); which characters use that section is #279's.

At each talk start (c2 B15): with such a partner, both pictures (and with them both panels), the answer log,
frame line 0 (with its title) and the trade button are hidden; with any other partner they are shown. The
backgrounds, frame line 1, its title and the question list are always shown.

## 10. The wide-screen variant

`talk_16.xml` has the same 14 elements in the same order (c1 D1); it differs in (c1 D10):

| Element | `talk.xml` | `talk_16.xml` |
|---|---|---|
| `left_character_icon` | (15, 32) 200 × 218 | (15, 32) 150 × 220, `stretch="1"`: picture drawn 150 × 220 from (4, 32) |
| `right_character_icon` | (820, 32) 200 × 218 | (850, 32) 150 × 220, `stretch="1"` |
| `answers_list` | (33, 46) 544 × 377 | (20, 46) 557 × 377: absolute (238, 88) |
| `questions_list` | (30, 100) 547 × 120 | (20, 100) 557 × 125: absolute (238, 585) |
| `question_item` | 527 wide; `content_text` 520, complex | 557 wide; `content_text` 550, no `complex_mode`: single line |
| answer templates | 527 wide; `content_text` 510; `name_caption` 520 | 557 wide; `content_text` 520; `name_caption` 550 |

The backgrounds, the frame lines with their titles, the button and all fonts and colours are the same
(c1 D10). The panels are then 150 × 220 and read `trade_character_16.xml` (section 5). Consequences: with
`talk_16.xml` a choice is never wrapped, so every entry of the question list is one line (*t*) high and a
long choice runs on to the list's right edge, where the list's view clips it (`ui-layout` 9.1); question
entries (5 + 557) are wider than the view (557) and their right part lies under the scroll bar when it is
shown.

## 11. Not read or without effect

- `highlight_text`, `hR`, `hG`, `hB` of `button` (section 8).
- The two `font` elements of the root the window reads (absent; nothing uses them) and the `auto_static`
  children of the root (absent) (section 2).
- The `height` of every item template and of its `content_text` (replaced, section 6.3, 7); the `x`, `y` of
  the templates (the list places entries, `ui-layout` 9.1).
- The two extra properties of the titles and panel values (section 4, believed).
- `reputation_*` and `biography_list`, which the panel would read but `trade_character.xml` lacks.

## 12. Unestablished

- That the last `_back` copy of a vertical band is cut at the bottom like a horizontal one's (section 4,
  believed).
- The effect of the two extra properties of the titles and panel values (section 4, believed none).
- That the value tested for the reddish picture is the character's health; whether the picture's colour
  returns to white when a panel tinted this way is later filled with another character (section 5).
- What reading the absent `font` elements of the root yields; that it does not stop the game is believed,
  as nothing of the window uses the result (section 2).
- The values the panels and titles show (names, icons, ranks, communities, relation kinds and their texts):
  #279 (B-chars 3) and `pda-scope` PD-14.
- Which characters have the special mode (section 9): #279.

## 13. Coverage

The names `dialogue-scope` section 7 gives DG-5: `<left_character_icon>@talk`, `<right_character_icon>@talk`,
`<top_background>@talk`, `<bottom_background>@talk` (sections 2, 3, 5); `<frame_line_window>@talk`
(section 4); `<answers_list>@talk`, `answers_list` (section 6); `<questions_list>@talk`, `questions_list`
(section 7); `<button>@talk` (section 8); `<question_item>@talk`, `question_item` (section 7);
`<other_answer_item>@talk`, `other_answer_item`, `<actor_answer_item>@talk`, `actor_answer_item` (section
6.1, 6.3); `<iconed_answer_item>@talk`, `iconed_answer_item`, `<iconed_trade_info>@talk` (sections 6.2,
6.3); `file:talk_16.xml` (section 10).

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binary `bin\xrGame.dll` (6,246,400 bytes).
- Data examined: the winning copies of `config\` (as extracted for the approved `pda-scope`) and of
  `textures\ui\` (as extracted for the approved `pda-controls`): `talk.xml`, `talk_16.xml`,
  `trade_character.xml`, `trade_character_16.xml`, the texture descriptions `ui_inventory2.xml`,
  `ui_numpad.xml`, `ui_common.xml`, the DDS headers of the pictures, the font metric files, the string
  tables and every `.ltx` file of `config\`.
- Analysis: static analysis of `bin\xrGame.dll` with Ghidra 12.2 (headless): decompilation and instruction
  listings of the routines that build the window and its panels, read frame lines, statics, buttons and
  scroll views, lay out and draw frame lines, add and size the entries of both lists, scroll the lists,
  refresh the panels, show and hide the elements at talk start, and read the special-mode key; reading of
  constants and strings from the binary. No debugging or execution; the game was not run.
- Checks: `checks/talk-window/c1_data.py`, `c2_binary.py` (with `lib.py` and the `pda-scope` helpers
  `tree.py`, `ltx.py`), run with `python3 -I` from that directory (`run_all.py` runs them); all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.
