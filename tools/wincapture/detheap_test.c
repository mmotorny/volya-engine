/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The deterministic heap's unit test (detheap.c), natively: its size
 * classes, carving and commits, LIFO reuse, the reallocation's paths and
 * the placeholder handles' answers (docs/behavior/capture-recipe.md,
 * rule 3), the system's functions faked. */

#include "detheap.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures;

#define CHECK(cond)                                                    \
    do {                                                               \
        if (!(cond)) {                                                 \
            fprintf(stderr, "%s:%d: %s\n", __FILE__, __LINE__, #cond); \
            failures++;                                                \
        }                                                              \
    } while (0)

/* The faked system: its heap's blocks carry their size before them, and
 * every call is counted with its heap. */
struct fake {
    size_t committed;
    int commits, locks, unlocks, raised;
    int frees, sizes, reallocs, destroys, validates, compacts;
    void *last_heap;
};

static int fake_commit(void *ctx, void *at, size_t n) {
    struct fake *f = ctx;
    (void)at;
    f->committed += n;
    f->commits++;
    return 1;
}
static void fake_lock(void *ctx) {
    ((struct fake *)ctx)->locks++;
}
static void fake_unlock(void *ctx) {
    ((struct fake *)ctx)->unlocks++;
}
static void fake_no_memory(void *ctx) {
    ((struct fake *)ctx)->raised++;
}

static void *sys_alloc(size_t n) {
    size_t *b = malloc(sizeof(size_t) * 2 + n);
    b[0] = n;
    return b + 2;
}
static int fake_free(void *ctx, void *heap, uint32_t flags, void *p) {
    struct fake *f = ctx;
    (void)flags;
    f->frees++;
    f->last_heap = heap;
    free((size_t *)p - 2);
    return 1;
}
static size_t fake_size(void *ctx, void *heap, uint32_t flags, const void *p) {
    struct fake *f = ctx;
    (void)flags;
    f->sizes++;
    f->last_heap = heap;
    return ((const size_t *)p)[-2];
}
static void *fake_realloc(void *ctx, void *heap, uint32_t flags, void *p, size_t n) {
    struct fake *f = ctx;
    (void)flags;
    (void)p;
    (void)n;
    f->reallocs++;
    f->last_heap = heap;
    return NULL;
}
static int fake_destroy(void *ctx, void *heap) {
    struct fake *f = ctx;
    f->destroys++;
    f->last_heap = heap;
    return 7;
}
static int fake_validate(void *ctx, void *heap, uint32_t flags, const void *p) {
    struct fake *f = ctx;
    (void)flags;
    (void)p;
    f->validates++;
    f->last_heap = heap;
    return 0;
}
static size_t fake_compact(void *ctx, void *heap, uint32_t flags) {
    struct fake *f = ctx;
    (void)flags;
    f->compacts++;
    f->last_heap = heap;
    return 42;
}

#define PROCESS_HEAP ((void *)0x123450000)
#define REAL_HEAP ((void *)0x67890000)
/* A test region of 13 MiB, so the commits' last step is short: four
 * blocks of 3 MiB (class 3,249,680 bytes) end past 12 MiB. */
#define SIZE (13u * 1024 * 1024)

static struct fake fake;
static struct detheap heap;
static uint8_t *region;

static void start(void) {
    memset(&fake, 0, sizeof fake);
    struct detheap_system sys = {
        .ctx = &fake,
        .process_heap = PROCESS_HEAP,
        .commit = fake_commit,
        .lock = fake_lock,
        .unlock = fake_unlock,
        .no_memory = fake_no_memory,
        .free = fake_free,
        .size = fake_size,
        .realloc = fake_realloc,
        .destroy = fake_destroy,
        .validate = fake_validate,
        .compact = fake_compact,
    };
    detheap_init(&heap, region, SIZE, &sys);
}

static void size_classes(void) {
    CHECK(detheap_class_size(0) == 8);
    CHECK(detheap_class_size(63) == 512);
    CHECK(detheap_class_size(64) == 576);  /* 512 + 64 */
    CHECK(detheap_class_size(65) == 648);  /* 576 + 72 */
    CHECK(detheap_class_size(66) == 736);  /* 648 + 81 = 729, up to 8 */
    CHECK(detheap_class_size(67) == 832);  /* 736 + 92 = 828 */
    CHECK(detheap_class_size(200) == 0);
    for (int c = 1; c < DETHEAP_CLASSES; c++) {
        uint64_t s = detheap_class_size(c), before = detheap_class_size(c - 1);
        CHECK(s % 8 == 0);
        CHECK(s > before);
        CHECK(c < 64 ? s == before + 8 : s == ((before + before / 8 + 7) & ~(uint64_t)7));
    }
    /* The largest class is past the region's size, so the region bounds a
     * request, not the classes. */
    CHECK(detheap_class_size(DETHEAP_CLASSES - 1) > DETHEAP_REGION_SIZE);
    CHECK(detheap_class(0) == 0);
    CHECK(detheap_class(1) == 0);
    CHECK(detheap_class(8) == 0);
    CHECK(detheap_class(9) == 1);
    CHECK(detheap_class(512) == 63);
    CHECK(detheap_class(513) == 64);
    CHECK(detheap_class(576) == 64);
    CHECK(detheap_class(577) == 65);
    CHECK(detheap_class(detheap_class_size(DETHEAP_CLASSES - 1)) == DETHEAP_CLASSES - 1);
    CHECK(detheap_class(detheap_class_size(DETHEAP_CLASSES - 1) + 1) == -1);
}

static void carving_and_reuse(void) {
    start();
    /* Carved in order, after the last one, whatever the handle. */
    uint8_t *a = detheap_alloc(&heap, PROCESS_HEAP, 0, 5);
    uint8_t *b = detheap_alloc(&heap, (void *)0x10, 0, 100);
    uint8_t *c = detheap_alloc(&heap, REAL_HEAP, 0, 8);
    CHECK(a == region + 8);
    CHECK(b == a + 8 + 8);
    CHECK(c == b + 104 + 8);
    CHECK(((uintptr_t)a | (uintptr_t)b | (uintptr_t)c) % 8 == 0);
    CHECK(((uint32_t *)a)[-2] == 0 && ((uint32_t *)a)[-1] == 5); /* the header: class, size */
    CHECK(((uint32_t *)b)[-2] == 12 && ((uint32_t *)b)[-1] == 100);
    CHECK(detheap_size(&heap, REAL_HEAP, 0, b) == 100);
    CHECK(detheap_owns(&heap, a) && detheap_owns(&heap, c));
    CHECK(fake.commits == 1 && fake.committed == DETHEAP_COMMIT_STEP);
    CHECK(fake.locks == 3 && fake.unlocks == 3);
    /* LIFO: the most recently freed block of the class first. */
    uint8_t *d = detheap_alloc(&heap, PROCESS_HEAP, 0, 3);
    CHECK(detheap_free(&heap, PROCESS_HEAP, 0, a) == 1);
    CHECK(detheap_free(&heap, REAL_HEAP, 0, c) == 1);
    CHECK(detheap_free(&heap, PROCESS_HEAP, 0, d) == 1);
    CHECK(fake.frees == 0);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, 7) == d);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, 1) == c);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, 8) == a);
    /* Then carved again. */
    uint8_t *e = detheap_alloc(&heap, PROCESS_HEAP, 0, 8);
    CHECK(e == d + 16);
    /* Another class's free list is its own. */
    CHECK(detheap_free(&heap, PROCESS_HEAP, 0, b) == 1);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, 8) == e + 16);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, 97) == b);
    CHECK(detheap_size(&heap, PROCESS_HEAP, 0, b) == 97);
}

