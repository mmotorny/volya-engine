# Scripted motion channels

Behaviour specification TC2 of the trader-and-crows scoping specification
(`docs/behavior/trader-and-crows-scope.md`, section 3), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the two motion channels and the sound that objects of the trader class have, and the only class
that has them. It covers, in full: what `set_trader_global_anim`, `set_trader_head_anim`, `set_trader_sound`,
`external_sound_start` and `external_sound_stop` do, and that they do nothing on any other object; the state
the channels keep; the events 30 – 32 and when each is raised; the head motion ended together with the global
one; the per-frame check and its skip while the object is under script control; what plays before a script
starts a channel. Data: the trader's motion file and the sounds the calls name. Worked example: the trader's
start chain (scope 1.1). What the scripts do when the events come (the scheme `mob_trader`) is the scripts'
(scope 3.1) and appears only in the worked example. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/behavior/motion-playback.md` (TC1, cited as "TC1"): playbacks, starting a cycle with mixing, a
  notification and its value (2.2, 3.1, 3.2), ending a playback by fade-out (3.3), the tracks update (4),
  a motion's end and its notification (5.1, 5.2), the trader's motions and their ends (9.1, 9.2). TC1's
  **playback channel** (0 … 3, its 7.2) is not the motion channel of this text; every start here uses
  playback channel 0.
- `docs/behavior/sound-positional.md` (cited as "S2"): creating a sound with a volume category (3.1), play
  requests and their flags looped and 2D (3.2), stopping at once and discarding (3.1, 4.4), the emitter and
  its end (4.1, 4.2, 4.4), 2D emitters (7.3), the effects category (5.4). `docs/formats/sound-ogg-params.md`
  (S1): how a sound name finds its file.
- `docs/behavior/script-api-dialogs-and-trade.md` (spec R, its section 6): the three `set_trader_*` names as
  scripts call them; `docs/behavior/script-api-object-event-callbacks.md` (spec O): the callback table, how
  an entry is called (3.2) and the ids 30 – 32 (2, 4); `docs/behavior/script-api-engine-services.md` (spec B,
  5.2): the lookup of a cycle by name and its failure; `docs/behavior/script-api-game-object-basics.md`
  (spec M, 2.3): the routine that swallows diagnostics; `docs/behavior/script-api-scope.md` (proposals R, T,
  O, V): who owns the names; `external_sound_start` and `external_sound_stop` are T's names, not approved, so
  only what the engine does behind them is stated here.
- `docs/behavior/script-api-object-binders.md` (spec L, 5.1): spawning and the object's own engine
  reinitialisation; `docs/behavior/characters-scope.md` CH-24 (script control: only the boundary, when the
  check of section 4 is skipped).
- `docs/formats/skeletal-motions.md` (motion sets, names, partitions); the scoping specification's 1.1 (the
  trader, his record, his motion file `actors\trader_animation`, the scripts and the start chain).

Conventions: the **trader class** is the class id `AI_TRD_S` (registered for scripts as `CAI_Trader`, spec O
conventions); a **trader** is an object of it (in the shipped data only `escape_trader`, scope 1.1). "The
game library" is `bin\xrGame.dll`, "the engine" `bin\XR_3DA.exe`, "the sound library" `bin\xrSound.dll`. Each
fact is **static** (from static analysis, not observed at run time), **measured** (from game data) or as
marked. Bracketed ids ([S1] …) name the check that backs the statement next to them: the functions of
`checks.py` in the dirty workspace's `checks/scripted-motion-channels/` (`run_all.py` runs them all; binary
evidence exported by `export.py` into `ev/`).

## 1. Terms

Every trader keeps one **channel record**, made when the object is constructed and freed when it is
destroyed [S2, S2b]:

| Part | Meaning |
|---|---|
| **global channel** | the name of the last global motion given (or none) and its motion (or **ended**) |
| **head channel** | the name of the last head motion given (or none) and its motion (or ended) |
| **the sound** | the trader's current sound (S2 3.1), or none |

"Ended" is a channel's state when it has no motion to report as playing: before any start, after its
motion's end notification, and when this text marks it ended. A channel's motion is the motion id found for
its name, not a playback: starting another cycle elsewhere that fades its playback out (TC1 3.3) does not
mark the channel ended.

The record is used by the five calls (section 2), the per-frame check (section 4), the object's
reinitialisation (section 3) and its construction and destruction, by nothing else; in particular it is not
part of the trader's saved state [S2f, S2g].

## 2. The calls (static)

### 2.1 On objects of other classes

Each of the five calls first tests that the object is of the trader class. On any other object it does
nothing: the diagnostic text it produces goes to the routine that returns at once (M 2.3) [S1b, S1c]. The
rest of this section applies to traders. Only the scheme script `mob_trader.script` calls any of the five
(measured) [S9].

### 2.2 `obj:set_trader_global_anim(name)`

1. The name is kept as the global channel's name [S3].
2. The cycle `name` is looked up among the trader model's motions as `CKinematicsAnimated:PlayCycle(name)`
   looks it up (B 5.2: the motion sets, the last set first, exact name); a name the model lacks is **fatal**:
   a failed check with the expression `motion_ID.valid()`, the description `! MODEL: can't find cycle: ` and
   the name as its argument [S3, S3d, S3e]. (This settles R 6's "what that does is unestablished".)
3. The motion found becomes the global channel's motion (the channel is no longer ended) [S3].
4. The cycle is started as TC1 3.1 and 3.2 describe: on its definition's partition (all partitions for
   0xFFFF), mixing, playback channel 0, with the **global notification** [S3, S3d; TC1 3.4]. The
   notification, called at the end of each of its partitions' playbacks (TC1 5.1, 5.2), marks the global
   channel ended [S3b, S3c].

