/* SPDX-FileCopyrightText: 2026 Maksym Motorny */
/* SPDX-License-Identifier: BSD-3-Clause */
/* Our minimal C library for wasm32 (csrc/wasm/libc.c): no files; stdin is
** empty, stdout and stderr are discarded. */
#pragma once
#include <stdarg.h>
#include <stddef.h>
typedef struct volya_file FILE;
extern FILE *stdin, *stdout, *stderr;
#define EOF (-1)
#define BUFSIZ 1024
FILE *fopen(const char *path, const char *mode);
int fclose(FILE *f);
size_t fread(void *p, size_t size, size_t n, FILE *f);
size_t fwrite(const void *p, size_t size, size_t n, FILE *f);
int getc(FILE *f);
int ungetc(int c, FILE *f);
int feof(FILE *f);
int ferror(FILE *f);
int fflush(FILE *f);
int fputc(int c, FILE *f);
int fputs(const char *s, FILE *f);
int fprintf(FILE *f, const char *fmt, ...);
int printf(const char *fmt, ...);
int sprintf(char *buf, const char *fmt, ...);
int snprintf(char *buf, size_t cap, const char *fmt, ...);
int vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap);
