# Console window

How the original's in-game console opens and closes, what it does to the input and the game below it,
how it is drawn, and in which states it cannot be used. Typing, editing, history, completion and
scrolling are `console-input` (console-scope section 11, item 3); the log's lines and file are
`console-log` (item 4); how an entered line becomes a command is `docs/behavior/console-commands.md`.
What could not be established is marked **unestablished**; a statement inferred without a direct check is
marked **believed**, with why. Nothing here was observed on screen: every fact comes from the shipped
binaries, the game data and approved specifications (Provenance).

Conventions:

- "The engine" is `bin\XR_3DA.exe`, "the game library" `bin\xrGame.dll`, "the renderers"
  `bin\xrRender_R1.dll` and `bin\xrRender_R2.dll`, all of the shipped Russian edition (GOG build).
- **Key codes** are the keyboard scan codes of OS section 11.2 (`kESCAPE` 1, `kGRAVE` 41, the Shift
  keys 42 and 54). An **action** and its primary and secondary key are those of OS section 11.1; a key
  "bound to" an action is its primary or secondary key.
- A **receiver** is a part of the program that takes keyboard and mouse input. Input always goes to the
  receiver that took it last (the **top** receiver); when that one gives input back, the receiver below it
  has input again.
- Positions are in the **normalised screen space** of console-scope: *x* from −1 (left edge) to +1
  (right edge), *y* from −1 (top) to +1 (bottom); a point (*x*, *y*) is drawn at pixel
  (⌊(*x* + 1) · *W* / 2⌋, ⌊(*y* + 1) · *H* / 2⌋) of a *W* × *H* screen. *y*₀ is 1 at full height and 0 at
  half height (4.1).
- Specifications referred to (`docs/behavior/` unless noted): CS `console-scope.md`, CC
  `console-commands.md`, OS `options-screens.md`, TM `title-menu.md`, ST `steering-the-original.md`,
  SPIC `docs/formats/save-picture.md`, TF `docs/formats/text-and-fonts.md`, R1S `r1-settings-scope.md`,
  startup `startup.md`.

**Checks.** Every claim carries a tag [CW-*n*]. The checks are one script in the dirty workspace,
`conwin/checks/run.py`; it reads the shipped binaries (bytes, export and import tables, data values),
read-only static analysis of them, the extracted game data (`config\fonts.ltx`, `config\ui\*.xml`,
`scripts\*.script`, `textures\ui\ui_font_console_02.ini`) and, where an approved specification is cited
for a fact, its text. All checks pass.

## 1. Opening

