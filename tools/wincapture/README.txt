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
- counterprobe.exe  the self-test's program, ours: reads the counter
- plan.txt          the views and walks, their frames and settings

  volyacapture selftest [--work DIR]
  volyacapture check [--runs N] [--game DIR] [--work DIR]
  volyacapture full  [--runs N] [--game DIR] [--work DIR]
  volyacapture audio [--audio-device NAME] [--runs N] [--game DIR] [--work DIR]

selftest: drrun and the client with counterprobe.exe in place of the game
(no game needed); check: the title menu only; full: every view. N runs
(default 2), compared.
audio: every walk with sound on, the game's output recorded by WASAPI
loopback from the default output device as it is (or the one whose name
contains NAME). Nothing changes a volume or a device setting; the
machine's outputs must be silenced physically. N runs (default 1).
DIR of the game: default the install's registry entry. Work folder: default
%TEMP%\volya-engine-capture (no space). Results: <work>\runs\. What it
logs names no folder of this machine (<game folder>, <work>, <home>). The
game runs full screen at the plan's size; keep the machine untouched while
it runs (keys go to the game's window, which must stay in front).

check, full and audio all run the game this way
(docs/behavior/capture-recipe.md).

Steps: docs/testing.md, The original on Windows.
