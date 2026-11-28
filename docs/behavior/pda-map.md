# The PDA map

Behaviour specification of the map view of the PDA of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the map tab and the tasks tab's map pane. It covers the pictures (the global picture and the
level pictures placed on it), zoom and movement, the tool bar, the scroll bars, the name label and the
hint window, how each map location's `level_map` mark, border and pointer are drawn (including the
pointer towards a level changer for a location on another level), and the player's mark and heading. It
is spec PD-4 (`pda-map`) of the approved scoping spec `pda-scope` (section 4).

It builds on the approved specifications `ui-layout`, `text-and-fonts`, `ltx-config`,
`colour-animations` (`docs/formats/`), `options-screens` (mouse events, the scroll bar of 4.8, the key
cell of 11.4), `save-load-screens` (wheel direction), `console-key-bindings` (key names), `world-coordinates`
and `player-movement` (position, yaw), `hud-scope` (spec 11, the map locations) and `pda-scope`
(`docs/behavior/`), cited by name. Of the scope's other proposed specs it relies on PD-1 (the PDA
window: when a tab is shown and which events reach it), PD-2 (the looks of the frames, frame lines,
buttons, hints and the `pda` scroll-bar profile) and PD-3 (the level-map data); where this spec needs a
fact of PD-3's data it states it here with its check.

Conventions:

- **Units** are those of the 1024 × 768 virtual screen of `ui-layout`. **Global units** are the
  coordinates of the global picture's `bound_rect` (section 1.2). **World** positions are (x, z) of the
  level, as in `world-coordinates`; *yaw* is as defined there (0 looking along +z, positive turning towards
  +x).
- Mouse events are named as in `options-screens` (*left press*, *move*, *wheel up*, *wheel down*,
  *double click*, …); keys by their DirectInput scan codes and the names of `console-key-bindings`.
- Every fact is established by static analysis of `bin\xrGame.dll` and `bin\XR_3DA.exe` and the game
  data unless marked **believed** (with why) or **unestablished**. Nothing here was measured by running
  the game.
- Every fact carries its check, a script in `checks/pda-map/` of the dirty workspace: `c1` =
  `c1_layout.py`, `c2` = `c2_config.py`, `c3` = `c3_binary.py`, `c4` = `c4_logic.py`, followed by the
  label of the assertion (e.g. c1 L2).

## 1. Data

### 1.1 The two map windows

