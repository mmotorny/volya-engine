# Saved games (`.sav`)

Functional specification of the saved-game files of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): when the game writes them and how they are named, the container (header and compression),
every chunk of the decompressed body and the records inside it, which game system each part belongs
to, how object records relate to the records of `spawns\all.spawn`, what loading a save does and
needs, and the picture file written next to some saves. It is written so that a reader and a writer
of saves the original game loads can be built from it; what could not be established is marked
**unestablished**, and what is inferred rather than read from the program or the data is marked
**believed**.

No saved game was available and the original game was not run: everything here was established by
static analysis of the shipped binaries and scripts, and checked, where the data allows, against
`spawns\all.spawn`, whose object records use the same encoding as the object records of a save
(section 7). Statements about what the game does at run time are therefore **static** (read from
the program, not observed); none is measured.

Conventions:

- Integers are little-endian; `u8`/`u16`/`u32`/`u64` unsigned, `s32` signed. `float` is an IEEE 754
  single. `float3` is three floats (x, y, z). `stringZ` is a zero-terminated byte string
  (Windows-1251); an empty string is the single byte 0. `bool8` is a `u8` that is 0 or 1.
- Sizes are in bytes. Paths are stored paths of the combined game-data tree, `\` separating
  directories; path aliases (`$game_saves$`, `$game_spawn$`, `$app_data_root$`, …) are those of
  `fsgame.ltx` (LTX specification, section 10).
- "Chunk", "chunk sequence", "sub-chunk" and "lookup by id" are those of the chunk-container
  specification (`docs/formats/chunks.md`). No chunk of a save is stored with the compressed flag;
  the whole body is compressed instead (section 2).
- Game time values are `u64` counts of game milliseconds (section 12).
- A "packet" is a byte string produced and consumed field by field with no padding or alignment.
  `q8(v, lo, hi)` denotes a `u8` holding `v` quantised to the range `lo … hi` (section 7.4.1).
- Object kinds are named by the `class` value of the object's configuration section (an LTX
  section of the `system.ltx` tree, e.g. `class = AI_STL_S`), here called the **class id**.
- `<user>` is the engine's user name (the program imports `GetUserNameA`; that this is the Windows
  logon name is believed, not established).

## 1. Files

### 1.1 Location and names

A saved game named `N` is the file `$game_saves$` + `N` + `.sav`. Next to it the game may write the
picture `$game_saves$` + `N` + `.dds` (section 13). `$game_saves$` is the saved-games directory of
`fsgame.ltx` (`$app_data_root$` + `savedgames\`; LTX specification, section 10).

The game writes a save in these situations; all use the same writer (sections 2–9):

| Situation | Name `N` | Picture |
|---|---|---|
| Console command `save <name>` (the save menu uses it) | `<name>` as given | yes |
| Console command `save` with an empty argument (quick save) | `<user>_quicksave` | yes |
| Periodic automatic save (below) | `<user>_autosave` | yes |
| The player uses a level changer (section 1.2) | `<user>_autosave` | no |
| Start of a New Game | the game name, i.e. the first server parameter `all` (level-loading specification section 2.1): the new simulation is saved as `all.sav` immediately after it is created, before the level is shown | no |

A name given to `save` is rejected ("invalid file name") if it contains any of `/ \ : * ? " < > |`.
The `save` command does nothing outside single-player games ("for single-mode only") and refuses
to save when the player character's health is not above 0 ("cannot make saved game because actor
is dead :("). An existing file of the same name is overwritten (believed).

**Periodic automatic save.** `config\alife.ltx` section `[alife]` gives `autosave_interval =
01:05:00` and `delay_autosave_interval = 00:00:30` (hours:minutes:seconds, converted to
milliseconds of the engine's frame clock; that this clock is real time is believed). While a
single-player simulation runs, once `autosave_interval` has passed since the last automatic save
the game saves if the player character exists, its health is above 0 and a further suppression
counter of the program is 0 (what sets that counter is **unestablished**); otherwise it tries again
after `delay_autosave_interval`.

The same directory also receives camera demo recordings (`.xrdemo`, console `demo_record`); they are
not part of saved games.

### 1.2 Level-change save

When the player uses a level changer, the single-player server handles the change once per session
(later requests are ignored) and writes a save before the new level is entered (static):

1. As before every save, the current state of the online objects is collected (section 8).
2. The player's object is moved to the destination received from the client: game-graph vertex,
   level-graph node, position and direction (believed: the level changer's destination fields,
   section 7.4.2, which the client copies). The position is the spawn packet's position (section
   7.2), the direction its angles; the update data's yaw becomes the direction's y, its pitch the
   direction's x and its roll 0 (section 7.4.1, creature update); model yaw and the offline path
   distance are not changed. If the player is in a vehicle or mounted weapon (holder id of the
   player state, section 7.4.2, not 0xFFFF), that object receives the same game vertex, level node,
   position and angles.
3. The game name in the server options is replaced by `<user>_autosave`, keeping the rest from the
   first `/` (the running simulation keeps only the first three server parameters, so
   `all/single/alife` becomes `<user>_autosave/single/alife`), and the save
   `<user>_autosave` is written. No picture is taken.
4. The moved objects get their previous location back in the running simulation.
5. The client then restarts the server with the rewritten options; the simulation loads
   `<user>_autosave.sav` (section 11.2), so the new level is entered from that save, with the
   player at the destination, which lies on the new level.

A writer that wants to reproduce this save applies step 2 to the player's records before writing.

### 1.3 Last save

The name of the most recent save written (every situation of section 1.1, including the New Game
and level-change saves) or started from is remembered, and the console command `load_last_save`
without an argument loads it. It is persisted in the user configuration file (`$app_data_root$` +
`user.ltx`) as the console line `load_last_save <name>`, written when the console configuration is
stored (at exit and by `cfg_save`) and only if a name is remembered; `load_last_save <name>` with an
argument (as executed when that file is loaded) only sets the remembered name. The title-menu
specification (section 4.1) describes how the menu uses it.

## 2. Container

A `.sav` file is:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `u32` marker `0xFFFFFFFF` |
| 4 | 4 | `u32` container version; the game writes **3** |
| 8 | 4 | `u32` `n`: length of the decompressed body |
| 12 | rest | the body compressed with LZO1X (section 2.1); its length is the file size − 12 |

A file is accepted as a saved game ("valid") only if it is at least 8 bytes long, starts with
`0xFFFFFFFF` and its container version is greater than 1. Otherwise loading stops with "Saved game
version mismatch or saved game is corrupted", and the load menu treats the file as invalid
(section 11.4). No other check is made: there is no checksum, and the decompressor does not check
its input or report errors.

The decompressed body is exactly `n` bytes and is a chunk sequence (section 3). The reader treats
the body as `n` bytes long whatever the decompressor produced, so a writer must give the exact
length.

### 2.1 Compression: LZO1X

The body is compressed with the public LZO1X method (the format of LZO1X-1 compressors; the game's
own compressor is the 1x-1 variant with a 2^14-entry dictionary). Any stream in the format below
decodes correctly; a writer need not reproduce the game's choice of matches.

The stream is a sequence of instructions that either copy **literals** (bytes taken from the
stream) or copy a **match** (bytes repeated from earlier output, `length` bytes starting `distance`
bytes back from the current end of output; a match may overlap its own output, i.e. copy byte by
byte from the start, so `distance` < `length` repeats). The decoder keeps a state `S`, the number of
literals copied by the previous instruction, which decides how a following instruction byte below
16 is read (`S` = 0, 1–3, or "4" meaning a literal run of 4 or more).

An **extended length** with base `B` is read as: if the instruction's length field is non-zero it
is the length; otherwise count the following zero bytes `z` (each adds 255), then take the next
(non-zero) byte `b`; the length is `B + 255·z + b`.

Decoding:

1. **First byte.** If the first byte `t` is greater than 17, copy `t − 17` literals; then `S` = that
   count if it is below 4, otherwise `S` = 4. Otherwise (first byte ≤ 17) start with `S` = 0 and
   decode the first byte as an ordinary instruction.
2. **Instruction byte `t`**, by range:
   - `t` ≥ 64: match, `length = (t >> 5) + 1` (3 … 8), `distance = ((t >> 2) & 7) + 8·b + 1`, where
     `b` is the next byte. Trailing literal count `L = t & 3`.
   - 32 ≤ `t` < 64: match, `length = (t & 31)` extended with base 31, plus 2; then a `u16` `D`;
     `distance = (D >> 2) + 1`; `L = D & 3`.
   - 16 ≤ `t` < 32: match or end: `length = (t & 7)` extended with base 7, plus 2; then a `u16` `D`;
     `h = ((t & 8) << 11) + (D >> 2)`. If `h` = 0 the stream **ends** here (nothing is copied). Otherwise
     it is a match with `distance = 16384 + h` and `L = D & 3`.
   - `t` < 16 and `S` = 0: literal run of `(t` extended with base 15`) + 3` bytes; then `S` = 4 and
     the next instruction is read (no trailing literals).
   - `t` < 16 and `S` = 1 … 3: match, `length = 2`, `distance = (t >> 2) + 4·b + 1` (`b` = next byte);
     `L = t & 3`.
   - `t` < 16 and `S` = 4: match, `length = 3`, `distance = (t >> 2) + 4·b + 2049`; `L = t & 3`.
3. After a match, copy `L` literals (0 … 3) and set `S = L`. Continue with step 2.

The end marker the game writes is the three bytes `11 00 00`. The game's compressor stores inputs
shorter than 14 bytes as one literal run followed by the end marker (e.g. the single byte `61` is
stored as `12 61 11 00 00`).

Test vector (an LZO1X-1 stream of 110 bytes that decodes to 244 bytes): the hex string

    0c532e542e412e4c2e4b2e452e522e20204838000031000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f616c6c200b08000a6c616c6c616c6c616c6c616c6c110000

decodes to the 15-byte ASCII string `S.T.A.L.K.E.R. ` (ending in a space) repeated 8 times, then the 64 bytes
`00 01 02 … 3F`, then `all` repeated 20 times.

## 3. The decompressed body

The body is a top-level chunk sequence. The game writes exactly these five chunks, in this order:

| Order | Chunk id | Content | System |
|---|---|---|---|
| 1 | 0 | simulation header (section 4) | offline simulation |
| 2 | 5 | game time (section 5) | game time |
| 3 | 1 | spawn binding (section 6) | object spawning |
| 4 | 2 | objects (sections 7–8) | all game objects |
| 5 | 9 | registries (section 9) | quests, relations, map, news, statistics |

Every chunk is found by lookup by id; order does not matter to the loader (section 11.2 gives the
order in which they are read). A chunk that is missing, or empty (an empty chunk cannot be told from
a missing one, chunk-container specification section 3), stops loading with an assertion whose text
names it: "Can't find chunk ALIFE_CHUNK_DATA" (chunk 0), "Cannot find chunk SPAWN_CHUNK_DATA!"
(chunk 1), "Can't find chunk OBJECT_CHUNK_DATA!" (chunk 2), "Can't find chunk GAME_TIME_CHUNK_DATA!"
(chunk 5), "Can't find chunk REGISTRY_CHUNK_DATA!" (chunk 9). No chunk is padded; the body ends
after chunk 9.

## 4. Chunk 0: simulation header

| Field | Type | Value |
|---|---|---|
| version | `u32` | the game writes 3; loading stops ("ALife version mismatch! (Delete saved game and try again)") if it is below 2 |

The value is consulted again when reading statistics records (section 9.10).

## 5. Chunk 5: game time

| Field | Type | Meaning |
|---|---|---|
| game time | `u64` | current game time in game milliseconds (section 12) |
| time factor | `float` | game-time speed: game milliseconds per real millisecond |
| normal time factor | `float` | the speed to return to after a temporary change |

16 bytes. At a New Game both factors come from `config\alife.ltx` section `[alife]` (`time_factor =
10`, `normal_time_factor = 10`) and the time from `start_date = 01.05.2012` (day.month.year) and
`start_time = 05:30:00`. Game time advances from the saved value at the saved factor after loading.

The load menu reads this chunk (with chunk 2 and the game graph, section 11.4) to show a save's
date.

## 6. Chunk 1: spawn binding

Chunk 1 is itself a chunk sequence of two sub-chunks:

| Sub-chunk | Content |
|---|---|
| 0 | `stringZ` spawn-file name (`all`), then the 16-byte identifier of that spawn file |
| 1 | one empty chunk (size 0) per spawn point of the spawn file, whose chunk **id** is the spawn point's object index, in increasing order (believed: one per object record of the spawn file's object list, as all.spawn's 8,641 records are its spawn points; ids 0 … 8,640, 8,641 chunks, 69,128 bytes) |

The 16-byte identifier is the first identifier of the spawn file's header (level-loading
specification section 5.10: header bytes 4 … 19 of
`spawns\all.spawn` chunk 0, i.e. `9a 80 c8 32 91 73 8f 42 b3 3b 35 90 3c ea d5 44`).

