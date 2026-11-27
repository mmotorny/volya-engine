/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* volyacapture.exe: the original's frames of the fidelity suite's views on
 * Windows (docs/testing.md, The original on Windows). It runs the shipped
 * game unmodified, from a copy of its install, on the frame clock of the
 * proxy winmm.dll beside it (frameclock.c), and steers it by its normal
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
 *
 * Its plan, plan.txt beside it, comes from tests/fidelity/views.rs
 * (`xtask wincapture plan`): the views, their frames, the configuration
 * file's settings and the camera tracks. Everything it writes is below
 * the work folder (default C:\volyacapture): the copy of the install
 * (game\), the game's user-data folder (userdata\, made afresh for each
 * run, named by the copy's fsgame.ltx) and each run's outputs
 * (runs\<mode>-<n>\: fidelity\<view>.png and its files, log.txt,
 * events.txt, hashes.txt), and runs\<mode>-summary.txt comparing the
 * runs. The frames are game assets: they stay on this machine. */

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

#include "frameclock.h"

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
#define ATTACH_TIMEOUT 60
#define QUIT_WAIT 60

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
    double t = (GetTickCount64() - started_ms) / 1000.0;
    printf("%8.1f s  %s\n", t, line);
    fflush(stdout);
    if (log_file) {
        fprintf(log_file, "%8.1f s  %s\n", t, line);
        fflush(log_file);
    }
}

static char failure[2048];

