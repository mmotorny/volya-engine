# PDA window

Functional specification of the PDA window of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) in the
single-player game, with its seven tabs treated as empty: when the window opens and closes, which tab each
action opens, the tab bar and switching between tabs, what the window draws around the tabs (background,
clock, new-content marks), its input while open, what the tutorials' `pda_section` values do to it, and the
information portions it gives the player character. The content of each tab is the subject of the tab's own
specification (`pda-scope` section 4: PD-4, PD-7, PD-10, PD-12, PD-13, PD-15, PD-17).

It is PD-1 of `docs/behavior/pda-scope.md` (cited as `scope`) and builds on these approved specifications
without repeating them:

- `docs/formats/ui-layout.md` (cited as `layout`): layout files, rectangles relative to the parent, statics,
  texture ids, picture geometry (5.3), text (7), the 1024 × 768 UI space;
- `docs/formats/text-and-fonts.md` (cited as `text`): string tables, fonts, a string's width *S* (3.1), the
  UI scale factors *s*ₓ, *s*ᵧ (4.4);
- `docs/behavior/inventory-screen.md` (cited as `inv`): opening and closing an in-game window (1.1–1.4);
- `docs/behavior/console-key-bindings.md` (cited as `keys`): how a key press reaches the level and the
  in-game interface (section 5);
- `docs/behavior/save-load-screens.md` (cited as `saveload`): the `quit` key while an in-game window is open
  (section 2);
- `docs/behavior/options-screens.md` (cited as `options`): actions and default keys (11), caption colours
  by state (4.1), the tab control (4.3);
- `docs/behavior/title-menu.md` (cited as `title`): the menu cursor (section 7);
- `docs/behavior/hud-scope.md` (cited as `hud`): the HUD parts (the PDA flashing icon, spec 2/8; tutorials,
  spec 18);
- `docs/behavior/player-state.md` (cited as `pstate`): the inventory slots (3.3).

The controls the window shares with its tabs (frame lines and their titles, the hint box) are PD-2's
(`pda-controls`); this specification says only which of them the window uses and where.

Conventions:

- Coordinates are units of the 1024 × 768 UI space (`layout` 3). "The window" is the PDA window; "the
  character" is the player character. Text files are Windows-1251; paths are paths in the combined game-data
  tree.
- A **tab** is one of the seven pages of the window, numbered 0–6 in the order of the tab bar (section 3.1).
- Statements are static (read from the program and the data) (section 11). **Believed** marks an inference with its reason; **unestablished**
  marks what could not be settled.
- Every statement carries the id of its check in brackets: `D…` = `c_data.py`, others = `c_static.py`, all in `checks/pda-window/` of the dirty workspace.

## 1. Files and lifetime

- The window is built, with every tab window, when the level's single-player in-game interface is created
  (right after the inventory screen), and lives as long as that interface; its layout is read then and not
  again [W1]. Files:

| File | Read for |
|---|---|
| `config\ui\pda.xml` | the window (section 2) [W2] |
| `config\ui\ui_pda.xml` | the texture id `ui_PDA` (a texture-description file, `layout` 6) [D3] |
| `config\ui\pda_map.xml` (`map_wnd`) | the map tab, tab 1 [W5] |
| `config\ui\pda_events.xml` | the tasks tab, tab 0 [W5] |
| `config\ui\events_new.xml` | the diary tab, tab 2 [W5] |
| `config\ui\pda_contacts_new.xml` | the contacts tab, tab 3 [W5] |
| `config\ui\stalkers_ranking.xml` | the ranking tab, tab 4 (`scope` 1.1) |
| `config\ui\actor_statistic.xml` | the statistics tab, tab 5 [W5] |
| `config\ui\encyclopedia.xml` | the encyclopedia tab, tab 6 (`scope` 1.1) |

