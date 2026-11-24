# Options screen

Functional specification of the options screen of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition), reached with the "Опции" item of the title menu and of the in-game menu: its layout, its four
tabs and the advanced-video page, every control (element, rectangle, caption, control type, the setting
it edits, the range or list of values, the default, how the shown value is obtained and when a change
takes effect), the Apply/Cancel buttons and Esc, key rebinding, the resolution list, the restart message,
and where and when the settings are stored.

Conventions:

- Paths are stored paths in the combined game-data tree (archive specification); text files are
  Windows-1251. "Units" are units of the 1024 × 768 UI space (`docs/formats/ui-layout.md`, section 3).
- Element names are element names (paths separated by `:`) of `config\ui\ui_mm_opt.xml`; their
  attributes are interpreted as in the UI-layout specification (rectangle relative to the parent,
  textures by texture id or path, text attributes). Texture ids resolve through the texture
  descriptions (UI-layout specification, section 6); all ids named here are in
  `config\ui\ui_common.xml` (`textures\ui\ui_common.dds`, 1024 × 1024) unless stated.
- Caption keys resolve through the string tables (`docs/formats/text-and-fonts.md`, section 1); the
  Russian text is given after the key.
- A **setting** is a console variable: a named value that the console command `<name> <value>` sets and
  whose current value can be read back as text (its *status*). Section 7 lists the variables used here.
- Built on the approved specifications `docs/behavior/title-menu.md` (section 4.5: "options" opens this
  screen and the title menu is hidden meanwhile), `docs/formats/ui-layout.md`,
  `docs/formats/text-and-fonts.md`, `docs/formats/ltx-config.md` (path aliases, section 10).
- Mouse buttons and wheel are reported to UI elements as events: *left press*, *left release*, *right
  press*, *middle press*, *move*, *wheel up*, *wheel down*, *double click* (second left press within
  250 ms, title-menu specification section 4.4). Keyboard keys are DirectInput scan codes ("DIK", as in
  the title-menu specification).

## 1. Overview

The options screen is a full-screen 2D window (0, 0, 1024, 768) shown instead of the menu. It has a
static background with two looping videos, a dialog panel with the caption "ОПЦИИ", a row of four tabs
(Видео, Звук, Игра, Управление), one content page at a time, and the buttons "Применить" (apply) and
"Отмена" (cancel). The video tab has a button that replaces its page by a scrollable advanced-video page
and back.

Every control is bound to one console variable. When the screen opens, each control shows the
variable's current value. Some controls change the variable the moment they are changed, others only
when Apply is pressed (section 5). Apply and Cancel close the screen; Esc closes it without applying or
undoing anything. Settings are written to the user configuration file only when the game exits
(section 12).

## 2. Opening and closing

### 2.1 Opening

- Activating the menu item `btn_options` (title menu or in-game menu) opens the screen. The screen
  object is created on the first activation and kept as long as the menu screen that created it exists;
  the title-menu specification states that the menu screen is rebuilt each time the menu opens, so a
  new options screen is created on the first activation after each opening of the menu.
- On every activation, before the screen is shown, all controls are refreshed from the current values
  of their variables and a backup of the shown values is taken for five of the seven groups
  (section 5.2). The selected tab and the shown page are **not** reset: on the first activation the
  video tab with its basic page is shown; later activations show the tab and page that were shown when
  the screen was last closed.
- If the hardware does not support the dynamic-lighting renderers (the test's criteria are not
  established, section 17), then, after the refresh and backup above, the renderer list is set to its
  first entry `renderer_r1` (its closed line shows "Статичное освещение"; the variable is not set at
  that moment) and disabled (section 4.6); otherwise the list is left as it is (enabled). Because the
  backup was taken before, the list then counts as changed (section 5.2) whenever the variable's value
  was not `renderer_r1`: Apply then sets `renderer renderer_r1`, and Cancel sets the variable back to
  its backed-up value. Then the dependent advanced controls are enabled or disabled (section 6.3).
  Any later refresh of the list (after a preset change, or by Cancel's undo, section 5.5) shows the
  variable's value again; the list stays disabled.
- The menu is hidden while the screen is open (title-menu specification, section 4.5).

### 2.2 Closing

| Action | What happens, in order |
|---|---|
| **Применить** (`btn_accept`, click) | 1. Every **changed** control (section 5.2; `mm_opt_controls` has no backup, see there) is *saved*, i.e. its variable is set by executing `<name> <value>` (section 5.3), group by group in the order `mm_opt_video_preset`, `key_binding`, `mm_opt_video`, `mm_opt_video_adv`, `mm_opt_gameplay`, `mm_opt_sound`, `mm_opt_controls`. 2. The restart step (section 5.4). 3. The screen closes and the menu is shown again. |
| **Отмена** (`btn_cancel`, click) | 1. Every control of the groups `mm_opt_video_preset`, `mm_opt_video`, `mm_opt_video_adv`, `mm_opt_sound` (in this order) whose value differs from its backup is set back to the backup and saved. Controls of `mm_opt_gameplay`, `mm_opt_controls` and `key_binding` are not touched. 2. The restart step. 3. The screen closes and the menu is shown again. |
| **Esc** pressed while the advanced-video page is shown | The basic video page is shown instead; the screen stays open. |
| **Esc** pressed otherwise | The screen closes and the menu is shown again. Nothing is saved or undone and the restart step does not run (changes already applied stay applied, section 5.3; pending check-box and key-cell changes are not applied (the next opening shows the variables and bindings again, section 11.5); a restart request stays pending until the next Apply or Cancel). |

Esc is acted on only on key press, and only if no control on the screen has consumed the key (a key
cell waiting for a key consumes it, section 11.4).

## 3. Layout

Drawing order is the order of the tables (later on top). Absolute rectangles are in units.

### 3.1 Screen layers

| # | Element | Absolute rectangle (x, y, w, h) | Content |
|---|---|---|---|
| 1 | `back_video` | 0, 0, 1024, 430 | Video `textures\ui\ui_mm_window_back_crop.ogm` (Ogg Theora, 1024 × 430 picture in a 1024 × 432 coded frame, 30 fps, 113 frames, looping; a rainy night village with a fire), texture rectangle (0, 0, 1024, 430): drawn 1024 × 430 |
| 2 | `background` | 0, 0, 1024, 768 | `textures\ui\ui_static_mm_back_03.dds` (path, 1024 × 1024 DXT5, no texture rectangle, no stretch, so drawn 1024 × 1024 from the top-left; the part below y = 768 is off-screen): a ruined tractor by a brick wall. Its alpha is low in an irregular sky region in the upper part, through which `back_video` shows: of the texels in rows 0–767, the 166,899 with alpha < 128 (standard DXT5 alpha interpolation) lie within x 137–1023, y 0–377; the smallest alpha is 1 (no texel is fully transparent) |
| 3 | `rug_video` | 275, 425, 128, 256 | Video `textures\ui\ui_mm_rug02.ogm` (128 × 256, 33 fps, 231 frames, looping), whole texture: a rag on barbed wire in front of the tractor wheel |
| 4 | `main_dialog:dialog` | 415, 168, 548, 461 | Texture id `ui_menu_options_dlg` (164, 232, 566, 461 px), no stretch, so drawn 566 × 461. Children: sections 3.2–3.4 |
| 5 | download strip | section 3.5 | |

The two videos follow the video-texture rules of the title-menu specification (section 3.2).

### 3.2 Dialog children (relative to `main_dialog:dialog`)

Attached in this order: `cap_options`, the five pages (video, sound, game, controls, advanced video;
each a 738 × 416 window at (55, 74), absolute (470, 242); only one is visible), `btn_accept`,
`btn_cancel`, `tab`.

| Element | Relative | Absolute | Content |
|---|---|---|---|
| `main_dialog:cap_options` | 45, 9, 378, 33 | 460, 177, 378, 33 | Static text `ui_mm_options` "ОПЦИИ", font `graffiti22`, colour (215, 195, 170), left/top |
| `main_dialog:btn_accept` | 219, 429, 157, 48 | 634, 597, 157, 48 | Button (section 4.2), textures `ui_button_main02_e/_d/_t/_h` (157 × 48 px each), caption `ui_mm_apply` "Применить", `graffiti19`, centred; caption colours e (227, 199, 178), t (180, 153, 155), d (106, 95, 91), h (0, 0, 0) |
| `main_dialog:btn_cancel` | 375, 429, 157, 48 | 790, 597, 157, 48 | Same, textures `ui_button_main03_*`, caption `ui_mm_cancel` "Отмена" |
| `main_dialog:tab` | 55, 48, 461, 26 | 470, 216, 461, 26 | Tab control (section 4.3) with four `button` children |

Tab buttons (each 116 × 26, relative to `tab`; texture `ui_button_tablist_e/_d/_t/_h`, 114 × 44 px,
drawn at pixel size with `texture_offset` (0, −7); caption font `graffiti19`, centred; caption colours
e (106, 95, 91), t (227, 199, 178), h (180, 153, 155)):

| Index | Relative x | Absolute rectangle | Caption | Page shown |
|---|---|---|---|---|
| 0 | 0 | 470, 216, 116, 26 | `ui_mm_video` "Видео" | video (basic page) |
| 1 | 116 | 586, 216, 116, 26 | `ui_mm_sound` "Звук" | sound |
| 2 | 232 | 702, 216, 116, 26 | `ui_mm_gameplay` "Игра" | game |
| 3 | 348 | 818, 216, 116, 26 | `ui_mm_controls` "Управление" | controls |

### 3.3 Page frames

Every page draws first a frame window (section 4.9) of 470 × 330 at (0, 0) of the page (absolute
470, 242, 470, 330) with texture base `ui_tablist_textbox` (`_lt`, `_t`, `_rt`, `_l`, `_back`, `_r`,
`_lb`, `_b`, `_rb`, each 32 × 32 px). The element is `frame` for the video, sound and controls pages,
`frame_gameplay` for the game page and `frame_videoadv` for the advanced page (identical data).
The controls of each page are listed in section 6 with their rectangles.

### 3.4 Page buttons

| Element | Page | Absolute rectangle | Caption | Action |
|---|---|---|---|---|
| `tab_video:btn_advanced` | video | 812, 567, 111, 29 | `ui_mm_advanced` "Расширенные" | shows the advanced page instead of the basic video page |
| `video_adv:btn_to_simply` | advanced | 812, 567, 111, 29 | `ui_mm_simply` "Базовые" | shows the basic video page instead of the advanced page |
| `tab_gameplay:btn_check_updates` | game | 812, 532, 111, 29 | `ui_mm_check_updates` "Обновления" | executes `check_for_updates 1` (online update check; section 3.5) |
| `tab_controls:btn_default` | controls | 812, 567, 111, 29 | `ui_mm_default` "По умолчанию" | key-binding defaults (section 11.6) |

All four are buttons (section 4.2) with texture `ui_button_ordinary_e/_d/_t/_h` (117 × 29 px, drawn at
pixel size, so 6 units wider than the element), caption font `letterica16`, colour (227, 199, 178)
for every state, centred.

Switching tabs hides all pages and shows the page of the new tab (for the video tab always the basic
page). The advanced page belongs to the video tab: while it is shown, the video tab stays active.

### 3.5 Download strip

Shown only while an update download started by the update check is in progress; hidden otherwise
(checked every frame). Elements (top level, absolute):

| Element | Rectangle | Content |
|---|---|---|
| `download_static` | 186, 717, 603, 51 | texture id `ui_patch_back` (`ui\ui_hud`: 421, 239, 603, 51) |
| `download_text` | 200, 730, 480, 30 | text `<p>%(<file>)`: the progress *p* in percent with no decimals, then the file name in parentheses (e.g. `42%(patch.exe)`); `letterica16`, (135, 123, 116), centred, simple mode |
| `progress_download` | 222, 754, 403, 10 | progress bar, horizontal, range 0–100, texture `ui_patch_progress` (`ui\ui_hud`: 621, 227, 403, 10) |
| `btn_cancel_download` | 652, 732, 111, 29 | button `ui_button_ordinary`, caption `mm_mp_cancel` "Отмена" (`letterica16`, (227, 199, 178)); cancels the download |

The update check and download themselves are online services outside this specification.

## 4. Control types

### 4.1 Common rules

- Rectangles, textures, text attributes and colours follow the UI-layout specification.
- **Showing and hiding.** Showing or hiding a window (the scripts' page switching, a drop-down list
  opening or closing) also enables or disables that window itself; its children keep their own
  enabled states. A hidden page is therefore disabled, and none of its controls receives input.
- **Input delivery.** Every window passes an input event to its children as follows, until one
  consumes it:
  1. If a child holds the mouse capture (mouse events; e.g. an open drop-down list) or the keyboard
     capture (keyboard events), it gets the event first; if it consumes it, delivery ends.
  2. Otherwise the children are tried in **reverse** attachment order (the last attached first, i.e.
     the topmost first, the opposite of the drawing order below), skipping disabled children.
     Visibility is not tested separately (but see *Showing and hiding*).
  3. Mouse events (press, release, move, wheel, double click) go only to a child the cursor is inside,
     and also, once, to a child that the cursor was inside on the previous mouse event and has just
     left; this event is how such a child notices that it lost the hover state (e.g. a key cell's
     waiting ends, section 11.4). Keyboard events and the *button held* event (section 4.8) go to
     every enabled child regardless of the cursor.
  On this screen only a waiting key cell consumes keys, so the keyboard order is not observable here.
- **Drawing order.** Every window draws itself and then its visible children in attachment order (later
  on top); nothing is ever moved to the front. A control's parts (e.g. an open drop-down list,
  section 4.6) are drawn as part of that control, so they cover only siblings attached **before** the
  control and are covered by siblings attached after it.