static int fail(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(failure, sizeof failure, fmt, ap);
    va_end(ap);
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

static struct {
    int64_t frame_ms, spin, epoch, sleep_advances, start_quantum_ms, first_frame_ms,
        prestart_sleeps;
    uint32_t width, height;
    struct buf ltx; /* the views' configuration file, CR LF lines */
    struct view views[32];
    int nviews;
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
    DWORD pid;
    struct frameclock *clock;
    int64_t frame;
    uint32_t w, h;
    uint8_t *pixels; /* the last look, B G R X */
    FILE *events;
    FILE *hashes;
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
    say("frame %6lld  %s", (long long)r->frame, what);
    fprintf(r->events, "%lld %s\n", (long long)r->frame, what);
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

/* Lets the game run until it has presented `frame` and holds it after it;
 * with `look`, the frame's back buffer is copied into r->pixels. */
static int advance(struct run *r, int64_t frame, int look) {
    if (frame <= r->frame) {
        return fail("frame %lld: the game is held after frame %lld", (long long)frame,
                    (long long)r->frame);
    }
    store(&r->clock->capture, look ? frame : 0);
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

static int send_key(struct run *r, WORD scan, int up) {
    if (!game_in_front(r)) {
        return 0;
    }
    INPUT in = {0};
    in.type = INPUT_KEYBOARD;
    in.ki.wScan = scan;
    in.ki.dwFlags = KEYEVENTF_SCANCODE | (up ? KEYEVENTF_KEYUP : 0);
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
 * joined by '\n'), decoded once whole. */
static uint8_t *new_screenshot(struct run *r, const char *before, uint32_t *w, uint32_t *h,
                               char *name, size_t name_size) {
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
            if (rgb) {
                snprintf(name, name_size, "%s", fd.cFileName);
            }
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
    char name[MAX_PATH], shot[MAX_PATH];
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
    while (!(rgb = new_screenshot(r, before, &w, &h, shot, sizeof shot))) {
        if (r->frame >= end) {
            return fail("%s: no screenshot within %d frames of the recorder's F12", v->name,
                        SCREENSHOT_TIMEOUT);
        }
        if (!advance(r, r->frame + 1, 0)) {
            return 0;
        }
    }
    char what[MAX_PATH + 64];
    snprintf(what, sizeof what, "%s: the recorder's screenshot of frame %lld, %s", v->name,
             (long long)frame, shot);
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
    for (int i = 0; i < plan.nviews; i++) {
        char name[128];
        snprintf(name, sizeof name, "fidelity\\%s.saved.ltx", plan.views[i].name);
        join(dest, sizeof dest, r->out, name);
        if (!CopyFileA(path, dest, FALSE)) {
            return fail("cannot copy %s", path);
        }
    }
    return 1;
}

/* The views in one run, every step at a fixed frame (counted from the title
 * menu's first frame, then from the first frame of play); `full` 0 only the
 * title menu's. */
static int steer(struct run *r, int full, ULONGLONG config_written) {
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
    const char *start[] = {"start server(all/single/alife/new) client(localhost)", "main_menu off"};
    if (!console(r, start, 2) || !until(r, LOAD_EVERY, LOAD_TIMEOUT, "the intro", intro_shows)) {
        return 0;
    }
    int64_t play = 0;
    for (int k = 0; k < INTRO_SKIPS && !play; k++) {
        if (!press(r, SC_ESCAPE, 0)) {
            return 0;
        }
        if (until(r, 1, INTRO_SKIP_RETRY, "play", not_letterboxed)) {
            play = r->frame;
        } else if (ended(r)) {
            return 0;
        }
    }
    if (!play) {
        return fail("the intro was not skipped");
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
    struct buf steer = plan.ltx;
    char hex[65];
    sha256_hex(steer.p, steer.n, hex);
    say("The views' configuration file: %zu bytes, SHA-256 %s", steer.n, hex);
    fprintf(r->hashes, "%s %zu steer.ltx\n", hex, steer.n);
    join(path, sizeof path, r->userdata, "steer.ltx");
    if (!write_file(path, steer.p, steer.n)) {
        return 0;
    }
    for (int i = 0; i < plan.nviews; i++) {
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

/* One run of the game: `mode` "check" or "full", its outputs in
 * work\runs\<mode>-<n>\. */
static int run_once(const char *exe_dir, const char *game, const char *work, const char *mode,
                    int index) {
    struct run r = {0};
    char copy[MAX_PATH], bin[MAX_PATH], path[MAX_PATH], from[MAX_PATH];
    snprintf(r.dir, sizeof r.dir, "%s\\runs\\%s-%d", work, mode, index);
    remove_tree(r.dir);
    snprintf(r.out, sizeof r.out, "%s", r.dir);
    join(path, sizeof path, r.dir, "fidelity\\display");
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
    r.clock->version = FRAMECLOCK_VERSION;
    r.clock->magic = FRAMECLOCK_MAGIC;

    /* The launch task (gog-install-additions.md section 5) and the options
     * of an unattended start (steering-the-original.md 1.2, 3.1). */
    char command[1024];
    snprintf(command, sizeof command,
             "\"%s\\XR_3DA.exe\" --skipmemcheck -silent_error_mode -nosound -ss_tga -ltx steer.ltx",
             bin);
    STARTUPINFOA si = {.cb = sizeof si};
    PROCESS_INFORMATION pi;
    r.job = CreateJobObjectA(NULL, NULL);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = {0};
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    SetInformationJobObject(r.job, JobObjectExtendedLimitInformation, &limits, sizeof limits);
    say("Starting %s (working folder %s)", command, bin);
    if (!CreateProcessA(NULL, command, NULL, NULL, FALSE, CREATE_SUSPENDED, NULL, bin, &si, &pi)) {
        return fail("cannot start the game (%lu)", GetLastError());
    }
    AssignProcessToJobObject(r.job, pi.hProcess);
    r.process = pi.hProcess;
    r.pid = pi.dwProcessId;
    AllowSetForegroundWindow(r.pid);
    ResumeThread(pi.hThread);
    CloseHandle(pi.hThread);

    int ok = 1;
    ULONGLONG until_ms = GetTickCount64() + ATTACH_TIMEOUT * 1000ull;
    while (!load(&r.clock->attached) && !ended(&r) && GetTickCount64() < until_ms) {
        Sleep(5);
    }
    if (!load(&r.clock->attached)) {
        ok = fail("the frame clock did not start in the game within %d s%s: its winmm.dll was "
                  "not loaded from %s",
                  ATTACH_TIMEOUT, ended(&r) ? " (the game ended)" : "", bin);
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
    }
    ok = ok && steer(&r, !strcmp(mode, "full"), written);
    double seconds = (GetTickCount64() - started_ms) / 1000.0;
    if (!ok) {
        store(&r.clock->hold, 0);
        TerminateProcess(r.process, 1);
        WaitForSingleObject(r.process, 10000);
        fprintf(r.events, "failed %s\n", failure);
    }
    fprintf(r.events, "seconds %.1f\n", seconds);
    say("Run %s-%d %s after %.1f s", mode, index, ok ? "done" : "failed", seconds);
    /* The game's logs, beside the run's own. */
    join(from, sizeof from, r.userdata, "logs");
    join(path, sizeof path, r.dir, "game-logs");
    copy_tree(from, path);
    fclose(r.events);
    fclose(r.hashes);
    CloseHandle(r.process);
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
    char game[MAX_PATH] = "", work[MAX_PATH] = "C:\\volyacapture", exe_dir[MAX_PATH];
    int runs = 2;
    for (int i = 2; i < argc; i++) {
        if (!strcmp(argv[i], "--game") && i + 1 < argc) {
            snprintf(game, sizeof game, "%s", argv[++i]);
        } else if (!strcmp(argv[i], "--work") && i + 1 < argc) {
            snprintf(work, sizeof work, "%s", argv[++i]);
        } else if (!strcmp(argv[i], "--runs") && i + 1 < argc) {
            runs = atoi(argv[++i]);
        } else {
            mode = "";
        }
    }
    if ((strcmp(mode, "check") && strcmp(mode, "full")) || runs < 1) {
        fprintf(stderr, "usage: volyacapture check|full [--runs N] [--game DIR] [--work DIR]\n");
        return 2;
    }
    GetModuleFileNameA(NULL, exe_dir, MAX_PATH);
    *strrchr(exe_dir, '\\') = 0;
    char plan_path[MAX_PATH];
    join(plan_path, sizeof plan_path, exe_dir, "plan.txt");
    if (!read_plan(plan_path)) {
        return 1;
    }
    if (!game[0] && !default_game(game, sizeof game)) {
        fail("no --game DIR, and the install's registry key (HKLM\\SOFTWARE\\GSC Game "
             "World\\STALKER-SHOC, InstallPath) is not there");
        return 1;
    }
    /* The game finds its options anywhere in its command line, the program's
     * path included (steering-the-original.md 1.1): no dash and no space in
     * the work folder, and a short one (the line holds 511 characters). */
    if (strpbrk(work, "- ") || strlen(work) > 100) {
        fail("the work folder %s must have no dash or space and at most 100 characters", work);
        return 1;
    }
    if (game_running()) {
        fail("XR_3DA.exe is running: a second instance exits silently; end it first");
        return 1;
    }
    timeBeginPeriod(1);
    printf("volyacapture %s: %d run(s), game %s, work folder %s\n", mode, runs, game, work);
    int ok = 1;
    for (int i = 1; i <= runs && ok; i++) {
        ok = run_once(exe_dir, game, work, mode, i);
    }
    if (ok && runs > 1) {
        compare(work, mode, runs);
    }
    printf("%s\n", ok ? "Done." : failure);
    return ok ? 0 : 1;
}
