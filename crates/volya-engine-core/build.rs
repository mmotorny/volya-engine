// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Builds the Open Dynamics Engine's core from the 0.5.0 release (Bazel's
//! `@ode`, whose `LICENSE-BSD.TXT` `VOLYA_ODE_LICENSE` names) under its BSD
//! option, with the generated matrix routines from 0.14 (`@ode_fast`, whose
//! `fastdot.c` `VOLYA_ODE_FAST` names): natively with the C++ toolchain, for
//! wasm32 with Bazel's pinned LLVM against wasi-sdk's headers, linking no C or
//! C++ library (csrc/ode/wasm). See docs/architecture/simulation.md.

use std::env;
use std::path::PathBuf;

/// The core's sources (ode/src), as 0.5.0's Makefile lists them without the
/// triangle mesh, the test functions (testing) and the DIF export, which the
/// shipped library lacks (docs/behavior/physics-library.md 2).
const CORE: &[&str] = &[
    "array",
    "error",
    "memory",
    "obstack",
    "odemath",
    "matrix",
    "misc",
    "rotation",
    "mass",
    "ode",
    "step",
    "stepfast",
    "quickstep",
    "util",
    "lcp",
    "joint",
    "timer",
    "mat",
    "collision_kernel",
    "collision_util",
    "collision_std",
    "collision_space",
    "collision_transform",
    "collision_quadtreespace",
];

/// The generated matrix routines (the Makefile's `ODE_PREGEN_SRC`), C.
const FAST: &[&str] = &["fastldlt", "fastlsolve", "fastltsolve", "fastdot"];

/// The shipped library's configuration (docs/behavior/physics-library.md 2):
/// single precision (csrc/ode/include/ode/config.h), assertions compiled
/// out, no triangle mesh (`dTRIMESH_ENABLED` unset).
const DEFINES: &[(&str, &str)] = &[("dNODEBUG", "1"), ("NDEBUG", "1")];

/// Every operation rounded as written: no fused multiply-adds and no
/// reassociation (clang's default without `-ffast-math`, stated).
const FLOAT_FLAGS: &[&str] = &["-ffp-contract=off", "-fno-fast-math"];

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let wasm = env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32";
    println!("cargo:rerun-if-changed=csrc");
    let ode = dir_of("VOLYA_ODE_LICENSE", "ODE's LICENSE-BSD.TXT");
    let fast = dir_of("VOLYA_ODE_FAST", "ODE 0.14's ode/src/fastdot.c");

    let common = |build: &mut cc::Build| {
        build
            .include(root.join("csrc/ode/include"))
            .include(ode.join("include"))
            .opt_level(2)
            .warnings(false)
            .flag_if_supported("-w");
        for flag in FLOAT_FLAGS {
            build.flag(flag);
        }
        for (name, value) in DEFINES {
            build.define(name, *value);
        }
    };

    let mut build = cc::Build::new();
    common(&mut build);
    build
        .cpp(true)
        .std("c++98")
        .include(ode.join("ode/src"))
        .flag("-fno-exceptions")
        .flag("-fno-rtti");
    if wasm {
        wasm_toolchain(&mut build, true);
        build.cpp_link_stdlib(None);
    }
    for name in CORE {
        build.file(ode.join(format!("ode/src/{name}.cpp")));
    }
    build.compile("volya_ode");

    if wasm {
        let mut abi = cc::Build::new();
        abi.cpp(true)
            .std("c++14")
            .opt_level(2)
            .flag("-fno-exceptions")
            .flag("-fno-rtti")
            .file(root.join("csrc/ode/wasm/cxxabi.cpp"));
        wasm_toolchain(&mut abi, true);
        abi.cpp_link_stdlib(None);
        abi.compile("volya_ode_cxxabi");
    }

    let mut c = cc::Build::new();
    common(&mut c);
    c.include(root.join("csrc/ode/fast"));
    if wasm {
        wasm_toolchain(&mut c, false);
    }
    for name in FAST {
        c.file(fast.join(format!("{name}.c")));
    }
    c.compile("volya_ode_fast");

    if wasm {
        let mut libc = cc::Build::new();
        libc.opt_level(2)
            .warnings(true)
            .extra_warnings(true)
            .file(root.join("csrc/ode/wasm/libc.c"));
        wasm_toolchain(&mut libc, false);
        libc.compile("volya_ode_libc");
    }
}

/// The directory of the file the environment variable `var` names (Bazel
/// sets it).
fn dir_of(var: &str, what: &str) -> PathBuf {
    println!("cargo:rerun-if-env-changed={var}");
    let file = PathBuf::from(
        env::var_os(var).unwrap_or_else(|| panic!("{var} names {what} (Bazel sets it)")),
    );
    file.parent()
        .unwrap_or_else(|| panic!("{var} names a file"))
        .to_path_buf()
}

/// LLVM clang for wasm32 with wasi-sdk's C and C++ headers (wasi-libc's and
/// libc++'s); nothing of theirs is linked.
fn wasm_toolchain(build: &mut cc::Build, cpp: bool) {
    let bin = llvm_bin();
    let sysroot = bin.parent().unwrap().join("share/wasi-sysroot");
    build
        .compiler(bin.join(if cpp { "clang++" } else { "clang" }))
        .archiver(bin.join("llvm-ar"))
        .ranlib(bin.join("llvm-ranlib"))
        // No configuration files (the SDK's default sysroot and target).
        .flag("--no-default-config")
        // The headers' target; the objects are plain wasm32 ones.
        .flag("--target=wasm32-wasip1")
        .flag(format!("--sysroot={}", sysroot.display()));
    if cpp {
        // libc++'s headers built without exceptions, named rather than
        // found by the driver (which looks for the SDK's libraries, not
        // Bazel's inputs).
        build.flag("-nostdinc++").flag("-isystem").flag(
            sysroot
                .join("include/wasm32-wasip1/noeh/c++/v1")
                .to_str()
                .unwrap(),
        );
    }
}

/// The `bin` directory of the LLVM for wasm32: Bazel's pinned one, whose
/// clang `VOLYA_WASM_CLANG` names (docs/architecture/build.md, Build
/// scripts): Apple's clang has no WebAssembly target.
fn llvm_bin() -> PathBuf {
    println!("cargo:rerun-if-env-changed=VOLYA_WASM_CLANG");
    let clang = env::var_os("VOLYA_WASM_CLANG")
        .expect("VOLYA_WASM_CLANG names the wasm32 clang (Bazel sets it)");
    PathBuf::from(clang)
        .parent()
        .expect("VOLYA_WASM_CLANG names a file")
        .to_path_buf()
}
