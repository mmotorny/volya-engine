/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* Gamma ramp precision: the windowed device's ramp is set, read back, and
 * a 256x8 grey gradient (x at pixel x) drawn 1:1 is mapped through the
 * ramp read back on the CPU (value v to ramp[v] >> 8). Computed, not
 * scanned out: neither Xvfb nor a Windows screenshot shows the hardware
 * ramp (believed). Cases identity (257 v), power_0.5 and power_2
 * (65535 (v / 255)^p, rounded) and low_byte (256 v + 255). */
#include "probe.h"

#include <math.h>
#include <stdio.h>
#include <string.h>

static void gamma_case(const char *name, double power, int low_byte) {
    if (!case_begin("gamma", name, 256, 8, 0)) return;
    D3DGAMMARAMP ramp, back;
    for (int i = 0; i < 256; i++) {
        WORD w = low_byte ? (WORD)(i * 256 + 255) : (WORD)floor(65535 * pow(i / 255.0, power) + 0.5);
        ramp.red[i] = ramp.green[i] = ramp.blue[i] = w;
    }
    IDirect3DDevice9_SetGammaRamp(device, 0, D3DSGR_NO_CALIBRATION, &ramp);
    /* GetGammaRamp returns nothing to check: a ramp it leaves alone reads
     * back as zeros (Wine's windowed device, measured). */
    memset(&back, 0, sizeof back);
    IDirect3DDevice9_GetGammaRamp(device, 0, &back);
    if (memcmp(&ramp, &back, sizeof ramp) != 0)
        printf("d3d9probes: gamma/%s: the ramp read back differs from the one set\n", name);
    DWORD texels[256];
    for (int x = 0; x < 256; x++) texels[x] = D3DCOLOR_ARGB(255, x, x, x);
    struct texture t = texture_argb(256, 1, texels);
    use_texture(&t, D3DTEXF_POINT);
    quad(-0.5f, -0.5f, 255.5f, 7.5f, 0, 0, 1, 1, 0xFFFFFFFF, 0);
    case_end(&back);
}

void probe_gamma(void) {
    gamma_case("identity", 1, 0);
    gamma_case("power_0.5", 0.5, 0);
    gamma_case("power_2", 2, 0);
    gamma_case("low_byte", 1, 1);
    D3DGAMMARAMP identity;
    for (int i = 0; i < 256; i++) identity.red[i] = identity.green[i] = identity.blue[i] = (WORD)(i * 257);
    IDirect3DDevice9_SetGammaRamp(device, 0, D3DSGR_NO_CALIBRATION, &identity);
}
