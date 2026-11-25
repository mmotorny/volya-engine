/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The exception tag LLVM's WebAssembly setjmp/longjmp lowering throws and
** catches (C_LONGJMP; csrc/wasm/libc.c): its payload is a pointer.
*/

.section .data,"",@
.globl __c_longjmp
.tagtype __c_longjmp i32
__c_longjmp:
