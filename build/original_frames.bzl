# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The original's reference frames (docs/testing.md, Fidelity): the files
in this machine's `~/Library/Caches/volya-engine/original-frames/fidelity/`
as `@original_frames//:frames` (`fidelity/<view>.png`, `<view>.user.ltx`,
`<view>.saved.ltx`), watched, so new frames rerun the fidelity suite. Game
assets: local only, never committed, published or deployed. No folder:
no frames."""

def _files(dir):
    """Every file below `dir`, with its path there (Starlark has no recursion)."""
    found = []
    pending = [(dir, "")]
    for _ in range(1000):
        if not pending:
            break
        folder, prefix = pending.pop()
        for entry in folder.readdir():
            name = prefix + entry.basename
            if entry.is_dir:
                pending.append((entry, name + "/"))
            else:
                found.append((entry, name))
    return found

def _original_frames_impl(rctx):
    home = rctx.getenv("HOME")
    dir = rctx.path(home + "/Library/Caches/volya-engine/original-frames/fidelity")
    rctx.watch(dir)
    if dir.exists:
        rctx.watch_tree(dir)
        for path, name in _files(dir):
            rctx.symlink(path, "fidelity/" + name)
    rctx.file("BUILD.bazel", """\
filegroup(
    name = "frames",
    srcs = glob(["fidelity/**"], allow_empty = True),
    visibility = ["//visibility:public"],
)
""")

original_frames = repository_rule(
    implementation = _original_frames_impl,
    doc = "The original's frames of the fidelity suite's views, from the local cache.",
)
