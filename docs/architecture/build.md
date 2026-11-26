# Build

Bazel builds and tests everything (decided by the user): agents work in many fresh worktrees at once, and each paid a cold build and reran tests whose inputs hadn't changed. Bazel gives build outputs and test results shared across worktrees and agents, and tags keep heavy tests out of `bazel test //...` until named.

## Layout

- `MODULE.bazel` (Bzlmod; no `WORKSPACE`), `MODULE.bazel.lock` committed, `.bazelversion` (the Bazel release bazelisk runs; `update` moves it), `.bazelrc`.
- A `BUILD.bazel` in every package (each crate, `platforms/web`, `tests/browser`, each tool). Our rules and macros live in `build/` (`build/*.bzl`, platforms in `build/platforms/`).
- Bazel's own rule sets only, from the Bazel Central Registry: `rules_rust` (with `crate_universe`), `rules_cc`, `apple_support`, `bazel_skylib`, `platforms`. All Apache-2.0.

## Sharing across worktrees

Each worktree is its own Bazel workspace, so it has its own output base and server; what is shared is on disk under `~/Library/Caches/volya-engine/bazel/`:

| What | Where | Flag (`.bazelrc`) |
|---|---|---|
| Action outputs and test results | `disk-cache/` | `--disk_cache`, `--experimental_disk_cache_gc_max_size=50G` |
| Downloads (toolchains, crates) | `repository-cache/` | `--repository_cache` |
| Fetched repositories (the Rust toolchain, crate sources) | `repository-cache/contents/` (Bazel's default under the repository cache) | `--repo_contents_cache`, shared by every workspace instead of extracted into each output base |

- Action keys hold execroot-relative paths, never the worktree's, so a worktree reuses another's outputs; rustc's absolute paths are remapped by rules_rust. Measured: the native build and tests in a copy of the tree at another path took 16 s, every compile a disk-cache hit, and every test result came from the cache.
- Test results come from the disk cache too, so an unchanged test passes from cache in any worktree. Tags that would bar the disk cache (`local`, `no-cache`, `no-remote`, `no-remote-cache`) are not used; tests that need the real machine are `no-sandbox` instead, which keeps caching.
- Servers: `startup --max_idle_secs=900` (a server per worktree holds a JVM, about 1 GB) and `startup --host_jvm_args=-Xmx3g`. Output bases of deleted worktrees are removed by `bazel run //tools/xtask -- gc` (an output base names its workspace; the one whose workspace is gone is expunged).

## Rust

- **Toolchain**: rules_rust's, hermetic, its version pinned in `MODULE.bazel` (`rust.toolchain(versions = [...])`, host and `wasm32-unknown-unknown`, with rustfmt and clippy). `rust-toolchain.toml` goes at the switch.
- **External crates**: `crate.from_cargo` over the workspace's `Cargo.toml` files and `Cargo.lock`, one hub `@crates`; `BUILD` files take external dependencies with `all_crate_deps()`, so they are declared once, in `Cargo.toml`. First-party dependencies are written in `BUILD` files. Cargo.toml and Cargo.lock stay as crate_universe's input.
- **Live at head**: `update` moves the toolchain's version to the latest stable (read from Rust's release channel), runs `cargo update` with the toolchain's cargo (`@rules_rust//tools/upstream_wrapper:cargo`), moves `.bazelversion` and the modules in `MODULE.bazel` to their latest releases, then runs the checks. crate_universe resolves from Cargo.lock on its next fetch, so there is no second lock file to repin.
- **Profiles** become flags: the dev profile's per-crate optimisation (`-Copt-level=3` for the decoders and hashes, `1` for `volya-engine` and `volya-engine-formats`) as rustc flags, through crate_universe annotations for external crates (native builds only: in the app the decoders follow its profile, as Cargo's release profile has them; at 3 they made the release wasm 75 KB larger, measured) and `select` on the compilation mode for ours; `-c opt` is the release profile (`opt-level = "s"`, `volya-engine-formats` at 3), with fat LTO for the app (the site's transition). The Bazel release wasm is 0.25% larger than Cargo's (4,185,876 against 4,175,232 bytes, measured) and differs in its crates' hashes, so the two are compared by their suites, not byte for byte.
- **Features**: the test build's features (`test-ops` → `render-selftest` → `selftest`) are one build setting, `//build:test_ops`, that our crates' `crate_features` select on; the web rule sets it by transition, so the release and test apps are two configurations of the same targets.
- **Patched crates**: the `inno` fork (`[patch.crates-io]`, a git branch) becomes `third_party/inno/expose-file-conditions.patch`, applied to the crates.io release by a crate_universe annotation (`patches`, `patch_args = ["-p1"]`). Upstream's latest release (0.7.6) still discards file conditions (checked against its main branch), so the patch stays until a release exposes them; `update` reports when the patch no longer applies. Cargo then resolves the unpatched release, so Cargo can no longer build the installer (below).

## Build scripts

Both stay as they are, run by `cargo_build_script`:

- **Shaders**: `crates/volya-engine-render/build.rs` translates the WGSL programs to GLSL ES 3.00 with naga into `OUT_DIR/shaders.rs`; its inputs are the `.wgsl` files (`data`), naga a build dependency from `@crates`. Built for the exec platform, so the native and wasm builds share one shader translation.
- **The script runtime's C** (`crates/volya-engine-script/build.rs`, `cc`): natively with the Bazel C++ toolchain (`apple_support`'s, Xcode's clang); for wasm32 with a pinned LLVM that Bazel downloads (decided by the user): an `http_archive` with its checksum, whose `clang`, `llvm-ar` and `llvm-ranlib` the build script gets by path (`VOLYA_LLVM_BIN`, set by the target; without it, as under Cargo, Homebrew's). `update` moves it to the latest release.

