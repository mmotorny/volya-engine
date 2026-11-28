# Offline simulation and the game graph

Functional specification of the offline simulation of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the complete format of `game.graph`, the per-level cross tables (`levels\<level>\level.gct`),
the parts of `levels\<level>\level.ai` and of the patrol paths of `spawns\all.spawn` that the offline
simulation uses, where each object is located while offline, which level is current, when objects
switch online and offline, how offline characters and monsters travel over time, and in which order
objects are created and registered at a New Game and on load. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked
**believed**. Behaviour was read from the shipped program without running it (**static**); nothing here
was measured in the running game.

Builds on (not repeated here):

- `docs/formats/sav.md`: the save container and object records. Field, block and kind names used below
  ("Object block", "state flags", "direct control", "Creature block", "Monster-like block", "client
  data", "spawn flags", kinds such as "stalker" or "monster") are that specification's (sections
  7.2–7.4; kinds in 7.4.2), as are the load sequence (11.2) and the client-data rule (8). Two
  corrections:
  - sav.md 7.4.1 calls the Object block's `float` "distance along the current offline path edge" (and
    sav.md 1.2 "the offline path distance"). Its meaning is the cross-table distance of the object's
    level vertex (section 5.1); progress along a path edge is not saved (section 7.5).
  - sav.md 7.4.1 describes the four location fields of the Monster-like update as offline movement
    fields. Offline travel (section 7) neither reads nor writes them (section 5.4).
- `docs/behavior/start-position.md`: locating records in `spawns\all.spawn`; the world coordinate system.
- `docs/behavior/level-loading.md` (2.2, 5.11): the first fields of `game.graph`, given in full here.
- `docs/formats/chunks.md` (chunk sequences, lookup by id), `docs/formats/ltx-config.md`.

Conventions: little-endian; `u8`/`u16`/`u32` unsigned, `u24` an unsigned 3-byte integer; `float` IEEE 754
single; `float3` three floats (x, y, z); `stringZ` zero-terminated (Windows-1251); `guid` 16 raw bytes
compared as a whole. Sizes in bytes. Game time is in game milliseconds (sav.md 12). Positions are level
coordinates (start-position.md section 1) unless called global. "The actor" is the player's object
(section `actor`). Configuration keys without a section are read from the object's own configuration
section (the section named in its spawn packet).

**Checks.** The facts of section 6.5, the limits of the switch pass in 6.2 and the worked example of
section 9.1 carry bracketed ids: [SW-*n*] and [LO0-*n*] name the checks of the dirty workspace's
`checks/level-objects-split/` (`run_all.py`, which also runs `lo0.py`; decoders `sp.py`, `pres.py`).

## 1. `game.graph`

One file, `$game_data$` + `game.graph` (winning copy in `gamedata.dbb`, 8,128,486 bytes;
`gamedata.db0` and `gamedata.dba` hold an older, larger copy that loses). It is not chunked: a header,
the level table, then three tables, then 116 trailing bytes.

### 1.1 Header

| Offset | Type | Field | Value in this edition |
|---|---|---|---|
| 0 | `u8` | version; must be 8 ("Graph version mismatch!") | 8 |
| 1 | `u16` | vertex count *V* | 3,512 |
| 3 | `u32` | edge count *E* (all vertices together) | 8,630 |
| 7 | `u32` | level point count *P* (all vertices together) | 396,380 |
| 11 | `guid` | graph identifier | `13 8f 12 1e e1 b4 7f 4f 88 fc d4 dc 9e 7c a7 0c` |
| 27 | `u8` | level count *L* | 29 |
| 28 | | *L* level records (1.2) | |

### 1.2 Level records

Each, in order:

| Type | Field |
|---|---|
| `stringZ` | level name: the name of the level's directory under `$game_levels$` (e.g. `l01_escape`) |
| `float3` | level offset: added to a level point gives the global point (1.3) |
| `u8` | level id (unique) |
| `stringZ` | section name in `config\game_levels.ltx` (e.g. `level01`) |
| `guid` | level identifier; equals the identifier in that level's `level.ai` (section 3) |

The game looks records up by level id. In this edition they are stored in increasing level id. 18 of
them are levels that ship with level data; 11 are levels without level data (`stohe_selo`, `ai_test`,
`ai_test2`, `jim_dark`, `slipch_al_test`, `plecha_selo`, `ai_test_new`, `koan_test`, `l05_bar_hw`,
`l03_agroprom_hw`, `l03u_agr_underground_hw`), and no record of `spawns\all.spawn` is on their vertices.
For the 18 shipped levels, names (ignoring case: `l01_escape` in the graph, `L01_Escape` in the
configuration), ids and offsets agree with `config\game_levels.ltx` (keys `name`, `id`, `offset`).

### 1.3 Vertex table

It starts right after the last level record (offset 1,486 in this edition) and holds *V* records of 42
bytes; vertex *i* is record *i*. "Table start" below is the offset of vertex 0.

| Offset | Type | Field |
|---|---|---|
| 0 | `float3` | level point: the vertex's position in its level |
| 12 | `float3` | global point: level point + the level's offset (1.2), to within 0.0005 |
| 24 | `u8` | level id of the vertex's level |
| 25 | `u24` | level vertex: a vertex of that level's `level.ai` (section 3) whose cell contains the level point |
| 28 | 4 × `u8` | vertex types *t0 t1 t2 t3* (1.6) |
| 32 | `u32` | edges offset: from the table start to this vertex's first edge (1.4) |
| 36 | `u32` | level points offset: from the table start to this vertex's first level point (1.5) |
| 40 | `u8` | edge count |
| 41 | `u8` | level point count |

The vertices of one level are consecutive, levels in the order of the level table. The vertex of an
object or a position is called its **game vertex**; its level is the level whose record has the
vertex's level id. A vertex's **own level vertex** is the one in its record.

### 1.4 Edges

The edge table follows the vertex table (table start + 42·*V*) and holds *E* records of 6 bytes. Each
vertex's edges are consecutive, and the runs are in vertex order (the edges offset of vertex *i*+1 is
that of vertex *i* + 6 × its edge count).

| Type | Field |
|---|---|
| `u16` | target vertex |
| `float` | distance in metres |

Edges are directed. For an edge between two levels, the distance equals the straight-line distance
between the two global points (to within 0.0001). For an edge within a level, it is never shorter than
the horizontal (x, z) distance between the two level points and often longer, but it can be much
shorter than the three-dimensional distance (e.g. `l03_agroprom` 645 → 646: 0.0907 m between points
5.001 m apart). The two directions of a pair need not have equal distances; 6 edges have no reverse
edge; no edge leads from a vertex to itself.

### 1.5 Level points

The level point table follows the edges (table start + 42·*V* + 6·*E*) and holds *P* records of 20 bytes,
each vertex's run consecutive in vertex order, like the edges:

