SPDX-FileCopyrightText: 2026 Maksym Motorny
SPDX-License-Identifier: BSD-3-Clause

The original's capture on Windows (volya-engine, tools/wincapture/)

Runs the installed S.T.A.L.K.E.R.: Shadow of Chernobyl (GOG) unmodified,
from a copy of its install, on a frame clock, and captures the frames of
the fidelity suite's views. Needs Windows 10 or later. Contains no game
data; everything it writes is game assets and stays on this machine.

- volyacapture.exe  the driver
- winmm.dll         the frame clock, copied into the copy's bin\ for a run
- plan.txt          the views, their frames and settings

  volyacapture check [--runs N] [--game DIR] [--work DIR]
  volyacapture full  [--runs N] [--game DIR] [--work DIR]

check: the title menu only; full: every view. N runs (default 2), compared.
DIR of the game: default the install's registry entry. Work folder: default
C:\volyacapture (no dash or space). Results: <work>\runs\. The game runs
full screen at the plan's size; keep the machine untouched while it runs
(keys go to the game's window, which must stay in front).

Steps: docs/testing.md, The original on Windows.
