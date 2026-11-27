# Console diagnostics commands

What the original's three diagnostic console commands write to the log: `stat_memory` (memory
statistics), `dump_resources` (counts of the graphics resources) and `dump_open_files` (the list of files
opened for reading), when the programs run `stat_memory` themselves, the phase lines written during
a load and the related lines written at shutdown. How a line reaches a command, the bare-name rule and the help text are those of
`console-commands.md` (CC); the log and the console's display of it are those of `console-scope.md`
section 7 (CS) and of the console-log specification. `rs_stats` is not part of this specification
(`r1-settings-scope.md` 2.11). What could not be established is marked **unestablished**; a statement
inferred without a direct check is marked **believed**, with why. Facts marked **measured** come from
headless runs of the shipped game (Provenance); the others from the shipped binaries.

Conventions:

- "The engine" is `bin\XR_3DA.exe`, "the game library" `bin\xrGame.dll`, "the renderer" the loaded
  renderer library (`xrRender_R1.dll` or `xrRender_R2.dll`), "the core library" `bin\xrCore.dll`, all of
  the shipped Russian edition (GOG build).
- Log lines are quoted between backticks exactly as written; `<…>` stands for a variable part. "Prints"
  means "writes as one log line". **K** below is a byte count divided by 1024, rounded down, unless
  stated otherwise; every number is printed in decimal without padding unless stated.
- "In a level" means after a New Game has finished loading; "in the main menu" means the title menu
  with no level loaded (`title-menu.md`).

**Checks.** Every claim carries a tag [DG-*n*]. The checks are three scripts in the dirty workspace,
`diag/checks/run.py` (DG-1 – DG-21), `diag/mem/checks.py` (DG-22 – DG-37) and `consnd/checks/sound.py`
(DG-38 – DG-48); they use read-only static analysis of the shipped binaries, the archives' file tables,
the extracted configuration and the logs of headless runs of the shipped game (`diag/run/`,
`consnd/run/`). All checks pass.

## 1. The commands

| Command | Program | Kind | Argument | Bare name |
|---|---|---|---|---|
| `stat_memory` | game library | action | ignored | runs |
| `dump_resources` | engine | action | ignored | runs |
| `dump_open_files` | engine (the listing is kept by the core library) | action | an integer (2.3) | shows: prints `- dump_open_files ` |

Kinds, the help text (`no arguments` for all three) and the bare-name rule are CC section 8 [DG-1,
DG-18]. Measured: `stat_memory extra words` prints the same lines as `stat_memory`, and
`dump_resources 0` the same as `dump_resources` [DG-10, DG-14].

## 2. Output

### 2.1 `stat_memory`

Prints four lines [DG-2, DG-3, DG-10]:

1. `* [win32]: free[<a> K], reserved[<b> K], committed[<c> K]`
2. `* [ D3D ]: textures[<t> K]`
3. `* [x-ray]: crt heap[<h> K], process heap[<p> K], game lua[<g> K], engine lua[<e> K], render[<r> K]`
4. `* [x-ray]: economy: strings[<s> K], smem[<m> K]`

Before measuring, the command releases memory the program holds but no longer uses: it returns unused
memory of the C runtime's heap to the system, compacts the process's default heap, drops shared strings
and shared memory blocks nothing refers to any more, and, when the command line contains
`-swap_on_compact`, asks the system to trim the process's working set (**believed** from the system
functions it calls; the effect on the figures is not measured) [DG-5].

The figures [DG-2 – DG-8, DG-22 – DG-31]:

| Figure | What it measures |
|---|---|
| *a*, *b*, *c* | the address space walked from address 0 to its end: the total size of the free regions, of the reserved (not committed) regions and of the committed regions, in bytes, then K [DG-3]. Every committed region of the process counts in *c*, whatever made it (the walk does not tell them apart): so it includes the heaps and pools below and the archives' mapped views (2.1.1), and also the program's and libraries' images and whatever the graphics system and the system libraries commit. The executable is marked large-address-aware; measured under Wine, *a* + *b* + *c* was 4,194,240 K (4 GiB less 64 K) in every block of every run [DG-31] |
| *t* | the memory of all textures the resource manager holds (the same list `dump_resources` counts as `textures`; render targets are not in it), lightmap textures included: the sum of each texture's own size, in bytes, then K [DG-2, DG-8, DG-29]. A texture's size is a number the renderer records when it loads the texture, not a query of the graphics system: the byte length of the texture's file, reduced to a quarter for each step the texture quality setting drops (**believed** from the computation; each step removes 0.7502 of the size) [DG-29]. **Disputed in review:** the number of steps is not the setting alone: it is a per-texture value the renderer derives from the texture's name, and a texture whose created form has a single level does not get its size from this computation at all (what it records then is unestablished). The measurement below contradicts a uniform rule: two steps of a quarter would leave 1/16 of the base part, but `texture_lod 2` left 35.5% of it (89,072 of 250,573 K), so many textures are reduced by fewer steps or not at all; which ones, and by how many steps, is **unestablished**. Measured: the 10 lightmap files of `l01_escape` are 1,048,704 bytes each, 10,487,040 in all, and the lightmap part is 10,241 K in every run, also under `texture_lod 2`, while the base part went from 250,573 K to 89,072 K for the same 865 textures [DG-29]. Measured in a level: equal to the sum of the two K figures of the `* t-report - base: …` and `* t-report - lmap: …` lines written during the load (260814 = 250573 + 10241). 0 when no resource manager exists (at startup) [DG-2, DG-10] |
| *h* | the bytes in the in-use blocks of the C runtime's heap: a walk of that heap with the system's heap walk, adding the data size the system reports for each busy block (**believed**: the size requested of the heap, without the heap's own overhead) [DG-4, DG-23]. This is the same measurement as `* phase cmem` (2.4): measured equal to the end-of-load `* phase cmem` in four runs (e.g. 227,118 K) [DG-22]. What it counts: 2.1.1 |
| *p* | the same walk of the process's default heap, a heap distinct from the C runtime's (measured at startup: *h* 6,009 K, *p* 729 K) [DG-4, DG-25]. What it counts: 2.1.1 |
| *g* | the bytes in use in the game library's own memory pool (the pool's total size less its free space) [DG-7]. The pool is memory the game library commits directly from the system in segments (not a heap), and its only user is the game's script machine, which is created with this pool as its allocator [DG-26] |
| *e* | the same for the engine's own pool: committed directly from the system, the allocator of the script machine the resource manager runs the shader scripts (`$game_shaders$`) in [DG-26] |
| *r* | the same for the loaded renderer's own pool (both renderers): committed directly from the system; its users grow lists (**believed**: lists the renderer builds while drawing frames). Measured: 0 at the end of every load, a few hundred K once frames have been drawn (313–377 K 15 s into the level) [DG-7, DG-26] |
| *s* | the bytes saved by sharing identical strings: for every distinct string, (its reference count − 1) × (its length + 1), less 32 for each distinct string, less 17 once; divided by 1024 rounded toward zero, so it can be negative [DG-2, DG-6] |
| *m* | the same for shared memory blocks: for every block, (its reference count − 1) × its size in bytes, less 36 for each block, less 21 once; divided by 1024 rounded toward zero [DG-6]. In the engine, shared blocks are made by the loading of skeletal motions and of skinned models [DG-37]; measured, *m* is 0 at the end of a load without prefetching and 28,221 K with it (2.4.1) [DG-33] |

Every figure is a byte count divided by 1024 and rounded down (K), except *s* and *m* (above) [DG-2,
DG-3].

When a heap walk finds the heap damaged, the program stops with a fatal error (`bad pointer to heap`,
`bad node in heap` or `bad start of heap`) [DG-4].

Measured values (R1, 800 × 600; they vary with the run and the machine and are examples only)
[DG-10]:

| When | *t* | *h* | *p* | *g* | *e* | *r* | *s* | *m* |
|---|---|---|---|---|---|---|---|---|
| startup (2.4) | 0 | 6009 | 724 | 0 | 0 | 0 | 1032 | 0 |
| main menu | 16 | 26449 | 11853 | 1952 | 171 | 0 | 529 | 0 |
| end of the New Game load | 260814 | 230339 | 400289 | 25781 | 248 | 0 | 3665 | 28221 |

Under `-r2` the main menu gave the same four lines with other values [DG-10].

#### 2.1.1 Which memory each figure counts

**The C runtime's heap (*h* and `* phase cmem`).** The engine, the game library, the renderers, the core,
collision, sound, particle, script, XML, network and physics libraries all use one C runtime (the shipped
`msvcr80.dll`), so they share its heap [DG-24]. All of them but the physics library allocate through the
core's memory manager; the physics library allocates from the C runtime directly; both land in this heap
[DG-24]. The memory manager serves a request of up to 862 bytes from one of 54 pools of fixed block sizes
(16, 32, … 864 bytes; a request of *n* bytes goes to the pool of 16 × (⌊(*n* + 1) / 16⌋ + 1) bytes); a
pool takes its memory from the C runtime's heap in blocks of 16 KiB (plus 22 bytes) and never returns
them: a freed small block goes back to its pool, so it still counts in *h*. A larger request is one heap block of the
request plus 23 bytes [DG-24]. So *h* counts every pool's blocks whole (used or free) and every large
block with its 23 bytes [DG-23, DG-24].

