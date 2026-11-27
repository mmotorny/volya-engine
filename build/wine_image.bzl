# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The Wine image's parts (docs/architecture/build.md, Game data): Wine and
box64, each built on this machine by one action (`//tools/xtask:image-part`)
with cross toolchains whose compilers run natively (llvm-mingw's LLVM, the
Debian sysroots), into a tarball of what they install, keyed on their
sources, the toolchains and the tool (which holds the build steps), so the
disk cache keeps them across worktrees and restarts. The image
(`tools/xtask/wine/Containerfile`) only adds them."""

load("@rules_cc//cc/common:cc_info.bzl", "CcInfo")

def _ends_with(files, suffix):
    found = [f for f in files if f.path.endswith(suffix)]
    if len(found) != 1:
        fail("expected one file ending in {}, found {}".format(suffix, len(found)))
    return found[0]

def _resources(_os, _inputs):
    # Its build runs a job per core (the tool's `make -j`), so Bazel runs
    # nothing beside it. Memory: about 8 GB measured for Wine.
    return {"cpu": 16, "memory": 8192}

def _part(ctx, part, tools, env):
    out = ctx.actions.declare_file(ctx.label.name + ".tar.gz")
    llvm = ctx.file._llvm_clang
    sysroot = ctx.file.sysroot
    env = dict(env)
    env.update({
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
        "VOLYA_CLANG": llvm.path,
        "VOLYA_MAKE": ctx.executable._make.path,
        # The sysroot's root is the directory `unpacked/` beside this file.
        "VOLYA_SYSROOT": sysroot.path,
    })
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = [part, out.path] + [f.path for f in ctx.files.srcs],
        inputs = depset(
            ctx.files.srcs + ctx.files._llvm + ctx.files.sysroot_files + [sysroot],
            transitive = [depset(tools)],
        ),
        tools = [ctx.attr._make[DefaultInfo].files_to_run],
        outputs = [out],
        env = env,
        mnemonic = "WineImagePart",
        progress_message = "Building %{label} with the cross toolchains (Wine: minutes)",
        resource_set = _resources,
    )
    return [DefaultInfo(files = depset([out]))]

_COMMON = {
    "srcs": attr.label_list(allow_files = True, mandatory = True),
    "sysroot": attr.label(allow_single_file = True, mandatory = True, doc = "The sysroot's `sysroot` file."),
    "sysroot_files": attr.label(mandatory = True, doc = "The sysroot's files."),
    "_llvm": attr.label(default = "@llvm_mingw//:files"),
    "_llvm_clang": attr.label(default = "@llvm_mingw//:bin/clang", allow_single_file = True),
    "_make": attr.label(default = "@make//:make", executable = True, cfg = "exec"),
    "_tool": attr.label(default = "//tools/xtask:image-part", executable = True, cfg = "exec"),
}

def _wine_part_impl(ctx):
    bison_data = _ends_with(ctx.files._bison_data, "/data/m4sugar/m4sugar.m4")
    headers = ctx.attr._freetype[CcInfo].compilation_context.headers.to_list()
    freetype_header = _ends_with(headers, "/include/ft2build.h")
    env = {
        "BISON_PKGDATADIR": bison_data.dirname.rsplit("/", 1)[0],
        "M4": ctx.executable._m4.path,
        "VOLYA_BISON": ctx.executable._bison.path,
        "VOLYA_FLEX": ctx.executable._flex.path,
        "VOLYA_HOST_FREETYPE": ctx.file._host_freetype.path,
        "VOLYA_HOST_FREETYPE_INCLUDE": freetype_header.dirname,
    }
    tools = ctx.files._bison_data + headers + [
        ctx.file._host_freetype,
        ctx.executable._bison,
        ctx.executable._flex,
        ctx.executable._m4,
    ]
    return _part(ctx, "wine", tools, env)

wine_part = rule(
    implementation = _wine_part_impl,
    doc = "Wine's part of the image, `<name>.tar.gz`, built by `image-part wine`.",
    attrs = dict(_COMMON, **{
        "_bison": attr.label(default = "@bison//:bison", executable = True, cfg = "exec"),
        "_bison_data": attr.label(default = "@bison//:bison_pkgdata", cfg = "exec"),
        "_flex": attr.label(default = "@flex//:flex", executable = True, cfg = "exec"),
        "_freetype": attr.label(default = "@freetype//:freetype", providers = [CcInfo], cfg = "exec"),
        "_host_freetype": attr.label(default = "//tools/xtask:host-freetype", allow_single_file = True, cfg = "exec"),
        "_m4": attr.label(default = "@bison//:m4", executable = True, cfg = "exec"),
    }),
)

def _box64_part_impl(ctx):
    python = _ends_with(ctx.files._python, "/bin/python3")
    cmake = ctx.file._cmake
    env = {
        "VOLYA_CMAKE": cmake.path,
        "VOLYA_PYTHON": python.path,
    }
    return _part(ctx, "box64", ctx.files._python + ctx.files._cmake_files + [cmake], env)

box64_part = rule(
    implementation = _box64_part_impl,
    doc = "box64's part of the image, `<name>.tar.gz`, built by `image-part box64`.",
    attrs = dict(_COMMON, **{
        "_cmake": attr.label(default = "@cmake//:CMake.app/Contents/bin/cmake", allow_single_file = True),
        "_cmake_files": attr.label(default = "@cmake//:files"),
        "_python": attr.label(default = "@python_3_13//:files"),
    }),
)

# What a container action of the image takes (the capture's, the D3D9
# probes'): the Containerfile and the parts, which tag the image (wine.rs),
# so a changed image runs the action again.
WINE_IMAGE = {
    "_box64_part": attr.label(default = "//tools/xtask:box64-part", allow_single_file = True),
    "_containerfile": attr.label(default = "//tools/xtask:wine/Containerfile", allow_single_file = True),
    "_wine_part": attr.label(default = "//tools/xtask:wine-part", allow_single_file = True),
}

def wine_image(ctx):
    """The image's inputs and environment for an action with `WINE_IMAGE`'s attributes.

    Args:
      ctx: the rule's context.

    Returns:
      (inputs, env): the files, and the variables naming the parts.
    """
    inputs = [ctx.file._containerfile, ctx.file._wine_part, ctx.file._box64_part]
    env = {
        "VOLYA_BOX64_PART": ctx.file._box64_part.path,
        "VOLYA_WINE_PART": ctx.file._wine_part.path,
    }
    return inputs, env
