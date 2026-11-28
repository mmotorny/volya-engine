# Script API K: simulation registry and game graph

What the simulation part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition), as scripts see it: the global `alife()` and the class `alife_simulator` it returns (finding objects, story
objects, creating and releasing objects, switching, restrictions, killing, information portions of offline objects,
the switch distance); the classes `story_ids` and `spawn_story_ids` and the story ids of the data; the global
`game_graph()` with the classes `CGameGraph` and `GameGraph__CVertex`; the smart-terrain task class
`CALifeSmartTerrainTask`; the brain and movement classes of offline characters (`CALifeMonsterBrain`,
`CALifeHumanBrain`, `CALifeMonsterMovementManager`, `CALifeMonsterDetailPathManager`,
`CALifeMonsterPatrolPathManager`); the six member-less base classes `ipure_…` and `cpure_server_object`; and the two
listed cast functions. This is spec K of the approved scoping specification `docs/behavior/script-api-scope.md`
(section 6, order 17); its scope rule and its review gaps apply: this spec owns every name of these classes as the
game library registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, its one number type, the
  250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (calls of missing members, a
  class's integer constants read as `Class.name`, 4.4; script overrides reached by engine calls, section 5); spec C,
  `docs/behavior/script-api-class-ids-and-the-object-factory.md` (making the server object of a configuration
  section, its section 6); spec E, `docs/behavior/script-api-value-types-and-math.md` (`vector`; argument
  conversions, its 2.2); spec F, `docs/behavior/script-api-configuration-and-files.md`; spec M,
  `docs/behavior/script-api-game-object-basics.md` (`story_id()` and the game objects); spec P,
  `docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md` and `docs/behavior/info-portions.md` (cited
  as `IP`: holding, its section 2).
- `docs/behavior/offline-simulation.md` (cited as `OS`): `game.graph` (its section 1: the level records, the vertex
  records and their fields, accessibility), patrol paths (4), where an object is (5), the current level and its
  list (6.1), switching (6.2–6.5: the switch distances and the switch predicates), offline travel (7: scheduling,
  the smart-terrain choice, the travel target, the path search, movement), creation and registration order (8).
- `docs/formats/sav.md` (the Object block's fields, among them the state flags, story id and spawn story id, 7.4.1;
  the kinds, 7.4.2; the Creature block's dynamic in- and out-restrictions); `docs/formats/ltx-config.md` (how
  configuration values are read).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [K-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from
  the game library's registration code and the code it registers.
- A call is written as scripts write it: `alife():object(id)`. A **number** is the runtime's double; **nil** Lua's
  nil. A **simulation object** is an object of the simulation (`OS`; spec J's classes as scripts see them); its
  **id** is the 16-bit object id (0 … 65,535; 65,535 means none). The **simulation** is the one object `alife()`
  returns.
- **Integer arguments.** Ids, vertex numbers and counts are passed as numbers and made integers by the binding
  library: **believed** truncation toward zero, as spec E 2.2 states for its integer parameters, keeping the low 16
  bits for ids, game vertices and other 16-bit values and 32 bits for level vertices and other 32-bit values.
  Boolean arguments are the runtime's truth value (spec E 2.2). Which overload a call with other arguments selects is
  spec A's.
- **Fatal** means a failed check reported as `console-log` 3.8 describes, after which the program ends. **Reads
  through no object** means the code goes on with a missing object and reads or writes memory through it
  (**believed** to end the program with an access violation; what happens then is not part of this spec).
- Each fact is **static** (from static analysis of the game library, not observed at run time), **measured** (from
  game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [K-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-simulation-registry/run_all.py` (`python3 -I run_all.py`): it reads the game library's bytes,
the evidence of read-only headless static analysis (produced by `gh.py` beside it), the listing, the scoping spec's
scan and the shipped scripts and data. All checks pass (`ALL PASS 36`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 713 listed items [K-2]:

| Owner | Items |
|---|---|
| `alife_simulator` | the class; 22 functions (sections 2–9) |
| `story_ids` | the class; 637 constants (section 4) |
| `spawn_story_ids` | the class; the constant `INVALID_SPAWN_STORY_ID` (section 4) |
| `CGameGraph`, `GameGraph__CVertex` | the classes; 4 and 4 functions (section 10) |
| `CALifeSmartTerrainTask` | the class; 3 functions and 2 constructors (section 11) |
| `CALifeMonsterBrain`, `CALifeHumanBrain` | the classes; 2 functions of the first (section 12.1) |
| `CALifeMonsterMovementManager`, `CALifeMonsterDetailPathManager`, `CALifeMonsterPatrolPathManager` | the classes; 5, 4 and 10 functions (12.2–12.4) |
| `ipure_alife_load_object`, `ipure_alife_save_object`, `ipure_alife_load_save_object`, `ipure_server_object`, `cpure_server_object`, `ipure_schedulable_object` | the classes, without members (section 13) |
| global functions | `alife`, `game_graph`, `cast_alife_object_to_creature`, `cast_alife_object_to_trader_abstract` |

The listing's `CALifeMonsterDetailPathManager` lists two forms of `speed` that the scoping spec's scan did not count
as items because the listing writes a tab after the name; they are this spec's (12.3) [K-2].

The scoping spec assigns this spec three candidate strings [K-2]:

- `can_choose_alife_tasks` is a registered member of `CALifeMonsterBrain` that the listing lacks (12.1) [K-23, K-29];
- `Spawn_Inventory_Item_Probability` is a configuration key the game reads (with `Spawn_Inventory_Item_Section`), not
  a registration [K-29];
- `st_loading_saved_game` is a text id of the loading screen, not a registration [K-29].

### 1.2 As registered (static)

- `alife_simulator` is registered with no base class, **no constructor** and no properties: scripts get its one
  object from `alife()` [K-3]. Its 22 member names in 29 forms are the listing's, each paired with an implementation
  (the forms are given with each member below) [K-3].
- `alife` and `game_graph` are registered at the top level of the scripts' namespace, each taking no argument
  [K-3, K-21].
- `story_ids` and `spawn_story_ids` are registered as holders of integer constants built from the game's
  configuration (section 4); in this edition `story_ids` has 639 constants, two more than the listing [K-4, K-5].
- `CGameGraph`, `GameGraph__CVertex`, `CALifeMonsterBrain`, `CALifeMonsterMovementManager`,
  `CALifeMonsterDetailPathManager` and `CALifeMonsterPatrolPathManager` are registered with no base and no constructor;
  `CALifeHumanBrain` with the base `CALifeMonsterBrain` and no members of its own; `CALifeSmartTerrainTask` with two
  constructors [K-21–K-26]. Their members are the listing's, with two differences: `CALifeMonsterBrain` also has
  `can_choose_alife_tasks` (12.1), and the detail path manager's `speed` is registered under the name `speed`
  followed by a tab character (12.3) [K-23, K-25].
- **Not registered:** `cast_alife_object_to_creature` and `cast_alife_object_to_trader_abstract` do not occur in the
  game library at all [K-28]. A call of either finds no such global (what calling nil does is the runtime's); one
  shipped script calls the second, `test_cast.script`, which is not loadable [K-30].
- No class of this spec has a property (an engine field read as `object.name`), registered or listed, so gap 2 of the
  scoping spec finds nothing here [K-3, K-21–K-27].

## 2. The simulation: `alife()` (static)

`alife()` returns the simulation, an `alife_simulator`, or nil when there is none [K-3]. The simulation is made when
a single-player game server starts (New Game or load) and the game keeps it as its one simulation until it is
destroyed; making it requires a non-empty game or spawn name in the server options and the server options `alife`
and `single`, all in one check ("Invalid server options!" otherwise, fatal) [K-3, K-32]. So `alife()` is nil in the
main menu before a game and **believed** nil after a game ends (the scoping spec's section 5 step 2: the main menu
tests `alife()` before disconnecting). Everything below is a member of that object.

## 3. Finding objects (static)

| Call | Result |
|---|---|
| `alife():object(id)` | the simulation object with that id, or nil when no object of the simulation has it [K-8] |
| `alife():object(id, flag)` | the same; the flag is not read (both forms have the same implementation) [K-8] |
| `alife():object(name)` | the first simulation object, in increasing id (**believed**: the objects are kept ordered by id), whose name equals the text `name` exactly; nil when none does [K-8]. The name is the object's own name: for an object of the spawn file, its record's name (`OS`; sav.md 7.2); for an object made by `create` from a section, as 7.1 states |
| `alife():valid_object_id(id)` | true when `id` is not 65,535, false when it is; it does not ask whether an object has the id [K-7] |
| `alife():actor()` | the player's simulation object (`OS` "the actor"), or nil before it is registered [K-7] |
| `alife():story_object(story_id)` | the simulation object registered with that story id (section 4.3), or nil when the story id is −1 or no registered object has it [K-9] |
| `alife():spawn_id(spawn_story_id)` | the spawn id (the record index in the spawn file, `OS` 8.1) of the record with that spawn story id, as a number (section 4.4) [K-20] |

The objects returned are the simulation objects themselves, not copies: their members (spec J) act on the
simulation's own objects [K-8] (static: the registry entries are returned).

## 4. Story ids

### 4.1 `story_ids` and `spawn_story_ids` (static)

Both classes are built the first time the simulation's classes are registered in a run and kept for the rest of
the run (the lists are built only while they are empty) [K-4]. Each is a holder of integer constants read as
`story_ids.Name` (spec A 4.4; a name the class lacks reads as nil). They are made from a section of the game
configuration (`config\game.ltx`, which includes `config\game_story_ids.ltx` and `config\game_spawn_story_ids.ltx`)
[K-4, K-5]:

| Class | Section | Reserved name |
|---|---|---|
| `story_ids` | `[story_ids]` | `INVALID_STORY_ID` |
| `spawn_story_ids` | `[spawn_story_ids]` | `INVALID_SPAWN_STORY_ID` |

1. If the game configuration has no such section, the game stops (fatal) [K-4].
2. Each line of the section, in the section's order, gives one constant: its **name** is the line's value read with
   its quotes removed (`ltx-config` 7, the unquoted string; e.g. `001 = "Escape_trader_self"` gives `Escape_trader_self`), and its
   **value** is the line's key read as a decimal integer (its leading digits, after an optional sign;
   0 when there are none) [K-4, K-5]. **Believed**: the reader removes the surrounding quotes, as the listing's names, which have none,
   show for the shipped values.
3. Fatal checks on each name, in this order: it contains no space ("Invalid story id description (contains
   spaces)!", for `spawn_story_ids` "Invalid spawn story id description (contains spaces)!"); it is not the reserved
   name ("INVALID_STORY_ID redifinition!", "INVALID_SPAWN_STORY_ID redifinition!"); no earlier line had the same name
   ("Duplicated story id description!", "Duplicated spawn story id description!") [K-4]. Values are not checked:
   two names may have the same value.
4. After the last line, the reserved name with the value −1 [K-4].

### 4.2 In this edition (measured)

- `[story_ids]` has 638 lines, all with decimal keys, distinct names without spaces and distinct values, so
  `story_ids` has **639 constants**: the 638 names and `INVALID_STORY_ID` = −1 [K-5]. Examples: `Escape_blockpost_case`
  = 0, `Escape_Trader` = 3, `Invalid` = 65,535 (the line `65535 = "Invalid"`) [K-5].
- The listing has 637 of them, each with the same value; it lacks **`escape_provodnik` = 92** and
  **`agr_doctor_spot` = 350** [K-5]. **Correction to spec P 3** (the task file's `object_story_id`): those two texts
  are constants of the registered `story_ids`, so they name the objects with story ids 92 and 350 (the records
  `esc_provodnik` of `l01_escape` and `kat_stalker` of `l03u_agr_underground`), not story id 0 [K-5, K-6].
- `[spawn_story_ids]` has no lines: `spawn_story_ids` has the one constant `INVALID_SPAWN_STORY_ID` = −1 [K-5].
- No shipped script reads `story_ids` or `spawn_story_ids` [K-30]; scripts pass story ids as numbers (from custom
  data and the scripts themselves) to `story_object`.

### 4.3 Story objects (static)

Every simulation object has a story id (sav.md 7.4.1; −1 for none). When an object is registered with the simulation
(`OS` 8.1 step 3.3, and creation in section 7), an object whose story id is not −1 is entered in the simulation's
**story registry** under it; if an object is already registered under that story id, the game stops ("Specified
story object is already in the Story registry!", fatal) [K-9]. When an object is unregistered (release, section 7.3)
its entry is removed; an object whose story id has no entry gives the log message "Cannot find story object with id
[%d] in the Story registry!" and nothing else [K-9]. `story_object` looks the registry up (section 3).

In `spawns\all.spawn`, 503 of the 8,641 records have a story id, all different; 498 of them are values of
`story_ids` constants, and 5 (19, 736, 737, 738, 740) are not [K-6]. For example the trader `escape_trader` of
`l01_escape` has story id 3 (`story_ids.Escape_Trader`) [K-6].

### 4.4 Spawn story ids (static)

The spawn file's records also carry a spawn story id (sav.md 7.4.1). `spawn_id(spawn_story_id)` searches the
simulation's table of (spawn story id, spawn id) pairs, kept ordered by spawn story id, and returns the spawn id of
the entry with that key [K-20]. For a key the table lacks it returns the 16-bit value just past the table's last
entry: **unestablished** (it is not a spawn id). No record of the shipped spawn file has a spawn story id, so in
this edition every call is of that kind; no shipped script calls `spawn_id` [K-6, K-30].

## 5. Levels: `level_id()`, `level_name(id)` (static)

- `alife():level_id()` returns the level id (`OS` 1.2) of the current level (`OS` 6.1) [K-7]. Before the current
  level is fixed (the actor's registration) it reads through no object [K-7].
- `alife():level_name(id)` returns the name (`OS` 1.2: the level's directory name, e.g. `l01_escape`) of the level
  record of `game.graph` with that level id [K-7]. For an id that no record has, the game stops ("there is no
  specified level in the game graph : %d" with the id, fatal) [K-7]. It works without a current level (it reads only
  the graph).

## 6. Switching (static)

### 6.1 The switch distance

- `alife():switch_distance()` returns the switch distance *d* of `OS` 6.5.1 [K-11].
- `alife():switch_distance(v)` sets *d* to `v` and both derived distances with the switch factor *f* (`OS` 6.5.1:
  0.1): online distance *v* · (1 − *f*), offline distance *v* · (1 + *f*), in single precision [K-11]. They apply from
  the next switch pass (`OS` 6.2; **believed**: the pass reads the stored distances). No shipped script calls either form [K-30].

### 6.2 `set_switch_online(id, flag)`, `set_switch_offline(id, flag)`, `set_interactive(id, flag)`

Each sets (flag true) or clears (flag false) one bit of the state flags of the simulation object with id `id`
(sav.md 7.4.1; `OS` 6.5.2) [K-10]:

| Member | Bit | Effect (`OS`) |
|---|---|---|
| `set_switch_online` | 0x02 | the object may switch online: the predicate *on* of 6.5.2 |
| `set_switch_offline` | 0x04 | the object may switch offline: the predicate *off* of 6.5.2 |
| `set_interactive` | 0x08 | one of the three bits a stalker's or monster's scheduled update requires (7.1); its other uses are **unestablished** here |

**Believed**: the field written is the Object block's state flags, as its bits are the ones `OS` 6.5.2 and 7.1 read.
Nothing else is done: the object is not switched by the call, the next switch pass applies the rule (`OS` 6.5.3). The
id is not checked: for an id that no object has, the call reads through no object [K-10]. The bits are saved with
the object (sav.md 7.4.1), so the change lasts. Scripted classes can replace the predicates (`OS` 6.5.4), which may
then ignore these bits.

## 7. Creating and releasing (static)

### 7.1 `create(section, position, level_vertex, game_vertex[, parent_id])`

`section` is a text, `position` a `vector` (level coordinates, `OS` conventions), `level_vertex` a 32-bit and
`game_vertex` and `parent_id` 16-bit numbers [K-13]. The four-argument form is the five-argument form with no
parent. With no parent, or with `parent_id` −1 (65,535):

1. The server object of the configuration section `section` is made (spec C section 6); if there is none, the game
   stops ("Cannot find item with section", fatal) [K-13].
2. It takes the next free id of the simulation's id pool (`OS` 8.1); if none is left, the game stops ("Not enough
   IDs", fatal) [K-13]. Its parent id is `parent_id` (65,535 for none), its spawn id none (65,535), its position,
   game vertex and level vertex the arguments [K-13]. Nothing checks that the level vertex belongs to the game
   vertex's level, or that the position lies near it.
3. Its **name** becomes the section name followed by the id in decimal, padded with zeros to four digits (ids below
   1,000; e.g. section `wpn_ak74`, id 57: `wpn_ak740057`) [K-13]. **Believed**: this is the name `object(name)` and the
   object's members read.
4. It is **registered** with the simulation as an object created at New Game is (`OS` 8.1 step 3.3: object
   registry, location, story registry of 4.3, schedule, smart terrains), with the script calls of 7.4 [K-13, K-17].
5. Its supplies (the objects its configuration and custom data make it carry, `OS` 8.1 step 3.4) are created
   (**believed**: the step that New Game's creation runs in the same place, which an object of a group kind replaces
   by creating its members, 7.2), then its `on_spawn` is called (7.4) [K-13, K-17].
6. The call returns the new simulation object [K-13].

The new object is offline. Whether it comes online is decided by the next switch passes (`OS` 6) [K-13] (static:
nothing in the call switches it).

With a `parent_id` other than −1 [K-13]:

- if no simulation object has that id, the log message "! invalid parent id [%d] specified" (with the id) is given
  and the call returns nil; nothing is created;
- if the parent is offline, as above with that parent id: the object is created offline in the parent;
- if the parent is **online**, the object is made as in steps 1–3 but not registered by this call; instead its
  spawn data is written into a spawn message of the game's server (the kind of message a networked spawn arrives
  in) and handed to the server, and the call returns the server's result. Before it is handed over, the new
  object's id is put back as free into the id pool of step 2, which is the game server's id pool [K-34]. The
  server's handling of a spawn message with respawn time 0 (sav.md 7.2) and flags bit 0x10 clear takes the
  message's id from that pool (fatal "Requesting ID has already been used!" if it is not free) [K-34]; the object
  made here has respawn time 0 and flags 0 when it is constructed, and this call changes neither (**believed** for
  every kind: only the common part of the constructors was examined), so the server's object gets the id step 2
  gave [K-34]. **Believed**: the server registers the object and brings it online in the parent at once, as for
  any spawn message (the rest of the server's handling is not part of this spec).

### 7.2 `create(spawn_id)` and `create_ammo(section, position, level_vertex, game_vertex, parent_id, count)`

`create(spawn_id)` creates the object of the spawn file's record with that spawn id (`OS` 8.1: the record index)
[K-14]:

1. The record's object is made from the record's class (`class` of its configuration section, spec C) and read from
   the record's packets; if none can be made, the game stops ("Cannot find item with section"), and if it is not a
   simulation object, "Non-ALife object in the 'game.spawn'" (fatal) [K-14]. If the object uses the level graph
   (`OS` 5.1) and its level vertex is invalid, "Invalid vertex for object " with the object's name (fatal) [K-14].
2. Its spawn id is the record's. Its id is 0 if it is the player's object and no actor exists yet, else the next free
   id ("Not enough IDs" when none is left, fatal) [K-14].
3. It is registered as in 7.1 step 4; then an object of a group kind creates its members, any other its supplies;
   then its `on_spawn` is called [K-14, K-17]. The call returns the new object [K-14].

A spawn id that the spawn file lacks reads through no object [K-14]. No shipped script calls this form [K-30].

`create_ammo` is the five-argument `create`, after which the new object's rounds count (the ammunition's `u16`
rounds left, `OS` 6.5.2) is set to `count` [K-15]. With a parent id of −1 and a creation that gives no object, it
writes through no object [K-15]. For an online parent the count is set before the object is handed to the server
[K-15]. **Believed**: `section` must name ammunition; for another kind the count is written into a different part
of the object (**unestablished** what that does).

### 7.3 `release(object, flag)`

Removes the simulation object `object` from the simulation; `flag` is not read [K-16]. For nil it reads through no
object [K-16].

- **Offline object:** each object it carries (its children) is released first, in the same way, recursively; then
  the object is unregistered (7.4: `on_unregister` first; then it leaves its parent, the object registry, the story
  registry, the smart-terrain and other registries, its location and the schedule) and destroyed [K-16, K-17]. Its id
  is no longer in use (**believed**: destroying returns it to the id pool).
- **Online object:** a destroy event for the object's id, stamped with the server's time, is sent to the game; the
  call does nothing else [K-16]. **Believed**: the game destroys the online object, which then leaves the simulation
  as an offline object does (the event's handling is not part of this spec); when, is **unestablished**.

### 7.4 Registration and the script overrides it reaches (static)

When a simulation object is registered (by `create`, and at New Game and load, `OS` 8), the steps run in this order
[K-17]:

1. its `on_before_register` (spec J; a scripted class's override, spec A section 5);
2. the simulation's registries (`OS` 8.1 step 3.3; story registry 4.3);
3. its `on_register`, **only** while a flag of the simulation is set [K-17]. The flag is set when the simulation is
   made [K-17]. **Believed**: it is clear while a New Game or a load creates its objects, since `OS` 8.1 step 4 and
   sav.md 11.2 step 7 run the post-registration step later in a pass of their own; so an object created by a script
   during play gets its `on_register` during `create`.

Unregistering (release) calls the object's `on_unregister` first [K-17]. The creation calls of 7.1 and 7.2 call the
object's `on_spawn` last [K-17]. For example, `se_stalker.script`'s `on_register` calls `self:brain():update()`
(12.1) when the stalker is alive and has no smart terrain, so a stalker created by a script chooses a smart terrain
during `create` [K-30].

## 8. Restrictions (static)

The listed forms take a creature's simulation object and a restrictor's id: `add_in_restriction(monster, id)`,
`add_out_restriction(monster, id)`, `remove_in_restriction(monster, id)`, `remove_out_restriction(monster, id)`;
`remove_all_restrictions(object_id, type)` takes an object id and a number [K-12]. They edit the creature's
**dynamic in-restrictions** and **dynamic out-restrictions** (sav.md 7.4.1, Creature block: two lists of restrictor
ids) [K-12]:

1. The creature is looked up again by its id. If no simulation object has that id, the log message "! cannot add
   restriction with id %d to the entity with id %d, because there is no creature with the specified id" (for the
   removing forms "! cannot remove restriction …") is given and nothing else is done; likewise, in this order, when
   no object has the restrictor's id ("… because there is no space restrictor with the specified id"), when the
   creature's object is not a creature ("… because there is an object with the specified id, but it is not a
   creature"), and when the restrictor's object is not a restrictor ("… but it is not a space restrictor") [K-12].
   **Believed**: a restrictor here is an object of a kind with the Shape block and a restrictor type (sav.md
   7.4.2: restrictor, zone, anomaly kinds, smart zone, level changer, team base).
2. `add_in_restriction` appends the restrictor's id to the in-restrictions, `add_out_restriction` to the
   out-restrictions, without checking for an equal entry (an id can be in a list twice) [K-12].
3. `remove_in_restriction` and `remove_out_restriction` remove the first equal entry of the in- or out-restrictions,
   keeping the order of the others; when the list has none, the log message "~ cannot remove restriction with id
   [%d][%s] to the entity with id [%d][%s], because it is not added" (ids and names) is given [K-12].
4. `remove_all_restrictions(object_id, type)` gives "! cannot remove restrictions to the entity with id %d, because
   there is no creature with the specified id" or "! cannot remove restriction to the entity with id %d, because
   there is an object with the specified id, but it is not a creature" in the same cases; otherwise it empties the
   in-restrictions when `type` is 4 and the out-restrictions for **any other** `type` [K-12]. (The registered type
   numbers: 4 in, 5 out [K-12]; the constants naming them are not this spec's.)

The lists are those of the simulation object; how an online creature's movement learns of the change is
**unestablished** here. No loadable script calls these members [K-30].

## 9. `kill_entity` (static)

Three forms: `kill_entity(monster, game_vertex, killer)`, `kill_entity(monster, game_vertex)` (no killer) and
`kill_entity(monster)` (no killer, the monster's own game vertex) [K-18]. `monster` is a stalker's or monster's
simulation object, `killer` a schedulable simulation object (spec J's `cse_alife_schedulable`) [K-18]. In order:

1. The objects the monster carries that are items are collected in a list of the simulation's [K-18] (their use is
   **unestablished** here).
2. The monster's health (Creature block: the value `alive()` and `health()` of spec J read) is set to 0 [K-33].
   Then its **death place** is set. A killer is asked whether it is an anomalous zone; only objects of
   `cse_anomalous_zone` and `cse_zone_visual` (and the classes deriving from them) answer yes, none of these is a
   schedulable object, and every schedulable class of the game library answers no [K-33].
   Without a killer, or with one that answers no, its game vertex becomes
   `game_vertex`, and its position, level vertex and distance those of one of that vertex's level points (`OS` 1.5),
   chosen with the simulation's own random generator, a linear congruential generator (below) [K-18]. A vertex with
   no level points takes index 0 without a draw, that is the 20 bytes at its level points offset (`OS` 1.3), which
   belong to a later vertex's level points or lie past the table (what is then read is **unestablished**); every
   vertex of this edition has at least one level point (measured) [K-33]. **Disputed in review:** that those 20
   bytes belong to a later vertex's level points or lie past the table is unbacked: where the level points offset
   of a vertex without level points refers is not established. If the vertex
   is on the current level and that level vertex is not a valid vertex of the level, the game stops ("Invalid vertex",
   fatal) [K-18]. Two 16-bit game-vertex fields of a monster-like object are set to the new game vertex (**believed**:
   the location fields of sav.md 7.4.1's Monster-like update) [K-18]. A killer that answers yes would get another
   rule, which no killer reaches (its details **unestablished**) [K-33].

   The generator: *s* ← *s* · 0x8088405 + 1 modulo 2³², the point index being the high 32 bits of *n* · *s*, *n* the vertex's number of level points [K-18]. <!-- spec-lint: constant -->
3. A step of the monster's own runs; every class of the game library that has the monster part (stalkers,
   monsters, monster groups and the script classes deriving from them) has the same one, which does nothing
   [K-18, K-33].
4. If the monster still carries any object, the game stops (fatal) [K-18].
5. If it is a member of a group it is taken out of the group; if its game vertex changed, its location is updated
   as for a relocation (`OS` 5.2); a schedulable monster is entered in a list of the simulation's [K-18]. These
   steps' details are **unestablished**.

After the call the monster's health is 0, so its `alive()` (spec J: health > 0) is false [K-33]; whether anything
else follows from the health of 0 is **unestablished**. No loadable script calls it (one non-loadable script
does) [K-30].

## 10. The game graph (static)

### 10.1 `game_graph()`

Returns the game graph, a `CGameGraph`: the contents of `game.graph` (`OS` section 1) [K-21]. It is loaded when the
game's AI data is first needed, so `game_graph()` loads it if no one has; it is never nil in the single-player game
(believed: the load does not fail without being fatal) [K-21]. It is the same object for the whole run.

### 10.2 `CGameGraph`

| Call | Result |
|---|---|
| `g:vertex(v)` | the vertex record `v` (`OS` 1.3), a `GameGraph__CVertex`; `v` is not checked: for `v` ≥ *V* the result refers past the vertex table (**unestablished** what its members then give) [K-21] |
| `g:valid_vertex_id(v)` | true when `v` < *V*, the vertex count (`OS` 1.1: 3,512) [K-21] |
| `g:vertex_id(vertex)` | the number of a vertex record obtained from `vertex` [K-21] |
| `g:accessible(v)` | the vertex's accessibility flag (`OS` 7.4) [K-21] |
| `g:accessible(v, flag)` | sets (true) or clears (false) that flag; `OS` 7.4 states what it changes (the path search) and that it is not saved [K-21] |

`accessible` does not check `v` either: for `v` ≥ *V* it reads or writes a flag past the vertices' flags [K-21].
Changing a flag does not affect paths already found (**believed**, from `OS` 7.4: a path is searched again only when
the object has none or it does not lead to the target).

### 10.3 `GameGraph__CVertex`

The four members read the vertex record (`OS` 1.3) [K-21]:

| Call | Result |
|---|---|
| `vertex:level_point()` | a `vector`: the level point |
| `vertex:game_point()` | a `vector`: the global point |
| `vertex:level_id()` | the level id of the vertex's level |
| `vertex:level_vertex_id()` | the vertex's own level vertex |

For example `smart_terrain.script` names a smart terrain's level with
`alife():level_name(game_graph():vertex(self.m_game_vertex_id):level_id())` [K-30].

## 11. Smart-terrain tasks: `CALifeSmartTerrainTask` (static)

A task names a point of a patrol path of the spawn file (`OS` 4) [K-22]:

- `CALifeSmartTerrainTask(path_name)`: point 0 of the patrol path named `path_name` (exact text);
- `CALifeSmartTerrainTask(path_name, point)`: point `point` of that path.

The path is looked up when the task is made [K-22]. If no patrol path has that name, it reads through no object; if
the path has no point `point`, the task refers to no point and its members read through no object (**believed**
from the result of the point lookup) [K-22].

| Call | Result |
|---|---|
| `task:game_vertex_id()` | the point's game vertex (`OS` 4) |
| `task:level_vertex_id()` | the point's level vertex |
| `task:position()` | a `vector`: the point's position |

The engine asks a smart terrain for a task with the smart-terrain object's `task(object)` override (spec J; `OS` 7.3)
and makes the task's point the character's travel target (12.1, 12.3) [K-22, K-23]. The shipped
`se_smart_terrain:task` (`smart_terrain.script`) returns `CALifeSmartTerrainTask(path_name)` with the job's path, and
`CALifeSmartTerrainTask("simulation_default_path")` when the character has no job path [K-30]; that path has one
point, on game vertex 1,545 (the first vertex of `l07_military`) [K-31].

## 12. Brains and movement of offline characters (static)

A stalker's or monster's simulation object has a **brain** (`CALifeHumanBrain` for stalkers, believed from the
class names; `CALifeMonsterBrain` otherwise), reached through the object's `brain()` member (spec J) [K-23]. The
brain has a movement manager, which has a detail path manager and a patrol path manager [K-23, K-24]. They implement
`OS` 7.1–7.5; this section gives what scripts reach.

### 12.1 `CALifeMonsterBrain` and `CALifeHumanBrain`

- `brain:movement()` returns the brain's movement manager [K-23].
- `brain:can_choose_alife_tasks(flag)` (registered, not listed) sets the per-object setting of `OS` 7.2 that allows
  the smart-terrain choice; it starts on [K-23]. No shipped script calls it [K-30].
- `brain:update()` runs, in this order [K-23]:
  1. the smart-terrain choice of `OS` 7.2: only when the object has no smart terrain, the setting is on, and the
     game time has reached the time of the last look plus `smart_terrain_choose_interval`; it records the time,
     asks every smart terrain in the simulation's order whether it accepts the object (its `enabled`) and how
     suitable it is (its `suitable`), takes the one with the greatest suitability (starting from −3.4028235 × 10³⁸;
     of equals the first) and, only if one was chosen, lets it take the object (its `register_npc`) and resets the
     time of the last look to 0 [K-23, K-35]; when none accepts the object (or none is more suitable than the
     starting value), nothing follows the recording of the time, which then stays the time of this look [K-35];
  2. if the object still has no smart terrain: the movement manager's path type becomes 3 and its update runs, which
     does nothing for that type (12.2) [K-23];
  3. otherwise: the smart terrain's `task(object)` gives the task (section 11); the movement manager's path type
     becomes 0, the detail path manager's target becomes the task's point (12.3), and the movement manager's update
     runs [K-23].

  It does not test the state-flag and health condition of `OS` 7.1, which the scheduled update tests before it
  [K-23] (static: the member's code tests neither). The shipped scripts call it in two places: `se_stalker.script`'s
  `on_register` (7.4) and `se_respawn.script` after creating an object [K-30].

`CALifeHumanBrain` has the members of `CALifeMonsterBrain` only [K-23].

### 12.2 `CALifeMonsterMovementManager`

| Call | Result |
|---|---|
| `m:detail()` | the detail path manager (12.3) |
| `m:patrol()` | the patrol path manager (12.4) |
| `m:path_type()`, `m:path_type(t)` | the path type, a number; set it |
| `m:actual()`, `m:completed()` | always true (the registered code returns true without reading anything) |

[K-24]. Its **update** (run by `brain:update()`): for path type 0, the detail path manager's update; for path type 2,
the patrol path manager's update, then the detail path manager's target becomes the patrol path manager's current
point (game vertex, level vertex, position), then the detail path manager's update; for any other type, nothing
[K-23, K-24].

### 12.3 `CALifeMonsterDetailPathManager`

| Call | Result |
|---|---|
| `d:target(game_vertex, level_vertex, position)` | sets the target to these values |
| `d:target(game_vertex)` | sets the target to that game vertex, its own level vertex and its level point (`OS` 1.3) |
| `d:target(task)` | sets the target to the task's point (section 11) |
| `d:completed()` | true when the object's game vertex and level vertex are the target's |
| `d:actual()` | true when the object has a path (`OS` 7.4) and it leads to the target's game vertex (**believed**: the entry it compares with the target is the path's destination) |
| `d:failed()` | true when the object has no path |

[K-25]. Its **update** is the movement update of `OS` 7.5 towards the target, run only when the game time is past
the travel clock [K-23, K-25].

The two forms of the travel speed are registered under the name `speed` followed by a tab character, as the listing
writes them; `d:speed(v)` therefore finds no member (spec A), and only `d["speed\t"](d, v)` and `d["speed\t"](d)`
reach them [K-25]. They set and return the stored speed (single precision), but every travel step first sets the
stored speed from the object's `current_level_going_speed` or `going_speed` (`OS` 7.5) and then uses it, so a value
set by a script has no effect on travel [K-25].

### 12.4 `CALifeMonsterPatrolPathManager`

Not used by the movement manager unless its path type is 2, which no shipped script sets [K-24, K-30].

| Call | Result |
|---|---|
| `p:path(name)` | sets the patrol path (`OS` 4) named `name`; none when no path has that name. If the manager is started and the path is the one it had, it stays started; otherwise it is not started |
| `p:start_type(t)`, `p:start_type()` | the start type (below); set it |
| `p:route_type(t)`, `p:route_type()` | the route type: 0 stops at a dead end, any other value turns back (below) |
| `p:use_randomness(flag)`, `p:use_randomness()` | whether the next point is chosen at random |
| `p:start_vertex_index(i)` | the point to start from for start type 3 |
| `p:actual()` | whether the manager is started |
| `p:completed()` | whether it is started and has reached a dead end with route type 0, or a point without links |
| `p:target_game_vertex_id()`, `p:target_level_vertex_id()`, `p:target_position()` | the current point's game vertex, level vertex and position (a `vector`) |

[K-26]. Its **update**, when it has a path [K-26]:

1. If it is not started, it starts: no previous point, started, not completed, and the current point by start type:
   0 point 0, 1 the last point, 2 the **nearest** point, 3 point `start_vertex_index` [K-26]. Other start types are
   **unestablished** (the choice reads past its four cases). The nearest point is the first point (in point order)
   on the object's game vertex, or else the point whose game vertex's global point is closest to the global point of
   the object's game vertex (the first of equals) [K-26]. If it is started and completed, the update ends.
2. If the object is at the current point (its game vertex and level vertex are the point's), the next point is
   chosen [K-26]: the **candidates** are the current point's links (`OS` 4) whose target is not the previous point.
   - With candidates: the candidate with index *r* is taken, *r* being 0, or with randomness a random value (the
     object's own generator, **believed**) modulo the number of candidates; the current point becomes its target and
     the old current point the previous one [K-26].
   - Without candidates, with route type 0 or when the point has no links: the manager is completed [K-26]. The
     choice then still runs and reads past the point's links: the point it takes is **unestablished**.
   - Without candidates otherwise, it **turns back**: the current point becomes the target of the dead end's first
     link, and the previous point the point before the dead end [K-26].

   With randomness, a choice without candidates divides by zero (**believed** fatal) [K-26].

The members that read the current point read through no object when there is none (before the first start, or for
an invalid start index) (**believed**).

## 13. The member-less classes (static)

`ipure_alife_load_object`, `ipure_alife_save_object`, `ipure_alife_load_save_object` (bases: the first two),
`ipure_server_object` (base: `ipure_alife_load_save_object`), `cpure_server_object` (base: `ipure_server_object`) and
`ipure_schedulable_object` are registered with no members and no constructors, with the bases the listing gives
[K-27]. They only make spec J's classes' bases complete: `ipure_schedulable_object` is registered as the base of
`cse_alife_schedulable` (spec J) [K-27]. Scripts cannot create them or call anything on them.

## 14. Uses in the shipped scripts (measured)

By the scan of `kuses.py` (calls on `alife()` and on names assigned from it, `game_graph()` and its vertices,
`:brain():update()`, the constructors, `story_ids.`, `spawn_story_ids.`; comments removed) [K-30]:

| Name | Loadable scripts (all) | Calls |
|---|---|---|
| `alife()` | 32 (60) | |
| `create` (all forms) | 8 (29) | 311 |
| `story_object` | 13 (16) | |
| `object` | 11 (15) | |
| `set_switch_online`, `set_switch_offline` | 3 (8) each | |
| `release` | 3 (3) | |
| `create_ammo` | 2 (2) | |
| `actor` | 2 (2) | |
| `has_info`, `level_id`, `level_name` | 1 (1) each | |
| `set_interactive`, `kill_entity` | 0 (1) each | |
| `game_graph()` | 4 (4) | |
| `vertex` | 3 (3) | |
| `level_id` (on a vertex) | 2 (2) | 3 |
| `accessible` | 1 (1) | |
| `CALifeSmartTerrainTask` | 1 (1) | 2 |
| `brain():update()` | 2 (2) | 2 |
| `cast_alife_object_to_trader_abstract` | 0 (1) | |

No shipped script uses `story_ids`, `spawn_story_ids`, `switch_distance`, the restriction members, `spawn_id`,
`valid_object_id`, `dont_has_info`, `vertex_id`, `valid_vertex_id`, the movement, detail and patrol path managers or
`can_choose_alife_tasks` [K-30]. `IP` 2 gives the use of `has_info` by `_g.script`.

## 15. `has_info(id, info_id)`, `dont_has_info(id, info_id)` (static)

`alife():has_info(id, info_id)` is true when the simulation's information-portion registry has a list for object
`id` and that list has an entry with id `info_id` (a text, compared as the game's shared strings are, by value);
false when the object has no list [K-19]. This is `IP` 2's *holds* for any object id. `dont_has_info` is its
negation [K-19].

## 16. Verification facts

- Game library identity [K-1]; 713 listed items, the three candidates [K-2].
- Registered: `alife_simulator` 22 names, 29 forms; `CGameGraph` 4 names, 5 forms; `GameGraph__CVertex` 4;
  `CALifeSmartTerrainTask` 3 and 2 constructors; `CALifeMonsterBrain` 3 (with `can_choose_alife_tasks`);
  movement manager 5 names, 6 forms; detail path manager 5 names, 7 forms (with `speed` + tab); patrol path manager
  10 names, 13 forms; six member-less classes; `alife`, `game_graph`; the casts absent [K-3, K-21–K-28].
- Story ids: 638 configuration lines, 639 constants, the listing 637; records: 503 with a story id, none with a
  spawn story id [K-5, K-6].
- Patrol paths of the spawn file: 4,294; `simulation_default_path` one point on game vertex 1,545 [K-31].

## 17. Unestablished

- What a member does after it reads or writes through no object (section conventions), and what `vertex(v)` and
  `accessible(v)` give for `v` ≥ *V*.
- `spawn_id` for a key the table lacks (4.4).
- Whether the simulation's registration flag is clear while a New Game or a load creates its objects (7.4, believed).
- When an online object sent the destroy event by `release` leaves the simulation (7.3).
- How an online creature learns of changed restrictions (section 8).
- `kill_entity`: the use of the collected items, the rule for a killer that answers yes (reached by no killer), what
  is read for a vertex with no level points, the details of step 5, and what else follows from the health of 0
  (section 9).
- What `create_ammo`'s count does for a section that is not ammunition (7.2).
- Patrol path manager: start types other than 0–3; the next point when there are no candidates (12.4).
- `set_interactive`'s bit beyond `OS` 7.1 (6.2).
- The scan of section 14 attributes calls by the receiver patterns it names; calls through other names (tables of
  objects, function arguments) are not counted.

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` as identified above;
  the winning copies of the root archives' files as extracted by the project's archive tool (`config\`, `scripts\`,
  `spawns\all.spawn`).
- Analysis: static analysis of `xrGame.dll` with Ghidra in headless, read-only mode (decompiling and disassembling
  the registration code of this spec's classes and the code it registers, the simulation's creation, registration
  and release code, the game server's handling of spawn messages and its id pool, and reading constants and strings from the file); a reader of `spawns\all.spawn` (records and
  patrol paths, as approved specs describe them); a reader of `game.graph` (its vertex records); a reader of the two story-id configuration files; a token scan of
  the 439 scripts. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.
