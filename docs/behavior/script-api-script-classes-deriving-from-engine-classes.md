# Script API A: script classes deriving from engine classes

How the shipped scripts of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) declare classes with `class`,
construct objects through `__init` and `super`, derive script classes from engine classes, how calls the engine
makes reach the methods such a class defines (overrides), and how a method calls the engine class's own
implementation (base-class calls). Spec A of `docs/behavior/script-api-scope.md` (section 6, order 1); the
entry points it owns are those of that document's section 4.5, row A.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`: the runtime (LuaJIT 1.0.3), the binding library exported by
  `xrLUA.dll` (its section 1.2), the game state.
- `docs/behavior/script-loading.md`: script names, namespaces, the chunk prefix that gives every script other
  than `_G` a local table `this` as its environment (its section 2.3), automatic loading, startup, and the
  250 loadable scripts (its section 3).
- `docs/behavior/script-api-scope.md`: the listing `scripts\lua_help.script`, the counting rules and the
  split. What each engine class does, which of its methods the engine calls and when, belongs to the spec owning
  that class (scope section 4.5); this spec defines only the mechanism common to all of them.

Conventions:

- Binaries: `xrLUA.dll` (290,816 bytes, MD5 `a65ba53867962afc92e0f17409e9cd1f`) and `xrGame.dll` (6,246,400
  bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), as identified in the script environment specification.
