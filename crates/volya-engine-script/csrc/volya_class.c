/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** Script classes deriving from engine classes
** (docs/behavior/script-api-script-classes-deriving-from-engine-classes.md;
** docs/architecture/scripts.md, Script classes): the global `class`,
** construction and `super`, the members of script objects and of class
** objects, `__finalize` and operators, engine classes registered from Rust,
** and the engine's calls into script overrides. Section numbers are the
** spec's.
**
** A class (script or engine) is a userdata whose environment table holds
** its parts: `name`, `members` (what its objects see, 4.1 step 3),
** `defaults` (what `Class.name` reads, 4.4), `getters` and `setters` (engine
** properties), `constants`, `base`, `engine` (its engine type: the engine
** class whose objects it is accepted as, or nil), `engine_class` (whether
** it is an engine class), `metatable` (its objects'), and for an engine
** class `ancestors` (itself and its engine bases, as keys) and `construct`
** (Rust: makes an engine part, returning its id, or nothing). A script
** object is a userdata holding a volya_object, whose environment table
** holds its own fields; its metatable holds its class at OBJECT_KEY.
** Neither environment is reachable from scripts.
*/

#include <string.h>

#include "lua.h"
#include "lauxlib.h"

#include "volya_glue.h"

/* Registry keys and the objects' metatables' class key: addresses. */
static char class_meta_key;    /* The classes' shared metatable. */
static char objects_key;       /* Weak-valued: engine part id -> object. */
static char release_key;       /* Rust: releases an engine part. */
static char operators_key;     /* Operator name -> shared metamethod. */
static char engine_classes_key; /* Engine class name -> class. */
static char object_key;        /* In an object's metatable: its class. */

typedef struct volya_object {
  int ok;       /* `__ok` (4.1): false while an awaited engine part is missing. */
  int has_part;
  double part;  /* The engine part's id (EnginePart, src/classes.rs). */
} volya_object;

static void push_registry(lua_State *L, void *key)
{
  lua_pushlightuserdata(L, key);
  lua_rawget(L, LUA_REGISTRYINDEX);
}

static int absolute(lua_State *L, int idx)
{
  return idx > 0 || idx <= LUA_REGISTRYINDEX ? idx : lua_gettop(L) + idx + 1;
}

static int is_class(lua_State *L, int idx)
{
  int same;
  idx = absolute(L, idx);
  if (lua_type(L, idx) != LUA_TUSERDATA || !lua_getmetatable(L, idx)) return 0;
  push_registry(L, &class_meta_key);
  same = lua_rawequal(L, -1, -2);
  lua_pop(L, 2);
  return same;
}

/* The object at `idx` with its class pushed, or NULL (nothing pushed). An
** object's metatable is its class's: scripts can't give a userdata one. */
static volya_object *to_object(lua_State *L, int idx)
{
  idx = absolute(L, idx);
  if (lua_type(L, idx) != LUA_TUSERDATA || !lua_getmetatable(L, idx)) return NULL;
  lua_pushlightuserdata(L, &object_key);
  lua_rawget(L, -2);
  if (is_class(L, -1)) {
    int same;
    lua_getfenv(L, -1);
    lua_getfield(L, -1, "metatable");
    same = lua_rawequal(L, -1, -4);
    lua_pop(L, 2);
    if (same) {
      lua_remove(L, -2);
      return (volya_object *)lua_touserdata(L, idx);
    }
  }
  lua_pop(L, 2);
  return NULL;
}

/* Whether the class at `idx` is an engine class. */
static int is_engine_class(lua_State *L, int idx)
{
  int engine;
  lua_getfenv(L, idx);
  lua_getfield(L, -1, "engine_class");
  engine = lua_toboolean(L, -1);
  lua_pop(L, 2);
  return engine;
}

/* The class name at `env` for messages. */
static const char *class_name(lua_State *L, int env)
{
  const char *name;
  lua_getfield(L, env, "name");
  name = lua_tostring(L, -1);
  lua_pop(L, 1); /* Still referenced by the environment. */
  return name != NULL ? name : "?";
}

static int is_special(lua_State *L, int idx)
{
  size_t len;
  const char *k;
  if (lua_type(L, idx) != LUA_TSTRING) return 0;
  k = lua_tolstring(L, idx, &len);
  return (len == 6 && memcmp(k, "__init", 6) == 0) ||
         (len == 10 && memcmp(k, "__finalize", 10) == 0);
}

/* Copies table `field` of environment `from` into that of `to`, without
** `__init` and `__finalize` if `skip_special`. */
static void copy_field(lua_State *L, int from, int to, const char *field, int skip_special)
{
  int src, dst;
  lua_getfield(L, from, field);
  src = lua_gettop(L);
  lua_getfield(L, to, field);
  dst = lua_gettop(L);
  lua_pushnil(L);
  while (lua_next(L, src)) {
    if (skip_special && is_special(L, -2)) {
      lua_pop(L, 1);
      continue;
    }
    lua_pushvalue(L, -2);
    lua_insert(L, -2);
    lua_rawset(L, dst);
  }
  lua_pop(L, 2);
}

/* --- Engine parts -------------------------------------------------------- */

/* Releases the object's engine part, if any. */
static void release_part(lua_State *L, volya_object *o)
{
  if (!o->has_part) return;
  o->has_part = 0;
  push_registry(L, &objects_key);
  lua_pushnumber(L, o->part);
  lua_pushnil(L);
  lua_rawset(L, -3);
  lua_pop(L, 1);
  push_registry(L, &release_key);
  lua_pushnumber(L, o->part);
  if (lua_pcall(L, 1, 0, 0) != 0) lua_pop(L, 1);
}

/* Constructs the engine part of object `obj` with engine class `cls`'s
** constructor and the `n` values at 1..n (3.1). */
static void construct_part(lua_State *L, int cls, int obj, int n)
{
  volya_object *o = (volya_object *)lua_touserdata(L, obj);
  int i;
  luaL_checkstack(L, n + 4, "too many arguments");
  lua_getfenv(L, cls);
  lua_getfield(L, -1, "construct");
  for (i = 1; i <= n; i++) lua_pushvalue(L, i);
  lua_call(L, n, 1);
  if (lua_type(L, -1) == LUA_TNUMBER) {
    double part = lua_tonumber(L, -1);
    release_part(L, o); /* Our choice: constructing again replaces the part. */
    o->part = part;
    o->has_part = 1;
    o->ok = 1;
    push_registry(L, &objects_key);
    lua_pushnumber(L, part);
    lua_pushvalue(L, obj);
    lua_rawset(L, -3);
    lua_pop(L, 1);
  }
  lua_pop(L, 2);
}

/* --- Script objects (4.1, 4.2, 7) ---------------------------------------- */

/* Arg 1, which must have the metatable at upvalue `mt`. */
static volya_object *own_object(lua_State *L, int mt)
{
  if (lua_type(L, 1) == LUA_TUSERDATA && lua_getmetatable(L, 1)) {
    int same = lua_rawequal(L, -1, lua_upvalueindex(mt));
    lua_pop(L, 1);
    if (same) return (volya_object *)lua_touserdata(L, 1);
  }
  luaL_error(L, "not an object of this class");
  return NULL;
}

/* `object.key` (4.1). Upvalues: members, getters, the metatable. */
static int object_index(lua_State *L)
{
  volya_object *o = own_object(L, 3);
  lua_settop(L, 2);
  if (lua_type(L, 2) == LUA_TSTRING) {
    size_t len;
    const char *k = lua_tolstring(L, 2, &len);
    if (len == 4 && memcmp(k, "__ok", 4) == 0) {
      lua_pushboolean(L, o->ok);
      return 1;
    }
  }
  lua_getfenv(L, 1);
  lua_pushvalue(L, 2);
  lua_rawget(L, -2);
  if (!lua_isnil(L, -1)) return 1;
  lua_settop(L, 2);
  lua_pushvalue(L, 2);
  lua_rawget(L, lua_upvalueindex(1));
  if (!lua_isnil(L, -1)) return 1;
  lua_pop(L, 1);
  lua_pushvalue(L, 2);
  lua_rawget(L, lua_upvalueindex(2));
  if (lua_isnil(L, -1)) return 1;
  lua_pushvalue(L, 1);
  lua_call(L, 1, 1);
  return 1;
}

/* `object.key = value` (4.2). Upvalues: setters, the metatable. */
static int object_newindex(lua_State *L)
{
  own_object(L, 2);
  lua_settop(L, 3);
  lua_pushvalue(L, 2);
  lua_rawget(L, lua_upvalueindex(1));
  if (!lua_isnil(L, -1)) {
    lua_pushvalue(L, 1);
    lua_pushvalue(L, 3);
    lua_call(L, 2, 0);
    return 0;
  }
  lua_pop(L, 1);
  lua_getfenv(L, 1);
  lua_pushvalue(L, 2);
  lua_pushvalue(L, 3);
  lua_rawset(L, -3);
  return 0;
}

/* Collection (7.1): `__finalize` of the class, then of each script base;
** then the engine part is released (our choice: the object owns it). */
static int object_gc(lua_State *L)
{
  volya_object *o = to_object(L, 1);
  if (o == NULL) return 0;
  while (is_class(L, -1) && !is_engine_class(L, -1)) {
    lua_getfenv(L, -1);
    lua_getfield(L, -1, "members");
    lua_getfield(L, -1, "__finalize");
    if (!lua_isnil(L, -1)) {
      lua_pushvalue(L, 1);
      /* Our choice: an error is discarded. */
      if (lua_pcall(L, 1, 0, 0) != 0) lua_pop(L, 1);
    } else {
      lua_pop(L, 1);
    }
    lua_pop(L, 1);
    lua_getfield(L, -1, "base");
    lua_remove(L, -2);
    lua_remove(L, -2);
  }
  release_part(L, o);
  return 0;
}

/* The 12 operators (7.2), in the order the spec lists them. */
static const char *const operators[] = {
  "__add", "__sub", "__mul", "__div", "__pow", "__lt", "__le", "__eq",
  "__call", "__unm", "__tostring", "__concat",
};

/* An operator; upvalue: its name. Looks it up on the first operand as
** `object.name` does, then on the second, and calls the first found with
** what Lua passed. */
static int object_operator(lua_State *L)
{
  size_t len;
  const char *name = lua_tolstring(L, lua_upvalueindex(1), &len);
  int n = lua_gettop(L), i;
  int binary = strcmp(name, "__call") != 0 && strcmp(name, "__unm") != 0 &&
               strcmp(name, "__tostring") != 0;
  for (i = 1; i <= (binary ? 2 : 1) && i <= n; i++) {
    if (to_object(L, i) == NULL) continue;
    lua_pop(L, 1);
    lua_getfield(L, i, name);
    if (!lua_isnil(L, -1)) {
      lua_insert(L, 1);
      lua_call(L, n, LUA_MULTRET);
      return lua_gettop(L);
    }
    lua_pop(L, 1);
  }
  return luaL_error(L, "No such operator defined");
}

/* --- Classes (1, 2, 3, 4.4) ---------------------------------------------- */

static void new_table_field(lua_State *L, int t, const char *field)
{
  lua_newtable(L);
  lua_setfield(L, t, field);
}

/* Pushes a new class, empty, with its objects' metatable. */
static void new_class(lua_State *L, int engine)
{
  static const char *const tables[] = {"members", "defaults", "getters", "setters", "constants"};
  int cls, env, mt;
  size_t i;
  luaL_checkstack(L, 8, "no stack space for a class");
  lua_newuserdata(L, 0);
  cls = lua_gettop(L);
  push_registry(L, &class_meta_key);
  lua_setmetatable(L, cls);
  lua_createtable(L, 0, 12);
  env = lua_gettop(L);
  lua_pushboolean(L, engine);
  lua_setfield(L, env, "engine_class");
  for (i = 0; i < sizeof(tables) / sizeof(tables[0]); i++) new_table_field(L, env, tables[i]);

  lua_createtable(L, 0, 16);
  mt = lua_gettop(L);
  lua_pushlightuserdata(L, &object_key);
  lua_pushvalue(L, cls);
  lua_rawset(L, mt);
  lua_getfield(L, env, "members");
  lua_getfield(L, env, "getters");
  lua_pushvalue(L, mt);
  lua_pushcclosure(L, object_index, 3);
  lua_setfield(L, mt, "__index");
  lua_getfield(L, env, "setters");
  lua_pushvalue(L, mt);
  lua_pushcclosure(L, object_newindex, 2);
  lua_setfield(L, mt, "__newindex");
  lua_pushcfunction(L, object_gc);
  lua_setfield(L, mt, "__gc");
  push_registry(L, &operators_key);
  for (i = 0; i < sizeof(operators) / sizeof(operators[0]); i++) {
    lua_getfield(L, -1, operators[i]);
    lua_setfield(L, mt, operators[i]);
  }
  lua_pop(L, 1);
  lua_setfield(L, env, "metatable");
  lua_setfenv(L, cls);
}

static int super_call(lua_State *L);

/* Sets the global `super` to the function bound to class `cls` and
** object `obj` (3.1). */
static void set_super(lua_State *L, int cls, int obj)
{
  lua_pushvalue(L, cls);
  lua_pushvalue(L, obj);
  lua_pushcclosure(L, super_call, 2);
  lua_setfield(L, LUA_GLOBALSINDEX, "super");
}

static void clear_super(lua_State *L)
{
  lua_pushnil(L);
  lua_setfield(L, LUA_GLOBALSINDEX, "super");
}

/* `super(args...)` (3.1). Upvalues: the class being initialised, the object. */
static int super_call(lua_State *L)
{
  int n = lua_gettop(L), env, base, benv, i;
  luaL_checkstack(L, n + 8, "too many arguments");
  lua_getfenv(L, lua_upvalueindex(1));
  env = lua_gettop(L);
  lua_getfield(L, env, "base");
  base = lua_gettop(L);
  if (!is_class(L, base)) return luaL_error(L, "class '%s' has no base", class_name(L, env));
  if (is_engine_class(L, base)) {
    lua_pushvalue(L, lua_upvalueindex(2));
    construct_part(L, base, lua_gettop(L), n);
    return 0;
  }
  lua_getfenv(L, base);
  benv = lua_gettop(L);
  lua_getfield(L, benv, "base");
  if (lua_isnil(L, -1)) {
    clear_super(L);
  } else {
    set_super(L, base, lua_upvalueindex(2));
  }
  lua_pop(L, 1);
  lua_getfield(L, benv, "members");
  lua_getfield(L, -1, "__init");
  lua_pushvalue(L, lua_upvalueindex(2));
  for (i = 1; i <= n; i++) lua_pushvalue(L, i);
  lua_call(L, n + 1, 0);
  clear_super(L);
  return 0;
}

/* `Class(args...)` (2.1). */
static int class_call(lua_State *L)
{
  int n = lua_gettop(L), env, obj, i;
  volya_object *o;
  if (!is_class(L, 1)) return luaL_error(L, "not a class");
  luaL_checkstack(L, n + 8, "too many arguments");
  lua_getfenv(L, 1);
  env = lua_gettop(L);
  lua_getfield(L, env, "base");
  o = (volya_object *)lua_newuserdata(L, sizeof *o);
  obj = lua_gettop(L);
  o->ok = lua_isnil(L, -2);
  o->has_part = 0;
  o->part = 0;
  lua_getfield(L, env, "metatable");
  lua_setmetatable(L, obj);
  lua_newtable(L);
  lua_setfenv(L, obj);
  if (!o->ok) set_super(L, 1, obj);
  lua_getfield(L, env, "members");
  lua_getfield(L, -1, "__init");
  lua_pushvalue(L, obj);
  for (i = 2; i <= n; i++) lua_pushvalue(L, i);
  lua_call(L, n, 0);
  lua_pushvalue(L, obj);
  return 1;
}

/* `Class.name` (4.4): a default, else an engine constant, else nil. */
static int class_index(lua_State *L)
{
  if (!is_class(L, 1)) return luaL_error(L, "not a class");
  lua_settop(L, 2);
  lua_getfenv(L, 1);
  lua_getfield(L, 3, "defaults");
  lua_pushvalue(L, 2);
  lua_rawget(L, -2);
  if (!lua_isnil(L, -1)) return 1;
  lua_getfield(L, 3, "constants");
  lua_pushvalue(L, 2);
  lua_rawget(L, -2);
  return 1;
}

/* `Class.name = value` (4.4): a member and a default, without metamethods. */
static int class_newindex(lua_State *L)
{
  if (!is_class(L, 1)) return luaL_error(L, "not a class");
  lua_settop(L, 3);
  lua_getfenv(L, 1);
  lua_getfield(L, 4, "members");
  lua_pushvalue(L, 2);
  lua_pushvalue(L, 3);
  lua_rawset(L, -3);
  lua_getfield(L, 4, "defaults");
  lua_pushvalue(L, 2);
  lua_pushvalue(L, 3);
  lua_rawset(L, -3);
  return 0;
}

/* `class "name" (Base)`: the function `class` returns (1.3). Upvalue: the
** class. */
static int class_derive(lua_State *L)
{
  int cls = lua_upvalueindex(1), env, benv;
  lua_settop(L, 1);
  lua_getfenv(L, cls);
  env = 2;
  /* Our choice where the spec has the outcome unestablished. */
  if (!is_class(L, 1)) {
    return luaL_error(L, "the base of class '%s' is not a class", class_name(L, env));
  }
  lua_getfenv(L, 1);
  benv = 3;
  lua_pushvalue(L, 1);
  lua_setfield(L, env, "base");
  copy_field(L, benv, env, "members", 1);
  copy_field(L, benv, env, "defaults", 1);
  copy_field(L, benv, env, "getters", 0);
  copy_field(L, benv, env, "setters", 0);
  copy_field(L, benv, env, "constants", 0);
  lua_getfield(L, benv, "engine");
  lua_setfield(L, env, "engine");
  return 0;
}

/* The global `class` (1.1). */
static int class_declare(lua_State *L)
{
  lua_Debug ar;
  int cls, target = 0, i;
  const char *local;
  lua_settop(L, 1);
  if (lua_isstring(L, 1)) {
    lua_tostring(L, 1);
  } else {
    lua_pushnil(L);
    lua_replace(L, 1);
  }
  new_class(L, 0);
  cls = 2;
  lua_getfenv(L, cls);
  lua_pushvalue(L, 1);
  lua_setfield(L, -2, "name");
  lua_pop(L, 1);
  /* The caller's first active local named `this`, if it holds a table. */
  if (lua_getstack(L, 1, &ar)) {
    for (i = 1; (local = lua_getlocal(L, &ar, i)) != NULL; i++) {
      if (strcmp(local, "this") == 0) {
        if (lua_istable(L, -1)) {
          target = lua_gettop(L);
        } else {
          lua_pop(L, 1);
        }
        break;
      }
      lua_pop(L, 1);
    }
  }
  if (!target) {
    lua_pushvalue(L, LUA_GLOBALSINDEX);
    target = lua_gettop(L);
  }
  lua_pushvalue(L, 1);
  lua_pushvalue(L, cls);
  lua_settable(L, target);
  lua_pushvalue(L, cls);
  lua_pushcclosure(L, class_derive, 1);
  return 1;
}

/* --- Engine classes ------------------------------------------------------ */

/* Replaces arg 1 with the id of its engine part, raising unless it is an
** object accepted as one of engine class upvalue 3 (1.3) and has its engine
** part. Upvalue 4 names the member for messages (our wording: the spec has
** no message for a failed match, 7.3). */
static void engine_self(lua_State *L)
{
  volya_object *o = to_object(L, 1);
  const char *what = lua_tostring(L, lua_upvalueindex(4));
  int accepted = 0;
  if (o != NULL) {
    lua_getfenv(L, -1);
    lua_getfield(L, -1, "engine");
    if (!lua_isnil(L, -1)) {
      lua_getfenv(L, -1);
      lua_getfield(L, -1, "ancestors");
      lua_pushvalue(L, lua_upvalueindex(3));
      lua_rawget(L, -2);
      accepted = lua_toboolean(L, -1);
      lua_pop(L, 3);
    }
    lua_pop(L, 3);
  }
  if (!accepted) luaL_error(L, "%s: not called on an object of its engine class", what);
  if (!o->has_part) luaL_error(L, "%s: the object has no engine part", what);
  lua_pushnumber(L, o->part);
  lua_replace(L, 1);
}

/* An engine method, property getter or setter on an object. Upvalues: the
** Rust function, its data, the engine class, the member's name. */
static int engine_member(lua_State *L)
{
  engine_self(L);
  return volya_call_rust(L, (volya_function)lua_touserdata(L, lua_upvalueindex(1)),
                         lua_touserdata(L, lua_upvalueindex(2)));
}

/* The default of a method registered without one (6.1). */
static int engine_pure(lua_State *L)
{
  return luaL_error(L, "pure virtual function called");
}

/* A Rust function. Upvalues: the function, its data. */
static int rust_closure(lua_State *L)
{
  return volya_call_rust(L, (volya_function)lua_touserdata(L, lua_upvalueindex(1)),
                         lua_touserdata(L, lua_upvalueindex(2)));
}

static void push_rust(lua_State *L, volya_rust_fn f)
{
  lua_pushlightuserdata(L, (void *)f.f);
  lua_pushlightuserdata(L, f.data);
  lua_pushcclosure(L, rust_closure, 2);
}

static void push_member(lua_State *L, volya_rust_fn f, int cls, const char *cname, const char *name)
{
  lua_pushlightuserdata(L, (void *)f.f);
  lua_pushlightuserdata(L, f.data);
  lua_pushvalue(L, cls);
  lua_pushfstring(L, "%s.%s", cname, name);
  lua_pushcclosure(L, engine_member, 4);
}

typedef struct volya_engine_method {
  const char *name;
  volya_rust_fn call;  /* The member objects see. */
  volya_rust_fn dflt;  /* The default implementation; f NULL: none. */
} volya_engine_method;

typedef struct volya_engine_property {
  const char *name;
  volya_rust_fn get;
  volya_rust_fn set; /* f NULL: read only. */
} volya_engine_property;

typedef struct volya_engine_constant {
  const char *name;
  int value;
} volya_engine_constant;

typedef struct volya_engine_class {
  const char *name;
  const char *base; /* An engine class registered before, or NULL. */
  volya_rust_fn construct;
  const volya_engine_method *methods;
  size_t nmethods;
  const volya_engine_property *properties;
  size_t nproperties;
  const volya_engine_constant *constants;
  size_t nconstants;
} volya_engine_class;

static int register_class(lua_State *L)
{
  const volya_engine_class *c = (const volya_engine_class *)lua_touserdata(L, 1);
  int cls, env, members, defaults, getters, setters, constants;
  size_t i;
  push_registry(L, &engine_classes_key);
  if (lua_isnil(L, -1)) return luaL_error(L, "the class system isn't open");
  lua_pop(L, 1);
  new_class(L, 1);
  cls = lua_gettop(L);
  lua_getfenv(L, cls);
  env = lua_gettop(L);
  lua_pushstring(L, c->name);
  lua_setfield(L, env, "name");
  lua_pushvalue(L, cls);
  lua_setfield(L, env, "engine");
  lua_newtable(L);
  lua_pushvalue(L, cls);
  lua_pushboolean(L, 1);
  lua_rawset(L, -3);
  lua_setfield(L, env, "ancestors");
  push_rust(L, c->construct);
  lua_setfield(L, env, "construct");
  if (c->base != NULL) {
    int benv;
    push_registry(L, &engine_classes_key);
    lua_getfield(L, -1, c->base);
    if (!is_class(L, -1)) {
      return luaL_error(L, "engine class '%s': no engine class '%s'", c->name, c->base);
    }
    lua_getfenv(L, -1);
    benv = lua_gettop(L);
    copy_field(L, benv, env, "members", 0);
    copy_field(L, benv, env, "defaults", 0);
    copy_field(L, benv, env, "getters", 0);
    copy_field(L, benv, env, "setters", 0);
    copy_field(L, benv, env, "constants", 0);
    copy_field(L, benv, env, "ancestors", 0);
    lua_pop(L, 3);
  }
  lua_getfield(L, env, "members");
  members = lua_gettop(L);
  lua_getfield(L, env, "defaults");
  defaults = lua_gettop(L);
  lua_getfield(L, env, "getters");
  getters = lua_gettop(L);
  lua_getfield(L, env, "setters");
  setters = lua_gettop(L);
  lua_getfield(L, env, "constants");
  constants = lua_gettop(L);
  for (i = 0; i < c->nmethods; i++) {
    const volya_engine_method *m = &c->methods[i];
    push_member(L, m->call, cls, c->name, m->name);
    lua_setfield(L, members, m->name);
    if (m->dflt.f != NULL) {
      push_member(L, m->dflt, cls, c->name, m->name);
    } else {
      lua_pushcfunction(L, engine_pure);
    }
    lua_setfield(L, defaults, m->name);
  }
  for (i = 0; i < c->nproperties; i++) {
    const volya_engine_property *p = &c->properties[i];
    push_member(L, p->get, cls, c->name, p->name);
    lua_setfield(L, getters, p->name);
    if (p->set.f != NULL) {
      push_member(L, p->set, cls, c->name, p->name);
      lua_setfield(L, setters, p->name);
    }
  }
  for (i = 0; i < c->nconstants; i++) {
    lua_pushinteger(L, c->constants[i].value);
    lua_setfield(L, constants, c->constants[i].name);
  }
  push_registry(L, &engine_classes_key);
  lua_pushvalue(L, cls);
  lua_setfield(L, -2, c->name);
  lua_pushvalue(L, cls);
  lua_setfield(L, LUA_GLOBALSINDEX, c->name);
  return 0;
}

/* Registers engine class `c`: its class in the globals under its name. */
int volya_register_class_protected(lua_State *L, volya_engine_class *c)
{
  return lua_cpcall(L, register_class, c);
}

/* --- The engine's calls into overrides (5) -------------------------------- */

enum { VOLYA_NOTHING, VOLYA_VALUE, VOLYA_INTEGER };
enum { VOLYA_CALLED, VOLYA_NO_OBJECT, VOLYA_NOT_OVERRIDDEN, VOLYA_FAILED };

typedef struct volya_dispatch {
  double part;
  const char *method;
  const volya_value *args;
  size_t nargs;
  int conversion; /* VOLYA_NOTHING, VOLYA_VALUE or VOLYA_INTEGER */
  int outcome;    /* Set: VOLYA_CALLED ... */
  volya_reader read; /* The result, or the error message. */
  void *ctx;
} volya_dispatch;

static int dispatch(lua_State *L)
{
  volya_dispatch *d = (volya_dispatch *)lua_touserdata(L, 1);
  int obj, env, found, nresults = d->conversion == VOLYA_NOTHING ? 0 : 1;
  luaL_checkstack(L, 16, "no stack space for an engine call");
  /* 5.2 step 1: the object, from the weak reference. */
  push_registry(L, &objects_key);
  lua_pushnumber(L, d->part);
  lua_rawget(L, -2);
  if (to_object(L, -1) == NULL) {
    d->outcome = VOLYA_NO_OBJECT;
    return 0;
  }
  obj = lua_gettop(L) - 1;
  lua_getfenv(L, -1);
  env = lua_gettop(L);
  /* Step 2: the method among the class's members. */
  lua_getfield(L, env, "members");
  lua_getfield(L, -1, d->method);
  found = lua_gettop(L);
  /* Step 3: the engine's own method is its default implementation. */
  lua_getfield(L, env, "engine");
  if (!lua_isnil(L, found) && !lua_isnil(L, -1)) {
    lua_getfenv(L, -1);
    lua_getfield(L, -1, "members");
    lua_getfield(L, -1, d->method);
    if (lua_rawequal(L, -1, found)) {
      d->outcome = VOLYA_NOT_OVERRIDDEN;
      return 0;
    }
  }
  /* Step 4: a protected call without an error handler. */
  lua_pushvalue(L, found);
  lua_pushvalue(L, obj);
  volya_push_values(L, d->args, d->nargs);
  if (lua_pcall(L, (int)d->nargs + 1, nresults, 0) != 0) {
    if (!lua_isstring(L, -1)) {
      lua_pushfstring(L, "(error object is a %s value)", luaL_typename(L, -1));
    } else {
      lua_tostring(L, -1);
    }
    d->outcome = VOLYA_FAILED;
    luaL_checkstack(L, VOLYA_READ_SLOTS, "no stack space to read the error");
    d->read(d->ctx, L, lua_gettop(L), 1);
    return 0;
  }
  d->outcome = VOLYA_CALLED;
  if (d->conversion == VOLYA_INTEGER) {
    /* 5.3: Lua's number conversion, rounded as numbers convert to
    ** integers (luaconf.h, docs/architecture/scripts.md). */
    lua_Integer i = lua_tointeger(L, -1);
    lua_pop(L, 1);
    lua_pushnumber(L, (lua_Number)(int)i);
  }
  luaL_checkstack(L, VOLYA_READ_SLOTS, "no stack space to read the result");
  d->read(d->ctx, L, lua_gettop(L) - nresults + 1, nresults);
  return 0;
}

/* Makes engine call `d` (5.2): 0 with `d->outcome` set, or an error status
** with the error at the top. */
int volya_dispatch_protected(lua_State *L, volya_dispatch *d)
{
  return lua_cpcall(L, dispatch, d);
}

/* --- Opening ------------------------------------------------------------- */

static int open_classes(lua_State *L)
{
  volya_rust_fn *release = (volya_rust_fn *)lua_touserdata(L, 1);
  size_t i;
  lua_pushlightuserdata(L, &class_meta_key);
  lua_createtable(L, 0, 3);
  lua_pushcfunction(L, class_index);
  lua_setfield(L, -2, "__index");
  lua_pushcfunction(L, class_newindex);
  lua_setfield(L, -2, "__newindex");
  lua_pushcfunction(L, class_call);
  lua_setfield(L, -2, "__call");
  lua_rawset(L, LUA_REGISTRYINDEX);

  lua_pushlightuserdata(L, &objects_key);
  lua_newtable(L);
  lua_createtable(L, 0, 1);
  lua_pushliteral(L, "v");
  lua_setfield(L, -2, "__mode");
  lua_setmetatable(L, -2);
  lua_rawset(L, LUA_REGISTRYINDEX);

  lua_pushlightuserdata(L, &engine_classes_key);
  lua_newtable(L);
  lua_rawset(L, LUA_REGISTRYINDEX);

  lua_pushlightuserdata(L, &release_key);
  push_rust(L, *release);
  lua_rawset(L, LUA_REGISTRYINDEX);

  lua_pushlightuserdata(L, &operators_key);
  lua_createtable(L, 0, 12);
  for (i = 0; i < sizeof(operators) / sizeof(operators[0]); i++) {
    lua_pushstring(L, operators[i]);
    lua_pushcclosure(L, object_operator, 1);
    lua_setfield(L, -2, operators[i]);
  }
  lua_rawset(L, LUA_REGISTRYINDEX);

  lua_pushcfunction(L, class_declare);
  lua_setfield(L, LUA_GLOBALSINDEX, "class");
  return 0;
}

/* Opens the class system: the global `class` (1.1); `release` frees an
** engine part, given its id. */
int volya_open_classes_protected(lua_State *L, volya_rust_fn *release)
{
  return lua_cpcall(L, open_classes, release);
}
