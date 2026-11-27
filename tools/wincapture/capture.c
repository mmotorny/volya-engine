/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* volyacapture.exe: the original's frames of the fidelity suite's views on
 * Windows (docs/testing.md, The original on Windows). It runs the shipped
 * game unmodified, from a copy of its install, on the frame clock of the
 * proxy winmm.dll beside it (frameclock.c, with its deterministic heap,
 * detheap.c), under DynamoRIO's drrun with the counter client (counter.c,
 * dynamorio\ beside it) until its first frame is held, in every mode that
 * runs the game (docs/behavior/capture-recipe.md), and steers it by its normal
 * inputs (docs/behavior/steering-the-original.md): its command line, its
 * configuration file, camera tracks in its saves folder, and keys sent with
 * SendInput while a frame is held, so the game reads each at the same frame
 * in every run. The procedure is the container capture's, in frames
 * (tools/xtask/src/original/views.rs before #760, on its frame clock):
 *
 *   volyacapture check [--runs N] [--game DIR] [--work DIR]
 *       the title menu reached with the clock held and stepped, its view
 *       captured, quit from its console; N runs (default 2) compared.
 *   volyacapture full [--runs N] [--game DIR] [--work DIR]
 *       every view: the title menu, then a New Game started from it.
 *   volyacapture selftest [--work DIR]
 *       drrun and the counter client on this machine, with counterprobe.exe
 *       of ours in place of the game: natively, then under drrun.
 *   volyacapture audio [--audio-device NAME] [--runs N] [--game DIR] [--work DIR]
 *       every walk of the audio-fidelity suite with sound on, its audio
 *       output recorded by loopback (loopback.c) from the default output
 *       device (or the one NAME names), and its frames where the player
 *       starts and ends kept; N runs (default 1).
 *
 * Its plan, plan.txt beside it, comes from tests/fidelity/views.rs
 * (`xtask wincapture plan`): the views, their frames, the configuration
 * file's settings and the camera tracks. Everything it writes is below
 * the work folder (default %TEMP%\volya-engine-capture): the copy of the install
 * (game\), the game's user-data folder (userdata\, made afresh for each
 * run, named by the copy's fsgame.ltx) and each run's outputs
 * (runs\<mode>-<n>\: fidelity\<view>.png and its files, log.txt,
 * events.txt, hashes.txt), and runs\<mode>-summary.txt comparing the
 * runs. The frames are game assets: they stay on this machine. What it
 * writes and prints names no folder of the machine: the install is
 * <game folder>, the work folder <work>, the runner's <runner> (redact.h). */

#define WIN32_LEAN_AND_MEAN
#define _WIN32_WINNT 0x0a00 /* Windows 10: BCryptHash */
#include <windows.h>
#include <bcrypt.h>
#include <mmsystem.h>
#include <tlhelp32.h>

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "detheap.h"
#include "frameclock.h"
#include "loopback.h"
#include "redact.h"

/* --- Constants of the procedure (views.rs's, in frames) ----------------- */

/* Frames between presses of the quit key while the logo sequence may play,
 * frames in a row with the title menu's corners lit that tell it has
 * opened, and the frames the sequence may take. */
#define SKIP_EVERY 25
#define MENU_LOOKS 3
#define MENU_TIMEOUT 5000

/* Frames between looks while the level loads, and the most it may take
 * before its intro shows; frames after the intro's quit key that it is
 * pressed again, and the presses it gets. */
#define LOAD_EVERY 10
#define LOAD_TIMEOUT 20000
#define INTRO_SKIP_RETRY 100
#define INTRO_SKIPS 3

/* A track's cycles (sections 4.2, 4.2.2); its precache's frames (50, from
 * the frame after its command) with one to spare; frames after it for the
 * view to settle (glows fade in). */
#define TRACK_CYCLES 4
#define PRECACHE 51
#define SETTLE 50

/* Frames the camera recorder gets before its F12 (section 5.1: two or
 * more); frames from the F12 to the screenshot's frame (the key read in
 * the next frame, the frame after drawn without the HUD); frames the
 * screenshot may take to be written. */
#define RECORDER_START 5
#define F12_TO_FRAME 2
#define SCREENSHOT_TIMEOUT 100
#define RECORDER "demo_record recorder"

/* Frames each key takes: pressed at a held frame, released at the next. */
#define KEY_FRAMES 2

/* Real time: milliseconds after a key is sent before the frame is let go
 * (the input reaches the game's queue), seconds a frame may take, seconds
 * the game gets to load the proxy, and to end after `quit`. */
#define INPUT_SETTLE_MS 30
#define FRAME_TIMEOUT 300
#define FIRST_FRAME_TIMEOUT 120
#define WATCHDOG 60
#define FRONT_CHECK_MS 100
#define ATTACH_TIMEOUT 60
#define QUIT_WAIT 60

/* Real milliseconds each frame of a walk takes at least (its recording's
 * frames), so a sound the game starts in a frame's update reaches the
 * output before the next frame is let run (shared-mode output latency is
 * tens of ms: believed) and each onset falls in its own frame's time. */
#define PACE_MS 200

/* Rows of the intro's letterbox looked at, top and bottom. */
#define BAR_ROWS 4

/* Scan codes (set 1) of the keys sent. */
#define SC_ESCAPE 0x01
#define SC_RETURN 0x1c
#define SC_GRAVE 0x29
#define SC_LSHIFT 0x2a
#define SC_F12 0x58

/* --- Small helpers ------------------------------------------------------ */

static FILE *log_file;
static ULONGLONG started_ms;

static void say(const char *fmt, ...) {
    va_list ap;
    char line[2048];
    va_start(ap, fmt);
    vsnprintf(line, sizeof line, fmt, ap);
    va_end(ap);
    redact(line, sizeof line);
    double t = (GetTickCount64() - started_ms) / 1000.0;
    printf("%8.1f s  %s\n", t, line);
    fflush(stdout);
    if (log_file) {
        fprintf(log_file, "%8.1f s  %s\n", t, line);
        fflush(log_file);
    }
}

static char failure[2048];

/* The bisect's toggles (docs/testing.md, The original on Windows), off by
 * default: the recipe (docs/behavior/capture-recipe.md) without its
 * deterministic heap (--no-heap), or with the client opening the shared
 * memory through kernel32 as before its rule 2 (--kernel32-client). */
static int no_heap, kernel32_client;

static int fail(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(failure, sizeof failure, fmt, ap);
    va_end(ap);
    redact(failure, sizeof failure);
    say("FAILED: %s", failure);
    return 0;
}

static void join(char *out, size_t n, const char *a, const char *b) {
    snprintf(out, n, "%s\\%s", a, b);
}

static int make_dirs(const char *path) {
    char p[MAX_PATH];
    snprintf(p, sizeof p, "%s", path);
    for (char *s = p + 3; *s; s++) {
        if (*s == '\\') {
            *s = 0;
            CreateDirectoryA(p, NULL);
            *s = '\\';
        }
    }
    return CreateDirectoryA(p, NULL) || GetLastError() == ERROR_ALREADY_EXISTS;
}

static int read_file(const char *path, uint8_t **data, size_t *size) {
    FILE *f = fopen(path, "rb");
    if (!f) {
        return 0;
    }
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    *data = malloc(n > 0 ? (size_t)n : 1);
    *size = n > 0 ? fread(*data, 1, (size_t)n, f) : 0;
    fclose(f);
    return n >= 0 && *size == (size_t)n;
}

static int write_file(const char *path, const void *data, size_t size) {
    FILE *f = fopen(path, "wb");
    if (!f) {
        return fail("cannot write %s", path);
    }
    size_t w = fwrite(data, 1, size, f);
    int ok = fclose(f) == 0 && w == size;
    return ok ? 1 : fail("cannot write %s", path);
}

static void remove_tree(const char *dir);

/* Every earlier invocation's outputs of `mode`: runs\<mode>-summary.txt
 * and each runs\<mode>-<n>\, so what the runs folder holds of the mode
 * after an invocation, failed or not, is that invocation's. */
static void clear_outputs(const char *work, const char *mode) {
    char path[MAX_PATH], pattern[MAX_PATH];
    snprintf(path, sizeof path, "%s\\runs\\%s-summary.txt", work, mode);
    DeleteFileA(path);
    snprintf(pattern, sizeof pattern, "%s\\runs\\%s-*", work, mode);
    WIN32_FIND_DATAA fd;
    HANDLE h = FindFirstFileA(pattern, &fd);
    if (h == INVALID_HANDLE_VALUE) {
        return;
    }
    do {
        if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
            snprintf(path, sizeof path, "%s\\runs\\%s", work, fd.cFileName);
            remove_tree(path);
        }
    } while (FindNextFileA(h, &fd));
    FindClose(h);
}

/* Removes a folder and everything in it. */
static void remove_tree(const char *dir) {
    char pattern[MAX_PATH], path[MAX_PATH];
    WIN32_FIND_DATAA fd;
    join(pattern, sizeof pattern, dir, "*");
    HANDLE h = FindFirstFileA(pattern, &fd);
    if (h != INVALID_HANDLE_VALUE) {
        do {
            if (!strcmp(fd.cFileName, ".") || !strcmp(fd.cFileName, "..")) {
                continue;
            }
            join(path, sizeof path, dir, fd.cFileName);
            if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
                remove_tree(path);
            } else {
                SetFileAttributesA(path, FILE_ATTRIBUTE_NORMAL);
                DeleteFileA(path);
            }
        } while (FindNextFileA(h, &fd));
        FindClose(h);
    }
    RemoveDirectoryA(dir);
}

/* SHA-256 of `data`, as 64 hex digits. */
static void sha256_hex(const void *data, size_t size, char out[65]) {
    uint8_t digest[32] = {0};
    BCRYPT_ALG_HANDLE alg;
    if (BCryptOpenAlgorithmProvider(&alg, BCRYPT_SHA256_ALGORITHM, NULL, 0) == 0) {
        BCryptHash(alg, NULL, 0, (PUCHAR)data, (ULONG)size, digest, 32);
        BCryptCloseAlgorithmProvider(alg, 0);
    }
    for (int i = 0; i < 32; i++) {
        sprintf(out + i * 2, "%02x", digest[i]);
    }
}

/* --- PNG (stored deflate) and TGA --------------------------------------- */

static uint32_t crc_table[256];

static uint32_t crc32_of(const uint8_t *p, size_t n, uint32_t crc) {
    if (!crc_table[1]) {
        for (uint32_t i = 0; i < 256; i++) {
            uint32_t c = i;
            for (int k = 0; k < 8; k++) {
                c = c & 1 ? 0xedb88320u ^ (c >> 1) : c >> 1;
            }
            crc_table[i] = c;
        }
    }
    crc = ~crc;
    for (size_t i = 0; i < n; i++) {
        crc = crc_table[(crc ^ p[i]) & 0xff] ^ (crc >> 8);
    }
    return ~crc;
}

struct buf {
    uint8_t *p;
    size_t n, cap;
};

static void put(struct buf *b, const void *data, size_t n) {
    if (b->n + n > b->cap) {
        b->cap = (b->n + n) * 2;
        b->p = realloc(b->p, b->cap);
    }
    memcpy(b->p + b->n, data, n);
    b->n += n;
}

static void put_be32(struct buf *b, uint32_t v) {
    uint8_t x[4] = {(uint8_t)(v >> 24), (uint8_t)(v >> 16), (uint8_t)(v >> 8), (uint8_t)v};
    put(b, x, 4);
}

static void png_chunk(struct buf *b, const char *kind, const uint8_t *data, size_t n) {
    put_be32(b, (uint32_t)n);
    size_t at = b->n;
    put(b, kind, 4);
    put(b, data, n);
    put_be32(b, crc32_of(b->p + at, n + 4, 0));
}

/* An 8-bit RGB PNG of `rgb`, its image data in stored deflate blocks, so
 * the same pixels give the same file. */
