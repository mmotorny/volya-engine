# Steering the original game

How the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition, GOG build) can be started and driven
through its normal inputs only — command-line options, the files it reads (file-system file, configuration
files, a benchmark file, a demo list, camera-track files, game configuration), its console and console
commands — so that it starts without dialogs at a chosen size and renderer, reaches gameplay, shows a chosen
view, saves or holds the frame, and quits. Each fact cites its check `[ID]` (`steer/checks/run.py` in the
dirty workspace).

Conventions:

- "User-data folder" is the folder the file-system file names `$app_data_root$` (section 2.1); "saves
  folder" and "screenshots folder" are `savedgames\` and `screenshots\` below it.
- Game-data paths (`config\...`, `scripts\...`) are stored paths in the combined game-data tree
  (`docs/formats/db-archive.md`); configuration syntax is `docs/formats/ltx-config.md`.
- Window details (style, placement, full-screen mode choice) are in `docs/behavior/display-mode.md`; this
  document only says which inputs select them.
- Every fact is established from the shipped binaries and data (static analysis) unless marked
  **measured**, **believed** (with why) or **unestablished**. No run of the game reached gameplay during this
  investigation (Provenance), so the runtime sequence as a whole is not confirmed by observation.

## 1. Command line

### 1.1 How options are read

- The engine copies the **whole process command line** (program path included), converts it to lower case
  and searches it for each option as a **plain substring**, anywhere [CL-1]. Consequences:
  - options and their arguments are case-insensitive, and file-name arguments arrive lower-cased;
  - the copy holds at most **511 characters**; a longer command line must be avoided (what happens then is
    **unestablished**);
  - an option also matches inside other text: e.g. `-i` matches any `-i…` anywhere (a folder name such as
    `c:\x-images`), and `-r2` matches `-r2a`.
- An option with an argument (`-ltx`, `-fsltx`, `-overlaypath`, `-batch_benchmark`, `-wf`) takes the text
  after the option and one space **up to the next space**; such paths cannot contain spaces
  [CL-3, CL-4, CL-8, BM-1].
- `-start` and `-load` take **everything after them to the end of the command line** (section 1.3); put
  them last [CL-5].
- Two options are searched in the **original, case-preserved** command line instead: `--skipmemcheck` and
  `-silent_error_mode` [CL-2, CL-9].
  **Disputed in review:** not only these two. `-fsltx` is found and its argument read in the same
  case-preserved arguments as `--skipmemcheck` (program path not included), so its file name keeps its case;
  `-batch_benchmark` is detected there (case-sensitive, so it must be written in lower case) while its
  argument is read from the lower-cased copy.

### 1.2 Options for unattended runs

