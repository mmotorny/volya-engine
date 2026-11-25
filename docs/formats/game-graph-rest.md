# The rest of `game.graph`: trailing bytes and level points

Functional specification of the parts of `game.graph` that `docs/behavior/offline-simulation.md` leaves
open (S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition): the bytes after the level-point table
(offline-simulation 1.7), which level points each vertex holds and in which order (offline-simulation
1.5), and the one use the game makes of level points. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked
**believed**. Program behaviour was read from the shipped program without running it (**static**).

Builds on (not repeated here): `docs/behavior/offline-simulation.md` 1–3 (file layout of `game.graph`,
`levels\<level>\level.gct` and `levels\<level>\level.ai`; the terms game vertex, level vertex, own
level vertex, cross-table cell, level point); `docs/formats/db-archive.md` (winning copies).

Bracketed ids name the check that backs the statement next to them. All checks are in the dirty
workspace's `checks/game-graph-rest/` (`run.py` runs them all): `T…` in `tail.py`, `P…` in `points.py`,
`S…` in `scripts.py`, `B…` in `binary.py`.

## 1. How the game reads `game.graph`

Only the game library reads the file: among the shipped executables and libraries only `bin\xrGame.dll`
contains the name `game.graph`, once [B1]. The game opens `$game_data$` + `game.graph` once, holds the
whole file in memory, reads the header (1, 2, 4, 4 and 16 bytes: version, *V*, *E*, *P*, graph
identifier), the level count and the level records field by field, and from then on addresses the
vertex table in place, starting at the position the level records end [B2, B3]. Edges and level points
are reached only through a vertex's edges offset and level points offset (offline-simulation 1.3).

## 2. Trailing bytes

In the winning copy (`gamedata.dbb`, 8,128,486 bytes) the level-point table ends at offset 8,128,370 and
116 bytes follow: 29 `u32` values, each 4; the file has 29 level records [T1]. The losing copy
(`gamedata.db0`, identical in `gamedata.dba` [T2]; 8,997,755 bytes, version 8, 31 level records, 3,846
vertices, 9,561 edges, 438,863 level points) ends exactly where its level-point table ends: it has no
trailing bytes [T3].

The game never reads the trailing bytes: nothing in the header or the tables points past the level-point
table, and the program reaches level points only through vertices (section 1) [B2, B3, B4]. A reader
ignores them and need not check their length or values. A writer may write them as in the winning copy
(29 × `u32` 4) or omit them; the game loads either alike (static). Their meaning, and whether their
number follows the level count (29 in the winning copy), is **unestablished**: the program does not use
them and the one other copy has none.

## 3. Which level points a vertex holds

The level points were produced when the graph was built, by a tool that does not ship with the game; the
rule below is established from the data alone, by reproducing every level-point list of every vertex of
the 18 levels that ship with level data.

### 3.1 Candidates and count

For a game vertex *g* of a shipped level, its **candidates** are the level vertices of that level whose
cross-table cell (offline-simulation 2) names *g*, in increasing level-vertex index. With *n* candidates
(every vertex has at least one [P13]), the vertex holds

- all *n*, if *n* ≤ 10;
- otherwise floor(*n* / 10), but at most 255

level points [P2]. Each shipped case occurs (e.g. a vertex with 10 candidates holds 10 points, one with
11 holds 1, every vertex with 2,560 or more holds 255) [P2b]. The points of a vertex are distinct
candidates [P3]. A vertex's own level vertex is not guaranteed a place: 2,507 of the 2,792 vertices of
shipped levels do not hold it [P7].

### 3.2 Which candidates, in which order

The points are the first *count* entries of the candidate list after shuffling it with a pseudo-random
generator [P4]:

- **Generator**: a 32-bit state *x*. A draw sets *x* ← (214,013 · *x* + 2,531,011) mod 2³² and yields
  floor(*x* / 65,536) mod 32,768 (a value 0 … 32,767). Bit 31 of the state never affects a draw, so any
  initial state congruent to it modulo 2³¹ yields the same draws [P6].
- **Shuffle** of a list of *n* entries (positions 0 … *n* − 1): for *k* = 1, 2, …, *n* − 1 in that order,
  draw *r*, let *j* = *r* mod (*k* + 1), and exchange the entries at positions *k* and *j*. A list of one
  entry takes no draw. The draws suffice because no vertex has 32,768 or more candidates (the largest has
  10,685) [P5]; how longer lists would be shuffled is **unestablished**.
- **One stream**: a single generator, with initial state 1, serves all vertices in increasing vertex
  index, starting at vertex 0; nothing else draws from it between vertices. Its first five draws are 41,
  18,467, 6,334, 26,500, 19,169 [P11].
- The vertex's level points, in file order, are entries 0 … *count* − 1 of its shuffled list; each
  point's position, level vertex and distance are those of that level vertex (offline-simulation 1.5).

This reproduces the level-point list, in order, of each of vertices 0 … 2,791 (all vertices of the 18
shipped levels, which precede all vertices of the 11 levels without data [P1]) [P4]. The vertices
2,792 … 3,511 of the 11 levels without data cannot be checked, because their cross tables do not ship;
whether the same stream continues over them is **unestablished** (believed: it would, since nothing
breaks it between shipped levels).

## 4. Use of level points in the game

