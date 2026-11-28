# Where the facts of physics-shell-shapeless-root.md went

Every check id *X* of the former spec is `SR-`*X* in its new home, run by `shapeless_root.py` in that spec's
checks folder (the check code itself is unchanged, in `checks/physics-shapeless-root-lib/`).

| Former section | New home |
|---|---|
| Introduction, conventions (element, switched off, rests, collision index) | `physics-shells.md` conventions; `physics-resting.md` conventions; `physics-contacts.md` 1.5 |
| 1.1 No fatal error (B1, D1) | `physics-shells.md` conventions, 1 |
| 1.2 The root's element (B2, D2b) | `physics-shells.md` 3.2 item 2, 3.5; the door's elements: 10 item 2 |
| 1.3 An element without shapes is fixed (B3, B5) | `physics-shells.md` 2.1, 3.5, 5.1 |
| 1.4 The record's fixed bones, exact solver (B4) | `physics-shells.md` 3.2 item 5, 3.5, 4.4; the door: 10 item 4 |
| 2.1 Which bodies the hinge joins (B6, B7, B7b) | `physics-shells.md` 3.3; the door: 10 item 3 |
| 2.1 The library's attach rule, stops, motor (P1, B7d) | `physics-joints.md` 2.1, 2.2 |
| 2.2 What that gives (B7c) | `physics-joints.md` 2.3 (general), 3 (the door) |
| 2.3 No motor at creation (B6b, B6c) | `physics-joints.md` 1, 3 |
| 3.1 It starts at rest (B8, B8b, B8c, D2, D2c, B9) | `physics-resting.md` 1, 2, 4 |
| 3.1 Entered in the collision index with its mark (B8d) | `physics-contacts.md` 1.5 |
| 3.2 Whether it comes to rest again (B10, P2) | `physics-resting.md` 3, 4 |
| 4.1 It is still collided (B11) | `physics-contacts.md` 1.1, 1.5 |
| 4.2 It is woken by a kept contact (B12) | `physics-contacts.md` 1.2, 1.5; the player's shapes: 7 item 5 |
| 4.3 The merge and the solver (B13, B4c) | `physics-contacts.md` 1.6, 7 item 5 |
| 5 `set_nonscript_usable` (B14) | the setter and default: `script-api-game-object-basics.md` 6.3 (already owned by M, unchanged) and `use-action.md` 10.1; hint beside the flag: `use-action.md` 10.1 |
| 5 The use action and the flag (D5, B15, B16, D6) | `use-action.md` 1.1 (the action `use`, 42), 2.2 (the crosshair record; the hint reads only its text), 3 (the steps and the gate), 10.2, 10.3 (the flag's readers and what it does), 7.1 and 11.1 (`[ph_capture_visuals]`; the trader's door) |
| 6 The record's mass (B17) | `physics-shells.md` 4.1 table, 4.5; the door: 10 item 1 |
| 7 Corrections: `scope` 1.2, `bone` 8, `world` 4.2, `door` 2.4, `door` 2.3 (first element) | `physics-shells.md` 11 |
| 7 Corrections: `door` 2.3 (angle, closed test, motor) | `physics-joints.md` 4 |
| 7 Corrections: `door` 3.2, `world` 4.2 (merge) | `physics-contacts.md` 8 |
| 8 Unestablished: checking-build callers | `physics-shells.md` 1, 12 (already there) |
| 8 Unestablished: rest while held, other wakers, rest rule | `physics-resting.md` 4, 5 |
| 8 Unestablished: solver of a merged group | `physics-contacts.md` 1.6, 9 |
| 8 Unestablished: living character's action; believed single flag reader | `use-action.md` 6.2 (the action is the talk); 10.2 (the flag has three readers) |
| 8 Believed: frame body never moves; mass reader | `physics-shells.md` 3.5, 4.5, 12 |
| 8 Believed: φ = 0 is the placed pose | `physics-joints.md` 3, 5 |
| 9 Provenance | the provenance of each receiving spec |
| Gaps found in review 1 – 3 (use action) | `use-action.md` 3 steps 2, 3 (gap 1); 3 step 7 and 6.1 (gap 2); 10.2 (gap 3) |
| Gaps found in review 4 (activated switched off, resting list) | `physics-resting.md`, Gaps found in review |

## Gaps found in review

- `use-action-usable-flag.md` was not approved as a separate specification: reading the flag belongs to the use action's own specification, `use-action.md`, which absorbs those facts (sections 1.1, 2.2, 3, 6.2, 7.1, 10, 11.1), with the checks `SR-B14`, `SR-B14b`, `SR-D5`, `SR-B15`, `SR-B15b`, `SR-B15c`, `SR-B15d`, `SR-B16`, `SR-D6`, and answers the first three review gaps.