static int write_png(const char *path, uint32_t w, uint32_t h, const uint8_t *rgb) {
    struct buf raw = {0}, z = {0}, png = {0};
    for (uint32_t y = 0; y < h; y++) {
        uint8_t filter = 0;
        put(&raw, &filter, 1);
        put(&raw, rgb + (size_t)y * w * 3, (size_t)w * 3);
    }
    uint8_t head[2] = {0x78, 0x01};
    put(&z, head, 2);
    uint32_t a = 1, s = 0;
    for (size_t i = 0; i < raw.n; i++) {
        a = (a + raw.p[i]) % 65521;
        s = (s + a) % 65521;
    }
    for (size_t at = 0; at < raw.n || at == 0;) {
        size_t n = raw.n - at > 65535 ? 65535 : raw.n - at;
        uint8_t block[5] = {(uint8_t)(at + n == raw.n), (uint8_t)n, (uint8_t)(n >> 8),
                            (uint8_t)~n, (uint8_t)(~n >> 8)};
        put(&z, block, 5);
        put(&z, raw.p + at, n);
        at += n;
        if (n == 0) {
            break;
        }
    }
    put_be32(&z, (s << 16) | a);
    static const uint8_t sig[8] = {0x89, 'P', 'N', 'G', '\r', '\n', 0x1a, '\n'};
    put(&png, sig, 8);
    uint8_t ihdr[13] = {(uint8_t)(w >> 24), (uint8_t)(w >> 16), (uint8_t)(w >> 8), (uint8_t)w,
                        (uint8_t)(h >> 24), (uint8_t)(h >> 16), (uint8_t)(h >> 8), (uint8_t)h,
                        8, 2, 0, 0, 0};
    png_chunk(&png, "IHDR", ihdr, 13);
    png_chunk(&png, "IDAT", z.p, z.n);
    png_chunk(&png, "IEND", NULL, 0);
    int ok = write_file(path, png.p, png.n);
    free(raw.p);
    free(z.p);
    free(png.p);
    return ok;
}

/* The pixels of a PNG this program wrote (stored deflate only), or null. */
static uint8_t *read_png(const char *path, uint32_t *w, uint32_t *h) {
    uint8_t *f;
    size_t n;
    if (!read_file(path, &f, &n) || n < 33) {
        return NULL;
    }
    struct buf z = {0};
    size_t at = 8;
    *w = (uint32_t)f[16] << 24 | f[17] << 16 | f[18] << 8 | f[19];
    *h = (uint32_t)f[20] << 24 | f[21] << 16 | f[22] << 8 | f[23];
    while (at + 12 <= n) {
        uint32_t len = (uint32_t)f[at] << 24 | f[at + 1] << 16 | f[at + 2] << 8 | f[at + 3];
        if (at + 12 + len > n) {
            break;
        }
        if (!memcmp(f + at + 4, "IDAT", 4)) {
            put(&z, f + at + 8, len);
        }
        at += 12 + len;
    }
    size_t row = (size_t)*w * 3, need = (row + 1) * *h;
    uint8_t *rgb = malloc(row * *h + 1);
    size_t got = 0, zi = 2;
    uint8_t *raw = malloc(need + 1);
    while (zi + 5 <= z.n && got < need) {
        size_t len = z.p[zi + 1] | z.p[zi + 2] << 8;
        if (zi + 5 + len > z.n || got + len > need) {
            break;
        }
        memcpy(raw + got, z.p + zi + 5, len);
        got += len;
        zi += 5 + len;
    }
    free(z.p);
    free(f);
    if (got != need) {
        free(raw);
        free(rgb);
        return NULL;
    }
    for (uint32_t y = 0; y < *h; y++) {
        memcpy(rgb + y * row, raw + y * (row + 1) + 1, row);
    }
    free(raw);
    return rgb;
}

/* A true-colour TGA (types 2 and 10, 24 or 32 bits) as RGB, top row first;
 * null for anything else or a file shorter than its pixels. */
static uint8_t *tga_rgb(const uint8_t *t, size_t n, uint32_t *w, uint32_t *h) {
    if (n < 18) {
        return NULL;
    }
    unsigned id = t[0], map = t[1], kind = t[2], bits = t[16], desc = t[17];
    *w = t[12] | t[13] << 8;
    *h = t[14] | t[15] << 8;
    if (map || (kind != 2 && kind != 10) || (bits != 24 && bits != 32) || (desc & 0x10) || !*w ||
        !*h) {
        return NULL;
    }
    size_t bytes = bits / 8, count = (size_t)*w * *h, at = 18 + id, done = 0;
    uint8_t *rgb = malloc(count * 3);
    while (done < count) {
        size_t run = 1;
        int repeat = 0;
        if (kind == 10) {
            if (at >= n) {
                break;
            }
            run = (t[at] & 0x7f) + 1;
            repeat = t[at] & 0x80;
            at++;
        } else {
            run = count;
        }
        if (done + run > count) {
            break;
        }
        for (size_t i = 0; i < run; i++) {
            size_t src = at + (repeat ? 0 : i * bytes);
            if (src + bytes > n) {
                free(rgb);
                return NULL;
            }
            uint8_t *d = rgb + (done + i) * 3;
            d[0] = t[src + 2];
            d[1] = t[src + 1];
            d[2] = t[src];
        }
        at += repeat ? bytes : run * bytes;
        done += run;
    }
    if (done != count) {
        free(rgb);
        return NULL;
    }
    if (!(desc & 0x20)) {
        size_t row = (size_t)*w * 3;
        uint8_t *tmp = malloc(row);
        for (uint32_t y = 0; y < *h / 2; y++) {
            memcpy(tmp, rgb + y * row, row);
            memcpy(rgb + y * row, rgb + (*h - 1 - y) * row, row);
            memcpy(rgb + (*h - 1 - y) * row, tmp, row);
        }
        free(tmp);
    }
    return rgb;
}

/* --- The plan ----------------------------------------------------------- */

enum place { TITLE_MENU, NEW_GAME_START, NEW_GAME_TRACK };

struct view {
    char name[64];
    enum place place;
    int64_t frame; /* of the title menu, or of play */
    uint8_t *track;
    size_t track_size;
};

/* A walk (tests/fidelity/walks.rs): keys held for frames, one segment
 * after the other, from frame `start` of play. */
struct segment {
    WORD keys[4];
    int nkeys;
    int64_t frames;
};

struct walk {
    char name[64];
    int64_t start, tail, preroll;
    struct segment segments[64];
    int nsegments;
};

static struct {
    int64_t frame_ms, spin, epoch, sleep_advances, start_quantum_ms, first_frame_ms,
        prestart_sleeps;
    uint32_t width, height;
    struct buf ltx;       /* the views' configuration file, CR LF lines */
    struct buf audio_ltx; /* the walks' */
    struct view views[32];
    int nviews;
    struct walk walks[8];
    int nwalks;
} plan;

static int hex_word(const char *s, uint32_t *v) {
    char *end;
    *v = (uint32_t)strtoul(s, &end, 16);
    return end != s;
}

static int read_plan(const char *path) {
    FILE *f = fopen(path, "r");
    if (!f) {
        return fail("no plan: %s", path);
    }
    char line[4096];
    int ok = 0;
    while (fgets(line, sizeof line, f)) {
        line[strcspn(line, "\r\n")] = 0;
        char *word = strtok(line, " ");
        if (!word || word[0] == '#') {
            continue;
        }
        if (!strcmp(word, "volya-capture-plan")) {
            ok = atoi(strtok(NULL, " ")) == 1;
        } else if (!strcmp(word, "frame_ms")) {
            plan.frame_ms = atoll(strtok(NULL, " "));
        } else if (!strcmp(word, "spin")) {
            plan.spin = atoll(strtok(NULL, " "));
        } else if (!strcmp(word, "sleep_advances")) {
            plan.sleep_advances = atoll(strtok(NULL, " "));
        } else if (!strcmp(word, "prestart_sleeps")) {
            plan.prestart_sleeps = atoll(strtok(NULL, " "));
        } else if (!strcmp(word, "start_quantum_ms")) {
            plan.start_quantum_ms = atoll(strtok(NULL, " "));
        } else if (!strcmp(word, "first_frame_ms")) {
            plan.first_frame_ms = atoll(strtok(NULL, " "));
        } else if (!strcmp(word, "epoch")) {
            plan.epoch = atoll(strtok(NULL, " "));
        } else if (!strcmp(word, "size")) {
            plan.width = (uint32_t)atoi(strtok(NULL, " "));
            plan.height = (uint32_t)atoi(strtok(NULL, " "));
        } else if (!strcmp(word, "ltx")) {
            /* A line of the configuration file, as it is. */
            char *rest = strtok(NULL, "");
            if (rest) {
                put(&plan.ltx, rest, strlen(rest));
            }
            put(&plan.ltx, "\r\n", 2);
        } else if (!strcmp(word, "audio_ltx")) {
            char *rest = strtok(NULL, "");
            if (rest) {
                put(&plan.audio_ltx, rest, strlen(rest));
            }
            put(&plan.audio_ltx, "\r\n", 2);
        } else if (!strcmp(word, "walk") && plan.nwalks < 8) {
            /* walk NAME START TAIL PREROLL KEYS:FRAMES... */
            struct walk *w = &plan.walks[plan.nwalks++];
            char *name = strtok(NULL, " "), *start = strtok(NULL, " "), *tail = strtok(NULL, " "),
                 *preroll = strtok(NULL, " ");
            if (!name || !start || !tail || !preroll) {
                break;
            }
            snprintf(w->name, sizeof w->name, "%s", name);
            w->start = atoll(start);
            w->tail = atoll(tail);
            w->preroll = atoll(preroll);
            char *seg;
            while ((seg = strtok(NULL, " ")) && w->nsegments < 64) {
                struct segment *g = &w->segments[w->nsegments++];
                char *colon = strchr(seg, ':');
                if (!colon) {
                    fclose(f);
                    return fail("plan: %s: segment %s has no frames", name, seg);
                }
                *colon = 0;
                g->frames = atoll(colon + 1);
                for (char *k = seg; *k && g->nkeys < 4;) {
                    char *end;
                    g->keys[g->nkeys++] = (WORD)strtoul(k, &end, 16);
                    k = *end == '+' ? end + 1 : end;
                    if (end == k && *k) {
                        break;
                    }
                }
            }
        } else if (!strcmp(word, "view") && plan.nviews < 32) {
            struct view *v = &plan.views[plan.nviews++];
            char *name = strtok(NULL, " "), *place = strtok(NULL, " "), *frame = strtok(NULL, " ");
            if (!name || !place || !frame) {
                break;
            }
            snprintf(v->name, sizeof v->name, "%s", name);
            v->place = !strcmp(place, "title-menu") ? TITLE_MENU
                       : !strcmp(place, "start")    ? NEW_GAME_START
                                                    : NEW_GAME_TRACK;
            v->frame = atoll(frame);
            if (v->place == NEW_GAME_TRACK) {
                /* track KEYS W0 … W15: KEYS equal 64-byte keys, the
                 * little-endian floats' bits in hex. */
                int keys = atoi(strtok(NULL, " "));
                uint8_t key[64];
                for (int i = 0; i < 16; i++) {
                    uint32_t word32;
                    char *s = strtok(NULL, " ");
                    if (!s || !hex_word(s, &word32)) {
                        fclose(f);
                        return fail("plan: %s: a track needs 16 words", name);
                    }
                    memcpy(key + i * 4, &word32, 4);
                }
                v->track_size = (size_t)keys * 64;
                v->track = malloc(v->track_size);
                for (int k = 0; k < keys; k++) {
                    memcpy(v->track + k * 64, key, 64);
                }
            }
        }
    }
    fclose(f);
    if (!ok || !plan.frame_ms || !plan.width || !plan.nviews || !plan.ltx.n) {
        return fail("plan: %s is not a capture plan of version 1", path);
    }
    return 1;
}

/* --- The install's copy ------------------------------------------------- */

struct tally {
    uint64_t files, bytes;
};

static void scan(const char *dir, struct tally *t) {
    char pattern[MAX_PATH], path[MAX_PATH];
    WIN32_FIND_DATAA fd;
    join(pattern, sizeof pattern, dir, "*");
    HANDLE h = FindFirstFileA(pattern, &fd);
    if (h == INVALID_HANDLE_VALUE) {
        return;
    }
    do {
        if (!strcmp(fd.cFileName, ".") || !strcmp(fd.cFileName, "..")) {
            continue;
        }
        join(path, sizeof path, dir, fd.cFileName);
        if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
            scan(path, t);
        } else {
            t->files++;
            t->bytes += (uint64_t)fd.nFileSizeHigh << 32 | fd.nFileSizeLow;
        }
    } while (FindNextFileA(h, &fd));
    FindClose(h);
}

static int copy_tree(const char *from, const char *to) {
    char pattern[MAX_PATH], src[MAX_PATH], dst[MAX_PATH];
    WIN32_FIND_DATAA fd;
    if (!make_dirs(to)) {
        return fail("cannot make %s", to);
    }
    join(pattern, sizeof pattern, from, "*");
    HANDLE h = FindFirstFileA(pattern, &fd);
    if (h == INVALID_HANDLE_VALUE) {
        return fail("cannot read %s", from);
    }
    int ok = 1;
    do {
        if (!strcmp(fd.cFileName, ".") || !strcmp(fd.cFileName, "..")) {
            continue;
        }
        join(src, sizeof src, from, fd.cFileName);
        join(dst, sizeof dst, to, fd.cFileName);
        if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
            ok = copy_tree(src, dst);
        } else {
            SetFileAttributesA(dst, FILE_ATTRIBUTE_NORMAL);
            ok = CopyFileA(src, dst, FALSE) ? 1 : fail("cannot copy %s (%lu)", src, GetLastError());
            SetFileAttributesA(dst, FILE_ATTRIBUTE_NORMAL);
        }
    } while (ok && FindNextFileA(h, &fd));
    FindClose(h);
    return ok;
}

