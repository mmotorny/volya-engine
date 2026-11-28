# Where `docs/behavior/trader-door.md` went

`docs/behavior/trader-door.md` followed one object through several systems. Every statement of it is now stated,
as a system fact with the door as a worked example, in the home given for its section, or is mapped "no home
yet: *system*" where no approved system specification owns it (each such system gets its own issue). Use this
table to repoint every citation of `trader-door.md` (also cited as `door`; `door` is also the name of the leaf's
bone): replace "trader-door *n*" by the home of section *n*. Where a section's content was split, the notes say
which part went where. Statements a later specification corrected are not carried over; the notes name the
correction.

Homes:

- **SH** `docs/behavior/physics-shells.md` (the door's record, model, shell build and drawing: 10)
- **J** `docs/behavior/physics-joints.md` (its hinge: 3)
- **RS** `docs/behavior/physics-resting.md` (its resting and waking: 4)
- **PC** `docs/behavior/physics-contacts.md` (its leaf against the player's body: 7)
- **PBC** `docs/behavior/player-body-contacts.md` (the player's rules for movable bodies; the door: 9)
- **STEP** `docs/behavior/step-assistance-movable-bodies.md`
- **UA** `docs/behavior/use-action.md` (the use action, the crosshair record, the door scheme's use callback: 4.1,
  11.1)
- **Y** `docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md` (the scripts' joint members; the door
  scheme's calls: 11)
- **F** `docs/behavior/script-api-configuration-and-files.md` (the door's logic file and its values: 9)
- **M** `docs/behavior/script-api-game-object-basics.md` (12), **OB** `docs/behavior/script-api-object-binders.md`
  (the physics-object binder: 8), **OC** `docs/behavior/script-api-object-event-callbacks.md` (`set_fastcall`,
  the level's call list: 5, 6, 10)
- **IP** `docs/behavior/info-portions.md` (9), **DG** `docs/behavior/dialogue-scope.md` (3), **CH**
  `docs/behavior/characters-scope.md` (4), **TC** `docs/behavior/trader-and-crows-scope.md` (the trader's start
  chain, 1.1), `docs/formats/dialog-files.md`, `docs/formats/character-descriptions.md`
- **OV** `docs/behavior/object-visual-and-transform.md` (object 79: 5.2), **BP** `docs/formats/bone-physics.md`,
  `docs/formats/level-cform.md` (cited as `cform`), `docs/formats/lanims-xr.md`, `docs/formats/colour-animations.md`
- **no home yet: PH-20** — the scripted physics schemes of `physics-scope.md` 3 (the door scheme `ph_door` and
  the logic's effects; the shipped scripts' own behaviour, run as shipped)
- **no home yet: HUD hint** — the hint of usable objects and its static, `hud-scope.md` 13 (`hud-pickup-hint`)
- **no home yet: script API P** — `set_tip_text` (`script-api-scope.md`, row P; M 12 assigns it there)
- **no home yet: object update schedule** — when an object's first update runs after a New Game

| `trader-door.md` | Home | Notes |
|---|---|---|
| Introduction, "Builds on" | each home's own | The scope statement (PH-20 with parts of PH-7 and PH-8, PH-9 not needed) carries no fact. |
| Conventions | SH 10; J 3; Y conventions | Door object, door (bone `door`), frame (bone `link`): SH 10. Units: `library`. Script names: Y conventions. |
| 1 The door object: the record | SH 10; F 9 item 2; OV 5.2 | Type 3, fixed bone `link`, model, `[logic]` and `[collide] ignore_static`: SH 10. The custom-data text naming `scripts\esc_trader_door.ltx`: F 9. Level, section, class: SH 10. "1.7 m from the player's start": OV 5.2 (object 79). |
| 1 The door object: the model table | SH 10 item 2; J 3; BP 3.2 | Bones, parents, the box, mass 100, the hinge about the bone's y axis limited to −1.9199 … 0, `lock` without physics, the material: SH 10 (with more precise values); the joint: J 3, BP 3.2 (example). The leaf's sizes (twice the half sizes): SH 10 item 2; the 2.645 m height: STEP 5. |
| 1 The door object: `ignore_static` | SH 5.2, 10 item 6; PC 1.3, 7 item 1 | Its "believed" bit is settled: SH 5.2 (with fixed bones), PC 1.3 (bit 0x2). |
| 2.1 The logic at the start: the binder | OB 8; UA 4.1 | Logic started at the first update once the player exists; every update passes "update" to the active scheme. When that first update runs: no home yet: object update schedule (OB 6 states only that the door stays scheduled). |
| 2.1 The logic at the start: the logic file | F 9 items 3 – 5; UA 11.1 | `[logic] active` = `ph_door@locked`, `locked`, `snd_open_start`, `on_info`, and the keys the section lacks: F 9. The sound set `trader_door_unlock` = `device\door_servomotor`: UA 11.1. The defaults the scheme gives the missing keys (`closed` true, `show_tips` true, `no_force` false, the locked hint `tip_door_locked` read from key `tip_open`): no home yet: PH-20. |
| 2.2 What starting the locked section does | Y 11 steps 2 – 4; OC 5, 6.1, 10; M 12; UA 10.3, 11.1 | The joint of bone `door` and its limits on axis 0: Y 11. The motor 200 / 3, then 10000 / 1: Y 11 step 4 (the values); the order and the "to block" mark, the silent first closing: no home yet: PH-20. The fast call: OC 5, 6.1, 10, which state it differently (one fast call per object, the three calls replacing each other; called once per level frame on the level's call list, after the physics steps); 2.2's "per step" and its disputed three entries are not carried over. The hint: `set_tip_text`, no home yet: script API P; the scheme's call of it: no home yet: PH-20. `set_nonscript_usable(false)`: M 12, UA 10.3 and 11.1, which settle the dispute (it changes nothing the player can see). |
| 2.3 The joint and the script's values: setter | Y 5.3; RS 2; J 1 | Waking, storing, the hinge's parameters 3 and 2 (public 0.5.0's numbering, `library` 5.1): Y 5.3, RS 2. No motor at creation (the belief settled): J 1; Y 5.1 ("Before any script setter"). |
| 2.3: the angle | Y 5.4; J 2.3 | The hinge angle whatever *n*, its zero at the hinge's making: Y 5.4, J 2.3. The shell at the bind pose at the record's transform: SH 7, 10 items 3 and 7. "Angle 0 is the door as placed" stays believed: J 3, 5. |
| 2.3: the stops | J 2.1, 2.3, 3 | Corrected: the door turns within φ = 0 … 1.9199 of the scripts' angle (J 3), not to −1.9199. |
| 2.3: `get_limits` | Y 5.2; J 2.3, 3; SH 3.3, 3.5, 10 | The rule: Y 5.2. The first element is the frame's own element, fixed (the belief settled): SH 3.3, 3.5, 10. The door's values (−0, 1.9199): J 3, Y 11 step 3. |
| 2.3: the closed test and its consequence | Y 11 step 5; J 3 | Corrected by J 3: "closed" holds only at φ ≤ 0 (the pose the hinge was made in, or pressed past it), not at any angle between the stops; "the fast call ends at its first step, one physics step after the start" is not carried over (J 3; OC 5 for when the fast call runs). |
| 2.4 How the door is held: limited and driven | J 2.3, 3; Y 11 step 4 | The closing motor (+1) drives toward φ = 0, the opening motor (−3) toward 1.9199, with the motor convention of J 2.3 (settling the belief); the torque 10000: Y 11. |
| 2.4: before the scheme starts | J 1, 3; RS 4 | No motor (settled); the shell rests until the first setter wakes it (RS 4), rather than being free between its stops. |
| 2.4: the frame | SH 3.3, 3.5, 5.1, 10 items 2 – 4 | Fixing and its values: SH 5.1. Forces applied to a fixed element ignored: SH 5.1 (added here). The dispute (which element `link` names) is settled: SH 3.5 (its own shapeless element). Built and activated before fixing: SH 4.4, 3.5. The hinge joins the door's body to the static environment, not to a frame body: SH 3.3, 10 item 3 (corrected). What keeps the frame in place: SH 3.5 (believed: nothing moves it). |
| 2.4: the masses | SH 10 items 1 – 2, 4.5; BP 5 | The door's 100; `lock` adds none. |
| 2.4: the joint's stiffness | J 3 (added here); BP 3.3 | Spring and damping 1 and 1, limit factors 1 and 1: the hinge and its stops as stiff as the world. |
| 2.5 Using the door while locked | UA 1, 3, 4.1, 11.1 | The use input, the use callback, the sound at the door, no switch without `on_use`. |
| 3.1 Which shapes meet | PBC 1.1, 1.3, 6, 9; PC 1.2, 5, 7 items 1, 3 | The four shapes in one collision space, the group collision of every shape with every other: PBC 1.1, PC 1.2. The outer sphere: PBC 6 (and STEP 2.1 for its size). The leaf's box as the only shape: PC 7 item 1. Which shapes give contacts with force: PBC 9 (also the three cylinders of PBC 1.1, 3.5). |
| 3.2 When the pair is collided | PC 1.1, 1.5, 5, 7 items 1 – 2; PBC 2.3 | Corrected by PC 1.5: resting objects are collided too. Whose pass and the normal's sign (settled): PC 1.1, 5, 7 item 2; PBC 2.3. The filter passing (settled): PC 7 item 1. |
| 3.3 The contact: properties and handling | PC 2.2, 2.3, 7 items 4 – 5 | Mode, friction, softness, bounce, the callbacks' order, the joint on both bodies. Corrected: the player's material is `creatures\actor` (PC 5), so the example values are PC 7 item 4 and PBC 2.3, 9. |
| 3.3: the player's rules | PBC 1.3, 2.2, 2.3, 9; STEP 4 | Normal softening with 5, traction 1, the impact, the unchanged rules, friction 0 while moving. The ERP / CFM example: PBC 2.3 (table), 9. |
| 3.3: how far the leaf yields | PBC 3.2 | Unestablished there. |
| 3.4 Not used here | F 9 item 5; no home yet: PH-20 | The logic sets no `hit_on_bone`: F 9 item 5. The scheme's switch on hits of named bones: no home yet: PH-20. "The approved `cform` contacts are against the level only" is superseded by PC and PBC. |
| 4.1 What unlocks it | F 9 item 5; IP 9 (added here); DG 3 | The locked section's `on_info`: F 9. Given only by phrase 32111; its other occurrences; without the talk the logic stays locked: IP 9. That the scheme tries `on_info` at every update: no home yet: PH-20 (OB 8 states that every update reaches the scheme). |
| 4.2 The dialog | CH 4; `character-descriptions.md` 9; TC 1.1; UA 6.2, 11.2; DG 3; `dialog-files.md` (worked example); IP 9 | The trader's start dialog: CH 4, `character-descriptions.md` 9. His start chain, talking off in its three waiting sections: TC 1.1. The player starts the talk with the use action: UA 6.2, 11.2. The path 0 → 12 → 3 → 31 / 32 → 321 → 3211 → 32111 → 321111, and 0 → 15 after 31: DG 3. `tutorial_end` given only by phrase 521111: IP 9 (added here). Speakers and texts: DG 3 (string ids), DG-1. |
| 4.3 What unlocking does | Y 11 step 4; J 3; no home yet: PH-20; no home yet: HUD hint | The opening motor 2100 / −3: Y 11, J 3. The effect `play_snd` playing 2D at the player, the switch to `ph_door@closed` (closed, unlocked), its closing again, the use switching to `ph_door@open`, `esc_close_door`: no home yet: PH-20. The hint `tip_door_open` and its text: no home yet: HUD hint. |
| 5.1 Trigger | UA 2.1, 2.2; no home yet: HUD hint | The crosshair record, only within `[inventory] take_dist` = 2: UA 2.2. The "other player condition" is settled: a monster controlling the player (UA 2.2). Which shapes the pick tests (settled): UA 2.1. A usable object's hint becoming the player's hint in single play, before the other hints; the texts `tip_door_locked` "Дверь заперта" and their string table: no home yet: HUD hint. The hint id set when the locked section starts: no home yet: PH-20 and script API P. |
| 5.2 Position and look | no home yet: HUD hint; `lanims-xr.md`; `colour-animations.md` 3, 4.3 | The `quick_info` static of `maingame.xml` and `maingame_16.xml`: no home yet: HUD hint. The entry `ui_btn_hint`: `lanims-xr.md`; its evaluation: `colour-animations.md` 3, 4.2 – 4.3. |
| 5.3 Duration | no home yet: HUD hint | |
| 6 Unestablished | per item | First update after a New Game: no home yet: object update schedule; motor before it: settled (J 1). Placed pose closed: believed (J 3, 5); the first element: settled (SH 3.3, 3.5). What keeps the frame in place: SH 3.5. Whose pass, normal sign, filter: settled (PC 1.1, 5, 7; PBC 2.3). The leaf's yield: PBC 3.2. The suppressing condition and the pick's shapes: settled (UA 2.1, 2.2). The HUD switches and the hint: no home yet: HUD hint. Speakers and texts: DG 3. |
| 7 Provenance | each home's provenance | The checks of `checks/trader-door/` the homes still use are rerun as `TD-…` by `checks/trader-door-retire/run_all.py`. |
| Gaps found in review 1 (shapeless root, the hinge's bodies) | SH 3.3, 3.5, 10; J 2 | |
| Gaps found in review 2 (`set_nonscript_usable(false)`) | UA 10.3, 11.1 | |
| Gaps found in review 3 (resting while held, collided and woken while resting) | RS 4; PC 1.5, 7 items 2, 5 | |
| Gaps found in review 4 (the record's mass) | SH 4.5, 10 item 1 | |

The other specifications that cited `trader-door.md` now cite these homes. `physics-contacts.md` still cites it
(`door` 3.1, 3.2, 3.3); its repointing waits for that specification's own fix.
