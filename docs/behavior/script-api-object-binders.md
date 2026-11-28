# Script API L: object binders

How the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl binaries (Russian edition) attach a script object, a
**binder**, to an object of the level, and when and in which order they call the binder's methods during the
object's life. This is spec L of the approved scoping specification `docs/behavior/script-api-scope.md`
(section 6, order 30). It owns the engine class `object_binder`, the game-object members `bind_object` and
`binded_object`, the configuration key `script_binding` with the binding functions it names, and the binder
overrides (the scoping spec's section 4.5, row L): their arguments, results, timing and order, which section 5
of the scoping spec leaves unestablished. It also answers spec A's review gap 1 for this class.

Builds on (not repeated here):

- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (cited as **A**): script
  classes, `super`, how an engine call reaches an override (its 5.2), the fatal handling of errors there (its
  5.3), base-class calls (its 6.1).
- Spec B, `docs/behavior/script-api-engine-services.md` (cited as **B**): the game type and
  `IsGameTypeSingle` (its 4.5).
- Spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md` (cited as **C**): class ids, the client
  class of each (its 3.3), the resolution of a name to a function (its 3.2).
- Spec D, `docs/behavior/script-api-engine-entry-points.md` (cited as **D**): finding and calling a script
  function by name (its 1.1, 1.2, 1.3), the New Game order (its section 3; the spawning stage of its 3.3).
- Spec M, `docs/behavior/script-api-game-object-basics.md` (cited as **M**): engine objects and their handles
  (its 2.1), kinds, identity (its 3), `CGameObject` and the client classes, `CPhysicObject` (its 10).
- `docs/formats/ltx-config.md` (section inheritance, its 5), `docs/formats/sav.md` (client data, its 7.2 and 8),
  `docs/behavior/level-objects.md`, `docs/behavior/trader-door.md` (the worked example of section 8).

Conventions:

- Binaries, identified as in the script environment specification: `bin\xrGame.dll` (6,246,400 bytes, MD5
  `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the game library"; `bin\XR_3DA.exe` (1,636,424 bytes, MD5
  `ba3ad0127bff6619ed991a7c82d8b946`), "the engine executable" [OB-10]. Game files are the winning copies of the
  root archives `gamedata.db0` … `gamedata.dbd`; scripts are the 439 scanned scripts of the scoping spec,
  **loadable** meaning one of its 250.
- An **object** is an engine object of the level (M 2.1); its **handle** is M's. A **binder** is a script object
  of a script class deriving from `object_binder` (A 1.3). An object's **attached binder** is the binder
  `bind_object` gave it (section 4). **Spawning** is the engine making an object live in the level from its spawn
  data (a spawn record or simulation object); **destroying** is the reverse (M 2.1: the object goes offline or
  is removed). A **client class** is a class of objects of the level (M 10.3).
- Each fact is **measured** (from the game files), **static** (from static analysis of the binaries, not
  observed at run time), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.
- Checks: `checks/object-binders/run_all.py` in the dirty workspace (`python3 -I run_all.py`; it reads the game
  files and the cached output of read-only headless static analysis produced beside it). Each claim carries the
  tag of the check lines that reproduce it, [OB-*n*]; the run prints `ALL PASS 110`.

## 1. What this spec owns

| Name | Kind | Listing (`scripts\lua_help.script`) |
|---|---|---|
| `object_binder` | engine class, no base | constructor `object_binder(game_object*)`, property `object`, 11 functions: `save`, `update`, `reload`, `net_export`, `net_save_relevant`, `load`, `net_destroy`, `reinit`, `net_Relcase`, `net_spawn`, `net_import` |
| `bind_object(binder)` | method of game objects | `function bind_object(object_binder*)` |
| `binded_object()` | method of game objects | `function binded_object()` |
| `script_binding` | key of an object's configuration section | — |

