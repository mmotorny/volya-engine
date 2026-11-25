# Static sound sources (`level.snd_static`)

Behaviour-and-format specification of a level's **static sound sources**: the file
`levels\<level>\level.snd_static` and the rule by which each of its sources plays during play. It is S3 of
the scoping specification `docs/behavior/level-sound-scope.md`. What could not be established is marked
**unestablished**; what is inferred rather than shown by the data or the program is marked **believed**.

Builds on (not repeated here):

- `sound-ogg-params.md` (S1): how a sound name finds its file (section 2), the file's parameters, the
  "unspecified" AI sound type (section 7.2).
- `sound-positional.md` (S2): sounds and emitters (creating a sound with a volume category, section 3.1;
  **play at position** with the flags looped and 2D, section 3.2; the controls play volume and frequency,
  section 3.3; an emitter's end, section 4.2; **stop with fade**, section 4.4; the effects category,
  section 5.4); a file's **duration** *D* in milliseconds (section 2).
- `docs/formats/chunks.md`: chunk sequences, lookup by id (section 3), sequential iteration (section 4).
- `docs/behavior/level-loading.md`: the loading image stays drawn over the first frames of the running
  level (section 2.3); `$level$` is the loaded level's directory (section 2.2).
- `docs/behavior/sky.md` section 1: the game time of day at the start, 05:30:00.
- `docs/behavior/title-menu.md` section 2: opening the main menu pauses the game.
- `docs/behavior/start-position.md` via `sky.md` section 1: the start eye (−246.72630, −23.263261,
  −134.43387), used only in the verification facts.

Conventions: little-endian; `u32` unsigned 32-bit, `f32` IEEE single, `float3` = 3 × `f32` (x, y, z);
`stringZ` = bytes up to and including a zero byte, Windows-1251. Positions are world coordinates of the
level files. "S1", "S2" are the specifications above.

**Checks.** Every claim carries a tag: [D*n*] data checks (`check_data.py`), [B*n*] checks against the
shipped binaries (`check_bin.py`, on exports made by `export.py`). All are in the dirty workspace,
`checks/sound-static-sources/`; `run_all.py` extracts the files (`extract.py`), regenerates the exports and
runs both.

## 1. The file

- Path: `level.snd_static` in the loaded level's directory (`$level$`). The file is optional: a level
  without it has no static sources [B1].
- It is read once, when the level is loaded (the same loading step reads `level.som`) [B13]; this happens at
  every load of a level, so the state of section 3 starts afresh each time (believed to include loading a
  saved game, which loads the level anew).
- Of the 30 level directories that have a `level` file, 14 have this file (section 6.2) [D1].

## 2. Format

The file is a chunk sequence, read by **sequential iteration** (chunks.md section 4): every top-level chunk,
in stored order and whatever its id, is one **source** [B1]. Shipped files number their chunks 0, 1, 2, …
in order and fill the file exactly [D2].

The payload of each top-level chunk is itself a chunk sequence. The source's **record** is the payload of
its sub-chunk with id 0, found by lookup (chunks.md section 3); a missing sub-chunk 0 is an assertion
failure [B2]. Other sub-chunks are not read. In every shipped file each top-level payload holds exactly one
sub-chunk, id 0 (no compressed flag), filling it [D2].

Record, fields back to back from offset 0:

| Field | Type | Meaning |
|---|---|---|
| **name** | `stringZ` | sound name (resolved to a file per S1 section 2) |
| **position** | `float3` | world position of the source |
| **volume** *v* | `f32` | play volume (S2 section 3.3) |
| **frequency** *f* | `f32` | frequency, the playback-rate factor (S2 section 3.3) |
| **active from**, **active to** | 2 × `u32` | time-of-day window in milliseconds; both 0 = always active |
| **play min**, **play max** | 2 × `u32` | range of play durations in milliseconds; both 0 = play the file once per start |
| **pause min**, **pause max** | 2 × `u32` | range of pauses in milliseconds; both 0 = play continuously, looped |

