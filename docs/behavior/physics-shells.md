# Shells from models and records

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of how the game builds the
**shells** of the objects it simulates: which objects get one and when, the parts of a shell (elements, their
bodies, geometries, masses and frames), the build from a model's bone physics data in the general case, the
physics types 0 – 3 of physics objects, the simple shells (the box shells of type 0 and of items, the
box-and-sphere shell), the shells of hanging lamps and breakables, fixed bones and the physics keys of a record's
custom data and of a model's user data, the shells built at spawn and the startup animation's part, the bones'
transforms and the object's transform taken from the bodies (what is drawn), and the fatal cases. It is PH-3 of
`docs/behavior/physics-scope.md` (section 3). Section 10 states what the first shell the game needs, the trader's
door at the Escape bunker, takes.

Builds on (not repeated):

- `docs/behavior/physics-scope.md` (cited as `scope`): terms *body*, *shell*, *joint*, *step*, *frame*; the
  record kinds and their counts (1.2), the model user data keys (1.3);
- `docs/formats/bone-physics.md` (cited as `bone`): every field of the bone physics data, *used shape*, the
  joint types, which bones get a body of their own and which join the body above (3.1), joint axes, limits and
  stiffness (3.2, 3.3), breakable bones (3.4), the shape volumes and the sharing of a bone's mass (2.3, 5), the
  bone materials (6);
- `docs/behavior/physics-world.md` (cited as `world`): the world, *object*, *group*, *active* objects, the step
  and its order, the exact and iterative solvers, the floor;
- `docs/behavior/physics-library.md` (cited as `library`): the shipped library and *public 0.5.0*;
- `docs/behavior/physics-shell-shapeless-root.md` (cited as `root`): the case of a model whose root bone has no
  used shape, the fixing of an element, the request for the exact solver, the hinge with a fixed first element,
  the record's mass for type 3, resting and waking of the trader's door; that case is not repeated here;
- `docs/behavior/level-objects.md` (cited as `objects`): the object's transform *X* (3.4), model files, bones,
  the bind pose and the model-space bind matrix *B* (4.4);
- `docs/behavior/hanging-lamps.md` (cited as `lamps`) 3.4; `docs/behavior/trader-door.md` (cited as `door`);
  `docs/formats/sav.md` (record layouts: the Visual, Skeleton and physics-object blocks, 7.4).

Conventions:

- An **element** is one body of a shell together with the collision geometries on it (`root`). Its **frame** is
  a transform: in model space while the shell is being built, in world space once it is activated. Its **mass
  centre** is a point in its frame. A shell's **transform** is the transform of the object it simulates as the
  shell holds it.
- **Activating** a shell puts it into the world: its bodies and joints are made. An element **switched off** has
  its body's disabled flag set (`root`).
- The **plain build** and the **checking build** are the two ways into the build from the bone data (`root`
  1.1): the same build, the second followed by a fatal test of the root.
- Transforms compose in the row-vector convention of `objects` 4.4: "A then B" is A · B.
- "Fatal" is a failed check with a message (`world` 5); the game stops.
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements
  are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing a statement: functions of `checks.py` in the dirty workspace's
  `checks/physics-shells/` (`python3 checks.py` runs them all).

## 1. Who builds a shell, and when

| Object | When | Build |
|---|---|---|
| physics object (`O_PHYS_S`, `P_DSTRBL`) | at spawn | by its physics type (section 4) [S1, T1] |
| hanging lamp with flag 0x01 (`lamps` 3.4) | at spawn | plain build with the lamp's fixed bones (section 5.3) [S2, F2] |
| breakable (`O_BRKBL`) | not at spawn: a static geometry without bodies; a shell only when it breaks | checking build (PH-14) [S2] |
| skeleton object (`P_SKELET`, parts split off at run time) | at its spawn | plain build, no fixed bones, switched off when skeleton flag 0x01 of its record is clear [S2b] |
| artefacts, the helicopter | (their specs) | plain build, no fixed bones, not switched off [S2b] |
| items | (PH-6) | the box shell or the box-and-sphere shell (section 6) [S2c] |
| a character or monster at death | (PH-15) | checking build [S2b, S2d] |
| rockets; parts of a shell that splits | (PH-17, PH-13) | their own builds [S2c] |

