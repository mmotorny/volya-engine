# HUD: the pick-up icon and the hint line

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of two parts of the in-level
HUD (`docs/behavior/hud-scope.md`, spec 13): the **hint line**, the line of text near the bottom of the screen
that says what the use action would do with the object under the crosshair, and the **pick-up icon**, the
inventory icon of the item the pick-up target step has chosen. It covers which hint the player has for each kind
of object under the crosshair, where its text comes from, and how and where the line and the icon are drawn.

Builds on (not repeated):

- `docs/behavior/use-action.md` (`UA`): the crosshair's pick (2.1), the crosshair record and its parts (2.2),
  characters, holders, items (conventions), the target item (9.1), `[ph_capture_visuals]` (7.1), the usable
  flag (10).
- `docs/behavior/console-misc-settings.md` (`MS`): the pick-up target and `cl_cod_pickup_mode` (5), `g_autopickup` (2).
- `docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md` (`P`): `set_tip_text`,
  `set_tip_text_default` (8).
- `docs/behavior/script-api-object-binders.md` (`OB`): when a binder's update runs (5.2).
- `docs/formats/colour-animations.md` (`CA`): interface colour animations, their clock and start (4.2, 4.3), the
  entry evaluation (3).
- `docs/formats/ui-layout.md` (`UL`): the UI space (3), windows (4), statics, their pictures and `stretch`
  (5), text (7); `docs/formats/text-and-fonts.md` (`TF`): string-table lookup (1.4), `$$ACTION_…$$` (1.6), complex
  mode (4.3); `docs/behavior/script-api-window-description-files-fonts-and-textures.md` (`WF`): the "has no
  entry" log line (6).
- `docs/behavior/inventory-screen.md` (`IS`): item icons (4.2).
- `docs/behavior/hud-scope.md` spec 2 (`hud-visibility`, not yet specified): whether the HUD, and so these two
  parts, is drawn at all, and the order of the HUD parts.

Conventions:

- The **player**, **single play**, **character**, **item**, **holder**, **usable part**, **hint text** and the
  **crosshair record** are `UA`'s. A **living kind** is an object that has health of its own: the player,
  stalkers, the trader and every monster class; every one of them is also a character (static: the only living
  kinds without an inventory are abstract bases that are never made) [H2h].
- The **player's hint** is the text the player holds for the hint line (section 1). It is either **none** or a
  text (possibly empty: an empty text is not none, [H6, H6b]).
- A **lookup** is the string-table lookup of `TF` 1.4: an entry's text, otherwise the key itself.
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements are
  marked **believed**, with why; what could not be established is marked **unestablished**.
- Bracketed ids are checks in `checks/hud-pickup-hint/checks.py` (`python3 -I checks.py` runs them all).

## 1. The player's hint

### 1.1 The built-in texts

When the player is loaded it keeps six string-table keys, fixed in the program [H1]:

| Key | Text (lookup, Russian tables) | Table |
|---|---|---|
| `character_use` | `Говорить ($$ACTION_USE$$)` | `string_table_level_tips` |
| `dead_character_use` | `Обыскать труп ($$ACTION_USE$$)` | `string_table_level_tips` |
| `dead_character_use_or_drag` | `Обыскать труп ($$ACTION_USE$$) \n Тащить труп (SHIFT+$$ACTION_USE$$)` | `string_table_level_tips` |
| `car_character_use` | `Открыть дверь ($$ACTION_USE$$)` | `string_table_level_tips` |
| `inventory_item_use` | `Поднять предмет ($$ACTION_USE$$)` | `string_table_general` |
| `inventory_box_use` | `Обыскать ящик ($$ACTION_USE$$)` | `string_table_ui` |

Each key has exactly one entry in the loaded tables (`config\localization.ltx` `files` and the includes of
`string_table_includes`; `string_table_tips.xml`, which repeats three of them, is not loaded) [H5].
`$$ACTION_USE$$` becomes the name of the key bound to `use` (`TF` 1.6; `bind use kF` by default) [H5b]; `\n` is
the two-character line break of complex mode (`TF` 1.6, 4.3). The player's copy of `inventory_box_use` is never
used for the hint (boxes carry the same key as their own hint text, 1.4) [H1b].

### 1.2 When the hint is chosen

