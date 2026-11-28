# Script API U: effects and weather

What the effects-and-weather part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition), as scripts see it: the engine class `effector` (screen effects defined by a script class, with
its override `process`) and its record `effector_params`; the `level` functions that add, weight and remove
animated screen effects, camera effects and complex effects; the colour animator `color_animator`; the particle
object `particles_object` and the record `particle_params`; the `level` functions of the weather, and the two
environment classes the listing lacks. This is spec U of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6, order 8); its scope rule, its section 4.5 row U (the `effector`
overrides) and its review gaps apply: this spec owns every name of these classes as the game library registers
them, listed or not, and the entry point the engine entry-points spec D (its 1.3) gives this spec, the function
a script hands to a camera effect.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type, the 250
  loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md`: deriving, `super`, how an
  engine call reaches an override (5.2), results and errors of such a call (5.3, the fatal `LUA error: %s`),
  base-class calls (6.1), how arguments are matched (and that a failed match has no message, 7.3).
- Spec E, `docs/behavior/script-api-value-types-and-math.md`: `vector`, `fcolor`, and the classes `color`,
  `noise`, `duality` (its 6.2 – 6.4), number conversions (2.2), property rules (2.4).
- Spec B, `docs/behavior/script-api-engine-services.md` 3.3 (cited as **B 3.3**): the device's global timer, its
  frame time `f_time_delta` (seconds, smoothed) and `time_global()` (milliseconds).
- Spec D, `docs/behavior/script-api-engine-entry-points.md`: finding a script function by name (1.1) and the
  protected call whose failure is fatal (1.2). Spec H, `docs/behavior/script-api-time.md` 3.2: the time of day
  and the time factor the level hands the environment each frame. Spec N, `docs/behavior/script-api-level.md`
  (the namespace `level` as registered, 1.2). Spec T, `docs/behavior/script-api-sound.md` (its conventions: a
  number **made an integer**).
- `docs/behavior/frame-presentation.md` (cited as **FP**): the ten screen-effect values (2.1, named as the
  scripts name them), the **identity**, the combination and hand-over to the renderer (2.2) and what the renderer
  does with them.
- `docs/behavior/view-projection.md` (cited as **VP**): the camera managers, the **player's manager**, and the
  order in which camera effects are applied (1, step 3).
- `docs/behavior/sky.md`, `docs/behavior/static-shading.md` 9 (cited as **SS 9**), `docs/behavior/sound-rain-thunder.md`
  2.3: weather cycles, hourly weather sections, weather effects, the two current sections A and B, the blend weight
  *t*, environment modifiers, the blended far plane, fog density and rain density ρ.
- `docs/formats/colour-animations.md` (cited as **CA**): the library `lanims.xr`, lookup by name (2.5),
  evaluation (3).
- `docs/behavior/level-placed-effects.md` (cited as **P8**) and `docs/behavior/particle-lifecycle.md` (cited as
  **P2**): an effect or group instance, placement by a matrix, a velocity and a mode, play, the deferred and
  immediate stop, playing, the update with *e* ms, the schedule, the visibility sphere; P6 for groups.
- `docs/formats/ltx-config.md` (reading configuration).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the engine"
  `bin\XR_3DA.exe` (1,636,424 bytes, MD5 `ba3ad0127bff6619ed991a7c82d8b946`), "the binding library" `bin\xrLUA.dll`
  (290,816 bytes, MD5 `a65ba53867962afc92e0f17409e9cd1f`) [U-1]. **The listing** is `scripts\lua_help.script`
  (scoping spec 2.1), a guide only.
- A call is written as scripts write it. A **number** is the runtime's double; a **text** a Lua string. A number
  the engine wants as an integer is **made an integer** (T's conventions); one it wants as a float is stored as
  a 32-bit float (E 2.2).
- **The player's manager** is the camera manager of the player (VP 1); every `level` effect function of this
  spec and `effector:start`/`finish` act on it through the player's object without testing that it exists, so
  called while there is no player (no level loaded) they read through a missing object (**believed**: the
  program crashes) [U-8, U-4].
- *dt* is the device's frame time `f_time_delta` in seconds and *now*ₛ its global timer in seconds (B 3.3; the
  engine keeps both on the device, the second as the global timer's elapsed seconds) [U-5].
- **Fatal** means the program reports the failure and ends; a failed **assertion** is fatal, as in D 2.1.
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (from game
  data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [U-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-effects-and-weather/run_all.py` (`python3 -I run_all.py`): it reads the binaries' bytes, the
