-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The level's Lua side (level.rs; docs/behavior/script-api-level.md, whose
-- section numbers these are): the functions of the namespace `level` over
-- the Rust side's operations (2, 3, 4.1), and the client spawn manager's
-- table of awaited ids with the members `add` and `remove` and the engine's
-- calls of its entries (6). Run once per state, after the game object
-- basics and before any script.
--
-- Returns the entry function, which the Rust side keeps in the registry.
-- Engine calls of script functions are protected; a failure returns
-- ("error", text) for the Rust side to stop the game with (D 1.2).

-- The Rust side's operations.
local raw = __volya_level
__volya_level = nil

local ipairs, pcall, rawget, rawset, select, tostring, type, error =
  ipairs, pcall, rawget, rawset, select, tostring, type, error
local insert, remove = table.insert, table.remove

-- The namespace `level`, shared with the specs adding its other functions.
local level = rawget(_G, "level")
if type(level) ~= "table" then
  level = {}
  rawset(_G, "level", level)
end

for _, name in ipairs({
  "name", "present", "object_by_id", "vertex_position", "vertex_in_direction",
  "cover_in_direction", "patrol_path_exists", "get_bounding_volume", "physics_world",
  "spawn_phantom", "client_spawn_manager",
}) do
  level[name] = function(...) return raw(name, ...) end
end

-- A raised value as text, raising nothing.
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

-- Our wording for a call no form matches (depends on A).
local function no_form(name)
  error(name .. ": no form of it takes these arguments", 3)
end

-- 6.1: awaited id -> its entries {requester, f, value, bound}, in
-- increasing requester id.
local awaited = {}

-- Calls entry `e` for object `id` with handle `handle`, protected (6.4).
local function call(e, id, handle)
  if e.bound then return pcall(e.f, e.value, id, handle) end
  return pcall(e.f, id, handle)
end

-- Removes the entry (who, requester); false when there is none.
local function drop(who, requester)
  local list = awaited[who]
  if list == nil then return false end
  for i, e in ipairs(list) do
    if e.requester == requester then
      remove(list, i)
      if #list == 0 then awaited[who] = nil end
      return true
    end
  end
  return false
end

local manager = rawget(_G, "client_spawn_manager")

-- 6.2: `csm:add(awaited, requester, f[, value])`.
manager.add = function(self, ...)
  local n = select("#", ...)
  if n ~= 3 and n ~= 4 then no_form("client_spawn_manager:add") end
  local who, requester, f, value = ...
  if type(who) ~= "number" or type(requester) ~= "number" or type(f) ~= "function" then
    no_form("client_spawn_manager:add")
  end
  who, requester = raw("id", who), raw("id", requester)
  local e = {requester = requester, f = f, value = value, bound = n == 4}
  local handle = raw("handle", who)
  if handle ~= nil then
    -- 1: called at once for the object, nothing stored. A failure is
    -- fatal (D 1.2): the Rust side stops the state, the error unwinds.
    local ok, raised = call(e, who, handle)
    if not ok then error(raw("fatal", describe(raised)), 0) end
    return
  end
  -- 2: made, or replaced in place (in requester order).
  local list = awaited[who]
  if list == nil then
    list = {}
    awaited[who] = list
  end
  for i, old in ipairs(list) do
    if old.requester == requester then
      list[i] = e
      return
    elseif old.requester > requester then
      insert(list, i, e)
      return
    end
  end
  list[#list + 1] = e
end

-- 6.3: `csm:remove(awaited, requester)`; a missing entry's diagnostic goes
-- to the routine that returns at once (M 2.3).
manager.remove = function(self, who, requester)
  if type(who) ~= "number" or type(requester) ~= "number" then
    no_form("client_spawn_manager:remove")
  end
  drop(raw("id", who), raw("id", requester))
end

return function(op, ...)
  if op == "spawned" then
    -- 6.4: every entry awaiting the id, in increasing requester id, then
    -- all of them and the id dropped.
    local id, handle = ...
    local list = awaited[id]
    if list == nil then return end
    awaited[id] = nil
    for _, e in ipairs(list) do
      local ok, raised = call(e, id, handle)
      if not ok then return "error", describe(raised) end
    end
  elseif op == "gone" then
    -- 6.5: every entry of the requester, from every awaited id.
    local requester = ...
    local ids = {}
    for who in pairs(awaited) do ids[#ids + 1] = who end
    for _, who in ipairs(ids) do drop(who, requester) end
  elseif op == "clear" then
    awaited = {}
  end
end
