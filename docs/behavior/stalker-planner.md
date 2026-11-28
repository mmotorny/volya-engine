# Stalker planner

Behaviour specification CH-17 of `docs/behavior/characters-scope.md` (cited as `scope`; its sections 1.5, 1.7, 2.4
and 5, with its "Gaps found in review", none of which concerns this part), for S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): the engine's decision planner of a stalker. It covers the stalker's root planner (its properties
with the engine's evaluators, its actions with their preconditions and effects, its two goals and how one is
chosen), the inner planners the root runs for death, for life without a fight (the offline task) and for anomalies,
the engine's own actions inside them and the gather-items action, how the root hands control to the actions and
evaluators the scripts add (the motivation manager of `xr_motivator`, `state_mgr` and the 19 stalker scheme
scripts), the update that drives it within the stalker's scheduled update, what of it a save holds, and the key
`need_osoznanie_mode`. It does not specify the general planner (how a planner builds and follows a plan), which is
`script-api-scope` W's, nor the inner planners of combat and danger, which are CH-18's and CH-19's.

Builds on (not repeated; cited by name and section):

- `script-api-scope` W, the action planner (cited as **W**): the planner, actions, evaluators, world properties and
  world states as scripts see them, `stalker_ids`, the motivation manager, the general planner update (evaluating
  the properties, finding a plan from the current state to the goal, switching and running its first action), an
  inner planner run as an action of its parent, script classes derived from the planner classes. Wherever this
  specification says "the general update", "a plan", "running an action" or "an inner planner", W's rules apply.
