# Console commands

How the original's console turns a line into a command: which commands exist, how a line is split,
looked up and given to its command, how each kind of command parses its argument, shows its value
(**status**), describes its valid arguments (**help text**) and rejects a value, what `help` lists,
which commands the configuration file saves, and every command's kind, range, built-in value, help
text, saving rule and bare-name rule. What each command does beyond storing its value is not part of
this specification: `docs/behavior/console-scope.md` section 6 names the specification of each effect.
What could not be established is marked **unestablished**; a statement inferred without a direct check is
marked **believed**, with why. Facts marked **measured** come from headless runs of the shipped game
(Provenance); the others from the shipped binaries.

Conventions:

- "The engine" is `bin\XR_3DA.exe`, "the game library" `bin\xrGame.dll`, "the renderer" the renderer
  library the game loads (`xrRender_R1.dll`, or `xrRender_R2.dll` under `renderer_r2a`/`renderer_r2`),
  all of the shipped Russian edition (GOG build). Column *Lib* of the table is E, G or R.
- A **line** is one console input: typed and entered, a line of a configuration script (`cfg_load`, the
  configuration file at startup), or a line the programs execute themselves. A **command** is a name the
  console knows; a **variable** is a command with a value.
- The **log** is the list of text lines the console shows (`console-scope.md` section 7). Log lines are
  quoted between backticks exactly as written, `<…>` standing for a variable part. "Prints" means
  "writes as one log line".
- Specifications referred to: OS `options-screens.md`, ST `steering-the-original.md`, DM
  `display-mode.md`, CS `console-scope.md` (all `docs/behavior/`).

**Checks.** Every claim carries a tag [CC-*n*]. The checks are one script in the dirty workspace,
`console-commands/checks/run.py`; it reads the shipped binaries (strings, data values, read-only static
analysis) and the logs and files of four headless runs of the shipped game (`console-commands/run/`), and
regenerates the table of section 8 from both, comparing it with this document row by row. All checks pass.

## 1. The commands

- Commands are registered by three programs: the engine (42 registrations), the game library (147) and
  the loaded renderer (68; both renderer libraries register the same names, kinds, ranges and values)
  [CC-1]. Registering a name that exists replaces the earlier command: `r1_dlights_clip` is registered
  twice by the renderer with identical commands, so 256 distinct names exist (42 + 147 + 67) [CC-1].
- Five engine commands — `demo_record_take_point`, `demo_record_clear_points`, `demo_record_use_points`,
  `demo_record_4step`, `demo_record_step` — are registered only when the command line contains the text
  `designer` [CC-1]. Measured: without it they are unknown and `help` lists 251 commands; with `-designer`
  it lists all 256 [CC-1].
- All commands exist before the configuration file is executed at startup (measured: every one answered
  from it) [CC-2]. **Disputed in review:** without `-r2` the configuration file is executed twice at
  startup (measured in every run without `-r2`; once with `-r2`): first right after `Initializing
  Engine...`, before the renderer and the game library are loaded, when none of its lines writes
  anything to the log (not even for the engine's own commands or for unknown names); then, after the
  libraries are loaded and `unbindall` has run, a second time, which is the one every command answered
  from. The statement holds only for that second execution.
- No command is registered disabled [CC-1].
- Commands are kept in ascending **byte order** of their names (`_` 95 before `a` 97; the listing starts
  with `_preset`). This is the order of `help` (5) and of the saved configuration file (6) [CC-1, CC-7].

## 2. Line processing

For every line [CC-2]:

1. Every tab and line-feed character becomes a space; every run of spaces becomes one space; a leading
   space is dropped. A line that is then empty does nothing (measured: a line of spaces prints nothing).
2. A line typed in the console and entered is written to the log as `~ <line>` (the processed line). Lines
   executed by configuration scripts and by the programs are not written (measured for scripts).
3. The **name** is the text up to the first space; the **argument** is the rest after that space, with one
   trailing space removed. So the argument never starts or ends with a space and has no double spaces
   (measured: `<tab><space>mouse_sens<tab><space><tab>0.3<3 spaces>` sets `mouse_sens` to 0.3).
4. The name is looked up exactly: byte comparison, case-sensitive. Unknown name: prints
   `! Unknown command:  <name>` — note **two spaces** before the name (the message text ends in a space and
   the name is appended after one more) (measured: `MOUSE_SENS` and `Mouse_sens 0.2` are unknown). A
   disabled command would print `! Command disabled.`; none is disabled (1).
5. The argument is converted to lower case, except for the three commands that keep case: `cdkey`,
   `mm_net_player_name` and `mm_net_srv_name` (measured: `cfg_save MiXeD_Name` writes
   `mixed_name.ltx`; `mm_net_srv_name AbC  dEf` stores `AbC dEf`) [CC-2, CC-3].
6. A non-empty argument is given to the command. With an empty argument, a command marked "shows" in
   section 8 prints `- <name> <status>` (with an empty status the line ends in the space after the name,
   e.g. `- bind `); a command marked "runs" is run with an empty argument instead (measured: the bare
   `flush` writes the log file) [CC-2].

There is no separator for several commands on one line: `hud_info off; hud_weapon off` gives `hud_info`
the argument `off; hud_weapon off`, which it rejects, and `hud_weapon` is not changed (measured) [CC-2].

## 3. Kinds

Each command has one kind; section 8 gives every command's kind. A **rejected** value leaves the value
unchanged and prints two lines [CC-3]:

    ~ Invalid syntax in call to '<name>'
    ~ Valid arguments: <help text>

The help text is the command's, as listed by `help` (section 8). Statuses are shown by the bare name (2,
step 6), by `help` (5) and saved by `cfg_save` (6).

