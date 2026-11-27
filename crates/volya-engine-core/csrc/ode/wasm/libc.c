/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The C library functions ODE's code refers to that wasm32 lacks (memory
** comes from Rust, src/c_heap.rs; memcpy, memset, strlen and math from Rust's
** compiler-builtins). They serve its diagnostics only: output is discarded,
** abort and exit trap. Weak, so the script runtime's C library
** (crates/volya-engine-script/csrc/wasm/libc.c), which behaves the same,
** takes their place where it is linked.
*/

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>

#define WEAK __attribute__((weak))

WEAK FILE *const stdout = NULL;
WEAK FILE *const stderr = NULL;

WEAK _Noreturn void abort(void)
{
  __builtin_trap();
}

WEAK _Noreturn void exit(int status)
{
  (void)status;
  __builtin_trap();
}

WEAK int vfprintf(FILE *restrict f, const char *restrict fmt, va_list ap)
{
  (void)f;
  (void)fmt;
  (void)ap;
  return 0;
}

WEAK int fprintf(FILE *restrict f, const char *restrict fmt, ...)
{
  (void)f;
  (void)fmt;
  return 0;
}

WEAK int printf(const char *restrict fmt, ...)
{
  (void)fmt;
  return 0;
}

WEAK int puts(const char *s)
{
  (void)s;
  return 0;
}

WEAK int fputc(int c, FILE *f)
{
  (void)f;
  return (unsigned char)c;
}

WEAK size_t fwrite(const void *restrict p, size_t size, size_t n, FILE *restrict f)
{
  (void)p;
  (void)size;
  (void)f;
  return n;
}

WEAK int fflush(FILE *f)
{
  (void)f;
  return 0;
}

WEAK char *strchr(const char *s, int c)
{
  for (;; s++) {
    if (*s == (char)c) return (char *)s;
    if (!*s) return NULL;
  }
}

WEAK int strncmp(const char *a, const char *b, size_t n)
{
  for (; n && *a && *a == *b; n--, a++, b++) {
  }
  return n ? (unsigned char)*a - (unsigned char)*b : 0;
}

WEAK char *strstr(const char *h, const char *n)
{
  size_t len = strlen(n);
  for (; *h; h++) {
    if (!strncmp(h, n, len)) return (char *)h;
  }
  return len ? NULL : (char *)h;
}

/* C99's: the next float after x towards y (ODE's spaces and heightfields
** widen bounds with it); the rest of ODE's math is compiler-builtins'. */
WEAK float nextafterf(float x, float y)
{
  union { float f; uint32_t u; } v = { x };
  if (x != x || y != y) return x + y;
  if (x == y) return y;
  if (x == 0) {
    v.u = 1; /* The smallest subnormal, with y's sign. */
    return y > 0 ? v.f : -v.f;
  }
  if ((x < y) == (x > 0)) v.u++;
  else v.u--;
  return v.f;
}

/* For ODE's timer (dTimerStart and the like), which the engine doesn't use:
** no clock. */
WEAK int gettimeofday(struct timeval *restrict tv, void *restrict tz)
{
  (void)tz;
  tv->tv_sec = 0;
  tv->tv_usec = 0;
  return 0;
}
