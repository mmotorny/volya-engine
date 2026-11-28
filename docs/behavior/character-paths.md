# Paths and restrictions

Behaviour specification CH-9 of `docs/behavior/characters-scope.md` (cited as `scope`; its sections 1.3, 2.2, 5 and
8, with its "Gaps found in review", neither of which concerns this part), for S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): how an online character's (stalker's or monster's) path is built and followed, and how space
restrictors limit it. It covers the restrictor records and their restrictor types, the out- and in-restrictions a
character has (from its spawn data, from the level's default restrictors, added and removed by scripts, and the
dynamic restrictions kept by object id), which level vertices and positions are **accessible** under them (what
the scripts' `accessible` and `accessible_nearest` give), the **border** that keeps the path search inside the
allowed area, the level path search over level vertices, the path pipeline of the level, game and patrol path
types with its path-building states, the choice of patrol points and the arrival at them, the terrain masks
(`terrain`, `terrain_sect`) and the key `SpaceRestrictionSection` that `scope` 8 gives this part.

Builds on (not repeated; cited by name and section):

- `docs/formats/sav.md` (cited as **sav**) 7.4.1 and 7.4.2: the Shape block and the restrictor type of the
  restrictor kinds, the Creature block's dynamic out- and in-restrictions (lists of object ids), the Monster-like
  block's out- and in-restriction texts (restrictor names separated by commas).
- `docs/behavior/script-api-movement-sight-and-memory.md` (**X**): what scripts set and read; its 3 (the movement
  request, `set_dest_level_vertex_id`, the path type numbers `game_path` 0, `level_path` 1, `patrol_path` 2,
  `no_path` 3), 4 (`set_patrol_path`, `get_current_point_index`, `path_completed`, the detail path's members), 5
  (`add_restrictions`, `remove_restrictions`, `remove_all_restrictions`, `in_restrictions`, `out_restrictions`,
  `base_in_restrictions`, `base_out_restrictions`, `accessible`, `accessible_nearest`).
- `docs/behavior/stalker-movement.md` (**CH-10**): the movement update that runs the path pipeline of section 6,
  the detail path (its building, extrapolation and completion), walking along it.
- `docs/behavior/offline-simulation.md` (**OS**) 1.6 (vertex types, terrain masks), 3 (level vertices, cells), 4
  (patrol paths: points, flags, links and their weights), 6.5 (switching online), 7.4 (the game graph's path search,
  its admission rule and the `terrain` key).
- `docs/behavior/script-api-object-event-callbacks.md` (**O**): the callback `patrol_path_in_point` (id 9) and
  `set_patrol_extrapolate_callback`.
- `docs/behavior/script-api-simulation-registry-and-game-graph.md` (**K**) 8: the simulation side of the dynamic
  restrictions (type numbers 4 in, 5 out).
- `docs/behavior/script-api-level.md` (**N**): the `patrol` holder and its constants.
- `docs/behavior/character-identity.md` (**CH-2**) 4: the terrain masks a stalker's simulation object gets from
  `terrain` and `terrain_sect`.
- `docs/behavior/object-visual-and-transform.md` (**objects**) 4: an object's transform from its position and angles.
- `docs/behavior/smart-terrain-jobs.md` (**CH-28**) and `docs/behavior/stalker-planner.md` (**CH-17**): the
  campfire job of the worked example.
- `docs/behavior/original-determinism.md` 2.2: the shared random generator.
- `docs/behavior/console-log.md` 3.8: the report of a fatal error.
- Not approved, cited by name: the **level graph** CH-8 (`level.ai`: the level vertices, their four links, their
  cells of side *c* = 0.7 m, the packed cell index that orders the vertices, the vertex plane, the level vertex of a
  position).

Conventions:

- **Character**: an online stalker or monster (`scope` conventions). Its **restricted part** exists for every
  character; the scripts reach it through X 5.
- A **restriction text** is a list of restrictor names separated by commas (sav 7.4.1); an **item** is one name of
  it, taken without surrounding spaces. **Union** of texts *a*, *b*: *a* followed by each item of *b* that *a* does
  not contain, joined with commas; **difference** *a* − *b*: the items of *a* that *b* does not contain [CP-7].
- A **sphere** (*p*, *r*) is a centre and a radius; positions and lengths in metres, world axes as `objects`.
- *c* is the level graph's cell size (0.7 on every level of this edition; CH-8).
- Each fact is **static** (from static analysis of the shipped `bin\xrGame.dll`, 6,246,400 bytes; nothing was run),
  **measured** (from game data or the shipped scripts' text), **believed** (with the reason) or **unestablished**.
  Every fact carries a check label [CP-*n*] of `checks/character-paths/checks.py` in the dirty workspace.

## 1. Data (measured)

### 1.1 Restrictors

A **restrictor** is a record of a kind with the Shape block followed by a `u8` **restrictor type** (sav 7.4.2:
restrictor, smart zone, level changer, team base, and the zone and anomaly kinds through the Zone block). In
`spawns\all.spawn`, 2,119 records have one: type 0 in 788, type 2 in 270, type 3 in 1,061; no record has type 1 or
any other value [CP-1]. The types mean [CP-5]:

| Type | Meaning |
|---|---|
| 0 | a restrictor that restricts only the characters that name it |
| 1 | a **default out-restrictor**: named in every character's out-restrictions on its level (2.2) |
| 2 | a **default in-restrictor**: named in every character's in-restrictions on its level (2.2) |
| 3 | not a restrictor: never registered, so no restriction text can use it |

Examples of type 2 on `l01_escape`: the mine fields `esc_mine_field_1` and `esc_mine_field_2`, the ten
`camp_fire_0000` … `camp_fire_0009` boxes around camp fires (section 9). Type 0 includes 226 smart-terrain records
and 74 respawner records besides the plain restrictors [CP-1].

### 1.2 The restriction fields of characters

Of the 1,073 stalker records, 312 have restriction texts in the Monster-like block: 243 an out-restriction text
only, 68 an in-restriction text only, 1 both; of the 276 monster records, 59 (47 out only, 12 in only). No record
has a dynamic out- or in-restriction [CP-2]. Examples: `dar_m_poltergeist_normal_flame_0000` out
`dar_poltergeist_territory`; `agr_nii_soldier1` in `agr_space_restrictor_tower1,agr_space_restrictor_tower2,
agr_space_restrictor_tower3` [CP-2].

### 1.3 Terrain masks

`config\creatures\m_stalker.ltx` has the 8 sections `scope` 1.3 lists; each line is a terrain mask of OS 1.6, a mask
over the four **game-vertex** type bytes (not over level vertices) [CP-3]:

| Section | Masks |
|---|---|
| `stalker_terrain` | 255,000,255,255; 008,001,255,255; 012,001,255,255 |
| `stalker_dolg_terrain`, `stalker_ecolog_terrain` | 255,000,255,255; 008,001,255,255 |
| `stalker_freedom_terrain` | 255,000,255,255; 012,001,255,255 |
| `stalker_zombied_terrain` | 255,000,255,255; 010,001,255,255 |
| `stalker_bandit_terrain`, `stalker_killer_terrain`, `stalker_military_terrain` | 255,000,255,255 |

All 95 stalker sections of the `system.ltx` tree, so every stalker record, have `terrain = 255,255,255,255,0,1`,
the single mask 255,255,255,255 (OS 7.4) [CP-3]. Every game vertex matches that mask, so a stalker's game path
search (OS 7.4 offline, 6.3 online) admits every accessible game vertex whatever `terrain_sect` adds (CH-2 4): in this
edition `terrain_sect` changes no search. No mask enters the level path search (section 5) [CP-21, CP-24].

### 1.4 `SpaceRestrictionSection`

The key occurs in 28 lines of 19 files under `config\creatures\` (e.g. `[stalker]`
`SpaceRestrictionSection = space_restrictor,zone_mine_field`). The text occurs in no shipped binary and no script:
the game does not read it [CP-4].

## 2. Restrictions of a character (static)

### 2.1 Registering restrictors

A restrictor of type 0, 1 or 2 is **registered** by name when it comes online (OS 6.5; restrictor kinds never
switch offline) while the level graph is loaded; type 3 is not [CP-5]. Registering a type-1 restrictor appends its
name to the level's **default out-list**, a type-2 restrictor to the **default in-list** (with a comma when the list
is not empty; the list is then normalised, 2.2); when the list changed, every character's restrictions are computed
again (2.2) [CP-5]. Names are looked up as given; the records' names are unique within each level
(`l01_escape`: 151 restrictors of types 0 to 2, no two with one name) [CP-27]. A restrictor that unregisters takes
its name out of the default lists, after which every character's restrictions are computed again (**believed**: it
unregisters when it is destroyed) [CP-5].

### 2.2 Own and combined restrictions

Each character has an **own out-text** and **own in-text**. From them and the level's default lists its
**combined** restrictions are [CP-6]:

- out = own out ∪ (default out-list − own in);
- in = own in ∪ (default in-list − own out).

So a character can exempt itself from a default restrictor by naming it in the other text. A combined text with
more than one item is kept **normalised**: its items sorted in ascending byte order and joined with commas
(duplicates kept) [CP-7]. `out_restrictions()` and `in_restrictions()` (X 5) give the combined texts;
`base_out_restrictions()` and `base_in_restrictions()` give the own texts, which include what scripts added [CP-6].
The **out area** is the union of the shapes of the combined out-restrictors, the **in area** that of the in-restrictors.

### 2.3 Setting them

- **Coming online**: the own out-text is the Monster-like block's out-restriction text followed by the names of
  the simulation objects of the Creature block's dynamic out-restrictions whose game vertex is on the current level,
  each appended with a comma; likewise the own in-text from the in-restriction text and the dynamic
  in-restrictions. The dynamic names are added only when the simulation exists (believed: always in this game)
  [CP-8].
- `add_restrictions(out, in)`: when either text is not empty, own out := own out ∪ out and own in := own in ∪ in;
  a character with no restrictions recorded yet gets the two texts as given [CP-9].
- `remove_restrictions(out, in)`: when either text is not empty, own out := own out − out and own in := own in − in;
  nothing when the character has no restrictions recorded [CP-9].
- `remove_all_restrictions()`: asks the simulation object to empty its dynamic out- and in-lists (types 5 and 4;
  **believed**: K 8's `remove_all_restrictions` edit), marks the path to be rebuilt when either combined text was not
  empty, then sets both own texts empty, removing the spawn data's restrictions as well [CP-10].
- **Dynamic restrictions** (by object id): adding a list of out ids and a list of in ids takes each id whose object
  is online and whose name does not already occur, as a substring, in the combined text of that kind; it sends the
  simulation object an "add" of that id with its type (5 out, 4 in) (**believed**: K 8's edit, so the id is saved in
  the Creature block) and adds the names as `add_restrictions` does. Removing takes the ids whose names do occur,
  sends a "remove" and removes the names [CP-11]. Engine users: the stalker's planner action of getting out of
  an anomaly (CH-17) adds as in-restrictions the anomalies of a list the stalker keeps (**unestablished** which;
  believed the anomalies it is in or near); a monster's anomaly memory adds as in-restrictions the anomalies it
  newly detects and removes those it has remembered longer than its `Anomaly_Detect_Time_Remember` (CH-12 for
  when) [CP-11].

Every change of either text (and coming online) marks the character's path to be rebuilt at its next movement
update (section 6) [CP-9, CP-10]. Whether the texts added online are written back into the Monster-like block for
a save is **unestablished** (**believed** not: no such write was found; `xr_gulag.script` sets a job's restrictions
again after a load, section 8).

## 3. Inside, cells and accessibility (static)

### 3.1 A restrictor's shape

A restrictor contains a sphere (*p*, *r*) when one of its Shape elements meets it [CP-12]:

- a sphere element (centre **q** transformed to the world by the restrictor's transform, radius *R* unchanged;
  **believed**: the transform is `objects` 4's, built the same way for these kinds without a model):
  |*p* − **q**|² < (*R* + *r*)²;
- a box element (rows **a₁**, **a₂**, **a₃**, **o** of sav 7.4.1, transformed to the world): the box is the
  parallelepiped of centre **o** and corners **o** ± ½**a₁** ± ½**a₂** ± ½**a₃**; it meets the sphere when, for each
  of its six face planes, the signed distance of *p* (positive outside, **believed** from the face order: with the
  other sign no box would contain its own centre) is at most *r*.

A test against several restrictors (a **composition**) is true when any of them contains the sphere [CP-12].

### 3.2 Cell tests

A level vertex's **five test points** are the four corners of its cell shrunk by 0.001 m, at (*x* ± *d*, *z* ± *d*)
with *d* = *c*/2 − 0.001 and heights on the vertex plane (CH-8), and the vertex position itself [CP-13]. A vertex is
**wholly inside** an area when every test point, as a sphere of radius 0.001, is inside it; **partly inside** when
at least one is [CP-13].

### 3.3 Accessible vertices

A level vertex is **accessible** to a character when [CP-14]:

1. its restrictions are not yet initialised (3.5): every vertex is accessible; otherwise
2. with combined out-restrictions, it is wholly inside the out area; and
3. with combined in-restrictions, no test point is inside the in area.

`accessible(v)` (X 5) gives exactly this [CP-14].

### 3.4 Accessible positions

`accessible(pos)` (X 5) tests the sphere (pos, 0.001) [CP-17]:

1. restrictions not initialised: true;
2. pos must be a valid vertex position of the level graph (CH-8: inside its bounds, in a cell that has a vertex);
   otherwise false;
3. with out-restrictions: the sphere must meet the out area; no vertex of the out area's border (3.6) in the same
   cell column may lie less than 2 m above or below pos; and the point of pos's level vertex plane under pos (as a
   sphere of radius 0.001) must meet the out area, pos's level vertex being valid;
4. with in-restrictions: the sphere must not meet the in area, and no vertex of the in area's border in the same
   cell column may lie less than 2 m above or below pos.

### 3.5 Initialisation

A character's restrictions are initialised the first time they are used once every restrictor they name is
registered; until then the test is retried at each use and everything counts as accessible, and no border is
applied [CP-16]. So a name that no online restrictor has (a misspelling, a restrictor of another level) leaves the
character unrestricted.

### 3.6 Borders

- A restrictor's **border**: for each Shape element, the level vertices whose packed cell index (CH-8) lies from the
  first vertex at or after the low corner of the element's horizontal bounding box to the first vertex after its
  high corner, inclusive, and that are partly but not wholly inside the restrictor (all elements) [CP-15]. A box's
  bounding box is that of its eight world corners; a sphere's is the restrictor position plus the element's centre,
  not rotated, ± *R* in x and z [CP-15]. A corner that is not a valid vertex position (3.4) stands for the first or
  the last vertex [CP-15].
- A composition's border: the union of its members' borders, without the vertices wholly inside the composition
  [CP-15].
- A character's border: with out-restrictions, the out area's border without the vertices wholly inside the in
  area, together with the in area's border vertices that are partly inside the out area; with in-restrictions only,
  the in area's border; with none, empty [CP-15].

Every border vertex is inaccessible (3.3): it is partly outside the out area or partly inside the in area.

## 4. `accessible_nearest(pos, out)` (static)

With combined out-restrictions [CP-18]:

1. **Candidates**: the character's border vertices that have at least one valid linked vertex accessible to the
   character (3.3).
2. **b**: the candidate whose position is nearest to pos (squared 3D distance; the first in ascending vertex order
   among equals).
3. **Result**: among b's linked vertices that are valid and accessible, the one nearest to pos; this number is
   returned.
4. `out` receives the one of the result's five test points (3.2) nearest to pos.

With in-restrictions only, the result is the linked vertex nearest to pos without a test point inside the in area,
of the border vertex nearest to pos (**believed**: the candidates are then the in area's border vertices with such a
link) [CP-18]. It does not first test whether pos itself is accessible: an accessible pos still gives a vertex
next to the border [CP-18]. With no restrictions at all (no default restrictors on the level and none of its own)
the result is **unestablished** (the program reads through an absent restriction; believed a crash).

## 5. The level path (static)

The level path is the sequence of level vertices from the character's level vertex (the **start**) to the
destination vertex. It is searched as follows [CP-19, CP-20, CP-21]:

1. If the start vertex is accessible (3.3), every vertex of the character's border is **disabled** in the level
   graph for the duration of the search, and enabled again after it. If the start is not accessible, nothing is
   disabled, so a character outside its allowed area can walk back into it. Every vertex is enabled when the level
   graph is loaded [CP-19, CP-20].
2. If this search has the same start and destination as the last failed search of this character, it fails at
   once without searching [CP-21].
3. **A\*** over the level vertices: the cost *g* of the start is 0; a vertex is expanded by taking, in link order,
   each of its four links that names a valid vertex (CH-8) that is not disabled; the start itself is not tested.
   Each step costs *c*, so *g* is *c* times the number of steps. The estimate *h* of a newly opened vertex is
   2 · *c* · (|Δcolumn| + |Δrow|), the cell distance from the **vertex being expanded** (not the new vertex) to the
   destination; the start's *h* is taken from the start. *f* = *g* + *h*. All in single precision, *h* computed as
   ((cells · *c*) · 2) [CP-21].
4. The open vertex with the smallest *f* is taken next; among equal *f* the most recently opened or improved one.
   Taking the destination ends the search; the path is the chain of predecessors. An already open vertex reached
   with a smaller *g* gets that *g*, its *f* = its first *h* + the new *g* and the new predecessor; closed vertices
   are never reopened [CP-21].
5. The search fails when no open vertex is left, or when, before the next vertex is taken, 65,500 vertices have
   been opened; there is no distance or step limit [CP-21].

The same border is disabled, on the same condition about its start vertex, while CH-10 builds the detail path
[CP-19]. The level path is used whole: the detail path is built over it from the character's position to the
destination (CH-10) [CP-22].

## 6. The path pipeline (static)

Run by CH-10's movement update while movement is enabled. Its **path-building state** is the number `path_completed`
(X 4.1) compares with 8. When the path is marked to be rebuilt (a changed destination, path type, restriction, patrol
path or start, CH-10 3.3 and section 2), the level, game and patrol parts are marked out of date and the state is
reset by path type: game 0, level 4, patrol 3, no path −1 (nothing is built) [CP-22].

### 6.1 Level path type

On reset, the destination vertex is made accessible: if it is not accessible (3.3), it becomes
`accessible_nearest` of its position (section 4), and the detail path's end position becomes that function's point;
otherwise, if the detail path's desired end position is not accessible (3.4), it becomes the destination vertex's
position [CP-22]. Then:

| State | What happens |
|---|---|
| 4 | search the level path (section 5); on success 5, on failure stay at 4 |
| 5 | take the whole level path as the stretch to walk; 6 |
| 6 | build the detail path from the character's position and body heading (CH-10); 7 on success, 4 on failure |
| 7 | follow it (CH-10); when the detail path is completed and so is the level path: 8; when either is out of date: 4 |
| 8 | the destination is reached; 4 again when either path becomes out of date |

A search (states 4 and 6) is either done at once or **deferred**: queued to a job the engine runs later in the frame
(believed: alongside rendering), the state then set as above when the job has run; deferral is used when a
program-wide setting (on for both searches at start) allows it, unless the character is flagged otherwise
(**unestablished** which flag) [CP-22]. Done at once, the level path search stops at state 5 until the next movement
update, unless both are to be done in one update (**unestablished** when) [CP-22].

### 6.2 Game path type

| State | What happens |
|---|---|
| 0 | choose the destination game vertex (**unestablished** how; believed: the one the engine's own actions set, CH-17); 1, or stay at 0 when none is chosen |
| 1 | search the game graph from the character's game vertex: OS 7.4's search with its admission rule (accessibility, terrain masks and the start-vertex rule), the masks being the online character's own (6.3); on failure the log lines `! Cannot build GAME path! (object %s)`, `! CURRENT LEVEL : %s`, `! CURRENT game point position : [%f][%f][%f]`, `! TARGET LEVEL : %s` and the state stays 1; on success 2 |
| 2 | take the next game vertex of the path; if it is on another level: 9; otherwise 4 |
| 4–7 | as 6.1, towards that game vertex's level vertex (made accessible as in 6.1); the detail path is extrapolated (CH-10) |
| 7 | when the level stretch is done: 2 for the next game vertex; when the game path is done: 8 |
| 9 | the character asks its simulation object to move to that game vertex (its level vertex and position) on the other level (believed: it then goes offline there); nothing more is built |

In states 7 and 8 a destination choice that is no longer valid restarts at 0 (**unestablished** when it stops being
valid) [CP-23].

### 6.3 Online terrain masks

The online character's masks are read by the same reading as OS 7.4's `terrain` rule from its section's `terrain`
key, a `terrain` key in section `[alife]` of its custom data replacing it; `terrain_sect` is stored with the specific character and read by
none of this (CH-2 4 gives it to the simulation object only) [CP-24]. With 1.3 this admits every accessible game
vertex for every stalker.

### 6.4 Patrol path type

The patrol path, start type, route type and random flag are X 4.1's `set_patrol_path` [CP-25]. States:

| State | What happens |
|---|---|
| 3 | choose a point (6.5); if no point is left: 8; otherwise the destination vertex is the point's level vertex and the desired end position the point's position; 4 (what stops the choice without either is **unestablished**) |
| 4–6 | as 6.1; in 6, the detail path's extrapolation (CH-10) is the result of the character's extrapolate callback (O 8) called with the current point's index, or on when there is none |
| 7 | as 6.1; when the detail path and the level path are completed, the point is **reached**: 3 |
| 8 | the path is finished (`path_completed()` true); 3 again when the patrol path is out of date |

A detail path counts as completed, for the extrapolated path, when its current point is the last point before the
extension; otherwise at its last point (CH-10 5.4) [CP-25].

### 6.5 Choosing a patrol point

When the patrol path is not yet under way (just set, or out of date), the **start point** is chosen by start type
(N's `patrol` constants) [CP-25]:

| Start type | Point |
|---|---|
| 0 `patrol.start` | point 0 |
| 1 (the number of `patrol.continue`) | the point with the highest index |
| 2 `patrol.nearest` | the point nearest the character (squared 3D distance) whose position is accessible (3.4) |
| 3 `patrol.custom` | the point given by `set_start_point` (X 4.1) |
| 4 `patrol.next` | the point after the previous one (index + 1 when that exists, else a link of the previous point chosen as below, ignoring the "not the previous" rule); when there is no previous point or no such point: as nearest |

No point is a fatal error (console-log 3.8) with the message `any vertex in patrol path [%s] in inaccessible for
object [%s]` (path and character names), after the log has listed the lines `DEFAULT OUT RESTRICTIONS :`,
`DEFAULT IN RESTRICTIONS  :`, `OUT RESTRICTIONS         :`, `IN RESTRICTIONS          :` with the texts; a point
whose level vertex number is not below the level graph's vertex count is a fatal error with the message
`patrol path[%s], point on path [%s],object [%s]` (path, point and character names; the point's game vertex is
not tested) [CP-25]. If the character's position is within 0.1 m of the start
point on each axis, the start point counts as reached at once [CP-25].

When a point is **reached** [CP-25]:

1. The character's callback `patrol_path_in_point` (O, id 9) is called with the point's index (O 4's arguments).
2. The next point is chosen among the point's links (OS 4, in stored order): **candidates** are the linked points
   other than the previous point whose positions are accessible (3.4). With the random flag set and more than one
   candidate, *r* = (one draw of the shared generator, original-determinism 2.2, 0 … 32,767) · (sum of the
   candidates' link weights) / 32,767, and the next point is the first candidate at which the running sum of
   weights reaches *r*; otherwise the first candidate.
3. With no candidate and a route type other than `patrol.stop` (0), e.g. `patrol.continue` (1): the first linked
   point, the previous one included, whose position is accessible; with `patrol.stop`, or no such point: the path is
   finished.
4. Otherwise the previous point becomes the reached one, the current point the chosen one, and state 3 sends the
   character there.

## 7. The callbacks the scripts use (measured, with static)

- `patrol_path_in_point`: called on every reached point (6.5), the first point included when the character starts
  on it [CP-25].
- The extrapolate callback is called once per detail path built on a patrol path (6.4); the stalker binder
  (`xr_motivator.script`) returns true (extrapolate) for a point without flags and false for a point with flags,
  so a walker passes through flagless points without stopping and stops on flagged ones; the monster binder
  (`bind_monster.script`) returns false when its monster is not under script control, otherwise the same [CP-26].

## 8. Use by the shipped scripts (measured)

- `xr_gulag.script` (`gulag:set_restrictions`), when a job is given: removes the out- and in-restrictions the smart
  terrain set before if they differ from the job's, then adds the job's `out_rest` and `in_rest` with
  `add_restrictions(out, in)` [CP-26].
- `move_mgr.script` (the movement of `xr_walker.script` and the other schemes that walk patrol paths): sets path type `patrol_path`, detail path type `move.line`, and
  `set_patrol_path(path_walk, patrol.nearest, patrol.continue, true)`, or, when a current point is known,
  `set_start_point(index)` with `patrol.next`; it registers `patrol_path_in_point` and on each call takes the point's
  flags (`a`, `s`, `p`, `sig`, `ret`) for its states; on leaving the scheme it sets path type `level_path` [CP-26].
  `utils.stalker_at_waypoint` counts a stalker as at a point within a squared distance of 0.13 [CP-26].

## 9. Worked example: a novice at the beginners' village campfire

`esc_lager1` (record 93, `esc_stalker_novice`, CH-10 7) comes online at a New Game at level vertex 43,968,
(−209.3, −19.74, −132.3); its Monster-like texts are empty [CP-2, CP-29]. On `l01_escape`, 33 restrictors have type 2
(no type 1): `camp_fire_0000` … `camp_fire_0009`, `esc_in_rest_5`, `esc_mine_field_1`, `esc_mine_field_2`,
`esc_soldiers_camp`, `esc_space_restrictor`, `esc_space_restrictor_0000` … `esc_space_restrictor_0016`,
`esc_specnaz_camp` [CP-27]. So its combined out is empty and its combined in is these 33 names, sorted (2.2).

Its campfire job (CH-28, `gulag_escape.script`, type `esc_lager`) has `out_rest = "esc_lager_guard_kill_zone"`,
`in_rest = ""`; `xr_gulag` calls `add_restrictions("esc_lager_guard_kill_zone", "")` [CP-26]: own out = combined out =
`esc_lager_guard_kill_zone`, combined in unchanged; the path is marked to be rebuilt (2.3).

- `esc_lager_guard_kill_zone` (type 0, angles 0, position (−212.61, −22.88, −122.06)) has four boxes; the one holding
  the village spans x −227.46 … −165.72, z −173.15 … −123.63, y −28.06 … −9.17 [CP-28].
- `camp_fire_0004` (type 2, position (−206.862, −19.443, −140.035), angles 0) is one box of 1.784 × 1.934 × 1.784 m:
  x −207.754 … −205.971, z −140.926 … −139.143. Its border is the 10 vertices 45,285, 45,286, 45,287, 45,943, 45,945,
  46,573, 46,575, 47,200, 47,201, 47,202; wholly inside it are 45,944 (the camp centre, CH-10 7) and 46,574. All ten
  stay in the character's border (they lie wholly inside the kill zone and partly in the in area) [CP-28].
- The start 43,968 and the seat 45,947 (CH-10 7's sector (0,0,1)) are accessible; 45,944 is not [CP-28].

The level path (section 5, the start being accessible so the border is disabled): 43,968, 43,967, 43,966, 43,965,
43,964, 43,963, 43,962, 43,961, 43,960 (eight steps of −0.7 m in z, to (−209.3, −137.9)), then 44,627, 45,289,
45,947 (three steps of +0.7 m in x): 12 vertices, 11 steps, 35 vertices opened. Without any restriction the search
gives the same path, so here the restrictions change nothing [CP-29].
Had a script sent the stalker to the camp centre 45,944 instead, state 4 would be preceded by 6.1's replacement:
the nearest candidate border vertex is 45,943 (0.70 m from the centre), its only accessible link 45,942, and the
end position its test point (−206.851, −20.013, −141.051) [CP-30]. No decision here lies within 0.1 m of changing,
so single-precision rounding cannot change these results [CP-31].

## 10. Unestablished and believed

- What makes the level path search run at once versus deferred, and the character flag that forbids deferral (6.1).
- How the game destination is chosen and when it stops being valid (6.2); what the level-change request does on the
  simulation side (6.2).
- Whether online-added restriction texts are saved (2.3); when a restrictor unregisters (2.1).
- What ends a patrol point choice without a point and without finishing (6.4).
- `accessible_nearest` with no restrictions at all (section 4).
- The face-plane orientation of boxes (3.1), believed outward.
- Everything here is static or read from the data: nothing was observed in the running game.

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binary `bin\xrGame.dll`
  (6,246,400 bytes); `bin\xrCore.dll` for the 16-bit vertex normal used to place cell corners (CH-8's).
- Data examined: the winning copies of `config\`, `scripts\` and `spawns\all.spawn` as extracted for the approved
  `pda-scope` (`config\creatures\*.ltx`, `xr_gulag`, `gulag_escape`, `move_mgr`, `xr_motivator`, `bind_monster`,
  `utils`, `lua_help.script`), `levels\l01_escape\level.ai`, the restrictor and character records of `all.spawn`.
- Analysis: static analysis of `bin\xrGame.dll` (the full headless Ghidra decompilation made for earlier approved
  specifications, read-only headless instruction listings, run-time type information, initial values read from the
  file): restrictor registration, the restriction sets and their texts, shape, composition and restriction tests and
  borders, accessibility and nearest accessible vertex, the border applied around path building, the level and game
  path searches, the path pipeline, patrol point choice, the patrol callbacks. No debugging or execution; the game
  was not run. The worked example's values come from a model of the rules above over the data
  (`checks/character-paths/model.py`).
- Checks: `checks/character-paths/checks.py` (with `sp.py`, `model.py`, `example.py`, the `stalker-planner`,
  `characters-scope` and `script-api-level` helpers), run with `python3 -I`; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used. A\* (Hart, Nilsson
  and Raphael, 1968) is named only to describe what the binary computes.

## Gaps found in review

- Besides marking the path to be rebuilt, `add_restrictions`, `remove_restrictions` and `remove_all_restrictions` (2.3) also send the character a further notification of the change; what it does (for a stalker or a monster) is not given (unestablished).
- Section 7 omits that the stalker binder's extrapolate callback, while a scheme is active, also passes the event to that scheme and to `move_mgr`, which records the current point index (and the time); that recorded index is the "current point" section 8's `set_start_point` with `patrol.next` uses.
