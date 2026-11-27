/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The frame clock for the original on Windows: a proxy winmm.dll, test-only,
 * placed in the game's bin\ (of the capture's copy of the install) next to
 * XR_3DA.exe, which imports winmm.dll by name, as does xrCore.dll, so the
 * loader takes this one from the program's folder before the system's
 * (winmm.dll is not a known DLL) and runs its DllMain before any of the
 * game's own libraries' initialisation.
 *
 * Every export of the system's winmm.dll is forwarded to it, except its two
 * clocks, timeGetTime and timeGetSystemTime. When the driver's shared
 * memory exists (frameclock.h), the proxy also replaces, in the import
 * address tables of every module that is not Windows' own (the game's
 * folder and the side-by-side C runtime), the kernel32 functions that read
 * a clock: the performance counter and its frequency, the tick counts and
 * the system and local times (the functions the game's modules import, read
 * from their import tables). All of them, and winmm's, then read one count
 * of 100 ns ticks that stands still between frames and advances by `step`
 * each time a Direct3D 9 device presents one; the system time starts at
 * `epoch`, the counters at fixed values, so every run reads the same times.
 * A wait for time to pass on the main thread with no frame presented would
 * never end, so every `spin` clock reads by the main thread since the last
 * frame add 1 ms (libTAS's time-tracking does so, documented in its source;
 * the Wine frame clock did the same, docs/testing.md).
 *
 * The device is reached through the game's own lookup of Direct3DCreate9
 * (GetProcAddress, replaced the same way); its CreateDevice and the
 * device's Present are replaced in their interface tables. After presenting
 * a frame, the proxy holds the presenting thread while the frame is at or
 * past the driver's `hold`, and before presenting the frame the driver asks
 * for it copies its back buffer into the shared memory.
 *
 * Generic Win32 technique (import-table and interface-table patching); it
 * reads nothing of the game but its import tables (docs/clean-room.md). */

#define WIN32_LEAN_AND_MEAN
#define PSAPI_VERSION 2
#include <windows.h>
#include <psapi.h>
#include <d3d9.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#include "frameclock.h"

/* --- Forwarding --------------------------------------------------------- */

/* Each forwarded export is a jump through its pointer to the system
 * function, resolved when the proxy loads. */
#define X(name)                                                            \
    void *real_##name;                                                     \
    __asm__(".globl _stub_" #name "\n_stub_" #name ":\n\tjmp *_real_" #name \
            "\n.section .drectve\n\t.ascii \" -export:" #name "=stub_" #name \
            "\"\n.text\n");
#include "winmm_exports.h"
#undef X

/* An export the system's winmm.dll lacks: nothing imports it, so a call is
 * a broken program. */
static void missing(void) {
    RaiseException(0xe0000001, EXCEPTION_NONCONTINUABLE, 0, NULL);
}

static HMODULE system_winmm;
static DWORD(WINAPI *real_timeGetTime)(void);
static UINT(WINAPI *real_timeGetSystemTime)(void *, UINT);

static void forward(void) {
    WCHAR path[MAX_PATH];
    UINT n = GetSystemDirectoryW(path, MAX_PATH);
    if (n == 0 || n + 11 >= MAX_PATH) {
        return;
    }
    memcpy(path + n, L"\\winmm.dll", 11 * sizeof(WCHAR));
    system_winmm = LoadLibraryW(path);
#define X(name)                                                                  \
    real_##name = system_winmm ? (void *)GetProcAddress(system_winmm, #name) : 0; \
    if (!real_##name) {                                                          \
        real_##name = (void *)missing;                                           \
    }
#include "winmm_exports.h"
#undef X
    if (system_winmm) {
        real_timeGetTime = (void *)GetProcAddress(system_winmm, "timeGetTime");
        real_timeGetSystemTime = (void *)GetProcAddress(system_winmm, "timeGetSystemTime");
    }
}

/* --- The clock ---------------------------------------------------------- */

static struct frameclock *clock_; /* the shared memory, or null: no clock */
static DWORD main_thread;
static volatile LONG reads_since_frame;

/* The counters' values at tick 0: an hour since start-up, the performance
 * counter at 10 MHz (100 ns, a tick). */
#define COUNTER_BASE 36000000000ull
#define TICK_COUNT_BASE_MS 3600000ull
#define COUNTER_FREQUENCY 10000000ull

static LONGLONG load(volatile int64_t *p) {
    return InterlockedCompareExchange64((volatile LONGLONG *)p, 0, 0);
}

static void store(volatile int64_t *p, LONGLONG v) {
    InterlockedExchange64((volatile LONGLONG *)p, v);
}

/* The clock in ticks, counting a read by the main thread. */
static ULONGLONG ticks(void) {
    if (GetCurrentThreadId() == main_thread) {
        InterlockedIncrement64((volatile LONGLONG *)&clock_->reads);
        if (clock_->spin && (ULONGLONG)InterlockedIncrement(&reads_since_frame) >= clock_->spin) {
            InterlockedExchange(&reads_since_frame, 0);
            InterlockedAdd64((volatile LONGLONG *)&clock_->extra, (LONGLONG)FRAMECLOCK_TICKS_PER_MS);
        }
    } else {
        InterlockedIncrement64((volatile LONGLONG *)&clock_->other_reads);
    }
    return (ULONGLONG)load(&clock_->frames) * clock_->step + (ULONGLONG)load(&clock_->extra) +
           (ULONGLONG)load(&clock_->sleep_ticks);
}

