// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The console's commands against docs/behavior/console-commands.md: its
//! measured examples, and its table row by row.

use super::*;

const DEFAULTS: &[u8] =
    b"bind quit kESCAPE\r\nbind forward kW\r\nbind jump kSPACE\r\nbind vote kF6\r\nbind quick_save kF6";

fn files() -> ConsoleFiles {
    let mut presets: [Vec<u8>; 5] = Default::default();
    presets[1] = b"r__geometry_lod .9\r\nr__tf_aniso 2\r\nph_iterations 20\r\n".to_vec();
    presets[4] = b"r__tf_aniso 16\n".to_vec();
    ConsoleFiles {
        default_controls: DEFAULTS.to_vec(),
        presets,
    }
}

/// The lines `line` prints.
fn printed(c: &mut Console, line: &str) -> Vec<String> {
    let from = c.log().len();
    c.exec(line);
    c.log()[from..].to_vec()
}

fn status(c: &Console, name: &str) -> String {
    c.status(name).unwrap()
}

#[test]
fn the_table_is_section_8s() {
    assert_eq!(COMMANDS.len(), SECTION_8.len());
    for (def, (name, built_in, help)) in COMMANDS.iter().zip(SECTION_8) {
        assert_eq!(def.name, *name);
        assert_eq!(help_text(def.name, def.kind), *help, "{name}");
        if let Some(built_in) = built_in {
            let c = Console::new(files());
            let shown = if def.designer() { "" } else { built_in };
            assert_eq!(c.status(name).unwrap_or_default(), shown, "{name}");
        }
    }
    // Byte order, `_` before `a`; 256 names, 251 without `designer`.
    assert!(
        COMMANDS
            .windows(2)
            .all(|w| w[0].name.as_bytes() < w[1].name.as_bytes())
    );
    assert_eq!(COMMANDS.len(), 256);
    assert_eq!(COMMANDS.iter().filter(|d| !d.designer()).count(), 251);
}

#[test]
fn lines_are_split_and_looked_up_exactly() {
    let mut c = Console::new(files());
    assert_eq!(c.exec("\t mouse_sens\t \t0.3   "), Exec::Done);
    assert_eq!(status(&c, "mouse_sens"), "0.3");
    assert_eq!(c.exec("      "), Exec::Ignored);
    assert!(c.log().is_empty(), "a line of spaces prints nothing");
    assert_eq!(
        printed(&mut c, "MOUSE_SENS"),
        ["! Unknown command:  MOUSE_SENS"]
    );
    assert_eq!(c.exec("Mouse_sens 0.2"), Exec::Unknown);
    assert_eq!(status(&c, "mouse_sens"), "0.3");
    // No separator for several commands.
    c.exec("hud_weapon on");
    assert_eq!(c.exec("hud_info off; hud_weapon off"), Exec::Rejected);
    assert!(c.on("hud_info") && c.on("hud_weapon"));
    // The designer's commands are not registered.
    assert_eq!(c.exec("demo_record_step 1"), Exec::Unknown);
}

#[test]
fn typed_lines_are_echoed() {
    let mut c = Console::new(files());
    c.enter("  hud_info\t\toff ");
    c.exec("hud_info");
    assert_eq!(c.log(), ["~ hud_info off ", "- hud_info off"]);
}

#[test]
fn arguments_are_lower_cased_except_three_commands() {
    let mut c = Console::new(files());
    assert_eq!(c.exec("mm_net_srv_name AbC  dEf"), Exec::Done);
    assert_eq!(status(&c, "mm_net_srv_name"), "AbC dEf");
    c.exec("load_last_save MySave");
    assert_eq!(c.last_save(), Some("mysave"));
}

#[test]
fn the_bare_name_shows_or_runs() {
    let mut c = Console::new(files());
    assert_eq!(printed(&mut c, "bind"), ["- bind "]);
    assert_eq!(printed(&mut c, "g_corpsenum"), ["- g_corpsenum 10"]);
    assert_eq!(printed(&mut c, "name"), ["- name "], "no game: empty");
    assert_eq!(c.exec("disconnect"), Exec::Action);
    assert_eq!(c.exec("cfg_load"), Exec::Shown);
    assert_eq!(c.exec("cfg_load other"), Exec::Action);
    assert_eq!(c.executions("g_corpsenum"), 1);
}

#[test]
fn flush_and_clear_log_write_the_log_file() {
    // console-log.md 3.3: created empty as the console is set up.
    let mut c = Console::new(files());
    assert_eq!(c.log_mut().take_file(), Some(Vec::new()));
    // 3.5: `flush`'s own line is not in the file it writes.
    c.enter("bogus");
    assert_eq!(c.exec("flush"), Exec::Done);
    assert_eq!(
        c.log_mut().take_file().unwrap(),
        b"~ bogus\r\n! Unknown command:  bogus\r\n"
    );
    assert_eq!(
        c.log().last().unwrap(),
        "* Log file has been saved successfully!"
    );
    // 3.6: the log and so the history are emptied, the file with them.
    assert_eq!(c.exec("clear_log"), Exec::Done);
    assert_eq!(c.log_mut().take_file(), Some(Vec::new()));
    assert_eq!(c.log(), ["* Log file has been cleaned successfully!"]);
}

#[test]
fn a_rejected_value_prints_the_valid_arguments() {
    let mut c = Console::new(files());
    for on in ["ON", "On", "1"] {
        c.exec("hud_info off");
        assert_eq!(c.exec(&format!("hud_info {on}")), Exec::Done);
        assert!(c.on("hud_info"));
    }
    for bad in ["yes", "2"] {
        assert_eq!(
            printed(&mut c, &format!("hud_info {bad}")),
            [
                "~ Invalid syntax in call to 'hud_info'",
                "~ Valid arguments: 'on/off' or '1/0'"
            ]
        );
    }
    assert_eq!(
        printed(&mut c, "g_game_difficulty bogus"),
        [
            "~ Invalid syntax in call to 'g_game_difficulty'",
            "~ Valid arguments: game difficulty"
        ]
    );
}

#[test]
fn startup_runs_the_renderer_lines_first() {
    let user = b"mouse_sens 0.3\r\n renderer renderer_r2\r\nRENDERER renderer_r2\r\nrenderer bogus\r\nrenderer \r\n";
    let c = Console::startup(files(), user);
    let bogus = [
        "~ Invalid syntax in call to 'renderer'",
        "~ Valid arguments: renderer_r1/renderer_r2a/renderer_r2",
    ];
    let first = [&bogus[..], &["- renderer renderer_r1"]].concat();
    // The second execution: every line; the leading space is dropped.
    let second = [
        &["! Unknown command:  RENDERER"][..],
        &bogus,
        &["- renderer renderer_r2"],
    ]
    .concat();
    assert_eq!(
        c.log(),
        [&first[..], &STAT_MEMORY[..], &second[..]].concat()
    );
}

