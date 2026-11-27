# PDA controls

Functional specification of the interface controls the PDA's tabs are built from, for S.T.A.L.K.E.R.:
Shadow of Chernobyl (Russian edition), as far as no approved specification covers them: tab bars and
filter tabs, buttons with caption colours only, check buttons, button hints and the hint window, frame
windows, frame lines with a title, animated corner pictures, `auto_static` groups, scroll views with the
PDA scroll-bar look, list windows and the tree list with its highlight, and the font elements the tree
reads. It also states, for the generic elements of the 13 window files of `pda-scope` (section 1.1), which
kind of control each is read as and which window it is attached to.

It is the spec PD-2 of `pda-scope` (section 4) and builds on, without repeating, `ui-layout`,
`text-and-fonts`, `colour-animations` (`docs/formats/`), `save-load-screens`, `options-screens`,
`sound-weather-ambient` and `pda-scope` (`docs/behavior/`), cited by name and section. What a tab shows,
which items it adds to its lists and what it does when its controls report something belong to that tab's
spec (`pda-scope` PD-4 … PD-17); the window itself (opening, closing, the tab bar's effect, the clock, the
new-content marks) to PD-1.

Conventions:

- Every fact carries its check, a script in `checks/pda-controls/` of the dirty workspace: `c1` =
  `c1_data.py` (game data), `c2` = `c2_binary.py` (static analysis of `bin\xrGame.dll`), followed by the
  label of the assertion (e.g. c2 B9). `run_all.py` runs both. File paths are archive paths of the winning
  copies; layout files are in `config\ui\`.
- "Element" is an element of a layout file; its attributes and children are read as `ui-layout` states
  (numbers like C `atoi`/`atof`, a missing required element stops the game, `ui-layout` 1.4). "Window",
  "static", the drawing order and the absolute rectangle are those of `ui-layout` 4 and 5.
- **Message** means a notification a control gives the window that owns it (its message target, or else
  its parent). The numbers are those of the table the game library gives the scripts (c2 B0): cursor
  entered (`STATIC_FOCUS_RECEIVED`) 14, cursor left (`STATIC_FOCUS_LOST`) 15, clicked (`BUTTON_CLICKED`)
  16, tab changed (`TAB_CHANGED`) 18, list item clicked (`LIST_ITEM_CLICKED`) 32, key pressed 10.
- Mouse button codes: left 337, right 338, middle 339 (`options-screens` 4.8; c2 B7).
- **Global clock**: the engine's global clock in milliseconds of `sound-weather-ambient` 3.1 (stopped while
  the game is paused). **Interface clock**: the real-time millisecond counter of `colour-animations` 4.2
  (not stopped by the pause). Which of the two a rule uses is stated with it (c2 B11, B14).
- **Believed** marks a reading with its reason; **unestablished** marks what this work could not settle.

## 1. Windows and statics

- Every element read as a window reads `x`, `y`, `alignment`, `width`, `height` (`ui-layout` 4.1), then its
  `font` child (font and colour, section 9) and its `window_name` child (its name), then its `auto_static`
  children (c2 B1). Statics, buttons, tab controls, frame windows and scroll views are read as windows
  first (c2 B1, B2, B5, B8). Elements the 13 files use as plain windows: `main_wnd`, `work_area`,
  `right_frame` of the tasks tab, `task_descr_view`, `character_info`, `level_frame` (section 8).
- **`auto_static` children.** Each `auto_static` child of a window element, in document order, is read as
  a static (`ui-layout` 5) and attached to that window (`ui-layout` 4.2); it is given the name
  `auto_static_<n>`, *n* its 0-based position among them (c2 B1).
- **Named groups.** Some tabs read every element of a given name among the root's children as a static and
  attach it to a window of their choice (c2 B1): `left_auto_static` and `right_auto_static`. Others read the
  `auto_static` children of a container element and attach them to a window of their choice: `left_auto`,
  `right_auto` (ranking; c2 B18) and `pda_char_auto_statics` (a contact's details, PD-13; c2 B1). The
  items' `auto` containers (ranking and statistics, PD-15, PD-17) are **believed** read the same way: the
  program names `auto` and `:auto` and reads `:auto` in an item set-up (c2 B1). **Disputed in review:** the
  only set-up found that reads `:auto` is that of the news items of `news.xml` (whose items have no `auto`
  element); nothing found reads the `auto` containers of the ranking and statistics items, so whether they
  are read at all is unestablished. The container itself is not
  shown. Which window
  each group goes to is in section 8.
- The PDA files' statics are ordinary statics. Two consequences of `ui-layout` 5.3 the data relies on (c1
  D9): the 24 vertical lines (`textures\ui\ui_pda_vertical_line_*`, 4 × 512 pixels) have no `stretch`, so
  each is drawn 4 × 512 units from its top-left whatever its `height` (388 … 467); the horizontal lines
  (`ui\ui_pda_horizontal_line`, 16 × 4 pixels) all have `stretch="1"` and are drawn at their element's size.
- In `stalkers_ranking.xml` two captioned `left_auto` statics give `align="r"` on the element, which has no
  effect (`ui-layout` 4.1), and one gives it on its `text` (c1 D11).

## 2. Frame windows

Elements with a nine-part border and background (`left_frame`, `right_frame_window`, `chicon_frame_window`,
`main_frame`, `main_map_frame`, …; the full list with their files is c1 D4).

- **Reading** (c2 B2): window attributes (section 1); the `texture` child's value as the frame's **base**;
  an optional `title` child read as a static and attached to the frame, then moved to the front of the
  frame's children (drawn last). No colour or alpha is read for the frame: the `a` attribute the PDA files
  give their frames (70, 80 or 90; c1 D4) has no effect, nor would colour attributes of the `texture` child
  (c2 B2). The `base_texture` child is read by nothing: no shipped binary names it (c2 B2), and only seven
  layout files contain it, none of them one of the 13 (c1 D4).
- **The base.** If `textures\<base>.ini` exists (the base with any extension removed), it describes the
  frame: its section `frame` gives `shader` and the nine parts as rectangles of the one texture `<base>`
  (c2 B3). Otherwise the frame is drawn from the nine texture ids or paths `<base>_lt`, `_t`, `_rt`, `_l`,
  `_back`, `_r`, `_lb`, `_b`, `_rb` (`ui-layout` 5.2), as `save-load-screens` 3.3 and `options-screens` 4.9
  draw it (c2 B3).
- The PDA files use the bases `ui_pda_frame_sub` and `ui_pda_frame2` (c1 D4). `textures\ui_pda_frame_sub.ini`
  and `textures\ui_pda_frame2.ini` do not exist, so the nine ids are used: for `ui_pda_frame_sub` 32 × 32
  regions of `ui\ui_old_textures` (1024 × 1024) at x 96, 128, 160 and y 96, 128, 160 (`_lt` (96, 96), `_t`
  (128, 96), `_rt` (160, 96), `_l` (96, 128), `_back` (128, 128), `_r` (160, 128), `_lb` (96, 160), `_b`
  (128, 160), `_rb` (160, 160)); `ui_pda_frame2_*` are nine 32 × 32 regions of the same texture (c1 D4). The
  files `textures\ui\ui_pda_frame_sub.ini` and `textures\ui\ui_pda_frame_sub_*.dds` exist but are not what
  these frames use (the `.ini` would be looked for at `textures\ui_pda_frame_sub.ini`; c1 D4, c2 B3).

## 3. Frame lines

Elements with a three-part horizontal band and an optional title (`left_frame_header`, `header_frame_line`,
`left_frame_line`, `info_frame_line`, `chicon_frame_line`, `map_header_frame_line`, …; 16 in the 13 files,
listed by c1 D5).

- **Reading** (c2 B4): `x`, `y`, `alignment` (`ui-layout` 4.1), `width`, `height`, `vertical` (non-zero: a
  vertical line; none in the PDA files), the `texture` child's value as the base, and the **colour** read
  from the `texture` child's attributes (`color`, or `r`, `g`, `b`, `a`, each default 255; `ui-layout` 2),
  given to all three parts. No PDA frame line's `texture` child gives a colour, so all are opaque white
  (c1 D5). Then an optional `title` child, read as a static and attached to the line.
- **Drawing**: `<base>_b` at the left, `<base>_back` repeated, `<base>_e` at the right, each at its pixel
  size, as `save-load-screens` 3.3 ("Line") and `options-screens` 4.9 state. The PDA bases `ui_string_01`,
  `ui_string_02`, `ui_string_03` name 32 × 32 regions of `ui\ui_old_textures` for each part (c1 D5), so the
  band is 32 units high whatever the element's `height` (21, 27 or 32; c1 D5). The title is drawn after the
  band (**believed**: it is a child of the line, and children are drawn after their window, `ui-layout`
  4.3).
- A frame line reads no `font`, `window_name` or `auto_static` children (c2 B4).

## 4. Buttons

The PDA's buttons (the tab and filter buttons, the map tool buttons of PD-4, the scroll-bar arrows) are read
as follows (c2 B5):

- window attributes (section 1); the `text` child (caption: string key, font, colour, alignment;
  `ui-layout` 7.1) and the caption colours `text_color` `e`, `d`, `t`, `h`; the looks: a `texture` child
  names a base whose four ids are `<base>_e`, `_t`, `_d`, `_h`, or `texture_e`, `texture_t`, `texture_d`,
  `texture_h` children name the looks one by one (a `texture_` child, as `job_item.xml` has, is not one of
  them and is not read; c2 B20); `texture_offset`; the sounds `sound_h` (played when the
  cursor enters) and `sound_t` (played on a click); the attributes `accel`, `accel_ext` (keys), and
  `shadow_offset_x`, `shadow_offset_y`; an optional `hint` child read as a static; `check_mode`; `hint`.
- **`check_mode`**: absent, the button keeps its mode (a push button); `1` makes it a **check button**; any
  other value (as `0`) a push button (c2 B5).
- **`hint`**: the hint text is the attribute's value passed through the string table (`text-and-fonts` 1.4:
  a missing key shows the key itself) (c2 B5). Section 6.
- **Look and caption colour**, chosen in every update (c2 B6): disabled → `d`; otherwise pressed (or, for a
  check button, checked; for a tab button, selected) → `t`; otherwise cursor over the button → `h`;
  otherwise `e`. A caption colour the data does not give falls back to `e`, and `e` absent is the `text`
  colour (`save-load-screens` 3.2). A look texture is drawn only when the button has textures; a button
  without any (the tab and filter buttons) draws only its caption (c2 B6).
- The caption's alignment starts centred horizontally and vertically, and the `text` child's `align`
  changes the horizontal one (`options-screens` 4.2); the PDA's tab and filter buttons give `align="l"`, so
  their captions are left aligned and centred vertically (c1 D1, D2).
- **Push buttons** (no check mode) behave as `save-load-screens` 3.2 states; a click reports *clicked* (16)
  and plays `sound_t` (c2 B7).
- **Check buttons** (c2 B7): every mouse-button press over the button reports *clicked* (16) to its owner;
  a left press (337) first toggles the checked state; a right or middle press reports without toggling.
  Releases do nothing. The look is `t` while checked. **Disputed in review:** a left press delivered as a
  double click (`save-load-screens` 3.1: less than 250 ms after the previous left press on the same
  screen) is not handled as a press: the check button neither toggles nor reports *clicked*; it reports
  the double-click notification (`WINDOW_LBUTTON_DB_CLICK`, 9) to its owner instead.
- The cursor-over flag, *cursor entered* (14) and *cursor left* (15) are those of `save-load-screens` 3.2;
  on entering, the button records the time of entry on the **global clock**, reports *cursor entered* and
  plays `sound_h` (c2 B11).

The one check-button element of the 13 files, `primary_or_all` in `pda_events.xml` (texture base
`ui_PDA_checker`, four 17 × 17 ids of `ui_common`; colours under `text_color_`), is never read: no binary
names it, nor `text_color_` (c1 D3, c2 B20). The map tool buttons' `check_mode` values are PD-4's.
**Disputed in review:** `primary_or_all` is not the only check-button element of the 13 files: `check_mode="1"`
is also given to `zoom_in_btn` and `zoom_out_btn` (`pda_map.xml`, `pda_events.xml`), the `notused_*` map
buttons, and in `job_item.xml` to `switch_description_btn`, `location_btn`, `show_pointer_btn` and
`show_descr_btn`.

## 5. Tab bars and filter tabs

The tab bar `tab` of `pda.xml` and the filter tabs `filter_tab` of `pda_events.xml` (tasks) and
`events_new.xml` (diary) are tab controls (c2 B18).

- **Reading** (c2 B8): window attributes; an optional `options_item` child (not present); the `radio`
  attribute (default 0; absent in the PDA files, c1 D1, D2); then one **tab button** for each child named
  exactly `button`, in document order, each read as a button (section 4). Children with other names are not
  read: `notused_button` (tasks filter) and `_button` (diary filter) are not shown (c1 D2, c2 B20).
- The PDA's buttons (c1 D1, D2): `pda.xml` has 7 (Задачи, План, Журнал, Контакты, Ранги, Данные, Справка),
  each 20 high at y 7 within the bar at (129, 573) 550 × 30; the tasks filter 3 (`pda_btn_active`,
  `pda_btn_accomplished`, `pda_btn_failed`), the diary filter 2 (`pda_btn_journal`, `pda_btn_news`). All have
  no texture, captions `letterica18`, left aligned, and the colours `t` (231, 168, 12), `d` (181, 170, 148),
  `e` (127, 127, 127), `h` (255, 255, 255); each has a `hint` (section 6).
- **Selection** (c2 B8, B9, B10): exactly one tab is active, initially tab 0, drawn with look `t` (its
  caption orange). A tab button has no press-and-release behaviour: a **left press** over it reports *tab
  changed* to the tab control at once. The tab control then makes that tab active; if it differs from the
  previous active tab, the old tab is drawn normal again, the new one selected, and the tab control reports
  *tab changed* (18) to its owner. A press on the active tab changes nothing. Other presses do nothing.
  **Disputed in review:** a left press delivered as a double click (`save-load-screens` 3.1) is not such a
  press: the tab button reports the double-click notification (9) instead, which the tab control passes
  on without changing the tab; so a second press within 250 ms of the previous left press on the same
  screen does not switch tabs.
  Inactive tabs use `h` while the cursor is over them, else `e`; a disabled tab `d`.
- **Keys** (c2 B10): a key press equal to a tab button's `accel` or `accel_ext` selects that tab like a
  press. No PDA tab button has an accelerator (c1 D1, D2), so the PDA's tabs have no keyboard control here.
- This is the tab control of `options-screens` 4.3, which the options screen uses with textured buttons.
  Which tab the PDA shows when it opens, what each change does and which tabs are disabled are PD-1's and
  the tabs' specs'.

## 6. Hints

A button with a non-empty hint text (section 4) shows it in the **hint window** while the cursor rests on
it. The PDA's tab and filter buttons all have hints; all 12 keys have strings (c1 D6). The keys of the
unread `primary_or_all` and `_button` would have none (c1 D6).

- **The hint window** (c2 B12) is built once, with the menu, from `hint_item.xml` element `button_hint`
  (window 200 × 30): a frame line `button_hint:frame_line` (base `ui_icons_PDA_string_tooltips`, parts
  `_b`, `_back`, `_e` 32 × 30 of `ui_common`) and a static `button_hint:description` at (10, 2), 192 × 20,
  single line, `letterica18`, left aligned, with the colour animation `ui_btn_hint`, played once
  (`la_cyclic="0"`), on the text only and only its alpha (`la_texture="0"`, `la_text="1"`, `la_alpha="1"`)
  (c1 D6). There is one hint window for all buttons; it belongs to at most one button at a time.
- **When it appears** (c2 B11): in a button's update, if the cursor is over the button, the button has a
  hint text, no button holds the hint window, and more than 500 ms of the **global clock** have passed since
  the cursor entered the button. The button then takes the hint window, which (c2 B11):
  1. gets the hint text;
  2. fits the description to the text: its width becomes the text's width in units (the font's width of
     the string, `text-and-fonts` 3.1, divided by a scale factor, **believed** the horizontal scale *sx* of
     `ui-layout` 3);
  3. restarts the description's colour animation (its start becomes now on the interface clock,
     `colour-animations` 4.2);
  4. takes the width max(80, description width + 30) for itself and its frame line; the height stays 30;
  5. is placed relative to the cursor's position (*cx*, *cy*) in UI units, with *w*, *h* the hint window's
     size: first left *cx*, top *cy* − *h* (above the cursor). If it is then off the screen — left ≤ 0, right
     ≥ 1024, top ≤ 0 or bottom ≥ 768 — it moves left by *w*; if still off, down by *h* (top *cy*); if still
     off, right by *w* (left *cx*) and down by 45 (top *cy* + 45), where it stays.
- **While shown** (c2 B13): the hint window is drawn only in frames in which its button is drawn. Each such
  frame it updates the description (its colour animation) and the frame line, gives the frame line's three
  parts the colour white with the description text's current alpha (so the band fades with the text), and
  draws itself. It is drawn over the interface (**believed**: it is drawn by its own drawing step after the
  windows, not as a child of any window).
- **When it goes** (c2 B11): when the cursor leaves the button, the button releases the hint window, and it
  is no longer drawn. It is also not drawn in frames in which the button is not drawn (c2 B13). No other
  release was found; that a press over the button leaves it shown is **believed**.
- The text and placement are fixed when it appears; another button can take it only after it is released.
- `text_hint` elements (the map panes' `text_hint`, PD-4, PD-7) are statics read with the map pane (c2
  B18); they are not this hint window.

## 7. Animated pictures (`a_static`, `anim_static`)

The corner animations of the tabs (c1 D7): six elements, 32 × 32, `ui\ui_pda_anims_01` (tasks, diary),
`ui\ui_pda_anims_03` (contacts, ranking, statistics), `ui\ui_pda_anims_04` (encyclopedia); each texture
256 × 64 pixels holds 16 frames of 32 × 32 in 8 columns and 2 rows. `ui\ui_pda_anims_02` and `_05` are not
used by the 13 files.

- **Reading** (c2 B14): a static (`ui-layout` 5), then `x_offset`, `y_offset` (floats, default 0; absent in
  the PDA files, c1 D7), `frames`, `duration` (ms), `columns`, `frame_width`, `frame_height`, `cyclic`,
  `autoplay` (integers, default 0). All six give 16, 1000, 8, 32, 32, 1, 1 (c1 D7). At set-up the shown frame
  is 0 and the elapsed time 0; with `autoplay` non-zero the animation is playing from then on.
- **Frame picture** (c2 B14): frame *k* shows the texture rectangle ((*k* mod `columns`) × `frame_width` +
  `x_offset`, (*k* div `columns`) × `frame_height` + `y_offset`, `frame_width`, `frame_height`) in pixels,
  drawn as the static's picture (`ui-layout` 5.2, 5.3: no `stretch`, so `frame_width` × `frame_height`
  units at the element's top-left).
- **Timing** (c2 B14), each update while playing, on the **interface clock**:
  1. On the element's first update while playing, the **step** *s* = `duration` / `frames`, rounded up to a
     whole number of milliseconds, is computed and frame 0 shown. The step is one value **shared by all
     animated pictures of the game**: each element sets it on its own first update, and every element uses
     the value last set.
  2. The elapsed time grows by the time since the element's previous update (or since it started playing).
  3. If the elapsed time exceeds `duration`, it becomes 0; a non-cyclic animation then stops playing (after
     this update, which shows frame 0).
  4. The frame shown is (elapsed time) div *s*.
- For the PDA (16 frames, 1000 ms): *s* = 63 (62.5 rounded up; c2 B14), so frames 0 … 14 last 63 ms each,
  frame 15 from an elapsed time of 945 to 1000, and the cycle restarts from frame 0 when the elapsed time
  passes 1000. The time past 1000 at that update is dropped.
- The update of an animated picture replaces the static's update: it applies no colour animation and
  updates no children (c2 B14). An element is updated only while shown (`colour-animations` 4.2), so a tab
  hidden for longer than `duration` shows frame 0 at its first update after being shown again.
- **Consequence of the shared step** (**believed**, from the rule above and the data): the only other
  animated picture of the data with other values is the multiplayer menu's `anim_00` of `ui_mm_mp.xml`
  (24 frames, 2000 ms, step 84; c1 D7, c2 B14). If its first update comes after the PDA pictures' first
  updates, the PDA pictures use the step 84 from then on (frames 0 … 11, each 84 ms, then frame 0 again).
  Whether that order occurs in single-player play is **unestablished**.

## 8. The elements of the tab files

Which control each generic element is read as and which window it is attached to (c2 B18). "Tab" is the
tab's own window, read from `main_wnd` (all at (110, 90), `pda-scope` 1.2); a parent's rectangle is the one
the child's position is relative to (`ui-layout` 4.1). Elements a tab's own spec owns (lists' items, map
contents, character panels) are named only where they carry a control of this spec.

- **Tasks** (`pda_events.xml`): `main_wnd:left_frame` frame window → tab; its `left_frame_header` frame line
  → `left_frame`; `left_frame_header:anim_static` animated picture → `left_frame_header`;
  `main_wnd:right_frame` window → tab; `main_wnd:left_frame:list` scroll view → `left_frame`;
  `main_wnd:left_frame:filter_tab` tab control → `left_frame`; `main_wnd:right_frame:task_descr_view`
  window, with `main_frame` frame window, `main_frame:header_frame_line` frame line and
  `main_frame:scroll_view` scroll view (each attached to the one before it; **believed** from the
  nesting, the attachment not traced); the map pane `main_wnd:right_frame:map_wnd` (PD-4). The `auto_static`
  children of `left_frame` and `main_frame` are attached to them (section 1). `primary_or_all` and
  `notused_button` are not read (section 4, 5).
- **Map pane** (`pda_map.xml` `map_wnd`, and the tasks tab's `map_wnd`): `main_wnd` window,
  `main_wnd:main_map_frame` frame window, `main_map_frame:level_frame` window,
  `main_wnd:map_header_frame_line` frame line, its `tool_bar` with the buttons `global_map_btn`,
  `actor_btn`, `zoom_in_btn`, `zoom_out_btn` (section 4), and `main_wnd:text_hint` static (c2 B18); the
  `notused_*` buttons are not read. Their parents and use are PD-4's.
- **Diary** (`events_new.xml`): `main_wnd:left_frame` frame window → tab; `left_frame_header` frame line →
  `left_frame`; `left_frame_header:filter_tab` tab control → `left_frame_header`;
  `left_frame_header:anim_static` → `left_frame_header`; `main_wnd:left_frame:work_area` window →
  `left_frame`; `work_area:src_list` list window (section 10) with the fonts `src_list:tree_item_font` and
  `src_list:tree_root_font` (section 9); `main_wnd:right_frame` **frame window** → tab (unlike the tasks
  tab); `right_frame:right_frame_header` frame line → `right_frame`; `right_frame:work_area` window →
  `right_frame`; `work_area:scroll_view` scroll view; `updated_section_static` and `old_section_static`
  statics (PD-12) (c2 B18).
- **Contacts** (`pda_contacts_new.xml`): `left_frame_window` frame window → tab; `left_frame_line` frame line
  → `left_frame_window`; `right_frame_window` → tab; `right_frame_line` → `right_frame_window`; `a_static`
  → `left_frame_line`; `list` scroll view → `left_frame_window`; `detail_list` scroll view →
  `right_frame_window`; the `left_auto_static` group → `left_frame_window`, the `right_auto_static` group
  → `right_frame_window` (c2 B18).
- **Ranking** (`stalkers_ranking.xml`): `chicon_frame_window` (at x 463) → tab; `chicon_frame_line` →
  `chicon_frame_window`; `info_frame_window` (at x 0) → tab; `info_frame_line` → `info_frame_window`;
  `a_static` → `info_frame_line`; `list` scroll view → `info_frame_window`; `character_info` window →
  `chicon_frame_window`; the `auto_static` children of `left_auto` → `info_frame_window`, of `right_auto`
  → `chicon_frame_window` (c2 B18).
- **Statistics** (`actor_statistic.xml`): `chicon_frame_window` (at x 0) → tab; `chicon_frame_line` →
  `chicon_frame_window`; `a_static` → `chicon_frame_line`; `info_frame_window` (at x 313) → tab;
  `info_frame_line` → `info_frame_window`; `detail_list` scroll view → `info_frame_window`; `master_list`
  scroll view → `chicon_frame_window`; `character_info` window → `chicon_frame_window`; the
  `right_auto_static` group → `chicon_frame_window`, the `left_auto_static` group → `info_frame_window`
  (c2 B18). The tab's set-up does not read its `list`, `article_header_static` or `mask_frame_window`.
- **Encyclopedia** (`encyclopedia.xml`): `right_frame_window` (at x 0) → tab; the fonts `tree_item_font` and
  `tree_root_font` (root children; section 9); `right_frame_line` → `right_frame_window`; `a_static` →
  `right_frame_line`; `left_frame_window` (at x 313) → tab; `left_frame_line` → `left_frame_window`;
  `article_header_static` static → `left_frame_window`; `idx_list` list window (section 10) →
  `right_frame_window`; `info_list` scroll view → `left_frame_window`; the `left_auto_static` group →
  `left_frame_window`, the `right_auto_static` group → `right_frame_window` (c2 B18). `mask_frame_window` is
  not read: the only part of the program that names it reads it from `pda_character.xml` (c2 B18).
- In the statistics and encyclopedia files "left" and "right" in the names do not match the positions: the
  `right_frame_window` of the encyclopedia is the left one on screen, and so on; the attachment follows the
  names as listed.
- The items of `job_item.xml`, `news.xml`, `encyclopedia_item.xml`, `pda_character.xml` and
  `stalkers_ranking_character.xml` (statics such as `text_cont`, `image`, `caption`) are read by the tabs'
  own code (PD-7, PD-10, PD-12, PD-13, PD-15) as statics (`ui-layout` 5); how is theirs to state.

## 9. Font elements

`font` children of windows and lists, and the tree fonts `tree_root_font` and `tree_item_font`, are read as
a font: the `font` attribute (a font name, `ui-layout` 7.2) and a colour (`color`, or `r`, `g`, `b`, `a`,
each default 255) (c2 B19). An element's text (the tree fonts' value `Encyclopedia`) is not read (c1 D10).

- The diary and the encyclopedia require both tree fonts to name a font; if either does not, an error is
  reported (c2 B19; what such a report does is `ui-layout` 1). Both files give `tree_root_font`
  `letterica18` (225, 225, 250) and `tree_item_font` `letterica16` (255, 255, 255) (c1 D10). In the diary
  they are children of `src_list`, in the encyclopedia children of the root (c1 D10). How the tabs apply
  them to their rows is PD-10's and PD-12's.

## 10. List windows and the tree list

`src_list` (diary) and `idx_list` (encyclopedia) are list windows, the older list type of `ui-layout` 9.2
(c2 B18).

- **Reading** (c2 B16): `x`, `y`, `alignment`, `width`, `height`, `item_height`, `active_bg`, the `font`
  child, `scroll_profile` (default `default`), `always_show_scroll` (default 0), `always_hide_scroll`,
  `flip_vert`. Both PDA lists give `item_height="20"` and `scroll_profile="pda"` (c1 D10).
- **Rows** (c2 B16): each row added at the end is placed at y = (rows before it) × `item_height` from the
  list's top, `item_height` high (with `flip_vert`, from the bottom instead). Its scroll bar uses the profile
  (section 11). How the list scrolls (by rows) and selects is **unestablished** beyond this.
- **Tree rows** (c2 B17) are the rows of these lists. A row is a **group** (has children) or a **leaf**.
  - A group's caption carries a sign: `+` while it is closed, `-` while it is open, in place of the last
    space before the caption (one space is added in front when there is none) (c2 B17). The exact leading
    spaces of rows at deeper levels are **unestablished**; each row records its depth, its parent's depth
    + 1 (c2 B17).
  - A click (*clicked* on the row) on a **leaf** marks it **read**; on a closed group opens it, inserting its
    children as rows directly after it; on an open group closes it, removing its rows and those of its open
    sub-groups (c2 B17).
  - Each row has an **unread** and a **read** colour (by default 0xFF00FF00 and 0xFFFFFFFF; the tabs may
    set others, PD-10, PD-12) and shows its caption in the one matching its state. A group counts as read
    when all its children are read, else unread; marking a leaf read re-evaluates every group above it
    (c2 B17).
  - **Highlight** (c2 B17): each row has the picture `ui\ui_pda_over_list` (256 × 32 pixels, c1 D9), drawn
    at its pixel size with texture offset (−20, 0), i.e. starting 20 units left of the row's left edge, and
    only while the cursor is over the row (*cursor entered* … *cursor left*). At most one row is highlighted:
    a row that receives *cursor entered* removes the highlight of the previously highlighted row. This is the
    list highlight `hud-scope` spec 2 hands on (`pda-scope` PD-2).

## 11. Scroll views and the PDA scroll bar

- **Scroll views** (`list` of the tasks, contacts and ranking tabs, `scroll_view`, `detail_list`,
  `master_list`, `info_list`; c2 B18) are read and laid out as `ui-layout` 9.1 states (`right_ident`,
  `left_ident`, `top_indent`, `bottom_indent`, `vert_interval`, `inverse_dir`, `scroll_profile`,
  `flip_vert`, `always_show_scroll`, `can_select`, `text` children) and behave as `options-screens` 4.7 and
  4.8 state, with the profile's values below in place of the `default` profile's. 13 elements of the 13
  files use `scroll_profile="pda"` (c1 D8).
- **The profile** (`config\ui\scroll_bar.xml`, vertical bars; c2 B15): the bar's thickness *t* is the
  profile's `height` (default 16); the up arrow is read from `up_arrow` and the down arrow from
  `down_arrow` as buttons (section 4), the up arrow at the bar's top, the down arrow at *H* − *t* from the
  top (*H* the bar's length); the box from `box_v` as a static; the track's texture from `back_v:texture`,
  which must be present (else an error is reported). The work length between the arrows is *H* − 2*t*.
- **`pda`** (c1 D8): *t* = 17; `up_arrow` 17 × 21 with looks `ui_scroll_PDA_btn_up_e`, `_h`, `_t` (17 × 21
  each); `down_arrow` 17 × 30 with `ui_scroll_PDA_btn_down_e`, `_h`, `_t` (17 × 30); track
  `ui_scroll_PDA_back` (17 × 30); box `ui_scroll_PDA_box_e` (17 × 28). All are ids of `ui_common`. No `_d`
  look is given; what a disabled arrow shows is **unestablished**. Consequences:
  the down arrow, placed 17 units above the bar's end, extends 13 units below it, and the up arrow covers the
  first 4 units of the work length (**believed**: the arrows' sizes come from their own elements, as for any
  button, while the placement uses *t*).
- How the track and box pictures are repeated or stretched over their lengths is stated for the `default`
  profile by `save-load-screens` 3.5 and `options-screens` 4.8; that the same rules apply with the `pda`
  sizes is **believed**.

## 12. Not read

Elements and attributes of the 13 window files that nothing reads, so they have no effect (c1, c2 B2, B18,
B20): `base_texture` (absent from the 13 files anyway), the `a` of frame windows, `notused_button`,
`_button`, `primary_or_all` with its `text_color_`, the `notused_*` map buttons, `_biography_list`,
`_relation_communities_list`, `mask_frame_window` of `encyclopedia.xml` and `actor_statistic.xml`, and the
statistics tab's `list` and `article_header_static`. The texts of `tree_root_font` and `tree_item_font`
(section 9).

## 13. Unestablished

- The exact caption text of tree rows below the top level (their leading spaces) (section 10).
- How list windows scroll and select (section 10).
- That the hint window is drawn over everything else (section 6, believed).
- Whether, in single-player play, the shared animation step can be changed by the multiplayer menu's
  animation after the PDA's pictures first played (section 7).
- The attachment of the tasks tab's `main_frame`, `header_frame_line` and `scroll_view` (section 8,
  believed from the nesting).
- The `pda` profile's repetition rules and the arrows' overhang (section 11, believed); a disabled arrow's
  look.
- That the items' `auto` containers are read like `left_auto` (section 1, believed); that a frame line's
  title is drawn after its band (section 3, believed); whether a press hides the hint (section 6).

## Provenance

- Date: 2026-11-26. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binary `bin\xrGame.dll`, with `XR_3DA.exe`, `xrCore.dll`,
  `xrRender_R1.dll`, `xrRender_R2.dll`, `xrXMLParser.dll` searched for names.
- Data examined: the winning copies of `config\` and `textures\ui\` and every `textures\*.ini`, extracted
  with the project's archive tool (`checks/pda-controls/extract.py`); the 13 window files, `hint_item.xml`,
  `scroll_bar.xml`, `ui_common.xml`, `ui_old_textures.xml`, `ui_mm_mp.xml`, the string tables, and the DDS
  headers of the PDA's pictures.
- Analysis: static analysis of `bin\xrGame.dll` with Ghidra 12.2 (headless): decompilation and instruction
  listings of the routines that read the controls' layout elements, of the tab, button, hint, animated
  picture, scroll-bar, list and tree-row behaviour, and of each PDA tab's set-up; reading of constants from
  the binary; an emulation of the binary's rounding of the animation step (`emu_step.py`). No debugging or
  execution.
- Checks: `checks/pda-controls/c1_data.py`, `c2_binary.py` (with `lib.py`, `tree.py`, `ltx.py`, `pe.py`,
  `emu_step.py`), run with `python3 -I` from that directory (`run_all.py` runs them); all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- `news.xml`'s `list` (profile `pda`, `right_ident` 10, with a `font` child) is read as a scroll view by the diary's news view; sections 8 and 11 do not list it or say which window it is attached to.
- The `text_colors` element of `job_item.xml` (children `accomplished`, `failed`, `active` under `task_sub_item:description`) is in PD-2's names, but the spec does not say how it is read.
- The tab control does not pass on its buttons' *cursor entered* (14) and *cursor left* (15) notifications as it does others (c2 B10); what it does with them instead is not stated.