/* --- The proxy's log ---------------------------------------------------- */

static HANDLE log_file = INVALID_HANDLE_VALUE;
static CRITICAL_SECTION log_lock;

/* A line in the proxy's log (the driver names it), with the real
 * milliseconds since start-up and the thread. */
static void dlog(const char *fmt, ...) {
    if (log_file == INVALID_HANDLE_VALUE) {
        return;
    }
    char line[512];
    int n = snprintf(line, sizeof line, "%10lu ms  thread %5lu  ", GetTickCount(),
                     GetCurrentThreadId());
    va_list ap;
    va_start(ap, fmt);
    int m = vsnprintf(line + n, sizeof line - (size_t)n - 2, fmt, ap);
    va_end(ap);
    n += m < 0 ? 0 : (m > (int)(sizeof line) - n - 3 ? (int)(sizeof line) - n - 3 : m);
    line[n++] = '\r';
    line[n++] = '\n';
    DWORD w;
    EnterCriticalSection(&log_lock);
    WriteFile(log_file, line, (DWORD)n, &w, NULL);
    LeaveCriticalSection(&log_lock);
}

static void open_log(const char *path) {
    InitializeCriticalSection(&log_lock);
    if (path[0]) {
        log_file = CreateFileA(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL,
                               OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    }
}

/* Who reads the clock: each function's calls by calling module, for the
 * log (the heartbeat writes them when they change). */
static HMODULE self;
static struct caller {
    const char *fn;
    HMODULE module;
    volatile LONG count;
} callers[96];
static volatile LONG ncallers;

static void note_caller(const char *fn, void *ret) {
    HMODULE m = NULL;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                           GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       (LPCWSTR)ret, &m);
    if (m == self) {
        return;
    }
    LONG n = ncallers < 96 ? ncallers : 96;
    for (LONG i = 0; i < n; i++) {
        if (callers[i].fn == fn && callers[i].module == m) {
            InterlockedIncrement(&callers[i].count);
            return;
        }
    }
    LONG i = InterlockedIncrement(&ncallers) - 1;
    if (i < 96) {
        callers[i].module = m;
        callers[i].count = 1;
        callers[i].fn = fn;
    }
}

#define CALLER(fn) note_caller(fn, __builtin_return_address(0))

static BOOL WINAPI fake_QueryPerformanceCounter(LARGE_INTEGER *v) {
    CALLER("QueryPerformanceCounter");
    v->QuadPart = (LONGLONG)(COUNTER_BASE + ticks());
    return TRUE;
}

static BOOL WINAPI fake_QueryPerformanceFrequency(LARGE_INTEGER *v) {
    CALLER("QueryPerformanceFrequency");
    v->QuadPart = (LONGLONG)COUNTER_FREQUENCY;
    return TRUE;
}

static ULONGLONG WINAPI fake_GetTickCount64(void) {
    CALLER("GetTickCount64");
    return TICK_COUNT_BASE_MS + ticks() / FRAMECLOCK_TICKS_PER_MS;
}

static DWORD WINAPI fake_GetTickCount(void) {
    CALLER("GetTickCount");
    return (DWORD)fake_GetTickCount64();
}

static void WINAPI fake_GetSystemTimeAsFileTime(FILETIME *t) {
    CALLER("GetSystemTimeAsFileTime");
    ULONGLONG v = clock_->epoch + ticks();
    t->dwLowDateTime = (DWORD)v;
    t->dwHighDateTime = (DWORD)(v >> 32);
}

static void WINAPI fake_GetSystemTime(SYSTEMTIME *t) {
    CALLER("GetSystemTime");
    FILETIME f;
    fake_GetSystemTimeAsFileTime(&f);
    FileTimeToSystemTime(&f, t);
}

static void WINAPI fake_GetLocalTime(SYSTEMTIME *t) {
    CALLER("GetLocalTime");
    SYSTEMTIME utc;
    fake_GetSystemTime(&utc);
    if (!SystemTimeToTzSpecificLocalTime(NULL, &utc, t)) {
        *t = utc;
    }
}

/* winmm's own clocks, exported by the proxy. Without the clock, the
 * system's. */
__declspec(dllexport) DWORD WINAPI timeGetTime(void) {
    CALLER("timeGetTime");
    if (!clock_) {
        return real_timeGetTime ? real_timeGetTime() : 0;
    }
    return fake_GetTickCount();
}

/* MMTIME with wType TIME_MS (1) and its ms field. */
struct mmtime_ms {
    UINT type;
    DWORD ms;
};

__declspec(dllexport) UINT WINAPI timeGetSystemTime(void *mmt, UINT size) {
    CALLER("timeGetSystemTime");
    if (!clock_) {
        return real_timeGetSystemTime ? real_timeGetSystemTime(mmt, size) : 1;
    }
    if (!mmt || size < sizeof(struct mmtime_ms)) {
        return 11; /* MMSYSERR_INVALPARAM */
    }
    ((struct mmtime_ms *)mmt)->type = 1;
    ((struct mmtime_ms *)mmt)->ms = fake_GetTickCount();
    return 0;
}

/* --- Direct3D 9: the frames -------------------------------------------- */