| Trigger | Condition | Spec |
|---|---|---|
| The program starts | opened when the application object is created, after the video device is created and before the main loop starts [CW-3]; the game then closes it when the logo sequence starts, or, with `-nointro`, executes `main_menu on`, which closes it (section 2) [CW-3]; it is not visible at startup (startup 5) [CW-16] | startup, TM |
| A key bound to `console` is pressed while the level has input | always: the action is handled before the pause and the game's windows (inventory, PDA, dialogue, …) are looked at [CW-3] | — |
| A key bound to `console` is pressed while the main menu has input | while the menu is active; handled before F12 (the menu's screenshot key) and before the menu's windows get the key [CW-3] | TM, OS |
| Key 41 is pressed in the camera-recording mode of `demo_record` | the key itself, not the binding [CW-3] | ST 4.3 |
| A level is unloaded (`disconnect`, the end of a session, `quit` with a level) | opened after the level is destroyed; then, unless a quit or a new level start is pending, `main_menu off` and `main_menu on` are executed: the menu opens, remembers the console as open and closes it (section 2) [CW-3, CW-4] | — |
| The main menu closes | opened if it was open when the menu opened [CW-4] | TM 2 |
| A save-picture capture asked for by the menu ends | opened if it was open when the capture was asked for [CW-3] | SPIC 3 |

Nothing else opens the console: these are all the places in the engine and the game library that do,
and the renderers never open or close it [CW-1]. Scripts could (the console object offers `show` and
`hide` to scripts), but no shipped script calls either [CW-1].

**What opening does** [CW-2]. Opening an open console does nothing. Otherwise the input line is emptied,
the key-repeat timing of `console-input` starts afresh, the console takes input (section 3) and its
per-frame update (cursor blink, key repeat) and drawing (section 4) start. The cursor phase is not reset:
it continues from where it was when the console last closed [CW-7].

## 2. Closing

| Trigger | Condition | Spec |
|---|---|---|
| Esc (1) pressed in the console | a press, not a key repeat; only while a level object exists or the main menu is active, otherwise the key does nothing (section 5) [CW-5] | — |
| Key 41 pressed in the console without Shift | the same as Esc, with the same condition; with Shift it types `~` (`console-input`) [CW-5] | — |
| The main menu opens | whether the console was open is remembered (section 1), then it is closed [CW-4] | TM 2 |
| A level starts (`start`) | `main_menu off` is executed, then the console is closed, before loading begins [CW-3] | — |
| `quit` | closed; then the level, if one exists, is unloaded and the program ends [CW-1]. The unload opens the console again with the quit pending (section 1, no menu), so with a level the console is open again until the program ends [CW-3] | ST 7 |
| `demo_play`, `demo_record` | closed when the command starts its track [CW-3] | ST 4.2, 4.3 |
| The menu asks for a save picture | closed for the capture (section 1) [CW-3] | SPIC 3 |
| The program ends | closed [CW-3] | — |

When the main menu closes while the console is open (e.g. `main_menu off` entered in the console over
the menu), the console is closed and at once opened again around the menu giving up input, so it stays
open and stays the top receiver [CW-4].

Only Esc and key 41 close the console from the keyboard; another key bound to `console` does nothing
while it is open (keys reach the console, not the bindings) [CW-5, CW-6].

**What closing does** [CW-2]. Closing a closed console does nothing; on a dedicated server (section 5)
closing never has an effect. Otherwise the console gives input back (section 3), its update and drawing
stop, and the input line is kept (opening empties it).

## 3. Input

### 3.1 Which parts take input

The receivers are: a default receiver set up at startup, the level (when it has loaded), the camera
recorder of `demo_record`, the console, the main menu and the sequences of
`config\ui\game_tutorials.xml` (which includes `ui_movies.xml`: the logo sequence `intro_logo`, the New
Game intro `intro_game`, tutorials) [CW-6, CW-15]. The main menu takes input in its own per-frame update
after it opens, not at once [CW-4]. The game's windows inside a level (inventory, PDA, dialogue, …) do not
take input themselves (**believed**: they get keys through the level, since no other part takes input)
[CW-6].

### 3.2 The console takes input

When the console opens it becomes the top receiver. First, input not yet delivered is delivered to the
receiver that had input; then that receiver is told it lost input (3.3); then the console has input; the
stored mouse movement is cleared [CW-6].

While the console is the top receiver, every key press, hold and release goes to it and to nothing else
[CW-6]. The console reacts to keys only: mouse movement, buttons and the wheel reach it and are ignored,
so they are lost [CW-6].

### 3.3 The receiver below loses input

The level, the main menu and the sequences handle losing input alike: each receives a release for every
key held at that moment and for every held mouse button (left, right, middle), and a "mouse stopped" for
both axes [CW-6]. Held movement, firing and the like therefore end as if the keys had been released
(what a release does is the receiver's own behaviour, not this specification's).

### 3.4 The receiver below gets input back

When the console closes while it is the top receiver, it is told it lost input (it receives the releases
of 3.3, which also clears its Shift state), it is removed, and the receiver below has input again [CW-6]:

- **The level** (and a sequence) then receives a press for every key that is still held and bound to one
  of the actions `left`, `right`, `up`, `down`, `crouch`, `accel`, `forward`, `back`, `lstrafe`,
  `rstrafe`, `llookout`, `rlookout` or `wpn_fire`: what those keys do resumes when they are still held.
  Other held keys act only when pressed again [CW-6].
- **The main menu** receives nothing [CW-6].

If the console is not the top receiver when it closes (another receiver took input after it), it is
removed from below without any receiver being told [CW-6].

**Shift.** The console counts Shift as held only after a Shift key is pressed while the console has
input, and stops at its release [CW-5]; a Shift key held from before opening has no effect until pressed
again. **Disputed in review:** a repeat of a Shift key also turns the Shift state on (console-input
sections 2 and 5): a Shift key held from before opening receives hold reports, so it takes effect as soon
as a repeat comes due for it (0.2 s after opening, or at once if a repeat was still due, when it is the
held key with the lowest code), not only when pressed again.

### 3.5 No pause

Opening or closing the console does not pause the game or change the pause [CW-7]; a level keeps
running below it. The console's own clock (cursor blink, key repeat) is the engine's unpaused
millisecond clock, so the cursor blinks while the game is paused too [CW-7].

## 4. Look

*W* × *H* is the screen size in pixels.

### 4.1 Area

Each frame the console clears one rectangle of the colour buffer to opaque (32, 32, 32) (alpha 255):
from the top-left corner, *W* wide and *H* high (**full height**), or ⌊*H* / 2⌋ high (**half height**)
while a level is loaded or the main menu is active [CW-8]. "Loaded" is set at the end of level loading
and cleared when the level stops [CW-8]. The rectangle is cleared, not blended: nothing below shows
through it; the rest of the screen shows the game or menu as drawn [CW-8].

### 4.2 Font

The `fonts.ltx` section `hud_font_di`: texture `ui\ui_font_console_02` and no `size`, `interval` or
resolution variants, so the same texture at every resolution (TF 2.3) [CW-9]. It is created on the first
draw with only the *screen-relative size* flag, not *gradient*, so every glyph has one uniform colour
(TF 3.4) [CW-9]. Its metric file has a `[symbol_coords]` section with height 19 [CW-9].

- **Height.** The console sets the glyph height every frame to *H* × 0.025 pixels (19.2 at *H* = 768),
  whatever the metric height; glyphs are stretched vertically to it (TF 3.3) [CW-9].
- **Width.** A text's width is the sum of its glyph widths from the metric file (interval 1), in pixels at
  every resolution, not scaled by the height (TF 3.1) [CW-9]: the space and `a` are 9 pixels, the
  printable ASCII glyphs 4 to 17 [CW-9].

The look does not depend on the game data beyond the font: no layout or configuration file is read
[CW-9].

### 4.3 Prompt line

The text `>>> `, then the input line, then `|` while the cursor is in its visible phase; colour
(128, 128, 255), alpha 255; drawn at *x* = −1, *y* = *y*₀ − 0.05 [CW-10]. The cursor is always at the
end of the line (`console-input`). The prompt is never broken (4.6): a prompt wider than the screen runs
past the right edge [CW-10]. A text of 512 bytes or more is not drawn at all (TF 3.1) [CW-10], so the
prompt row disappears when the prompt text reaches 512 characters: with 508 or more characters in the
input line it is never drawn; with 507 it is drawn only while the cursor is in its hidden phase.

**Cursor blink.** The phase toggles each time 0.1 s of the console's clock has accumulated (frame time
per frame clamped to 0.06666 s) [CW-7].

