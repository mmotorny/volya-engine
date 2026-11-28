/* SPDX-FileCopyrightText: 2026 Maksym Motorny
 * SPDX-License-Identifier: BSD-3-Clause */

/* The game's audio device on the frame clock: an OpenAL32.dll for the
 * copy's bin\ in an audio run (docs/testing.md, The original on Windows),
 * the OpenAL 1.1 implementation MojoAL (zlib; mojoal.c, unchanged, compiled
 * into this file) on SDL 3 (zlib; SDL3.dll beside it), with no output
 * device. MojoAL mixes each context into an SDL audio stream from the
 * device's callback; here the stream is bound to no device and nothing
 * calls it on its own: the frame clock (frameclock.c) calls `volya_render`
 * once a frame presented, which runs the callback for exactly the samples
 * asked for and returns them, 16-bit stereo at the output rate. So the
 * device consumes the game's sound as a real device would, in time with
 * the frame clock instead of the wall clock, silently.
 *
 * The device is listed as "Generic Software" (beside MojoAL's default
 * device; either name opens it), the name of the device the game chose of
 * the shipped implementation (console-diagnostics.md 2.1.1); like it, it
 * offers no EAX 2.0 (sound-reverb.md 2.2). */

#include <SDL3/SDL.h>
#include <stdint.h>

#define VOLYA_DEVICE_ID ((SDL_AudioDeviceID)2)
#define VOLYA_DEVICE_NAME "Generic Software"
#define VOLYA_RATE 44100
#define VOLYA_CHANNELS 2

/* The contexts' streams: each with MojoAL's callback for it. */
#define VOLYA_MAX_STREAMS 8
static struct {
    SDL_AudioStream *stream;
    SDL_AudioStreamCallback callback;
    void *userdata;
} volya_streams[VOLYA_MAX_STREAMS];

/* SDL's audio with the driver that opens nothing, whatever the
 * environment says. */
static bool volya_SDL_Init(SDL_InitFlags flags) {
    SDL_SetHintWithPriority(SDL_HINT_AUDIO_DRIVER, "dummy", SDL_HINT_OVERRIDE);
    return SDL_Init(flags);
}

static bool volya_SDL_InitSubSystem(SDL_InitFlags flags) {
    SDL_SetHintWithPriority(SDL_HINT_AUDIO_DRIVER, "dummy", SDL_HINT_OVERRIDE);
    return SDL_InitSubSystem(flags);
}

/* One playback device, no recording device. */
static SDL_AudioDeviceID *volya_SDL_GetAudioPlaybackDevices(int *count) {
    SDL_AudioDeviceID *ids = SDL_calloc(2, sizeof *ids);
    if (ids) {
        ids[0] = VOLYA_DEVICE_ID;
    }
    if (count) {
        *count = ids ? 1 : 0;
    }
    return ids;
}

static SDL_AudioDeviceID *volya_SDL_GetAudioRecordingDevices(int *count) {
    if (count) {
        *count = 0;
    }
    return SDL_calloc(1, sizeof(SDL_AudioDeviceID));
}

static const char *volya_SDL_GetAudioDeviceName(SDL_AudioDeviceID id) {
    return id == VOLYA_DEVICE_ID ? VOLYA_DEVICE_NAME : NULL;
}

/* The device's format: stereo at the output rate (sound-positional.md 2). */
static bool volya_SDL_GetAudioDeviceFormat(SDL_AudioDeviceID id, SDL_AudioSpec *spec,
                                           int *frames) {
    (void)id;
    spec->format = SDL_AUDIO_F32;
    spec->channels = VOLYA_CHANNELS;
    spec->freq = VOLYA_RATE;
    if (frames) {
        *frames = 0;
    }
    return true;
}

/* A context's stream, bound to no device: its callback is kept for
 * volya_render. */
static SDL_AudioStream *volya_SDL_OpenAudioDeviceStream(SDL_AudioDeviceID id,
                                                        const SDL_AudioSpec *spec,
                                                        SDL_AudioStreamCallback callback,
                                                        void *userdata) {
    (void)id;
    for (int i = 0; i < VOLYA_MAX_STREAMS; i++) {
        if (!volya_streams[i].stream) {
            SDL_AudioStream *s = SDL_CreateAudioStream(spec, spec);
            if (s) {
                volya_streams[i].callback = callback;
                volya_streams[i].userdata = userdata;
                volya_streams[i].stream = s;
            }
            return s;
        }
    }
    return NULL;
}

