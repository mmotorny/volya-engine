/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* BC1 and BC3 (DXT1, DXT5) decoding: 16x16 textures of 4x4 blocks drawn
 * 1:1 with point sampling onto texel centres. Block k's endpoints sweep the
 * 5- and 6-bit ranges (565 expansion and interpolation rounding); texel
 * (x, y) of a BC1 block uses index (x + y) & 3, texel i of a BC3 alpha
 * block index i & 7. Cases:
 *   bc1_4colour   colour0 > colour1: two interpolated colours
 *   bc1_3colour   colour0 <= colour1 (the last block equal): one
 *                 interpolated colour and index 3 transparent black
 *   bc3_alpha8    alpha0 > alpha1: six interpolated alphas, white colour
 *   bc3_alpha6    alpha0 <= alpha1: four interpolated alphas, 0 and 255
 *   bc3_colour    BC3 with the bc1_3colour endpoints (BC3's colour block
 *                 has four colours whatever their order), alpha 255 */
#include "probe.h"

#include <string.h>

static WORD rgb565(int r, int g, int b) {
    return (WORD)(r << 11 | g << 5 | b);
}

/* Block k's two colours: c0 > c1 for four colours, c0 <= c1 otherwise. */
static void colours(int k, int four, WORD *c0, WORD *c1) {
    WORD a = rgb565(31 - k, 63 - 3 * k, 2 * k), b = rgb565(k, 3 * k + 1, 31 - 2 * k);
    if (four ? a < b : a > b) {
        WORD t = a;
        a = b;
        b = t;
    }
    if (!four && k == 15) b = a;
    *c0 = a;
    *c1 = b;
}

static void colour_block(BYTE *out, WORD c0, WORD c1) {
    DWORD indices = 0;
    for (int y = 0; y < 4; y++)
        for (int x = 0; x < 4; x++) indices |= (DWORD)((x + y) & 3) << 2 * (y * 4 + x);
    out[0] = (BYTE)c0;
    out[1] = (BYTE)(c0 >> 8);
    out[2] = (BYTE)c1;
    out[3] = (BYTE)(c1 >> 8);
    memcpy(out + 4, &indices, 4);
}

static void alpha_block(BYTE *out, int k, int eight) {
    int a = 255 - 13 * k, b = 7 * k + 3;
    if (eight ? a <= b : a > b) {
        int t = a;
        a = b;
        b = t;
    }
    out[0] = (BYTE)a;
    out[1] = (BYTE)b;
    unsigned long long bits = 0;
    for (int i = 0; i < 16; i++) bits |= (unsigned long long)(i & 7) << 3 * i;
    for (int i = 0; i < 6; i++) out[2 + i] = (BYTE)(bits >> 8 * i);
}

static void bc_case(const char *name, D3DFORMAT format, const BYTE *blocks) {
    if (!case_begin("bc", name, 16, 16, 0)) return;
    const void *levels[1] = {blocks};
    struct texture t = texture_create(format, 16, 16, 1, levels);
    use_texture(&t, D3DTEXF_POINT);
    quad(-0.5f, -0.5f, 15.5f, 15.5f, 0, 0, 1, 1, 0xFFFFFFFF, 0);
    case_end(NULL);
}

void probe_bc(void) {
    BYTE bc1[16 * 8], bc3[16 * 16];
    WORD c0, c1;
    for (int four = 1; four >= 0; four--) {
        for (int k = 0; k < 16; k++) {
            colours(k, four, &c0, &c1);
            colour_block(bc1 + k * 8, c0, c1);
        }
        bc_case(four ? "bc1_4colour" : "bc1_3colour", D3DFMT_DXT1, bc1);
    }
    for (int eight = 1; eight >= 0; eight--) {
        for (int k = 0; k < 16; k++) {
            alpha_block(bc3 + k * 16, k, eight);
            colour_block(bc3 + k * 16 + 8, 0xFFFF, 0xFFFF);
        }
        bc_case(eight ? "bc3_alpha8" : "bc3_alpha6", D3DFMT_DXT5, bc3);
    }
    for (int k = 0; k < 16; k++) {
        memset(bc3 + k * 16, 0, 8);
        bc3[k * 16] = 255;
        bc3[k * 16 + 1] = 255;
        colours(k, 0, &c0, &c1);
        colour_block(bc3 + k * 16 + 8, c0, c1);
    }
    bc_case("bc3_colour", D3DFMT_DXT5, bc3);
}
