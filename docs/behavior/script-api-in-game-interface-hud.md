# Script API G4: in-game interface (HUD)

What the in-game interface part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition), as scripts see it: the global `get_hud` and the object it returns (class `CUIGameCustom`),
its **custom statics** (class `SDrawStaticStruct`) and **custom messages**; the windows drawn over the level
(`AddDialogToRender`, `RemoveDialogToRender` and their `level` forms); the menus opened over the level (class
`CDialogHolder`, `level.start_stop_menu`, `level.main_input_receiver`); the HUD indicators
(`level.show_indicators`, `level.hide_indicators`); the player-input switch (`level.disable_input`,
`level.enable_input`); and the script-derivable game interface `UIGame_custom_script` with the map-entity
classes `ZoneMapEntities` and `SZoneMapEntityData`, including the overrides of the scoping spec's 4.5 row G4.
This is spec G4 of the approved scoping specification `docs/behavior/script-api-scope.md` (section 6, order 13);
its scope rule and its review gaps apply: this spec owns every name of these classes as the game library
registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type, the
  250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (deriving from engine
  classes, how an engine call reaches an override, 5.1 – 5.3; `Class.name` and base-class calls, 4.4 and 6;
  reading a missing member gives nil, 4.1); spec E, `docs/behavior/script-api-value-types-and-math.md` (a
  `number` parameter accepts only a Lua number, 2.2; the class `vector`; calls that match no form, 2.7);
  spec B, `docs/behavior/script-api-engine-services.md` (cited as **B**: the device's global timer and
  `time_global()`, 3.3); spec O, `docs/behavior/script-api-object-event-callbacks.md` (6.5: when the level
  removes all of its objects); spec N, `docs/behavior/script-api-level.md` (the `level` namespace, which gives
  this spec 8 of its functions).
- `docs/behavior/autosave-timing.md` (cited as **AT**): the **player-input switch** and what the level ignores
  while it is on (5.1), its effect on F6 and F7 (5.2), and the shipped scripts that use it (5.3).
- `docs/behavior/console-key-bindings.md` (cited as **KB**): how a key press reaches the in-game interface and
  the open windows (5, rule 3).
- `docs/behavior/hud-scope.md` (cited as **HS**): the layout file `ui_custom_msgs.xml` and its statics (1.3),
  the parts of the HUD and their owners (section 3); `docs/formats/ui-layout.md` (how a static is built from
  a layout file element); `docs/formats/text-and-fonts.md` (fonts, the string table).
- Specs G1 (windows, dialog windows, `GetHolder`), G2 (statics and their text members such as `SetTextST`), G3
  (fonts, `GetFont…`, `GetARGB`) and Z (multiplayer game classes, `createGameUI`, `GetMapEntities`) own the
  classes this spec's members take or return; they are cited by letter.

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [G4-1].
  **The listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is
  established from the game library's registration code.
- A call is written as scripts write it: `hud:AddCustomStatic("main_task", true)`. A **number** is the
  runtime's double, a **text** a Lua string, a **boolean** a Lua boolean, **nil** Lua's nil.
- **The level interface** is the in-game interface object of the running level: it holds the **game
  interface** (2), the **menu stack** (section 5), the list of **windows to draw** (section 4) and the
  **indicators switch** (section 6). **The game interface** is the object of class `CUIGameCustom` that
  `get_hud()` returns.
- **Global seconds** is the device's global timer (B 3.3) in seconds, as a 32-bit float, set at the start of
  every frame together with `time_global()` (which is the same timer in milliseconds); in the constant-step
  mode of B 3.3 it grows by 0.02 per frame [G4-12].
