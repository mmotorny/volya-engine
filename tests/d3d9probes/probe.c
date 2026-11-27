/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* d3d9probes.exe: runs every probe in the working directory, writing
 * results/ (see probe.h); exits with 1 if any D3D call failed. */
#include "probe.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

IDirect3DDevice9 *device;

static IDirect3D9 *d3d;
static FILE *scenes;
static int failures;

/* The case in progress. */
static char case_path[MAX_PATH];
static int case_w, case_h;
static IDirect3DTexture9 *target;
static IDirect3DSurface9 *target_surface;
enum { MAX_TEXTURES = 16 };
static IDirect3DTexture9 *textures[MAX_TEXTURES];
static int texture_count;
static DWORD next_texture_id = 1;

void failed(const char *what, HRESULT hr) {
    failures++;
    printf("d3d9probes: %s: %s failed: 0x%08lx\n", case_path, what, (unsigned long)hr);
}

static void put_u32(DWORD v) {
    BYTE b[4] = {(BYTE)v, (BYTE)(v >> 8), (BYTE)(v >> 16), (BYTE)(v >> 24)};
    fwrite(b, 1, 4, scenes);
}

static void record(DWORD tag, DWORD size) {
    put_u32(tag);
    put_u32(size);
}

DWORD float_bits(float f) {
    DWORD d;
    memcpy(&d, &f, 4);
    return d;
}

void set_rs(D3DRENDERSTATETYPE state, DWORD value) {
    record(TAG_RS, 8);
    put_u32(state);
    put_u32(value);
    HRESULT hr = IDirect3DDevice9_SetRenderState(device, state, value);
    if (FAILED(hr)) failed("SetRenderState", hr);
}

void set_tss(DWORD stage, D3DTEXTURESTAGESTATETYPE type, DWORD value) {
    record(TAG_TSS, 12);
    put_u32(stage);
    put_u32(type);
    put_u32(value);
    HRESULT hr = IDirect3DDevice9_SetTextureStageState(device, stage, type, value);
    if (FAILED(hr)) failed("SetTextureStageState", hr);
}

void set_ss(DWORD sampler, D3DSAMPLERSTATETYPE type, DWORD value) {
    record(TAG_SS, 12);
    put_u32(sampler);
    put_u32(type);
    put_u32(value);
    HRESULT hr = IDirect3DDevice9_SetSamplerState(device, sampler, type, value);
    if (FAILED(hr)) failed("SetSamplerState", hr);
}

