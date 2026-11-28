# The dialogue window: scope and split

Scoping specification for conversations with characters in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the dialog data files and their phrases, the conditions and effects a phrase carries, how a talk
starts and ends, how a dialog proceeds, the window that shows it and its input, the messages and voices
shown or played during a talk, the dialogs built by scripts, and the step from a talk to trade. It lists
every part, the data each part reads, and proposes how to split the area into small specifications, each
covering one format or one behaviour, that can be investigated and reviewed on their own; it states for each
which approved specifications it builds on and which systems not yet specified it waits on, and it points
out the shortest path to the first conversation the game needs: the trader's start dialog in the bunker of
the first level, which gives the information portion `esc_trader_can_leave`. It specifies no behaviour
itself.

It builds on the approved specifications `ui-layout`, `text-and-fonts`, `ltx-config`, `sav`, `db-archive`,
`sound-ogg-params` (`docs/formats/`), `inventory-screen`, `pda-window`, `pda-controls`, `save-load-screens`, `options-screens`,
`console-key-bindings`, `sound-positional`, `hud-scope`, `script-loading`, `script-environment`, `script-api-scope`,
`pda-scope`, `narrowed-specs-scope`, `trader-and-crows-scope`, `trader-door` (`docs/behavior/`), all cited by
name.

Conventions:

- **Named in `xrGame.dll`** means the exact text occurs in the shipped `bin\xrGame.dll` (6,246,400 bytes [F7c],
  the file of `script-environment`) as a printable string of 4 or more characters, or as one `:`-separated
  part of such a string. It shows that the program refers to that file, element, key or message; what it
  does with it is for the proposed spec to establish.
- The functional descriptions (what an element means, who says a phrase, what a list is for) are readings
  of the names in the data and the shipped scripts and are **believed** until the proposed spec
  establishes them; they fix what each spec must cover, not how the part behaves. **Unestablished** marks
  what this scoping work could not settle and leaves to the proposed spec.