/// `stat_memory`'s lines with no figure reported.
const STAT_MEMORY: [&str; 4] = [
    "* [win32]: free[0 K], reserved[0 K], committed[0 K]",
    "* [ D3D ]: textures[0 K]",
    "* [x-ray]: crt heap[0 K], process heap[0 K], game lua[0 K], engine lua[0 K], render[0 K]",
    "* [x-ray]: economy: strings[0 K], smem[0 K]",
];

#[test]
fn stat_memory_prints_the_four_lines_whatever_the_argument() {
    // console-diagnostics.md 2.1.
    let mut c = Console::new(files());
    assert_eq!(printed(&mut c, "stat_memory"), STAT_MEMORY);
    assert_eq!(printed(&mut c, "stat_memory extra words"), STAT_MEMORY);
    c.set_memory_statistics(MemoryStatistics {
        free: 2047,
        reserved: 1024,
        committed: 1 << 30,
        textures: 260_814 * 1024 + 1023,
        crt_heap: 6009 * 1024,
        process_heap: 724 * 1024,
        game_lua: 1952 * 1024,
        engine_lua: 171 * 1024,
        render: 5,
        strings: -1500,
        smem: 28_221 * 1024,
        cmem: 0,
    });
    assert_eq!(
        printed(&mut c, "stat_memory"),
        [
            "* [win32]: free[1 K], reserved[1 K], committed[1048576 K]",
            "* [ D3D ]: textures[260814 K]",
            "* [x-ray]: crt heap[6009 K], process heap[724 K], game lua[1952 K], engine lua[171 K], render[0 K]",
            // Toward zero.
            "* [x-ray]: economy: strings[-1 K], smem[28221 K]",
        ]
    );
}

#[test]
fn dump_resources_prints_thirteen_counts() {
    // 2.2: every label but `lst_constants` is ten characters long.
    assert!(
        RESOURCE_LABELS
            .iter()
            .all(|l| l.len() == 10 || *l == "lst_constants")
    );
    let mut c = Console::new(files());
    c.set_resource_counts(ResourceCounts([
        255, 2, 0, 6, 9, 6, 20, 219, 0, 0, 306, 306, 314,
    ]));
    let lines = printed(&mut c, "dump_resources 0");
    assert_eq!(lines.len(), 13);
    assert_eq!(lines[0], "* RM_Dump: textures  : 255");
    assert_eq!(lines[9], "* RM_Dump: lst_constants: 0");
    assert_eq!(lines[12], "* RM_Dump: v_shaders : 314");
    assert_eq!(printed(&mut c, "dump_resources"), lines);
    // 2.5: again as the device is destroyed at exit.
    let from = c.log().len();
    c.device_destroyed();
    assert_eq!(c.log()[from..], lines);
}

#[test]
fn dump_resources_at_startup_ends_the_program() {
    // 2.2, measured as a crash: nothing after it runs.
    let c = Console::startup(
        files(),
        b"dump_resources
hud_info off
",
    );
    assert!(c.fatal().is_some());
    assert!(c.on("hud_info"));
    // `stat_memory` and `dump_open_files` work there.
    let c = Console::startup(
        files(),
        b"dump_open_files
stat_memory
",
    );
    assert!(c.fatal().is_none());
    assert_eq!(
        c.log()[4..],
        [&["- dump_open_files "][..], &STAT_MEMORY].concat()
    );
}

#[test]
fn dump_open_files_lists_nothing_without_file_activity() {
    // 2.3: the argument is `atoi`'s; the bare name shows.
    let mut c = Console::new(files());
    assert_eq!(printed(&mut c, "dump_open_files"), ["- dump_open_files "]);
    for one in ["1", "1x", " 1", "+1"] {
        assert!(printed(&mut c, &format!("dump_open_files {one}")).is_empty());
    }
    for other in ["0", "2", "-1", "abc", "11"] {
        assert_eq!(
            printed(&mut c, &format!("dump_open_files {other}")),
            ["----un-used"],
            "{other}"
        );
    }
}

#[test]
fn a_load_prints_its_phases_then_stat_memory() {
    // 2.4.
    let mut c = Console::new(files());
    c.set_memory_statistics(MemoryStatistics {
        cmem: 26_124 * 1024 + 7,
        ..Default::default()
    });
    c.phase_caption(0, "Кэширование объектов...");
    c.load_ended(1234);
    assert_eq!(
        c.log()[..5],
        [
            "* phase time: 0 ms",
            "* phase cmem: 26124 K",
            "Кэширование объектов...",
            "* phase time: 1234 ms",
            "* phase cmem: 26124 K",
        ]
    );
    // Without the echo.
    assert_eq!(c.log()[5..], STAT_MEMORY);
}

#[test]
fn integers_read_as_atoi() {
    let mut c = Console::new(files());
    for bad in ["4294967306", "-4294967286", "99999999999"] {
        assert_eq!(
            c.exec(&format!("g_corpsenum {bad}")),
            Exec::Rejected,
            "{bad}"
        );
    }
    for bad in ["4294967295", "-4294967297", "-2147483649"] {
        assert_eq!(
            c.exec(&format!("sv_remove_corpse {bad}")),
            Exec::Rejected,
            "{bad}"
        );
    }
    for bad in ["101", "-1"] {
        assert_eq!(c.exec(&format!("g_corpsenum {bad}")), Exec::Rejected);
    }
    for (text, shown) in [
        ("100", "100"),
        ("12abc", "12"),
        ("+5", "5"),
        ("abc", "0"),
        ("0x10", "0"),
    ] {
        assert_eq!(c.exec(&format!("g_corpsenum {text}")), Exec::Done, "{text}");
        assert_eq!(status(&c, "g_corpsenum"), shown);
    }
}

#[test]
fn reals_read_as_atof_within_the_tolerance() {
    let mut c = Console::new(files());
    assert_eq!(c.exec("mouse_sens 0.6000099"), Exec::Done);
    assert_eq!(status(&c, "mouse_sens"), "0.60001");
    for bad in ["0.60002", "0.04", "abc"] {
        assert_eq!(
            c.exec(&format!("mouse_sens {bad}")),
            Exec::Rejected,
            "{bad}"
        );
    }
    c.exec("mouse_sens 0.123456789");
    assert_eq!(status(&c, "mouse_sens"), "0.12346");
    for (text, shown) in [
        ("abc", "0."),
        ("1e-1", "0.1"),
        ("0,5", "0."),
        ("1", "1."),
        ("0.5", "0.5"),
    ] {
        c.exec(&format!("cam_inert {text}"));
        assert_eq!(status(&c, "cam_inert"), shown, "{text}");
    }
    c.exec("r2_ls_depth_bias -0.0000004");
    assert_eq!(status(&c, "r2_ls_depth_bias"), "-0.");
}

