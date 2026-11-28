# Dialog flow

Behaviour specification DG-4 of the dialogue scoping specification (`docs/behavior/dialogue-scope.md`, sections
2.4 and 4) for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): how a dialog proceeds once a talk is
open. It covers who says phrase `0` and every phrase after it, how the partner chooses among several phrases it
can say, how the player character's choices are made, listed and ordered (the phrases it can say, or the dialogs
it can start when no dialog is in progress), what a phrase without text does, what follows the end of a dialog
while the talk goes on (and whether a finished dialog is offered again), the two messages of the scope's 1.5,
and a phrase none of whose continuations can be said. It also settles, for the scope's section 3, which of the
continuations of the trader's phrase `0` are evaluated and in which order.

It builds on, without repeating:

- `docs/formats/dialog-files.md` (cited as `files`: dialogs, phrases, phrase `0`, ids, links and their order,
  `priority`, `goodwill`, `caption`, texts, the six lists);
- `docs/behavior/script-api-dialogs-and-trade.md` (cited as R: the arguments, results and failures of the
  precondition and action functions, 3.1 to 3.3; `stop_talk`, 4.1);
- `docs/behavior/info-portions.md` (cited as `portions`: the player character's dialog list, section 6; holding
  and giving portions; the marks of 3.4 step 4 and 4.2);
- `docs/behavior/talk-window.md` (cited as `window`: the answer log and its lines, 6.1; the question list and
  its entries, 7; "talk start");
- `docs/behavior/use-action.md` 6.2 (a talk being accepted and the window shown),
  `docs/behavior/original-determinism.md` 2.2 (the shared random generator), `docs/behavior/console-log.md` 3.8
  and 3.10 (fatal errors);
- `docs/behavior/dialogue-scope.md` (cited as `DG`; *player character*, *partner*).

How a talk starts and ends, which dialogs a character offers and the start dialog are DG-3's; the window's
input (which click reaches the flow, when the window closes) DG-6's; the drawing of the lines and entries
DG-5's (`window`); what the precondition and action functions and the portion lists decide and do DG-2's
together with R; the voices DG-8's; the generated dialogs' phrases DG-9's. This spec states only the order in
which these are reached.

Conventions:

- "The program" is the shipped `bin\xrGame.dll` (6,246,400 bytes [B0]). **Fatal error** is a failed check with a
  message, reported as `console-log` 3.8 describes (the description given here is the report's `Description`
  row; its `Expression` row is a fixed text of the original program and carries nothing needed for
  compatibility), after which the program ends as `console-log` 3.10 describes.
- A dialog's **choices** are the phrases that may be said next in it (section 1). A phrase is **sayable** when
  its preconditions hold as R 3.2 evaluates them. A phrase's **line** is the entry of the answer log that
  `window` 6.1 adds for it; a **question** is an entry of the question list (`window` 7).
- "Draw" is one draw of the shared generator of `original-determinism` 2.2 (an integer 0 … 32767); "*r* mod
  *n*" is the remainder of that draw divided by *n*.
- Every fact carries its check, cited by label in brackets: `D`*n* are assertions of `checks/dialog-flow/c_data.py`
  (the game data), `B`*n* of `checks/dialog-flow/c_bin.py` (static analysis of the program, read from the
  decompilation and disassembly that `checks/dialog-flow/ghidra.py` re-creates). `run_all.py` runs both; all
  pass. **Static** marks a fact read from the program and not observed at run time; **believed** a reading with
  its reason; **unestablished** what could not be settled statically.

## 1. A dialog in a talk

While it is in progress, a dialog of a talk has (static) [B2, B3]:

- a **first** and a **second speaker**: the character that started it and the one it was started with;
- a **turn**: whose phrase is next, the first speaker's or the second's;
- the **choices**: the phrases that may be said next, in order;
- whether it is **finished**.

