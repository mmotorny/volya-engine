# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The game files' repositories (build/gog.bzl declares them). Kept apart
and loading nothing: a repository's key includes the file defining its
rule, so an edit here extracts the game again (6 GB, about 26 s)."""

_BUILD = """\
exports_files(
    ["manifest.tsv"],
    visibility = ["//visibility:public"],
)
"""

def _extractor(rctx):
    """The extractor, its digest read so a changed one re-keys this."""
    rctx.read(Label("@gog_extractor//:digest.txt"))
    return rctx.path(Label("@gog_extractor//:gog-extract"))

def _gog_game_impl(rctx):
    extractor = _extractor(rctx)
    if not rctx.attr.exe:
        rctx.file("manifest.tsv", "")
    else:
        rctx.report_progress("Extracting the game files (6 GB)")
        result = rctx.execute([extractor, rctx.attr.exe, rctx.path(".")], timeout = 3600, quiet = False)
        if result.return_code != 0:
            fail("extracting the game files: " + result.stderr)
    rctx.file("BUILD.bazel", _BUILD)
    return rctx.repo_metadata(reproducible = True)

gog_game = repository_rule(
    implementation = _gog_game_impl,
    doc = "Every `ru-RU` game file in `files/`, and `manifest.tsv` (path, size, MD5).",
    attrs = {
        "exe": attr.string(doc = "The installer's .exe; empty: no game data."),
        "fingerprint": attr.string(doc = "The installer's identity, the repository's key."),
    },
)

def _gog_original_impl(rctx):
    extractor = _extractor(rctx)
    if not rctx.attr.fingerprint:
        rctx.file("manifest.tsv", "")
    else:
        files = rctx.path(Label("@gog_game//:manifest.tsv")).dirname.get_child("files")
        result = rctx.execute([extractor, "install", files, rctx.path(".")], timeout = 600, quiet = False)
        if result.return_code != 0:
            fail("assembling the original's install folder: " + result.stderr)
    rctx.file("BUILD.bazel", _BUILD)
    return rctx.repo_metadata(reproducible = True)

gog_original = repository_rule(
    implementation = _gog_original_impl,
    doc = "The original's install folder in `game/`, and `manifest.tsv`.",
    attrs = {"fingerprint": attr.string(doc = "The installer's identity, the repository's key.")},
)
