# Testing

## Strategy

The game renders to an opaque WebGPU canvas, so DOM-level tools can't see it. Tests go through the engine, not the page.

1. **Native Rust tests carry most of the weight**: formats, VFS, importer, simulation, scripting, save/load. The simulation is single-threaded and deterministic, so tests feed scripted input, step N frames and assert on state, headless.
2. **Rendering** is checked by native offscreen `wgpu` renders compared against reference images with a tolerance. Committed references use only synthetic content; references rendered from game data stay local, like the data itself.
3. **The engine test interface** (#14) lets tests inject input, step/pause frames, query state and read back the framebuffer: a direct API natively, a small JS API messaging the engine worker in the browser.
4. **Browser tests are thin** and cover only platform integration: startup, worker, WebGPU/`OffscreenCanvas`, OPFS, input forwarding, service worker. No WebDriver: a page loaded with `?test=<suite>` runs the suite itself and POSTs results to the dev server, which fails on timeout. Targets are opened directly: `xcrun simctl openurl` into an iPhone 17 Pro Max simulator, headless desktop Chrome, or the real iPhone by hand. Input is synthesized in-page; import tests feed a `File` into the picker's code path. The file picker and native gestures are manual real-device checks. This replaces Playwright and Node (#13, #15).

## Current tooling

All testing is local. `scripts/check.sh` runs fast static checks (SPDX, licenses, fmt, clippy) and gates every push. `scripts/e2e.sh` runs everything below except real-device testing, on demand only. The dev server mimics GitHub Pages (served under `/volya-engine/`, no custom headers): `scripts/build-web.sh && cargo run -p volya-devserver`, then open `http://localhost:8080/volya-engine/`.

| Layer | Tool | Browsers |
|---|---|---|
| Native unit tests | `cargo test` | n/a |
| Wasm unit tests | `wasm-bindgen-test` via chromedriver | headless desktop Chrome |
| Browser tests | Playwright (`tests/e2e/`) | installed desktop Chrome; WebKit emulating iPhone 17 Pro Max |
| Real devices | Safari Web Inspector | iPhone 17 Pro Max: Safari and Chrome |

### Setup

- Rust toolchain from `rust-toolchain.toml`.
- `wasm-bindgen` CLI (provides `wasm-bindgen-test-runner`) matching the `wasm-bindgen` version pinned in `Cargo.toml`.
- `cargo-deny`.
- Google Chrome. A matching chromedriver is downloaded into `.cache/` on first run.
- `npm ci && npx playwright install webkit`.

### Real devices

iOS only exposes OPFS to secure contexts, so serve over HTTPS:

1. Create a local CA and certificate with [mkcert](https://github.com/FiloSottile/mkcert):
   `mkcert -install && mkcert -cert-file .cache/dev.pem -key-file .cache/dev-key.pem <mac-hostname>.local`
2. Install the CA on the device: AirDrop `$(mkcert -CAROOT)/rootCA.pem`, install the profile in Settings, then enable it under Settings > General > About > Certificate Trust Settings.
3. `scripts/build-web.sh && cargo run -p volya-devserver -- --port 8443 --tls-cert .cache/dev.pem --tls-key .cache/dev-key.pem`
4. On the device, open `https://<mac-hostname>.local:8443/volya-engine/`.
5. Debug from the Mac: enable Settings > Apps > Safari > Advanced > Web Inspector on the device, connect it, and use Safari's Develop menu.
