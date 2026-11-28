# Script API W: the action planner

What the action-planner part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition), as scripts see it: world properties and world states (`world_property`, `world_state`), the property
store (`property_storage`), property evaluators (`property_evaluator`, `property_evaluator_const`), actions
(`action_base`), planners (`action_planner`) and actions that are planners (`planner_action`), how a planner
chooses and runs its actions, the constants `stalker_ids`, the character's **motivation manager**
(`motivation_action_manager`), the global `ef_storage`, and the overrides of these classes the engine calls. This is
spec W of the approved scoping specification `docs/behavior/script-api-scope.md` (section 6, order 9); its scope
rule, its section 4.3 row "planner", its section 6 assignment of two candidate strings and its review gaps apply:
this spec owns every name of these classes as the game library registers them, listed or not.

The worked example (section 12) is the camp fire of the beginners' camp on `l01_escape`: the camp scheme
`xr_kamp` of its characters, whose evaluators and actions the scheme adds to each character's motivation manager.
What the engine's own actions of that manager do, and when the engine updates it, belong to the characters'
stalker-planner spec (`docs/behavior/characters-scope.md` CH-17, cited as **CH-17**); this spec gives only what the
example needs of it (12.2).

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type, the 250
  loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md`: deriving, `super`, reading and
  assigning members (4.1, 4.2: a read-only property), how an engine call reaches an override (5.2), results and
  errors of such a call (5.3, the fatal `LUA error: %s`), base-class calls (6.1), argument matching (7.3).
- Spec M, `docs/behavior/script-api-game-object-basics.md`: game-object handles, the kinds of object (2.2, among them
  **stalker**), the routine that returns at once (2.3); `debug_planner` is not registered (1.2).
- Spec B, `docs/behavior/script-api-engine-services.md` 3.3: `time_global()`, the device's milliseconds.
- Spec X, `docs/behavior/script-api-movement-sight-and-memory.md` 12: the class `cef_storage` that `ef_storage()`
  returns.
- Spec T, `docs/behavior/script-api-sound.md` (its conventions: a number **made an integer**).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [W-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only.
- A call is written as scripts write it. A **number** is the runtime's double; a **truth value** is read as Lua reads
  conditions (nil and false are false, anything else true). A number the engine wants as an integer is **made an
  integer** (T's conventions); an **id** is made an unsigned 32-bit integer; a **weight** is made an integer and kept
  as an unsigned 16-bit number (modulo 65,536).
- **Fatal** means the program reports the failure and ends (A 5.3).
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (from game
  data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [W-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-action-planner/run_all.py` (`python3 -I run_all.py`): it reads the binaries' bytes, the evidence
