/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The C library surface the LuaJIT 1.0.3 interpreter uses, for
** wasm32-unknown-unknown, which has none. Memory, number parsing,
** floating-point digits and math come from Rust (src/libc.rs); memcpy,
** memmove, memset, memcmp and strlen from Rust's compiler-builtins. There
** are no files (fopen fails), stdin is empty and output is discarded.
*/

#include <ctype.h>
#include <errno.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int errno;

/* --- setjmp/longjmp ------------------------------------------------------ */

/* The runtime of LLVM's WebAssembly setjmp/longjmp lowering
** (-mllvm -wasm-enable-sjlj). setjmp records the calling function's
** invocation and a label; longjmp throws the C_LONGJMP tag (__c_longjmp)
** with {env, val}; the function that called setjmp catches it, asks
** __wasm_setjmp_test whether the env is one of its own, and resumes at the
** label with val. */

struct jmp_buf_impl {
  void *invocation;
  uint32_t label;
  struct {
    void *env;
    int val;
  } arg;
};

_Static_assert(sizeof(struct jmp_buf_impl) <= sizeof(jmp_buf), "jmp_buf too small");

void __wasm_setjmp(void *env, uint32_t label, void *invocation)
{
  struct jmp_buf_impl *jb = env;
  if (label == 0 || invocation == NULL) __builtin_trap();
  jb->invocation = invocation;
  jb->label = label;
}

uint32_t __wasm_setjmp_test(void *env, void *invocation)
{
  struct jmp_buf_impl *jb = env;
  if (jb->label == 0 || invocation == NULL) __builtin_trap();
  return jb->invocation == invocation ? jb->label : 0;
}

_Noreturn void __wasm_longjmp(void *env, int val)
{
  struct jmp_buf_impl *jb = env;
  jb->arg.env = env;
  jb->arg.val = val == 0 ? 1 : val;
  __builtin_wasm_throw(1, &jb->arg); /* 1: the C_LONGJMP tag. */
}

/* --- math (the rest comes from Rust's compiler-builtins) ------------------ */

double frexp(double x, int *e)
{
  union { double d; uint64_t u; } v;
  int biased;
  v.d = x;
  biased = (int)(v.u >> 52 & 0x7ff);
  if (biased == 0) {
    if (x == 0) {
      *e = 0;
      return x;
    }
    x = frexp(x * 0x1p64, e); /* Subnormal. */
    *e -= 64;
    return x;
  }
  if (biased == 0x7ff) {
    *e = 0;
    return x;
  }
  *e = biased - 1022;
  v.u = (v.u & 0x800fffffffffffffULL) | 0x3fe0000000000000ULL;
  return v.d;
}

double modf(double x, double *i)
{
  *i = __builtin_trunc(x);
  if (x != x) return x;
  if (x - x != 0) return __builtin_copysign(0.0, x); /* Infinite. */
  return __builtin_copysign(x - *i, x);
}

/* --- stdlib -------------------------------------------------------------- */

double volya_strtod(const char *s, char **end); /* src/libc.rs */

double strtod(const char *s, char **end)
{
  return volya_strtod(s, end);
}

unsigned long strtoul(const char *s, char **end, int base)
{
  const char *p = s;
  unsigned long n = 0;
  int negative = 0, any = 0, overflow = 0;
  while (isspace((unsigned char)*p)) p++;
  if (*p == '+' || *p == '-') negative = *p++ == '-';
  if ((base == 0 || base == 16) && p[0] == '0' && (p[1] == 'x' || p[1] == 'X') &&
      isxdigit((unsigned char)p[2])) {
    p += 2;
    base = 16;
  } else if (base == 0) {
    base = p[0] == '0' ? 8 : 10;
  }
  for (;; p++) {
    int c = (unsigned char)*p, d;
    if (isdigit(c)) d = c - '0';
    else if (isalpha(c)) d = tolower(c) - 'a' + 10;
    else break;
    if (d >= base) break;
    if (n > (~0UL - (unsigned long)d) / (unsigned long)base) overflow = 1;
    else if (!overflow) n = n * (unsigned long)base + (unsigned long)d;
    any = 1;
  }
  if (end != NULL) *end = (char *)(any ? p : s);
  if (overflow) {
    errno = ERANGE;
    return ~0UL;
  }
  return negative ? 0UL - n : n;
}

/* The C standard's example generator. */
static unsigned long next_rand = 1;

int rand(void)
{
  next_rand = next_rand * 1103515245 + 12345;
  return (int)((next_rand / 65536) % 32768);
}

void srand(unsigned seed)
{
  next_rand = seed;
}

int abs(int x)
{
  return x < 0 ? -x : x;
}

_Noreturn void abort(void)
{
  __builtin_trap();
}

_Noreturn void exit(int status)
{
  (void)status;
  __builtin_trap();
}

void *calloc(size_t n, size_t size)
{
  void *p;
  if (size != 0 && n > (size_t)-1 / size) return NULL;
  p = malloc(n * size);
  if (p != NULL) memset(p, 0, n * size);
  return p;
}

/* --- ctype: the "C" locale ----------------------------------------------- */

