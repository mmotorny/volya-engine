/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Blending rounding and saturation on a 256x16 target: the destination is
 * x in every channel at pixel x (a 256x1 texture), the source row y's
 * colour (255, 17y, 255 - 17y) with alpha 17y (a 1x16 texture), both drawn
 * 1:1 with point sampling. Cases alpha (SRCALPHA, INVSRCALPHA), additive
 * (ONE, ONE), modulate (DESTCOLOR, ZERO) and srcalpha_one (SRCALPHA, ONE). */
#include "probe.h"

static void blend_case(const char *name, D3DBLEND src, D3DBLEND dest) {
    if (!case_begin("blend", name, 256, 16, 0)) return;
    DWORD dest_texels[256], src_texels[16];
    for (int x = 0; x < 256; x++) dest_texels[x] = D3DCOLOR_ARGB(x, x, x, x);
    for (int y = 0; y < 16; y++) src_texels[y] = D3DCOLOR_ARGB(17 * y, 255, 17 * y, 255 - 17 * y);
    struct texture d = texture_argb(256, 1, dest_texels);
    struct texture s = texture_argb(1, 16, src_texels);
    use_texture(&d, D3DTEXF_POINT);
    quad(-0.5f, -0.5f, 255.5f, 15.5f, 0, 0, 1, 1, 0xFFFFFFFF, 0);
    use_texture(&s, D3DTEXF_POINT);
    set_rs(D3DRS_ALPHABLENDENABLE, TRUE);
    set_rs(D3DRS_SRCBLEND, src);
    set_rs(D3DRS_DESTBLEND, dest);
    quad(-0.5f, -0.5f, 255.5f, 15.5f, 0, 0, 1, 1, 0xFFFFFFFF, 0);
    case_end(NULL);
}

void probe_blend(void) {
    blend_case("alpha", D3DBLEND_SRCALPHA, D3DBLEND_INVSRCALPHA);
    blend_case("additive", D3DBLEND_ONE, D3DBLEND_ONE);
    blend_case("modulate", D3DBLEND_DESTCOLOR, D3DBLEND_ZERO);
    blend_case("srcalpha_one", D3DBLEND_SRCALPHA, D3DBLEND_ONE);
}