- Scripts: the 439 scanned scripts of the scoping spec (440 `.script` files of `scripts\` without
  `lua_help.script`), as the archive precedence gives them, tokenized after the comment removal of
  script-loading.md section 1.5. **Loadable** means one of the 250 of script-loading.md section 3.
- "Engine class" means a class the binding library registers from `xrGame.dll` (named as the listing and the
  scripts spell it); "script class" a class a script declares with `class`; "script object" an object made by
  calling a script class. "The globals" is the game state's global table.
- Each fact is **measured** (from the scripts or file bytes), **static** (from static analysis of the binaries,
  not observed at run time), **believed** (inferred, with the reason) or **unestablished**. No binary was run.
- Checks are Python scripts in the dirty workspace directory `checks/script-classes/`, named in brackets.
  `run_all.py` runs them all (85 PASS lines, output in `run_all.out`).

## 1. Declaring a class

### 1.1 The global `class` (static, [bin_lua.py])

Opening the binding library for the game state (before `_G` is loaded, script-loading.md section 2.2) stores a
C function in the globals under the name `class`. It is the only way scripts create classes.

`class "name"` (the function called with one string) creates a new, empty script class and stores it as
follows:

- If the function that executes the `class` call has an active local variable named `this` (the first local
  of that name, in declaration order) and that local holds a table, the class is stored in that table under
  `name`.
- Otherwise it is stored in the globals under `name`.

The store is an ordinary table assignment. The call returns a function: calling it with one argument (written
`class "name" (Base)`) makes `Base` the class's base (1.3). Not calling it leaves the class without a base.

A name that is not a string goes through Lua's string conversion (a number becomes its text); for any other
value the name is missing, and storing then fails with Lua's error for a nil table key (believed: the store is
an ordinary assignment with a nil key). Every shipped declaration passes a string literal (measured,
[data_classes.py]).

Consequences, with script-loading.md section 2.3 (believed, derived from the two specifications):

- In every script other than `_G`, the chunk prefix declares `local this = {}`, and a declaration at the top
  level of the file (outside any function body) stores the class in the script's namespace. Other scripts reach
  it as `namespace.name`; the script itself reaches it by its bare name, through its environment.
- In `scripts\_g.script` there is no `this` local, so its classes are global.
- A declaration inside a function body never sees the chunk's `this` as a local (it is an upvalue there), so
  the class would be global.
- Two scripts may declare classes of the same name without conflict; a declaration replaces an earlier value of
  the same name in the same table.

### 1.2 Declarations in the shipped scripts (measured, [data_classes.py])

| Fact | All 439 scripts | Loadable |
|---|---|---|
| declarations `class "name" ...` | 380 (355 distinct names) in 190 scripts | 278 (270 names) in 134 scripts |
| with a base `class "name" (Base)` | 286 | 200 |
| without a base `class "name"` | 94 | 78 |
| inside a function body | 0 | 0 |

All declarations are at the top level of their script's main chunk; 5 (all in `_g.script`) are inside an `if`
block of it. `_g.script` declares 7 classes (`FSM`, `script_object`, `pp_effector`, `pp_linear_lerp`,
`const_evaluator`, `wait_evaluator`, `enabled_evaluator`) and has no local named `this`; another script uses
`script_object` once by its bare name. 17 bases are written `namespace.Class` (for example
`class "options_controls_tab" (ui_options_base_tab.options_base_tab)`), each naming a class that the script of
that namespace declares; 1 of them is in a loadable script. Reading `namespace.Class` loads that script if it is
not loaded yet (automatic loading, script-loading.md 2.2), so its whole main chunk, including the base's
methods, has run before the derived class is declared.

### 1.3 Deriving: `class "name" (Base)` (static, [bin_lua.py])

`Base` must be a class object (a script class or an engine class). The argument is not checked: any other
value, nil included, is used as if it were a class, with no error message; the outcome is unestablished
(believed to crash the game, since nothing about the value is valid).

Deriving **copies** from the base, at that moment:

- every member visible on the base's objects (methods, and for an engine class its engine methods), except
  `__init` and `__finalize`;
- every member visible as `Base.name` (4.4), except `__init` and `__finalize`;
- for an engine base, its engine properties (readable and writable attributes), its integer constants and its
  operators; the derived class also takes over the base's engine type, so its objects are accepted wherever
  the engine expects an object of the base class.

Members added to the base later are not copied; lookups on a derived class's objects never consult the base
(section 2). A script class has one base (a second call of the returned function is not made by any script;
its effect is unestablished).

Bases in the shipped scripts (measured, [data_classes.py]):

- 266 declarations derive directly from an engine class, 17 from a script class (all through
  `namespace.Class`, and every such chain ends at the engine class `CUIScriptWnd`), so 283 declarations (200 in
  loadable scripts) reach an engine class. 3 name a base that is neither declared by any script nor listed:
  `motivation`, `motivation_action` (`bug_test.script`) and `attachable_item` (`ce_new_attachable_item.script`);
  none of these scripts is loadable.
- Every script base is declared in another script than the class deriving from it, so its methods are all
  defined when the derivation runs (1.2).
- 33 distinct engine bases, 31 of them in loadable scripts. Declarations per base (all scripts / loadable):
  `property_evaluator` 116/86, `action_base` 73/51, `CUIScriptWnd` 17/11, `object_binder` 11/10, `effector`
  9/5, `CUIWindow` 8/8, `CUIListItemEx` 3/3, `cse_alife_item` 2/1, `cse_alife_smart_zone` 2/2,
  `game_sv_mp_script` 2/1, and 1/1 each for `CGameObject`, `CSE_AbstractVisual`, `UIGame_custom_script`,
  `cse_alife_car`, `cse_alife_helicopter`, `cse_alife_human_stalker`, `cse_alife_item_artefact`,
  `cse_alife_item_custom_outfit`, `cse_alife_item_torch`, `cse_alife_item_weapon`,
  `cse_alife_item_weapon_magazined`, `cse_alife_item_weapon_magazined_w_gl`, `cse_alife_item_weapon_shotgun`,
  `cse_alife_monster_base`, `cse_alife_object_physic`, `cse_alife_space_restrictor`, `cse_alife_trader`,
  `cse_anomalous_zone`, `cse_zone_visual`, `game_PlayerState`, `game_cl_mp_script`; and 1/0 each for
  `action_planner` and `game_sv_Deathmatch`.

## 2. Constructing: calling a script class, `__init`

### 2.1 The call (static, [bin_lua.py])

Calling a script class, `Name(a, b, ...)`, makes a new script object and returns it. In order:

1. A new object is made, with no fields of its own. If the class has a base, the object is marked as having no
   engine part yet, and the global `super` is set (section 3).
2. `__init` is read from the class's **own** members (not inherited, 1.3: deriving never copies it) and called
   as `__init(object, a, b, ...)`; its results are discarded. Errors propagate to the caller of `Name(...)`.
3. The object is returned.

A class without its own `__init` therefore cannot be constructed: the call fails with Lua's error for calling a
nil value (believed: the call is an ordinary call of the looked-up value).

### 2.2 In the shipped scripts (measured, [data_classes.py])

Every one of the 380 declared classes defines `__init` in its own script: 379 as `function Name:__init(...)`,
one (`script_object` in `_g.script`) as `function script_object.__init(self, ...)`, which is the same thing.

## 3. `super`

### 3.1 What it is (static, [bin_lua.py])

`super` is a global variable that the binding library assigns while an object is being constructed; it is not a
fixed function. Before calling `__init` of a class that has a base, the construction stores in the globals a
function bound to that class and to the new object, under `super`. Inside `__init`, `super(args...)` therefore
constructs the base part of the object being initialised. Scripts read `super` as a bare name; in a namespaced
script it is found through the environment's fallback to the globals (believed; with the `-_g` prefix of
script-loading.md 2.3 there is no fallback, and `super` would be nil there).

`super(args...)` does, depending on the base:

- **Script base**: calls the base's own `__init(object, args...)`. During that call `super` is the function
  for the base's base if the base has one, or nil if not. After it returns, `super` is set to nil.
- **Engine base**: chooses one of the engine class's registered constructors and constructs the object's engine
  part with `args...`: if the engine class has exactly one constructor it is used without checking the
  arguments; otherwise the constructors whose parameter count equals the number of arguments are compared by the
  binding library's argument-match score and the best is used. No error is raised when none matches; the
  outcome is unestablished. The constructor receives a weak reference to the script object; this is what lets
  the engine reach the object's overrides later (section 5). `super` is not reset afterwards. The object is
  then marked as having its engine part.

Consequences (believed, from the above):

- `super` must be called before anything else in `__init` constructs another script object, since that
  construction replaces the global `super`.
- An object of a class with an engine base whose `__init` never calls `super` has no engine part; what engine
  methods and engine calls do with such an object is unestablished.
- Calling `super` twice, or outside a construction, constructs again with whatever `super` holds at that time.

### 3.2 In the shipped scripts (measured, [data_classes.py])

- `super` is called in 286 methods, each an `__init`, once each, and in every case as the first statement of
  the body.
- Every class with a base calls it; no class without a base calls it.
- For each of the 266 classes with an engine base, the number of arguments passed to `super` equals the
  parameter count of a constructor the listing gives for that base: `property_evaluator` 2 arguments (106
  classes) or none (10); `action_base` 2 (72) or none (1); `effector` 2 (9); `object_binder` 1 (11); every
  `cse_…` base and `CSE_AbstractVisual` 1; `CUIScriptWnd`, `CUIWindow`, `CUIListItemEx`, `UIGame_custom_script`,
  `CGameObject`, `action_planner`, `game_PlayerState`, `game_cl_mp_script`, `game_sv_Deathmatch`,
  `game_sv_mp_script` none. What the arguments mean belongs to each base's spec.

## 4. Members of script objects

### 4.1 Reading `object.key` (static, [bin_lua.py])

In this order, the first that applies:

1. The key `__ok`: true if the object has its engine part (3.1), false if not. No script uses `__ok`
   (measured, [data_classes.py]).
2. A field stored on the object itself (4.2).
3. A member of the object's class: the methods it defines, the members copied from its base (1.3), and anything
   assigned to the class later (4.4).
4. An engine property of the class (copied from an engine base), read from the engine part.
5. Otherwise nil. The base class is never consulted at this point (1.3).

So `object:method(...)` finds a script override before the engine method of the same name, and an engine method
the class does not override is found among the copied members and called on the engine part.

### 4.2 Assigning `object.key = value` (static, [bin_lua.py])

If the class has an engine property `key` that can be written, the engine property is set. Otherwise the value
is stored as a field of the object itself; this includes engine properties that can only be read, so after
such an assignment reading `object.key` gives the stored field (4.1 step 2), not the engine value.

In the shipped scripts (measured, [data_classes.py]): assignments `self.key = ...` in classes that reach an
engine class name a member the listing gives for the engine base chain only for `type`, in three declarations of two multiplayer
classes (`C_new_game_sv_Deathmatch`, `game_sv_LastStanding` twice). No class assigns an object field whose
name is one of the method names the engine dispatches (5.4).

### 4.3 Engine properties the listing lacks (gap 2 of the scoping spec; measured, [data_classes.py])

Every field read as `self.key` in the methods of the 283 classes that reach an engine class (with the methods of
their script bases) was compared with the fields those methods assign, the methods they define, and the members
the listing gives for the engine base chain:

- Listed engine properties read this way: `object`, `id`, `m_game_vertex_id`, `m_level_vertex_id`, `online`,
  `parent_id`, `position`, `script_version`.
- 42 further names are read without being assigned in the class, but every one of them is assigned by some
  script as `x.name = ...` (the window controls of the multiplayer and options dialogs are assigned by the code
  that builds them, for example `owner`).
- Exactly one is assigned nowhere and is not listed: `enemy_last_accessible_vid`, read in
  `xr_combat_zombied.script` (class `action_zombie_shoot`, base `action_base`, a loadable script). It does not
  occur in `xrGame.dll`, so it is not an engine property (believed) and reads nil (4.1).

So the scripts give no evidence of an engine property, on any engine class they derive from, that the listing
lacks. Calls `self:m()` of names that the class chain neither defines nor lists for its engine base:
`GetRadioGroup` (`game_last_dialog.script`), `setEnabled`, `setVisible` (`ce_switcher.script`) — none occurs in
`xrGame.dll` (absent names of the scoping spec, 2.2) — and `id`, `name`, `position` in
`xr_motivator.script` (class `motivator_binder`, base `object_binder`; these are listed for game objects, not for
`object_binder`). **Disputed in review:** the calls of `id`, `name` and `position` in `xr_motivator.script`
are all in `function motivator_binder:hear_callback(self, who, ...)`, whose explicit first parameter `self`
shadows the method's own receiver; they are calls on whatever the caller passes as that argument, not on the
binder object, so the conclusion below does not apply to them (measured from the script). All six scripts are
loadable. **Disputed in review:** the six names come from three scripts (`game_last_dialog.script`,
`ce_switcher.script`, `xr_motivator.script`), all loadable. By 4.1 such a call finds nil and fails with Lua's
error for calling a nil value (believed).

### 4.4 Class objects: `Class.name` and assignments to a class (static, [bin_lua.py])

- `Class.name` (for script and engine classes alike) reads the class's **defaults**: for an engine method, a
  function that calls the engine class's own implementation (section 6); for a script class, its methods
  and other members. Otherwise the engine class's integer constant `name`; otherwise nil.
- Assigning to a class (`function Class:m() ... end`, `function Class.m(...)`, `Class.x = v`) stores the value
  both among the members its objects see (4.1 step 3) and among its defaults, without metamethods. This is how a
  script class gets its methods and its `__init` and `__finalize`.
- No script assigns members to an engine class (measured, [data_classes.py]: no `function` statement defines a
  member on a listed class name).

## 5. Engine calls into script overrides

### 5.1 Which engine classes reach overrides (static, [bin_game.py])

An engine call reaches a script override only for engine classes that the engine built for it: classes whose
engine type keeps the weak reference handed over by `super` (3.1) and whose engine methods, when called by the
engine, look for a script override first. `xrGame.dll` has 70 such run-time types (types deriving from the
binding library's wrapper base, read from its run-time type information).

The registration code (static, approximate: registrations attributed to the nearest preceding class name in code
order, as in the scoping spec 2.2) registers **default implementations** (6.1) in 66 registration functions,
for these class names: `action_base`, `action_planner`, `planner_action`, `property_evaluator`,
`property_evaluator_const`, `object_binder`, `effector`, `DLL_Pure`, `CGameObject`, `CUIScriptWnd`,
`UIGame_custom_script`, `game_PlayerState`, `game_cl_mp_script`, `game_sv_Deathmatch`, `game_sv_mp_script`,
`CSE_AbstractVisual`, and 51 classes whose names begin with `cse_` (from `cse_abstract` to `cse_zone_visual`;
full list in [bin_game.py]'s output) — 71 names in all. The same function as `effector` also registers `effector_params`,
`noise`, `color` and `duality`; that their default implementations belong to `effector` alone is believed.
These are the engine classes whose methods a script class can override for the engine (believed: an
overridable method is registered with a default implementation). Which methods of each are overridable, their
arguments, results and when the engine calls them belong to the spec owning the class (scope 4.5).

Of the 33 engine bases the scripts use (1.3), all appear in that list except `CUIWindow` and `CUIListItemEx`
(both used only by loadable scripts). Their script classes define only `__init`, `__finalize` and methods of
their own (measured, [data_classes.py]: none defines a method the listing gives for the base chain), so no engine call to an
override is involved; that the engine makes none for these two bases is believed.

### 5.2 How an engine call finds the method (static, [bin_lua.py], [bin_game.py])

When the engine calls an overridable method `m` on an engine part that belongs to a script object:

1. It gets the script object from the weak reference.
2. It looks `m` up among the members of the object's **class** (4.1 step 3), not among the object's own fields:
   an override must be defined on the class (`function Class:m(...)`); a function stored in an object field is
   never called by the engine.
3. If what it finds is the engine's own method `m` (the class does not override it, so the member copied from
   the engine base is found), it uses the engine's default implementation instead. So a method the script class
   does not override behaves as in the engine class, without calling back into itself.
4. Otherwise it calls what it found as `found(object, arguments...)`, in a protected call.

No error handler is installed for these calls (static: `xrGame.dll` does not import the binding library's
setter for one), so the error message is Lua's message without a traceback (believed).

### 5.3 Results and errors (static, [bin_game.py])

- A call with a result keeps exactly one result. Of the call shapes examined, one with a numeric result reads it
  with Lua's number conversion (a non-number gives 0) and rounds it to an integer; how each other result type is
  converted belongs to the method's spec.
- If the call fails (an error in the override, or a found value that is not callable), the engine's error
  handler writes the error message to the log, unless the message is exactly `cannot resume dead coroutine`, and
  then stops the game with the fatal error `LUA error: %s` (the message). Should that handler return, the
  program terminates; that it never returns is believed (it ends in the engine's fatal-error routine).
- A result that cannot be converted to the engine type expected stops the game with the fatal error
  `LUA error: cannot cast lua value to %s` (the type) (static: this handler is installed together with the error
  handler; which calls use it is unestablished).

So an error in any script override the engine calls ends the game (as in the original, fatal).

### 5.4 Method names the engine dispatches (static, [bin_game.py])

54 engine functions perform the lookup of 5.2: 49 name the method directly, 41 distinct names: `Dispatch`
`FillProps` `GetMapEntities` `OnEvent` `OnKeyboard` `OnPlayerHitPlayer` `STATE_Read` `SetClGame`
`TranslateGameMessage` `_construct` `createGameUI` `createPlayerState` `detect_probability` `enabled` `execute`
`finalize` `init` `initialize` `load` `net_Export` `net_Export_State` `net_Import` `net_Relcase` `net_Spawn`
`net_destroy` `net_export` `net_import` `net_save_relevant` `net_spawn` `on_death` `process` `reinit` `reload`
`save` `setup` `suitable` `task` `type_name` `update` `use` `weight`; 5 take the name from a parameter (names
unestablished here). This is not the complete set of overridable methods (overrides the scoping spec 4.3 counts,
such as `evaluate` or `shedule_Update`, are not among the literal names); which engine class calls which name,
and the rest of the set, belong to the owning specs.

## 6. Base-class calls: `Base.m(self, ...)`

### 6.1 Engine bases (static, [bin_lua.py])

Every engine method is registered twice: as the member objects see (which calls the method normally, so an
engine class's own overriding takes effect) and as a default of the class (4.4), which calls the engine
class's own implementation of exactly that class, not a script override. `EngineBase.m(self, ...)` reads the
default, so inside an override it runs the engine's implementation that the override replaced.

If the method was registered without a separate default implementation, the default raises the error
`pure virtual function called`. Which methods have one belongs to the owning specs (5.1).

### 6.2 Script bases (believed, from 1.3 and 4.4)

`ScriptBase.m(self, ...)` reads the script base's member `m` (its own method, or what it copied from its own
base) and calls it as an ordinary Lua function. For a member the script base copied from an engine class, this is
the engine default (6.1).

### 6.3 In the shipped scripts (measured, [data_classes.py])

- 380 calls `EngineBase.m(self, ...)`, each inside the script class's override of the same name `m`, each
  naming an engine class in the calling class's own base chain. 86 distinct (base, method) pairs, 79 of them in
  loadable scripts (listed in [data_classes.py]'s output). 8 pairs name a method the listing lacks for that base
  chain: `CGameObject.Load`, `CGameObject.net_Destroy`, `CGameObject.shedule_Update`,
  `UIGame_custom_script.IR_OnKeyboardPress`, `action_planner.reinit`, `cse_anomalous_zone.update`,
  `cse_zone_visual.update`, `game_sv_mp_script.OnPlayerConnect`; whether the engine registers them belongs to the
  owning specs (scoping spec 2.2).
- 22 calls `namespace.ScriptBase.m(self, ...)` on a script base: `InitControls` 17, `OnKeyboard` 4,
  `InitCallbacks` 1.

## 7. `__finalize` and operators

### 7.1 `__finalize` (static, [bin_lua.py])

When a script object is collected by Lua's garbage collector, `__finalize` of its class (its own member; it is
never copied, 1.3) is called with the object, if present; then the same for its script base, and so on up the
chain of script bases (engine bases have none). Results are discarded. Whether an error there is reported, and
when collection happens, is unestablished. 55 classes define `__finalize` (26 in loadable scripts; measured,
[data_classes.py]).

### 7.2 Operators (static, [bin_lua.py])

A script object supports 12 operators through methods of these names: `__add` `__sub` `__mul` `__div` `__pow`
`__lt` `__le` `__eq` `__call` `__unm` `__tostring` `__concat`. Applying one looks the name up on the first
operand (as in 4.1), then on the second, and calls the first found; if neither has it, the error is
`No such operator defined`. The argument order passed to the method is unestablished. No shipped script class
defines any of them (measured, [data_classes.py]).

### 7.3 Messages (measured, [bin_lua.py])

The class system's only messages are `the attribute '`… `' is read only` (assigning a read-only engine
property on an engine object; not on a script object, 4.2), `No such operator defined` and
`pure virtual function called`. In particular there is no message for a failed constructor or method match.

## 8. Verification facts

All measured unless marked; `run_all.py` reproduces every number above.

- Scripts: 439 scanned, 250 loadable; 380 declarations (355 names) in 190 scripts, 278 in 134 loadable scripts;
  286 with a base, 94 without; 266 engine bases, 17 script bases, 3 unresolved (none loadable); 283 reaching an
  engine class; 33 distinct engine bases (31 in loadable scripts) [data_classes.py].
- `__init` in all 380; `super` in 286 `__init` bodies, first statement each; argument counts matching a listed
  constructor in all 266 engine-based classes; 55 `__finalize`; 0 operator methods; 0 uses of `__ok`
  [data_classes.py].
- Base calls: 380 on engine bases (86 pairs, 79 in loadable scripts), 22 on script bases [data_classes.py].
- Static, `xrGame.dll`: 108 names imported from `xrLUA.dll`; 54 functions performing the engine-to-script
  lookup, 49 with literal names (41 distinct); 70 run-time types deriving from the wrapper base; 66 registration
  functions with default implementations; 12 functions that release a script object's ownership of its engine
  part [bin_game.py].
- Static, `xrLUA.dll`: 24 functions examined; 12 operator names; the class system's strings [bin_lua.py].

## 9. Unestablished

- The outcome of deriving from a value that is not a class (1.3), of a second base call, and of a declaration
  with a missing name.
- What engine methods and calls do on an object whose engine part was never constructed (3.1).
- The outcome of `super` when no engine constructor matches the arguments.
- Lifetime: when the engine takes ownership of a script object's engine part (12 functions release the script
  object's ownership; which ones and when belongs to the specs of the members concerned), and what happens if a
  script object is collected while the engine still uses its engine part (the reference the engine holds is
  weak).
- Whether an engine object the engine passes to a script is the same script object that was constructed for
  it.
- Which methods of each class in 5.1 are overridable and which have default implementations; the 5 dispatched
  names passed as parameters; the conversion of each result type and which calls use the cast-failure handler.
- Errors inside `__finalize`, and the argument order of operator methods.
- Whether `CUIWindow` and `CUIListItemEx` reach any override.

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrLUA.dll` and `xrGame.dll` as
  identified above; the archive file tree of `gamedata.db0` … `gamedata.dbd`, extracted with the project's
  archive tool.
- Analysis: static analysis with Ghidra of the binding library's class system in `xrLUA.dll` (class creation
  and derivation, construction, `super`, member reading and assignment on objects and classes, method
  registration, the engine-to-script member lookup, operators, collection) and of `xrGame.dll` (the functions
  calling the engine-to-script lookup, their call and error paths, the error handlers installed at state
  creation, registration code with default implementations); run-time type information and import tables read by
  a throwaway parser; a throwaway Lua tokenizer and scanner over the 439 scripts. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used. The
  binding library is identified only by the export names of `xrLUA.dll` (script-environment 1.2).

## Gaps found in review

1. The callers of the 5 engine functions that take the dispatched method name from a parameter (5.4) were not
   traced, so it is not established that the overrides the scoping spec (4.3) counts but 5.4 does not name
   (for example `evaluate`, `shedule_Update`, `STATE_Write`, `on_register`) are reached by the lookup of 5.2.
2. The cast-failure handler of 5.3 also writes the message on top of the stack to the log, with the same
   exception for `cannot resume dead coroutine`, before its fatal error; 5.3 states this only for the error
   handler.
3. What Lua's `type` and `tostring` give for a script class and for a script object (one without
   `__tostring`) is not stated.
4. With the `-_g` prefix (script-loading.md 2.3), the effect on `class` declarations themselves (the bare
   name `class` is read through the same environment as `super`) is not stated.
