# Script API J: simulation objects

How the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl binaries (Russian edition) present the objects of the offline
simulation to scripts: the 59 engine classes whose names begin with `cse_` and `CSE_AbstractVisual`, their bases
and constructors, their properties and methods, which of their methods a script class can override for the engine,
what the engine's own implementation of each does, and when the engine calls each. This is spec J of the approved
scoping specification `docs/behavior/script-api-scope.md` (section 6, order 16); it owns the entry points of that
document's section 4.5, row J (simulation-object overrides), and answers its gap 2 for these classes.

Builds on (not repeated here):

- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (cited as **A**): script classes,
  `super`, members of script objects (its 4.1, 4.2), how an engine call reaches an override (its 5.2), results and
  the fatal handling of errors there (its 5.3), base-class calls `Base.m(self, …)` (its 6.1).
- Spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md` (cited as **C**): class ids and their
  numbers (its 5, 7), the object factory, the scripted class ids and their server classes (its 3.3), creating a server
  object through a script entry (its 6.2).
- `docs/behavior/offline-simulation.md` (cited as **offline**): where an object is (5), the current level's list and
  the switch pass and rule (6), scheduling, smart-terrain choice, travel target and movement (7), creation and
  registration order (8).
- `docs/formats/sav.md` (cited as **sav**): the spawn packet (7.2), the update packet (7.3), the blocks and their
  fields (7.4.1), the layouts per class id (7.4.2), the scripted state additions (7.5), client data and its rule
  (8), loading (11.2).
- `docs/behavior/character-identity.md` (cited as **identity**): the **choice** of a specific character (its 2) and
  when it runs (its 4.1).
- Spec F, `docs/behavior/script-api-configuration-and-files.md`: the `ini_file` class `spawn_ini` returns. Spec E,
  `docs/behavior/script-api-value-types-and-math.md`: `vector`.
- The classes that the scoping split gives to spec K (`cpure_server_object`, `ipure_schedulable_object`, the brain
  classes `CALifeMonsterBrain` and `CALifeHumanBrain`, `CALifeSmartTerrainTask`) are named here only where a member
  of this spec returns or derives from them.

Conventions:

- Binary: `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), as identified in the script
  environment specification [J-1]. Game files are the winning copies of the root archives `gamedata.db0` …
  `gamedata.dbd`; scripts are the 439 scanned scripts of the scoping spec, **loadable** meaning one of its 250.
- A **simulation object** is an object of the offline simulation (offline; one record of sav section 7). Its engine
  part (A 3.1) is an object of one of this spec's classes. A **scripted** object is one whose class id has a script
  server class (C 3.3): its engine part belongs to a script object of that class. **State flags** are the Object
  block's `u32` state flags (sav 7.4.1); bit values are written in hexadecimal.
- **Dispatched**: an engine call of the method looks for a script override first (A 5.2). **Default**: what
  `Class.m(self, …)` runs (A 6.1).
- Each fact is **static** (from static analysis of `xrGame.dll`, not observed at run time), **measured** (from the
  scripts or file bytes), **believed** (inferred, with the reason) or **unestablished**. No binary was run.
- Checks are Python scripts in the dirty workspace directory `checks/script-api-simulation-objects/`, cited as
  [J-*n*]; `run_all.py` runs them all (27 PASS lines, no failure).

## 1. Scope (measured, [J-2], [J-4], [J-5])

The scoping split gives this spec 148 listed items: 60 classes and 88 declared members of them (scoping 2.1); the
loadable scripts use 56 names of it (with the upper count of scoping section 1: 118 loadable scripts, 160 of all).

Of the scoping spec's 5 candidate strings for J, `m_smart_terrain_id` is a registration: a property of
`cse_alife_monster_abstract` that the listing lacks (3.6). The other four (`child`, `def_ml_enabled`, `pTVItem`,
`st_detail_list_for_community_relations`) are referenced neither by the registration code of these classes nor by
the helpers it calls, so none is a name these classes register.

Gap 2 of the scoping spec (properties the listing lacks): the registration code of the 60 classes registers exactly
the listed members, inherited ones included, plus `m_smart_terrain_id`, minus `OnEvent` (3.12) [J-4]. Spec A 4.3
found no other engine property read by the scripts' classes.

## 2. The classes (static, [J-3])

### 2.1 Bases and constructors

