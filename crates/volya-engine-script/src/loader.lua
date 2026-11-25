-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The script loader's Lua side (loader.rs; docs/behavior/script-loading.md
-- section 2): namespaces, the already-loaded test, automatic loading on
-- global access and `prefetch`. Run once per state, before any script.
--
-- Returns the entry function, which the Rust side keeps in the registry.
-- Scripts reach the loader only through C functions calling that entry (the
-- globals' `__index` handler and `prefetch`), so neither the functions here
-- nor the Rust functions they hold are reachable from scripts: jit.util
-- reads only Lua functions' upvalues, and there is no debug library.

-- The Rust side: the chunk for a script name (2.3), failure reports, and
-- the factory of C functions calling the kept entry.
local chunk_for, report, kept = __volya_script_chunk, __volya_script_report, __volya_kept
__volya_script_chunk, __volya_script_report, __volya_kept = nil, nil, nil
local ENTRY = "volya.script_loader" -- loader.rs: ENTRY

-- Taken now, so scripts replacing them change nothing here.
local globals = _G
local loadstring, pcall, rawget, setmetatable, tostring, type, error =
  loadstring, pcall, rawget, setmetatable, tostring, type, error
local find, sub = string.find, string.sub

-- The fatal error's message once raised (2.2): the original stops the game.
-- Lua can't unwind past a script's pcall, so (our choice) every later load
-- raises it again, and the Rust side stops the state.
local fatal

-- The dot-separated components of `name` and their count, or nil when one
-- is empty (2.3).
local function components(name)
  local parts, n, start = {}, 0, 1
  while true do
    local dot = find(name, ".", start, true)
    local part = sub(name, start, (dot or 0) - 1)
    if part == "" then return nil end
    n = n + 1
    parts[n] = part
    if not dot then return parts, n end
    start = dot + 1
  end
end

-- Whether namespace `name` is already loaded: following its components from
-- the globals by raw access ends at a table (2.2). A value on the way that
-- is neither nil nor a table is the fatal error (for one before the last
-- component, our choice). Its message's `namespace <N>: ` prefix is ours.
local function loaded(name, parts, n)
  local t = globals
  for i = 1, n do
    t = rawget(t, parts[i])
    if t == nil then return false end
    if type(t) ~= "table" then
      fatal = "namespace " .. name ..
        ": the namespace name is already being used by the non-table object"
      report("fatal", name, fatal)
      error(fatal, 0)
    end
  end
  return true
end

-- A raised value as text, raising nothing (a failing `__tostring` too).
local function describe(raised)
  local ok, text = pcall(tostring, raised)
  if ok and type(text) == "string" then return text end
  return "(error object is a " .. type(raised) .. " value)"
end

-- Loads namespace `name` (2.2, 2.3), first testing whether it is already
-- loaded if `test`. Returns true when its file doesn't exist. Failures are
-- discarded, as the original does (2.4); the Rust side keeps them.
local function load(name, test)
  if fatal then error(fatal, 0) end
  local parts, n = components(name)
  if not parts then return false end
  if test and loaded(name, parts, n) then return false end
  local chunk, chunkname = chunk_for(name)
  if not chunk then return chunkname == "missing" end
  local f, message = loadstring(chunk, chunkname)
  if not f then
    report("compile", name, describe(message))
    return false
  end
  local ok, raised = pcall(f)
  -- The fatal error isn't a run failure: it goes on unwinding.
  if fatal then error(fatal, 0) end
  if not ok then report("run", name, describe(raised)) end
  return false
end

-- Automatic loading on global access (2.2.3). The most recently missing
-- name isn't looked for again while it is the one remembered; an existing
-- file that can't be read isn't remembered (our choice).
local missing
local function index(key)
  local name = key
  if type(key) == "number" then
    name = tostring(key)
  elseif type(key) ~= "string" then
    return nil
  end
  if name ~= "" and name ~= missing and load(name, true) then missing = name end
  return rawget(globals, key)
end

-- `prefetch(N)` loads N as a function name's namespace is (2.2.5). Our
-- choice: a number is converted, any other non-string raises.
local function prefetch(name)
  if type(name) == "number" then
    name = tostring(name)
  elseif type(name) ~= "string" then
    error("bad argument #1 to 'prefetch' (string expected, got " .. type(name) .. ")", 0)
  end
  load(name, true)
end

setmetatable(globals, { __index = kept(ENTRY, "index") })
globals.prefetch = kept(ENTRY, "prefetch")

return function(op, a, b)
  if op == "index" then
    return index(b)
  elseif op == "prefetch" then
    prefetch(a)
  elseif op == "globals" then
    -- Startup loads `_G` without the test (2.2.1).
    load("_G", false)
  else
    load(a, true)
  end
end
