# Material pairs (`gamemtl.xr` chunk 0x1003)

Format specification of the **material-pair list** of the game material library `gamemtl.xr`, S.T.A.L.K.E.R.:
Shadow of Chernobyl (Russian edition): the pair header, the sound strings of each pair and the other strings
stored beside them, how the game turns the strings into sounds when it loads the library, how the pair of two
materials is found, and whether a pair linked to another pair takes values from it. It is S9 of the scoping
specification `docs/behavior/level-sound-scope.md`. Which sound of a list plays, when and where, is for the
specifications that play them (S11 footsteps, S13 contacts, S14 breaking, S20 bullet impacts). What could not be
established is marked **unestablished**; what is inferred rather than shown by the data or the program is marked
**believed**.

Builds on (not repeated here):

- `docs/formats/level-cform.md` section 3: the file `gamemtl.xr`, loaded once at game start; its top-level
  chunks (section 3.1); the materials, each with an **id** and an **index** (its position in file order); the
  mapping of a triangle's material id to that index when a level loads (section 3.3).
- `docs/formats/chunks.md`: chunk sequences, lookup by id (section 3), sequential iteration (section 4).
- `docs/formats/sound-ogg-params.md` (S1): how a sound name finds its file `sounds\<name>.ogg` and the cases that
  are fatal (a missing file among them); the "unspecified" AI sound type (section 7.2).
- `docs/behavior/sound-positional.md` (S2): creating a sound from a name and a volume category (section 3.1),
  the **effects** category; `-nosound` (section 2).

Conventions: little-endian; `u32` unsigned 32-bit; `stringZ` = bytes up to and including a zero byte (the data
holds ASCII only). Sizes in bytes. "Pair" = one sub-chunk of chunk 0x1003. Ids and indices of materials are those
of `level-cform.md` section 3.

**Checks.** Every claim carries a tag: [D*n*] data checks (`check_data.py`), [B*n*] checks against the shipped
binaries (`check_bin.py`, on exports made by `export.py`), [P] the throwaway parser written from this text
(`parser.py`). All are in the dirty workspace, `checks/material-pairs/`; `run_all.py` extracts the files
(`extract.py`), regenerates the exports and runs all three.

## 1. The pair list

- `gamemtl.xr` is stored identically in `gamedata.db0` and `gamedata.dba` (310,494 bytes, CRC-32 `ca9561b8`); its
  top-level chunks are 0x1000 (2 bytes), 0x1001 (8 bytes: next free material id 94, next free pair id 1,398),
  0x1002 (the materials, 8,990 bytes) and 0x1003 (the pairs, 301,462 bytes) [D1].
- Chunk 0x1003 is a chunk sequence with one sub-chunk per pair. The game reads it once, when the library loads
  (after the materials), by sequential iteration: every sub-chunk is a pair, in stored order; the sub-chunk ids
  play no part [B1]. In the shipped file the ids are 0 … 1,246 in order: 1,247 pairs [D1].

## 2. A pair

A pair's payload is itself a chunk sequence. The game looks up four sub-chunks by id; each is required, and a
missing one is a fatal error (assertion failure). Other sub-chunks are not read [B2]. Every shipped pair has
exactly these four, in the order 0x1000, 0x1002, 0x1003, 0x1005 [D2].

| Sub-chunk | Content |
|---|---|
| 0x1000 | header, 20 bytes: five `u32` (table below) |
| 0x1002 | one `stringZ`: the **breaking** sound list |
| 0x1003 | one `stringZ`: the **step** sound list |
| 0x1005 | three `stringZ` in a row: the **collide** sound list, the collide **particle** list, the collide **mark** list |

Header [B3]:

| Offset | Field |
|---|---|
| 0 | id of the first material |
| 4 | id of the second material |
| 8 | the pair's own **id** |
| 12 | **link**: the id of another pair, or 0xFFFFFFFF for none (section 6) |
| 16 | **fifth word** (section 6) |

All five are kept as stored [B3]. The two materials are unordered: the pair serves both orders (section 5).

In the shipped file [D2]: sub-chunk 0x1000 is 20 bytes in every pair; sub-chunks 0x1002 and 0x1003 each hold
exactly one string and 0x1005 exactly three, each sub-chunk ending with the last string's zero byte.

## 3. Lists

Each of the five strings is a **list** of items separated by commas [B5]:

- **Item count.** An empty string has 0 items. Otherwise the commas are counted from the start, stopping after a
  comma that is directly followed by another comma (that first comma is counted, nothing after it); the count is
  the number of counted commas, plus 1 if any text follows the last counted comma (or if no comma was counted).
  So `a` has 1 item, `a,b` 2, `a,b,` 2, `a,,b` 2.
