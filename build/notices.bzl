# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""The site's licences: Volya Engine's LICENSE, then the third-party
notices from the dependency graph
(docs/architecture/build.md, Third-party notices): every crate linked into
the wasm, the Rust standard library it links, and every component a build
script compiles in, each with its declared licence and notice files; served
files are ours. `tools/site notices` writes the file and fails on anything
undeclared or not on deny.toml's allow-list."""

load("@rules_rust//rust:defs.bzl", "rust_common")

ThirdPartyInfo = provider(
    doc = "A third-party component built into the app, declared next to its archive.",
    fields = {
        "component": "Its name and version, as the notices name it.",
        "license": "Its licence, an SPDX expression.",
        "notices": "Files whose whole text is its notice.",
        "header_notices": "Files whose leading comment is its notice.",
        "files": "Every file it covers (its sources and notices).",
    },
)

BuildToolInfo = provider(
    doc = "Files a build script runs, never compiled into its output.",
    fields = {"files": "The tool's files."},
)

def _third_party_impl(ctx):
    files = depset(ctx.files.srcs + ctx.files.notices + ctx.files.header_notices)
    return [
        DefaultInfo(files = depset(ctx.files.srcs)),
        ThirdPartyInfo(
            component = ctx.attr.component,
            license = ctx.attr.license,
            notices = ctx.files.notices,
            header_notices = ctx.files.header_notices,
            files = files,
        ),
    ]

third_party = rule(
    implementation = _third_party_impl,
    doc = "A third-party component's sources, with its licence and notices; provides the sources as files.",
    attrs = {
        "component": attr.string(mandatory = True),
        "header_notices": attr.label_list(allow_files = True),
        "license": attr.string(mandatory = True),
        "notices": attr.label_list(allow_files = True),
        "srcs": attr.label_list(allow_files = True),
    },
)

def _build_tool_impl(ctx):
    return [
        DefaultInfo(files = depset(ctx.files.srcs)),
        BuildToolInfo(files = depset(ctx.files.srcs)),
    ]

build_tool = rule(
    implementation = _build_tool_impl,
    doc = "Files a build script runs (a compiler), not compiled into what it builds.",
    attrs = {"srcs": attr.label_list(allow_files = True, mandatory = True)},
)

_LinkedInfo = provider(
    doc = "What a target links into the wasm (the aspect's result).",
    fields = {
        "lines": "depset of manifest lines (tools/site/src/notices.rs).",
        "inputs": "depset of the files they name.",
    },
)

def _rel(f):
    """A file's path within its repository."""
    root = f.owner.workspace_root
    return f.path[len(root) + 1:] if root else f.path

def _repo(label):
    return label.workspace_name or "_main"

def _line(*fields):
    return "\t".join(fields)

def _linked_aspect_impl(target, ctx):
    lines = []
    inputs = []
    transitive_lines = []
    transitive_inputs = []
    kind = ctx.rule.kind
    # A crate it links, or the wasm's (a cdylib, without CrateInfo).
    crate_info = rust_common.crate_info
    if kind == "rust_cdylib_library" or (crate_info in target and target[crate_info].type != "proc-macro"):
        for dep in getattr(ctx.rule.attr, "deps", []):
            if _LinkedInfo in dep:
                transitive_lines.append(dep[_LinkedInfo].lines)
                transitive_inputs.append(dep[_LinkedInfo].inputs)
        tags = ctx.rule.attr.tags
        crate = [t for t in tags if t.startswith("crate-name=")]
        if target.label.workspace_name:
            if not crate:
                lines.append(_line("problem", "{}: an external Rust target that isn't a crate from Cargo.lock".format(target.label)))
            else:
                # The crate's manifest and licence files: its root's,
                # REUSE's LICENSES/, and any its release lacks, added from
                # ours (MODULE.bazel's annotations); and its root module,
                # for the copyright its header states.
                files = [
                    f
                    for f in ctx.rule.files.compile_data
                    if f.is_source and (
                        "/" not in _rel(f) or
                        (_rel(f).startswith("LICENSES/") and _rel(f).count("/") == 1) or
                        not f.owner.workspace_name
                    )
                ]
                manifests = [f for f in files if _rel(f) == "Cargo.toml" and f.owner.workspace_name]
                if len(manifests) != 1:
                    lines.append(_line("problem", "{}: no Cargo.toml".format(target.label)))
                else:
                    crate_root = target[crate_info].root
                    lines.append(_line(
                        "crate",
                        manifests[0].path,
                        crate_root.path,
                        *[f.path for f in files if f != manifests[0]]
                    ))
                    inputs += files + [crate_root]
    elif kind == "cargo_build_script":
        # What it compiles in: declared components, or its own crate's files
        # (ours outside vendor/); compilers it runs are tools.
        covered = {}
        own = _repo(target.label)
        for dep in ctx.rule.attr.data:
            if ThirdPartyInfo in dep:
                info = dep[ThirdPartyInfo]
                lines.append(_line(
                    "component",
                    info.component,
                    info.license,
                    ";".join([f.path for f in info.notices]),
                    ";".join([f.path for f in info.header_notices]),
                ))
                inputs += info.notices + info.header_notices
                for f in info.files.to_list():
                    covered[f.path] = True
            if BuildToolInfo in dep:
                for f in dep[BuildToolInfo].files.to_list():
                    covered[f.path] = True
        for dep in ctx.rule.attr.data:
            for f in dep.files.to_list():
                if f.path in covered:
                    continue
                if _repo(f.owner) == own and not (own == "_main" and "/vendor/" in "/" + f.path):
                    continue
                lines.append(_line("problem", "{} compiles {} ({}), which no third_party or build_tool target declares".format(target.label, dep.label, f.short_path)))
                break
    return [_LinkedInfo(
        lines = depset(lines, transitive = transitive_lines),
        inputs = depset(inputs, transitive = transitive_inputs),
    )]

