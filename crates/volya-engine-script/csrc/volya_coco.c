/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** Coco's C API (lcoco.h) without C stacks, in place of lcoco.c, whose
** context switches (stack switching with setjmp, ucontext or fibers) can't
** be done in WebAssembly. The coroutine library Coco installs (lcocolib.c)
** is kept, so coroutine.create, resume, yield, wrap and cstacksize and
** coroutine.coco exist; coroutines are Lua 5.1's own, so a yield across a
** C call (a metamethod, pcall, an iterator or a function registered from
** Rust) raises "attempt to yield across metamethod/C-call boundary" where
** Coco would switch stacks (#100). See docs/architecture.md (Scripts).
*/

#define LUA_CORE

#include "lua.h"

#include "lstate.h" /* Includes lcoco.h. */

/* LuaJIT 1.0.3's Coco defaults (lcoco.c). */
#define DEFAULT_CSTACKSIZE (65536-4096)
#define MIN_CSTACKSIZE 4096

static int defaultcstacksize = DEFAULT_CSTACKSIZE;

/* Every coroutine is a plain Lua thread: the size is not used. */
lua_State *lua_newcthread(lua_State *L, int cstacksize)
{
  (void)cstacksize;
  return lua_newthread(L);
}

/* Threads never get a C stack (LHASCOCO is false), so lstate.c and ldo.c
** never call these. */
void luaCOCO_free(lua_State *L)
{
  (void)L;
}

int luaCOCO_resume(lua_State *L, int nargs)
{
  (void)nargs;
  lua_pushliteral(L, "coroutine has a C stack");
  return LUA_ERRRUN;
}

int luaCOCO_yield(lua_State *L, int nresults)
{
  (void)L;
  (void)nresults;
  return -1;
}

/* coroutine.cstacksize: the setting is kept as Coco keeps it, and returned. */
int luaCOCO_cstacksize(int cstacksize)
{
  int old = defaultcstacksize;
  if (cstacksize >= 0) {
    if (cstacksize == 0)
      cstacksize = DEFAULT_CSTACKSIZE;
    else if (cstacksize < MIN_CSTACKSIZE)
      cstacksize = MIN_CSTACKSIZE;
    defaultcstacksize = cstacksize;
  }
  return old;
}
