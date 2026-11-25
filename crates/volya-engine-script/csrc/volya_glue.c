/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The C side of the Rust API (src/lib.rs). Lua raises errors with longjmp,
** which must never cross Rust frames, so every Lua API call that can raise
** runs here, in C, inside lua_cpcall. Rust calls in only through these
** entry points and is called back only to read values, which raises
** nothing (the allocator never fails: it aborts). Values cross as a flat
** array in prefix order: a table is followed by its keys and values.
*/

#include <stddef.h>
#include <stdlib.h>

#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

LUALIB_API int luaopen_coco(lua_State *L); /* lcoco.h needs lstate.h. */

enum { VOLYA_NIL, VOLYA_BOOLEAN, VOLYA_NUMBER, VOLYA_STRING, VOLYA_TABLE };

typedef struct volya_value {
  int kind;
  int boolean;
  double number;
  const char *bytes; /* VOLYA_STRING */
  size_t len;        /* VOLYA_STRING: bytes; VOLYA_TABLE: key-value pairs */
} volya_value;

/* Reads `count` values from the stack, from index `first`. */
typedef void (*volya_reader)(void *ctx, lua_State *L, int first, int count);

/* A Rust function's results or error, owned by Rust until released. */
typedef struct volya_reply {
  const volya_value *values;
  size_t count; /* Top-level values. */
  const char *error;
  size_t error_len;
  void *token;
  void (*release)(void *token);
} volya_reply;

/* Calls a Rust function with its arguments at 1..top; fills `reply`. */
typedef void (*volya_function)(void *data, lua_State *L, volya_reply *reply);

static const volya_value *push_value(lua_State *L, const volya_value *v)
{
  switch (v->kind) {
  case VOLYA_BOOLEAN: lua_pushboolean(L, v->boolean); return v + 1;
  case VOLYA_NUMBER: lua_pushnumber(L, v->number); return v + 1;
  case VOLYA_STRING: lua_pushlstring(L, v->bytes, v->len); return v + 1;
  case VOLYA_TABLE: {
    size_t i, pairs = v->len;
    luaL_checkstack(L, 3, "table too deep");
    lua_createtable(L, 0, pairs > 1000000 ? 0 : (int)pairs);
    v++;
    for (i = 0; i < pairs; i++) {
      v = push_value(L, v);
      v = push_value(L, v);
      lua_rawset(L, -3); /* Raises for a nil or NaN key. */
    }
    return v;
  }
  default: lua_pushnil(L); return v + 1;
  }
}

static void push_values(lua_State *L, const volya_value *v, size_t count)
{
  size_t i;
  luaL_checkstack(L, count > 1000000 ? 1000000 : (int)count, "too many values");
  for (i = 0; i < count; i++) v = push_value(L, v);
}

static void abort_panic(void)
{
  abort();
}

static int panic(lua_State *L)
{
  (void)L;
  abort_panic(); /* Unreachable: every call is protected. */
  return 0;
}

/* --- State --------------------------------------------------------------- */

static int unavailable(lua_State *L)
{
  return luaL_error(L, "%s is unavailable: scripts run on the interpreter only",
                    lua_tostring(L, lua_upvalueindex(1)));
}

/* jit.util functions that report machine code, which never exists. */
static const char *const no_mcode[] = {"mcode", "mcodeaddr", "jsubmcode"};

static int open_libs(lua_State *L)
{
  /* The libraries the original opens by default
  ** (docs/behavior/script-environment.md 2.1), opened as linit.c does. */
  static const luaL_Reg libs[] = {
    {"", luaopen_base},
    {LUA_TABLIBNAME, luaopen_table},
    {LUA_STRLIBNAME, luaopen_string},
    {LUA_MATHLIBNAME, luaopen_math},
    {LUA_JITLIBNAME, luaopen_jit},
    {LUA_COLIBNAME, luaopen_coco},
    {NULL, NULL},
  };
  const luaL_Reg *lib;
  size_t i;
  for (lib = libs; lib->func; lib++) {
    lua_pushcfunction(L, lib->func);
    lua_pushstring(L, lib->name);
    lua_call(L, 1, 0);
  }
  lua_getglobal(L, LUA_JITLIBNAME);
  lua_getfield(L, -1, "util");
  for (i = 0; i < sizeof(no_mcode) / sizeof(no_mcode[0]); i++) {
    lua_pushfstring(L, "jit.util.%s", no_mcode[i]);
    lua_pushcclosure(L, unavailable, 1);
    lua_setfield(L, -2, no_mcode[i]);
  }
  lua_pop(L, 2);
  return 0;
}

/* A new state with the original's default libraries open, or NULL. */
lua_State *volya_newstate(lua_Alloc alloc, void *ud)
{
  lua_State *L = lua_newstate(alloc, ud);
  if (L == NULL) return NULL;
  lua_atpanic(L, panic);
  if (lua_cpcall(L, open_libs, NULL) != 0) {
    lua_close(L);
    return NULL;
  }
  return L;
}

/* --- Running chunks and functions ---------------------------------------- */

typedef struct volya_run {
  const char *code; /* A chunk to load, or NULL to call `global`. */
  size_t len;
  const char *name; /* The chunk's name. */
  const char *global;
  int call; /* 0: only compile the chunk. */
  int load_status; /* Set when loading fails. */
  const volya_value *args;
  size_t nargs;
  volya_reader read;
  void *ctx;
} volya_run;

static int run(lua_State *L)
{
  volya_run *r = (volya_run *)lua_touserdata(L, 1);
  int base = lua_gettop(L);
  if (r->code != NULL) {
    r->load_status = luaL_loadbuffer(L, r->code, r->len, r->name);
    if (r->load_status != 0) lua_error(L);
    if (!r->call) return 0;
  } else {
    lua_getglobal(L, r->global);
  }
  push_values(L, r->args, r->nargs);
  lua_call(L, (int)r->nargs, LUA_MULTRET);
  r->read(r->ctx, L, base + 1, lua_gettop(L) - base);
  return 0;
}

/* Runs `r` protected: 0, or an error status with the error at the top. */
int volya_run_protected(lua_State *L, volya_run *r)
{
  return lua_cpcall(L, run, r);
}

/* --- Rust functions ------------------------------------------------------ */

static int trampoline(lua_State *L)
{
  volya_function f = (volya_function)lua_touserdata(L, lua_upvalueindex(1));
  void *data = lua_touserdata(L, lua_upvalueindex(2));
  volya_reply reply = {NULL, 0, NULL, 0, NULL, NULL};
  int top = lua_gettop(L);
  f(data, L, &reply);
  /* No Rust frames from here on. If a push raises, the reply leaks. */
  if (reply.error != NULL) {
    lua_pushlstring(L, reply.error, reply.error_len);
    reply.release(reply.token);
    return lua_error(L);
  }
  push_values(L, reply.values, reply.count);
  reply.release(reply.token);
  return lua_gettop(L) - top;
}

typedef struct volya_register {
  const char *name;
  volya_function f;
  void *data;
} volya_register;

static int set_function(lua_State *L)
{
  volya_register *r = (volya_register *)lua_touserdata(L, 1);
  lua_pushlightuserdata(L, (void *)r->f);
  lua_pushlightuserdata(L, r->data);
  lua_pushcclosure(L, trampoline, 2);
  lua_setglobal(L, r->name);
  return 0;
}

/* Sets the global `name` to a function calling `f` with `data`. */
int volya_register_protected(lua_State *L, volya_register *r)
{
  return lua_cpcall(L, set_function, r);
}