typedef IDirect3D9 *(WINAPI *create9_fn)(UINT);
typedef HRESULT(WINAPI *create_device_fn)(IDirect3D9 *, UINT, D3DDEVTYPE, HWND, DWORD,
                                          D3DPRESENT_PARAMETERS *, IDirect3DDevice9 **);
typedef HRESULT(WINAPI *present_fn)(IDirect3DDevice9 *, const RECT *, const RECT *, HWND,
                                    const RGNDATA *);

static create9_fn real_Direct3DCreate9;
static create_device_fn real_CreateDevice;
static present_fn real_Present;
static BOOL layout_set;

/* Writes `value` into the interface-table slot `slot` (Direct3D's, read
 * only), keeping the slot's function in `*real` the first time. */
static void dlog(const char *fmt, ...);

/* The module holding `address`, as a path, for the log. */
static void module_of(const void *address, char *out, DWORD n) {
    HMODULE m = NULL;
    out[0] = 0;
    if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                               GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                           (LPCSTR)address, &m)) {
        GetModuleFileNameA(m, out, n);
    }
}

static void patch_slot_named(const char *name, void **slot, void *value, void **real) {
    DWORD old = 0;
    char table[MAX_PATH], function[MAX_PATH];
    if (*slot == value) {
        dlog("%s: its slot %p holds the hook already", name, (void *)slot);
        return;
    }
    if (!*real) {
        *real = *slot;
    }
    module_of(slot, table, MAX_PATH);
    module_of(*slot, function, MAX_PATH);
    BOOL ok = VirtualProtect(slot, sizeof(void *), PAGE_READWRITE, &old);
    DWORD error = ok ? 0 : GetLastError();
    if (ok) {
        *slot = value;
        VirtualProtect(slot, sizeof(void *), old, &old);
        FlushInstructionCache(GetCurrentProcess(), slot, sizeof(void *));
    }
    dlog("%s: slot %p (table in %s, function %p in %s, protection 0x%lx): %s", name, (void *)slot,
         table, *real, function, (unsigned long)old,
         *slot == value ? "hooked, read back" : (ok ? "NOT HOOKED: wrote but reads back unchanged" : "NOT HOOKED: VirtualProtect failed"));
    if (!ok) {
        dlog("%s: VirtualProtect error %lu", name, (unsigned long)error);
    }
}

#define patch_slot(slot, value, real) patch_slot_named(#slot, slot, value, real)

/* The back buffer of the frame about to be presented, into the shared
 * memory, as B G R X rows top first. */
static void copy_back_buffer(IDirect3DDevice9 *dev, LONGLONG frame) {
    IDirect3DSurface9 *back = NULL, *resolved = NULL, *sys = NULL;
    D3DSURFACE_DESC desc;
    D3DLOCKED_RECT lr;
    HRESULT hr = IDirect3DDevice9_GetBackBuffer(dev, 0, 0, D3DBACKBUFFER_TYPE_MONO, &back);
    if (SUCCEEDED(hr)) {
        hr = IDirect3DSurface9_GetDesc(back, &desc);
    }
    if (SUCCEEDED(hr) && (desc.Format != D3DFMT_X8R8G8B8 && desc.Format != D3DFMT_A8R8G8B8)) {
        hr = E_NOTIMPL;
    }
    if (SUCCEEDED(hr) && (ULONGLONG)desc.Width * desc.Height > FRAMECLOCK_MAX_PIXELS) {
        hr = E_OUTOFMEMORY;
    }
    IDirect3DSurface9 *source = back;
    if (SUCCEEDED(hr) && desc.MultiSampleType != D3DMULTISAMPLE_NONE) {
        hr = IDirect3DDevice9_CreateRenderTarget(dev, desc.Width, desc.Height, desc.Format,
                                                 D3DMULTISAMPLE_NONE, 0, FALSE, &resolved, NULL);
        if (SUCCEEDED(hr)) {
            hr = IDirect3DDevice9_StretchRect(dev, back, NULL, resolved, NULL, D3DTEXF_NONE);
            source = resolved;
        }
    }
    if (SUCCEEDED(hr)) {
        hr = IDirect3DDevice9_CreateOffscreenPlainSurface(dev, desc.Width, desc.Height, desc.Format,
                                                          D3DPOOL_SYSTEMMEM, &sys, NULL);
    }
    if (SUCCEEDED(hr)) {
        hr = IDirect3DDevice9_GetRenderTargetData(dev, source, sys);
    }
    if (SUCCEEDED(hr)) {
        hr = IDirect3DSurface9_LockRect(sys, &lr, NULL, D3DLOCK_READONLY);
    }
    if (SUCCEEDED(hr)) {
        for (UINT y = 0; y < desc.Height; y++) {
            memcpy(clock_->pixels + (size_t)y * desc.Width * 4,
                   (const BYTE *)lr.pBits + (size_t)y * lr.Pitch, (size_t)desc.Width * 4);
        }
        IDirect3DSurface9_UnlockRect(sys);
        store(&clock_->width, desc.Width);
        store(&clock_->height, desc.Height);
        store(&clock_->format, desc.Format);
    }
    store(&clock_->capture_error, SUCCEEDED(hr) ? 0 : (LONGLONG)(ULONG)hr);
    store(&clock_->captured, frame);
    if (sys) {
        IDirect3DSurface9_Release(sys);
    }
    if (resolved) {
        IDirect3DSurface9_Release(resolved);
    }
    if (back) {
        IDirect3DSurface9_Release(back);
    }
}