Thirteen places in the game make a shell; besides those of the table, two are **unestablished** [S2c].

The checking build is reached only from the death shell of characters and from a breaking breakable; the
builds of physics objects, skeleton objects, artefacts, the helicopter and hanging lamps use the plain build, so
the root test (and its fatal message, section 9) never applies to them [S2d]. (This settles the open item of
`root` 1.1 for the builds found; no other caller of the checking build was found, **believed** none.)

## 2. The parts of a shell

### 2.1 Elements and their bodies

An element's body is placed with its **origin at the element's mass centre** carried by the element's frame,
and with the frame's rotation; reading back, the element's frame is the body's pose moved back by the rotated
mass centre [E1]. So a body's origin is its centre of mass, while the element's frame keeps the bone's own origin
and axes.

Activating an element [E1b]: its world frame becomes its frame (model space) followed by the shell's transform;
its body is made, given the element's mass, placed as above, given zero linear and angular velocity, switched off
when the activation asks for it; an element with no geometry is fixed instead of given a mass (`root` 1.3); and
the bone that made the element is given the shell's bone callback (section 8.1). Activating a shell activates
every element with the shell's transform first, then makes the joints [B3b] (`root` 2.1).

### 2.2 Geometries

- A **box** raises each half size to at least 0.005 [E2] (`bone` 2.3).
- An element with more than one geometry gets a collision space of its own (**believed**: so that its
  geometries are tested together against others) [E2b].
- Each geometry records the bone it was made from [B1e], and carries the element's material [E2b].

### 2.3 Mass

An element's mass is set in one of three ways [E3]:

- from a **mass and a centre** (the bone data's mass and centre of mass): the mass centre is that centre, the
  density is mass / (total volume of the element's geometries), and each geometry carries volume × density at
  its own centre, all added about the mass centre;
- from a **mass** alone: the same, about the volume-weighted centre of the geometries;
- from a **density** alone: each geometry volume × density, about the volume-weighted centre.

The volumes are those of `bone` 2.3. A shell's mass is set in one of three ways [E3b]: a total shared between
the elements in proportion to their geometries' volumes; a total shared equally between the elements; a density
given to every element.

### 2.4 An element's motion parameters

Every element starts with [E4]: linear speed limit 150, angular speed limit 9.817477, two velocity divisors
1.01 (linear, angular), linear drag 0.0002, angular drag 0.05. A shell can set both drags of all its elements at
once (sections 3.4, 4.4, 5.3) [E4b].

What reads them, as found [E4b]: a switched-on body's linear and angular velocity divided by their divisor; a
speed above its limit is limited (how: **unestablished**); a torque −(angular drag) × angular velocity; a force
−*k* × velocity with *k* = (linear drag) × speed, but at most (body mass) / (step length). When in the step this
runs is **unestablished** (**believed**: before solving, as an object's pre-step work of `world` 3 item 4; PH-5
owns resting).

## 3. The build from the bone data

### 3.1 Before the build

For objects with a Skeleton block (`sav.md` 7.4.1), the model's **root bone** and its **visible bones** are set
from the record's saved bone state before the shell is built; a record without saved state (skeleton flags bit
0x04 clear, every record of `spawns\all.spawn`) gives every bone visible and root bone 0 [S1b]. The root of every
physics-object model is bone 0 [D1c], so the build starts at the model's own root.

### 3.2 The walk

The bones are walked from the root, children in bone order (`bone` 3.4). Each bone's **transform** is its bind
transform (`objects` 4.4, *L*) followed by the transform passed down from above, so at the root it is *L* and
below it is the bone's model-space bind matrix *B* [B1]. For each bone:

1. A bone that is not visible adds nothing; its children are walked with its transform and the same element
   above [B1b].
2. A bone that gets an element of its own (`bone` 3.1, `root` 1.2) [B1d, E2c]: the element's frame is the
   bone's transform (model space, the bind pose); the element records the bone; its material is the bone's
   material (`bone` 6); its element above is recorded; if the bone has a used shape, its shape (box, sphere or
   cylinder of the bone data, in the bone's own space = the element's frame) is added and its mass set from
   the bone's mass and centre of mass (2.3). The element is added to the shell, and the bone's joint is made
   (`bone` 3.1 – 3.3; built at activation, `root` 2.1).
3. A rigid bone with a used shape and a body above: its transform is taken into the frame of the element above
   (followed by the inverse of that frame), its shape is added to that element there and its mass added to that
   element's mass (`bone` 5) [B1c]. A rigid breakable bone becomes a part that can split off (`bone` 3.4,
   PH-13).