- `pda.xml` has no `_16` variant (`scope` 1.1), so the same file is used on every screen shape.
- The elements of `pda.xml` read, in this order: `main`, `background_static`, `auto_static` (children of
  that name: none in the shipped file), `mbbackground_frame_line`, `timer_frame_line`, `tab`,
  `updated_section_static`, `old_section_static` [W2, D1]. The file's first element, `off_button` (1 × 1 at
  (0, 0), texture path `ui\ui_pda_button`, which no archive holds, and a caption), is never read: its name
  occurs nowhere in `xrGame.dll` [W3, D12]. So the window has **no off button**; the string
  `pda_btn_off_hint` «Выключить» exists but is unused by it [D12].
- Outside single player all tab buttons except the map tab's are disabled after the window is built [W6]
  (multiplayer is not otherwise covered here).

## 2. Layout and drawing

### 2.1 Elements

| Element | Rectangle (relative) | Parent | Content |
|---|---|---|---|
| `main` | (0, 0) 1024 × 768 | UI space | the window's own rectangle; draws nothing itself [D2] |
| `background_static` | (3, 20) 891 × 643 | the window | picture `ui_PDA` [D2] |
| `mbbackground_frame_line` | (110, 574) 591 × 32 | background | frame line `ui_string_01` (PD-2), behind the tab bar [D5] |
| `timer_frame_line` | (710, 573) 161 × 32 | background | frame line `ui_string_01` with the clock as its title (2.3) [D5] |
| `tab` | (129, 573) 550 × 30 | background | the tab bar (section 3) [D6] |
| the active tab's window | its `main_wnd` (all at (110, 90), 761 wide) | background | the tab (its own specification) [D18, W4] |
| `updated_section_static`, `old_section_static` | 9 × 11 each, placed per tab (2.4) | none | the new-content marks [D10] |

- Positions are relative to the parent (`layout` 4.1): every element but the window and the marks is a child
  of `background_static`, so its absolute top-left is its own plus (3, 20) [W4]. Absolute: frame lines (113,
  594) and (713, 593), tab bar (132, 593), tab windows (113, 110).
- **Background.** `ui_PDA` is (0, 0) 1020 × 702 of `ui\ui_PDA` (`ui_pda.xml`) [D3], i.e.
  `textures\ui\ui_pda.dds`, 1024 × 1024 DXT5 [D4]. The element has no `stretch`, so the picture is drawn
  1020 × 702 units from the element's top-left (`layout` 5.3), covering (3, 20)–(1023, 722), larger than the
  element's 891 × 643 [D2, D3].
- **`rus_pda`.** The archives also hold `textures\ui\rus_pda\ui_pda.dds` and `textures\ui\eng_pda\ui_pda.dds`;
  neither folder name occurs in `xrGame.dll` or `XR_3DA.exe` [D19], and the window names only `ui_PDA`, so
  the Russian edition always draws `textures\ui\ui_pda.dds` (byte-identical to the `eng_pda` copy, `scope`
  1.2). Believed: no other path to those files exists (no binary, script or configuration file names them,
  `scope` 1.2).

### 2.2 Drawing order

Each frame the window draws, in this order [W4, L2]:

1. `background_static`: its picture, then its children in attachment order (`layout` 4.3):
   `mbbackground_frame_line`, `timer_frame_line`, the tab bar (its buttons, then its separators, 3.2), then
   the active tab's window (attached last, at each switch, 3.3);
2. the seven new-content marks (2.4), over everything else.

### 2.3 Clock

- Each update of the window while it is shown, the text **`<time> <date>`** is built from the game's current
  date and time: `<time>` = hours and minutes as `HH:MM` (two digits each, `:`), `<date>` = `DD/MM/YYYY`
  (day and month two digits, year four, `/`), joined by one space [K1, K2, K3]. When the text differs from
  the one last set, it becomes the text of `timer_frame_line`'s `title` (at (10, 7) 140 × 18 in the frame
  line, i.e. absolute (723, 600); font `letterica18`, white, `complex_mode` 0, no `align`, so left-aligned
  single-line text, `layout` 7, `text` 4.2) [K1, D5].
