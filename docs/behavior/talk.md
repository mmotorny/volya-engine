# Talk start and end

Behaviour specification DG-3 of `docs/behavior/dialogue-scope.md` (sections 2.3 and 4, with its "Gaps found in
review"), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): how a talk between the player character and
another character starts (the use action, `run_talk_dialog`, `switch_to_talk`), what a character must be for it,
what each side of a talk is, which dialogs a talk offers (the partner's opening dialog from its start dialog and
`hello_dialog`, and the player character's dialogs from its information portions and the partner's actor
dialogs), the turn of the player's view toward the partner while the talk lasts, how a talk ends (on either side,
by scripts, by distance, by death, by the window closing) and what `is_talking` reports on both sides meanwhile,
a character's start dialog with its default and its saving, and the talk contacts of the save.

Builds on (not repeated; cited by name and section):

- `docs/formats/dialog-files.md` (`files`): dialogs, phrases, links, `priority`, captions, loading a dialog by id
  and its failures (3.2), texts (7).
- `docs/behavior/script-api-dialogs-and-trade.md` (`R`): the talk members of `game_object` as scripts call them
  (4.1 to 4.4); the dialog-level conditions and the phrase functions as entry points (3.2, 3.3).
- `docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md` (`P`) and `docs/behavior/info-portions.md`
  (`IP`): holding portions; `IP` 6, the player character's dialog list, which this specification uses as it is.
- `docs/behavior/talk-window.md` (`TW`): the window's elements, its set-up at talk start (4, 5, 6, 7, 9), the log
  entries (6.1) and the special mode (9).
- `docs/behavior/use-action.md` (`use`): the use action and its talk with the living (1.2, 6.2).
- `docs/formats/character-descriptions.md` (`CH-1`, its 6: the start and actor dialog ids) and
  `docs/behavior/character-identity.md` (`CH-2`, its 5: the online character).
