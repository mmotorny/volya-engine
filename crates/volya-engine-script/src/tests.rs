// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::ffi::{CStr, c_char};

use super::*;

#[test]
fn selftest() {
    if let Err(failures) = selftest::check() {
        panic!("{failures}");
    }
}

/// The script classes' checks, one test per section of
/// docs/behavior/script-api-script-classes-deriving-from-engine-classes.md
/// (all of them run in [`selftest`] too, on every target).
mod classes {
    use crate::selftest::classes as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn declaring_a_class() {
        section(checks::declaring);
    }

    #[test]
    fn constructing() {
        section(checks::constructing);
    }

    #[test]
    fn super_calls() {
        section(checks::super_calls);
    }

    #[test]
    fn members_of_objects_and_classes() {
        section(checks::members);
    }

    #[test]
    fn engine_calls_into_overrides() {
        section(checks::engine_calls);
    }

    #[test]
    fn base_class_calls() {
        section(checks::base_calls);
    }

    #[test]
    fn finalize_and_operators() {
        section(checks::finalize_and_operators);
    }
}

/// The object factory's checks, one test per section of
/// docs/behavior/script-api-class-ids-and-the-object-factory.md (all of
/// them run in [`selftest`] too, on every target).
mod object_factory {
    use crate::selftest::object_factory as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn class_ids() {
        section(checks::class_ids);
    }

    #[test]
    fn the_object_factory() {
        section(checks::factory);
    }

    #[test]
    fn register() {
        section(checks::register);
    }

    #[test]
    fn class_registrators() {
        section(checks::registrators);
    }

    #[test]
    fn clsid() {
        section(checks::clsid);
    }

    #[test]
    fn creation_through_the_factory() {
        section(checks::creation);
    }
}

/// The engine services' checks, one test per section of
/// docs/behavior/script-api-engine-services.md (all of them run in
/// [`selftest`] too, on every target).
mod services {
    use crate::selftest::services as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn what_is_registered() {
        section(checks::registered);
    }

    #[test]
    fn the_console() {
        section(checks::console);
    }

    #[test]
    fn the_render_device() {
        section(checks::device);
    }

    #[test]
    fn global_functions() {
        section(checks::functions);
    }

    #[test]
    fn classes() {
        section(checks::classes);
    }
}

/// Configuration and files' checks, one test per section of
/// docs/behavior/script-api-configuration-and-files.md (all of them run in
/// [`selftest`] too, on every target).
mod files {
    use crate::selftest::files as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn what_is_registered() {
        section(checks::registered);
    }

    #[test]
    fn configuration_objects() {
        section(checks::configuration_objects);
    }

    #[test]
    fn readers() {
        section(checks::readers);
    }

    #[test]
    fn token_classes() {
        section(checks::tokens);
    }

    #[test]
    fn the_file_system() {
        section(checks::file_system);
    }

    #[test]
    fn command_line_and_user_name() {
        section(checks::command_line);
    }

    #[test]
    fn the_logic_reads_of_a_door() {
        section(checks::logic_reads);
    }
}

/// The value types' checks, one test per section of
/// docs/behavior/script-api-value-types-and-math.md (all of them run in
/// [`selftest`] too, on every target).
mod values {
    use crate::selftest::values as checks;
    use crate::values::{X, to_bits32, to_flags16, to_flags32};

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn items() {
        section(checks::items);
    }

    #[test]
    fn common_rules() {
        section(checks::common_rules);
    }

    #[test]
    fn vector() {
        section(checks::vector);
    }

    #[test]
    fn matrix() {
        section(checks::matrix);
    }

    #[test]
    fn rectangles_boxes_and_points() {
        section(checks::rectangles);
    }

    #[test]
    fn colours() {
        section(checks::colours);
    }

    #[test]
    fn flags() {
        section(checks::flags);
    }

    #[test]
    fn bit_functions() {
        section(checks::bit_functions);
    }

    #[test]
    fn worked_examples() {
        section(checks::worked_examples);
    }

    /// The conversions' edges (2.2, 8).
    #[test]
    fn conversions() {
        assert_eq!(to_flags32(f64::NAN), 0);
        assert_eq!(to_flags32(-9.3e18), 0);
        assert_eq!(to_flags32(-(2.0f64.powi(40) + 5.9)), 4_294_967_291);
        assert_eq!(to_flags16(-2_147_483_648.5), 0);
        assert_eq!(to_flags16(2_147_483_648.0), 0);
        assert_eq!(to_flags16(-65_537.0), 0xFFFF);
        assert_eq!(to_bits32(-2_147_483_649.0), i32::MIN);
        assert_eq!(to_bits32(f64::INFINITY), i32::MIN);
    }

