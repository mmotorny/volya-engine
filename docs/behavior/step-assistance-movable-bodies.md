# Step assistance while the player's body touches a movable body

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of what the step
assistance of the player's body (`level-cform` 6.3) and its drive (`level-cform` 6.2) do while the body
touches a movable body: whether the step probe tests movable bodies' shapes, how the step-assistance decision
changes, and what of the drive, the step replacement and the contact friction changes. It fills the part
`level-cform` 6.3 leaves out ("when the body touches a movable object, different rules apply"). Its trigger is
the trader's door at the start of `l01_escape` (`trader-door`).

Builds on (not repeated): `docs/formats/level-cform.md` (cited as `cform`; the body, its shapes, its contact
rules 5.4, the forces 6.2, the probe and the step replacement 6.3), `docs/behavior/trader-door.md` (cited as
`door`; the player's contacts with a movable body, 3.1 – 3.3), `docs/behavior/physics-world.md` (cited as
`world`; the order of a step, 3), `docs/behavior/player-movement.md` (cited as `movement`; the stored
acceleration `a_k`, 6.1, and the drive, 6.2), `docs/behavior/climbing.md` (cited as `climbing`; the climbing
states, 2.2), `docs/behavior/start-position.md`.

Conventions:

- A **movable body** is a physics body other than the player's own: a contact is **with a movable body** when
  both its shapes are attached to bodies (`door` 3.3 uses the same test). Shapes of the level mesh have no body.
