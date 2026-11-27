// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Builds the Open Dynamics Engine's core and OU from the 0.16.6 release
//! (Bazel's `@ode`, whose `LICENSE-BSD.TXT` `VOLYA_ODE_LICENSE` names; left
//! unmodified)
//! under their BSD option: natively with the C++ toolchain, for wasm32 with
//! Bazel's pinned LLVM against wasi-sdk's headers, linking no C or C++
//! library (csrc/ode/wasm). See docs/architecture/simulation.md.

use std::env;
use std::path::PathBuf;

/// The core's sources (ode/src), as its CMake build lists them with
/// trimeshes, threading and libccd off. The threading pools are left out:
/// with the threading interface disabled they hold nothing, as the
/// thread-local storage does (odetls.cpp) without OU's TLS feature.
const CORE: &[&str] = &[
    "array",
    "box",
    "capsule",
    "collision_cylinder_box",
    "collision_cylinder_plane",
    "collision_cylinder_sphere",
    "collision_kernel",
    "collision_quadtreespace",
    "collision_sapspace",
    "collision_space",
    "collision_transform",
    "collision_trimesh_disabled",
    "collision_util",
    "convex",
    "cylinder",
    "default_threading",
    "error",
    "export-dif",
    "fastdot",
    "fastldltfactor",
    "fastldltsolve",
    "fastlsolve",
    "fastltsolve",
    "fastvecscale",
    "heightfield",
    "lcp",
    "mass",
    "mat",
    "matrix",
    "memory",
    "misc",
    "objects",
    "obstack",
    "ode",
    "odeinit",
    "odemath",
    "odeou",
    "plane",
    "quickstep",
    "ray",
    "resource_control",
    "rotation",
    "simple_cooperative",
    "sphere",
    "step",
    "threading_base",
    "threading_impl",
    "timer",
    "util",
    "joints/amotor",
    "joints/ball",
    "joints/contact",
    "joints/dball",
    "joints/dhinge",
    "joints/fixed",
    "joints/hinge",
    "joints/hinge2",
    "joints/joint",
    "joints/lmotor",
    "joints/null",
    "joints/piston",
    "joints/plane2d",
    "joints/pr",
    "joints/pu",
    "joints/slider",
    "joints/transmission",
    "joints/universal",
];

/// OU's sources (ou/src/ou) that its basic features use; its atomics and
/// thread-local storage are empty without theirs.
const OU: &[&str] = &["customization", "malloc"];

/// The configuration, as ODE's release build (CMake, not Debug) sets it
/// with these options: single precision (`dIDESINGLE`), no threading
/// interface or built-in threading, OU's basic features only (no TLS or
/// atomics), no internal checks; OU's target OS follows the target.
const DEFINES: &[(&str, &str)] = &[
    ("dIDESINGLE", "1"),
    ("dTHREADING_INTF_DISABLED", "1"),
    ("dOU_ENABLED", "1"),
    ("_OU_NAMESPACE", "odeou"),
    ("_OU_FEATURE_SET", "_OU_FEATURE_SET_BASICS"),
    ("dNODEBUG", "1"),
    ("NDEBUG", "1"),
];

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let wasm = env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32";
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-env-changed=VOLYA_ODE_LICENSE");
    let license = PathBuf::from(
        env::var_os("VOLYA_ODE_LICENSE")
            .expect("VOLYA_ODE_LICENSE names ODE's LICENSE-BSD.TXT (Bazel sets it)"),
    );
    let ode = license
        .parent()
        .expect("VOLYA_ODE_LICENSE names a file")
        .to_path_buf();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++14")
        .include(root.join("csrc/ode"))
        .include(ode.join("include"))
        .include(ode.join("ode/src"))
        .include(ode.join("ode/src/joints"))
        .include(ode.join("ou/include"))
        .opt_level(2)
        .warnings(false)
        .flag_if_supported("-w")
        .flag("-fno-exceptions")
        .flag("-fno-rtti");
    for (name, value) in DEFINES {
        build.define(name, *value);
    }
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        // What ODE's CMake build finds on any current macOS.
        build.define("_OU_TARGET_OS", "_OU_TARGET_OS_MAC");
        build.define("MAC_OS_X_VERSION", "1050");
    } else {
        build.define("_OU_TARGET_OS", "_OU_TARGET_OS_GENUNIX");
    }
    if wasm {
        wasm_toolchain(&mut build, true);
        build.cpp_link_stdlib(None);
        build.file(root.join("csrc/ode/wasm/cxxabi.cpp"));
    }
    for name in CORE {
        build.file(ode.join(format!("ode/src/{name}.cpp")));
    }
    for name in OU {
        build.file(ode.join(format!("ou/src/ou/{name}.cpp")));
    }
    build.compile("volya_ode");

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