- Every fact carries its check, an assertion of `checks/dialogue-scope/checks.py` or `coverage.py` in the
  dirty workspace, cited by label in brackets (e.g. [F2]). `run_all.py` runs both. File paths are archive
  paths of the winning copies (the later archive wins, `db-archive`); dialog and character files are in
  `config\gameplay\`, layout files in `config\ui\`, string tables in `config\text\rus\`. "Script" means a
  file of `scripts\`; calls are counted outside comments, and the shipped API listing
  `scripts\lua_help.script` (`script-api-scope` 2.1) is not counted as a script.
- "Player character" is the character the player controls; "partner" is the character the player
  character talks to.

## 1. Data

### 1.1 Dialog files

- `config\system.ltx` `[dialogs] files` lists 14 names, in this order: `dialogs_test`, `dialogs`,
  `dialogs_escape`, `dialogs_garbage`, `dialogs_agroprom`, `dialogs_deadcity`, `dialogs_darkvalley`,
  `dialogs_pripyat`, `dialogs_bar`, `dialogs_yantar`, `dialogs_military`, `dialogs_labx18`, `dialogs_radar`,
  `dialogs_aes`; each is `config\gameplay\<name>.xml` [F1, F1b]. The key `dialogs` is named [F7].
- `config\gameplay\` has 18 files with "dialog" in the name [F1c]. The four outside the list
  [F1d]: `character_dialogs.xml`, a fragment included by 11 character description files (1.3) [F1g, F1h];
  `dialogs old.xml`, `dialogs_escape_1.xml` and `dialogs_escape_1_string.xml` (a string table of 12
  strings), which no configuration file, script or string of `xrGame.dll` names (believed unused) [F1e,
  F1f].
- The listed files define **190 dialogs**, each id once across the files [F2]. 159 have a `phrase_list`;
  the other 31, all in `dialogs.xml`, have instead an `init_func` naming a script function (the
  **generated dialogs**, 2.6); no other dialog has one [F2b, F2c]. Every phrase list has a phrase of id `0`
  [F2d]. There are **2,296 phrases** [F2e].
- Elements and attributes [F2f, F2g, F2h, F2k]:

  | Where | Element or attribute | Count |
  |---|---|---|
  | dialog | attribute `id` | 190 |
  | dialog | attribute `priority` | 41 (values −5, −3, −2, −1 ×14, 0 ×13, 1 ×2, 2 ×2, 5 ×2, 6, 7, 10 ×3) [F2i] |
  | dialog | `phrase_list` | 159 |
  | dialog | `has_info`, `dont_has_info`, `precondition`, `init_func` | 60, 57, 51, 31 |
  | phrase | attribute `id` | 2,296 |
  | phrase | `next`, `text` | 2,196, 2,188 |
  | phrase | `dont_has_info`, `give_info`, `has_info`, `disable_info` | 603, 414, 371, 5 |
  | phrase | `action`, `precondition` | 304, 83 |

  The attribute `pda` (one dialog, `general_pda_talk`) and the element `script_text` occur only in the
  unlisted `dialogs old.xml` [F2j]. Named in `xrGame.dll`: `dialog`, `phrase_list`, `phrase`, `next`,
  `text`, `precondition`, `action`, `has_info`, `dont_has_info`, `give_info`, `disable_info`, `init_func`,
  `priority`; not named: `script_text` [F7, F7b].
- References inside the listed files:
  - one `next` names a phrase its dialog does not have (`val_escort_nap1_dialog`, phrase 2111 → 412) [F3];
    213 phrases cannot be reached from phrase 0 through `next` [F3b];
  - texts: 108 phrases have no `text` element; of the others 1,827 texts are string ids, 348 are empty and
    13 are neither (the `gar_dolg_bribe_*` and `optional_dialog_test_*` ids, absent from the string table)
    [F3c, F3d];
  - script functions are written `<script>.<function>` [F3g]; distinct functions: 65 named by
    `precondition`, 78 by `action`, 31 by `init_func` [F3e]; 12 are defined by no shipped script: the
    preconditions `bar_dialogs.actor_need_reward`, `bar_dialogs.have_bar_tiran_pda`,
    `bar_dialogs.have_lucky_pda`, the actions `bar_dialogs.give_bar_tiran_pda`, `bar_dialogs.give_lucky_pda`,
    `bar_dialogs.have_borov_reward`, `bar_dialogs.have_svoboda_leader_reward`, `bar_dialogs.have_tiran_reward`,
    `bar_dialogs.have_volkodav_reward`, `escape_dialog.give_wounded_flash_reward`, and the init functions
    `dialog_manager.init_action_info_dialog`, `dialog_manager.init_trade_info_dialog` [F3f];
  - 20 distinct information-portion ids named by `has_info`, `dont_has_info` or `give_info` are declared by
    no file of `[info_portions]` [F3h].
- Strings: the `[string_table]` files of `config\localization.ltx` include `stable_dialogs`, the 12
  `stable_dialogs_<level>` tables and `stable_dialog_manager`, `stable_dialog_manager_uni` [F11e].

### 1.2 The window's layout

`talk.xml` (named in `xrGame.dll`; `talk_16.xml` is its `_16` variant, `ui-layout` 1.1, and has the same
elements) [F6, F6i]. Top elements in order, with their rectangles in the 1024 × 768 space and textures
[F6, F6b, F6c]:

| Element | Rectangle | Texture | Notes |
|---|---|---|---|
| `left_character_icon` | (15, 32) 200 × 218 | `ui_charinfo_left` | believed the player character's panel |
| `right_character_icon` | (820, 32) 200 × 218 | `ui_charinfo_right` | believed the partner's panel |
| `top_background` | (0, 0) 1024 × 128 | `ui\ui_top_background` | |
| `bottom_background` | (0, 736) 1024 × 32 | `ui\ui_bottom_background` | |
| `button` | (437, 726) 157 × 48 | `ui_button_main01` | text `ui_st_trade`, font `graffiti22` [F6d]; believed the step to trade (2.10) |
| `frame_line_window` | (218, 42) 600 × 470 | `ui_talk_dialogue` | vertical, with a `title` [F6e]; believed the answers' frame |
| `frame_line_window` | (218, 485) 600 × 250 | `ui_talk_dialogue2` | vertical, with a `title`; believed the questions' frame |
| `questions_list` | (30, 100) 547 × 120 | — | believed the player character's choices |
| `answers_list` | (33, 46) 544 × 377 | — | believed the log of what was said |

**Disputed in review:** that the rectangles of `questions_list` and `answers_list` are in the 1024 × 768
space is unbacked: they are written as top elements, but each fits inside one of the two frame lines
(547 × 120 inside 600 × 250, 544 × 377 inside 600 × 470); which origin the program gives them is
unestablished (DG-5).

Then the item templates `question_item` (a `content_text` with text and the colours `t`, `e`, `h`) [F6k],
`other_answer_item`, `actor_answer_item` (each `name_caption` and `content_text`), `iconed_answer_item` and
`iconed_trade_info` (each with a `msg_icon` too) [F6j]. The texture ids are defined by `ui_inventory2.xml`
(`ui_charinfo_*`), `ui_numpad.xml` (`ui_talk_dialogue` and `ui_talk_dialogue2` with `_b`, `_back`, `_e`)
and `ui_common.xml` (`ui_button_main01` with `_d`, `_e`, `_h`); the two background pictures exist [F6f,
F6g]. Every element and the attributes `bottom_footer`, `min_height`, `right_ident`, `always_show_scroll`,
`highlight_text` are named, except `iconed_trade_info` [F6h]. Which file gives the character panels their
contents is **unestablished**: `pda-window` 9 found `pda_dialog_character.xml` read only by the PDA's
statistics tab [F12b].

### 1.3 Characters' dialogs

- The character description files of `[profiles] specific_characters_files` define 743 specific
  characters: 603 with a `start_dialog`, 398 with one or more `actor_dialog`s; 48 distinct start dialogs and
  82 distinct actor dialogs [F4, F4b]. The most used are the start dialogs `dm_hello_dialog` (364) and
  `hello_dialog` (305, defined in `dialogs.xml` and again in the unlisted `dialogs old.xml`) and the actor
  dialogs `dm_help_wounded_medkit_dialog` (368) and `dm_cool_info_dialog` (363) [F4d, F4e, F4f];
  `dm_hello_dialog` and the two actor dialogs are those of `character_dialogs.xml` [F1h]. 8 dialog ids named there are defined by no listed file
  [F4c]; 68 listed dialogs are named by no specific character [F4g]. `start_dialog`, `actor_dialog` and
  `hello_dialog` are named in `xrGame.dll` [F7].
- The specific character `actor` has no dialogs [F4l]. The information portion `global_dialogs` is the only
  one of the `[info_portions]` files with a `dialog` child, naming `actor_break_dialog` [F11c, F11i]; the
  player character's binder gives `global_dialogs` when the character does not hold it [F11j].
  `actor_break_dialog` (`dialogs.xml`, priority −5, precondition `dialogs.allow_break_dialog`, false only
  when the partner's profile is `osoznanie`) is one phrase, text `actor_break_dialog_0`, whose action is
  `dialogs.break_dialog` [F11h, F11k]: believed the player character's way to end any talk.
- Scripts change a character's start dialog: 19 logic files set `meet_dialog`, which `xr_meet` passes to
  `set_start_dialog` [F4h]; `sav` 7.4.2 saves a `stringZ` start dialog in the stalker's update [F12c].

### 1.4 Phrase voices

`sounds\characters_voice\dialogs\` holds 62 `.ogg` files [F10]; 60 are named after a phrase text id of the
listed dialogs (`<text id>.ogg`), two are not (`yantar_general_ucheniy_talk_start_1111`, `…_51111111`)
[F10b]. `characters_voice\dialogs\` is named in `xrGame.dll` [F7]; no script names it [F10e]. Believed: the
program plays a phrase's file when the phrase is said. An archive file `sounds\characters_voice\dialogs.rar`
is also stored (believed unused) [F10d].

### 1.5 Keys and interface information portions

- The action `use` (default `kF`, `config\default_controls.ltx`; key-binding entry `kb_use`,
  `options-screens` 11) [F11f, F12h]; `kUSE` is named [F7]. The trader's logic shows the tip
  `tips_esc_trader_how_to_talk`, whose text tells the player to aim at someone and press the use action to
  talk [F11g]: believed the use action on a character starts a talk.
- `info_portions.xml` declares `ui_talk`, `ui_talk_hide` and `ui_trade`, `ui_trade_hide`, each pair
  disabling each other [F11]; all four are named [F7]; no script names `ui_talk` or `ui_talk_hide` [F11b].
  Believed given on opening and closing the window, as `pda-window` 4 and 5 establish for `ui_pda`.
- Messages named in `xrGame.dll`: `No available phrase to say, dialog[%s]` and `ID = [%s] of selected
  question is out of range of available dialogs ` [F7].

### 1.6 The script interface

The shipped API listing has these members (`script-api-scope` R, "Dialogs and trade", and P for
information portions) [F8, F8b, F8c, F12e]:

- of `game_object`: `run_talk_dialog(game_object*)`, `switch_to_talk()`, `stop_talk()`, `is_talking()`,
  `enable_talk()`, `disable_talk()`, `is_talk_enabled()`, `set_start_dialog(string)`, `get_start_dialog()`,
  `restore_default_start_dialog()`, `give_talk_message(string, string, Frect, string)`, `switch_to_trade()`,
  `enable_trade()`, `disable_trade()`, `is_trade_enabled()`, and the constants `dialog_pda_msg` 0,
  `info_pda_msg` 1, `no_pda_msg` 2;
- of the phrase classes: `GetPhraseScript()`, `AddPhrase(string, number, number, number)`,
  `AddHasInfo`, `AddGiveInfo`, `AddDisableInfo`, `AddDontHasInfo`, `AddAction`, `AddPrecondition` (each
  `(string)`);
- callback numbers `trade_start` 0, `trade_stop` 1, `trade_sell_buy_item` 2, `trade_perform_operation` 3;
  window messages `TALK_DIALOG_TRADE_BUTTON_CLICKED` 45, `TALK_DIALOG_QUESTION_CLICKED` 46,
  `TRADE_WND_CLOSED` 52, and `PDA_DIALOG_WND_BACK_BUTTON_CLICKED` 49, `PDA_DIALOG_WND_MESSAGE_BUTTON_CLICKED`
  50 (believed never sent, `pda-window` 9).

Scripts calling them [F9]:

| Member | Scripts |
|---|---|
| `run_talk_dialog` | `xr_effects`, `xr_meet`, `xr_sos`, `xr_zoneguard` |
| `stop_talk` | `dialogs`, `dialogs_aes`, `escape_dialog`, `xr_meet`, `xr_punch` |
| `enable_talk` | `mob_remark`, `mob_trader`, `xr_assistance`, `xr_companion`, `xr_meet`, `xr_sos`, `xr_wounded` |
| `set_start_dialog` / `restore_default_start_dialog` | `xr_meet`, `xr_sos`, `xr_wounded` / `xr_meet` |
| `give_talk_message` | `news_manager`, `task_manager` |
| `AddPhrase` / `AddGiveInfo` | `dialog_manager`, `task_manager` / `dialog_manager` |
| `switch_to_talk`, `switch_to_trade`, `get_start_dialog`, `enable_trade`, `disable_trade`, `is_trade_enabled`, `AddHasInfo`, `AddDontHasInfo`, `AddDisableInfo` | none |

`give_talk_message` is called 6 times by `news_manager` with the template `iconed_answer_item`, each inside
a test that the player character is talking, and once by `task_manager` with `iconed_trade_info` [F9b,
F9c]. `xr_meet` starts a talk from the player character
with its character [F9d]. `dialog_manager` reads `config\misc\dialog_manager.ltx` [F11d].

## 2. Parts

### 2.1 Dialog files (DG-1)

The format of 1.1: the list, the element tree, ids and their scope (dialog ids global, phrase ids per
dialog), `priority` and its default, the text as a string id or literal (and an empty or missing text),
`next`, what the unlisted files and the elements found only there are, and what each kind of broken
reference does (missing `next`, missing text id, missing script function, undeclared information portion,
unreachable phrases): the program's error or silence is for the spec to establish.

### 2.2 Conditions and effects (DG-2)

A dialog's own conditions (`has_info`, `dont_has_info`, `precondition`) decide whether it is offered; a
phrase's decide whether it can be said; a phrase's effects (`give_info`, `disable_info`, `action`) happen
when it is said (believed). The spec establishes the evaluation order and whether it stops at the first
false condition, which speaker is asked whether it holds a portion (the player character, believed, as the
shipped scripts' `has_alife_info` asks the simulation about object 0 [F5r]), the arguments a script
function receives (the shipped functions take a first and a second speaker, some a dialog id), the order
of effects, and what a precondition that does not return a boolean does.

**Disputed in review:** `script-api-scope` 4.5 assigns the arguments, results, timing and failure handling
of the dialog-file functions (`action`, `precondition`, `init_func`) and of `AddAction`, `AddPrecondition`
to its spec R; DG-2 (and DG-9 for `init_func`) can establish them only together with R, or R's assignment
has to change.

### 2.3 Talk start and end (DG-3)

How a talk starts: the use action on a character (1.5), `run_talk_dialog` from scripts (the PDA tutorial's
end, section 3; `xr_meet`), `switch_to_talk`; what a character must be for it (talk enabled: `enable_talk`,
`disable_talk`, `is_talk_enabled`; alive or wounded; not already talking). Which dialogs a talk offers: the
partner's start dialog (`start_dialog`, `set_start_dialog`, `restore_default_start_dialog`,
`get_start_dialog`), the partner's actor dialogs and the player character's own (1.3, the `dialog` children
of held information portions). How it ends: a phrase with no `next`, `stop_talk` (on one or both
speakers), `break_dialog`, the death of either (the window rule of `inventory-screen` 1.2 item 4, cited by
`pda-window` 5 [F12i]), a script disabling talk while talking (`xr_meet` also stops a talk when the
player character is more than 3 m away [F9e]), and what `is_talking` reports on both sides. What is saved: the talk contacts (`sav`
chunk 9 part 3) and the stalker's start dialog (`sav` 7.4.2) [F12c].

### 2.4 Dialog flow (DG-4)

How a dialog proceeds: who says phrase 0 and the phrases after it (believed alternating, with phrases
without text passed over), how the speaker who is not the player chooses among several sayable next
phrases, how the player character's choices are listed (one entry per sayable next phrase, or per offered
dialog before a dialog starts) and ordered (`priority`, file order), what happens after a dialog ends
without ending the talk (back to the list of dialogs; whether a finished dialog is offered again), the two
error messages of 1.5, and a phrase whose every continuation is unsayable.

### 2.5 Window layout (DG-5)

The drawing of `talk.xml` (1.2): the two panels and what they show (name, picture, community, rank,
relation; the panel file, 1.2), the backgrounds and frames, the frame-line titles, the answer log with its
three item kinds (`other_answer_item`, `actor_answer_item`, the iconed ones), names and texts, heights and
footers, the question list with its colours, scrolling of both lists (`always_show_scroll`,
`right_ident`), when the trade button is shown, and the `_16` variant.

### 2.6 Generated dialogs (DG-9)

The 31 dialogs with `init_func` (the `dm_*` and `tm_*` dialogs of `dialogs.xml`): when the function runs,
the phrase-building members (1.6) and the phrases they make, `config\misc\dialog_manager.ltx` and the
`stable_dialog_manager*` strings, the two missing init functions (`dm_trade_info_dialog`,
`dm_action_info_dialog`), and what `dialog_manager` saves (it defines `save(npc, p)` and `load(npc,
reader, ver)`, believed per character [F11l]).

### 2.7 Window opening and input (DG-6)

When the window opens and closes with the talk, the `ui_talk`, `ui_talk_hide` portions, and the input of
the original: a mouse click on a question (message 46), on the trade button (45), the wheel on the lists,
the `quit` key and the menu key while the window is open (`narrowed-specs-scope` SL-3 gives the menu-key
rule and leaves this window's case to its spec [F12d]), `use`, `pause`, the console, keys passed to the
character (as `inventory-screen` 1.3 for its screen), the cursor, the other in-game windows (inventory,
PDA: `pda-window` 6 leaves to this spec what a talk starting while the PDA is open does [F12i]). The
original reads keyboard and mouse only (believed; `inventory-screen` 6.1 states no gamepad support for
its screen [F12g]); how the touch and gamepad paths reach the same choices is a design of the clean side,
not a fact of the original, and this spec gives them only the original's set of choices and events.

### 2.8 Talk messages (DG-7)

`give_talk_message` (text, texture, rectangle, item template) adding iconed lines to the answer log
during a talk (1.6), the `iconed_answer_item` and `iconed_trade_info` templates, whether such lines also
go to the news list (`pda-scope` PD-11) or the diary's `talk_item` (`pda-scope` PD-12), and the meaning of
`dialog_pda_msg`, `info_pda_msg`, `no_pda_msg`.

### 2.9 Phrase voices (DG-8)

When a phrase's voice file (1.4) plays, from where (the speaker's position, `sound-positional`), whether a
new phrase or the end of the talk stops it, and how long it delays the next phrase if at all.

### 2.10 From talk to trade (DG-10)

The dialogue side of trade: when the trade button is shown and usable (`enable_trade`, `disable_trade`,
`is_trade_enabled`, the partner's trade state), what a click does (message 45, `switch_to_trade`), and
returning to the talk when the trade window closes (message 52). The trade screen itself, its callbacks
(`trade_start` … `trade_perform_operation`), `ui_trade`, `ui_trade_hide`, the trade files and prices are
issue #284's.

### 2.11 Boundaries

- **Trade** (`B-trade`, #284): 2.10's last sentence; `can_trade` (item sections), `trade_section`.
- **Characters** (`B-chars`, #279): what the dialogue needs from characters, left to that scope and not
  specified here: (1) which objects are characters that can talk (stalkers, traders; the class `trader`
  of section 3) and their talk-enabled state with its saving; (2) a character's specific character and profile
  (`npc_profile.xml` character → specific character, `sav` 7.4.2 Trader-like block) and, read from it, its
  `start_dialog` and `actor_dialog` lists (the reading of the character files, includes such as
  `character_dialogs.xml` and the undefined ids of 1.3); (3) its name, picture, community, rank,
  reputation and relation to the player character, as the panels show them (the values; their texts are
  `pda-scope` PD-14); (4) whether it is alive or wounded; (5) its behaviour while talking (turning to the
  player character, animations, sounds: for the trader `trader-and-crows-scope` TC3 and TC4 [F12d]); (6)
  its items and money as the dialog scripts ask for them (`object`, `script-api-scope` S).
- **Information portions** (`B-ip`): giving, disabling and holding them, and what a given portion causes
  (articles, tasks, its `dialog` children as a list): `pda-scope` PD-8 [F12f] and `script-api-scope` P.
  The dialogue specs say only when a phrase gives or checks one.
  **Disputed in review:** `pda-scope` PD-8 waits on the dialogue window for the `dialog` child, so it does
  not cover it; within this draft, 2.3 gives the player character's dialogs from held portions to DG-3
  while section 7 gives `dlg:info_portion/dialog` to DG-4.
- **The trader's logic** (`B-trader`): `trader-and-crows-scope` TC4 covers the sections entered by talking
  and trading (`on_info` with `talking`), `can_talk`, the tip; `set_trader_*` and the callbacks
  `trader_global_anim_request` 30, `trader_head_anim_request` 31, `trader_sound_end` 32 [F8e].
- **PDA** (`B-pda`): the diary's `talk_item` (`pda-scope` PD-12).
- **Script windows and others** (`B-other`): `AddDialogToRender`, `RemoveDialogToRender`, `GetDialogWnd`
  (`script-api-scope` G1, G4), `add_precondition` (the action planner, W), `talk` (a sound type and other
  words).
- **The script API** (`script-api-scope` R): the members of 1.6 as scripts call them (arguments, return
  values); the dialogue specs establish what the engine does on each call. P owns the information-portion
  members.

## 3. The trader's start dialog

The trader of the first level is `escape_trader` (`trader-door` 4.2 [F12j]): `npc_profile.xml`'s
character `escape_trader` names the specific character `escape_trader` (class `trader`), whose start dialog
is `escape_trader_start_dialog` and whose actor dialogs are `escape_trader_talk_info`, `escape_trader_jobs`,
`tm_trader_dialog`, `tm_trader_reward`, `escape_trader_done_blockpost_box` [F4i, F4j]; the listing's story
id constant `Escape_Trader` is 3 [F4k]. `trader-door` 4 establishes that the door opens on
`esc_trader_can_leave`, which only phrase 32111 of this dialog gives among the listed dialogs; the portion
is declared in `info_l01escape.xml` [F5i, F5j, F12].

The dialog (`dialogs_escape.xml`, 51 phrases, no dialog-level conditions) [F5]:

- phrase 0 has an empty text and ten continuations, 12, 13, 14, 15, 18, 19, 17, 20, 30, 66, each with
  an empty text and conditions [F5b];
- 12: precondition `escape_dialog.is_tutorial`, not holding `esc_return` nor
  `esc_trader_tutorial_chosen`; next 3 [F5c];
- 3: next 31 and 32 [F5d] (believed the trader's question and the player character's two answers);
- 32 gives `esc_trader_experienced`, `esc_trader_tutorial_chosen`; next 321 [F5e]; then 321 → 3211 →
  32111, which gives **`esc_trader_can_leave`** and `tutorial_wounded_start`, → 321111, whose action is
  `dialogs.break_dialog` and which has no next [F5g];
- 31 gives `esc_trader_newbie`, `esc_trader_tutorial_chosen`, with the actions `dialogs.break_dialog` and
  `escape_dialog.run_tutorial_pda`; next 311 [F5f]. `run_tutorial_pda` stops the talk on both speakers and
  starts the tutorial `part_1_pda` [F5o], whose `function_on_stop` is `xr_effects.esc_init_dialod`, which
  makes the player character run a talk with the object of story id 3 [F5p, F5q]; there 15
  (`escape_dialog.is_tutorial`, holding `esc_trader_tutorial_chosen`, not holding `esc_trader_can_leave`
  nor `esc_return`) leads to 321 and the same end [F5c];
- the texts of 3, 31, 32, 321, 3211, 32111 and 321111 are the string ids
  `escape_trader_start_dialog_<id>`, all in the string table [F5h]; phrases 3 and 321 have voice files
  (and 111, off this path) [F10c].

`escape_dialog.is_tutorial` is true while the player character does not hold `tutorial_end` [F5l].
`dialogs.break_dialog` calls `stop_talk` on both speakers [F5m]. The functions named by phrase 0's
continuations and by the path are 10, all defined: `escape_dialog.is_tutorial`, `is_not_tutorial`,
`trader_alredy_give_job`, `trader_dont_give_job`, `trader_give_extra_job`, `trader_need_extra_job`,
`run_tutorial_pda`, `bar_dialogs.have_darklab_documents`, `hasnt_darklab_documents` (which ask the first
speaker for an item, `object`) and `dialogs.break_dialog` [F5k, F5n]. Off the path, phrase 5212111 of the
same dialog names the undefined `escape_dialog.give_wounded_flash_reward` [F5s]. Whether all ten
continuations of phrase 0 are evaluated, and in which order their conditions are tried, is DG-2's and
DG-4's (it decides which of these functions the first talk calls).

The trader's actor dialogs and their dialog-level conditions [F5t]: `escape_trader_talk_info`
(precondition `escape_dialog.trader_alredy_give_job`, holding `tutorial_end`), `escape_trader_jobs`
(priority 5, holding `esc_serious_talk`), `tm_trader_dialog` (−1, generated, holding `esc_serious_talk`),
`tm_trader_reward` (0, generated, `task_manager.has_active_vendor_task`, holding `esc_serious_talk`),
`escape_trader_done_blockpost_box` (10, `escape_dialog.have_blockpost_case`, holding
`escape_blockpost_case_have`). With `actor_break_dialog` (1.3) these are, believed, what the question list may
offer once the start dialog is over (DG-3, DG-4).

**Path to the first conversation** (the answer 32): DG-1 (reading the dialog and its texts), DG-2 (the
conditions and effects above), DG-3 (the use action on the trader, `stop_talk`, the end of the talk), DG-4
(the trader's start dialog first, the choice between 31 and 32), DG-5 and DG-6 (showing the phrases and
choosing one with every input path). Outside this split it needs: information portions (`pda-scope` PD-8:
holding and giving the portions named above), the script runtime and API (`script-loading`,
`script-environment`; `script-api-scope` R for `stop_talk`, P and K for `has_alife_info`'s query of the
simulation [F8d], S for `object`), the characters scope (#279) for the trader's profile and talk state, and the trader's logic
(`trader-and-crows-scope` TC3, TC4: `can_talk`, the waiting sections in which talking is off at a New
Game, `trader-door` 4.2). The answer 31 needs, in addition, the tutorials (`hud-scope` 18 [F12k]) and the PDA
(`pda-window`). DG-8 (voices), DG-7, DG-9 and DG-10 are not needed for it; the generated `tm_trader_*`
dialogs are, believed, not offered before `esc_serious_talk` is held.

## 4. Proposed specs

Each entry: functional description (section 2); data; builds on (approved); waits on (not yet specified);
scripts; order. Section 7 lists every name each spec owns. "First path" marks the specs of section 3.

**DG-1 Dialog files** (`dialog-files`, format). 2.1. Data: 1.1, the string tables of 1.1. Builds on:
`ltx-config`, `ui-layout` (XML, `#include`), `text-and-fonts` (string table), `db-archive`. Waits on:
nothing. Scripts: none (function names are only read). Order 1. First path.