On load, `$game_spawn$` + name + `.spawn` is opened (missing: "Can't find spawn file:") and read as
for a New Game; its identifier must equal the saved one, otherwise loading stops with "Saved game
doesn't correspond to the spawn : DELETE SAVED GAME!". The spawn file's own checks apply as at a New
Game (its chunk 3 must exist: "Spawn version mismatch - REBUILD SPAWN!"; its second identifier must
equal `game.graph`'s: "Spawn doesn't correspond to the graph : REBUILD SPAWN!"). Sub-chunk 1 is
**not read** on load; a writer should reproduce it as described.

## 7. Chunk 2: objects

### 7.1 Layout

| Field | Type |
|---|---|
| object count `c` | `u32` |
| `c` object records | below |

Each object record:

| Field | Type |
|---|---|
| spawn-packet length `a` | `u16` |
| spawn packet | `a` bytes (section 7.2) |
| update-packet length `u` | `u16` |
| update packet | `u` bytes (section 7.3) |

These are the same two packets that `spawns\all.spawn` stores per object (start-position
specification section 2), without the chunk wrapping.

**Which objects are saved.** An object "can be saved" according to its kind:

- the player character (`O_ACTOR`): always, whatever its state flags;
- bolts (`II_BOLT`): never;
- physics objects, skeleton objects, vehicles and helicopters (`O_PHYSIC`, `P_DSTRBL`, `O_PHYS_S`,
  `P_SKELET`, `C_NIVA`, `SCRPTCAR`, `C_HLCPTR`, `C_HLCP_S`): if bit 0x08 of their skeleton flags
  (section 7.4.1, Skeleton) is clear, whatever their state flags;
- every other object: if bit `0x200` of its state flags (section 7.4.1, Object) is set.

**Which objects, in which order.** Every object of the world (online or offline, on any level) that
has no parent and can be saved is written, in increasing order of object id; each such object is
immediately followed by its children (objects whose parent id is its id, e.g. the items in an
inventory), recursively, in the order the parent lists them, each child only if it too can be
saved. `c` counts all records written.

The **player's object must be the first record**: the load menu reads only the first record and
treats it as the player character (its health and level, section 11.4); a writer must therefore
give the player the smallest id among parentless saved objects. The player's record must also have
spawn flags bit 0x08 (section 7.2) and direct control 1 (section 7.4.1): on load that is what marks
it as the simulation's player (a record of any other kind with both stops loading with an
assertion), and the player's game-graph vertex decides the
current level (section 11.2).

