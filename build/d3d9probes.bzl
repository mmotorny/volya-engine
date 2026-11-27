# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The Direct3D 9 probes (tests/d3d9probes/, docs/testing.md) as actions:
built with MinGW in the Wine image, run under Wine there, and packed into
the Windows runner zip. The container actions take the Wine image's
Containerfile and box64's source as inputs, which tag the image (wine.rs),
so a changed image builds and runs them again."""

load("@local_tools//:defs.bzl", "HOME")

_TOOL = attr.label(default = "//tools/xtask:d3d9probes-action", executable = True, cfg = "exec")

_IMAGE = {
    "_box64": attr.label(default = "@box64//:CMakeLists.txt", allow_single_file = True),
    "_containerfile": attr.label(default = "//tools/xtask:wine/Containerfile", allow_single_file = True),
}

def _container_action(ctx, arguments, inputs, outputs, mnemonic, progress_message):
    box64 = ctx.file._box64
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = arguments,
        inputs = inputs + [box64, ctx.file._containerfile],
        outputs = outputs,
        env = {
            # As from the user's shell: Apple's `container` and its tools.
            "HOME": HOME,
            "PATH": "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            "VOLYA_BOX64": box64.path,
        },
        # Apple's container service and a VM: the machine itself.
        execution_requirements = {"no-sandbox": "1"},
        mnemonic = mnemonic,
        progress_message = progress_message,
    )

def _exe_impl(ctx):
    out = ctx.actions.declare_file(ctx.attr.out)
    _container_action(
        ctx,
        arguments = ["build", out.path] + [f.path for f in ctx.files.srcs],
        inputs = ctx.files.srcs,
        outputs = [out],
        mnemonic = "D3d9ProbesBuild",
        progress_message = "Building the D3D9 probes with MinGW: %{label}",
    )
    return [DefaultInfo(files = depset([out]))]

d3d9probes_exe = rule(
    implementation = _exe_impl,
    doc = "The probes compiled into one 32-bit Windows executable.",
    attrs = dict(_IMAGE, **{
        "out": attr.string(mandatory = True),
        "srcs": attr.label_list(allow_files = [".c", ".h"], mandatory = True),
        "_tool": _TOOL,
    }),
)

def _wine_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    _container_action(
        ctx,
        arguments = ["run", ctx.file.exe.path, out.path],
        inputs = [ctx.file.exe],
        outputs = [out],
        mnemonic = "D3d9ProbesWine",
        progress_message = "Running the D3D9 probes under Wine: %{label}",
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles([out]))]

d3d9probes_wine = rule(
    implementation = _wine_impl,
    doc = "The probes' output under Wine: `<name>/results/` and `<name>/log.txt`.",
    attrs = dict(_IMAGE, **{
        "exe": attr.label(mandatory = True, allow_single_file = [".exe"]),
        "_tool": _TOOL,
    }),
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
