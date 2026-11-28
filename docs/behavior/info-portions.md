# Information portions

Format and behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of
**information portions**: the story flags the game keeps per object. It covers the files of
`[info_portions]` and what each `info_portion` element and its children (`article`, `article_disable`,
`task`, `disable`, `dialog`, `action` and the other script children) mean; giving and disabling a portion,
whoever does it; the list of portions an object holds and the times it received them (`sav` 9 part 1); what
a newly given portion causes (its action functions, the portions it disables, its articles, its tasks, the
player character's callback, its dialogs); the questions (holds, does not hold); the portions the engine
gives by itself (the interface portions `ui_*`) and those of documents and PDAs (`sav` 7); and what a
reference to an id that is not declared does. It is PD-8 of `pda-scope`, and it also settles what the
`dialog` child does (`dialogue-scope`'s review gap 5).

Builds on (not repeated): `docs/formats/sav.md` (cited as `sav`), `docs/formats/ltx-config.md` (cited as
`ltx`), `docs/formats/ui-layout.md` (cited as `layout`: the XML reader of section 1, its `#include` lines and
its meaning of "the game stops"), `docs/behavior/script-loading.md`, `docs/behavior/script-environment.md`,
`docs/behavior/pda-scope.md`, `docs/behavior/dialogue-scope.md`, `docs/behavior/pda-window.md`,
`docs/behavior/script-api-scope.md`.

Conventions:

- The **player character** is the object the player controls. A **holder** is an object that can hold
  portions: an object that carries an inventory (the player character and the characters; believed: also
  every other kind that carries one). Only holders give, disable or hold portions.
- A portion is **declared** when an `info_portion` element of section 1 has its id.
- **The game stops** has `layout` 1's meaning (with that section's review note): the message named is
  reported, and nothing after the failing step happens.
- Script names (`give_info_portion`, `has_info`, `inventory_info`, `bind_stalker`, …) are the shipped
  scripts' and the script interface's own names, which the game data uses. How scripts call the members
  named here (argument forms, results) is `script-api-scope` P's; this specification states what the engine
  does when a portion is given, disabled or asked about, whoever asks.
- **Named in `xrGame.dll`** means the exact text occurs in the shipped `bin\xrGame.dll` as a printable
  string.
- Statements are static (read from the data and the shipped binaries; the game was not run) unless marked
  otherwise. Inferred statements are marked **believed**; what could not be established is marked
  **unestablished**.
- Bracketed ids are the checks reproducing a statement: `D…` in `c1_data.py`, `B…` in `c2_binary.py`, in the
  dirty workspace's `checks/info-portions/` (`python3 -I run_all.py` runs them all; the data comes from
  `checks/pda-scope/extract.py`).

## 1. Declaration

### 1.1 Files

- `config\system.ltx` `[info_portions]` `files` is a comma-separated list of file names (`ltx` list rules;
  each item trimmed) [D1, B1]. For each name, in list order, the engine opens `config\gameplay\<name>.xml`
  (path alias `$game_config$`, directory `gameplay`, the name with `.xml` appended; the names carry no
  extension) with `layout` 1's reader, `#include` lines included [B1, B1b, D1c].
  - A file that cannot be opened stops the game with the message "error while parsing XML file" and the
    file's name [B1, B17]. A file that is not well-formed stops it with `layout` 1.3's parse error
    [B17].
  - The **root** is the file's first element; its name is not checked [B17]. The portions are the root's
    children named `info_portion` (names compared exactly); other children are ignored [B1]. A file with
    no element at all, or whose root has no `info_portion`, declares nothing and is not an error [B17,
    B17b, B1].
- The list has 32 names, from `info_portions` to `info_stories` [D1]. All 32 files exist; none has an
  `#include` line [D1b, D1e]. `config\gameplay` holds 43 `info_*.xml` files, so 11 are never read
  (`info_escape`, `info_l04darkdolina`, `info_l05rostok`, `info_l06yantar`, `info_l07military`,
  `info_l12stanciya`, `info_l13generators`, `info_l15radar`, `info_l18pripyat`, `info_l20sarcofag`,
  `info_news`) [D1d].
