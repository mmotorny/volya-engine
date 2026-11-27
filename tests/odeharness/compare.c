/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** `odeharness.exe compare <ours> <shipped>`: every scenario's output from
** our build (the release's ours\) against the shipped library's (a run's
** results\), record by record. Per scenario: identical, or how many words
** differ, by record tag, and the first difference (record, tag, word, both
** words); any two NaNs count as equal (the shipped library's NaNs are the
** x87's and SSE's default NaN, ours the platform's). Then the shipped
** library's runs with and without denormals-are-zero against each other
** (#769): every <name>-nodaz scenario against <name>. Plain C, also built natively for its test.
*/

#include "harness.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TAG_NAME(value, name) [value] = #name,
static const char *TAGS[] = {[0] = "?", VOLYA_ODE_TAGS (TAG_NAME)};

static const char *tag_name (uint32_t tag)
{
  uint32_t t = tag & 0xFF;
  return t < sizeof TAGS / sizeof TAGS[0] && TAGS[t] ? TAGS[t] : "?";
}

/* the file's words (little-endian; both machines are), or NULL */
static uint32_t *load (const char *dir, const char *name, size_t *n)
{
  char path[1024];
  snprintf (path, sizeof path, "%s/%s.bin", dir, name);
  FILE *f = fopen (path, "rb");
  if (!f) return NULL;
  fseek (f, 0, SEEK_END);
  long bytes = ftell (f);
  fseek (f, 0, SEEK_SET);
  uint32_t *w = (uint32_t *) malloc ((size_t) bytes + 4);
  *n = fread (w, 4, (size_t) bytes / 4, f);
  fclose (f);
  return w;
}

static int is_nan (uint32_t w)
{
  return (w & 0x7F800000u) == 0x7F800000u && (w & 0x007FFFFFu) != 0;
}

/* writes one line about a against b; 0 when identical */
static int compare_words (FILE *out, const char *label, const char *an, const uint32_t *a,
			  size_t na, const char *bn, const uint32_t *b, size_t nb)
{
  size_t i = 0, record = 0, compared = 0, differing = 0;
  size_t by_tag[sizeof TAGS / sizeof TAGS[0]] = {0};
  int have_first = 0;
  size_t f_record = 0, f_word = 0;
  uint32_t f_tag = 0, f_a = 0, f_b = 0;
  while (i + 1 < na && i + 1 < nb) {
    uint32_t tag = a[i], n = a[i + 1];
    if (b[i] != tag || b[i + 1] != n || i + 2 + n > na || i + 2 + n > nb) {
      fprintf (out, "%s: records differ at record %zu: %s tag %#x of %u words, %s tag %#x of %u\n",
	       label, record, an, tag, n, bn, b[i], b[i + 1]);
      return 1;
    }
    for (uint32_t w = 0; w < n; w++) {
      uint32_t x = a[i + 2 + w], y = b[i + 2 + w];
      compared++;
      if (x != y && !(is_nan (x) && is_nan (y))) {
	differing++;
	uint32_t t = tag & 0xFF;
	by_tag[t < sizeof TAGS / sizeof TAGS[0] ? t : 0]++;
	if (!have_first) {
	  have_first = 1;
	  f_record = record;
	  f_tag = tag;
	  f_word = w;
	  f_a = x;
	  f_b = y;
	}
      }
    }
    i += 2 + n;
    record++;
  }
  if (na != nb) {
    fprintf (out, "%s: lengths differ: %zu words against %zu\n", label, na, nb);
    return 1;
  }
  if (!differing) {
    fprintf (out, "%s: identical (%zu words)\n", label, compared);
    return 0;
  }
  fprintf (out, "%s: %zu of %zu words differ (", label, differing, compared);
  const char *sep = "";
  for (size_t t = 0; t < sizeof TAGS / sizeof TAGS[0]; t++) {
    if (by_tag[t]) {
      fprintf (out, "%s%s %zu", sep, TAGS[t], by_tag[t]);
      sep = ", ";
    }
  }
  fprintf (out, "); first: record %zu (%s, tag %#x) word %zu: %s %#010x, %s %#010x\n",
	   f_record, tag_name (f_tag), f_tag, f_word, an, f_a, bn, f_b);
  return 1;
}

int volya_ode_compare (const char *ours, const char *shipped, FILE *out)
{
  int different = 0;
  for (int i = 0; i < volya_ode_scenario_count (); i++) {
    const char *name = volya_ode_scenario_name (i);
    size_t na = 0, nb = 0;
    uint32_t *a = load (ours, name, &na), *b = load (shipped, name, &nb);
    if (!a || !b) {
      fprintf (out, "%s: missing in %s\n", name, a ? shipped : ours);
      different++;
    } else {
      different += compare_words (out, name, "ours", a, na, "shipped", b, nb);
    }
    free (a);
    free (b);
  }
  /* every <name>-nodaz scenario against <name>, both the shipped library's */
  for (int i = 0; i < volya_ode_scenario_count (); i++) {
    const char *nodaz = volya_ode_scenario_name (i);
    size_t len = strlen (nodaz);
    if (len <= 6 || strcmp (nodaz + len - 6, "-nodaz")) continue;
    char name[128];
    snprintf (name, sizeof name, "%.*s", (int) (len - 6), nodaz);
    size_t na = 0, nb = 0;
    uint32_t *a = load (shipped, name, &na), *b = load (shipped, nodaz, &nb);
    if (a && b) {
      char label[300];
      snprintf (label, sizeof label, "shipped %s against %s (denormals-are-zero)", name, nodaz);
      compare_words (out, label, "with", a, na, "without", b, nb);
    }
    free (a);
    free (b);
  }
  return different;
}
