# Script API R: dialogs and trade

What the dialog and trade part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition), as scripts see it: the phrase classes `CPhraseDialog`, `CPhrase` and `CPhraseScript` with
which scripts build dialogs; the script functions the dialog files name (`precondition`, `action`,
`init_func`) and the members `AddPrecondition`, `AddAction` that add more, as entry points (their arguments,
results, order and failures); the members of `game_object` that start, end and query talks and set a
character's start dialog; the members that enable trade and switch a talk to trade; the trader's animation and
sound members; the three global trade-condition functions; and the class `game_difficulty`. This is spec R of
the approved scoping specification `docs/behavior/script-api-scope.md` (section 6, order 24); its scope rule,
its section 4.5 row R and its review gaps apply: this spec owns every name of these classes as the game
library registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type,
  namespaces, the 250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched,
  calls of missing members, what a mismatch does); spec F, `docs/behavior/script-api-configuration-and-files.md`
  (the class `ini_file`); spec M, `docs/behavior/script-api-game-object-basics.md` (the handle, the **kinds** of
  object of its 2.2, what a member does on an object of the wrong kind, 2.3, the diagnostics routine that returns
  at once, and its section 7, the `game_object` forms of `sell_condition`, `buy_condition`, `show_condition`);
  spec O, `docs/behavior/script-api-object-event-callbacks.md` (the trade and trader callback ids).
- `docs/formats/dialog-files.md` (cited as `files`: dialogs, phrases, ids, the six lists, links, script function
  names and their lookup, `init_func`), `docs/behavior/dialogue-scope.md` (cited as `DG`: the parts DG-2 to DG-10,
  *player character*, *partner*), `docs/behavior/talk-window.md`, `docs/behavior/use-action.md` (cited as `use`;
  its 6.2, talk with the living), `docs/behavior/character-identity.md`.
- Spec D, `docs/behavior/script-api-engine-entry-points.md` (1.1, finding a script function by name; 1.2, the call
  and its failure; 1.3, where the engine finds script functions by name).