These are the scoping spec's 15 listed items of L (the class, its property, its 11 functions, the two game-object
methods) [OB-1]. The game library registers `object_binder` with exactly these members: one constructor, the
read-only property `object` and the 11 methods, each with a default implementation (2.2) [OB-11]. The scoping spec
assigns L no further candidate registration [OB-1].

## 2. The engine class `object_binder` (static)

### 2.1 Constructing, `object`

`object_binder(object)` keeps its argument, a game-object handle, as the binder's `object` [OB-11]. The property
can only be read (A 4.2: assigning `self.object` in a script binder would store a field of the script object that
hides it; no shipped binder class assigns it [OB-6]). Script binder classes construct their engine part with
`super(obj)`, one argument (A 3.2) [OB-4]. What the constructor does with an argument that is not a handle is
**unestablished**.

### 2.2 Methods

| Method | Arguments | Result | What the engine's own implementation does |
|---|---|---|---|
| `reinit()` | — | — | nothing |
| `reload(section)` | the object's section name (text) | — | nothing |
| `net_spawn(data)` | the simulation object of the object's spawn data (listing type `cse_alife_object`; spec J's classes) | boolean | returns true |
| `net_destroy()` | — | — | nothing |
| `net_import(packet)`, `net_export(packet)` | a `net_packet` (spec I) | — | nothing |
| `update(delta)` | a number: the update interval (5.2) | — | nothing |
| `save(packet)` | the `net_packet` being written (spec I, sav.md 8) | — | nothing |
| `load(reader)` | the `reader` over the object's saved client data (spec I, sav.md 8) | — | nothing |
| `net_save_relevant()` | — | boolean | returns false |
| `net_Relcase(object)` | the handle of an object being destroyed (5.5) | — | nothing |

[OB-11, OB-12, OB-13]. A base call `object_binder.m(self, …)` (A 6.1) runs the engine's own implementation of
the table, so it does nothing, or returns true for `net_spawn` and false for `net_save_relevant` [OB-11].

### 2.3 How the engine reaches a binder's override (answers A's gap 1 for this class)

Each of the 11 methods is called by the engine through the lookup of A 5.2, with the method named by its own
literal name [OB-12]: so every override the scoping spec (4.3) counts for `object_binder` (`reinit`, `reload`,
`net_spawn`, `net_destroy`, `net_import`, `net_export`, `net_save_relevant`, `net_Relcase`, `update`, `save`,
`load`) is reached by that lookup whenever the engine makes the call, and a method the binder class does not
override runs the engine's own implementation (A 5.2 step 3). Section 5 says when the engine makes each call;
`net_import` and `net_export` it never makes (5.7).

The two boolean results are read as Lua's truth of the one result kept: nil and false are false, any other value
true [OB-12]. An error in an override, or a found value that cannot be called, ends the game as A 5.3 says (the
error callback, then termination) [OB-12].

## 3. Binding: the key `script_binding`

### 3.1 When and how (static)

At every spawning of an object, as one step of it (5.1), the engine looks at the key `script_binding` of the
object's section (its section name, M 3, `section()`; keys inherited from parent sections count, ltx-config.md
5) [OB-14, OB-16]:

1. If the section has no such key, nothing happens.
2. Otherwise the value is resolved as a function (D 1.1, C 3.2) [OB-14].
3. Not found: the message `function %s is not loaded!` with the value goes to the message routine of D 3.3
   (which does nothing), and spawning continues without a binder [OB-14].
4. Found: it is called with one argument, the object's handle (made on first need and kept, M 2.1; nil if the
   object has none, **believed** never), in a protected call; results are discarded [OB-14]. An error there ends
   the game (D 1.2) [OB-14].

Then the attached binder, if any, gets `reload(section)` (5.1). Nothing else happens with the key; binding happens
only during spawning [OB-15]. On a dedicated server the step is skipped [OB-16]. What a binding function attaches
(or whether it attaches anything) is the script's decision: the function attaches a binder by calling
`bind_object` on the handle (section 4).

