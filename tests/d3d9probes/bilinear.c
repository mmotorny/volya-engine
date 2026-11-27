/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Bilinear weights: a 2x1 texture (every channel a, then b) magnified x256
 * onto a 512x2 target, clamped. Pixel x samples u = x / 512, so pixels 128
 * to 384 weight the second texel by (x - 128) / 256: every weight in
 * 1/256 steps. Cases ramp_<a>_<b>: 0/255, 0/1 and 100/101. */
#include "probe.h"

#include <stdio.h>

void probe_bilinear(void) {
    static const int ramps[][2] = {{0, 255}, {0, 1}, {100, 101}};
    for (int r = 0; r < 3; r++) {
        char name[32];
        snprintf(name, sizeof name, "ramp_%d_%d", ramps[r][0], ramps[r][1]);
        if (!case_begin("bilinear", name, 512, 2, 0)) continue;
        int a = ramps[r][0], b = ramps[r][1];
        DWORD texels[2] = {D3DCOLOR_ARGB(a, a, a, a), D3DCOLOR_ARGB(b, b, b, b)};
        struct texture t = texture_argb(2, 1, texels);
        use_texture(&t, D3DTEXF_LINEAR);
        quad(0, 0, 512, 2, 0, 0, 1, 1, 0xFFFFFFFF, 0);
        case_end(NULL);
    }
}
