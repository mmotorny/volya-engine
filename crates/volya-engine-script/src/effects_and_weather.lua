-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- Effects and weather's Lua side (effects_and_weather.rs;
-- docs/behavior/script-api-effects-and-weather.md, whose section numbers
-- these are): `effector:start` and `effector:finish`, which hand the object
-- to the engine (2.2), and the `level` functions (4 – 7) over the Rust
-- side's operations. Run once per state, after the game object basics and
-- the classes of this spec, before any script.

-- The Rust side's operations, and `level.environment`.
local raw, environment = __volya_effects_and_weather, __volya_environment
__volya_effects_and_weather, __volya_environment = nil, nil

local ipairs, rawget, rawset, type = ipairs, rawget, rawset, type

-- The effectors the engine holds (2.2): the engine part keeps a strong
-- reference, never released, so they are never collected.
local held = {}

effector.start = function(self)
  raw("start", self)
  held[self] = true
end

effector.finish = function(self)
  raw("finish", self)
  held[self] = true
end

local level = rawget(_G, "level")
if type(level) ~= "table" then
  level = {}
  rawset(_G, "level", level)
end

for _, name in ipairs({
  "add_pp_effector", "remove_pp_effector", "set_pp_effector_factor",
  "add_complex_effector", "remove_complex_effector",
  "add_cam_effector", "add_cam_effector2", "remove_cam_effector",
  "set_weather", "get_weather", "set_weather_fx", "is_wfx_playing", "rain_factor",
}) do
  level[name] = function(...) return raw(name, ...) end
end

level.environment = environment
