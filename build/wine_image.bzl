# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The Wine image's parts (docs/architecture/build.md, Game data): Wine and
box64 each built as an action, in a container (`//tools/xtask:image-part`),
into a tarball of what they install, keyed on their sources and the tool
(which holds the build scripts), so the disk cache keeps them across
worktrees and restarts. The image (`tools/xtask/wine/Containerfile`) only
adds them."""

load("@local_tools//:defs.bzl", "HOME")

def _image_part_impl(ctx):
    out = ctx.actions.declare_file(ctx.label.name + ".tar.gz")
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = [ctx.attr.part, out.path] + [f.path for f in ctx.files.srcs],
        inputs = ctx.files.srcs,
        outputs = [out],
        env = {
            # As from the user's shell: Apple's `container`.
            "HOME": HOME,
            "PATH": "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        },
        # Apple's container service and a VM: the machine itself.
        execution_requirements = {"no-sandbox": "1"},
        mnemonic = "WineImagePart",
        progress_message = "Building %{label} in a container (Wine: about an hour)",
    )
    return [DefaultInfo(files = depset([out]))]

image_part = rule(
    implementation = _image_part_impl,
    doc = "A part of the Wine image, `<name>.tar.gz`, built by `image-part <part>`.",
    attrs = {
        "part": attr.string(mandatory = True, values = ["wine", "box64"]),
        "srcs": attr.label_list(allow_files = True, mandatory = True),
        "_tool": attr.label(default = "//tools/xtask:image-part", executable = True, cfg = "exec"),
    },
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
