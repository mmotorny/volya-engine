/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** ODE 0.5.0's per-machine configuration (the config.h its configurator
** generates), the same on every target: the standard headers, single
** precision, an infinite dInfinity (docs/behavior/physics-library.md 3.1 and
** 4.2: the library's maximum correcting velocity and unbounded rows are
** +inf), no PENTIUM timer code, an unsigned pointer-sized intP.
*/

#ifndef _ODE_CONFIG_H_
#define _ODE_CONFIG_H_

#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <string.h>
#include <stdarg.h>
#include <alloca.h>
#include <float.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef char int8;
typedef unsigned char uint8;
typedef short int16;
typedef unsigned short uint16;
typedef int int32;
typedef unsigned int uint32;

/* An integer type a pointer casts to and from without loss of bits. */
typedef uintptr_t intP;

#define dSINGLE 1

#define dInfinity INFINITY

/* The shipped library's floating-point environment and run time
** (docs/behavior/physics-library.md 7, 8), from volya-engine-core
** (src/physics/ode/x87.rs, msvc_rand.rs): its sines, cosines and arc tangents
** as the x87 instructions give them, and the C run time's per-thread rand.
** Square roots are sqrtf's: the same as fsqrt at 24-bit precision. */
float volya_x87_sinf (float x);
float volya_x87_cosf (float x);
float volya_x87_atan2f (float y, float x);
int volya_ode_rand (void);
#define sinf volya_x87_sinf
#define cosf volya_x87_cosf
#define atan2f volya_x87_atan2f
#define rand volya_ode_rand

#ifdef __cplusplus
}
#endif
#endif
