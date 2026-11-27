SPDX-FileCopyrightText: 2026 Maksym Motorny
SPDX-License-Identifier: BSD-3-Clause

Direct3D 9 probes (volya-engine, tests/d3d9probes/)

Small Direct3D 9 programs drawing synthetic scenes, each isolating one
rendering rule at its edges. No game data. Needs Windows 10 or later (the
Universal C Runtime).

Run d3d9probes.exe from this folder, on the desktop of a logged-in user (it
opens a small window for its device and closes it when done, in seconds).
It writes results\ beside itself:

  results\<probe>\<case>.png  each case's frame (RGBA)
  results\info.txt            the adapter and its capabilities
  results\scenes.bin          every case's states, textures and vertices

It prints one line per failed Direct3D call and exits with 1 if any failed,
0 otherwise.

Bring results\ back as docs/testing.md in the repository says (Direct3D 9
probes on real hardware).
