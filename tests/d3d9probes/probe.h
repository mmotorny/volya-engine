/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* The Direct3D 9 probes' shared helpers. Each case draws pre-transformed
 * vertices into an A8R8G8B8 render-target texture and writes it as
 * results/<probe>/<case>.png (RGBA, alpha kept). Every D3D call a case makes
 * goes through these helpers, which also log it to results/scenes.bin, so a
 * replay draws the same scenes from the log.
 *
 * scenes.bin, little-endian: the magic "VD9P", a u32 version (1), then
 * records of a u32 tag, a u32 payload size in bytes and the payload:
 *   CASE        u32 width, u32 height, u32 clear colour (ARGB), the probe's
 *               name and the case's name, each NUL-terminated. The render
 *               target is cleared to the colour; the states listed in
 *               probe.c's defaults follow as RS, TSS, SS and SET_TEXTURE
 *               records.
 *   RS          u32 D3DRENDERSTATETYPE, u32 value
 *   TSS         u32 stage, u32 D3DTEXTURESTAGESTATETYPE, u32 value
 *   SS          u32 sampler, u32 D3DSAMPLERSTATETYPE, u32 value
 *   TEXTURE     u32 id (from 1), u32 D3DFORMAT, u32 width, u32 height,
 *               u32 levels, then each level's texels from level 0, rows
 *               packed (A8R8G8B8: 4 bytes per texel as D3D stores them,
 *               B G R A; DXT1/DXT5: rows of 4x4 blocks, 8 or 16 bytes each)
 *   SET_TEXTURE u32 stage, u32 id (0: none)
 *   DRAW        u32 D3DPRIMITIVETYPE, u32 primitive count, u32 vertex
 *               count, then the vertices (struct vertex, 32 bytes each)
 *   RAMP        the gamma ramp read back (D3DGAMMARAMP: 256 u16 red, green,
 *               blue); the PNG's colours are mapped through it (the value
 *               v becomes ramp[v] >> 8) on the CPU
 *   END         nothing: the case's PNG is written
 * A texture's id stays valid until its case's END. */
#ifndef PROBE_H
#define PROBE_H

#include <windows.h>
#include <d3d9.h>

enum {
    TAG_CASE = 1,
    TAG_RS = 2,
    TAG_TSS = 3,
    TAG_SS = 4,
    TAG_TEXTURE = 5,
    TAG_SET_TEXTURE = 6,
    TAG_DRAW = 7,
    TAG_RAMP = 8,
    TAG_END = 9,
};

/* D3DFVF_XYZRHW | D3DFVF_DIFFUSE | D3DFVF_SPECULAR | D3DFVF_TEX1. */
struct vertex {
    float x, y, z, rhw;
    DWORD diffuse, specular;
    float u, v;
};
#define VERTEX_FVF (D3DFVF_XYZRHW | D3DFVF_DIFFUSE | D3DFVF_SPECULAR | D3DFVF_TEX1)

/* A texture made by texture_create: its D3D object and its log id. */
struct texture {
    IDirect3DTexture9 *d3d;
    DWORD id;
};

extern IDirect3DDevice9 *device;

/* Starts a case: a w x h render target cleared to `clear`, and the default
 * states. Returns 0 if the target can't be made (the case is skipped). */
int case_begin(const char *probe, const char *name, int w, int h, D3DCOLOR clear);
/* Reads the target back and writes the case's PNG; with `ramp`, its colours
 * mapped through it first (logged as RAMP). Releases the case's textures. */
void case_end(const D3DGAMMARAMP *ramp);

void set_rs(D3DRENDERSTATETYPE state, DWORD value);
void set_tss(DWORD stage, D3DTEXTURESTAGESTATETYPE type, DWORD value);
void set_ss(DWORD sampler, D3DSAMPLERSTATETYPE type, DWORD value);
/* A float state's value (FOGSTART, MIPMAPLODBIAS). */
DWORD float_bits(float f);

/* A texture with `levels` levels, each level's data rows packed (see
 * TEXTURE above); its `d3d` is NULL if it can't be made. Released at the
 * case's end. */
struct texture texture_create(D3DFORMAT format, int w, int h, int levels, const void *const *data);
/* An A8R8G8B8 texture of one level from ARGB values. */
struct texture texture_argb(int w, int h, const DWORD *argb);
void set_texture(DWORD stage, const struct texture *tex);
/* Stage 0's colour and alpha from the texture, sampled with `filter`. */
void use_texture(const struct texture *tex, D3DTEXTUREFILTERTYPE filter);

void draw(D3DPRIMITIVETYPE type, UINT primitives, const struct vertex *v, UINT count);
/* A rectangle as two triangles: (x0, y0) to (x1, y1) with texture
 * coordinates (u0, v0) to (u1, v1), z 0.5, rhw 1. */
void quad(float x0, float y0, float x1, float y1, float u0, float v0, float u1, float v1,
          D3DCOLOR diffuse, D3DCOLOR specular);

/* Counts a failure (the run exits with 1) and prints it. */
void failed(const char *what, HRESULT hr);

void probe_texel_centres(void);
void probe_raster(void);
void probe_bilinear(void);
void probe_mip(void);
void probe_aniso(void);
void probe_bc(void);
void probe_alpha_test(void);
void probe_blend(void);
void probe_fog(void);
void probe_gamma(void);

#endif
