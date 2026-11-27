/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Alpha test: white with alpha x at pixel x of 256 (a 256x1 texture drawn
 * 1:1 with point sampling), on transparent black, in six 4-row bands for
 * ALPHAREF 0, 1, 127, 128, 254 and 255. One case per ALPHAFUNC: never,
 * less, equal, lessequal, greater, notequal, greaterequal, always. */
#include "probe.h"

void probe_alpha_test(void) {
    static const DWORD refs[] = {0, 1, 127, 128, 254, 255};
    static const char *const funcs[] = {"never",   "less",     "equal",        "lessequal",
                                        "greater", "notequal", "greaterequal", "always"};
    DWORD texels[256];
    for (int x = 0; x < 256; x++) texels[x] = D3DCOLOR_ARGB(x, 255, 255, 255);
    for (int f = 0; f < 8; f++) {
        if (!case_begin("alpha_test", funcs[f], 256, 24, 0)) continue;
        struct texture t = texture_argb(256, 1, texels);
        use_texture(&t, D3DTEXF_POINT);
        set_rs(D3DRS_ALPHATESTENABLE, TRUE);
        set_rs(D3DRS_ALPHAFUNC, D3DCMP_NEVER + f);
        for (int r = 0; r < 6; r++) {
            set_rs(D3DRS_ALPHAREF, refs[r]);
            quad(-0.5f, r * 4 - 0.5f, 255.5f, r * 4 + 3.5f, 0, 0, 1, 1, 0xFFFFFFFF, 0);
        }
        case_end(NULL);
    }
}
