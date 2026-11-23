# Testing

## Strategy

The game renders to an opaque WebGPU canvas, so DOM-level tools can't see it. Tests go through the engine, not the page.

1. **Native Rust tests carry most of the weight**: formats, VFS, importer, simulation, scripting, save/load. The simulation is single-threaded and deterministic, so tests feed scripted input, step N frames and assert on state, headless.
2. **Rendering** is checked by native offscreen `wgpu` renders compared against reference images with a tolerance. Committed references use only synthetic content; references rendered from game data stay local, like the data itself.
3. **The engine test interface** (#14) lets tests inject input, step/pause frames, query state and read back the framebuffer: a direct API natively, a small JS API messaging the engine worker in the browser.
4. **Browser tests are thin** and cover only platform integration: startup, worker, WebGPU/`OffscreenCanvas`, OPFS, input forwarding, service worker. No WebDriver: a page loaded with `?test=<suite>&run=<id>` runs the suite itself (`tests/browser/`) and POSTs a report to the dev server, which fails on timeout. Input is synthesized in-page; import tests feed a `File` into the picker's code path. The file picker and native gestures are manual real-device checks.

## Tooling

All testing is local. `cargo xtask check` runs fast static checks (SPDX, licenses, fmt, clippy) and gates every push. `cargo xtask e2e [chrome] [iphone]` runs native tests, builds the web app and runs every browser suite on each target, on demand only.

| Layer | How | Where |
|---|---|---|
| Native tests | `cargo test` | Mac |
| Browser suites | `tests/browser/<suite>.js`, run by `harness.js`; the dev server serves them under `/volya-engine/__test/`, never in `dist/` | Headless desktop Chrome; Mobile Safari in a dedicated `Volya iPhone 17 Pro Max` simulator (created on first run, left booted) |
| Real devices | Same suites, opened by hand; Safari Web Inspector for debugging | iPhone 17 Pro Max: Safari and Chrome |

Add a browser suite by creating `tests/browser/<suite>.js` exporting `tests` (name → function, may be async) and listing it in `SUITES` in `tools/xtask/src/e2e.rs`.

### Setup

- Rust toolchain from `rust-toolchain.toml`.
- `wasm-bindgen` CLI matching the `wasm-bindgen` version pinned in `Cargo.toml`.
- `cargo-deny`.
- Google Chrome.
- Xcode with an iOS simulator runtime.

### Dev server

Mimics GitHub Pages (served under `/volya-engine/`, no custom headers) and serves the browser suites: `cargo xtask web --dev && cargo run -p volya-devserver`, then open `http://localhost:8080/volya-engine/`. Opening `?test=<suite>` prints that suite's report in the server's terminal.

### Real devices

iOS only exposes OPFS to secure contexts, so serve over HTTPS:

1. Create a local CA and certificate with [mkcert](https://github.com/FiloSottile/mkcert):
   `mkcert -install && mkcert -cert-file .cache/dev.pem -key-file .cache/dev-key.pem <mac-hostname>.local`
2. Install the CA on the device: AirDrop `$(mkcert -CAROOT)/rootCA.pem`, install the profile in Settings, then enable it under Settings > General > About > Certificate Trust Settings.
3. `cargo xtask web --dev && cargo run -p volya-devserver -- --port 8443 --tls-cert .cache/dev.pem --tls-key .cache/dev-key.pem`
4. On the device, open `https://<mac-hostname>.local:8443/volya-engine/` (append `?test=smoke` to run a suite; the report prints in the server's terminal).
5. Debug from the Mac: enable Settings > Apps > Safari > Advanced > Web Inspector on the device, connect it, and use Safari's Develop menu.
