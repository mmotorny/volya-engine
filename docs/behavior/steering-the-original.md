# Steering the original game

How the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition, GOG build) can be started and driven
through its normal inputs only — command-line options, the files it reads (file-system file, configuration
files, a benchmark file, a demo list, camera-track files, game configuration), its console and console
commands — so that it starts without dialogs at a chosen size and renderer, reaches gameplay, shows a chosen
view, saves or holds the frame, and quits. Each fact cites its check `[ID]` (`steer/checks/run.py` in the
dirty workspace; the IDs PC-…, EX-…, RUN-2, RUN-3, CAM-7, DRP-… and LLS-… are in `steer549/checks.py`).

Conventions:

- "User-data folder" is the folder the file-system file names `$app_data_root$` (section 2.1); "saves
  folder" and "screenshots folder" are `savedgames\` and `screenshots\` below it.
- Game-data paths (`config\...`, `scripts\...`) are stored paths in the combined game-data tree
  (`docs/formats/db-archive.md`); configuration syntax is `docs/formats/ltx-config.md`.
- Window details (style, placement, full-screen mode choice) are in `docs/behavior/display-mode.md`; this
  document only says which inputs select them.
- Every fact is established from the shipped binaries and data (static analysis) unless marked
  **measured**, **believed** (with why) or **unestablished**. The first investigation had no run that
  reached gameplay; facts marked measured come from later headless runs that did (Provenance).

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
| Load a remembered save | `load_last_save <save>` (only remembers the name), then `load_last_save` (3.2.1) [LLS-1] |
| A level without A-Life | `start server(<level>/single) client(localhost)`: the text before the first `/` is looked up as a level name; unknown → log `Can't find level: <name>` [ST-4] |

#### 3.2.1 `load_last_save`

[LLS-1] `load_last_save <name>` with an argument only remembers `<name>` (up to 519 characters) and does
nothing else. `load_last_save` without an argument then runs `start server(<name>/single/alife/load)` (the
client options become `localhost`, as above) when no world simulation exists, and `load <name>` when one does
(**believed**: the value tested is the simulation's presence in the game's AI object; not traced further).
With no name remembered it logs `! cannot load last saved game since it hasn't been specified`. It is an
alternative route to goal 2 only: it places no camera. Example (configuration file lines, section 2.2, no
`-start`):

    load_last_save mysave
    load_last_save

starts the game from `<saves folder>\mysave.sav` (**believed** to behave as the `-start …alife/load…` form,
whose command it runs; not measured).

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
- **Playback** [CAM-2, CAM-3]: one key per second of **track time** (below). At track time `s`, with `k = floor(s)` and
  `t = s − k`, the four keys `k, k+1, k+2, k+3` (indices modulo the key count) are blended row by row with
  the Catmull-Rom weights `0.5(−t³+2t²−t)`, `0.5(3t³−5t²+2)`, `0.5(−3t³+4t²+t)`, `0.5(t³−t²)` (so at `t = 0`
  the result is key `k+1`); the blended matrix is inverted and its rows give the camera's up, direction and
  position. When `k` reaches the key count one cycle ends; after `<cycles>` cycles (missing or 0 → 1) the
  track ends. Total: cycles × key count seconds of track time. A track whose keys are all equal holds **one
  fixed view** for that long. At its end the player's own camera returns.
- **Track time** [PC-6, PC-7]: starts at 0 when the precache (4.2.1) has ended and advances each frame by the
  game's frame time: 0.9 × the measured duration of the frame + 0.1 × the previous frame time, **at most
  0.1 s**, and **0 while the game is paused**. So below 10 frames per second track time runs slower than real
  time (at most 0.1 s per frame). No console command makes the frame time fixed [PC-7].
- **A new `demo_play`** while a track runs replaces it: the running track is removed first [CAM-7]. The line
  `* [DEMO] FPS: …` is logged when a track that has begun playing (after its precache) ends **or is
  removed**, so this line alone does not show that a track ran to its end [CAM-7].
- **Field of view:** a track changes only position and orientation (`docs/behavior/view-projection.md` §3.5);
  the player's field of view stays 67.5° vertical (unless the player is aiming). It cannot be set.
