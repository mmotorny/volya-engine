-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- Object event callbacks' Lua side (callbacks.rs;
-- docs/behavior/script-api-object-event-callbacks.md, whose section numbers
-- these are): each object's callback table (3), the sounds queued for id 17
-- (4), the level's call list and its fast calls (5, 6), and the fighters'
-- enemy and patrol callbacks (7, 8). The members are set on the class
-- `game_object` and the namespace `level`. Run once per state, after the
-- class `game_object` is registered.
--
-- Returns the entry function, which the Rust side keeps in the registry.
-- Engine calls of script functions are protected; a failure returns
-- ("error", text) for the Rust side to stop the game with (D 1.2).

-- The Rust side: a handle's object id and whether it is a fighter, raising
-- for anything else.
local object_of = __volya_callback_object
__volya_callback_object = nil

local pcall, rawequal, rawget, rawset, select, tostring, type, error, unpack =
  pcall, rawequal, rawget, rawset, select, tostring, type, error, unpack
local remove = table.remove

local game_object = rawget(_G, "game_object")
local vector = rawget(_G, "vector")

-- The namespace `level`, shared with the specs adding its other functions.
local level = rawget(_G, "level")
if type(level) ~= "table" then
  level = {}
  rawset(_G, "level", level)
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

-- A function stored with its bound value, if any: {f, value, bound}.
local function entry(n, f, value)
  return {f = f, value = value, bound = n == 2}
end

-- Calls entry `e` protected with `...` after its bound value (3.2).
local function call(e, ...)
  if e.bound then return pcall(e.f, e.value, ...) end
  return pcall(e.f, ...)
end

-- 3.1: object id -> event id -> entry. An entry made on lookup is never
-- seen, so none is.
local tables = {}

-- `set_callback(id)`, `(id, f)`, `(id, f, value)`: clears the entry, then
-- stores `f` (and `value`). `nil` for `f` is an empty function: the same as
-- clearing. The id is kept as the number given (our choice: the spec has
-- no conversion of a fraction).
local function set_callback(self, ...)
  local n = select("#", ...)
  local id, f = ...
  if n < 1 or n > 3 or type(id) ~= "number" or (f ~= nil and type(f) ~= "function") then
    no_form("game_object:set_callback")
  end
  local object = object_of(self)
  local t = tables[object]
  if t == nil then
    t = {}
    tables[object] = t
  end
  if f == nil then
    t[id] = nil
  else
    t[id] = entry(n - 1, f, (select(3, ...)))
  end
end

-- Vectors cross from Rust as {x, y, z}; the arguments of 4 have no table.
local function argument(v)
  if type(v) == "table" then
    local r = vector()
    r:set(v[1], v[2], v[3])
    return r
  end
  return v
end

