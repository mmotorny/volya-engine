# Smart-terrain jobs

Behaviour specification CH-28 of `docs/behavior/characters-scope.md` (cited as `scope`; its sections 2.6, 5 and 8, with
its "Gaps found in review", none of which concerns CH-28), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition):
the jobs of a smart terrain. It covers the data that define jobs (`config\misc\gulag_*.ltx`, `config\misc\smart_terrain*.ltx`,
`[smart_terrain_rank_change]`, the job tables of the `gulag_*` scripts), how a member of a smart terrain is given a job,
begins it and leaves it, and the engine's part in all of this: when the engine runs the smart terrain's scripted
update, which engine calls the job scripts depend on, and what an online stalker or monster does on its own to reach
its job. The worked example is the smart terrain `esc_lager` of `l01_escape` at a New Game and its campfire job.

The jobs are defined and run by the shipped scripts `smart_terrain`, `xr_gulag`, `gulag_tasks` and the 16 level
scripts that `xr_gulag` lists, on the runtime of `script-environment`. What this specification states about those
scripts is a static reading of them, given so that the engine side can be built and checked; the engine's part is
what the engine must do for them (section 3) and on its own (section 5).

Builds on (not repeated; cited by name and section):

- `docs/behavior/offline-simulation.md` (`OS`): 6.5.4 (the job-online value), 7.1 (scheduling), 7.2 (the smart-terrain
  choice), 7.3 (the travel target), 7.5 (movement).
- `docs/behavior/script-api-simulation-objects.md` (`J`): the simulation-object members the scripts call (3.5 creatures,
  3.6 monsters, 3.7 characters, 3.8 smart zones), dispatching (4), scheduled updates (5.7), the smart-terrain calls
  (5.9), the shipped script classes (6).
- `docs/behavior/script-api-simulation-registry-and-game-graph.md` (`K`): `alife()`, `game_graph()`, tasks (11),
  brains (12).
- `docs/behavior/script-api-game-object-basics.md` (`M`): game objects and their kinds.
- `docs/behavior/script-api-object-binders.md` (`OB`): binding (3), `update` (the table of 5), `net_Relcase` (5.5).
- `docs/behavior/script-api-movement-sight-and-memory.md` (`X`): the movement request (4), restrictions and
  accessibility (5), sight (6.1), `set_item` (10.1), `weapon_strapped` (10.2), script animations (10.3).
- `docs/behavior/script-api-relations-community-and-ranking.md` (`Q`): `change_team` (3).
- `docs/behavior/script-api-sound.md` (`T`): `play_sound` (6.3).
- `docs/behavior/script-api-level.md` (`N`): `patrol`, `level.object_by_id`, `level.patrol_path_exists`,
  `level.vertex_position`.