4. A bone without a used shape and a body above adds nothing (`bone` 3.1).
5. The record's fixed bones (section 5) note, for each named bone, the element the bone ended in: its own or
   the one it joined (`root` 1.4).

### 3.3 Joints

The joints of `bone` 3.1 – 3.3 connect the bone's element (second) to the element above (first). A joint built
from the bone data has its **anchor at the origin of its second element's frame**: the bone's bind origin, not
its centre of mass (2.1) [B2]. (This settles the first gap of `bone`.) Kinds, axes, limits and stiffness are
`bone`'s; their simulation is PH-7.

### 3.4 After the build

Objects that build from the bone data (physics objects of type 3, skeleton objects, artefacts, the helicopter)
then [B3]: tie the shell to the object, set the shell's transform to the object's transform *X*, activate the
shell (switched off when asked: section 4.3, table of section 1) and set the drags of every element to linear
0.0002, angular 0.05. Hanging lamps do the same in their own order (section 5.3).

## 4. Physics objects

### 4.1 By type

The record's physics type selects the build [T1] (`scope` 1.2):

| Type | Shell | Mass |
|---|---|---|
| 0 | one element with one box (4.2) | the record's mass |
| 1, 2 | one element per bone with that bone's box (4.3) | the record's mass, shared equally |
| 3 | the bone data (section 3) with the record's fixed bones (4.4) | the bone data's; the record's is not read (`root` 6) |

Then, whatever the type [T1]: the shell's transform is set to *X*, the drags of every element to linear 0.001,
angular 0.02 (replacing those of 3.4), and, if the model is a skeleton model, every element gets the rest
parameters of the model's `[disable]` (`scope` 1.1; their use is PH-5).

In `spawns\all.spawn` [D1, D4]: `O_PHYS_S` has 10 records of type 0 (1 with a rigid model, 9 with animated
character models on `l05_bar` and `l12_stancia_2`, `scope` 1.2) and 475 of type 3; `P_DSTRBL` has 1,420, all of
type 3. Of the 1,895 type-3 records, 503 name fixed bones; 1,518 have skeleton flag 0x01 clear (built switched
off), 377 set.

### 4.2 Type 0: the simple box shell

One element with one box: unrotated, centred at the centre of the model's bounding box (`objects`: the visual's
box), with half sizes 0.5 × its extent (then at least 0.005, 2.2) [T1b]. The record's mass is spread over the
shell by volume (one element: all of it), about the box centre (2.3). Unless the object already has a parent object
(**believed**: the condition tests a field holding another object [T1c]), the shell is activated at once at *X*,
switched off when skeleton flag 0x01 is clear; with a parent it is not activated here. No element of it records a bone, so no bone gets a
callback: the bones of an animated model keep their animation (section 7), and the whole object moves with the
box (section 8.2).

### 4.3 Types 1 and 2

Unused in the data (`scope` 1.2) [D4]. From the root, for every bone [T2]: an element whose frame is the bone's
current model-space pose, with the bone's chunk-13 box (`objects` 4.4; when the length of its half-size vector is
below 0.05, 0.05 is added to each half size), mass 10; the bone is given the shell's callback; it is joined to the
element above by a three-axis joint (`bone` 3.2) with its anchor at (0, 0, 0), first axis x, third axis y, and
limits −π/2 … π/2 on all three axes (the limits only when both elements exist). Type 2 makes no joint for the
root; type 1 makes one for the root too, with no element above. Afterwards every element's mass is replaced by
the record's mass / element count [T1]. No activation is part of this build; where such a shell is activated is
**unestablished** [T2b].

### 4.4 Type 3