/* The game's keys are typed by the driver as a US keyboard's scan codes;
 * the game's process reads them with that layout. */
static void use_us_layout(void) {
    if (PRIMARYLANGID(LOWORD((DWORD_PTR)GetKeyboardLayout(0))) != LANG_ENGLISH) {
        LoadKeyboardLayoutA("00000409", KLF_ACTIVATE | KLF_SETFORPROCESS);
    }
}

typedef HRESULT(WINAPI *chain_present_fn)(IDirect3DSwapChain9 *, const RECT *, const RECT *, HWND,
                                          const RGNDATA *, DWORD);
static chain_present_fn real_chain_Present;
static IDirect3DDevice9 *the_device;
static DWORD presenting_thread; /* inside a hooked Present: an inner one counts not */
static volatile LONG n_tcl, n_begin, n_end, n_reset, n_present, n_chain;

/* A frame presented: the back buffer copied first when asked for, then
 * `present`, then the clock advanced and the frame held. */
static HRESULT frame_presented(IDirect3DDevice9 *dev, HRESULT (*present)(void *), void *args) {
    if (presenting_thread == GetCurrentThreadId()) {
        return present(args);
    }
    LONGLONG frame = load(&clock_->frames) + 1;
    if (!layout_set) {
        layout_set = TRUE;
        use_us_layout();
    }
    if (dev && load(&clock_->capture) == frame) {
        copy_back_buffer(dev, frame);
    }
    presenting_thread = GetCurrentThreadId();
    HRESULT hr = present(args);
    presenting_thread = 0;
    if (frame == 1) {
        /* How long the startup took in the clock depends on how often it
         * waited, which may depend on other threads; from the first frame
         * on, every run reads the same times. */
        LONGLONG before = load(&clock_->extra) + load(&clock_->sleep_ticks);
        store(&clock_->start_ticks, before);
        if (clock_->first_frame && (ULONGLONG)before < clock_->first_frame) {
            InterlockedAdd64((volatile LONGLONG *)&clock_->sleep_ticks,
                             (LONGLONG)clock_->first_frame - before);
        }
        dlog("the first frame: the clock was at %lld ms, moved to %llu ms", before / 10000,
             (unsigned long long)(clock_->first_frame / 10000));
    }
    /* The clock advances: reads from here on are of the next frame. */
    store(&clock_->frames, frame);
    InterlockedExchange(&reads_since_frame, 0);
    if (frame <= 3 || frame % 500 == 0) {
        dlog("frame %lld presented (0x%lx)", frame, (unsigned long)hr);
    }
    LONGLONG hold = load(&clock_->hold);
    if (hold && frame >= hold) {
        store(&clock_->held, frame);
        while ((hold = load(&clock_->hold)) && frame >= hold) {
            Sleep(1);
        }
        store(&clock_->held, 0);
    }
    return hr;
}

struct present_args {
    IDirect3DDevice9 *dev;
    IDirect3DSwapChain9 *chain;
    const RECT *src, *dst;
    HWND window;
    const RGNDATA *dirty;
    DWORD flags;
};

static HRESULT device_present(void *p) {
    struct present_args *a = p;
    return real_Present(a->dev, a->src, a->dst, a->window, a->dirty);
}

static HRESULT chain_present(void *p) {
    struct present_args *a = p;
    return real_chain_Present(a->chain, a->src, a->dst, a->window, a->dirty, a->flags);
}

static HRESULT WINAPI hook_Present(IDirect3DDevice9 *dev, const RECT *src, const RECT *dst,
                                   HWND window, const RGNDATA *dirty) {
    if (InterlockedIncrement(&n_present) == 1) {
        dlog("the device's Present: first call");
    }
    struct present_args a = {dev, NULL, src, dst, window, dirty, 0};
    return frame_presented(dev, device_present, &a);
}

static HRESULT WINAPI hook_chain_Present(IDirect3DSwapChain9 *chain, const RECT *src,
                                         const RECT *dst, HWND window, const RGNDATA *dirty,
                                         DWORD flags) {
    if (InterlockedIncrement(&n_chain) == 1) {
        dlog("the swap chain's Present: first call");
    }
    struct present_args a = {NULL, chain, src, dst, window, dirty, flags};
    return frame_presented(the_device, chain_present, &a);
}

typedef HRESULT(WINAPI *dev0_fn)(IDirect3DDevice9 *);
typedef HRESULT(WINAPI *reset_fn)(IDirect3DDevice9 *, D3DPRESENT_PARAMETERS *);
static dev0_fn real_TestCooperativeLevel, real_BeginScene, real_EndScene;
static reset_fn real_Reset;

static volatile LONG last_tcl = 1;

static HRESULT WINAPI hook_TestCooperativeLevel(IDirect3DDevice9 *dev) {
    HRESULT hr = real_TestCooperativeLevel(dev);
    if (InterlockedIncrement(&n_tcl) == 1 || hr != last_tcl) {
        dlog("TestCooperativeLevel: 0x%lx (call %ld)", (unsigned long)hr, n_tcl);
        last_tcl = hr;
    }
    return hr;
}

static HRESULT WINAPI hook_BeginScene(IDirect3DDevice9 *dev) {
    if (InterlockedIncrement(&n_begin) == 1) {
        dlog("BeginScene: first call");
    }
    return real_BeginScene(dev);
}

