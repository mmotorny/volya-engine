# Automatic-save timing and input suppression

Functional specification of when S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) writes its
periodic automatic save, what postpones it, the clock it is timed on, how the menu and the other pauses
affect it, and the player-input switch that blocks the quick-save and quick-load keys (F6, F7). It is
proposal SL-2 of `docs/behavior/narrowed-specs-scope.md`.

It builds on:

- `docs/formats/sav.md` (cited as `sav`): the save situations, names and the periodic automatic save of
  section 1.1, the level change (1.2), loading (11.2), game time (12), the picture (13);
- `docs/behavior/save-load-screens.md` (cited as `screens`): opening the menu during a game (2), the
  quick-save and quick-load keys (9.2), the automatic saves as the player sees them (10), and its list of
  items not established (14);
- `docs/formats/ltx-config.md`: configuration files and value syntax.

It settles:

- `sav` 1.1: "that this clock is real time is believed" (corrected: real time with paused periods removed,
  section 1) and "what sets that counter is **unestablished**" (section 3);
- `screens` 9.2, "player input is not disabled by the game's scripts (believed: e.g. during scripted
  scenes)", and `screens` 14, "The input-disable condition that blocks F6/F7" (section 5);
- `screens` 10: the 3 s of the "Автосохранение..." message are counted on the world clock of section 1,
  not on game time (section 2.4).

Conventions:

- The **world clock** is the engine clock of section 1, in milliseconds. "Now" is its current value.
- A **creature** is a stalker or a monster of any kind: the online objects that choose an enemy
  (section 3). The **player character** is the player's online object (configuration class `O_ACTOR`,
  `config\creatures\actor.ltx` section `[actor]` [D3]).
- **Single-player** means the game type that the console command `save` accepts ("for single-mode only"
  otherwise, `sav` 1.1); the same test is used wherever this document says single-player [P1].
- Bracketed ids name the checks that back the statement next to them. They are in the dirty workspace's
  `checks/autosave-timing/`: `bin.py` (static analysis of `bin\xrGame.dll`, `bin\XR_3DA.exe`,
  `bin\xrCore.dll`), `data.py` (game data, after `extract.py`); `run.py` runs both.
- "Static" marks behaviour read from the program without running it; **believed** marks an inference
  the program does not show directly; **unestablished** marks what this investigation could not
  establish.

## 1. The world clock

1. The world clock is an unsigned 32-bit count of milliseconds. Its value is taken once per frame, at
   the start of the frame's update, from a high-resolution timer of the engine; all reads during the
   frame see that value [K1]. The engine's scheduler of section 2.3 uses the same value [K1b].
   The same timer read in seconds (floating point) at the same moment is the clock that times the
   automatic-save message (section 2.4) [K1].
2. The timer is a pausable timer: while the engine's pause of section 4 is on, it stands still, and when
   the pause ends it continues from the value it had, so paused time is never counted [K2, K3].
3. The timer runs at real time multiplied by a factor. The factor starts at 1.0 [K2]; the engine function that
   changes it is used by no shipped module [K4, K5]. The world clock is therefore real time
   with the paused periods removed. It does not follow game time (`sav` 12): changing the speed of game
   time does not change it [K5].
4. The engine also has a fixed-step mode in which every frame advances the world clock by exactly 20 ms,
   whatever the real time and also while paused. The mode is off at start [K6]; whether anything in the
   shipped game turns it on is **unestablished**.

## 2. The periodic automatic save

### 2.1 Settings

`config\alife.ltx` section `[alife]` (included by `config\system.ltx` [D1b]) gives two values of the
form `h:m:s`, three decimal integers separated by `:`. Each is converted to
((h × 60 + m) × 60 + s) × 1000 ms [A1]:

| Key | Shipped value | Milliseconds |
|---|---|---|
| `autosave_interval` | `01:05:00` | 3 900 000 |
| `delay_autosave_interval` | `00:00:30` | 30 000 |

[D1]. Both are read once, when the schedule of section 2.2 is created [A1]. What a value with fewer than
three fields gives is **unestablished**.

### 2.2 The schedule and its lifetime

The periodic save is driven by one **schedule** per client level. It holds the interval and the delay
(section 2.1), a **last-save time** and the **suppression counter** of section 3 [A1].