static void volya_SDL_DestroyAudioStream(SDL_AudioStream *stream) {
    for (int i = 0; stream && i < VOLYA_MAX_STREAMS; i++) {
        if (volya_streams[i].stream == stream) {
            volya_streams[i].stream = NULL;
        }
    }
    SDL_DestroyAudioStream(stream);
}

static SDL_AudioDeviceID volya_SDL_GetAudioStreamDevice(SDL_AudioStream *stream) {
    (void)stream;
    return VOLYA_DEVICE_ID;
}

static bool volya_SDL_ResumeAudioStreamDevice(SDL_AudioStream *stream) {
    (void)stream;
    return true;
}

static bool volya_SDL_PauseAudioStreamDevice(SDL_AudioStream *stream) {
    (void)stream;
    return true;
}

#define SDL_Init volya_SDL_Init
#define SDL_InitSubSystem volya_SDL_InitSubSystem
#define SDL_GetAudioPlaybackDevices volya_SDL_GetAudioPlaybackDevices
#define SDL_GetAudioRecordingDevices volya_SDL_GetAudioRecordingDevices
#define SDL_GetAudioDeviceName volya_SDL_GetAudioDeviceName
#define SDL_GetAudioDeviceFormat volya_SDL_GetAudioDeviceFormat
#define SDL_OpenAudioDeviceStream volya_SDL_OpenAudioDeviceStream
#define SDL_DestroyAudioStream volya_SDL_DestroyAudioStream
#define SDL_GetAudioStreamDevice volya_SDL_GetAudioStreamDevice
#define SDL_ResumeAudioStreamDevice volya_SDL_ResumeAudioStreamDevice
#define SDL_PauseAudioStreamDevice volya_SDL_PauseAudioStreamDevice

#include "mojoal.c"

#undef SDL_DestroyAudioStream

/* The output rate (0 while no context exists). */
__declspec(dllexport) uint32_t volya_rate(void) {
    for (int i = 0; i < VOLYA_MAX_STREAMS; i++) {
        if (volya_streams[i].stream) {
            return VOLYA_RATE;
        }
    }
    return 0;
}

/* Renders the next `frames` samples of every context into `out` (16-bit,
 * stereo, interleaved), as the device's callback would on a device;
 * silence with no context. Returns the samples clipped converting to 16
 * bits. Called by the frame clock on the game's main thread, once a frame. */
__declspec(dllexport) uint32_t volya_render(int16_t *out, uint32_t frames) {
    static float *mix, *got;
    static uint32_t capacity;
    uint32_t n = frames * VOLYA_CHANNELS;
    if (n > capacity) {
        SDL_free(mix);
        SDL_free(got);
        mix = SDL_calloc(n, sizeof *mix);
        got = SDL_calloc(n, sizeof *got);
        capacity = mix && got ? n : 0;
    }
    SDL_memset(out, 0, (size_t)n * sizeof *out);
    if (!capacity || !api_lock) {
        return 0;
    }
    SDL_memset(mix, 0, (size_t)n * sizeof *mix);
    const int bytes = (int)(n * sizeof(float));
    grab_api_lock();
    for (int i = 0; i < VOLYA_MAX_STREAMS; i++) {
        SDL_AudioStream *s = volya_streams[i].stream;
        if (!s) {
            continue;
        }
        SDL_LockAudioStream(s);
        volya_streams[i].callback(volya_streams[i].userdata, s, bytes, bytes);
        int have = SDL_GetAudioStreamData(s, got, bytes);
        SDL_UnlockAudioStream(s);
        for (int k = 0; have > 0 && k < have / (int)sizeof(float); k++) {
            mix[k] += got[k];
        }
    }
    ungrab_api_lock();
    uint32_t clipped = 0;
    for (uint32_t k = 0; k < n; k++) {
        float v = mix[k] * 32767.0f;
        if (v > 32767.0f) {
            v = 32767.0f;
            clipped++;
        } else if (v < -32768.0f) {
            v = -32768.0f;
            clipped++;
        }
        out[k] = (int16_t)(v >= 0.0f ? v + 0.5f : v - 0.5f);
    }
    return clipped;
}
