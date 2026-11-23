# Architecture

Decisions and the reasons behind them. Update this file when a decision changes.

## Threading: one engine thread, in a worker

The engine (VFS, simulation, scripting, A-Life, physics, rendering) runs on a single thread inside a dedicated Web Worker.

- **Single-threaded simulation** matches the original X-Ray engine's observable behavior and update order, keeps simulation deterministic, and fits comfortably within the reference device's budget (iPhone 17 Pro Max).
- **In a worker** because OPFS `FileSystemSyncAccessHandle` is worker-only. Synchronous reads let archive readers be plain `Read + Seek` Rust.
- **Rendering** uses WebGPU on an `OffscreenCanvas` transferred from the main thread. Pending verification on iOS Safari (#11). Fallback if unavailable: render on the main thread from frame data posted by the engine worker.
- **Main thread** owns DOM UI and input. It forwards normalized input events (touch, keyboard/mouse, gamepad) to the engine worker via `postMessage`.
- **Audio**: Web Audio lives on the main thread / an AudioWorklet; the engine sends it commands or buffers by message. It is an output channel, not shared state.
- **Other workers** do self-contained jobs and exchange messages only, never simulation state: the importer (multi-GB unpack), and later, if profiling demands it, asset decoding.
- **No `SharedArrayBuffer`, no wasm threads.** This removes the need for cross-origin isolation (COOP/COEP), which GitHub Pages cannot provide. Revisit only with profiling data.

## Hosting: GitHub Pages, project site

Served from `https://mmotorny.github.io/volya-engine/`, deployed by pushing the locally built `dist/` to the `gh-pages` branch (#12). No custom domain: it emphasizes the project's free, non-commercial nature.

Constraints this imposes:

- **No custom headers.** Never depend on cross-origin isolation. `scripts/serve.mjs` sends no such headers and a browser test asserts the page is not isolated.
- **Base path `/volya-engine/`.** All URLs are relative. The dev server serves under the same path.
- **Shared origin.** Every `mmotorny.github.io` project site shares one origin, so storage quota, OPFS, IndexedDB, caches and service worker registrations are shared with unrelated sites. Keep everything under a `volya-engine` namespace (OPFS directory, cache names, IndexedDB names), scope the service worker to `/volya-engine/`, and treat stored data as possibly modified by others: validate on load, never trust it blindly.
- **No cache control.** Pages sets its own short cache lifetimes; the service worker owns app versioning.

## Game data: BYOA, local only

Game data never reaches any server. The user imports their own copy (GOG offline installer of the original Shadow of Chernobyl first, Enhanced Edition later) via the file picker; the importer extracts the game's `.db` archives into OPFS as whole files, and the engine reads them in place with random access.
