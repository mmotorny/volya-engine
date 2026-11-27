# Console key bindings

How the original's console binds a key to a console line and what pressing that key does: the commands
`bind_console` and `unbind_console`, the table of console-line bindings they edit, how it is saved and
cleared, when a press of a bound key executes its line, and the two listing commands `list_actions` and
`bind_list`. It is item 5 of section 11 of `docs/behavior/console-scope.md` and builds on
`docs/behavior/console-commands.md`. What could not be established is marked **unestablished**; a
statement inferred without a direct check is marked **believed**, with why. Facts marked **measured**
come from headless runs of the shipped game (Provenance); the others from the shipped binaries.

Conventions:

- "The game library" is `bin\xrGame.dll` of the shipped Russian edition (GOG build); all four commands
  belong to it.
- Specifications referred to (`docs/behavior/` unless noted): CC `console-commands.md`, CS
  `console-scope.md`, OS `options-screens.md`, AT `autosave-timing.md`, SLS `save-load-screens.md`, IS
  `inventory-screen.md`.
- A **line** and the **log** are as in CC (conventions). Log lines are quoted between backticks exactly
  as written, `<…>` standing for a variable part; "prints" means "writes as one log line".
- **Actions**, their **kinds** (1 any game, 3 single player only, 5 multiplayer only), the **key names**
  with their key codes, and a key's **display name** are those of OS 11.1 and 11.2. A key's **action**
  bindings are the primary and secondary slots of OS 11.1.
- **The level** is the running game's world as the player plays it; "a press reaches the level" means the
  press is not taken by the console or the menu before it (which of them takes it while open is the
  subject of the console-window and menu specifications).

**Checks.** Every claim carries a tag [KB-*n*] (static, `console-keybind/checks/static.py` and, for KB-14 to
KB-19, `console-keybind/checks/interface.py`: read-only analysis of the game library and of `bin\XR_3DA.exe`,
and data values read from them) or [KM-*n*] (measured,
`console-keybind/checks/measured.py`: the logs and files of the headless runs in `console-keybind/run/`),
both in the dirty workspace; `console-keybind/checks/run.py` runs both. All checks pass.

## 1. The console-line bindings

- Besides the action bindings, the game keeps one table of **console-line bindings**: each entry is a key
  (by its key code) and one line. A key has at most one entry; keys are kept in ascending order of key
  code [KB-5, KB-7]. Only `bind_console`, `unbind_console` and `unbindall` change it, and only saving
  (section 4) and a key press (section 5) read it [KB-5b]. At startup `unbindall` empties it before the
  user configuration file is executed (OS 12.2) [KB-8].
- The table is independent of the action bindings: binding a line to a key leaves the key's action
  bindings as they are, and `bind`, `bind_sec`, `unbind` and `unbind_sec` (OS 11.5) do not touch the table
  (measured: after `bind_sec forward kUP`, `bind forward kO` with `kO` bound to a line, and `unbind back`,
  the `bind_console` lines were saved unchanged) [KB-5, KM-3b].
- No control of the options screens creates or shows these bindings (OS 12.3).

## 2. `bind_console`

`bind_console <words> <key name>` [KB-3, KB-4, KB-5, KM-2]:

1. The argument (already processed and lower-cased as every line is, CC 2) is split at its spaces. The
   last word is the key name; the words before it, each followed by one space, are the line. So the line
   ends in a space (`flush kO` binds `flush ` to `kO`), and with a single word the line is empty and the
   word is the key (`kL` binds an empty line to `kL`) (measured).
2. The key name is looked up among the key names of OS 11.2, ignoring case (measured: `KP`, `MOUSE1`,
   `kf5` work).