- `docs/formats/sav.md` (`sav`: 7.4.2 the stalker's update, 9 part 3 the talk contacts).
- `docs/behavior/inventory-screen.md` 1.2 item 4 (in-game windows closed on the player character's death),
  `docs/behavior/console-key-bindings.md` 5 (presses taken by an open window), `docs/behavior/torch.md` 7.4 (the
  angle smoothing law), `docs/behavior/player-movement.md` 8 (the camera's yaw and pitch),
  `docs/behavior/console-log.md` 3.8 (a failed check), `docs/behavior/script-loading.md` (scripts load when named).
- `docs/behavior/trader-and-crows-scope.md` 1.1 and 3.2: the trader's talk-enabled state comes from his logic
  file through `mob_trader` (`can_talk`; off in his three waiting sections at a New Game). This answers the scoping
  review's gap on DG-3's waits-on: nothing of the trader's logic is engine behaviour of this specification.

Waits on (not yet specified): `characters-scope` CH-7 (the talk-enabled state's changes and saving, a character's
behaviour while talking) and CH-5 (alive, wounded); DG-4 (the flow of a dialog once started: who says which
phrase, the non-player speaker's choice, the two error messages); DG-6 (the window's inputs and its other ways of
closing; whether the level goes on while it is open).

Conventions:

- **Player character**, **partner**: as `dialogue-scope`. A **character** is a stalker, a trader or the player
  character (`R` conventions); monsters never talk (`R` 4.1).
- Each character has a **talk flag** (talking or not), a **partner** (the character it talks with, or none) and a
  **talk-enabled flag**: `R` 4.1's three values. One character's flag and partner are its **side** of a talk.
- **The talk window** is the window of `TW`, shown and hidden as one screen with the trade window (`TW` 2).
- A **dialog in progress** is the dialog the window currently follows (at most one). The **choices** are the
  entries of the question list (`TW` 7).
- **Scheduled update**: the periodic update each character gets while it is online (how often is not established
  here).
- **Fatal**: a failed check reported as `console-log` 3.8, after which the program ends.
- Statements are **static** (read from the shipped `bin\xrGame.dll`; the game was not run) or **measured** (from
  the game data) unless marked **believed** (with the reason) or **unestablished**.
- **Checks**: every claim carries a label in brackets, an assertion of `checks/talk/checks.py` in the dirty
  workspace (`python3 -I checks.py`; `B` from the static analysis, `D` from the data). All 39 pass.

## 1. The talk state of a character

- A character's object starts with the talk-enabled flag set (talk enabled) and the trade-enabled flag set [B4];
  `enable_talk`, `disable_talk` and `is_talk_enabled` act on it as `R` 4.1 states. Its later changes and whether
  it is saved are CH-7's. The only reader of the flag is the offer of 2.2 (`R` 4.1): a character whose talk is
  disabled while it talks goes on talking [B2, B9].
- **Starting a side** (the partner on accepting, the player character after it): the talk flag is set, the partner
  is kept, and the speaker's trade is readied with the other speaker (what that does is the trade's, #284) [B1].
- **Stopping a side** (`stop_talk`, and the rules of 5.1): the flag is cleared and the partner forgotten (`R` 4.1).
  Stopping one side does not stop the other side and does not close the window [B9].

## 2. Starting a talk

### 2.1 Routes

| Route | What reaches the offer of 2.2 |
|---|---|
| the use action on a living character within 2 m | `use` 6.2: only while the player character is not talking |
| `actor:run_talk_dialog(partner)` | `R` 4.2: on the player character only, without testing whether it is talking; on any other object nothing |
| `obj:switch_to_talk()` | nothing, on any object (`R` 4.3) |

In the shipped scripts `run_talk_dialog` is called on the player character by `xr_meet` (when its condition list
picks `self` and the character is not talking) and by `xr_effects.esc_init_dialod` (with the object of story id 3, the
trader; the function the tutorial `part_1_pda` names on its stop, `dialogue-scope` 3) [D5, D6]. `xr_sos` and
`xr_zoneguard` also call it, but no other script and no configuration file names either of them, so they never
load (`script-loading`) and never run; `xr_sos` calls it on a non-player character, which would do nothing
[D7]. **Disputed in review:** that `xr_sos` calls it on a non-player character is unbacked: D7 checks only
that neither script is named elsewhere, not which object `xr_sos` calls it on.

### 2.2 The offer

The partner is offered the talk. It accepts when its talk is enabled and both it and the player character have
health above 0 (`use` 6.2 step 2, `R` 4.2); whether either is already talking is not tested [B2]. On acceptance
the partner starts its side with the player character, then the player character starts its side with the
partner [B1], the topmost open in-game window, if any, is toggled off, and the talk window is toggled on (`use`
6.2 step 3, `R` 4.2). A refused offer does nothing.

**Believed** (from these steps): `run_talk_dialog` while the talk window is open toggles the talk window itself
off (5.2: hiding it stops the player character's side), so the following toggle shows it with the player
character not talking, its set-up (3) does nothing and the window closes at its first update (5.2). The new
partner's side then stops at its scheduled update (5.1). The shipped calls test `is_talking` first or follow the
end of a talk (`xr_meet`, `dialogue-scope` 3) [D5, D6].

## 3. The talk's set-up

When the talk window is shown it sets up the talk, then shows itself [B5]. The set-up does nothing unless the
player character is talking; the partner is the player character's partner [B6]. In order [B6]:

1. the panels and frame-line titles are filled (`TW` 4, 5) and both lists emptied (`TW` 6, 7);
2. the opening dialog is started (3.2);
3. the choices are marked for refresh and the window's update runs at once, which refreshes them (3.3, 5.2);
4. the special mode's elements are hidden or shown (`TW` 9), the talk part is shown and the trade window hidden
   (`TW` 2).

### 3.1 The partner's opening list

Made at each set-up, for the partner, with the player character as the other speaker [B10, B11]:

1. the list and its record of the dialog ids tried are emptied;
2. the partner's start dialog (6), when it has one, is tried; then `hello_dialog` is tried;
3. the list is sorted (3.4).

**Trying** a dialog id [B12]: an id already tried in this build is skipped; otherwise it is recorded as tried,
the dialog is loaded by id (`files` 3.2: an id no listed file defines is fatal) and added to the list only if its
own conditions hold (`R` 3.2, the dialog-level form, with the partner as the list's holder and the player
character as its partner). So when the start dialog is `hello_dialog` (as `xr_sos` would set) the list has it
once; when the start dialog's conditions fail, `hello_dialog` alone remains.

`hello_dialog` (`dialogs.xml`) has priority 0 and no dialog-level conditions; its phrase 0 has no text and one
continuation [D1]. `hello_dialog` is named in `xrGame.dll` [B28].

### 3.2 The opening dialog

If the opening list is not empty, its first dialog becomes the dialog in progress [B10]:

1. the partner starts it with the player character: the dialog is set up with the partner as first speaker and
   the player character as second, and added to both speakers' dialogs in progress [B10, B18];
2. phrase 0's text is added to the answer log under the partner's name (`TW` 6.1; an empty text adds nothing,
   `files` 7) [B10];
3. the partner says phrase 0 (its effects `R` 3.3; what follows is DG-4's flow) [B10];
4. if the dialog is then over (3.6), it is dropped: there is no dialog in progress [B10].

If the opening list is empty the talk starts with no dialog in progress, and the choices are the player
character's dialogs (3.3). The other entries of the opening list are never used: only its first dialog opens a
talk [B10].

### 3.3 The player character's dialogs

Whenever the choices are refreshed while no dialog is in progress, the player character's dialog list is built
anew for the partner as `IP` 6 describes (the `dialog` children of the portions it holds, then the partner's actor
dialogs, each tried as in 3.1 with the player character as holder, then sorted as 3.4), and one choice per dialog
is listed, its text the dialog's caption (`files` 4; `TW` 7) [B14, B15]. Reading the partner's actor dialogs
first checks that the partner has a specific character; one without is fatal [B14] (every online stalker and
trader has one, `CH-2` 5).

The choices are refreshed at the window's update after they were marked [B15]. They are marked: at the set-up
(3, step 3); after the player character chooses (3.5); when the partner says a phrase while the window is shown
[B29]; and when the player character is given or loses an information portion while the window is open (`IP`
3.4, 4.2).

The only `dialog` child of the shipped portions is `global_dialogs` → `actor_break_dialog` (priority −5,
precondition `dialogs.allow_break_dialog`, false only when the partner's profile is `osoznanie`; its phrase 0's
action `dialogs.break_dialog` calls `stop_talk` on both speakers) [D3, D4]; the player binder gives
`global_dialogs` (`IP` 7.2) [D8]. So, with the lowest priority of the shipped dialogs, it is the last choice in
every talk except with `osoznanie`, and choosing it ends the talk (5.3).

### 3.4 Order

Both lists are sorted by the dialogs' `priority`, a higher one first [B13]. A list of up to 32 dialogs keeps
dialogs of equal priority in the order they were tried (the sort is then an insertion sort); for longer lists the
order of equal priorities is **unestablished** [B13]. No shipped character offers more than 15 dialogs (14 actor
dialogs, `CH-1` 6, and `actor_break_dialog`), so for the shipped data the order of `IP` 6 step 4 is settled: equal
priorities keep the order of trying.

### 3.5 Choosing a dialog

When the player character chooses one of its dialogs (how a choice is made is DG-6's), the player character
starts it with the partner (the player character first, the partner second), it becomes the dialog in progress,
and the player character says its phrase 0, whose text is added to the log under the player character's name
[B16, B18]. What follows is DG-4's.

### 3.6 The end of a dialog

A dialog is over when the phrase just said has no continuation [B17]. After each phrase the other speaker
receives it at once: a partner answers (DG-4) unless the dialog is over [B29]; so the end of a dialog, by either
speaker's phrase, is known when the phrase the player character chose (or phrase 0 of 3.2) has been said, and the
dialog is then dropped [B10, B16]. The talk does not end with a dialog: the next refresh, with no dialog in
progress, offers the player character's dialogs again (3.3) [B15]. A phrase with no continuation therefore ends
only its dialog; the talk ends only as section 5 describes.

## 4. While the talk lasts: the player's view

On each update of the talk window that has a partner, the player character's active camera (its yaw and pitch,
`player-movement` 8) turns toward the partner [B19, B20, B21]:

- the **target point** is the centre of the partner object raised by half its radius (**believed**: the centre
  and radius of its bounding sphere, the two queries the routine makes); *d* = (*dx*, *dy*, *dz*) = target point −
  the camera's position;
- the **target yaw** is the direction of (*dx*, *dz*), equal to atan2(*dx*, *dz*) up to a whole turn:
  atan(*dx*/*dz*) for *dz* > 0, atan(*dx*/*dz*) − π for *dz* < 0, and,
  when |*dz*| < 10⁻⁷, π/2 for *dx* > 0 and −π/2 otherwise; 0 when |*dx*| and |*dz*| are both below 10⁻⁷;
- the **target pitch** is −atan(*dy* / √(*dx*² + *dz*²)) (so a partner above the camera makes it look up); when
  the horizontal distance is below 10⁻⁷ it is −π/2 for *dy* > 0 and π/2 otherwise, and 0 when also |*dy*| < 10⁻⁷;
- the yaw and the pitch each, separately: the difference between the current angle and the target, both brought
  into [−π, π] and the difference brought into [−π, π] as `torch` 7.4 step 2 does, is compared with 0.2 rad; only
  if its absolute value exceeds 0.2 does the angle move, by the smoothing law of `torch` 7.4 step 2 with v_min =
  0.15, v_max = 0.2 (rad/s), R = π/6 and the frame time, toward the target, and the result becomes the camera's
  angle.

So the first update of a talk brings the view to within π/6 of the partner (the law's last step), and the view
then creeps on at 0.15 to 0.2 rad/s until it is within 0.2 rad of the target, where it stops. The window's update is **believed** to run once per
frame while it is shown (the in-game windows' update).

## 5. How a talk ends

### 5.1 The end of a side

A side ends (1) when:

- a script calls `stop_talk` on that character (`R` 4.1);
- at the character's scheduled update, while it is talking: if its partner is no longer talking, its side stops;
  and if its own health is 0 or less, its side stops [B3, B3b]. The same rule holds for the player character, the
  stalkers and the trader;
- for the player character only: when the talk window is hidden while it is still talking (5.2) [B8].

### 5.2 The window

On each update the talk window closes itself (the same toggle that opened it, 2.2) [B7, B7b] when:

1. the player character is not talking; or
2. the player character's or the partner's object is missing; or
3. the player character's and the partner's positions (**believed**: the positions of their objects) are more
   than 3 m apart and the partner does not have the special mode (`TW` 9; only `[m_osoznanie]` gives it) [B7,
   B27].

The window also closes on the inputs DG-6 gives it and on the player character's death (`inventory-screen` 1.2
item 4). When it is hidden, by any route: the trade window is hidden, the dialog in progress is dropped, and if
the player character is still talking its side stops [B8]. Hiding does not stop the partner's side; the partner's
side stops at its next scheduled update (5.1) [B9, B3].

### 5.3 What each cause leads to

| Cause | Sequence |
|---|---|
| `dialogs.break_dialog` (e.g. `actor_break_dialog`, the trader's phrases 31 and 321111) | both sides stop at once [D4]; the window closes at its next update (5.2 rule 1) |
| `stop_talk` on the partner only (e.g. `xr_meet` beyond 3 m [D5]) | the player character's side stops at its next scheduled update; then the window closes |
| `stop_talk` on the player character only | the window closes at its next update; the partner's side stops at its next scheduled update |
| more than 3 m apart (no special mode) | the window closes; hiding stops the player character's side; the partner's side stops at its scheduled update |
| the partner's death | the partner's side stops at its scheduled update; then the player character's; then the window |
| the player character's death | its side stops at its scheduled update; the window closes (also `inventory-screen` 1.2 item 4); then the partner's side |
| `disable_talk` on either | nothing ends (1); `xr_meet` stops a talk in progress when it disables talk [D5] |
| a phrase with no continuation | the dialog ends, not the talk (3.6) |

`xr_meet`'s own 3 m test (squared distance 9) matches rule 3 of 5.2 [D5].

### 5.4 What `is_talking` reports

`obj:is_talking()` is the character's talk flag (`R` 4.1). For the player character it is true from the
acceptance (2.2) until its side stops (5.1); for the partner from its acceptance until its side stops. The two
differ for up to one scheduled update of the side that stops second (5.3). A partner that accepts an offer while
it already talks with another character (2.2: not tested) takes the player character as its partner.

## 6. The start dialog

Only stalkers and traders have a start dialog and a default start dialog (`R` 4.4).

- **Coming online**: a new online character has both empty; in the single-player game, when it comes online and
  its start dialog is empty (always, for a new one), its start dialog and its default start dialog both become its
  character information's start dialog [B22]. That value is the description's `start_dialog` (`CH-1` 6), taken
  when the character's specific character is set online (`CH-2` 5), the character information's own being empty
  then [B23]. This settles the believed statement of `CH-1` 6 that the start dialog collected for a talk is the
  one read from the description: it is, until a script sets another.
- **Scripts**: `set_start_dialog`, `get_start_dialog`, `restore_default_start_dialog` act as `R` 4.4; the next
  set-up's opening list (3.1) uses the start dialog current at that moment. In the shipped scripts `xr_meet` sets
  its section's `meet_dialog` and restores the default, and `xr_wounded` sets `help_start_dialog` (`dialogue-scope`
  1.3 counts the logic files) [D8].
- **Saving**: the online stalker writes its current start dialog into its update to the simulation object, which
  keeps it and saves it as the `stringZ` start dialog of `sav` 7.4.2 [B25]. A trader's start dialog is not saved
  (its update is empty, `sav` 7.4.2). The stalker's coming online reads neither its saved start dialog nor its own
  dialog part [B24], so after coming online again (also after loading a save) a stalker's start dialog is its
  description's until a script sets another (**believed**: no other path that reads the saved value was found;
  the static reading covers the two routines that make the online character).

## 7. What is saved

- The stalker's start dialog: section 6.
- **Talk contacts** (`sav` 9 part 3): the game library has an access wrapper for each of the other seven
  per-object registries of chunk 9 and none for the talk contacts, and the contacts' registry is referenced only
  where the registries are constructed [B26]; no step of a talk adds to it [B26]. **Believed**: nothing fills it
  in this edition, so part 3 is written as it was read (empty for a New Game).
- The talk itself (both sides, the dialog in progress, the lists) is **unestablished**: whether a save can be made
  while a talk is open, and what loading one restores, was not established (the scoping review's gap); the talk
  state has no field of its own in `sav`.

## 8. Worked example: the trader's first talk

The trader (`escape_trader`, story id 3) comes online with the start dialog and default start dialog
`escape_trader_start_dialog`, from his description (6; `CH-1` 6). During his three waiting sections his talk is
disabled by his logic (`trader-and-crows-scope` 1.1), so an offer is refused; from `new_start_intro3` it is
enabled. The player character, alive, not talking, within 2 m and aiming at him, presses `use`: he accepts (2.2),
both sides start, and the talk window is shown (3):

1. the opening list: `escape_trader_start_dialog` (priority 0, no dialog-level conditions) is tried and added, then
   `hello_dialog` (priority 0, no conditions); sorting keeps them in that order, so the start dialog opens the
   talk [D1, D2, B13];
2. the trader starts it (first speaker) with the player character; phrase 0 has no text, so no log line; the
   trader says phrase 0 (DG-4 decides which of its ten continuations follow; `dialogue-scope` 3);
3. the choices are refreshed: with the dialog in progress they are the dialog's phrases (DG-4).

Along answer 32 the dialog reaches phrase 32111, which gives `esc_trader_can_leave`, and then 321111, whose action
`dialogs.break_dialog` stops both sides (5.3): the window closes at its next update. Along answer 31, phrase 31's
actions stop both sides and start the tutorial `part_1_pda`, whose stop runs `xr_effects.esc_init_dialod` (2.1):
a new talk with the trader, set up again from 3 (`dialogue-scope` 3) [D4, D6]. Had the player character walked
more than 3 m away first, the window would have closed (5.2 rule 3) and the trader's side stopped at his next
scheduled update.

## 9. Coverage

The names `dialogue-scope` section 7 gives DG-3: `run_talk_dialog`, `api:run_talk_dialog`, `switch_to_talk`,
`api:switch_to_talk` (2.1, 2.2); `stop_talk`, `api:stop_talk`, `is_talking`, `api:is_talking` (1, 5); `enable_talk`,
`disable_talk`, `is_talk_enabled` and their `api:` names (1, 5.3); `start_dialog`, `dlg:start_dialog`,
`set_start_dialog`, `restore_default_start_dialog` and their `api:` names, `api:get_start_dialog` (3.1, 6);
`actor_dialog`, `dlg:actor_dialog` (3.3); `hello_dialog` (3.1); `file:character_dialogs.xml` (the fragment that
gives 348 characters the start dialog `dm_hello_dialog` and two actor dialogs, `CH-1` 7: they reach a talk as 3.1
and 3.3). The disputed `dlg:info_portion/dialog` (scope 7, DG-4's row): its use in a talk is 3.3, through `IP` 6.

## 10. Unestablished and believed

- That `run_talk_dialog` during an open talk ends it (2.2, believed from the steps).
- The order of equal priorities in a list of more than 32 dialogs (3.4).
- That the camera's target is the partner's bounding-sphere centre and radius, and that the window updates once
  per frame (4, believed).
- That the positions compared in 5.2 are the objects' positions (believed).
- How often a scheduled update runs (5.1, 5.3).
- That a stalker's saved start dialog is never read back (6, believed).
- That the talk contacts are never filled (7, believed); saving while a talk is open and what loading restores
  (7).
- What a partner's side does while talking (turning, motions, sounds): CH-7, `trader-and-crows-scope`.
- Whether the level, game time and the player character go on while the window is open (the scoping review's gap;
  DG-6): the turn of section 4 uses the frame time, so the window's update runs then, but nothing more is
  established here.

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binary `bin\xrGame.dll` (6,246,400 bytes).
- Data examined: the winning copies of `config\` and `scripts\` as extracted for the approved `pda-scope`:
  `dialogs.xml`, `dialogs_escape.xml`, the shipped scripts (`dialogs`, `xr_meet`, `xr_effects`, `xr_wounded`,
  `xr_sos`, `xr_zoneguard`, `bind_stalker`, `modules`) and every configuration file.
- Analysis: static analysis of `bin\xrGame.dll` (the full headless Ghidra decompilation and instruction listing
  made for earlier approved specifications; run-time type information read from the file bytes): the talk offer
  and both sides' start and stop, the characters' inventory-owner update and the scheduled updates that run it,
  the talk window's show, set-up, update and hide, the opening list and the trying of dialogs, the player
  character's list, the sort and its comparison, starting and choosing dialogs, saying a phrase and receiving it,
  the camera turn, the start dialog at coming online, the stalker's update export and its simulation object's
  update, and the save registries. No debugging or execution; the game was not run.
- Checks: `checks/talk/checks.py` (with the `talk-window` and `game-object-basics` helpers), run with `python3 -I`;
  39 pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Whether a dead character still gets its scheduled update is not established; the health rule of 5.1 and the
  partner's-death and player's-death rows of 5.3 depend on it.
- The shipped player binder (`bind_stalker`) hides the player character's weapon while it is talking and
  restores it afterwards; this specification does not describe it and names no owner for it.