- Worked example [CAM-6]: eye at the New Game start (−246.72630, −23.263261, −134.43387), yaw 1.3962544,
  pitch 0.0043165 (angles as in `docs/behavior/start-position.md` §5): d = (0.984797, −0.0043165, 0.173655),
  u = (0.0042509, 0.999991, 0.00074959), r = u × d (the camera's R·(1,0,0) of that document); the key is the `view` matrix above, written 1 or more times; `demo_play
  start_eye,30` with that file as `savedgames\start_eye.xrdemo` holds the view for 30 × keys seconds of
  track time, after the precache (4.2.1). The dirty workspace's generator `steer/run/mkdemo.py` writes such
  files.

#### 4.2.1 What playing a track does to the frame: the precache

Every `demo_play` that finds its track (a key file of valid size, or an object animation) starts a
**precache of 50 frames** [PC-1]; a missing file or a size that is not a multiple of 64 returns before it.
(Loading a level precaches 30 frames, a device reset 20 [PC-8]; the interactive recorder, 4.3, none [PC-1].)
While the precache runs:

- **Every frame is cleared to black just before it is shown** [PC-3, PC-4]: after the scene and the HUD are
  drawn, the loading-screen draw runs; the loading-screen picture is freed at the end of the level's own
  precache and made again only when a level load begins, so outside a level load that draw clears the whole
  render target to black instead. The **whole display is black, HUD included**, and a screenshot (section 5)
  saves a black picture. This is the game's behaviour, not the environment's.
- The track itself does nothing: its time does not advance and it does not set the camera [PC-6]. The device
  camera instead turns about the vertical axis at its current position, through 2π × (frames left ÷ 50)
  [PC-5] (not visible, the frames being black).
- The sound level is 0 [PC-3]; a light (colour 255, 255, 255, range 5) is created at the camera position if
  none exists [PC-2].
- At the 50th frame [PC-3]: the gamma ramp is applied again, the light is removed, the sound level returns to
  1, and the log gets `* MEMORY USAGE: <n> K`. From the next frame the scene shows and the track plays.

The precache lasts **50 frames, not a fixed time**: about 1 s at 50 frames per second, **about 25 s at 2 frames
per second** (measured under Wine and box64: averages of 1.4 to 8.9 frames per second in the `* [DEMO] FPS` lines).
A new `demo_play` during a precache starts it again from 50 (the counter is set, not added to) [PC-2].
Measured [RUN-3]: after `demo_play`, captures of the display are all black (every pixel 0) until
`* MEMORY USAGE` is logged; after it the track's view shows, with the console closed and with it open.
A display that "stays black after the track has ended" is a precache that has not ended: the track has not
even started.

The only input that skips the precache is the command-line switch `-gpu_ref`, which also makes the game ask
Direct3D for the **reference device** instead of the hardware one [PC-2]; what that device does under Wine is
**unestablished**, so it is not a way to capture views.

#### 4.2.2 Holding a view for capture

1. Write the track: N equal keys of the wanted view (4.2), e.g. N = 15.
2. In a game (section 3), run `demo_play <name>,<cycles>`; the view then lasts cycles × N seconds of track
   time.
3. Wait until the precache has ended: until the display is no longer black, or until the log holds
   `* MEMORY USAGE` after the `*** Playing demo: …` line (`flush` writes the log, section 2.3; the console
   shows the same lines). A fixed wait must cover 50 frames at the slowest frame rate expected.
4. Capture (section 5, or from outside) while track time is below cycles × N.
5. Before quitting, let any precache end (section 7).

Worked example, 2 frames per second (Wine and box64): `start_eye.xrdemo` with 15 equal keys, `demo_play
start_eye,100`: black for 50 frames (about 25 s); then track time advances 0.1 s per frame (the cap), so the
view holds for 100 × 15 = 1500 s of track time = 15000 frames, about 2 h 5 min of real time. At 60 frames per
second the same track is black for about 0.8 s and then holds for 1500 s.

Worked example, short track: `demo_play caged-lamp` (15 keys, 1 cycle) at 2 frames per second: black for
about 25 s, then the view for 15 s of track time (150 frames, about 75 s), then the player's camera. Captures
4 to 8 s after the command are black; a capture 30 s after it shows the view.

### 4.3 Recording a track interactively

`demo_record <name>` (single-player, in a game) starts a free camera and writes `<saves folder>\<name>.xrdemo`
(an existing file is replaced) [CAM-1, CAM-4]. Space stores the current view as a key, Esc ends recording,
`` ` `` opens the console [CAM-5]; movement speeds come from `[demo_record]` in `config\system.ltx`
[CAM-4]. The other movement keys are not specified here. The recorder starts **no precache** [PC-1], but it
starts from the current view and is moved by keys and mouse, so it cannot be set to a given position and
direction by numbers.

**Designer commands** [DRP-1]: `demo_record_take_point`, `demo_record_clear_points`,
`demo_record_use_points`, `demo_record_4step` and `demo_record_step` exist only when the command line
contains `designer` (substring search, 1.1; e.g. `-designer`); otherwise they are unknown commands.
`demo_record_take_point` widens a horizontal box (x, z) to include the current camera position and logs
`Local BBox (…, …) - (…, …)`; `demo_record_clear_points` resets it; `demo_record_use_points 0|1`,
`demo_record_4step 0|1` and `demo_record_step 0..3` choose whether that box, or one quarter of it, is the
area of the recorder's top-down level-map picture. None of them places or steps the camera, so they are not a
way to place a view.

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
- **During a precache** (4.2.1) the saved picture is black (**measured** by the clean side: an F12 screenshot
  in a track's first seconds was a black JPEG; explained by the black clear of every precache frame [PC-3,
  PC-4]).
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
- **Quitting during a precache** (4.2.1) crashes at exit [EX-1, RUN-2]: the precache's light is removed only at
  the precache's last frame; still held at exit, it is released by an exit-time handler that calls into the
  renderer's object, **believed** already gone then. Measured in 9 headless runs that ended with `quit`: the
  3 that quit while a precache was pending (no `* MEMORY USAGE` after the last `*** Playing demo`) ended with
  an unhandled access violation (exception code `c0000005`) in Wine's log; the 6 others (no track, or the
  precache over) exited with code 0. **Exit code 5** is that exception code's low 8 bits, all a Linux process
  exit status holds (**believed**: Wine passes the exception code on as the exit code; not checked here). So
  wait for `* MEMORY USAGE` before `quit`.

