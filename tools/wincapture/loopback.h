/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The game's audio output recorded from outside it (loopback.c): WASAPI
 * loopback capture of the default output device as it is (or the one
 * named), which the game plays to. Every time is the performance
 * counter's, in 100 ns ticks, the unit WASAPI stamps its packets in. */

#ifndef VOLYA_LOOPBACK_H
#define VOLYA_LOOPBACK_H

#include <stdint.h>

typedef void (*loopback_say)(const char *fmt, ...);

struct loopback;

/* Opens the default render endpoint, or with `name` the active one whose
 * name contains it (case ignored; exactly one must), and prepares its
 * loopback stream; null after saying why. No name is logged. */
struct loopback *loopback_open(const char *name, loopback_say say);

/* Starts recording: the recording's sample 0 is now. */
int loopback_start(struct loopback *lb);

/* Stops recording. */
void loopback_stop(struct loopback *lb);

/* The performance counter now, in 100 ns ticks. */
int64_t loopback_now(void);

/* The recording's sample rate and channels. */
uint32_t loopback_rate(const struct loopback *lb);
uint32_t loopback_channels(const struct loopback *lb);

/* The recording's sample at time `t` (loopback_now's ticks). */
int64_t loopback_sample_at(const struct loopback *lb, int64_t t);

/* Writes the recording as a 16-bit PCM WAV file; 0 on failure. */
int loopback_write(const struct loopback *lb, const char *path);

/* What the recording met: packets, gaps filled with silence, packets
 * marked silent, with a discontinuity or with an unreliable time, and
 * samples clipped converting to 16 bits. */
void loopback_report(const struct loopback *lb, loopback_say say);

/* Stops and frees everything. */
void loopback_close(struct loopback *lb);

#endif