static HRESULT WINAPI hook_EndScene(IDirect3DDevice9 *dev) {
    if (InterlockedIncrement(&n_end) == 1) {
        dlog("EndScene: first call");
    }
    return real_EndScene(dev);
}

static HRESULT WINAPI hook_Reset(IDirect3DDevice9 *dev, D3DPRESENT_PARAMETERS *params) {
    HRESULT hr = real_Reset(dev, params);
    InterlockedIncrement(&n_reset);
    dlog("Reset: 0x%lx", (unsigned long)hr);
    return hr;
}

/* Hooks a function of Windows' own d3d9.dll at its code, so every caller
 * goes through `hook`, whichever interface table it reached it by: the
 * hot-patch form Microsoft's x86 system code is built for, a two-byte
 * `mov edi, edi` at the entry and five bytes of padding before it. The
 * padding becomes `jmp hook`, the entry `jmp short` to it; the original
 * goes on at entry + 2 (`*original`). A jump at the entry (a thunk) is
 * followed first. Returns 0, logging the bytes, when the code has not
 * that form. */
static int hotpatch(const char *name, void *target, void *hook, void **original) {
    BYTE *f = target;
    char where[MAX_PATH];
    for (int i = 0; i < 4; i++) {
        if (f[0] == 0xe9) {
            f = f + 5 + *(int32_t *)(f + 1);
        } else if (f[0] == 0xff && f[1] == 0x25) {
            f = **(BYTE ***)(f + 2);
        } else {
            break;
        }
    }
    module_of(f, where, MAX_PATH);
    if (f[0] == 0xeb && f[1] == 0xf9 && f[-5] == 0xe9 &&
        f - 5 + 5 + *(int32_t *)(f - 4) == (BYTE *)hook) {
        dlog("%s: %p in %s is hooked already", name, (void *)f, where);
        return 1;
    }
    int pad = 1;
    for (int i = 1; i <= 5; i++) {
        pad &= f[-i] == 0x90 || f[-i] == 0xcc;
    }
    if (!(f[0] == 0x8b && f[1] == 0xff) || !pad) {
        dlog("%s: %p in %s is not hot-patchable: %02x %02x %02x %02x %02x | %02x %02x %02x %02x",
             name, (void *)f, where, f[-5], f[-4], f[-3], f[-2], f[-1], f[0], f[1], f[2], f[3]);
        return 0;
    }
    DWORD old;
    if (!VirtualProtect(f - 5, 7, PAGE_EXECUTE_READWRITE, &old)) {
        dlog("%s: VirtualProtect error %lu", name, (unsigned long)GetLastError());
        return 0;
    }
    *original = f + 2;
    f[-5] = 0xe9;
    *(int32_t *)(f - 4) = (int32_t)((BYTE *)hook - f);
    /* The entry last, in one write, so a thread entering sees either form. */
    __atomic_store_n((volatile uint16_t *)f, (uint16_t)0xf9eb, __ATOMIC_SEQ_CST);
    VirtualProtect(f - 5, 7, old, &old);
    FlushInstructionCache(GetCurrentProcess(), f - 5, 7);
    dlog("%s: %p in %s hot-patched", name, (void *)f, where);
    return 1;
}

static HRESULT WINAPI hook_CreateDevice(IDirect3D9 *d3d, UINT adapter, D3DDEVTYPE type, HWND focus,
                                        DWORD flags, D3DPRESENT_PARAMETERS *params,
                                        IDirect3DDevice9 **device) {
    dlog("CreateDevice: adapter %u, type %d, flags 0x%lx, %ux%u, windowed %d", adapter, type,
         (unsigned long)flags, params ? params->BackBufferWidth : 0,
         params ? params->BackBufferHeight : 0, params ? params->Windowed : -1);
    HRESULT hr = real_CreateDevice(d3d, adapter, type, focus, flags, params, device);
    dlog("CreateDevice: 0x%lx", (unsigned long)hr);
    if (SUCCEEDED(hr) && device && *device) {
        the_device = *device;
        dlog("the device %p, its table %p", (void *)*device, (void *)(*device)->lpVtbl);
        /* Present, BeginScene, EndScene and the swap chain's Present at
         * their code in d3d9.dll, for every device (a device's table may
         * be its own, on the heap: measured on the PC, #762); the rest
         * only logged, in this device's table. */
        static int coded;
        IDirect3DSwapChain9 *chain = NULL;
        void *chain_present = NULL;
        if (SUCCEEDED(IDirect3DDevice9_GetSwapChain(*device, 0, &chain)) && chain) {
            chain_present = (void *)chain->lpVtbl->Present;
            IDirect3DSwapChain9_Release(chain);
        }
        if (!coded) {
            coded = 1;
            if (!hotpatch("Present", (void *)(*device)->lpVtbl->Present, (void *)hook_Present,
                          (void **)&real_Present)) {
                patch_slot((void **)&(*device)->lpVtbl->Present, (void *)hook_Present,
                           (void **)&real_Present);
            }
            if (!hotpatch("BeginScene", (void *)(*device)->lpVtbl->BeginScene,
                          (void *)hook_BeginScene, (void **)&real_BeginScene)) {
                patch_slot((void **)&(*device)->lpVtbl->BeginScene, (void *)hook_BeginScene,
                           (void **)&real_BeginScene);
            }
            if (!hotpatch("EndScene", (void *)(*device)->lpVtbl->EndScene, (void *)hook_EndScene,
                          (void **)&real_EndScene)) {
                patch_slot((void **)&(*device)->lpVtbl->EndScene, (void *)hook_EndScene,
                           (void **)&real_EndScene);
            }
            if (chain_present && !hotpatch("the swap chain's Present", chain_present,
                                           (void *)hook_chain_Present,
                                           (void **)&real_chain_Present)) {
                dlog("the swap chain's Present is not hooked");
            }
        }
        patch_slot((void **)&(*device)->lpVtbl->TestCooperativeLevel,
                   (void *)hook_TestCooperativeLevel, (void **)&real_TestCooperativeLevel);
        patch_slot((void **)&(*device)->lpVtbl->Reset, (void *)hook_Reset, (void **)&real_Reset);
        store(&clock_->hooked, 1);
        dlog("Present hooked (the device's and its swap chain's)");
    }
    return hr;
}