3. The key's entry is set to the line; an earlier line of that key is replaced (measured: `bind_console
   screenshot kF5`, then `bind_console flush kF5`, keeps only `flush `) [KM-2b].
4. Nothing is printed [KM-2e].

- An **unknown key name** prints `! cant find corresponding [_keyboard*] for keyname <name>` and then
  reads the key from a missing entry: the game library does not test for it [KB-4, KB-4b]. Measured: after
  `bind_console flush kNOSUCH` the log showed that message (with the name lower-cased, `knosuch`) and the
  program crashed: a crash dump was written and the next line was never executed [KM-5].
- The bare name `bind_console` prints `- bind_console ` (CC 2, step 6: its status is always empty, whatever
  is bound) (measured) [KM-1].
- Help text `no arguments`; saving: section 4 (CC 8).

## 3. `unbind_console`

`unbind_console <key name>` removes that key's entry, if it has one; a key without one is left as it is,
and nothing is printed either way (measured: `unbind_console kf5` removed the `kF5` entry,
`unbind_console kq` changed nothing) [KB-6, KM-2c]. The whole argument is the key name: it is not split at its spaces, and as
no key name contains a space, an argument with a space in it is always an unknown key name (so
`unbind_console kf5 x` looks up `kf5 x`; static, not measured) [KB-18]. The key name is looked up as in section 2, step 2; an
unknown key name prints the same message and has the same fault as there (measured: `unbind_console
kNOSUCH` crashed the same way) [KB-4, KB-4b, KM-5b]. The bare name prints `- unbind_console ` [KM-1]. Help text `no arguments`; not saved (CC 8).

## 4. Saving and `unbindall`

- `cfg_save` writes, at the position of `bind_console` in the command order (OS 12.3), one line per entry
  in ascending key-code order: `bind_console ` + the entry's line + ` ` + the key's name (OS 11.2, with the
  case of that list, e.g. `kNUMPAD7`, `mouse1`) + LF (a single byte 0x0A, unlike the CR LF of every other
  line) [KB-7, KM-2]. As the stored line ends in a space, the written line has **two** spaces before the
  key name; with an empty line it is `bind_console  <key>` (measured):

      bind_console bind_list  k9
      bind_console flush  kO
      bind_console cfg_save kb_test  kP
      bind_console  kL
      bind_console screenshot  kF5
      bind_console hud_info off  kNUMPAD7
      bind_console r2_gloss_factor 0.5  mouse1

  (from the configuration lines `bind_console bind_list k9`, `bind_console flush kO`, `bind_console
  cfg_save   Kb_Test  KP`, `bind_console kL`, `bind_console Screenshot   kF5`, `bind_console
  r2_gloss_factor 0.5 MOUSE1`, `bind_console hud_info off kNUMPAD7`) [KM-2]. Read back, such a line binds
  the same line again (its spaces are processed as in CC 2).
- `unbindall` (OS 11.5) empties the table as well as every action slot, then executes
  `default_controls.ltx`, which has no `bind_console` lines (measured: a configuration file saved after
  `unbindall` has no `bind_console` line) [KB-8, KM-2d]. At startup `unbindall` runs before the user
  configuration file (OS 12.2), so the bindings saved there are restored at every start.

## 5. Pressing a bound key

A press of a key reaches the level's key handling (only a press: releases and holds never execute a
console line) [KB-9]. A press of the left, right or middle mouse button reaches it as a press of the key
`mouse1`, `mouse2` or `mouse3` [KB-12]. The game reads the mouse with eight buttons; a press of buttons 4 to
8 reaches the level's key handling directly as a press of the key `mouse4` … `mouse8` (key codes 340–344, OS
11.2), so a line bound to one of them is executed by the rules below like that of any other key (static, not
measured) [KB-19]. The level then handles the press in this order, the first rule
that applies ending it [KB-10, KB-10b, KB-11, KB-11b, KB-13]:

1. The key's **action** is the first action, in the order of OS 11.1, whose primary or secondary slot holds
   the key, counting only actions of kind 1 and of the current game's kind; the current kind is 3 in
   single player (and before any game) and 5 in the multiplayer game types; a key without such an action
   has none (so in single player a key bound only to a multiplayer action, e.g. `kO` of `skin_menu`, has
   none) [KB-11, KB-11b].
2. If the action is `pause`, `screenshot`, `quit` or `console`, it is handled as such (AT 5.1; SLS 2;
   OS 13) and the key's console line, if any, is **never** executed [KB-10].
3. Otherwise nothing more happens when any of these holds: the player-input switch is on (AT 5.1); the
   in-game interface does not exist; the in-game interface takes the key (below); the pause is on; the
   game type's own handling takes the action (AT 5.2: the single-player game takes no key) [KB-10, KB-13].
   The in-game interface is offered the press as follows [KB-14, KB-15, KB-16, KB-17]:
   - If an in-game window is open, the topmost one is offered the press first; whatever it and the game
     type's interface (next item) do with it, the in-game interface takes the press while any in-game
     window is open (AT 5.2, item 3).
   - The game type's interface is then offered the press. **Disputed in review:** it is offered the press
     only when the topmost window (if any) did not take it; a press the topmost window takes ends there
     (taken), without reaching the game type's interface. This changes nothing for console lines, as any
     open window takes the press either way. In single player it acts only while the game is
     not paused and a player character exists with health above 0 (IS 1.1), and only on these actions:
     - `inventory`: when no in-game window is open, the inventory screen opens; when the inventory screen
       is the topmost window, it closes (IS 1.1, 1.2). Either way the press is taken.
     - `active_jobs`, `map`, `contacts`: the same with the PDA window instead of the inventory screen; the
       press is taken.
     - `scores`: acted on (what it shows is outside this specification), but the press is **not** taken,
       so it goes on through rules 4 to 6.

     Every other action, and these ones when the conditions do not hold (e.g. with the character dead),
     is not taken. What the multiplayer game types' interfaces take is **unestablished** (section 8).
   - No other part of the in-game interface takes a press in the shipped game: the one further part that
     could, an adjustment mode, is never switched on [KB-15].

   So a press of a key bound to `inventory` that also has a console-line binding opens (or closes) the
   inventory screen and its line is **not** executed; only when the interface does not act on it (no
   living player character) does the press reach rule 5, and then the line is executed. A key bound to
   `scores` both acts as `scores` and executes its line, in that order. (Static, not measured.)
4. In single player, the actions `quick_save` and `quick_load` act as in AT 5.2 and the console line is
   not executed. In a multiplayer game these actions do not act here, so the press goes on to rule 5
   [KB-10b].
5. If the key has a console-line binding, its line is executed as a console line (CC 2: as a line the
   programs execute, so it is not echoed as `~ <line>`), and the press is used up: the player character
   does not get the action's press, though it still gets the key's hold and release (believed: those go
   their own way, which consults no console-line binding [KB-9]) [KB-9b]. An empty line is executed and
   does nothing (CC 2, step 1) [KM-4b].
6. Otherwise the press goes to the player character (its action, if any).

So a console-line binding acts only in a running game, while the level gets the key: never in the main
menu, never while the console or the menu is open, never while paused, never while an in-game window is
open, and in single player never on a press the interface takes for `inventory`, `active_jobs`, `map` or
`contacts` [KB-9, KB-10, KB-17]. A press it does not act on is not kept for later [KM-6]. Measured: section 7.

## 6. `list_actions` and `bind_list`

- `list_actions` prints `- --- Action list start ---`, then `- <action>` for each of the 77 actions in the
  order of OS 11.1 (all of them, whatever the game type), then `- --- Action list end   ---` (three
  spaces before the last `---`) [KB-1, KB-1b, KM-1b].
- `bind_list` prints `- --- Bind list start ---`, then for each of the 77 actions in that order
  `[<action>] primary is[<primary>] secondary is[<secondary>]`, then `- --- Bind list end   ---` (three
  spaces); `<primary>` and `<secondary>` are the display names of the keys in the slots (OS 11.2), or
  `NULL` for an empty slot [KB-1, KB-2, KM-1c]. Measured under Wine (whose input layer supplies the display
  names): `[left] primary is[Left] secondary is[NULL]`, `[jump] primary is[space] secondary is[NULL]`,
  `[wpn_fire] primary is[mouse1] secondary is[NULL]`, ``[console] primary is[`] secondary is[NULL]``, and
  after `bind_sec forward kUP`: `[forward] primary is[W] secondary is[Up]`, `[up] primary is[NULL]
  secondary is[NULL]` [KM-1c, KM-3].