The player's update, right after it fills the crosshair record (`UA` 2.2), chooses the player's hint, only in
single play (in other game types the hint is left as it was) [H2, H2b]. Earlier in the same update, while the
player is in a holder (`UA` 8), or while either of two of the player object's own state flags is clear
(**unestablished** which; **believed** both set throughout play), the hint is set to none and the update stops
there, so the crosshair record is not refreshed either [H2g].

**Disputed in review:** "in other game types the hint is left as it was" holds only while the crosshair record is
filled. The game-type test comes after the record is filled; when the record is cleared (1.3), the hint is set to
none in every game type.

### 1.3 The choice

With the crosshair record (`UA` 2.2, of the crosshair's pick of `UA` 2.1) cleared (no object, distance at least `take_dist`, a monster controlling the player)
the hint is none [H2]. Otherwise the first rule that applies, for the record's object [H2c, H2d, H2e, H2f]:

1. **Hint text.** The object's usable part has a hint text (not none; an empty text counts): the hint is the
   lookup of that text.
2. **Living character.** The object is a character with health above 0: `character_use`.
3. **Dead.** The object is a living kind with health 0 or less: `dead_character_use_or_drag` when the object's
   model name is a key of `[ph_capture_visuals]` (the list and the matching of `UA` 7.1: 147 names, e.g.
   `actors\bandit\stalker_bandit_1`, no trader model [H2i]), otherwise `dead_character_use`.
4. **Holder.** The object is a holder (a car, a mounted or a stationary gun): `car_character_use`.
5. **Item.** The inventory's target item (`UA` 2.2, 9.1) exists and passes the **take test**: `inventory_item_use`.
6. Otherwise none.

Rules 2 to 5 store the key itself, not its text; the line looks it up when it shows it (2.3). The take test is the
item's take flag (its section's `can_take`, `UA` 9.1), and for the 13 artefact kinds also one state of the
artefact that must be clear (**unestablished** which) [H3]; it is the test the default usefulness test of `UA` 9.1
uses, without the other item kinds' additions [H3b]. The usable flag (`UA` 10) is not read (`UA` 10.2). Neither
`cl_cod_pickup_mode` nor `g_autopickup` is read (`MS` 5.1 names the only reader of the first; `MS` 2).

### 1.4 Where objects' hint texts come from

Every object's usable part starts with no hint text (none) [H4b]. Then:

- **Scripts** set it with `set_tip_text(text)`, which stores `text` as given, the empty text included, and clear it
  with `set_tip_text_default()`, which stores none (`P` 8) [H6, H6d]. An empty hint text therefore makes rule 1
  apply with an empty hint: the line is shown with no visible text (2.2), and rules 2 to 5 cannot apply.
- **Inventory boxes**, when they come online (spawn into the level), set their hint text to the key
  `inventory_box_use` [H4]: rule 1 then gives `Обыскать ящик (…)`, whatever the box's usable flag.

What the shipped scripts store is the scripts' behaviour; the parts that decide the cases this specification
names (data, [H6c, H10, H10c, H10d]):

- Stalkers (their binder, `xr_motivator`) and every object bound with `bind_monster` (monsters and the trader,
  `[m_trader]` `script_binding = bind_monster.bind`), on every update of their binder (`OB` 5.2): alive with talk
  enabled (`UA` 6.2): `"character_use"`; alive with talk disabled: `""`; dead: `set_tip_text_default()`.
  `bind_monster` does this before passing the update to its active scheme.
