# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The game data, from the GOG installer `VOLYA_GOG_INSTALLER` names
(docs/architecture/build.md, Game data). `@gog_installer`: the installer's
fingerprint (`fingerprint.txt`: its `.exe`'s SHA-256, which records every
game file's MD5, and the parts' names and sizes; empty without the
variable) and its directory (`defs.bzl`'s `GOG_INSTALLER`)."""

def _installer_parts(rctx, dir):
    exes = [
        p
        for p in rctx.path(dir).readdir()
        if p.basename.startswith("setup_s.t.a.l.k.e.r._shadow_of_chernobyl") and
           "(russian)" in p.basename and p.basename.endswith(".exe")
    ]
    if not exes:
        fail("VOLYA_GOG_INSTALLER: no Russian Shadow of Chernobyl installer .exe in " + dir)
    exe = exes[0]
    bin = rctx.path(str(exe)[:-len(".exe")] + "-1.bin")
    return exe, bin

def _size(rctx, path):
    result = rctx.execute(["stat", "-f", "%z", str(path)])
    if result.return_code != 0:
        fail("{}: {}".format(path, result.stderr))
    return result.stdout.strip()

def _gog_installer_impl(rctx):
    dir = rctx.getenv("VOLYA_GOG_INSTALLER") or ""
    fingerprint = ""
    if dir:
        exe, bin = _installer_parts(rctx, dir)

        # 0.9 MB: watched by content, so a new installer re-evaluates this.
        rctx.watch(exe)
        sha = rctx.execute(["shasum", "-a", "256", str(exe)]).stdout.split(" ")[0]
        fingerprint = "{} {} {}\n{} {}\n".format(exe.basename, _size(rctx, exe), sha, bin.basename, _size(rctx, bin))
    rctx.file("fingerprint.txt", fingerprint)
    rctx.file("defs.bzl", "GOG_INSTALLER = {}\n".format(repr(dir)))
    rctx.file("BUILD.bazel", """\
exports_files(
    ["fingerprint.txt"],
    visibility = ["//visibility:public"],
)
""")

gog_installer = repository_rule(
    implementation = _gog_installer_impl,
    doc = "The GOG installer's fingerprint and directory.",
)
