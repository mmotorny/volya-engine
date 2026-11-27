/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Texel centres and the half-pixel offset: an 8x8 texture of distinct
 * colours on a quad from (d, d) to (s + d, s + d), texture coordinates 0 to
 * 1, at 1:1 (s 8) and magnified x4 (s 32), point and linear, for offsets d
 * of 0, -0.5, -0.25 and 0.25. Cases <filter>_x<scale>_<offset>. */
#include "probe.h"

#include <stdio.h>

void probe_texel_centres(void) {
    DWORD texels[64];
    for (int y = 0; y < 8; y++)
        for (int x = 0; x < 8; x++)
            texels[y * 8 + x] = D3DCOLOR_ARGB(255 - (x + y) * 16, x * 36, y * 36, (x ^ y) & 1 ? 255 : 0);
    static const float offsets[] = {0.0f, -0.5f, -0.25f, 0.25f};
    static const char *const offset_names[] = {"0", "m0.5", "m0.25", "p0.25"};
    static const D3DTEXTUREFILTERTYPE filters[] = {D3DTEXF_POINT, D3DTEXF_LINEAR};
    static const char *const filter_names[] = {"point", "linear"};
    static const int scales[] = {1, 4};
    for (int f = 0; f < 2; f++)
        for (int s = 0; s < 2; s++)
            for (int o = 0; o < 4; o++) {
                char name[64];
                snprintf(name, sizeof name, "%s_x%d_%s", filter_names[f], scales[s], offset_names[o]);
                int size = 8 * scales[s];
                if (!case_begin("texel_centres", name, size, size, 0)) continue;
                struct texture t = texture_argb(8, 8, texels);
                use_texture(&t, filters[f]);
                float d = offsets[o];
                quad(d, d, size + d, size + d, 0, 0, 1, 1, 0xFFFFFFFF, 0);
                case_end(NULL);
            }
}