-- 3.2: the engine raises event `id` on `object` with `...`. The entry is
-- read before the call (TODO(#1018): gap 1, the simplest reading: a change
-- the function makes to its table applies from the next event).
local function raise(object, id, ...)
  local t = tables[object]
  local e = t and t[id]
  if e == nil then return "ok" end
  local n = select("#", ...)
  local args = {...}
  for i = 1, n do args[i] = argument(args[i]) end
  local ok, raised = call(e, unpack(args, 1, n))
  if not ok then return "error", describe(raised) end
  return "ok"
end

local function has(object, id)
  local t = tables[object]
  return t ~= nil and t[id] ~= nil
end

-- 4, id 17: object id -> the sounds queued, each its arguments. Delivered
-- through the entry at delivery (TODO(#1018): gap 2, the simplest reading).
local sounds = {}

local function perceive(object, ...)
  if not has(object, 17) then return false end
  local queue = sounds[object]
  if queue == nil then
    queue = {}
    sounds[object] = queue
  end
  queue[#queue + 1] = {n = select("#", ...), ...}
  return true
end

local function deliver(object)
  local queue = sounds[object]
  sounds[object] = nil
  if queue == nil then return "ok" end
  for i = 1, #queue do
    local outcome, text = raise(object, 17, unpack(queue[i], 1, queue[i].n))
    if outcome ~= "ok" then return outcome, text end
  end
  return "ok"
end

-- 6.1: the level's call list. Each entry is {kind, ...}: "plain" (add_call's
-- first form: c, a), "bound" (its second and third: value, c, a) or "fast"
-- (set_fastcall: object, f, value).
local calls = {}

-- Removes every entry `matches`, keeping the order of the rest (6.3).
local function remove_where(matches)
  local kept = {}
  local n = #calls
  for i = 1, n do
    if not matches(calls[i]) then kept[#kept + 1] = calls[i] end
  end
  for i = 1, n do calls[i] = kept[i] end
end

-- Lua equality, or both nil (6.3).
local function same_value(a, b)
  return a == b
end

-- 6.2.
local function add_call(...)
  local n = select("#", ...)
  local a1, a2, a3 = ...
  if n == 2 and type(a1) == "function" and type(a2) == "function" then
    calls[#calls + 1] = {kind = "plain", c = a1, a = a2}
  elseif n == 3 and type(a2) == "string" and type(a3) == "string" then
    -- Looked up now; a missing member is stored as found (our choice: the
    -- spec has it unestablished), failing when called.
    local c, a = a1[a2], a1[a3]
    for i = 1, #calls do
      local e = calls[i]
      if e.kind == "bound" and same_value(e.value, a1) and rawequal(e.c, c)
          and rawequal(e.a, a) then
        return
      end
    end
    calls[#calls + 1] = {kind = "bound", value = a1, c = c, a = a}
  elseif n == 3 and type(a2) == "function" and type(a3) == "function" then
    calls[#calls + 1] = {kind = "bound", value = a1, c = a2, a = a3}
  else
    no_form("level.add_call")
  end
end

-- 6.3: only the second form removes anything.
local function remove_call(...)
  local n = select("#", ...)
  local a1, a2, a3 = ...
  if n == 2 and type(a1) == "function" and type(a2) == "function" then
    return
  elseif n == 3 and type(a2) == "string" and type(a3) == "string" then
    return
  elseif n == 3 and type(a2) == "function" and type(a3) == "function" then
    remove_where(function(e)
      return e.kind == "bound" and same_value(e.value, a1) and rawequal(e.c, a2)
        and rawequal(e.a, a3)
    end)
  else
    no_form("level.remove_call")
  end
end

-- 6.4.
local function remove_calls_for_object(...)
  if select("#", ...) ~= 1 then no_form("level.remove_calls_for_object") end
  local value = ...
  remove_where(function(e)
    return e.kind == "bound" and same_value(e.value, value)
  end)
end

-- 5: one form, `set_fastcall(f, value)`.
local function set_fastcall(self, ...)
  local n = select("#", ...)
  local f, value = ...
  if n ~= 2 or type(f) ~= "function" then no_form("game_object:set_fastcall") end
  local object = object_of(self)
  remove_where(function(e) return e.kind == "fast" and e.object == object end)
  calls[#calls + 1] = {kind = "fast", object = object, f = f, value = value}
end

-- An entry's condition, then its action if the condition is true: whether
-- the entry is done (its action ran, or a fast call's condition is true).
local function evaluate(e)
  if e.kind == "fast" then
    return e.f(e.value) and true or false
  elseif e.kind == "plain" then
    if not e.c() then return false end
    e.a()
  else
    if not e.c(e.value) then return false end
    e.a(e.value)
  end
  return true
end

local function index_of(e)
  for i = 1, #calls do
    if rawequal(calls[i], e) then return i end
  end
end

-- 6.1: one update. The length is read at every step, so entries added
-- during the update are reached in it. An entry the calls moved is found
-- again; one they removed leaves the walk at the entry that took its place
-- (our choice: the spec doesn't cover removal during an update).
local function update()
  local i = 1
  while i <= #calls do
    local e = calls[i]
    local ok, done = pcall(evaluate, e)
    if not ok then return "error", describe(done) end
    local at = i
    if not rawequal(calls[i], e) then at = index_of(e) end
    if at == nil then
      -- Removed by its own calls: the walk goes on at `i`.
    elseif done then
      remove(calls, at)
      i = at
    else
      i = at + 1
    end
  end
  return "ok"
end

-- 7, 8: object id -> entry.
local enemy, patrol = {}, {}

-- `set_enemy_callback()`, `(f)`, `(f, value)` and likewise
-- `set_patrol_extrapolate_callback`: on anything but a fighter nothing.
local function fighter_callback(store, name)
  return function(self, ...)
    local n = select("#", ...)
    local f, value = ...
    if n > 2 or (f ~= nil and type(f) ~= "function") then no_form(name) end
    local object, fighter = object_of(self)
    if not fighter then return end
    if f == nil then
      store[object] = nil
    else
      store[object] = entry(n, f, value)
    end
  end
end

-- Asks a fighter's callback: ("none") without one, ("result", truth).
local function ask(store, object, ...)
  local e = store[object]
  if e == nil then return "none" end
  local ok, result = call(e, ...)
  if not ok then return "error", describe(result) end
  return "result", result and true or false
end

-- Object `object` is destroyed: its tables and queued sounds go (believed:
-- it comes online again as a new object, 13), and a physical object's
-- fast call (5).
local function forget(object, physical)
  tables[object], sounds[object], enemy[object], patrol[object] = nil, nil, nil, nil
  if physical then
    remove_where(function(e) return e.kind == "fast" and e.object == object end)
  end
end

game_object.set_callback = set_callback
game_object.set_fastcall = set_fastcall
game_object.set_enemy_callback = fighter_callback(enemy, "game_object:set_enemy_callback")
game_object.set_patrol_extrapolate_callback =
  fighter_callback(patrol, "game_object:set_patrol_extrapolate_callback")
level.add_call = add_call
level.remove_call = remove_call
level.remove_calls_for_object = remove_calls_for_object

return function(op, ...)
  if op == "raise" then
    return raise(...)
  elseif op == "perceive" then
    return perceive(...)
  elseif op == "has" then
    return has(...)
  elseif op == "deliver" then
    return deliver(...)
  elseif op == "update" then
    return update()
  elseif op == "clear" then
    for i = #calls, 1, -1 do calls[i] = nil end
  elseif op == "count" then
    return #calls
  elseif op == "enemy" then
    return ask(enemy, ...)
  elseif op == "patrol" then
    return ask(patrol, ...)
  elseif op == "forget" then
    forget(...)
  end
end