- **Fatal** means the program reports the failure and ends. "Reads through no object" means the code reads or
  calls through a missing part without a test (**believed**: the program crashes).
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (from
  game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was
  run.

**Checks.** Every claim carries a tag [G4-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-hud/run_all.py` (`python3 -I run_all.py`): it reads the game library's bytes, the evidence of
read-only headless static analysis (in `bin/` beside it, and the evidence of spec B's checks it cites), the
listing, the scoping spec's scan, the shipped scripts and `config\ui\ui_custom_msgs.xml`. All checks pass
(`ALL PASS 24`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 34 listed items [G4-2]:

| Owner | Items |
|---|---|
| global | `get_hud` (section 2) |
| `CUIGameCustom` | the class; `AddCustomStatic`, `GetCustomStatic`, `RemoveCustomStatic` (section 3); `AddCustomMessage` (2 forms), `CustomMessageOut`, `RemoveCustomMessage` (section 7); `AddDialogToRender`, `RemoveDialogToRender` (section 4) |
| `SDrawStaticStruct` | the class; the property `m_endTime` and the function `wnd` (3.2) |
| `CDialogHolder` | the class; `AddDialogToRender`, `RemoveDialogToRender` (section 4); `start_stop_menu`, `MainInputReceiver` (section 5) |
| `UIGame_custom_script` | the class; `Init`, `SetClGame` (section 8) |
| `ZoneMapEntities` | the class; `push_back` (8.4) |
| `SZoneMapEntityData` | the class; the properties `pos`, `color` (8.4) |
| `level` | `add_dialog_to_render`, `remove_dialog_to_render` (section 4); `start_stop_menu`, `main_input_receiver` (section 5); `show_indicators`, `hide_indicators` (section 6); `disable_input`, `enable_input` (section 9) |

and the entry points of the scoping spec's 4.5 row G4, the overrides of `UIGame_custom_script` (8.2). The listing
gives `UIGame_custom_script` the base `CUIGameCustom` and repeats its members; the game library registers
exactly that base [G4-3]. The listing's forms: `AddCustomStatic(string, boolean)`,
`AddCustomMessage(string, number, number, number, CGameFont*, number, number)` and the same with one more
`number`, `start_stop_menu(CUIDialogWnd*, boolean)` [G4-3].

The scoping spec assigns this spec 2 candidate strings: `main_task` and `mp_friend_radio_location`. Neither is a
registration: the first is the name of a custom static that the single-player game interface adds itself (3.5),
the second is used by a multiplayer game routine that registers nothing (static) [G4-2]. The scoping spec assigns
it one name the binary lacks, `UIGame_custom_script.IR_OnKeyboardPress` (8.3) [G4-2, G4-21].

Of these names the loadable scripts use 24, in 20 (31) loadable scripts (scoping spec section 6); the upper count,
31 loadable scripts and 54 of all scripts, includes scripts that use the names `Init`, `start_stop_menu`, `pos`
only through other classes that also declare them [G4-2].

### 1.2 Registration (static)

All the classes and functions of 1.1 are registered by the game library with the members named there, and
nothing more [G4-3]:

- `CUIGameCustom`: no constructor (scripts cannot make one); the nine functions of 1.1.
- `SDrawStaticStruct`: no constructor; `m_endTime` readable and writable; `wnd()`.
- `CDialogHolder`: no constructor; the four functions of 1.1.
- `UIGame_custom_script`: one constructor without arguments, base `CUIGameCustom`; `Init` and `SetClGame`
  registered as overridable methods (spec A 5.1 lists `UIGame_custom_script` among the classes with defaults).
- `ZoneMapEntities`: no constructor; `push_back`.
- `SZoneMapEntityData`: one constructor without arguments; `pos` and `color` readable and writable.
- `get_hud` in the global namespace; the eight `level` functions in the namespace of spec N.

## 2. `get_hud()` (static)

`get_hud()` takes no argument and returns the level interface's game interface as a `CUIGameCustom`, or nil when
the level interface has none [G4-4]. It reads the running level's HUD and its level interface without a test: with
no level, or before the HUD has made its level interface, it reads through no object [G4-4].

When the level's HUD is loaded, it makes the level interface if there is none (with the indicators switch on and the
crosshair switch on, section 6, and no game interface yet), then asks the running game for its game interface and
keeps what the game returns; when the HUD is unloaded, the game interface is destroyed and the game is asked for a
new one [G4-5]. A missing result there fails a debug test whose consequence is the engine's debug report
(**believed** fatal, as other such tests in the approved specs). Which object the game returns is the game type's:
the single-player game returns its own game interface (a class derived from `CUIGameCustom` that scripts cannot
name); a multiplayer script game returns what its script override returns (Z; 8.1) (**believed**: from
`game_last_cl.script`, whose `createGameUI` returns its `UIGame_custom_script` object, [G4-24]).

So, in a single-player game, `get_hud()` returns the same object for as long as the HUD stays loaded.

The game interface, when made, prepares [G4-6]:

- an empty list of **custom statics** (section 3);
- one window holding the **custom messages** (section 7);
- the layout file `ui\ui_custom_msgs.xml` of the game configuration folder, read once, from which every custom
  static is built (HS 1.3).

## 3. Custom statics

A **custom static** is an entry of the game interface's list: a **name** (a text), a **window** (a static built
from the element of `ui_custom_msgs.xml` with that name) and an **end time** in global seconds, where a negative
end time means "no end" [G4-7, G4-9]. Several entries may have the same name (3.1).

### 3.1 `AddCustomStatic(name, single_instance)` (static)

1. If `single_instance` is true and the list has an entry with this name (the first such entry, in list order), that
   entry is returned and nothing else happens [G4-7].
2. Otherwise a new entry is appended to the list with end time −1, its window is made, given the name, and built
   from the element `name` (its first occurrence, index 0) of `ui_custom_msgs.xml` as a static (ui-layout) [G4-7].
   An element that does not exist fails the debug test `XML node not found` of the static builder (its
   consequence: **believed** fatal, as in 2) [G4-7].
3. The element's attribute `ttl` is read as a number, default −1. If it is greater than 0, the end time becomes the
   current global seconds plus `ttl` [G4-7].
4. The new entry is returned (an `SDrawStaticStruct`) [G4-7].

So with `single_instance` false every call adds a further entry with the same name; `GetCustomStatic` and
`RemoveCustomStatic` then find only the first.

In `ui_custom_msgs.xml`, `ttl` is given only for `gun_jammed`, `cant_walk`, `cant_walk_weight` (3) and
`max_ping_warn_1` … `_4` (8); the statics the shipped scripts add (`main_task`, `cs_psy_danger`,
`cs_radiation_danger`, `hud_timer`, `hud_timer_text`) have none, so they have no end until a script sets one
(measured) [G4-23].

### 3.2 `SDrawStaticStruct` (static)

- `wnd()` returns the entry's window (a static of spec G2; scripts set its text with members such as
  `SetTextST`, G2) [G4-9].
- `m_endTime` reads and writes the entry's end time, a 32-bit float in global seconds (writing stores the number
  as a 32-bit float) [G4-9]. Writing a negative value makes the entry endless; writing a value not later than the
  current global seconds removes it at the next update (3.4).

A returned `SDrawStaticStruct` refers to the entry in the list; adding entries can move the list's storage
(**believed**: the list is a growable array), so a script should not keep one across later `AddCustomStatic`
calls. None of the shipped scripts keeps one: they call `GetCustomStatic` again each time [G4-24].

### 3.3 `GetCustomStatic(name)` and `RemoveCustomStatic(name)` (static)

- `GetCustomStatic(name)` returns the first entry with this name, or nil [G4-8].
- `RemoveCustomStatic(name)` destroys the first entry's window and removes the entry; with no such entry it does
  nothing [G4-8].

Names are compared as texts (exact, case-sensitive) [G4-8].

### 3.4 Update and drawing (static)

Every frame, in its update the game interface, in list order [G4-10]:

1. for each entry: if its end time is negative or later than the current global seconds, its window is updated;
   otherwise its window is destroyed;
2. every entry without a window is removed from the list.

Then, if the level has removed all of its objects since the last update (O 6.5), every entry is destroyed and the
list emptied [G4-13].

When the game interface is drawn, it draws its custom-messages window first (section 7), then the window of every
entry, in list order [G4-10]. The level interface updates and draws the game interface only while the indicators
switch is on (section 6) [G4-11]. So hidden indicators hide every custom static, and the end times keep running
meanwhile (global seconds do not stop; they stop only while the game is paused, B 3.3).

### 3.5 Statics the engine adds itself (static)

The single-player game interface adds the static `main_task` itself, single instance, the same way as 3.1, when it
handles a press of the action `scores` (KB 5, rule 3: acted on but not taken) in an unpaused game with a living
player character, and sets its text to the active task's title or to the string `st_no_active_task`; it also
removes `main_task` in one further case of its own (which: **unestablished**) [G4-14]. What these show is the HUD message spec's
(HS 3, spec 8); here it matters only that a script's `main_task` entry and the engine's are the same entry. Other
statics of `ui_custom_msgs.xml` named in the game library (HS 1.3) are added by engine code outside this spec.

## 4. Windows drawn over the level (static)

The level interface keeps a list of **windows to draw**, each with an **active** mark [G4-16]. The six members

- `hud:AddDialogToRender(wnd)`, `holder:AddDialogToRender(wnd)`, `level.add_dialog_to_render(wnd)`
- `hud:RemoveDialogToRender(wnd)`, `holder:RemoveDialogToRender(wnd)`, `level.remove_dialog_to_render(wnd)`

all act on this one list: the `CUIGameCustom` and `level` forms ignore their object and use the level interface of
the running level (reading through no object without one, as in 2); the `CDialogHolder` forms act on the holder
they are called on, which is the level interface when the holder is that of a window opened over the level (5.4)
[G4-16]. The listing types the argument as `CUIWindow*` for the methods and `CUIDialogWnd*` for the `level` forms
[G4-3]; so the `level` forms take only dialog windows (G1), the methods any window (A, argument matching).

- **Add**: if the window is in the list and active, nothing happens. Otherwise an entry (window, active) is
  appended — also when the window is already in the list with an inactive mark, so a window may be in the list
  twice (**believed** harmless: see removal) — and the window is shown (G1's `Show(true)`, which also enables it)
  [G4-16]. **Disputed in review:** only the window's first entry in the list is tested: if that entry is active
  nothing happens, otherwise (also when a later entry for the window is active) an entry is appended.
- **Remove**: if the window is in the list (the first entry for it), the window is shown false and disabled, and
  that entry is marked inactive; otherwise nothing happens [G4-16].
- Inactive entries are dropped in the level interface's scheduled update (the list is ordered with the active
  entries first, and inactive entries are cut from the end) [G4-16]; how often that update runs is the engine's
  scheduler's (**unestablished** here).
- Every frame, each active entry whose window is enabled is updated; when the level interface is drawn, each
  active entry whose window is shown is drawn, in list order, after the HUD's other parts (6.2) [G4-16, G4-11].

The list keeps the window object without owning it: a script must keep its own reference while the window is in
the list (**believed**, from spec A's handling of script objects; the shipped multiplayer script keeps its windows in
fields of its game object) [G4-24].

## 5. Menus over the level (static)

### 5.1 The menu stack

The level interface keeps a **menu stack** of dialog windows (G1's `CUIDialogWnd`), each entry with two saved
switches (5.2) [G4-17, G4-18]. The topmost window is the one an open in-game window means in KB 5 rule 3 and AT 5.2.

- `level.main_input_receiver()` and `holder:MainInputReceiver()` return the topmost window of the stack, or nil
  when it is empty [G4-17].
- `level.start_stop_menu(wnd, hide)` and `holder:start_stop_menu(wnd, hide)`: if `wnd` is shown, it is **stopped**
  (5.3), otherwise it is **started** (5.2) with `hide`; `hide` matters only when starting [G4-17]. The `level` form
  uses the level interface of the running level (reading through no object without one); the holder form acts on
  its holder.

### 5.2 Starting a window (static)

Starting `wnd` with `hide` [G4-18]:

1. Starting a window that is already shown fails a debug test (**believed** fatal as in 2).
2. `wnd` is added to the windows to draw (section 4, add).
3. If `wnd` is not the topmost window of the stack, it is pushed onto the stack.
4. For the level interface (the main menu is also a holder, see 5.4): the new stack entry saves the current
   crosshair switch and indicators switch (section 6); then, if `hide` is true, both are turned off.
5. `wnd`'s holder becomes this holder, and `wnd` is shown as a menu: it becomes shown and enabled and its child
   windows are told so (G1 owns what a window does then; for script windows, G1 and A).
6. If `wnd` wants the cursor (a property of the dialog window, true unless the window's class says otherwise; G1),
   the interface cursor is turned on.
7. If the player character exists: if `wnd` wants the player stopped (likewise true by default; G1), the
   character's movement requests are cleared; in any case the character gets the actions `wpn_zoom` (33) and
   `wpn_fire` (32, the listing's `key_bindings` values) through the same input entry, **believed** a release (so a
   zooming or firing character stops when a menu opens; which of release or hold that entry is: **unestablished**,
   as in AT 5.1) [G4-18].

### 5.3 Stopping a window (static)

Stopping `wnd` [G4-18]:

1. Stopping a window that is not shown fails a debug test (**believed** fatal).
2. If `wnd` is the topmost window: the crosshair and indicators switches are restored from its stack entry (level
   interface only), it is removed from the windows to draw (section 4, remove), and it is popped off the stack.
3. Otherwise it is removed from the windows to draw and from its place in the stack, its saved switches being
   handed to the entry above it (so restoring follows the window that is closed last) (**believed**: from the code
   shape) [G4-18].
4. `wnd` is hidden (no longer shown, disabled) and its holder cleared.
5. If the new topmost window wants the cursor, the cursor stays on; otherwise (also with an empty stack) it is
   turned off.

### 5.4 Holders

A `CDialogHolder` is what G1's `GetHolder()` returns for a window that has been started: the level interface for
windows opened over the level, or the main menu for windows the main menu opens (the main menu is a holder too; its
`start_stop_menu` additionally sets a mark of its own and then behaves as 5.1 — whether it saves and restores the
switches of 5.2 step 4 is **unestablished**; the main menu is G5's) [G4-17]. The shipped scripts use
`self:GetHolder():start_stop_menu(self, true)` to close their own windows (e.g. `ui_numpad.script`), and
`level.start_stop_menu(numpad, true)` to open the code panel (`ph_code.script`, HS 2) [G4-24].

## 6. The HUD indicators (static)

### 6.1 The two switches

- The **indicators switch** of the level interface: on when the level interface is made [G4-5].
- The **crosshair switch**: one switch of the HUD (not one of the console settings `hud_crosshair`,
  `hud_crosshair_dist`, `hud_info`, `hud_weapon`), on when the level interface is made [G4-5, G4-19].

`level.show_indicators()` turns both on [G4-19].

`level.hide_indicators()` first asks the game interface to close its windows: the single-player game interface stops
(5.3) the topmost window of the menu stack when it is its inventory, PDA, talk or body-search window (the windows of
the actions `inventory`, `active_jobs`/`map`/`contacts`, talk and body search; HS 2); other game interfaces,
`UIGame_custom_script` among them, do nothing here. Then it turns both switches off [G4-19]. Both read the running
level's HUD without a test (as in 2).

Starting and stopping menus also save, clear and restore them (5.2, 5.3).

### 6.2 What they gate

- While the indicators switch is off the level interface neither updates nor draws the game interface (custom
  statics and custom messages, 3.4, section 7) nor draws the HUD's main window (health, armour, weapon and the other
  parts HS section 3 owns), which is drawn only when also two further HUD conditions of HS spec 2 hold (**believed**: one of
  them is off while the player looks through a scope) [G4-11]. The windows to draw (section 4) are drawn either way; the
  HUD's message window is drawn either way except in single player while a condition of its own holds
  (**unestablished**) [G4-11].
- The crosshair is drawn only while the crosshair switch is on, together with the console setting `hud_crosshair`
  and a further HUD condition of HS spec 2 (**believed**: off while zoomed in) [G4-19].
- The custom-messages window itself is drawn only while the same two further HUD conditions hold [G4-11].

So `hide_indicators` hides the crosshair, the main HUD window, the custom statics and messages, and with them the
save messages `game_saved` and `autosave` (as `docs/formats/save-picture.md` records).

## 7. Custom messages (static)

The game interface's custom-messages window holds an ordered list of **messages**, each with an **id** (a text), a
position (x, y), a height number, a font, an alignment, a colour and a text [G4-15].

- `AddCustomMessage(id, x, y, height, font, alignment, colour)` appends a message with these values and an empty text;
  an id that is already in the list fails the debug test `message already defined !!!` (**believed** fatal) [G4-15].
  `font` is a `CGameFont` (G3, e.g. `GetFontDI()`), `alignment` one of `CGameFont`'s alignment constants (G3),
  `colour` a 32-bit colour (`GetARGB`, G3).
- The form with an eighth number does the same and then switches on the text effects of the message and sets the
  first effect's value to that number (**believed** a flashing period; what the effect does is G2's text-banner
  effect, **unestablished** here) [G4-15].
- `CustomMessageOut(id, text, colour)` sets the message's text to `text` translated through the string table (an id
  without an entry gives itself, with a log line, text-and-fonts) and its colour to `colour` [G4-15].
- `RemoveCustomMessage(id)` removes the message [G4-15].
- `CustomMessageOut` and `RemoveCustomMessage` with an id not in the list fail the debug test `cannot find msg`
  (**believed** fatal) [G4-15].
- Drawing: each message's text is written with its font, colour and alignment at the window's position plus (x, y),
  each coordinate multiplied by one of a pair of interface scale factors (which factors: **unestablished**;
  text-and-fonts) [G4-15].

Ids are compared as texts. The shipped scripts call `AddCustomMessage` only in `game_last_ui.script`, twice, with an
eighth argument `""`; a text does not match a `number` parameter (E 2.2) and the seven-argument form does not take eight
arguments, so these calls match neither form (what then happens: A, E 2.7) [G4-24].

## 8. `UIGame_custom_script` and the map entities

### 8.1 Making one (static)

`UIGame_custom_script()` (or a script class deriving from it, `class "x" (UIGame_custom_script)` and `super()`,
A) makes a game interface as in section 2 (its own custom statics, messages and layout file) that keeps the
script object (A 3.1) [G4-21]. It becomes the level's game interface only when a multiplayer script game returns it
from its `createGameUI` override (Z); a single-player game never uses it [G4-5].

### 8.2 Overrides (static)

The engine reaches two methods of a script class deriving from `UIGame_custom_script` through the lookup of A 5.2
[G4-21]:

| Method | Arguments | Engine default | When the engine calls it |
|---|---|---|---|
| `SetClGame(self, game)` | the level's client game (a `game_cl_GameState`, Z) | does nothing | when the HUD loads with a game interface already given (static: the one engine call found); **unestablished** whether a shipped path reaches it |
| `Init(self)` | none | does nothing | no engine call found (**unestablished**) |

Calling the methods from a script (`ui:SetClGame(g)`, `ui:Init()`) goes through the same lookup, so it reaches the
override; the base-class calls `UIGame_custom_script.SetClGame(self, g)` and `UIGame_custom_script.Init(self)`
(A 6) do nothing [G4-21]. No result is used. An error in an override is fatal (A 5.3).

No other method of the game interface is registered with a default (A 5.1), and the script class's key-handling
entries are those of `CUIGameCustom`, which take no key [G4-21].

### 8.3 `IR_OnKeyboardPress` (static)

The name `IR_OnKeyboardPress` does not occur in the game library [G4-21]. So the engine never calls a script
method of that name, and `UIGame_custom_script.IR_OnKeyboardPress` reads nil (A 4.4); calling it, as
`game_last_ui.script`'s method `IR_OnKeyboardPress` does, is Lua's error for calling a nil value (**believed**, A
4.3) [G4-24]. The game interface's own key handling takes no key for this class (KB 5 rule 3: the interface is
offered the press; a `UIGame_custom_script` takes none) [G4-21].

### 8.4 `ZoneMapEntities` and `SZoneMapEntityData` (static)

- `SZoneMapEntityData()` makes a record with `pos` = (0, 0, 0) and `color` = `0xff00ff00` (opaque green, as ARGB)
  <!-- spec-lint: constant --> [G4-22].
- `pos` reads a new `vector` holding a copy of the position and writes the components of the `vector` given (E);
  `color` reads and writes the colour as an unsigned 32-bit number [G4-22].
- `ZoneMapEntities` is a list of such records; `list:push_back(record)` appends a copy of the record (so changing the
  record afterwards does not change the list) [G4-22].

A `ZoneMapEntities` is only ever handed to scripts by the engine, as the argument of a multiplayer game's
`GetMapEntities` override (Z, scoping spec 4.3); what the engine does with the records (map marks) is Z's and the HUD
mini-map spec's [G4-24].

## 9. The player-input switch (static)

`level.disable_input()` turns AT 5.1's player-input switch on and `level.enable_input()` turns it off; neither
takes an argument or returns anything [G4-20]. The switch is a single process-wide flag, off when the program
starts, written by these two functions only and read by five input handlers of the level; everything it does is
AT 5.1 and 5.2 [G4-20]. Both functions work with or without a running level.

## 10. The shipped scripts (measured)

Worked examples, all loadable scripts [G4-24]:

- `news_manager.script` shows a task message: `hud:AddCustomStatic("main_task", true)`, sets its text with
  `GetCustomStatic("main_task"):wnd():SetTextST(...)`, and sets `m_endTime = time_global()/1000 + 5`: since
  `time_global()` is the same timer in milliseconds, this ends the static about 5 seconds later (3.2, 3.4).
- `bind_stalker.script`, `sr_psy_antenna.script`: endless single-instance statics `cs_radiation_danger`,
  `cs_psy_danger`, added when a danger starts (tested first with `GetCustomStatic`) and removed with
  `RemoveCustomStatic`. `bind_stalker.script` also calls `level.show_indicators()` when the player character is
  spawned.
- `sr_timer.script`: a single-instance static named by level logic (default `hud_timer`, HS 1.3) and
  `hud_timer_text`, keeping their windows to set the timer text, removed when the timer ends.
- `xr_effects.script`: `hide_indicators` with `disable_input` and `show_indicators` with `enable_input`, three
  times each (AT 5.3).
- `ph_code.script`, `ui_numpad.script`: open and close the code panel with `start_stop_menu(…, true)`.
- `game_last_ui.script`, `game_last_cl.script` (the multiplayer "last standing" game): a `UIGame_custom_script`
  class with `SetClGame` and `IR_OnKeyboardPress` methods (8.2, 8.3), windows shown with `AddDialogToRender`,
  custom messages (section 7), and map entities with `SZoneMapEntityData` and `push_back` (8.4).

## 11. Verification facts

- Items: 34 listed, 6 classes, 8 `level` functions, `get_hud`; 24 used names; candidates `main_task`,
  `mp_friend_radio_location` [G4-2].
- `ui_custom_msgs.xml`: `ttl` 3 on three statics and 8 on the four `max_ping_warn_…` statics [G4-23].
- Constants: default end time and default `ttl` −1; map-entity colour `0xff00ff00`
  <!-- spec-lint: constant -->; `wpn_fire` 32, `wpn_zoom` 33, `scores` 43, `inventory` 49 [G4-7, G4-14, G4-18,
  G4-19, G4-22].

## 12. Unestablished

- Whether the debug tests named in sections 2, 3, 5 and 7 end the game in the shipped build (believed fatal).
- How often the level interface's scheduled update runs, so when inactive windows to draw are dropped (section 4).
- Which of release or hold the actions of 5.2 step 7 are delivered as; whether the main menu saves and restores the
  switches when it starts a window (5.4).
- The text effect set by the eight-argument `AddCustomMessage`, and the scale factors of custom-message positions
  (section 7).
- When, if ever, the engine calls `Init` and `SetClGame` of a script game interface in a shipped game (8.2).
- The condition under which the HUD's message window is not drawn in single player (6.2).
- What a window does when it is shown as a menu, and which windows want the cursor or the player stopped (G1).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` as identified above;
  the scripts and `config\ui\ui_custom_msgs.xml` as the winning copies of the root archives.
- Analysis: static analysis of `xrGame.dll` with Ghidra in headless read-only mode (the registration code of the
  classes and functions of 1.1, the routines they register and the routines those call, the game library's
  run-time type information and virtual tables); reading of file bytes; reading of the shipped scripts and of the
  scoping spec's scan. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- 6.2: the HUD's main window is also drawn only while the level has a current controlled object; without one, only the message window and the windows to draw are drawn.
- 6.2 and 12: the single-player condition under which the HUD's message window is not drawn is establishable: it is skipped while the single-player game interface's PDA window (the window of the actions `active_jobs`, `map`, `contacts`) is shown.
- Section 7: what the `height` argument of `AddCustomMessage` does is neither stated nor marked unestablished.
- 3.5: when the active task has no title, the engine's `main_task` text is set to no text; this case is not stated.