Every listed class is registered under its listed name with exactly the listed bases, in the listed order. 51 classes
have one constructor, which takes one string; 9 have none and can only be used as bases or reached through other
objects: `cse_abstract`, `cse_shape`, `cse_visual`, `cse_motion`, `cse_ph_skeleton`, `cse_alife_schedulable`,
`cse_alife_group_abstract`, `cse_alife_inventory_item`, `cse_alife_trader_abstract`.

| Class | Bases |
|---|---|
| `cse_abstract` | `cpure_server_object` (spec K) |
| `cse_shape`, `cse_visual`, `cse_motion`, `cse_ph_skeleton`, `cse_alife_group_abstract`, `cse_alife_inventory_item`, `cse_alife_trader_abstract` | — |
| `cse_alife_schedulable` | `ipure_schedulable_object` (spec K) |
| `cse_spectator`, `cse_temporary`, `cse_alife_graph_point`, `cse_alife_object` | `cse_abstract` |
| `CSE_AbstractVisual` | `cse_visual`, `cse_abstract` |
| `cse_alife_object_climable` | `cse_shape`, `cse_abstract` |
| `cse_alife_dynamic_object` | `cse_alife_object` |
| `cse_alife_dynamic_object_visual` | `cse_alife_dynamic_object`, `cse_visual` |
| `cse_alife_space_restrictor`, `cse_custom_zone` | `cse_alife_dynamic_object`, `cse_shape` |
| `cse_alife_online_offline_group` | `cse_alife_dynamic_object`, `cse_alife_schedulable` |
| `cse_alife_level_changer`, `cse_alife_team_base_zone` | `cse_alife_space_restrictor` |
| `cse_alife_smart_zone` | `cse_alife_space_restrictor`, `cse_alife_schedulable` |
| `cse_anomalous_zone` | `cse_custom_zone` |
| `cse_torrid_zone` | `cse_custom_zone`, `cse_motion` |
| `cse_zone_visual` | `cse_anomalous_zone`, `cse_visual` |
| `cse_alife_object_projector`, `cse_alife_object_breakable`, `cse_alife_mounted_weapon`, `cse_alife_creature_abstract` | `cse_alife_dynamic_object_visual` |
| `cse_alife_ph_skeleton_object`, `cse_alife_car`, `cse_alife_object_hanging_lamp`, `cse_alife_object_physic` | `cse_alife_dynamic_object_visual`, `cse_ph_skeleton` |
| `cse_alife_helicopter` | `cse_alife_dynamic_object_visual`, `cse_motion`, `cse_ph_skeleton` |
| `cse_alife_item` | `cse_alife_dynamic_object_visual`, `cse_alife_inventory_item` |
| `cse_alife_item_torch`, `_ammo`, `_weapon`, `_detector`, `_artefact`, `_pda`, `_document`, `_grenade`, `_explosive`, `_bolt`, `_custom_outfit` (each `cse_alife_item_` + suffix) | `cse_alife_item` |
| `cse_alife_item_weapon_shotgun`, `cse_alife_item_weapon_magazined` | `cse_alife_item_weapon` |
| `cse_alife_item_weapon_magazined_w_gl` | `cse_alife_item_weapon_magazined` |
| `cse_alife_trader` | `cse_alife_dynamic_object_visual`, `cse_alife_trader_abstract` |
| `cse_alife_creature_crow`, `cse_alife_creature_phantom` | `cse_alife_creature_abstract` |
| `cse_alife_creature_actor` | `cse_alife_creature_abstract`, `cse_alife_trader_abstract`, `cse_ph_skeleton` |
| `cse_alife_monster_abstract` | `cse_alife_creature_abstract`, `cse_alife_schedulable` |
| `cse_alife_monster_zombie` | `cse_alife_monster_abstract` |
| `cse_alife_monster_base` | `cse_alife_monster_abstract`, `cse_ph_skeleton` |
| `cse_alife_psydog_phantom` | `cse_alife_monster_base` |
| `cse_alife_human_abstract` | `cse_alife_trader_abstract`, `cse_alife_monster_abstract` |
| `cse_alife_human_stalker` | `cse_alife_human_abstract`, `cse_ph_skeleton` |

### 2.2 Constructing

The constructor argument is the configuration section (`super(section)` in the shipped server classes, A 3.2, C
3.3). It builds the engine part as an object of that class for that section, reading the section's configuration
keys as the engine does for the same kind (believed: the class's own construction is used; for example a monster
reads `MaxHealthValue`, `going_speed`, `current_level_going_speed`, `terrain` and `rank`, 3.6) [J-27]. The engine part it
builds is the class's **script variant**: for each of the 51 classes with a constructor the binary has one run-time
type that keeps the link to the script object and replaces the dispatched methods of section 4 (A 5.1) [J-10].

