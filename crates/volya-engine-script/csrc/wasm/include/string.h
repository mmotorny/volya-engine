/* SPDX-FileCopyrightText: 2026 Maksym Motorny */
/* SPDX-License-Identifier: BSD-3-Clause */
/* Our minimal C library for wasm32 (csrc/wasm/libc.c; memcpy, memmove,
** memset, memcmp and strlen come from Rust's compiler-builtins). */
#pragma once
#include <stddef.h>
void *memchr(const void *s, int c, size_t n);
int memcmp(const void *a, const void *b, size_t n);
void *memcpy(void *restrict d, const void *restrict s, size_t n);
void *memmove(void *d, const void *s, size_t n);
void *memset(void *d, int c, size_t n);
char *strcat(char *restrict d, const char *restrict s);
char *strchr(const char *s, int c);
int strcmp(const char *a, const char *b);
int strcoll(const char *a, const char *b);
char *strcpy(char *restrict d, const char *restrict s);
size_t strcspn(const char *s, const char *reject);
char *strerror(int e);
size_t strlen(const char *s);
char *strncat(char *restrict d, const char *restrict s, size_t n);
int strncmp(const char *a, const char *b, size_t n);
char *strncpy(char *restrict d, const char *restrict s, size_t n);
char *strpbrk(const char *s, const char *accept);
char *strrchr(const char *s, int c);
size_t strspn(const char *s, const char *accept);
char *strstr(const char *h, const char *n);
