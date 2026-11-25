/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** A C99 vsnprintf for our wasm C library (csrc/wasm/libc.c): flags, width,
** precision, the length modifiers hh, h, l, ll, z, t and j, and the
** conversions d i u o x X c s p e E f F g G %. Floating-point digits come
** from Rust (volya_format_double, src/libc.rs). Built on every target so
** native tests can compare it with the system's.
*/

#include <stdarg.h>
#include <stddef.h>

/* The digits of |v| for conversion `conv` (e f g, any case) and precision
** `prec` (>= 0), with '#' if `alt`; returns the length (may exceed cap). */
size_t volya_format_double(char *out, size_t cap, double v, int conv, int prec, int alt);

typedef struct {
  char *buf;
  size_t cap;
  size_t len;
} sink;

static void put(sink *s, char c)
{
  if (s->len + 1 < s->cap) s->buf[s->len] = c;
  s->len++;
}

static void put_n(sink *s, const char *p, size_t n)
{
  while (n--) put(s, *p++);
}

static void pad(sink *s, char c, int n)
{
  while (n-- > 0) put(s, c);
}

enum { F_MINUS = 1, F_PLUS = 2, F_SPACE = 4, F_HASH = 8, F_ZERO = 16 };

/* Writes a converted field: sign/prefix, zeros, body, aligned to width. */
static void field(sink *s, int flags, int width, const char *prefix, size_t plen,
                  int zeros, const char *body, size_t blen)
{
  int total = (int)(plen + blen) + zeros;
  if (!(flags & F_MINUS) && !(flags & F_ZERO)) pad(s, ' ', width - total);
  put_n(s, prefix, plen);
  if (!(flags & F_MINUS) && (flags & F_ZERO)) pad(s, '0', width - total);
  pad(s, '0', zeros);
  put_n(s, body, blen);
  if (flags & F_MINUS) pad(s, ' ', width - total);
}

static void integer(sink *s, int flags, int width, int prec, char conv,
                    unsigned long long u, int negative)
{
  char digits[64], prefix[3];
  const char *set = conv == 'X' ? "0123456789ABCDEF" : "0123456789abcdef";
  unsigned base = (conv == 'o') ? 8 : (conv == 'x' || conv == 'X' || conv == 'p') ? 16 : 10;
  size_t n = 0, plen = 0;
  int zeros;
  while (u != 0) {
    digits[sizeof(digits) - 1 - n++] = set[u % base];
    u /= base;
  }
  if (negative) prefix[plen++] = '-';
  else if (conv == 'd' || conv == 'i') {
    if (flags & F_PLUS) prefix[plen++] = '+';
    else if (flags & F_SPACE) prefix[plen++] = ' ';
  }
  if (conv == 'p' || ((flags & F_HASH) && (conv == 'x' || conv == 'X') && n > 0)) {
    prefix[plen++] = '0';
    prefix[plen++] = conv == 'X' ? 'X' : 'x';
  }
  if (prec < 0) prec = 1;
  else flags &= ~F_ZERO; /* A precision overrides '0'. */
  zeros = prec > (int)n ? prec - (int)n : 0;
  if (conv == 'o' && (flags & F_HASH) && zeros == 0 &&
      (n == 0 || digits[sizeof(digits) - n] != '0'))
    zeros = 1;
  field(s, flags, width, prefix, plen, zeros, digits + sizeof(digits) - n, n);
}

static void floating(sink *s, int flags, int width, int prec, char conv, double v)
{
  char body[512], prefix[1];
  size_t plen = 0, blen;
  int finite = v - v == 0;
  if (v != v) {} /* NaN: no sign (as the system libraries we compare with). */
  else if (__builtin_signbit(v)) prefix[plen++] = '-';
  else if (flags & F_PLUS) prefix[plen++] = '+';
  else if (flags & F_SPACE) prefix[plen++] = ' ';
  if (!finite) flags &= ~F_ZERO;
  if (prec < 0) prec = 6;
  blen = volya_format_double(body, sizeof(body), v, conv, prec, (flags & F_HASH) != 0);
  if (blen > sizeof(body)) blen = sizeof(body);
  field(s, flags, width, prefix, plen, 0, body, blen);
}

