/*
 * SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause
 */

/* `xtask wine check`, on the frame clock (docs/testing.md): waits for
 * 100 ms of the performance counter, busy, which only the clock's reads
 * advance (WINE_FRAME_SPIN); then presents
 * frames with Direct3D 9 and writes, before the first and after each, the
 * times a program reads, twice with a real 30 ms sleep between, to
 * clocksmoke.txt: a line per read of "frame qpc qpf tick time filetime tsc"
 * (performance counter and frequency, GetTickCount, timeGetTime,
 * GetSystemTimeAsFileTime, the time-stamp counter). */
#include <windows.h>
#include <d3d9.h>
#include <mmsystem.h>
#include <stdio.h>
#include <intrin.h>

enum { W = 64, H = 48, FRAMES = 4 };

static int fail(const char *what, HRESULT hr) {
    printf("clocksmoke: %s failed: 0x%08lx\n", what, (unsigned long)hr);
    return 1;
}

static void times(FILE *out, int frame) {
    LARGE_INTEGER qpc, qpf;
    FILETIME ft;
    QueryPerformanceCounter(&qpc);
    QueryPerformanceFrequency(&qpf);
    DWORD tick = GetTickCount();
    DWORD time = timeGetTime();
    GetSystemTimeAsFileTime(&ft);
    unsigned long long tsc = __rdtsc();
    fprintf(out, "%d %lld %lld %lu %lu %llu %llu\n", frame, qpc.QuadPart, qpf.QuadPart,
            (unsigned long)tick, (unsigned long)time,
            ((unsigned long long)ft.dwHighDateTime << 32) | ft.dwLowDateTime, tsc);
}

int main(void) {
    HINSTANCE inst = GetModuleHandleA(NULL);
    WNDCLASSA wc = {0};
    wc.lpfnWndProc = DefWindowProcA;
    wc.hInstance = inst;
    wc.lpszClassName = "clocksmoke";
    RegisterClassA(&wc);
    HWND wnd = CreateWindowA("clocksmoke", "clocksmoke", WS_OVERLAPPEDWINDOW, 0, 0, W, H, NULL, NULL, inst, NULL);
    if (!wnd) return fail("CreateWindow", GetLastError());
    IDirect3D9 *d3d = Direct3DCreate9(D3D_SDK_VERSION);
    if (!d3d) return fail("Direct3DCreate9", 0);
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

    LARGE_INTEGER start, now;
    QueryPerformanceCounter(&start);
    do QueryPerformanceCounter(&now);
    while (now.QuadPart - start.QuadPart < 1000000);

    FILE *out = fopen("clocksmoke.txt", "w");
    if (!out) return fail("fopen", 0);
    for (int frame = 0; frame <= FRAMES; frame++) {
        if (frame > 0) {
            hr = IDirect3DDevice9_Clear(dev, 0, NULL, D3DCLEAR_TARGET, D3DCOLOR_XRGB(0, 0, 0), 1.0f, 0);
            if (FAILED(hr)) return fail("Clear", hr);
            hr = IDirect3DDevice9_Present(dev, NULL, NULL, NULL, NULL);
            if (FAILED(hr)) return fail("Present", hr);
        }
        times(out, frame);
        Sleep(30);
        times(out, frame);
    }
    fclose(out);
    return 0;
}
