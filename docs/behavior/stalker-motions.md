# Stalker motions

Behaviour specification CH-11 of the characters scoping specification (`docs/behavior/characters-scope.md`,
section 5), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): which motions a stalker's model plays.
It covers, in full: the motion lists the engine builds from a stalker model's motion files and the names they are
made of; the stalker's motion update and its five slots (whole body, head, torso, legs, script); how each slot's
motion is chosen each update from the body, movement and mental states, the item in hand and the sounds; the
hand-over of the movement speed through the legs motion; the queue of script animations (`add_animation`,
`clear_animations`, `animation_count`) and how it takes over the model; the step manager's part for stalkers
(`LegsCount`, `step_params`, `foot_bones`) and `crouch_type`; the scripts that drive the queue (`state_mgr_animation`,
`state_mgr_animation_list`, `state_mgr_animstate`, `state_lib`). Worked example: a novice at the beginners' village
campfire on `l01_escape` sitting down, eating and playing the guitar. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/formats/skeletal-motions.md`: motion files, definitions, their flags, partitions and names (sections 1, 2,
  4); `docs/behavior/player-cycle-lengths.md` (S10): finding a cycle by name, last motion file first (section 2),
  a cycle's length *L*, speed and duration (section 3).
- `docs/behavior/motion-playback.md` (TC1): starting a cycle on one partition with mixing, a notification and
  channel 0 (3.1, 2.2), fading out the playbacks it replaces (3.3), the weight *a* of a playback (1, 4.2), the
  tracks update (4), a motion's end and its notification (5.1, 5.2), flags bit 3 (5.3).
  `docs/behavior/scripted-motion-channels.md` (TC2) is the trader's own motion system; nothing of it applies to
  stalkers, and nothing here applies to the trader.
- `docs/behavior/player-footsteps.md` (S11): the step table and its reading (section 1), the step timing (section 3)
  and the step sound (section 4); the same routines serve stalkers (section 9 here).
- `docs/behavior/script-api-movement-sight-and-memory.md` (X): `add_animation`, `clear_animations`,
  `animation_count`, `animation_slot` (10.3), `weapon_strapped` and the object handler's state numbers (10.2),
  `head_orientation` (3), the sight request (6.1); `docs/behavior/script-api-object-event-callbacks.md` (O): the
  callback table, the id 29 `script_animation` and how an entry is called (3.2);
  `docs/behavior/script-api-game-object-basics.md` (M 2.3): the routine that swallows diagnostics.
- `docs/behavior/stalker-planner.md` (CH-17) and `docs/behavior/smart-terrain-jobs.md` (CH-28): the stalker, its
  planner and its campfire job (their worked examples).
- Boundaries owned by specifications not yet written (`characters-scope` 2.2 – 2.6): the movement (CH-10: the body,
  movement and mental states, the speeds of `[stalker_movement_speeds]`, the body's and head's yaws and their
  turning, the path), the weapons and the object handler (CH-20: the item in hand, its states), critical wounds
  (CH-21: when a stalker is critically wounded, its body-part value and wound weights), the stalker's sounds
  (CH-25), script control (CH-24).

Conventions: angles in radians; "wrap(*x*)" is *x* brought into [−π, π] by adding or subtracting 2π; times of
`time_global()` in ms. "The generator" is the game's global pseudo-random generator of S11 4.2 (results
0 … 32,767). Each fact is **static** (from static analysis of `bin\xrGame.dll`, not observed at run time),
**measured** (from game data) or as marked. Bracketed ids ([B1] …) name the checks of
`checks/stalker-motions/checks.py` in the dirty workspace (B: the program, D: the data, K: the scripts;
`lists.py` is a model of section 2 written from this text); `export.py` regenerates the binary evidence.

## 1. Terms

The stalker's **states**, as its movement keeps them (CH-10; numbers as the scripts' `move` and `anim` values):

| Symbol | State | Values |
|---|---|---|
| *b* | body state | 0 crouched, 1 standing, 2 (no script value; **unestablished** when the movement uses it) |
| *t* | movement type | 0 walk, 1 run, 2 stand |
| *m* | mental state | 0 danger, 1 free, 2 panic |

- *v* is the stalker's current speed as its movement physics reports it (**believed**; CH-10). The stalker is
  **still** when *v* < 0.001 or *t* = 2, otherwise **moving** [B24, B29].
- θ_b and θ_b* are the body's current and target yaw, θ_h and θ_h* the head's current and target yaw, as the movement
  keeps them (CH-10; X 3 reads θ_h through `head_orientation`). ψ is the yaw of the current segment of the stalker's
  path in the same convention (**believed**: computed from the segment's horizontal direction) [B31].
- The **prefix** *P* of body state *b* is `cr_`, `norm_` or `dmg_norm_` for *b* = 0, 1, 2 [B1].
- A **list** is an ordered sequence of motions (section 2.2); "element *k*" is its motion number *k*, from 0.
- The **slots** are the five places where the stalker keeps a motion it plays: **global** (whole body), **head**,
  **torso**, **legs** and **script** (section 3).

## 2. The motion lists (static, measured)

### 2.1 Names

Names are made by appending table items ([B1]):

| Table | Items |
|---|---|
| body prefixes | `cr_`, `norm_`, `dmg_norm_` |
| item slots *s* | `0_`, `1_`, … , `10_` |
| torso actions | `draw_`, `attack_`, `drop_`, `holster_`, `reload_`, `pick_`, `aim_`, `walk_`, `run_`, `idle_`, `prepare_`, `strap_`, `unstrap_`, `look_beack_ls_`, `look_beack_rs_` |
| movement types | `walk_`, `run_` |
| directions *d* | `fwd_` (0), `back_` (1), `ls_` (2), `rs_` (3) |
| in-place names | `idle_0`, `idle_1`, `turn_right_0`, `turn_left_0`, `turn_right_1`, `turn_left_1`, `jump_begin`, `jump_idle`, `jump_end`, `jump_end_1` |
| head names | `head_idle_0`, `head_talk_0` |
| global names (index) | `damage_` (0), `escape_` (1), `dead_stop_` (2), `hello_` (3), `1_critical_hit_head_` (4), `1_critical_hit_torso_`, `1_critical_hit_hend_left_`, `1_critical_hit_hend_right_`, `1_critical_hit_legs_left_`, `1_critical_hit_legs_right_` (9), the same six with `2_` (10 – 15) and `3_` (16 – 21), `panic_stand_` (22) |

For each body prefix *P* the engine builds [B2]:

- the **global lists** *P* + global name (23 lists);
- the **torso lists** *P* + `torso_` + *s* + action, for each item slot and torso action (11 × 15 lists), written
  T(*s*, action, *k*) for element *k* of the list of the current prefix and T_norm(…) for that of `norm_`;
- the **movement lists** *P* + type + direction (8 lists);
- the **in-place set**: the ten in-place names, each one motion (or none) [B3].

And, once for the model: the two **head motions** `head_idle_0` and `head_talk_0` [B3], and the **item lists**
`item_` + *s* + action (11 × 15).

### 2.2 How a list is filled

A list of base name *N* is filled for *i* = 0, 1, 2, …: the name *N* + *i* (decimal, no padding) is looked up as a
cycle (S10 section 2) and, when there is none, as an fx motion; the motion found is appended. When neither exists,
the scan ends if *i* > 9 and goes on with *i* + 1 otherwise [B4]. So gaps among 0 … 9 are skipped, and from 10 on the
list runs only as long as the numbers are consecutive. The in-place and head names are looked up as cycles by the
exact name [B3].

The lists are built once for each distinct sequence of motion files and shared by every model that has the same
files in the same order (when the stalker's reload, section 3.1, finds no lists for its model's files) [B5].

Which lists the selection reads (sections 4 – 7): the global lists 1 and 4 – 21; of the torso actions `draw_`,
`attack_`, `holster_`, `reload_`, `aim_`, `walk_`, `run_`, `idle_`, `strap_`, `unstrap_`, `look_beack_ls_`,
`look_beack_rs_`; all movement lists; in-place names 0 – 5; both head motions. The other lists (global `damage_`,
`dead_stop_`, `hello_`, `panic_stand_`; torso `drop_`, `pick_`, `prepare_`; in-place jumps; all item lists) are built
but not read by the routines of this specification (**believed** unread anywhere; no reader was found).

### 2.3 The data

Of the 184 models under `meshes\actors\` with a motion-file list, 182 list `actors\stalker_animation` first, the others
are `barman_animation` and `trader_animation` (measured) [D3]. `stalker_animation.omf` defines the partitions `legs`
(0), `torso` (1) and `head` (2); the head motions are on the head partition, the movement and in-place motions on
`legs`, the torso motions on `torso` [D6].

The eight specific characters of class `esc_stalker_novice` (`character_desc_escape.xml`) use the models
`actors\novice\green_stalker_2` … `_5`, whose motion files are `actors\stalker_animation` and
`actors\critical_hit_grup_3`, and all have `crouch_type` −1 [D4]. Their lists include ([D5]; full dump
`bin/lists_novice.txt`):

| List | Motions |
|---|---|
| `norm_walk_fwd_`, `norm_run_fwd_` | 2 each (`…_0`, `…_1`) |
| `norm_walk_back_`, `_ls_`, `_rs_`; the same for `run_`; every `cr_` movement list | 1 each (`…_0`) |
| `norm_escape_` | 1 |
| `norm_torso_0_aim_`, `cr_torso_0_aim_` | 4 each |
| `norm_torso_0_idle_`, `norm_torso_0_walk_`, `norm_torso_0_run_` | 2 each |
| `cr_torso_0_idle_`, `cr_torso_0_walk_`, `cr_escape_` | empty |
| in-place names 0 – 5 of `norm_` and of `cr_` | present |

The other stalker models list further motion files after `stalker_animation` (measured, the 182 models) [D11]:

| Motion files after `stalker_animation` | Models |
|---|---|
| none | 57 |
| `critical_hit_grup_4`; `_6`; `_3`; `_5`; `_7` | 24; 16; 15; 13; 4 |
| `soldiers_animation`, `critical_hit_grup_5`; the same with `_6`; `soldiers_animation` alone | 14; 2; 3 |
| `zombie_animation`, `critical_hit_grup_1` | 10 |
| `bandits_animation` alone; with `critical_hit_grup_3`; with `_4`; with `stalker_drunk` | 5; 4; 3; 1 |
| `exoskeleton_animation`, `critical_hit_grup_2`; the same with `_7` | 5; 1 |
| `critical_hit_grup_7`, `zombie_wounded` | 3 |
| `ucheniy` | 2 |

What each further file does to the lists (2.2, the last file first) [D12]: no file adds a list that is empty with
`stalker_animation` alone, makes a list shorter, or changes a `cr_` or `dmg_norm_` list; the critical-hit files and
`zombie_animation` replace every motion of the lists they change, the others the elements named; in the shipped
sequences no list takes motions from two further
files, and where two define the same names (`bandits_animation`, then `stalker_drunk`) the later one's are taken.
In the table a list name without a prefix is that of `norm_`; *S* is the item slots 1, 2, 3, 4, 8, 9, 10.

| File | Motions it replaces in the lists (a list named alone: all its motions; lengths with the file) |
|---|---|
| `critical_hit_grup_1`, `_7` | `norm_2_critical_hit_` `torso_` (2), `hend_left_`, `hend_right_` (1 each) |
| `critical_hit_grup_2` | `norm_2_critical_hit_` `head_`, `hend_left_`, `hend_right_`, `torso_` (3 each), `legs_left_`, `legs_right_` (2 each) |
| `critical_hit_grup_3`, `_4` | `norm_2_critical_hit_` `head_`, `hend_left_`, `hend_right_`, `legs_left_`, `legs_right_` (3 each), `torso_` (2) |
| `critical_hit_grup_5`, `_6` | the six `norm_2_critical_hit_` lists (3 each) |
| `soldiers_animation` | `norm_idle_1`; element 1 of `torso_`*s*`_idle_` for *s* = 0 and *S*; `torso_`*s*`_strap_` (both) and `_unstrap_` (all four) for *s* in *S* |
| `bandits_animation` | as `soldiers_animation`, and element 1 of `walk_fwd_` and of `torso_`*s*`_walk_` for *s* = 0 and *S* |
| `stalker_drunk` | element 1 of `walk_fwd_` and of `torso_0_walk_` |
| `exoskeleton_animation` | element 1 of `run_fwd_` and of `torso_`*s*`_run_` for *s* = 0 and *S* |
| `ucheniy` | `head_talk_0`, `norm_idle_1`, `norm_turn_left_0`, `norm_turn_right_0`; `walk_back_` (1); element 1 of `walk_fwd_`, `torso_0_idle_`, `torso_0_walk_` |
| `zombie_animation` | both head motions; `norm_idle_0`, `_1` and the four turns; all of `walk_fwd_`, `_back_`, `_ls_`, `_rs_`, `run_fwd_`, `run_back_` and `damage_` (12); `torso_0_` `aim_`, `idle_`, `walk_`, `run_`; for *s* in *S* `torso_`*s*`_` `aim_`, `attack_`, `draw_`, `reload_`, `idle_`, `walk_`, `run_`, `strap_`, `unstrap_`; `torso_2_look_beack_ls_`, `_rs_` |
| `zombie_wounded` | none |

The further files also define names outside the lists (motions that scripts queue by name, 8.1): of the names each
defines, those `stalker_animation` defines too replace its own for these models (of their motions, for example 131 of 131 for
`bandits_animation`, 128 of 128 for `soldiers_animation`, 246 of 254 for `zombie_animation`) [D12]; which of them the
scripts queue is not surveyed here.

Where a rule below asks for an element beyond a list's end (for example element 1 of `cr_walk_fwd_`, or any element
of an empty list), what the stalker plays is **unestablished** (**believed** an undefined motion number read past
the list).

## 3. The motion update (static)

### 3.1 Set-up

- At the stalker's spawning its motion state is reloaded with the stalker [B41]: its crouch type preference is
  copied from its specific character (section 10) in every case; then, only when a further condition of the stalker
  holds (**unestablished** which), the lists of its model are taken (or built, 2.2) and, when in addition its health
  is above 0, the **partition mask** is made: every partition except the partition of `head_idle_0`'s definition
  (for the shipped models every partition but `head`) [B12, B42]. The head, shoulder and spin bone callbacks set up
  with the mask (under the same two conditions) are CH-10's.
- The slots start **empty**: no motion, no playback, no remembered list choice (section 4), and marked current
  [B12, B43]. Every emptying in this specification leaves a slot in the same state [B43]. An empty slot starts a
  motion as soon as one is chosen for it, because the chosen motion differs from none (3.4).

### 3.2 When it runs

The motion update runs from the stalker's client update, as part of the general client update of creatures, when
the scripted-action step (CH-24) reports no work, and not while the game is paused [B17] (**believed** once per
frame; the general update's further conditions on it are **unestablished**). It does nothing unless the stalker's
health is above 0 [B6].

### 3.3 One update

In this order [B7 – B11]:

1. If a script animation ended since the last update (8.3), the mark is cleared and callback 29
   (`script_animation`) of the stalker's callback table is called as O 3.2 describes, with no arguments [B7].
2. If the script queue is not empty and callback 29 is set, or if functions are registered on the global, head,
   torso or legs slot (3.5), the model's tracks update runs (TC1 4) [B8].
3. **Script mode**, when the queue is not empty: the global, torso and legs slots are emptied; the script slot
   plays the queue's front entry (section 8); then the head slot is chosen and played (section 5) [B11].
4. Otherwise the script slot is emptied and the global slot is chosen (section 4). If it gives a motion: the torso
   and legs slots are emptied, the global slot plays it, and the head slot is chosen and played [B10]. Otherwise the
   global slot is emptied, and the head (section 5), torso (section 6) and legs (section 7, with the speed hand-over
   7.4) are chosen and played in that order; last the **time carry** (3.6) [B9].

### 3.4 Playing a slot's motion

Each slot keeps its motion, the playback it started, and whether it is **current** [B13, B14]:

- When a motion is chosen for the slot, the slot stays current only if it was current and the motion is the one it
  holds; the chosen motion becomes the slot's motion [B14].
- A slot that is not current starts its motion and becomes current [B13]:
  - head, torso, legs: one start on the motion definition's own partition, mixing, channel 0, with the slot's end
    notification (TC1 3.1) [B13];
  - global and script: one start on each partition of the partition mask, in partition order (a partition the model
    lacks starts nothing, TC1 3.1 step 1), mixing, channel 0; only the first start that makes a playback gets the end
    notification and is kept as the slot's playback [B12, B13].
- The global, legs and script slots are **step slots**: each start of their motion also starts the steps with that
  motion (S11 3.2) [B12, B13].
- A start fades out the playbacks it replaces on its partitions (TC1 3.3), so a motion that leaves a slot fades out
  when another slot or the same slot starts on its partitions; emptying a slot stops nothing.

So a slot restarts its motion when the choice changes and when its motion ended (3.5); a looping motion that stays
chosen plays on.

### 3.5 End notifications

The end notification of a slot (TC1 5.2) marks the slot not current and calls, in order, the functions registered on
that slot [B15]. Which game code registers functions on the slots is **unestablished** (**believed** CH-21's critical
wounds and other actions; the scripts cannot). The torso's notification also ends a look-back (6.6); the script
slot's is section 8.3.

### 3.6 Time carry

When the torso and the legs slots both have a playback and both motions have flags bit 3 (TC1 5.3), the torso
playback's time is set to the legs playback's time, at every update [B16]. So a torso motion with bit 3 runs in step
with the legs motion; the walking motions `norm_walk_fwd_1` and `norm_torso_0_walk_1` both have it (1.334 s each)
[D10].

## 4. The global slot (static)

The first that applies [B18, B19]:

1. *m* = panic: when *v* < 1e−7, no motion; otherwise a motion of the list *P* + `escape_` [B18].
2. The stalker is critically wounded (CH-21: it has a wounded body-part value *k* and its planner property 29,
   `property_critically_wounded`, is true; when the property is false the value is cleared): if the global slot holds
   a motion, that motion; otherwise a motion of the global list number *k* + 6*w* − 6 of the **standing** (`norm_`)
   lists, *w* the active item's animation slot (with no active item **unestablished**), chosen with the stalker's
   critical wound weights (CH-21) [B19].
3. No motion.

**Choosing from a list** (also used in sections 6.3 – 6.5): the slot remembers the list it last chose from and the
motion chosen. When the list asked for is the remembered one, the remembered motion is given; otherwise the list is
remembered and a motion chosen: element *r* mod *n* (*r* one draw of the generator, *n* the list's length) without
weights; with weights *w*₀, *w*₁, … (the first min(*n*, number of weights) of them), the first element whose running
sum of weights reaches *r* · Σ*w* / 32,767 [B18, B19, B20]. The memory is forgotten whenever the slot
starts a motion other than the one chosen [B13].

So a panicking stalker on the move plays one escape motion on legs and torso, restarting it while it stays moving
and panicking; its torso and legs slots stay empty meanwhile.

## 5. The head slot (static)

`head_talk_0` while at least one of the stalker's sounds (CH-25) is playing and none of the sounds of its
`sound_running_in_danger` collection is playing or due; otherwise `head_idle_0` [B21]. The head slot is chosen and
played at every update in all three modes (3.3).

## 6. The torso slot (static)

### 6.1 The default rule R(*s*)

| *m* | still | walking (*t* = 0) | running (*t* = 1) |
|---|---|---|---|
| free | T(*s*, `idle_`, 1) | T(*s*, `walk_`, 1) | T(*s*, `run_`, 1) |
| danger, panic | T(*s*, `aim_`, 0) | T(*s*, `aim_`, 2) | T(*s*, `aim_`, 3) |

[B24].

### 6.2 Which item

The item in the stalker's active inventory slot (CH-20) decides [B22]:

1. No active slot, or nothing in it: R(0).
2. A weapon: when it is strapped (as `weapon_strapped()` reports it, X 10.2: a weapon that can be strapped, outside the
   object-handler states 3 – 6, that is strapped), R(0) [B23]; otherwise the weapon rule (6.3) with *s* the weapon's
   animation slot (`animation_slot()`, X 10.3).
3. A missile (grenade, bolt and the like, **believed**): the missile rule (6.4) with *s* its animation slot, but *s* = 0
   when crouched.
4. Any other item: the object-handler rule O(0) (6.5).

### 6.3 Weapon rule, by the weapon's state number (CH-20)

| State | Torso motion |
|---|---|
| 1, 2 | still: T(*s*, `attack_`, 0); walking: look-back (6.6) element 1, else T(*s*, `attack_`, 0); running: look-back element 0, else T(*s*, `attack_`, 3) |
| 3 | T(*s*, `reload_`, *r*), *r* = 0, 1 or 2 for the weapon's reload kind 0, 1 or other (CH-20) |
| 4 | a motion of T(*s*, `draw_`), chosen as in section 4 |
| 5 | a motion of T(*s*, `holster_`), chosen as in section 4 |
| 6 | R(0) |
| others | O(*s*) |

[B25]. The meanings of the numbers are CH-20's (**believed** from the motions: 1, 2 firing, 3 reloading, 4 drawing,
5 holstering, 6 hidden).

### 6.4 Missile rule, by the missile's state number (CH-20)

1: a motion of T(*s*, `draw_`); 3: T(*s*, `attack_`, 0); 4: T(*s*, `attack_`, 1); 5, 6, 9: T(*s*, `attack_`, 2);
8: a motion of T(*s*, `holster_`); any other: a motion of T(*s*, `aim_`); the choices as in section 4 [B26].

### 6.5 Object-handler rule O(*s*), by the stalker's object-handler state (X 10.2's numbers)

| State | Torso motion |
|---|---|
| 3, 4 | T_norm(*s*, `strap_`, 0), T_norm(*s*, `strap_`, 1) |
| 5, 6 | T_norm(*s*, `unstrap_`, 0), T_norm(*s*, `unstrap_`, 1) |
| 9, 10, 17, 18, 21, 22, 23, 24 | still: T(*s*, `aim_`, 0); walking: look-back element 1, else T(*s*, `aim_`, 0); running: look-back element 0, else T(*s*, `aim_`, 3) |
| others | R(*s*) |

The strap and unstrap motions are always the standing ones, whatever the body state [B27]. Which actions the numbers
are is CH-20's.

### 6.6 Look-back

Asked for only when *b* = 1 and *s* = 2 in the rows above. A look-back **side** (`look_beack_ls_` or `look_beack_rs_`)
is kept by the stalker; when none is kept, one is chosen only if the legs direction (7.2) is back and `time_global()`
has reached the **look-back time**: `ls` or `rs` for *r* mod 2 = 0 or 1 of one draw of the generator. With a side,
the torso plays element 1 (walking) or 0 (running) of T(*s*, side) [B28]. At the end of any torso motion while a side
is kept, the side is dropped and the look-back time becomes `time_global()` + 2,000 [B28]. The look-back time is also
set to `time_global()` at every update in which the legs are in place, and to at least `time_global()` when the legs
direction changes (7.2) [B28, B31, B33].

## 7. The legs slot (static)

Moving (section 1): 7.1 or 7.2; still: 7.3 [B29]. Each sets a **target speed** *s*₁ (and the in-place rule a kept
speed *s*₀) for 7.4. The speeds are entries of the movement's speed table (CH-10: the 19 speeds of
`[stalker_movement_speeds]`): number (2*b* + *t*) · 4 + *d* for danger, 16 + *t* for free, 18 for panic [B30, B31]
(which key is which entry is CH-10's).

### 7.1 Moving, free or panic

Element 1 of *P* + type + `fwd_`; *s*₁ the free entry (16 + *t*) or the panic entry (18) [B30].

### 7.2 Moving, danger

1. **Body direction.** The stalker keeps a body direction *e* (0 … 3, as *d*), a pending direction *p* and a time
   [B32]. A candidate *c* is taken from δ = wrap(ψ − θ_h): 0 when |δ| ≤ π/4, 1 when |δ| > 3π/4, otherwise 2 when
   sin(ψ − θ_h) ≤ 0 and 3 when it is above 0. Then: if *c* = *e* the time becomes now; else if *c* ≠ *p*, *p* := *c* and
   the time becomes now; else (*c* = *p*), when now − time > 500 · *k* ms, *e* := *c* and the time becomes now, where
   *k* = 0 if *p* and *e* (as they were at the start of this update) are opposite (0 and 1, or 2 and 3) and 1
   otherwise. Last, θ_b* := ψ + (0, π, π/2, −π/2) for *e* = 0, 1, 2, 3 [B32].
2. **Legs direction** *d*, from δ = wrap(ψ − θ_b) by the same rule with θ_b for θ_h [B31].
3. When *d* differs from the previous legs direction: the look-back time (6.6) becomes at least now; if *p* and *e* are
   now opposite, *s*₀ := *s*₁ := 0; the previous direction becomes *d* [B31].
4. Element 0 of *P* + type + direction *d*; *s*₁ the danger entry (2*b* + *t*) · 4 + *d* [B31].

So in danger the body turns to face away from the path by the offset of its direction, so that the head can keep
its target, and the legs walk forward, back or sideways relative to the body; the turn to a new direction waits
500 ms (none to the opposite one). Which way "left" and "right" fall depends on the yaw convention (CH-10;
**believed** as the motion names say). Every moving update also forgets the crouch type (7.3).

### 7.3 Still

*s*₀ := *s*₁ := 0. If no crouch type has been chosen since the stalker last moved, the crouch type *c* is chosen: the
stalker's preference (section 10), or *r* mod 2 of one draw of the generator when the preference is −1 [B33]. Then,
with *D* = wrap(θ_b* − θ_b) [B33]:

- |*D*| < 0.001 (the body has reached its target):
  - *m* ≠ free: crouched, in-place name *c* (`cr_idle_0` or `cr_idle_1`); otherwise `idle_0` [B33].
  - *m* = free: when the sight's turning flag (CH-10; X 6.1) is clear and |wrap(θ_b − θ_h)| ≤ π/6: `idle_1`
    (crouched too: `cr_idle_1`). Otherwise θ_b* := θ_h* and the stalker turns toward it (next item) [B33].
- Turning toward θ_b*: when sin(θ_b − θ_b*) ≤ 0, `turn_right_1` (free) or `turn_right_0` (otherwise); else
  `turn_left_1` or `turn_left_0` [B33].

The names are in-place names of the current prefix. The turn motions stop at the end (flags 2): while the stalker keeps
turning the slot restarts them (3.4) [D10].

### 7.4 The speed hand-over

After the legs slot is chosen and played [B34]:

1. If the legs motion changed, a previous one existed and the slot still has the previous playback, with *a* its weight
   (TC1): *s*₀ := *s*₀ + (*s*₁ − *s*₀) · *a* (with the *s*₁ just set).
2. The slot plays (3.4).
3. If the legs motion did not change and the slot has a playback of weight *a*: *v*′ = *s*₀ + (*s*₁ − *s*₀) · *a*; otherwise
   *v*′ = 0.
4. If |*v*′| ≥ 1e−7 and the slot has a playback, the movement's speed becomes *v*′ (CH-10 uses it).

The weight *a* of step 1 is that of the previous legs playback, the one the slot held before step 2 started the new
one; the weight of step 3 is that of the slot's current playback [B34, B44]. So:

- In the update in which the legs motion changes, *v*′ = 0 and the movement's speed is not written (left to CH-10).
- When the previous legs playback had reached full weight (*a* = 1, TC1 4.2), step 1 makes *s*₀ = *s*₁: from the next
  update on the speed is the new target at once, whatever the new playback's weight; there is no ramp.
- A ramp occurs only when the previous playback was still fading in (*a* < 1: *s*₀ moves only part of the way) or
  when there was no previous legs motion (the slot was empty: at spawning, or emptied by script mode or a global
  motion, 3.3), so that step 1 does not apply and *s*₀ keeps its earlier value. Then the speed moves from *s*₀ to
  *s*₁ as the new playback's weight rises from 1e−7 (TC1 4.2: by accrue · Δ per update) [B34, B44].
- While the legs motion stays the same, the speed is *s*₀ + (*s*₁ − *s*₀) · *a* of the current playback; a restart of
  the same motion (after its end, 3.4) gives a new playback of weight 1e−7, so where *s*₀ ≠ *s*₁ the speed drops to
  about *s*₀ and rises again.
  **Disputed in review:** this holds only while |*s*₀| ≥ 1e−7; with *s*₀ = 0 (for example a ramp from a slot emptied
  by script mode or a global motion after still legs, which left *s*₀ = 0) step 4 writes nothing in the update of the restart, so the speed is left to
  CH-10 there rather than dropped to about 0.
- Still legs set *s*₀ = *s*₁ = 0, so *v*′ = 0 and the speed is left to CH-10.

The hand-over runs only in the normal mode (3.3 step 4).

## 8. Script animations (static)

### 8.1 The queue

The stalker keeps a queue of **entries** (a motion and two flags *a*, *b*). `add_animation(name, a, b)` appends
(`name` looked up as a cycle, S10 section 2; a name the model lacks gives the diagnostic `There is no animation %s
(object %s)!` to the routine of M 2.3 and appends nothing); `clear_animations()` empties it and empties the script
slot; `animation_count()` is its length (X 10.3) [B36]. Every shipped call passes *a* = true and *b* = false (19 calls)
[K1].

### 8.2 Playing

While the queue is not empty the update is in script mode (3.3 step 3): the script slot is given the front entry's
motion and plays it on the partitions of the mask, every partition but the head's (3.4); the head slot goes on with
section 5. A front entry whose motion stops at the end plays until its end (8.3); one that loops plays until the queue
changes. When the entry's flag *b* is true, the first playback also becomes the stalker's motion controller (the root
bone's motion moves the object) [B38]; no shipped script uses it and it is not covered here. Flag *a* is kept with the
entry and not read by the routines of this specification (**believed** unused).

The other slots were emptied, so when the queue empties the normal selection starts the global or the torso and legs
motions afresh (fading the script motion out, TC1 3.3). `clear_animations()` itself stops no playback.

### 8.3 The end of a script motion

At the end notification of the script slot's playback: if the slot holds a motion, the queue is not empty and its front
entry's motion is the slot's motion, the front entry is removed and the script slot emptied; in every case the end is
marked, so callback 29 is called at the next step 1 of the motion update (3.3) [B37]. So a queue of motions plays them one after the
other; the same motion twice in a row plays twice (the emptied slot starts it again, 3.1).

When the next entry starts: at step 3 of the first motion update after the tracks update that found the end (TC1
5.2: the notification is called inside the tracks update) [B8, B11, B37]. While the queue is not empty and
callback 29 is set (as with the scripts of section 11), every motion update runs the tracks update in step 2,
before step 3 (3.3), so the next entry starts in the same motion update in which the end is reached; callback 29
is called at step 1 of the following update [B8, B45]. If a tracks update of the same model already ran earlier in the frame at the same `time_global()` (TC1 4.1: the second
one does nothing), that earlier one found the end; the next entry still starts in step 3 of this motion update, and
callback 29 is then called in step 1 of this same update. Whether any tracks update of the stalker's model runs
before its motion update in a frame is **unestablished** (TC1 4.1). Without callback 29 set and with no functions
registered on the slots, step 2 does not run, and the end is found by whatever tracks update comes next (TC1 4.1).

## 9. Steps (static, measured)

The stalker's steps use S11's step table, timing and sound (sections 1, 3, 4) with the stalker's configuration
[B39, B40, D1, D2]:

- `[stalker]` of `config\creatures\m_stalker.ltx`: `LegsCount` 2, `step_params = stalker_step_manager` (the same table
  as the player's), `foot_bones = stalker_foot_bones`, own material `material = creatures\human` [D1] (**believed** read
  as for the player, S11 section 1).
- The steps are loaded with the stalker's configuration and updated in its client update while its health is above 0,
  after the motion update (**believed** once per frame) [B39].
- Steps start whenever a step slot (global, legs, script) starts a motion (3.4), with that motion: a motion with a line
  in the step table steps, any other gives no steps until the next start (S11 3.2).
- The **foot bones** (the bones at which a step's particle effect starts, S11 4.3): from the section `[foot_bones]` of
  the model's own configuration when it has one, otherwise from the section the `foot_bones` key names; its lines
  `front_left`, `front_right`, `back_right`, `back_left` give the bones of legs 0, 1, 2, 3 [B40]. For stalkers
  `[stalker_foot_bones]`: leg 0 `bip01_l_foot`, leg 1 `bip01_r_foot` [D1]. When neither exists the result is a failed
  check with the description `section [foot_bones] not found in monster user_data` [B40].
- The ground material and the airborne state that S11 4.1 reads come from the stalker's own body (**believed**;
  PH-22).

Of the stalker's legs motions in the table [D2]: walking (`norm_walk_*_0`, `norm_walk_fwd_1`) power 0.2, running
(`norm_run_*_0`) 1, crouched (`cr_*_0`) 0.1, `norm_escape_0` 1.3, the `*_0` turns 0.2, leg 0 at 0.2 and leg 1 at
0.65 of a pass; `norm_run_fwd_1` (the free run), the idles and the `*_1` turns have no line, so they make no steps.

## 10. `crouch_type` (static, measured)

The preference is the integer `crouch_type` of the stalker's specific character (0 when the element is absent); the
`crouch_type` line of `[stalker]` (0, commented "0 - crouch a little, 1 - crouch full; -1 - random crouch") is not read
by the game library [B35]. It chooses `cr_idle_0` (0) or `cr_idle_1` (1) for a crouched stalker standing still outside
the free mental state, and −1 draws one at random each time the stalker stops (7.3). The novices of the example have
−1 [D4].

## 11. The scripts' part (measured)

### 11.1 Wiring

`state_mgr_goap` sets the stalker's callback 29 to the state animation's callback (11.3); each update of the state
manager updates the pose (11.2) and then the state animation (11.3) [K2]. Requests come from `state_mgr.set_state`
with a state of `state_lib` (each state names a weapon mode, movement, mental and body state, an `animstate` (the
pose) and an `animation`); its planner (`state_mgr_goap`) starts a pose change when no state animation is playing and the
direction, mental, weapon and movement are settled, and starts the state animation once the pose and the rest are
settled (preconditions of its actions) [K7]. While the pose is busy or the state animation is `_wait` or `_new` the
state manager is locked [K7].

### 11.2 Poses (`state_mgr_animstate`)

Poses: `stand`, `sit`, `sit_knee`, `sit_ass`, `sleep`, `wounded`, `wounded_heavy` [K3, K8]. Changing to pose *q*:
`clear_animations()`; the transfer motions listed from the current pose to *q* are queued (the `lazy` variant when it
exists and the stalker has neither a best enemy nor a best danger, otherwise `nrm`), then *q*'s idle motion
(`sit_0_idle_0`, `sit_1_idle_0`, `sit_2_idle_0`, `sleep_idle_0`; `stand`, `wounded` and `wounded_heavy` have none);
the pose is `busy`. At each update a busy pose becomes *q* when `animation_count()` is at most 1 (when *q* has an idle
motion) or 0 (otherwise) [K3]. The pose's idle can be queued again on request (`add_idle_animation`; nothing while the
pose is `busy` or has no idle). From `stand` to `sit_knee` the motions are `idle_0_to_sit_1`, `sit_1_idle_0`; back,
`sit_1_to_idle_0` or the lazy `sit_1_lazy_idle_0`.

Transfer motions are listed only between `stand`, `sit`, `sit_knee`, `sit_ass` and `sleep` (not from a pose to itself)
[K8]. When none is listed from the current pose to *q*, only *q*'s idle is queued, if it has one. So a change to
`wounded` or `wounded_heavy` queues nothing and settles at the first update after it (count 0), and a change from
them queues only the new pose's idle; a change requested while the pose is `busy` finds no transfer motions either
(only *q*'s idle). The state manager's planner requests pose changes only while it is not locked (11.1), so not while
the pose is `busy`; the case arises only for another caller of the pose change (**believed** none in the shipped
scripts: no other caller was found) [K7, K8].

### 11.3 State animations (`state_mgr_animation`, `state_mgr_animation_list`)

Each state animation of `state_mgr_animation_list` has `prop` (`maxidle`, `sumidle`, `rnd`), `into`, `out`, `idle`,
`rnd` and optional sub-levels `subanim[1 …]` (each with `prop`, `into_music`, `into`, `out`, `idle`, `rnd`). Each
motion field is indexed by the **weapon slot**: 0 when the stalker has no active item or its weapon is strapped,
otherwise the item's `animation_slot()`; a missing index uses 0 [K5]. A field's entries are motion names (queued) or
tables (`a` attach an item, `d` detach one, `s` a sound, `sh` a hit of that power, `f` a function), which are tied to
the motion queued just before them [K5].

Setting state animation *x* (from current *y*) [K5]:

1. `clear_animations()`; the tied tables are forgotten.
2. Leaving *y*: in an emergency request no `out` motions are queued (the items its `out` detaches are detached at once);
   otherwise `y.out` is queued. (A loop meant for the `out` motions of *y*'s sub-levels runs from the requested level up
   to 0 and so never runs for a level above 0.)
3. If *y* never settled, every attachable item is detached at once.
4. Entering *x*: `x.into` is queued; then `x.idle`, or, when *x* has none, the pose's idle (11.2).
5. *x* is `_new`; its level is 0.

At each update: pending attach and detach requests are applied; a `_new` or `_wait` state settles when
`animation_count()` is at most 1 (when *x* or the pose has an idle motion) or 0; a settled state follows level changes
(up: `clear_animations()`, the `into` motions of the new levels and their `into_music` sound, the new level's idle,
`_wait`; down: `clear_animations()`, the `out` motions of the level left, the idle of the new level (the state's own
idle for level 0), `_wait`) and plays random motions [K5, K9]. Going down from level *l* to level *n* < *l*, the `out`
motions are those of level *l* only when *n* = *l* − 1; for a drop of two levels or more no `out` motions are queued
(the loop over the levels left counts upward from *l* to *n* + 1, so it runs only when *l* = *n* + 1) [K9].

Random motions play at the first update after settling, and afterwards when `time_global()` − start > idle, with
idle = (random(`maxidle`) + `sumidle`) · 1000 ms drawn at the start; a random motion plays (always the first time,
otherwise when random(100) < `rnd`) by `clear_animations()`, queueing one entry of `rnd`, then the idle, and `_wait`
[K5]. The entry: with *n* = 1 entries, the first (the memory is not changed); with *n* > 1, when no entry was
remembered, random(*n*); otherwise *r* = random(*n* − 1), plus 1 when *r* ≥ the remembered index; the index chosen is
remembered [K5, K11]. So the remembered entry is not repeated when its index is at most *n*, and with a remembered
index above *n* the last entry is the one left out. The memory belongs to the stalker's state animation manager and
is kept across state animations: setting a state animation does not reset it [K11]. Random motions wait while a
synchronized sound plays and while the state manager's animation and weapon evaluators do not hold [K5].

**Reset.** A state animation set with reset (the state manager's reset action: a state of `state_lib` marked `reset`
was requested and the state manager's current state is the requested one) [K10]: after step 4's `into` motions and before the idle, the callback's
start time is cleared and the random-motion routine is called with the state animation's name in place of its entry
and 100 in place of "must play". The routine first tests the animation and weapon evaluators. The reset action runs
only while the animation evaluator is false (its precondition), and nothing between the planner's evaluation and the
action's start changes what that evaluator reads (**believed**: W 6.3 starts the action in the same planner update),
so the routine returns at that test and queues nothing [K10]. Were the test passed, the routine would fail on the name
(a script error: the name has no `prop` field) [K10].

The callback 29 function takes the first tied entry: a placeholder is dropped; a table is carried out (attach or
detach marked for the next update, sound played, hit dealt, function called) and dropped [K5].

## 12. Worked example: a novice at the campfire

The novice of CH-17 section 8 and CH-28 section 6 (`esc_lager1`, a specific character of class `esc_stalker_novice`,
model `actors\novice\green_stalker_2` … `_5`, crouch type −1) has taken a campfire job of `esc_lager`; its scheme
`kamp1@esc_lager` (`xr_kamp`) sets, through `state_mgr.set_state`, states such as `sit`, `sit_knee`, `eat_bread` and
`guitar` (and `guitar` with sub-level 1 while it plays) [K6]. All these states have weapon mode `strapped`, movement
`stand`, mental `free`, body `standing` [K4]. Durations are *L*/speed (S10 section 3); a motion that stops at the end
notifies 1/30 s earlier (TC1 4.2) [D7, D8, D10].

1. **Walking in.** Free, walking, standing, its weapon strapped or no weapon: legs `norm_walk_fwd_1` (power 0.2 steps
   at 0.2 and 0.65 of its 1.334 s), torso `norm_torso_0_walk_1` kept in time with the legs (3.6), head `head_idle_0`
   (or `head_talk_0` while it speaks). Coming from in-place legs that have reached full weight, there is no ramp
   (7.4): in the update of the change the legs leave the speed to CH-10, and from the next update on it is the free
   walk speed.
2. **Standing at the fire.** Still and free: legs `norm_idle_1`, torso `norm_torso_0_idle_1`; when its head is turned
   more than π/6 from its body, it turns with `norm_turn_left_1` or `norm_turn_right_1` (0.834 s each, restarted while
   turning) toward the head's target (7.3).
3. **Sitting down** (pose `sit_knee`, through `sit_knee`, `eat_bread` or `guitar`): the queue becomes
   `idle_0_to_sit_1` (2.501 s, stops at the end), `sit_1_idle_0` (6.670 s, loops); script mode: both play on legs and
   torso one after the other, the head goes on with its own motion; after `idle_0_to_sit_1` ends the pose is
   `sit_knee` (count 1).
4. **`sit_knee` state.** `clear_animations()`; no `into`, no idle of its own, so the pose's idle `sit_1_idle_0` is queued
   again (restarting it: the script slot was emptied). At the first settled update a random motion: the queue becomes
   one of `sit_1_idle_1`, `_2`, `_3` (6.670, 8.004, 6.670 s) and `sit_1_idle_0`; afterwards another one, with
   probability 79/100, each time 4 – 8 s have passed since the previous draw.
5. **Eating bread** (`eat_bread`, on the same pose): `clear_animations()`; queue `item_2_draw_0` (1.334 s; tied
   `{a = "bread"}`), `item_2_draw_1` (2.001 s), idle `item_2_aim_0` (3.335 s, loops). When `item_2_draw_0` ends,
   callback 29 marks the bread to be attached at the next update; random motions `item_2_prepare_0`,
   `item_2_attack_0` (3.335 s each) every 4 – 6 s with probability 79/100. Leaving: `item_2_holster_0` (tied
   `{d = "bread"}`), `item_2_holster_1` (1.334 s each).
6. **Playing the guitar** (`guitar`): queue `sit_1_guitar_0_0` (1.001 s; tied `{a = "guitar_a"}`),
   `sit_1_guitar_0_1` (2.335 s), idle `guitar_3` (2.001 s, loops); random `guitar_1`, `guitar_2` (7.671 s each). When
   the scheme moves to playing (level 1): `clear_animations()`, the level's music `guitar` is started (CH-25), idle
   `guitar_0` (5.336 s, loops), no random motions. Leaving the state: `guitar_0_sit_1_0` (2.001 s; tied
   `{d = "guitar_a"}`), `guitar_0_sit_1_1` (1.001 s).
7. **Steps** stop while sitting: none of these motions has a step-table line (section 9).

## 13. Verification facts

`python3 -I checks.py` (all pass, `0 failure(s)`):

- Name tables, list building and sharing [B1 – B5]; the update's order and modes [B6 – B11, B17]; slots, mask,
  notifications, time carry [B12 – B16]; global [B18 – B20]; head [B21]; torso [B22 – B28]; legs and speeds
  [B29 – B34]; `crouch_type` [B35]; the queue [B36 – B38]; steps [B39, B40].
- Data: configuration and step table [D1, D2]; models and lists [D3 – D6]; the example's motions [D7, D8, D10];
  constants [D9]. Scripts [K1 – K7]. Spawning [B41].
- Set-up gates and empty slots [B42, B43]; the weights of the speed hand-over [B44]; the order of the tracks update
  and the script slot [B45]; the other models' motion files and their lists [D11, D12] (`combos.py`); poses, level
  drops, reset and the random-motion memory [K8 – K11].

## 14. Unestablished and believed

- When the body state is 2 (1); the source of *v* and the yaw convention of ψ, left and right (1, 7.2); what plays
  for an element beyond a list's end (2.3); the further condition of the set-up and further conditions of the
  update's call (3.1, 3.2; believed once per frame); whether another tracks update of the model runs before the
  motion update in a frame (8.3).
- Who registers functions on the slots (3.5); the meaning of the weapon, missile and object-handler state numbers and
  of the reload kind (6.3 – 6.5, CH-20); the critical-wound value and weights and the case without an active item
  (4, CH-21); the sight's turning flag (7.3).
- Flag *a* believed unused, flag *b*'s motion controller not covered (8.2); the lists not read (2.2) believed unread.
- The stalker's own and ground materials and airborne state for steps (9).
- Whether the guitar music counts as a playing sound for the head (12; CH-25); the reset's call of the random-motion
  routine believed always to return at its first test, and no caller of a pose change while `busy` believed (11.2,
  11.3).
- Everything is static or read from the data and scripts: nothing was observed in the running game.

## 15. Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binary `bin\xrGame.dll`.
- Data examined: the winning copies of `meshes\actors\` (models and motion files), `config\creatures\m_stalker.ltx`,
  `config\creatures\actor.ltx`, `config\gameplay\character_desc_escape.xml`, `npc_profile.xml`, `scripts\` (`state_mgr*`,
  `state_lib`, `xr_kamp`, `xr_sound`, `lua_help` and every caller of `add_animation`), as extracted for earlier approved
  specifications.
- Analysis: static analysis of `bin\xrGame.dll` (the full headless Ghidra decompilation made for earlier approved
  specifications; read-only headless instruction listings and table reads of the stalker's motion code, its client
  update, the script routine of `add_animation`, the step manager's foot-bone reading); a model of section 2
  (`lists.py`) run on the novice's model and, with `combos.py`, on the motion-file sequences of all 182 stalker
  models. No debugging or execution; the game was not run.
- No leaked source code, derived projects or third-party documentation of the engine were used.

