# Stalker movement

Behaviour specification CH-10 of `docs/behavior/characters-scope.md` (cited as `scope`; its sections 1.2, 2.2, 5 and
8, with its "Gaps found in review", neither of which concerns this part), for S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): how the movement state a stalker is given (a destination level vertex, the path type, a desired
position and direction, the body state, mental state and movement type, and the sight setting) makes it walk and
turn. It covers the speeds of `[stalker_movement_speeds]` and the travel table built from them, the current states
the stalker reports, the **detail path** (the smooth trajectory laid over the level path), walking along it, turning
of the body and the head (the sight types, `max_left_torso_angle`, `max_right_torso_angle`) and the turning of the
head, shoulder and spine bones (`bone_head`, `bone_shoulder`, `bone_spin`), and the keys `bone_model` and
`bone_torso` that `scope` 8 gives this part.

Builds on (not repeated; cited by name and section):

- `docs/behavior/script-api-movement-sight-and-memory.md` (cited as **X**): what scripts set and read. Its 3: the
  **movement request** (target body state, movement type, mental state, path type, detail path type, desired position
  and direction with their "set" marks, destination vertex) and the **current** states the movement update moves
  toward it, with their numbers (`move.crouch` 0, `move.standing` 1; `move.walk` 0, `move.run` 1, `move.stand` 2;
  `anim.danger` 0, `anim.free` 1, `anim.panic` 2; `move.line` 0, `move.dodge` 1, `move.criteria` 2); its 4.2
  (`location_on_path`, `extrapolate_length`, `movement_enabled`); its 6 (the **sight request**: sight type, vector,
  object and the flags *A*, *B*, *C*; the sight types `eSightType…` 0 … 10 and the scripts' `look` constants).
- `docs/behavior/script-api-game-object-basics.md` (**M**): `is_body_turning()` (its 5.5).
- `docs/behavior/script-api-level.md` (**N**) 3.3: `level.vertex_in_direction`, used by the worked example.
- `docs/behavior/offline-simulation.md` (**OS**) 3: `level.ai`'s header and vertex positions, the cell of a level
  vertex (side *c* = 0.7, centred on the vertex's x and z).
- `docs/formats/ltx-config.md`: reading a missing key is fatal.
- `docs/behavior/stalker-planner.md` (CH-17) and `docs/behavior/smart-terrain-jobs.md` (CH-28): the planner and the
  campfire job of the worked example.
- Not approved, cited by name: the **level graph** CH-8 (the level vertices, their links, cells and planes) and
  **paths and restrictions** CH-9 (the level path search over level vertices, game and patrol paths, accessibility,
  arrival); the bodies of living creatures PH-22 (`physics-scope`); stalker motions CH-11; script control CH-24.
  Where this spec says "the level path" it means CH-9's sequence of level vertices from the stalker's vertex to the
  destination vertex; "walkable" and "the straight walk" mean CH-8's line tests over the level graph (a segment
  followed from cell to cell through linked level vertices, as N 3.3 describes for one such walk).

Conventions:

- **Stalker**: an object of the class id `AI_STL_S` (`scope` conventions). The scripts' names of states are used:
  body state standing / crouch, movement type walk / run / stand, mental state danger / free / panic.
- **Heading** (yaw) *h* of a horizontal direction (*dx*, *dz*) is the angle with (*dx*, *dz*) ∝ (sin *h*, cos *h*),
  that is atan2(*dx*, *dz*); the stalker's world rotation is the rotation by its body heading about the vertical axis,
  with the forward axis (sin *h*, 0, cos *h*) [SM-13e]. **Pitch** of a direction (*dx*, *dy*, *dz*) is
  −atan(*dy* / √(*dx*² + *dz*²)) (positive looks down) [SM-13c]. Angles are radians; "normalised" means brought into
  [−π, π].
- **Update** is the stalker's scheduled AI update (when it is alive, after its decision planner, CH-17); **frame** is
  each rendered frame of the online stalker, with *dt* its duration in seconds (believed: the per-frame update of
  creatures). Both are static except where marked.
- Each fact is **static** (from static analysis of the shipped `bin\xrGame.dll`, 6,246,400 bytes; nothing was run),
  **measured** (from game data or the shipped scripts' text), **believed** (with the reason) or **unestablished**.
  Every fact carries a check label [SM-*n*] of `checks/stalker-movement/checks.py` in the dirty workspace.

## 1. Keys

### 1.1 `[stalker]` and `[stalker_movement_speeds]` (measured)

`[stalker]` has `movement_speeds = stalker_movement_speeds`, `max_left_torso_angle = 45`,
`max_right_torso_angle = 30`, `bone_head = bip01_head`, `bone_shoulder = bip01_spine2`, `bone_spin = bip01_spine1`
and `bone_model = bip01` [SM-1, SM-1b]. `[stalker_movement_speeds]` (metres per second) [SM-1]:

| Key | Value | Key | Value | Key | Value |
|---|---|---|---|---|---|
| `danger_crouch_walk_forward` | 1.1 | `danger_crouch_run_forward` | 2.5 | `danger_stand_walk_forward` | 1.8 |
| `danger_crouch_walk_backward` | 1.05 | `danger_crouch_run_backward` | 2.5 | `danger_stand_walk_backward` | 1.8 |
| `danger_crouch_walk_left` | 1.05 | `danger_crouch_run_left` | 2.4 | `danger_stand_walk_left` | 2.0 |
| `danger_crouch_walk_right` | 1.05 | `danger_crouch_run_right` | 2.4 | `danger_stand_walk_right` | 2.0 |
| `danger_stand_run_forward` | 3.6 | `danger_stand_run_backward` | 3.0 | `danger_stand_run_left` | 2.0 |
| `danger_stand_run_right` | 2.0 | `free_stand_walk_forward` | 1.1 | `free_stand_run_forward` | 3.0 |
| `panic_stand_run_forward` | 7.0 | | | | |

### 1.2 How they are read (static)

- When a stalker's section is loaded, the section named by its `movement_speeds` is read: all 19 keys above, each
  without a presence test (so a missing one is fatal, `ltx-config`) [SM-3]. The 19 values are read once per section
  name and shared by every stalker naming it [SM-3].
- `max_left_torso_angle` and `max_right_torso_angle` are read in degrees, each only when present, else 90 and 60
  [SM-13d].
- `bone_head`, `bone_shoulder` and `bone_spin` name the bones turned with the head (section 6) [SM-14].
- `bone_model` and `bone_torso` are not named in `xrGame.dll` [SM-2]; no reading of them by the stalker was found
  (**believed** not read; `bone_torso` occurs only in monster sections, CH-12's).

## 2. The travel table (static)

A stalker's movement is planned and followed with **travel entries**. Each has a **travel code** (bits below), a
**speed** (metres per second), a **path turn rate** *w* (radians per second; the trajectory's turning circles have
radius speed / *w*) and a **body turn rate** (how fast the body heading follows, section 6) [SM-4, SM-4b, SM-11b].

Code bits: 0x01 stand, 0x02 walk, 0x04 run; 0x08 standing, 0x10 crouch; 0x40 danger, 0x80 free, 0x100 panic;
0x80000000 forward, 0x40000000 a second direction (**believed** backward) [SM-4, SM-7]. The table has 17 entries
[SM-4, SM-4b]:

| Code | States | Speed | Path turn rate | Body turn rate |
|---|---|---|---|---|
| 0x89 | free, standing, stand | 0 | π/4 | 2π |
| 0x109 | panic, standing, stand | 0 | 2π | 2π |
| 0x49 | danger, standing, stand | 0 | 2π | 2π |
| 0x91 | free, crouch, stand | 0 | 2π | 2π |
| 0x111 | panic, crouch, stand | 0 | 2π | 2π |
| 0x51 | danger, crouch, stand | 0 | 2π | 2π |
| 0x8000008a | free, standing, walk | `free_stand_walk_forward` | π/8 | π/4 |
| 0x8000008c | free, standing, run | `free_stand_run_forward` | π/16 | π/8 |
| 0x8000004a | danger, standing, walk | `danger_stand_walk_forward` | 100π | 2π |
| 0x80000052 | danger, crouch, walk | `danger_crouch_walk_forward` | 100π | π |
| 0x8000004c | danger, standing, run | `danger_stand_run_forward` | 100π | 2π |
| 0x80000054 | danger, crouch, run | `danger_crouch_run_forward` | 100π | 2π |
| 0x8000010c | panic, standing, run | `panic_stand_run_forward` | π/16 | π/8 |
| 0x4000004a, 0x40000052, 0x4000004c, 0x40000054 | as the four danger entries | as those | 100π | as those |

So only 7 of the 19 speeds are used; the `backward`, `left` and `right` keys are read but enter no entry [SM-3,
SM-4]. The four 0x4000… entries are never selected: every mask of section 3.2 has the bit 0x80000000 or neither top
bit [SM-7]. There is no moving entry for free crouching, panic walking or panic crouching movement: such a stalker
has only stand entries as candidates (5.1) (**believed**: it cannot travel) [SM-4, SM-11].

## 3. The update (static)

### 3.1 Order

At each update of a stalker whose movement is enabled (X 4.2), in this order [SM-6]:

1. The movement request is copied into a **working state** (the target body state, movement type, mental state,
   path type, detail path type, desired position and direction with their marks) [SM-6].
2. When the working movement type is not stand: the path settings (3.3) [SM-6].
3. If the stalker is under script control (CH-24) and its current scripted action has a speed of at least 10⁻⁷ in
   size, that speed becomes the stalker's speed and the update ends here [SM-6b].
4. When the working movement type is not stand: the travel masks (3.2), then the path pipeline (section 4) [SM-6].
   **Disputed in review:** steps 2 and 4 also run with movement type stand while a further flag of the stalker's
   movement is on; some of the decision planner's own actions (CH-17) switch it on together with a stand request
   and off when they finish (which actions, and what the path does then, is unestablished).
5. The current states and speeds (3.4), then the run test (3.5) [SM-6].

The request's defaults, before any script sets it: standing, stand, danger, path type 3 (no path), detail type line,
desired position and direction unset; body turn rate 2π, head turn rate 3π/2 [SM-5]. The path types are
`game_object.game_path` 0, `level_path` 1, `patrol_path` 2, `no_path` 3 (the listing, measured) [SM-16].

### 3.2 Travel masks

From the working states a **mask** is formed: 0x80000000, plus 0x08 standing or 0x10 crouch, plus 0x40 danger,
0x80 free or 0x100 panic, plus 0x02 walk or 0x04 run; for stand both top bits are cleared and 0x01 is set [SM-7].
Then [SM-7]:

- danger: **allowed** = mask, **wanted** = mask | 0x01;
- free or panic: allowed = mask | 0x01, wanted = mask | 0x03, and **shortest choice** is switched on (it stays on until
  the stalker's movement is reinitialised; danger does not switch it off).

A change of either mask marks the detail path to be rebuilt [SM-7].

### 3.3 Path settings

- With path type game (0) or patrol (2), the target's desired-position mark is cleared: a desired position is not
  used with those path types [SM-6c].
- The detail path type and whether the desired direction is set pass to the detail path (section 5) [SM-6c]; the
  detail path type changes nothing (5.1) [SM-11e].
- With the desired position set, the destination vertex becomes the level vertex of that position (CH-9 for
  accessibility), and the detail path ends at the position; otherwise (level path) it ends at the destination
  vertex's position, moved to an accessible one when the restrictions forbid it (CH-9) [SM-6c, SM-10b].

### 3.4 Current states and speeds

Each path point carries a travel code (5.4). With the working movement type not stand and an up-to-date, unfinished
detail path, let *e* be the entry of the stalker's current point [SM-8]:

- *e* has speed 0 (a stand point): in free, the body and head targets become the heading of the current segment
  and the head turn rate becomes the body turn rate in force; if the body heading is then not within 0.001 of the
  segment heading and the point is new, *e* is kept. Otherwise (and always in danger or panic) the entry of the
  next point is used [SM-8, SM-8b].
- *e* moves, in panic: if the body heading is more than π/16 from the segment heading, the danger entry with the same
  other bits is used instead [SM-8].
- *e* moves, in free: if the body heading is more than π/16 from the segment heading, the stalker stops to turn:
  speed 0 and body turn rate π, with body and head targets on the segment heading (as above) and the standing
  torso rule (6.3) off [SM-8, SM-8b].
- in danger, *e* is used as it is [SM-8].

The chosen entry gives the stalker's speed and body turn rate, and its code the **current** states: body standing if
the code has 0x08 (else crouch), mental danger / free / panic from 0x40 / 0x80 / else, movement stand if the code's
low bits are 0x01, walk if 0x02, else run [SM-8]. Head turn rate: in free, π/2 while the standing torso rule is on
(it is switched on at every update and off only by the cases above); in danger or panic 3π/2 [SM-8b].

Without such a path, or with movement type stand: speed 0, body turn rate 2π in danger and π/2 otherwise, head turn
rate as above, current movement type stand; the current body and mental states are the working (target) ones
[SM-8]. So a stalker told to stand stops at once, wherever it is on its path [SM-6, SM-8].

### 3.5 The run test

A stalker whose current movement type is run in danger looks ahead along an unfinished path that has at least two
more points: with the current segment's direction as reference (point *i* to *i*+1; (0, 0, 1) when shorter than
0.001), it adds up the distance from the stalker to point *i*+1 and then the later segments one by one; if a segment
of length at least 0.001 turns by more than 3π/8 from the reference before the sum reaches 2 m, the current movement
type becomes walk [SM-9]. Only the reported state changes (the speed was set in 3.4) [SM-9].

## 4. The path pipeline (static)

By path type: none (3) builds nothing; game (0), level (1) and patrol (2) run CH-9's pipeline of the game path, level
path and patrol path, each ending in the building of the detail path (section 5) [SM-10]. For the level path
[SM-10b]:

1. The level path from the stalker's level vertex to the destination vertex is searched (CH-9), at once or later
   (**believed**: deferred when the update's time budget is spent).
2. The detail path is set up: its start is the stalker's position and the direction of its body heading; its
   extrapolation flag is off (for game paths it is on; for patrol paths CH-9's) [SM-10b].
3. The detail path is built (section 5); if the build fails, the pipeline starts again at step 1 [SM-10b, SM-11e].
4. While both paths are up to date it is followed; when the detail path is finished and the level path too, the
   path-building state becomes 8 (X 4.1's `path_completed`) [SM-10b].

A change of the destination vertex, the masks, the desired position or direction, the restrictions or the stalker
having been moved while disabled (X 4.2) marks a path to be rebuilt, which restarts the pipeline [SM-6c, SM-7,
SM-10].

## 5. The detail path (static)

### 5.1 Start, end and candidates

- **Start**: the stalker's position and the direction of its body heading; **end**: the destination position (3.3)
  and, with the desired direction set, its x–z direction, otherwise (0, 1); both directions are normalised, and one
  with squared length below 0.001 becomes (0, 1) [SM-11, SM-10b]. A start or end position that does not lie in its
  level vertex's cell is moved 0.001 m toward that cell's centre [SM-11f].
- **Candidates**: the travel entries whose code has no bit outside the wanted mask; of those whose code also has no
  bit outside the allowed mask, the fastest and the slowest are noted [SM-11]. For the departure every candidate is
  tried; for the arrival, every candidate when the desired direction is set, otherwise a single entry of speed 0 and
  turn rates 2π (the arrival direction then does not matter) [SM-11]. With no candidate the build fails [SM-11].
- The detail path type (X 3) is read but every value builds the same path [SM-11e].

### 5.2 Key points

The level path's vertices are reduced to **key points**: from the start, a binary search over the remaining
vertices finds the farthest one the straight walk from the current key point reaches; it becomes the next key
point, until the destination vertex (whose key point is the end position) [SM-11f]. Then each inner key point *B*
with neighbours *P* and *Q* is slid along *BQ* toward the point of that line equidistant from *P* and *Q* (the cosine
of the angle at *Q* clamped to ±0.99999), as far as a binary search (to 0.01 of the way) keeps the lines from *P* and
to *Q* walkable; this is done once toward the next and once toward the previous neighbour, and the one of the two
positions that makes the turn at *B* gentler is kept (**believed**: the comparison is of the cosines of the two turns)
[SM-11f].

### 5.3 Segments between key points

Each stretch from one key point to the next is a turn–straight–turn curve. Its departure state is the key point
with the direction of arrival there (at the start, the stalker's direction); its arrival state is the next key point
with the direction of the following stretch (at the end, the end direction) [SM-11, SM-11b]. For every pair of a
departure candidate and an arrival candidate [SM-11b, SM-11c]:

- each state gets two turning circles, left and right of its direction, of radius speed / path turn rate (a speed of 0
  gives radius 0: turning on the spot);
- for each of the four pairs of circles, the common tangent leaving the first circle and reaching the second in
  their turning senses is taken (when it exists), giving a first arc, a straight piece and a second arc;
- the curves are ordered by their **time**: straight length / speed of the fastest allowed candidate + |first arc
  angle| / departure path turn rate + |second arc angle| / arrival path turn rate; the first, in that order, whose
  arcs and straight piece pass the level graph is the pair's curve [SM-11b].

Without shortest choice the first pair (in table order) that yields a curve is used; with it, the pair of least time
[SM-11c]. **Disputed in review:** the order is not the table's as listed in section 2. The entries are kept sorted
by code (ascending, unsigned); the candidate list puts the entries that fit the allowed mask first, in descending
code order, followed by the other candidates in ascending code order, and pairs are tried with the departure
candidate in the outer loop and the arrival candidate in the inner one. So a danger stalker (no shortest choice)
tries its moving entry before its stand entry. If no pair yields a curve for some stretch, the whole detail path is discarded and the build fails
[SM-11c, SM-11g]. A negative speed
would reverse the direction; no entry has one [SM-11c, SM-1].

### 5.4 Points

- An arc of length *L* above 0.1 m and angle *θ* is sampled at *n* = min(⌊10 *L* + 1.5⌋, ⌊10 |*θ*| / *w* + 1.5⌋)
  equal angular steps, each sample followed through the level graph from the previous one (the arc fails when a
  sample leaves it); a shorter arc is a single point [SM-11d]. The straight piece is followed by the straight walk,
  with a point where it crosses into each next vertex (**believed** from the walk's structure) [SM-11d].
- Points of the first arc carry the departure entry's code, the straight piece the fastest allowed entry's, the second
  arc the arrival entry's (**believed**: the code is the one passed while each part is built) [SM-11b].
- Each point holds its position, its level vertex and its code; its height is the height of its level vertex's plane
  (CH-8) at the point's x and z [SM-11g]. The number X 4.2's `location_on_path` returns with each point is that
  point's level vertex (static: the value is the point's vertex field, and the one returned without a path is the
  stalker's level vertex) [SM-11g].
- With the extrapolation flag on, the path is extended past its last point along the last segment by the extrapolate
  length (X 4.2; 8 by default), and the path counts as finished at the last point before the extension [SM-11g,
  SM-10b].
- A built path is used only if its last point is within 10⁻⁷ (in x and z) of the end position; it is then current
  from point 0 [SM-11e]. **Disputed in review:** the test is that the squared x–z distance is below 10⁻⁷; with the
  extrapolation flag on, the point tested is the last one before the extension. A failure writes a message with the start and end positions to the log [SM-11e].

## 6. Walking and turning

### 6.1 Walking (static)

Each frame (*dt* in seconds; nothing below 10⁻⁵ s), when movement is enabled, the path up to date and not finished,
and the speed *v* (3.4) not 0 [SM-12]:

1. Points already passed are skipped: while the stalker is farther from the current point than the segment to the
   next is long and also farther from it than from the next, the next becomes current [SM-12].
2. The stalker advances by *v*·*dt* along the points, taking each point it reaches; within 0.001 m of the last point it
   has arrived (speed 0, the last point current) [SM-12]. **Disputed in review:** the 0.001 m test is on the length
   of the last hop the step considered (from the position before it to the point it was heading for), not on the
   distance left to the path's last point. When the step reaches the last point by a hop of 0.001 m or more, the last
   point becomes current and the rest of the step continues along that hop's direction past it (step 3), after which
   the finished path stops the stalker (speed 0); a step that does not reach the next point while that point is less
   than 0.001 m away also makes the last point current, wherever it is on the path.
3. Otherwise it moves toward the next point by the rest: its velocity is the unit direction to that point with the
   vertical part limited (above 0.9: 0.8, below −0.9: −0.8, renormalised), times *v*; the body (PH-22) moves it,
   considering objects within 2 m (2.5 m when its body is active), and its position is the body's [SM-12].
4. The speed it reports (X, CH-11) is the average of *v* and the distance actually covered divided by *dt* [SM-12].
   **Disputed in review:** the distance averaged is the planned one (the hops taken plus the rest of the step), not
   the body's actual movement; it always equals *v*·*dt*, so the reported speed is *v* while walking.

### 6.2 Head and body targets by sight type (static)

At each update the sight request (X 6.1) sets the **head target** (heading and pitch) and for some types the **body
target** [SM-13]:

| Type | Head target | Body target |
|---|---|---|
| 0 current direction | the head's current heading and pitch | — |
| 1 path direction | heading of the current path segment, pitch 0; past the path's end, the heading of the stalker's last move; if it has not moved, the current head | the head target |
| 2 direction | heading and pitch of the vector | — |
| 3 position | heading and pitch from the stalker's eye point to the position | — |
| 4 object | toward the object (flag *C* set: pitch 0) | — |
| 9 fire object | toward the object by a rule of its own (**unestablished** in detail) | — |
| 5 cover, 8 cover look-over | look-around rules over the level vertex's cover values; look-over alternates on a 3,500 ms cycle (**unestablished** in detail) | — |
| 6 search | heading by the cover rule (**unestablished** in detail), pitch π/4 | — |
| 7 look-over | the type's entry is empty (**believed**: never set by the scripts; it would fail) | — |

[SM-13, SM-13b, SM-13c, SM-11g]. Flag *B* is read only by the cover and search rules [SM-13]. Flag *A* (X: "turn the torso only", believed) chooses which bones carry the head's turn
(6.4); for type 3 with *A* set, a position less than √0.1 m (in x–z) from the eye point is looked at from the
stalker's own position instead [SM-13c, SM-14].

### 6.3 The standing torso rule (static)

When the stalker's reported speed is below 10⁻⁷ and the rule is on (3.4), at each update [SM-13d]:

- not turning: let the side be given by sin(body heading − head heading) > 0 (then the limit is
  `max_right_torso_angle`) or not (`max_left_torso_angle`); if the head is within that limit of the body, the body
  target stays the body's heading; otherwise the stalker starts turning, with the head's current heading as body
  target;
- turning: when the body heading is within 0.001 of the head target, turning stops (body target = body heading);
  else the body target is the head target.

While moving, turning stops [SM-13d]. A path-direction sight then sets the body target again (6.2) [SM-13b].

### 6.4 Each frame (static)

Each frame, unless the stalker is in a state that suspends it (**unestablished** which), the four angles move
toward their targets: body heading and pitch at the body turn rate, head heading and pitch at the head turn rate
(π/16 instead while a cover look-over is running) [SM-13e]. A step moves by at most rate·*dt*, in the shorter
direction; within that it snaps to the target [SM-13e]. Before that, if the body target and the head lie on opposite
sides of the body heading and the two angles from it add up to more than 3.14059, the body target becomes the head's
heading (so the body turns the way the head looks) [SM-13e]. The stalker's world rotation is then set from the body
heading alone [SM-13e]. (M's `is_body_turning()` compares these headings with their targets.)

At spawn, body and head headings (current and target) are minus the creature's yaw from its server object (believed:
the creature update's yaw, `sav` 7.4.1), normalised, and the pitches 0 [SM-15].

### 6.5 Bones (static)

The bones named by `bone_head`, `bone_shoulder` and `bone_spin` are turned after the animation by Δ = (head heading +
an extra heading from the stalker's aiming, **unestablished**, zero when inactive) − body heading, normalised, and
by the head pitch (plus the extra pitch) [SM-14]:

| Flag *A* of the running sight | Head bone (heading, pitch share) | Shoulder and spine bones (each) |
|---|---|---|
| not set | 0.5, 1 | 0.25, 0 |
| set, or no sight running | 0, 0 | 0.5, 0.5 |

So the shares add up to one in heading and in pitch either way [SM-14]. The rotation is applied in the bone's own
frame after its animated rotation (**believed** from the matrix order).

## 7. Worked example: a novice at the beginners' village campfire

On a New Game, `esc_lager1` (record 93 of `spawns\all.spawn`, a stalker with the profile `esc_stalker_novice`) is at
(−209.067, −19.751, −132.083) on `l01_escape` [SM-17b]. Take it given the campfire job (CH-28, CH-17 8): its scheme
`kamp1@esc_lager` has `center_point = camp_center`; `esc_lager_camp_center` is one point without flags at level vertex
45,944, (−207.2, −19.972, −140.0) [SM-17]. The scripts (measured) [SM-16]:

- The camp has eight sectors around the centre, directions (1,0,0), (1,0,1), (0,0,1), (−1,0,1), (−1,0,0), (−1,0,−1),
  (0,0,−1), (1,0,−1); none is blocked (no flagged points); one free sector is taken at random. The seat is
  `level.vertex_in_direction` from 45,944 in that direction, `radius` 2 m (the default), then moved again by
  `math.random(0, 0.5)` metres in a random direction; the look target *pp* is `level.vertex_in_direction` from 45,944
  by `math.random(0, 0.5)` metres (**believed**: 0 m, the runtime's integer form, so *pp* = 45,944 and the seat is the
  2 m vertex).
- "Go to position": destination vertex = the seat, desired direction = *pp* − seat, path type `level_path`, state
  `walk` (`def_state_moving`): walk, free, standing; no look position, so the sight is `look.path_dir` (type 1).
- On the seat vertex, "wait": a state such as `sit` (stand, free, standing, with an animation) with look position
  *pp*: `set_sight(look.direction, dir, true)` with *dir* = *pp* − position, its y set to 0.

The eight 2 m seats (believed: N 3.3's walk reaches the cell containing the target, every cell on the way having a
vertex) [SM-17c, SM-17d]:

| Sector | Seat vertex | Position | Heading toward 45,944 |
|---|---|---|---|
| (1,0,0) | 47,843 | (−205.1, −19.99, −140.0) | −90° |
| (1,0,1) | 47,203 | (−205.8, −19.95, −138.6) | −135° |
| (0,0,1) | 45,947 | (−207.2, −19.9, −137.9) | 180° |
| (−1,0,1) | 44,626 | (−208.6, −19.9, −138.6) | 135° |
| (−1,0,0) | 43,957 | (−209.3, −19.96, −140.0) | 90° |
| (−1,0,−1) | 44,622 | (−208.6, −20.03, −141.4) | 45° |
| (0,0,−1) | 45,941 | (−207.2, −20.06, −142.1) | 0° |
| (1,0,−1) | 47,199 | (−205.8, −20.04, −141.4) | −45° |

The movement, for the seat 45,947 (6.2 m from the stalker) [SM-7, SM-11, SM-18]. **Disputed in review:** 6.2 m is
the distance to the 2 m sector target; the seat vertex's position is 6.1 m (in x–z) from the stalker.

1. Masks: mask 0x8000008a; allowed 0x8000008b, wanted 0x8000008b, shortest choice on. Candidates: 0x89 (speed 0,
   path turn rate π/4) and 0x8000008a (1.1 m/s, π/8: turning radius 2.80 m); the fastest allowed is 0x8000008a.
2. The level path runs from the stalker's vertex to 45,947 (CH-9); the key points follow (5.2).
3. The end direction is the desired direction, (0, −2.1) normalised to (0, −1), heading 180°; with it set, both
   candidates may arrive. Every departure–arrival pair is tried and the curve of least time kept (5.3): a departure
   with 0x89 turns on the spot (π/4 per second of time), one with 0x8000008a curves with radius 2.80 m; likewise at
   the seat. Which pair wins depends on the stalker's starting heading and the level graph (**unestablished** here).
4. Walking: on 0x8000008a points at 1.1 m/s, body turn rate π/4; on a stand point it first turns (3.4). The sight
   (path direction) keeps head and body targets on each segment's heading.
5. As soon as its level vertex is the seat, the scheme's "wait" sets movement type stand: speed 0 at once, so the
   stalker stops where it entered the seat's cell, the 0.7 m square around the seat (the scheme's test is on the
   vertex; **believed**: the test runs at the planner's update, CH-17) [SM-6, SM-8].
6. Seated: head target heading of *dir* (about 180°, toward *pp*), pitch 0, at π/2 per second; since the path ended
   along the desired direction the body already faces close to it, and the torso rule turns the body only if the head
   is more than 45° or 30° (by side) off; flag *A* is set, so the shoulder and spine bones carry the head's turn,
   half each (6.5).

## 8. Unestablished and believed

- Which of the 0x4000… entries' bit means backward, and why they exist (2).
- The look-around rules of the sight types 5, 6 and 8 (6.2); the state that suspends the per-frame turning (6.4); the
  extra aiming heading and pitch of the bones (6.5); the frame of the bone rotation (6.5).
- The codes of straight-piece points and the point placement of the straight walk (5.4); the comparison choosing a
  corner position (5.2).
- That the frame update runs every rendered frame (conventions); the spawn heading's source field (6.4).
- Patrol paths' extrapolation flag and the game and patrol pipelines (CH-9).
- Everything here is static or read from the data: nothing was observed in the running game.

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binary `bin\xrGame.dll`
  (6,246,400 bytes).
- Data examined: the winning copies of `config\`, `scripts\` and `spawns\all.spawn` as extracted for the approved
  `pda-scope` (`config\creatures\m_stalker.ltx`, `xr_kamp`, `state_lib`, `state_mgr_movement`, `state_mgr_bodystate`,
  `state_mgr_mental`, `state_mgr_direction`, `lua_help.script`), `levels\l01_escape\level.ai`, the patrol paths of
  `all.spawn`, the record `esc_lager1`.
- Analysis: static analysis of `bin\xrGame.dll` (the full headless Ghidra decompilation made for earlier approved
  specifications, read-only headless instruction listings, run-time type information, initial values read from the
  file): the stalker's movement set-up, reload, update, travel table, path pipeline, detail path building, walking,
  sight control, per-frame turning, bone callbacks and spawn. No debugging or execution; the game was not run.
- Checks: `checks/stalker-movement/checks.py` (with the `stalker-planner`, `characters-scope` and `script-api-level`
  helpers), run with `python3 -I`; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used. The turn–straight–turn
  construction is the generic shortest-path curve for a vehicle with a minimum turning radius (Dubins, 1957), used
  here only to name what the binary computes.

## Gaps found in review

- 5.3: after the common tangent of a pair of circles is found, two further tests decide whether that curve is kept;
  what they test is not given (believed: that the tangent points lie on the level graph).
- 5.1 and 5.3: the arrival rule (all candidates only with the desired direction set) applies to the last stretch
  only; at every inner key point all candidates are tried as arrival states.
- 5.4: after each stretch is built, a last point whose x–z distance to the point before it is below 10⁻⁷ is
  dropped; the spec does not say so.
- 3.4: at a stand point with no next point on the path, the stand point's own entry is kept in every mental state.
