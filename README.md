# Volya Engine

A clean-room, BYOA (bring your own assets) game engine for the original S.T.A.L.K.E.R. series, written in Rust. The first target is S.T.A.L.K.E.R.: Shadow of Chernobyl.

Volya ships no game assets. You need your own legally obtained copy of the game.

Live version: https://mmotorny.github.io/volya-engine/

## Platforms

Web (desktop Chrome; Chrome and Safari on iOS, installable as an iPhone Home Screen app), then native iOS and iPadOS, then VR. See [docs/roadmap.md](docs/roadmap.md).

## Running

Open the [live version](https://mmotorny.github.io/volya-engine/) in a supported browser.

To run locally on macOS:

1. Install [rustup](https://rustup.rs); the pinned toolchain and wasm target install automatically on first build.
2. Build and serve (the first build also installs the matching `wasm-bindgen` CLI into `.cache/`):

   ```sh
   cargo xtask web --dev
   cargo run -p volya-engine-devserver
   ```

3. Open http://localhost:8080/volya-engine/ in Chrome.

To try it on an iPhone, serve over HTTPS as described in [docs/testing.md](docs/testing.md#real-devices).

## Development

- `cargo xtask check`: fast static checks (needs [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny)).
- `cargo xtask e2e`: full test suite, including browser tests in headless Chrome and Mobile Safari in the iOS Simulator (needs Google Chrome and Xcode).
- `cargo xtask deploy`: publish to GitHub Pages.
- `cargo xtask update`: move to the latest stable Rust and dependencies.

See [docs/testing.md](docs/testing.md) and [docs/architecture.md](docs/architecture.md).

## License

BSD-3-Clause.
