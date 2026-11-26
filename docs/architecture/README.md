# Architecture

Decisions, their reasons, and where things live. Update the document an area's decision belongs to when it changes; track work in issues.

## Crates

Split only where a split buys compile-time isolation, a compiler-enforced boundary, or reuse outside the product; subsystems (AI, physics, A-Life, UI) are modules, not crates. Naming: AGENTS.md.

| Package | Contents | Why separate |
|---|---|---|
| `crates/volya-engine` | The platform-free product: composes the role crates into one engine (screens such as the startup sequence, the title menu, the loading screen and the level, driven by a clock and input, producing scenes the presenter turns into draw lists), save and load, the traits platforms implement (`SaveFiles`, `AudioOut`), and the test interface. | The product as one library every platform ships. |
| `crates/volya-engine-core` | Platform-free engine: the VFS trait, its native backend and the game-archive tree, configuration and the console, audio mixing and the sound rules, the offline simulation, collision and movement, the character, weather, lights, particles, zones. | Deterministic and headless-testable; cannot reach rendering or any platform. |
| `crates/volya-engine-render` | The WebGL 2 renderer over its GL layer (`gl.rs`): textures, video textures, the level's world, sky, particles, lights, GPU timing; draw lists to frames. | Keeps renderer rebuilds off the simulation's edit loop; keeps rendering out of simulation. |
| `crates/volya-engine-formats` | Parsers and decoders over `Read + Seek` or bytes, no I/O: the game's own formats (from clean specs) and the public ones it uses (DDS, Ogg Vorbis, Ogg Theora), the ETC2 encoder; the `volya-engine-db` tool. | Reused by tools; one home for clean-room provenance and game-data tests. |
| `crates/volya-engine-script` | The script runtime: vendored LuaJIT 1.0.3, our C library for wasm32, a thin safe Rust API, the loader of the game's scripts over core's VFS. | Keeps the C build and all `unsafe` FFI in one place; one home for the third-party source and its provenance. |
| `crates/volya-engine-installer` | GOG offline installer reader and extractor (Inno Setup headers, GOG parts). | Unrelated to the game; runs in the import worker; has its own CLI. |
| `platforms/web` | Crate `volya-engine-web` (wasm `cdylib`: entry point, OPFS, workers, `OffscreenCanvas`) plus `static/` (HTML, JS, CSS, icons). The only package using `web-sys` but the render crate's GL layer (WebGL 2). | Platform packaging and APIs; compiles to nothing elsewhere. |

Dependencies point one way: `volya-engine-web` → `volya-engine` (and `formats`, `installer`); `volya-engine` → `core`, `render`, `script`, `formats`; `script` → `core`; `core` → `formats`; `render` → `formats`. `core` and `render` never see each other; the installer is otherwise only a dev-dependency (game-data tests). Everything in `crates/` builds and tests natively. Tools live in `tools/`: `xtask` (every project task), `devserver` (serves the app and the browser suites), `simdriver` (an XCUITest project outside the workspace, run by `cargo xtask sim`).

## Threads and workers

| Where | Runs |
|---|---|
| Page (`static/main.js`) | The app's own interface, input, the AudioContext; forwards input to the engine worker. |
| Engine worker (`static/engine-worker.js`, the wasm build) | The whole engine on one thread: VFS over OPFS sync handles, loading, simulation, scripts, the sound system, rendering on the transferred `OffscreenCanvas` through WebGL 2. |
| Audio worker (`static/audio-worker.js`, the same wasm build in its own instance) | Applies the sound system's mixer commands, decodes and mixes. |
| AudioWorklet (`static/audio-worklet.js`) | Plays the mixed frames. |
| Import worker (`static/import-worker.js`) | Unpacks the GOG installer into OPFS. |

## Main data flow

1. **Import, once**: the player picks the GOG installer; the import worker inflates every game file into OPFS as stored ([game-data.md](game-data.md)).
2. **Start**: the page starts the engine worker with the canvas; the engine mounts the imported files read-only, reads the configuration and the user's `user.ltx`, and the page connects audio ([web-platform.md](web-platform.md)).
3. **Each frame** in the engine worker: input from the page → the app's update (screens, the level: [simulation.md](simulation.md), [scripts.md](scripts.md)) → the sound system → mixer commands into a shared ring for the audio worker ([audio.md](audio.md)) → a scene → draw lists → the renderer → the canvas ([rendering.md](rendering.md)).
4. **Loading a level**: one loading stage per frame, reading the game's original formats synchronously from OPFS, then the level's textures and geometry prepared on the GPU before the level shows ([rendering.md](rendering.md#level-preparation)).

## Documents

- [web-platform.md](web-platform.md): threads and workers, synchronous I/O, the page, the hidden page, browser zoom, hosting, isolation, the service worker and updates, the UX audit page.
- [game-data.md](game-data.md): bringing the installer, import, original formats, storage.
- [rendering.md](rendering.md): WebGL 2, textures and ETC2, level preparation, the video mode, the world, objects, colour and video.
- [audio.md](audio.md): the workers' split, the shared-memory transport, decoding, positional playback, the level's sounds, footsteps.
- [simulation.md](simulation.md): the offline simulation, collision and movement, the character, particles, zones, lights, torches.
- [scripts.md](scripts.md): the LuaJIT runtime, its build and the script loader.
- [input.md](input.md): input forwarding, the touchpad, the level's controls, text entry, full screen.
- [screens.md](screens.md): fatal errors, settings, the menu during a game, saved games, the inventory screen.
- [diagnostics.md](diagnostics.md): the diagnostic log.
- [memory.md](memory.md): memory by lifetime and owner, the one-heap design, GPU ownership and release, the migration.
- [build.md](build.md): Bazel: sharing across worktrees, Rust and crates, build scripts, the web app, checks, tests and their tags, commands, the migration from Cargo.
- Testing: [docs/testing.md](../testing.md).