static void zeroing_commits_and_failure(void) {
    start();
    uint8_t *a = detheap_alloc(&heap, PROCESS_HEAP, 0, 20);
    memset(a, 0xab, 24);
    detheap_free(&heap, PROCESS_HEAP, 0, a);
    /* HEAP_ZERO_MEMORY zeroes the whole class. */
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, DETHEAP_ZERO_MEMORY, 17) == a);
    for (int i = 0; i < 24; i++) {
        CHECK(a[i] == 0);
    }
    /* Committed in 4 MiB steps as the carving reaches them, the last step
     * up to the region's end. */
    size_t big = 3u * 1024 * 1024;
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, big) != NULL);
    CHECK(fake.committed == DETHEAP_COMMIT_STEP);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, big) != NULL);
    CHECK(fake.commits == 2 && fake.committed == 2 * DETHEAP_COMMIT_STEP);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, big) != NULL);
    CHECK(fake.commits == 3 && fake.committed == 3 * DETHEAP_COMMIT_STEP);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, big) != NULL);
    CHECK(fake.commits == 4 && fake.committed == SIZE);
    /* Past the region: null, or STATUS_NO_MEMORY raised when the call's
     * own flags ask for it. */
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, big) == NULL);
    CHECK(fake.raised == 0);
    CHECK(detheap_alloc(&heap, (void *)0x20, DETHEAP_GENERATE_EXCEPTIONS, big) == NULL);
    CHECK(fake.raised == 1);
    /* Larger than any class. */
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, DETHEAP_GENERATE_EXCEPTIONS, (size_t)-1) == NULL);
    CHECK(fake.raised == 2);
    CHECK(fake.locks == fake.unlocks);
}

