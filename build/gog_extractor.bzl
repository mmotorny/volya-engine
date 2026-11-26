# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The game data's extractor (build/gog.bzl declares it). The game files'
repositories run its binary but aren't keyed on it: they're keyed on the
extraction format (`EXTRACTION_FORMAT` in tools/gog-extract/src/main.rs)."""

# inno at Cargo.lock's version, with our patch (MODULE.bazel's annotation).
_INNO = "inno"

def _locked(lock, name):
    """The version and checksum of `name` in Cargo.lock."""
    blocks = lock.split("[[package]]\n")
    for block in blocks:
        if block.startswith('name = "{}"\n'.format(name)):
            fields = {}
            for line in block.splitlines():
                if " = " in line:
                    key, value = line.split(" = ", 1)
                    fields[key] = value.strip('"')
            return fields["version"], fields.get("checksum", "")
    fail("Cargo.lock has no " + name)

def _gog_extractor_impl(rctx):
    workspace = rctx.path(Label("//:Cargo.toml")).dirname
    patch = rctx.path(Label("//third_party/inno:expose-file-conditions.patch"))
    rctx.watch(patch)
    for tree in ["tools/gog-extract", "crates/volya-engine-installer"]:
        rctx.watch_tree(workspace.get_child(tree))

    # A workspace of the extractor and the installer crate alone, with
    # Cargo.lock (Cargo may write it here, never in the source tree) and the
    # patched inno.
    lock = rctx.read(workspace.get_child("Cargo.lock"))
    rctx.file("src/Cargo.lock", lock)
    manifest = rctx.read(workspace.get_child("Cargo.toml"))
    start = manifest.find("members = [")
    end = manifest.find("]", start)
    manifest = manifest[:start] + 'members = ["crates/volya-engine-installer", "tools/gog-extract"]' + manifest[end + 1:]
    manifest += '\n[patch.crates-io]\n{} = {{ path = "inno" }}\n'.format(_INNO)
    rctx.file("src/Cargo.toml", manifest)
    for tree in ["tools/gog-extract", "crates/volya-engine-installer"]:
        result = rctx.execute(["mkdir", "-p", "src/" + tree.rsplit("/", 1)[0]])
        result = rctx.execute(["cp", "-R", str(workspace.get_child(tree)), "src/" + tree])
        if result.return_code != 0:
            fail("copying {}: {}".format(tree, result.stderr))
    version, checksum = _locked(lock, _INNO)
    rctx.download_and_extract(
        url = "https://static.crates.io/crates/{0}/{0}-{1}.crate".format(_INNO, version),
        type = "tar.gz",
        sha256 = checksum,
        stripPrefix = "{}-{}".format(_INNO, version),
        output = "src/inno",
    )
    result = rctx.execute(["patch", "-p1", "-d", "src/inno", "-i", str(patch)])
    if result.return_code != 0:
        fail("patching inno: " + result.stdout + result.stderr)

    cargo = rctx.path(Label("@rust_host_tools//:bin/cargo"))
    result = rctx.execute(
        [
            cargo,
            "build",
            "--release",
            "--package",
            "volya-engine-gog-extract",
            "--manifest-path",
            "src/Cargo.toml",
            "--target-dir",
            "target",
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

gog_extractor = repository_rule(
    implementation = _gog_extractor_impl,
    doc = "`tools/gog-extract`, built with the Rust toolchain's cargo.",
)