When the factory makes a scripted server object (C 6.2), it then calls the object's `init()` (dispatched, 4.1) and
takes its result as the object to use; a missing result is an assertion failure. The engine's `init` returns the
object itself (for some kinds after kind-specific setup, 4.2) [J-15].

## 3. Properties and methods (static)

### 3.1 Properties ([J-6], [J-7])

| Class | Property | Value | Access |
|---|---|---|---|
| `cse_abstract` | `id` | the object's id (spawn packet, sav 7.2), 0 … 0xFFFF | read only |
| `cse_abstract` | `parent_id` | the parent id (spawn packet), 0xFFFF for none | read only |
| `cse_abstract` | `script_version` | the record's script version (spawn packet) | read only |
| `cse_abstract` | `position` | the position (spawn packet), as a new `vector` (a copy) | read and write |
| `cse_alife_object` | `m_game_vertex_id` | game vertex (Object block, offline 5.1), 16-bit | read only |
| `cse_alife_object` | `m_level_vertex_id` | level vertex (Object block), 32-bit unsigned | read only |
| `cse_alife_object` | `m_story_id` | story id (Object block), as a 32-bit unsigned number: a story id of −1 reads 4294967295 | read only |
| `cse_alife_object` | `online` | true while the object has its in-game counterpart (offline 6.3) | read only |
| `cse_alife_creature_abstract` | `team`, `squad`, `group` | the three bytes of the Creature block (sav 7.4.1) | read and write (the number taken as an 8-bit unsigned value, believed) |
| `cse_alife_monster_abstract` | `m_smart_terrain_id` (unlisted) | smart-terrain id (Monster-like block), 0xFFFF for none | read and write (16-bit) |

The 16-bit and 8-bit values read as non-negative numbers. Writing a read-only property of a script object stores a
field of the script object instead (A 4.2); on an engine object it is the error of A 7.3 (believed).

### 3.2 Methods of `cse_abstract` ([J-13], [J-14], [J-11])

- `name()`: the object's name (spawn packet). `section_name()`: its configuration section.
- `clsid()`: the number of the object's class id (C 5, 7).
- `spawn_ini()`: the object's custom data (Object block) as an `ini_file` (spec F), parsed with the game
  configuration folder as its base; made at the first call and kept, so later calls return the same file object.
- `STATE_Read(packet, size)`, `STATE_Write(packet)`: read and write the object state of the spawn packet (sav 7.2,
  7.4.2): the reader is passed the packet positioned at the object state and the **data size** field's value
  (*s*, which counts itself); the writer writes the state after the data-size field, which the engine fills in
  afterwards (and requires to exceed 2 except for the spectator, sav 7.2).
- `UPDATE_Read(packet)`, `UPDATE_Write(packet)`: read and write the update data (sav 7.3).

Called on an object these run the object's own implementation (for a scripted object its override, A 4.1). The
defaults of `cse_abstract` itself (A 6.1) do nothing but write a log line `Attempt to call pure virtual method
<name> in …` naming the method; `UPDATE_Read` and `UPDATE_Write` are registered on `cse_abstract` only, so
`Class.UPDATE_Read(self, …)` on a derived class reaches that default (believed: a derived class's defaults are those
copied from its bases, A 1.3), and so reads or writes nothing.

### 3.3 Methods of `cse_alife_object`: state flags ([J-8], [J-9])

| Method | Engine implementation |
|---|---|
| `can_switch_online()`, `can_switch_offline()` | the predicates *on* and *off* of offline 6.5.2 (with the kind exceptions listed there) |
| `can_switch_online(b)`, `can_switch_offline(b)` | set state flag 0x02 (resp. 0x04) if `b` is true, clear it otherwise |
| `used_ai_locations()` | by kind, as offline 5.1 lists them: state flag 0x80 set for the kinds listed there as "when bit 0x80 is set", constant true or false for the others [J-27] |
| `can_save()` | state flag 0x200 set, except for the kinds sav 7.1 lists separately: the player true, the bolt false, physics objects, skeleton objects, vehicles and helicopters by their skeleton flag 0x08 [J-27] |
| `interactive()` | state flags 0x08, 0x10 and 0x20 all set |
| `visible_for_map()`, `visible_for_map(b)` | state flag 0x400: read; set if `b` is true, clear otherwise |
| `move_offline()`, `move_offline(b)` | true while state flag 0x40 is clear; `b` true clears the flag, false sets it |

