# Level graph

Format and behaviour specification CH-8 of `docs/behavior/characters-scope.md` (cited as `scope`; its sections 1.5,
2.2, 5, 7 and 8, with its "Gaps found in review", neither of which concerns this part), for S.T.A.L.K.E.R.: Shadow of
Chernobyl (Russian edition): the level's navigation graph `levels\<level>\level.ai` in full, and how the game uses it at
run time for the characters and monsters on the level: the vertex record's every byte, what the game keeps of the
graph while the level is loaded, which vertices count as accessible, the lookup of a vertex for a point, the
neighbours of a vertex, the straight-line tests over linked cells, and the two searches over the graph (the level path
search and the nearest-vertex search). It also covers the patrol paths' point flags and links and whether and how
`levels\<level>\level.game` is read (`scope` 2.2, 7).

Builds on (not repeated; cited by name and section):

- `docs/behavior/offline-simulation.md` (**OS**): 2 (`level.gct`, the cross table: its identifiers and its cell per
  level vertex), 3 (`level.ai`'s header, the 21-byte vertex record's position bytes, the vertex position formula, the
  row length *r*, the column count *s*, a vertex's **cell**), 4 (the patrol paths of `spawns\all.spawn` chunk 3), 5.2
  (a point **off the level's grid**).
- `docs/behavior/object-visual-and-transform.md` (**OV**) 4: when an object comes online, its level vertex and the
  height correction (4.1), the **plane height** of a vertex at a point (4.2), which kinds use the level graph online
  (4.3), and **the vertex search** for a point from a start vertex, with its **cell vertex** (step 3) and its fallback
  (step 4) (4.4).
- `docs/behavior/script-api-level.md` (**N**): 3.1 (the links, the cover values and the plane bytes of a vertex record),
  3.3 (the walk of `level.vertex_in_direction`), 3.4 (reading the cover values), 4 (the patrol path store and its
  members as scripts read them).
- `docs/behavior/script-api-game-object-basics.md` (**M**) 4: `level_vertex_light`, the light value of a vertex.
- `docs/behavior/script-api-movement-sight-and-memory.md` (**X**) 4.3 and 5: the fighter members `vertex_in_direction`
  and `accessible`.
- `docs/formats/chunks.md`: chunk sequences (`level.game` is one; `level.ai` is not).
- `docs/behavior/stalker-movement.md` (CH-10) and `docs/behavior/smart-terrain-jobs.md` (CH-28): the walk to a campfire
  seat of the worked example.
- Not approved, cited by name: **paths and restrictions** CH-9 (which vertices a restricted creature may not enter, when
  the level path search runs and what is done with its result, following a patrol path), stalker combat CH-18 (cover).

Conventions:

- A **vertex** is a level vertex of the current level's `level.ai`, numbered from 0 in file order; *N* is their count,
  *c* the cell size (0.7 in all 18 levels), *r* and *s* the row length and column count of OS 3. A vertex's **column**
  and **row** are floor(*p* / *r*) and *p* mod *r* of its packed value *p* (OS 3); its **centre** is the x and z of its
  position. A vertex number is **valid** when it is below *N*.
- "x–z" means the horizontal plane (the world's y is up). Distances called x–z ignore height.
- Each fact is **static** (from static analysis of the shipped `bin\xrGame.dll`, 6,246,400 bytes; nothing was run),
  **measured** (from the game data), **believed** (with the reason) or **unestablished**.

**Checks.** Every fact carries a label [LG-*n*] of `lgr/checks.py` in the dirty workspace (`python3 -I checks.py`;
helpers `fn.py`, `pe.py`, `gh.py`, `leveldata.py`, `lgame.py`, `lg.py`, `rec.py`, `vstats.py` beside it). All 23 checks
pass.

## 1. `level.ai`

### 1.1 The file and its loading (static)

`level.ai` is opened from the level's folder when the level's navigation data is loaded and stays open, read in
place, while the level is loaded [LG-1]. The header is OS 3's (56 bytes); a version other than 8 is fatal (an
assertion) [LG-1]. *r* and *s* are computed from the header as OS 3 gives them [LG-1]. Three identifiers must agree,
each mismatch fatal [LG-2]:

| Compared | Message |
|---|---|
| the cross table's level identifier (OS 2) and the `level.ai` identifier | `cross_table doesn't correspond to the AI-map` |
| the cross table's game identifier and `game.graph`'s identifier (OS 1) | `graph doesn't correspond to the cross table` |
| the current level's identifier in `game.graph`'s level record (OS 1.2) and the `level.ai` identifier | `graph doesn't correspond to the AI-map` |

The graph also takes the level id of the current level's record (OS 1.2) as its own [LG-2]. When the graph is
loaded relative to the simulation and the level's other loading steps is OS's and `docs/behavior/level-loading.md`'s,
not this spec's.

### 1.2 The vertex record

Every byte of the 21-byte record (vertex *k* at byte 56 + 21·*k*), little-endian:

| Bytes | Content | Where specified |
|---|---|---|
| 0–11, bits 0–91 | four **links**, 23 bits each, at bits 0, 23, 46 and 69: the vertex adjacent toward −x (link 0), +z (link 1), +x (link 2), −z (link 3); 8,388,607 (2²³ − 1) means none | N 3.1 [LG-5] |
| 11, bits 4–7 (bits 92–95) | the **light** value 0 … 15 | M 4 |
| 12–13 | four **cover** values of 4 bits, toward −x, +z, +x, −z | N 3.1, 3.4 |
| 14–15 | the packed **plane** | OV 4.2 |
| 16–18 | the packed column and row *p* | OS 3 |
| 19–20 | the height *q* | OS 3 |

No byte is left over. A link is used only when it is valid (below *N*); the value "none" is never below *N* [LG-5].

### 1.3 Measured properties of the 18 levels

Over the 3,807,603 vertices of the 18 levels [LG-3, LG-4]:

- every level has version 8 and *c* = 0.7, and its vertices are stored sorted by packed column and row (OV 4.4 relies on
  it);
- of the 15,230,412 links, 481,775 are none and 14,748,637 name a vertex; every one of these is valid, is not the vertex
  itself, and names a vertex whose column and row differ from the vertex's own by exactly the step of its direction
  (−1 column, +1 row, +1 column, −1 row);
- 3,503 links have no link back (the vertex linked to does not link back in the opposite direction): 396 on
  `l01_escape`, none on `l12u_control_monolith`; so the graph is not always symmetric, and every operation below
  follows only the links of the vertex it is at;
- every vertex has at least one link;
- the light value is 0 in every vertex of every level except `l11_pripyat`, where it is 1 in every vertex;
- every cover value 0 … 15 occurs in every level.

## 2. The graph at run time

### 2.1 Neighbours (static)

The **neighbours** of a vertex are the valid vertices its four links name, taken in link order 0, 1, 2, 3 (−x, +z, +x,
−z) [LG-5]. Every operation below that moves from a vertex to a neighbour uses this order, and ties go by it where
stated.

### 2.2 Accessibility (static)

The game keeps one **accessibility bit** per vertex for the loaded level [LG-6]:

- When the graph is created, there are *N* bits, all set [LG-6].
- A vertex is **accessible** when it is valid and its bit is set [LG-6].
- The bits are changed only for a list of vertices that a creature's restrictions give: they are cleared just before a
  level path search (section 4.1) or another test made for a creature with restrictions, and set again just after it
  [LG-6]. Which vertices are on the list, and for which creatures and tests, is CH-9's. Outside such a search or test
  every vertex is accessible.

Each operation below says whether it tests accessibility.

### 2.3 Looking up the vertex of a point (static)

- **On the grid**, **cell** and **cell contains a point**: OS 5.2, OS 3 and OV 4.1 step 5.
- **The cell vertex** of a point: OV 4.4 step 3 (several vertices may share a cell at different heights; the one at or
  just below the point's height is taken).
- **The vertex search** from a start vertex for a point: OV 4.4. It does not test accessibility.
- **Online creatures**: OV 4.1 gives the search made when an object that uses the level graph online (OV 4.3; every
  stalker, trader and monster) comes online. The same search is made each time the object's location is brought up to
  date after it moves, with the object's current vertex as the start vertex and the point (position x, the y of the
  object's centre, position z), the query OV 4.1 makes at coming online; the result becomes its current vertex and its game
  vertex becomes the cross table's for that vertex (OS 2) [LG-7]. So while a creature stays in its vertex's cell the
  vertex is kept (OV 4.4 step 2); when it walks into a neighbouring cell, that cell's vertex is taken by the height rule
  of step 3. The height is not corrected then (OV 4.1).

## 3. Straight-line tests (static)

These are the "straight walk" and "walkable" tests `stalker-movement` (CH-10) relies on. Each follows a straight line in
the x–z plane from cell to cell through links, never jumping between cells that are not linked.

### 3.1 Crossing a cell

A line through a point **a** with direction **d** (both x–z) **crosses** a vertex's cell when, with the cell's sides at
x = *x*v ± *c*/2 and z = *z*v ± *c*/2 measured from **a**, either *d*x ≠ 0 and the line's z at one of the two x sides
lies within the z sides widened by 10⁻⁵ on each side, or *d*z ≠ 0 and the line's x at one of the two z sides lies
within the x sides widened by 10⁻⁵ [LG-8]. The line is unbounded both ways (the walks below keep moving toward the
end point, so in effect only the stretch between the points matters). Single precision.

### 3.2 The walk to a point

Input: a start vertex *k*0, a start point **a** and an end point **b** (x–z). Result: the vertex whose cell contains
**b**, or **none** [LG-8].

1. If **b** is off the level's grid, none.
2. The walk is at *k*0, with no previous vertex, and its **best distance** is the squared x–z distance from *k*0's
   centre to **b**.
3. At each vertex it looks at the neighbours in link order, skipping the vertex it came from and every neighbour
   whose cell the line from **a** through **b** does not cross (3.1). A crossed neighbour whose cell is **b**'s cell
   (same packed column and row) ends the walk: the result is that neighbour if it is accessible, otherwise none. A
   crossed neighbour whose centre is farther from **b** (squared x–z distance) than the best distance is skipped; the
   first crossed neighbour that is not farther is taken. If no neighbour is taken, the result is none; if the one taken
   is not accessible, the result is none. Otherwise the walk moves to it, the vertex it left becomes the one it came
   from, and the best distance becomes the new vertex's.

The start vertex's own cell is not compared with **b**'s; a caller that needs it compares first (**believed**: the
callers do so).

### 3.3 The walk to a vertex

Input: a start vertex *k*0, an end point **b** and an end vertex *k*e. Result: true or false [LG-9]. As 3.2, with the
start point **a** being *k*0's centre and without 3.2's step 1, except that a crossed neighbour that is *k*e itself (by
number, not by cell) ends the walk: the result is whether *k*e is accessible; every other way the walk of 3.2 gives
none gives false here.

**Disputed in review:** the inputs and the line are not as stated. The given point is the line's start, not its end:
the input is a start vertex *k*0, a start point **a** (x–z) and an end vertex *k*e; the line runs from **a** through
*k*e's centre (direction: *k*e's centre minus **a**; cells measured from **a** as in 3.1); the best distance starts as
the squared x–z distance from *k*0's centre to *k*e's centre, and each crossed neighbour's distance is measured to
*k*e's centre. The rest (no off-grid step, the end by vertex number, the accessibility tests) is as stated. The
stalker's detail path passes its current point and that point's vertex as **a** and *k*0, and a level path vertex as
*k*e.

### 3.4 The step test in a direction

Input: a vertex *k* and a direction **d** (x–z, normalised; a zero direction stays zero). Result: true or false
[LG-10]. Let **e** be *k*'s centre moved 4·*c* (2.8 m) along **d**. The result is true when some neighbour of *k* (in
link order) is accessible, its cell is crossed by the line from *k*'s centre along **d**, and its centre is not farther
(squared x–z distance) from **e** than *k*'s centre is; otherwise false. The constant 4 is fixed [LG-10].

### 3.5 The walk toward a target

N 3.3's walk (toward a target at a distance, keeping the vertex where the line leaves the linked cells), with one
option: **accessible only**. With the option set, the walk stops, keeping the vertex it is at, when the neighbour it
would move to is not valid or not accessible [LG-11]. `level.vertex_in_direction` (N 3.3) uses the walk without the
option; the fighter member `vertex_in_direction` (X 4.3) with it [LG-11].

### 3.6 Users

The detail path of stalkers (CH-10) uses 3.2 and 3.3 for its key points and curves; 3.4 and 3.2 are also used by other
movement code of creatures (**believed**: monsters' and stalkers' local moves; not identified here). Which test each
user calls, with which points, is that user's spec's.

## 4. Searches (static)

### 4.1 The level path search

Input: a start vertex, a goal vertex and three limits (a largest estimated length, a largest number of steps, a
largest number of search entries). Result: the **level path**, the list of vertices from the start to the goal, each
linked from the one before; or failure [LG-12, LG-13].

The search is a best-first search over the vertices with, for each vertex reached, a **length** *g* from the start, an
**estimate** *e* and their sum *f* = *g* + *e* [LG-12]:

- The start has *g* = 0 and the estimate of the start vertex itself.
- Moving over a link costs *c* (0.7): a neighbour reached from vertex *v* has *g* = *g*(*v*) + *c* [LG-12].
- The **estimate** given to a vertex when it is first reached is 2·*c*·(|*C* − *C*g| + |*R* − *R*g|), with *C*, *R* the
  column and row **of the vertex being expanded** (not of the new vertex) and *C*g, *R*g those of the goal [LG-12]. It
  is twice the cell-step distance, so the search need not find a shortest path.
- Only accessible neighbours are reached (2.2) [LG-12].

Each step takes the **best** waiting vertex, the one of least *f*; among equal *f*, the one that entered the waiting
list (or re-entered it, below) last [LG-13]. Before each step the search fails if no vertex is waiting, if the best
*f* is not below the length limit, if the steps made reach the step limit, or if the entries made (every vertex ever
reached in this search, the start included) reach the entry limit [LG-13]. If the best vertex is the goal, the search
succeeds: the level path is the chain of vertices from the start through which the goal was last reached [LG-13].
Otherwise the vertex is done (never reached again in this search) and its neighbours are taken in link order [LG-12]:

- a neighbour not yet reached gets *g*, its estimate as above, and enters the waiting list;
- a neighbour still waiting is updated only if its new *g* is strictly smaller: it keeps its estimate, takes the new
  *g* and *f* and the new vertex as the one it was reached through, and re-enters the waiting list;
- a done neighbour is left as it is.

All lengths and sums are single precision [LG-12]. A start equal to the goal gives the one-vertex path.

**The limits for creatures' movement**: the length limit is the largest single-precision value (so it never ends a
search), the step limit 4,294,967,295, and the entry limit 65,500 [LG-14].

