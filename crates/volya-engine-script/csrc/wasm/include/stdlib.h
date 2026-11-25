/* SPDX-FileCopyrightText: 2026 Maksym Motorny */
/* SPDX-License-Identifier: BSD-3-Clause */
/* Our minimal C library for wasm32 (csrc/wasm/libc.c, src/libc.rs). */
#pragma once
#include <stddef.h>
#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#define RAND_MAX 32767
void *malloc(size_t size);
void *calloc(size_t n, size_t size);
void *realloc(void *p, size_t size);
void free(void *p);
double strtod(const char *s, char **end);
unsigned long strtoul(const char *s, char **end, int base);
int rand(void);
void srand(unsigned seed);
int abs(int x);
_Noreturn void abort(void);
_Noreturn void exit(int status);
