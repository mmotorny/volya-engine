-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- Object binders' Lua side (object_binder.rs;
-- docs/behavior/script-api-object-binders.md, whose section numbers these
-- are): the game-object members `bind_object` and `binded_object` (4), the
-- binders the engine owns, and calling a binding function (3.1). Run once
-- per state, after `game_object` and `object_binder` are registered.
--
-- Returns the entry function, which the Rust side keeps in the registry.

-- The Rust side: attaching a binder, the attached binder, and the factory
-- of C functions calling kept entries.
local attach, attached, kept =
  __volya_binder_attach, __volya_binder_attached, __volya_binder_kept
__volya_binder_attach, __volya_binder_attached, __volya_binder_kept = nil, nil, nil

-- Names an engine-used name resolves to (loader.lua: resolve).
local resolve = kept("volya.script_loader", "resolve")

local pcall, tostring, type = pcall, tostring, type

-- Engine part -> the binder the engine owns (4): kept from Lua's collector
-- until the engine deletes it (5.6).
local owned = {}

local game_object = rawget(_G, "game_object")

-- `bind_object(binder)` (4): the binder becomes the engine's, and the
-- object's attached binder where the game type allows.
game_object.bind_object = function(self, binder)
  owned[attach(self, binder)] = binder
end

-- `binded_object()` (4): the attached binder, or nil.
game_object.binded_object = function(self)
  return attached(self)
end

-- A raised value as text, raising nothing.
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

-- The binding (3.1): the function `name` names, called with `handle` in a
-- protected call, its results discarded. ("missing") when no function is
-- found, ("error", text) for a script error, else ("bound").
local function bind(name, handle)
  local f = resolve(name, "function")
  if f == nil then return "missing" end
  local ok, raised = pcall(f, handle)
  if not ok then return "error", describe(raised) end
  return "bound"
end

return function(op, a, b)
  if op == "bind" then
    return bind(a, b)
  elseif op == "delete" then
    owned[a] = nil
  end
end
