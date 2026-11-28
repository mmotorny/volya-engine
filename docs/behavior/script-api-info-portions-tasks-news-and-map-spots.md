# Script API P: information portions, tasks, news and map spots

How the game scripts of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) reach the player's story state through
the engine: the `game_object` members for information portions, tasks, news, talk messages and use tips; the
classes `CGameTask`, `SGameTaskObjective` and `task`; the `level` functions for map spots and the game difficulty;
the `game` functions for tutorials; and the script functions the engine calls from the task file
(`function_complete`, `function_fail`, `function_call_complete`, `function_call_fail`), from the script-added task
objectives (`add_complete_func` and the three others), and from tutorial and movie sequences (`function_on_start`,
`function_on_stop`). This is spec P of the approved scoping specification `docs/behavior/script-api-scope.md`
(section 6, order 22); its scope rule and review gaps apply: this spec owns these names as the binary registers
them, listed or not, and the entry points its section 4.5 assigns to P. For each entry point it states arguments,
results, timing and failure handling.

Builds on (not repeated here):

- `docs/behavior/script-environment.md` (the runtime, one number type, the binding library),
  `docs/behavior/script-loading.md` (the 250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched to a
  member's forms; what a call of a member the engine lacks does). Statements that need it say **depends on A**.
- Spec D, `docs/behavior/script-api-engine-entry-points.md`: its section 1.1 (**resolution** of a script function
  by its `namespace.function` name) and 1.2 (a call that fails ends the program; written **fatal as D 1.2**).
- Spec F, `docs/behavior/script-api-configuration-and-files.md` (the text of configuration files).
- Spec M, `docs/behavior/script-api-game-object-basics.md`: the **handle**, the **kinds** of its 2.2 (here:
  inventory owner), the routine that swallows diagnostics (its 2.3, written **swallowed**).
- Spec O, `docs/behavior/script-api-object-event-callbacks.md`: the callback ids `task_state` (13) and
  `map_location_added` (14), their receivers and arguments, and how an entry is called.
- `docs/behavior/info-portions.md` (cited as `IP`): holders, the **transfer** of its 3.1, receiving (3.2–3.4),
  disabling (4), the questions (2), the player character's tasks given by a portion's `task` child (3.4 step 2).
- `docs/formats/sav.md` (cited as `sav`): the task record (9.8), the map-location and news registries (9).
- `docs/behavior/talk-window.md` 6.2 (iconed lines), `docs/behavior/use-action.md` (the usable part and its hint
  text, 2.2 and 10), `docs/behavior/hud-scope.md` (specs 11 map locations and 18 tutorials),
  `docs/behavior/pda-scope.md` (PD-5 task definitions, PD-6 tasks, PD-11 news).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [P-1].
- **The listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only.
- **The player's object** is the engine object the player controls (O's conventions). **The player's tasks** are
  its list of tasks (the simulation's registry that `sav` 9 part 8 saves). A **number** is the runtime's double, a
  **text** a Lua string; **true** for a result means Lua truth. Game times are milliseconds (`sav` 12). A call is
  written as scripts write it.
- **The task file** is `config\gameplay\game_tasks.xml` with its `#include` lines (PD-5's data). **Logged** means
  written to the game's log; **swallowed** means handed to M 2.3's routine, which writes nothing.
- Engine classes are named only as the game library registers them for scripts. Each fact is **static** (from
  static analysis of the binaries, not observed at run time), **measured** (read from game data or file bytes),
  **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag: [P-*n*] in `checks/script-api-tasks/run_all.py` in the dirty workspace
(`python3 -I run_all.py`; it reads the binaries' bytes and run-time type information, the cached evidence of
read-only headless static analysis produced by `gh.py` beside it, the listing and the scoping spec's scan), and
[D-*n*] in `data.py` beside it (the game data and the shipped scripts), which `run_all.py` also runs. All checks
pass (`ALL PASS 31`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 54 listed items [P-2]:

| Owner | Items |
|---|---|
| `task` | the class and its 4 constants (section 3.1) |
| `CGameTask` | the class; `add_objective`, `get_id`, `get_objective`, `get_objectives_cnt`, `get_title`, `load`, `set_title` (3.2) |
| `SGameTaskObjective` | the class; `add_complete_func`, `add_complete_info`, `add_fail_func`, `add_fail_info`, `add_on_complete_func`, `add_on_complete_info`, `add_on_fail_func`, `add_on_fail_info`, `get_description`, `get_idx`, `get_state`, `set_article_id`, `set_article_key`, `set_description`, `set_map_hint`, `set_map_location`, `set_object_id` (3.3) |
| `game_object` | `give_info_portion`, `disable_info_portion`, `has_info`, `dont_has_info`, `get_info_time`, `info_add`, `info_clear` (section 2); `give_task`, `get_task_state`, `set_task_state` (4); `give_game_news`, `give_talk_message` (7); `set_tip_text`, `set_tip_text_default` (8) |
| `level` | `map_add_object_spot`, `map_add_object_spot_ser`, `map_change_spot_hint`, `map_has_object_spot`, `map_remove_object_spot` (9); `get_game_difficulty`, `set_game_difficulty` (10) |
| `game` | `start_tutorial`, `has_active_tutorial` (11) |

The scoping spec also assigns it 8 candidate strings: `def_ml_enabled`, `get_priority`, `set_id` and `set_priority`
are registrations (1.2); `function_complete`, `function_fail`, `function_call_complete` and `function_call_fail` are
element names of the task file (section 6), not registrations [P-2, P-3, P-7]. The scripts use 42 names of this spec,
by 71 loadable scripts (73 when method names are attributed by name alone, scoping spec section 6); one of them,
`give_info_portion_via_pda`, is absent: it occurs in no file under `bin\` (section 2.4) [P-2].

### 1.2 As registered (static)

- `task` has no constructor and holds 4 integer constants (3.1) [P-3].
- `CGameTask` has one constructor with no arguments and the members `load`, `set_title`, `get_title`,
  `get_priority`, `set_priority`, `get_id`, `set_id`, `get_objectives_cnt`, `add_objective`, `get_objective`;
  `get_priority`, `set_priority` and `set_id` are not in the listing [P-3].
- `SGameTaskObjective` has one constructor taking two arguments (a task or `nil`, and a number), the members of 1.1
  and two that the listing lacks: the method `set_icon_name(text)` and the read-write property `def_ml_enabled`
  (a boolean) [P-3, P-8].
- Each of the 9 `level` and `game` functions is registered once [P-4].
- A Lua script's `CGameTask` or `SGameTaskObjective` value made by its constructor belongs to the script runtime,
  which frees it when it is collected (believed: the binding library's usual ownership; the registrations name a
  destructor for each class) [P-6, P-3].

## 2. Information-portion members of `game_object` (static)

### 2.1 Giving, disabling and asking

What giving, disabling and the questions do is `IP` 3–5. As called from scripts [P-5]:

| Call | On a holder (an object of M's inventory-owner kind) | On any other object |
|---|---|---|
| `obj:give_info_portion(id)` | a transfer *give* of `id` (`IP` 3.1); returns `true` | nothing; returns `false` |
| `obj:disable_info_portion(id)` | a transfer *disable* of `id` (`IP` 4.1); returns `true` | nothing; returns `false` |
| `obj:has_info(id)` | `true` when the holder holds `id` (`IP` 2) | `false` |
| `obj:dont_has_info(id)` | the negation of `has_info` | `true` |

All four take a text and share one argument form [P-5].

### 2.2 `get_info_time(id)`

`obj:get_info_time(id)` returns a new game-time value (of the class that spec H names `CTime`) [P-5]: the time at
which the holder received `id` (`IP` 2); a time of 0 when the holder does not hold `id`, and when `obj` is not a
holder (this settles `IP`'s second review gap) [P-5]. The shipped scripts never call it (`IP` 2).

### 2.3 `info_add(text)` and `info_clear()`

Both do nothing: their implementations return at once [P-5, P-19]. Three loadable scripts call `info_add` and two
`info_clear` (section 12); the calls have no effect.

### 2.4 `give_info_portion_via_pda`

No shipped binary has this name [P-2]; what calling it does is A's rule for a member the engine lacks (`IP` 5;
**depends on A**).

## 3. Task values

### 3.1 `task` (static)

The constants, as listed [P-3]:

| Name | Value | Meaning in this spec |
|---|---|---|
| `fail` | 0 | an objective's failed state |
| `in_progress` | 1 | an objective's in-progress state |
| `completed` | 2 | an objective's completed state |
| `task_dummy` | −1 | the result of `get_task_state` when there is no such task or objective (4.2) |

### 3.2 `CGameTask` (static)

A task holds [P-6, P-7, P-10]: an **id** (a text), a **title** (a text), a **priority** (a whole number), a list of
**objectives** (3.3; objective 0 stands for the task as a whole, section 5), and three game times: **received**,
**finished** and the **time limit** (the three times of the `sav` 9.8 record, in that order; the record's "third
time" is the time limit).

- `CGameTask()` makes a task with no id, no title, no objectives, received and finished times 0; its priority and
  time limit are not set (**unestablished**: whatever the new object's memory holds; **believed** harmless: `load`
  sets the priority and `give_task` the limit, and the shipped script always calls both) [P-6].
- `t:load(id)` (3.4) fills the task from the task file.
- `t:get_id()`, `t:set_id(text)`; `t:get_title()`, `t:set_title(text)`: read and replace the id and the title. A
  task with no id or title returns no text (believed: `nil`) [P-6].
- `t:get_priority()`, `t:set_priority(n)`: read and replace the priority (stored as a 32-bit integer; how a number
  is converted is **depends on A**) [P-6].
- `t:get_objectives_cnt()`: the number of objectives [P-6].
- `t:get_objective(n)`: objective *n* (from 0) of the task itself, not a copy: changing it changes the task
  (`task_manager.script` sets objective 0's article this way). There is no range test: an *n* outside 0 … count − 1
  refers outside the list (**believed**: undefined, a crash or garbage) [P-6]. The value refers into the task's
  list, so it is no longer valid once `add_objective` has grown the list (believed: the list may move).
- `t:add_objective(o)` appends a **copy** of `o` (3.3) to the task's objectives, after resolving `o`'s four lists of
  function names (section 6.1) [P-9]. Later changes to `o` do not reach the task.

### 3.3 `SGameTaskObjective` (static)

An objective holds [P-8, P-9]: its **task** (the task given to its constructor, or none) and **index**; its
**state** (3.1); a **description** text; an **article** id; an **article key**; a **map spot type**, a **map spot
hint** and a **target object id** (none, 65,535, by default); an **icon** (a texture name and a rectangle); the flag
**shown by default** (true by default); four lists of portion ids and four lists of script functions:

| List | Filled by | Used (section 5) |
|---|---|---|
| complete portions | `add_complete_info(id)`; task file `infoportion_complete` | all held → completed |
| fail portions | `add_fail_info(id)`; `infoportion_fail` | all held → failed |
| set-on-complete portions | `add_on_complete_info(id)`; `infoportion_set_complete` | given to the player on completion |
| set-on-fail portions | `add_on_fail_info(id)`; `infoportion_set_fail` | given to the player on failure |
| complete functions | `add_complete_func(name)`; `function_complete` | all true → completed |
| fail functions | `add_fail_func(name)`; `function_fail` | all true → failed |
| on-complete functions | `add_on_complete_func(name)`; `function_call_complete` | called on completion |
| on-fail functions | `add_on_fail_func(name)`; `function_call_fail` | called on failure |

- `SGameTaskObjective(t, n)` makes an objective of task `t` (or of no task, when `t` is `nil`) with index *n*,
  state in progress (1), no target object, shown by default, every text and list empty [P-3, P-8]. *n* is stored as
  a 32-bit integer and is not checked against the task (**depends on A** for the conversion). The index is what the
  engine passes to the objective's functions and to `task_state` (O); the objective's place in a task is where
  `add_objective` puts it, so a script must give consecutive indices from 1 (believed: the shipped
  `task_manager.script` does) [P-8]. In this spec "objective *n*" is the *n*-th of a task's list (what
  `get_objective`, `get_task_state` and `set_task_state` use); the stored index is what is passed to functions and
  callbacks, and what the time-limit test of 5.1 reads.
- The `add_*_info` members append a portion id to their list; the `add_*_func` members append a function name to
  their list of **names** (section 6.1) [P-9].
- `set_description(text)`, `set_article_id(id)`, `set_article_key(text)`, `set_map_location(type)`,
  `set_map_hint(text)` store their text; `set_object_id(id)` stores the target object id (16 bits) [P-8].
  `get_description()` returns the description (no text: believed `nil`), `get_state()` the state, `get_idx()` the
  index [P-8].
- `set_icon_name(name)` stores `name` as the icon and looks it up as a texture id of the interface's texture
  descriptions (spec G3's `GetTextureInfo` data): the icon becomes that entry's texture file and rectangle
  (x, y, width, height) [P-8]. What an unknown id gives is **unestablished**.
- `o.def_ml_enabled` reads and writes the flag shown by default [P-8]; the task file's `map_location_hidden`
  clears it (3.4). Its effect is in 4.1 and 5.3.
- An objective has no member returning its task, texts other than the description, or its lists.

### 3.4 `load(id)`: a task from the task file (static; data measured)

`t:load(id)` sets the task's id to `id`, then reads the `game_task` element of the task file whose attribute `id`
equals `id` [P-7]. The file is opened the first time any task is loaded and kept for the rest of the run [P-7]. A
missing element stops the game with "NavigateToNode failed in XML file " and the file name (`IP` 3.4 step 2, the
same lookup) [P-7]. From the element:

1. The title is the text of its `title` child; the priority is its attribute `prio`, −1 when absent [P-7].
2. The objective list is emptied; then for each `objective` child *i* in file order an objective of this task with
   index *i* (3.3's defaults) is appended and filled [P-7]:
   - description: `text`; article: `article` (when present); article key: the attribute `key` of the objective;
   - only for objective 0, the icon: the `icon` text; when it is not empty and is not `ui\ui_icons_task` (compared
     ignoring case), it is a texture id resolved as by `set_icon_name`; otherwise the rectangle is the attributes
     `x`, `y`, `width`, `height` of `icon` (0 when absent) [P-7]. Icons of other objectives are never read (the data
     has 104) [D-8];
   - map spot type: `map_location_type`; hint: its attribute `hint`; shown by default is cleared when a
     `map_location_hidden` child exists; target object: when `object_story_id` exists, its text is looked up in
     the global table `story_ids` (an engine class whose constants are the story ids, spec K) and the story id
     converted to an object id: through the simulation's story registry when the simulation runs, else by searching
     the level's objects; none (65,535) when no object has that story id [P-7];
   - the four portion lists from every `infoportion_complete`, `infoportion_fail`, `infoportion_set_complete`,
     `infoportion_set_fail` child, in order;
   - the four function lists from every `function_complete`, `function_fail`, `function_call_complete`,
     `function_call_fail` child, each **resolved** at once (section 6.1); the objective's lists of names stay empty
     [P-7].
3. Nothing else of the task changes (its times, for example) [P-7].

A story-id text that `story_ids` lacks reads as 0 (believed: Lua's conversion of `nil` to a number gives 0), so the
target is the object with story id 0. The data has two such texts, `agr_doctor_spot` (`tasks_escape.xml`) and
`escape_provodnik` (`tasks_yantar.xml`), and story id 0 is the listing's `Escape_blockpost_case` [D-9].

The task file has 164 tasks, 340 objectives, 61 tasks with a `prio`; 162 objectives with a map spot type (141 with a
hint), 25 with `map_location_hidden`, 162 with `object_story_id` [D-8].

## 4. The player's tasks: `give_task`, `get_task_state`, `set_task_state` (static)

All three act on **the player's tasks**, whatever object they are called on: the receiver is not used [P-10, P-11].
The loadable scripts call `give_task` and `set_task_state` on `db.actor` (the player's handle) [D-6]. With no
player's object (before it exists) they reach through a missing object (**believed**: a crash; the scripts call them
only once the player exists).

### 4.1 `give_task(t, time, check)`

`obj:give_task(t, time, check)` (`t` a `CGameTask`, `time` a number of milliseconds, `check` a boolean) [P-10]:

1. If `check` is true and the player already has a task with `t`'s id, nothing happens. With `check` false a second
   task with the same id is added (the shipped script passes `false`) [P-10, D-6].
2. The player's tasks are marked changed (5.1), and `t` itself (not a copy) is appended to them. Its received time
   becomes the current game time and its time limit the received time plus `time` (unsigned 32-bit arithmetic);
   `time` 0 means no limit (5.1) [P-10]. Whether the engine then owns `t` or the script runtime may free it is
   **unestablished** (believed: the engine keeps it; the shipped script drops its reference at once).
3. The player's tasks are sorted by priority, lowest first (unsigned comparison, so −1 sorts last; the order of
   equal priorities is **unestablished**) [P-10].
4. For each objective in order [P-10]:
   - a non-empty article id that the player's known articles lack is added to them with the current game time, the
     article's type and *not read*, as `IP` 3.4 step 1 adds one, but without the `article_info` callback (an
     article id that is not declared: believed the same stop as `IP` 1.2);
   - when the objective has a target object, a non-empty map spot type and is shown by default, a map spot of that
     type is added for the target (section 9.1, with the callback `map_location_added`), given the objective's hint
     when that is not empty, and marked saved (`sav` 9) [P-10, P-20].
5. When the player has no active task (5.4), or the active task's priority is greater than `t`'s, `t` becomes the
   active task with objective 1 [P-10]. **Disputed in review:** the priorities are compared unsigned, as in step 3,
   so an active task with priority −1 is replaced by any new task; "greater" alone does not say so.
6. The PDA is told that its contents changed (what it shows: PD-1, PD-7) [P-10].
7. The callback `task_state` (O, id 13) is raised on the player's object with `t`, objective 0 and objective 0's
   state (normally in progress) [P-10]. This is the only callback this member raises [P-10].

The data path that gives a task (a portion's `task` child, `IP` 3.4 step 2) loads the task by its id, uses the same
rule for the active task and raises the same callback (static, `IP` B10) [P-10]; its other steps are PD-6's.

### 4.2 `get_task_state(id, n)`

Returns the state of objective *n* of the first of the player's tasks with that id (the list's order, 4.1 step 3);
`task.task_dummy` (−1) when the player has no such task, and also when *n* is not less than the task's objective
count, after a swallowed diagnostic "wrong objective num" [P-11].

### 4.3 `set_task_state(state, id, n)`

Sets objective *n* of the first of the player's tasks with that id to `state` (section 5.2) [P-11]. When the player
has no such task, the message `actor does not has task [<id>]` is logged and nothing else happens [P-11]. *n* is not
range-checked (**believed**: an index outside the objectives refers outside the list) [P-11]. The argument order is
the listing's (state, task id, objective); the loadable caller `level_tasks.set_task_state` keeps it [D-6]
(`info_test1.script`, which is not loadable, passes another order).

## 5. Objective states (static)

### 5.1 The update

In the player's scheduled update (the update the engine's scheduler calls for the player's object; its rate is
**unestablished** here), and only when the game is single-player (believed: the game type value the update tests),
the engine walks a copy of the player's tasks in order [P-14]. For each task it walks the objectives in order:

- an objective that is not in progress is skipped, except objective 0: then the rest of the task is skipped;
- an objective in progress is **evaluated** (below); when the result is failed (0) or completed (2) its state is
  **changed** to it (5.2) at once, so a later objective of the same task sees the change.

**Evaluation** of an objective in progress, in this order; the first that applies gives the result [P-13]:

1. Only an objective whose stored index is 0: when the task's time limit differs from its received time (a limit was
   given) and the current game time is later than the limit: failed.
2. The fail portions: the list is not empty and the player's object holds every portion of it: failed.
3. The fail functions (section 6.2) are all true: failed.
4. The complete portions: not empty and all held: completed.
5. The complete functions are all true: completed.
6. Otherwise the objective keeps its state.

A portion list that is empty never applies; neither does a function list that is empty [P-13]. "Holds" is `IP` 2's
question on the player's object.

After the walk: the active objective's map spot is marked as the active task's (what that shows: `hud-scope` 11);
and when the player's tasks are marked changed, the hidden spots and the active task are brought up to date (5.3,
5.4) and the mark is cleared [P-14].

### 5.2 A state change

Changing objective *n* of a task to `state`, by the update or by `set_task_state` [P-12]:

1. The player's tasks are marked changed.
2. When `state` is failed or completed and the objective has a map spot of its type on its target, one count of that
   spot is removed (section 9.1); the objective's map spot type is then cleared and its target becomes none.
3. The state is stored. Then [P-12]:
   - failed (0): each set-on-fail portion is given to the player's object by a transfer (`IP` 3.1), in order; then
     each on-fail function is called (section 6.2);
   - completed (2): the set-on-complete portions are given, then the on-complete functions called, likewise;
   - in both cases the callback `task_state` (O, id 13) is raised on the player's object with the task, the
     objective and the new state. Any other value is stored with nothing else (O's "only for the new states 0
     and 2" at this calling place).
4. The active task (5.4):
   - when *n* is 0, or no objective from 1 on is still in progress: if this is the active task, there is no active
     task any more; and when *n* is 0, every objective still in progress is changed to the same `state` by this same
     step, in order;
   - otherwise, when this objective is the active objective and is not the task's last, the next objective
     (*n* + 1) becomes the active one [P-12].
5. The task's finished time becomes the current game time when objective 0 becomes completed, and when **any**
   objective becomes failed [P-12].
6. The PDA is told that its contents changed [P-12].

Consequences (believed, from step 4): `set_task_state(task.in_progress, id, 0)` on an objective 0 that is in
progress repeats step 4 on itself without end (a crash); objectives completed or failed before objective 0 keep
their state when objective 0 changes.

### 5.3 Hidden map spots

When the player's tasks are marked changed, after the update's walk: for every task whose objective 0 is in progress,
each objective that is **not** shown by default, is in progress, has a target and a map spot type, and whose previous
objective is completed, gets its map spot (section 9.1, with its hint when not empty, marked saved) [P-14]. So a
`map_location_hidden` objective's spot appears once the objective before it is completed (believed: on every such
update while that holds, each adding one count of the spot, section 9.1).

### 5.4 The active task

The player has at most one **active task and objective** (saved with the tasks, `sav` 9). It changes in
4.1 step 5, 5.2 step 4 and here: after a marked change, when there is no active objective, the first task (in list
order) whose objective 0 is in progress and that has an objective from 1 on in progress gets its first such
objective as the active one [P-14]. **Disputed in review:** the condition is not "no active objective" but that
the active objective is none of the objectives of the tasks whose objective 0 is in progress (so an active objective
of a task whose objective 0 is no longer in progress is replaced too). Choosing one in the PDA is PD-7's. Making objective 0 the active one logs
`! g_active_task_objective_idx==0` [P-12].

## 6. The task functions (entry points, static; data measured)

### 6.1 Resolution

Each function name of an objective (from the task file, or a name added by `add_*_func`) is **resolved** (D 1.1)
into a function kept with the objective [P-7, P-9]:

- from the task file, when the task is loaded (3.4): a name that does not resolve is kept as an entry that is never
  called; nothing is written [P-7];
- from the lists of names, when `add_objective` copies the objective into a task (3.2), and when a saved game is
  loaded (6.4): the list of functions is rebuilt from the names; a name that does not resolve is logged as
  `Cannot find script function described in task objective  <name>` (with two spaces) and is never called [P-9].

### 6.2 The calls

Every task function is called with two arguments: the task's **id** (a text) and the objective's **index** (a
number) [P-13]. The call is protected; a failing call is **fatal as D 1.2** [P-13].

- **Complete and fail functions** (`function_complete`, `function_fail`, `add_complete_func`, `add_fail_func`) are
  conditions: called in list order during evaluation (5.1), each result taken as Lua truth; the walk stops at the
  first false. An entry that does not resolve is not called and keeps the running result, which starts false; so the
  list is true when its first entry resolves and every function called returned true [P-13].
- **On-complete and on-fail functions** (`function_call_complete`, `function_call_fail`, `add_on_complete_func`,
  `add_on_fail_func`) are actions: called in list order on the state change (5.2), results ignored [P-13].

So during an update, the fail functions of an objective in progress are called every update (unless its time limit
or fail portions already failed it), and its complete functions every update in which it did not fail.

### 6.3 Data

The task file has 103 `function_complete`, 113 `function_fail`, 4 `function_call_complete` and 1
`function_call_fail` elements naming 17, 19, 4 and 1 distinct functions, 40 in all, in 9 script namespaces
(`agroprom_tasks`, `bar_rostok_tasks`, `darkvalley_tasks`, `deadcity_tasks`, `escape_tasks`, `garbage_tasks`,
`mil_tasks`, `task_manager`, `yantar_tasks`); every one is defined in its script [D-1]. 27 of them take two
parameters (named `task_id, subtask`, `task, objective`, `actor, npc` or `p1, p2`) and 13 take none, ignoring the
arguments [D-1, D-2]. The only name a script adds is `task_manager.task_complete` (`add_complete_func`, in
`task_manager.script`; no script calls the other three `add_*_func`) [D-6].

### 6.4 Saving and loading

A saved task (`sav` 9.8) does not carry the functions of its task-file objectives [P-15]: an objective's "has
functions" flag and lists are its lists of **names** (3.3), which only `add_*_func` fills. On loading, each task is
first made again from the task file by its id (3.4; so its id must be a `game_task` of the task file, or the game
stops), objectives are appended when the save has more than the file, and each saved objective then overwrites the
objective's fields; when its "has functions" flag is set, its four lists of names are read and resolved (6.1),
replacing its function lists [P-15]. So task-file functions come back from the file, and script-added functions
from the save.

## 7. News and talk messages (static; data measured)

### 7.1 `give_game_news(text, texture, rect, delay, show)`

`obj:give_game_news(text, texture, rect, delay, show)` (texts, a `Frect` of spec E, two numbers) makes a news item
of the player's object, whatever object it is called on [P-16]: the text, the texture and its rectangle, and a show
time of `show` milliseconds, 5,000 when `show` is 0. With `delay` 0 it is added to the player's news at once;
otherwise it is queued with a due time of the frame clock's `time_global()` (spec B) at the call plus `delay`
milliseconds [P-16] (believed: added when the clock reaches it; the queue's processing was not traced). What adding
a news item does (the news list saved in `sav` 9 part 5, the HUD lines, the sound): PD-11 and `hud-scope` 9. The
shipped `news_manager.script` passes a delay in seconds × 1,000 with its own show time, and for task news a delay of
0 and a show time of 3,000 [D-7].

### 7.2 `give_talk_message(text, texture, rect, template)`

`obj:give_talk_message(text, texture, rect, template)` adds an iconed line to the talk window (`talk-window` 6.2),
whatever object it is called on, only while the single-player interface's talk window is shown (believed the talk
window: the window tested is the one the line is added to); otherwise nothing happens [P-17]. With no template
text, `iconed_answer_item` is used [P-17]. The shipped scripts name `iconed_answer_item` and `iconed_trade_info`
[D-7].

## 8. Use tips: `set_tip_text(text)`, `set_tip_text_default()` (static)

Every game object has a usable part with a hint text (`use-action` 2.2, 10). `obj:set_tip_text(text)` stores `text`
as that hint text; `obj:set_tip_text_default()` empties it [P-18]. On a handle with no object, both do nothing after
a swallowed diagnostic [P-18]. What the HUD shows for the hint is `hud-scope` 13's.

## 9. Map spots: the `level` functions (static)

### 9.1 The spots

The player's map spots are kept per pair (object id, spot type) with a **count** (the `map_spots.xml` types and
what a spot shows are `hud-scope` 11's) [P-20]. **Adding** a spot for a pair that has one adds 1 to its count
(and, when the spot has a lifetime, believed from its use, restarts it: the lifetime × 1,000 ms from the frame
clock); otherwise a new spot with count 1 is made, and when the game is single-player (believed: the game type the code tests) and the player's object
exists, the callback `map_location_added` (O, id 14) is raised with the type and the object id [P-20]. **Removing**
one count of a pair removes the spot when its count is 1 and otherwise lowers the count; nothing happens when the
pair has no spot [P-20].

### 9.2 The functions

| Call | Effect |
|---|---|
| `level.map_add_object_spot(id, type, hint)` | adds a spot (9.1); when `hint` is not empty it becomes the spot's hint |
| `level.map_add_object_spot_ser(id, type, hint)` | the same, and marks the spot saved: only marked spots are in `sav` 9 part 7 |
| `level.map_remove_object_spot(id, type)` | removes one count |
| `level.map_has_object_spot(id, type)` | the spot's count, 0 when there is none (a number, not a boolean) |
| `level.map_change_spot_hint(id, type, hint)` | sets the hint of an existing spot; nothing when there is none |

[P-20]. So a spot added *k* times needs *k* removals; `level_tasks.script` removes in a loop up to the count, and
tests the count with `~= 0` [D-6]. A spot made without `_ser` is not saved (the objectives' spots of 4.1 and 5.3 are
marked saved) [P-10, P-20].

## 10. Game difficulty: `level.get_game_difficulty()`, `level.set_game_difficulty(d)` (static; data measured)

The difficulty is one stored whole number, the value of the constants of `game_difficulty` (novice 0, stalker 1,
veteran 2, master 3) [D-5]; the console command `g_game_difficulty` is registered over the same value, with the
names `gd_novice`, `gd_stalker`, `gd_veteran`, `gd_master` for 0 to 3 [P-21]. `get_game_difficulty()` returns it.
`set_game_difficulty(d)` stores `d` and then applies it to the player's object: its immunities are read again from
the section `actor_immunities_<name>` and its hit probability from the key `hit_probability_<name>` of its own
section (believed: its configuration section), `<name>` being the value's name (empty for a value outside 0 … 3) [P-21]; the data has the four sections and
keys in `config\creatures\actor.ltx` (hit probability 0.20, 0.30, 0.40, 0.50) [D-5]. What the two settings do is another spec's
(the player's hits). Whether the console command also applies the value to the player is **unestablished**.

## 11. Tutorials and movie sequences (static; data measured)

### 11.1 `game.start_tutorial(name)` and `game.has_active_tutorial()`

The game has at most one **current sequence**. `game.start_tutorial(name)` [P-22]:

1. If there is a current sequence, it becomes the **previous** one; on the next frame of the game it is ended by a
   step that does not stop its items, and discarded [P-22] (believed: so its current item's stop functions, 11.2,
   are not called).
2. A new sequence is made the current one and started from the root child `name` of
   `config\ui\game_tutorials.xml` (which includes `ui\ui_credits.xml` and `ui\ui_movies.xml`, 34 sequences in all):
   its `item` children, `play_each_item` and `global_wnd` are read, and its first item starts [P-22, D-3]. What a
   sequence and its items do (pause, sound, windows, input) is `hud-scope` 18's.
3. The previous sequence's saved input target is carried over to the new one [P-22].

A name with no such element: the sequence has no item and starting its first item reaches through a missing one
(**believed**: a crash). Every one of the 18 names the scripts pass literally exists [D-4].

`game.has_active_tutorial()` is true while there is a current sequence [P-22]. When a sequence has ended (its last
item stopped), it is discarded on the next frame of the game, and the result becomes false [P-22]. The player's death
starts the sequence `game_over` the same way (believed: the routine that does it is the player's death handling)
[P-22].

### 11.2 `function_on_start`, `function_on_stop` (entry points)

Each `item` of a sequence may have `function_on_start` and `function_on_stop` children, each naming a script
function, **resolved** (D 1.1) when the item is read; a name that does not resolve is never called, with no message
[P-22]. When an item starts, its start functions are called first, in order; when it stops, its stop functions are
called last, in order [P-22]. Each is called with no arguments, results ignored; a failing call is **fatal as
D 1.2** [P-22]. Only children of an `item` are read: a `function_on_stop` directly under a sequence is never called
[P-22, D-3].

Data [D-3]: the items have 2 `function_on_start` and 18 `function_on_stop` children naming 9 functions
(`mob_trader.on_start_tutorial_item_1`, `mob_trader.on_stop_tutorial_item`, `xr_effects.after_credits`,
`xr_effects.end_radar_dream`, `xr_effects.end_x18_dream`, `xr_effects.end_yantar_dream`, `xr_effects.game_credits`,
`xr_effects.on_tutor_gameover_stop`, `xr_effects.presentation_task`), each defined, taking no parameters or
`actor, npc` (then both `nil`). The one function element outside an item, `xr_effects.esc_init_dialod` in
`part_1_pda`, is never called.

## 12. Use by the shipped scripts (measured)

By the scoping spec's scan [P-2]: `give_info_portion` is used by 44 loadable scripts (101 in all),
`disable_info_portion` by 10 (11), `has_info` by 3 (4), `set_tip_text` by 9 (11), `set_tip_text_default` by 2 (2),
`info_add` by 3 (3), `info_clear` by 2 (2); the task members by `task_manager.script` (`CGameTask`,
`SGameTaskObjective`, `give_task`, `load`, `set_title`, `get_objective`, `add_objective`, the objective setters,
`add_complete_func`), `bind_stalker.script` (the `task_state` handler: `get_id`, `get_idx`, `get_objective`,
`get_state`, the `task` constants), `news_manager.script` (`get_title`, `get_description`, `get_objectives_cnt`,
`give_game_news`, `give_talk_message`) and a few others (some only by a shared method name); `set_task_state` by `level_tasks.script`; the map functions by
`bar_dialogs`, `level_tasks`, `se_monster`, `se_stalker`, `sim_statistic`, `smart_terrain`, `sr_mapspot` and
`treasure_manager`; `game.start_tutorial` by 7 (8), `game.has_active_tutorial` by `xr_conditions.script`,
`level.get_game_difficulty` by 3 (3); `level.set_game_difficulty` by none.

## 13. Verification facts

All reproduced by `run_all.py` (`ALL PASS 31`, no failure):

- Scope: 54 items, 8 candidates, 42 used names, 1 absent [P-2]; the three classes' registrations, with 5 members the
  listing lacks [P-3]; 9 namespace functions [P-4].
- Members and their implementations: [P-5] to [P-11], [P-16] to [P-22].
- The task machinery: state change [P-12], evaluation and function calls [P-13], the update [P-14], saving
  [P-15].
- Data: task-file functions 103 / 113 / 4 / 1 elements, 40 distinct functions, all defined [D-1, D-2]; sequences 34,
  item functions 2 + 18, 18 literal sequence names all present [D-3, D-4]; difficulty constants and the player's
  settings [D-5]; call forms [D-6, D-7]; objective map data [D-8, D-9].

## 14. Unestablished

- A `CGameTask()`'s priority before `set_priority` or `load` (3.2); ownership of a task once given (4.1).
- The order of tasks of equal priority (4.1).
- What an unknown texture id gives to `set_icon_name` and to an objective-0 `icon` (3.3, 3.4).
- The rate of the player's scheduled update, so how often the task functions are called (5.1).
- Whether the console command `g_game_difficulty` applies the difficulty to the player (10).
- What a missing sequence name does to `start_tutorial` (believed: a crash, 11.1).
- Everything that depends on A: how numbers and booleans are converted for these members, and what a wrong argument
  type does.
- The meaning, for the PDA and the HUD, of the marks set on the PDA and on map spots (4.1 step 6, 5.1, 5.2 step 6):
  PD-1, PD-7 and `hud-scope` 11.

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries identified above; the
  archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs (`config\` and
  `scripts\`).
- Analysis: static analysis with Ghidra (headless, read-only) of the game library's script registration of
  `CGameTask`, `SGameTaskObjective`, `task`, the `game_object` members of this spec and the `level` and `game`
  functions, of their implementations, of the task loading from the task file, the player's task list (giving,
  sorting, the active task), the objective evaluation and state change, the player's update that runs it, the saving
  and loading of tasks, the news and talk-message paths, the usable part's hint text, the map-spot registry, the
  difficulty setting and the tutorial sequencer; import tables, strings and run-time type information read from the
  file bytes by a throwaway reader; the scoping spec's script scan reused; the listing, the scripts and the
  configuration files read as text. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

1. `add_objective` does not set the copied objective's task: the copy keeps the task given to its constructor (none,
   or another task). Which task's id the task functions then receive (6.2), and what evaluation's time-limit test
   (5.1) and the hidden-spot test (5.3) do for an objective of no task, are not stated.
2. The hidden-spot test of 5.3 also applies to objective 0, whose "previous objective" lies before the list; what it
   reads there is not stated.