/* The states every case starts from. */
static void defaults(void) {
    set_rs(D3DRS_ZENABLE, D3DZB_FALSE);
    set_rs(D3DRS_ZWRITEENABLE, FALSE);
    set_rs(D3DRS_FILLMODE, D3DFILL_SOLID);
    set_rs(D3DRS_SHADEMODE, D3DSHADE_GOURAUD);
    set_rs(D3DRS_CULLMODE, D3DCULL_NONE);
    set_rs(D3DRS_LIGHTING, FALSE);
    set_rs(D3DRS_DITHERENABLE, FALSE);
    set_rs(D3DRS_SPECULARENABLE, FALSE);
    set_rs(D3DRS_COLORWRITEENABLE, 0xF);
    set_rs(D3DRS_SRGBWRITEENABLE, FALSE);
    set_rs(D3DRS_ALPHABLENDENABLE, FALSE);
    set_rs(D3DRS_SRCBLEND, D3DBLEND_ONE);
    set_rs(D3DRS_DESTBLEND, D3DBLEND_ZERO);
    set_rs(D3DRS_BLENDOP, D3DBLENDOP_ADD);
    set_rs(D3DRS_ALPHATESTENABLE, FALSE);
    set_rs(D3DRS_ALPHAREF, 0);
    set_rs(D3DRS_ALPHAFUNC, D3DCMP_ALWAYS);
    set_rs(D3DRS_FOGENABLE, FALSE);
    set_rs(D3DRS_FOGCOLOR, 0);
    set_rs(D3DRS_FOGTABLEMODE, D3DFOG_NONE);
    set_rs(D3DRS_FOGVERTEXMODE, D3DFOG_NONE);
    set_rs(D3DRS_FOGSTART, float_bits(0.0f));
    set_rs(D3DRS_FOGEND, float_bits(1.0f));
    set_tss(0, D3DTSS_COLOROP, D3DTOP_SELECTARG1);
    set_tss(0, D3DTSS_COLORARG1, D3DTA_DIFFUSE);
    set_tss(0, D3DTSS_COLORARG2, D3DTA_CURRENT);
    set_tss(0, D3DTSS_ALPHAOP, D3DTOP_SELECTARG1);
    set_tss(0, D3DTSS_ALPHAARG1, D3DTA_DIFFUSE);
    set_tss(0, D3DTSS_ALPHAARG2, D3DTA_CURRENT);
    set_tss(0, D3DTSS_TEXCOORDINDEX, 0);
    set_tss(0, D3DTSS_TEXTURETRANSFORMFLAGS, D3DTTFF_DISABLE);
    set_tss(1, D3DTSS_COLOROP, D3DTOP_DISABLE);
    set_tss(1, D3DTSS_ALPHAOP, D3DTOP_DISABLE);
    set_ss(0, D3DSAMP_ADDRESSU, D3DTADDRESS_CLAMP);
    set_ss(0, D3DSAMP_ADDRESSV, D3DTADDRESS_CLAMP);
    set_ss(0, D3DSAMP_MAGFILTER, D3DTEXF_POINT);
    set_ss(0, D3DSAMP_MINFILTER, D3DTEXF_POINT);
    set_ss(0, D3DSAMP_MIPFILTER, D3DTEXF_NONE);
    set_ss(0, D3DSAMP_MIPMAPLODBIAS, float_bits(0.0f));
    set_ss(0, D3DSAMP_MAXMIPLEVEL, 0);
    set_ss(0, D3DSAMP_MAXANISOTROPY, 1);
    set_ss(0, D3DSAMP_SRGBTEXTURE, FALSE);
    set_texture(0, NULL);
}

int case_begin(const char *probe, const char *name, int w, int h, D3DCOLOR clear) {
    char dir[MAX_PATH];
    snprintf(dir, sizeof dir, "results/%s", probe);
    CreateDirectoryA(dir, NULL);
    snprintf(case_path, sizeof case_path, "results/%s/%s.png", probe, name);
    case_w = w;
    case_h = h;
    HRESULT hr = IDirect3DDevice9_CreateTexture(device, w, h, 1, D3DUSAGE_RENDERTARGET, D3DFMT_A8R8G8B8,
                                                D3DPOOL_DEFAULT, &target, NULL);
    if (FAILED(hr)) {
        failed("CreateTexture (render target)", hr);
        return 0;
    }
    IDirect3DTexture9_GetSurfaceLevel(target, 0, &target_surface);
    hr = IDirect3DDevice9_SetRenderTarget(device, 0, target_surface);
    if (FAILED(hr)) failed("SetRenderTarget", hr);

    DWORD size = 12 + (DWORD)strlen(probe) + 1 + (DWORD)strlen(name) + 1;
    record(TAG_CASE, size);
    put_u32(w);
    put_u32(h);
    put_u32(clear);
    fwrite(probe, 1, strlen(probe) + 1, scenes);
    fwrite(name, 1, strlen(name) + 1, scenes);

    hr = IDirect3DDevice9_Clear(device, 0, NULL, D3DCLEAR_TARGET, clear, 1.0f, 0);
    if (FAILED(hr)) failed("Clear", hr);
    hr = IDirect3DDevice9_SetFVF(device, VERTEX_FVF);
    if (FAILED(hr)) failed("SetFVF", hr);
    defaults();
    hr = IDirect3DDevice9_BeginScene(device);
    if (FAILED(hr)) failed("BeginScene", hr);
    return 1;
}