/* The install copied to `copy` unless the copy there was made from the
 * same files (by path, count and size). */
static int copy_install(const char *game, const char *copy) {
    struct tally t = {0};
    char marker[MAX_PATH], want[1024];
    scan(game, &t);
    if (!t.files) {
        return fail("no game files in %s", game);
    }
    snprintf(want, sizeof want, "%s\n%llu files\n%llu bytes\n", game, (unsigned long long)t.files,
             (unsigned long long)t.bytes);
    join(marker, sizeof marker, copy, "volya-copy.txt");
    uint8_t *have;
    size_t n;
    if (read_file(marker, &have, &n) && n == strlen(want) && !memcmp(have, want, n)) {
        free(have);
        say("The install's copy in %s is current (%llu files)", copy, (unsigned long long)t.files);
        return 1;
    }
    say("Copying the install (%llu files, %.1f GB) to %s", (unsigned long long)t.files,
        t.bytes / 1e9, copy);
    remove_tree(copy);
    if (!copy_tree(game, copy)) {
        return 0;
    }
    struct tally c = {0};
    scan(copy, &c);
    if (c.files != t.files || c.bytes != t.bytes) {
        return fail("the copy holds %llu files, %llu bytes; the install %llu files, %llu bytes",
                    (unsigned long long)c.files, (unsigned long long)c.bytes,
                    (unsigned long long)t.files, (unsigned long long)t.bytes);
    }
    say("Copied: %llu files, %llu bytes", (unsigned long long)c.files, (unsigned long long)c.bytes);
    return write_file(marker, want, strlen(want));
}

/* The copy's fsgame.ltx: the install's, its user-data folder (the line of
 * $app_data_root$, its last field) replaced by `userdata`
 * (gog-install-additions.md section 2). */
static int write_fsgame(const char *game, const char *copy, const char *userdata) {
    char path[MAX_PATH];
    uint8_t *text;
    size_t n;
    join(path, sizeof path, game, "fsgame.ltx");
    if (!read_file(path, &text, &n)) {
        return fail("cannot read %s", path);
    }
    struct buf out = {0};
    size_t at = 0;
    int found = 0;
    while (at < n) {
        size_t end = at;
        while (end < n && text[end] != '\n') {
            end++;
        }
        size_t len = end - at;
        if (len >= 15 && !memcmp(text + at, "$app_data_root$", 15)) {
            size_t bar = len;
            while (bar > 0 && text[at + bar - 1] != '|') {
                bar--;
            }
            if (!bar) {
                free(text);
                return fail("fsgame.ltx: no field in its $app_data_root$ line");
            }
            put(&out, text + at, bar);
            put(&out, " ", 1);
            put(&out, userdata, strlen(userdata));
            if (len && text[at + len - 1] == '\r') {
                put(&out, "\r", 1);
            }
            found = 1;
        } else {
            put(&out, text + at, len);
        }
        if (end < n) {
            put(&out, "\n", 1);
        }
        at = end + 1;
    }
    free(text);
    join(path, sizeof path, copy, "fsgame.ltx");
    SetFileAttributesA(path, FILE_ATTRIBUTE_NORMAL);
    int ok = found ? write_file(path, out.p, out.n) : fail("fsgame.ltx: no $app_data_root$ line");
    free(out.p);
    return ok;
}

/* --- The run ------------------------------------------------------------ */

struct run {
    char dir[MAX_PATH], out[MAX_PATH], userdata[MAX_PATH], screenshots[MAX_PATH];
    HANDLE process, job, map;
    HANDLE drrun; /* DynamoRIO's drrun, which started the game (process) */
    DWORD pid;
    struct frameclock *clock;
    int64_t frame;
    uint32_t w, h;
    uint8_t *pixels; /* the last look, B G R X */
    FILE *events;
    FILE *hashes;
    ULONGLONG front_checked; /* when keep_in_front last looked (real ms) */
    int audio;               /* the walks' run, its audio recorded by `loopback` */
    struct loopback *loopback;
};

static int ended(struct run *r) {
    return WaitForSingleObject(r->process, 0) == WAIT_OBJECT_0;
}

static int64_t load(volatile int64_t *p) {
    return InterlockedCompareExchange64((volatile LONGLONG *)p, 0, 0);
}

static void store(volatile int64_t *p, int64_t v) {
    InterlockedExchange64((volatile LONGLONG *)p, v);
}

static void event(struct run *r, const char *what) {
    char line[1024];
    snprintf(line, sizeof line, "%s", what);
    redact(line, sizeof line);
    say("frame %6lld  %s", (long long)r->frame, line);
    fprintf(r->events, "%lld %s\n", (long long)r->frame, line);
    fflush(r->events);
}

/* The frame clock's figures, for a failure's message. */
static const char *clock_state(struct run *r) {
    static char s[512];
    snprintf(s, sizeof s,
             "attached %lld, hooked %lld, frames %lld, held %lld, hold %lld, main reads %lld, "
             "other reads %lld, extra %lld ms, sleeps %lld (%lld ms); see frameclock.log",
             (long long)load(&r->clock->attached), (long long)load(&r->clock->hooked),
             (long long)load(&r->clock->frames), (long long)load(&r->clock->held),
             (long long)load(&r->clock->hold), (long long)load(&r->clock->reads),
             (long long)load(&r->clock->other_reads),
             (long long)(load(&r->clock->extra) / (int64_t)FRAMECLOCK_TICKS_PER_MS),
             (long long)load(&r->clock->sleeps),
             (long long)(load(&r->clock->sleep_ticks) / (int64_t)FRAMECLOCK_TICKS_PER_MS));
    return s;
}

static void keep_in_front(struct run *r);

/* Lets the game run until it has presented `frame` and holds it after it;
 * with `look`, the frame's back buffer is copied into r->pixels. */
static int advance(struct run *r, int64_t frame, int look) {
    if (frame <= r->frame) {
        return fail("frame %lld: the game is held after frame %lld", (long long)frame,
                    (long long)r->frame);
    }
    store(&r->clock->capture, look ? frame : 0);
    keep_in_front(r);
    store(&r->clock->hold, frame);
    /* The first frame comes within seconds of the start (measured on the PC). */
    ULONGLONG until = GetTickCount64() + (r->frame ? FRAME_TIMEOUT : FIRST_FRAME_TIMEOUT) * 1000ull;
    ULONGLONG progressed = GetTickCount64();
    int64_t seen[4] = {0};
    while (load(&r->clock->held) != frame) {
        if (ended(r)) {
            DWORD code = 0;
            GetExitCodeProcess(r->process, &code);
            return fail("the game ended (exit code 0x%lx) before frame %lld", code, (long long)frame);
        }
        /* Progress: a frame, or a clock read or sleep by the main thread
         * (other threads read the clock while the main one is stuck too).
         * None for WATCHDOG seconds and the game is stuck. */
        int64_t now[4] = {load(&r->clock->frames), load(&r->clock->reads),
                          load(&r->clock->sleeps), load(&r->clock->extra)};
        if (memcmp(now, seen, sizeof now)) {
            memcpy(seen, now, sizeof now);
            progressed = GetTickCount64();
        }
        if (GetTickCount64() - r->front_checked >= FRONT_CHECK_MS) {
            keep_in_front(r);
        }
        if (GetTickCount64() > until || GetTickCount64() - progressed > WATCHDOG * 1000ull) {
            return fail("stuck waiting for frame %lld (%s): %s", (long long)frame,
                        GetTickCount64() > until ? "frame timeout" : "no progress for the watchdog",
                        clock_state(r));
        }
        Sleep(1);
    }
    r->frame = frame;
    if (look) {
        if (load(&r->clock->captured) != frame || load(&r->clock->capture_error)) {
            return fail("frame %lld: its back buffer was not copied (error 0x%llx)", (long long)frame,
                        (unsigned long long)load(&r->clock->capture_error));
        }
        r->w = (uint32_t)load(&r->clock->width);
        r->h = (uint32_t)load(&r->clock->height);
        memcpy(r->pixels, (const void *)r->clock->pixels, (size_t)r->w * r->h * 4);
    }
    return 1;
}

/* The game's main window: the visible top-level window of its process. */
static HWND found_window;

static BOOL CALLBACK window_of(HWND hwnd, LPARAM pid) {
    DWORD p;
    GetWindowThreadProcessId(hwnd, &p);
    if (p == (DWORD)pid && IsWindowVisible(hwnd)) {
        found_window = hwnd;
        return FALSE;
    }
    return TRUE;
}

/* Keys go to the foreground window: only ever to the game's. */
static int game_in_front(struct run *r) {
    DWORD p = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), &p);
    if (p == r->pid) {
        return 1;
    }
    found_window = NULL;
    EnumWindows(window_of, (LPARAM)r->pid);
    if (found_window) {
        SetForegroundWindow(found_window);
        Sleep(200);
        GetWindowThreadProcessId(GetForegroundWindow(), &p);
    }
    return p == r->pid ? 1 : fail("the game's window is not in the foreground: no key is sent");
}

/* Rule 3 of original-determinism.md section 7: the game's window stays
 * active for the whole run, so its loop never takes the branches of an
 * inactive window. Once it has a window, the driver brings it back to the
 * front whenever another took it; each time from the first frame on is an
 * event (runs that lost it differ). */
static void keep_in_front(struct run *r) {
    r->front_checked = GetTickCount64();
    DWORD p = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), &p);
    if (p == r->pid) {
        return;
    }
    found_window = NULL;
    EnumWindows(window_of, (LPARAM)r->pid);
    if (!found_window) {
        return;
    }
    SetForegroundWindow(found_window);
    Sleep(200);
    GetWindowThreadProcessId(GetForegroundWindow(), &p);
    /* Before the first frame the game's window may not have come to the
     * front by itself yet. */
    if (r->frame < 1) {
        return;
    }
    event(r, p == r->pid ? "another window had the foreground: the game's brought back"
                         : "another window has the foreground: the game's not brought back");
}

static int send_key(struct run *r, WORD scan, int up) {
    if (!game_in_front(r)) {
        return 0;
    }
    INPUT in = {0};
    in.type = INPUT_KEYBOARD;
    /* 0xe0__: an extended key (the arrows). */
    in.ki.wScan = scan & 0xff;
    in.ki.dwFlags = KEYEVENTF_SCANCODE | (up ? KEYEVENTF_KEYUP : 0) |
                    ((scan >> 8) == 0xe0 ? KEYEVENTF_EXTENDEDKEY : 0);
    if (SendInput(1, &in, sizeof in) != 1) {
        return fail("SendInput failed (%lu)", GetLastError());
    }
    return 1;
}

/* A key pressed at the held frame and released at the next, with Shift
 * when `shift`: KEY_FRAMES frames. */
static int press(struct run *r, WORD scan, int shift) {
    if ((shift && !send_key(r, SC_LSHIFT, 0)) || !send_key(r, scan, 0)) {
        return 0;
    }
    Sleep(INPUT_SETTLE_MS);
    if (!advance(r, r->frame + 1, 0)) {
        return 0;
    }
    if (!send_key(r, scan, 1) || (shift && !send_key(r, SC_LSHIFT, 1))) {
        return 0;
    }
    Sleep(INPUT_SETTLE_MS);
    return advance(r, r->frame + 1, 0);
}

/* A US keyboard's scan code of a character, and whether it takes Shift. */
static int scan_of(char c, WORD *scan, int *shift) {
    static const char *rows[] = {"1234567890-=", "qwertyuiop[]", "asdfghjkl;'", "zxcvbnm,./"};
    static const char *shifted[] = {"!@#$%^&*()_+", "QWERTYUIOP{}", "ASDFGHJKL:\"", "ZXCVBNM<>?"};
    static const WORD first[] = {0x02, 0x10, 0x1e, 0x2c};
    if (c == ' ') {
        *scan = 0x39;
        *shift = 0;
        return 1;
    }
    for (int row = 0; row < 4; row++) {
        const char *at = strchr(rows[row], c);
        if (at && c) {
            *scan = first[row] + (WORD)(at - rows[row]);
            *shift = 0;
            return 1;
        }
        at = strchr(shifted[row], c);
        if (at && c) {
            *scan = first[row] + (WORD)(at - shifted[row]);
            *shift = 1;
            return 1;
        }
    }
    return fail("no key types %c", c);
}