**DG-5 Talk window layout** (`talk-window`, format and behaviour). 2.5. Data: `talk.xml`, `talk_16.xml`, the
texture descriptions of 1.2. Builds on: `ui-layout`, `text-and-fonts`, `save-load-screens` (buttons),
`pda-controls` (frame lines with titles, lists), `inventory-screen`. Waits on: nothing for the frames and
lists; the panels' values on #279 (B-chars 3) and `pda-scope` PD-14. Scripts: none. Order 2. First path.

**DG-2 Conditions and effects** (`dialog-conditions`, behaviour). 2.2. Data: DG-1's conditions and effects.
Builds on: DG-1, `script-environment`, `script-loading`. Waits on: information portions (`pda-scope` PD-8),
`script-api-scope` P (the queries the shipped functions use) and M (the speaker objects handed to them).
Scripts: the 65 precondition and 78 action functions (1.1); for the first path the 10 of section 3. Order 3.
First path.

**DG-3 Talk start and end** (`talk`, behaviour). 2.3. Data: 1.3, 1.5. Builds on: DG-1, `console-key-bindings`
5 (how the use key reaches the level), `options-screens` 11, `inventory-screen` 1.2, `sav` (7.4.2, chunk 9
part 3). Waits on: #279 (B-chars 1, 2, 4), `script-api-scope` R, information portions (PD-8) for the
`dialog` children. Scripts: `xr_meet`, `xr_sos`, `xr_wounded`, `xr_zoneguard`, `xr_effects`, `dialogs`,
`mob_trader`, `bind_stalker` (`global_dialogs`). Order 4. First path.

