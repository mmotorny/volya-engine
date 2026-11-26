# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""Tools installed on the machine, not by Bazel, as declared inputs: each
one's version in a file that tests take as data, so a new version reruns
them (docs/architecture/build.md)."""

def _version(rctx, path, args):
    result = rctx.execute([path] + args)
    if result.return_code != 0:
        return "missing"
    return (result.stdout or result.stderr).strip()

def _local_tools_impl(rctx):
    deny = rctx.which("cargo-deny")
    if deny:
        rctx.watch(deny)
        rctx.symlink(deny, "cargo-deny")
    else:
        # The check fails, saying it's missing (docs/testing.md, Setup).
        rctx.symlink("/usr/bin/false", "cargo-deny")
    rctx.file("cargo_deny_version.txt", (_version(rctx, deny, ["--version"]) if deny else "missing") + "\n")
    rctx.file("BUILD.bazel", """\
exports_files(
    ["cargo-deny", "cargo_deny_version.txt"],
    visibility = ["//visibility:public"],
)
""")

local_tools = repository_rule(
    implementation = _local_tools_impl,
    doc = "cargo-deny from PATH, with its version.",
    environ = ["PATH"],
)