The record's fixed-bone text is split at commas and each name looked up in the model; a name the model lacks is
fatal ("wrong fixed bone") [T3]. The shell is built from the bone data with them (section 3; `root` for the
shapeless root), activated switched off when skeleton flag 0x01 is clear [T3] (`root` 3.1). If any fixed bone
is named, the shell requests the exact solver and each named bone's element is fixed (`root` 1.3, 1.4).

Then the physics keys of section 5.2 are applied twice: first from the record's **custom data** (read as LTX
text; the same text other features read sections such as `[alife]` from), then from the **model's user data**;
for both, "the shell has fixed bones" means the record's fixed-bone text is not empty [T3b].

## 5. Fixed bones and the collide keys

### 5.1 Fixing

A fixed element is held in place: `root` 1.3 (fixed flag, a sphere mass adjusted to 10⁸, gravity off, zero
velocities, force and torque).

### 5.2 The physics keys

Read from a custom data or a model user data, in this order [F1, F1b]:

- `[physics_common]` `fixed_bones`: each named bone's element is fixed; a name the model lacks is fatal ("wrong
  fixed bone"). The key is read whenever the section exists; a section without the key is **unestablished**.
- `[collide]` `ignore_static`, only when the shell has fixed bones: the shell is marked **ignore static**: its
  collision pass makes no contacts with the level's static mesh (it still collides with other objects) [F1c].
- `[collide]` `small_object`, `ignore_small_objects`, `ignore_ragdoll`: each sets a mark of the shell's
  collision class (their effect: PH-4).

The spawn of an object with a Skeleton block (section 7) also reads the model's user data after the shell is
built [S1c]: `[collide]` `not_collide_parts` gives the shell a collision-group number of its own (from a counter; its use is
PH-4), and `[collide_parts]` `small_object` and `ignore_small_objects` set the same marks as above.

In the data [D2, D2b]: no custom data and no model user data has `[physics_common]`; model user data has
`[collide]` in 39 models (only `not_collide_parts`, `scope` 1.3). Custom data has `[collide]` in 707 records
(316 `P_DSTRBL`, 283 `O_HLAMP`, 108 `O_PHYS_S`), with `ignore_static` 704, the misspelling `ignore_staic` 3
(which nothing reads), `ignore_ragdoll` 3 and `cfg` 3. Of the 421 physics objects with `ignore_static`, the 412
of type 3 all name fixed bones, so they ignore the level's mesh; the 9 of type 0 do not, their build reading no
custom data [T1b].

### 5.3 Hanging lamps

A lamp with flag 0x01 and a model (`lamps` 3.4) [F2]: the plain build, the shell tied to the lamp, its transform
set to *X*, activated **switched off**, drags 0.0002 / 0.05; then the fixed bones' elements are fixed: the
comma-separated names of the lamp's fixed-bone field (a name the model lacks is fatal), or the model's root
bone when the field holds no text at all; then *X* again, drags 0.001 / 0.02, the `[disable]` rest parameters,
and the custom-data keys of 5.2 with "has fixed bones" = the field's text is not empty. The keys of 5.2 are not
read from the model's user data for a lamp, and a lamp makes no request for the exact solver [F2].

In the data the 10 lamps with flag 0x01 name `link` (9) or nothing (1: `trader_tv_0002`); all 10 have
`ignore_static` in their custom data, which applies to the 9 [D3, D4b]. Whether the empty field of
`trader_tv_0002` reads as no text (its root then fixed) or as an empty text (whose effect depends on how an empty
list splits) is **unestablished**.

## 6. Item shells

Items build one of two simple shells (which items use which, and when they are activated, is PH-6) [S2c, I1]:

- **box**: one element with an unrotated box at the centre of the model's bounding box, half sizes 0.5 × extent;
  density 2,000 for every element;
- **box and spheres**: the same box with its two shorter half sizes halved (the longest kept), plus two spheres
  centred on the longest axis at ± its half size from the box centre; with *r* the smaller of the two shorter
  half sizes, the radii are 1.4142 · *r* and 0.5 · *r* (which sphere gets which is **unestablished**); density
  2,000; drags 0.0002 / 0.05.

## 7. Spawn and the startup animation