### 3.2 In the shipped configuration (measured)

- `config\system.ltx` (with everything it includes) yields 1,245 sections; 177 of them have `script_binding`,
  counting inherited keys: `bind_stalker.init` 95, `bind_monster.bind` 68, `bind_physic_object.init` 6, and one
  each of `bind_heli.bind`, `bind_respawn.bind`, `bind_restrictor.bind`, `bind_smart_terrain.bind`,
  `bind_stalker.actor_init`, `phantom_manager.bind`, `xr_zones.bind`, and one key with an empty value [OB-2].
- The 10 functions named are all defined by the script of their namespace [OB-2], in 24 configuration files
  [OB-2]. Among them `physic_object` has `bind_physic_object.init` [OB-2].
- The empty value is in section `online_offline_group` (`config\alife.ltx`) [OB-2]. What resolving an empty name
  gives, and whether an object of that section ever spawns in the level, is **unestablished** (**believed**:
  not found, so no binder).
- Six keys name `stalker.init`, for which no script exists; they are in `config\creatures\m_army.ltx` and
  `m_bandit.ltx`, which, like `m_fraction.ltx`, `system.ltx` does not include, so no loaded section has that value
  [OB-2].

### 3.3 The shipped binding functions (measured)

Each takes one parameter, the handle [OB-3]. Nine call `bind_object` themselves, once; `bind_stalker.init` calls
`xr_motivator.AddToMotivator(obj)`, which calls it only when `alife()` is true [OB-3]. These 10 calls are every
`bind_object` call of the loadable scripts (an eleventh is in `bug_test.script`, not loadable) [OB-6]. No shipped
script calls `binded_object` [OB-6].

## 4. `bind_object` and `binded_object` (static)

`handle:bind_object(binder)`:

- Converting the argument takes the binder away from the runtime's ownership: the engine owns it from then on,
  and it is deleted by the engine (5.6), not by Lua's collector [OB-22].
- When the game type is single-player or none (the test of `IsGameTypeSingle()`, B 4.5), the binder becomes the
  object's attached binder [OB-22]. In any other game type nothing is attached (**believed**: the binder, no
  longer owned by the runtime, is then never deleted).
- An earlier attached binder is replaced without being called or deleted (**believed**: it stays allocated)
  [OB-22].
- Nothing else happens: no binder method is called. A binder attached outside the binding step (3.1) gets the
  calls of section 5 from then on but none of the spawning calls already made (**believed**, from 5.1); no
  shipped script does this (3.3).

