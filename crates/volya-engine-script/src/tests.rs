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

/// Game object basics' checks, one test per section of
/// docs/behavior/script-api-game-object-basics.md (all of them run in
/// [`selftest`] too, on every target).
mod game_object {
    use crate::selftest::game_object as checks;

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
    fn the_handle_and_kinds() {
        section(checks::handle);
    }

    #[test]
    fn identity() {
        section(checks::identity);
    }

    #[test]
    fn place() {
        section(checks::place);
    }

    #[test]
    fn condition() {
        section(checks::condition);
    }

    #[test]
    fn zones_restrictors_use_and_visibility() {
        section(checks::zones);
    }

    #[test]
    fn trade_parameters_and_names() {
        section(checks::trade);
    }

    #[test]
    fn constants() {
        section(checks::constants);
    }

    #[test]
    fn hit() {
        section(checks::hit);
    }

    #[test]
    fn cgameobject_client_classes_and_explosive() {
        section(checks::engine_classes);
    }

    #[test]
    fn unregistered_names() {
        section(checks::unregistered);
    }

    #[test]
    fn the_door_scheme() {
        section(checks::door);
    }
}

/// Object binders' checks, one test per section of
/// docs/behavior/script-api-object-binders.md (all of them run in
/// [`selftest`] too, on every target).
mod object_binder {
    use crate::selftest::object_binder as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn the_class() {
        section(checks::class);
    }

    #[test]
    fn the_binding() {
        section(checks::binding);
    }

    #[test]
    fn bind_object_and_binded_object() {
        section(checks::bind_object);
    }

    #[test]
    fn spawning() {
        section(checks::spawning);
    }

    #[test]
    fn update_and_saving() {
        section(checks::update_and_saving);
    }

    #[test]
    fn release_and_destroying() {
        section(checks::destroying);
    }
}

/// The story state's checks, one test per section of
/// docs/behavior/script-api-info-portions-tasks-news-and-map-spots.md (all
/// of them run in [`selftest`] too, on every target).
mod story {
    use crate::selftest::story as checks;

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
    fn information_portions() {
        section(checks::portions);
    }

    #[test]
    fn task_values() {
        section(checks::task_values);
    }

    #[test]
    fn the_players_tasks() {
        section(checks::player_tasks);
    }

    #[test]
    fn callbacks_raised() {
        section(checks::callbacks);
    }

    #[test]
    fn objective_states() {
        section(checks::states);
    }

    #[test]
    fn the_task_functions() {
        section(checks::functions);
    }

    #[test]
    fn news_and_talk_messages() {
        section(checks::news);
    }

    #[test]
    fn use_tips() {
        section(checks::tips);
    }

    #[test]
    fn map_spots() {
        section(checks::map_spots);
    }

    #[test]
    fn game_difficulty() {
        section(checks::difficulty);
    }

