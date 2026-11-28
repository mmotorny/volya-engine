# Character descriptions

Format specification CH-1 of the characters scoping specification (`docs/behavior/characters-scope.md`,
sections 1.3, 2.1 and 5, and its first review gap) for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition):
the character profiles of `npc_profile.xml` and the specific characters of the `character_desc_*.xml` files,
which files the game reads and when, their element tree, ids and their lookup, every element and attribute
the program reads with its type and default, the string ids (`name`, `bio`), `money` with `min`, `max` and
`infinitive`, the `supplies` text, the included fragments, the elements found outside the format (the
misspelt ones among them), the program's messages for broken or missing fields, and what reading does with a
start or actor dialog id that no dialog file defines. How a record's profile becomes a specific character
(CH-2) and what each value then does (the specifications named in section 5's table) are not covered here;
this specification gives only what the files hold and how they are read.

It builds on the approved specifications `ltx-config` (configuration files, lists, path aliases),
`ui-layout` (XML documents: sections 1.2 `#include` lines, 1.3 syntax, 1.4 document model, lookups and
values), `text-and-fonts` (string table, section 1) and `db-archive` (winning copies) in `docs/formats/`, and
cites `console-log` (the fatal-error report, sections 3.8 and 3.10) in `docs/behavior/`. None of them is
repeated here.

Conventions:

- Paths are archive paths of the winning copies (`db-archive`); the files are in `config\gameplay\`. "The
  program" is the shipped `bin\xrGame.dll` (with the shipped XML reader `bin\xrXMLParser.dll` for the lookups
  of `ui-layout` 1.4 and the shipped `bin\xrCore.dll` for reading lines).
- **Fatal error** means an error the program reports as `console-log` 3.8 describes, after which it ends as
  `console-log` 3.10 describes. "A failed check with the description *D* and the argument *A*" and "a fatal
  message" are the kinds of that section's table; the `Expression` row is a fixed text of the original
  program and carries nothing needed for compatibility. **Log line** means a line written to the log
  (`console-log`).
- **Not set** for an integer means the value −2,147,483,647.
- "Player character" is the character the player controls. **Named in `xrGame.dll`** is the scope's
  convention (a printable string of 4 or more characters).
- Every fact carries its check, cited by label in brackets: `D`*n* are assertions of
  `checks/character-descriptions/c_data.py` (the game data), `B`*n* and `X`*n* of
  `checks/character-descriptions/c_bin.py` (static analysis of `xrGame.dll`, and of `xrXMLParser.dll` and
  `xrCore.dll`, read from the disassembly and decompilation that `checks/character-descriptions/ghidra.py`
  re-creates). `run_all.py` runs both; all pass. **Believed** marks a reading not checked directly, with why;
  **unestablished** what could not be settled statically.

## 1. Which files are read, and when

- Two lists in `config\system.ltx`, section `[profiles]` (`ltx-config` lists; each item is trimmed of
  surrounding white space) [B1b, B2, B2s]:
  - `files`: the profile files. In this edition one name, `npc_profile` (written `files =  npc_profile`, two
    spaces) [D1, D1b].
  - `specific_characters_files`: the specific-character files. In this edition 15 names, in this order
    [D1]: `character_desc_general`, `character_desc_simulation`, `character_desc_escape`,
    `character_desc_garbage`, `character_desc_agroprom`, `character_desc_darkvalley`,
    `character_desc_deadcity`, `character_desc_bar`, `character_desc_yantar`, `character_desc_military`,
    `character_desc_radar`, `character_desc_pripyat`, `character_desc_aes`, `character_desc_kishka`,
    `character_desc_sarcofag` (one comma has no space after it, which trimming makes irrelevant [D1b]).
- Item *N* is the file `gameplay\`*N*`.xml` under the path alias `$game_config$`, i.e.
  `config\gameplay\`*N*`.xml` [B2, B2s]; all 16 exist [D1]. The wide-screen `_16` rule of `ui-layout` 1.1
  does not apply (the directory is not `ui`).
- The files are read once, when the program starts: the game library's start step registers the profiles,
  then the specific characters (and then the communities of `[game_relations]`, section 5.5) [B1]. Unlike
  the dialog files, this happens also on a dedicated server [B1].
- Each file is opened and parsed whole at that time and its document is kept open for the rest of the run;
  a file that defines no entry is closed again [B2, B2s, B2b]. An entry's contents are read from the kept
  document only when the program first loads that entry's id (section 3.2). The registrations are released
  when the program ends [B12].
- A listed file that cannot be opened is a fatal error: a failed check with the description
  `error while parsing XML file` and the argument *N*`.xml` [B2, B2s]. A file that opens but does not parse
  stops the game inside the XML reader as `ui-layout` 1.3 describes.
- Files not in the lists are never read (section 8).

## 2. Document form

The XML rules are those of `ui-layout` 1.2 to 1.4: `#include` lines are expanded, the lenient syntax of 1.3
is accepted, the root is the first element, element names are compared exactly (case-sensitive), an
attribute is found by exact name, an element's value is its text when its first child is text and otherwise
the reader's default [X3], integers are read as C `atoi` and floats as C `atof` [X3]. Three consequences
matter for these files:

- **Include lines.** Only a line whose first byte is `#` (and which contains `#include`) is an include line;
  the line is taken from its first byte, so a line that starts with tabs or spaces before `#include` is an
  ordinary line and stays in the text [X1] (`ui-layout` 1.2). Section 7 lists what this does in this edition.
- **Element text is condensed.** When an element's text is read, white space at its start and end is dropped
  and every run of white space inside it (space, tab, CR, LF and the other C `isspace` bytes) becomes one
  space [X2]. Line breaks written in the files therefore never reach a value; the `supplies` text uses the
  two characters `\n` instead (section 5.3). Attribute values are taken as written, without trimming [X2b].
- **Empty elements.** An element with no text as its first child (`<icon x="12" y="2"></icon>`, `<bio/>`) has
  no value: the reader's default applies, exactly as for a missing element [X3].

The roots and entries:

- The root element's name is not checked; every listed file uses `<xml>` [D2]. Only the root's children
  named `character` (profile files) or `specific_character` (specific-character files) are entries; they are
  counted and taken in document order [B2, B2s]. Any other child of the root, text included, is ignored. In
  this edition every child element of the roots is an entry [D2]; `npc_profile.xml`'s root also holds stray
  text lines such as `;-----Quest Npc---------` [D2d].
- Entries per file: 474 profiles in `npc_profile.xml`; specific characters, in list order, 4, 251, 33, 23,
  24, 49, 20, 80, 26, 26, 68, 28, 13, 79, 19 (743 in all) [D2b].
- `npc_profile.xml` declares `encoding="UTF-8"` (its only non-ASCII bytes are in comments); the 15 others
  declare `windows-1251` [D2c]. The declaration does not change decoding (`ui-layout` 1.3).

## 3. Ids

### 3.1 Registration at start

- An entry's id is its attribute `id`. Each list forms **one id space** of its own: profile ids are
  registered from the `files` list, specific-character ids from the `specific_characters_files` list, in
  list order and, within a file, in document order [B2, B2s, B2b]. Each id is kept with its **registration
  index** (0, 1, 2, … across the list), its file and the entry's position among that file's entries [B2b].
  The program can also take entries by registration index, and knows the last index (the choice among
  specific characters is CH-2's) [B4c].
- An entry without an `id` attribute is a fatal error: a failed check with the description
  `id for item don't set, number` *n* `in` *N*, where *n* is the entry's 0-based position among the file's
  entries and *N* the list item (without `.xml`) [B2, B2s].
- An id that an earlier entry of any file of the same list already has is a fatal error: a failed check with
  the description `duplicate item id` and the id as argument [B2, B2s, B2b]. So a file listed twice is a fatal
  error.
- In this edition the 474 profile ids and the 743 specific-character ids are unique within their lists, all
  letters, digits and `_`; 55 profile ids and 51 specific-character ids contain capitals [D4, D4b]. The two
  spaces are separate: 353 ids are both a profile's and a specific character's (e.g. `escape_trader`) [D4c].

### 3.2 Loading an entry

- The program loads a profile or a specific character by id. The id is looked up exactly (byte for byte)
  [B3, B3s]. An id that its list does not define writes one log line `[`*n*`]=[`*id*`]` for every registered
  id of that list (*n* from 0, in registration order) and then is a fatal error: a failed check with the
  description `item not found, id` and the looked-up id as argument [B3, B3s]. Loading an empty id is a failed
  check as well [B4].
- A found entry is read from its kept document: the *n*-th entry of its file's root, *n* being its position
  [B5, B6]. Its contents are read the first time its id is loaded and then shared by every later load of that
  id until the program ends [B4, B4b]. Errors in an entry's contents (sections 4 and 5) therefore appear when
  it is first loaded, not at start.
- Which ids the program loads, and when, is CH-2's.

## 4. A profile (`character`)

| Item | Read as | Default | Meaning |
|---|---|---|---|
| attribute `id` | text | none (fatal, 3.1) | the profile's id |
| element `specific_character` | text | none | the id of the specific character this profile always gives (CH-2) |
| element `class` | text, lower-cased | none | read only when there is no `specific_character`: **believed** the class among whose specific characters CH-2 chooses |
| element `rank` | integer | not set | read only when there is no `specific_character` (CH-2) |
| element `reputation` | integer | not set | read only when there is no `specific_character` (CH-2) |

Sources: [B5]. When `specific_character` has a value, nothing else of the profile is read: its `class`,
`rank` and `reputation` have no effect [B5].

In this edition [D5, D5b, D5c]: every profile has a `class` and 18 also a `specific_character`; no profile has
`rank`, `reputation`, another child, another attribute, or a child twice. The 18 `specific_character` values
all name defined specific characters, 16 of them the profile's own id; their `class` values (`Trader` for
`escape_trader`, `Actor` for `actor`, …) are not read. Of the 456 profiles whose class is read, 92 write it
with capitals (it is lower-cased); two classes, `esc_plemiannik` and `gar_newbie_in_tonnel`, are the class of
no specific character, ignoring case (what that does is CH-2's).

## 5. A specific character (`specific_character`)

### 5.1 Attributes

| Attribute | Read as | Default | Meaning |
|---|---|---|---|
| `id` | text | none (fatal, 3.1) | the specific character's id |
| `no_random` | integer; the flag is on only when it equals 1 | off | **believed** never taken by a random choice (CH-2) |
| `team_default` | integer; the flag is on only when it equals 1 | off | **believed** the default specific character of its class (CH-2) |

Both flags on is a fatal error: a failed check with the description
`cannot set 'no_random' and 'team_default' flags simultaneously, profile id` and the specific character's id
as argument (the description says "profile", the argument is the specific character's) [B6].

In this edition [D6, D6b]: `team_default` is 1 in 738 specific characters and 3 in one
(`val_monolith_regular_default1`, whose flag is therefore off); `no_random` is 1 in four: `actor`, `mp_actor`,
`escape_trader`, `bar_barman`; none has both, and no entry has another attribute. They are written with
spaces around `=` (`no_random = "1"`, `team_default = "1"` 690 times) or without (`team_default="1"` 48
times), which the lenient syntax accepts.

### 5.2 Elements

The elements are read in the order of the table; the checks of required fields fail in that order [B7, B7d,
B7e]. "First" means the first child of that name in document order; any further child of that name is ignored,
except where stated.

| Element | Read as | Default | Meaning (owner) |
|---|---|---|---|
| `start_dialog` | text (first) | empty | the character's start dialog id (section 6; CH-2, CH-7, DG-3) |
| `actor_dialog` | text, **every** child in document order, each its own entry of a list | empty text for an element without value | the dialogs the player character can start with this character (section 6) |
| `icon` | text (first) | `ui_npc_u_barman` | **believed** the texture id of the character's picture (CH-2) |
| `name` | text (first), kept as written | empty | the character's name: a string id or text (5.4; CH-2) |
| `bio` | text (first), passed at once through the string table's lookup (`text-and-fonts` 1.4) and kept translated | empty | the character's biography text (5.4) |
| `panic_threshold` | float (first) | 0 | CH-18 |
| `hit_probability_factor` | float (first) | 1 | CH-20 |
| `crouch_type` | integer (first) | 0 | CH-11 |
| `critical_wound_weights` | text (first) | `1` | CH-21 |
| `visual` | text (first) | empty | **believed** the model (`meshes\<visual>.ogf`; CH-2) |
| `supplies` | text (first), then every `\n` replaced (5.3) | empty | the starting items (CH-6) |
| `npc_config` | text (first) | empty | **unestablished**: no file of this edition has it |
| `snd_config` | text (first) | empty | **believed** the voice folder under `sounds\` (CH-25) |
| `terrain_sect` | text (first) | empty | **believed** a section naming where the character may walk (CH-9) |
| `class` | text, lower-cased, once per `class` child (below) | no entry | the classes this specific character belongs to (CH-2) |
| `community` | text (first), lower-cased | **required** | the community (5.5; CH-3) |
| `rank` | integer (first) | **required** | the rank (CH-3) |
| `reputation` | integer (first) | **required** | the reputation (CH-3) |
| `money` | the attributes of the first `money` element (5.6) | min 0, max 0, infinitive off | starting money (CH-6) |

Sources: [B7, B7b, B7c, B7d, B7e, B9]. Notes:

- `class`: the program counts the `class` children and adds one class per child, but each time it reads the
  value of the **first** `class` child, so *k* `class` children give the first class *k* times; a `class`
  without value adds an empty class [B7c]. In this edition no specific character has more than one `class`
  [D7b].
- Every element and attribute name of the table and of 5.1 that has 4 or more characters is named in
  `xrGame.dll`; `bio`, `id`, `min`, `max` are shorter, and are read all the same [B10b, B7, B7e].
- Nothing else of a specific character is read: no other element, no attribute of any element other than
  those of `money` (5.6), and no text directly inside the `specific_character` element [B10, B7].

### 5.3 The `supplies` text

After condensing (section 2), a non-empty `supplies` text has every occurrence of the two characters `\`
`n` replaced by one line feed (byte 10); the search goes on after each replacement, so all are replaced
[B7b]. The result is kept as one text; what it lists and how it is read are CH-6's.

Example: `escape_trader`'s element

```
		<supplies>
			[spawn] \n
			wpn_knife \n
		</supplies>
```

condenses to `[spawn] \n wpn_knife \n` [D11f] and is kept as `[spawn] ` LF ` wpn_knife ` LF.

In this edition [D11f, D7d]: 118 `supplies` elements are empty (`<supplies></supplies>` or similar, no value);
the other 625 all begin with `[spawn]` and use `\n`. Every line of the included fragments `character_items`,
`character_food`, `character_drugs` (section 7) has the form *section* `= 1, prob=0.`*N* ` \n` (`prob`
**believed** a spawn probability, CH-6) [D3d].

### 5.4 Texts: `name` and `bio`

- `name` is kept exactly as read; it is not looked up when read [B7]. In this edition [D10]: 520 names begin
  `GENERATE_NAME_` (`stalker` 344, `sergeant` 96, `bandit` 39, `private` 17, `lieutenant` 17, `captain` 5,
  `science` 2; `GENERATE_NAME_` is named in `xrGame.dll`, its handling is CH-2's), 115 are the text `Test`,
  and the other 108 are string ids. How the name is shown (and whether it is translated then) is CH-2's.
- `bio` is translated when read: its value goes through the string table's lookup and the result is kept
  [B7, B9]. So a string id gives its string, other text gives itself, and an empty or missing `bio` gives the
  empty text (`text-and-fonts` 1.4). In this edition [D10b]: `bio` is missing in `actor` and `mp_actor`, empty
  in 115, a string id in 613, and other text in 13: 11 Russian sentences (kept as written) and 2 id-shaped
  texts that the table does not define, `val_soldier_regular_bio.` and `yan_kruglov_bio` (kept as written).

### 5.5 `community`, `rank`, `reputation`

- `community` without a value is a fatal error: a failed check with the description
  `'community' field not fulfiled for specific character` and the specific character's id as argument [B7d].
- Otherwise the text is lower-cased and looked up, comparing texts byte by byte, among the communities
  of `[game_relations] communities` of the system configuration (`config\creatures\game_relations.ltx`,
  included by `system.ltx`; the list's form is CH-3's) [B8, D8]. A text not found is a fatal error: a failed
  check with the description `item not found, id` and the lower-cased text as argument; the fatal message
  `wrong 'community' '`*text*`' in specific character `*id*` ` follows it in the program, and is reported only
  if the first report returns (`console-log` 3.10) [B8, B7d].
- `rank` and `reputation` are integers whose default is "not set"; a missing one (or one whose value reads as
  −2,147,483,647) is a fatal error: a failed check with the description
  `'rank' field not fulfiled for specific character`, respectively `'reputation' field not fulfiled for
  specific character`, and the id as argument [B7d, B7f]. A value that is not a number reads as `atoi` does
  (`abc` → 0) and passes.
- In this edition [D8, D8b]: every specific character has the three; the 13 communities used are all lower
  case and all in the list (`stalker` 334, `monolith` 87, `bandit` 68, `military` 57, `zombied` 53, `dolg` 49,
  `killer` 33, `freedom` 23, `arena_enemy` 18, `ecolog` 15, `actor` 2, `actor_dolg` 2, `trader` 2); ranks are
  decimal integers from 0 to 1000 (the actor's written `00`), reputations from −1000 to 198. What the values
  mean (rank and reputation names, relations) is CH-3's.

### 5.6 `money`

- No `money` element: min 0, max 0, infinitive off [B7e].
- Otherwise the first `money` element's attributes: `min` and `max`, integers with default 0, and
  `infinitive`, on when its integer value is not 0 (default off). Then, comparing both as unsigned 32-bit
  numbers, a `max` lower than `min` is raised to `min` [B7e]. The element's own text is not read.
- Meaning (**believed**, CH-6): the character starts with an amount between `min` and `max`; `infinitive` on
  means its money never runs out (the trader's).
- In this edition [D9, D9b]: 226 specific characters have `money`, always with the three attributes, as
  (`min`, `max`, `infinitive`): (600, 2000, 0) 112, (200, 600, 0) 47, (2000, 5000, 0) 40, (5000, 10000, 0) 22,
  (100000, 110000, 1) 3, (100000, 100000, 1) 1, (0, 200, 0) 1; `max` is never below `min`; `infinitive` is 1
  in `bar_barman`, `bar_dolg_petrenko`, `escape_trader` and `yan_general_ecolog`.

### 5.7 Values in this edition

Children of the 743 specific characters (number having each) [D7]: `name`, `icon`, `class`, `community`,
`rank`, `reputation`, `visual`, `supplies` 743; `bio` 741; `snd_config` 735; `map_icon` 710; `terrain_sect`
652; `start_dialog` 600; `critical_wound_weights` 520; `crouch_type` 513; `actor_dialog` 394; `money` 226;
`team` 40; `panic_threshold` 20; `panic_treshold` 7; `terrain_sect_` 1; `hit_probability_factor` 1. No child
has children; attributes occur only on `money` (`min`, `max`, `infinitive`), `map_icon` (`x`, `y`) and 98
`icon` elements (`x`, `y`) [D7c]. Repeated children: `actor_dialog` in 375 characters, `start_dialog` in 113,
`terrain_sect` in 19, `critical_wound_weights` in one [D7b].

- `icon`: 98 elements (79 in `character_desc_kishka.xml`, 19 in `character_desc_deadcity.xml`) are empty
  with only `x` and `y` attributes, e.g. `<icon x="12" y="2"></icon>`, so these characters get the default
  `ui_npc_u_barman`; the other 645 have a value and no attribute; 113 distinct values, `ui_npc_u_barman`
  among them (written by 3) [D10c, D10d].
- `crouch_type`: −1 in 236, 0 in 206, 1 in 71, missing (0) in 230 [D11].
- `panic_threshold`: `0.0` in 20; `hit_probability_factor`: `1.5` in one [D11].
- `critical_wound_weights` (first): `55,30,15` in 519, `75,25,25` in one (`sim_soldier_specnaz_default2`,
  which has `55,30,15` as a second, ignored one), missing (`1`) in 223 [D11b].
- `terrain_sect` (first): one of 8 sections, `stalker_terrain` 267, `stalker_military_terrain` 137,
  `stalker_bandit_terrain` 74, `stalker_zombied_terrain` 53, `stalker_dolg_terrain` 50,
  `stalker_killer_terrain` 33, `stalker_freedom_terrain` 23, `stalker_ecolog_terrain` 15; missing in 91; the
  19 repeated ones repeat the same value [D11c].
- `snd_config`: 32 distinct values, all `characters_voice\…\` folders; 78 are written
  `characters_voice\human_01\ military\` with a space, which condensing keeps; missing in 8 [D11d].
- `visual`: 119 distinct values; 118 are models `meshes\<visual>.ogf` of the archives, one is not:
  `actors\stalker_zombi\stalker_zombie_8` (`ds_zomb_12`); what that does is CH-2's [D11e].

## 6. Dialog ids

- Reading a specific character keeps its start dialog id and its actor dialog ids as text. Nothing looks them
  up while the specific character is read: an id that no dialog file defines is kept like any other, without
  a message [B11].
- The ids are used as dialog ids later, when the dialogs of a talk are collected (when that happens is
  DG-3's): the player character's collection takes each actor dialog id of the partner's specific character,
  in order, and a non-player character's collection adds its own start dialog (when it has one) and then
  `hello_dialog` [B11b, B11d] (that a non-player character's start dialog is the one read here is
  **believed**: CH-2 and CH-7 carry it there). Each id not already among the offered dialogs is loaded as a
  dialog by id [B11c]. For an id that no listed dialog file (`[dialogs] files`) defines, that load writes one
  log line `[`*n*`]=[`*id*`]` for every registered dialog and then is a fatal error: a failed check with the
  description `item not found, id` and the id as argument [B11c].
- In this edition 8 ids named by specific characters are defined by no dialog file [D12c]: the start dialog
  `gar_wounded_bandit` of `gar_bandit_novice_wounded`; the start dialog `bar_bar_psih_start_pain` and the actor
  dialogs `bar_bar_psih_talk_pain`, `bar_bar_psih_give_medkit`, `bar_bar_psih_talk_gorbun`,
  `bar_bar_psih_talk_lisiy` of `bar_psih`; the start dialog `bar_svoboda_volkodav_start` and the actor dialog
  `bar_svoboda_volkodav_info_dp` of `bar_svoboda_attack_commander`. Each of these start dialogs is its
  character's only `start_dialog`. So reading these three characters succeeds; collecting their dialogs for a
  talk is the fatal error above. **Disputed in review:** established only for `bar_psih` and
  `bar_svoboda_attack_commander`, through their undefined actor dialogs; for `gar_bandit_novice_wounded`, whose
  only undefined id is its start dialog, it rests on the believed statement above that the start dialog read
  here is the one collected, and the game's scripts do set other start dialogs on characters (CH-7), so it is
  believed, not established.
- Start dialogs in this edition (first `start_dialog`) [D12]: none in 143 characters (empty), `hello_dialog`
  295, `dm_hello_dialog` 257, 48 distinct ids. 113 characters have two `start_dialog` elements: 86
  `hello_dialog` before the `dm_hello_dialog` of the fragment `character_dialogs.xml` (so `hello_dialog` is
  theirs), 10 the reverse, 17 their own before `dm_hello_dialog` (so their own is theirs).
- Actor dialogs [D12b, D12d]: 82 distinct ids, the most used `dm_help_wounded_medkit_dialog` (364) and
  `dm_cool_info_dialog` (359); no character's list repeats an id; the longest list has 14; no
  `start_dialog` or `actor_dialog` element is empty.

## 7. Included fragments

The include lines of the listed files name 10 fragments in `config\gameplay\`; each is bare (no
declaration, no root, no include line of its own) [D3d]:

| Fragment | Contents | Expanded include lines, where |
|---|---|---|
| `character_criticals_1.xml` … `_6.xml` | each the same single element `<critical_wound_weights>55,30,15</critical_wound_weights>` | 141, 41, 94, 124, 55, 65, directly in a specific character |
| `character_dialogs.xml` | `<start_dialog>dm_hello_dialog</start_dialog>`, `<actor_dialog>dm_cool_info_dialog</actor_dialog>`, `<actor_dialog>dm_help_wounded_medkit_dialog</actor_dialog>` | 348, directly in a specific character |
| `character_items.xml` | 3 lines *section* `= 1, prob=0.`*N* `\n` | 195, inside `supplies` |
| `character_food.xml` | 4 such lines | 407, inside `supplies` |
| `character_drugs.xml` | 2 such lines (`medkit = 1, prob=0.2 \n`, `bandage = 1, prob=0.4 \n`) | 478, inside `supplies` |

Sources: [D3, D3d, D3f]; 1,948 expanded include lines in all. An include line is replaced whole, so text after
the closing quote on that line (` \n` on 302 of them) is dropped; the fragment's own lines carry their `\n`.

**Lines that are not include lines.** 19 further lines of the listed files have `#include` after leading tabs
or spaces, so they stay text (section 2) [D3b, D3c, X1]:

- 14 inside a `supplies` text (11 naming `character_items`, 2 `character_food`, 1 `character_drugs`), in 12
  specific characters (`agr_krot`, `sim_stalker_general_default1`, four `aes_Monolit_…` and six
  `sar2_monolith_…` ones): their condensed `supplies` text contains the words `#include "gameplay\character_…"`
  where the fragment's lines would have been. What CH-6's reading of the text makes of them is CH-6's.
- 5 directly inside a specific character (4 naming `character_dialogs`, 1 `character_criticals_4`), in
  `sim_stalker_general_default1`, `bar_dolg_ivancov`, `mil_killer_sniper`, `mil_killer_master`: they are text
  of the `specific_character` element, which is not read, so these characters do not get the fragment's
  dialogs or weights.
- One more `#include` is inside a comment (`character_desc_deadcity.xml`) and has no effect [D3b].

Because of this rule, 600 specific characters have a `start_dialog`, 394 an `actor_dialog` and 520 a
`critical_wound_weights`; `characters-scope` 1.3 counted 603, 398 and 521, expanding the indented lines too
[D3e].

## 8. Elements and files outside the format

- Elements of specific characters that the program does not read (section 5.2) [B10, B10b, D13]:
  - `team` (40 characters, 16 values such as `Svoboda_specnaz`, `Bandit_master`): not read by this reader
    (the name `team` is used only by other parts of the program).
  - `map_icon` (710 characters, always empty with `x` and `y`): not read; not named in `xrGame.dll`.
  - the attributes `x`, `y` of `icon` (98): not read.
  - the misspellings `panic_treshold` (7 characters, value `0`) and `terrain_sect_` (one, `stalker_terrain`):
    not read, so these characters keep the defaults of `panic_threshold` (0) and `terrain_sect` (empty)
    unless they also have the correct element. `panic_treshold` is not named in `xrGame.dll`.
  - stray text directly in a `specific_character` element (section 7).
- Profile `class`, `rank`, `reputation` next to a `specific_character` (section 4).
- Files: `config\gameplay\` has 26 `character_desc_*.xml` files; 11 are not listed: `character_desc_aes2`,
  `_bandit`, `_dar`, `_dolg`, `_ecolog`, `_killer`, `_monolit`, `_soldier`, `_stalker`, `_svoboda`,
  `_zombied`, with 150 specific characters, 4 of `character_desc_dar.xml`'s with ids of listed ones; no
  configuration file, script or string of `xrGame.dll` names them, so they have no effect (if listed, the four
  would be fatal duplicates, 3.1) [D13b]. The fragments of section 7 are read only through include lines.

## 9. Worked example: the trader of the first level

The profile `escape_trader` (`npc_profile.xml`) and the specific character `escape_trader` (the first entry of
`character_desc_escape.xml`) use [D14, D14b, D14c, D14d]:

- Profile: `id`; `class` `Trader` (not read, section 4); `specific_character` `escape_trader`, which is
  therefore the specific character this profile gives (CH-2).
- Specific character, attributes: `id` `escape_trader`; `no_random = "1"` (flag on); no `team_default` (off).
- Its children, in document order, and what reading makes of them:

  | Element | Written | Read |
  |---|---|---|
  | `name` | `escape_trader_name` | kept as written (a string id; the string table gives Сидорович) |
  | `icon` | `ui_npc_u_trader` | `ui_npc_u_trader` |
  | `bio` | `escape_trader_bio` | translated: the string table's text for `escape_trader_bio` |
  | `class` | `trader` | one class, `trader` |
  | `community` | `trader` | `trader`, found in the community list |
  | `visual` | `actors\trader\trader` | as written |
  | `rank` | `330` | 330 |
  | `reputation` | `23` | 23 |
  | `money` | `min="100000" max="100000" infinitive="1"` | min 100,000, max 100,000, infinitive on |
  | `supplies` | `[spawn] \n` / `wpn_knife \n` on two lines | `[spawn] ` LF ` wpn_knife ` LF (5.3) |
  | `start_dialog` | `escape_trader_start_dialog` | start dialog `escape_trader_start_dialog` |
  | `actor_dialog` × 5 | `escape_trader_talk_info`, `escape_trader_jobs`, `tm_trader_dialog`, `tm_trader_reward`, `escape_trader_done_blockpost_box` | the actor dialog list, in this order |

- Not written, so defaults: `snd_config`, `npc_config`, `terrain_sect` empty; `crouch_type` 0;
  `panic_threshold` 0; `hit_probability_factor` 1; `critical_wound_weights` `1`. No include line, no stray
  text, nothing outside the read set.
- No fatal error can arise: the three required fields are present, `trader` is a community, the flags are not
  both on, and every dialog id it names is defined by a dialog file [D14c].

So every element and attribute this example uses is covered by sections 4 to 6.

## 10. Unestablished

- What the program does with `npc_config` (read and kept; no file of this edition has it).
- What follows the fatal error of an unknown community: whether the `wrong 'community' …` message is ever
  reported (section 5.5; it follows the first report only if that returns).
- What the believed meanings of section 5.2 are (each named owner establishes it).
- A file that repeats an attribute on one element: none occurs; what the reader keeps is not established.
- That a non-player character's start dialog in a talk is the one read here (section 6; believed).
- Everything here is static: nothing was observed in the running game.

## Provenance

- Date: 2026-11-27. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\xrXMLParser.dll` and `bin\xrCore.dll`.
- Data examined: the winning copies of `config\` as extracted for the approved `pda-scope`, with its archive
  listing; the profile, specific-character, fragment, dialog, string-table and relation files.
- Analysis: static analysis (disassembly and decompilation with Ghidra, headless and read-only) of the
  shipped game library, XML reader and core library; reading constants from them; printable-string scans of
  `xrGame.dll`; reading the XML files with a reader written from `ui-layout` 1.2–1.4 and the facts of
  section 2. No program was executed; the game was not run.
- Checks: `checks/character-descriptions/c_data.py` and `c_bin.py`, run with `python3 -I` (`run_all.py` runs
  both), with the `pda-scope` and `dialog-files` helpers; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What a profile with neither a `specific_character` value nor a `class` value gives: section 4 states the
  `class` default only as "none", not what the program keeps for it or whether it fails (no profile of this
  edition has this form).
- Which characters lower-casing changes in `class` and `community` texts (only the ASCII letters `A` to `Z`, or
  also Windows-1251 letters) is not stated.
