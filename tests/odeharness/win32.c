/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** odeharness.exe: runs the scenarios (scenarios.c) against the game's
** bin\ode.dll, loaded from the folder given, through its exports alone, on
** this thread with the floating-point environment each scenario sets and the
** C run time's rand of the run time the library imports (msvcr80.dll, which
** the library loads, found among the process's modules), seeded by its
** srand. Writes results\<scenario>.bin and
** results\info.txt (the library's size) and prints a line per scenario;
** a path in the game's folder is printed as <game folder>\bin\..., and
** nothing names the machine. Exit status 0, or 1 if the library or an export is missing.
**
** Usage: odeharness.exe "<game folder>\bin" [results folder]
**        odeharness.exe compare ours results
** The second form compares our build's outputs (the release's ours\) with
** a run's (compare.c) and prints a line per scenario; exit status 0.
*/

#include "harness.h"

#include <stdio.h>
#include <string.h>
#include <windows.h>
#include <tlhelp32.h>

static void fpenv (uint16_t x87, uint32_t mxcsr)
{
  __asm__ volatile ("fldcw %0" : : "m" (x87));
  __asm__ volatile ("ldmxcsr %0" : : "m" (mxcsr));
}

static void (__cdecl *crt_srand) (unsigned int);

/* The library's C run time, a side-by-side assembly the loader binds
** through the library's manifest, so not necessarily the module our own
** activation context names msvcr80.dll: the process's loaded modules are
** listed and the one named msvcr80.dll taken (our program links the
** Universal C Runtime, so any msvcr80.dll is the library's); of several, the
** one in the game's folder. With none, every module is printed. */
/* `path` as printed: in `bin`, as <game folder>\bin\...; Windows' own
** modules as they are. */
static const char *shown (const char *bin, const char *path)
{
  static char out[MAX_PATH * 2];
  size_t n = strlen (bin);
  while (n && (bin[n - 1] == '\\' || bin[n - 1] == '/')) n--;
  if (n && !_strnicmp (path, bin, n) && (path[n] == '\\' || path[n] == '/' || !path[n])) {
    snprintf (out, sizeof out, "<game folder>\\bin%s", path + n);
    return out;
  }
  return path;
}

static HMODULE find_crt (const char *bin)
{
  HANDLE snap = CreateToolhelp32Snapshot (TH32CS_SNAPMODULE, 0);
  if (snap == INVALID_HANDLE_VALUE) {
    fprintf (stderr, "cannot list the process's modules: error %lu\n", GetLastError ());
    return NULL;
  }
  MODULEENTRY32 m;
  m.dwSize = sizeof m;
  HMODULE found = NULL;
  char found_path[MAX_PATH] = "";
  size_t bin_len = strlen (bin);
  for (BOOL ok = Module32First (snap, &m); ok; ok = Module32Next (snap, &m)) {
    if (_stricmp (m.szModule, "msvcr80.dll")) continue;
    int in_bin = !_strnicmp (m.szExePath, bin, bin_len);
    if (!found || in_bin) {
      found = m.hModule;
      snprintf (found_path, sizeof found_path, "%s", m.szExePath);
      if (in_bin) break;
    }
  }
  if (!found) {
    fprintf (stderr, "no msvcr80.dll among the process's modules after loading the library:\n");
    m.dwSize = sizeof m;
    for (BOOL ok = Module32First (snap, &m); ok; ok = Module32Next (snap, &m))
      fprintf (stderr, "  %s  %s\n", m.szModule, shown (bin, m.szExePath));
  } else {
    printf ("library's run time: %s\n", shown (bin, found_path));
  }
  CloseHandle (snap);
  return found;
}

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

int main (int argc, char **argv)
{
  if (argc == 4 && !strcmp (argv[1], "compare")) {
    volya_ode_compare (argv[2], argv[3], stdout);
    return 0;
  }
  if (argc < 2) {
    fprintf (stderr, "usage: odeharness.exe \"<game folder>\\bin\" [results folder]\n"
	     "       odeharness.exe compare <ours folder> <results folder>\n");
    return 1;
  }
  const char *bin = argv[1], *out = argc > 2 ? argv[2] : "results";
  char path[MAX_PATH * 2];
  SetDllDirectoryA (bin);
  snprintf (path, sizeof path, "%s\\ode.dll", bin);
  HMODULE ode = LoadLibraryA (path);
  if (!ode) {
    fprintf (stderr, "cannot load %s: error %lu\n", shown (bin, path), GetLastError ());
    return 1;
  }
  HMODULE crt = find_crt (bin);
  if (!crt) return 1;
  if (!(crt_srand = (void (__cdecl *) (unsigned int)) GetProcAddress (crt, "srand"))) {
    fprintf (stderr, "msvcr80.dll has no srand export\n");
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
    WIN32_FILE_ATTRIBUTE_DATA a;
    char dll[MAX_PATH * 2];
    snprintf (dll, sizeof dll, "%s\\ode.dll", bin);
    unsigned long size = GetFileAttributesExA (dll, GetFileExInfoStandard, &a) ? a.nFileSizeLow : 0;
    fprintf (info, "library size: %lu\n", size);
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
