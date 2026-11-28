-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The object factory's Lua side (object_factory.rs;
-- docs/behavior/script-api-class-ids-and-the-object-factory.md, whose
-- section numbers these are): the factory object and `register` (3), the
-- `class_registrators` call (4), and calling the scripts' class objects to
-- create objects (6.2). Run once per state, after the loader and before
-- any script. The entries themselves are kept by the Rust side, which this
-- chunk tells of each script entry; their class objects are kept here.
--
-- Returns the entry function, which the Rust side keeps in the registry.

-- The Rust side: the class system's factory tools (csrc/volya_class.c),
-- the factory of C functions calling kept entries, and adding an entry.
local tools, kept, add = __volya_factory_tools, __volya_kept, __volya_factory_add
__volya_factory_tools, __volya_kept, __volya_factory_add = nil, nil, nil

-- Names an engine-used name resolves to (loader.lua: resolve).
local resolve = kept("volya.script_loader", "resolve")

local pcall, rawequal, select, tostring, type, error =
  pcall, rawequal, select, tostring, type, error

-- The engine class `object_factory` (3.1) and the factory object, the only
-- one: scripts get it only as a registrator's argument.
local factory_class = rawget(_G, "object_factory")
local factory = tools.object(factory_class)

-- Slot -> the client and server class objects of a script entry.
local clients, servers = {}, {}

-- `register` in both forms (3.1, 3.2). A call that matches neither raises
-- (our wording). A class name that doesn't resolve ends the call silently.
local function register(self, ...)
  local n = select("#", ...)
  local matches = rawequal(self, factory) and (n == 3 or n == 4)
  for i = 1, n do
    if type((select(i, ...))) ~= "string" then matches = false end
  end
  if not matches then
    error("object_factory:register: no form of it takes these arguments", 2)
  end
  local client_name, second, third, fourth = ...
  local client = resolve(client_name, "userdata")
  if client == nil then return end
  if n == 4 then
    local server = resolve(second, "userdata")
    if server == nil then return end
    local slot = add(third, fourth, 4)
    clients[slot], servers[slot] = client, server
  else
    local slot = add(second, third, 3)
    clients[slot], servers[slot] = client, client
  end
end

factory_class.register = register

-- A raised value as text, raising nothing.
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

-- The `class_registrators` call (4): each name that resolves to a function
-- (its namespace loaded first) is called with the factory object, its
-- results ignored. A script error ends the call: ("error", text).
local function registrators(...)
  for i = 1, select("#", ...) do
    local f = resolve((select(i, ...)), "function")
    if f ~= nil then
      local ok, raised = pcall(f, factory)
      if not ok then return "error", describe(raised) end
    end
  end
  return "ok"
end

-- Creates the object of the class object at `slot` for `side` (6.2): the
-- client class called with no arguments, the server class with the
-- section. An engine class is constructed with those arguments by its
-- constructor (TODO(#935): gap 1, the simplest reading). The result's
-- engine part becomes the engine's: ("created", id); ("error", text) for a
-- script error, ("cast", type name) for a result that isn't an object,
-- ("missing") for one without an engine part.
local function create(slot, side, ...)
  local class
  if side == "client" then class = clients[slot] else class = servers[slot] end
  local ok, result
  if tools.is_engine(class) then
    ok, result = pcall(tools.construct, class, ...)
  else
    ok, result = pcall(class, ...)
  end
  if not ok then return "error", describe(result) end
  local part = tools.adopt(result)
  if part == nil then
    -- Our choice: the type named is the class's engine type, else the
    -- class (the spec names none).
    local named, name = pcall(tools.type_name, class)
    return "cast", named and name or "?"
  end
  if part == false then return "missing" end
  return "created", part
end

return function(op, ...)
  if op == "registrators" then
    return registrators(...)
  elseif op == "create" then
    return create(...)
  elseif op == "release" then
    return tools.release(...)
  end
end