- Spec P, `docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md` (its 2.1, giving, disabling and
  asking; its 10, the difficulty functions) and `docs/behavior/info-portions.md` (cited as `IP`: holding, 2; the
  transfer *give*, 3.1; receiving and its script part, 3.2 and 3.3; the transfer *disable*, 4.1).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [R-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from
  the game library's registration code.
- A call is written as scripts write it: `dlg:AddPhrase("text", "1", "0", -10000)`. A **number** is the
  runtime's double; a **text** a Lua string; **nil** Lua's nil.
- A **dialog**, a **phrase**, a phrase's **links** (its continuations), a **dialog id** and a **phrase id** are
  those of `files` 3 to 5; the **six lists** of a dialog or phrase (preconditions, actions, has, dont-has, give,
  disable) those of `files` 6. A **script function name** is `files` 6's *script*`.`*function*, looked up as
  `files` 6 and 8 state.
- A **character** is an object of the inventory-owner kind (M 2.2: the 19 creature classes and the player); the
  **player** is the player's object. A **stalker** or **trader** is an object of class `CAI_Stalker` or
  `CAI_Trader`. A **monster** is a creature of the other 17 creature classes.
- **Fatal** means a failed check reported as `console-log` 3.8 describes, after which the program ends.
- Each fact is **static** (from static analysis of the game library, not observed at run time), **measured**
  (from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was
  run.

**Checks.** Every claim carries a tag [R-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-dialogs-and-trade/run_all.py` (`python3 -I run_all.py`): it reads the game library's bytes and
run-time type information, the evidence of read-only headless static analysis (produced by `gh.py` beside it,
and the evidence of the checks of specs D, P and `IP` that it cites), the listing, the scoping spec's scan and the
shipped scripts and data. All checks pass (`ALL PASS 29`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 36 listed items [R-2]:

| Owner | Items |
|---|---|
| `CPhraseDialog` | the class; `AddPhrase` (section 2.2) |
| `CPhrase` | the class; `GetPhraseScript` (2.3) |
| `CPhraseScript` | the class; `AddPrecondition`, `AddAction`, `AddHasInfo`, `AddDontHasInfo`, `AddGiveInfo`, `AddDisableInfo` (2.4) |
| `game_difficulty` | the class; the constants `novice`, `stalker`, `veteran`, `master` (section 7) |
| `game_object` | 17 functions: `run_talk_dialog`, `switch_to_talk`, `stop_talk`, `is_talking`, `enable_talk`, `disable_talk`, `is_talk_enabled` (section 4); `set_start_dialog`, `get_start_dialog`, `restore_default_start_dialog` (4.4); `enable_trade`, `disable_trade`, `is_trade_enabled`, `switch_to_trade` (section 5); `set_trader_global_anim`, `set_trader_head_anim`, `set_trader_sound` (section 6) |
| global functions | `sell_condition`, `buy_condition`, `show_condition` (section 5.2) |

and the entry points of the scoping spec's 4.5 row R: the dialog-file functions `precondition`, `action`,
`init_func` and the functions added by `AddPrecondition` and `AddAction` (section 3). The scoping spec assigns
this spec two candidate strings, `false` and `total`; neither is referenced by the registration of any class of
this spec, so neither is a registration of it [R-3]. The listing lacks no name of this spec that the scripts use
(the scoping spec's 2.2 lists none for R) [R-2].

### 1.2 As registered (static)

`CPhraseScript`, `CPhraseDialog` and `CPhrase` are registered with no base class and **no constructor**: scripts
cannot create them, they receive them from the engine (2.1) [R-4]. Each has exactly the members of 1.1 [R-4].
`game_difficulty` is registered as a holder of four integer constants, without constructor [R-12]. The three
global functions are registered at the top level of the scripts' namespace, in five forms (5.2) [R-13]. The
`game_object` members are paired with their implementations in the registration of `game_object` [R-18].

Against the listing: `AddPhrase` is listed as `AddPhrase(string, number, number, number)`; it is registered as
taking three texts and a number (2.2) [R-6].

## 2. The phrase classes (static)

### 2.1 Where scripts get them

Scripts receive these objects through a dialog's `init_func` (`files` 4; **believed** the only way, as no other
member of this spec returns one and the shipped scripts use them nowhere else [R-2]): the program calls it once, when
the dialog is first loaded (`files` 3.2), with **the dialog itself** as its one argument, a `CPhraseDialog`
[R-8]. From it, `AddPhrase` returns `CPhrase` objects and `GetPhraseScript` a `CPhraseScript` [R-6].

A dialog's phrases are kept with its id for the rest of the run and shared by every use of that id (`files`
3.2), so what an init function adds is added once per run, to that shared data [R-7, R-8] (static; that an
init function runs only at the first load is `files` 3.2's). In this edition the 31 dialogs with `init_func` name
29 functions defined by the shipped scripts, each declaring one parameter, and 2 that no script defines
(`dialog_manager.init_trade_info_dialog`, `init_action_info_dialog`; `files` 8) [R-24].

### 2.2 `dlg:AddPhrase(text, id, previous_id, goodwill)`

Arguments: three texts and a number (a call whose first three arguments are not texts, or whose fourth is not
a number, matches no form; what that does is A's) [R-6]. `goodwill` is made an integer (**believed** by
truncation toward zero: the game library's conversion routine for such arguments) [R-6].

1. If the dialog has no phrase with phrase id `id` (an exact text comparison), a new phrase is made with that
   id, the text `text` (a phrase text as `files` 7 uses it: a string id or a literal; empty for none), the
   goodwill `goodwill` (`files` 5.2's value, default there −10000) and six empty lists, and added to the dialog
   [R-7]. The call returns it, as a `CPhrase`.
2. If the dialog already has a phrase with that id, nothing of that phrase changes, and the call returns
   **nil** [R-7].
3. In both cases, if `previous_id` is not the empty text, a link from the phrase with id `previous_id` to the
   phrase with id `id` is added, after that phrase's earlier links (`files` 5.2 gives links their order) [R-7].
   If the dialog has no phrase `previous_id`, the program reaches through no phrase (**believed**: an access
   fault) [R-7].

Phrase `0` is the dialog's first phrase (`files` 5.1); the shipped init functions add it first, with an empty
`previous_id` (for example `dlg:AddPhrase("dm_general_help_medkit","0","",-10000)` in `dialog_manager.script`)
[R-24]. A phrase added by `AddPhrase` is the same as one read from a file: the reading of `files` 5.2 makes
its phrases and links with the same routine, and then reads the six lists into the phrase's part (2.4) [R-7].

### 2.3 `phr:GetPhraseScript()`

Returns the phrase's own condition-and-effect part, a `CPhraseScript` that belongs to the phrase (not a copy):
members called on it change that phrase [R-6].

### 2.4 The `CPhraseScript` members

Each takes one text (exactly a Lua string) and appends it, unchecked, to the end of one list of the part [R-5,
R-6]:

| Member | List (`files` 6) | What the entry is |
|---|---|---|
| `AddPrecondition(name)` | preconditions | a script function name (3.2) |
| `AddAction(name)` | actions | a script function name (3.3) |
| `AddHasInfo(id)` | has | an information-portion id the player character must hold |
| `AddDontHasInfo(id)` | dont-has | an id the player character must not hold |
| `AddGiveInfo(id)` | give | an id given to the player character when the actions run |
| `AddDisableInfo(id)` | disable | an id disabled for the player character when the actions run |

Nothing is checked when an entry is added: a name of a function that does not exist, or an id no information
portion has, is found only when the list is used, as `files` 8 describes for the files [R-5]. The same entry may
be added twice and is then used twice (static: the lists are appended to without search) [R-5]. The has and
dont-has entries ask whether the player character holds the id (`IP` 2, the question of P 2.1's `has_info`); a
give entry is a transfer *give* of the id to the player character (`IP` 3.1) and a disable entry a transfer
*disable* on it (`IP` 4.1), the same transfers as P 2.1's `give_info_portion` and `disable_info_portion`, as
`files` 6 states for the lists read from the files [R-5, R-27]. These lists act on the player character in every
case, whoever the speakers are (3.2) [R-5].

## 3. The dialog-file functions as entry points (static)

### 3.1 Calls and failures

The dialog functions are among the entry points of D 1.3 (the names of the dialog files and the names scripts
pass to `AddPrecondition` and `AddAction`) [R-26]. The program finds a script function named in a list by D 1.1's
resolution (the lookup of `files` 6 and 8) and calls it with the arguments below in a protected call, as D 1.2
states for every entry point [R-9, R-26]. If the function raises an error, the binding library's error callback
is called, which is the fatal `LUA error: %s` of A 5.3 (D 1.2): an error in a dialog function is fatal; the
program would end even if that callback returned [R-9, R-26]. A function that is not found is `files` 8's case.

### 3.2 Preconditions

A phrase's preconditions are evaluated for one speaker pair as follows [R-9]:

1. The has and dont-has lists first: if the player character does not hold every id of the has list, or holds
   any id of the dont-has list, the result is false and no precondition function is called [R-5, R-9].
2. Then each precondition function in list order, with **five arguments**: the handle of the character who
   would say the phrase, the handle of the other speaker, the dialog id, the id of the phrase just said and the
   id of the phrase being tested (texts) [R-9, R-10].
3. The result is read as the runtime's truth value: false for `false` and nil (also for a function that returns
   nothing), true for any other value [R-9] (static; the conversion is the runtime's documented truth test). At the first false result the evaluation stops and the phrase's
   result is false; when every function returned true, it is true [R-9].

When the program tests the continuations of a phrase just said (`DG` DG-4), each continuation's preconditions
get as first argument the speaker of the continuation and as second the speaker of the phrase just said [R-10].
Which of the two characters says which phrase is DG-4's (static here: the program keeps a turn flag per dialog
and orders the two speakers by it [R-10]).

The shipped precondition functions (65 distinct names in the listed dialog files) declare 2 parameters (51), 0
(7), 3 (4), or are not defined (3) [R-24]; a function declaring fewer parameters ignores the rest (the runtime's
rule). For example `task_manager.script` defines `CRandomTask:active_task(actor, npc, p1, p2, p3)`, which uses
`p3`, the tested phrase's id [R-24].

A dialog's own conditions (the dialog-level lists of `files` 4) are evaluated the same way when the program adds
the dialog to a character's list of available dialogs: has and dont-has first, then each precondition function
with five arguments, the handle of the character whose list is being made, the handle of its partner, the
dialog id and two empty texts; the dialog is added only when the result is true [R-10b]. Which character's list
is made when, is DG-3's and DG-4's.

### 3.3 Actions

When a phrase is said, its effects run as follows [R-9, R-10]:

1. First the give list, in order: each id is given to the player character; then the disable list, in order:
   each id is disabled for it, each by a transfer (`IP` 3.1, 4.1; 2.4) [R-5, R-9, R-27].
2. Then each action function in list order, with **four arguments**: the handle of the character who said the
   phrase, the handle of the other speaker, the dialog id and the id of the phrase said (texts); results are
   ignored [R-9, R-10].

An action that ends the talk or changes the lists of a phrase does not stop the remaining actions of the list
(static: the count of the list is read again before each call, and nothing else is tested) [R-9]. The shipped
action functions (78 distinct names) declare 2 parameters (70), 3 (1: `dialogs.break_dialog`), or are not
defined (7) [R-24].

### 3.4 The one-object forms

The same condition-and-effect part has a second precondition form, called with **one argument** (the handle of
one object; has and dont-has first, then each function, the same result rule), and a second action form,
called with **two arguments** (one object's handle and a text; the actions first, then give and disable on the
player character) [R-11]. Dialogs use the two-speaker forms of 3.2 and 3.3 [R-10].

- The **one-object action form** is used when a holder receives an information portion: the portion's own
  condition-and-effect part runs it with the receiving holder's handle and nil, so its `action` functions get
  (holder, nil), and then its give and disable lists act on the player character (`IP` 3.3) [R-27].
- The **one-object precondition form**: no user is known. Receiving a portion tests none of its conditions
  (`IP` 3.3), the call on receiving is to the action form only [R-27], and P adds no use of either form;
  **believed** unused in this edition.

### 3.5 `init_func`

Called once when the dialog is first loaded, with the dialog (`CPhraseDialog`) as its one argument; its result
is ignored (2.1) [R-8]. An error in it ends the program as 3.1 [R-8, R-9].

## 4. Talk members of `game_object` (static)

### 4.1 Talk state

These members apply to characters; on any other object they do nothing and the queries return `false` (M 2.3)
[R-19]:

| Member | Effect |
|---|---|
| `obj:is_talking()` | `true` while the character is talking: its talk flag, set when it starts a talk (`use` 6.2) and cleared by `stop_talk` [R-19] |
| `obj:stop_talk()` | ends the character's own side of a talk: clears its talk flag, forgets its partner and clears the partner its trade keeps [R-19]. It does not end the partner's side; ending a whole talk and closing the window are DG-3's and DG-6's (believed: scripts call it on both speakers, `DG` 2.3) |
| `obj:enable_talk()`, `obj:disable_talk()` | set and clear the character's talk-enabled flag [R-19] |
| `obj:is_talk_enabled()` | the talk-enabled flag; for a monster always `false`, whatever `enable_talk` set [R-19] |

A character whose talk is not enabled refuses a talk offered to it (`use` 6.2 step 2) [R-20]; so a monster never
accepts one. The initial value of the flag and its saving are DG-3's and `characters-scope`'s.

### 4.2 `actor:run_talk_dialog(partner)`

Applies only when `actor` is the player; on any other object it does nothing (the diagnostic text goes to the
routine that returns at once, M 2.3) [R-20]. On the player it offers the talk to `partner` and opens the talk
window exactly as `use` 6.2 steps 2 and 3 describe: the partner accepts when its talk is enabled and both it and
the player have health above 0, and starts talking with the player; then the player starts talking with the
partner, an open in-game window is toggled off and the talk window is shown [R-20]. Unlike the use action it does
not first test whether the player is already talking (`use` 6.2 step 1) [R-20]. `partner` must be a character
(for another object the program calls through no character, **believed** an access fault) [R-20]. If the partner
refuses, nothing happens and nothing is reported [R-20].

### 4.3 `obj:switch_to_talk()`

Does nothing, on any object [R-22].

### 4.4 The start dialog

Apply to stalkers and traders only; on any other object (the player, monsters) they do nothing and
`get_start_dialog` returns nothing [R-21]:

| Member | Effect |
|---|---|
| `obj:set_start_dialog(id)` | the character's start dialog becomes the dialog id `id` [R-21] |
| `obj:get_start_dialog()` | its start dialog id [R-21]; **believed** a text, nil when the character has none (the value is the program's shared-text kind, which converts to nil when empty) |
| `obj:restore_default_start_dialog()` | the start dialog becomes the character's default start dialog [R-21] |

The id is not checked when it is set: a dialog id no file defines is fatal when the dialog is loaded (`files`
3.2) [R-21]. The default start dialog is a second value of the character, set by the engine (**believed** from
the description's `start_dialog`, CH-1; when, is DG-3's) [R-21]. What the start dialog does in a talk is DG-3's;
its saving is `sav` 7.4.2's.

## 5. Trade members (static)

### 5.1 Trade enabled and the switch to trade

| Member | Effect |
|---|---|
| `obj:enable_trade()`, `obj:disable_trade()` | set and clear the character's trade-enabled flag; on other objects nothing [R-22] |
| `obj:is_trade_enabled()` | the flag; `false` on other objects [R-22] |
| `actor:switch_to_trade()` | applies only to the player, and only while the talk window is shown: when both speakers of the talk have trade enabled, the talk window switches to trade (its trade button's action, DG-10); otherwise, and on any other object, nothing [R-22] |

What the trade does (the trade window, its callbacks of spec O, the price) is #284's; the parts of the price that
the trade-condition members set are 5.3.

### 5.2 The trade-condition functions

Each character has its own **trade parameters**; in addition there are **default trade parameters**, one set
for the whole game. The `game_object` forms of M 7 act on the character's own; the global functions act on the
defaults [R-13]:

| Global function | Effect on the defaults |
|---|---|
| `sell_condition(ini, section)`, `buy_condition(ini, section)` | read section `section` of the `ini_file` `ini` as the defaults' sell (buy) part (5.3) [R-13, R-15] |
| `sell_condition(f1, f2)`, `buy_condition(f1, f2)` | set the defaults' two sell (buy) factors to `f1` (first) and `f2` (second) [R-13] |
| `show_condition(ini, section)` | reads section `section` of `ini` as the defaults' show list (5.3) [R-13, R-15] |

The defaults are made on their first use, from the section `[trade]` of the system configuration: the first and
second buy factors from `buy_price_factor_hostile` and `buy_price_factor_friendly`, the sell factors from
`sell_price_factor_hostile` and `sell_price_factor_friendly`, with empty lists and tables [R-14]. In this edition
all four are 1 [R-14]. The defaults are destroyed when the level is destroyed and made afresh at their next use,
so a change by these functions lasts only until then [R-14]. A character's own parameters are made afresh, the
same way from a section the character names, each time it spawns, so a change by M 7's members lasts until its
next spawn [R-17] (which section is #284's). No shipped script calls the global forms; `trade_manager.script`
calls the `game_object` forms [R-2, R-24].

### 5.3 What a section gives, and how the trade uses it

Trade parameters have a **buy part** and a **sell part**, each with two default factors (first, second), an item
list and a factor table, and a **show list** [R-14, R-15].

- **Reading a section** into a part (`sell_condition(ini, section)`, `buy_condition(ini, section)`, either
  form): a missing section is a failed check with the message `cannot find section %s` (**believed** fatal,
  `console-log` 3.8) [R-15]. Otherwise the part's item list and factor table are emptied
  and then each line of the section is taken in order: a line without a value adds its key (an item section) to
  the item list; a line `key = a, b` gives the item section `key` the factors `a` (first) and `b` (second), read as
  decimal numbers, replacing earlier ones for that key [R-15]. The part's two default factors are not changed
  [R-15].
- **Reading a section** as the show list (`show_condition`, either form): the show list is emptied, then each
  line without a value adds its key; lines with a value are skipped [R-15].
- **An item in a part's item list** (the trading character's or the defaults') has the price 0 for that part:
  it is not traded that way (measured: the shipped trade files mark such lines `;NO TRADE`) [R-16].
- **The factors of an item** for a part: the character's table entry for the item if any, else the defaults'
  table entry, else the character's own two default factors [R-16].
- **The price** the trade computes from them (static; the trade is #284's): with the first factor *f*₁, the
  second *f*₂ and a value *t* = 0.5 + *v* / 2000 limited to 0 … 1 (*t* = 0 when the value *v* is missing), the
  factor is *f*₂ + (*f*₁ − *f*₂)·*t*; the price is that factor times (0.9·*c* + 0.1)^0.75 times the item's cost
  (in one case a cost the trader gives instead, #284), and 0 for an item in the part's item lists [R-16]. **Believed:** *v* is the attitude between the two trading characters (spec Q), *c* the item's
  condition. So the first factor (the one `[trade]` names *hostile*) applies in full at *v* ≥ 1000 and the
  second (*friendly*) at *v* ≤ −1000 [R-14, R-16]. Which part (buy or sell) is used for which direction of a
  deal, whose parameters (the trader's, believed), and what is done with a price 0 are **unestablished** here
  (#284).
- **The show list** (the character's and the defaults') answers whether an item is in neither list; it is asked
  by a routine that first tests whether the character's health is 0 or less [R-16]; what the answer decides is
  **unestablished** (believed: which items a dead character's body offers).

## 6. Trader members (static)

Apply to traders only; on any other object they do nothing (the diagnostic text goes to the routine that returns
at once, M 2.3) [R-23]:

| Member | Effect |
|---|---|
| `obj:set_trader_global_anim(name)` | plays the trader model's motion `name` as its body motion; when the motion ends it is forgotten [R-23] |
| `obj:set_trader_head_anim(name)` | the same for its head motion [R-23] |
| `obj:set_trader_sound(sound, head_anim)` | stops the trader's current sound (if any), plays the head motion `head_anim`, and plays the sound file `sound` for the trader [R-23] |

A motion name the model lacks is looked up as no motion; what that does is **unestablished** [R-23]. When the
engine asks for a new body or head motion or reports a sound's end (the callbacks `trader_global_anim_request`,
`trader_head_anim_request`, `trader_sound_end`) is spec O's; how the sound is positioned is spec T's (the call
passes flags whose meaning is T's) [R-23].

## 7. `game_difficulty` (static)

Integer constants: `novice` 0, `stalker` 1, `veteran` 2, `master` 3 [R-12]. They are the game difficulty values
of P 10: `level.get_game_difficulty()` returns and `level.set_game_difficulty(d)` stores the one difficulty value,
0 to 3 as these constants (what setting it does is P 10's) [R-12, R-28]. No shipped script names the class; `bind_stalker.script` maps the numbers 0 to 3 to the
console values `gd_novice`, `gd_stalker`, `gd_veteran`, `gd_master` (measured) [R-12].

## 8. Worked example: the generated help dialog

`dm_help_wounded_medkit_dialog` (`dialogs.xml`) has `init_func` `dialog_manager.init_help_wounded_medkit_dialog`
[R-24]. When the dialog is first loaded the function receives the dialog (2.1) and calls
`dlg:AddPhrase("dm_general_help_medkit","0","",-10000)`, making phrase `0` with no link (2.2); then
`phr:GetPhraseScript():AddAction("dialogs.transfer_medkit")` appends one action (2.4); then, for each thank-you
phrase of its table, `dlg:AddPhrase(v.name, v.phr_id, "0", -10000)`, making each phrase with a link from phrase
`0`, and, when the result is not nil, appends the precondition `dialog_manager.precondition_help_thanks` [R-24].
When phrase `0` is said, `dialogs.transfer_medkit` is called with the speaker of phrase `0`, the other speaker,
`"dm_help_wounded_medkit_dialog"` and `"0"` (3.3); each continuation's precondition is called with the
continuation's speaker, the speaker of phrase `0`, the dialog id, `"0"` and the continuation's id (3.2).

## 9. Use by the shipped scripts (measured)

By the scoping spec's scan, this spec's names are used by 17 loadable scripts (22 of all scripts) [R-2, R-25]:
`is_talking` (6 loadable scripts), `disable_talk` (5), `enable_talk`, `is_talk_enabled`, `stop_talk` (4 each),
`AddAction`, `AddPhrase`, `AddPrecondition`, `GetPhraseScript`, `run_talk_dialog`, `set_start_dialog` (2 each),
`AddGiveInfo`, `restore_default_start_dialog`, `set_trader_global_anim`, `set_trader_head_anim`,
`set_trader_sound` (1 each) [R-25]. `switch_to_talk`, `switch_to_trade`, `get_start_dialog`, `enable_trade`,
`disable_trade`, `is_trade_enabled`, `AddHasInfo`, `AddDontHasInfo`, `AddDisableInfo`, the global trade
functions and `game_difficulty` are used by none [R-2].

## 10. Verification facts

All reproduced by `run_all.py` (29 PASS lines, no failure):

- Scope: 36 listed items, 2 candidate strings that are not registrations, 16 used names, 17 loadable scripts
  [R-2, R-3].
- Phrase classes: 3 classes without base or constructor, 8 members paired with implementations; the six lists
  [R-4, R-5]; `AddPhrase` registered as (text, text, text, number) [R-6].
- Entry points: preconditions with 5 arguments and a truth-value result (dialog-level ones with two empty
  texts), actions with 4 and none, one-object forms with 1 and 2; init functions with the dialog [R-8 – R-11];
  every one found and called as D 1.1 and 1.2 state [R-26]; the information portions' receiving calls the
  one-object action form with the holder and nil, the lists' entries use the transfers of `IP` [R-27].
- Trade: 5 global registrations; defaults from `[trade]` (4 keys); the price rule's constants 0.0005, 0.5, 1,
  0.9, 0.1, 0.75 [R-13 – R-16].
- `game_object`: 17 members paired; talk flags on 20 character classes, monsters (17 classes) never talk-enabled;
  start dialog on 2 classes [R-18 – R-23].
- Data: 65 precondition, 78 action, 31 init function names in the listed dialog files [R-24].
- Difficulty: the level functions take and give the value of `game_difficulty`'s constants [R-28].

## 11. Unestablished

- Whether anything uses the one-object precondition form (3.4; believed nothing).
- `get_start_dialog`'s result when a character has no start dialog (4.4; believed nil); how the default start
  dialog is set (4.4).
- What an unknown motion name does in the trader members; the sound's positioning (section 6; spec T).
- In the price: what *v* and *c* are (believed attitude and condition), which part serves which direction,
  whose parameters, and what a price 0 leads to (5.3; #284).
- The section from which a character's own trade parameters are made (5.2; #284); what the show list decides
  (5.3).
- Whether a failed `cannot find section` check is fatal (5.3; believed).
- Everything that depends on A (argument matching, calls of missing members).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the game library identified above;
  the archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs
  (`scripts\*.script`, `config\system.ltx`, `config\gameplay\*.xml`, `config\misc\trade_*.ltx`).
- Analysis: static analysis with Ghidra (headless, read-only) of the script registration of the phrase classes,
  `game_difficulty`, the global trade functions and the talk, trade and trader members of `game_object`; of the
  member implementations and the engine routines they reach (adding phrases and links, the condition-and-effect
  part's evaluation and calls into scripts, saying a phrase, the dialog's load with its init function, the
  inventory owner's talk state and talk offer, the player's talk start, the dialog manager's start dialogs, the
  trade parameters, their reading, their defaults and the price computation, the trader's motions and sound);
  run-time type information read from the file bytes; the scoping spec's script scan reused; the listing, the
  scripts and the data read as text. No binary was executed.
- Revision (2026-11-28): the calls into scripts compared with spec D's entry points (the lookups and the error
  callback they share); the information portions' receiving and its call into their condition-and-effect part;
  the difficulty functions against spec P.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

