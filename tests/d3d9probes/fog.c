/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Fog on a 256x8 target: rows 0-3 white, rows 4-7 black (diffuse), fog
 * colour 0x2080E0, from left to right more fog. Cases:
 *   vertex             fog factor from the specular alpha, 255 at the
 *                      left edge to 0 at the right (FOGTABLEMODE NONE)
 *   table_linear       FOGTABLEMODE LINEAR on z, 0 at the left edge to 1
 *                      at the right, FOGSTART 0.25, FOGEND 0.75
 *   table_linear_full  the same with FOGSTART 0, FOGEND 1 */
#include "probe.h"

static void band(float y0, float y1, D3DCOLOR diffuse, int table) {
    /* The specular alpha: the vertex fog factor, 255 (none) without it. */
    DWORD left = 0xFF000000, right = table ? 0xFF000000 : 0;
    float zl = table ? 0.0f : 0.5f, zr = table ? 1.0f : 0.5f;
    struct vertex a = {-0.5f, y0, zl, 1, diffuse, left, 0, 0};
    struct vertex b = {255.5f, y0, zr, 1, diffuse, right, 0, 0};
    struct vertex c = {-0.5f, y1, zl, 1, diffuse, left, 0, 0};
    struct vertex d = {255.5f, y1, zr, 1, diffuse, right, 0, 0};
    struct vertex v[6] = {a, b, c, b, d, c};
    draw(D3DPT_TRIANGLELIST, 2, v, 6);
}

static void fog_case(const char *name, int table, float start, float end) {
    if (!case_begin("fog", name, 256, 8, 0)) return;
    set_rs(D3DRS_FOGENABLE, TRUE);
    set_rs(D3DRS_FOGCOLOR, 0x002080E0);
    if (table) {
        set_rs(D3DRS_FOGTABLEMODE, D3DFOG_LINEAR);
        set_rs(D3DRS_FOGSTART, float_bits(start));
        set_rs(D3DRS_FOGEND, float_bits(end));
    }
    band(-0.5f, 3.5f, 0xFFFFFFFF, table);
    band(3.5f, 7.5f, 0xFF000000, table);
    case_end(NULL);
}

void probe_fog(void) {
    fog_case("vertex", 0, 0, 0);
    fog_case("table_linear", 1, 0.25f, 0.75f);
    fog_case("table_linear_full", 1, 0, 1);
}
