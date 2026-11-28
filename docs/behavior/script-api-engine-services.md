# Script API: engine services

What the engine services of the game scripts' interface do in S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): the console object and its methods, the render device object and its fields and
methods, the global functions `app_ready`, `editor`, `log`, `error_log`, `flush`, `prefetch`,
`verify_if_thread_is_running`, `xrRender_test_r2_hw`, `script_server_object_version` and
`IsGameTypeSingle`, and the classes `DLL_Pure`, `ISheduled`, `IRenderable`, `ICollidable`,
`IRender_Visual`, `CKinematicsAnimated` and `CBlend`. This is spec B of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6), which assigns these names to it; its scope rule applies:
this spec owns every name of these classes and functions as the binary registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md` (the runtime, LuaJIT 1.0.3; one number type, the 64-bit double),
  `docs/behavior/script-loading.md` (namespaces, loading, `prefetch` as occasion 5 of its section 2.2).
- The console specifications (`docs/behavior/`): CC `console-commands.md` (line processing, kinds,
  statuses), CW `console-window.md` (opening and closing), CI `console-input.md` (the input line), CL
  `console-log.md` (the fatal-error report, section 3.8).
- `docs/formats/ltx-config.md` (reading `config\system.ltx`; reading a missing key is an error),
  `docs/formats/skeletal-motions.md` (motion sets, cycles, partitions), `docs/behavior/title-menu.md`
  (the pause caption, section 2), `docs/behavior/loading-screen.md`.
- Spec A, script classes deriving from engine classes
  (`docs/behavior/script-api-script-classes-deriving-from-engine-classes.md`). Where a statement needs A's ground (how script classes derive
  from these classes, how the engine reaches a script override, how arguments of a call are matched and
  what a mismatch does), it says **depends on A** and states nothing of its own about it.

Conventions:

- "The engine" is `bin\XR_3DA.exe` (1,636,424 bytes, MD5 `ba3ad0127bff6619ed991a7c82d8b946`), "the game
  library" `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the core library"
  `bin\xrCore.dll` (856,064 bytes, MD5 `eab71f90f01575eca3500e623873e57c`), "the runtime library"
  `bin\xrLUA.dll` (290,816 bytes, MD5 `a65ba53867962afc92e0f17409e9cd1f`) [ES-1].
- **The listing** is `scripts\lua_help.script` (script-api-scope 2.1). It is a guide only; every member
  below is established from the binary's registration code.
