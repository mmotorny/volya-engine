# New Game creation: supplies, random generator and ids

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of what the simulation
creates beside the spawn file's records when a New Game is created: which objects receive supplies (the items
created with them) and in which order, which random draws decide them and from which generator, and which object
id every created object receives. It is OS-3 of `docs/behavior/narrowed-specs-scope.md` (section 3.3) and settles
`docs/behavior/offline-simulation.md` 8.1 (the supply rules, the random generator behind them, the New Game ids)
and `docs/behavior/player-state.md` 3.1 (the ids of the created items).

Builds on (not repeated):

- `docs/behavior/offline-simulation.md` (cited as `offline`): 8.1 the creation and registration order.
- `docs/behavior/player-state.md` (cited as `player`): 3.1 how a `[spawn]` text is read (entries by index in
  sorted key order, one entry per key with the last value winning, count, `prob=`, `cond=`, the addon words), the
  draw that decides each item, and the record every created item receives; 2.1 the player's record and id 0.
- `docs/behavior/character-identity.md` (cited as `identity`): 4.1 the supplies step of stalkers and traders
  (the PDA, the second choice, the specific character's supplies and `[dont_spawn_character_supplies]`), 4.2 the
  New Game choices; the shared random generator of its conventions.
- `docs/formats/character-descriptions.md` (cited as CH-1): 5.3 the kept `supplies` text, 7 the included
  fragments and the indented include lines.
- `docs/formats/ltx-config.md` (cited as `ltx`): 1–4 lines, entries and includes, 6 the ordering.
- `docs/formats/sav.md` (cited as `sav`): 7.2 the spawn packet (id, parent id), 7.4 the blocks.
- `docs/behavior/console-log.md` (cited as `console-log`): the report of a fatal error.
- `docs/behavior/original-determinism.md` (cited as `determinism`): 2.2 the shared generator, 3 the C runtime's
  per-thread generator that the scripts' `math.random` draws from.

Conventions:

- **Record**: as in `offline` 8.1. **Record object** here: the object created from a record at the New Game
  (`offline` 8.1 step 3.1; `identity` 4.2 step 3's new object), not the object the spawn file is read into
  (`identity` 4.2 step 2). **Created object**: a record object or a supply item. **Supplies** of an
  object: the objects created right after it, with it as parent, by its supplies step (`offline` 8.1 step 3.4).
- A **`[spawn]` text** is a text read as `player` 3.1 item 1 reads the player's custom data; **reading** one creates
  its items as that item describes. "Probability *p*" is the value of `prob=` there (1 when absent).
- **Kinds** are those of `sav` 7.4.2 (player, stalker, trader, monster, item, …).
- Statements are static (read from the data and the shipped `bin\xrGame.dll` and `bin\xrCore.dll`); the game was
  not run. Inferred statements are marked **believed**; what could not be established is marked
  **unestablished**.
- Bracketed labels are the checks reproducing a statement: assertions of `checks.py` in the dirty workspace's
  `checks/new-game-supplies/` (`python3 -I checks.py`; `export.py` re-creates their evidence in `ev/`).

## 1. The supplies step per kind

Every record object runs its supplies step right after its registration (`offline` 8.1 step 3.4; a group kind
creates its members instead) [B10]. What the step does depends only on the kind [B1–B4]:

| Kind | Supplies step, in this order |
|---|---|
| player | 1. its custom data read as a `[spawn]` text; 2. the PDA, the choice and the specific character's supplies as for a trader (`identity` 4.1 steps 2–5; for the player `player` 3.1 items 2 and 3) |
| trader | 1. its custom data read as a `[spawn]` text; 2. `identity` 4.1 steps 2–5 (PDA, choice, the specific character's `supplies` read as a `[spawn]` text unless the custom data has `[dont_spawn_character_supplies]`) |
| stalker | 1. `identity` 4.1 steps 1–5 (choice, PDA, choice, the specific character's `supplies`, with the same exception); 2. **then** its custom data read as a `[spawn]` text |
| every other kind (monsters, crows, phantoms, items, inventory boxes, physics objects, vehicles, zones, restrictors, smart terrains, …) | its custom data read as a `[spawn]` text, nothing else |

So a stalker's custom-data items come after its PDA and its character supplies, while the player's and the
trader's come before their PDA [B2, B3]. The specific character's `supplies` text and the custom data are read by
the same reader, with the same rules (`player` 3.1 item 1) [B4, B5]. An empty or absent text creates nothing [B5].

A monster's supplies step reads only its custom data; the monster keys `Spawn_Inventory_Item_Section` and
`Spawn_Inventory_Item_Probability` take no part in it [B1, B5] (what they do is outside this specification).

Every created item runs the supplies step of its own kind in turn, right after its creation and registration
[B9]: for an item that is a reading of its custom data, which for an item created from a section comes from the
section's `custom_data` key (`player` 3.1); no item section of this edition has that key (only 94 stalker
sections do), so no supplied item creates further items [D4].

### 1.1 Reading a `[spawn]` text

Beyond `player` 3.1 item 1:

- The text is read as a configuration (`ltx` 1–3) whose include base is the game's configuration folder
  (`config\`) [B5]. A line of the text that starts with `#` and contains `#include` (after trimming, `ltx` 1) is
  therefore an include line naming a file under `config\` (`ltx` 4).
- In this edition 12 specific characters have such a line in their `supplies` text, the indented include lines of
  CH-1 7, each naming one of `gameplay\character_items.xml`, `gameplay\character_food.xml`,
  `gameplay\character_drugs.xml` (all present) [D5]. Read as configuration files these fragments have no section
  header, so their lines belong to no section and add nothing (`ltx` 4): those 12 characters get none of the
  fragment's items [D5]. (Only the first include of a line is read, `ltx` 4; one line names three fragments.) That
  the include base is the configuration folder itself (its path ending in a separator) is **believed**; with any
  base under which the three files exist the outcome is the same.
- A key written on several lines of one text is one entry with the last line's value (`ltx` 3). Shipped example:
  `esc_stalker_novice_default2`, `_default5` and `_default10` have `device_torch` alone (certain) and then, from
  `character_items.xml`, `device_torch = 1, prob=0.5`; the torch is one entry with probability 0.5 [D9].

### 1.2 Shipped supplies

- Records whose custom data has a `[spawn]` section: 68 stalkers, 8 inventory boxes, the player and the trader;
  no record of any other kind [D1]. No record writes a `prob=` in its custom data (every custom-data item has
  probability 1); counts there go up to 231 [D2].
- Probabilities written in the specific characters' `supplies`: none (1), 0.2, 0.3, 0.4, 0.5, 0.7; counts go up to
  4 [D2].

## 2. The random generator of the supplies

- Each draw that decides a supply item (`player` 3.1 item 1) comes from a generator **held by the object whose
  supplies are read** (the stalker, trader, player or box itself): a 32-bit state *x* stepped as
  *x* ← *x* · 0x343FD + 0x269EC3 (mod 2³²), the draw being (*x* ≫ 16) AND 0x7FFF [B6]. <!-- spec-lint: constant -->
  It is the same stepping as the shared generator of `identity` and `determinism` 2.2, applied to a different
  state: supply draws neither use nor change the shared state, and the shared generator's draws (the specific
  characters chosen, generated names, money, `identity`) neither use nor change an object's own state [B6, B8].
- The state is set once, when the object is constructed, to the low 32 bits of a reading of the Windows
  performance counter (`QueryPerformanceCounter`) [B7]. A record object is constructed when it is created at the
  New Game (the objects the spawn file is read into are other objects, `identity` 4.2), so its supply draws
  start from that reading. Nothing else sets the state (**believed**: no other write to it was found).
- The scripts' `math.random` draws from the C runtime's generator of the calling thread (`determinism` 3), which
  is a third, separate generator: the supply draws do not draw from it and do not change it [B5].
- Every count unit of every entry makes exactly one draw, also for probability 1, and the draw is made before the
  item is created [B5]. The draws of one object are made in the order of its supplies step (section 1): for a
  stalker the character supplies' entries first, then the custom data's; for the player and the trader the custom
  data's first. Creating an item draws nothing from either generator [B8]. **Disputed in review:** [B8] shows
  only that the item-creation and record-creation steps themselves make no draw; draws made by what they call
  (the new object's construction, registration and its own supplies step) are not checked, so this is
  believed, not established.
- An item is created when *p* is greater than the single-precision product of the draw *r* and the
  single-precision value of 1/32,767, *p* being single precision too [B6]. Of the 32,768 values of *r*, this
  creates the item for [D3]:

| *p* written | values of *r* creating the item | chance with *r* uniform |
|---|---|---|
| none (1) | 32,767 (all but 32,767) | 0.999969 |
| 0.7 | 22,937 | 0.699982 |
| 0.5 | 16,384 | 0.5 |
| 0.4 | 13,107 | 0.399994 |
| 0.3 | 9,831 | 0.300018 |
| 0.2 | 6,554 | 0.200012 |

- Because the seed is a performance-counter reading, which items a game creates is not reproducible from the data.
  Treating each draw as uniform over the 32,768 values is **believed** (every seed being equally likely, each
  draw of the full-period stepping takes every value equally often); the draws of one object are successive
  outputs of one stepping, not independent ones.

## 3. Object ids at a New Game

### 3.1 The id pool

- Ids come from one pool of the ids 0 … 65,534, split into 256 blocks by *id* ≫ 8 (block 255 holds 255 ids, the
  others 256) [B12]. Each block keeps a list of its free ids and a time stamp. A fresh pool lists every id in its
  block in descending order and has every stamp 0 [B12].
- The pool is replaced by a fresh one each time a game is started (New Game or load), before the simulation is
  created [B16].
- **Taking any id:** the block is the first, in block order, of the non-empty blocks with the smallest stamp; the
  id is the last id of that block's list, which is removed [B13, B14]. In a fresh pool this gives the ids in increasing order.
- **Taking a given id:** it is removed from its block's list, the list's last id taking its place [B14].
- **Giving an id back** (when an object is destroyed): the id is appended to its block's list and the block's stamp
  becomes the engine's timer at that moment [B15]. (What the timer counts is not covered here.)
- Taking an id from a pool with no non-empty block is the failed check "Not enough IDs" (`console-log`'s fatal
  report, ending the game) [B17]. A New Game of this edition creates at most 34,018 objects, so it never reaches it [D10].

### 3.2 Ids during creation

1. After the spawn file is read and before any object is created, id 0 is taken out of the fresh pool [B11]
   (`offline` 8.1 step 2).
2. Objects are created in increasing record index (`offline` 8.1 step 3). The player's record object gets id 0
   without the pool (`player` 2.1); every other record object takes any id when it is created, before its
   registration [B10].
3. Each supply item takes any id when it is created, before the next item is created [B9]. An item that a draw does
   not create takes no id. Supplies are created right after their owner and before the next record's object
   (section 1, `offline` 8.1 step 3.4).
4. Nothing gives an id back during the New Game's creation (**believed**: no object is destroyed between the fresh
   pool and the save), and scripts create no object before the save: the registration callbacks of the shipped
   server-object scripts (15, in `se_item`, `se_monster`, `se_respawn`, `se_stalker`, `se_zones` and
   `smart_terrain`) create and release nothing, nor do the modules they call [S1]; the scripts' creation happens in
   updates after the start, through the script API (`docs/behavior/script-api-simulation-objects.md`).

So at a New Game the ids are 0 for the player and 1, 2, 3, … for every other object in the order of creation:
each record's object, then its supplies in the order of section 1, then the next record. An object's id is one
more than the number of objects created before it, the player not counted. The ids are fixed by the data except
for the supplies' draws (section 2) and the specific characters chosen (`identity` 4.2), which decide how many
items each character receives; from the first object whose supplies depend on a draw on, the ids cannot be
predicted from the data.

**The player's items** (`player` 3.1): the player is record 63 [D12]; its five items (the four custom-data items,
then the PDA) take the next ids after every object created for records 0 … 62, consecutively in their order of
creation, when every draw creates them (each of the four custom-data items is skipped with a chance of 1 in 32,768,
which leaves out its id).

**The trader** (record 84) [D11]: his object takes id *t*; then his custom-data items `ammo_9x18_fmj` (*t* + 1) and
`wpn_pm` (*t* + 2), the keys in sorted order; then his PDA (*t* + 3); then his specific character's `wpn_knife`
(*t* + 4); the next record's object takes *t* + 5 (each of the three items with probability 1 being skipped with a
chance of 1 in 32,768, which shifts the later ids down by one).

## 4. Worked example: the novices of the beginners' village on `l01_escape`

Records 93 … 99 are the stalkers `esc_lager1` … `esc_lager7`: section `stalker`, profile `esc_stalker_novice`,
custom data naming the smart terrain `esc_lager` (the beginners' village), and none with
`[dont_spawn_character_supplies]` [D7]. Their custom-data `[spawn]` items, each count 1 and probability 1 [D7]:

| Record | Name | Custom-data item |
|---|---|---|
| 93 | `esc_lager1` | `conserva` |
| 94 | `esc_lager2` | `bandage` |
| 95 | `esc_lager3` | `bread` |
| 96 | `esc_lager4` | none |
| 97 | `esc_lager5` | `kolbasa` |
| 98 | `esc_lager6` | none |
| 99 | `esc_lager7` | none |

The profile is a class profile; its supplies choice (`identity` 4.2) picks one of the eight specific characters of
the class, `esc_stalker_novice_default1` … `_default6`, `_default9`, `_default10`, by draws of the shared generator.
Their `supplies` texts give these `[spawn]` entries, in the order they are read (sorted keys), all with count 1
[D8, D9]:

| Entry | Probability | default1, 3, 5 | default2, 4 | default6 | default9, 10 |
|---|---|---|---|---|---|
| `ammo_12x70_buck` | 1 | yes | — | yes | — |
| `ammo_9x18_fmj` | 1 | — | yes | — | yes |
| `bandage` | 0.4 | — | — | yes | yes |
| `bread` | 0.3 | yes | yes | yes | yes |
| `device_torch` | 0.5 | yes | yes | yes | yes |
| `energy_drink` | 0.5 | yes | yes | yes | yes |
| `guitar_a` | 0.7 | yes | yes | yes | yes |
| `harmonica_a` | 0.7 | yes | yes | yes | yes |
| `kolbasa` | 0.3 | yes | yes | yes | yes |
| `medkit` | 0.2 | — | — | yes | yes |
| `vodka` | 0.2 | yes | yes | yes | yes |
| `wpn_bm16` | 1 | yes | — | yes | — |
| `wpn_pm` | 1 | — | yes | — | yes |

(`default2`, `default5` and `default10` also name `device_torch` alone, before the fragment's line: one entry
with the later probability 0.5, section 1.1.) The entries of probability below 1 come from the fragments
`character_items.xml` (`harmonica_a`, `guitar_a`, `device_torch`), `character_food.xml` (`bread`, `kolbasa`,
`vodka`, `energy_drink`) and `character_drugs.xml` (`medkit`, `bandage`) (CH-1 7) [D6].

**At a New Game** each of the seven novices, with id *n*, is followed by (section 1, stalker row; section 3.2):

1. its PDA, id *n* + 1;
2. the entries of its chosen specific character, in the table's order, each created when its draw from the
   novice's own generator passes (section 2), each created one taking the next id;
3. its custom-data item (if any), created unless its draw is 32,767, taking the next id.

The next novice's object takes the id after the last of these. For example, with `esc_lager1` on `default1`
and every draw creating its item: `esc_lager1` *n*, PDA *n* + 1, `ammo_12x70_buck` *n* + 2, `bread` *n* + 3,
`device_torch` *n* + 4, `energy_drink` *n* + 5, `guitar_a` *n* + 6, `harmonica_a` *n* + 7, `kolbasa` *n* + 8,
`vodka` *n* + 9, `wpn_bm16` *n* + 10, `conserva` *n* + 11, and `esc_lager2` *n* + 12; with only the
probability-1 items created: `ammo_12x70_buck` *n* + 2, `wpn_bm16` *n* + 3, `conserva` *n* + 4, `esc_lager2`
*n* + 5.

**The guitar.** Every one of the eight specific characters has `guitar_a = 1, prob=0.7` from
`gameplay\character_items.xml` [D6, D8], so whichever is chosen, each novice draws once for one guitar and
receives it for 22,937 of the 32,768 draw values: a chance of 0.69998 (section 2) [D3]. The guitar is its
sixth draw on `default1` … `_default5` (after the ammunition, `bread`, `device_torch`, `energy_drink`) and its
seventh on `default6`, `_default9`, `_default10` (after `bandage` as well); the harmonica's draw follows it.
**Disputed in review:** with the four entries named before it, the guitar is the fifth draw on `default1` …
`_default5` and the sixth on `default6`, `_default9`, `_default10` (the table's sorted order). The
seven novices' draws come from seven separately seeded generators, so whether each gets a guitar does not depend
on the others (**believed**, as the uniformity of section 2).

## 5. Unestablished

- Which items a given game creates: the seed of each object's generator (a performance-counter reading) and the
  specific characters chosen (`identity` 10); hence the ids after the first object with a drawn supply.
- That nothing else sets an object's own generator state after construction (section 2, believed) and that nothing
  gives an id back during the New Game's creation (section 3.2, believed).
- What the engine timer stamped on a block counts (section 3.1); it does not occur at a New Game.
- The ids of objects created after the New Game's save (the player's bolt, `player` 3.2; objects created by
  scripts): taken from the same pool by its rules, which objects and when is outside this specification.
- Everything here is static: nothing was observed in the running game.

## 6. Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries
  `bin\xrGame.dll` and `bin\xrCore.dll`.
- Data examined: winning copies of `spawns\all.spawn` (all 8,641 records, custom data read with the layouts of
  `sav` 7.4), `config\system.ltx` tree (sections with `custom_data`), `config\gameplay\` (the 15 specific-character
  files and their fragments, read with the reader written from CH-1), the shipped `scripts\` (registration
  callbacks of the server-object scripts and the modules they call).
- Analysis: static analysis of `xrGame.dll` (the existing headless Ghidra decompilation and instruction listing of
  the whole library, and its tables of object kinds) of the supplies step of every simulation object kind, the
  reading of a `[spawn]` text, the per-object generator and its seeding at construction, the creation of items and
  of record objects, the New Game sequence, the id pool (construction, block choice, taking and giving back ids,
  its replacement at game start); of `xrCore.dll` (headless Ghidra decompilation) of the counter read that seeds
  the generator and of the configuration reader's include base. The exact creation counts were computed from
  single-precision arithmetic. No debugging or execution; the game was not run.
- Checks: `checks/new-game-supplies/checks.py` (30 assertions), with `export.py` (the binary evidence in `ev/`)
  and `data.py` (record, configuration and supplies readers).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What creating a supply item of a section absent from the configuration does (the failed check "Cannot find
  item with section" is present), that the shipped `supplies` of `cit_killer_leader` (`cit_killers_documents`,
  `cit_leader_killer_pda`, `outfit_killer_m2`) and `cit_vp` (`cit_vp_pda`) name sections that no file of the
  `config\system.ltx` tree defines, and whether either specific character can be chosen at a New Game.
