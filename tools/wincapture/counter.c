/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The time-stamp counter on the frame clock: a DynamoRIO client, test-only,
 * that the driver runs the game under (drrun, capture.c). Every `rdtsc` and
 * `rdtscp` instruction executed in a module of the game's folder (the
 * client's one option) is replaced, the same way at every place, by a call
 * that returns a virtual counter:
 *
 *     COUNTER_BASE + clock_ms × COUNTER_PER_MS + n × COUNTER_STEP
 *
 * where clock_ms is the frame clock's milliseconds (frameclock.h, read from
 * the driver's shared memory) and n the counter reads substituted before
 * this one in the process. So the counter advances with the frame clock at
 * 3 GHz (a wait of 1,000 ms of the clock spans 3 × 10⁹ of it), every read
 * returns a larger value than the one before, and two runs that read it at
 * the same points of the frame clock read the same values
 * (docs/behavior/original-determinism.md section 7, rule 1). `rdtscp`'s
 * processor number (ecx) reads 0.
 *
 * Generic environment control (docs/clean-room.md): it transforms an
 * instruction wherever it occurs in the game's folder and records nothing
 * of the program but the total of reads substituted (in the shared
 * memory); it knows nothing of where or why the program reads the counter.
 *
 * No C runtime: only DynamoRIO's API and kernel32 (DynamoRIO loads the
 * client and its imports privately). */

#define WINDOWS
#define X86_32
#include "dr_api.h"

#include "frameclock.h"

/* An hour's worth of the counter at the clock's start, as the proxy's
 * performance counter starts an hour after start-up (frameclock.c). */
#define COUNTER_PER_MS 3000000ull
#define COUNTER_BASE (3600000ull * COUNTER_PER_MS)
/* Back-to-back reads 30 apart: 10 ns at 3 GHz. */
#define COUNTER_STEP 30ull

static struct frameclock *clock_;

/* The game's folder, lower case, ending in a backslash. */
static char folder[MAX_PATH + 1];
static size_t folder_len;

static char lower(char c) {
    return c >= 'A' && c <= 'Z' ? (char)(c - 'A' + 'a') : c;
}

static bool in_folder(const char *path) {
    if (!path) {
        return false;
    }
    /* A path in the NT or long-path form ("\??\", "\\?\"). */
    if ((path[0] == '\\') && (path[1] == '?' || path[1] == '\\') && path[2] == '?' &&
        path[3] == '\\') {
        path += 4;
    }
    for (size_t i = 0; i < folder_len; i++) {
        char c = lower(path[i]);
        if (c == '/') {
            c = '\\';
        }
        if (c != folder[i]) {
            return false;
        }
    }
    return true;
}

/* The call that replaces a counter read: the value into edx:eax (and 0
 * into ecx for rdtscp). */
static void substitute(uint rdtscp) {
    void *dc = dr_get_current_drcontext();
    dr_mcontext_t mc;
    mc.size = sizeof mc;
    mc.flags = DR_MC_INTEGER;
    dr_get_mcontext(dc, &mc);
    uint64_t n = (uint64_t)__atomic_fetch_add(&clock_->counter_reads, 1, __ATOMIC_SEQ_CST);
    uint64_t ms = frameclock_ticks(clock_) / FRAMECLOCK_TICKS_PER_MS;
    uint64_t v = COUNTER_BASE + ms * COUNTER_PER_MS + n * COUNTER_STEP;
    mc.xax = (reg_t)(uint32_t)v;
    mc.xdx = (reg_t)(uint32_t)(v >> 32);
    if (rdtscp) {
        mc.xcx = 0;
    }
    dr_set_mcontext(dc, &mc);
}

static dr_emit_flags_t block(void *dc, void *tag, instrlist_t *bb, bool for_trace,
                             bool translating) {
    (void)for_trace;
    (void)translating;
    instr_t *next;
    int checked = 0, ours = 0;
    for (instr_t *in = instrlist_first(bb); in; in = next) {
        next = instr_get_next(in);
        int op = instr_get_opcode(in);
        if (op != OP_rdtsc && op != OP_rdtscp) {
            continue;
        }
        if (!checked) {
            module_data_t *m = dr_lookup_module(dr_fragment_app_pc(tag));
            ours = m && in_folder(m->full_path);
            if (m) {
                dr_free_module_data(m);
            }
            checked = 1;
        }
        if (!ours) {
            break;
        }
        instrlist_set_translation_target(bb, instr_get_app_pc(in));
        dr_insert_clean_call(dc, bb, in, (void *)substitute, false, 1,
                             OPND_CREATE_INT32(op == OP_rdtscp));
        instrlist_set_translation_target(bb, NULL);
        instrlist_remove(bb, in);
        instr_destroy(dc, in);
    }
    return DR_EMIT_DEFAULT;
}

/* A failure the driver sees: the client not attached (its message on
 * drrun's error output, which the driver keeps). */
static void fail(const char *why) {
    dr_fprintf(STDERR, "volya counter client: %s\n", why);
    dr_abort_with_code(3);
}

DR_EXPORT void dr_client_main(client_id_t id, int argc, const char *argv[]) {
    (void)id;
    if (argc != 2) {
        fail("usage: counter.dll <game folder>");
    }
    size_t n = 0;
    for (const char *s = argv[1]; *s && n < MAX_PATH - 1; s++) {
        char c = lower(*s);
        folder[n++] = c == '/' ? '\\' : c;
    }
    if (n == 0 || n >= MAX_PATH - 1) {
        fail("the game folder is empty or too long");
    }
    if (folder[n - 1] != '\\') {
        folder[n++] = '\\';
    }
    folder_len = n;

    HANDLE map = OpenFileMappingA(FILE_MAP_ALL_ACCESS, FALSE, FRAMECLOCK_NAME);
    clock_ = map ? MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(struct frameclock))
                 : NULL;
    if (!clock_ || clock_->magic != FRAMECLOCK_MAGIC || clock_->version != FRAMECLOCK_VERSION) {
        fail("no frame clock of this version (start the game with volyacapture)");
    }
    dr_register_bb_event(block);
    __atomic_store_n(&clock_->counter_attached, 1, __ATOMIC_SEQ_CST);
}

/* No C runtime: the entry point DynamoRIO's loader calls. */
BOOL WINAPI DllMainCRTStartup(HINSTANCE dll, DWORD reason, LPVOID reserved) {
    (void)dll;
    (void)reason;
    (void)reserved;
    return TRUE;
}
