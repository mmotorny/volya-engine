# Dialog conditions and effects

Behaviour specification DG-2 of the dialogue scoping specification (`docs/behavior/dialogue-scope.md`, sections
2.2 and 4, with its review gaps) for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): what a dialog's and
a phrase's conditions (`has_info`, `dont_has_info`, `precondition`) decide and when they are tested, what a
phrase's effects (`give_info`, `disable_info`, `action`) do and when they run, the order of both and whether
testing stops at the first false condition, which character is asked whether it holds an information portion,
the arguments the script functions receive and what their results mean, what happens when no continuation can
be said, and the shipped functions: the 65 precondition and 78 action functions the dialog files name, what
engine members they reach, and the 10 the first conversation needs. How a talk starts and ends (DG-3), which
phrase is said and by whom once the sayable ones are known (DG-4), the window (DG-5, DG-6) and the generated
dialogs (DG-9) are those specifications'; this one gives them the conditions and effects.

Builds on (not repeated here):

- `docs/formats/dialog-files.md` (cited as `files`): dialogs, phrases, links (continuations), the six lists
  and their reading, script function names and their lookup, broken references.
- `docs/behavior/script-api-dialogs-and-trade.md` (cited as `R`): the dialog-file functions as entry points,
  their argument counts and results (its 3.1 to 3.3, with its review notes), the talk members (`stop_talk`).
- `docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md` (cited as `P`),
  `docs/behavior/info-portions.md` (cited as `IP`): holding a portion (`IP` 2), the transfer that gives or
  disables one and what receiving it causes (`IP` 3, 4), the player character's dialog list (`IP` 6), the
  simulation's question about an object id (`IP` 2).
- `docs/behavior/script-api-engine-entry-points.md` (cited as `D`): resolving a script function by name (1.1)
  and the call that fails (1.2: fatal).
- `docs/behavior/script-api-game-object-basics.md` (cited as `M`): the handle a script receives for an object.
- `docs/behavior/script-api-scope.md` (cited as `scope`): the split of the script interface into specs A to Z
  (section 6), used in section 7 to name each engine member's owner.