int volya_vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap)
{
  sink s;
  s.buf = buf;
  s.cap = cap;
  s.len = 0;
  while (*fmt) {
    int flags = 0, width = 0, prec = -1, length = 0;
    char conv;
    if (*fmt != '%') {
      put(&s, *fmt++);
      continue;
    }
    fmt++;
    for (;; fmt++) {
      if (*fmt == '-') flags |= F_MINUS;
      else if (*fmt == '+') flags |= F_PLUS;
      else if (*fmt == ' ') flags |= F_SPACE;
      else if (*fmt == '#') flags |= F_HASH;
      else if (*fmt == '0') flags |= F_ZERO;
      else break;
    }
    if (*fmt == '*') {
      width = va_arg(ap, int);
      if (width < 0) {
        flags |= F_MINUS;
        width = -width;
      }
      fmt++;
    } else {
      while (*fmt >= '0' && *fmt <= '9') width = width * 10 + (*fmt++ - '0');
    }
    if (*fmt == '.') {
      fmt++;
      prec = 0;
      if (*fmt == '*') {
        prec = va_arg(ap, int);
        fmt++;
      } else {
        while (*fmt >= '0' && *fmt <= '9') prec = prec * 10 + (*fmt++ - '0');
      }
    }
    if (flags & F_MINUS) flags &= ~F_ZERO;
    if (flags & F_PLUS) flags &= ~F_SPACE;
    for (;; fmt++) {
      if (*fmt == 'h') length = length == -1 ? -2 : -1;
      else if (*fmt == 'l') length = length == 1 ? 2 : 1;
      else if (*fmt == 'z' || *fmt == 't' || *fmt == 'j') length = 3;
      else break;
    }
    conv = *fmt;
    if (conv == '\0') break;
    fmt++;
    switch (conv) {
    case 'd':
    case 'i': {
      long long v;
      if (length == 2 || length == 3) v = length == 2 ? va_arg(ap, long long) : (long long)va_arg(ap, ptrdiff_t);
      else if (length == 1) v = va_arg(ap, long);
      else v = va_arg(ap, int);
      if (length == -1) v = (short)v;
      if (length == -2) v = (signed char)v;
      integer(&s, flags, width, prec, conv,
              v < 0 ? 0ULL - (unsigned long long)v : (unsigned long long)v, v < 0);
      break;
    }
    case 'u':
    case 'o':
    case 'x':
    case 'X': {
      unsigned long long v;
      if (length == 2) v = va_arg(ap, unsigned long long);
      else if (length == 3) v = va_arg(ap, size_t);
      else if (length == 1) v = va_arg(ap, unsigned long);
      else v = va_arg(ap, unsigned int);
      if (length == -1) v = (unsigned short)v;
      if (length == -2) v = (unsigned char)v;
      integer(&s, flags, width, prec, conv, v, 0);
      break;
    }
    case 'p':
      integer(&s, flags & F_MINUS, width, -1, 'p', (unsigned long long)(size_t)va_arg(ap, void *), 0);
      break;
    case 'c': {
      char c = (char)va_arg(ap, int);
      field(&s, flags & F_MINUS, width, "", 0, 0, &c, 1);
      break;
    }
    case 's': {
      const char *str = va_arg(ap, const char *);
      size_t n = 0;
      if (str == NULL) str = "(null)";
      while (str[n] && (prec < 0 || n < (size_t)prec)) n++;
      field(&s, flags & F_MINUS, width, "", 0, 0, str, n);
      break;
    }
    case 'e':
    case 'E':
    case 'f':
    case 'F':
    case 'g':
    case 'G':
      floating(&s, flags, width, prec, conv, va_arg(ap, double));
      break;
    case '%':
      put(&s, '%');
      break;
    default: /* Unknown: written as it is. */
      put(&s, '%');
      put(&s, conv);
      break;
    }
  }
  if (cap > 0) s.buf[s.len < cap ? s.len : cap - 1] = '\0';
  return (int)s.len;
}

int volya_snprintf(char *buf, size_t cap, const char *fmt, ...)
{
  va_list ap;
  int n;
  va_start(ap, fmt);
  n = volya_vsnprintf(buf, cap, fmt, ap);
  va_end(ap);
  return n;
}
