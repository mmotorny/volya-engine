/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Anisotropic filtering: a floor receding to the top of a 128x128 target,
 * its perspective from rhw (1 at the near edge, y 128, to 0.125 at the far
 * one, y 16), textured with a wrapped 64x64 checker of 2x2-texel cells
 * with box-filtered mips. Cases linear (linear min, mag and mip filters)
 * and max<n> (anisotropic min filter, MAXANISOTROPY n: 1, 2, 4, 8, 16). */
#include "probe.h"

#include <stdio.h>
#include <stdlib.h>

enum { TEX_SIZE = 64, LEVELS = 7 };

static void floor_case(const char *name, D3DTEXTUREFILTERTYPE min, DWORD max, const void *const *levels) {
    if (!case_begin("aniso", name, 128, 128, 0)) return;
    struct texture t = texture_create(D3DFMT_A8R8G8B8, TEX_SIZE, TEX_SIZE, LEVELS, levels);
    use_texture(&t, D3DTEXF_LINEAR);
    set_ss(0, D3DSAMP_MINFILTER, min);
    set_ss(0, D3DSAMP_MIPFILTER, D3DTEXF_LINEAR);
    set_ss(0, D3DSAMP_MAXANISOTROPY, max);
    set_ss(0, D3DSAMP_ADDRESSU, D3DTADDRESS_WRAP);
    set_ss(0, D3DSAMP_ADDRESSV, D3DTADDRESS_WRAP);
    /* Depth 1 near, 8 far; v = 4 * depth. */
    struct vertex far_l = {56, 16, 0.5f, 0.125f, 0xFFFFFFFF, 0, 0, 32};
    struct vertex far_r = {72, 16, 0.5f, 0.125f, 0xFFFFFFFF, 0, 8, 32};
    struct vertex near_l = {0, 128, 0.5f, 1, 0xFFFFFFFF, 0, 0, 4};
    struct vertex near_r = {128, 128, 0.5f, 1, 0xFFFFFFFF, 0, 8, 4};
    struct vertex v[6] = {far_l, far_r, near_l, far_r, near_r, near_l};
    draw(D3DPT_TRIANGLELIST, 2, v, 6);
    case_end(NULL);
}

void probe_aniso(void) {
    DWORD *data[LEVELS];
    const void *levels[LEVELS];
    data[0] = malloc(TEX_SIZE * TEX_SIZE * sizeof(DWORD));
    for (int y = 0; y < TEX_SIZE; y++)
        for (int x = 0; x < TEX_SIZE; x++) data[0][y * TEX_SIZE + x] = ((x >> 1) ^ (y >> 1)) & 1 ? 0xFFFFFFFF : 0xFF000000;
    for (int l = 1; l < LEVELS; l++) {
        int n = TEX_SIZE >> l, m = n * 2;
        data[l] = malloc(n * n * sizeof(DWORD));
        for (int y = 0; y < n; y++)
            for (int x = 0; x < n; x++) {
                const DWORD *s = data[l - 1];
                DWORD p[4] = {s[2 * y * m + 2 * x], s[2 * y * m + 2 * x + 1], s[(2 * y + 1) * m + 2 * x],
                              s[(2 * y + 1) * m + 2 * x + 1]};
                DWORD out = 0;
                for (int c = 0; c < 32; c += 8) {
                    DWORD sum = 0;
                    for (int i = 0; i < 4; i++) sum += p[i] >> c & 0xFF;
                    out |= (sum + 2) / 4 << c;
                }
                data[l][y * n + x] = out;
            }
    }
    for (int l = 0; l < LEVELS; l++) levels[l] = data[l];
    floor_case("linear", D3DTEXF_LINEAR, 1, levels);
    static const DWORD maxes[] = {1, 2, 4, 8, 16};
    for (int i = 0; i < 5; i++) {
        char name[16];
        snprintf(name, sizeof name, "max%lu", (unsigned long)maxes[i]);
        floor_case(name, D3DTEXF_ANISOTROPIC, maxes[i], levels);
    }
    for (int l = 0; l < LEVELS; l++) free(data[l]);
}