    /// The x87 unit's 24-bit rounding (2.5): ties to even, and the wider
    /// exponent range keeps values f32 would make subnormal.
    #[test]
    fn x87_rounding() {
        let one = X(1.0);
        assert_eq!((X(1.0 + 2.0f64.powi(-24)) * one).0, 1.0);
        assert_eq!(
            (X(1.0 + 3.0 * 2.0f64.powi(-24)) * one).0,
            1.0 + 2.0f64.powi(-22)
        );
        assert_eq!(
            (X(2.0 - 2.0f64.powi(-25)) * one).0,
            2.0,
            "a carry into the exponent"
        );
        let tiny = 2.0f64.powi(-140) * (1.0 + 2.0f64.powi(-23));
        assert_eq!((X(tiny) * one).0, tiny);
        assert_eq!(X(tiny).store(), tiny as f32);
    }
}

#[test]
fn rust_panics_become_lua_errors() {
    let mut lua = Lua::new().unwrap();
    lua.register("rust_panic", |_| panic!("deliberate"))
        .unwrap();
    let results = lua.exec("=panic", b"return pcall(rust_panic)").unwrap();
    assert_eq!(results, [false.into(), "a Rust function panicked".into()]);
    // The state is still usable.
    assert_eq!(lua.exec("=after", b"return 1 + 1").unwrap(), [2.0.into()]);
}

#[test]
fn a_state_allocates_from_the_game_scripts_pool() {
    use volya_engine_core::memory_manager::GAME_SCRIPTS;
    let before = GAME_SCRIPTS.in_use_bytes();
    let mut lua = Lua::new().unwrap();
    // 8 MB kept by the state; other tests' states, made and closed on
    // other threads meanwhile, are far smaller.
    lua.exec("=big", b"big = string.rep('x', 8000000)").unwrap();
    let kept = GAME_SCRIPTS.in_use_bytes();
    assert!(kept > before + 7_000_000);
    drop(lua);
    // Measured over the drop alone: other tests' states come and go
    // meanwhile on other threads, several at once.
    assert!(GAME_SCRIPTS.in_use_bytes() < kept - 7_000_000);
}

#[test]
fn states_are_independent() {
    let mut a = Lua::new().unwrap();
    let mut b = Lua::new().unwrap();
    a.exec("=a", b"x = 1").unwrap();
    assert_eq!(b.exec("=b", b"return x").unwrap(), [Value::Nil]);
    assert_eq!(a.exec("=a", b"return x").unwrap(), [1.0.into()]);
}

#[test]
fn names_with_nul_are_rejected() {
    let mut lua = Lua::new().unwrap();
    let error = lua.call("a\0b", &[]).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Api);
}