    #[test]
    fn tutorials() {
        section(checks::tutorials);
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

/// The engine entry points' checks, one test per section of
/// docs/behavior/script-api-engine-entry-points.md (all of them run in
/// [`selftest`] too, on every target).
mod entry_points {
    use crate::selftest::entry_points as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn finding_and_calling() {
        section(checks::finding_and_calling);
    }

    #[test]
    fn the_common_script_list() {
        section(checks::initialize);
    }

    #[test]
    fn the_game_type_function() {
        section(checks::game_type);
    }

    #[test]
    fn the_start_callback() {
        section(checks::start_callback);
    }

    #[test]
    fn the_tutorial_thresholds() {
        section(checks::tutorial_thresholds);
    }

    #[test]
    fn the_weapon_parameters() {
        section(checks::weapon_parameters);
    }

    #[test]
    fn the_new_game_call_order() {
        section(checks::call_order);
    }
}

/// Object event callbacks' checks, one test per section of
/// docs/behavior/script-api-object-event-callbacks.md (all of them run in
/// [`selftest`] too, on every target).
mod object_callbacks {
    use crate::selftest::object_callbacks::SECTIONS;

    #[test]
    fn every_section() {
        let failures: Vec<String> = SECTIONS
            .iter()
            .filter_map(|(what, f)| f().err().map(|e| format!("{what}: {e}")))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

/// Script API Y's checks, one test per section of
/// docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md (all
/// of them run in [`selftest`] too, on every target).
mod physics {
    use crate::selftest::physics as checks;

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
    fn the_members_of_game_object() {
        section(checks::game_object_members);
    }

    #[test]
    fn physics_shell() {
        section(checks::shell);
    }

    #[test]
    fn physics_element() {
        section(checks::element);
    }

    #[test]
    fn physics_joint() {
        section(checks::joint);
    }

    #[test]
    fn holder_and_car() {
        section(checks::car);
    }

    #[test]
    fn helicopter() {
        section(checks::helicopter);
    }

    #[test]
    fn hanging_lamp() {
        section(checks::lamp);
    }

    #[test]
    fn the_door_scheme() {
        section(checks::door_scheme);
    }
}

/// The dialog files' functions' checks, one test per section of
/// docs/behavior/script-api-dialogs-and-trade.md 3 (all of them run in
/// [`selftest`] too, on every target).
mod dialogs {
    use crate::selftest::dialogs::SECTIONS;

    #[test]
    fn every_section() {
        let failures: Vec<String> = SECTIONS
            .iter()
            .filter_map(|(what, f)| f().err().map(|e| format!("{what}: {e}")))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

/// The dialogs and trade's checks, one test per section of
/// docs/behavior/script-api-dialogs-and-trade.md (all of them run in
/// [`selftest`] too, on every target).
mod dialogs_and_trade {
    use crate::selftest::dialogs_and_trade as checks;

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
    fn the_phrase_classes() {
        section(checks::phrase_classes);
    }

    #[test]
    fn the_one_object_forms() {
        section(checks::one_object_forms);
    }

    #[test]
    fn init_functions() {
        section(checks::init_func);
    }

    #[test]
    fn talk() {
        section(checks::talk);
    }

    #[test]
    fn the_start_dialog() {
        section(checks::start_dialog);
    }

    #[test]
    fn trade_enabled() {
        section(checks::trade_enabled);
    }

    #[test]
    fn the_trade_condition_functions() {
        section(checks::trade_conditions);
    }

    #[test]
    fn factors_and_price() {
        section(checks::factors_and_price);
    }

    #[test]
    fn trader_members() {
        section(checks::trader_members);
    }

    #[test]
    fn game_difficulty() {
        section(checks::game_difficulty);
    }

    #[test]
    fn the_worked_example() {
        section(checks::worked_example);
    }
}

/// The characters' talk state's checks, one test per section of
/// docs/behavior/character-talk-state.md (all of them run in [`selftest`]
/// too, on every target).
mod character_talk_state {
    use crate::selftest::character_talk_state as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn which_objects_can_talk() {
        section(checks::which_objects_can_talk);
    }

    #[test]
    fn the_talk_enabled_flag() {
        section(checks::talk_enabled_flag);
    }

    #[test]
    fn the_characters_side() {
        section(checks::side);
    }

    #[test]
    fn the_start_dialog() {
        section(checks::start_dialog);
    }
}

/// The time names' checks, one test per section of
/// docs/behavior/script-api-time.md (all of them run in [`selftest`] too,
/// on every target).
mod time {
    use crate::selftest::time as checks;

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
    fn game_time_values() {
        section(checks::game_time_values);
    }

    #[test]
    fn the_clocks() {
        section(checks::clocks);
    }

    #[test]
    fn the_level_time_functions() {
        section(checks::level_functions);
    }

    #[test]
    fn ctime_constructors() {
        section(checks::constructors);
    }

    #[test]
    fn ctime_setting_and_reading() {
        section(checks::setting_and_reading);
    }

    #[test]
    fn ctime_arithmetic() {
        section(checks::arithmetic);
    }

    #[test]
    fn ctime_operators() {
        section(checks::operators);
    }

    #[test]
    fn ctime_texts() {
        section(checks::texts);
    }

    #[test]
    fn time_global() {
        section(checks::time_global);
    }

    #[test]
    fn profile_timer() {
        section(checks::profile_timer);
    }
}

/// The sound's checks, one test per section of
/// docs/behavior/script-api-sound.md (all of them run in [`selftest`] too,
/// on every target).
mod sound {
    use crate::selftest::sound as checks;

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
    fn construction() {
        section(checks::construction);
    }

    #[test]
    fn play_and_play_at_pos() {
        section(checks::play);
    }

    #[test]
    fn play_no_feedback() {
        section(checks::play_no_feedback);
    }

    #[test]
    fn stop() {
        section(checks::stop);
    }

    #[test]
    fn playing_and_length() {
        section(checks::playing_and_length);
    }

    #[test]
    fn positions() {
        section(checks::positions);
    }

    #[test]
    fn properties() {
        section(checks::properties);
    }

    #[test]
    fn a_collected_objects_sound_goes() {
        section(checks::collection);
    }

    #[test]
    fn snd_type() {
        section(checks::snd_type);
    }

    #[test]
    fn the_records() {
        section(checks::records);
    }

    #[test]
    fn the_effects_factor() {
        section(checks::effects_factor);
    }

    #[test]
    fn prefetch_sound() {
        section(checks::prefetch_sound);
    }

    #[test]
    fn iterate_sounds() {
        section(checks::iterate_sounds);
    }

    #[test]
    fn add_sound() {
        section(checks::add_sound);
    }

    #[test]
    fn play_sound() {
        section(checks::play_sound);
    }

    #[test]
    fn masks_and_counts() {
        section(checks::masks_and_counts);
    }

    #[test]
    fn prefixes() {
        section(checks::prefixes);
    }

    #[test]
    fn the_traders_external_sound() {
        section(checks::external_sound);
    }

    #[test]
    fn hearing_and_remembered_sounds() {
        section(checks::hearing);
    }

    #[test]
    fn the_worked_examples() {
        section(checks::worked_examples);
    }
}

/// The simulation registry's checks, one test per section of
/// docs/behavior/script-api-simulation-registry-and-game-graph.md (all of
/// them run in [`selftest`] too, on every target).
mod simulation {
    use crate::selftest::simulation as checks;

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
    fn alife() {
        section(checks::alife);
    }

    #[test]
    fn finding_objects() {
        section(checks::finding_objects);
    }

    #[test]
    fn story_ids() {
        section(checks::story_ids);
    }

    #[test]
    fn levels() {
        section(checks::levels);
    }

    #[test]
    fn switching() {
        section(checks::switching);
    }

    #[test]
    fn creating_and_releasing() {
        section(checks::creating_and_releasing);
    }

    #[test]
    fn restrictions() {
        section(checks::restrictions);
    }

    #[test]
    fn kill_entity() {
        section(checks::kill_entity);
    }

    #[test]
    fn game_graph() {
        section(checks::game_graph);
    }

    #[test]
    fn smart_terrain_tasks() {
        section(checks::tasks);
    }

    #[test]
    fn brains_and_movement() {
        section(checks::brains);
    }

    #[test]
    fn has_info() {
        section(checks::has_info);
    }
}

/// The simulation objects' checks, one test per section of
/// docs/behavior/script-api-simulation-objects.md (all of them run in
/// [`selftest`] too, on every target).
mod simulation_objects {
    use crate::selftest::simulation_objects as checks;

    fn section(f: fn() -> Result<(), String>) {
        if let Err(failures) = f() {
            panic!("{failures}");
        }
    }

    #[test]
    fn classes() {
        section(checks::class_list);
    }

    #[test]
    fn constructing_and_init() {
        section(checks::constructing);
    }

    #[test]
    fn properties() {
        section(checks::properties);
    }

    #[test]
    fn cse_abstract() {
        section(checks::abstract_methods);
    }

    #[test]
    fn state_flags() {
        section(checks::state_flags);
    }

    #[test]
    fn kinds() {
        section(checks::kinds);
    }

    #[test]
    fn dispatch() {
        section(checks::dispatch);
    }

    #[test]
    fn engine_calls() {
        section(checks::engine_calls);
    }

    #[test]
    fn spawn_data() {
        section(checks::spawn_data);
    }
}

/// The scripted actions' checks, one test per section of
/// docs/behavior/script-api-scripted-actions.md (all of them run in
/// [`selftest`] too, on every target).
mod scripted_actions {
    use crate::selftest::scripted_actions as checks;

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
    fn scripted_objects() {
        section(checks::scripted_objects);
    }

    #[test]
    fn script_control() {
        section(checks::script_control);
    }

    #[test]
    fn the_queue() {
        section(checks::queue);
    }

    #[test]
    fn running_the_queue() {
        section(checks::running);
    }

    #[test]
    fn entity_action() {
        section(checks::entity_action);
    }

    #[test]
    fn completeness() {
        section(checks::completeness);
    }

    #[test]
    fn move_parts() {
        section(checks::movement);
    }

    #[test]
    fn look_parts() {
        section(checks::look);
    }

    #[test]
    fn anim_parts() {
        section(checks::anim);
    }

    #[test]
    fn sound_parts() {
        section(checks::sound);
    }

    #[test]
    fn particle_parts() {
        section(checks::particle);
    }

    #[test]
    fn object_parts() {
        section(checks::object);
    }

    #[test]
    fn act_parts() {
        section(checks::act);
    }
}

/// The window description files' checks, one test per section of
/// docs/behavior/script-api-window-description-files-fonts-and-textures.md
/// (all of them run in [`selftest`] too, on every target).
mod window_files {
    use crate::selftest::window_files as checks;

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
    fn the_reader_and_parse_file() {
        section(checks::reader);
    }

    #[test]
    fn building_and_attaching() {
        section(checks::attaching);
    }

    #[test]
    fn the_24_methods() {
        section(checks::methods);
    }

    #[test]
    fn init_window_and_init_auto_static_group() {
        section(checks::init_window);
    }

    #[test]
    fn the_worked_example() {
        section(checks::worked_example);
    }

    #[test]
    fn get_font() {
        section(checks::fonts);
    }

    #[test]
    fn cgamefont() {
        section(checks::font_constants);
    }

    #[test]
    fn get_argb() {
        section(checks::get_argb);
    }

    #[test]
    fn texture_descriptions() {
        section(checks::textures);
    }

    #[test]
    fn translate_string() {
        section(checks::translate_string);
    }
}

/// Script API X's checks, one test per section of
/// docs/behavior/script-api-movement-sight-and-memory.md (all of them run
/// in [`selftest`] too, on every target).
mod movement_sight_and_memory {
    use crate::selftest::movement_sight_and_memory as checks;

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
    fn kinds_and_fallbacks() {
        section(checks::kinds);
    }

    #[test]
    fn stalker_movement() {
        section(checks::stalker_movement);
    }

    #[test]
    fn paths_and_patrol_paths() {
        section(checks::paths);
    }

    #[test]
    fn space_restrictions() {
        section(checks::restrictions);
    }

    #[test]
    fn sight() {
        section(checks::sight);
    }

    #[test]
    fn vision_and_memory() {
        section(checks::vision_and_memory);
    }

    #[test]
    fn enemies_and_dangers() {
        section(checks::enemies);
    }

    #[test]
    fn covers() {
        section(checks::covers);
    }

    #[test]
    fn items_weapons_and_animations() {
        section(checks::items);
    }

    #[test]
    fn monsters() {
        section(checks::monsters);
    }

    #[test]
    fn cef_storage() {
        section(checks::cef_storage);
    }

    #[test]
    fn absent_names() {
        section(checks::absent);
    }
}

/// The inventory's checks, one test per section of
/// docs/behavior/script-api-inventory.md (all of them run in [`selftest`]
/// too, on every target).
mod inventory {
    use crate::selftest::inventory as checks;

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
    fn items() {
        section(checks::items);
    }

    #[test]
    fn iterate_inventory() {
        section(checks::iterate_inventory);
    }

    #[test]
    fn inventory_for_each() {
        section(checks::inventory_for_each);
    }

    #[test]
    fn slots() {
        section(checks::slots);
    }

    #[test]
    fn activate_slot() {
        section(checks::activate_slot);
    }

    #[test]
    fn drop_and_transfer() {
        section(checks::drop_and_transfer);
    }

    #[test]
    fn dropped_marks() {
        section(checks::dropped_marks);
    }

    #[test]
    fn eat() {
        section(checks::eat);
    }

    #[test]
    fn buy_supplies() {
        section(checks::buy_supplies);
    }

    #[test]
    fn money() {
        section(checks::money);
    }

    #[test]
    fn ammunition() {
        section(checks::ammunition);
    }

    #[test]
    fn other_item_members() {
        section(checks::other_items);
    }

    #[test]
    fn the_worked_example() {
        section(checks::worked_example);
    }
}

/// Script API N's checks, one test per section of
/// docs/behavior/script-api-level.md (all of them run in [`selftest`] too,
/// on every target).
mod level {
    use crate::selftest::level::SECTIONS;

    #[test]
    fn every_section() {
        let failures: Vec<String> = SECTIONS
            .iter()
            .filter_map(|(what, f)| f().err().map(|e| format!("{what}: {e}")))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

/// docs/behavior/script-api-effects-and-weather.md's checks (all of them
/// run in [`selftest`] too, on every target).
mod effects_and_weather {
    use crate::selftest::effects_and_weather::SECTIONS;

    #[test]
    fn every_section() {
        let failures: Vec<String> = SECTIONS
            .iter()
            .filter_map(|(what, f)| f().err().map(|e| format!("{what}: {e}")))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

/// Script API Q's checks, one test per section of
/// docs/behavior/script-api-relations-community-and-ranking.md (all of
/// them run in [`selftest`] too, on every target).
mod relations {
    use crate::selftest::relations::SECTIONS;

    #[test]
    fn every_section() {
        let failures: Vec<String> = SECTIONS
            .iter()
            .filter_map(|(what, f)| f().err().map(|e| format!("{what}: {e}")))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
