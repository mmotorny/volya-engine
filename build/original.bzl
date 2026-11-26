# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The original's reference frames as actions (docs/architecture/build.md,
Game data): one run of the original per place (the title menu, a New
Game), its PNGs into the disk cache, keyed on the view list and settings
(compiled into the capture tool), the procedure (the tool), the install
folder's manifest and the Wine image's Containerfile. Game assets: in the
local cache only."""

load("@local_tools//:defs.bzl", "HOME")

def _original_frames_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    manifest = ctx.file._install
    ctx.actions.run(
        executable = ctx.executable._capture,
        arguments = [ctx.attr.place, out.path],
        inputs = [manifest] + ctx.files._containerfile,
        outputs = [out],
        env = {
            # As from the user's shell: Apple's `container` and its tools.
            "HOME": HOME,
            "PATH": "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            "VOLYA_GOG_ORIGINAL": manifest.path,
        },
        # Apple's container service and a VM: the machine itself.
        execution_requirements = {"no-sandbox": "1"},
        mnemonic = "OriginalFrames",
        progress_message = "Running the original for its frames: %{label} (minutes)",
    )
    return [DefaultInfo(files = depset([out]))]

original_frames = rule(
    implementation = _original_frames_impl,
    doc = "The original's frames of the views at `place`, in `<name>/fidelity/`.",
    attrs = {
        "place": attr.string(values = ["level", "title"], mandatory = True),
        "_capture": attr.label(default = "//tools/xtask:capture", executable = True, cfg = "exec"),
        "_containerfile": attr.label(default = "//tools/xtask:wine/Containerfile", allow_files = True),
        "_install": attr.label(default = "@gog_original//:manifest.tsv", allow_single_file = True),
    },
)

def _merged_frames_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    ctx.actions.run(
        executable = ctx.executable._capture,
        arguments = ["merge", out.path] + [d.path for d in ctx.files.frames],
        inputs = ctx.files.frames,
        outputs = [out],
        mnemonic = "OriginalFramesMerge",
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles([out]))]

merged_frames = rule(
    implementation = _merged_frames_impl,
    doc = "Several places' frames in one `<name>/fidelity/`, as the fidelity suite is served them.",
    attrs = {
        "frames": attr.label_list(mandatory = True),
        "_capture": attr.label(default = "//tools/xtask:capture", executable = True, cfg = "exec"),
    },
)