**Repeated failure**: the movement's level-path step remembers the start and goal of the last search that failed;
asked again for the same start and goal it reports failure without searching [LG-15]. The memory is cleared when the
creature's movement is reinitialised (**believed**: when it comes online), and replaced only by a later failure
[LG-15].

When a creature has restrictions, its search runs between the clearing and the setting again of the accessibility
bits (2.2) [LG-6]. When the search is asked for and what is done with its result and its failure is CH-9's.

### 4.2 The nearest-vertex search

Input: a start vertex, a radius *ρ* in metres, a target point (x–z is used) and limits on steps and entries as in 4.1
(its one caller sets both to 4,294,967,295; there is no length limit). Result: the vertex nearest the target among those it reaches [LG-16].

It is the search of 4.1 with three differences [LG-16]: every estimate is 0 (so it reaches vertices in order of *g*,
the number of links times *c*, ties as in 4.1); a neighbour is reached only if it is accessible and its column and row
differ from the start's by (Δ*C*, Δ*R*) with Δ*C*² + Δ*R*² not above *ρ*² / *c*² + 0.5 rounded toward zero; and it has
no goal: it runs until no vertex is waiting (or a limit is reached). Each vertex, as it is taken, is compared with the
best so far by the squared x–z distance from its centre to the target; it replaces the best only when strictly nearer
[LG-16]. The start is the first compared. Its callers (**believed**: creatures choosing a reachable vertex near a
wanted position) and their radius are CH-9's and the monster specs'.

