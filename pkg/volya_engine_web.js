/**
 * Sends the mixer's commands since the last call to the audio worker
 * through the ring of commands (no message, nothing for a garbage
 * collector) and wakes it; returns the files they play, `[number,
 * Uint8Array]` pairs for the worker to post, or null. The files leave the
 * engine's heap.
 * @returns {any}
 */
export function audio_flush() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.audio_flush();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * A new output on `memory` (`crate::audio_ring`), whose ring of frames
 * is kept filled to `target` frames (`static/audio-ring.js`): a fresh
 * mixer from the engine's first commands. `running`: whether the output
 * is expected to run ([`audio_host_running`]).
 * @param {number} sample_rate
 * @param {number} target
 * @param {any} memory
 * @param {boolean} running
 */
export function audio_host_connect(sample_rate, target, memory, running) {
    __wbg_call_guard();
    try {
        wasm.audio_host_connect(sample_rate, target, memory, running);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * File `number` arrived from the engine worker (`audio_flush`). It
 * stays in JavaScript memory, read a piece at a time ([`JsFile`]), so the
 * wasm heap, which never shrinks, holds none of it.
 * @param {number} number
 * @param {Uint8Array} bytes
 */
export function audio_host_file(number, bytes) {
    __wbg_call_guard();
    try {
        wasm.audio_host_file(number, bytes);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * A report for the engine (`audio_report`), JSON, when there is news: the
 * voices that ended, the recorded blocks, a heap that grew, and every
 * second of output the counters and the diagnostic log's numbers.
 * `undefined` otherwise.
 * @returns {string | undefined}
 */
export function audio_host_report() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.audio_host_report();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    let v1;
    if (ret[0] !== 0) {
        v1 = getStringFromWasm0(ret[0], ret[1]);
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v1;
}

/**
 * Whether the output is expected to run: its page shown and its
 * AudioContext running. A hidden page's workers are throttled or frozen
 * and a suspended context consumes nothing, so the time between services
 * counts only while this holds.
 * @param {boolean} on
 */
export function audio_host_running(on) {
    __wbg_call_guard();
    try {
        wasm.audio_host_running(on);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * Applies the commands the ring holds and fills the ring of frames to its
 * target. Run whenever the audio worker wakes; `polled`: woken by its own
 * timeout rather than a wake.
 * @param {boolean} polled
 */
export function audio_host_service(polled) {
    __wbg_call_guard();
    try {
        wasm.audio_host_service(polled);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * (Re)starts audio output at the output's sample rate on `memory`, a new
 * output's shared memory (`crate::audio_ring`); stops any sounds and keeps
 * the category volumes and the master gain.
 * @param {number} sample_rate
 * @param {any} memory
 */
export function audio_init(sample_rate, memory) {
    __wbg_call_guard();
    try {
        wasm.audio_init(sample_rate, memory);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * A report of the audio worker (`crate::audio_host::audio_host_report`):
 * the voices that ended, its counters, the diagnostic log's numbers and,
 * while capturing, its blocks.
 * @param {string} report
 */
export function audio_report(report) {
    const ptr0 = passStringToWasm0(report, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.audio_report(ptr0, len0);
    } catch(e) {
        __wbg_handle_catch(e);
    }
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

/**
 * Engine interface for the page, tests and debugging: runs `op` with JSON
 * `args` and returns its JSON result.
 * @param {string} op
 * @param {string} args
 * @returns {Promise<string>}
 */
export function call(op, args) {
    let ptr0, len0, ptr1, len1;
    try {
        ptr0 = passStringToWasm0(op, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passStringToWasm0(args, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len1 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
        throw e;
    }
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.call(ptr0, len0, ptr1, len1);
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * Whether the console is open: the page reads the clipboard for an
 * Insert press then (#639).
 * @returns {boolean}
 */
export function console_open() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.console_open();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret !== 0;
}

/**
 * Whether the console would open over the main menu now: the page's
 * Console button for touch screens shows over the menu then (#642).
 * @returns {boolean}
 */
export function console_opens_over_menu() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.console_opens_over_menu();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret !== 0;
}

/**
 * Whether the game shows its mouse cursor: the page's touch drives the
 * cursor then (the on-screen stick, #856, or direct clicks, #807).
 * @returns {boolean}
 */
export function cursor_shown() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.cursor_shown();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret !== 0;
}

/**
 * The JS bridge: adds `n` to the count `name`.
 * @param {string} name
 * @param {number} n
 */
export function diagnostics_count(name, n) {
    const val2 = +n;
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    __wbg_call_guard();
    try {
        wasm.diagnostics_count(ptr0, len0, val2);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * The JS bridge: how the engine or a game ended (`engine.end`, on thread
 * `tid`), stored at once: an uncaught error or rejection in the page, the
 * engine worker or the audio worklet, the page hidden or closed.
 * @param {string} reason
 * @param {string} message
 * @param {number} tid
 */
export function diagnostics_end(reason, message, tid) {
    const val3 = +tid;
    let ptr0, len0, ptr1, len1;
    try {
        ptr0 = passStringToWasm0(reason, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passStringToWasm0(message, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len1 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
        throw e;
    }
    __wbg_call_guard();
    try {
        wasm.diagnostics_end(ptr0, len0, ptr1, len1, val3);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * The JS bridge: an instant event `name` on thread `tid` (2: the page, 3:
 * the audio worklet) with `args`, a JSON object.
 * @param {string} name
 * @param {number} tid
 * @param {string} args
 */
export function diagnostics_event(name, tid, args) {
    const val3 = +tid;
    let ptr0, len0, ptr1, len1;
    try {
        ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len0 = WASM_VECTOR_LEN;
        ptr1 = passStringToWasm0(args, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        len1 = WASM_VECTOR_LEN;
    } catch (e) {
        if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
        if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
        throw e;
    }
    __wbg_call_guard();
    try {
        wasm.diagnostics_event(ptr0, len0, val3, ptr1, len1);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * The JS bridge: `count` values of the metric `name` aggregated by the
 * page (their minimum, sum and maximum).
 * @param {string} name
 * @param {number} count
 * @param {number} min
 * @param {number} sum
 * @param {number} max
 */
export function diagnostics_metrics(name, count, min, sum, max) {
    const val2 = +count;
    const val3 = +min;
    const val4 = +sum;
    const val5 = +max;
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    __wbg_call_guard();
    try {
        wasm.diagnostics_metrics(ptr0, len0, val2, val3, val4, val5);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * The game's resolution follows the canvas's size (`on`: the page's
 * ergonomic mobile mode, #806) or not; [`resize`] first gives the
 * size of now.
 * @param {boolean} on
 */
export function follow_screen(on) {
    __wbg_call_guard();
    try {
        wasm.follow_screen(on);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * Runs one animation frame; `time` in milliseconds.
 * @param {number} time
 */
export function frame(time) {
    __wbg_call_guard();
    try {
        wasm.frame(time);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * The page is in the browser's full screen (`on`) or not, or can't be
 * (`supported` false): `rs_fullscreen` follows it, and the options
 * screen's check box is disabled where it can't be.
 * @param {boolean} on
 * @param {boolean} supported
 */
export function full_screen(on, supported) {
    __wbg_call_guard();
    try {
        wasm.full_screen(on, supported);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * Full screen asked of the page by a video restart, once: 1 to enter, 0
 * to leave, −1 for nothing.
 * @returns {number}
 */
export function full_screen_request() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.full_screen_request();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * One input message from the page (format in the `input` module).
 * @param {Array<any>} message
 */
export function input(message) {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.input(message);
    } catch(e) {
        __wbg_handle_catch(e);
    }
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

/**
 * Whether the engine owns the screen, so the page should forward input to
 * it. Off by default; the `input_capture` op sets it.
 * @returns {boolean}
 */
export function input_capture() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.input_capture();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret !== 0;
}

/**
 * The level's controls for the page: bit 1 while they are live,
 * plus 2 while the crouch toggle is engaged, 4 while a sprint is
 * requested, 8 while the toggled crouch is the low crouch and 16 while the
 * sprint lock holds the touch stick; 32 while the character takes keys
 * (the level's controls live, or the inventory screen over them, #110);
 * 64 while the touch Torch button shows and 128 while the torch is on, 256
 * while the Night vision button shows and 512 while night vision is
 * active (#363); 1024 while the PDA window is open over the running level
 * (the touch PDA button shows over it, #730); 2048 while an object under
 * the crosshair has a hint (the touch Use button shows).
 * @returns {number}
 */
export function level_controls() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.level_controls();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret >>> 0;
}

/**
 * Parses the installer's `.exe` and returns the files an import would write
 * (JSON `{language, files: [{path, size}], total}`). The language comes from
 * the installer's data, preferring the device's `preferred` languages
 * (BCP 47 tags) among those it offers.
 * @param {any} exe
 * @param {string[]} preferred
 * @returns {string}
 */
export function plan_import(exe, preferred) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passArrayJsValueToWasm0(preferred, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        let ret;
        __wbg_call_guard();
        try {
            ret = wasm.plan_import(exe, ptr0, len0);
        } catch(e) {
            __wbg_handle_catch(e);
        }
        var ptr2 = ret[0];
        var len2 = ret[1];
        if (ret[3]) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        __wbg_call_guard();
        try {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        } catch(e) {
            __wbg_handle_catch(e);
        }
    }
}

/**
 * The canvas's size in CSS pixels and the page's devicePixelRatio, on start
 * and whenever they change.
 * @param {number} width
 * @param {number} height
 * @param {number} dpr
 */
export function resize(width, height, dpr) {
    __wbg_call_guard();
    try {
        wasm.resize(width, height, dpr);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * Extracts the planned files (or only those whose paths are in `only`) from
 * the `.bin` into OPFS, verifying every MD5. Resumes after an interruption:
 * files the manifest already lists are skipped. Calls
 * `progress(doneBytes, totalBytes, path)` as data is written.
 * @param {any} bin
 * @param {any} only
 * @param {Function} progress
 * @returns {Promise<void>}
 */
export function run_import(bin, only, progress) {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.run_import(bin, only, progress);
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * Starts the engine on the canvas transferred from the page. Without
 * graphics the engine still starts; `status` reports why they are
 * unavailable.
 * @param {OffscreenCanvas} canvas
 * @returns {Promise<void>}
 */
export function start(canvas) {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.start(canvas);
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * The save screen's name field while it shows, for the page's text entry
 * (#94): `[x, y, width, height, focused, over]`, canvas CSS pixels, then 1
 * or 0 for its focus and for the cursor over it (#108).
 * @returns {Float32Array | undefined}
 */
export function text_field() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.text_field();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    let v1;
    if (ret[0] !== 0) {
        v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    }
    return v1;
}

/**
 * Where the destination of a tap-to-move trip under way shows in the
 * picture, for the touch controls' marker: `[x, y]` from its centre in half
 * its height, +y down; none without a trip or with it behind the eye.
 * @returns {Float32Array | undefined}
 */
export function trip_marker() {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.trip_marker();
    } catch(e) {
        __wbg_handle_catch(e);
    }
    let v1;
    if (ret[0] !== 0) {
        v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    }
    return v1;
}

/**
 * The page was hidden (`hidden`) or shown again (our design, #93): the
 * engine stops while hidden; the worker's frame loop idles meanwhile and
 * the page suspends audio output.
 * @param {boolean} hidden
 */
export function visibility(hidden) {
    __wbg_call_guard();
    try {
        wasm.visibility(hidden);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_208446807d0d1e30: function() { return wrapError(function (arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg___wbindgen_boolean_get_4f1baf9a00be3c74: function() { return wrapError(function (arg0) {
            const v = arg0;
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        }, arguments); },
        __wbg___wbindgen_debug_string_f249a2a76caa9a55: function() { return wrapError(function (arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg___wbindgen_is_falsy_d864b623500ae65d: function() { return wrapError(function (arg0) {
            const ret = !arg0;
            return ret;
        }, arguments); },
        __wbg___wbindgen_is_function_741ff2bb612836f5: function() { return wrapError(function (arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        }, arguments); },
        __wbg___wbindgen_is_null_c22311b1cc228e3a: function() { return wrapError(function (arg0) {
            const ret = arg0 === null;
            return ret;
        }, arguments); },
        __wbg___wbindgen_is_undefined_be5b34249ff1fc15: function() { return wrapError(function (arg0) {
            const ret = arg0 === undefined;
            return ret;
        }, arguments); },
        __wbg___wbindgen_memory_17a1178bc3328ea3: function() { return wrapError(function () {
            const ret = wasm.memory;
            return ret;
        }, arguments); },
        __wbg___wbindgen_number_get_934b640d0889af78: function() { return wrapError(function (arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64((arg0 >>> 0) + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, !isLikeNone(ret), true);
        }, arguments); },
        __wbg___wbindgen_string_get_7ce8eee7fd327545: function() { return wrapError(function (arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg___wbindgen_throw_4c88d8bf264fd984: function() { return wrapError(function (arg0, arg1) {
            throw new WebAssembly.Exception(__wbindgen_wrapped_jstag, [new Error(getStringFromWasm0(arg0, arg1))]);
        }, arguments); },
        __wbg__wbg_cb_unref_09736879a3f67559: function() { return wrapError(function (arg0) {
            arg0._wbg_cb_unref();
        }, arguments); },
        __wbg_activeTexture_89cef8c297ad0698: function() { return wrapError(function (arg0, arg1) {
            arg0.activeTexture(arg1 >>> 0);
        }, arguments); },
        __wbg_addEventListener_847fd86b8c33ec43: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_add_1927681aea05b9ca: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = Atomics.add(arg0, arg1 >>> 0, arg2);
            return ret;
        }, arguments); },
        __wbg_apply_bfe2b07b146427e2: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.apply(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_attachShader_3ed301cc92db59f6: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.attachShader(arg1, arg2);
        }, arguments); },
        __wbg_bindBufferRange_da5accbcd5d223ad: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.bindBufferRange(arg1 >>> 0, arg2 >>> 0, arg3, arg4, arg5);
        }, arguments); },
        __wbg_bindBuffer_cbc9075f76fffa51: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.bindBuffer(arg1 >>> 0, arg2);
        }, arguments); },
        __wbg_bindFramebuffer_79534f24b57b7b95: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.bindFramebuffer(arg1 >>> 0, arg2);
        }, arguments); },
        __wbg_bindSampler_0fb033a6ffd5d7b3: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.bindSampler(arg1 >>> 0, arg2);
        }, arguments); },
        __wbg_bindTexture_a23350e1b573ddfd: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.bindTexture(arg1 >>> 0, arg2);
        }, arguments); },
        __wbg_bindVertexArray_cc64c48bf67ab8d4: function() { return wrapError(function (arg0, arg1) {
            arg0.bindVertexArray(arg1);
        }, arguments); },
        __wbg_blendEquationSeparate_0dceb23863585a87: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.blendEquationSeparate(arg1 >>> 0, arg2 >>> 0);
        }, arguments); },
        __wbg_blendFuncSeparate_6db2212299fd7f1e: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.blendFuncSeparate(arg1 >>> 0, arg2 >>> 0, arg3 >>> 0, arg4 >>> 0);
        }, arguments); },
        __wbg_blitFramebuffer_d17b7bddf5e86dbc: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8, arg9, arg10) {
            arg0.blitFramebuffer(arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8, arg9 >>> 0, arg10 >>> 0);
        }, arguments); },
        __wbg_bufferData_41b591a9f2d502ad: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.bufferData(arg1 >>> 0, arg2, arg3 >>> 0);
        }, arguments); },
        __wbg_bufferData_bab7dfe2951e9eab: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.bufferData(arg1 >>> 0, getArrayU8FromWasm0(arg2, arg3), arg4 >>> 0);
        }, arguments); },
        __wbg_bufferSubData_6dcda1ba2321d376: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.bufferSubData(arg1 >>> 0, arg2, getArrayU8FromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_buffer_0eaa216bac6b8749: function() { return wrapError(function (arg0) {
            const ret = arg0.buffer;
            return ret;
        }, arguments); },
        __wbg_buffer_aca05a93c046db1a: function() { return wrapError(function (arg0) {
            const ret = arg0.buffer;
            return ret;
        }, arguments); },
        __wbg_byteLength_fa48f24814f5ef46: function() { return wrapError(function (arg0) {
            const ret = arg0.byteLength;
            return ret;
        }, arguments); },
        __wbg_call_275b78b3b6520b92: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            const ret = arg0.call(arg1, arg2, arg3, arg4);
            return ret;
        }, arguments); },
        __wbg_call_40b008ba32f73991: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0.call(arg1);
            return ret;
        }, arguments); },
        __wbg_call_a410eb4b59ed5965: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_call_fc92fee6137d452d: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.call(arg1, arg2, arg3);
            return ret;
        }, arguments); },
        __wbg_clearBufferfv_474b28c18db287a8: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.clearBufferfv(arg1 >>> 0, arg2, getArrayF32FromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_close_264222edec0e3164: function() { return wrapError(function (arg0) {
            arg0.close();
        }, arguments); },
        __wbg_colorMask_24c1b6c3ec53f8c8: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.colorMask(arg1 !== 0, arg2 !== 0, arg3 !== 0, arg4 !== 0);
        }, arguments); },
        __wbg_compileShader_11eb6754e9b15970: function() { return wrapError(function (arg0, arg1) {
            arg0.compileShader(arg1);
        }, arguments); },
        __wbg_compressedTexSubImage2D_2a0eeab76302261a: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8, arg9) {
            arg0.compressedTexSubImage2D(arg1 >>> 0, arg2, arg3, arg4, arg5, arg6, arg7 >>> 0, getArrayU8FromWasm0(arg8, arg9));
        }, arguments); },
        __wbg_createBuffer_2c27ad5f899d418d: function() { return wrapError(function (arg0) {
            const ret = arg0.createBuffer();
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_createFramebuffer_cdf686778c61014d: function() { return wrapError(function (arg0) {
            const ret = arg0.createFramebuffer();
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_createProgram_ea784724f6f0074f: function() { return wrapError(function (arg0) {
            const ret = arg0.createProgram();
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_createSampler_5bc030b64ed02c52: function() { return wrapError(function (arg0) {
            const ret = arg0.createSampler();
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_createShader_9754a1e4ef374e84: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0.createShader(arg1 >>> 0);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_createSyncAccessHandle_0a8b725af58d1cf9: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0.createSyncAccessHandle(arg1);
            return ret;
        }, arguments); },
        __wbg_createSyncAccessHandle_9fc3d780677bd197: function() { return wrapError(function (arg0) {
            const ret = arg0.createSyncAccessHandle();
            return ret;
        }, arguments); },
        __wbg_createTexture_3287f26407473071: function() { return wrapError(function (arg0) {
            const ret = arg0.createTexture();
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_createVertexArray_1293d2ffa8346be2: function() { return wrapError(function (arg0) {
            const ret = arg0.createVertexArray();
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_cullFace_9eaca57652d7ed37: function() { return wrapError(function (arg0, arg1) {
            arg0.cullFace(arg1 >>> 0);
        }, arguments); },
        __wbg_deleteBuffer_673584ba5a55669d: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteBuffer(arg1);
        }, arguments); },
        __wbg_deleteFramebuffer_ffdb1bee4c92ae4c: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteFramebuffer(arg1);
        }, arguments); },
        __wbg_deleteProgram_c07d952a1b0c1cb6: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteProgram(arg1);
        }, arguments); },
        __wbg_deleteSampler_2b73428131e403e1: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteSampler(arg1);
        }, arguments); },
        __wbg_deleteShader_ac74568b64a1c3c7: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteShader(arg1);
        }, arguments); },
        __wbg_deleteSync_eede535e5826f144: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteSync(arg1);
        }, arguments); },
        __wbg_deleteTexture_1653a9994a44215e: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteTexture(arg1);
        }, arguments); },
        __wbg_deleteVertexArray_2589b74b67ba20c8: function() { return wrapError(function (arg0, arg1) {
            arg0.deleteVertexArray(arg1);
        }, arguments); },
        __wbg_depthFunc_04abd9f6ff980ba9: function() { return wrapError(function (arg0, arg1) {
            arg0.depthFunc(arg1 >>> 0);
        }, arguments); },
        __wbg_depthMask_2ba7e83178c3cec3: function() { return wrapError(function (arg0, arg1) {
            arg0.depthMask(arg1 !== 0);
        }, arguments); },
        __wbg_depthRange_8b4b69a7c91ff92b: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.depthRange(arg1, arg2);
        }, arguments); },
        __wbg_disable_8a9e81e6d64d046b: function() { return wrapError(function (arg0, arg1) {
            arg0.disable(arg1 >>> 0);
        }, arguments); },
        __wbg_drawArrays_a9c18db2e94f4a82: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.drawArrays(arg1 >>> 0, arg2, arg3);
        }, arguments); },
        __wbg_drawElementsInstanced_3240c0c168d6af87: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.drawElementsInstanced(arg1 >>> 0, arg2, arg3 >>> 0, arg4, arg5);
        }, arguments); },
        __wbg_drawElements_d76df94ac144b73b: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.drawElements(arg1 >>> 0, arg2, arg3 >>> 0, arg4);
        }, arguments); },
        __wbg_enableVertexAttribArray_eeecc6185af24227: function() { return wrapError(function (arg0, arg1) {
            arg0.enableVertexAttribArray(arg1 >>> 0);
        }, arguments); },
        __wbg_enable_e955d456ef6b93f7: function() { return wrapError(function (arg0, arg1) {
            arg0.enable(arg1 >>> 0);
        }, arguments); },
        __wbg_error_d1fc0193610e0e39: function() { return wrapError(function (arg0) {
            console.error(arg0);
        }, arguments); },
        __wbg_fenceSync_53f6f24f4bb43d24: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.fenceSync(arg1 >>> 0, arg2 >>> 0);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_flush_34f73503687e5840: function() { return wrapError(function (arg0) {
            arg0.flush();
        }, arguments); },
        __wbg_flush_a9981e0bd1a44897: function() { return wrapError(function (arg0) {
            arg0.flush();
        }, arguments); },
        __wbg_framebufferTexture2D_b6250a0c37eb48c2: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.framebufferTexture2D(arg1 >>> 0, arg2 >>> 0, arg3 >>> 0, arg4, arg5);
        }, arguments); },
        __wbg_frontFace_8bbd77ca3b8f90a6: function() { return wrapError(function (arg0, arg1) {
            arg0.frontFace(arg1 >>> 0);
        }, arguments); },
        __wbg_getContext_f2ea37484e3f8ff4: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.getContext(getStringFromWasm0(arg1, arg2), arg3);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_getDate_a44b5f4a8c42a3c3: function() { return wrapError(function (arg0) {
            const ret = arg0.getDate();
            return ret;
        }, arguments); },
        __wbg_getDirectoryHandle_7250477470a7f35b: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.getDirectoryHandle(getStringFromWasm0(arg1, arg2), arg3);
            return ret;
        }, arguments); },
        __wbg_getDirectory_50787a69f3a13110: function() { return wrapError(function (arg0) {
            const ret = arg0.getDirectory();
            return ret;
        }, arguments); },
        __wbg_getExtension_30c10f49f67eb05d: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.getExtension(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_getFileHandle_1cb5562219601c4f: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.getFileHandle(getStringFromWasm0(arg1, arg2), arg3);
            return ret;
        }, arguments); },
        __wbg_getFileHandle_d551aa420ea57cd0: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.getFileHandle(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_getFile_ebc6243a91493807: function() { return wrapError(function (arg0) {
            const ret = arg0.getFile();
            return ret;
        }, arguments); },
        __wbg_getFullYear_94114da14f235fca: function() { return wrapError(function (arg0) {
            const ret = arg0.getFullYear();
            return ret;
        }, arguments); },
        __wbg_getHours_d55d13f9f95415bb: function() { return wrapError(function (arg0) {
            const ret = arg0.getHours();
            return ret;
        }, arguments); },
        __wbg_getMinutes_d0d71c490ef83b47: function() { return wrapError(function (arg0) {
            const ret = arg0.getMinutes();
            return ret;
        }, arguments); },
        __wbg_getMonth_a4698d688e95d9bd: function() { return wrapError(function (arg0) {
            const ret = arg0.getMonth();
            return ret;
        }, arguments); },
        __wbg_getParameter_4c67158a3cef7a34: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0.getParameter(arg1 >>> 0);
            return ret;
        }, arguments); },
        __wbg_getProgramInfoLog_01799fd7b07b7f3c: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg1.getProgramInfoLog(arg2);
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getProgramParameter_1e7562e7d66170c3: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.getProgramParameter(arg1, arg2 >>> 0);
            return ret;
        }, arguments); },
        __wbg_getSeconds_7143dabeecfaf7d1: function() { return wrapError(function (arg0) {
            const ret = arg0.getSeconds();
            return ret;
        }, arguments); },
        __wbg_getShaderInfoLog_3cfe5b3e097d0f2d: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg1.getShaderInfoLog(arg2);
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getSize_c72004a2a12e972a: function() { return wrapError(function (arg0) {
            const ret = arg0.getSize();
            return ret;
        }, arguments); },
        __wbg_getSyncParameter_8b7b36e9efd95806: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.getSyncParameter(arg1, arg2 >>> 0);
            return ret;
        }, arguments); },
        __wbg_getUniformBlockIndex_0742ae35f926850a: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.getUniformBlockIndex(arg1, getStringFromWasm0(arg2, arg3));
            return ret;
        }, arguments); },
        __wbg_getUniformLocation_6352e0669c8f5393: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.getUniformLocation(arg1, getStringFromWasm0(arg2, arg3));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_get_2080c49015e35254: function() { return wrapError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_d7176cfb76d71114: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        }, arguments); },
        __wbg_get_unchecked_e6325ad374eff861: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        }, arguments); },
        __wbg_height_a6466ebf29036658: function() { return wrapError(function (arg0) {
            const ret = arg0.height;
            return ret;
        }, arguments); },
        __wbg_instanceof_FileSystemDirectoryHandle_4d0d8d8275d0019c: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof FileSystemDirectoryHandle;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_FileSystemFileHandle_5a487c9c312d952b: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof FileSystemFileHandle;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_FileSystemHandle_4efda303ea0169b1: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof FileSystemHandle;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_FileSystemSyncAccessHandle_75b74eadc68c22d2: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof FileSystemSyncAccessHandle;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_File_8b277b2875442560: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof File;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_Memory_5e105758fd5c05e5: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof WebAssembly.Memory;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_Uint8Array_d7aeb0dcb3108fea: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_WebGl2RenderingContext_de6feb460369d6c6: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof WebGL2RenderingContext;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_instanceof_WorkerGlobalScope_a8f1cd8c19b6daec: function() { return wrapError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof WorkerGlobalScope;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        }, arguments); },
        __wbg_invalidateFramebuffer_33206c632642a1bd: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.invalidateFramebuffer(arg1 >>> 0, arg2);
        }, arguments); },
        __wbg_isArray_bcf1376d0d732d08: function() { return wrapError(function (arg0) {
            const ret = Array.isArray(arg0);
            return ret;
        }, arguments); },
        __wbg_kind_e454307def230018: function() { return wrapError(function (arg0) {
            const ret = arg0.kind;
            return (__wbindgen_enum_FileSystemHandleKind.indexOf(ret) + 1 || 3) - 1;
        }, arguments); },
        __wbg_lastModified_c103c69e6c705fd3: function() { return wrapError(function (arg0) {
            const ret = arg0.lastModified;
            return ret;
        }, arguments); },
        __wbg_length_186163212ffe79c3: function() { return wrapError(function (arg0) {
            const ret = arg0.length;
            return ret;
        }, arguments); },
        __wbg_length_6e9d2869242cabf3: function() { return wrapError(function (arg0) {
            const ret = arg0.length;
            return ret;
        }, arguments); },
        __wbg_length_b03a0fabb109b1e9: function() { return wrapError(function (arg0) {
            const ret = arg0.length;
            return ret;
        }, arguments); },
        __wbg_linkProgram_99848b73f1ebab5f: function() { return wrapError(function (arg0, arg1) {
            arg0.linkProgram(arg1);
        }, arguments); },
        __wbg_load_21f270a6384df4e4: function() { return wrapError(function (arg0, arg1) {
            const ret = Atomics.load(arg0, arg1 >>> 0);
            return ret;
        }, arguments); },
        __wbg_name_eceada78cb9b38d8: function() { return wrapError(function (arg0, arg1) {
            const ret = arg1.name;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_navigator_9641ab32fa612dd1: function() { return wrapError(function (arg0) {
            const ret = arg0.navigator;
            return ret;
        }, arguments); },
        __wbg_new_0_36d738a0c8cfac43: function() { return wrapError(function () {
            const ret = new Date();
            return ret;
        }, arguments); },
        __wbg_new_13de428334042d26: function() { return wrapError(function (arg0) {
            const ret = new Uint8Array(arg0);
            return ret;
        }, arguments); },
        __wbg_new_37eb45aaf5eeb820: function() { return wrapError(function () {
            const ret = new Array();
            return ret;
        }, arguments); },
        __wbg_new_3e95cd7386bb9a6c: function() { return wrapError(function (arg0) {
            const ret = new Date(arg0);
            return ret;
        }, arguments); },
        __wbg_new_ba40181b3f24ac80: function() { return wrapError(function (arg0, arg1) {
            try {
                var state0 = {a: arg0, b: arg1};
                var cb0 = (arg0, arg1) => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined_______true_(a, state0.b, arg0, arg1);
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = new Promise(cb0);
                return ret;
            } finally {
                state0.a = 0;
            }
        }, arguments); },
        __wbg_new_cd4348290afae586: function() { return wrapError(function () {
            const ret = new Object();
            return ret;
        }, arguments); },
        __wbg_new_f2c208823248a2e7: function() { return wrapError(function () {
            const ret = new FileReaderSync();
            return ret;
        }, arguments); },
        __wbg_new_from_slice_b8daf545a41c6aa0: function() { return wrapError(function (arg0, arg1) {
            const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_new_typed_e09da9dae6ee5935: function() { return wrapError(function (arg0, arg1) {
            try {
                var state0 = {a: arg0, b: arg1};
                var cb0 = (arg0, arg1) => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined_______true_(a, state0.b, arg0, arg1);
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = new Promise(cb0);
                return ret;
            } finally {
                state0.a = 0;
            }
        }, arguments); },
        __wbg_new_with_byte_offset_and_length_1ea36b9f0c3bc55a: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = new Float32Array(arg0, arg1 >>> 0, arg2 >>> 0);
            return ret;
        }, arguments); },
        __wbg_new_with_byte_offset_and_length_3cd32055a417f095: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0);
            return ret;
        }, arguments); },
        __wbg_new_with_byte_offset_and_length_d462c70bf9b35a75: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = new Int32Array(arg0, arg1 >>> 0, arg2 >>> 0);
            return ret;
        }, arguments); },
        __wbg_new_with_length_01b143a7e5dd5a3e: function() { return wrapError(function (arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        }, arguments); },
        __wbg_next_040cd66fe7246513: function() { return wrapError(function (arg0) {
            const ret = arg0.next();
            return ret;
        }, arguments); },
        __wbg_notify_0ddec4d1bfac1be8: function() { return wrapError(function (arg0, arg1) {
            const ret = Atomics.notify(arg0, arg1 >>> 0);
            return ret;
        }, arguments); },
        __wbg_now_043d245551b22f9c: function() { return wrapError(function (arg0) {
            const ret = arg0.now();
            return ret;
        }, arguments); },
        __wbg_now_463ac581d2fc7ea3: function() { return wrapError(function () {
            const ret = Date.now();
            return ret;
        }, arguments); },
        __wbg_of_027eb15371aac932: function() { return wrapError(function (arg0) {
            const ret = Array.of(arg0);
            return ret;
        }, arguments); },
        __wbg_of_e7ea0fcd535c1444: function() { return wrapError(function (arg0, arg1) {
            const ret = Array.of(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_performance_2584c0f220473336: function() { return wrapError(function (arg0) {
            const ret = arg0.performance;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_pixelStorei_6dd56b4da047c480: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.pixelStorei(arg1 >>> 0, arg2);
        }, arguments); },
        __wbg_prototypesetcall_13162965d1fe7aa3: function() { return wrapError(function (arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        }, arguments); },
        __wbg_push_c9f4893fc209f768: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0.push(arg1);
            return ret;
        }, arguments); },
        __wbg_queueMicrotask_1e8503a8a690f0a3: function() { return wrapError(function (arg0) {
            const ret = arg0.queueMicrotask;
            return ret;
        }, arguments); },
        __wbg_queueMicrotask_4a1a4d8dd332373b: function() { return wrapError(function (arg0) {
            queueMicrotask(arg0);
        }, arguments); },
        __wbg_readAsArrayBuffer_b7018416df375ab3: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0.readAsArrayBuffer(arg1);
            return ret;
        }, arguments); },
        __wbg_readPixels_90fcbb55c8aec24b: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8) {
            arg0.readPixels(arg1, arg2, arg3, arg4, arg5 >>> 0, arg6 >>> 0, arg7 === 0 ? undefined : getArrayU8FromWasm0(arg7, arg8));
        }, arguments); },
        __wbg_read_fd16d549b57ad237: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.read(getArrayU8FromWasm0(arg1, arg2), arg3);
            return ret;
        }, arguments); },
        __wbg_removeEntry_66566d62eb738227: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.removeEntry(getStringFromWasm0(arg1, arg2), arg3);
            return ret;
        }, arguments); },
        __wbg_removeEntry_81c62dd6ae8e3673: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.removeEntry(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_resolve_dbf78ab4942478b0: function() { return wrapError(function (arg0) {
            const ret = Promise.resolve(arg0);
            return ret;
        }, arguments); },
        __wbg_samplerParameterf_1fb6c6796e22f10f: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.samplerParameterf(arg1, arg2 >>> 0, arg3);
        }, arguments); },
        __wbg_samplerParameteri_defcb3b230e696e7: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.samplerParameteri(arg1, arg2 >>> 0, arg3);
        }, arguments); },
        __wbg_scissor_17956d9d23dfe025: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.scissor(arg1, arg2, arg3, arg4);
        }, arguments); },
        __wbg_setTimeout_7b54aa5f72cd7066: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.setTimeout(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_set_2793479888329cb6: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(arg0, arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_set_at_f569a2586a7350aa: function() { return wrapError(function (arg0, arg1) {
            arg0.at = arg1;
        }, arguments); },
        __wbg_set_create_29c03b2d68cbe451: function() { return wrapError(function (arg0, arg1) {
            arg0.create = arg1 !== 0;
        }, arguments); },
        __wbg_set_create_8034c70deebee24f: function() { return wrapError(function (arg0, arg1) {
            arg0.create = arg1 !== 0;
        }, arguments); },
        __wbg_set_d0e2abd6ed08e04c: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.set(getArrayF32FromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_set_feb3714f475485ba: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.set(getArrayU8FromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_set_height_ae24c2f7d580ae0a: function() { return wrapError(function (arg0, arg1) {
            arg0.height = arg1 >>> 0;
        }, arguments); },
        __wbg_set_mode_1696242c3757a4a7: function() { return wrapError(function (arg0, arg1) {
            arg0.mode = __wbindgen_enum_FileSystemSyncAccessHandleMode[arg1];
        }, arguments); },
        __wbg_set_recursive_23a91460289c2a19: function() { return wrapError(function (arg0, arg1) {
            arg0.recursive = arg1 !== 0;
        }, arguments); },
        __wbg_set_width_eb3001e1dcabb909: function() { return wrapError(function (arg0, arg1) {
            arg0.width = arg1 >>> 0;
        }, arguments); },
        __wbg_shaderSource_519d0f56b613b2f9: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.shaderSource(arg1, getStringFromWasm0(arg2, arg3));
        }, arguments); },
        __wbg_slice_35cca9a0734cfe3f: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.slice(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_static_accessor_GLOBAL_THIS_003ee833af9699fc: function() { return wrapError(function () {
            const ret = typeof globalThis === 'undefined' ? undefined : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_static_accessor_GLOBAL_e4293fb22cbe77da: function() { return wrapError(function () {
            const ret = typeof global === 'undefined' ? undefined : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_static_accessor_SELF_53bb20457401453d: function() { return wrapError(function () {
            const ret = typeof self === 'undefined' ? undefined : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_static_accessor_WINDOW_b5e0810bfecb7feb: function() { return wrapError(function () {
            const ret = typeof window === 'undefined' ? undefined : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_storage_681cb2810b22dae6: function() { return wrapError(function (arg0) {
            const ret = arg0.storage;
            return ret;
        }, arguments); },
        __wbg_store_273b0a161f1e2604: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = Atomics.store(arg0, arg1 >>> 0, arg2);
            return ret;
        }, arguments); },
        __wbg_subarray_58f388a9247ac0bb: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        }, arguments); },
        __wbg_subarray_7343a34544ab34ea: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        }, arguments); },
        __wbg_texParameteri_e24e4fdc17153838: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.texParameteri(arg1 >>> 0, arg2 >>> 0, arg3);
        }, arguments); },
        __wbg_texStorage2D_e0ebb5940fa1fc5e: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.texStorage2D(arg1 >>> 0, arg2, arg3 >>> 0, arg4, arg5);
        }, arguments); },
        __wbg_texSubImage2D_4343af4633b8b8f8: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8, arg9) {
            arg0.texSubImage2D(arg1 >>> 0, arg2, arg3, arg4, arg5, arg6, arg7 >>> 0, arg8 >>> 0, arg9);
        }, arguments); },
        __wbg_texSubImage2D_80efe9d77c0f29a7: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8, arg9, arg10) {
            arg0.texSubImage2D(arg1 >>> 0, arg2, arg3, arg4, arg5, arg6, arg7 >>> 0, arg8 >>> 0, arg9 === 0 ? undefined : getArrayU8FromWasm0(arg9, arg10));
        }, arguments); },
        __wbg_then_656f253a5dd7d8ea: function() { return wrapError(function (arg0, arg1, arg2) {
            const ret = arg0.then(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_then_e3f7f2d365708be8: function() { return wrapError(function (arg0, arg1) {
            const ret = arg0.then(arg1);
            return ret;
        }, arguments); },
        __wbg_timeOrigin_8402c1d2014ef1ea: function() { return wrapError(function (arg0) {
            const ret = arg0.timeOrigin;
            return ret;
        }, arguments); },
        __wbg_toISOString_51ebc75332aba76a: function() { return wrapError(function (arg0) {
            const ret = arg0.toISOString();
            return ret;
        }, arguments); },
        __wbg_truncate_5f8dea9c665ccb45: function() { return wrapError(function (arg0, arg1) {
            arg0.truncate(arg1);
        }, arguments); },
        __wbg_uniform1i_778a02a48ecdceed: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.uniform1i(arg1, arg2);
        }, arguments); },
        __wbg_uniform1ui_79071fccd0396b88: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.uniform1ui(arg1, arg2 >>> 0);
        }, arguments); },
        __wbg_uniformBlockBinding_61c5a97721e0f36b: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            arg0.uniformBlockBinding(arg1, arg2 >>> 0, arg3 >>> 0);
        }, arguments); },
        __wbg_useProgram_d9c24a7d8632c4c1: function() { return wrapError(function (arg0, arg1) {
            arg0.useProgram(arg1);
        }, arguments); },
        __wbg_values_1f671b5d99ca862a: function() { return wrapError(function (arg0) {
            const ret = arg0.values();
            return ret;
        }, arguments); },
        __wbg_vertexAttribDivisor_5b64fc697c0496e1: function() { return wrapError(function (arg0, arg1, arg2) {
            arg0.vertexAttribDivisor(arg1 >>> 0, arg2 >>> 0);
        }, arguments); },
        __wbg_vertexAttribIPointer_a6240b64263fb63b: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.vertexAttribIPointer(arg1 >>> 0, arg2, arg3 >>> 0, arg4, arg5);
        }, arguments); },
        __wbg_vertexAttribPointer_39b706eb26c50057: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            arg0.vertexAttribPointer(arg1 >>> 0, arg2, arg3 >>> 0, arg4 !== 0, arg5, arg6);
        }, arguments); },
        __wbg_viewport_cdae9e8b2f33343d: function() { return wrapError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.viewport(arg1, arg2, arg3, arg4);
        }, arguments); },
        __wbg_warn_228ff2257384d295: function() { return wrapError(function (arg0) {
            console.warn(arg0);
        }, arguments); },
        __wbg_warn_42702744b2284b6f: function() { return wrapError(function (arg0, arg1) {
            console.warn(arg0, arg1);
        }, arguments); },
        __wbg_width_000813f5f73b7341: function() { return wrapError(function (arg0) {
            const ret = arg0.width;
            return ret;
        }, arguments); },
        __wbg_write_d97a1c0a72df2595: function() { return wrapError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.write(getArrayU8FromWasm0(arg1, arg2), arg3);
            return ret;
        }, arguments); },
        __wbindgen_generic_0000000000000001: function() { return wrapError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 68, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___wasm_bindgen_902d319ce2a9649e___JsValue__core_6a51abefd5362b2___result__Result_____wasm_bindgen_902d319ce2a9649e___JsError___true_);
            return ret;
        }, arguments); },
        __wbindgen_generic_0000000000000002: function() { return wrapError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("FileSystemSyncAccessHandle")], shim_idx: 68, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___wasm_bindgen_902d319ce2a9649e___JsValue__core_6a51abefd5362b2___result__Result_____wasm_bindgen_902d319ce2a9649e___JsError___true__32);
            return ret;
        }, arguments); },
        __wbindgen_generic_0000000000000003: function() { return wrapError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 683, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke_______true_);
            return ret;
        }, arguments); },
        __wbindgen_generic_0000000000000004: function() { return wrapError(function (arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        }, arguments); },
        __wbindgen_generic_0000000000000005: function() { return wrapError(function (arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        }, arguments); },
        __wbindgen_init_externref_table: function() { return wrapError(function () {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
        }, arguments); },
        __wbindgen_jstag: __wbindgen_jstag_polyfill,
        __wbindgen_wrapped_jstag: __wbindgen_wrapped_jstag,
    };
    return {
        __proto__: null,
        "./volya_engine_web_bg.js": import0,
    };
}

