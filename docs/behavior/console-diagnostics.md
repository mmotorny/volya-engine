# Console diagnostics commands

What the original's three diagnostic console commands write to the log: `stat_memory` (memory
statistics), `dump_resources` (counts of the graphics resources) and `dump_open_files` (the list of files
opened for reading), when the programs run `stat_memory` themselves, and the related lines written at
shutdown. How a line reaches a command, the bare-name rule and the help text are those of
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

**Checks.** Every claim carries a tag [DG-*n*]. The checks are one script in the dirty workspace,
`diag/checks/run.py`; it uses read-only static analysis of the shipped binaries and the logs of six
headless runs of the shipped game (`diag/run/`). All checks pass.

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

The figures [DG-2 – DG-8]:

| Figure | What it measures |
|---|---|
| *a*, *b*, *c* | the 32-bit address space walked from address 0 to its end: the total size of the free regions, of the reserved (not committed) regions and of the committed regions [DG-3] |
| *t* | the memory of all textures the resource manager holds (the same list `dump_resources` counts as `textures`), lightmap textures included: the sum of each texture's own size, in bytes, then K [DG-2, DG-8]. Measured in a level: equal to the sum of the two K figures of the `* t-report - base: …` and `* t-report - lmap: …` lines written during the load (260814 = 250573 + 10241). 0 when no resource manager exists (at startup) [DG-2, DG-10] |
| *h* | the bytes in the in-use blocks of the C runtime's heap (a walk of the heap) [DG-4] |
| *p* | the same for the process's default heap [DG-4] |
| *g* | the bytes in use in the game library's own memory pool (the pool's total size less its free space); **believed** to be the pool of the game's scripts (its label) [DG-7] |
| *e* | the same for the engine's own pool (**believed**: the engine's scripts, its label) [DG-7] |
| *r* | the same for the loaded renderer's own pool (both renderers) [DG-7] |
| *s* | the bytes saved by sharing identical strings: for every distinct string, (its reference count − 1) × (its length + 1), less 32 for each distinct string, less 17 once; divided by 1024 rounded toward zero, so it can be negative [DG-2, DG-6] |
| *m* | the same for shared memory blocks: for every block, (its reference count − 1) × its size in bytes, less 36 for each block, less 21 once; divided by 1024 rounded toward zero [DG-6] |

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
- **At the end of a load**: when the last of the nested loading phases ends (the loading screen of
  `loading-screen.md` finishes), the engine prints `* phase time: <ms> ms` (milliseconds since the
  previous phase) and `* phase cmem: <n> K` (the bytes the program's own memory manager has in use),
  then runs `stat_memory` [DG-9, DG-10]. It does so through the console's input line: the line is set to
  `stat_memory` and executed without the `~ <line>` echo, then emptied, so a line the user was typing in
  the console is lost (**believed**: this is the line the console edits; not measured with the console
  open) [DG-9]. Measured for a New Game; **believed** for every load (loading a save, changing level),
  since they end through the same loading phases (not measured).

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
- What the game library's, the engine's and the renderer's own pools are used for (2.1, believed from
  the labels).
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
  resource manager's count lines and its destruction, startup, the end of loading, the console's line
  execution), `xrGame.dll` (the memory statistics command), `xrCore.dll` (address-space and heap
  statistics, memory release, string and memory sharing statistics, the open-file list, the command-line
  option), `xrRender_R1.dll` and `xrRender_R2.dll` (the renderer's memory figure).
- Six headless runs of the unmodified `XR_3DA.exe` under Wine in the project's container
  (`bazel run //tools/xtask -- wine run`), with `--skipmemcheck -silent_error_mode -nosound -ltx`
  and a configuration file made of the shipped `user.ltx` plus `vid_mode 800x600`: at the main menu
  under R1, R1 with `-file_activity`, and R2 (`-r2`); in a New Game under R1 with `-file_activity`
  (`-start server(all/single/alife/new) client(localhost)`, `steering-the-original.md` 1.3); and with
  the commands in the configuration file at startup. Commands were run from configuration scripts
  (`cfg_load`) started by loose copies of two shipped scripts with one added line each (the main menu's
  script when the menu is created; the player's script 15 s into the level); results read from the
  log files the runs wrote.
- Approved specifications of the repository read for context (CC, CS, `steering-the-original.md`).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Each loading-stage title change during a load (`loading-screen.md`), not only the end of the load, prints `* phase time: <ms> ms` and `* phase cmem: <n> K` (same meanings as 2.4) and then the new stage title as its own log line (measured: seventeen such groups during a New Game before the final pair of 2.4).