### 2.3 `obj:set_trader_head_anim(name)`

The same for the head channel: the name is kept as the head channel's name, looked up with the same failure,
its motion becomes the head channel's motion, and the cycle is started on its definition's partition, mixing,
playback channel 0, with the **head notification**, which marks the head channel ended [S3, S3b, S3e].

The channels are names, not partitions: which bones a channel's motion moves is its definition's partition
(skeletal-motions 4.3). Both channels start on playback channel 0, so each start fades out (TC1 3.1 step 2)
the playbacks on its partitions, the other channel's among them; a playback faded out before its end never
notifies (TC1 5.1). So a global motion on all partitions started over a playing head motion leaves the head
channel's motion unreported until section 4's rule for event 30 marks it ended (consequence of TC1 and S3;
**believed**: not observed).

### 2.4 `obj:set_trader_sound(sound, head_anim)`

In this order [S1e, S4]:

1. If the trader has a sound, it is **stopped** (2.7).
2. `head_anim` is started on the head channel as 2.3.
3. A new sound is created from the name `sound` (resolved per S1) in the **effects** category (S2 3.1, 5.4)
   and becomes the trader's sound; the creation also passes the value 2²⁷, whose meaning (S1's reports to
   the AI) is **unestablished** here [S4, S4c].
4. The sound is played with the flags **2D** and not looped, delay 0 (S2 3.2): a new emitter at the
   listener-relative position (0, 0, 0) [S4, S4d]. The play request names no object; what an object there
   would do (S1's reports to the AI) is **unestablished** here [S4].

No call of the game library sets the position of the trader's sound or plays it at a position [S4g]. So the
sound stays a 2D emitter at the listener: by S2 7.3 its distance factor is 1 (*δ'* = 0, within any min
distance), and it is heard at the same level wherever the trader and the camera are (consequence of S2;
**believed**: not observed). With the sound system disabled (S2 2, `-nosound`) creating makes no sound data
[S4c], and the sound counts as ended at the next check (section 4).

### 2.5 `obj:external_sound_start(sound)`

In this order [S4b]:

1. If the trader has a sound, it is stopped (2.7).
2. A new sound is created and played as 2.4 steps 3 and 4.
3. The head channel is marked ended. No head motion is started.

So, while that sound plays, the next check raises event 31 (section 4): the scripts are asked for a head
motion to go with the sound.

### 2.6 `obj:external_sound_stop()`

If the trader has a sound, it is stopped (2.7); otherwise nothing happens [S1d]. The channels are unchanged.

### 2.7 Stopping the sound

The sound's emitter, if it has one, is stopped at once (S2 4.4); the sound is discarded (S2 3.1) and the
trader has no sound [S4e]. No event is raised by stopping (event 32 comes only from the check, section 4).

## 3. Spawning and reinitialisation (static)

- Construction makes the record with both channels ended, no names and no sound [S2].
- At every spawning, the object's own engine reinitialisation (L 5.1 step 4) clears the record: no global or
  head name, both channels ended, no sound (a sound it held is dropped without being stopped) [S2d, S2e].
- The trader's own spawning starts no motion, nor do the general spawning routines it runs; the engine's
  model code starts no cycle by itself [S7, S7b]. So nothing plays on a trader's model until a script calls
  2.2, 2.3 or 2.4 (**believed** for routines deeper than those examined). What a model with no playback
  shows is TC1 7.3's; whether a trader is drawn in that state before the first start is **unestablished**
  (scope 5: when the binder's first update runs).
- Destruction frees the record without stopping or discarding its sound [S2b]: a sound playing when a trader
  is destroyed is believed to play on to its end (S2 4.2).

## 4. The per-frame check (static)

### 4.1 When it runs

The trader's client update (the per-frame update of a game object: **believed** to run once per frame while
the object is processed and the game is not paused, as `artefact-idle-effects.md` 6.1 states for artefacts)
runs the general object update and the trader's other parts first and the **check** last [S6, S6b]. The check
is skipped while the trader is under **script control**, the state `get_script` reports (CH-24; spec V):
outside script control it always runs [S6b, S6c, S6d]. Notifications are not part of the check: they come
from the tracks update (TC1 4), also under script control; a channel ended meanwhile is reported at the first
check after control is given back (consequence of S3b and S6b).

### 4.2 What it raises

Each check, in this order [S5 – S5e]:

1. **Event 32** (`trader_sound_end`): if the trader has a sound and that sound has no emitter (its emitter
   has ended, S2 4.4, or it has no sound data). After the event's call the sound is stopped and discarded
   (2.7), so event 32 comes once per sound.
2. **Event 30** (`trader_global_anim_request`): if the global channel is ended. After the event's call, if a
   global motion name has been given since the spawning (section 3), the head channel is marked ended.
3. **Event 31** (`trader_head_anim_request`): if the head channel is ended and the trader has a sound that
   has an emitter.

Each event is one lookup of its id in the trader's callback table and one call of the entry as O 3.2
describes, with no event arguments (O 4: "none") [S5e, S5f, S5g]. The check never clears an ended state
itself [S5e]: event 30 is raised at every check while the global channel stays ended, and event 31 at every
check while the head channel stays ended and the sound plays; they stop only when a call (2.2, 2.3, 2.4)
gives the channel a motion again, or (31) when the sound ends.

Consequences, in the order of the steps (static, from the same check):

- A call made by the event's function takes effect before the next step: a global motion started by the
  function of event 30 does not prevent the head channel being marked ended just after it (whenever a global
  name is kept), and in the same check event 31 follows if the sound plays.
- A sound started by the function of event 32 (2.4, 2.5) is stopped and discarded at once after the call by
  step 1.
- Before any global motion is given (after spawning), event 30 is raised at every check, and the head channel
  is not marked by it; with no callback set for id 30 the call does nothing (O 3.2).
- An event follows a motion's end at the first check after the tracks update that found the end (TC1 5.1);
  the order of the client update and the tracks updates within a frame is **unestablished**.

## 5. Data (measured)

- All 65 motions of `actors\trader_animation` stop at end (flags 2), so every motion a channel starts ends
  with its notification unless faded out first; 34 are on all partitions, 22 on `torso`, 9 on `head` (TC1
  9.1) [S8].
- Sounds the shipped scripts pass (scope 1.1): `characters_voice\scenario\trader\trader_monolog1` (26,369 ms),
  `…\trader1b` (3,237 ms), `…\trader_tutorial_pda_12` (20,951 ms), mono 44,100 Hz; durations as S2 2 [S8c].
- `external_sound_start` and `external_sound_stop` are called only by the functions
  `mob_trader.on_start_tutorial_item_1` (with `characters_voice\scenario\trader\trader_tutorial_pda_12`) and
  `mob_trader.on_stop_tutorial_item`, which `config\ui\game_tutorials.xml` names as the start and stop
  functions of two tutorial items [S9b, S9c]; when tutorial items run is the tutorials' spec's.

## 6. Worked example: the start chain

The trader at a New Game (scope 1.1), not under script control. The scheme's responses (in italics) are the
scripts' (`mob_trader.script`: on event 30 it sets the signal `animation_end` and, unless the section waits
for that signal, starts another motion of the section's set; on event 31 it starts another head motion of the
section's head set; on event 32 it sets the signal `sound_phrase_end`; when a section starts it sets the three
callbacks, a global motion and, with `sound_phrase`, the sound with a head motion) [S9e]. Times of
ends are TC1 9.2's with 20 ms updates.