/// Our wasm `printf` (csrc/volya_printf.c) against the system's.
#[test]
fn printf_matches_the_system() {
    unsafe extern "C" {
        fn snprintf(buf: *mut c_char, cap: usize, fmt: *const c_char, ...) -> c_int;
    }
    let ours = |f: &dyn Fn(*mut c_char) -> c_int| {
        let mut buf = [0 as c_char; 600];
        let n = f(buf.as_mut_ptr());
        // SAFETY: snprintf NUL-terminates.
        let s = unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (n, s)
    };
    let doubles = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        1.5,
        2.5,
        0.125,
        0.25,
        0.375,
        1.005,
        std::f64::consts::PI,
        1.0 / 3.0,
        2.0 / 3.0,
        100.0,
        1e5,
        1e6,
        1e-4,
        1e-5,
        123456.789,
        1e15,
        1e16,
        1e21,
        1e100,
        1e300,
        1e-300,
        5e-324,
        f64::MAX,
        f64::MIN_POSITIVE,
        9007199254740992.0,
        0.1,
        0.2,
        0.3,
        123456789012345.0,
        999999.5,
        9.9999995,
        0.000099999,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    let double_formats = [
        "%.14g",
        "%g",
        "%G",
        "%e",
        "%E",
        "%f",
        "%.0f",
        "%.1f",
        "%.2f",
        "%.3e",
        "%.0e",
        "%#.0e",
        "%#.0f",
        "%#g",
        "%#.3g",
        "%.1g",
        "%.0g",
        "%.17g",
        "%.20f",
        "%10.3f",
        "%-10.3f|",
        "%+.2e",
        "% g",
        "%010.2f",
        "%+010.2e",
        "%-+12.4g|",
        "%5.1G",
        "%.99f",
    ];
    for v in doubles {
        for fmt in double_formats {
            let c = std::ffi::CString::new(fmt).unwrap();
            // SAFETY: matching format and arguments, buffers of the given size.
            let theirs = ours(&|buf| unsafe { snprintf(buf, 600, c.as_ptr(), v) });
            let mine = ours(&|buf| unsafe { ffi::volya_snprintf(buf, 600, c.as_ptr(), v) });
            assert_eq!(mine, theirs, "{fmt} of {v:e}");
        }
    }
    let ints = [0, 1, -1, 7, 42, 255, -255, 65535, i32::MAX, i32::MIN];
    let int_formats = [
        "%d", "%i", "%5d", "%-5d|", "%05d", "%+d", "% d", "%.3d", "%8.3d", "%x", "%X", "%#x",
        "%#X", "%o", "%#o", "%u", "%c", "%-3c|", "%#.0o", "%.0d", "%08x", "%-#8x|",
    ];
    for v in ints {
        for fmt in int_formats {
            if fmt.contains('c') && !(0..128).contains(&v) {
                continue;
            }
            let c = std::ffi::CString::new(fmt).unwrap();
            // SAFETY: as above.
            let theirs = ours(&|buf| unsafe { snprintf(buf, 600, c.as_ptr(), v) });
            let mine = ours(&|buf| unsafe { ffi::volya_snprintf(buf, 600, c.as_ptr(), v) });
            assert_eq!(mine, theirs, "{fmt} of {v}");
        }
    }
    for fmt in ["%s", "%10s", "%-10s|", "%.2s", "%5.1s", "%%|%s"] {
        let c = std::ffi::CString::new(fmt).unwrap();
        let s = c"text";
        // SAFETY: as above.
        let theirs = ours(&|buf| unsafe { snprintf(buf, 600, c.as_ptr(), s.as_ptr()) });
        let mine = ours(&|buf| unsafe { ffi::volya_snprintf(buf, 600, c.as_ptr(), s.as_ptr()) });
        assert_eq!(mine, theirs, "{fmt}");
    }
    for long in [0_i64, -1, i64::from(i32::MAX) + 1, i64::MIN] {
        // SAFETY: as above.
        let theirs =
            ours(&|buf| unsafe { snprintf(buf, 600, c"%ld|%lld|%lu".as_ptr(), long, long, long) });
        let mine = ours(&|buf| unsafe {
            ffi::volya_snprintf(buf, 600, c"%ld|%lld|%lu".as_ptr(), long, long, long)
        });
        assert_eq!(mine, theirs, "long {long}");
    }
    // Truncation returns the full length.
    let mut small = [0 as c_char; 4];
    // SAFETY: as above.
    let n = unsafe { ffi::volya_snprintf(small.as_mut_ptr(), 4, c"%d".as_ptr(), 123456) };
    assert_eq!(n, 6);
    // SAFETY: NUL-terminated.
    assert_eq!(unsafe { CStr::from_ptr(small.as_ptr()) }, c"123");
}

/// Our `strtod` against the system's, on decimal input.
#[test]
fn strtod_matches_the_system_on_decimals() {
    unsafe extern "C" {
        fn strtod(s: *const c_char, end: *mut *mut c_char) -> f64;
    }
    let inputs = [
        "0",
        "-0",
        "1",
        "  42",
        "\t\n-3.5x",
        "+.5",
        "5.",
        ".",
        "-.",
        "1e5",
        "1E+5",
        "1e-5",
        "1e",
        "1e+",
        "2.5e3z",
        "0.1",
        "0.30000000000000004",
        "123456789012345678901234567890",
        "1e400",
        "-1e400",
        "1e-400",
        "4.9e-324",
        "2.2250738585072014e-308",
        "abc",
        "",
        " ",
        "- 1",
        "00012",
        "1.7976931348623157e308",
        "9007199254740993",
        "0.000001234",
    ];
    for input in inputs {
        let c = std::ffi::CString::new(input).unwrap();
        let mut their_end = std::ptr::null_mut();
        let mut my_end = std::ptr::null_mut();
        // SAFETY: NUL-terminated input, writable end pointers.
        let (theirs, mine) = unsafe {
            (
                strtod(c.as_ptr(), &mut their_end),
                libc::volya_strtod(c.as_ptr(), &mut my_end),
            )
        };
        assert_eq!(mine.to_bits(), theirs.to_bits(), "{input:?}");
        assert_eq!(my_end, their_end, "{input:?} end");
    }
    // C99 extras the ANSI C strtod doesn't read.
    for (input, len) in [("0x10", 1), ("inf", 0), ("nan", 0), ("0x1p4", 1)] {
        let c = std::ffi::CString::new(input).unwrap();
        let mut end = std::ptr::null_mut();
        // SAFETY: as above.
        unsafe { libc::volya_strtod(c.as_ptr(), &mut end) };
        // SAFETY: end points into c.
        assert_eq!(unsafe { end.offset_from(c.as_ptr()) }, len, "{input:?}");
    }
}
