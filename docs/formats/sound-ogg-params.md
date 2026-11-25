# Sound parameters in `.ogg` files

Specification S1 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`): the
per-file sound parameters that a sound file carries in its Vorbis comment header, how a sound name finds
its file, what the program does when the parameters are missing or invalid, and what each parameter is
used for. The playback rules that use the parameters (attenuation, volume categories, AI hearing) are
for S2 and the specifications of the sounds' users; this specification states only which parameter
feeds which use.

Builds on: `docs/behavior/title-menu.md` section 6 (sound files are Ogg Vorbis, `sounds\<name>.ogg`);
`docs/formats/ltx-config.md` section 10 (path aliases); `docs/behavior/level-loading.md` (`$level$` is
the loaded level's directory); `docs/formats/db-archive.md` (stored paths, the later archive wins).
The Ogg container and the Vorbis identification and comment headers are the public Xiph.Org formats
(the Ogg encapsulation format, RFC 3533, and the Vorbis I specification); this specification uses only
their public structure: the comment header is the second Vorbis packet, holding a vendor string and a
list of *user comments*, each a 32-bit little-endian length followed by that many bytes.

**Checks.** Every factual claim carries a tag. The checks are scripts in the dirty workspace,
`checks/sound-ogg-params/`; `run_all.py` extracts every `.ogg` file and the scripts from the archives
(`extract.py`) and runs `check_data.py` (tags D1 – D9, decoding every file with `ogg_params.py`, a
parser written from this text) and `check_binary.py` (tags B1 – B12, static analysis of the shipped
program files).

## 1. Files

- The archives hold 16,306 distinct `.ogg` stored paths (compared case-insensitively), all under
  `sounds\`; no `.ogg` file is stored in a level directory or anywhere else [D1].
- Every one of them is an Ogg Vorbis stream with 1 channel at 44,100 Hz [D2].
- The program accepts only such files for sounds: a file with another channel count or sample rate is a
  fatal error (section 5) [B4].

## 2. Finding a sound's file

A sound is requested by a name *n* (as configuration files write it, e.g. `ambient\sparks1`).

1. *n* is lowercased, and the part of *n* from its last `.` onwards is removed (if *n* contains a `.`).
   This removal is applied three times in succession while the name is passed on, so up to three
   `.`-suffixes are removed [B5].
2. `.ogg` is appended.
3. The file is looked up first as `$level$<n>.ogg` (in the loaded level's directory), and only if that
   does not exist as `$game_sounds$<n>.ogg`, i.e. `sounds\<n>.ogg` [B5]. As no level directory holds an
   `.ogg` file [D1], every sound of this edition resolves to `sounds\<n>.ogg`.
4. A file that cannot be opened or is empty is a fatal error (section 5) [B4].

A consequence of rule 1: the stored file `sounds\scripts\m_gameplay\podoidi-blije.sfk.ogg`, the only
stored `.ogg` path with a further `.` in it [D1], can't be reached by any name (believed; follows from
the rule, not executed).

The parameters are kept per resolved name: the first request of a name loads the file and reads its
parameters, and later requests of the same name reuse them (believed from static analysis: the program
searches its loaded sounds by name before loading a file) [B6].

## 3. The parameter record

The parameters are in the **first user comment** of the comment header; the vendor string and any
further user comments are ignored [B1]. The comment's bytes are not text: they are a binary record of
little-endian fields, `u32` integers and `f32` IEEE 754 single-precision numbers [B1, D4]. The first field
is a version, which selects the layout:

| Offset | Version 1 | Version 2 | Version 3 |
|---|---|---|---|
| 0 | `u32` version = 1 | `u32` version = 2 | `u32` version = 3 |
| 4 | `f32` min distance | `f32` min distance | `f32` min distance |
| 8 | `f32` max distance | `f32` max distance | `f32` max distance |
| 12 | `u32` AI sound type | `f32` base volume | `f32` base volume |
| 16 | — | `u32` AI sound type | `u32` AI sound type |
| 20 | — | — | `f32` max AI distance |
| Size | 16 bytes | 20 bytes | 24 bytes |

[B1]

- A version 1 record has no base volume: it is 1.0 [B1].
- Version 1 and 2 records have no max AI distance: it equals the max distance [B1].
- What the program does with a record shorter than its version's size is **unestablished**; no shipped
  file has one [D3].

In this edition every record is version 3 and 24 bytes long [D3]:

- 16,304 files have exactly one user comment, a 24-byte version 3 record [D3];
- 2 files have no user comment: `sounds\scripts\m_gameplay\podoidi-blije.sfk.ogg` and
  `sounds\characters_voice\scenario\video\intro_gsc_wp_r.ogg` [D3];
- no file has more than one user comment, and none has a version 1 or 2 record [D3].

## 4. Missing or unknown records

| Case | Result |
|---|---|
| No user comment | a warning is written to the log; the defaults below apply [B1] |
| Version other than 1, 2, 3 | a warning is written to the log; the defaults below apply [B1] |

Defaults: min distance 1.0, max distance 300.0, base volume 1.0, AI sound type 0, max AI distance
300.0 [B2, B3].

After the record is read (or the defaults are kept), a max distance or a max AI distance below 0.1 is a
fatal error (section 5) [B1, B3]. Every shipped record passes: see section 6 [D4].

## 5. Fatal errors

The cases called fatal above (wrong channel count or rate, file missing or empty, max distance or max AI
distance below 0.1) make the program show an error and stop (believed from static analysis of the
shipped program files; not executed) [B7].

## 6. Values in this edition

Over the 16,304 records [D4]:

| Field | Distinct values | Smallest | Largest | Most common (files) |
|---|---|---|---|---|
| min distance | 43 | 0.06818182 | 1000.0 | 1.5 (6,584) |
| max distance | 34 | 5.0 | 1000.0 | 50.0 (7,464) |
| base volume | 15 | 0.01 | 2.0 | 1.0 (16,082) |
| max AI distance | 26 | 1.0 | 1000.0 | 30.0 (6,481) |

Invariants holding for every record: min distance > 0; min distance ≤ max distance; max distance ≥ 0.1;
max AI distance ≥ 0.1; every `f32` is finite [D4]. The max AI distance equals the max distance in 1,373
records [D4].

The AI sound type takes 19 distinct values (section 7.2) [D5].

## 7. What the parameters are used for

Each parameter applies to every sound played from the file [B8]. The rules themselves belong to S2 and to
the specifications of the sounds' users; the uses below are established by static analysis [B8, B9, B10].

### 7.1 Distances and volume

- **Min distance**: the distance from the sound within which it is heard at full volume; beyond it the
  volume falls with distance (S2) [B9]. **Disputed in review:** the distance attenuation factor is the
  min distance divided by (the rolloff setting times the distance), clamped to 0 – 1, so full volume
  holds within the min distance divided by the rolloff setting; that equals the min distance only when
  the rolloff setting is 1 (the setting's value is for S2).
- **Max distance**: beyond it the sound is not heard [B9].
- **Base volume**: a factor on the sound's volume, applied together with the volume category's setting
  (S2) [B9].
- **Max AI distance**: the range within which the sound is reported to the game's AI as heard; the range
  is the max AI distance times a per-sound volume factor (1 when a sound starts; how it changes: S2),
  capped at the max AI distance, and no report is made when that range is below 0.1 [B8, B10]. Reports
  are made only for a sound whose AI sound type (section 7.2) is not 0 and that the game has associated
  with an object (the second condition believed) [B10]. What the AI does with a report is not part of
  this specification (S24).

### 7.2 AI sound type

The AI sound type is a set of bit flags describing the kind of sound for the game's AI. When the program
creates a sound without stating a type of its own (it passes the value −1 for "unspecified"), the sound
takes the file's AI sound type; when a type is stated, the file's value is not used [B11]. Which callers
state a type is for each sound's specification.

The scripts see the flag values under the names `snd_type.<name>` [B12]; the shipped scripts use such
names (e.g. `snd_type.talk`, `snd_type.weapon_shoot`) [D9]. The single-bit flags:

| Bit value | Name | Bit value | Name |
|---|---|---|---|
| 0x00000080 | `ambient` | 0x00100000 | `empty` |
| 0x00000100 | `object_explode` | 0x00200000 | `shoot` |
| 0x00000200 | `object_collide` | 0x00400000 | `use` |
| 0x00000400 | `object_break` | 0x00800000 | `take` |
| 0x00000800 | `idle` | 0x01000000 | `hide` |
| 0x00001000 | `eat` | 0x02000000 | `drop` |
| 0x00002000 | `attack` | 0x04000000 | `pick_up` |
| 0x00004000 | `talk` | 0x08000000 | `world` |
| 0x00008000 | `step` | 0x10000000 | `anomaly` |
| 0x00010000 | `injure` | 0x20000000 | `monster` |
| 0x00020000 | `die` | 0x40000000 | `item` |
| 0x00040000 | `reload` | 0x80000000 | `weapon` |
| 0x00080000 | `bullet_hit` | | |

`no_sound` is 0 [B12]. The combined names, each the sum of one of the five kind flags `world`, `anomaly`,
`monster`, `item`, `weapon` and one action flag [B12]:

| Value | Name | Value | Name |
|---|---|---|---|
| 0x08000080 | `world_ambient` | 0x20020000 | `monster_die` |
| 0x08000100 | `world_object_explode` | 0x40400000 | `item_use` |
| 0x08000200 | `world_object_collide` | 0x40800000 | `item_take` |
| 0x08000400 | `world_object_break` | 0x41000000 | `item_hide` |
| 0x10000800 | `anomaly_idle` | 0x42000000 | `item_drop` |
| 0x20001000 | `monster_eat` | 0x44000000 | `item_pick_up` |
| 0x20002000 | `monster_attack` | 0x80040000 | `weapon_reload` |
| 0x20004000 | `monster_talk` | 0x80080000 | `weapon_bullet_hit` |
| 0x20008000 | `monster_step` | 0x80100000 | `weapon_empty` |
| 0x20010000 | `monster_injure` | 0x80200000 | `weapon_shoot` |

The values in this edition's records, with their file counts [D5]:

| Value | Files | Value | Files |
|---|---|---|---|
| 0x00000000 | 1,923 | 0x28000000 | 6 |
| 0x08000080 | 215 | 0x40400000 | 5 |
| 0x08000100 | 8 | 0x40800000 | 17 |
| 0x08000200 | 13 | 0x41000000 | 2 |
| 0x08000400 | 8 | 0x80040000 | 33 |
| 0x20001000 | 27 | 0x80080000 | 33 |
| 0x20002000 | 2,527 | 0x80100000 | 3 |
| 0x20004000 | 10,780 | 0x80200000 | 89 |
| 0x20008000 | 154 | | |
| 0x20010000 | 272 | | |
| 0x20020000 | 189 | | |

All are combined names of the table above or 0, except 0x28000000 (`monster` plus `world`, no action
flag; 6 files) [D5]. How the AI interprets the flags is not part of this specification (S24).

## 8. Verification on the first level's files

Parameters of the files of the level's static sound sources (S3), of its music (S5) and of the menu
music (`title-menu.md` section 6); all are version 3. Here and in section 6, `f32` values are shown
rounded to 7 significant digits [D6]:

| File (under `sounds\`) | Min | Max | Volume | AI type | Max AI |
|---|---|---|---|---|---|
| `nature\new_flies1.ogg` | 1.0 | 20.0 | 1.0 | 0x00000000 | 20.0 |
| `ambient\sparks1.ogg` | 0.2 | 10.0 | 1.0 | 0x00000000 | 10.0 |
| `scripts\magnitofon\magnitofon_2.ogg` | 0.4 | 15.0 | 1.0 | 0x08000080 | 1.0 |
| `script_replics\radio\message\radio_noise_1.ogg` | 0.1363636 | 20.0 | 1.0 | 0x00000000 | 20.0 |
| `ambient\indoors\small_fan.ogg` | 0.5 | 15.0 | 1.0 | 0x08000080 | 15.0 |
| `music\amb01_l.ogg` | 1.0 | 300.0 | 1.0 | 0x00000000 | 300.0 |
| `music\amb01_r.ogg` | 1.0 | 300.0 | 1.0 | 0x00000000 | 300.0 |
| `music\amb20_l.ogg` | 1.0 | 300.0 | 1.0 | 0x00000000 | 300.0 |
| `music\amb20_r.ogg` | 1.0 | 300.0 | 1.0 | 0x00000000 | 300.0 |
| `music\wasteland2_l.ogg` | 1.0 | 300.0 | 1.0 | 0x00000000 | 300.0 |
| `music\wasteland2_r.ogg` | 1.0 | 300.0 | 1.0 | 0x00000000 | 300.0 |

Digest over all files [D7]: the SHA-256 of the UTF-8 text made of one line per `.ogg` file, sorted by
lowercased stored path, each line the lowercased stored path, a space, and the lowercase hexadecimal
bytes of its first user comment (empty when it has none), each line ending in `\n`, is
`406b0717d2331713eef09cca9dd0c1efe31f3941efae7e67092a879c775c5345`.

Exact sums of the `f32` values over all 16,304 records, rounded to 6 decimal places [D8]: min distance
38,288.879544, max distance 1,567,394.369480, base volume 16,268.079999, max AI distance 774,157.0.

## 9. Unestablished

- What the program does with a record shorter than its version's size (section 3); no shipped file has
  one.
- Which callers create sounds with an unspecified AI sound type, and so use the file's (section 7.2).
- How the AI interprets the AI sound type and the reports (S24).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives
  `gamedata.db0` … `gamedata.dbd`, and the shipped program files of `bin\`.
- Data examined: every `.ogg` file of the archives (Ogg pages, Vorbis identification and comment headers)
  and the shipped `scripts\*.script` files.
- Analysis: static analysis (decompilation and disassembly with Ghidra) of the shipped sound, core and
  game program files; reading constants from them. No program was executed.

## Gaps found in review

- A file whose Vorbis stream information cannot be read (not a valid Ogg Vorbis stream) is also a fatal
  error (section 5); section 1 and section 5 list only the channel count, the sample rate and a missing
  or empty file.
- The search among loaded sounds (section 2) compares the name after two of the three `.`-suffix
  removals, while a loaded sound keeps its name after all three; so a name with three or more `.` never
  matches a loaded sound and its file is loaded again on every request (believed from static analysis).