`handle:binded_object()` returns the attached binder [OB-22]; with none, nil (**believed**: the binding
library's conversion of an absent object). That the value returned is the same script object that was attached
is **believed** (A section 9 leaves it open for objects the engine passes to scripts).

## 5. When the engine calls the binder

All of this section is static. Every call goes to the attached binder only; an object without one gets none of
them, and the engine behaves as with the default results of 2.2 [OB-13].

### 5.1 Spawning

Creating an object from its section (the class id read from the section's `class`, then the object's own reading
of its section) happens before spawning and involves no binder [OB-16]. Spawning an object then runs, in this
order [OB-16]:

1. Its identity and place are set (M 3, 4) and it is registered among the level's objects.
2. The object's own engine reload of its section.
3. The binding (3.1), then the attached binder's **`reload(section)`**.
4. The object's own engine reinitialisation, then the binder's **`reinit()`**.
5. Only when the spawn data carries a non-empty client-data block (sav.md 8: a saved game; never at a New Game,
   whose spawn data has none): the object reads its own saved state from the block, then the binder's
   **`load(reader)`** reads on from there [OB-16].
   **Disputed in review:** "a saved game" is unbacked as the only case. The condition is a non-empty block;
   sav.md 8 keeps the block an object had when a save collected it while online, so an object that went offline
   after a save and comes online again in the same session is believed to carry one too (and then to get `load`).
   Only "never at a New Game" holds as stated.
6. The engine's remaining spawning work.
7. Last, the binder's **`net_spawn(data)`**, `data` being the simulation object of the spawn data. When the spawn
   data has no simulation object, the call is not made (the result counts as true) [OB-13].

Steps 3 and 4 are skipped on a dedicated server [OB-16]. These are the only calls of `reload`, `reinit` and
`net_spawn`, and the binding is the only one of `script_binding` [OB-15]. So for every object: `reload`, `reinit`,
(`load`), `net_spawn`, each exactly once per spawning. A later spawning of the same spawn data is a new object
whose binding runs again (**believed**: destroying ends the binder, 5.6).

The result of `net_spawn` is the result of this general spawning [OB-16]. The level's spawning of an object, when
that result reaches it as false, runs the object's destroying (so the binder gets `net_destroy`, 5.6), deletes the
object and logs `! Failed to spawn entity '%s'` with the section name [OB-24]. A client class's own spawning
runs the general one inside it and may pass its result on or not: `CGameObject` passes it [OB-16]; `CPhysicObject`
ignores it and always succeeds (section 6) [OB-25]. For the other client classes (44 distinct spawning routines
among 65 classes examined [OB-24]) whether a false `net_spawn` fails the spawning is **unestablished**.
**Disputed in review:** "A client class's own spawning runs the general one inside it" is backed only for
`CGameObject` and `CPhysicObject`; for the other spawning routines it is unestablished, and so is "for every
object" in the paragraph above, which depends on it. Every
shipped `net_spawn` override calls `object_binder.net_spawn`, which returns true [OB-4, OB-11]; when an override
returns false is its script's.

### 5.2 Update

Each time the scheduler runs an object's scheduled update, the object's engine update runs first, then the
binder's **`update(delta)`** [OB-19]. `delta` is the time since the object's previous scheduled update as the
scheduler measures it (**believed** milliseconds), at least 1 and at most the larger of 1000 and the object's
maximum scheduling interval [OB-19]. How often an object is scheduled belongs to the scheduler, not here
(**unestablished** in the approved specs). Skipped on a dedicated server [OB-19].

### 5.3 Saving

When the game collects the client data of the level's objects (sav.md 8), it takes each object not marked for
destruction and asks whether it is **save-relevant**; only then is its block written (framed with the object's id
and size; the layout is sav.md's): the object's own state, then the binder's **`save(packet)`** [OB-17]. Save
relevance depends on the client class
[OB-17]:

- 28 of the 65 client classes examined are always relevant and never ask the binder: among them `CAI_Stalker`,
  `CAI_Trader`, every listed monster class, the player, `CCar` and `CPhysicObject`;
- 35 answer with the binder's **`net_save_relevant()`** (false without a binder, so such an object without a
  binder is not saved): among them `CGameObject`, `CSpaceRestrictor`, `CMincer`, `CMosquitoBald`, `CHairsZone`;
- 2 combine it with their own state: `CHelicopter`, and one further class (**believed** the hanging lamp's), which
  is relevant when the binder says so or when it has a further part (**believed** its physics shell).

Classes outside the 65 (item classes among them) are **unestablished**. sav.md 8 says a fresh block is collected
from every online object; the relevance test above narrows that (static).

### 5.4 Loading

The binder's **`load(reader)`** is called only from spawning (5.1 step 5) [OB-15]; whether the engine reaches an
object's load of saved state in any other way is **unestablished**.

### 5.5 Release of destroyed objects: `net_Relcase`

After the frame's update of the level's objects, if objects are queued for destruction: every object of both of the
level's object lists (**believed** the awake and the sleeping objects), for each queued object, taken from the last
queued to the first, gets
the engine's release notice for it; then each queued object is destroyed (5.6) [OB-20]. The notice reaches the
binder as **`net_Relcase(handle)`**, the handle of the object about to be destroyed [OB-13, OB-20]. A queued
object is still listed at that point, so its own binder gets the notice for its own object as well (**believed**,
from the order). Skipped on a dedicated server [OB-20].

### 5.6 Destroying

Destroying an object runs its engine destroying, removes it from the level's objects, then calls the binder's
**`net_destroy()`**, then deletes the binder and detaches it; the object's handle is freed after that (M 2.1)
[OB-13, OB-18].

### 5.7 Never called: `net_import`, `net_export`

No engine code calls the binder's `net_import` or `net_export` (static: neither the routines that make the binder
calls nor any other code reading the attached binder calls them) [OB-21]; so they are **believed** never called.
Only `bug_test.script` (not loadable) overrides them [OB-4].

### 5.8 Failures

An error in a binder override ends the game (2.3). If instead the engine meets an exception of its own while
calling the binder, the binder is deleted and detached and the object continues without one [OB-23]
(**believed** never reached by a script error, which ends the game first).

### 5.9 Order over an object's life

| Phase | Binder calls, in order |
|---|---|
| spawning | `reload(section)`, `reinit()`, `load(reader)` (saved game only), `net_spawn(data)` |
| while live, in the order the events occur | `update(delta)` at each scheduled update; `save(packet)` at each collection if save-relevant (`net_save_relevant()` asked first, 5.3); `net_Relcase(handle)` for each object destroyed through the queue of 5.5 |
| destroying | when destroyed through the queue of 5.5: `net_Relcase` for itself (**believed**, 5.5); then `net_destroy()` |

**Disputed in review:** "(saved game only)" for `load(reader)`: the call is made whenever the spawn data carries a
non-empty client-data block (see the mark at 5.1 step 5), not only when a saved game is loaded.

The order between objects (which object spawns or updates first) belongs to the specs of those steps (D 3 for the
New Game; level-objects.md for which objects spawn).

## 6. Physics objects (`CPhysicObject`, static)

The client class of class id `O_PHYS_S` is `CPhysicObject` (C 3.3, M 10.3). Its spawning first runs the general
spawning of 5.1, so all binder calls of 5.1 happen inside it, before the physics object's own steps: its collision
form, its physics part from the spawn data, being made visible and enabled [OB-25]. It returns success whatever
the binder's `net_spawn` returned [OB-25]. At the end of spawning a physics object leaves the scheduler only when it
has no attached binder and two further conditions of its own hold (**unestablished** which), so a physics object
with a binder keeps getting scheduled updates [OB-25].

Its scheduled update runs the general update (so the binder's `update`) before its physics part's own update
[OB-25]. It is always save-relevant (5.3) [OB-25]; its saved state is the physics flag of sav.md 8, then the
binder's part. Its destroying runs the general destroying (so the binder's `net_destroy`) before its physics
shell is deleted [OB-25]. It reads its saved state with the general load (5.1 step 5) [OB-25].

## 7. The shipped binder classes (measured)

11 script classes derive from `object_binder`, 10 in loadable scripts; each defines `__init` and calls `super(obj)`
[OB-4]. Overrides ("+": calls `object_binder.m(self, …)` of the same name; "o": does not):

| Class | Script | Loadable | Overrides |
|---|---|---|---|
| `heli_binder` | `bind_heli.script` | yes | reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+ |
| `generic_object_binder` | `bind_monster.script` | yes | reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+ |
| `generic_physics_binder` | `bind_physic_object.script` | yes | reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+ |
| `respawn_binder` | `bind_respawn.script` | yes | net_spawn+ net_destroy+ update+ |
| `restrictor_binder` | `bind_restrictor.script` | yes | reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o save+ load+ |
| `smart_terrain_binder` | `bind_smart_terrain.script` | yes | net_spawn+ net_destroy+ net_Relcase o update+ |
| `actor_binder` | `bind_stalker.script` | yes | reinit+ net_spawn+ net_destroy+ update+ save+ load+ |
| `stalker_binder` | `bug_test.script` | no | reinit+ reload+ net_spawn+ net_destroy+ net_import+ net_export+ |
| `Phantom` | `phantom_manager.script` | yes | net_destroy o |
| `motivator_binder` | `xr_motivator.script` | yes | reinit+ reload+ net_spawn+ net_destroy+ net_save_relevant o update+ save+ load+ |
| `arena_zone_binder` | `xr_zones.script` | yes | net_spawn+ net_destroy+ save+ load+ |

[OB-4]. Every base call `object_binder.m(self, …)` is inside the override of the same `m` [OB-4]. Every binder
script except `phantom_manager.script` reads `self.object` [OB-6]. The binder classes also define 25 further method
names [OB-4]: `__finalize` (A 7.1) and handlers such as `hit_callback` and `death_callback`, which the engine does
not dispatch as binder methods (2.3); handing them to the engine as callbacks is spec O's.

## 8. Worked example: the trader's door

The door is a spawn record of section `physic_object` (trader-door.md 1), class id `O_PHYS_S`, client class
`CPhysicObject` (section 6); its section has `script_binding = bind_physic_object.init` [OB-2]. At a New Game the
door spawns in the spawning stage of D 3.3. Within its spawning (5.1, section 6):

1. The binding calls `bind_physic_object.init(handle)`. The function reads the custom data (`spawn_ini()`, M 3):
   a section `[animation]` would make it call the script's `abort`; it returns without binding only when the
   custom data exists, has neither `[logic]` nor `[drop_box]`, and the class id is not `projector`. The door's
   custom data has `[logic]` (trader-door.md 1), so it sets the script table `db.storage[id]` and attaches a new
   `generic_physics_binder`, whose `__init` calls `super(obj)` and sets `initialized` and `loaded` to false
   [OB-7].
2. `reload("physic_object")` (the base call, nothing more), then `reinit()` (the base call, then a fresh
   `db.storage` table for the object, kept as `self.st`) [OB-4, OB-7].
3. No `load`: the New Game's spawn data has no client data (sav.md 8). In a saved game `load(reader)` sets
   `loaded`, then reads the logic's saved state with `xr_logic.load_obj` [OB-7].
4. The physics object's engine spawning, then `net_spawn(data)`: the base call returns true, so the override goes on
   (the door is no `projector` and has no `[drop_box]`, trader-door.md 1) and returns true [OB-7]; `CPhysicObject`
   succeeds in any case (section 6). Then the door's collision form and
   physics shell are made (section 6, trader-door.md 1).

Having a binder, the door stays scheduled (section 6). At each scheduled update, after the engine's update, its
binder's `update(delta)` runs; the first one in which the player's script object exists (`db.actor`) starts the
logic with `xr_logic.initialize_obj` (the `ph_door` scheme of trader-door.md 2.1), and every update with an active
section (or with `[drop_box]`) passes `"update"` to the scheme and sets the hit, death and use callbacks (spec O)
[OB-7]. When the game is
saved the door is always save-relevant; its block holds the physics flag, then `xr_logic.save_obj`'s data written
by the binder's `save` [OB-7, OB-17]. `generic_physics_binder:net_save_relevant()` returns true but is never asked
for a physics object (5.3) [OB-7]. When the door is destroyed, its binder's `net_destroy` runs while its physics
shell still exists (section 6).

How many objects of `l01_escape` are bound at the start is the scoping spec's (its section 5, step 6: 206 objects
with `bind_physic_object.init` among 367 with a binding function).