## The web app

- **wasm**: `platforms/web` is a `rust_shared_library` built under a transition to `//build/platforms:wasm32` (`@platforms//cpu:wasm32`, `@platforms//os:none`), with `--cfg=web_sys_unstable_apis` (today's `.cargo/config.toml`) for `web-sys` and our crates.
- **wasm-bindgen**: the CLI must equal the `wasm-bindgen` crate in Cargo.lock, and rules_rust's own wasm-bindgen rules pin a version of their own. So the CLI is built from crates.io by crate_universe (`crate.spec` of `wasm-bindgen-cli` at Cargo.lock's version, `gen_binaries`), in a second hub `@tools` that `update` keeps equal to Cargo.lock (today `cargo install` into `.cache/tools`).
- **The site**: a rule `web_site` (`build/web.bzl`) runs, as actions, wasm-bindgen (`--target web --no-typescript`), the engine stamp (the wasm's hash as its custom section and the glue's `BUILD_ID`) and the static files' copy, producing a directory. Then the injection of `update.js` and `sw.js` (the build, its date, the precache list with hashes and sizes).
  - The build's commit is not a Bazel input: the sites tests use are stamped `0000000` with a fixed date, so a commit that changes no input keeps every cached result. `deploy` injects the real commit and date into a copy, outside Bazel (one pass over a few files).
  - Targets: `//platforms/web:site` (release, deployed), `:site_test` (release with the test ops, what suites run), `:site_dev` (unoptimised, with the test ops), `:site_next` (the update flow's next build, from `:site_test`).
- The code shared by xtask and the site rule's tool (stamping, injection, the next build) is a small library crate, `tools/site` (`volya-engine-site`), with the tool as its binary.

## Static checks

Tests in `//:checks` (a `test_suite`), cached like any test. Every package's `BUILD.bazel` ends with `volya_checks()` (`build/checks.bzl`), which makes its `fmt`, `clippy`, `clippy_wasm` and `all_files` from the Rust targets it declares, and the root's `PACKAGES` lists every package: a new package adds both.

| Check | Target |
|---|---|
| Format | `<package>:fmt`, rules_rust's `rustfmt_test` over the package's Rust targets |
| Clippy, native and wasm | `<package>:clippy` and `:clippy_wasm`, `rust_clippy_test` (the second on `//build/platforms:wasm32`), failing on warnings (`clippy_flags=-Dwarnings` in `.bazelrc`) |
| SPDX headers | `//build:spdx`, a Rust test over `//:all_files` (the root's files and every package's) in its runfiles |
| Pins | `//build:pins`: MODULE.bazel's wasm-bindgen CLI equals Cargo.lock's crate |
| Licences | `//build:licenses`: cargo-deny's licences, bans and sources over the workspace in its runfiles, with the toolchain's cargo; `no-sandbox` and `requires-network` (it reads the crates' metadata). cargo-deny is installed on the machine (`@local_tools`, its version an input). Advisories are left to `update`: they change with the day, not with an input. |
| Spec drafts | `bazel run //tools/xtask -- spec-lint <paths>` stays a command: its inputs are drafts outside the repository |

## Native tests

- Each crate's unit tests are a `rust_test`; each integration test file (`tests/*.rs`) is its own `rust_test`, so one file's change reruns one test.
- **Game data** is a declared input, its files held by Bazel (below, Game data); a game-data test takes `@gog_game//:manifest` as `data` and reads the files beside it. Without `VOLYA_GOG_INSTALLER` the manifest is empty and the tests skip, cached as such.
- Tags: `gamedata` on every test that reads it. Tests that walk the whole game or measure (the `--release` cost and memory tests, `visuals.rs`, `collision_rays.rs`, the counting-allocator binaries) are `manual`, with `-c opt` through a `measure` config (`bazel test --config=measure //crates/volya-engine-core:particles_cost`). The rest of the game-data tests run in `bazel test //...` (skipping, cached, without the data).

## Game data

Bazel holds what was hand-kept in `~/Library/Caches/volya-engine/<installer>-<sizes>/` (decided by the user), as repositories in the repo contents cache, so every worktree shares one copy (`build/gog.bzl`). The installer is `VOLYA_GOG_INSTALLER`, set in `user.bazelrc` as `common --repo_env=VOLYA_GOG_INSTALLER=<dir>`; `tools/bazel`, the wrapper Bazelisk runs for every command (AGENTS.md's one shell script), reads it from there or the environment and adds the installer's identity to the commands that fetch: `VOLYA_GOG_INSTALLER_EXE_SHA256` (the `.exe`'s SHA-256: 0.9 MB, it records every game file's MD5; milliseconds) and `VOLYA_GOG_INSTALLER_BIN` (the `.bin`'s name and size). So replacing the installer re-keys the game data with no step of its own, and the 3.7 GB `.bin` is never hashed (measured: 1.6 s with `openssl`, 8.4 s with `shasum`, warm). Without an installer there are no flags and the tests skip, cached as such. An invocation that bypasses the wrapper (`BAZELISK_SKIP_WRAPPER`) sees the same fingerprint, computed again by the repository rule, and keeps the same data (measured).

- **`@gog_installer`** (a repository rule): `fingerprint.txt` (the parts' names and sizes and the `.exe`'s SHA-256, checked against the wrapper's), the `.exe`'s path and the directory (`defs.bzl`'s `GOG_INSTALLER`). A module extension reads the fingerprint and declares the rest, keyed on it.

- **`@gog_game`**: every `ru-RU` game file in the game-root layout under `files/` (what Cargo's tests kept in `archives/` and `splash/`), and `manifest.tsv`: each file's path, size and the MD5 the installer records. The repository is marked reproducible and keyed on the fingerprint; extraction checks each file against its MD5, so a `.bin` that doesn't match its `.exe` fails loudly. Its rule lives alone in `build/gog_game.bzl`, loading nothing: a repository's key includes the file defining its rule, so an edit there extracts the game again (measured: an edit to `gog.bzl` re-extracts nothing). Extracting the 6.0 GB took 26 s (measured), once for the machine: a copy of the tree at another path built and tested in 15 s, extracting nothing. The files are not action inputs (Bazel would hash them for each test's key): a game-data test takes the manifest (`VOLYA_GOG_GAME`, its runfiles path) and opens the files beside its target, read-only, in the sandbox (measured: the sandbox allows it).
  - The extractor is `tools/gog-extract`, a binary over `volya-engine-installer`. A repository can't use Bazel's build, so `@gog_extractor` (reproducible; Cargo.lock, Cargo.toml and both crates' sources watched; `digest.txt`, the binary's SHA-256, which the game files' repositories read, so a changed extractor re-keys them and an edit that leaves the binary as it was doesn't, measured) builds it with rules_rust's host cargo from the crates in Cargo.lock (with the `inno` patch once #541 lands), 11 s (measured).
  - Without the variable: an empty manifest and no files; the tests skip, cached as such.
- **`@gog_original`**: the original's install folder in `game/`: every game file cloned copy-on-write from `@gog_game` (`gog-extract install`), plus what the install adds to its folder (docs/formats/gog-install-additions.md: the generated `fsgame.ltx`, read-only, and the empty `userdocs` folders), with a manifest. Reproducible, keyed like `@gog_game`. Each run of the original clones it and adds the user folder (`user.ltx`, `traffic.net`, the run's `steer.ltx`) and the registry values; `bazel run //tools/xtask -- original` takes it from its runfiles.
- **The original's reference frames**: capture actions (`build/original.bzl`), one run of the original per place (`//tests/fidelity:frames_title`, `:frames_level`, both `manual`), merged into `//tests/fidelity:frames` (`fidelity/<view>.png`), which the `fidelity` suite takes as its references. Their inputs: the view list and settings (`tests/fidelity/views.rs`, compiled into the tool), the procedure (the tool, `//tools/xtask:capture`: xtask's `original`, `wine` and `machine` modules alone, so an edit elsewhere in xtask captures nothing again), `@gog_original`'s manifest and the Wine image's `Containerfile`. The image is tagged with a hash of that file (`wine.rs`), so a changed file builds a new image before a capture uses it. Outputs go to the disk cache: game assets, local only, never committed, published or deployed.
  - Each capture runs `no-sandbox` (Apple's `container` service, run with the user's `HOME` and `PATH`) in the temporary directory: the run's clone of the install, its Wine prefix (box64 and Wine) and its captures are scratch, removed after; only the PNGs are kept. `bazel run //tools/xtask -- original` stays the command for looking at the running game.
- The extraction, locks and cache directory of `crates/volya-engine-formats/tests/support/gog_archives.rs` go at the switch, as Cargo-run tests use them until then.

## Browser suites

One test target per suite and target: `//tests/browser:<suite>_<target>` (e.g. `smoke_chrome`, `render_iphone`; dashes become underscores), made by `browser_suite` (`build/browser.bzl`) from the lists in `tests/browser/BUILD.bazel`. The test runs the xtask binary's `suite TARGET SUITE` (`e2e.rs`), which serves the site and the suites from its runfiles and opens the suite's page, as `cargo xtask e2e` does for each.

- **Inputs**: `:site_test` and `:site_next`, the suites' `.js` and `.html`, the runner, for suites that read the installer `@gog_installer//:fingerprint.txt` (the `.exe`'s SHA-256 and the parts' names and sizes), and the browser: `@local_tools//:chrome_version.txt` (Chrome's `Info.plist`, watched, so a Chrome update reruns the suites) or `:simulator_runtime.txt` (the newest iOS runtime from `simctl`, read again by `bazel fetch --force @local_tools`). Environment that changes outcomes (`VOLYA_DUMP_DIR`, `VOLYA_COMPARE_DIR`, `VOLYA_SIM_MEDIA`) is passed with `--test_env`, part of the key. Suites run with the user's `HOME`: under Bazel's own, Chrome sends no report (measured; why is not known).
- **The installer** is the game data's (Game data, above); `user.bazelrc` is gitignored, and `.bazelrc` imports the main checkout's, which its worktrees (`.claude/worktrees/<name>`) share, then the checkout's own.
- **Tags**:

| Suites | chrome | iphone |
|---|---|---|
| smoke, platform, render, import | default | `manual`, `exclusive` |
| input, audio, game-startup, game-input, game-screens, render-screens, render-level | `manual` | `manual`, `exclusive` |
| game-level, game-memory, draw-calls (they measure), fidelity (Chrome only) | `manual`, `exclusive` | |

  Every suite is `no-sandbox` (the real Chrome and Simulator, profiles outside the execroot) and `resources:browser:1`, with `--local_resources=browser=1`: one browser suite at a time per Bazel invocation, while other tests run beside it. `exclusive` runs a test alone, for the Simulator and for measurements; `browser_suite` fails the build when an iPhone or measuring suite lacks `manual` and `exclusive`, and the groups are made `manual` by `suite_group` (a `test_suite` matched by `//...` runs every test it names, manual ones included: measured, it ran iPhone suites). Groups: `//tests/browser:chrome`, `:iphone`, `:game_chrome` (`test_suite`s, naming manual tests explicitly). Timeouts are `eternal` for the whole-game suites (a report timeout of 1500 s), `long` for those of 240 s, `moderate` otherwise.
- **The kept Chrome profile** is `~/Library/Caches/volya-engine/chrome-profile/` under a file lock (`chrome-profile.lock`; Cargo's `e2e` and `parity` take it too): one import of the game for every worktree (each imported its own, gigabytes, minutes), one Chrome using it at a time across processes. Its port stays in the profile, and only a run holding the lock reads or writes it.
- **Parity** builds another revision: `bazel run //tools/xtask -- parity [REV]` checks REV out into a temporary worktree and builds its `:site_test` there with the shared disk cache (mostly cache hits), then runs the `parity` suite on both. **Wine** (`wine run`, `wine check`) stays a command.

## The memory gate and the Simulator lock

- Heavy tests gate themselves: the runner and game-data tests call the memory gate (at least 20% free) before starting, so Bazel's parallel tests never start a browser into pressure; time waiting counts against an `eternal` timeout.
- Builds: Bazel's `--local_resources=memory=HOST_RAM*.5` (`.bazelrc`) caps what it schedules; before a build or `bazel test` an agent runs `bazel run //tools/xtask -- wait-memory` (cached, instant), as today.
- The Simulator lock (`/tmp/volya-engine-simulator.lock`, held by PID) is taken by each iphone suite's runner; `exclusive` keeps one per invocation, the lock one per machine.

## Commands

Operational tasks are `bazel run //tools/xtask -- <task>`: the xtask binary built by Bazel, its root from `BUILD_WORKSPACE_DIRECTORY`, the paths it is given relative to `BUILD_WORKING_DIRECTORY` (where `bazel run` was called): `deploy`, `update`, `wait-memory`, `sim`, `simshot`, `simrec`, `wine`, `original`, `parity`, `spec-lint`, `gc`. `update` under `bazel run` moves Rust (from its stable channel), Bazel (`.bazelversion`, its latest release), every `bazel_dep` (the registry's latest), wasi-sdk (its latest release, the checksum computed), the crates (`cargo update` through rules_rust's cargo) and the wasm-bindgen CLI's pin (from Cargo.lock), then runs `//:checks` and cargo-deny's advisories. `gc` removes the output bases (`~/Library/Caches/bazel/_bazel_<user>/<hash>/`) whose workspace is gone, leaving one whose server still runs. The dev server is `bazel run //tools/devserver` (serving `:site_dev` and the workspace's `tests/browser`; `--root` and `--tests` as before).

## Cargo after the switch

Cargo no longer builds the workspace: without the fork, `cargo build` gets the unpatched `inno` and the installer (and everything above it) fails to compile, and generating a patched path crate for Cargo would be a second build to keep working. Editors use rust-analyzer with `rust-project.json`, generated by `bazel run @rules_rust//tools/rust_analyzer:gen_rust_project` (gitignored; `update` regenerates it). Cargo stays as crate_universe's resolver: `Cargo.toml` files and `Cargo.lock`. `.cargo/config.toml` goes.

## Migration

Cargo keeps working, and stays what deploys, until the switch. The `inno` patch (#541) lands with the switch: until then Cargo.lock holds the fork, which crate_universe builds as Cargo does. Each step is an issue, landed on main with the checks of its time:

1. Bazel workspace and native build (#540; the `inno` patch #541): `MODULE.bazel`, `.bazelrc` with the shared caches, the toolchain, `@crates`, `BUILD.bazel` for every crate and tool with both build scripts; `bazel build //...` natively.
2. Native tests (#542): unit and integration tests as targets, profiles as flags, the `gamedata`, `manual` and `measure` tags; the game files as `@gog_game`.
3. The wasm build (#543): the wasm32 platform and transition, the pinned LLVM, `web_sys_unstable_apis`, `//build:test_ops`, `@tools` with wasm-bindgen; `tools/site`; the four site targets.
4. Static checks as tests (#544): `//:checks`.
5. Browser suites as test targets (#545): the runner, the macro, `@local_browsers`, the shared kept profile, tags and resources.
6. Operational commands under `bazel run` (#546): xtask's root and paths, `update` and `gc`, the dev server.
   Game data (#553 the game files, #554 the install folder, #555 the reference frames), after step 1, 5 and 6 as each needs.
7. The switch (#547): AGENTS.md ("tasks are Bazel targets"; the iteration rule becomes `bazel test //...` plus named manual targets) and docs/testing.md; `rust-project.json`; the fork's `[patch.crates-io]` and `.cargo/config.toml` removed; xtask's Cargo-driven paths (`check`, `web`, `e2e`) removed. Cold and warm builds measured in a fresh worktree, against the Cargo numbers below.
8. Deploy from Bazel (#548): the Bazel site compared with Cargo's (byte for byte, or the full suites on both targets where it differs), then `deploy` switched and the live site checked.

Before (Cargo, a fresh worktree, 16 cores, measured 2026-11-26): `cargo xtask check` 24 s cold, 2 s warm; `cargo test --workspace --no-run` 29 s cold (after the check), 0.1 s warm; `cargo xtask web` (release, the wasm-bindgen CLI installed first) 45 s cold, 0.6 s warm. Test results were never reused: each run reran every test.