#[test]
fn lists_match_ignoring_case() {
    let mut c = Console::new(files());
    assert_eq!(c.exec("g_game_difficulty GD_VETERAN"), Exec::Done);
    assert_eq!(status(&c, "g_game_difficulty"), "gd_veteran");
    // Stored numbers, not indices.
    c.exec("mm_net_srv_gamemode st_artefacthunt");
    assert_eq!(c.number("mm_net_srv_gamemode"), 7.0);
    assert_eq!(c.exec("_preset low extra"), Exec::Rejected);
}

#[test]
fn vectors_read_three_numbers() {
    let mut c = Console::new(files());
    for bad in ["0.1,0.2", "2,0,0"] {
        assert_eq!(
            c.exec(&format!("r2_aa_break {bad}")),
            Exec::Rejected,
            "{bad}"
        );
    }
    assert_eq!(c.exec("r2_aa_break 0.4, 0.5, 0.6"), Exec::Done);
    assert_eq!(status(&c, "r2_aa_break"), "0.400000,0.500000,0.600000");
    assert_eq!(c.exec("r2_aa_break 0.7,0.8,0.9x"), Exec::Done);
    assert_eq!(status(&c, "r2_aa_break"), "0.700000,0.800000,0.900000");
}

#[test]
fn texts_keep_63_characters() {
    let mut c = Console::new(files());
    let long = "x".repeat(70);
    assert_eq!(c.exec(&format!("cdkey {long}")), Exec::Done);
    assert_eq!(status(&c, "cdkey").len(), 63);
}

#[test]
fn commands_with_their_own_rules() {
    let mut c = Console::new(files());
    assert_eq!(
        printed(&mut c, "vid_mode abc"),
        ["! Wrong video mode [abc]"]
    );
    assert_eq!(status(&c, "vid_mode"), "1024x768");
    c.exec("vid_mode 800x600");
    assert_eq!(status(&c, "vid_mode"), "800x600");
    assert_eq!(c.video_mode(), (800, 600));
    c.exec("vid_mode 0x768trailing");
    assert_eq!(status(&c, "vid_mode"), "0x768");
    assert_eq!(c.video_mode(), (1, 768));
    for (text, shown) in [
        ("300", "200.00000"),
        ("10", "50.00000"),
        ("60", "60.00000"),
        ("abc", "50.00000"),
    ] {
        assert_eq!(c.exec(&format!("ph_frequency {text}")), Exec::Done);
        assert_eq!(status(&c, "ph_frequency"), shown, "{text}");
    }
    // `name` acts only in a game with a player.
    assert_eq!(c.exec("name someone"), Exec::Ignored);
    assert!(printed(&mut c, "name a/b").is_empty());
}

#[test]
fn help_lists_every_command() {
    let mut c = Console::new(files());
    let lines = printed(&mut c, "help anything");
    assert_eq!(lines.len(), 251 + 2);
    assert_eq!(lines[0], "- --- Command listing: start ---");
    assert_eq!(lines[252], "- --- Command listing: end ----");
    assert_eq!(
        lines[1],
        "_preset              (Default   ) --- Minimum/Low/Default/High/Extreme"
    );
    assert!(
        lines.contains(&"mm_mm_net_srv_dedicated (off       ) --- 'on/off' or '1/0'".to_owned())
    );
    assert!(
        lines
            .contains(&"beclient             (          ) --- BattlEye Client commands".to_owned())
    );
    assert!(c.fatal().is_none());
}

#[test]
fn help_at_startup_ends_the_program_before_name() {
    let c = Console::startup(files(), b"hud_info off\r\nhelp\r\nhud_weapon off\r\n");
    assert!(c.fatal().is_some());
    assert!(c.log().last().unwrap().starts_with("mouse_sens "));
    assert!(c.on("hud_weapon"), "nothing runs after");
}

#[test]
fn a_preset_runs_its_script_even_when_the_name_is_rejected() {
    let mut c = Console::new(files());
    assert_eq!(c.exec("_preset low"), Exec::Done);
    assert_eq!(status(&c, "_preset"), "Low");
    assert_eq!(status(&c, "r__geometry_lod"), "0.9");
    assert_eq!(c.number("r__tf_aniso"), 2.0);
    assert_eq!(status(&c, "ph_iterations"), "20");
    c.exec("r__tf_aniso 9");
    assert_eq!(c.exec("_preset Maximum"), Exec::Rejected);
    assert_eq!(status(&c, "_preset"), "Low");
    assert_eq!(c.number("r__tf_aniso"), 2.0, "Low's script ran again");
    c.exec("_preset Extreme");
    assert_eq!(c.number("r__tf_aniso"), 16.0);
}

#[test]
fn binding_takes_the_key_from_other_actions_except_single_and_multi() {
    let mut c = Console::startup(files(), b"");
    let k = |c: &Console, a: &str| c.keys(action_index(a).unwrap());
    assert_eq!(k(&c, "forward"), [Some("kW"), None]);
    // kF6 is on both vote (multiplayer) and quick_save (single player).
    assert_eq!(k(&c, "vote"), [Some("kF6"), None]);
    assert_eq!(k(&c, "quick_save"), [Some("kF6"), None]);
    c.exec("bind_sec forward kspace");
    assert_eq!(k(&c, "forward"), [Some("kW"), Some("kSPACE")]);
    assert_eq!(k(&c, "jump"), [None, None], "taken from jump");
    // The same action keeps both slots.
    c.exec("bind FORWARD kSPACE");
    assert_eq!(k(&c, "forward"), [Some("kSPACE"), Some("kSPACE")]);
    c.exec("bind jump kF6");
    assert_eq!(k(&c, "vote"), [None, None]);
    assert_eq!(k(&c, "quick_save"), [None, None]);
    c.exec("unbind_sec forward");
    assert_eq!(k(&c, "forward"), [Some("kSPACE"), None]);
    // Unknown names do nothing.
    assert_eq!(c.exec("bind nothing kW"), Exec::Rejected);
    assert_eq!(c.exec("bind jump kNOTHING"), Exec::Rejected);
    // unbindall loads the defaults.
    c.exec("unbindall");
    assert_eq!(k(&c, "forward"), [Some("kW"), None]);
    assert_eq!(k(&c, "jump"), [Some("kSPACE"), None]);
}

