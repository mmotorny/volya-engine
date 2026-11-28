# Script usage: what the shipped scripts use

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of what the shipped
scripts need from the runtime that runs them: the language features, the standard-library functions and the
global names the loadable game scripts use; the global names no script defines (the names the engine has to
provide, or that are nil); and whether the scripts yield, from where, and across what. The shader scripts,
which run in the other Lua state, are covered in section 6. Nothing here says what an engine-provided name
does.

Builds on (not repeated): `docs/behavior/script-environment.md` (cited as `env`: the runtime, the two states,
the libraries each opens and their name tables, 2.1), `docs/behavior/script-loading.md` (cited as `loading`:
the chunk text the game builds, 2.3 and 1.5; namespaces and automatic loading, 2.2; the 250 loadable files,
section 3). "SE-1" … "SE-9" are the proposals of `docs/behavior/narrowed-specs-scope.md` 5.3 that cover what
this specification leaves open.

Conventions:

- **The loadable scripts** are the 250 files of `loading` section 3. Each is examined as the chunk the game
  builds for it: the default prefix of `loading` 2.3 (none for `_g.script`), then the file with the
  transformation of `loading` 1.5. `-_g` (the other prefix) is not examined.
- A script is named by its file name without `.script` (`_g` is `scripts\_g.script`). Line numbers are the
  runtime's (`loading` 1.4); every line cited here is also the file's physical line, as none lies after a
  block comment (`loading` 4.2) [C1].
- **Reads a global** means the compiled chunk looks the name up in its environment (`x`, `x.y`, `x:y()` where
  `x` is not a local), after the prefix: the prefix's own reads (`setmetatable`, `setfenv`, `_G`, in every
  prefixed chunk) are not counted. `_G.x` counts as a read of `x` from the globals.
- **"The public release"** is the public LuaJIT 1.0.3 source distribution (a third-party library, comparison
  material); its compiler was used to compile the chunks (nothing was run).
- Words in code font (`wait`, `xr_logic`, `level`, …) are the scripts' own names, as the game data spells
  them. One of `_g`'s names in 4.2 begins with `local` and an underscore; spec-lint reports it as a
  decompiler-style name, which it is not.
- Statements are **measured** (counted from the data, or read from file bytes and PE tables), **static**
  (derived from the compiled chunks, or from the shipped binaries by static analysis, without running them),
  **documented** (public operating-system documentation) or **unestablished**; inferences are marked
  **believed**. Nothing was run.
- Bracketed ids are the checks reproducing a statement: `[C1]` … `[C9]` are `check_1_chunks.py` …
  `check_9_computed.py` in the dirty workspace's `script-usage/` (`python3 -I run_all.py` runs them all; each
  asserts its numbers and ends with `OK`).

## 1. Material

- The 250 loadable chunks all compile under the public release; the hashes of the chunks of `_g`, `a1`,
  `bind_stalker`, `game_last_dialog`, `h` and `xr_rest` equal those of `loading` 4.3, so the chunks examined
  are the ones `loading` specifies (measured) [C1].
