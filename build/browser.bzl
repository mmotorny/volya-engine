# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""Browser suites as tests (docs/architecture/build.md, Browser suites): one
per suite and target, run by `xtask suite TARGET SUITE` on the test site."""

load("@gog_installer//:defs.bzl", "GOG_INSTALLER")
load("@local_tools//:defs.bzl", "CACHE_DIR", "HOME")

def _browser_suite_test_impl(ctx):
    runner = ctx.executable.runner
    exe = ctx.actions.declare_file(ctx.label.name)
    ctx.actions.symlink(output = exe, target_file = runner, is_executable = True)
    env = {
        # The user's: under Bazel's own HOME, Chrome sends no report
        # (measured; why is not known).
        "HOME": HOME,
        "VOLYA_CACHE": CACHE_DIR,
        "VOLYA_SITE": ctx.file.site.short_path,
        "VOLYA_SITE_NEXT": ctx.file.site_next.short_path,
        "VOLYA_TESTS": ctx.label.package,
    }
    files = []
    if ctx.attr.references:
        # The fidelity suite's original frames, the d3d9probes suite's results.
        files = ctx.files.references
        env["VOLYA_REFERENCES"] = ctx.files.references[0].short_path
    if ctx.attr.gamedata and GOG_INSTALLER:
        env["VOLYA_GOG_INSTALLER"] = GOG_INSTALLER
    runfiles = ctx.runfiles(
        files = [ctx.file.site, ctx.file.site_next] + ctx.files.suite_files + ctx.files.data + files,
    ).merge(ctx.attr.runner[DefaultInfo].default_runfiles)
    return [
        DefaultInfo(executable = exe, runfiles = runfiles),
        RunEnvironmentInfo(environment = env),
    ]

browser_suite_test = rule(
    implementation = _browser_suite_test_impl,
    test = True,
    attrs = {
        "data": attr.label_list(allow_files = True, doc = "Further inputs: the browser's version, the game data's fingerprint."),
        "gamedata": attr.bool(doc = "Serve the installer to the page."),
        "references": attr.label(doc = "What the page compares with, served as `<suite>/...`: a directory."),
        "runner": attr.label(executable = True, cfg = "target", mandatory = True),
        "site": attr.label(allow_single_file = True, mandatory = True),
        "site_next": attr.label(allow_single_file = True, mandatory = True),
        "suite_files": attr.label_list(allow_files = True, mandatory = True),
    },
)

# Suites that measure (docs/architecture/build.md, Browser suites).
MEASURING = ["draw-calls", "fidelity", "game-level", "game-memory"]

def suite_group(name, tests):
    """A test_suite of browser suites, itself manual: `bazel test //...`
    would otherwise run every test it names, manual ones included.

    Args:
        name: the group's name.
        tests: the suites.
    """
    native.test_suite(name = name, tags = ["manual"], tests = tests)

def browser_suite(suite, target, tags = [], timeout = "moderate", gamedata = True, references = None):
    """`<suite>_<target>`: the browser suite `suite` on `target`.

    Args:
        suite: the suite's name (tests/browser/<suite>.js).
        target: `chrome` or `iphone`.
        tags: tags besides the ones every suite has.
        timeout: Bazel's timeout (the suite's report timeout is its own).
        gamedata: whether the suite reads the installer.
        references: what the page compares with (the fidelity and d3d9probes suites').
    """
    # Never in `bazel test //...` by accident: the iOS Simulator and the
    # measurements run only when named, alone on the machine.
    if (target == "iphone" or suite in MEASURING) and not ("manual" in tags and "exclusive" in tags):
        fail("{} on {} must be tagged manual and exclusive".format(suite, target))
    browser = "@local_tools//:chrome_version.txt" if target == "chrome" else "@local_tools//:simulator_runtime.txt"
    browser_suite_test(
        name = "{}_{}".format(suite.replace("-", "_"), target),
        args = ["suite", target, suite],
        data = [browser] + (["@gog_installer//:fingerprint.txt"] if gamedata else []),
        gamedata = gamedata,
        references = references,
        runner = "//tools/xtask",
        site = "//platforms/web:site_test",
        site_next = "//platforms/web:site_next",
        suite_files = native.glob(["*.js", "*.html"]),
        # The real browser and Simulator, profiles outside the execroot; one
        # browser suite at a time per invocation (--local_resources).
        tags = ["browser", "no-sandbox", "resources:browser:1"] + tags,
        timeout = timeout,
    )