- Of the 32 files, 10 are 45 bytes holding only the XML declaration (`info_l04agroprom_u`,
  `info_l08rostok_bar`, `info_l09marshes`, `info_l09rostok_dolg`, `info_l12yantar_u`, `info_l13deadcity`,
  `info_l14swamp`, `info_l17monolith_control`, `info_l21generators`, `info_l22warlab`) and
  `info_l11yantar` has an empty root; the other 21 declare 1,707 portions, every root is
  `game_information_portions`, and no root has another child [D2, D2b, D2c, D2d].

### 1.2 The index of ids

- Every `info_portion` must have an attribute `id`; one without it stops the game with "id for item don't
  set, number <n> in <file>" (*n* its 0-based position among the file's portions) [B1]. An id declared
  twice, in the same or another file, stops the game with "duplicate item id" and the id [B1]. Ids are
  compared exactly.
- The index (every id with its file and position) is built once, by the game's one-time initialisation of
  its game data, and is not built on a dedicated server [B1c]; which moment that is (believed: at program
  start, before the title menu) is not traced here.
- **Looking up an id** searches the index in file order and position order [B2]. An id that is not declared
  stops the game: first every declared id is written to the log as one line `[<index>]=[<id>]` (index from
  0 in index order), then the message "item not found, id" and the id [B2]. Articles (`[encyclopedia]`) and
  dialogs (`[dialogs]`) are looked up the same way, with the same failure [B2b, B2c].
- A portion's children (1.3) are read from its element the first time the portion's data is used
  (any giving or disabling, 3 and 4), then kept for the rest of the run and shared by all holders [B3c].
- Data: all 1,707 elements have an id, no id occurs twice; ids are ASCII letters, digits, `_` and `-`
  (40 have a `-`) [D3, D3b]. `info_l01escape` declares 140, `info_l07rostok` 241 [D2e].

### 1.3 Children

A portion's children are the element's direct children with these names, in file order within each name
[B3, B3b]. The value of each is its text (`layout` 1.4).

| Child | Meaning | Count in the data [D4] |
|---|---|---|
| `dialog` | a dialog id offered to the player character while it holds the portion (6) | 1 |
| `disable` | a portion id disabled on the same holder when this one is given (3.2) | 61 |
| `precondition`, `has_info`, `dont_has_info` | read, and never used when the portion is given (believed: no effect at all, 3.3) | 0 |
| `action` | a script function called when the portion is given (3.3) | 2 |
| `give_info`, `disable_info` | portion ids given to / disabled on the player character when this one is given (3.3) | 0 |
| `article` | an article id given to the player character (3.4) | 297 |
| `article_disable` | an article id removed from the player character's articles (3.4) | 0 |
| `task` | a task id given to the player character (3.4) | 79 |

- No other child occurs; every child in the data has a non-empty text and no children [D4, D4b]. What an
  empty child would do is **unestablished**.
- 1,439 portions have no child: giving them only adds them to the list (3.2) [D4c].
- The two `action` children are both in `info_l10military`: `mil_cook_map_spot` →
  `mil_tasks.set_cook_mapspot`, `mil_lukash_map_spot` → `mil_tasks.set_lukash_mapspot`; both functions
  exist in `scripts\mil_tasks.script` and take no parameters [D4d, D4e].
- The one `dialog` child: `global_dialogs` (`info_portions.xml`) → `actor_break_dialog`, a dialog declared
  by the `[dialogs]` files [D4f, D5d].

## 2. The list a holder keeps

- Each holder has a **list** of the portions it holds: entries of id and the game time at which it was
  given, in the order given [B4, B4c]. It is the simulation's registry of `sav` 9 part 1, which saves each
  entry as the id and its 8-byte time; loading an entry restores only those two values [B18, B18b], so
  loading a save causes none of the effects of 3 (believed: nothing else runs on load).
- Disabling removes the entry (4). That removing keeps the other entries' order is **believed**. Whether a
  holder whose last portion was removed keeps an empty entry in the saved registry is **unestablished**.
