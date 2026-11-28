# Joints

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the joints of shells
built from the bone data: here, a joint's motor state when it is built, and a hinge whose first element is fixed
(which bodies the library joins, the axis it is given, and what that means for its angle, stops, motor and the
scripts' reading of its angle and limits). It is part of PH-7 of `docs/behavior/physics-scope.md` (section 3);
the rest of PH-7's scope (the joint kinds' building and simulation in general, limits, springs and damping,
stiffness, joint friction, the door's check that one axis is active, the wheel joint) is **unestablished** here.

Builds on (not repeated): `docs/behavior/physics-scope.md` (cited as `scope`; terms *body*, *shell*, *joint*,
*step*, *scripts*), `docs/formats/bone-physics.md` (cited as `bone`; joint types, axes and limits, 3.2),
`docs/behavior/physics-library.md` (cited as `library`; *public 0.5.0*),
`docs/behavior/physics-shells.md` (cited as `shells`; elements, fixing, the shapeless root and which bodies a
joint joins, 3.3, 3.5, 5.1), PH-5, resting and waking (`physics-resting.md`, cited as `resting`),
`docs/behavior/trader-door.md` (cited as `door`; the worked example).

Conventions:

- An **element** is `shells`'s. Library terms (body 1, body 2, stops, motor, parameters, maximum force,
  velocity) are public 0.5.0's.
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements
  are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing a statement, run by `shapeless_root.py` in the dirty workspace's
  `checks/physics-joints/` (`python3 shapeless_root.py`).

## 1. No motor at creation

Every joint built from the bone data has its maximum force and velocity set to 0 on all its axes when it is
built [SR-B6b], and a hinge gets motor parameters only for a maximum force above 0 [SR-B6c]. So a joint has no
motor until something sets one (for the scripts' setter, `set_max_force_and_velocity`, and its waking of the
shell: `resting`).

## 2. A hinge whose first element is fixed

### 2.1 What the library is given

A joint connects the bone's element (second) to the element above (first) (`shells` 3.3). For a hinge whose
first element is fixed, or has no body, the game passes **no first body** and the **negated axis** (each of its
three components negated) to the library [SR-B7]; the stops passed are the bone's limits unchanged, whatever the
first element [SR-B7d]. The joints are made after every element's body, so a first element fixed at activation
(a shapeless root, `shells` 3.5) is already fixed when the hinge is made [SR-B7b].

### 2.2 What the library does with it

In the library (public 0.5.0, unchanged here, `library` 5.1) attaching with no first body makes the second
body **body 1** and marks the joint reversed [SR-P1]. The stops and the motor act on the angle of body 1 about
the axis as stored in body 1's frame; the angle getter returns that angle negated [SR-P1].

### 2.3 What that gives (derived from SR-P1, SR-B7, SR-B7c, SR-B7d)

With φ the second element's rotation about the bone data's hinge axis (`bone` 3.2: the bone's y axis),
measured from its pose when the hinge was made (φ = 0), right-handed about that axis as built, and the bone's
limits *lo* … *hi*:

- the stops act on −φ: the body turns only within φ = −*hi* … −*lo* (as it would if the fixed body were body 1:
  the negated axis cancels the swap);
- a motor velocity *v* drives φ at −*v*;
- the scripts' hinge angle (`get_axis_angle`, `door` 2.3) is φ; the scripts' limits for such a hinge come back
  negated and swapped, (−*hi*, −*lo*), because its first element is fixed [SR-B7c], matching the reversed getter.

## 3. Worked example: the trader's door

The door's model `door_trader.ogf` [SR-D2b]: root `link` (no shape, rigid), `door` (box, joint type 2, a hinge),
`lock` (no physics). The door's joint is a hinge made with the root's element (fixed, `shells` 3.5) as its first
element and the door's as its second [SR-B6]; so it joins the door's body to the static environment
(`shells` 3.3). Its limits are −1.9199 … 0 [SR-B7d]. By 2.3:

- the door turns only within φ = 0 … 1.9199;
- `door`'s closing motor (velocity +1) drives the door toward φ = 0, its opening motor (velocity −3) toward
  1.9199;
- the scripts' hinge angle runs from 0 to 1.9199, and the scripts' limits come back as (−0, 1.9199), as `door`
  2.3 states;
- the hinge has no motor until the scheme sets one (section 1; `door` 2.3, 2.4, settling their belief).

So `door`'s "closed" test (angle ≤ the low value received, −0) holds only when φ ≤ 0: at the pose the hinge was
made in, or pressed past it by the motor within the stop's softness; not "at any angle between the stops".
That φ = 0 is the placed pose is `door` 2.3's belief.

## 4. Corrections to approved specifications

- `door` 2.3: the script's hinge angle runs 0 … 1.9199, so "closed" holds only at the pose the hinge was made in
  (section 3); there is no motor before the scheme sets one (section 1).

## 5. Unestablished and believed

- The rest of PH-7's scope (introduction).
- **Believed**: that φ = 0 is the door's placed pose (section 3, `door` 2.3).

## 6. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (merged view); binary `bin\xrGame.dll`.
- Data examined: the trader's door's model (bone data, `bone`); public ODE 0.5.0's sources for the attach rule,
  the hinge angle, stops and motor rows.
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and instruction listings with Ghidra) of
  the joints' defaults, the hinge's creation and the scripts' limit reading; constants read from the binary. No
  execution or debugging; the game was not run. The checks in `checks/physics-joints/` reproduce every fact
  (moved here with their facts from an earlier specification of the trader's door's shell).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Which bodies a hinge with a fixed first element joins (no first body passed, the joints made after the bodies) is stated both here (2.1) and in `shells` 3.3; it needs one home, the other citing it.