static void reallocation(void) {
    start();
    /* Within the class: the block stays, its size updated, the growth
     * zeroed when asked. */
    uint8_t *a = detheap_alloc(&heap, PROCESS_HEAP, 0, 10);
    memset(a, 0xcd, 16);
    CHECK(detheap_realloc(&heap, PROCESS_HEAP, DETHEAP_ZERO_MEMORY, a, 14) == a);
    CHECK(detheap_size(&heap, PROCESS_HEAP, 0, a) == 14);
    CHECK(a[9] == 0xcd && a[10] == 0 && a[13] == 0 && a[14] == 0xcd);
    CHECK(detheap_realloc(&heap, PROCESS_HEAP, DETHEAP_REALLOC_IN_PLACE_ONLY, a, 3) == a);
    CHECK(detheap_size(&heap, PROCESS_HEAP, 0, a) == 3);
    CHECK(detheap_realloc(&heap, PROCESS_HEAP, 0, a, 16) == a);
    /* Past the class: in place only fails, returning null (only HeapAlloc
     * raises)... */
    CHECK(detheap_realloc(&heap, PROCESS_HEAP, DETHEAP_REALLOC_IN_PLACE_ONLY, a, 17) == NULL);
    CHECK(detheap_realloc(&heap, PROCESS_HEAP,
                          DETHEAP_REALLOC_IN_PLACE_ONLY | DETHEAP_GENERATE_EXCEPTIONS, a,
                          17) == NULL);
    CHECK(fake.raised == 0);
    CHECK(heap.in_place_failures == 2);
    CHECK(detheap_size(&heap, PROCESS_HEAP, 0, a) == 16);
    /* ...else a new block, the old contents copied, the old freed. */
    for (int i = 0; i < 16; i++) {
        a[i] = (uint8_t)i;
    }
    uint8_t *b = detheap_realloc(&heap, PROCESS_HEAP, DETHEAP_ZERO_MEMORY, a, 40);
    CHECK(b == a + 16 + 8);
    CHECK(detheap_size(&heap, PROCESS_HEAP, 0, b) == 40);
    CHECK(b[0] == 0 && b[15] == 15 && b[16] == 0 && b[39] == 0);
    CHECK(detheap_alloc(&heap, PROCESS_HEAP, 0, 16) == a); /* freed */
    /* Shrinking keeps the block (the smaller size fits its class). */
    uint8_t *c = detheap_alloc(&heap, PROCESS_HEAP, 0, 600);
    for (int i = 0; i < 600; i++) {
        c[i] = (uint8_t)i;
    }
    uint8_t *d = detheap_realloc(&heap, PROCESS_HEAP, 0, c, 100);
    CHECK(d == c); /* 100 bytes fit the class of 600 */
    CHECK(detheap_size(&heap, PROCESS_HEAP, 0, d) == 100);
    CHECK(fake.frees == 0 && fake.reallocs == 0);

    /* A system heap's block moves into the region: the smaller of the old
     * and new sizes copied, freed with the system's function. */
    uint8_t *s = sys_alloc(32);
    for (int i = 0; i < 32; i++) {
        s[i] = (uint8_t)(100 + i);
    }
    uint8_t *m = detheap_realloc(&heap, REAL_HEAP, 0, s, 20);
    CHECK(detheap_owns(&heap, m));
    CHECK(m[0] == 100 && m[19] == 119);
    CHECK(detheap_size(&heap, REAL_HEAP, 0, m) == 20);
    CHECK(fake.sizes == 1 && fake.frees == 1 && fake.last_heap == REAL_HEAP);
    s = sys_alloc(4);
    memset(s, 0x77, 4);
    m = detheap_realloc(&heap, (void *)0x30, DETHEAP_ZERO_MEMORY, s, 12);
    CHECK(m[3] == 0x77 && m[4] == 0 && m[11] == 0);
    CHECK(fake.frees == 2 && fake.last_heap == PROCESS_HEAP); /* a placeholder's: the process heap */
    /* In place only: the system's function, the placeholder's the process
     * heap. */
    s = sys_alloc(4);
    CHECK(detheap_realloc(&heap, REAL_HEAP, DETHEAP_REALLOC_IN_PLACE_ONLY, s, 8) == NULL);
    CHECK(fake.reallocs == 1 && fake.last_heap == REAL_HEAP);
    CHECK(detheap_realloc(&heap, (void *)0x40, DETHEAP_REALLOC_IN_PLACE_ONLY, s, 8) == NULL);
    CHECK(fake.reallocs == 2 && fake.last_heap == PROCESS_HEAP);
    fake_free(&fake, NULL, 0, s);
}

