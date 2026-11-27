/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The scenarios (scenarios.c) against our build, linked into
** volya-engine-core natively (its build.rs): the library's functions
** directly, the run time's rand ours (physics::ode::msvc_rand), the x87
** instructions our emulation (physics::ode::x87) and sqrtf. The environment
** is not set: our build computes as the shipped library's does in it.
*/

#include "harness.h"

#include <math.h>
#include <string.h>

#define DECLARE(ret, name, params) ret name params;
VOLYA_ODE_API (DECLARE)

float volya_x87_sinf (float x);
float volya_x87_cosf (float x);
float volya_x87_atan2f (float y, float x);
void volya_ode_srand (uint32_t seed);

static void fpenv (uint16_t x87, uint32_t mxcsr)
{
  (void) x87;
  (void) mxcsr;
}

static float x87 (int op, float y, float x)
{
  switch (op) {
  case 0: return sqrtf (x);
  case 1: return volya_x87_sinf (x);
  case 2: return volya_x87_cosf (x);
  default: return volya_x87_atan2f (y, x);
  }
}

/* runs scenario i, its records to `record'; the scenarios' count when i is
** out of range */
int volya_ode_harness (int i, void (*record) (void *, uint32_t, const uint32_t *, uint32_t),
		       void *ctx)
{
  if (i < 0 || i >= volya_ode_scenario_count ()) return volya_ode_scenario_count ();
  OdeHarness h;
  memset (&h, 0, sizeof h);
#define SET(ret, name, params) h.name = name;
  VOLYA_ODE_API (SET)
  h.fpenv = fpenv;
  h.srand = volya_ode_srand;
  h.x87 = x87;
  h.record = record;
  h.ctx = ctx;
  volya_ode_scenario_run (&h, i);
  return volya_ode_scenario_count ();
}

const char *volya_ode_harness_name (int i)
{
  return volya_ode_scenario_name (i);
}

/* compare.c's comparison, its lines written to the file `report' */
int volya_ode_harness_compare (const char *ours, const char *shipped, const char *report)
{
  FILE *f = fopen (report, "w");
  if (!f) return -1;
  int n = volya_ode_compare (ours, shipped, f);
  fclose (f);
  return n;
}
