/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The game's audio output, recorded from outside it (loopback.h).
 *
 * WASAPI's loopback mode captures "the audio stream that is being played
 * by a rendering endpoint device" (documented: Microsoft Learn, Loopback
 * Recording), in shared mode only; its packets carry the performance
 * counter at their first frame in 100 ns units (documented:
 * IAudioCaptureClient::GetBuffer). The documentation says nothing of
 * whether the endpoint's or a session's volume or mute applies before the
 * loopback tap, so nothing is muted: the PC's outputs are silenced
 * physically (the user's setup), and the recording is of the default
 * output device as it is, which the game plays to (believed: it is given
 * no device), or of the one named. Nothing changes any volume or device
 * setting; the endpoint's own volume and mute are only read, for the log.
 * No device's name is logged (a detail of the machine). */

#define WIN32_LEAN_AND_MEAN
#define COBJMACROS
#define _WIN32_WINNT 0x0a00
#include <initguid.h>
#include <windows.h>
#include <audioclient.h>
#include <endpointvolume.h>
#include <functiondiscoverykeys_devpkey.h>
#include <mmdeviceapi.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "loopback.h"

/* The recording grows in steps of this many frames. */
#define GROW (1u << 20)
/* A packet later than the samples so far by more than this many ms
 * leaves a gap, filled with silence. */
#define GAP_MS 2

struct loopback {
    IMMDevice *device;
    IAudioClient *client;
    IAudioCaptureClient *capture;
    WAVEFORMATEX *format;
    int is_float;
    uint32_t bits, channels, rate, block;
    loopback_say say;

    HANDLE thread;
    volatile LONG stop;
    int64_t origin; /* loopback_now at the recording's sample 0 */
    int16_t *samples;
    size_t frames, cap; /* frames of `channels` samples */
    uint64_t packets, gaps, silent, discontinuities, time_errors, clipped;
    HRESULT error;
};

int64_t loopback_now(void) {
    static LARGE_INTEGER freq;
    LARGE_INTEGER now;
    if (!freq.QuadPart) {
        QueryPerformanceFrequency(&freq);
    }
    QueryPerformanceCounter(&now);
    /* Seconds and remainder apart, so the product does not overflow. */
    int64_t s = now.QuadPart / freq.QuadPart, r = now.QuadPart % freq.QuadPart;
    return s * 10000000 + r * 10000000 / freq.QuadPart;
}

static IMMDeviceEnumerator *enumerator(void) {
    IMMDeviceEnumerator *e = NULL;
    CoInitializeEx(NULL, COINIT_MULTITHREADED);
    if (FAILED(CoCreateInstance(&CLSID_MMDeviceEnumerator, NULL, CLSCTX_ALL,
                                &IID_IMMDeviceEnumerator, (void **)&e))) {
        return NULL;
    }
    return e;
}

/* The endpoint's form factor (EndpointFormFactor). */
static unsigned describe(IMMDevice *d) {
    IPropertyStore *props = NULL;
    unsigned form = ~0u;
    if (FAILED(IMMDevice_OpenPropertyStore(d, STGM_READ, &props))) {
        return form;
    }
    PROPVARIANT v;
    PropVariantInit(&v);
    if (SUCCEEDED(IPropertyStore_GetValue(props, &PKEY_AudioEndpoint_FormFactor, &v)) &&
        v.vt == VT_UI4) {
        form = v.ulVal;
    }
    PropVariantClear(&v);
    IPropertyStore_Release(props);
    return form;
}

static const char *form_name(unsigned f) {
    static const char *names[] = {"remote network device", "speakers", "line level", "headphones",
                                  "microphone", "headset", "handset", "unknown digital passthrough",
                                  "S/PDIF", "digital audio display device", "unknown"};
    return f < sizeof names / sizeof *names ? names[f] : "not given";
}

/* The active render endpoint whose name contains `name`, if exactly one
 * does. */
static IMMDevice *named(IMMDeviceEnumerator *e, const char *name, loopback_say say) {
    IMMDeviceCollection *all = NULL;
    UINT count = 0;
    if (FAILED(IMMDeviceEnumerator_EnumAudioEndpoints(e, eRender, DEVICE_STATE_ACTIVE, &all)) ||
        FAILED(IMMDeviceCollection_GetCount(all, &count))) {
        say("cannot list the audio output devices");
        return NULL;
    }
    WCHAR want[256];
    MultiByteToWideChar(CP_ACP, 0, name, -1, want, 256);
    CharLowerW(want);
    IMMDevice *found = NULL;
    int matches = 0;
    for (UINT i = 0; i < count; i++) {
        IMMDevice *d = NULL;
        IPropertyStore *props = NULL;
        if (FAILED(IMMDeviceCollection_Item(all, i, &d))) {
            continue;
        }
        PROPVARIANT v;
        PropVariantInit(&v);
        int match = 0;
        if (SUCCEEDED(IMMDevice_OpenPropertyStore(d, STGM_READ, &props)) &&
            SUCCEEDED(IPropertyStore_GetValue(props, &PKEY_Device_FriendlyName, &v)) &&
            v.vt == VT_LPWSTR) {
            WCHAR lower[512];
            wcsncpy(lower, v.pwszVal, 511);
            lower[511] = 0;
            CharLowerW(lower);
            match = wcsstr(lower, want) != NULL;
        }
        PropVariantClear(&v);
        if (props) {
            IPropertyStore_Release(props);
        }
        if (match && !found) {
            found = d;
        } else {
            IMMDevice_Release(d);
        }
        matches += match;
    }
    IMMDeviceCollection_Release(all);
    if (matches != 1) {
        say("%d of the %u audio output devices have the name given in theirs; one must", matches,
            count);
        if (found) {
            IMMDevice_Release(found);
        }
        return NULL;
    }
    return found;
}

struct loopback *loopback_open(const char *name, loopback_say say) {
    IMMDeviceEnumerator *e = enumerator();
    if (!e) {
        say("cannot reach the audio devices");
        return NULL;
    }
    IMMDevice *found = NULL;
    if (name && name[0]) {
        found = named(e, name, say);
    } else if (FAILED(IMMDeviceEnumerator_GetDefaultAudioEndpoint(e, eRender, eConsole, &found))) {
        found = NULL;
        say("no default audio output device");
    }
    IMMDeviceEnumerator_Release(e);
    if (!found) {
        return NULL;
    }
    struct loopback *lb = calloc(1, sizeof *lb);
    lb->device = found;
    lb->say = say;
    unsigned form = describe(found);
    say("The game's audio output: %s (%s)", name && name[0] ? "the device named" : "the default device",
        form_name(form));

    IAudioEndpointVolume *vol = NULL;
    if (SUCCEEDED(IMMDevice_Activate(found, &IID_IAudioEndpointVolume, CLSCTX_ALL, NULL,
                                     (void **)&vol))) {
        float level = 0;
        BOOL mute = FALSE;
        IAudioEndpointVolume_GetMasterVolumeLevelScalar(vol, &level);
        IAudioEndpointVolume_GetMute(vol, &mute);
        say("Its volume (read, not changed): %.3f%s", level, mute ? ", muted" : "");
        IAudioEndpointVolume_Release(vol);
    }

    HRESULT hr = IMMDevice_Activate(found, &IID_IAudioClient, CLSCTX_ALL, NULL, (void **)&lb->client);
    if (SUCCEEDED(hr)) {
        hr = IAudioClient_GetMixFormat(lb->client, &lb->format);
    }
    if (SUCCEEDED(hr)) {
        hr = IAudioClient_Initialize(lb->client, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
                                     10000000, 0, lb->format, NULL);
    }
    if (SUCCEEDED(hr)) {
        hr = IAudioClient_GetService(lb->client, &IID_IAudioCaptureClient, (void **)&lb->capture);
    }
    if (FAILED(hr)) {
        say("cannot open the device's loopback stream (0x%lx)", (unsigned long)hr);
        loopback_close(lb);
        return NULL;
    }
    WAVEFORMATEX *f = lb->format;
    unsigned tag = f->wFormatTag;
    if (tag == WAVE_FORMAT_EXTENSIBLE && f->cbSize >= 22) {
        tag = (unsigned)((WAVEFORMATEXTENSIBLE *)f)->SubFormat.Data1;
    }
    lb->bits = f->wBitsPerSample;
    lb->channels = f->nChannels;
    lb->rate = f->nSamplesPerSec;
    lb->block = f->nBlockAlign;
    lb->is_float = tag == WAVE_FORMAT_IEEE_FLOAT;
    if (!(lb->is_float && lb->bits == 32) && !(tag == WAVE_FORMAT_PCM && (lb->bits == 16 ||
                                                                         lb->bits == 32))) {
        say("the mix format is not 32-bit float or 16- or 32-bit PCM (tag %u, %u bits)", tag,
            lb->bits);
        loopback_close(lb);
        return NULL;
    }
    say("Its mix format: %u Hz, %u channels, %u-bit %s", lb->rate, lb->channels, lb->bits,
        lb->is_float ? "float" : "PCM");
    return lb;
}

uint32_t loopback_rate(const struct loopback *lb) {
    return lb->rate;
}

uint32_t loopback_channels(const struct loopback *lb) {
    return lb->channels;
}

int64_t loopback_sample_at(const struct loopback *lb, int64_t t) {
    int64_t d = t - lb->origin;
    return d / 10000000 * lb->rate + d % 10000000 * lb->rate / 10000000;
}

static void reserve(struct loopback *lb, size_t frames) {
    if (frames > lb->cap) {
        size_t cap = lb->cap;
        while (cap < frames) {
            cap += GROW;
        }
        lb->samples = realloc(lb->samples, cap * lb->channels * sizeof(int16_t));
        lb->cap = cap;
    }
}

static int16_t to16(const struct loopback *lb, const BYTE *p, uint64_t *clipped) {
    if (lb->is_float) {
        float v;
        memcpy(&v, p, 4);
        if (v > 1.0f || v < -1.0f) {
            ++*clipped;
            v = v > 0 ? 1.0f : -1.0f;
        }
        return (int16_t)(v * 32767.0f);
    }
    if (lb->bits == 16) {
        int16_t v;
        memcpy(&v, p, 2);
        return v;
    }
    int32_t v;
    memcpy(&v, p, 4);
    return (int16_t)(v >> 16);
}

/* One packet: placed by its time, after silence where it comes late. */
static void place(struct loopback *lb, const BYTE *data, UINT32 n, DWORD flags, UINT64 qpc) {
    lb->packets++;
    lb->silent += (flags & AUDCLNT_BUFFERFLAGS_SILENT) != 0;
    lb->discontinuities += (flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY) != 0;
    if (flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR) {
        lb->time_errors++;
    } else {
        int64_t at = loopback_sample_at(lb, (int64_t)qpc);
        if (at > (int64_t)lb->frames + (int64_t)lb->rate * GAP_MS / 1000) {
            reserve(lb, (size_t)at);
            memset(lb->samples + lb->frames * lb->channels, 0,
                   ((size_t)at - lb->frames) * lb->channels * sizeof(int16_t));
            lb->frames = (size_t)at;
            lb->gaps++;
        }
    }
    reserve(lb, lb->frames + n);
    int16_t *out = lb->samples + lb->frames * lb->channels;
    if (flags & AUDCLNT_BUFFERFLAGS_SILENT) {
        memset(out, 0, (size_t)n * lb->channels * sizeof(int16_t));
    } else {
        uint32_t bytes = lb->bits / 8;
        for (UINT32 i = 0; i < n; i++) {
            for (uint32_t c = 0; c < lb->channels; c++) {
                out[i * lb->channels + c] = to16(lb, data + i * lb->block + c * bytes, &lb->clipped);
            }
        }
    }
    lb->frames += n;
}

static DWORD WINAPI record(LPVOID arg) {
    struct loopback *lb = arg;
    CoInitializeEx(NULL, COINIT_MULTITHREADED);
    while (!InterlockedCompareExchange(&lb->stop, 0, 0)) {
        Sleep(4);
        UINT32 next = 0;
        while (SUCCEEDED(IAudioCaptureClient_GetNextPacketSize(lb->capture, &next)) && next) {
            BYTE *data;
            UINT32 n;
            DWORD flags;
            UINT64 pos, qpc;
            HRESULT hr = IAudioCaptureClient_GetBuffer(lb->capture, &data, &n, &flags, &pos, &qpc);
            if (FAILED(hr)) {
                lb->error = hr;
                break;
            }
            place(lb, data, n, flags, qpc);
            IAudioCaptureClient_ReleaseBuffer(lb->capture, n);
        }
    }
    CoUninitialize();
    return 0;
}

int loopback_start(struct loopback *lb) {
    lb->origin = loopback_now();
    lb->stop = 0;
    HRESULT hr = IAudioClient_Start(lb->client);
    if (FAILED(hr)) {
        lb->say("cannot start the loopback stream (0x%lx)", (unsigned long)hr);
        return 0;
    }
    lb->thread = CreateThread(NULL, 0, record, lb, 0, NULL);
    return lb->thread != NULL;
}

void loopback_stop(struct loopback *lb) {
    if (!lb->thread) {
        return;
    }
    InterlockedExchange(&lb->stop, 1);
    WaitForSingleObject(lb->thread, INFINITE);
    CloseHandle(lb->thread);
    lb->thread = NULL;
    IAudioClient_Stop(lb->client);
}

static void le16(FILE *f, uint32_t v) {
    fputc((int)(v & 0xff), f);
    fputc((int)(v >> 8 & 0xff), f);
}

static void le32(FILE *f, uint32_t v) {
    le16(f, v & 0xffff);
    le16(f, v >> 16);
}

int loopback_write(const struct loopback *lb, const char *path) {
    FILE *f = fopen(path, "wb");
    if (!f) {
        return 0;
    }
    uint32_t data = (uint32_t)(lb->frames * lb->channels * 2);
    fwrite("RIFF", 1, 4, f);
    le32(f, 36 + data);
    fwrite("WAVEfmt ", 1, 8, f);
    le32(f, 16);
    le16(f, 1);
    le16(f, lb->channels);
    le32(f, lb->rate);
    le32(f, lb->rate * lb->channels * 2);
    le16(f, lb->channels * 2);
    le16(f, 16);
    fwrite("data", 1, 4, f);
    le32(f, data);
    size_t w = lb->frames ? fwrite(lb->samples, 2, lb->frames * lb->channels, f) : 0;
    return fclose(f) == 0 && w == lb->frames * lb->channels;
}

void loopback_report(const struct loopback *lb, loopback_say say) {
    say("The recording: %.1f s; %llu packets, %llu gaps filled with silence, %llu marked silent, "
        "%llu with a discontinuity, %llu with an unreliable time; %llu samples clipped%s",
        (double)lb->frames / lb->rate, (unsigned long long)lb->packets,
        (unsigned long long)lb->gaps, (unsigned long long)lb->silent,
        (unsigned long long)lb->discontinuities, (unsigned long long)lb->time_errors,
        (unsigned long long)lb->clipped, lb->error ? "; a read failed" : "");
}

void loopback_close(struct loopback *lb) {
    if (!lb) {
        return;
    }
    loopback_stop(lb);
    if (lb->capture) {
        IAudioCaptureClient_Release(lb->capture);
    }
    if (lb->client) {
        IAudioClient_Release(lb->client);
    }
    if (lb->format) {
        CoTaskMemFree(lb->format);
    }
    if (lb->device) {
        IMMDevice_Release(lb->device);
    }
    free(lb->samples);
    free(lb);
}
