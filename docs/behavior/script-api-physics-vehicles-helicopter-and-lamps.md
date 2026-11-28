# Script API Y: physics, vehicles, helicopter and lamps

What the physics, vehicle, helicopter and lamp part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow
of Chernobyl (Russian edition), as scripts see it: the classes `physics_shell`, `physics_element` and
`physics_joint`; the classes `holder`, `CCar` and `CHelicopter`; the class `hanging_lamp`; and the eight members of
`game_object` that lead to them or act on an object's physics (`get_physics_shell`, `get_car`, `get_helicopter`,
`get_hanging_lamp`, `get_holder_class`, `get_current_holder`, `explode`, `set_const_force`). This is spec Y of the
approved scoping specification `docs/behavior/script-api-scope.md` (section 6, order 20); its scope rule and its
review gaps apply: this spec owns every name of these classes as the game library registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type, the 250
  loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched,
  calls of missing members); spec B, `docs/behavior/script-api-engine-services.md` (the render device and its
  times); spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md` (class ids, client classes); spec
  E, `docs/behavior/script-api-value-types-and-math.md` (`vector`, `matrix`; a vector or matrix result is a new
  object); spec F, `docs/behavior/script-api-configuration-and-files.md`; spec M,
  `docs/behavior/script-api-game-object-basics.md` (the handle, the **kinds** of object of its 2.2, what a member
  does on an object of the wrong kind, 2.3, and the diagnostics routine that returns at once).
- The physics specifications: `docs/behavior/physics-scope.md` (cited as `scope`; terms *body*, *shell*,
  *joint*, *step*), `docs/behavior/physics-world.md` (cited as `world`; the step length *h*, the world spring and
  damping, the per-step actions of its 4.3), `docs/behavior/physics-shells.md` (cited as `shells`; *element*, its
  frame and mass centre, *activating*, *switched off*, fixing an element, 5.1; the trader's door, 10),
  `docs/behavior/physics-contacts.md`, `docs/formats/bone-physics.md` (cited as `bone`;
  joint axes and limits, 3.2; joint stiffness from spring and damping, 3.3), `docs/behavior/physics-library.md`
  (cited as `library`; the shipped library, *public 0.5.0*), PH-7, joints (`docs/behavior/physics-joints.md`, cited
  as `joints`; a joint's motor state when built, a hinge whose first element is fixed, the trader's door's hinge, 3),
  PH-5, resting and waking (`docs/behavior/physics-resting.md`, cited as `resting`).
- `docs/behavior/hanging-lamps.md` (cited as `lamps`).

Joints, PH-7 of `scope` (not yet specified), own how joints are built from the bone data and what their stored
values do in the simulation. This spec states what a script call stores, what it hands to the library when the
joint exists in the world, and what it returns; everything further about a joint's mechanics is **PH-7's**.
**Disputed in review:** PH-7 is not unspecified: `docs/behavior/physics-joints.md` (approved) is part of PH-7 and
states a joint's motor state when it is built and a hinge whose first element is fixed, including what the
scripts' hinge angle and limits read for it.

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [Y-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from
  the game library's registration code.
- A call is written as scripts write it: `shell:get_joint_by_bone_name("door")`. A **number** is the runtime's
  double; values pass to the engine as single-precision floats or integers. **nil** is Lua's nil; a member said to
  return "none" returns nil. Angles are radians, times milliseconds where stated, and the library's units are
  those of `library` (believed: metres, kilograms, seconds).
- A shell, element or joint is **active** when it is in the physics world (`shells` 2.1: activating makes its
  bodies and joints). A body is **switched off** when the library's disabled flag is set on it (`shells`
  conventions, `library` 3.3), **switched on** otherwise.
- Class names are the names the game library registers for scripts (`CCar`, `CHelicopter`); engine classes scripts
  cannot name are described ("a stationary machine gun").
- Each fact is **static** (from static analysis of the game library, not observed at run time), **measured**
  (from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was
  run.

**Checks.** Every claim carries a tag [Y-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-physics/run_all.py` (`python3 -I run_all.py`): it reads the game library's bytes and run-time
type information, the evidence of read-only headless static analysis (produced by `gh.py` beside it), the
listing, the scoping spec's scan and the shipped scripts and data. All checks pass (`ALL PASS 15`). Tags
[TD-*n*] are run by `checks/trader-door-retire/run_all.py` in the same workspace (`python3 run_all.py`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 139 listed items [Y-2]:

| Owner | Items |
|---|---|
| `physics_shell` | the class; 15 functions (section 3) |
| `physics_element` | the class; 11 functions (section 4) |
| `physics_joint` | the class; 20 functions (section 5) |
| `holder` | the class; `engaged`, `Action`, `SetParam` (section 6) |
| `CCar` | the class; 10 functions and 6 constants (section 7) |
| `CHelicopter` | the class; 32 functions, 12 properties and 13 constants (section 8) |
| `hanging_lamp` | the class; `turn_on`, `turn_off` (section 9) |
| `game_object` | `get_physics_shell`, `get_car`, `get_helicopter`, `get_hanging_lamp`, `get_holder_class`, `get_current_holder`, `explode`, `set_const_force` (section 2) |

The scoping spec also assigns this spec three candidate strings, `SetLinearAcc`, `other_answer_item` and
`ph_skeleton_object`, and the two names `air_attack`, `air_attack_active`, which the scripts use and the game
library lacks (section 10) [Y-2].

### 1.2 The classes as registered (static)

| Class | Bases | Constructor | Registered members |
|---|---|---|---|
| `physics_shell` | none | none | the 15 listed functions [Y-3] |
| `physics_element` | none | none | the 11 listed functions [Y-3] |
| `physics_joint` | none | none | the 20 listed functions [Y-3] |
| `holder` | none | none | `engaged`, `Action`, `SetParam` [Y-3] |
| `CCar` | `CGameObject`, `holder` | one, no arguments | the 10 listed functions, `Action` and `SetParam` again (the car's own, section 6.2), and the 6 constants [Y-3] |
| `CHelicopter` | `CGameObject` | one, no arguments | the 32 listed functions (two of them with two forms: `SetEnemy`, `UseFireTrail`), the unlisted `SetLinearAcc`, 12 properties, 13 constants [Y-3] |
| `hanging_lamp` | `CGameObject` | one, no arguments | `turn_on`, `turn_off` [Y-3] |

Against the listing: the listing gives `CHelicopter` neither a base nor a constructor, but the game library
registers both; and it lacks `SetLinearAcc` [Y-3]. Every other registered name and every constant value matches
the listing [Y-3]. The members of `CGameObject` (spec M 10.1) are reached on `CCar`, `CHelicopter` and
`hanging_lamp` objects through the base (spec A 1.3).

Scripts cannot create shells, elements, joints or holders; they receive them from the members of section 2 and of
the shell and joint (no constructor) [Y-3]. Calling `CCar()`, `CHelicopter()` or `hanging_lamp()` is a client class
constructor as spec M 10.3 describes for its client classes (**believed** the same: the registrations have the same
form).

### 1.3 Values the engine returns (believed)

A shell, element, joint or holder returned to a script refers to the engine's own object, which the engine keeps
and destroys (the shell with its object; **believed**: the values are passed as references, not handed to the
runtime). A script holding one after the engine destroyed it holds a value whose object no longer exists; what
calling a member on it does is **unestablished**. A member that would return no object returns nil.

## 2. Members of `game_object` (static)

| Member | Result |
|---|---|
| `get_physics_shell()` | the object's shell when the object has the kind physical (spec M 2.2) and has a shell; nil otherwise, without a diagnostic [Y-5] |
| `get_car()` | the object as a `CCar` when its class is the car's (a run-time class test); otherwise nil, after the diagnostic text `CGameObject : cannot access class member get_car!` is handed to the routine that returns at once (spec M 2.3) [Y-5] |
| `get_helicopter()` | likewise for the helicopter; text `CGameObject : cannot access class member get_helicopter!` [Y-5] |
| `get_hanging_lamp()` | likewise for a hanging lamp, as a `hanging_lamp`; text `CGameObject : it is not a lamp!` [Y-5] (`lamps` 6.1) |
| `get_holder_class()` | the object as a `holder` when it has the kind **holder**; otherwise nil, text `CGameObject : it is not a holder!` [Y-5] |
| `get_current_holder()` | for the player's object (kind player, spec M 2.2): a holder the player's object keeps (**believed** the vehicle it is in), nil when none; nil for any other object [Y-5] |
| `explode(n)` | section 2.1 |
| `set_const_force(direction, power, time)` | section 2.2 |

The kind **holder** is answered by exactly three classes: the car, and two mounted-weapon classes that scripts
cannot name (a stationary machine gun and a mounted weapon; whether the shipped levels place either is
**unestablished** here) [Y-6]. The kind **explosive** (2.1) is answered by the car, the helicopter, the grenade
classes, rockets and explosive items [Y-6]. The car, the helicopter and the hanging lamps have the kind physical
(spec M 2.2) [Y-6].

### 2.1 `explode(n)`

The argument is not read [Y-5]. On an object with the kind explosive and no parent (spec M 3): its explosive part
finds a normal by a ray query from the object (**believed** a surface below it; when the query finds nothing, or
finds an object, the normal is (0, 1, 0)), records the object's own id as the explosion's initiator, and sends the
explosive's **explode event** (event number 23) carrying the initiator's id, the object's position and that
normal, then marks itself as exploding [Y-5, Y-14]. The event is sent only where the game is the server side
(**believed**: the code tests the game's network state before sending; which states pass is **unestablished**).
What the event then does is the explosions' (**unestablished** here). An object with a parent, or without the kind:
nothing; in each case a diagnostic text goes to the routine that returns at once [Y-5].

### 2.2 `set_const_force(direction, power, time)`

`direction` a vector, `power` a number, `time` a number of milliseconds (read as an unsigned 32-bit integer)
[Y-5]. The object's shell is asked through the physical kind without testing for the kind (**believed**: on an
object without it the program crashes). Then [Y-5]:

- no physics world: nothing;
- no shell: nothing;
- otherwise a per-step action (`world` 4.3) is added: the force `direction × power` (each component multiplied,
  `direction` not normalised) acting on the shell for `0.001 · time` seconds, counted in steps as `world` 4.3
  states. At each step the action applies the force through the shell's own force application, that of
  `shell:apply_force` (section 3.1); the action reports itself done when the shell is gone or not active
  (**believed**: that test is its "done" report) [Y-5].

The diagnostic texts go to the routine that returns at once [Y-5] (`world` 4.3).

## 3. `physics_shell` (static)

### 3.1 Force and velocity

- `apply_force(x, y, z)`: with **F** = (x, y, z) and *f* = |**F**|: if *f* < 10⁻⁷, nothing [Y-7]. Otherwise, only
  while the shell is active, each element of the shell receives a force along **F** / *f* of size *f* · *m*ₑ / *M*
  (*m*ₑ the element's mass, *M* the shell's mass), so the force is shared in proportion to mass; then the shell is
  woken (**believed**: the call that follows is the shell's waking). An inactive shell: nothing [Y-7]. How an
  element applies a force given as direction and size is **believed** the same as `physics_element:apply_force`
  (section 4.1).
- `get_linear_vel(v)`, `get_angular_vel(v)`: those of the shell's first element (section 4.2), written into `v`
  [Y-7]. A shell without elements: **believed** an invalid read (the first element is taken unchecked).

### 3.2 Elements and joints

The shell keeps its elements and its joints in two lists, in the order of its build (`shells` 3) [Y-7].

| Member | Result |
|---|---|
| `get_elements_number()` | the number of elements [Y-7] |
| `get_element_by_order(n)` | element `n` (0 is the first); `n` at or beyond the number of elements is a failed check (**believed** fatal, as failed checks are, `console-log.md` 3.8) [Y-7] |
| `get_element_by_bone_id(id)` | see below |
| `get_element_by_bone_name(name)` | the bone id of `name` in the shell's model, then as `get_element_by_bone_id` [Y-7]; a name the model lacks: **unestablished** (the model's "no bone" id is passed on) |
| `get_joints_number()` | the number of joints [Y-7] |
| `get_joint_by_order(n)` | joint `n`; not range-tested (`n` beyond the list: **believed** an invalid read) [Y-7] |
| `get_joint_by_bone_id(id)` | the first joint whose bone (5.1) is `id`; nil if none [Y-7] |
| `get_joint_by_bone_name(name)` | the bone id of `name` in the shell's model, then as `get_joint_by_bone_id` [Y-7] |

`get_element_by_bone_id(id)` [Y-7]: while the shell has a model and is active, the element that drives bone `id`
of the model (the element whose bone callback, `shells` 8.1, is set on that bone), or nil when no element drives
it; otherwise (inactive, or no model) the first element in the list whose bone is `id`, or nil.

### 3.3 Breaking

A shell that can break (PH-13) has breaking data, whose check the world runs every step [Y-7]:

- `block_breaking()`: takes the shell's breaking check out of the world's per-step checks and marks breaking
  blocked; nothing for a shell without breaking data [Y-7].
- `unblock_breaking()`: clears the mark and puts the check back; when the shell is active it also runs one check at
  once (**believed**: one step's breaking check) [Y-7].
- `is_breaking_blocked()`: `true` when the shell has breaking data and breaking is blocked [Y-7].
- `is_breakable()`: `true` when the shell has breaking data and breaking is **not** blocked [Y-7].

What the breaking check does is PH-13's.

## 4. `physics_element` (static)

### 4.1 Force

`apply_force(x, y, z)`: only while the element is active and not fixed (4.3): a switched-off body is switched on, the
element's shell is told of it (**believed**: its resting is reset), the force (x, y, z) is added to the body (the
library's body force in world axes, which acts during the next step, public 0.5.0), and then the body's
accumulated force and torque are limited (how: **unestablished**, the hit and force rules of PH-9) [Y-8].
Otherwise nothing [Y-8].

### 4.2 Motion and mass

| Member | Result |
|---|---|
| `get_linear_vel(v)`, `get_angular_vel(v)` | writes into the vector `v` the body's linear (angular) velocity when the element is active and its body switched on; otherwise (0, 0, 0) [Y-8]. The result is in `v`; the call returns nothing (**believed**: `v` is passed as a reference to the script's vector) |
| `get_mass()` | the element's mass, as its mass was set (`shells` 2.3) [Y-8] |
| `get_volume()` | the sum of the volumes of the element's geometries (`bone` 2.3), which the element also keeps as its volume [Y-8] |
| `get_density()` | the mass divided by the volume the element keeps (as last computed, **believed** first by the mass setup of `shells` 2.3) [Y-8] |
| `global_transform()` | a new matrix (spec E): the element's frame in the world, `i`, `j`, `k` the body's rotation axes and `c` the body's origin moved back by the rotated mass centre (`shells` 2.1) [Y-8]. The body is read without a test: on an inactive element **believed** an invalid read |

The listing writes `global_transform(physics_element*)`: the registered function takes the element itself as its
argument, so scripts call it as `element:global_transform()` [Y-4].

### 4.3 Fixing

- `fix()`: marks the element fixed and fixes its body as `shells` 5.1 states (a large sphere mass, gravity off, zero
  velocities, force and torque) [Y-8]. A fixed element ignores `apply_force` (4.1).
- `release_fixed()`: when the element is fixed: clears the mark and, if the element is active, gives the body
  back the element's own mass [Y-8]. Gravity, switched off by fixing, is **not** switched back on by this call
  (static: only the mass is restored) [Y-8].
- `is_fixed()`: `true` while the element is marked fixed [Y-8].
- `is_breakable()`: `true` when the element has breaking data of its own (PH-13) [Y-8].

## 5. `physics_joint` (static; mechanics PH-7's)

### 5.1 The joint

A joint connects its **first element** and its **second element** (a joint to the static world has no second
element, **believed**) and belongs to a bone of the model [Y-9]:

- `get_bone_id()`: the joint's bone id (16 bits); `get_first_element()`, `get_stcond_element()`: the first and
  second elements, nil when there is none [Y-9];
- `is_breakable()`: `true` when the joint has breaking data (PH-13) [Y-9];
- `get_axes_number()`: the number of axis records the joint holds [Y-9]: those it was made with, by its kind
  (below): none for a ball, 1 for a hinge, 2 for a two-axis hinge and for a slider, 5 for a ball with angular motor;
  nothing else adds records [TD-J2].

Every joint is of one **kind**, which decides the library joint it is made as, read from the library calls each
kind makes [Y-9]: **ball** (no axis), **hinge** (one axis), **two-axis hinge**, **ball with angular motor** (three
axes) and **slider** (an axis of translation with an angular motor about it). Which bone-data joint types become
which kind is PH-7's.

**Axis index.** The setters of limits, motors, axis directions and axis stiffness, `get_limits` and `get_axis_dir`
first adjust their axis argument *n* to the kind [Y-9]: *n* below −1 becomes −1; for a ball, −1; for a hinge, 0;
for a two-axis hinge or a slider, at most 1; for a ball with angular motor, at most 2. −1 means "every axis" for
`set_max_force_and_velocity` and `set_axis_spring_dumping_factors`; `set_limits` and the axis-direction setters do
nothing with it. `get_axis_angle` and
`get_max_force_and_velocity` take *n* as given [Y-9].

A joint's values are **stored** in the joint; when the joint is active (made in the library) most setters also
hand them to the library at once, as stated for each. How the stored values are used when the joint is made is
PH-7's.

**Before any script setter.** Each axis record starts with the limits −∞ … +∞ and maximum force and velocity 0
[TD-J2]. A joint built from the bone data (`shells` 3.3) is given, through the same function as `set_limits`, the
bone's limits for each axis that gets limits (`bone` 3.2: high − low below 2π) [TD-J3], and maximum force and
velocity 0 on every axis (`joints` 1). So before any script setter `get_limits` (5.2) returns the bone's limits of
the axis, or (−∞, +∞) for an axis without limits, each negated and swapped when the joint's first element is fixed
or has no body; the stored force and velocity are 0 and 0, and the joint has no motor (`joints` 1).

### 5.2 Limits

- `set_limits(low, high, n)`: only when the joint has both elements; *n* adjusted; *n* = −1: nothing [Y-9].
  Stores `low` and `high` as axis *n*'s limits, and also stores for that axis an angle computed from the
  orientations of the two elements at the call (**believed** a reference angle; its use is PH-7's) [Y-9]. An
  active joint gets them at once: for a hinge, the library's parameters 0 and 1 (**believed** the low and high
  stops, public 0.5.0) are set to `low` and `high`; the other kinds set their axes' stops likewise [Y-9].
- `get_limits(low, high, n)`: *n* adjusted. Returns **two numbers**, the call's two results [Y-10]: when the
  joint's first element is not fixed and has a body, axis *n*'s stored (`low`, `high`); otherwise (−`high`,
  −`low`) [Y-9]. The first two arguments must be numbers (they are read) but their values are not used [Y-10].
  Scripts write `lo, hi = joint:get_limits(lo, hi, 0)`. With *n* adjusted to −1 (a ball) the record before the
  first is read: **unestablished** (believed an invalid read). For a hinge whose first element is fixed, the
  negated and swapped limits match the library's reversed angle, so they bound the hinge angle scripts read
  (`joints` 2.3).

### 5.3 Motor

- `set_max_force_and_velocity(force, velocity, n)`: first, when the joint belongs to a shell that is active, the
  shell is woken (**believed** the shell's waking) [Y-9]. **Disputed in review:** not only believed:
  `docs/behavior/physics-resting.md` 2 establishes that this setter switches every body of its activated shell on
  and makes the shell active. Then stores `force` and `velocity` for axis *n* (*n*
  adjusted; *n* = −1: for every axis of the kind; a ball: nothing) [Y-9]. An active hinge gets them at once as its
  motor: the library's parameter 3 (maximum motor force) is `force` and parameter 2 (motor target velocity) is
  `velocity` (public 0.5.0's parameter numbering, `library` 5.1: 0 low stop, 1 high stop, 2 motor velocity, 3
  motor maximum force [TD-B1d]); the other kinds set the motors of their axes likewise [Y-9]. For a hinge whose
  first element is fixed, a motor velocity *v* drives the scripts' hinge angle at −*v* (`joints` 2.3).
- `get_max_force_and_velocity(force, velocity, n)`: returns the stored force and velocity of axis *n* (*n* not
  adjusted) through its first two arguments, which the binding takes as references to engine values, not as
  numbers; it returns no result [Y-10]. What a script call with numbers does is **unestablished** (**believed**:
  it fails to match the arguments, spec A). No shipped script calls it [Y-15].

### 5.4 Angle, axis and anchor

- `get_axis_angle(n)` [Y-9]:

| Kind | Result |
|---|---|
| hinge | the library's hinge angle, whatever *n* |
| two-axis hinge | the library's first hinge angle, whatever *n* |
| ball with angular motor | the angular motor's angle of axis *n* (*n* not adjusted; outside 0 … 2 the library's behaviour, **unestablished**) |
| slider | *n* = 0: the slider's position (a distance); *n* = 1: the angular motor's angle of axis 0; otherwise +∞ |
| ball | +∞ |

  For a hinge, angle 0 is the pose of the bodies when the hinge was made (public 0.5.0 records it when the axis is
  set [TD-B2b], which the hinge's creation does, `joints` 2.1); for a hinge whose first element is
  fixed, the angle is the second element's rotation about the hinge axis, φ of `joints` 2.3. The joint is
  read through the library; on a joint not made in the world, **unestablished**.
- `get_axis_dir(n, v)`: writes into `v` the library's current axis in world axes [Y-9]: a hinge's axis; for a
  two-axis hinge its first axis when *n* = 0, its second otherwise; for a ball with angular motor the motor's axis
  *n*; a slider's axis; for a ball `v` is left unchanged.
- `get_anchor(v)`: writes into `v` the library's anchor point [Y-9]: a ball's (also for a ball with angular motor);
  for a hinge and a two-axis hinge, the library's hinge anchor (for the two-axis hinge, what the hinge getter
  returns from it is the library's, **unestablished**); for a slider, a failed check with the text `position of
  slider joint is undefinite` (**believed** fatal) and `v` receives undefined values.

### 5.5 Setting up anchor and axes

- `set_anchor_global(x, y, z)`, `set_anchor_vs_first_element(x, y, z)`, `set_anchor_vs_second_element(x, y, z)`:
  store the anchor point and the frame it is given in (the world, the first element's, the second element's)
  [Y-9]. Nothing is handed to the library: an active joint is not changed (**believed**: the values are used when
  the joint is made, PH-7).
- `set_axis_dir_global(x, y, z, n)`, `set_axis_dir_vs_first_element(x, y, z, n)`,
  `set_axis_dir_vs_second_element(x, y, z, n)`: likewise for the direction of axis *n* (*n* adjusted; −1: nothing)
  [Y-9].

### 5.6 Stiffness

- `set_joint_spring_dumping_factors(spring, damping)`: stores the joint's error reduction and constraint-force
  mixing computed from `spring` and `damping` by the formula of `bone` 3.3 with the world spring and damping
  (`world` 1.2) and the step length *h*; for a two-axis hinge with the constants 20,000 and 1,000 of `bone` 3.3
  [Y-9]. An active joint gets at once: for a hinge, a ball with angular motor (each axis) and a slider, the
  library's parameter 6 (**believed** constraint-force mixing) set to the mixing; for a two-axis hinge, parameters
  9 and 10 (**believed** the suspension's error reduction and mixing) [Y-9]. The error reduction is not handed to
  the library for the other kinds by this call [Y-9].
- `set_axis_spring_dumping_factors(spring, damping, n)`: stores axis *n*'s stop stiffness from `spring` and
  `damping` by the same formula (for a two-axis hinge, error reduction 1 and mixing 0, `bone` 3.3); *n* = −1: every
  axis of the kind [Y-9]. An active joint gets at once its library parameters 7 and 8 (**believed** the stops'
  error reduction and mixing) for the axes concerned [Y-9].

## 6. `holder` (static)

### 6.1 Members

- `engaged()`: `true` while the holder is occupied (**believed**: the holder's occupant is set when someone, the
  player, is in it) [Y-4].
- `Action(id, value)`, `SetParam(id, v)` (`v` a vector): what they do depends on the holder's class [Y-11]:

| Holder | `Action(id, value)` | `SetParam(id, v)` |
|---|---|---|
| the car | 6.2 | 6.2 |
| a stationary machine gun | `id` 32: `value` 1 starts firing, any other value stops it; other ids nothing | nothing |
| a mounted weapon | nothing | nothing |

### 6.2 The car's weapon

The car may have a **weapon** part (`HasWeapon`, section 7). Without one, `Action` and `SetParam` do nothing
[Y-11]. With one, using the constants of `CCar` [Y-11]:

| Call | Effect |
|---|---|
| `Action(CCar.eWpnFire, value)` (3) | `value` 1: firing on; any other value: firing off, and the weapon's second operation runs, the one `eWpnActivate` runs for a value other than 1 (**believed** deactivating) |
| `Action(CCar.eWpnActivate, value)` (4) | `value` 1: the weapon's first operation (**believed** activating); otherwise its second (**believed** deactivating) |
| `Action(CCar.eWpnAutoFire, value)` (5) | automatic fire on when `value` is 1, off otherwise |
| `Action(CCar.eWpnToDefaultDir, value)` (6) | the desired aim is set back to a direction the weapon keeps (**believed** its default) |
| `Action` with 1 or 2 | nothing |
| `SetParam(CCar.eWpnDesiredPos, point)` (2) | the desired aim becomes the direction from the weapon's firing point to `point`, normalised when its length is not negligible |
| `SetParam` with any other id (`eWpnDesiredDir` 1 included) | nothing |

How the weapon turns and fires is the vehicles' (PH-21 and the weapons', **unestablished** here).

## 7. `CCar` (static)

Constants: `eWpnDesiredDir` 1, `eWpnDesiredPos` 2, `eWpnFire` 3, `eWpnActivate` 4, `eWpnAutoFire` 5,
`eWpnToDefaultDir` 6 [Y-3]. A `CCar` value has the members of `holder` (section 6) and of `CGameObject`.

| Member | Result |
|---|---|
| `HasWeapon()` | `true` when the car has a weapon part [Y-11] |
| `CanHit()` | the weapon's "can hit" flag (**believed**: the aim is within the weapon's tolerance); `false` without a weapon [Y-11] |
| `FireDirDiff()` | the angle, in degrees, between two directions the weapon keeps (**believed** its current and desired aim); 0 without a weapon [Y-11] |
| `IsObjectVisible(obj)` | see below |
| `CurrentVel()` | a new vector: the linear velocity of the car's shell, as `shell:get_linear_vel` (3.1) [Y-11]; without a shell **believed** an invalid read |
| `GetfHealth()` | number: the health the car's conditions hold [Y-4, Y-11] |
| `SetfHealth(h)` | sets that health to `h`, not clamped [Y-11] |
| `SetExplodeTime(t)` | see below |
| `ExplodeTime()` | see below |
| `CarExplode()` | see below |

The health of `GetfHealth` is an entity's (spec M 2.2: the car and the helicopter have the kind entity, not living),
so spec M's `health` property reads −1 on them while `GetfHealth` reads this value (static, M 5.1) [Y-11].

`IsObjectVisible(obj)` [Y-11]: when the car has a further part the code tests first (which part is
**unestablished**), the result comes from a test through that part (**unestablished**). Otherwise: `obj` nil gives
`false` after a log message saying the member was called with a null argument (**believed** written to the
log); else a ray is tested from the car's centre (with its height raised to the
weapon's firing height when the car has a weapon, **believed**) to `obj`'s centre, over the ray's length, against
the level (**believed** its static geometry); `true` when nothing blocks it.

**Explosion timer** [Y-11]: the car keeps a timer of its explosion, with a "set" and a "running" mark.

- `SetExplodeTime(t)`: when the timer is not running: its time becomes `0.001 · t` seconds (0 when below 10⁻⁷ s)
  and it is marked set; a running timer is not changed. What starts it running is **unestablished**.
- `ExplodeTime()`: 0 when not set; otherwise its time (minus the render device's current time in seconds while
  running, **believed** spec B's global time) rounded to a whole number and multiplied by 1000: a whole number of
  seconds, in milliseconds.

`CarExplode()` [Y-11]: once only (a second call does nothing): marks the car exploded, switches its weapon's firing
off, acts on a list of its parts (**unestablished** which), sends the explode event of 2.1 at the car's position
with the normal (0, 1, 0), acts on its shell (**unestablished** how), and, when two marks of the car allow it
(**unestablished** which), creates an object of the section `physic_destroyable_object` for it (**believed**). What
the explosion does is the explosions' and PH-21's.

## 8. `CHelicopter` (static)

### 8.1 Constants and states

| State (member) | Constants [Y-3] |
|---|---|
| life (`GetState()`) | `eAlive` 0, `eDead` 1 |
| movement (`GetMovementState()`) | `eMovNone` 0, `eMovToPoint` 1, `eMovPatrolPath` 2, `eMovRoundPath` 3, `eMovLanding` 4, `eMovTakeOff` 5 |
| enemy (`GetHuntState()`) | `eEnemyNone` 0, `eEnemyPoint` 1, `eEnemyEntity` 2 |
| body (`GetBodyState()`) | `eBodyByPath` 0, `eBodyToPoint` 1 |

Each getter returns the helicopter's current value of that state as a number [Y-12]. The members of 8.2 set the
movement state only to `eMovToPoint`, `eMovPatrolPath` and `eMovRoundPath` [Y-12]; what else sets it is the
helicopter's (**unestablished** here).

### 8.2 Movement

The helicopter's movement keeps a destination, a speed and a path; its initial values are read from the object's
section [Y-12].

| Member | Effect |
|---|---|
| `SetDestPosition(p)` | the destination becomes the vector `p` and the movement state `eMovToPoint`; a path the movement had built for itself (a round path) is dropped [Y-12] |
| `GoPatrolByPatrolPath(name, from)` | follows the level's patrol path `name` (spec N) from its point `from`: the destination becomes that point's position, the movement state `eMovPatrolPath` [Y-12]. An unknown name: **believed** an invalid read |
| `GoPatrolByRoundPath(centre, radius, clockwise)` | builds a ring of points around `centre` (vector) with `radius`, named `heli_round_path_pt_%d`, and follows it from a point chosen by distance, movement state `eMovRoundPath` [Y-12]. A `radius` below a minimum computed from the speed and the turning speeds of the section: the log message `! Helicopter: cannot build round path R=%f. Min R=%f` and nothing else [Y-12]. When the movement state already is `eMovRoundPath`, `clockwise` is inverted first [Y-12]; which direction each value gives is **unestablished** |
| `GetDistanceToDestPosition()` | the distance from the helicopter's current position (as its movement keeps it) to the destination [Y-12] |
| `SetMaxVelocity(v)`, `GetMaxVelocity()` | the speed of the movement; initially the section's `velocity` [Y-12] |
| `GetCurrVelocity()` | a speed the movement keeps (**believed** its current speed) [Y-12] |
| `GetCurrVelocityVec()` | a new vector: the unit direction given by the current heading and pitch of the movement (not scaled by the speed) [Y-12] |
| `SetSpeedInDestPoint(v)` | the speed to have at the destination [Y-12] |
| `GetSpeedInDestPoint(n)` | the argument is not read; that speed, or 0 while the movement follows a path in a state the code tests (**unestablished** which) [Y-12] |
| `SetOnPointRangeDist(d)`, `GetOnPointRangeDist()` | the distance at which a point counts as reached (**believed**); initially the section's `on_point_range_dist` [Y-12] |
| `SetLinearAcc(fw, bk)` (unlisted) | the movement's two accelerations along a path; initially the section's `path_linear_acc_fw` and `path_linear_acc_bk` [Y-12] |
| `GetRealAltitude()` | the distance to the level below: a ray straight down from the helicopter's position, up to 1000 m, against the level (**believed** its static geometry) [Y-12]; when nothing is hit, **unestablished** |
| `GetSafeAltitude()` | the highest y of a bounding box of the level (**believed** the level box of `world` 4.5) plus the section's `safe_altitude` [Y-12] |

How the helicopter flies, and what the speeds, accelerations and ranges do, are the helicopter's (**unestablished**
here; PH-21 owns its death fall).

### 8.3 Enemy and looking

- `SetEnemy(obj)` (a handle): enemy state `eEnemyEntity`, the enemy is `obj`'s id (spec M 3) [Y-12].
- `SetEnemy(p)` (a vector): enemy state `eEnemyPoint`, the enemy is the point `p` [Y-12].
- `ClearEnemy()`: enemy state `eEnemyNone` [Y-12].
- `LookAtPoint(p, on)`: stores `p` and `on`; the body state becomes `eBodyToPoint` when `on` is true, `eBodyByPath`
  otherwise [Y-12].
- `isVisible(obj)`: `true` when a ray from the helicopter's position to `obj`'s centre is not blocked by the level
  (**believed** its static geometry) [Y-12].

### 8.4 Weapons and the properties

The 12 properties: 8 readable and writable, 4 read-only [Y-3]. The readable and writable ones hold values the
helicopter reads from its section when it is loaded (static, by key) [Y-12]:

| Property | Kind | Section key |
|---|---|---|
| `m_use_rocket_on_attack` | boolean | `use_rocket` |
| `m_use_mgun_on_attack` | boolean | `use_mgun` |
| `m_min_rocket_dist`, `m_max_rocket_dist` | number | `min_rocket_attack_dist`, `max_rocket_attack_dist` |
| `m_min_mgun_dist`, `m_max_mgun_dist` | number | `min_mgun_attack_dist`, `max_mgun_attack_dist` |
| `m_time_between_rocket_attack` | number (unsigned 32-bit) | `time_between_rocket_attack` |
| `m_syncronize_rocket` | boolean | `syncronize_rocket` |

The read-only ones report what the members of 8.5 did: `m_flame_started` (`StartFlame`), `m_light_started`
(`TurnLighting`), `m_exploded` (`Explode`), `m_dead` (`Die`) [Y-12]. How the helicopter's attack uses the weapon
values is the helicopter's (**unestablished** here). The shipped helicopter section is `[helicopter]` of
`config\creatures\helicopter.ltx` [Y-12] (measured; its values are data).

Further weapon members [Y-12]:

- `SetFireTrailLength(l)`: the fire trail's length; initially the section's `fire_trail_length`.
- `UseFireTrail()`: `true` while the fire trail is used; `UseFireTrail(on)`: sets it, and sets the fire
  dispersion to the section's `fire_dispersion_null` when `on`, `fire_dispersion_base` otherwise, each read as
  degrees and converted to radians (× 0.0174533); initially from `use_fire_trail`.
- `SetBarrelDirTolerance(t)`: the barrel's direction tolerance, stored as given; initially the section's
  `barrel_dir_tolerance` (degrees by the section's comment).

### 8.5 Effects, death and explosion

- `GetfHealth()`, `SetfHealth(h)`: as the car's (section 7) [Y-4, Y-12].
- `TurnLighting(on)`: switches the helicopter's light on or off; `m_light_started` becomes `on` [Y-12].
- `TurnEngineSound(on)`: the engine sound's volume becomes 1 when `on`, 0 otherwise (**believed**: the value is
  set on the engine sound) [Y-12].
- `StartFlame()`: once (a second call does nothing): starts a particle effect named by the helicopter (**believed**
  the section's `smoke_particle`) and sets `m_flame_started` [Y-12].
- `Die()`: once (nothing when the state already is `eDead`): plays the section's `broken_snd` sound at the
  helicopter's position, hides the bones of a list the helicopter keeps (**unestablished** where it comes from),
  hands its shell to the death fall with velocities from its last movement (PH-21's), sets the state `eDead` and
  `m_dead`, and stops the helicopter's per-frame processing [Y-12].
- `Explode()`: sets `m_exploded`, stops the flame's particle effect, may create an object of the section
  `physic_destroyable_object` (as the car's, section 7), sends the explode event of 2.1 at the helicopter's
  position, and stops the sound object `Die` plays its sound through (**believed** the same sound) [Y-12].

## 9. `hanging_lamp` (static)

`turn_on()` and `turn_off()` are the switching on and off of `lamps` 5.1: the same engine functions [Y-13]. A
`hanging_lamp` value comes from `game_object:get_hanging_lamp()` (section 2).

## 10. Names the scripts use that are not registered; candidate strings (static, measured)

- `air_attack(obj)` and `air_attack_active()`: called on a helicopter by `h.script` (loadable) and `test_heli.script`;
  neither name occurs in the game library [Y-15]. What reading an unregistered name on such a value gives is
  **unestablished** (spec M 11 states the same for handles).
- `SetLinearAcc`: registered on `CHelicopter` (8.2) [Y-3].
- `ph_skeleton_object`: the name of a configuration section, `[ph_skeleton_object]` of `config\system.ltx`
  (`lamps` 5.3: split parts); `other_answer_item`: a string of the game library. No registration of this spec's
  classes or members names either, so neither is a name of this spec [Y-15].

## 11. Worked example: the door scheme

The trader's door (`shells` 10) is a physics object whose handle has the kind physical (spec M 12). The door scheme
`scripts\ph_door.script` calls [Y-15]:

1. `self.object:get_physics_shell()`, and stops if it is nil: the door object's shell (section 2).
2. `ph_shell:get_joint_by_bone_name("door")`: the bone id of `door` in the door's model, then the first joint of
   the shell whose bone is that id (3.2): the hinge of the door bone, whose first element is the fixed frame
   element (`shells` 10, `joints` 3).
3. `self.joint:get_limits(self.low_limits, self.hi_limits, 0)` after setting both to 0: axis 0 of the hinge (5.2);
   the two zeros are read and ignored, and the call returns (low, high) or (−high, −low) by the state of the
   hinge's first element (5.2); for the door, whose first element is fixed and whose limits are −1.9199 … 0, it
   returns (−0, 1.9199) (`joints` 3).
4. `self.joint:set_max_force_and_velocity(force, velocity, 0)` with (0, 0), (200, 3), (10000, 1) and (2100, −3)
   in its sections: the hinge's motor on axis 0 (5.3), handed to the library at once while the hinge is active.
5. `self.joint:get_axis_angle(90)`: the hinge angle; the 90 is ignored for a hinge (5.4). The scheme counts the
   door as closed when this angle is at most the low value of step 3, −0: only at the pose the hinge was made in,
   or pressed past it within the stop's softness (`joints` 3).

The gate scheme `ph_gate.script` uses the same members on two joints with axis 1 (`set_limits(min, max, 1)`,
`get_axis_angle(1)`) and blocks and unblocks the shell's breaking (3.3) [Y-15]; for a hinge, axis 1 is adjusted to
axis 0 (5.1).

## 12. Use by the shipped scripts (measured)

By the scoping spec's scan (names attributed to every class declaring them, so counts are upper bounds), this
spec's names are used by 15 loadable scripts (21 of all scripts) [Y-15]. The most used are `get_physics_shell` (6
loadable scripts), `get_joint_by_bone_name` (5), `set_max_force_and_velocity` (4), and `get_axis_angle`,
`get_helicopter`, `GetfHealth`, `set_const_force` (3 each) [Y-15].

## 13. Verification facts

All reproduced by `run_all.py` (15 PASS lines, no failure):

- Scope: 139 listed items; 3 candidates; 59 names used by the loadable scripts, 2 of them absent [Y-2].
- Registration: 7 classes with their bases and constructors; `CHelicopter` with a base and a constructor the
  listing lacks, and the unlisted `SetLinearAcc`; 19 constants as listed [Y-3].
- Implementations: every member paired with the function that runs; 45 of the 46 physics members (all but `global_transform`) and the holder's
  `Action` and `SetParam` reached through virtual calls, one implementation per concrete class [Y-4].
- Kinds: holder 3 classes, explosive 7 classes [Y-6].

## 14. Unestablished

- What calls on a shell, element, joint or holder do after the engine destroyed it (1.3).
- What the explode event does; which network states let it be sent (2.1); what the breaking check does (3.3,
  PH-13); how an element's accumulated force and torque are limited (4.1).
- A shell's element or joint lookups with an unknown bone name or out-of-range order (3.2); `get_limits` on a ball
  (5.2); `get_axis_angle` and `get_anchor` on kinds where the library's own behaviour decides (5.4); a call of
  `get_max_force_and_velocity` with numbers (5.3).
- Everything joints do with their stored values when made and in the simulation, and which bone-data joint types
  give which kind (PH-7).
- Which levels place the two mounted-weapon holders (2); how the car's weapon turns and fires (6.2); the car's
  further part in `IsObjectVisible`, the start of its explosion timer, its explosion's parts and marks (7).
- How the helicopter flies and attacks; `GetRealAltitude` without a hit; the state
  `GetSpeedInDestPoint` tests; the round path's direction; the source of the bones `Die` hides (8).
- What reading `air_attack` and `air_attack_active` gives (10).
- Everything that depends on A (argument matching, calls of missing members).

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `bin\xrGame.dll` as identified above;
  the archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs
  (`scripts\*.script`, `scripts\lua_help.script`, `config\system.ltx`, `config\creatures\helicopter.ltx`).
- Analysis: static analysis with Ghidra (headless, read-only) of the script registration of the seven classes and
  of the eight `game_object` members in the game library, of the member implementations they reach (through the
  virtual-function tables of the concrete shell, element, joint and holder classes, read from the run-time type
  information in the file bytes), of the shell's force sharing, breaking block, element and joint lookups, the
  joint's library calls, the binding's argument and result passing for `get_limits` and
  `get_max_force_and_velocity`, the constant-force action, the explosive part's explode event, the car's weapon
  and explosion timer, and the helicopter's movement, members and loading of its section; constants read from the
  file bytes; the scoping spec's script scan reused; the scripts and configuration read as text. No binary was
  executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.