[B2, B9]. The record's size is not checked: bytes after the last field are ignored, and a shorter record is
not detected [B2]. In every shipped record the name is followed by exactly the 44 bytes of the other fields
[D2].

## 3. Setting up a source

For each source, in file order, when the file is read [B2, B3]:

1. A sound is created from *name* in the **effects** volume category (S2 section 3.1), with the AI sound type
   left unspecified (S1 section 7.2). The file is resolved and loaded at this point (S1); a file that
   cannot be opened is a fatal error (S1 section 2).
2. Position, volume, frequency and the six window and range values are kept as read.
3. Two times, **next start** and **stop time** (both milliseconds of the clock *now* of section 4), are set
   to 0.

Sources with identical records are separate sources, each with its own sound (two shipped files repeat
records, section 6.2) [B1, D6].

## 4. The play rule

### 4.1 When it runs

The rule runs once per frame of the running level [B4], except:

- while the game is paused (the pause that opening the main menu causes, title-menu.md section 2) [B4];
- on the frames on which the loading image is still drawn after loading (level-loading.md section 2.3): the
  first frame on which it runs is the first frame without the loading image [B4].

On each frame on which it runs, two values are taken once and used for all sources:

- **now**: the game's clock in whole milliseconds, as an unsigned 32-bit value. This clock does not advance
  while the game is paused [B12]. (Not the sound system's own clock of S2 section 5.1.)
- **τ**: the game time of day in milliseconds, the game time in milliseconds modulo 86,400,000; the game
  time is the same value that the sky is given (sky.md) [B4, B11]. At the start of New Game τ =
  19,800,000 (05:30:00, sky.md section 1).

The sources are then processed in file order [B4].

### 4.2 Per source

Terms:

- **Playing**: the source's sound has an emitter (S2: from a play until the emitter ends, including the
  whole fade of a stop with fade) [B5].
- **Active**: active from = active to = 0, or active from ≤ τ < active to, comparing the values as signed
  32-bit integers (all shipped values are below 2³¹ [D4]). A window does not wrap past midnight: a window
  with active from > active to is never active [B5].
- **Start (looped or once)**: a play at position *position*, looped or not, not 2D, without delay
  (S2 section 3.2); then the emitter's play volume is set to *v* and its frequency to *f* [B5, B8, B9].
- **Stop**: stop with fade (S2 section 4.4) [B7].
- **R(a, b)**: *a* + (*n* mod (*b* − *a*)), where *n* is the next output (0 … 32,767) of a pseudo-random
  generator and "mod" is the remainder of truncating signed 32-bit division (so for *b* < *a* the result is
  *a* + (*n* mod |*b* − *a*|)). With *b* = *a* the original divides by zero and fails; ranges wider than
  32,768 only reach *a* … *a* + 32,767 [B6]. The original's generator is one program-wide linear
  congruential generator (state *s* ← *s* · 214,013 + 2,531,011 mod 2³², output (*s* >> 16) & 32,767)
  shared with other uses, so its sequence as seen by this rule depends on everything else that draws from
  it [B6]; reproducing the exact sequence is not required for this behaviour (believed).
- All sums of times are unsigned 32-bit (they wrap modulo 2³²); comparisons of now with next start and stop
  time are unsigned [B5].

Rule, for one source:

1. **Not active:** if playing, stop. Nothing else changes; next start and stop time keep their values [B5].
2. **Active, not playing:**
   - pause min = pause max = 0: start **looped**; stop time = never (0xFFFFFFFF). Next start is not
     changed [B5].
   - otherwise, if now < next start: nothing [B5].
   - otherwise, if play min = play max = 0: start **once** (not looped); stop time = never; next start =
     now + *D* + R(pause min, pause max), with *D* the file's duration (S2 section 2), not adjusted for the
     frequency [B5, B10].
   - otherwise: start **looped**; stop time = now + R(play min, play max); then next start = stop time +
     R(pause min, pause max) (the two numbers are drawn in this order) [B5].