## 5. Patrol paths: point flags and links (measured)

The patrol paths of OS 4 (all levels, 4,294 paths, 10,367 points) [LG-17]:

- **Point flags** (the `u32` of each point): 4,540 points have a non-zero value; the set bits are 0 … 14, 20 and 21 (bit
  0 in 3,497 points). The engine keeps them with the point and hands them to scripts (N 4.4, `flags` and `flag`); what
  a bit means is given by the scripts that read them and by CH-9's following of a patrol path, not here.
- **Links**: 7,267 links in all; 7,257 have weight 1.0, the other ten weights between 0.17 and 0.46; no link goes from a
  point to itself or to a point the path lacks, and no point links twice to the same point. 4,115 points have no link
  from them (N 4.4's `terminal`), 5,691 one, 274 two, and at most eight. How a follower chooses among several links (and
  whether the weights matter) is CH-9's.

## 6. `level.game`

### 6.1 Format (measured, static)

Each of the 18 levels has a `level.game`, a chunk sequence (`chunks.md`) of two chunks [LG-18]:

| Chunk | Content |
|---|---|
| 4096 (0x1000) | one chunk per patrol path, ids 0, 1, … |
| 8192 (0x2000) | spawn points for multiplayer games; empty (size 0) in all 18 levels |

Each path chunk holds four chunks [LG-18, LG-19]:

| Chunk | Content |
|---|---|
| 1 | `u16` version; must be 19 (fatal otherwise) |
| 5 | `stringZ` path name |
| 2 | `u16` point count *n*, then *n* points: `float3` position, `u32` flags, `stringZ` point name |
| 3 | `u16` link count *m*, then *m* links: `u16` from point, `u16` to point, `float` weight |

A point has no level or game vertex in the file. When paths are read from it, each point's level vertex is the cell
vertex (2.3) of its position raised by 0.15 m, if the position is on the grid, otherwise none; when the point lies
outside that vertex's cell its position is replaced by the vertex's position; its game vertex is the cross table's for
the vertex [LG-19].

The 4,294 paths of the 18 files are exactly the 4,294 of `all.spawn` chunk 3 by name, and 4,291 of them agree in every
point's name, position and flags and in every link; the other three (`bar_bar_square_3_place_2_walk`,
`bar_bar_square_3_place_3_walk`, `bar_bar_square_3_place_4_walk` of `l05_bar`) differ only in their one point's x
[LG-18].

### 6.2 When it is read (static)

- **The paths**: the level's loading reads chunk 4096 into the patrol path store only when there is no simulation and
  there is a game graph [LG-19] (N 4.1). In a single-player game the simulation exists, so `level.game`'s paths are
  never read and the store holds `all.spawn`'s (N 4.1).