- `docs/behavior/script-api-configuration-and-files.md`: `ini_file`, `create_ini_file`, `system_ini`.
- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md`: the runtime and its libraries
  (`math.random`, `table.sort`).
- `docs/formats/ltx-config.md`, `docs/formats/sav.md` (7.4: the Object, Creature and Monster-like blocks).

Conventions:

- **Static** facts come from static analysis of the shipped `bin\xrGame.dll` or from reading the shipped data and
  scripts; nothing was run. **Believed** marks a reading with its reason; **unestablished** what could not be settled.
- A *smart terrain* is an object of class id `SMRTTRRN`; its *gulag* is the job state its script keeps (the scripts'
  word). A *member* is a stalker or monster that has joined it (`OS` 7.2), so that its smart-terrain id (Monster-like
  block) is the smart terrain's id. *Game time* is the simulation's game clock in milliseconds (`game.time()`) or as a
  time value (`game.get_game_time()`).
- Every fact carries its check, an assertion of `checks/smart-terrain-jobs/checks.py` in the dirty workspace, cited by
  label in brackets (e.g. [SJ-1]).

## 1. The split with OS-5

`narrowed-specs-scope` OS-5 (`smart-terrains.md`) covers how a creature comes to belong to a smart terrain: the
acceptance, the suitability and the joining asked by the choice of `OS` 7.2 (`enabled`, `suitable`, `register_npc`
as decisions: agreement, exclusive smart terrains, communities, presets, the level groups, whether any job suits),
the task the smart terrain gives for offline travel (`OS` 7.3), the job-online value and what switching does with it
(`OS` 6.5.4), and which creatures of `l01_escape` join which smart terrain at a New Game and are online in the first
frame.

This specification (CH-28) covers everything a member's job is: the job set and the gulag's state, the assignment
of members to jobs, beginning and leaving a job and what each does (including setting the job-online value that
OS-5 consumes and the rank change made on joining), the cadence of the gulag's update, and the engine's online
behaviour toward the job. Where `register_npc` and `task` act on jobs, the job part is here and the joining part is
OS-5's.

## 2. Data

### 2.1 Smart-terrain records

`spawns\all.spawn` holds 282 smart-terrain records, 41 of them on `l01_escape` [SJ-1]. Each has a `[smart_terrain]`
or `[gulag1]` section in its custom data (the script reads `[smart_terrain]`, else `[gulag1]`) [SJ-1b]. The keys
that concern jobs (the others are OS-5's):

| Key | Use |
|---|---|
| `type` | the gulag type, which selects the job set (2.3), the state function (4.2) and the generated configuration (2.2) |
| `capacity` | the most members; default 100; the real capacity is the smaller of this and the number of jobs that exist in state 0 [SJ-6d, SJ-6e] |
| `squad`, `groups` | passed to the level scripts' job definitions, which use them to fill jobs' `squad` and `group` |
| `switch_0`, `switch_1` | condition lists read when the gulag is made ready (4.2) |
| `cond` | while it does not hold, the gulag is removed (4.7) |
| `respawn` | the respawner called the first time each member begins a job (4.5) |
| `idle`, `duration` | read and turned into two times that are saved but never compared (4.8) |

[SJ-8]

### 2.2 `config\misc\gulag_*.ltx`

`config\misc` holds 27 `gulag_*.ltx` files (and a file `gulag_radar_u.script`, which is outside `scripts\` and so not a
script) [SJ-2]. `gulag_tasks.ltx` includes 15 of them, beginning with `gulag_general.ltx` and `gulag_escape.ltx`
[SJ-2b]; `gulag_military.ltx` includes the ten `gulag_mil_*.ltx`; `gulag_selo.ltx` is included by nothing, and the
script `gulag_selo` is named by nothing [SJ-2c].

These files hold the jobs' logic sections. A job names a section (`logic@…`); that section's key `active` names the
section of the scheme the member runs at the job, and that scheme section names the job's place by the first of the
keys `path_walk`, `path_main`, `path_home`, `center_point` it has. The job's **path** is the smart terrain's name, `_`,
and that key's value; for `center_point` the path `<that>_task` is taken instead when a patrol path of that name
exists [SJ-2d, SJ-6d]. A job section without `active`, or an `active` section with none of the four keys, is a
fatal error (`gulag: ltx=%s  no 'active' in section %s`, `gulag: ltx=%s, there is no path in section %s`) [SJ-8].

The sections are read from `misc\gulag_tasks.ltx` (opened with `ini_file`), unless the gulag type's level script
generates a configuration text for the smart terrain: that text is made into a configuration with `create_ini_file`,
kept under the name `*<smart terrain>*<type>`, and used instead [SJ-2d, SJ-2e]. Seven level scripts can generate one
(`gulag_agroprom`, `gulag_agroprom_underground`, `gulag_dark_valley`, `gulag_deadcity`, `gulag_general`,
`gulag_pripyat`, `gulag_radar`) [SJ-2f].

### 2.3 Job tables of the gulag scripts

`xr_gulag` asks every level script it lists for the jobs of a gulag type; each adds a table per job [SJ-6]. The 1,374
job tables of the shipped `gulag_*` scripts all have `section`, `idle`, `prior`, `state`, `in_rest` and `out_rest`;
optional fields occur as follows [SJ-6, SJ-6b]:

| Field | Tables | Meaning (as `xr_gulag` uses it) |
|---|---|---|
| `section` | 1,374 | the job's logic section (2.2) |
| `prior` | 1,374 | priority: greater is filled first |
| `state` | 1,374 | the gulag states in which the job exists |
| `idle` | 1,374 | after a member began it, the job is not available to another for this long (ms of game time) |
| `in_rest`, `out_rest` | 1,374 | in- and out-restrictions given to the member at the job (names; `""` none) |
| `group`, `squad` | 669, 656 | group and squad given to the member (4.4) |
| `online` | 324 | the job-online value: `true`, `false` or a condition list (`OS` 6.5.4) |
| `position_threshold` | 321 | arrival distance (default 120), or `"graph"` (4.5) |
| `predicate` | 303 | a function of the member's information deciding whether the job suits it |
| `timeout` | 33 | after a member began it, the job ends for it after this long (ms of game time) |
| `idle_after_death` | 10 | after its member's death the job is unavailable this long (s; default 600) |
| `fill_idle` | 2 | the job may be filled once per state (4.3) **Disputed in review:** in effect never applied; see 4.3 |
| `dependent` | 8 | never read [SJ-6c] |
| `redicate` | 1 | a misspelt `predicate`, never read: that job suits every member [SJ-6b] |

`info_rest` is read but no job defines it [SJ-6c].

### 2.4 `config\misc\smart_terrain*.ltx`

- `smart_terrain.ltx`: the configuration section `[smart_terrain]` (class id `SMRTTRRN`, `script_binding =
  bind_smart_terrain.bind`), and `[smart_terrain_stay_time]`: `quick = 2, 5`, `medium = 6, 8`, `long = 12, 14`,
  `default = 10000, 10001` (hours) [SJ-3]. The custom-data key `stay` names one of these (default `default`); a
  member's stay end is set from it on joining but never compared (4.8).
- `smart_terrain_presets.ltx`: 18 preset sections of community ranks, used by OS-5's acceptance [SJ-3b].

### 2.5 `[smart_terrain_rank_change]`

In `config\creatures\game_relations.ltx`: `min = 1`, `max = 30` [SJ-4]. When a **stalker** joins a smart terrain, its
rank becomes its rank (`rank()`, J 3.7) plus `math.random(min, max)` (the runtime's generator, an integer from 1 to 30
inclusive), set with `set_rank` [SJ-4b]. Monsters' ranks do not change.

### 2.6 Who reads these

None of `smart_terrain_rank_change`, `smart_terrain_stay_time`, `smart_terrain_presets`, `gulag_tasks` or `gulag` is
named in `xrGame.dll` [SJ-5]: the engine reads none of this data; the scripts read it through `ini_file`,
`system_ini` and `create_ini_file`.

## 3. The engine's part: entry points

### 3.1 The smart terrain's update

The scripted update of a smart terrain (`se_smart_terrain:update`, section 4.7) runs from two places:

- **offline**: as the smart zone's scheduled update (J 5.7: a smart zone is in the schedule while it has direct
  control, uses AI locations and is offline; `OS` 7.1's limit of `objects_per_update` applies);
- **online**: from the binder's `update` (OB 5): `bind_smart_terrain.bind` binds a binder to every `SMRTTRRN` object
  whose custom data has `[smart_terrain]` or `[gulag1]`, provided a simulation exists; its `update(delta)` calls the
  simulation object's `update()` [SJ-3, SJ-7c].

The binder's `net_Relcase(handle)` (OB 5.5) marks a member being destroyed as offline in the gulag [SJ-7c]. A smart
zone never switches offline once online (`OS` 6.5.2), so on the current level its update is the binder's.

### 3.2 The task

The engine asks a smart terrain for a member's task with the dispatched `task(obj)` (J 3.8, 5.9): offline at the
movement step (`OS` 7.3, K 12.1), and online from five places: the stalker's smart-terrain action (5.1, at each
execution) and the monster's smart-terrain task state (5.2: when it starts, at each execution, and in its game-graph
walk when that starts and when its start condition is tested) [SJ-10, SJ-10b]. The script's `task` gives a task on
point 0 of the member's job path (K 11); when the member has **no job**, it releases the member from the smart
terrain (4.6) and gives the task of `simulation_default_path` [SJ-7d]. So asking for the task of a member without a
job removes it from the smart terrain.

### 3.3 Hooks in the creatures' scripts

- Coming online: the stalker binder (`xr_motivator`) and the monster binder (`bind_monster`) run
  `xr_gulag.setup_gulag_and_logic_on_spawn` from `net_spawn` [SJ-7] (4.5).
- Death of an online creature: their death callbacks run `smart_terrain.on_death(id)` [SJ-7] (4.6). A creature
  killed offline (K's `kill_entity`, which calls no death callback, J 5.8) keeps its job.
- Unregistration: `se_stalker` and `se_monster` call `smart_terrain.unregister_npc` from `on_unregister` (J 5.2)
  [SJ-7b].

### 3.4 Engine members the job scripts call

The job scripts (`smart_terrain`, `xr_gulag`, `bind_smart_terrain`, `gulag_tasks`, `smart_terrain_params`) reach the
engine only through members already specified:

| Member | Specified in |
|---|---|
| simulation objects: `id`, `name()`, `section_name()`, `clsid()`, `m_game_vertex_id`, `position`, `m_story_id`, `squad`, `group` (written), `g_team()`, `g_squad()`, `g_group()`, `alive()`, `rank()`, `set_rank()`, `community()`, `profile_name()`, `smart_terrain_id()`, `clear_smart_terrain()`, `smart_terrain_task_activate()`, `smart_terrain_task_deactivate()`, `spawn_ini()` | J 3 |
| `alife():object()`, `alife():story_object()`, `alife():level_name()`, `game_graph():vertex()` with `level_id()`, `level_vertex_id()`, `CALifeSmartTerrainTask(path)` | K |
| `patrol(name)` with `game_vertex_id(i)`, `point(i)`; `level.object_by_id`, `level.patrol_path_exists`, `level.name`, `level.get_time_hours` | N |
| game objects: `id()`, `name()`, `alive()`, `position()`, `game_vertex_id()`, `squad()`, `group()`, `change_team()`, `add_restrictions()`, `remove_restrictions()`, `relation()`, `set_relation()`, `inside()` | M, Q, X |
| `game.time()`, `game.get_game_time()`, `game.CTime` and its `setHMSms`, `add`, comparison | `docs/behavior/script-api-time.md` |
| `ini_file`, `create_ini_file`, `system_ini` and the configuration readers | `docs/behavior/script-api-configuration-and-files.md` |
| `object_binder` and `bind_object` | OB |

[SJ-9]

## 4. Jobs (shipped scripts)

### 4.1 The job set

When the smart terrain registers (J 5.2 `on_register`), its gulag loads its jobs: the job tables of its type (2.3),
sorted by `prior` from greatest to smallest with `table.sort` (not a stable sort: the order of equal priorities is the
runtime's) [SJ-6d]. For each job, its path (2.2), the game vertex and position of point 0 of that path, and that
vertex's level are recorded [SJ-6d]. **Disputed in review:** the paths, their point 0 and its vertex and level are
recorded not at registration but when the gulag starts working (its first member, 4.7) or, for a gulag saved as
working, when it registers after a load; so the fatal errors of 2.2 arise then. The capacity becomes the smaller of the custom-data `capacity` and the number of
jobs whose `state` contains 0 [SJ-6d].

### 4.2 The gulag's state

A working gulag has a state number (0 at first). At each gulag update, when the player exists, the type's state
function gives the state; when it differs, the state changes, its start time is recorded, and every job that does not
exist in the new state is freed (4.6) [SJ-8b]. A job whose `fill_idle` was used up in the previous state may be filled
again [SJ-8b]. The type's state functions are the level scripts' (`load_states`); an unknown type is fatal (`ERROR:
unrecognized gulag's type '%s'`) [SJ-8b]. When the gulag starts working (its first member), the condition lists
`switch_0` and `switch_1` of the custom data are parsed and kept for the state functions [SJ-8]. `esc_lager`'s states
are in section 6.

### 4.3 Availability of a job

A job is **available** when: it has never been begun, or more than `idle` ms of game time have passed since it was
last begun; and its time of unavailability after a death (4.6) has passed; and, for a job with `fill_idle`, it has not
been filled once already in this state (or, when `fill_idle` is a time, it is still within that time after its
beginning). A job **remains current** for its member while it has no `timeout`, or less than `timeout` ms have passed
since it was begun [SJ-8c]. **Disputed in review:** when the gulag starts working, every job's `fill_idle` is replaced
by its saved value, which is none for a gulag that was not saved as working; the value saved is that replaced one, so
it is none again after a load. The `fill_idle` of the job tables therefore never limits a job in the shipped scripts,
and the once-per-state use (4.2, 4.5 step 1) never occurs.

### 4.4 Assignment

At each gulag update, the jobs are distributed (`updateJob`):

1. Every available job of the current state whose member has not yet arrived, or for which it is no longer current,
   is freed (4.6); the available jobs form the list to fill.
2. The members without a job are the candidates.
3. The list is taken in blocks of equal priority, from the highest. For a block, every pair (free job of the block,
   candidate whose information the job's predicate accepts) gets a distance (below); the pairs are sorted by distance
   (shortest first; `table.sort`, so ties in the runtime's order) and taken in that order, giving each still-free job
   to each still-free candidate. Then each job of the block that is still free may take a member from a job of lower
   priority: among the jobs of lower priority, from the lowest, the members of the lowest-priority group of such jobs
   whose predicate the job accepts, the nearest is moved (its old job freed). **Disputed in review:** the search
   from the lowest stops before the position, in the list of available jobs, equal to the free job's position in the
   whole sorted job list of 4.1; lower-priority jobs at or before that position in the available list are not
   searched, so the search covers all lower-priority jobs only when every job before the free one is available.

[SJ-8d]

The **distance** of a member to a job is the straight-line distance between the member's position and the job's point
when both are on the same level (an online member: its game object's position; an offline one: its simulation
object's position), and 10,000 otherwise [SJ-7h].

Giving a job (`setJob`) records it for the member as not yet arrived and sets the member's squad and group
[SJ-7f]:

- online: `change_team(team, s, g)` (Q 3) with the member's team, *s* the number of the **current** level in
  `xr_gulag`'s list of 18 (`l01_escape` 1, …, `l12_stancia_2` 18) [SJ-8j], and *g* the job's `group` or else the member's group from
  before it joined; the member's information then takes the new squad and group; a dead online member is cleared
  instead (4.6);
- offline: the simulation object's `squad` and `group` (J 3.5) become the job's `squad` and `group`, or else those
  from before it joined.

### 4.5 Beginning a job

At each gulag update, before and after the distribution, every member with a job it has not begun is tested for
arrival: with `position_threshold = "graph"`, when the level vertex of its game vertex equals that of the job's game
vertex; otherwise when its distance to the job (4.4) is less than `position_threshold`, default 120 [SJ-7h, SJ-6e].
An arriving member **begins** the job:

1. it is marked arrived, the job's begin time is the game time, and a once-per-state job is used up;
2. its simulation object's job-online value becomes the job's `online` (absent: none) (`OS` 6.5.4) [SJ-7g];
3. a monster's smart-terrain task is activated (`smart_terrain_task_activate`, J 3.6: the "task reached" byte 1),
   which ends its online task state (5.2) [SJ-7g];
4. if online: its restrictions become the job's (`out_rest`, `in_rest`: those this gulag gave before and differ are
   removed with `remove_restrictions`, the new ones added with `add_restrictions`, X 5), and its logic is set from the
   job's section: the schemes of the job's logic section in the gulag's configuration (2.2) are configured and the
   section to activate is activated (`xr_logic`), so the member's scheme takes over from the engine's own behaviour
   (5.1, 5.2);
5. the first time this member begins any job since joining, the smart terrain's respawner (`respawn`) is called
   [SJ-7i];
6. the count of arrived members grows by one.

[SJ-8e]

When a member comes online (3.3): if it has begun its job, its restrictions are set again, and its logic is set from
the job unless the game was just loaded (then its own saved logic is restored); a member that has not begun, or a
creature without a smart terrain, starts its own logic (`xr_logic.initialize_obj`) [SJ-8f]. **Disputed in review:** the
test is the member's "arrived" flag, which freeing a job (4.6) does not clear (only a new job clears it); a member
freed after arriving that comes online before it is given another job gets neither job logic nor its own logic
(believed: static reading).

### 4.6 Leaving a job and the smart terrain

**Freeing** a job (the member stays in the smart terrain): the job and member are unlinked, the member's job-online
value is cleared, a monster's smart-terrain task is deactivated (the byte 0), the restrictions this gulag gave are
removed if the member had arrived online, and the arrived count drops if it had arrived; if the member is online and
had arrived, its logic is restarted as at spawn (`xr_logic.initialize_obj`) [SJ-7g, SJ-8g].

**Releasing** a member (`unregister_npc`): its job is freed; it leaves the gulag; its simulation object's squad and
group return to those from before it joined; its smart-terrain id is cleared (`clear_smart_terrain`, J 3.6), after
which `OS` 7.2's choice applies to it again [SJ-8g]. A dead member that had an exclusive smart terrain returns one
exclusive place (OS-5's) [SJ-8g]. A member is released:

- when it dies online: the job counts a casualty and stays unavailable for `idle_after_death` (default 600) seconds of
  game time, then the member is released [SJ-7i]; **Disputed in review:** the casualty and the unavailability apply only
  when the member had a job; the member is released at once, not after that time;
- when its exclusive condition for this smart terrain no longer holds (checked at the gulag update, 4.7) [SJ-7j];
- when the engine asks for its task while it has no job (3.2) [SJ-7d];
- when it is pushed out by a newcomer that no free job suits (OS-5's joining; the first non-exclusive member whose job
  suits the newcomer (**Disputed in review:** "first" is in the runtime's traversal order of the table of members keyed
  by id, not the order of joining; whether it matches the original's is unestablished); if there is none, a fatal error `[smart_terrain %s] can't add npc, all are exclusive or there
  are no suitable jobs`) [SJ-8g];
- when the gulag is removed (4.7), with every member [SJ-8h];
- when the creature is unregistered from the simulation (3.3).

### 4.7 The update and the gulag's life

The gulag starts working when its first member joins (OS-5's `register_npc`, which also runs a gulag update at once)
[SJ-8h].
At each update of the smart terrain (3.1), if the gulag works:

- if the custom-data `cond` is given and does not hold: the gulag is **removed**;
- otherwise, at most once per 50,000 ms of game time (the first time at once) [SJ-6f]: members whose exclusive
  condition no longer holds are released; then, with no member left, the gulag is removed; else the gulag update runs
  [SJ-7j].

The **gulag update**: members that went offline without notice are marked offline (their restrictions forgotten, an
online member found by `level.object_by_id` set up online again; **Disputed in review:** in this pass a member is marked
offline only when `level.object_by_id` does not find it, so the setting up online again never happens here; it
happens when a creature that is online joins); the arrival test (4.5); the state (4.2, only while
the player exists); the distribution (4.4); the arrival test again [SJ-8h].

**Removal** releases every member, then makes a new, idle gulag of the same type with its jobs loaded (a new working
period starts with the next member) [SJ-8h].

### 4.8 Times recorded but not used

The working period's end (`duration`: hours drawn with `math.random` between the custom data's two values, default
10,000 and 10,001) [SJ-8], the idle period's end after a removal (`idle`, default 0: none) and each member's stay end
(`stay`, 2.4) are computed and saved, but no shipped script compares them [SJ-6g, SJ-6h].

### 4.9 What a save holds

The smart terrain's saved state (its scripted `STATE_Write`, after the engine part, J 3.8) is: the working period's
end and the idle period's end (game times), a flag "working", and when working: the casualties, the state, its start
time, the total, arrived and non-exclusive member counts (8 bits each; the casualties and the state are 8 bits too), the
number of jobs (8 bits) and per job its begin time,
`fill_idle` and end of unavailability after death (32 bits each, 0 for none), the number of members (8 bits) and per
member its id (16 bits), its group and then its squad from before joining (8 bits each), whether it joined
exclusively, its stay end, whether it has arrived, whether it has not yet begun any job since joining, and its job's
number (8 bits, 0 for none), and finally whether the population is locked [SJ-8i]. How the scripted packet's values are
encoded is `script-api-scope` I's (data streams, not yet specified). The job of a member is restored by its number in the sorted list of 4.1 (believed safe: the
sort of the same table in the same runtime gives the same order).

## 5. The engine's part: online behaviour toward the task

### 5.1 Stalkers

The stalker's planner (CH-17) has, as its action 57 (`stalker_ids.action_alife_planner`), a planner of three actions
over three properties [SJ-11, SJ-11b, SJ-11c, SJ-11f, SJ-14]:

| Property | Value |
|---|---|
| 3 `property_alife` | a simulation exists |
| 4 `property_puzzle_solved` | always false |
| 5 `property_smart_terrain_task` | false without a simulation; otherwise the smart-terrain choice of `OS` 7.2 (K 12.1 step 1: with its interval and per-object setting) is run for the stalker, then true when the stalker has a smart terrain |

| Action | Conditions | Effect (believed: from the planner's pattern of conditions and one effect per action) |
|---|---|---|
| 3 `action_no_alife` | 3 false, 4 false | 4 true |
| 4 `action_smart_terrain_task` | 3 true, 5 true | 5 false |
| 5 `action_solve_zone_puzzle` | 3 true, 5 false, 4 false | 4 true |

[SJ-11b, SJ-11d, SJ-11e]. So an online stalker with a smart terrain, whenever the planner runs this planner, takes
**action 4**; one without a smart terrain takes action 5 (CH-17's), and the evaluation of property 5 is how an online
stalker chooses a smart terrain. When the stalker's planner runs action 57 is CH-17's; the shipped schemes add their
own conditions to it (26 scripts fetch it for that) [SJ-7e], so a stalker whose scheme is active does not run it.

Action 4 (the smart-terrain action), static [SJ-12]:

- **When it starts**: the start time is recorded; the stalker's script animations are cleared as `clear_animations()`
  does (X 10.3); the movement request (X 4) gets no desired direction, detail path type 0 (line), body state 1
  (standing), movement type 0 (walk), mental state 1 (free); a sight request of type 1 (`look.path_dir`) with the
  flags *A* and *B* 0 is handed to the sight control (X 6.1) [SJ-12b, SJ-12c, SJ-12d]. The hand: when the stalker has a
  best weapon (CH-20's choice), `set_item(object.idle)` without object (X 10.1), then, unless the weapon is strapped
  (as `weapon_strapped()`, X 10.2), `set_item(object.idle, best weapon)`; without one, `set_item(object.idle)`
  [SJ-12e]. One further movement setting is cleared (**unestablished** which).
- **At each execution**: from 30,000 ms after the start, `set_item(object.strap, best weapon)` (none when it has none)
  [SJ-11, SJ-12f]; `play_sound(3, 60000, 10000, 0, 0)` (T 6.3; 3 is `stalker_ids.sound_humming`) [SJ-12g]; then the
  task is asked for (3.2), and:
  - if the stalker's game vertex is the task's game vertex: path type 1; when the task's level vertex is accessible
    to the stalker (as `accessible(v)`, X 5), the destination level vertex is it and the desired position is the
    task's position; otherwise the destination is the accessible place nearest the task's position (as
    `accessible_nearest`, X 5; the exact rule is CH-9's) [SJ-12h];
  - otherwise: path type 0, with the task's game vertex as the destination over the game graph [SJ-12h].

  What path types 0 and 1 mean for the movement is CH-9's and CH-10's (believed: 0 a path over the game graph, 1 a
  path over the level graph, from the destinations each is given).
- **When it ends**: the script animations are cleared again, and the movement setting cleared at the start is set
  again [SJ-12i].

So an online member walks, in the free mental state and humming, to its job's point; within 120 m of it (4.5) the
gulag begins the job and the member's scheme takes over.

### 5.2 Monsters

The 15 monster kinds with a state machine (bloodsucker, boar, dog, flesh, pseudodog, burer, cat, chimera, controller,
fracture, poltergeist, pseudo-giant, snork, tushkano, zombie) have a **smart-terrain task state**, which their rest state
tries first, before its other substates (the poltergeist's own rest state likewise); when the rest state runs is
CH-22's [SJ-13, SJ-13b].

- It **starts** when a simulation exists, the monster is not a psy-dog phantom, and, after the smart-terrain choice of
  `OS` 7.2 is run for it, the monster has a smart terrain and its "smart-terrain task reached" byte (J 3.6) is 0
  [SJ-13c]. It **completes** when the monster has no smart terrain or the byte is not 0 [SJ-13c].
- It asks for the task when it starts and at each execution; when the task differs from the one it had, it abandons its
  current substate and chooses again [SJ-13e].
- Its substates, chosen in this order [SJ-13d, SJ-13e, SJ-13f, SJ-13g]:
  1. **game-graph walk**, when the monster's game vertex is not the task's: the task's game vertex becomes its
     destination over the game graph; it completes at that vertex (its other settings are **unestablished**; CH-12);
  2. **move to point**: to the task's level vertex, at that vertex's own position (as `level.vertex_position`, not the
     patrol point's position; its other movement parameters are **unestablished**; CH-12);
  3. then a **custom action** with the monster action 8 (which motion that is is CH-12's).

Beginning the job (4.5) sets the byte, which completes the state; freeing the job clears it.

## 6. Worked example: `esc_lager` and its campfire job

`esc_lager` (the novices' camp) is record 101 of `all.spawn`, on `l01_escape`, game vertex 61, level vertex 44,622,
position (−208.55, −20.09, −141.65); its custom data is `[smart_terrain]` with `type = esc_lager`, `capacity = 13`,
`squad = 1`, `groups = 5`, `respawn = esc_respawn_inventory_box_0000` (a respawner record, `RE_SPAWN`) [SJ-20, SJ-24d].
Only `gulag_escape` defines jobs for its type, and it generates no configuration, so the jobs' sections come from
`gulag_tasks.ltx` (that is, `gulag_escape.ltx`) [SJ-20b]. Type `esc_lager` accepts the community `stalker` (OS-5's
acceptance) [SJ-24c]; 11 stalker records name `esc_lager` in their `[smart_terrains]` section (OS-5's exclusives)
[SJ-25].

**Jobs.** 30 jobs; 13 exist in state 0, so the capacity is min(13, 13) = 13 [SJ-21]. Every job path exists in
`all.spawn` [SJ-23d].

**States.** Without the player the state is kept; with infos `escape_lager_help` and `escape_lager_killers_die` it is 2
(defence); else 0 for game hours 5 … 22 (day) and 1 otherwise (night) [SJ-24]. A New Game starts at 05:30:00, so the
state is 0 [SJ-24b].

**The campfire job.** Six jobs use the section `logic@esc_lager_kamp1` [SJ-22]:

| Priority | States | Other |
|---|---|---|
| 6 | 0 | three such jobs: out-restriction `esc_lager_guard_kill_zone`, no in-restriction |
| 16 | 2 | for the profile `esc_shustryi` only (predicate); `online = true`; no restrictions |
| 3 | 1 | out-restriction `esc_lager_guard_kill_zone` |
| 0 | 0, 1 | out-restriction `esc_lager_guard_kill_zone` |

None has `group`, `squad`, `position_threshold` or `timeout` [SJ-22c]. The restrictor `esc_lager_guard_kill_zone` is
record 87 (`SPC_RS_S`) [SJ-22b]. In `gulag_escape.ltx`, `[logic@esc_lager_kamp1]` has `active = kamp1@esc_lager`, and
`[kamp1@esc_lager]` has `center_point = camp_center`, `path_walk = camp_center_task`, `soundgroup = esc_lager`,
`meet = meet@lager`; `path_walk` comes first, so the job path is `esc_lager_camp_center_task` [SJ-23]. That path has one
point, `wp00`, at game vertex 61, level vertex 44,621, position (−208.527, −20.055, −141.995), 0.35 m from the smart
terrain's own position; the scheme's centre `esc_lager_camp_center` is at (−207.002, −20.191, −140.228) [SJ-23b,
SJ-23c].

So, at a New Game, in state 0, a member given one of the three priority-6 campfire jobs:

- if offline: travels toward game vertex 61 (`OS` 7.3, 7.5, the task being point 0 of `esc_lager_camp_center_task`); its
  squad and group stay as they were (the job has none) (4.4); it begins the job at the first gulag update at which its
  simulation object's position is within 120 m of (−208.527, −20.055, −141.995) (4.5); the job-online value stays
  unset (the job has no `online`);
- if online: its team, squad and group become (its team, 1, its old group) (4.4); it walks to the job's point under
  the smart-terrain action (5.1), at game vertex 61 heading for level vertex 44,621 and position (−208.527, −20.055,
  −141.995); within 120 m it begins: it gets the out-restriction `esc_lager_guard_kill_zone` and the logic
  `kamp1@esc_lager` (the camp scheme, CH-17's), and on its first beginning the respawner
  `esc_respawn_inventory_box_0000` is called (4.5).

Which members `esc_lager` has at a New Game, and so who takes the campfire jobs, is OS-5's.

## 7. Unestablished and believed

- The order of jobs of equal priority and of pairs of equal distance (4.1, 4.4) is the runtime's `table.sort`'s; it
  matches the original only if the runtime's sort is the same algorithm (`script-environment`).
- The meaning of the stalker's path types 0 and 1 (believed: game-graph and level-graph paths) and one movement setting
  the smart-terrain action clears and sets (5.1).
- The effect of each alife-planner action (believed from the planner's pattern, 5.1); what action 5 does (CH-17).
- The monster's game-graph walk's other settings, the move-to-point parameters and which motion action 8 is (5.2).
- Whether restoring a member's job by its number after a load is always the same job (believed, 4.9).

## 8. Coverage

`scope` 8 gives CH-28 three names: `file:config\misc\gulag_*.ltx` (2.2), `file:config\misc\smart_terrain*.ltx` (2.4),
`relations:[smart_terrain_rank_change]` (2.5).

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binary `bin\xrGame.dll`.
- Data examined: winning copies of `config\`, `scripts\` and `spawns\all.spawn` as extracted for the approved
  `pda-scope`; `game.graph` and `all.spawn` as extracted for an earlier specification of the level's objects; the shipped
  API listing `scripts\lua_help.script`; the approved specifications listed above.
- Analysis: reading of the spawn records and patrol paths with readers written from `sav` 7 and `OS` 4; static reading
  of the shipped scripts and configuration; static analysis (read-only decompilation and disassembly, run-time type
  information) of `xrGame.dll` for the engine's smart-terrain calls, the stalker's alife planner and its smart-terrain
  action, and the monsters' smart-terrain task state. Nothing was run.

## Gaps found in review

- The binder's binding is fatal for an object with a `[smart_terrain]` or `[gulag1]` custom-data section whose class id
  is not `SMRTTRRN` (`You must use SMART_TERRAIN instead of SCRIPT_ZONE %s`), not stated in 3.1.
- Releasing a member restores only its simulation object's squad and group (4.6); what becomes of an online member's
  team, squad and group set with `change_team` (4.4) is not stated.