3. **Active, playing:** if now ≥ stop time, stop [B5].

Consequences (follow from the rule):

- A looped continuous source (pause 0) starts on the first frame on which it is active and plays until its
  window ends; a later reopening of the window starts it from the beginning.
- For a once source with a pause, starts are at least *D* + pause min apart, measured from start to start.
  The emitter ends *D* after its start whatever the frequency (S2 section 4.2), so the silence between two
  plays is the drawn pause.
- A stopped source counts as playing until its fade has ended, so it is not restarted before then.
- After a window ends and reopens, a source whose next start has passed starts at once.
- Next start and stop time are 0 after loading, so a source with a pause starts on the first frame on which
  it is active.

## 5. `levels\l01_escape\level.snd_static`

435 bytes, 5 sources [D3]:

| # | Name | Position | *v* | *f* | Window | Play / pause | Rule |
|---|---|---|---|---|---|---|---|
| 0 | `nature\new_flies1` | (−255.7179, −18.8127, −127.2483) | 0.3 | 1 | always | 0, 0 / 0, 0 | looped |
| 1 | `ambient\sparks1` | (−245.3441, −23.2075, −135.9194) | 0.5 | 1 | always | 0, 0 / 0, 0 | looped |
| 2 | `scripts\magnitofon\magnitofon_2` | (−245.3842, −23.9443, −133.3607) | 0.5 | 1 | always | 0, 0 / 0, 0 | looped |
| 3 | `script_replics\radio\message\radio_noise_1` | (−245.1295, −23.9443, −134.5009) | 0.5 | 1 | 7,200,000 … 10,800,000 (02:00:00 – 03:00:00) | 0, 0 / 0, 0 | looped while active |
| 4 | `ambient\indoors\small_fan` | (−243.9568, −22.9818, −133.2737) | 1.0 | 1 | always | 0, 0 / 0, 0 | looped |

All five files exist as `sounds\<name>.ogg` [D5]. At the start of New Game (τ = 05:30:00) sources 0, 1, 2
and 4 are active and start on the first frame after the loading image; source 3 is inactive until 02:00:00
game time of the next day, plays looped until 03:00:00, and is then stopped with fade [D11, B5].

## 6. Verification facts

### 6.1 `l01_escape`

- `levels\l01_escape\level.snd_static`: `gamedata.db0`, 435 bytes, CRC-32 `af5f06dd`, SHA-256
  `f95c0bd65ce9d8227fff55dd2a4b6bacea45837642e0065d7f7c415adb300272` [D1, D3].
- Record lengths (payloads of the sub-chunks 0) 62, 60, 76, 87, 70 bytes [D3].
- Bit patterns of *v*: `0x3E99999A`, `0x3F000000` (sources 1 – 3), `0x3F800000`; *f* is `0x3F800000`
  (1.0) in all five [D3].
- Distances from the start eye to sources 0 … 4: 12.3405, 2.0298, 1.8484, 1.7373, 3.0158 m [D9].
- The five files (size in bytes, first 16 hex digits of SHA-256, granule position of the last Ogg page):
  `nature\new_flies1` 40,962, `721b881aa259b4bd`, 271,866; `ambient\sparks1` 24,459, `d58f62e26458c467`,
  176,953; `scripts\magnitofon\magnitofon_2` 1,967,558, `caeb8a2e3bbd942b`, 11,567,232;
  `script_replics\radio\message\radio_noise_1` 87,186, `2902f0e251972cc4`, 569,344;
  `ambient\indoors\small_fan` 7,677, `6fc26be3184788e7`, 28,710 [D7].

### 6.2 All shipped levels