### 3.1 on/off

- Accepts exactly `on` or `1` (sets the value) and `off` or `0` (clears it); anything else is rejected
  [CC-3]. The comparison follows lower-casing (2, step 5), so `ON` and `On` are accepted; `yes` and `2` are
  rejected (measured) [CC-3].
- Status `on` or `off`. Help text `'on/off' or '1/0'` [CC-4].
- Each on/off command sets or clears a bit mask in a flag word given at its registration [CC-3].

### 3.2 toggle

Executing the command flips the bit and prints `<name> is on` or `<name> is off`; status and help text as
on/off. No shipped command has this kind [CC-3].

### 3.3 integer

- The argument is read as a C `atoi` decimal: an optional sign, then decimal digits up to the first other
  character; no digits read as 0 [CC-3]. The value is accepted when *min* ≤ value ≤ *max* (both inclusive),
  else rejected [CC-3].
- Measured with `g_corpsenum` (0 … 100): `101` and `-1` rejected; `100` accepted; `12abc` → 12; `+5` → 5;
  `abc` → 0; `0x10` → 0 [CC-3].
- Status: the value in decimal. Help text `integer value in range [<min>,<max>]` (decimal) [CC-4].

### 3.4 real

- The argument is read as a C `atof` number (a decimal number with optional sign, fraction and exponent,
  up to the first other character; no number reads as 0) [CC-3]. It is accepted when it is at least
  *min* − 0.00001 (compared in double precision) and, once converted to single precision, at most
  *max* + 0.00001 (in single precision); it is stored as that single-precision number, not clamped
  [CC-3]. Otherwise it is rejected.
- Measured with `mouse_sens` (0.05 … 0.6): `0.6000099` accepted (status `0.60001`); `0.60002`, `0.04` and
  `abc` (0) rejected; `0.123456789` → status `0.12346`. With `cam_inert` (0 … 1): `abc` → `0.`, `1e-1` →
  `0.1`, `0,5` → `0.` (the comma ends the number), `1` → `1.`, `0.5` → `0.5` [CC-3].
- Status: the value printed with C `%3.5f` (five decimals), then trailing `0` characters removed (the
  decimal point stays): `1.`, `0.5`, `100.`; a small negative value prints `-0.` (measured:
  `r2_ls_depth_bias -0.0000004`) [CC-3, CC-4].
- Help text `float value in range [<min>,<max>]`, the bounds with C `%3.3f` (three decimals: 0.00001
  shows `0.000`) [CC-4].

### 3.5 list

- The argument is compared with each value name of the command, ignoring case; the first match stores
  its number; no match rejects [CC-3] (measured: `GD_VETERAN` accepted, `bogus` rejected).
- Status: the name whose number equals the stored value, `?` when none does [CC-4].
- Help text: the value names in order joined by `/`, except `g_game_difficulty`, whose help text is
  `game difficulty` (also in its rejection lines, measured) [CC-3, CC-4].

### 3.6 vector

- The argument is read as C `sscanf` with `%f,%f,%f`: three numbers separated by commas, each optionally
  preceded by spaces; text after the third number is ignored; fewer than three numbers reject [CC-3]. Each
  component must lie within its own *min* … *max* (inclusive, no tolerance), else the value is rejected
  [CC-3].
- Measured with `r2_aa_break`: `0.1,0.2` and `2,0,0` rejected; `0.4, 0.5, 0.6` and `0.7,0.8,0.9x` accepted
  [CC-3].
- Status: C `%f,%f,%f` (six decimals each, no trimming), e.g. `0.800000,0.100000,0.000000`. Help text
  `vector3 in range [<x>,<y>,<z>]-[<x>,<y>,<z>]`, minimum then maximum, each with C `%f` [CC-4].

### 3.7 text

- The argument is stored as given, cut to *size* − 1 characters; never rejected [CC-3]. All three text
  commands have *size* 64 and keep case (2, step 5) (measured: 70 characters stored as 63) [CC-3].
- Status: the text. Help text `string with up to <size> characters` (`64`, not 63; measured) [CC-4].
- Measured: setting `mm_net_player_name` also prints
  `! Unable to find Software\GSC Game World\STALKER-SHOC\ in registry` in a run whose registry has no
  install key (what it writes there belongs to its own specification, CS 11, 9c) [CC-3].

### 3.8 action

An action has no value: its status is empty and its help text is `no arguments` unless the command has
its own (section 8) [CC-4]. What it does with its argument is its effect (CS section 6).

### 3.9 custom

Three commands parse, show and describe their value their own way: `vid_mode`, `ph_frequency` and `name`
(section 4).

## 4. Commands with their own rules

### 4.1 `vid_mode`