| Option | Effect |
|---|---|
| `--skipmemcheck` | skips the startup memory check and its error box (`docs/behavior/startup.md` §2.1) [CL-2] |
| `-silent_error_mode` | on a crash, the crash reporter saves its report instead of showing its dialog (reporter activity 2 instead of 1) [CL-9]; the exact files written are **unestablished** |
| `-nointro` | no logo sequence and no New Game intro video; instead, at the first frame, the console is shown and the main menu is opened (`main_menu on`) [CL-10]; see 3.4 |
| `-nosound` | a switch of the sound library [CL-11]; **believed** to disable sound output (the name; its code was not traced) |
| `-ltx <name>` | configuration file to use instead of `user.ltx` (section 2.2) [CL-4] |
| `-fsltx <file>` | file-system file to use instead of `fsgame.ltx` (section 2.1) [CL-3] |
| `-overlaypath <folder>` | replaces the root of the user-data folder and of the log folder by `<folder>` [CL-8] |
| `-wf <folder>` | sets the working folder before anything is read [CL-8] |
| `-r2a` / `-r2` | force the renderer to `renderer_r2a` / `renderer_r2` (checked in that order), overriding the configuration file [CL-6] |
| `-center_screen` | windowed mode: window centred instead of at the desktop's top-left (`docs/behavior/display-mode.md` A.2) |
| `-i` | input devices are created with their first flag cleared [CL-7]; **believed** to mean non-exclusive keyboard and mouse (the flag's use was not traced) |
| `-ss_tga` | each screenshot is also saved as TGA (section 5) [CL-12] |
| `-batch_benchmark <file>` | benchmark mode (section 6) [BM-1] |
| `-demomode <file>` | demo list (section 6.2) [BM-3] |
| `-start <command text>` | runs a `start …` console command at startup (1.3) [CL-5] |
| `-load <command text>` | runs a `load …` console command at startup (1.3) [CL-5] |

Other switches exist in the binaries (developer and network options); they are not needed here and not
specified.

### 1.3 `-start` and `-load`

When `-start` is present, the text from just after its dash to the end of the command line — i.e. the word
`start` followed by everything after it — is run as one console command; the same for `-load` with `load`
[CL-5]. Both run after the configuration file and before the window and graphics device are created; `-start`
before `-load` [CL-4]. Example (whole command line, working folder `bin\`):

    XR_3DA.exe --skipmemcheck -silent_error_mode -nosound -ltx steer.ltx -start server(all/single/alife/new) client(localhost)

runs `start server(all/single/alife/new) client(localhost)`. `-load <save>` is of no use at startup: the
`load` command needs a game already running and otherwise only logs `! ALife simulator has not been started
yet` [ST-3]; to start from a save use `-start` with `alife/load` (3.2).

## 2. Files read at startup

### 2.1 File-system file (`fsgame.ltx`)

- Name: `fsgame.ltx`, or the `-fsltx` argument. Looked for in the working folder, then in the program's
  folder (`bin\`), then in that folder's parent (the game root) [CL-3]. Not found: a message box
  "Cannot open file "…". Check your working folder." and the program ends [CL-3] — start with working folder
  `bin\` (or the game root) to avoid it.
- Each line is `$alias$ = flag | flag | root | path …`. The installed file sets [CF-8]:
  `$app_data_root$` = the user-data folder (in this installation `C:\users\<user>\Documents\Stalker-SHOC`),
  `$game_saves$` = `$app_data_root$\savedgames\`, `$screenshots$` = `$app_data_root$\screenshots\`,
  `$logs$` = `$app_data_root$\logs\`, `$game_data$` = `<game root>\gamedata\`.
- Loose files below `<game root>\gamedata\` (e.g. `gamedata\config\alife.ltx`) are **believed** to replace
  the archived file of the same stored path (the `$game_data$` line; precedence between loose files and
  archives was not traced here).

### 2.2 Configuration file (`user.ltx` or `-ltx`)

- Name: `user.ltx`, or the `-ltx` argument [CL-4]. Its extension is replaced by `.ltx`; it is looked for in
  the user-data folder, and if not there used as given (relative to the working folder, or an absolute path)
  [CF-1].
- Startup order [CL-4]: `unbindall` runs, then **every line of the file runs as a console command**
  (`cfg_load <name>`), then `-start`, then `-load`, then the window is shown and the graphics device created,
  then the main loop starts. Lines may hold any console command, not only settings.
- The renderer is chosen before this, when the renderer library is loaded: only lines of the file beginning
  `renderer ` are run at that point, unless `-r2a`/`-r2` is given [CL-6]. So `renderer …` in the file works;
  changing it later in the session has no effect until the next start (**believed**: the library is loaded
  once).
- At a **normal exit** (not in benchmark mode) `cfg_save` runs and **rewrites the configuration file** with
  every saved setting, replacing its content (any extra command lines are lost) [CF-3, BM-2]. Use a copy
  made for each run, or benchmark mode.
  **Disputed in review:** the file written is not necessarily the one read. A name that does not start
  with a drive letter and colon is written as `<user-data folder>\<name>.ltx`, even when the file was read
  from the working folder because it was not in the user-data folder; an existing read-only file there has
  its read-only attribute cleared first.
- Shipped defaults (`__support\userdocs\user.ltx`): full screen, `vid_mode 1024x768`, `renderer
  renderer_r1`, key bindings including `bind screenshot kF12`, `bind console kGRAVE`, `bind quit kESCAPE`
  [CF-7]. Settings absent from the file keep the program's built-in values (**believed**: nothing else sets them).

### 2.3 Console syntax

A console line is: command name, a space, the argument text. Tabs and line feeds become spaces, repeated
spaces collapse; for most commands the argument is lower-cased. With no argument, a setting prints its value
(`- <name> <value>`); an unknown name logs `! Unknown command: <name>` [CF-2]. There is no separator for
several commands on one line [CF-2]. `help` writes every command with its value and help text to the log
[CF-6]. The log is `$logs$\xray_<user>.log`; it is written when `flush` runs and at exit (**believed** from
the measured run: the file stayed empty while the game ran).

## 3. Goal 1 and 2: start without dialogs and reach gameplay

### 3.1 Window size and render settings

Put these in the configuration file (2.2):

| Line | Effect |
|---|---|
| `rs_fullscreen off` / `on` | windowed or full screen [CF-7] |
| `vid_mode <W>x<H>` | size: windowed exactly W × H client area; full screen the matching listed mode, else the first listed mode (`docs/behavior/display-mode.md` A.5) [CF-4] |
| `renderer renderer_r1` / `renderer_r2a` / `renderer_r2` | the R1 library, or the R2 library in its two modes (values 0, 1, 2) [CF-5]; if the R2 library fails to load, R1 is used [CF-5] |
| `_preset <name>`, `r__…`, `r1_…`, `r2_…`, `rs_…`, `texture_lod` … | render settings, as listed by `help` [CN-1] |
| `snd_volume_eff 0`, `snd_volume_music 0` | silence |

Example `steer.ltx` (in the user-data folder):

    rs_fullscreen off
    vid_mode 1280x720
    renderer renderer_r1
    _preset Extreme
    snd_volume_eff 0
    snd_volume_music 0
    hud_crosshair off
    hud_info off

Dialogs to avoid [CL-2, CL-3, CL-9]: the memory error box (`--skipmemcheck`), the missing-`fsgame.ltx` box
(working folder), the crash dialog (`-silent_error_mode`). A second instance exits silently while one runs
[CL-2]. The splash window takes no input (`docs/behavior/startup.md` §3). No other startup dialog was found
(**believed**: none in the code paths read).

### 3.2 The `start` command

`start server(<server options>) client(<client options>)` [ST-1]:

- Each option text is what lies between `server(` / `client(` and the next `)`; it cannot contain `)`.
- If there are no client options and the server options contain `single`, the client options become
  `localhost`. With no client options otherwise, it logs `! Can't start game without client. Arguments:
  '…'.` and does nothing [ST-1].
- Server options are fields separated by `/`. For a single-player game with the world simulation ("A-Life")
  they must contain `single` and `alife`, else the game stops with "Invalid server options!" [ST-4].

| Goal | Command |
|---|---|
| New Game | `start server(all/single/alife/new) client(localhost)` — what the menu's New Game runs (then `main_menu off`) [ST-2]; `all` is the spawn file `spawns\all.spawn` |
| Load a save | `start server(<save>/single/alife/load) client(localhost)` — what the load menu runs when no game is running [ST-3]; the save is `<saves folder>\<save>.sav` [ST-3] |
| Load a save during a game | `load <save>` [ST-3] |
| A level without A-Life | `start server(<level>/single) client(localhost)`: the text before the first `/` is looked up as a level name; unknown → log `Can't find level: <name>` [ST-4] |

**Which level:** a New Game always starts in the actor's level of `spawns\all.spawn` (`l01_escape`,
`docs/behavior/start-position.md`). Any other level is reached by loading a save made there. The
no-A-Life form loads a named level directly, but of the 30 shipped `level.spawn` files only `l01_escape`'s
holds an actor entry [ST-5]; what a player or camera does in another level started this way is
**unestablished**.

Difficulty for a New Game: `g_game_difficulty gd_novice|gd_stalker|gd_veteran|gd_master` before `start`
[CN-2].

### 3.3 Example: straight into a New Game

    XR_3DA.exe --skipmemcheck -silent_error_mode -nosound -ltx steer.ltx -start server(all/single/alife/new) client(localhost)

(working folder `bin\`, `steer.ltx` as in 3.1).

### 3.4 Intro videos and the main menu

- Without `-nointro`: the logo sequence plays only when no server options are set and no level exists at the
  first frame [CL-10], so with `-start` it is **believed** not to play (the options are set before the first
  frame, 1.3). When a game is started with `new`, the New Game intro video plays once the level is loaded,
  unless `-nointro` [CL-10]. A game started with `load` has no intro.
- With `-nointro`: logos and intro video are skipped, but at the first frame the console is shown and
  `main_menu on` runs [CL-10]. Whether this still happens when `-start` is given depends on whether the start
  request is processed before the first frame: **unestablished**. If the menu appears over the game,
  `main_menu off` (console) or the menu's own keys close it [Q-2].
- Recommended unattended route: start from a save with `-start …alife/load…` and without `-nointro` — no
  logos, no intro video, no menu (**believed**, from the two rules above).

## 4. Goal 3: view position, direction, field of view, time and weather

### 4.1 What cannot be set directly

The retail build registers 42 engine, 147 game and 68 renderer console commands [CN-1]. None moves the
player, sets the camera, the field of view or the game-time speed: there is no `jump_to_level`, `fov`,
`hud_fov`, `time_factor`, `set_weather`, `g_god` or script-running command [CN-3]. The **player cannot be
placed** at a chosen position by these inputs (other than where a New Game or a save puts it). The
**camera** can, with a camera track (4.2).

### 4.2 Camera tracks (`demo_play`)

- `demo_play <name>[,<cycles>]` plays `<saves folder>\<name>.xrdemo` as a camera effect on top of the
  player's view; single-player only (else logs `For this game type Demo Play is disabled.`), and only with a
  level loaded (`! There are no level(s) started`) [CAM-1].
  **Disputed in review:** the game type is read from the level before the no-level test, so with no level
  the message is not reached; the command is **believed** to end the program with a crash instead. The same
  holds for `demo_record`, which has no level test. Run either only while a level is loaded. It hides the console. If a file
  `<name>.anm` exists in the level's folder or the animation folder, that object animation is played instead
  [CAM-2]. **Disputed in review:** the name looked up below those folders is the whole saves-folder path
  of the track with its extension changed to `.anm`, not `<name>.anm`; whether any file can match is
  **unestablished** (believed never). While it plays, `hud_weapon 0` runs (weapon hidden); at its end `hud_weapon 1` [CAM-2, BM-2].
- **File format** [CAM-2, CAM-4]: a sequence of **keys of 64 bytes**, no header; the file size must be a
  whole multiple of 64, else nothing is played (and the effect is removed). Each key is 16 little-endian
  32-bit floats: a 4 × 4 matrix written row by row, in the row-vector convention of
  `docs/behavior/start-position.md` (point `p` transformed as `p · M`). The stored matrix is the **view
  matrix**, i.e. the inverse of the camera's world matrix

      world = [ r 0 ; u 0 ; d 0 ; e 1 ]      (rows: right r, up u, direction d, eye position e)
      view  = [ r.x u.x d.x 0 ; r.y u.y d.y 0 ; r.z u.z d.z 0 ; −e·r −e·u −e·d 1 ]

  for an orthonormal r, u, d. The game's own recorder (`demo_record`, 4.3) writes the same thing.
- **Playback** [CAM-2, CAM-3]: one key per second. At time `s` since the start, with `k = floor(s)` and
  `t = s − k`, the four keys `k, k+1, k+2, k+3` (indices modulo the key count) are blended row by row with
  the Catmull-Rom weights `0.5(−t³+2t²−t)`, `0.5(3t³−5t²+2)`, `0.5(−3t³+4t²+t)`, `0.5(t³−t²)` (so at `t = 0`
  the result is key `k+1`); the blended matrix is inverted and its rows give the camera's up, direction and
  position. When `k` reaches the key count one cycle ends; after `<cycles>` cycles (missing or 0 → 1) the
  track ends. Total time: cycles × key count seconds. A track whose keys are all equal holds **one fixed
  view** for that long.
- **Field of view:** a track changes only position and orientation (`docs/behavior/view-projection.md` §3.5);
  the player's field of view stays 67.5° vertical (unless the player is aiming). It cannot be set.
- Worked example [CAM-6]: eye at the New Game start (−246.72630, −23.263261, −134.43387), yaw 1.3962544,
  pitch 0.0043165 (angles as in `docs/behavior/start-position.md` §5): d = (0.984797, −0.0043165, 0.173655),
  u = (0.0042509, 0.999991, 0.00074959), r = u × d (the camera's R·(1,0,0) of that document); the key is the `view` matrix above, written 1 or more times; `demo_play
  start_eye,30` with that file as `savedgames\start_eye.xrdemo` holds the view for 30 × keys seconds. The
  dirty workspace's generator `steer/run/mkdemo.py` writes such files.

### 4.3 Recording a track interactively

`demo_record <name>` (single-player, in a game) starts a free camera and writes `<saves folder>\<name>.xrdemo`
(an existing file is replaced) [CAM-1, CAM-4]. Space stores the current view as a key, Esc ends recording,
`` ` `` opens the console [CAM-5]; movement speeds come from `[demo_record]` in `config\system.ltx`
[CAM-4]. The other movement keys are not specified here.

### 4.4 Time of day

- A New Game starts at `[alife] start_time = 05:30:00`, `start_date = 01.05.2012`, with game time running
  at `time_factor = 10` (10 game seconds per real second) (`config\alife.ltx`) [TW-2]. A different start
  time or speed is **believed** possible with a loose `gamedata\config\alife.ltx` (2.1); not confirmed.
- `sv_setenvtime <h>:<m>:<s>.<ms>` sets the game and environment time [TW-1]; it acts only when a condition
  on the running game is met, and whether that holds in single player is **unestablished**.
- A save keeps the time at which it was made (**believed**).

### 4.5 Weather

- `sv_setweather <cycle>` sets the weather cycle; it acts only where the game runs the server, which a
  single-player game is **believed** to do [TW-1]. Cycle names (`config\weathers\environment.ltx`,
  `[weathers]`): `map`, `test`, `default`, `indoor`, `indoor_x18`, `pripyat`, `radar`, `rain`, `sarkofag`,
  `stancia`, `stancia2`, `yantar`, `yantar_indoor` [TW-3].
- The game's script picks each level's cycle from `game.ltx` (`weathers` of the level's section) when a level
  starts and again on each new game day [TW-4], so a cycle set by command may be replaced then.
- Which weather inside a cycle shows depends on the time of day (`docs/behavior/sky.md`).

## 5. Goal 4: saving the frame

- **Command:** `screenshot` (console), or the `screenshot` action (default key F12) in a game, or F12 in the
  main menu [SS-3]. Typed in the console, the console is on screen when the frame is taken (**believed**:
  the command does not hide it); a key bound with `bind screenshot <key>` or `bind_console screenshot <key>`
  avoids that.
- **Full screen only:** in a window it does nothing but log `~ Can't capture screen while in windowed
  mode...` (both renderers) [SS-1].
- **What is saved** [SS-1]: the front buffer — the whole screen at the full-screen mode's size — as 32-bit
  colour, with the gamma ramp (`rs_c_gamma`, `rs_c_brightness`, `rs_c_contrast`) applied to the pixels and
  alpha opaque, encoded as **JPEG** (quality not set by the game). With `-ss_tga` a second copy as **TGA**.
- **Where** [SS-1, SS-2]: `<screenshots folder>\ss_<user>_<MM-DD-YY>_<HH-MM-SS>_(<level>).jpg`, and
  `ssq_…_(<level>).tga` with `-ss_tga`; `<user>` is the Windows user name, the date and time are local,
  `<level>` is the level name, or `mainmenu` when no level exists. Two screenshots in the same second have the
  same name (**believed** the second replaces the first). Example:
  `screenshots\ss_maksym_11-26-26_01-15-07_(l01_escape).jpg`. The `screenshot` command's argument is not used
  [SS-3].
- **Nothing in the inputs takes a screenshot without a key press or typed command.** Unattended capture is
  therefore either a key sent to the game window from outside, or capture of the window from outside while a
  camera track holds the view (4.2) — in a window, or full screen.

## 6. Unattended sequences

### 6.1 Benchmark mode (`-batch_benchmark`)

- `-batch_benchmark <file>`: `<file>` is read from the user-data folder as a configuration-syntax file with a
  section `[benchmark]`; each line `<name> = <options>` is one run: the game starts with `<options>` as its
  whole command line (so `-ltx`, `-nointro`, `-start`, `-demomode`… are given there), runs, and the next line
  follows; the program ends after the last [BM-1]. This replaces the normal start: `-r2`/`-r2a` and the
  renderer pre-read (2.2) are not applied, so the renderer is the built-in default (`renderer_r1`, **believed**
  from the default flag word in `docs/behavior/display-mode.md` DM-1) [BM-1].
- In this mode, when a camera track ends, `<name>.result` (or `benchmark.result`) with frame-rate figures is
  written to the user-data folder and **`quit` runs** [BM-2]; the configuration file is not rewritten [BM-2].

### 6.2 Demo list (`-demomode`)

- `-demomode <file>`: `<file>` is opened by its path (absolute, or relative to the working folder) [BM-3].
  Each line is `<server options>,<client options>,<track>,<seconds>`. From the first frame of a game, the
  current game is left, a game is started with those options (as `start`), and `demo_play <track>` runs; after
  `<seconds>` seconds the next line follows. After the last line it starts again from the first [BM-3].
- Example line: `all/single/alife/new,localhost,start_eye,40`.
- Whether `demo_play` here runs before the level has finished loading (the track then begins when the game
  view first updates) is **unestablished**.

### 6.3 Example: one held view, then exit

User-data folder:

    bench.ltx
        [benchmark]
        view1 = -nosound -ltx steer.ltx -demomode c:\steer\view1.list
    steer.ltx        (3.1)
    savedgames\start_eye.xrdemo   (4.2, 20 equal keys)

`c:\steer\view1.list`: `mysave/single/alife/load,localhost,start_eye,30`

Command line: `XR_3DA.exe --skipmemcheck -silent_error_mode -batch_benchmark bench.ltx`. Expected (from the
rules above, not observed): the save loads, the camera holds the view for 20 s, the track ends, the result
file is written and the program quits.
**Disputed in review:** the demo list is read only on frames while a level is loaded; no line of it is run
before a game exists. The benchmark line above starts no game (no `-start`), so the list never runs and
the expected sequence does not happen. The line needs a game started as well, e.g.
`view1 = -nosound -ltx steer.ltx -demomode c:\steer\view1.list -start server(mysave/single/alife/load) client(localhost)`
(`-start` last, 1.3); the list then leaves that game and starts its own line's game.

## 7. Goal 5: quitting

- `quit` (console, or a configuration file line): leaves the game, then ends the program [Q-1].
  `disconnect` only leaves the game [Q-1].
- Benchmark mode quits by itself when a camera track ends (6.1).
- Closing the window does not end the game (`docs/behavior/display-mode.md` A.2.1).
- A normal exit rewrites the configuration file (2.2).

## 8. Summary of what can and cannot be done

| Goal | Needed | Not possible / not established |
|---|---|---|
| 1 Start without dialogs, size, render settings | working folder `bin\`; `--skipmemcheck -silent_error_mode`; configuration file with `rs_fullscreen`, `vid_mode`, `renderer`, render settings | — |
| 2 Level or New Game, reach gameplay | `-start server(all/single/alife/new) client(localhost)` or `…<save>/single/alife/load…` | any level other than `l01_escape` without a save made there (no-A-Life start unestablished); whether `-nointro` with `-start` opens the menu is unestablished |
| 3 Position, direction, fov, time, weather | camera track file + `demo_play` (or `-demomode`); `sv_setweather`; start time from `config\alife.ltx` | moving the player; setting the fov (fixed 67.5°); `sv_setenvtime` in single player unestablished |
| 4 Save or hold the frame | full screen + `screenshot` command or F12 → JPEG (TGA with `-ss_tga`) in the screenshots folder; or a held camera track for outside capture | a screenshot without a key press or typed command; screenshots in a window |
| 5 Quit | `quit`; benchmark mode quits after its track | — |

## Provenance

- Date: 2026-11-26. Edition: Russian, GOG build (`goggame-1207660573`), binaries in `bin\` as installed,
  archives `gamedata.db0` … `gamedata.dbd`, the installed `fsgame.ltx`, the default `user.ltx` in
  `__support\userdocs\`.
- Static analysis of `XR_3DA.exe`, `xrCore.dll`, `xrGame.dll`, `xrRender_R1.dll`, `xrRender_R2.dll` with
  Ghidra (decompilation of the command-line handling, console, configuration loading, command registration,
  camera-track playback and recording, screenshot, benchmark and demo-list handling); string and constant
  reads from the PE files. Game data read from an extraction made with the project's archive tool (scripts,
  configuration, level spawn files).
- Execution: the unmodified game was started twice under Wine 11.0 on macOS (an existing prefix with audio
  disabled) with `-batch_benchmark` and the files of section 6, once with file-access tracing. Measured: the
  benchmark file was read, but within 150 s neither the demo list nor the camera track was opened [RUN-1] and
  no screenshot or log output resulted; the runs did not reach the menu or gameplay, so no behaviour above is
  confirmed by a run. That environment's failure is not specified as game behaviour.
- No leaked source code, derived engines, third-party documentation or recollection was used.

## Gaps found in review

- New Game intro video (`intro_game`, 182.2 s per `docs/behavior/startup.md` §5): whether it ends by itself, whether it can be skipped by an input, and whether gameplay waits for it are not established; this affects the New Game route of 3.3.
- The commands `demo_record_4step`, `demo_record_step`, `demo_record_take_point`, `demo_record_use_points` and `demo_record_clear_points` are registered and not specified; they may offer other ways to place or step the camera.
- Whether the world (game time, weather, characters, scripted events, harm to the player) keeps running while a camera track holds a view, and whether pausing the game freezes it while the track still sets the camera, is not established.
- The configuration file name given with `-ltx` is held in a 64-byte buffer (at most 63 characters); what happens with a longer name is not established.
- `load_last_save <save>` followed by `load_last_save` with no argument starts the game from that save (`start server(<save>/single/alife/load)`) when no game runs, and is believed to load it during a game; this alternative route for goal 2 is not described.
