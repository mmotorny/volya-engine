# Character identity and profile choice

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of how a character's
identity is filled in: which specific character it gets from its profile (the profile's own specific
character, or a random choice among the specific characters of the profile's class), when that choice is
made, and what the chosen specific character then fills in: the specific character, community, rank,
reputation and name of the Trader-like block, the money, the model, the team, the terrain masks and the PDA's specific character;
and what the `community`, `rank` and `spec_rank` keys of the character and monster sections give. It is
CH-2 of `docs/behavior/characters-scope.md` (cited as `scope`), section 5.

Builds on (not repeated): `docs/formats/sav.md` (cited as `sav`; 7.4 the Trader-like, Creature and
Visual blocks and the layouts per kind, 9 chunk 9, 11.2 the load order), `docs/behavior/offline-simulation.md`
(cited as `offline`; 8 the creation and registration order, 7.4 and 1.6 the terrain masks),
`docs/behavior/object-visual-and-transform.md` (cited as `objects`; 2 the model name), `docs/formats/text-and-fonts.md`
1.4 (string-table lookup), `docs/behavior/original-determinism.md` 2.2 (the shared random generator),
`docs/behavior/console-log.md` (the report of a fatal error), `docs/formats/ltx-config.md`.

The format of the profiles and specific characters is CH-1, `docs/formats/character-descriptions.md` (cited
as CH-1). This specification uses its terms and does not specify the format; section 1.2 lists exactly what
it takes from CH-1, and every statement resting on that ground is marked **(CH-1)**.

Conventions:

- **Character** and **character record** are `scope`'s (class ids `AI_STL_S`, `AI_STL`, `AI_TRD_S`; the
  records of `spawns\all.spawn` with them). A character's **object** is its simulation object, whose saved
  state is `sav` 7.4; the **online character** is its counterpart on the loaded level. **Stalker** and
  **trader** are `sav` 7.4.2's kinds.
- **Profile** and **specific character** are CH-1's: a `<character>` of `config\gameplay\npc_profile.xml`
  and a `<specific_character>` entry of the files of `[profiles] specific_characters_files` (`scope` 1.3).
  The **specific character of an object** is the id of such an entry; what it gives is that entry's.
- **Unset**: an empty specific character or name, community −1, rank or reputation −2,147,483,647.
- **Draw** means one draw of the shared random generator (`original-determinism.md` 2.2): the state *s*
  becomes *s* · 214,013 + 2,531,011 (mod 2³²) and the draw is (*s* ≫ 16) AND 32,767 [B11]; in the
  single-player game nothing in `xrGame.dll` sets the state (its only write there is in a multiplayer game
  mode) [B12]. "*a* mod *n*" is the remainder of the draw *a* by *n*.
- **Text of** an id is its string-table lookup (`text-and-fonts.md` 1.4: a missing id gives the id itself).
- An **assertion failure** ends the game with the error report of `console-log.md` (believed, as in the
  other specifications: the game was not run).
- Statements are static (read from the data and the shipped `bin\xrGame.dll`); the game was not run.
  Inferred statements are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed labels are the checks reproducing a statement: assertions of `checks.py` in the dirty
  workspace's `checks/character-identity/` (`python3 -I checks.py`; `export.py` re-creates their evidence).

## 1. The values involved

### 1.1 On the object

| Value | Where it is kept | Unset / initial value |
|---|---|---|
| specific character | Trader-like block | empty |
| character profile | Trader-like block | the section's `character_profile`, else `default` |
| community | Trader-like block (a position in the community list, 1.3) | −1 |
| rank, reputation | Trader-like block | −2,147,483,647 each |
| character name | Trader-like block | empty |
| money | Trader-like block | the section's `money`, else 0 |
| model name | Visual block (`objects` 2) | — |
| team | Creature block, first byte (stalkers only) | — |
| terrain masks | not saved (`offline` 7.4) | from the section's `terrain` |