- "Hover" means the cursor is inside the control's rectangle.
- A 4-state texture base *B* names the texture ids *B*`_e` (enabled), *B*`_d` (disabled), *B*`_t`
  (pressed or selected) and *B*`_h` (hovered). Caption colours `text_color` `e`, `d`, `t`, `h` are used
  in the same states; a state whose colour is not given uses `e`.
- State choice, in priority order: disabled → `d`; pressed or selected → `t`; hovered → `h`;
  otherwise `e`.

### 4.2 Button

- Pressed state: entered on *left press* (or *double click*) over the button; while the left button
  stays down the state is pressed when the cursor is over the button and normal when it has left it.
  On *left release* over the button while pressed, the button is **clicked** and returns to normal; a
  release outside does nothing.
- The caption is centred horizontally and vertically in the button rectangle: every button starts with
  horizontal alignment centre and vertical alignment centre, and the layout data changes an alignment
  only when its `text` element gives `align` (or `vert_align`) explicitly (UI-layout specification,
  section 7.1: absent attributes leave the alignment unchanged). This holds for the restart box's
  `button_ok`, whose `text` has no `align`.
- The data of this screen gives no keyboard accelerators, hints or sounds for its buttons.

### 4.3 Tab control

- A row of tab buttons. Exactly one is **active** and drawn in the selected state (`_t` texture and `t`
  caption colour); the others use `h` when hovered, else `e`.
- A *left press* on an inactive tab makes it active at once (no release needed) and switches the page.
  A press on the active tab does nothing. There is no keyboard control of the tabs (no accelerators in
  the data).
- Initially tab 0 is active.

### 4.4 Check box

- Element rectangle as given; the box image is texture base `ui_checker` (`_e` 114, 262, `_t` 114, 239,
  `_d` 114, 285, `_h` 213, 206; each 32 × 23 px) drawn at pixel size at the element's top-left; the
  caption (the element's own `text`, if any) is left aligned and starts 32 units to the right of the
  element's left edge (the width of the box image). Its text area is the element's width × 23 units
  (the height of the box image, not the element's 21), starting at the element's top, and the caption
  is centred vertically in it: with *lh* the font's line height in units (text-and-fonts
  specification), the caption's top is at element top + (23 − *lh*) / 2. (Check boxes keep the
  buttons' vertical centring and get left horizontal alignment; the data gives neither `align` nor
  `vert_align` for them.)
- The checked state is shown with the `_t` image (and `t` caption colour); unchecked uses `h` when
  hovered, else `e`; disabled uses `d`.
- A *left press* anywhere in the element toggles it. Toggling does **not** change the variable; the
  variable is set on Apply (section 5.3).
- Shown value: the variable is read as on/off.

### 4.5 Track bar (slider)

- Height is forced to 21 units regardless of the data (all data heights are 21). Width *w* from the
  data.
- Background: a frame line (section 4.9) at (0, 2, *w*, 17) relative to the bar, texture base
  `ui_slider_e` (`_b` 41, 65, `_back` 65, 65, `_e` 89, 65; each 24 × 17 px) when enabled and
  `ui_slider_d` (`_b` 141, 215, `_back` 165, 215, `_e` 189, 215) when disabled.
- Thumb: a 24 × 17 button (section 4.2 textures `ui_slider_button_e` 65, 45, `_t` 89, 45,
  `_h` 41, 45, `_d` 147, 198) at (*p*, 2) relative to the bar.
- Value model: minimum *m*, maximum *M* (from the variable, section 7), step *s* (data attribute
  `step`, default 0.1), integer or real (data attribute `is_integer` non-zero → integer), `invert`
  (non-zero → the minimum is at the right). Thumb position *p* = (*v* − *m*) · (*w* − 24) / (*M* − *m*),
  mirrored to (*w* − 24) − *p* when inverted.
- Mouse: whenever the bar receives a mouse event while the cursor is over it and the left button is
  held (this includes the *left press* itself and every *move*), the value is set from the cursor:
  with *x* the cursor x relative to the bar (mirrored to *w* − *x* when inverted), clamped to
  [12, *w* − 12]: *r* = (*x* − 12) · (*M* − *m*) / (*w* − 24); *q* = *s* · trunc(*r* / *s*), plus *s*
  if *r* − *q* > *s* / 2; *v* = clamp(*q* + *m*, *m*, *M*), truncated toward zero for integer bars.
  Dragging with the cursor outside the bar's rectangle does not move it. There is no wheel or keyboard
  control.
- **Every** such mouse event sets the variable at once (section 5.3), even when the value did not
  change, and so does refreshing the bar from its variable (each time the thumb position is
  recomputed, the variable is set from the bar's value).
- Disabled: the disabled background and thumb texture are shown and the bar ignores the mouse.

### 4.6 Drop-down list ("combo box")

- Height is forced to 23 units (the data says 21). Width *w* from the data.
- Closed: a frame line background (0, 0, *w*, 23) with texture base `ui_cb_linetext_e` (`_b` 954, 0,
  `_back` 974, 0, `_e` 186, 180; 25 × 23 px) normally and `ui_cb_linetext_h` (`_b` 954, 23, `_back` 974, 23,
  `_e` 277, 206) while hovered; the text of the selected entry over the same rectangle: a one-line
  text, font `letterica16`, colour `text_color` `e` (`d` when disabled), left aligned at the line's
  left edge (no offset) and centred vertically in the 23 units.
- Open: below the line, at (0, 23) relative to the control, a frame window (section 4.9) with texture
  base `ui_cb_listbox` (10 × 10 px parts at 723–753, 77–107) of size *w* × (*n* · 18), *n* = the data
  attribute `list_length` (default 4), and over it a scrolling list of all entries, one per 18-unit
  row, in the order of the variable's value list. A vertical scroll bar (16 units wide, the `default`
  profile of `config\ui\scroll_bar.xml`) is shown always when `always_show_scroll` is 1 (the default
  for drop-down lists, so all four lists of this screen show it), otherwise only when the entries do
  not fit.
