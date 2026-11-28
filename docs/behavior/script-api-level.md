# Script API N: level

What the core of the `level` namespace of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition), as scripts see it: the namespace `level` itself, with its functions for the level's presence and
name, objects by id, level vertices, the level's bounding volume, the physics world, phantoms and the client spawn
manager; the class `patrol` (patrol paths); the classes `cover_point` and `object_params`; and the class
`client_spawn_manager` with the engine's calls of the functions handed to its `add`. This is spec N of the approved
scoping specification `docs/behavior/script-api-scope.md` (section 6, order 19); its scope rule and its review gaps
apply: this spec owns every name of these classes and of the namespace as the game library registers them, listed or
not, and the entry point `add` of the client spawn manager (its section 4.5).

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type, the 250
  loadable scripts, namespaces as tables reachable from the globals).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched and
  converted, what a call of a missing member does; where a statement needs it, it says **depends on A**); spec D,
  `docs/behavior/script-api-engine-entry-points.md` (1.2: an error in a protected call of a script function is
  fatal); spec E, `docs/behavior/script-api-value-types-and-math.md` (`vector`, `Fbox`, `flags32`; a vector or box
  result is a new object, 2.3, 2.4); spec M, `docs/behavior/script-api-game-object-basics.md` (the **handle** of an
  engine object, 2.1; the diagnostics routine that returns at once, 2.3; the level vertex and game vertex of an
  object, 4); spec O, `docs/behavior/script-api-object-event-callbacks.md` (how the engine calls a function with or
  without a bound value, 3.2; the level's call list and when the level removes all of its objects, 6.5).
- `docs/behavior/offline-simulation.md` (cited as `offsim`): `game.graph`, `levels\<level>\level.ai` and its
  21-byte level vertex records and vertex positions (3), the patrol paths of `spawns\all.spawn` chunk 3 (4),
  simulation objects; `docs/behavior/level-loading.md` (the level list and which level is loaded, 2.2);
  `docs/formats/level-cform.md` (the header of `level.cform`).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the engine"
  `bin\XR_3DA.exe` (1,636,424 bytes, MD5 `ba3ad0127bff6619ed991a7c82d8b946`) [N-1]. **The listing** is
  `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from the game
  library's registration code.
- **The level** is the level being played, as the game keeps it while it is loaded; it exists from when the game
  creates it until it is destroyed (believed: from the start of loading until the player leaves it). A call is
  written as scripts write it: `level.vertex_position(v)`, `p:point(0)`. A **number** is the runtime's double;
  a **text** is a Lua string; **nil** is Lua's nil. A **level vertex** is a vertex of the level's `level.ai`
  (`offsim` 3), numbered from 0; *N* is their count; a vertex's **cell** is its column and row.
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (read from
  game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [N-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-level/run_all.py` (`python3 -I run_all.py`): it reads the binaries' bytes and run-time type
information, the evidence of read-only headless static analysis of the game library, the engine and the binding
library (produced by `gh.py` beside it), the listing, the scoping spec's scan and the shipped scripts and data. All
checks pass (`ALL PASS 16`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 41 listed items [N-2]:

| Owner | Items |
|---|---|
| namespace `level` | 14 functions: `check_object`, `client_spawn_manager`, `cover_in_direction`, `debug_actor`, `debug_object`, `get_bounding_volume`, `name`, `object_by_id`, `patrol_path_exists`, `physics_world`, `present`, `spawn_phantom`, `vertex_in_direction`, `vertex_position` (sections 2, 3, 7) |
| `patrol` | the class; 10 functions and 7 constants (section 4) |
| `cover_point` | the class; `level_vertex_id`, `position` (5.1) |
| `object_params` | the class; the properties `level_vertex`, `position` (5.2) |
| `client_spawn_manager` | the class; `add`, `remove` (section 6) |

and the two names `level.actor` and `level.object`, which the scripts use and the listing lacks (section 7). The
scoping spec assigns this spec two candidate strings, `current` and `deadbody_location` (1.4) [N-2].

### 1.2 The namespace `level` as registered (static)

The game library opens the namespace `level` in exactly one place and registers in it 59 functions under 53
distinct names, and nothing else (no class) [N-3]. Every registered name is listed; the listing's 56 names add
three that the game library does not register at all and whose names occur nowhere in it: `check_object`,
`debug_actor` and `debug_object` (section 7) [N-3]. Four names have more than one form, told apart by their
arguments (**depends on A**): `add_call` and `remove_call` three each, `iterate_sounds` and
`set_pp_effector_factor` two each [N-3].

Of the 53 names, the scoping split gives 11 to this spec and the rest to the specs owning their function [N-3]:

| Spec | Names |
|---|---|
| N (this spec) | `client_spawn_manager`, `cover_in_direction`, `get_bounding_volume`, `name`, `object_by_id`, `patrol_path_exists`, `physics_world`, `present`, `spawn_phantom`, `vertex_in_direction`, `vertex_position` |
| O | `add_call`, `remove_call`, `remove_calls_for_object` |
| H | `get_time_days`, `get_time_hours`, `get_time_minutes`, `get_time_factor`, `set_time_factor` |
| U | `add_cam_effector`, `add_cam_effector2`, `add_complex_effector`, `add_pp_effector`, `environment`, `get_weather`, `is_wfx_playing`, `rain_factor`, `remove_cam_effector`, `remove_complex_effector`, `remove_pp_effector`, `set_pp_effector_factor`, `set_weather`, `set_weather_fx` |
| T | `get_snd_volume`, `iterate_sounds`, `prefetch_sound`, `set_snd_volume` |
| G4 | `add_dialog_to_render`, `disable_input`, `enable_input`, `hide_indicators`, `main_input_receiver`, `remove_dialog_to_render`, `show_indicators`, `start_stop_menu` |
| P | `get_game_difficulty`, `map_add_object_spot`, `map_add_object_spot_ser`, `map_change_spot_hint`, `map_has_object_spot`, `map_remove_object_spot`, `set_game_difficulty` |
| Z | `game_id` |

The code that opens the namespace first builds the two environment classes of spec U (scoping spec 2.2); they are
built before the namespace is opened and are not among its contents [N-3].

The forms of this spec's 11 functions, as registered (arguments and results in script terms) [N-3]:

| Function | Arguments | Result |
|---|---|---|
| `name()` | none | text |
| `present()` | none | boolean |
| `object_by_id(id)` | number | a handle or nil |
| `vertex_position(vertex)` | number | a vector |
| `vertex_in_direction(vertex, direction, distance)` | number, vector, number | number |
| `cover_in_direction(vertex, direction)` | number, vector | number |
| `patrol_path_exists(name)` | text | boolean |
| `get_bounding_volume()` | none | an `Fbox` |
| `physics_world()` | none | an object of a class that is not registered (2.5) |
| `spawn_phantom(position)` | vector | nothing |
| `client_spawn_manager()` | none | the `client_spawn_manager` object (section 6) |

### 1.3 The classes as registered (static)

All four classes are registered in the global namespace, with no base class [N-9, N-11, N-12]:

| Class | Constructor | Registered members |
|---|---|---|
| `patrol` | five forms, 1 to 5 arguments (4.2) | the 10 listed functions and the 7 listed constants [N-9] |
| `cover_point` | none | `position`, `level_vertex_id` [N-11] |
| `object_params` | none | the read-only properties `level_vertex`, `position` [N-11] |
| `client_spawn_manager` | none | `add` (two forms), `remove` [N-12] |

Every registered name matches the listing [N-9, N-11, N-12]. Scripts cannot create a cover point, an object
parameter record or a spawn manager; they receive them from the engine.

### 1.4 The candidate strings (static)

Neither is a registration of this spec [N-15]: `current` is a method registered on the environment class of spec U
(next to which the scoping spec's rule placed it), and `deadbody_location` is a text the game library keeps as a
shared string, with no registration call.

## 2. The level (static)

Unless a member says otherwise, the functions of this section and section 3 use the level without testing that it
exists; called while there is no level (in the main menu, before a game is loaded) they read through a missing
object (**believed**: the program crashes) [N-4]. The two loadable scripts that call `level.present()` are the
main-menu scripts `ui_main_menu.script` and `ui_mp_main.script` [N-16].

### 2.1 `level.present()`

True when the level exists, false otherwise; the only function of this spec that tests it [N-4].

### 2.2 `level.name()`

The name of the level: the text the level keeps as its name [N-4]. That text is set when the level is started on
the client side: the game takes the level name the game server reports, looks it up in the level list
(level-loading.md 2.2; a name not in the list ends the start with a disconnect), loads that level and keeps the
name [N-4]. So it is the name of the loaded level's directory under `$game_levels$` as the server spelled it, for
example `l01_escape` on a New Game (level-loading.md 2.2; **believed**: the server reports the level name of
`game.graph`, `offsim` 1.2). A level without a name gives nil (believed never). The scripts compare the result with
level names as texts (measured, section 9).

### 2.3 `level.object_by_id(id)`

The handle (M 2.1) of the level's object whose id is `id`, or nil when the level has no such object [N-4]. The
lookup is in the level's table of its objects by id, where each object is entered with its 16-bit id (M 3) [N-4];
**believed**: an object is in the table from its spawn until it is destroyed, so an object that is offline gives
nil. An `id` of 65,536 or more is never found (the lookup compares the full number) [N-4]. Converting the argument
**depends on A**. The handle is made on first request as M 2.1 describes [N-4].

### 2.4 `level.get_bounding_volume()`

A new `Fbox` (spec E 5.2) holding the level's bounding box: `min` and `max` are the bounding box minimum and maximum
of the header of the level's `level.cform` (level-cform.md: the two `float3` at bytes 12 and 24), as the engine reads
them when it loads the level's collision data [N-4]. For `l01_escape`: `min` (−678.897, −38.667, −633.791), `max`
(766.565, 113.877, 811.671) (measured) [N-16].

### 2.5 `level.physics_world()`

Hands the level's physics world (`docs/behavior/physics-world.md`) to the runtime as an object of a class that the
game library never registers for scripts: the only code that refers to that class's type is this conversion, and
it looks the class up and uses the result without a test [N-14]. The outcome is **unestablished** (**believed**:
the program crashes, as the class record read does not exist). Whether a missing physics world gives nil is
**unestablished** (the conversion itself has no test) [N-14]. No script calls it (section 9).

### 2.6 `level.spawn_phantom(position)`

Asks the game server to create a new object of section `m_phantom` at `position` [N-4]. The game library builds a
spawn record of that section (with no section of that name: a fatal failed assertion `Cannot find item with
section`), with the position given, no level vertex (the number −1), no parent, and the flags and other fields of a
new object of a local spawn request, sends it to the game server as a spawn message, and deletes its own copy; it
returns nothing and creates nothing itself [N-4]. The new object appears when the server's spawn reaches the level
(**believed** a later frame; the server and client spawn steps are not this spec's). The section is shipped in
`config\creatures\m_phantom.ltx` (`[m_phantom]:monster_base`, `class = AI_PHANT`, `script_binding =
phantom_manager.bind`) (measured) [N-4].

### 2.7 `level.client_spawn_manager()`

The level's client spawn manager (section 6): one object per level, created with the level and deleted with it; the
result refers to it and does not own it [N-4]. A script keeping it after the level is destroyed holds a value whose
object no longer exists (**believed**: as M 2.1 for handles).

## 3. Level vertices (static, measured)

### 3.1 What the level vertex records hold

The functions of this section read the level's `level.ai` records (`offsim` 3) [N-5, N-6, N-7]. Besides the
position (bytes 16–20, `offsim` 3), they use:

| Bytes | Content |
|---|---|
| 0–11 (bits 0–91) | four **links**, 23 bits each, at bits 0, 23, 46 and 69 of the little-endian bytes: the level vertex adjacent in direction −x (link 0), +z (link 1), +x (link 2), −z (link 3); the value 8,388,607 (2²³ − 1) means none |
| 12–13 | four **cover values** of 4 bits, the little-endian 16-bit value's bits 0–3 (*c*0), 4–7 (*c*1), 8–11 (*c*2), 12–15 (*c*3) |
| 14–15 | a packed plane of the vertex (used by 3.3 only for heights that do not change its result) |

Measured on `l01_escape` (*N* = 595,499): every link is either none (44,238 links) or a vertex whose column and row
differ from the vertex's own by exactly the step of its direction (−1 column, +1 row, +1 column, −1 row); 396 links
have no link back [N-5]. The direction of each cover value is established by 3.4: *c*0 −x, *c*1 +z, *c*2 +x, *c*3 −z
(**believed** to be the cover the level offers in that direction; byte 11's upper four bits are M 4's light).

### 3.2 `level.vertex_position(vertex)`

A new vector: the position of level vertex `vertex` by the formula of `offsim` 3 [N-5]. The vertex is not tested
against *N* (**believed**: a number beyond the level reads past the records). For vertex 0 of `l01_escape`:
(−283.5, −10.615, −113.4) (measured) [N-5].

### 3.3 `level.vertex_in_direction(vertex, direction, distance)`

A level vertex reached from `vertex` by walking over the links toward a point [N-6]:

1. `direction` is normalised; when its squared length is not above the smallest normal single-precision value
   (about 1.18 × 10⁻³⁸) it is used as given [N-6].
2. The **start** is `vertex_position(vertex)`, the **target** start + direction × `distance`, and *L* the
   distance between them (in three dimensions) [N-6].
3. The walk begins at `vertex`. At each vertex it stops when the target lies inside the level's area (the
   `level.ai` box widened by half a cell) and its cell is the current vertex's cell [N-6]. Otherwise it looks at the
   current vertex's neighbours through its four links (a link counts when it is below *N* and is not the vertex it
   came from), takes those whose cells the segment from start to target crosses in the x–z plane, and moves to the
   one whose crossing point is nearest the target in x–z [N-6]. It stops when no neighbour is crossed, or when the
   x–z distance from the start to the last crossing point is at least *L* + 0.001 [N-6].
4. The result is the vertex last moved to; when the walk never moved, `vertex` itself [N-6].

So the result is the vertex where the straight line from the vertex toward the target leaves the connected area of
the level graph, or the target's vertex when the line reaches it. How the crossing point is taken when the segment
runs along a cell's edge or through its corner (the intersection test reports such cases separately) is
**believed** to follow the same rule. Ties: the neighbours are tried in link order, and a crossed neighbour replaces
the best so far when its crossing point is less than 0.001 farther from the target (in x–z) than the best one's, the
first one being compared with the start; so of nearly equal crossings the later link wins [N-6]. The corner heights the walk computes come from the plane of bytes 14–15;
they do not change which vertex is chosen (static: only x and z are compared) [N-6]. `vertex` is not tested against
*N* (believed: as 3.2).

### 3.4 `level.cover_in_direction(vertex, direction)`

A number in [0, 1] in most directions: the cover values of `vertex` (3.1), each divided by 15, interpolated by the
heading of `direction` [N-7]. Only the x and z components are read [N-7]. With *x*, *z* the components and ε =
10⁻⁷, the heading *a* (radians) is:

| Case | *a* |
|---|---|
| \|*x*\| < ε and \|*z*\| < ε | 0 |
| otherwise, \|*z*\| < ε | −π/2 when *x* > 0, else π/2 |
| otherwise, *z* < 0 | π − atan(*x* / *z*) |
| otherwise | −atan(*x* / *z*) |

Then, with *n*k = *c*k / 15 and the factor 2/π [N-7]:

| Range of *a* | Result |
|---|---|
| *a* < π/2 | *n*1 + (*n*0 − *n*1) · *a* · 2/π |
| π/2 ≤ *a* < π | *n*0 + (*n*3 − *n*0) · (*a* − π/2) · 2/π |
| π ≤ *a* < 3π/2 | *n*3 + (*n*2 − *n*3) · (*a* − π) · 2/π |
| 3π/2 ≤ *a* | *n*2 + (*n*1 − *n*2) · (*a* − 3π/2) · 2/π |

So +z gives *n*1, −x *n*0, −z *n*3, and directions between them interpolate; but *a* is negative for every
direction with *x* > 0 and *z* ≥ 0, so there the first row **extrapolates** from *n*1 away from *n*0 (direction +x
gives 2·*n*1 − *n*0, which can lie outside [0, 1]) instead of reaching *n*2 [N-7]. All arithmetic is single
precision. **Disputed in review:** not all of it: the quotient *x* / *z* and the interpolation are single
precision, but the arctangent and the subtraction π − atan are computed at the x87 unit's extended precision, and
only the heading *a* is then rounded to single precision. The vertex is not tested against *N* (believed: as 3.2). Worked values: section 8.1 [N-7].

## 4. Patrol paths (static, measured)

### 4.1 Where the paths come from, and `level.patrol_path_exists(name)`

The game keeps one store of **patrol paths**, each a named set of points with links [N-8]. With the simulation
running (a single-player game), the store is filled from chunk 3 of `spawns\all.spawn` when the simulation's spawn
data is loaded: the 4,294 paths of `offsim` 4, of all levels, not only the current one [N-8]. Without a simulation
(and with a game graph) it is filled from the level's `level.game` when the level loads (a format that is not this
spec's) [N-8]. A
**point** of a path has the fields of `offsim` 4: its index *j*, name, position, flags (32 bits), level vertex and
game vertex; its **links** are the path's links from it.

`level.patrol_path_exists(name)` is true when the store has a path named exactly `name`, false otherwise [N-8].
Names are compared as exact texts (believed: by the shared-string identity, which is exact and case-sensitive).

### 4.2 The class `patrol`: constructing

`patrol(name[, start[, route[, random[, index]]]])` makes a **patrol object** [N-9, N-10]:

| Argument | Default | Conversion |
|---|---|---|
| `name` | — | a text |
| `start` (a start type, 4.3) | 2 (`patrol.nearest`) | a number, truncated toward zero to an integer |
| `route` (a route type, 4.3) | 1 (`patrol.continue`) | as `start` |
| `random` | true | true when the value is true in Lua's sense |
| `index` | −1 | a number, truncated to an integer, kept as 32 bits |

[N-10]. The path named `name` is looked up when the object is made and kept; a name the store lacks is kept as **no
path**, with no message [N-10]. The other four values are stored as given, without a test; what they do belongs to
the members that receive a patrol object (moving along paths, specs V and X) and is **unestablished** here. The
patrol object belongs to the script side (the runtime collects it, believed).

### 4.3 The constants

The class has two tables of constants, added in this order: start types `start` 0, `stop` 1, `nearest` 2, `custom`
3, `next` 4, `dummy` −1; then route types `stop` 0, `continue` 1, `dummy` −1 [N-9]. A later constant of the same
name replaces an earlier one [N-9]. So scripts see `patrol.start` 0, `patrol.stop` 0, `patrol.nearest` 2,
`patrol.custom` 3, `patrol.next` 4, `patrol.continue` 1, `patrol.dummy` −1, as the listing gives [N-9]: the start
type 1 has no name scripts can read, and `patrol.stop` passed as a start type means 0 (the same as `patrol.start`).

### 4.4 The members

All members read the kept path; on a patrol object with no path, each reads through a missing path (**believed**:
the program crashes) [N-10]. A point index *i* names point *i* of the path; for an index the path lacks, all
members except `point` read through a missing point (**believed**: a crash) [N-10]:

| Member | Result |
|---|---|
| `count()` | number: the number of points [N-10] |
| `point(i)` | a new vector: the position of point *i*; for an index the path lacks, a diagnostic text (`Can't get information about patrol point number %d in the patrol way %s`) is handed to the routine that returns at once (M 2.3) and the position of the path's lowest-indexed point is returned [N-10] |
| `name(i)` | text: the point's name; nil when it has none [N-10] |
| `level_vertex_id(i)` | number: the point's level vertex [N-10] |
| `game_vertex_id(i)` | number: the point's game vertex, 0 … 65535 [N-10] |
| `flags(i)` | a `flags32` (spec E 7) holding the point's flags [N-10] |
| `flag(i, n)` | boolean: whether bit *n* mod 32 of the point's flags is set (*n* taken as 0 … 255) [N-10] |
| `terminal(i)` | boolean: true when the point has no links from it [N-10] |
| `index(name)` | number: the index of the lowest-indexed point named `name` (exact text); −1 when none [N-10] |
| `get_nearest(position)` | number: the index of the point nearest `position` (three-dimensional distance; on a tie the lowest index) [N-10]; on a path without points: believed a crash |

`level_vertex_id(i)` also reads the level of the point's game vertex from the game graph and the current level, and
discards the result [N-10]; for a point whose game vertex is not a vertex of `game.graph` (`offsim` 4: 360 points
carry 51,048), that read lies beyond the graph's vertex table (**unestablished** whether it can fail). The level
vertex returned is the stored one, which may be 4,294,967,295 (none) (`offsim` 4).

## 5. Cover points and object parameters (static)

### 5.1 `cover_point`

A cover point of the level [N-11], received from members of other specs (**believed**: the `game_object` members
`best_cover` and `safe_cover` of spec X, by the listing). `cp:position()` is a vector that refers
to the cover point's stored position (not a copy and not owned; the binding library marks it as a constant
reference) [N-11]; changing it through a method is **depends on A**; using it after the cover point is gone is
**unestablished**. `cp:level_vertex_id()` is the number of the cover point's level vertex [N-11].

### 5.2 `object_params`

A record of where an object was [N-11], received through the memory objects of spec X (**believed**: their
properties `self_info` and `object_info`, registered next to it). Its two properties are
read-only [N-11]: `level_vertex` (number: a level vertex) and `position` (a vector, read as a vector property of
E 2.4, believed a new copy). Assigning to either **depends on A**.

## 6. The client spawn manager (static)

The scoping spec's entry point `add` of the client spawn manager (its 4.5) is defined here.

### 6.1 The table

The level's spawn manager keeps, for each **awaited** object id, the entries of the objects that wait for it: one
entry per (awaited id, **requester** id), in increasing requester id [N-12, N-13]. An entry holds a script
function with an optional **bound value**, and may hold an engine-side handler instead or as well (the engine
itself makes entries in the same table, for objects that wait for another object; believed) [N-12, N-13]. Ids are
16-bit: only the low 16 bits of each id argument are used [N-12] (converting the numbers **depends on A**).

### 6.2 `csm:add(awaited, requester, f[, value])`

1. If the level already has an object with id `awaited` (2.3), `f` is called at once (6.4) for that object, and
   nothing is stored [N-12].
2. Otherwise the entry (`awaited`, `requester`) is made, or, when it exists, replaced (its function, bound value and
   engine-side handler are all replaced by the new ones, so a script entry also drops an engine handler of the same
   pair) [N-12].

The three-argument form stores no bound value [N-12]. `f` is a function; other values **depend on A**.

### 6.3 `csm:remove(awaited, requester)`

Removes the entry (`awaited`, `requester`); the awaited id is dropped when no entry is left for it [N-12]. When the
pair or the awaited id has no entry, the text `There is no spawn callback on object with id %d from object with id
%d!` is handed to the routine that returns at once (M 2.3), and nothing else happens [N-12].

### 6.4 When and how the engine calls an entry

When the level spawns an object on the client side, it creates the object and runs its spawn routine (**believed**
to include the binder's `net_spawn`, spec L); if that succeeds, then, before the object is attached to a parent, every entry
awaiting the object's id is called, in increasing requester id, and then all those entries and the awaited id are
dropped (each entry is called at most once) [N-13]. For each entry, first its engine-side handler (if any), then its
script function, as O 3.2 calls a function [N-12, N-13]:

- with a bound value: `f(value, id, handle)`;
- otherwise: `f(id, handle)`;

where `id` is the spawned object's id as a number and `handle` its handle (M 2.1), or nil if it has none [N-12].
Results are not read; the call is protected and an error is fatal as D 1.2 [N-12]. Step 1 of 6.2 calls the same way.
**Disputed in review:** not quite the same way: step 1 of 6.2 passes the handle made on request as M 2.1 describes,
while the calls of this section pass the object's handle only if it already exists and do not make one (so nil
when none has been made yet).

### 6.5 When entries are dropped without a call

- When the engine object of a creature (a character or monster of the engine's creature classes, not the player)
  is deleted, and when any object's spawn fails (`! Failed to spawn entity '%s'` is written to the log), every entry
  whose requester is that object's id is removed, from every awaited id, and awaited ids left without entries are
  dropped [N-13]. Entries of other requesters are not removed when the requester goes away [N-13]. (For each awaited id that has no entry from that requester, the text of 6.3 is
  handed to the routine that returns at once.)
- When the level removes all of its objects (O 6.5) the whole table is emptied, and when the level is destroyed the
  manager is deleted [N-13].

Entries whose awaited object never spawns stay until one of these happens.

### 6.6 Worked example (measured)

`scripts\test_client_spawner.script` (not loadable) calls `level.client_spawn_manager():add(y:id(),
get_actor():id(), spawn_callback)` with `function spawn_callback(id, obj)`; since `y` is an object it found in the
level, step 1 of 6.2 calls `spawn_callback(y's id, y's handle)` at once [N-13].

## 7. Names the scripts use that are not registered (static, measured)

- `level.check_object`, `level.debug_actor`, `level.debug_object` are listed but not registered, and their names
  occur nowhere in the game library [N-3]. `level.actor` and `level.object` (scoping spec 2.2: their names occur in the
  game library for other reasons) are not functions of `level` either [N-15]. Reading any of these from `level` gives
  nil, so calling it is a call of nil (**believed**: a run-time error of the runtime, handled by whatever called the
  script, for example fatal in a function the engine calls, D 1.2; `level` is believed a table holding only the 59
  functions of 1.2).
- Loadable scripts use `level.actor` in 7 scripts, `level.object` in 5, `level.debug_object` in 2 and
  `level.debug_actor` in 1 (section 9); in `_g.script`, `debug_get_actor` and `debug_get_level_object` call them in
  loops waiting for a non-nil result (measured).

## 8. Worked examples (measured; computed with a model of the rules above)

### 8.1 Vertex 0 of `l01_escape`

Its record is `ff ff ff 00 00 c0 05 00 e0 ff ff 0f 93 7d 59 31 0d 02 00 47 66` [N-5]: links none, 1, 23, none
(−x, +z, +x, −z); cover values *c*0 … *c*3 = 3, 9, 13, 7; packed column and row 525 (column 0, row 525); position
(−283.5, −10.615, −113.4) [N-5]. `level.cover_in_direction(0, d)` gives (rounded to 4 places) [N-7]:

| `d` (x, z) | Heading *a* | Result |
|---|---|---|
| (0, 1) | 0 | 0.6 (9/15) |
| (−1, 0) | π/2 | 0.2 (3/15) |
| (0, −1) | π | 0.4667 (7/15) |
| (1, 0) | −π/2 | 1.0 (2·9/15 − 3/15) |
| (1, 1) | −π/4 | 0.8 |
| (1, −1) | 5π/4 | 0.6667 |

### 8.2 A patrol path

`patrol("mil_actor_path")` (used by `dialogs_military.script`) has two points [N-16]: point 0 `wp00` at (−229.701,
−21.775, 197.692), flags 0, level vertex 101,671, game vertex 1,765; point 1 `wp01` at (−227.432, −21.775, 197.783),
level vertex 103,407, game vertex 1,765; one link, 0 → 1 (weight 1). So `count()` is 2, `index("wp01")` is 1,
`terminal(0)` false and `terminal(1)` true, `flags(0):get()` 0, `get_nearest(p:point(1))` 1, and `point(5)` gives
point 0's position [N-10, N-16]. In all paths, 4,540 points have flags other than 0 [N-16].

## 9. Use by the shipped scripts (measured)

Loadable scripts using each name (methods and properties by name on any object, so upper bounds; scoping spec section
1) [N-16]: the namespace `level` 70; `patrol` (the class) 35; `level.object_by_id` 14, `level.name` 11,
`level.vertex_position` 9, `level.patrol_path_exists` 5, `level.vertex_in_direction` 4, `level.present` 2,
`level.spawn_phantom` 1, `level.get_bounding_volume` 1; the constants `patrol.continue` 5, `patrol.next` 3,
`patrol.nearest` 2, `patrol.start` 1; `add` 7, `remove` 1. No script calls `cover_in_direction`, `physics_world` or
`client_spawn_manager` except the one non-loadable script of 6.6 [N-16]. All 30 names together: 137 loadable scripts
(upper bound) [N-16].

Literals (measured) [N-16]: loadable scripts pass 9 distinct texts directly to `patrol(...)`; 8 name paths of the
store; `way1` names none. They compare `level.name()` with `l01_escape`, `l02_garbage`, `l03_agroprom` and
`l03_agroprom_ai2`; the last is not a level name of `game.graph`, so that comparison is never true.

## 10. Verification facts

All reproduced by `run_all.py` (16 PASS lines, no failure):

- Scope: 41 listed items, 2 candidates, 30 used names [N-2].
- `level`: one namespace, 59 functions, 53 names, 3 listed names not registered; ownership 11 N, 14 U, 8 G4, 7 P, 5 H,
  4 T, 3 O, 1 Z [N-3].
- Level functions [N-4]; level vertex records and links [N-5]; the walk [N-6]; cover [N-7]; patrol store and
  existence [N-8]; the class `patrol` [N-9, N-10]; cover points and object parameters [N-11]; the spawn manager
  [N-12, N-13]; the physics world [N-14]; names not registered and candidates [N-15]; uses and examples [N-16].

## 11. Unestablished

- What `physics_world()` does at run time (2.5); when the phantom requested by `spawn_phantom` appears (2.6).
- What the start type, route type, random flag and index of a patrol object do (4.2); what a patrol point's flags
  mean.
- The tie-breaking of 3.3 in its edge and corner cases (believed as stated).
- Whether `level_vertex_id(i)` can fail for a point with an invalid game vertex (4.4).
- Which members of other specs return cover points and object parameter records, and when (5.1, 5.2).
- Which engine objects make entries in the spawn manager's table (6.1), and what happens when an entry is added or
  removed during the calls of 6.4.
- Everything that depends on A (argument conversion, other argument types, calls of missing names).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries identified above and the
  binding library `bin\xrLUA.dll`; the archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier
  approved specs (`scripts\*.script`, `config\creatures\m_phantom.ltx`, `spawns\all.spawn`, `game.graph`,
  `levels\l01_escape\level.ai` and `level.cform`).
- Analysis: static analysis with Ghidra (headless, read-only) of the game library's registration of the namespace
  `level` and of the four classes, of the implementations of their members, of the patrol path store and its two
  loaders, of the level vertex walk and the cover interpolation, of the client spawn manager and every routine of
  the game library that uses it, of the level's name and start, of the conversion of the physics world; of the
  engine's level loading, collision data header and object table; of the binding library's class constants; run-time
  type information and data values read from the file bytes by a throwaway reader; the scoping spec's script scan
  reused; throwaway readers of `level.ai`, `level.cform`, `game.graph` and the patrol paths of `all.spawn`; the
  listing and the scripts read as text. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- Whether an object spawned on the client side already has a handle when the entries of 6.4 are called (so
  whether scripts can receive nil there) is not established.
- `patrol:point(i)` on a path without points (the fallback of 4.4 finds no lowest-indexed point) is not stated
  (believed: a read through a missing point, as the other members).