/* CRC-32 (ISO 3309, as PNG uses) and Adler-32. */
static DWORD crc_table[256];

static DWORD crc(DWORD c, const BYTE *p, size_t n) {
    if (!crc_table[1])
        for (DWORD i = 0; i < 256; i++) {
            DWORD k = i;
            for (int j = 0; j < 8; j++) k = k & 1 ? 0xEDB88320u ^ (k >> 1) : k >> 1;
            crc_table[i] = k;
        }
    c = ~c;
    while (n--) c = crc_table[(c ^ *p++) & 0xFF] ^ (c >> 8);
    return ~c;
}

static void be32(BYTE *b, DWORD v) {
    b[0] = (BYTE)(v >> 24);
    b[1] = (BYTE)(v >> 16);
    b[2] = (BYTE)(v >> 8);
    b[3] = (BYTE)v;
}

static void chunk(FILE *f, const char *type, const BYTE *data, size_t n) {
    BYTE b[4];
    be32(b, (DWORD)n);
    fwrite(b, 1, 4, f);
    fwrite(type, 1, 4, f);
    fwrite(data, 1, n, f);
    DWORD c = crc(crc(0, (const BYTE *)type, 4), data, n);
    be32(b, c);
    fwrite(b, 1, 4, f);
}

/* An 8-bit RGBA PNG of `rgba` (rows top-down), deflate in stored blocks. */
static int write_png(const char *path, int w, int h, const BYTE *rgba) {
    size_t row = (size_t)w * 4 + 1, raw_n = row * h;
    BYTE *raw = malloc(raw_n);
    size_t blocks = raw_n / 65535 + 1;
    size_t z_n = 2 + raw_n + blocks * 5 + 4;
    BYTE *z = malloc(z_n);
    if (!raw || !z) return 0;
    for (int y = 0; y < h; y++) {
        raw[row * y] = 0;
        memcpy(raw + row * y + 1, rgba + (size_t)w * 4 * y, (size_t)w * 4);
    }
    size_t at = 0;
    z[at++] = 0x78;
    z[at++] = 0x01;
    DWORD a = 1, b = 0;
    for (size_t done = 0;;) {
        size_t n = raw_n - done < 65535 ? raw_n - done : 65535;
        int last = done + n == raw_n;
        z[at++] = (BYTE)last;
        z[at++] = (BYTE)n;
        z[at++] = (BYTE)(n >> 8);
        z[at++] = (BYTE)~n;
        z[at++] = (BYTE)(~n >> 8);
        memcpy(z + at, raw + done, n);
        for (size_t i = 0; i < n; i++) {
            a = (a + raw[done + i]) % 65521;
            b = (b + a) % 65521;
        }
        at += n;
        done += n;
        if (last) break;
    }
    be32(z + at, b << 16 | a);
    at += 4;

    FILE *f = fopen(path, "wb");
    if (!f) {
        free(raw);
        free(z);
        return 0;
    }
    static const BYTE signature[8] = {0x89, 'P', 'N', 'G', '\r', '\n', 0x1A, '\n'};
    fwrite(signature, 1, 8, f);
    BYTE ihdr[13] = {0};
    be32(ihdr, w);
    be32(ihdr + 4, h);
    ihdr[8] = 8; /* bit depth */
    ihdr[9] = 6; /* RGBA */
    chunk(f, "IHDR", ihdr, 13);
    chunk(f, "IDAT", z, at);
    chunk(f, "IEND", NULL, 0);
    int ok = !ferror(f);
    fclose(f);
    free(raw);
    free(z);
    return ok;
}

