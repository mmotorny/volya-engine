# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

"""What the machine has, not Bazel, as declared inputs (docs/architecture/
build.md): each tool's version in a file that tests take as data, so a new
version reruns them, and the machine's cache directory."""

_CHROME_PLIST = "/Applications/Google Chrome.app/Contents/Info.plist"

def _run(rctx, args):
    result = rctx.execute(args)
    if result.return_code != 0:
        return None
    return result.stdout.strip()

def _local_tools_impl(rctx):
    deny = rctx.which("cargo-deny")
    if deny:
        rctx.watch(deny)
        rctx.symlink(deny, "cargo-deny")
    else:
        # The check fails, saying it's missing (docs/testing.md, Setup).
        rctx.symlink("/usr/bin/false", "cargo-deny")
    deny_version = _run(rctx, [deny, "--version"]) if deny else None
    rctx.file("cargo_deny_version.txt", (deny_version or "missing") + "\n")

    # Chrome: its bundle's version, watched, so an update reruns the suites.
    chrome = None
    if rctx.path(_CHROME_PLIST).exists:
        rctx.watch(_CHROME_PLIST)
        chrome = _run(rctx, ["/usr/libexec/PlistBuddy", "-c", "Print CFBundleShortVersionString", _CHROME_PLIST])
    rctx.file("chrome_version.txt", (chrome or "missing") + "\n")

    # The newest iOS Simulator runtime, as `cargo xtask suite` picks it;
    # read again when this repository is (`bazel fetch --force @local_tools`).
    runtimes = _run(rctx, ["xcrun", "simctl", "list", "runtimes"]) or ""
    ios = [line.split(" ")[-1] for line in runtimes.splitlines() if "SimRuntime.iOS-" in line]
    rctx.file("simulator_runtime.txt", (ios[-1] if ios else "missing") + "\n")

    home = rctx.getenv("HOME")
    rctx.file("defs.bzl", "HOME = {}\nCACHE_DIR = {}\n".format(repr(home), repr(home + "/Library/Caches/volya-engine")))
    rctx.file("BUILD.bazel", """\
exports_files(
    glob(["*"]),
    visibility = ["//visibility:public"],
)
""")

local_tools = repository_rule(
    implementation = _local_tools_impl,
    doc = "cargo-deny, Chrome, the iOS Simulator runtime and the cache directory.",
)
