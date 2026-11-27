# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The original's outputs a suite compares with (docs/testing.md, The
original game as a black box): the files in one folder of this machine's
`~/Library/Caches/volya-engine/` as `@<name>//:files` (also `:frames`), each
at `<suite>/<path>`, watched, so new files rerun the suite. `original_frames`
is the fidelity suite's frames (`original-frames/fidelity/`: `<view>.png`,
`<view>.user.ltx`, `<view>.saved.ltx`), `original_audio` the audio-fidelity
suite's captures (`original-audio/audio-fidelity/`: `<walk>.wav`,
`<walk>.frames.txt`). Game assets: local only, never committed, published or
deployed. No folder: no files."""

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

def _impl(rctx):
    home = rctx.getenv("HOME")
    suite = rctx.attr.suite
    dir = rctx.path("{}/Library/Caches/volya-engine/{}/{}".format(home, rctx.attr.folder, suite))
    rctx.watch(dir)
    if dir.exists:
        rctx.watch_tree(dir)
        for path, name in _files(dir):
            rctx.symlink(path, suite + "/" + name)
    rctx.file("BUILD.bazel", """\
filegroup(
    name = "files",
    srcs = glob(["{suite}/**"], allow_empty = True),
    visibility = ["//visibility:public"],
)

alias(
    name = "frames",
    actual = ":files",
    visibility = ["//visibility:public"],
)
""".format(suite = suite))

_ATTRS = {
    "folder": attr.string(mandatory = True, doc = "The folder below the cache."),
    "suite": attr.string(mandatory = True, doc = "The suite's folder below it, and the files' prefix."),
}

original_files = repository_rule(
    implementation = _impl,
    attrs = _ATTRS,
    doc = "The original's outputs a suite compares with, from the local cache.",
)
