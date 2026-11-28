# A shell whose root bone has no shape: the trader's door

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of how the shell of a
physics object of physics type 3 is built when its model's root bone has no used shape, as the trader's door
(`trader_door`, `l01_escape`) has; which bodies the door's hinge joins and what that means for its angle,
stops and motor; whether the door's shell rests (sleeps) while it is held closed; whether a resting shell is
still collided with the player's body and woken by it; what `set_nonscript_usable(false)` changes; and whether
the record's mass matters for a type-3 shell. It fills the review gaps of `docs/behavior/trader-door.md` and
corrects statements of the specs it builds on (section 7).

Builds on (not repeated): `docs/behavior/physics-scope.md` (cited as `scope`; terms *body*, *shell*, *joint*,
*step*, *scripts*), `docs/formats/bone-physics.md` (cited as `bone`; *used shape*, joint types, fixing),
`docs/behavior/physics-world.md` (cited as `world`; *object*, *group*, *active* objects, the rest pass, the
exact and iterative solvers), `docs/behavior/physics-library.md` (cited as `library`; *public 0.5.0*),
`docs/behavior/trader-door.md` (cited as `door`), `docs/formats/level-cform.md` (cited as `cform`; the
player's body), `docs/formats/sav.md` (record layouts).

Conventions:

- The **door object**, the **door**, the **frame** are `door`'s: the record `trader_door`, the model bone
  `door`, the model bone `link`. An **element** is one body of a shell together with the shapes on it (a shell
  has one element per body). A body is **switched off** when the library's disabled flag is set on it
  (`library` 3.3); a shell **rests** when it is taken out of the world's active objects because all its bodies
  are switched off (`world` 4.4).
- The **collision index** is the spatial index of physics objects that the collision passes query (`door`
  3.2: "the physics objects' spatial index").
- Library terms (body 1, body 2, stops, motor, parameters) are public 0.5.0's.
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements
  are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing a statement: functions of `checks.py` in the dirty workspace's
  `checks/physics-shell-shapeless-root/` (`python3 checks.py` runs them all).

## 1. Building the shell

### 1.1 No fatal error

A physics object of physics type 3 builds its shell through a build from the bone data that tests nothing
about the root bone; the fatal test "No physics shapes was assigned for model or no shapes in main root bone!!!"
(the shell's first element has no shape) belongs to a second build entry, which the physics object's build does
not use [B1, B1b, B1c]. So a type-3 physics object whose model's root has no used shape is built normally.
Which other objects build through the checking entry is PH-3's (**unestablished** here).

In the data, 63 of the 475 `physic_object` records of type 3 have such a model (7 models:
`physics\door\door_traders_box` 39, `codedoor_left` 8, `door_trader` 6, `door_prison_01` 4,
`codedoor_right` 4, `physics\decor\rupor` 1, `krisagenerator` 1), and every one of the 63 names that root among
its fixed bones [D1].

### 1.2 The root's element

Bones are built from the root down (`bone` 3.1). A bone joins the body above it (no element of its own) when it
has no used shape or is rigid (joint type 0) **and** there is a body above it. The root has no body above it, so
it always gets an element of its own, whatever its shape and joint type; the element gets a shape only when the
bone has a used shape [B2]. Each new element is appended to the shell's elements before its children are built,
so the root's element is the shell's **first element**; a joint is made only for a bone with a body above
it [B2b].

For `door_trader.ogf` (`link` root, shape 0, rigid; `door` box, joint type 2, mass 100; `lock` no physics)
[D2b] the shell therefore has two elements: the frame's (no shape) and the door's (the door's box; `lock` adds
nothing, `bone` 3.1), and one joint (section 2).

### 1.3 An element without shapes is fixed

When a shell is activated each element's body is made; an element without shapes is then **fixed** instead of
given a mass, exactly as fixing a bone fixes it: its fixed flag is set and its body gets a sphere mass adjusted
to a total of 10⁸, gravity off, zero linear and angular velocity, force and torque (`door` 2.4) [B3, B3c]. It
puts nothing into the shell's collision space, but its body is in the shell's body list and is stepped with it
[B3b]. So the frame's body is fixed from its creation, carries no collision shape and takes part in no contact;
nothing pushes or turns it (**believed**: no joint acts on it, section 2.1, and it has no gravity or velocity,
so it stays where it was made).

The shell's first element is this body. The floor test of `world` 4.5 reads the first element's body [B5], so
for this door it tests the fixed frame, which never falls.

### 1.4 The record's fixed bones

The record's fixed-bone list (`link`) names, for each bone, the element that bone ended in: its own, or the one
above it that it joined [B4b]. `link` names the frame's own element, which is fixed again (the same values)
after activation (`door` 2.4) [B4].

When the record names at least one fixed bone, the shell also sets, once, its request for the **exact solver**
(`world` 4.2) [B4, B4d]. Each step a group's own requests become its marks for that step, and a group marked
for the exact solver with fewer than 30 joints (contacts included) is solved by the exact stepper [B4c]. So the
door's shell, alone in its group, is solved by the exact stepper (it has one joint plus its contacts), not the
iterative one.

## 2. The door's hinge

### 2.1 Which bodies it joins

The door's joint is a hinge (`bone` 3.2) made with the frame's element as its first element and the door's as
its second [B6]. Activating a shell makes every element's body first and the joints after [B7b], so when the
hinge is made the frame's element is already fixed (1.3). For a hinge whose first element is fixed, or has no
body, the game passes **no first body** and the **negated axis** to the library [B7]. The hinge therefore joins
the **door's body to the static environment**; the frame's body is joined by nothing.

In the library (public 0.5.0, unchanged here, `library` 5.1) attaching with no first body makes the door's body
body 1 and marks the joint reversed [P1]. Its stops (the bone's limits, −1.9199 … 0, passed unchanged [B7d]) and
its motor act on the angle of body 1 about the axis as stored in body 1's frame; the angle getter returns that
angle negated [P1].

### 2.2 What that gives (derived from P1, B7, B7c, B7d)

With φ the door's rotation about the bone data's hinge axis (`bone` 3.2: the bone's y axis), measured from its
pose when the hinge was made (φ = 0; that this is the placed pose is `door` 2.3's belief), right-handed about
that axis as built:

- the stops act on −φ: the door turns only within φ = 0 … 1.9199 (as it would if the frame were body 1: the
  negated axis cancels the swap);
- a motor velocity v drives φ at −v: `door`'s closing motor (velocity +1) drives the door toward φ = 0, its
  opening motor (velocity −3) toward 1.9199;
- the script's hinge angle (`get_axis_angle`, `door` 2.3) is φ, from 0 to 1.9199; the script's limits for this
  hinge come back as (−0, 1.9199), as `door` 2.3 states, because its first element is fixed [B7c].

So `door`'s "closed" test (angle ≤ the low value received, −0) holds only when φ ≤ 0: at the placed pose, or
pressed past it by the motor within the stop's softness; not "at any angle between the stops" (correction to
`door` 2.3, section 7).

### 2.3 No motor at creation

Every joint built from the bone data has its maximum force and velocity set to 0 on all its axes at build
[B6b], and the hinge gets motor parameters only for a maximum force above 0 [B6c]. So the door's hinge has no
motor until the scheme sets one (`door` 2.3, 2.4, settling their belief).

## 3. Resting while held

### 3.1 It starts at rest

A type-3 physics object's shell is activated **switched off** when bit 0x01 of the record's skeleton flags
(`sav.md` 7.4.1) is clear [B8, B8b]: every body is switched off after it is made and the shell is not put among
the world's active objects (it is not stepped) [B8c]. The door's skeleton flags are 0 [D2], so its shell rests
from the moment it exists. (349 of the 475 type-3 records have the bit clear, 126 set [D2c].) Either way the shell is
entered in the collision index with its "to be collided" mark set (4.1) [B8d].

The first `set_max_force_and_velocity` of the scheme (`door` 2.2) wakes it: setting a joint's maximum force and
velocity switches every body of its activated shell on and makes the shell active [B9] (`door` 2.3). Until then
the door does not move.

### 3.2 Whether it comes to rest again

After each step every element whose body is switched on runs the rest test; when no body of the shell is left
switched on, the shell leaves the active objects (`world` 4.4) [B10]. The test runs on the fixed frame's body as
on the door's, and a body with a force or torque in its accumulator at the test does not rest [B10b]. Nothing in
the door's build changes its rest parameters: they are the defaults of `scope` 1.1, the model having no
`[disable]` (no user data) [B10c, D2b]. A motor driving into its stop adds its maximum torque to body 1 while the
step's rows are built, and both steppers clear the accumulators at the end of the step [P2], so the closing
motor's torque is not seen by the rest test.

So nothing keeps the held door's shell awake by construction. Whether its bodies meet the rest test while the
motor presses the door on its stop (the rest test's own rule is PH-5's) is **unestablished** statically
(**believed**: yes, both bodies being still).

## 4. A resting shell and the player's body

### 4.1 It is still collided

An object's collision pass (the player's body and shells share it) collides it with every object of the
collision index whose box overlaps its own and whose **"to be collided" mark** is set, subject to the collision
classes, and ends by clearing its own mark [B11b]. The mark is set whenever an object's box is updated: after
every step it was active in, after its post-step work (in which a shell that has come to rest leaves the active
objects) [B11], and when the shell is entered in the index [B8d]. Only the object's own pass clears it [B11c],
and only active objects run passes [B11d].

So a resting shell keeps its mark set, and every active object whose box overlaps its box (as last updated) is
collided with it at every step: the player's body is collided with the resting door exactly as with an active
one (`door` 3.1, 3.3), at every step its box overlaps the door's. (Correction to `door` 3.2, which takes only
active objects.)

### 4.2 It is woken by a kept contact

When a pair yields at least one contact joint, the two groups are merged for the step and, if the other object
is not active, it is made active [B12]: taken off the resting list (`world` 4.4) and appended to the end of the
active objects, so it runs its own collision pass later in the same step and is solved with the merged group
[B12c]. The count is that of contact joints made; a contact the callbacks drop makes none [B12b]. So:

- the player's bottom sphere, torso cylinder or head sphere touching the resting door wakes it in that step
  (`door` 3.1: their contacts are kept);
- the outer sphere's overlap alone does not (its contacts only set the "blocked" flag, `door` 3.1);
- the door's bodies are stepped from then on; that stepping the merged group switches its bodies back on is
  `library` 4.1's rule (every listed body's disabled flag is cleared).

### 4.3 The merge and the solver

A merge adds the other group's own requests to the colliding object's own requests, which the per-step copy
keeps from then on [B13, B4c]. So when the player's pass merges the door's group (requesting the exact solver,
1.4), the player's body itself requests the exact solver from then on (**believed** consequence: from that step
on, every group the player's body heads with fewer than 30 joints is solved by the exact stepper). Which solver
solves a merged group in the step of the merge, and in later steps, depends on which object's group heads it
(**unestablished**).

## 5. `set_nonscript_usable`

Every game object carries a flag "usable not only through its script", set when the object is made;
`set_nonscript_usable(b)` stores `b` in it [B14]; the hint text (`door` 5) is kept beside it [B14b]. The flag
is read by the player's **use action** (action `use`, 42 in the key-action table, `door` 2.5) [D5], which, for the
object currently under the crosshair (`door` 5.1) [B15, B15c]:

1. runs that object's use callback (script callback 15, `door` 2.5's use) **whatever the flag**;
2. if an inventory box is under the crosshair and its flag is set, opens it, and does nothing else;
3. otherwise, **only if the object under the crosshair has its flag set** (or there is none): for a character
   under the crosshair, when its health is at most 0 and left Shift is not held, opens its search window, and
   when its health is above 0 does another action (**unestablished**); then, for the object the crosshair's
   physics pick reaches, without left Shift sends the take event when the object allows taking, and with left
   Shift starts dragging the picked bone when the object's model is listed in `[ph_capture_visuals]`
   (`scope` 1.4, PH-16) [B15b].

