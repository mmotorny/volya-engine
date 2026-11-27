# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The Direct3D 9 probes (tests/d3d9probes/, docs/testing.md) as actions:
built with MinGW in the Wine image, run under Wine there, and packed into
the Windows runner zip. The container actions take the Wine image
(`WINE_IMAGE`: its Containerfile and parts) as inputs, which tag the image
(wine.rs), so a changed image builds and runs them again."""

load("@local_tools//:defs.bzl", "HOME")
load(":wine_image.bzl", "WINE_IMAGE", "wine_image")

_TOOL = attr.label(default = "//tools/xtask:d3d9probes-action", executable = True, cfg = "exec")

_IMAGE = WINE_IMAGE

def _container_action(ctx, arguments, inputs, outputs, mnemonic, progress_message):
    image, image_env = wine_image(ctx)
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = arguments,
        inputs = inputs + image,
        outputs = outputs,
        env = dict(image_env, **{
            # As from the user's shell: Apple's `container` and its tools.
            "HOME": HOME,
            "PATH": "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        }),
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

def _references_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    wine = ctx.file.wine
    names = ["wine"]
    pairs = ["wine={}/results".format(wine.path)]
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
        inputs = [wine] + ctx.files.hardware,
        outputs = [out],
        mnemonic = "D3d9ProbesReferences",
        progress_message = "Gathering the D3D9 probes' references: %{label}",
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles([out]))]

d3d9probes_references = rule(
    implementation = _references_impl,
    doc = "What the `d3d9probes` suite compares with: `<name>/d3d9probes/<reference>/` " +
          "(a results folder) for Wine and each GPU, and `references.json` naming them.",
    attrs = {
        "hardware": attr.label_list(allow_files = True, doc = "`hardware/<gpu>/results/**`."),
        "wine": attr.label(mandatory = True, allow_single_file = True),
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
