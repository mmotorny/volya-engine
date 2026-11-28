# Dialog files

Format specification DG-1 of the dialogue scoping specification (`docs/behavior/dialogue-scope.md`, sections
2.1 and 4) for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): the dialog data files, which of them
the game reads and when, their element tree, dialog and phrase ids and their scope, `priority` and the other
values a dialog and a phrase carry, texts as string ids or literals (and empty or missing texts), `next`,
the files and elements found outside the format, and what each kind of broken reference does. The meaning
of the conditions and effects (DG-2), how a talk starts (DG-3), how a dialog proceeds (DG-4), the window
(DG-5) and the generated dialogs' functions (DG-9) are those specifications'; this one gives only what the
files hold and how they are read.

It builds on the approved specifications `ltx-config` (configuration files, lists, path aliases),
`ui-layout` (XML documents: sections 1.2 `#include` lines, 1.3 syntax, 1.4 document model, lookups and
values), `text-and-fonts` (string table, section 1) and `db-archive` (winning copies) in `docs/formats/`,
and cites `console-log` (the fatal-error report, sections 3.8 and 3.10) and `script-loading` (script names
and namespaces, section 2.2) in `docs/behavior/`. None of them is repeated here.

Conventions:

- Paths are archive paths of the winning copies (`db-archive`); the dialog files are in `config\gameplay\`.
  "The program" is the shipped `bin\xrGame.dll` (with the shipped XML reader `bin\xrXMLParser.dll` for the
  lookups of `ui-layout` 1.4).
- **Fatal error** means a failed check with a message as `console-log` 3.8 reports it, after which the
  program ends as `console-log` 3.10 describes. Where a description and an argument are given, they are the
  report's `Description` and `Arguments` rows; the `Expression` row of these reports is a fixed text of the
  original program and carries nothing needed for compatibility.
- "Player character" is the character the player controls.
- Every fact carries its check, cited by label in brackets: `D`*n* are assertions of
  `checks/dialog-files/c_data.py` (the game data), `B`*n* of `checks/dialog-files/c_bin.py` (static
  analysis of the shipped binaries, read from the disassembly and decompilation that
  `checks/dialog-files/ghidra.py` re-creates). `run_all.py` runs both; all pass. **Believed** marks a
  reading not checked directly, with why; **unestablished** what could not be settled statically.

## 1. Which files are read, and when

- The list is `config\system.ltx`, section `[dialogs]`, key `files`: a comma-separated list (`ltx-config`)
  whose items are trimmed of surrounding white space [B2]. In this edition it has 14 names, in this order
  [D1]: `dialogs_test`, `dialogs`, `dialogs_escape`, `dialogs_garbage`, `dialogs_agroprom`,
  `dialogs_deadcity`, `dialogs_darkvalley`, `dialogs_pripyat`, `dialogs_bar`, `dialogs_yantar`,
  `dialogs_military`, `dialogs_labx18`, `dialogs_radar`, `dialogs_aes`. The names are written without
  extension and with a space after each comma [D1b].
- Item *N* is the file `gameplay\`*N*`.xml` under the path alias `$game_config$`, i.e. `config\gameplay\`*N*`.xml`
  [B2b]; all 14 exist [D1]. The wide-screen `_16` rule of `ui-layout` 1.1 does not apply (the directory is
  not `ui`).
- The files are read once, when the program starts: the game library's start step reads three id lists in
  this order, the information portions (`[info_portions] files`), the encyclopedia articles
  (`[encyclopedia] files`) and then the dialogs, before handing over to the engine's own start [B1, B1b].
  **Believed:** this is before the title menu is shown (it is the game library's application-start step).
  On a dedicated server the step is skipped [B1].
- Each file is opened and parsed whole at that time and its document is kept open for the rest of the run;
  a file that defines no dialog is closed again [B2e]. The contents of a dialog are read from the kept
  document only when the program first loads that dialog (section 3.2).
- A listed file that cannot be opened is a fatal error with the description `error while parsing XML file`
  and the argument *N*`.xml` [B2c]. A file that opens but does not parse stops the game inside the XML
  reader as `ui-layout` 1.3 describes.
- Files not in the list are never read by the dialog loader (section 9).

## 2. Document form

The XML rules are those of `ui-layout` 1.2 to 1.4: `#include` lines are expanded, the lenient syntax of 1.3
is accepted, the root is the first element, element names are compared exactly (case-sensitive) [B10d], an
attribute is found by exact name, an element's value is its text content when its first child is text and
otherwise the reader's default [B10e], and integers are read as `ui-layout` 1.4 states.

