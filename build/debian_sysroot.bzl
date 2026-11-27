# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""A Debian sysroot (docs/architecture/build.md, The Wine image's parts):
the headers and libraries of `build/debian_sysroots.lock`'s packages for one
architecture, unpacked from their `.deb`s, that a cross clang compiles and
links against with `--sysroot`. Only `usr/include` and `usr/lib*` are kept.
Debian's merged `/usr` leaves linker scripts naming `/lib/...`; they are
rewritten to `/usr/lib/...`, so the tree needs no directory links, and links
leaving the tree are made relative or dropped."""

_LOCK = Label("//build:debian_sysroots.lock")

_ARCHIVE = "https://deb.debian.org/debian/"

_SNAPSHOT = "https://snapshot.debian.org/archive/debian/{}/"

def _lock(rctx):
    snapshot = None
    debs = []
    for line in rctx.read(_LOCK).splitlines():
        fields = [f for f in line.split(" ") if f]
        if len(fields) == 2 and fields[0] == "snapshot":
            snapshot = fields[1]
        elif len(fields) == 4 and fields[0] == rctx.attr.arch:
            if fields[2] == "-":
                fail("debian_sysroots.lock: {} has no file; run `xtask update`".format(fields[1]))
            debs.append(struct(name = fields[1], file = fields[2], sha256 = fields[3]))
    if not debs:
        fail("debian_sysroots.lock: no {} packages".format(rctx.attr.arch))
    return snapshot, debs

def _debian_sysroot_impl(rctx):
    snapshot, debs = _lock(rctx)
    for deb in debs:
        urls = [_ARCHIVE + deb.file]
        if snapshot and snapshot != "-":
            urls.append(_SNAPSHOT.format(snapshot) + deb.file)
        unpacked = "debs/" + deb.name
        rctx.download_and_extract(urls, output = unpacked, sha256 = deb.sha256, type = "deb")
        data = [f for f in rctx.path(unpacked).readdir() if f.basename.startswith("data.tar")]
        if len(data) != 1:
            fail("{}: no data.tar".format(deb.name))
        rctx.extract(data[0], output = "unpacked")
    rctx.delete("debs")

    root = rctx.path("unpacked")
    for top in root.readdir():
        if top.basename != "usr":
            rctx.delete(top)
    for sub in rctx.path("unpacked/usr").readdir():
        if not (sub.basename == "include" or sub.basename.startswith("lib")):
            rctx.delete(sub)

    # Links: absolute ones pointed into the tree; links to nothing dropped
    # (globs fail on them).
    links = rctx.execute(["find", "usr", "-type", "l", "-exec", "stat", "-f", "%N\t%Y", "{}", "+"], working_directory = "unpacked")
    if links.return_code != 0:
        fail("listing the sysroot's links: " + links.stderr)
    for line in links.stdout.splitlines():
        path, target = line.split("\t")
        link = "unpacked/" + path
        if target.startswith("/"):
            for merged in ["/lib/", "/lib64/"]:
                if target.startswith(merged):
                    target = "/usr" + target
            rctx.delete(link)
            rctx.symlink("unpacked" + target, link)
        if not rctx.path(link).exists:
            rctx.delete(link)

    # Linker scripts (small `.so` files of text) naming `/lib/...`.
    scripts = rctx.execute(["grep", "-rlI", "--include=*.so", "GNU ld script", "usr"], working_directory = "unpacked")
    for path in scripts.stdout.splitlines():
        text = rctx.read("unpacked/" + path)
        for prefix in [" /lib/", "(/lib/", " /lib64/", "(/lib64/"]:
            text = text.replace(prefix, prefix.replace("/lib", "/usr/lib", 1))
        rctx.file("unpacked/" + path, text, legacy_utf8 = False)

    rctx.file("BUILD.bazel", """\
# The sysroot's files; `sysroot` is its root, as `--sysroot` takes it.
filegroup(
    name = "files",
    srcs = glob(["unpacked/**"]),
    visibility = ["//visibility:public"],
)

exports_files(["sysroot"], visibility = ["//visibility:public"])
""")
    rctx.file("sysroot", "The sysroot is unpacked/, next to this file.\n")

debian_sysroot = repository_rule(
    implementation = _debian_sysroot_impl,
    doc = "A Debian sysroot of `build/debian_sysroots.lock`'s packages for `arch`.",
    attrs = {
        "arch": attr.string(mandatory = True, doc = "Debian's name of the architecture: amd64, arm64."),
    },
)
