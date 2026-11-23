# Testing

All testing is local. `scripts/check.sh` runs everything below except real-device testing.

| Layer | Tool | Browsers |
|---|---|---|
| Native unit tests | `cargo test` | n/a |
| Wasm unit tests | `wasm-bindgen-test` via chromedriver | headless desktop Chrome |
| Browser tests | Playwright (`tests/e2e/`) | installed desktop Chrome; WebKit emulating iPhone 17 Pro Max |
| Real devices | Safari Web Inspector | iPhone 17 Pro Max: Safari and Chrome |

## Setup

- Rust toolchain from `rust-toolchain.toml`.
- `wasm-bindgen` CLI (provides `wasm-bindgen-test-runner`) matching the `wasm-bindgen` version pinned in `Cargo.toml`.
- `cargo-deny`.
- Google Chrome. A matching chromedriver is downloaded into `.cache/` on first run.
- `npm ci && npx playwright install webkit`.

## Real devices

iOS only exposes OPFS and cross-origin isolation to secure contexts, so serve over HTTPS:

1. Create a local CA and certificate with [mkcert](https://github.com/FiloSottile/mkcert):
   `mkcert -install && mkcert -cert-file .cache/dev.pem -key-file .cache/dev-key.pem <mac-hostname>.local`
2. Install the CA on the device: AirDrop `$(mkcert -CAROOT)/rootCA.pem`, install the profile in Settings, then enable it under Settings > General > About > Certificate Trust Settings.
3. `scripts/build-web.sh && TLS_CERT=.cache/dev.pem TLS_KEY=.cache/dev-key.pem PORT=8443 node scripts/serve.mjs`
4. On the device, open `https://<mac-hostname>.local:8443/`.
5. Debug from the Mac: enable Settings > Apps > Safari > Advanced > Web Inspector on the device, connect it, and use Safari's Develop menu.
