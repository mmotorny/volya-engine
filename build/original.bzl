# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The original's reference frames as an action (docs/architecture/build.md,
Game data): one run of the original (the title menu, then a New Game
started from it), its PNGs into the disk cache, keyed on the view list and settings
(compiled into the capture tool), the procedure (the tool), the install
folder's manifest and the Wine image (its Containerfile and parts). Game
assets: in the local cache only."""

load("@local_tools//:defs.bzl", "HOME")
load(":wine_image.bzl", "WINE_IMAGE", "wine_image")

def _original_frames_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    manifest = ctx.file._install
    image, image_env = wine_image(ctx)
    ctx.actions.run(
        executable = ctx.executable._capture,
        arguments = [out.path],
        inputs = [manifest] + image,
        outputs = [out],
        env = dict(image_env, **{
            # As from the user's shell: Apple's `container` and its tools.
            "HOME": HOME,
            "PATH": "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            "VOLYA_GOG_ORIGINAL": manifest.path,
        }),
        # Apple's container service and a VM: the machine itself.
        execution_requirements = {"no-sandbox": "1"},
        mnemonic = "OriginalFrames",
        progress_message = "Running the original for its frames: %{label} (minutes)",
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles([out]))]

original_frames = rule(
    implementation = _original_frames_impl,
    doc = "The original's frames of the views, in `<name>/fidelity/`.",
    attrs = dict(WINE_IMAGE, **{
        "_capture": attr.label(default = "//tools/xtask:capture", executable = True, cfg = "exec"),
        "_install": attr.label(default = "@gog_original//:manifest.tsv", allow_single_file = True),
    }),
)
