# Console input

How text is typed into the original's console while it is open: which keys type which characters, the
editing keys, key repeat, pasting from the clipboard, the command history, name completion and scrolling
back through the log. When the console opens and closes, that it takes all keyboard input while open,
and how the input line and the log are drawn are `console-window` (`docs/behavior/console-scope.md`
section 11, spec 2); what happens to a line once it is entered is CC. What could not be established is
marked **unestablished**; a statement inferred without a direct check is marked **believed**, with why.
Facts marked **measured** come from a headless run of the shipped game (Provenance); the others from the
shipped engine.

Conventions:

- "The engine" is `bin\XR_3DA.exe` of the shipped Russian edition (GOG build); the console and its key
  handling belong to it.
- **Key codes** are the keyboard scan codes of OS section 11.2 (DirectInput codes, e.g. `kGRAVE` 41,
  `kESCAPE` 1). They name key positions, not characters.
- The **line** is the console's input line (the text after the prompt). The **log** is the list of text
  lines the console shows (CS section 7).
- A **frame** is one pass of the engine's main loop.
- Specifications referred to: CS `console-scope.md`, CC `console-commands.md`, OS `options-screens.md`
  (all `docs/behavior/`).

**Checks.** Every claim carries a tag [CI-*n*]. The checks are one script in the dirty workspace,
`cinput/checks/run.py`; it uses read-only static analysis of the shipped engine and bytes read directly
from it, and the log of one headless run of the shipped game driven with keyboard input from outside
(`cinput/run/drive.py`) and of a second such run (`cinput/run/drive608.py`, with a small clipboard
program `cinput/run/clip608.c`). All checks pass.

## 1. Key events

- While the console is open it receives every key event (CS 2.3). Each frame the input system first
  delivers the key presses and releases that arrived since the previous frame, in order (at most 64
  events are read each frame), then a **hold report** for every key that is down, in ascending key code
  (0 to 255) [CI-8].
- Whether a key is down is tracked by the input system for every key, whichever receiver saw the
  press [CI-8]. So a key that was already held when the console opened gives the console hold reports
  (and repeats, section 5) although the console never saw its press.
- **Alt+F4.** Each frame, after the key events and the hold reports, the input system itself checks
  whether F4 (62) and either Alt key (56 or 184) are down; if they are, the session ends and the program
  quits as after `quit` (CS 2.2), except that the console is not closed first [CI-16]. This is the
  input system's rule, not the console's: it applies whichever receiver has input, so also while the
  console is open [CI-16]. It fires at most once per program run [CI-16]. Only the state after the
  frame's events counts, so both keys must still be down then: an F4 press and release within one
  frame's events does not trigger it [CI-8, CI-16]. The two keys' events also reach the console, where
  they do nothing (section 3). Measured: with the console open, holding left Alt and then F4 ended the
  program without `quit` being typed [CI-17].
- Each key code has one action (sections 2 to 9). A press runs it as a **first press**; a repeat
  (section 5) runs it again as a **repeat**. The two differ only for Esc and key 41 (section 3) and for
  the repeat timing (section 5) [CI-7, CI-10].
- After the action of every key, pressed or repeated, including the keys that do nothing, a line longer
  than 1015 characters is cut to its first 1015 [CI-4]. A character typed into a line of 1015 characters
  therefore has no effect.

## 2. Characters

The console turns key codes into characters from a fixed table; the keyboard layout and the operating
system's text input play no part [CI-1]. A character is appended at the end of the line; there is no
cursor to move (the cursor drawn by `console-window` is always at the end) [CI-1]. Letters are always
lower case; Shift changes only the digits and the punctuation [CI-1]:

| Keys (codes) | Without Shift | With Shift |
|---|---|---|
| `1` … `9`, `0` (2 … 11) | `1` … `9`, `0` | `!` `@` `#` `$` `%` `^` `&` `*` `(` `)` |
| 12, 13 | `-` `=` | `_` `+` |
| 26, 27 | `[` `]` | `{` `}` |
| 39, 40 | `;` `'` | `:` `"` |
| 41 (`kGRAVE`) | as Esc (section 3) | `~` |
| 43 | `\` | `|` |
| 51, 52, 53 | `,` `.` `/` | `<` `>` `?` |
| Q … P (16 … 25) | `q` `w` `e` `r` `t` `y` `u` `i` `o` `p` | the same |
| A … L (30 … 38) | `a` `s` `d` `f` `g` `h` `j` `k` `l` | the same |
| Z … M (44 … 50) | `z` `x` `c` `v` `b` `n` `m` | the same |
| Space (57) | space | space |

No other key types a character [CI-1]. Measured: typed `Cfg` gave `cfg`, `~QW` gave `~qw`, the shifted
digits and punctuation and the unshifted punctuation gave the characters of the table [CI-15].

**Shift.** A press (or repeat) of either Shift key (42, 54) turns the Shift state on; a release of either
turns it off, so releasing one Shift key while the other is still held turns it off [CI-2]. The state
starts off and is not changed by opening or closing the console [CI-2]; since releases reach the
console only while it is open (CS 2.3), a Shift key released while the console is closed leaves the
state as it was until a Shift key is next pressed or released in the console.

## 3. Editing keys

| Key (code) | Action |
|---|---|
| Backspace (14) | removes the last character, if the line is not empty [CI-5] |
| Enter (28) | executes the line (CC section 2; a line typed and entered is written to the log as `~ <line>`), then the line is empty [CI-6] |
| Esc (1) | closes the console on a first press, under the condition of `console-window` (CS 2.2); never on a repeat [CI-10] |
| 41 (`kGRAVE`) without Shift | the same as Esc, including the first-press condition [CI-1, CI-10] |
| Tab (15) | name completion (section 8) |
| Insert (210) | pastes the clipboard (section 6) |
| Up (200), Down (208) | history (section 7) |
| Page Up (201), Page Down (209) | scrolling (section 9) |

- Every other key does nothing in the console (apart from the length cut and Alt+F4 of section 1):
  Ctrl, Alt, Caps Lock, the function keys, the whole numeric keypad including its Enter (156), Home,
  End, Left, Right, Delete and every code above 210 [CI-3]. Measured: `y`, then keypad 1, keypad Enter, F1, Home,
  Left, Delete, Ctrl, Alt and Caps Lock twice, then `x` and Enter executed the single line `yx` [CI-15].
- There is no cursor movement, no deletion other than Backspace, no selection and no copying [CI-5,
  CI-11].
- **Executing a line resets the browsing state.** Whenever any line is executed, typed or executed by
  the programs or a script (CC section 2), the scroll offset (section 9) and the history position
  (section 7) are first set back to 0 [CI-6].
- **Lines executed by the programs share the line.** A line the programs or a script execute is put
  into the same input line and leaves it empty afterwards [CI-6]; anything typed and not yet entered
  when that happens is lost. Such lines are not written to the log as `~ <line>` (CC section 2) [CI-6].

## 4. Length

The line holds at most 1015 characters after any key (section 1) [CI-4]. Pasting (section 6) and
history (section 7) can briefly produce more, which the same cut removes before the key's handling
ends [CI-4, CI-11].

## 5. Key repeat

A held key repeats its action. The console keeps an accumulated time, a rate and a "repeat due" mark
[CI-7]:

- Opening the console sets the accumulated time to 0 and the rate to 1; it leaves the "repeat due" mark
  as it is [CI-7].
- A first press of any key sets the rate to 1; after its action the accumulated time is 0 and no repeat
  is due [CI-7].
- A release of any key sets the rate to 1 and the accumulated time to 0 [CI-7].
- Each frame while the console is open, the frame time is added to the accumulated time, multiplied by
  the rate. When the accumulated time then exceeds 0.2 s, 0.2 s is taken off it, a repeat becomes due
  and the rate grows by 0.2; this happens at most once a frame [CI-7].
- When a repeat is due, the next hold report (section 1) runs that key's action as a repeat and the
  repeat is no longer due; the accumulated time is kept [CI-7]. Only one key repeats per due repeat:
  the held key with the lowest code.
- **Across closing and opening.** Closing does not clear the mark either, and while the console is
  closed its frame update does not run, so no repeat becomes due [CI-7]. The mark is cleared only by a
  key action (press or repeat) and by the hold report that uses it [CI-7]. A repeat still due when the
  console closed therefore stays due, and after the console opens again the first hold report it
  receives runs that key's action as a repeat at once (the held key with the lowest code at that time),
  unless a key press reaches the console first (presses come before hold reports, section 1) [CI-7,
  CI-8]. A console closed by its own keys (Esc, key 41, or Enter executing a line that closes it) has no
  due repeat left, as the action of the closing key clears the mark after closing [CI-4, CI-7, CI-10];
  one closed by another trigger of CS 2.2 can have one.
- **Frame time** is the real time since the console's previous frame, from a millisecond clock, capped
  at 0.06666 s [CI-7]. The clock is real time: the game's pause does not stop it and the time factor
  does not scale it [CI-9].

Results:

- With short frames, the first repeat comes 0.2 s after the press and the *n*-th interval is
  0.2 / (1 + 0.2 · (*n* − 1)) s: 0.2, 0.1667, 0.1429, 0.125 s … [CI-7]. At most one repeat happens per
  frame; below 15 frames a second the capped frame time slows repeats down.
- Every key with an action repeats it: characters, Backspace, Enter, Tab, Insert, Up, Down, Page Up and
  Page Down [CI-7]. Esc and key 41 do not close on a repeat [CI-10]; held key 41 without Shift does
  nothing, with Shift it repeats `~`.
- Every key that is down has hold reports, also keys whose action is nothing (Ctrl, Alt, …) and the
  Shift keys, so while such a key is held it takes every repeat from the keys with higher codes
  [CI-7, CI-8]. Which key repeats is the held key with the lowest code, not the last one pressed. While
  left Shift (42) is held, `\`/`|` (43), `z` … `m` (44 … 50), 51 … 53 and Space (57) do not repeat; keys
  2 … 41 do. While right Shift (54) is held, of the character keys only Space (57) does not repeat.
  Measured: a held `q` repeated; with left Shift held, a held `z` typed one `z` and a held `q` repeated
  [CI-15].
- A key held while the console opens (e.g. a movement key) starts repeating its character into the line
  0.2 s after opening, or at once if a repeat was still due (above) [CI-7, CI-8].

## 6. Clipboard

**Insert** reads the clipboard's plain-text form (the system's 8-bit text format) [CI-11]:

- If the clipboard cannot be opened, nothing happens [CI-11].
- If it opens but holds no text, the line is unchanged and the clipboard is left open: the game does
  not close it [CI-11]. Measured: after such an Insert the typed line was executed unchanged; under
  Wine another program could still open the clipboard afterwards [CI-17]. Whether under Windows the
  clipboard then stays unavailable to other programs, and until when, is **unestablished**.
- Otherwise the whole line is replaced by the clipboard text, at most its first 1023 bytes [CI-11]; in
  it every non-printable character (tab, CR, LF and the other control characters) becomes a space and
  every other character is converted to lower case [CI-11]; then the cut of section 1 leaves at most
  1015 characters [CI-4].
- Nothing is ever put on the clipboard [CI-11].
- Measured: with the clipboard holding `AbC`, tab, `x`, CR, LF, `Y~`, typing `zzz` then Insert and Enter
  executed `abc x y~` (the spaces collapsed by CC section 2) [CI-15].
- What becomes of bytes 0x80 … 0xFF (e.g. Cyrillic text in the Windows code page) is **unestablished**:
  whether they count as printable and how they are lower-cased depends on the C runtime's character
  tables and the `English` locale the engine uses for lower-casing.

## 7. History

- The history is not kept separately: its **entries** are the log's lines that begin with `~`, newest
  first; an entry's text is its log line from the third character on [CI-12]. Every typed and entered
  line is logged as `~ <line>` (section 3), so it becomes the newest entry; other log lines beginning
  with `~` are entries too, e.g. `~ Invalid syntax in call to '<name>'` and `~ Valid arguments: …`
  (CC section 3) [CI-12]. `clear_log` empties the log and so the history (CS section 6).
- The console keeps a **position** and a **last found** position, both 0 when the console is created and
  after any line is executed (section 3) [CI-6, CI-14]. Position −1 is the newest entry, −2 the one
  before it, and so on; 0 is the line being typed.
- **Up** lowers the position by 1, **Down** raises it by 1 [CI-12]. Then:
  - if an entry exists at the new position, its text replaces the line and the position becomes the
    last found;
  - otherwise the position goes back to the last found and, if the step was Down (the new position was
    above the last found), the line is emptied; after Up the line is left unchanged [CI-12].
- Results [CI-12]:
  - Up past the oldest entry keeps the oldest entry and the position.
  - Down from the line being typed (position 0) empties the line: what was typed is lost.
  - Down from the newest entry empties the line but the position stays on the newest entry, so the next
    Up gives the second newest. Measured: after Up, Down, Up the line was the second newest entry
    [CI-15].
  - With no entries, Up leaves the line as it is and Down empties it.
- The position counts from the newest entry, so a `~` line written while browsing makes the same
  position name the next older entry (**believed**: follows from the rule; not run).
- The entry's text is copied as it is (not lower-cased), then cut as in section 1 [CI-12, CI-4].
- Opening and closing the console do not change the position: closing in the middle of browsing and
  opening again continues from the same position [CI-12]. Measured: Up, Esc, key 41, Up gave the second
  newest entry [CI-15].

## 8. Completion

**Tab** completes a command name [CI-13]:

- If the line begins with `ra ` (the remote-administration command and one space), those three
  characters are kept and only the text after them is completed; otherwise the whole line is [CI-13].
- The text is replaced by the first command name, in byte order of the names (CC section 1), that is not
  less than the whole text, followed by one space. If every name is less than the text, nothing changes
  [CI-13]. Names are compared byte by byte as unsigned bytes [CI-13]. The names are those registered at
  the time (CC section 1: 251 without `designer` on the command line, 256 with it).
- It is not a prefix match, and the whole text counts, arguments included [CI-13]. Examples:

  | Line | After Tab |
  |---|---|
  | (empty) | `_preset ` |
  | `cfg` | `cfg_load ` |
  | `zz` | `zz` (unchanged) |
  | `bind q` | `bind_console ` (the argument is lost) |
  | `hud_crosshair ` | `hud_crosshair_dist ` |
  | `ra cfg` | `ra cfg_load ` |

  The last but one row is a second Tab after `hud_crosshair`: as the added space sorts after the name,
  pressing Tab again steps on to the next name in order [CI-13]. Measured: `Cfg` + Tab, `zz` + Tab,
  `ra cfg` + Tab, Tab on an empty line and `hud_crosshair` + Tab + Tab executed `cfg_load`, `zz`,
  `ra cfg_load`, `_preset` and `hud_crosshair_dist` [CI-15].
- There are no hints, no list of matches and no completion of arguments [CI-13].

## 9. Scrolling

- The console keeps a **scroll offset**: the number of log lines the shown part of the log is moved back
  from the newest (how the lines are drawn from it is `console-window`) [CI-14].
- **Page Up** raises it by 1, to at most the number of log lines − 1; **Page Down** lowers it by 1, to at
  least 0 [CI-14].
- It is 0 when the console is created and is set back to 0 whenever any line is executed (section 3)
  [CI-6, CI-14]. Opening and closing do not change it [CI-14]. The mouse wheel does not scroll (CS 2.3).
- The offset counts from the newest line, so new log lines move the shown part along with the newest
  (**believed**: follows from the rule; drawing not captured).
- Scrolling was not measured (the run captured no frames).

## 10. Unestablished

- Bytes 0x80 … 0xFF pasted from the clipboard (section 6).
- Whether, under Windows, an Insert with a clipboard holding no text leaves the clipboard unavailable to
  other programs, and until when (section 6).
- In which frame a due repeat is first used relative to the console's own frame update (a delay of at
  most one frame; the order of the engine's per-frame clients was not established).
- Whether the operating system's keyboard auto-repeat adds key presses to the events the console
  receives (**believed** not: the input system reads changes of key state, and a held key does not
  change state; the run's repeats match section 5).
- The measured run ran under Wine; the same behaviour under Windows is **believed** (the measured facts
  are the engine's own key handling).

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user, installed by the project's install assembly (`@gog_original`).
- Static analysis: Ghidra 12.2, headless and read-only, of `XR_3DA.exe` (the console's key handling,
  repeat timing, history, completion, clipboard, line execution; the input system's key events; the
  device's timers; the input system's Alt+F4 test and the `quit` command); data values and import tables
  read directly from the file.
- Execution: one headless run of the unmodified `XR_3DA.exe` under Wine in the project's container
  (`xtask wine run`) with `--skipmemcheck -silent_error_mode -nosound -nointro -ltx cmd.ltx`, the
  configuration file a copy of the shipped `user.ltx` and `fsgame.ltx` pointing the user-data folder into
  the run folder. Keys were typed into the virtual display from outside with `xdotool` in the main menu;
  the clipboard was set from outside by a small program written for the purpose; the game was ended with
  `quit`. A second run (2026-11-26, the same setup) emptied the clipboard and tested from outside whether
  it could be opened, by another small program, around an Insert, and ended the game by holding left Alt
  and F4 in the console. Evidence: the log files the game wrote and the drivers' output (the clipboard
  tests, whether the program ended).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- A frame in which the keyboard's buffered input cannot be read (input lost or not acquired, and not
  regained at once) delivers no key events and no hold reports to any receiver, and makes no Alt+F4
  test (section 1).
- `display-mode.md` A.2.1 says the game ends only from its own menus or console; the Alt+F4 rule of
  section 1 also ends it, from any receiver.