## 9. Use by the shipped scripts (measured)

`bind_object` is called in 11 scripts (10 loadable); the 10 loadable calls are inside the binding functions or the
function one of them calls (3.3) [OB-6]; `binded_object` in none [OB-6]. The overridden methods, per name (all
scripts / loadable):
`__init` 11/10, `reinit` 7/6, `reload` 6/5, `net_spawn` 10/9, `net_destroy` 11/10, `net_import` 1/0,
`net_export` 1/0, `net_save_relevant` 5/5, `net_Relcase` 1/1, `update` 7/7, `save` 7/7, `load` 7/7 [OB-4].

## 10. Verification facts

All reproduced by `run_all.py` (110 PASS lines, no failure):

- Listing: `object_binder` with no base, the property `object`, one constructor, 11 functions; 15 listed items
  [OB-1].
- Configuration: 177 of 1,245 sections with `script_binding`; 10 defined binding functions in 24 files; one empty
  value; the unloaded `stalker.init` keys [OB-2]. Binding functions and `bind_object` calls [OB-3, OB-6]; binder
  classes and overrides [OB-4]; the physics-object binder [OB-7].
- Static: the registration of `object_binder` and its defaults [OB-11]; dispatch by literal name and boolean
  results [OB-12]; the binder calls and their callers [OB-13, OB-15]; the binding [OB-14]; spawning order [OB-16];
  saving and relevance by class [OB-17]; destroying [OB-18]; update and its interval [OB-19]; release notices
  [OB-20]; no import or export call [OB-21]; `bind_object`, `binded_object` [OB-22]; exceptions [OB-23]; failed
  spawning [OB-24]; physics objects [OB-25].