/* Frames console() takes for `commands` (n of them). */
static int64_t typing_frames(const char *const *commands, int n) {
    int64_t keys = 1;
    for (int i = 0; i < n; i++) {
        keys += (int64_t)strlen(commands[i]) + 1;
    }
    return KEY_FRAMES * keys;
}

/* Runs `commands` in the game's console: the grave key opens it (the
 * shipped bindings, steering-the-original.md 2.2), each line typed a key
 * at a time and entered (2.3). */
static int console(struct run *r, const char *const *commands, int n) {
    if (!press(r, SC_GRAVE, 0)) {
        return 0;
    }
    for (int i = 0; i < n; i++) {
        for (const char *c = commands[i]; *c; c++) {
            WORD scan;
            int shift;
            if (!scan_of(*c, &scan, &shift) || !press(r, scan, shift)) {
                return 0;
            }
        }
        if (!press(r, SC_RETURN, 0)) {
            return 0;
        }
    }
    return 1;
}

static int console1(struct run *r, const char *command) {
    return console(r, &command, 1);
}

/* --- Looking at frames --------------------------------------------------- */

static int corners_lit(const struct run *r) {
    uint32_t w = r->w, h = r->h;
    if (w < 32 || h < 32) {
        return 0;
    }
    uint32_t xs[4] = {0, w - 32, 0, w - 32}, ys[4] = {0, 0, h - 32, h - 32};
    for (int k = 0; k < 4; k++) {
        uint32_t sum = 0;
        for (uint32_t y = ys[k]; y < ys[k] + 32; y++) {
            const uint8_t *p = r->pixels + ((size_t)y * w + xs[k]) * 4;
            for (int x = 0; x < 32; x++) {
                sum += p[x * 4] + p[x * 4 + 1] + p[x * 4 + 2];
            }
        }
        if (sum <= 5u * 32 * 32 * 3) {
            return 0;
        }
    }
    return 1;
}

static int dark_rows(const struct run *r, uint32_t from, uint32_t to) {
    for (uint32_t y = from; y < to; y++) {
        const uint8_t *p = r->pixels + (size_t)y * r->w * 4;
        for (uint32_t x = 0; x < r->w; x++) {
            if (p[x * 4] > 2 || p[x * 4 + 1] > 2 || p[x * 4 + 2] > 2) {
                return 0;
            }
        }
    }
    return 1;
}

static int letterboxed(const struct run *r) {
    return dark_rows(r, 0, BAR_ROWS) && dark_rows(r, r->h - BAR_ROWS, r->h);
}

static int black(const struct run *r) {
    return dark_rows(r, 0, r->h);
}

static uint8_t *look_rgb(const struct run *r) {
    size_t n = (size_t)r->w * r->h;
    uint8_t *rgb = malloc(n * 3);
    for (size_t i = 0; i < n; i++) {
        rgb[i * 3] = r->pixels[i * 4 + 2];
        rgb[i * 3 + 1] = r->pixels[i * 4 + 1];
        rgb[i * 3 + 2] = r->pixels[i * 4];
    }
    return rgb;
}

/* Keeps `rgb` as `name` (a path below the run's outputs) and its hash. */
static int keep(struct run *r, const char *name, uint32_t w, uint32_t h, const uint8_t *rgb) {
    char path[MAX_PATH], hex[65];
    join(path, sizeof path, r->out, name);
    if (!write_png(path, w, h, rgb)) {
        return 0;
    }
    sha256_hex(rgb, (size_t)w * h * 3, hex);
    fprintf(r->hashes, "%s %ux%u %s\n", hex, w, h, name);
    fflush(r->hashes);
    say("frame %6lld  kept %s (%ux%u, %.16s)", (long long)r->frame, name, w, h, hex);
    return 1;
}

static int keep_look(struct run *r, const char *name) {
    uint8_t *rgb = look_rgb(r);
    int ok = keep(r, name, r->w, r->h, rgb);
    free(rgb);
    return ok;
}

/* Advances `every` frames at a time, looking at each held frame, until
 * `done` holds or `timeout` frames have passed; 1 when it held. */
static int until(struct run *r, int64_t every, int64_t timeout, const char *what,
                 int (*done)(const struct run *)) {
    int64_t end = r->frame + timeout;
    while (r->frame < end) {
        if (!advance(r, r->frame + every, 1)) {
            return 0;
        }
        if (done(r)) {
            event(r, what);
            return 1;
        }
    }
    return fail("no %s within %lld frames", what, (long long)timeout);
}

static int not_letterboxed(const struct run *r) {
    return !letterboxed(r);
}

static int intro_shows(const struct run *r) {
    return letterboxed(r) && !black(r);
}

/* Skips the logo sequence and returns the title menu's first frame (0 on
 * failure): Esc every SKIP_EVERY frames while the menu does not show, the
 * menu being the four corners lit for MENU_LOOKS frames in a row (views.rs,
 * skip_logos). */
static int64_t skip_logos(struct run *r) {
    int64_t end = r->frame + MENU_TIMEOUT;
    int lit = 0;
    while (r->frame < end) {
        int pressed = 0;
        if (lit == 0 && r->frame % SKIP_EVERY == 0) {
            if (!send_key(r, SC_ESCAPE, 0)) {
                return 0;
            }
            Sleep(INPUT_SETTLE_MS);
            pressed = 1;
        }
        if (!advance(r, r->frame + 1, 1)) {
            return 0;
        }
        if (pressed) {
            if (!send_key(r, SC_ESCAPE, 1)) {
                return 0;
            }
            Sleep(INPUT_SETTLE_MS);
        }
        lit = corners_lit(r) ? lit + 1 : 0;
        if (lit >= MENU_LOOKS) {
            int64_t first = r->frame + 1 - MENU_LOOKS;
            char what[64];
            snprintf(what, sizeof what, "the title menu (from frame %lld)", (long long)first);
            event(r, what);
            return first;
        }
    }
    fail("no title menu within %d frames", MENU_TIMEOUT);
    return 0;
}

/* --- The views ------------------------------------------------------------ */

static void demo_play(const struct view *v, char *out, size_t n) {
    snprintf(out, n, "demo_play %s,%d", v->name, TRACK_CYCLES);
}

static int64_t recorder_lead(void) {
    const char *c = RECORDER;
    return typing_frames(&c, 1) + RECORDER_START + F12_TO_FRAME;
}

/* Frames from a view's first command to its frame. */
static int64_t lead(const struct view *v) {
    if (v->place != NEW_GAME_TRACK) {
        return recorder_lead();
    }
    char play[128];
    const char *c = play;
    demo_play(v, play, sizeof play);
    return typing_frames(&c, 1) + PRECACHE + SETTLE + recorder_lead();
}

/* Frames after a view's frame until it is done: the screenshot written, at
 * the most, and the recorder ended. */
#define AFTER (SCREENSHOT_TIMEOUT + KEY_FRAMES)

/* The newest screenshot TGA of the run that is not in `before` (names
 * joined by '\n'), decoded once whole. Its name holds the user's name
 * (steering-the-original.md 5): it is not recorded. */
static uint8_t *new_screenshot(struct run *r, const char *before, uint32_t *w, uint32_t *h) {
    char pattern[MAX_PATH], path[MAX_PATH];
    WIN32_FIND_DATAA fd;
    join(pattern, sizeof pattern, r->screenshots, "ssq_*.tga");
    HANDLE f = FindFirstFileA(pattern, &fd);
    uint8_t *rgb = NULL;
    if (f == INVALID_HANDLE_VALUE) {
        return NULL;
    }
    do {
        char quoted[MAX_PATH + 2];
        snprintf(quoted, sizeof quoted, "\n%s\n", fd.cFileName);
        if (strstr(before, quoted)) {
            continue;
        }
        uint8_t *data;
        size_t n;
        join(path, sizeof path, r->screenshots, fd.cFileName);
        if (read_file(path, &data, &n)) {
            rgb = tga_rgb(data, n, w, h);
            free(data);
        }
    } while (!rgb && FindNextFileA(f, &fd));
    FindClose(f);
    return rgb;
}

static void screenshot_names(struct run *r, char *out, size_t n) {
    char pattern[MAX_PATH];
    WIN32_FIND_DATAA fd;
    size_t at = 0;
    out[0] = '\n';
    out[1] = 0;
    at = 1;
    join(pattern, sizeof pattern, r->screenshots, "ssq_*.tga");
    HANDLE f = FindFirstFileA(pattern, &fd);
    if (f == INVALID_HANDLE_VALUE) {
        return;
    }
    do {
        int k = snprintf(out + at, n - at, "%s\n", fd.cFileName);
        if (k > 0 && (size_t)k < n - at) {
            at += (size_t)k;
        }
    } while (FindNextFileA(f, &fd));
    FindClose(f);
}

/* The view's frame `frame` without the HUD: the camera recorder's
 * screenshot (steering-the-original.md 5.1). `demo_record` starts the
 * recorder from the current view; its F12 has it draw a frame with every
 * HUD switch off and save it (here also as TGA, -ss_tga); Esc ends it. The
 * frame shown at the F12, HUD included, is kept as display\<view>.png. */
static int without_hud(struct run *r, const struct view *v, int64_t frame) {
    static char before[65536];
    char name[MAX_PATH];
    screenshot_names(r, before, sizeof before);
    if (!console1(r, RECORDER) || !advance(r, frame - F12_TO_FRAME, 1)) {
        return 0;
    }
    snprintf(name, sizeof name, "fidelity\\display\\%s.png", v->name);
    if (!keep_look(r, name) || !send_key(r, SC_F12, 0)) {
        return 0;
    }
    Sleep(INPUT_SETTLE_MS);
    if (!advance(r, r->frame + 1, 0) || !send_key(r, SC_F12, 1)) {
        return 0;
    }
    Sleep(INPUT_SETTLE_MS);
    int64_t end = frame + SCREENSHOT_TIMEOUT;
    uint32_t w, h;
    uint8_t *rgb;
    while (!(rgb = new_screenshot(r, before, &w, &h))) {
        if (r->frame >= end) {
            return fail("%s: no screenshot within %d frames of the recorder's F12", v->name,
                        SCREENSHOT_TIMEOUT);
        }
        if (!advance(r, r->frame + 1, 0)) {
            return 0;
        }
    }
    char what[MAX_PATH + 64];
    snprintf(what, sizeof what, "%s: the recorder's screenshot of frame %lld", v->name,
             (long long)frame);
    event(r, what);
    if (w != plan.width || h != plan.height) {
        free(rgb);
        return fail("%s: the screenshot is %ux%u", v->name, w, h);
    }
    snprintf(name, sizeof name, "fidelity\\%s.png", v->name);
    int ok = keep(r, name, w, h, rgb);
    free(rgb);
    return ok && press(r, SC_ESCAPE, 0);
}

/* Quits the game from its console (section 7), lets it run free and waits
 * for its end; then keeps the configuration file it rewrote as each view's
 * <view>.saved.ltx (options-screens.md 12.3). */