evidence of read-only headless static analysis (produced by `gh.py` beside it; it also reads the evidence of the
checks of N and of the colour-animation format that it cites), the listing, the scoping spec's scan and the
shipped data extracted by `extract.py`. All checks pass (`ALL PASS 19`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 44 listed items [U-2]:

| Owner | Items |
|---|---|
| `effector` | the class; its constructor; `start`, `process`, `finish` (section 2) |
| `effector_params` | the class; its constructor; the properties `blur`, `gray`, `dual`, `noise`, `color_base`, `color_gray`, `color_add`; `assign` (section 3) |
| `level` | `add_pp_effector`, `remove_pp_effector`, `set_pp_effector_factor` (section 4), `add_complex_effector`, `remove_complex_effector` (section 5), `add_cam_effector`, `add_cam_effector2`, `remove_cam_effector` (section 6), `set_weather`, `get_weather`, `set_weather_fx`, `is_wfx_playing`, `rain_factor`, `environment` (section 7) |
| `color_animator` | the class; `load`, `calculate`, `length` (section 8) |
| `particles_object` | the class; `play`, `play_at_pos`, `stop`, `stop_deffered`, `playing`, `looped`, `move_to`, `load_path`, `start_path`, `stop_path`, `pause_path` (section 9) |
| `particle_params` | the class (section 10) |

and further: the two environment classes the listing lacks, with their members (7.6); the `effector` overrides of
the scoping spec 4.5 row U (2.3); the camera effect's function name (6.4). The one candidate string the scoping spec
assigns, `common`, is not a registration of this spec: the game library refers to it only in the routines that read
the section `[common]` of `config\script.ltx` for the class registrators (spec C) and the game-type function
(spec D) [U-18]. The classes `color`, `noise` and `duality`, registered by the same code as `effector`, are spec E's
(E 1.1). Of this spec's names the loadable scripts use 31; counted by the scoping spec's upper rule, 33 loadable
scripts (46 of all scripts) use them [U-2].

### 1.2 As registered (static)

- `effector`: no base class, one constructor of two numbers, the methods `start`, `finish` and `process` [U-3].
- `effector_params`: no base, one constructor without arguments, seven properties, each readable and writable, and
  `assign` [U-3]. The listing agrees in names and forms for both classes [U-3].
- `color_animator`: one constructor of a text; `load`, `calculate`, `length` [U-15].
- `particles_object`: one constructor of a text and the 11 methods of 1.1, each in one form [U-16]; the listing
  agrees [U-16].
- `particle_params`: four constructors, no other member [U-17].
- The 14 `level` functions are registered in the namespace `level` (N 1.2), `set_pp_effector_factor` in two forms,
  each other in one; their forms are given with each function [U-8].
- No class of this spec has a property the listing lacks other than those of the two environment classes (7.6)
  (the scoping spec's gap 2) [U-3, U-14, U-16].

## 2. `effector`: screen effects defined by scripts (static)

A script makes a screen effect by deriving a script class from `effector` (A 1.3), constructing it with `super`
and **starting** it. The started effect is one of the player's manager's **screen effects**, the list FP 2.2
step 2 combines; this section says what an effect of this class does there.

### 2.1 Construction: `effector(number, life)`

`super(n, life)` (A 3.1) makes the engine part of the script object: a screen effect with the **number** *n* (made
an integer) and the **remaining life** *life* (a 32-bit float, in seconds), not yet in any list [U-4]. The engine
part is marked **not freed on removal**: removing it from the player's manager never deletes it (2.4) [U-4]. Every
shipped script class derived from `effector` passes two numbers [U-19]; they use the numbers 1001, 1003 and others
taken from their own data, and lives of 10,000,000 s, except `_g.script`'s `pp_effector` and `pp_linear_lerp`, which
pass a time in milliseconds as the life (measured, `scripts\_g.script`).

### 2.2 `e:start()` and `e:finish()`

- `e:start()` adds the effect to the player's manager. Adding a screen effect first removes the first screen effect
  that has the same number (2.4); so a started effect replaces any earlier effect of its number, whatever its kind
  [U-4, U-5].
- `e:finish()` removes, from the player's manager, the first screen effect whose number is *n* (2.4) — normally `e`
  itself, but any effect of that number if `e` was not in the list [U-4].
- Both calls hand the script object to the engine: the object stops being owned by the script side, and its engine
  part takes a strong reference to it [U-4]. Since a removed effect of this class is never deleted (2.1), that
  reference is never released, so a started or finished effector object is never collected (**believed**:
  consequence of the two rules; the reference is released only when the engine part is destroyed) [U-4].

### 2.3 The override `process(pp)`

This is the entry point of the scoping spec's 4.5 row U. Each time the player's manager processes its screen
effects (2.4) and the effect is valid, the engine calls the script class's `process` as A 5.2 calls an override,
with the object and **pp** [U-6, U-5]:

- **pp** is an `effector_params` value that refers to the engine's record for this effect in this processing (not a
  copy and not owned by the script); every field starts at 0 (section 3's fields, all ten values 0) [U-5, U-6].
  Assigning to `pp`'s properties, or `pp:assign(p)`, sets the effect's values for this processing. The reference is
  valid only during the call (**believed**: the record is the manager's for that one effect).
- The result is read as a truth value (nil and false are false) [U-6]. False ends the effect: it is removed (2.4).
- An error in the call is fatal (A 5.3) [U-6].
- If the class does not override `process`, the engine's own is used (A 5.2); it is also what the base call
  `effector.process(self, pp)` runs (A 6.1): it lowers the remaining life by *dt* and returns true, and leaves `pp`
  unchanged [U-3, U-5].

The shipped overrides all call `effector.process(self, pp)` and return true; `sr_postprocess.script`,
`sr_aes_deadzone.script`, `postprocess.script` and `level_psy_antenna.script` fill `pp` with `pp:assign(self.params)`
from an `effector_params` they keep and change [U-19] (measured, the scripts).

### 2.4 The player's manager and its screen effects (static)

The player's manager keeps an ordered list of screen effects; every adder of this spec uses it [U-5]. In each camera update (VP 1), for each effect **from the last in the list to the first** [U-5]:

1. the effect's values start as all ten 0 (not the identity);
2. if the effect is not **valid** (its remaining life is not above 0; section 4.2 gives the animated effect's own
   rule), or its processing (for this class, 2.3) returns false, it is removed from the list: the first effect of
   its number is taken out, and deleted unless it is marked not freed on removal (2.1);
3. otherwise the effect's ten values minus the identity's (FP 2.1) are added to the running set, which started
   from the identity.

When no effect added anything, the set is the identity; a negative noise grain is replaced by the identity's 1
(FP 2.2 steps 3 – 4) [U-5]. So an effect contributes its values as differences from the identity: a field it leaves
at 0 subtracts the identity's value (for example `color_base` 0 subtracts 0.5) [U-5].

Removing by number takes the first effect in list order whose number matches; adding one first removes that way,
then appends [U-5].

## 3. `effector_params` (static)

A record of the ten screen-effect values of FP 2.1 [U-3, U-7]:

| Property | Type | Values (FP 2.1 names) |
|---|---|---|
| `blur` | number (32-bit float) | blur |
| `gray` | number | gray |
| `dual` | `duality` (E 6.4) | dual.h, dual.v |
| `noise` | `noise` (E 6.3) | noise.intensity, noise.grain, noise.fps |
| `color_base` | `color` (E 6.2) | color_base r, g, b |
| `color_gray` | `color` | color_gray r, g, b |
| `color_add` | `color` | color_add r, g, b |

- `effector_params()` makes a record holding the engine's **default record**: blur 0, gray 0, dual (0, 0), noise
  (intensity 0, grain 1, fps 10), color_base (0.5, 0.5, 0.5), color_gray (0.333, 0.333, 0.333), color_add (0, 0, 0)
  [U-7]. It differs from the identity (FP 2.1: noise.fps 30) only in noise.fps, so an effect whose values are a
  default record contributes −20 to noise.fps and nothing else [U-5, U-7].
- Reading `blur` or `gray` gives the stored float exactly; assigning converts the value with Lua's number conversion
  without a type test (a non-number gives 0) and stores it as a 32-bit float, as E 2.4 [U-7].
- Reading `dual`, `noise` or a colour property gives a **new** object holding a copy of the values (owned by the
  script): changing it does not change the record, so `pp.dual.h = 1` has no effect and a script must assign the
  whole value, as `_g.script` does (`pp.dual = dual`) [U-7, U-19]. Assigning one copies its values into the record
  [U-7]. What assigning a value of another type (or nil) does is **unestablished**.
- `p:assign(q)` copies all ten values (the whole record) of `q` into `p` [U-3].

## 4. Animated screen effects: `add_pp_effector`, `set_pp_effector_factor`, `remove_pp_effector` (static)

### 4.1 `level.add_pp_effector(file, n, cyclic)`

Arguments: a text, a number made an integer, a boolean; no result [U-8]. It makes an **animated effect** with the
number *n*, marked freed on removal, loads it from the effect file and adds it to the player's manager (2.4: any
effect of number *n* is removed first) [U-9]:

1. The file `file` is looked for in the running level's folder (`$level$`), then in the game's animation folder
   (`$game_anims$`, `anims\`); if neither has it, fatal `Can't find motion file '%s'.` [U-9].
2. If the found file's name has a `.` and the part from the last `.` is not exactly `.ppe`, fatal `ERROR: Can't
   support files with many animations set. Incorrect file.` [U-9]. With `.ppe` the file's ten value curves are read
   (their layout and evaluation are the proposed specification `ppe-format` of `docs/behavior/hud-scope.md`, its
   item 19; **unestablished** here); what a name without `.` does is **unestablished** (believed: nothing is read and
   every curve is empty).
3. The effect's **length** *L* is the greatest length of its ten curves [U-9]. Not cyclic: the remaining life is *L*;
   cyclic: 100,000 s, reset to 100,000 at every processing, so a cyclic effect never runs out [U-9, U-10].

The shipped loadable scripts pass names with `.ppe`, all of which exist in `anims\`: `blink.ppe`, `dead_zone.ppe`,
`nightvision_bad.ppe`, `nightvision_good.ppe`, `proba.ppe`, `radar_psi.ppe`, `teleport.ppe`, `vibros.ppe`,
`vibros_p.ppe`; `level_weathers.script` passes the level's `postprocess` value from `game.ltx` with number 999,
cyclic [U-19].

### 4.2 An animated effect over time

Each processing (2.4), in order [U-10]:

1. Cyclic: the remaining life is set to 100,000. Then the remaining life is lowered by *dt*.
2. The **start time** *s* is set to *now*ₛ the first time; a cyclic effect whose local time *now*ₛ − *s* exceeds *L*
   moves *s* on by *L*. The ten curves are evaluated at *now*ₛ − *s* (`ppe-format`).
3. The **factor** *k* moves: normally *k* ← *k* + (*target* − *k*) · *dt* · *rate*; while **stopping**, *k* ← *k* −
   *dt* · *rate*. Then *k* is limited to 0.0001 … 1.
4. The effect's values are lerp(identity, the curves' values, *k*) (each value: identity + (curve − identity) · *k*).
5. If the resulting noise grain is not above 0: a failed assertion `noise.grain cant be zero! see postprocess`, with
   the file's name (fatal).
6. The processing returns false, so the effect is removed (2.4), when |*k* − 0.0001| < 10⁻⁷.

It is **valid** while it is cyclic or its remaining life is above 0 [U-10]. A new effect has *k* = 1, *target* = 1,
*rate* = 1 and is not stopping [U-10]. So a non-cyclic effect plays once over *L* seconds and is removed; a cyclic
one plays until stopped. (Which values the identity and the curves hold beyond FP 2.1 is `ppe-format`'s.)

### 4.3 `level.set_pp_effector_factor(n, f)` and `level.set_pp_effector_factor(n, f, rate)`

Find the first screen effect of number *n*; if there is one and it is an animated effect (4.1, 5), set [U-10]:

- `(n, f)`: *k* = *target* = *f* (a jump);
- `(n, f, rate)`: *target* = *f* and *rate* = `rate`; *k* then moves toward *f* by step 3 of 4.2, by the fraction
  *dt* · *rate* of the remaining distance each processing (an exponential approach).

Otherwise nothing happens [U-10]. A factor at or below about 0.0001 ends the effect at its next processing (4.2 step
6); the shipped scripts set 0.01 just after adding (`xr_detector.script`, `sr_psy_antenna.script`) and then move it
with rate 0.3 [U-19].

### 4.4 `level.remove_pp_effector(n)`

Finds the first screen effect of number *n*; if it is an animated effect not already stopping, it starts
**stopping** with *rate* = 1: *k* falls by *dt* each processing, so from *k* = 1 the effect fades out over about 1 s
and is then removed (4.2) [U-10]. An effect of another kind (an `effector`, 2) is not affected; nothing happens
when there is none [U-10].

## 5. Complex effects: `add_complex_effector(section, n)` and `remove_complex_effector(n)` (static)

`level.add_complex_effector(section, n)` (a text and a number made an integer) reads the section `section` of the
game's configuration (`system.ltx` and its includes) [U-11]:

- if it has the key `pp_eff_name`: an animated effect (4.1) of number *n*, cyclic when the boolean `pp_eff_cyclic`
  is true, loaded from the file `pp_eff_name` names, added to the player's manager;
- then, if it has the key `cam_eff_name`: a camera effect (6.1) of number *n*, cyclic when `cam_eff_cyclic` is
  true, in the relative mode, from the animation file `cam_eff_name` names, added to the player's manager.

A section without either key adds nothing. Reading a boolean key that is missing behaves as ltx-config states for a
missing key (**believed** fatal). `level.remove_complex_effector(n)` removes the camera effect of number *n* (6.3)
and the first screen effect of number *n* (2.4) [U-11]. No shipped script calls either [U-19].

## 6. Camera effects (static)

### 6.1 `level.add_cam_effector(file, n, cyclic, f)` and `level.add_cam_effector2(file, n, cyclic, f)`

Arguments: a text, a number made an integer, a boolean, a text (a script function's name, or empty); result a
number [U-8]. Both make a **camera effect** of number *n* and add it to the player's manager; `add_cam_effector2`
makes it **absolute**, `add_cam_effector` **relative** [U-12]:

1. The animation file `file` is looked for in `$level$`, then `$game_anims$`; if neither has it, fatal `Can't find
   motion file '%s'.` [U-12]. A name ending in `.anm` holds one motion, one ending in `.anms` several; the file's
   motions (their format: **unestablished** here, no approved specification) are loaded; a file that cannot be read
   as a motion is fatal `ERROR: Can't load motion. Incorrect file version.` [U-12].
2. The first motion is played, looped when `cyclic` is true; without any motion, fatal (a message that the cycle
   `%s` was not found) [U-12].
3. The motion's **length** *L* = (last frame − first frame) / its frame rate, in seconds, becomes the remaining life
   and is the result [U-12].

### 6.2 What a camera effect does

The player's manager keeps its camera effects in a list it applies as VP 1 step 3 states: from the last added to
the first [U-12]. A camera effect added during an update enters the list at the end of that update, after any effect
of the same number already there has been removed and deleted; so adding replaces the effect of that number one
update later [U-12]. In each update, for each effect [U-12]:

1. If it is **valid** — cyclic, or its remaining life is above 0 — it is processed: the remaining life is lowered by
   *dt*, the motion advanced by *dt*, then (valid still) the camera is changed:
   - **relative**: the animation's position is added in the camera's own frame (along its right, up and direction
     vectors), and the animation's direction and up are turned the same way, so the motion moves and turns the view
     from where it is;
   - **absolute**: the camera's position, direction and up become the animation's.
2. An effect that is not valid at step 1 (or whose processing reports false) is removed and deleted. An absolute
   effect first applies the animation's position, direction and up once more (so the view ends on the last pose).

A non-cyclic effect thus moves the camera for *L* seconds; a cyclic one until removed. Only position and orientation
change (VP 3.5).

### 6.3 `level.remove_cam_effector(n)`

Removes and deletes the first camera effect of number *n* among those already in the list [U-12]. An effect added in
the same update (6.2: not yet in the list) is not removed [U-12].

### 6.4 The function `f` (entry point)

When a camera effect made by 6.1 is found not valid (6.2 step 2) and `f` is not empty, the engine finds the function
named `f` as D 1.1 resolves names (`namespace.function` or a global) [U-12]:

- if it is not found as a function: a failed assertion (fatal);
- otherwise it is called once with no arguments, its results ignored, in the protected call of D 1.2 (an error is
  fatal) [U-12].

Then `f` is cleared, so it is called at most once, and the effect is removed [U-12]. A cyclic effect is never found
not valid, so its `f` is never called [U-12]. `f` is not called when the effect is removed by
`remove_cam_effector` (6.3) or replaced (6.2) [U-12]. The one shipped name is `sr_cutscene.effector_callback`
(`sr_cutscene.script`, with `add_cam_effector2`); every other shipped call passes `""` [U-19].

The shipped loadable scripts pass `camera_effects\agroprom_doctor_cam.anm`, `camera_effects\earthquake.anm` and
`camera_effects\fusker.anm`, which exist in `anims\`, and, in one call of `xr_effects.script` with only two
arguments, `camera_effects\test.anm`, which does not exist (`anims\test.anm` does); that call matches no form, with
the outcome of A 7.3 [U-19].

## 7. Weather (static)

The environment keeps: the **weather cycles** of `[weathers]` and the **weather effects** of `[weather_effects]` of
`config\weathers\environment.ltx` (each a list of weather sections ordered by their time keys, sky.md 3.2); the
**current list** (the cycle or effect in force) and its **name**; the **set weather** (the cycle to return to); the
current pair A, B and the blend weight *t* (SS 9.2); whether an effect **plays** and its **remaining time**; the
time of day τ and the time factor *F* the level hands it each frame (H 3.2) [U-13]. This edition has 13 cycles and 2
effects, `surge_day` (sections at 00:01:00 … 00:05:00) and `p_surge_day` [U-19].

**Each frame** (with a level loaded): if an effect plays and its remaining time is not above 0, it ends (7.3); then
the pair is updated: when τ has passed B's time (with the wrap at 24 h), A becomes the old B and B the first section
of the current list whose time is after τ (the first of the list when none is) [U-13]. A section's time is its key
`hh:mm:ss` in seconds of the day; a key outside 0:0:0 … 23:59:59 is a failed assertion `Incorrect weather time`
[U-14]. Handing the environment a new τ lowers the remaining time of a playing effect by the game time passed
(wrapped at 86,400 s) [U-13].

### 7.1 `level.set_weather(name, now)`

A text and a boolean; no result [U-8]. In order [U-13]:

1. An empty name is fatal `! Empty weather name`.
2. A name that is not a weather cycle logs `! Invalid weather name: %s` and changes nothing.
3. The set weather becomes `name`.
4. If `now`: a playing effect stops at once (no return to a weather is made, 7.3) and the pair is cleared.
5. If no effect plays: the current list and name become the cycle's.
6. If `now`: the pair is chosen at once for τ from the current list: A = the last section at or before τ and B the
   next (wrapping at the ends) (**believed** from the selection rule applied to an empty pair).
   **Disputed in review:** this is established statically, not believed: for an empty pair, B is the first section of
   the current list whose time is after τ (the first of the list when none is) and A the section before B (the last
   of the list when B is the first). The same selection runs in each frame's update while the pair is empty.

So without `now` the new cycle takes over only at the next change of the pair (each frame's rule): the old B blends
to the new cycle's next section over the rest of that hour (**believed**: consequence of the rule). With an effect
playing and `now` false, only the set weather changes; the effect continues and returns to it (7.3).
`level_weathers.script` calls it with `now` true when the level changed and false once a game day [U-19].

### 7.2 `level.set_weather_fx(name)`

A text; result a boolean [U-8]. If an effect plays, nothing happens and the result is false [U-13]. Otherwise
[U-13]:

1. An empty name is fatal `! Empty weather effect name`; a name that is not a weather effect is a failed assertion
   `Invalid weather effect name.` (fatal).
2. The current list and name become the effect's (so `level.get_weather()` returns the effect's name while it
   plays). With *d* = 5 · *F* (5 real seconds in game seconds), the effect's sections, ordered by their own times
   *e*₀ … *e*ₘ (*m* ≥ 1), are rescheduled:
   - section 0 becomes a copy of the current A, at the time that keeps the present mix: τ − (*d* · (B − A) /
     (B − τ) − *d*), so that at τ the pair (section 0, section 1) has the weight A, B had;
   - section 1 becomes a copy of the current B, at τ + *d*;
   - each section *i* = 2 … *m* − 1 keeps its values, at τ + *d* + *e*ᵢ;
   - the weather's sections that follow the effect are chosen from the list in force before: X₀ = the first
     section after the time of section *m* − 1, X₁ the first after X₀'s time + 0.5 s; section *m* becomes a copy of
     X₀, at the time of section *m* − 1 plus *d*;
   all times wrapped into 0 … 86,400 s (A, B, τ in seconds of the day).
3. The remaining time becomes the time of section *m* minus τ (wrapped); the effect plays; A, B become sections 0
   and 1; the result is true.

So the screen blends from the weather into the effect over *d*, plays the effect's middle sections at their own
offsets, and blends back into the weather over *d*. The effect's sections 0, 1 and *m* are overwritten by these
copies, so their own values are never shown [U-13]. Worked example: `surge_day` (sections at 60, 120, 180, 240,
300 s) with *F* = 10 (`config\alife.ltx time_factor`) gives *d* = 50 s: copy of A, copy of B at τ + 50, `v_03` at τ +
230, `v_02` at τ + 290, copy of X₀ at τ + 340, remaining time 340 game seconds (34 real seconds) [U-19, U-13].

### 7.3 The end of an effect

When the remaining time is not above 0 (each frame, 7): the effect no longer plays; `set_weather(the set weather,
false)` runs (7.1, so the current list and name become the set weather's); A, B become X₀, X₁ of 7.2 [U-13].

### 7.4 `level.get_weather()` and `level.is_wfx_playing()`

- `get_weather()` returns the current name (a cycle's, or an effect's while it plays); nil when none was ever set
  (**believed**: the runtime gives nil for a missing text) [U-13].
- `is_wfx_playing()` returns true while an effect plays [U-13].

### 7.5 `level.rain_factor()`

Returns the blended rain density ρ = lerp(A.`rain_density`, B.`rain_density`, *t*) of the last frame's blend
(sound-rain-thunder.md 2.3; each section's value limited to 0 … 1 when read) [U-14]. `gulag_bar.script` and
`sound_theme.script` compare it with 0.07 [U-19].

### 7.6 `level.environment()` and the two environment classes

These do not use the level: they work whenever the program runs (**believed**: the environment belongs to the
program, not to a level) [U-8, U-14].

- `level.environment()` returns the environment as an object of the **environment class**, a registered class the
  listing lacks, with one method and no constructor [U-8, U-14].
- `env:current()` returns the environment's **blended values** of the last frame as an object of the **descriptor
  class** (also registered, unlisted, no constructor) with two read-only number properties [U-14]:
  - `far_plane`: the blended far plane (lerp + modifiers) × `rs_vis_distance` / (1 + Σ weights) (SS 9.2,
    VP 2.1);
  - `fog_density`: the blended fog density (lerp + modifiers) / (1 + Σ weights) (SS 9.2).

Both objects refer to the environment's own data (believed: no copy is made). No shipped script calls
`environment()` [U-19]; the classes' registered names are used by no script.

## 8. `color_animator` (static)

- `color_animator(name)` and `a:load(name)` look the colour animation `name` up in the library (CA 2.5); if none is
  found, a failed assertion `Can't find color anim:` with the name (fatal) [U-15]. `load` replaces the entry.
- `a:calculate(t)` returns a new `fcolor` (E) holding the entry's value at time *t* seconds (CA 3: wrapped at the
  length, whole frames, interpolated), each channel byte × 1/255 (the 32-bit float of E's `fcolor:set(number)`):
  r, g, b from the red, green, blue bytes and a from alpha [U-15].
- `a:length()` returns *N* / *fps* × 1000 (in 32-bit float) as an integer number of milliseconds, the fraction cut
  [U-15].

The only shipped user, the non-loadable `level_psy_antenna.script`, uses the entries `levels\psy_antenna\starting`,
`working` (5 frames per second, 100 frames: 20,000 ms) and `stopping` (10, 100: 10,000 ms), all in `lanims.xr`, and
passes milliseconds / 1000 to `calculate` [U-15, U-19].

## 9. `particles_object` (static)

A particle object made by a script: one effect or group instance of P2/P6 with the placement, schedule, updates and
visibility sphere of P8 sections 2 and 5, plus an optional **path** [U-16].

### 9.1 `particles_object(name)`

Makes the instance from `name` as P8 section 2 does (an effect of `particles.xr`, else a group) (**believed**: the
same creation request) [U-16]. It is not removed automatically when it ends [U-16]. It is **looped** when its length
(P2 7, P6) is not above 0 [U-16]. It is registered with the schedule of P8 5.2, its last-update time set to the clock
(P8's clock, the device's `time_global()`) [U-16]. Its placement is the identity until placed.

### 9.2 Play, place and stop

- `p:play()`: plays the instance (P2 7, P6), sets the last-update time to the clock − 33 and updates it at once with
  the elapsed time (33 ms when the clock did not move; P8 5.1), then the visibility sphere (P8 5.4); the object is
  no longer marked stopped [U-16].
- `p:play_at_pos(pos)`: first places the instance (P2 5.1) by the identity matrix with translation `pos`, velocity
  (0, 0, 0), in world mode; then as `play` [U-16].
- `p:move_to(pos, vel)`: places it by the identity with translation `pos`, velocity `vel`, world mode, then updates
  the visibility sphere; it does not play [U-16].
- `p:stop()`: stop, immediate (P2 7); `p:stop_deffered()`: stop, deferred; both mark the object stopped [U-16].
- `p:playing()`: the instance's playing state (P2 7) [U-16]; `p:looped()`: the flag of 9.1 [U-16].

### 9.3 Path

- `p:load_path(name)`: gives the object an animator if it has none; if the animator already holds `name`, nothing
  else; otherwise it clears it and loads the animation file `name` as 6.1 step 1 (same folders, endings and fatal
  messages) [U-16].
- `p:start_path(looped)` plays the animator's first motion (6.1 step 2; fatal without a motion), `p:stop_path()`
  stops it, `p:pause_path(b)` pauses or resumes it [U-16]. Calling them before `load_path` reads through no object
  (**believed**) [U-16].
- In each scheduled update (P8 5.2), after the update of P8 5.1: if the object has an animator, it is advanced by the
  scheduler's elapsed time *e* ms (as *e* · 0.001 s), and the instance is placed by the animator's matrix with the
  velocity (new position − old position) / (*e* · 0.001), world mode; then the visibility sphere is updated [U-16]. A
  stopped or paused path leaves the placement as it was (**believed**: the animator's matrix does not change).

### 9.4 End

When the runtime collects a particles object, its instance is destroyed (**believed**: removed at the engine's next
clean-up of destroyed instances) [U-16]. If the engine destroys the instance first (with its level), the script
object no longer has one, and every member then reads through no object (**believed**) [U-16]. When the collector
runs is the runtime's (A); so an object nothing keeps can stop at any collection (**believed**).

## 10. `particle_params` (static)

A record of three vectors *a*, *b*, *c* (E's `vector`): `particle_params()` all (0, 0, 0); `particle_params(a)`,
`particle_params(a, b)`, `particle_params(a, b, c)` set the given ones in order and the rest to (0, 0, 0) [U-17]. It
has no member scripts can read; its only use is as an argument of spec V's `particle` (the listing's `particle`
constructors take one), which owns what the vectors mean [U-17]. The shipped scripts construct it in
`particle_flame.script` (non-loadable) [U-19].

## 11. Use by the shipped scripts (measured)

- `effector` subclasses: `_g.script` (`pp_effector`, `pp_linear_lerp`), `postprocess.script`, `sr_postprocess.script`,
  `sr_aes_deadzone.script` among loadable scripts; also `level_psy_antenna.script`, `xr_radar.script` and the file `scripts\.script` [U-2, U-19].
- `level`: `add_pp_effector` 8 loadable scripts, `remove_pp_effector` 5, `set_pp_effector_factor` 2,
  `add_cam_effector` 1 (`xr_effects.script`), `add_cam_effector2` 2, `remove_cam_effector` 1, `set_weather` 1
  (`level_weathers.script`), `set_weather_fx` 3 (`surge_day` only), `is_wfx_playing` 1 (`xr_conditions.script`),
  `rain_factor` 2; `add_complex_effector`, `remove_complex_effector`, `get_weather`, `environment` none [U-2, U-19].
- `particles_object`: `bind_physic_object.script`, `sr_particle.script` (with the path members), `xr_effects.script`
  [U-2].
- `color_animator` and `particle_params`: only non-loadable scripts (8, 10).

## 12. Verification facts

- Items: 44 listed (5 classes; 8 `effector_params`, 3 `effector`, 3 `color_animator`, 11 `particles_object` members;
  14 `level` functions); candidate `common` not a registration; 31 names used, by 33 loadable scripts (46 in all) by
  the upper rule [U-2, U-18].
- Default record: noise.fps 10 against the identity's 30 [U-5, U-7].
- Animated effect: factor limited to 0.0001 … 1, removal when within 10⁻⁷ of 0.0001; cyclic life 100,000 s;
  `remove_pp_effector` rate 1 [U-10].
- Weather effect blend time *d* = 5 × the time factor; `surge_day` remaining time 340 game seconds at factor 10
  [U-13, U-19].
- `color_animator:length()` of `levels\psy_antenna\starting`: 20,000 [U-15].

## 13. Unestablished

- The `.ppe` curves and their evaluation (`ppe-format`), and the camera animation format (`.anm`, `.anms`).
- What a `.ppe` name without a `.` does (4.1); what assigning a non-record to `dual`, `noise` or a colour property
  does (3).
- Whether a missing boolean key in a complex-effect section is fatal (5).
- What `get_weather()` returns before any weather is set (7.4); whether the environment and descriptor objects are
  references (7.6).
- When the engine destroys a particle instance a script still holds, and when collected instances are cleared (9.4).
- The behaviour of `set_weather(name, true)` relies on the pair selection rule applied to an empty pair (7.1).
  **Disputed in review:** established; see 7.1 step 6.

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll`, `XR_3DA.exe` and
  `xrLUA.dll` as identified above; the winning copies of `config\*.ltx`, `anims\*` and `scripts\*` of the archives
  `gamedata.db0` … `gamedata.dbd`, extracted with the project's archive tool; `lanims.xr` as extracted for the
  approved colour-animation specification.
- Analysis: static analysis of the three binaries with Ghidra (headless, read-only: decompilation and disassembly of
  the registration of this spec's classes and `level` functions, their implementations, the camera manager's
  processing of screen and camera effects, the animated screen effect, the camera effect, the weather functions and
  the environment's per-frame selection, loading and blending of weather sections, the colour animator, the particle
  object; constants read from the files); a parser of the listing; the scoping spec's scan of the scripts. No binary
  was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

1. `set_weather(name, true)` (7.1 step 4) also clears one state of the sun's lens flare when the environment has
   one; which state, and what it changes on screen, is not stated.
2. `set_weather_fx` (7.2): the wrap rules of the differences in the time of section 0 are not stated: B − A is taken
   across midnight (86,400 − A + B when A is after B), while B − τ is used as is, without wrapping.
3. `set_weather_fx` with a weather effect of fewer than two sections: the outcome is not stated.
4. The animated screen effect (4.2): what is done to the evaluated curve values before the lerp of step 4 is not
   stated and not clearly assigned to `ppe-format`: the colour values are offset by the identity's colour values, the
   noise values take the identity's when their curves give none, and the noise fps value is scaled by a constant.