- `docs/behavior/script-api-object-binders.md` (`L`): the spawning order (5.1: the object's own reload, the binder's
  `reload`, the object's own reinitialisation, the binder's `reinit`, ..., the binder's `net_spawn` last), the
  scheduled update and the binder's `update(delta)` (5.2), saving and loading (5.3, 5.4).
- `docs/behavior/script-api-movement-sight-and-memory.md` (`X`): the movement targets of a stalker (3: target body
  state, movement type, mental state, path type, detail path type, desired position and direction) and their
  values (`move.crouch` 0, `move.standing` 1; `move.walk` 0, `move.run` 1, `move.stand` 2; `anim.danger` 0,
  `anim.free` 1, `anim.panic` 2; `move.line` 0), the chosen enemy, danger and item (8, 9).
- `docs/behavior/script-api-time.md` (`H`) 6: `time_global()`, the global timer in milliseconds; "the clock" below
  is this timer.
- `docs/behavior/talk-window.md` (`TW`) 9: the special mode of a talk partner read from `need_osoznanie_mode`.
- `docs/behavior/offline-simulation.md` 6.3 (switching online creates the stalker's in-game object).
- The parts of `scope` 2 whose systems this planner reads or drives, cited by part: health (CH-5), the level
  graph and paths (CH-8, CH-9), movement (CH-10), motions (CH-11), vision (CH-13), memory and the choices of enemy,
  danger and item (CH-15), combat with its group rating (CH-18), danger (CH-19), weapons and the item handler
  (CH-20), death (CH-21), script control (CH-24), sounds (CH-25), saved state (CH-27), smart-terrain jobs (CH-28).

Conventions:

- **Stalker**: an object of the class id `AI_STL_S` (`scope` conventions). **Root**: the stalker's root planner,
  what scripts get from the stalker as `motivation_action_manager()` (W). **Inner planner**: a planner that is
  also an action of the root (W).
- Properties and actions are named by their `stalker_ids` constants (`scripts\lua_help.script`) [D1]:

  | Property | Id | Property | Id | Action | Id | Action | Id |
  |---|---|---|---|---|---|---|---|
  | `property_alive` | 0 | `property_items` | 6 | `action_dead` | 0 | `get_out_of_anomaly` | 10 |
  | `property_dead` | 1 | `property_enemy` | 7 | `action_dying` | 1 | `detect_anomaly` | 11 |
  | `property_already_dead` | 2 | `property_danger` | 8 | `action_gather_items` | 2 | `action_death_planner` | 56 |
  | `property_alife` | 3 | `property_anomaly` | 42 | `action_no_alife` | 3 | `action_alife_planner` | 57 |
  | `property_puzzle_solved` | 4 | `property_inside_anomaly` | 43 | `action_smart_terrain_task` | 4 | `action_combat_planner` | 58 |
  | `property_smart_terrain_task` | 5 | `property_script` | 44 | `action_solve_zone_puzzle` | 5 | `action_anomaly_planner` | 59 |
  | | | | | | | `action_danger_planner` | 60 |
  | | | | | | | `action_script` | 61 |

  In the text the prefixes are dropped where unambiguous ("alive", "enemy", "the alife planner"). A precondition or
  effect "alive = true" is the world property (id, value) of W.
- **Stored set**: the set of (property id, value) pairs a planner keeps besides its evaluators (W's property
  storage); its actions may write it and evaluators may read it.
- Statements are **static** (from static analysis of the shipped `bin\xrGame.dll`, 6,246,400 bytes; the game was not
  run) or **measured** (from the game data and the shipped scripts' text) unless marked **believed** (with the
  reason) or **unestablished**. What a shipped script does is measured from its text; that it runs as read is
  believed wherever it depends on the script runtime and on specifications of other systems.
- **Checks**: every claim carries a label in brackets, an assertion of `checks/stalker-planner/checks.py` in the
  dirty workspace (`python3 -I checks.py`; `B`, `E`, `S`, `A`, `U`, `V`, `O` from the static analysis, `D`, `K`, `X`
  from the data and scripts). All 41 pass. Statements cited from an approved specification carry its checks.

## 1. The root

### 1.1 One root per stalker, built at the stalker's reload

Each stalker object is created with one root, which lives as long as the object; `motivation_action_manager()`
returns it, and on an object that is not a stalker reports the error `CAI_Stalker : cannot access class member
action_planner!` (W's rule for a member on the wrong kind of object) [B5b].

The root is **set up** as the first step of the stalker's own reload of its section, the step that `L` 5.1 runs
before the binding and before the binder's `reload` and `reinit` [B5]. Setting up, in this order [B3, B3b]:

1. binds the root to the stalker and resets its state (W's set-up: no current action, an empty stored set);
2. removes every action and every evaluator the root had (so a reload rebuilds it from nothing);
3. adds the seven evaluators of 1.2, then the six actions of 1.3;
4. defines the two goals of 1.4.

Since the binder's `reinit` and `net_spawn` come later in the same spawning (`L` 5.1), everything the scripts add
(section 4) is added to the freshly built root.

### 1.2 Properties and evaluators

The root evaluates seven properties, added in this order [B1, B1b] (W keeps them in id order, [S4]):

| Property | True when |
|---|---|
| `already_dead` (2) | never: a constant false |
| `puzzle_solved` (4) | never: a constant false |
| `alive` (0) | the stalker's health (CH-5) is above 0 [B4] |
| `enemy` (7) | the memory's enemy choice (CH-15, `X` 8.1 `best_enemy`) has an enemy, or the clock is less than 3,000 ms past the last update of that choice that had one [E1] |
| `danger` (8) | the danger choice (CH-15, `X` 8.2 `best_danger`) has a danger [E2] |
| `anomaly` (42) | see below [E3] |
| `items` (6) | the item choice (CH-15, `X` 9) has an item [E2] |

The enemy rule: each time the enemy choice is updated and has an enemy, it records the clock; the property stays
true for 3,000 ms after the last such update, so a stalker that loses its enemy keeps "enemy" for up to 3 s [E1].
The recorded time starts at 0 when the stalker's memory is made (**believed** of no effect at a New Game, when the
clock is already well past 3,000 ms). **Disputed in review:** no check establishes the initial value of the recorded
time; treat it as unestablished.

The anomaly rule [E3]: the stalker counts as in an anomaly when it currently touches a zone object that is not a
radiation zone (its touch sense, updated in its scheduled update after the think step, 5.1; **believed** the class
id `Z_RADIO` is the radiation zone, by its name), or when the root's stored set has `anomaly` = true (written by the
anomaly planner, 2.3). In an anomaly, the property is true when the stalker has no chosen enemy; with an enemy, only
when the group rating of CH-18 for the stalker is 0 (a rating the engine keeps per group, renewed at most every
2,000 ms and computed over the group's living members within 300 m; what 0 means is CH-18's, **unestablished**
here). Not in an anomaly, it is false. The shipped scripts replace this evaluator by a constant false on every
script-bound stalker (4.2), so it decides nothing in the shipped game (**believed**: every stalker section binds the
motivator, `scope` 1.2, and the simulation always exists in single-player play).

### 1.3 Actions

The root has six actions, added in this order [B2, B2b] (W keeps them in id order):

| Action | Preconditions | Effect | What it is |
|---|---|---|---|
| `action_death_planner` (56) | alive = false, already_dead = false | already_dead = true | inner planner, 2.1 |
| `action_alife_planner` (57) | alive = true, enemy = false, anomaly = false, danger = false, items = false, puzzle_solved = false | puzzle_solved = true | inner planner, 2.2 |
| `action_combat_planner` (58) | alive = true, anomaly = false, enemy = true | enemy = false | inner planner, CH-18 |
| `action_danger_planner` (60) | alive = true, enemy = false, anomaly = false, danger = true | danger = false | inner planner, CH-19 |
| `action_anomaly_planner` (59) | alive = true, anomaly = true | anomaly = false | inner planner, 2.3 |
| `action_gather_items` (2) | alive = true, enemy = false, anomaly = false, danger = false, items = true | items = false | action, 3.3 |

Each part added to a planner is set up with the stalker and with the adding planner's stored set, and adding an
action makes the current plan invalid (W's rule) [S4].

### 1.4 Goals and the goal choice

The root has two goals [B3]: the **life goal** {puzzle_solved = true} and the **death goal** {already_dead = true}.

Each root update first chooses the goal: the death goal when the stalker's health is at most 0, else the life goal;
the plan stays valid only if the chosen goal is the one of the previous update (a change of goal makes the general
update plan anew). Then it runs the general update (W) [B4, B4b].

Since `puzzle_solved` is always false at the root, the life goal is reached only through the alife planner, the one
action whose effect is puzzle_solved = true. So a living stalker always plans towards the alife planner: when its
preconditions hold, it runs; when enemy, danger, items or anomaly is true, the plan starts with the combat, danger,
gather-items or anomaly action, whose effects restore them; and every precondition the scripts add to the alife
planner (4.3) puts the script actions that produce it in front of it. A dead stalker plans towards the death
planner, the one action whose effect is already_dead = true; since `already_dead` is also always false, the death
planner, once chosen, stays the running action.

## 2. The inner planners

### 2.1 Death (`action_death_planner`)

When the root adds it, it is set up (as every inner planner here: it keeps the root's stored set, takes its own
effects as its goal, then builds its parts [S6]); building stores dead = false in its own stored set, removes its
parts, then adds [S1, S6]:

- evaluators: `already_dead` (2), a constant false; `dead` (1), true when its own stored set has dead = true [E5];
- actions: `action_dying` (1), precondition dead = false, effect dead = true; `action_dead` (0), precondition
  dead = true, effect already_dead = true.

Its goal is its effect, already_dead = true [S6]. So it first runs
`action_dying`, which ends by storing dead = true (3.2), and from the next update on `action_dead`, which never
reaches its effect (the evaluator is constant) and so runs for as long as the object exists.

### 2.2 Life without a fight (`action_alife_planner`)

Building it removes its parts, then adds [S2]:

- evaluators: `puzzle_solved` (4), a constant false; `smart_terrain_task` (5), true when the simulation exists and
  the stalker's simulation object has a task assigned (its field for that task is not the "none" value; **believed**
  the job of a smart terrain, CH-28) [E4]; `alife` (3), true when the simulation exists [E4];
- actions [S2]:

  | Action | Preconditions | Effect |
  |---|---|---|
  | `action_no_alife` (3) | alife = false, puzzle_solved = false | puzzle_solved = true |
  | `action_smart_terrain_task` (4) | alife = true, smart_terrain_task = true | smart_terrain_task = false |
  | `action_solve_zone_puzzle` (5) | alife = true, smart_terrain_task = false, puzzle_solved = false | puzzle_solved = true |

Its goal is its effect, puzzle_solved = true [S6]. With the simulation and a task, the plan is the smart-terrain task then the zone
puzzle, and the task action runs; without a task, the zone puzzle runs; without the simulation, `action_no_alife`.
None of them reaches the goal (the evaluator is constant), so the running one stays until the alife planner itself
stops being the root's choice.

### 2.3 Anomalies (`action_anomaly_planner`)

Building it stores anomaly = false in its own stored set and in the root's, removes its parts, then adds [S3]:

- evaluators: `inside_anomaly` (43), true when the stalker touches a zone that is not a radiation zone, with the
  same enemy and group-rating condition as the anomaly rule of 1.2 (no stored-set alternative) [E4]; `anomaly` (42),
  the same rule as the root's (1.2), reading this planner's stored set [S3, E3]. **Disputed in review:** the anomaly
  evaluator here reads the root's stored set, not this planner's: it is the same evaluator as the root's and always
  looks up `anomaly` in the stalker's root;
- actions: `get_out_of_anomaly` (10), precondition inside_anomaly = true, effect inside_anomaly = false;
  `detect_anomaly` (11), preconditions inside_anomaly = false, anomaly = true, effect anomaly = false.

Each update of this planner runs its general update, then copies its stored `anomaly` value into the root's stored
set [S3b], so the root's `anomaly` stays true while this planner's actions keep it true (3.6).

### 2.4 Combat and danger

`action_combat_planner` and `action_danger_planner` are inner planners whose contents are CH-18's and CH-19's; this
specification gives only their place at the root (1.3) [B2b].

## 3. The engine's actions

The movement requests below are the targets of `X` 3 (written as the engine sets them; the movement of CH-10 carries
them out); "desired position" is `X`'s desired position with its flag.

### 3.1 What every engine action does

Starting an engine action records the clock as its start time and clears a flag of the root that other actions set
(CH-18's) [A1]. A plain action (`action_dead`) does nothing while it runs; finishing an engine action resets the
stalker's motion manager (CH-11; **believed**: its script animation request) [A1]. An action writes the stored set
of the planner that added it [S5].

### 3.2 `action_dying` and `action_dead`

`action_dying` [A2]: it drops the items in the stalker's inventory slots, never a bolt. A condition on the item in
the active slot and on a time less than 501 ms before now decides when (**unestablished** what the condition is;
**believed** "it was firing when it died"): when it holds, every slot item but the active one is dropped when the
action starts, after a further request to the inventory (**unestablished**); when it does not, every slot item but
the active one is dropped at its first run and the active one is marked instead (**believed**: to fall with the
body, CH-21). Then, at each run, it stores dead = true in the death planner's set [A2, S5]. **Disputed in review:** dead = true
is stored only at a run where the condition does not hold; while it holds, a run drops nothing and stores nothing,
and the drop-and-mark pass (every slot item but the active one dropped, the active one marked) and the storing of
dead = true happen at the first run where it no longer holds. What dropping does is
CH-6's and CH-21's.

`action_dead` is a plain action (3.1) [A1, S1].

### 3.3 `action_gather_items`

Starting [A3]: no desired direction; path type level path (1), detail path line (0), body standing (1), movement
walk (0), mental state danger (0); the stalker's current sounds are stopped (**believed**, from the sound mask it
sets and restores, CH-25); the item handler is asked to hold the best weapon idle (**believed**, CH-20's request).
Each run: with a chosen item, the target level vertex is the item's and the desired position is the item's position;
the sight is updated (CH-13 / `X`). Finishing: the desired position is dropped and, if alive, the sounds are reset
(**believed**).

### 3.4 `action_no_alife`, `action_smart_terrain_task`, `action_solve_zone_puzzle`

All three start with no desired direction, detail path line (0), body standing (1), movement walk (0) and mental
state free (1); `action_no_alife` also drops the desired position [A4]. `action_no_alife` and
`action_smart_terrain_task` play, at each run, the humming sound kind (`sound_humming`, 3) with the two intervals
10,000 and 60,000 ms (**believed**: the shortest and the longest pause; the sound player is CH-25's);
`action_solve_zone_puzzle` does not [A4].

- `action_no_alife` and `action_solve_zone_puzzle`: path type game path (0) [A4]. Starting, each records a time:
  the start, or 30,000 ms plus a random part of up to 30,000 ms after it when the weapon in the active slot is the
  one the item handler holds as best; each run renews the choice of the best weapon and asks the item handler to
  hold it idle (**believed**, CH-6 and CH-20's requests); what the recorded time changes between the two is
  **unestablished** [A4].
- `action_smart_terrain_task`: on each run, when the stalker's game-graph vertex is the task's, path type level path
  (1) to the task's level vertex and position (with the desired position set once it is close), else path type
  game path (0) to the task's game vertex (**Disputed in review:** "once it is close" is unbacked: on the level-path
  branch the desired position is set at every run, either to the task's position directly or to a position the
  movement finds from the task's position and level vertex; what decides between the two is unestablished); from 30,000 ms after its start it renews the item handler's request each
  run (**believed**, CH-20). The task's place is CH-28's [A4].

### 3.5 `get_out_of_anomaly`

Starting: the stalker's sounds are stopped (**believed**); path type level path (1), detail line (0), standing (1),
walk (0), mental state danger (0); the item handler's request depends on whether it has an enemy and holds its best
weapon (**believed**, CH-20); then it stores anomaly = true in the anomaly planner's set [A5, S5]. Each run: the same movement
targets, and the stalker moves away from the zones it touches (it collects them and requests a position clear of
them, CH-9 / CH-10) [A5].

### 3.6 `detect_anomaly`

Starting: sounds stopped (**believed**); a duration of 15,000 ms plus a random part of up to 5,000 ms; a further
request (**unestablished**). Each run: while the duration has not passed and the stalker has no chosen enemy, the
item handler's idle request (**believed**); otherwise it stores anomaly = false in the anomaly planner's set [S5] (so the root's
`anomaly` becomes false at that planner's next update, 2.3) [A5]. **Disputed in review:** the copy into the root's set
comes at the end of the same anomaly-planner update in which the action ran, not at the next one.

## 4. Handing control to the scripts

### 4.1 What the scripts get

`bind_stalker.init` (the binding of every stalker section, `scope` 1.2) attaches the motivator binder of
`xr_motivator`, and only when the simulation exists (`alife()`) [K1]. Through `motivation_action_manager()` the
scripts reach the root with W's members: adding and removing evaluators and actions, fetching an action by id to add
preconditions, `initialized()` and `current_action_id()`. The scripts' own ids start at `property_script` (44) for
properties and `action_script` (61) for actions (`xr_evaluators_id`, `xr_actions_id`) [K4].

### 4.2 When they act

In the spawning order of `L` 5.1 [B5, K1, K5]:

1. The stalker's own reload: the root is built (1.1).
2. The binder's `reload`: nothing on the root.
3. The binder's `reinit`: `state_mgr.bind_manager` adds two evaluators (`property_script` + 101 "idle in combat",
   + 102 "idle in life") and two actions (`action_script` + 101, + 102, each turning the body state machine to idle),
   and adds preconditions to five engine actions: the alife planner and gather items need "idle in life" = true; the
   combat, anomaly and danger planners need "idle in combat" = true [K5]. Its own body-state planner (built by
   `state_mgr_goap`) is a separate planner, not part of the root; the binder's `update` runs it (5.1) [K5].
4. The binder's `net_spawn`, for a living stalker: the root's `anomaly` evaluator is removed and replaced by a
   constant false [K1]; then the logic set-up of `xr_logic` (through `xr_gulag`) enables the stalker's schemes, each
   of which adds its evaluators and actions and preconditions on the engine's actions (4.3).

The "idle in life" evaluator is true whenever the stalker has an active scheme section [K5].

### 4.3 How a script action gets control

The root's goals are fixed (1.4), so a script action runs only when the plan towards the goal starts with it. The
shipped scripts arrange this by adding, to the engine's actions, preconditions on their own properties, and giving
their actions the effects that satisfy them [K3, K6]. The stalker scheme scripts are the 19 that `modules.script`
loads for stalkers (20 schemes; `xr_meet` twice) [K2]. Shipped scripts that fetch an engine action and add a
precondition to it [K3]: the alife planner 26 scripts (`state_mgr` and 25 schemes and helpers), the combat planner
6, the danger planner 3 (`state_mgr`, `xr_heli_hunter`, `xr_wounded`), the anomaly planner 2 (`state_mgr`,
`xr_wounded`), gather items 2 (`state_mgr`, `xr_wounded`). For example the campfire scheme `xr_kamp` adds
"kamp_end" (`property_script` + 1021, true when the scheme is no longer active) and "on_position" (+ 1022, true on
the scheme's level vertex), the actions "go to position" (`action_script` + 141; preconditions alive, no danger, no
enemy, no anomaly, the common ones of `xr_motivator`, on_position = false; effect on_position = true) and "wait"
(+ 143; the same with on_position = true; effect kamp_end = true), and the precondition kamp_end = true on the
alife planner [K6]. While the scheme is active, the life goal can be reached only through "wait", so "wait" (after
"go to position" when needed) is the running action; once it ends, kamp_end becomes true and the alife planner runs.

The engine's own preconditions stay in force: whatever the scripts add, the combat, danger and gather-items actions
come first when enemy, danger or items is true, the death planner when the stalker dies (1.3).
**Disputed in review:** not whatever the scripts add: shipped scripts add root actions whose effect is enemy = false (the camper
scheme's patrol, the wounded action, the zombied, camper and monolith combat types) or danger = false (the zombied
combat type), and preconditions on the combat planner (for example the camper's close-combat property, the combat
checker's property), so with enemy or danger true such a script action can lead the plan instead of the engine's
planner; the same limits the "plan starts with the combat, danger ... action" statement of 1.4 to the engine's own
parts.

## 5. The update

### 5.1 The stalker's scheduled update

Each scheduled update of a stalker (with the interval since its previous one, `L` 5.2) does, in this order [U1, U3,
U4]:

1. When alive: its sensing and memory work, in this order: critical-wound handling (CH-21), the group's periodic
   update (CH-18), vision (CH-13, alternating parts on alternate updates), taking an enemy or danger from a group
   member when it has none (CH-15), and the memory update with the interval in seconds (CH-15) (**believed**
   attributions, from what each part reads; their content is the cited parts').
2. The common update of a creature, which begins with the object's general scheduled update and with it the binder's
   `update(delta)` (`L` 5.2): so `xr_motivator`'s update (with `state_mgr`'s planner, the sounds) runs before the
   think step of the same update.
3. When the object is controlled locally (a spawn flag; **believed** always so in single-player play): the **think
   step** (5.2), or, when scripts have taken the stalker over (CH-24), the scripted actions instead; the root is then
   not updated.
4. When alive: its touch sense (whose touched objects 1.2 and 2.3 read at the next think step).
5. Further work (network state, the inventory's update; not this specification's).

**Disputed in review:** steps 4 and the network state of 5 run only in the locally controlled case of step 3 (they
are skipped with it); the inventory's update runs in every case.

How often the scheduled update runs is CH-26's.

### 5.2 The think step

The think step updates the root (1.4: the goal choice, then the general update, which runs the current action and,
inside an inner planner, that planner's update), then, when the stalker is alive, its movement (CH-10) [U2]. It
runs while the stalker is dead too, so the death planner runs (2.1).

## 6. What a save holds

A stalker's saved state ends with the root's [V1]. A planner writes, in this order: each evaluator's own state (in id
order), each action's own state (in id order; an inner planner writes its planner state in the same way), then its
stored set: a u32 count, then for each pair a u32 property id and a u8 value. Loading reads the same and marks the
planner as loaded (W's meaning) [V1]. The engine's evaluators and plain actions write nothing of their own; what a
script evaluator or action writes is W's [V1]. Where the stalker's block sits in the save is CH-27's.

## 7. `need_osoznanie_mode`

The key is read from the object's section with its other inventory-owner keys; absent, it is off [O1]. Its effect
on the talk window is `TW` 9. None of the routines of this specification (the root's set-up, evaluators, actions,
update, the think step) reads it [O1]; whether anything else does is **unestablished**.

## 8. Worked example: a novice at the beginners' village campfire

On a New Game, the record `esc_lager1` of `spawns\all.spawn` is a stalker of the section `[stalker]` with the
profile `esc_stalker_novice` on `l01_escape`, whose custom data names the smart terrain `esc_lager`; among
`esc_lager`'s jobs are three of `logic@esc_lager_kamp1`, whose active section `kamp1@esc_lager` is the campfire
scheme with `center_point = camp_center` [X1]. Which novice takes which job is CH-28's; take one that gets the
campfire job.

1. Coming online, its reload builds the root: seven evaluators, six actions, two goals (1.1–1.4).
2. Its binder's `reinit` adds `state_mgr`'s ids 145, 146 and 162, 163 and their preconditions (4.2).
3. Its `net_spawn` makes `anomaly` constant false, and the logic set-up enables the campfire scheme and the generic
   ones (`xr_wounded`, `xr_abuse`, `xr_meet`, `xr_heli_hunter` and others, `xr_logic`), adding among others
   kamp_end 1065, on_position 1066, "go to position" 202 and "wait" 204, and the precondition kamp_end = true on the
   alife planner [K6].
4. At its first think step: health is above 0, so the life goal; no enemy (and none in the last 3 s), no danger,
   no item chosen (**believed** for this place), anomaly false; the scheme is active, so kamp_end is false and "idle
   in life" is true. The plan is "go to position" (if not on its vertex), "wait", then the alife planner; the first
   runs: the stalker walks to its place at the fire. On its vertex, "wait" runs and keeps running while the scheme is
   active.
5. If an enemy is chosen, enemy becomes true and the combat planner leads the plan (CH-18); 3 s after the last
   enemy is gone, the plan returns to the campfire actions. If it dies, the death goal is chosen and the death
   planner runs `action_dying`, then `action_dead` (2.1).

## 9. Unestablished and believed

- What the condition of `action_dying` is and its further inventory request; the further requests of
  `detect_anomaly` and of `action_no_alife` / `action_solve_zone_puzzle` (3.2, 3.4, 3.6).
- What group rating 0 means for the anomaly rule (1.2; CH-18).
- The sound and item-handler requests of the actions are believed from what they set (3.3–3.6; CH-20, CH-25).
- That `Z_RADIO` is the radiation zone (1.2), that the task field of 2.2 is a smart terrain's job (CH-28), that the
  local-control flag is always set in single-player play (5.1).
- Whether anything reads `need_osoznanie_mode` besides the talk window (7).
- Everything here is static or read from the scripts: nothing was observed in the running game.

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binary `bin\xrGame.dll`
  (6,246,400 bytes).
- Data examined: the winning copies of `config\`, `scripts\` and `spawns\all.spawn` as extracted for the approved
  `pda-scope`: `lua_help.script` (`stalker_ids`), `bind_stalker`, `xr_motivator`, `state_mgr`, `state_mgr_goap`,
  `xr_actions_id`, `xr_evaluators_id`, `modules`, `xr_logic`, `xr_kamp`, `xr_meet`, `xr_wounded`, `xr_abuse`,
  `xr_heli_hunter`, `gulag_escape` and every script fetching the root's actions; `config\misc\gulag_escape.ltx`; the
  record `esc_lager1`.
- Analysis: static analysis of `bin\xrGame.dll` (the full headless Ghidra decompilation made for earlier approved
  specifications, read-only headless instruction listings of the planner-building routines, run-time type
  information, initial values read from the file): the root's construction, set-up, evaluators, actions, goals and
  update; the three inner planners and their actions; the evaluators' routines; the stalker's reload, scheduled
  update, think step, save and load; the script member returning the root; the reading of `need_osoznanie_mode`.
  No debugging or execution; the game was not run.
- Checks: `checks/stalker-planner/checks.py` (with the `characters-scope` helpers), run with `python3 -I`; 41 pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The generic danger scheme (`xr_danger`), enabled for every script-bound stalker at the logic set-up, removes the
  root's `danger` evaluator and adds its own script evaluator under the same id; what the root then reads as danger
  is not specified (4.2 lists no replacement besides `anomaly`).
- Script actions added to the root with effects on the engine's properties (enemy = false, danger = false) and the
  script preconditions on the combat planner: which scripts, under which conditions, and how they share the enemy
  case with the combat planner.
- Starting an engine action also resets the same motion request that finishing resets (3.1 gives it only for
  finishing).
- The start of `action_no_alife`, `action_solve_zone_puzzle`, `action_smart_terrain_task` and `get_out_of_anomaly`
  also updates the stalker's sight (3.3 gives a sight update only for gather items).
- Each run of `action_dying` also clears a movement value of the stalker, first recording the stalker's position
  into it when it was set; its meaning is not specified.
