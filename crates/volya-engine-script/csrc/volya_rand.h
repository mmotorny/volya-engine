/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** Included before every vendored file (build.rs): math.random's rand and
** srand become ours (csrc/volya_rand.c), one generator on every target
** (#106).
*/

#ifndef volya_rand_h
#define volya_rand_h

#include <stdlib.h>

#undef RAND_MAX
#define RAND_MAX 32767
#define rand volya_rand
#define srand volya_srand

int volya_rand(void);
void volya_srand(unsigned seed);

#endif