The argument is read as `<W>x<H>` (two decimal numbers, C `%dx%d`) [CC-6]. A malformed argument prints
`! Wrong video mode [<argument>]` and leaves the value unchanged (measured: `abc`); `800x600` is stored and
shown as `800x600` (measured) [CC-6]. Status `<W>x<H>`. Help text `change screen resolution WxH` [CC-6].
Which sizes take effect is DM's.

### 4.2 `ph_frequency`

The argument is read as `atof`, clamped to 50 … 200 (never rejected); the stored physics step becomes
1 / value [CC-6]. Status: 1 / step with C `%3.5f`, **not** trimmed (`100.00000`) [CC-6]. Help text
`no arguments` [CC-6]. Measured: `300` → `200.00000`, `10` → `50.00000`, `60` → `60.00000`,
`abc` → `50.00000` [CC-6].

### 4.3 `name`

Only acts and shows a value while a game with a player exists [CC-6]. Then: an argument containing `/`
prints `!  '/' is not allowed in names!` (two spaces after `!`) and changes nothing; otherwise the argument,
cut to 17 characters, is passed on as the player's new name (**believed**: it goes into a network message;
multiplayer, CS 11, 9c); status
`is "<name>" ` [CC-6]. Without a game the status is empty (measured in the main menu) [CC-5]. Help text
`player name` [CC-4]. Never saved (6).

### 4.4 Commands that act when their value is stored

`rs_c_gamma`, `r__tf_aniso`, `ph_iterations`, `g_game_difficulty` and `_preset` parse, show and reject as
their kind but do more when a value is stored; that is their effect (CS section 6) [CC-3].

### 4.5 `mm_net_srv_name`