- Neither lists the console-line bindings [KB-1b, KB-2]. Both use no argument and run when entered bare
  (CC 8) [KB-1c].

## 7. Measured in a game

Single player, New Game, with lines `cfg_save <name>` bound to keys (each press that executes its line
writes the file `<name>.ltx`), keys sent one at a time; another key bound to `flush` was pressed after
each test to show the level was taking presses again:

| Key pressed (its action) | Situation | Line executed |
|---|---|---|
| `kO` (`skin_menu`, kind 5) | in the game | yes [KM-4] |
| `kW` (`forward`) | in the game | yes [KM-4] |
| left mouse button (`mouse1`, `wpn_fire`) | in the game | yes [KM-4] |
| `kNUMLOCK` (none; `pause` is `kPAUSE`) | in the game | yes [KM-4h] |
| `kJ`, bound to an empty line | in the game | nothing happens; the game goes on [KM-4b] |
| `kF6` (`quick_save`) | in the game | no; the quick save `<user>_quicksave.sav` was written [KM-4c] |
| `kESCAPE` (`quit`) | in the game | no; the menu opened [KM-4d] |
| `kU` (`team_menu`, kind 5) | menu open during the game | no; after closing the menu, yes [KM-4d] |
| `kT` (none) | console open | no; after closing the console, yes [KM-4e] |
| `kPAUSE` (`pause`) | in the game | no; the game paused [KM-6] |
| `kSEMICOLON` (none), `kK` (`flush`) | paused | no; neither executed after the pause ended [KM-6] |
| `kF12` (`screenshot`), `kGRAVE` (`console`) | in the game | no [KM-6b] |
| `kSLASH` (none) | inventory window open | no; after closing it, yes [KM-6c] |
| `kZ` (`speech_menu_1`, kind 5), bound to `list_actions`, held 8 s | in the game | once [KM-4f] |
| `kEND`, bound to `quit` | in the game | yes: the game ended normally [KM-6d] |
| `kK`, `kO`, `kW`, `kT`, left mouse button | main menu (no game) | no; a line typed in the console there was executed [KM-7] |

No executed line was echoed to the log as `~ <line>` [KM-4g].

## 8. Unestablished

- The longest line and key name `bind_console` accepts.
- Whether the crash on an unknown key name (sections 2 and 3) also happens under Windows (measured under
  Wine only; believed, as the fault is a read through a missing entry, KB-4b).
- What the multiplayer game types' own handling and their in-game interfaces take (section 5, rule 3);
  no multiplayer game was run, and their interfaces were not examined.
- Measurements of section 5, rule 3 (the inventory and PDA actions, `scores`), of mouse buttons 4–8 and of
  `unbind_console`'s argument: these were established statically only, as no headless run could be made
  for this revision.

## Provenance

- Date: 2026-11-26 (revised the same day: section 5, rule 3; mouse buttons 4–8; `unbind_console`'s
  argument). Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user, installed by the project's install assembly (`@gog_original`).
- Static analysis: Ghidra 12.2, headless and read-only, of `xrGame.dll` (the four commands, the
  console-line table, its saving, `unbindall`, the level's key press handling, the in-game interface's and
  the single-player interface's key press handling, the mouse-button mapping, the current game kind) and
  of `XR_3DA.exe` (the mouse input); data values and strings read directly from the files.
- Execution: headless runs of the unmodified `XR_3DA.exe` under Wine in the project's container
  (`xtask wine run`), with `--skipmemcheck -silent_error_mode -nosound -ltx cmd.ltx`, and for the game
  runs `-start server(all/single/alife/new) client(localhost)` (a New Game). The install's `fsgame.ltx`
  pointed the user-data folder into the run folder, and a loose copy of the shipped
  `scripts\ui_main_menu.script` with one added line executed a second configuration file
  (`cfg_load menu_tests`) when the main menu was created (as in CC). Key presses and mouse clicks were sent
  to the container's virtual display from outside the game (`xdotool`). Evidence: the log file (`flush`),
  the configuration files the game wrote (`cfg_save`), the saved games.
- No leaked source code, derived projects or third-party documentation of the engine were used.
