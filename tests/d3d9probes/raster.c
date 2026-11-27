/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Rasterization: which pixels a triangle covers. Pixel centres are at
 * integer coordinates. Cases:
 *   top_left      rectangles and triangles with edges exactly on pixel
 *                 centres (the top-left fill rule decides), white
 *   rect_offsets  4x4 rectangles moved by 0, 0.25, 0.5 and 0.75, white
 *   shared_edges  a grid of irregular cells split into triangles sharing
 *                 edges, added (ONE/ONE) in 0x40: a pixel covered twice is
 *                 0x80, one missed 0
 *   fan           a 24-slice fan around a pixel centre, added in 0x40
 *   thin          slivers 0.05 to 0.9 pixels wide at assorted angles, white
 *   degenerate    zero-area triangles (repeated and collinear vertices),
 *                 added in 0x40 */
#include "probe.h"

#include <math.h>

#define GREY 0x40404040u
#define WHITE 0xFFFFFFFFu

static struct vertex at(float x, float y, D3DCOLOR c) {
    struct vertex v = {x, y, 0.5f, 1, c, 0, 0, 0};
    return v;
}

static void triangle(float x0, float y0, float x1, float y1, float x2, float y2, D3DCOLOR c) {
    struct vertex v[3] = {at(x0, y0, c), at(x1, y1, c), at(x2, y2, c)};
    draw(D3DPT_TRIANGLELIST, 1, v, 3);
}

static void additive(void) {
    set_rs(D3DRS_ALPHABLENDENABLE, TRUE);
    set_rs(D3DRS_SRCBLEND, D3DBLEND_ONE);
    set_rs(D3DRS_DESTBLEND, D3DBLEND_ONE);
}

void probe_raster(void) {
    if (case_begin("raster", "top_left", 32, 32, 0)) {
        quad(2, 2, 6, 6, 0, 0, 0, 0, WHITE, 0);
        quad(8, 2, 9, 3, 0, 0, 0, 0, WHITE, 0);
        quad(11, 2, 11, 6, 0, 0, 0, 0, WHITE, 0); /* zero width */
        triangle(14, 2, 22, 2, 14, 10, WHITE);     /* top and left edges */
        triangle(30, 2, 30, 10, 22, 10, WHITE);    /* right and bottom edges */
        triangle(2, 12, 10, 20, 2, 20, WHITE);     /* a 45-degree edge through centres */
        triangle(12, 12, 20, 12, 16, 20, WHITE);
        triangle(22, 20, 26, 12, 30, 20, WHITE);
        triangle(2, 22, 12, 27, 2, 30, WHITE); /* slope 1/2 */
        triangle(14, 22, 30, 22, 30, 30, WHITE);
        case_end(NULL);
    }
    if (case_begin("raster", "rect_offsets", 32, 32, 0)) {
        static const float o[] = {0.0f, 0.25f, 0.5f, 0.75f};
        for (int y = 0; y < 4; y++)
            for (int x = 0; x < 4; x++) {
                float x0 = 1 + x * 8 + o[x], y0 = 1 + y * 8 + o[y];
                quad(x0, y0, x0 + 4, y0 + 4, 0, 0, 0, 0, WHITE, 0);
            }
        case_end(NULL);
    }
    if (case_begin("raster", "shared_edges", 32, 32, 0)) {
        additive();
        /* Inner corners moved by -1.5 to 1.5 in quarter-pixel steps. */
        float px[5][5], py[5][5];
        for (int j = 0; j < 5; j++)
            for (int i = 0; i < 5; i++) {
                int inner_x = i > 0 && i < 4, inner_y = j > 0 && j < 4;
                px[j][i] = i * 8.0f + inner_x * ((i * 7 + j * 3) % 13 - 6) * 0.25f;
                py[j][i] = j * 8.0f + inner_y * ((i * 5 + j * 11) % 13 - 6) * 0.25f;
            }
        for (int j = 0; j < 4; j++)
            for (int i = 0; i < 4; i++) {
                float ax = px[j][i], ay = py[j][i], bx = px[j][i + 1], by = py[j][i + 1];
                float cx = px[j + 1][i + 1], cy = py[j + 1][i + 1], dx = px[j + 1][i], dy = py[j + 1][i];
                if ((i + j) & 1) {
                    triangle(ax, ay, bx, by, cx, cy, GREY);
                    triangle(ax, ay, cx, cy, dx, dy, GREY);
                } else {
                    triangle(ax, ay, bx, by, dx, dy, GREY);
                    triangle(bx, by, cx, cy, dx, dy, GREY);
                }
            }
        case_end(NULL);
    }
    if (case_begin("raster", "fan", 32, 32, 0)) {
        additive();
        struct vertex v[26];
        v[0] = at(16, 16, GREY);
        for (int i = 0; i < 24; i++) {
            double a = i * 6.283185307179586 / 24;
            v[i + 1] = at((float)(16 + 14 * cos(a)), (float)(16 + 14 * sin(a)), GREY);
        }
        v[25] = v[1];
        draw(D3DPT_TRIANGLEFAN, 24, v, 26);
        case_end(NULL);
    }
    if (case_begin("raster", "thin", 32, 32, 0)) {
        for (int i = 0; i < 9; i++) {
            float w = 0.1f * (i + 1), x = 1.0f + i * 3.4f, y = 17 + i * 1.6f;
            quad(x, 1, x + w, 14, 0, 0, 0, 0, WHITE, 0);
            quad(1, y, 31, y + w * 0.5f, 0, 0, 0, 0, WHITE, 0);
        }
        /* Slivers tapering to a point, at a slant. */
        for (int i = 0; i < 5; i++) {
            float y = 2.5f + i * 2.5f, w = 0.2f * (i + 1);
            triangle(31, y, 31, y + w, 18, y + 14, WHITE);
        }
        case_end(NULL);
    }
    if (case_begin("raster", "degenerate", 16, 16, 0)) {
        additive();
        triangle(4, 4, 4, 4, 4, 4, GREY);
        triangle(2, 2, 12, 2, 2, 2, GREY);
        triangle(2, 6, 7, 8, 12, 10, GREY);
        triangle(8, 1, 8, 15, 8, 8, GREY);
        triangle(1.5f, 13.5f, 14.5f, 13.5f, 8, 13.5f, GREY);
        case_end(NULL);
    }
}
