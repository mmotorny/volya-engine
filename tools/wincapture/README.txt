SPDX-FileCopyrightText: 2026 Maksym Motorny
SPDX-License-Identifier: BSD-3-Clause

The original's capture on Windows (volya-engine, tools/wincapture/)

Runs the installed S.T.A.L.K.E.R.: Shadow of Chernobyl (GOG) unmodified,
from a copy of its install, on a frame clock, and captures the frames of
the fidelity suite's views, or the audio output of the audio-fidelity
suite's walks. Needs Windows 10 or later. Contains no game
data; everything it writes is game assets and stays on this machine.

- volyacapture.exe  the driver
- winmm.dll         the frame clock and a deterministic heap (a region
                    reserved at 0x20000000; a run fails without it), copied
                    into the copy's bin\ for a run
- counter.dll       the time-stamp counter on the frame clock, a DynamoRIO
                    client the game starts under, importing only ntdll;
                    DynamoRIO is detached once the first frame is held
- dynamorio\        DynamoRIO's drrun and runtime, 32-bit (BSD; its
                    License.txt), which starts the game with the client
- counter-kernel32.dll  the same client opening the frame clock through
                    kernel32, as before the recipe (--kernel32-client)
- counterprobe.exe  the self-test's program, ours: reads the counter
- OpenAL32.dll      the walks' audio device on the frame clock (--mojoal),
                    ours: MojoAL (zlib; mojoal\LICENSE.txt) with no output
                    device, copied into the copy's bin\ for such a run
- SDL3.dll          SDL 3 (zlib; sdl3\LICENSE.txt), which MojoAL runs on
- plan.txt          the views and walks, their frames and settings

  volyacapture state
  volyacapture selftest [--work DIR]
  volyacapture check [--runs N] [--game DIR] [--work DIR]
  volyacapture full  [--runs N] [--game DIR] [--work DIR]
  volyacapture audio [--walk NAME] [--audio-device NAME | --mojoal]
                     [--runs N] [--game DIR] [--work DIR]

selftest: drrun and the client with counterprobe.exe in place of the game
(no game needed); check: the title menu only; full: every view. N runs
(default 2), compared.
audio: every walk (or the one NAME names; walks whose frames overlap
need one named) with sound on, the game's output recorded by WASAPI
loopback from the default output device as it is (or the one whose name
contains NAME), and each walk's frames at its start, each segment's end and its end kept.
Nothing changes a volume or a device setting; the
machine's outputs must be silenced physically. N runs (default 1).
--mojoal, unverified: the game's audio rendered instead by our
OpenAL32.dll, one frame's samples at each frame, with no output device
(nothing is heard); N runs (default 2), compared. Tried on this machine:
the game created no sound source on it (its log: AL_INVALID_OPERATION),
cause unknown. A run that swapped the DLLs puts the shipped OpenAL32.dll
back; the next run does if it died first.
DIR of the game: default the install's registry entry. Work folder: default
%TEMP%\volya-engine-capture (no space). Results: <work>\runs\. What it
logs names no folder of this machine (<game folder>, <work>, <home>). The
game runs full screen at the plan's size; keep the machine untouched while
it runs (keys go to the game's window, which must stay in front).

check, full and audio all run the game this way
(docs/behavior/capture-recipe.md). Each invocation first removes its
mode's earlier <work>\runs\<mode>-* folders and summary; a failed one
writes a summary of its failure.
To narrow down a failure, check, full and audio take --no-heap (the
proxy leaves the heap functions alone) and --kernel32-client (the game
starts under counter-kernel32.dll); both are recorded in log.txt and
events.txt.

Before each start log.txt has the machine's state: the display, the
session, the time since the last input and since the previous run's
start, the display mode, the foreground window's kind (never its title).
A full-screen start that fails says so in the run's failure. The display
is left as Windows keeps it. --keep-display-on, unverified (not yet run
here): the display is kept on while the driver runs, and one Windows
reports off before a start is asked on and waited for (up to 15 s), else
the run fails saying so; with it, --wake-input, unverified, also sends a
zero mouse move to a display that is off. --front-from-frame-1,
unverified, leaves the game's window as it comes up until the first
frame. volyacapture
state prints the machine's state alone.

Steps: docs/testing.md, The original on Windows.
