/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The generator behind math.random on every target: the C standard's
** example rand (RAND_MAX 32767, seed 1). Our choice until #106 establishes
** the original's.
*/

static unsigned long next_rand = 1;

int volya_rand(void)
{
  next_rand = next_rand * 1103515245 + 12345;
  return (int)((next_rand / 65536) % 32768);
}

void volya_srand(unsigned seed)
{
  next_rand = seed;
}