- **The spawn points**: chunk 8192 is read when the game's state is created (each point a `float3` position, a `float3`
  of angles and four `u8`, **believed**: a team, a kind, a game type and an unused byte) [LG-19]. It is empty in all 18
  levels, so nothing results from it. That a single-player game's state creation reaches this reading is **believed**
  (it is the base step the game types' own creation steps call).

## 7. Worked example: a novice walking to a campfire seat on `l01_escape`

`l01_escape`: *N* = 595,499, *c* = 0.7, *r* = 1,697, *s* = 994 (OV 4.5). `esc_lager1` (record 93, profile
`esc_stalker_novice`, CH-10 7) is at (−209.067, −19.751, −132.083); its record's level vertex is 43,968 [LG-20].

1. **Its vertex** (2.3): the point is in column 106, row 498, the cell of vertex 43,968 alone (position (−209.3,
   −19.738, −132.3)); the stored vertex's cell contains the point, so the search keeps 43,968 when the stalker comes
   online (OV 4.4 step 2) [LG-20].
2. **Its neighbours** (2.1): links 43,290 (−x, column 105), 43,969 (+z, row 499), 44,635 (+x, column 107), 43,967
   (−z, row 497), all valid; cover values 3, 4, 4, 4; light 0 [LG-20].
3. **The level path** to the seat 45,947 (column 109, row 490: three columns +x and eight rows −z; CH-10 7), with every
   vertex accessible (no restriction is assumed here; CH-9) and the limits of 4.1 [LG-21]: the search makes 36 entries
   and 13 steps before taking the goal, and the path is 43,968, 43,967, 43,966, 43,965, 43,964, 43,963, 43,962, 43,961,
   43,960, 44,627, 45,289, 45,947: eight steps −z, then three +x, 12 vertices, as short as the column and row
   differences allow. The paths to the other seats of CH-10 7 are likewise as short as possible: 18, 15, 11, 12, 15, 18
   and 19 vertices to 47,843, 47,203, 44,626, 43,957, 44,622, 45,941 and 47,199 [LG-23].
4. **The straight walk** (3.2) from the stalker's position to the seat's centre (−207.2, −137.9) starts at 43,968 and
   reaches 45,947's cell through 12 cells: 43,968, 44,635, 44,634, 44,633, 44,632, 45,294, 45,293, 45,292, 45,291,
   45,949, 45,948, 45,947; the walk to the vertex (3.3) from 43,968 to 45,947 is true [LG-22]. **Disputed in
   review:** the check computes 3.3 as stated above, not as the program does (see the dispute in 3.3); computed as the
   program does, the walk to the vertex 45,947 from 43,968 is true with the stalker's position as the start point (the
   same 12 cells as the straight walk) and with 43,968's centre as the start point. So the detail path's key
   points (CH-10 5.2) can join the start directly to the seat although the level path is a staircase.
