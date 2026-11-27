/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Mip selection and LOD bias: a 256x256 texture with all 9 levels, each a
 * solid colour of its own, on a strip of 41 16x16 quads, quad i at LOD
 * -1 + i/8 (texture coordinates 0 to 2^LOD / 16, so 2^LOD texels per
 * pixel), linear min and mag filters. Cases point and linear (the mip
 * filter), and bias_<b> (linear, MIPMAPLODBIAS -1, -0.5, 0.5, 1). */
#include "probe.h"

#include <math.h>
#include <stdlib.h>

enum { LEVELS = 9, STEPS = 41 };

static const DWORD level_colours[LEVELS] = {
    0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFF00, 0xFFFF00FF,
    0xFF00FFFF, 0xFFFFFFFF, 0xFF808080, 0xFF000000,
};

static void strip(const char *name, D3DTEXTUREFILTERTYPE mip, float bias, const void *const *levels) {
    if (!case_begin("mip", name, STEPS * 16, 16, 0)) return;
    struct texture t = texture_create(D3DFMT_A8R8G8B8, 256, 256, LEVELS, levels);
    use_texture(&t, D3DTEXF_LINEAR);
    set_ss(0, D3DSAMP_MIPFILTER, mip);
    set_ss(0, D3DSAMP_MIPMAPLODBIAS, float_bits(bias));
    for (int i = 0; i < STEPS; i++) {
        float span = (float)(pow(2.0, -1.0 + i / 8.0) / 16.0);
        quad(i * 16.0f, 0, i * 16.0f + 16, 16, 0, 0, span, span, 0xFFFFFFFF, 0);
    }
    case_end(NULL);
}

void probe_mip(void) {
    DWORD *data[LEVELS];
    const void *levels[LEVELS];
    for (int l = 0; l < LEVELS; l++) {
        int n = (256 >> l) * (256 >> l);
        data[l] = malloc(n * sizeof(DWORD));
        for (int i = 0; i < n; i++) data[l][i] = level_colours[l];
        levels[l] = data[l];
    }
    strip("point", D3DTEXF_POINT, 0, levels);
    strip("linear", D3DTEXF_LINEAR, 0, levels);
    strip("bias_m1", D3DTEXF_LINEAR, -1, levels);
    strip("bias_m0.5", D3DTEXF_LINEAR, -0.5f, levels);
    strip("bias_p0.5", D3DTEXF_LINEAR, 0.5f, levels);
    strip("bias_p1", D3DTEXF_LINEAR, 1, levels);
    for (int l = 0; l < LEVELS; l++) free(data[l]);
}
