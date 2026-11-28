-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The dialogs and trade's Lua side (dialogs.rs;
-- docs/behavior/script-api-dialogs-and-trade.md, whose section numbers
-- these are): the talk, start-dialog, trade and trader members of
-- `game_object` (4 to 6) over the Rust side's operations. Run once per
-- state, after the game object basics and the story, before any script.

-- The Rust side's operations.
local raw = __volya_dialogs
__volya_dialogs = nil

local game_object = rawget(_G, "game_object")
for _, name in ipairs({
  "run_talk_dialog", "switch_to_talk", "stop_talk", "is_talking", "enable_talk",
  "disable_talk", "is_talk_enabled", "set_start_dialog", "get_start_dialog",
  "restore_default_start_dialog", "enable_trade", "disable_trade", "is_trade_enabled",
  "switch_to_trade", "set_trader_global_anim", "set_trader_head_anim", "set_trader_sound",
}) do
  game_object[name] = function(...) return raw(name, ...) end
end