- Before the window's first update the title is empty (believed: nothing else sets it).

### 2.4 New-content marks

- The program keeps one **new-content flag per tab** (seven flags, all clear when the game starts; not
  saved — believed, as no save code reads or writes them) [N1, N5].
- Each frame, after the rest of the window, each tab *i* gets one mark drawn at the tab bar's absolute
  top-left plus the tab's mark point *p*ᵢ (3.2): `updated_section_static` (texture id `ui_pda_m_noupdate`)
  when tab *i*'s flag is set, otherwise `old_section_static` (texture id `ui_pda_m_update`) [L2, L3, D10].
  The two ids are (670, 998) and (680, 998), 9 × 11 of `ui\ui_common` [D11]. (The ids' names read opposite
  to their use; the program uses them as stated.)
- **Set** by (each also switches the HUD's PDA flashing icon on, 2.5) [N1, N2, N3]:
  - tab 0's flag (tasks): a task is given to the character, or a task's or objective's state changes
    (PD-6);
  - tab 2's flag (diary): a news item is added to the character's news list (PD-11); the same event sets a
    second flag the diary tab uses for its own marks (PD-12).
  No event sets the flags of tabs 1, 3, 4, 5 or 6 [N1].
- **Cleared**: tab *i*'s flag when tab *i* becomes the active tab (3.3), however that happens [T2]; the diary
  tab's own views also clear flags: showing its journal view clears **every** tab's flag, showing its news view
  clears the diary flag and its own second flag (PD-12) [N4].

### 2.5 HUD flashing icon

The HUD's flashing icon of type `pda` (`maingame.xml` `flashing_icons`: (13, 210) 48 × 29, texture
`ui_hud_icon_PDA`, colour animation `ui_slow_blinking_alpha`; `hud` spec 2) [D17, C2] is switched on by the
events of 2.4 and switched **off when the window closes** [C1, N2]. Opening the window does not switch it
off [O8]. How the HUD shows it is `hud`'s.

## 3. Tabs

### 3.1 The seven tabs

| Tab | Button text (`ui_st_pda`) | Hint | Window | Portion given on selection |
|---|---|---|---|---|
| 0 | `pda_btn_quests` «Задачи» | `pda_btn_quests_hint` | tasks (`pda_events.xml`) | none |
| 1 | `pda_btn_map` «План» | `pda_btn_map_hint` | map (`pda_map.xml`) | none (but `ui_pda_map_local`, section 8) |
| 2 | `pda_btn_diary` «Журнал» | `pda_btn_diary_hint` | diary (`events_new.xml`) | `ui_pda_events` |
| 3 | `pda_btn_contacts` «Контакты» | `pda_btn_contacts_hint` | contacts (`pda_contacts_new.xml`) | `ui_pda_contacts` |
| 4 | `pda_btn_ranking` «Ранги» | `pda_btn_ranking_hint` | ranking (`stalkers_ranking.xml`) | `ui_pda_ranking` |
| 5 | `pda_btn_statistics` «Данные» | `pda_btn_statistics_hint` | statistics (`actor_statistic.xml`) | `ui_pda_actor_info` |
| 6 | `pda_btn_encyclopedia` «Справка» | `pda_btn_encyclopedia_hint` | encyclopedia (`encyclopedia.xml`) | `ui_pda_encyclopedia` |

[D6, D8, D9, T1, T2]. Hint texts: «Показать информацию по квестам», «Показать карту Зоны», «Показать дневник
игрока», «Показать список контактов сталкеров», «Показать рейтинг лучших сталкеров», «Показать статистику по
прохождению», «Показать энциклопедию по Зоне» [D9].

### 3.2 Tab bar layout