**Not in *h*:**

- The three pools of *g*, *e* and *r*: committed directly from the system, counted only in *c* [DG-26].
- The script library's private heap, which it creates executable (**believed**: for compiled script
  code); no figure but *c* counts it [DG-28].
- The Direct3D helper library's allocations: it uses another C runtime and the process heap [DG-24].
- Files read from the archives: an archive is mapped, and a file is read through a view of the archive
  mapped while the file is open (no entry of this edition is compressed, so none is copied; a compressed
  one would be unpacked into a memory-manager block): counted in *c*, not in a heap [DG-27]. A loose
  file of 16 KiB or more is mapped the same way; a smaller one is read into memory [DG-27].
- The graphics resources: the level's vertex and index buffers, the models' buffers and the textures are
  created in Direct3D's managed pool, where Direct3D keeps a copy of each in system memory [DG-30].
  Measured under Wine: lowering `texture_lod` from 0 to 2 lowered *t* by 161,500 K, *p* by 163,626 K and
  *c* by 168,064 K, and left *h* unchanged; so under Wine the textures' copies are in the process heap
  [DG-30]. At the end of the New Game load *p* less *t* was 139,465 K; the level's buffers alone are
  36,404 + 18,868 Kb by their log lines (**believed** to be part of it) [DG-30]. Where Direct3D keeps these
  copies on Windows is **unestablished**.

**The process heap (*p*).** What the system libraries and the Direct3D helper library allocate from the
default heap; under Wine it holds Direct3D's copies of the textures (measured above) [DG-24, DG-30].
What else it holds is **unestablished**.

**Sound.** Measured with sound on (seven New Games, both volumes `snd_volume_eff` and `snd_volume_music`
set to 0, no `-nosound`; Provenance) and from the binaries [DG-38 – DG-48]:

- **Where the sound library's memory comes from.** The sound library (`xrSound.dll`) imports no heap,
  virtual-memory or file-mapping function of the system: it allocates only through the core's memory
  manager and through the shared C runtime's `malloc`, `calloc` and `realloc` (its built-in Ogg Vorbis
  decoder uses the latter). So everything it holds itself is in the C runtime's heap and counts in *h*
  and `* phase cmem` [DG-38].
- **No device, no sound memory.** With `-nosound` the sound device is never created; when it is
  requested but cannot be created (the log says `OpenAL: Can't create sound device.`, measured when the
  system has no audio device), the result is the same. Without a device, creating a sound does nothing:
  no sound file is loaded, no cache or channel exists [DG-39, DG-44]. Measured: a New Game with sound on
  but no audio device gave the same *h* as with `-nosound` (load start 26,111 against 26,115 K, end
  227,113 against 227,118 K), and *p* 1,072 K higher (**believed**: the system's audio libraries looking for a
  device) [DG-44].
- **The sound cache** holds the decoded audio of all sounds. It is made when the device is created (at
  startup, after the startup `stat_memory` of 2.4) as two blocks of the memory manager: *lines* × *bpl*
  bytes of data and 20 bytes per line, where *bpl* is 50 ms of the device's output format (16-bit stereo
  at 44,100 Hz: 8,820 bytes) and *lines* = ⌊`snd_cache_size` × 2,097,152 / *bpl*⌋ + 1. The sound library logs
  `* sound : cache: <size K> kb, <lines> lines, <bpl> bpl` (measured: `snd_cache_size 16` gives
  `32773 kb, 3805 lines, 8820 bpl`; 32 gives `65538 kb, 7609 lines, 8820 bpl`). Measured: 32 instead of 16
  raised *h* at the start of the load by 32,840 K, the 3,804 more lines of 8,820 + 20 bytes [DG-40].
  Its size does not depend on the level or the sounds played.
- **A sound file** (every distinct one, including music and ambient sounds: the library has one kind of
  sound and one way of playing it, no separate stream) is recorded by a 52-byte record kept until
  shutdown and a table of 2 bytes per cache line of its decoded length. Loading it opens the file,
  reads its stream header and comment, and closes it: no file data is kept [DG-41].
- **Channels** (`snd_targets`, 4 … 32): each a record of 800 bytes made when the device is created,
  with one source and three buffers of the device. A channel that plays a sound opens its file, read in
  place like every archive file (counted in *c*, 2.1.1), and decodes it with the decoder (in *h*), through
  the cache; each buffer it hands to the device holds 400 ms of 16-bit mono at 44,100 Hz (35,280
  bytes), filled from one shared buffer of the memory manager [DG-42]. Measured: 4, 16 and 24 channels
  gave the same *h* at the start and the end of the load [DG-46].
