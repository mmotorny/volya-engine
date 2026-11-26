# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""A package's static checks (docs/architecture/build.md, Static checks):
rustfmt and clippy over its Rust targets, natively and for wasm32, and its
files for the SPDX check. Called last in every BUILD.bazel."""

load("@rules_rust//rust:defs.bzl", "rust_clippy_test", "rustfmt_test")

_RUST_KINDS = ["rust_binary", "rust_library", "rust_shared_library", "rust_test"]

def volya_checks(wasm = True):
    """`fmt`, `clippy`, `clippy_wasm`, `all_files` and the suite `checks`.

    Args:
        wasm: whether the package's crates build for wasm32, for clippy there.
    """
    targets = [
        ":" + r["name"]
        for r in native.existing_rules().values()
        if r["kind"] in _RUST_KINDS
    ]
    checks = []
    if targets:
        rustfmt_test(
            name = "fmt",
            targets = targets,
            tags = ["check"],
        )
        rust_clippy_test(
            name = "clippy",
            targets = targets,
            tags = ["check"],
        )
        checks += [":fmt", ":clippy"]
        if wasm:
            rust_clippy_test(
                name = "clippy_wasm",
                platform = Label("//build/platforms:wasm32"),
                targets = targets,
                tags = ["check"],
            )
            checks.append(":clippy_wasm")
    native.filegroup(
        name = "all_files",
        srcs = native.glob(["**"], allow_empty = True),
        visibility = ["//:__pkg__"],
    )
    native.test_suite(
        name = "checks",
        tests = checks,
        visibility = ["//:__pkg__"],
    )