The talk window holds at most one dialog in progress, its **current dialog** [B7b, B9b, B9f]. Each speaker also
keeps the dialogs it takes part in and drops one when it finishes it by saying a phrase [B2b, B3g]; nothing
else of the flow reads that record. **Disputed in review:** no check establishes that nothing reads this record;
only the sayer drops the dialog, so the other speaker keeps a dialog the sayer finished, and who reads or clears
the record is unestablished.

## 2. Starting a dialog

A dialog is started by one speaker with the other [B2, B2b]: the starter becomes the first speaker, the other
the second; the turn is the first speaker's; the choices become the one phrase `0`; the dialog is not
finished. Phrase `0`'s own preconditions are not tested at any point: it is a choice without a test [B2]. (Only
one phrase `0` of the listed files has conditions of its own, that of `optional_dialog_test` in
`dialogs_test.xml` [D5].) The dialog's own conditions are tested when it is added to a dialog list (R 3.2,
`portions` 6), not when it starts. A dialog can be started in two ways.

### 2.1 By the partner, at talk start

At each talk start (`window`), after the answer log and the question list are emptied [B7, B7b]:

1. The partner builds its own dialog list for the player character. That list is emptied and so is its record
   of the ids tried; the partner's start dialog (when it has one), then `hello_dialog`, are tried as `portions`
   6 step 3 describes (skipped when already tried in this build; otherwise added only when the dialog's own
   conditions hold for the partner and the player character); the list is then ordered by `priority`, highest
   first (section 6.2) [B7c, B7d, B4c]. Which start dialog a character has is DG-3's.
2. If the list is empty, no dialog is started. Otherwise its first dialog becomes the current dialog, the
   partner starts it with the player character, a line is added for phrase `0` with the partner as its
   speaker, and the partner says phrase `0` (section 3) [B7b]. If the dialog is then finished, there is again
   no current dialog [B7b].
3. Then the window is marked for a refill (section 5) [B7].

