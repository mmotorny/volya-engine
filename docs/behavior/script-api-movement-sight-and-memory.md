# Script API X: movement, sight and memory

What the movement, sight and memory part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of
Chernobyl (Russian edition), as scripts see it: the members of `game_object` that steer a character's movement,
path, sight, body and mental state, items in hand and script animations, that read and change what it sees and
remembers, its enemies and dangers, its space restrictions and covers, and the monster members (home, jump,
panic thresholds, berserk, hit information, the bloodsucker's control and the zombie's fake death); the classes
`CSightParams`, `danger_object`, `MonsterHitInfo`, `MonsterSpace`, `cef_storage` and the memory record classes
`memory_object`, `game_memory_object`, `entity_memory_object`, `visible_memory_object`, `sound_memory_object`,
`hit_memory_object`, `memory_info` and `not_yet_visible_object`. This is spec X of the approved scoping
specification `docs/behavior/script-api-scope.md` (section 6, order 28); its scope rule and review gaps apply:
this spec owns every name of these classes as the game library registers them, listed or not. The row owns
`hide_weapon` and `restore_weapon` (section 10.4) and the monsters' panic thresholds `set_custom_panic_threshold`
and `set_default_panic_threshold` (section 11.3): the scoping spec's split gives both to this spec [X-2].

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type, the 250
  loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched to
  forms, what a mismatch or a missing member does); spec B, `docs/behavior/script-api-engine-services.md` (its 3.3:
  the device's `time_global()`, in milliseconds); spec E, `docs/behavior/script-api-value-types-and-math.md`
  (`vector`; a vector result is a new vector; its 2.2 argument conversions); spec F,
  `docs/behavior/script-api-configuration-and-files.md` (reading a section's key; a missing key read without a
  test is fatal there); spec M, `docs/behavior/script-api-game-object-basics.md` (the handle and its engine
  object, 2.1; the **kinds** of object, 2.2; a member on an object of the wrong kind, 2.3); spec T,
  `docs/behavior/script-api-sound.md` (its 6.1: the **monster** kind; its 7.2: `memory_sound_objects`, whose
  values are this spec's `sound_memory_object`).
- `docs/behavior/characters-scope.md` (cited as **CH**): what the engine does behind these members over time is
  CH's: paths and restrictions (CH-9), stalker movement and motions (CH-10, CH-11), monster movement (CH-12), vision
  (CH-13), memory and enemy choice (CH-15), evaluation functions (CH-16), combat and covers (CH-18), danger (CH-19),
  weapons (CH-20), monster behaviour and abilities (CH-22, CH-23). This spec owns the names, the forms, the values
  each call reads or sets at once, and the fallbacks.
- Spec N (the `level` namespace, patrol paths, the classes `object_params` and `cover_point`) and spec W (the
  global `ef_storage`, the action planner) are not approved; where a member here returns or takes their objects
  this spec says so and describes only what this spec's member does.

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [X-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from
  the game library's registration code.
- A call is written as scripts write it: `npc:set_sight(look.direction, dir, true)`. A **number** is the runtime's
  double, a **text** a Lua string, **nil** Lua's nil, a **handle** a `game_object` and its **object** the engine
  object (M 2.1). A number given where the engine keeps an integer (a state, a type, a vertex, an index, a count)
  is **made an integer** and one given where it keeps a float is stored as a 32-bit float (E 2.2's conversions,
  **believed** the same here).
- **Now** is the device's `time_global()` (B 3.3), in milliseconds. **FLT_MAX** is the largest 32-bit float,
  3.4028235 × 10^38.
- "Reads through no object" means the code uses a missing part without a test (**believed**: the program
  crashes), as in M 2.3. **Fatal** means the program reports the failure and ends.
- Each fact is **static** (from static analysis of the game library, not observed at run time), **measured**
  (from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary
  was run.

**Checks.** Every claim carries a tag [X-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-movement-sight-and-memory/run_all.py` (`python3 -I run_all.py`): it reads the game library's
bytes and run-time type information, the evidence of read-only headless static analysis (produced by `gh.py`
beside it), the listing, the scoping spec's scan and the shipped scripts and configuration. All checks pass
(`ALL PASS 24`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 160 listed items [X-2]:

| Owner | Items |
|---|---|
| `game_object` | 88 functions (sections 3 – 11) |
| `CSightParams` | the class; the properties `m_sight_type`, `m_object`, `m_vector`; 12 constants (5.3) |
| `danger_object` | the class; 6 functions `type`, `time`, `position`, `object`, `dependent_object`, `perceive_type`; 11 constants (8.2) |
| memory records | the classes `memory_object`, `game_memory_object`, `entity_memory_object`, `visible_memory_object`, `sound_memory_object`, `hit_memory_object`, `memory_info`, `not_yet_visible_object` and their 18 declared members (7.1) |
| `MonsterHitInfo` | the class; the properties `who`, `direction`, `time` (11.4) |
| `MonsterSpace` | the class; 5 constants (11.6) |
| `cef_storage` | the class; the function `evaluate` (section 12) |

Also: the candidate string `__eq` that the scoping spec assigns to this spec, which is the equality of
`danger_object` (8.2); and three names that the loadable scripts call on game objects and that occur nowhere in
the game library, `nearest_accessible_position`, `set_node_evaluator`, `set_path_evaluator` (scoping spec 2.2's
absent names; section 13) [X-2]. Of these names the loadable scripts use 85, in 59 loadable scripts by the
scoping spec's lower count and 121 by its upper count [X-2]. No entry point of the scoping spec's 4.5 belongs to
this spec [X-2].

### 1.2 As registered (static)

- Every one of the 88 `game_object` functions is registered in the registration of `game_object` (M 1.2), paired
  with its implementation; 14 of them in several forms: `accessible` (2), `extrapolate_length` (2: reader,
  writer), `ignore_monster_threshold` (2), `max_ignore_monster_distance` (2), `movement_enabled` (2),
  `play_cycle` (2), `see` (2), `set_desired_direction` (2), `set_desired_position` (2), `set_item` (4) and
  `set_sight` (8); the others in one [X-3]. The forms of each are those of the listing, in the listing's order,
  except `add_animation`: the listing has `(name)` and `(name, flag)`, the game library registers one form with
  three arguments `(name, flag, flag)` [X-3, X-4].
- The memory record classes are registered with no constructor and only readable properties: `memory_object`
  without base; `game_memory_object` and `entity_memory_object` with base `memory_object`;
  `visible_memory_object` and `sound_memory_object` with base `game_memory_object`; `memory_info` with base
  `visible_memory_object`; `hit_memory_object` with base `entity_memory_object`; `not_yet_visible_object`
  without base. `danger_object` has no base, no constructor, 11 integer constants, `__eq` and its 6 functions
  [X-5]. `CSightParams` has one constructor without arguments, 12 integer constants and 3 readable properties;
  `MonsterHitInfo` no constructor and 3 properties, each readable and writable; `MonsterSpace` is a holder of 5
  integer constants; `cef_storage` has no constructor and 8 forms of `evaluate` [X-5, X-6].
- No class of this spec has a property the listing lacks (the scoping spec's gap 2) [X-5, X-6].

## 2. Kinds

The members need these kinds of object (M 2.2, T 6.1, and two more answered by the same kind queries) [X-7]:

| Kind | Classes whose objects have it |
|---|---|
| **fighter** (M 2.2) | the 18 creature classes other than `CAI_Trader` |
| **stalker** (M 2.2) | `CAI_Stalker` |
| **monster** (T 6.1) | the 17 creature classes other than `CAI_Stalker` and `CAI_Trader` |
| **scripted** | the 19 creature classes and `CCar` (not the player, not `CHelicopter`) |
| **living**, **inventory owner** (M 2.2) | the 19 creature classes and the player |
| **player** (M 2.2) | the player's object |
| **item** (M 2.2) | the 36 item classes |
| **weapon** | the 19 weapon classes of M 2.2's item kind (`CWeaponAK74` … `CWeaponWalther`; not the grenades) |

Four members test for one class instead of a kind: `set_alien_control` (`CAI_Bloodsucker`), `fake_death_fall_down`
and `fake_death_stand_up` (`CZombie`), `get_current_direction` (the **searchlight** class: **believed** the client class of objects of the class id
`clsid.projector`, 84, spec C; the scripts cannot name it) [X-7].

On an object of the wrong kind a member does nothing and returns the fallback its section gives, its diagnostic
going to the routine that returns at once (M 2.3) [X-8]. The monster members `berserk`, `set_home`, `remove_home`,
`skip_transfer_enemy`, `set_custom_panic_threshold`, `set_default_panic_threshold` and `set_alien_control`,
`fake_death_*` pass no diagnostic at all [X-8]. Exceptions that read through no object on the wrong kind:
`memory_visible_objects`, `memory_hit_objects`, `not_yet_visible_objects` and `visibility_threshold` (on an object
that is not a fighter) [X-8].

## 3. Stalker movement (static)

A stalker keeps a **movement request** (the **target** body state, movement type, mental state, path type, detail
path type, desired position, desired direction, destination vertex) and its **current** body state, movement type,
mental state and path type, which its movement update moves toward the request (CH-10) [X-9]. All members of this
section need the stalker kind.

| Member | Does | Fallback |
|---|---|---|
| `set_body_state(s)` | target body state := `s` | nothing |
| `set_movement_type(t)` | target movement type := `t` | nothing |
| `set_mental_state(m)` | target mental state := `m` | nothing |
| `set_path_type(p)` | target path type := `p` | nothing |
| `set_detail_path_type(d)` | target detail path type := `d` | nothing |
| `body_state()`, `target_body_state()` | the current, the target body state | 1 |
| `movement_type()`, `target_movement_type()` | the current, the target movement type | 2 |
| `mental_state()`, `target_mental_state()` | the current, the target mental state | 0 |
| `path_type()` | the current path type | 3 |
| `detail_path_type()` | always 0, whatever the target | 0 |

[X-9]. The numbers are those of the scripts' constant holders (spec V's classes, measured from the listing): body
states `move.crouch` 0, `move.standing` 1; movement types `move.walk` 0, `move.run` 1, `move.stand` 2; mental states
`anim.danger` 0, `anim.free` 1, `anim.panic` 2; detail path types `move.line` 0, `move.dodge` 1, `move.criteria` 2;
path types have no holder (3 is believed "no path", the fallback) [X-10]. So the fallbacks read as standing, standing
still, danger [X-9, X-10].

- `set_desired_position(pos)` with a vector: the request's desired position := `pos`, marked set;
  `set_desired_position()` or with nil: marked not set (the position kept). `set_desired_direction` the same for
  the desired direction [X-9]. 11 and 12 loadable scripts call them (24 and 25 in all) [X-24].
- `set_dest_level_vertex_id(v)`: if `v` is below the level's number of level vertices, the destination vertex :=
  `v`, and the path is marked to be rebuilt when the vertex changed; otherwise nothing, without a diagnostic
  [X-9].
- `head_orientation()`: a new vector, the unit direction of the stalker's head, made from the head's current
  heading and pitch, both negated, by the engine's heading-and-pitch to direction conversion (**believed** the one
  behind E's `setHP`) [X-11]; fallback (FLT_MAX, FLT_MAX, FLT_MAX).

## 4. Paths, patrol paths and position finding (static)

### 4.1 Patrol paths

- `set_patrol_path(name, start, route, random)` (stalker): the stalker's patrol path becomes the patrol path
  `name` of the level (looked up by name in the level's patrol paths, spec N); when the path changes, its current,
  previous and start point indices are reset to "none" (−1); the **start type** := `start`, the **route type** :=
  `route` and a **random** flag := `random`; a changed start or route type marks the path to be rebuilt [X-12].
  The scripts pass `patrol.start`, `patrol.next`, `patrol.nearest` and `patrol.continue` (spec N's holder)
  [X-24]. A name without a patrol path: **unestablished** (believed: the path is cleared).
- `patrol()`: text, the name of the patrol path: for a stalker the name its patrol path was set with, or `""`
  with a diagnostic when none is set; for another scripted object its scripted part's patrol path name (CH-24);
  fallback `""` [X-12].
- `get_current_point_index()` (fighter): the current point index of its patrol path (−1 when none); fallback −1
  [X-12].
- `set_start_point(i)`, `set_previous_point(i)` (fighter): set the start, the previous point index to `i`, when a
  patrol path is set and `i` is a point of it; otherwise nothing (two diagnostics, M 2.3) [X-12].
- `patrol_path_make_inactual()` (fighter): marks its patrol path to be rebuilt [X-12].
- `path_completed()` (fighter): `true` when the fighter's path is marked up to date and its path-building state is
  the state numbered 8 (**believed** "path completed"; the states are CH-9's); otherwise `false`; fallback
  `false` [X-12].

### 4.2 The detail path

- `location_on_path(d, out)` (fighter): with `out` nil, a diagnostic and −1. Otherwise `out` is first set to the
  fighter's position; if the fighter has a detail path (CH-10) beyond its current point, the first later point
  whose segment from the point before it is longer than `d` (each segment compared alone, not summed) is written to
  `out` and the point's travel-speed number returned, or the last point and its number when no segment is longer;
  without such a path the result is a number of the fighter's movement state (**unestablished** which) [X-12].
  `xr_patrol.script` calls `commander:location_on_path(15, pos)` [X-24].
- `extrapolate_length()` (fighter): the detail path's extrapolate length; fallback 0. `extrapolate_length(v)` sets
  it, marking the detail path to be rebuilt when it changes by 10^−5 or more [X-12].
- `movement_enabled(b)` (fighter): enables or disables movement; disabling records the fighter's position; enabling
  again marks the path to be rebuilt when the fighter has moved by 0.001 or more on any axis since. `movement_enabled()`
  reads the flag; fallback `false` [X-12].

### 4.3 Vertices

- `vertex_in_direction(v, dir, dist)` (fighter): if level vertex `v` is not accessible to the fighter (section
  5's test, radius 0.001), a diagnostic and −1. Otherwise `dir` is normalised (when its squared length exceeds the
  smallest normal float), the point `p` = position of `v` + `dir`·`dist` is taken, and the result is the last level
  vertex the level graph's straight walk from `v` toward `p` reaches through vertices the fighter may enter; when that
  is not a valid vertex, `v` itself [X-13]. The walk is spec N's (`level.vertex_in_direction`, which most scripts
  use instead); `xr_attendant.script` calls the member [X-24].

## 5. Space restrictions and accessibility (static)

Restrictions are given as texts listing restrictor names separated by commas (CH-9) [X-14]. All need the fighter
kind.

| Member | Does | Fallback |
|---|---|---|
| `add_restrictions(out, inside)` | adds the restrictors of `out` to the fighter's out-restrictions and those of `inside` to its in-restrictions, in the level's restriction manager, when either text is not empty; then marks its path to be rebuilt | nothing |
| `remove_restrictions(out, inside)` | removes them the same way | nothing |
| `remove_all_restrictions()` | removes every added restriction of both kinds, then (if any was set) marks the path to be rebuilt | nothing |
| `in_restrictions()`, `out_restrictions()` | text: the fighter's current in-, out-restrictions (**believed** base and added; `""` when none) | `""` |
| `base_in_restrictions()`, `base_out_restrictions()` | text: those it has from its spawn data | `""` |
| `accessible(pos)` | `true` when a sphere of radius 0.001 at the vector `pos` is accessible under its restrictions | `false` |
| `accessible(v)` | the same for level vertex `v` | `false` |
| `accessible_nearest(pos, out)` | number: the nearest accessible level vertex to `pos`; its position is written to the vector `out` | −1 |

[X-14]. The first argument is the out-restrictions: `xr_gulag.script` calls `obj:add_restrictions(change_r_out,
change_r_in)` [X-24]. Which positions are accessible is CH-9's.

## 6. Sight (static)

### 6.1 `set_sight` and the sight request

A stalker keeps a **sight request**: a sight type, a vector, an object and three flags *A*, *B*, *C* [X-15].
`set_sight` builds a new request and hands it to the stalker's sight control, which follows it (CH-10's turning of
body and head) [X-15]. All forms need the stalker kind and do nothing on another object.

| Form | Sight type | Vector | Object | *A* | *B* | *C* |
|---|---|---|---|---|---|---|
| `set_sight(t, vec)`, `set_sight(t, vec, n)` | `t`, except 10 (`eSightTypeFirePosition`) becomes 3 (`eSightTypePosition`) | `vec` (nil: not set) | none | 1 if `t` was 10, else 0 | 0 | not set |
| `set_sight(t, a, b)` | `t` | not set | none | `a` | `b` | not set |
| `set_sight(t, vec, a)` (`vec` a vector) | `t` | `vec` | none | `a` | 0 | not set |
| `set_sight(obj)` | 4 (`eSightTypeObject`) | not set | `obj`'s object | 0 | 0 | 0 |
| `set_sight(obj, a)` | 4 | not set | `obj`'s object | `a` | 0 | 0 |
| `set_sight(obj, a, f)` | 9 (`eSightTypeFireObject`) if `f`, else 4 | not set | `obj`'s object | `a` | 0 | 0 |
| `set_sight(obj, a, f, c)` | 9 if `f`, else 4 | not set | `obj`'s object | `a` | 0 | `c` |

[X-15]. The number `n` of the first form is not used [X-15]. "Not set" means the field is not written (its value is
**unestablished**; believed left over from earlier stack contents, unused for those types). The meaning of the
flags is CH-10's: the listing's argument order and the scripts' use read *A* as "turn the torso only" (believed),
*B* and *C* are **unestablished**. The scripts pass `look` constants (spec V's holder, whose values are this
section's types: `look.cur_dir` 0, `look.path_dir` 1, `look.direction` 2, `look.point` 3, `look.danger` 5,
`look.search` 6, `look.fire_point` 10) [X-10, X-24].

### 6.2 `sight_params()`

Returns a new `CSightParams` holding the current request's type (`m_sight_type`), object (`m_object`: its handle,
nil when none) and vector (`m_vector`) [X-15]. On an object that is not a stalker: type −1, object nil, vector
(FLT_MAX, FLT_MAX, FLT_MAX) [X-15].

### 6.3 `CSightParams`

`CSightParams()` makes one with unset fields (**unestablished** values). Its properties are readable only. Its
constants are the sight types [X-6]:

| Constant | Value | Constant | Value |
|---|---|---|---|
| `eSightTypeDummy` | −1 | `eSightTypeCover` | 5 |
| `eSightTypeCurrentDirection` | 0 | `eSightTypeSearch` | 6 |
| `eSightTypePathDirection` | 1 | `eSightTypeLookOver` | 7 |
| `eSightTypeDirection` | 2 | `eSightTypeCoverLookOver` | 8 |
| `eSightTypePosition` | 3 | `eSightTypeFireObject` | 9 |
| `eSightTypeObject` | 4 | `eSightTypeFirePosition` | 10 |

### 6.4 `get_current_direction()`

On the searchlight class (section 2): a new vector, the unit direction of its current heading and pitch by the
conversion of section 3; otherwise (0, 0, 0) [X-11].

## 7. Vision and memory (static)

### 7.1 The memory records

The memory of a fighter holds records of what it has seen, heard and been hit by (CH-15). Scripts receive records
as these classes; every property is a copy read when it is read [X-5, X-16]:

| Class | Members |
|---|---|
| `memory_object` | `level_time`, `last_level_time`: numbers (**believed** now when the record was last and next-to-last updated, in milliseconds) |
| `game_memory_object`, `entity_memory_object` | `object_info`, `self_info`: `object_params` values (spec N: `level_vertex` and `position`; **believed** where the remembered object and the remembering fighter were); `object()`: the remembered object's handle |
| `visible_memory_object` | the members of `game_memory_object` (a seen object) |
| `memory_info` | `visual_info`, `sound_info`, `hit_info`: booleans (**believed**: which of the three memories the information comes from) |
| `sound_memory_object` | `power`: number (**believed** the heard power, CH-14); `type()`: number, the sound's type (T's `snd_type` bits, believed) |
| `hit_memory_object` | `direction`: vector; `bone_index`: number (16-bit); `amount`: number (**believed** the hit's power) |
| `not_yet_visible_object` | `value`: number (**believed** the visibility accumulated so far, CH-13); `object()`: the object's handle |

The `object()` member is called as a method: `rec:object()` [X-16].

### 7.2 Iterating the memories

`memory_visible_objects()`, `memory_hit_objects()` and `not_yet_visible_objects()` (fighter) return the fighter's
seen, hitting and not-yet-seen records for use in `for r in npc:not_yet_visible_objects() do … end`, as T 7.2's
`memory_sound_objects` (**believed** the same iteration; static: they hand the binding the memory's list itself)
[X-16]. On another object they read through no object (section 2). The non-loadable `xr_reactions.script` and
`xr_talker.script` call `not_yet_visible_objects()` [X-24].

### 7.3 Members on remembered objects

- `memory_time(obj)` (fighter): 0 when the fighter is dead (health 0 or less); otherwise the greatest `level_time`
  among its seen, heard and hitting records of `obj`'s object, 0 when there is none [X-16].
- `memory_position(obj)` (fighter): a new vector: (0, 0, 0) when the fighter is dead or has no record of `obj`;
  otherwise the `object_info` position of whichever of its seen, heard and hitting records of the object has the
  greatest `level_time` (the first of these, in that order, on a tie) [X-16]. Fallback (0, 0, 0).
- `enable_memory_object(obj, b)` (fighter): sets an enabled flag to `b` on the fighter's seen, heard and hitting
  records of `obj`'s object, those it has (**believed**: a disabled record is not used for enemy choice, CH-15)
  [X-16].
- `make_object_visible_somewhen(obj)` (stalker): `obj` must be a living object (else a diagnostic, nothing);
  updates the stalker's seen record of the object as if seen now (records it with full visibility for each of its
  visibility checks) and keeps the record's flag for the current viewer (**believed**: the object counts as seen,
  CH-13) [X-16].

### 7.4 Vision

| Member | Kind | Does | Fallback |
|---|---|---|---|
| `fov()`, `range()` | living | the eye's field of view (degrees, believed), the eye's range | −1 |
| `set_fov(v)`, `set_range(v)` | fighter | sets the eye's field of view, range | nothing |
| `enable_vision(b)`, `vision_enabled()` | fighter | sets, reads the vision flag of its visual memory | `false` |
| `visibility_threshold()` | fighter | the visibility threshold of its vision parameters: of one set when the fighter is a stalker whose current mental state is 0 (danger), else of the other (**believed** the danger and the free sets of CH-13) | — (section 2) |
| `see(obj)` | see below | | `false` |
| `see(section)` | fighter | `true` when the fighter currently sees an object of the configuration section `section` (believed: any of its visible records with that section, CH-13) | `false` |

[X-17]. `see(obj)`: on a living object that is dead, a diagnostic and `false`; on a scripted object, whether its
vision currently sees `obj`'s object (CH-13); on the player, whether the player's visual memory holds the object
as seen; otherwise `false` [X-17].

## 8. Enemies and dangers (static)

### 8.1 Enemies

- `best_enemy()` (fighter): the handle of the enemy its enemy choice selected (CH-15), nil when none; fallback nil
  [X-18].
- `get_enemy()` (fighter): the handle of its current enemy as its fighting logic holds it, nil when none or when
  that object carries the state flag of T 7.4's `who`; fallback nil [X-18].
- `get_enemy_strength()` (scripted): a number the scripted part gives (CH-24; **unestablished** what it measures);
  fallback 0 [X-18].
- `ignore_monster_threshold(v)` (stalker): the enemy choice's ignore-monster threshold := `v` limited to 0 … 1.
  `ignore_monster_threshold()` reads it (fallback 0). `restore_ignore_monster_threshold()` sets it back to the
  stalker's section key `ignore_monster_threshold`, 1 when the key is absent [X-18].
- `max_ignore_monster_distance(v)`, `max_ignore_monster_distance()` (stalker): set, read the maximum ignore distance
  (fallback 0); `restore_max_ignore_monster_distance()` sets it to the section key `max_ignore_distance`, 0 when
  absent [X-18]. What both do is CH-15's.
- `skip_transfer_enemy(b)` (monster): sets a flag of the monster to `b` (**believed**: whether it passes its enemy
  to the monsters around it, CH-22; `mob_camp.script` sets it from its `skip_transfer_enemy` key) [X-18, X-24].

### 8.2 Dangers and `danger_object`

`best_danger()` (fighter) returns the danger its danger choice selected as a `danger_object`, nil when none or on
another object [X-18]. A `danger_object` is the danger's record [X-5]:

| Member | Result |
|---|---|
| `object()` | the handle of the object the danger comes from; nil when none |
| `dependent_object()` | the handle of a second object of the danger (**believed**: for a corpse or an attacked object, who caused it); nil when none |
| `position()` | a new vector: where the danger is |
| `time()` | number: when it was perceived (believed now, ms) |
| `type()` | number: the danger type, one of the constants below |
| `perceive_type()` | number: how it was perceived: `visual` 0, `sound` 1, `hit` 2 |
| `a == b` | `true` when both have no object or objects with the same id, and the same type and perceive type |

The danger types: `bullet_ricochet` 0, `attack_sound` 1, `entity_attacked` 2, `entity_death` 3, `entity_corpse` 4,
`attacked` 5, `grenade` 6, `enemy_sound` 7 [X-5]. (`danger_object.hit` 2 is a perceive type; the listing has both
groups in one class, as does the registration.) What makes a danger is CH-19's.

## 9. Covers (static)

- `best_cover(pos, enemy_pos, radius, min_d, max_d)` (stalker): searches the level's cover points within `radius`
  of `pos`, judged against an enemy at `enemy_pos` with the enemy-distance limits `min_d`, `max_d` (CH-18's
  evaluator), and returns the best cover point (spec N's `cover_point`), nil when none; a repeated call with the
  same arguments (each within 10^−5) reuses the evaluator's previous result unless 1000 ms have passed or the
  restrictions changed (**believed** from the stored time and the change test) [X-19].
- `safe_cover(pos, radius, min_d)` (stalker): the same with the safe-cover evaluator and its minimum distance
  `min_d` [X-19]. Fallback of both: nil, with `best_cover`'s diagnostic [X-19]. No shipped script calls them [X-24].

## 10. Items, weapons and animations (static)

### 10.1 `set_item`

`set_item(action[, obj[, q[, i]]])` (stalker) gives the stalker's object handler a target: the object action
`action` (spec V's `object` constants: `idle` 8, `strap` 9, `fire1` 6, `aim1` 4, …) on `obj`'s object (none when
omitted or nil) [X-20]:

| Form | Queue size | Interval |
|---|---|---|
| `set_item(action)`, `set_item(action, obj)` | −1 (taken as 1) | 300 |
| `set_item(action, obj, q)` | `q` | 300 |
| `set_item(action, obj, q, i)` | `q` | `i` |

The target is a state made from the object's id and the action (the action converted to a handler state first);
for a weapon that cannot be strapped one such state is replaced by another (**believed** strapping by idling), and
no object gives a fixed "none" target [X-20]. When the object is a weapon with a magazine and the queue or
interval changed (or the previous interval has run out), the weapon's fire queue size := max(`q`, 1) and the next
interval := `i` milliseconds after now (an interval of 0 is taken as 300 for two of the handler's waits,
**believed** between shots); what firing does is CH-20's
[X-20].

### 10.2 Weapons

- `set_queue_size(n)` (weapon): sets the weapon's fire queue size to `n` when it has a magazine; otherwise nothing
  [X-20].
- `weapon_strapped()` (stalker): `false` unless its active item is a weapon that can be strapped; then `false` while
  the stalker's weapon state is one of four transition states (numbers 3 … 6; **unestablished** which motions),
  otherwise whether the weapon is strapped [X-20]. `weapon_unstrapped()`: `true` unless the active item is such a
  weapon; then `false` in those states, otherwise whether it is not strapped [X-20]. Fallback `false`.
- `accuracy()` (inventory owner): its accuracy number (CH-20); fallback −1 [X-17].

### 10.3 Script animations and cycles

- `add_animation(name, a, b)` (stalker): when the stalker's model has a motion `name`, appends it with the flags `a`,
  `b` to the stalker's queue of script animations; otherwise a diagnostic and nothing (CH-11 plays the queue)
  [X-21]. The scripts always pass three arguments, e.g. `add_animation(v, true, false)` [X-24].
- `clear_animations()` (stalker): empties the queue and resets the script animation being played [X-21].
- `animation_count()` (stalker): number of queued script animations; fallback −1 [X-21].
- `animation_slot()` (item): the item's inventory animation slot number; fallback −1 [X-21].
- `play_cycle(name)`, `play_cycle(name, mix)` (any object with an animated model): plays the model's cycle `name`,
  mixing when `mix` is true (the one-argument form mixes); a model that is not animated, or a cycle it lacks, gives a
  diagnostic and nothing [X-21].

### 10.4 `hide_weapon()` and `restore_weapon()`

Both ignore the object they are called on [X-21]. If the player is alive (health above 0) and is the level's current
controlled object, each sends the player the game event that hides (`hide_weapon`, flag 1) or restores
(`restore_weapon`, flag 0) the player's weapon, with the slot argument −1 (**believed**: all slots); otherwise
nothing [X-21]. What the player does with the event is the player's (`player-state.md`; **unestablished** there).
`bind_stalker.script` and `xr_effects.script` call them on the player [X-24].

## 11. Monsters (static)

### 11.1 Home and jump

- `set_home(name, r_min, r_max, aggressive)` (monster): the monster's home := the level's patrol path `name` (none
  when the level has no such path), the radii `r_min`, `r_max` and the flag `aggressive` (CH-22) [X-22].
- `remove_home()` (monster): clears the home's path and flag [X-22].
- `jump(pos, factor)` (monster): asks the monster to jump to `pos` with the factor `factor` (CH-12, CH-22);
  otherwise a diagnostic [X-22].
- `berserk()` (monster): records now as the monster's berserk start (**believed**: it then attacks regardless of
  morale for a time, CH-22) [X-22].

### 11.2 Abilities

- `set_alien_control(b)` (`CAI_Bloodsucker`): `true` starts, `false` ends the bloodsucker's control of the player's
  view (CH-23: while active the bloodsucker is hidden, the player's weapon is hidden by the event of 10.4, the
  player's camera and screen effects are added and the HUD flag is cleared; ending restores them) [X-22].
- `fake_death_fall_down()` (`CZombie`): if the zombie is not already in fake death, starts it and returns `true`,
  else `false`; `fake_death_stand_up()` ends it (up to 4 tries of its stand-up test) [X-22]. Fallback `false`,
  nothing.

### 11.3 Panic thresholds

Each monster has a **panic threshold** and a **default panic threshold** [X-23]:

- When the monster loads, its section's key `panic_threshold` is read (without a presence test, so a section lacking
  it is fatal, F) into the threshold, and the default is set equal to it [X-23].
- `set_custom_panic_threshold(v)` sets the threshold to `v`; `set_default_panic_threshold()` sets it back to the
  default [X-23]. On an object that is not a monster both do nothing (no diagnostic).
- On `CAI_Trader` objects (not monsters, section 2) both do nothing; so a script that calls them on the trader, as
  `mob_panic.script` does for whatever object its scheme runs on, changes nothing there [X-7, X-23].
- The threshold is an input to the monster's decision whether to fight or flee its enemy (CH-22), which also reads
  two more values of the monster and a distance of 30 (**believed** meters) [X-23].
- The creature files of `config\creatures\` give it values from 0 to 0.5 (0.05, 0.1, 0.2 and 0.5 the common ones)
  [X-23]. Stalkers have a field at the same place, loaded from the specific character's `panic_threshold` when that
  is not 0 (CH-18); the members do nothing on stalkers, which are not monsters [X-23].
- `mob_panic.script` reads the scheme key `panic` and calls `set_custom_panic_threshold(v)` when present, else
  `set_default_panic_threshold()` [X-24].

### 11.4 `get_monster_hit_info()` and `MonsterHitInfo`

`get_monster_hit_info()` (monster) returns a new `MonsterHitInfo` [X-22]:

- With no hit recorded: `who` nil, `direction` (0, 0, 1), `time` 0 [X-22].
- Otherwise, of the monster's recorded hits the one with the greatest time: `who` its hitter's handle (nil when none or
  being destroyed), `time` that time, `direction` the monster's own facing direction (heading and pitch taken from
  its forward axis) turned by the side the hit came from: side 0 unchanged, 1 by π, 2 by +π/2, 3 by −π/2 in
  heading, as a unit vector [X-22]. Which side a hit gets is CH-21's.

Its properties are readable and writable: `who` a handle or nil, `direction` a vector, `time` a number made an
integer; assigning changes the copy only [X-6, X-22]. The scripts compare `time` with `time_global()` (believed ms)
[X-24].

### 11.5 Fallbacks of section 11

`get_monster_hit_info` as "no hit", `jump` and the class-tested members nothing; the others of section 11.1 – 11.3
nothing [X-8, X-22].

### 11.6 `MonsterSpace`

Constants: `head_anim_normal` 0, `head_anim_angry` 1, `head_anim_glad` 2, `head_anim_kind` 3 (**believed** head
motions), `sound_script` 128 (**believed** a sound type for scripted monster sounds) [X-6]. The listing agrees [X-6].

## 12. `cef_storage` (static)

`cef_storage` is the store of evaluation functions (CH-16); scripts reach it through spec W's global
`ef_storage()`. `evaluate(name, o1[, o2[, o3[, o4]]])` evaluates the function `name` for up to four game objects
(handles) or, in the other four forms, up to four simulation objects (spec J), and returns a number [X-6]. What the
functions compute is CH-16's. No shipped script calls `ef_storage()` [X-24].

## 13. Absent names (measured)

`nearest_accessible_position`, `set_node_evaluator` and `set_path_evaluator` occur nowhere in the game library [X-2].
A script reaching one gets a call of a missing member (A) [X-2]; the scripts that name them are the scoping spec's
absent-name scripts.

## 14. Use by the shipped scripts (measured)

Loadable scripts (all scripts) calling, by the scoping spec's method-name rule: `set_desired_position` 11 (24),
`set_desired_direction` 12 (25), `best_danger` 7 (7), `set_item` 3 (10), `add_animation` 3 (5),
`get_monster_hit_info` 3 (3), `set_sight` 2 (9), `hide_weapon` and `restore_weapon` 2 (3), `set_patrol_path` 1 (3),
`add_restrictions` 1 (1) [X-24]. Not called by any shipped script: `best_cover`, `safe_cover`,
`memory_visible_objects`, `memory_hit_objects`, `ef_storage` [X-24].

## 15. Verification facts

- Items: 160 listed (88 `game_object` functions; 12 classes and their members); 85 names used by the loadable
  scripts in 59 (121) loadable scripts [X-2].
- Forms: `set_sight` 8, `set_item` 4, `add_animation` 1 with three arguments [X-3, X-4].
- Kinds: fighter 18, stalker 1, monster 17, scripted 20 classes [X-7].
- Constants: sight types −1 … 10; danger types 0 … 7 and perceive types 0 … 2; `MonsterSpace` 0 … 3 and 128 [X-5,
  X-6].
- Worked example (6.1): `npc:set_sight(look.fire_point, pos, 0)` makes a request of type 3 with flag *A* 1 and vector
  `pos`; `sight_params()` then returns type 3 and `pos` [X-15].
- Worked example (11.3): a monster loaded from a section with `panic_threshold = 0.1` has threshold and default 0.1;
  `set_custom_panic_threshold(0.5)` makes the threshold 0.5; `set_default_panic_threshold()` makes it 0.1 again
  [X-23].

## 16. Unestablished

- The unwritten fields of a sight request (6.1) and of a new `CSightParams` (6.3); what flags *B* and *C* do.
- What `patrol()`'s and `get_enemy_strength()`'s scripted parts return (4.1, 8.1); what a missing patrol path name
  does (4.1); the movement-state number `location_on_path` returns without a path (4.2).
- The meaning of the path-building state 8 (4.1) and of path type numbers other than the fallback (section 3).
- Which four weapon states block `weapon_strapped` (10.2).
- What the player does with the weapon event (10.4).
- Everything behind the calls: CH-9 … CH-24 as cited.

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` as identified above; the
  scripts and configuration of the archives `gamedata.db0` … `gamedata.dbd` as extracted for the approved script
  specs.
- Analysis: static analysis of `xrGame.dll` with Ghidra (headless, read-only: decompilation and disassembly of the
  registration of the classes of this spec and of `game_object`'s members of this spec, and of the routines they
  call; run-time type information for the kinds of section 2); a parser of the listing; the scoping spec's scan of
  the 439 scripts; a scan of `config\creatures\`. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- No member of this spec is shown to hand scripts a `memory_info` record; which member, if any, produces one is not
  stated.