The getter and the setter of a name are two overloads chosen by the number of arguments. Only the getters of
`can_switch_online`, `can_switch_offline`, `used_ai_locations`, `can_save` and `interactive` are dispatched (4.1);
the setters, `visible_for_map` and `move_offline` are not. What the engine does with flags 0x40 and 0x400 is
**unestablished** here.

### 3.4 `cse_alife_dynamic_object`

`keep_saved_data_anyway()`, `on_spawn()`, `on_before_register()`, `on_register()`, `on_unregister()`: section 4 (all
dispatched).

### 3.5 Creatures: `cse_alife_creature_abstract` ([J-22], [J-19])

- `alive()`: health (Creature block) > 0. `health()`: the health.
- `g_team()`, `g_squad()`, `g_group()`: the Creature block's team, squad and group bytes (dispatched).
- `on_death(killer)`: section 5.8 (dispatched).

### 3.6 Monsters: `cse_alife_monster_abstract` ([J-22])

- `smart_terrain_id()`: the smart-terrain id (Monster-like block; 65535 for none); `clear_smart_terrain()` sets it to
  0xFFFF; the unlisted property `m_smart_terrain_id` reads and writes the same field (3.1).
- `smart_terrain_task_activate()`, `smart_terrain_task_deactivate()`: set the Monster-like block's "smart-terrain task
  reached" byte to 1, resp. 0.
- `rank()`: the integer value of the key `rank` of the object's section, 0 when the key is absent; fixed at
  construction and not saved.
- `brain()`: the object's brain (class `CALifeMonsterBrain`, spec K), made by the engine's `init` (4.2).
- `update()`: the scheduled update (section 5.7; dispatched).

### 3.7 Characters: `cse_alife_trader_abstract`, `cse_alife_human_abstract` ([J-23], [J-26])

- `rank()`, `reputation()`: the Trader-like block's rank and reputation; `set_rank(n)` sets the rank. Each first makes
  the choice (identity 2; it changes nothing when the specific character is set).
- `community()`: the name of the Trader-like block's community (its position in the community list, identity 1.3),
  without the choice; a position outside the list is fatal (`item by index not found in section %s, line %s`).
- `profile_name()`: the character profile (Trader-like block).
- `cse_alife_human_abstract` registers `brain()` a second time, returning the object's character brain (class
  `CALifeHumanBrain`, spec K); which of the two `brain` overloads a call on a stalker selects is **unestablished**
  (believed: the one taking the more derived class).

### 3.8 Smart zones: `cse_alife_smart_zone` ([J-20], [J-12])

All dispatched: `detect_probability()` (a number), `smart_touch(obj)`, `enabled(obj)` (a boolean), `suitable(obj)` (a
number, kept as a single-precision float), `register_npc(obj)`, `unregister_npc(obj)`, `task(obj)` (a task object of
class `CALifeSmartTerrainTask`, spec K), `update()`. `obj` is a character's or monster's simulation object. The
engine's implementations: `detect_probability` 0; `smart_touch`, `register_npc`, `unregister_npc` and `update` do
nothing; `enabled` false; `suitable` 0; `task` no task.

### 3.9 Anomalies: `cse_anomalous_zone.spawn_artefacts()` ([J-24])

Reads the zone section's keys `min_start_power`, `max_start_power`, `min_artefact_count`, `max_artefact_count` and
`artefacts` (a list of section, weight pairs), draws random numbers, and creates artefact objects by section (as 5.4
creates objects by section) at the zone's location. A created object that is not an artefact is fatal (`Anomalous
zone can't generate non-artefact objects since they don't have an 'anomaly property'!`), as is a section that cannot
be created (`Can't spawn artefact `). The rule that turns the draws into counts, sections, positions and powers, and
the random generator, are **unestablished**.

### 3.10 Items

`bfUseful()` on the 14 item classes other than `cse_alife_item` (dispatched); the engine's implementation returns true
for all 14 [J-27]. When the engine calls it is **unestablished**.

### 3.11 `CSE_AbstractVisual.getStartupAnimation()`

The startup animation name of the object's visual part, or nothing when it has none (believed: an absent name is
passed as nil) [J-24].

