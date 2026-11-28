-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The action planner's Lua side (action_planner.rs;
-- docs/behavior/script-api-action-planner.md, whose section numbers these
-- are): the members of `action_planner` and `planner_action` (6.2, 8) and
-- `game_object:motivation_action_manager` (10) over the Rust side's
-- operations, so no planner's engine part is held while its evaluators
-- and actions run. Run once per state, after the classes are registered.
--
-- Returns the entry function, which the Rust side keeps in the registry:
-- it calls an object's method for the planner (a script class's override
-- or the engine's own), tells whether an action's `weight` is overridden,
-- and runs a planner's update for the engine.

-- The Rust side's operations.
local raw = __volya_action_planner
__volya_action_planner = nil

local rawget, rawequal, select, pairs = rawget, rawequal, select, pairs

local action_base = rawget(_G, "action_base")
local action_planner = rawget(_G, "action_planner")
local planner_action = rawget(_G, "planner_action")
local game_object = rawget(_G, "game_object")

-- The engine takes over the script objects added to a planner (6.2): the
-- planner object keeps them in a field of its own, by kind and id, so the
-- script side collects them with it and not before.
local HELD = "__volya_planner_parts"

local function keep(self, kind, id, object)
  if id == nil then return end
  local held = self[HELD]
  if held == nil then
    held = {action = {}, evaluator = {}}
    self[HELD] = held
  end
  held[kind][id] = object
end

local members = {
  setup = function(self, object) raw("setup", self, object) end,
  update = function(self) raw("update", self) end,
  add_action = function(self, id, action)
    keep(self, "action", raw("add_action", self, id, action), action)
  end,
  remove_action = function(self, id)
    keep(self, "action", raw("remove_action", self, id), nil)
  end,
  action = function(self, id) return raw("action", self, id) end,
  add_evaluator = function(self, id, evaluator)
    keep(self, "evaluator", raw("add_evaluator", self, id, evaluator), evaluator)
  end,
  remove_evaluator = function(self, id)
    keep(self, "evaluator", raw("remove_evaluator", self, id), nil)
  end,
  evaluator = function(self, id) return raw("evaluator", self, id) end,
  set_goal_world_state = function(self, s) raw("set_goal_world_state", self, s) end,
  current_action_id = function(self) return raw("current_action_id", self) end,
  current_action = function(self) return raw("current_action", self) end,
  initialized = function(self) return raw("initialized", self) end,
  actual = function(self) return raw("actual", self) end,
  clear = function(self) raw("clear", self) end,
}

for name, f in pairs(members) do
  action_planner[name] = f
  -- 8: a planner action's own `setup` is the action's.
  if name ~= "setup" then planner_action[name] = f end
end

-- 8: the planner action's own members.
planner_action.setup = function(self, object, storage)
  raw("pa_setup", self, object, storage)
end
planner_action.initialize = function(self) raw("pa_initialize", self) end
planner_action.execute = function(self) raw("pa_execute", self) end
planner_action.finalize = function(self) raw("pa_finalize", self) end
local planner_action_weight = function(self, s1, s2)
  return raw("pa_weight", self, s1, s2)
end
planner_action.weight = planner_action_weight

-- 10.
game_object.motivation_action_manager = function(self)
  return raw("motivation_action_manager", self)
end

-- The member an action's `weight` is when no script class overrides it.
local action_base_weight = action_base().weight

return function(op, object, ...)
  if op == "call" then
    local method = ...
    return object[method](object, select(2, ...))
  elseif op == "weight_overridden" then
    local weight = object.weight
    return not (rawequal(weight, action_base_weight) or rawequal(weight, planner_action_weight))
  elseif op == "update" then
    object:update()
  end
end
