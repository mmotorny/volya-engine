/* SPDX-FileCopyrightText: 2026 Maksym Motorny */
/* SPDX-License-Identifier: BSD-3-Clause */
/* Our minimal C library for wasm32 (csrc/wasm/libc.c). */
#pragma once
extern int errno;
#define ENOENT 2
#define EINVAL 22
#define ERANGE 34