This opening runs only at talk start [B7]. In this edition every start dialog that a listed file
defines has priority 0, like `hello_dialog`, so a start dialog whose conditions hold always comes first and is
the one started [D3, D2, B4b]. `hello_dialog` has no conditions of its own [D2], so a stalker or trader always
opens a talk with one of the two (whether a character is a stalker or trader is DG-3's).

### 2.2 By the player character, from the question list

When no dialog is in progress the questions stand for the dialogs of the player character's list (section 5).
A click on one (DG-6) with the dialog id *d* (static) [B9, B9b, B9c]:

1. If *d* is empty or no dialog of the player character's current list has the id *d*, the program formats the
   text `ID = [`*d*`] of selected question is out of range of available dialogs ` and does nothing with it: it
   is neither shown nor written to the log [B9b]. It then fetches *d* from the list, which, for such an id, is
   a fatal error whose description is *d* [B9c]. **Believed** never reached from the window's own questions:
   each question stands for a dialog of the list as it was last built, and clicks are ignored while a refill
   is pending (section 5) [B9, B8].
2. Otherwise the dialog becomes the current dialog, the player character starts it with the partner, and the
   player character says phrase `0` (section 3, with its line first as in 5.3) [B9b, B9e]. So, as in 2.1, phrase `0` is said by the starter.

## 3. Saying a phrase

When a speaker says the phrase *p* of the current dialog, in this order (static) [B3, B3b, B3c, B3d, B3e, B3f]:

1. *p* becomes the phrase just said; the sayer is the speaker whose turn it was, and the turn passes to the
   other speaker [B3]. The turn passes on every phrase, with or without text: the speakers alternate strictly,
   so phrase `0` is the starter's, the phrases said after it the other's, and so on.
2. *p*'s effects run: the portion lists and then the action functions, with the sayer as the first argument and
   the other speaker as the second (R 3.3) [B3b].
3. The choices are emptied [B3b]. If *p* has no links, the dialog is finished, and step 4 is skipped [B3b].
4. Otherwise every link of *p* is taken, in link order (`files` 5.2), and its preconditions are tested with the
   other speaker (the one whose turn it now is) as the first argument and the sayer as the second (R 3.2); each
   continuation that is sayable is appended to the choices [B3c]. Every link is tested, also after one has
   been found sayable [B3c]. If none is sayable, that is a fatal error with the description
   `No available phrase to say, dialog[`*id*`]`, *id* being the dialog's id [B3d]. The choices are then ordered
   by goodwill (section 6.1) [B3e].
5. The other speaker **hears** the phrase [B3f]:
   - a stalker or trader answers it at once (section 4), when the dialog is not finished [B1, B1b, B5];
   - the player character marks the talk window for a refill (section 5), when the window is shown [B1, B6].

Because step 2 comes before step 4, the continuations of *p* are tested after *p*'s portions were given and
its actions ran: a portion that *p* gives is already held when its continuations are tested (it is received
at once, `portions` 3.1 step 3).

The line of a phrase is added before the phrase is said, by whoever makes it say the phrase (2.1, 4, 5.3)
[B7b, B5d, B9e]; a phrase whose text is empty adds no line (`files` 7, `window` 6.1) [B8d].

An action that ends the talk (`stop_talk` on both speakers, R 4.1) does not stop these steps: the continuations
are still tested and the partner still answers, as long as the dialog is not finished (static: none of the
steps tests whether the speakers are talking) [B3, B5]. What the end of the talk then does to the window is
DG-3's and DG-6's.

## 4. The partner's answer

When a stalker or trader hears a phrase of a dialog that is not finished, it chooses one of the choices and
says it, in the same step, before control returns to whatever made the other speaker say its phrase (static)
[B1b, B5, B5b, B5c, B5d]. The choices are in goodwill order (section 6.1), *n* of them (at least one: an
unfinished dialog without choices is the fatal error of 3 step 4).

The choice (static) [B5, B5b, B5c]:

- Let *a* be the value the program computes for the pair of the answering character and the other speaker
  with the routine from which the trade price takes its value *v* (R 5.3; **believed** the answering
  character's attitude towards the other speaker, spec Q of `script-api-scope`), and *g* the goodwill of the
  last choice (the lowest).
- If all *n* choices have the same goodwill, or *a* is less than *g*, one of all *n* choices is taken: choice
  number (*r* mod *n*), counting from 0, for one draw *r*.
- Otherwise (the goodwills differ and *a* ≥ *g*), no choice qualifies, and the division by the count 0 is an
  integer division by zero (**believed** a crash).

The text of the choice plays no part. The answering character then adds the choice's line with itself as the
speaker and says it (section 3) [B5d].

In this edition every phrase has the goodwill −10000: the files give none (`files` 5.2: default −10000) and
every phrase the shipped scripts add is given −10000 [D1]. So **the partner takes one of its sayable
continuations uniformly at random** (one draw, *r* mod *n*), whatever its attitude.

## 5. The player character's choices

### 5.1 The refill

The talk window keeps a **refill mark**. It is set at talk start (2.1 step 3), after every question click
(5.3), when the player character hears a phrase while the window is shown (3 step 5), and when the player
character is given or loses an information portion while the window is open (`portions` 3.4 step 4, 4.2)
[B7, B9d, B6]. On each of its updates while the mark is set, the window refills the question list [B8e]:

1. The question list is emptied and scrolled to its beginning [B8].
2. **No dialog in progress:** the player character's dialog list is built for the partner exactly as
   `portions` 6 describes (it is emptied with its record of tried ids at every build, so every refill
   re-evaluates every dialog's own conditions) [B8, B7e]; then one question is added per dialog of the list,
   in list order, standing for the dialog's id, with the dialog's caption (`files` 4: its `caption`, else its
   phrase `0`'s text) [B8]. A dialog whose caption is empty adds no question (`files` 7) [B8d].
3. **A dialog in progress, the player character's turn:** [B8b, B8c]
   - if every choice has an empty text, one of them is taken for the player character, choice number
     (*r* mod *n*) of the *n* choices for one draw *r*, regardless of goodwill, and the player character says it
     (5.3, without a click); then, if no dialog is in progress any more, or the new choices again all have empty
     texts, the refill starts over from step 1 [B8c];
   - otherwise one question is added per choice, in the order of the choices, standing for the phrase's id,
     with the phrase's text; a choice whose text is empty adds no question and so cannot be chosen [B8c, B8d].
4. **A dialog in progress, the partner's turn:** nothing is added [B8b].
5. The mark is cleared [B8c].

While a dialog is in progress, a refill lists the choices as they were made when the last phrase was said
(3 step 4): the continuations are not tested again [B8c]. So a portion given or taken during a dialog (the
mark of `portions` 3.4 step 4 and 4.2) changes the questions only from the next phrase on, and the player
character's dialog list (and with it the dialogs that portions offer, `portions` 6) is built only when no
dialog is in progress. This settles the case `portions` 6 leaves to this specification.

### 5.2 Phrases without text

A phrase without text is said like any other, in its turn (3 step 1), and has its effects; only its line is
missing [B3, B8d]. For the partner the text plays no part (4). For the player character, a choice without text
is said by the program on its own when all the choices lack text (5.1 step 3), and cannot be chosen when some
choices have text [B8c, B8d].

### 5.3 A question click

A click on a question (window message 46; which inputs produce it is DG-6's) is ignored while the refill mark
is set [B9]. Otherwise [B9b, B9d, B9e]:

- with no dialog in progress, the question's dialog is started (2.2);
- with a dialog in progress, the player character says the phrase whose id the question stands for: its line is
  added with the player character as the speaker, then the phrase is said (section 3), which, through the
  partner's answer (4), may say the partner's next phrase too [B9e, B3f];
- if the dialog is then finished, there is no current dialog any more [B9e, B9f];
- the window is marked for a refill [B9d].

## 6. Order

### 6.1 Choices

The choices of a phrase are ordered by goodwill, highest first; choices of equal goodwill keep their link order
(static) [B3e, B4, B4b]. The ordering used sorts ranges of up to 32 entries by a stable insertion and larger
ones otherwise; a phrase of this edition has at most 14 links [D4], so the order is always that rule. With every
goodwill −10000 in this edition [D1], the choices are in link order, the document order of the phrase's `next`
elements (`files` 5.2), and so are the questions of 5.1 step 3.

### 6.2 Dialog lists

The player character's and the partner's dialog lists are ordered by the dialogs' `priority`, highest first,
with the same rule: dialogs of equal priority keep the order in which they were added (static) [B4, B4b, B4c,
B7c, B7e]. Lists of this edition have at most 15 entries (the player character's: at most 14 actor dialogs of a
partner plus the one `dialog` child of `global_dialogs`; the partner's: at most 2) [D4], so for the player
character's list the order of `portions` 6 step 4 is completed: equal priorities keep the order of `portions` 6
step 2 (portions' dialogs first, then the partner's actor dialogs, each in its own order).

## 7. After a dialog

A dialog is finished when a phrase without links is said, by either speaker (3 step 3). Then [B9e, B7b, B8]:

- there is no current dialog; the talk goes on (unless a phrase's action ended it, DG-3);
- the window is already marked for a refill (by the click, 5.3, or by talk start, 2.1 step 3), or the refill
  under way starts over (5.1 step 3), so the questions become the player character's dialog list, built afresh
  (5.1 step 2).

Nothing records that a dialog was finished: the list is rebuilt from the dialogs' own conditions at every
refill, so **a finished dialog is offered again** as long as its own conditions hold (static) [B7d, B7e, B8].
Dialogs that should not repeat rely on their conditions (for example a portion the dialog gives that its
`dont_has_info` names). The partner's opening (2.1) runs only at talk start [B7]: within a talk, the flow does
not start the partner's start dialog or `hello_dialog` again.

## 8. Errors

| Case | What the program does |
|---|---|
| a phrase with links none of which is sayable is said | fatal error, description `No available phrase to say, dialog[`*id*`]` (3 step 4) [B3d] |
| a question click whose dialog id is empty or not in the player character's list | the text `ID = [`*d*`] of selected question is out of range of available dialogs ` is formatted and discarded; then a fatal error whose description is *d* (2.2) [B9b, B9c]; believed unreachable from the window's own questions |
| the partner's choices differ in goodwill and its value *a* is at least the lowest | integer division by zero (4), believed a crash; impossible in this edition, where all goodwills are equal [D1] |

Neither fatal check can be skipped: the program reads its skip flag only at the check itself and sets it nowhere
else [B3d, B9c]. A phrase none of whose continuations is sayable stops the game only when it is said; a phrase
without links ends the dialog instead (3 step 3).

## 9. Worked examples

### 9.1 `hello_dialog`

`hello_dialog` (`dialogs.xml`, priority 0, no conditions of its own) has phrase `0` (no text) → `1` (no text) →
`2` (precondition `dialogs.is_not_wounded`, text `hello_dialog_2`) or `3` (precondition `dialogs.is_wounded`,
text `hello_dialog_3`), neither with links; `is_not_wounded` is the negation of `is_wounded` [D2]. Started by the
partner at talk start (2.1): the partner says `0` (no line); the player character's choices are `[1]`, all
without text, so at the first refill the program says `1` for the player character (no line) (5.1 step 3);
`1`'s continuations are tested with the partner first, exactly one of `2`, `3` is sayable (their results are
read as truth values, R 3.2, and one function is the other's negation), and the partner says
it with its line (4); having no links it finishes the dialog, and the refill starts over with the player
character's dialog list (5.1, 7).

### 9.2 The trader's start dialog

The trader's start dialog is `escape_trader_start_dialog` (priority 0, no conditions of its own) [D6], so,
while it is the trader's start dialog (DG-3), it is started at every talk start with the trader (2.1). Phrase `0` has no text and the links 12, 13, 14, 15, 18, 19, 17, 20,
30, 66, all without text [D6b]. The partner (the trader) says `0` with no line; then all ten continuations are
tested in that order, each with the player character as first argument (3 step 4); within each, the portion
lists first and then the precondition functions in order, stopping at the first false one (R 3.2).

While the player character holds none of `tutorial_end`, `esc_return`, `esc_trader_tutorial_chosen`,
`tutorial_wounded_give_info`, `tutorial_wounded_start`, `esc_trader_return`, `agroprom_military_case` (believed
the case at the first talk of a New Game; which portions are held then is not this specification's):

| Link | Conditions [D6b] | Result | Functions called |
|---|---|---|---|
| 12 | not `esc_return`, not `esc_trader_tutorial_chosen`; `escape_dialog.is_tutorial` | sayable (`is_tutorial` is true while `tutorial_end` is not held [D6d]) | `is_tutorial` |
| 13 | `esc_return`; … | not | none |
| 14 | `tutorial_wounded_give_info`; … | not | none |
| 15 | `esc_trader_tutorial_chosen`; … | not | none |
| 18 | `tutorial_wounded_start`; … | not | none |
| 19 | `esc_return`; … | not | none |
| 17 | not `esc_return`; `escape_dialog.is_not_tutorial`, `trader_dont_give_job` | not (`is_not_tutorial` false [D6d]) | `is_not_tutorial` |
| 20 | `escape_dialog.trader_give_extra_job`, `is_not_tutorial` | not | `trader_give_extra_job`, and `is_not_tutorial` when that returned true |
| 30 | `esc_trader_return`; … | not | none |
| 66 | `agroprom_military_case`; … | not | none |

So the player character's choices are `[12]`, without text: at the first refill the program says 12 for the
player character (no line) (5.1 step 3); 12's one link 3 has no conditions, and the trader says 3 with its line
(4); 3's links 31 and 32 have no conditions and have texts, so the refill lists two questions, 31 then 32
(6.1) [D6c].

- **32:** the player character says 32 (its line; it gives `esc_trader_experienced` and
  `esc_trader_tutorial_chosen`), the trader answers 321, the questions are `[3211]`; then the trader answers
  32111 (giving `esc_trader_can_leave` and `tutorial_wounded_start`), the questions are `[321111]`, and saying
  321111 runs `dialogs.break_dialog` and, having no links, finishes the dialog [D6c]. Every phrase of this
  path has a text [D6c].
- **31:** the player character says 31 (its line; it gives `esc_trader_newbie` and
  `esc_trader_tutorial_chosen`, then runs `dialogs.break_dialog` and `escape_dialog.run_tutorial_pda`); although
  the talk was just ended, 31's link 311 is still tested and the trader still says 311 (no line: its text is
  empty), which runs `dialogs.break_dialog` again and, having no links, finishes the dialog (3) [D6c].

At the talk that the end of the tutorial starts (`DG` 3), while the player character holds
`esc_trader_tutorial_chosen` and none of `tutorial_end`, `esc_trader_can_leave`, `esc_return`,
`tutorial_wounded_give_info`, `tutorial_wounded_start`, `esc_trader_return`, `agroprom_military_case`, 12 is not
sayable and 15 is (and the others are not, as in the table), so the program says 15 for the player character and
the trader continues with 321 and the path of 32 [D6b, D6c, D6d].

## 10. Unestablished

- What the value *a* of the partner's choice is (4; believed its attitude, spec Q); it matters only when
  goodwills differ, which no phrase of this edition does.
- What the division by zero of 4 leads to (believed a crash).
- That the fetch of 2.2 step 1 can never be reached from the window's own questions (believed).
- The order of equal entries in a list of more than 32 entries (6; none in this edition).
- Which portions the player character holds at a New Game's first talk with the trader (9.2; believed none of
  those named there).

## 11. Coverage

`dialogue-scope` section 7 gives this specification `dlg:info_portion/dialog` (disputed there in favour of
DG-3). What a portion's `dialog` child does is `portions` 6; this specification adds when the list it feeds is
built (5.1 step 2: at every refill with no dialog in progress), the order of its equal priorities (6.2) and the
case of a dialog in progress (5.1). The other names this flow reaches are owned elsewhere: `next`, `priority`,
`goodwill` (DG-1, `files`), the six lists (DG-2, R), `start_dialog`, `hello_dialog`, `stop_talk` (DG-3), the
answer log and question list (DG-5, `window`), message 46 (DG-6). Of the scope's review gaps, the fifth (the
`dialog` child) is settled by `portions` 6 together with 5.1 here; the others concern systems outside the
dialog flow.

## Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binary `bin\xrGame.dll` (6,246,400 bytes).
- Data examined: the winning copies of `config\` and `scripts\` as extracted for the approved `pda-scope`: the
  listed dialog files, the specific characters of `[profiles]`, the `[info_portions]` files, the shipped
  scripts.
- Analysis: static analysis (decompilation and instruction listings with Ghidra, headless and read-only) of the
  program's routines that start a dialog, say a phrase, test and order its continuations, let a stalker or
  trader answer, build the characters' dialog lists, and, in the talk window, open a talk, refill the question
  list and handle a question click; the sorting routines they use; the shared generator's import; constants and
  strings read from the binary. No program was executed; the game was not run.
- Checks: `checks/dialog-flow/c_data.py` and `c_bin.py`, run with `python3 -I` (`run_all.py` runs both), with the
  `dialogue-scope`, `pda-scope` and `dialog-files` helpers and the `dialog-files` and
  `script-api-dialogs-and-trade` evidence; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What becomes of a dialog still in progress when the talk ends or the window closes (whether it stops being
  the current dialog, and whether either speaker's record of it is kept), and so whether a later talk can find
  it still in progress.
