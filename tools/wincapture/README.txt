SPDX-FileCopyrightText: 2026 Maksym Motorny
SPDX-License-Identifier: BSD-3-Clause

The original's capture on Windows (volya-engine, tools/wincapture/)

Runs the installed S.T.A.L.K.E.R.: Shadow of Chernobyl (GOG) unmodified,
from a copy of its install, on a frame clock, and captures the frames of
the fidelity suite's views, or the audio output of the audio-fidelity
suite's walks. Needs Windows 10 or later. Contains no game
data; everything it writes is game assets and stays on this machine.

- volyacapture.exe  the driver
- winmm.dll         the frame clock, copied into the copy's bin\ for a run
- counter.dll       the time-stamp counter on the frame clock, a DynamoRIO
                    client the game runs under
- dynamorio\        DynamoRIO's drrun and runtime, 32-bit (BSD; its
                    License.txt), which starts the game with the client
- counterprobe.exe  the self-test's program, ours: reads the counter
- plan.txt          the views and walks, their frames and settings

  volyacapture selftest [--work DIR]
  volyacapture check [--runs N] [--game DIR] [--work DIR]
  volyacapture full  [--runs N] [--game DIR] [--work DIR]
  volyacapture audio --audio-device NAME [--runs N] [--game DIR] [--work DIR]
  volyacapture audio-devices

selftest: drrun and the client with counterprobe.exe in place of the game
(no game needed); check: the title menu only; full: every view. N runs
(default 2), compared.
audio: every walk with sound on, the game's output recorded by WASAPI
loopback from the output device whose name contains NAME, which is made
the default output for the run and the previous default put back after;
it must be a device no speaker plays (a virtual one, or an output with
nothing connected). No volume is changed. N runs (default 1).
audio-devices: lists the output devices.
DIR of the game: default the install's registry entry. Work folder: default
%TEMP%\volya-engine-capture (no space). Results: <work>\runs\. What it
logs names no folder of this machine (<game folder>, <work>, <home>). The
game runs full screen at the plan's size; keep the machine untouched while
it runs (keys go to the game's window, which must stay in front).

Steps: docs/testing.md, The original on Windows.