- Entry rows: entry *k* (0-based, in list order) is a row at (0, 18 · *k*) of the list content, 18 units
  high and *w* − 16 − 5 units wide (the list width minus the scroll bar's 16 when it is shown, minus 5;
  the list's indents are 0). The text of a row is one line, font `letterica16`, left aligned at the
  row's left edge (no offset), centred vertically in the 18 units, in the `list_font` colour; for
  disabled rows (none occur here) the text alpha is halved (128).
- **Selected entry.** At most one entry is selected: the one matched by the last refresh (below) or
  the one last clicked. The selected row is drawn over a frame line with texture base
  `ui_cb_listline` (`_b` 186, 142, 4 × 18; `_back` 186, 161, 26 × 18; `_e` 191, 142, 4 × 18) spanning
  the whole row (0, 0, *w* − 21, 18), and its text uses the `list_font_s` colour; the other rows have no
  background and use the `list_font` colour. Hovering over rows changes nothing (the list does not
  select on hover; hover only highlights the closed line's background, above).
- Entry texts are the values' names passed through the string table (missing keys show the name
  itself, section 6.1 lists them).
- Mouse: *left press* on the closed control opens the list; while open the control holds the mouse
  capture, and a *left press* that is not on the list closes it without change; a click on an entry
  selects it, shows its text on the closed line and closes the list. If the clicked entry's value
  differs from the control's stored current value, that value is stored, the variable is set **at
  once** (section 5.3) and the screen is notified (sections 6.1, 10); otherwise nothing else
  happens. The wheel scrolls the open list (section 4.7). No keyboard control.
- Refreshing a drop-down from its variable (on opening, after a preset change, after an undo): the
  entries are removed and rebuilt from the variable's current value list (so the resolution list,
  section 9, is re-read each time); the closed line's text is set to the variable's current value
  text passed through the string table; the entry whose text is byte-for-byte equal to that text is
  selected, and the control's stored current value becomes that entry's value. If no entry matches
  (e.g. a `vid_mode` the adapter does not list), no entry is selected, the closed line still shows the
  variable's text, and the stored current value becomes **1**; clicking the entry whose value is 1 then
  changes only the closed line's text (the stored value does not differ, so the variable is not set),
  and if the mismatch already existed at opening the backup is also 1, so Apply does not set it
  either. Unlike a track bar, a refresh does **not** set the variable and does not mark a restart.
- Layering: while open, the control's height grows to 23 + *n* · 18 and the list is drawn as part of the
  control (section 4.1): it covers the controls attached before the drop-down and is covered by those
  attached after it. On this screen every control that an open list overlaps was attached earlier
  (sections 6.1, 6.5), so open lists are drawn over them.
- Disabled: shows the `d` text colour and ignores the mouse.

### 4.7 Scroll view

A vertical list of items (UI-layout specification, section 9.1 for item placement: item *j* at x =
`right_ident`, y = `top_indent` + Σ(previous heights + `vert_interval`) + `vert_interval`), clipped to
the view, with a vertical scroll bar of 16 units at the right edge (profile `default` of
`config\ui\scroll_bar.xml`: arrows `ui_scroll_btn_up` / `ui_scroll_btn_down`, track `ui_scroll_back`,
box `ui_scroll_box`, each 15 × 16 px). The scroll bar is shown always if `always_show_scroll` is 1,
otherwise only when the content is taller than the view.

- Wheel: each *wheel up* scrolls the content up (toward its start) by one step, each *wheel down* down
  by one step; the step is max(1, trunc(0.1 · view height)) units. (Which physical wheel direction is
  "up" was inferred, not measured.)
- Dragging: a *move* with the left button held over the view scrolls the content by the cursor's
  vertical movement, clamped to the content.
- The scroll bar (section 4.8).

### 4.8 Scroll bar

Used by both scroll views and by every drop-down list. Vertical, at the right edge of its view:
*t* = 16 units wide (the `height` of the `default` profile) and *H* = the view height (for a drop-down
list *H* = *n* · 18) high, drawn over the track texture `ui_scroll_back` (124, 189, 15 × 16 px).

- **Value model** (integers): position *pos*, minimum 0, maximum *max* = trunc(content height) (for a
  scroll view with zero indents; for a drop-down list the rows' total height), page *pg* = trunc(*H*),
  step *st* = max(1, trunc(0.1 · *H*)). *pos* is kept within [0, *max* − *pg* + 1]. The content is drawn
  shifted up by *pos* units. *n* = max(1, *max* − *pg* + 1).
- **Arrows**: up arrow at (0, 0) and down arrow at (0, *H* − 16), each 16 × 16 (profile `up_arrow` /
  `down_arrow`, textures `ui_scroll_btn_up` (124, 173, 15 × 16 px) and `ui_scroll_btn_down` (124, 221,
  15 × 16 px), drawn at pixel size). They are buttons (section 4.2) whose *press* and whose *click* each
  move *pos* by one step (up arrow −*st*, down arrow +*st*, clamped).
- **Repeat while held.** On every input update on which a mouse button is down and was already down
  at the previous update, the menu delivers a *button held* event to its topmost window with the
  button's key code (left `mouse1` 337, right 338, middle 339); it travels down the window tree like a
  keyboard event (section 4.1, so only enabled, and therefore only shown, scroll bars get it). Scroll
  bars react only to code 337. A scroll bar receiving it tests whether the cursor is inside the
  absolute rectangle of its up or down arrow; if so, and if **more than** 100 ms (integer milliseconds
  of the game clock) have passed since the last repeat, it makes one step in that arrow's direction
  and records the current time as the last repeat. The last-repeat time is a single value shared by
  all scroll bars, initially 0. Consequently a click whose button stays down for at least one input
  update after the press (and with no repeat by any scroll bar in the preceding 100 ms) moves **three**
  steps: the press, an immediate repeat, and the click on release; further steps follow every
  > 100 ms while the button is held over the arrow. A click released before any held update moves
  two steps.
- **Box** (texture `ui_scroll_box`, 124, 205, 15 × 16 px, stretched to its size): width *t*, length
  *L* = clamp(*W*ₐ · *pg* / *max*, min(*t*, *H* − 32), *H* − 32), where the work length *W*ₐ =
  trunc(*H* − 2 · 16). Its top is at trunc(((*pos*) · (*W*ₐ − trunc(*L*)) + *n* · 16) / *n*) (integer
  division), then clamped so that it lies between the bottom of the up arrow (16) and the top of the
  down arrow (*H* − 16 − *L*).
- **Dragging the box** with the left button moves it along the bar (clamped as above); *pos* is
  recomputed from its new top *y* by the inverse rule, *pos* = trunc(*n* · (*y* − 16) / (*W*ₐ − *L*)),
  clamped to [0, *max* − *pg* + 1] (the subtraction of the arrow length was inferred), and the content
  follows.
- **Wheel** over the bar: *wheel up* −*st*, *wheel down* +*st*.
- A press on the track outside the box and arrows does nothing.

### 4.9 Frame window and frame line

- **Frame window** with base *B*: the corner pieces *B*`_lt`, `_rt`, `_lb`, `_rb` are drawn at their pixel
  size at the four corners; the edges `_t`, `_b` (horizontally) and `_l`, `_r` (vertically) are repeated
  between the corners, and `_back` is repeated in both directions over the interior; each run of
  repeats ends with a partial tile cut to the remaining length.
- **Frame line** with base *B* (horizontal): *B*`_b` at the left, *B*`_e` at the right, *B*`_back`
  repeated between them (same cut rule; inferred to be the same as for frame windows).

### 4.10 Key cell

Section 11.3.

## 5. How settings are read, applied and undone

### 5.1 Groups

Every control belongs to one group, given by its `options_item group` attribute: `mm_opt_video`,
`mm_opt_video_preset`, `mm_opt_video_adv`, `mm_opt_sound`, `mm_opt_gameplay`, `mm_opt_controls`, or (key
cells) `key_binding`.

### 5.2 Reading and backup

On every opening (section 2.1) the controls of all groups are refreshed, in the order
`mm_opt_video_preset`, `mm_opt_video`, `mm_opt_video_adv`, `mm_opt_gameplay`, `mm_opt_sound`,
`mm_opt_controls`, `key_binding`, each from the **current** value of its variable (the value in effect
in the running game, not the file). Immediately after refreshing, a backup of the shown value is taken
for the groups `mm_opt_video_preset`, `mm_opt_video`, `mm_opt_video_adv`, `mm_opt_gameplay` and
`mm_opt_sound`. A control is **changed** when its shown value differs from its backup (real values:
when they differ by at least 0.00001; check boxes: when the shown state, 1 for checked and 0 for
unchecked, differs from the backup byte); key cells are changed when a key was assigned to them since
the last save (section 11.4). Saving a control (Apply, section 5.3) does not update its backup.

**Groups without a backup.** No backup is ever taken for `mm_opt_controls` (the mouse-sensitivity bar
and the invert-mouse check box) or for `key_binding` (key cells use their own changed mark, above).
Apply nevertheless tests `mm_opt_controls` for changes like the other groups, against backup values
that are never taken:

- `track_mousesens`: a track bar's backup is set to 0 when the bar is created and is never written
  afterwards for this group. Since `mouse_sens` is always at least 0.05, the bar always counts as
  changed, so every Apply executes `mouse_sens <current value>` (`%f`). This sets the value it already
  has (the bar sets it at once, section 4.5) and marks no restart: no observable effect.
- `check_mouseinvert`: a check box's backup byte *b* is **not** set when the box is created (its
  content is whatever the memory held when the screen object was created; the memory is not cleared)
  and is never written afterwards for this group, so it stays the same for the lifetime of that
  screen object. Apply sets `mouse_invert` (section 5.3) only when the box's shown state
  (1 checked, 0 unchecked) differs from *b*:
  - *b* = 0: applied only when the box is checked when Apply is pressed (turning inversion on works;
    turning it off is silently not applied);
  - *b* = 1: applied only when the box is unchecked (turning it off works; turning it on is not
    applied);
  - any other *b*: always applied (every toggle takes effect, as for the other check boxes).

  A toggle that was not applied is visible on the next opening: the box is refreshed from the
  unchanged variable. Which value *b* has is not determined by the game data or by the program
  (section 17).

### 5.3 When a change takes effect

| Control type | Variable set |
|---|---|
| Track bar | at once, on every movement of the thumb (and again on Apply if changed) |
| Drop-down list | at once, when a different entry is chosen (and again on Apply if changed) |
| Check box | on Apply only (if changed) |
| Key cell | on Apply only (if a key was assigned) |

"Setting the variable" means executing the console command `<name> <value>` with the value formatted
as: integers in decimal; reals with six decimals (`%f`, e.g. `0.500000`); on/off variables as `on` or
`off`; list variables as the name of the chosen entry. A value outside the variable's range is
rejected by the variable (section 7) and the variable keeps its value.

What a set variable does at once is the variable's own behaviour (section 7): e.g. gamma, brightness
and contrast change the display at once, volumes change at once, the resolution and full-screen
variables only change what the next video restart will use.

### 5.4 Restart step

