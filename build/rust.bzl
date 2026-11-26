# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""Our crates' targets: a library or binary, its unit tests, and a test per
integration test file (docs/architecture/build.md, Rust)."""

load("@crates//:defs.bzl", "aliases", "all_crate_deps")
load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_library", "rust_test")

# The workspace's [lints.rust] (Cargo.toml).
LINTS = ["-Dunsafe_code"]

def crate_name(name):
    return name.replace("-", "_")

def volya_library(name, deps = [], dev_deps = [], rustc_flags = [], test_data = [], test_tags = [], **kwargs):
    """A library crate and its unit tests (`<name>-unit`).

    Args:
        name: the package's crate name (hyphens allowed).
        deps: first-party dependencies.
        dev_deps: first-party dev-dependencies.
        rustc_flags: extra rustc flags.
        test_data: the unit tests' runtime data.
        test_tags: the unit tests' tags.
        **kwargs: passed to rust_library.
    """
    rust_library(
        name = name,
        crate_name = crate_name(name),
        srcs = native.glob(["src/**/*.rs"], exclude = ["src/bin/**"]),
        aliases = aliases(),
        deps = all_crate_deps(normal = True) + deps,
        proc_macro_deps = all_crate_deps(proc_macro = True),
        rustc_flags = LINTS + rustc_flags,
        **kwargs
    )
    rust_test(
        name = name + "-unit",
        crate = ":" + name,
        aliases = aliases(normal_dev = True, proc_macro_dev = True),
        deps = all_crate_deps(normal_dev = True) + dev_deps,
        proc_macro_deps = all_crate_deps(proc_macro_dev = True),
        rustc_flags = LINTS + rustc_flags,
        data = test_data,
        tags = test_tags,
    )

def volya_binary(name, srcs, crate_root = None, deps = [], rustc_flags = [], **kwargs):
    """A binary crate.

    Args:
        name: the binary's name.
        srcs: its sources.
        crate_root: its root file.
        deps: first-party dependencies.
        rustc_flags: extra rustc flags.
        **kwargs: passed to rust_binary.
    """
    rust_binary(
        name = name,
        crate_name = crate_name(name),
        srcs = srcs,
        crate_root = crate_root,
        aliases = aliases(),
        deps = all_crate_deps(normal = True) + deps,
        proc_macro_deps = all_crate_deps(proc_macro = True),
        rustc_flags = LINTS + rustc_flags,
        **kwargs
    )

def volya_integration_tests(crate, deps = [], rustc_flags = [], compile_data = [], tags = {}, common_tags = [], **kwargs):
    """A rust_test per file in tests/ (Cargo's integration tests).

    Args:
        crate: the library under test.
        deps: first-party dependencies besides `crate`.
        rustc_flags: extra rustc flags.
        compile_data: files the tests include (`#[path]`, `include!`).
        tags: extra tags by test name (the file's stem).
        common_tags: tags of every test.
        **kwargs: passed to every rust_test.
    """
    for src in native.glob(["tests/*.rs"], allow_empty = True):
        stem = src[len("tests/"):-len(".rs")]
        rust_test(
            name = stem,
            crate_name = stem,
            srcs = [src],
            aliases = aliases(normal_dev = True, proc_macro_dev = True),
            deps = [crate] + all_crate_deps(normal = True, normal_dev = True) + deps,
            proc_macro_deps = all_crate_deps(proc_macro = True, proc_macro_dev = True),
            rustc_flags = LINTS + rustc_flags,
            compile_data = compile_data,
            tags = common_tags + tags.get(stem, []),
            **kwargs
        )
