/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** ODE's dReal in our build (C++ only; C sees a float of the same layout and
** calling convention): a single-precision value whose arithmetic is the
** shipped library's scalar SSE arithmetic under the game's MXCSR,
** flush-to-zero and denormals-are-zero (docs/behavior/physics-library.md 7,
** docs/behavior/physics-harness.md 7), in software, since wasm32 has no such
** mode and the same results are wanted on every target.
**
** - Denormals-are-zero: every operand of an addition, subtraction,
**   multiplication, division or comparison that is a denormal is read as a
**   zero of its sign.
** - Flush-to-zero: every result whose magnitude is below the smallest normal
**   value (2^-126) after rounding to 24 bits with an unbounded exponent (the
**   SSE unit detects tininess after rounding: measured under Rosetta 2, see
**   the unit tests) is a zero of the result's sign. A tiny sum or difference
**   of two floats is exact, so only products, quotients and conversions from
**   double can round up to the smallest normal value from below it.
** - Not arithmetic, so unchanged: copies, negation (a sign change), absolute
**   values, and the x87 functions ODE calls through sqrtf, sinf, cosf and
**   atan2f, which take and give raw floats (docs/behavior/physics-library.md
**   7.2: x87 does not flush).
**
** A dReal compares with a double as C compares a float with one, in double
** (exactly); other arithmetic mixing a dReal with a double does not compile:
** ODE has none once M_PI and M_SQRT1_2 are its own single-precision
** constants (config.h).
**
** Each operation computes its result directly and takes a slow path, out of
** line, only when an operand or the result is a denormal (or, for a product
** or quotient, the result is the smallest normal value), so that in the
** common case the tests are off the arithmetic's dependency chain.
*/

#ifndef _ODE_SSE_H_
#define _ODE_SSE_H_

#ifdef __cplusplus

#include <stdint.h>
#include <string.h>

/* A product or quotient whose exact value is below this in magnitude is tiny
** after rounding: 2^-126 - 2^-151, the midpoint between the smallest normal
** value and the largest value below it at 24-bit precision. */
#define dSSE_TINY (1.1754943508222875e-38 - 3.5032461608120427e-46)

static inline uint32_t dSSEBits (float x)
{
  uint32_t u;
  memcpy (&u,&x,4);
  return u;
}

static inline float dSSEFloat (uint32_t u)
{
  float x;
  memcpy (&x,&u,4);
  return x;
}

/* Twice the bits (the sign dropped), minus 1, unsigned: below 0x00ffffff
** exactly for a denormal, below 0x01000000 for a denormal or the smallest
** normal value; a zero wraps to the largest value. */
static inline uint32_t dSSEClass (float x)
{
  return (dSSEBits (x) << 1) - 1;
}

static inline bool dSSEDenormal (float x)
{
  return dSSEClass (x) < 0x00ffffffu;
}

static inline bool dSSEDenormalOrMin (float x)
{
  return dSSEClass (x) < 0x01000000u;
}

/* A zero of x's sign. */
static inline float dSSEZero (float x)
{
  return dSSEFloat (dSSEBits (x) & 0x80000000u);
}

/* An operand as denormals-are-zero reads it. */
static inline float dSSEOperand (float x)
{
  return dSSEDenormal (x) ? dSSEZero (x) : x;
}

/* A result r flushed: a zero of its sign when tiny. `exact' is a product's
** or quotient's exact value (within double rounding, which for one operation
** of floats decides the same); a tiny sum or difference is exact, so it is
** flushed whenever it is a denormal. */
static inline float dSSEFlush (float r, double exact)
{
  if (dSSEDenormal (r) ||
      (dSSEDenormalOrMin (r) && __builtin_fabs (exact) < dSSE_TINY))
    return dSSEZero (r);
  return r;
}

__attribute__ ((noinline)) static float dSSEAddSlow (float a, float b)
{
  return dSSEFlush (dSSEOperand (a) + dSSEOperand (b),0);
}

__attribute__ ((noinline)) static float dSSESubSlow (float a, float b)
{
  return dSSEFlush (dSSEOperand (a) - dSSEOperand (b),0);
}

__attribute__ ((noinline)) static float dSSEMulSlow (float a, float b)
{
  a = dSSEOperand (a);
  b = dSSEOperand (b);
  return dSSEFlush (a * b,(double) a * (double) b);
}

__attribute__ ((noinline)) static float dSSEDivSlow (float a, float b)
{
  a = dSSEOperand (a);
  b = dSSEOperand (b);
  return dSSEFlush (a / b,(double) a / (double) b);
}

#define dSSE_SLOW(c) __builtin_expect ((c),0)

static inline float dSSEAdd (float a, float b)
{
  float r = a + b;
  if (dSSE_SLOW (dSSEDenormal (a) | dSSEDenormal (b) | dSSEDenormal (r)))
    return dSSEAddSlow (a,b);
  return r;
}

