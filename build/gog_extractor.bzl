# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The game data's extractor (build/gog.bzl declares it). Kept apart and
loading nothing: a repository's key includes the file defining its rule,
and the game files' repositories take this one's binary."""

def _gog_extractor_impl(rctx):
    workspace = rctx.path(Label("//:Cargo.toml")).dirname
    for watched in ["Cargo.lock", "Cargo.toml"]:
        rctx.watch(workspace.get_child(watched))
    for tree in ["tools/gog-extract", "crates/volya-engine-installer"]:
        rctx.watch_tree(workspace.get_child(tree))
    cargo = rctx.path(Label("@rust_host_tools//:bin/cargo"))
    result = rctx.execute(
        [
            cargo,
            "build",
            "--locked",
            "--release",
            "--package",
            "volya-engine-gog-extract",
            "--manifest-path",
            workspace.get_child("Cargo.toml"),
            "--target-dir",
            rctx.path("target"),
        ],
        environment = {"RUSTC": str(rctx.path(Label("@rust_host_tools//:bin/rustc")))},
        timeout = 1800,
        quiet = False,
    )
    if result.return_code != 0:
        fail("building gog-extract: " + result.stderr)
    rctx.symlink("target/release/gog-extract", "gog-extract")

    # The binary's digest, which the game files' repositories read: an
    # extractor that changed re-keys them.
    result = rctx.execute(["shasum", "-a", "256", "target/release/gog-extract"])
    if result.return_code != 0:
        fail("hashing gog-extract: " + result.stderr)
    rctx.file("digest.txt", result.stdout.split(" ")[0] + "\n")
    rctx.file("BUILD.bazel", """\
exports_files(
    ["digest.txt", "gog-extract"],
    visibility = ["//visibility:public"],
)
""")
    return rctx.repo_metadata(reproducible = True)

gog_extractor = repository_rule(
    implementation = _gog_extractor_impl,
    doc = "`tools/gog-extract`, built with the Rust toolchain's cargo.",
)