- Saving a control whose variable is `vid_mode`, `_preset`, `rs_fullscreen`, `r__supersample`,
  `rs_refresh_60hz`, `rs_no_v_sync` or `texture_lod` marks a **video restart** as needed; saving
  `snd_efx` marks a **sound restart** as needed. Because track bars and drop-down lists save on every
  change, moving the texture-quality or anti-aliasing slider, or choosing a resolution or preset,
  marks the restart at once. (`rs_v_sync`, edited by the vertical-sync check box, is **not** in this
  list; `rs_no_v_sync` is not edited by any control.)
- The restart step (on Apply and on Cancel): if a video restart is marked, the console command
  `vid_restart` is executed (the graphics device is reset with the current resolution, full-screen,
  refresh and vertical-sync settings; textures are reloaded); then, if a sound restart is marked,
  `snd_restart` (the sound device is restarted); then both marks are cleared. Cancel therefore also
  restarts video if, for example, a resolution was chosen and then undone.
- Because a track bar sets its variable whenever it is refreshed (section 4.5), refreshing the
  texture-quality and anti-aliasing bars on every opening of the screen (and after a preset change)
  marks a video restart. Consequently every Apply and every Cancel ends with `vid_restart`, even when
  nothing was changed; only Esc avoids it (the mark then stays pending).

### 5.5 Undo (Cancel)

For each changed control of an undoable group: its shown value is set to the
backup (for a drop-down: the entry with the backed-up value is selected and its text shown) and the
variable is set from it (section 5.3 formatting; this also marks restarts per section 5.4). Track
bars and drop-down lists are then refreshed from the variable (sections 4.5, 4.6); check boxes are not
refreshed (they already show the backed-up state). Unchanged controls are not touched.

## 6. The controls

Columns: element (relative to its page; absolute in parentheses), caption key and text, control type,
variable, values, default, effect. "Default" gives the variable's built-in initial value and, after a
slash, the value in the shipped user configuration file `__support\userdocs\user.ltx` (which the
installer copies to the user folder; section 12). All captions use font `letterica16`, colour
(215, 195, 170), left aligned; caption statics are 143 × 21 (150 × 21 for the mouse caption).

### 6.1 Video tab (basic page)

| Element (absolute) | Caption | Type | Variable [group] | Values | Default | Effect |
|---|---|---|---|---|---|---|
| `list_renderer` 194, 20, 261 × 23 (664, 262); caption `cap_renderer` (490, 262) | `ui_mm_renderer` "Тип рендера" | drop-down, `list_length` 3 | `renderer` [`mm_opt_video`] | entries in order: `renderer_r1` "Статичное освещение", `renderer_r2a` "Динамическое освещение объектов", `renderer_r2` "Полное динамическое освещение" | `renderer_r1` / `renderer_r1` | value at once; the renderer itself changes only at the next game start (section 10) |
| `list_presets` 194, 50, 261 × 23 (664, 292); `cap_preset` (490, 292) | `ui_mm_quality_presets` "Установки качества" | drop-down, `list_length` 5 | `_preset` [`mm_opt_video_preset`] | `Minimum` "минимальные", `Low` "низкие", `Default` "средние", `High` "высокие", `Extreme` "максимальные" | `Default` / `Extreme` | at once: loads the preset file (section 8), then all controls of `mm_opt_video` and `mm_opt_video_adv` are refreshed from their variables; marks a video restart |
| `list_resolution` 194, 80, 261 × 23 (664, 322); `cap_resolution` (490, 322) | `ui_mm_resolution` "Разрешение" | drop-down, `list_length` 4 (default) | `vid_mode` [`mm_opt_video`] | the resolution list (section 9); entry texts `WxH` (no string-table entries exist) | `1024x768` / `1024x768` | value at once; applied by the video restart (marked at once) |
| `track_gamma` 194, 110, 261 (664, 352); `cap_gamma` (490, 352) | `ui_mm_gamma` "Гамма" | track bar, real, step 0.1 | `rs_c_gamma` [`mm_opt_video`] | 0.5–1.5 | 1.0 / 1. | at once |
| `track_contrast` 194, 140, 261 (664, 382); `cap_contrast` (490, 382) | `ui_mm_contrast` "Контраст" | track bar, real, step 0.1 | `rs_c_contrast` [`mm_opt_video`] | 0.5–1.5 | 1.0 / 1. | at once |
| `track_brightness` 194, 170, 261 (664, 412); `cap_brightness` (490, 412) | `ui_mm_brightness` "Яркость" | track bar, real, step 0.1 | `rs_c_brightness` [`mm_opt_video`] | 0.5–1.5 | 1.0 / 1. | at once |
| `check_fullscreen` 194, 200, 30 × 21 (664, 442); `cap_fullscreen` (490, 442) | `ui_mm_fullscreen` "Полный экран" | check box | `rs_fullscreen` [`mm_opt_video`] | on/off | on / on | on Apply; applied by the video restart that saving it marks |

Choosing a different renderer entry also triggers the restart message (section 10) and re-evaluates
the dependent advanced controls (section 6.3).

Attachment (drawing) order on this page — the reverse of the visual top-to-bottom order of the table:
frame, `cap_fullscreen`, `check_fullscreen`, `cap_brightness`, `track_brightness`, `cap_contrast`,
`track_contrast`, `cap_gamma`, `track_gamma`, `cap_resolution`, `list_resolution`, `cap_preset`,
`list_presets`, `cap_renderer`, `list_renderer`, `btn_advanced`. Hence (section 4.1) an open list is
drawn over the controls below it: the renderer list (absolute y 285–339) over the preset and
resolution lines; the preset list (315–405) over the resolution line and the gamma and contrast bars;
the resolution list (345–417) over the gamma, contrast and brightness bars.

### 6.2 Advanced video page

Elements of `video_adv`. The page has the frame `frame_videoadv`, then the scroll view `scroll_v`
(0, 10, 457, 313 → absolute 470, 252, 457 × 313; all indents and the interval 0; `always_show_scroll`
0, so its scroll bar is shown because the content, 390 units, is taller than 313; wheel step 31 units),
then the button `btn_to_simply`. The scroll view holds 13 rows; each row is a 450 × 30 window
(`templ_item`) at y = 30 · *j* of the content and contains the caption at (20, 0) and the control at
(194, 0) of the row. Track bars here are 245 units wide; check boxes 30 × 21 except the NPC torch box
(243 × 21). Absolute positions are given for the content scrolled to the top: caption (490, 252 + 30*j*),
control (664, 252 + 30*j*).

| Row *j* | Caption element / key / text | Control element, type | Variable [group] | Values | Default | Effect |
|---|---|---|---|---|---|---|
| 0 | `cap_vis_dist` `ui_mm_vis_distance` "Дальность видимости" | `track_vis_dist`, real, step 0.1 | `rs_vis_distance` [`mm_opt_video_adv`] | 0.4–1.5 | 1.0 / 1. | at once |
| 1 | `cap_geometry_lod` `ui_mm_geometry_lod` "Детализация объектов" | `track_geometry_lod`, real, step 0.1 | `r__geometry_lod` [`mm_opt_video_adv`] | 0.1–1.2 | 1.0 / 1. | at once |
| 2 | `cap_detail_density` `ui_mm_detail_density` "Плотность травы" | `track_detail_density`, real, step 0.02, inverted | `r__detail_density` [`mm_opt_video_adv`] | 0.2–0.6 (0.6 at the left) | 0.3 / 0.2 | at once |
| 3 | `cap_texture_lod` `ui_mm_texture_quality` "Качество текстур" | `track_texture_lod`, integer, step 1, inverted | `texture_lod` [`mm_opt_video_adv`] | 0–4 (4 at the left) | 0 / 0 | marks a video restart at once; textures reload at the restart |
| 4 | `cap_aniso` `ui_mm_anisotropic` "Фильтрация текстур" | `track_aniso`, integer, step 1 | `r__tf_aniso` [`mm_opt_video_adv`] | 1–16 | 4 / 16 | at once |
| 5 | `cap_ssample` `ui_mm_supersample` "Сглаживание" | `track_ssample`, integer, step 1 | `r__supersample` [`mm_opt_video_adv`] | 1–4 | 1 / 2 | marks a video restart at once |
| 6 | `cap_r2_sun` `ui_mm_r2_sun` "Тень от солнца" | `check_r2_sun`, check box | `r2_sun` [`mm_opt_video`] | on/off | on / on | on Apply; enabled only with renderer entry 2 |
| 7 | `cap_r2_sun_details` `ui_mm_r2_sun_details` "Тень травы" | `check_r2_sun_details`, check box | `r2_sun_details` [`mm_opt_video`] | on/off | off / off | on Apply; enabled only with renderer entry 2 |
| 8 | `cap_light_distance` `ui_mm_light_distance` "Дальность освещения" | `track_light_distance`, real, step 0.1 | `r2_slight_fade` [`mm_opt_video_adv`] | 0.02–2.0 | 1.0 / 0.3333 | at once; enabled only with renderer entry 1 or 2 |
| 9 | `cap_npc_torch` `ui_mm_npc_torch` "Свет фонариков НПС" | `check_npc_torch`, check box (243 × 21) | `ai_use_torch_dynamic_lights` [`mm_opt_video_adv`] | on/off | on / on | on Apply |
| 10 | `cap_particles_distance` `ui_mm_sun_quality` "Качество теней" | `track_particles_distance`, real, step 0.1 | `r2_ls_squality` [`mm_opt_video_adv`] | 0.5–1.0 | 1.0 / 1. | at once; enabled only with renderer entry 1 or 2 |
| 11 | `cap_vsync` `ui_mm_vsync` "Верт. синхронизация" | `check_vsync`, check box | `rs_v_sync` [`mm_opt_video_adv`] | on/off | off / off | on Apply; used at the next video restart (does not itself mark one) |
| 12 | `cap_60hz` `ui_mm_60_hz` "Частота 60Гц" | `check_60hz`, check box | `rs_refresh_60hz` [`mm_opt_video_adv`] | on/off | off / on | on Apply; marks a video restart |

Note that the two sun-shadow check boxes belong to the group `mm_opt_video`, not `mm_opt_video_adv`.

### 6.3 Dependent controls

Each time the screen opens and each time a different renderer entry is chosen, with *i* the index of
the renderer list's selected entry (0 static, 1 dynamic objects, 2 full dynamic): `check_r2_sun` and
`check_r2_sun_details` are enabled only when *i* = 2; `track_light_distance` and
`track_particles_distance` are enabled only when *i* is 1 or 2. Disabled controls keep their values.

### 6.4 Sound tab