- **The sound device.** The library plays through the shipped OpenAL library (`OpenAL32.dll`, which
  allocates from the same C runtime: *h*) and its implementation `wrap_oal.dll` (measured: the device
  `Generic Software` was chosen and created) [DG-43]. That implementation links no shared C runtime library (it carries its own) and
  imports no way of reaching the process heap: it allocates only from heaps it creates itself and from
  virtual memory, so its memory (the buffers the channels hand to it, **believed**: OpenAL copies a
  buffer's data when it is filled) counts in no figure but *c* [DG-43]. Below it is the system's sound
  output. Measured under Wine at the end of the load, against `-nosound`: *h* +33,417 K, *p*
  +1,703 – 1,718 K, *c* +37,432 – 37,496 K, so 2,297 – 2,376 K of the committed growth is in neither heap
  (**believed**: the implementation's own heaps and the sound threads) [DG-45]. Of the *p* growth, 1,072 K
  appear without any device (above); the runs' silent test device keeps its own buffers in *p* too
  (Provenance). Where the system's sound output keeps its buffers on Windows is **unestablished**.
- **Playing.** In the first 15 s in the level *h* grew by 16,309 – 16,378 K with sound against 14,595 K
  without (four runs) [DG-45]: **believed**: the decoders of the playing channels and the records of the
  sounds the level starts; how it divides is **unestablished**.

### 2.2 `dump_resources`

Prints thirteen lines, each `* RM_Dump: <label>: <count>`, in this order and with these labels exactly
(the label includes the spaces shown; every label but `lst_constants` is ten characters long) [DG-11, DG-14]:

| Line | Label (between the backticks) | Counts the resource manager's |
|---|---|---|
| 1 | `textures  ` | textures |
| 2 | `rtargets  ` | render targets |
| 3 | `rtargetsc ` | cube-map render targets |
| 4 | `vs        ` | vertex shaders |
| 5 | `ps        ` | pixel shaders |
| 6 | `dcl       ` | vertex declarations |
| 7 | `states    ` | render-state blocks |
| 8 | `tex_list  ` | texture lists |
| 9 | `matrices  ` | matrix lists |
| 10 | `lst_constants` | constant lists |
| 11 | `v_passes  ` | passes |
| 12 | `v_elements` | shader elements |
| 13 | `v_shaders ` | shaders (**believed**: the one collection not filled by a creation step of its own kind) |

Lines 1–12: each count is the number of entries of the collection that the resource manager creates
that kind of resource into [DG-12]. The command always prints only these counts: the argument is not
used, and the per-resource listing of 2.5 never appears from the command [DG-11, DG-14].

Measured counts [DG-14]:

| When | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| main menu, R1 | 255 | 2 | 0 | 6 | 9 | 6 | 20 | 219 | 0 | 0 | 306 | 306 | 314 |
| main menu, R2 | 280 | 16 | 0 | 12 | 38 | 12 | 44 | 244 | 0 | 0 | 526 | 526 | 328 |
| in a level, R1 | 885 | 5 | 0 | 70 | 28 | 15 | 75 | 2589 | 0 | 0 | 3320 | 3336 | 1200 |

Measured: `dump_resources` executed from the configuration file at startup, before the graphics device
exists, crashes the program: the log ends with `stack trace:` and an `[error]` line, a crash dump is
written beside the log, nothing after the command runs, and the process did not exit by itself (under
`-silent_error_mode`) [DG-20].

### 2.3 `dump_open_files`

The argument is read as a C `atoi` integer (after the lower-casing of CC 2): leading digits count, other
text gives 0 (measured: `1x` acts as 1; `abc`, `-1`, `2` and `0` act alike) [DG-15, DG-18, DG-19]. The
bare name does not run the command (1).

- **Argument 1:** if at least one listed file is open, prints `----opened files`, then one line per
  open entry, then `----total count= <n>`, where *n* is the number of all entries, open or not
  (measured: 2 open + 489 not open = 491). With no open entry it prints nothing [DG-15, DG-18, DG-19].
- **Any other value:** prints `----un-used`, then one line per entry that is not open [DG-15, DG-19].

An entry line is `[<count>] fname:<path>`: *count* is the number of times the entry has been opened,
*path* the file's full path as the file system resolved it, in lower case (measured: e.g.
`[1] fname:z:\work\game\fsgame.ltx`, `[478] fname:z:\work\game\gamedata\config\gameplay\character_drugs.xml`;
files inside the game archives are named by the path they have in the game-data tree, like loose files)
[DG-19]. Entries are printed in the order they were first created (measured: the first entry is the
file-system file `fsgame.ltx`, the first file read) [DG-19].

**The list** [DG-16, DG-17]:

- Files are listed only when the command line contains `-file_activity` (matched in the lower-cased
  command line, like the other options of `steering-the-original.md` 1.1). Without it the list stays
  empty for the whole run: measured, `dump_open_files 1` prints nothing and any other value prints only
  `----un-used`.
- With it, every opening of a file for reading through the game's file system (from an archive or a
  loose file, as a whole or as a stream, including configuration files) adds to the list; files opened
  for writing are not listed.