- `mob_trader` (the trader's scheme), when a section starts: the section's `tip_text`, `character_use` when absent,
  `""` when given empty.
- `ph_door`: with `show_tips` (default true) a closed door (`closed`, default true) gets `tip_door_locked` when
  `locked`, else `tip_door_open`; an opened door gets `tip_door_close`; leaving the scheme sets `""`. The keys can
  be changed with `tip_open` and `tip_close` (the locked text is also read from `tip_open`).
- `ph_car`: `""` when its scheme starts, later `tip_car_use` or `tip_car_locked`. `ph_code` (default
  `st_codelock`), `ph_idle`, `ph_button`, `ph_gate` and `explode_zone` set texts of their own.

Texts of those keys [H5]: `tip_door_locked` `Дверь заперта`; `tip_door_open` `Нажмите ($$ACTION_USE$$), чтобы
открыть дверь`; `tip_door_close` `Нажмите ($$ACTION_USE$$), чтобы закрыть дверь`; `tip_car_use` `Открыть дверь
($$ACTION_USE$$)`; `tip_car_locked` `Заблокировано`; `st_codelock` `Кодовый замок ($$ACTION_USE$$)`.

### 1.5 By kind of object (with the shipped scripts)

| Under the crosshair, within 2 m | Player's hint | Line shows |
|---|---|---|
| nothing, or farther than 2 m | none | hidden |
| a stalker or the trader, alive, talk enabled | `character_use` (rule 1) | `Говорить (<use key>)` |
| a stalker or the trader, alive, talk disabled | empty (rule 1) | shown, no visible text |
| a living character no script gives a hint text | `character_use` (rule 2) | `Говорить (<use key>)` |
| a dead character, model in `[ph_capture_visuals]` | `dead_character_use_or_drag` (rule 3) | two lines (2.4) |
| a dead character, model not listed (the trader) | `dead_character_use` (rule 3) | `Обыскать труп (<use key>)` |
| an inventory box | `inventory_box_use` (rule 1) | `Обыскать ящик (<use key>)` |
| a door with `ph_door` | its scheme's key (rule 1) | e.g. `Дверь заперта` |
| a physics object with no hint text | none (no rule) | hidden |
| a car with `ph_car` | its scheme's text (rule 1) | e.g. empty, `Открыть дверь (<use key>)` |
| a holder with no hint text | `car_character_use` (rule 4) | `Открыть дверь (<use key>)` |
| an item that may be taken, no hint text | `inventory_item_use` (rule 5) | `Поднять предмет (<use key>)` |
| an item whose take flag is off (e.g. `[explosive_barrel]`, `UA` 9.1) | none | hidden |

`<use key>` is the key name of `TF` 1.6. The 2 m is the record's `take_dist` (`UA` 2.2).

## 2. The hint line

### 2.1 The element

The line is the static element `quick_info` of `config\ui\maingame.xml` (the same in `maingame_16.xml`), a child
of the in-level HUD window, set up from the layout as every static is (`UL` 5, 7; `CA` 4.1) [H8]:

    <quick_info x="1" y="700" width="1022" height="50" complex_mode="1" light_anim="ui_btn_hint"
                la_cyclic="0" la_texture="0" la_text="1" la_alpha="1">
        <text align="c" font="graffiti22" r="225" g="225" b="250" a="255" />
    </quick_info>

So: a rectangle (1, 700, 1022 × 50) in the 1024 × 768 UI space (`UL` 3), text in `graffiti22`, centred
horizontally, colour (225, 225, 250), complex mode (line breaks and wrapping, `TF` 4.3; placement `UL` 7.3, 7.4),
no picture, and the colour animation `ui_btn_hint` on the text's alpha only, played once.

`ui_btn_hint` (`lanims.xr`): 30 frames at 30 fps (length 1 s), keys at frame 0 (0x00000000) and frame 29
(0xFF000000) [H8b]: with `la_alpha` the text keeps (225, 225, 250) and its alpha rises from 0 to 255 over the
first second after the animation's start (`CA` 3, 4.3), then holds the last value applied (`CA` 4.3).

### 2.2 Each frame

As the last step of drawing the HUD window, after its shown children (the line among them) have been drawn, and
only while the HUD has the player, the line is updated from the player's hint [H7, H7b]:

1. The line is shown when the player's hint is not none, hidden when it is none (an empty hint shows the line
   with no visible text).
2. If the hint is not none and differs from the line's current text (compared ignoring letter case), the line's
   text is set to the lookup of the hint [H7b, H7c].
3. If the record's object is not the object remembered from the last time this step ran, the line's text is set
   again to the lookup of the hint (empty when the hint is none), the line's colour animation is restarted (2.3),
   and the record's object is remembered [H7d].

The remembered object is one value for the whole run of the game: none when the game starts, never reset by a new
HUD, a level change or a load [H7e].

Because the update follows the drawing, a change shows from the next frame's drawing (**believed**: an element's
text and visibility are taken when it is drawn).

### 2.3 Text and fade