- `docs/behavior/player-state.md` 2.1 (the player character's object id 0), `docs/behavior/script-loading.md`
  (the 250 loadable scripts and their comment forms), `docs/behavior/script-environment.md` (the runtime),
  `docs/behavior/console-log.md` 3.8 and 3.10 (a failed check and the end of the program),
  `docs/behavior/talk-window.md` (the question list and the answer log).

Conventions:

- **Player character** is the character the player controls; **partner** the character it talks to
  (`dialogue-scope`). A **character** is either. A dialog has a **first speaker** and a **second speaker**
  (section 4.1). The **speaker** of a phrase is the character who says it; the **listener** is the other one.
- The **conditions** of a dialog or phrase are its `has_info`, `dont_has_info` and `precondition` lists; its
  **effects** are its `give_info`, `disable_info` and `action` lists (`files` 6). **Holds** is `IP` 2's
  question; a **transfer** is `IP` 3.1 (*give*) and 4.1 (*disable*).
- A phrase's **continuations** are its links in their order (`files` 5.2). A phrase is **sayable** when it is
  a continuation of the phrase just said and its conditions hold (section 3). The **sayable list** is what the
  dialog keeps of them (section 4.2).
- **Fatal** means a failed check reported as `console-log` 3.8 describes, after which the program ends as its
  3.10 describes.
- "The program" is the shipped `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [B1].
  Each fact is **static** (read from the program by static analysis; nothing was run), **measured** (from the
  game data and the shipped scripts), **believed** (inferred; the reason is given) or **unestablished**.
- Every fact carries its check in brackets: `D`*n* are assertions of `checks/dialog-conditions/data.py` (the
  game data and shipped scripts), `B`*n* of `checks/dialog-conditions/bin.py` (static analysis), in the dirty
  workspace; `run_all.py` beside them runs both, and all pass. Facts cited from another spec carry that
  spec's name instead.

## 1. Where conditions and effects are used

The six lists are read for dialogs and phrases alike (`files` 6), but the program uses them in three places
only (static):

| Use | Lists | When | Section |
|---|---|---|---|
| a dialog's own conditions | `has_info`, `dont_has_info`, `precondition` of the dialog | each time the dialog is tried for a character's dialog list | 2 |
| a phrase's conditions | the same lists of the phrase | when the phrase is a continuation of the phrase just said, at once after that phrase's effects | 3, 4 |
| a phrase's effects | `give_info`, `disable_info`, `action` of the phrase | when the phrase is said | 4 |

Not used:

- **A dialog's own effects** (its `action`, `give_info`, `disable_info`) are read and never run: the only
  use the program makes of a dialog's own lists is the condition test of section 2; a search of the whole
  program's decompilation finds no other use of them [B8, B8c]. This settles `files` 11's open question. No
  dialog of this edition has such a list [D3].
- **Phrase 0's conditions** are never tested: when a dialog starts, phrase 0 is made the only sayable phrase
  without any test (section 4.1) [B7]. Phrase 0's effects run when it is said, as any phrase's. In this
  edition one phrase 0 has a condition: `optional_dialog_test` (in `dialogs_test.xml`), precondition
  `dialogs.dialog_allowed`, which therefore is never called through it; no specific character names that
  dialog. No phrase 0 has `has_info` or `dont_has_info`; 14 have `give_info` and 9 `action`, none
  `disable_info` [D3b].
- An information portion's `precondition`, `has_info` and `dont_has_info` children are not dialog conditions
  and are never tested (`IP` 3.3).

## 2. A dialog's own conditions

A dialog is tried for a character's dialog list (which characters' lists are built, and when, is DG-3's; the
player character's list is `IP` 6). Trying it: if the dialog was already tried during this build of the list
it is skipped; otherwise it is recorded as tried, loaded (`files` 3.2), and its own conditions are tested as
section 3 states, with [B8, B8b]:

- the character whose list is built as the first character, its partner as the second;
- the dialog id as the third argument and the empty text as both the fourth and the fifth (`R` 3.2);

and it is added to the list only when the test is true [B8b]. So for the player character's list the
dialog's precondition functions receive (player character, partner, dialog id, `""`, `""`); for the partner's
list, which the talk's start builds for the player character (DG-3), they receive (partner, player character,
dialog id, `""`, `""`) [B9, B9b]. The `has_info` and `dont_has_info` lists ask the player character in both
cases (section 3).

The test is made each time a list is built, so its result follows the portions held at that moment: the
player character's list is built again on the talk window's update after a portion was given to or disabled
on the player character, while no dialog is in progress (`IP` 3.4 step 4, 4.2 and 6) [B11]. In this edition 51
dialog-level preconditions name 21 distinct functions [D1, D1b].

## 3. Testing conditions

A test has two characters (the first and the second, given by the caller: section 2 for dialogs, section 4.2
for phrases) and texts. It goes as follows (static) [B3, B4]:

1. **`has_info`**, in list order: for each id, if the player character does not hold it (`IP` 2), the test
   is false and nothing further is tested.
2. **`dont_has_info`**, in list order: for each id, if the player character holds it, the test is false and
   nothing further is tested.
3. **`precondition`**, in list order: each function is called (looked up by name, `files` 6, and called in
   the runtime's protected mode, `R` 3.1) with five arguments: the first character's handle, the second
   character's handle (`M`), and three texts (`R` 3.2: the dialog id, the id of the phrase just said, the id
   of the phrase being tested). The result is the runtime's truth value of what the function returns: `false`
   and nil (also a function that returns nothing) are false, every other value (`true`, a number, a text, a
   table, an object) is true [B3]. At the first false result the test is false and no later function is
   called.
4. Otherwise the test is true.

- **Who is asked about portions:** always the player character, whoever the two characters are; the two
  characters are not used by steps 1 and 2 [B4]. An undeclared id is never held, without error (`IP` 2,
  `files` 8).
- **Stopping:** the test stops at the first false step, so later lists and functions are not reached: a
  precondition function is called only when every `has_info` id is held and no `dont_has_info` id is, and
  only when every earlier precondition function of the same list returned true [B3, B4]. Since only the
  functions can have effects, the stop matters only for them: a function placed after a false one is not
  called.
- **The functions' own questions:** the shipped precondition functions ask about portions through the shipped
  script helper `has_alife_info(id)` of `_g.script`, which answers false when there is no simulation and
  otherwise asks the simulation whether object 0 holds `id` [D6b] (`IP` 2: the same list the dialog lists
  ask); object 0 is the player character (`player-state` 2.1). So the lists and the shipped functions ask the
  same holder.
- **Parameters:** a function declaring fewer parameters than it receives ignores the rest (`R` 3.2). Of the
  62 defined precondition functions, 51 declare 2 parameters, 7 none and 4 three; the parameter names are
  only names (for example `escape_dialog.trader_alredy_give_job(trader, actor)` receives the phrase's speaker
  first, whoever that is) [D2, D6d].
- **Errors:** an error raised by a function is fatal (`R` 3.1 as disputed in its review, `D` 1.2).

## 4. Saying a phrase

### 4.1 Starting a dialog: the speakers and the turn

When a character starts a dialog with another, it becomes the dialog's **first speaker** and the other the
**second speaker** [B7, B7b]. The dialog then has no phrase said yet, its sayable list is phrase 0 alone
(untested, section 1) and the **turn** is the first speaker's [B7]. Each phrase said hands the turn to the
other character (section 4.2 step 1) [B6, B6b], so phrases are said alternately, phrase 0 by the first
speaker, its continuation by the second, and so on; this holds for every phrase, including those said
without the player's choice (DG-4) [B6, B9b, B9c, B10].

Who starts (static; which dialog, and when, is DG-3's and DG-4's) [B9b, B9c]:

- at the start of a talk, when the partner's list (built for the player character) is not empty, the partner
  starts its first dialog with the player character and says phrase 0 at once: the partner is the first
  speaker;
- when the player chooses a dialog in the question list while no dialog is in progress, the player character
  starts it with the partner and says phrase 0: the player character is the first speaker.

### 4.2 The step

Saying a phrase *p* of a dialog is one step, in this order (static) [B6, B6b, B6c, B6d]:

1. *p* becomes the phrase just said, and the turn passes to the other character. The speaker *S* is the
   character whose turn it was; the listener *L* is the other.
2. **Effects of *p*** (static) [B5]:
   1. each `give_info` id, in list order, is given to the **player character** by a transfer *give*
      (`IP` 3.1: an undeclared id is fatal there; a portion new to the player character is received with all
      of `IP` 3.2 to 3.4, including its articles, tasks and the script callback `inventory_info`, before the
      next id);
   2. then each `disable_info` id, in list order, is disabled on the player character by a transfer
      *disable* (`IP` 4.1);
   3. then each `action` function, in list order, is called with four arguments: *S*'s handle, *L*'s handle,
      the dialog id and *p*'s id (texts); its result is ignored (`R` 3.3). An action that ends the talk or
      changes lists does not stop the later actions (`R` 3.3).

   The effects go to the player character whichever character says *p*.
3. The sayable list is emptied. If *p* has no continuation, the dialog is **finished** [B6].
4. Otherwise **every** continuation of *p* is tested, in link order (`files` 5.2), each as section 3 with *L*
   as the first character and *S* as the second, and the texts dialog id, *p*'s id and the continuation's id
   (`R` 3.2, 3.3) [B6]. No test stops the others: each continuation whose test is true is appended to the
   sayable list [B6]. The tests see every effect of step 2 (a portion *p* gave is held, one it disabled is
   not).
5. If no continuation passed, the program reports a failed check with the description `No available phrase to
   say, dialog[`*id*`]` (*id* the dialog id): fatal [B6].
6. The sayable list is ordered by the phrases' goodwill (`files` 5.2), greater first; phrases of equal
   goodwill keep their link order (static for up to 32 sayable phrases, which the program orders by a method
   that keeps equal elements in place; a phrase of the listed files has at most 10 links, `files` 5.2) [B6c].
   No phrase of the listed files has a `goodwill` element (`files` 5.2), so all have the default and their
   sayable lists are in link order; the goodwill of generated phrases is DG-9's. Which of them is
   said, and by which choice, is DG-4's.
7. *L* is told that *p* was said [B6b]:
   - a partner, when the dialog is not finished, at once chooses one of the sayable phrases and says it: a new
     step 1 to 7, inside this one [B10]. How it chooses is DG-4's.
   - the player character only marks the talk window to rebuild its question list on its next update [B10b].

Nothing in the step tests whether the two characters are still talking: an action that stops the talk
(`stop_talk`, `R` 4.1) does not prevent steps 3 to 7, nor the partner's answer [B6d, B10] (static; what the
window and the talk do afterwards is DG-3's and DG-6's).

### 4.3 When the tests run, and when they do not

- A continuation's conditions are tested once, in step 4 of the phrase it continues; the result is kept in
  the sayable list until the next phrase is said [B6].
- Rebuilding the question list while a dialog is in progress (after the player character was told, step 7,
  or after a portion was given or disabled, `IP` 3.4 step 4 and 4.2) lists the kept sayable phrases and tests
  nothing [B11]. So a portion given or disabled while the player chooses (by a script or by the engine) does
  not change which phrases are offered until the next phrase is said. A portion an action gives or disables
  (for example through `give_info_portion`, `P` 2.1) is seen by the tests of the same step, as they come
  after the actions (section 4.2 steps 2 and 4).
- When the player character says a phrase, the partner's answer, its effects and the tests of the answer's
  continuations all happen in the same step, before the window updates (step 7) [B10]. So the effects of two
  phrases (the player's and the answer) run within one choice of the player.

## 5. What goes wrong

| Case | What happens | In this edition |
|---|---|---|
| a condition or effect names an information portion no file declares | `has_info`, `dont_has_info`: never held, no error; `give_info`, `disable_info`: fatal when the effect runs (`files` 8, `IP` 8) | `files` 8 |
| a precondition or action names a function no shipped script defines | the call is made with no function (`files` 8: believed an access fault; unestablished) | 3 preconditions, 7 actions; which of them are reachable: `files` 8 [D2] |
| a function raises an error | fatal (`R` 3.1, `D` 1.2) | one shipped function can: `bar_dialogs.barman_has_job` calls the global `barman_need_kill_veterans`, which no script defines, whenever `has_alife_info("bar_darklab_document_done")` is true, which raises the runtime's error for calling nil; it is named only by phrases 44, 61 and 71 of `bar_bar_barmen_jobs`, none of which can be reached from phrase 0, so it is never called [D5] |
| a precondition returns something other than a boolean | its truth value (section 3 step 3) | every defined precondition function has a `return` statement [D8]; what each returns is the script's |
| a phrase with continuations of which none is sayable | fatal (section 4.2 step 5) | which talks reach it depends on the portions held: DG-4 |
| phrase 0 has conditions | never tested (section 1) | one: `optional_dialog_test` [D3b] |
| a dialog has its own effects | never run (section 1) | none [D3] |

## 6. The arguments, by case

| Function | 1st | 2nd | 3rd | 4th | 5th |
|---|---|---|---|---|---|
| a dialog's precondition, player character's list | player character | partner | dialog id | `""` | `""` |
| a dialog's precondition, partner's list | partner | player character | dialog id | `""` | `""` |
| a continuation's precondition | the continuation's speaker (*L*) | the speaker of the phrase just said (*S*) | dialog id | id of the phrase just said | the continuation's id |
| a phrase's action | its speaker (*S*) | the listener (*L*) | dialog id | the phrase's id | — |

Sources: sections 2 and 4.2 [B6, B6b, B8, B8b, B9]; `R` 3.2, 3.3.

## 7. The shipped functions

### 7.1 Counts (measured)

The listed dialog files name 65 distinct precondition functions and 78 distinct action functions, no name in
both lists [D1]. Preconditions: 51 dialog-level and 83 phrase-level occurrences; 21 names are used by dialogs,
52 by phrases, 8 by both. Actions: 304 occurrences, all on phrases [D1, D1b]. Defined by the shipped scripts:
62 preconditions (parameters: 2 for 51, none for 7, 3 for 4) and 71 actions (2 for 70, 3 for 1); not defined:
the preconditions `bar_dialogs.actor_need_reward`, `have_bar_tiran_pda`, `have_lucky_pda` and the actions
`bar_dialogs.give_bar_tiran_pda`, `give_lucky_pda`, `have_borov_reward`, `have_svoboda_leader_reward`,
`have_tiran_reward`, `have_volkodav_reward`, `escape_dialog.give_wounded_flash_reward` [D2]. The five with 3
parameters are `dialogs.allow_break_dialog`, `allow_wounded_dialog`, `break_dialog`, `dialog_allowed` and
`is_opp_wounded`; the third is the dialog id [D2b].

### 7.2 Engine members they reach (measured)

The 133 defined functions run as shipped scripts; implementing them needs the engine members they reach. The
table lists every engine name reached from them, directly or through the shipped script functions they call
(a call `script.function` or a bare name of the same script or of `_g.script` is followed, and a method call
whose name is not an engine member is followed into every script class defining that name, so the closure is
an upper bound; Lua's own library is left out), with the number of the 133 that reach it, grouped by the
owner the scoping split gives (`scope` 6; a name several classes declare has several owners) [D4]:

| Owner | Names (functions reaching) | Owner approved |
|---|---|---|
| B | `log` 11 | yes |
| E | `getH` 1; with H: `sub` 1; with F, H, N: `add` 5 | yes |
| F | `r_string` 33, `system_ini` 32, `ini_file` 2, `line_count` 2, `line_exist` 2, `r_line` 2, `section_exist` 2; with I: `r_float` 2, `r_bool` 1, `r_u32` 1; with M: `id` 43; with J, M, N: `name` 8 | yes |
| G3 | `GetTextureInfo` 54, `game.translate_string` 54, `get_file_name` 54, `get_rect` 54 | no |
| G4 | `level.enable_input` 2, `level.show_indicators` 2 | no |
| H | `game.get_game_time` 26, `timeToString` 26 | no |
| J | `community` 5, `g_group` 5, `g_squad` 5, `g_team` 5, `section_name` 5; with M: `profile_name` 6, `clsid` 5, `alive` 4; with Q: `rank` 5 | no |
| K | `alife` 68, `create` 32, `story_object` 7, `create_ammo` 1, `release` 1; with M, N: `game_vertex_id` 31, `level_vertex_id` 31; with P: `has_info` 30; with S, V, X: `object` 54; with M, N, V, X: `position` 54 | no |
| M | `set_actor_direction` 1, `set_actor_position` 1, `story_id` 1 | yes |
| N | `level.name` 2, `patrol` 1, `point` 1 | no |
| P | `give_game_news` 54, `give_talk_message` 54, `give_info_portion` 8, `level.map_add_object_spot` 4, `disable_info_portion` 2, `game.start_tutorial` 2, `level.map_add_object_spot_ser` 1, `level.map_has_object_spot` 1, `level.map_remove_object_spot` 1 | yes |
| Q | `set_relation` 4, `character_community` 3, `set_character_community` 2, `change_character_reputation` 1, `character_rank` 1, `relation` 1, `relation_registry.set_community_goodwill` 1 | no |
| R | `is_talking` 54, `stop_talk` 4 | yes |
| S | `money` 35, `transfer_item` 33, `transfer_money` 27, `give_money` 26 | no |
| T, U | `play` 1 | no |
| X | `restore_weapon` 2 | no |
| none | `barman_need_kill_veterans` 1 (defined nowhere, section 5) | — |

`has_info` here is the simulation's question (`K`, `IP` 2) reached through `has_alife_info`; the 54 functions
reaching `give_talk_message`, `give_game_news` and the window texture names do so through the shipped item and
money transfer helpers of `dialogs.script` (`relocate_item_section`, `relocate_money`), which report the
transfer in the talk (DG-7) [D4, D4b]. What each member
does is its owner's.

### 7.3 The ten of the first path (measured)

The functions the trader's start dialog names on its first path (`dialogue-scope` 3) [D6, D6b, D6c, D6d]:

| Function | Kind | What it does |
|---|---|---|
| `escape_dialog.is_tutorial()` | precondition | true when `has_alife_info("tutorial_end")` is false |
| `escape_dialog.is_not_tutorial()` | precondition | `has_alife_info("tutorial_end")` |
| `escape_dialog.trader_alredy_give_job(trader, actor)` | precondition | true when one of these pairs has the first held and the second not: (`agroprom_military_case`, `agroprom_military_case_done`), (`escape_blockpost_case`, `escape_blockpost_case_done`), (`tutorial_artefact_start`, `tutorial_artefact_done`), (`tutorial_wounded_start`, `esc_serious_talk`); else false |
| `escape_dialog.trader_dont_give_job(first_speaker, second_speaker)` | precondition | the negation of `trader_alredy_give_job` |
| `escape_dialog.trader_give_extra_job(trader, actor)` | precondition | `escape_blockpost_case` held and `escape_blockpost_case_done` not |
| `escape_dialog.trader_need_extra_job(trader, actor)` | precondition | `escape_blockpost_case` not held and `esc_return` held |
| `bar_dialogs.have_darklab_documents(first_speaker, second_speaker)` | precondition | true when the first argument's `object("dar_document4")` is not nil (an item of that section in its inventory: `S`, unestablished here) |
| `bar_dialogs.hasnt_darklab_documents(first_speaker, second_speaker)` | precondition | the opposite: that `object` is nil |
| `escape_dialog.run_tutorial_pda(first_speaker, second_speaker)` | action | logs a fixed text (`printf` of `_g.script`, which formats and logs [D4b]), calls `stop_talk` on the first and then the second argument, then `game.start_tutorial("part_1_pda")` (`P`) |
| `dialogs.break_dialog(first_speaker, second_speaker, id)` | action | calls `stop_talk` on the first and then the second argument (`R` 4.1) |

"Held" is `has_alife_info`, the player character's (section 3). The six tests that ask only about portions
use no argument; the two document tests ask the phrase's speaker (the first argument, section 6) [D6, D6c].

## 8. Worked example: the trader's start dialog

`escape_trader_start_dialog` (`dialogs_escape.xml`; `files` 10, `dialogue-scope` 3) has no dialog-level lists,
so it passes its own test whoever builds the list. Phrase 0 has no lists and ten continuations, in link order
[D7]:

| Continuation | `has_info` | `dont_has_info` | `precondition` |
|---|---|---|---|
| 12 | — | `esc_return`, `esc_trader_tutorial_chosen` | `escape_dialog.is_tutorial` |
| 13 | `esc_return` | — | `escape_dialog.trader_dont_give_job`, `bar_dialogs.hasnt_darklab_documents` |
| 14 | `tutorial_wounded_give_info` | `esc_return`, `esc_trader_direct`, `agroprom_military_case` | `escape_dialog.trader_alredy_give_job` |
| 15 | `esc_trader_tutorial_chosen` | `esc_trader_can_leave`, `esc_return` | `escape_dialog.is_tutorial` |
| 18 | `tutorial_wounded_start` | `esc_return`, `tutorial_wounded_give_info` | — |
| 19 | `esc_return` | `bar_darklab_document_done`, `esc_darklab_documents_read` | `bar_dialogs.have_darklab_documents`, `escape_dialog.is_not_tutorial` |
| 17 | — | `esc_return` | `escape_dialog.is_not_tutorial`, `escape_dialog.trader_dont_give_job` |
| 20 | — | — | `escape_dialog.trader_give_extra_job`, `escape_dialog.is_not_tutorial` |
| 30 | `esc_trader_return` | — | `escape_dialog.trader_need_extra_job`, `escape_dialog.is_not_tutorial` |
| 66 | `agroprom_military_case` | `esc_return` | — |

When the talk starts with this dialog (DG-3), the trader is the first speaker and says phrase 0 (section 4.1);
phrase 0 has no effects. Its ten continuations are the player character's phrases: each is tested with (player
character, trader, `"escape_trader_start_dialog"`, `"0"`, the continuation's id), all ten, in the order above
(section 4.2 step 4). If the player character holds none of the portions this table names, the tests call
exactly three functions, `is_tutorial` (for 12, true), `is_not_tutorial` (for 17, false, so 17's second
function is not called) and `trader_give_extra_job` (for 20, false), and only 12 is sayable [D7c]; the other
continuations stop at their `has_info` lists before any function. `trader_alredy_give_job`,
`trader_dont_give_job`, `trader_need_extra_job` and the two document tests are not called in that case.

Then, along the path 0 → 12 → 3 → 32 → 321 → 3211 → 32111 → 321111 [D7b]: the speakers alternate, so 12, 32,
3211 and 321111 are the player character's and 3, 321 and 32111 the trader's (section 4.1); none of these
phrases after 12 has a condition, so each test is true and each phrase's only continuation is sayable.

- **32** (the player character's choice): its effects give `esc_trader_experienced` and then
  `esc_trader_tutorial_chosen` to the player character (two transfers, each calling `inventory_info` for a
  portion new to it, `IP` 3.4), then
  321 is tested (true); the trader is told and says 321 at once (section 4.2 step 7).
- **32111** (the trader's): gives `esc_trader_can_leave` and then `tutorial_wounded_start` to the player
  character (the latter's task and article, `IP` 9), although the trader says it.
- **321111** (the player character's): its action `dialogs.break_dialog` receives (player character, trader,
  `"escape_trader_start_dialog"`, `"321111"`) and stops the talk on both (its third parameter, `id`, is the
  dialog id; the fourth argument is ignored); 321111 has no continuation, so the dialog is finished
  (section 4.2 step 3).

On the other answer, **31** (the player character's): it gives `esc_trader_newbie` and then
`esc_trader_tutorial_chosen`, then calls `dialogs.break_dialog` and then `escape_dialog.run_tutorial_pda`,
each with (player character, trader, dialog id, `"31"`): the talk is stopped twice and the tutorial
`part_1_pda` starts. The step does not test the talk state, so 311 is still tested (no condition: sayable)
and the trader, told of 31 with the dialog not finished, says 311 at once, whose `dialogs.break_dialog`
receives (trader, player character, dialog id, `"311"`); 311 has no continuation [D7b, B6d, B10]. What the
window does meanwhile is DG-3's and DG-6's. On a later talk, once `esc_trader_tutorial_chosen` is held and the
tutorial not ended, and neither `esc_trader_can_leave` nor `esc_return` is held, 12 fails at its
`dont_has_info` and 15 passes, leading to 321 and the same end (section 3) [D7].

## 9. Unestablished

- What a call of a function no script defines does beyond the call itself (`files` 8: believed an access
  fault); none of the undefined functions is reached on the first path.
- What the program does after the failed check of section 4.2 step 5 if the report is set to be ignored
  (`console-log` 3.8 gives what such a check does).
- Whether the program runs a dialog's own effects by a route the decompilation search of section 1 cannot
  see, such as a call through a pointer to those lists kept elsewhere (believed none: no such pointer was
  found, and the dialog's own routines use only the test).
- The order of more than 32 sayable phrases of equal goodwill (section 4.2 step 6); no phrase of this edition
  has more than 10 continuations.
- What the engine members of section 7.2 do (their owners'), and what stopping the talk inside a step does to
  the window (DG-3, DG-6).

## Provenance

- Date: 2026-11-27. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binary `bin\xrGame.dll` identified above.
- Data examined: the winning copies of `config\` and `scripts\` as extracted for the approved `pda-scope`: the
  dialog, character and information-portion files, and the shipped scripts that define or are reached by the
  dialog files' functions; the approved script-interface split.
- Analysis: static analysis (decompilation and instruction listing with Ghidra, headless and read-only) of the
  game library's dialog code: the condition-and-effect part's tests and effects, saying a phrase, starting a
  dialog, the characters' dialog lists and answers, the talk window's start, choice and rebuild; a search of the
  whole decompilation for uses of a dialog's own lists; a tokenizing scan of the shipped Lua scripts (function
  bodies, parameters and calls, followed through the scripts). No program was executed; the game was not run.
- Checks: `checks/dialog-conditions/data.py` and `bin.py`, run with `python3 -I` (`run_all.py` runs both), with
  `luafn.py` and the `dialogue-scope` and `pda-scope` helpers; all pass.
- No leaked source code, derived projects, third-party documentation of the engine or recollection were used.
