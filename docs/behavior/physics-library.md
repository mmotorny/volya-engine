# The shipped physics library

Functional specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the physics library
the game ships as `bin\ode.dll` (`physics-scope` 1.1, 5): which public release of the Open Dynamics Engine
(ODE) it is built from, its build configuration, every way it differs from that release (each added or
changed export and each changed behaviour, stated so that the public release can be patched to match), the
floating-point environment it runs in, what it depends on and keeps as global state, and whether a step is
repeatable. It extends `docs/behavior/physics-scope.md` (section 5) and is read with
`docs/behavior/physics-world.md` (PH-2), which states how the game drives the library.

It builds on (not repeated):

- `docs/behavior/physics-scope.md` (cited as `scope`): terms *body*, *shell*, *joint*, *step*; the library's
  use by the game (1.1);
- `docs/behavior/physics-world.md` (cited as `world`): the library-wide parameters and their values in play
  (1.3), the groups and the solver choice (4.2), the threads the physics runs on (2.1);
- `docs/formats/level-cform.md` (cited as `cform`): the contact properties (4.2).

Conventions:

- **Public 0.5.0** is ODE 0.5.0 as tagged `0.5.0` (2004-05-29) in the public ODE repository
  (`bitbucket.org/odedevs/ode`), its source and its Windows export list `config/msvcdefs.def`. Public ODE's
  own API names, parameter names and concepts (ERP, CFM, QuickStep, SOR, LCP, `dContact`, ...) are used;
  they are the library's published interface. Nothing is named after the shipped program's internals.
- Values are single precision unless stated. "h" is the step length passed to a step function.
- Statements are **static** (read from the binaries; the game was not run) unless marked otherwise.
  Inferred statements are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are checks in the dirty workspace's `checks/physics-library/` (`python3 run_all.py` runs
  them all; `odever/checks/export.py` re-creates their evidence): `I*` identity, build, world, bodies; `S*` steppers;
  `J*` joints; `C*` collision; `F*` floating-point environment and global state.

## 1. Identity

**Base release: public 0.5.0.** The shipped library exports 316 names. They are exactly the 309 names of
public 0.5.0's export list without the triangle mesh, minus 8 and plus 15 [I1]:

- not exported: `dDebug`, `dError`, `dMessage`, `dNormalize3`, `dTestMatrixComparison`, `dTestRand`,
  `dTestSolveLCP`, `dWorldExportDIF`;
- added: `dCollideRayBox`, `dCollideRayCCylinder`, `dCollideRaySphere`, `dGeomMoved`, `dSetAllocHandler`,
  `dSetFreeHandler`, `dSetReallocHandler`, `dxStepBody` (functions that exist in public 0.5.0 without being
  exported), and `dNormalize3_slow`, `dJointCreateContactSpecial`, `dJointSetFixedQuaternionPos`,
  `dWorldAddBody`, `dWorldRemoveBody`, `dWorldAddJoint`, `dWorldRemoveJoint` (not in public ODE) (sections
  3 – 6).

Fingerprints placing it at 0.5.0 and not later: it has 0.5.0's newest additions (the QuickStep solver and
its joint feedback, the contact parameter functions, auto-disable, StepFast with its auto-enable depth) [I2b];
it has none of the API public ODE added after 0.5.0 and before 0.8 (the next tagged release, 2007-02-11):
no plane-2D joint, no geometry offsets, no `dInitODE`, no capsule or flat cylinder, no damping [I1b, I2]; its
angular motor keeps a 0.5.0 behaviour that later public releases changed (section 5.1) [J11]. Whether it
was taken from the release itself or from the public source shortly after it is **unestablished** (the
export set and the compared code match the release).

**The file**: 196,608 bytes; linked 2008-02-27 with linker 8.0 (Microsoft Visual C++ 2005), against the
run-time library MSVCR80 (assembly version 8.0.50727.762); its debug record names `x:\binaries\ode.pdb`;
no version resource [I3]. The game's `bin\xrGame.dll` imports 142 of the exports, including 13 of the 15
added ones (all but `dCollideRayCCylinder` and `dJointSetFixedQuaternionPos`), and not `dWorldCreate`,
`dSpaceCollide` or `dHashSpaceCreate` [I18].