- The root element's name is not checked: every listed file uses `game_dialogs` [D2], and the program does
  not name it [B12b].
- Only the root's children named `dialog` are dialogs; they are counted and taken in document order [B2d].
  Any other child of the root is ignored. In this edition every child of every root is a `dialog` [D2].
- Dialogs per file, in list order: 3, 34, 17, 6, 14, 4, 5, 1, 52, 3, 29, 1, 10, 11 (190 in all) [D2b].
- All 14 files declare `encoding="windows-1251"` (two also `standalone="yes"`) [D2c]; none has an
  `#include` line or any line starting with `#` [D2].

Element tree read by the program (the **read set**) [B4, B4b, B5, B5b, B6]:

```
<game_dialogs>                       root, name not checked
  <dialog id="…" priority="…">       one per dialog
    <caption>…</caption>             optional
    <precondition>…</precondition>   0 or more each, in any order:
    <action>…</action>                 the condition and effect lists (section 6)
    <has_info>…</has_info>
    <dont_has_info>…</dont_has_info>
    <give_info>…</give_info>
    <disable_info>…</disable_info>
    <phrase_list>                    either this …
      <phrase id="…">
        <text>…</text>               optional
        <goodwill>…</goodwill>       optional
        <precondition>…</precondition>  … the same six lists …
        <next>…</next>               0 or more
      </phrase>
    </phrase_list>
    <init_func>…</init_func>         … or this, read only when there is no phrase_list
  </dialog>
</game_dialogs>
```

Nothing else is read: any other attribute or element of a dialog or phrase is ignored. The listed files use
exactly these element paths and attributes, no `caption` and no `goodwill` [D3, D3b, D3c]; every tag and
attribute name is written in lower case [D3b]. Every element and attribute name of the read set is named in
`xrGame.dll` [B12c, B4b].

## 3. Dialog ids

### 3.1 Registration at start

- The dialog's id is its attribute `id`. The ids of all listed files form **one id space**, registered in
  list order and, within a file, in document order; each id is kept with the file and the dialog's position
  among that file's `dialog` children [B2e].
- A `dialog` without an `id` attribute is a fatal error with the description `id for item don't set, number`
  *n* `in` *N*, where *n* is the dialog's 0-based position among the file's `dialog` children and *N* the
  list item (without `.xml`) [B2c].
- An id that an earlier dialog of any listed file already has is a fatal error with the description
  `duplicate item id` and the id as argument [B2c, B2f]. So a file listed twice is a fatal error.
  **Disputed in review:** only when that file defines at least one dialog; a file without dialogs registers
  no id, so listing it twice is not a duplicate.
- In this edition the 190 ids are unique and all of lower-case letters, digits and `_` [D4].

### 3.2 Loading a dialog

- The program loads a dialog by id. The id is looked up exactly (byte for byte) [B3]. An id that no listed
  file defines writes one log line `[`*n*`]=[`*id*`]` for every registered dialog (*n* from 0, in
  registration order) and then is a fatal error with the description `item not found, id` and the id as
  argument; nothing can make the lookup quiet [B3, B3b].
- A found dialog is read from its kept document: the *n*-th `dialog` child of that file's root, *n* being
  its registered position [B4f]. Its contents are read the first time its id is loaded and then shared by
  every later use of that id until the program ends [B9, B9b]. Errors in a dialog's contents (sections 5.2
  and 8) therefore appear when it is first loaded, not at start.
