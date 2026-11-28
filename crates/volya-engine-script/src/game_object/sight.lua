-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- Script API X's memory iterators (sight.rs;
-- docs/behavior/script-api-movement-sight-and-memory.md 7.2): each gives
-- the fighter's records for `for r in npc:not_yet_visible_objects() do`,
-- as script API T's memory_sound_objects. Run once per state, after
-- game_object is registered, before any script.

local raw = __volya_memory
__volya_memory = nil

local game_object = rawget(_G, "game_object")
for _, name in ipairs({
  "memory_visible_objects", "memory_hit_objects", "not_yet_visible_objects",
}) do
  game_object[name] = function(obj)
    local records = raw(name, obj)
    local i = 0
    return function()
      i = i + 1
      return records[i]
    end
  end
end