static IDirect3D9 *WINAPI hook_Direct3DCreate9(UINT version) {
    IDirect3D9 *d3d = real_Direct3DCreate9(version);
    dlog("Direct3DCreate9(%u): %p", version, (void *)d3d);
    if (d3d) {
        patch_slot((void **)&d3d->lpVtbl->CreateDevice, (void *)hook_CreateDevice,
                   (void **)&real_CreateDevice);
    }
    return d3d;
}

/* --- Import tables ------------------------------------------------------ */

static CRITICAL_SECTION patching;
static WCHAR windows_dir[MAX_PATH], sxs_dir[MAX_PATH];

static HMODULE(WINAPI *real_LoadLibraryA)(LPCSTR);
static HMODULE(WINAPI *real_LoadLibraryW)(LPCWSTR);
static HMODULE(WINAPI *real_LoadLibraryExA)(LPCSTR, HANDLE, DWORD);
static HMODULE(WINAPI *real_LoadLibraryExW)(LPCWSTR, HANDLE, DWORD);
static FARPROC(WINAPI *real_GetProcAddress)(HMODULE, LPCSTR);

static void patch_all(void);

static HMODULE WINAPI hook_LoadLibraryA(LPCSTR name) {
    dlog("LoadLibraryA %s", name ? name : "(null)");
    HMODULE m = real_LoadLibraryA(name);
    patch_all();
    return m;
}

static HMODULE WINAPI hook_LoadLibraryW(LPCWSTR name) {
    dlog("LoadLibraryW %ls", name ? name : L"(null)");
    HMODULE m = real_LoadLibraryW(name);
    patch_all();
    return m;
}

static HMODULE WINAPI hook_LoadLibraryExA(LPCSTR name, HANDLE file, DWORD flags) {
    dlog("LoadLibraryExA %s 0x%lx", name ? name : "(null)", (unsigned long)flags);
    HMODULE m = real_LoadLibraryExA(name, file, flags);
    patch_all();
    return m;
}

static HMODULE WINAPI hook_LoadLibraryExW(LPCWSTR name, HANDLE file, DWORD flags) {
    dlog("LoadLibraryExW %ls 0x%lx", name ? name : L"(null)", (unsigned long)flags);
    HMODULE m = real_LoadLibraryExW(name, file, flags);
    patch_all();
    return m;
}

static void(WINAPI *real_Sleep)(DWORD);
static DWORD(WINAPI *real_SleepEx)(DWORD, BOOL);

/* A main-thread sleep of `ms`: counted, and with sleep_advances it adds its
 * time to the clock, so a wait that sleeps until time has passed ends (as
 * a busy wait does through spin). */
static void slept(DWORD ms) {
    if (GetCurrentThreadId() != main_thread) {
        return;
    }
    LONGLONG n = InterlockedIncrement64((volatile LONGLONG *)&clock_->sleeps);
    if (n <= 5 || n % 1000 == 0) {
        dlog("main-thread Sleep(%lu), number %lld, frame %lld", (unsigned long)ms, n,
             load(&clock_->frames));
    }
    if (clock_->sleep_advances && ms != INFINITE) {
        /* Before the first frame a sleep advances at least the quantum: the
         * startup waits with Sleep(0) for time to pass (measured on the PC,
         * #762); the clock is set to a fixed time at the first frame. */
        ULONGLONG add = ms;
        if (load(&clock_->frames) == 0 && add < clock_->start_quantum) {
            add = clock_->start_quantum;
        }
        InterlockedAdd64((volatile LONGLONG *)&clock_->sleep_ticks,
                         (LONGLONG)add * (LONGLONG)FRAMECLOCK_TICKS_PER_MS);
    }
}

static void WINAPI fake_Sleep(DWORD ms) {
    slept(ms);
    real_Sleep(ms);
}

static DWORD WINAPI fake_SleepEx(DWORD ms, BOOL alertable) {
    slept(ms);
    return real_SleepEx(ms, alertable);
}

static FARPROC WINAPI hook_GetProcAddress(HMODULE module, LPCSTR name);