### 3.12 Listed but not registered: `OnEvent`

The listing gives `OnEvent(packet, number, number, ClientID)` for `cse_alife_object`, `cse_alife_graph_point`,
`CSE_AbstractVisual`, `cse_spectator`, `cse_temporary` and `cse_alife_object_climable` (and their derived classes).
No class of this spec registers it: the binary's only stand-alone string `OnEvent` is referenced by two functions
outside these registrations [J-4]. A call `obj:OnEvent(…)` therefore finds nothing and fails as a call of a nil value
(believed, A 4.1). `FillProps` and `init` are registered on every class with a constructor.

## 4. Overrides (static)

### 4.1 Which methods are dispatched ([J-10], [J-11])

A script class deriving from one of the 51 constructible classes can override, for the engine, exactly these
methods of its engine base (A 5.2); every other engine call of a method runs the engine implementation even when the
script class defines a method of that name:

| Engine base | Dispatched methods |
|---|---|
| `CSE_AbstractVisual`, `cse_alife_graph_point`, `cse_alife_item`, `cse_alife_object_climable`, `cse_spectator`, `cse_temporary` | `STATE_Read`, `STATE_Write`, `init`, `FillProps` |
| `cse_alife_object` | the above, and `can_switch_online`, `can_switch_offline`, `used_ai_locations`, `can_save`, `interactive` (the getters, 3.3) |
| `cse_alife_dynamic_object`, `cse_alife_dynamic_object_visual`, `cse_alife_ph_skeleton_object`, `cse_alife_space_restrictor`, `cse_alife_level_changer`, `cse_alife_team_base_zone`, `cse_alife_online_offline_group`, `cse_alife_object_projector`, `cse_alife_object_breakable`, `cse_alife_mounted_weapon`, `cse_alife_object_hanging_lamp`, `cse_alife_object_physic`, `cse_alife_car`, `cse_alife_helicopter`, `cse_alife_trader`, `cse_custom_zone`, `cse_anomalous_zone`, `cse_torrid_zone`, `cse_zone_visual` (19) | the above, and `keep_saved_data_anyway`, `on_spawn`, `on_before_register`, `on_register`, `on_unregister` |
| the 14 `cse_alife_item_…` classes (not `cse_alife_item`) | the 19 classes' set, and `bfUseful` |
| `cse_alife_creature_abstract`, `cse_alife_creature_actor`, `cse_alife_creature_crow`, `cse_alife_creature_phantom` | the 19 classes' set, and `g_team`, `g_squad`, `g_group`, `on_death` |
| `cse_alife_monster_abstract`, `cse_alife_monster_zombie`, `cse_alife_monster_base`, `cse_alife_psydog_phantom`, `cse_alife_human_abstract`, `cse_alife_human_stalker` | the creatures' set, and `update` |
| `cse_alife_smart_zone` | the 19 classes' set, and `detect_probability`, `smart_touch`, `enabled`, `suitable`, `register_npc`, `unregister_npc`, `task`, `update` |

Never dispatched for any class: `UPDATE_Read`, `UPDATE_Write`, `name`, `section_name`, `clsid`, `spawn_ini` and the
other non-dispatched members of section 3. In particular `cse_alife_item` is in the first row: a script class deriving
from it cannot override `on_register`, `on_unregister` or any other method of the third row for the engine, although
its objects are dynamic objects; `cse_anomalous_zone` and `cse_zone_visual` have no `update` (they are not scheduled
kinds).

Each dispatched name is registered on its class with a default (A 6.1), so `Base.m(self, …)` in an override runs the
engine implementation of that base class (section 4.2; for `STATE_Read`, `STATE_Write`, `UPDATE_Read` and
`UPDATE_Write` on `cse_abstract`, the logging default of 3.2) [J-11].

### 4.2 Results

A dispatched call keeps one result (A 5.3). By the method: `can_switch_online`, `can_switch_offline`,
`used_ai_locations`, `can_save`, `interactive`, `keep_saved_data_anyway`, `enabled` and `bfUseful` give booleans;
`suitable` and `detect_probability` numbers kept as single-precision floats; `g_team`, `g_squad`, `g_group` numbers
(believed: taken as bytes); `task` a task object; `init` the object to use (2.2); the others none [J-12]. A script
error in any of them ends the game (A 5.3).