**Reading a record** (on load, section 11.2): the spawn packet's message type must be 1 and the
update packet's 0 (otherwise loading stops with an assertion); the object
kind named by the section's `class` is created (section 7.4.2; a kind that is not a simulation
object stops loading: "Non-ALife object in the saved game!"), the spawn packet is read into it
(section 7.2) and then the update packet, and the object is registered under the id stored in its
spawn packet.

### 7.2 Spawn packet

Read and written in this order:

| Field | Type | Notes |
|---|---|---|
| message type | `u16` | 1 |
| section | `stringZ` | the object's configuration section |
| name | `stringZ` | unique object name (all.spawn: e.g. `level_prefix_actor_0001`) |
| game type | `u8` | 0 in all.spawn; meaning **unestablished** (believed: the game mode the record belongs to) |
| `u8` | `u8` | 254 in all.spawn; meaning **unestablished** |
| position | `float3` | world position (start-position specification section 1) |
| angles | `float3` | orientation (start-position specification section 5) |
| respawn time | `u16` | 0 in all.spawn; meaning beyond the name **unestablished** |
| id | `u16` | the object's id (0 … 0xFFFE); 0xFFFF in all.spawn (assigned when the object is created) |
| parent id | `u16` | id of the containing object (inventory owner, box, …) or 0xFFFF for none |
| phantom id | `u16` | 0xFFFF in all.spawn; meaning **unestablished** |
| flags | `u16` | bit 0x20: a version follows; bit 0x01: "local"; bits 0x01 and 0x08 together on the player's record: the player-controlled entity (section 7.1); bit 0x02: meaning **unestablished**. all.spawn: 0x0021 in 8,629 records, 0x0023 in 11, 0x0029 for the player |
| version | `u16` | 118 when written by this game (all records of all.spawn) |
| script version | `u16` | written as the value of `current_server_entity_version` in section `[script]` of `config\system.ltx`: **7** in this edition (the winning copy, dbc; db0 has 5, dbb 6), so every record this game writes carries 7 (static); all.spawn records carry 6. Scripted object kinds use the record's value to decide which of their fields to read (section 7.5) |
| client-data length `k` | `u16` | (a `u8` for versions below 94) |
| client data | `k` bytes | state of the object's interactive (online) counterpart, section 8; empty in all.spawn |
| spawn id | `u16` | object index of the all.spawn record the object was created from (section 10) |
| data size `s` | `u16` | `s` counts itself, i.e. `s` = 2 + length of the object state |
| object state | `s − 2` bytes | layout by object kind, section 7.4 |