void case_end(const D3DGAMMARAMP *ramp) {
    HRESULT hr = IDirect3DDevice9_EndScene(device);
    if (FAILED(hr)) failed("EndScene", hr);
    IDirect3DSurface9 *copy = NULL;
    hr = IDirect3DDevice9_CreateOffscreenPlainSurface(device, case_w, case_h, D3DFMT_A8R8G8B8,
                                                      D3DPOOL_SYSTEMMEM, &copy, NULL);
    if (FAILED(hr)) {
        failed("CreateOffscreenPlainSurface", hr);
    } else if (FAILED(hr = IDirect3DDevice9_GetRenderTargetData(device, target_surface, copy))) {
        failed("GetRenderTargetData", hr);
    } else {
        D3DLOCKED_RECT lr;
        hr = IDirect3DSurface9_LockRect(copy, &lr, NULL, D3DLOCK_READONLY);
        if (FAILED(hr)) {
            failed("LockRect", hr);
        } else {
            BYTE *rgba = malloc((size_t)case_w * case_h * 4);
            for (int y = 0; y < case_h; y++) {
                const BYTE *src = (const BYTE *)lr.pBits + lr.Pitch * y;
                BYTE *dst = rgba + (size_t)case_w * 4 * y;
                for (int x = 0; x < case_w; x++) {
                    BYTE r = src[x * 4 + 2], g = src[x * 4 + 1], b = src[x * 4 + 0];
                    if (ramp) {
                        r = (BYTE)(ramp->red[r] >> 8);
                        g = (BYTE)(ramp->green[g] >> 8);
                        b = (BYTE)(ramp->blue[b] >> 8);
                    }
                    dst[x * 4 + 0] = r;
                    dst[x * 4 + 1] = g;
                    dst[x * 4 + 2] = b;
                    dst[x * 4 + 3] = src[x * 4 + 3];
                }
            }
            IDirect3DSurface9_UnlockRect(copy);
            if (!write_png(case_path, case_w, case_h, rgba)) failed("writing the PNG", 0);
            free(rgba);
        }
    }
    if (copy) IDirect3DSurface9_Release(copy);
    if (ramp) {
        record(TAG_RAMP, sizeof *ramp);
        const WORD *channels[3] = {ramp->red, ramp->green, ramp->blue};
        for (int c = 0; c < 3; c++)
            for (int i = 0; i < 256; i++) {
                BYTE b[2] = {(BYTE)channels[c][i], (BYTE)(channels[c][i] >> 8)};
                fwrite(b, 1, 2, scenes);
            }
    }
    record(TAG_END, 0);

    IDirect3DDevice9_SetTexture(device, 0, NULL);
    for (int i = 0; i < texture_count; i++) IDirect3DTexture9_Release(textures[i]);
    texture_count = 0;
    IDirect3DSurface9 *back;
    if (SUCCEEDED(IDirect3DDevice9_GetBackBuffer(device, 0, 0, D3DBACKBUFFER_TYPE_MONO, &back))) {
        IDirect3DDevice9_SetRenderTarget(device, 0, back);
        IDirect3DSurface9_Release(back);
    }
    IDirect3DSurface9_Release(target_surface);
    IDirect3DTexture9_Release(target);
    target = NULL;
}

/* A level's bytes: rows of texels or of 4x4 blocks, and their count. */
static void level_size(D3DFORMAT format, int w, int h, size_t *row, int *rows) {
    if (format == D3DFMT_DXT1 || format == D3DFMT_DXT5) {
        *row = (size_t)((w + 3) / 4) * (format == D3DFMT_DXT1 ? 8 : 16);
        *rows = (h + 3) / 4;
    } else {
        *row = (size_t)w * 4;
        *rows = h;
    }
}