static int quit(struct run *r, ULONGLONG config_written) {
    /* The line typed, its Return pressed at a held frame; the game may end
     * within the next frames, so it is let run before the key's release. */
    if (!press(r, SC_GRAVE, 0)) {
        return 0;
    }
    for (const char *c = "quit"; *c; c++) {
        WORD scan;
        int shift;
        if (!scan_of(*c, &scan, &shift) || !press(r, scan, shift)) {
            return 0;
        }
    }
    if (!send_key(r, SC_RETURN, 0)) {
        return 0;
    }
    Sleep(INPUT_SETTLE_MS);
    event(r, "quit entered, the game let run");
    store(&r->clock->hold, 0);
    Sleep(INPUT_SETTLE_MS * 3);
    DWORD front = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), &front);
    if (front == r->pid) {
        INPUT in = {.type = INPUT_KEYBOARD};
        in.ki.wScan = SC_RETURN;
        in.ki.dwFlags = KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP;
        SendInput(1, &in, sizeof in);
    }
    if (WaitForSingleObject(r->process, QUIT_WAIT * 1000) != WAIT_OBJECT_0) {
        return fail("the game was still running %d s after quit", QUIT_WAIT);
    }
    DWORD code = 0;
    GetExitCodeProcess(r->process, &code);
    say("The game ended with exit code 0x%lx after %lld frames (extra ticks %lld, main-thread "
        "clock reads %lld)",
        code, (long long)load(&r->clock->frames), (long long)load(&r->clock->extra),
        (long long)load(&r->clock->reads));
    fprintf(r->events, "end exit=0x%lx frames=%lld\n", code, (long long)load(&r->clock->frames));
    /* Diagnostics, not compared: the main thread's clock reads differ by a
     * few between otherwise identical runs (measured on the PC, #762). */
    fprintf(r->events, "clock extra=%lld reads=%lld other_reads=%lld sleeps=%lld\n",
            (long long)load(&r->clock->extra), (long long)load(&r->clock->reads),
            (long long)load(&r->clock->other_reads), (long long)load(&r->clock->sleeps));
    fprintf(r->events,
            "clock secondary pass: identified at frame %lld, entries to B %lld (%lld waited), "
            "frame sleeps %lld (%lld waited), timeouts %lld; sleeps of 1 ms or more after the "
            "first frame %lld\n",
            (long long)load(&r->clock->sync_identified), (long long)load(&r->clock->b_entries),
            (long long)load(&r->clock->b_entries_waited), (long long)load(&r->clock->frame_sleeps),
            (long long)load(&r->clock->frame_sleeps_waited),
            (long long)load(&r->clock->sync_timeouts), (long long)load(&r->clock->idle_sleeps));
    say("The counter client substituted %lld time-stamp-counter reads",
        (long long)load(&r->clock->counter_reads));
    /* Diagnostics, not compared: written every 2 s of real time. */
    say("The deterministic heap: %lld MiB of the region's %u carved",
        (long long)(load(&r->clock->heap_carved) >> 20), DETHEAP_REGION_SIZE >> 20);
    fprintf(r->events, "clock counter reads substituted %lld\n",
            (long long)load(&r->clock->counter_reads));
    if (code != 0) {
        return fail("the game ended with exit code 0x%lx", code);
    }
    char path[MAX_PATH], dest[MAX_PATH];
    join(path, sizeof path, r->userdata, "steer.ltx");
    WIN32_FILE_ATTRIBUTE_DATA a;
    if (!GetFileAttributesExA(path, GetFileExInfoStandard, &a)) {
        return fail("no %s after quit", path);
    }
    ULARGE_INTEGER m = {.LowPart = a.ftLastWriteTime.dwLowDateTime,
                        .HighPart = a.ftLastWriteTime.dwHighDateTime};
    if (m.QuadPart <= config_written) {
        return fail("the game did not rewrite its configuration file");
    }
    int n = r->audio ? plan.nwalks : plan.nviews;
    for (int i = 0; i < n; i++) {
        char name[128];
        if (r->audio) {
            snprintf(name, sizeof name, "audio\\%s.saved.ltx", plan.walks[i].name);
        } else {
            snprintf(name, sizeof name, "fidelity\\%s.saved.ltx", plan.views[i].name);
        }
        join(dest, sizeof dest, r->out, name);
        if (!CopyFileA(path, dest, FALSE)) {
            return fail("cannot copy %s", path);
        }
    }
    return 1;
}

/* --- The walks ------------------------------------------------------------- */

/* The keys walk `w` holds at its frame `k` (0 its first), none past its
 * segments. */
static const struct segment *segment_at(const struct walk *w, int64_t k) {
    for (int i = 0; i < w->nsegments && k >= 0; i++) {
        if (k < w->segments[i].frames) {
            return &w->segments[i];
        }
        k -= w->segments[i].frames;
    }
    return NULL;
}

static int64_t walk_frames(const struct walk *w) {
    int64_t n = w->tail;
    for (int i = 0; i < w->nsegments; i++) {
        n += w->segments[i].frames;
    }
    return n;
}

static int holds(const struct segment *g, WORD key) {
    for (int i = 0; g && i < g->nkeys; i++) {
        if (g->keys[i] == key) {
            return 1;
        }
    }
    return 0;
}

/* Walk `w` from the New Game's frame of play `play` (its frame 1): the
 * audio recorded from `preroll` frames before its first, every frame let
 * run at least PACE_MS of real time; its keys pressed and released while
 * the frame before the one that reads them is held. Keeps audio\<walk>.wav
 * and audio\<walk>.frames.txt: each frame of play with the recording's
 * samples at which the game was let run to it and at which it was
 * presented (the same performance counter as the packets'); and the
 * frames where the player starts and ends, audio\<walk>-start.png (the
 * last frame before the keys) and audio\<walk>-end.png (the walk's last),
 * from their back buffers, HUD included, written once the recording
 * stops. */
static int walk_audio(struct run *r, int64_t play, const struct walk *w) {
    /* Frame n of play is the game's frame play + n - 1. */
    int64_t first = play + w->start - 1, from = first - w->preroll, total = walk_frames(w);
    if (from - 1 <= r->frame) {
        return fail("%s: its recording starts at frame %lld, the game is at %lld", w->name,
                    (long long)from, (long long)r->frame);
    }
    if (!advance(r, from - 1, 0) || !loopback_start(r->loopback)) {
        return 0;
    }
    char what[160];
    snprintf(what, sizeof what, "%s: recording, its keys from frame %lld of play", w->name,
             (long long)w->start);
    event(r, what);
    int64_t rows = first + total - from;
    int64_t (*times)[3] = calloc((size_t)rows, sizeof *times);
    const struct segment *held = NULL;
    /* The start's and the end's frames: their game frames, pixels, sizes. */
    int64_t viewed[2] = {first - 1, first + total - 1};
    uint8_t *views[2] = {NULL, NULL};
    uint32_t view_w[2] = {0, 0}, view_h[2] = {0, 0};
    int ok = 1;
    for (int64_t g = from; ok && g < first + total; g++) {
        const struct segment *want = segment_at(w, g - first);
        int changed = 0;
        for (int i = 0; ok && held && i < held->nkeys; i++) {
            if (!holds(want, held->keys[i])) {
                ok = send_key(r, held->keys[i], 1);
                changed = 1;
            }
        }
        for (int i = 0; ok && want && i < want->nkeys; i++) {
            if (!holds(held, want->keys[i])) {
                ok = send_key(r, want->keys[i], 0);
                changed = 1;
            }
        }
        held = want;
        if (!ok) {
            break;
        }
        if (changed) {
            Sleep(INPUT_SETTLE_MS);
        }
        int64_t released = loopback_now();
        int view = g == viewed[0] ? 0 : g == viewed[1] ? 1 : -1;
        ok = advance(r, g, view >= 0);
        int64_t presented = loopback_now();
        if (ok && view >= 0) {
            views[view] = look_rgb(r);
            view_w[view] = r->w;
            view_h[view] = r->h;
        }
        times[g - from][0] = g - play + 1;
        times[g - from][1] = released;
        times[g - from][2] = presented;
        while (ok && loopback_now() < released + (int64_t)PACE_MS * 10000) {
            Sleep(1);
        }
    }
    loopback_stop(r->loopback);
    loopback_report(r->loopback, say);
    for (int i = 0; i < 2; i++) {
        char view_name[160];
        snprintf(view_name, sizeof view_name, "audio\\%s-%s.png", w->name, i ? "end" : "start");
        if (ok && views[i]) {
            ok = keep(r, view_name, view_w[i], view_h[i], views[i]);
        }
        free(views[i]);
    }
    if (!ok) {
        free(times);
        return 0;
    }
    char path[MAX_PATH], name[160];
    snprintf(name, sizeof name, "audio\\%s.wav", w->name);
    join(path, sizeof path, r->out, name);
    if (!loopback_write(r->loopback, path)) {
        free(times);
        return fail("cannot write %s", path);
    }
    snprintf(name, sizeof name, "audio\\%s.frames.txt", w->name);
    join(path, sizeof path, r->out, name);
    FILE *f = fopen(path, "w");
    if (!f) {
        free(times);
        return fail("cannot write %s", path);
    }
    fprintf(f,
            "volya-audio-frames 1\nwalk %s\nrate %u\nchannels %u\nframe_ms %lld\npace_ms %d\n"
            "start %lld\npreroll %lld\nframes %lld\n"
            "# frame of play, the sample it was let run at, the sample it was presented at\n",
            w->name, loopback_rate(r->loopback), loopback_channels(r->loopback),
            (long long)plan.frame_ms, PACE_MS, (long long)w->start, (long long)w->preroll,
            (long long)total);
    for (int64_t i = 0; i < rows; i++) {
        fprintf(f, "%lld %lld %lld\n", (long long)times[i][0],
                (long long)loopback_sample_at(r->loopback, times[i][1]),
                (long long)loopback_sample_at(r->loopback, times[i][2]));
    }
    free(times);
    if (fclose(f) != 0) {
        return fail("cannot write %s", path);
    }
    snprintf(what, sizeof what, "%s: recorded %lld frames", w->name, (long long)rows);
    event(r, what);
    return 1;
}

/* A New Game started from the title menu's console and its intro skipped:
 * the first frame of play (0 on failure). */
static int64_t start_play(struct run *r) {
    const char *start[] = {"start server(all/single/alife/new) client(localhost)", "main_menu off"};
    if (!console(r, start, 2) || !until(r, LOAD_EVERY, LOAD_TIMEOUT, "the intro", intro_shows)) {
        return 0;
    }
    for (int k = 0; k < INTRO_SKIPS; k++) {
        if (!press(r, SC_ESCAPE, 0)) {
            return 0;
        }
        if (until(r, 1, INTRO_SKIP_RETRY, "play", not_letterboxed)) {
            return r->frame;
        } else if (ended(r)) {
            return 0;
        }
    }
    fail("the intro was not skipped");
    return 0;
}

/* The walks in one run: a New Game from the title menu, each walk at its
 * frames of play. */
static int steer_audio(struct run *r, ULONGLONG config_written) {
    if (!skip_logos(r)) {
        return 0;
    }
    int64_t play = start_play(r);
    if (!play) {
        return 0;
    }
    for (int i = 0; i < plan.nwalks; i++) {
        if (!walk_audio(r, play, &plan.walks[i])) {
            return 0;
        }
    }
    return quit(r, config_written);
}

/* The views in one run, every step at a fixed frame (counted from the title
 * menu's first frame, then from the first frame of play); `full` 0 only the
 * title menu's. */
static int steer(struct run *r, int full, ULONGLONG config_written) {
    if (r->audio) {
        return steer_audio(r, config_written);
    }
    int64_t menu = skip_logos(r);
    if (!menu) {
        return 0;
    }
    for (int i = 0; i < plan.nviews; i++) {
        const struct view *v = &plan.views[i];
        if (v->place != TITLE_MENU) {
            continue;
        }
        if (!advance(r, menu + v->frame - 1, 1)) {
            return 0;
        }
        if (r->w != plan.width || r->h != plan.height) {
            return fail("the frame is %ux%u, not %ux%u (is %ux%u a display mode here?)", r->w, r->h,
                        plan.width, plan.height, plan.width, plan.height);
        }
        char name[128];
        snprintf(name, sizeof name, "fidelity\\%s.png", v->name);
        if (!keep_look(r, name)) {
            return 0;
        }
    }
    if (!full) {
        return quit(r, config_written);
    }
    /* New Game views in the order of their frames, each one's commands
     * after the previous one is done. */
    const struct view *plays[32];
    int n = 0;
    for (int i = 0; i < plan.nviews; i++) {
        if (plan.views[i].place != TITLE_MENU) {
            plays[n++] = &plan.views[i];
        }
    }
    for (int i = 1; i < n; i++) {
        for (int k = i; k > 0 && plays[k]->frame < plays[k - 1]->frame; k--) {
            const struct view *t = plays[k];
            plays[k] = plays[k - 1];
            plays[k - 1] = t;
        }
    }
    int64_t free_from = 1;
    for (int i = 0; i < n; i++) {
        if (plays[i]->frame < free_from + lead(plays[i])) {
            return fail("%s: at frame %lld of play its commands would start before frame %lld",
                        plays[i]->name, (long long)plays[i]->frame, (long long)free_from);
        }
        free_from = plays[i]->frame + AFTER;
    }
    int64_t play = start_play(r);
    if (!play) {
        return 0;
    }
    /* Frame n of play is the game's frame play + n − 1. */
    for (int i = 0; i < n; i++) {
        const struct view *v = plays[i];
        int64_t at = play + v->frame - 1, first = at - lead(v);
        if (first < r->frame) {
            return fail("%s: its commands start at frame %lld, the game is at %lld", v->name,
                        (long long)first, (long long)r->frame);
        }
        if (first > r->frame && !advance(r, first, 0)) {
            return 0;
        }
        if (v->place == NEW_GAME_TRACK) {
            char command[128];
            demo_play(v, command, sizeof command);
            if (!console1(r, command) || !advance(r, r->frame + PRECACHE, 1)) {
                return 0;
            }
            if (black(r)) {
                return fail("%s: the frame is black %d frames after its track was played", v->name,
                            PRECACHE);
            }
            char what[128];
            snprintf(what, sizeof what, "%s: its track shows", v->name);
            event(r, what);
            if (!advance(r, r->frame + SETTLE, 0)) {
                return 0;
            }
        }
        if (!without_hud(r, v, at)) {
            return 0;
        }
    }
    return quit(r, config_written);
}