The same map window is built twice: for the map tab from `config\ui\pda_map.xml`, element `map_wnd`, and
for the tasks tab's map pane from `config\ui\pda_events.xml`, element `main_wnd:right_frame:map_wnd`
(c3 B1). Below that element it reads (c3 B2): `main_wnd` (the map window's own rectangle),
`main_wnd:main_map_frame` (a frame window), `main_wnd:main_map_frame:level_frame` (called **the frame**
below: the area the map is drawn in), `main_wnd:map_header_frame_line` (a frame line) and, in it,
`tool_bar` with the four buttons `global_map_btn`, `actor_btn`, `zoom_in_btn`, `zoom_out_btn`, and
`main_wnd:text_hint` (the **label**, section 7.1). It reads no other element: the `notused_*` buttons of
both files (`notused_add_spot_btn`; in the tasks pane also `notused_highlight_spot_btn`,
`notused_remove_spot_btn`) are never created (no such name occurs in the binary, c3 B2).

| Element | Map tab (`pda_map.xml`) | Tasks pane (`pda_events.xml`) |
|---|---|---|
| `main_wnd` | 110, 90, 761 × 475 (c1 L1) | 0, 0, 409 × 475 in `right_frame` (352, 0) of `main_wnd` (110, 90) (c1 L4) |
| `main_map_frame` | 0, 0, 761 × 475, `a="90"`, `ui_pda_frame_sub` (c1 L1) | 0, 0, 409 × 475 (c1 L4) |
| `level_frame` (the frame) | 8, 41, 725 × 406 (c1 L1) | 8, 41, 373 × 406 (c1 L4) |
| the frame on screen | 118, 131 – 843, 537 (c1 L5) | 470, 131 – 843, 537 (c1 L5) |
| `map_header_frame_line` | 8, 17, 747 × 21 (c1 L1) | 8, 17, 395 × 21 (c1 L4) |
| `tool_bar` | 0, 6, 100 × 16 (c1 L1) | no rectangle attributes (c1 L4) |
| `global_map_btn` | 5, 1, 21 × 15, `check_mode="0"` | 0, 1, 21 × 15, `check_mode="0"` |
| `actor_btn` | 27, 1, 21 × 15, no `check_mode` | 22, 1, 21 × 15, `check_mode="0"` |
| `zoom_in_btn` | 120, 2, 21 × 15, `check_mode="1"` | 296, 2, 21 × 15, `check_mode="1"` |
| `zoom_out_btn` | 142, 2, 21 × 15, `check_mode="1"` | 318, 2, 21 × 15, `check_mode="1"` |
| `text_hint` | 735, 450, 100 × 22, `complex_mode="1"`, font `graffiti22` | the same (c1 L4) |

Button pictures and hints (c1 L2, c2 C9): `global_map_btn` `ui_icons_newPDA_compas`, hint
`global_map_btn_hint` "На карту зоны"; `actor_btn` `ui_icons_newPDA_showpers`, `actor_btn_hint` "Центр на
Меченом"; `zoom_in_btn` `ui_icons_newPDA_zoom`, `zoom_in_btn_hint` "масштаб +"; `zoom_out_btn`
`ui_icons_newPDA_ZoomMin`, `zoom_out_btn_hint` "масштаб -". How buttons, `check_mode`, button hints,
frames and frame lines look and react is PD-2's. The header line's `title` has no text and the program
sets none (c1 L3; **believed**: no text is assigned to it in the map window's set-up, c4 D27).

The window tree (c4 D27, D14): the map window has the rectangle of `main_wnd`; it holds `main_map_frame`,
which holds the frame, the header line (with the buttons) and the two scroll bars (section 5.6). The frame
holds the **global picture**, which holds one **level picture** per listed level, which holds the
**marks** of section 6. The label and the hint window (section 7) belong to no parent: their rectangles
are screen rectangles. Children are drawn after their parent, in the order they were added.

### 1.2 The pictures (`config\game_maps_single.ltx`)

`config\game.ltx` includes `game_maps_single.ltx` (c2 C1); the map reads it through `game.ltx`.

- **The level list** is the section `[level_maps_single]` (the multiplayer list `[level_maps_mp]` is used
  only outside single player, c4 D27): 17 names, `l01_escape`, `l02_garbage`, `l03_agroprom`,
  `l03u_agr_underground`, `l04_darkvalley`, `l04u_labx18`, `l05_bar`, `l06_rostok`, `l07_military`,
  `l08_yantar`, `l10_radar`, `l11_pripyat`, `l12_stancia`, `l12_stancia_2`, `l12u_control_monolith`,
  `l12u_sarcofag`, `l08u_brainlab` (c2 C2). Seven are written with capitals in the file (`L01_escape`, …,
  `l04u_LabX18`); every name is lower-cased before use (c4 D27). A name listed twice is a fatal error
  ("Duplicate level name not allowed", c3 B12). Each name is a level section and a string id (c2 C9).
- **The global picture** is the section `[global_map]`: `texture = ui\ui_global_map`, `bound_rect = 0, 0,
  1024, 2634` (x1, y1, x2, y2 in global units), `max_zoom = 6.0` (c2 C3; c4 D27). `max_zoom` is read only
  here; no level section has it (c2 C4).
- **A level picture** reads from its section `texture` and `bound_rect` (x1, y1, x2, y2: the world x
  range x1…x2 and z range y1…y2 the picture covers) and `global_rect` (x1, y1, x2, y2: where it lies on
  the global picture, in global units) (c3 B12, c4 D27). Without a `texture` key the picture is
  `ui\ui_nomap2` with bounds (−10000, −10000, 10000, 10000) and `bound_rect` is not read (c3 B12): this is
  the case of `l12u_control_monolith` and `l12u_sarcofag` (c2 C4). All 17 sections have `global_rect`,
  every textured one has `bound_rect`, and every texture exists (c2 C4). Other keys of these sections
  (`weathers`, `music_tracks`) are not read by the map.
- Sections with a `global_rect` that are not in the list get no picture: `l10u_bunker`, `l06u_labx16`,
  `l09_deadcity`, `andy_test`, `ai_test` and 14 multiplayer ones (c2 C5). Of the shipped single-player
  levels only `l10u_bunker` is not in the list (c2 C5): in it the player's level has no picture
  (section 5.2, 6.7).
- Each picture is its whole texture stretched over the picture's rectangle (`ui-layout` stretch rules;
  c4 D27).

### 1.3 Map spots data used here (`config\ui\map_spots.xml`)

The locations and their spot types are `hud-scope` spec 11's; this spec uses these elements:

- `actor_location` (hint `st_map_actor` "Меченый") and `actor_location_p` (hint `actor` "Одиночка"),
  each with a `level_map` spot only and no pointer: `actor_level_spot` (60 × 60, `alignment="c"`,
  `ui\ui_common` 730, 394, 60 × 60, no heading) and `actor_level_spot_p` (60 × 60, `alignment="c"`,
  `heading="1"`, `ui\ui_common` 794, 394, 60 × 60) (c1 L7, c2 C9).
- `level_map_spot_border`: 36 × 36, `alignment="c"`, `stretch="1"`, texture `ui\ui_common` 728, 576,
  51 × 51 with colour 200, 200, 200 (c1 L8). Its `texture_bk` child is not read: the name occurs in none
  of `xrGame.dll`, `XR_3DA.exe` and the XML parser (c1 L8, c3 B12).
- `quest_pointer` (the pointer of most types): 11 × 24, `heading="1"`, `alignment="c"`, `ui\ui_hud` 607,
  65 (c1 L9).
- The spot attribute `scale`: `scale="1"` makes a mark grow with the zoom (section 6.3); 30 spot
  elements have it (c1 L10, c3 B12, c4 D19).

### 1.4 The hint window (`config\ui\hint_item.xml`)

`hint_item`: 210 × 200 with a frame `frame` (0, 0, 210 × 200, texture `ui_icons_PDA_dialog`) and a text
`description` (10, 4, 192 × 192, `complex_mode="1"`, font `letterica18`, left aligned) (c1 L6, c3 B2).

### 1.5 Names the scope assigned to this spec that the map does not use

- `ui_icons_map.xml` is one of the 23 texture descriptions of `ui-layout` 6.1 (c2 C10). Its ids `death`
  and `artefact` are used by no layout file and no script (c2 C10); **believed** unused (the binary's
  strings `death` and `artefact` are a callback name and multiplayer names, not texture lookups). Its
  picture `ui\ui_icons_map` is used by map spots by path (e.g. `general_wounded_spot`), which is
  `hud-scope` spec 11's.
- `ui_map_area_anim` (a colour animation, `colour-animations`) is not used by the map window: its two
  uses in the binary lie outside the map code (c3 B15). One is the blinking key cell of
  `options-screens` 11.4; the other is in the text-line layout code (**unestablished** what it animates
  there).

## 2. Coordinates

### 2.1 The global picture and the zoom

- The **scale** *s* of the global picture is its width in units divided by the bound width
  (1024): units per global unit. The same *s* is used for both axes (c4 D25).
- **Set-up size** ("cover"): with the frame size *fw* × *fh* and bound size *bw* × *bh* = 1024 × 2634,
  if *bh*/*fh* < *bw*/*fw* the picture is *fh*·*bw*/*bh* × *fh*, otherwise *fw* × *fw*·*bh*/*bw*; it is
  placed at (0, 0) of the frame (c4 D1). So the picture always covers the frame.
- **Minimum zoom** *s*min = the set-up scale; **maximum zoom** *s*max = `max_zoom` (c4 D8, D27). Map tab:
  picture 725 × 1864.89, *s*min = 0.7080078; tasks pane: 373 × 959.46, *s*min = 0.3642578 (c2 C6). At
  *s*max = 6 the picture is 6144 × 15804 units.
- The map window keeps a **wanted zoom** *Z* (set-up: *s*min) and a **target point** *T* in global units
  (c4 D8). *Z* is always clamped to [*s*min, *s*max] when set (c4 D8, D11, c3 B6). "At minimum zoom" means
  |*s* − *s*min| < 0.001 (c3 B4, B8).
- **Offset**: the picture's top-left (*px*, *py*) relative to the frame. A global point (*gx*, *gy*) is at
  frame position (*px* + *s*·(*gx* − bx1), *py* + *s*·(*gy* − by1)) with (bx1, by1) = (0, 0) the bound's
  top-left (no flip; c4 D25).
- **Clamp** (used after a drag, an arrow key and in transitions): if *px* + width < *fw* then *px* = *fw*
  − width; if *py* + height < *fh* then *py* = *fh* − height; then if *px* > 0 then *px* = 0; if *py* > 0
  then *py* = 0 (c4 D2).

### 2.2 Level pictures

Every update, each level picture's rectangle is its `global_rect` converted to the global picture: top-left
= frame position of (x1, y1), bottom-right = of (x2, y2) (c4 D16). Level pictures and the global picture are
clipped to the frame (c4 D14).

### 2.3 World position on a level picture

For a level picture with `bound_rect` (x1, y1, x2, y2) and current width *W* units, let *k* = *W* /
(x2 − x1). A world point (*wx*, *wz*) is at (*k*·(*wx* − x1), *k*·(y2 − *wz*)) from the picture's
top-left (c4 D25): x grows with world x, y grows towards −z (north up), and the **width ratio is used
for both axes**. In global units: *gx* = gx1 + (*wx* − x1)·*r*, *gy* = gy1 + (y2 − *wz*)·*r*, *r* = (gx2 − gx1)
/ (x2 − x1), with (gx1, gy1, gx2, gy2) the `global_rect`. Where `global_rect` and `bound_rect` differ in
aspect, marks do not lie where the stretched picture shows the place: for 15 of the 16 textured levels
the aspects agree within 0.1 %, `l08u_brainlab`'s `global_rect` is 4.74 times taller relative to its
`bound_rect` (c2 C7).

Example: the New Game start point (−246.72630, z −134.43387; `world-coordinates` 4) is at global (376.559,
2300.566) (c2 C8).

## 3. Showing and hiding

- Each time a map window is **shown**: the global picture's and every level picture's clip is set to the
  frame's screen rectangle and the level pictures are put back on the global picture; the hint window is
  closed; and, in single player with a player present, the player is given the information portion
  `ui_pda_map_local` (its meaning is PD-1's / PD-8's) (c4 D15, c3 B13).
- The **first time** each of the two map windows is shown it does what the actor button does (section
  5.2) (c4 D15, c3 B13). Since it starts at minimum zoom, this flies to the player at maximum zoom.
  Worked example, map tab after a New Game on Escape: target picture rectangle (−1896.86, −13600.40),
  6144 × 15804; first duration 6.539 s; the player point starts outside the grown frame, so the approach
  phase of 4.2 runs first (c2 C12).
- When **hidden**: the level pictures are taken off the global picture and the hint window is closed
  (c4 D15).
- Transitions (section 4) advance in the map window's per-frame update (c4 D14); **believed** only while
  it is shown (updates go to shown windows).

## 4. Transitions

A **transition** moves the global picture's rectangle smoothly towards the rectangle that shows *T* at
zoom *Z*.

### 4.1 Target rectangle

Width 1024·*Z*, height 2634·*Z*; top-left (*fw*/2 − *T*.x·*Z*, *fh*/2 − *T*.y·*Z*) (centring *T* in the
frame), then clamped as in 2.1 (c4 D3).

### 4.2 Phases and starting

A transition is requested by the operations of section 5 (they set *Z* and/or *T* first). The map then
works towards the state "reached" through two phases (c3 B10, c4 D6, D7):

- **Near test**: *T* counts as near when its current frame position lies inside the frame grown by its
  own width on the left and right and by its own height above and below (a 3 × 3 frame area); once true
  for a request it stays true until the request is reached (the result is stored, c4 D6).
- **Approach** (when *T* is not near): starts towards the minimum zoom with an end time 0.5 s later; on
  its first step, if *Z* differs from *s*min by 1e-5 or more, it is restarted towards *Z* with the
  duration rule of 4.3 (c4 D5, D7, c3 B9). So it ends after exactly 0.5 s only when *Z* is the minimum
  zoom. As soon as *T* becomes near, the approach gives way to the settle phase.
- **Settle** (when *T* is near and the target is not reached): starts towards (*Z*, *T*) with the
  duration rule (c4 D7).
- When the settle or approach phase reaches its end time the state is "reached" and the map is idle
  until the next request (c3 B9, B10).
- A new request while a phase runs: if the phase stays the same it keeps its target rectangle and end
  time, except that a change of *Z* by 1e-5 or more restarts it from the current rectangle towards the
  new (*Z*, *T*) with a new duration (c4 D5); if the phase changes (approach ↔ settle) the new phase
  starts from the current rectangle (c3 B10).

### 4.3 Duration

Let *d* be the distance in global units between the global point at the frame's top-left corner now and
in the target: *d* = √((*px*/*s* − *px*′/*Z*)² + (*py*/*s* − *py*′/*Z*)²), with (*px*′, *py*′) the target
offset (c4 D3). With
end time = now + *t* (c4 D4, c3 B9):

- zoom unchanged (|*Z* − *s*| < 0.001): *d* < 0.001 → *t* = 0 (the target is taken at once); otherwise *t*
  = max(*d* / 350, 0.25 s);
- zoom changing: *d* < 0.001 → *t* = 0.5 s; otherwise *t* = max(*d* / 350, 0.5 s).

### 4.4 Each frame

With *r* = end time − now and the frame's time step Δ: if now ≥ end time the rectangle becomes the target
rectangle; otherwise each of its four edges moves the fraction min(Δ, *r*) / *r* of the way to the target
(c4 D5). The picture is then updated and the scroll bars follow (section 5.6). Linear in time when Δ is
constant.

## 5. Controls

### 5.1 Show-a-point

The operation used by the actor button, the first showing and the tasks tab (PD-7 shows a task
objective's location with it, as **believed** from the tasks code's use of it): given a level picture
and a world point, *T* = that point in global units (2.3); if it is asked with "zoom up" and the map is at
minimum zoom, *Z* = *s*max; otherwise *Z* stays (c4 D8). Given the global picture instead: *Z* = *s*min
and *T* = the global point at the frame's centre now (c4 D8). The hint window is not closed. A transition
is requested.

### 5.2 Actor button (`actor_btn`)

Pressing it (PD-2's button behaviour) takes the player's position (x, z) and the current level's name; if
the name is in the level list, show-a-point on that level's picture with "zoom up"; otherwise the log gets
`~ Level Map '<name>' not registered` and show-a-point on the global picture (c4 D9, c3 B12).

### 5.3 Global map button (`global_map_btn`)

Show-a-point on the global picture: back to minimum zoom around the current centre (c4 D8, D10).

### 5.4 Zoom tools (`zoom_in_btn`, `zoom_out_btn`)

The two buttons are toggles (`check_mode="1"`, PD-2). Pressing one turns its tool on if the button is now
checked and turns the other tool off; both buttons then show their tools' state, so at most one tool is on
and pressing the checked one turns it off (c4 D10). With a tool on, a *left press* that reaches the map
window (not taken by a button, the scroll bars or another element under the cursor) multiplies *Z* by 1.5
(zoom in) or 2/3 (zoom out), clamped; if *Z* changed by at least 1e-5, *T* becomes the global point under
the cursor, the hint window is closed and a transition is requested; otherwise nothing happens (c3 B6,
c4 D11). The press may be anywhere in the map window, also outside the frame. A second press within
250 ms is a *double click* and does not zoom (c3 B7).

### 5.5 Wheel

*Wheel down* zooms in (× 1.5), *wheel up* zooms out (× 2/3), as the zoom tools but with no tool needed
(c3 B6, c4 D11). The program reports a positive wheel movement of DirectInput as *wheel up* and zero or
negative as *wheel down* (c3 B7, c4 D13); DirectInput reports rotating the wheel forward (away from the
user) as positive (documented by DirectInput; the same direction as `save-load-screens`). So: wheel
forward zooms out, backward zooms in. Over a scroll bar the bar takes the wheel instead (**believed**:
`options-screens` 4.8 scroll bars use it, and elements under the cursor get mouse events first).

### 5.6 Moving directly

These move the picture at once, without a transition, and do not change *T* (c4 D11, D12, D26):

- **Drag**: a *move* with the left button held and the cursor inside the global picture's rectangle (its
  whole unclipped rectangle, so also outside the visible frame as long as the event reaches the map
  window) moves the picture by the cursor's movement, then clamps; the hint window is closed (c3 B6,
  c4 D11). Without a button held, a *move* over a picture only updates the label (section 7.1).
- **Arrow keys**: each *hold* event of Up / Down / Left / Right (DirectInput 0xC8, 0xD0, 0xCB, 0xCD) moves
  the picture 1 unit: Up +1 in y, Down −1 in y, Left +1 in x, Right −1 in x (the view moves the way of the
  arrow), then clamps; the hint window is closed (c3 B5, c4 D12). Holds are sent for every key that is down
  at each input update (c4 D12), **believed** once per frame.
- **Scroll bars**: a horizontal bar at (frame left, frame bottom + 5) of the frame's width and a vertical
  bar at (frame right + 5, frame top) reaching to the bottom of the horizontal bar, both with the scroll-bar
  profile `pda` (PD-2; thickness 17 in that profile, **believed** to be the bar width) (c3 B3). Values (as
  `options-screens` 4.8): range 0 … trunc(picture width) (horizontal) or trunc(picture height)
  (vertical), page = trunc(frame width / height), step = max(1, trunc(0.1 · frame width / height)); their
  positions follow the picture: trunc(−*px*) and trunc(−*py*), kept in [0, range − page + 1] (c4 D26).
  Moving a bar sets *px* = −position (or *py*); this is not clamped (c4 D26). The horizontal bar is
  **believed** to behave as the vertical bar of `options-screens` 4.8 turned on its side.

### 5.7 Keyboard zoom

Numpad + (`kADD`, 0x4E) multiplies *Z* by 1.5, numpad − (`kSUBTRACT`, 0x4A) by 2/3, clamped, on **every
keyboard event of that key, press and release alike**, so one press-and-release zooms twice (c3 B5,
c4 D12). *T* is not changed (the transition goes to the last target point, not the current view centre)
and the hint window is not closed; a transition is requested even when *Z* did not change (c4 D12).
Keyboard events reach the map window only when its parents pass them on: every shown child gets them
until one takes the event (c4 D12); which events the PDA window passes to its tabs is PD-1's.

### 5.8 Other input

Right and middle presses and double clicks do nothing in the map, and a press on a picture has no action
of its own (with a zoom tool on it zooms as in 5.4). There is no
click action on a level picture or a mark (c4 D11, D16, D18). There is no separate level view and no
switching between a "global" and a "level" mode: the level pictures lie on the global picture, and a level
is "chosen" only by zooming into its picture (5.4, 5.5) or by show-a-point (5.1); the global map button
zooms back out (5.3).

## 6. Marks

### 6.1 When marks are drawn

Each level picture removes all its marks every update and adds back the ones of this frame (c4 D20). It
adds marks only when the map is **not** at minimum zoom and the level picture's rectangle overlaps the
frame (c3 B8, c4 D20). At minimum zoom no marks are drawn at all, including the player's.

For every map location (`hud-scope` spec 11) that is current (not past its lifetime and with its object
present, online or in the world simulation; c4 D20) and has a `level_map` spot that is shown (c4 D21):

- if the location's level is this picture's level: its mark, border and pointer (6.2–6.5);
- otherwise, if this picture is the player's current level: the pointer towards the level changer (6.6).

### 6.2 Conditions on the object

For a location bound to an object (not one placed by the player), while the world simulation runs
(c4 D21, c3 B14):

- its object must exist in the simulation, else nothing is drawn and the log gets a "critical" line
  with the object's id;
- a type with `no_offline` draws nothing while its object is offline;
- the object's **`visible_for_map`** property must be on. Every simulation object starts with it on
  (c3 B14); scripts change it through the object's `visible_for_map(bool)`, which the shipped scripts do
  only in `stalker_generic` (from a character's logic key `show_spot`, off for the `zombied` community;
  c2 C11). So `visible_for_map` does limit what both the PDA map and the minimap draw (the code is shared;
  c4 D21). Whether spawn data can store it off is **unestablished** (believed not: no reader found).

### 6.3 The mark

The mark is a static element built from the spot's `map_spots.xml` element (`ui-layout`, incl. `xform_anim`
of `colour-animations`) (c4 D19). Its position is the location's world position (x, z) converted by 2.3
(c4 D23, D25); with `alignment="c"` that is its centre. It is added only if its rectangle overlaps the
frame (c4 D21), and drawn clipped to the frame. If the spot has `heading`, its angle (`ui-layout` 5.4) is
*a* = −yaw of the location's direction (c4 D21, D23): for the object the player is viewing through, the
camera's direction; for others the object's own direction. The heading of a direction (x, z) is
−atan(x/z), less π when z < 0, and for |z| < 1e-7: 0 if |x| < 1e-7, −π/2 if x > 0, +π/2 otherwise (c4 D23,
c3 B4); this equals −yaw modulo 2π. With `ui-layout` 5.4 (positive angle counter-clockwise), a mark
drawn pointing up turns clockwise as the yaw grows, i.e. it points the way the player looks on the north-up
map. A spot with `scale="1"` is sized (its layout width and height) × *s* every frame; other marks keep
their layout size at every zoom (c4 D19).

### 6.4 The border

In single player, when the location's pointer is on and its type has a pointer (the state and its setters
are `hud-scope` spec 11's), the border `level_map_spot_border` is put at the mark's position, after the
mark (so over it) (c4 D21, c3 B12). It is added whether or not the mark is inside the frame; whether it is
clipped to the frame is **unestablished**.

### 6.5 The pointer

When the location's pointer is on and the mark is not fully inside the frame shrunk by 5 units on every
side, the pointer is drawn (c3 B11, c4 D21): on the line from the frame's centre to the mark, where it
crosses the frame's edge (the mark's own position if it is inside), moved half the pointer's width
towards the centre and truncated to whole units; rotated (`ui-layout` 5.4) by *a* = atan2(*d*x, *d*y) with
*d* the unit vector from the mark to the frame centre in screen units (y down), which turns the picture's
upward direction to point from the centre towards the mark (c4 D24); clipped to the frame. A location has
at most one pointer per frame (the first picture to place it wins; which one when two could is
**unestablished**). A pointer has no hint.

### 6.6 Locations on another level: the level-changer pointer

On the level picture of the player's current level, for a location on another level whose pointer is on
(c4 D22): the program finds a path over the game graph from the player's graph point to the location
object's graph point (for a location placed by the player, its stored graph point), then the level changer
of the current level whose graph point lies on that path (scanning the path from its end, so the one
latest on the path), and draws the location's pointer as in 6.5 towards that changer's position (x, z),
even when the changer is inside the frame. The location's own mark is drawn only on its own level's
picture (when that picture is visible and the map zoomed in). If no changer of the current level lies on
the path, nothing is drawn; a debug message (`Error. Path from actor to selected map spot does not contain
level changer :(` followed by the path and the changers) exists (c3 B14) but is printed only under a
switch **believed** off in the shipped game. How the path is searched (weights, ties) is **unestablished**.
`map_spots_lchangers.xml` (the changers' own spots) is `hud-scope` spec 11's.

### 6.7 The player's mark

When the player is spawned in a single-player game, the program adds two locations bound to
the player: `actor_location` and `actor_location_p` (c3 B14). Both are drawn by 6.1–6.3 on the player's
current level's picture: `actor_level_spot` (a fixed 60 × 60 picture) and over it `actor_level_spot_p` (60 ×
60, turned by the camera's yaw, 6.3), centred on the player's position, at the same size at every zoom (no
`scale`) (c1 L7). Neither has a pointer or border. Their hints are "Меченый" and "Одиночка" (c2 C9). In a
level without a picture (`l10u_bunker`) the player has no mark (1.2). Their order relative to other marks
follows the location list (`hud-scope` spec 11).

## 7. Label and hint window

### 7.1 The label (`text_hint`)

A one-element text at the screen rectangle 735, 450, 100 × 22 (both windows; for the map tab inside the
frame's lower right, for the tasks pane outside the pane), font `graffiti22`, `complex_mode="1"` (c1 L3,
L4). **Disputed in review:** the rectangle (735, 450 – 835, 472) is not outside the tasks pane: it lies
inside the tasks pane's frame (470, 131 – 843, 537, section 1.1), in its lower right, as in the map tab. It
is drawn every frame after the map (c4 D14). A *move* without a button held over a level picture
sets its text to that level's name translated through the string table (`l01_escape` "Кордон"); over the
global picture outside every level picture, to `global_map` "Карта Зоны" (c4 D17, c2 C9). Only these two
set it (c3 B16): it starts empty (**believed**: nothing sets it at set-up) and is never cleared. Moves are
passed to the pictures only while the cursor is inside the frame, so only hovering inside the frame
changes it (**believed** from the rule that elements get mouse events only under the cursor,
`options-screens` 4.1).

### 7.2 The hint window

One hint window per map window, from `hint_item` (1.4); it shows a text near the cursor (c4 D18).

- **Opening**: the cursor's coming over an element starts a 500 ms clock (milliseconds of the game clock;
  c4 D18). After more than 500 ms over a mark (or border), the hint opens with the location's hint id
  translated through the string table (a missing key shows the id itself, `text-and-fonts`; the default
  id of a type without `hint` is `no hint`, which is not a key, c2 C9) (c3 B8, c4 D18). After more than
  500 ms over a level picture **at minimum zoom**, it opens with the level's name translated; when the map
  is not at minimum zoom a level's hint is closed (c3 B8, c4 D16). The hint opens only if none is open and
  the cursor is inside the frame (c4 D18).
- **Size**: width 210; the text is laid out in the description's width (192) and the window's height is
  max(64, 4 + text height + 20) (c3 B11, c4 D18).
- **Place** (c4 D18, c3 B11): with the cursor at (*cx*, *cy*) and the hint *w* × *h*: first above and to the
  right ((*cx*, *cy* − *h*) – (*cx* + *w*, *cy*)); if that is not strictly inside the frame, above and to
  the left; if still not, below and to the left; if still not, below and to the right, a further 45 units
  down (used whatever it covers).
- **Closing**: when the cursor leaves the mark that opened it; when the level picture that opened it is
  seen away from minimum zoom; on a drag, an arrow-key move, a zoom-tool or wheel zoom that changes *Z*,
  and on showing and hiding the map window (c4 D11, D12, D15, D16, D18). Show-a-point and the keyboard
  zoom do not close it.
- It is drawn after the label, over everything of the map window (c4 D14).

## 8. Unestablished and believed

- Whether transitions advance while the map window is hidden (believed not, section 3).
- The draw order among overlapping level pictures (it follows an internal ordering of the names); e.g.
  `l12u_sarcofag` and `l12u_control_monolith` share one `global_rect`, as do `l12_stancia` and
  `l12_stancia_2` (c2 C4).
- Whether the border is clipped to the frame (6.4); which picture places a pointer when two could (6.5).
- The path search of 6.6 and whether its debug message can appear.
- Whether spawn data can switch `visible_for_map` off (6.2).
- What `ui_map_area_anim` animates in the text-line code (1.5); that `death` and `artefact` are unused
  (believed, 1.5).
- That the scroll bars take the wheel over them and that the horizontal bar mirrors the vertical one
  (believed, 5.5, 5.6); the bars' exact thickness (PD-2's profile).
- That PD-7 shows a task objective with show-a-point and "zoom up" (believed from its call, 5.1).
- Which keyboard and mouse events the PDA window passes to the map window (PD-1).
- Nothing here was measured on the running game.

## Provenance

- Date: 2026-11-26. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries
  `bin\xrGame.dll`, `bin\XR_3DA.exe`; game data from archives `gamedata.db0` … `gamedata.dbd` (winning
  copies extracted with the project's archive tool for the `pda-scope` checks, reused read only).
- Data examined: `config\ui\pda_map.xml`, `pda_events.xml`, `hint_item.xml`, `map_spots.xml` with its
  includes, `ui_icons_map.xml`, `scroll_bar.xml`; `config\game.ltx`, `game_maps_single.ltx`,
  `game_levels.ltx`, `system.ltx`; the string tables; the scripts; the level folders and the pictures'
  presence.
- Analysis: static analysis of the two binaries (decompilation and disassembly with Ghidra in headless
  mode, string and constant scans); reading of the layout, configuration and script files. No debugging,
  no execution of the game.
- Checks: `checks/pda-map/c1_layout.py`, `c2_config.py`, `c3_binary.py`, `c4_logic.py` with `mlib.py`,
  run with `python3 -I` (`run_all.py` runs them all; `c4` runs Ghidra read only on project copies in the
  dirty workspace); all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The four tool-bar buttons' actions, the drag (5.6) and the label's update over a level picture (7.1) are skipped while a state flag of the global picture is set (the zoom buttons then still toggle, PD-2, but no tool changes); what sets that flag, and whether it is ever set in the shipped game, is not stated.
- How a location type's `level_map` element in `map_spots.xml` and its includes is read (its `spot` and `pointer` attributes naming spot elements; what a missing `pointer` means) is not stated; the scope assigns these `level_map` halves to this spec.
- What closes a level picture's hint (opened at minimum zoom, 7.2) when the cursor leaves that picture while staying at minimum zoom is not stated.
- Whether the object conditions of 6.2 (`no_offline`, `visible_for_map`) apply to the level-changer pointer of 6.6 is not stated (the program checks neither on that path).
