/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The deterministic heap (docs/behavior/capture-recipe.md, rule 3): what the
 * proxy winmm.dll (frameclock.c) serves the game's modules' heap functions
 * from. Portable C, with the system reached through `struct detheap_system`,
 * so the unit test (detheap_test.c) runs it on the Mac.
 *
 * Every request is served from one region, whatever heap handle is passed:
 * rounded up to one of 200 size classes, each block an 8-byte header (its
 * class and the requested size) and the class's bytes, taken from the head
 * of its class's free list (the most recently freed) or else carved from the
 * region in order, after the last one, committing it in 4 MiB steps. Blocks
 * outside the region go to the system's functions; heaps created are
 * placeholder handles within the first 64 KiB. */

#ifndef VOLYA_DETHEAP_H
#define VOLYA_DETHEAP_H

#include <stddef.h>
#include <stdint.h>

/* The region at the proxy's start (capture-recipe.md, rule 3). */
#define DETHEAP_REGION_BASE 0x20000000u
#define DETHEAP_REGION_SIZE 0x38000000u /* 896 MiB */
#define DETHEAP_COMMIT_STEP 0x400000u   /* 4 MiB */

#define DETHEAP_CLASSES 200
#define DETHEAP_HEADER 8

/* Windows' heap flags, as the calls pass them. */
#define DETHEAP_GENERATE_EXCEPTIONS 0x4u
#define DETHEAP_ZERO_MEMORY 0x8u
#define DETHEAP_REALLOC_IN_PLACE_ONLY 0x10u

/* Placeholder handles: multiples of 0x10 from 0x10 to 0xfff0, then from
 * 0x10 again. */
#define DETHEAP_PLACEHOLDER_FIRST 0x10u
#define DETHEAP_PLACEHOLDER_LAST 0xfff0u
/* What HeapCompact answers for a placeholder. */
#define DETHEAP_COMPACT 0x100000u

/* The system: its heap functions for blocks outside the region and real
 * handles, the lock, the region's commit and the exception a failure
 * raises. `ctx` is passed back to each. */
struct detheap_system {
    void *ctx;
    void *process_heap;
    int (*commit)(void *ctx, void *at, size_t n); /* 1: committed */
    void (*lock)(void *ctx);
    void (*unlock)(void *ctx);
    void (*no_memory)(void *ctx); /* raises STATUS_NO_MEMORY */
    int (*free)(void *ctx, void *heap, uint32_t flags, void *p);
    size_t (*size)(void *ctx, void *heap, uint32_t flags, const void *p);
    void *(*realloc)(void *ctx, void *heap, uint32_t flags, void *p, size_t n);
    int (*destroy)(void *ctx, void *heap);
    int (*validate)(void *ctx, void *heap, uint32_t flags, const void *p);
    size_t (*compact)(void *ctx, void *heap, uint32_t flags);
};

struct detheap {
    uint8_t *base;
    size_t size;      /* the region's */
    size_t carved;    /* bytes carved from its start */
    size_t committed; /* bytes committed from its start */
    void *free_[DETHEAP_CLASSES];
    uintptr_t next_placeholder;
    struct detheap_system sys;
    /* For the log (racy counts, diagnostics only). */
    long created, failed_allocs, failed_reallocs, in_place_failures, moved;
};

/* HeapWalk's one entry for a placeholder handle: the region. */
struct detheap_walk {
    void *data;
    size_t size, committed;
};

/* The class sizes: 8 to 512 bytes in steps of 8, then each 1/8 larger than
 * the one before, rounded up to a multiple of 8. */
uint64_t detheap_class_size(int c);
/* The class of a request of `n` bytes, or -1 when none is that large. */
int detheap_class(uint64_t n);

/* The region at `base` of `size` bytes, none committed. */
void detheap_init(struct detheap *h, void *base, size_t size, const struct detheap_system *sys);
int detheap_owns(const struct detheap *h, const void *p);
int detheap_is_placeholder(const void *heap);

/* The heap functions, with Windows' arguments and answers. */
void *detheap_create(struct detheap *h);
int detheap_destroy(struct detheap *h, void *heap);
void *detheap_alloc(struct detheap *h, void *heap, uint32_t flags, size_t n);
void *detheap_realloc(struct detheap *h, void *heap, uint32_t flags, void *p, size_t n);
int detheap_free(struct detheap *h, void *heap, uint32_t flags, void *p);
size_t detheap_size(struct detheap *h, void *heap, uint32_t flags, const void *p);
int detheap_validate(struct detheap *h, void *heap, uint32_t flags, const void *p);
size_t detheap_compact(struct detheap *h, void *heap, uint32_t flags);
/* A placeholder's walk: 1 and the region's entry when `previous` is null,
 * then 0 (no more items); -1 for a real handle (the system's). */
int detheap_walk(const struct detheap *h, const void *heap, const void *previous,
                 struct detheap_walk *entry);

#endif