**DG-4 Dialog flow** (`dialog-flow`, behaviour). 2.4. Data: DG-1. Builds on: DG-1, DG-2, DG-3. Waits on: what
DG-2 and DG-3 wait on. Scripts: as DG-2. Order 5. First path.

**DG-6 Talk window opening and input** (`talk-window-input`, behaviour). 2.7. Data: `talk.xml`; the
`ui_talk` portions. Builds on: DG-5, DG-3, `console-key-bindings` 5, `save-load-screens`, `pda-window` 5 and
6, `inventory-screen` 1.3 and 6, `narrowed-specs-scope` SL-3 (the rule; this window's case is this spec's).
Waits on: PD-8 for giving the `ui_talk` portions. Scripts: none call the window directly. Order 6. First path.

**DG-7 Talk messages** (`talk-messages`, behaviour). 2.8. Data: the iconed templates of `talk.xml`. Builds
on: DG-5, DG-6. Waits on: `script-api-scope` R (and P for the news half), `pda-scope` PD-11. Scripts:
`news_manager`, `task_manager`. Order 7.

**DG-8 Phrase voices** (`dialog-voices`, behaviour). 2.9. Data: 1.4. Builds on: DG-4, `sound-positional`,
`sound-ogg-params` [F12k]. Waits on: nothing beyond DG-4. Scripts: none. Order 8.

