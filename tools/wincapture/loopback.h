/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The game's audio output recorded from outside it (loopback.c): WASAPI
 * loopback capture of a render endpoint the driver makes the default for
 * the run, so the game, which plays to the default, plays to it, and the
 * machine's own speakers stay silent. Every time is the performance
 * counter's, in 100 ns ticks, the unit WASAPI stamps its packets in. */

#ifndef VOLYA_LOOPBACK_H
#define VOLYA_LOOPBACK_H

#include <stdint.h>

typedef void (*loopback_say)(const char *fmt, ...);

struct loopback;

/* Lists the active render endpoints (name, form factor, whether default)
 * with `say`; 0 when they cannot be listed. */
int loopback_list(loopback_say say);

/* Puts back the default render endpoint an earlier run left switched, if
 * `marker` (a file of the work folder) says one did. */
void loopback_restore_left(const char *marker, loopback_say say);

/* Opens the active render endpoint whose name contains `name` (case
 * ignored; exactly one must), makes it the default for every role, the
 * one before written to `marker` until loopback_close puts it back, and
 * prepares its loopback stream; null after saying why. */
struct loopback *loopback_open(const char *name, const char *marker, loopback_say say);

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

/* Puts the default endpoint back and frees everything. */
void loopback_close(struct loopback *lb);

#endif