The engine implementations not given in section 3, for the 43 classes whose tables were compared (all with these
methods except the two human classes, believed to be the same as their bases'): `keep_saved_data_anyway` is false for
every one; `on_before_register` does nothing except for `cse_alife_online_offline_group` (whose step is
**unestablished**); `on_spawn` is in 5.4; `init` returns the object (monsters first apply the custom data's `[alife]`
`terrain` key, offline 7.4, and make the brain) [J-15], [J-22], [J-27].

## 5. When the engine calls each method (static)

### 5.1 `STATE_Read`, `STATE_Write` ([J-14], [J-17])

Whenever the engine reads or writes an object's spawn packet (sav 7.2): reading the spawn file's records and a save's
object records (sav 11.2 step 4); writing a save (sav 7.1); and creating an object from a spawn record at a New Game
(5.4), where the record's object writes its packet and the new object reads it. That the spawn file's records are
held as objects of their class, so that a scripted record's script object runs both, is believed (the record is
asked to write its packet). The update packet's `UPDATE_Read` and `UPDATE_Write` run the engine implementation
(4.1).

### 5.2 `init` and `FillProps`

`init`: once, right after the factory constructs a server object (2.2). `FillProps(prefix, list)`: when the engine
calls it is **unestablished** (believed: only by an editor's property pages; its list argument is the listing's
editor property list type).

### 5.3 Registration: `on_before_register`, `on_register` ([J-16])

Registering an object with the simulation (offline 8.1 step 3.3, 8.2) calls `on_before_register` first, then makes
the registry entries, and ends with `on_register` only while the simulation is running. At a New Game and on load the
simulation is not running while the objects are registered: after every object is registered (and, on load, the
registries of sav 11.2 step 6 are read), the simulation is marked running and `on_register` is called for every
object of the registry in its order (increasing id; offline 6.1, believed). This is the post-registration step of
offline 8.1 step 4 and sav 11.2 step 7; the engine's `on_register` applies the client-data rule of sav 8 (asking
`keep_saved_data_anyway`). Objects created later (5.4) get `on_register` at the end of their registration.

### 5.4 Creation: `on_spawn` ([J-17], [J-24])

`on_spawn` is called once for an object the simulation creates, after it is registered and its supplies are made:
for every record of the spawn file at a New Game (offline 8.1 step 3), and for every object created by section (the
six callers of that creation are believed to be: items handed out, the artefacts of 3.9, a monster's item below, and
the objects scripts create through spec K). **Disputed in review:** creation by section registers the object only
when its caller asks for it (a yes/no argument of the creation); `on_spawn` follows either way, so an object created
by section is not always registered before its `on_spawn`; which callers skip the registration is unestablished.
Objects read from a save get no `on_spawn`. The engine's `on_spawn`
does nothing except [J-27]:

- anomalies (`cse_anomalous_zone`, `cse_zone_visual`): `spawn_artefacts` (3.9);
- creatures (`cse_alife_creature_abstract`, `_crow`, `_phantom`, `_actor`, `cse_alife_monster_abstract`,
  `cse_alife_monster_zombie`): for kinds without the Trader-like block, the team from the section's `species`; for a
  creature with health ≤ 0, the death time 0 (believed from the decompilation: details **unestablished**);
- `cse_alife_monster_base`, `cse_alife_psydog_phantom`: the creature step, then, if the section has
  `Spawn_Inventory_Item_Section`, possibly one item of that section created as the monster's child (with the
  probability `Spawn_Inventory_Item_Probability`; the draw is **unestablished**).

### 5.5 Release and the end of the simulation: `on_unregister` ([J-18])

Releasing an object from the simulation (believed, from its five callers: scripts through spec K, the removal of dead
bodies of offline 6.4, and the destruction of an online object) first releases its children the same way, then calls `on_unregister` before removing the object from
the registries. When the simulation itself is destroyed, `on_unregister` is called for every object in the registry's
order before the object is deleted. The engine's `on_unregister` removes the map spots of the object's id (believed:
the list it edits is keyed by object id and holds the map locations).

### 5.6 Switching, location and saving

- `can_switch_online`, `can_switch_offline`: in the switch pass, by the rule of offline 6.5.3 (offline 6.5.4 lists the
  shipped replacements).
- `used_ai_locations`: whether the object's location is kept current (offline 5.1, 5.2), and in registration and
  scheduling (5.7).
- `can_save`: which objects a save writes (sav 7.1).
- `interactive`: whether a scheduled update of a stalker or monster does anything (offline 7.1).
- `keep_saved_data_anyway`: the client-data rule (sav 8), from the engine's `on_register`.

