# Script API V: scripted actions

What the scripted-action part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition), as scripts see it: the class `entity_action` (one scripted action) and its eight part classes `move`,
`look`, `anim`, `sound`, `particle`, `object`, `cond` and `act`; the members of `game_object` that put an object
under script control, queue actions for it and read its queue; and how the engine runs the queue: in which order it
applies an action's parts, when a part and an action are done, and what each kind of object does with each part as
far as this interface reaches. This is spec V of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6, order 27); its scope rule and its review gaps apply: this spec owns
every name of these classes as the game library registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type, the 250
  loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched,
  calls of missing members, what a mismatch does); spec B, `docs/behavior/script-api-engine-services.md` (its 3.3:
  the device's `time_global()`, in milliseconds); spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md`
  (class ids, their names and the classes they create); spec E, `docs/behavior/script-api-value-types-and-math.md`
  (`vector`, its 2.2 argument conversions); spec M, `docs/behavior/script-api-game-object-basics.md` (the handle
  and its engine object, 2.1; the **kinds** of object, 2.2; what a member does on an object of the wrong kind and
  the diagnostics routine that returns at once, 2.3); spec O, `docs/behavior/script-api-object-event-callbacks.md`
  (the callback ids 18 – 23, its 3.2 and 4); spec T, `docs/behavior/script-api-sound.md` (`sound_object`, the AI
  sound types of `snd_type`, the conversion "made an integer" of its conventions).
- `docs/formats/sound-ogg-params.md` (how a sound name finds its file, 2).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [V-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from
  the game library's registration code.
- A call is written as scripts write it: `obj:command(act, false)`. A **number** is the runtime's double; a **text** a
  Lua string; **nil** Lua's nil; a **handle** a `game_object` (M 2.1) and its **object** the engine object it refers
  to. A number given where the engine wants an integer (a constant, a flag set, a time) is **made an integer** as in
  T's conventions (**believed** the same routine); one given where it wants a float is stored as a 32-bit float.
- **Now** is the device's `time_global()` (B 3.3), in milliseconds [V-8].
- A **part** is one object of a part class; each part has a **done** flag, which its `completed()` returns [V-3].
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (from game
  data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [V-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-scripted-actions/run_all.py` (`python3 -I run_all.py`): it reads the binary's bytes and run-time
type information, the evidence of read-only headless static analysis (produced by `gh.py` beside it), the
listing, the scoping spec's scan and the shipped scripts. All checks pass (`ALL PASS 22`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 147 listed items [V-2]:

| Owner | Items |
|---|---|
| `move` | the class; 28 constants; 8 functions `body`, `move`, `path`, `object`, `patrol`, `position`, `input`, `completed` (section 4) |
| `look` | the class; 7 constants; 5 functions `type`, `object`, `bone`, `direct`, `completed` (section 5) |
| `anim` | the class; 12 constants; 3 functions `anim`, `type`, `completed` (section 6) |
| `sound` | the class; 9 constants; 6 function names `set_sound`, `set_position`, `set_bone`, `set_angles`, `set_sound_type`, `completed` (section 7) |
| `particle` | the class; 6 functions `set_particle`, `set_position`, `set_bone`, `set_angles`, `set_velocity`, `completed` (section 8) |
| `object` | the class; 21 constants; 3 function names `object`, `action`, `completed` (section 9) |
| `cond` | the class; 7 constants (section 3.3) |
| `act` | the class; 4 constants (section 10) |
| `entity_action` | the class; 10 function names `set_action`, `move`, `look`, `anim`, `sound`, `particle`, `object`, `time`, `all`, `completed` (section 3) |
| `game_object` | 9 functions: `script`, `get_script`, `get_script_name`, `can_script_capture` (section 2.2), `command`, `action`, `action_count`, `action_by_index`, `reset_action_queue` (section 2.3) |

The scoping spec assigns this spec no unlisted name and one candidate string, `class`; it is not a registration: the
code referencing it reads the configuration key `class` of a section [V-2]. Of these names the loadable scripts use
65, in 30 loadable scripts counted by the scoping spec's lower rule (106 by its upper rule) [V-2, V-22].

### 1.2 As registered (static)

All nine classes are registered with no base class [V-3]. Their constructors, by argument count, are exactly the
listing's forms in the listing's order (`move` 21, `look` 7, `anim` 5, `sound` 17, `particle` 6, `object` 5, `cond`
3, `act` 3, `entity_action` 2) [V-3]. Every listed function is registered and no other function is [V-3]. The 88
constants have exactly the listing's names and values [V-3]:

| Class | Constants |
|---|---|
| `move` | body states `crouch` 0, `standing` 1; movement types `walk` 0, `run` 1, `stand` 2; path types `line` 0, `dodge` 1, `criteria` 2, `curve` 0, `curve_criteria` 2; input keys `none` 1, `fwd` 2, `back` 4, `left` 8, `right` 16, `up` 32, `down` 64, `handbrake` 128, `on` 256, `off` 512; monster moves `walk_fwd` 0, `walk_bkwd` 1, `run_fwd` 2, `drag` 3, `jump` 4, `steal` 5; monster speed `default` 0, `force` 1 |
| `look` | `cur_dir` 0, `path_dir` 1, `direction` 2, `point` 3, `danger` 5, `search` 6, `fire_point` 10 |
| `anim` | mental states `danger` 0, `free` 1, `panic` 2; monster animations `stand_idle` 0, `sit_idle` 1, `lie_idle` 2, `eat` 3, `sleep` 4, `rest` 5, `attack` 6, `look_around` 7, `turn` 8 |
| `sound` | monster sounds `idle` 1, `eat` 2, `attack` 3, `attack_hit` 4, `take_damage` 5, `die` 7, `threaten` 9, `steal` 10, `panic` 11 |
| `object` | `switch1` 0, `switch2` 1, `reload` 2, `reload1` 2, `reload2` 3, `aim1` 4, `aim2` 5, `fire1` 6, `fire2` 7, `idle` 8, `strap` 9, `drop` 10, `activate` 15, `deactivate` 16, `use` 17, `turn_on` 18, `turn_off` 19, `show` 20, `hide` 21, `take` 22, `dummy` −1 |
| `cond` | `move_end` 1, `look_end` 2, `anim_end` 4, `sound_end` 8, `object_end` 32, `time_end` 64, `act_end` 128 |
| `act` | `rest` 0, `eat` 1, `attack` 2, `panic` 3 |

The grouping in the first column names which argument or field the constants are given for (sections 4 – 10); the
constant names are the engine's [V-3].

## 2. Objects under script control

### 2.1 Which objects run scripted actions (static)

The members of 2.2 and 2.3 apply to objects of one further kind, called **scripted** here, not in M 2.2's table:
the 19 creature classes of M 2.2's entity row (`CAI_Stalker`, `CAI_Trader` and the 17 monster classes), `CCar`, and
the objects of class ids `O_SEARCH` (section `projector`) and `SCRPTOBJ` (section `script_object`) of C's table, whose
classes scripts cannot name [V-4]. The player, the helicopter, items, physics objects and the others are not
scripted [V-4]. On an object that is not scripted, each member of 2.2 and 2.3 does nothing, passes a diagnostic to
M 2.3's routine and returns: `get_script` and `can_script_capture` `false`, `get_script_name` the empty text,
`action` nil, `action_count` 0, `action_by_index` nil [V-5].

A scripted object keeps: a **control** flag and a **control name** (a text, empty at first); a **capture** flag; an
**action queue** of `entity_action` objects, first to last; and its own sound for actions (7.3) [V-6, V-7].

### 2.2 Script control (static)

- `obj:script(on, name)` (`on` a boolean, `name` a text) [V-6]:
  - not under control, `on` true: if the capture flag is set, the object comes under control with control name
    `name`; otherwise nothing happens (no diagnostic).
  - under control, `on` false, and the current control name is not empty and equal to `name` (compared
    byte-wise): control ends; the control name becomes empty, the action queue is emptied as by
    `reset_action_queue`.
  - any other case (taking control twice, releasing an object not under control, releasing with another name or
    with an empty current name): the diagnostic "Invalid sequence of taking an entity under script control" is
    passed to M 2.3's routine and nothing changes.
- `obj:get_script()` returns the control flag [V-5, V-6].
- `obj:get_script_name()` returns the control name [V-5, V-6].
- `obj:can_script_capture()` returns the capture flag [V-5]. The flag is set by the routine that also ends control
  and empties the queue (**believed**: the object's reinitialisation when it comes into the level) [V-6]. Engine code of the monsters clears it and sets it again
  (**believed**: while a monster behaviour of the engine holds the monster; which one is **unestablished**) [V-21];
  no member of this spec changes it.

While under control the object runs its queue (2.4). Taking control does not change the object's queue.

### 2.3 The queue (static)

- `obj:command(act, first)` (`act` an `entity_action`, `first` a boolean) [V-7]:
  - `first` false, or the queue empty: a copy of `act` is added at the end.
  - `first` true and the queue not empty: the current (first) action is finished (2.5), replaced by a copy of itself,
    and a copy of `act` is put before it. So `act` runs now and the interrupted action, with its parts' state at
    the moment of interruption, runs after it.
  - If the queue was empty and the object is in the level (spawned), the queue is run once at once (2.4).
  The queue holds copies: changing `act` afterwards changes nothing queued [V-7].
- `obj:action()`: a new `entity_action`, a copy of the current action, or nil when the queue is empty [V-5, V-7].
- `obj:action_count()`: the number of queued actions [V-5].
- `obj:action_by_index(i)`: the queued action at position `i` counted from 0 (the current one is 0), itself, not a
  copy [V-5]. `i` is not checked against the count; an index outside the queue reads past it (**believed**: an
  invalid value or a crash) [V-5].
- `obj:reset_action_queue()`: the current action is finished (2.5), then every queued action is removed; control is
  not changed [V-7].

### 2.4 Running the queue (static)

The engine runs the queue of an object under control in one of the object's periodic updates (**believed**: its
scheduled update) [V-8], and once at the moments 2.3 and 6.2 say. Monsters run it only while alive (health above 0)
[V-18]. One run [V-8]:

1. While the queue is not empty: take the current action. If it is not the action seen last time (a new current
   action), its **start time** becomes now and its "checked" flag and its sound part's "started" flag are cleared.
   Then test whether the action is **complete** (3.3). If it is not, stop this loop; if it is, finish it (2.5),
   remove it and repeat with the next.
2. If an action remains, apply its parts in this order: look, animation, sound, particle, object, movement, then
   the `act` part (section 10). Each part is applied as its section and the object's class say (sections 4 – 10). A
   part whose done flag becomes set during its application makes the engine call the object's callback: look 19
   (`action_watch`), sound 21, particle 22, object 23, movement 18 (O 3.2's arguments). The animation's callback 20
   is called where the animation part becomes done (6.2) [V-8].

So an action stays at least one run: the first completeness test of an action always answers "not complete"
(3.3) [V-8].

### 2.5 Finishing an action (static)

Finishing an action stops and releases the object's sound for actions (7.3), and, unless the particle part was made
with auto-removal (8), stops and releases its particles [V-8].

## 3. `entity_action` (static)

### 3.1 Construction and parts

`entity_action()` makes an action whose parts are as each part class's `()` constructor makes them [V-9]: every
default part is done except the particle part, which becomes done when first applied (8); the `cond` part has no conditions and no time limit (3.3) [V-9]. `entity_action(other)` copies `other`, with
every part's state, its start time and its "checked" flag; `entity_action(nil)` gives a default one (**believed**:
the argument is converted to "no action") [V-9].

### 3.2 Members

- `act:set_action(part)`: one form for each of the eight part classes; copies `part` (with its done flag) into the
  action's part of that class, replacing it [V-9]. Setting a part twice keeps the last.
- `act:move()`, `act:look()`, `act:anim()`, `act:sound()`, `act:particle()`, `act:object()`: the done flag of that
  part (booleans, not parts) [V-9].
- `act:time()`: true when the action's time limit has passed: the start time plus the limit (3.3), as 64-bit
  unsigned integers, is below 2^32 and below now [V-9]. With no limit it is false. **Disputed in review:** "no
  limit" is the limit 2^64 − 1 (`cond()`, and `cond(flags)`, whose limit is −1 made a 64-bit integer); the start
  time is 2^64 − 1 until the action first becomes current, then now with its upper 32 bits 0 (2.4). The sum, taken
  modulo 2^64, is then the start time minus 1, so with no limit `time()` is false only for an action that never
  became current and true once it has (start time above 0); `time_end` without a time limit holds on the action's
  second completeness test.
- `act:all()` and `act:completed()`: the same routine, the completeness test of 3.3 [V-9].

### 3.3 Completeness and `cond`

`cond()` has no conditions and no time limit; `cond(flags)` has the condition set `flags`; `cond(flags, time)` also
the time limit `time` in milliseconds (made a 64-bit integer) [V-10]. `cond` has no other member [V-3]. The action's
`cond` part is set by `set_action` and read only by this test.

The completeness test of an action [V-9]:

1. If its "checked" flag is clear, set it and answer "not complete".
2. Otherwise, from the condition set clear each bit whose condition holds: `move_end` the movement part done,
   `look_end` the look part done, `anim_end` the animation part done, `sound_end` the sound part done, `object_end`
   the object part done, `time_end` `act:time()` true, `act_end` the `act` part done; and bit 16, which no `cond`
   constant names, the particle part done. Other bits are never cleared.
3. Answer "complete" when no bit remains.

So a condition set of 0 completes on the second test; every condition in the set must hold (they combine with "and");
a set with a bit above 128 never completes [V-9]. The "checked" flag is cleared when the action becomes current (2.4), so a
script calling `completed()` on an action that was never queued gets "not complete" the first time [V-9].

## 4. `move` (static)

### 4.1 What it holds

A body state, a movement type, a path type (defaults `standing`, `stand`, `line`), a **target**, which is one of: an
object, a patrol path, a position, a position with a level vertex, a direct position, input keys, or none; a
number (below, "the number"); and the monster's move and speed and a monster distance [V-11]. The default part has
target none and is done [V-11].

### 4.2 Construction

Every constructor with arguments makes a part that is not done [V-11]. With `b` a body state, `m` a movement type,
`p` a path type, `obj` a handle, `pt` a `patrol` object (spec N), `v` a `vector`, `n` a number, `ma` a monster move,
`sp` a monster speed:

| Form | Sets |
|---|---|
| `move(keys)`, `move(keys, n)` | target input keys `keys`; the number `n` (0 when absent) |
| `move(b, m, p, obj[, n])` | body, movement, path; target the object; the number |
| `move(b, m, p, pt[, n])` | body, movement, path; target the patrol path with the patrol's further values (**believed**: its start type, route type, flag and point index, in its constructor's order); the number |
| `move(b, m, p, v[, n])` | body, movement, path; target the position `v`; the number |
| `move(v, n)` | body `standing`, movement `stand`, path `line`; target the direct position `v`; the number `n` |
| `move(ma, v)`, `move(ma, v, n)` | monster move `ma`; target the position; monster distance `n` (absent: unestablished) |
| `move(ma, pt)`, `move(ma, pt, n)` | monster move; target the patrol path as above; monster distance |
| `move(ma, obj)`, `move(ma, obj, n)` | monster move; target the object; monster distance |
| `move(ma, n1, v)`, `move(ma, n1, v, n)` | monster move; target the position `v` with the level vertex `n1` (made an integer); monster distance |
| `move(ma, v, n, sp)`, `move(ma, pt, n, sp)`, `move(ma, obj, n, sp)` | as the forms above, with monster speed `sp` |

[V-11]. A handle is stored as its object (M 2.1); a nil handle stores no object.

### 4.3 Members

`body(b)`, `move(m)`, `path(p)` set that value; `object(obj)` sets the target to that object (nil: no object);
`patrol(path, name)` sets the target to that patrol path; `position(v)` sets the target to the position; `input(keys)`
sets the target to input keys [V-11]. Each clears the done flag. `completed()` returns the done flag [V-3, V-11].

### 4.4 Applying it (static)

A done movement part is not applied [V-17]. Otherwise, by the object's class:

- **Monsters** (the 17 monster classes): if the monster is dead (health 0 or less), the part is done. Its monster
  move selects the gait (`walk_fwd`, `walk_bkwd`, `run_fwd`, `drag`, `steal`; `jump` selects nothing) and speed
  `force` sets a flag of the monster's movement. Target object, position, level-vertex or direct position: the
  monster moves toward it (**believed**: the target's position); target patrol path: it follows that path from the
  part's point index; input keys or none: nothing. Once the monster's path was built after the action started (**believed**: the test
  compares the action's start time with a time kept by the monster's movement), the part is done when the monster is within the monster distance of the target (when that is above 0), or when it has
  reached the end of its path [V-18].
- **Stalkers** (`CAI_Stalker`): if the stalker is dead, the part is done. Target object, patrol path or position: the target
  is handed to the stalker's movement (**believed**: it moves to the object, follows the path from the part's point
  index, or moves to the level vertex at the position); target direct position: it moves straight toward it and the part is done
  when it is within 0.2 of it along each axis; input keys or none: movement stops and the part is done. Otherwise
  the part is done when the stalker's movement reports its path's end. The stalker also takes the part's body state,
  movement type and path type, and the mental state of the action's animation part (6.3) [V-19].
- **The car** (`CCar`): each run sets the car's controls from the input keys: `fwd`, `back`, `left`, `right`, `up`,
  `down` and `handbrake` held while their bit is set and released while clear; `on` starts the engine; `off` stops it
  if it runs [V-20]. The part is never made done: actions of the car end by their time condition (the shipped
  `car.script` does so, 13). What `up` and `down` do for the car is the car's (spec Y) [V-20].
- **The trader, searchlights and `SCRPTOBJ` objects**: a movement part that is not done is not applied; a diagnostic
  ("Cannot assign a movement action not to a monster!") goes to M 2.3's routine; the part never becomes done
  [V-17, V-20].

What "the number" of the stalker forms does is **unestablished** (**believed**: a speed or a distance).

## 5. `look` (static)

### 5.1 Construction and members

A look part holds a sight type (a `look` constant), a target (an object with an optional bone name, a direction, or
none) and two numbers [V-12]. `look()` is done, type `cur_dir`, no target. `look(type)` sets the type with no
target; `look(type, v)` the type and the direction `v`; `look(type, obj)` and `look(type, obj, bone)` the type and the
object (and bone); `look(v, n1, n2)` and `look(obj, n1, n2)` the direction or the object and the two numbers [V-12].
Each is not done. `type(t)`, `object(obj)`, `bone(name)` and `direct(v)` set that value and clear the done flag;
`object` and `direct` also set the target kind [V-12].

### 5.2 Applying it

A look part is applied only while not done [V-17]:

- **Monsters**: type `direction`: the monster turns to face the point 2 units along the vector from its position;
  type `point`: it turns to face the point; other types: no turn. The part is done when the monster's turn has
  finished (**believed**: its turn test answers "not turning") [V-18].
- **Stalkers**: target object: the stalker looks at the object (at the bone's position when a bone is given, else at
  a point the object gives, **believed** its look point); target direction: it looks with the given type and direction; no target: the
  part's type becomes `cur_dir` and the part is done. With a target, the part is done when the difference between
  the stalker's current and target view angles, normalised to (−π, π], is below 0.001 [V-19]. A part with a type and
  no target given (`look(type)`) never becomes done for a stalker [V-19].
- **Searchlights**: the light turns toward the target with the part's two numbers as its turning speeds (**believed**:
  horizontal and vertical); done when both angle differences are below 0.001 [V-20].
- **Other scripted objects**: not applied; the part stays as it is [V-17].

## 6. `anim` (static)

### 6.1 Construction and members

An animation part holds an animation name, a mental state, a monster animation and a number, and a flag [V-13].
`anim()` is done; `anim(name)` and `anim(name, flag)` set the name (and the flag) and are not done; `anim(state)` sets
the mental state and is **done**; `anim(ma, n)` sets the monster animation `ma` and the number `n`, not done [V-13].
`anim(name)` sets the name and clears the done flag; `type(state)` sets the mental state and **sets** the done flag
[V-13]. So a part made only to carry a mental state counts as done for `cond.anim_end`.

### 6.2 Applying it

- **All scripted objects** (the base behaviour): when the part is not done and its name is not empty, the object's
  animation of that name is selected (**believed**: as a cycle of its skeleton; an unknown name gives no cycle); when
  that animation ends the part becomes done, callback 20 is called with the number 2 (O 3.2), and the queue is run
  at once (2.4) [V-17].
- **Monsters**: additionally the monster animation selects the monster's matching scripted animation (`stand_idle`,
  `sit_idle`, `lie_idle`, `eat`, `sleep`, `rest`, `attack`, `look_around`; `turn` selects nothing) [V-18].
- **Stalkers**: when a name is given, the stalker's own animation state for scripts is cleared before the name is
  used; the mental state is taken with the movement part (4.4) [V-19].

## 7. `sound` (static)

### 7.1 Construction and members

A sound part holds a sound name, a bone name, a position, angles, a looped flag, an AI sound type (T's `snd_type`),
a monster sound and a monster sound delay, and a "started" flag [V-14]. The forms: `sound()` (done);
`sound(name, bone[, position[, angles[, looped]]])`; `sound(name, position[, angles[, looped]])`;
`sound(obj_sound, bone, position[, angles[, looped]])` and `sound(obj_sound, position[, angles[, looped]])`
(`obj_sound` a `sound_object`, spec T; what these forms take from it is **unestablished**); `sound(ms)` and `sound(ms, delay)` (`ms` a monster sound; delay absent −1);
`sound(name, bone, head_anim)` [V-14]. The forms with arguments are not done. Setting a name (`set_sound(name)` or a
constructor) looks for the file `name` with extension `.ogg` under the game's sounds (S1 2); when it does not exist
the part is made done at once [V-14]. `set_position(v)`, `set_bone(name)`, `set_angles(v)`, `set_sound_type(t)` set
those values; `set_position` and `set_sound_type` clear the "started" flag (**believed** the other two alike) [V-14].

### 7.2 Applying it (base behaviour)

The object keeps one sound for actions [V-17]. When the sound part is not done:

1. If the object has no such sound and the part has a name, the sound is made from the name with the part's AI sound
   type (T, S1), and the part waits for the next run; with no name the part is done.
2. If its sound is playing, nothing.
3. If the part has not started, the sound is played at the bone's position (when a bone is given) or at the part's
   position, looped if the flag says so, and the part is marked started.
4. Otherwise (started and no longer playing) the part is done.

Stalkers, the trader and the other scripted objects use this; the trader's own handling calls the same steps [V-20].

### 7.3 Monsters

A monster sound other than −1 makes the monster play its own sound of that kind (`idle`, `eat`, `attack`,
`attack_hit`, `take_damage`, `die`, `threaten`, `steal`, `panic`) with the part's delay, or the monster's own delay
for that kind when the delay is −1; the part does not become done by this [V-18]. A part with no monster sound
uses 7.2 [V-18].

## 8. `particle` (static)

A particle part holds a particle effect name, an auto-removal flag, a bone name, a position, angles and a velocity
[V-15]. `particle(name, bone[, params[, auto]])` and `particle(name, params[, auto])` (`params` a `particle_params`,
spec U) create the particles object for `name` at once (**believed** for the forms other than `set_particle`, which
static analysis shows); `set_particle(name, auto)` replaces it [V-15]. `set_bone`, `set_position`, `set_angles`,
`set_velocity` set those values; the setters examined clear the done flag (`set_particle`, `set_bone` and one of the
vector setters; **believed** the others alike) [V-15]. `particle()` makes a part with no particles that is **not**
done; the default auto-removal flag is set [V-15].

Applying it (all scripted objects) [V-17]: when not done: with no particles object, or once started, the part is
done; otherwise the particles are played at the bone's position or the part's position and the part is marked
started. So the part is done on the run after it starts, not when the effect ends. When the action is finished
(2.5), particles without auto-removal are stopped and released.

## 9. `object` (static)

### 9.1 Construction and members

An object part holds an item (an object), an object name, an object action (an `object` constant; default `idle`)
and a number [V-16]. `object()` is done (no item, action `idle`); `object(obj, a)` (number −1) and
`object(obj, a, n)` set the item, the action and the number; `object(a)` sets the action; `object(name, a)` the name and
the action (**believed**); each is not done [V-16]. `object(name)` and `object(obj)` set the name or the item;
`action(a)` the action; each clears the done flag [V-16].

### 9.2 Applying it

- **Stalkers**: with no item, an item that is not an inventory item, or a done part, the stalker's object handling is
  set to no object and the part is done once that handling has fewer than 2 requests pending. Otherwise the request
  is handed to the stalker's object handling with the part's number, and by the action [V-19]: `switch1`, `switch2`,
  `reload`, `reload2` reload the active item when it is the given item (else a diagnostic and the part is done);
  `fire1`, `fire2` fire; `idle` idles the item, the part done when the handling reports so; `drop` drops the item if
  the stalker holds it (else the diagnostic "item is not in the inventory!"); `activate`, `deactivate`, `use` act on
  an item outside the inventory (**believed**: the stalker uses an object it is not carrying); `take` takes the item
  if not held (else "item is already in the inventory!"), the part done. The others (`aim1`, `aim2`, `strap`, `show`,
  `hide`, `turn_on`, `turn_off`) are handed on with no case of their own; their effect is **unestablished**.
- **The car**: the object name names a bone of the car. For a door bone, `activate` opens the door and
  `deactivate` closes it, the part staying not done; `use` acts on the door and the part is done. For another bone,
  `activate`, `deactivate`, `use` act on it (**believed**: the car's other usable parts) and the part is done. An
  empty name makes the part done [V-20].
- **Searchlights**: `turn_on` and `turn_off` switch the light; the part is never made done [V-20].
- **Monsters**: the object part does not become done by applying it [V-18].
- **Other scripted objects**: not applied [V-17].

## 10. `act` (static)

An `act` part holds a monster action (an `act` constant) and an object [V-16]. `act()` is done; `act(a)` and
`act(a, obj)` are not done [V-16]. It is applied only by monsters, and only while the current action's `act` part is
not done: `rest` sets the monster's scripted behaviour to rest; `eat` to eating the object, `attack` to attacking it,
`panic` to fleeing it, each when the object passes tests of its state (**believed**: dead for `eat`, alive for the
others); any other case falls back
to rest [V-18]. Applying it never sets the done flag; when the part becomes done (and so whether `cond.act_end`
holds) is **unestablished** [V-18]. Other objects ignore it [V-17].

## 11. Worked example: `_g.script`'s `action`

`action(obj, move(move.walk_fwd, pt), anim(anim.stand_idle, 0), cond(cond.move_end))` builds an `entity_action`,
sets the three parts in turn, queues a copy with `obj:command(act, false)` and returns another copy (3.1, 2.3). On a
monster under control: the run after it becomes current, the completeness test answers "not complete" (first test);
the parts are applied (look and sound are default and done; the monster's scripted animation is set to stand idle;
it walks along the path); when the movement part becomes done, callback 18 is called; on the next run the test finds
`move_end` satisfied and the action is removed [V-8, V-9, V-18].

## 12. Use by the shipped scripts (measured)

- `entity_action`, `command`, `set_action`: `_g.script` (`action`, `action_first`, `new_action`, `update_action`,
  `script_object`), `anomaly.script`, `utils.script` [V-22].
- Part classes: `move` by 19 loadable scripts, `cond` by 15, `anim` by 12, `look` by 7, `object` by 5, `sound` by 3,
  `act` by 1 (`_g.script`); `particle` by none [V-22].
- `script` by 8 loadable scripts, `get_script` by 4, `can_script_capture` by `mob_eluder.script`; `get_script_name`,
  `action_count`, `action_by_index` and `reset_action_queue` by no script [V-22].
- Constants used: 44 of the 88 (V-22's table); `cond.object_end`, `cond.act_end` and every `act` constant by none
  [V-22].

## 13. Verification facts

- Items 147, used names 65, loadable scripts 30 (106); candidate `class` a configuration key [V-2].
- Registration: 9 classes, no base; constructor arities as 1.2; 88 constants equal to the listing [V-3].
- Scripted kind: 19 creature classes, `CCar`, and the objects of `O_SEARCH` and `SCRPTOBJ` [V-4].
- `car.script`: 6 actions, each with a time condition of 1000 to 8000 ms, the car's object parts on bones
  `phy_door_left`, `phy_door_right`, `phy_door_rear` [V-22].

## 14. Unestablished

- What a stalker's "number" of `move` does (4.4); a missing monster distance (4.2).
- The patrol object's values the move part copies, as named (4.2; believed).
- What the monster's `jump`, the car's `up` and `down`, and the stalker's `aim1`, `aim2`, `strap`, `show`, `hide`,
  `turn_on`, `turn_off` do (4.4, 9.2).
- When an `act` part becomes done (10); which engine behaviour of a monster clears the capture flag (2.2).
- What `sound(name, bone, head_anim)`'s third argument and the `sound_object` forms do (7.1); what the animation part's flag does (6.1).
- For which classes the base animation handling of 6.2 runs beside their own; whether a stalker's look compares one
  or two angles (5.2).
- What out-of-range `action_by_index` reads give (2.3).
- Everything the parts reach behind the call: movement, sight, the object handling, the monsters' behaviours
  (spec X and the characters' specs).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` as identified above; the
  scripts of the archives `gamedata.db0` … `gamedata.dbd` as extracted for the approved script specs.
- Analysis: static analysis of `xrGame.dll` with Ghidra (headless, read-only: decompilation and disassembly of the
  registration of this spec's classes and members, of their constructors and members, of the routines that run an
  object's action queue and of each scripted class's handling of the parts; run-time type information for the
  scripted kind); a parser of the listing; the scoping spec's scan of the 439 scripts. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- Whether the control flag, the control name, the capture flag and the action queue are kept when the game is saved and loaded, or when the object leaves and re-enters the level, is not stated.