- The seven buttons are `tab`'s `button` children (no other children), each 20 high at `y` = 7, caption
  font `letterica18`, `align="l"`, no texture, no `accel` [D6, D7].
- When the window is built, the tab bar is **laid out again** from left to right, overriding the buttons'
  `x` and `width` [L1]. Let *x* be the first button's `x` (0) and *y* its `y` (7), in the tab bar's
  coordinates. For each button *i* = 0 … 6 in order:
  1. if *i* > 0, a **separator** is added to the tab bar: a text element with the text `//`, colour
     (90, 90, 90) opaque, the button's caption font, placed at (*x*, *y*), as high as the button and as wide
     as its text (the string width *S*, `text` 3.1, divided by *s*ₓ); *x* grows by that width;
  2. the mark point *p*ᵢ = (*x*, *y* + ⌊(*h* − 11) / 2⌋), *h* = the button's height (20), so *p*ᵢ.y = 11;
  3. *x* grows by 12; the button is moved to (*x*, *y*) and its width becomes the width of its caption text
     (*S* / *s*ₓ, as for the separator); *x* grows by that width plus 3.
  So each tab shows: the mark (9 units), 3 units of space, the caption, 3 units, the grey `//`, the next
  mark. The `x` and `width` values of the file (0, 65, 120, 195, 275, 345, 440 and 50, 40, 60, 60, 53, 78,
  100) have no effect [D6, L1]. The divisor is the UI's horizontal factor *s*ₓ, the one by which UI units
  are multiplied into screen pixels (`text` 4.4) [L4]; the widths therefore depend on the font variant
  chosen for the screen (`text` 2.3).
- The separators are attached after the buttons, so they are drawn after them (2.2) [L1].

### 3.3 Selecting a tab

- **Mouse.** A left press on a tab button (as soon as it is pressed, no release needed) makes it the active
  tab and switches the window to it; a press on the active tab changes nothing [T4, T3]. This is the tab
  control of `options` 4.3 and `pda-controls` 5 (including its handling of a press counted as a double
  click).
- **No keyboard.** The buttons have no `accel`, so no key selects a tab inside the window [T5, D6]; the
  actions of section 4 select tabs 0, 1 and 3 from outside.
- **Caption colours** (`text_color` of each button, `options` 4.1): selected `t` (231, 168, 12); otherwise,
  with the cursor over the caption rectangle, `h` (255, 255, 255); otherwise `e` (127, 127, 127); `d`
  (181, 170, 148) only for a disabled button, which single player never has [T6, D7, W6].
- **Hints.** With the cursor over a button for more than 500 ms, its hint text (3.1) is shown in the shared
  hint box near the cursor (PD-2) [T7, D9].
- **What a switch to tab *i* does** (from any source: a button, an action of section 4, a tutorial item of
  section 7), when *i* is not already the active tab [T3, T2]:
  1. the previous tab's window, if any, is hidden and detached;
  2. tab *i*'s window is attached to the background (2.2) and shown;
  3. tab *i*'s button becomes the selected one, if it is not already;
  4. tab *i*'s new-content flag is cleared (2.4);
  5. the information portion of 3.1 is given, if the tab has one (section 8).
  When *i* is already active nothing happens (no portion, no flag change).
- When the window is built no tab is active [T3] (the tab bar itself starts with tab 0's button selected,
  `pda-controls` 5); every opening selects a tab first (section 4), so the
  window is never shown without one. The active tab is kept while the window is closed.

## 4. Opening

### 4.1 The actions

The key press reaches the in-game interface as `keys` 5 rule 3 describes (no in-game window takes it first,
or the PDA is the topmost window). The interface then acts when the game is **not paused** and the
controlled object is the **character with health above 0** [O1]:

| Action (number) | Default key | When no in-game window is open | When the PDA is the topmost window |
|---|---|---|---|
| `active_jobs` (53) | `kP` | switch to tab 0, then open | switch to tab 0, then close |
| `map` (54) | `kM` | switch to tab 1, then open | switch to tab 1, then close |
| `contacts` (55) | `kH` | switch to tab 3, then open | switch to tab 3, then close |

