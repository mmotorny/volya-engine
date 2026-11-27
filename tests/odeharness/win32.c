/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** odeharness.exe: runs the scenarios (scenarios.c) against the game's
** bin\ode.dll, loaded from the folder given, through its exports alone, on
** this thread with the floating-point environment each scenario sets and the
** C run time's rand of the run time the library imports (msvcr80.dll, which
** the library loads), seeded by its srand. Writes results\<scenario>.bin and
** results\info.txt (the processor, the library's size) and prints a line per
** scenario. Exit status 0, or 1 if the library or an export is missing.
**
** Usage: odeharness.exe "<game folder>\bin" [results folder]
*/

#include "harness.h"

#include <stdio.h>
#include <string.h>
#include <windows.h>

static void fpenv (uint16_t x87, uint32_t mxcsr)
{
  __asm__ volatile ("fldcw %0" : : "m" (x87));
  __asm__ volatile ("ldmxcsr %0" : : "m" (mxcsr));
}

static void (__cdecl *crt_srand) (unsigned int);

static void seed_rand (uint32_t s)
{
  crt_srand (s);
}

static float x87 (int op, float y, float x)
{
  float r;
  switch (op) {
  case 0:
    __asm__ volatile ("flds %1\n\tfsqrt\n\tfstps %0" : "=m" (r) : "m" (x) : "st");
    break;
  case 1:
    __asm__ volatile ("flds %1\n\tfsin\n\tfstps %0" : "=m" (r) : "m" (x) : "st");
    break;
  case 2:
    __asm__ volatile ("flds %1\n\tfcos\n\tfstps %0" : "=m" (r) : "m" (x) : "st");
    break;
  default:
    /* fpatan: atan(st1 / st0), popping; y pushed first */
    __asm__ volatile ("flds %1\n\tflds %2\n\tfpatan\n\tfstps %0"
		      : "=m" (r) : "m" (y), "m" (x) : "st", "st(1)");
    break;
  }
  return r;
}

static void record (void *ctx, uint32_t tag, const uint32_t *words, uint32_t n)
{
  FILE *f = (FILE *) ctx;
  fwrite (&tag, 4, 1, f);
  fwrite (&n, 4, 1, f);
  fwrite (words, 4, n, f);
}

static void cpu_name (char *out)
{
  unsigned int r[12];
  for (unsigned int i = 0; i < 3; i++) {
    __asm__ volatile ("cpuid"
		      : "=a" (r[i * 4]), "=b" (r[i * 4 + 1]), "=c" (r[i * 4 + 2]), "=d" (r[i * 4 + 3])
		      : "a" (0x80000002u + i));
  }
  memcpy (out, r, 48);
  out[48] = 0;
}

int main (int argc, char **argv)
{
  if (argc < 2) {
    fprintf (stderr, "usage: odeharness.exe \"<game folder>\\bin\" [results folder]\n");
    return 1;
  }
  const char *bin = argv[1], *out = argc > 2 ? argv[2] : "results";
  char path[MAX_PATH * 2];
  SetDllDirectoryA (bin);
  snprintf (path, sizeof path, "%s\\ode.dll", bin);
  HMODULE ode = LoadLibraryA (path);
  if (!ode) {
    fprintf (stderr, "cannot load %s: error %lu\n", path, GetLastError ());
    return 1;
  }
  HMODULE crt = GetModuleHandleA ("msvcr80.dll");
  if (!crt || !(crt_srand = (void (__cdecl *) (unsigned int)) GetProcAddress (crt, "srand"))) {
    fprintf (stderr, "msvcr80.dll's srand not found (the library's run time)\n");
    return 1;
  }

  OdeHarness h;
  memset (&h, 0, sizeof h);
  int missing = 0;
#define LOAD(ret, name, params) \
  if (!(h.name = (ret (*) params) (void *) GetProcAddress (ode, #name))) { \
    fprintf (stderr, "export missing: %s\n", #name); \
    missing = 1; \
  }
  VOLYA_ODE_API (LOAD)
  if (missing) return 1;
  h.fpenv = fpenv;
  h.srand = seed_rand;
  h.x87 = x87;
  h.record = record;

  CreateDirectoryA (out, NULL);
  snprintf (path, sizeof path, "%s\\info.txt", out);
  FILE *info = fopen (path, "wb");
  if (info) {
    char cpu[49];
    cpu_name (cpu);
    WIN32_FILE_ATTRIBUTE_DATA a;
    char dll[MAX_PATH * 2];
    snprintf (dll, sizeof dll, "%s\\ode.dll", bin);
    unsigned long size = GetFileAttributesExA (dll, GetFileExInfoStandard, &a) ? a.nFileSizeLow : 0;
    fprintf (info, "processor: %s\nlibrary size: %lu\n", cpu, size);
    fclose (info);
  }

  for (int i = 0; i < volya_ode_scenario_count (); i++) {
    snprintf (path, sizeof path, "%s\\%s.bin", out, volya_ode_scenario_name (i));
    FILE *f = fopen (path, "wb");
    if (!f) {
      fprintf (stderr, "cannot write %s\n", path);
      return 1;
    }
    h.ctx = f;
    volya_ode_scenario_run (&h, i);
    long bytes = ftell (f);
    fclose (f);
    /* back to the run time's default between scenarios */
    fpenv (0x037F, 0x1F80);
    printf ("%s: %ld bytes\n", volya_ode_scenario_name (i), bytes);
  }
  return 0;
}
