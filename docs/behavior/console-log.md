# Console log output

How the original's log works: which programs write to it, how a written text becomes log lines, how the
lines are kept, what of them the console shows, and the log file (its name and folder, its line ends,
every occasion on which it is written, the `-nolog` switch, and the commands `flush` and `clear_log`).
The console's look (rows, colours, scrolling) belongs to the console window and input specifications;
what each command prints belongs to `console-commands.md` and the specifications named there. What could
not be established is marked **unestablished**; a statement inferred without a direct check is marked
**believed**, with why. Facts marked **measured** come from headless runs of the shipped game
(Provenance); the others from the shipped binaries.

Conventions:

- "The engine" is `bin\XR_3DA.exe`, "the game library" `bin\xrGame.dll`, "the core library"
  `bin\xrCore.dll`, all of the shipped Russian edition (GOG build).
- The **log** is the list of text lines kept by the core library while the program runs. A **write** is
  one text handed to the log by any program. The **log file** is the file the log is saved to.
- Specifications referred to (`docs/behavior/` unless noted): CS `console-scope.md`, CC
  `console-commands.md`, ST `steering-the-original.md`, GIA `docs/formats/gog-install-additions.md`.

**Checks.** Every claim carries a tag [CL-*n*]. The checks are one script in the dirty workspace,
`console-log/checks/run.py` (helpers in the same folder); it reads the shipped binaries (import and
export tables, data values, read-only static analysis) and the files of headless runs of the shipped game
(`console-log/run/out/`). All checks pass.

## 1. Log lines

### 1.1 Who writes

- One log exists; it belongs to the core library. The engine, the game library, both renderer libraries,
  the sound library, the network library and the collision library write to it [CL-1]. No other shipped
  library writes to it [CL-1].
- The log exists from the core library's initialisation, which the engine performs first, before the
  CPU is detected, the file system is set up, and the engine and the console are created [CL-2]. So the
  log holds every line from the start of the program; measured, its first lines are the CPU detection
  lines (`* Detected CPU: …`, `* CPU Features: …`) [CL-2]. A write while the log does not exist stores
  nothing [CL-2].

### 1.2 From text to lines

- A written text is split at line feeds (byte 10) [CL-3]:
  - every piece that ends in a line feed is one line; such a piece that is empty is stored as a single
    space (`" "`) [CL-3];
  - the piece after the last line feed (the whole text when it has no line feed) is always one more line,
    stored as it is, so an empty one is stored as an empty line [CL-3].
- So a text ending in a line feed adds an empty line after its last line. Measured: the core library's
  build message is followed by an empty line in the log file, as is the CPU features line, and a crash's
  `stack trace:` line [CL-3].
- No other byte is treated specially: carriage returns, tabs and the first character are stored as they
  are [CL-3].

### 1.3 Message forms

The programs write texts in these forms [CL-4, CL-5]:

| Form | Text written |
|---|---|
| formatted message | the formatted text, cut to its first 1023 characters; a formatted text of length 0 writes nothing (not even an empty line) [CL-4] |
| text and a word | `<text> <word>`; `<text>` alone when there is no word [CL-5] |
| text and an integer | `<text> <integer>`, the integer in signed decimal (an unsigned integer is written the same way, so values from 2³¹ up appear negative) [CL-5] |
| text and a real | `<text> <real>`, the real with six digits after the point (C `%f`) [CL-5] |
| text and a vector | `<text> (<x>,<y>,<z>)`, each with six digits after the point [CL-5] |
| text and a 4 × 4 matrix | `<text>:`, then four lines of four comma-separated reals (six digits after the point), then an empty line (the form ends in a line feed, 1.2) [CL-5] |
| system error | a formatted message `<text>: <the operating system's message for the error code>` [CL-5] |

The forms with a value are built in a 1024-byte buffer, so they hold at most 1023 characters [CL-5];
what happens when one would be longer is **unestablished** (**believed** a fatal error of the C runtime
rather than a cut; no shipped caller was found that reaches it). The texts each command and subsystem
writes are those of CC and of the specifications of their subsystems; CC 2 gives `~ <line>` for entered
lines and `! Unknown command: <name>`.

### 1.4 How lines are kept

- Every line is appended to the end of the one list, under a lock, so writes from several threads are
  kept whole and in the order they were appended [CL-6].
- The list has no line limit: room for 128 lines is set aside when the log file is set up and the list
  grows past it [CL-7]. Nothing removes lines while the program runs except `clear_log` (3.6): the core
  library only creates, appends to, saves and finally frees the list; the engine only reads it (to draw
  the console, for the history, for keys, and for the dedicated-server window); the game library touches
  it only in `clear_log`; no other library reaches it [CL-7].
