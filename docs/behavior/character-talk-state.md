# Character talk state

Behaviour specification CH-7 of `docs/behavior/characters-scope.md` (cited as `scope`; its sections 2.5 and 5, with
its "Gaps found in review"), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): the talk state that the
engine keeps for each character. It covers which objects can talk (by class id and by class), the talk-enabled flag
of each character with its initial value, what changes it and what reads it, the character's own side of a talk
(the talk flag and the partner) as state, the character's current start dialog and default start dialog as state,
what of all this a save holds, and the character's part while it talks: what the engine does (nothing of its own
beyond the rules of the talk) and, as worked examples, what the shipped scripts do with the state through the
script interface (the stalkers under `xr_meet` and `xr_wounded`, the trader under `mob_trader`, the binders and the
dialog functions).

Builds on (not repeated; cited by name and section):

- `docs/behavior/talk.md` (`DG-3`): the three talk values of a character (its conventions), starting and stopping a
  side (1), the offer (2.2), the end of a side at the scheduled update (5.1), the window (5.2), what `is_talking`
  reports (5.4), the start dialog at coming online, by scripts and in the stalker's update (6), the talk contacts
  (7). This specification states the same state from the character's side and adds what DG-3 leaves to CH-7.
- `docs/behavior/dialog-flow.md` (`DG-4`): a dialog once started; nothing of it depends on the state here beyond
  DG-3's.
- `docs/behavior/script-api-dialogs-and-trade.md` (`R`): the members `enable_talk`, `disable_talk`,
  `is_talk_enabled`, `is_talking`, `stop_talk` (4.1), `run_talk_dialog` (4.2), `set_start_dialog`,
  `get_start_dialog`, `restore_default_start_dialog` (4.4) as scripts call them.
- `docs/behavior/script-api-game-object-basics.md` (`M`): the kinds of object (2.2: *inventory owner*, the 19
  creature classes and the player), a member on an object of the wrong kind (2.3), the handle and its object (2.1).
- `docs/behavior/script-api-class-ids-and-the-object-factory.md` (`C`): class ids, their registration (its table
  of the shipped registrations).
- `docs/formats/character-descriptions.md` (`CH-1`, 6: `start_dialog`) and `docs/behavior/character-identity.md`
  (`CH-2`, 5: the online character).
- `docs/formats/sav.md` (`sav`): 7.4.2 (the stalker's update with its start dialog; the trader's empty update), 8
  (client data, the inventory-owner block).
- `docs/behavior/offline-simulation.md` 6.3 (switching online builds the in-game counterpart, switching offline
  destroys it).
- `docs/behavior/head-bone-turning.md` (`TC4`): the trader's head callback, which reads only the two objects'
  positions and the trader's heading (3).
- `docs/behavior/trader-and-crows-scope.md` 1.1 (the trader's logic file and `mob_trader`, the start chain and its
  `can_talk`), `docs/behavior/use-action.md` 6.2 (the use action's talk), `docs/behavior/script-loading.md` (scripts
  load when named), `docs/formats/dialog-files.md` 3.2 (loading a dialog by an id no listed file defines is fatal).

Conventions:

- **Character**, **stalker**, **trader**, **monster**, **player character**: as `scope` (class ids) and `R`
  (classes `CAI_Stalker`, `CAI_Trader`; monsters the other 17 creature classes). **Partner**: as `DG-3`.
- A character's **talk values** are the three of `DG-3`'s conventions: the **talk-enabled flag**, the **talk flag**
  (talking or not) and the **partner**. Stalkers and traders also have a **start dialog** and a **default start
  dialog** (`R` 4.4).