## 8. Summary of what can and cannot be done

| Goal | Needed | Not possible / not established |
|---|---|---|
| 1 Start without dialogs, size, render settings | working folder `bin\`; `--skipmemcheck -silent_error_mode`; configuration file with `rs_fullscreen`, `vid_mode`, `renderer`, render settings | — |
| 2 Level or New Game, reach gameplay | `-start server(all/single/alife/new) client(localhost)` or `…<save>/single/alife/load…` | any level other than `l01_escape` without a save made there (no-A-Life start unestablished); whether `-nointro` with `-start` opens the menu is unestablished |
| 3 Position, direction, fov, time, weather | camera track file + `demo_play` (or `-demomode`), shown after its 50-frame black precache (4.2.1, 4.2.2); `sv_setweather`; start time from `config\alife.ltx` | moving the player; setting the fov (fixed 67.5°); placing a view with the recorder or its designer commands; `sv_setenvtime` in single player unestablished |
| 4 Save or hold the frame | full screen + `screenshot` command or F12 → JPEG (TGA with `-ss_tga`) in the screenshots folder; or a held camera track for outside capture, after its precache | a screenshot without a key press or typed command; screenshots in a window; a picture during a precache (black) |
| 5 Quit | `quit` once no precache is pending (else a crash at exit, exit code 5 under Wine); benchmark mode quits after its track | — |

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
- Targeted fix (issue 549, 2026-11-26): static analysis with Ghidra of the camera-track start, the device's
  precache, frame end, frame loop and frame time, the loading-screen draw, the camera-effect list, the
  exit-time handlers, the Direct3D device creation, the recorder's designer commands and `load_last_save`.
  Measured: the logs, Wine logs and display captures of 9 headless runs of the unmodified game (Wine 11 WoW64
  under box64, Xvfb 1024 × 768, full screen, `renderer_r1`) made by the clean side with `cargo xtask original`
  and kept in its local cache; most reached gameplay in a New Game, and four of them played camera tracks with
  `demo_play`.
- No leaked source code, derived engines, third-party documentation or recollection was used.

## Gaps found in review

- New Game intro video (`intro_game`, 182.2 s per `docs/behavior/startup.md` §5): whether it ends by itself, whether it can be skipped by an input, and whether gameplay waits for it are not established; this affects the New Game route of 3.3.
- Whether the world (game time, weather, characters, scripted events, harm to the player) keeps running while a camera track holds a view is not established; nor is whether the track still sets the camera while the game is paused (its track time then stands still, 4.2), or whether opening the console pauses a single-player game.
- The configuration file name given with `-ltx` is held in a 64-byte buffer (at most 63 characters); what happens with a longer name is not established.