static void placeholders(void) {
    start();
    /* Multiples of 0x10 from 0x10 to 0xfff0, then from 0x10 again. */
    CHECK(detheap_create(&heap) == (void *)0x10);
    CHECK(detheap_create(&heap) == (void *)0x20);
    void *last = NULL;
    for (int i = 0; i < 4093; i++) {
        last = detheap_create(&heap);
    }
    CHECK(last == (void *)0xfff0);
    CHECK(detheap_create(&heap) == (void *)0x10);
    CHECK(heap.created == 4096);
    CHECK(detheap_is_placeholder((void *)0x10) && detheap_is_placeholder((void *)0xfff0));
    CHECK(!detheap_is_placeholder(NULL) && !detheap_is_placeholder((void *)0x18));
    CHECK(!detheap_is_placeholder((void *)0x10000) && !detheap_is_placeholder(REAL_HEAP));

    void *ph = (void *)0x10;
    uint8_t *a = detheap_alloc(&heap, ph, 0, 8);
    CHECK(detheap_owns(&heap, a));
    /* HeapDestroy succeeds and keeps the blocks. */
    CHECK(detheap_destroy(&heap, ph) == 1);
    CHECK(detheap_size(&heap, ph, 0, a) == 8);
    CHECK(fake.destroys == 0);
    CHECK(detheap_destroy(&heap, REAL_HEAP) == 7 && fake.destroys == 1);
    /* HeapValidate: success, and for any block in the region. */
    CHECK(detheap_validate(&heap, ph, 0, NULL) == 1);
    CHECK(detheap_validate(&heap, REAL_HEAP, 0, a) == 1);
    CHECK(fake.validates == 0);
    CHECK(detheap_validate(&heap, REAL_HEAP, 0, NULL) == 0 && fake.validates == 1);
    /* HeapCompact: 1 MiB. */
    CHECK(detheap_compact(&heap, ph, 0) == 0x100000);
    CHECK(detheap_compact(&heap, REAL_HEAP, 0) == 42 && fake.compacts == 1);
    /* HeapWalk: one region entry, then no more items. */
    struct detheap_walk entry = {0};
    CHECK(detheap_walk(&heap, ph, NULL, &entry) == 1);
    CHECK(entry.data == region && entry.size == SIZE && entry.committed == SIZE);
    CHECK(detheap_walk(&heap, ph, entry.data, &entry) == 0);
    CHECK(detheap_walk(&heap, REAL_HEAP, NULL, &entry) == -1);
    /* A block outside the region with a placeholder: the process heap. */
    uint8_t *s = sys_alloc(5);
    CHECK(detheap_size(&heap, ph, 0, s) == 5 && fake.last_heap == PROCESS_HEAP);
    CHECK(detheap_free(&heap, ph, 0, s) == 1 && fake.last_heap == PROCESS_HEAP);
    s = sys_alloc(5);
    CHECK(detheap_size(&heap, REAL_HEAP, 0, s) == 5 && fake.last_heap == REAL_HEAP);
    CHECK(detheap_free(&heap, REAL_HEAP, 0, s) == 1 && fake.last_heap == REAL_HEAP);
}

int main(void) {
    region = aligned_alloc(4096, SIZE);
    size_classes();
    carving_and_reuse();
    zeroing_commits_and_failure();
    reallocation();
    placeholders();
    if (failures) {
        fprintf(stderr, "%d checks failed\n", failures);
        return 1;
    }
    printf("the deterministic heap: every check passed\n");
    return 0;
}
