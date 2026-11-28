# Script API T: sound

What the sound part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition), as scripts see it: the class `sound_object`, with which scripts make and play their own sounds; the
constant holder `snd_type` (the AI sound types); the record classes `sound_params` and `SoundInfo`; the members of
`game_object` that give a creature sounds to play, set its hearing threshold and read what it has heard; and the
`level` functions `set_snd_volume`, `get_snd_volume`, `prefetch_sound` and `iterate_sounds`, the last one as an
entry point (its arguments, calls, order and failures). This is spec T of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6, order 26); its scope rule, its section 4.5 row T and its review
gaps apply: this spec owns every name of these classes as the game library registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type,
  namespaces, the 250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched,
  calls of missing members, what a mismatch does; the collection of script values); spec B,
  `docs/behavior/script-api-engine-services.md` (its 3.3: the device's `time_global()`, in milliseconds); spec E,
  `docs/behavior/script-api-value-types-and-math.md` (the class `vector`, its 2.2 argument conversions); spec M,
  `docs/behavior/script-api-game-object-basics.md` (the handle and its engine object, 2.1; the **kinds** of object,
  2.2; what a member does on an object of the wrong kind and the diagnostics routine that returns at once, 2.3);
  spec O, `docs/behavior/script-api-object-event-callbacks.md` (its 3.2: how the engine calls a function with or
  without a bound value); spec D, `docs/behavior/script-api-engine-entry-points.md` (1.2, the protected call and
  its fatal failure).
- `docs/behavior/sound-positional.md` (cited as **S2**): a **sound** and its volume category (3.1, 5.4; the
  **effects** category), the play requests **play**, **play at position** and **play independently** with their
  flags **looped** and **2D** and their **delay** in seconds (3.2), the **emitter**, its phases and its controls
  (position, frequency, range, play volume; 3.3, 4), **stop at once** and **stop with fade** (4.4), discarding a
  sound (3.1), the **effects factor** *F* (5.4), the sound system disabled by `-nosound` (2).
- `docs/formats/sound-ogg-params.md` (cited as **S1**): how a sound name finds its file (2), the fatal errors (5),
  the min and max distance and the base volume (3, 7.1), the **AI sound type** and the reports to the AI (7.1,
  7.2; its table of the `snd_type` names and values).
- `docs/behavior/scripted-motion-channels.md` (cited as **TC2**): what the trader's sound does (its 2.4 – 2.7).
- `docs/behavior/characters-scope.md` (cited as **CH**): the creatures' own sounds (CH-25), hearing (CH-14) and
  memory (CH-15) own what the engine does behind the creature members of sections 6 and 7; this spec owns the
  names and how scripts call them (CH 3). `docs/formats/character-descriptions.md`: the field `snd_config`.

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the sound
  library" `bin\xrSound.dll` (229,376 bytes, MD5 `21026ed330e01edccd551b2fb69c02ef`) [T-1]. **The listing** is
  `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from the game
  library's registration code.
- A call is written as scripts write it: `snd:play_at_pos(obj, pos, 0, sound_object.s2d)`. A **number** is the
  runtime's double; a **text** a Lua string; **nil** Lua's nil; a **handle** a `game_object` (M 2.1) and its
  **object** the engine object it refers to.
- A number given where the engine wants an integer (flags, a sound type, a count, an index, a time) is
  **made an integer**: truncated toward zero to a 64-bit integer whose low 32 bits are used (static for the
  sound type of 2.1 [T-6]; **believed** the same routine for the others, as for E 2.2's flags arguments). A
  number given where the engine wants a float is stored as a 32-bit float (E 2.2's "to f32", **believed** the
  same conversion).
- **Now** in sections 6 and 7 is the device's `time_global()` (B 3.3), in milliseconds [T-15].
- **Fatal** means the program reports the failure and ends; "reads through no object" means the code reads or
  calls through a missing part without a test (**believed**: the program crashes), as in M 2.3.
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (from
  game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [T-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-sound/run_all.py` (`python3 -I run_all.py`): it reads the binaries' bytes and run-time type
information, the evidence of read-only headless static analysis (produced by `gh.py` beside it, and the evidence
of the checks of S2, M and TC2 that it cites), the listing, the scoping spec's scan and the shipped scripts. All
checks pass (`ALL PASS 18`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 93 listed items [T-2]:

| Owner | Items |
|---|---|
| `sound_object` | the class; 2 constructors (2.1); the constants `s3d`, `s2d`, `looped` (2.2); the properties `frequency`, `min_distance`, `max_distance`, `volume` (2.8); 9 function names in 13 forms: `play` (3 forms), `play_at_pos` (3), `play_no_feedback`, `stop`, `stop_deffered`, `playing`, `length`, `get_position`, `set_position` (2.3 – 2.7) |
| `snd_type` | the class and its 46 constants (section 3) |
| `sound_params` | the class; the properties `position`, `volume`, `frequency`, `min_distance`, `max_distance` (section 4) |
| `SoundInfo` | the class; the properties `who`, `position`, `power`, `time`, `danger` (7.4) |
| `game_object` | 13 functions: `add_sound`, `play_sound`, `remove_sound`, `set_sound_mask`, `active_sound_count`, `sound_prefix` (section 6), `external_sound_start`, `external_sound_stop` (6.6), `sound_voice_prefix` (6.5), `set_sound_threshold`, `restore_sound_threshold`, `memory_sound_objects`, `get_sound_info` (section 7) |
| `level` | `set_snd_volume`, `get_snd_volume` (5.1), `prefetch_sound` (5.2), `iterate_sounds` (5.3) |

and the entry point of the scoping spec's 4.5 row T, the function `iterate_sounds` takes (5.3). The scoping spec
assigns this spec no candidate strings and no unlisted name [T-2]. Of these names the loadable scripts use 47
(scoping spec section 6) [T-2].

### 1.2 As registered (static)

- `sound_object` is registered with no base class, two constructors, three integer constants, four properties
  and the 13 function forms of 1.1, each paired with its implementation [T-3]. The listing agrees with the
  registration in names, forms and constants [T-3].
- `sound_params` and `SoundInfo` are registered with no base class and **no constructor**; each has exactly the
  five properties of 1.1, each readable and writable [T-4]. `snd_type` is a holder of 46 integer constants
  without constructor [T-4].
- The `game_object` members are paired with their implementations in the registration of `game_object` (M 1.2):
  `add_sound` in 2 forms, `play_sound` in 6, `active_sound_count` in 2, `sound_prefix` in 2 (a reader and a
  writer), the others in one [T-12]. The four `level` functions are registered in the namespace `level`,
  `iterate_sounds` in 2 forms [T-10, T-11].
- No class of this spec has a property the listing lacks (the scoping spec's gap 2) [T-3, T-4].

## 2. `sound_object` (static)

A sound object holds at most one sound of S2 (S2 3.1), its **sound**, made when the object is constructed, or none
(**no sound**). Every member below acts on that sound and its emitter, if it has one [T-3, T-5].

### 2.1 Construction

- `sound_object(name)`: if the file `<name>.ogg` exists among the game's sound files (`$game_sounds$`, S1 2), a
  sound is created from `name` (S1 2: the name lower-cased, suffixes cut, the level's directory searched first) in
  the **effects** category, with the AI sound type **0** (S1 7.2: so no report to the AI is ever made for it) [T-6,
  T-5]. If no such file exists, the object has no sound, and nothing is reported [T-6]. The existence test takes `name` as given (no lower-casing or suffix removal of its own); whether
  it is case-sensitive is the file system's (**unestablished** here; **believed** not, as the archives' paths are
  compared ignoring case).
- `sound_object(name, type)`: the same, with the AI sound type `type`, made an integer (static: truncated toward
  zero) [T-6]. A type of −1 means "the file's own type" (S1 7.2) [T-5]. The names of `snd_type` (section 3) are the
  values scripts pass.
- With the sound system disabled (S2 2), creating makes no sound, so the object has no sound [T-5].

The shipped scripts construct sound objects at three places (`escape_heli.script`, `xr_sound.script`,
`xr_zones_sound.script`), all with one argument [T-18].

### 2.2 The flags

The play members take a **flags** number whose bits are the request flags of S2 3.2: `sound_object.looped` = 1
(looped), `sound_object.s2d` = 2 (2D), `sound_object.s3d` = 0 (neither) [T-3, T-5]. The flags are made an integer;
other bits are ignored (S2 3.2 reads only these two) [T-5]. Scripts add them: `sound_object.s2d +
sound_object.looped` (`sr_psy_antenna.script`) [T-7].

### 2.3 `play` and `play_at_pos`

| Form | Request (S2 3.2) | Delay (s) | Flags |
|---|---|---|---|
| `snd:play(obj)` | play | 0 | 0 |
| `snd:play(obj, delay)` | play | `delay` | 0 |
| `snd:play(obj, delay, flags)` | play | `delay` | `flags` |
| `snd:play_at_pos(obj, pos)` | play at position `pos` | 0 | 0 |
| `snd:play_at_pos(obj, pos, delay)` | play at position `pos` | `delay` | 0 |
| `snd:play_at_pos(obj, pos, delay, flags)` | play at position `pos` | `delay` | `flags` |

[T-7]. `obj` is a handle, `pos` a `vector` (E), `delay` a number of **seconds** (S2's delay, passed unchanged as a
32-bit float) [T-7]. So a script that has milliseconds divides by 1000 (`sr_sound2d.script` passes `d / 1000.0`)
[T-7]. (This settles the unit left open in `sound-notifications.md` section 6: the delays there are seconds.)

The handle's object becomes the sound's **owner**, the object S1 7.1 associates the sound with for reports to the
AI [T-5, T-7]. Before the request the handle's object is asked for its handle once more, which changes nothing
(M 2.1: the handle already exists) [T-7]. A sound object with no sound plays nothing (S2's requests do nothing
without sound data) [T-7]. What a `nil` first argument does depends on A (whether `nil` matches a handle
parameter); if it matches, the code reads through no object (**believed**) [T-7]. No shipped call passes a literal
`nil` there [T-7].

By S2 3.2, a play on a sound whose emitter still exists restarts that emitter (S2 4.3) and keeps its looped flag,
delay and position; `play_at_pos` then sets the new position; the 2D flag switches it to 2D either way.

### 2.4 `snd:play_no_feedback(obj, flags, delay, pos, volume)`

S2's **play independently**, with the owner `obj`'s object, the flags and delay as in 2.3, the position `pos` and
the play volume `volume` set at once; frequency and range are not set (they stay the file's, S2 3.2) [T-7]. The new
emitter belongs to no sound object: no member of the object reaches it, and it runs until it ends (S2 3.2); a looped
one never ends (**believed**: S2 4.2, a looped emitter does not end by itself) [T-7, T-5]. With no sound,
nothing is played [T-7].

### 2.5 `stop` and `stop_deffered`

`snd:stop()` stops the emitter **at once**, `snd:stop_deffered()` stops it **with fade** (S2 4.4) [T-8, T-5]. Without
an emitter (or a sound) neither does anything [T-8].

### 2.6 `playing` and `length`

- `snd:playing()` is `true` while the sound has an emitter attached, in every phase of S2 4.1 (also while it waits
  for its delay), else `false`; with no sound, `false` [T-8].
- `snd:length()` is the sound's duration *D* (S2 2), in milliseconds, as a number [T-8, T-5]. With no sound the
  code reads through no object [T-8].

### 2.7 `get_position` and `set_position`

- `snd:get_position()` returns a `vector` holding the emitter's position (S2 3.3; listener-relative for a 2D
  emitter) [T-8]. Without an emitter (or a sound) it returns (0, 0, 0); the diagnostic it passes on goes to the
  routine of M 2.3 that returns at once [T-8].
- `snd:set_position(pos)` sets the emitter's position (S2 3.3) [T-8]. Without an emitter (or a sound) the code
  reads through no object [T-8].

### 2.8 The properties

Each reads or sets a control of the emitter (S2 3.3) [T-8]:

| Property | Reads | Assigning `v` |
|---|---|---|
| `volume` | the play volume *v* | sets *v* |
| `frequency` | the frequency *f* | sets *f* |
| `min_distance` | the min distance *m* | sets the range to (`v`, the current *M*) |
| `max_distance` | the max distance *M* | sets the range to (the current *m*, `v`) |

Values are 32-bit floats, read exactly as numbers [T-8]. Reading a property without an emitter (or a sound)
reads through no object; assigning one then does nothing [T-8]. After a new play the emitter's controls are those
of a new emitter (S2 3.2), so values assigned before the play do not carry over (consequence of S2 3.2).

### 2.9 When a sound object is collected

When the runtime collects a sound object, its sound is discarded, which stops its emitter at once (S2 3.1, 4.4)
[T-9]. When the collector runs is the runtime's (A; **unestablished** here). So a sound played from an object that
nothing keeps (as `xr_zones_sound.script` does: `sound_object(...):play_at_pos(...)`) is cut off at the next
collection of that object (**believed**: consequence of the rule, not observed); `play_no_feedback`'s emitter is not
affected (2.4).

## 3. `snd_type` (static, measured)

The 46 constants are the AI sound types of S1 7.2, with S1's names and values: `no_sound` 0, the 25 single-bit
flags and the 20 combined names of S1's tables [T-4]. They are registered as signed 32-bit integers, so the five
names with the top bit set read as negative numbers: `weapon` −2147483648, `weapon_reload` −2147221504,
`weapon_bullet_hit` −2146959360, `weapon_empty` −2146435072, `weapon_shoot` −2145386496; the listing prints the same
values [T-4]. Passed back where the engine wants a type (2.1, 6.2), they give the same 32 bits (the conversion of
the conventions) [T-6].

## 4. `sound_params` (static)

A record of five fields, all readable and writable: `position` (a `vector`), `volume`, `frequency`, `min_distance`,
`max_distance` (32-bit floats), laid out as the emitter keeps its position, play volume, frequency and range [T-4,
T-5]. No member of this spec returns one and it has no constructor, so the scripts cannot obtain one (**believed**:
no registered member of the listing returns it) [T-4]. No shipped script uses the class [T-2].

## 5. `level` functions (static)

### 5.1 `level.set_snd_volume(v)` and `level.get_snd_volume()`

`set_snd_volume` sets S2's effects factor *F* (S2 5.4) to `v`, then limits it to 0 … 1 [T-10]. `get_snd_volume`
returns *F* [T-10]. So a script lowers every sound of the effects category, sound objects included (2.1), without
touching the music. This is the setter of S2 5.4 whose user S2 left unestablished [T-10]; the shock effect of S2 5.4
also writes *F* while it runs and restores its saved value when it ends. The loadable scripts
`sr_aes_deadzone.script`, `sr_postprocess.script` and `sr_psy_antenna.script` call them [T-18]. *F* is not
restored by anything of this spec.

### 5.2 `level.prefetch_sound(name)`

The name is lower-cased and the part from its last `.` is removed [T-10]. If the running level has not prefetched
that name yet, it creates a sound of that name (effects category, the file's AI sound type) and keeps it; otherwise
nothing happens [T-10]. A kept sound is never played (**believed**: nothing else reaches it); creating it loads its
file (S1 2), so later sounds of the same name reuse the loaded data (S1 2). A name whose file does not exist is fatal (S1 2 step 4, S1 5) [T-10]. How long the
level keeps the sounds is **unestablished** (**believed**: until the level ends). The only shipped caller is the
non-loadable `sound_prefetch.script` [T-11].

### 5.3 `level.iterate_sounds(list, n, f)` and `level.iterate_sounds(list, n, object, f)`

The entry point of the scoping spec's 4.5 row T. `list` is a text, `n` a number made an integer, `f` a function,
`object` any value (the bound value) [T-11]. The call is synchronous: every call of `f` happens before
`iterate_sounds` returns [T-11]. For each item of `list` split at `,` (item by item, in order; the items are
**believed** to have surrounding spaces removed: the splitting routine is asked to trim) [T-11]:

1. If `<item>.ogg` exists among the game's sound files (`$game_sounds$`), `f` is called with **the whole text
   `list`** (not the item; the same when `list` has one item) [T-11].
2. Then for *i* = 0, 1, …, `n` − 1: if `<item><i>.ogg` exists (*i* in decimal, appended to the item without
   separator), `f` is called with the text `<item><i>` [T-11].

Each call is as O 3.2 calls a function: `f(name)` in the first form, `f(object, name)` in the second; results are
not read; the call is protected and an error in `f` is fatal as D 1.2 [T-11]. If no function is held (what a
non-function argument does depends on A), nothing is called [T-11]. `iterate_sounds` itself plays or loads nothing.
The shipped scripts call it only in `sound_prefetch.script`, which no loadable script reaches [T-11].

## 6. Creature sounds (static)

These members give a creature a **sound player**: named collections of sounds that the creature plays at one of its
bones. What the player does over time (when a queued sound starts, follows its bone, ends; how the creature's own
logic uses it) is CH-25's; this section states what each call does at once.

### 6.1 Kinds

The members of this section and section 7 need these kinds of M 2.2 (on an object of another kind, M 2.3 and the
fallback given with each member apply) [T-12, T-13]:

| Kind | Classes | Members |
|---|---|---|
| **fighter** (M 2.2) | the 18 creature classes other than `CAI_Trader` | `add_sound`, `play_sound`, `remove_sound`, `set_sound_mask`, `active_sound_count`, `sound_prefix`, `set_sound_threshold`, `restore_sound_threshold`, `memory_sound_objects` |
| **monster** | the 17 creature classes other than `CAI_Stalker` and `CAI_Trader` | `get_sound_info` |
| **inventory owner** (M 2.2) | the 19 creature classes and the player | `sound_voice_prefix` |
| the class `CAI_Trader` | the trader | `external_sound_start`, `external_sound_stop` |

The monster kind is one more kind query of the engine object, answered by exactly these 17 classes [T-12].

### 6.2 `obj:add_sound(prefix, n, type, priority, mask, id[, bone])`

Registers a **collection** under the number `id` (the **internal type**) on the object's sound player [T-14]:

1. If a collection is already registered under `id`, nothing changes and the result is 0 [T-14].
2. Otherwise a collection with the priority `priority`, the mask `mask`, the bone `bone` (a text; `bip01_head`
   when omitted) and these sounds is registered [T-14, T-12]: for each item of `prefix` split at `,` (as 5.3), with
   *p* the object's **sound prefix** (6.5): the sound `<p><item>` if `<p><item>.ogg` exists among the game's sound
   files, then `<p><item><i>` for *i* = 0 … `n` − 1 if that file exists; each created in the effects category with
   the AI sound type `type` [T-14].
3. The result is the number of sounds in the collection [T-14].

All numbers are made integers. Collections with the same text, sound prefix, count and type are kept once and
shared by every object that registers them (**believed**: they are looked up in one store of the game library)
[T-14]. On an object that is not a fighter the result is 0 [T-13]. `sound_theme.script` registers its talk sounds with
type `snd_type.talk`, priority 2 and mask 1 [T-18].

### 6.3 `obj:play_sound(id[, max_start[, min_start[, max_stop[, min_stop[, index]]]]])`

Missing numbers are 0, a missing `index` −1 [T-15]. The object's sound player keeps a **mask** (6.4) and a list of
**queued sounds**, each with the priority and mask of its collection, its bone, its own sound, a start time and a
stop time [T-15]. In this order [T-15]:

1. **May it play?** Only if a collection is registered under `id`, the player's mask and the collection's mask have
   no bit in common, and no queued sound whose mask shares a bit with the collection's mask has a priority number
   less than or equal to the collection's; otherwise nothing happens. So a smaller priority number wins, and an equal
   one does not interrupt.
2. If the collection has no sounds, nothing happens.
3. Every queued sound whose mask shares a bit with the collection's mask is removed, and so is every queued sound
   that has no emitter and whose stop time has come; a removed sound is stopped at once (**believed**: its sound is
   discarded, S2 3.1).
4. One sound of the collection is chosen: number `index` when `index` is not −1 (no range test: **unestablished**
   what a number past the end does); otherwise one at random, uniform, and when the collection has 3 or more sounds
   never the one it gave last time.
5. A new sound is made from the chosen one (same file, effects category) with the file's own AI sound type (S1 7.2;
   the collection's `type` is not used for it), and the object as its owner (S1 7.1).
6. Start time = now + *d*₁, stop time = start time + *D* (the sound's duration, S2 2) + *d*₂, where *d*₁ is drawn
   from `min_start`, `max_start` and *d*₂ from `min_stop`, `max_stop` (milliseconds) by this rule: 0 when the
   maximum is 0; the maximum when the maximum is not greater than the minimum; otherwise minimum + an integer drawn
   uniformly from 0 … maximum − minimum − 1 (a 32-bit linear congruential draw, multiplier 134775813, increment 1,
   scaled by the high 32 bits of the product).
7. The queued sound is added. If its start time has come (*d*₁ = 0), it is played at once with S2's play at
   position, flags 0 and delay 0, at the world position of its bone on the object's model.

With all numbers 0 the sound starts at once and its queued entry lasts as long as the sound (6.4). Which bone a
missing bone name gives is **unestablished**. On an object that is not a fighter nothing happens [T-13].

### 6.4 `remove_sound`, `set_sound_mask`, `active_sound_count`

- `obj:remove_sound(id)` removes the collection registered under `id` [T-16]. Queued sounds of it are not removed
  (**believed**: the removal touches only the collections). On an object that is not a fighter nothing happens
  [T-13].
- `obj:set_sound_mask(m)` sets the player's mask to `m` (made an integer), then removes, as 6.3 step 3 does with
  `m` for the collection's mask, every queued sound whose mask shares a bit with `m` and every finished one [T-16].
  While a bit is set, collections whose mask has it cannot play (6.3 step 1); `set_sound_mask(0)` lifts every block.
  The player's mask before any call is **unestablished** (**believed** 0). `_g.script` is the one loadable script
  calling it [T-18].
- `obj:active_sound_count()` counts the queued sounds that have an emitter or whose start time has come;
  `obj:active_sound_count(true)` counts only those with an emitter (`false` is the same as no argument) [T-16]. On an
  object that is not a fighter the result is −1 [T-13].

### 6.5 Prefixes

- `obj:sound_prefix()` returns the object's sound prefix, a text, or nil when it has none (**believed**: the runtime
  gives nil for a missing text) [T-16]; `obj:sound_prefix(text)` sets it [T-16]. The prefix is put before the items
  of collections registered **later** (6.2); collections already registered keep their sounds. On an object that is
  not a fighter the reader returns nil and the writer does nothing [T-13]. Where the prefix comes from at spawning
  is CH-25's.
- `obj:sound_voice_prefix()` returns the text the character's description gives as `snd_config`
  (`character-descriptions.md`) [T-17] (**believed** for the link from the object to its description: the field
  read is the one the description reader fills from `snd_config`). On an object that is not an inventory owner the
  result is nil (**believed**, as for `sound_prefix`) [T-13].

### 6.6 `external_sound_start(name)` and `external_sound_stop()`

On the trader they do what TC2 2.5 and 2.6 state [T-12]. On any other object they do nothing [T-13].

## 7. Hearing and remembered sounds (static)

### 7.1 `obj:set_sound_threshold(v)` and `obj:restore_sound_threshold()`

The fighter's hearing keeps a **current threshold** and a **base threshold** [T-17]. `set_sound_threshold(v)` sets the
current threshold to `v` (a 32-bit float); `restore_sound_threshold()` sets it back to the base threshold, which is
read when the creature is loaded from its section's key `sound_threshold` (0.05 when the key is absent) [T-17]. That
the current threshold starts equal to the base is **believed**. What the threshold does is CH-14's. On an object
that is not a fighter nothing happens [T-13].

### 7.2 `obj:memory_sound_objects()`

Returns an iterator function over the creature's remembered sounds, for use as `for s in obj:memory_sound_objects()
do … end`; each value it gives is a `sound_memory_object` (spec X's class; **believed** from the listing, which has that class for
the remembered sounds) [T-17]. What is remembered is CH-15's. On an
object that is not a fighter the code reads through no object [T-13]. No shipped script calls it [T-18].

### 7.3 `obj:get_sound_info()`

Returns a new `SoundInfo` (7.4) describing one of the monster's remembered sounds: of the records its memory holds,
the one with the greatest value in one field of the record (the first such one in the memory's order; **believed** an
importance, the field is CH-15's) [T-17]. When the memory
holds none, or the object is not a monster, every field is zero: `who` nil, `position` (0, 0, 0), the numbers 0 [T-17,
T-13]. What the memory holds and how that field is valued is CH-15's. No shipped script calls it [T-18].

### 7.4 `SoundInfo`

| Field | Value |
|---|---|
| `who` | the handle of the object that made the sound; nil when there is none or the object carries a state flag the code tests there (**unestablished** which state; **believed**: being destroyed) |
| `position` | a `vector` copied from the record (**believed**: where the sound was heard) |
| `power` | a number copied from the record (**believed**: the sound's power as heard) |
| `time` | a number copied from the record (**believed**: when it was remembered, in CH-15's units) |
| `danger` | the number 1 when the sound's kind (CH-14's classification) is below 8, else 0; a number, not a boolean, so it is true in Lua either way |

[T-17, T-4]. Assigning a field changes the record only, not the creature's memory (**believed**: the record is a
copy) [T-17].

## 8. Use by the shipped scripts (measured)

- Sound objects: constructed at 3 places (2.1); `play_at_pos` by 10 loadable scripts (14 in all), `stop` by 11,
  `playing` by 7, `play` by 7, `set_position` by 4, `play_no_feedback` by 2 (`sr_teleport.script`,
  `xr_effects.script`), `length` by `news_manager.script` [T-18, T-2]; these counts are of the method names on any
  object (scoping spec section 1), so they include particle objects' same-named methods.
- `snd_type`: `sound_theme.script` (`talk`, for `add_sound`) and `xr_motivator.script` (tests of a type's bits)
  [T-2, T-18].
- Creature members: `add_sound`, `sound_prefix` (`sound_theme.script`), `play_sound` (`xr_camper.script`,
  `xr_sound.script`), `active_sound_count` (6 scripts), `set_sound_mask` (`_g.script`), `external_sound_*`
  (`mob_trader.script`, TC2 5) [T-2]. `get_sound_info` and `memory_sound_objects` are called by no script [T-18].
- `level`: `set_snd_volume` (3 loadable scripts), `get_snd_volume` (`sr_psy_antenna.script`) (5.1); `prefetch_sound`
  and `iterate_sounds` only in the non-loadable `sound_prefetch.script` [T-11, T-18].

## 9. Verification facts

- Items: 93 listed (4 classes; 46 `snd_type`, 16 `sound_object`, 5 `sound_params`, 5 `SoundInfo` members; 13
  `game_object` functions; 4 `level` functions); 47 names used by the loadable scripts, by 52 loadable scripts (70
  in all) counted by the scoping spec's upper rule [T-2].
- Registration: `sound_object` 2 constructors, 3 constants (`s3d` 0, `s2d` 2, `looped` 1), 4 properties, 13 function
  forms; `sound_params`, `SoundInfo` 5 properties each, no constructor; `snd_type` 46 constants equal to the
  listing's [T-3, T-4].
- The sound object's sound: effects category; AI sound type 0 for one argument [T-6].
- `level.set_snd_volume`: *F* limited to 0 … 1 [T-10].
- Worked example (`sr_sound2d.script`): `snd_obj_l:play_at_pos(actor, pos_l, d / 1000.0, sound_object.s3d)` with
  `d` = 500 makes the sound's emitter (new, or restarted) wait 0.5 s, at `pos_l`, 3D, owned by `actor`'s object (2.3).
- Worked example (6.3): a collection of priority 2 and mask 1 (`sound_theme.script`) playing; a second
  `play_sound` of the same collection is refused (equal priority, shared mask bit); one of priority 1 and mask 1
  removes and stops it and plays.

## 10. Unestablished

- Whether the existence test of 2.1 and 5.3 is case-sensitive (believed not).
- What a `nil` handle argument does (2.3; depends on A); what a non-function `f` does (5.3; depends on A).
- When the runtime collects a sound object (2.9).
- How long a level keeps prefetched sounds (5.2; believed until it ends).
- Which bone a missing bone name gives, and what an `index` past the end does (6.3).
- The sound player's initial mask (6.4; believed 0); whether queued sounds of a removed collection go on (6.4;
  believed so).
- That the current threshold starts equal to the base (7.1).
- Which state the flag tested for `SoundInfo.who` marks (7.4).
- Everything the creature members reach behind the call: the sound player over time (CH-25), hearing (CH-14),
  memory and the kinds of a remembered sound (CH-15).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` and `xrSound.dll` as
  identified above; the scripts of the archives `gamedata.db0` … `gamedata.dbd` as extracted for the approved
  script specs.
- Analysis: static analysis of `xrGame.dll` and `xrSound.dll` with Ghidra (headless, read-only: decompilation and
  disassembly of the registration of the classes and namespace functions of this spec and of the routines they
  call; run-time type information for the kinds of 6.1); a parser of the listing; the scoping spec's scan of the
  439 scripts. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- 6.3: where the random state of a collection starts and whether the pick of step 4 and the draws of step 6 share it are not stated; nor how the pick of step 4 scales its draw to the collection's size.
- 6.2: what `add_sound` does when the object has no sound prefix (6.5's nil case) is not stated.
- 5.2: what `level.prefetch_sound` does with the sound system disabled (S2 2), including whether a missing file is then fatal, and with no level running, is not stated.