/* The kernel32 functions replaced: name, the system's, the replacement. */
static struct hook {
    const char *name;
    void *real;
    void *fake;
} hooks[] = {
    {"QueryPerformanceCounter", 0, (void *)fake_QueryPerformanceCounter},
    {"QueryPerformanceFrequency", 0, (void *)fake_QueryPerformanceFrequency},
    {"GetTickCount", 0, (void *)fake_GetTickCount},
    {"GetTickCount64", 0, (void *)fake_GetTickCount64},
    {"GetSystemTimeAsFileTime", 0, (void *)fake_GetSystemTimeAsFileTime},
    {"GetSystemTimePreciseAsFileTime", 0, (void *)fake_GetSystemTimeAsFileTime},
    {"GetSystemTime", 0, (void *)fake_GetSystemTime},
    {"GetLocalTime", 0, (void *)fake_GetLocalTime},
    {"LoadLibraryA", 0, (void *)hook_LoadLibraryA},
    {"LoadLibraryW", 0, (void *)hook_LoadLibraryW},
    {"LoadLibraryExA", 0, (void *)hook_LoadLibraryExA},
    {"LoadLibraryExW", 0, (void *)hook_LoadLibraryExW},
    {"GetProcAddress", 0, (void *)hook_GetProcAddress},
    {"Sleep", 0, (void *)fake_Sleep},
    {"SleepEx", 0, (void *)fake_SleepEx},
};
#define HOOKS (sizeof(hooks) / sizeof(hooks[0]))

static struct hook *hook_named(const char *name) {
    for (size_t i = 0; i < HOOKS; i++) {
        if (strcmp(hooks[i].name, name) == 0) {
            return &hooks[i];
        }
    }
    return NULL;
}

static struct hook *hook_at(void *address) {
    for (size_t i = 0; i < HOOKS; i++) {
        if (hooks[i].real && hooks[i].real == address) {
            return &hooks[i];
        }
    }
    return NULL;
}

static FARPROC WINAPI hook_GetProcAddress(HMODULE module, LPCSTR name) {
    FARPROC p = real_GetProcAddress(module, name);
    if (!p || IS_INTRESOURCE(name)) {
        return p;
    }
    if (strcmp(name, "Direct3DCreate9") == 0) {
        dlog("GetProcAddress Direct3DCreate9: hooked");
        real_Direct3DCreate9 = (create9_fn)p;
        return (FARPROC)(void *)hook_Direct3DCreate9;
    }
    if (strcmp(name, "Direct3DCreate9Ex") == 0) {
        dlog("GetProcAddress Direct3DCreate9Ex: not hooked (its devices still go through the "
             "hot-patched code)");
    }
    if (!strncmp(name, "Direct3D", 8) && strcmp(name, "Direct3DShaderValidatorCreate9")) {
        dlog("GetProcAddress %s: not hooked", name);
    }
    struct hook *h = hook_named(name);
    if (h && h->real == (void *)p) {
        dlog("GetProcAddress %s: the frame clock's", name);
        return (FARPROC)h->fake;
    }
    return p;
}

static BOOL starts_with_dir(const WCHAR *path, const WCHAR *dir) {
    size_t n = wcslen(dir);
    return n && _wcsnicmp(path, dir, n) == 0 && (path[n] == L'\\' || path[n] == 0);
}

/* Windows' own modules keep their imports; the rest (the game's folder,
 * the side-by-side C runtime) are patched. */
static BOOL patched_module(HMODULE m) {
    WCHAR path[MAX_PATH];
    if (m == self || !GetModuleFileNameW(m, path, MAX_PATH)) {
        return FALSE;
    }
    return !starts_with_dir(path, windows_dir) || starts_with_dir(path, sxs_dir);
}

static void patch_imports(HMODULE m) {
    BYTE *base = (BYTE *)m;
    IMAGE_DOS_HEADER *dos = (IMAGE_DOS_HEADER *)base;
    if (dos->e_magic != IMAGE_DOS_SIGNATURE) {
        return;
    }
    IMAGE_NT_HEADERS *nt = (IMAGE_NT_HEADERS *)(base + dos->e_lfanew);
    IMAGE_DATA_DIRECTORY dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (!dir.VirtualAddress || !dir.Size) {
        return;
    }
    int patched = 0;
    for (IMAGE_IMPORT_DESCRIPTOR *d = (IMAGE_IMPORT_DESCRIPTOR *)(base + dir.VirtualAddress); d->Name;
         d++) {
        if (_stricmp((const char *)(base + d->Name), "kernel32.dll") != 0) {
            continue;
        }
        IMAGE_THUNK_DATA *iat = (IMAGE_THUNK_DATA *)(base + d->FirstThunk);
        IMAGE_THUNK_DATA *names =
            d->OriginalFirstThunk ? (IMAGE_THUNK_DATA *)(base + d->OriginalFirstThunk) : NULL;
        for (size_t i = 0; iat[i].u1.Function; i++) {
            struct hook *h = NULL;
            if (names && !(names[i].u1.Ordinal & IMAGE_ORDINAL_FLAG)) {
                IMAGE_IMPORT_BY_NAME *by = (IMAGE_IMPORT_BY_NAME *)(base + names[i].u1.AddressOfData);
                h = hook_named((const char *)by->Name);
            } else {
                h = hook_at((void *)iat[i].u1.Function);
            }
            if (!h || (void *)iat[i].u1.Function == h->fake) {
                continue;
            }
            DWORD old;
            if (VirtualProtect(&iat[i], sizeof(iat[i]), PAGE_READWRITE, &old)) {
                iat[i].u1.Function = (DWORD_PTR)h->fake;
                VirtualProtect(&iat[i], sizeof(iat[i]), old, &old);
                patched++;
            } else {
                dlog("cannot patch %s in a module", h->name);
            }
        }
    }
    if (patched) {
        char path[MAX_PATH];
        GetModuleFileNameA(m, path, MAX_PATH);
        dlog("patched %d imports of %s", patched, path);
    }
}