**DG-9 Generated dialogs** (`generated-dialogs`, behaviour). 2.6. Data: the 31 dialogs of `dialogs.xml` with
`init_func`, `config\misc\dialog_manager.ltx`, the `stable_dialog_manager*` strings. Builds on: DG-1, DG-2,
DG-4. Waits on: `script-api-scope` R (the phrase-building members), the player's tasks (`pda-scope` PD-6)
for the `tm_*` dialogs, saving and loading for `dialog_manager`'s saved state. Scripts: `dialog_manager`,
`task_manager`. Order 9.

**DG-10 From talk to trade** (`talk-trade-switch`, behaviour). 2.10. Data: the trade button of `talk.xml`.
Builds on: DG-5, DG-6. Waits on: the trade screen (#284), `script-api-scope` R. Scripts: `mob_trade` and
`bind_stalker` set the trade callbacks (B-trade) [F9f]. Order 10.

## 5. What can be built on approved specs alone

| Spec | Builds on approved specs alone | Waits on |
|---|---|---|
| DG-1 dialog files | yes | — |
| DG-5 window layout | frames, lists, items, button | panels' values: #279, PD-14 |
| DG-2 conditions and effects | the order and the effects' timing | holding and giving portions: PD-8; API P, M |
| DG-3 talk start and end | the use action, start and end rules | which objects talk and their dialog lists: #279; API R |
| DG-4 dialog flow | the flow over DG-1 data | as DG-2, DG-3 |
| DG-6 window input | the original's keyboard and mouse | `ui_talk` portions: PD-8 |
| DG-7 talk messages | the log lines | API R, P; PD-11 |
| DG-8 voices | yes (after DG-4) | — |
| DG-9 generated dialogs | — | API R, PD-6, saving |
| DG-10 talk to trade | the button | #284, API R |

## 6. Unestablished

- What each kind of broken reference of 1.1 does at run time (DG-1, DG-2).
- Who says each phrase, and how the non-player speaker chooses among sayable phrases (DG-4).
- Whether all continuations of phrase 0 of the trader's dialog are evaluated and in what order (section 3).
- Which file fills the window's two character panels (1.2).
- What the 62 voice files are played with and when (1.4, DG-8).
- Whether `ui_talk`, `ui_talk_hide`, `ui_trade`, `ui_trade_hide` are given, and when (1.5).
- The meaning of `dialog_pda_msg`, `info_pda_msg`, `no_pda_msg` (DG-7).
- Every keyboard and mouse input of the window beyond the two messages of 1.6 (DG-6).
- What `dialog_manager` saves (DG-9).

## 7. Coverage

Universe (`coverage.py` V1–V3): 126 names: the 51 strings of `xrGame.dll` that match the pattern of
`coverage.py` (dialog, talk, phrase, condition, question, answer and trade words) and occur as a word in the
shipped configuration or scripts (2 class names left out); the 13 distinct top elements of `talk.xml`
(`<element>@talk`); 20 names of the dialog and character files (`dlg:`); 35 members and constants of the
shipped listing (`api:`, all present); the voice folder (`snd:`); and 6 files (`file:`). Each has exactly one
owner; counts per owner: DG-1 17, DG-2 11, DG-3 25, DG-4 1, DG-5 20, DG-6 5, DG-7 5, DG-8 1, DG-9 14,
DG-10 6, B-trade 9, B-trader 4, B-ip 2, B-pda 1, B-other 5. The table is `checks/dialogue-scope/coverage_table.md`,
reproduced here:

| Owner | Count | Names |
|---|---|---|
| DG-1 | 17 | `dialog`, `dialogs`, `dlg:@id`, `dlg:@pda`, `dlg:@priority`, `dlg:dialog`, `dlg:game_dialogs`, `dlg:next`, `dlg:phrase`, `dlg:phrase_list`, `dlg:script_text`, `dlg:text`, `file:dialogs old.xml`, `file:dialogs_escape_1.xml`, `file:dialogs_escape_1_string.xml`, `phrase`, `phrase_list` |
| DG-2 | 11 | `disable_info`, `dlg:action`, `dlg:disable_info`, `dlg:dont_has_info`, `dlg:give_info`, `dlg:has_info`, `dlg:precondition`, `dont_has_info`, `give_info`, `has_info`, `precondition` |
| DG-3 | 25 | `actor_dialog`, `api:disable_talk`, `api:enable_talk`, `api:get_start_dialog`, `api:is_talk_enabled`, `api:is_talking`, `api:restore_default_start_dialog`, `api:run_talk_dialog`, `api:set_start_dialog`, `api:stop_talk`, `api:switch_to_talk`, `disable_talk`, `dlg:actor_dialog`, `dlg:start_dialog`, `enable_talk`, `file:character_dialogs.xml`, `hello_dialog`, `is_talk_enabled`, `is_talking`, `restore_default_start_dialog`, `run_talk_dialog`, `set_start_dialog`, `start_dialog`, `stop_talk`, `switch_to_talk` |
| DG-4 | 1 | `dlg:info_portion/dialog` (**Disputed in review:** 2.3 puts this in DG-3) |
| DG-5 | 20 | `<actor_answer_item>@talk`, `<answers_list>@talk`, `<bottom_background>@talk`, `<button>@talk`, `<frame_line_window>@talk`, `<iconed_answer_item>@talk`, `<iconed_trade_info>@talk`, `<left_character_icon>@talk`, `<other_answer_item>@talk`, `<question_item>@talk`, `<questions_list>@talk`, `<right_character_icon>@talk`, `<top_background>@talk`, `actor_answer_item`, `answers_list`, `file:talk_16.xml`, `iconed_answer_item`, `other_answer_item`, `question_item`, `questions_list` |
| DG-6 | 5 | `api:PDA_DIALOG_WND_BACK_BUTTON_CLICKED`, `api:PDA_DIALOG_WND_MESSAGE_BUTTON_CLICKED`, `api:TALK_DIALOG_QUESTION_CLICKED`, `ui_talk`, `ui_talk_hide` |
| DG-7 | 5 | `api:dialog_pda_msg`, `api:give_talk_message`, `api:info_pda_msg`, `api:no_pda_msg`, `give_talk_message` |
| DG-8 | 1 | `snd:sounds\characters_voice\dialogs\` |
| DG-9 | 14 | `AddPhrase`, `AddPrecondition`, `GetPhraseScript`, `api:AddAction`, `api:AddDisableInfo`, `api:AddDontHasInfo`, `api:AddGiveInfo`, `api:AddHasInfo`, `api:AddPhrase`, `api:AddPrecondition`, `api:GetPhraseScript`, `dlg:init_func`, `file:dialog_manager.ltx`, `init_func` |
| DG-10 | 6 | `api:TALK_DIALOG_TRADE_BUTTON_CLICKED`, `api:TRADE_WND_CLOSED`, `api:disable_trade`, `api:enable_trade`, `api:is_trade_enabled`, `api:switch_to_trade` |
| B-trade | 9 | `api:trade_perform_operation`, `api:trade_sell_buy_item`, `api:trade_start`, `api:trade_stop`, `can_trade`, `trade`, `trade_section`, `ui_trade`, `ui_trade_hide` |
| B-trader | 4 | `set_trader_global_anim`, `set_trader_head_anim`, `set_trader_sound`, `trader` |
| B-ip | 2 | `disable_info_portion`, `give_info_portion` |
| B-pda | 1 | `talk_item` |
| B-other | 5 | `AddDialogToRender`, `GetDialogWnd`, `RemoveDialogToRender`, `add_precondition`, `talk` |

## Provenance

- Date: 2026-11-27. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binary `bin\xrGame.dll`.
- Data examined: winning copies of `config\` and `scripts\` and the archive listing, as extracted for the
  approved `pda-scope` (`checks/pda-scope/extract.py`); the shipped API listing `scripts\lua_help.script`.
- Analysis: searches of the dialog, character, information-portion, layout, texture-description,
  string-table and logic files and the scripts; a walk of each dialog's phrases; printable-string scans of
  `xrGame.dll`. No disassembly, debugging or execution; the game was not run.
- Checks: `checks/dialogue-scope/checks.py` and `coverage.py`, run with `python3 -I` (`run_all.py` runs
  both), with `lib.py` and the `pda-scope` helpers `tree.py`, `ltx.py`; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- DG-2's waits-on omits `script-api-scope` R (the dialog-file functions as entry points, its 4.5) and S
  with #279 (B-chars 6), which the first path needs for `object` in `have_darklab_documents` and
  `hasnt_darklab_documents`.
- DG-3's waits-on omits the trader's logic (`trader-and-crows-scope` TC4: `can_talk`, the sections in
  which talking is off), which section 3 needs for the first talk.
- No spec owns whether the level, game time and the player character (hits, movement, the weapon and the
  HUD) go on while the window is open.
- No spec owns saving while a talk is open (whether it is possible, and what loading such a save
  restores).
- No spec owns what an information portion's `dialog` child does: `pda-scope` PD-8 leaves it to the
  dialogue window, and this draft leaves it to PD-8.