A physics object spawns in this order [S1, S1b, S1c]: the record's physics type and mass are kept; the game
object's own spawn; a collision form from the model's bones; then, unless the record is the copy of a part split
off another object (skeleton flags bit 0x02; PH-13), the root bone and visible bones (3.1), the **shell** (section
4), the **startup animation** (below), a step that receives the record's Skeleton block (**believed**: restoring a
saved bone state, PH-19), the object's transform read back from the shell if it is active, and the model's
`[collide]` keys (5.2); then the object is made visible and enabled.

**The startup animation** [A1]: if the model has motions, the record's startup animation is played as a looping
cycle; an empty name is then fatal ("no startup animation"). Whatever the model, its bones are then computed at
once. In the data all 37 physics-object records with an animated model name one (`idle` 16,
`lab_primary_switcher_idle` 5, `idle_2x` 4, the rest one or two each) [D1b]; the other 1,868 have rigid models,
for which nothing plays.

What the bone computation does to the shell is section 8.1: the first callback of each element's bone places
that element's body at the bone's pose as computed then. So at spawn every body is moved to its bone's pose under
the startup animation (**believed**: the callback runs during this computation, and its pose is the animation's
for an animated model and the bind pose otherwise). From then on a bone with a callback follows its body and no
longer the animation; bones without a callback (section 8.1) keep being computed from their parent and their own
animation or bind pose (**believed**).

## 8. What is drawn

### 8.1 Bones from bodies

The bone that made an element gets the shell's bone callback; bones that joined an element get none [W1d]. The
callback [W1]:

- **first call after activation**: the element's frame becomes the bone's current pose (model space); the body
  is re-placed at that pose followed by the shell's transform (2.1); for the root element the inverse of that
  pose is kept. The bone's own pose is left as computed.
- **every later call**: the bone's pose = the element's world frame followed by the inverse of the shell's
  transform.

### 8.2 The object's transform

Each frame, in the object's frame update (`world` 2.1: before the frame's steps) [W1b]:

- **type 0**: the object's transform is the element's frame;
- **otherwise**: every element's frame is brought up to date, and the object's transform = (the kept inverse of
  the root bone's pose) followed by the root element's frame. So the object follows its root body, and the bones
  are drawn relative to it (8.1).

An element's frame for drawing is blended between its states of the last two steps by the fraction (a value
of the world) / (step length), when a step has run since it was last read; otherwise it is read from the body
[W1c] (**believed**: the fraction is the accumulator's remainder, `world` gaps).

## 9. Fatal cases

| Message | When |
|---|---|
| "wrong fixed bone" | a fixed-bone name the model lacks: the record's list (type 3), `[physics_common]` `fixed_bones`, a lamp's list [T3, F1, F2, X1] |
| "No physics shapes was assigned for model or no shapes in main root bone!!!" | the checking build, when the shell's first element has no geometry (section 1) [X1] |
| "ower 64 bones in breacable are not supported" | a breakable bone above index 63 (`bone` 3.4) [X1] |
| "bad bone mass params" | a rigid bone's mass sum not valid (`bone` 5) |
| "Required dynamic game material" | a bone material without the dynamic flag (`bone` 6) |
| "no startup animation" | an animated model with an empty startup animation (section 7) [A1] |
| "need visual to build" | a breakable without a model (section 1) [S2] |
| "pShell already initialized!!" | a second death shell (PH-15) [S2b] |
| "trying to activate destroyed or not created object!" | activating an object without collision geometry (`world` 5) |

None of the first and the sixth fires for a record of `spawns\all.spawn`: every fixed-bone name of the type-3
records (531 names in 503 records) and of the lamps is a bone of the model [D5], and every animated model has a
startup animation [D1b].

## 10. The trader's door

`trader_door` (`l01_escape`, `physic_object`, `O_PHYS_S`) [P1, P1b, P1c, P1d]: type 3, fixed bone `link`,
skeleton flags 0, empty startup animation; custom data `[logic]` (its scheme, PH-20) and `[collide]`
`ignore_static`; model `physics\door\door_trader` (rigid, no motions, no user data; bones `link`, `door`,
`lock`, none rotated in the bind pose, all of material `objects\large_metal_trash`).

What building it takes, in order:

1. At spawn: root bone 0 (`link`), all bones visible (3.1); the record's mass 10 ignored (4.1).
2. The bone-data build (plain): `link` (no used shape, root) → its own shapeless element, frame at (0, 1.9743, 0)
   in model space, fixed (`root` 1.2, 1.3); `door` (box, joint type 2) → an element with frame at
   (0, 1.3675, 0), unrotated, with one box centred at (0.7029, 0, −0.0134) of that frame, half sizes (0.6902,
   1.3225, 0.1254), unrotated, mass 100 about the centre of mass (0.6601, 0, −0.0380) (2.3); `lock` (no physics)
   → nothing (`bone` 2.2).
3. Tie, place at *X*, activate **switched off** (flags 0): the door's body starts with its origin at its centre
   of mass, world (−248.3419, −23.3206, −134.3756); the hinge (`bone` 3.2: about the door bone's y axis,
   −1.9199 … 0) is made with the fixed frame element first, so it joins the door's body to the static
   environment (`root` 2.1), anchored at the door bone's origin, world (−248.3039, −23.3206, −133.7155) (3.3).
4. Fixed bone named: exact-solver request; `link`'s element fixed again (`root` 1.4).
5. Drags 0.001 / 0.02; rest parameters the defaults (no `[disable]`) (4.1).
6. Custom data: `[collide]` `ignore_static` with fixed bones named → the door's shell never collides with the
   level's mesh, only with other objects (the player's body included, `door` 3) (5.2).