| Element (absolute) | Caption | Type | Variable [group] | Values | Default | Effect |
|---|---|---|---|---|---|---|
| `track_mastervolume` 194, 20, 245 (664, 262); caption `cap_mastervolume` 20, 20 (490, 262) | `ui_mm_master_volume` "Громкость" | track bar, real, step 0.1 | `snd_volume_eff` [`mm_opt_sound`] | 0.0–1.0 | 1.0 / 1. | at once (effects volume) |
| `track_musicvolume` 194, 45, 245 (664, 287); caption `cap_musicvolume` 20, 45 (490, 287) | `ui_mm_music_volume` "Громкость музыки" | track bar, real, step 0.1 | `snd_volume_music` [`mm_opt_sound`] | 0.0–1.0 | 0.7 / 1. | at once (music volume) |
| `check_eax` 20, 70, 143 × 21 (490, 312) | own caption `ui_mm_eax` "EAX" (right of the box) | check box | `snd_efx` [`mm_opt_sound`] | on/off | on / on | on Apply; marks a sound restart |

Attachment order on this page: frame, the two captions, the check box, the music bar, the volume bar.

### 6.5 Game tab

| Element (absolute) | Caption | Type | Variable [group] | Values | Default | Effect |
|---|---|---|---|---|---|---|
| `list_difficulty` 194, 20, 261 × 23 (664, 262); caption `cap_difficulty` (490, 262) | `ui_mm_difficulty` "Сложность" | drop-down, `list_length` 4 (default) | `g_game_difficulty` [`mm_opt_gameplay`] | `gd_novice` "новичок", `gd_stalker` "опытный", `gd_veteran` "ветеран", `gd_master` "мастер" | `gd_master` / `gd_novice` | at once (also in a running single-player game, section 14) |
| `check_crosshair` 20, 80, 243 × 21 (490, 322) | `ui_mm_show_crosshair` "Показывать прицел" | check box | `hud_crosshair` | on/off | on / on | on Apply |
| `check_dyn_crosshair` 20, 105 (490, 347) | `ui_mm_dyn_crosshair` "Динамический прицел" | check box | `cl_dynamiccrosshair` | on/off | on / on | on Apply |
| `check_show_weapon` 20, 130 (490, 372) | `ui_mm_show_weapon` "Показывать оружие" | check box | `hud_weapon` | on/off | on / on | on Apply |
| `check_dist` 20, 155 (490, 397) | `ui_mm_crosshair_distance` "Дистанция до цели" | check box | `hud_crosshair_dist` | on/off | off / off | on Apply |
| `check_tips` 20, 180 (490, 422) | `ui_mm_tips` "Распознавание НПС" | check box | `hud_info` | on/off | on / on | on Apply |
| `btn_check_updates` | section 3.4 | | | | | |

All check boxes here are 243 × 21 with their own captions, group `mm_opt_gameplay`. Attachment order:
frame, caption, `check_tips`, `check_crosshair`, `check_dyn_crosshair`, `check_show_weapon`,
`check_dist`, `list_difficulty`, `btn_check_updates`. The open difficulty list (absolute y 285–357)
is therefore drawn over `check_crosshair` and `check_dyn_crosshair`. Cancel does not undo this group: a difficulty
chosen and then cancelled stays in effect; changed check boxes are simply not applied.

### 6.6 Controls tab

| Element (absolute) | Caption | Type | Variable [group] | Values | Default | Effect |
|---|---|---|---|---|---|---|
| `track_mousesens` 194, 20, 261 (664, 262); caption `cap_mousesens` 20, 20, 150 × 21 (490, 262) | `ui_mm_mouse_sense` "Чувствительность мыши" | track bar, real, step 0.01 | `mouse_sens` [`mm_opt_controls`] | 0.05–0.6 | 0.12 / 0.12 | at once; not undone by Cancel |
| `check_mouseinvert` 20, 55, 143 × 21 (490, 297) | own caption `ui_mm_invert_mouse` "Инвертировать мышь" | check box | `mouse_invert` [`mm_opt_controls`] | on/off | off / off | on Apply, subject to the unwritten backup (section 5.2) |
| `cap_keyboardsetup` 6, 95, 456 × 23 (476, 337) | `ui_mm_keyboard_setup` "Настройка клавиатуры", text offset x = 10 | label: frame line `ui_brokenline` (`_b` 46, 0, 9 × 21; `_back` 55, 0, 32 × 21; `_e` 87, 0, 9 × 21) with text | | | | |
| `key_binding` 10, 120, 451 × 199 (480, 362) | | key-binding table | section 11 | | | on Apply |
| `btn_default` | section 3.4 | | | | | section 11.6 |

The label `cap_keyboardsetup` is created twice, at the same place (two identical labels drawn on top
of each other). Attachment order: frame, `cap_mousesens`, the two labels, `track_mousesens`,
`check_mouseinvert`, `key_binding`, `btn_default`.

## 7. Variables

### 7.1 Kinds

| Kind | Accepted value | Status (text read back and stored) |
|---|---|---|
| on/off | `on` or `1` → on; `off` or `0` → off (exact, case-sensitive); anything else is rejected | `on` / `off` |
| integer | decimal integer (C `atoi`); rejected unless *m* ≤ value ≤ *M* | decimal |
| real | C `atof`; rejected unless *m* − 0.00001 ≤ value ≤ *M* + 0.00001 | `%3.5f` with trailing zeros removed (so `1.00000` → `1.`, `0.33330` → `0.3333`, `0.00000` → `0.`) |
| list | a name from the list, compared case-insensitively; anything else is rejected | the current name (`?` if none matches) |

### 7.2 The variables of this screen

| Variable | Kind | Range / list (index = value) | Built-in initial value |
|---|---|---|---|
| `renderer` | list | `renderer_r1` 0, `renderer_r2a` 1, `renderer_r2` 2 | `renderer_r1` |
| `_preset` | list | `Minimum` 0, `Low` 1, `Default` 2, `High` 3, `Extreme` 4 | `Default` |
| `vid_mode` | resolution | `<W>x<H>` (two decimal integers); any well-formed value is accepted (the list of section 9 is offered by the control); a malformed one is rejected | `1024x768` |
| `rs_c_gamma`, `rs_c_contrast`, `rs_c_brightness` | real | 0.5–1.5; setting one rebuilds the display gamma ramp from the three values at once | 1.0 each |
| `rs_fullscreen` | on/off | | on |
| `rs_vis_distance` | real | 0.4–1.5 | 1.0 |
| `r__geometry_lod` | real | 0.1–1.2 | 1.0 |
| `r__detail_density` | real | 0.2–0.6 | 0.3 |
| `texture_lod` | integer | 0–4 | 0 |
| `r__tf_aniso` | integer | 1–16 | 4 |
| `r__supersample` | integer | 1–4 | 1 |
| `r2_sun` | on/off | | on |
| `r2_sun_details` | on/off | | off |
| `r2_slight_fade` | real | 0.02–2.0 | 1.0 |
| `r2_ls_squality` | real | 0.5–1.0 | 1.0 |
| `ai_use_torch_dynamic_lights` | on/off | | on |
| `rs_v_sync` | on/off | | off |
| `rs_refresh_60hz` | on/off | on: the display's default refresh rate; off: the highest refresh rate the adapter lists for the chosen resolution | off |
| `snd_volume_eff`, `snd_volume_music` | real | 0.0–1.0 | 1.0, 0.7 |
| `snd_efx` | on/off | | on |
| `g_game_difficulty` | list | `gd_novice` 0, `gd_stalker` 1, `gd_veteran` 2, `gd_master` 3 | `gd_master` |
| `hud_crosshair`, `cl_dynamiccrosshair`, `hud_weapon`, `hud_info` | on/off | | on |
| `hud_crosshair_dist` | on/off | | off |
| `mouse_sens` | real | 0.05–0.6 | 0.12 |
| `mouse_invert` | on/off | | off |

The renderer-specific variables (`_preset`, `r__*`, `r2_*`) have the same kinds, ranges and initial
values whichever renderer is running. The built-in initial values matter only when the user
configuration file does not set the variable; the shipped file sets all of them (section 12.4).

## 8. Quality presets

Setting `_preset` first stores the name (an unknown name is rejected and the value is kept), then,
in either case, executes the configuration script of the current value:
`$game_config$rspec_<file>.ltx` (via `cfg_load <full path>`), where `<file>` is `minimum`, `low`,
`default`, `high`, `extreme` for `Minimum` … `Extreme`. (`config\rspec_maximum.ltx` exists but no
name selects it.) A script is executed line by line as console commands (section 12.2). The values
the five scripts set for the controls of this screen:

| Variable | minimum | low | default | high | extreme |
|---|---|---|---|---|---|
| `rs_vis_distance` | .75 | .75 | 1. | 1. | 1 |
| `r__geometry_lod` | 1. | .9 | 1. | 1. | 1. |
| `r__detail_density` | 0.4 | 0.4 | 0.3 | 0.2 | 0.2 |
| `texture_lod` | 2 | 2 | 1 | 1 | 0 |
| `r__tf_aniso` | 1 | 2 | 4 | 4 | 16 |
| `r__supersample` | 1 | 1 | 1 | 1 | 2 |
| `r2_sun` | off | on | on | on | on |
| `r2_sun_details` | off | off | off | off | off |
| `r2_slight_fade` | – | 0.4 | 0.5 | 0.6 | – |
| `r2_ls_squality` | – | 0.75 | 1.0 | 1.0 | – |
| `ai_use_torch_dynamic_lights` | – | on | on | on | – |

("–": not set by that script.) The scripts also set other variables: all five set `ph_iterations`,
`r1_dlights`, `r1_dlights_clip`, `r1_glows_per_frame`, `r1_tf_mipbias`, `r2_tf_mipbias`,
`r__wallmark_ttl` and `rs_skeleton_update`; `low`, `default` and `high` also set `snd_cache_size` and
`snd_targets`; `minimum` and `extreme` also set `r1_ssa_lod_a`, `r1_ssa_lod_b`, `r__dtex_range`,
`r__ssa_glod_end`, `r__ssa_glod_start`, `rs_detail` and `vid_bpp` (16 in `minimum`, 32 in `extreme`).

## 9. The resolution list