1. **Spawning.** Both channels ended, no names, no sound (section 3). Until the scheme's first section starts
   (when the binder's first update runs: scope 5), each check raises event 30 to an empty entry: nothing.
2. **`new_start_wait`.** *2.2 with `idle_2` or `idle_2_def_0` (all partitions).* No sound, so the head
   channel stays ended without event 31. `idle_2_def_0` ends after about 3.3 s: event 30, *another motion of
   the set*; `idle_2` (8.0 s) is still playing when the section's 5,000 ms run out.
3. **`new_start_intro1`.** *2.2 with `spinka_idle`* (all partitions): the earlier playbacks fade out and never
   notify. At about 1.64 s its two partitions notify; from the next check on, event 30 at every check, each
   time marking the head channel ended (a global name is kept); no sound, no event 31. *The section waits for
   `animation_end`: no new motion; the section changes at a later scheme update.*
4. **`new_start_intro2`.** *2.2 with `vstuplenie` (`torso`), then 2.4 with `trader_monolog1` and
   `vstuplenie_head` (`head`).* The sound is created in the effects category and played 2D, at the listener.
   Both motions end at about 23.36 s, in the same tracks update [S8b]. The sound (26,369 ms) still plays
   [S8d], so the next check raises event 30 (*signal; no new motion*), marks the head channel ended, and
   raises event 31 (*`vstuplenie_head` again, the only motion of its set* [S9d]); this repeats at every check
   until the section changes.
5. **`new_start_intro3`.** *2.2 with a motion of `desk_idle`* (all partitions): `vstuplenie` and the restarted
   `vstuplenie_head` fade out without notifying. No `sound_phrase`: no 2.4, and the trader's sound plays on,
   about 3.0 s more. At its end, event 32 (*signal `sound_phrase_end`, unused by this section*), and the sound
   is discarded. Each end of a `desk_idle` motion: event 30 (*another of the set*), head marked ended, no
   event 31 (no sound).
   **Disputed in review:** "no event 31" holds only once the sound has ended. The scheme gives a section
   without `sound_phrase` the head set `normal` once it has started (scope 1.1), and the remaining part of
   `trader_monolog1` (at most about 3.0 s, less by the time the section change takes) can outlast a
   `desk_idle` motion (`palci`, 1.667 s): a `desk_idle` end while the sound plays gives event 30, the head
   marked ended, then event 31 (*a head motion of `normal`*), which repeats at each end of that head motion
   until the sound ends.
6. **`new_start_intro4`.** *2.2 with `desk_talk`, 2.4 with `…\trader1b` and a head motion of the set
   `normal` (`talk_0`, `talk_1`, `talk_4`)* [S9d]. While the 3,237 ms sound plays, each end of a head motion
   gives event 31 (*another of `normal`*); at the sound's end, event 32 (*signal `sound_phrase_end`, which
   the section waits for*).

