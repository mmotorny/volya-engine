-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The runtime's behaviour, checked on every target (selftest.rs): returns
-- the failed checks' messages.

local failures = {}
local function eq(got, want, what)
  if got ~= want then
    table.insert(failures, what .. ": got " .. tostring(got) .. ", want " .. tostring(want))
  end
end
local function raises(f, pattern, what)
  local ok, err = pcall(f)
  if ok then
    table.insert(failures, what .. ": no error")
  elseif not string.find(tostring(err), pattern, 1, true) then
    table.insert(failures, what .. ": error " .. tostring(err))
  end
end

-- Version (spec 1.1).
eq(_VERSION, "Lua 5.1 (alpha) + LuaJIT 1.0.3", "_VERSION")

-- Libraries (spec 2.1): the original's default set, nothing else.
for _, name in ipairs({ "io", "os", "debug", "package", "require", "module", "loadlib", "bit", "ffi" }) do
  eq(_G[name], nil, name)
end
for _, name in ipairs({ "assert", "collectgarbage", "dofile", "error", "gcinfo", "getfenv",
    "getmetatable", "loadfile", "load", "loadstring", "next", "pcall", "print", "rawequal",
    "rawget", "rawset", "select", "setfenv", "setmetatable", "tonumber", "tostring", "type",
    "unpack", "xpcall", "ipairs", "pairs" }) do
  eq(type(_G[name]), "function", name)
end
local libs = {
  coroutine = { "running", "status", "resume", "yield", "create", "wrap", "cstacksize" },
  table = { "concat", "foreach", "foreachi", "getn", "insert", "remove", "setn", "sort" },
  string = { "byte", "char", "dump", "find", "format", "gfind", "gmatch", "gsub", "len",
    "lower", "match", "rep", "reverse", "sub", "upper" },
  math = { "abs", "acos", "asin", "atan2", "atan", "ceil", "cosh", "cos", "deg", "exp", "floor",
    "fmod", "frexp", "ldexp", "log10", "log", "max", "min", "modf", "pow", "rad", "random",
    "randomseed", "sinh", "sin", "sqrt", "tanh", "tan" },
  jit = { "on", "off", "debug", "compile", "compilesub" },
}
for lib, names in pairs(libs) do
  for _, name in ipairs(names) do
    eq(type(_G[lib][name]), "function", lib .. "." .. name)
  end
end
for _, name in ipairs({ "stats", "bytecode", "const", "upvalue", "closure", "closurenup",
    "mcode", "mcodeaddr", "jsubmcode" }) do
  eq(type(jit.util[name]), "function", "jit.util." .. name)
end
eq(coroutine.coco, true, "coroutine.coco")

