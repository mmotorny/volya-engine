# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The game data, from the GOG installer `VOLYA_GOG_INSTALLER` names
(`user.bazelrc`; docs/architecture/build.md, Game data). `tools/bazel`, the
wrapper Bazelisk runs, passes its identity on every command:
`VOLYA_GOG_INSTALLER_EXE_SHA256` (the `.exe`'s SHA-256, 0.9 MB, which
records every game file's MD5) and `VOLYA_GOG_INSTALLER_BIN` (the `.bin`'s
name and size), so replacing the installer re-keys everything below with no
step of its own, and the 3.7 GB `.bin` is never hashed. As repositories:

- `@gog_installer` (the repository rule `gog_installer`): `fingerprint.txt`
  (the parts' names and sizes and the `.exe`'s SHA-256, checked against the
  wrapper's; empty without the variable), `exe.txt` (its path) and
  `defs.bzl`'s `GOG_INSTALLER`.
- `@gog_extractor` (build/gog_extractor.bzl): `tools/gog-extract`, built
  with the Rust toolchain's cargo.
- `@gog_game` and `@gog_original` (build/gog_game.bzl): the game files and
  the original's install folder, reproducible and keyed on the fingerprint
  and the extraction format (`EXTRACTION_FORMAT` in
  tools/gog-extract/src/main.rs), not the extractor's binary, so every
  worktree shares one copy through the repo contents cache.
"""

load(":gog_extractor.bzl", "gog_extractor")
load(":gog_game.bzl", "gog_game", "gog_original")


def _stdout(ctx, args):
    result = ctx.execute(args)
    if result.return_code != 0:
        fail("{}: {}".format(" ".join(args), result.stderr))
    return result.stdout.strip()

def _installer_parts(rctx, dir):
    exes = [
        p
        for p in rctx.path(dir).readdir(watch = "no")
        if p.basename.startswith("setup_s.t.a.l.k.e.r._shadow_of_chernobyl") and
           "(russian)" in p.basename and p.basename.endswith(".exe")
    ]
    if not exes:
        fail("VOLYA_GOG_INSTALLER: no Russian Shadow of Chernobyl installer .exe in " + dir)
    exe = exes[0]
    bin = rctx.path(str(exe)[:-len(".exe")] + "-1.bin")
    if not bin.exists:
        fail("VOLYA_GOG_INSTALLER: {} has no {}".format(dir, bin.basename))
    return exe, bin

def _gog_installer_impl(rctx):
    dir = rctx.getenv("VOLYA_GOG_INSTALLER") or ""

    # From tools/bazel; recorded, so a new installer re-keys this.
    passed_sha = rctx.getenv("VOLYA_GOG_INSTALLER_EXE_SHA256") or ""
    passed_bin = rctx.getenv("VOLYA_GOG_INSTALLER_BIN") or ""
    fingerprint = ""
    exe = ""
    if dir:
        exe_path, bin = _installer_parts(rctx, dir)
        sha = _stdout(rctx, ["shasum", "-a", "256", str(exe_path)]).split(" ")[0]
        exe_size = _stdout(rctx, ["stat", "-f", "%z", str(exe_path)])
        bin_size = _stdout(rctx, ["stat", "-f", "%z", str(bin)])
        bin_entry = "{} {}".format(bin.basename, bin_size)
        if passed_sha and (passed_sha != sha or passed_bin != bin_entry):
            fail("the installer changed during the command (tools/bazel saw {} {}, now {} {}): run it again".format(passed_sha, passed_bin, sha, bin_entry))
        fingerprint = "{} {} {}\n{}\n".format(exe_path.basename, exe_size, sha, bin_entry)
        exe = str(exe_path)
    rctx.file("fingerprint.txt", fingerprint)
    rctx.file("exe.txt", exe)
    rctx.file("defs.bzl", "GOG_INSTALLER = {}\n".format(repr(dir)))
    rctx.file("BUILD.bazel", """\
exports_files(
    ["exe.txt", "fingerprint.txt"],
    visibility = ["//visibility:public"],
)
""")

gog_installer = repository_rule(
    implementation = _gog_installer_impl,
    doc = "The GOG installer's fingerprint (see the module's docstring).",
)

def _extraction_format(mctx):
    """`EXTRACTION_FORMAT` in the extractor's source. Passed as an attribute,
    so any other edit there runs this again and re-keys nothing."""
    prefix = "const EXTRACTION_FORMAT: u32 = "
    for line in mctx.read(Label("//tools/gog-extract:src/main.rs")).splitlines():
        if line.startswith(prefix) and line.endswith(";"):
            return int(line[len(prefix):-1])
    fail("tools/gog-extract/src/main.rs has no `{}N;` line".format(prefix))

def _gog_impl(mctx):
    # Read from @gog_installer, so the extension runs again when it changes.
    fingerprint = mctx.read(Label("@gog_installer//:fingerprint.txt"))
    exe = mctx.read(Label("@gog_installer//:exe.txt"))
    format = _extraction_format(mctx)
    gog_extractor(name = "gog_extractor")
    gog_game(name = "gog_game", exe = exe, fingerprint = fingerprint, format = format)
    gog_original(name = "gog_original", fingerprint = fingerprint, format = format)
    return mctx.extension_metadata(reproducible = False)

gog = module_extension(
    implementation = _gog_impl,
    doc = "The game data's repositories (see the module's docstring).",
)