- The 69 shader scripts (35 in `shaders\r1\`, 34 in `shaders\r2\`) are compiled as their files after the
  transformation of `loading` 1.5, with no prefix (`loading` 4.4: how the shader state prefixes them is not
  covered) [C1, C6].
- Counts come from two readings of each chunk: its tokens (keywords, operators, literals) and its compiled
  form (global reads and writes, field reads with constant keys, calls, loops, function prototypes). Calls are
  attributed to a callee by name only where the callee is a global, a field with a constant name of a global,
  a local or an upvalue; everything else is a **computed** access (static) [C2–C5].

## 2. Language features (loadable game scripts)

### 2.1 Lexical (measured) [C2]

| Feature | Use |
|---|---|
| Keywords | all except `repeat` and `until` (never used) |
| Operators | all of Lua 5.1 except `#` (length; never used). `%` once (`xr_conditions` line 645), `^` five times (`heli_combat` lines 34 and 43, `sr_psy_antenna` line 110), `..` 4,986 times in 47 files, `...` 28 times in 6 files |
| `;` | 122 times in 25 files |
| Numerals | decimal only: 11,931 integers, 260 with a fraction; no hexadecimal numeral and no exponent |
| Quoted strings | `"…"` 17,577 in 243 files; `'…'` 10 in 7 files |
| Escapes in quoted strings | only `\n` (1,667, in 8 files), `\\` (451, in 34 files) and `\"` (24, in 4 files): no decimal escape, no escaped line break, no other escape |
| Long strings | 226, in 7 files, all `[[ … ]]` (no `=` level); none contains `[[` |
| Comments | `--` comments 6,148 in 203 files; `--[[ … ]]` long comments 82 in 48 files, all without `=` level (C-style comments: `loading` 4.2) |
| Function definitions | 3,498 (`function` keyword): 1,510 `function a:b(…)` method definitions, 1,560 other named definitions (of which 13 `local function`), 428 anonymous function expressions |

### 2.2 Constructs (static) [C2]

- Every function the files write compiles to one closure: 3,498. 397 of them use upvalues (variables of an
  enclosing function); upvalues are assigned 1,126 times.
- Calls: 7,063 method calls (`a:b(…)`), 188 tail calls (`return f(…)`).
- Loops: 240 numeric `for`; 298 generic `for`, 55 `while`, 31 `break`. The generic `for` iterator
  expressions: `pairs(…)` 257, `string.gfind(…)` 22, `ipairs(…)` 17, and **2 that are not calls**: a table
  expression in the Lua 5.0 form `for k, v in t do` (`enemy` line 2, `for k,v in db.obj_by_id do`;
  `game_last_sv` line 292, `for wpn_it in self.default_items do`). In Lua 5.1 the value after `in` is called as
  the iterator; what the embedded runtime does with a table there is **unestablished** (SE-8's comparison of
  the virtual machine).
- Variable arguments: 15 functions declare `...`; 13 use `...` in an expression (`{...}`, passing it on).
  The other 2 use the Lua 5.0 implicit table `arg` instead: `anomaly` (function at line 49, `arg` read at
  lines 53 and 54) and `utils` (function at line 390, lines 394 and 395). In the public release, whose
  configuration leaves the 5.0 vararg compatibility off, `arg` there is a global read. `xrLUA.dll` contains no
  NUL-delimited string `arg` (measured), so the embedded parser is **believed** not to declare the implicit
  `arg` local either (SE-3 compares the parser); `arg` is then a global no script defines (section 4.5).
- Not used anywhere: the length operator, `repeat … until`.
- The compiled chunks use 36 of the public release's 38 instruction kinds: all but the length operator's and
  the one that closes captured variables at the end of an inner block, so no function captures a local
  variable declared inside a loop or another inner block (**believed**, from the public compiler).

## 3. Standard library (loadable game scripts) [C3]

### 3.1 Base functions and library tables read

Global reads of runtime names (reads, files) (measured):

| Name | Reads | Files |
|---|---|---|
| `table` | 1,407 | 57 |
| `pairs` | 257 | 64 |
| `math` | 242 | 52 |
| `string` | 165 | 30 |
| `tostring` | 153 | 31 |
| `tonumber` | 75 | 29 |
| `type` | 37 | 12 (in `dream`, its 5 reads are of `dream`'s own `type`; 32 reads in 11 files are the base function) |
| `ipairs` | 17 | 3 |
| `_G` | 15 | 7 (section 4.4) |
| `coroutine` | 4 | 1 (`_g`; section 5) |
| `assert` | 3 | 2 (`_g`, `profiler`) |
| `unpack` | 2 | 1 (`sr_territory`) |
| `_VERSION` | 2 | 1 (`version`) |
| `jit` | 1 | 1 (`_g` line 1, compared with nil; section 4.6) |

Never read: `collectgarbage`, `dofile`, `error`, `gcinfo`, `getfenv`, `getmetatable`, `loadfile`, `load`,
`loadstring`, `next`, `pcall`, `print`, `rawequal`, `rawget`, `rawset`, `select`, `setfenv` and
`setmetatable` (both only in the prefix), `xpcall`. No script compiles text at run time, changes an
environment, or calls a function in protected mode. Five namespaces define their own `load` (none reads it).

`pairs` and `ipairs` are not in `env` 2.1's base name table; both occur in `xrLUA.dll` as NUL-delimited
strings (measured); the public release's base library sets
exactly these two outside its name table, so they are **believed** present (SE-1 settles the fields outside
the name tables).

### 3.2 Library fields read

All with constant names: no library field is read with a computed key, no library table is stored, passed
or returned as a value (a local holding one counts as the table itself), and no script writes a field of a
library table (static):

| Library | Field: reads (files) |
|---|---|
| `table` | `insert` 1,298 (44), `getn` 90 (29), `remove` 10 (5), `sort` 7 (4), `foreach` 2 (1) |
| `math` | `random` 164 (39), `pi` 12 (6), `pow` 12 (3), `floor` 8 (2), `sqrt` 8 (2), `mod` 7 (3), `abs` 6 (3), `acos` 5 (3), `cos` 4 (4), `min` 4 (4), `sin` 4 (4), `log10` 3 (1), `ceil` 2 (2), `max` 1 (1), `randomseed` 1 (1), `tan` 1 (1) |
| `string` | `sub` 46 (14), `find` 40 (13), `len` 27 (8), `format` 24 (11), `gfind` 22 (10), `char` 4 (1), `byte` 1 (1), `gsub` 1 (1) |
| `coroutine` | `yield` 4 (1) |

- `math.pi` (`ph_camera`, `ph_car`, `ph_gate`, `sr_sound`, `utils`, `xr_combat_zombied`) and `math.mod` (`_g`,
  `xr_kamp`, `xr_wounded`) are not in `env` 2.1's `math` name table. `pi`, `huge` and `mod` occur
  in `xrLUA.dll` as NUL-delimited strings (measured); in the public
  release the `math` library sets exactly these three outside its name table (`mod` as `fmod`, under its 5.0
  compatibility switch). So both are **believed** present (SE-1 settles it).
- Lua 5.0 names in use: `table.getn`, `table.foreach`, `string.gfind`, `math.mod` (all of them in `env` 2.1
  except `math.mod`, above).
- Library functions that call back into script functions: `table.sort` is called with a comparison function at
  all 7 sites (written in place at 6; `xr_conditions_impl` line 34 passes its own `dist_less`);
  `table.foreach` with a function written in place at both sites (`_g` lines 195 and 222). `string.gsub` is
  called once, with a string replacement (`utils` line 425).

### 3.3 String methods

The only method calls named after a `string` function are 39 calls `:sub(…)` in 20 files; none is applied to a
string literal, and whether any receiver is a string is **unestablished** statically.

## 4. Global names (loadable game scripts) [C4]

### 4.1 How a read is attributed

The 250 chunks read 904 distinct global names, 15,004 times (static). By `loading` 2.3 and 2.2, a read in a
namespaced chunk finds, in this order: a field of its own namespace; a value in the globals; a script of that
name loaded by the globals' automatic loading. Each read is attributed accordingly, per reading file:

1. **own**: the reading file assigns the name as a global of its own (`function f() … end`, `x = …`, or
   `this.x = …`); `_g`'s own names are the globals themselves, so this applies to the other files only;
2. **defined in the globals by a script**: assigned by `_g` (whose chunk has no prefix and so writes the
   globals), assigned as `_G.x = …` by any script, or declared as a class by `class "x"`. Where the binding
   library puts a class name (the globals or the declaring chunk's environment) is SE-9's subject; a class
   name is counted here wherever it is read;
3. **runtime**: a name of `env` 2.1 (base names, the library tables, `_G`, `_VERSION`) or `pairs` / `ipairs`
   (3.1); these are values in the globals, so `_G` counts here although `_g.script` matches it by name;
4. **namespace**: a file in `scripts\` has that name (case-insensitive, `loading` 2.1);
5. **none**: none of these; the name must come from the engine, or is nil.

A name can be attributed differently in different files. Distinct names per attribution: own 305, defined in
the globals by a script 312 (270 of them class names), runtime 14 (the names of 3.1), namespace 129, none 153
(static).

### 4.2 Names scripts define in the globals

- `_g` assigns 79 global names (static): `GiveInfoViaPda` `IsMonster` `IsStalker` `abort` `action`
  `action_first` `ammo_section` `clear_table` `day_time` `debug_get_actor` `debug_get_level_object`
  `debug_log` `debug_script_name` `distance_between` `distance_between_safe` `get_actor_id` `get_clsid`
  `get_texture_info` `has_alife_info` `id_by_sid` `if_then_else` `interrupt_action` `isWeapon`
  `is_object_monster` `is_object_online` `last_action` `level_object_by_sid` `load_scheme` `local_hours`
  `near_point_with_bit` `new_action` `new_point` `obj_last_actions` `object_type` `odd` `on_actor_bleeding`
  `on_actor_cant_walk_weight` `on_actor_critical_max_power` `on_actor_critical_power` `on_actor_psy`
  `on_actor_radiation` `on_actor_satiety` `on_actor_weapon_jammed` `parse_key_value` `parse_names`
  `parse_nums` `perform_action` `point_with_bit` `point_with_bit_id` `print_table` `printf` `quest_section`
  `random_choice` `random_number` `remove_postprocess` `reset_action` `round` `schemes` `set_actor_rank`
  `set_current_time` `set_inactivate_input_time` `set_postprocess` `set_sleep_relocate`
  `start_game_callback` `stop_play_sound` `str_split` `stypes` `switch_offline` `switch_online`
  `time_global` `time_infinite` `update_action` `vector_cross` `vector_rotate_y` `wait` `wait_game` `yaw`
  `yaw_degree` `yaw_degree3d`.
- `_G.g_start_avi = …` is the only assignment through `_G` (`bind_stalker` line 318).
- `class "…"` is called 278 times in 134 files, declaring 270 distinct names; 7 names are declared in more
  than one file: `action_idle` (`ph_idle`, `sr_idle`), `PPEffector` (`postprocess`, `sr_aes_deadzone`,
  `sr_postprocess`), `action_postprocess` (`sr_aes_deadzone`, `sr_postprocess`), `body_state`
  (`state_mgr_animstate`, `xr_state`), `action_wait` (`xr_attendant`, `xr_kamp`), `action_patrol`
  (`xr_camper`, `xr_patrol`), `action_shoot` (`xr_combat_camper`, `xr_heli_hunter`). `_g` declares 7: `FSM`
  (line 144), `script_object` (271), and the five of 4.6.
- Names a namespace defines for itself that are also runtime or globals' names (each shadows the other only
  inside that file): `type` (`dream`), `load` (5 files), `printf` (15 files), `get_actor` (`i1`, `i2`),
  `debug` (`test_utils`), and others (the check lists them).

### 4.3 Runtime names

The runtime names read are those of 3.1: `table`, `pairs`, `math`, `string`, `tostring`, `tonumber`, `type`,
`ipairs`, `_G`, `coroutine`, `assert`, `unpack`, `_VERSION`, `jit`.

### 4.4 Access through `_G` and computed names

- `_G.x` reads: `_G.abort` (8 reads in `mob_camp`, `mob_home`, `xr_camper`; `_g`'s `abort`), `_G.alife` and
  `_G.IsGameTypeSingle` (`se_zones`, each compared with nil), `_G.main_menu` (`ui_main_menu`; a class it
  declares) (static).
- Reads with a computed name, all in `xr_logic` (static): `_G[…]` at lines 368, 371 (a local `filename`) and
  457 (an entry of the table `schemes`, filled by `_g`'s `load_scheme`); fields of the namespaces
  `xr_conditions` (lines 529, 535, 547) and `xr_effects` (lines 586, 591, 593) named by the field `func` of a
  local table `infop`. Through `_G` they can trigger automatic loading (`loading` 2.2.3); a field the namespace
  `xr_conditions` or `xr_effects` lacks is looked up in the globals (`loading` 2.3).
- No other read with a computed name is made of the globals, of a namespace, or of a chunk's own `this`
  (static).
- The names the data supplies (measured) [C9]:
  - `_G[…]` at all three places reads a value of the table `schemes`, which only the 66 `load_scheme` calls of
    `modules` fill (no other loadable script calls `load_scheme`); their first arguments name 65 distinct
    files (`xr_meet` twice), all loadable scripts and each defining `set_scheme` or `add_to_binder`:
    `fx_sound` `heli_move` `mob_camp` `mob_combat` `mob_death` `mob_eluder` `mob_fake_death` `mob_home`
    `mob_jump` `mob_kicker` `mob_panic` `mob_remark` `mob_trade` `mob_trader` `mob_walker` `ph_appforce`
    `ph_button` `ph_camera` `ph_car` `ph_code` `ph_death` `ph_door` `ph_gate` `ph_hit` `ph_idle` `ph_impulse`
    `ph_on_hit` `ph_oscillate` `ph_sound` `sr_aes_deadzone` `sr_cutscene` `sr_danger` `sr_idle` `sr_light`
    `sr_mapspot` `sr_no_weapon` `sr_particle` `sr_postprocess` `sr_psy_antenna` `sr_sleep` `sr_sound`
    `sr_sound2d` `sr_teleport` `sr_territory` `sr_timer` `sr_tip` `xr_abuse` `xr_attendant` `xr_camper`
    `xr_combat` `xr_combat_ignore` `xr_danger` `xr_death` `xr_heli_hunter` `xr_hit` `xr_kamp` `xr_meet`
    `xr_patrol` `xr_remark` `xr_rest` `xr_sleeper` `xr_test` `xr_use` `xr_walker` `xr_wounded`.
  - The condition and effect names come from condition lists: blocks `{…}` (conditions) and `%…%` (effects)
    whose items are all `+name`, `-name`, `=name` or `!name`, optionally with a parenthesised argument; `=` and
    `!` name a function. Searched in the configuration tree (893 files), the other data files `loading` section 3
    examines (the level and spawn files among them) and the 250 loadable scripts' text (a line's text after `;` is a comment, as is a script string
    starting with `;`): 88 distinct condition names and 142 effect names. `xr_conditions` defines all but 2
    condition names (`cit_fighting_hunt_killer`, `cit_fighting_hunt_victim`); `xr_effects` defines all but 23
    effect names (`arena_release`, `bar_arena_actor_afraid`, `bar_arena_actor_dead`, `bar_arena_add_ammo`,
    `bar_arena_after_fight`, `bar_arena_check_lose`, `bar_arena_fight_10` … `bar_arena_fight_90` in steps of
    10, `bar_arena_fight_begin`, `bar_arena_introduce`, `esc_wolf_thanks`, `kishka_heli_mapspot`,
    `test_hit_in_actor_dir`, `val_bs_victim_dead`, `val_suicide_kill_self`, `val_suicide_stalker_died`).
    None of these 25 is defined in the globals by a script, is a runtime name or names a script, and none
    occurs as a NUL-terminated string in `xrGame.dll`; so each finds nothing in the globals either (nil,
    unless the engine provides it: **believed** not). What `xr_logic` does with a nil there is outside this
    specification.
  - These blocks are found by pattern, not by following `xr_logic`'s parsing of each field, so a block in a
    field that is never read as a condition list is counted too (**believed** harmless to the lists above).

### 4.5 Global names no script defines

153 names are read where no script defines them (attribution "none" in 4.1, counting the reads `_G.alife` and
`_G.IsGameTypeSingle` too) (static). They are listed with the number of reads and of reading files, whether the
name occurs in the binaries as a string (the bytes of the name, preceded by a byte that cannot be part of a
name and followed by NUL; measured) and, where a script defines the name only in its own namespace, which.
The string column is a hint only: the binding of names is the subject of the script interface
specifications, and a missing string does not prove a name is nil (the test misses a name that occurs in a
binary only embedded in a longer string, as `action_base` does in `xrGame.dll`).

| Name | Reads | Files | String in | Defined only in the namespace of |
|---|---|---|---|---|
| `act` | 1 | 1 | xrGame.dll |  |
| `action_base` | 204 | 23 | neither |  |
| `actor` | 1 | 1 | xrGame.dll | db |
| `actor_stats` | 13 | 5 | xrGame.dll |  |
| `alife` | 97 | 32 | xrGame.dll | xr_actions_id |
| `anim` | 134 | 12 | xrGame.dll |  |
| `app_ready` | 1 | 1 | xrGame.dll |  |
| `arg` | 4 | 2 | neither |  |
| `barman_need_kill_veterans` | 1 | 1 | neither |  |
| `bit_and` | 19 | 2 | xrGame.dll |  |
| `bit_or` | 3 | 1 | xrGame.dll |  |
| `CALifeSmartTerrainTask` | 2 | 1 | xrGame.dll |  |
| `callback` | 55 | 12 | xrGame.dll |  |
| `CCar` | 7 | 1 | xrGame.dll |  |
| `CGameFont` | 17 | 6 | xrGame.dll |  |
| `CGameObject` | 7 | 1 | xrGame.dll |  |
| `CGameTask` | 1 | 1 | xrGame.dll |  |
| `CHelicopter` | 1 | 1 | xrGame.dll |  |
| `class` | 278 | 134 | xrGame.dll |  |
| `clsid` | 196 | 20 | xrGame.dll |  |
| `color` | 18 | 4 | xrGame.dll |  |
| `cond` | 98 | 14 | xrGame.dll |  |
| `COptionsManager` | 11 | 2 | xrGame.dll |  |
| `create_ini_file` | 1 | 1 | xrGame.dll |  |
| `CSavedGameWrapper` | 1 | 1 | xrGame.dll |  |
| `CScriptXmlInit` | 6 | 6 | xrGame.dll |  |
| `CSE_AbstractVisual` | 6 | 1 | xrGame.dll |  |
| `cse_alife_car` | 3 | 1 | xrGame.dll |  |
| `cse_alife_helicopter` | 1 | 1 | xrGame.dll |  |
| `cse_alife_human_stalker` | 10 | 1 | xrGame.dll |  |
| `cse_alife_item` | 2 | 1 | xrGame.dll |  |
| `cse_alife_item_artefact` | 3 | 1 | xrGame.dll |  |
| `cse_alife_item_custom_outfit` | 2 | 1 | xrGame.dll |  |
| `cse_alife_item_torch` | 2 | 1 | xrGame.dll |  |
| `cse_alife_item_weapon` | 2 | 1 | xrGame.dll |  |
| `cse_alife_item_weapon_magazined` | 2 | 1 | xrGame.dll |  |
| `cse_alife_item_weapon_magazined_w_gl` | 2 | 1 | xrGame.dll |  |
| `cse_alife_item_weapon_shotgun` | 2 | 1 | xrGame.dll |  |
| `cse_alife_monster_base` | 10 | 1 | xrGame.dll |  |
| `cse_alife_object_physic` | 1 | 1 | xrGame.dll |  |
| `cse_alife_smart_zone` | 11 | 2 | xrGame.dll |  |
| `cse_alife_space_restrictor` | 1 | 1 | xrGame.dll |  |
| `cse_alife_trader` | 1 | 1 | xrGame.dll |  |
| `cse_anomalous_zone` | 5 | 1 | xrGame.dll |  |
| `cse_zone_visual` | 5 | 1 | xrGame.dll |  |
| `CUI3tButton` | 3 | 1 | xrGame.dll |  |
| `CUIButton` | 7 | 3 | xrGame.dll |  |
| `CUICheckButton` | 1 | 1 | xrGame.dll |  |
| `CUIDMFragList` | 1 | 1 | neither |  |
| `CUIDMPlayerList` | 1 | 1 | neither |  |
| `CUIEditBox` | 1 | 1 | xrGame.dll |  |
| `CUIFrameWindow` | 1 | 1 | xrGame.dll |  |
| `CUIInventoryWnd` | 1 | 1 | neither |  |
| `CUIListItemEx` | 3 | 3 | xrGame.dll |  |
| `CUIListWnd` | 2 | 2 | xrGame.dll |  |
| `CUIMessageBox` | 1 | 1 | xrGame.dll |  |
| `CUIMessageBoxEx` | 5 | 5 | xrGame.dll |  |
| `CUIRadioButton` | 3 | 1 | neither |  |
| `CUIRadioGroup` | 1 | 1 | neither |  |
| `CUIScriptWnd` | 20 | 11 | xrGame.dll |  |
| `CUIStatic` | 20 | 8 | xrGame.dll |  |
| `CUIWindow` | 10 | 9 | xrGame.dll |  |
| `danger_object` | 44 | 3 | xrGame.dll |  |
| `death` | 2 | 1 | xrGame.dll | xr_actions_id xr_evaluators_id |
| `debug` | 7 | 1 | xrLUA.dll | test_utils |
| `device` | 27 | 10 | xrGame.dll |  |
| `DIK_keys` | 30 | 4 | xrGame.dll |  |
| `dik_to_bind` | 6 | 6 | xrGame.dll |  |
| `duality` | 5 | 3 | xrGame.dll |  |
| `editor` | 9 | 7 | xrGame.dll |  |
| `eee` | 1 | 1 | neither |  |
| `effector` | 10 | 4 | xrGame.dll |  |
| `effector_params` | 3 | 3 | xrGame.dll |  |
| `entity_action` | 10 | 3 | xrGame.dll |  |
| `field` | 2 | 1 | xrGame.dll |  |
| `flags32` | 1 | 1 | xrGame.dll |  |
| `Frect` | 1 | 1 | xrGame.dll |  |
| `FS` | 12 | 3 | xrGame.dll |  |
| `game` | 139 | 27 | xrGame.dll |  |
| `game_cl_mp_script` | 7 | 1 | xrGame.dll |  |
| `game_graph` | 10 | 4 | xrGame.dll |  |
| `game_messages` | 13 | 2 | xrGame.dll |  |
| `game_object` | 57 | 21 | xrGame.dll |  |
| `game_phases` | 13 | 2 | xrGame.dll |  |
| `game_player_flags` | 9 | 2 | xrGame.dll |  |
| `game_PlayerState` | 4 | 1 | xrGame.dll |  |
| `game_sv_mp_script` | 6 | 1 | xrGame.dll |  |
| `GAME_TYPE` | 4 | 2 | xrGame.dll |  |
| `get_actor` | 1 | 1 | neither | i1 i2 |
| `get_console` | 29 | 9 | xrGame.dll |  |
| `get_hud` | 5 | 4 | xrGame.dll |  |
| `get_level_object` | 9 | 7 | neither |  |
| `GetARGB` | 8 | 3 | xrGame.dll |  |
| `GetFontDI` | 2 | 1 | xrGame.dll |  |
| `GetFontLetterica16Russian` | 3 | 3 | xrGame.dll |  |
| `GetFontLetterica18Russian` | 3 | 3 | xrGame.dll |  |
| `GetFontLetterica25` | 4 | 1 | xrGame.dll |  |
| `GetFontMedium` | 13 | 10 | xrGame.dll |  |
| `getFS` | 7 | 3 | xrGame.dll |  |
| `GetTextureInfo` | 1 | 1 | xrGame.dll |  |
| `goodwill` | 1 | 1 | xrGame.dll |  |
| `hit` | 46 | 12 | xrGame.dll |  |
| `ini_file` | 22 | 18 | xrGame.dll |  |
| `IsGameTypeSingle` | 2 | 1 | xrGame.dll |  |
| `key_bindings` | 11 | 4 | xrGame.dll |  |
| `level` | 302 | 72 | xrGame.dll |  |
| `log` | 3 | 2 | xrGame.dll |  |
| `look` | 23 | 7 | xrGame.dll |  |
| `MonsterSpace` | 1 | 1 | xrGame.dll |  |
| `move` | 285 | 19 | xrGame.dll |  |
| `net_packet` | 4 | 2 | xrGame.dll |  |
| `noise` | 12 | 4 | xrGame.dll |  |
| `npc` | 4 | 3 | xrGame.dll |  |
| `obj` | 1 | 1 | xrGame.dll | ph_car |
| `obj_id` | 1 | 1 | neither |  |
| `object` | 42 | 5 | xrGame.dll |  |
| `object_binder` | 61 | 10 | xrGame.dll |  |
| `object_id_list` | 1 | 1 | neither |  |
| `packer` | 1 | 1 | neither |  |
| `particles_object` | 4 | 3 | xrGame.dll |  |
| `patrol` | 116 | 35 | xrGame.dll |  |
| `prefetch` | 1 | 1 | xrGame.dll |  |
| `profile_timer` | 5 | 1 | xrGame.dll |  |
| `properties_helper` | 1 | 1 | neither |  |
| `property_evaluator` | 86 | 26 | xrGame.dll |  |
| `property_evaluator_const` | 1 | 1 | xrGame.dll |  |
| `relation_registry` | 6 | 4 | xrGame.dll |  |
| `RPoint` | 1 | 1 | xrGame.dll |  |
| `section` | 2 | 1 | xrGame.dll |  |
| `SGameTaskObjective` | 2 | 1 | xrGame.dll |  |
| `snd` | 2 | 2 | neither |  |
| `snd_type` | 39 | 2 | xrGame.dll |  |
| `sound` | 10 | 3 | xrGame.dll | db |
| `sound_object` | 37 | 9 | xrGame.dll |  |
| `SServerFilters` | 1 | 1 | xrGame.dll |  |
| `st` | 1 | 1 | neither |  |
| `stalker_ids` | 109 | 23 | xrGame.dll |  |
| `state_inside` | 2 | 2 | neither |  |
| `super` | 200 | 73 | xrLUA.dll |  |
| `system_ini` | 19 | 11 | xrGame.dll |  |
| `SZoneMapEntityData` | 1 | 1 | xrGame.dll |  |
| `task` | 21 | 6 | xrGame.dll |  |
| `ui_events` | 101 | 10 | xrGame.dll |  |
| `UIGame_custom_script` | 3 | 1 | xrGame.dll |  |
| `valid_saved_game` | 2 | 1 | xrGame.dll |  |
| `vector` | 120 | 27 | xrGame.dll |  |
| `verify_if_thread_is_running` | 2 | 1 | xrGame.dll |  |
| `victim` | 1 | 1 | neither |  |
| `world_property` | 370 | 19 | xrGame.dll |  |
| `world_state` | 1 | 1 | xrGame.dll |  |
| `xr_position` | 1 | 1 | neither |  |
| `xrRender_test_r2_hw` | 1 | 1 | xrGame.dll |  |
| `zone_id` | 1 | 1 | neither |  |

### 4.6 What `_g` does only under conditions

Three statements of `_g`'s top level depend on what exists when it runs (`loading` 2.2.1: at startup, after the
engine has registered its script classes) (static) [C4]:

- Line 1: `if (jit == nil) then profiler.setup_hook () end`. The `jit` library is absent only with `-nojit`
  (`env` 2.1); the call then loads `profiler`, the only reader of `debug` (never opened, `env` 2.1) and of
  `profile_timer`.
- Line 24: `if nil == time_global then time_global = function () return device():time_global() end end`:
  `_g` defines `time_global` only if no `time_global` exists when it runs. Whether the engine provides one is
  **unestablished** here (the name occurs as a string in `xrGame.dll`).
- Lines 397–588: `if (editor() == false) then … end`: the classes `pp_effector`, `pp_linear_lerp`,
  `const_evaluator`, `wait_evaluator` and `enabled_evaluator` and their methods are declared only when the
  engine's `editor()` returns false.

## 5. Coroutines and yields (loadable game scripts)

### 5.1 Who creates and resumes coroutines (static) [C5]

No loadable script reads `coroutine.create`, `coroutine.resume`, `coroutine.wrap`, `coroutine.status`,
`coroutine.running`, `coroutine.cstacksize` or `coroutine.coco`. The global `coroutine` is read 4 times, all in
`_g`, each to call `coroutine.yield`; the table is never stored, passed, returned or indexed with a computed
key. So no script creates, resumes or wraps a coroutine or inspects one: every coroutine a script runs in is
created and resumed by the engine.

### 5.2 Where scripts yield (static) [C5]

`coroutine.yield` is called at 4 places, all with no arguments and with the results discarded, each a direct
call from the body of a script function:

- `wait_game` (`_g`, function at line 28; yields at lines 31 and 35): after calling the engine's
  `verify_if_thread_is_running()`, yields once if called without an argument, otherwise yields repeatedly
  while `game.time()` is at most its value at the start plus the argument;
- `wait` (`_g`, function at line 40; yields at lines 43 and 47): the same with `time_global()`.

With `-nojit` the game state has no `coroutine.yield` (`env` 2.1: it comes with `coco`), so reaching these
calls is then an error (static).

### 5.3 The functions from which a yield can be reached (static) [C5]

25 functions reach `coroutine.yield` through calls resolved by name (a global, a namespace field with a
constant name, a local or an upvalue; method calls by method name; section 1):

| File | Function (line) | Calls into the set | Called by scripts at |
|---|---|---|---|
| `_g` | `wait_game` (28) | yields | `test_utils` 158, 164 |
| `_g` | `wait` (40) | yields | `_g` 105, 114, 244, 390; `a1` 10; `a3` 9; `anomaly` 45; `car` 44; `game_last_dialog` 8; `i1` 5; `i2` 3; `path` 28; `postprocess` 317; `walk` 25 |
| `_g` | `debug_get_level_object` (95), `debug_get_actor` (110), method `run_loop` of the class `FSM` (217), `set_current_time` (376) | `wait` | no script |
| `a1`, `a3`, `anomaly`, `car`, `game_last_dialog`, `path`, `postprocess`, `walk` | `main` (lines 1, 1, 23, 1, 4, 9, 312, 2) | `wait` | no script |
| `i1`, `i2` | `get_actor` (3; 1), the file's own | `wait` | its file's `main`: `i1` 18, 21; `i2` 15, 18 |
| `i1`, `i2` | `main` (11; 9) | `get_actor` | no script |
| `test_utils` | `run_test` (153), `run_global` (163) | `wait_game` | `run_test`: `pripyat` 169, `qa` 18, `x18` 74; `run_global`: `pripyat` 177, `qa` 26 |
| `pripyat`, `qa`, `x18` | `r` (167; 16; 72) | `run_test` | no script |
| `pripyat`, `qa` | `main` (175; 24) | `run_global` | no script |

So 19 of them are **entry points** that no script calls: the 12 `main` functions, the 3 `r` functions,
`debug_get_level_object`, `debug_get_actor`, `FSM`'s `run_loop` and `set_current_time`. No configuration or
spawn file names any of the 25 as `<namespace>.<function>` (measured).

### 5.4 What a yield passes through, on the scripts' side (static) [C3, C5, C9]

- Every script call on a path from an entry point to `coroutine.yield` is a direct call by name from one
  function of the set to another; no method call and no call with a computed name reaches the set. The
  computed calls of 4.4 call the fields `set_scheme` and `add_to_binder` of the 65 scheme files, which no
  function of the set is named, and the condition and effect functions named by the data: these are members of
  `xr_conditions` and `xr_effects`, none of which is in the set, or one of the 25 names those namespaces lack,
  which fall back to the globals and find nothing there (4.4). None of the data's names is one of the five
  functions of the set that `_g` defines in the globals (`wait`, `wait_game`, `debug_get_level_object`,
  `debug_get_actor`, `set_current_time`) (measured).
- None of the 25 functions is used as a value other than by the assignment that defines it: none is passed
  to any function (library or engine), stored elsewhere, returned, compared or put in a table; neither the
  globals table nor the namespace of a file holding one of them is used as a value after the prefix. So none
  can be called back by a library function (`table.sort`, `table.foreach`; 3.2), be the iterator of a generic
  `for` (only `pairs`, `ipairs`, `string.gfind` and the 2 table expressions of 2.2), or be called by an engine
  function given it as an argument.
- No script sets a metatable (`setmetatable` is used only by the prefix, with `__index = _G`, `loading` 2.3).
  The only keys starting with `__` scripts store are `__init` (278 times) and `__finalize` (26), methods of
  classes called by the binding library; none of the set is stored under such a key.
- No script calls a function in protected mode (`pcall`, `xpcall` never read; 3.1).
- The engine functions called inside `wait` and `wait_game` (`verify_if_thread_is_running`, `game.time`,
  `time_global`) return before the yield; no yield is made from inside a call to an engine function.

So on the scripts' side, between an entry point and the yield there are only script functions calling each
other by name: no library function, engine function, metamethod, `for` iterator or protected call.

### 5.5 The engine's side

Static analysis of the shipped binaries [C5, C7, C8]:

- **The runtime's primitives.** `xrGame.dll` imports from `xrLUA.dll` the public `coco` / Lua C API
  functions `lua_newcthread` (create a coroutine with a C stack of its own) and `lua_resume`, and neither
  `lua_newthread` nor `lua_yield`; `XR_3DA.exe` imports none of the four, and no other shipped binary imports
  from `xrLUA.dll` (measured). `xrGame.dll` creates coroutines at exactly one place and resumes them at
  exactly one other (static).
- **The C stack.** The engine requests a C stack size of 0. In the shipped `xrLUA.dll`, creating a coroutine
  with a size of 0 or more always gives it a C stack of its own (a Windows fiber); a size of 0 takes the
  runtime's default, which is 0 in the file and is changed only by the coroutine library's stack-size function
  (`coroutine.cstacksize`, which no script reads, 5.1); a size between 1 and 4095 is raised to 4096; a negative
  size gives no C stack of its own (static). A fiber created with stack size 0 gets the executable's default
  commit size (documented); `XR_3DA.exe`'s header sets a default stack reserve of 1,048,576 bytes and commit
  of 4,096 bytes (measured), so each engine coroutine's C stack is believed to reserve 1 MiB.
- **Yielding inside the runtime.** In the shipped `xrLUA.dll`, a yield in a coroutine with its own C stack
  is handed to `coco` before any other test, as in the public release; otherwise a pending counted C-level
  call makes it fail with "attempt to yield across metamethod/C-call boundary" (static; the public
  release's interpreter makes C callbacks, metamethod calls and generic-`for` iterator calls through that
  counted call [C7]). Only two functions reach the yield primitive: the exported `lua_yield`, which no code
  calls and no shipped binary imports, and the coroutine library's `yield`. So neither the engine nor the
  binding library yields by itself; every yield is a script's call of `coroutine.yield` (static).
- **What a coroutine's body is.** Each engine coroutine runs one entry of a list of script names. For an
  entry naming the script `<name>`, the engine builds the text `<name>.main()`, compiles it as a chunk named
  `@_thread_main`, without any namespace prefix (so it runs in the globals), into the new coroutine, and
  resumes that coroutine with no arguments. The chunk is the coroutine's body: a Lua function that reads the
  global `<name>` (which can trigger automatic loading, `loading` 2.2.3, finished before the call) and calls
  its field `main` with no arguments, a call from Lua to Lua. No engine or binding-library function stands
  between the resume and the script function (static). A coroutine whose chunk fails to compile is dropped
  without being resumed; a resume that yields keeps the coroutine; one that returns or fails removes it (a
  failure is reported) (static). Each update of a list resumes one of its coroutines, in turn (static).
- **The other kind of entry.** The coroutine maker also has a form that runs a text instead of a script's
  `main`; no entry of that form is made anywhere in the shipped binaries: every entry is made with that option
  off, and with a second option, whose effect is not examined here, off too (static).
- **Where the names come from.** Entries are made only from comma-separated lists (an empty or missing list
  has no item), at two points (static): when the game state is created, from the `script` value of the section
  of `config\script.ltx` named after the game type; and when a level is loaded, from the `script` value of the
  section `[level_scripts]` of the level's configuration (believed to be its `level.ltx`), or an empty list
  without that section. In the shipped data every such list is empty: in `script.ltx` the `script` value of
  each game-type section (`single`, `deathmatch`, `teamdeathmatch`, `artefacthunt`, `lastingame`) is empty
  after its comment is removed (`[common]`'s `script = _G` is not a game-type section), and none of the 30
  `level.ltx` files has a section `[level_scripts]` (measured).
- **So, in the shipped game, the engine creates no coroutine** (static, from the binaries and the data),
  and none of the 4 yields of 5.2 is reached inside one. Of the 19 entry points of 5.3, only the 12 `main`
  functions have the form the engine calls, and only if a list named their scripts; the names of the other 7
  (the 3 `r`, `debug_get_level_object`, `debug_get_actor`, `run_loop`, `set_current_time`), like `wait`,
  `wait_game`, `run_test`, `run_global` and `get_actor`, occur as strings in no shipped binary (`r` aside, too
  short to test) (measured), and none of the 25 functions is ever passed to the engine as a value (5.4), so
  the engine is **believed** never to call them. A call of `coroutine.yield` outside any coroutine is an error
  (**believed**, from the public release; SE-8).

### 5.6 Consequence for a runtime whose coroutines cannot yield across C calls (static)

Every coroutine the engine can make has a Lua chunk as its body, which calls the script's `main` from Lua
(5.5), and on the scripts' side every path from an entry point to `coroutine.yield` passes only script
functions calling each other by name (5.4). So no yield the shipped scripts can make crosses a C call: the
public Lua 5.1 coroutines, without `coco`'s C stacks, support all of them. The engine gives each coroutine a C
stack of its own anyway (5.5), which does not change this. In the shipped data the engine starts no coroutine,
so none of these yields happens at all.

## 6. Shader scripts

The 69 shader scripts (section 1) run in the shader state, which opens `base`, `table`, `string`, `math` and
`jit` and has no `coco` (`env` 2.1). Everything here is measured or static from their compiled text [C2, C6].

### 6.1 Language features

- Keywords used: only `function`, `end`, `local`, `true`, `false`, `or`. No conditional, loop, `return`,
  table constructor or comparison; operators only `..` (5 times, in 2 files) and `...` (2, in 2), with
  parentheses, `,`, `.`, `:` and `=`.
- Literals: decimal integers (146, in 52 files) and double-quoted strings (550, in 68 files), whose only
  escape is `\\` (61, in 23 files); 29 `--` comments in 21 files and 1 long comment.
- 139 functions, all named global definitions. The 2 vararg functions, `printf` in `shaders\r1\_.s` and
  `shaders\r2\_.s`, use the implicit `arg` table (no `...` in an expression), as in 2.2. No shader script
  calls `printf`.
- The compiled chunks use 14 instruction kinds: calls, method calls, constants and booleans, global reads and
  writes, field reads, moves, upvalue reads, function creation, return, concatenation, and two `or` expressions (in
  one file).

### 6.2 Library and globals

- Library use: `string.format` and `unpack`, both only in the two `printf` functions.
- Globals the shader scripts assign: the functions `normal` (67 definitions in 64 files), `l_special` (40),
  `l_point` (11), `l_spot` (11), `normal_hq` (6), `printf` (2), `r1_lpoint` and `r1_lspot` (in `shaders\r1\_.s`);
  and the string values `t_rt`, `t_noise` (in `shaders\r1\_.s` and `shaders\r2\postprocess.s`), `t_distort`,
  `t_point_att` (in `shaders\r1\_.s`).
- Read by shader scripts that do not assign them: `r1_lpoint` and `r1_lspot` (11 files each), `t_rt`,
  `t_noise` (the two `postprocess` files of `r1`) and `t_distort` (`r1\postprocess_d.s`), all assigned only in
  `shaders\r1\_.s` (and `t_rt`, `t_noise` in `r2\postprocess.s`). Whether these reads find the values depends on
  whether the shader chunks share one environment and `_.s` runs first: **unestablished** (how the shader
  state loads its scripts is SE-5's subject).
- Read by shader scripts and assigned by none (the names the engine has to provide, or nil): `blend` (198
  reads in 55 files; fields read: `invsrcalpha` 83, `srcalpha` 77, `one` 18, `srccolor` 14, `zero` 5,
  `destcolor` 1), `log` (2, in `printf`), `arg` (2, in `printf`, as in 2.2).
- Methods called on the objects the engine passes in (by name, with counts): `sampler` 209, `texture` 209,
  `begin` 115, `fog` 115, `zb` 109, `blend` 99, `sorting` 92, `distort` 61, `aref` 54, `clamp` 51, `f_linear` 28,
  `project` 7, `emissive` 4, `f_anisotropic` 4, `wrap` 4, `wmark` 3.
- Which of the assigned functions the engine calls, with what arguments, is **unestablished** here.

### 6.3 Coroutines

No shader script reads `coroutine`; the shader state has none of `coco`'s functions (`env` 2.1). No shader
script yields.

## 7. Not established

- What the embedded runtime does with a table as the value after `in` of a generic `for` (2.2; SE-8).
- Whether the embedded parser declares the implicit `arg` of Lua 5.0 vararg functions (2.2, **believed** not;
  SE-3), and so whether `arg` is a global read in `anomaly`, `utils` and the shader `printf`.
- Whether `pairs`, `ipairs`, `math.pi` and `math.mod` exist (**believed**; SE-1).
- Whether the receivers of the 39 `:sub(…)` calls include strings (3.3).
- Which of the names of 4.5 and 6.2 the engine provides, and what they are (the script interface
  specifications); where the binding library puts the names declared by `class` (SE-9).
- Whether every condition-list block of 4.4 is read as one by the game (4.4, found by pattern), and whether the engine provides any of the 25 names the two namespaces lack (**believed** not).
- The engine's side of coroutines (5.5): which configuration file the level's `[level_scripts]` is read from
  (believed `level.ltx`); the C stack reserve of an engine coroutine (believed 1 MiB); whether the engine
  calls any of the 7 entry points that are not `main` (believed not); what a yield outside any coroutine does
  in the embedded runtime (believed an error; SE-8).
- How the shader state loads, prefixes and calls its scripts (6.2; SE-5).
- Scripts outside the loadable set (`loading` 3) are not examined.

## Provenance

- Date: 2026-11-27 (section 5.5 and the data-supplied names of 4.4 revised 2026-11-28).
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build): the `scripts\` and `shaders\`
  trees of the archives `gamedata.db0` … `gamedata.dbd` (extracted with the project's archive tool), the
  configuration and spawn files used by `loading` section 3 (`config\script.ltx` and the 30 `level.ltx` files
  among them), and `bin\xrGame.dll`, `bin\XR_3DA.exe`, `bin\xrLUA.dll` as identified in `env`, `bin\xrCore.dll`,
  and the import tables of every other `.dll` and `.exe` in `bin\`.
- Analysis: the chunks of the 250 loadable scripts built as `loading` 2.3 and 1.5 specify, and the 69 shader
  scripts after `loading` 1.5; each compiled (not run) with the public LuaJIT 1.0.3 release's compiler and its
  function prototypes dumped by a throwaway program; a throwaway tokenizer, a throwaway reader of the
  compiled prototypes (register origins within a function, name-based call resolution), a throwaway PE import
  reader and byte searches of the binaries. The public release's source was read for its compatibility
  settings, its `math` and base libraries' extra fields, its instruction list and its coroutine and `coco`
  functions. For 5.5, static analysis of `xrGame.dll`, `xrLUA.dll` and `xrCore.dll` with Ghidra (headless,
  read-only: disassembly, decompilation, cross-references), and the PE headers; Microsoft's public
  documentation of `CreateFiber` for a stack size of 0. No binary of the game was run for this specification.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- Section 5.5 does not say that, for an entry naming a script, the engine loads that script's namespace (in the game state, outside any coroutine) before it creates the coroutine, nor what the entry's second option changes about that load (the second option is named, but its effect is left unexamined).