5. As the stalker walks, its vertex follows it cell by cell (2.3): it stays 43,968 until the stalker leaves that 0.7 m
   cell, then becomes the cell vertex of each cell it enters.

## 8. Unestablished and believed

- That the callers of the walk to a point (3.2) compare the start vertex's own cell first; which creatures' movement
  uses 3.4 and 3.2 besides the stalker's detail path (3.6).
- When the memory of the last failed level path search is cleared (4.1).
- The callers and radius of the nearest-vertex search (4.2).
- The meaning of the patrol point flags and of link weights other than 1 (5), and the meaning of the four bytes of a
  multiplayer spawn point; whether a single-player game's state creation reads chunk 8192 (6.2).
- Everything here is static or read from the data: nothing was observed in the running game.

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binary `bin\xrGame.dll`
  (6,246,400 bytes).
- Data examined: the winning copies of `levels\<level>\level.ai` and `levels\<level>\level.game` of the 18
  single-player levels and of `spawns\all.spawn` (patrol paths, the record `esc_lager1`), as extracted for earlier
  approved specifications.
- Analysis: static analysis of `bin\xrGame.dll` (the full headless Ghidra decompilation made for earlier approved
  specifications, read-only headless instruction listings, an instruction listing of the whole binary, initial values
  read from the file): the graph's loading, the vertex record's readers, the accessibility bits and their writers, the
  online location update, the straight-line tests, the level path search and its limits, the nearest-vertex search,
  the patrol path readers of `level.game`. Readers of the data written from OS 3–4 and this spec, and a model of the
  searches and walks as specified here for the worked example. No debugging or execution; the game was not run.
- Checks: `lgr/checks.py` (with its helpers), run with `python3 -I`; all 23 pass.
- No leaked source code, derived projects or third-party documentation of the engine were used. Best-first search with
  a length and an estimate is the generic A* scheme (Hart, Nilsson and Raphael, 1968), and the search without an
  estimate Dijkstra's (1959); both names are used here only to describe what the binary computes.

## Gaps found in review

- The layout of chunk 8192 of `level.game` (6.2): each multiplayer spawn point is a chunk of its own inside it,
  numbered 0, 1, …, read in that order until the first missing number; the spec gives only a point's fields.
