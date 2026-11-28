# Script API C: class ids and the object factory

Behaviour specification of how the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl binaries (Russian edition)
identify kinds of game object by **class id**, keep the **object factory** (the table that maps each class id
to the code that creates objects of that kind), let scripts add entries to it (`object_factory:register`, in
both its forms, called from the `class_registrators` of `config\script.ltx`), number the class ids for scripts
(the global table `clsid`), and create objects through it. This is spec C of the script API split
(`docs/behavior/script-api-scope.md`, section 6).

Builds on (not repeated here):

- `docs/behavior/script-environment.md`: the runtime, its number type, the binding library that exports the
  engine interface.
- `docs/behavior/script-loading.md`: script names and namespaces, the loading occasions of its section 2.2
  (occasion 4 is used below), startup, the silent failures of its section 2.4, the comment removal of its
  section 1.5, and the 250 **loadable** scripts of its section 3.
- `docs/behavior/script-api-scope.md`: the listing `scripts\lua_help.script` (its 2.1), the counting rules
  (its section 1), the scope rule and the entry points of its 4.5.
- `docs/behavior/offline-simulation.md` and `docs/formats/sav.md` (7.4.2, 7.4.3): the object kinds by class id
  and which class ids can occur in the simulation and in saves.
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md`:
  **script classes** and **engine classes** (both **class objects**), where a declaration stores a class
  (its 1.1), what calling a class does (its 2.1), the **script object** and its **engine part** (its 3.1).
  Statements below that rest on A are marked **(depends on A)**.
- Spec B, `docs/behavior/script-api-engine-services.md`: `editor()` is always false (its 4.2).

Conventions:

- Files are the winning copies of the root archives `gamedata.db0` … `gamedata.dbd`, compared by stored path
  ignoring case. Binaries are those of the script environment specification: `xrGame.dll` (6,246,400 bytes,
  MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), `xrCore.dll`, `xrLUA.dll`.
- Names are given only as the shipped scripts, the shipped data or the binaries' message strings spell them.
- Each fact is **measured** (from the data or file bytes), **static** (from static analysis of the binaries,
  not observed at run time), **believed** (inferred, not traced) or **unestablished**. No binary was run.
- "Fatal" means the game stops with an error message, as for a failed assertion; "silently" means nothing is
  shown or logged (the messages the binary formats in these places go to a function that does nothing,
  static).
- Checks are Python scripts in the dirty workspace directory `checks/class-ids/`, named in brackets after
  each claim. `run_all.py` runs them all in order and writes `run_all.out`; every number below is in it.

## 1. Class ids (static, [static.py] facts 1)

A **class id** is an unsigned 64-bit number made from a short text, the **id text** (`AI_STL`, `SMRTTRRN`,
`MAIN_MNU`, …):

1. Take the text's first 8 bytes (fewer if it is shorter; a longer text is cut to 8).
2. Pad it with spaces (20) at the end to exactly 8 bytes.
3. The id is these 8 bytes read as a big-endian number: the first byte is the most significant.

So `AI_STL` is the bytes `41 49 5F 53 54 4C 20 20`, the id 0x41495F53544C2020; the empty text is
0x2020202020202020. A byte of 80–FF makes every more significant byte FF as well (each byte is
sign-extended before it is combined); no shipped id text has such a byte or more than 8 characters
[numbering.py], [config_ids.py], [engine_table.py]. The reverse conversion writes the 8 bytes most
significant first, so it returns the padded text (with its trailing spaces).

Where a class id comes from:

- A configuration section's kind is the class id of its `class` key (static, [static.py] facts 5; the
  configuration reader has a reading for this, also reached by scripts as `ini_file:r_clsid`, section 7).
- The engine's own factory entries use fixed ids (section 2); scripts pass id texts to `register`
  (section 3); the game-type function of `config\script.ltx` returns id texts (spec D).

Ids order as unsigned 64-bit numbers. For texts of bytes below 80 this is the byte order of the padded texts,
in which a space (20) sorts below every other character the shipped ids use; for the 188 shipped ids it is
also the plain order of the unpadded texts [numbering.py].

## 2. The object factory

### 2.1 Entries (static)

The factory is a table of **entries**. Each entry has:

- a class id;
- a **class-id name**: the name under which scripts see the id's number (section 5), such as `stalker` or
  `script_stalker`;
- a **client side** and a **server side**: what creating a client object or a server object of this id does
  (section 6). An entry created by the engine creates objects of fixed engine kinds; an entry created by a
  script holds class objects to call.

Adding an entry appends it to the table and marks the table unsorted. There is no test for an id or a name
that is already present: a second entry with an existing id would be kept beside the first (static,
[static.py] 2). No shipped registration repeats an id or a name (measured: 188 entries, 188 distinct ids,
188 distinct names [numbering.py]).

### 2.2 The engine's entries (static, [engine_table.py], [sides.py])

When the factory is created, the engine adds 134 entries (distinct ids and names; their ids, names and sides
are in the table of section 5.3, "engine" rows):

| Sides | Entries |
|---|---|
| client and server | 115 |
| client and server, the kind chosen by the game type | 1 (`O_ACTOR`, `actor`) |
| client only | 15 (`G_LEVEL`, `G_PERSIS`, `HUD_MNGR`, the 4 `SV_…`, 4 `CL_…` and 4 `UI_…` game-type ids) |
| server only | 3 (`AI_FLE_G`, `AI_GRAPH`, `ON_OFF_G`) |

- Creating an object of a side the entry does not have is fatal: "Cannot instantiate client object,
  because client class is not declared!" or "Cannot instantiate server object, because server class is not
  declared!" [sides.py].
- The `O_ACTOR` entry creates, on each side, one kind when the current game-type value is 0 or 1 and another
  kind otherwise [sides.py]; that 1 is the single-player game type is **believed**.
- What each engine kind is, is outside this spec (the server kinds are those of sav.md 7.4.2 by class id).

On a dedicated server only, the engine adds 26 further entries, after those 134, with 26 of the ids and names
of the script registrations of section 3.3 (`SCRPTART` `artefact_s`, `TORCH_S`,
`E_STLK`, `WP_SCOPE`, the 17 `WP_…` weapon ids, `ZS_BFUZZ`, `ZS_MBALD`, `ZS_GALAN`, `ZS_MINCE`,
`SPC_RS_S` `script_restr`), with engine kinds in place of the scripts' classes [engine_table.py]. A
dedicated server creates no script state (static, [static.py]); it is not covered further.

### 2.3 Lifetime (static, [static.py] facts 2, 3, 5)

- There is one factory. It is created on first use. Creating it adds the engine's entries (2.2) and then,
  except on a dedicated server, makes sure the script state exists; creating the script state runs startup
  (script-loading.md 2.2, occasions 1 and 2), whose class registrators add their entries to this same factory
  (section 4). So once the factory exists, the scripts' entries are in it as well.
- When the simulation is created (a New Game or a load), unless the lowercased command line contains
  `-keep_lua` (a substring test, like `-nojit` in the script environment specification), the factory is
  discarded and the whole startup runs again: the previous script state is closed and a new one opened, the
  engine interface registered, `_G` loaded, the class registrators called with a new factory, and the `clsid`
  table built anew. Registrations therefore never accumulate across runs. (The call order of startup as a
  whole is spec D's.)

### 2.4 Order, lookup and numbering (static, [static.py] facts 2)

- The entries are sorted by class id, ascending, as unsigned 64-bit numbers. Sorting happens when the table
  is next used after it was marked unsorted (a lookup, a numbering, or building the `clsid` table).
- The **number** of a class id is the 0-based position, in that order, of the first entry whose id is not
  below it. For a registered id this is the entry's own position; for an id that is not registered it is the
  position of the next larger registered id (or the entry count, if there is none). The factory does not
  detect the difference.
- The **entry for** a class id is likewise the first entry whose id is not below it, with no test that its id
  matches: creating an object of an unregistered id uses the entry of the next larger id, or reads past the
  end of the table (behaviour undefined). Of the 128 distinct `class` values of the `.ltx` files of
  `config\`, 6 are not registered (`AI_RAT`, `AI_RAT_G`, `AI_SPGRP`, `EVENT`, `II_BTTCH`, `NW_ATTCH`)
  [config_ids.py]; sav.md 7.4.3 lists them as never created, and creating an object of one of their sections
  would not be stopped by the factory. 66 registered ids are named by no `class` key of `config\`.

## 3. Registration from scripts: `object_factory:register`

### 3.1 The factory object (static, [static.py] facts 4)

The engine registers a class `object_factory` with exactly two methods, both named `register`, and no
constructor; scripts get the factory object only as the argument of a class registrator (section 4). A call
`f:register(…)` chooses between the two forms by its arguments (by their number and types; **believed**: the
binding library's choice among same-named overloads, which spec A 3.1 describes for constructors):

- the **4-argument form** `register(client class, server class, id text, class-id name)`;
- the **3-argument form** `register(client class, id text, class-id name)`.

The receiver must be the factory object as a registrator receives it, and every other argument must be a
Lua string (a number is not accepted in its place). A call that matches neither form raises a script error
(**believed**, depends on A).

### 3.2 What a call does (static, [static.py] facts 4; [client_classes.py])

Class names are resolved as follows:

1. The name is split at its last `.` into a namespace and a member name; without a `.` the namespace is
   `_G`, the global table.
2. A namespace other than `_G` is loaded first as in occasion 4 of script-loading.md 2.2 (loaded if it does
   not exist yet, even if its file was missing earlier).
3. The namespace must exist as a table; the member is then found by going through the namespace table's own
   entries (raw, so the namespace's fallback to the globals and other metatable lookups are not used), and it
   counts only if its value is a userdata value. Class objects are userdata values (**believed**: the binding
   library's class objects are userdata; depends on A).

4-argument form: the client class name is resolved; if that fails the call ends; otherwise the server class
name is resolved; if that fails the call ends. If both resolve, the id text is converted (section 1) and an
entry is added with that id, the class-id name, and the two class objects as its client and server side.

3-argument form: the client class name is resolved; if that fails the call ends. Otherwise an entry is added
with the converted id, the class-id name, and the client class object stored as both its client and its
server side.

A failed resolution is silent: the call returns normally and nothing is added, so the id has no entry and
no `clsid` name. A failure while loading the namespace is silent too (script-loading.md 2.4).

### 3.3 The shipped registrations (measured, [registrations.py], [client_classes.py])

`scripts\class_registrator.script` and `scripts\game_registrator.script` make 54 registrations in this
order (all 54 rows of the "script" entries in 5.3):

- `class_registrator.register(object_factory)` calls two helpers of its own script: `cs_register(factory, …)`
  always calls the 4-argument form; `c_register(factory, …)` calls the 3-argument form only when `editor()`
  is `false`, which it always is (spec B 4.2). Its calls are: `c_register` once (`ui_main_menu.main_menu`,
  `MAIN_MNU`, `main_menu`), then `cs_register` 51 times.
- `game_registrator.register(object_factory)` calls the 3-argument form twice, inside a test that `editor()`
  is `false`: `game_last_sv.game_sv_LastStanding`, `SV_LAST`, `game_sv_LastStanding`; then
  `game_last_cl.game_cl_LastStanding`, `CL_LAST`, `game_cl_LastStanding`.

Of the 52 distinct client classes, 48 are engine class names without a `.` (`CAI_Stalker`, `CMincer`,
`ce_smart_zone`, …; `CMincer` and `ce_smart_zone` are each used twice), each registered by the engine as a
class under that name (static); 4 are script classes (`ce_switcher.ce_switcher` and the three of the
3-argument calls). The 20 distinct server classes are all script classes. Each of these 24 script classes is
declared, with a base, in the script its namespace names; every server class's `__init` takes one parameter
(`section`, or `sect` in `se_switcher.cse_switcher`), every client script class's `__init` none (measured).
Every one of these declarations is at the top level of its script, so it stores the class in the script's
namespace (spec A 1.1, 1.2), where the raw search of 3.2 finds it: all 54 registrations add their entry
(**believed**, from A and the 48 engine names above; depends on A).

No other script calls `register` (measured, [uses.py]). The ids and names of the shipped registrations are
disjoint from the engine's (measured, [numbering.py]).

## 4. The `class_registrators` call (static, [static.py] facts 6)

During startup, after `_G` is loaded (script-loading.md 2.2, occasion 1):

1. The engine reads `$game_config$script.ltx`. If it has no section `[common]`, there are no registrators;
   if `[common]` has no key `class_registrators`, the list is empty.
2. The value is a comma-separated list (LTX list items; white space around items trimmed, **believed**). In
   this edition it is `class_registrator.register, game_registrator.register` (measured).
3. For each item in order, the name is resolved as in 3.2, except that the value must be a function; its
   namespace is loaded first (script-loading.md 2.2, occasion 2). An item that cannot be resolved is skipped
   silently.
4. A resolved function is called with one argument, the factory object (section 3.1; the factory is created
   at this point if it does not exist, 2.3), and its results are ignored.
5. A script error during the call is fatal: the message is `LUA error: ` followed by the error text. This
   includes a `register` call that matches neither form (3.1).

The factory's entries are complete when the last registrator returns: the registrators and their helpers
only call `register` on the factory object and keep no reference to it (measured, [registrations.py]). A script that kept it and registered later would add an entry that creation
finds but that has no `clsid` name, and would shift the numbers of every larger id away from the `clsid`
table's values (static, 2.4 and 5.1); this does not arise in the shipped scripts.

## 5. The `clsid` table

### 5.1 When and what (static, [static.py] facts 3)

Right after the class registrators (and the factory's creation, if no registrator caused it), startup
builds the global `clsid`: a class object of the engine (no constructor, no methods) whose constants are,
for every factory entry in sorted order (2.4), the entry's class-id name with its position as the value (a
number, 0 to 187 in this edition). Then the `[common] script` list is loaded (script-loading.md 2.2,
occasion 2). It is rebuilt with each startup (2.3).

So `clsid` does not exist while `_G`'s top-level code or the class registrators run: during that time the
name `clsid` is an undefined global, which loads nothing (there is no `scripts\clsid.script`) and reads as
nil (script-loading.md 2.2, occasion 3). None of the 13 scripts loaded during registration (the namespaces
of the registered classes; their top-level code names no further script) reads `clsid` outside its functions
(measured, [early.py]). In `class_registrator.script`, `clsid` is a parameter name of its helpers, not the
table.

Reading `clsid.name` gives the class table's own field if it has one (none in the shipped scripts: no
script assigns to a `clsid` field, measured [uses.py]), else the constant of that name, else nil, without
an error (static, `xrLUA.dll`). Names compare exactly, so case matters (**believed**: string comparison).

### 5.2 Values in this edition (measured, [numbering.py])

The rule of 5.1 applied to the 134 engine entries and the 54 shipped registrations (`editor()` false, spec
B) gives 188 constants, numbered 0–187, listed in 5.3.

The listing `scripts\lua_help.script` has 185 `clsid` constants: all names of the table except
`script_heli`, `script_phys` and `script_restr` (the registrations `C_HLCP_S`, `O_PHYS_S`, `SPC_RS_S`). Its
values are exactly the positions the same rule gives over its own 185 names, so the listing came from a
build whose registrators lacked those three; 49 of its values equal the shipped ones (those of the ids below
`C_HLCP_S`) and the other 136 are lower by 1 to 3. The shipped values are those of 5.3.

(Were `editor()` true, the 3 conditional registrations would be missing and the numbers would differ; spec B
4.2 has it always false.)

### 5.3 The table

Value, id text, class-id name, and the entry: "engine" with its sides (2.2), or "script" with its client and
server class (3.3; "only" for the 3-argument form).

| Value | Id | Name | Entry |
|---|---|---|---|
| 0 | `AF_BAST` | `art_bast_artefact` | engine, client and server |
| 1 | `AF_BDROP` | `art_black_drops` | engine, client and server |
| 2 | `AF_BGRAV` | `art_gravi_black` | engine, client and server |
| 3 | `AF_DUMMY` | `art_dummy` | engine, client and server |
| 4 | `AF_EBALL` | `art_electric_ball` | engine, client and server |
| 5 | `AF_FBALL` | `art_faded_ball` | engine, client and server |
| 6 | `AF_GALAN` | `art_galantine` | engine, client and server |
| 7 | `AF_GRAVI` | `art_gravi` | engine, client and server |
| 8 | `AF_MBALL` | `art_mercury_ball` | engine, client and server |
| 9 | `AF_NEEDL` | `art_needles` | engine, client and server |
| 10 | `AF_RHAIR` | `art_rusty_hair` | engine, client and server |
| 11 | `AF_THORN` | `art_thorn` | engine, client and server |
| 12 | `AF_ZUDA` | `art_zuda` | engine, client and server |
| 13 | `AI_BLOOD` | `bloodsucker` | engine, client and server |
| 14 | `AI_BOAR` | `boar` | engine, client and server |
| 15 | `AI_BURER` | `burer` | engine, client and server |
| 16 | `AI_CAT` | `cat` | engine, client and server |
| 17 | `AI_CONTR` | `controller` | engine, client and server |
| 18 | `AI_CROW` | `crow` | engine, client and server |
| 19 | `AI_DOG_B` | `dog_black` | engine, client and server |
| 20 | `AI_DOG_F` | `psy_dog_phantom` | engine, client and server |
| 21 | `AI_DOG_P` | `psy_dog` | engine, client and server |
| 22 | `AI_DOG_R` | `dog_red` | engine, client and server |
| 23 | `AI_FLESH` | `flesh` | engine, client and server |
| 24 | `AI_FLE_G` | `flesh_group` | engine, server |
| 25 | `AI_FRACT` | `fracture` | engine, client and server |
| 26 | `AI_GIANT` | `pseudo_gigant` | engine, client and server |
| 27 | `AI_GRAPH` | `graph_point` | engine, server |
| 28 | `AI_HIMER` | `chimera` | engine, client and server |
| 29 | `AI_PHANT` | `phantom` | engine, client and server |
| 30 | `AI_POLTR` | `poltergeist` | engine, client and server |
| 31 | `AI_SNORK` | `snork` | engine, client and server |
| 32 | `AI_STL` | `stalker` | engine, client and server |
| 33 | `AI_STL_S` | `script_stalker` | script: `CAI_Stalker`, `se_stalker.se_stalker` |
| 34 | `AI_TRADE` | `trader` | engine, client and server |
| 35 | `AI_TRD_S` | `script_trader` | script: `CAI_Trader`, `se_stalker.se_trader` |
| 36 | `AI_TUSH` | `tushkano` | engine, client and server |
| 37 | `AI_ZOM` | `zombie` | engine, client and server |
| 38 | `AMMO` | `wpn_ammo` | engine, client and server |
| 39 | `ARTEFACT` | `artefact` | engine, client and server |
| 40 | `A_M209` | `wpn_ammo_m209` | engine, client and server |
| 41 | `A_OG7B` | `wpn_ammo_og7b` | engine, client and server |
| 42 | `A_VOG25` | `wpn_ammo_vog25` | engine, client and server |
| 43 | `CL_AHUNT` | `game_cl_artefact_hunt` | engine, client |
| 44 | `CL_DM` | `game_cl_deathmatch` | engine, client |
| 45 | `CL_LAST` | `game_cl_LastStanding` | script: `game_last_cl.game_cl_LastStanding` only |
| 46 | `CL_SINGL` | `game_cl_single` | engine, client |
| 47 | `CL_TDM` | `game_cl_team_deathmatch` | engine, client |
| 48 | `C_HLCPTR` | `helicopter` | engine, client and server |
| 49 | `C_HLCP_S` | `script_heli` | script: `CHelicopter`, `se_car.se_heli` |
| 50 | `C_NIVA` | `car` | engine, client and server |
| 51 | `D_PDA` | `device_pda` | engine, client and server |
| 52 | `D_SIMDET` | `device_detector_simple` | engine, client and server |
| 53 | `D_TORCH` | `device_torch` | engine, client and server |
| 54 | `EQU_EXO` | `equ_exo` | engine, client and server |
| 55 | `EQU_MLTR` | `equ_military` | engine, client and server |
| 56 | `EQU_SCIE` | `equ_scientific` | engine, client and server |
| 57 | `EQU_STLK` | `equ_stalker` | engine, client and server |
| 58 | `E_STLK` | `equ_stalker_s` | script: `CStalkerOutfit`, `se_item.se_outfit` |
| 59 | `G_F1` | `wpn_grenade_f1` | engine, client and server |
| 60 | `G_FAKE` | `wpn_grenade_fake` | engine, client and server |
| 61 | `G_LEVEL` | `level` | engine, client |
| 62 | `G_PERSIS` | `game` | engine, client |
| 63 | `G_RGD5` | `wpn_grenade_rgd5` | engine, client and server |
| 64 | `G_RPG7` | `wpn_grenade_rpg7` | engine, client and server |
| 65 | `HUD_MNGR` | `hud_manager` | engine, client |
| 66 | `II_ANTIR` | `obj_antirad` | engine, client and server |
| 67 | `II_ATTCH` | `obj_attachable` | engine, client and server |
| 68 | `II_BANDG` | `obj_bandage` | engine, client and server |
| 69 | `II_BOLT` | `obj_bolt` | engine, client and server |
| 70 | `II_BOTTL` | `obj_bottle` | engine, client and server |
| 71 | `II_DOC` | `obj_document` | engine, client and server |
| 72 | `II_EXPLO` | `obj_explosive` | engine, client and server |
| 73 | `II_FOOD` | `obj_food` | engine, client and server |
| 74 | `II_MEDKI` | `obj_medkit` | engine, client and server |
| 75 | `LVL_CHNG` | `level_changer` | engine, client and server |
| 76 | `MAIN_MNU` | `main_menu` | script: `ui_main_menu.main_menu` only |
| 77 | `MP_PLBAG` | `mp_players_bag` | engine, client and server |
| 78 | `ON_OFF_G` | `online_offline_group` | engine, server |
| 79 | `O_ACTOR` | `actor` | engine, client and server, kind by game type |
| 80 | `O_BRKBL` | `obj_breakable` | engine, client and server |
| 81 | `O_CLMBL` | `obj_climable` | engine, client and server |
| 82 | `O_HLAMP` | `hanging_lamp` | engine, client and server |
| 83 | `O_INVBOX` | `inventory_box` | engine, client and server |
| 84 | `O_PHYSIC` | `obj_physic` | engine, client and server |
| 85 | `O_PHYS_S` | `script_phys` | script: `CPhysicObject`, `se_item.se_physic` |
| 86 | `O_SEARCH` | `projector` | engine, client and server |
| 87 | `O_SWITCH` | `switcher` | script: `ce_switcher.ce_switcher`, `se_switcher.cse_switcher` |
| 88 | `P_DSTRBL` | `obj_phys_destroyable` | engine, client and server |
| 89 | `P_SKELET` | `obj_phskeleton` | engine, client and server |
| 90 | `RE_SPAWN` | `respawn` | script: `ce_smart_zone`, `se_respawn.se_respawn` |
| 91 | `SCRIPTZN` | `script_zone` | engine, client and server |
| 92 | `SCRPTART` | `artefact_s` | script: `CElectricBall`, `se_artefact.se_artefact` |
| 93 | `SCRPTCAR` | `car_s` | script: `CCar`, `se_car.se_car` |
| 94 | `SCRPTOBJ` | `script_object` | engine, client and server |
| 95 | `SMRTTRRN` | `smart_terrain` | script: `ce_smart_zone`, `smart_terrain.se_smart_terrain` |
| 96 | `SMRTZONE` | `smart_zone` | engine, client and server |
| 97 | `SM_BLOOD` | `bloodsucker_s` | script: `CAI_Bloodsucker`, `se_monster.se_monster` |
| 98 | `SM_BOARW` | `boar_s` | script: `CAI_Boar`, `se_monster.se_monster` |
| 99 | `SM_BURER` | `burer_s` | script: `CBurer`, `se_monster.se_monster` |
| 100 | `SM_CAT_S` | `cat_s` | script: `CCat`, `se_monster.se_monster` |
| 101 | `SM_CHIMS` | `chimera_s` | script: `CChimera`, `se_monster.se_monster` |
| 102 | `SM_CONTR` | `controller_s` | script: `CController`, `se_monster.se_monster` |
| 103 | `SM_DOG_F` | `psy_dog_phantom_s` | script: `CPsyDogPhantom`, `se_monster.se_monster` |
| 104 | `SM_DOG_P` | `psy_dog_s` | script: `CPsyDog`, `se_monster.se_monster` |
| 105 | `SM_DOG_S` | `dog_s` | script: `CAI_Dog`, `se_monster.se_monster` |
| 106 | `SM_FLESH` | `flesh_s` | script: `CAI_Flesh`, `se_monster.se_monster` |
| 107 | `SM_GIANT` | `gigant_s` | script: `CPseudoGigant`, `se_monster.se_monster` |
| 108 | `SM_IZLOM` | `fracture_s` | script: `CFracture`, `se_monster.se_monster` |
| 109 | `SM_POLTR` | `poltergeist_s` | script: `CPoltergeist`, `se_monster.se_monster` |
| 110 | `SM_P_DOG` | `pseudodog_s` | script: `CAI_PseudoDog`, `se_monster.se_monster` |
| 111 | `SM_SNORK` | `snork_s` | script: `CSnork`, `se_monster.se_monster` |
| 112 | `SM_TUSHK` | `tushkano_s` | script: `CTushkano`, `se_monster.se_monster` |
| 113 | `SM_ZOMBI` | `zombie_s` | script: `CZombie`, `se_monster.se_monster` |
| 114 | `SPACE_RS` | `space_restrictor` | engine, client and server |
| 115 | `SPC_RS_S` | `script_restr` | script: `CSpaceRestrictor`, `se_zones.se_restrictor` |
| 116 | `SPECT` | `spectator` | engine, client and server |
| 117 | `SV_AHUNT` | `game_sv_artefact_hunt` | engine, client |
| 118 | `SV_DM` | `game_sv_deathmatch` | engine, client |
| 119 | `SV_LAST` | `game_sv_LastStanding` | script: `game_last_sv.game_sv_LastStanding` only |
| 120 | `SV_SINGL` | `game_sv_single` | engine, client |
| 121 | `SV_TDM` | `game_sv_team_deathmatch` | engine, client |
| 122 | `TORCH_S` | `device_torch_s` | script: `CTorch`, `se_item.se_item_torch` |
| 123 | `UI_AHUNT` | `game_ui_artefact_hunt` | engine, client |
| 124 | `UI_DM` | `game_ui_deathmatch` | engine, client |
| 125 | `UI_SINGL` | `game_ui_single` | engine, client |
| 126 | `UI_TDM` | `game_ui_team_deathmatch` | engine, client |
| 127 | `WP_AK74` | `wpn_ak74_s` | script: `CWeaponAK74`, `se_item.se_weapon_magazined_w_gl` |
| 128 | `WP_BINOC` | `wpn_binocular_s` | script: `CWeaponBinoculars`, `se_item.se_weapon_magazined` |
| 129 | `WP_BM16` | `wpn_bm16_s` | script: `CWeaponBM16`, `se_item.se_weapon_shotgun` |
| 130 | `WP_GROZA` | `wpn_groza_s` | script: `CWeaponGroza`, `se_item.se_weapon_magazined_w_gl` |
| 131 | `WP_HPSA` | `wpn_hpsa_s` | script: `CWeaponHPSA`, `se_item.se_weapon_magazined` |
| 132 | `WP_KNIFE` | `wpn_knife_s` | script: `CWeaponKnife`, `se_item.se_weapon` |
| 133 | `WP_LR300` | `wpn_lr300_s` | script: `CWeaponLR300`, `se_item.se_weapon_magazined` |
| 134 | `WP_PM` | `wpn_pm_s` | script: `CWeaponPM`, `se_item.se_weapon_magazined` |
| 135 | `WP_RG6` | `wpn_rg6_s` | script: `CWeaponRG6`, `se_item.se_weapon_shotgun` |
| 136 | `WP_RPG7` | `wpn_rpg7_s` | script: `CWeaponRPG7`, `se_item.se_weapon_magazined` |
| 137 | `WP_SCOPE` | `wpn_scope_s` | script: `CScope`, `se_item.se_item` |
| 138 | `WP_SHOTG` | `wpn_shotgun_s` | script: `CWeaponShotgun`, `se_item.se_weapon_shotgun` |
| 139 | `WP_SVD` | `wpn_svd_s` | script: `CWeaponSVD`, `se_item.se_weapon_magazined` |
| 140 | `WP_SVU` | `wpn_svu_s` | script: `CWeaponSVU`, `se_item.se_weapon_magazined` |
| 141 | `WP_USP45` | `wpn_usp45_s` | script: `CWeaponUSP45`, `se_item.se_weapon_magazined` |
| 142 | `WP_VAL` | `wpn_val_s` | script: `CWeaponVal`, `se_item.se_weapon_magazined` |
| 143 | `WP_VINT` | `wpn_vintorez_s` | script: `CWeaponVintorez`, `se_item.se_weapon_magazined` |
| 144 | `WP_WALTH` | `wpn_walther_s` | script: `CWeaponWalther`, `se_item.se_weapon_magazined` |
| 145 | `W_AK74` | `wpn_ak74` | engine, client and server |
| 146 | `W_BINOC` | `wpn_binocular` | engine, client and server |
| 147 | `W_BM16` | `wpn_bm16` | engine, client and server |
| 148 | `W_FN2000` | `wpn_fn2000` | engine, client and server |
| 149 | `W_FORT` | `wpn_fort` | engine, client and server |
| 150 | `W_GLAUNC` | `wpn_grenade_launcher` | engine, client and server |
| 151 | `W_GROZA` | `wpn_groza` | engine, client and server |
| 152 | `W_HPSA` | `wpn_hpsa` | engine, client and server |
| 153 | `W_KNIFE` | `wpn_knife` | engine, client and server |
| 154 | `W_LR300` | `wpn_lr300` | engine, client and server |
| 155 | `W_MOUNTD` | `wpn_mounted` | engine, client and server |
| 156 | `W_PM` | `wpn_pm` | engine, client and server |
| 157 | `W_RG6` | `wpn_rg6` | engine, client and server |
| 158 | `W_RPG7` | `wpn_rpg7` | engine, client and server |
| 159 | `W_SCOPE` | `wpn_scope` | engine, client and server |
| 160 | `W_SHOTGN` | `wpn_shotgun` | engine, client and server |
| 161 | `W_SILENC` | `wpn_silencer` | engine, client and server |
| 162 | `W_STMGUN` | `wpn_stat_mgun` | engine, client and server |
| 163 | `W_SVD` | `wpn_svd` | engine, client and server |
| 164 | `W_SVU` | `wpn_svu` | engine, client and server |
| 165 | `W_USP45` | `wpn_usp45` | engine, client and server |
| 166 | `W_VAL` | `wpn_val` | engine, client and server |
| 167 | `W_VINT` | `wpn_vintorez` | engine, client and server |
| 168 | `W_WALTHR` | `wpn_walther` | engine, client and server |
| 169 | `W_WMAGAZ` | `wpn_wmagaz` | engine, client and server |
| 170 | `W_WMAGGL` | `wpn_wmaggl` | engine, client and server |
| 171 | `ZS_BFUZZ` | `zone_bfuzz_s` | script: `CHairsZone`, `se_zones.se_zone_visual` |
| 172 | `ZS_GALAN` | `zone_galant_s` | script: `CMincer`, `se_zones.se_zone_anom` |
| 173 | `ZS_MBALD` | `zone_mbald_s` | script: `CMosquitoBald`, `se_zones.se_zone_anom` |
| 174 | `ZS_MINCE` | `zone_mincer_s` | script: `CMincer`, `se_zones.se_zone_anom` |
| 175 | `Z_ACIDF` | `zone_acid_fog` | engine, client and server |
| 176 | `Z_AMEBA` | `ameba_zone` | engine, client and server |
| 177 | `Z_BFUZZ` | `zone_bfuzz` | engine, client and server |
| 178 | `Z_DEAD` | `zone_dead` | engine, client and server |
| 179 | `Z_GALANT` | `zone_galantine` | engine, client and server |
| 180 | `Z_MBALD` | `zone_mosquito_bald` | engine, client and server |
| 181 | `Z_MINCER` | `zone_mincer` | engine, client and server |
| 182 | `Z_NOGRAV` | `nogravity_zone` | engine, client and server |
| 183 | `Z_RADIO` | `zone_radioactive` | engine, client and server |
| 184 | `Z_RUSTYH` | `zone_rusty_hair` | engine, client and server |
| 185 | `Z_TEAMBS` | `team_base_zone` | engine, client and server |
| 186 | `Z_TORRID` | `torrid_zone` | engine, client and server |
| 187 | `Z_ZONE` | `zone` | engine, client and server |

### 5.4 How the scripts use it (measured, [uses.py])

- 26 scripts read `clsid` fields, 20 of them loadable; they read 106 distinct names, 101 in loadable
  scripts; 88 names of the table are read by no script.
- Names read that the table lacks, so they read as nil (5.1): in the loadable `xr_state.script`,
  `wpn_fn2000_s`, `wpn_fort_s` and `wpn_shotgun_S` (the table has `wpn_shotgun_s`); in scripts that are not
  loadable, `rat`, `rat_group` (the file named `.script`, and `xr_assistance.script`) and `soldier` (those two and
  `xr_talker.script`).
- What a script does with a nil in place of a number is the subject of the spec owning that script's
  behaviour.

## 6. Creation through the factory

### 6.1 The two operations (static, [static.py] facts 5, [sides.py])

- **Client object of a class id**: find the entry for the id (2.4) and create its client-side object; the
  new object records the class id. `xrGame.dll` exports this operation (one of its two exports) for the
  engine executable; both `XR_3DA.exe` and `dedicated\XR_3DA.exe` contain its export name (measured,
  [static.py]). When the main menu is activated, `xrGame.dll` asks the engine to create the client object of
  id `MAIN_MNU` and abandons the activation if no object results (static); that the engine's creation entry
  is this exported operation is **believed**. The ids of the level, the game and the HUD manager
  (`G_LEVEL`, `G_PERSIS`, `HUD_MNGR`) occur in `XR_3DA.exe` as their two 32-bit halves, not as text
  (measured); that it creates those objects this way is **believed**.
- **Server object of a configuration section**: read the section's `class` key, convert it (section 1),
  find the entry for that id and create its server-side object for the section; the new object records the
  id of its section's `class` key. Every engine use of the entry lookup other than the exported client
  operation (9 places) takes the id from a section's `class` key: 8 create server objects, 1 a client object
  (static, [static.py]). Among them are creating simulation objects at a New Game and from saves
  (offline-simulation.md 8.1, sav.md 11.2), whose failures are assertions ("Can't create entity.", "Cannot
  find item with section", and sav.md's "Non-ALife object in the saved game!").

### 6.2 Entries made by scripts (static, [static.py] facts 5)

- **Client side**: the stored client class object is called with no arguments (constructing a script
  object, spec A 2.1). A script error in the call is fatal (`LUA error: …`, as in section 4). The result's
  engine part is taken as the client object, without testing what the result is; a missing object is a fatal
  assertion. The script object's ownership of its engine part is released to the engine (spec A 9 leaves the
  general lifetime open; this release is static here). Then the object's construction runs as for engine
  kinds. What happens for a result without an engine part (spec A 3.1) is **unestablished** (believed: the
  missing object is caught by the assertion).
  **Disputed in review:** "constructing a script object, spec A 2.1" holds only for the 4 client classes that
  are script classes; 48 of the 52 shipped client classes are engine classes (3.3), and calling an engine
  class is not a script-class call of spec A 2.1. What calling an engine class with no arguments constructs and
  returns (and whether each of the 48 has a constructor taking no arguments) is not established here; the
  "engine part" and the ownership release in this bullet are established only for a script-object result.
- **Server side**: the stored server class object is called with one argument, the section name as a
  string. A script error is fatal as above. The result's engine part is taken as the server object, without
  testing what the result is; a missing one is a fatal assertion; ownership is released to the engine as on
  the client side; then the object's initialisation runs, and a missing result of it is a fatal assertion.
  (The shipped server classes take the section as their `__init` parameter, 3.3.)
- A failed conversion of the result reported by the binding library is fatal with
  `LUA error: cannot cast lua value to ` followed by a type name.
- For an entry made by the 3-argument form, the server side is the client class: creating a server object
  of such an id would call the client class with the section and take its result as a server object.
  **Unestablished** what then happens; no shipped section has such an id as its `class` (`MAIN_MNU`,
  `SV_LAST`, `CL_LAST` are named by no `class` key, measured [config_ids.py]).
- Which overrides of the created script objects the engine then calls, and when, is the subject of specs J
  (simulation objects), M (client objects), G1/G5 (the main menu) and Z (the multiplayer game types).

## 7. Where the numbering reaches scripts (static, [static.py] facts 7)

Engine objects keep the number of their own class id (2.4): 7 places look it up and store it in the object
(static; **believed**: while the object loads its configuration). The script members that return such
numbers belong to other specs by the scope rule: the `clsid()` methods of the game object (spec M) and of the
simulation-object classes (spec J), and `ini_file:r_clsid`
(spec F), which returns the number of the id read from a key. Their values are numbers of section 5, so they
compare with `clsid` constants; for an id that is not registered they give the next larger id's number
(2.4).

## 8. Scope accounting

- Listed items owned ([static.py], [numbering.py]): the classes `clsid` and `object_factory`, the 185 listed
  `clsid` constants and the listed method `register` (both forms): 188. The shipped binary registers on these
  classes exactly the 188 constants of 5.3 and the two `register` forms, nothing else (static).
- Names the loadable scripts use from this spec: 101 `clsid` names in 20 scripts, and `register` in the 2
  registrator scripts [uses.py].
- The scoping spec attributed 29 candidate strings to this spec (its section 6); none is a name these
  classes register, since `clsid`'s names come only from the factory entries and `object_factory` registers
  only `register` (static, [static.py] fact 8).
- Entry points (scoping spec 4.5): the `class_registrators` call (section 4).

## 9. Verification facts

All reproduced by `run_all.py` (58 PASS lines, no FAIL):

- Engine entries: 134 (115 client and server, 1 by game type, 15 client, 3 server), 26 more on a dedicated
  server [engine_table.py]; every entry's sides as its kind says, and the two fatal messages [sides.py].
- Shipped registrations: 54 (51 four-argument, 3 three-argument, all 3 conditional on `editor()`); 52
  client classes (48 engine, all registered by the engine under that name), 20 server classes, 24 script
  classes all declared with a base [registrations.py], [client_classes.py].
- Numbering: 188 entries, 188 distinct ids and names; the listing's 185 values equal the rule over its own
  names [numbering.py].
- Script uses: 106 names (101 loadable), 6 missing (3 loadable) [uses.py]; no top-level `clsid` read during
  registration [early.py]; 128 config `class` values, 6 unregistered [config_ids.py].
- Static facts of sections 1–8: [static.py].

## 10. Unestablished

- What creating a server object of an id registered by the 3-argument form does (6.2).
- How the binding library chooses between the two `register` forms in detail, and what error a call matching
  neither raises (3.1; believed, depends on A).
- What the engine executable creates through the exported client operation, beyond the main menu (6.1).
- The meaning of the game-type values that select the `O_ACTOR` kinds (2.2).
- The behaviour for id texts with bytes 80–FF beyond the conversion of section 1 (none is shipped).

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll`, `xrCore.dll`,
  `xrLUA.dll`, `XR_3DA.exe`, `dedicated\XR_3DA.exe` as shipped; the archive file tree of `gamedata.db0` …
  `gamedata.dbd`, extracted with the project's archive tool.
- Analysis: static analysis with Ghidra (headless, read-only) of the factory, its entries and lookups, the
  registration methods, the class-registrator call, the building of the `clsid` table, the creation
  operations and the script-state rebuild in `xrGame.dll`, the id conversion in `xrCore.dll`, and the
  class-constant lookup of the binding library in `xrLUA.dll`; a throwaway scanner of the shipped scripts and
  configuration files. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

1. Client side of a script-made entry whose client class is an engine class (48 of the 52 shipped client
   classes): what the call with no arguments constructs and returns, whether each of those engine classes has a
   constructor taking no arguments, and what is taken as the client object (6.2).
2. The `class_registrators` call (section 4, step 4) can pass nil in place of the factory object; when it
   does is not established.
3. When the script state is rebuilt (2.3), the registrators create a new factory, and creating a factory makes
   sure the script state exists; whether that can start a further startup during the rebuild is not
   established.