- When and which dialogs the program loads (a character's start and actor dialogs, the information
  portions' dialogs) is the talk specification's (DG-3). Dialog ids named outside the dialog files:
  - 8 ids named by specific characters are defined by no listed file: `bar_bar_psih_give_medkit`,
    `bar_bar_psih_start_pain`, `bar_bar_psih_talk_gorbun`, `bar_bar_psih_talk_lisiy`,
    `bar_bar_psih_talk_pain`, `bar_svoboda_volkodav_info_dp`, `bar_svoboda_volkodav_start`,
    `gar_wounded_bandit` [D12]; loading any of them is the fatal error above.
  - An information portion's `dialog` children (in the `[info_portions]` files) are read, when the
    portion is loaded, as a list of dialog ids in document order, each the element's value (default empty)
    [B13]. The only one in this edition is `global_dialogs` → `actor_break_dialog`, which is defined
    [D12b]. What the program does with this list is **unestablished** here (the scope's review found that
    no specification owns it).
  - The program itself names the dialog id `hello_dialog` [B12], defined by `dialogs.xml` [D12c]; what it
    uses it for is DG-3's.

## 4. A dialog

| Item | Read as | Default | Meaning |
|---|---|---|---|
| attribute `id` | text | none (fatal, 3.1) | the dialog's id |
| attribute `priority` | integer | 0 | **believed** the order in which offered dialogs are listed (DG-4) |
| element `caption` | text | none | the text that stands for the dialog where the talk window lists dialogs to choose from; when absent or empty, the text of the dialog's phrase `0` stands for it [B7, B7b] |
| the six lists | section 6 | empty | the dialog's own conditions and effects (DG-2) |
| element `phrase_list` | section 5 | — | the dialog's phrases |
| element `init_func` | text: a script function name | empty | read only when there is no `phrase_list` [B4c]: the function the program calls, once, when the dialog is loaded, with one argument and no result, to build the phrases (DG-9) [B4b, B4d] |

Sources: [B4, B4b, B4c, B4d, B7]. `priority` and `caption` are read from the dialog itself; the first
`phrase_list` child is taken.

In this edition [D4b, D4c, D9]: 159 dialogs have a `phrase_list` and 31 an `init_func` (all in
`dialogs.xml`), none both and none neither, none two `phrase_list`s; 41 dialogs have `priority`, all
decimal integers: −5, −3, −2, −1 (14), 0 (13), 1 (2), 2 (2), 5 (2), 6, 7, 10 (3). The dialog-level lists in
use are `has_info` (60), `dont_has_info` (57) and `precondition` (51) (`dialogue-scope` 1.1); no dialog has
`action`, `give_info` or `disable_info` [D3]. Whether the dialog-level effects would be run is DG-2's.

A dialog with neither `phrase_list` nor `init_func` has an empty function name; what that does is section 8.

## 5. Phrases

### 5.1 Ids and their scope

- A phrase's id is its attribute `id`. Phrase ids belong to their dialog: they are looked up only among the
  `phrase` children of that dialog's `phrase_list` [B4f, B10b]; the same id in another dialog is another
  phrase (`0` occurs in all 159 phrase lists) [D5c].
- The lookup takes those children in document order and returns the first whose `id` equals the wanted
  text exactly; a `phrase` without `id` compares as the empty text [B10b, B10c]. So of two phrases with the
  same id only the first is ever read. In this edition phrase ids are unique within each dialog and all
  decimal digits, 2,296 phrases in all [D5].
- Phrase `0` is the dialog's first phrase. It need not be the first `phrase` element: it is in only 30 of the
  159 lists [D5b]. Every phrase list of this edition has one [D5b].

### 5.2 Reading the phrases

The phrases are read when the dialog is loaded (3.2), starting from phrase `0` and following `next`
[B4b, B5b]:

1. Phrase `0` is looked up (5.1) and read with no previous phrase.
2. Reading a phrase: its first `text` element's value (default: empty) and its `goodwill` element as an
   integer (default −10000) [B5]. If a phrase with this id was already read in this dialog, nothing more is
   read from it [B5c, B5d]; otherwise it becomes a phrase of the dialog with that id, text and goodwill
   [B5d], and its six lists (section 6) are read [B5b].
3. Unless there is no previous phrase, a link from the previous phrase to this one is added; this happens
   also when the phrase had been read before [B5d]. **Disputed in review:** the test is on the previous
   phrase's id, not on its existence: no link is added when the previous phrase's id is empty, which is also
   the case for a phrase without `id` reached through an empty `next` (5.1); none occurs in this edition.
4. For a phrase read for the first time, each of its `next` elements in document order names the id of a
   continuation (default: empty), which is looked up (5.1) and read by steps 2 to 4 with this phrase as the
   previous one [B5b].

Consequences:

- A phrase's continuations are its links, in the document order of its `next` elements; a phrase may be the
  continuation of several phrases, and links may form cycles back to an earlier phrase (a phrase met again
  gains only the link) [B5d]. In this edition: 2,196 `next` elements, none repeated within a phrase and none
  naming its own phrase [D8]; 70 phrases are the continuation of more than one phrase of their dialog, and
  10 dialogs have a cycle reachable from phrase `0` [D8b]. Which continuation is said, and in which order
  they are tried, is DG-4's.
- **Phrases that cannot be reached from phrase `0` through `next` are never read**: their texts, lists and
  `next` elements have no effect, broken or not. 213 phrases of this edition are such [D8d].
- `goodwill`: **believed** the value the non-player speaker's choice among several sayable continuations
  compares with its attitude (DG-4); no listed file has it [D3c].
- Data shapes [D6, D6b, D14e]: no phrase has two `text` elements; no list, `next` or `init_func` element is
  empty; no value has surrounding white space; empty texts are written `<text/>` (254) or `<text></text>`
  (94); a phrase has up to 10 `next` elements.

## 6. Condition and effect lists

Dialogs and phrases carry the same six lists, read in this order: `precondition`, `action`, `has_info`,
`dont_has_info`, `give_info`, `disable_info` [B6, B6c]. Each list is the values of all child elements of
that name, in document order (an element without text gives an absent value) [B6b]; elements of different
names may be interleaved, as only each name's own order is kept.

| Element | Value |
|---|---|
| `precondition`, `action` | a script function name (below) |
| `has_info`, `dont_has_info`, `give_info`, `disable_info` | an information-portion id (`[info_portions]` files) |

- **Script function names** are written *script*`.`*function*; for a name with one dot the program splits
  it into the namespace *script* and the function *function*, loads the namespace first unless it is `_G`
  (as `script-loading` 2.2, occasion 4), and then looks the function up [B8]. Every name in this edition
  has exactly one dot and is of lower-case letters, digits and `_` [D10]. `init_func` (section 4) takes the
  same path.
- **Information-portion ids** are compared exactly with the ids the player character holds (`has_info`,
  `dont_has_info`) or handed to the player character's giving and disabling of portions (`give_info`,
  `disable_info`) [B11d, B11e].