A new object starts with the initial values of the table before its state is read [W3]. Every character
record of `spawns\all.spawn` leaves the specific character, name, community, rank and reputation unset
(`scope` 1.1) and names a profile (205 distinct ones) [D4].

### 1.2 From the specific characters (CH-1)

What this specification takes from the specific-character files, as the program reads them (CH-1 owns the
reading, its messages and the elements not listed here):

- **Profile**: if it has a `specific_character` element, that id, and nothing else is used; otherwise its
  `class` (lower-cased) and its `rank` and `reputation` (unset when absent) [B3]. No profile of the shipped
  `npc_profile.xml` has `rank` or `reputation` [D6].
- **Registration index** (CH-1 3.1): the specific characters are numbered 0 … 742 in the order of the files
  in `specific_characters_files` and, within a file, of the `specific_character` children of its root
  element [D1].
- **Per specific character**: its id; **no_random** and **team_default**, each set only when the attribute's
  value is exactly 1 (both set is an assertion failure); its **class list**, lower-cased (each `class` element adds an
  entry; the reader takes the text of the first `class` element for every entry, so the list holds the first
  class repeated: **(CH-1)**, immaterial for the shipped data, where every specific character has exactly one
  `class` [D3]); its **community** as the position of the `community` text in the community list (1.3); its
  `rank` and `reputation`; `name` (a string id); `visual`; `terrain_sect`; `icon`; and `money` `min` and `max`
  (0 when absent; a `max` below `min` is read as `min`) [B4, B5, B6, S3, S8].

### 1.3 The community list

`[game_relations] communities` of `config\creatures\game_relations.ltx` is a list of name, number pairs; a
community's **position** is the 0-based index of its pair, its **number** the number, kept as a byte
[S3]. The shipped list has 15 pairs: `actor` 0, `actor_dolg` 1, `actor_freedom` 2, `stalker` 5, `monolith` 6,
`military` 7, `killer` 8, `ecolog` 9, `dolg` 10, `freedom` 11, `bandit` 12, `zombied` 13, `stranger` 14,
`trader` 15, `arena_enemy` 16; so `trader` has position 13 and number 15 [D10].

## 2. The choice

The **choice** returns an object's specific character, making one first if it is unset. Its steps:

1. **Multiplayer**: if a level is loaded and its game type is not single player (the game type of `single`
   is 1), the current value is returned and nothing else is done [B1]. Multiplayer is outside this
   specification; everything below is the single-player game.
2. **Already set**: a non-empty specific character is returned unchanged; nothing else is done [B2].
3. **Explicit**: the object's profile is read (1.2). If it names a specific character, that one is **set**
   (section 3) and returned [B3]. `no_random` plays no part here.
4. **Random choice** otherwise, from two lists that start empty, the **candidates** and the **defaults**.
   Every specific character is visited in registration-index order [B4]; for each:
   1. one with `no_random` is skipped [B4];
   2. unless the profile's class is empty, one whose class list does not contain the profile's class is
      skipped (exact comparison of the lower-cased texts) [B5];
   3. one with `team_default` is added to the defaults [B6];
   4. if the profile has a rank and the specific character's rank differs from it by 10 or more
      (|difference| ≥ 10), it is not a candidate; likewise for the reputation [B7];
   5. if the simulation exists and the specific character's id is in the **in-use registry** (2.1), it is not a
      candidate [B8];
   6. otherwise it is added to the candidates [B8].

   Then: if the defaults are empty, this is an assertion failure "no default specific character set for
   class" (with the profile's class), even when there are candidates [B9]. Otherwise the chosen specific
   character is candidates[*a* mod *c*] when there are *c* > 0 candidates, else defaults[*a* mod *d*] with *d*
   defaults, *a* being one draw; it is set (section 3) and returned [B10, B11].

So candidates are unused, eligible specific characters; the defaults ignore rank, reputation and use, and
are the fallback when every eligible specific character is in use. With the shipped data the rank and
reputation test never applies (no profile has them) [D6].