struct texture texture_create(D3DFORMAT format, int w, int h, int levels, const void *const *data) {
    struct texture t = {NULL, 0};
    if (texture_count == MAX_TEXTURES) {
        failed("texture_create (too many textures)", 0);
        return t;
    }
    HRESULT hr = IDirect3DDevice9_CreateTexture(device, w, h, levels, 0, format, D3DPOOL_MANAGED, &t.d3d, NULL);
    if (FAILED(hr)) {
        failed("CreateTexture", hr);
        t.d3d = NULL;
        return t;
    }
    textures[texture_count++] = t.d3d;
    t.id = next_texture_id++;
    DWORD size = 20;
    for (int l = 0; l < levels; l++) {
        size_t row;
        int rows;
        level_size(format, w >> l ? w >> l : 1, h >> l ? h >> l : 1, &row, &rows);
        size += (DWORD)(row * rows);
    }
    record(TAG_TEXTURE, size);
    put_u32(t.id);
    put_u32(format);
    put_u32(w);
    put_u32(h);
    put_u32(levels);
    for (int l = 0; l < levels; l++) {
        size_t row;
        int rows;
        level_size(format, w >> l ? w >> l : 1, h >> l ? h >> l : 1, &row, &rows);
        fwrite(data[l], row, rows, scenes);
        D3DLOCKED_RECT lr;
        hr = IDirect3DTexture9_LockRect(t.d3d, l, &lr, NULL, 0);
        if (FAILED(hr)) {
            failed("LockRect (texture)", hr);
            continue;
        }
        for (int y = 0; y < rows; y++)
            memcpy((BYTE *)lr.pBits + lr.Pitch * y, (const BYTE *)data[l] + row * y, row);
        IDirect3DTexture9_UnlockRect(t.d3d, l);
    }
    return t;
}

struct texture texture_argb(int w, int h, const DWORD *argb) {
    /* A8R8G8B8 in memory is B G R A: a little-endian DWORD 0xAARRGGBB. */
    const void *levels[1] = {argb};
    return texture_create(D3DFMT_A8R8G8B8, w, h, 1, levels);
}

void set_texture(DWORD stage, const struct texture *tex) {
    record(TAG_SET_TEXTURE, 8);
    put_u32(stage);
    put_u32(tex ? tex->id : 0);
    HRESULT hr = IDirect3DDevice9_SetTexture(device, stage, tex ? (IDirect3DBaseTexture9 *)tex->d3d : NULL);
    if (FAILED(hr)) failed("SetTexture", hr);
}

void use_texture(const struct texture *tex, D3DTEXTUREFILTERTYPE filter) {
    set_texture(0, tex);
    set_tss(0, D3DTSS_COLORARG1, D3DTA_TEXTURE);
    set_tss(0, D3DTSS_ALPHAARG1, D3DTA_TEXTURE);
    set_ss(0, D3DSAMP_MAGFILTER, filter);
    set_ss(0, D3DSAMP_MINFILTER, filter);
}

void draw(D3DPRIMITIVETYPE type, UINT primitives, const struct vertex *v, UINT count) {
    record(TAG_DRAW, 12 + count * (DWORD)sizeof *v);
    put_u32(type);
    put_u32(primitives);
    put_u32(count);
    fwrite(v, sizeof *v, count, scenes);
    HRESULT hr = IDirect3DDevice9_DrawPrimitiveUP(device, type, primitives, v, sizeof *v);
    if (FAILED(hr)) failed("DrawPrimitiveUP", hr);
}

void quad(float x0, float y0, float x1, float y1, float u0, float v0, float u1, float v1,
          D3DCOLOR diffuse, D3DCOLOR specular) {
    struct vertex v[6] = {
        {x0, y0, 0.5f, 1, diffuse, specular, u0, v0}, {x1, y0, 0.5f, 1, diffuse, specular, u1, v0},
        {x0, y1, 0.5f, 1, diffuse, specular, u0, v1}, {x1, y0, 0.5f, 1, diffuse, specular, u1, v0},
        {x1, y1, 0.5f, 1, diffuse, specular, u1, v1}, {x0, y1, 0.5f, 1, diffuse, specular, u0, v1},
    };
    draw(D3DPT_TRIANGLELIST, 2, v, 6);
}

