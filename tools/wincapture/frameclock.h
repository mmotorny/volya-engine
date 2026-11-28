/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The frame clock's shared memory, between the proxy winmm.dll in the game
 * (frameclock.c), the counter client the game runs under (counter.c) and
 * the driver (capture.c). The driver creates it before it starts the game;
 * the proxy opens it when the process loads it and runs the clock only then
 * (without it, it forwards everything unchanged); the client opens it when
 * it starts, before the game's first instruction.
 *
 * Every field is written by one side only and read with full barriers
 * (Interlocked operations, sequentially consistent atomics), so each side
 * sees the other's writes in order. */

#ifndef VOLYA_FRAMECLOCK_H
#define VOLYA_FRAMECLOCK_H

#include <stdint.h>

#define FRAMECLOCK_NAME "Local\\VolyaFrameClock"
#define FRAMECLOCK_MAGIC 0x6b636f6c63796c76ull /* "vlyclock" */
#define FRAMECLOCK_VERSION 11

/* The counter client's time-stamp counter (counter.c): TSC_BASE +
 * clock_ms × TSC_PER_MS + n × TSC_STEP, n the reads substituted
 * before. 3 GHz of the clock, an hour's worth at its start (as the proxy's
 * performance counter starts an hour after start-up), back-to-back reads
 * 10 ns apart. */
#define TSC_PER_MS 3000000ull
#define TSC_BASE (3600000ull * TSC_PER_MS)
#define TSC_STEP 30ull

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
    uint64_t sleep_advances; /* 1: a main-thread Sleep(n) adds n ms to the clock */
    uint64_t start_quantum;  /* before the first frame, ms a main-thread Sleep adds at least */
    uint64_t first_frame;    /* ticks the clock is moved up to at the first frame (0: not) */
    uint64_t prestart_sleeps; /* 1: sleeps before the first frame advance the clock too */
    char log_path[260];      /* the proxy's own log (empty: none) */
    int64_t heap_off;        /* 1: no deterministic heap (the bisect's --no-heap) */
    char audio_path[260];    /* the game's audio, rendered a frame at a time (empty: none) */

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
    volatile int64_t other_reads; /* clock reads by the other threads */
    volatile int64_t sleeps;    /* Sleep calls by the main thread */
    volatile int64_t sleep_ticks; /* ticks they added (sleep_advances) */
    volatile int64_t start_ticks; /* the clock when the first frame was presented, before the move */
    volatile int64_t captured;  /* the frame whose back buffer `pixels` holds */
    volatile int64_t capture_error; /* the failing HRESULT of the last copy, else 0 */
    volatile int64_t width;     /* the copied frame's size */
    volatile int64_t height;
    volatile int64_t format;    /* its D3DFORMAT */
    /* The secondary pass (frameclock.c, original-determinism.md 7, rule 2). */
    volatile int64_t sync_identified; /* the frame its thread and sections were found at (0: not) */
    volatile int64_t b_entries;       /* the main thread's entries to B, gated */
    volatile int64_t b_entries_waited; /* those that waited */
    volatile int64_t frame_sleeps;    /* the frame's Sleep(0), gated */
    volatile int64_t frame_sleeps_waited; /* those that waited */
    volatile int64_t sync_timeouts;   /* waits released by the watchdog */
    volatile int64_t idle_sleeps;     /* main-thread Sleep(1 ms or more) after the first frame */

    /* Written by the counter client (counter.c, original-determinism.md 7,
     * rule 1). */
    volatile int64_t counter_attached; /* 1 once the client runs in the game */
    volatile int64_t counter_reads;    /* time-stamp-counter reads substituted, all told */
    volatile int64_t counter_at_proxy; /* by the proxy: counter_attached when it attached */
    volatile int64_t counter_detached; /* 1 once DynamoRIO detaches from the game (the driver asks) */

    /* Written by the proxy: the deterministic heap (detheap.h,
     * capture-recipe.md rule 3). */
    volatile int64_t heap; /* 1 its region reserved, -1 the reservation failed, 0 off */
    volatile int64_t heap_carved; /* bytes carved from the region (every 2 s) */

    /* Written by the proxy: the device's creation (#869). */
    volatile int64_t create_device_calls;
    volatile int64_t create_device_failures;
    volatile int64_t create_device_first; /* the first failure's HRESULT (0: none) */
    volatile int64_t create_device_last;  /* the last failure's */

    /* Written by the proxy: the game's audio on the frame clock (openal.c),
     * appended to audio_path at each frame presented once the game's
     * OpenAL32.dll is ours: 16-bit stereo at audio_rate, audio_first_frame
     * the first frame whose samples the file starts with. */
    volatile int64_t audio_rate;        /* 0 until the device's first context */
    volatile int64_t audio_first_frame; /* 0 until then */
    volatile int64_t audio_frames;      /* samples (per channel) written */
    volatile int64_t audio_clipped;     /* samples clipped converting to 16 bits */
    volatile int64_t audio_error;       /* a failed write's error, else 0 */
    uint8_t pixels[FRAMECLOCK_MAX_PIXELS * 4]; /* rows top first, B G R X */
};

/* The clock in ticks, as every clock the game reads derives it (the proxy
 * counts the read first, frameclock.c). */
static inline uint64_t frameclock_ticks(struct frameclock *c) {
    return (uint64_t)__atomic_load_n(&c->frames, __ATOMIC_SEQ_CST) * c->step +
           (uint64_t)__atomic_load_n(&c->extra, __ATOMIC_SEQ_CST) +
           (uint64_t)__atomic_load_n(&c->sleep_ticks, __ATOMIC_SEQ_CST);
}

#endif