- **Online object**: the in-game counterpart of a simulation object (`offline-simulation` 6.3; `M`'s engine object).
- Statements are **static** (read from the shipped `bin\xrGame.dll`, 6,246,400 bytes; the game was not run) or
  **measured** (from the game data, including the shipped scripts' text) unless marked **believed** (with the
  reason) or **unestablished**. What a shipped script does is measured from its text; that it runs as read is
  believed wherever it depends on the script runtime and on approved specifications of other systems.
- **Checks**: every claim carries a label in brackets, an assertion of `checks/character-talk-state/checks.py` in
  the dirty workspace (`python3 -I checks.py`; `B` from the static analysis, `D` from the data). All 27 pass.
  Statements cited from an approved specification carry that specification's checks.

## 1. Which objects can talk

### 1.1 Where the talk state is kept

Every object of the inventory-owner kind (`M` 2.2: the 19 creature classes and the player) has the three talk
values [B1]. The talk-enabled query that the offer of `DG-3` 2.2 asks, and that `is_talk_enabled` returns, gives:

| Objects | Talk-enabled query |
|---|---|
| the player character, stalkers (`CAI_Stalker`), traders (`CAI_Trader`) | the talk-enabled flag [B1] |
| the 17 monster classes | always false, whatever the flag holds [B1] |

The offer asks this query of the offered character before anything else and refuses when it is false [B2]. So:

- **Monsters never talk**: an offer to one is always refused; `enable_talk` and `disable_talk` still set and clear
  a monster's flag, but nothing reads it (`R` 4.1) [B1, B3].
- **Stalkers and traders** talk when their flag is set and the other conditions of `DG-3` 2.2 hold.
- **The player character's** flag is read only if the player character is itself offered a talk, which happens only
  when a script calls `run_talk_dialog` with the player character as the partner (`R` 4.2); no shipped script does:
  every call in the scripts that load is on the player character with another character as the argument [D15]. So in the shipped
  game the player character's flag has no effect.

Objects of any other kind have no talk values; the talk members do nothing on them (`R` 4.1, `M` 2.3).

### 1.2 By class id

| Class id | Client class | Records in `spawns\all.spawn` |
|---|---|---|
| `AI_STL_S` (scripted) | `CAI_Stalker` | 1,073 |
| `AI_TRD_S` (scripted) | `CAI_Trader` | 1 (the trader, `escape_trader`) |
| `AI_STL` (engine) | `CAI_Stalker` | 0 (one section, `m_izgoy`, `scope` 1.2) |
| `AI_TRADE` (engine) | `CAI_Trader` | 0 |

[B9, D1, D2]. So the characters that can talk in the shipped game are the 1,073 stalkers and the trader, and a
stalker of section `m_izgoy` if one is created during play (`scope` 1.2).

## 2. The talk-enabled flag

### 2.1 Initial value

The flag is set (talk enabled) when the online object is made, together with the trade-enabled flag (`DG-3` 1)
[B4]. Coming online (the inventory owner's spawn step) and its reset do not touch it [B5]. An online object is made
anew every time the character switches online (`offline-simulation` 6.3), at a New Game, after a load, and on every
return from offline alike; so **each time a character comes online its talk is enabled**, until a script disables
it [B4, B5].

### 2.2 What changes it

Only `enable_talk` (sets it) and `disable_talk` (clears it) (`R` 4.1). Nothing in the engine changes it: not
starting or ending a talk, not death, combat, a hit or a change of relation; the only writers of the flag are the
object's making and the two routines behind these members, and those routines are reached only from the two script
members [B3]. A change takes effect at the next offer; it does not end a talk in progress (`DG-3` 1) [B3].

### 2.3 What reads it

Only the talk-enabled query of 1.1, which is asked only by the offer (`DG-3` 2.2) and by `is_talk_enabled` (`R`
4.1) [B3]. In particular the hint text shown when the player character aims at a character does not depend on it
in the engine [B3]; the shipped binders set that text from it (4.3) [D11].

### 2.4 Saving

The flag is not saved [B6, D14]:

- the engine part of the client data (`sav` 8) of the player, a stalker and a trader holds, in its inventory-owner
  block, the active slot, then the character information's start dialog (6.2), then a text and a number of the
  inventory-owner part (their meanings are not part of this specification); none of the three talk values is
  written or read [B6];
- no other part of `sav` holds it (`sav` 7.4.2: the stalker's and the trader's state and update have no such
  field);
- the scripted part of the client data, written by the binders, holds none of it (4.6) [D14].

So after loading a save a character's talk is enabled when it comes online (2.1), and whatever state the shipped
scripts want is set again by them (4.6).

## 3. The character's side of a talk

The talk flag and the partner form the character's side (`DG-3` conventions). As state:

- **Written** only by starting a side (`DG-3` 1: the flag set, the partner kept; also the player character's own
  start, `DG-3` 2.2) and stopping one (`DG-3` 1, 5.1: the flag cleared, the partner forgotten) [B7].
- **Initial value**: making the online object gives neither the talk flag nor the partner a value, and coming
  online does not set them [B4, B5]: what they hold before the first start is **unestablished** (**believed** not
  talking and no partner: a set flag would make the scheduled update of `DG-3` 5.1 read the partner, and the shipped
  scripts call `is_talking` on characters that never talked, 4.1, 4.3).
- **Not saved**: neither is in the client data or the simulation object's state [B6]; the talk itself after a load
  is `DG-3` 7's (unestablished there).
- **Read** by `is_talking` (`R` 4.1, `DG-3` 5.4) and by the scheduled-update rule of `DG-3` 5.1 (the partner's flag)
  [B8]. Every other reader found concerns the player character only: the talk window's set-up, update and hide
  (`DG-3` 3, 5.2), the use action's first step (`use` 6.2), and one routine of the player character's own that also
  shows the hint texts `cant_walk` and `cant_walk_weight`, whose behaviour is not part of this specification
  (**unestablished** here) [B8]. Nothing of a stalker's or trader's own movement, sight, memory, decisions, motions
  or sounds reads its talk flag or its partner [B8].

## 4. The character's part while talking

### 4.1 What the engine does

Beyond the rules of `DG-3` (the scheduled update that stops the side, 5.1; the player's view turning toward the
partner, 4) the engine does nothing to a stalker or a trader because it talks: it does not stop it, turn it, change
its motions or sounds, or hold its decisions [B8]. The trader's head turns toward the level's controlled object (the
player character) whether he talks or not: the head callback reads only the two positions and the trader's heading
(`TC4` 3). Whatever else a character does while it talks comes from the shipped scripts, through the script
interface; 4.2 to 4.6 give them as worked examples. The scripts named in `scope` 1.7 that call the talk members are
[D4]:

| Member | Callers (comments removed) |
|---|---|
| `enable_talk` | `mob_remark`, `mob_trader`, `xr_assistance`, `xr_companion`, `xr_meet`, `xr_sos`, `xr_wounded` |
| `disable_talk` | `dialogs`, `mob_remark`, `mob_trader`, `xr_assistance`, `xr_meet`, `xr_wounded` |
| `is_talk_enabled` | `bind_monster`, `mob_remark`, `xr_motivator`, `xr_use` |
| `set_start_dialog` | `xr_meet`, `xr_sos`, `xr_wounded` |
| `restore_default_start_dialog` | `xr_meet` |

`xr_assistance`, `xr_companion`, `xr_sos` and `xr_zoneguard` are named by no other script and no configuration file,
so they never load (`script-loading`) and none of their calls runs [D5]; `modules` loads the schemes `xr_meet`,
`xr_wounded`, `xr_use`, `mob_remark` and `mob_trader` [D5]. `dialogs.disable_talk_self` and
`dialogs.disable_talk_victim` disable talk on the first or the second speaker of a dialog, but no configuration file
names them [D6].

### 4.2 Stalkers under `xr_meet` (believed: the script's reading)

At each update of its meet manager (while the meet scheme's contact evaluator runs) a stalker [D7]:

- with the player character holding a weapon picks `use_wpn`, otherwise `use` (condition lists of the active
  section; defaults for a section without its own meet settings: `use = true`, `use_wpn = false`); `true` enables
  talk; `false` disables it and stops the stalker's side if it talks; `self` starts a talk from the player character
  (`run_talk_dialog`, `DG-3` 2.1) unless the stalker already talks;
- stops its side when it talks and the player character is more than 3 m away (squared distance above 9), unless
  the pick is `self`;
- while asleep (`xr_sleeper`) disables talk and stops its side;
- when the planner's enemy property is true, disables talk.

While the stalker talks [D8]:

- its distance to the player character is taken as 0, so the nearest entry of `meet_state` applies (default
  `5|wait@wait`: the state `wait`), and the look target is the `victim` entry (default `actor`, the player
  character);
- no meet sound is chosen (and none while the player character talks with someone else);
- the meet action, when it starts, clears the stalker's desired position and direction and then sets the state
  with the victim as the object to look at.

So, **believed**, a stalker talking under the default meet settings stands in the state `wait`, looking at the
player character, without meet sounds; what the state and the look target do is `scope` CH-10 and CH-11's. The
meet manager also sets the start dialog: the pick of the section's `meet_dialog`, or the default start dialog when
it picks nothing (6.1) [D9].

### 4.3 The use hint (binders)

At each update of a living stalker (`xr_motivator`) or a living monster-type object (`bind_monster`, which binds the
trader) the binder sets the hint text to `character_use` while `is_talk_enabled` is true and to an empty text
otherwise; a dead one gets the default hint [D11, D9b]. What the hint line shows is `hud-scope`'s.

### 4.4 Wounded stalkers (`xr_wounded`)

When the wounded action starts it sets the section's `help_start_dialog` as the start dialog (if given); while it
runs it disables talk when the stalker's relation to the player character is enemy and enables it otherwise; when
it ends it disables talk; and it does not switch section while the stalker talks [D13].

### 4.5 The trader (`mob_trader`, `mob_remark`)

The trader is set up as a monster-type object (his section's binder is `bind_monster.bind`) [D9b]. On entering a
section `mob_trader` enables talk when the section's `can_talk` is true (the default) and disables it otherwise
[D10]; which sections set it is `trader-and-crows-scope` 1.1's. `mob_remark` (not used by the trader's logic file,
`trader-and-crows-scope` 1.1) disables talk at its reset and then follows its `dialog_cond` [D10]. At every scheme
switch of a monster-type object the default start dialog is restored: the override that would set another
(`meet_dialog`) is never read [D9]. So the trader's start dialog is always his default.

### 4.6 The player character and what the scripts save

The player binder hides the player character's weapon while it talks and restores it afterwards [D12] (this answers
`DG-3`'s review gap on who owns it: the player binder, a script). The
stalker binder saves a boolean, then the logic state (with the active section), the dialog manager's and the trade
manager's state; the monster binder saves the logic state and, for the trader, the trade manager's; none of these
holds a talk value [D14]. **Believed** (from the logic state holding the active section): after a load the active
section is entered again, so `mob_trader` and `xr_meet` set the talk state again; how a section is re-entered on
load is `xr_logic`'s and `scope` CH-27's.

## 5. Being in a talk, end to end

For one character (stalker or trader), with the rules of `DG-3`:

1. it comes online: talk enabled (2.1); its start dialog and default start dialog are its description's (6.1);
   the scripts then set its state (4);
2. it is offered a talk: accepted only if its query (1.1) is true and both healths are above 0 (`DG-3` 2.2); it
   starts its side;
3. while it talks: `is_talking` is true; enabling or disabling talk changes nothing of the talk (2.2); the engine
   does nothing else to it (4.1);
4. its side ends by `stop_talk`, or at its scheduled update when its partner no longer talks or its health is 0 or
   less (`DG-3` 5.1);
5. it goes offline: the online object, with all its talk values, is destroyed (`offline-simulation` 6.3); only the
   stalker's start dialog goes to its update (6.1).

## 6. The start dialog

### 6.1 As state of the character

Only stalkers and traders have a start dialog and a default start dialog [B10] (`R` 4.4). They are written only
when the online object's start-dialog part is made (both empty) and freed, by `set_start_dialog` (the start dialog),
by `restore_default_start_dialog` (the start dialog becomes the default) and at coming online (both from the
character information, when the start dialog is empty: always for a new online object, `DG-3` 6) [B10]. So:

- **current start dialog**: the description's `start_dialog` (`CH-1` 6, through `CH-2` 5) from coming online until a
  script calls `set_start_dialog` or `restore_default_start_dialog`;
- **default start dialog**: the description's `start_dialog`, for the whole time the character is online; no
  script member changes it;
- **saving**: the online stalker writes its current start dialog into its update, which its simulation object keeps
  and saves (`sav` 7.4.2, `DG-3` 6); coming online reads neither it nor anything else back into the start dialog
  (`DG-3` 6, believed there); the trader's start dialog is not saved (`sav` 7.4.2: his update is empty).

### 6.2 The character information's start dialog

The value copied at coming online is the character information's start dialog (`DG-3` 6). It is written only when
the character information is made (empty) and freed, when it is filled from the description (`DG-3` 6: only while
empty) and when the client data is read (2.4: it is the first text of the inventory-owner block) [B11, B6].
**Believed** (from these writers): it always holds the description's `start_dialog`, so reading it back from a save
changes nothing.

### 6.3 Start dialogs that no dialog file defines

Three specific characters have a `start_dialog` that no listed dialog file defines: `bar_psih`
(`bar_bar_psih_start_pain`), `bar_svoboda_attack_commander` (`bar_svoboda_volkodav_start`) and
`gar_bandit_novice_wounded` (`gar_wounded_bandit`); profiles of the same ids are named by 0, 1 and 1 records of
`spawns\all.spawn` [D3]. Such an id is kept as the character's start dialog and default start dialog without being
looked up (`CH-2` 5; 6.1); the first talk set-up that tries it is fatal (`DG-3` 3.1, `dialog-files` 3.2), unless a
script has set another start dialog by then. This is the part of `scope`'s first review gap that concerns the start
dialog; which specific character a record gets is `CH-2`'s, and the undefined `actor_dialog` ids are the dialogue's.

## 7. What is saved

| Value | Saved? |
|---|---|
| talk-enabled flag | no (2.4); enabled again at coming online |
| talk flag, partner | no (3) |
| stalker's start dialog | in the stalker's update, `sav` 7.4.2; not read back (6.1) |
| trader's start dialog | no (6.1) |
| default start dialog | no; the description's at coming online |
| character information's start dialog | in the inventory-owner block of the client data (`sav` 8; 6.2) |
| talk contacts | `DG-3` 7 |

`scope` 3, B-chars 1, gives the saving of the talk-enabled state to CH-7 with CH-27: this specification settles that
the engine saves none of it; CH-27 owns the layout of the rest of the client data.

## 8. Worked example: the trader

The trader (record `escape_trader`, class id `AI_TRD_S`, client class `CAI_Trader`) comes online at a New Game with
talk enabled (2.1) and with the start dialog and default start dialog `escape_trader_start_dialog` (6.1; `DG-3` 8).
His logic file starts the start chain (`trader-and-crows-scope` 1.1): on entering `new_start_wait`, whose `can_talk`
is false, `mob_trader` disables talk (4.5), and at its updates the monster binder sets an empty hint (4.3); an offer by the use
action is refused (1.1). From `new_start_intro3`, which leaves `can_talk` unset, talk is enabled and the hint is
`character_use`. The player character presses `use` within 2 m: he accepts and starts his side (`DG-3` 2.2), and
`is_talking` on him is true. While he talks nothing in the engine moves or turns him; his head keeps turning toward
the player character as it does at all times (`TC4` 3), and his motions and sounds are his scripts' choices on his
channels (`trader-and-crows-scope` 1.1). When the talk ends (`DG-3` 5.3) his side stops at once or at his next
scheduled update. His start dialog is restored to the default at every section change (4.5), so every talk with him
opens with `escape_trader_start_dialog` (`DG-3` 8). After a save and a load he comes online with talk enabled again
and, believed, his active section sets it as its `can_talk` says (4.6).

## 9. Coverage

`scope` section 8 gives CH-7 no name of its universe (its data are the class ids and the stalker update's start
dialog): the class ids are 1.2, the stalker update's start dialog 6.1. The scope's items for CH-7 (2.5): which
objects can talk (1), the talk-enabled state with its default (2.1) and its changes (2.2), being in a talk on the
character's side (3), the current start dialog (6), the character's part while talking (4: the stalker's under
`xr_meet`, 4.2; the trader's, 4.5, 8). B-chars 1 (saving): 2.4, 7. B-chars 5 (behaviour while talking): 4; the
sounds while talking are the scripts' (4.2: none from the meet scheme) and `scope` CH-25's. Of `scope`'s "Gaps found
in review", the first concerns the start dialog as 6.3 states; the second (CH-26's waits-on) does not concern this
specification.

## 10. Unestablished and believed

- What the talk flag and the partner hold before a character's first talk (3; believed not talking).
- The routine of the player character's own that reads its talk flag with the `cant_walk` hints (3).
- That the scripts run as read (4, 8), in particular the stalker's state while talking under `xr_meet` (4.2) and the
  talk state being set again after a load (4.6).
- That the character information's start dialog always equals the description's (6.2).
- Whether a record of `bar_svoboda_attack_commander` or `gar_bandit_novice_wounded` gets the specific character with
  the undefined start dialog (6.3; `CH-2`).

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binary `bin\xrGame.dll` (6,246,400 bytes).
- Data examined: the winning copies of `config\`, `scripts\` and `spawns\all.spawn` as extracted for the approved
  `pda-scope`: the character and dialog files, `system.ltx` and its includes, the shipped scripts (`xr_meet`,
  `xr_wounded`, `xr_logic`, `xr_motivator`, `bind_monster`, `bind_stalker`, `mob_trader`, `mob_remark`, `dialogs`,
  `dialog_manager`, `trade_manager`, `modules`, `class_registrator` and the others named in 4.1).
- Analysis: static analysis of `bin\xrGame.dll` (the full headless Ghidra decompilation and instruction listing made
  for earlier approved specifications; run-time type information read from the file bytes): the inventory owner's
  construction, spawn, reset, client-data write and read, the talk-enabled routines and query of every creature
  class and the player, every call of them and of the talk query found by searching the whole decompilation and
  listing, every write of the talk values in the inventory owner's code and the classes' constructors, the object
  factory's engine entries for the two class ids, and the start-dialog part and the character information's writers.
  No debugging or execution; the game was not run.
- Checks: `checks/character-talk-state/checks.py` (with the `talk-window`, `game-object-basics`, `characters-scope`
  and `dialogue-scope` helpers), run with `python3 -I`; 27 pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What `xr_use` does when the player character uses a living character whose talk is disabled (it reads
  `is_talk_enabled` on the used character and, when false, may react with sounds depending on the character's
  enemies and relation to the player character): not stated in 4.
- What `xr_meet` does when its `use_wpn` pick is `false` while the player character holds a weapon beyond disabling
  talk (it marks the meet manager and starts the tutorial `part_15_hide_weapon` unless the info
  `tutorial_part_15_hide_weapon` is given): not stated in 4.2.
