/* SPDX-FileCopyrightText: 2026 Maksym Motorny */
/* SPDX-License-Identifier: BSD-3-Clause */
/* Our minimal C library for wasm32 (csrc/wasm/libc.c). clang lowers these
** calls to WebAssembly exception handling (-mllvm -wasm-enable-sjlj), run by
** __wasm_setjmp, __wasm_setjmp_test and __wasm_longjmp. */
#pragma once
typedef struct { void *slots[4]; } jmp_buf[1];
int setjmp(jmp_buf env);
_Noreturn void longjmp(jmp_buf env, int val);