- `â` is the unit direction of the stored acceleration `a_k` (`movement` 6.1), `a` that vector. Positions are
  world positions, +y up. "The ground contact", "the obstacle contact", their normals (pointing from the surface
  towards the body) and "walkable" are those of `cform` 6.2; the **ground point** and the **obstacle point** are
  the positions of those two contacts (the library's contact position).
- "Step" is a physics step (`cform` 6.1); its **contact phase**, **pre-step work** and **post-step work** are
  items 3, 4 and 8 of `world` 3.
- Statements are static (read from the shipped binaries; the game was not run) unless marked otherwise.
  Inferred statements are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing a statement: functions of `checks.py` in the dirty workspace's
  `checks/step-assistance-movable-bodies/` (`python3 run_all.py` re-creates the evidence and runs them all).

## 1. The step probe tests only the level mesh

- The probe of `cform` 6.3 makes a single query: a box query of the level's static collision mesh, the mesh
  built from `level.cform` when the level loads; it then tests only the triangles that query returns (skipping
  pass-through materials, as `cform` 6.3 states). It makes no collision test of the physics library and no
  query of objects [P1, P1b].
- So **no movable body's shape ever takes part in the probe**: a movable body under the lower box does not turn
  step assistance on, and a movable body in the upper box does not turn it off or set the "blocked" flag
  through the probe. The trader's door's leaf is not seen by it; a level-mesh sill under the leaf is.
- Unchanged from `cform` 6.3: an upper-box hit sets the step's "blocked" flag and reports off; otherwise a
  lower-box hit reports on [P3].
- With a stored acceleration of length 0 (`a_k` = 0, no movement), the probe reports **on** without testing
  either box [P2]. The probe is evaluated at every pre-step of an awake body whatever the movement request
  [O1b]; `cform` 6.3's "every step while movement is requested" describes the box test, which applies whenever
  `|a_k| > 0` (so also for `0 < |a_k| ≤ 0.1`). This holds in both decisions below.

## 2. Touching a movable body

### 2.1 The touch flag

The body keeps a per-step **touch flag**. During the contact phase, every contact handed to the player's
contact rules (`cform` 5.4) sets it when the contact is with a movable body [T1], with one exception: a contact
whose other shape (the one that is not the player's) has a pass-through material and is not being made leaves
it unchanged (the rules return before it) [T1].

Which contacts reach the rules [T2, T2b, T2c]:

- In a contact between two objects' shapes the shapes' own callbacks run first, then both objects' contact
  callbacks, whatever the shapes' callbacks decided; that decision ("made" or not) only chooses whether a
  contact joint is created [T2].
- The player's object callback hands, in single play, every contact of its shapes to the rules, made or not;
  the exception is a contact of one of three further shapes the player's body keeps besides the four of `cform`
  5.1, which only sets the "blocked" flag and does not reach the rules [T2b]. (What those three shapes are and
  which bodies they meet is **unestablished** here; the other game modes' path is out of scope.)
  **Disputed in review:** such a contact does more than set "blocked": its friction is set to 0, and when it is
  still made and a further condition on the other object holds (**unestablished**), it is attached as a contact
  joint to the other object's body alone, marked not made, and the traction value so far is multiplied by a
  constant (its value not established here).
- The **outer sphere** (`cform` 5.1; radius `2r` = 0.7 centred 1.225 m above the feet for the standing box,
  `cform` 7.4) is one of the four shapes the rules recognise. Its own callback marks each of its contacts not
  made and sets "blocked" (`door` 3.1); its contacts still reach the rules and so **set the touch flag
  whenever the outer sphere overlaps a movable body's shape**, before any shape of the body proper touches it
  [T1, T2c]. Such contacts end in the rules right after the touch flag (and the step replacement's test): they
  are not counted, and are neither the ground nor the obstacle contact (section 3.2) [T1, S5].
  **Disputed in review:** before handing a contact to the rules, the player's object callback marks it made
  whenever either shape's material has the actor-obstacle flag (`cform` material flag 0x1000); an outer-sphere
  contact with such a shape therefore reaches the rules as made, is counted (3.3) and can become the ground or
  obstacle contact. Whether any movable body's shape carries such a material is **unestablished**.

Movable bodies in this sense are every shape attached to a body: physics objects such as the trader's door's
leaf, and other creatures' bodies (**believed**: their collision shapes are attached to their character
bodies, which carry a flag of the same kind for their own contacts [T4]).

### 2.2 Its lifetime

- The flag is set during a step's contact phase; at that step's post-step work it is copied to a
  **previous-step flag** and cleared (together with the step's "blocked", ground, obstacle and contact-count
  values) [T3]. A body at rest (switched off) skips this, as it skips its pre-step work (3.1) [T3, O1b].
- The body counts as **touching a movable body** for the decision of section 3 when the touch flag of the
  current step or the previous-step flag is set [S1]: from the step of the first such contact until the step
  after the last one.
- The body's creation clears the touch flag; the previous-step flag is not set at creation, so its value at the
  body's first step is **unestablished** (it is overwritten at the end of the first step) [T3].
- Nothing else in the program reads or writes these two flags [T4]: the drive, the side force, the contact
  rules and the step replacement do not depend on them (section 4).

## 3. The step-assistance decision

### 3.1 When it is made

In a step the contact phase runs first, then the pre-step work: there, for an awake body, the step-assistance
decision is made unconditionally, then the drive force is computed from it and applied [O1, O1b]. (A body
switched off, at rest, skips its pre-step work and keeps its last decision [O1b].) The decision of a step is
used by that step's drive (`cform` 6.2) and, since the next step's contact phase comes before its pre-step
work, by the next step's contact rules: the step replacement of `cform` 6.3 and the "× 4" contact friction of
`cform` 5.4 [D2b, O1].

### 3.2 The two decisions

**Not touching a movable body** (`cform` 6.3): step assistance is on when the probe reports on and the climbing
state is not *at the bottom* (`climbing` 2.2) [S1, C1]. (That climbing state is the "internal movement state"
`cform` 6.3 leaves unestablished; it is the state near a ladder's lower end, not climbing, and so occurs on
foot.)

**Touching a movable body**: the following run in this order, each on the result of the previous [S1, S2, S3,
S4]:

1. **Probe.** Step assistance is set to the probe's result, without the climbing-state exception [S1].
2. **Height limit.** If step assistance is on and the body centre's height differs from the height of the
   **recorded position** (below) by more than 0.5 m (`|y − y_rec| > 0.5`), it is turned off [S2, S4b].
3. **Switch-on.** It is turned on (whatever it is) when all of these hold [S3]:
   - the body keeps no ladder, or its climbing state is *near* (`climbing` 2.2) [S3, C1];
   - an obstacle contact exists in this step;
   - more than one contact was counted in this step (the count of section 3.3);
   - the obstacle normal's y is below 0.70710677 (the obstacle is steeper than 45°);
   - the step's "blocked" flag is not set: no head-sphere contact, outer-sphere contact or contact of the three further
     shapes (2.1) in this step's contact phase, and no upper-box hit by this step's probe (item 1) [S3, P3, T2b, T2c];
   - with `Δ` = obstacle point − ground point and `F` the drive force of the previous step (section 3.3),
     `Δ.x·F.x + Δ.z·F.z > 0.05` (the obstacle lies ahead of the ground contact along the previous drive);
   - `Δ.y > 0.01` (the obstacle point is more than 1 cm above the ground point).
4. **Recording.** If an obstacle contact exists in this step, more than one contact was counted and step
   assistance is now on, and in addition `|Δ.z + Δ.x| > 0.05` (the plain sum of the two horizontal components,
   not a length) and `Δ.y > 0.01`, the body centre (all three coordinates) becomes the recorded position [S4,
   S4b].

The result after item 4 is the step's step assistance. In item 3 a ladder state other than *free* or *near*
only skips that item; items 2 and 4 apply in every climbing state [S3, S4].

All comparisons are strict; the constants are 0.5, 0.05, 0.70710677 and 0.01 (the last a stored value no
instruction writes) [S2, S3, S4, K1].

### 3.3 The quantities

- **Contact count**: the number of this step's contacts handled by the contact rules as made, so the outer
  sphere's contacts and contacts not being made are not counted; contacts with the level and with movable bodies
  both count. It is reset at each post-step [S5].
- **Ground point** and **obstacle point** are stored when the ground and the obstacle contact are chosen
  (`cform` 6.2), with their normals [S5b]. The obstacle exists only if chosen in this step; the ground point is
  the last one stored: in a step without a ground contact it is that of an earlier step [S5b, T3].
- **Previous drive force** `F`: the movement force of `movement` 6.2 (with the changes of `cform` 6.2 during
  step assistance) is computed in each step's pre-step work after the decision, so at the decision it holds the
  value computed in the previous step (0 before the first) [S6, O1b].
- **Recorded position**: written only by item 4 and read only by item 2 [S7]. It is not set when the body is
  created: the body is taken from the general heap without being cleared, so before the first recording its
  value is whatever the memory held, **unestablished** [S7, S7b]. It is kept across periods without contact
  with movable bodies (nothing else writes it), so a later first touch compares against the position recorded
  at an earlier one. **Disputed in review:** the check establishes "nothing else writes it" only for the
  body's own code (its type's functions and the part of the binary around them); accesses from elsewhere were
  not excluded, so this is **believed**, not established.

## 4. The drive and the other rules while touching

- **Drive and side force.** Their rules are those of `cform` 6.2 and `movement` 6.2, unchanged: during step
  assistance with an obstacle contact the drive is `25·m·T` along `â` turned into the obstacle plane, × 4,
  with the vertical component made non-negative and the horizontal signs of `â`; the side coefficient 700; they
  read the step-assistance result and the obstacle contact, never the touch flags [D1]. The obstacle contact may
  be a contact with the movable body (its normal toward the body, `door` 3.3). Example: moving straight into a
  vertical face (obstacle normal `−â`), the assisted drive points straight up, at `100·m·T` [W1, D1].
- **Step replacement.** Its conditions (`cform` 6.3) contain nothing about movable bodies [D2]; it acts on
  contacts with movable bodies as on the level's (`door` 3.3). The "two internal body states" `cform` 6.3
  leaves unestablished are the climbing states *climbing up* and *climbing down* (`climbing` 2.2): the
  replacement is off while climbing [D2, C1]. While the outer sphere overlaps a movable body, the "blocked"
  flag it sets keeps the replacement off in that step and the next (`cform` 6.3, `door` 3.3).
  **Disputed in review:** within that step it keeps the replacement off only for contacts handled after the
  outer-sphere contact set the flag (the replacement tests "blocked" as set so far in the step); contacts
  handled earlier in the step are not affected by it.
- **Contact friction and traction.** The "× 4 during step assistance" of `cform` 5.4 follows the decision as
  made (section 3.1) [D2b]; a contact with a movable body has the traction value 1 (`cform` 5.4, `door` 3.3).

## 5. At the trader's door (derived)

- The probe ignores the leaf (section 1); a level-mesh sill below it can report on.
- Every contact of the bottom sphere, the torso cylinder or the head sphere with the leaf's face comes with the
  outer sphere overlapping the leaf: a shape touching a vertical face has the body axis within its radius
  (at most 0.35 m) of the face, inside the outer sphere's 0.7 m at 1.225 m above the feet, which the 2.645 m leaf
  covers if it stands on the floor (**believed**) [G1]. In every such step the touch flag and "blocked" are set,
  so the switch-on (item 3) cannot act and step assistance is the probe's result subject to the height limit.
- Recording needs step assistance on. So, at the first touch:
  - if the recorded position lies more than 0.5 m from the body centre in height, step assistance stays **off**
    in every step in which the body touches the leaf, nothing is recorded, and the drive is the unassisted one of
    `movement` 6.2: the body stays on the floor. This holds, for example, if the memory held zeros: at the start
    of `l01_escape` the body centre is more than 20 m below y = 0 [X1, S7b];
  - otherwise step assistance follows the probe, the recording keeps following the body whenever its own
    conditions hold, and the assisted drive into the leaf points up (section 4), as in the issue's report of
    our behaviour.
- Which case holds in the shipped game cannot be decided statically (**unestablished**: it depends on the
  recorded position's initial value, section 3.3). The original's player staying on the floor at the door
  (issue #877, a black-box observation) is consistent with the first case.

## 6. Unestablished

- The previous-step flag's value at the body's first step (2.2) and the recorded position's value before its
  first recording (3.3); so which case of section 5 holds in the shipped game.
- What the player's body's three further shapes are, which bodies they meet, and the other game modes' contact
  path (2.1).
- That other creatures' collision shapes are attached to bodies (2.1; believed).
- That the trader's door's leaf covers 1.225 m above the player's feet where the player meets it (5; believed).

## 7. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrGame.dll`,
  `bin\XR_3DA.exe`, `bin\xrCore.dll`.
- Data examined: the approved specifications listed above (the climbing states' numbering through the climbing
  specification's own checks; the start eye height of `start-position`).
- Analysis: static analysis of the binaries (headless decompilation and disassembly with Ghidra) of the player's
  body: the step probe and its query, the step-assistance decision and its rules for movable bodies, the contact
  rules and the touch flag, the pair contact generation and the shapes' and objects' contact callbacks, the
  body's pre- and post-step work, its construction and allocation; the engine's loading of the level's static
  collision mesh; the allocator. Constants read from the binary. No execution or debugging; the game was not
  run. The checks in `checks/step-assistance-movable-bodies/` reproduce every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Section 5 considers only the steps in which the body touches the leaf; in the step after the last contact
  (touching through the previous-step flag alone) "blocked" can be clear, so the switch-on and a recording can
  act on the other contacts and change the recorded position, and with it which case of section 5 applies at a
  later touch.