- It is created when the client's level is created, except on a dedicated server, and destroyed with the
  level [A2]. At creation the last-save time is set to now and the counter to 0 [A1].
- A new client level is created whenever a level is entered by starting or restarting the server: New
  Game, a load from outside a game, a load that restarts the server, a level change (`sav` 1.2, 11.2)
  (believed: [A2] shows only that the schedule is created with the level). Whether a load that reloads
  the simulation in place (`sav` 11.2) keeps the schedule is **unestablished**.
- Nothing else sets the last-save time: saving from the save screen and quick save do not restart the
  interval [A2, A3, A6].

### 2.3 When the schedule is checked

The schedule is a task of the engine's scheduler. Its update interval is configured as a minimum and a
maximum of 5000 ms with a priority scale of 0.5, which the scheduler turns into a fixed period: after
each check the next check is due 3000 ms of world clock later [S1, S2]. The first check is due at
creation [K1b]. A due task runs at the scheduler's next pass in a frame whose clock value is past the due time;
when the scheduler's time budget for a frame is used up, the remaining due tasks wait for a later frame
[S3] (the budget itself is **unestablished** here).

### 2.4 Each check

At each check [A3, A4]:

1. If no single-player simulation is running (the simulation `sav` 1.1 requires has not been started),
   nothing happens. **Disputed in review:** the check tests only whether the simulation has been started;
   it makes no game-type test of its own.
2. If last-save time + interval is not less than now (unsigned 32-bit arithmetic, the sum wrapping
   modulo 2^32), nothing happens.
3. Otherwise the save is due. If the player character exists, its health is above 0 and the suppression
   counter is exactly 0, the last-save time is set to now and the periodic automatic save of `sav` 1.1 is
   written (`<user>_autosave`, with the picture of `sav` 13 and the message of `screens` 10).
4. Otherwise the save is postponed: the last-save time is increased by the delay, so the save is next due
   30 000 ms after the time it was due now.

Nothing else is consulted [A4]: not the player-input switch of section 5, not open in-game windows. While
the menu is open in a single-player game the world clock stands still (section 4), so no check finds the
save due.

The message "Автосохранение..." (`screens` 10) is shown until 3.0 s of the world clock (section 1.1,
seconds) after the check that requested the save; a value in the layout file does not change this, and
the shipped `autosave` element of `config\ui\ui_custom_msgs.xml` has no `ttl` attribute [A5, D2].

Example with the shipped values and no pauses: a level entered at world clock T is saved at the first
check after T + 3 900 000 ms, i.e. about 65 minutes of unpaused play later; if the counter is not 0 then,
the save is retried at the first check after each further 30 000 ms until it succeeds; the next interval
counts from the successful save.

## 3. The suppression counter

The counter of `sav` 1.1 is a 32-bit integer of the schedule, 0 at creation (section 2.2) [A1].
It is changed by nothing but the enemy choice of creatures [A2, E5]; no script function changes it
directly (scripts reach it only through creature behaviour, e.g. the `wounded` state below) [A2, E8].

Each creature has a mark, "counted against the automatic save", which is off when the creature is
created [E1].