- **Item *k*** (from 0) is the text after the *k*-th comma (the whole string's start for *k* = 0) up to the next
  comma or the end of the string, with leading and trailing bytes 0x00 – 0x20 removed. (So `a,,b` gives the items
  `a` and an empty item.)
- **At most 4 items** per list; a list with more is a fatal error (assertion failure) when the library loads.
- An item longer than 127 bytes in a sound list, or longer than 255 bytes in a particle or mark list, is not
  supported (the original copies items into fixed buffers of 128 and 256 bytes) [B5].

In the shipped file every item is non-empty, printable ASCII without spaces or commas, and lower case; the
longest items are 38 (breaking), 55 (step), 55 (collide sounds), 30 (particles) and 26 (marks) bytes [D7]. Item
counts (number of items: number of pairs) [D7]:

| List | 0 | 1 | 2 | 3 | 4 | Items in all | Distinct items |
|---|---|---|---|---|---|---|---|
| breaking | 1,245 | 2 | — | — | — | 2 | 1 |
| step | 406 | — | 146 | 72 | 623 | 3,000 | 93 |
| collide sounds | 443 | 40 | 266 | 127 | 371 | 2,437 | 111 |
| collide particles | 637 | 241 | 268 | 39 | 62 | 1,142 | 36 |
| collide marks | 1,075 | 36 | 6 | 63 | 67 | 505 | 53 |

The breaking list is non-empty in two pairs only, 788 (`objects\bottle` / `objects\bottle`) and 1051
(`objects\glass` / `materials\asphalt`), both `material\bottle\collide\bottle_break_1` [D7].

## 4. What loading makes of the lists

When the library loads [B4, B6, B7]:

- **Sound lists** (breaking, step, collide sounds): each item becomes one sound, created from the item as its
  sound name (S1) with the **effects** volume category and an unspecified AI sound type (so the file's AI sound
  type applies, S1 section 7.2). The pair keeps its sounds per list in item order (item *k* is sound *k*). All of
  them are created at this point, so a missing file is fatal at game start (S1). While the sound system is
  disabled (`-nosound`, S2) no sound is created [B6].
- **Particle list**: the items are kept as names [B7]. Every shipped particle item occurs as a zero-terminated
  name in `particles.xr` (36 names) [D8] (believed: names of particle effects; their use belongs to the
  specifications of contact effects).
- **Mark list**: each item makes one surface mark with the shader `effects\wallmark` and the item as its texture
  [B7]. Every shipped mark item names an existing `textures\<item>.dds` (53 names) [D8].

Every sound item of the shipped file names an existing `sounds\<item>.ogg` (1 breaking, 93 step, 111 collide
names) [D8].

## 5. Finding the pair of two materials

After all pairs are read [B8]:

- With *N* the number of materials (57 in the shipped file), the game builds a table of *N* × *N* entries, all
  empty.
- For each pair in load order: each of its two material ids is turned into the **index** of the first material
  (in file order) with that id; the pair is stored in the entry (first index, second index) and in the entry
  (second index, first index). A later pair stored in the same entry replaces an earlier one.
- The pair of two materials is looked up by their two indices (e.g. a level triangle's material after the mapping
  of `level-cform.md` section 3.3): the entry, which is a pair or empty ("no pair"). An index ≥ *N* is a fatal
  error (assertion failure). The order of the two materials does not matter.
- A material id in a pair that no material has gives the index *N*, which addresses no proper entry; what follows is
  undefined in the original (a reader should reject such a pair) [B8]. No shipped pair has one [D3].

The pair's own id plays no part in the lookup [B8, B9].

In the shipped file [D3]: both material ids of every pair belong to the 57 materials; no two pairs name the
same two materials (in either order), so no entry is replaced; 29 pairs pair a material with itself (the first
id is smaller in 759 pairs, larger in 459); 1,247 of the 1,653 unordered combinations of two materials (a
material with itself included) have a pair and 406 have none. The pair ids are unique and ascending in file
order, 72 … 1,397 [D4].

## 6. Linked pairs

- The game does not resolve links: loading stores the link and the fifth word and does nothing else with them,
  and after loading the pairs are reached only through the table of section 5 [B9]. A pair's lists are therefore
  its own stored strings (believed: no other reader of the link or the fifth word was found; the check covers the
  code that holds the pair list and the table, not every possible use of a pair).
- For the shipped data it makes no difference: in every linked pair, each list whose bit in the fifth word is
  clear is equal to the same list of the pair it links to, the bits being 0x2 (breaking), 0x4 (step), 0x10
  (collide sounds), 0x20 (collide particles), 0x40 (collide marks) [D6]. Following the links and taking the
  linked pair's lists wherever the bit is clear would give every pair exactly its stored lists (by induction
  along each chain, as every chain ends at an unlinked pair). An implementation reads each pair's stored lists
  and ignores the link and the fifth word.