[O2, O3, O4, D16]. When another in-game window is the topmost one (inventory, trade, dialogue, body search,
level change) the three actions do nothing [O2]. In each case the press is taken (`keys` 5).
  **Disputed in review:** the press is taken only when the action acts (the two right-hand columns); when
  another in-game window is the topmost one, the game is paused or the character's health is not above 0,
  the press is not taken.

- "Then open / then close": after the switch the window's state is **toggled**: opened if it was closed,
  closed if it was open [O2, O4]. So with the window open, the key of another tab first switches to that tab
  (with the effects of 3.3: its flag is cleared and its portion given) and then closes the window; the
  window is not left open on the new tab. The next opening switches again to the tab of its key.
- **No PDA item is needed.** Nothing in these conditions refers to the character's items: the window opens
  whether or not slot 7 (`pstate` 3.3) or the inventory holds `device_pda` [O2]. Static only (not measured).
- Nothing else opens the window in single player except the tutorials (section 7); no script function opens
  it (`scope` 4, PD-1).

### 4.2 What opening does

- As for the inventory screen (`inv` 1.3), through the same opening path [O5]: the game is not paused; the
  menu cursor (`title` 7) is shown; the HUD indicators are hidden and the HUD flag of `inv` 1.3 cleared,
  both restored on closing; the actions `wpn_fire` and `wpn_zoom` are released; the window does not stop the
  character's movement.
- The window gives the character the information portion **`ui_pda`** (section 8) [O6]. When opened by an
  action whose tab was not active, the tab's portion (3.3) comes first, then `ui_pda` [O2, O6].
- No sound is played [X3] (none of the window's files names one, `scope` 1.1).

## 5. Closing

The window closes on any of:

1. the same three actions (4.1) while it is the topmost window [O2];
2. a press of the key bound to `quit` (default `kESCAPE`): the key is first offered to the window
   (`saveload` 2); no part of the window or of its tabs takes it (the map tab takes only numeric-keypad minus
   and plus, PD-4), so the window is closed and the main menu does **not** open on this press [C3, C4, C5].
   While the game is paused the window takes no key at all, so the `quit` key closes it then too [C4];
3. the character's death (health ≤ 0): the in-game interface's periodic update closes the topmost window if
   it is the inventory screen, the PDA, the dialogue window or the body-search window (`inv` 1.2 item 4) [C6];
4. a tutorial item (section 7).

There is no close button (1). Closing [C1, O4]:

- gives the character the information portion **`ui_pda_hide`** (section 8);
- switches the HUD's PDA flashing icon off (2.5);
- restores what opening changed (4.2: cursor, HUD indicators and flag);
- plays no sound [X3].

The active tab and its window's state are kept for the next opening (3.3).

## 6. While the window is open

- **Keyboard.** A key press is ignored while the game is paused (6, pause) [C4]. Otherwise it is offered to
  the window's elements (the tab bar takes none, 3.3; the active tab may take keys, its own specification)
  and, when none takes it, passed on to the character's controls as with the inventory screen (`inv` 1.3:
  the character can walk, crouch, jump, use, …), and it still reaches the in-game interface's handling of
  `keys` 5 [C4, O5]. Key releases and holds are passed on the same way (believed: the same handling as the
  inventory screen, whose input handlers the window shares) [O5].
- **Mouse.** Taken by the window as for the inventory screen (`inv` 1.3): buttons, movement and wheel never
  reach the character's controls; the cursor moves as in `title` 7 [O5].
- **Pause.** The `pause` key still pauses the game (`keys` 5 rule 2); while paused the window stays drawn and
  takes no keyboard or mouse input [C4]; `quit` closes it (5).
