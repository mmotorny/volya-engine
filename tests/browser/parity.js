// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The views both renderers draw, kept by the first build's run and
// compared pixel by pixel by the second's (`cargo xtask parity`, #478).

import { gameChecks } from "./render-game.js";

export const tests = await gameChecks("parity");