int isdigit(int c) { return c >= '0' && c <= '9'; }
int islower(int c) { return c >= 'a' && c <= 'z'; }
int isupper(int c) { return c >= 'A' && c <= 'Z'; }
int isalpha(int c) { return islower(c) || isupper(c); }
int isalnum(int c) { return isalpha(c) || isdigit(c); }
int isxdigit(int c) { return isdigit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'); }
int isspace(int c) { return c == ' ' || (c >= '\t' && c <= '\r'); }
int iscntrl(int c) { return (c >= 0 && c < 32) || c == 127; }
int isprint(int c) { return c >= 32 && c < 127; }
int isgraph(int c) { return c > 32 && c < 127; }
int ispunct(int c) { return isgraph(c) && !isalnum(c); }
int tolower(int c) { return isupper(c) ? c - 'A' + 'a' : c; }
int toupper(int c) { return islower(c) ? c - 'a' + 'A' : c; }

/* --- string -------------------------------------------------------------- */

void *memchr(const void *s, int c, size_t n)
{
  const unsigned char *p = s;
  for (; n > 0; n--, p++)
    if (*p == (unsigned char)c) return (void *)p;
  return NULL;
}

char *strchr(const char *s, int c)
{
  for (;; s++) {
    if (*s == (char)c) return (char *)s;
    if (*s == '\0') return NULL;
  }
}

char *strrchr(const char *s, int c)
{
  const char *last = NULL;
  for (;; s++) {
    if (*s == (char)c) last = s;
    if (*s == '\0') return (char *)last;
  }
}

int strcmp(const char *a, const char *b)
{
  while (*a && *a == *b) a++, b++;
  return (unsigned char)*a - (unsigned char)*b;
}

int strncmp(const char *a, const char *b, size_t n)
{
  for (; n > 0; n--, a++, b++) {
    if (*a != *b) return (unsigned char)*a - (unsigned char)*b;
    if (*a == '\0') return 0;
  }
  return 0;
}

/* The "C" locale collates by byte value. */
int strcoll(const char *a, const char *b)
{
  return strcmp(a, b);
}

char *strcpy(char *restrict d, const char *restrict s)
{
  char *r = d;
  while ((*d++ = *s++) != '\0') {}
  return r;
}

char *strncpy(char *restrict d, const char *restrict s, size_t n)
{
  size_t i = 0;
  for (; i < n && s[i] != '\0'; i++) d[i] = s[i];
  for (; i < n; i++) d[i] = '\0';
  return d;
}

char *strcat(char *restrict d, const char *restrict s)
{
  strcpy(d + strlen(d), s);
  return d;
}

char *strncat(char *restrict d, const char *restrict s, size_t n)
{
  char *p = d + strlen(d);
  for (; n > 0 && *s != '\0'; n--) *p++ = *s++;
  *p = '\0';
  return d;
}

size_t strspn(const char *s, const char *accept)
{
  size_t n = 0;
  while (s[n] != '\0' && strchr(accept, s[n]) != NULL) n++;
  return n;
}

size_t strcspn(const char *s, const char *reject)
{
  size_t n = 0;
  while (s[n] != '\0' && strchr(reject, s[n]) == NULL) n++;
  return n;
}

char *strpbrk(const char *s, const char *accept)
{
  s += strcspn(s, accept);
  return *s != '\0' ? (char *)s : NULL;
}

char *strstr(const char *h, const char *n)
{
  size_t len = strlen(n);
  for (; *h != '\0'; h++)
    if (strncmp(h, n, len) == 0) return (char *)h;
  return len == 0 ? (char *)h : NULL;
}

char *strerror(int e)
{
  switch (e) {
  case 0: return "No error";
  case ENOENT: return "No such file or directory";
  case EINVAL: return "Invalid argument";
  case ERANGE: return "Result out of range";
  default: return "Unknown error";
  }
}

/* --- stdio: no files ----------------------------------------------------- */

struct volya_file {
  int unused;
};

static FILE in, out, err;
FILE *stdin = &in, *stdout = &out, *stderr = &err;

FILE *fopen(const char *path, const char *mode)
{
  (void)path;
  (void)mode;
  errno = ENOENT;
  return NULL;
}

int fclose(FILE *f)
{
  (void)f;
  return 0;
}

size_t fread(void *p, size_t size, size_t n, FILE *f)
{
  (void)p, (void)size, (void)n, (void)f;
  return 0;
}

size_t fwrite(const void *p, size_t size, size_t n, FILE *f)
{
  (void)p, (void)size, (void)f;
  return n;
}

int getc(FILE *f)
{
  (void)f;
  return EOF;
}

int ungetc(int c, FILE *f)
{
  (void)c, (void)f;
  return EOF;
}

int feof(FILE *f)
{
  (void)f;
  return 1;
}

int ferror(FILE *f)
{
  (void)f;
  return 0;
}

int fflush(FILE *f)
{
  (void)f;
  return 0;
}

int fputc(int c, FILE *f)
{
  (void)f;
  return (unsigned char)c;
}

int fputs(const char *s, FILE *f)
{
  (void)s, (void)f;
  return 0;
}

int volya_vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap); /* volya_printf.c */

int vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap)
{
  return volya_vsnprintf(buf, cap, fmt, ap);
}

int snprintf(char *buf, size_t cap, const char *fmt, ...)
{
  va_list ap;
  int n;
  va_start(ap, fmt);
  n = volya_vsnprintf(buf, cap, fmt, ap);
  va_end(ap);
  return n;
}

/* Callers size their buffers (Lua's are fixed and large enough). */
int sprintf(char *buf, const char *fmt, ...)
{
  va_list ap;
  int n;
  va_start(ap, fmt);
  n = volya_vsnprintf(buf, (size_t)-1 / 2, fmt, ap);
  va_end(ap);
  return n;
}

int fprintf(FILE *f, const char *fmt, ...)
{
  va_list ap;
  int n;
  (void)f;
  va_start(ap, fmt);
  n = volya_vsnprintf(NULL, 0, fmt, ap);
  va_end(ap);
  return n;
}

int printf(const char *fmt, ...)
{
  va_list ap;
  int n;
  va_start(ap, fmt);
  n = volya_vsnprintf(NULL, 0, fmt, ap);
  va_end(ap);
  return n;
}