- **Forwarding.** The core library can pass every stored line to one receiver as well [CL-6]. The
  engine installs none [CL-6]; the game library installs one only while it executes a line sent by a
  remote administrator of a multiplayer server, so that the lines the command writes are sent back to
  that administrator [CL-6]. This belongs to the multiplayer specifications; in single player no line is
  forwarded [CL-6].

## 2. What the console shows

- The console has no store of its own: every frame it draws, it reads the log itself, from the newest
  line (less the scroll offset) backwards, one line per row [CL-8]. So:
  - a line appears in the console in the first frame drawn after it was written, whether the console was
    open or not when it was written;
  - lines written before the console existed (from the start of the program, 1.1) are shown like any
    other;
  - after `clear_log` the console shows only the lines written since (3.6) [CL-7, CL-8].
- The rows, the area, the colours chosen by a line's first character (`!`, `#`, `*`, `-`, `~`) and
  the dropping of the first two characters of such lines are the console window's (CS 3); an empty line
  and a single-space line (1.2) take a row each and show nothing [CL-8].
- The command history is the log's lines beginning with `~` (CS 4.4), read from the same list; so
  `clear_log` empties the history as well [CL-19].
- The dedicated server's separate text window shows the same list in the same way [CL-8].

## 3. The log file

### 3.1 Name and folder

- The name is `<application>_<user>.log`, where `<application>` is `xray` (given by the engine) and
  `<user>` is the name of the Windows account the program runs under, as the operating system reports it
  (up to 63 characters) [CL-2, CL-9].
- The file is in the folder of the file system's `$logs$` entry, when the file system defines one
  [CL-9]; the shipped file-system file sets `$logs$` to `$app_data_root$\logs\` (GIA, ST 2.1). Without
  the entry the name is used as it is (**believed**: relative to the working folder; not run).
- Measured: run under the account `root`, the file was `logs\xray_root.log` in the user-data folder
  [CL-9].

### 3.2 Contents and line ends

- Every write of the file replaces the file with the whole log, from its first line to its last, every
  line followed by a carriage return and a line feed (bytes 13, 10) [CL-12]. There is no header, no
  trailer and no other separator [CL-12].
- Measured: every line of every written file ends in CR LF; a file left from before the run had no
  trace in the written file [CL-12].

### 3.3 When the file is written

| Occasion | What happens | |
|---|---|---|
| Startup, at the end of the file system's setup (before the engine and the configuration file) | unless `-nolog` (3.4), the file is created empty, replacing any earlier file; if it cannot be created, a message box `Can't create log file.` titled `Error` is shown and the program aborts. Measured: a file from before became empty during startup and stayed empty for the whole run until it was next written [CL-10] | CL-10 |
| `flush` | written (3.5) | CL-14 |
| `clear_log` | written after the log is emptied (3.6) | CL-15 |
| Normal end of the program (e.g. `quit`, ST 7) | written when the core library shuts down, the last step of the program's end after the session and the engine have ended, so the file ends with the last line of the shutdown. Measured: after `quit` the file held the shutdown lines up to the last one (`DeviceREF: 1`) [CL-16] | CL-16 |
| A fatal error (the error report the program shows) | the report is written to the log, then the file; then, unless the command line contains `-no_call_stack_assert` or a debugger is attached, `stack trace:` and the stack trace are written to the log and the file is written again [CL-17] | CL-17 |
| An unhandled exception (a crash) | the stack trace (headed `stack trace:`, unless `-no_call_stack_assert`) and the last operating-system error, if any, are written to the log, then the file. Measured: a crash at startup left a file ending in `stack trace:` and an empty line [CL-17]. **Disputed in review:** the last operating-system error is written only together with the stack trace (not with `-no_call_stack_assert`), as an empty line followed by `[error][<code, right-aligned in 8 places>]    : <the system's message>`; when a fatal error has already been reported in the run, neither the stack trace nor the error is written, and the file is still written | CL-17 |
| A fatal message box ending the program | the file is written before the box is shown [CL-17] | CL-17 |
| The graphics device cannot be created | the two lines `Failed to initialize graphics hardware.` and `Please try to restart the game.` are written to the log, then the file; the same text is shown in a message box titled `Error!` and the process ends [CL-17] | CL-17 |

- The file is written on no other occasion: in particular not while the game runs, not when a level is
  loaded or saved, and not when the process is stopped from outside. Measured: the file stayed empty for
  over 40 s of running after startup; a run stopped from outside after `flush` kept the file as `flush`
  wrote it [CL-10, CL-14]. (**Believed** complete: these are all the places that write the file in the
  shipped programs, found by static analysis.)