static inline float dSSESub (float a, float b)
{
  float r = a - b;
  if (dSSE_SLOW (dSSEDenormal (a) | dSSEDenormal (b) | dSSEDenormal (r)))
    return dSSESubSlow (a,b);
  return r;
}

static inline float dSSEMul (float a, float b)
{
  float r = a * b;
  if (dSSE_SLOW (dSSEDenormal (a) | dSSEDenormal (b) |
		 dSSEDenormalOrMin (r)))
    return dSSEMulSlow (a,b);
  return r;
}

static inline float dSSEDiv (float a, float b)
{
  float r = a / b;
  if (dSSE_SLOW (dSSEDenormal (a) | dSSEDenormal (b) |
		 dSSEDenormalOrMin (r)))
    return dSSEDivSlow (a,b);
  return r;
}

/* A double converted to single precision (cvtsd2ss), flushed. */
static inline float dSSENarrow (double x)
{
  return dSSEFlush ((float) x,x);
}

struct dRealSSE {
  float v;

  dRealSSE () = default;
  /* a copy: nothing is flushed */
  constexpr dRealSSE (float x) : v (x) {}
  dRealSSE (double x) : v (dSSENarrow (x)) {}
  constexpr dRealSSE (int x) : v ((float) x) {}
  constexpr dRealSSE (unsigned x) : v ((float) x) {}
  constexpr dRealSSE (long x) : v ((float) x) {}
  constexpr dRealSSE (unsigned long x) : v ((float) x) {}

  /* the raw value: what is copied, and what the x87 functions read */
  operator float () const { return v; }
  /* a test against zero, a comparison */
  explicit operator bool () const { return dSSEOperand (v) != 0; }

  dRealSSE operator- () const { return dRealSSE (-v); }
  dRealSSE operator+ () const { return *this; }

  dRealSSE &operator+= (dRealSSE b) { v = dSSEAdd (v,b.v); return *this; }
  dRealSSE &operator-= (dRealSSE b) { v = dSSESub (v,b.v); return *this; }
  dRealSSE &operator*= (dRealSSE b) { v = dSSEMul (v,b.v); return *this; }
  dRealSSE &operator/= (dRealSSE b) { v = dSSEDiv (v,b.v); return *this; }
};

/* Binary operators of two dReals, or of a dReal and a float or an integer
** (converted to single precision first, as C does). */
#define dSSE_BINARY(op,fn) \
  static inline dRealSSE operator op (dRealSSE a, dRealSSE b) \
  { return dRealSSE (fn (a.v,b.v)); } \
  static inline dRealSSE operator op (dRealSSE a, float b) \
  { return dRealSSE (fn (a.v,b)); } \
  static inline dRealSSE operator op (float a, dRealSSE b) \
  { return dRealSSE (fn (a,b.v)); } \
  static inline dRealSSE operator op (dRealSSE a, int b) \
  { return dRealSSE (fn (a.v,(float) b)); } \
  static inline dRealSSE operator op (int a, dRealSSE b) \
  { return dRealSSE (fn ((float) a,b.v)); }

dSSE_BINARY (+,dSSEAdd)
dSSE_BINARY (-,dSSESub)
dSSE_BINARY (*,dSSEMul)
dSSE_BINARY (/,dSSEDiv)

/* Comparisons: of the operands as read, a double one exactly. */
#define dSSE_COMPARE(op,name) \
  __attribute__ ((noinline)) static bool name (float a, float b) \
  { return dSSEOperand (a) op dSSEOperand (b); } \
  static inline bool operator op (dRealSSE a, dRealSSE b) \
  { \
    if (dSSE_SLOW (dSSEDenormal (a.v) | dSSEDenormal (b.v))) \
      return name (a.v,b.v); \
    return a.v op b.v; \
  } \
  static inline bool operator op (dRealSSE a, float b) \
  { return a op dRealSSE (b); } \
  static inline bool operator op (float a, dRealSSE b) \
  { return dRealSSE (a) op b; } \
  static inline bool operator op (dRealSSE a, int b) \
  { return a op dRealSSE (b); } \
  static inline bool operator op (int a, dRealSSE b) \
  { return dRealSSE (a) op b; } \
  static inline bool operator op (dRealSSE a, double b) \
  { return (double) dSSEOperand (a.v) op b; } \
  static inline bool operator op (double a, dRealSSE b) \
  { return a op (double) dSSEOperand (b.v); }

dSSE_COMPARE (<,dSSELessSlow)
dSSE_COMPARE (<=,dSSELessEqualSlow)
dSSE_COMPARE (>,dSSEGreaterSlow)
dSSE_COMPARE (>=,dSSEGreaterEqualSlow)
dSSE_COMPARE (==,dSSEEqualSlow)
dSSE_COMPARE (!=,dSSENotEqualSlow)

#endif /* __cplusplus */
#endif