static void patch_all(void) {
    HMODULE modules[1024];
    DWORD bytes;
    EnterCriticalSection(&patching);
    if (EnumProcessModules(GetCurrentProcess(), modules, sizeof(modules), &bytes)) {
        DWORD n = bytes / sizeof(HMODULE);
        for (DWORD i = 0; i < n && i < 1024; i++) {
            if (patched_module(modules[i])) {
                patch_imports(modules[i]);
            }
        }
    }
    LeaveCriticalSection(&patching);
}

/* --- Start -------------------------------------------------------------- */

/* Every 2 real seconds, the clock's figures when they changed (and every
 * 20 s regardless), so the log tells where a run stands. */
static DWORD WINAPI heartbeat(void *unused) {
    (void)unused;
    LONGLONG last[6] = {-1, -1, -1, -1, -1, -1};
    for (int beat = 0;; beat++) {
        Sleep(2000);
        LONGLONG now[6] = {load(&clock_->frames), load(&clock_->held), load(&clock_->reads),
                           load(&clock_->other_reads), load(&clock_->extra),
                           load(&clock_->sleeps)};
        if (memcmp(now, last, sizeof now) || beat % 10 == 0) {
            dlog("frames %lld, held %lld, hold %lld, hooked %lld, main reads %lld, other reads "
                 "%lld, extra %lld ms, sleeps %lld (%lld ms)",
                 now[0], now[1], load(&clock_->hold), load(&clock_->hooked), now[2], now[3],
                 now[4] / (LONGLONG)FRAMECLOCK_TICKS_PER_MS, now[5],
                 load(&clock_->sleep_ticks) / (LONGLONG)FRAMECLOCK_TICKS_PER_MS);
            memcpy(last, now, sizeof now);
            DWORD front = 0;
            GetWindowThreadProcessId(GetForegroundWindow(), &front);
            dlog("calls: Present %ld, swap chain Present %ld, BeginScene %ld, EndScene %ld, "
                 "TestCooperativeLevel %ld (last 0x%lx), Reset %ld; the foreground window is %s",
                 n_present, n_chain, n_begin, n_end, n_tcl, (unsigned long)last_tcl, n_reset,
                 front == GetCurrentProcessId() ? "the game's" : "another process's");
        }
        if (beat % 5 == 0) {
            LONG n = ncallers < 96 ? ncallers : 96;
            for (LONG i = 0; i < n; i++) {
                char path[MAX_PATH] = "?";
                if (callers[i].fn) {
                    GetModuleFileNameA(callers[i].module, path, MAX_PATH);
                    dlog("  %s from %s: %ld", callers[i].fn, path, callers[i].count);
                }
            }
        }
    }
    return 0;
}

static void start_clock(void) {
    HANDLE map = OpenFileMappingA(FILE_MAP_ALL_ACCESS, FALSE, FRAMECLOCK_NAME);
    if (!map) {
        return;
    }
    struct frameclock *c = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(struct frameclock));
    if (!c || c->magic != FRAMECLOCK_MAGIC || c->version != FRAMECLOCK_VERSION) {
        return;
    }
    open_log(c->log_path);
    dlog("the frame clock starts: step %llu ticks, spin %llu, sleep_advances %llu",
         (unsigned long long)c->step, (unsigned long long)c->spin,
         (unsigned long long)c->sleep_advances);
    HMODULE kernel32 = GetModuleHandleA("kernel32.dll");
    for (size_t i = 0; i < HOOKS; i++) {
        hooks[i].real = (void *)GetProcAddress(kernel32, hooks[i].name);
    }
    real_Sleep = (void *)GetProcAddress(kernel32, "Sleep");
    real_SleepEx = (void *)GetProcAddress(kernel32, "SleepEx");
    real_LoadLibraryA = (void *)GetProcAddress(kernel32, "LoadLibraryA");
    real_LoadLibraryW = (void *)GetProcAddress(kernel32, "LoadLibraryW");
    real_LoadLibraryExA = (void *)GetProcAddress(kernel32, "LoadLibraryExA");
    real_LoadLibraryExW = (void *)GetProcAddress(kernel32, "LoadLibraryExW");
    real_GetProcAddress = (void *)GetProcAddress(kernel32, "GetProcAddress");
    UINT n = GetWindowsDirectoryW(windows_dir, MAX_PATH);
    if (n && n + 8 < MAX_PATH) {
        memcpy(sxs_dir, windows_dir, n * sizeof(WCHAR));
        memcpy(sxs_dir + n, L"\\WinSxS", 8 * sizeof(WCHAR));
    }
    InitializeCriticalSection(&patching);
    /* DllMain of a library the program imports runs on its main thread. */
    main_thread = GetCurrentThreadId();
    clock_ = c;
    patch_all();
    store(&c->attached, 1);
    dlog("attached; main thread %lu", main_thread);
    /* Starts once the loader lets go: it only writes the log. */
    CreateThread(NULL, 0, heartbeat, NULL, 0, NULL);
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved) {
    (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) {
        self = instance;
        DisableThreadLibraryCalls(instance);
        forward();
        start_clock();
    }
    return TRUE;
}