### 2.1 The in-use registry

The simulation keeps a registry of names with an integer each; it is part 6 of `sav` chunk 9 ("named
counters"), which the save writes and a load reads [U1, U2]. It holds the specific characters already
taken:

- setting a specific character with the simulation present removes the object's previous specific character
  from it (if that was non-empty and present) and adds the new id with the value 1 (if absent) [S1];
- loading chunk 9 empties it and then fills it from the save [U2];
- it is created empty together with the simulation's other registries, before the spawn file is read
  (section 4.2) [W6].

Nothing else of the choice and set adds or removes entries. Believed: nothing else of the program does
either (no other access to it was found), so ids are never released while the objects that took them die
or are destroyed. The value is always 1 here; the order of the saved entries is not defined (`sav` 9).

### 2.2 What a choice loads

CH-1 3.2 loads profiles and specific characters by id and leaves to this specification which ones are loaded
and when:

- Every choice that reaches step 3 loads the object's profile by its id [B3, E2]; a profile id that the
  profile files do not define is CH-1 3.2's fatal error there.
- A random choice loads every specific character, by registration index, before testing it (step 4.1),
  whatever its class or flags [B4, E2]. An entry's contents are read the first time it is loaded (CH-1 3.2),
  so the first random choice of a run reads all 743 specific characters, and a content error of CH-1 5 in any
  of them, of whatever class, appears then. In this edition none has one (CH-1 5.1, 5.5). The spawn file's
  reading (4.2 step 2, and 4.3) makes the first random choice at record 93 (`esc_lager1`, profile
  `esc_stalker_novice`): every character record before it has an explicit profile [E3].
- The set loads the specific character it sets, after its registry step (section 3 step 1) [E2]. A random
  choice only sets ids it has loaded; reading a profile takes its `specific_character` as text without loading
  it [B3], so an id that no specific character has is CH-1 3.2's fatal error at the set. In
  this edition every explicit profile names a defined specific character (CH-1 4).

## 3. Setting a specific character

Setting an empty id is an assertion failure, before any of the steps below [E1]. No choice passes one: the
explicit case sets only a non-empty id (2 step 3), and a random choice sets the id of a registered specific
character, all of which are non-empty in this edition (CH-1 3.1).

Setting the specific character *D* on an object does, in this order [S1–S9]:

1. **Registry**: as 2.1, then *D*'s id is stored as the specific character [S1].
2. **Model**: if *D*'s `visual` is not empty, it becomes the model name (extension removed, lower-cased),
   replacing the Visual block's [S2]. The text is not checked here: a `visual` that names no model file is
   stored and saved like any other, and the model lookup of `objects` 1 fails on it when the character
   comes online, a fatal error there (believed: the lookup is the one every object with a model makes,
   `objects` 2; the game was not run). The one such specific character of this edition is in section 9.
3. **Community and team**: if the community is unset (−1), it becomes *D*'s community position; then, for a
   stalker, the Creature block's team becomes that community's number. A community that is already set is
   kept, and the team is then not touched [S3, S10].
4. **Terrain masks**: for a stalker, if *D*'s `terrain_sect` is not empty, every line of that section of the
   configuration whose key is four comma-separated numbers is appended to the object's terrain masks (each
   number as a byte; the masks from `terrain` are kept); if the masks are still empty afterwards, the mask
   255,255,255,255 is added (`offline` 7.4). A section that does not exist is an assertion failure (believed,
   as for any section read) [S4, S10].
5. **Rank and reputation**: each is set to *D*'s only while unset; a rank or reputation already set is kept
   [S5].
6. **Name**: the name always becomes the text of *D*'s `name` [S6]. If that text contains
   `GENERATE_NAME_`, a name is generated: the **suffix** is the text after its first 14 characters (the
   length of `GENERATE_NAME_`; in the shipped data it always starts with it [D7]); `name_cnt` *n* and
   `last_name_cnt` *m* are read from section `[stalker_names_<suffix>]` of `system.ltx`; the name becomes
   the text of `name_<suffix>_<a₁ mod n>`, a space, and the text of `lname_<suffix>_<a₂ mod m>`, with two
   draws *a₁* then *a₂* (decimal numbers without leading zeros) [S7].
