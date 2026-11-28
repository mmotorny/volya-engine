# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""A package's static checks (docs/architecture/build.md, Static checks):
rustfmt and clippy over its Rust targets, natively and for wasm32, its
files for the SPDX check, and the tests `bazel test //...` runs in it, for
//build:default_only. Called last in every BUILD.bazel."""

load("@rules_rust//rust:defs.bzl", "rust_clippy_test", "rustfmt_test")

_RUST_KINDS = ["rust_binary", "rust_library", "rust_shared_library", "rust_test"]

# A static check's tags: `check` for //build:checks_static, `manual` to keep
# it out of `bazel test //...` (they gate merging, by name: `//:checks`).
CHECK = ["check", "manual"]

def volya_checks(wasm = True):
    """`fmt`, `clippy`, `clippy_wasm`, `all_files` and the suites `checks`
    and `default_tests`.

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
            tags = CHECK,
        )
        rust_clippy_test(
            name = "clippy",
            targets = targets,
            tags = CHECK,
        )
        checks += [":fmt", ":clippy"]
        if wasm:
            rust_clippy_test(
                name = "clippy_wasm",
                platform = Label("//build/platforms:wasm32"),
                targets = targets,
                tags = CHECK,
            )
            checks.append(":clippy_wasm")
    # What `bazel test //...` runs in the package, named explicitly: its tests
    # and test suites not tagged `manual` (genquery takes no wildcards).
    default = [
        ":" + r["name"]
        for r in native.existing_rules().values()
        if (r["kind"].endswith("_test") or r["kind"] == "test_suite") and
           "manual" not in r.get("tags", [])
    ]
    native.filegroup(
        name = "all_files",
        srcs = native.glob(["**"], allow_empty = True),
        visibility = ["//:__pkg__"],
    )
    # Tagged `check`: an empty `tests` would mean every non-manual test of
    # the package, the tag keeps it to static checks.
    native.test_suite(
        name = "checks",
        tags = CHECK,
        tests = checks,
        visibility = ["//:__pkg__"],
    )
    # Manual, as every suite here: `bazel test //...` would run every test a
    # suite it matches names.
    native.test_suite(
        name = "default_tests",
        tags = ["manual"],
        tests = default,
        visibility = ["//:__pkg__"],
    )
