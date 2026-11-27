# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The original's capture on Windows (tools/wincapture/, docs/testing.md) as
actions: the frame clock's proxy winmm.dll, the counter's DynamoRIO client
and volyacapture.exe built with llvm-mingw into 32-bit Windows files, and
the driver's plan from the fidelity suite's views."""

_TOOL = attr.label(default = "//tools/xtask:wincapture-action", executable = True, cfg = "exec")

def _binary_impl(ctx):
    out = ctx.actions.declare_file(ctx.attr.out)
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = ["build", ctx.file._clang.path, out.path, ctx.attr.kind] + [f.path for f in ctx.files.srcs + ctx.files.sdk],
        inputs = ctx.files.srcs + ctx.files.sdk + ctx.files._llvm_mingw,
        outputs = [out],
        mnemonic = "WincaptureBuild",
        progress_message = "Building %{label} with llvm-mingw",
    )
    return [DefaultInfo(files = depset([out]))]

wincapture_binary = rule(
    implementation = _binary_impl,
    doc = "C sources compiled into a 32-bit Windows DLL (`kind` dll), executable (exe) or " +
          "DynamoRIO client (client: `sdk` holds DynamoRIO's headers and import library; " +
          "client-kernel32 the bisect's variant importing kernel32).",
    attrs = {
        "kind": attr.string(mandatory = True, values = ["dll", "exe", "client", "client-kernel32"]),
        "out": attr.string(mandatory = True),
        "sdk": attr.label_list(allow_files = [".h", ".lib"]),
        "srcs": attr.label_list(allow_files = [".c", ".h"], mandatory = True),
        "_clang": attr.label(default = "@llvm_mingw//:bin/i686-w64-mingw32-clang", allow_single_file = True, cfg = "exec"),
        "_llvm_mingw": attr.label(default = "@llvm_mingw//:files", cfg = "exec"),
        "_tool": _TOOL,
    },
)

def _plan_impl(ctx):
    out = ctx.actions.declare_file(ctx.attr.out)
    ctx.actions.run(
        executable = ctx.executable._tool,
        arguments = ["plan", out.path],
        outputs = [out],
        mnemonic = "WincapturePlan",
        progress_message = "Writing the Windows capture's plan: %{label}",
    )
    return [DefaultInfo(files = depset([out]))]

wincapture_plan = rule(
    implementation = _plan_impl,
    doc = "The driver's plan.txt, from tests/fidelity/views.rs (compiled into the tool).",
    attrs = {
        "out": attr.string(mandatory = True),
        "_tool": _TOOL,
    },
)