## 11. Unestablished

- What the constructor does with an argument that is not a handle (2.1).
- Resolving an empty `script_binding`, and whether an object of `online_offline_group` ever spawns (3.2).
- For client classes other than `CGameObject` and `CPhysicObject`, whether a false `net_spawn` fails spawning
  (5.1).
- How often objects are scheduled, and the time unit of `delta` (believed milliseconds) (5.2).
- Save relevance of the client classes outside the 65 examined, and the exact rule of the two combining classes
  (5.3).
- Other ways the engine reaches an object's load of saved state (5.4).
- That an object gets the release notice for itself (believed, 5.5); that `net_import`, `net_export` are never
  called (believed, 5.7).
- The two further conditions under which a physics object without a binder leaves the scheduler (section 6).
- `bind_object` in a game type other than single-player or none, and a second `bind_object`: believed leaks
  (section 4); `binded_object` without a binder (believed nil) and the identity of the returned object.

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` and `XR_3DA.exe` as
  identified above; the archive file tree of `gamedata.db0` … `gamedata.dbd` extracted with the project's archive
  tool; `config\system.ltx` loaded with its includes and section inheritance as ltx-config.md defines.
- Analysis: static analysis with Ghidra of `xrGame.dll` (the registration of `object_binder`, `bind_object` and
  `binded_object`; the engine side of the binder and its callers; the client object's spawning, update, saving,
  loading, release and destroying; the physics object's; the level's spawning and save collection) and of
  `XR_3DA.exe` (object creation, the object list's update and destruction queue, the scheduler's step); run-time
  type information read by a throwaway parser; a throwaway Lua tokenizer and scanner over the 439 scripts. No
  binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

1. For the client classes other than `CGameObject` and `CPhysicObject`, whether their own spawning, scheduled
   update, saving, release notice and destroying each reach the general ones of section 5 (and so the binder
   calls), and where their own steps fall relative to the binder calls, is not established.
2. Ways an object stops being live other than the destruction queue of 5.5 (the level ending, quitting to the
   menu, loading a saved game over a running one) are not stated, nor whether its binder gets `net_Relcase` and
   `net_destroy` then.
3. What the engine's deletion of a binder (5.6, 5.8) does to its script object is not stated: whether and when
   `__finalize` (A 7.1) runs, and whether the script object stays reachable from scripts that kept it.
4. Whether going offline writes the object's client-data block (and so calls the binder's `save`), which decides
   when `load` runs on coming online again (5.1 step 5), is not stated.