Starts as the Windows computer name [CC-8] (measured: the run's 15-character computer name).

### 4.6 `cdkey` and `mm_net_player_name`

Measured empty at startup and in the main menu [CC-8]. Whether anything sets them later (the
multiplayer menus, the registry) is **unestablished** (CS 11, 9c).

## 5. `help`

`help` runs with or without an argument and does not use it [CC-5]. It prints [CC-5]:

    - --- Command listing: start ---
    <one line per command, in name order (1)>
    - --- Command listing: end ----

Each command's line is C `%-20s (%-10s) --- %s` of the name, the status and the help text: name padded
with spaces to 20 characters, status to 10; longer ones are not cut, so the line just widens (measured:
`mm_mm_net_srv_dedicated (off       ) --- 'on/off' or '1/0'`) [CC-5]. The line has no leading marker
character [CC-5]. Section 8's *Help text* column is what `help` printed (measured) [CC-4].

Measured: `help` executed from the configuration file at startup stops after the line of `mouse_sens`
(the one before `name`) and the program ends with a crash report [CC-5]. **Believed** cause: `name`'s
status reads the game objects, which do not exist before the graphics device is created; in the main menu
`help` lists every command [CC-5].

## 6. What is saved

`cfg_save` and the save at a normal exit (OS 12.3) write the same file: for each command in name order
(1), its own lines [CC-7]:

- *Saved* "yes": `<name> <status>` + CR LF, when the status is not empty [CC-7].
- "—" (actions): nothing (empty status) [CC-7].
- "no": never a line: `cdkey`, `mm_net_player_name`, `name`, `ra`, `sv_no_auth_check`,
  `demo_record_use_points`, `demo_record_4step`, `demo_record_step` [CC-7].
- "own": `bind` writes `unbindall` then the binding lines (OS 12.3) at its place in the order; `bind_sec`
  and `bind_console` their own lines (OS 12.3); `renderer` is written as `renderer <status>` except when
  the command line contains `-r2` (measured: absent under `-r2`); `load_last_save` writes
  `load_last_save <name>` only when a name is set (measured: absent without one) [CC-7].

Measured: the file holds exactly the "yes" variables of the run plus `renderer`, in name order, each with
its status (e.g. `mouse_sens 0.12346`, `ph_frequency 50.00000`, `r2_aa_break 0.700000,0.800000,0.900000`)
[CC-7].

## 7. Built-in values

The *Built-in* column of section 8 is each variable's status when the configuration file is executed at
startup (**Disputed in review:** at its second execution, see section 1; without `-r2` an earlier
execution comes before the renderer and the game library are loaded) (measured: the bare name of every "shows" command, `name` aside, executed from that file) [CC-2,
CC-8]. Under the R2 renderer every status is the same, except `renderer` itself, which `-r2` sets before
the file runs (measured) [CC-8]. Every measured value equals the value stored in the programs after the
registration code (static) [CC-8]; in particular `mouse_sens` is 0.12 [CC-8]. `vid_mode` shows `1024x768`
at that point and the display mode actually chosen once the graphics device exists (measured: `1280x1024`
in the main menu; DM) [CC-8]. `unbindall` and the bindings are
outside this column (OS 11.5).

## 8. The command table

Columns: *Kind* (3); *Range or values* (for a list each name with its stored number; for text the
characters kept); *Built-in* (7; `empty` for an empty status, `—` for commands without a value);
*Help text* (3, 5); *Saved* (6); *Bare name* (2, step 6). Every row [CC-9].

| Command | Lib | Kind | Range or values | Built-in | Help text | Saved | Bare name |
|---|---|---|---|---|---|---|---|
| `_preset` | R | list | `Minimum` 0, `Low` 1, `Default` 2, `High` 3, `Extreme` 4 | `Default` | `Minimum/Low/Default/High/Extreme` | yes | shows |
| `ai_use_torch_dynamic_lights` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `beclient` | G | action | — | — | `BattlEye Client commands` | — | runs |
| `bemsg` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `beserver` | G | action | — | — | `BattlEye Server commands` | — | runs |
| `bind` | G | action | — | — | `no arguments` | own | shows |
| `bind_console` | G | action | — | — | `no arguments` | own | shows |
| `bind_list` | G | action | — | — | `no arguments` | — | runs |
| `bind_sec` | G | action | — | — | `no arguments` | own | shows |
| `cam_inert` | E | real | 0. … 1. | `0.7` | `float value in range [0.000,1.000]` | yes | shows |
| `cam_slide_inert` | E | real | 0. … 1. | `0.25` | `float value in range [0.000,1.000]` | yes | shows |
| `cdkey` | G | text | at most 63 characters | empty | `string with up to 64 characters` | no | shows |
| `cfg_load` | E | action | — | — | `no arguments` | — | shows |
| `cfg_save` | E | action | — | — | `no arguments` | — | runs |
| `chat` | G | action | — | — | `no arguments` | — | shows |
| `check_for_updates` | G | action | — | — | `no arguments` | — | runs |
| `cl_cod_pickup_mode` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `cl_dynamiccrosshair` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `cl_voteno` | G | action | — | — | `Vote No` | — | runs |
| `cl_votestart` | G | action | — | — | `Starts Voting` | — | shows |
| `cl_voteyes` | G | action | — | — | `Vote Yes` | — | runs |
| `clear_log` | G | action | — | — | `no arguments` | — | runs |
| `demo_play` | G | action | — | — | `no arguments` | — | runs |
| `demo_record` | G | action | — | — | `no arguments` | — | shows |
| `demo_record_4step` | E | integer | 0 … 1 | `0` | `integer value in range [0,1]` | no | shows |
| `demo_record_clear_points` | E | action | — | — | `no arguments` | — | runs |
| `demo_record_step` | E | integer | 0 … 3 | `0` | `integer value in range [0,3]` | no | shows |
| `demo_record_take_point` | E | action | — | — | `no arguments` | — | runs |
| `demo_record_use_points` | E | integer | 0 … 1 | `0` | `integer value in range [0,1]` | no | shows |
| `disconnect` | E | action | — | — | `no arguments` | — | runs |
| `dump_open_files` | E | action | — | — | `no arguments` | — | shows |
| `dump_resources` | E | action | — | — | `no arguments` | — | runs |
| `flush` | G | action | — | — | `no arguments` | — | runs |
| `g_always_run` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `g_autopickup` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `g_backrun` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `g_corpsenum` | G | integer | 0 … 100 | `10` | `integer value in range [0,100]` | yes | shows |
| `g_eventdelay` | G | integer | 0 … 1000 | `0` | `integer value in range [0,1000]` | yes | shows |
| `g_game_difficulty` | G | list | `gd_novice` 0, `gd_stalker` 1, `gd_veteran` 2, `gd_master` 3 | `gd_master` | `game difficulty` | yes | shows |
| `g_kill` | G | action | — | — | `player kill` | — | runs |
| `g_restart` | G | action | — | — | `restart game` | — | runs |
| `g_restart_fast` | G | action | — | — | `restart game fast` | — | runs |
| `g_swapteams` | G | action | — | — | `swap teams for artefacthunt game` | — | runs |
| `get_server_address` | G | action | — | — | `List Players` | — | runs |
| `help` | E | action | — | — | `no arguments` | — | runs |
| `hud_crosshair` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `hud_crosshair_dist` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `hud_info` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `hud_weapon` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `list_actions` | G | action | — | — | `no arguments` | — | runs |
| `load` | G | action | — | — | `no arguments` | — | runs |
| `load_last_save` | G | action | — | — | `no arguments` | own | runs |
| `main_menu` | G | action | — | — | `no arguments` | — | runs |
| `mm_mm_net_srv_dedicated` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_con_publicserver` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_con_spectator` | G | integer | 1 … 32 | `20` | `integer value in range [1,32]` | yes | shows |
| `mm_net_con_spectator_on` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_filter_battleye` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_filter_empty` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_filter_full` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_filter_listen` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_filter_pass` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_filter_wo_ff` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_filter_wo_pass` | G | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_player_name` | G | text | at most 63 characters | empty | `string with up to 64 characters` | no | shows |
| `mm_net_srv_gamemode` | G | list | `st_deathmatch` 2, `st_team_deathmatch` 6, `st_artefacthunt` 7 | `st_deathmatch` | `st_deathmatch/st_team_deathmatch/st_artefacthunt` | yes | shows |
| `mm_net_srv_maxplayers` | G | integer | 2 … 32 | `32` | `integer value in range [2,32]` | yes | shows |
| `mm_net_srv_name` | G | text | at most 63 characters | the computer name (4.5) | `string with up to 64 characters` | yes | shows |
| `mm_net_srv_reinforcement_type` | G | integer | 0 … 2 | `1` | `integer value in range [0,2]` | yes | shows |
| `mm_net_use_battleye` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `mm_net_weather_rateofchange` | G | real | 0. … 100. | `1.` | `float value in range [0.000,100.000]` | yes | shows |
| `mouse_invert` | E | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `mouse_sens` | E | real | 0.05 … 0.6 | `0.12` | `float value in range [0.050,0.600]` | yes | shows |
| `name` | G | custom | — | — | `player name` | no | shows |
| `net_cl_clearstats` | G | action | — | — | `clear client net statistic` | — | runs |
| `net_cl_icurvesize` | G | integer | 0 … 2000 | `0` | `integer value in range [0,2000]` | yes | shows |
| `net_cl_icurvetype` | G | integer | 0 … 2 | `0` | `integer value in range [0,2]` | yes | shows |
| `net_cl_interpolation` | G | real | −1. … 1. | `0.` | `float value in range [-1.000,1.000]` | yes | shows |
| `net_cl_log_data` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `net_cl_resync` | G | action | — | — | `resyncronize client` | — | runs |
| `net_compressor_enabled` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `net_compressor_gather_stats` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `net_compressor_status` | G | action | — | — | `valid arguments is [info info_full on off]` | — | shows |
| `net_dbg_dump_export_obj` | E | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `net_dbg_dump_import_obj` | E | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `net_dbg_dump_update_read` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `net_dbg_dump_update_write` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `net_dedicated_sleep` | E | integer | 0 … 64 | `5` | `integer value in range [0,64]` | yes | shows |
| `net_sv_clearstats` | G | action | — | — | `clear server net statistic` | — | runs |
| `net_sv_gpmode` | G | integer | 0 … 2 | `0` | `integer value in range [0,2]` | yes | shows |
| `net_sv_log_data` | G | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `net_sv_pending_lim` | G | integer | 0 … 10 | `2` | `integer value in range [0,10]` | yes | shows |
| `net_sv_update_rate` | G | integer | 1 … 100 | `30` | `integer value in range [1,100]` | yes | shows |
| `ph_frequency` | G | custom | — | `100.00000` | `no arguments` | yes | shows |
| `ph_iterations` | G | integer | 15 … 50 | `18` | `integer value in range [15,50]` | yes | shows |
| `quit` | E | action | — | — | `no arguments` | — | runs |
| `r1_dlights` | R | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `r1_dlights_clip` | R | real | 10. … 150. | `30.` | `float value in range [10.000,150.000]` | yes | shows |
| `r1_glows_per_frame` | R | integer | 2 … 32 | `16` | `integer value in range [2,32]` | yes | shows |
| `r1_lmodel_lerp` | R | real | 0. … 0.333 | `0.1` | `float value in range [0.000,0.333]` | yes | shows |
| `r1_pps_u` | R | real | −1. … 1. | `0.` | `float value in range [-1.000,1.000]` | yes | shows |
| `r1_pps_v` | R | real | −1. … 1. | `0.` | `float value in range [-1.000,1.000]` | yes | shows |
| `r1_ssa_lod_a` | R | real | 16. … 96. | `64.` | `float value in range [16.000,96.000]` | yes | shows |
| `r1_ssa_lod_b` | R | real | 16. … 64. | `48.` | `float value in range [16.000,64.000]` | yes | shows |
| `r1_tf_mipbias` | R | real | −0.5 … 0.5 | `0.` | `float value in range [-0.500,0.500]` | yes | shows |
| `r2_aa` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_aa_break` | R | vector | (0., 0., 0.) … (1., 1., 1.) | `0.800000,0.100000,0.000000` | `vector3 in range [0.000000,0.000000,0.000000]-[1.000000,1.000000,1.000000]` | yes | shows |
| `r2_aa_kernel` | R | real | 0.3 … 0.7 | `0.5` | `float value in range [0.300,0.700]` | yes | shows |
| `r2_aa_weight` | R | vector | (0., 0., 0.) … (1., 1., 1.) | `0.250000,0.250000,0.000000` | `vector3 in range [0.000000,0.000000,0.000000]-[1.000000,1.000000,1.000000]` | yes | shows |
| `r2_allow_r1_lights` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_gi` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_gi_clip` | R | real | 0.00001 … 0.1 | `0.001` | `float value in range [0.000,0.100]` | yes | shows |
| `r2_gi_depth` | R | integer | 1 … 5 | `1` | `integer value in range [1,5]` | yes | shows |
| `r2_gi_photons` | R | integer | 8 … 256 | `16` | `integer value in range [8,256]` | yes | shows |
| `r2_gi_refl` | R | real | 0.001 … 0.99 | `0.9` | `float value in range [0.001,0.990]` | yes | shows |
| `r2_gloss_factor` | R | real | 0. … 10. | `1.` | `float value in range [0.000,10.000]` | yes | shows |
| `r2_ls_bloom_fast` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_ls_bloom_kernel_b` | R | real | 0.01 … 1. | `0.7` | `float value in range [0.010,1.000]` | yes | shows |
| `r2_ls_bloom_kernel_g` | R | real | 1. … 7. | `3.3` | `float value in range [1.000,7.000]` | yes | shows |
| `r2_ls_bloom_kernel_scale` | R | real | 0.5 … 2. | `1.` | `float value in range [0.500,2.000]` | yes | shows |
| `r2_ls_bloom_speed` | R | real | 0. … 100. | `10.` | `float value in range [0.000,100.000]` | yes | shows |
| `r2_ls_bloom_threshold` | R | real | 0. … 1. | `0.3` | `float value in range [0.000,1.000]` | yes | shows |
| `r2_ls_depth_bias` | R | real | −0.5 … 0.5 | `-0.0001` | `float value in range [-0.500,0.500]` | yes | shows |
| `r2_ls_depth_scale` | R | real | 0.5 … 1.5 | `1.00001` | `float value in range [0.500,1.500]` | yes | shows |
| `r2_ls_dsm_kernel` | R | real | 0.1 … 3. | `0.7` | `float value in range [0.100,3.000]` | yes | shows |
| `r2_ls_psm_kernel` | R | real | 0.1 … 3. | `0.7` | `float value in range [0.100,3.000]` | yes | shows |
| `r2_ls_squality` | R | real | 0.5 … 1. | `1.` | `float value in range [0.500,1.000]` | yes | shows |
| `r2_ls_ssm_kernel` | R | real | 0.1 … 3. | `0.7` | `float value in range [0.100,3.000]` | yes | shows |
| `r2_mblur` | R | real | 0. … 1. | `0.5` | `float value in range [0.000,1.000]` | yes | shows |
| `r2_parallax_h` | R | real | 0. … 0.5 | `0.02` | `float value in range [0.000,0.500]` | yes | shows |
| `r2_slight_fade` | R | real | 0.02 … 2. | `1.` | `float value in range [0.020,2.000]` | yes | shows |
| `r2_ssa_lod_a` | R | real | 16. … 96. | `48.` | `float value in range [16.000,96.000]` | yes | shows |
| `r2_ssa_lod_b` | R | real | 32. … 64. | `32.` | `float value in range [32.000,64.000]` | yes | shows |
| `r2_sun` | R | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `r2_sun_depth_far_bias` | R | real | −0.5 … 0.5 | `0.` | `float value in range [-0.500,0.500]` | yes | shows |
| `r2_sun_depth_far_scale` | R | real | 0.5 … 1.5 | `1.` | `float value in range [0.500,1.500]` | yes | shows |
| `r2_sun_depth_near_bias` | R | real | −0.5 … 0.5 | `-0.00004` | `float value in range [-0.500,0.500]` | yes | shows |
| `r2_sun_depth_near_scale` | R | real | 0.5 … 1.5 | `1.00001` | `float value in range [0.500,1.500]` | yes | shows |
| `r2_sun_details` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_sun_focus` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_sun_lumscale` | R | real | −1. … 3. | `1.` | `float value in range [-1.000,3.000]` | yes | shows |
| `r2_sun_lumscale_amb` | R | real | 0. … 3. | `1.` | `float value in range [0.000,3.000]` | yes | shows |
| `r2_sun_lumscale_hemi` | R | real | 0. … 3. | `1.` | `float value in range [0.000,3.000]` | yes | shows |
| `r2_sun_near` | R | real | 1. … 50. | `12.` | `float value in range [1.000,50.000]` | yes | shows |
| `r2_sun_near_border` | R | real | 0.5 … 1. | `0.66` | `float value in range [0.500,1.000]` | yes | shows |
| `r2_sun_tsm` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_sun_tsm_bias` | R | real | −0.5 … 0.5 | `-0.05` | `float value in range [-0.500,0.500]` | yes | shows |
| `r2_sun_tsm_proj` | R | real | 0.001 … 0.8 | `0.18` | `float value in range [0.001,0.800]` | yes | shows |
| `r2_tf_mipbias` | R | real | −0.5 … 0.5 | `0.` | `float value in range [-0.500,0.500]` | yes | shows |
| `r2_tonemap` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_tonemap_adaptation` | R | real | 0.01 … 10. | `5.` | `float value in range [0.010,10.000]` | yes | shows |
| `r2_tonemap_amount` | R | real | 0. … 1. | `0.5` | `float value in range [0.000,1.000]` | yes | shows |
| `r2_tonemap_lowlum` | R | real | 0.0001 … 1. | `0.001` | `float value in range [0.000,1.000]` | yes | shows |
| `r2_tonemap_middlegray` | R | real | 0. … 2. | `0.25` | `float value in range [0.000,2.000]` | yes | shows |
| `r2_wait_sleep` | R | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `r2_zfill` | R | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `r2_zfill_depth` | R | real | 0.001 … 0.5 | `0.1` | `float value in range [0.001,0.500]` | yes | shows |
| `r2em` | R | real | 0. … 4. | `0.` | `float value in range [0.000,4.000]` | yes | shows |
| `r__detail_density` | R | real | 0.2 … 0.6 | `0.3` | `float value in range [0.200,0.600]` | yes | shows |
| `r__geometry_lod` | R | real | 0.1 … 1.2 | `1.` | `float value in range [0.100,1.200]` | yes | shows |
| `r__supersample` | E | integer | 1 … 4 | `1` | `integer value in range [1,4]` | yes | shows |
| `r__tf_aniso` | R | integer | 1 … 16 | `4` | `integer value in range [1,16]` | yes | shows |
| `ra` | G | action | — | — | `no arguments` | no | shows |
| `renderer` | E | list | `renderer_r1` 0, `renderer_r2a` 1, `renderer_r2` 2 | `renderer_r1` | `renderer_r1/renderer_r2a/renderer_r2` | own | shows |
| `rs_c_brightness` | E | real | 0.5 … 1.5 | `1.` | `float value in range [0.500,1.500]` | yes | shows |
| `rs_c_contrast` | E | real | 0.5 … 1.5 | `1.` | `float value in range [0.500,1.500]` | yes | shows |
| `rs_c_gamma` | E | real | 0.5 … 1.5 | `1.` | `float value in range [0.500,1.500]` | yes | shows |
| `rs_fullscreen` | E | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `rs_refresh_60hz` | E | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `rs_stats` | E | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `rs_v_sync` | E | on/off | — | `off` | `'on/off' or '1/0'` | yes | shows |
| `rs_vis_distance` | E | real | 0.4 … 1.5 | `1.` | `float value in range [0.400,1.500]` | yes | shows |
| `save` | G | action | — | — | `no arguments` | — | runs |
| `screenshot` | R | action | — | — | `no arguments` | — | shows |
| `snd_acceleration` | E | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `snd_cache_size` | E | integer | 4 … 32 | `16` | `integer value in range [4,32]` | yes | shows |
| `snd_efx` | E | on/off | — | `on` | `'on/off' or '1/0'` | yes | shows |
| `snd_restart` | E | action | — | — | `no arguments` | — | runs |
| `snd_targets` | E | integer | 4 … 32 | `16` | `integer value in range [4,32]` | yes | shows |
| `snd_volume_eff` | E | real | 0. … 1. | `1.` | `float value in range [0.000,1.000]` | yes | shows |
| `snd_volume_music` | E | real | 0. … 1. | `0.7` | `float value in range [0.000,1.000]` | yes | shows |
| `start` | E | action | — | — | `no arguments` | — | shows |
| `stat_memory` | G | action | — | — | `no arguments` | — | runs |
| `sv_addmap` | G | action | — | — | `Adds map to map rotation list` | — | shows |
| `sv_anomalies_enabled` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_anomalies_length` | G | integer | 0 … 180 | `3` | `integer value in range [0,180]` | yes | shows |
| `sv_artefact_respawn_delta` | G | integer | 0 … 600 | `30` | `integer value in range [0,600]` | yes | shows |
| `sv_artefact_spawn_force` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `sv_artefact_stay_time` | G | integer | 0 … 180 | `3` | `integer value in range [0,180]` | yes | shows |
| `sv_artefacts_count` | G | integer | 1 … 100 | `10` | `integer value in range [1,100]` | yes | shows |
| `sv_auto_team_balance` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `sv_auto_team_swap` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_banplayer` | G | action | — | — | `Ban Player by Name` | — | shows |
| `sv_banplayer_ip` | G | action | — | — | `Ban Player by IP` | — | shows |
| `sv_bearercantsprint` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_changegametype` | G | action | — | — | `Changing Game Type` | — | shows |
| `sv_changelevel` | G | action | — | — | `Changing Game Type` | — | shows |
| `sv_changelevelgametype` | G | action | — | — | `Changing level and game type` | — | shows |
| `sv_client_reconnect_time` | G | integer | 0 … 60 | `0` | `integer value in range [0,60]` | yes | shows |
| `sv_console_update_rate` | E | integer | 1 … 100 | `1` | `integer value in range [1,100]` | yes | shows |
| `sv_dedicated_server_update_rate` | E | integer | 1 … 1000 | `100` | `integer value in range [1,1000]` | yes | shows |
| `sv_dmgblockindicator` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_dmgblocktime` | G | integer | 0 … 600 | `0` | `integer value in range [0,600]` | yes | shows |
| `sv_dump_online_statistics` | G | action | — | — | `Shows current server settings` | — | runs |
| `sv_dump_online_statistics_period` | G | integer | 0 … 60 | `0` | `integer value in range [0,60]` | yes | shows |
| `sv_forcerespawn` | G | integer | 0 … 3600 | `0` | `integer value in range [0,3600]` | yes | shows |
| `sv_fraglimit` | G | integer | 0 … 100 | `10` | `integer value in range [0,100]` | yes | shows |
| `sv_friendly_indicators` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `sv_friendly_names` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `sv_friendlyfire` | G | real | 0. … 2. | `1.` | `float value in range [0.000,2.000]` | yes | shows |
| `sv_hail_to_winner_time` | G | integer | 0 … 60 | `7` | `integer value in range [0,60]` | yes | shows |
| `sv_kick` | G | action | — | — | `Kick Player by name` | — | shows |
| `sv_listmaps` | G | action | — | — | `List maps in map rotation list` | — | runs |
| `sv_listplayers` | G | action | — | — | `List Players` | — | runs |
| `sv_listplayers_banned` | G | action | — | — | `List of Banned Players` | — | runs |
| `sv_max_ping_limit` | G | integer | 1 … 2000 | `2000` | `integer value in range [1,2000]` | yes | shows |
| `sv_nextanomalyset` | G | action | — | — | `Activating pointed Anomaly set` | — | shows |
| `sv_nextmap` | G | action | — | — | `Switch to Next Map in map rotation list` | — | runs |
| `sv_no_auth_check` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | no | shows |
| `sv_pda_hunt` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_prevmap` | G | action | — | — | `Switch to Previous Map in map rotation list` | — | runs |
| `sv_reinforcement_time` | G | integer | −1 … 3600 | `20` | `integer value in range [-1,3600]` | yes | shows |
| `sv_remove_corpse` | G | integer | −1 … 1 | `1` | `integer value in range [-1,1]` | yes | shows |
| `sv_remove_weapon` | G | integer | −1 … 1 | `1` | `integer value in range [-1,1]` | yes | shows |
| `sv_return_to_base` | G | action | — | — | `no arguments` | — | shows |
| `sv_returnplayers` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_rpoint_freeze_time` | G | integer | 0 … 60000 | `0` | `integer value in range [0,60000]` | yes | shows |
| `sv_setenvtime` | G | action | — | — | `no arguments` | — | shows |
| `sv_setweather` | G | action | — | — | `Set new weather` | — | shows |
| `sv_shieldedbases` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_spectr_firsteye` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_spectr_freefly` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `sv_spectr_freelook` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_spectr_lookat` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_spectr_teamcamera` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_startteammoney` | G | action | — | — | `Set Start Team Money` | — | runs |
| `sv_statistic_collect` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `sv_statistic_save` | G | action | — | — | `saving statistic data` | — | runs |
| `sv_status` | G | action | — | — | `Shows current server settings` | — | runs |
| `sv_teamkill_limit` | G | integer | 0 … 100 | `3` | `integer value in range [0,100]` | yes | shows |
| `sv_teamkill_punish` | G | integer | 0 … 1 | `1` | `integer value in range [0,1]` | yes | shows |
| `sv_timelimit` | G | integer | 0 … 180 | `0` | `integer value in range [0,180]` | yes | shows |
| `sv_unbanplayer_ip` | G | action | — | — | `UnBan Player by IP` | — | shows |
| `sv_vote_enabled` | G | integer | 0 … 255 | `255` | `integer value in range [0,255]` | yes | shows |
| `sv_vote_participants` | G | integer | 0 … 1 | `0` | `integer value in range [0,1]` | yes | shows |
| `sv_vote_quota` | G | real | 0. … 1. | `0.51` | `float value in range [0.000,1.000]` | yes | shows |
| `sv_vote_time` | G | real | 0.5 … 10. | `1.` | `float value in range [0.500,10.000]` | yes | shows |
| `sv_votestop` | G | action | — | — | `Stops Current Voting` | — | runs |
| `sv_warm_up` | G | integer | 0 … 3600 | `0` | `integer value in range [0,3600]` | yes | shows |
| `texture_lod` | E | integer | 0 … 4 | `0` | `integer value in range [0,4]` | yes | shows |
| `unbind` | G | action | — | — | `no arguments` | — | runs |
| `unbind_console` | G | action | — | — | `no arguments` | — | shows |
| `unbind_sec` | G | action | — | — | `no arguments` | — | runs |
| `unbindall` | G | action | — | — | `no arguments` | — | runs |
| `vid_mode` | E | custom | — | `1024x768` | `change screen resolution WxH` | yes | shows |
| `vid_restart` | E | action | — | — | `no arguments` | — | runs |

## 9. Unestablished

- Lines longer than 1023 characters (the name is copied into a 1024-byte buffer): **unestablished** [CC-2].
- Whether `cdkey` and `mm_net_player_name` are set after the main menu opens (4.6).
- The measured runs ran under Wine; the same behaviour under Windows is **believed** (nothing measured
  depends on the system except the computer name and the display mode).
- The `~ <line>` echo of typed lines (2, step 2) is static only: the runs typed nothing.

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user, installed by the project's install assembly (`@gog_original`).
- Static analysis: Ghidra 12.2, headless and read-only, of `XR_3DA.exe` (command kinds, line processing,
  registration, `help`, saving), `xrGame.dll` and both renderer libraries (registrations, own commands),
  `xrCore.dll` (log, startup names); data values and strings read directly from the files.
- Execution: four headless runs of the unmodified `XR_3DA.exe` under Wine in the project's container
  (`xtask wine run`), with `--skipmemcheck -silent_error_mode -nosound -ltx cmd.ltx` and, in two runs,
  `-designer` or `-r2`. The install's `fsgame.ltx` pointed the user-data folder into the run folder, and a
  loose copy of the shipped `scripts\ui_main_menu.script` with one added line executed a second
  configuration file (`cfg_load menu_tests`) when the main menu was created. The registry had no install
  key. Evidence: the log file (`flush`) and the configuration files the game wrote.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- What the first execution of the configuration file at startup (without `-r2`, before the renderer and
  the game library are loaded; section 1) does with its lines, and which commands exist at that point.
- Whether lower-casing an argument (2, step 5) changes non-ASCII letters (Cyrillic text in the Windows
  code page) or only `A` … `Z`.
- What an integer argument outside the signed 32-bit range (e.g. `99999999999`) reads as (3.3).
