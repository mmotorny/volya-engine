-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The sound's Lua side (sound.rs; docs/behavior/script-api-sound.md, whose
-- section numbers these are): the `level` functions (5) and the creature
-- members of `game_object` (6, 7) over the Rust side's operations. Run
-- once per state, after the game object basics and the dialogs and trade,
-- before any script.

-- The Rust side's operations.
local raw = __volya_sound
__volya_sound = nil

local pcall, rawget, rawset, select, tostring, type, error, ipairs =
  pcall, rawget, rawset, select, tostring, type, error, ipairs

-- A raised value as text, raising nothing.
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

local game_object = rawget(_G, "game_object")
for _, name in ipairs({
  "add_sound", "play_sound", "remove_sound", "set_sound_mask", "active_sound_count",
  "sound_prefix", "sound_voice_prefix", "external_sound_start", "external_sound_stop",
  "set_sound_threshold", "restore_sound_threshold", "get_sound_info",
}) do
  game_object[name] = function(...) return raw(name, ...) end
end

-- 7.2: an iterator over the remembered sounds.
game_object.memory_sound_objects = function(obj)
  local remembered = raw("memory_sound_objects", obj)
  local i = 0
  return function()
    i = i + 1
    return remembered[i]
  end
end

local level = rawget(_G, "level")
if level == nil then
  level = {}
  rawset(_G, "level", level)
end

-- 5.1, 5.2.
for _, name in ipairs({ "set_snd_volume", "get_snd_volume", "prefetch_sound" }) do
  level[name] = function(...) return raw(name, ...) end
end

-- 5.3: `f(name)`, or `f(object, name)` with a bound value, for each name in
-- order, before returning; results not read; a failing call is fatal (D
-- 1.2). Without a function nothing is called.
level.iterate_sounds = function(...)
  local bound = select("#", ...) >= 4
  local list, n, object, f
  if bound then
    list, n, object, f = ...
  else
    list, n, f = ...
  end
  local names = raw("iterate_sounds", list, n)
  if type(f) ~= "function" then return end
  for _, name in ipairs(names) do
    local ok, raised
    if bound then
      ok, raised = pcall(f, object, name)
    else
      ok, raised = pcall(f, name)
    end
    if not ok then error(raw("fatal", describe(raised)), 0) end
  end
end
