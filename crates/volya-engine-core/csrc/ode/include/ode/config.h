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

#ifdef __cplusplus
}
#endif
#endif