### 4.4 Log rows

Above the prompt the log's lines are drawn one per row, from the newest (minus the scroll offset of
`console-input`) backwards: the newest at *y* = *y*₀ − 0.15 (one empty row above the prompt), each older
one 0.05 higher, while the row's *y* is not above −1 [CW-11]. The positions are computed in single
precision, so the row that would lie exactly at −1 lies just above it and is not drawn [CW-11]:

| Height | Rows | Their *y* |
|---|---|---|
| full | 37 | 0.85, 0.80, …, −0.95 |
| half | 17 | −0.15, −0.20, …, −0.95 |

The topmost row therefore starts one glyph height (*H* × 0.025) below the top edge [CW-11]. A line broken
over several rows (4.6) takes that many rows; its upper rows are drawn even where they lie above the top
edge (**believed** off screen there: the breaking does not test the edge) [CW-12].

### 4.5 Line colours

From the line's first character; for the marked kinds the line is drawn from its third character
[CW-11]:

| First character | Colour (R, G, B, alpha) | Drawn from |
|---|---|---|
| `!` | (255, 0, 0, 255) | 3rd character |
| `#` | (0, 222, 205, 145) | 3rd character |
| `*` | (128, 128, 128, 255) | 3rd character |
| `-` | (0, 255, 0, 255) | 3rd character |
| `~` | (255, 255, 0, 255) | 3rd character |
| anything else | (255, 255, 255, 255) | 1st character |

All text is drawn after the area is cleared, over it [CW-11].

### 4.6 Long lines

Applied to the drawn text of a log line (after the two characters of 4.5 are dropped) [CW-12]:

1. If its width (4.2) is not more than 1024 pixels, it is one row.
2. Otherwise let *k* be the smallest number of leading characters whose width is more than 1024. The upper
   row is those *k* characters (so it is wider than 1024 pixels); the rest of the text, **starting with
   the *k*-th character**, is placed in the rows below it by the same rule. The *k*-th character therefore
   ends one row and starts the next.

The rows of one line are drawn top to bottom in that order, the whole line taking as many rows as the
rule makes, and the older lines move up accordingly [CW-12]. A row of visible glyphs made by the rule has
at most 257 characters (the narrowest glyph is 4 pixels), below the limit of 4.3 [CW-12].

Examples [CW-12]: a line of 200 × `a` (9 pixels each) is drawn as rows of 114 and 87 characters (201 in
all: the 114th `a` appears twice); a line of 300 × `a` as rows of 114, 114 and 74.

