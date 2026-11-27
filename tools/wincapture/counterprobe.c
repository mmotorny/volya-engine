/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* counterprobe.exe: the driver's self-test program (`volyacapture
 * selftest`, capture.c). Ours alone: it reads the time-stamp counter with
 * `rdtsc` and `rdtscp` and prints what it read, and whether the frame
 * clock's shared memory is open and the counter client (counter.c) has
 * started. Run natively it reads the processor's counter; run by the
 * driver under drrun with the client, the client's.
 *
 * Exit code: 0 the reads are the client's (consecutive reads TSC_STEP
 * apart, on the formula of frameclock.h with the clock at 0), 2 they are
 * not, 3 no shared memory. */

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdint.h>
#include <stdio.h>

#include "frameclock.h"

#define READS 4

static uint64_t read_rdtsc(void) {
    uint32_t lo, hi;
    __asm__ volatile("rdtsc" : "=a"(lo), "=d"(hi));
    return (uint64_t)hi << 32 | lo;
}

static uint64_t read_rdtscp(uint32_t *aux) {
    uint32_t lo, hi;
    __asm__ volatile("rdtscp" : "=a"(lo), "=d"(hi), "=c"(*aux));
    return (uint64_t)hi << 32 | lo;
}

int main(void) {
    HANDLE map = OpenFileMappingA(FILE_MAP_READ, FALSE, FRAMECLOCK_NAME);
    struct frameclock *c =
        map ? MapViewOfFile(map, FILE_MAP_READ, 0, 0, sizeof(struct frameclock)) : NULL;
    printf("counterprobe: shared memory %s", c ? "open" : "not open");
    if (c) {
        printf(", version %llu (driver's %d), counter client started %lld, reads before %lld",
               (unsigned long long)c->version, FRAMECLOCK_VERSION,
               (long long)c->counter_attached, (long long)c->counter_reads);
    }
    printf("\n");
    uint64_t v[READS + 1];
    for (int i = 0; i < READS; i++) {
        v[i] = read_rdtsc();
    }
    uint32_t aux = 0;
    v[READS] = read_rdtscp(&aux);
    int ours = 1;
    for (int i = 0; i <= READS; i++) {
        printf("counterprobe: %s %d: %llu\n", i < READS ? "rdtsc" : "rdtscp", i,
               (unsigned long long)v[i]);
        if (i && v[i] - v[i - 1] != TSC_STEP) {
            ours = 0;
        }
    }
    printf("counterprobe: rdtscp's processor number %lu\n", (unsigned long)aux);
    if (v[0] < TSC_BASE || (v[0] - TSC_BASE) % TSC_STEP || aux) {
        ours = 0;
    }
    if (c) {
        printf("counterprobe: reads substituted after %lld\n", (long long)c->counter_reads);
    }
    printf("counterprobe: %s\n", ours ? "the reads are the counter client's"
                                      : "the reads are not the counter client's");
    fflush(stdout);
    return !c ? 3 : ours ? 0 : 2;
}