of read-only headless static analysis (produced by `gh.py` beside it, and the full decompilation the
simulation-objects checks produced), the listing, the scoping spec's scan and the shipped scripts and configuration.
All checks pass (`ALL PASS 20`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 176 listed items [W-1]:

| Owner | Items |
|---|---|
| `world_property` | the class; `condition`, `value` (section 2) |
| `world_state` | the class; `add_property`, `remove_property`, `property`, `includes`, `clear` (section 2) |
| `property_storage` | the class; `property`, `set_property` (section 3) |
| `property_evaluator` | the class; the properties `object`, `storage`; `setup`, `evaluate` (section 4) |
| `property_evaluator_const` | the class (section 4) |
| `action_base` | the class; the properties `object`, `storage`; `add_precondition`, `add_effect`, `remove_precondition`, `remove_effect`, `setup`, `initialize`, `execute`, `finalize`, `weight`, `set_weight`, `show` (section 5) |
| `action_planner` | the class; the properties `object`, `storage`; `setup`, `add_action`, `remove_action`, `action`, `add_evaluator`, `remove_evaluator`, `evaluator`, `set_goal_world_state`, `update`, `current_action_id`, `current_action`, `initialized`, `actual`, `clear`, `show` (sections 6, 7) |
| `planner_action` | the class (section 8) |
| `stalker_ids` | the class and its 119 constants (section 9) |
| `game_object` | `motivation_action_manager`, `debug_planner` (section 10) |
| global | `ef_storage`, `cast_action_to_planner`, `cast_planner_to_action` (section 10) |

and further: the overrides of the scoping spec's section 4.3 row "planner" (1.3), and the two candidate strings
the scoping spec assigns, `__lt` and `__eq`, which are registrations: the operators `<` and `==` of `world_property`
and `world_state` (2.1, 2.2) [W-4]. Of this spec's names the loadable scripts use 40; counted by the scoping spec's
upper rule, 110 loadable scripts (153 of all scripts) use them [W-1].

### 1.2 As registered (static)

- `world_property`: one constructor of (number, truth value); `condition`, `value`; the operators `<` and `==`
  [W-4].
- `world_state`: two constructors, `world_state()` and `world_state(s)` (a copy of `s`); `add_property`,
  `remove_property`, `clear`, `includes`, `property`; the operators `<` and `==` [W-4].
- `property_storage`: one constructor without arguments; `set_property`, `property` [W-4].
- `property_evaluator`: three constructors, `()`, `(object)`, `(object, text)`; the read-only properties `object` and
  `storage`; `setup`, `evaluate` [W-4, W-10]. `property_evaluator_const`: derives from `property_evaluator`; one
  constructor of a truth value; no member of its own [W-4].
- `action_base`: three constructors, `()`, `(object)`, `(object, text)`; the read-only properties `object` and
  `storage`; `add_precondition`, `add_effect`, `remove_precondition`, `remove_effect`, `setup`, `initialize`,
  `execute`, `finalize`, `weight`, `set_weight` [W-2].
- `action_planner`: one constructor without arguments; the read-only properties `object` and `storage`; `actual`,
  `setup`, `update`, `add_action`, `remove_action`, `action`, `add_evaluator`, `remove_evaluator`, `evaluator`,
  `current_action_id`, `current_action`, `initialized`, `set_goal_world_state`, `clear` [W-3].
- `planner_action`: derives from both `action_planner` and `action_base`; three constructors, `()`, `(object)`,
  `(object, text)`; registers of its own `setup`, `initialize`, `execute`, `finalize`, `weight` (the action's, section
  8) [W-3].
- `stalker_ids`: the 119 constants of section 9 [W-5].
- `game_object:motivation_action_manager()` and the global `ef_storage()` (section 10) [W-6, W-16].
- The listed `show` (of `action_base` and `action_planner`), `debug_planner`, `cast_action_to_planner` and
  `cast_planner_to_action` are not registered: the registration code has no `show`, and the three other names occur
  in no file under `bin\` [W-2, W-6]. Reading one gives nil (A 4.1), so calling it is a Lua error. The one loadable
  script naming `debug_planner` (`state_mgr_goap.script`) calls it only when `npc.debug_planner ~= nil`, which is
  never true [W-6] (measured, the script).
- No class of this spec has a property the listing lacks (the scoping spec's gap 2): the only properties registered
  are the read-only `object` and `storage` above [W-2, W-3, W-4].

### 1.3 Overrides (static)

A script class derived from these classes (A 1.3) can override, for the engine, exactly these methods; each is
called as A 5.2 calls an override, an error in it is fatal (A 5.3), and a base call `Class.m(self, …)` runs the
default given in the section named (A 6.1) [W-10, W-11, W-12, W-15]:

| Base | Overrides the engine calls |
|---|---|
| `property_evaluator` | `setup(object, storage)`, `evaluate()` (section 4) |
| `action_base` | `setup(object, storage)`, `initialize()`, `execute()`, `finalize()`, `weight(s1, s2)` (section 5) |
| `action_planner` | `setup(object)`, `update()` (section 6) |
| `planner_action` | `setup(object, storage)`, `initialize()`, `execute()`, `finalize()`, `weight(s1, s2)` (section 8); not `update` or `setup(object)` of its planner part |

A `property_evaluator_const` is not built for a script class: it has no engine call into a script (believed: its
engine object keeps no reference to a script object); no shipped script derives from it (measured, the scripts). The
overridden name `reinit` of the scoping spec's 4.3 (a class derived from `action_planner` in the non-loadable
`stalker_test.script`) is not registered, so the engine never calls it and the base call `action_planner.reinit(self,
obj)` there is a call of nil [W-3, W-19]. The shipped scripts declare 61 classes deriving from `action_base` and 103
from `property_evaluator` (declarations counted in all scripts); no loadable script overrides `weight` (only the
non-loadable `stalker_test.script` does) [W-19].

## 2. World properties and world states (static)

### 2.1 `world_property(condition, value)`

A **world property** is a pair: a **condition** (an id) and a **value** (a truth value) [W-7].

- `world_property(c, v)`: the condition `c` is made an id; the value is the truth of `v` [W-7].
- `p:condition()` returns the condition, `p:value()` the value [W-7].
- `p == q`: true when both condition and value are equal. `p < q`: by condition, then by value (false before true)
  [W-7].
- Each property has a **hash**, a 32-bit number computed from it, used by states (2.2) and by planning (7.4) [W-7]:
  with every product and sum taken modulo 2^32, let *r*(*x*) = *x* · 134,775,813 + 1 and *s*(*x*) = the upper 32 bits
  of the 64-bit product *x* · 4,294,967,295; then *a* = *s*(*r*(*c* + 1)) and the hash is *a* XOR *s*(*r*(*v* + *a*)),
  where *v* is 1 for true and 0 for false. For example the property (4, true) has the hash 103,320,731 and (2, true)
  2,637,066,335 [W-7].

### 2.2 `world_state`

A **world state** is a list of world properties kept sorted by condition, then value, with a hash: the XOR of its
properties' hashes (0 when empty) [W-8].

- `world_state()` is empty; `world_state(s)` a copy of `s` [W-4, W-8].
- `s:add_property(p)` inserts a copy of `p` at its sorted place and XORs its hash into the state's. It does not test
  whether the condition is already there: adding (5, false) and then (5, true) keeps both [W-8].
- `s:remove_property(c)` (c made an id) removes the first property not sorted below (c, false), that is the first
  property whose condition is at least c, and XORs its hash out — without testing that its condition is c: if there
  is none with condition c, it removes the next one; if none follows, the list is read past its end (**believed**:
  the state is corrupted or the program crashes) [W-8].
- `s:property(c)` returns that same property (the first whose condition is at least c), which may have another
  condition, or nil when there is none [W-8].
- `s:includes(t)` is true when every property of `t` is in `s` with the same condition and value [W-8].
- `s:clear()` empties the list and sets the hash to 0 [W-8].
- `s == t` compares only the two hashes: different states with equal hashes compare equal [W-8].
- `s < t` compares the lists property by property (2.1's order); a list that is a proper beginning of the other is the
  smaller [W-8].

## 3. `property_storage` (static)

A **property store** is a list of (id, truth value) pairs in the order added [W-9].

- `property_storage()` is empty [W-9].
- `st:set_property(id, v)`: if the id is in the list its value becomes `v`, otherwise (id, `v`) is appended [W-9].
- `st:property(id)` returns the value stored for the id; for an id not in the list the value read is past the end of
  the list (**believed**: an arbitrary truth value) [W-9].

Each planner has its own store (6.1), handed to its actions and evaluators by `setup` (4, 5); no shipped loadable
script reads or writes a store (the scripts keep their own tables) [W-1].

## 4. Property evaluators (static)

An **evaluator** gives the current value of one condition. It has an **object** and a **store**, both none until it
is set up [W-10].

- `property_evaluator()`, `property_evaluator(object)`, `property_evaluator(object, name)`: the object is the
  argument (none for nil or the first form); the name is read and not kept [W-10]. A script class calls one of them
  through `super` (`xr_kamp` passes `super(nil, name)`, so its evaluators have no object until set up) [W-19].
- `e.object`, `e.storage`: the object (a game-object handle) and the store; read-only (A 4.2) [W-4].
- `e:setup(object, storage)`: the default sets both [W-10]. The planner calls the method when the evaluator is added
  (6.2) with the planner's object and store.
- `e:evaluate()`: the default returns false [W-10]. An override's result is read as a truth value [W-10].
- `property_evaluator_const(v)`: an evaluator whose `evaluate` always returns the truth of `v`; its `setup` sets object
  and store as above [W-10].

## 5. Actions: `action_base` (static)

An **action** has an object, a store, **preconditions** and **effects** (two world states, 2.2, initially empty), a
**weight** (an unsigned 16-bit number, initially 1), a **start time** and a **first-execution flag** [W-11].

- `action_base()`, `action_base(object)`, `action_base(object, name)`: the object is the argument (none for nil or the
  first form); the name is read and not kept; no store [W-11].
- `a.object`, `a.storage`: read-only (A 4.2) [W-2].
- `a:add_precondition(p)`, `a:add_effect(p)`: `add_property` (2.2) on the preconditions or the effects [W-11].
  `a:remove_precondition(c)`, `a:remove_effect(c)`: `remove_property` (2.2), with its lack of a test [W-11].
- `a:setup(object, storage)`: the default sets both [W-11]. The planner calls the method when the action is added
  (6.2).
- `a:initialize()`: the default sets the start time to the device's `time_global()` (B 3.3) and the first-execution
  flag [W-11].
- `a:execute()`: the default clears the first-execution flag [W-11]. `a:finalize()`: the default does nothing [W-11].
  No member of this spec reads the start time or the flag.
- `a:weight(s1, s2)`: the default returns the weight [W-11]. The planner calls the method with two states (7.3); an
  override's result is made a weight [W-11].
- `a:set_weight(w)`: the weight becomes `w` made a weight [W-11].

The engine also keeps, for every action, a **minimum weight**: the default `weight` raises the weight to it, `set_weight`
never sets less, and when an override's result is below it the engine hands the text `Weight is less than effect
count! It is corrected from %d to %d` to the routine that returns at once (M 2.3) and uses the minimum instead [W-11].
When computed, the minimum is the number of effects that are not also preconditions with the same value [W-11].
For the actions of this spec the minimum is 0 and never changes: it starts as computed with the value 0, and it is
recomputed only after the action has been linked to a flag that adding a condition clears; the link is made by no
member of this spec and by no direct call in the game library (**believed**: by nothing) [W-11]. So these rules
never change a weight.

## 6. Planners: `action_planner` (static)

### 6.1 Parts of a planner

A **planner** has [W-12]:

- an **object** and a **store** (3);
- its **evaluators** and **actions**, each kept by id in increasing order. Adding an evaluator under an id that is
  there already replaces the old evaluator, which is then no longer used and is not deleted [W-12]; adding an action
  under an id already there is **unestablished**;
- a **goal** (a world state);
- the **cached world values** of the last planning (7.2) and the **plan**: the ids of the actions found by it, in
  order (7.3);
- a **current action id**, an **initialized** flag and an **actual** flag.

`action_planner()` makes a planner with none of these set up: the initialized flag is false, while the actual flag,
the object and the current action id are left unset until `setup` [W-12]. A script must call `setup` first.

### 6.2 Members

- `pl:setup(object)` (an override's default, A 6.1): empties the goal, the cached world values and the plan; the
  actual flag becomes true; the current action id becomes **none** (the unsigned 32-bit value 4,294,967,295); the
  object becomes `object`; the store is emptied; initialized becomes false [W-12]. It does not set up the actions or evaluators already added.
- `pl:add_action(id, a)`: adds `a` under the id (made an id); the engine takes over the script object (it is no longer
  collected by the script side, as an adopted object); actual becomes false; then `a:setup(pl.object, pl.storage)` is
  called (section 5) [W-12].
- `pl:add_evaluator(id, e)`: adds `e` under the id; then `e:setup(pl.object, pl.storage)` (section 4); actual is left
  as it was [W-12]. That the engine also takes over the evaluator is **believed** (the planner deletes it on removal).
- `pl:remove_action(id)`, `pl:remove_evaluator(id)`: deletes the action or evaluator and removes it; actual becomes
  false [W-12]. The entry removed is the first whose id is not below `id` (no test that it is `id`; with none, the
  list is read past its end) [W-12]. **Disputed in review:** this holds for `remove_action` only;
  `remove_evaluator` removes the evaluator with exactly that id, and with none it acts on the position past the end
  of the evaluator list (static).
- `pl:action(id)`: the action of the first entry whose id is not below `id` (no test; past the end when none)
  [W-12]. `pl:evaluator(id)`: the evaluator with exactly that id; with none, a value read past the end of the list
  (**believed**: not an evaluator) [W-12]. Both return the engine's object, also for actions and evaluators the
  engine added itself (as the motivation manager's, 12.2), so a script can add conditions to them.
- `pl:set_goal_world_state(s)`: the goal becomes a copy of `s`; actual stays true only if it was true and `s` equals
  the old goal (the same hash and the same properties in the same order) [W-12].
- `pl:current_action_id()`: the current action id (none before the first `update` after `setup`; **believed** read by
  scripts as the number 4,294,967,295) [W-12]. `pl:current_action()`: `pl:action(current id)` [W-12].
  `pl:initialized()`: the initialized flag [W-12].
- `pl:actual()`: the **actuality test** of 6.3 step 1 (it calls evaluators) [W-12].
- `pl:clear()`: deletes and removes all actions, then all evaluators; nothing else changes [W-12]. **Disputed in
  review:** each removal is done as `remove_action` and `remove_evaluator` do it, so when there is anything to remove
  the actual flag becomes false (static).
- `pl.object`, `pl.storage`: read-only (A 4.2) [W-3].
- `pl:update()`: section 6.3 (an override's default).

### 6.3 `update()` (static)

Each call does, in order [W-13]:

1. **Actuality test.** The planner is actual when its actual flag is true and every cached world value (7.2) still
   holds: in increasing condition order, the evaluator of each cached condition is called and its result compared
   with the cached value; the test stops at the first difference.
2. If it is not actual, the planner **plans** (section 7): the actual flag becomes true, the cached world values
   are emptied, and the search runs from the current world to the goal. If the search finds a plan, it replaces the
   plan; if it fails, the previous plan is kept (and the planner still counts as actual until something changes).
3. If initialized is false, it becomes true, the current action id becomes the plan's first id and that action's
   `initialize` is called. Otherwise, if the current action id differs from the plan's first id, the current action's
   `finalize` is called, then the current action id becomes the plan's first id and that action's `initialize` is
   called; if it is the same, neither is called.
4. The current action's `execute` is called.

So each update runs exactly one action, the first of the plan; the others are only the reason it was chosen. An
action is initialized when it becomes the first step and finalized when another becomes the first step. If the plan
is empty (the goal held in the current world) or no plan was ever found, the first id is read without a test and
is not an id of any plan (**unestablished** what follows; **believed** an arbitrary action, or a crash) [W-13].

## 7. Planning (static)

### 7.1 States relative to the world

Planning searches over world states that hold only the conditions whose values differ from the current world: a
**search state** gives a value for some conditions, and every other condition has its **world value** (7.2). A
condition's value in a search state is the state's own value if it has one, else the world value [W-14].

- The **start state** is empty (the current world) [W-14].
- A state **satisfies** a world state *w* (the goal, or an action's preconditions) when, for every property of *w*,
  the condition's value in the state equals the property's value; the test goes through *w*'s properties in
  increasing condition order and stops at the first that fails [W-14].
- **Applying** an action to a state gives the state whose values are the state's, overwritten by the action's effects,
  with every condition whose resulting value equals its world value left out [W-14].
- Two search states are the same when they hold the same properties (the same hash and the same sorted list,
  6.2's comparison) [W-14].

### 7.2 World values

The world value of a condition is computed only when planning first needs it, by calling the `evaluate` of the
planner's evaluator with that id, and is then cached for the rest of this planning; the cache is what the actuality
test of 6.3 checks at later updates [W-14]. Conditions are evaluated in the order the search needs them (goal tests,
then preconditions and effects as actions are tried), each at most once per planning. A condition with no evaluator
in the planner reads a value past the end of its evaluator list (**believed**: a crash) [W-14].

### 7.3 The search

The search is a best-first search over search states. Each state reached has a cost *g*, an estimate *h*, *f* = *g* +
*h* (all unsigned 16-bit numbers, sums modulo 65,536), a predecessor and the id of the action that reached it, and is
**open** or **closed** [W-14]:

- *h* of a state is the number of goal properties the state does not itself hold with the goal's value (a goal
  property that holds only through its world value counts) [W-14].
- The start state has *g* = 0 and is the first open state [W-14].
- Repeatedly, while there are open states and the open state with the smallest *f* (7.4) has *f* below 65,535, fewer
  than 4,294,967,295 states have been expanded, and fewer than 8,000 states have been reached in this search
  (the start included) [W-13, W-14]:
  1. That state is taken from the open states. If it satisfies the goal, the search ends with success.
  2. Otherwise it is closed, and every action of the planner is tried in increasing id order: if the state satisfies
     the action's preconditions, the action is applied (7.1) and the action's `weight` is called with (the new state,
     the state being expanded); the cost of the step is its result.
  3. A new state not reached before gets *g* = (the expanded state's *g*) + cost, its *h*, the expanded state as
     predecessor and the action's id, and becomes open. A state reached before that is still open and now gets a
     smaller *g* takes that *g*, the new predecessor and id, and its new *f*. A closed state is left as it is.
- When the loop ends without success the search **fails** [W-14].
- On success the **plan** is the action ids along the predecessors from the start state to the state that
  satisfied the goal, first step first; it is empty when the start state satisfied the goal [W-14].

The two states handed to `weight` are search states (only the conditions that differ from the world); the order and
lifetime of the script objects they arrive as are **unestablished** beyond that [W-14].

### 7.4 Order of the open states

Which of several open states with the same *f* is taken first depends on the open states' order, which is an
array-based binary heap on *f* (each parent's *f* not above its children's) with these rules [W-14]:

- **Adding** a state *v*: if there is a first element and its *f* is at least *v*'s, that element is moved to the end of
  the array and *v* put first; otherwise *v* is put at the end. Then the element at the end moves up: while it has a
  parent whose *f* is above its own, it changes places with the parent.
- **Taking** the first element: the last element *x* is set aside and the first is removed. Starting from the
  emptied first place, the place is repeatedly filled from one of its two children (at 2*i* + 1 and 2*i* + 2): the
  left one if its *f* is below the right one's, else the right one, for as long as both children exist; if the place
  then has only a left child, it is filled from that child. Then *x* is put in the place reached and moves up as in
  adding.
- **A smaller *f*** for an open state: the state moves up from its place as in adding.

### 7.5 Cost of planning

Every planning evaluates evaluators (7.2) and calls `weight` once per step tried; the actuality test at each update
evaluates the cached conditions again. How often the engine updates the motivation manager is CH-17's.

## 8. Actions that are planners: `planner_action` (static)

A `planner_action` is an action (section 5) that holds a planner (sections 6, 7): its planner members (`add_action`,
`add_evaluator`, `action`, `current_action_id`, …) act on that inner planner [W-15]. Its own members:

- `setup(object, storage)` (the action's, called when the planner action is added to a planner; an override's default):
  sets the inner planner up with the object (6.2 `setup`), sets the action's object and store, and sets the inner
  planner's goal to the action's own effects as they are at that moment (as `set_goal_world_state`) [W-15].
- `initialize()`: as `action_base`'s default [W-15].
- `execute()`: clears the first-execution flag, then updates the inner planner (6.3) [W-15].
- `finalize()`: calls `finalize` of the inner planner's current action and sets the inner initialized flag to false,
  so the next `execute` initializes the inner plan's first step without finalizing anything [W-15].
- `weight(s1, s2)`: as `action_base`'s default [W-15].

No loadable script uses `planner_action` (one commented-out use in `xr_combat.script`; measured, the scripts).

## 9. `stalker_ids` (static)

The class holds 119 integer constants: 58 action and planner ids, 37 condition ids and 24 sound kinds [W-5]. They are
the ids the engine uses in the characters' motivation manager (CH-17); the ids `action_script` (61) and
`property_script` (44) are the bases from which the scripts number their own actions and conditions (12.1).

Actions and planners [W-5]:

| Value | Name | Value | Name | Value | Name |
|---|---|---|---|---|---|
| 0 | `action_dead` | 20 | `action_retreat_from_enemy` | 43 | `action_danger_unknown_take_cover` |
| 1 | `action_dying` | 21 | `action_take_cover` | 44 | `action_danger_unknown_look_around` |
| 2 | `action_gather_items` | 22 | `action_look_out` | 45 | `action_danger_unknown_search` |
| 3 | `action_no_alife` | 23 | `action_hold_position` | 46 | `action_danger_in_direction_take_cover` |
| 4 | `action_smart_terrain_task` | 24 | `action_get_distance` | 47 | `action_danger_in_direction_look_out` |
| 5 | `action_solve_zone_puzzle` | 25 | `action_detour_enemy` | 48 | `action_danger_in_direction_hold_position` |
| 6 | `action_reach_task_location` | 26 | `action_search_enemy` | 49 | `action_danger_in_direction_detour` |
| 7 | `action_accomplish_task` | 28 | `action_sudden_attack` | 50 | `action_danger_in_direction_search` |
| 8 | `action_reach_customer_location` | 29 | `action_kill_enemy_if_not_visible` | 51 | `action_danger_grenade_take_cover` |
| 9 | `action_communicate_with_customer` | 30 | `action_reach_wounded_enemy` | 52 | `action_danger_grenade_wait_for_explosion` |
| 10 | `get_out_of_anomaly` | 32 | `action_prepare_wounded_enemy` | 53 | `action_danger_grenade_take_cover_after_explosion` |
| 11 | `detect_anomaly` | 33 | `action_kill_wounded_enemy` | 54 | `action_danger_grenade_look_around` |
| 12 | `action_get_item_to_kill` | 34 | `action_post_combat_wait` | 55 | `action_danger_grenade_search` |
| 13 | `action_find_item_to_kill` | 35 | `action_kill_if_player_on_the_path` | 56 | `action_death_planner` |
| 14 | `action_make_item_killing` | 36 | `action_critically_wounded` | 57 | `action_alife_planner` |
| 15 | `action_find_ammo` | 37 | `action_kill_if_enemy_critically_wounded` | 58 | `action_combat_planner` |
| 16 | `action_aim_enemy` | 39 | `action_danger_unknown_planner` | 59 | `action_anomaly_planner` |
| 17 | `action_get_ready_to_kill` | 40 | `action_danger_in_direction_planner` | 60 | `action_danger_planner` |
| 19 | `action_kill_enemy` | 41 | `action_danger_grenade_planner` | 61 | `action_script` |
| | | 42 | `action_danger_by_sound_planner` | | |

Conditions (properties) [W-5]:

| Value | Name | Value | Name | Value | Name |
|---|---|---|---|---|---|
| 0 | `property_alive` | 13 | `property_ready_to_kill` | 29 | `property_critically_wounded` |
| 1 | `property_dead` | 14 | `property_ready_to_detour` | 30 | `property_enemy_critically_wounded` |
| 2 | `property_already_dead` | 15 | `property_see_enemy` | 34 | `property_danger_unknown` |
| 3 | `property_alife` | 17 | `property_panic` | 35 | `property_danger_in_direction` |
| 4 | `property_puzzle_solved` | 18 | `property_in_cover` | 36 | `property_danger_grenade` |
| 5 | `property_smart_terrain_task` | 19 | `property_looked_out` | 37 | `property_danger_by_sound` |
| 6 | `property_items` | 20 | `property_position_holded` | 38 | `property_cover_actual` |
| 7 | `property_enemy` | 21 | `property_enemy_detoured` | 39 | `property_cover_reached` |
| 8 | `property_danger` | 22 | `property_use_suddenness` | 40 | `property_looked_around` |
| 9 | `property_item_to_kill` | 23 | `property_pure_enemy` | 41 | `property_grenade_exploded` |
| 10 | `property_found_item_to_kill` | 24 | `property_use_crouch_to_look_out` | 42 | `property_anomaly` |
| 11 | `property_item_can_kill` | | | 43 | `property_inside_anomaly` |
| 12 | `property_found_ammo` | | | 44 | `property_script` |

Sound kinds [W-5]: `sound_die` 0, `sound_die_in_anomaly` 1, `sound_injuring` 2, `sound_humming` 3, `sound_alarm` 4,
`sound_attack_no_allies` 5, `sound_attack_allies_single_enemy` 6, `sound_attack_allies_several_enemies` 7,
`sound_backup` 8, `sound_detour` 9, `sound_search1_with_allies` 10, `sound_search1_no_allies` 11,
`sound_injuring_by_friend` 12, `sound_panic_human` 13, `sound_panic_monster` 14, `sound_tolls` 15,
`sound_grenade_alarm` 16, `sound_friendly_grenade_alarm` 17, `sound_need_backup` 18, `sound_running_in_danger` 19,
`sound_kill_wounded` 20, `sound_enemy_critically_wounded` 21, `sound_script` 23, and
`sound_enemy_killed_or_wounded`, registered with the 32-bit pattern 3,489,665,024 (hexadecimal D0001000) [W-5];
whether scripts see it as that number or as the signed −805,302,272 is **unestablished** (**believed** the signed
value: constants are registered as signed integers). What the sound kinds select belongs to the characters' sound
spec; no loadable script uses `sound_enemy_killed_or_wounded` [W-1].

## 10. `motivation_action_manager`, `ef_storage` (static)

- `npc:motivation_action_manager()`: for an object of the kind **stalker** (M 2.2), the character's **motivation
  manager**, the planner the engine made for it (an `action_planner`: every member of section 6 applies, its update
  being 6.3; when the engine itself updates it, it first sets the goal, 12.2) [W-16, W-18]. For any other object the text `CAI_Stalker :
  cannot access class member action_planner!` is handed to the routine that returns at once (M 2.3), and the result
  is then read through the missing stalker part (**believed**: the program crashes) [W-16]. The same planner is
  returned on every call; the engine owns it. 18 loadable scripts call the member (measured, scoping scan [W-1]).
- `ef_storage()`: the store of evaluation functions (spec X 12's `cef_storage`); the engine's AI services are created
  on the first call if not yet made (the same as on their first use elsewhere) [W-6]. No shipped script calls it.
- `debug_planner`, `cast_action_to_planner`, `cast_planner_to_action`: not registered (1.2).

## 11. Saving a planner (static)

A planner saves, into the save packet it is given: each evaluator's own data in id order, each action's own data in
id order, then the store as a 32-bit count and, for each pair in order, the id (32 bits) and the value (8 bits); loading
reads the same and sets each pair as `set_property` does [W-17]. The evaluators and actions of this spec save nothing
of their own, so for them only the store is saved [W-17]. Who saves the motivation manager, and when, belongs to
CH-17 (**unestablished** here).

## 12. Worked example: the camp fire of the beginners' camp, `l01_escape`

### 12.1 What the camp scheme adds (measured, the scripts and configuration [W-19])

The beginners' camp's group (`gulag_escape.script`, type `esc_lager`) has six jobs with the logic section
`logic@esc_lager_kamp1` (`config\misc\gulag_escape.ltx`), whose active scheme is `kamp1@esc_lager`: the camp
scheme `xr_kamp` with `center_point = camp_center`, read with the group's name as `esc_lager_camp_center` [W-19].
The first time the scheme starts for a character, `xr_logic` calls `xr_kamp.add_to_binder`, which takes the
character's motivation manager and adds [W-19]:

| Id | Kind | Script class | What it does |
|---|---|---|---|
| 1065 (`property_script` + 1021) | evaluator `kamp_end` | `evaluator_kamp_end` | true when the camp scheme is not the character's active scheme |
| 1066 (`property_script` + 1022) | evaluator `on_position` | `evaluator_on_position` | true when the character's level vertex is the camp place chosen for it |
| 204 (`action_script` + 143) | action `wait` | `action_wait` | sits at the fire; preconditions alive, no danger, no enemy, no anomaly, the four common preconditions, `on_position` true; effect `kamp_end` true |
| 202 (`action_script` + 141) | action `go_position` | `action_go_position` | walks to the camp place; the same preconditions with `on_position` false; effect `on_position` true |

and it gives the engine's action 57 (`action_alife_planner`, the manager's own, 12.2) the further precondition
`kamp_end` true. The four common preconditions (`xr_motivator.addCommonPrecondition`) are the conditions 1075, 2054,
3144 and 1164 false, conditions of other schemes (meeting, wounded, helicopter hunting, abuse) [W-19]. The script
classes construct their engine parts with `super(nil, name)` (no object until `setup`), and their `initialize`,
`execute` and `finalize` begin with the base call (`action_base.initialize(self)` and so on), so the defaults of
section 5 run first [W-19]. The motivator also replaces the engine's evaluator of `property_anomaly` (42) with
`property_evaluator_const(false)` when a living character spawns: `remove_evaluator` then `add_evaluator` [W-19].

### 12.2 What the example needs of the engine (static, CH-17's for the full rules)

When the engine updates a character's motivation manager it first sets the goal (with `set_goal_world_state`'s rule
for actuality, then runs 6.3): `property_puzzle_solved` (4) true, or
`property_already_dead` (2) true when a value the engine reads from the character is at most 0 (**believed**: its
health) [W-18]. The engine's own actions of the manager, with their conditions [W-18]:

| Id | Preconditions | Effect |
|---|---|---|
| 2 `action_gather_items` | alive, ¬enemy, ¬anomaly, ¬danger, items | ¬items |
| 56 `action_death_planner` | ¬alive, ¬already_dead | already_dead |
| 57 `action_alife_planner` | alive, ¬enemy, ¬anomaly, ¬danger, ¬items, ¬puzzle_solved | puzzle_solved |
| 58 `action_combat_planner` | alive, ¬anomaly, enemy | ¬enemy |
| 59 `action_anomaly_planner` | alive, anomaly | ¬anomaly |
| 60 `action_danger_planner` | alive, ¬enemy, ¬anomaly, danger | ¬danger |

(alive is condition 0, already_dead 2, puzzle_solved 4, items 6, enemy 7, danger 8, anomaly 42.) What these engine
actions and evaluators do is CH-17's.

### 12.3 Planning at the fire [W-20]

Suppose a living character of the camp with no enemy, danger, anomaly or items to gather, the four common
preconditions false, puzzle_solved false, and the actions of other schemes not applicable (they add more actions;
this example leaves them out). Weights are all 1 (no override, 5).

1. **Away from its camp place** (`on_position` false, `kamp_end` false while the camp scheme is active). The start
   state is empty; goal {4: true}; *h* = 1. Expanding it: action 57 fails its new precondition `kamp_end`; 202 applies
   and gives {1066: true} (*g* 1, *f* 2). Expanding that: 204 applies and gives {1065: true, 1066: true} (*g* 2,
   *f* 3). Expanding that: 57 applies and gives {4: true, 1065: true, 1066: true} (*g* 3, *h* 0), which satisfies the
   goal when taken. The plan is 202, 204, 57: the update initializes and executes `go_position`, whose `execute` sends
   the character to the camp place. The planning cached the world values of conditions 0, 4, 6, 7, 8, 42, 1065, 1066
   and the four common ones.
2. **At the camp place**: `evaluator_on_position` now gives true, so the next update's actuality test fails at 1066
   and the manager plans again: 202 no longer applies, 204 does; the plan is 204, 57. The update finalizes
   `go_position`, initializes `wait` and executes it; later updates find the planner actual and execute `wait` again.
   `wait`'s effect (`kamp_end` true) never comes about while the scheme is active: it exists only to give the planner
   a route to the goal through 57, so the character keeps sitting.
3. **The scheme ends** (the logic switches the character to another section): `kamp_end` evaluates true, the plan
   becomes 57 alone, and the update finalizes `wait` and initializes the engine's 57.
4. **An enemy appears** while away from the camp place: 57, 202 and 204 need no enemy, so the plan is 58, 202, 204,
   57 and the combat planner runs first.

In each of these situations there is only one route to the goal, so the order of 7.4 does not change the result.

## 13. Verification facts

All checks pass (`ALL PASS 20`): the binary identified, the 176 items and usage counts [W-1]; the registrations
[W-2 … W-4]; the 119 constants [W-5]; the absent names and `ef_storage` [W-6]; the state and property rules
[W-7 … W-9]; evaluators, actions, planners, update, search and planner actions [W-10 … W-15]; the motivation manager
[W-16, W-18]; saving [W-17]; the example's data and plans, the plans computed by the check with the rules of
section 7 [W-19, W-20].

## 14. Unestablished

- Adding an action or evaluator under an id that is already in the planner. **Disputed in review:** for an
  evaluator this is established in 6.1 (the old one is replaced); only the action case is open here.
- What `update` does with an empty plan or when no plan was ever found (6.3); what reading a missing evaluator,
  action, store entry or state entry gives (2.2, 3, 6.2, 7.2).
- Whether the engine takes over an added evaluator (believed, 6.2), and the lifetime of the states handed to
  `weight` (7.3).
- Whether `sound_enemy_killed_or_wounded` reads as signed or unsigned (9).
- Who saves the motivation manager and when (11); when the engine updates it, how it sets its goal in full, and
  what its own actions and evaluators do (CH-17).
- The value the engine reads to choose the motivation manager's goal (believed health, 12.2).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` as identified above; the
  shipped scripts (`scripts\*.script`, among them `lua_help.script`, `xr_kamp.script`, `xr_motivator.script`,
  `xr_evaluators_id.script`, `xr_actions_id.script`, `gulag_escape.script`) and configuration
  (`config\misc\gulag_escape.ltx`), extracted from `gamedata.db0` … `gamedata.dbd` with the project's archive tool;
  the scoping spec's scan.
- Analysis: static analysis of `xrGame.dll` with Ghidra in headless read-only mode (decompilation and disassembly of
  the registration code of these classes, their members, the planner's update and search, the script call-outs, the
  registration of the `stalker_ids` constants and of the motivation manager's own actions); string searches of the
  files under `bin\`; reading the shipped scripts and configuration; a small model of section 7 run on the example.
  No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- Adding an action under an id already in the planner is established statically: the new entry is inserted before
  the existing one and both are kept, so lookups and removals by that id find the new one first.
- Loading a planner (11) also sets a flag of the planner that `setup` clears; what reads that flag is not given.