_linked_aspect = aspect(
    implementation = _linked_aspect_impl,
    attr_aspects = ["deps"],
    doc = "Collects what a Rust target links: crates, and components its build scripts compile.",
)

def _release_wasm_transition_impl(_settings, _attr):
    return {
        "//command_line_option:compilation_mode": "opt",
        "//command_line_option:platforms": str(Label("//build/platforms:wasm32")),
    }

_release_wasm_transition = transition(
    implementation = _release_wasm_transition_impl,
    inputs = [],
    outputs = [
        "//command_line_option:compilation_mode",
        "//command_line_option:platforms",
    ],
)

# The standard library's crates a program built without the test harness
# or proc_macro doesn't link (they ship in rust-std for those).
_STD_UNLINKED = ["getopts", "proc_macro", "rustc_literal_escaper", "sysroot", "test"]

_RUST_SRC_NOTICE_PREFIXES = ["LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE"]

def _third_party_notices_impl(ctx):
    linked = ctx.attr.wasm[0][_LinkedInfo]
    lines = linked.lines.to_list()
    inputs = [linked.inputs]
    analyzer = ctx.toolchains["@rules_rust//rust/rust_analyzer:toolchain_type"]
    std = ctx.attr.rust_std[ThirdPartyInfo]
    lines.append(_line(
        "std",
        std.component + " " + analyzer.version,
        std.license,
        ";".join([f.path for f in std.notices]),
    ))
    extra = list(std.notices)

    # The standard library's linked crates: the target's rlibs.
    for f in ctx.files._rust_std:
        if f.extension != "rlib":
            continue
        name = f.basename[len("lib"):].rsplit("-", 1)[0]
        if name not in _STD_UNLINKED:
            lines.append(_line("std_rlib", name))

    # Their sources' manifests and licence files (rust-src, from the
    # rust-analyzer toolchain of the same release).
    srcs = analyzer.rustc_srcs
    library = None
    for f in srcs.files.to_list():
        if f.basename == "Cargo.toml" or [p for p in _RUST_SRC_NOTICE_PREFIXES if f.basename.upper().startswith(p)]:
            rel = _rel(f)
            at = rel.find("/library/")
            if at < 0:
                continue
            if library == None:
                library = f.path[:len(f.path) - len(rel) + at + len("/library")]
            depth = rel[at + len("/library/"):].count("/")
            if depth > 3:
                continue
            extra.append(f)
            lines.append(_line("rust_src", f.path))
    if library == None:
        fail("rust-src has no library/: the rust-analyzer toolchain's rustc_srcs")
    lines.append(_line("rust_library", library))

    for f in ctx.files.static:
        lines.append(_line("static", f.path, _repo(f.owner)))
    extra += ctx.files.static
    lines.append(_line("deny", ctx.file.deny.path))
    extra.append(ctx.file.deny)
    lines.append(_line("license", ctx.file.license.path))
    extra.append(ctx.file.license)

    manifest = ctx.actions.declare_file(ctx.label.name + ".manifest")
    ctx.actions.write(manifest, "\n".join(lines) + "\n")
    out = ctx.actions.declare_file(ctx.label.name + ".txt")
    ctx.actions.run(
        executable = ctx.executable._site,
        arguments = ["notices", manifest.path, out.path],
        inputs = depset([manifest] + extra, transitive = inputs),
        outputs = [out],
        mnemonic = "ThirdPartyNotices",
        progress_message = "Writing the third-party notices %{label}",
    )
    return [DefaultInfo(files = depset([out]))]

third_party_notices = rule(
    implementation = _third_party_notices_impl,
    doc = "The notices of everything linked into the release wasm; fails on anything undeclared or not allowed.",
    attrs = {
        "deny": attr.label(allow_single_file = True, mandatory = True, doc = "deny.toml: its licence allow-list."),
        "license": attr.label(allow_single_file = True, mandatory = True, doc = "Volya Engine's own LICENSE, first in the file."),
        "rust_std": attr.label(providers = [ThirdPartyInfo], mandatory = True, doc = "The standard library's in-tree notices."),
        "static": attr.label_list(allow_files = True, doc = "The files the page serves besides the engine."),
        "wasm": attr.label(cfg = _release_wasm_transition, aspects = [_linked_aspect], mandatory = True),
        "_rust_std": attr.label(
            default = "@rules_rust//rust/toolchain:current_rust_stdlib_files",
            cfg = _release_wasm_transition,
        ),
        "_site": attr.label(
            default = "//tools/site",
            executable = True,
            cfg = "exec",
        ),
    },
    toolchains = ["@rules_rust//rust/rust_analyzer:toolchain_type"],
)