/// console-misc-settings.md section 9.
/// The `bind_console` lines `c` saves.
fn console_lines(c: &Console) -> Vec<String> {
    String::from_utf8(c.save())
        .unwrap()
        .split_inclusive('\n')
        .filter(|l| l.starts_with("bind_console "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn bound_lines_are_saved_by_key_code_with_two_spaces() {
    // console-key-bindings.md section 4's measured file.
    let user = b"bind_console bind_list k9\r\nbind_console flush kO\r\nbind_console cfg_save   Kb_Test  KP\r\nbind_console kL\r\nbind_console Screenshot   kF5\r\nbind_console r2_gloss_factor 0.5 MOUSE1\r\nbind_console hud_info off kNUMPAD7\r\n";
    let c = Console::startup(files(), user);
    let saved = [
        "bind_console bind_list  k9\n",
        "bind_console flush  kO\n",
        "bind_console cfg_save kb_test  kP\n",
        "bind_console  kL\n",
        "bind_console screenshot  kF5\n",
        "bind_console hud_info off  kNUMPAD7\n",
        "bind_console r2_gloss_factor 0.5  mouse1\n",
    ];
    assert_eq!(console_lines(&c), saved);
    assert_eq!(c.console_line(24), Some("flush "));
    assert_eq!(c.console_line(38), Some(""));
    assert_eq!(c.console_line(16), None);
    // Read back, the same lines are bound again.
    let read = Console::startup(files(), &c.save());
    assert_eq!(console_lines(&read), saved);
    assert_eq!(read, c);
}

#[test]
fn bind_console_replaces_and_unbind_console_removes() {
    let mut c = Console::startup(files(), b"");
    assert!(printed(&mut c, "bind_console screenshot kF5").is_empty());
    c.exec("bind_console flush kF5");
    assert_eq!(console_lines(&c), ["bind_console flush  kF5\n"]);
    assert_eq!(printed(&mut c, "bind_console"), ["- bind_console "]);
    assert_eq!(printed(&mut c, "unbind_console"), ["- unbind_console "]);
    // A key without a line is left as it is; nothing is printed.
    assert!(printed(&mut c, "unbind_console kq").is_empty());
    assert_eq!(console_lines(&c).len(), 1);
    assert!(printed(&mut c, "unbind_console kf5").is_empty());
    assert!(console_lines(&c).is_empty());
    assert_eq!(c.fatal(), None);
    // The action bindings and the lines are independent.
    c.exec("bind_console flush kW");
    c.exec("bind forward kO");
    c.exec("unbind back");
    assert_eq!(console_lines(&c), ["bind_console flush  kW\n"]);
    assert_eq!(c.keys(action_index("forward").unwrap()), [Some("kO"), None]);
    // unbindall empties the table.
    c.exec("unbindall");
    assert!(console_lines(&c).is_empty());
}

#[test]
fn an_unknown_key_name_ends_the_program() {
    for (line, name) in [
        ("bind_console flush kNOSUCH", "knosuch"),
        ("unbind_console kNOSUCH", "knosuch"),
        // The whole argument is the name (#643).
        ("unbind_console kf5 x", "kf5 x"),
    ] {
        let mut c = Console::startup(files(), b"");
        assert_eq!(
            printed(&mut c, line),
            [format!(
                "! cant find corresponding [_keyboard*] for keyname {name}"
            )]
        );
        assert!(c.fatal().is_some(), "{line}");
    }
    // In the configuration file, the next line is never executed.
    let c = Console::startup(
        files(),
        b"bind_console flush kNOSUCH\r\nbind_console flush kO\r\n",
    );
    assert!(c.fatal().is_some());
    assert_eq!(c.console_line(24), None);
}

#[test]
fn the_key_action_counts_single_player_and_any_game_actions() {
    let mut c = Console::startup(files(), b"");
    let name = |c: &Console, code| c.bound_action(code).map(|a| ACTIONS[a].0);
    assert_eq!(name(&c, 17), Some("forward"));
    // kF6: vote (multiplayer) comes first, but only quick_save counts.
    assert_eq!(name(&c, 64), Some("quick_save"));
    c.exec("bind skin_menu kO");
    assert_eq!(name(&c, 24), None);
    c.exec("bind_sec wpn_fire mouse1");
    assert_eq!(name(&c, 337), Some("wpn_fire"));
}

#[test]
fn list_actions_and_bind_list() {
    let mut c = Console::startup(files(), b"");
    let actions = printed(&mut c, "list_actions");
    assert_eq!(actions.len(), 79);
    assert_eq!(actions[0], "- --- Action list start ---");
    assert_eq!(actions[1], "- left");
    assert_eq!(actions[77], "- quick_load");
    assert_eq!(actions[78], "- --- Action list end   ---");
    c.exec("bind_sec forward kUP");
    c.exec("bind_console flush kL");
    let binds = printed(&mut c, "bind_list");
    assert_eq!(binds.len(), 79);
    assert_eq!(binds[0], "- --- Bind list start ---");
    assert_eq!(binds[1], "[left] primary is[NULL] secondary is[NULL]");
    assert_eq!(binds[10], "[forward] primary is[kW] secondary is[kUP]");
    assert_eq!(binds[78], "- --- Bind list end   ---");
    assert!(!binds.iter().any(|l| l.contains("kL")), "no console lines");
}

#[test]
fn r2em_switches_its_message_and_prints_it_after_a_number() {
    let mut c = Console::new(files());
    assert!(printed(&mut c, "r2em 2.5").is_empty());
    assert_eq!(c.exec("r2em ON"), Exec::Done);
    assert_eq!(status(&c, "r2em"), "2.5");
    assert_eq!(
        printed(&mut c, "r2em 2.5"),
        ["* material set to [phong]-[metal], with lerp of [0.500000]"]
    );
    assert_eq!(
        printed(&mut c, "r2em 4"),
        ["* material set to [oren]-[blin], with lerp of [0.000000]"]
    );
    assert_eq!(
        printed(&mut c, "r2em 0.25"),
        ["* material set to [oren]-[blin], with lerp of [0.250000]"]
    );
    assert_eq!(
        printed(&mut c, "r2em 5"),
        [
            "~ Invalid syntax in call to 'r2em'",
            "~ Valid arguments: float value in range [0.000,4.000]"
        ]
    );
    assert_eq!(c.exec("r2em off"), Exec::Done);
    assert!(printed(&mut c, "r2em 1").is_empty());
    assert_eq!(status(&c, "r2em"), "1.");
    // The switch is not saved: only the value is.
    c.exec("r2em on");
    let saved = String::from_utf8_lossy(&c.save()).into_owned();
    assert!(saved.contains("r2em 1.\r\n") && !saved.contains("r2em on"));
}

#[test]
fn the_user_file_round_trips_in_byte_order() {
    let user = b"_preset Extreme\r\nai_use_torch_dynamic_lights on\r\nunbindall\r\nbind left kLEFT\r\nbind quit kESCAPE\r\nbind_console disconnect kF10\nbind_console quit kF9\nbind_sec jump kRCONTROL\r\ng_always_run on\r\nmouse_sens 0.3\r\nph_iterations 18\r\nr2_slight_fade 0.3333\r\nvid_mode 1280x1024\r\nload_last_save all\r\ncdkey secret\r\nsv_no_auth_check 1\r\nnot_a_command 1\r\n";
    let c = Console::startup(files(), user);
    assert_eq!(c.number("r__tf_aniso"), 16.0, "the preset ran");
    let saved = String::from_utf8(c.save()).unwrap();
    let lines: Vec<&str> = saved.split_inclusive('\n').collect();
    assert_eq!(lines[0], "_preset Extreme\r\n");
    assert_eq!(lines[1], "ai_use_torch_dynamic_lights on\r\n");
    let at = |text: &str| {
        lines
            .iter()
            .position(|l| *l == text)
            .unwrap_or_else(|| panic!("{text:?}"))
    };
    let unbindall = at("unbindall\r\n");
    assert_eq!(lines[unbindall + 1], "bind left kLEFT\r\n");
    assert_eq!(
        lines[unbindall + 2],
        "bind jump kSPACE\r\n",
        "defaults, in action order"
    );
    assert_eq!(lines[unbindall + 3], "bind forward kW\r\n");
    assert!(at("bind quit kESCAPE\r\n") > unbindall);
    // bind < bind_console < bind_sec; console keys by code (F9 67, F10 68).
    assert_eq!(
        lines[at("bind_console quit  kF9\n") + 1],
        "bind_console disconnect  kF10\n"
    );
    assert!(at("bind_console quit  kF9\n") > at("bind quit kESCAPE\r\n"));
    assert!(at("bind_sec jump kRCONTROL\r\n") > at("bind_console disconnect  kF10\n"));
    assert!(at("g_always_run on\r\n") > at("bind_sec jump kRCONTROL\r\n"));
    for line in [
        "mouse_sens 0.3\r\n",
        "r2_slight_fade 0.3333\r\n",
        "vid_mode 1280x1024\r\n",
        "ph_iterations 18\r\n",
        "load_last_save all\r\n",
        "renderer renderer_r1\r\n",
        // Values the file did not set, at their built-in values.
        "rs_c_gamma 1.\r\n",
        "ph_frequency 100.00000\r\n",
        "r2_aa_break 0.800000,0.100000,0.000000\r\n",
    ] {
        assert!(saved.contains(line), "{line:?}");
    }
    // Never saved; actions and empty statuses write nothing.
    for name in [
        "cdkey",
        "sv_no_auth_check",
        "name",
        "mm_net_player_name",
        "mm_net_srv_name",
        "help",
        "not_a_command",
    ] {
        assert!(
            !lines.iter().any(|l| l.starts_with(&format!("{name} "))),
            "{name}"
        );
    }
    // Every saved line in name order.
    let names: Vec<&str> = lines
        .iter()
        .filter(|l| !l.starts_with("bind") && !l.starts_with("unbindall"))
        .map(|l| l.split(' ').next().unwrap())
        .collect();
    assert!(names.windows(2).all(|w| w[0] < w[1]), "each once, in order");
    let variables = COMMANDS
        .iter()
        .filter(|d| d.saved == Saved::Yes && !d.initial.is_empty() && !d.designer())
        .count();
    assert_eq!(
        names.len(),
        variables + 2,
        "the variables, renderer and load_last_save"
    );
    // Reading what was saved saves the same file; the console differs only
    // in what is never saved.
    let read = Console::startup(files(), saved.as_bytes());
    assert_eq!(String::from_utf8(read.save()).unwrap(), saved);
    assert_eq!(read.status("cdkey").unwrap(), "");
}

/// Section 8's rows: the name, the built-in status (`None`: the computer
/// name) and the help text.
#[rustfmt::skip]
const SECTION_8: &[(&str, Option<&str>, &str)] = &[
    (
        "_preset",
        Some("Default"),
        "Minimum/Low/Default/High/Extreme",
    ),
    (
        "ai_use_torch_dynamic_lights",
        Some("on"),
        "'on/off' or '1/0'",
    ),
    ("beclient", Some(""), "BattlEye Client commands"),
    ("bemsg", Some("1"), "integer value in range [0,1]"),
    ("beserver", Some(""), "BattlEye Server commands"),
    ("bind", Some(""), "no arguments"),
    ("bind_console", Some(""), "no arguments"),
    ("bind_list", Some(""), "no arguments"),
    ("bind_sec", Some(""), "no arguments"),
    (
        "cam_inert",
        Some("0.7"),
        "float value in range [0.000,1.000]",
    ),
    (
        "cam_slide_inert",
        Some("0.25"),
        "float value in range [0.000,1.000]",
    ),
    ("cdkey", Some(""), "string with up to 64 characters"),
    ("cfg_load", Some(""), "no arguments"),
    ("cfg_save", Some(""), "no arguments"),
    ("chat", Some(""), "no arguments"),
    ("check_for_updates", Some(""), "no arguments"),
    (
        "cl_cod_pickup_mode",
        Some("1"),
        "integer value in range [0,1]",
    ),
    ("cl_dynamiccrosshair", Some("on"), "'on/off' or '1/0'"),
    ("cl_voteno", Some(""), "Vote No"),
    ("cl_votestart", Some(""), "Starts Voting"),
    ("cl_voteyes", Some(""), "Vote Yes"),
    ("clear_log", Some(""), "no arguments"),
    ("demo_play", Some(""), "no arguments"),
    ("demo_record", Some(""), "no arguments"),
    (
        "demo_record_4step",
        Some("0"),
        "integer value in range [0,1]",
    ),
    ("demo_record_clear_points", Some(""), "no arguments"),
    (
        "demo_record_step",
        Some("0"),
        "integer value in range [0,3]",
    ),
    ("demo_record_take_point", Some(""), "no arguments"),
    (
        "demo_record_use_points",
        Some("0"),
        "integer value in range [0,1]",
    ),
    ("disconnect", Some(""), "no arguments"),
    ("dump_open_files", Some(""), "no arguments"),
    ("dump_resources", Some(""), "no arguments"),
    ("flush", Some(""), "no arguments"),
    ("g_always_run", Some("on"), "'on/off' or '1/0'"),
    ("g_autopickup", Some("off"), "'on/off' or '1/0'"),
    ("g_backrun", Some("off"), "'on/off' or '1/0'"),
    ("g_corpsenum", Some("10"), "integer value in range [0,100]"),
    ("g_eventdelay", Some("0"), "integer value in range [0,1000]"),
    ("g_game_difficulty", Some("gd_master"), "game difficulty"),
    ("g_kill", Some(""), "player kill"),
    ("g_restart", Some(""), "restart game"),
    ("g_restart_fast", Some(""), "restart game fast"),
    ("g_swapteams", Some(""), "swap teams for artefacthunt game"),
    ("get_server_address", Some(""), "List Players"),
    ("help", Some(""), "no arguments"),
    ("hud_crosshair", Some("on"), "'on/off' or '1/0'"),
    ("hud_crosshair_dist", Some("off"), "'on/off' or '1/0'"),
    ("hud_info", Some("on"), "'on/off' or '1/0'"),
    ("hud_weapon", Some("on"), "'on/off' or '1/0'"),
    ("list_actions", Some(""), "no arguments"),
    ("load", Some(""), "no arguments"),
    ("load_last_save", Some(""), "no arguments"),
    ("main_menu", Some(""), "no arguments"),
    ("mm_mm_net_srv_dedicated", Some("off"), "'on/off' or '1/0'"),
    ("mm_net_con_publicserver", Some("off"), "'on/off' or '1/0'"),
    (
        "mm_net_con_spectator",
        Some("20"),
        "integer value in range [1,32]",
    ),
    ("mm_net_con_spectator_on", Some("off"), "'on/off' or '1/0'"),
    ("mm_net_filter_battleye", Some("on"), "'on/off' or '1/0'"),
    ("mm_net_filter_empty", Some("on"), "'on/off' or '1/0'"),
    ("mm_net_filter_full", Some("on"), "'on/off' or '1/0'"),
    ("mm_net_filter_listen", Some("on"), "'on/off' or '1/0'"),
    ("mm_net_filter_pass", Some("on"), "'on/off' or '1/0'"),
    ("mm_net_filter_wo_ff", Some("on"), "'on/off' or '1/0'"),
    ("mm_net_filter_wo_pass", Some("on"), "'on/off' or '1/0'"),
    (
        "mm_net_player_name",
        Some(""),
        "string with up to 64 characters",
    ),
    (
        "mm_net_srv_gamemode",
        Some("st_deathmatch"),
        "st_deathmatch/st_team_deathmatch/st_artefacthunt",
    ),
    (
        "mm_net_srv_maxplayers",
        Some("32"),
        "integer value in range [2,32]",
    ),
    ("mm_net_srv_name", None, "string with up to 64 characters"),
    (
        "mm_net_srv_reinforcement_type",
        Some("1"),
        "integer value in range [0,2]",
    ),
    ("mm_net_use_battleye", Some("off"), "'on/off' or '1/0'"),
    (
        "mm_net_weather_rateofchange",
        Some("1."),
        "float value in range [0.000,100.000]",
    ),
    ("mouse_invert", Some("off"), "'on/off' or '1/0'"),
    (
        "mouse_sens",
        Some("0.12"),
        "float value in range [0.050,0.600]",
    ),
    ("name", Some(""), "player name"),
    ("net_cl_clearstats", Some(""), "clear client net statistic"),
    (
        "net_cl_icurvesize",
        Some("0"),
        "integer value in range [0,2000]",
    ),
    (
        "net_cl_icurvetype",
        Some("0"),
        "integer value in range [0,2]",
    ),
    (
        "net_cl_interpolation",
        Some("0."),
        "float value in range [-1.000,1.000]",
    ),
    ("net_cl_log_data", Some("off"), "'on/off' or '1/0'"),
    ("net_cl_resync", Some(""), "resyncronize client"),
    (
        "net_compressor_enabled",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "net_compressor_gather_stats",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "net_compressor_status",
        Some(""),
        "valid arguments is [info info_full on off]",
    ),
    (
        "net_dbg_dump_export_obj",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "net_dbg_dump_import_obj",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "net_dbg_dump_update_read",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "net_dbg_dump_update_write",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "net_dedicated_sleep",
        Some("5"),
        "integer value in range [0,64]",
    ),
    ("net_sv_clearstats", Some(""), "clear server net statistic"),
    ("net_sv_gpmode", Some("0"), "integer value in range [0,2]"),
    ("net_sv_log_data", Some("off"), "'on/off' or '1/0'"),
    (
        "net_sv_pending_lim",
        Some("2"),
        "integer value in range [0,10]",
    ),
    (
        "net_sv_update_rate",
        Some("30"),
        "integer value in range [1,100]",
    ),
    ("ph_frequency", Some("100.00000"), "no arguments"),
    (
        "ph_iterations",
        Some("18"),
        "integer value in range [15,50]",
    ),
    ("quit", Some(""), "no arguments"),
    ("r1_dlights", Some("on"), "'on/off' or '1/0'"),
    (
        "r1_dlights_clip",
        Some("30."),
        "float value in range [10.000,150.000]",
    ),
    (
        "r1_glows_per_frame",
        Some("16"),
        "integer value in range [2,32]",
    ),
    (
        "r1_lmodel_lerp",
        Some("0.1"),
        "float value in range [0.000,0.333]",
    ),
    (
        "r1_pps_u",
        Some("0."),
        "float value in range [-1.000,1.000]",
    ),
    (
        "r1_pps_v",
        Some("0."),
        "float value in range [-1.000,1.000]",
    ),
    (
        "r1_ssa_lod_a",
        Some("64."),
        "float value in range [16.000,96.000]",
    ),
    (
        "r1_ssa_lod_b",
        Some("48."),
        "float value in range [16.000,64.000]",
    ),
    (
        "r1_tf_mipbias",
        Some("0."),
        "float value in range [-0.500,0.500]",
    ),
    ("r2_aa", Some("off"), "'on/off' or '1/0'"),
    (
        "r2_aa_break",
        Some("0.800000,0.100000,0.000000"),
        "vector3 in range [0.000000,0.000000,0.000000]-[1.000000,1.000000,1.000000]",
    ),
    (
        "r2_aa_kernel",
        Some("0.5"),
        "float value in range [0.300,0.700]",
    ),
    (
        "r2_aa_weight",
        Some("0.250000,0.250000,0.000000"),
        "vector3 in range [0.000000,0.000000,0.000000]-[1.000000,1.000000,1.000000]",
    ),
    ("r2_allow_r1_lights", Some("off"), "'on/off' or '1/0'"),
    ("r2_gi", Some("off"), "'on/off' or '1/0'"),
    (
        "r2_gi_clip",
        Some("0.001"),
        "float value in range [0.000,0.100]",
    ),
    ("r2_gi_depth", Some("1"), "integer value in range [1,5]"),
    (
        "r2_gi_photons",
        Some("16"),
        "integer value in range [8,256]",
    ),
    (
        "r2_gi_refl",
        Some("0.9"),
        "float value in range [0.001,0.990]",
    ),
    (
        "r2_gloss_factor",
        Some("1."),
        "float value in range [0.000,10.000]",
    ),
    ("r2_ls_bloom_fast", Some("off"), "'on/off' or '1/0'"),
    (
        "r2_ls_bloom_kernel_b",
        Some("0.7"),
        "float value in range [0.010,1.000]",
    ),
    (
        "r2_ls_bloom_kernel_g",
        Some("3.3"),
        "float value in range [1.000,7.000]",
    ),
    (
        "r2_ls_bloom_kernel_scale",
        Some("1."),
        "float value in range [0.500,2.000]",
    ),
    (
        "r2_ls_bloom_speed",
        Some("10."),
        "float value in range [0.000,100.000]",
    ),
    (
        "r2_ls_bloom_threshold",
        Some("0.3"),
        "float value in range [0.000,1.000]",
    ),
    (
        "r2_ls_depth_bias",
        Some("-0.0001"),
        "float value in range [-0.500,0.500]",
    ),
    (
        "r2_ls_depth_scale",
        Some("1.00001"),
        "float value in range [0.500,1.500]",
    ),
    (
        "r2_ls_dsm_kernel",
        Some("0.7"),
        "float value in range [0.100,3.000]",
    ),
    (
        "r2_ls_psm_kernel",
        Some("0.7"),
        "float value in range [0.100,3.000]",
    ),
    (
        "r2_ls_squality",
        Some("1."),
        "float value in range [0.500,1.000]",
    ),
    (
        "r2_ls_ssm_kernel",
        Some("0.7"),
        "float value in range [0.100,3.000]",
    ),
    (
        "r2_mblur",
        Some("0.5"),
        "float value in range [0.000,1.000]",
    ),
    (
        "r2_parallax_h",
        Some("0.02"),
        "float value in range [0.000,0.500]",
    ),
    (
        "r2_slight_fade",
        Some("1."),
        "float value in range [0.020,2.000]",
    ),
    (
        "r2_ssa_lod_a",
        Some("48."),
        "float value in range [16.000,96.000]",
    ),
    (
        "r2_ssa_lod_b",
        Some("32."),
        "float value in range [32.000,64.000]",
    ),
    ("r2_sun", Some("on"), "'on/off' or '1/0'"),
    (
        "r2_sun_depth_far_bias",
        Some("0."),
        "float value in range [-0.500,0.500]",
    ),
    (
        "r2_sun_depth_far_scale",
        Some("1."),
        "float value in range [0.500,1.500]",
    ),
    (
        "r2_sun_depth_near_bias",
        Some("-0.00004"),
        "float value in range [-0.500,0.500]",
    ),
    (
        "r2_sun_depth_near_scale",
        Some("1.00001"),
        "float value in range [0.500,1.500]",
    ),
    ("r2_sun_details", Some("off"), "'on/off' or '1/0'"),
    ("r2_sun_focus", Some("off"), "'on/off' or '1/0'"),
    (
        "r2_sun_lumscale",
        Some("1."),
        "float value in range [-1.000,3.000]",
    ),
    (
        "r2_sun_lumscale_amb",
        Some("1."),
        "float value in range [0.000,3.000]",
    ),
    (
        "r2_sun_lumscale_hemi",
        Some("1."),
        "float value in range [0.000,3.000]",
    ),
    (
        "r2_sun_near",
        Some("12."),
        "float value in range [1.000,50.000]",
    ),
    (
        "r2_sun_near_border",
        Some("0.66"),
        "float value in range [0.500,1.000]",
    ),
    ("r2_sun_tsm", Some("off"), "'on/off' or '1/0'"),
    (
        "r2_sun_tsm_bias",
        Some("-0.05"),
        "float value in range [-0.500,0.500]",
    ),
    (
        "r2_sun_tsm_proj",
        Some("0.18"),
        "float value in range [0.001,0.800]",
    ),
    (
        "r2_tf_mipbias",
        Some("0."),
        "float value in range [-0.500,0.500]",
    ),
    ("r2_tonemap", Some("off"), "'on/off' or '1/0'"),
    (
        "r2_tonemap_adaptation",
        Some("5."),
        "float value in range [0.010,10.000]",
    ),
    (
        "r2_tonemap_amount",
        Some("0.5"),
        "float value in range [0.000,1.000]",
    ),
    (
        "r2_tonemap_lowlum",
        Some("0.001"),
        "float value in range [0.000,1.000]",
    ),
    (
        "r2_tonemap_middlegray",
        Some("0.25"),
        "float value in range [0.000,2.000]",
    ),
    ("r2_wait_sleep", Some("0"), "integer value in range [0,1]"),
    ("r2_zfill", Some("off"), "'on/off' or '1/0'"),
    (
        "r2_zfill_depth",
        Some("0.1"),
        "float value in range [0.001,0.500]",
    ),
    ("r2em", Some("0."), "float value in range [0.000,4.000]"),
    (
        "r__detail_density",
        Some("0.3"),
        "float value in range [0.200,0.600]",
    ),
    (
        "r__geometry_lod",
        Some("1."),
        "float value in range [0.100,1.200]",
    ),
    ("r__supersample", Some("1"), "integer value in range [1,4]"),
    ("r__tf_aniso", Some("4"), "integer value in range [1,16]"),
    ("ra", Some(""), "no arguments"),
    (
        "renderer",
        Some("renderer_r1"),
        "renderer_r1/renderer_r2a/renderer_r2",
    ),
    (
        "rs_c_brightness",
        Some("1."),
        "float value in range [0.500,1.500]",
    ),
    (
        "rs_c_contrast",
        Some("1."),
        "float value in range [0.500,1.500]",
    ),
    (
        "rs_c_gamma",
        Some("1."),
        "float value in range [0.500,1.500]",
    ),
    ("rs_fullscreen", Some("on"), "'on/off' or '1/0'"),
    ("rs_refresh_60hz", Some("off"), "'on/off' or '1/0'"),
    ("rs_stats", Some("off"), "'on/off' or '1/0'"),
    ("rs_v_sync", Some("off"), "'on/off' or '1/0'"),
    (
        "rs_vis_distance",
        Some("1."),
        "float value in range [0.400,1.500]",
    ),
    ("save", Some(""), "no arguments"),
    ("screenshot", Some(""), "no arguments"),
    ("snd_acceleration", Some("on"), "'on/off' or '1/0'"),
    (
        "snd_cache_size",
        Some("16"),
        "integer value in range [4,32]",
    ),
    ("snd_efx", Some("on"), "'on/off' or '1/0'"),
    ("snd_restart", Some(""), "no arguments"),
    ("snd_targets", Some("16"), "integer value in range [4,32]"),
    (
        "snd_volume_eff",
        Some("1."),
        "float value in range [0.000,1.000]",
    ),
    (
        "snd_volume_music",
        Some("0.7"),
        "float value in range [0.000,1.000]",
    ),
    ("start", Some(""), "no arguments"),
    ("stat_memory", Some(""), "no arguments"),
    ("sv_addmap", Some(""), "Adds map to map rotation list"),
    (
        "sv_anomalies_enabled",
        Some("1"),
        "integer value in range [0,1]",
    ),
    (
        "sv_anomalies_length",
        Some("3"),
        "integer value in range [0,180]",
    ),
    (
        "sv_artefact_respawn_delta",
        Some("30"),
        "integer value in range [0,600]",
    ),
    (
        "sv_artefact_spawn_force",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "sv_artefact_stay_time",
        Some("3"),
        "integer value in range [0,180]",
    ),
    (
        "sv_artefacts_count",
        Some("10"),
        "integer value in range [1,100]",
    ),
    (
        "sv_auto_team_balance",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "sv_auto_team_swap",
        Some("1"),
        "integer value in range [0,1]",
    ),
    ("sv_banplayer", Some(""), "Ban Player by Name"),
    ("sv_banplayer_ip", Some(""), "Ban Player by IP"),
    (
        "sv_bearercantsprint",
        Some("1"),
        "integer value in range [0,1]",
    ),
    ("sv_changegametype", Some(""), "Changing Game Type"),
    ("sv_changelevel", Some(""), "Changing Game Type"),
    (
        "sv_changelevelgametype",
        Some(""),
        "Changing level and game type",
    ),
    (
        "sv_client_reconnect_time",
        Some("0"),
        "integer value in range [0,60]",
    ),
    (
        "sv_console_update_rate",
        Some("1"),
        "integer value in range [1,100]",
    ),
    (
        "sv_dedicated_server_update_rate",
        Some("100"),
        "integer value in range [1,1000]",
    ),
    (
        "sv_dmgblockindicator",
        Some("1"),
        "integer value in range [0,1]",
    ),
    (
        "sv_dmgblocktime",
        Some("0"),
        "integer value in range [0,600]",
    ),
    (
        "sv_dump_online_statistics",
        Some(""),
        "Shows current server settings",
    ),
    (
        "sv_dump_online_statistics_period",
        Some("0"),
        "integer value in range [0,60]",
    ),
    (
        "sv_forcerespawn",
        Some("0"),
        "integer value in range [0,3600]",
    ),
    ("sv_fraglimit", Some("10"), "integer value in range [0,100]"),
    (
        "sv_friendly_indicators",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "sv_friendly_names",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "sv_friendlyfire",
        Some("1."),
        "float value in range [0.000,2.000]",
    ),
    (
        "sv_hail_to_winner_time",
        Some("7"),
        "integer value in range [0,60]",
    ),
    ("sv_kick", Some(""), "Kick Player by name"),
    ("sv_listmaps", Some(""), "List maps in map rotation list"),
    ("sv_listplayers", Some(""), "List Players"),
    ("sv_listplayers_banned", Some(""), "List of Banned Players"),
    (
        "sv_max_ping_limit",
        Some("2000"),
        "integer value in range [1,2000]",
    ),
    (
        "sv_nextanomalyset",
        Some(""),
        "Activating pointed Anomaly set",
    ),
    (
        "sv_nextmap",
        Some(""),
        "Switch to Next Map in map rotation list",
    ),
    (
        "sv_no_auth_check",
        Some("0"),
        "integer value in range [0,1]",
    ),
    ("sv_pda_hunt", Some("1"), "integer value in range [0,1]"),
    (
        "sv_prevmap",
        Some(""),
        "Switch to Previous Map in map rotation list",
    ),
    (
        "sv_reinforcement_time",
        Some("20"),
        "integer value in range [-1,3600]",
    ),
    (
        "sv_remove_corpse",
        Some("1"),
        "integer value in range [-1,1]",
    ),
    (
        "sv_remove_weapon",
        Some("1"),
        "integer value in range [-1,1]",
    ),
    ("sv_return_to_base", Some(""), "no arguments"),
    (
        "sv_returnplayers",
        Some("1"),
        "integer value in range [0,1]",
    ),
    (
        "sv_rpoint_freeze_time",
        Some("0"),
        "integer value in range [0,60000]",
    ),
    ("sv_setenvtime", Some(""), "no arguments"),
    ("sv_setweather", Some(""), "Set new weather"),
    (
        "sv_shieldedbases",
        Some("1"),
        "integer value in range [0,1]",
    ),
    (
        "sv_spectr_firsteye",
        Some("1"),
        "integer value in range [0,1]",
    ),
    (
        "sv_spectr_freefly",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "sv_spectr_freelook",
        Some("1"),
        "integer value in range [0,1]",
    ),
    (
        "sv_spectr_lookat",
        Some("1"),
        "integer value in range [0,1]",
    ),
    (
        "sv_spectr_teamcamera",
        Some("1"),
        "integer value in range [0,1]",
    ),
    ("sv_startteammoney", Some(""), "Set Start Team Money"),
    (
        "sv_statistic_collect",
        Some("0"),
        "integer value in range [0,1]",
    ),
    ("sv_statistic_save", Some(""), "saving statistic data"),
    ("sv_status", Some(""), "Shows current server settings"),
    (
        "sv_teamkill_limit",
        Some("3"),
        "integer value in range [0,100]",
    ),
    (
        "sv_teamkill_punish",
        Some("1"),
        "integer value in range [0,1]",
    ),
    ("sv_timelimit", Some("0"), "integer value in range [0,180]"),
    ("sv_unbanplayer_ip", Some(""), "UnBan Player by IP"),
    (
        "sv_vote_enabled",
        Some("255"),
        "integer value in range [0,255]",
    ),
    (
        "sv_vote_participants",
        Some("0"),
        "integer value in range [0,1]",
    ),
    (
        "sv_vote_quota",
        Some("0.51"),
        "float value in range [0.000,1.000]",
    ),
    (
        "sv_vote_time",
        Some("1."),
        "float value in range [0.500,10.000]",
    ),
    ("sv_votestop", Some(""), "Stops Current Voting"),
    ("sv_warm_up", Some("0"), "integer value in range [0,3600]"),
    ("texture_lod", Some("0"), "integer value in range [0,4]"),
    ("unbind", Some(""), "no arguments"),
    ("unbind_console", Some(""), "no arguments"),
    ("unbind_sec", Some(""), "no arguments"),
    ("unbindall", Some(""), "no arguments"),
    ("vid_mode", Some("1024x768"), "change screen resolution WxH"),
    ("vid_restart", Some(""), "no arguments"),
];