- Shipped data [D5, D6]: 372 pairs have the link 0xFFFFFFFF, and all of them the fifth word 0xFFFFFFFF; 875
  pairs link to the id of another existing pair (never their own). Chains end at an unlinked pair after 1 (750
  pairs), 2 (102) or 3 (23) links, with no cycles; the linked pair comes earlier in the file in 807 cases and
  later in 68. Fifth words of linked pairs: 0 (600), 0x4 (18), 0x10 (47), 0x14 (5), 0x20 (10), 0x24 (15), 0x30
  (117), 0x34 (60), 0x50, 0x76 and 0xFFFFFFBD (1 each). Lists that differ from the linked pair's: 1 breaking,
  66 step, 91 collide sounds, 93 collide particles, 2 collide marks, every one with its bit set; lists with the
  bit set: 1, 100, 232, 204, 2 respectively.
- The meaning of the other bits of the fifth word is **unestablished** (the game does not use the word).

## 7. Verification facts

All from the Russian edition's `gamemtl.xr` (section 1) and the `level.cform` files of the 30 level directories
of `gamedata.db0` … `gamedata.dbd`.

- **Canonical text.** For each pair in file order, one line: pair id, first material id, second material id,
  link, fifth word (all as unsigned decimal), then the breaking, step, collide sound, collide particle and
  collide mark strings exactly as stored (without their zero bytes), the ten fields separated by a tab and the
  line ended by LF. The whole text is 252,322 bytes with MD5 `57522a1ae37621a8fa33228ce9a21a0b` [D9].
- **Test vectors** [D11]:
  - Pair 1211: materials 90 (`creatures\actor`) / 7 (`materials\earth`), link and fifth word 0xFFFFFFFF; step
    list `material\actor\step\earth1,material\actor\step\earth2,material\actor\step\earth3,material\actor\step\earth4`
    (4 sounds); breaking, collide sound, particle and mark lists empty.
  - Sub-chunk 100 is pair 172: materials 3 / 71, linked to pair 85 (materials 0 / 71), fifth word 0x30.
  - Pair 1051 (`objects\glass` / `materials\asphalt`): linked to pair 1047, fifth word 0x76.
- **Levels** [D10]: in each of the 30 levels, every material id used by `level.cform` has a pair with
  `creatures\actor` (id 90). That pair's step list is non-empty, except for `materials\earth_death` in the three
  multiplayer levels `testers_mp_factory`, `testers_mp_railroad` and `testers_mp_workshop`.
- **`l01_escape`** [D10]: its 22 material ids and the ids of their pairs with `creatures\actor`: 0 → 1204,
  2 → 1206, 3 → 1207, 4 → 1208, 5 → 1209, 6 → 1210, 7 → 1211, 8 → 1212, 10 → 1213, 12 → 1215, 13 → 1216,
  18 → 1219, 19 → 1220, 34 → 1223, 37 → 1224, 39 → 1226, 48 → 1227, 49 → 1228, 53 → 1229, 60 → 1231, 80 → 1249,
  81 → 1250. The canonical lines of these 22 pairs, in file order, have the MD5
  `5ed900ed7bf672191be953f738b55a0e`.
- A parser written from sections 1 – 6 decodes every pair, builds the table of section 5 and reproduces all of
  this section [P].

## 8. Unestablished and believed

- Believed: nothing else in the program reads a pair's link or fifth word (section 6); this does not change the
  values for the shipped data.
- Believed: particle items name particle effects (section 4).
- Unestablished: the meaning of the bits of the fifth word other than those of section 6.
- Not covered here (for the specifications that play the sounds): which sound of a list plays, how the pair is
  chosen for a contact, step, break or bullet hit, and what is done when the lookup finds no pair.

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\xrCore.dll`, `bin\xrSound.dll`.
- Data examined: `gamemtl.xr` (all materials and pairs, decoded and compared), `particles.xr` (searched for the
  particle names), the archive listing (existence of sound and texture files) and the `level.cform` files of all
  30 level directories (material ids used).
- Binaries: static analysis (Ghidra 12.2, headless) of the game library's loading of the material library
  (pairs, lists, the lookup table and its release), of the core library's list-item functions and of the sound
  library's sound creation. No execution or debugging.
- A throwaway parser written from this text decoded every pair and reproduced section 7.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What loading makes of an empty item (a list such as `a,,b`, or an item of only bytes 0x00 – 0x20): the sound
  created from an empty name, or the mark with an empty texture, is not stated.
- What happens when the top-level chunk 0x1003 is absent (seen in review, without a check: the game then reads no pairs
  and reports no error), and when a pair's header sub-chunk is shorter than 20 bytes or a string lacks its zero byte.
