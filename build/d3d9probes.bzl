# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The Direct3D 9 probes (tests/d3d9probes/, docs/testing.md) as actions:
built with llvm-mingw into a 32-bit Windows executable, packed into the
Windows runner zip, and real hardware's results gathered as the
`d3d9probes` suite's references."""

_TOOL = attr.label(default = "//tools/xtask:d3d9probes-action", executable = True, cfg = "exec")

def _exe_impl(ctx):
    out = ctx.actions.declare_file(ctx.attr.out)
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = [ctx.attr.action, ctx.file._clang.path, out.path] + [f.path for f in ctx.files.srcs],
        inputs = ctx.files.srcs + ctx.files._llvm_mingw,
        outputs = [out],
        mnemonic = "D3d9ProbesBuild",
        progress_message = "Building a Windows runner with llvm-mingw: %{label}",
    )
    return [DefaultInfo(files = depset([out]))]

d3d9probes_exe = rule(
    implementation = _exe_impl,
    doc = "The probes (or, with `action = \"build-odeharness\"`, the physics " +
          "library comparison) compiled into one 32-bit Windows executable.",
    attrs = {
        "action": attr.string(default = "build", values = ["build", "build-odeharness"]),
        "out": attr.string(mandatory = True),
        "srcs": attr.label_list(allow_files = [".c", ".h"], mandatory = True),
        "_clang": attr.label(default = "@llvm_mingw//:bin/i686-w64-mingw32-clang", allow_single_file = True, cfg = "exec"),
        "_llvm_mingw": attr.label(default = "@llvm_mingw//:files", cfg = "exec"),
        "_tool": _TOOL,
    },
)

def _references_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    names = []
    pairs = []
    for f in ctx.files.hardware:
        # tests/d3d9probes/hardware/<gpu>/results/<path>
        rest = f.path.split("/hardware/", 1)[1]
        gpu, _, path = rest.partition("/results/")
        if gpu not in names:
            names.append(gpu)
        pairs.append("{}/{}={}".format(gpu, path, f.path))
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = ["references", out.path, ",".join(names)] + pairs,
        inputs = ctx.files.hardware,
        outputs = [out],
        mnemonic = "D3d9ProbesReferences",
        progress_message = "Gathering the D3D9 probes' references: %{label}",
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles([out]))]

d3d9probes_references = rule(
    implementation = _references_impl,
    doc = "What the `d3d9probes` suite compares with: `<name>/d3d9probes/<gpu>/` " +
          "(a results folder) for each GPU, and `references.json` naming them.",
    attrs = {
        "hardware": attr.label_list(allow_files = True, doc = "`hardware/<gpu>/results/**`."),
        "_tool": _TOOL,
    },
)

def _zip_impl(ctx):
    out = ctx.actions.declare_file(ctx.attr.out)
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = ["zip", out.path] + [f.path for f in ctx.files.srcs],
        inputs = ctx.files.srcs,
        outputs = [out],
        mnemonic = "D3d9ProbesZip",
        progress_message = "Packing the D3D9 probes' runner: %{label}",
    )
    return [DefaultInfo(files = depset([out]))]

d3d9probes_zip = rule(
    implementation = _zip_impl,
    doc = "The files, by their names, in one zip.",
    attrs = {
        "out": attr.string(mandatory = True),
        "srcs": attr.label_list(allow_files = True, mandatory = True),
        "_tool": _TOOL,
    },
)
