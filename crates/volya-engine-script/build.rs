// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Builds the vendored LuaJIT 1.0.3 interpreter (vendor/luajit-1.0.3, left
//! unmodified) with our shims (csrc/): natively with the system C compiler,
//! for wasm32 with Homebrew LLVM clang against our minimal C library
//! (csrc/wasm). See docs/architecture.md (Scripts).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const VENDOR: &str = "vendor/luajit-1.0.3/src";

/// The interpreter's C files: the Lua core and the libraries the original
/// opens (base, table, string, math, jit, coco). Not built: the machine-code
/// compiler (ljit_core.c, ljit_mem.c, ljit_backend.c, ljit_dasm.c, DynASM),
/// Coco's C stacks (lcoco.c), the io, os, debug and package libraries, and
/// the stand-alone programs.
const VENDORED: &[&str] = &[
    "lapi", "lcode", "ldebug", "ldo", "ldump", "lfunc", "lgc", "llex", "lmem", "lobject",
    "lopcodes", "lparser", "lstate", "lstring", "ltable", "ltm", "lundump", "lvm", "lzio",
    "lauxlib", "lbaselib", "ltablib", "lstrlib", "lmathlib", "ljitlib", "lcocolib",
];

/// Our C files, built on every target.
const SHIMS: &[&str] = &[
    "volya_jit",
    "volya_coco",
    "volya_glue",
    "volya_printf",
    "volya_rand",
];

/// The build's only change to a vendored file, applied to a copy in OUT_DIR:
/// `luaconf.h` is Lua's configuration header, meant to be edited by embedders.
/// Each `(from, to)` must match exactly once.
const LUACONF_PATCH: &[(&str, &str)] = &[
    // The original's x86 build converts numbers to integers by rounding
    // (docs/behavior/script-environment.md 2.5), with this file's x86 branch:
    // add 2^52 + 2^51 and take the low 32 bits of the double, which rounds to
    // nearest (ties to even) and wraps out-of-range values. Every target we
    // build for would take the plain cast, which truncates; the same method
    // on all of them gives the x86 results, in range and out of it.
    (
        "#define lua_number2int(i,d)\t((i)=(int)(d))\n\
         #define lua_number2integer(i,d)\t((i)=(lua_Integer)(d))\n",
        "static inline int volya_number2int(double d) {\n\
         \x20 union { double d; unsigned long long u; } c;\n\
         \x20 c.d = d + 6755399441055744.0;\n\
         \x20 return (int)(unsigned int)(c.u & 0xffffffffu);\n\
         }\n\
         #define lua_number2int(i,d)\t((i)=volya_number2int(d))\n\
         #define lua_number2integer(i,d)\t((i)=(lua_Integer)volya_number2int(d))\n",
    ),
    // One string-to-number conversion on every target (src/libc.rs):
    // C library `strtod`s differ (C99 ones also read hexadecimal, `inf` and
    // `nan`); Lua 5.1 alpha is written for ANSI C's decimal-only one.
    (
        "#define lua_str2number(s,p)\tstrtod((s), (p))\n",
        "double volya_strtod(const char *s, char **end);\n\
         #define lua_str2number(s,p)\tvolya_strtod((s), (p))\n",
    ),
];

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let wasm = env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32";
    println!("cargo:rerun-if-changed=vendor");
    println!("cargo:rerun-if-changed=csrc");

    let src = out.join("luajit");
    copy_patched(&root.join(VENDOR), &src);

    let mut build = cc::Build::new();
    build
        .include(&src)
        .opt_level(2)
        .warnings(false)
        .flag_if_supported("-w")
        .flag("-include")
        .flag(root.join("csrc/volya_rand.h").to_str().unwrap());
    if wasm {
        wasm_toolchain(&mut build, &root);
    }
    for name in VENDORED {
        build.file(src.join(format!("{name}.c")));
    }
    build.compile("volya_luajit");

    let mut shims = cc::Build::new();
    shims
        .include(&src)
        .opt_level(2)
        .warnings(true)
        .extra_warnings(true);
    if wasm {
        wasm_toolchain(&mut shims, &root);
        shims.file(root.join("csrc/wasm/libc.c"));
        shims.file(root.join("csrc/wasm/c_longjmp.S"));
    }
    for name in SHIMS {
        shims.file(root.join(format!("csrc/{name}.c")));
    }
    shims.compile("volya_luajit_shims");
}

/// Copies the vendored sources to `to`, patching `luaconf.h`.
fn copy_patched(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        let mut text = fs::read_to_string(&path).unwrap();
        if name == "luaconf.h" {
            for (from, to) in LUACONF_PATCH {
                assert_eq!(text.matches(from).count(), 1, "luaconf.h patch: {from:?}");
                text = text.replacen(from, to, 1);
            }
        }
        fs::write(to.join(name), text).unwrap();
    }
}

/// LLVM clang for wasm32, our C library's headers, and setjmp/longjmp
/// lowered to WebAssembly exception handling (csrc/wasm/libc.c).
fn wasm_toolchain(build: &mut cc::Build, root: &Path) {
    let bin = llvm_bin();
    build
        .compiler(bin.join("clang"))
        .archiver(bin.join("llvm-ar"))
        .ranlib(bin.join("llvm-ranlib"))
        .flag("-nostdlibinc")
        .include(root.join("csrc/wasm/include"))
        .flag("-mexception-handling")
        .flag("-mllvm")
        .flag("-wasm-enable-sjlj")
        // The legacy encoding: the widest browser support (docs/architecture.md, Scripts).
        .flag("-mllvm")
        .flag("-wasm-use-legacy-eh=true");
}

/// The `bin` directory of Homebrew's LLVM (`brew install llvm`): Apple's
/// clang has no WebAssembly target.
fn llvm_bin() -> PathBuf {
    let has_clang = |prefix: &Path| prefix.join("bin/clang").is_file();
    let known = ["/opt/homebrew/opt/llvm", "/usr/local/opt/llvm"].map(PathBuf::from);
    if let Some(prefix) = known.iter().find(|p| has_clang(p)) {
        return prefix.join("bin");
    }
    let brew = Command::new("brew").args(["--prefix", "llvm"]).output();
    if let Ok(out) = brew
        && out.status.success()
    {
        let prefix = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        if has_clang(&prefix) {
            return prefix.join("bin");
        }
    }
    panic!(
        "volya-engine-script: building for wasm32 needs Homebrew's LLVM clang \
         (`brew install llvm`); no clang found at /opt/homebrew/opt/llvm/bin, \
         /usr/local/opt/llvm/bin or `brew --prefix llvm`/bin"
    );
}
