-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The story state's Lua side (story.rs, story/members.rs;
-- docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md, whose
-- section numbers these are): the members of `game_object`, `CGameTask`,
-- `level` and `game` over the Rust side's operations, and what their
-- engine parts ask: resolving and calling the task and sequence functions
-- (6, 11.2), the portions' `action` functions (info-portions.md 3.3) and
-- the `story_ids` lookups of a loaded task (3.4). Run once per state, after
-- the game object basics and before any script.
--
-- Returns the entry function, which the Rust side keeps in the registry.

-- The Rust side's operations, and the factory of C functions calling kept
-- entries.
local raw, kept = __volya_story, __volya_kept
__volya_story, __volya_kept = nil, nil

-- Names an engine-used name resolves to (loader.lua: resolve).
local resolve = kept("volya.script_loader", "resolve")

local globals = _G
local ipairs, pcall, rawget, rawset, tonumber, tostring, type, error =
  ipairs, pcall, rawget, rawset, tonumber, tostring, type, error

-- Key -> the function a name resolved to (6.1); keys of names that didn't
-- resolve have none, and are never called.
local functions = {}

-- A raised value as text, raising nothing.
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

-- A failing call is fatal (D 1.2): the Rust side stops the state, and the
-- error goes on unwinding.
local function failed(raised)
  error(raw("fatal", describe(raised)), 0)
end

local function call(f, ...)
  local ok, raised = pcall(f, ...)
  if not ok then failed(raised) end
end

-- `story_ids[text]`, the story id a task file's `object_story_id` names
-- (3.4): a text it lacks reads as 0 (believed). TODO(#215): spec K
-- registers `story_ids`; until then every text reads as 0.
local function story_id(text)
  local ids = rawget(globals, "story_ids")
  if ids == nil then return 0 end
  local ok, id = pcall(function() return ids[text] end)
  return ok and tonumber(id) or 0
end

local requests = {
  resolve = function(key, name, log)
    local f = resolve(name, "function")
    if f ~= nil then
      functions[key] = f
    elseif log then
      raw("log", "Cannot find script function described in task objective  " .. name)
    end
  end,
  story_id = function(key, text)
    raw("story_id", key, story_id(text))
  end,
  call = function(key, task, index)
    local f = functions[key]
    if f ~= nil then call(f, task, index) end
  end,
  call_bare = function(key)
    local f = functions[key]
    if f ~= nil then call(f) end
  end,
  -- A portion's `action`: what a name that doesn't resolve does is
  -- unestablished (info-portions.md 3.3); fatal here (our wording).
  action = function(name, holder)
    local f = resolve(name, "function")
    if f == nil then failed("cannot find script function " .. name) end
    call(f, holder, nil)
  end,
}

-- Runs what the engine parts asked, in order.
local function drain()
  local batch = raw("take")
  for i = 1, #batch do
    local r = batch[i]
    requests[r[1]](r[2], r[3], r[4])
  end
end

-- A complete or fail function's result (6.2): nil when its name didn't
-- resolve.
local function condition(key, task, index)
  local f = functions[key]
  if f == nil then return nil end
  local ok, result = pcall(f, task, index)
  if not ok then failed(result) end
  return not not result
end

-- A member over the Rust operation `name`; `draining` runs what it asked
-- when it returns.
local function member(name, draining)
  if draining then
    return function(...)
      local result = raw(name, ...)
      drain()
      return result
    end
  end
  return function(...) return raw(name, ...) end
end

local function define(target, names, draining)
  for _, name in ipairs(names) do target[name] = member(name, draining) end
end

-- A namespace table, made unless another spec made it.
local function namespace(name)
  local t = rawget(globals, name)
  if t == nil then
    t = {}
    rawset(globals, name, t)
  end
  return t
end

local game_object = rawget(globals, "game_object")
define(game_object, {
  "has_info", "dont_has_info", "info_add", "info_clear", "get_task_state",
  "give_talk_message", "set_tip_text", "set_tip_text_default",
}, false)
define(game_object, {
  "give_info_portion", "disable_info_portion", "give_task", "set_task_state",
  "give_game_news",
}, true)
define(rawget(globals, "CGameTask"), { "load", "add_objective" }, true)
define(namespace("level"), {
  "map_add_object_spot", "map_add_object_spot_ser", "map_change_spot_hint",
  "map_has_object_spot", "map_remove_object_spot", "get_game_difficulty",
  "set_game_difficulty",
}, false)
local game = namespace("game")
define(game, { "has_active_tutorial" }, false)
define(game, { "start_tutorial" }, true)

return function(op, ...)
  if op == "drain" then
    drain()
  elseif op == "condition" then
    return condition(...)
  elseif op == "resolved" then
    return functions[(...)] ~= nil
  end
end