- On an opening: if an entry with the same path is currently open, it prints
  `file opened at least twice <path>`. Then the first entry with the same path that is not open is
  reused; if there is none, a new entry with count 0 is added at the end. The entry's count grows by 1
  and it becomes open. Measured: the configuration file, read twice at startup, is one entry
  `[2] fname:z:\work\data\cmd.ltx`; no run printed the twice-opened line.
- Closing the file makes its entry not open; entries are never removed.

Measured at the main menu, the open entries were `game.graph` and the configuration script being
executed; in a level also the level's `level.ai`, `level.gct` and `level.details` [DG-19].

### 2.4 When the programs run `stat_memory`

- **At startup**, once, after the renderer and the game library are loaded and before
  `default_controls.ltx` and the configuration file are executed for the second time (CC 1): measured,
  its four lines follow `Loading DLL: xrGame.dll` (and any registry message) and precede
  `Executing config-script "…default_controls.ltx"...`; the texture, script-pool and renderer figures
  are then 0 [DG-9, DG-10].
- **At each stage caption of a load**: whenever a new stage caption of the loading screen begins
  (`loading-screen.md` section 5), before its image is drawn, the engine prints three lines: `* phase
  time: <ms> ms` (milliseconds since the previous caption began or, for the first caption, since the
  load began), `* phase cmem: <n> K` (the bytes in the C runtime heap's in-use blocks at that moment,
  the same measurement as *h* of 2.1, with the same inclusions, 2.1.1; K rounded down) [DG-21, DG-22],
  and the caption's displayed text (`loading-screen.md` 4.3, with the `...`) as a line of its own
  [DG-21]. So the figure printed with a caption is what the heap held when that caption began, and what
  a stage's work keeps is the next group's figure less this one (2.4.1).
  The lines the stage's work writes follow (none, for some stages). Measured for a New Game: seventeen
  such groups, one per caption of `loading-screen.md` section 5 in its order (e.g. `* phase time: 0 ms`,
  `* phase cmem: 26124 K`, `Кэширование объектов...` first, `Клиент: Соединение с localhost...` sixth),
  then the end-of-load pair below [DG-21].
- **At the end of a load**: when the last of the nested loading phases ends (the loading screen of
  `loading-screen.md` finishes), the engine prints `* phase time: <ms> ms` (milliseconds since the
  last stage caption began) and `* phase cmem: <n> K` (the same figure; measured equal to the *h* that
  follows) [DG-22], then runs `stat_memory` [DG-9, DG-10, DG-21]. It does so through the console's input line: the line is set to
  `stat_memory` and executed without the `~ <line>` echo, then emptied, so a line the user was typing in
  the console is lost (**believed**: this is the line the console edits; not measured with the console
  open) [DG-9]. Measured for a New Game; **believed** for every load (loading a save, changing level),
  since they end through the same loading phases (not measured).

#### 2.4.1 What a New Game's load keeps under each caption

Measured in a New Game of `l01_escape` (R1, 800 × 600, `-nosound`, the shipped `user.ltx` with
`texture_lod 0`; see Provenance): the C runtime heap held 26,115 K when the load began and 227,118 K
when it ended, so the load kept 201,003 K there [DG-32]. "Kept" is the next `* phase cmem` less the
caption's own; what a stage allocates and frees again before the next caption is not seen. The column
"without prefetching" is the same load with `-noprefetch` (141,314 K at its end) [DG-32].

**Disputed in review:** the per-caption figures of the table are those of one run and are not stable
across runs with prefetching. In every such run two amounts of about 527 K each fall under different
captions: in this run under `st_server_starting` (732 = 205 + 527) and `st_loading_sectors_portals`
(668 = 141 + 527); in the `texture_lod 2` run under `st_server_starting` and `st_client_spawning`; in the
`-file_activity` run of 2.3, and in three black-box runs of the same New Game on another container image
(all three identical to each other), under `st_prefetching_objects` (about 85,050 K instead of 84,520)
and `st_loading_spatial_db` (4,297 instead of 3,770), with `st_server_starting` at 205. Without
prefetching neither amount appeared during the load. Without `-file_activity`, the load's total (201,003 – 201,005 K) and the
other captions agree across these runs within a few K. What the two amounts are is **unestablished**.

