-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The script loader's Lua side (loader.rs; docs/behavior/script-loading.md
-- section 2): namespaces, the already-loaded test, automatic loading on
-- global access and `prefetch`. Run once per state, before any script.
-- Returns the function the Rust side calls; nothing of it stays reachable
-- from the globals except the metatable's handler and `prefetch`.

-- The Rust side: the chunk for a script name (2.3) and failure reports.
local chunk_for, report = __volya_script_chunk, __volya_script_report
__volya_script_chunk, __volya_script_report = nil, nil

-- Taken now, so scripts replacing them change nothing here.
local globals = _G
local loadstring, pcall, rawget, setmetatable, tostring, type, error =
  loadstring, pcall, rawget, setmetatable, tostring, type, error
local find, sub = string.find, string.sub

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
-- is neither nil nor a table is the fatal error.
local function loaded(name, parts, n)
  local t = globals
  for i = 1, n do
    t = rawget(t, parts[i])
    if t == nil then return false end
    if type(t) ~= "table" then
      local message = "namespace " .. name ..
        ": the namespace name is already being used by the non-table object"
      report("fatal", name, message)
      error(message, 0)
    end
  end
  return true
end

-- Loads namespace `name` (2.2, 2.3), first testing whether it is already
-- loaded if `test`. Returns true when its file doesn't exist. Failures are
-- discarded, as the original does (2.4); the Rust side keeps them.
local function load(name, test)
  local parts, n = components(name)
  if not parts then return false end
  if test and loaded(name, parts, n) then return false end
  local chunk, chunkname = chunk_for(name)
  if not chunk then return chunkname == "missing" end
  local f, message = loadstring(chunk, chunkname)
  if not f then
    report("compile", name, message)
    return false
  end
  local ok, raised = pcall(f)
  if not ok then report("run", name, tostring(raised)) end
  return false
end

-- Automatic loading on global access (2.2.3). The most recently missing
-- name isn't looked for again while it is the one remembered.
local missing
setmetatable(globals, {
  __index = function(_, key)
    local name = key
    if type(key) == "number" then
      name = tostring(key)
    elseif type(key) ~= "string" then
      return nil
    end
    if name ~= "" and name ~= missing and load(name, true) then missing = name end
    return rawget(globals, key)
  end,
})

-- `prefetch(N)` loads N as a function name's namespace is (2.2.5).
function prefetch(name)
  if type(name) == "number" then
    name = tostring(name)
  elseif type(name) ~= "string" then
    error("bad argument #1 to 'prefetch' (string expected, got " .. type(name) .. ")", 2)
  end
  load(name, true)
end

return function(op, name)
  if op == "globals" then
    -- Startup loads `_G` without the test (2.2.1).
    load("_G", false)
  else
    load(name, true)
  end
end