### 5.7 Scheduled updates: `update` ([J-21])

A schedulable object (`cse_alife_monster_abstract` and its derived classes, `cse_alife_smart_zone`, and
`cse_alife_online_offline_group`, whose condition is **unestablished**) is entered in the schedule at registration when its kind's condition holds, and
`update()` is its scheduled update (offline 7.1: at most `objects_per_update` per simulation update). Conditions: a
monster or character while it has direct control, uses AI locations, is offline and its health is above 0.001
(offline 7.1); a smart zone while it has direct control, uses AI locations and is offline (smart zones use AI
locations unless a script replaces `used_ai_locations`). The engine's `update` of a monster is offline 7.1's scheduled
update; that of a smart zone does nothing.

### 5.8 Death: `on_death` ([J-19])

When the single-player game's server handles the death of an online creature, it stores the killer's id in the
victim's Creature block and calls the victim's `on_death(killer)` with the killer's simulation object. The engine's
`on_death` sets the death time (Creature block) to the current game time and the health to −1. The simulation's own
killing of an object (the `kill_entity` method of spec K) does not call `on_death`; whether any other path does is
**unestablished**.

### 5.9 Smart terrains ([J-20])

- `enabled`, `suitable`, `register_npc`: in the smart-terrain choice of offline 7.2: for each smart zone in order,
  `enabled(obj)` and, if true, `suitable(obj)`; the largest value strictly above the most negative float is taken
  (of equals the first); `register_npc(obj)` is called on the chosen zone.
- `task(obj)`: at the movement step of offline 7.3, and from five further places used by online characters and
  monsters (believed: their behaviour toward their simulation task).
- `unregister_npc`, `detect_probability`, `smart_touch`: no engine call was found in the simulation's code or among
  the users of an object's smart terrain; that the engine never calls them is believed (the shipped scripts call
  `unregister_npc` themselves).

### 5.10 Creatures' `g_team`, `g_squad`, `g_group`, and `bfUseful`

When the engine calls them is **unestablished**.

## 6. The shipped script classes (measured, [J-25])

21 script classes derive from these classes (20 in loadable scripts; `CSE_NewAttachableItem` in
`se_new_attachable_item.script` is not loadable). Per class, the methods it defines that the engine reaches (4.1),
and those it never reaches:

| Script class (file) | Base | Reached | Never reached by the engine |
|---|---|---|---|
| `se_stalker` (`se_stalker.script`) | `cse_alife_human_stalker` | `STATE_Read`, `STATE_Write`, `can_switch_online`, `can_switch_offline`, `on_before_register`, `on_register`, `on_spawn`, `on_unregister`, `on_death` | — |
| `se_trader` (`se_stalker.script`) | `cse_alife_trader` | `keep_saved_data_anyway` | — |
| `se_monster` (`se_monster.script`) | `cse_alife_monster_base` | `STATE_Read`, `STATE_Write`, `can_switch_online`, `can_switch_offline`, `on_before_register`, `on_register`, `on_unregister`, `on_death` | — |
| `se_car`, `se_heli` (`se_car.script`) | `cse_alife_car`, `cse_alife_helicopter` | `can_switch_online`, `can_switch_offline`; `keep_saved_data_anyway` | — |
| `se_artefact` (`se_artefact.script`) | `cse_alife_item_artefact` | `can_switch_online`, `can_switch_offline` | — |
| `se_outfit`, `se_weapon`, `se_weapon_shotgun`, `se_weapon_magazined`, `se_weapon_magazined_w_gl`, `se_item_torch` (`se_item.script`) | the matching `cse_alife_item_…` class | `on_register`, `on_unregister` | — |
| `se_item` (`se_item.script`) | `cse_alife_item` | — | `on_register`, `on_unregister` |
| `se_physic` (`se_item.script`) | `cse_alife_object_physic` | `keep_saved_data_anyway` | — |
| `se_zone_anom`, `se_zone_visual` (`se_zones.script`) | `cse_anomalous_zone`, `cse_zone_visual` | `STATE_Read`, `STATE_Write`, `on_register` | `update` |
| `se_restrictor` (`se_zones.script`) | `cse_alife_space_restrictor` | `keep_saved_data_anyway` | — |
| `se_smart_terrain` (`smart_terrain.script`) | `cse_alife_smart_zone` | `STATE_Read`, `STATE_Write`, `detect_probability`, `enabled`, `suitable`, `register_npc`, `unregister_npc`, `task`, `on_register`, `on_unregister`, `update` | — |
| `se_respawn` (`se_respawn.script`) | `cse_alife_smart_zone` | `STATE_Read`, `STATE_Write`, `on_register`, `update` | — |
| `cse_switcher` (`se_switcher.script`) | `CSE_AbstractVisual` | `STATE_Read`, `STATE_Write`, `FillProps` | `UPDATE_Read`, `UPDATE_Write` |
| `CSE_NewAttachableItem` (`se_new_attachable_item.script`) | `cse_alife_item` | `STATE_Read`, `STATE_Write`, `FillProps` | — |

