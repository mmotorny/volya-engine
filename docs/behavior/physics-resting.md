# Resting and waking of shells

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of when a shell rests and
what wakes it: here, a physics object's shell activated at rest, waking by the scripts' joint motor setter, and
what the rest test after a step sees (which bodies run it, accumulated forces and a motor's torque, the rest
parameters of a physics object's build). It is part of PH-5 of `docs/behavior/physics-scope.md` (section 3);
the rest of PH-5's scope (the rest test's own rule, its thresholds and count, what resting stops beyond
stepping, waking by hits, the first-frame settling of objects placed by the level) is **unestablished** here.
Waking by a contact is PH-4's (`contacts`).

Builds on (not repeated): `docs/behavior/physics-scope.md` (cited as `scope`; terms *body*, *shell*, *joint*,
*step*, *scripts*; the default rest parameters and `[disable]`, 1.1), `docs/behavior/physics-world.md` (cited
as `world`; *object*, *group*, *active* objects, the rest pass, 4.4), `docs/behavior/physics-library.md` (cited as
`library`; *public 0.5.0*, the disabled flag, 3.3, the steppers, 4.1), `docs/behavior/physics-shells.md` (cited
as `shells`; elements, fixing, the physics types), `docs/behavior/physics-contacts.md` (cited as `contacts`;
the collision passes and waking by a contact), PH-7, joints (`physics-joints.md`, cited as `joints`),
`docs/formats/sav.md` (record layouts, the skeleton flags, 7.4.1), `docs/behavior/trader-door.md` (cited as
`door`; the worked example).

Conventions:

- An **element** is `shells`'s. A body is **switched off** when the library's disabled flag is set on it
  (`library` 3.3); a shell **rests** when it is taken out of the world's active objects because all its bodies
  are switched off (`world` 4.4).
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements
  are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing a statement, run by `shapeless_root.py` in the dirty workspace's
  `checks/physics-resting/` (`python3 shapeless_root.py`).

## 1. A shell that starts at rest

A type-3 physics object's shell is activated **switched off** when bit 0x01 of the record's skeleton flags
(`sav.md` 7.4.1) is clear [SR-B8, SR-B8b]: every body is switched off after it is made and the shell is not put
among the world's active objects (it is not stepped) [SR-B8c]. Of the 475 `physic_object` records of type 3,
349 have the bit clear and 126 set [SR-D2c]. Either way the shell is entered in the physics objects' spatial
index, where a resting shell is still collided (`contacts`).

## 2. Waking by the joint motor setter

Setting a joint's maximum force and velocity (the scripts' `set_max_force_and_velocity`) switches every body of
its activated shell on and makes the shell active [SR-B9]. (A joint built from the bone data has no motor until
then, `joints` 1.)

## 3. What the rest test sees

After each step every element whose body is switched on runs the rest test; when no body of the shell is left
switched on, the shell leaves the active objects (`world` 4.4) [SR-B10]. The test runs on a fixed element's body
(`shells` 5.1) as on any other, and a body with a force or torque in its accumulator at the test does not rest
[SR-B10b].

Nothing in a physics object's build changes its rest parameters other than the model's `[disable]` section
(`scope` 1.1) [SR-B10c]. A motor driving into its stop adds its maximum torque to body 1 while the step's rows
are built, and both steppers clear the accumulators at the end of the step (public 0.5.0) [SR-P2], so a motor's
torque is not seen by the rest test.

## 4. Worked example: the trader's door

- The door's record has skeleton flags 0 [SR-D2], so its shell rests from the moment it exists (section 1).
- The first `set_max_force_and_velocity` of its scheme (`door` 2.2, 2.3) wakes it (section 2); until then the
  door does not move.
- Its model has no user data, so no `[disable]` [SR-D2b]: its rest parameters are the defaults of `scope` 1.1
  (section 3). Its fixed frame body (`shells` 3.5) runs the rest test as the door's body does, and the closing
  motor's torque is not seen by the test (section 3).

So nothing keeps the held door's shell awake by construction. Whether its bodies meet the rest test while the
motor presses the door on its stop is **unestablished** statically (**believed**: yes, both bodies being still).

## 5. Unestablished and believed

- The rest of PH-5's scope (introduction), the rest test's own rule among it.
- Whether the trader's door's shell comes to rest while held closed (section 4; believed yes).
- Whether any other per-frame work of the trader's door object wakes its shell while locked.

## 6. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (merged view); binary `bin\xrGame.dll`.
- Data examined: `spawns\all.spawn` (every `physic_object` record's physics-object block, `sav.md` 7.4.2, and
  skeleton flags, 7.4.1), the trader's door's model (bone data, user data); public ODE 0.5.0's sources for the
  steppers' clearing of accumulators and the limit-motor rows.
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and instruction listings with Ghidra) of
  shell activation switched off, the joint motor setter, the rest test and resting, the physics object's
  build of its rest parameters; constants read from the binary. No execution or debugging; the game was not run.
  The checks in `checks/physics-resting/` reproduce every fact (moved here with their facts from an earlier
  specification of the trader's door's shell).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Whether a shell activated switched off (section 1) is put on the world's list of resting shells (`world` 4.4), and so handled by its rest pass before it has first been stepped, is not stated.