| Type | Field |
|---|---|
| `float3` | position: the centre of the level vertex below |
| `u32` | level vertex (of the vertex's level) |
| `float` | distance: equal to the cross-table distance of that level vertex (section 2) |

Every level point's level vertex is one whose cross-table cell names the owning game vertex. A vertex
has 1 to 255 level points, a subset of those level vertices (vertex 0: 25 of the 256 level vertices
whose cells name it). The rule that picks the subset is **unestablished**. The points are read-only
data; their use is outside this specification.

### 1.6 Vertex types

The four bytes classify the vertex. In this edition *t3* is always 0, *t2* is 0 or 1, *t1* is 0, 1 or 2
and *t0* is 0 … 17. `config\game_graphs.ltx` names the values of each byte in sections `[location_0]` …
`[location_3]` (e.g. `location_0`: `001` = Escape, `008` = Bar; `location_1`: `001` restricted
access, `002` forbidden access; `location_2`: `001` not for the player). That file is documentation
(believed: the program does not read it), and not every vertex follows it (e.g. every vertex of
`l10u_bunker` has *t0* = 0).

The offline simulation uses the types only in the path search (7.4); other parts of the program
(the online AI, believed) also read them, which is outside this specification.

A **terrain mask** is 4 bytes. A vertex
matches it if each of its four type bytes equals the mask's byte at the same position or that mask
byte is 255.

### 1.7 Trailing bytes

After the level points the file holds 116 bytes: 29 `u32` values, all 4. Their meaning is
**unestablished**.

### 1.8 Checks

The game requires, fatally otherwise: graph version 8 when the graph is read; and, when a level's
navigation data is loaded (6.1), the cross table's game identifier (section 2) equal to the graph identifier ("graph doesn't correspond
to the cross table"); the identifier in the current level's record equal to the level's `level.ai`
identifier ("graph doesn't correspond to the AI-map"). The graph identifier is also checked against the
spawn file (sav.md 6).

## 2. `levels\<level>\level.gct` (cross table)

Chunk sequence (chunks.md), two chunks:

| Chunk | Size | Content |
|---|---|---|
| 0 | 44 | `u32` version (must be 8: "Cross table version mismatch!"), `u32` level vertex count *N*, `u32` game vertex count (the level's number of game vertices), `guid` level identifier (must equal the `level.ai` identifier: "cross_table doesn't correspond to the AI-map"), `guid` game identifier (1.8) |
| 1 | 6·*N* | one cell per level vertex, in level-vertex order |

A missing file is fatal ("Can't open cross table!"), as is a missing chunk 0 or 1 ("Cross table is
corrupted!"). A cell is:

| Type | Field |
|---|---|
| `u16` | game vertex nearest to the level vertex (a vertex of the same level) |
| `float` | distance from the level vertex to that game vertex, in metres; a whole multiple of the cell size (0.7) |

Invariants in all 18 levels: *N* equals the level vertex count of `level.ai`; every cell names a game
vertex of the level and every game vertex of the level is named by some cell; the cell of each game
vertex's own level vertex names that vertex with distance 0.

## 3. `levels\<level>\level.ai` (only what the offline simulation uses)

Not chunked. Header (56 bytes):

| Offset | Type | Field |
|---|---|---|
| 0 | `u32` | version; must be 8 |
| 4 | `u32` | level vertex count *N* |
| 8 | `float` | cell size *c* (0.7 in all levels) |
| 12 | `float` | height range *h* |
| 16 | `float3` | box minimum (*x0*, *y0*, *z0*) |
| 28 | `float3` | box maximum (*x1*, *y1*, *z1*) |
| 40 | `guid` | level identifier |

Then *N* level vertices of 21 bytes (level vertex *k* at 56 + 21·*k*; the file ends there). Only the
position is used here: bytes 16–18 are a `u24` packed column and row *p*, bytes 19–20 a `u16` height
*q*. With the row length *r* = floor((*z1* − *z0*) / *c* + 1.501):

    x = x0 + c · floor(p / r),   z = z0 + c · (p mod r),   y = y0 + q / 65535 · h

The column count is *s* = floor((*x1* − *x0*) / *c* + 1.501); in all 18 levels every level vertex has
column floor(*p* / *r*) < *s* and row *p* mod *r* < *r*, and both maxima are reached. A level vertex's
**cell** is the square of side *c* centred on its x and z. The other 16 bytes (among
them links to neighbouring level vertices) are **unestablished** here.

## 4. Patrol paths (`spawns\all.spawn` chunk 3)

The travel targets of section 7 are points of named patrol paths stored in chunk 3 of the spawn file
(re-read from the spawn file on load, sav.md 10). Chunk 3 is a chunk sequence:

| Chunk | Content |
|---|---|
| 0 | `u32` path count (4,294) |
| 1 | one chunk per path, ids 0, 1, … in order |

Each path chunk holds chunk 0, the path name (`stringZ`, unique), and chunk 1, the path's points and
links, itself a chunk sequence:

| Chunk | Content |
|---|---|
| 0 | `u32` point count *n* |
| 1 | *n* chunks with ids 0 … *n*−1; point chunk *j* holds chunk 0 = `u32` *j* and chunk 1 = the point: `stringZ` point name, `float3` position, `u32` flags, `u32` level vertex, `u16` game vertex |
| 2 | links: repeated until the chunk ends: `u32` from point, `u32` count *m*, then *m* × (`u32` to point, `float` weight) |

**Point *j*** of a path is the point of point chunk *j*. In 9,778 of the 10,367 points the game vertex
is valid and the cross-table cell of the level vertex names it. The others have level vertex
0xFFFFFFFF (229 with a valid game vertex, 360 with the invalid game vertex 51,048). Point flags and
links are not used here.

## 5. Where an object is

### 5.1 Fields

Every object of the simulation carries, in its Object block (sav.md 7.4.1):

- **game vertex**: its vertex of `game.graph`; the level of that vertex is the object's level;
- **level vertex**: a level vertex of that level (section 3);
- **distance**: the cross-table distance of the level vertex (section 2), except after an arrival in
  travel (5.3);

and its **position** (spawn packet, sav.md 7.2) in that level's coordinates.

In all 8,641 records of `spawns\all.spawn` the cross-table cell of the record's level vertex (in the
level of its game vertex) holds exactly the record's game vertex and distance. A writer should keep
this invariant.

The position need not lie near the level vertex: objects that do not use the level graph (below) keep a
nominal location (e.g. the trader `escape_trader` and the helicopter `esc_heli` of `l01_escape` have game
vertex 0 and that vertex's own level vertex, far from their positions).

**Uses the level graph**, by kind and class id (sav.md 7.4.2; read for every class id listed there):

- always: player; stalker; monster (all its class ids, `SM_*` included); monster group (`AI_FLE_G`);
  smart zone (`SMRTZONE`, `SMRTTRRN`, `RE_SPAWN`);
- never: crow; phantom; vehicle; helicopter; the bolt (`II_BOLT`); zone; anomaly; visual anomaly;
  torrid zone; level changer; restrictor; team base; the searchlight (`O_SEARCH`); hanging lamp;
  physics object; breakable; climbable; skeleton object;
- when state flags bit 0x80 is set: trader; every item kind other than the bolt (item, torch, PDA,
  document, outfit, ammunition, knife, the weapon kinds); `W_MOUNTD`, `SCRPTOBJ`, `O_INVBOX`; stationary
  gun; online/offline group (`ON_OFF_G`).

A scripted class id (sav.md 7.4.2 †) has the value of the kind it belongs to. Scripts could replace it
(`used_ai_locations`); no shipped script does. The location update of the two group kinds differs from
5.2 and is not covered here (neither occurs in `spawns\all.spawn`).

### 5.2 Keeping the location current

The switch pass (6.2) brings the location of each object it visits up to date if the object has no
parent and uses the level graph. Nothing changes if any of these holds:

- the object is offline and its level vertex is not a valid vertex of the level (≥ *N*);
- its position is off the level's grid: its x or z lies more than *c*/2 outside the `level.ai` box, or
  its column floor((x − *x0*) / *c* + 0.5) is not below *s*, or its row floor((z − *z0*) / *c* + 0.5)
  is not below *r* (section 3), or column · *r* + row is not below 0x7FFFFF (never the case in the
  shipped levels);
- its position lies in the cell of its level vertex (x and z only).

Otherwise the game finds a level vertex *k* for the position: normally one whose cell contains the
position. Which one is taken when several are stacked at different heights, and which when none lies
there (the game then falls back to a level vertex near the old one), is **unestablished**. With *g* the
game vertex of *k*'s cross-table cell:

- **online object**: level vertex = *k*; game vertex = *g*; distance = the distance of *k*'s cell.
- **offline object, *g* equal to its game vertex**: level vertex = *k*; distance = the distance of
  *k*'s cell; game vertex and position unchanged.
- **offline object, *g* different**: the object arrives at *g* as in 5.3; then, if *k*'s cell contains
  the position the object had before, that position is restored; finally the distance is set from the
  cell of the level vertex now in place (*g*'s own level vertex), so it becomes 0. End state: game
  vertex *g*, level vertex *g*'s own level vertex, distance 0, position the old one if *k*'s cell
  contains it, else *g*'s level point.

### 5.3 Arrival at a vertex

When an offline object arrives at a game vertex (in travel, 7.5, or in 5.2), its game vertex becomes
that vertex, its position the vertex's level point and its level vertex the vertex's own level vertex.
The distance is not changed. The object leaves the current level's list (6.1) and is entered again if
the new vertex is on the current level. After an arrival in travel the stored distance can differ from
the cross-table distance of the level vertex (0), and 5.2 does not repair it (the level point lies in
the cell of the vertex's own level vertex), so the invariant of 5.1 then does not hold.

### 5.4 Monster-like update fields

The four location fields of the Monster-like update (sav.md 7.4.1) are neither read nor written by
offline travel (section 7), which uses only the Object block's game vertex and level vertex, the
position and the Monster-like block's smart-terrain id. In `spawns\all.spawn` they are 0xFFFF, 0xFFFF,
0, 0 in all 1,349 stalker and monster records.

## 6. Current level and switching

### 6.1 The current level

The current level is the level of the actor's game vertex. It is fixed when the actor's object (the
object with spawn flags bit 0x08 and direct control) is registered (sections 8.1, 8.2): the level record
with that level id gives the level name (a missing record is fatal: "Graph point level ID not found!"),
the engine selects the level of that name among its level directories (level-loading.md 2.2), and that
level's navigation data is loaded with the checks of 1.8. Level changes go through a save and a load
(sav.md 1.2), so the current level does not change while a simulation runs.

The **current level's list** holds every registered object that has direct control, is not an item held
by another object (an item whose parent id is not 0xFFFF), and whose game vertex is on the current
level, in increasing object id (believed: the list is kept sorted by id). Objects registered before the
actor are collected and entered when the current level is fixed; later ones on registration; travellers
as in 5.3. Objects not in the list are never evaluated for switching, so objects of other levels are
always offline.

All objects are offline when created at a New Game and when read from a save (the online state is not
saved).

### 6.2 The switch pass

The simulation is updated as often as the engine schedules it (`[alife]` `schedule_min = 1`,
`schedule_max = 1`, milliseconds of real time; believed: every frame). Each update first runs the
**switch pass**, then the scheduled updates (7.1).

The switch pass walks the current level's list in increasing id, starting after the object where the
previous pass stopped and wrapping around, and visits each object at most once per pass. It stops when
it comes back to an object already visited in this pass, or once it has run for

    process_time − update_monster_factor · process_time · 10⁻⁶   seconds of real time

[LO0-4b, LO0-5a] with `[alife]` `process_time = 900` and `update_monster_factor = 0.1` [LO0-1] (about 900 s:
899.99991 s) [LO0-2, LO0-3], so with the shipped values every pass visits every object of the list [LO0-3,
LO0-5a]. `process_time` is read as an integer number of seconds in this formula, although the comment beside
it in `config\alife.ltx` says microseconds [LO0-1, LO0-4a, LO0-4b, LO0-5a]. `objects_per_update` (`[alife]`:
10) [LO0-1] limits only the scheduled updates that follow the switch pass in the same update (7.1), not the
switch pass [LO0-5b, LO0-5c, LO0-6].

For each visited object, in this order:

1. If the object is removable (6.4), it is removed from the simulation, with the objects it carries, and
   the pass goes on with the next object.
2. If it has no parent and uses the level graph, its location is brought up to date (5.2).
3. If it has a parent, nothing more happens. If it is offline: a stalker or monster is entered in or
   taken out of the schedule as 7.1's condition now says; then the object switches online if the rule of
   6.5.3 says so (online distance 135 m); if it stays offline and its kind does not keep its client data
   (sav.md 8), its client data is discarded. If it is online: it switches offline if that rule says so
   (offline distance 165 m).
4. If the object is removable now, it is removed as in step 1.

The distance is the straight-line distance between the object's position and the actor's position.

### 6.3 What switching does

- **Switching online**: the object gets its in-game counterpart, built from its records (its stored
  client data, if non-empty, is applied: sav.md 8); objects with the Trader-like block bring the items
  they carry online with them. A stalker or monster leaves the schedule, and its travel path (7.4) is discarded.
- **Switching offline**: the in-game counterpart is destroyed; the object keeps its id. Its client data
  is then discarded unless its kind keeps it (sav.md 8). A stalker or monster re-enters the schedule if
  7.1's condition holds, and its travel path is discarded.

### 6.4 Removal of dead bodies

A stalker or monster is removable when its health (Creature block) is 0 or less, it is offline, its story
id is −1, its death time is not 0, and the game time has reached death time +
`stay_after_death_time_interval` hours (`[monsters_common]`, `config\creatures\monsters.ltx`: 36).
Objects of other kinds are never removable this way. Removed objects are not saved. As removal happens
only in the switch pass, bodies on other levels stay until their level is current.

### 6.5 The switch rule

#### 6.5.1 Switch distances

From section `[alife]` of `config\alife.ltx` (dbb) [SW-1]:

| Key | Value | Use |
|---|---|---|
| `switch_distance` | 150 | *d* |
| `switch_factor` | 0.1 | *f* |

The **online distance** is *d* · (1 − *f*) = **135 m**, the **offline distance** *d* · (1 + *f*) = **165 m**
[SW-1]. The distance is the Euclidean distance in three dimensions between the object's position and the
actor's position (at the start: the positions stored in the spawn packets, start-position.md section 3.1).
The scripts can change the switch distance at run time (`alife():switch_distance`); none does before the
first frame of a New Game (believed: no call is reachable from the start-game callbacks).

#### 6.5.2 Switch capability of an object

Two predicates, **can switch online** (*on*) and **can switch offline** (*off*), are evaluated per object.
With *F* the object's state flags (`u32`, sav.md 7.4.1) and *m* the object's **renderer match** (below):

- *on* = *m* and (*F* & 0x02 ≠ 0), for every kind;
- *off* = (not *m*) or (*F* & 0x04 ≠ 0), except:
  - for the kinds zone, anomaly, visual anomaly, torrid zone, level changer, restrictor, smart zone, team
    base, breakable and climbable (sav.md 7.4.2): *off* = false;
  - for creatures (player, stalker, monster, crow, phantom, monster group): *off* = ((not *m*) or
    (*F* & 0x04 ≠ 0)) and health > 0 (health from the Creature block). Dead creatures never switch offline;
  - for ammunition (`AMMO`, `A_*`): *off* = ((not *m*) or (*F* & 0x04 ≠ 0)) and rounds left ≠ 0 (the
    state's `u16` rounds left; 10 or 20 in every ammunition record of `l01_escape`) [SW-7].

The **renderer match** *m* is true for every kind except the hanging lamp (`O_HLAMP`), for which it is true
only if the lamp's light flags (hanging-lamp block, `u16`) have bit 0x04 set and the first renderer is in use
(bit 0x08 plays the same role for the second renderer). A lamp with neither bit set is a fatal error ("no
renderer type set for hanging-lamp") [SW-8]; every shipped lamp record has one of them [SW-4]. So a lamp
marked only for the second renderer never comes online with the first renderer.

Scripted kinds (class ids marked † in sav.md 7.4.2) can replace both predicates from their scripts; 6.5.4
lists the replacements that exist in the shipped scripts.

#### 6.5.3 The rule

For each object of the current level's list (6.1), whenever the switch pass (6.2) visits it:

- **offline** object: if *on* is false, it stays offline. Otherwise, if *off* is false it switches online
  regardless of distance; otherwise it switches online when its distance to the actor is ≤ 135 m.
- **online** object: if *off* is false, it stays online. Otherwise, if *on* is false it switches offline;
  otherwise it switches offline when its distance to the actor is > 165 m.

All objects start offline at a New Game (6.1). The level's objects are believed to have all been evaluated
once during the 30 frames in which the loading image is still shown (level-loading.md 2.3), so the first
visible frame shows the result of one evaluation at the start positions: an object is present if *on* and
(not *off* or distance ≤ 135 m).

#### 6.5.4 Scripted replacements (shipped scripts)

`scripts\class_registrator.script` binds scripted class ids to classes in `scripts\se_*.script`. The
following replace *on* and/or *off* (static reading of the scripts) [SW-9, SW-9b]. Distances computed by
these scripts are measured from the position of the object's level-graph node (`level.vertex_position` of
the Object block's level-graph node) to the actor's current position, not from the record's position.

| Class ids | Script | Replacement |
|---|---|---|
| `AI_STL_S` | `se_stalker.script` | *off*: if the object's job-online value is set (true, false or a condition list), *off* = not that value; else the engine's *off*. *on*: false if the object's `dont_spawn_online` field is true (set by no shipped script); else the job-online value if set; else, if the record's custom data has no `[spawner]` section, the engine's *on*; else (key `cond` of `[spawner]`): if the actor exists and is dead, the current online state; while offline, (the condition list holds) and the engine's *on*; while online, the condition list holds |
| `SM_*` | `se_monster.script` | *off* as for `AI_STL_S`. *on*: the job-online value if set; else, without a `[spawner]` section, the engine's *on*; else, while offline: false if the actor does not exist or is dead; false if the spawner sets `check_distance = true` and the distance is < *r*; otherwise (the `[spawner]` condition holds) and the engine's *on*. While online: true if the actor does not exist or is dead, or if the condition holds; false if `check_distance` is set and the distance is > *r*; else the engine's *on*. *r* is the spawner's `min_distance` if given, else 150. No monster record of `l01_escape` sets `check_distance` |
| `SCRPTART` | `se_artefact.script` | for sections `af_electra_flash`, `af_electra_moonlight`, `af_electra_sparkler` only (others: engine predicates): *on* = the current online state if the actor does not exist or is dead; true if the artefact has a parent; for an offline artefact: true only if the game hour is 0 … 4 (`utils.electro_art_enabled`) and the distance is at least 50 m; for an online one: true if the hour is 0 … 4 or the distance is < 50 m |
| `SCRPTCAR` | `se_car.script` | *on*: the engine's *on* without a `[spawner]` section, else the `[spawner]` condition |

Smart terrains (`SMRTTRRN`) set a character's job-online value when the character **begins** a job
(`scripts\xr_gulag.script`: an offline character begins when it is at its job's graph point); jobs of the
first level's smart terrains are defined with `online = true` (e.g. `scripts\gulag_escape.script`) [SW-11].
Whether any character begins such a job before the first frame, which would bring it online regardless of
distance, is **unestablished**; the counts of section 9.1 assume none does.

## 7. Offline travel

Offline travel applies to stalkers and monsters (the kinds with the Monster-like block; monster groups
do not occur in `spawns\all.spawn`).

### 7.1 Scheduling

A stalker or monster belongs in the **schedule** while it has direct control, is offline, and its health
is above 0.001. This condition is applied when the object is registered (8.1, 8.2), when it switches
(6.3) and in the switch pass (6.2).

Each simulation update gives a scheduled update to at most `objects_per_update` (`[alife]`: 10)
scheduled objects, in id order (believed: the schedule is kept sorted by id), starting after the last
one served and wrapping around, each at most once per update.

A scheduled update of a stalker or monster does anything only if its state flags have all of bits 0x08,
0x10 and 0x20 set and its health is above 0.001. It then does, in this order:

1. the smart-terrain choice (7.2);
2. if the object still has no smart terrain (smart-terrain id 0xFFFF): nothing more; in particular, its
   travel clock (7.5) is not touched;
3. otherwise: it takes its target from its smart terrain (7.3), then makes a movement update (7.5).

The scheduled updates of other kinds are not covered here.

### 7.2 Choosing a smart terrain

An object whose smart-terrain id is 0xFFFF looks for a smart terrain if a per-object setting allows it
and at least `smart_terrain_choose_interval` of game time (hours:minutes:seconds; `00:15:00` for every
stalker and monster of `spawns\all.spawn`) has passed since its last look. The setting is on when the
object is created; scripts can turn it off (believed: through `can_choose_alife_tasks`, which no shipped
script uses). The time of the last look is 0 when the object is created or loaded, so its first
scheduled update looks.

A look records the current game time as the time of the last look, then asks every smart terrain of the
simulation, in increasing id (believed: their registry is keyed by object id), whether it accepts the object and,
if it does, how suitable it is; the most suitable one is taken (of equals, the first). If one is taken,
the object joins it and the time of the last look is reset to 0. Smart terrains are the scripted kind
`SMRTTRRN`; acceptance, suitability and joining are defined by `scripts\smart_terrain.script`
(`enabled`, `suitable`, `register_npc`) and the gulag scripts it uses, which are not covered here.

### 7.3 The travel target

At each such scheduled update the smart terrain gives the object a task (`se_smart_terrain:task` in
`scripts\smart_terrain.script`) naming a patrol path (section 4). The target is point 0 of that path:
its game vertex, level vertex and position. What happens when the task names a path that does not
exist, or one whose point 0 has an invalid game vertex (242 paths of `spawns\all.spawn`, section 9),
is **unestablished**.

### 7.4 The path search

**Path.** A movement update (7.5) first compares the target's game vertex and level vertex with the
object's. If both are equal, nothing happens. Otherwise, if the object has no path, or its path does
not lead to the target's game vertex, a new path is searched from the object's game vertex to the
target's game vertex. If none is found, the object stays where it is, without a path, and searches again
at its next movement update.

**Search.** The search is A* over the directed edges of `game.graph`:

- the cost *g* of a vertex is the sum of the edge distances from the start; the estimate *h* is the
  straight-line distance between the vertex's global point and the target's global point; the priority
  is *f* = *g* + *h*; all of these are single-precision floats;
- the start vertex is opened with *g* = 0;
- repeatedly, the open vertex with the smallest *f* is taken. If it is the target, the search ends; the
  path is the chain of predecessors from the start to the target. Otherwise the vertex is closed and its
  edges are examined in stored order. An edge to a vertex that is not admitted (below) is skipped; an
  edge to a closed vertex is skipped (closed vertices are never reopened); an edge to an open vertex
  replaces that vertex's *g*, *f* and predecessor if the new *g* is strictly smaller; an edge to a vertex
  not yet seen opens it;
- among open vertices of equal *f*, the one most recently opened or improved is taken first;
- the search fails when no open vertex is left.

The estimate is not a lower bound (an edge within a level can be shorter than the straight-line
distance, 1.4), so the path found is not always a shortest one (section 9).

**Admission.** A vertex is admitted if it is **accessible** and, when the masks apply to this search,
it matches at least one of the object's terrain masks (1.6). The start vertex is not tested for
admission, but it decides whether the masks apply:

- if the start vertex is not accessible, the masks do not apply (only accessibility counts);
- if it is accessible and matches one of the masks, the masks apply;
- if it is accessible but matches none of the masks, whether the masks apply is **unestablished**. The
  program reads, for this test, its record of whether masks apply before setting it for the new search:
  if that record says no, the start vertex passes and the masks apply; if it says yes, they do not
  apply. The record lives in working memory that is reused between searches, so it often still holds
  the value of an earlier search, but nothing guarantees which. 11 monster records of
  `spawns\all.spawn` start on such a vertex (section 9).

**Accessibility** is a per-vertex flag of the game graph. Every vertex is accessible when the graph is
loaded. The flags are not saved; whether they are reset when another game is started or loaded in the
same run is **unestablished**. Scripts change them through `game_graph():accessible(vertex, flag)`. The
only shipped use is `scripts\bind_physic_object.script`: a physics object whose `[logic]` section has
`graph_point_marker = <path>` makes the game vertex of point 0 of that patrol path inaccessible at its
first update after coming online (once the actor exists), and accessible again in its death callback,
when it is destroyed. One record of `spawns\all.spawn` has it (section 9).

**Terrain masks** come from the configuration key `terrain`:

- if its value names a configuration section that has at least one line, every line of that section
  whose key (the text before any `=`) is exactly four comma-separated numbers is a mask (e.g.
  `[flesh_terrain]` `255,000,255,255`); if there is no such line, the single mask 255,255,255,255;
- otherwise the value is a list of numbers taken four at a time, a final group of fewer than four
  ignored (e.g. `255,255,255,255,0,1` is the single mask 255,255,255,255); if no group remains, the
  single mask 255,255,255,255.

Each number is taken modulo 256. A key `terrain` in section `[alife]` of the object's custom data
replaces the configuration's; a section it names is then looked up in the custom data. No record of
`spawns\all.spawn` has an `[alife]` section in its custom data.

### 7.5 Movement and timing

Each stalker and monster has a **travel state**, which is not saved: its path, its progress along the
path's current edge (metres), its target and its travel clock (a game time). When the object is created
(New Game or load), the path is empty, the progress and the clock are 0, and the target is the object's
own location.

A **movement update**, with *now* the game time:

1. If the clock is not earlier than *now*, nothing happens. Otherwise, with Δ = *now* − clock, the object
   moves by Δ (steps 2–4) unless the clock is 0, and in either case the clock then becomes *now*. So
   movement is measured from the object's previous movement update, and a clock of 0 only starts the
   clock. An object without a smart terrain makes no movement updates (7.1); when it later joins one,
   its first movement covers the whole time since its last movement update, or nothing if it has never
   had one.
2. The target test and path search of 7.4. A new path of more than one vertex restarts the progress at 0.
3. If the path holds only the target's game vertex (the object is at it), the object takes the target's
   level vertex and position, the progress becomes 0, and the update ends.
4. Otherwise the object covers, along its path,

       t / normal_time_factor · speed   metres,   with t = Δ · 0.001 game seconds,

   where `normal_time_factor` is the second factor of the game-time chunk (sav.md 5; 10 at a New Game)
   and `speed` is `current_level_going_speed` if the object's game vertex is on the current level,
   `going_speed` otherwise (`current_level_going_speed` defaults to `going_speed`; e.g. section
   `stalker`: `going_speed = 2`, `current_level_going_speed = 1`). At the default speed of game time the
   object thus moves `speed` metres per real second.

   The edge is the first stored edge from the object's game vertex to the next vertex of the path. If
   progress + covered is less than the edge's distance, the progress grows by the covered distance and
   the update ends. Otherwise the object arrives at the next vertex (5.3) and the progress becomes 0; if
   the path has a further vertex, the time left over, (progress + covered − edge distance) ·
   `normal_time_factor` / speed, is used again in step 4, with the speed that applies at the new vertex.

Arriving at the target's game vertex on the last edge does not yet take the target's level vertex and
position; that happens at the next movement update (step 3). Between vertices the object's position
stays at the level point of the last vertex reached. After a load, or after switching online or
offline, the path is empty; an object already at the target's game vertex then takes the target's level
vertex and position at its next movement update.

## 8. Creation and registration order

### 8.1 New Game

1. The spawn file is read (sav.md 6). Its records form a spawn graph whose edges are stored in chunk 1,
   sub-chunk 2 (empty in this edition), so every record is spawned exactly once.
2. Object id 0 is reserved for the actor.
3. The records are created in increasing object index (spawn id). For each record:
   1. the object is created from the record's packets;
   2. it gets its id: 0 for the actor, otherwise the next id upwards from 1;
   3. it is registered: entered in the object registry under its id; entered in the current level's
      list or collected for it (6.1), the actor's registration fixing the current level; scheduled if
      7.1's condition holds; entered in the story-id and smart-terrain registries;
   4. its supplies (the items it carries, defined by its configuration and custom data; not covered here)
      are created right after it, as objects of their own, in the same way, with it as parent. An object of
      a group kind (`ON_OFF_G`, `AI_FLE_G`) creates its members instead (none occurs in
      `spawns\all.spawn`).
4. Then every object, in increasing id, runs its post-registration step (sav.md 11.2 step 7).
5. The game is saved under the game name (sav.md 1.1).

So ids follow creation order, and the actor (index 63) has id 0. Supplies involve random draws (the
generator is **unestablished**), so the ids after the first object with supplies cannot be predicted
from the data.

### 8.2 Load

As sav.md 11.2: objects are read with their saved ids; the first pass registers them in increasing id
(the actor's registration fixing the current level, 6.1); then the registries; then the
post-registration step in increasing id. Nothing is created from spawn records. Travel states start as in
7.5.

## 9. Verification facts

`game.graph` (`gamedata.dbb`): 8,128,486 bytes, CRC-32 `f5e1987b`, SHA-256
`a1ddb5f7b25f3f0f859883b0693ecdcc56bb269853c4181e91141eb8260fd76d`. Header and tables as in 1.1; the
vertex table starts at 1,486, the edges at 148,990, the level points at 200,770, the trailing bytes at
8,128,370. Sum of all edge distances 275,640.907; sum of all level point distances 7,731,072.870; sum
of the level vertices of all vertices 474,202,540.

Levels (level id, offset; vertices first … last; edges; level points). The section name of every record
is `level` + its id with at least two digits (`level01`, …, `level169`).

| Level | Id | Offset | Vertices | Edges | Points |
|---|---|---|---|---|---|
| `l01_escape` | 1 | 3000, 1000, 0 | 0 … 251 | 652 | 40,733 |
| `l02_garbage` | 2 | 3000, 1000, 1000 | 252 … 415 | 428 | 33,149 |
| `l03_agroprom` | 3 | 2000, 1000, 1000 | 416 … 702 | 778 | 42,019 |
| `l03u_agr_underground` | 4 | 2000, 800, 1000 | 703 … 810 | 225 | 585 |
| `l04_darkvalley` | 5 | 4000, 1000, 1000 | 811 … 1108 | 728 | 33,645 |
| `l04u_labx18` | 6 | 4000, 800, 1000 | 1109 … 1167 | 123 | 745 |
| `l05_bar` | 7 | 3000, 1000, 2000 | 1168 … 1307 | 320 | 9,131 |
| `l06_rostok` | 8 | 2000, 1000, 2000 | 1308 … 1437 | 301 | 6,711 |
| `l08_yantar` | 9 | 1000, 1000, 2000 | 1438 … 1528 | 220 | 12,306 |
| `l08u_brainlab` | 10 | 1000, 800, 2000 | 1529 … 1544 | 31 | 795 |
| `l07_military` | 11 | 3000, 1000, 3000 | 1545 … 1861 | 896 | 41,473 |
| `l10_radar` | 13 | 3000, 1000, 4000 | 1862 … 2116 | 609 | 22,459 |
| `l11_pripyat` | 14 | 3000, 1000, 5000 | 2117 … 2272 | 398 | 18,779 |
| `l12_stancia` | 15 | 2000, 1000, 8000 | 2273 … 2401 | 319 | 30,664 |
| `l12u_sarcofag` | 16 | 2000, 1200, 8000 | 2402 … 2466 | 136 | 785 |
| `l12u_control_monolith` | 17 | 2000, 1400, 8000 | 2467 … 2516 | 104 | 382 |
| `l12_stancia_2` | 19 | 2000, 1000, 9000 | 2517 … 2660 | 321 | 22,989 |
| `l10u_bunker` | 20 | 3000, 800, 4000 | 2661 … 2791 | 273 | 878 |
| 11 levels without data (ids 102 … 169) | | | 2792 … 3511 | 1,768 | 78,152 |

The level identifier of `l01_escape` is `23 ce 43 3a 31 b3 13 4c a0 6e 2c af 5b 66 84 52`.

43 edges join different levels, among them `l01_escape` 0 → `l02_garbage` 415 (1,206.288) and 1 → 252
(21.196), and 252 → 1 (21.196) back.

Vertex 0 (`l01_escape`): level point (368.648, 14.986, −39.828), global point (3368.648, 1014.986,
−39.828), level vertex 594,270, types 1 0 0 0, 2 edges (→ 140, 28.516; → 415, 1,206.288), 25 level
points; 256 cross-table cells name it. Vertex 4: level point (−247.103, −24.788, −134.425), level vertex
11,366, types 1 0 0 0, one edge (→ 7, 3.405), one level point. 1,543 vertices are named by more than 255
cells but have fewer than 255 level points (e.g. vertex 2: 156 points, 1,564 cells).

Cross tables (`gamedata.dbb`) and `level.ai` (`gamedata.db0`–`db3`, the only copies), *N* and sizes:

| Level | *N* | `level.gct` bytes | `level.ai` bytes | row length *r* |
|---|---|---|---|---|
| `l01_escape` | 595,499 | 3,573,054 | 12,505,535 | 1,697 |
| `l02_garbage` | 382,663 | 2,296,038 | 8,035,979 | 903 |
| `l03_agroprom` | 437,421 | 2,624,586 | 9,185,897 | 790 |
| `l03u_agr_underground` | 4,932 | 29,652 | 103,628 | 272 |
| `l04_darkvalley` | 390,125 | 2,340,810 | 8,192,681 | 1,095 |
| `l04u_labx18` | 7,581 | 45,546 | 159,257 | 166 |
| `l05_bar` | 99,539 | 597,294 | 2,090,375 | 1,285 |
| `l06_rostok` | 67,713 | 406,338 | 1,422,029 | 823 |
| `l07_military` | 418,268 | 2,509,668 | 8,783,684 | 770 |
| `l08_yantar` | 141,471 | 848,886 | 2,970,947 | 643 |
| `l08u_brainlab` | 8,008 | 48,108 | 168,224 | 81 |
| `l10_radar` | 227,189 | 1,363,194 | 4,771,025 | 890 |
| `l10u_bunker` | 8,821 | 52,986 | 185,297 | 213 |
| `l11_pripyat` | 261,219 | 1,567,374 | 5,485,655 | 1,141 |
| `l12_stancia` | 477,923 | 2,867,598 | 10,036,439 | 630 |
| `l12_stancia_2` | 264,577 | 1,587,522 | 5,556,173 | 551 |
| `l12u_control_monolith` | 3,982 | 23,952 | 83,678 | 112 |
| `l12u_sarcofag` | 10,672 | 64,092 | 224,168 | 114 |

`l01_escape`: `level.gct` CRC-32 `9c9940ab` (the losing `gamedata.db0` copy is `6ae470f4`), sum of all
cell distances 14,024,781.2; `level.ai` CRC-32 `db52f6d7`, box (−283.5, −37.482147, −480.9) …
(411.59998, 29.764366, 706.3), height range 67.247513, column count *s* 994.

Invariants (all 18 levels): section 2's cross-table invariants; level points match their cross-table
cells (1.5) and lie at the decoded position of their level vertex (section 3) to within 0.0001 in x and
z and 0.00001 in y; each vertex's level point lies within half a cell (0.35) of its own level vertex's
decoded x, z; the three identifiers agree (1.8).

Patrol paths (`spawns\all.spawn` chunk 3, 1,070,283 bytes): 4,294 paths, 10,367 points, 7,267 links
(7,257 of weight 1.0). 242 paths have a point 0 with level vertex 0xFFFFFFFF and game vertex
51,048.

Objects (`spawns\all.spawn`): for all 8,641 records the cross-table invariant of 5.1 holds, and none is
on a level without data; 870 records are on `l01_escape`. Records of `l01_escape`:

| Record | Name | Class id | Game vertex | Level vertex | Distance | Level vertex position |
|---|---|---|---|---|---|---|
| 63 | `level_prefix_actor_0001` | `O_ACTOR` | 4 | 11,713 | 0.7 | (−246.4, −24.790, −134.4) |
| 84 | `escape_trader` | `AI_TRD_S` | 0 | 594,270 | 0 | (368.9, 15.022, −39.9) |
| 100 | `esc_wolf` | `AI_STL_S` | 61 | 42,599 | 3.5 | (−210.7, −20.059, −142.8) |
| 107 | `esc_heli` | `C_HLCP_S` | 0 | 594,270 | 0 | (368.9, 15.022, −39.9) |
| 137 | `esc_fox` | `AI_STL_S` | 236 | 438,370 | 8.4 | (133.7, −0.413, 315.0) |
| 164 | `esc_bridge_soldier1` | `AI_STL_S` | 98 | 339,597 | 16.1 | (53.9, 1.644, 176.4) |
| 285 | `exit_to_garbage_01` | `LVL_CHNG` | 1 | 309,788 | 7.0 | (29.4, 16.571, 686.0) |
| 445 | `esc_shustryi` | `AI_STL_S` | 118 | 422,012 | 8.4 | (119.0, −7.488, 7.7) |

(Record 3449 of `l05_bar` has the same name `exit_to_garbage_01`.)

Stalkers and monsters: 1,349 records (1,073 stalker, 276 monster), all with `smart_terrain_choose_interval
= 00:15:00` and `current_level_going_speed = 1`; the 717 records of section `stalker` have `going_speed
= 2` and `terrain = 255,255,255,255,0,1`. Terrain masks: all 1,073 stalkers have the single mask
255,255,255,255; most monsters 255,0,255,255 (e.g. section `dogs_terrain`, 90 records); `snork_terrain`
(68 records) gives the two masks 255,0,255,255 and 10,1,255,255. 11 monster records start on a game vertex
that matches none of their masks, e.g. record 3362 `bar_m_dog_e_0009`: vertex 1264 (`l05_bar`), types 8
1 0 0, mask 255,0,255,255. The only record with `graph_point_marker` is 5197 `mil_zabor` (section
`physic_destroyable_object`, `l07_military`): path `mil_graph_zabor_changer_1`, whose point 0 has game
vertex 1625.

Path search (7.4) with all vertices accessible and the mask 255,255,255,255: from 225 to 36
(`l01_escape`) it finds the 28 vertices 225, 227, 195, 192, 190, 191, 185, 181, 182, 179, 177, 171, 164,
163, 84, 81, 79, 73, 75, 72, 44, 107, 106, 104, 102, 100, 34, 36, of length 1,108.307, while the shortest
path is 1,108.176 long. Over all 63,504 ordered pairs of `l01_escape` vertices (all connected), 192 found
paths are longer than the shortest, the most for 232 → 189 (163.872 against 163.130).

Configuration (`config\alife.ltx`, `[alife]`): `switch_distance = 150`, `switch_factor = 0.1`,
`process_time = 900`, `update_monster_factor = 0.1`, `objects_per_update = 10`, `schedule_min = 1`,
`schedule_max = 1`, `normal_time_factor = 10`; `config\creatures\monsters.ltx` `[monsters_common]`
`stay_after_death_time_interval = 36`.

### 9.1 Worked example: online objects at the start of a New Game on `l01_escape`

Every record of the shipped `spawns\all.spawn` has an Object block; 870 of the 8,641 records are on
`l01_escape`, all with parent id 0xFFFF (no container) [SW-2]. Applying 6.5 to them gives 671 online
objects [SW-3]; distances are from the actor's stored position.

**State at the start** (believed, from the scripts and the records): the job-online value of every stalker
and monster record of the level is "not set" (the job-online byte, sav.md 7.5, is 2 in all of them). Every
`[spawner]` section of the level's records (10 stalkers, 13 monsters) has a `cond` that requires an
information portion (`{+escape_stalker_help}`, `{+esc_fanat_spawn}`, `{+escape_lager_spawn_killers}`,
`{+tutorial_wounded_start}`, `{+escape_stalker_dog_spawn}`, `{+esc_boar_dogs_restrictor}`,
`{+esc_dog_spawn}`); the actor has none of them at the start (they are given later by dialogs and scripts),
so all 23 are absent [SW-5]. The start time is 05:30 (level-loading.md 2.6), so the three electra artefacts
of the level are absent [SW-10].

| Class id | Records | Online |
|---|---|---|
| `AI_CROW` | 55 | 55 |
| `AI_STL_S` | 63 | 32 |
| `AI_TRD_S` | 1 | 1 |
| `AMMO` | 9 | 1 |
| `ARTEFACT` | 3 | 1 |
| `C_HLCP_S` | 1 | 1 |
| `E_STLK` | 2 | 0 |
| `II_ATTCH` | 1 | 0 |
| `II_BANDG` | 4 | 0 |
| `II_BOTTL` | 17 | 3 |
| `II_EXPLO` | 6 | 0 |
| `II_FOOD` | 15 | 2 |
| `II_MEDKI` | 2 | 0 |
| `LVL_CHNG` | 1 | 1 |
| `O_ACTOR` | 1 | 1 |
| `O_BRKBL` | 49 | 49 |
| `O_CLMBL` | 14 | 14 |
| `O_HLAMP` | 27 | 5 |
| `O_INVBOX` | 25 | 5 |
| `O_PHYS_S` | 69 | 69 |
| `P_DSTRBL` | 127 | 127 |
| `RE_SPAWN` | 9 | 9 |
| `SCRPTART` | 3 | 0 |
| `SMRTTRRN` | 41 | 41 |
| `SM_BOARW` | 10 | 0 |
| `SM_DOG_S` | 48 | 0 |
| `SM_FLESH` | 8 | 1 |
| `SM_P_DOG` | 2 | 0 |
| `SPC_RS_S` | 75 | 75 |
| `WP_BM16`, `WP_LR300`, `WP_PM`, `WP_WALTH` | 1 each | 0 |
| `ZS_MBALD` | 96 | 96 |
| `ZS_MINCE` | 4 | 4 |
| `Z_MBALD` | 60 | 60 |
| `Z_RADIO` | 18 | 18 |

[SW-3] Of the kinds with a model (`object-visual-and-transform.md`): 69 `O_PHYS_S` and 127 `P_DSTRBL`
physics objects, 49 breakables, 5 hanging lamps, 5 inventory boxes, 7 items (1 `AMMO`, 1 `ARTEFACT`,
3 `II_BOTTL`, 2 `II_FOOD`), 0 weapons, 0 outfits, 32 stalkers, 1 trader, 1 monster, 55 crows, 1 helicopter.

Characteristic cases: all physics objects, breakables, crows, the helicopter and the 5 online lamps are
online at any distance because their state flags lack bit 0x04 or their kind never switches offline; the 5
inventory boxes and 7 items are online by distance (≤ 135 m); the 16 soldiers of the checkpoint and the
special-forces group (state flags 0xFFFFFFBB) are online at any distance; the 8 dead stalker bodies (health
0) are online at any distance; the 8 camp stalkers within 135 m are online by distance; of the 27 hanging
lamps only the 5 with light-flags bit 0x04 come online [SW-4, SW-6].

The 32 online stalkers: `esc_lager1` … `esc_lager7`, `esc_wolf` (distance 25 … 78 m, state flags
0xFFFFFFBF), `esc_specnaz1` … `6`, `esc_blokpost_soldier1` … `9`, `esc_blokpost_commander` (232 … 267 m,
state flags 0xFFFFFFBB), and the bodies `esc_stalker_corpse`, `esc_tutorial_dead_novice`,
`esc_stalker_corps1`, `esc_stalker_corps1_0000`, `esc_stalker_corps1_0001`, `esc_trup_bratka`,
`esc_stalker_0000`, `esc_trup_scene` (health 0, 166 … 764 m). The online monster is `esc_suicidal_flesh`
(state flags 0xFFFFFFF3, 70 m) [SW-6]. Absent because of a `[spawner]` section: 10 stalkers (`esc_fox`,
`esc_stalker_fanat`, `esc_stalker`, `esc_killer1` … `esc_killer8` without `esc_killer5`) and 13 monsters
[SW-5, SW-5b].

## 10. Unestablished

- The trailing 116 bytes of `game.graph` (1.7); the choice of a vertex's level points (1.5); the 16
  non-position bytes of a `level.ai` vertex (3); the choice of the level vertex *k* among stacked
  vertices and the fallback when none lies at the position (5.2).
- Whether terrain masks apply to a search whose start vertex is accessible but matches none of them
  (7.4); whether accessibility flags are reset between games in one run (7.4); the order among *f*
  values that differ only by rounding.
- Smart-terrain acceptance, suitability, joining and job logic (scripts, 7.2); the supply rules and the
  random generator behind them, hence most New Game ids (8.1).
- The scheduled updates of kinds other than stalkers and monsters (7.1).
- Whether smart-terrain jobs force characters online before the first frame (6.5.4), and the exact run-time
  script state at the first switch evaluation (believed as stated in 9.1).
- Everything here is static: none of it was observed in the running game.

## 11. Provenance

- Date: 2026-11-24 (fourth revision the same day: level-graph use read for every class id, the use of
  vertex types restricted to the offline simulation, the off-grid rule completed, the start-vertex mask
  case narrowed, unusable task paths; earlier the same day: the path search as A*, accessibility, the
  travel clock, level-point counts, the sav.md corrections).
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binary `bin\xrGame.dll`.
- Data examined: `game.graph` (dbb), `spawns\all.spawn` (dbb), `levels\<level>\level.gct` (dbb) and
  `levels\<level>\level.ai` (db0–db3) of all 18 levels, `config\alife.ltx`, `config\game_graphs.ltx`,
  `config\game_levels.ltx`, `config\creatures\*.ltx`, and all shipped scripts (searched for uses of the
  simulation's and the game graph's script interface; `scripts\smart_terrain.script` and
  `scripts\bind_physic_object.script` read).
- Analysis: static analysis of the binary (Ghidra 12.2, headless; disassembly and decompilation) of the
  graph, cross-table and level-graph readers, the simulation's creation, registration, id assignment,
  current-level setup, switch pass, location update, scheduling, smart-terrain choice, travel, path
  search and the configuration they read. Throwaway decoders written in the dirty workspace from this
  text decoded every file named above, checked every count, sum and invariant of section 9, and ran the
  path search of 7.4 against a shortest-path search.
- Section 6.5, the limits of 6.2 with their checks and section 9.1 were moved here unchanged on 2026-11-27
  from sections 2.2 – 2.6, 7.1 and 8 of `level-objects.md` (dated 2026-11-24), when that specification was
  split by system. Their analysis: static analysis of the shipped binary for the switch predicates of every
  simulation object kind and the switching rule and the renderer match of hanging lamps; reading of the
  game's scripts (`class_registrator.script`, `se_*.script`, `utils.script`, `xr_gulag.script`,
  `gulag_escape.script`) and configuration (`config\gameplay\npc_profile.xml`, `character_desc_*.xml`,
  `info_*.xml`, `dialogs_escape.xml`) as data; for 6.2 the `[alife]` values, the constant of the time limit
  read from `bin\xrGame.dll`, and static analysis of the binary's reading of the `[alife]` keys, the setup
  of the two limits, the switch pass and the scheduled updates (checks LO0-1 … LO0-6).
- No leaked source code, derived projects or third-party documentation of the engine were used.