## 2. Build configuration

- **Single precision**: every real is 4 bytes (arguments, the 312-byte body, the 104-byte `dContact`) [I4].
- **Assertions compiled out** (public ODE's `dNODEBUG`): none of the argument, user or internal assertion
  messages and not the "inertia must be positive definite" message are present; the unconditional messages
  remain: "LCP internal error, s <= 0 (s=%.4e)", "setSize() out of space in LOCAL array", "too many user
  classes, you must increase the limit and recompile ODE", the quadtree space's message that
  fetching a geometry by index is not yet implemented, "not yet implemented", "num_bytes too large", "warning: destroying world containing grouped joints" [I5].
- **No triangle mesh**: no triangle-mesh exports, geometry class or OPCODE code; the geometry classes are
  sphere, box, capped cylinder, plane, ray, geometry transform and user-defined, and the spaces simple, hash
  and quadtree [I6, C4]. (GIMPACT postdates 0.5.0.) No DIF export and no test functions [I6].
- **Code generation**: arithmetic is scalar single-precision SSE except square roots, sines, cosines and arc
  tangents, which are x87 instructions (section 7.1) [I7, F1].
- **Messages** as public 0.5.0 on Windows: an internal error shows a message box titled "ODE INTERNAL ERROR
  n" (stop icon) and aborts, unless a handler is set; a message is printed to standard error as "ODE
  Message" [I15]. Without `dError` exported, no "ODE Error" text exists.
- **Work memory**: steppers and the LCP solver take their work arrays from the stack only (as public 0.5.0;
  0.5.0's optional heap variant is not built) [S14]. Memory for objects goes through the allocation handlers
  (section 3.4).

## 3. Worlds, parameters and bodies

### 3.1 A world is only a list

A world is 16 bytes: its body list, its joint list and their two counts [I8]. Every world parameter is one
**library-wide value**: the setters of gravity, ERP, CFM, QuickStep iteration count and relaxation factor
`w`, contact maximum correcting velocity, contact surface layer and the auto-disable values store into the
library's single copy and ignore the world they are given; the steppers and joints read those values
[I8, S3, S3b] (`world` 1.3 states the values in play). Public 0.5.0 keeps them per world [S3c].

Values when the library loads (before any `dWorldCreate`) [I9]: gravity (0, −1, 0); ERP 6/11 = 0.5454545 and
CFM 1/880,000 = 1.1363636 × 10⁻⁶ (the error reduction and mixing of `cform` 4.2 at h = 0.02); QuickStep 20
iterations, `w` 1.1; contact surface layer 0.001; maximum correcting velocity +∞ (set by a load-time
initialiser). `dWorldCreate` (not called by the game) resets them to gravity 0, ERP 0.2, CFM 10⁻⁵, 20
iterations, `w` **1.1** (public 1.3), surface layer **0.001** (public 0), maximum velocity +∞ [I10, S4,
S4b]. `dWorldDestroy` frees the world's bodies and the joints not in a group (warning for grouped ones) and
the 16 bytes [I8].

**Added functions** [I12, J4, J5]:

- `dWorldAddBody(world, body)`: puts the body at the head of the world's body list, sets its world and counts
  it. `dWorldRemoveBody(world, body)` takes it out, clears its world and counts down.
- `dWorldAddJoint(world, joint)` and `dWorldRemoveJoint(world, joint)`: the same for joints.

`dBodyCreate` and every `dJointCreate*` accept no world (a null world): the object is then in no list.
With a world they are linked at the head of its list as in public 0.5.0. `dBodyDestroy` and `dJointDestroy`
unlink only when the object has a world [I12, J4]. The game makes its joints without a world and adds them
with `dWorldAddJoint` [J4]; that the game uses worlds as the groups of `world` 4.2 (adding the bodies and
joints of touching objects to one world for a step) is **believed** from these functions and that section.

### 3.2 Gravity, bodies and their flags

Gravity is the single library-wide vector (section 3.1); each body's no-gravity flag works as public 0.5.0
(`world` 1.4) [S10d]. Body state, its setters and getters, force and torque accumulation, mass setting
(with the identity inverse inertia when the inertia is not positive definite, silently) and the
finite-rotation settings are as public 0.5.0 (believed from reading; the mass functions' constants are
checked [I17]). `dBodySetPosition`, `dBodySetRotation` and `dBodySetQuaternion` mark the body's geometries
moved as public `dGeomMoved` does [C22].

### 3.3 No auto-disabling

The body carries no auto-disable state: the per-body setters of linear and angular threshold, steps and time
do nothing and the getters return 0; the per-body flag is kept (cleared at creation and by
`dBodySetAutoDisableDefaults`) [I11, S2b]. The world's auto-disable values are stored but read by nothing
else, and no stepper auto-disables [I11, S2]. `dBodyEnable`, `dBodyDisable` and `dBodyIsEnabled` set and
read the disabled flag as public, but the steppers clear it (section 4.1).

### 3.4 Memory and messages

`dSetAllocHandler`, `dSetReallocHandler` and `dSetFreeHandler` are public 0.5.0's handlers, exported: when
set, every allocation, reallocation and release of the library goes through them (release with the block's
size), else through the run-time `malloc`, `realloc` and `free` [I13]. The game imports all three (**believed**: it sets them) [I18]. The message
and debug handlers are public 0.5.0's [I15].

### 3.5 Math helpers

`dNormalize3_slow` is public 0.5.0's `dNormalize3`: divide by the largest component's magnitude, then scale
by the reciprocal square root, the largest component taking the sign of the original; a zero vector becomes
(1, 0, 0) [I14]. Inside the library, wherever public 0.5.0 normalises a 3-vector (contact normals, joint axes
and rows, rotations), the shipped library instead takes the squared length `L` and, when `L ≥ 1.1920929 ×
10⁻⁵` (bit pattern 0x37480000), multiplies by `1 / √L`; only below that it uses `dNormalize3_slow` [C5, J10].
The results differ from public only in rounding. `dCloseODE`, `dWorldImpulseToForce` and
`dWorldSetAutoEnableDepthSF1` are as public [I16].

## 4. Stepping

### 4.1 What a step steps

`dWorldStep(world, h)` and `dWorldQuickStep(world, h)` hand the whole world to one routine with the exact or
the QuickStep stepper [S1]. That routine, when the world has at least one body, lists every joint of the
world's joint list and every body of its body list, each in list order (most recently added first), clears
every listed body's disabled flag and calls the stepper **once** with all of them [S1b, S1c, S1d, S15]. There
is **no island search and no auto-disabling**: public 0.5.0 first auto-disables, then starts islands only
from enabled bodies, gathers each island through joints and steps each island separately.

Patch consequences: a disabled body in a stepped world is stepped (and left enabled); bodies not connected by
joints are solved as one system; the stepper sees bodies and joints in list order. A joint in the list
attached to no body is passed to the stepper too (what then happens is **unestablished**; believed not to
occur in the game's use). `dWorldStepFast1` (not imported by the game) also takes the whole world as one
group without auto-disabling, and uses the exact stepper when the world has 3 joints or fewer; its own
stepper was not compared (**unestablished**) [S13].

### 4.2 The exact stepper (`dWorldStep`)

As public 0.5.0's exact island stepper (the one 0.5.0 uses): inertia in the world frame, the gyroscopic
term, gravity times mass for bodies without the no-gravity flag, joints with no rows dropped, rows ordered
unbounded / mixed / bounded, row defaults (`erp` = the library-wide ERP, `cfm` = the library-wide CFM,
bounds ±∞, no friction index), `A = J M⁻¹ Jᵀ` with `cfm / h` on the diagonal, right-hand side
`c / h − J (v / h + M⁻¹ f)`, the Dantzig LCP, the constraint force `Jᵀ λ` with joint feedback, integration by
`dxStepBody`, accumulators zeroed [S10, S10c, S10d, S10e]. The LCP solver is public 0.5.0's (friction bounds
`hi = |hi × λ(normal)|`, `lo = −hi`, fixed when the first friction index is reached; the six index-switching
cases; the bail-out at `s ≤ 0` with its message) [S11, S11b, S11c]; its internal helpers were compared in
structure only and are **believed** identical.

**Changed — non-finite guard.** After the constraint force plus the external force of a body is multiplied
by `h`, each of its 3 linear and 3 angular components that is NaN or ±∞ is replaced by 0 before it changes
the velocities (`lvel += invMass × value`, `avel += invI × value`) [S5, S5b, S5c]. (The test is the run
time's floating-point classification asked for NaN, ±∞ and double-precision denormals; a single-precision
value converted to double is never a double denormal, so in effect only NaN and ±∞ are replaced [S5b].)

### 4.3 The QuickStep stepper (`dWorldQuickStep`)

As public 0.5.0's QuickStep: the same preprocessing; rows with right-hand side
`c / h − J (v / h + M⁻¹ f)` and `cfm / h`; warm starting from each joint's six stored multipliers, stored
back after the solve; joint feedback; the external force applied as `lvel += h × invMass × f`,
`avel += invI × (h × τ)`; `dxStepBody`; accumulators zeroed [S6b, S10b].

**Changed — non-finite guard and warm-start reset.** Each of the 6 components per body of the solved
constraint acceleration (`M⁻¹ Jᵀ λ`) that is NaN or ±∞ is replaced by 0 before `h ×` it is added to the
velocities; if any component of any body was replaced, the six stored multipliers of every joint of the step
that has rows are set to 0, so the next step does not warm-start from them [S6, J15]. The external-force part
is not tested [S6c]. (`J15` also counts denormals; as in section 4.2, for single-precision values only NaN
and ±∞ are affected [S5b].)

The SOR solver, as public 0.5.0: the stored multipliers scaled by 0.9 to warm-start; per-row
`Ad = w / (J M⁻¹ Jᵀ + cfm)`; the row update `delta = b − λ Ad − J · fc`; rows without a friction index before
those with one; friction bounds recomputed at each visit from the current normal multiplier; `w` and the
iteration count library-wide [S9, S9b, S9c, S9d]. Changed:

- **Reshuffle period and generator.** The row order is reshuffled at iterations 0, 4, 8, … (public: every
  8th) [S7]. Each reshuffle runs `k = 1 … m − 1` (m rows), drawing `j = r mod (k + 1)` and swapping entries
  k and j, where r is the C run time's `rand()` (15-bit values; when `k + 1` exceeds 32,767 further values are
  appended 15 bits at a time until the range covers `k + 1`) [S7b]. Public uses the library's own
  `dRandInt`; the shipped shuffle neither uses nor advances that generator, so `dRandSetSeed` does not
  affect it [S7c]. The run-time generator is section 8.
- **Clamping.** The new multiplier is kept when `lo ≤ new ≤ hi`; otherwise it becomes `hi` when it is not
  below `lo`, else `lo`. For finite values this equals public 0.5.0; a NaN becomes `lo` (public keeps it)
  [S8].

### 4.4 `dxStepBody` (exported)

As public 0.5.0: position `+= h × lvel`; rotation by the infinitesimal quaternion update, or by the finite
rotation (whole angular velocity, or its component along the finite-rotation axis plus an infinitesimal
update for the rest), with `sin(x) / x` replaced by `1 − x² / 6` below `|x|` = 10⁻⁴; the quaternion
renormalised, or (1, 0, 0, 0) when of zero length; the rotation matrix rebuilt; every attached geometry marked
moved [S12, S12b, S12c, S12d]. The game calls it directly [I18].

## 5. Joints

### 5.1 As public 0.5.0

Kinds and `dJointGetType` numbers: ball 1, hinge 2, slider 3, contact 4, universal 5, hinge-2 6, fixed 7,
null 8, angular motor 9; no `dJointCreateNull` export [J1]. The rows of every kind (including 0.5.0's slider
quirk of writing the half-offset cross terms only into the second body's angular part, the universal joint's
second limit row on the unmodified body-2 axis, the hinge-2 suspension rows), the limit and motor rows (power,
fudge factor, equal stops, stop ERP and CFM, bounce), the parameter numbering and set rules, the angle, rate,
anchor and axis getters, `dJointAdd*`, `dJointAttach` (a missing first body swaps the bodies), data and
feedback, `dAreConnected` and `dAreConnectedExcluding` and joint-group memory (arenas of 16,384 bytes) are as
public 0.5.0 [J1, J6, J8, J12, J13, J14, J16]. The angular motor in user mode keeps 0.5.0's behaviour for an
axis set relative to body 1: the axis is stored in body 1's frame and then used as if it were a world axis
(only axes relative to body 2 are rotated); public 0.8 rotates it. `dJointGetAMotorAngleRate` is
unimplemented (internal error) as in 0.5.0 [J11]. No functions for attaching joints to moving geometry [J1].

### 5.2 Changed

1. **Worlds**: section 3.1 (made without a world, added and removed by the new functions) [J4, J5].
2. **Emptying a joint group** detaches each grouped joint that has a world from its bodies (newest first)
   but leaves it in the world's joint list and count (public 0.5.0 removes it from both); joints without a
   world stay attached; the group's memory is then reused [J6].
3. **Defaults from the library-wide values**: a new joint's normal CFM and stop CFM are the library-wide
   CFM, its stop ERP the library-wide ERP, and a hinge-2's suspension ERP and CFM the same, taken when the
   joint is made; later changes do not reach existing joints [J7].
4. **Contact normal row**: right-hand side `min(maxvel, (1 / h) · e · max(0, depth − layer))`, `e` the
   contact's soft ERP when `dContactSoftERP` is set, else the library-wide ERP; `layer` and `maxvel` the
   library-wide surface layer and maximum correcting velocity; bounce as public 0.5.0 (when `bounce_vel ≥ 0`
   and the approach speed exceeds it, raised to `−bounce × outgoing` if larger). The formula is 0.5.0's,
   which reads the joint's world [J12]. With the values in play (`world` 1.3: layer 0.001, no limit) this is
   `world` 1.3's rule, which holds for both contact kinds.
5. **`dJointCreateContactSpecial(world, group, contact)`** (added): a contact joint made and stored like
   `dJointCreateContact` (the whole `dContact` copied), type number 4 like it (`dJointGetType` cannot tell
   them apart), with the same row count and rows, except that the normal row's upper force bound is
   **100,000** instead of +∞: the normal constraint force of a special contact is capped at 100,000 (the
   library's force units); friction, softness, bounce and motion are unchanged [J2, J3]. This is the second
   kind of contact joint of `scope` 1.1.
6. **`dJointSetFixedQuaternionPos(joint, q, pos)`** (added; not used by the game): for a fixed joint with a
   first body and no second, sets the joint's reference rotation to the conjugate of `q` (w, −x, −y, −z) and
   its reference offset to `pos`, the state `dJointSetFixed` would give for that body if its orientation were
   `q` and its position `pos`, so the body is held at that pose; with two bodies or none it does nothing [J9].
7. **Normalisation** inside joints: section 3.5 [J10].

## 6. Collision

### 6.1 As public 0.5.0

The class numbers (sphere 0, box 1, capped cylinder 2, cylinder 3, plane 4, ray 5, transform 6, triangle
mesh 7, spaces 8 – 10, user classes 11 – 14) and the 15 × 15 collider table, filled as public (no colliders
for classes 3 and 7) [C1, C2]; `dCollide` (no contacts for the same geometry or two on one body; reversed
entries swap and negate; the caller's stride; the flags' low 16 bits the maximum count) [C3, C10]; the
sphere, plane, capped-cylinder and ray colliders, box–plane, the face cases of box–box, the transform
collider, `dBoxTouchesBox` and the closest-point helpers [C9, C10, C11, C17, C19]; geometry creation,
placement, destruction, bounding boxes (planes and `dInfiniteAABB` from −∞ to +∞), enabling, category and
collide bits and `dGeomMoved` [C18, C22, C24]; user classes (`dCreateGeomClass`, `dCreateGeom`,
`dGeomGetClassData`: at most 4, the class record, zeroed class data, the collider looked up on first use and
cached both ways) [C14] — the game-defined geometry kinds of `scope` 1.1 are such user classes, whose
behaviour is the game's (PH-4); the simple, hash and quadtree spaces and their pair filter, `dSpaceCollide2`
and the space functions [C15, C16, C22]; `dCloseODE` [C23]. `dCollideRaySphere`, `dCollideRayBox`,
`dCollideRayCCylinder` and `dGeomMoved` are public 0.5.0's functions, exported unchanged (the collider table
uses the same ones) [C21]; the game calls the first two and `dGeomMoved` directly [C20]. The ray's closest-hit
flag is stored and read back only (as in public, where only triangle-mesh colliders read it) [C13].

### 6.2 Changed

1. **Normalising contact normals** (sphere–box with the centre outside, the ray–sphere helper used also by
   capped-cylinder caps, the ray–capped-cylinder side hit): section 3.5's rule [C5].
2. **Box–box edge–edge contacts.** When the separating axis is an edge–edge axis (codes 7 – 15), public
   0.5.0 returns one contact: the midpoint of the closest points A (on box 1's edge line) and B (on box 2's),
   depth d. The shipped library returns that contact first and then appends up to six more, so 1 to 7
   contacts, **not** limited by the maximum the caller asks for; all carry the first contact's normal and
   geometries [C6]. With box i's centre pᵢ, axes the columns of Rᵢ, side lengths sᵢ, n the separating normal
   as the box–box routine computes it (from box 1 to box 2, before the collider negates it), k₁ = (code −
   7) div 3 and k₂ = (code − 7) mod 3 the two boxes' edge axes:
   - For each box, of its other two axes a = (k + 1) mod 3 and b = (k + 2) mod 3, M is the one with the larger
     `|n · axis|` (b when equal) and S the other [C25].
   - A candidate (face j of a box, sign σ, the line through P along unit u): face centre
     `F = p + σ (s[j] / 2) axis_j`, with p, s and the axes the face's box's, except that candidates 5 and 6
     take p from box 2's centre p₂ (`F = p₂ + σ (s₁[k₁] / 2)` box 1's axis k₁) [C27];
     `t = (axis_j · P − axis_j · F) / (axis_j · u)`; `Q = P − t u`; accepted
     when for the box's other two axes c, e: `|axis_c · Q − axis_c · F| ≤ s[c] / 2` and likewise for e [C7].
   - The candidates in order: (1) box 2, face S₂, σ = +1 if `S₂ · (p₂ − A) > 0` else −1, line through A along
     box 1's axis k₁; (2) box 2, face k₂, σ = +1, same line; (3) box 2, face k₂, σ = −1, same line; (4) box 1,
     face S₁, σ = +1 if `S₁ · (p₁ − B) > 0` else −1, line through B along box 2's axis k₂; (5) box 1, face k₁,
     σ = +1, face centre from p₂; (6) box 1, face k₁, σ = −1, face centre from p₂ [C8, C26, C27].
   - An accepted candidate is written as the next contact at Q with depth
     `d − |(n · M) ((Q − P) · M)|`, M of the box whose face it is, P the line's point; it counts only when
     that depth is above 0; an accepted candidate of depth ≤ 0 is written into the next free slot and
     overwritten by the next counted one [C9]. A caller must therefore have room for 7 contacts whenever two
     boxes can touch. A line parallel to the face gives a non-finite `t` and fails the tests (**believed**).
