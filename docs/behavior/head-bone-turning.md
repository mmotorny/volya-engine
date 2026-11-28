# Head bone turning

Behaviour specification TC4 of the trader-and-crows scoping specification
(`docs/behavior/trader-and-crows-scope.md`, section 3), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the bone callback that every object of the trader class (class id `AI_TRD_S`) installs at spawn on
the bone `bip01_head` of its model. It covers, in full: when and how the callback is installed (section 2);
which object the head turns toward and how the turning angle is computed from the two objects' positions and
the trader's heading, with its range (there is no limit or rate, section 3); how the turn combines with the
pose the motions give (section 4); when it runs, also under script control (section 5); and a worked example,
the trader at the fidelity view `bunker-start` (section 6). Which motions the trader plays is TC2's and the
scripts' (scope 1.1). What could not be established is marked **unestablished**; what is inferred rather than
read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/behavior/motion-playback.md` (TC1): the local matrix *L* and current matrix *T* = *L* · *T*(parent)
  of a bone from its playbacks (7.3), when bone matrices are computed (7.4, with its review note), which
  requests are made (4.1, 11: unestablished there).
- `docs/formats/skeletal-motions.md`: the row-vector matrix convention (a point **p** goes to [**p** 1] ·
  *M*; *M*[*r*][*c*], row 3 the translation), the local and current matrices (5.4), bone visibility and bone
  callbacks (5.5), partitions (4.3), the pose of one motion at a time *t* (5).
- `docs/formats/skeleton-ogf.md`: bones and their parents (4). `docs/behavior/model-shading.md`: the render
  matrix *K* = *B*⁻¹ · *T* and skinning (2).
- `docs/behavior/object-visual-and-transform.md`: an object's transform *X* and position **p** (4).
  `docs/behavior/world-coordinates.md`: +y is up.
- `docs/formats/sav.md`: the spawn flags of a record (7.2) and the player's record (7.1, 10.1).
  `docs/behavior/player-state.md`: the player character's record.
- `docs/behavior/trader-and-crows-scope.md` 1.1: the trader class, the record `escape_trader`, its model
  `actors\trader\trader` and the partitions of `actors\trader_animation`; the per-frame channel check that
  is skipped under script control. `docs/behavior/characters-scope.md` CH-24 and
  `docs/behavior/script-api-scope.md` V: script control.

Conventions as in skeletal-motions: f32(*x*) is *x* rounded to single precision; angles in radians; π is
f32(π) = 3.14159274 where the text compares or adds it. atan(*x*) is the arctangent; atan2(*a*, *b*) is the
four-quadrant arctangent of *a* / *b* (the angle of the point (*b*, *a*), in −π … π); trunc(*x*) drops the
fraction (rounds toward zero). Arithmetic is in single precision; arctangents, sines and cosines are
evaluated in extended precision and rounded to single precision. A **heading** *h* is an angle about the
vertical whose **forward** direction is (−sin *h*, 0, cos *h*) (x, z of the world). Bracketed ids ([H1] …)
name the check that backs the statement next to them: the functions of `checks.py` in the dirty workspace's
`checks/head-bone-turning/` (binary evidence exported by `export.py` into `ev/`, constants read from the
shipped `bin\xrGame.dll`, spawn records and model data decoded with the decoders written from the approved
specs, and `turn.py`, a model written from this text).

## 1. Terms

- A **bone callback** is a function the game code attaches to one bone of a model's bone instances, with a
  **subject** (a value of that code's choosing) and a **mode**: **follow** (the bone's matrix is first
  computed from the motions, then the callback changes it) or **replace** (the callback alone gives it). A
  bone holds one callback; attaching another replaces it [H1, H3].
- The **head callback** is the bone callback of this specification, the **trader** the object of the trader
  class that installed it, the **head bone** the bone it is on.
- The level's **controlled object** is the object the level designates as the one being controlled (3.1);
  the level also has a **view object**, which this callback does not read [H4].

## 2. Installation

At its spawn, once the spawn has succeeded and the object has been made visible and enabled, an object of
the trader class looks up the bone named `bip01_head` in its model and attaches the head callback to it, in
follow mode, with the trader itself as the subject [H1]. A spawn that fails returns before this [H1]. No
other function of the trader class's interface attaches or removes a bone callback [H2]. Whether any code
outside the trader class later replaces the head callback on the trader's head bone is **unestablished**
[H2].

The only record of the trader class uses the model `actors\trader\trader` (scope 1.1). In it `bip01_head`
is bone 12 of 55, its parent is `bip01_neck`, it belongs to the partition `torso` of
`actors\trader_animation`, and its descendants are exactly the 10 bones of the partition `head`
(`bip01_ponytail1`, `bip01_ponytail2`, `bone01`, `bone02`, `bone04`, `bone06`, `bone08`, `bone10`,
`bone12`, `bone14`) [H11]. What a model without a bone of that name would do is not part of this
specification (no shipped record of the class has one).

## 3. The turning angle

Each run of the head callback computes one angle *A* from the target's and the trader's positions and the
trader's heading; it reads nothing else [H4, H10].

### 3.1 The target

The target is the level's controlled object at the moment the callback runs, and its position **t** is that
object's position (the translation of its transform, object-visual-and-transform 4); the trader's position
**o** is the trader's own. Only the x and z components are used: the height plays no part [H4]. The callback
does not test whether the level has a controlled object; what happens when it has none is **unestablished**
(**believed** a fault, since the target's position is read through it) [H4].

Which object is the controlled object (every place found where the game library sets it) [H5]:

- An object spawned on the client whose spawn flags (sav.md 7.2) have both bits 0x01 and 0x08 becomes the
  controlled object and the view object. In `spawns\all.spawn` only the player's record (section `actor`, on
  `l01_escape`) has both bits [H5], so in a single-player game the target is the player character from its
  spawn on.
- A stalker spawned while the level has no view object becomes the controlled object and the view object
  [H5]. Whether this happens at a New Game (whether any stalker is spawned before the player's
  record) is **unestablished**.
- When the controlled object is destroyed, the level has none (and no view object) [H5].
- The multiplayer spectator and the deathmatch game modes also set it [H5]; they are not part of this
  specification.

### 3.2 The target heading

With *dx* = f32(**t**ₓ − **o**ₓ), *dz* = f32(**t**_z − **o**_z) and ε = f32(1e−7) = 1.00000001e−07, the
target heading *y* is [H6]:

- 0 when |*dx*| < ε and |*dz*| < ε;
- otherwise, when |*dz*| < ε: −π/2 when *dx* > 0, π/2 when *dx* < 0;
- otherwise, with *r* = atan(f32(*dx* / *dz*)): *y* = −*r* when *dz* ≥ 0, *y* = −(*r* − π) when *dz* < 0.

So *y* is a heading whose forward direction points from the trader to the target [H15]. When the two
positions coincide horizontally *y* is 0, not the trader's heading [H6].

### 3.3 The trader's heading

The trader's heading *h* is taken from its transform *X* (object-visual-and-transform 4) [H4, H7]: if
√(f32(*X*[0][1]² + *X*[1][1]²)) > 2⁻¹⁹ (1.90734863e−06), *h* = −atan2(*X*[2][0], *X*[2][2]); otherwise
*h* = −atan2(−*X*[0][2], *X*[0][0]) [H7]. For an object with angles (0, *a*₁, 0) (no tilt, as the trader's
record) *h* = *a*₁ for *a*₁ in −π … π, and row 2 of *X*, (−sin *a*₁, 0, cos *a*₁), is its forward direction
[H15].

### 3.4 Wrapping

The difference *d* = f32(*y* − *h*) is brought into −π … π [H7, H8]:

- if −π ≤ *d* ≤ π, *w* = *d*;
- otherwise *x* = f32(*d* · f32(1 / (2π))) (0.159154937), *f* = f32(*x* − trunc(*x*)); if *f* < 0, *f* =
  f32(*f* + 1); *f* = f32(*f* · f32(2π)) (6.28318548); if *f* > π, *f* = f32(*f* − f32(2π)); *w* = *f*.

The turning angle is *A* = *w* (the program forms it from the absolute value of *w* and its sign, which gives
*w* again) [H8]. The rules of 3.2 – 3.4 reproduce the program's angle on random positions and headings
[H8].

### 3.5 No limit, rate or state

*A* lies in −π … π and is used as it is: there is no other limit (the head turns fully toward a target
beside or behind the trader), no rate or smoothing, no dependence on time, distance, configuration or the
object's state, and the callback keeps nothing between runs [H8, H10]. Each run starts again from the
positions and the heading of that moment.

## 4. The turn and the pose

The head callback runs in follow mode: the head bone's current matrix *T* is first computed from its
playbacks as motion-playback 7.3 gives it (*T* = *L* · *T*(parent)), then the callback replaces it with
*T*′ [H3]. With *s* = sin *A*, *c* = cos *A* and *T*ᵣ the rows of *T* (first three components) [H9]:

| Row of *T*′ | Columns 0 – 2 | Column 3 |
|---|---|---|
| 0 | *T*₀ | 0 |
| 1 | *c* · *T*₁ − *s* · *T*₂ | 0 |
| 2 | *s* · *T*₁ + *c* · *T*₂ | 0 |
| 3 | *T*₃ (the translation, unchanged) | 1 |

That is *T*′ = *R* · *T* with *R* the rotation about the bone's own x axis whose rows are (1, 0, 0),
(0, *c*, −*s*), (0, *s*, *c*): the head bone keeps its position and turns about its own x axis by *A*,
whatever direction that axis has in the pose [H9, H13].

- The bone's children are computed after it with *T*′ as their parent's matrix, so every descendant (on the
  trader, the 10 bones of the partition `head`, whatever motions play on that partition) turns with it
  [H3, H11, H13].
- The head bone's render matrix *K* = *B*⁻¹ · *T*′ (model-shading 2) is computed after the turn, so the
  skinned head shows it [H3, H14].
- Bones outside the head bone's subtree are not changed [H13].

In the trader's motions the head bone's x axis is close to the model's vertical (+y up, world-coordinates;
for the desk motion `idle` at *t* = 0 it is (0.0299, 0.9930, 0.1132) in model space), so the turn is close
to a horizontal turn of the head, and a positive *A* turns the head toward increasing heading: the head
turns toward the target by the difference between the target heading and the body's heading [H13].

## 5. When it runs

The head callback runs each time the head bone's matrix is computed and at no other time [H3, H10]:

- only while the head bone's visibility bit is set (skeletal-motions 5.5): a bone not computed is not turned
  [H3];
- only when the model's bone matrices are recomputed (motion-playback 7.4); between recomputations the
  last turned matrix stays, so the head follows the target at that cadence. Which requests for the trader's
  bone matrices drawing and game code make is **unestablished** (motion-playback 4.1, 11).

Nothing else gates it: the bone calculation consults neither the object's state nor whether it is under
script control, so the head turns under script control too (CH-24; the skip of scope 1.1 concerns only the
per-frame check of the motion channels), while talking or not, and whatever motions play on either partition
[H3, H10].

## 6. Worked example: the trader at `bunker-start`

Records of `spawns\all.spawn` on `l01_escape` (sav.md): `escape_trader` at (−244.6927, −24.9580,
−133.9600) with angles (0, 1.5491940, 0); the player's record (section `actor`) at (−246.7263, −24.7933,
−134.4339) [H12]. At the view `bunker-start` the player has not moved and its position is **believed** to
be its record's horizontally (the view's 30 s pass with no input, scope 1.1).

- *dx* = −2.0336, *dz* = −0.4739 [H15]; the trader's heading *h* = 1.5491940; the target heading *y* =
  1.7997276; *A* = 0.2505336 (14.35°) [H12].
- With the desk motion `idle` (all partitions) at *t* = 0 (motion-playback 7, `turn.py`): the head bone's z
  axis turns by 0.2515 of heading (its x axis is not exactly vertical); the head bone stays at (−0.0311,
  1.1592, 0.0958) in model space, which is (−244.789, −23.799, −133.989) in the world; its descendant
  `bone14` moves from (−0.0367, 1.2541, 0.1915) to (−0.0573, 1.2553, 0.1868) in model space; no bone outside
  the head's subtree moves [H13].

## 7. Not covered

- Which motions the trader plays and when (TC2 and the scripts, scope 1.1).
- Which requests for bone matrices drawing and game code make, and the cadence of recomputation
  (motion-playback 4.1, 7.4).
- Bone callbacks other than the head callback (other classes install their own on their bones).
- The multiplayer game modes that set the controlled object (3.1).

## 8. Unestablished

- What happens when the level has no controlled object while the callback runs (3.1; **believed** a fault).
- Whether a stalker becomes the controlled object at a New Game before the player's record is spawned
  (3.1).
- Whether any code outside the trader class replaces the head callback on the trader (section 2).
- Which requests for the trader's bone matrices are made, so how often the head is turned (section 5).
- The player's horizontal position at the view `bunker-start` (section 6; **believed** its record's).

## 9. Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (later archives win); binaries `bin\xrGame.dll` and `bin\XR_3DA.exe`.
- Data examined: `spawns\all.spawn` (all records, for the spawn flags and the two records of the worked
  example), `meshes\actors\trader\trader.ogf`, `meshes\actors\trader_animation.omf`; the approved
  specifications listed above.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation with Ghidra) of
  the trader class's spawn and the head callback with its heading and arctangent helpers, every reference to
  the callback, the bone-callback setter and the engine's bone calculation, the level's controlled and view
  objects and every place the game library sets them, the interface functions of the trader class; the
  integer conversion in the wrap emulated instruction by instruction; constants read from the binary. No
  execution or debugging. `turn.py`, a model written from this text on top of the playback model of
  motion-playback, reproduces the program's angle and matrix and section 6.
- No leaked source code, derived projects or third-party documentation of the engine were used.
