/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** What our C files share of the Rust API's C side (csrc/volya_glue.c): the
** flat values crossing to and from Rust, and calling a Rust function.
*/

#ifndef VOLYA_GLUE_H
#define VOLYA_GLUE_H

#include <stddef.h>

#include "lua.h"

enum { VOLYA_NIL, VOLYA_BOOLEAN, VOLYA_NUMBER, VOLYA_STRING, VOLYA_TABLE };

typedef struct volya_value {
  int kind;
  int boolean;
  double number;
  const char *bytes; /* VOLYA_STRING */
  size_t len;        /* VOLYA_STRING: bytes; VOLYA_TABLE: key-value pairs */
} volya_value;

/* Stack slots reserved before Rust reads values: 2 per table level, up to
** MAX_DEPTH (src/lib.rs) levels. With them, reading pushes without growing
** the stack, so it allocates nothing and can raise nothing. */
#define VOLYA_READ_SLOTS (2 * 64 + 2)

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

/* A Rust function and its data; `f` NULL for none. */
typedef struct volya_rust_fn {
  volya_function f;
  void *data;
} volya_rust_fn;

/* Pushes `count` flat values. */
void volya_push_values(lua_State *L, const volya_value *v, size_t count);

/* Calls `f` with the values at 1..top as its arguments and returns its
** results' count, as a C function does; a Rust error is raised. */
int volya_call_rust(lua_State *L, volya_function f, void *data);

#endif
