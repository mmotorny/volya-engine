/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* What the driver and the frame clock write may be committed or reported:
 * no user name, home folder or absolute path of the machine in it. A line
 * is rewritten before it is written: each folder registered (the install,
 * the runner, the work folder) by its placeholder, then the user's profile
 * folder, then the user's and the computer's names where they stand as a
 * word (not inside another one). */

#ifndef VOLYA_REDACT_H
#define VOLYA_REDACT_H

#include <windows.h>

#include <ctype.h>
#include <stdio.h>
#include <string.h>

#define REDACT_MAX 8

static struct {
    char from[MAX_PATH];
    const char *to;
    int word;
} redact_list[REDACT_MAX];
static int redact_count;

/* `from` (a folder, trailing separators dropped) is written as `to`; with
 * `word`, only where it is not part of a longer word. */
static void redact_add(const char *from, const char *to, int word) {
    if (redact_count == REDACT_MAX || !from) {
        return;
    }
    size_t n = strlen(from);
    while (n && (from[n - 1] == '\\' || from[n - 1] == '/')) {
        n--;
    }
    if (n < 2 || n >= MAX_PATH) {
        return;
    }
    memcpy(redact_list[redact_count].from, from, n);
    redact_list[redact_count].from[n] = 0;
    redact_list[redact_count].to = to;
    redact_list[redact_count].word = word;
    redact_count++;
}

/* The user's profile folder, user name and computer name. */
static void redact_add_user(void) {
    static char profile[MAX_PATH], user[MAX_PATH], computer[MAX_PATH];
    DWORD n = GetEnvironmentVariableA("USERPROFILE", profile, sizeof profile);
    if (n && n < sizeof profile) {
        redact_add(profile, "<home>", 0);
    }
    n = GetEnvironmentVariableA("USERNAME", user, sizeof user);
    if (n && n < sizeof user) {
        redact_add(user, "<user>", 1);
    }
    n = GetEnvironmentVariableA("COMPUTERNAME", computer, sizeof computer);
    if (n && n < sizeof computer) {
        redact_add(computer, "<computer>", 1);
    }
}

static int redact_word_char(char c) {
    return isalnum((unsigned char)c) || (unsigned char)c >= 0x80;
}

/* `s` (of `size` bytes) with every registered text replaced, in place. */
static void redact(char *s, size_t size) {
    char out[4096];
    if (size > sizeof out) {
        size = sizeof out;
    }
    for (int k = 0; k < redact_count; k++) {
        const char *from = redact_list[k].from, *to = redact_list[k].to;
        size_t n = strlen(from), t = strlen(to), at = 0;
        int changed = 0;
        for (const char *p = s; *p && at + 1 < size;) {
            int match = !_strnicmp(p, from, n);
            if (match && redact_list[k].word) {
                match = (p == s || !redact_word_char(p[-1])) && !redact_word_char(p[n]);
            }
            if (match) {
                size_t m = t < size - 1 - at ? t : size - 1 - at;
                memcpy(out + at, to, m);
                at += m;
                p += n;
                changed = 1;
            } else {
                out[at++] = *p++;
            }
        }
        if (changed) {
            out[at] = 0;
            memcpy(s, out, at + 1);
        }
    }
}

#endif