- Before the file has been set up at startup, writing does nothing: two file-system errors at startup
  (the file-system file cannot be opened; it contains duplicated lines) try to write the file and write
  nothing [CL-11].
- With `-batch_benchmark` the program ends without shutting the core library down, so there is no write
  at the end of the program (**believed**: nothing else writes it there) [CL-16].
- The fatal paths write to the log only once the memory manager is set up (**believed**: always the case
  once the program runs) [CL-17].

### 3.4 `-nolog`

When the command line contains `-nolog`, the log file is never created, emptied or written, on any
occasion of 3.3 [CL-10, CL-11]. The log itself is kept and shown as usual (1, 2), and `flush` and
`clear_log` still write their lines to the log [CL-14, CL-15]. Measured: with `-nolog`, a file left from
before was not touched through `flush` and a normal end of the program [CL-13].

### 3.5 `flush`

`flush` writes the file (3.2), then writes `* Log file has been saved successfully!` to the log [CL-14].
So that line is not in the file `flush` writes; it reaches the file only at the next write. Measured: after
`flush`, the file ended with the line written just before it, and neither `flush`'s own line nor any later
line was in it [CL-14].

### 3.6 `clear_log`

`clear_log` empties the log, then writes the file (which becomes empty), then writes
`* Log file has been cleaned successfully!` to the log [CL-15]. The removed lines are gone for good: they
are not in any later write of the file, the console or the history [CL-7, CL-15, CL-19]. Measured: after
`clear_log` and `quit`, the file began with `* Log file has been cleaned successfully!`, and no earlier
line (not even the startup lines) was in it [CL-15]; the file stayed empty from `clear_log` until the end
of the program [CL-15].

### 3.7 After a crash

The log file's name is given to the crash reporter [CL-18]. Measured: after a crash at startup, a
byte-identical copy of the log file, named `<log name without .log>_<mm>-<dd>-<yy>_<hh>-<mm>-<ss>.mdmp.log`,
lay in the same folder beside a minidump of the same stem with extension `.mdmp` [CL-18]. Which program
makes the copy, and whether it is made on every crash, is **unestablished** (**believed** the crash
reporter, which is handed the file).

## 4. Relation to approved specifications

- CS 7.1 says an empty piece is stored as a single space; that holds for pieces ending in a line feed
  only: the last piece is stored as it is, empty or not (1.2).
- CS 7.3 gives the line end as **believed** CR LF; it is CR LF (3.2). CS 7.3 and ST 2.3 mark when else the
  file is written as unestablished or believed (at exit); it is given in 3.3.

## 5. Unestablished

- What happens when a form with a value (1.3) would exceed 1023 characters, or when a text handed to the
  log without formatting has a piece longer than 1023 characters (**believed** never reached by the
  shipped programs).
- What the console draws for a line of a single marked character (e.g. `!` alone), whose first two
  characters it skips (CS 3).
- Which program makes the crash copy of 3.7 and when.
- Where the file goes when the file system defines no `$logs$` entry (3.1, believed).

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; the shipped binaries in `bin\`; the file-system file generated by the GOG installer, with
  `$app_data_root$` pointed at the run's own folder.
- Binary analysis: read-only static analysis with Ghidra (headless) and disassembly of `xrCore.dll` (the
  log, its writing forms, the log file's setup, saving and shutdown, the fatal-error and crash paths),
  `XR_3DA.exe` (core initialisation and shutdown, the console's drawing and history, the graphics device
  failure, the benchmark path) and `xrGame.dll` (`flush`, `clear_log`, the forwarding receiver); import
  and export tables of every shipped library.
- Runs: the shipped `XR_3DA.exe`, unmodified, headless under Wine in the project's container
  (`bazel run //tools/xtask -- wine run`), with `--skipmemcheck -silent_error_mode -nosound -ltx <file>`;
  console lines were run from the startup configuration file and from a configuration script run when the
  main menu is created. Before each run the log file was seeded with a marker line and its size sampled
  every second during the run. Runs: `clear_log` then `quit`; `flush` then stopped from outside;
  `-nolog` with `flush` and `quit`; and the log files of a run that crashed at startup (made for
  `console-commands.md`). Evidence of the runs is game output and stays local.

## Gaps found in review

- The text of the fatal-error report written to the log (its lines and labels) is not given; it is written as one formatted message, so it is cut to its first 1023 characters (1.3).
- After a fatal error has been reported, the program ends through the unhandled-exception occasion, which writes the file once more without adding a stack trace or system error.
- A write of the file other than the startup creation that cannot open the file writes nothing and reports nothing.