- `level.snd_static` exists for 14 of the 30 level directories with a `level` file, each stored in exactly one archive [D1]:

  | Level | Archive | Sources |
  |---|---|---|
  | `l01_escape` | db0 | 5 |
  | `l03_agroprom` | db0 | 21 |
  | `l03u_agr_underground` | db0 | 17 |
  | `l04u_labx18` | db1 | 30 |
  | `l05_bar` | db1 | 9 |
  | `l07_military` | db1 | 1 |
  | `l08u_brainlab` | db1 | 5 |
  | `l10_radar` | db2 | 3 |
  | `l12_stancia` | db2 | 1 |
  | `l12_stancia_2` | db2 | 19 |
  | `l12u_control_monolith` | db3 | 1 |
  | `l12u_sarcofag` | db3 | 7 |
  | `testers_mp_military_2` | db3 | 2 |
  | `testers_mp_pool` | db3 | 2 |

  123 sources in all [D1].
- Every file: top-level chunk ids 0 … *n* − 1 in order, filling the file; each payload is one sub-chunk with
  id 0, filling it; each record is the name, its zero byte and 44 bytes [D2].
- Volumes: 1.0 (116 sources), 0.5 (5), 0.3 (2). Frequencies 0.27 … 1.03; 16 sources have a frequency other
  than 1.0 [D4].
- The six `u32` values (active from, active to, play min, play max, pause min, pause max) are all 0 in 115
  sources; the others are (0, 0, 0, 0, 2000, 30000) in the 5 sources `ambient\bar\kitchen_1` … `_5` of
  `l05_bar` (once, with a pause of 2,000 … 29,999 ms), (0, 86400000, 0, 86400000, 0, 0) in the 2 sources of
  `testers_mp_pool` (always active in practice, looped) and (7200000, 10800000, 0, 0, 0, 0) in source 3 of
  `l01_escape` [D4]. No shipped source draws from a range with equal bounds, and every range drawn from is
  at most 32,768 wide [D10].
- The kitchen files' last-page granule positions: `kitchen_1` 433,655, `_2` 876,546, `_3` 604,188, `_4`
  680,959, `_5` 400,348 [D8].
- 48 distinct names, all lower case and without extension; every one exists as `sounds\<name>.ogg`; no
  `.ogg` file is stored under `levels\` [D5].
- Repeated records: in `l04u_labx18` sources 12 … 23 repeat sources 0 … 11 field for field, and in
  `l08u_brainlab` source 4 repeats source 3; no other file repeats a record [D6].

## 7. Unestablished and believed

- That loading a saved game reads the file anew with next start and stop time 0 (believed; section 1).
- What happens to the sources' sounds when the level is left: S2 section 4.4 says every emitter is stopped
  when a level is left; whether the sources' sounds are discarded individually is not examined here
  (**unestablished**; no audible difference is known).
- The seed of the original's generator at the time the rule first draws from it, and which other uses draw
  from it in between (**unestablished**; section 4.2).
- Behaviour for records shorter than 44 bytes after the name: the original does not detect it (section 2);
  what the missing fields then hold is **unestablished** (no shipped record is short).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrSound.dll`,
  `bin\xrCore.dll`.
- Data examined: `level.snd_static` of all 14 levels that have it; the archive listings (level directories,
  `.ogg` files); the `.ogg` files named by the 123 sources (sizes, hashes, last Ogg page).
- Analysis: static analysis of the shipped binaries with Ghidra (headless decompilation and disassembly of
  the reading of the file, the per-frame source rule, the sound creation and play requests, the frame clock,
  the loading-image frame counter and the pause); measurement of the data with a parser written from this
  text (dirty workspace).

## Gaps found in review

- The clock *now* of section 4.1 is the engine's frame timer: its elapsed time is multiplied by the
  engine's timer speed factor (1 unless changed; what changes it is not established here), and while the
  engine's fixed-frame-time setting is on it advances by exactly 20 ms per frame instead. This text does
  not state either.