Consequences (believed, from the table and the scripts):

- `se_item` is the server class only of `WP_SCOPE` (C 3.3); its `on_register` and `on_unregister` (registering the
  object as a task target and counting items by section) never run for those objects.
- `se_zone_anom:update` and `se_zone_visual:update` never run, so their artefact timer (`last_spawn_time`) stays
  unset and their `STATE_Write` always appends the single byte 0 (sav 7.5). Their base calls
  `cse_anomalous_zone.update(self)` and `cse_zone_visual.update(self)` name a member neither class registers (the
  unlisted names of the scoping spec), which would fail if the methods were called by a script.
- `cse_switcher`'s `UPDATE_Read` and `UPDATE_Write` never run for the engine; `O_SWITCH` is not a simulation object
  (sav 7.4.3). Its `FillProps` calls `properties_helper()`, a name that does not occur in `xrGame.dll`, and
  `create_u32` (the scoping spec's absent name), neither of which the engine provides; that `FillProps` is never
  called in the game (5.2) is believed.
- No shipped script class overrides `init`.

## 7. Verification facts

All reproduced by `run_all.py` (27 PASS lines):

- Scope: 148 listed items (60 classes, 88 members), 5 candidates (1 registration), 56 used names, 118 loadable
  scripts (160 of all) [J-2], [J-5].
- Registration: 60 classes with the listed names and bases, 51 constructors of one string; all listed members by name
  but `OnEvent` (6 classes); 1 unlisted property [J-3], [J-4].
- Dispatch: 51 script variants, 7 sets of dispatched names over 6 / 1 / 19 / 14 / 4 / 6 / 1 classes; every other
  replaced entry of a script variant is a destructor [J-10].
- Script classes: 21 with these bases (20 loadable); 4 never reach 6 of their methods [J-25].

## 8. Unestablished

- What the engine does with state flags 0x40 (`move_offline`) and 0x400 (`visible_for_map`) (3.3).
- When the engine calls `FillProps`, `g_team`, `g_squad`, `g_group`, `bfUseful`, `detect_probability`, `smart_touch`
  and `unregister_npc` (5.2, 5.9, 5.10); the five online callers of `task` (5.9).
- The artefact rule and random generator of `spawn_artefacts` (3.9).
- Which `brain` overload a call on a character selects (3.7); the exact number conversions of the 8-bit and 16-bit
  setters and of `g_team` results.
- Whether the spawn file's record objects are script objects for scripted class ids (5.1); the engine's
  `on_unregister` list (5.5); the creature `on_spawn` details (5.4).
- Everything here is static; none of it was observed in the running game.

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `bin\xrGame.dll` as identified above;
  the archive file tree of `gamedata.db0` … `gamedata.dbd`, extracted with the project's archive tool.
- Analysis: static analysis with Ghidra (headless, read-only) of the registration code of the 60 classes, their
  property accessors and methods, the run-time type information of the classes and their script variants (virtual
  tables compared slot by slot), the engine-to-script dispatch functions, and the simulation's creation,
  registration, New Game, load, release, scheduling, smart-terrain choice and death handling; the whole-program
  decompilation and disassembly made in earlier dirty investigations, searched for callers; a throwaway scanner of
  the shipped scripts. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- `cse_alife_human_abstract` registers `rank()` and `set_rank(n)` itself (the same implementation as the Trader-like
  `rank` of 3.7; `cse_alife_trader_abstract` registers no `set_rank`), so on a character `rank()` is the
  Trader-like rank, not the section-key `rank` of 3.6 (believed: a class's own registration is found before its
  bases'); section 3.7 does not say which `rank` a character's object reaches.
- What makes the character brain that 3.7's second `brain()` returns, and when, is not given.