/* --- Setting up a run ------------------------------------------------------ */

static int game_running(void) {
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32 pe = {.dwSize = sizeof pe};
    int found = 0;
    if (snap == INVALID_HANDLE_VALUE) {
        return 0;
    }
    for (BOOL ok = Process32First(snap, &pe); ok && !found; ok = Process32Next(snap, &pe)) {
        found = !_stricmp(pe.szExeFile, "XR_3DA.exe");
    }
    CloseHandle(snap);
    return found;
}

/* The run's user-data folder made afresh: the views' configuration file
 * (the plan's, nothing of the install's or the user's) and the camera
 * tracks in its saves folder; when it was written, for quit(). */
static int write_userdata(struct run *r, ULONGLONG *written) {
    char path[MAX_PATH];
    remove_tree(r->userdata);
    join(path, sizeof path, r->userdata, "savedgames");
    if (!make_dirs(path)) {
        return fail("cannot make %s", path);
    }
    struct buf steer = r->audio ? plan.audio_ltx : plan.ltx;
    char hex[65];
    sha256_hex(steer.p, steer.n, hex);
    say("The %s configuration file: %zu bytes, SHA-256 %s", r->audio ? "walks'" : "views'",
        steer.n, hex);
    fprintf(r->hashes, "%s %zu steer.ltx\n", hex, steer.n);
    join(path, sizeof path, r->userdata, "steer.ltx");
    if (!write_file(path, steer.p, steer.n)) {
        return 0;
    }
    for (int i = 0; r->audio && i < plan.nwalks; i++) {
        char name[128];
        snprintf(name, sizeof name, "audio\\%s.user.ltx", plan.walks[i].name);
        join(path, sizeof path, r->out, name);
        if (!write_file(path, steer.p, steer.n)) {
            return 0;
        }
    }
    for (int i = 0; !r->audio && i < plan.nviews; i++) {
        char name[128];
        snprintf(name, sizeof name, "fidelity\\%s.user.ltx", plan.views[i].name);
        join(path, sizeof path, r->out, name);
        if (!write_file(path, steer.p, steer.n)) {
            return 0;
        }
        if (plan.views[i].track) {
            snprintf(name, sizeof name, "savedgames\\%s.xrdemo", plan.views[i].name);
            join(path, sizeof path, r->userdata, name);
            if (!write_file(path, plan.views[i].track, plan.views[i].track_size)) {
                return 0;
            }
        }
    }
    FILETIME now;
    GetSystemTimeAsFileTime(&now);
    *written = (ULONGLONG)now.dwHighDateTime << 32 | now.dwLowDateTime;
    return 1;
}

/* The game started by drrun (`drrun_pid`) in `job`: XR_3DA.exe's process,
 * opened, and its id in `pid`; else null. */
static HANDLE game_in_job(HANDLE job, DWORD drrun_pid, DWORD *pid) {
    struct {
        JOBOBJECT_BASIC_PROCESS_ID_LIST list;
        ULONG_PTR more[63];
    } ids;
    if (!QueryInformationJobObject(job, JobObjectBasicProcessIdList, &ids, sizeof ids, NULL)) {
        return NULL;
    }
    for (DWORD i = 0; i < ids.list.NumberOfProcessIdsInList; i++) {
        DWORD id = (DWORD)ids.list.ProcessIdList[i];
        if (id == drrun_pid) {
            continue;
        }
        HANDLE p = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                               FALSE, id);
        char image[MAX_PATH];
        DWORD n = sizeof image;
        if (p && QueryFullProcessImageNameA(p, 0, image, &n)) {
            const char *name = strrchr(image, '\\');
            if (!_stricmp(name ? name + 1 : image, "XR_3DA.exe")) {
                *pid = id;
                return p;
            }
        }
        if (p) {
            CloseHandle(p);
        }
    }
    return NULL;
}

/* drrun's output (DynamoRIO's and the client's errors), with the folders
 * of this machine written as redact.h says, into the run's
 * drrun-output.txt; its first line in `first` ("none" if empty). */
static void drrun_output(struct run *r, const char *raw, char *first, size_t n) {
    uint8_t *data = NULL;
    size_t size = 0;
    snprintf(first, n, "none");
    if (!read_file(raw, &data, &size)) {
        return;
    }
    char path[MAX_PATH];
    join(path, sizeof path, r->dir, "drrun-output.txt");
    FILE *out = fopen(path, "w");
    char line[1024];
    size_t len = 0;
    for (size_t i = 0; i <= size; i++) {
        char c = i < size ? (char)data[i] : '\n';
        if (c == '\r') {
            continue;
        }
        if (c != '\n') {
            if (len < sizeof line - 1) {
                line[len++] = c;
            }
            continue;
        }
        line[len] = 0;
        redact(line, sizeof line);
        if (len && !strcmp(first, "none")) {
            snprintf(first, n, "%s", line);
        }
        if (out && (len || i < size)) {
            fprintf(out, "%s\n", line);
        }
        len = 0;
    }
    if (out) {
        fclose(out);
    }
    free(data);
}

static long run_printing(const char *program, char *command, const char *dir,
                         const char *output, int seconds);

/* DynamoRIO detached from the game once its first frame is held
 * (drconfig -detach, DynamoRIO's documented detach), so the rest of the
 * run is native: the start-up's counter reads, the seed's among them,
 * were the client's; later ones read the processor's counter (a 32-bit
 * process under DynamoRIO through a level load ended with an error, on
 * the PC). An event once the client saw the detach end. */
static int detach(struct run *r, const char *exe_dir, const char *work) {
    char drconfig[MAX_PATH], output[MAX_PATH], command[512];
    join(drconfig, sizeof drconfig, exe_dir, "dynamorio\\bin32\\drconfig.exe");
    join(output, sizeof output, work, "drconfig-output.txt");
    snprintf(command, sizeof command, "\"%s\" -detach %lu", drconfig, (unsigned long)r->pid);
    say("Detaching DynamoRIO from the game (%lld counter reads substituted so far):",
        (long long)load(&r->clock->counter_reads));
    long code = run_printing(drconfig, command, exe_dir, output, 120);
    /* drconfig waits for the detach to end; the client's end mark comes
     * from DynamoRIO's exit event, which a detach runs too (believed). */
    ULONGLONG until_ms = GetTickCount64() + 10000;
    while (load(&r->clock->counter_detached) != 2 && !ended(r) && GetTickCount64() < until_ms) {
        Sleep(5);
    }
    say("The client's detach state %lld (2: ended)", (long long)load(&r->clock->counter_detached));
    if (load(&r->clock->counter_detached) == 0 || (load(&r->clock->counter_detached) == 1 && code)) {
        return fail("DynamoRIO did not detach from the game (drconfig's exit code %ld, the "
                    "client's detach state %lld%s)",
                    code, (long long)load(&r->clock->counter_detached),
                    ended(r) ? ", the game ended" : "");
    }
    event(r, "DynamoRIO detached from the game");
    return 1;
}

/* One run of the game: `mode` "check" or "full", its outputs in
 * work\runs\<mode>-<n>\. */
