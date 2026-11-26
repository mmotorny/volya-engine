/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* `xtask wine check`: creates a Direct3D 9 device, clears its back
 * buffer to 0x2080e0 and saves it as d3d9smoke.bmp (32-bit, top-down). */
#include <windows.h>
#include <d3d9.h>
#include <stdio.h>

enum { W = 64, H = 48 };

static int fail(const char *what, HRESULT hr) {
    printf("d3d9smoke: %s failed: 0x%08lx\n", what, (unsigned long)hr);
    return 1;
}

int main(void) {
    HINSTANCE inst = GetModuleHandleA(NULL);
    WNDCLASSA wc = {0};
    wc.lpfnWndProc = DefWindowProcA;
    wc.hInstance = inst;
    wc.lpszClassName = "d3d9smoke";
    RegisterClassA(&wc);
    HWND wnd = CreateWindowA("d3d9smoke", "d3d9smoke", WS_OVERLAPPEDWINDOW, 0, 0, W, H, NULL, NULL, inst, NULL);
    if (!wnd) return fail("CreateWindow", GetLastError());
    IDirect3D9 *d3d = Direct3DCreate9(D3D_SDK_VERSION);
    if (!d3d) return fail("Direct3DCreate9", 0);
    D3DADAPTER_IDENTIFIER9 id;
    IDirect3D9_GetAdapterIdentifier(d3d, 0, 0, &id);
    printf("d3d9smoke: adapter %s\n", id.Description);

    D3DPRESENT_PARAMETERS pp = {0};
    pp.Windowed = TRUE;
    pp.SwapEffect = D3DSWAPEFFECT_DISCARD;
    pp.BackBufferFormat = D3DFMT_X8R8G8B8;
    pp.BackBufferWidth = W;
    pp.BackBufferHeight = H;
    pp.hDeviceWindow = wnd;
    IDirect3DDevice9 *dev;
    HRESULT hr = IDirect3D9_CreateDevice(d3d, 0, D3DDEVTYPE_HAL, wnd, D3DCREATE_HARDWARE_VERTEXPROCESSING, &pp, &dev);
    if (FAILED(hr)) return fail("CreateDevice", hr);
    hr = IDirect3DDevice9_Clear(dev, 0, NULL, D3DCLEAR_TARGET, D3DCOLOR_XRGB(0x20, 0x80, 0xE0), 1.0f, 0);
    if (FAILED(hr)) return fail("Clear", hr);

    IDirect3DSurface9 *back, *copy;
    hr = IDirect3DDevice9_GetBackBuffer(dev, 0, 0, D3DBACKBUFFER_TYPE_MONO, &back);
    if (FAILED(hr)) return fail("GetBackBuffer", hr);
    hr = IDirect3DDevice9_CreateOffscreenPlainSurface(dev, W, H, D3DFMT_X8R8G8B8, D3DPOOL_SYSTEMMEM, &copy, NULL);
    if (FAILED(hr)) return fail("CreateOffscreenPlainSurface", hr);
    hr = IDirect3DDevice9_GetRenderTargetData(dev, back, copy);
    if (FAILED(hr)) return fail("GetRenderTargetData", hr);
    D3DLOCKED_RECT lr;
    hr = IDirect3DSurface9_LockRect(copy, &lr, NULL, D3DLOCK_READONLY);
    if (FAILED(hr)) return fail("LockRect", hr);

    FILE *out = fopen("d3d9smoke.bmp", "wb");
    if (!out) return fail("fopen", 0);
    BITMAPFILEHEADER fh = {0};
    BITMAPINFOHEADER ih = {0};
    ih.biSize = sizeof ih;
    ih.biWidth = W;
    ih.biHeight = -H;
    ih.biPlanes = 1;
    ih.biBitCount = 32;
    fh.bfType = 0x4D42;
    fh.bfOffBits = sizeof fh + sizeof ih;
    fh.bfSize = fh.bfOffBits + W * H * 4;
    fwrite(&fh, sizeof fh, 1, out);
    fwrite(&ih, sizeof ih, 1, out);
    for (int y = 0; y < H; y++) fwrite((BYTE *)lr.pBits + lr.Pitch * y, 4, W, out);
    fclose(out);
    IDirect3DSurface9_UnlockRect(copy);
    return 0;
}