7. Model user data: none. Startup animation: none (rigid model); the bones are computed once, and the bodies are
   re-placed at the bind pose, which is where they already are (7, 8.1).
8. Drawing: the `door` bone follows the door's body (8.1); `link`'s callback keeps it at the fixed body;
   `lock` has no callback and follows `door` (**believed**, 8.1); the object's transform stays *X* while `link`'s
   body does not move (**believed**, `root` 1.3).

## 11. Unestablished and believed

- Two of the thirteen shell makers (section 1), and which object other than physics objects of type 0 uses the
  simple box shell (one other user was found); that no build other than the death shell and the breaking
  breakable uses the checking build (believed).
- How a speed limit is enforced, and when in the step the drags and divisors act (2.4).
- Where shells of physics types 1 and 2 are activated (4.3).
- `[physics_common]` without `fixed_bones` (5.2); the empty fixed-bone field of `trader_tv_0002` (5.3).
- Which sphere of the box-and-sphere shell gets which radius (section 6).
- **Believed**: an element with several geometries has its own space for collision grouping (2.2); the type-0
  shell is not activated for an object with a parent (4.2); the bone callback runs during the startup
  computation, so the bodies take the startup pose at spawn (7); bones without a callback keep their animation or
  bind pose relative to their parent (7, 8.1); the drawing blend uses the accumulator's remainder (8.2); for the
  trader's door, `lock` follows `door` and the object's transform stays *X* (10).

## 12. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (merged view); binary `bin\xrGame.dll`.
- Data examined: `spawns\all.spawn` (every physics-object, hanging-lamp and other record's blocks of `sav.md`
  7.4, their custom data), every model those records name (bones, bind pose, bone physics data, user data), every
  model file's user data.
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and instruction listings with Ghidra)
  of the physics object's spawn and shell build by type, the simple, item and box-and-sphere builds, the build
  from the bone data and its two entries, the element's body placement, geometries, masses and defaults, the
  drag and speed-limit code, shell activation, joint anchors, fixed bones, the custom-data and user-data physics
  keys and the collision pass's level test, the hanging lamp's and breakable's builds, the skeleton spawn and
  its saved-state defaults, the startup animation, the bone callback, the per-frame transform update; constants
  read from the binary. No execution or debugging; the game was not run. The checks in
  `checks/physics-shells/` reproduce every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The startup animation `$editor` (`scope`'s PH-3 entry): one physics-object record with a model that has motions names it, a name that model's motions are not shown to include; what playing a startup animation the model lacks does is not stated (section 7).
- A breakable's shell: its static geometry at spawn, and how the shell built when it breaks is placed, given its mass and activated, are not described (section 1).
- Whether sphere and cylinder geometries get a minimum size, as a box's half sizes do (0.005), is not stated (2.2).