- **Text.** The line shows the lookup of the hint (`TF` 1.4). For rules 2 to 5 that is the key's text; for rule 1
  the hint is already the lookup of the object's hint text, so the line shows the lookup of that lookup (the same
  text, unless the first text is itself a key). The `$$ACTION_USE$$` substitution is the table's (`TF` 1.6). A key
  with no entry is shown as itself and may write the "has no entry" log line (`WF` 6). Rewriting an unchanged text
  (step 2 runs every frame for rules 2 to 5, whose key never equals its text) changes nothing visible.
- **Fade.** A restart sets the colour start by the same formula as an element's first update (`CA` 4.2, from the
  interface clock's *ms* and the element's *delay*, here 0) [H7d, H7f]. So each time the crosshair record moves to a different object, or from or
  to none, the hint text fades in again over 1 s, even when the hint stays the same (e.g. from one stalker to
  another). The colour start is otherwise fixed by the line's first update (`CA` 4.2), and the animation advances
  only on the HUD's updates while the line is shown (`CA` 4.2).

### 2.4 Example

A dead stalker in a listed model: hint `dead_character_use_or_drag`; text `Обыскать труп (<use key>) \n Тащить
труп (SHIFT+<use key>)`, drawn in complex mode as two centred lines, `Обыскать труп (<use key>)` and `Тащить труп
(SHIFT+<use key>)` (how complex mode treats the spaces beside the break: `TF` 4.3).

### 2.5 Settings

`hud_info` (HUD flag 8) does not affect the line: the only part that tests that flag is the target information
near the crosshair (`hud-scope` spec 14) [H8c]. `hud_draw` and the other switches are spec 2's.

## 3. The pick-up icon

### 3.1 The element

`pick_up_item` of `maingame.xml` (the same in `_16`): `<pick_up_item x="437" y="400" width="150" height="100"/>`,
a static child of the HUD window [H8, H9]. When the HUD is made, the program gives it the picture
`ui\ui_icon_equipment` with shader `hud\default` (`IS` 4.2), turns its clipping on (as `clipper` 1, `UL` 4.3), and
remembers its layout rectangle (437, 400, 150 × 100) as the **icon area** (an element with a non-zero alignment
would have it shifted by half its size; this one has the default alignment) [H9, H9b].

### 3.2 Each update

On every update of the HUD window that has the player, just before its children are updated [H9e, H9d]:

- If there is a pick-up target (`MS` 5.1) and the object the player controls is of class `O_ACTOR` (`MS` 5.3):
  1. The item's icon (`IS` 4.2): from its section's `inv_grid_x`, `inv_grid_y`, `inv_grid_width`,
     `inv_grid_height` (read as unsigned integers), the texture rectangle (*gx* · 50, *gy* · 50) to (*gx* · 50 +
     *iw*, *gy* · 50 + *ih*), *iw* = `inv_grid_width` · 50, *ih* = `inv_grid_height` · 50 pixels; `stretch` on
     (`UL` 5.3) [H9c, H9d].
  2. Scale *s* = min(min(1, *W* / *iw*), min(1, *H* / *ih*)), *W* × *H* the icon area's size (150 × 100), in
     single precision [H9d].
  3. The element's size becomes *iw* · *s* × *ih* · *s* and its position (*X* + (*W* − *iw* · *s*) × 0.5, *Y* +
     (*H* − *ih* · *s*) × 0.5), (*X*, *Y*) the icon area's corner (437, 400): the icon is centred in the icon area
     and never larger than it [H9c, H9d].
  4. The picture's colour is 0xC0FFFFFF (white, alpha 192) and the element is shown. <!-- spec-lint: constant -->
- Otherwise the element is hidden.

While the HUD has no player the icon is left as it was [H9e]. Examples [H11]: `medkit` (6, 14, 2 × 1): pixels
(300, 700)–(400, 750), *s* = 1, drawn 100 × 50 at (462, 425); `wpn_ak74` (5, 5, 5 × 2): pixels (250, 250)–(500,
350), *s* = 0.6, drawn 150 × 60 at (437, 420).

### 3.3 Settings

- `cl_cod_pickup_mode`: with 1 the pick-up target follows `MS` 5.1 each frame. With 0 the target is no longer
  written by anything during play (its only writers are the step of `MS` 5.1, the HUD's start and the HUD's
  creation) [H9f], so the icon keeps showing the target set before the switch, if any. This corrects `MS` 5.4,
  which says the icon is not shown in mode 0: that holds only when no target was set before the switch.
