# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The web app's site (docs/architecture/build.md, The web app): the wasm
built for wasm32 in the profile and features asked for, through wasm-bindgen,
stamped, with the static files and the third-party notices
(build/notices.bzl)."""

_TEST_OPS = str(Label("//build:test_ops"))
_LTO = str(Label("@rules_rust//rust/settings:lto"))

def _wasm_transition_impl(_settings, attr):
    release = attr.compilation_mode == "opt"
    return {
        "//command_line_option:compilation_mode": attr.compilation_mode,
        "//command_line_option:platforms": str(Label("//build/platforms:wasm32")),
        _LTO: "fat" if release else "unspecified",
        _TEST_OPS: attr.test_ops,
    }

_wasm_transition = transition(
    implementation = _wasm_transition_impl,
    inputs = [],
    outputs = [
        "//command_line_option:compilation_mode",
        "//command_line_option:platforms",
        _LTO,
        _TEST_OPS,
    ],
)

def _web_site_impl(ctx):
    wasm = [f for f in ctx.files.wasm if f.extension == "wasm"]
    if len(wasm) != 1:
        fail("expected one .wasm, got {}".format(ctx.files.wasm))
    out = ctx.actions.declare_directory(ctx.label.name)
    static_root = ctx.label.package + "/" + ctx.attr.static_root
    ctx.actions.run(
        executable = ctx.executable._site,
        arguments = [
            "build",
            static_root,
            ctx.file.notices.path,
            wasm[0].path,
            ctx.executable._wasm_bindgen.path,
            out.path,
        ],
        inputs = ctx.files.static + wasm + [ctx.file.notices],
        tools = [ctx.executable._wasm_bindgen],
        outputs = [out],
        mnemonic = "WebSite",
        progress_message = "Building the site %{label}",
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles([out]))]

web_site = rule(
    implementation = _web_site_impl,
    doc = "A site directory: the static files, the wasm-bindgen output, stamped for tests.",
    attrs = {
        "compilation_mode": attr.string(
            doc = "`opt` (release) or `fastbuild` (dev).",
            values = ["fastbuild", "opt"],
            mandatory = True,
        ),
        "notices": attr.label(
            doc = "The third-party notices (a third_party_notices target).",
            allow_single_file = True,
            mandatory = True,
        ),
        "static": attr.label_list(allow_files = True, mandatory = True),
        "static_root": attr.string(
            doc = "The static files' directory, relative to the package.",
            default = "static",
        ),
        "test_ops": attr.bool(doc = "The test build's engine ops."),
        "wasm": attr.label(cfg = _wasm_transition, mandatory = True),
        "_site": attr.label(
            default = "//tools/site",
            executable = True,
            cfg = "exec",
        ),
        "_wasm_bindgen": attr.label(
            default = "@tools//:wasm-bindgen-cli__wasm-bindgen",
            executable = True,
            cfg = "exec",
        ),
    },
)

def _next_site_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    static_root = ctx.label.package + "/" + ctx.attr.static_root
    site = ctx.file.site
    ctx.actions.run(
        executable = ctx.executable._site,
        arguments = ["next", site.path, static_root, out.path],
        inputs = ctx.files.static + [site],
        outputs = [out],
        mnemonic = "WebSiteNext",
        progress_message = "Building the next site %{label}",
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles([out]))]

next_site = rule(
    implementation = _next_site_impl,
    doc = "A site as the next deploy would be, for the update flow's test.",
    attrs = {
        "site": attr.label(allow_single_file = True, mandatory = True),
        "static": attr.label_list(allow_files = True, mandatory = True),
        "static_root": attr.string(default = "static"),
        "_site": attr.label(
            default = "//tools/site",
            executable = True,
            cfg = "exec",
        ),
    },
)
