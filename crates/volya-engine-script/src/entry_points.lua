-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The engine's entry points' Lua side (entry_points.rs;
-- docs/behavior/script-api-engine-entry-points.md, whose section numbers
-- these are): finding a script function by name and calling it (1.1, 1.2),
-- the `[common] script` list's `_initialize` functions (2.1), and the
-- weapon-parameter functions kept from their first use (2.5). Run once per
-- state, after the loader and before any script.
--
-- Returns the entry function, which the Rust side keeps in the registry.

local kept = __volya_kept
__volya_kept = nil

-- Names an engine-called name resolves to (loader.lua: resolve), and
-- loading a namespace as for an engine-called function (loader.lua's
-- default operation).
local resolve = kept("volya.script_loader", "resolve")
local load = kept("volya.script_loader", "namespace")

local globals = _G
local ipairs, pcall, rawget, select, tonumber, tostring, type, unpack =
  ipairs, pcall, rawget, select, tonumber, tostring, type, unpack

-- A raised value as text, raising nothing.
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

-- Lua's string conversion of a result (2.2 step 4): a number becomes its
-- text, any other non-string no text (nil).
local function text_of(value)
  if type(value) == "string" then return value end
  if type(value) == "number" then return tostring(value) end
  return nil
end

-- Lua's number conversion of a result (2.5 step 2): a non-number gives 0.
local function number_of(value)
  if type(value) == "number" then return value end
  if type(value) == "string" then return tonumber(value) or 0 end
  return 0
end

-- A protected call's outcome: ("ok", results...) or ("error", text) (1.2).
local function outcome(ok, ...)
  if not ok then return "error", describe((...)) end
  return "ok", ...
end

-- Calls `f` with the arguments in a protected call (1.2).
local function protected(f, ...)
  return outcome(pcall(f, ...))
end

-- Section 2.1: for each item X of the list, namespace X is loaded, then the
-- global X_initialize, found by raw access, is resolved and called with no
-- arguments if it is a function. ("ok", names called...), ("error", text)
-- or ("unresolved", name).
local function initialize(...)
  local called = {}
  for i = 1, select("#", ...) do
    local item = (select(i, ...))
    load(item)
    local name = item .. "_initialize"
    if type(rawget(globals, name)) == "function" then
      local f = resolve(name, "function")
      if f == nil then return "unresolved", name end
      local ok, raised = pcall(f)
      if not ok then return "error", describe(raised) end
      called[#called + 1] = name
    end
  end
  return "ok", unpack(called)
end

-- Section 1: resolves `name` to a function and calls it with the arguments.
-- ("missing") when it doesn't resolve; else as `protected`, the first
-- result converted by `conversion` ("text", "number", "truth" or "none").
local function call(name, conversion, ...)
  local f = resolve(name, "function")
  if f == nil then return "missing" end
  local outcome, result = protected(f, ...)
  if outcome ~= "ok" then return outcome, result end
  if conversion == "text" then return "ok", text_of(result) end
  if conversion == "number" then return "ok", number_of(result) end
  -- The runtime's truth value: false for false and nil (also no result).
  if conversion == "truth" then return "ok", result ~= nil and result ~= false end
  return "ok"
end

-- Section 2.5: the five weapon-parameter functions, resolved the first
-- time (a rebuild makes a new state, which resolves them again).
local WEAPON = { "GetRPM", "GetDamage", "GetDamageMP", "GetHandling", "GetAccuracy" }
local weapon

-- The four calls of one update for item section `section`: ("ok", rpm,
-- accuracy, damage, handling), ("error", text) or ("missing", name) for a
-- function that didn't resolve.
local function weapon_parameters(section, single)
  if weapon == nil then
    weapon = {}
    for i = 1, #WEAPON do
      weapon[WEAPON[i]] = resolve("ui_wpn_params." .. WEAPON[i], "function") or false
    end
  end
  local damage = single and "GetDamage" or "GetDamageMP"
  local values = {}
  for i, name in ipairs({ "GetRPM", "GetAccuracy", damage, "GetHandling" }) do
    local f = weapon[name]
    if not f then return "missing", "ui_wpn_params." .. name end
    local ok, result = pcall(f, section)
    if not ok then return "error", describe(result) end
    values[i] = number_of(result)
  end
  return "ok", values[1], values[2], values[3], values[4]
end

-- The names among `...` that aren't defined globals (raw access).
local function undefined(...)
  local names = {}
  for i = 1, select("#", ...) do
    local name = (select(i, ...))
    if rawget(globals, name) == nil then names[#names + 1] = name end
  end
  return unpack(names)
end

return function(op, ...)
  if op == "initialize" then
    return initialize(...)
  elseif op == "call" then
    return call(...)
  elseif op == "weapon" then
    return weapon_parameters(...)
  elseif op == "undefined" then
    return undefined(...)
  end
end