The breaking works in a space of 1024 characters: when the position of the breaking character counted
from 0 (*k* − 1) plus the length of the text being broken reaches 1024 — for a line of `a` from 911
characters — the program writes beyond that space; what is then drawn, or whether the program fails, is
**unestablished** [CW-12]. (Log lines can be up to 1023 characters long, CS 7.1.)

### 4.7 Drawing order

Each frame the console is drawn after everything else in the frame's drawing list — the level, the main
menu, the sequences' windows and the other text — and before the statistics display, which is drawn over
it when on (e.g. `rs_stats`, R1S 2.11) [CW-13]. No other part of the engine, the game library or the
renderers is drawn after it in that list [CW-13]. That a level's HUD is drawn as part of the level, and so
below the console, is **believed** (the HUD has no entry of its own in the list) [CW-13]. The console is
drawn the same way over the main menu and over a level.

## 5. States where the console cannot be used

- **Startup:** not visible until the title menu (startup 5) [CW-16].
- **Sequences** (`intro_logo`, `intro_game`, tutorials): while one runs it is the top receiver. A key bound
  to `quit` ends its current item; any other key press is passed to the receiver below it unless the
  current item keeps input for itself [CW-15]. The sequence never opens the console itself [CW-15], so the
  console opens during a sequence only when the press is passed to a level below (section 1). During the
  logo sequence no key opens it (startup 6.5) [CW-16]. Which items keep input is **unestablished** here.
- **Level loading:** the console is closed when loading starts (section 2). While loading steps remain,
  each pass of the main loop runs one step and draws only the loading screen: the per-frame update and the
  drawing list do not run [CW-14]. Input is read only by the per-frame update [CW-14], so during loading
  the console can be neither drawn nor opened. Whether a key pressed during loading still acts afterwards
  is **unestablished**.
- **No level and no active menu:** Esc and key 41 do not close the console (section 2) [CW-5]. This
  happens when the console is open with no level object and the menu inactive, e.g. after `main_menu off`
  is entered in the title menu: the console then stays until a command opens the menu or starts a level.
- **Camera-recording mode** (`demo_record`): the console's binding is not used; only key 41 opens it
  (section 1) [CW-3].
- **Dedicated server:** closing has no effect and the console always uses the full height [CW-17]; while
  a level loads it is drawn instead of the loading picture [CW-14]. The dedicated server is outside this
  specification's use (**believed** not part of the single-player game).
- No command is disabled (CC), so no command is unavailable by itself.

## 6. Unestablished

- What is drawn, or whether the program fails, when a long line exceeds the breaking's working space
  (4.6).
- Which sequence items keep input (section 5).
- Whether a key pressed during level loading acts after it (section 5).
- Whether a key bound to `console` opens the console while an options screen waits for a key to bind
  (OS 11.4), i.e. whether that wait receives keys through the main menu's key handling (section 1).
- When else, besides `rs_stats`, the statistics display is drawn over the console (4.7).
- None of the look (section 4) was compared with a frame of the running game.

## Gaps found in review

- The camera recorder of `demo_record` as the receiver below the console (it is when key 41 opens it): 3.3
  and 3.4 leave it out; it handles losing input the default way (releases, as 3.3) and receives nothing on
  getting input back (as the main menu).
- Startup without `-nointro` (section 1): the logo sequence, and with it the closing, starts only under
  further conditions (among them: not a dedicated server, no level); what the console does when they are
  not met is not stated.
- A marked log line (4.5) shorter than three characters, e.g. `!` alone: drawing from its third character
  starts past the line's end; what is drawn is not covered.
- The game window losing or regaining focus while the console is open: what the console and the receiver
  below receive is not covered.
- console-input section 2 says closing does not change the Shift state; 3.4 here (closing with a Shift key
  held turns it off, through the releases of losing input) contradicts it for that case; one of the two
  needs a targeted fix.

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archives extracted with the project's archive tool (`config\fonts.ltx`, `config\ui\*.xml`,
  `scripts\*.script`, `textures\ui\ui_font_console_02.ini`).
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only) of `XR_3DA.exe` (console
  opening, closing, update and drawing; input receivers and their order; the frame's drawing list and its
  order; the main loop during loading; font text measurement and output), `xrGame.dll` (the places that
  open and close the console; the level's, main menu's and sequences' input handling; additions to the
  drawing list; the script binding of the console object) and the two renderer libraries (absence of
  console and drawing-list use); export tables, import tables, strings and data values read directly from
  the files. No execution or debugging.
- Approved specifications of the repository read and cited: those listed under Conventions.
- No leaked source code, derived projects or third-party documentation of the engine were used.