| *n* | Caption id | Kept, K | Without prefetching, K | What it is |
|---|---|---|---|---|
| 1 | `st_prefetching_objects` | 84,520 | 0 | The 394 models listed in `prefetch_visuals_single` (`config\prefetch\prefetch_single.ltx`, included by `system.ltx`), loaded and kept for later use; `prefetch_objects_single` is empty there, so no game object is pre-created [DG-33]. The stage's own line `* [prefetch] memory:  84519Kb` is the same growth measured over its work [DG-33]. |
| 2 | `st_server_starting` | 732 | 205 | The local server (**believed**, from the stage's work in `loading-screen.md`). **Disputed in review:** 527 K of the 732 is one of the two moving amounts above; the stage's own figure is 205 K in every other run. |
| 3 | `st_loading_alife_simulator` | 0 | 0 | |
| 4 | `st_creating_new_game` | 32,164 | 32,179 | The new game made from `spawns\all.spawn`: by the stage's own lines, 8,641 spawn points loaded and between 16,689 and 16,778 objects saved (the count varied between runs) [DG-32]. **Believed**: this is those objects and registries; how it divides among them is **unestablished**. `all.spawn` (4,474,073 bytes) is read in place (2.1.1), not into the heap [DG-27, DG-32]. |
| 5 | `st_server_connecting` | 0 | 0 | |
| 6 | `st_client_connecting_to` | 8 | 8 | |
| 7 | `st_opening_stream` | 0 | 0 | The `level` file (4,094,604 bytes) is opened and read in place [DG-27, DG-32]. |
| 8 | `st_loading_cform` | 72,668 | 72,668 | The collision model: a copy of `level.cform`'s vertices (12 bytes each) and triangles (16 bytes each), 698,013 and 1,375,470 of them, i.e. the file less its 36-byte header (30,383,676 bytes), plus 32 bytes per triangle (44,015,040 bytes; **believed**: the collision model's search tree), plus about 13 K [DG-34]. A further copy of the triangles (12 bytes each) is made and freed during the stage [DG-34]. |
| 9 | `st_loading_shaders` | 1,401 | 1,406 | The level's shaders (**believed**; no texture data is loaded here). |
| 10 | `st_loading_geometry` | 353 | 356 | **Believed**: the descriptions of the level's visuals. The level's vertex and index buffers, 36,404 + 18,868 Kb by the stage's lines, are Direct3D managed buffers, not in this figure (2.1.1) [DG-30, DG-35]. |
| 11 | `st_loading_spatial_db` | 3,770 | 3,778 | The spatial index of the level's static visuals (**believed**, from the stage's work in `loading-screen.md`). |
| 12 | `st_loading_details` | 10 | 10 | `level.details` (8,392,388 bytes) stays open and is read in place [DG-36]; its vertex and index buffers (794 + 154 K by the stage's line) are Direct3D's [DG-36]. |
| 13 | `st_loading_sectors_portals` | 668 | 142 | Sectors, portals, `level.hom`, glows (**believed**). Measured 141 K in the run with `texture_lod 2`, whose client spawning kept 527 K more: those 527 K fall under one caption or the other from run to run (why is **unestablished**) [DG-32]. **Disputed in review:** the amount does not move only between this caption and client spawning; it is one of the two moving amounts above, and the stage's own figure is 141 – 142 K. |
| 14 | `st_loading_ai_objects` | 0 | 1 | `level.ai` (12,505,535 bytes) and `level.gct` stay open and are read in place [DG-36]. |
| 15 | `st_client_spawning` | 4,709 | 4,446 | The client's objects of the level (**believed**, from the stage's work). |
| 16 | `st_loading_textures` | 0 | 0 | The textures' data goes to Direct3D (counted in *t*; under Wine in *p*, 2.1.1); their files are read in place [DG-27, DG-29, DG-30]. |
| 17 | `st_client_synchronising` | 0 | 0 | |

Without prefetching the load also ends with 234 fewer base textures (631 instead of 865; *t* 81,773 K
lower), *m* 0 instead of 28,221 K, *p* 149,320 K and *c* 242,176 K lower, while client spawning kept 263 K
less, not more [DG-33]: so the prefetched models and their textures are not otherwise loaded by this
load (measured for `l01_escape`), and they stay held after it (**believed**: they are kept for objects
that appear later). In a run with `texture_lod 2` every caption kept the same as with `texture_lod 0`
but for the 527 K of caption 13 [DG-32].