Writing: the game sets flags bit 0x20 (also in the object itself) and, in saves, bit 0x01; every
other bit is written as the object has it (so the player keeps 0x08). It requires `s` > 2 (object
state not empty) for every kind except the spectator, otherwise it stops ("object isn't successfully
saved, get your backup :("). The reader applies the same rule ("cannot read object, which is not
successfully saved :(").

Reading: if flags bit 0x20 is clear (no version is read), or the version is 0, the reader does not
read the rest of the record (it treats it as invalid). Versions below 118 have legacy fields not described here; every
record this game writes has 118, and readers of version 118 records see exactly the fields above.
(For versions below 70 the script version is absent, below 71 the client-data block, below 80 the
spawn id; versions 83 … 111 carry further fields before the data size; none occurs.)

### 7.3 Update packet

| Field | Type |
|---|---|
| message type | `u16` = 0 |
| object update data | rest of the packet; layout by object kind (section 7.4) |

The update data holds frequently changing state (health, position as last simulated, ammunition, …);
for many kinds it is empty (the packet is then 2 bytes).

### 7.4 Object state and update layouts

Layouts are built from the blocks below. All fields listed are present; there are no optional
fields except where a condition is stated. Types and order are established; field names are
believed from how the values are used (configuration keys, script interfaces, values in
all.spawn). The kinds that occur in all.spawn are verified on its records (section 15); the others
(phantom, PDA, document, torch, knife, zone, team base, stationary gun, online/offline group,
monster group) are established statically only. "State" goes into the spawn packet's object-state
area, "update" into the update packet after its message type.

#### 7.4.1 Blocks

**Object** (every simulation object):

| Field | Type | Meaning |
|---|---|---|
| game-graph vertex | `u16` | offline location (vertex of `game.graph`) |
| distance | `float` | distance along the current offline path edge (all.spawn: 0.7 for the actor) |
| direct control | `u32` | 0 or 1 (all.spawn actor: 1); an object with 0 is not entered into the simulation's location registry on load (static; meaning beyond that **unestablished**) |
| level-graph node | `u32` | node of the level's AI navigation graph |
| state flags | `u32` | bit 0x200: can be saved (section 7.1); other bits: believed to control online/offline switching (0x02 may switch online, 0x04 may switch offline) and AI visibility/use (0x08, 0x10, 0x20, 0x80); **unestablished** in detail |
| custom data | `stringZ` | per-object LTX text (e.g. `[logic]` sections for scripts) |
| story id | `s32` | −1 if none |
| spawn story id | `s32` | −1 if none |

**Visual**: `stringZ` model (e.g. `actors\hero\stalker_novice`), `u8` visual flags.

**Shape** (restrictors, zones): `u8` count, then per shape `u8` type: type 0 = sphere: `float3`
centre, `float` radius; type 1 = box: four `float3` rows (the box's 4×3 transform: three axes, then
the centre). Other types do not occur.

**Skeleton** (physically simulated skeletons): `stringZ` startup animation, `u8` skeleton flags,
`u16` source id; if skeleton flags bit 0x04 is set, saved **bone state** follows:

| Field | Type |
|---|---|
| bones mask | `u64` |
| root bone | `u16` |
| bounding box min | `float3` |
| bounding box max | `float3` |
| bone count | `u16` |
| per bone: position | 3 × `q8`, axis by axis in the box min … max |
| per bone: rotation | 4 × `q8(…, −1, 1)` quaternion components (which is the scalar part is **unestablished**) |
| per bone: enabled | `u8` |

Skeleton flags bit 0x08 decides whether physics objects, skeleton objects, vehicles and helicopters
are saved (section 7.1). Bits 0x04 and 0x08 are clear in all.spawn.

**Creature** = Object, Visual, then:

| Field | Type |
|---|---|
| team, squad, group | 3 × `u8` |
| health | `float` (0 = dead) |
| dynamic out-restrictions | `u32` count + count × `u16` object ids |
| dynamic in-restrictions | `u32` count + count × `u16` object ids |
| killer id | `u16` (0xFFFF none) |
| death time | `u64` game time |

**Monster-like** = Creature, then `stringZ` out-restrictions, `stringZ` in-restrictions (comma
separated restrictor names), `u16` smart-terrain id (0xFFFF none), `u8` smart-terrain task reached.

**Trader-like** (characters that own money and a profile):

| Field | Type |
|---|---|
| money | `u32` |
| specific character | `stringZ` |
| trader flags | `u32` |
| character profile | `stringZ` |
| community | `s32` (index; −1 none) |
| rank | `s32` |
| reputation | `s32` |
| character name | `stringZ` |

**Item** = Object, Visual, `float` condition (0 … 1).

**Item update** (physics state of an item):

| Field | Type | Condition |
|---|---|---|
| `f` | `u8` | bits 0–4: number of physics elements; 0x20: a further state flag of the item (believed: physics active); 0x40: the first velocity is zero and omitted; 0x80: the second velocity is zero and omitted |
| position | `float3` | if `f` ≠ 0 |
| rotation | 4 × `q8(…, 0, 1)` quaternion components | if `f` ≠ 0 |
| first velocity (believed: angular) | 3 × `q8(…, 0, 20π)` | if `f` ≠ 0 and bit 0x40 clear |
| second velocity (believed: linear) | 3 × `q8(…, −32, 32)` | if `f` ≠ 0 and bit 0x80 clear |

(`20π` = 62.831856.) If `f` = 0 the block is the single byte 0.

**Creature update**:

| Field | Type |
|---|---|
| health | `float` |
| timestamp | `u32` |
| flags | `u8` |
| position | `float3` (the same value as the spawn packet's position) |
| model yaw, yaw, pitch, roll | 4 × `float` |
| team, squad, group | 3 × `u8` |

**Monster-like update** = Creature update, `u16` next game vertex, `u16` previous game vertex, `float`
distance from the previous vertex, `float` distance to the next vertex (offline movement).

**Weapon state** = Item, `u16` ammo in reserve, `u16` ammo in magazine, `u8` weapon state, `u8`
installed addons, `u8` ammo type.

**Weapon update** = Item update, `q8(condition, 0, 1)`, `u8` weapon flags, `u16` ammo in magazine,
`u8` addons, `u8` ammo type, `u8` weapon state, `u8` zoom.

**Zone** = Object, Shape, `u8` restrictor type, `float` maximum power, `u32` owner id, `u32` enabled
time, `u32` disabled time, `u32` start-time shift.

**Anomaly** = Zone, `float` offline interactive radius, `u16` artefact spawn count, `u32` artefact
position offset.

`q8(v, lo, hi)` is written as `floor((v − lo) / (hi − lo) · 255 + 0.5)` (as a byte; `v` within the
range); a reader recovers `byte / 255 · (hi − lo) + lo` (believed; the read side's rounding was not
examined and does not affect the format).

#### 7.4.2 Layout per class id

State and update per class id of the simulation object kinds (scripted ones from
`scripts\class_registrator.script` marked †). "—" means empty. Every class id used by a section of
the `system.ltx` tree is either listed here or in section 7.4.3 with the reason it cannot occur in
a save.

| Kind | Class ids | State | Update |
|---|---|---|---|
| player | `O_ACTOR` | Creature, Trader-like, Skeleton, `u16` holder id (vehicle or mounted weapon in use, 0xFFFF none) | Creature update, then the player block below |
| stalker | `AI_STL`, `AI_STL_S`† | Trader-like, Monster-like, `u32` count + count × `u8` equipment preferences (count 5 in all.spawn, values 0–2), `u32` count + count × `u8` main-weapon preferences (count 4, values 0–2), Skeleton | Monster-like update, `stringZ` start dialog |
| trader | `AI_TRADE`, `AI_TRD_S`† | Object, Visual, Trader-like | — |
| monster | `AI_FLESH`, `AI_HIMER`, `AI_DOG_R`, `AI_BLOOD`, `AI_BOAR`, `AI_DOG_B`, `AI_DOG_P`, `AI_DOG_F`, `AI_BURER`, `AI_GIANT`, `AI_CONTR`, `AI_POLTR`, `AI_ZOM`, `AI_FRACT`, `AI_SNORK`, `AI_CAT`, `AI_TUSH`, `SM_*`† (every `SM_` id, including `SM_ZOMBI`) | Monster-like, Skeleton, `u16` special object id | Monster-like update |
| crow, phantom | `AI_CROW`, `AI_PHANT` | Creature | Creature update |
| vehicle | `C_NIVA`, `SCRPTCAR`† | Object, Visual, Skeleton, `float` health | — |
| helicopter | `C_HLCPTR`, `C_HLCP_S`† | Object, Visual, `stringZ` motion, Skeleton, `stringZ` startup animation, `stringZ` engine sound | — |
| item | `W_SCOPE`, `W_SILENC`, `W_GLAUNC`, `WP_SCOPE`†, `II_MEDKI`, `II_BANDG`, `II_ANTIR`, `II_FOOD`, `II_BOTTL`, `II_ATTCH`, `II_BOLT` (never saved, section 7.1), `II_EXPLO`, `G_F1`, `G_RGD5`, `D_SIMDET`, `MP_PLBAG`, artefacts `AF_*`, `ARTEFACT`, `SCRPTART`† | Item | Item update |
| torch | `D_TORCH`, `TORCH_S`† | Item | Item update, `u8` (bit 0: on, bit 1: night vision on, bit 2: **unestablished**) |
| PDA | `D_PDA` | Item, `u16` original owner id, `stringZ` specific character, `stringZ` info portion | Item update |
| document | `II_DOC` | Item, `stringZ` info portion | Item update |
| outfit | `EQU_SCIE`, `EQU_STLK`, `EQU_MLTR`, `EQU_EXO`, `E_STLK`† | Item | Item update, `q8(condition, 0, 1)` |
| ammunition | `AMMO`, `A_VOG25`, `A_OG7B`, `A_M209` | Item, `u16` rounds left | Item update, `u16` rounds left |
| knife | `W_KNIFE`, `WP_KNIFE`† | Weapon state | Weapon update |
| magazine weapon | `W_WMAGAZ`, `W_LR300`, `W_HPSA`, `W_PM`, `W_FORT`, `W_BINOC`, `W_SVD`, `W_SVU`, `W_RPG7`, `W_VAL`, `W_VINT`, `W_WALTHR`, `W_USP45`, `WP_LR300`†, `WP_BINOC`†, `WP_SVD`†, `WP_HPSA`†, `WP_PM`†, `WP_RPG7`†, `WP_SVU`†, `WP_USP45`†, `WP_VAL`†, `WP_VINT`†, `WP_WALTH`† | Weapon state | Weapon update, `u8` fire mode |
| weapon with grenade launcher | `W_WMAGGL`, `W_FN2000`, `W_AK74`, `W_GROZA`, `WP_AK74`†, `WP_GROZA`† | Weapon state | `bool8` grenade mode, Weapon update, `u8` fire mode |
| shotgun | `W_SHOTGN`, `W_BM16`, `W_RG6`, `WP_BM16`†, `WP_RG6`†, `WP_SHOTG`† | Weapon state | Weapon update, `u8` fire mode, `u8` count + count × `u8` ammo types of the loaded rounds |
| zone | `Z_ZONE` | Zone | — |
| anomaly | `Z_MBALD`, `Z_MINCER`, `Z_ACIDF`, `Z_GALANT`, `Z_RADIO`, `Z_DEAD`, `Z_NOGRAV`, `ZS_MBALD`†, `ZS_GALAN`†, `ZS_MINCE`† | Anomaly | — |
| visual anomaly | `Z_BFUZZ`, `Z_RUSTYH`, `Z_AMEBA`, `ZS_BFUZZ`† | Anomaly, Visual, `stringZ` idle animation, `stringZ` attack animation | — |
| torrid zone | `Z_TORRID` | Zone, `stringZ` motion | — |
| level changer | `LVL_CHNG` | Object, Shape, `u8` restrictor type, `u16` destination game vertex, `u32` destination level node, `float3` destination position, `float3` destination direction, `stringZ` destination level, `stringZ` destination graph point, `bool8` silent | — |
| restrictor | `SCRIPTZN`, `SPACE_RS`, `SPC_RS_S`† | Object, Shape, `u8` restrictor type | — |
| smart zone | `SMRTZONE`, `SMRTTRRN`†, `RE_SPAWN`† | Object, Shape, `u8` restrictor type | — |
| team base | `Z_TEAMBS` | Object, Shape, `u8` restrictor type, `u8` team | — |
| plain visual object | `O_SEARCH`, `W_MOUNTD`, `SCRPTOBJ`, `O_INVBOX` | Object, Visual | — |
| stationary gun | `W_STMGUN` | Object, Visual | `bool8` working, `float3` aim direction |
| hanging lamp | `O_HLAMP` | Object, Visual, Skeleton, then the lamp block below | — |
| physics object | `O_PHYSIC`, `P_DSTRBL`, `O_PHYS_S`† | Object, Visual, Skeleton, `u32` physics type, `float` mass, `stringZ` fixed bones | — |
| breakable | `O_BRKBL` | Object, Visual, `float` health | — |
| climbable | `O_CLMBL` | Object, Shape | — |
| skeleton object | `P_SKELET` | Object, Visual, Skeleton | — |
| online/offline group | `ON_OFF_G` | Object, `u32` count + count × `u16` member ids | — |
| monster group | `AI_FLE_G` (`flesh_group`) | the monster state (Monster-like, Skeleton, `u16` special object id), then `u32` flag (0/1), `u16` (meaning **unestablished**), `u32` count + count × `u16` member ids | Monster-like update, `u32` flag (the same flag) |

Hanging lamp block: `u32` colour, `float` brightness, `stringZ` colour animator, `float` range, `u16`
light flags, `stringZ` startup animation, `stringZ` fixed bones, `float` health, `float` virtual
size, `float` ambient radius, `float` ambient power, `stringZ` ambient texture, `stringZ` light
texture, `stringZ` light bone, `float` spot cone angle, `stringZ` glow texture, `float` glow radius,
`stringZ` ambient light bone.

Player update block, after the Creature update:

| Field | Type | Condition |
|---|---|---|
| movement state | `u16` | |
| acceleration | `u16` compressed direction + `float` magnitude | |
| velocity | `u16` compressed direction + `float` magnitude | |
| radiation | `float` | |
| active weapon | `u8` | |
| physics element count `m` | `u16` | |
| physics state | `u8` enabled, then 5 × `float3`, 4 × `float` | if `m` = 1 |
| interpolation data | `u8` `e`, then `m · e + 24` bytes | if `m` > 1 |

A vector is written as its direction (compressed into a `u16`) and its length; a vector of length
≤ 1e−7 is written as the direction (0, 0, 1) with length 0. The `u16` direction encoding is
**unestablished**; a reader may skip each such field as 6 bytes.
The physics-state floats and the interpolation data are **unestablished** in meaning. all.spawn has
`m` = 0.

#### 7.4.3 Class ids that cannot occur in a save

A save holds only objects of the offline simulation: the game saves the simulation's object
registry, and on load every record must create a simulation object ("Non-ALife object in the saved
game!", section 7.1). The remaining class ids of the configuration therefore never appear:

| Class id (example section) | Reason |
|---|---|
| `SPECT` (`spectator`) | a registered kind, but not a simulation object (multiplayer spectator); it is also the one kind exempt from the non-empty-state rule of section 7.2 (its state is empty) |
| `AI_GRAPH` (`graph_point`) | a registered kind, but not a simulation object; none occurs in all.spawn |
| `G_RPG7`, `G_FAKE` (rockets) | registered with a temporary server object that is not a simulation object |
| `O_SWITCH` (`trigger`) | registered by `scripts\class_registrator.script` to `se_switcher.cse_switcher`, whose base class (named in `scripts\se_switcher.script`) is a plain visual server object, not a simulation object |
| `AI_RAT` (`m_rat_e`), `AI_RAT_G` (`rat_group`), `AI_SPGRP` (`spawn_group`, `spawn_group_zone`), `EVENT` (`??tg_event`), `II_BTTCH` (`new_attachable_item`), `NW_ATTCH` (`custom_script_object`) | no object kind is registered for the id, neither by the program nor by `scripts\class_registrator.script`, so no object of such a section can be created (at a New Game, by scripts or on load); none occurs in all.spawn |

(`MAIN_MNU` in `class_registrator.script` is the main menu, a client-only kind; the `WP_MAGAZ`
registration there is commented out.)

### 7.5 Scripted state additions

Class ids marked † are given by `scripts\class_registrator.script` a scripted server class
(`scripts\se_*.script`, `scripts\smart_terrain.script`). Those listed below append fields after the
engine's state in their state writer and reader; the appended fields are part of the object state
(inside `s`). The other † kinds append nothing.

| Class ids | Appended after the engine state |
|---|---|
| `AI_STL_S` | `u8` job-online mode `j` (0, 1, 2; 3 = condition list); if `j` = 3: `stringZ` condition list; `bool8` was in a smart terrain; `bool8` death items dropped |
| `SM_*` (including `SM_ZOMBI`) | as `AI_STL_S` without the last `bool8` |
| `ZS_MBALD`, `ZS_GALAN`, `ZS_MINCE`, `ZS_BFUZZ` | in single-player games only (both writer and reader): `u8` `t`; if `t` = 1: a script time (below) of the last artefact spawn |
| `RE_SPAWN` | `u8` `n`, then `n` × `u16` ids of objects it spawned |
| `SMRTTRRN` | the fields of the smart-terrain table below |

Smart-terrain (`SMRTTRRN`) appended fields, in order (as written by the state writer of
`scripts\smart_terrain.script`):

| Field | Type | Present |
|---|---|---|
| duration end | script time | always |
| idle end | script time | always |
| working `w` | `bool8` | always |
| gulag common state | written by `save_common` of `scripts\xr_gulag.script` | only if `w` = 1 |
| member count `m` | `u8` | only if `w` = 1 |
| per member: id | `u16` | only if `w` = 1 |
| per member: group, squad | `u8`, `u8` | only if `w` = 1 |
| per member: exclusive | `bool8` | only if `w` = 1 |
| per member: stay end | script time | only if `w` = 1 |
| per member: gulag member state | written by `save_obj` of `scripts\xr_gulag.script` | only if `w` = 1 |
| population locked | `bool8` | only if `w` = 1 |

When not working the appended fields are just two script times and the byte 0 (all 282 `SMRTTRRN`
records of all.spawn: `00 00 00`). Members are written in the scripts' table order, which is not
defined.

A **script time** is `u8` year − 2000; if that byte is 0 the time is "unset" and nothing else
follows; otherwise `u8` month, `u8` day, `u8` hour, `u8` minute, `u8` second, `u16` millisecond
(as written by `w_CTime` of `scripts\utils.script`).

The scripted readers read the job-online byte only if the record's script version ≥ 3 (stalkers) or
≥ 4 (monsters), "was in a smart terrain" if ≥ 5, "death items dropped" if ≥ 6; both 6 (all.spawn)
and 7 (saves) therefore read every field. The writers always write all fields. The scripts of the
shipped data are authoritative for these fields; the table restates them.

## 8. Client data

The client-data block of a spawn packet (section 7.2) holds the state of the object's online
counterpart (physics, inventory slots, conditions, AI memory, script variables) as of the last time
the object was online. It is empty in all.spawn and for objects that were never online.

**An empty block is always acceptable**: when an object comes online the stored block is applied
only if it is non-empty; with an empty block the object starts as at a New Game. A writer that does
not model online state should write length 0.

**On load** an object's stored block is discarded, after the registries are read (section 11.2),
unless its top-level container (the object itself, or the outermost object reached through parent
ids) is registered on the current level (the level of the player's game-graph vertex), or the
object's kind keeps it. Every kind runs this rule: the engine's own kinds directly, and every
scripted class of the shipped scripts that defines `on_register` first calls its base class's
`on_register`. To decide "keeps it" the engine asks the object's `keep_saved_data_anyway`: for every
engine kind the answer is false; for a scripted kind (†, section 7.4.2) the question is passed to
the script method of that name. The shipped scripts answer true in four classes:

| Class id | Scripted class (file) |
|---|---|
| `SPC_RS_S` | `se_restrictor` (`scripts\se_zones.script`) |
| `O_PHYS_S` | `se_physic` (`scripts\se_item.script`) |
| `AI_TRD_S` | `se_trader` (`scripts\se_stalker.script`) |
| `C_HLCP_S` | `se_heli` (`scripts\se_car.script`) |

So after a load, client data survives only for objects on the player's level and for objects of
these four class ids on any level (static). That a scripted class without its own
`keep_saved_data_anyway` gets the engine's false answer is believed.

Before each save, the game collects a fresh block from every online object; offline objects keep
the block they have. The `k` bytes of the block (section 7.2) are, with no further length prefix:

| Part | Content |
|---|---|
| engine part | by kind, below |
| script part | written by the object's script binder (`save(packet)` of the binder class in `scripts\bind_*.script`), if the object has one; read back by its `load` |

Engine parts. The **physics flag** is a `u8`: 2 if the object has no physics shell (or its shell is
not active), otherwise 0 or 1 according to the shell's state (which state is **unestablished**).
Rows marked (believed) were read statically in an earlier pass and not re-checked in this revision.

| Kind | Engine part |
|---|---|
| player | physics flag, player condition block, inventory-owner block, `u8` |
| trader | physics flag, condition block, inventory-owner block |
| monster | physics flag, condition block; if its health is above 0: AI memory (**unestablished**) |
| stalker | monster part, inventory-owner block, further AI state (**unestablished**) |
| phantom | `u32` |
| plain objects, restrictors, zones, level changers, inventory boxes, projectors, script objects | — (empty) (believed) |
| physics object, skeleton object, stationary and mounted guns, hanging lamp; also vehicle, breakable, climbable, crow | physics flag (believed) |
| item, artefact, outfit, explosive, grenade, scope, silencer, launcher, detector, food, medkit | physics flag, item block (believed) |
| knife | physics flag, item block, `u32`, `u8`, `u32`, `u8` (weapon state; meanings **unestablished**) (believed) |
| magazine weapon, shotgun | knife part, then `u32`, `u32`, `u32` (believed) |
| weapon with grenade launcher | magazine-weapon part, `u8`, `u32` (count of loaded grenades; **unestablished**) (believed) |
| binoculars | magazine-weapon part, `u32` (believed) |
| PDA | physics flag, item block, `stringZ` (believed) |
| helicopter | **unestablished** |

Item block: `u8` item place, `float` condition, then `u8` physics-element flag (0: no physics state
follows; otherwise followed by the item's physics state, **unestablished**) (believed).

Condition block: `bool8` alive; if alive: 4 × `float` (believed: power, health-related values,
radiation), `u8` wound count `w`, then `w` wound records (**unestablished**). Player condition
block: the condition block, then `float`, `u16`, `float` (**unestablished**) (believed).

Inventory-owner block: `u8` active slot (0xFF none), `stringZ`, `stringZ`, `u32` (meanings
**unestablished**: believed to hold the talk/dialog and character name state).

Because the block is length-prefixed in the spawn packet, a reader can always skip it.

## 9. Chunk 9: registries

Chunk 9 is a flat byte stream (not a chunk sequence) of these parts, in this order. "Per object"
registries are maps keyed by object id: `u32` entry count, then per entry `u16` object id and the
value; entries are in increasing id order.

| # | Part | Value per entry |
|---|---|---|
| 1 | known info portions, per object | `u32` count, then per info portion: `stringZ` id, `u64` game time received |
| 2 | relations, per object | personal: `u32` count, per entry `u16` object id + `s32` goodwill; community: `u32` count, per entry `s32` community index + `s32` goodwill |
| 3 | talk contacts, per object | `u32` count, then per contact 16 raw bytes: `u64` game time, `u16` object id (0xFFFF none), 6 padding bytes (content arbitrary; write 0) |
| 4 | encyclopedia articles, per object | `u32` count, then per article: `u64` game time received, `stringZ` article id, `u8` read flag, `u32` article type |
| 5 | news, per object | `u32` count, then per item: `u32` news type, `stringZ` text, `u64` game time, `stringZ` texture, 4 × `float` texture rectangle |
| 6 | named counters | `u32` count, then per entry `stringZ` name and `s32` value (keyed by name, not object id; order not defined; meaning **unestablished**) |
| 7 | map locations, per object | `u32` count `n`, then `n` × (`u16` object id, `stringZ` location type, `u8` 0, location state: `u16` object id, `stringZ` hint, `u32` flags) |
| 8 | game tasks, per object | `u32` count, then per task the task record (9.8) |
| — | active task | `stringZ` id of the active task (empty if none), `u16` active objective index (0xFFFF none) |
| 9 | statistics, per object | `u32` count, then per section the statistics record (9.10) |

Map locations: only the locations whose flags have bit 0x01 set are written (and counted in `n`);
bit 0x01 is therefore set in every written location's `u32` flags. An object whose locations are all
unflagged still has its entry, with `n` = 0.

The types and order of all fields of this section are established; the field names are believed
from how the values are used (e.g. the XML element a task field is filled from). Which objects own
entries (believed: mostly the player; characters own their known info portions and relations) is
not fixed by the format.

### 9.8 Task record

| Field | Type |
|---|---|
| task id | `stringZ` (id in `config\gameplay\game_tasks.xml`) |
| received | `u64` game time |
| finished | `u64` game time |
| third time | `u64` game time (believed: time limit; **unestablished**) |
| title | `stringZ` |
| objective count | `u32` |
| objectives | objective records |

Objective record:

| Field | Type | Meaning |
|---|---|---|
| `u32` | | **unestablished** |
| state | `u32` | objective state (**unestablished** values) |
| text | `stringZ` | from `<text>` |
| map location type | `stringZ` | from `<map_location_type>` |
| object id | `u16` | target of the map location (from `<object_story_id>`), 0xFFFF none |
| state | `u32` | the same value as the second field |
| map location hidden | `u8` | |
| map location hint | `stringZ` | |
| icon texture | `stringZ` | |
| icon rectangle | 4 × `float` | x, y, width, height |
| article | `stringZ` | from `<article>` |
| info portions to complete | `u32` count + `stringZ` each | `<infoportion_complete>` |
| info portions to fail | `u32` count + `stringZ` each | `<infoportion_fail>` |
| info portions set on complete | `u32` count + `stringZ` each | `<infoportion_set_complete>` |
| info portions set on fail | `u32` count + `stringZ` each | `<infoportion_set_fail>` |
| has functions | `u8` | 1 if any of the four function lists is non-empty |
| function lists | 4 × (`u32` count + `stringZ` each) | only if "has functions": `<function_complete>`, `<function_fail>`, `<function_call_complete>`, `<function_call_fail>` |

### 9.10 Statistics record

| Field | Type |
|---|---|
| detail count | `u32` |
| details | per detail: `stringZ` key, `u32`, `u32`, then a `stringZ` (meanings of the `u32`s and the last string **unestablished**) |
| section key | `stringZ` |

The writer always writes every field. The reader reads each detail's last `stringZ` only if the
simulation header version (section 4) is greater than 2, and the section key only if that version
is not 2 (for version 2 it reads a `u32` legacy code instead). Saves of this game have version 3, so
both are present.

## 10. Relation to the spawn file

- `spawns\all.spawn` is the template of the world: at a New Game objects are created from its
  records (object state, update packet), receive ids, and from then on live in the saves. A save
  contains **all** saved objects; nothing is re-read from all.spawn's object records on load except
  to rebuild the spawn bookkeeping.
- An object created from an all.spawn record keeps that record's object index in its spawn
  packet's **spawn id** field (all.spawn: spawn id = object index in all 8,641 records). Its state
  layout is the all.spawn record's layout, with the id, parent id and flags changed as above.
- Objects created at run time (inventory items handed out from character supplies, scripted spawns)
  appear only in saves; their spawn id is **unestablished**.
- The save does not repeat all.spawn's other parts (header, chunks 2 and 3: artefact spawn data and
  patrol paths), which are re-read from the spawn file on load.

## 11. Loading

### 11.1 What loading needs

1. The `.sav` (section 2), with all five chunks.
2. The spawn file named in chunk 1 with the same identifier, and `game.graph` matching it.
3. The configuration sections of every object (`class` values must name simulation object kinds),
   the scripts of the scripted classes (they read their appended fields, section 7.5), and the
   level data of the level the player is on.

### 11.2 Sequence (static)

**Starting a load.** From outside a game (title menu, load menu without a running simulation,
`load_last_save`) the game starts the server with the options `<name>/single/alife/load` (load
screen: `scripts\ui_load_dialog.script`). The simulation copies `<name>` as the last save (section
1.3) and loads `<name>.sav`; if that fails with the fourth option `load`, it stops ("Cannot find the
specified saved game"); without a fourth option (the level-change restart of section 1.2) a missing
file starts a New Game under that name instead. In a running single-player game the console
command `load <name>` (used by the load screen then) first checks that the file exists and is valid
(section 2), then rewrites the game name in the server options to `<name>` (keeping the rest from
the first `/`) and has the client either reload the simulation in place, when the save's level
(section 11.4) is the current level, or restart the server with the rewritten options (static on
the server and on the client; that the request passes between them as for the level change of
section 1.2 is believed; how the in-place reload differs is **unestablished**).

**Reading the save**, in this order:

1. Chunk 0 (version check, section 4).
2. Chunk 5: the game time and both factors are set.
3. Chunk 1 and the spawn file (section 6).
4. Chunk 2: every record is read (section 7.1) and the object is entered into the object registry
   under its id.
5. First pass over all objects, in increasing id order: the object's id is reserved in the
   server's id pool, and the object is registered with the simulation's bookkeeping (offline
   location by game-graph vertex for objects with direct control and no parent, story ids,
   scheduling, containment by parent id). Registering the player (spawn flags bit 0x08 with direct
   control) sets up the current level: the level of the player's game-graph vertex, whose objects
   are then tracked as on that level.
6. Chunk 9 (section 9).
7. Second pass over all objects, in increasing id order: each object's post-registration step runs:
   the client-data rule of section 8, and for scripted kinds their scripts' `on_register`.

The client then connects and the level containing the player's object is loaded: the level of the
player's game-graph vertex (believed, as for New Game in the level-loading specification section
2.2; the load menu derives a save's level the same way). Objects on that level near the player are
brought online, applying their client data (section 8).

### 11.3 Writing a save

A save the loader accepts needs, besides the container (section 2): chunk 0 with 3; chunk 5; chunk 1
naming the spawn file with its identifier; chunk 2 with one record per object, each with a unique
id (0 … 0xFFFE), version 118, script version 7 (the value the game itself writes; records copied
from all.spawn keep their 6 unless re-encoded, which the scripted readers also accept, section 7.5),
flags bits 0x20 and 0x01 (plus 0x08, with direct control 1, for the player, section 7.1), a
non-empty object state laid out for its class id (sections 7.4, 7.5), a parent id that is 0xFFFF
or the id of a record written before it (children follow their parent), the player's record first,
and only objects that can be saved (section 7.1; e.g. no bolts); chunk 9 (every count may be 0, the
active task empty with index 0xFFFF). Client data may be empty for every object (section 8). The
picture is optional. Records copied from all.spawn need only their id and flags set. That the game
accepts such a save is unverified (section 16).

### 11.4 Load menu data

For listing, the game opens each save, decompresses it, reads chunk 5 (game time) and the first
object record of chunk 2 (with the record checks of section 7.1); the first record must be the
player's: its Creature health is shown, and the level of its game-graph vertex (`game.graph`). For a
file that is not valid (section 2) the menu shows the configured New Game start time
(`config\alife.ltx`), health 1.0 and the level of the first entry of the game graph's level table
(believed: the level with the smallest level id).

## 12. Game time

Game time is a `u64` count of milliseconds. A calendar date converts as

    days = (Y − 1)·365 + ⌊Y/4⌋ − ⌊Y/100⌋ + ⌊Y/400⌋ + (days of the months before M, with February
           counted as 29 if Y is a leap year) + (D − 1)
    time = ((days·24 + hour)·60 + minute)·60·1000 + second·1000 + millisecond

where Y is a leap year if divisible by 4 and not by 100, or by 400. (Note that the leap day of
year Y is counted by both the ⌊Y/4⌋ terms and the month sum; the formula is the game's.) The New
Game start 01.05.2012 05:30:00 is **63,471,533,400,000** (`u64` bytes `c0 9f 33 1e ba 39 00 00`).
The inverse conversion used for display is **unestablished**.

## 13. Picture file (`.dds`)

With each console `save` (manual or quick) and each periodic automatic save (section 1.1) the game
writes `$game_saves$` + name + `.dds`: a DirectDraw Surface (public format) of **128 × 128 pixels,
DXT1, one mip level**, the frame on screen (with the display gamma ramp applied) scaled down to that
size. It is taken on the next rendered frame after the save is requested, and only in full-screen
mode: in a windowed mode no picture is written (the renderer logs "~ Can't capture screen while in
windowed mode..."). The level-change save and the New Game save have no picture (believed: an
existing `<user>_autosave.dds` from an earlier periodic automatic save is left as it is). A missing
picture does not make a save invalid.

## 14. Game systems a save depends on

For planning, the systems whose state a save carries or whose behavior produces saves, with the
sections that describe them:

1. **Container and compression**: header, LZO1X (2).
2. **Offline simulation** ("A-life"): simulation header (4), game time and its speed (5), every
   object's offline location (game-graph vertex, level node, path progress), online/offline
   switching flags (7.4.1), the spawn binding (6), registration order and the current level on load
   (11.2).
3. **Object model**: object ids and parent/child containment, which objects are saved (7.1, 7.2),
   the per-kind state and update layouts (7.4), configuration sections and class ids.
4. **Spawn file and game graph**: reading `spawns\all.spawn` and `game.graph` (6, 10, 11).
5. **Player character**: health and conditions, money, profile and community, inventory, active
   weapon, movement and physics state (7.4.2 player, 8).
6. **Characters (stalkers, traders)**: profiles, ranks, reputation, communities, restrictions,
   smart-terrain membership, AI memory (7.4, 8).
7. **Monsters**: as characters without trading (7.4, 8).
8. **Items and weapons**: condition, ammunition, addons, inventory placement (7.4, 8).
9. **Anomalies, restrictors, level changers, smart terrains**: shapes, timings, destinations,
   artefact spawning (7.4, 7.5); level changes, which write a save (1.2).
10. **Physics**: item and skeleton physics states, bone states (7.4.1, 8).
11. **Info portions** (story flags), **relations**, **talk contacts**, **encyclopedia**, **news**,
    **map locations**, **tasks and the active task**, **statistics** (9).
12. **Scripting**: scripted server classes (7.5), script binders' saved data (8), scripted
    environment state kept there (e.g. weather, timers: they are saved by the scripts through the
    player's binder, not by a dedicated chunk).
13. **Console and user configuration**: the `save`, `load` and `load_last_save` commands, the
    automatic-save schedule and the persisted last save (1.1, 1.3, 11.2).
14. **Save picture** (13) and the **load menu** (11.4).

There is no separate chunk for weather, the level's static world or the camera: the weather state
is script data (`scripts\level_weathers.script` saves through the player's binder), the camera is
derived from the player.

## 15. Verification facts

From the game data (`spawns\all.spawn`, dbb, 4,474,073 bytes; `config\`; `scripts\`):

- Every one of the 8,641 all.spawn object records parses with the layouts of section 7.4–7.5 so
  that the object state ends exactly at the end of the spawn packet and the update data ends exactly
  at the end of the update packet. Counts per kind: physics object 1,905, anomaly 1,350, stalker
  1,073, restrictor 1,027, hanging lamp 745, item 459, breakable 406, smart zone 376, monster 276,
  plain visual object 256, climbable 232, crow 197, ammunition 96, visual anomaly 92, outfit 44,
  level changer 37, helicopter 27, magazine weapon 17, weapon with grenade launcher 13, vehicle 6,
  shotgun 4, player 1, trader 1, torrid zone 1.
- Update-packet lengths (including the 2-byte message type): player 63, stalker 55, monster 54,
  crow 42, items 3 (415 records: element byte 0) or 22 (44 records: element byte non-zero,
  position and rotation present), outfits 4 (42) or 23 (2), ammunition 5, magazine weapons 12,
  grenade-launcher weapons and shotguns 13, all other kinds 2.
- In all 8,641 records: game type 0, the following byte 254, respawn time 0, id 0xFFFF, parent id
  0xFFFF, phantom id 0xFFFF, version 118, script version 6, client-data length 0, spawn id = object
  index; flags 0x0021 (8,629 records), 0x0023 (11) or 0x0029 (the player, object index 63); every
  record's state flags have bit 0x200 set, and every record with a Skeleton block (physics object,
  stalker, hanging lamp, monster, helicopter, vehicle, player) has skeleton flags bits 0x04 and 0x08
  clear, so all would be saved. No record is a bolt, and no smart terrain is "working". Stalker
  preference counts are 5 and 4 in all 1,073 stalkers, values 0–2.
- The player's record: spawn packet 298 bytes, data size 222, game-graph vertex 4, distance 0.7,
  direct control 1, health 1.0, update `m` = 0.
- The configuration's sections use 127 class ids; each is in section 7.4.2 or 7.4.3.
- `config\system.ltx` (winning copy in dbc) `[script]` `current_server_entity_version = 7`
  (db0: 5, dbb: 6); all.spawn records carry 6, saves written by the game carry 7.
- `config\alife.ltx`: `time_factor = 10`, `normal_time_factor = 10`, `start_date = 01.05.2012`,
  `start_time = 05:30:00`, `autosave_interval = 01:05:00`, `delay_autosave_interval = 00:00:30`;
  game time 63,471,533,400,000.
- Spawn binding of a save of this edition: sub-chunk 0 is the 20 bytes `61 6c 6c 00` + `9a 80 c8 32
  91 73 8f 42 b3 3b 35 90 3c ea d5 44` (chunk payload 20 bytes); sub-chunk 1 is 8,641 empty chunks
  (69,128 bytes); chunk 1's payload is 8 + 20 + 8 + 69,128 = 69,164 bytes.
- The LZO1X test vector of section 2.1.

Invariants of any save written by this game: bytes 0–7 are `ff ff ff ff 03 00 00 00`; the body is
`n` bytes; chunk 0's payload is `03 00 00 00`; chunk 5 is 16 bytes; the first object record is the
player's (section `actor`, flags with bits 0x01 and 0x08); every spawn packet has version 118,
script version 7 and flags bits 0x20 and 0x01; every child record follows its parent; no record is
a bolt.

## 16. Unestablished

- Everything here is static; no save was produced or loaded. Whether the game accepts a save made
  from this text is unverified.
- The meaning of the game-type byte, the byte after it, the respawn time and phantom id, flags
  bit 0x02, and most state-flag bits.
- The meaning of several fields (monster-group `u16`, team-base team values, equipment preference
  contents, torch bit 2, task objective first field and state values, the third task time,
  statistics detail fields, named counters).
- The compressed-direction encoding and the physics fields of the player update; the quaternion
  component order; item physics state in client data.
- Most of the client data (section 8): wound records, AI memory, helicopter state, inventory-owner
  strings; the rows marked believed; the script binders' data is defined by the shipped scripts.
- The spawn id of objects created at run time; the exact level-selection rule on load (believed:
  the player's game vertex); what the in-place reload of section 11.2 skips.
- The inverse of the game-time conversion.

## 17. Provenance

- Date: 2026-11-24 (second revision the same day: the level-change save, the persisted last save,
  the can-save rules per kind, exact chunk assertion texts, the load order, legacy version ranges,
  the statistics conditions, the client-data discard rule on load, and corrections of the zombie,
  projectile, graph-point and team-base entries; third revision: the four scripted kinds that keep
  client data on load).
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\xrCore.dll`, `bin\XR_3DA.exe`,
  `bin\xrRender_R1.dll`.
- No saved game existed, and the original game was not run (no Windows environment was available).
- Binaries: static analysis (Ghidra 12.2, headless: disassembly and decompilation) of saving and
  loading of the simulation, the container and its compressor and decompressor, the object
  serialisation of every object kind and the object factory, the client-state collection and its
  use on load, the registries, the save, load and last-save console commands and the console
  configuration file, automatic saving, the level-change handling, the load-menu reader and the
  renderer's save picture.
- Data: `spawns\all.spawn` (all records parsed with the layouts above by a throwaway parser written
  from this text), `config\system.ltx` tree (class ids of every record and every section),
  `config\alife.ltx`, `scripts\class_registrator.script`, `scripts\se_*.script` (among them
  `scripts\se_stalker.script`, `scripts\se_car.script`, `scripts\se_zones.script` and
  `scripts\se_item.script` for `keep_saved_data_anyway`), `scripts\lua_help.script`,
  `scripts\smart_terrain.script`, `scripts\utils.script`, `scripts\ui_load_dialog.script`,
  `scripts\ui_main_menu.script`, `config\gameplay\game_tasks.xml`.
- The LZO1X description was checked by a decoder written from section 2.1 against the test vector
  and against 306 streams produced by an independent LZO1X-1 compressor (the publicly available LZO
  library) from varied inputs, including all.spawn and inputs of 0–13 bytes; the library reproduces
  the test vector byte for byte.
- No leaked source code, derived projects or third-party documentation of the engine were used.
  LZO1X and DDS are public formats.