const __wbindgen_jstag_polyfill = new WebAssembly.Tag({ parameters: ['externref'] });


const __wbindgen_wrapped_jstag = new WebAssembly.Tag({ parameters: ['externref'] });


let __wbg_terminated_addr;
let __wbg_called_abort = false;
function __wbg_call_abort_hook() {
    __wbg_called_abort = true;
    try {
        const idx = getInt32ArrayMemory0()[wasm.__abort_handler.value / 4];
        if (idx) wasm.__wbindgen_export.get(idx)();
    } catch(_) {}
}

function __wbg_handle_catch(e) {
    if (e instanceof WebAssembly.Exception && e.is(__wbindgen_wrapped_jstag)) {
        throw e.getArg(__wbindgen_wrapped_jstag, 0);
    }
    getInt32ArrayMemory0()[__wbg_terminated_addr] = 1;
    __wbg_call_abort_hook();
    throw e;
}


function __wbg_call_guard() {
    __wbg_terminated_addr ??= wasm.__instance_terminated.value / 4;
    const flag = getInt32ArrayMemory0()[__wbg_terminated_addr];
    if (flag) {
        if (!__wbg_called_abort) {
            __wbg_call_abort_hook();
        }throw new Error('Module terminated');
    }
}
function wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke_______true_(arg0, arg1) {
    __wbg_call_guard();
    try {
        wasm.wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke_______true_(arg0, arg1);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

function wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___wasm_bindgen_902d319ce2a9649e___JsValue__core_6a51abefd5362b2___result__Result_____wasm_bindgen_902d319ce2a9649e___JsError___true_(arg0, arg1, arg2) {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___wasm_bindgen_902d319ce2a9649e___JsValue__core_6a51abefd5362b2___result__Result_____wasm_bindgen_902d319ce2a9649e___JsError___true_(arg0, arg1, arg2);
    } catch(e) {
        __wbg_handle_catch(e);
    }
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

function wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___wasm_bindgen_902d319ce2a9649e___JsValue__core_6a51abefd5362b2___result__Result_____wasm_bindgen_902d319ce2a9649e___JsError___true__32(arg0, arg1, arg2) {
    let ret;
    __wbg_call_guard();
    try {
        ret = wasm.wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___wasm_bindgen_902d319ce2a9649e___JsValue__core_6a51abefd5362b2___result__Result_____wasm_bindgen_902d319ce2a9649e___JsError___true__32(arg0, arg1, arg2);
    } catch(e) {
        __wbg_handle_catch(e);
    }
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

function wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined_______true_(arg0, arg1, arg2, arg3) {
    __wbg_call_guard();
    try {
        wasm.wasm_bindgen_902d319ce2a9649e___convert__closures_____invoke___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined___js_sys_fc3a133260de3747___Function_fn_wasm_bindgen_902d319ce2a9649e___JsValue_____wasm_bindgen_902d319ce2a9649e___sys__Undefined_______true_(arg0, arg1, arg2, arg3);
    } catch(e) {
        __wbg_handle_catch(e);
    }
}


const __wbindgen_enum_FileSystemHandleKind = ["file", "directory"];


const __wbindgen_enum_FileSystemSyncAccessHandleMode = ["read-only", "readwrite", "readwrite-unsafe"];

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => wasm.__wbindgen_destroy_closure(state.a, state.b));

function debugString(val) {
    switch (typeof val) {
        // primitive types
        case 'number':
        case 'boolean':
        case 'undefined':
        return `${val}`;
        case 'string':
        return JSON.stringify(val);
        case 'bigint':
        return `${val}n`;
        case 'symbol':
        return val.description !== undefined
            ? `Symbol(${val.description})`
            : 'Symbol';
        case 'function':
        return typeof val.name === 'string' && val.name !== ''
            ? `Function(${val.name})`
            : 'Function';
        case 'object': {
            if (val === null) {
                return 'null';
            }
            // objects
            if (Array.isArray(val)) {
                return '[' + Array.from(val, debugString).join(', ') + ']';
            }
            // errors
            if (val instanceof Error) {
                return `${val.name}: ${val.message}\n${val.stack}`;
            }

            const toStringOutput = toString.call(val);
            // Test for built-in
            const className = /\[object ([^\]]+)\]/.exec(toStringOutput)?.[1];

            // Failed to match the standard '[object ClassName]'
            if (className === undefined) {
                return toStringOutput;
            }
            if (className === 'Object') {
                // we're a user defined class or Object
                // JSON.stringify avoids problems with cycles, and is generally much
                // easier than looping through ownProperties of `val`.
                try {
                    return 'Object(' + JSON.stringify(val) + ')';
                } catch (_) {
                    return 'Object';
                }
            }
            // TODO we could test for more things here, like `Set`s and `Map`s.
            return className;
        }
        // Per MDN: 'This list of values is exhaustive.'
        // https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/typeof#description
    }
}

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

let cachedInt32ArrayMemory0 = null;
function getInt32ArrayMemory0() {
    if (cachedInt32ArrayMemory0 === null || cachedInt32ArrayMemory0.byteLength === 0) {
        cachedInt32ArrayMemory0 = new Int32Array(wasm.memory.buffer);
    }
    return cachedInt32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function makeMutClosure(arg0, arg1, f) {
    const state = { a: arg0, b: arg1, cnt: 1 };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            wasm.__wbindgen_destroy_closure(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

function wrapError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        if (e instanceof WebAssembly.Exception) throw e;
        throw new WebAssembly.Exception(__wbindgen_jstag_polyfill, [e], { traceStack: true });
    }
}

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedInt32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('volya_engine_web_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };

export const BUILD_ID = "b8814c8045ff9ed7";