The hint shown for the object reads only its hint text, not the flag [B15c]. A physics object is no character
and allows no taking, while the pick does accept it [B16]; `[ph_capture_visuals]` lists no door and no model under
`physics\` [D6]. So for the trader's door `set_nonscript_usable(false)` changes nothing the player can see: the
use callback still runs, and the steps it skips would do nothing for this object (**believed**: no other reader
of the flag; no other instruction reading the flag directly was found [B15d]).

## 6. The record's mass

The record's physics type and mass are copied to the object at spawn [B17]. Types 0, 1 and 2 pass the mass to
the shell's build; type 3 does not read it [B17b], and no other reader was found in the physics object's code
[B17c]. So the trader door's record mass (10) [D2] has **no effect**: the door's body has the bone data's mass
(100, `bone` 5), the frame's body the fixed 10⁸ (1.3).

## 7. Corrections to approved specifications

- `scope` 1.2 and `bone` 8 ("a model whose root bone has no shape is fatal", for physics type 3): the type-3
  build of physics objects has no such test; the root gets its own shapeless, fixed element (1.1 – 1.3). The
  fatal test belongs to another build entry.
- `world` 4.2 ("no code sets the exact-solver mark"): naming fixed bones in a physics object's record sets it
  (1.4); a merge carries it into the colliding object's requests (4.3).
- `door` 2.3: the hinge's first element is the frame's own element, fixed at activation (settling its belief);
  the script's hinge angle runs 0 … 1.9199, so "closed" holds only at the placed pose (2.2); no motor before the
  scheme (2.3).
- `door` 2.4: the frame (`link`) is its own shapeless element (1.2); the hinge joins the door's body to the
  static environment, not to the frame's body (2.1).
- `door` 3.2: resting objects are collided too (4.1).

## 8. Unestablished and believed

- Which objects other than type-3 physics objects build their shells through the checking entry (1.1).
- Whether the door's shell comes to rest while held closed (3.2; believed yes); the rest rule itself (PH-5).
- Whether any other per-frame work of the door object wakes its shell while locked.
- Which solver solves a merged group, by which object heads it (4.3); believed: the player's body requests the
  exact solver after merging with the door.
- The action the use action runs for a living character (5).
- **Believed**: the frame's body never moves (1.3); no other reader of the "nonscript usable" flag or of the
  record's mass (5, 6); that φ = 0 is the placed pose (2.2, `door` 2.3).

## 9. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (merged view); binary `bin\xrGame.dll`.
- Data examined: `spawns\all.spawn` (every `physic_object` record's physics-object block, `sav.md` 7.4.2, and
  skeleton flags, 7.4.1), the models those records name (bone data, `bone`; user data), `config\system.ltx`
  (`[ph_capture_visuals]`); public ODE 0.5.0's sources for the attach rule, the hinge angle, stops and motor rows
  and the steppers' clearing of accumulators.
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and instruction listings with Ghidra) of
  the physics object's shell build by type, the build from the bone data and its two entries, element and joint
  activation, fixing, the hinge's creation, the joints' defaults, the group requests and the solver choice, the
  rest test and resting, the collision pass and its marks, the pair collision, merging and waking, the script
  functions `set_nonscript_usable` and the joint motor setter, the player's use action and crosshair record,
  the key-action table and the spawn reading of the physics-object fields; constants read from the binary. No
  execution or debugging; the game was not run. The checks in `checks/physics-shell-shapeless-root/` reproduce
  every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The use action's steps before the use callback (an early exit in one state of the player, and a step taken in another) are not described (section 5).
- The character part of the use action runs only under a further condition on the game's mode, and opening a dead character's search window has one more condition; neither is described (section 5).
- The search for other readers of the "usable not only through its script" flag covered reads addressed from the object only, not reads through the object's usable part, which is how the use action's own gate reads it (sections 5 and 8).
- Whether a shell activated switched off (3.1) is put on the world's list of resting shells (`world` 4.4), and so handled by its rest pass before it has first been stepped, is not stated.