## 7. Verification facts

`run_all.py` (all checks pass, `FAILS: none`):

- The five names paired with their routines; each acts only on the trader class [S1 – S1e].
- The record: made at construction, cleared at every spawning's reinitialisation, freed at destruction
  without stopping its sound, used by nothing else [S2 – S2g].
- The channels: lookup by name with the fatal check, start with mixing on playback channel 0 and a
  notification that marks the channel ended [S3 – S3e].
- The sound: effects category, 2D, not looped, no delay, no position ever set; stopping at once and
  discarding [S4 – S4g].
- The check: events 32, 30, 31 in that order with their conditions; the head marking after event 30;
  skipped under script control [S5 – S5g, S6 – S6d].
- No start of a cycle at spawning [S7, S7b]; data of section 5 and the scheme's responses of section 6
  [S8 – S8d, S9 – S9e].

## 8. Unestablished

- Whether the trader's client update runs every frame (believed, 4.1); the order of the client update and the
  tracks updates within a frame (4.2).
- Whether anything deeper in the spawning starts a motion on the trader's model (believed not, section 3), and
  whether the trader is drawn before the scheme's first start (section 3).
- The meaning of the value 2²⁷ given at the sound's creation and of the absent object of its play request
  (2.4).
- That a sound playing when a trader is destroyed plays on to its end (believed, section 3).
- That the sound is heard at the same level wherever the trader is (believed from S2, 2.4).

## 9. Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (later archives win); binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`, `bin\xrSound.dll`.
- Data examined: `meshes\actors\trader\trader.ogf`, `meshes\actors\trader_animation.omf`, the sounds of
  section 5, `scripts\mob_trader.script`, `scripts\lua_help.script`, every `scripts\*.script` for the five
  names, `config\ui\game_tutorials.xml`; the approved specifications listed above.
- Analysis: static analysis (headless disassembly and decompilation with Ghidra, read-only) of the game
  library's trader class (construction, destruction, reinitialisation, spawning, client update, the channel
  and sound routines, the notifications, the per-frame check, the registration of the five names and their
  script routines), its import table, the general client spawning, the script-control state and the
  scripted-action step; of the engine's cycle lookup and start and of the callers of its cycle starts; of the
  sound library's creation, play and discard. No execution or debugging.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The function of event 32 leaving the trader without a sound (calling `external_sound_stop`): the check
  then stops a sound that is gone; what that does is not stated (**believed** a crash, since the stop after
  the call is made without testing that a sound remains).
