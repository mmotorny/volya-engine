// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// A New Game's level drawn from game data: its start view, objects, lights, the torch, zones, artefacts, grass, presentation and particles (volya_engine::selftest).

import { gameChecks } from "./render-game.js";

export const tests = await gameChecks("render-level");
