/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The deterministic heap (detheap.h; docs/behavior/capture-recipe.md,
 * rule 3). */

#include "detheap.h"

#include <string.h>

/* A block's header, before the pointer returned. */
struct header {
    uint32_t class_;
    uint32_t size; /* requested */
};

static uint64_t class_sizes[DETHEAP_CLASSES];

static void fill_classes(void) {
    for (int c = 0; c < DETHEAP_CLASSES; c++) {
        if (c < 64) {
            class_sizes[c] = 8u * (uint64_t)(c + 1);
        } else {
            uint64_t s = class_sizes[c - 1] + class_sizes[c - 1] / 8;
            class_sizes[c] = (s + 7) & ~(uint64_t)7;
        }
    }
}

uint64_t detheap_class_size(int c) {
    if (!class_sizes[0]) {
        fill_classes();
    }
    return c >= 0 && c < DETHEAP_CLASSES ? class_sizes[c] : 0;
}

int detheap_class(uint64_t n) {
    if (!class_sizes[0]) {
        fill_classes();
    }
    if (n <= 512) {
        return n ? (int)((n + 7) / 8) - 1 : 0;
    }
    int lo = 64, hi = DETHEAP_CLASSES - 1;
    if (n > class_sizes[hi]) {
        return -1;
    }
    while (lo < hi) {
        int mid = (lo + hi) / 2;
        if (class_sizes[mid] >= n) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    return lo;
}

void detheap_init(struct detheap *h, void *base, size_t size, const struct detheap_system *sys) {
    memset(h, 0, sizeof *h);
    h->base = base;
    h->size = size;
    h->next_placeholder = DETHEAP_PLACEHOLDER_FIRST;
    h->sys = *sys;
    fill_classes();
}

int detheap_owns(const struct detheap *h, const void *p) {
    uintptr_t a = (uintptr_t)p, start = (uintptr_t)h->base;
    return h->base && a >= start + DETHEAP_HEADER && a < start + h->carved;
}

int detheap_is_placeholder(const void *heap) {
    uintptr_t u = (uintptr_t)heap;
    return u >= DETHEAP_PLACEHOLDER_FIRST && u <= DETHEAP_PLACEHOLDER_LAST && (u & 0xf) == 0;
}

static struct header *header_of(const void *p) {
    return (struct header *)((uint8_t *)p - DETHEAP_HEADER);
}

/* A system heap for a block outside the region: a placeholder's is the
 * process heap. */
static void *system_heap(const struct detheap *h, void *heap) {
    return detheap_is_placeholder(heap) ? h->sys.process_heap : heap;
}

static void *failed(struct detheap *h, uint32_t flags) {
    if (flags & DETHEAP_GENERATE_EXCEPTIONS) {
        h->sys.no_memory(h->sys.ctx);
    }
    return NULL;
}

/* A block of `n` bytes: the head of its class's free list, or carved after
 * the last one. */
static void *take(struct detheap *h, size_t n, int zero) {
    int c = detheap_class(n);
    if (c < 0 || (uint64_t)n > UINT32_MAX) {
        return NULL;
    }
    uint64_t class_size = class_sizes[c];
    h->sys.lock(h->sys.ctx);
    uint8_t *p = h->free_[c];
    if (p) {
        h->free_[c] = *(void **)p;
    } else {
        uint64_t end = (uint64_t)h->carved + DETHEAP_HEADER + class_size;
        if (end > h->size) {
            h->sys.unlock(h->sys.ctx);
            return NULL;
        }
        if (end > h->committed) {
            uint64_t to = (end + DETHEAP_COMMIT_STEP - 1) / DETHEAP_COMMIT_STEP * DETHEAP_COMMIT_STEP;
            if (to > h->size) {
                to = h->size;
            }
            if (!h->sys.commit(h->sys.ctx, h->base + h->committed, (size_t)to - h->committed)) {
                h->sys.unlock(h->sys.ctx);
                return NULL;
            }
            h->committed = (size_t)to;
        }
        p = h->base + h->carved + DETHEAP_HEADER;
        h->carved = (size_t)end;
    }
    h->sys.unlock(h->sys.ctx);
    struct header *hd = header_of(p);
    hd->class_ = (uint32_t)c;
    hd->size = (uint32_t)n;
    if (zero) {
        memset(p, 0, (size_t)class_size);
    }
    return p;
}

/* A block of the region to the head of its class's free list. */
static void give_back(struct detheap *h, void *p) {
    struct header *hd = header_of(p);
    h->sys.lock(h->sys.ctx);
    *(void **)p = h->free_[hd->class_];
    h->free_[hd->class_] = p;
    h->sys.unlock(h->sys.ctx);
}

/* The next placeholder, from 0x10 to 0xfff0 and round again: a placeholder
 * holds nothing of its own, so one used before serves as well. */
void *detheap_create(struct detheap *h) {
    h->sys.lock(h->sys.ctx);
    uintptr_t u = h->next_placeholder;
    h->next_placeholder = u >= DETHEAP_PLACEHOLDER_LAST ? DETHEAP_PLACEHOLDER_FIRST : u + 0x10;
    h->created++;
    h->sys.unlock(h->sys.ctx);
    return (void *)u;
}

int detheap_destroy(struct detheap *h, void *heap) {
    return detheap_is_placeholder(heap) ? 1 : h->sys.destroy(h->sys.ctx, heap);
}

void *detheap_alloc(struct detheap *h, void *heap, uint32_t flags, size_t n) {
    (void)heap;
    void *p = take(h, n, (flags & DETHEAP_ZERO_MEMORY) != 0);
    if (!p) {
        h->failed_allocs++;
        return failed(h, flags);
    }
    return p;
}

void *detheap_realloc(struct detheap *h, void *heap, uint32_t flags, void *p, size_t n) {
    int zero = (flags & DETHEAP_ZERO_MEMORY) != 0;
    if (detheap_owns(h, p)) {
        struct header *hd = header_of(p);
        if ((uint64_t)n <= class_sizes[hd->class_]) {
            if (zero && n > hd->size) {
                memset((uint8_t *)p + hd->size, 0, n - hd->size);
            }
            hd->size = (uint32_t)n;
            return p;
        }
        /* A failure returns null: only HeapAlloc raises (rule 3). */
        if (flags & DETHEAP_REALLOC_IN_PLACE_ONLY) {
            h->in_place_failures++;
            return NULL;
        }
        void *q = take(h, n, zero);
        if (!q) {
            h->failed_reallocs++;
            return NULL;
        }
        memcpy(q, p, hd->size < n ? hd->size : n);
        give_back(h, p);
        return q;
    }
    /* A block of a system heap: moved into the region, unless in place
     * only (or no block). */
    void *sys_heap = system_heap(h, heap);
    if ((flags & DETHEAP_REALLOC_IN_PLACE_ONLY) || !p) {
        return h->sys.realloc(h->sys.ctx, sys_heap, flags, p, n);
    }
    size_t old = h->sys.size(h->sys.ctx, sys_heap, 0, p);
    void *q = take(h, n, zero);
    if (!q) {
        h->failed_reallocs++;
        return NULL;
    }
    memcpy(q, p, old < n ? old : n);
    h->sys.free(h->sys.ctx, sys_heap, 0, p);
    h->moved++;
    return q;
}

int detheap_free(struct detheap *h, void *heap, uint32_t flags, void *p) {
    if (detheap_owns(h, p)) {
        give_back(h, p);
        return 1;
    }
    return h->sys.free(h->sys.ctx, system_heap(h, heap), flags, p);
}

size_t detheap_size(struct detheap *h, void *heap, uint32_t flags, const void *p) {
    if (detheap_owns(h, p)) {
        return header_of(p)->size;
    }
    return h->sys.size(h->sys.ctx, system_heap(h, heap), flags, p);
}

int detheap_validate(struct detheap *h, void *heap, uint32_t flags, const void *p) {
    if ((p && detheap_owns(h, p)) || detheap_is_placeholder(heap)) {
        return 1;
    }
    return h->sys.validate(h->sys.ctx, heap, flags, p);
}

size_t detheap_compact(struct detheap *h, void *heap, uint32_t flags) {
    return detheap_is_placeholder(heap) ? DETHEAP_COMPACT : h->sys.compact(h->sys.ctx, heap, flags);
}

int detheap_walk(const struct detheap *h, const void *heap, const void *previous,
                 struct detheap_walk *entry) {
    if (!detheap_is_placeholder(heap)) {
        return -1;
    }
    if (previous) {
        return 0;
    }
    entry->data = h->base;
    entry->size = h->size;
    entry->committed = h->size;
    return 1;
}
