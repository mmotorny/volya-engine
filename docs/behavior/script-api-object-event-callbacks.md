# Script API O: object event callbacks

How the game scripts of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) hand functions to the engine to be
called on events of objects in the level: the class `callback` (the event ids), the `game_object` members
`set_callback`, `set_fastcall`, `set_enemy_callback` and `set_patrol_extrapolate_callback`, and the `level`
functions `add_call`, `remove_call` and `remove_calls_for_object`; with the name `set_use_callback`, which the
scripts use and the game library lacks. This is spec O of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6, order 21); its scope rule and review gaps apply: this spec owns
these names as the binary registers them, listed or not, and the entry points its section 4.5 assigns to O
(`set_callback`, `set_fastcall`, `set_enemy_callback`, `set_patrol_extrapolate_callback`, `level.add_call` and
`level.remove_call` in all their forms). For each event id it states which objects the engine calls it for, when,
and with which arguments.

Builds on (not repeated here):

- `docs/behavior/script-environment.md` (the runtime, one number type), `docs/behavior/script-loading.md` (the 250
  loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (script classes, how arguments
  are matched; what a call of a member the engine lacks does). Statements that need it say **depends on A**.
- Spec B, `docs/behavior/script-api-engine-services.md`; spec C,
  `docs/behavior/script-api-class-ids-and-the-object-factory.md` (class ids such as `O_ACTOR`, `O_HLAMP`,
  `O_INVBOX`, `P_DSTRBL`, `SCRIPTZN`); spec E, `docs/behavior/script-api-value-types-and-math.md` (`vector`);
  spec F, `docs/behavior/script-api-configuration-and-files.md`.
- Spec D, `docs/behavior/script-api-engine-entry-points.md`: its section 1.2 (a script function the engine calls
  runs in a protected call; a failure goes to the binding library's error callback, the fatal `LUA error: %s`,
  and the program ends if that returns). Every engine call of this spec is such a call; this is written
  **fatal as D 1.2**.
- Spec M, `docs/behavior/script-api-game-object-basics.md`: the **handle**, the **engine object**, the **kinds** of
  its section 2.2 (entity, living, inventory owner, fighter, stalker, player, item, physical, anomalous zone,
  restrictor), the routine that swallows diagnostics (its 2.3), `CGameObject` and its `use` (10.1), the player's
  use action (6.3).
- `docs/behavior/use-action.md` (the use action, 3, its use callback and the door scheme's, 4, the usable flag,
  10) and `docs/behavior/physics-shells.md` (the trader's door, 10: the worked example of section 10).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the engine
  executable" `bin\XR_3DA.exe` (1,636,424 bytes, MD5 `ba3ad0127bff6619ed991a7c82d8b946`) [OC-1].
- **The listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only.
- A **function** is a Lua function value; **true** for a result means Lua truth (anything but `nil` and `false`);
  a **number** is the runtime's double, a **text** a Lua string. A call is written as scripts write it.
- **The player's object** is the engine object the player controls; the game library keeps one reference to it,
  set when that object spawns from the player's simulation object [OC-9].
- Engine classes are named only as the game library registers them for scripts (`CAI_Stalker`, `CAI_Trader`,
  `CHelicopter`, `CPhysicObject`, `CGameObject`, `CGameTask`, `SGameTaskObjective`, the 17 monster classes of
  M 2.2, that is its 19 creature classes other than `CAI_Stalker` and `CAI_Trader`); other engine classes are
  described ("hanging lamps"), with their class ids of spec C where the mapping is given.
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (read from
  game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [OC-*n*]. The checks are one script in the dirty workspace,
`checks/object-event-callbacks/run_all.py` (`python3 -I run_all.py`): it reads the binaries (bytes, run-time type
information), the cached evidence of read-only headless static analysis of them (produced by `evidence.py`
beside it), the listing, the scoping spec's own scan and the shipped scripts. All checks pass (`ALL PASS 18`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 42 listed items [OC-2]:

| Owner | Items |
|---|---|
| `callback` | the class and its 34 constants (section 2) |
| `game_object` | `set_callback` (section 3), `set_fastcall` (section 5), `set_enemy_callback` (section 7), `set_patrol_extrapolate_callback` (section 8) |
| `level` | `add_call`, `remove_call`, `remove_calls_for_object` (section 6) |

and the name `set_use_callback`, which the scripts use and which occurs nowhere in the game library (section 9)
[OC-2]. Of these, 30 names are used by the shipped scripts, by 20 loadable scripts (30 scripts in all); no script
uses `level.add_call`, `level.remove_call` or `level.remove_calls_for_object` [OC-2].

### 1.2 As registered (static)

- `callback` is registered with no base class and **no constructor**; it only holds the 34 integer constants of
  section 2, read as `callback.name` [OC-3].
- On `game_object`: `set_callback` in three forms, `set_fastcall` in one, `set_enemy_callback` and
  `set_patrol_extrapolate_callback` in three each, exactly the forms the listing gives [OC-4, OC-10, OC-17].
- In `level`: `add_call` and `remove_call` in three forms each and `remove_calls_for_object` in one, as listed
  [OC-11].

None of the `game_object` members tests a kind of object except the two of sections 7 and 8 [OC-4, OC-10,
OC-17].

## 2. The event ids (static)

The 34 constants of `callback`, the same values as the listing [OC-3]:

| Value | Name | Value | Name | Value | Name |
|---|---|---|---|---|---|
| 0 | `trade_start` | 12 | `article_info` | 24 | `actor_sleep` |
| 1 | `trade_stop` | 13 | `task_state` | 25 | `helicopter_on_point` |
| 2 | `trade_sell_buy_item` | 14 | `map_location_added` | 26 | `helicopter_on_hit` |
| 3 | `trade_perform_operation` | 15 | `use_object` | 27 | `on_item_take` |
| 4 | `zone_enter` | 16 | `hit` | 28 | `on_item_drop` |
| 5 | `zone_exit` | 17 | `sound` | 29 | `script_animation` |
| 6 | `level_border_exit` | 18 | `action_movement` | 30 | `trader_global_anim_request` |
| 7 | `level_border_enter` | 19 | `action_watch` | 31 | `trader_head_anim_request` |
| 8 | `death` | 20 | `action_animation` | 32 | `trader_sound_end` |
| 9 | `patrol_path_in_point` | 21 | `action_sound` | 33 | `take_item_from_box` |
| 10 | `inventory_pda` | 22 | `action_particle` | | |
| 11 | `inventory_info` | 23 | `action_object` | | |

The id is a plain number: `set_callback` accepts any number as an id (section 3.1), and these names only give
the values the engine calls (section 4).

## 3. `set_callback` (static)

### 3.1 Each object's table

Every engine object keeps a **callback table**: at most one entry per id, each entry holding a function (or
none) and a **bound value** (or none) [OC-4]. An entry is made the first time an id is set or looked up; asking
for an id that has no entry makes an empty one [OC-4]. No member of this spec removes an entry. `set_callback` works on a handle of
any object, with no kind test [OC-4]:

| Form | Effect on the entry of `id` |
|---|---|
| `obj:set_callback(id, f)` | clears it (function and bound value), then stores `f` |
| `obj:set_callback(id, f, value)` | clears it, then stores `f` and `value` as the bound value |
| `obj:set_callback(id)` | clears it |

So a later call replaces an earlier one; setting the same id again never adds a second function [OC-4]. The
argument `f` may be a function or `nil`; `nil` stores an empty function, the same as clearing [OC-5]. The scripts
clear an id with `obj:set_callback(id, nil)` (measured, for example `bind_stalker.script`). Other argument types:
**depends on A** (the forms are matched as A describes). `value` may be any value; the scripts pass the object
that owns `f` as a method (`self`) [OC-18].

### 3.2 How the engine calls an entry

When the engine raises an event with id *n* on an object, it looks up entry *n* of that object's table [OC-6,
OC-7]:

- no function stored: nothing happens;
- a bound value is stored: it calls `f(value, a1, …, ak)`, so a method `obj:m(...)` set with `self` receives the
  event arguments after `self`;
- otherwise: it calls `f(a1, …, ak)`.

The arguments `a1 … ak` are those of section 4 for the id. Results are not read [OC-6]. The call is protected;
an error in `f` is **fatal as D 1.2** [OC-6]. Handles passed as arguments are the objects' handles (M 2.1); an
argument the engine has no object for is passed as `nil` where section 4 says so [OC-8].

## 4. The events the engine raises (static)

Every engine call of a callback entry goes through one table lookup of the game library (directly, or through
one accessor used by the scripted-action routine); all of its callers are accounted for below [OC-7]. "Receiver"
is the object whose table is used; arguments are listed without the bound value. Argument kinds are static
[OC-8]; their meanings are **believed** where marked, from the shipped scripts' parameter names [OC-18].

| Id | Receiver | When | Arguments |
|---|---|---|---|
| 0 `trade_start` | each partner of a trade that is a trader (`CAI_Trader`) | when trading starts in the trade window (believed: the window's routine that marks the trade and the trader as trading) [OC-9] | none |
| 1 `trade_stop` | likewise | when trading stops (believed: the routine that clears those marks) [OC-9] | none |
| 2 `trade_sell_buy_item` | the player's object | in the trade window's transfer of an item, under conditions on the two partners that are **unestablished** [OC-9] | handle (believed the item), boolean (believed sold or bought), number (believed the money) |
| 3 `trade_perform_operation` | a trade partner that is a trader | when the trade window performs a deal [OC-9] | number, number (meanings **unestablished**) |
| 4 `zone_enter` | a script zone (believed class id `SCRIPTZN`) | when an object starts touching the zone [OC-9] | handle of the zone, handle of the object (believed this order) |
| 5 `zone_exit` | likewise | when an object stops touching the zone, unless that object is marked for destruction (M's "destroy" state); and when an object the zone tracks is removed from the level [OC-9] | likewise |
| 6 `level_border_exit` | the player's object | in the player's update, while the player is the object the level follows (believed: the level's current entity): when its movement's count of contacts with the level border (believed) becomes positive [OC-9] | the player's handle |
| 7 `level_border_enter` | likewise | likewise, when that count returns to zero [OC-9] | the player's handle |
| 8 `death` | every living object (M 2.2) | when it dies [OC-9] | its own handle, the killer's handle or `nil` [OC-8] |
| 8 `death` | breakable physics objects (believed class id `P_DSTRBL`) | when a hit destroys it [OC-9] | its own handle, the last hitter as a `CGameObject` value (M 10.1), not a handle [OC-8] |
| 9 `patrol_path_in_point` | a creature moving along a patrol path | when it reaches a point of the path (believed) [OC-9] | its own handle, the number 0, the point's index [OC-8] |
| 11 `inventory_info` | the player's object | when it receives an information portion [OC-9] | handle (believed the player's), text: the portion's id |
| 12 `article_info` | the player's object | when it receives an encyclopedia article (believed) [OC-9] | the player's handle, text (believed the group), text (believed the name), number (believed the article type) |
| 13 `task_state` | the player's object | when a task objective's state changes; at one of the three calling places only for the new states 0 and 2 [OC-9] | the task (`CGameTask`), the objective (`SGameTaskObjective`), the new state (number) |
| 14 `map_location_added` | the player's object | when a map location is added for an object [OC-9] | text (the location's type), number (the object's id) |
| 15 `use_object` | the object being used | when it is used: its scripted use, which the player's use action calls (M 6.3, 10.1) [OC-9] | its own handle, the user's handle |
| 15 `use_object` | the player's object | when an item of the player's inventory is used up on the player (believed: eating or applying it) [OC-9] | handle (believed the item) |
| 16 `hit` | the 17 monster classes, `CAI_Stalker`, breakable physics objects (believed `P_DSTRBL`), hanging lamps (believed `O_HLAMP`) | when hit, in each class's hit handling [OC-9] | handle (believed its own), number (the hit's power, believed), vector (believed the direction), handle of the hitter (or `nil`), number (believed the bone) [OC-8] |
| 17 `sound` | creatures and other objects able to run scripted actions | when the object perceives a sound that has a source and entry 17 has a function at that moment, the sound is queued; the queue is delivered and emptied in the object's update [OC-9] | its own handle, the sound's source (handle), number (believed the sound type), vector (believed the position), number (believed the power) [OC-8] |
| 18 `action_movement` | objects running scripted actions (spec V) | when the movement part of the current scripted action becomes done [OC-7, OC-9] | handle, number, number (meanings **unestablished**) |
| 19 `action_watch` | likewise | when the look part becomes done | handle, the number 1 [OC-7] |
| 20 `action_animation` | likewise | when the animation part becomes done | handle, the number 2 [OC-7] |
| 21 `action_sound` | likewise | when the sound part becomes done | handle, the number 3 [OC-7] |
| 22 `action_particle` | likewise | when the particle part becomes done | handle, the number 4 [OC-7] |
| 23 `action_object` | likewise | when the object part becomes done | handle, the number 5 [OC-7] |
| 25 `helicopter_on_point` | a helicopter (`CHelicopter`) | when it reaches a point of its path (two calling places) [OC-9] | number (believed the distance), vector (believed the position), number (believed the path point index) |
| 26 `helicopter_on_hit` | a helicopter | when hit while not destroyed, by another object that is the player's object (class id `O_ACTOR`), of the stalker kind, or passes one further test (**unestablished**) [OC-8] | four numbers (believed power, impulse, hit type, the hitter's id) |
| 27 `on_item_take` | every inventory owner (M 2.2) | when an item enters its inventory [OC-9] | handle of the item |
| 28 `on_item_drop` | every inventory owner | when an item leaves its inventory [OC-9] | handle of the item |
| 29 `script_animation` | `CAI_Stalker` | in its update and its animation handling, when a flag of its script animation is set (believed: the script animation finished) [OC-9] | none |
| 30 `trader_global_anim_request` | `CAI_Trader` | in the trader's update, when its body animation needs a new one (believed) [OC-9] | none |
| 31 `trader_head_anim_request` | `CAI_Trader` | likewise for its head animation (believed) [OC-9] | none |
| 32 `trader_sound_end` | `CAI_Trader` | likewise when its sound has ended (believed) [OC-9] | none |
| 33 `take_item_from_box` | the player's object | when an item leaves an inventory box (believed class id `O_INVBOX`) while the box is marked as in use (believed: opened by the player) [OC-9] | handle of the box, handle of the item |

The engine **never calls** ids 10 (`inventory_pda`) and 24 (`actor_sleep`): no caller of the lookup uses them
[OC-7]. Setting them stores a function that is never called.

The handles of section 3.2 are the receivers' and arguments' own handles (M 2.1); where a row says "its own
handle" it is static that the handle is made from the receiver [OC-9]. Which objects "creatures and other objects
able to run scripted actions" are exactly is **believed**: the routines belong to an engine part shared by the
creatures, the car and one further class [OC-9].

## 5. `set_fastcall(f, value)` (static)

`obj:set_fastcall(f, value)` (one form; `f` a function, `value` any value, usually `self`) [OC-10]:

1. removes from the level's call list (section 6) every **fast call** of an object with the same id as `obj`;
2. adds a new fast call for `obj` at the end of that list.

So an object has at most one fast call; setting it again replaces it [OC-10]. A fast call is an entry of the
level's call list (section 6.1) whose condition calls `f(value)` and remembers the result, and whose action does
nothing. On every update of the list `f(value)` is called; once it returns true the entry is removed in that same
update [OC-10, OC-13]. `f` is therefore called once per level frame (section 6.1) until it returns true. An error
in `f` is **fatal as D 1.2** [OC-13]. No kind of object is required [OC-10].

A fast call is also removed when its object is a physical object (M 2.2) that is destroyed (goes offline or is
removed) [OC-10]. For other objects it stays until `f` returns true (**believed**: `f` itself does not use the
object, so it keeps being called after the object is gone). `level.remove_call` and
`level.remove_calls_for_object` never remove a fast call [OC-14, OC-15].

## 6. The level's call list (static)

### 6.1 The list and its update

The level keeps one **call list** for scripts, an ordered list of entries, each a **condition** and an
**action** [OC-11, OC-12]. Every form of `level.add_call` and `level.remove_call`,
`level.remove_calls_for_object` and `set_fastcall` work on this list [OC-11]. New entries go to the end [OC-12].

The list is updated once in every frame of the level, by the level's per-frame routine, after the physics steps
and after the engine executable's own level frame (believed: the objects' updates) [OC-12]. (The level has a
second list of the same kind, updated just before it, which scripts cannot reach [OC-12].) One update walks the
entries in order [OC-12]:

1. the condition is evaluated; if it is true, the action runs;
2. if the action has run (an action runs at most once), or the condition reports that it is finished (only a
   fast call's does, section 5), the entry is removed and the walk continues with the entry that took its place;
   otherwise the walk moves on.

The walk re-reads the list's length at every step, so an entry added during the update (by a condition or an
action) is reached in the same update (static) [OC-12]. So an entry added with `add_call` calls its condition
once per frame until the condition returns true; then the action is called once, and the entry is gone [OC-13].
All these calls are protected; an error is **fatal as D 1.2** [OC-13].

### 6.2 `level.add_call`

| Form | Condition evaluates | Action calls |
|---|---|---|
| `level.add_call(c, a)` | `c()`, true when its result is true | `a()` |
| `level.add_call(value, c, a)` | `c(value)` | `a(value)` |
| `level.add_call(value, "cname", "aname")` | `value.cname(value)`, as `value:cname()` | `value:aname()` |

[OC-11, OC-13]. In the third form the two members are looked up when `add_call` is called (by Lua indexing of
`value`, so class members count), and the entry stores the functions found; it is then the same as
`level.add_call(value, value.cname, value.aname)` [OC-11]. Unlike the other forms, the third form adds nothing
when the list already holds an equal entry of the second kind (same value and same two functions, by the
comparison of 6.3) [OC-11]. What it does when a member is missing or is not a function is **unestablished**
[OC-11].

### 6.3 `level.remove_call`

Each form removes **every** entry it matches, keeping the order of the rest [OC-14]:

- `level.remove_call(value, c, a)` matches entries made by `add_call(value', c', a')` or by the third form of
  6.2, when `value'` equals `value` by Lua equality (or both are `nil`) and `c'` equals `c` and `a'` equals `a` as
  Lua values [OC-14].
- `level.remove_call(c, a)` **matches nothing** and so has no effect: the conditions of entries made by
  `add_call(c, a)` are compared by the identity of a private copy that each call makes, and two copies are never
  the same [OC-14].
- `level.remove_call(value, "cname", "aname")` **matches nothing** either: it compares only with entries that
  keep the member names, and no member makes such entries (the third form of `add_call` keeps the functions,
  6.2) [OC-14].

So, of the shipped engine, only `remove_call(value, c, a)` and `remove_calls_for_object` remove entries made by
scripts.

### 6.4 `level.remove_calls_for_object(value)`

Removes every entry made by the second or third form of `add_call` whose stored value equals `value` (Lua
equality, or both `nil`) [OC-15]. Entries of the first form and fast calls are not removed [OC-15].

### 6.5 When the list is emptied

All entries are dropped, without being called, when the level removes all of its objects, and when the level is
destroyed [OC-16]. When the level removes all of its objects (a step of one level routine and of one routine of the
game's persistent part) is **unestablished** here [OC-16].

## 7. `set_enemy_callback` (static)

For objects of the **fighter** kind (M 2.2) only; on any other object the member does nothing (its diagnostic is
swallowed, M 2.3) [OC-17]:

| Form | Effect |
|---|---|
| `obj:set_enemy_callback()` | clears the object's enemy callback |
| `obj:set_enemy_callback(f)` | clears it, then stores `f` |
| `obj:set_enemy_callback(f, value)` | clears it, then stores `f` and the bound value `value` |

The engine consults it when the fighter's enemy handling decides whether an object it knows of is an enemy worth
keeping [OC-17]. The engine's own tests come first (the candidate is not the fighter itself; further tests,
**believed** to include that it is alive and hostile); a candidate they reject is not an enemy and the callback is not asked. For a
candidate they accept: with no callback stored it is an enemy; otherwise the callback is called with two handles
(believed: the fighter's, then the candidate's; `xr_combat_ignore.script` names them `obj, enemy`), after
`value` when one is bound, and the candidate is an enemy exactly when the result is true [OC-17, OC-18]. An error
is **fatal as D 1.2** [OC-17]. How often the enemy handling runs is **unestablished** here (spec X).

## 8. `set_patrol_extrapolate_callback` (static)

For fighters only, with the same three forms and the same handling of other objects as section 7 [OC-17]. The
engine consults it while the fighter moves along a patrol path: it calls the callback with one number, the index
of the path's current point (after `value` when one is bound) and keeps the result; with no callback stored the
result is true [OC-17]. The result is stored with the creature's patrol path, compared with the result stored
before [OC-17]. What it changes is **believed** from the member's name: whether the path is extrapolated beyond
the point (spec X). An error is **fatal as D 1.2** [OC-17].

## 9. `set_use_callback` (measured)

The name `set_use_callback` occurs nowhere in the game library [OC-2]; `game_object` has no such member. The
scripts call it on handles in 3 scripts, one of them loadable: `utils.script` (`door:set_use_callback(...)` in its
function `door_init`) [OC-2]. What reading a name `game_object` lacks gives is **unestablished** (as M 11:
**depends on A** for script objects, unestablished for handles). The engine's use event is id 15 of section 4,
set with `set_callback(callback.use_object, ...)`.

## 10. Worked example: the trader's door

The door is a `CPhysicObject` (M 12): of M's kinds it has only physical. Its binder and its door scheme use
[OC-18]:

- `bind_physic_object.script` (`generic_physics_binder:update`): while the object's logic has an active section,
  on every update, `set_callback(callback.hit, ...)`, `set_callback(callback.death, ...)` and
  `set_callback(callback.use_object, ...)`, each with the binder as the bound value. Each update replaces the
  three entries (3.1); nothing accumulates. **Disputed in review:** the condition is incomplete: the binder sets
  the three callbacks on every update while the object's logic has an active section **or** the object's custom
  data (`spawn_ini()`) has a section `drop_box` (measured, `bind_physic_object.script`).
- Of these three ids the engine calls only `use_object` for a `CPhysicObject`: its class has the scripted use
  that raises id 15 (M 10.1), and none of the classes whose hit or death handling raises ids 16 and 8 (section 4)
  [OC-18]. So the player's use action (M 6.3, `use-action.md` 3, 4) calls
  `generic_physics_binder.use_callback(binder, door, player)`: the door's handle and the user's [OC-9, OC-18]. The
  binder passes the use on to the active scheme (`use-action.md` 4.1). The hit and death callbacks the binder sets
  are never called for the door (static, from the call sites of section 4).
- `ph_door.script` (`action_door:close_door`): `self.object:set_fastcall(self.fastcall, self)` three times in a
  row. Each call replaces the door's fast call (section 5), so one remains: `action_door.fastcall(action)` is
  called once per level frame until it returns true, which it does once the door is blocked and closed (after
  its `close_action`) [OC-18]. As the door is physical, its fast call is also dropped when the door object is
  destroyed (section 5).

## 11. Use by the shipped scripts (measured)

By the scoping spec's scan [OC-2]: `set_callback` is used by 15 loadable scripts (24 in all), `set_fastcall` by 3
(3: `heli_combat.script`, `ph_car.script`, `ph_door.script`), `set_patrol_extrapolate_callback` by 2 (2),
`set_enemy_callback` by 1 (1); the scripts use 24 of the 34 constants, each in 1 to 4 loadable scripts (`hit` and
`use_object` 4 each). The ids the scripts set agree in arity with section 4 where the scripts' handlers name
their parameters [OC-18]. One script, `xr_motivator.script`, has its `set_callback(callback.sound, ...)` commented
out [OC-18].

## 12. Verification facts

All reproduced by `run_all.py` (18 PASS lines, no failure):

- Scope: 42 items (the class, 34 constants, 4 methods, 3 `level` functions), 30 names, 20 loadable scripts;
  `set_use_callback` absent [OC-2].
- Constants: 34, values 0 … 33, as listed; no constructor [OC-3].
- `set_callback`: 3 forms; a function or `nil` accepted [OC-4, OC-5]; 19 call shapes, each with one argument more
  when a value is bound [OC-6].
- Events: 32 ids called by the engine from the lookup's callers, every caller accounted for; ids 10 and 24 never
  [OC-7]; argument kinds and receivers [OC-8, OC-9].
- The level's call list: 7 `level` forms and `set_fastcall` on one list, updated once per level frame; matching
  rules of removal [OC-10 to OC-16]; the enemy and patrol callbacks [OC-17].

## 13. Unestablished

- The meanings of the numbers marked believed or unestablished in section 4 (trade numbers, movement callback
  numbers, sound type and power, helicopter numbers), the conditions of `trade_sell_buy_item`, and the further
  hitter test of `helicopter_on_hit`.
- Which task states the other two calling places of `task_state` pass.
- The exact set of objects that receive `sound` and the action ids (believed, section 4); the mapping of the
  engine classes of breakable physics objects, hanging lamps, inventory boxes and script zones to class ids
  (believed).
- What `add_call` with member names does for a missing member or a member that is not a function (6.2).
- When the level removes all of its objects (6.5); what happens to a callback table when its object is destroyed
  and the object comes online again (believed: a new engine object with an empty table).
- What the patrol flag of section 8 does; how often the enemy handling runs (section 7).
- What calling `set_use_callback` on a handle does (section 9).
- Everything that depends on A (argument matching of the forms).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries identified above; the
  archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs
  (`scripts\*.script`).
- Analysis: static analysis with Ghidra (headless, read-only) of the game library's script registration of
  `callback`, the four `game_object` members and the three `level` functions, of their implementations, of the
  per-object callback table and the routines that call its entries, the argument pushing of each call shape, the
  level's call list (its entries, update, comparison and removal) and the level's per-frame routine, the
  enemy and patrol movement routines that consult the two fighter callbacks; run-time type information and data
  values read from the file bytes by a throwaway reader; the scoping spec's script scan reused; the listing and
  the scripts read as text. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

1. What happens when a function the engine is calling sets, replaces or clears its own entry (or another entry of the same object's table) during that call is not stated (3.1, 3.2).
2. For id 17 (`sound`), whether a queued sound is still delivered when entry 17 was cleared or replaced between queueing and the object's update is not stated (section 4).
3. When one occurrence raises several ids on one object (for example a hit that destroys a breakable physics object, ids 16 and 8), the order of the calls is not stated (section 4).