3. **`dGeomRaySet(ray, start, dir)`**: public 0.5.0 writes the start into the position and the direction
   into the third column of the rotation as given. The shipped library sets the position as
   `dGeomSetPosition` does and a whole rotation built by `dRFromZAxis` from the direction (third column the
   normalised direction, the first two from `dPlaneSpace`) as `dGeomSetRotation` does — both through the body
   when the ray is attached to one [C12]. A non-unit direction is normalised; a ray on a body moves and turns
   the body.

## 7. Floating-point environment

### 7.1 Instruction mix

The library computes in scalar single-precision SSE (17,395 such instructions, 1,466 packed moves and
logic, 2 double-precision SSE, 3,021 x87) [F1]. All square roots (43), sines (8), cosines (8) and arc
tangents (7) are x87 instructions; no SSE square root or reciprocal estimate is used [F1]. The x87 also
computes the reciprocal square roots and the products of the 4-vector normalisations (`dNormalize4`,
`dxStepBody`'s renormalisation), of the fast path of section 3.5's normalisation, the box–box separation
tests, some collider distances and joint getters (as `dJointGetSliderPosition`); these x87 results are not
flushed (section 7.2) [F1c]. Per part [F1]:

- the SOR row loop: SSE only (3 x87 instructions: a sign change and a store);
- the exact and QuickStep steppers: SSE, with x87 for the square roots and the sine and cosine of the
  integration they contain;
- `dxStepBody`: SSE, with x87 for the angular speed's square root, the sine and cosine of the finite
  rotation, `sin(x) / x` and the quaternion's renormalisation.

`bin\xrGame.dll` mixes both: 90,747 scalar SSE and 26,878 x87 instructions over the program; its code that
makes contacts (calling `dCollide`, `dJointCreateContact`, `dJointCreateContactSpecial`) is SSE only, its
code that computes forces and calls the step mixes both [F1b].

### 7.2 Control word and MXCSR while physics steps

- The engine's core library (`bin\xrCore.dll`) at process start sets the x87 control word to 53-bit
  precision, round to nearest, all exceptions masked, then prepares control words for precision 24, 53 and
  64 bits with rounding to nearest or towards zero [F3].
- At start-up for the main thread and at the start of every thread the engine creates (among them the
  secondary frame thread that runs the physics, `world` 2.1), it loads the **24-bit precision, round to
  nearest, exceptions masked** control word; and when the processor reports SSE (and the command line does
  not carry `-x86`) it sets MXCSR **flush-to-zero** and then, unless the processor faults on it, **denormals
  are zero** [F3, F3b]. The main thread reloads the 24-bit, round-to-nearest word at every frame's start;
  `bin\xrGame.dll` changes precision only around one timer conversion (to 64 bits and back to 24) [F3b].
  The library itself never changes the control word or MXCSR [F3b].
- The Direct3D 9 device is created with hardware, pure-hardware or software vertex processing and the
  multithreaded flag, **without** FPU preservation [F2]. Which control word Direct3D then sets is Direct3D's
  behaviour (not read here); since the engine already holds 24-bit precision with rounding to nearest on
  both threads, the physics runs under that word either way (**believed**).

So while physics steps (static; **not measured** in a run — a run under the container's Wine would show
Wine's and its Direct3D layer's behaviour, not Windows'): x87 at 24-bit precision, round to nearest,
exceptions masked; SSE with round to nearest, flush-to-zero and denormals-are-zero (on processors with SSE
and denormals-are-zero; **believed** every processor the game runs on today). Consequences (**believed**):
denormal inputs and results of the SSE arithmetic are 0; the x87 square roots, quotients and products round
to single precision as SSE would (no extended intermediate precision), and x87 does not flush denormals;
the transcendental instructions are not affected by the precision setting.
**Disputed in review:** "round to single precision as SSE would" holds only in the normal range: the x87's
exponent range is wider, so an x87 result below the normal range is rounded twice, to 24 bits and then to the
denormal grid when stored (`docs/behavior/physics-harness-3.md`, its definition of x87).

## 8. Dependencies, global state and repeatability

**Self-contained.** The library imports only the C run time (MSVCR80: `malloc`, `realloc`, `free`, `rand`,
`_fpclass`, `_copysign`, `floor`, `pow`, `ldexp`, `frexp`, `memcpy`, `memmove`, `memset`, formatted output),
`MessageBoxA` and the kernel functions of the run time's start-up and failure handling (the start-up reads
the clock only to seed the run time's own security value); no other time source, no thread or file use, no
`srand` [F4]. `rand` is called only by the QuickStep row shuffle and StepFast,
`_fpclass` only by the two steppers' non-finite guards, `pow` and `floor` only by space code (**believed**: the
quadtree and hash spaces) [F4b]. So its exports can be driven in isolation by any 32-bit Windows program (or under a Windows
compatibility layer) through the public API, `dWorldCreate` or the added world functions included
(**believed**; not done). To reproduce the game's results such a harness must set the floating-point
environment of section 7.2 and the run-time generator state below on the stepping thread.

**Global state** [I8, I9, I13, I15, C14, C23, S7c, F5]: the library-wide parameters (section 3.1), the
collider table and the user classes (reset by `dCloseODE`), the allocation and message handlers, the
library's own random seed (`dRandSetSeed` / `dRandGetSeed`; not used by the steppers), and — outside the
library — the run time's generator state used by the shuffle.

**The shuffle's random source.** The run time's `rand` keeps its state per thread: each thread starts at 1,
and each call sets `s = s × 214,013 + 2,531,011` and returns `(s >> 16) & 32,767` [F5]. Neither the library
nor `bin\xrGame.dll` seeds it; `srand` is called by `bin\XR_3DA.exe` only while setting up its noise tables,
and by `bin\xrLUA.dll` and `bin\xrParticles.dll` (**believed**: the scripts' random seeding and the particle
system) [F5]. Other users of the run-time `rand` are one function of `bin\xrGame.dll`, the scripts' random
numbers (**believed**) and the particle system [F5]. A QuickStep step draws at least `m − 1` values per
reshuffle and reshuffles at iterations 0, 4, 8, … (section 4.3).

**Repeatability.** Given the same inputs, the same floating-point environment and the same generator state
on the stepping thread, a step is a deterministic function of them (**believed**: no time, threads or
uninitialised data enter it). In the game the generator state is not under control: it starts at 1 per
thread, is advanced by every step and by any other run-time `rand` user on the same thread, and the physics
runs on the secondary thread or, when that thread has not run it for a frame, on the main thread, each with
its own state (`world` 2.1). Run-to-run identity of the game's physics is therefore **unestablished**
(**believed** not guaranteed). Which threads the other `rand` users run on is **unestablished**.

## 9. Corrections to earlier specifications

- `scope` 5 and 6: the second kind of contact joint is a contact whose normal force is capped at 100,000
  (section 5.2); the game-defined geometries are public 0.5.0's user classes, whose behaviour the game
  supplies (section 6.1).
- `world` 1.3: confirmed for both contact kinds (section 5.2, item 4); the library's initial ERP and CFM
  (section 3.1) are overwritten by the game's world creation (`world` 1.2).

## 10. Unestablished

- Whether the base was the 0.5.0 release itself or public source of shortly after (section 1).
- What a stepped joint attached to no body does; StepFast's own stepper (section 4.1).
- The LCP solver's internal helpers beyond their structure (believed identical, section 4.2).
- A line parallel to a box face in the box–box edge–edge contacts (believed: no contact, section 6.2).
- The floating-point environment measured in a run on Windows; the processors that fault on
  denormals-are-zero (section 7.2).
- Which threads the other run-time `rand` users run on; run-to-run repeatability (section 8).

## 11. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\ode.dll`,
  `bin\xrGame.dll`, `bin\xrCore.dll`, `bin\XR_3DA.exe`, `bin\msvcr80.dll` and the import tables of every
  binary in `bin\`.
- Public material: ODE 0.5.0 and 0.8 as tagged in the public ODE repository (source, export lists,
  change log), compared with the shipped library; header constants of Direct3D 9 and the C run time's
  `float.h` (from Wine's public headers, read only) and the SSE control bits (from the compiler's public
  intrinsics headers).
- Analysis: export, import, section and resource tables; Ghidra 12.2 headless decompilation of the whole
  of `bin\ode.dll` and of the device creation, start-up, thread start and floating-point control code of the
  game binaries; objdump instruction listings; instruction counts per function; constants read from the
  binaries. No execution or debugging; the game was not run.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- `dWorldCreate` also resets the world auto-disable values (as public 0.5.0: flag off, linear and angular
  thresholds 0.001, stored squared, 10 steps, time 0); section 3.1 lists only the other values it resets.
- Public ODE releases distributed outside the repository's tags between 0.5.0 and 0.8, if any, were not
  compared (section 1 compares the tagged 0.5.0 and 0.8 only).
