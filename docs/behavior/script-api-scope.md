# Script API: scope and split

Scoping specification of the engine-side interface that the shipped game scripts of S.T.A.L.K.E.R.:
Shadow of Chernobyl (Russian edition) use: what scripts can reach (globals, namespaces, classes and their
members, constants), the calls the engine makes into scripts, measurements of both, the order of the
engine's calls into scripts during a New Game as far as needed to choose the first specifications, and a
proposed split of the interface into small specifications. It defines no behaviour of individual
functions; each proposed specification does that.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`: the runtime (LuaJIT 1.0.3), its standard libraries, the binding
  library that exports the engine interface, the game state.
- `docs/behavior/script-loading.md`: script names, namespaces, automatic loading, the `[common]` keys of
  `config\script.ltx`, startup (engine registration, then `_G`, then the class registrators), the
  comment removal of its section 1.5, and the 250 scripts the game can be led to load (its section 3),
  called **loadable** here.
- `docs/behavior/level-objects.md` (which objects of `levels\l01_escape\` are online at the start, section 2),
  `docs/behavior/offline-simulation.md` (creation order at New Game, section 8.1),
  `docs/behavior/player-state.md`, `docs/behavior/save-load-screens.md`.

Conventions:

- Files are the winning copies of the root archives `gamedata.db0` … `gamedata.dbd`, compared by stored
  path ignoring case. `scripts\` holds 441 files: 440 `.script` files and one backup
  (`ui_mm_mp_join.script~`). One `.script` file, `scripts\lua_help.script`, is not Lua but a listing (2.1);
  the other **439 scripts** are scanned. `xrGame.dll` is the file of `bin\` identified in the script
  environment specification (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [extra.py].
- Names are given only as the shipped scripts or the shipped data spell them. Everything else is counted,
  not named.
- Each fact is **measured** (from the data, or from file bytes), **static** (from static analysis of
  `xrGame.dll`, not observed at run time), **believed** (inferred, not traced) or **unestablished**. No
  binary was run.
- Checks are Python scripts in the dirty workspace directory `checks/script-api-scope/`, named in
  brackets after each claim. `run_all.py` is the one entry point: it runs them all in order, is
  deterministic, and every number below is in its output (`run_all.out`). **Disputed in review:** the
  output is not fully deterministic: the order of one printed list (the 6 members inherited through two
  bases, 2.1) differs between runs; every number is the same.

## 1. Counting rules

These rules define every count of this document unless a count states its own.

- **Script text.** Each scanned script is decoded as Windows-1251 after the comment removal of
  script-loading.md section 1.5, and its `--` comments (line and long) are dropped. String literals are
  tokens, never uses (a name inside a string is not counted, except where 4.1 and 4.4 count strings that
  name functions) [scan.py].
- **A use** is one occurrence, outside comments and strings, of one of the forms of the table below. Names
  are compared case-sensitively. Local scopes are tracked: a bare name is a use only where no local
  (parameter, `local`, loop variable) of that name is in scope. `_G.x` is read as the bare name `x`.
- **Bare names.** A bare name is an engine name only when the listing (2.1) has it as a global function,
  a namespace or a class, or it is one of the two unlisted globals of 2.2 (`IsGameTypeSingle`,
  `valid_saved_game`). It is not one when it is defined by the script itself (a function, a global
  assignment, a declared class) or by `_g.script` (for scripts other than `_g.script`), is a script name,
  or is a standard library name. A bare name that is none of these is **never defined** (2.2) [classify.py].
- **Member names.** `:name(` is a method use when `name` is a listed class function and the receiver is not
  a script namespace; `.name` is a property use when `name` is a listed property and the expression before
  it is not a script namespace or a name `_g.script` defines; `Class.name` after a listed class is a
  constant or a base-class call; `ns.name` after a namespace is a namespace function. A member name is
  attributed to every listed class that **declares** a member of that name (inherited repeats of the
  listing excluded, 2.1); a name several classes declare counts for each of them, so counts that attribute
  names to areas or specs are upper bounds where stated.
- **Unlisted and absent.** A method, namespace member or class member that the scripts use and the listing
  lacks is **unlisted** when it passes REG or DLL (2.2) and **absent** otherwise (a one-character method
  name is always absent). A method name that a
  script class defines is neither (it is the script's own). Bare names are not tested this way (2.2,
  "Bare names").
- **Duplicates.** Every count of names is of distinct names (distinct (kind, name) pairs where kinds are
  counted separately); every count of scripts is of distinct files. Repeated uses in one script count once.
- **Loadable.** A script counts as loadable when it is one of the 250 of script-loading.md section 3
  [run_all.py, `reach250.txt`].

| Way a script reaches the engine | Example from the scripts | Kind in the tables below |
|---|---|---|
| global function | `time_global()`, `alife()`, `get_console()` | global |
| namespace, and a function or class in it | `level.object_by_id(id)`, `game.get_game_time()`, `game.CTime()`, `main_menu.get_main_menu()` | namespace, namespace function |
| class used by name: constructor call, constant holder | `vector()`, `CUIScriptWnd()`, `clsid.script_stalker`, `callback.death` | class, constant |
| method of an object the engine returned | `npc:position()`, `ini:r_string(s, k)` | method |
| field of such an object | `v.x`, `obj.health` | property |
| script class deriving from an engine class | `class "main_menu" (CUIScriptWnd)` | base |
| call of an engine base-class method | `action_base.initialize(self)`, `CGameObject.Load(self, section)` | base call |
| the binding library's own globals | `class`, `super` | binding |

A property the listing lacks cannot be found by these rules (a field counts only when its name is a listed
property), so the unlisted and absent sets cover methods, namespace and class members, but not properties.

## 2. Sources and cross-checks

### 2.1 The listing `scripts\lua_help.script` (measured, [xcheck.py], [decl.py], [extra.py])

A shipped data file whose first line is `List of the classes exported to LUA`. It lists 301 classes (each
with its base classes, properties, constants, constructors and functions with parameter types) and 5
namespaces: the global one and `game`, `level`, `relation_registry`, `actor_stats`, with 116 distinct
functions in all (distinct names per namespace). It does not show which classes are registered inside a
namespace (the scripts reach the listed class `CTime` only as `game.CTime`). It is not loaded by the game
(script-loading.md section 4.1). It was produced by a build close to, but not identical with, the shipped
`xrGame.dll` (2.2).

The listing repeats a class's inherited members in every derived class. Counted per class, distinct per
kind (function, property, constant) and name, the 301 classes list 6,054 members; 3,211 of them are
**inherited repeats** (a class in the member's base chain, following the listed bases transitively, lists
a member of the same kind and name), and 2,843 are **declared** members (listed by a class none of whose
bases list it). Each member is counted once, at the class that declares it; 6 of the inherited repeats reach
their class through two bases that both declare the member, and belong to both. Every base the listing names is a listed class
[decl.py].

### 2.2 Cross-check against `xrGame.dll` (static and measured, [xcheck.py], [unlisted_evidence.py])

Every function of `xrGame.dll` that calls the binding library's registration entry points, directly or
through small helper functions, was collected with Ghidra (static; `script-api/regevents.txt`, from
`ghidra_scripts/RegEvents.java`), with its identifier-string references and registration calls in
code order. Three tests per name:

- **REG**: registration code references the name as a string;
- **paired**: such a reference is directly followed by a registration call (of a class, namespace,
  method, property, constant or function);
- **DLL**: the name's bytes occur in `xrGame.dll` not preceded by a letter, digit, `_` or `.` and directly
  followed by a zero byte (measured, [dllstr.py]).

REG and paired are approximations: registration code also references other strings (configuration keys
and the like), and names passed through helper code are missed. Where the listing and the binary
disagree, both are reported below; neither is preferred.

| Listed names (distinct) | Count | REG | REG or DLL | In neither |
|---|---|---|---|---|
| class names | 301 | 300 | 300 | 1 |
| class function names | 964 | 951 | 951 | 13 |
| class property names | 139 | 136 | 136 | 3 |
| constant names (without the story ids, 2.3) | 820 | 644 | 794 | 26 |
| global functions | 48 | 29 | 44 | 4 |
| `level` functions | 56 | 35 | 53 | 3 |
| `game`, `relation_registry`, `actor_stats` functions | 5, 3, 4 | 4, 2, 2 | 5, 3, 4 | 0 |

**Classes and namespaces** (static): 305 strings are paired with the class-registration entry point;
298 are listed classes. Of the other 7, 2 are listed constant names of listed classes (believed not to name
the class registered there), and 5 are class names the listing lacks: the class of
the main-menu object, the class of the object its `GetPatchProgress()` returns (its registration is
followed by method registrations that reference `GetFlieName`), `CPhysicObject` (named as a client class by
`class_registrator.script`), and 2 environment classes. 7 strings are paired with opening a namespace: the
4 listed namespaces, `main_menu` (not listed), and 2 that are listed global functions.

**Listed but in neither test, used by scripts**: the method `debug_planner` (1 loadable script),
`level.debug_actor` (1), `level.debug_object` (2), the property `Skip` (1), the global
`cast_alife_object_to_trader_abstract` (no loadable script, 1 script), and 20 `clsid` constants, all script
class-id names added by the registrators (2.3) [unlisted_evidence.py], [extra.py]. Reaching one of the
first five at run time fails on a missing name (believed).

**Unlisted names the scripts use**: 28 distinct names in 43 scripts; 24 of them in 25 loadable scripts
[classify.py]. With the evidence for each (loadable scripts; REG; paired; DLL), and the object the
scripts call it on (measured from the scripts) [unlisted_evidence.py]:

| Name as used | Loadable scripts | REG | Paired with | DLL |
|---|---|---|---|---|
| `IsGameTypeSingle` (global) | 1 | yes | (referenced among the global-function registrations, next to the listed `command_line`) | yes |
| `valid_saved_game` (global) | 1 | yes | namespace scope | yes |
| `main_menu.get_main_menu` (namespace `main_menu`) | 3 | no | — | yes |
| `game.CTime` (listed class, in namespace `game`) | 4 | yes | class | yes |
| `level.actor`, `level.object` | 7, 5 | yes | (common strings; not established as `level` functions) | yes |
| methods `CancelDownload`, `GetGSVer`, `GetPatchProgress`, `ValidateCDKey`, called on `mm = main_menu.get_main_menu()` | 2, 2, 2, 1 | yes | method (on which class: unestablished) | yes |
| methods `GetFlieName`, `GetInProgress`, `GetProgress`, called on `sss = mm:GetPatchProgress()` | 2 each | yes | method; `GetFlieName` within the registration of that object's class (above), the other two unestablished | yes |
| method `InitProgressBar`, called on the window-description reader (`xml:InitProgressBar(...)`) | 2 | yes | method, within the registration of the listed reader class `CScriptXmlInit`, after its listed `Init…` methods | yes |
| method `SetProgressPos`, called on the control `InitProgressBar` returns | 2 | yes | method, within the registration of the listed progress-bar class | yes |
| method `ModifDigitOnly`, called on a file-list entry (`flist:GetAt(it)`, `flist` from the file system's `file_list_open_ex`) | 2 | yes | method, within the registration of the listed file-system entry class, after its listed `NameFull`, `NameShort`, `Modif` | yes |
| method `set_community_goodwill` (on game objects) | 2 | yes | (the string is registered as the listed `relation_registry` function; as a method, not established) | yes |
| method `community_goodwill` (on game objects) | 1 | no | — | yes |
| method `actor_sleep` (on the actor) | 1 | yes | — | yes |
| base calls `CGameObject.Load`, `CGameObject.shedule_Update` | 1 | yes | method (on which class: unestablished) | yes |
| base call `CGameObject.net_Destroy` | 1 | no | — | yes |
| base calls `cse_anomalous_zone.update`, `cse_zone_visual.update` | 1 | yes | method (`update` is declared by other listed simulation classes) | yes |
| 4 names only in non-loadable scripts | 0 | yes | class or method | yes |

"Within the registration of" means the nearest preceding class registration in code order, with that
class's listed members referenced between the two (static); that the binary keeps one class's
registration together is believed. Where "paired" gives no class, which class the binary registers the
name on is **unestablished** and belongs to the owning spec (section 6).

**Absent names the scripts use** (in neither the listing nor REG nor DLL; 69 names in 36 scripts, 23 in 13
loadable scripts): the methods `AddText`, `DetachAll`, `GetCheckedPos`, `GetFrameRect`, `GetRadioGroup`,
`SetCheckedPos`, `air_attack`, `air_attack_active`, `create_u32`, `give_info_portion_via_pda`,
`is_actor_sleeping`, `nearest_accessible_position`, `setEnabled`, `setVisible`, `set_node_evaluator`,
`set_path_evaluator`, `set_use_callback`; the base call `UIGame_custom_script.IR_OnKeyboardPress`; the
constants `clsid.wpn_fn2000_s`, `clsid.wpn_fort_s`, `clsid.wpn_shotgun_S`, `key_bindings.kEXT_1`,
`key_bindings.kEXT_2` (the 23 of loadable scripts; [classify.py], kind `absent`). A script reaching one of
them at run time gets nil or fails (believed).

**Bare names read and never defined** (section 1): 80 names (51 in loadable scripts). By the rule of
section 1 they are not engine names; REG and DLL are not applied to bare names, because common words occur
as strings for other reasons. Applied, 16 of the 80 (12 in loadable scripts) pass REG or DLL: `actor`,
`death`, `field`, `float`, `force`, `goodwill`, `item`, `item_name`, `l`, `name`, `npc`, `obj`,
`particles`, `result`, `section`, `stalker`. None is paired with a registration of a global function or a
namespace; 3 (`goodwill`, `name`, `section`) are paired only with member registrations, 2 (`actor`, `obj`)
with a helper call that registers classes, members and scopes together, and 11 are unpaired
[unlisted_evidence.py]. So the scan and the binary disagree on none of them as globals, but whether the
engine defines any of the 16 as a global is **unestablished**. The 80 are nil at run time unless defined
as globals by the engine or by means the scan does not follow (believed).

### 2.3 Constants that come from game data (measured, [datadriven.py], [xcheck.py], [extra.py])

- The listed class `story_ids` has 637 constants; 636 have the same name and value as an entry of
  `config\game_story_ids.ltx` (638 entries), and only 1 of the 637 names passes DLL. No script uses
  `story_ids`.
- The listed class `clsid` has 185 constants. The 25 that fail DLL are all among the 54 script class-id
  names that the class registrators pass (`script_stalker`, `smart_terrain`, …; 51 of the 54 are listed).
  So `clsid` holds the engine's class ids plus the ids the registrators add.
- Of the 820 other listed constants, 26 are in neither REG nor DLL: those 25 `clsid` names and one more,
  which no script uses and whose source is **unestablished**.

## 3. What scripts reach, by functional area

Every listed class, namespace function and global function, and every declared member of the class
`game_object` (the scripts' handle to an object in the level), is assigned to one area by an explicit
table ([areas.py]). "Entries" are distinct names per area: the declared members of its classes (2.1;
inherited repeats not counted), plus the class itself when it has a constructor or lists no members, plus
its namespace and global functions. "Found" counts entries passing REG or DLL. Script counts are
"lower (upper)": lower counts a script only for names that belong to that area alone; upper counts every
script using a name the area has (section 1). This table covers listed names; the unlisted names of 2.2 are
in section 6 ([report.py]).

| Area | Listed entries | Found in xrGame.dll (REG or DLL) | Names used by loadable scripts | Loadable scripts using it | All 439 scripts using it |
|---|---|---|---|---|---|
| Values and math (vectors, matrices, colours, rectangles, flags, bit operations) | 89 | 89 | 46 | 60 (64) | 106 (112) |
| Time (game time, real time, time factor) | 30 | 30 | 19 | 22 (67) | 29 (90) |
| Configuration and files (`.ltx` access, token lists, file system) | 70 | 70 | 34 | 55 (141) | 72 (203) |
| Engine services (console, device, log, editor flag, prefetch) | 44 | 44 | 15 | 27 (27) | 60 (61) |
| Data streams (packets of saved and network data) | 57 | 57 | 22 | 25 (51) | 28 (67) |
| Classes and class ids (`clsid`, the object factory) | 186 | 161 | 100 | 22 (22) | 28 (28) |
| Simulation (simulation objects, the simulator, game graph) | 176 | 174 | 114 | 46 (148) | 75 (223) |
| Level (`level` namespace core, patrol paths, level calls) | 39 | 36 | 26 | 63 (118) | 89 (190) |
| Game objects (identity, place, condition, inventory, kinds of object, binders, callbacks) | 362 | 362 | 177 | 97 (169) | 140 (257) |
| Character control and AI (actions, planner, movement, sight, memory) | 404 | 401 | 178 | 77 (159) | 121 (242) |
| Sound | 85 | 85 | 47 | 22 (52) | 33 (71) |
| Effects and weather (effectors, post-processing, particles, weather) | 44 | 44 | 32 | 17 (33) | 25 (48) |
| User interface (windows, controls, window files, fonts, keys, HUD) | 470 | 457 | 184 | 33 (36) | 56 (60) |
| Story systems (info portions, tasks, news, map spots, relations, dialogs, trade) | 112 | 111 | 77 | 86 (88) | 155 (159) |
| Multiplayer | 166 | 163 | 111 | 15 (52) | 19 (89) |
| Story ids (`story_ids`, from game data, 2.3; and the one constant of `spawn_story_ids`) | 638 | 2 | 0 | 0 (0) | 0 (0) |

Totals: 2,972 listed entries (summed over areas), 2,286 found in the binary; 1,107 distinct listed names
used by the loadable scripts; 232 of the 250 loadable scripts and 388 of all 439 use at least one listed
engine name.

By kind of use, distinct names (in loadable scripts) and loadable scripts using them ([classify.py]):

| Kind | Names | Loadable scripts |
|---|---|---|
| global function | 25 | 65 |
| namespace (`level`, `game`, `relation_registry`, `actor_stats`, `main_menu`) / listed namespace functions | 5 / 50 | 84 / 73 |
| class used by name | 93 | 148 |
| constant | 301 | 108 |
| method | 500 | 224 |
| property | 82 | 133 |
| base class of a script class | 31 | 73 |
| base-class method call | 73 | 57 |
| `class`, `super` | 2 | 134 |
| unlisted (2.2) | 24 | 25 |
| absent (2.2) | 23 | 13 |
| standard library names (not engine interface) | 14 | 120 |

The 25 global functions used by loadable scripts: `GetARGB`, `GetFontDI`, `GetFontLetterica16Russian`,
`GetFontLetterica18Russian`, `GetFontLetterica25`, `GetFontMedium`, `GetTextureInfo`, `alife`, `app_ready`,
`bit_and`, `bit_or`, `create_ini_file`, `device`, `dik_to_bind`, `editor`, `game_graph`, `getFS`,
`get_console`, `get_hud`, `log`, `prefetch`, `system_ini`, `time_global`, `verify_if_thread_is_running`,
`xrRender_test_r2_hw` ([extra.py]).

## 4. Calls from the engine into scripts

### 4.1 Script functions named in data (measured, [cfgrefs.py], [extra.py])

Counted: a `namespace.function` word (not preceded or followed by a letter, digit, `_` or `.`) whose
namespace is a script name (ignoring case) and whose function that script defines (as a function or a
class), in the 893 `.ltx` and
`.xml` files of `config\` (of its 899 files; the others are 2 `.scc`, 2 `.script`, 1 `.bin` and 1 `.ltx~`
backup) and the 195 `.ltx`, `.ini`, `.spawn` and `.game` files outside `config\` and `scripts\`. Each is
attributed to the `.ltx` key or the enclosing `.xml` element it occurs in. Result: 234 distinct functions.
By where they are named:

| Where | Functions | Files |
|---|---|---|
| `config\gameplay\` dialog files: element `action` | 74 | 18 |
| dialog files: element `precondition` | 64 | 12 |
| `config\gameplay\dialogs.xml`: element `init_func` | 29 | 1 |
| `config\gameplay\` task files: `function_fail`, `function_complete`, `function_call_complete`, `function_call_fail` | 19, 17, 4, 1 | 8, 8, 3, 1 |
| `config\ui\game_tutorials.xml`, `ui_credits.xml`, `ui_movies.xml`: `function_on_stop`, `function_on_start` | 9, 1 | 3, 1 |
| key `script_binding` of object sections | 10 | 24 |
| `config\script.ltx [common]`: `class_registrators`, `game_type_clsid_factory`, `ui_type_clsid_factory` | 2, 1, 1 | 1 |
| `config\alife.ltx [alife] start_game_callback` (`_G.start_game_callback`) | 1 | 1 |
| `config\creatures\actor.ltx`: `can_sleep_callback`, `sleep_video_name_callback` | 1, 1 | 1 |

The 10 `script_binding` functions: `bind_heli.bind`, `bind_monster.bind`, `bind_physic_object.init`,
`bind_respawn.bind`, `bind_restrictor.bind`, `bind_smart_terrain.bind`, `bind_stalker.actor_init`,
`bind_stalker.init`, `phantom_manager.bind`, `xr_zones.bind`.

`ui_type_clsid_factory` (value `ui_registrator.get_ui_clsid`) occurs in no file under `bin\` (measured,
[static_newgame.py]), so the engine never reads it (believed).

### 4.2 Script function names in the binaries (measured, [binnamed.py])

Counted: in every one of the 37 files under `bin\` (subfolders included), each string of the form
`namespace.function` (not preceded by a letter, digit, `_` or `.`, directly followed by a zero byte) whose
namespace is `_G` or a script name (ignoring case) and whose function that script defines. Only
`xrGame.dll` has any: 13 strings, all distinct: `_G.on_actor_critical_power`,
`_G.on_actor_critical_max_power`, `_G.on_actor_bleeding`, `_G.on_actor_satiety`, `_G.on_actor_radiation`,
`_G.on_actor_psy`, `_G.on_actor_cant_walk_weight`, `_G.on_actor_weapon_jammed` (8, defined in `_g.script`),
and `ui_wpn_params.GetRPM`, `ui_wpn_params.GetDamage`, `ui_wpn_params.GetDamageMP`,
`ui_wpn_params.GetHandling`, `ui_wpn_params.GetAccuracy` (5). When the engine calls them is
**unestablished** here.

### 4.3 Overridden engine-class methods (measured, [overrides.py])

249 of the 355 distinct script class names declared with `class "name" (base)` reach an engine class
through their base chain. A method such a class defines is counted as an override the engine can call when its name is
`__init`, or a listed member of the engine base chain, or a name whose engine base-class method the
method's own body calls (`CGameObject.Load(self, …)` inside `Load`); the last rule finds overrides of
members the listing lacks. By family of the engine base class; "script classes" counts the distinct class names
with at least one override, and script counts are of the files defining the overriding methods:

| Family | Script classes | Overridden names | Loadable scripts (all) |
|---|---|---|---|
| object binder (`object_binder`) | 11 | 12: `__init` `reinit` `reload` `net_spawn` `net_destroy` `net_import` `net_export` `net_save_relevant` `net_Relcase` `update` `save` `load` | 10 (11) |
| simulation object (`cse_…` bases) | 21 | 21: `__init` `STATE_Read` `STATE_Write` `UPDATE_Read` `UPDATE_Write` `FillProps` `on_before_register` `on_register` `on_spawn` `on_unregister` `on_death` `update` `can_switch_online` `can_switch_offline` `keep_saved_data_anyway` `detect_probability` `enabled` `suitable` `register_npc` `unregister_npc` `task` | 9 (10) |
| planner (`action_base`, `action_planner`, `property_evaluator`, `property_evaluator_const`) | 178 | 7: `__init` `initialize` `execute` `finalize` `evaluate` `weight` `reinit` | 26 (43) |
| UI window (`CUIScriptWnd`, `CUIListItemEx`, `CUIWindow`, `UIGame_custom_script`) | 28 | 8: `__init` `Init` `Show` `Update` `OnKeyboard` `Dispatch` `SetClGame` `IR_OnKeyboardPress` | 20 (26) |
| effector (`effector`) | 6 | 2: `__init` `process` | 4 (7) |
| client object (`CGameObject`) | 1 | 8: `__init` `Load` `shedule_Update` `net_Spawn` `net_Destroy` `net_Import` `net_Export` `use` | 1 (1) |
| multiplayer (`game_…` bases) | 4 | 22 | 3 (5) |

Found only through the base-call rule: `CGameObject` `Load`, `shedule_Update`, `net_Destroy`;
`cse_anomalous_zone` and `cse_zone_visual` `update` (declared by other listed simulation classes);
`action_planner` `reinit` (no loadable script); `UIGame_custom_script` `IR_OnKeyboardPress` (absent, 2.2);
one multiplayer name.

### 4.4 Functions handed to the engine at run time (measured, [entrypoints.py])

Counted: listed members with a parameter whose listed type is a script function (`function<…>`), 11 of
them, and listed members whose string argument names a script function (the dialog and task members whose
data counterparts are in 4.1, and the string form of the level call members), 8 entries. Script counts are
loadable (all) scripts using the member's name (methods by name on any object, section 1):

- taking a function: `set_callback` 15 (24), `AddCallback` of script windows 10 (17), `set_fastcall` 3 (3),
  `set_patrol_extrapolate_callback` 2 (2), `set_enemy_callback` 1 (1), `iterate_inventory` 1 (1),
  `inventory_for_each` 1 (1), `level.iterate_sounds` 0 (1), `level.add_call` and `level.remove_call` 0 (0),
  and `add` of the client spawn manager (the name `add` is used by 7 (15) scripts on various objects; the
  only script reaching the client spawn manager, through `level.client_spawn_manager()`, is not loadable);
- taking a function name as a string: `AddAction`, `AddPrecondition` 2 (2) each, `add_complete_func`
  1 (1), `add_fail_func`, `add_on_complete_func`, `add_on_fail_func` 0 (0), and the forms
  (object, method name, method name) of `level.add_call` and `level.remove_call` 0 (0).

`bind_object` (10 (11)) attaches a script binder object, whose overrides are those of 4.3. `set_callback`
takes an event id from `callback`; the scripts use 24 of its 34 listed constants: `article_info`, `death`,
`helicopter_on_hit`, `helicopter_on_point`, `hit`, `inventory_info`, `level_border_enter`,
`level_border_exit`, `on_item_drop`, `on_item_take`, `patrol_path_in_point`, `script_animation`,
`take_item_from_box`, `task_state`, `trade_perform_operation`, `trade_sell_buy_item`, `trade_start`,
`trade_stop`, `trader_global_anim_request`, `trader_head_anim_request`, `trader_sound_end`, `use_object`,
`zone_enter`, `zone_exit` ([extra.py]).

### 4.5 Which spec defines each entry point

Each entry point's arguments, results, timing and failure handling are defined by exactly one spec of
section 6 ([entrypoints.py] asserts that every member of 4.4 has one):

| Entry point | Spec |
|---|---|
| how the engine reaches a script override at all, `__init`, base-class calls | A |
| `class_registrators` (the call and its object-factory argument) | C |
| `game_type_clsid_factory`, `start_game_callback`, `can_sleep_callback`, `sleep_video_name_callback`, the 13 functions of 4.2, the unread `ui_type_clsid_factory`; and the order of the steps of section 5 across specs | D |
| `script_binding`, `bind_object`, binder overrides | L |
| simulation-object overrides | J |
| planner overrides | W |
| window overrides of `CUIScriptWnd`, `CUIListItemEx`, `CUIWindow`; `AddCallback` | G1 |
| overrides of `UIGame_custom_script` | G4 |
| `effector` overrides | U |
| `CGameObject` overrides | M |
| multiplayer overrides | Z |
| `set_callback`, `set_fastcall`, `set_enemy_callback`, `set_patrol_extrapolate_callback`, `level.add_call` and `level.remove_call` in all their forms | O |
| `add` of the client spawn manager | N |
| `level.iterate_sounds` | T |
| `iterate_inventory`, `inventory_for_each` | S |
| dialog-file functions (`action`, `precondition`, `init_func`), `AddAction`, `AddPrecondition` | R |
| task-file functions (`function_complete`, `function_fail`, `function_call_complete`, `function_call_fail`), `add_complete_func`, `add_fail_func`, `add_on_complete_func`, `add_on_fail_func`, tutorial and movie functions (`function_on_start`, `function_on_stop`) | P |

## 5. Engine calls into scripts during New Game

Steps in order; each names what it is established from. "Closure" is the static call closure of
[closure.py]: from the step's entry functions, it follows bare calls of functions of the same script or of
`_g.script`, `script.function` and `script:function` calls (which also load that script), `self:method`
inside methods of script classes, and instantiation of script classes (their `__init` and every override
of 4.3). Every branch is followed, so a closure over-approximates what runs, and engine names are
attributed by name (section 1), so a closure can name a spec the run does not need.

1. **Startup** (script-loading.md 2.2): the engine registers its interface, loads `_G`
   (`scripts\_g.script`), then calls the class registrators `class_registrator.register` and
   `game_registrator.register`, each with the object factory. The top-level code of `_g.script` (outside
   its functions) uses ([closure.py]): `jit` (standard; only when it is nil, `profiler.setup_hook()` is
   called); `time_global` (only when it is nil, a replacement calling `device():time_global()` is
   defined); `editor()`; `class`; and the engine base classes `effector` and `property_evaluator`, whose
   script classes it declares only when `editor()` is false (measured, `scripts\_g.script`). The top-level
   code of `class_registrator.script` calls `prefetch("_G")`. The registrators call
   `object_factory:register` 54 times ([registrators.py]):
   - 51 times with (client class, server class, class id, script class-id name): the client classes are
     48 distinct engine class names given as strings (`CAI_Stalker`, `CHelicopter`, `ce_smart_zone`, …;
     all 48 pass DLL) and one script class (`ce_switcher.ce_switcher`); the 20 distinct server classes are
     all script classes (`se_stalker.se_stalker`, `smart_terrain.se_smart_terrain`, …);
   - once with (client class, class id, script class-id name): `ui_main_menu.main_menu`, `MAIN_MNU`,
     `main_menu`, only when `editor()` is false;
   - twice (multiplayer, only when `editor()` is false) from `game_registrator.register`.
2. **Main menu.** When the menu is activated, the engine creates an object of class id `MAIN_MNU` through
   its class factory (static, [static_newgame.py]); by step 1 that is the script class
   `ui_main_menu.main_menu`, derived from `CUIScriptWnd`, so its `__init` runs (believed). Its window
   overrides and its `AddCallback` handlers are then called by the engine on input (believed; 4.3, 4.4).
   `main_menu:__init` builds the window and shows the version that `GetGSVer` of
   `_G.main_menu.get_main_menu()` returns; the New Game items' handlers execute the console commands
   `g_game_difficulty <difficulty>`, then (after `disconnect` if `alife()` is not nil)
   `start server(all/single/alife/new) client(localhost)` and `main_menu off` (measured,
   `scripts\ui_main_menu.script`).
3. **Game type** (static, [static_newgame.py]): to get the game's class id, the engine reads
   `[common] game_type_clsid_factory` of `config\script.ltx` and calls that function with the game-type
   option (`single`) and a flag that is true for the server side and false for the client side; the
   returned text is converted to a class id, and an empty result stops the game with
   `Unknown game type: %s`. `game_registrator.get_game_clsid` returns `SV_SINGL` and `CL_SINGL` for
   `single` and uses no engine name (measured). A dedicated server uses a fixed table instead and calls no
   script. Which side asks first is **unestablished**.
4. **Start of the simulation** (static, [static_newgame.py]): when the simulation is created, it reads
   `[alife] start_game_callback` of `config\alife.ltx` (`_G.start_game_callback`); if the function cannot
   be found the game stops ("failed to get start game callback"). It then calls it with no arguments,
   ignoring results; a script error in it ends the program. Only after that does it start the new or the
   loaded game (it compares the server option with `new` and `load`). The comment beside the key in the
   data reads "on starting new game or loading saved one". The shipped function calls
   `task_manager.clear_task_manager()`, `treasure_manager.clear_treasure_manager()`,
   `xr_sound.clear_all_sound_object()` and `dialog_manager.fill_phrase_table()` (measured); these clear
   script tables and read `.ltx` data through `ini_file`, and loading those scripts runs their top-level
   code.
5. **Simulation objects** (offline-simulation.md 8.1): every record of `spawns\all.spawn` is created in
   increasing index; 4,127 of the 8,641 records have a class id that step 1 binds to a script server class
   (436 of the 870 records of `l01_escape`, 13 distinct classes), most of them `se_stalker.se_stalker`
   (1,073), `se_zones.se_restrictor` (1,026) and `se_zones.se_zone_anom` (672) (measured,
   [newgame_objects.py]). Their script classes derive from engine simulation classes and override the
   methods of 4.3. Which creation and registration step calls which override (`__init`, `STATE_Read`,
   `UPDATE_Read`, `on_before_register`, `on_register`, `on_spawn`, …) is **unestablished**; it is the
   subject of spec J.
6. **Level objects and binders**: of the 671 objects of `l01_escape` online at the start
   (level-objects.md 2.6), 367 have a section with `script_binding` (measured, [newgame_objects.py]):
   `bind_physic_object.init` 206, `bind_restrictor.bind` 75, `bind_smart_terrain.bind` 41,
   `bind_stalker.init` 32, `bind_respawn.bind` 9, `bind_monster.bind` 2, `bind_stalker.actor_init` 1,
   `bind_heli.bind` 1. When an object's section has the key, the engine looks the function up and calls
   it for the object; if it is not found it logs `function %s is not loaded!` and continues (static,
   [static_newgame.py]). The binding functions attach a script binder with `bind_object` (measured, 4.4).
   The order of the binder overrides (`reinit`, `reload`, `net_spawn`, `update`, …) is **unestablished**;
   it is the subject of spec L. The actor's binder (`bind_stalker.actor_init`) is the level's start-up
   script: `config\script.ltx [single] script` is empty, its comment saying the weather moved to the
   actor's binder (measured).

Closures (measured, [closure.py]); "engine names" are distinct (kind, name) pairs used by the closure's
functions, standard library names excluded; "specs" are those owning a name the closure uses that belongs
to that spec alone, "new" those not needed by an earlier step:

| Step | Closure functions (scripts) | Engine names | New specs |
|---|---|---|---|
| 1 startup | 9 (4) | 16 | A B C F H U W |
| 2 main menu (`main_menu` class and handlers) | 30 (9) | 78 | G1 G2 G3 J K N G4 G5 |
| 3 game type | 1 (1) | 0 | — |
| 4 `start_game_callback` | 30 (9) | 25 | Y |
| 5 the 13 server classes of `l01_escape` | 365 (58) | 254 | E I M O P Q R S T V X Z |
| 6 the 8 binding functions of `l01_escape` | 391 (78) | 273 | L |

Step 1 needs `effector` (U) and `property_evaluator` (W) to exist, because `_g.script` declares script
classes on them; step 2 reaches `alife()` (K), `level.present` (N) and the main-menu object (G5). So every
proposed spec that owns names is reached during a New Game of `l01_escape` (D owns entry points, among them
those of steps 3 and 4). **Disputed in review:** unbacked as a statement about the run: the closures
over-approximate and attribute names by name alone, and some specs are reached only that way (step 5
reaches Z through 1 name; step 2 reaches J only through the listed property names `init` and `online`);
that every spec is needed during a New Game is believed at most. The order of section 6 follows the first step that needs each spec and then the
dependencies.

## 6. Proposed split

**Scope rule.** A spec owns every name of its classes, namespaces and functions as the binary registers
them, listed or not, plus the entry points of 4.5; it establishes those names from the binary, the
listing (2.1) being only a guide. A member belongs to the spec of the class that declares it (2.1); a
derived class's inherited repeats are not owned again by the derived class's spec. The declared members of
`game_object` are split by function as in [specsplit.py].

**Completeness** ([scope.py]): each of 3,260 listed items is assigned to exactly one spec, and the check
asserts it: the 301 listed classes, the 2,843 declared members (2.1) and the 116 namespace and global
functions. Each of the 3,211 inherited repeats resolves to items of its declaring classes, so no member is
owned by two specs (asserted). Also assigned to exactly one spec: the 6 registered classes and namespaces
the listing lacks (`main_menu` and the 5 classes of 2.2) and the 28 unlisted names the scripts use. The 146
further references to strings that registration code pairs with a registration call but that neither the
listing nor a script names (93 distinct strings; candidates, many of them configuration keys) are each
assigned to the spec owning the nearest registration of a declared member of exactly one listed class or
namespace around them, since the binary keeps one class's registration code together (believed); each
owning spec decides which are registrations.

The table gives, per spec, the listed items it owns (as counted above), and the distinct engine names
(kind, name) the loadable scripts use from it with the loadable scripts using them, lower (upper) as in
section 3 ([specsplit.py]). A used name that several declaring classes have is counted in every spec owning
one of them: 55 of the 1,209 used (kind, name) pairs are in 2 to 5 specs, so the column sums to 1,287. The
absent names of 2.2 are assigned to the spec of the class or object the scripts address them through.

| Order | Id | Title | Description | Depends on | Listed items | Names used | Loadable scripts |
|---|---|---|---|---|---|---|---|
| 1 | A | Script classes deriving from engine classes | `class`, `super`, `__init`, script classes with engine bases, how engine calls reach overrides, base-class calls | script-environment, script-loading | 0 | 106 | 134 (134) |
| 2 | B | Engine services | console (`get_console`, `execute`, value getters), `device`, `time_global` of the device, `editor`, `app_ready`, `log`, `prefetch`, `IsGameTypeSingle`, render and thread queries | A | 47 | 16 | 27 (27) |
| 3 | C | Class ids and the object factory | `clsid` values (engine and registered), `object_factory:register` (both forms), creation through the factory, the `class_registrators` call | A | 188 | 103 | 22 (22) |
| 4 | D | Engine entry points and the New Game call order | the entry points of 4.5 assigned to D, and the order of section 5 across specs | A, B, C | 0 | 0 | — |
| 5 | E | Value types and math | `vector`, `matrix`, `fcolor`, `Frect`, `flags16`/`flags32`, colour types, bit functions | A | 116 | 46 | 60 (64) |
| 6 | F | Configuration and files | `ini_file`, `system_ini`, `create_ini_file`, token lists, `getFS`, file lists and their entries (with the unlisted `ModifDigitOnly`) | E, ltx-config | 84 | 37 | 55 (141) |
| 7 | H | Time | `CTime` (in `game`), `game.get_game_time`, `game.time`, `time_global`, `level` time and time factor, `profile_timer` | E, sav | 30 | 21 | 31 (72) |
| 8 | U | Effects and weather | `effector` and its override, post-processing, camera effects, `particles_object`, weather and the 2 unlisted environment classes | A, E (object members refer to M) | 44 | 31 | 17 (33) |
| 9 | W | Action planner | `action_planner`, `action_base`, `property_evaluator`, world states and properties, `stalker_ids`, motivation manager, planner overrides | A, E (object members refer to M) | 176 | 40 | 36 (110) |
| 10 | G1 | Script windows: base, events and keys | `CUIScriptWnd`, `CUIWindow`, dialog windows, `AddCallback`, `ui_events`, window overrides, `DIK_keys`, `key_bindings`, `dik_to_bind` | A, E | 270 | 72 | 22 (22) |
| 11 | G2 | Window controls | statics, buttons, check buttons, edit boxes, progress bars (with the unlisted `SetProgressPos`), lists and list items, tab controls, message boxes, other controls | G1 | 145 | 47 | 19 (19) |
| 12 | G3 | Window description files, fonts and textures | `CScriptXmlInit` (with the unlisted `InitProgressBar`), `GetFont…`, `GetTextureInfo`, `GetARGB`, `game.translate_string` | G2, ui-layout, text-and-fonts | 50 | 38 | 26 (26) |
| 13 | G4 | In-game interface (HUD) | `get_hud`, custom statics and messages, menus over the level, indicators, input on/off, `UIGame_custom_script` overrides | G3 | 34 | 24 | 20 (31) |
| 14 | G5 | Main menu object, options and saved-game windows | namespace `main_menu`, the main-menu object (`GetGSVer`, `GetPatchProgress`, `CancelDownload`, `ValidateCDKey`) and the patch-progress object it returns (`GetInProgress`, `GetProgress`, `GetFlieName`), their 2 unlisted classes, options manager, saved-game information, `valid_saved_game` | G3, sav | 13 | 22 | 4 (7) |
| 15 | I | Data streams | `net_packet` and `reader` reads and writes | E, sav | 81 | 22 | 25 (51) |
| 16 | J | Simulation objects | `cse_…` classes as scripts see them: fields, methods, overrides of 4.3 and when each is called | A, C, I, offline-simulation | 148 | 56 | 25 (118) |
| 17 | K | Simulation registry and game graph | `alife()` and its methods, story objects and story ids (637 of its items are `story_ids` constants), switch distance, `game_graph()`, smart-terrain tasks | J | 713 | 21 | 33 (99) |
| 18 | M | Game object basics | `game_object` identity, place, condition, kind accessors; engine client classes (with the unlisted `CPhysicObject`) and `CGameObject` overrides | A, E, F | 162 | 63 | 62 (139) |
| 19 | N | Level | `level` namespace core: name, presence, objects by id, vertices, patrol paths (`patrol`), client spawn manager and its `add` | M | 41 | 30 | 91 (137) |
| 20 | Y | Physics, vehicles, helicopter and lamps | physics shell, element and joint; car, helicopter, holder, hanging lamp | M | 139 | 59 | 15 (15) |
| 21 | O | Object event callbacks | `callback` ids, `set_callback`, `set_fastcall`, other function-taking setters, `level.add_call`, `level.remove_call` | M | 42 | 30 | 20 (20) |
| 22 | P | Info portions, tasks, news and map spots | info portion members, `CGameTask`, `SGameTaskObjective`, `task`, news and tips, `level.map_…`, tutorials, task-file functions | M, F | 54 | 42 | 71 (73) |
| 23 | Q | Relations, community and ranking | `relation_registry`, `actor_stats`, community, rank, reputation, goodwill and relation members (with the unlisted `set_community_goodwill`, `community_goodwill`) | M | 27 | 24 | 31 (32) |
| 24 | R | Dialogs and trade | phrase classes, dialog start and talk members, trade members and conditions, dialog-file functions | M, P | 36 | 16 | 17 (17) |
| 25 | S | Inventory | items, slots, money, transfer, iteration, ammunition | M | 29 | 21 | 26 (54) |
| 26 | T | Sound | `sound_object`, `snd_type`, object sound members, `level.iterate_sounds` | M | 93 | 47 | 22 (52) |
| 27 | V | Scripted actions | `entity_action` and its parts (`move`, `look`, `anim`, `sound`, `particle`, `object`, `cond`, `act`), action queue, script control | M | 147 | 65 | 30 (106) |
| 28 | X | Movement, sight and memory | character movement, path, sight, mental and body state, enemies, danger, memory, restrictions | M, N | 160 | 85 | 59 (121) |
| 29 | Z | Multiplayer | game-type classes, player state and messages, multiplayer overrides | all above | 176 | 97 | 15 (52) |
| 30 | L | Object binders | `object_binder`, `bind_object`, `script_binding`, binder overrides and their order | A, D, M | 15 | 6 | 13 (94) |

The specs with the most listed items (K, G1, W, Z, M, X, J, V, G2; K's are mostly story-id constants) or
used names (A, C, Z, X) may split further when investigated; the counts above are the measure for that
decision.

## 7. Verification facts

All measured unless marked; `run_all.py` reproduces every number in this document.

- Files: 441 under `scripts\` (440 `.script`), 439 scanned; 893 of the 899 files of `config\` and 195
  further files scanned for function names; `xrGame.dll` 6,246,400 bytes [extra.py]. Loadable 250
  (script-loading.md section 3) [run_all.py], [scan.py].
- Listing: 301 classes, 116 distinct free functions [xcheck.py]; 6,054 class members, 2,843 declared,
  3,211 inherited repeats [decl.py]; 305 class registrations in the binary (static) [xcheck.py], of them
  5 classes the listing lacks and 2 listed constant names, and the unlisted namespace `main_menu`
  [unlisted_evidence.py].
- Story ids: 637 listed, 636 equal to `config\game_story_ids.ltx` entries [datadriven.py].
- Class registrators: 54 registrations, 54 class ids, 20 distinct script server classes, 48 engine client
  class names [registrators.py].
- Used names: 1,107 distinct listed names in loadable scripts [report.py]; 28 unlisted, 69 absent, 80 bare
  names never defined, 16 of them passing REG or DLL [classify.py], [unlisted_evidence.py]; 1,209 (kind,
  name) pairs in section 6, 55 in several specs [specsplit.py].
- Script functions named in data 234 [cfgrefs.py]; named in the binaries 13, all in `xrGame.dll`
  [binnamed.py]; members taking a function 11, taking a function name 8, each with one spec
  [entrypoints.py]; script classes with an engine base 249 of 355 [overrides.py].
- Completeness: 3,260 listed items each owned by one spec, 3,211 inherited repeats resolved to them; 146
  candidate references (93 strings) assigned [scope.py].
- New Game: 4,127 records with script server classes, 436 on `l01_escape`; 367 of its 671 online objects
  with `script_binding` [newgame_objects.py]; closures of section 5 [closure.py].
- Static facts of section 5 steps 2–4 and 6 and of 4.1: 8 PASS lines [static_newgame.py].

## 8. Unestablished

- Exact signatures, value ranges and behaviour of every name (the subject of the proposed specs).
- For the unlisted names of 2.2 without a class in "Paired with": whether and on which class the binary
  registers them; for `level.actor` and `level.object`, whether `level` has them.
- Whether the engine defines any of the 16 never-defined bare names of 2.2 that pass REG or DLL as a
  global.
- Properties the listing lacks: the scan cannot find them (section 1), so their number is unknown.
- Which listed classes are registered inside a namespace, other than `CTime` in `game`.
- The source of one listed constant (2.3).
- Which of the 93 candidate strings of section 6 are registrations.
- Which simulation step calls which simulation-object override, and the order of the binder overrides
  (section 5 steps 5–6).
- When the engine calls the 13 functions of 4.2, the `[alife]` sleep callbacks, and the window overrides
  and handlers of the main menu (step 2, believed).
- Which side calls the game-type function first (step 3).
- The scan and the closures are static: a name used in a script is not necessarily reached at run time;
  names reached dynamically (built from strings, through tables of functions) are not counted, and names
  are attributed to classes by name.

## Provenance

- Date: 2026-11-24 (revised after the second review the same day).
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` as identified
  above; the archive file tree of `gamedata.db0` … `gamedata.dbd` listed and extracted with the project's
  archive tool; `spawns\all.spawn` records as decoded for the approved level-objects and
  offline-simulation specs.
- Analysis: a throwaway Lua tokenizer, scope tracker and call-closure builder over the 439 scripts; a
  parser of `scripts\lua_help.script`; scans of the configuration files; string searches of the files
  under `bin\`; static analysis of `xrGame.dll` with Ghidra (collecting the string references and
  registration calls of the code that calls the binding library's registration entry points, and reading
  the code that references the strings `start_game_callback`, `game_type_clsid_factory`, `script_binding`
  and `MAIN_MNU`). No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

Gaps don't block this scoping spec; each is listed for the user and the orchestrator to judge. Those
addressed in this revision say so.

1. Listed members that take a script function and were owned by no entry-point row:
   `client_spawn_manager:add` (used only by a non-loadable script), `level.iterate_sounds` (used only by
   `sound_prefetch.script`, non-loadable), and the string forms (object, method name, method name) of
   `level.add_call` and `level.remove_call`. Addressed: 4.4 now counts every listed member taking a
   function or a function name, and 4.5 gives each one spec (N, T, O; the task-objective string members
   to P), asserted by [entrypoints.py].
2. Engine properties the listing lacks cannot be found by the scan (a field read counts only when its
   name is a listed property), so the unlisted set covers methods, namespace and class members but not
   properties. Addressed: stated in section 1 and section 8; finding them is left to the owning specs.
3. Section 4.2 searches the binaries only for `namespace.function` strings. Functions the engine might
   look up by a bare global name are not searched: 8 functions that `_g.script` defines (`abort`,
   `action`, `printf`, `round`, `script_object`, `start_game_callback`, `switch_offline`,
   `switch_online`) also occur in `xrGame.dll` as bare zero-terminated strings (measured); apart from
   `start_game_callback` (4.1), whether the engine calls any of them is unestablished and belongs to D.