static int run_once(const char *exe_dir, const char *game, const char *work, const char *mode,
                    int index, struct loopback *loopback) {
    struct run r = {0};
    r.audio = loopback != NULL;
    r.loopback = loopback;
    char copy[MAX_PATH], bin[MAX_PATH], path[MAX_PATH], from[MAX_PATH];
    snprintf(r.dir, sizeof r.dir, "%s\\runs\\%s-%d", work, mode, index);
    remove_tree(r.dir);
    snprintf(r.out, sizeof r.out, "%s", r.dir);
    join(path, sizeof path, r.dir, r.audio ? "audio" : "fidelity\\display");
    if (!make_dirs(path)) {
        return fail("cannot make %s", path);
    }
    join(path, sizeof path, r.dir, "log.txt");
    log_file = fopen(path, "w");
    join(path, sizeof path, r.dir, "events.txt");
    r.events = fopen(path, "w");
    join(path, sizeof path, r.dir, "hashes.txt");
    r.hashes = fopen(path, "w");
    say("Run %s-%d: %s", mode, index, r.dir);

    join(copy, sizeof copy, work, "game");
    join(r.userdata, sizeof r.userdata, work, "userdata");
    join(r.screenshots, sizeof r.screenshots, r.userdata, "screenshots");
    join(bin, sizeof bin, copy, "bin");
    ULONGLONG written;
    if (!copy_install(game, copy) || !write_fsgame(game, copy, r.userdata) ||
        !write_userdata(&r, &written)) {
        return 0;
    }
    join(from, sizeof from, exe_dir, "winmm.dll");
    join(path, sizeof path, bin, "winmm.dll");
    if (!CopyFileA(from, path, FALSE)) {
        return fail("cannot copy %s to %s", from, path);
    }

    SetLastError(0);
    r.map = CreateFileMappingA(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0,
                               (DWORD)sizeof(struct frameclock), FRAMECLOCK_NAME);
    if (!r.map || GetLastError() == ERROR_ALREADY_EXISTS) {
        return fail("cannot make the frame clock's shared memory (a game from an earlier run still "
                    "running?)");
    }
    r.clock = MapViewOfFile(r.map, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(struct frameclock));
    r.pixels = malloc((size_t)FRAMECLOCK_MAX_PIXELS * 4);
    if (!r.clock || !r.pixels) {
        return fail("cannot map the frame clock's shared memory");
    }
    r.clock->step = (uint64_t)plan.frame_ms * FRAMECLOCK_TICKS_PER_MS;
    r.clock->spin = (uint64_t)plan.spin;
    /* Seconds since 1970 to ticks since 1601. */
    r.clock->epoch = ((uint64_t)plan.epoch + 11644473600ull) * 10000000ull;
    r.clock->sleep_advances = (uint64_t)plan.sleep_advances;
    r.clock->start_quantum = (uint64_t)plan.start_quantum_ms;
    r.clock->prestart_sleeps = (uint64_t)plan.prestart_sleeps;
    r.clock->first_frame = (uint64_t)plan.first_frame_ms * FRAMECLOCK_TICKS_PER_MS;
    join(r.clock->log_path, sizeof r.clock->log_path, r.dir, "frameclock.log");
    r.clock->hold = 1;
    r.clock->heap_off = (uint64_t)no_heap;
    r.clock->version = FRAMECLOCK_VERSION;
    r.clock->magic = FRAMECLOCK_MAGIC;

    /* The game runs under DynamoRIO with the counter client (counter.c),
     * started by its drrun, which this runner holds in dynamorio\ (its
     * configuration files in the work folder). The client takes the
     * game's folder, the copy, as its modules' paths name it (long names). */
    char drrun[MAX_PATH], client[MAX_PATH], drlog[MAX_PATH], long_copy[MAX_PATH];
    join(drrun, sizeof drrun, exe_dir, "dynamorio\\bin32\\drrun.exe");
    join(client, sizeof client, exe_dir, kernel32_client ? "counter-kernel32.dll" : "counter.dll");
    if (GetFileAttributesA(drrun) == INVALID_FILE_ATTRIBUTES ||
        GetFileAttributesA(client) == INVALID_FILE_ATTRIBUTES) {
        return fail("no %s or %s: unpack the whole runner", drrun, client);
    }
    DWORD n = GetLongPathNameA(copy, long_copy, sizeof long_copy);
    if (n == 0 || n >= sizeof long_copy) {
        return fail("cannot read the long name of %s (%lu)", copy, GetLastError());
    }
    static int long_work_redacted;
    char long_work[MAX_PATH];
    if (!long_work_redacted && GetLongPathNameA(work, long_work, sizeof long_work) - 1u <
                                   sizeof long_work - 1u) {
        redact_add(long_work, "<work>", 0);
        long_work_redacted = 1;
    }
    join(path, sizeof path, work, "dynamorio-config");
    if (!make_dirs(path) || !SetEnvironmentVariableA("DYNAMORIO_CONFIGDIR", path)) {
        return fail("cannot make %s", path);
    }
    join(drlog, sizeof drlog, work, "drrun-output.txt");
    SECURITY_ATTRIBUTES inherit = {.nLength = sizeof inherit, .bInheritHandle = TRUE};
    HANDLE output = CreateFileA(drlog, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                &inherit, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (output == INVALID_HANDLE_VALUE) {
        return fail("cannot make %s", drlog);
    }

    /* The launch task (gog-install-additions.md section 5) and the options
     * of an unattended start (steering-the-original.md 1.2, 3.1). The game
     * searches its whole command line for its options (1.1): the program's
     * path is not in it, as when it is started from its folder (1.3);
     * drrun passes the game the words after its `--` as they are, and
     * starts it in its own working folder. DynamoRIO shows no message
     * boxes and writes its errors to drrun's output; drrun warns not of
     * the parts of DynamoRIO the runner leaves out (64-bit, debug). */
    /* The walks with sound (their configuration file's volumes), the
     * views without. */
    const char *game_command =
        r.audio ? "XR_3DA.exe --skipmemcheck -silent_error_mode -ss_tga -ltx steer.ltx"
                : "XR_3DA.exe --skipmemcheck -silent_error_mode -nosound -ss_tga -ltx steer.ltx";
    char command[2048];
    snprintf(command, sizeof command,
             "\"%s\" -quiet -msgbox_mask 0 -stderr_mask 12 -c \"%s\" \"%s\" -- %s", drrun, client,
             long_copy, game_command);
    STARTUPINFOA si = {.cb = sizeof si,
                       .dwFlags = STARTF_USESTDHANDLES,
                       .hStdInput = GetStdHandle(STD_INPUT_HANDLE),
                       .hStdOutput = output,
                       .hStdError = output};
    PROCESS_INFORMATION pi;
    r.job = CreateJobObjectA(NULL, NULL);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = {0};
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    SetInformationJobObject(r.job, JobObjectExtendedLimitInformation, &limits, sizeof limits);
    say("Starting %s under DynamoRIO with the counter client (working folder %s)", game_command,
        bin);
    if (!CreateProcessA(drrun, command, NULL, NULL, TRUE, CREATE_SUSPENDED, NULL, bin, &si, &pi)) {
        CloseHandle(output);
        return fail("cannot start drrun (%lu)", GetLastError());
    }
    CloseHandle(output);
    AssignProcessToJobObject(r.job, pi.hProcess);
    r.drrun = pi.hProcess;
    ResumeThread(pi.hThread);
    CloseHandle(pi.hThread);

    int ok = 1;
    ULONGLONG until_ms = GetTickCount64() + ATTACH_TIMEOUT * 1000ull;
    while (!(r.process = game_in_job(r.job, pi.dwProcessId, &r.pid)) &&
           WaitForSingleObject(r.drrun, 0) != WAIT_OBJECT_0 && GetTickCount64() < until_ms) {
        Sleep(5);
    }
    if (!r.process) {
        char first[256];
        drrun_output(&r, drlog, first, sizeof first);
        ok = fail("drrun did not start the game within %d s%s; drrun's output: %s",
                  ATTACH_TIMEOUT,
                  WaitForSingleObject(r.drrun, 0) == WAIT_OBJECT_0 ? " (drrun ended)" : "",
                  first);
        /* The rest stops a run whose game is the drrun process. */
        r.process = r.drrun;
        r.drrun = NULL;
    } else {
        AllowSetForegroundWindow(r.pid);
    }
    while (ok && !load(&r.clock->attached) && !ended(&r) && GetTickCount64() < until_ms) {
        Sleep(5);
    }
    if (ok && !load(&r.clock->attached)) {
        ok = fail("the frame clock did not start in the game within %d s%s: its winmm.dll was "
                  "not loaded from %s",
                  ATTACH_TIMEOUT, ended(&r) ? " (the game ended)" : "", bin);
    }
    /* The deterministic heap's region (capture-recipe.md, rule 3): never
     * at another address, so without it the run fails. */
    if (ok && !no_heap && load(&r.clock->heap) != 1) {
        ok = fail("the frame clock could not reserve the deterministic heap's region (0x%x bytes "
                  "at 0x%08x) in the game; see frameclock.log",
                  DETHEAP_REGION_SIZE, DETHEAP_REGION_BASE);
    }
    /* The client's start, and whether it came before the proxy's, so
     * before the game's libraries' initialisation (an event: the same in
     * every run). */
    while (ok && load(&r.clock->counter_attached) == 0 && !ended(&r) &&
           GetTickCount64() < until_ms) {
        Sleep(5);
    }
    if (ok && load(&r.clock->counter_attached) != 1) {
        char first[256];
        drrun_output(&r, drlog, first, sizeof first);
        ok = fail("the counter client did not start in the game within %d s (its state %lld%s); "
                  "drrun's output: %s (volyacapture selftest checks drrun and the client)",
                  ATTACH_TIMEOUT, (long long)load(&r.clock->counter_attached),
                  ended(&r) ? ", the game ended" : "", first);
    }
    if (ok && (no_heap || kernel32_client)) {
        char what[128];
        snprintf(what, sizeof what, "bisect: the deterministic heap %s, the client %s",
                 no_heap ? "off" : "on", kernel32_client ? "counter-kernel32.dll" : "counter.dll");
        event(&r, what);
    }
    if (ok) {
        event(&r, load(&r.clock->counter_at_proxy)
                      ? "the counter client started before the game's libraries"
                      : "the counter client started after the game's libraries");
    }
    if (ok && (ok = advance(&r, 1, 0))) {
        event(&r, load(&r.clock->hooked) ? "the first frame, held"
                                         : "the first frame, held (Present not hooked?)");
        /* An event: the clock's startup is a function of the game's own
         * calls, so every run reaches frame 1 at the same clock. */
        char what[96];
        snprintf(what, sizeof what, "the startup took %lld ticks of the clock",
                 (long long)load(&r.clock->start_ticks));
        event(&r, what);
        if ((uint64_t)load(&r.clock->start_ticks) >= r.clock->first_frame) {
            say("WARNING: more than the plan's first_frame_ms: runs may differ from here on");
        }
        ok = detach(&r, exe_dir, work);
    }
    ok = ok && steer(&r, !strcmp(mode, "full"), written);
    if (r.audio) {
        loopback_stop(r.loopback);
    }
    double seconds = (GetTickCount64() - started_ms) / 1000.0;
    if (!ok) {
        store(&r.clock->hold, 0);
        TerminateProcess(r.process, 1);
        WaitForSingleObject(r.process, 10000);
        fprintf(r.events, "failed %s\n", failure);
    }
    fprintf(r.events, "seconds %.1f\n", seconds);
    say("Run %s-%d %s after %.1f s", mode, index, ok ? "done" : "failed", seconds);
    /* drrun ends with the game. */
    if (r.drrun && WaitForSingleObject(r.drrun, 10000) != WAIT_OBJECT_0) {
        say("WARNING: drrun was still running 10 s after the game");
    }
    {
        char first[256];
        drrun_output(&r, drlog, first, sizeof first);
    }
    /* The game's logs, beside the run's own. */
    join(from, sizeof from, r.userdata, "logs");
    join(path, sizeof path, r.dir, "game-logs");
    copy_tree(from, path);
    fclose(r.events);
    fclose(r.hashes);
    CloseHandle(r.process);
    if (r.drrun) {
        CloseHandle(r.drrun);
    }
    CloseHandle(r.job);
    UnmapViewOfFile(r.clock);
    CloseHandle(r.map);
    free(r.pixels);
    join(path, sizeof path, bin, "winmm.dll");
    DeleteFileA(path);
    fclose(log_file);
    log_file = NULL;
    return ok;
}

/* --- The self-test ------------------------------------------------------- */

/* Runs `command` (`program` its executable) in the folder `dir`, its
 * output into `output`, for at most `seconds`; then prints that output,
 * each line as redact.h says. Its exit code, or -1 if it did not start or
 * end. */
static long run_printing(const char *program, char *command, const char *dir,
                         const char *output, int seconds) {
    SECURITY_ATTRIBUTES inherit = {.nLength = sizeof inherit, .bInheritHandle = TRUE};
    HANDLE out = CreateFileA(output, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, &inherit,
                             CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (out == INVALID_HANDLE_VALUE) {
        say("cannot make %s", output);
        return -1;
    }
    STARTUPINFOA si = {.cb = sizeof si,
                       .dwFlags = STARTF_USESTDHANDLES,
                       .hStdInput = GetStdHandle(STD_INPUT_HANDLE),
                       .hStdOutput = out,
                       .hStdError = out};
    PROCESS_INFORMATION pi;
    HANDLE job = CreateJobObjectA(NULL, NULL);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = {0};
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits, sizeof limits);
    long code = -1;
    if (!CreateProcessA(program, command, NULL, NULL, TRUE, CREATE_SUSPENDED, NULL, dir, &si,
                        &pi)) {
        say("cannot start it (%lu)", GetLastError());
    } else {
        AssignProcessToJobObject(job, pi.hProcess);
        ResumeThread(pi.hThread);
        CloseHandle(pi.hThread);
        if (WaitForSingleObject(pi.hProcess, (DWORD)seconds * 1000) == WAIT_OBJECT_0) {
            DWORD c = 0;
            GetExitCodeProcess(pi.hProcess, &c);
            code = (long)c;
        } else {
            say("still running after %d s: ended", seconds);
        }
        CloseHandle(pi.hProcess);
    }
    CloseHandle(job);
    CloseHandle(out);
    uint8_t *data = NULL;
    size_t size = 0;
    int lines = 0;
    if (read_file(output, &data, &size)) {
        char line[1024];
        size_t len = 0;
        for (size_t i = 0; i <= size; i++) {
            char ch = i < size ? (char)data[i] : '\n';
            if (ch == '\r') {
                continue;
            }
            if (ch != '\n') {
                if (len < sizeof line - 1) {
                    line[len++] = ch;
                }
                continue;
            }
            line[len] = 0;
            if (len) {
                say("  | %s", line);
                lines++;
            }
            len = 0;
        }
        free(data);
    }
    say("exit code %ld (0x%lx), %d lines of output", code, (unsigned long)code, lines);
    return code;
}

/* `volyacapture selftest`: drrun and the counter client on this machine,
 * with a program of ours (counterprobe.exe) in place of the game, the
 * frame clock's shared memory made as for a run: the probe natively (the
 * processor's counter), then under drrun with the client as the game is
 * started, without -quiet and with drrun's and DynamoRIO's diagnostics,
 * the client verbose. */
static int selftest(const char *exe_dir, const char *work) {
    char probe[MAX_PATH], drrun[MAX_PATH], client[MAX_PATH], long_dir[MAX_PATH];
    char path[MAX_PATH], output[MAX_PATH], command[2048];
    join(probe, sizeof probe, exe_dir, "counterprobe.exe");
    join(drrun, sizeof drrun, exe_dir, "dynamorio\\bin32\\drrun.exe");
    join(client, sizeof client, exe_dir, "counter.dll");
    if (!make_dirs(work)) {
        return fail("cannot make %s", work);
    }
    DWORD n = GetLongPathNameA(exe_dir, long_dir, sizeof long_dir);
    if (n == 0 || n >= sizeof long_dir) {
        return fail("cannot read the long name of %s (%lu)", exe_dir, GetLastError());
    }
    redact_add(long_dir, "<runner>", 0);
    join(path, sizeof path, work, "dynamorio-config");
    if (!make_dirs(path) || !SetEnvironmentVariableA("DYNAMORIO_CONFIGDIR", path)) {
        return fail("cannot make %s", path);
    }
    say("DynamoRIO's configuration folder: %s", path);
    SYSTEM_INFO native;
    GetNativeSystemInfo(&native);
    BOOL wow64 = FALSE;
    IsWow64Process(GetCurrentProcess(), &wow64);
    say("Windows %s, this runner %s",
        native.wProcessorArchitecture == PROCESSOR_ARCHITECTURE_AMD64 ? "x64" : "not x64",
        wow64 ? "32-bit under WOW64" : "native");

    SetLastError(0);
    HANDLE map = CreateFileMappingA(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0,
                                    (DWORD)sizeof(struct frameclock), FRAMECLOCK_NAME);
    if (!map || GetLastError() == ERROR_ALREADY_EXISTS) {
        return fail("cannot make the frame clock's shared memory (a game still running?)");
    }
    struct frameclock *c = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(struct frameclock));
    if (!c) {
        return fail("cannot map the frame clock's shared memory");
    }
    c->step = (uint64_t)plan.frame_ms * FRAMECLOCK_TICKS_PER_MS;
    c->version = FRAMECLOCK_VERSION;
    c->magic = FRAMECLOCK_MAGIC;

    say("1. counterprobe.exe natively:");
    join(output, sizeof output, work, "selftest-native.txt");
    snprintf(command, sizeof command, "counterprobe.exe");
    long native_code = run_printing(probe, command, exe_dir, output, 30);

    say("2. drrun's own check of DynamoRIO (drrun -verbose -version):");
    join(output, sizeof output, work, "selftest-version.txt");
    snprintf(command, sizeof command, "\"%s\" -verbose -version", drrun);
    run_printing(drrun, command, exe_dir, output, 30);

    say("3. counterprobe.exe under drrun with the counter client:");
    store(&c->counter_attached, 0);
    store(&c->counter_reads, 0);
    join(output, sizeof output, work, "selftest-drrun.txt");
    snprintf(command, sizeof command,
             "\"%s\" -verbose -msgbox_mask 0 -stderr_mask 15 -c \"%s\" \"%s\" verbose -- "
             "counterprobe.exe",
             drrun, client, long_dir);
    say("  %s", command);
    long code = run_printing(drrun, command, exe_dir, output, 120);
    say("the counter client's state %lld, reads substituted %lld",
        (long long)load(&c->counter_attached), (long long)load(&c->counter_reads));
    UnmapViewOfFile(c);
    CloseHandle(map);
    int ok = native_code == 2 && code == 0;
    say("selftest %s: natively %s, under drrun %s", ok ? "passed" : "FAILED",
        native_code == 2 ? "the processor's counter" : "unexpected",
        code == 0   ? "the client's counter"
        : code == 2 ? "not the client's counter"
        : code == 3 ? "no shared memory"
                    : "the probe did not run to its end");
    return ok;
}