7. **Money**: if *D*'s money `min` and `max` are both non-zero, the money becomes `min`, plus (*a* mod
   (`max` − `min`)) with one draw *a* when they differ; otherwise the money is kept [S8]. Since a draw is at
   most 32,767, money more than 32,767 above `min` is never reached (no shipped range is that wide: the
   widest is 10,000) [D9].

A set thus makes 0 to 3 draws, in the order first name, last name, money [S9]; a random choice adds one draw
before them (2, step 4).

A trader object has no Creature block and no terrain masks, so steps 3 and 4 only set its community [S10].

## 4. When the choice runs

### 4.1 What makes it

- **Reading a Trader-like block** (the object state of a stalker or trader, `sav` 7.4, from the spawn file or
  a save) ends with the choice [W1]. Writing the block writes the stored values as they are [W2]. A stalker's
  state is read Trader-like block first, then its Monster-like block (`sav` 7.4.2), so the Creature block's
  team and the Visual block's model name read after it replace what step 3 and 2 set at that moment
  (`objects` 2 item 3).
- **The supplies step** (the items created with a new object, `offline` 8.1 step 3.4), for every stalker and
  trader, scripted or not [W4, W5]:
  1. a stalker first makes the choice (it changes nothing when the specific character is already set);
  2. a PDA (section `device_pda`) is created with the object as its original owner;
  3. the specific character is emptied and the community set to −1 (rank, reputation, name and money are
     left as they are), and the choice is made again (so a random choice is made anew, and an explicit
     specific character is set again);
  4. the PDA's specific character (`sav` 7.4.2) becomes the result;
  5. the items of the result's `supplies` are created, unless the object's custom data has a section
     `[dont_spawn_character_supplies]` (the items are CH-6's).
- **The post-registration step** of a stalker (`offline` 8.1 step 4) makes the choice after its other work
  [W10]; with the specific character set, nothing changes.
- **Reading or setting the rank, and reading the reputation**, through the program's accessors (believed:
  those of scripts and relations), make the choice first [W11]. So does the online character's creation
  (section 5).

### 4.2 New Game

With `offline` 8.1:

1. The simulation is created, registers itself, and creates its registries; the in-use registry is empty
   [W6].
2. **The spawn file is read**: every record is read into an object of its own (the record objects), state
   included [W7]. So every character record, in record order, makes its first choice now (the **spawn-file
   choice**), and every chosen id enters the in-use registry.
3. **Creation**, in increasing record index (`offline` 8.1 step 3): the new object is made from the record
   object's packets [W8], so it receives the record object's specific character, community, rank, reputation,
   name, money and (for a stalker) the record's model name and team (4.1); reading its state makes no new
   choice. Its supplies step (4.1) then makes the **supplies choice**: a random choice now excludes every id
   taken so far (by all spawn-file choices and the earlier supplies choices), including the object's own
   spawn-file choice.
4. The post-registration step (no change), then the save (`offline` 8.1 steps 4, 5), whose chunk 9 part 6
   lists every id chosen in steps 2 and 3.

After a New Game a character therefore has [W1–W8, S1–S8]:

| Value | From |
|---|---|
| specific character, the PDA's specific character | the supplies choice |
| community; a stalker's team | the supplies choice |
| name | the supplies choice (a generated name drawn anew) |
| money | the supplies choice if its specific character has a money range, else the spawn-file choice's if its has one, else the record's |
| rank, reputation | the spawn-file choice (never unset again) |
| model name | the supplies choice's `visual` (every specific character has one [D17]) |
| terrain masks of a stalker | the section's `terrain`, then the supplies choice's `terrain_sect` |

