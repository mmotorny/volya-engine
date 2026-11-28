-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The inventory's Lua side (inventory.rs;
-- docs/behavior/script-api-inventory.md, whose section numbers these are):
-- the members of `game_object` (1.2) over the Rust side's operations, and
-- the calls of the script functions `iterate_inventory` and
-- `inventory_for_each` make (3.3, 3.4). Run once per state, after the game
-- object basics and before any script.

-- The Rust side's operations.
local raw = __volya_inventory
__volya_inventory = nil

local pcall, rawget, tostring, type, error = pcall, rawget, tostring, type, error

local game_object = rawget(_G, "game_object")

for _, name in ipairs({
  "object_count", "object", "item_in_slot", "active_slot", "active_item",
  "activate_slot", "get_current_outfit", "get_current_outfit_protection",
  "drop_item", "drop_item_and_teleport", "transfer_item", "mark_item_dropped",
  "marked_dropped", "eat", "buy_supplies", "money", "give_money",
  "transfer_money", "get_ammo_in_magazine", "get_ammo_total",
  "set_ammo_elapsed", "unload_magazine", "enable_attachable_item",
  "attachable_item_enabled", "is_inv_box_empty", "best_item", "best_weapon",
}) do
  game_object[name] = function(...) return raw(name, ...) end
end

-- A raised value as text, raising nothing.
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

-- Calls `f` for each handle of the list operation `name` gives (nil for an
-- object that is not an owner): with `arg` first when `with_arg`
-- (`iterate_inventory`), else with the owner second (`inventory_for_each`). A failing
-- call is fatal (A 5.3, D 1.2): the Rust side stops the state. The list is
-- taken before the first call (our choice: what the function may do to the
-- inventory meanwhile is unestablished, 3.3).
local function each(name, with_arg)
  return function(self, f, arg)
    if f ~= nil and type(f) ~= "function" then
      error("game_object:" .. name .. ": no form of it takes these arguments", 2)
    end
    local items = raw(name, self)
    if items == nil then return end
    if f == nil then
      -- Believed a crash (3.3).
      if #items > 0 then raw("crash") end
      return
    end
    for i = 1, #items do
      local ok, raised
      if with_arg then
        ok, raised = pcall(f, arg, items[i])
      else
        ok, raised = pcall(f, items[i], self)
      end
      if not ok then error(raw("fatal", describe(raised)), 0) end
    end
  end
end

game_object.iterate_inventory = each("iterate_inventory", true)
game_object.inventory_for_each = each("inventory_for_each", false)
