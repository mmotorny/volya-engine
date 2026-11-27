/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The frame clock's shared memory, between the proxy winmm.dll in the game
 * (frameclock.c) and the driver (capture.c). The driver creates it before it
 * starts the game; the proxy opens it when the process loads it and runs
 * the clock only then (without it, it forwards everything unchanged).
 *
 * Every field is written by one side only and read with full barriers
 * (Interlocked operations), so each side sees the other's writes in order. */

#ifndef VOLYA_FRAMECLOCK_H
#define VOLYA_FRAMECLOCK_H

#include <stdint.h>

#define FRAMECLOCK_NAME "Local\\VolyaFrameClock"
#define FRAMECLOCK_MAGIC 0x6b636f6c63796c76ull /* "vlyclock" */
#define FRAMECLOCK_VERSION 1

/* Ticks of the clock: 100 ns, the unit of the system time. Every clock the
 * game reads is derived from one count of them (frameclock.c). */
#define FRAMECLOCK_TICKS_PER_MS 10000ull

/* The largest frame the proxy can copy out: 3840 × 2160, 4 bytes a pixel. */
#define FRAMECLOCK_MAX_PIXELS (3840u * 2160u)

struct frameclock {
    /* Set by the driver before the game starts. */
    uint64_t magic;
    uint64_t version;
    uint64_t step;  /* ticks a presented frame advances the clock */
    uint64_t spin;  /* main-thread clock reads with no frame that add 1 ms (0: never) */
    uint64_t epoch; /* the system time at tick 0, in ticks since 1601 (FILETIME) */

    /* Written by the driver while the game runs. */
    volatile int64_t hold;    /* hold after presenting this frame (0: never) */
    volatile int64_t capture; /* copy this frame's back buffer before presenting it (0: none) */

    /* Written by the proxy. */
    volatile int64_t attached;  /* 1 once the proxy runs the clock in the game */
    volatile int64_t hooked;    /* 1 once a Direct3D 9 device's Present is hooked */
    volatile int64_t frames;    /* frames presented */
    volatile int64_t held;      /* the frame the game is held after (0: running) */
    volatile int64_t extra;     /* ticks added by reads with no frame (spin) */
    volatile int64_t reads;     /* clock reads by the main thread, all told */
    volatile int64_t captured;  /* the frame whose back buffer `pixels` holds */
    volatile int64_t capture_error; /* the failing HRESULT of the last copy, else 0 */
    volatile int64_t width;     /* the copied frame's size */
    volatile int64_t height;
    volatile int64_t format;    /* its D3DFORMAT */
    uint8_t pixels[FRAMECLOCK_MAX_PIXELS * 4]; /* rows top first, B G R X */
};

#endif
