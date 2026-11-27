# Physics (scoping specification)

Scoping specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the area of issue #278:
everything the original's physics simulation does in single player: the physics world and its stepping, the
player's physical body and its contacts, physical objects and items (dropping, throwing, falling, resting),
skeletons and ragdolls, joints and breakable objects, impulses from hits and explosions, and the physics data
in the game's configuration and in the models. Section 1 states what produces each in the original; section 2
what the approved specifications already cover, with the player's body in detail; section 3 proposes the split
into small specs, one format or one behaviour each, with their scope, dependencies and size; section 4 orders
them and maps the open work that waits on them; section 5 states what the choice of a physics library must
respect. It specifies no behaviour in full: the specs of section 3 do. What could not be established is marked
**unestablished**; what is inferred rather than read from the data or the program is marked **believed**.

Builds on (not repeated): `docs/formats/level-cform.md` (the static collision mesh, game materials, the
contact model, the player's collision body), `docs/behavior/player-movement.md` (the character physics of the
player on foot), `docs/behavior/climbing.md`, `docs/behavior/environment-hits.md`,
`docs/behavior/player-hits.md`, `docs/formats/sav.md` (record and save layouts), `docs/behavior/level-objects.md`
(models, bones, the layout of the bone physics data), `docs/formats/material-pairs.md`,
`docs/behavior/hanging-lamps.md`, `docs/behavior/artefact-idle-effects.md`, `docs/formats/ltx-config.md`,
`docs/behavior/script-api-scope.md`, `docs/behavior/level-sound-scope.md`,
`docs/behavior/narrowed-specs-scope.md`, `docs/behavior/trader-and-crows-scope.md`.

Terms. A **body** is one simulated rigid body; a **shell** is the set of bodies and joints that simulates one
object; a **joint** connects two bodies (or a body and the world). "Step" is one fixed step of the simulation,
"frame" one rendered frame. "Scripts" are the shipped Lua scripts (`scripts\*.script`) and "logic" the scheme
sections that a record's custom data or a logic file gives it; both are data. Class ids and section names are
those of the game's configuration (`config\system.ltx` tree). Every fact carries the name of its check, a
function of `checks.py` in `checks/physics-scope/` of the dirty workspace (`run_all.py` runs them all).

## 1. What produces each in the original

### 1.1 The library and the world

**The library.** The game simulates physics with the shipped library `bin\ode.dll`, whose own messages name it
ODE (a public rigid-body library; "recompile ODE", "ODE INTERNAL ERROR"); `bin\xrGame.dll` uses 142 of its 316
exported functions [B1]. What the game uses of it: an iterative step and an exact step, the iteration count,
the world's error reduction and constraint-force mixing (their values: `level-cform` 4.2) and gravity; ball,
hinge, two-axis hinge, slider, angular-motor and contact joints, and contact joints of a second, game-defined
kind; box, sphere, ray, offset-transform and game-defined geometry kinds; switching bodies off; reading the
forces a joint transmits [B1b]. It does not use the library's own automatic switching-off of resting bodies,
its capped cylinders or its triangle meshes [B1c]: resting and the collision with the level mesh are the game's
own, and so (believed) is the collision of the cylinders of the bone data (PH-4, PH-5).

**The world.** One world per level (believed). Each frame whole steps of a stored step length (initially 0.01 s;
`ph_frequency` sets it, `level-cform` 6.1) are run from a time accumulator, the remainder kept for the next
frame; more than 20 steps in one frame logs a diagnostic message [B2] (the frame time is limited to 0.1 s,
`player-movement` 6.1, so at most 10 steps run at the default step [A2b]). `ph_frequency` and `ph_iterations` are
console variables [B2b] (`console-scope`, `r1-settings-scope`). The world as a whole can be frozen and
unfrozen [B2c]; when the game does so is **unestablished** (believed: while the game is paused). Gravity is
19.62 m/s² downward (`player-movement` 6.1); that the same value acts on every body is **believed**.
**Disputed in review:** not every body: `environment-hits` 6.5 establishes that the no-gravity zone kind
(`Z_NOGRAV`, one record, `zone_no_gravity` on `l11_pripyat`) switches the gravity of the objects in its list
and of objects entering and leaving it.

**Resting.** Every shell has rest thresholds that start at 0.001 and 0.1 (linear), 0.005 and 0.05 (angular)
and a count 64 [B3]; a model's user data section `[disable]` multiplies the two linear thresholds by
`linear_factor` and the two angular ones by `angular_factor`, and `change_count` shifts the count by that many
bits [B3b]. 474 model files have `[disable]` (with both factors; `change_count` in 1) [D5]. How the thresholds
and the count decide that a shell rests and wakes is **unestablished** (PH-5). `artefact-idle-effects` 6.1
records that resting also stops an object's per-frame processing [A2c].

### 1.2 What is simulated

| Kind (class id, section) | Records in `spawns\all.spawn` | Shell |
|---|---|---|
| physics object (`O_PHYS_S`, `physic_object`) | 485 (69 on `l01_escape`) | by its **physics type** (below) [D1, D2] |
| destroyable physics object (`P_DSTRBL`, `physic_destroyable_object`) | 1,420 (127 on `l01_escape`) | by its physics type; destroyed into part models (1.4) [D1, D10b] |
| breakable (`O_BRKBL`, `breakable_object`) | 406 (49 on `l01_escape`) | from its level-local model `levels\<level>\meshes\brkbl#n.ogf` [D1, D10] |
| skeleton object (`P_SKELET`, `ph_skeleton_object`) | 0: created at run time for parts that break off (1.4) | from its model [D1, D3c, B10b] |
| items (inventory objects lying in the level) | 459 items, 96 ammunition, 44 outfits, 34 weapons (`sav.md` 15) [A2] | a one-body box (or box-and-sphere) shell from the model's bounds [B5]; whether every item kind uses one is for PH-6 |
| hanging lamps (`O_HLAMP`) | 745; 10 with the physics-shell bit [D13] | from the model with fixed bones (`hanging-lamps` 3.4) |
| characters and monsters | 1,073 stalkers, 276 monsters, the trader, the player [A2] | a character body while alive (the player's: `level-cform` 5); a jointed skeleton shell at death (ragdoll, 1.4) [B7b] |
| crows (`AI_CROW`) | 197 | a shell at death (`trader-and-crows-scope` 1.2) |
| vehicles (`SCRPTCAR`), helicopters (`C_HLCP_S`) | 6, 27 | believed from their models, with `[car_definition]` / `[helicopter_definition]` [D1, D5d] |
| thrown and launched objects | bolt (`II_BOLT`), grenades, rockets: created by the player's and characters' actions | believed item shells; rockets build their own [D7b, B12] |

Records of physics objects carry, after their Skeleton block, a `u32` physics type, a `float` mass and a
`stringZ` of fixed bones (`sav.md` 7.4.2). The program builds the shell by the type [B4]: **0**, one body whose
box is the model's bounding box, of the record's mass [B4b]; **1** and **2**, one body per bone from the bone's
box, recursively from the root, with joints between them (type 2 does not fix the root) [B4c]; **3**, from the
model's bone physics data (1.3), with the named bones fixed; a fixed-bone name the model lacks is fatal ("wrong
fixed bone") [B4d], and a model whose root bone has no shape is fatal [B4e]. In the data type 3 is in 1,895
records and 0 in 10 (`O_PHYS_S`: nine character models on `l05_bar` and `l12_stancia_2`, one garbage model);
types 1 and 2 are unused; the mass is 10 in 1,890 records; 512 records name fixed bones [D2, D2b, D2c, D2f]. The
startup animation of the Skeleton block is `$editor` in 1,688 records, empty in 179, `idle` in 18 and other
names in 20 [D2d]. 303 physics-object records have script logic [D2e]. Both physics-object sections keep a
removed object for `remove_time` 60 s and bind `bind_physic_object.init` [D3].

On the first level: 137 of its 196 physics objects have one-bone models and 28 name fixed bones [D9b]. The
trader's door `trader_door`, 1.7 m from the player's start in the bunker [A2d], is type 3 with the fixed bone `link`; its model's
bone `door` hangs from `link` on a joint of type 2 limited to −1.92 … 0 rad about one axis, and it has script
logic [D9].

### 1.3 Physics data

**Bone physics data** (chunk 16 of skeleton models; layout in `level-objects` 4.4, which reads only the bind
pose): 1,579 model files have it [D4]. Per bone: shape type 0 … 3 (7,343 / 8,976 / 2,490 / 1,876 bones; with
the box, sphere and cylinder stored beside it) and shape flags 0, 1, 2, 4, 5, 6 [D4b, D4f]; joint type 0 … 5
(12,982 / 48 / 2,854 / 70 / 4,710 / 21 bones) [D4c] with 12 limit values, spring, damping and joint flags;
break force at least 1 on 275 bones and within 1 of 0 on the others [D4d], and break torque; friction; mass and
centre of mass; a game material, 22 distinct, `objects\dead_body` on 9,683 bones [D4e]. Every breakable's model
has joint type 4, box shapes and break force 0 on every bone [D10]. The meaning of each type and flag value is
**unestablished** here (PH-1).

**Model user data** (chunk 17, LTX text; an `#include` in it names a file under `config\`, e.g. the player's
model includes `config\models\capture\stalker_hero_captures.ltx` [D6d]). The program reads, among others
[B6, B6b]: `[disable]` (1.1); `[collide]` `not_collide_parts`, `ignore_static`, `small_object`,
`ignore_small_objects`, `ignore_ragdoll`; `[collide_parts]` `small_object`, `ignore_small_objects`;
`[physics_common]` `fixed_bones`; `[destroyed]` (part model names, `destroyed_vis_name`); `[particles]`
`destroy_particles`; `[sound]` `break_sound`; `[impulse_transition_to_parts]` and
`[impulse_transition_from_source_bone]` (`random_min`, `random_hit_imp`, `ref_bone`, `imp_transition_factor`,
`lv_transition_factor`, `av_transition_factor`); `[autoremove]` and `[autoremove_parts]` `time`. In the data
[D5, D5b, D5c]: `[collide]` only with `not_collide_parts` (39 models); `[collide_parts]` `small_object` in 2;
no `[physics_common]` and none of the other `[collide]` keys; `[destroyed]` 37, `[autoremove_parts]` and
`[impulse_transition_to_parts]` 31 each, `[autoremove]` 1, `[particles]` 27, `break_sound` 27,
`[collision_damage]` 38, `[damage_bones]` 18, `[capture]` 244 (mostly through includes).

**Configuration keys** read by the program, with where the data has them:

| Keys | Where | Check |
|---|---|---|
| `ph_box0_size` … `ph_box2_center`, `ph_mass`, `ph_crash_speed_min`, `ph_crash_speed_max`, `ph_collision_damage_factor` | character and monster sections; `[actor]` `ph_mass` 80, crash speeds 14 / 30, damage factor 1.0 | B9, D6b; **Disputed in review:** B9 shows only `ph_mass`, the crash speeds and the damage factor read together; that the `ph_box*` keys are read with them, and in how many sections the data has them, is unbacked |
| `ph_skeleton_airr_ang_factor`, `ph_skeleton_airr_lin_factor`, `ph_skeleton_hinger_factor1`, `ph_skeleton_ddelay`, `ph_skel_fatal_impulse_factor`, `ph_skeleton_skin_ddelay`, `ph_skeleton_skin_friction_start`, `ph_skeleton_skin_friction_end`, `ph_character_have_wounded_state`, `ph_skeleton_skin_ddelay_after_wound`, `ph_pelvis_factor_low_pose_detect`, `ph_skel_shot_up_factor`, `ph_after_death_velocity_factor`, `bonce_damage_factor_for_objects` (ragdolls) | 169 sections, among them the player's `[actor]` through its parent `common_ph_friction_params_on_npc_death` | B7, D6, D11 |
| `destroyed_vis_name` | 165 creature sections | D11c |
| `[physics]` `object_damage_factor` (1.2; used squared) | `system.ltx` | B9b, D3d |
| `[sound]` `snd_collide_min_volume`, `snd_collide_max_volume` (0.1, 200) | `system.ltx` | D3e |
| `remove_time`, `hit_break_threthhold`, `collision_break_threthhold`, `immunity_factor` | `[breakable_object]`: 10, 0, 2000, 1.3 | B10, D3b |
| `remove_time` | `[ph_skeleton_object]`: 120 | B10b, D3c |
| `hit_impulse` (weapons), `k_impulse` (ammunition) | 102, 53 sections | B8b, D7 |
| `blast`, `blast_r`, `blast_impulse`, `frags`, `frags_r`, `frag_hit`, `frag_hit_impulse`, `up_throw_factor`, `fragment_speed` | explosives: 21 sections | B8, D7 |
| `hit_impulse_scale`; `throw_in_impulse`, `throw_in_impulse_alive`, `tele_height`, `time_to_tele`, `tele_pause` | zones: 61; mincer kind: 12 | B8c, D7 |
| `impulse_threshold`, `strike_impulse` | `[af_fireball]`: 10, 20 | B8d, D7c |
| `force_min`, `force_const`, `force_max`, `force_grow_speed`, `throw_point`, `throw_dir` | the bolt and grenades | D7b |
| `[capture]` `bone`, `distance`, `time_limit`, `pull_force`, `pull_distance`, `velocity_scale`, `capture_force`; `[ph_capture_visuals]` | model user data; `system.ltx` lists 147 character models, "objects that can be dragged" | B11, D6c, D6d |
| `[ph_heavy]` in an object's custom data | 4 records; tested when nearby objects are gathered for telekinesis-like effects | B11b, D8 |
| `max_velocity_in_zone` | `[zone_ameba]` 0.5 | D11b; **Disputed in review:** D11b shows only the value in the data; that the program reads the key is unbacked, and no record of `spawns\all.spawn` has the section `zone_ameba` |

**Game materials** (`level-cform` 3–4): friction, damping, spring, bounce and the flags of each material, the
contact properties of a pair, the liquid damping; every bone material above is a material of this library with
the **dynamic** flag 0x100 [D12], which `level-cform` 3.3 excludes from the level mesh.

**Saved state** (`sav.md`): the Item update (physics elements, position, rotation, two velocities), the
Skeleton block's bone state, the physics flag of the client data, the player update's physics fields
(**unestablished** there).

### 1.4 Behaviours

- **Falling and resting** of every shell (1.2), with contacts against the level mesh and each other; resting
  (1.1).
- **Joints and breaking**: joints from the bone data, with limits, springs, damping and friction; a joint
  is believed to break by its break force and torque; shells split [B10c]; a part that breaks off becomes a new
  skeleton object (`P_SKELET`, removed after its `remove_time`) [B10b]; `[impulse_transition_*]` and
  `[autoremove_parts]` govern the parts.
- **Destruction**: destroyable objects read `[destroyed]`, `[immunities]`, `[sound]` and `[particles]` of their
  model [B12d] and are believed replaced, when destroyed, by the part models `[destroyed]` names; a model's
  `[collision_damage]` gives a factor per bone, an unknown bone name being fatal [B12c]; breakables break by hit
  power and collision impulse (`hit_break_threthhold`, `collision_break_threthhold`).
- **Impulses**: hits move what they hit through its shell (weapon and ammunition impulses, zone hits scaled by
  `hit_impulse_scale`, explosion blast and fragments, the fireball artefact's strike, the mincer kind's pull and
  telekinesis) [B8, B8b, B8c, B8d]; `environment-hits` 8 and `player-hits` 7 hand their direction, point and impulse to the
  physics [A2e].
- **Collision damage**: impacts hurt characters (`ph_crash_speed_*`, `ph_collision_damage_factor`) and objects
  (`object_damage_factor`, `[collision_damage]`), and make contact sounds (`level-sound-scope` S13).
- **Ragdolls**: a dying character or monster switches from its animated skeleton to a jointed shell [B7b] (the keys
  of 1.3); a corpse is believed to be swapped later for the model `destroyed_vis_name` names.
- **Dragging** (`[capture]`, `[ph_capture_visuals]`): the player can drag the bodies of the listed models
  (believed from the list's own comment; the input and the rules are **unestablished**).
- **Throwing**: the bolt and grenades leave the hand with a force believed to lie between `force_min` and
  `force_max` [D7b].
- **Scripted schemes**: the logic of physics objects uses the schemes `ph_door`, `ph_idle`, `ph_sound`,
  `ph_button`, `ph_oscillate`, `ph_code`, `ph_car`, `ph_force`, `ph_gate`, `ph_on_death`, implemented in 15
  scripts `scripts\ph_*.script` [D8, D8b] through the script API's physics part (`script-api-scope` Y).
- **Vehicles and helicopters**: wheeled vehicles from `[car_definition]`, the helicopter's crash.

## 2. What the approved specs cover

| Need | Approved coverage | Missing |
|---|---|---|
| Static collision mesh, materials, contact properties | `level-cform` 2–4 (friction, softness from the world's constants, bounce, friction pyramid, pass-through, liquid flag) [A1] | contacts of movable bodies with the mesh and each other; material factors 0, 2, 3; the liquid damping's conditions (its section 8) |
| The player's body and movement | `level-cform` 5–6, `player-movement` 6 (fixed step, accumulator, gravity 19.62, forces, caps, jump) [A1, A1b] | see 2.1 |
| Ladders | `climbing` (its physics step) [A1g] | when the physics world tells a ladder about a character (believed only, #392) |
| Hits, falls, zones | `environment-hits` (impact speed, fall damage, zone hits), `player-hits` (`physic_strike`) [A1e, A1i] | their direction, point and impulse acting through the physics (#701, #702); the mincer pull (#700) |
| Model bone data | `level-objects` 4.4: the layout; only the bind pose read [A1c] | every physics field's meaning |
| Saved physics state | `sav.md` 7.4, 8: layouts [A1d] | the meanings of the physics state, the player update's physics fields |
| Collide and break sound strings | `material-pairs` (S9) [A1i] | which contacts play them (S13, S14) |
| Hanging lamps, artefacts | `hanging-lamps` 3.4 (shell built with fixed bones), `artefact-idle-effects` 6.4 (jumping impulse) [A1f] | the shells' motion |
| Physics objects at the first frame | `level-objects` (drawn at their records' transforms; settling deferred) [A1c] | settling |

No approved specification covers the physics simulation [A1h, A1j].

### 2.1 The player's body: covered and left

Covered (`level-cform` 5–6, `player-movement` 6, `climbing`, `environment-hits` 3): the four shapes and their
sizes per posture, the never-rotating body of mass 80, the contact rules against the level (dropped downward
contacts of the bottom sphere, head softening, softening by the normal, traction, friction zero while moving),
step assistance, the posture-change test, the step and accumulator, the forces, caps and jump, the impact speed
and the deadly ground.

Left, with the spec of section 3 that takes it:

- contacts with movable bodies and other characters: standing on, pushing and being pushed by them, the step
  rules while touching them, the sign of their contact normals (`level-cform` 5.4, 8) — PH-8;
- what a contact records for later use (point, direction, the other object) and the handler that may change the
  contact speed (`environment-hits` 3.2) — PH-8;
- the outer sphere's use and the ladder notification (`level-cform` 5.1; `climbing` 2.4) — PH-8;
- the pending vector from impulses of other objects (`player-movement` 6.1, 10; `environment-hits` 6.4) and
  the effect of hits' impulses — PH-11;
- the actor's material after creation, the liquid damping (`level-cform` 8) — PH-4;
- the player's death (ragdoll) and dragging bodies — PH-15, PH-16;
- the physics fields of the player update (`sav.md` 16) — PS-13 (#258), with PH-19.

## 3. Proposed specs

Names `PH-n`; target files under `docs/formats/` (formats) or `docs/behavior/`. "AI/scripts" says whether the
spec's behaviour needs character AI or the shipped scripts to be observed in play.

| # | Title (file) | Covers (data and observable behaviour) | Depends on | AI/scripts | Size |
|---|---|---|---|---|---|
| PH-1 | Bone physics data (`formats/bone-physics.md`) | every field of chunk 16 (1.3): shape types and flags, joint types, the 12 limits and their axes, spring, damping, joint flags, break force and torque, friction, mass, centre of mass, the material name; verified on all 1,579 models | `level-objects` 4.4 | no | M |
| PH-2 | The physics world and its step (`physics-world.md`) | the world's creation per level, gravity for every body, the step loop, `ph_frequency`, `ph_iterations`, which step (iterative or exact) runs when, the world's error reduction and constraint-force mixing for all joints, freezing and unfreezing, where the physics runs in a frame relative to object updates, the diagnostic messages | `player-movement` 6.1, `level-cform` 4.2, 6.1 | no | M |
| PH-3 | Shells from models and records (`physics-shells.md`) | building a shell: bones merged into bodies by joint type, mass and centre of mass per body, geometries per shape (box, sphere, cylinder), fixed bones, physics types 0 – 3 of physics objects, the simple shells, the shells of hanging lamps and breakables, the bones' transforms from the bodies (what is drawn), the fatal cases; shells built at spawn and the startup animation's part (`$editor`, `idle`) | PH-1, PH-2; `level-objects`, `hanging-lamps` 3.4 | no | L; splits, if one review round cannot check it, into building (bodies, geometries, mass) and the objects' use of it (types, fixed bones, spawn, drawing) |
| PH-4 | Contacts of movable bodies (`physics-contacts.md`) | collision of box, sphere and cylinder geometries with the level mesh (front side, pass-through, the game's own triangle test) and with each other; contact count, point, normal, depth; contact properties for two dynamic materials or a dynamic and a level material; liquid damping; the bounce; the collision filters (`[collide]`, `[collide_parts]`, small objects, ragdolls, parts of one object) | PH-2, PH-3; `level-cform` 2–4 | no | L; splits into geometry (contact generation) and properties with filters |
| PH-5 | Resting and waking (`physics-resting.md`) | when a shell rests (the thresholds and count of 1.1, `[disable]`), what resting stops (simulation, per-frame processing), what wakes it (contacts, hits, joints, scripts), the first-frame settling of objects placed by the level (LO-2) | PH-2, PH-3, PH-4 | no | M |
| PH-6 | Items in the world (`item-physics.md`) | when an item gets a shell (spawned lying in the level, dropped, thrown, taken out of a container) and loses it (taken), the box and box-and-sphere shells and which items get which, the mass, the dropped item's starting position and velocity, items of the Item update (`sav.md` 7.4.2: position, rotation, velocities at spawn) | PH-3, PH-4, PH-5; `inventory-screen` 5.7 | no | M |
| PH-7 | Joints (`physics-joints.md`) | the joint kinds built from the bone data (ball, hinge, two-axis hinge, slider, angular motor; rigid), limits, springs and damping, joint friction, the door's check that one axis is active, the wheel joint [B12b] | PH-1, PH-3 | no | M |
| PH-8 | The player's body and movable bodies (`player-body-contacts.md`) | 2.1's first three items: contacts between the player's shapes and other shells and characters, standing on and pushing objects and being pushed, the step rules then, the contact record (point, direction, cause; the fall hit's bone ray, #702), the outer sphere, the ladder notification (#392) | PH-4; `level-cform` 5–6, `player-movement` 6, `climbing` | no | M |
| PH-9 | Hit impulses on shells (`hit-impulses.md`) | how a hit's impulse, direction, point and bone move a shell: weapons (`hit_impulse`, `k_impulse`), the knife, zones (`hit_impulse_scale`), the fireball artefact (`strike_impulse`, `impulse_threshold`), the jumping artefact's push (#572) | PH-3, PH-5; `player-hits`, `environment-hits`, `artefact-idle-effects` 6.4 | no (hits by characters need AI) | M |
| PH-10 | Zone pull and telekinesis on objects (`zone-physics.md`) | the mincer kind's `throw_in_impulse` and telekinesis (`tele_height`, `time_to_tele`, `tele_pause`) on listed objects, `throw_out_impulse`, `max_velocity_in_zone`; physics objects in a zone's list (#699) | PH-9; `environment-hits` 4, 6.4 | no | S |
| PH-11 | Impulses on the player (`player-impulses.md`) | the pending vector of `player-movement` 6.1 from other objects' impulses, hits' impulses on the player's body (#701), the mincer kind's pull on the player (#700), being thrown by explosions | PH-8, PH-9, PH-10; `player-movement` 6 | no | S |
| PH-12 | Collision damage and contact events (`collision-damage.md`) | impact speed of shells, damage to objects (`object_damage_factor`, `[collision_damage]`) and to characters by objects (`ph_collision_damage_factor`), the contact events that sound specs use (S13: which contacts and how strong; `snd_collide_min_volume` and `snd_collide_max_volume` are read with the physics constants [B9b]) | PH-4; `environment-hits`, `material-pairs` | no | M |
| PH-13 | Breaking joints and split parts (`shell-breaking.md`) | joints breaking by force and torque, the split into new skeleton objects (`P_SKELET`, `remove_time` 120), `[impulse_transition_*]`, `[autoremove_parts]`, `[autoremove]`, the parts' saved bone state | PH-7, PH-9, PH-12; `sav.md` 7.4.1 | no | M |
| PH-14 | Destroyable and breakable objects (`object-destruction.md`) | `P_DSTRBL` health from hits (`[damage_bones]`, `[immunities]`), destruction into `[destroyed]` parts with `[particles]` and `[sound]` (S14), `remove_time`; `O_BRKBL` breaking by `hit_break_threthhold`, `collision_break_threthhold`, `immunity_factor`, `remove_time` 10 | PH-12, PH-13; `player-hits` | no (hits by characters need AI) | M |
| PH-15 | Ragdolls (`ragdolls.md`) | a character's or monster's death shell from its model, the hand-over from the animated pose (`ph_after_death_velocity_factor`), the ragdoll keys of 1.3, the fatal impulse and shot-up factors, wounded state, corpse removal, `destroyed_vis_name`; the player's own death; crows (TC7) | PH-3, PH-7, PH-9; `skeletal-motions`; motion playback (TC1) | AI for characters' deaths; none for the player's | L; splits into building and hand-over, and the ragdoll's own forces and friction |
| PH-16 | Dragging bodies (`dragging.md`) | the player's capture of a body listed in `[ph_capture_visuals]` by the `[capture]` values of the player's model, its input and limits | PH-15, PH-8 | no | S |
| PH-17 | Thrown and launched objects (`throwing.md`) | the bolt's and grenades' launch (`force_min` … `force_max`, `force_grow_speed`, `throw_point`, `throw_dir`), `destroy_time`, rockets' engine impulse | PH-6; weapons (none approved) | no for the player; AI for characters | S |
| PH-18 | Explosion impulses (`explosion-impulses.md`) | `blast_impulse`, `blast_r`, `frag_hit_impulse`, `frags`, `frags_r`, `up_throw_factor`, `fragment_speed` on shells and characters, the rays that decide what is hit | PH-9, PH-11; explosives (none approved) | no | M |
| PH-19 | Saved physics state (`physics-save.md`) | the Item update's fields, the Skeleton block's bone state, the client data's physics flag and item physics state, what loading restores; with PS-13 (#258) for the player update | PH-3, PH-6; `sav.md` | no | M |
| PH-20 | Scripted physics schemes (`physics-schemes.md`) | the schemes of 1.4 (doors with locks and limits, idle, sound, button, oscillate, code, force, gate, on-death), `[ph_heavy]`; the trader's door at the start | PH-7, PH-9; script API Y (#218), `script-environment` | scripts | L; splits by scheme |
| PH-21 | Vehicles and the helicopter's crash (`vehicle-physics.md`) | wheeled vehicles from `[car_definition]` and their joints and wheels (`ph_car`), the helicopter's death fall | PH-7, PH-12 | scripts and AI | L |
| PH-22 | Character bodies of others (`character-bodies.md`) | the body of a living character or monster (`ph_box*`, `ph_mass`, crash speeds), its contacts with the player and objects, the telekinesis query (`[ph_heavy]`) | PH-8 | AI | M |

## 4. Order and what waits

Order, each step after the ones it depends on:

1. PH-1, PH-2 (independent of each other).
2. PH-3; then PH-4 and PH-7 (independent).
3. PH-5; then PH-6 and PH-8 (independent). With PH-1 – PH-6 objects and items fall, rest and settle at the
   first frame, which is believed to change what the first level shows.
4. PH-9, PH-12; then PH-10, PH-11, PH-13.
5. PH-14, PH-19.
6. PH-15, PH-16, PH-17, PH-18 (they need deaths, weapons and explosives); PH-20, PH-21, PH-22 (scripts, AI).

Work that waits on them:

| Issue | Needs |
|---|---|
| #572 jumping artefacts | PH-2 – PH-6, PH-9 |
| #699 zones: other objects in a zone's list | PH-3, PH-6 (physics objects and items with bounds); PH-10 |
| #700 the mincer kind's pull and telekinesis on the player | PH-10, PH-11 |
| #701 environment hits' impulses on the player | PH-9, PH-11 |
| #702 the fall hit's bone, direction and cause | PH-8 (the contact); the ray against the player's bones needs their shapes (PH-1) |
| #392 the ladder notification | PH-8 |
| #234 LO-2 first-frame motion | PH-5 for settling; LO-2 itself covers the startup animation |
| #255 PS-8 taking items, a dropped item's fall | PH-6 |
| #111 inventory: dropped items | PH-6 |
| #258 PS-13 the player update's physics fields | PH-8, PH-19 |
| #136 S13 contact sounds | PH-12 |
| #137 S14 breaking sounds | PH-13, PH-14 |
| #218 script API Y | PH-3, PH-7 for the meaning of shells, elements and joints |
| #347 hanging lamps' shells and `ph_oscillate` | PH-3, PH-7, PH-20 |
| TC7 crow death | PH-15 |

## 5. The physics library

The behaviour to reproduce is the shipped library's as the game drives it: contacts softened by error reduction
and constraint-force mixing (`level-cform` 4.2), a friction pyramid, a fixed step, an iterative solver whose
iteration count `ph_iterations` sets, joints of the kinds of 1.1, and the game's own resting, collision with the
level mesh and contact rules on top. The specs of section 3 state it in library-neutral terms. Whether a library
(the public release of the same library, or another) reproduces it closely enough is for the clean side to
measure against black-box runs; any library used must be permissively licensed (no copyleft, including LGPL and
MPL), and its licence is to be read from its own distribution. The shipped library differs from a public
release in functions the game imports (a second kind of contact joint, game-defined geometry) **believed**
added for the game; PH-2 and PH-4 state what they do.

## 6. Unestablished

- When the world is frozen (believed: during pause) and whether gravity is the same for every body (PH-2).
- How the rest thresholds and count decide resting (PH-5).
- The meaning of each shape type, shape flag, joint type and joint flag value (PH-1).
- The input and rules of dragging; that it is the player who drags (believed from the list's comment; PH-16).
- What the second kind of contact joint and the game-defined geometries do (PH-2, PH-4).

## 7. Provenance

- Date: 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (later archives win); binaries `bin\xrGame.dll`, `bin\ode.dll`.
- Data examined: `spawns\all.spawn` (all records, physics-object blocks read with `sav.md` 7.4.2), the
  configuration tree from `config\system.ltx` (all sections), `config\creatures\actor.ltx`,
  `config\models\capture\*.ltx`, every model file (`meshes\**\*.ogf` and every level's `meshes\brkbl#*.ogf`) for
  bone physics data and user data, the custom data of all records and the logic files they name,
  `scripts\ph_*.script` (names only); the approved specifications listed above.
- Analysis: the import and export tables of `bin\xrGame.dll` and `bin\ode.dll` and the library's strings;
  static analysis of `bin\xrGame.dll` (headless decompilation with Ghidra) of the step loop, the rest
  parameters, the building of physics objects' shells by type, item shells, the configuration and user-data
  keys read for shells, collisions, destruction, breaking, ragdolls, impulses, explosions, zones, characters'
  bodies and dragging; constants read from the binary. No execution or debugging; the game was not run. The
  checks in `checks/physics-scope/` reproduce every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Monsters' telekinesis of objects: 8 creature sections (burer and poltergeist kinds) have `Tele_*` keys (object count, minimum and maximum mass, raise speed, hold time, fly velocity, find radius); lifting, holding and throwing physics objects is assigned to no spec (PH-22 names only the `[ph_heavy]` query).
- The no-gravity zone kind (`Z_NOGRAV`, `environment-hits` 6.5) switching gravity per object is assigned to no spec (PH-2 or PH-10).
- Explosive objects lying in the level (`II_EXPLO`, 139 records: `explosive_barrel` 92, `explosive_fuelcan` 17, `explosive_barrel_low` 16, `explosive_mobiltank` 11, `explosive_dinamit` 2, `explosive_tank` 1) are missing from table 1.2; their shells and their being set off by hits are assigned to no spec.
- Inventory boxes (`O_INVBOX`, 255 records, section `inventory_box`: `cform = skeleton`, a model under `physics\`, `script_binding = bind_physic_object.init`) are missing from table 1.2; whether they get a shell is not stated or assigned.
- Model user data `[door]` (`opened`, in one model, `meshes\physics\decor\freezer.ogf`) is not listed in 1.3; whether the program reads it is not stated.
- The library's ray geometry, imported by the game (1.1), is assigned to no spec: which queries use it (and whether rays test shells, e.g. for hits) is not stated.
