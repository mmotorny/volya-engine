# Testing

## Strategy

The game renders to an opaque WebGPU canvas, so DOM-level tools can't see it. Tests go through the engine, not the page.

1. **Native Rust tests carry most of the weight**: formats, VFS, importer, simulation, scripting, save/load. The simulation is single-threaded and deterministic, so tests feed scripted input, step N frames and assert on state, headless.
2. **Rendering** is checked by native offscreen `wgpu` renders (Metal on the Mac) of synthetic textures, read back and compared pixel by pixel with a tolerance (`crates/volya-engine-render/tests/render.rs`). Committed references use only synthetic content; references rendered from game data stay local, like the data itself.
3. **The engine test interface** (#14) lets tests inject input, step/pause frames, query state and read back the framebuffer: a direct API natively (`volya_engine::test_api`), and in the browser `await volya.call(op)` (plus `volya.status()`), which messages the engine worker and resolves with the op's JSON result. Ops live in `call` in `platforms/web/src/lib.rs`.
4. **Browser tests are thin** and cover only platform integration: startup, worker, WebGPU/`OffscreenCanvas`, OPFS, input forwarding, service worker. No WebDriver: each suite has a page, `tests/browser/<suite>.html`, served by the dev server at `/volya-engine/__test/<suite>.html` (never deployed). It starts the unchanged app, runs the suite and POSTs a report to the dev server, which fails on timeout; `cargo xtask e2e` opens `__test/run/<id>/<suite>.html` so reports match runs. Input is synthesized in-page; import tests feed a `File` into the picker's code path. The file picker and native gestures are manual real-device checks.

## Tooling

All testing is local. `cargo xtask check` runs fast static checks (SPDX, licenses, fmt, clippy) and gates every push. `cargo xtask e2e [chrome] [iphone]` runs native tests, builds the web app and runs every browser suite on each target, on demand only.

| Layer | How | Where |
|---|---|---|
| Native tests | `cargo test` | Mac |
| Browser suites | `tests/browser/<suite>.js`, run by `harness.js`; the dev server serves them under `/volya-engine/__test/`, never in `dist/` | Headless desktop Chrome; Mobile Safari in a dedicated `Volya iPhone 17 Pro Max` simulator (created on first run, left booted) |

Add a browser suite by creating `tests/browser/<suite>.js` exporting `tests` (name → function, may be async), a `<suite>.html` page calling `runPage("<suite>")`, and listing it in `SUITES` in `tools/xtask/src/e2e.rs`. A test may return a string note for the report. Tests report what really happens; failures a target is known to have (the iOS Simulator has no WebGPU adapter) are turned into skips by `KNOWN_FAILURES` in `tools/xtask/src/e2e.rs`.

### Shared conformance checks

Behavior every backend must share is written once in Rust and run on each backend: e.g. `vfs::conformance::check` runs natively against `StdVfs` and, via the `vfs_selftest` op, against the OPFS VFS in the engine worker on every browser target.

### Game data

Tests that need real game data (BYOA, never committed) read its location from an environment variable and skip, saying so, when it is unset:

| Variable | Points to |
|---|---|
| `VOLYA_GOG_INSTALLER` | Directory holding the GOG offline installer parts (`.exe` + `-1.bin`) of the original Shadow of Chernobyl, Russian edition |

`cargo xtask e2e` and `volya-engine-devserver` serve `VOLYA_GOG_INSTALLER` to browser tests under `/volya-engine/__test/installer/` (with `index.json` and HTTP range support; never deployed); the `import` suite imports two small archives from it through the same path as picked files, and skips when it is unset. The `audio` suite imports `gamedata.dba`, plays a game sound through the engine's mixer and the AudioWorklet, and renders it with an `OfflineAudioContext` (no user gesture needed). The `render` suite imports `gamedata.dbb` and draws a texture and a video from it with the `debug_draw` op, reading pixels back with `debug_capture` (the renderer, not the DOM).

Add variables here as new kinds of game data are needed.

Archive tests (`tests/gog.rs` of `volya-engine-formats`, `volya-engine-core` and `volya-engine-render`, plus `volya-engine-formats/tests/{video,dds}.rs`) extract the `.db` / `.xdb` archives from the installer once into `target/tmp/gog-ru-RU-archives/` (the game-root layout, ~6 GB; a few minutes) and reuse them while sizes match; delete the directory to re-extract. Shared helper: `crates/volya-engine-formats/tests/support/gog_archives.rs`. Example: `VOLYA_GOG_INSTALLER=~/<downloads folder> cargo test -p volya-engine-formats -p volya-engine-core`.

Dev CLI for archives: `cargo run -p volya-engine-formats --bin volya-engine-db -- list <archive>` or `dump <archive> <stored path>`.

### Setup

- rustup (installs the toolchain from `rust-toolchain.toml`; `xtask` installs the matching `wasm-bindgen` CLI).
- `cargo-deny`.
- Google Chrome.
- Xcode with an iOS simulator runtime.

### Dev server

Mimics GitHub Pages (served under `/volya-engine/`, no custom headers) and serves the browser suites: `cargo xtask web --dev && cargo run -p volya-engine-devserver`, then open `http://localhost:8080/volya-engine/`. Opening `/volya-engine/__test/<suite>.html` runs that suite and prints its report in the server's terminal.
