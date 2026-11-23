# Volya Engine

A clean-room, BYOA (bring your own assets) game engine for the original S.T.A.L.K.E.R. series, written in Rust. The first target is S.T.A.L.K.E.R.: Shadow of Chernobyl.

Volya ships no game assets. You need your own legally obtained copy of the game.

Live version: https://mmotorny.github.io/volya-engine/

## Platforms

1. Web: desktop Chrome; Chrome and Safari on iOS
2. iOS and iPadOS
3. VR

## Running

Open the [live version](https://mmotorny.github.io/volya-engine/) in a supported browser. Early days: it currently starts the engine and shows its version; importing your game comes next.

To run locally on macOS:

1. Install [rustup](https://rustup.rs); the pinned toolchain and wasm target install automatically on first build.
2. Install the `wasm-bindgen` CLI at the version pinned in `Cargo.toml`: `cargo install wasm-bindgen-cli --version 0.2.130`.
3. Build and serve:

   ```sh
   cargo xtask web --dev
   cargo run -p volya-engine-devserver
   ```

4. Open http://localhost:8080/volya-engine/ in Chrome.

To try it on an iPhone, serve over HTTPS as described in [docs/testing.md](docs/testing.md#real-devices).

## Development

- `cargo xtask check`: fast static checks (needs [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny)).
- `cargo xtask e2e`: full test suite, including browser tests in headless Chrome and Mobile Safari in the iOS Simulator (needs Google Chrome and Xcode).
- `cargo xtask deploy`: publish to GitHub Pages.

See [docs/testing.md](docs/testing.md) and [docs/architecture.md](docs/architecture.md).

## License

BSD-3-Clause.