- What the lists decide and do, their evaluation order and the functions' arguments are DG-2's.

## 7. Texts

- A phrase's text is the value of its first `text` element. A missing `text` element and an empty one
  (`<text/>`, `<text></text>`) are the same: the empty text [B5].
- Where the talk window shows a text (a line of the answer log, an entry of the question list) it passes it
  through the string table's lookup (`text-and-fonts` 1.4) [B7c, B7d, B7e]. So a text that is a string id
  shows that string, and any other text shows itself, verbatim.
- An empty text is never shown: the window adds no line and no entry for it [B7c, B7d]; the same holds for a
  dialog whose caption and phrase-`0` text are both empty (section 4). What the dialog's flow does with a
  phrase without text is DG-4's.
- In this edition [D7, D7b]: 108 phrases have no `text` element and 348 an empty one; of the others, 1,827
  are string ids and 13 are not in the string table (the `gar_dolg_bribe_*` and `optional_dialog_test_*`
  texts, all id-shaped), so these 13 show their own text.

## 8. Broken references

| Reference | What the program does | In this edition |
|---|---|---|
| a `next` naming no phrase of the dialog | when the dialog is loaded and the phrase is reached (5.2): a fatal error whose description is the missing id [B5b] | one: `val_escort_nap1_dialog`, phrase 2111 → 412; 2111 cannot be reached from phrase `0`, so nothing happens, and no specific character names the dialog [D8c] |
| no phrase `0` in the phrase list | when the dialog is loaded: reading the missing phrase is a fatal error of the XML reader (a lookup from no element; argument the document's path `gameplay\`*N*`.xml`) [B4b, B10, B10f] | none [D5b] |
| a text that is not a string id | shown verbatim (section 7) | 13 [D7] |
| a script function no shipped script defines (`precondition`, `action`, `init_func`) | nothing at load; when the program calls it (DG-2; `init_func` when the dialog is loaded) the lookup's failure is not tested and the call is made with no function and no runtime state [B8, B8b, B8c, B8d, B8e, B8f, B4g]. **Believed** an access fault (a crash); what follows is **unestablished** | 12 names (`dialogue-scope` 1.1). In phrases reachable from phrase `0`: `bar_dialogs.give_lucky_pda`, `give_bar_tiran_pda`, `have_svoboda_leader_reward`, `have_tiran_reward`, `have_borov_reward`, `have_volkodav_reward`; only in unreachable phrases: `escape_dialog.give_wounded_flash_reward`; in dialog preconditions: `bar_dialogs.actor_need_reward`, `have_lucky_pda`, `have_bar_tiran_pda`; init functions: `dialog_manager.init_trade_info_dialog`, `init_action_info_dialog` [D10b; the dialog preconditions and init functions: `dialogue-scope` 1.1] |
| an information portion no `[info_portions]` file declares, in `has_info` or `dont_has_info` | silent: such a portion is never held [B11] | has_info 12 ids, dont_has_info 11 [D11] |
| the same, in `give_info` or `disable_info` | when the effect runs: the portion is loaded by id first [B11b, B11c], so one log line `[`*n*`]=[`*id*`]` per declared portion, then a fatal error with the description `item not found, id` and the id [B3c] | give_info 8 ids, all in phrases reachable from phrase `0`: `bar_bar_kill_bloodsucker_done`, `bar_bar_kill_borov_done`, `bar_bar_kill_svoboda_leader_done`, `bar_bar_kill_veterans_done`, `bar_bar_yantar_off_done`, `bar_kill_borov_reward`, `bar_kill_svoboda_leader_reward`, `bar_volkodav_have_reward`; disable_info none [D11, D11b] |
| a dialog id no listed file defines | when loaded: fatal error (3.2) | 8 named by characters [D12] |
| an unreachable phrase | never read (5.2) | 213 [D8d] |

The function-name rows hold for any name, also for a dialog with neither `phrase_list` nor `init_func`,
whose `init_func` is empty: an empty name is never found [B8].

## 9. Files and elements outside the format

- `config\gameplay\` has four further files with "dialog" in the name (`dialogue-scope` 1.1):
  `character_dialogs.xml` (a fragment of the character files, not a dialog file), `dialogs old.xml`,
  `dialogs_escape_1.xml` and `dialogs_escape_1_string.xml`. Only listed files are read (section 1), and no
  configuration file, script or string of `xrGame.dll` names the other three (`dialogue-scope` 1.1), so they
  have no effect.
- `dialogs old.xml` (10 dialogs) and `dialogs_escape_1.xml` (8 dialogs) have the dialog-file form [D13];
  some of their ids are also ids of listed dialogs (`actor_break_dialog`, `hello_dialog`,
  `stalker_wounded_friend`) [D13c], which would be a fatal duplicate (3.1) if they were listed.
- The dialog attribute `pda` and the phrase element `script_text` occur only in `dialogs old.xml` [D13b].
  Neither is in the read set (section 2), so both would be ignored even in a listed file; `xrGame.dll` does
  not name `script_text` [B12b].
- `caption` and `goodwill` are in the read set but unused by the shipped data (sections 4, 5.2).

## 10. Worked example: the trader's start dialog

`escape_trader_start_dialog` in `dialogs_escape.xml` (`dialogue-scope` 3) uses [D14, D14b, D14c, D14d, D14e]:

- `dialog@id`, one `phrase_list`, no `priority`, no dialog-level list;
- 51 `phrase@id`, with the children `text` (51: 36 string ids, 15 empty), `next` (46), `precondition` (18),
  `give_info` (14), `dont_has_info` (13), `action` (10), `has_info` (7) and `disable_info` (1);
- repeated elements: phrase `0` has 10 `next` elements, in the order 12, 13, 14, 15, 18, 19, 17, 20, 30, 66;
  a phrase has up to 3 `give_info` and up to 3 `dont_has_info`; phrase 31 has two `action` elements,
  `dialogs.break_dialog` then `escape_dialog.run_tutorial_pda`; phrase 32111 gives `esc_trader_can_leave`
  and `tutorial_wounded_start`;
- every `next` names a phrase of the dialog, every text is empty or a string id, and every information
  portion is declared;
- 13 phrases cannot be reached from phrase `0` and are never read: 6, 111, 199, 511, 571, 1111, 1999, 5111,
  5711, 51111, 57111, 521211, 5212111; the dialog's one undefined function,
  `escape_dialog.give_wounded_flash_reward`, is in 5212111 and so has no effect.

So every element and attribute this dialog uses is covered by sections 4 to 7.

## 11. Unestablished

- What follows the call of a script function that does not exist (section 8): believed an access fault.
- What an element of the six lists without text does when the list is used (section 6); none occurs.
- What the program does with an information portion's `dialog` list (section 3.2).
- Whether the dialog-level `action`, `give_info` and `disable_info` lists are ever run (section 4; DG-2).
- That the start step runs before the title menu is shown (section 1; believed).

## Provenance

- Date: 2026-11-27. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binaries `bin\xrGame.dll` and `bin\xrXMLParser.dll`.
- Data examined: the winning copies of `config\` and `scripts\` as extracted for the approved `pda-scope`;
  the dialog, character, information-portion and string-table files.
- Analysis: static analysis (disassembly and decompilation with Ghidra, headless and read-only) of the
  shipped game library and XML reader; reading constants from them; searches and a walk of each dialog's
  phrases in the data. No program was executed; the game was not run.
- Checks: `checks/dialog-files/c_data.py` and `c_bin.py`, run with `python3 -I` (`run_all.py` runs both),
  with the `dialogue-scope` and `pda-scope` helpers; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- How a script function name with no dot, or with more than one dot, is split into namespace and function
  (section 6 states only names with one dot).
- What an empty item of `[dialogs] files` (two commas in a row, a trailing comma) does, and what a missing
  `[dialogs]` section or `files` key does.