-- Numbers (spec 2.5): doubles, written with %.14g.
eq(7 / 2, 3.5, "7 / 2")
eq(2 ^ 10, 1024, "2 ^ 10")
eq(-7 % 3, 2, "-7 % 3")
eq(5.5 % -2, -0.5, "5.5 % -2")
eq(tostring(1 / 3), "0.33333333333333", "tostring(1/3)")
eq(tostring(100), "100", "tostring(100)")
eq(tostring(-0.5), "-0.5", "tostring(-0.5)")
eq(tostring(0.1), "0.1", "tostring(0.1)")
eq(tostring(1e15), "1e+15", "tostring(1e15)")
eq(tostring(123456789012345), "1.2345678901234e+14", "tostring(123456789012345)")
eq(tostring(2 ^ 53), "9.007199254741e+15", "tostring(2^53)")
eq(tostring(1e-5), "1e-05", "tostring(1e-5)")
eq(tostring(1 / 0), "inf", "tostring(1/0)")
eq(tostring(-1 / 0), "-inf", "tostring(-1/0)")
eq(10 .. "", "10", "10 .. ''")
eq(1.5 .. "", "1.5", "1.5 .. ''")
eq("10" + 5, 15, "'10' + 5")
eq(tonumber("  12  "), 12, "tonumber('  12  ')")
eq(tonumber("1e2"), 100, "tonumber('1e2')")
eq(tonumber(".5"), 0.5, "tonumber('.5')")
eq(tonumber("5."), 5, "tonumber('5.')")
eq(tonumber("-3.25e-2"), -0.0325, "tonumber('-3.25e-2')")
eq(tonumber("abc"), nil, "tonumber('abc')")
eq(tonumber("1e"), nil, "tonumber('1e')")
eq(tonumber("0x10"), nil, "tonumber('0x10'): decimal only")
eq(tonumber("inf"), nil, "tonumber('inf')")
eq(tonumber("10", 16), 16, "tonumber('10', 16)")
eq(tonumber("ff", 16), 255, "tonumber('ff', 16)")
eq(tonumber("zz", 36), 1295, "tonumber('zz', 36)")
eq(tonumber("8", 8), nil, "tonumber('8', 8)")
eq(0.1 + 0.2 == 0.3, false, "0.1 + 0.2 ~= 0.3")
eq(math.floor(-3.5), -4, "math.floor")
eq(math.ceil(-3.5), -3, "math.ceil")
eq(math.fmod(7, 3), 1, "math.fmod")
eq(math.sqrt(16), 4, "math.sqrt")
eq(math.pow(2, 0.5), math.sqrt(2), "math.pow")
eq(math.max(1, 5, 3), 5, "math.max")
eq(math.abs(-2), 2, "math.abs")
eq(math.mod(7, 3), 1, "math.mod (the release's default compatibility, LUA_COMPAT_MOD)")
eq(string.format("%.6f", math.sin(1)), "0.841471", "math.sin")
eq(string.format("%.6f", math.exp(1)), "2.718282", "math.exp")
eq(string.format("%.6f", math.log10(2)), "0.301030", "math.log10")
eq(string.format("%.6f", math.atan2(1, 2)), "0.463648", "math.atan2")
local m, e = math.frexp(12)
eq(m, 0.75, "math.frexp mantissa")
eq(e, 4, "math.frexp exponent")
eq(math.ldexp(0.75, 4), 12, "math.ldexp")
local i, f = math.modf(-3.25)
eq(i, -3, "math.modf integer")
eq(f, -0.25, "math.modf fraction")
-- One generator on every target (#106): the C standard's example rand.
math.randomseed(42)
eq(math.random(10), 6, "math.random(10) after randomseed(42)")
math.randomseed(42)
eq(math.random(), 19081 / 32767, "math.random() after randomseed(42)")
local x = math.random()
eq(x >= 0 and x < 1, true, "math.random()")

-- Conversions to integers round (spec 2.5).
eq(string.format("%d", 2.5), "2", "%d of 2.5 (ties to even)")
eq(string.format("%d", 3.5), "4", "%d of 3.5")
eq(string.format("%d", 2.7), "3", "%d of 2.7")
eq(string.format("%d", -2.7), "-3", "%d of -2.7")
eq(string.rep("x", 2.6), "xxx", "string.rep count 2.6")
eq(string.sub("abcdef", 1.6), "bcdef", "string.sub start 1.6")
eq(string.char(65.4), "A", "string.char(65.4)")
-- Out of range, as x86: the low 32 bits.
eq(string.format("%d", 2 ^ 32 + 5), "5", "%d of 2^32 + 5 wraps")
eq(string.format("%d", 2 ^ 31), "-2147483648", "%d of 2^31 wraps")
eq(string.format("%d", -2 ^ 31 - 1), "2147483647", "%d of -2^31 - 1 wraps")

-- string.format.
eq(string.format("%5.2f|%-5d|%05d|%+d|% d", 3.14159, 42, 42, 7, 7), " 3.14|42   |00042|+7| 7", "format numbers")
eq(string.format("%x|%X|%#x|%o|%#o|%u", 255, 255, 255, 8, 8, 3), "ff|FF|0xff|10|010|3", "format integers")
eq(string.format("%e|%E|%.0e|%#.0e", 12345.678, 0.00012, 5e10, 5), "1.234568e+04|1.200000E-04|5e+10|5.e+00", "format %e")
eq(string.format("%g|%g|%g|%g|%G", 100000, 1000000, 0.0001, 0.00001, 1e-10), "100000|1e+06|0.0001|1e-05|1E-10", "format %g")
eq(string.format("%.3g|%#g|%g|%10.4g|%-10.2g|", 3.14159, 1.5, 0, 123.456, 0.000123), "3.14|1.50000|0|     123.5|0.00012   |", "format %g precision")
eq(string.format("%.0f|%.0f|%.0f|%.1f|%.2f", 0.5, 1.5, 2.5, 0.25, 1.005), "0|2|2|0.2|1.00", "format rounding")
eq(string.format("%.14g|%.14g", 0.1, 1 / 3), "0.1|0.33333333333333", "format %.14g")
eq(string.format("%f|%e", 1 / 0, -1 / 0), "inf|-inf", "format inf")
eq(string.format("%08.3f|%-8.3f|%+.1f", -3.14159, 2.5, 0), "-003.142|2.500   |+0.0", "format flags")
eq(string.format("%c%c%c", 76, 117, 97), "Lua", "format %c")
eq(string.format("%s|%10s|%-10s|%.2s", "hi", "right", "left", "cut"), "hi|     right|left      |cu", "format %s")
eq(string.format("%q", 'a "quoted"\n\0'), '"a \\"quoted\\"\\\n\\000"', "format %q")
eq(string.format("%%"), "%", "format %%")
eq(string.format("%s %s", 1, 2.5), "1 2.5", "format %s of numbers")
raises(function() return string.format("%y", 1) end, "invalid option", "format %y")
raises(function() return string.format("%123d", 1) end, "invalid format", "format width too long")

-- Strings (Lua 5.1 alpha).
eq(string.len("abc"), 3, "string.len")
eq(("abc"):upper(), "ABC", "string methods")
eq(string.byte("A"), 65, "string.byte")
eq(string.reverse("abc"), "cba", "string.reverse")
eq(string.lower("ÀBC"), "Àbc", "string.lower leaves bytes above 127")
local s, e2 = string.find("hello world", "o w")
eq(s, 5, "string.find start")
eq(e2, 7, "string.find end")
eq(string.find("a.b", ".", 1, true), 2, "string.find plain")
eq(string.match("key = value", "(%w+)%s*=%s*(%w+)"), "key", "string.match")
eq(string.gsub("hello world", "o", "0"), "hell0 w0rld", "string.gsub")
eq((string.gsub("abc", "%w", "%0%0")), "aabbcc", "string.gsub %0")
raises(function() string.gsub("x", "x", {}) end, "string or function expected", "string.gsub table (5.1 alpha has none)")
eq((string.gsub("abc", ".", function(c) return c:upper() end)), "ABC", "string.gsub function")
local words = {}
for w in string.gfind("one two three", "%a+") do table.insert(words, w) end
eq(table.concat(words, ","), "one,two,three", "string.gfind (as gmatch)")
words = {}
for k, v in string.gmatch("a=1, b=2", "(%w+)=(%w+)") do table.insert(words, k .. v) end
eq(table.concat(words, ","), "a1,b2", "string.gmatch")
eq(type(string.dump(function() return 1 end)), "string", "string.dump")
eq(loadstring(string.dump(function() return 7 end))(), 7, "loadstring of a dump")
eq(#"abc", 3, "# of a string")
eq([[
long]], "long", "long string")

-- Tables (Lua 5.1 alpha).
local t = { 10, 20, 30 }
eq(table.getn(t), 3, "table.getn")
eq(#t, 3, "# of a table")
table.insert(t, 40)
table.insert(t, 1, 5)
eq(table.concat(t, " "), "5 10 20 30 40", "table.insert")
eq(table.remove(t), 40, "table.remove")
eq(table.remove(t, 1), 5, "table.remove(t, 1)")
local sum = 0
table.foreachi(t, function(i, v) sum = sum + i * v end)
eq(sum, 140, "table.foreachi")
local keys = 0
table.foreach({ a = 1, b = 2 }, function(k, v) keys = keys + v end)
eq(keys, 3, "table.foreach")
eq(table.foreach({ 1 }, function() return "stop" end), "stop", "table.foreach result")
raises(function() table.setn(t, 2) end, "'setn' is obsolete", "table.setn")
local sorted = { 3, 1, 2 }
table.sort(sorted)
eq(table.concat(sorted), "123", "table.sort")
table.sort(sorted, function(a, b) return a > b end)
eq(table.concat(sorted), "321", "table.sort with a comparator")
eq(select("#", unpack({ 1, nil, 3 })), 3, "unpack")
eq(select(2, "a", "b", "c"), "b", "select")
local function va(...) return select("#", ...), ... end
eq(va(1, nil), 2, "varargs")
local mt = setmetatable({}, { __index = function(_, k) return k .. "!" end })
eq(mt.hi, "hi!", "__index")
eq(rawget(mt, "hi"), nil, "rawget")
local env = setfenv(function() return fenv_value end, { fenv_value = 9 })
eq(env(), 9, "setfenv")
eq(type(newproxy), "function", "newproxy")

-- Errors.
local ok, err = pcall(error, "boom")
eq(ok, false, "pcall of error")
eq(err, "boom", "pcall error value")
ok, err = pcall(function() error("where") end)
eq(string.match(err, "^selftest:%d+: where$") ~= nil, true, "error position: " .. err)
ok, err = pcall(function() error("nowhere", 0) end)
eq(err, "nowhere", "error level 0")
ok, err = pcall(function() error({ code = 7 }) end)
eq(type(err) == "table" and err.code, 7, "error with a table")
ok, err = pcall(function() local a = nil; return a.b end)
eq(string.find(err, "attempt to index", 1, true) ~= nil, true, "runtime error")
eq(select("#", pcall(function() return 1, 2 end)), 3, "pcall results")
ok, err = xpcall(function() error("inner") end, function(m) return "handled: " .. m end)
eq(string.match(err, "^handled: selftest:%d+: inner$") ~= nil, true, "xpcall handler: " .. err)
ok, err = pcall(pcall)
eq(ok, false, "pcall without a function")
ok, err = pcall(function() return pcall(error, "nested") end)
eq(ok, true, "nested pcall")
eq(err, false, "nested pcall result")
ok, err = loadstring("return +")
eq(ok, nil, "loadstring syntax error")
eq(string.find(err, "unexpected symbol", 1, true) ~= nil, true, "syntax error message")
eq(assert(1, "unused"), 1, "assert passes")
raises(function() assert(false, "failed") end, "failed", "assert fails")
raises(function() return 1 + {} end, "attempt to perform arithmetic", "arithmetic on a table")
raises(function() return #nil end, "attempt to get length", "length of nil")
raises(function() local function f() return f() + 1 end return f() end, "stack overflow", "stack overflow")
raises(function() dofile("absent.lua") end, "cannot open absent.lua", "dofile")

-- Coroutines (Coco without C stacks, #100).
local co = coroutine.create(function(a, b)
  local c = coroutine.yield(a + b)
  local d, e3 = coroutine.yield(c * 2)
  return d + e3
end)
eq(coroutine.status(co), "suspended", "coroutine.status before")
local okr, v = coroutine.resume(co, 1, 2)
eq(okr and v, 3, "resume/yield 1")
okr, v = coroutine.resume(co, 10)
eq(okr and v, 20, "resume/yield 2")
okr, v = coroutine.resume(co, 3, 4)
eq(okr and v, 7, "coroutine return")
eq(coroutine.status(co), "dead", "coroutine.status after")
okr, v = coroutine.resume(co)
eq(okr, false, "resume a dead coroutine")
eq(v, "cannot resume dead coroutine", "dead coroutine message")
local gen = coroutine.wrap(function() for n = 1, 3 do coroutine.yield(n) end end)
eq(gen() + gen() + gen(), 6, "coroutine.wrap")
eq(coroutine.running(), nil, "coroutine.running outside")
local inside
co = coroutine.create(function() inside = coroutine.running(); eq(coroutine.status(inside), "running", "status running") end)
coroutine.resume(co)
eq(inside, co, "coroutine.running inside")
co = coroutine.create(function() error("in coroutine") end)
okr, v = coroutine.resume(co)
eq(okr, false, "error in a coroutine")
eq(string.find(v, "in coroutine", 1, true) ~= nil, true, "coroutine error message")
eq(type(coroutine.cstacksize()), "number", "coroutine.cstacksize")
co = coroutine.create(function() pcall(coroutine.yield, 1) end)
okr, v = coroutine.resume(co)
eq(okr, true, "yield across pcall: resume")
co = coroutine.create(function() local ok2, e4 = pcall(coroutine.yield, 1); error(e4, 0) end)
okr, v = coroutine.resume(co)
eq(v, "attempt to yield across metamethod/C-call boundary", "yield across a C call")
co = coroutine.create(function() table.foreach({ 1 }, function() coroutine.yield() end) end)
okr, v = coroutine.resume(co)
eq(okr, false, "yield across table.foreach")
eq(string.find(v, "attempt to yield across", 1, true) ~= nil, true, "yield across table.foreach message")

-- jit (spec 2.2): mode flags only, nothing compiled.
local function add(a, b) return a + b end
eq(select("#", jit.on()), 0, "jit.on")
eq(add(1, 2), 3, "runs with jit on")
eq(select("#", jit.off()), 0, "jit.off")
eq(add(2, 3), 5, "runs with jit off")
jit.on()
jit.on(add)
jit.off(add, true)
jit.debug(0)
eq(jit.util.status.ENGINE_OFF, 3, "jit.util.status")
eq(jit.compile(add), jit.util.status.ENGINE_OFF, "jit.compile compiles nothing")
eq(jit.compilesub(function() return function() end end), jit.util.status.ENGINE_OFF, "jit.compilesub")
eq(jit.util.stats(add).status, jit.util.status.ENGINE_OFF, "jit.util.stats status")
eq(jit.util.stats(add).params, 2, "jit.util.stats params")
eq(jit.util.stats(add).mcodesize, nil, "no machine code")
eq(jit.util.bytecode(add, 1), "ADD", "jit.util.bytecode")
local function k() return "konst" end
eq(jit.util.const(k, 1), "konst", "jit.util.const")
local up = 5
local function u() return up end
eq(jit.util.upvalue(u, 0), 5, "jit.util.upvalue")
eq(jit.util.closurenup(function() return u end, 0), nil, "jit.util.closurenup out of range")
raises(function() jit.util.mcode(add) end, "jit.util.mcode is unavailable", "jit.util.mcode")
raises(function() jit.util.mcodeaddr(add, 1) end, "jit.util.mcodeaddr is unavailable", "jit.util.mcodeaddr")
raises(function() jit.util.jsubmcode() end, "jit.util.jsubmcode is unavailable", "jit.util.jsubmcode")
raises(function() jit.on(1) end, "Lua function expected", "jit.on(1)")

-- Garbage collection.
collectgarbage()
eq(type(gcinfo()), "number", "gcinfo")
eq(type(collectgarbage("count")), "number", "collectgarbage count")

return failures
