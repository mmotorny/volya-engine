-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The scripted actions' Lua side (scripted_actions.rs;
-- docs/behavior/script-api-scripted-actions.md, whose section numbers
-- these are): the members of `game_object` of 2.2 and 2.3 over the Rust
-- side's operations. Run once per state, after the game object basics and
-- the sound, before any script.

-- The Rust side's operations.
local raw = __volya_scripted_actions
__volya_scripted_actions = nil

local rawget, ipairs = rawget, ipairs

local game_object = rawget(_G, "game_object")
for _, name in ipairs({
  "script", "get_script", "get_script_name", "can_script_capture",
  "command", "action", "action_count", "action_by_index", "reset_action_queue",
}) do
  game_object[name] = function(...) return raw(name, ...) end
end