/* results/info.txt: the adapter and the capabilities the probes touch. */
static void write_info(void) {
    FILE *f = fopen("results/info.txt", "w");
    if (!f) {
        failed("writing info.txt", 0);
        return;
    }
    D3DADAPTER_IDENTIFIER9 id;
    IDirect3D9_GetAdapterIdentifier(d3d, 0, 0, &id);
    fprintf(f, "adapter: %s\n", id.Description);
    fprintf(f, "driver: %s %u.%u.%u.%u\n", id.Driver, (unsigned)HIWORD(id.DriverVersion.HighPart),
            (unsigned)LOWORD(id.DriverVersion.HighPart), (unsigned)HIWORD(id.DriverVersion.LowPart),
            (unsigned)LOWORD(id.DriverVersion.LowPart));
    fprintf(f, "vendor: 0x%04lx\ndevice: 0x%04lx\nsubsystem: 0x%08lx\nrevision: 0x%lx\n",
            (unsigned long)id.VendorId, (unsigned long)id.DeviceId, (unsigned long)id.SubSysId,
            (unsigned long)id.Revision);
    D3DCAPS9 caps;
    IDirect3DDevice9_GetDeviceCaps(device, &caps);
    fprintf(f, "Caps2: 0x%08lx (FULLSCREENGAMMA %d, CANCALIBRATEGAMMA %d)\n", (unsigned long)caps.Caps2,
            !!(caps.Caps2 & D3DCAPS2_FULLSCREENGAMMA), !!(caps.Caps2 & D3DCAPS2_CANCALIBRATEGAMMA));
    fprintf(f, "RasterCaps: 0x%08lx (FOGVERTEX %d, FOGTABLE %d, ZFOG %d, WFOG %d, MIPMAPLODBIAS %d, ANISOTROPY %d)\n",
            (unsigned long)caps.RasterCaps, !!(caps.RasterCaps & D3DPRASTERCAPS_FOGVERTEX),
            !!(caps.RasterCaps & D3DPRASTERCAPS_FOGTABLE), !!(caps.RasterCaps & D3DPRASTERCAPS_ZFOG),
            !!(caps.RasterCaps & D3DPRASTERCAPS_WFOG), !!(caps.RasterCaps & D3DPRASTERCAPS_MIPMAPLODBIAS),
            !!(caps.RasterCaps & D3DPRASTERCAPS_ANISOTROPY));
    fprintf(f, "TextureCaps: 0x%08lx\n", (unsigned long)caps.TextureCaps);
    fprintf(f, "TextureFilterCaps: 0x%08lx\n", (unsigned long)caps.TextureFilterCaps);
    fprintf(f, "TextureAddressCaps: 0x%08lx\n", (unsigned long)caps.TextureAddressCaps);
    fprintf(f, "MaxAnisotropy: %lu\n", (unsigned long)caps.MaxAnisotropy);
    fprintf(f, "MaxTextureWidth: %lu\nMaxTextureHeight: %lu\n", (unsigned long)caps.MaxTextureWidth,
            (unsigned long)caps.MaxTextureHeight);
    fprintf(f, "AlphaCmpCaps: 0x%08lx\n", (unsigned long)caps.AlphaCmpCaps);
    fprintf(f, "SrcBlendCaps: 0x%08lx\nDestBlendCaps: 0x%08lx\n", (unsigned long)caps.SrcBlendCaps,
            (unsigned long)caps.DestBlendCaps);
    fprintf(f, "ShadeCaps: 0x%08lx\n", (unsigned long)caps.ShadeCaps);
    fprintf(f, "PrimitiveMiscCaps: 0x%08lx\n", (unsigned long)caps.PrimitiveMiscCaps);
    fprintf(f, "TextureOpCaps: 0x%08lx\n", (unsigned long)caps.TextureOpCaps);
    fprintf(f, "PixelShaderVersion: %lu.%lu\nVertexShaderVersion: %lu.%lu\n",
            (unsigned long)(caps.PixelShaderVersion >> 8 & 0xFF), (unsigned long)(caps.PixelShaderVersion & 0xFF),
            (unsigned long)(caps.VertexShaderVersion >> 8 & 0xFF), (unsigned long)(caps.VertexShaderVersion & 0xFF));
    D3DDISPLAYMODE mode;
    IDirect3D9_GetAdapterDisplayMode(d3d, 0, &mode);
    static const D3DFORMAT formats[] = {D3DFMT_DXT1, D3DFMT_DXT5};
    static const char *const names[] = {"DXT1", "DXT5"};
    for (int i = 0; i < 2; i++)
        fprintf(f, "%s textures: %s\n", names[i],
                SUCCEEDED(IDirect3D9_CheckDeviceFormat(d3d, 0, D3DDEVTYPE_HAL, mode.Format, 0, D3DRTYPE_TEXTURE,
                                                       formats[i]))
                    ? "yes"
                    : "no");
    fprintf(f, "A8R8G8B8 render target: %s\n",
            SUCCEEDED(IDirect3D9_CheckDeviceFormat(d3d, 0, D3DDEVTYPE_HAL, mode.Format, D3DUSAGE_RENDERTARGET,
                                                   D3DRTYPE_TEXTURE, D3DFMT_A8R8G8B8))
                ? "yes"
                : "no");
    fclose(f);
}