/* --- Comparing runs -------------------------------------------------------- */

/* Compares every frame run 1 kept with run n's, by hash and, where they
 * differ, by pixel; and their events. Writes the summary. */
static int compare(const char *work, const char *mode, int runs) {
    char a[MAX_PATH], b[MAX_PATH], path[MAX_PATH];
    snprintf(path, sizeof path, "%s\\runs\\%s-summary.txt", work, mode);
    FILE *out = fopen(path, "w");
    int same_all = 1;
    for (int k = 2; k <= runs; k++) {
        snprintf(a, sizeof a, "%s\\runs\\%s-1\\hashes.txt", work, mode);
        snprintf(b, sizeof b, "%s\\runs\\%s-%d\\hashes.txt", work, mode, k);
        FILE *fa = fopen(a, "r");
        if (!fa) {
            continue;
        }
        char line[512];
        while (fgets(line, sizeof line, fa)) {
            char hash[65], size[32], name[256];
            if (sscanf(line, "%64s %31s %255s", hash, size, name) != 3) {
                continue;
            }
            char result[512];
            int same = 0;
            /* The same line in run k's hashes, both read as text (CR LF on
             * Windows: a binary read of one never matched, #762). */
            FILE *fb = fopen(b, "r");
            char other_line[512];
            while (fb && !same && fgets(other_line, sizeof other_line, fb)) {
                char h2[65], s2[32], n2[256];
                same = sscanf(other_line, "%64s %31s %255s", h2, s2, n2) == 3 &&
                       !strcmp(h2, hash) && !strcmp(s2, size) && !strcmp(n2, name);
            }
            if (fb) {
                fclose(fb);
            }
            if (same) {
                snprintf(result, sizeof result, "run 1 = run %d  %s: byte-identical", k, name);
            } else if (strstr(name, ".png")) {
                uint32_t w1, h1, w2, h2;
                snprintf(a, sizeof a, "%s\\runs\\%s-1\\%s", work, mode, name);
                snprintf(path, sizeof path, "%s\\runs\\%s-%d\\%s", work, mode, k, name);
                uint8_t *p1 = read_png(a, &w1, &h1), *p2 = read_png(path, &w2, &h2);
                if (!p1 || !p2 || w1 != w2 || h1 != h2) {
                    snprintf(result, sizeof result, "run 1 / run %d  %s: missing or another size", k,
                             name);
                } else {
                    size_t differ = 0, over16 = 0;
                    int max = 0;
                    uint32_t x0 = w1, y0 = h1, x1 = 0, y1 = 0;
                    for (uint32_t y = 0; y < h1; y++) {
                        for (uint32_t x = 0; x < w1; x++) {
                            size_t i = ((size_t)y * w1 + x) * 3;
                            int d = 0;
                            for (int c = 0; c < 3; c++) {
                                int e = abs((int)p1[i + c] - (int)p2[i + c]);
                                d = e > d ? e : d;
                            }
                            if (d) {
                                differ++;
                                over16 += d > 16;
                                max = d > max ? d : max;
                                x0 = x < x0 ? x : x0;
                                y0 = y < y0 ? y : y0;
                                x1 = x > x1 ? x : x1;
                                y1 = y > y1 ? y : y1;
                            }
                        }
                    }
                    snprintf(result, sizeof result,
                             "run 1 / run %d  %s: %zu of %u pixels differ (%zu by more than 16), "
                             "largest %d, within (%u, %u)-(%u, %u)",
                             k, name, differ, w1 * h1, over16, max, x0, y0, x1, y1);
                }
                free(p1);
                free(p2);
            } else {
                snprintf(result, sizeof result, "run 1 / run %d  %s: differs", k, name);
            }
            same_all &= same;
            say("%s", result);
            if (out) {
                fprintf(out, "%s\n", result);
            }
        }
        fclose(fa);
        /* The events (frames of each step, the end's clock figures). */
        snprintf(a, sizeof a, "%s\\runs\\%s-1\\events.txt", work, mode);
        snprintf(b, sizeof b, "%s\\runs\\%s-%d\\events.txt", work, mode, k);
        FILE *ea = fopen(a, "r"), *eb = fopen(b, "r");
        char la[512], lb[512];
        int line_no = 0, differ = 0;
        while (ea && eb) {
            char *ga = fgets(la, sizeof la, ea), *gb = fgets(lb, sizeof lb, eb);
            if (!ga && !gb) {
                break;
            }
            line_no++;
            if (ga && gb && !strncmp(la, "clock ", 6) && strcmp(la, lb)) {
                say("run 1 / run %d  diagnostics, not compared: %.*s / %.*s", k,
                    (int)strcspn(la, "\r\n"), la, (int)strcspn(lb, "\r\n"), lb);
                if (out) {
                    fprintf(out, "run 1 / run %d  diagnostics, not compared: %s / %s", k, la, lb);
                }
            }
            if (ga && (!strncmp(la, "seconds ", 8) || !strncmp(la, "clock ", 6))) {
                continue;
            }
            if (!ga || !gb || strcmp(la, lb)) {
                differ = 1;
                say("run 1 / run %d  events line %d: %s / %s", k, line_no, ga ? la : "(none)\n",
                    gb ? lb : "(none)\n");
                if (out) {
                    fprintf(out, "run 1 / run %d  events line %d differ: %s / %s", k, line_no,
                            ga ? la : "(none)\n", gb ? lb : "(none)\n");
                }
            }
        }
        if (ea) {
            fclose(ea);
        }
        if (eb) {
            fclose(eb);
        }
        same_all &= !differ;
        if (out) {
            fprintf(out, "run 1 %s run %d: events %s\n", differ ? "/" : "=", k,
                    differ ? "differ" : "identical (every step at the same frame)");
        }
    }
    if (out) {
        fprintf(out, "%s\n", same_all ? "all identical" : "not all identical");
        fclose(out);
    }
    say("%s", same_all ? "All runs identical" : "The runs differ");
    return same_all;
}

/* --- Main --------------------------------------------------------------- */

static int default_game(char *out, size_t n) {
    HKEY key;
    DWORD size = (DWORD)n, type;
    if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SOFTWARE\\GSC Game World\\STALKER-SHOC", 0,
                      KEY_READ | KEY_WOW64_32KEY, &key) != ERROR_SUCCESS) {
        return 0;
    }
    LONG got = RegQueryValueExA(key, "InstallPath", NULL, &type, (BYTE *)out, &size);
    RegCloseKey(key);
    if (got != ERROR_SUCCESS || type != REG_SZ) {
        return 0;
    }
    size_t len = strlen(out);
    while (len && (out[len - 1] == '\\' || out[len - 1] == '/')) {
        out[--len] = 0;
    }
    return len > 0;
}

int main(int argc, char **argv) {
    started_ms = GetTickCount64();
    const char *mode = argc > 1 ? argv[1] : "";
    char game[MAX_PATH] = "", work[MAX_PATH] = "", exe_dir[MAX_PATH];
    {
        /* The default work folder: the user's temp folder, which this
           session may clean up (a top-level C:\ folder it may not). */
        char temp[MAX_PATH];
        DWORD n = GetTempPathA(sizeof temp, temp);
        if (n == 0 || n >= sizeof temp) snprintf(temp, sizeof temp, "C:\\Windows\\Temp\\");
        snprintf(work, sizeof work, "%svolya-engine-capture", temp);
    }
    int runs = -1;
    char audio_device[256] = "";
    for (int i = 2; i < argc; i++) {
        if (!strcmp(argv[i], "--game") && i + 1 < argc) {
            snprintf(game, sizeof game, "%s", argv[++i]);
        } else if (!strcmp(argv[i], "--work") && i + 1 < argc) {
            snprintf(work, sizeof work, "%s", argv[++i]);
        } else if (!strcmp(argv[i], "--runs") && i + 1 < argc) {
            runs = atoi(argv[++i]);
        } else if (!strcmp(argv[i], "--audio-device") && i + 1 < argc) {
            snprintf(audio_device, sizeof audio_device, "%s", argv[++i]);
        } else if (!strcmp(argv[i], "--no-heap")) {
            no_heap = 1;
        } else if (!strcmp(argv[i], "--kernel32-client")) {
            kernel32_client = 1;
        } else {
            mode = "";
        }
    }
    int audio = !strcmp(mode, "audio");
    if (runs < 0) {
        /* The walks' audio is not compared run with run: one by default. */
        runs = audio ? 1 : 2;
    }
    if ((strcmp(mode, "check") && strcmp(mode, "full") && strcmp(mode, "selftest") && !audio) ||
        runs < 1) {
        fprintf(stderr, "usage: volyacapture check|full|selftest [--runs N] [--game DIR] "
                        "[--work DIR] [--no-heap] [--kernel32-client]\n"
                        "       volyacapture audio [--audio-device NAME] [--runs N] [--game DIR] "
                        "[--work DIR] [--no-heap] [--kernel32-client]\n");
        return 2;
    }
    GetModuleFileNameA(NULL, exe_dir, MAX_PATH);
    *strrchr(exe_dir, '\\') = 0;
    redact_add(exe_dir, "<runner>", 0);
    redact_add(work, "<work>", 0);
    char plan_path[MAX_PATH];
    join(plan_path, sizeof plan_path, exe_dir, "plan.txt");
    if (!read_plan(plan_path)) {
        return 1;
    }
    if (!strcmp(mode, "selftest")) {
        redact_add_user();
        int ok = selftest(exe_dir, work);
        printf("%s\n", ok ? "Done." : failure[0] ? failure : "selftest failed");
        return ok ? 0 : 1;
    }
    if (audio && !plan.nwalks) {
        fail("the plan has no walk");
        return 1;
    }
    if (!game[0] && !default_game(game, sizeof game)) {
        fail("no --game DIR, and the install's registry key (HKLM\\SOFTWARE\\GSC Game "
             "World\\STALKER-SHOC, InstallPath) is not there");
        return 1;
    }
    /* Before the user's folders: the install may be below them. */
    redact_add(game, "<game folder>", 0);
    redact_add_user();
    /* No space (its user-data folder's line in fsgame.ltx was not tried
     * with one); paths below it hold up to MAX_PATH characters. */
    if (strchr(work, ' ') || strlen(work) > 100) {
        fail("the work folder %s must have no space and at most 100 characters", work);
        return 1;
    }
    if (game_running()) {
        fail("XR_3DA.exe is running: a second instance exits silently; end it first");
        return 1;
    }
    timeBeginPeriod(1);
    say("volyacapture %s: %d run(s), game %s, work folder %s", mode, runs, game, work);
    say("The deterministic heap %s; the client %s", no_heap ? "OFF (--no-heap)" : "on",
        kernel32_client ? "counter-kernel32.dll (--kernel32-client)" : "counter.dll");
    clear_outputs(work, mode);
    /* The walks' output device: the default as it is, or the one named. */
    struct loopback *lb = NULL;
    if (audio) {
        lb = loopback_open(audio_device, say);
        if (!lb) {
            fail("no audio output to record");
            return 1;
        }
    }
    int ok = 1;
    for (int i = 1; i <= runs && ok; i++) {
        ok = run_once(exe_dir, game, work, mode, i, lb);
    }
    loopback_close(lb);
    if (ok && runs > 1 && !audio) {
        compare(work, mode, runs);
    } else if (!ok) {
        /* A summary of this invocation, never an earlier one's. */
        char path[MAX_PATH];
        snprintf(path, sizeof path, "%s\\runs\\%s-summary.txt", work, mode);
        FILE *out = fopen(path, "w");
        if (out) {
            fprintf(out, "FAILED: %s\n", failure);
            fclose(out);
        }
    }
    printf("%s\n", ok ? "Done." : failure);
    return ok ? 0 : 1;
}