- **Holds** (the question `has_info` asks): whether the holder's list has an entry with that id [B6b]. It
  does not look the id up in the index, so asking about an undeclared id gives *does not hold*, without
  error [B6b].
- The simulation can be asked the same about any object id (the script interface's `has_info` of the
  simulation, which `_g.has_alife_info` calls with object 0, believed the player character [D10c]): *does not
  hold* when that object has no list [B7f].
- The list also gives each held portion's received time (named in `xrGame.dll`: `get_info_time`; the
  shipped scripts never call it [D10]).

## 3. Giving

### 3.1 Transfer: the step every giver uses

Every giver except the document (7.4) gives a portion to a holder by a **transfer**, which does, in order
[B5]:

1. sends a message to the game's server naming the holder, the id and *give*;
2. looks the id up (1.2): an undeclared id stops the game here;
3. **receives** the portion on the holder at once (3.2, and 3.4 for the player character).

The server sends the message back to every client, the game's own included [B14]; when it arrives, the
holder receives the portion again (3.2) [B14b, B14c]. That the returning message is applied on a later
update than the transfer is **believed** (its delivery was not traced). Because receiving a held portion
does nothing (3.2 step 1), the returning message normally has no effect. **Believed** consequence: when the
portion was disabled after the transfer and before its message returned (a give and a disable of the same
id in one update, or a portion's own `disable` child naming a portion given just before), the returning
*give* gives it again with all of 3.2–3.4, and a returning *disable* (4) then removes it again.

### 3.2 Receiving, on any holder

[B4]

1. If the holder's list holds the id, nothing happens (and, for the player character, nothing of 3.4).
2. An entry (id, current game time) is appended to the list.
3. The portion's data is used (1.2; an undeclared id stops the game).
4. The script part runs (3.3).
5. Each `disable` child, in order, is disabled on the same holder by a transfer (4.1): so a `disable`
   naming an undeclared portion stops the game.

### 3.3 The script part

[B13, B13b, B13f]

1. Each `action` child, in order, names a script function (`namespace.function`, `script-loading` 2.2
   occasion 4), which is called with two arguments: the receiving holder's script object and nil
   [B4b, B13c, B13e]. An error raised by the function ends the program [B13d]. What happens when no such
   function exists is **unestablished** (the engine does not test the lookup's result before calling)
   [B13].
2. Each `give_info` child, in order, is given to the **player character** by a transfer, and then each
   `disable_info` child is disabled on the player character by a transfer — whichever holder received the
   portion [B13f].

The `precondition`, `has_info` and `dont_has_info` children are read with the others (1.3) but receiving
does not test them [B4, B13]; no other use of them was found (believed: none).

### 3.4 Receiving, on the player character

The player character receives as in 3.2, and only when that gave it a portion it did not hold, then [B8,
B8b, B8e]:

1. **Articles** [B9]: first every article the player character knows whose id equals an
   `article_disable` child is removed from its articles; then each `article` child, in order, that the
   player character does not yet know is looked up among the articles (1.2: an undeclared article stops
   the game) and added to its articles with the current game time, the article's type and *not read*
   (`sav` 9 part 4), and the script callback `article_info` (number 12) is called for it. What the PDA shows
   for a new article (marks, news, the diary) is `pda-scope` PD-9's.
2. **Tasks** [B10]: each `task` child, in order, that the player character does not have yet is created from
   the `game_task` element of `config\gameplay\game_tasks.xml` (with its `#include` lines) whose `id` is the
   child's text, given the current game time as its received time, and added, and the script callback
   `task_state` (number 13) is called. What giving a task does beyond this is `pda-scope` PD-6 and PD-7's. A
   task id with no `game_task` element stops the game with "NavigateToNode failed in XML file " and the
   file name [B10b, B10c, B10d, B10e].
3. **The callback** `inventory_info` (number 11 in the shipped listing) is called with the player
   character's script object and the portion's id [B8b, B8c]. It is called only here: not on disabling, not
   for other holders, and not when the portion was already held. The shipped player binder sets it to
   `info_callback(npc, info_id)`, which runs `level_tasks.proceed` and `level_tasks.process_info_portion`
   [D10d] (`pda-scope`'s second review gap).
4. If the dialogue window is open (believed: the window the test names), it is marked to rebuild its
   questions (6) [B8b].

Other holders get none of these: a portion with `article` or `task` children given to a character adds
only the list entry and runs 3.2 and 3.3.

## 4. Disabling

### 4.1 Transfer

Every disabling goes through a transfer like 3.1, with *disable*: the message to the server, the lookup of
the id (an undeclared id stops the game, even when the holder does not hold it), then the disabling at once,
and again when the message returns [B5, B14b].

### 4.2 Disabling on a holder

The entry with that id is removed from the holder's list, if there is one; nothing else happens: no action
function, no callback, and the portion's articles and tasks stay [B6]. On the player character, if the
dialogue window is open it is then marked to rebuild its questions (6) [B8d].

A pair of portions that disable each other (each the other's only `disable` child) is a two-state switch:
giving one removes the other. The data has six such pairs, all `ui_*` (7.3) [D7b]; no portion disables
itself [D5e].

## 5. Giving and asking from scripts and dialogs

What the engine does for each caller; the call forms are `script-api-scope` P's (scripts) and
`dialogue-scope` DG-2's (when a phrase or dialog acts).

- The script object members `give_info_portion(id)` and `disable_info_portion(id)` transfer (3.1, 4.1) on
  the object when it is a holder; on any other object they do nothing [B7, B7b, B7c].
- `has_info(id)` answers *holds* (2) on a holder and *false* on any other object; `dont_has_info(id)` is its
  negation, *true* on any other object [B7, B7d, B7e]. Both are named in `xrGame.dll` [B7h].
- A dialog phrase's `give_info` and `disable_info` lists act like 3.3 step 2: transfers on the player
  character [B13f] (believed: the phrases use the same step; when they act is DG-2's).
- `_g.GiveInfoViaPda` (in `_g.script`; used by `info_pda`, `info_pda2`, `info_test`, `info_test1`) [D11c] calls the member
  `give_info_portion_via_pda`, which no shipped binary contains [D10b, B7g]: the call has no engine
  behaviour, and what the script runtime does with it (believed: an error, `script-api-scope` 4) is not
  this specification's (`pda-scope`'s fifth review gap).
- Data: the shipped scripts call `give_info_portion` 800 times, `disable_info_portion` 17, `dont_has_info`
  2 [D10].

## 6. The `dialog` child

The player character's **dialog list** (the questions it can start a dialog with in a talk; `dialogue-scope`
DG-3) is built for a talk partner as follows [B11, B11b, B11c, B11d]:

1. The list is emptied, and so is the record of the dialogs tried.
2. For each portion the player character holds, in list order, each of its `dialog` children, in order, is
   tried; then each of the partner's actor dialogs (its specific character's `actor_dialog` list,
   `dialogue-scope` 1.3) is tried.
3. Trying a dialog id: if the id was already tried in this build, nothing; otherwise it is recorded as
   tried, the dialog is looked up (1.2: an undeclared dialog id stops the game) and it is added to the list
   only if its own conditions hold for the player character and the partner (`dialogue-scope` DG-2).
4. The list is ordered by the dialogs' `priority`, highest first. The order of dialogs of equal priority is
   **unestablished**.

The list is built when the dialogue window needs it (believed: when a talk starts, DG-3) and again on the
window's next update after it was marked (3.4 step 4, 4.2), when no dialog is in progress [B12, B12b];
while a dialog is in progress the mark is handled by the dialog's own phrase list (believed; DG-4).

So a held portion's `dialog` child offers its dialog in every talk, with every partner, subject to the
dialog's own conditions; disabling the portion withdraws it at the next build. No other use of the
`dialog` child was found; portions held by other holders offer nothing (believed). In the data the only one
is `global_dialogs` → `actor_break_dialog` (`priority` −5, precondition `dialogs.allow_break_dialog`, in
`dialogs.xml`) [D11, D11b], which the shipped player binder gives when the simulation says the player
character does not hold it [D4f, D6d].

## 7. Portions the engine gives

### 7.1 Who gives in the data

Besides the scripts (5) and phrases, the engine gives portions in 7.3 and 7.4 only; portions' own
`give_info` children do not occur in the data [D4]. **Disputed in review:** "in 7.3 and 7.4 only" is
unbacked: the giving step is reached indirectly, and no complete search for every engine route that gives a
portion was made; treat other engine givers as unestablished.

### 7.2 The first level's start

The shipped player binder gives, when not held: `storyline_actor_start` on `l01_escape` (its child: the
task `storyline_eliminate_gunslinger`), `global_dialogs`, `level_changer_icons` [D6c, D6d, D11d].

### 7.3 Interface portions (`ui_*`)

All 29 `ui_*` portions are declared in `info_portions.xml`; six pairs disable each other (`ui_pda` /
`ui_pda_hide`, `ui_pda_jobs` / `…_hide`, `ui_inventory`, `ui_talk`, `ui_trade`, `ui_car_body`, each with its
`_hide`), the other 17 have no children [D7, D7b, D7c].

The engine gives these, by a transfer to the player character, only in single player (believed: the game
type it tests) and only while the controlled object is the player character [B16, B16o]:

| Portion | Given when |
|---|---|
| `ui_pda` / `ui_pda_hide` | the PDA window is shown / hidden [B16b, B16c] |
| `ui_inventory` / `ui_inventory_hide` | the inventory window is shown / hidden [B16d, B16e] |
| `ui_talk` / `ui_talk_hide` | the dialogue window's dialog part is shown / hidden [B16f, B16g] |
| `ui_trade` / `ui_trade_hide` | the trade window is shown / hidden [B16h, B16i] |
| `ui_car_body` / `ui_car_body_hide` | the window showing another object's items (believed: bodies and boxes, `carbody_new.xml`) is shown / hidden [B16j, B16k] |
| `ui_pda_events`, `ui_pda_contacts`, `ui_pda_ranking`, `ui_pda_actor_info`, `ui_pda_encyclopedia` | the PDA's active tab becomes tab 2, 3, 4, 5, 6 (`pda-window`'s numbering), by any route, including (believed) tutorial steps that open a PDA tab [B16m, B16p] |
| `ui_pda_map_local` | the map window is shown (believed: tab 1 becoming active) [B16n] |
| `ui_pda_news_hide` | the news window is hidden [B16l] |

Tabs 0 and 1 give no portion on becoming active [B16m]. The other `ui_*` portions (`ui_pda_contacts_hide`,
`ui_pda_map_global`, `ui_pda_news`, `ui_pda_jobs*`, `ui_pda_contracts`, `ui_pda_diary*`,
`ui_pda_encyclopedia_hide`) are given by no engine code that names them: of the 29 ids, only the 17 in the
table are named in `xrGame.dll` [B16q].

### 7.4 Documents and PDAs

- A **document** item (class `II_DOC`, section `document`) carries one portion id (`sav` 7.4.2). When it is
  taken into the inventory of a holder and its id is not empty, the engine sends the server a *give*
  message for that holder and id, without the immediate receiving of 3.1; the holder receives it when the
  message returns (3.1) [B15, B15b, B14]. It is sent again each time the item is taken; a portion still
  held is not given again (3.2). An undeclared id stops the game when the message is applied (1.2).
- A **PDA** item (class `D_PDA`) carries an original owner id, a specific character and a portion id
  (`sav` 7.4.2, in that order) [B15d]. The PDA in the level reads only the owner and the specific character
  (which names it) [B15c, B15e]; no use of its portion id was found, so taking a PDA gives nothing
  (believed).
- Data: `all.spawn`'s 8,641 objects include no document and no PDA [D8, D8b]; no shipped object gives a
  document or PDA portion (believed: the shipped scripts create neither).

## 8. References to undeclared ids

| Reference | What happens | In the data |
|---|---|---|
| giving or disabling an undeclared portion (any route) | the game stops (1.2), when it is given or disabled | the dialogs give 8 undeclared ids (`give_info`), disable none [D9]; the scripts give 6 by literal id: `pl_task_2`, `pl_task_3`, `agr_krot_band`, `pri_followers_task_end`, `rat_swarm`, `start` [D9c] |
| asking whether an undeclared portion is held | *does not hold*, no error (2) | the dialogs test 20 undeclared ids (`has_info` 12, `dont_has_info` 11) [D9, D9b] |
| a `disable` child naming an undeclared portion | the game stops when the portion is given (3.2) | none [D5c] |
| an `article` child naming no article | the game stops when the portion is given to the player character (3.4) | 3: `tr_escape_broken_car` → `tr_escape_broken_car`; `agr_nii_security_shedule_file_have` → `agr_nii_security_shedule`; `agr_nii_fab_mil_task` → `agr_nii_fab_mil_task` [D5] |
| a `task` child naming no task | the game stops when the portion is given to the player character (3.4) | 6 portions, 5 ids: `gar_help_newbie` → `gar_help_tunnel_newbie`; `bar_kill_professor_start` → `bar_kill_professor`; `mil_test_task` → `mil_test_task`; `rad_find_stalkers_reward_start`, `rad_find_stalkers_free_start` → `rad_find_stalkers`; `rad_find_treasurer_start` → `rad_find_treasurer` [D5b] |
| a `dialog` child naming no dialog | the game stops when the player character's dialog list is built (6) | none [D5c] |

Given to a holder other than the player character, the portions with bad `article` or `task` children do
not stop the game (3.4). Whether the shipped game ever gives any of these is not established here.

## 9. The first path

The first level's trader dialog and tutorial (`dialogue-scope` 3) use these portions, all declared in
`info_l01escape.xml` [D6, D6b]:

| Portion | Children | Effect when given (beyond 3.2) |
|---|---|---|
| `esc_trader_can_leave` | none | none |
| `esc_trader_experienced` | none | none |
| `esc_trader_tutorial_chosen` | none | none |
| `tutorial_wounded_start` | `task` `esc_flash_task`, `article` `sl_first_opinions` | to the player character: the task and the article (3.4), both declared |
| `esc_trader_newbie` | none | none |
| `tutorial_end` | none | none |
| `esc_serious_talk` | none | none |

On the player character each of them also calls `inventory_info` (3.4 step 3), through which the shipped
scripts react (`level_tasks`; the trader's door opens on `esc_trader_can_leave`, `trader-door` 4).
`global_dialogs` (6, 7.2) gives the first talk its `actor_break_dialog`.

## 10. Not established

- The moment the index of 1.2 is built.
- When the returning message of 3.1 is applied, and so the exact consequences stated as believed there.
- A call of an `action` function that does not exist (3.3).
- What an empty child (an element with no text) does (1.3).
- The order of dialogs of equal priority (6).
- Whether removing a portion keeps the other entries' order, and whether a holder with no portion left keeps
  an empty saved entry (2).
- Whether any engine code other than that described reads a portion's `dialog`, `precondition`, `has_info`
  or `dont_has_info` children, or a PDA's portion id.

## Provenance

- Date: 2026-11-27. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG installer), data
  as extracted by the dirty workspace's `checks/pda-scope/extract.py` (winning copies of `config\`,
  `scripts\` and `spawns\all.spawn`).
- Data examined: `config\system.ltx` (`[info_portions]`, `[encyclopedia]`, `[dialogs]`), the 32 listed and
  11 unlisted `config\gameplay\info_*.xml`, `config\gameplay\game_tasks.xml` with its includes, the article
  and dialog files, `scripts\*.script`, `spawns\all.spawn`.
- Binaries examined: `bin\xrGame.dll`, `bin\xrXMLParser.dll`, `bin\xrLUA.dll` of the same installation.
- Kinds of analysis: static analysis of the shipped binaries (Ghidra, headless, read-only), string
  searches, and scripted reading of the game data. The game was not run.

## Gaps found in review

- Section 9 omits two portions the trader's start dialog and actor dialogs test: `esc_return` (phrases 12
  and 15 require it not held) and `escape_blockpost_case_have` (the condition of
  `escape_trader_done_blockpost_box`); both are declared in `info_l01escape.xml` with no children.
- What the received-time question (`get_info_time`) answers for a portion the holder does not hold is not
  stated.