**Enemy-choice update.** As part of updating its memory, a creature performs an enemy-choice update (how
often is the creature AI's rule, not specified here). In it [E2, E4, E5]:

1. If the mark is on, the counter is decreased by 1 and the mark is turned off.
2. The creature re-chooses its enemy if one of the conditions below holds. Re-choosing scores every object
   on its current list of candidate enemies, in list order; if the player character is one of them, the
   mark is turned on.
3. If the mark is on, the counter is increased by 1.

**Teardown.** When the creature's online object is torn down with the mark on, the counter is decreased
by 1 and the mark is turned off [E3, E9]. That this teardown happens whenever the creature leaves the
client world, including when it is switched offline, is believed.

Between updates the counter is therefore the number of creatures whose latest enemy-choice update
re-chose and scored the player character.

**Re-choice conditions.** A creature re-chooses when any of these holds [E6]:

- it has no current enemy;
- its current enemy's health is 0 or less;
- it does not currently see its current enemy (seeing is the creature AI's rule). This condition applies
  only while the creature's "see check" is on: it is on when the creature is created [E1], and one stalker
  behaviour turns it off while it runs and restores it after; nothing else changes it [E10, E11];
- its current enemy is in the `wounded` state while at least one candidate is not. The `wounded` state of
  a stalker is set and cleared by the script method `wounded` of its game object [E8].

When not every candidate is `wounded`, some candidates are removed from the list before the conditions
are tested; which ones is **unestablished**. **Disputed in review:** established: every candidate in the
`wounded` state is removed; the other candidates stay, in their list order.

**Scoring outside a re-choice.** The rule that decides whether an object becomes a candidate also scores
objects, but only when the creature is a stalker and the object is not human; the player character
counts as human (as stalkers do, unlike monsters), so this scoring never turns the mark on [E7]. That
candidates are scored nowhere else is believed (no other place was found).

Not specified here: which objects a creature holds as candidate enemies, how it sees and remembers them
and how often it updates (creature AI). **Unestablished**: whether a dead creature still performs
enemy-choice updates; if it does not, a creature that dies with the mark on keeps counting until its
teardown, by the rules above.

## 4. Pauses and the menu

The engine's pause has a clock part and a sound part, which each event below switches separately; this
section concerns the clock part, called "the pause" here. While it is on, the world clock stands still
(section 1.2) and F6 and F7 do nothing (`screens` 9.2). Turning it on takes effect only in a
single-player game; turning it off always takes effect [P1].

The pause is turned on and off by [P2, P3, P4, I4, P5]:

| Event | Effect |
|---|---|
| The menu opens over a single-player game (`screens` 2) | on, unless it is already on |
| The menu closes | off, if it was off when the menu opened |
| Action `pause` pressed (key press of section 5) in a single-player game, while the level-change window is not shown | toggles |
| The level-change window is shown / hidden | on / off |
| The application loses focus in a single-player game | on (remembering whether it already was) |
| The application regains focus | off, unless it was on before the focus was lost |
| Console command `load` in a running game | off, before loading |
| One further loading event of the game (when it occurs: **unestablished**) | off |
| Interface video and picture items start or stop | change it as well (their rules: **unestablished**) |

These are all the places that change it [P5].

Effect on the periodic save: no check finds it due while paused, and paused time counts neither towards
the interval nor towards the delay. When the pause ends the last-save time is unchanged, so the save
comes due after the remainder of the interval of unpaused time (section 2.4).

## 5. The player-input switch

### 5.1 The switch

A single on/off switch, off when the program starts [I2]. It is turned on only by the script function
`level.disable_input()` and off only by `level.enable_input()` [I1, I2]; the engine never resets it, so it
keeps its state across loads, level changes and New Game [I2] (the player character's script binder
turns it off when the player character is spawned, section 5.3).

While it is on, the level ignores [I2, I3, I3b, I4]:

- every key press, except the keys bound to the actions `pause`, `screenshot`, `quit` (which opens the
  menu, `screens` 2) and `console`, which are handled before the switch is tested;
- every key release and key hold;
- the mouse buttons, which the level handles as key presses, releases and holds;
- mouse movement and the mouse's other event (believed: the wheel).

The switch does not affect the periodic automatic save (section 2.4) [A4].

### 5.2 F6 and F7

With the menu closed (`screens` 9.2), the actions `quick_save` (F6) and `quick_load` (F7) act only if
all of these hold [I4, I5, I6]:

1. the player-input switch is off;
2. the in-game interface (HUD) exists;
3. no in-game window is open: an open window is offered the key first and keeps it, whether or not it
   acts on it; two further parts of the in-game interface may also take the key (which: **unestablished**);
4. the pause is off;
5. the game is single-player.

Then F6 executes the console command `save` (no name) and F7 executes `load <user>_quicksave` if that save
is valid, as in `screens` 9.2. The single-player game itself takes no key before them [I4].

### 5.3 What the shipped scripts do with it

`level.disable_input()` and `level.enable_input()` are called (outside comments) by these scripts [D4]:

| Script | `disable_input` | `enable_input` |
|---|---|---|
| `scripts\xr_effects.script` | 3 | 3 |
| `scripts\_g.script` | 1 | 0 |
| `scripts\avi_start.script` | 1 | 1 |
| `scripts\bind_stalker.script` | 0 | 2 |
| `scripts\sr_cutscene.script` | 0 | 1 |

`scripts\lua_help.script` lists both functions [D4b]. What they do:

- `xr_effects.script`: the effects `disable_ui` (which also hides the weapon and the HUD indicators) and
  `enable_ui`; `disable_ui_elements` and `enable_ui_elements` turn the switch on or off when given `all`
  or `input` [D5c].
- `_g.script`, `set_inactivate_input_time(delta)`: records the current game time and `delta` (game
  seconds) in the player character's script storage and turns the switch on [D5]. Its only caller is the
  effect `actor_punch` of `xr_effects.script`, with 30 [D5b].
- `bind_stalker.script` (the player character's script binder): in its update, once at least `delta` game
  seconds have passed since the recorded time, it turns the switch off and clears the record; when the
  player character is spawned, it turns the switch off unless a record is present [D6].
- Saving and loading that record (`bind_stalker.script`): with no record the save writes a boolean false;
  with a record the save code calls a method on a variable that no shipped script defines (`packer`), a
  script runtime error (its consequence is the script environment's, SE-4 of the scoping
  specification). On load the code reads a byte and compares it with the boolean `true`; values of
  different types are never equal in the script language, so the record is never restored from a save
  [D7].
- `avi_start.script`: turns the switch on, shows a dialog, waits 16 000 ms, removes the dialog and turns
  the switch off [D8].
- `sr_cutscene.script`: when a scripted scene ends while the player character is talking, it only turns
  the switch off; otherwise it applies `enable_ui` [D9].

## 6. Verification facts

- `[alife]`: `autosave_interval = 01:05:00` → 3 900 000 ms, `delay_autosave_interval = 00:00:30` →
  30 000 ms [D1].
- Check period of the schedule: 3000 ms (configured minimum and maximum 5000 ms, priority scale 0.5) [S2].
- The automatic-save message lasts 3.0 s of the world clock [A5].
- Action identifiers of the key-action table: `pause` 0x28, `screenshot` 0x2E, `quit` 0x2F, `console`
  0x30, `quick_save` 0x4B, `quick_load` 0x4C [I5].
- Script call sites of `level.disable_input` / `level.enable_input`: 5 / 7 in 5 files (table of 5.3)
  [D4].
- The player character's configuration class: `O_ACTOR` [D3].

## 7. Unestablished

- Whether anything in the shipped game turns on the fixed-step mode of the world clock (1.4).
- What an `[alife]` time value with fewer than three fields gives (2.1).
- Whether a load that reloads the simulation in place keeps the schedule and its last-save time (2.2).
- The scheduler's per-frame time budget (2.3).
- Which candidates are removed when not every candidate is `wounded` (3). **Disputed in review:** established
  in section 3 (the `wounded` ones).
- Whether a dead creature still performs enemy-choice updates (3).
- How interface video and picture items change the pause, and when the further loading event of 4
  occurs (4).
- Which further parts of the in-game interface can take F6 and F7 (5.2).
- That candidates are scored only in the two places of section 3 (believed).
- That the mouse's remaining event is the wheel (5.1; believed).
- That a creature's teardown happens when it is switched offline (3; believed), and that every way of
  entering a level creates a new client level (2.2; believed).

## Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`, `bin\xrCore.dll`; all modules of `bin\`
  scanned for imports.
- Data examined: `config\alife.ltx`, `config\system.ltx`, `config\creatures\actor.ltx`,
  `config\ui\ui_custom_msgs.xml`, every `scripts\*.script` (winning copies).
- Analysis: static analysis (disassembly and decompilation with Ghidra, headless) of the periodic-save
  schedule, the engine clock, pause and scheduler, the creatures' enemy choice as far as it changes the
  suppression counter, the level's input handling, the script functions `level.disable_input`,
  `level.enable_input` and `wounded`, and every place the pause is changed; reading of the scripts. The
  original was not run. Every bracketed claim is reproduced by the checks named in the conventions.

## Gaps found in review

- The player character's script binder replaces its script storage with an empty one when the player
  character is reinitialised, which discards any record of section 5.3; whether that happens before the
  spawn check of section 5.3 (and so whether "unless a record is present" can ever apply) is not stated.