- **Console.** The console opens and works as without the window (`keys` 5 rule 2); the window stays open
  behind it (believed: nothing in the console's opening touches in-game windows).
- **Other windows.** The inventory screen does not open (its action needs no window open or the inventory
  topmost) [O2]; the body-search and box windows open only when no in-game window is open [P1]; the
  level-change window opens over any window, so the PDA stays open beneath it and gets no input until it
  closes [P1, P2]. The dialogue window's opening is the dialogue specification's (`scope` 3).
- **HUD flashing icon** events (2.4) keep arriving while the window is open; the marks update the same frame.

## 7. Tutorials (`pda_section`)

A tutorial item of the simple kind (`config\ui\game_tutorials.xml`; when items start and stop is `hud`
spec 18) reads its `pda_section` child as text (empty when absent) [U4]. The shipped files have 37
`pda_section` elements: 21 empty (`<pda_section/>`) and `pda_quests` (6), `pda_map` (3), `pda_closed` (2),
`pda_contacts`, `pda_diary`, `pda_ranking`, `pda_statistics`, `pda_encyclopedia` (1 each) [D15].

- **When the item starts** [U1, U2]: if the value is one of (compared ignoring letter case)

  | Value | Tab |
  |---|---|
  | `pda_quests` | 0 |
  | `pda_map` | 1 |
  | `pda_diary` | 2 |
  | `pda_contacts` | 3 |
  | `pda_ranking` | 4 |
  | `pda_statistics` | 5 |
  | `pda_encyclopedia` | 6 |

  the window switches to that tab (3.3) and is then opened if it is closed (4.2; it stays open if open).
  Any other value — `pda_closed`, or an empty value (an item without `pda_section`) — closes the window if it
  is open, and does nothing otherwise. So `pda_closed` has no meaning of its own: it is one of the values
  that close the window.
- These openings and closings do not test the conditions of 4.1 (pause, the character's health, other open
  windows) [U2].
- **When the item stops** (ends or is cut short), the window is closed if it is open, whatever the value
  [U3].
- Each frame, an item with an empty (or no) `pda_section` hides its own window while the PDA, the
  inventory screen, the dialogue, body-search or level-change window is open and shows it otherwise; an item
  with a non-empty value leaves its window as it is (`hud` spec 18) [U5].

## 8. Information portions

Of the 21 `ui_pda*` information portions of `config\gameplay\info_portions.xml` [D13], the program gives
these nine to the character; it gives them only in single player and only while the character exists [O7,
I1, D14]:

| Portion | Given when |
|---|---|
| `ui_pda` | the window opens (4.2) |
| `ui_pda_hide` | the window closes (5) |
| `ui_pda_events` | tab 2 (diary) becomes active (3.3) |
| `ui_pda_contacts` | tab 3 becomes active |
| `ui_pda_ranking` | tab 4 becomes active |
| `ui_pda_actor_info` | tab 5 (statistics) becomes active |
| `ui_pda_encyclopedia` | tab 6 becomes active |
| `ui_pda_map_local` | a PDA map is shown: tab 1 becoming active, and the tasks tab's map pane when it is shown (PD-4, PD-7) |
| `ui_pda_news_hide` | the diary's news view is hidden (PD-12) |

[O6, C1, T2, I2, I3]. `ui_pda` and `ui_pda_hide` disable each other in the file [D13]; what giving and
disabling mean is PD-8's. Tabs 0 and 1 give no portion of their own. The other twelve (`ui_pda_contacts_hide`,
`ui_pda_map_global`, `ui_pda_news`, `ui_pda_jobs`, `ui_pda_jobs_hide`, `ui_pda_jobs_current`,
`ui_pda_jobs_accomplished`, `ui_pda_jobs_failed`, `ui_pda_contracts`, `ui_pda_diary`, `ui_pda_diary_hide`,
`ui_pda_encyclopedia_hide`) are not named by the program and are never given by it [D14] (scripts may give
them, PD-8).

