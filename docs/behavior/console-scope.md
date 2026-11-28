# The console (scoping specification)

Scoping specification for the original's in-game console: when it opens and closes and what that does to
the game, how it looks, how text is typed into it, how a typed or scripted line is turned into a command,
every command the shipped programs register (name, kind, range or values, built-in value, effect), the
log lines it shows, and the states in which it cannot be used. It states what approved specifications
already cover and splits the rest into small specifications (section 11). Rules belonging to those
specifications are stated here only as far as they decide scope. What could not be established is marked
**unestablished**; a statement inferred without a direct check is marked **believed**, with why.

Conventions:

- "The engine" is `bin\XR_3DA.exe`, "the game library" `bin\xrGame.dll`, "the renderer" the renderer
  library the game loads (`xrRender_R1.dll` or `xrRender_R2.dll`), "the core library" `xrCore.dll`, all
  of the shipped Russian edition (GOG build). In the command tables the column *Lib* is E (engine),
  G (game library) or R (renderer).
- A **command** is a name the console knows; a **variable** is a command with a value (OS section 7:
  `<name> <value>` sets it, its **status** is the text read back). A **line** is one console input:
  typed and entered, or one line of a configuration script, or a line the programs execute themselves.
- The **log** is the list of text lines every part of the program writes (section 7); the console shows
  its end.
- **Key codes** are the keyboard scan codes of OS section 11.2 (DirectInput codes; `kGRAVE` 41,
  `kESCAPE` 1).
- Positions in the **normalised screen space**: *x* from −1 (left edge) to +1 (right edge), *y* from −1
  (top) to +1 (bottom); a point (*x*, *y*) is drawn at pixel (⌊(*x* + 1) · *W* / 2⌋, ⌊(*y* + 1) · *H* / 2⌋)
  of a *W* × *H* screen [CS-11].
- Abbreviations of approved specifications (`docs/behavior/` unless noted): OS `options-screens.md`,
  ST `steering-the-original.md`, TM `title-menu.md`, SLS `save-load-screens.md`, SAV
  `docs/formats/sav.md`, LL `level-loading.md`, PM `player-movement.md`, DG `display-gamma.md`,
  DM `display-mode.md`, TQ `texture-quality-filtering.md`, VP `view-projection.md`, FP
  `frame-presentation.md`, DL `dynamic-lights.md`, GL `glows.md`, MS `model-shading.md`, TO `torch.md`,
  CF `docs/formats/level-cform.md`, SR `sound-reverb.md`, SPOS `sound-positional.md`, SLM
  `sound-level-music.md`, AT `autosave-timing.md`, PH `player-hits.md`, DO `detail-objects.md`, R1S
  `r1-settings-scope.md`, R2S `r2-renderers-scope.md`, SWA `sound-weather-ambient.md`, TF
  `docs/formats/text-and-fonts.md`, SPIC `docs/formats/save-picture.md`, startup `startup.md`.

**Checks.** Every factual claim carries a tag [CS-*n*]. The checks are one script in the dirty workspace,
`console/checks/run.py` (helpers in the same folder); it reads the shipped binaries directly (strings,
export and import tables, data values) and uses read-only static analysis of the same binaries, the game
data extracted from the archives, the shipped `user.ltx`, and, for CS-3, the approved specifications in
the repository. It regenerates the command tables of section 6 from the binaries and compares them with
this document row by row. All checks pass.

## 1. What the console is

- One console exists; it belongs to the engine. It is a text window over the game: a prompt line and,
  above it, the last lines of the log [CS-11].
- Commands are registered by three programs, each at its own start: the engine (42 registrations), the
  game library (147), and the loaded renderer (68; both renderer libraries register the same list with
  the same kinds, ranges and built-in values) [CS-1]. One name, `r1_dlights_clip`, is registered twice
  by the renderer, with two identical commands (same variable and range), the second replacing the first
  in the table; so a running game knows 256 distinct names
  (42 + 147 + 67) [CS-1]. Five engine commands exist only when the command line contains `designer`
  (section 6, camera tracks) [CS-19].
- The engine registers its commands when the console is initialised, the game library and the renderer
  when each library is loaded; the libraries are loaded before the user configuration file is executed
  (R1S, check RS-4), so every command can be set from it [CS-19].
- Commands are kept in ascending byte order of their names; this is the order of `help` (5.5), of
  `cfg_save` (OS 12.3) and of name completion (4.4) [CS-1, CS-14].

## 2. Opening and closing

### 2.1 When the console opens

| Trigger | Effect | Spec |
|---|---|---|
| The program starts | the console is opened once the application object exists (**believed** before the logo sequence starts, which closes it) [CS-8] | — |
| `-nointro` on the command line | opened, then `main_menu on` is executed, which hides it again (TM 2) [CS-8] | TM |
| A key bound to the action `console` while a level receives input | opened [CS-8] | — |
| A key bound to `console` (primary or secondary slot) while the main menu receives input | opened (TM 4.4, OS 13) [CS-8] | TM, OS |
| The key `kGRAVE` (41) in the camera-recording mode of `demo_record` (ST 4.3) | opened; this key is fixed, not the binding [CS-8] | ST |
| A level is unloaded (`disconnect`, the end of a session) | opened after the level is destroyed; unless a quit or a start is pending, `main_menu off` and `main_menu on` are then executed (which hide it) [CS-8] | — |
| A save-picture capture requested through the menu ends | opened again if it was open when the request was made (SPIC 3) [CS-8] | SPIC |

Opening an open console does nothing. Opening clears the input line, makes the console the receiver of
keyboard and mouse input (2.3) and starts its per-frame update and drawing [CS-10].

### 2.2 When the console closes

| Trigger | Condition | Spec |
|---|---|---|
| Esc (1) pressed in the console | only while a level object exists or the main menu is active; otherwise the key does nothing [CS-9] | — |
| `kGRAVE` (41) pressed in the console without Shift | the same as Esc, with the same condition; with Shift it types `~` (4.1) [CS-9] | — |
| The main menu opens | closed; whether it was open is remembered and restored when the menu closes (TM 2) [CS-8] | TM |
| A level starts loading (`start`) | `main_menu off` is executed, then the console is closed [CS-8] | — |
| `quit` | closed, then the session ends and the program quits (ST 7) [CS-8] | ST |
| `demo_play`, `demo_record` | closed when the track starts (ST 4.2, 4.3) [CS-4] | ST |
| A save-picture capture is requested through the menu | closed for the capture (2.1) [CS-8] | SPIC |
| The program ends | closed [CS-8] | — |

Only Esc and the fixed key 41 close the console from the keyboard: another key bound to `console` does
not close it (while the console is open, keys reach the console, not the bindings) [CS-9, CS-10].
Closing gives input back to the receiver below and stops the update and drawing [CS-10]. Closing does
not clear the input line; opening does [CS-10].

### 2.3 Effect on the game

- **No pause.** Opening or closing the console does not pause the game or change the pause (also SWA 5)
  [CS-10]. A level keeps running behind it.
- **Input capture.** While open, the console is the topmost input receiver: every key press, hold and
  release goes to it, and nothing reaches the level, the HUD or a menu below [CS-10]. The console does
  nothing with mouse movement, buttons or the wheel, so mouse input is lost as well [CS-10]. When the
  console becomes the receiver, pending input is first delivered to the previous receiver, which is then
  notified (**believed**: that it lost input) [CS-10]; what a level or menu does on losing input
  (e.g. stopping held movement) is **unestablished** (spec 2).