For an explicit profile both choices give the same specific character. For a class profile the two choices are
different whenever an unused candidate remains; when none remains the supplies choice falls back on the
defaults, which may repeat any of them. Which specific characters are chosen depends on the generator's state,
which comes from a time-stamp-counter read and every earlier draw (`original-determinism.md` 2.2): it is not
reproducible from the data.

**A stalker's model.** Reading the record object's state at creation leaves the record's model name, the
Visual block being read after the Trader-like block (4.1, `objects` 2 item 3); the supplies step that
follows sets the supplies choice, whose `visual` replaces it (section 3 step 2) [W8, W4, W5, S2]. Nothing
sets it back: the specific character's `visual` is used by the set only [M1]; apart from the set, the
program sets a model name only when an object is constructed (from its section's `visual`, before its state
is read), in multiplayer, for phantoms, for physics-skeleton objects made anew, and on a change-model event
that only phantoms send [M2]; no script function sets one [M4]. **Disputed in review:** [M2] enumerates
only the uses of the program's model-name setter (the one that removes the extension and lower-cases); the
reading of a Visual block also sets the model name, stored as read without that setter, and is missing from
this list; that it never runs again on an existing object is not checked, so "nothing sets it back" is
believed, not established. Writing the Visual block writes the model
name as set [M3], so the save and the online character (which takes the object's model name, `objects` 2)
carry it. A stalker therefore shows the `visual` of its supplies choice, not its record's: for 651 of the
1,073 stalker records some candidate's visual differs from the record's [D17]. The trader's is the same
either way (section 8).

### 4.3 Load

`sav` 11.2: the spawn file is read again (step 3), so every character record object makes a choice and the
in-use registry fills; the saved objects (step 4) already have their specific characters, so reading them
makes no choice and adds nothing; chunk 9 (step 6) then replaces the registry by the saved one [W1, U2].
No supplies step runs. A stalker loaded from a save has its saved model name (its Visual block, read after
the Trader-like block, which makes no choice) [W1, B2], and only the terrain masks of its section's
`terrain` (step 4 of section 3 does not run again); every stalker record's section has `terrain =
255,255,255,255,0,1`, whose mask 255,255,255,255 every game vertex matches (`offline` 1.6), so its added
masks never changed a path [D18].

### 4.4 Objects created during play

- **From a record** (believed: when scripts create an object from a stored record): as step 3 of 4.2, with the
  record object's choice made when the spawn file was last read and the registry of the running game.
- **From a section** (the scripts' creation by section name, e.g. `scripts\se_respawn.script`): no state is
  read [W9]; the object has the section's profile (`character_profile`, else `default`) and the initial values
  of 1.1, and its supplies step makes the choice: a stalker twice (steps 1 and 3 of the supplies step, the
  second excluding the first's id), a trader once. Rank and reputation come from the first choice.

## 5. The online character

When a stalker or trader comes online in the single-player game [O1]:

- an empty character profile on its object is an assertion failure;
- it copies the object's community, rank, reputation and profile, makes the choice on the object (which
  changes nothing: the specific character is always set by then) and takes the object's specific character,
  whose entry gives it the other properties CH-1 reads (its icon, start dialog and the rest; their
  uses belong to their own specifications);
- its name is the object's character name.

A specific character's `start_dialog` and `actor_dialog` ids are stored as text when it is read, without being looked
up (CH-1) [B13]; an id that no dialog file defines (`scope`, "Gaps found in review") is therefore accepted by
the identity, and what it does when a talk uses it is the dialogue's (believed; `dialogue-scope`).

So the name the talk window and the crosshair show is the Trader-like block's character name, and the
picture the specific character's `icon` (believed: as `scope` 3, B-chars 3, and the panels' specifications use them).
**Disputed in review:** only that the online character's name is the object's character name is checked here;
that the talk window and the crosshair show that name is not, so it is believed as well, like the picture.

## 6. Monsters and the section keys

- `character_profile` (94 sections, all of class `AI_STL_S` or `AI_STL`) is the profile of an object created
  from its section (1.1); a record's own profile replaces it when the record's state is read [W3, D12].
- `rank` (60 sections, 58 of them monsters'): a monster's simulation object reads it as a signed number
  (0 when absent), and the online monster reads it again as an unsigned number (0 when absent) [O2]. 249 of
  the 276 monster records have a section with `rank` (121 with 0, 84 with 401, 44 with 801) [D13]. What uses
  the value is CH-3's (believed: the relations). The two other sections with `rank`, `m_trader` and
  `m_izgoy` (both `rank = 1`), are not monsters': it is not the Trader-like rank, and the trader's object is
  not monster-like (section 3), so it is believed unused for the trader; `m_izgoy` has no record [D12b].
- `community` and `spec_rank` (143 sections each, character and monster ones): the program does not read
  them: `spec_rank` is not named in `xrGame.dll`, and the text `community` is used only by the
  specific-character reader and as the name of a script method [D15, O3]. `scripts\se_respawn.script` reads both from the
  sections it respawns, and uses them only in its functions `lstats` and `estats` (statistics printouts),
  which no shipped script calls [D14]; so they have no effect (believed: those functions are reached only by
  hand, from a console script call).
- A monster has no Trader-like block: it has no specific character, profile, name or Trader-like
  community, rank or reputation.

## 7. The player

The player's record also has a Trader-like block, with profile `actor`, which names the specific character
`actor` (`no_random` 1) [D16]; the same reading and choice apply to it (believed). The player is outside this
specification (`scope` 1, "character").

## 8. Worked example: the trader

The trader is record 84 (`scope` 4): profile `escape_trader`, money 1,000,000, model `actors\trader\trader`,
identity unset [D11]. The profile names the specific character `escape_trader` (`character_desc_escape.xml`,
`no_random` 1): community `trader`, rank 330, reputation 23, name `escape_trader_name`, visual
`actors\trader\trader`, money `min` = `max` = 100,000, no `terrain_sect`, and a name that is not generated
[D11, D7]. At a New Game:

1. Reading the spawn file: the explicit case sets `escape_trader`: in-use registry {`escape_trader`: 1};
   model `actors\trader\trader` (unchanged); community position 13 (`trader`, 1.3) [D10]; rank 330;
   reputation 23; name the text of `escape_trader_name`; money 100,000 (both bounds non-zero and equal: no
   draw), replacing the record's 1,000,000. No draw at all.
2. Creation copies these. The supplies step creates his PDA, empties the specific character and community,
   and sets `escape_trader` again (the registry already has it), giving the same values; the PDA's specific
   character is `escape_trader`; his `supplies` (`[spawn] wpn_knife`, CH-6) are created, his custom data
   having no `[dont_spawn_character_supplies]` (`scope` 4).
3. Online he copies these; his name is the text of `escape_trader_name`, his picture the specific character's icon
   `ui_npc_u_trader`.

His identity is thus the same in every game and needs no random choice (`scope` 6: "the explicit choice").

## 9. The shipped data

- 1,074 character records name 205 profiles: 14 profiles name a specific character (one record each, among
  them `escape_trader`), 191 only a class (1,060 records) [D4].
- For every one of those 191 classes the candidates before the in-use test equal its defaults and are not
  empty, so no shipped record reaches the assertion of 2 [D5]. Records by the number of such specific characters of
  their profile's class: 1 (369 records), 2 (112), 3 (140), 4 (98), 5 (148), 6 (47), 7 (33), 8 (30), 24 (83)
  [D5]. Profiles shared by more records than their class has specific characters give repeated identities, from the
  defaults.
- `no_random` is set on `actor`, `mp_actor`, `escape_trader`, `bar_barman`; `team_default` is 1 on 738
  specific characters, absent on 4 and 3 (so not set) on `val_monolith_regular_default1`; none has both [D2].
- 520 specific characters have a generated name `GENERATE_NAME_<suffix>`: `stalker` 344, `sergeant` 96, `bandit` 39,
  `private` 17, `lieutenant` 17, `captain` 5, `science` 2; the seven sections `[stalker_names_<suffix>]` give
  (`name_cnt`, `last_name_cnt`) `stalker` (102, 599), `bandit` (36, 180), `science` (14, 27), `private`,
  `sergeant`, `lieutenant`, `captain` (1, 200) each, and every id below those counts is in the string table;
  no string id contains `GENERATE_NAME_` [D7, D8].
- 226 specific characters have `money`; (`min`, `max`): (600, 2,000) 112, (200, 600) 47, (2,000, 5,000) 40,
  (5,000, 10,000) 22, (100,000, 110,000) 3, (100,000, 100,000) 1 (the trader), (0, 200) 1, which leaves the
  money unchanged (3 step 7) [D9].
- Every `terrain_sect` names a section of the configuration [D18].
- Two profiles have a class that no specific character has (CH-1 4): `esc_plemiannik` and
  `gar_newbie_in_tonnel`, each its own class. A choice for either finds neither a candidate nor a default:
  the assertion failure of section 2. None is ever made: no record names them, no section's
  `character_profile`, no other file of `config\`, `scripts\` or `spawns\` (only `npc_profile.xml`, which
  defines them), and no string of `xrGame.dll`; the script interface reads a profile's name but sets none
  [E4].
- `ds_zomb_12` (CH-1 5.7) has the `visual` `actors\stalker_zombi\stalker_zombie_8`, already lower case and
  without an extension, which no archive file matches (no path ending `stalker_zombie_8.ogf`): setting it
  makes that text the model name, with the outcome of section 3 step 2 [E5]. It is the only specific
  character of its class (`team_default` 1, `no_random` off), and only the class profile `ds_zomb_12` has
  that class, which no profile names explicitly; no record, no section's `character_profile`, no script and
  no string of `xrGame.dll` names that profile (only `character_desc_kishka.xml` and `npc_profile.xml` hold
  the id), so in this edition it is never chosen [E5].

## 10. Unestablished

- Which specific characters the random choices pick in a given game: the generator's state at each choice (its
  seed and the number of draws made before, by every consumer) (2, 4.2).
- That nothing else of the program adds to or removes from the in-use registry, or changes a set specific
  character (2.1; believed from the absence of other accesses found).
- What uses a monster's `rank`, and whether any composed key name reads `community` or `spec_rank` (6).
- How the player's identity is filled (7) and what multiplayer does (2 step 1).
- What follows an assertion failure in the running game (believed: the game ends).
- Everything here is static: nothing was observed in the running game.

## 11. Provenance

- Date: 2026-11-27. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binary
  `bin\xrGame.dll`.
- Data examined: winning copies of `config\` (`system.ltx` tree, `gameplay\npc_profile.xml` and the 15
  specific-character files, `creatures\game_relations.ltx`, the Russian string tables), `scripts\` and
  `spawns\all.spawn`, as extracted for `pda-scope`, read with the helpers of `checks/characters-scope/`
  (spawn reader written from `sav` 7.2–7.4).
- Analysis: static analysis (the existing headless Ghidra decompilation and instruction listing of the whole
  of `xrGame.dll`) of the choice, the set, the Trader-like block's reading and writing, the object's
  supplies, post-registration and creation steps, New Game, the spawn file's reading, the simulation's
  registries and the loading of chunk 9 part 6, the online character's creation, the monsters' rank reading,
  the shared generator's draw, every place that sets a model name or reads a specific character's `visual`,
  and the Visual block's writing; the script interface listed in `scripts\lua_help.script`; data surveys of
  the specific characters, profiles, sections, string tables and the archive listing. No debugging or
  execution; the game was not run.
- Checks: `checks/character-identity/checks.py` (68 assertions), with `export.py` (the binary evidence in
  `ev/`) and `vtx.py`; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.