The game reads level points in one place only [B4]: placing an object that the simulation's script
method `kill_entity` kills. The method takes an object, optionally a game vertex and optionally a killer
object; `scripts\lua_help.script` lists three forms [S2], and the one-argument form uses the object's own
game vertex and no killer [B5b]. No other part of the game calls it [B6]. No shipped script reaches it:
its one call (`scripts\task_level4.script`) is inside a function that no code line of any script calls,
and no configuration file names it [S1].

Without a killer, with game vertex *g* (the given one, or the object's own) [B5, B5b]:

- If *g* has *m* level points, the game advances the simulation's generator, *y* ← (134,775,813 · *y* + 1)
  mod 2³², and takes level point *i* = floor(*m* · *y* / 2³²) of *g* (0 ≤ *i* < *m*). With no level
  points, no draw is made and point 0 is taken [B5c] (no vertex of the shipped graph has none, offline-simulation 1.5).
  **Disputed in review:** "point 0" is misleading for an empty list: the game reads the 20-byte record at
  the vertex's level points offset, which is not a level point of that vertex (it belongs to whatever
  follows in the table).
- The object's game vertex becomes *g*; its position, level vertex and distance (offline-simulation 5.1)
  become the point's position, level vertex and distance.
- If *g* is on the current level and the point's level vertex is not a valid level vertex of that level,
  the game stops with "Invalid vertex" (never the case in the shipped data: every level point's level
  vertex is a candidate of its vertex, hence a level vertex of its level [P3]).

The generator state *y* is set when the simulation is created, to the low 32 bits of the system's
high-resolution performance counter at that moment [B7], so the chosen point cannot be predicted. Which
other operations of the simulation draw from the same state, and whether anything resets it later, is
**unestablished** here (proposal OS-3 covers the simulation's random generator).

With a killer of certain kinds, the game takes the location from data other than `game.graph` [B5c];
which kinds and which data is **unestablished** here. The rest of what `kill_entity` does to the object and
the simulation (its death, other fields it changes) is outside this specification (script API, #119).

## 5. Verification facts

- `game.graph` (`gamedata.dbb`): level points end at 8,128,370, then 116 bytes = 29 × `u32` 4 [T1].
- `l01_escape` (vertices 0 … 251): 595,499 candidates in all (every level vertex of the level is a
  candidate of exactly one vertex), 40,733 level points, sum of their level vertices 12,381,792,924,
  SHA-256 of their level vertices as consecutive little-endian `u32` in file order, first 16 hex digits
  `6a04e9fbd92c0b3d`; 595,247 draws; generator state after its last vertex 3,015,589,574 [P8].
- Vertex 0: 256 candidates, 25 level points, the first five with level vertices 594,070, 594,338,
  593,567, 594,262, 594,335 [P9].
- Vertices 0 … 2,791: 3,804,811 draws in all; state after vertex 2,791: 995,072,706 [P10].
- Generator state before the first vertex of each shipped level (useful to test one level alone) [P12]: `l01_escape` (vertex 0) 1; `l02_garbage` (252) 3,015,589,574; `l03_agroprom` (416)
  3,593,363,219; `l03u_agr_underground` (703) 3,300,848,481; `l04_darkvalley` (811) 1,693,440,857;
  `l04u_labx18` (1,109) 192,814,002; `l05_bar` (1,168) 2,036,840,988; `l06_rostok` (1,308)
  2,927,415,477; `l08_yantar` (1,438) 2,352,967,514; `l08u_brainlab` (1,529) 192,867,454;
  `l07_military` (1,545) 1,826,352,438; `l10_radar` (1,862) 1,602,972,751; `l11_pripyat` (2,117)
  4,280,588,293; `l12_stancia` (2,273) 2,592,710,578; `l12u_sarcofag` (2,402) 3,033,686,812;
  `l12u_control_monolith` (2,467) 1,074,436,413; `l12_stancia_2` (2,517) 3,381,655,945; `l10u_bunker`
  (2,661) 2,387,900,056.

## 6. Unestablished

- The meaning of the trailing bytes and whether their number follows the level count (2).
- How a candidate list of 32,768 or more entries would be shuffled (3.2).
- Whether the shuffle stream continues over the vertices of the 11 levels without data (3.2).
- Which other operations draw from the simulation's generator used in 4, and whether it is reset later.
- Where a killer of which kinds places the killed object (4).
- Everything in sections 1, 2 and 4 is static: none of it was observed in the running game.

## 7. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries in `bin\`, chiefly `bin\xrGame.dll`.
- Data examined: `game.graph` (all three copies: `gamedata.db0`, `gamedata.dba`, `gamedata.dbb`),
  `levels\<level>\level.gct` and `levels\<level>\level.ai` of all 18 shipped levels, all
  `scripts\*.script` and `config\` files (winning copies).
- Analysis: static analysis of `bin\xrGame.dll` (Ghidra 12.2, headless; disassembly and decompilation) of
  the graph reader, every instruction that loads a vertex's level-point count, the code that places a
  killed object and the simulation's construction; a byte search of all shipped executables and
  libraries. Data analysis by throwaway decoders written from the approved specifications: comparison of
  each vertex's level points with its cross-table candidates; the shuffle and generator of 3.2 were
  found by searching all 2³² generator states against the point lists of five consecutive small
  vertices and then confirmed on every vertex of every shipped level.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Whether the simulation's generator state of section 4 is stored in a save and restored on load, or
  seeded anew when a saved game is loaded, is not stated.