**With sound on.** The table above was measured with `-nosound`. Six New Games with sound on (a working,
silent device; both volumes 0; `snd_cache_size 16` unless stated; Provenance) gave [DG-45, DG-46]:

- When the load began the heap held 59,492 K instead of 26,115 K (in all five runs with
  `snd_cache_size 16`): 33,377 K more, of which 32,773 K are the sound cache and 74 K its line table
  (2.1.1); the other 530 K are the rest of what sound keeps from startup (**believed**: its channels, the
  OpenAL library's own memory and the sounds made before the load; how it divides is **unestablished**). With `snd_cache_size 32`: 92,332 K.
- `st_loading_ai_objects` kept 40 K instead of 0 in every run (566 K in one, with one of the moving
  amounts of 2.4.1); what these 40 K are is **unestablished**.
- `st_client_spawning` kept 4,709 K as without sound in three runs, 5,236 K (one of the moving amounts)
  in two, and 8,429 K once (3,720 K more; why is **unestablished**).
- Every other caption kept the same as without sound, but for the moving amounts of about 527 K, which
  fall under the prefetching, new-game, sectors, AI-objects and client-spawning captions from run to run
  (in five of the six runs one of them fell under `st_creating_new_game`, 32,692 K; none under
  `st_server_starting`, 205 K).
- At the end of the load *h* was 260,535 K in four of the five runs with `snd_cache_size 16` (263,728 K in
  the fifth, the 3,720 K above; **Disputed in review:** the fifth run ended 3,193 K above the other four, not 3,720 K; the other 527 K of its `st_client_spawning` figure is a moving amount, missing from its `st_loading_sectors_portals` caption) against 227,118 K: the load with sound keeps 201,043 K, 40 K more than
  without, and the 33,417 K difference is the startup part above plus those 40 K. *t*, *e* and the other
  captions' work are the same; sound adds nothing to the textures.

### 2.5 At shutdown

When the graphics device is destroyed at exit, after `Destroying Direct3D...`, the thirteen lines of
2.2 are printed again by the resource manager itself [DG-13, DG-14]. In this case, after each of the
lines 1–5 whose count is not 0, one line per remaining resource of that kind follows,
`*        : <n>: <name>` (*n* printed with at least three characters, right-aligned; **believed** to be
the resource's reference count) [DG-13]. Measured in every run (menu and level, R1 and R2): all counts
were 0, so no such lines appeared, and the next line was `refCount:pBaseZB 1` [DG-14].

## 3. Unestablished

- What exactly the memory release of 2.1 changes in the figures (not measured against a run without
  it).
- What the renderer's own pool holds (2.1, believed: lists built while drawing).
- Where Direct3D keeps its system-memory copies on Windows, and what else the process heap holds (2.1.1;
  under Wine the textures' copies are measured in it).
- Where the system's sound output keeps its buffers on Windows, and what the 2,297 – 2,376 K of committed
  memory in neither heap are, with sound on (2.1.1; measured only under Wine).
- What the 530 K the sound device keeps at startup beyond the cache are, what the 40 K kept under
  `st_loading_ai_objects` with sound are, and what the 3,720 K once kept under `st_client_spawning` were
  (2.4.1).
- How the growth of *h* while sounds play divides among decoders and sound records (2.1.1).
- What the script library's private heap holds (2.1.1, believed: compiled script code).
- How the growth of creating the new game divides among its objects and registries, and what the
  growth of the shaders, geometry, spatial-index, sectors and client-spawning captions is beyond the
  caption's work (2.4.1, believed).
- Why 527 K fall under the sectors caption in some runs and under client spawning in others (2.4.1). **Disputed in review:** two such amounts move, among the prefetching, server-start, spatial-index, sectors and client-spawning captions (2.4.1); what they are and what places them is unestablished.
- Whether a texture's recorded size is reduced exactly as stated for every kind of texture (2.1,
  believed from the computation; checked only on the lightmaps).
- Whether the end of loading a save or changing level runs `stat_memory` (2.4, believed).
- Whether a line being typed in the open console is lost at the end of a load (2.4, believed).
- What *n* of the shutdown listing is (2.5, believed reference count); that listing was never seen.
- The `file opened at least twice` line was never seen in a run (2.3).
- What `stat_memory` and `dump_open_files` do when executed from the first, silent execution of the
  configuration file at startup (CC 1); from the second they work (measured [DG-20]).

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; the shipped `__support\userdocs\user.ltx`; the archives' script `bind_stalker.script`, extracted
  with the project's archive tool.
- Static analysis with Ghidra 12.2 (headless, read-only) of `XR_3DA.exe` (the two engine commands, the
  resource manager's count lines and its destruction, startup, the stage captions and the end of loading, the console's line
  execution), `xrGame.dll` (the memory statistics command), `xrCore.dll` (address-space and heap
  statistics, memory release, string and memory sharing statistics, the open-file list, the command-line
  option), `xrRender_R1.dll` and `xrRender_R2.dll` (the renderer's memory figure).
- Nine headless runs of the unmodified `XR_3DA.exe` under Wine in the project's container
  (`bazel run //tools/xtask -- wine run`), with `--skipmemcheck -silent_error_mode -nosound -ltx`
  and a configuration file made of the shipped `user.ltx` plus `vid_mode 800x600`: at the main menu
  under R1, R1 with `-file_activity`, and R2 (`-r2`); in a New Game under R1 with `-file_activity`
  (`-start server(all/single/alife/new) client(localhost)`, `steering-the-original.md` 1.3); and with
  the commands in the configuration file at startup. Commands were run from configuration scripts
  (`cfg_load`) started by loose copies of two shipped scripts with one added line each (the main menu's
  script when the menu is created; the player's script 15 s into the level); results read from the
  log files the runs wrote. Three more New Games under R1 (2.1.1, 2.4.1), each running `stat_memory`
  15 s into the level and quitting: with the configuration above (`texture_lod 0`), with `texture_lod 2`,
  and with `-noprefetch`.
- The archives' file tables (`gamedata.db0` – `gamedata.dbd`, 25,684 files), read with the project's
  archive specification; the shipped configuration `config\system.ltx` and
  `config\prefetch\prefetch_single.ltx`; `levels\l01_escape\level.cform`'s header.
- Static analysis also of `xrCDB.dll` (the collision model), `xrLUA.dll` (its private heap), the
  game library's and the engine's script pools, the renderers' texture loading and buffer creation, the
  core library's memory manager and file reading, and the import tables of the shipped modules.
- Approved specifications of the repository read for context (CC, CS, `steering-the-original.md`).
- Sound (2.1.1, 2.4.1): static analysis with Ghidra 12.2 (headless, read-only) of `xrSound.dll` (the sound
  manager's creation, device creation, the sound cache, sound files, channels and the built-in decoder) and
  the import tables of `xrSound.dll`, `OpenAL32.dll` and `wrap_oal.dll`. Seven New Games under R1 as above
  but with sound on (no `-nosound`) and the configuration's `snd_volume_eff` and `snd_volume_music` set to
  0, each running `stat_memory` 15 s into the level and quitting (nothing was audible: the container has no
  path to any speaker). The container's Wine has no audio driver, so for six of them the prefix was given a
  silent test device: a native 32-bit `mmdevapi.dll` written for these runs (dirty workspace
  `consnd/run/nullmmdev/`, built in the container with its MinGW), offering one render endpoint that
  accepts the formats asked for and consumes the written frames in real time without playing them; Wine's
  own DirectSound and wave output ran unchanged on it, and the game's OpenAL device `Generic Software`
  was created on it. Its stream buffers are in the process heap (*p*): one per open stream, of the length the stream asks
  for (at least 30 ms).
  Those six: three with the defaults (`snd_cache_size 16`, `snd_targets 24`), one with
  `snd_cache_size 32`, one with `snd_targets 4` and one with `snd_targets 48` (out of range, rejected, so
  the default 16). The seventh had no device at all.
- No leaked source code, derived projects or third-party documentation of the engine were used.


## Gaps found in review

- The phase timer of 2.4 is restarted (when the load begins and at each stage caption) only while the engine's global timer is not paused; whether it can be paused during a load, and so whether a `* phase time` can then span several captions, is not established.
- Whether the log's stored lines (`console-log.md` 1.4) and the open-file list of 2.3 are held in the C runtime's heap and so counted in *h* and `* phase cmem` (the `-file_activity` run's figure was 9 K higher when the load began and 3,221 K higher at its end).
- Where the shared strings and shared memory blocks that *s* and *m* describe are themselves stored, and so which figure counts them.
- Which heap the Direct3D helper library's own C runtime allocates from, and so whether *p* or no figure but *c* counts those allocations.
- Whether the per-caption figures of 2.4.1 differ under R2 (all the New Game runs used R1).
- How the sound device's output rate (`snd_freq`, 22,050 or 44,100 Hz) changes the cache's bytes per line and line count and the channels' buffer size with sound on (all runs used 44,100 Hz).
