// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Views of the first level and what each frame asks of WebGL 2 (volya_engine::selftest).

import { gameChecks } from "./render-game.js";

export const tests = await gameChecks("draw-calls");
