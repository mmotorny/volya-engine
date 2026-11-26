# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The game data, from the GOG installer `VOLYA_GOG_INSTALLER` names
(docs/architecture/build.md, Game data), as repositories:

- `@gog_installer`: `fingerprint.txt` (the `.exe`'s SHA-256, which records
  every game file's MD5, and the parts' names and sizes; empty without the
  variable) and `defs.bzl`'s `GOG_INSTALLER` (its directory).
- `@gog_extractor`: `tools/gog-extract`, built with the Rust toolchain's
  cargo.
- `@gog_game`: every `ru-RU` game file in `files/`, and `manifest.tsv`
  (path, size, MD5). Reproducible and keyed on the fingerprint, so every
  worktree shares one copy through the repo contents cache.
"""

def _installer_parts(ctx, dir):
    exes = [
        p
        for p in ctx.path(dir).readdir()
        if p.basename.startswith("setup_s.t.a.l.k.e.r._shadow_of_chernobyl") and
           "(russian)" in p.basename and p.basename.endswith(".exe")
    ]
    if not exes:
        fail("VOLYA_GOG_INSTALLER: no Russian Shadow of Chernobyl installer .exe in " + dir)
    exe = exes[0]
    return exe, ctx.path(str(exe)[:-len(".exe")] + "-1.bin")

def _stdout(ctx, args):
    result = ctx.execute(args)
    if result.return_code != 0:
        fail("{}: {}".format(" ".join(args), result.stderr))
    return result.stdout.strip()

def _size(ctx, path):
    return _stdout(ctx, ["stat", "-f", "%z", str(path)])

# @gog_installer

def _gog_installer_impl(rctx):
    rctx.file("fingerprint.txt", rctx.attr.fingerprint)
    rctx.file("defs.bzl", "GOG_INSTALLER = {}\n".format(repr(rctx.attr.dir)))
    rctx.file("BUILD.bazel", """\
exports_files(
    ["fingerprint.txt"],
    visibility = ["//visibility:public"],
)
""")
    return rctx.repo_metadata(reproducible = True)

_gog_installer = repository_rule(
    implementation = _gog_installer_impl,
    attrs = {
        "dir": attr.string(),
        "fingerprint": attr.string(),
    },
)

# @gog_extractor

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
    rctx.file("BUILD.bazel", """\
exports_files(
    ["gog-extract"],
    visibility = ["//visibility:public"],
)
""")
    return rctx.repo_metadata(reproducible = True)

_gog_extractor = repository_rule(implementation = _gog_extractor_impl)

# @gog_game

def _gog_game_impl(rctx):
    if not rctx.attr.exe:
        rctx.file("manifest.tsv", "")
    else:
        extractor = rctx.path(Label("@gog_extractor//:gog-extract"))
        rctx.report_progress("Extracting the game files (a few minutes)")
        result = rctx.execute([extractor, rctx.attr.exe, rctx.path(".")], timeout = 3600, quiet = False)
        if result.return_code != 0:
            fail("extracting the game files: " + result.stderr)
    rctx.file("BUILD.bazel", """\
exports_files(
    ["manifest.tsv"],
    visibility = ["//visibility:public"],
)
""")
    return rctx.repo_metadata(reproducible = True)

_gog_game = repository_rule(
    implementation = _gog_game_impl,
    attrs = {
        "exe": attr.string(doc = "The installer's .exe; empty: no game data."),
        "fingerprint": attr.string(doc = "The installer's identity, the repository's key."),
    },
)

def _gog_impl(mctx):
    dir = mctx.getenv("VOLYA_GOG_INSTALLER") or ""
    fingerprint = ""
    exe = ""
    if dir:
        exe_path, bin = _installer_parts(mctx, dir)

        # Evaluated again when the variable changes (an extension can't watch
        # files outside the workspace): replacing the installer in the same
        # directory needs `bazel mod deps --lockfile_mode=refresh`.
        sha = _stdout(mctx, ["shasum", "-a", "256", str(exe_path)]).split(" ")[0]
        fingerprint = "{} {} {}\n{} {}\n".format(
            exe_path.basename,
            _size(mctx, exe_path),
            sha,
            bin.basename,
            _size(mctx, bin),
        )
        exe = str(exe_path)
    _gog_installer(name = "gog_installer", dir = dir, fingerprint = fingerprint)
    _gog_extractor(name = "gog_extractor")
    _gog_game(name = "gog_game", exe = exe, fingerprint = fingerprint)
    return mctx.extension_metadata(reproducible = False)

gog = module_extension(
    implementation = _gog_impl,
    doc = "The game data's repositories (see the module's docstring).",
)