## 9. Talk mode, callback 10 and the PDA item

- **No talk mode.** The shipped window has exactly the seven tabs of 3.1 and no further page [T1]. The file
  `pda_dialog_character.xml` is read only by the statistics tab (its character panel, PD-17) [X1]. The
  message numbers 49 and 50 that the script API lists as `PDA_DIALOG_WND_BACK_BUTTON_CLICKED` and
  `PDA_DIALOG_WND_MESSAGE_BUTTON_CLICKED` (and 51, `PDA_CONTACTS_WND_CONTACT_SELECTED`) are not sent by any
  code found (believed never sent: a search of the whole program found no sender) [X4]. The constants
  `dialog_pda_msg`, `info_pda_msg`, `no_pda_msg` are values of a script enumeration of the game-object
  class, not window modes (dialogue specifications, `scope` 3).
- **Callback 10** (`inventory_pda` in the shipped API listing): believed never raised — none of the
  program's 44 look-ups of a script callback uses the number 10, while 11, 12 and 13 are used [X2]. Its
  meaning is therefore not established.
- **The PDA item** plays no part in opening the window (4.1).

## 10. Not covered here

The content and input of each tab (`scope` PD-4 … PD-17); the frame lines' drawing and the hint box (PD-2);
the HUD's PDA indicators and news lines (`hud`); multiplayer (only noted in 1).

## 11. Measured in a game

Nothing in this specification is measured: no black-box run could be made while it was written (another
run of the original held the machine). Every statement is static; the measurements still wanted are listed
in section 12.

## 12. Unestablished and believed

- Believed: the flags are not saved (2.4); the clock title is
  empty before the first update (2.3); key releases and holds pass to the character (6); the console leaves
  the window open (6); callback 10 is never raised and messages 49–51 never sent (9); `rus_pda` is never
  drawn (2.1).
- Not measured (static only): everything, in particular the tab opened by each action and the
  switch-then-close of 4.1, the window opening without the PDA item (4.1), the clock text (2.3), the tab-bar
  positions (3.2), the `quit` key while paused (5), closing on death and by tutorials (5, 7).

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), owned by the user;
  data from `gamedata.db0`–`gamedata.dbd` resolved by archive precedence with the project's archive tool
  (`checks/pda-window/extract.py`).
- Data examined: `config\ui\pda.xml`, `ui_pda.xml`, `ui_common.xml`, `ui_hud.xml`, `maingame.xml`,
  `game_tutorials.xml` (with includes), the tab windows' layout files (their `main_wnd` only),
  `config\gameplay\info_portions.xml`, `config\default_controls.ltx`, the string tables of
  `config\localization.ltx`, the DDS header of `textures\ui\ui_pda.dds`, the archive listing.
- Binary analysis: static analysis with Ghidra (headless; disassembly and decompilation) of the shipped
  `bin\xrGame.dll`: the window's construction, opening, closing, update, drawing and tab switching; the tab
  bar layout; the tab buttons' states and hints; the in-game interface's key handling, periodic update and
  window stack; the level's `quit` key handling; the tutorial items' start and stop; the giving of
  information portions; the HUD flashing icons; the game's date and time strings; the script callback
  look-ups; the action table. Printable-string searches of `bin\xrGame.dll` and `bin\XR_3DA.exe`.
- Execution: none.
- Checks: `checks/pda-window/c_data.py`, `c_static.py` (run with `python3 -I`; `run_all.py`
  runs them), with `ghidra.py` and `gh.py` re-creating the Ghidra dumps they read.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What happens to the seven new-content flags (2.4) when a saved game is loaded or a new game is started is not stated: nothing resets them except the clears of 2.4, so within one run of the program they keep their values across loads and new games.
- Whether a dialogue that starts while the window is open (a character talking to the player character, or the player character using one) closes the window, opens over it or is refused is not stated, though `scope` gives closing on another window opening to this specification.
