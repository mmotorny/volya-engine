# Script API D: engine entry points and the New Game call order

When and how the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl binaries (Russian edition) call functions of the game
scripts that they find by name, and in which order the engine calls into scripts from program start to the first
frame of a New Game. This is spec D of the approved scoping specification `docs/behavior/script-api-scope.md`
(section 6, order 4). It owns the entry points that section 4.5 assigns to D: `game_type_clsid_factory`,
`start_game_callback`, `can_sleep_callback`, `sleep_video_name_callback`, the 13 functions of section 4.2, the unread
`ui_type_clsid_factory`, and the order of the steps of section 5 across specs; and gap 3 of that document (functions
the engine might call by a bare global name). One further entry point, found here, belongs to the same startup step:
the `_initialize` call of the `[common] script` list (section 2.1).

Builds on (not repeated here):

- `docs/behavior/script-environment.md` (the runtime, the game state, `-nojit`), `docs/behavior/script-loading.md`
  (script names, namespaces, the loading occasions of its section 2.2, the silent failures of its 2.4, startup),
  `docs/behavior/script-usage.md` (cited as **usage**; its section 5.5: the engine's coroutines and the script lists
  they come from), `docs/behavior/offline-simulation.md` (cited as **offline**; its section 8.1: creation order at a
  New Game).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (script classes, overrides, the
  error handling of engine calls into scripts, its section 5.3).
- Spec B, `docs/behavior/script-api-engine-services.md` (the console, `log`, `editor()`, `app_ready()`, the game type of
  its section 4.5, precache phases of its 3.4).
- Spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md` (cited as **C**: class ids, the object factory,
  the **creation entry** of its section 6.1, the **resolution** of a name to a function or class of its section 3.2,
  the `class_registrators` call of its section 4, the script-state **rebuild** of its section 2.3).
- The specs that own what happens inside the steps ordered here: offline (its sections 5 and 6),
  `docs/behavior/loading-screen.md`, `docs/formats/sav.md` (section 10), `docs/behavior/world-coordinates.md`,
  `docs/behavior/view-projection.md`.

Conventions:

- Binaries, identified as in the script environment specification: `bin\xrGame.dll` (6,246,400 bytes, MD5
  `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the game library"; `bin\XR_3DA.exe` (1,636,424 bytes, MD5
  `ba3ad0127bff6619ed991a7c82d8b946`), "the engine executable". Game files are the winning copies of the root
  archives `gamedata.db0` … `gamedata.dbd`.
- **Startup** is the sequence of script-loading.md 2.2 and C 2.3 and 5.1: a new game state, the engine interface
  registered, `_G` loaded, the class registrators called, the `clsid` table built, the `[common] script` list
  processed (section 2.1 here).
- **Loading stage**: one step of the engine's queue of loading work (section 3.3). **Frame**: one run of the engine's
  per-frame work; the **first frame** of a game is the first frame after its last loading stage.
- Each fact is **measured** (from the game files or file bytes), **static** (from static analysis of the binaries,
  not observed at run time), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.
- Checks: `checks/script-api-engine-entry-points/run_all.py` in the dirty workspace (`python3 -I run_all.py`; it reads
  the game files and the cached output of read-only headless static analysis produced by `evidence.py` beside it).
  Each claim carries the tag of the check line that reproduces it, [EP-*n*]; the run prints `ALL PASS 64`.

## 1. Finding and calling a script function by name

### 1.1 Resolution (static)

Every entry point of this spec finds its function by the resolution of C 3.2 asked for a function (Lua type
function): the name is split at its last `.`; the namespace before it is loaded first (script-loading.md 2.2,
occasion 4) unless it is `_G`; the member is then searched among the namespace table's own entries, and counts only
if it is a function [EP-25, EP-51, EP-53, EP-57]. Consequences (believed, from C 3.2):

- A name `_G.f` is found only if the globals table itself holds `f`; reading it does not trigger automatic loading.
- A function that a namespace only reaches through its fallback to the globals is not found in that namespace.
- What a failed resolution does is each entry point's own rule (section 2).

### 1.2 The call (static)

