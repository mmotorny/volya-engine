# Level music

Behaviour specification S5 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`):
which music tracks play during play on a level, and when. It covers the per-level track list in the game
configuration and the meaning of its five numbers, the two sounds of a track, the rule that picks, starts
and spaces the tracks, how the music is heard, and what pausing, the main menu, leaving the level, loading a
saved game and the scripts do to it. What could not be established is marked **unestablished**; what is
inferred rather than shown by the data or the program is marked **believed**.

Builds on (not repeated here):

- `docs/formats/sound-ogg-params.md` (S1): how a sound name finds its file (section 2), fatal errors
  (section 5), the file's parameters, the AI sound type and the condition for reports to the AI
  (sections 7.1, 7.2).
- `docs/behavior/sound-positional.md` (S2): creating a sound with a volume category (section 3.1);
  **play at position** with the flags looped and 2D and a delay (section 3.2); the play volume (section 3.3);
  an emitter's start, end and duration *D* (sections 2, 4.2); restart (section 4.3); **stop with fade** and
  stop all (section 4.4); pause (section 4.5); the volume step and the music category (sections 5.3, 5.4);
  voices (section 6); 2D emitters (section 7.3).
- `docs/formats/ltx-config.md`: entry values (section 3), lookups, case and the order of a section's
  entries (section 6), value interpretations (section 7).
- `docs/behavior/startup.md`: the game configuration `config\game.ltx`.
- `docs/behavior/level-loading.md`: the level-load steps and the loading image over the first frames
  (section 2.3), the intro movie, which pauses game time (section 2.3), the start time 05:30:00 of a New Game
  (section 2.6).
- `docs/behavior/player-state.md`: game time (the saved game time) and its speed, 10 times real time.
- `docs/behavior/title-menu.md`: the main menu pauses the game (section 2); the menu music, also a `_l` /
  `_r` pair of 2D music-category sounds at the same positions (section 6).

Conventions: times are in milliseconds unless stated; "unsigned" and "signed" are 32-bit integer
arithmetic; *n* is the next output of the pseudo-random generator (section 4.2); "mod" is the remainder of
truncating signed division. A **track** is one entry of a level's track list (section 2).

**Checks.** Every claim carries a tag: [D*n*] data checks (`check_data.py`), [B*n*] checks against the
shipped binaries (`check_bin.py`, on exports made by `export.py`), [M*n*] an implementation of sections 2 – 4
written from this text (`model.py`, reproducing the examples of section 8.3). All are in the dirty
workspace, `checks/sound-level-music/`; `run_all.py` extracts the files (`extract.py`), regenerates the
exports and runs all three. `check_data.py` reads every level's track list with a reader written from
section 2.

## 1. Overview

- A level may name a list of music tracks in the game configuration. A track is a pair of sounds, a left
  and a right channel, with a time-of-day window, a play volume and a range of pauses (section 2).
- At most one track plays at a time. When none is playing and the next check time has passed, one track is
  picked at random among those whose window contains the current time of day and played once, both
  channels together, as listener-relative ("2D") sounds in the music category. When it has ended, a pause is
  drawn from the track's range before the next pick. When no track is in its window, the check is repeated
  10 seconds later (section 4).
- On `l01_escape` only `music\amb01` ever plays, from 06:00 to 22:00 game time; its night track
  `music\amb20` never plays, because its window runs past midnight (section 7).

## 2. The track list

### 2.1 When and where it is read

The list is read once each time a level is loaded, in the level-load step that also reads the level's
static sound sources (`level.snd_static`, scoping S3), right after them [B1]. It comes from the game
configuration, the configuration loaded from `config\game.ltx` (`startup.md`) [B2], whose includes bring in
`config\game_maps_single.ltx` (the level sections) and `config\game_levels_music.ltx` (the track sections)
[D1].

1. If the configuration has no section named by the level's name, the level has no music [B2]. The test is
   the plain existence test of `ltx-config.md` section 6 (the name as given, not lowercased); the level's
   name is the name of its directory, e.g. `l01_escape` (believed; it is the name the level is loaded by).
2. If that section has no key `music_tracks` (keys are case-sensitive), the level has no music [B2].
3. The key's value is taken as a string (`ltx-config.md` section 7, quotes included). If it has no value or
   is empty, the level has no music [B2].
4. Otherwise the section that the value names is read (looked up lowercased; a section that does not exist
   is an error, `ltx-config.md` section 6). Every entry of that section, in the section's entry order (sorted
   by key, `ltx-config.md` section 6), is one track; that order is the order of the track list [B2].

Nothing else adds, removes or changes tracks [B14].

### 2.2 An entry

- The entry's **key** is the track's name *N*, as written (case kept) [B2, B3].
- The entry's **value** (as stored: whitespace outside quotes removed, `ltx-config.md` section 3) is scanned
  as by the C library's `sscanf` with the format `%d,%d,%f,%d,%d` into five fields, in this order [B3]:

  | Field | Scanned as | Default | Kept as |
  |---|---|---|---|
  | start hour *h*₀ | integer | 0 | window **start** = *h*₀ · 3,600,000 ms |
  | end hour *h*₁ | integer | 0 | window **end** = *h*₁ · 3,600,000 ms |
  | volume *v* | 32-bit float | 1.0 | the track's play volume |
  | pause min *p*₀ | integer, seconds | 0 | **pause min** = *p*₀ · 1,000 ms |
  | pause max *p*₁ | integer, seconds | 0 | **pause max** = *p*₁ · 1,000 ms |

- The scan stops at the first field that does not convert; that field and all after it keep their
  defaults. The number of fields is not checked [B3].
- Before the scaling, if *p*₀ = *p*₁, then *p*₁ is increased by 1 [B3]. So the two pause bounds always
  differ by at least 1,000 ms.
- An entry with no value (`ltx-config.md` section 3) is not handled by the original; what happens is
  **unestablished** (no shipped entry lacks a value [D4]).

Examples [M2]: `6,22,0.2,20,40` → start 21,600,000, end 79,200,000, volume 0.2, pauses 20,000 … 40,000;
`0,0,0.2,20,40` → start 0, end 0 (always, section 4.2); `1,2,0.5,30,30` → start 3,600,000, end 7,200,000,
volume 0.5, pauses 30,000 … 31,000; `6` → start 21,600,000, end 0, volume 1.0, pauses 0 … 1,000;
`6,22,x` → start 21,600,000, end 79,200,000, volume 1.0, pauses 0 … 1,000.

### 2.3 The sounds of a track

When the entry is read, two sounds are created [B3, B4]:

- the **left** channel from the name *N* + `_l`, the **right** channel from *N* + `_r` (resolved and loaded
  per S1 section 2, so `sounds\music\amb01_l.ogg` for `music\amb01`; a file that cannot be opened is fatal,
  S1 section 5, at level load);
- both in the **music** volume category (S2 section 3.1);
- both with the AI sound type stated as 0, so the files' own type is not used (S1 section 7.2) and no report
  to the AI is ever made for them (S1 section 7.1) [B4].

A new sound has no emitter (S2 section 3.1) [B4].

## 3. State

Per level, besides the track list [B5]:

- **current track**: none at first (the list is read with none current);
- **next check time** (a value of the clock *now* of section 4.1): 0 at first.

The state is created with the level, so it starts afresh at every level load [B5, B13]. It is used by
nothing but the level's creation, loading, frame and teardown [B14]; that it is not part of a saved game is
believed from this.

## 4. The rule

### 4.1 When it runs

The rule runs once per frame of the running level, in the same step as the static sound sources of S3,
after them [B6]. It does not run [B6]:

- while game time is paused (the pause of the game's timers: by the main menu, the pause action, switching
  away from the program, the scripts' device pause, S2 section 4.5; and during the intro movie,
  `level-loading.md` section 2.3); pausing sounds alone does not stop it, pausing game time does;
- on the frames on which the loading image is still drawn after loading (`level-loading.md` section 2.3).

It does nothing when the track list is empty [B6]. On each frame on which it runs it takes:

- **now**: the game's clock in whole milliseconds, an unsigned value; it advances with real time and stops
  while game time is paused [B12]. It is not the sound clock of S2 section 5.1.
- **τ**: the time of day in milliseconds, the game time in milliseconds modulo 86,400,000; the same game time
  the sky is given [B6, B11].

### 4.2 Terms

- A track is **playing** while the sound of either channel has an emitter (S2: from a play until the emitter
  ends, including the whole fade of a stop with fade) [B7, B9].
- A track is **active** when start = end = 0, or start ≤ τ < end, comparing as signed values [B7]. A window
  does not wrap past midnight: a track with start > end, or with start = end ≠ 0, is never active.
- **Starting** a track: both channels are played at position (S2 section 3.2), not looped, 2D, without
  delay and without an associated object; the left channel at (−0.5, 0, 0.3), the right one at
  (0.5, 0, 0.3), in listener-relative coordinates (S2 section 7.3); then the play volume of each channel's
  emitter is set to the track's volume *v* [B8]. Both start in the same frame.
- **Stopping** a track: each channel whose sound has an emitter is stopped with fade (S2 section 4.4) [B7].
- *n*: the next output, 0 … 32,767, of the original's pseudo-random generator: one program-wide linear
  congruential generator, state *s* ← *s* · 214,013 + 2,531,011 mod 2³², output (*s* >> 16) & 32,767,
  shared with other uses (among them the static sound sources of S3), so the sequence this rule sees depends
  on everything else that draws from it [B10]. Reproducing the exact sequence is not required for this
  behaviour (believed).

### 4.3 Each frame

1. **Pick.** If there is no current track and now > next check time (unsigned comparison) [B7]:
   - every track that is playing is stopped [B7] (in normal play none is: a track stops being current only
     once neither channel has an emitter, step 2);
   - the active tracks are collected in list order [B7];
   - if there is none, next check time = now + 10,000 [B7];
   - otherwise the current track becomes the collected track number *n* mod *k* (counting from 0), *k* the
     number of collected tracks, and that track is started [B7, B8].
2. **End.** Then, in the same frame, if there is a current track and neither channel's sound has an
   emitter, there is no current track any more, and next check time = now + pause min +
   (*n* mod (pause max − pause min)) [B9]. (The original skips the draw when pause min = pause max = 0, which
   section 2.2 never produces [B3, B9].)

Sums are unsigned and wrap modulo 2³² [B7, B9]. A track is not stopped when τ leaves its window, nor by
anything else of this rule while it is current [B7, B9].

Consequences (follow from the rule):

- The first check is on the first frame on which the rule runs (next check time is 0).
- While no track is active, the check repeats on the first frame with now more than 10,000 after the previous
  check.
- A track, once started, plays to its end: *D* of its files (S2 section 4.2; both channels of every shipped
  track have the same length [D4]), however τ moves meanwhile.
- After it, the silence before the next pick is at least pause min and less than pause max (plus up to one
  frame). The draw is not uniform when the range is not a divisor of 32,768: for 20,000 … 40,000 the values
  20,000 … 32,767 are twice as likely as 32,768 … 39,999 [M2].
- With one active track the same track is picked every time.
- Whether a track can be picked depends only on τ at the moment of the pick.

## 5. How it is heard

- Each channel is a 2D emitter in the music category; its output volume settles at *b* · *v* · *c*, with *b*
  the file's base volume, *v* the track's volume and *c* = `snd_volume_music` (S2 sections 5.3, 5.4) [B4,
  B8]. All shipped track files have *b* = 1, min distance 1, max distance 300 [D4]; a 2D emitter at
  (±0.5, 0, 0.3), 0.583 m from the listener, is within the min distance, so the device's distance factor
  is 1 (S2 section 7.3). With the shipped volume 0.2 the level music plays at 0.2 · `snd_volume_music`:
  0.2 with the default `user.ltx` (1.0), 0.14 with the engine's initial 0.7 [M2].
- The effects factor of S2 section 5.4 does not apply to the music category, so nothing that changes it
  changes the level music.
- The channels are not looped; each takes a voice when it gets one (S2 section 6).

## 6. Pausing, the menu, leaving the level, saved games, scripts

- **Main menu during play, pause action, switching away:** game time and sounds are paused (S2 section 4.5):
  the rule stops (section 4.1) and the playing channels are paused and resume where they were (S2 section
  4.5); the menu plays its own music meanwhile (`title-menu.md` section 6).
- **Leaving the level:** every emitter is stopped (S2 section 4.4), and the level's teardown discards the
  tracks' sounds (S2 section 3.1: a discarded sound's emitter stops at once) [B13].
- **Loading a saved game** loads the level anew, so the state of section 3 starts afresh (believed; section
  3).
- **Scripts:** none of the 439 shipped scripts names `music_tracks`, a `*_musics` section, a
  `music\amb…` file or `snd_volume_music` [D6]; the rule's start and stop of tracks are used by nothing else
  [B14]. The engine registers a music event of its own, but the level never acts on it [B14]. So scripts do
  not start, stop or choose level music; they can affect it only through game time and pausing. The only
  shipped script code that changes the speed of game time is a function of `_g.script`
  (`set_current_time`) that no script calls [D7].

## 7. `l01_escape`

`[l01_escape]` (in `config\game_maps_single.ltx`) has `music_tracks = l01_escape_musics`; that section (in
`config\game_levels_music.ltx`) has two entries, in this order [D2]:

| Track | Value | Window | Volume | Pauses | Files, *D* |
|---|---|---|---|---|---|
| `music\amb01` | `6,22,0.2,20,40` | 21,600,000 … 79,200,000 (06:00 – 22:00) | 0.2 | 20,000 … 40,000 | `sounds\music\amb01_l.ogg`, `_r.ogg`; 521,142 ms |
| `music\amb20` | `22,6,0.2,20,40` | start 79,200,000 > end 21,600,000: never active | 0.2 | 20,000 … 40,000 | `sounds\music\amb20_l.ogg`, `_r.ogg`; 322,666 ms |

[D2, D3, M1]

At a New Game the rule first runs after the intro movie (which pauses game time), at τ = 05:30:00
(`level-loading.md` section 2.6): no track is active, and it checks again about every 10 s. With game time
at 10 times the clock (`player-state.md`), τ reaches 06:00:00 about 180 s of the clock after the first run;
the next check starts `music\amb01` [M3]. It then repeats, with pauses of 20 … 40 s, while τ is between
06:00 and 22:00 at each pick [M4]. A track started before 22:00 plays to its end; after that no music plays
until a pick at τ ≥ 06:00 [M5]. `music\amb20` never plays [M1].

## 8. Verification facts

### 8.1 `l01_escape`

- `config\game.ltx` (`gamedata.dbb`, 218 bytes, SHA-256 starting `c502a7d26aff3990`) includes, in order,
  `game_maps_single.ltx`, `game_maps_mp.ltx`, `game_levels.ltx`, `game_graphs.ltx`, `game_story_ids.ltx`,
  `game_spawn_story_ids.ltx`, `game_levels_music.ltx` [D1].
- `config\game_maps_single.ltx`: `gamedata.dbb`, 6,712 bytes, SHA-256 starting `617b34c56e8cefd0`;
  `config\game_levels_music.ltx`: `gamedata.dbb`, 1,921 bytes, SHA-256 starting `905ec86504bc1f2f`; both
  also in `gamedata.db0` with the same size and CRC; the same holds for `game.ltx` [D1].
- `game_levels_music.ltx` starts with the lines `[l01_escape_musics]`, ``music\amb01<5 tabs>=  6, 22,   0.2,   20, 40``
  and ``music\amb20<2 tabs, 4 spaces, 2 tabs>= 22, 6,    0.2,   20, 40`` (CRLF line ends); the stored values are
  `6,22,0.2,20,40` and `22,6,0.2,20,40`; the volume is the 32-bit float `0x3E4CCCCD` [D2].
- The four files (`gamedata.db5`): mono, 44,100 Hz; `amb01_l`, `amb01_r` 22,982,378 samples (*D* = 521,142
  ms), `amb20_l`, `amb20_r` 14,229,607 samples (*D* = 322,666 ms); each with min distance 1, max distance 300,
  base volume 1, AI type 0 (as S1 section 8 lists) [D3].

### 8.2 All shipped levels

Of the 30 level directories with a `level` file, 16 have music and 14 have none [D4]:

| Level | Section | Tracks (day = `6,22,0.2,20,40`, night = `22,6,0.2,20,40`, always = `0,0,0.2,20,40`) |
|---|---|---|
| `l01_escape` | `l01_escape_musics` | `amb01` day, `amb20` night |
| `l02_garbage` | `l02_garbage_musics` | `amb02` day, `amb20` night |
| `l03_agroprom` | `l01_escape_musics` | `amb01` day, `amb20` night |
| `l03u_agr_underground` | `l03u_agr_underground_musics` | `amb10` always |
| `l04_darkvalley` | `l04_darkvalley_musics` | `amb05` day, `amb09` night |
| `l06_rostok` | `l06_rostok_musics` | `amb06` day, `amb09` night |
| `l07_military` | `l07_military_musics` | `amb17` night, `amb21` day |
| `l08_yantar` | `l08_yantar_musics` | `amb17` night, `amb22` day |
| `l08u_brainlab` | `l08u_brainlab_musics` | `amb15` always |
| `l10_radar` | `l10_radar_musics` | `amb03` day, `amb17` night |
| `l10u_bunker` | `l10u_bunker_musics` | `amb04` always |
| `l11_pripyat` | `l11_pripyat_musics` | `amb07` day, `amb17` night |
| `l12_stancia` | `l12_stancia_musics` | `amb08` day, `amb17` night |
| `l12_stancia_2` | `l12_stancia_2_musics` | `amb12` day, `amb17` night |
| `l12u_control_monolith` | `l12u_control_musics` | `amb13` always |
| `l12u_sarcofag` | `l12u_sarcofag_musics` | `amb11` always |

(track names under `music\`, in list order) [D4]

- No music: `l04u_labx18`, `l05_bar` and the 12 `testers_mp_*` levels; all have a level section without
  `music_tracks`, except `testers_mp_pool`, which has no section [D4]. In `game_maps_single.ltx` the lines for
  `l04u_labx18` and `l05_bar` are commented out, and the line for `l03_agroprom` naming
  `l03_agroprom_musics` is commented out and followed by one naming `l01_escape_musics` [D5].
- 27 tracks in all, each value one of the three forms above; 11 levels have a day and a night track, and every one of those night tracks is never active; the 5
  other levels (`l03u_agr_underground`, `l08u_brainlab`, `l10u_bunker`, `l12u_control_monolith`,
  `l12u_sarcofag`) have one track that is always active [D4, M1].
- 18 distinct track names (`amb01` … `amb13`, `amb15`, `amb17`, `amb20` … `amb22`); every `_l` and `_r`
  file exists, all mono 44,100 Hz, each pair with equal sample counts, every file with min distance 1, max
  distance 300, base volume 1, AI type 0 [D4]. Durations *D* (ms): `amb01` 521,142, `amb02` 337,297, `amb03`
  300,000, `amb04` 363,870, `amb05` 300,000, `amb06` 191,588, `amb07` 226,966, `amb08` 278,316, `amb09`
  247,857, `amb10` 248,333, `amb11` 829,999, `amb12` 796,666, `amb13` 306,666, `amb15` 283,833, `amb17`
  137,166, `amb20` 322,666, `amb21` 299,666, `amb22` 294,000 [D4].
- Sections named by no `music_tracks` key: `[k01_darkscape_musics]`, `[l05_bar_musics]` (in
  `game_levels_music.ltx`) and `[level_music_tracks]` (in `game_maps_single.ltx`). `[l03_agroprom_musics]`
  is named only by the section `[ai_test]`, and `[l01_escape_musics]` also by `[andy_test]` (both in
  `game_maps_single.ltx`; neither is a level directory) [D5].

### 8.3 Worked examples

From sections 2 – 4 (`model.py`; frames every 1 ms, *n* = 0 at every draw, the end of a track seen in the
frame in which its emitters end, τ advancing 10 ms per ms of now) [M1 – M5]:

| Case | Result |
|---|---|
| `l01_escape`, active tracks | at τ = 05:30:00 and 05:59:59.999 none; at 06:00:00, 12:00:00 and 21:59:59.999 `amb01`; at 22:00:00, 23:30:00, 00:00:00 and 03:00:00 none; `amb20` at no time of day [M1] |
| pause draw for 20,000 … 40,000 with *n* = 0, 19,999, 20,000, 32,767 | 20,000, 39,999, 20,000, 32,767; over all 32,768 values of *n*, each of 20,000 … 32,767 occurs twice and each of 32,768 … 39,999 once [M2] |
| New Game, first run at now = 100,000, τ = 05:30:00 | checks at now = 100,000, 110,001, …, the 18th at 270,017 (τ = 05:58:20.170) finding nothing; `amb01` starts at now = 280,018 (τ = 06:00:00.180) [M3] |
| that start | ends at 801,160; next check time 821,160; `amb01` starts again at 821,161 (τ = 07:30:11.610) [M4] |
| start at τ = 21:59:59 | plays all 521,142 ms; the check 20,001 ms after its end (τ = 23:30:10.440) finds nothing [M5] |

## 9. Unestablished and believed

- That the level's name used for the section lookup is its directory name (believed; section 2.1).
- What an entry without a value does (section 2.2; no shipped entry).
- The generator's state when this rule first draws, and which other uses draw in between (section 4.2).
- Whether, within a frame, the sound system's update (which ends emitters, S2 section 5) comes before or
  after this rule, i.e. whether a track's end is seen in the frame its emitters end or in the next one (at
  most one frame; believed inaudible).
- That the clock *now* and game time advance together at the 10 : 1 ratio of `player-state.md` (believed;
  the examples assume it).
- That the step, when the engine runs it from its list of work deferred within the frame rather than
  directly, still runs in the same frame (believed) [B6].
- What a sound restart (`options-screens.md`) does to a playing track (**unestablished**).
- That the music state is not saved and starts afresh after loading a saved game (believed; section 3).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrSound.dll`,
  `bin\xrCore.dll`.
- Data examined: `config\game.ltx` with its includes; every `config\*.ltx`, `scripts\*.script` and
  `sounds\music\*.ogg` of the archives (Ogg pages, Vorbis identification and comment headers); the archive
  listings (level directories).
- Analysis: static analysis of the shipped binaries with Ghidra (headless decompilation and disassembly of
  the reading of the track list, the per-frame rule, the sound creation, play, stop and volume requests, the
  frame clock, the pause, the loading-image frame counter, the level's creation, teardown and event
  handling, and a search of every use of the music state); measurement of the data with a reader written
  from this text (dirty workspace). No program was executed.