- The list is built when the graphics device is created (and discarded when it is destroyed; a video
  restart resets the device and is believed to keep the list), from the display modes
  the graphics adapter reports for the back-buffer pixel format in use (full screen: 32-bit colour if
  `vid_bpp` is 32 — X8R8G8B8, else A8R8G8B8, else R8G8B8 — or for 16-bit R5G6B5, else X1R5G5B5, else
  X4R4G4B4, the first the adapter supports; windowed: the desktop's format).
- Modes narrower than 800 pixels are skipped. Each remaining mode is written as `<width>x<height>`
  (decimal); a text already in the list (the same size at another refresh rate) is not added again.
  The order is the adapter's enumeration order. Entry *k* has the value *k*.
- When the device is created or reset in full screen and the current `vid_mode` is not in the list,
  `vid_mode` is set to the first entry. In a window the current `vid_mode` is used as it is.

## 10. Renderer change and the restart message

When a different renderer entry is chosen (section 4.6), the dependent controls are re-evaluated
(section 6.3) and, the first time this happens in the lifetime of the screen object (section 2.1), a
message box opens over the screen. The variable is set at once, but the running renderer does not
change; the new value is used when the game is next started (it is stored on exit, section 12).

Message box `message_box_restart_game` of `config\ui\message_box.xml` (type `ok`), absolute
(200, 250, 510, 206), texture id `ui_frame_error` (213, 0, 510, 206 px):

| Child | Rectangle (relative) | Content |
|---|---|---|
| `picture` | 35, 65, 77, 77 | texture `ui_frame_error_sign_red` (800, 0, 77, 77 px) |
| `message_text` | 170, 40, 310, 60 | `ui_mm_need_restart` "Изменения вступят в силу только после перезагрузки игры." — `letterica18`, (216, 186, 140), multi-line (`complex_mode="1"`), wrapped to 310 |
| `button_ok` | 370, 154, 117, 29 | button `ui_button_ordinary`, caption `ui_st_btn_ok` "ОК", `letterica16`, (216, 186, 140) |

Clicking the button closes the box. While the box is open, all mouse and keyboard input of the menu
goes to the box only (input always goes to the topmost open window of the menu): the options screen
receives nothing, so Esc does not close it. The box itself reacts to no key (its button has no
accelerator), so it can be closed only by clicking the button. The console key and F12 keep working
(section 13).

## 11. Key bindings

### 11.1 Actions

Key bindings map **actions** to keys. Each action has a primary and a secondary key slot (either may
be empty). The 77 actions, in their fixed order, with their kind (1 = any game, 3 = single player only,
5 = multiplayer only):

`left` 1, `right` 1, `up` 1, `down` 1, `jump` 1, `crouch` 1, `crouch_toggle` 1, `accel` 1,
`sprint_toggle` 1, `forward` 1, `back` 1, `lstrafe` 1, `rstrafe` 1, `llookout` 1, `rlookout` 1,
`turn_engine` 3, `cam_1` 1, `cam_2` 1, `cam_3` 1, `cam_4` 1, `cam_zoom_in` 1, `cam_zoom_out` 1, `torch` 1,
`night_vision` 1, `wpn_1` 1, `wpn_2` 1, `wpn_3` 1, `wpn_4` 1, `wpn_5` 1, `wpn_6` 1, `artefact` 5,
`wpn_next` 1, `wpn_fire` 1, `wpn_zoom` 1, `wpn_zoom_inc` 1, `wpn_zoom_dec` 1, `wpn_reload` 1, `wpn_func` 1,
`wpn_firemode_prev` 1, `wpn_firemode_next` 1, `pause` 1, `drop` 1, `use` 1, `scores` 1, `chat` 5,
`chat_team` 5, `screenshot` 1, `quit` 1, `console` 1, `inventory` 1, `buy_menu` 5, `skin_menu` 5,
`team_menu` 5, `active_jobs` 3, `map` 1, `contacts` 3, `ext_1` 1, `vote_begin` 5, `vote` 5, `vote_yes` 5,
`vote_no` 5, `next_slot` 1, `prev_slot` 1, `speech_menu_0` … `speech_menu_9` 5, `use_bandage` 3,
`use_medkit` 3, `quick_save` 3, `quick_load` 3.

Action names are compared case-insensitively.

### 11.2 Keys

Keys are named by these 129 names (compared case-insensitively), each with its DIK code; the eight
mouse buttons use codes 337–344:

`kESCAPE` 1, `k1`…`k9` 2–10, `k0` 11, `kMINUS` 12, `kEQUALS` 13, `kBACK` 14, `kTAB` 15, `kQ` 16, `kW` 17,
`kE` 18, `kR` 19, `kT` 20, `kY` 21, `kU` 22, `kI` 23, `kO` 24, `kP` 25, `kLBRACKET` 26, `kRBRACKET` 27,
`kRETURN` 28, `kLCONTROL` 29, `kA` 30, `kS` 31, `kD` 32, `kF` 33, `kG` 34, `kH` 35, `kJ` 36, `kK` 37,
`kL` 38, `kSEMICOLON` 39, `kAPOSTROPHE` 40, `kGRAVE` 41, `kLSHIFT` 42, `kBACKSLASH` 43, `kZ` 44, `kX` 45,
`kC` 46, `kV` 47, `kB` 48, `kN` 49, `kM` 50, `kCOMMA` 51, `kPERIOD` 52, `kSLASH` 53, `kRSHIFT` 54,
`kMULTIPLY` 55, `kLMENU` 56, `kSPACE` 57, `kCAPITAL` 58, `kF1`…`kF10` 59–68, `kNUMLOCK` 69, `kSCROLL` 70,
`kNUMPAD7` 71, `kNUMPAD8` 72, `kNUMPAD9` 73, `kSUBTRACT` 74, `kNUMPAD4` 75, `kNUMPAD5` 76, `kNUMPAD6` 77,
`kADD` 78, `kNUMPAD1` 79, `kNUMPAD2` 80, `kNUMPAD3` 81, `kNUMPAD0` 82, `kDECIMAL` 83, `kF11` 87, `kF12` 88,
`kF13` 100, `kF14` 101, `kF15` 102, `kKANA` 112, `kCONVERT` 121, `kNOCONVERT` 123, `kYEN` 125,
`kNUMPADEQUALS` 141, `kCIRCUMFLEX` 144, `kAT` 145, `kCOLON` 146, `kUNDERLINE` 147, `kKANJI` 148,
`kSTOP` 149, `kAX` 150, `kUNLABELED` 151, `kNUMPADENTER` 156, `kRCONTROL` 157, `kNUMPADCOMMA` 179,
`kDIVIDE` 181, `kSYSRQ` 183, `kRMENU` 184, `kHOME` 199, `kUP` 200, `kPRIOR` 201, `kLEFT` 203, `kRIGHT` 205,
`kEND` 207, `kDOWN` 208, `kNEXT` 209, `kINSERT` 210, `kDELETE` 211, `kLWIN` 219, `kRWIN` 220, `kAPPS` 221,
`kPAUSE` 197, `mouse1`…`mouse8` 337–344 (`mouse1` left, `mouse2` right, `mouse3` middle).

**Display name** of a key (shown in the key cells, and used for `$$ACTION_<name>$$` in string tables,
text-and-fonts specification section 1.6, as `<primary>` or `<primary> , <secondary>`): the name the
operating system's input layer reports for that scan code (DirectInput key name, converted to 8-bit
text), or, if there is none or it cannot be converted, the key's name from the list above (e.g.
`mouse1`). The display names are therefore not determined by the game data.

### 11.3 The key-binding table (`tab_controls:key_binding`)

Built from `config\ui\ui_mm_opt.xml` and `config\ui\ui_keybinding.xml`. Relative to `key_binding`
(absolute 480, 362):

| Part | Element | Rectangle | Content |
|---|---|---|---|
| header 1 | `header_1` | 0, 0, 229, 17 | label: frame line `ui_table_button_e` (`_b` 147, 141, 5 × 19; `_back` 152, 141, 28 × 19; `_e` 180, 141, 5 × 19), text `ui_mm_action` "Действие", offset x 2 |
| header 2 | `header_2` | 229, 0, 102, 17 | same, `ui_mm_key` "Кнопка" |
| header 3 | `header_3` | 330, 0, 102, 17 | same, `ui_mm_alternative` "Альтернатива" |
| frame | `frame` | 0, 20, 431, 181 | frame window `ui_cb_listbox` |
| list | `scroll_view` | 0, 20, 447, 181 | scroll view (section 4.7), `vert_interval` 3, scroll bar always shown, wheel step 18 |

Header texts: `letterica16`, (215, 195, 170). The element `key_binding:list` is not used.

Rows of the scroll view, for each `group` element of `ui_keybinding.xml` in document order:

1. a **group row**: a static 160 × 25 (`scroll_view:item_group`) whose text is the group's `name`
   (string key), `letterica16`, (215, 195, 170);
2. for each `command` child in order, a **command row**: a static 414 × 18 (`scroll_view:item_key`)
   whose text is the command's `id` (string key), `letterica16`, (215, 215, 215), containing two **key
   cells**, each 99 × 18 at y 0: the primary cell at x 229 and the secondary cell at x 330 (the
   headers' x, the headers' width minus 3). Both cells edit the action named by `exe`.

With the scroll-view placement rule, row *k* starts 3 units below the end of row *k* − 1 (the first
3 units below the top). There are 6 group rows and 59 command rows (65 rows, content height 1407):

