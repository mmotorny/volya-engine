<p align="center"><img src="docs/logo.svg" alt="Volya Engine logo" width="384"></p>

# Volya Engine

A clean-room, BYOA (bring your own assets) game engine for the original S.T.A.L.K.E.R. series, written in Rust. The first target is S.T.A.L.K.E.R.: Shadow of Chernobyl.

Volya Engine ships no game assets. You need your own legally obtained copy of the game.

Live version: https://mmotorny.github.io/volya-engine/

UX audit (every screen of the app, in each state, on each device): https://mmotorny.github.io/volya-engine/ux-audit/

## Clean room

Volya Engine's knowledge of the original game comes only from:

- the game's own data, as shipped in a legally obtained copy;
- analysis of the original game's shipped executables, for interoperability;
- public specifications of standard formats (DDS, Ogg Vorbis, Ogg Theora).

It does not use the leaked X-Ray engine source code or any project derived from it, such as OpenXRay. Investigators who analyze the game write functional specifications ([docs/formats](docs/formats), [docs/behavior](docs/behavior)); a reviewer admits a specification only if it contains no code, pseudo-code or internal names from the original; implementers write Volya Engine's code from those specifications alone. See [docs/clean-room.md](docs/clean-room.md).

## Platforms

Web (desktop Chrome; Chrome and Safari on iOS, installable as an iPhone Home Screen app), then native iOS and iPadOS, then VR. See [docs/roadmap.md](docs/roadmap.md).

## Running

Open the [live version](https://mmotorny.github.io/volya-engine/) in a supported browser.

To run locally on macOS:

1. Install [rustup](https://rustup.rs); the stable toolchain and the wasm target install automatically on first build.
   Install LLVM's clang (`brew install llvm`): the web build compiles the script runtime's C for wasm with it.
2. Build and serve (the first build also installs the matching `wasm-bindgen` CLI into `.cache/`):

   ```sh
   cargo xtask web --dev
   cargo run -p volya-engine-devserver
   ```

3. Open http://localhost:8080/volya-engine/ in Chrome.

## Development

- `cargo xtask check`: fast static checks (needs [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny)).
- `cargo xtask e2e`: full test suite, including browser tests in headless Chrome and Mobile Safari in the iOS Simulator (needs Google Chrome and Xcode).
- `cargo xtask deploy`: publish to GitHub Pages.
- `cargo xtask update`: move to the latest stable Rust and dependencies.

See [docs/testing.md](docs/testing.md) and [docs/architecture.md](docs/architecture.md).

## License

BSD-3-Clause. The vendored LuaJIT 1.0.3 (`crates/volya-engine-script/vendor/luajit-1.0.3`) and the files derived from it are MIT-licensed; see their notices.

## Trademarks

Volya Engine is not affiliated with or endorsed by GSC Game World. S.T.A.L.K.E.R. is a trademark of its owner and is used here only to describe compatibility.