- The menu cursor stays as it is (the console neither shows nor hides it) [CS-10].

## 3. Look

All values [CS-11] unless noted. *W* × *H* is the screen size in pixels.

- **Area.** Each frame the console clears a rectangle from the top-left corner, *W* wide and *H* high, or
  ⌊*H* / 2⌋ high when a level is loaded (set at the end of level loading, cleared when the level stops,
  believed: "loaded") or the main menu is active; the colour is opaque (32, 32, 32) (alpha 255). The
  area is cleared, not blended, so nothing of the game or menu shows through it.
- **Font.** The fonts.ltx section `hud_font_di` (TF 2.1: texture `ui\ui_font_console_02`, shader `font`),
  created on first draw with only the *screen-relative size* flag (not *gradient*); its glyph height is
  set every frame to *H* × 0.025 pixels.
- **Prompt line.** The text `>>> ` followed by the input line, followed by `|` while the cursor is in its
  visible phase; colour (128, 128, 255), alpha 255; drawn at *x* = −1 and *y* = *y*₀ − 0.05, where
  *y*₀ = 1 (full height) or 0 (half height). The cursor is always at the end of the line (4.2).
- **Cursor blink.** The cursor phase toggles each time 0.1 s of frame time has accumulated (frame time
  per frame clamped to 0.06666 s, from the engine's millisecond clock) [CS-12].
- **Log lines.** Above the prompt, the log's lines from the newest (minus the scroll offset, 4.5)
  backwards, one per row: the newest at *y*₀ − 0.15 (one empty row above the prompt), every further row
  0.05 higher (one glyph height, *H* × 0.025 pixels), while the row's *y* is not above −1 (the top edge)
  [CS-11]. That gives 38 rows at full height and 18 at half height (**believed**: arithmetic from the
  rule; the last row depends on the rounding of the repeated subtraction, not counted on screen).
- **Line colours** from the line's first character; for the marked kinds the first two characters are
  not drawn [CS-11, CS-15]:

  | First character | Colour (R, G, B, alpha) | First two characters drawn |
  |---|---|---|
  | `!` | (255, 0, 0, 255) | no |
  | `#` | (0, 222, 205, 145) | no |
  | `*` | (128, 128, 128, 255) | no |
  | `-` | (0, 255, 0, 255) | no |
  | `~` | (255, 255, 0, 255) | no |
  | anything else | (255, 255, 255, 255) | yes |

- **Long lines.** A line wider than 1024 (the font's width measure; believed pixels) is broken over
  several rows, which push the older lines up; the exact break positions (the program builds the rows
  recursively and the result may repeat text) are **unestablished** (spec 2).
- **Order with other drawing.** The console is drawn in the device's per-frame drawing list with
  priority 1, a list drawn in descending priority (**believed** later than the level, HUD and menus;
  their priorities are not established here). It is drawn in the main menu and over a level alike [CS-11].
- The console's appearance does not depend on the game data beyond the font (no layout file) [CS-11].

## 4. Input

### 4.1 Characters

The console turns key codes into characters itself, from a fixed table that does not depend on the
keyboard layout or the operating system [CS-13]. Letters are always lower case; Shift (either Shift key,
codes 42 and 54, held) changes only the digits and punctuation:

| Keys (codes) | Without Shift | With Shift |
|---|---|---|
| `1` … `9`, `0` (2 … 11) | `1` … `9`, `0` | `!` `@` `#` `$` `%` `^` `&` `*` `(` `)` |
| 12, 13 | `-` `=` | `_` `+` |
| 26, 27 | `[` `]` | `{` `}` |
| 39, 40 | `;` `'` | `:` `"` |
| 41 (`kGRAVE`) | closes (2.2) | `~` |
| 43 | `\` | `|` |
| 51, 52, 53 | `,` `.` `/` | `<` `>` `?` |
| letters Q … P (16 … 25), A … L (30 … 38), Z … M (44 … 50) | `q` … `p`, `a` … `l`, `z` … `m` | the same |
| Space (57) | space | space |

Every other key (numeric keypad, function keys, Ctrl, Alt, Caps Lock, arrows Left and Right, Home, End,
Delete) types nothing and does nothing else [CS-13]. A character is appended at the end of the line.

### 4.2 Editing

- **Backspace** (14) removes the last character [CS-13]. There is no cursor movement, no deletion
  elsewhere, no selection [CS-13].
- **Enter** (28) executes the line (section 5) and empties it; the scroll offset and the history position
  are reset [CS-5, CS-14]. An empty line does nothing [CS-5].
- **Length.** After every key, a line longer than 1015 characters is cut to 1015 [CS-14].
- **Key repeat.** A held key repeats its action (characters, Backspace, Enter, history and scroll keys
  alike): repeats start once 0.2 s of frame time has accumulated after the press; each repeat raises the
  repeat rate by 0.2 (the *n*-th repeat interval is 0.2 / (1 + 0.2 · (*n* − 1)) s); a new press or any
  release resets the rate to 1 [CS-12].

### 4.3 Clipboard

**Insert** (210) replaces the whole line with the clipboard's text (plain-text clipboard format), up to
1023 characters, every non-printable character replaced by a space and every character converted to
lower case [CS-14]. There is no copy to the clipboard [CS-13].

### 4.4 History and completion

- **History** is not stored separately: it is the log's lines beginning with `~` (5.1: every entered line
  is logged as `~ <line>`) [CS-14]. **Up** (200) steps one entry back, **Down** (208) one
  forward; the entry's text after its first two characters replaces the input line. Stepping back past
  the oldest entry keeps the line and position; stepping forward past the newest empties the line
  [CS-14]. Other log lines beginning with `~` (e.g. the invalid-syntax messages of 5.3) are entries too
  [CS-14, CS-6]. `clear_log` therefore also clears the history [CS-15].
- **Completion.** **Tab** (15) replaces the line with the first command name that is not less (byte
  order) than the whole line, followed by a space; if no such name exists nothing happens [CS-14]. It is
  not a prefix match (the line `zz` is left unchanged; `cfg` becomes `cfg_load `) [CS-14]. If the line
  begins with `ra ` only the text after it is completed and the prefix kept [CS-14].
- There are no hints, no list of matches and no argument completion [CS-13].

### 4.5 Scrolling

**Page Up** (201) moves the shown part of the log one line back, at most to the oldest line;
**Page Down** (209) one line forward, at most to the newest [CS-13]. Entering a line returns to the
newest [CS-5]. The mouse wheel does not scroll [CS-10].

## 5. The command system

### 5.1 Line processing

For every line [CS-5]:

1. Tab and line-feed characters become spaces; runs of spaces become one; a leading space is dropped.
2. An empty result does nothing. Otherwise, if the line was typed in the console and entered, it is
   written to the log as `~ <line>` (lines executed by the programs and by scripts are not).
3. The **name** is the text up to the first space, the **argument** the rest, with one trailing space
   removed. The name is looked up exactly (byte comparison, case-sensitive).
4. Unknown name: the log line `! Unknown command: <name>`. A command marked disabled: `! Command
   disabled.` (no shipped command is disabled [CS-1]).
5. Unless the command keeps case, the argument is converted to lower case. Only `cdkey`,
   `mm_net_player_name` and `mm_net_srv_name` keep case [CS-1].
6. A non-empty argument is given to the command. An empty argument shows the status as the log line
   `- <name> <status>`, except for commands marked "runs" in section 6 (column *Bare name*), which are
   run with an empty argument instead [CS-5].

There is no separator for several commands on one line (ST 2.3) [CS-5].

### 5.2 Kinds

| Kind | Accepted argument | Status | Help text ("valid arguments") |
|---|---|---|---|
| on/off | exactly `on` or `1` → on, `off` or `0` → off | `on` / `off` | `'on/off' or '1/0'` |
| toggle | none needed: every execution flips the bit and logs `<name> is on` / `<name> is off` (no shipped command has this kind) | `on` / `off` | `'on/off' or '1/0'` |
| integer | decimal (C `atoi`); rejected outside *min* … *max* | decimal | `integer value in range [<min>,<max>]` |
| real | C `atof`; rejected unless *min* − 0.00001 ≤ value ≤ *max* + 0.00001 | `%3.5f`, trailing zeros removed (OS 7.1) | `float value in range [<min>,<max>]` with `%3.3f` |
| list | a value name, compared case-insensitively | the current name, `?` if none matches | the names joined by `/` |
| vector | `x,y,z` (three C `%f` numbers separated by commas); rejected unless each component is within its range | `%f,%f,%f` | `vector3 in range [%f,%f,%f]-[%f,%f,%f]` |
| text | any text, cut to the size − 1 characters | the text | `string with up to <size> characters` |
| action | its own (section 6) | empty | `no arguments`, unless its own text (section 6) |
| custom | its own (`vid_mode`, `ph_frequency`, `name`; section 6) | its own | its own |

All [CS-6, CS-7]. Some commands of a kind add their own behaviour around storing the value (e.g.
`rs_c_gamma`, `r__tf_aniso`, `ph_iterations`, `g_game_difficulty`, `_preset`; section 6) [CS-1].

### 5.3 Rejected values

A rejected value (integer, real, list, vector, and on/off for anything but its four words) leaves the
value unchanged and writes two log lines: `~ Invalid syntax in call to '<name>'` and
`~ Valid arguments: <help text>` [CS-6]. Commands of the kind text never reject [CS-6].

### 5.4 Status display

The bare name of a variable shows `- <name> <status>` (5.1 step 6); status formats per kind in 5.2
[CS-5, CS-7].

### 5.5 Help

`help` writes `- --- Command listing: start ---`, then one line per command in name order (1),
formatted `%-20s (%-10s) --- %s` with the name, the status and the help text, then
`- --- Command listing: end ----` [CS-7]. (ST 2.3 states the same.)

### 5.6 Scripts, `cfg_load` and `cfg_save`

Executing a configuration script (`cfg_load`), the user configuration file at startup, the save at exit
and the file format are approved (OS 12.2 – 12.4, ST 2.2). Additional facts [CS-7, CS-18]:

- `cfg_load` writes `Executing config-script "<name>"...` before and `[<path>] successfully loaded.` or
  `! Cannot open script file [<path>]` after; every line of the file goes through 5.1 (not echoed).
- `cfg_save` writes `Config-file [<path>] saved successfully`, or `!Cannot store config file [<path>]`
  when the read-only attribute cannot be cleared.
- **What is saved.** Each command writes `<name> <status>` + CR LF when its status is not empty (column
  *Saved* "yes"); actions have an empty status and write nothing ("—"). Commands that never write a
  line ("no"): `cdkey`, `mm_net_player_name`, `name`, `ra`, `sv_no_auth_check` and the three
  `demo_record_*` variables. Commands with their own rule ("own"): `bind`, `bind_sec`, `bind_console`
  (OS 12.3), `renderer` (omitted under `-r2`, OS 12.3) and `load_last_save` (written as
  `load_last_save <name>` only when a name is set; SAV) [CS-18].

## 6. The command list

Columns: *Kind* as 5.2; *Range or values* (for a list: each name with its stored number); *Built-in*:
the status before any configuration file is applied, from the value stored in the program and the
assignments the registration code makes (**believed** for every row: code run later at startup could
still change a value; no such write was looked for beyond the registration code [CS-2]); *`user.ltx`*:
the value in the shipped file (OS 12.4), `—` if the file has no line for it; *Saved* as 5.6; *Bare name*:
"shows" (the bare name shows the status) or "runs" (it runs the command with an empty argument), 5.1;
*Effect*: what the command does as far as this scope establishes it, quoted texts being the command's own
log lines or help text [CS-4]; *Spec*: the approved specification that specifies the effect (each
mentions the command [CS-3]) or the proposed specification of section 11. All rows [CS-2].

#### Console and scripts

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `help` | E | action | — | — | — | — | runs | writes the command listing to the log (5.5) | ST |
| `cfg_save` | E | action | — | — | — | — | runs | writes the user configuration file (OS 12.3) | OS |
| `cfg_load` | E | action | — | — | — | — | shows | runs a configuration script (OS 12.2) | OS |
| `flush` | G | action | — | — | — | — | runs | writes the log file, then prints `* Log file has been saved successfully!` (7.3) | ST 4 |
| `clear_log` | G | action | — | — | — | — | runs | empties the log, so also the console lines and the command history (4.4), writes the log file, then prints `* Log file has been cleaned successfully!` (7.3) | 4 |

#### Key bindings

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `bind` | G | action | — | — | — | own | shows | OS 11.5 | OS |
| `bind_sec` | G | action | — | — | — | own | shows | OS 11.5 | OS |
| `unbind` | G | action | — | — | — | — | runs | OS 11.5 | OS |
| `unbind_sec` | G | action | — | — | — | — | runs | OS 11.5 | OS |
| `unbindall` | G | action | — | — | — | — | runs | OS 11.5 | OS |
| `list_actions` | G | action | — | — | — | — | runs | prints every action name between `- --- Action list start ---` and `- --- Action list end   ---` | 5 |
| `bind_list` | G | action | — | — | — | — | runs | prints `[<action>] primary is[<key>] secondary is[<key>]` per action between `- --- Bind list start ---` and `- --- Bind list end   ---` | 5 |
| `bind_console` | G | action | — | — | — | own | shows | `bind_console <command line> <key>`: the last word is the key, the words before it the line bound to it (saving: OS 12.3); what pressing the key does is unestablished | 5 |
| `unbind_console` | G | action | — | — | — | — | shows | removes a console-line binding (argument unestablished) | 5 |

#### Game session

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `start` | E | action | — | — | — | — | shows | ST 3.2, LL | ST LL |
| `disconnect` | E | action | — | — | — | — | runs | ST 7 | ST |
| `quit` | E | action | — | — | — | — | runs | hides the console, then ends the session and the program (ST 7) | ST |
| `main_menu` | G | action | — | — | — | — | runs | TM 2 | TM |
| `save` | G | action | — | — | — | — | runs | SAV 1.1, SLS | SAV SLS |
| `load` | G | action | — | — | — | — | runs | SAV, SLS | SAV SLS |
| `load_last_save` | G | action | — | — | `all` | own | runs | SAV, TM | SAV TM |
| `g_restart` | G | action | — | — | — | — | runs | help text `restart game`; effect unestablished (believed multiplayer) | 9b |
| `g_restart_fast` | G | action | — | — | — | — | runs | help text `restart game fast`; effect unestablished (believed multiplayer) | 9b |
| `g_kill` | G | action | — | — | — | — | runs | help text `player kill`; effect unestablished (believed multiplayer) | 9b |
| `check_for_updates` | G | action | — | — | — | — | runs | asks for a patch over the network while the menu exists; on failure prints `! Unable to send query for patch!` | 7 |

#### Video and display

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `vid_mode` | E | custom | — | `1024x768` | `1024x768` | yes | shows | custom: `<W>x<H>`; help text `change screen resolution WxH`; a malformed value prints `! Wrong video mode [%s]` (OS 7.2, 9; DM) | OS DM |
| `vid_restart` | E | action | — | — | — | — | runs | resets the video device (OS 5.4) | OS DM |
| `renderer` | E | list | `renderer_r1` 0, `renderer_r2a` 1, `renderer_r2` 2 | `renderer_r1` | `renderer_r1` | own | shows | OS 7.2, 10; LL; not saved when the command line contains `-r2` (OS 12.3) | OS LL |
| `rs_fullscreen` | E | on/off | — | on | `on` | yes | shows | OS 6.1, DM | OS DM |
| `rs_v_sync` | E | on/off | — | off | `off` | yes | shows | R1S 2.8, DM | R1S DM |
| `rs_refresh_60hz` | E | on/off | — | off | `on` | yes | shows | OS 7.2, DM | OS DM |
| `rs_c_gamma` | E | real | 0.5 … 1.5 | 1. | `1.` | yes | shows | DG | DG |
| `rs_c_brightness` | E | real | 0.5 … 1.5 | 1. | `1.` | yes | shows | DG | DG |
| `rs_c_contrast` | E | real | 0.5 … 1.5 | 1. | `1.` | yes | shows | DG | DG |
| `rs_vis_distance` | E | real | 0.4 … 1.5 | 1. | `1.` | yes | shows | VP 2.1, R1S 2.2 | VP R1S |
| `r__supersample` | E | integer | 1 … 4 | 1 | `2` | yes | shows | R1S 2.7, FP | R1S FP |
| `texture_lod` | E | integer | 0 … 4 | 0 | `0` | yes | shows | TQ | TQ |
| `rs_stats` | E | on/off | — | off | `off` | yes | shows | engine statistics overlay (R1S 2.11) | R1S |
| `screenshot` | R | action | — | — | — | — | shows | `screenshot <name>` takes a screenshot (ST 5, AT); the bare name only shows the empty status | ST AT |

#### Renderer (registered by the loaded renderer library; the same list in both)

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `_preset` | R | list | `Minimum` 0, `Low` 1, `Default` 2, `High` 3, `Extreme` 4 | `Default` | `Extreme` | yes | shows | runs `cfg_load` of the preset file (OS 8, R1S) | OS R1S |
| `r__geometry_lod` | R | real | 0.1 … 1.2 | 1. | `1.` | yes | shows | R1S 2.3 | R1S |
| `r__detail_density` | R | real | 0.2 … 0.6 | 0.3 | `0.2` | yes | shows | R1S 2.4, DO | R1S DO |
| `r__tf_aniso` | R | integer | 1 … 16 | 4 | `16` | yes | shows | R1S 2.6, TQ | R1S TQ |
| `r1_ssa_lod_a` | R | real | 16. … 96. | 64. | `64.` | yes | shows | R1S 2.3 | R1S |
| `r1_ssa_lod_b` | R | real | 16. … 64. | 48. | `48.` | yes | shows | R1S 2.3 | R1S |
| `r1_lmodel_lerp` | R | real | 0. … 0.333 | 0.1 | `0.1` | yes | shows | MS 7 | MS |
| `r1_tf_mipbias` | R | real | −0.5 … 0.5 | 0. | `0.` | yes | shows | R1S 2.6, TQ | R1S TQ |
| `r1_dlights` | R | on/off | — | on | `on` | yes | shows | DL | DL |
| `r1_dlights_clip` | R | real | 10. … 150. | 30. | `75.` | yes | shows | DL; registered twice with identical commands (1) | DL |
| `r1_pps_u` | R | real | −1. … 1. | 0. | `0.` | yes | shows | FP | FP |
| `r1_pps_v` | R | real | −1. … 1. | 0. | `0.` | yes | shows | FP | FP |
| `r1_glows_per_frame` | R | integer | 2 … 32 | 16 | `16` | yes | shows | GL | GL |
| `r2em` | R | real | 0. … 4. | 0. | `2.2` | yes | shows | not in R2S 3; also accepts `on` and `off`, which set and clear a renderer flag; while that flag is set a number prints `* material set to [%s]-[%s], with lerp of [%f]`; effect otherwise unestablished | 7 |
| `r2_aa` | R | on/off | — | off | `off` | yes | shows | R2S 3 | R2S |
| `r2_aa_break` | R | vector | (0., 0., 0.) … (1., 1., 1.) | (0.8, 0.1, 0.) | `0.800000,0.500000,0.000000` | yes | shows | R2S 3 | R2S |
| `r2_aa_kernel` | R | real | 0.3 … 0.7 | 0.5 | `0.5` | yes | shows | R2S 3 | R2S |
| `r2_aa_weight` | R | vector | (0., 0., 0.) … (1., 1., 1.) | (0.25, 0.25, 0.) | `0.250000,0.250000,0.000000` | yes | shows | R2S 3 | R2S |
| `r2_allow_r1_lights` | R | on/off | — | off | `off` | yes | shows | R2S 3 | R2S |
| `r2_gi` | R | on/off | — | off | `off` | yes | shows | R2S 3 | R2S |
| `r2_gi_clip` | R | real | 0.00001 … 0.1 | 0.001 | `0.001` | yes | shows | R2S 3 | R2S |
| `r2_gi_depth` | R | integer | 1 … 5 | 1 | `1` | yes | shows | R2S 3 | R2S |
| `r2_gi_photons` | R | integer | 8 … 256 | 16 | `16` | yes | shows | R2S 3 | R2S |
| `r2_gi_refl` | R | real | 0.001 … 0.99 | 0.9 | `0.9` | yes | shows | R2S 3 | R2S |
| `r2_gloss_factor` | R | real | 0. … 10. | 1. | `1.5` | yes | shows | R2S 3 | R2S |
| `r2_ls_bloom_fast` | R | on/off | — | off | `off` | yes | shows | R2S 3 | R2S |
| `r2_ls_bloom_kernel_b` | R | real | 0.01 … 1. | 0.7 | `0.7` | yes | shows | R2S 3 | R2S |
| `r2_ls_bloom_kernel_g` | R | real | 1. … 7. | 3.3 | `3.` | yes | shows | R2S 3 | R2S |
| `r2_ls_bloom_kernel_scale` | R | real | 0.5 … 2. | 1. | `0.7` | yes | shows | R2S 3 | R2S |
| `r2_ls_bloom_speed` | R | real | 0. … 100. | 10. | `100.` | yes | shows | R2S 3 | R2S |
| `r2_ls_bloom_threshold` | R | real | 0. … 1. | 0.3 | `0.001` | yes | shows | R2S 3 | R2S |
| `r2_ls_depth_bias` | R | real | −0.5 … 0.5 | −0.0001 | `-0.0003` | yes | shows | R2S 3 | R2S |
| `r2_ls_depth_scale` | R | real | 0.5 … 1.5 | 1.00001 | `1.00001` | yes | shows | R2S 3 | R2S |
| `r2_ls_dsm_kernel` | R | real | 0.1 … 3. | 0.7 | `0.7` | yes | shows | R2S 3 | R2S |
| `r2_ls_psm_kernel` | R | real | 0.1 … 3. | 0.7 | `0.7` | yes | shows | R2S 3 | R2S |
| `r2_ls_squality` | R | real | 0.5 … 1. | 1. | `1.` | yes | shows | R2S 3 | R2S |
| `r2_ls_ssm_kernel` | R | real | 0.1 … 3. | 0.7 | `0.7` | yes | shows | R2S 3 | R2S |
| `r2_mblur` | R | real | 0. … 1. | 0.5 | `0.` | yes | shows | R2S 3 | R2S |
| `r2_parallax_h` | R | real | 0. … 0.5 | 0.02 | `0.02` | yes | shows | R2S 3 | R2S |
| `r2_slight_fade` | R | real | 0.02 … 2. | 1. | `0.3333` | yes | shows | R2S 3 | R2S |
| `r2_ssa_lod_a` | R | real | 16. … 96. | 48. | `64.` | yes | shows | R2S 3 | R2S |
| `r2_ssa_lod_b` | R | real | 32. … 64. | 32. | `48.` | yes | shows | R2S 3 | R2S |
| `r2_sun` | R | on/off | — | on | `on` | yes | shows | R2S 3 | R2S |
| `r2_sun_depth_far_bias` | R | real | −0.5 … 0.5 | 0. | `-0.00001` | yes | shows | R2S 3 | R2S |
| `r2_sun_depth_far_scale` | R | real | 0.5 … 1.5 | 1. | `1.` | yes | shows | R2S 3 | R2S |
| `r2_sun_depth_near_bias` | R | real | −0.5 … 0.5 | −0.00004 | `0.00001` | yes | shows | R2S 3 | R2S |
| `r2_sun_depth_near_scale` | R | real | 0.5 … 1.5 | 1.00001 | `1.` | yes | shows | R2S 3 | R2S |
| `r2_sun_details` | R | on/off | — | off | `off` | yes | shows | R2S 3 | R2S |
| `r2_sun_focus` | R | on/off | — | off | `on` | yes | shows | R2S 3 | R2S |
| `r2_sun_lumscale` | R | real | −1. … 3. | 1. | `1.` | yes | shows | R2S 3 | R2S |
| `r2_sun_lumscale_amb` | R | real | 0. … 3. | 1. | `0.5` | yes | shows | R2S 3 | R2S |
| `r2_sun_lumscale_hemi` | R | real | 0. … 3. | 1. | `1.` | yes | shows | R2S 3 | R2S |
| `r2_sun_near` | R | real | 1. … 50. | 12. | `12.` | yes | shows | R2S 3 | R2S |
| `r2_sun_near_border` | R | real | 0.5 … 1. | 0.66 | `0.75` | yes | shows | R2S 3 | R2S |
| `r2_sun_tsm` | R | on/off | — | off | `on` | yes | shows | R2S 3 | R2S |
| `r2_sun_tsm_bias` | R | real | −0.5 … 0.5 | −0.05 | `-0.01` | yes | shows | R2S 3 | R2S |
| `r2_sun_tsm_proj` | R | real | 0.001 … 0.8 | 0.18 | `0.3` | yes | shows | R2S 3 | R2S |
| `r2_tf_mipbias` | R | real | −0.5 … 0.5 | 0. | `0.` | yes | shows | R2S 3 | R2S |
| `r2_tonemap` | R | on/off | — | off | `on` | yes | shows | R2S 3 | R2S |
| `r2_tonemap_adaptation` | R | real | 0.01 … 10. | 5. | `1.` | yes | shows | R2S 3 | R2S |
| `r2_tonemap_amount` | R | real | 0. … 1. | 0.5 | `0.7` | yes | shows | R2S 3 | R2S |
| `r2_tonemap_lowlum` | R | real | 0.0001 … 1. | 0.001 | `0.0001` | yes | shows | R2S 3 | R2S |
| `r2_tonemap_middlegray` | R | real | 0. … 2. | 0.25 | `0.6` | yes | shows | R2S 3 | R2S |
| `r2_wait_sleep` | R | integer | 0 … 1 | 0 | `0` | yes | shows | R2S 3 | R2S |
| `r2_zfill` | R | on/off | — | off | `off` | yes | shows | R2S 3 | R2S |
| `r2_zfill_depth` | R | real | 0.001 … 0.5 | 0.1 | `0.5` | yes | shows | R2S 3 | R2S |

#### Sound

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `snd_volume_eff` | E | real | 0. … 1. | 1. | `1.` | yes | shows | OS 7.2, SPOS | OS SPOS |
| `snd_volume_music` | E | real | 0. … 1. | 0.7 | `1.` | yes | shows | OS 7.2, SPOS, SLM | OS SPOS |
| `snd_restart` | E | action | — | — | — | — | runs | restarts the sound device (OS) | OS |
| `snd_acceleration` | E | on/off | — | on | `on` | yes | shows | effect unestablished | 7 |
| `snd_efx` | E | on/off | — | on | `on` | yes | shows | SR | SR |
| `snd_targets` | E | integer | 4 … 32 | 16 | `24` | yes | shows | SPOS | SPOS |
| `snd_cache_size` | E | integer | 4 … 32 | 16 | `16` | yes | shows | OS | OS |

#### Mouse and camera

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `mouse_invert` | E | on/off | — | off | `off` | yes | shows | PM 8.1 | PM |
| `mouse_sens` | E | real | 0.05 … 0.6 | 1. | `0.12` | yes | shows | PM 8.1; built-in value: see 8. **Disputed in review:** the built-in value is 0.12, not 1.: the registration code sets the variable to 0.12 before registering the command, which matches OS 7.2 and PM 1.2 | PM |
| `cam_inert` | E | real | 0. … 1. | 0.7 | `0.` | yes | shows | VP 1.1 | VP |
| `cam_slide_inert` | E | real | 0. … 1. | 0.25 | `0.25` | yes | shows | effect unestablished | 7 |

#### Gameplay and HUD

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `g_game_difficulty` | G | list | `gd_novice` 0, `gd_stalker` 1, `gd_veteran` 2, `gd_master` 3 | `gd_master` | `gd_novice` | yes | shows | OS 7.2, PH; help text `game difficulty`; outside single player prints `For this game type difficulty level is disabled.` | OS PH |
| `g_always_run` | G | on/off | — | on | `on` | yes | shows | PM 4.1 | PM |
| `g_backrun` | G | on/off | — | off | `on` | yes | shows | PM 1.2 (no effect on movement found) | PM |
| `g_autopickup` | G | on/off | — | off | `on` | yes | shows | effect unestablished | 7 |
| `g_corpsenum` | G | integer | 0 … 100 | 10 | `10` | yes | shows | effect unestablished | 7 |
| `g_eventdelay` | G | integer | 0 … 1000 | 0 | `0` | yes | shows | effect unestablished | 7 |
| `cl_cod_pickup_mode` | G | integer | 0 … 1 | 1 | `1` | yes | shows | effect unestablished | 7 |
| `hud_weapon` | G | on/off | — | on | `on` | yes | shows | OS 6.5 | OS |
| `hud_info` | G | on/off | — | on | `on` | yes | shows | OS 6.5 | OS |
| `hud_crosshair` | G | on/off | — | on | `on` | yes | shows | OS 6.5 | OS |
| `hud_crosshair_dist` | G | on/off | — | off | `off` | yes | shows | OS 6.5 | OS |
| `cl_dynamiccrosshair` | G | on/off | — | on | `on` | yes | shows | OS 6.5 | OS |
| `ai_use_torch_dynamic_lights` | G | on/off | — | on | `on` | yes | shows | TO | TO |
| `ph_frequency` | G | custom | — | `100.00000` | `100.00000` | yes | shows | custom: a number, clamped to 50 … 200 (never rejected); the physics step becomes 1 / value; status `%3.5f` of 1 / step, not trimmed (CF 6.1) | CF |
| `ph_iterations` | G | integer | 15 … 50 | 18 | `38` | yes | shows | CF 6.1, R1S | CF R1S |

#### Camera tracks

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `demo_play` | G | action | — | — | — | — | runs | ST 4.2 | ST |
| `demo_record` | G | action | — | — | — | — | shows | ST 4.3 | ST |
| `demo_record_take_point` | E | action | — | — | — | — | runs | ST 4.3; registered only with `-designer` | ST |
| `demo_record_clear_points` | E | action | — | — | — | — | runs | ST 4.3; registered only with `-designer` | ST |
| `demo_record_use_points` | E | integer | 0 … 1 | 0 | — | no | shows | ST 4.3; registered only with `-designer` | ST |
| `demo_record_4step` | E | integer | 0 … 1 | 0 | — | no | shows | ST 4.3; registered only with `-designer` | ST |
| `demo_record_step` | E | integer | 0 … 3 | 0 | — | no | shows | ST 4.3; registered only with `-designer` | ST |

#### Diagnostics

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `stat_memory` | G | action | — | — | — | — | runs | prints memory statistics lines starting `* [ D3D ]: textures[`, `* [x-ray]: crt heap[`, `* [x-ray]: economy: strings[`; the engine runs it once at startup | 6 |
| `dump_resources` | E | action | — | — | — | — | runs | writes the renderer resource list to the log (contents unestablished) | 6 |
| `dump_open_files` | E | action | — | — | — | — | shows | `dump_open_files <n>`: writes the open-file list to the log (contents unestablished) | 6 |
| `net_dbg_dump_export_obj` | E | integer | 0 … 1 | 0 | `0` | yes | shows | network debugging (multiplayer) | 9c |
| `net_dbg_dump_import_obj` | E | integer | 0 … 1 | 0 | `0` | yes | shows | network debugging (multiplayer) | 9c |
| `net_dbg_dump_update_write` | G | integer | 0 … 1 | 0 | `0` | yes | shows | network debugging (multiplayer) | 9c |
| `net_dbg_dump_update_read` | G | integer | 0 … 1 | 0 | `0` | yes | shows | network debugging (multiplayer) | 9c |

#### Multiplayer server settings

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `sv_remove_weapon` | G | integer | −1 … 1 | 1 | `1` | yes | shows | believed multiplayer | 9a |
| `sv_remove_corpse` | G | integer | −1 … 1 | 1 | `1` | yes | shows | believed multiplayer | 9a |
| `sv_statistic_collect` | G | integer | 0 … 1 | 0 | `1` | yes | shows | believed multiplayer | 9a |
| `sv_no_auth_check` | G | integer | 0 … 1 | 0 | — | no | shows | believed multiplayer | 9a |
| `sv_artefact_spawn_force` | G | integer | 0 … 1 | 0 | `0` | yes | shows | believed multiplayer | 9a |
| `sv_hail_to_winner_time` | G | integer | 0 … 60 | 7 | `7` | yes | shows | believed multiplayer | 9a |
| `sv_client_reconnect_time` | G | integer | 0 … 60 | 0 | `60` | yes | shows | believed multiplayer | 9a |
| `sv_rpoint_freeze_time` | G | integer | 0 … 60000 | 0 | `0` | yes | shows | believed multiplayer | 9a |
| `sv_vote_enabled` | G | integer | 0 … 255 | 255 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_spectr_freefly` | G | integer | 0 … 1 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_spectr_firsteye` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_spectr_lookat` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_spectr_freelook` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_spectr_teamcamera` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_vote_participants` | G | integer | 0 … 1 | 0 | `0` | yes | shows | believed multiplayer | 9a |
| `sv_vote_quota` | G | real | 0. … 1. | 0.51 | `0.51` | yes | shows | believed multiplayer | 9a |
| `sv_vote_time` | G | real | 0.5 … 10. | 1. | `1.` | yes | shows | believed multiplayer | 9a |
| `sv_forcerespawn` | G | integer | 0 … 3600 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_fraglimit` | G | integer | 0 … 100 | 10 | `10` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_timelimit` | G | integer | 0 … 180 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_dmgblockindicator` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_dmgblocktime` | G | integer | 0 … 600 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_anomalies_enabled` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_anomalies_length` | G | integer | 0 … 180 | 3 | `3` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_pda_hunt` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_warm_up` | G | integer | 0 … 3600 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_max_ping_limit` | G | integer | 1 … 2000 | 2000 | `2000` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_auto_team_balance` | G | integer | 0 … 1 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_auto_team_swap` | G | integer | 0 … 1 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_friendly_indicators` | G | integer | 0 … 1 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_friendly_names` | G | integer | 0 … 1 | 0 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_friendlyfire` | G | real | 0. … 2. | 1. | `1.` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_teamkill_limit` | G | integer | 0 … 100 | 3 | `3` | yes | shows | believed multiplayer | 9a |
| `sv_teamkill_punish` | G | integer | 0 … 1 | 1 | `1` | yes | shows | believed multiplayer | 9a |
| `sv_artefact_respawn_delta` | G | integer | 0 … 600 | 30 | `30` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_artefacts_count` | G | integer | 1 … 100 | 10 | `10` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_artefact_stay_time` | G | integer | 0 … 180 | 3 | `3` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_reinforcement_time` | G | integer | −1 … 3600 | 20 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9a |
| `sv_bearercantsprint` | G | integer | 0 … 1 | 1 | `1` | yes | shows | believed multiplayer | 9a |
| `sv_shieldedbases` | G | integer | 0 … 1 | 1 | `1` | yes | shows | believed multiplayer | 9a |
| `sv_returnplayers` | G | integer | 0 … 1 | 1 | `1` | yes | shows | believed multiplayer | 9a |
| `sv_dump_online_statistics_period` | G | integer | 0 … 60 | 0 | `0` | yes | shows | believed multiplayer | 9a |

#### Multiplayer commands

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `sv_kick` | G | action | — | — | — | — | shows | help text `Kick Player by name` | 9b |
| `sv_banplayer` | G | action | — | — | — | — | shows | help text `Ban Player by Name` | 9b |
| `sv_banplayer_ip` | G | action | — | — | — | — | shows | help text `Ban Player by IP` | 9b |
| `sv_unbanplayer_ip` | G | action | — | — | — | — | shows | help text `UnBan Player by IP` | 9b |
| `sv_listplayers` | G | action | — | — | — | — | runs | help text `List Players` | 9b |
| `sv_listplayers_banned` | G | action | — | — | — | — | runs | help text `List of Banned Players` | 9b |
| `sv_changegametype` | G | action | — | — | — | — | shows | help text `Changing Game Type` | 9b |
| `sv_changelevel` | G | action | — | — | — | — | shows | help text `Changing Game Type` | 9b |
| `sv_changelevelgametype` | G | action | — | — | — | — | shows | help text `Changing level and game type` | 9b |
| `sv_addmap` | G | action | — | — | — | — | shows | help text `Adds map to map rotation list` | 9b |
| `sv_listmaps` | G | action | — | — | — | — | runs | help text `List maps in map rotation list` | 9b |
| `sv_nextmap` | G | action | — | — | — | — | runs | help text `Switch to Next Map in map rotation list` | 9b |
| `sv_prevmap` | G | action | — | — | — | — | runs | help text `Switch to Previous Map in map rotation list` | 9b |
| `sv_nextanomalyset` | G | action | — | — | — | — | shows | help text `Activating pointed Anomaly set` | 9b |
| `cl_votestart` | G | action | — | — | — | — | shows | help text `Starts Voting` | 9b |
| `sv_votestop` | G | action | — | — | — | — | runs | help text `Stops Current Voting` | 9b |
| `cl_voteyes` | G | action | — | — | — | — | runs | help text `Vote Yes` | 9b |
| `cl_voteno` | G | action | — | — | — | — | runs | help text `Vote No` | 9b |
| `sv_setenvtime` | G | action | — | — | — | — | shows | believed multiplayer | 9b |
| `sv_setweather` | G | action | — | — | — | — | shows | help text `Set new weather` | 9b |
| `sv_statistic_save` | G | action | — | — | — | — | runs | help text `saving statistic data` | 9b |
| `sv_return_to_base` | G | action | — | — | — | — | shows | believed multiplayer | 9b |
| `get_server_address` | G | action | — | — | — | — | runs | help text `List Players` | 9b |
| `sv_startteammoney` | G | action | — | — | — | — | runs | help text `Set Start Team Money` | 9b |
| `g_swapteams` | G | action | — | — | — | — | runs | help text `swap teams for artefacthunt game` | 9b |
| `ra` | G | action | — | — | — | no | shows | believed multiplayer | 9b |
| `sv_status` | G | action | — | — | — | — | runs | help text `Shows current server settings` | 9b |
| `chat` | G | action | — | — | — | — | shows | believed multiplayer | 9b |
| `sv_dump_online_statistics` | G | action | — | — | — | — | runs | help text `Shows current server settings` | 9b |

#### Multiplayer menu, network and identity settings

| Command | Lib | Kind | Range or values | Built-in | `user.ltx` | Saved | Bare name | Effect | Spec |
|---|---|---|---|---|---|---|---|---|---|
| `net_dedicated_sleep` | E | integer | 0 … 64 | 5 | `5` | yes | shows | believed multiplayer | 9c |
| `sv_console_update_rate` | E | integer | 1 … 100 | 1 | `1` | yes | shows | believed multiplayer | 9c |
| `sv_dedicated_server_update_rate` | E | integer | 1 … 1000 | 100 | `100` | yes | shows | believed multiplayer | 9c |
| `net_cl_interpolation` | G | real | −1. … 1. | 0. | `0.` | yes | shows | believed multiplayer | 9c |
| `net_cl_icurvetype` | G | integer | 0 … 2 | 0 | `0` | yes | shows | believed multiplayer | 9c |
| `net_cl_icurvesize` | G | integer | 0 … 2000 | 0 | `0` | yes | shows | believed multiplayer | 9c |
| `net_cl_resync` | G | action | — | — | — | — | runs | help text `resyncronize client` | 9c |
| `net_cl_clearstats` | G | action | — | — | — | — | runs | help text `clear client net statistic` | 9c |
| `net_sv_clearstats` | G | action | — | — | — | — | runs | help text `clear server net statistic` | 9c |
| `net_sv_update_rate` | G | integer | 1 … 100 | 30 | `30` | yes | shows | believed multiplayer | 9c |
| `net_sv_pending_lim` | G | integer | 0 … 10 | 2 | `3` | yes | shows | believed multiplayer | 9c |
| `net_sv_gpmode` | G | integer | 0 … 2 | 0 | `0` | yes | shows | believed multiplayer | 9c |
| `net_sv_log_data` | G | on/off | — | off | `off` | yes | shows | believed multiplayer | 9c |
| `net_cl_log_data` | G | on/off | — | off | `off` | yes | shows | believed multiplayer | 9c |
| `cdkey` | G | text | at most 63 characters | unestablished | — | no | shows | believed multiplayer | 9c |
| `name` | G | custom | — | — | — | no | shows | help text `player name` | 9c |
| `beserver` | G | action | — | — | — | — | runs | help text `BattlEye Server commands` | 9c |
| `beclient` | G | action | — | — | — | — | runs | help text `BattlEye Client commands` | 9c |
| `bemsg` | G | integer | 0 … 1 | 1 | `1` | yes | shows | believed multiplayer | 9c |
| `net_compressor_status` | G | action | — | — | — | — | shows | help text `valid arguments is [info info_full on off]` | 9c |
| `net_compressor_enabled` | G | integer | 0 … 1 | 0 | `0` | yes | shows | believed multiplayer | 9c |
| `net_compressor_gather_stats` | G | integer | 0 … 1 | 0 | `0` | yes | shows | believed multiplayer | 9c |
| `mm_net_player_name` | G | text | at most 63 characters | set at startup (unestablished) | — | no | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_srv_maxplayers` | G | integer | 2 … 32 | 32 | `32` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_srv_gamemode` | G | list | `st_deathmatch` 2, `st_team_deathmatch` 6, `st_artefacthunt` 7 | `st_deathmatch` | `st_deathmatch` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_mm_net_srv_dedicated` | G | on/off | — | off | `off` | yes | shows | believed multiplayer | 9c |
| `mm_net_con_publicserver` | G | on/off | — | off | `off` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_con_spectator_on` | G | on/off | — | off | `off` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_use_battleye` | G | on/off | — | off | `off` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_con_spectator` | G | integer | 1 … 32 | 20 | `20` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_srv_reinforcement_type` | G | integer | 0 … 2 | 1 | `0` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_weather_rateofchange` | G | real | 0. … 100. | 1. | `1.` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_srv_name` | G | text | at most 63 characters | set at startup (unestablished) | `WINNY` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_filter_empty` | G | on/off | — | on | `on` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_filter_full` | G | on/off | — | on | `on` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_filter_pass` | G | on/off | — | on | `on` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_filter_wo_pass` | G | on/off | — | on | `on` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_filter_wo_ff` | G | on/off | — | on | `on` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_filter_listen` | G | on/off | — | on | `on` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |
| `mm_net_filter_battleye` | G | on/off | — | on | `on` | yes | shows | a control of the multiplayer menus (`entry`) | 9c |

## 7. Log output

### 7.1 Lines

- The engine, the game library, both renderers and the sound library write to one log, kept by the
  core library; the console shows that log (section 3) [CS-15].
- A written text is split at line feeds: each piece before a line feed is one line (an empty piece is
  stored as a single space), the text after the last line feed is one more line [CS-15]. Formatted
  messages are cut to 1023 characters before splitting; a formatted message of length 0 writes nothing
  [CS-15].
- A message with an extra word is written as `<text> <word>` (this is how `~ <line>` and
  `! Unknown command: <name>` are made) [CS-15].
- Lines are never dropped while the program runs: there is no line limit (**believed**: the store is a
  growing list and only `clear_log` empties it) [CS-15].
- The first character is a convention, not a type: lines beginning `!` (errors), `~` (entered lines and
  syntax messages), `-` (listings and status), `*` (information) and `#` get the colours of section 3;
  any program text may use them [CS-11, CS-15].

### 7.2 What appears in the console

Everything written to the log since it was created, in order; the console shows
the newest lines that fit (3) and scrolls back with Page Up (4.5) [CS-11, CS-13]. Typed lines appear as
`~ <line>`; replies of commands as their own lines (section 6) [CS-5].

### 7.3 The log file

- `flush` writes the whole log to the log file, every line followed by a line end of two bytes
  (**believed** CR LF; the bytes are not read here), then logs `* Log file has been saved successfully!`
  [CS-15]. The file's name and folder are those of ST 2.3 (`$logs$\xray_<user>.log`).
- `clear_log` empties the log, then writes the (empty) file, then logs
  `* Log file has been cleaned successfully!` [CS-15].
- When else the file is written (ST 2.3: at exit, believed) is **unestablished** here (spec 4).

## 8. Conflicts with approved specifications

- `mouse_sens`: the value stored in the engine is 1.0, the registration code does not change it, and no
  code writes it except the command; the game library only reads it [CS-17]. OS 7.2 and PM 1.2 give the
  built-in value 0.12, which is the shipped `user.ltx` value. Spec 8 corrects them.
  **Disputed in review:** there is no conflict. The engine's registration code sets the variable to
  0.12 before registering `mouse_sens` (that is one of the two engine references, a write, not a
  read), so the built-in value is 0.12 and OS 7.2 and PM 1.2 are right; the stored 1.0 is replaced
  before any command can read it.

## 9. States where the console is unavailable

- **Startup splash** (startup section 5): the console is not drawn; the device does not exist yet
  (**believed**) [CS-8].
- **Logo sequence:** the game library closes the console when the logo sequence starts; during it every
  key except the quit binding is ignored, so the console cannot be opened (startup 6.5) [CS-8].
- **Level loading:** the console is closed when loading starts (2.2); whether it can be opened during the
  loading screen is **unestablished** (spec 2).
- **No level and no active menu:** Esc and key 41 do not close the console (2.2). This happens when the
  console is open with no level loaded and the menu inactive, e.g. after `main_menu off` is entered in the
  title menu: the console then stays until a command opens the menu or starts a level [CS-9].
- **Camera-recording mode** (`demo_record`): the console's own binding is not used; only key 41 opens it
  (2.1) [CS-8].
- **Dedicated server:** a separate text window exists for it (**believed** to replace the console
  there); on a dedicated server closing has no effect and the console always uses the full screen
  height [CS-9, CS-11]. The dedicated-server
  program is outside this scope (**believed** not part of the single-player game).
- No shipped command is disabled (5.1), so no command is unavailable by itself [CS-1]; but many
  commands do nothing outside their context and say so (e.g. `! Only for multiplayer games!`,
  `For this game type Demo Play is disabled.`, `for single-mode only`; section 6) [CS-4].

## 10. Commands named by the game data

- Every command line of the shipped `user.ltx` (239 lines) names a registered command [CS-16].
- Scripts execute `check_for_updates`, `disconnect`, `flush`, `g_game_difficulty`, `load`,
  `load_last_save`, `main_menu`, `quit`, `save`, `start` and `unbindall`, all registered, and
  `jump_to_level` (in `level_tasks.script`), which no program registers: executing it only logs
  `! Unknown command: jump_to_level` [CS-16]. Whether that script line runs is **unestablished**.
- The menu layout files bind 75 controls to commands by name (`entry`); two of the names, `snd_freq` and
  `mm_net_battleye_auto_update`, are registered by no program [CS-16]. (OS covers `snd_freq`.)
- No code refers directly to the variables of the `mm_net_` commands [CS-20]; they are read and set by
  name through the console (**believed**; most are controls of the multiplayer menus, column *Effect*).

## 11. Proposed specifications

In order; each is one behaviour; size S (a few rules) or M (a screen's worth).

| # | Name | Covers | Size | After |
|---|---|---|---|---|
| 1 | `console-commands` | line processing, lookup, the case rule, the bare-name rule, every kind's parsing, status, help text and rejection, `help`, the saving rule (5); confirms the built-in values of section 6 | M | — |
| 2 | `console-window` | opening and closing triggers and conditions, input capture and what the receivers below do on losing input, the look (area, font, prompt, cursor, colours, rows, long-line breaking, drawing order), the states of section 9 | M | 1 |
| 3 | `console-input` | the key table, editing, key repeat, clipboard, history, completion, scrolling (4) | S | 1 |
| 4 | `console-log` | log lines (7.1), what the console shows (7.2), the log file: name, line ends, when it is written; `flush`, `clear_log` | S | 2 |
| 5 | `console-key-bindings` | `bind_console`, `unbind_console` (what a bound key does and where), `list_actions`, `bind_list` | S | 1 |
| 6 | `console-diagnostics` | `stat_memory`, `dump_resources`, `dump_open_files` output (`rs_stats` stays with R1S 2.11) | S | 4 |
| 7 | `misc-settings` | effects of `g_autopickup`, `g_corpsenum`, `g_eventdelay`, `cl_cod_pickup_mode`, `cam_slide_inert`, `snd_acceleration`, `check_for_updates` in single player, and `r2em` (not covered by R2S 3) | M | 1 |
| 8 | fix of OS 7.2 and PM 1.2 | the `mouse_sens` built-in value (8). **Disputed in review:** not needed; OS 7.2 and PM 1.2 already give the correct built-in value 0.12 (see 8) | S | — |
| 9a | `mp-server-settings` | the `sv_` variables | M | 1 |
| 9b | `mp-commands` | the multiplayer actions, `g_restart`, `g_restart_fast`, `g_kill` | M | 9a |
| 9c | `mp-menu-network-settings` | the `mm_`, `net_` and identity commands (`cdkey`, `name`, `be*`) | M | 9a |

Effects already specified by approved specifications (column *Spec* of section 6) need no new spec.

## 12. Unestablished

- Which code, if any, changes a built-in value after registration and before the configuration file
  (all *Built-in* values, section 6); spec 1. A headless run that executes `help` from a configuration
  file holding nothing else would show every built-in status.
- The startup values of `mm_net_player_name`, `mm_net_srv_name` and `cdkey` (6).
- What the level and the menus do when the console takes input from them (2.3).
- The break positions of long lines (3); the drawing order relative to menus and HUD (3, believed).
- Whether the console can be opened during the loading screen (9).
- The two line-end bytes of the log file and when else it is written (7.3).
- What pressing a key bound with `bind_console` does (6, spec 5).
- The effects marked unestablished in section 6.

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archives `gamedata.db0` … `gamedata.dbd` extracted with the project's archive tool
  (`config\fonts.ltx`, `config\ui\*.xml`, `scripts\*.script`); the shipped `__support\userdocs\user.ltx`.
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only) of `XR_3DA.exe` (console
  window, input handling, drawing, command registration and execution, the command kinds, startup and
  level events), `xrGame.dll` (command registration and handlers, the console action, the menu's key
  handling), `xrRender_R1.dll` and `xrRender_R2.dll` (command registration and handlers) and `xrCore.dll`
  (the log); export tables, import tables, strings and data values read directly from the files.
  No execution or debugging.
- Approved specifications of the repository read for coverage (CS-3).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The shipped binding of the action `console`: both `config\default_controls.ltx` (used by `unbindall`, OS 11.5) and the shipped `user.ltx` bind it to `kGRAVE` (41) as the primary key, with no secondary key; section 2.1 names only "a key bound to `console`".