| Group (key "text") | Commands (`id` "text" → `exe`) |
|---|---|
| `kb_grp_direction` "Направление" | `kb_left` "Влево" → `left`; `kb_right` "Вправо" → `right`; `kb_up` "Вверх" → `up`; `kb_down` "Вниз" → `down` |
| `kb_grp_movement` "Движение" | `kb_forward` "Вперед" → `forward`; `kb_backward` "Назад" → `back`; `kb_left_strafe` "Шаг влево" → `lstrafe`; `kb_right_strafe` "Шаг вправо" → `rstrafe`; `kb_jump` "Прыжок" → `jump`; `kb_crouch` "Присесть" → `crouch`; `kb_crouch_toggle` "Всегда в присяди" → `crouch_toggle`; `kb_always_run` "Шаг" → `accel`; `kb_sprint` "Бег" → `sprint_toggle`; `kb_left_lookout` "Выглянуть влево" → `llookout`; `kb_right_lookout` "Выглянуть вправо" → `rlookout` |
| `kb_grp_weapons` "Оружие" | `kb_weapon1`…`kb_weapon6` "Оружие 1"…"Оружие 6" → `wpn_1`…`wpn_6`; `kb_wpn_next` "Сменить тип патронов" → `wpn_next`; `kb_next_slot` "Оружие из следующего слота" → `next_slot`; `kb_prev_slot` "Оружие из предыдущего слота" → `prev_slot`; `kb_fire` "Огонь" → `wpn_fire`; `kb_zoom` "Приблизить" → `wpn_zoom`; `kb_reload` "Перезарядить" → `wpn_reload`; `kb_func` "Подствольник" → `wpn_func`; `kb_firemode_next` "Режим стрельбы след." → `wpn_firemode_next`; `kb_firemode_prev` "Режим стрельбы пред." → `wpn_firemode_prev` |
| `kb_grp_inventory` "Инвентарь" | `kb_inventory` "Инвентарь" → `inventory`; `kb_active_jobs` "Активные задачи в КПК" → `active_jobs`; `kb_map` "Карта в КПК" → `map`; `kb_contacts` "Контакты в КПК" → `contacts`; `kb_torch` "Фонарик" → `torch`; `kb_night_vision` "Прибор ночного видения" → `night_vision`; `kb_use_bandage` "Быстрое использ. бинта" → `use_bandage`; `kb_use_medkit` "Быстрое использ. аптечки" → `use_medkit`; `kb_drop` "Выложить" → `drop` |
| `kb_grp_common` "Общее" | `kb_pause` "Пауза" → `pause`; `kb_use` "Использовать" → `use`; `kb_screenshot` "Скриншот" → `screenshot`; `kb_quit` "Выход" → `quit`; `kb_console` "Консоль" → `console`; `ui_mm_save_game` "Сохранить игру" → `quick_save`; `ui_mm_load_game` "Загрузить игру" → `quick_load` |
| `kb_grp_multiplayer` "Сетевая игра" | `kb_artefact` "Артефакт" → `artefact`; `kb_scores` "Очки" → `scores`; `kb_chat` "Чат" → `chat`; `kb_chat_team` "Командный чат" → `chat_team`; `kb_buy_menu` "Меню покупки" → `buy_menu`; `kb_skin_menu` "Меню выбора модели" → `skin_menu`; `kb_team_menu` "Меню выбора команды" → `team_menu`; `kb_vote_begin` "Начать голосование" → `vote_begin`; `kb_vote_menu` "Проголосовать" → `vote`; `kb_vote_yes` "Проголосовать "Да"" → `vote_yes`; `kb_vote_no` "Проголосовать "Нет"" → `vote_no`; `speech_menu_group` "Голосовые сообщения (1)" → `speech_menu_0`; `speech_menu_personal` "Голосовые сообщения (2)" → `speech_menu_1` |

Absolute y of a row with the content scrolled to the top: group rows at 385, 497, 756, 1099, 1316,
1491; command row `kb_left` at 413, then +21 per command row within a group; key cells at absolute x
709 (primary) and 810 (secondary). Only the part inside the view (absolute y 382–563) is drawn.

**Key cell.** A frame line with texture base `ui_options_string` (`_b` 431, 213, 14 × 16; `_back`
445, 213, 32 × 16; `_e` 477, 213, 14 × 16) and a one-line text: the display name of the key in its slot,
or empty when the slot is empty. The cell's text is not configured by the data; it uses the defaults of
a text label: font `letterica16`, colour opaque white (255, 255, 255), left aligned at the cell's left
edge (offset 0), centred vertically in the cell's 18 units, simple (single-line) mode. The text `---`
of a cleared cell (section 11.4) is drawn the same way.

### 11.4 Capturing a key

