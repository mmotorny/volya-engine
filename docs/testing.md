# Testing

## Strategy

The game renders to an opaque WebGPU canvas, so DOM-level tools can't see it. Tests go through the engine, not the page.

1. **Native Rust tests carry most of the weight**: formats, VFS, importer, simulation, scripting, save/load. The simulation is single-threaded and deterministic, so tests feed scripted input, step N frames and assert on state, headless.
2. **Rendering** is checked by native offscreen `wgpu` renders (Metal on the Mac) of synthetic textures, read back and compared pixel by pixel with a tolerance (`crates/volya-engine-render/tests/render.rs`). Committed references use only synthetic content; references rendered from game data stay local, like the data itself.
3. **The engine test interface** (#14) lets tests inject input, step/pause frames, query state and read back the framebuffer: a direct API natively (`volya_engine::test_api`), and in the browser `await volya.call(op)` (plus `volya.status()`), which messages the engine worker and resolves with the op's JSON result. Ops live in `call` in `platforms/web/src/lib.rs`.
4. **Browser tests are thin** and cover only platform integration: startup, worker, WebGPU/`OffscreenCanvas`, OPFS, input forwarding, service worker. No WebDriver: each suite has a page, `tests/browser/<suite>.html`, served by the dev server at `/volya-engine/__test/<suite>.html` (never deployed). It starts the unchanged app, runs the suite and POSTs a report to the dev server, which fails on timeout; `cargo xtask e2e` opens `__test/run/<id>/<suite>.html` so reports match runs. Input is synthesized in-page; import tests feed the installer, served with HTTP range requests, through the same import path as picked files. The native file picker and gestures are not covered by automated tests.

## Tooling

All testing is local. `cargo xtask check` runs fast static checks (SPDX, licenses, fmt, clippy) and gates every push. `cargo xtask e2e [chrome] [iphone] [suite...]` runs native tests, builds the web app and runs every browser suite on each target, on demand only; naming suites runs only those (no native tests), e.g. `cargo xtask e2e chrome render`.

| Layer | How | Where |
|---|---|---|
| Native tests | `cargo test` | Mac |
| Browser suites | `tests/browser/<suite>.js`, run by `harness.js`; the dev server serves them under `/volya-engine/__test/`, never in `dist/` | Headless desktop Chrome; Mobile Safari in a dedicated `Volya Engine iPhone 17 Pro Max` simulator (created on first run, left booted) |

Add a browser suite by creating `tests/browser/<suite>.js` exporting `tests` (name → function, may be async), a `<suite>.html` page calling `runPage("<suite>")`, and listing it in `SUITES` in `tools/xtask/src/e2e.rs`. A test may return a string note for the report. Tests report what really happens; failures a target is known to have (the iOS Simulator has no WebGPU adapter: WebKit disables WebGPU on simulators, whose Metal reports GPU family Apple2, below the Apple4 it needs; WebKit bug 285709) are turned into skips by `KNOWN_FAILURES` in `tools/xtask/src/e2e.rs`.

### Shared conformance checks

Behavior every backend must share is written once in Rust and run on each backend: e.g. `vfs::conformance::check` runs natively against `StdVfs` and, via the `vfs_selftest` op, against the OPFS VFS in the engine worker on every browser target.

### Game data

Tests that need real game data (BYOA, never committed) read its location from an environment variable and skip, saying so, when it is unset:

| Variable | Points to |
|---|---|
| `VOLYA_GOG_INSTALLER` | Directory holding the GOG offline installer parts (`.exe` + `-1.bin`) of the original Shadow of Chernobyl, Russian edition |

`cargo xtask e2e` and `volya-engine-devserver` serve `VOLYA_GOG_INSTALLER` to browser tests under `/volya-engine/__test/installer/` (with `index.json` and HTTP range support; never deployed); the `import` suite imports two small archives from it through the same path as picked files, and skips when it is unset. The `audio` suite imports `gamedata.dba`, plays a game sound through the engine's mixer and the AudioWorklet, and renders it with an `OfflineAudioContext` (no user gesture needed). The `render` suite imports `gamedata.dbb` and draws a texture and a video from it with the `debug_draw` op, reading pixels back with `debug_capture` (the renderer, not the DOM). The `menu` suite imports the archives the startup sequence and the title menu read (`gamedata.db5`–`dbd`, about 2.3 GiB) plus the splash's executable, launches the game, skips the logos with synthesized Escape presses and drives the menu with synthesized keys, mouse and touch, checking `menu_state` and `menu_scene` (sprites for a given resolution) and, with WebGPU, `debug_capture`.

Add variables here as new kinds of game data are needed.

Archive tests (`tests/gog.rs` of `volya-engine-formats`, `volya-engine-core`, `volya-engine-render` and `volya-engine` (the startup sequence and title menu, rendered headless; `VOLYA_DUMP=<file.ppm>` writes its menu frame locally for inspection), plus `volya-engine-formats/tests/{video,dds}.rs`) extract the `.db` / `.xdb` archives from the installer once into `target/tmp/gog-ru-RU-archives/` (the game-root layout, ~6 GB; a few minutes) and reuse them while sizes match; delete the directory to re-extract. Shared helper: `crates/volya-engine-formats/tests/support/gog_archives.rs`. Example: `VOLYA_GOG_INSTALLER=~/<downloads folder> cargo test -p volya-engine-formats -p volya-engine-core`.

Dev CLI for archives: `cargo run -p volya-engine-formats --bin volya-engine-db -- list <archive>` or `dump <archive> <stored path>`.

### Real touches in the simulator

`cargo xtask sim STEP...` drives Safari in the test simulator with real touches (`tools/simdriver`, an XCUITest; no window, no extra permissions), for what in-page synthesized input can't reach: system sheets such as the file picker, iOS gestures, the path from the screen to the page. Steps: `open <url>`, `wait <s>`, `tap <x> <y>` (screen points), `tap-text <label>`, `hold <x> <y> <s>`, `drag <x1> <y1> <x2> <y2>`, `expect-text <label>`, `screenshot <path>`. A step that fails stops the run with the reason. Example: `cargo xtask sim "open http://localhost:8080/volya-engine/" "tap-text Play" "wait 3" "screenshot /tmp/s.png"`. XCUITest can't hold independent fingers on different spots at once; multi-finger input stays covered by in-page tests. Files for pickers go in the simulator's "On My iPhone" folder: the `File Provider Storage` directory of the `group.com.apple.FileProvider.LocalStorage` app group in the device's data.

### Setup

- rustup (installs the toolchain from `rust-toolchain.toml`; `xtask` installs the matching `wasm-bindgen` CLI).
- `cargo-deny`.
- Google Chrome.
- Xcode with an iOS simulator runtime.

### Dev server

Mimics GitHub Pages (served under `/volya-engine/`, no custom headers) and serves the browser suites: `cargo xtask web --dev && cargo run -p volya-engine-devserver`, then open `http://localhost:8080/volya-engine/`. Opening `/volya-engine/__test/<suite>.html` runs that suite and prints its report in the server's terminal.