int main(void) {
    strcpy(case_path, "results");
    CreateDirectoryA("results", NULL);
    HINSTANCE inst = GetModuleHandleA(NULL);
    WNDCLASSA wc = {0};
    wc.lpfnWndProc = DefWindowProcA;
    wc.hInstance = inst;
    wc.lpszClassName = "d3d9probes";
    RegisterClassA(&wc);
    HWND wnd = CreateWindowA("d3d9probes", "d3d9probes", WS_OVERLAPPEDWINDOW, 0, 0, 64, 64, NULL, NULL, inst, NULL);
    if (!wnd) {
        failed("CreateWindow", GetLastError());
        return 1;
    }
    d3d = Direct3DCreate9(D3D_SDK_VERSION);
    if (!d3d) {
        failed("Direct3DCreate9", 0);
        return 1;
    }
    D3DPRESENT_PARAMETERS pp = {0};
    pp.Windowed = TRUE;
    pp.SwapEffect = D3DSWAPEFFECT_DISCARD;
    pp.BackBufferFormat = D3DFMT_X8R8G8B8;
    pp.BackBufferWidth = 64;
    pp.BackBufferHeight = 64;
    pp.hDeviceWindow = wnd;
    HRESULT hr = IDirect3D9_CreateDevice(d3d, 0, D3DDEVTYPE_HAL, wnd, D3DCREATE_HARDWARE_VERTEXPROCESSING, &pp, &device);
    if (FAILED(hr))
        hr = IDirect3D9_CreateDevice(d3d, 0, D3DDEVTYPE_HAL, wnd, D3DCREATE_SOFTWARE_VERTEXPROCESSING, &pp, &device);
    if (FAILED(hr)) {
        failed("CreateDevice", hr);
        return 1;
    }
    write_info();
    scenes = fopen("results/scenes.bin", "wb");
    if (!scenes) {
        failed("opening scenes.bin", 0);
        return 1;
    }
    fwrite("VD9P", 1, 4, scenes);
    put_u32(1);

    probe_texel_centres();
    probe_raster();
    probe_bilinear();
    probe_mip();
    probe_aniso();
    probe_bc();
    probe_alpha_test();
    probe_blend();
    probe_fog();
    probe_gamma();

    if (fclose(scenes) != 0) failed("writing scenes.bin", 0);
    IDirect3DDevice9_Release(device);
    IDirect3D9_Release(d3d);
    printf("d3d9probes: %d failure(s)\n", failures);
    return failures ? 1 : 0;
}