- A *left press* on a cell puts it into the **waiting** state. While waiting, the cell's text blinks:
  its alpha follows the colour animation `ui_map_area_anim` (its colour keeps the cell's own RGB); the
  alpha returns to opaque when waiting ends.
- While waiting, the first of these events assigns a key and ends waiting:
  - a *left*, *right* or *middle press* with the cursor over the cell → `mouse1`, `mouse2`, `mouse3`;
  - any keyboard event that reaches the screen with a scan code in the key list (section 11.2) → that
    key. This includes Esc (the waiting cell consumes it, so it does not close the screen). The console
    key and F12 are handled by the menu before the screen sees them (section 13), so they cannot be
    captured.
  Other keyboard events are consumed and ignored while waiting.
- Waiting also ends, without any change, as soon as the cursor leaves the cell (the cell loses the
  hover state).
- Assigning shows the new key's display name in the cell and marks the cell changed. It does **not**
  change the bindings yet.
- **Conflicts** in the table: on assignment, every other cell of the table that shows the same key is
  cleared — its text becomes `---` and its slot empty (without marking it changed) — except cells of
  the same action, and except when one of the two actions is of kind 3 and the other of kind 5
  (section 11.1).

### 11.5 Saving

On Apply, for each changed cell, in table order: `unbind <action>` (primary cell) or
`unbind_sec <action>` (secondary cell), then, if the cell holds a key, `bind <action> <key>` or
`bind_sec <action> <key>` with the key's name from section 11.2; the cell is then unchanged.
Cancel and Esc do not save cells. On the next opening every cell's key is refreshed from the current
bindings, but its changed mark is kept (only saving clears it), so a later Apply saves such a cell
again; since it then holds the current binding, this has no effect on the bindings.

Binding commands:

| Command | Effect |
|---|---|
| `bind <action> <key>` / `bind_sec <action> <key>` | Sets the primary / secondary slot of the action (both names case-insensitive; unknown names: no effect). The key is then removed from both slots of every other action, except when one of the two actions is of kind 3 and the other of kind 5. Strings with `$$ACTION_…$$` are refreshed. |
| `unbind <action>` / `unbind_sec <action>` | Empties the primary / secondary slot. |
| `unbindall` | Empties every slot (and the console-command key bindings), then executes `$game_config$default_controls.ltx` as a script (section 12.2). |

### 11.6 Defaults button

`btn_default` executes `unbindall` at once (so the bindings become those of
`config\default_controls.ltx`: 58 `bind` lines, no secondary keys), then refreshes the mouse controls
and all key cells from the current values. The change is immediate and is not undone by Cancel. (A cell
that was changed before pressing the button stays marked changed, so Apply would still save it.)

## 12. Persistence: the user configuration file

### 12.1 Name and location

The user configuration file is named `user.ltx`; if the command line contains `-ltx ` followed by a
name, that name (up to the next space) is used instead. The folder is `$app_data_root$` (LTX
specification, section 10: `<Documents>\Stalker-SHOC\` in this edition's installation); how a name is
turned into a path differs slightly between reading (section 12.2) and writing (section 12.3). The
GOG installer copies `__support\userdocs\user.ltx` there.

### 12.2 Format and reading

A **configuration script**: one console command per line (CR LF line ends when written; any of CR, LF,
CR LF accepted when read), no sections. At startup the game executes `unbindall` (section 11.5, which
loads the default bindings) and then executes the user configuration file as a script
(`cfg_load <name>`). Executing a script `cfg_load <name>`:

1. The name's extension is replaced by `.ltx`: everything from the last `.` of the name is removed (if
   it has one) and `.ltx` is appended (so `-ltx foo.cfg` reads `foo.ltx`, and `-ltx foo` reads
   `foo.ltx`).
2. If `$app_data_root$` followed by that name exists, it is used; otherwise the name as it is now.
3. Each line (up to 1023 bytes) is executed as a console command; lines that are not valid commands
   only produce a console message. A missing file only produces a console message.

(When during startup each variable becomes available is outside this specification.)

### 12.3 Writing

- The file is written by the console command `cfg_save [<name>]` (no name: the user configuration
  file). The game executes `cfg_save` once, when it shuts down normally. **Apply does not write the
  file**; settings changed in a session are stored only at exit.
- Path: unless the name is at least 3 characters long and its second character is `:` (a drive
  letter path), it is prefixed with `$app_data_root$`; then its extension is replaced by `.ltx` as in
  section 12.2, step 1. If the file exists its read-only attribute is cleared first (if that fails,
  nothing is written).
- Content: the file is replaced by the lines each console command writes, command by command in
  ascending byte order of the command names (the order of the console's command table; it matches the
  shipped file):
  - every command except the three below writes `<name> <status>` + CR LF if its status (the text it
    reports as its current value) is not empty; in the shipped file only console variables do;
  - at the position of `bind`: one line `unbindall` + CR LF, then one line `bind <action> <key>` + CR LF
    per action with a primary key, in action order (section 11.1);
  - at the position of `bind_sec`: one line `bind_sec <action> <key>` + CR LF per action with a
    secondary key, in action order;
  - at the position of `bind_console`: one line `bind_console <command> <key>` + **LF only** per
    console-command key binding (a key bound directly to a console command line, which may contain
    spaces; the last word of a `bind_console` line is the key), in ascending order of the key's code.
    No control of this screen creates such bindings, and the shipped file has none.
- The `renderer` line is omitted when the command line contains `-r2` (which includes `-r2a`; the
  renderer was then forced by the switch, see the level-loading specification).
- Example lines (shipped file): `_preset Extreme`, `bind left kLEFT`, `mouse_sens 0.12`,
  `r2_slight_fade 0.3333`, `rs_c_gamma 1.`, `vid_mode 1024x768`.

### 12.4 The shipped file

`__support\userdocs\user.ltx`: 5,028 bytes, 239 lines, CR LF, variables in ascending byte order, 58
`bind` lines after one `unbindall` line, no `bind_sec` or `bind_console` lines. Values for this screen's variables are the
values after the slash in section 6.

## 13. Input summary

**Mouse.** Hover shows the `h` state of buttons, tabs and check boxes and the highlighted background
of drop-down lists. Left press: tab switch (section 4.3), check-box toggle (4.4), track-bar set and drag
(4.5), drop-down open/close (4.6), key-cell waiting (11.4). Left release: button click (4.2). Wheel:
scrolls the advanced page, the key table and an open drop-down list (4.7). Right and middle press are
used only by a waiting key cell. The cursor is the menu cursor (title-menu specification, section 7).

**Keyboard.** Esc (section 2.2); a waiting key cell takes any listed key (11.4). There is no keyboard
navigation between controls, no tab switching and no keyboard operation of sliders, lists or buttons.
Before a key press reaches the screen, the menu handles two keys itself (as for the title menu,
title-menu specification section 4.4): a key bound to the `console` action opens the console, and F12
(88) takes a screenshot; neither reaches the screen, even while a key cell is waiting.

All input goes to the topmost open window of the menu (the options screen, or the restart box while it
is open, section 10).

## 14. Differences when opened during a game

The screen, its controls and its behaviour are the same; the menu that opens it pauses the game
(title-menu specification, section 2). Differences that follow from the variables:

- Choosing a difficulty applies it at once to the running single-player game; in a multiplayer game
  only the variable changes (the console prints "For this game type difficulty level is disabled.").
- A video restart on Apply or Cancel resets the graphics device while the level is loaded (all
  textures reload).
- A renderer change still takes effect only at the next game start.

## 15. Data present but not used

- `_back_video` (`ui\ui_vid_back_02`), `tab_sound:_cap_soundquality` (`ui_mm_sound_quality`
  "Качество звука"), `tab_sound:_list_soundquality` (`snd_freq`), `tab_sound:btn_default_set`
  (`ui_mm_default`), `key_binding:list`: in the data, never created.
- The screen reacts to clicks of buttons named `btn_default_graphic` and `btn_default_sound` (they would
  reset every control of `mm_opt_video` / `mm_opt_sound` to the middle of its range, (*m* + *M*) / 2,
  rounded down for integers, setting the variables at once), but no such buttons are created.
- `config\rspec_maximum.ltx`.

## 16. Verification facts

- `config\ui\ui_mm_opt.xml` (winning copy in `gamedata.dbb`, 17,204 bytes): root `window`; 32
  `options_item` elements, 31 of them on created controls (only `snd_freq` is unused): 4 drop-downs
  (`renderer`, `_preset`, `vid_mode`, `g_game_difficulty`), 14 track bars (`rs_c_gamma`,
  `rs_c_contrast`, `rs_c_brightness`, `rs_vis_distance`, `r__geometry_lod`, `r__detail_density`,
  `texture_lod`, `r__tf_aniso`, `r__supersample`, `r2_slight_fade`, `r2_ls_squality`,
  `snd_volume_eff`, `snd_volume_music`, `mouse_sens`) and 13 check boxes (`rs_fullscreen`, `r2_sun`,
  `r2_sun_details`, `ai_use_torch_dynamic_lights`, `rs_v_sync`, `rs_refresh_60hz`, `snd_efx`,
  `hud_info`, `hud_crosshair`, `cl_dynamiccrosshair`, `hud_weapon`, `hud_crosshair_dist`,
  `mouse_invert`). Per page: basic video 3 + 3 + 1, advanced 0 + 8 + 5, sound 0 + 2 + 1, game 1 + 0 + 5,
  controls 0 + 1 + 1 (drop-downs + track bars + check boxes).
- Advanced page: 13 rows, content 390 units in a 313-unit view; row 12 control at absolute (664, 612).
- Key table: 6 group rows + 59 command rows = 65 rows, 118 key cells, content height 1407 units
  (6 · 28 + 59 · 21); last row (`speech_menu_personal`) at absolute y 1771 when scrolled to the top.
- Absolute rectangles: dialog (415, 168, 548, 461); pages at (470, 242); Apply (634, 597, 157, 48);
  Cancel (790, 597, 157, 48); tabs at x 470, 586, 702, 818, y 216; renderer list (664, 262, 261, 23).
- All 49 distinct caption keys used by `ui_mm_opt.xml` resolve in the string tables except `mm_mp_progress`
  (shown as the key until the download strip sets its text); all 65 keys of `ui_keybinding.xml`
  resolve.
- `config\default_controls.ltx` (`gamedata.dbb`, 1,111 bytes): 58 lines, all `bind`, first
  `bind quit kESCAPE`, last `bind quick_load kF7` (no final line break). The shipped `user.ltx` binds
  the same 58 action/key pairs, in action order.
- No wide-screen `_16` variant exists of `ui_mm_opt.xml`, `ui_keybinding.xml`, `message_box.xml` or
  `scroll_bar.xml`.

## 17. Not established

- Which physical wheel direction is reported as *wheel up*.
- The timing of the colour animation `ui_map_area_anim`.
- The drop-down's closed-line background in the disabled state (only `_e` and `_h` textures are given).
- The exact display names of keys on a given system (operating-system dependent).
- The value of the never-written backup byte of `check_mouseinvert` (section 5.2), and therefore
  which of the three listed Apply behaviours of that check box occurs. The byte is part of memory
  obtained from the general-purpose allocator without clearing; its content depends on earlier use of
  that memory and was not measured (no execution). It is not established that it is the same in every
  run.
- The criteria of the hardware test that disables the renderer list (section 2.1).
- Whether the frame line repeats its middle piece exactly as a frame window does (inferred).

## 18. Evidence (Russian edition)

| Path | Winning archive | Notes |
|---|---|---|
| `config\ui\ui_mm_opt.xml` | `gamedata.dbb` | 17,204 bytes (older copies in `db0`, `dba`) |
| `config\ui\ui_keybinding.xml` | `gamedata.dbb` | 3,279 bytes, root `keybingidg` (sic), 6 groups, 59 commands |
| `config\ui\message_box.xml` | `gamedata.dbc` | `message_box_restart_game` |
| `config\ui\scroll_bar.xml`, `config\ui\ui_common.xml` | | scroll-bar profile, texture ids |
| `config\text\rus\ui_st_mm.xml`, `ui_st_other.xml`, `ui_st_keybinding.xml`, `string_table_general.xml`, `ui_st_mm_mp.xml` | | caption texts |
| `config\default_controls.ltx` | `gamedata.dbb` | default bindings |
| `config\rspec_minimum.ltx` … `rspec_maximum.ltx` | `gamedata.dbb` | presets |
| `scripts\ui_mm_opt_main.script`, `ui_mm_opt_video.script`, `ui_mm_opt_video_adv.script`, `ui_mm_opt_sound.script`, `ui_mm_opt_gameplay.script`, `ui_mm_opt_controls.script` | `gamedata.dbb` | the screen's script data: which elements are created, in which order, and the button/tab/Esc reactions |
| `scripts\ui_main_menu.script` | `gamedata.dbb` | opening from the menu |
| `textures\ui\ui_mm_window_back_crop.ogm` (1,771,398 bytes), `ui_mm_rug02.ogm` (549,678), `ui_static_mm_back_03.dds` (1,048,704) | `gamedata.db8` | background media |
| `textures\ui\ui_common.dds` | `gamedata.db7` | 1024 × 1024 DXT5 |
| `__support\userdocs\user.ltx` | installer files | shipped user configuration |

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by
  the user; archives `gamedata.db0`…`gamedata.dbd` resolved by archive precedence with the project's
  archive tool.
- Data analysis: the files of section 18 (XML, LTX, scripts, string tables), texture descriptions,
  DDS headers and alpha, `ffprobe` of the videos; element rectangles and row positions computed by a
  throwaway script that builds the screen's element list from the XML by the rules of this document.
- Binary analysis: static analysis with Ghidra 12.2 (headless) of the shipped 32-bit binaries
  `xrGame.dll` (the options controls, their reading, saving and undo, the restart marks, the key
  table and key capture, binding commands, the variables registered there), `XR_3DA.exe` (console
  variable kinds, value formats, configuration-file name, reading and writing, the resolution list,
  refresh and gamma handling), `xrSound.dll` (sound variables' initial values), `xrRender_R1.dll` and `xrRender_R2.dll` (preset and
  renderer variables), `xrCore.dll` (whether newly allocated memory is cleared). No execution or
  debugging.
- No leaked source code, derived projects or third-party documentation of the engine were used.
- **Revision 2026-11-24** (after review; same edition and tools): added the basic video page's
  attachment order and the layering of open drop-down lists (re-read from the screen's script data and
  the generic window drawing in `xrGame.dll`); established the key-cell text attributes, the drop-down
  entry rows, selection and closed-line text, the check-box caption placement, the scroll bar
  (arrows, repeat, box size and position, dragging, wheel), the input routing while the restart box is
  open, and the menu's own handling of the console key and F12; corrected that mouse buttons are
  captured only over the cell and that waiting ends when the cursor leaves the cell; listed per preset
  file which other variables it sets; stated that refreshing a drop-down does not set its variable and
  how button captions are aligned.
- **Revision 2, 2026-11-24** (after a second review; same edition and tools, plus static analysis of
  `xrCore.dll`'s memory allocator): stated that `mm_opt_controls` and `key_binding` have no backup and
  what Apply then does for the mouse controls, with the cases for the never-written check-box backup
  (section 5.2); described the extension replacement when reading a script and the `bind_console`
  lines when writing; corrected the undo of check boxes (no refresh), the renderer list's handling
  when the hardware test fails (its backup makes Apply set `renderer_r1`), the drop-down refresh (entries
  rebuilt, text first, stored value 1 when nothing matches), that a track bar sets its variable on
  every mouse event with the button held, that an unknown preset name still runs the current preset's
  script, and the background's alpha region; renumbered section 4. Every other statement was
  re-checked against the data with new throwaway scripts (element list and rectangles, key table rows,
  caption keys through the string tables, texture rectangles, preset and binding files, the shipped
  user file, the media) and, for the variables' kinds, ranges and initial values, the action and key
  tables, the restart marks and the restart step, the configuration reading and writing, the binding
  commands, the drop-down, check-box and track-bar options behaviour and the resolution list, against
  new headless Ghidra runs over the shipped binaries.
- **Revision 3, 2026-11-24** (after a third review; same edition and tools): corrected section 5.2 —
  the mouse-sensitivity bar's backup is set to 0 at creation (re-read from the track bar's creation
  in `xrGame.dll`), so Apply always re-sets `mouse_sens`; only the check box's backup byte is
  uninitialised.
- **Revision 4, 2026-11-24** (after a fourth review; same edition and tools): corrected the input
  delivery order (reverse attachment order, capture first, the one event to a window the cursor has
  just left) and stated that showing/hiding also enables/disables a window (re-read from the generic
  window input dispatch and show operation and the script binding of the latter in `xrGame.dll`);
  re-established the arrow repeat from the menu's held-button event (`xrGame.dll`, and the mouse
  input update in `XR_3DA.exe`): strict > 100 ms, shared last-repeat time, three steps for a held
  click; stated that the renderer list shows the variable again after a later refresh, and that key
  cells keep their changed mark across Cancel/Esc.
