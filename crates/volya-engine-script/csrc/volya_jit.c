/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** LuaJIT 1.0.3's JIT engine interface (ljit.h, luajit.h) without a
** machine-code compiler, in place of ljit_core.c, ljit_mem.c,
** ljit_backend.c and ljit_dasm.c (x86 only, and wasm can't run generated
** code). Volya Engine runs the interpreter: the engine's mode flags change
** as LuaJIT's do (jit.on, jit.off, jit.debug and luaJIT_setmode), but
** nothing is ever compiled; every function runs as bytecode with the status
** JIT_S_ENGINE_OFF. See docs/architecture.md (Scripts).
*/

#define LUA_CORE

#include <string.h>

#include "lua.h"

#include "ldo.h"
#include "lmem.h"
#include "lobject.h"
#include "lstate.h"

#include "ljit.h"
#include "luajit.h"

void luaJIT_initstate(lua_State *L)
{
  jit_State *J = luaM_new(L, jit_State);
  memset(J, 0, sizeof(*J)); /* Engine off, no frontend, no backend. */
  G(L)->jit_state = J;
}

void luaJIT_freestate(lua_State *L)
{
  jit_State *J = G(L)->jit_state;
  if (J == NULL) return;
  luaM_free(L, J);
  G(L)->jit_state = NULL;
}

/* Called by luaD_precall for a prototype not yet looked at: mark it as not
** compiled and run it as bytecode. */
int luaJIT_run(lua_State *L, StkId func, int nresults)
{
  clvalue(func)->l.p->jit_status = JIT_S_ENGINE_OFF;
  return luaD_precall(L, func, nresults);
}

/* Only bytecode positions exist. */
int luaJIT_currentpc(lua_State *L, Proto *pt, const Instruction *savedpc)
{
  (void)L;
  return (int)(savedpc - pt->code) - 1;
}

/* jit.compile: Lua functions are never compiled. */
int luaJIT_compile(lua_State *L, int nargs)
{
  StkId func;
  int status = -1;
  lua_lock(L);
  api_check(L, (nargs+1) <= (L->top - L->base));
  func = L->top - (nargs+1);
  if (isLfunction(func)) {
    clvalue(func)->l.p->jit_status = JIT_S_ENGINE_OFF;
    status = JIT_S_ENGINE_OFF;
  }
  lua_unlock(L);
  return status;
}

static void setmode_subs(Proto *pt, int on)
{
  int i;
  for (i = 0; i < pt->sizep; i++) {
    pt->p[i]->jit_status = on ? JIT_S_NONE : JIT_S_OFF;
    setmode_subs(pt->p[i], on);
  }
}

/* The modes of luajit.h, kept as flags and prototype statuses only. */
int luaJIT_setmode(lua_State *L, int idx, int mode)
{
  jit_State *J = G(L)->jit_state;
  int mm = mode & LUAJIT_MODE_MASK;
  int on = (mode & LUAJIT_MODE_ON) != 0;
  switch (mm) {
  case LUAJIT_MODE_ENGINE:
    if (on) J->flags |= JIT_F_ON; else J->flags &= ~JIT_F_ON;
    return 1;
  case LUAJIT_MODE_DEBUG: {
    int dbg = idx == 0 ? 0 :
              idx == 1 ? JIT_F_DEBUG_CALL : JIT_F_DEBUG_CALL|JIT_F_DEBUG_INS;
    J->flags = (J->flags & ~JIT_F_DEBUG) | dbg;
    return 1;
  }
  case LUAJIT_MODE_FUNC:
  case LUAJIT_MODE_ALLFUNC:
  case LUAJIT_MODE_ALLSUBFUNC: {
    StkId func;
    Proto *pt;
    lua_lock(L);
    func = idx == 0 ? (L->ci-1)->func :
           (idx > 0 ? L->base + (idx-1) : L->top + idx);
    if (!isLfunction(func)) {
      lua_unlock(L);
      return 0;
    }
    pt = clvalue(func)->l.p;
    if (mm != LUAJIT_MODE_ALLSUBFUNC)
      pt->jit_status = on ? JIT_S_NONE : JIT_S_OFF;
    if (mm != LUAJIT_MODE_FUNC)
      setmode_subs(pt, on);
    lua_unlock(L);
    return 1;
  }
  default:
    return 0;
  }
}

/* No machine code to free. */
void luaJIT_freeproto(lua_State *L, Proto *pt)
{
  (void)L;
  (void)pt;
}

/* The compiler pipeline's backend (ljitlib.c installs it). The pipeline
** never runs: the frontend is only started by compiling. */
int luaJIT_backend(lua_State *L)
{
  lua_pushinteger(L, JIT_S_ENGINE_OFF);
  return 1;
}