- `g_autopickup` has no effect in single play (`MS` 2), here included.

## 4. Worked examples

### 4.1 The trader

The trader (`escape_trader`) is bound with `bind_monster` [H10c]. Within 2 m, alive: in the waiting sections of
his start chain (talk off, `docs/behavior/trader-and-crows-scope.md` worked example) his hint text is `""`, so the
line is shown with no visible text and no built-in text appears; once talk is enabled it is `character_use` and
the line shows `Говорить (<use key>)`, fading in when the crosshair reaches him. After a section of `mob_trader`
starts, the text that section sets holds until the binder's next update replaces it by the talk rule (1.4)
(**believed**: sections start during the scheme's update event, which `bind_monster` passes on after its own rule).
Dead: his hint text is none (binder), he is a living kind with health 0, his model is not listed, so the line shows
`Обыскать труп (<use key>)`.

### 4.2 The trader's door

The door runs `ph_door` from `config\scripts\esc_trader_door.ltx` [H10b]: in `ph_door@locked` it is closed and
locked, so its hint text is `tip_door_locked` and within 2 m the line shows `Дверь заперта`; in `ph_door@closed`
`Нажмите (<use key>), чтобы открыть дверь`; in `ph_door@open` `Нажмите (<use key>), чтобы закрыть дверь`. The
door's cleared usable flag (`UA` 11.1) does not change the line.

### 4.3 A medkit on the floor

Looked at within 2 m: the record's item is the target item; with no hint text and its take flag on, the hint is
`inventory_item_use`: `Поднять предмет (<use key>)`. Independently, with `cl_cod_pickup_mode 1`, if `MS` 5.1 picks
it as the pick-up target, its icon is drawn 100 × 50 at (462, 425) (3.2).

## 5. Unestablished and believed

- Which two state flags of the player stop the hint choice (believed always set in play) (1.2).
- The artefact state the take test also needs (1.3).
- That a change shows from the next frame's drawing (2.2).
- The order of the player's update and the HUD's draw within a frame (believed: the update first, as the frame's
  object updates precede drawing; not checked) (2.2).
- That the trader's scheme starts sections during its update event (believed) (4.1).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`
  (merged view); binaries `bin\xrGame.dll`, `bin\xrCore.dll`.
- Data examined: `config\ui\maingame.xml`, `maingame_16.xml`; `config\localization.ltx` and the string tables of
  `config\text\rus\`; `config\default_controls.ltx`; `config\system.ltx` (`[ph_capture_visuals]`);
  `config\creatures\m_person.ltx`; `config\misc\items.ltx`, `config\weapons\w_ak74.ltx`;
  `config\scripts\esc_trader_door.ltx`; `lanims.xr` (`ui_btn_hint`); the shipped scripts `xr_motivator`,
  `bind_stalker`, `bind_monster`, `mob_trader`, `ph_door`, `ph_car`, `ph_code`, `ph_idle`, `ph_button`,
  `ph_gate`, `explode_zone`.
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and instruction listings with Ghidra;
  vtables from its run-time type information): the player's loading of its hint keys, the player's update (the
  choice of the hint after the crosshair record), the object kinds' type queries and the item take tests, the
  inventory box's coming online, the usable part's hint-text getter and setters, the string-table lookup, the HUD
  window's set-up, update and drawing, the hint line's update, the pick-up icon's update, the HUD flag tests;
  of `bin\xrCore.dll`: the shared-string container's handling of empty and null texts. Constants read from the
  binary. No execution or debugging; the game was not run.
- The checks in `checks/hud-pickup-hint/` (`checks.py`) reproduce every fact; `out/dock.c` is the headless
  decompilation of the xrCore string container's entry (`DecompAt.java` on the `xrcore` Ghidra project).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The table of 1.5 has no rows for monsters. They are bound with `bind_monster` (the `script_binding` of every
  monster section in `config\creatures\`), so alive they get `character_use` or `""` by their talk state (whether a
  monster's talk can be enabled is not established here), and dead they get none; `[ph_capture_visuals]` lists only
  `actors\…` models, so a dead monster gives `dead_character_use`.
- `P` 8 says `set_tip_text_default()` empties the hint text; this specification establishes that it stores none,
  which differs from an empty text here (1.3 rule 1, 2.2). `P` 8 needs the same correction.