The function is called in a protected call with the arguments each entry point gives. If the call fails, the
engine calls the binding library's error callback, which is the fatal `LUA error: %s` of A 5.3 and C 4, and if that
returned the program would end [EP-19, EP-24, EP-55]. So an error in any function of this spec ends the game (fatal,
as in the original).

### 1.3 Where the engine finds script functions by name (static)

The resolution of C 3.2 is used by exactly six routines of the game library: the two `register` forms of
`object_factory` (spec C), and four lookups that keep the found function for the engine to call [EP-57]. Every
use of these lookups takes its name from one of these sources [EP-58]:

| Name source | Entry point | Spec |
|---|---|---|
| `config\script.ltx [common] class_registrators` | the class registrators | C |
| `config\script.ltx [common] script` | `<item>_initialize` | D (2.1) |
| `config\script.ltx [common] game_type_clsid_factory` | the game-type function | D (2.2) |
| `config\alife.ltx [alife] start_game_callback` | the start callback | D (2.3) |
| 8 names written in the game library | the tutorial thresholds | D (2.4) |
| 5 names written in the game library | the weapon parameters | D (2.5) |
| key `script_binding` of an object's section | binding functions | L |
| dialog files (`init_func`, and the phrases' script functions), and names scripts pass to dialog members (believed) | dialog functions | R |
| task files (`function_complete`, `function_fail`, `function_call_complete`, `function_call_fail`), and names scripts pass to task objectives | task functions | P |
| sequence files: `function_on_start`, `function_on_stop` | tutorial and movie functions | P |
| a name a script hands to a camera effector (believed) | camera-effector callback | U |

The game library writes no other script function name: of all `namespace.function` names of the scoping spec 4.2,
each of the 13 occurs in `xrGame.dll` exactly once and in no other file under `bin\` [EP-13]. The globals table is
reached by the game library in only three other places: installing the automatic-loading metatable
(script-loading.md 2.2, occasion 3), the test whether a namespace is loaded, and making a handle of the globals table
for the resolution [EP-59].

## 2. The entry points

### 2.1 `[common] script` and `<item>_initialize` (static; data measured)

The last step of every startup, after the class registrators and the `clsid` table (C 5.1) [EP-15]: if
`$game_config$script.ltx` has a section `[common]` with a key `script`, its value is a comma-separated list (LTX list
items). For each item *X* in order [EP-14]:

1. Namespace *X* is loaded (script-loading.md 2.2, occasion 2).
2. The name *X* followed by `_initialize` (no separator: for `_G`, `_G_initialize`) is searched among the globals
   table's own entries (raw iteration, not the automatic loading) for a value of type function.
3. Only if one is there, it is resolved (section 1.1; the name has no `.`, so it is a global) and called with no
   arguments, its results ignored. A failed resolution at this point is a fatal failed assertion.

In this edition the list is `_G` [EP-1], and no script defines a global function `_G_initialize` (none even contains
the name) [EP-8], so step 3 never runs: startup calls no function here.

### 2.2 The game-type function `game_type_clsid_factory` (static; data measured)

Asked for the class id of a game type, the game library does the following, except on a dedicated server [EP-22]:

1. Reads `[common] game_type_clsid_factory` of `$game_config$script.ltx` (a missing key is ltx-config.md's error for
   a missing key, believed: an ordinary read).
2. Resolves it (section 1.1). Not found: a fatal failed assertion.
3. Calls it with two arguments: the **game-type text** (a Lua string) and a boolean, `true` when the class id of the
   server's game object is asked for, `false` for the client's [EP-24].
4. Keeps one result, converted by Lua's string conversion (a number becomes its text, any other non-string gives
   no text) [EP-24].
5. No text, or an empty one: fatal error `Unknown game type: %s`, with the game-type text as argument [EP-22].
6. Otherwise the text is converted to a class id (C 1) and returned [EP-22].

On a dedicated server no script is called: a fixed table maps the game-type texts to the engine's ids [EP-23].

When it is asked (static, sections 3.3 and 3.4):

- **Server side** (flag `true`): when the server is connected (section 3.3, step 9). The game-type text is the second
  `/`-separated item of the server options (`single` in `all/single/alife/new`) [EP-26].
- **Client side** (flag `false`): when the client receives the server's game-configuration message (section 3.3,
  step 11.2).
  The text is read from the message. If the client already has a game object whose type name equals it, nothing
  happens; otherwise any previous one is deleted, `- Game configuring : Started ` is logged and the function is
  called [EP-29]. The server sends the type name of its game object (believed: the single-player server game's type
  name is `single` [EP-28]; the sending code was not traced).
- So the server side asks first (static: its loading stage runs before the client stages, EP-30, EP-31), and in a
  game with the simulation the two calls run in different game states: the server's in the state of the previous
  startup, the client's in the state the rebuild made (section 3.3) (believed, from that order).

The returned id is used to create the game object through the creation entry (C 6.1) [EP-26, EP-29]. The shipped
function `game_registrator.get_game_clsid(game_type_option, is_server)` returns `SV_SINGL` and `CL_SINGL` for
`single` [EP-10]; both are engine entries (C 5.3), so no script object is made.

### 2.3 The start callback `start_game_callback` (static; data measured)

When the simulation is created (a single-player server game whose options contain `/alife`, section 3.3) [EP-27],
in this order [EP-18, EP-19, EP-20, EP-21]:

1. The script state is rebuilt (C 2.3; section 3.3) unless the command line contains `-keep_lua`.
2. The server options must have a non-empty first item (the game or spawn name), `alife` as the third and `single`
   as the second; otherwise a failed assertion with the description `Invalid server options!` (the items as the
   game-persistent object holds them, believed: parsed from the `server(…)` text in this order).
3. `[alife] start_game_callback` of the settings is read (`config\alife.ltx`: `_G.start_game_callback`, with the
   comment `on starting new game or loading saved one`) [EP-2] and resolved (section 1.1). Not found: a failed
   assertion with the description `failed to get start game callback`.
4. It is called once, with no arguments; its results are ignored.
5. Then the simulation starts the game named by the first option: the fourth option is compared with `new` and
   `load`; unless it is `new`, the saved game of that name is loaded if it exists. If none was loaded: unless the
   fourth option is `new`, or it is not `load` and the name is not empty, this is a failed assertion (`Cannot find
   the specified saved game `); otherwise the new game is created from the spawn of that name (offline 8.1). The loading screen shows the text id
   `st_loading_alife_simulator` meanwhile.

So the callback runs before any object of the new or loaded game exists, for New Game and load alike. The shipped
`_G.start_game_callback()` (in `scripts\_g.script`) calls `printf` (of `_g.script`) and then
`task_manager.clear_task_manager()`, `treasure_manager.clear_treasure_manager()`, `xr_sound.clear_all_sound_object()`
and `dialog_manager.fill_phrase_table()` [EP-7]; each of these namespaces is loaded on first access (script-loading.md
2.2, occasion 3).

### 2.4 The tutorial thresholds: the 8 `_G.on_actor_…` functions (static; data measured)

At the end of each update of the player's condition, and only while the game type of spec B 4.5 is 0 or 1 [EP-63],
the game library checks the player against thresholds and may call one of 8 global functions [EP-52]. The update
itself runs only while the player's health is above 0 (and under further conditions on the player whose meaning is
unestablished) [EP-52]; how often it runs is unestablished here (believed: with the player's per-frame update).

- The 7 thresholds are read once, at the first check, from section `[tutorial_conditions_thresholds]` of the settings
  [EP-47]; in `config\system.ltx`: `power` 0.1, `max_power` 0.75, `bleeding` 0.4, `satiety` 0.5, `radiation` 0.1,
  `weapon_jammed` 0.9, `psy_health` 0.5 [EP-5].
- The player's condition keeps a set of **notified** flags, all clear when the condition object is made [EP-50].
- Each check calls at most one function: the first rule below that applies, in this order [EP-48, EP-49]:

| Rule | Applies when | Then |
|---|---|---|
| 1 | power not notified, and power is below `power` | mark it notified; call `_G.on_actor_critical_power` |
| 2 | maximum power not notified, and it is below `max_power` | mark; call `_G.on_actor_critical_max_power` |
| 3 | bleeding not notified, and the player's total bleeding (a sum over its wounds) is above `bleeding` | mark; call `_G.on_actor_bleeding` |
| 4 | satiety not notified, and satiety is below `satiety` | mark; call `_G.on_actor_satiety` |
| 5 | radiation not notified, and radiation is above `radiation` | mark; call `_G.on_actor_radiation` |
| 6 | the psy flag is clear, and psy health is above `psy_health` | call `_G.on_actor_psy` (no flag is set) |
| 7 | the player is not marked overweight | call `_G.on_actor_cant_walk_weight` (no flag is set) |
| 8 | the player is marked overweight, the jammed flag is clear, the player's active slot holds a weapon, and its condition is below `weapon_jammed` | mark jammed; call `_G.on_actor_weapon_jammed` |

  The values compared are the player's conditions the threshold keys name (believed: each rule compares one stored
  value with the threshold of that name; what the values are belongs to the player-condition spec). The comparisons
  are strict.
- The **overweight** mark is set by the player's walk-weight query when the carried weight exceeds the walking limit
  (plus the outfit's addition) and cleared otherwise [EP-50]; when that query runs is unestablished here. No code
  sets the psy flag [EP-50].
- Each name is resolved (section 1.1) at the check that needs it; not found: a fatal failed assertion. The function
  is called with no arguments, its results ignored [EP-51].

Consequences (believed, from the rules as the binary has them): while the player is healthy and not overweight, rule
6 applies at every check, so `_G.on_actor_psy` is called each time; rule 7 then applies whenever psy health is at or
below 0.5 and no earlier rule does; rule 8 is reachable only while the player is overweight. The flags are not
saved (believed: they are cleared when the condition object is made and no other code clears them).

The shipped functions (`scripts\_g.script`) take no parameters; `on_actor_critical_max_power`, `on_actor_satiety`
and `on_actor_psy` are empty [EP-6]; the others give info portions and start tutorials (their engine names belong to
specs P and K).

### 2.5 The weapon parameters: the 5 `ui_wpn_params.…` functions (static; data measured)

The interface's item-information code updates a weapon-parameter window for an item whose configuration section has
the key `fire_dispersion_base` and is not `wpn_addon_silencer`, `wpn_binoc` or `mp_wpn_binoc` [EP-56]. Which screens
show it is unestablished here (it belongs to the interface specs). Each such update [EP-53, EP-54, EP-55]:

1. The first time (or the first time after a rebuild, which releases them [EP-16]), the five functions
   `ui_wpn_params.GetRPM`, `GetDamage`, `GetDamageMP`, `GetHandling`, `GetAccuracy` are resolved, in this order
   (section 1.1; resolving loads namespace `ui_wpn_params`). A failed resolution is not checked; calling such a
   function later is unestablished (believed: a crash).
2. Four calls, each with one argument, the item's section name, keeping one result converted by Lua's number
   conversion (a non-number gives 0): `GetRPM`, then `GetAccuracy`, then `GetDamage` when the client game's type value
   is 1 or `GetDamageMP` otherwise, then `GetHandling`. The value 1 is believed to be single-player (the
   single-player server game sets the same value, EP-28).
3. Each result is the position of one of four progress bars of the window, clamped to the bar's range (the bars are
   spec G2's).

The shipped functions take one parameter, `section` [EP-9]. The calls happen only when that window is shown, so
never before the first frame of a game (believed).

### 2.6 Names the engine never reads (measured)

`ui_type_clsid_factory` (`config\script.ltx [common]`, value `ui_registrator.get_ui_clsid`), and
`can_sleep_callback` and `sleep_video_name_callback` (`config\creatures\actor.ltx`, values
`dream.can_sleep_callback` and `dream.sleep_video_name_callback`, both defined in `scripts\dream.script`) occur in
none of the 37 files under `bin\`, as single-byte or as 16-bit text [EP-1, EP-3, EP-4]. So the engine never reads
these keys and never calls these functions (believed: a key the binaries do not name cannot be read). An
implementation calls none of them.

### 2.7 Bare global names (gap 3 of the scoping spec; static)

Of the 8 functions of `_g.script` whose bare names occur in `xrGame.dll` as whole strings, none is called by the
engine through a name it writes: `abort` and `printf` occur only as names the game library imports from the C
runtime; `switch_online` and `switch_offline` only inside message texts no code refers to; `action` as a method name
of three engine classes and as an element name of dialog files; `round` as a property of an engine class;
`script_object` as a class-id name of the factory; `start_game_callback` as the `[alife]` key the simulation reads
(section 2.3) [EP-61]. With section 1.3 (every name the engine resolves comes from data, scripts, or the 13 names it
writes) [EP-57, EP-58], the engine calls none of `abort`, `action`, `printf`, `round`, `script_object`,
`switch_offline`, `switch_online` (static), and `start_game_callback` only through the data value of section 2.3.

## 3. The New Game call order

The steps in the order the engine performs them, with the spec that owns what runs in scripts at each. "No script"
means no call into scripts was found in that step (static, following the code of the step; calls through virtual
methods were followed only where named).

### 3.1 Program start

1. The engine executable executes the command line's `-start` and `-load` commands (if any), creates the render
   device, and creates the **game-persistent object** through the game library's creation entry (C 6.1, the engine id
   `G_PERSIS`), before its main loop starts [EP-38].
2. The creation entry makes sure the factory exists; creating the factory makes sure the game state exists, which
   runs **startup** (C 2.3) [EP-43]: the engine interface, `_G`, the class registrators (C 4), the `clsid` table,
   the `[common] script` list (section 2.1). So startup has run, at the latest, before the main loop. Nothing earlier
   reaches it: the game library's load-time initialisation reaches neither the game state nor the factory [EP-62]
   (believed: only through direct calls was this followed).
3. In the first frames, the game-persistent object's per-frame step executes the console command `main_menu on`: at
   once with `-nointro`, otherwise after the video sequence `intro_logo` has played [EP-44]. The sequence names no
   script function [EP-11].
4. `main_menu on` activates the main menu, which creates the object of id `MAIN_MNU` through the creation entry
   [EP-46]: by C 5.3 the script class `ui_main_menu.main_menu`, called with no arguments (C 6.2; spec A 2.1). Its
   window overrides and handlers are spec G5's and G1's.

### 3.2 The New Game item

5. The menu's New Game handlers (`scripts\ui_main_menu.script`) execute `g_game_difficulty <difficulty>`, then, after
   `disconnect` when `alife()` is not nil, `start server(all/single/alife/new) client(localhost)` and `main_menu off`
   [EP-12] (console line execution: spec B 2.2).
6. The console command `start` takes the texts inside `server(…)` and `client(…)` (a single-player server without a
   client gets `localhost`) and only queues the event `KERNEL:start` [EP-39]; `main_menu off` then deactivates the
   menu (its window is deleted later, step 9) [EP-46].
7. Queued events are handled by the application's per-frame handler [EP-41] (in the same frame or the next one:
   the order of the per-frame handlers is unestablished here). Handling `KERNEL:start` does: `main_menu off` once
   more, the console hidden, the render device reset, the game-persistent object's pre-start and start, the game
   level object created (engine id `G_LEVEL`) with a loading phase around the level start, which queues the loading
   stages [EP-40, EP-30]. **Disputed in review:** the order is misstated: the pre-start comes first, then the game
   level object is created, then the loading phase is opened, then the game-persistent object's start runs, then
   the level start, then the loading phase is closed; so the start runs after the level object exists and inside
   the loading phase. No script in these; the start installs a per-frame step for after loading (step 13) [EP-64].

### 3.3 Loading stages (before the first frame)

While a loading stage is queued, each iteration of the main loop runs the first one (removing it when it reports
done) and draws the loading screen; no frame runs [EP-42]. The level start queues six stages; the fourth removes
itself and puts six client stages in front of the remaining two [EP-30, EP-31]. In order:

8. **Server starting** (loading-screen text id `st_server_starting`): the server object is made. No script [EP-32].
9. **Server connect** [EP-32, EP-26]:
   1. The **game-type function**, server side (section 2.2), in the state of the startup of step 2.
   2. The server's game object is created through the creation entry (`SV_SINGL`: an engine entry, no script).
   3. Its base creation makes the **game-type script list** of usage 5.5 from `script.ltx [single] script` [EP-36]:
      empty in this edition (usage 5.5), so no coroutine.
   4. With `/alife` in the options, the **simulation** is created [EP-27], which does, in order:
      - The **rebuild** (unless `-keep_lua`) [EP-16, EP-17]: the weapon-parameter functions are released (section
        2.5), the main menu deletes its window object at once (the script object of step 4), the factory is
        discarded, and startup runs again (C 2.3): the old game state is closed and a new one opened, so every
        namespace loaded so far is gone, and the class registrators run again with a new factory. That closing the
        old state runs the `__finalize` methods of its remaining script objects (spec A 7.1) is believed (the public
        Lua 5.1 closes a state by collecting everything in it).
      - The server-option assertion and the **start callback** (section 2.3).
      - The new game: the records of the spawn `all` are created and registered and the game is saved (offline
        8.1); the server objects of records whose class id has a script server class are made by calling that class
        (C 6.2); which of their overrides run and when is spec J's.
   5. The network server then accepts connections (believed: the network start that follows).
10. **Options**: port, password and key options of the client line. No script [EP-30].
11. **Client stages** [EP-33]:
    1. Connecting (`st_client_connecting_to`). No script.
    2. Connect: the client connects and processes the server's messages until it is accepted; the
       game-configuration message makes it ask the **game-type function**, client side (section 2.2), in the new
       state, and create its game object (`CL_SINGL`: an engine entry) [EP-29, EP-34].
    3. Level: the level is chosen and loaded (failure: `Loading failed.`); level loading makes the **level script
       list** of usage 5.5 from `[level_scripts] script` [EP-37]: no level of this edition has that section (usage
       5.5), so no coroutine.
    4. Spawning (`st_client_spawning`): the client processes the server's messages, and the server is updated, until
       the message that ends spawning [EP-33, EP-34]. Client objects created in this stage are made through the
       factory (C 6.2: an engine client class, or a script class called with no arguments); an object whose section
       has `script_binding` gets its binding function resolved and called (section 1.3; its failure goes to a message
       function that does nothing [EP-60]), which attaches a script binder (spec L; the binder overrides and their
       order are spec L's). Which objects come online in this stage, and so which binding functions run before the
       first frame, is unestablished here (offline 6, with its switch rule in 6.5, owns when objects switch online).
    5. Textures (`st_loading_textures`). No script.
    6. Synchronising (`st_client_synchronising`): a precache phase of 30 frames is requested and the loading phase
       of the client is closed [EP-33].
12. **Ready**, then **end**: the last stage closes the level start's loading phase; on failure it logs
    `! Failed to start client. Check the connection or level existance.` and executes `main_menu on` [EP-35].

### 3.4 From the first frame

13. The frames of the precache phase follow (spec B 3.4). Without `-nointro`, once the level runs and fewer than
    three precache frames remain, the start's per-frame step plays the video sequence `intro_game` for a new game
    [EP-45]; it names no script function [EP-11].
14. From the first frame on, the engine's per-frame and scheduled work makes its calls into scripts (binder and
    simulation-object updates, callbacks; specs L, J, O and others), and the tutorial thresholds (section 2.4) are
    checked with the player's condition updates (believed: those updates run in frames, not in loading stages). The
    weapon-parameter functions run only when their window is shown (section 2.5).

### 3.5 Summary across specs

| Order | Step | Calls into scripts | Spec |
|---|---|---|---|
| 1 | program start, startup | `_G` top-level code; `class_registrator.register`, `game_registrator.register`; `<item>_initialize` (none in this edition) | loading, C, D |
| 2 | first frames | the main-menu script class and its window methods | C, G5, G1 |
| 3 | New Game item | menu handlers; `start`, `main_menu off` | G5, B |
| 4 | server connect | game-type function (server side, old state) | D |
| 5 | game-type script list | none in this edition | usage |
| 6 | rebuild | `__finalize` of old script objects (believed); a full startup again | A, C |
| 7 | start callback | `_G.start_game_callback` | D |
| 8 | new game | simulation-object classes and overrides | J |
| 9 | client connect | game-type function (client side, new state) | D |
| 10 | level loading | level script list: none in this edition | usage |
| 11 | client spawning | client classes; binding functions; binder overrides | C, L, M |
| 12 | first frame onwards | updates and callbacks; tutorial thresholds | L, J, O, D |

### 3.6 Worked example: a New Game of `l01_escape`

With the shipped data and no `-keep_lua`, the calls this spec defines before the first frame of a New Game are, in
order: the startup at program start (`_G`, the two registrators; no `_initialize` function), the main-menu script
object, the menu handlers; `game_registrator.get_game_clsid("single", true)` returning `SV_SINGL` (old state); the
rebuild with a second full startup; `_G.start_game_callback()`; the creation of the spawn `all`'s records (the actor's
record places the game on `l01_escape`, `docs/formats/sav.md` 10.2 and offline 5.1; 436 of the level's 870 records have a script server class,
scoping spec section 5); `game_registrator.get_game_clsid("single", false)` returning `CL_SINGL` (new state); then the
client objects of the level and their binding functions (spec L; 367 of the 671 objects online at the start have a
`script_binding`, scoping spec section 5). No `_G.on_actor_…` and no `ui_wpn_params.…` function is called before
the first frame (believed, sections 2.4 and 2.5).

## 4. Scope accounting

The entry points of scoping spec 4.5 assigned to D: `game_type_clsid_factory` (2.2), `start_game_callback` (2.3),
`can_sleep_callback`, `sleep_video_name_callback` and the unread `ui_type_clsid_factory` (2.6), the 13 functions of
its 4.2 (2.4, 2.5), and the order of its section 5 across specs (section 3, whose steps 9.4 and 11.4 are its steps 5
and 6). Its gap 3 is section 2.7. Added: `<item>_initialize` (2.1). This spec owns no listed item and no engine name
the scripts use (scoping spec section 6, row D).

## 5. Unestablished

- How often the player's condition is updated, and the meaning of the further conditions that skip the update
  (2.4); when the walk-weight query runs; the formula of the total bleeding (player-condition spec).
- What happens when a weapon-parameter function was not found (believed a crash), and which screens show the
  weapon-parameter window (2.5).
- The text the server sends for the client's game type (believed its game's type name, `single`) (2.2).
- Which objects come online, and which binding functions run, before the first frame (3.3, step 11.4).
- Whether closing the old state at the rebuild runs `__finalize` methods (believed yes) and what happens to a
  non-empty game-type script list across the rebuild (none in this edition) (3.3, step 9).
- That no step of 3.1–3.3 calls into scripts other than as stated: static, following each step's code; calls through
  virtual methods were not all followed.

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `bin\xrGame.dll` and `bin\XR_3DA.exe`
  as identified above, and the 37 files under `bin\` for string searches; from the archive file tree of
  `gamedata.db0` … `gamedata.dbd`: `config\script.ltx`, `config\alife.ltx`, `config\system.ltx`,
  `config\creatures\actor.ltx`, `config\ui\ui_movies.xml` and the `scripts\` files.
- Analysis: static analysis with Ghidra (headless, read-only: decompilation, disassembly, cross-references, caller
  trees and instruction scans) of the game library's name resolution and its callers, the startup's script list, the
  script-state rebuild, the simulation's creation, the game-type function and its callers, the level start and its
  loading stages, the client's message handling, the server game's creation, the player's condition thresholds and
  the weapon-parameter window; of the engine executable's program start, `start` command, event handling and main
  loop. String and byte searches of the binaries; the configuration files and scripts read as text. No binary was
  executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- Section 2.3 does not say whether `alife()` already returns the simulation while the start callback runs.
- Section 3 gives no call order for a game started from the command line (`-start`) without the main menu,
  beyond step 1 of 3.1.
- Section 3.3 step 9.3 makes the game-type script list in the server game's base creation, while usage 5.5 says
  it is made when the game state is created; the difference is not reconciled.