- A call is written as scripts write it: `get_console():execute(line)`, `device().width`. A **number** is
  the runtime's double; a **boolean** is `true` or `false`; a **text** is a Lua string.
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured**
  (read from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**.
  No binary was run.
- "Fatal error" means the program ends through the fatal-error report of CL section 3.8; the row values
  given are those the report shows (expression, description, arguments).

**Checks.** Every claim carries a tag [ES-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-engine-services/run.py` (`python3 -I run.py`): it reads the binaries (bytes, export and
import tables, data values), the evidence of read-only headless static analysis of them (produced by
`ghidra.py` beside it into `bin/`), the listing, `config\system.ltx` and the shipped scripts. All checks
pass (`ALL PASS 14`).

## 1. What is registered

All names below are registered in the game state, in the global scope unless stated [ES-2, ES-3, ES-8,
ES-9, ES-10, ES-11, ES-12]. They match the listing exactly: 9 classes with 27 members, 11 global functions
(47 items), plus `IsGameTypeSingle`, which the listing lacks [ES-2]. No further member of these classes
is registered [ES-3, ES-8, ES-12].

| Name | Kind | Arguments → result | Section |
|---|---|---|---|
| `get_console()` | global function | → the console object | 2 |
| `CConsole` | class, no constructor | `execute(text)`, `execute_script(text)`, `show()`, `hide()`, `get_string(text)`, `get_token(text)`, `get_integer(text)`, `get_float(text)`, `get_bool(text)` | 2 |
| `device()` | global function | → the render device object | 3 |
| `render_device` | class, no constructor | read-only fields `width`, `height`, `frame`, `precache_frame`, `time_delta`, `f_time_delta`, `cam_pos`, `cam_dir`, `cam_top`, `cam_right`, `fov`, `aspect_ratio`; methods `time_global()`, `is_paused()`, `pause(boolean)` | 3 |
| `app_ready()` | global function | → boolean | 4.1 |
| `editor()` | global function | → boolean | 4.2 |
| `log(text)`, `error_log(text)` | global functions | → nothing | 4.3 |
| `flush()`, `verify_if_thread_is_running()` | global functions | → nothing | 4.3 |
| `prefetch(text)` | global function | → nothing | 4.4 |
| `IsGameTypeSingle()` | global function (unlisted) | → boolean | 4.5 |
| `xrRender_test_r2_hw()` | global function | → boolean | 4.6 |
| `script_server_object_version()` | global function | → number | 4.7 |
| `DLL_Pure`, `ISheduled`, `IRenderable`, `ICollidable` | classes | see 5.1 | 5.1 |
| `IRender_Visual` | class | `dcast_PKinematicsAnimated()` | 5.2 |
| `CKinematicsAnimated` | class, no constructor | `PlayCycle(text)` | 5.2 |
| `CBlend` | class, no constructor, no members | — | 5.2 |

How a call with arguments of the wrong number or type is rejected is the binding library's general rule
(**depends on A**). The listing writes some methods with an explicit first parameter
(`get_bool(CConsole*, string)`, `time_global(const render_device*)`); scripts call them as methods.

## 2. The console

### 2.1 `get_console()`

Returns the engine's single console object: the same object on every call, the console of CC, CW and CI
[ES-8]. **Believed**: it exists whenever scripts run (the engine creates it at startup, CW section 1,
before the game library loads scripts).

### 2.2 `execute(line)`

Executes `line` as one console line [ES-8]:

1. The first 1023 bytes of `line` are put into the console's input line, replacing what is there (CI
   section 3: anything typed and not yet entered is lost).
2. The line goes through line processing (CC section 2) with the echo of step 2 off: it is not written to
   the log as `~ <line>`. Every other step applies: tab and line-feed replacement, the collapse of spaces,
   the exact, case-sensitive name lookup, `! Unknown command:  <name>`, lower-casing of the argument
   except for the three case-keeping commands, the status line for a bare "shows" command, and so on.
3. The input line is left empty afterwards.

Nothing is returned; whether the command existed or accepted its argument is not reported to the script
[ES-8]. There is no separator for several commands in one line (CC section 2). Lines longer than 1023
bytes are cut to their first 1023 bytes [ES-8].

The loadable scripts call `execute` with these command names as literal text (measured): `check_for_updates`,
`disconnect`, `flush`, `g_game_difficulty`, `jump_to_level`, `load`, `load_last_save`, `main_menu`, `quit`,
`save`, `start`, `unbindall`; they also execute lines built at run time (measured, 9 loadable scripts call
`get_console()`) [ES-13].

### 2.3 `execute_script(name)`

Executes the line `cfg_load <name>` (the text `cfg_load` and one space, then `name`) exactly as `execute`
does (2.2); `cfg_load` is console-scope section 5.6 [ES-8]. The line is built in a 128-byte buffer whose
concatenation is bounded at 119 bytes: a `name` longer than 109 bytes makes the C runtime's bounded
concatenation fail, and the core library's handler for that failure ends the program with a fatal error
reporting `error handler is invoked!` [ES-8]. Which rows of the report hold that text and what fills the
others is **unestablished**. No shipped script calls `execute_script` [ES-13].

### 2.4 `show()` and `hide()`

`show()` opens the console exactly as CW section 1 "What opening does" describes (nothing if it is
open); `hide()` closes it exactly as CW section 2 "What closing does" describes (nothing if it is closed,
and nothing on a dedicated server) [ES-8]. No shipped script calls either (CW section 1) [ES-13].

### 2.5 The value getters

Each getter looks its argument up as a command name with the same exact, case-sensitive lookup as line
processing (CC section 2, step 4); the name is neither trimmed nor lower-cased [ES-8]. Kinds are those of
CC section 3 and its command table.

| Call | Name not found | Found |
|---|---|---|
| `get_string(name)` | `nil` | the command's status text, as a bare "shows" command prints it (CC section 3; empty for an action) |
| `get_token(name)` | `nil` | the same as `get_string` |
| `get_integer(name)` | `0` | integer kind: its value; on/off kind: `1` when on, `0` when off; any other kind: fatal error, a failed check without a message whose checked condition is `cm` (CL 3.8: expression `assertion failed`, description `cm`) |
| `get_float(name)` | `0` | real kind: its value (the single-precision number it stores, as a double); any other kind: **believed** the program crashes (the command is read as a real command without one, a read through no object) |
| `get_bool(name)` | **unestablished**: the result is taken from storage the call never sets, so it is indeterminate | on/off kind: `true` when on; integer kind: `true` when its value is not 0; any other kind: **believed** a crash, as for `get_float` |

[ES-8]. The `nil` of `get_string` and `get_token` comes from the runtime's string push, which pushes nil
for no text (static); that the binding library hands the getter's result to that push unchanged is
**believed** (the result type is text). The text returned is a copy: a later call does not change a text
already returned (**believed**, the runtime copies every string it pushes) [ES-8]. No shipped script calls
any getter [ES-13].

## 3. The render device

### 3.1 `device()`

Returns the engine's single render device object, the same on every call [ES-9]. All fields are
read-only: the class registers a reader and no writer for each [ES-3]. What assigning to one does is the
binding library's general rule (**depends on A**).

### 3.2 Fields

Each read returns the current value [ES-3]. The vector fields return a new vector object (spec E's class)
holding a copy of the three components at the time of the read; changing it changes nothing in the
engine [ES-3]. Unsigned fields are converted to non-negative numbers [ES-3].

| Field | Type | Meaning |
|---|---|---|
| `width`, `height` | unsigned 32-bit | the size in pixels of the frames being drawn: set whenever the video device is reset, from the back-buffer size it was reset with [ES-6] (which sizes those are: `docs/behavior/display-mode.md`) |
| `frame` | unsigned 32-bit | the frame counter: increased by 1 at the start of every frame the main loop moves the device on [ES-4]; its value before the first frame is **unestablished** (**believed** 0) |
| `precache_frame` | unsigned 32-bit | the number of frames left in the current precache phase, 0 outside one [ES-6] (3.4) |
| `time_delta` | unsigned 32-bit | `time_global()` of this frame minus that of the previous frame, in milliseconds [ES-4] |
| `f_time_delta` | 32-bit float | the frame time in seconds, smoothed (3.3) [ES-4] |
| `cam_pos`, `cam_dir`, `cam_top`, `cam_right` | vector | the camera's position and its forward, up and right directions, as the camera last applied them to the device [ES-6]; while precaching, the direction set of 3.4 |
| `fov` | 32-bit float | the camera's vertical field of view in degrees, as last applied: with *t* = tan(`fov` · π/180 / 2), the device's projection has vertical scale 1/*t* [ES-6] |
| `aspect_ratio` | 32-bit float | the camera's aspect ratio, as last applied: the projection's horizontal scale is `aspect_ratio` / *t*, so the ratio is height over width (**believed**: from that use alone) [ES-6] |

The camera's values reach the device once per frame when the camera is applied (**believed**: the
applying routine is the camera's per-frame update; its callers were not traced) [ES-6]. Which values the
camera computes (view-projection, the player's view) is outside this spec.

### 3.3 Times

The device keeps two timers that run in real time scaled by the device's **time factor** (the factor
scales both; spec H owns how the factor is set) [ES-4, ES-5]:

- the **global timer**, which stops while the game is paused (3.5) [ES-5];
- the **frame timer**, restarted at every frame while it is not paused [ES-4].

At the start of every frame [ES-4]:

1. `frame` increases by 1.
2. Normally: *e* = the frame timer's elapsed seconds; `f_time_delta` becomes 0.9 · *e* + 0.1 · (its
   previous value), then at most 0.1; while the game is paused it is 0 instead. `time_global()` becomes the
   global timer's elapsed milliseconds, and `time_delta` the difference from its previous value.
3. In the **constant-step mode** (a device flag; no console command of the engine sets it, and what sets
   it is **unestablished**): `time_global()` grows by exactly 20, `time_delta` is 20 and `f_time_delta` is
   0.02, whatever the real time [ES-4].

`time_global()` (the method) returns the current value, in milliseconds [ES-5]. It is set to 0 when the
main loop starts and from then on follows the global timer [ES-4]; when the global timer was started is
**unestablished** here. It belongs to the same device as the global `time_global` of spec H.

The loadable scripts read `device():time_global()` (6 scripts, among them `_g.script`'s replacement for
an absent global `time_global`, script-api-scope section 5), `device().precache_frame` (2), `device().width`
and `device().height` (2) [ES-13].

### 3.4 Precaching

A **precache phase** of *n* frames starts when the engine asks for one; `precache_frame` is then *n*
[ES-6]. **Disputed in review:** not unconditionally: while a device-wide state holds (which one is
unestablished), every request sets `precache_frame` to 0 instead, so no phase starts (static). The engine asks for one of 20 frames whenever the video device is reset [ES-6]; the phase at the
start of a level is loading-screen.md's ("precaches for 30 frames before the loading screen ends") and
other requests are **unestablished**. At the end of every frame of the phase, `precache_frame` decreases
by 1 and the loading image is drawn; sound is muted during the phase [ES-6]. While `precache_frame` is not 0,
the device's camera directions are replaced at the start of each frame: `cam_dir` turns once around the
vertical over the phase (its angle is 2π · `precache_frame` / *n*, in the horizontal plane), `cam_top` is
(0, 1, 0) and `cam_right` is the cross product of `cam_top` and `cam_dir` (this order) [ES-4, ES-6]. The
precise angle convention (which of sine and cosine goes to which component) is **unestablished**.

`bind_stalker.script` returns early from the player's update while `app_ready()` is false or
`device().precache_frame` is above 1 (measured) [ES-13]. **Disputed in review:** the player binder's
update does not test either; the test of both is in the player binder's callback for an added map
location (whose remaining body is commented out), and its article callback returns early while
`device().precache_frame` is above 1 (measured from the script).

### 3.5 `is_paused()` and `pause(on)`

The engine has one **game pause**: while it is on, every pausable timer stops, the global timer of 3.3
among them, and `f_time_delta` is 0 [ES-5]. `is_paused()` returns `true` while it is on [ES-5].

`pause(on)` [ES-5]:

- Does nothing at all while the engine runs a benchmark (a mode it has; how it is entered is
  **unestablished**, **believed** the `-batch_benchmark` run).
- `pause(true)`: if the game pause is off, the pause caption (title-menu section 2) is enabled: while the
  game pause is on, the HUD draws it [ES-5]. Then the game pause is switched on only if the current game
  type allows pausing, i.e. `IsGameTypeSingle()` (4.5) is true; otherwise only the caption flag changed.
- `pause(false)`: the game pause is switched off if it is on. The caption flag is left as it is.
- Sounds are not paused or resumed by either call [ES-5].

Switching the game pause on when it is on, or off when it is off, changes nothing [ES-5]. No shipped
script calls `is_paused` or `pause` (measured) [ES-13].

## 4. Global functions

### 4.1 `app_ready()`

`true` exactly while no loading phase is open [ES-7]. Loading phases nest: the first one opened sets
`app_ready()` to false; when the last open one closes, the engine runs `stat_memory` (CC, without echo) and
then `app_ready()` becomes true [ES-7]. Starting a level opens a phase around the game's start and the
level's loading [ES-7] (the game library opens further nested ones, **believed** its loading stages). The
loading screen is drawn by the engine only while `app_ready()` is false [ES-7]. Before the first level
start `app_ready()` is **false**: the flag starts as 0 and nothing else sets it [ES-7].

### 4.2 `editor()`

Always `false` [ES-9]. The scripts test it to choose their game behaviour (7 loadable scripts, among them
`_g.script`, whose script classes on `effector` and `property_evaluator` are declared only when it is
false, and the class registrators) [ES-13].

### 4.3 Functions that do nothing

- `log(text)` and `error_log(text)`: write nothing anywhere [ES-9]: both end in a log
  routine that returns at once. `_g.script` and `profiler.script` call `log` (measured)
  [ES-13].
- `flush()`: does nothing [ES-9]; unlike the console command `flush` (CL 3.5), it does not write the log
  file. No shipped script calls it (`level_tasks.script` executes the console command instead) [ES-13].
- `verify_if_thread_is_running()`: does nothing [ES-9]. `_g.script` calls it (measured) [ES-13].

### 4.4 `prefetch(name)`

Loads the namespace `name` as script-loading.md section 2.2, occasion 5 describes, and returns nothing
[ES-9]. In particular: an empty name loads nothing; a namespace already loaded is not loaded again; the
remembered missing name of occasion 3 does not stop it, and it does not test whether the file exists
before trying (a missing file then fails silently, script-loading section 2.4) [ES-9].
`class_registrator.script` calls `prefetch("_G")` at its top level, which loads nothing because `_G` is
loaded (script-api-scope section 5) [ES-13].

### 4.5 `IsGameTypeSingle()`

`true` when the current **game type** is single-player or none [ES-10]. The game type is a number kept by
the game: 0 at startup; each time a game is started, the game-type option is compared, as text, with
`single` (1), `deathmatch` (2), `teamdeathmatch` (6) and `artefacthunt` (7), and any other text gives 0
[ES-10]. The game-type option is the second `/`-separated item, lower-cased, of the option the engine
starts the game with [ES-10]; that this is the second item of `start server(…)` (`single` in
`start server(all/single/alife/new) client(localhost)`) is **believed** (the caller passing that option
was not traced). The same test decides whether `pause(true)` may pause (3.5) [ES-10].

The listing lacks `IsGameTypeSingle`; `se_zones.script` tests `_G.IsGameTypeSingle == nil` before calling
it (measured) [ES-13].

### 4.6 `xrRender_test_r2_hw()`

`true` when the first display adapter's hardware device can run the R2 renderer [ES-11]: the call
creates a Direct3D 9 object, asks it for the capabilities (`D3DCAPS9`, public Direct3D 9 documentation)
of adapter 0 with the hardware device type (**believed**: the method called is the capabilities query,
by its argument shape and place in the interface), releases the object, and returns true exactly when
all three hold:

- the pixel shader major version (bits 8–11 of `PixelShaderVersion`) is at least 2;
- `PS20Caps.NumInstructionSlots`, compared as a 16-bit number, is at least 256;
- `NumSimultaneousRTs`, compared as a 16-bit number, is at least 3.

[ES-11]. What happens when Direct3D 9 cannot be created is **unestablished**. `ui_mm_opt_main.script`
disables the options' R2 choice when it is false (measured) [ES-13].

### 4.7 `script_server_object_version()`

Returns the 16-bit number of `current_server_entity_version` in section `[script]` of the settings,
`config\system.ltx` (**believed**: the settings object read is the one the engine loads from that file)
[ES-9]. The value is read once, at the first call, and the same number is returned afterwards [ES-9]. If
the section or the key is missing, the function stores 0 and then reads the key anyway, so the read fails
as ltx-config.md says for a missing key [ES-9b]. In the shipped data the value is `7` (measured) [ES-9].
No shipped script calls it [ES-13].

## 5. Classes

### 5.1 `DLL_Pure`, `ISheduled`, `IRenderable`, `ICollidable`

These are the four engine bases the listing gives for `CGameObject` (`C++ class CGameObject :
DLL_Pure,ISheduled,ICollidable,IRenderable`) and, for `DLL_Pure`, of further listed classes (`CUIScriptWnd`,
`ce_script_zone`, `ce_smart_zone`, `game_GameState`) [ES-2]. As registered [ES-12]:

- `DLL_Pure`: a constructor (no arguments) and one method, `_construct()`. The class is registered so that
  script classes can derive from it and override `_construct` (**depends on A** for both: A
  counts `DLL_Pure` among the engine classes with default implementations and `_construct` among the names
  the engine dispatches, its sections 5.1 and 5.4); for an engine
  object the engine's own `_construct` returns the object itself [ES-12]. When the engine calls
  `_construct` on an object it creates is spec C's (the object factory).
- `ICollidable`: a constructor (no arguments), no members.
- `ISheduled`, `IRenderable`: no constructor, no members.

Scripts use these only as bases (script-api-scope section 4.3: `ce_switcher` derives from
`CGameObject`) [ES-13]; what an object constructed directly from `DLL_Pure()` or `ICollidable()` can be
used for is **unestablished** (**believed** nothing: no shipped script does it).

### 5.2 Visuals: `IRender_Visual`, `CKinematicsAnimated`, `CBlend`

- `IRender_Visual` has a constructor (no arguments) and `dcast_PKinematicsAnimated()`, which returns the
  same visual as a `CKinematicsAnimated` when it is a skeletal visual with motions, and `nil` for any
  other visual (the base answer is none; the skeletal visual with motions answers itself) [ES-12].
  Which game-object member hands scripts a visual is spec M's (`ce_switcher.script` uses
  `self:Visual()`).
- `CKinematicsAnimated:PlayCycle(name)` plays the cycle `name` of the visual [ES-12]:
  - The name is searched among the cycles of the visual's motion sets (skeletal-motions.md), the last
    set first, by exact (case-sensitive) name; the first set holding it wins [ES-12].
  - Not found: fatal error, a failed check with a message: expression `motion_ID.valid()`, description
    `! MODEL: can't find cycle: `, argument the name [ES-12].
  - Found: it is played on its definition's partition (on every partition, 0 … 3, for 0xFFFF), mixing in:
    the cycles already playing on that partition are faded out at the new definition's falloff rather
    than stopped at once; its playback parameters (speed, accrue, falloff, the option of flags bit 1)
    come from its definition [ES-12]. The result (the new playback, a `CBlend`) is not
    returned to the script [ES-12]. How playback then proceeds (advancing, fading, the scaling of the
    stored parameters) is **unestablished** here and belongs to the playback of skeletal motions, which
    skeletal-motions.md leaves open.
- `CBlend`: registered with no constructor and no members; no registered member returns one [ES-12].

`ce_switcher.script` (loadable) calls `self:Visual():dcast_PKinematicsAnimated():PlayCycle(anim_name)`
(measured) [ES-13].

## 6. Unestablished

- How arguments are matched, what a mismatched call or an assignment to a read-only field does, and how
  script classes derive from `DLL_Pure` and override `_construct` (all **depend on A**).
- `get_bool` of a name that is not a command (2.5); the crash of `get_float`/`get_bool` on other kinds
  (believed).
- The fatal report's rows for an over-long `execute_script` name (2.3).
- The starting value of `frame`, when the global timer starts, who sets the constant-step mode and the
  time factor (3.2, 3.3).
- The requests for precache phases other than the video-device reset and the level start, and the angle
  convention of the precache directions (3.4).
- How the benchmark mode is entered (3.5).
- The caller of the game start with its option text (4.5, believed `start server(…)`).
- `xrRender_test_r2_hw` when Direct3D 9 cannot be created (4.6).
- Skeletal playback after `PlayCycle` (5.2).

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries identified above;
  the archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs
  (`scripts\*.script`, `config\system.ltx`).
- Analysis: static analysis with Ghidra (headless, read-only) of the script registration of these names
  in the game library and of the engine, core-library and runtime-library routines they reach (console
  line execution and command lookup, the device's frame timing, pause, precache and camera application,
  the loading phases, the game type, the skeletal cycle lookup and start, the C runtime's
  invalid-parameter handler); export and import tables and data values read by a throwaway PE reader;
  the listing and the scripts read as text. The layout of `D3DCAPS9` comes from Microsoft's public
  Direct3D 9 documentation (`d3d9caps.h`). No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- What else the end of a precache phase does (3.4): it restores the sound muted during the phase and
  writes a memory-usage line `* MEMORY USAGE: <n> K` to the log, among other effects not described here.
- Which iterations of the main loop do not move the device on (3.2, 3.3: `frame` and the times are then not
  updated) is not stated.
- `PlayCycle` (5.2) on a partition index the visual does not define (in particular for 0xFFFF, which tries
  partitions 0 … 3 whatever the motion set has): what is skipped and when is not stated.
