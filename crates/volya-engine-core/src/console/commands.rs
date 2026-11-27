// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Every console command: its kind, range, built-in value, saving rule and
//! bare-name rule (docs/behavior/console-commands.md section 8).

/// How a command parses, shows and describes its value (section 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// `on`/`1`, `off`/`0` (3.1).
    OnOff,
    /// `atoi`, *min* … *max* inclusive (3.3).
    Int { min: i64, max: i64 },
    /// `atof`, *min* … *max* within 0.00001 (3.4).
    Real { min: f32, max: f32 },
    /// A value name, ignoring case, and its stored number (3.5).
    List(&'static [(&'static str, i64)]),
    /// `x,y,z`, each component within its range (3.6).
    Vector { min: [f32; 3], max: [f32; 3] },
    /// Any text, cut to `size` − 1 characters (3.7).
    Text { size: usize },
    /// No value; its own help text, `no arguments` without one (3.8).
    Action(Option<&'static str>),
    /// `<W>x<H>` (`vid_mode`, 4.1).
    Mode,
    /// The physics frequency (`ph_frequency`, 4.2).
    Frequency,
    /// The player's name (`name`, 4.3).
    Name,
}

/// Which lines the configuration file saves of a command (section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// `<name> <status>` when the status is not empty ("yes", and "—" for
    /// the actions, whose status is empty).
    Yes,
    /// Never a line.
    No,
    /// Its own lines.
    Own,
}

/// What the bare name does (section 2, step 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bare {
    /// Prints `- <name> <status>`.
    Shows,
    /// Runs the command with an empty argument.
    Runs,
}

/// A console command (section 8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommandDef {
    pub name: &'static str,
    pub kind: Kind,
    /// The status before any configuration file runs (section 7); empty for
    /// commands without a value.
    pub initial: &'static str,
    pub saved: Saved,
    pub bare: Bare,
}

impl CommandDef {
    /// Whether the argument keeps its case (section 2, step 5).
    pub fn keeps_case(&self) -> bool {
        matches!(
            self.name,
            "cdkey" | "mm_net_player_name" | "mm_net_srv_name"
        )
    }

    /// Whether the command is registered only when the command line
    /// contains `designer` (section 1). Volya Engine has no command line, so
    /// these are never registered.
    pub fn designer(&self) -> bool {
        matches!(
            self.name,
            "demo_record_take_point"
                | "demo_record_clear_points"
                | "demo_record_use_points"
                | "demo_record_4step"
                | "demo_record_step"
        )
    }
}

const fn c(
    name: &'static str,
    kind: Kind,
    initial: &'static str,
    saved: Saved,
    bare: Bare,
) -> CommandDef {
    CommandDef {
        name,
        kind,
        initial,
        saved,
        bare,
    }
}

/// The presets' script file names, `$game_config$rspec_<file>.ltx`, by
/// `_preset` value (docs/behavior/options-screens.md section 8).
pub const PRESET_FILES: [&str; 5] = ["minimum", "low", "default", "high", "extreme"];

/// The values of a list command, with their stored numbers.
pub const PRESETS: &[(&str, i64)] = &[
    ("Minimum", 0),
    ("Low", 1),
    ("Default", 2),
    ("High", 3),
    ("Extreme", 4),
];
/// The values of a list command, with their stored numbers.
pub const DIFFICULTIES: &[(&str, i64)] = &[
    ("gd_novice", 0),
    ("gd_stalker", 1),
    ("gd_veteran", 2),
    ("gd_master", 3),
];
/// The values of a list command, with their stored numbers.
pub const GAME_MODES: &[(&str, i64)] = &[
    ("st_deathmatch", 2),
    ("st_team_deathmatch", 6),
    ("st_artefacthunt", 7),
];
/// The values of a list command, with their stored numbers.
pub const RENDERERS: &[(&str, i64)] =
    &[("renderer_r1", 0), ("renderer_r2a", 1), ("renderer_r2", 2)];

/// Every command, in ascending byte order of the names (section 1).
/// `mm_net_srv_name` starts empty: Volya Engine has no computer name (4.5
/// starts it as the Windows computer name).
#[rustfmt::skip]
pub const COMMANDS: &[CommandDef] = {
    use Bare::*;
    use Kind::*;
    use Saved::*;
    &[
        c("_preset", List(PRESETS), "Default", Yes, Shows),
        c("ai_use_torch_dynamic_lights", OnOff, "on", Yes, Shows),
        c("beclient", Action(Some("BattlEye Client commands")), "", Yes, Runs),
        c("bemsg", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("beserver", Action(Some("BattlEye Server commands")), "", Yes, Runs),
        c("bind", Action(None), "", Own, Shows),
        c("bind_console", Action(None), "", Own, Shows),
        c("bind_list", Action(None), "", Yes, Runs),
        c("bind_sec", Action(None), "", Own, Shows),
        c("cam_inert", Real { min: 0.0, max: 1.0 }, "0.7", Yes, Shows),
        c("cam_slide_inert", Real { min: 0.0, max: 1.0 }, "0.25", Yes, Shows),
        c("cdkey", Text { size: 64 }, "", No, Shows),
        c("cfg_load", Action(None), "", Yes, Shows),
        c("cfg_save", Action(None), "", Yes, Runs),
        c("chat", Action(None), "", Yes, Shows),
        c("check_for_updates", Action(None), "", Yes, Runs),
        c("cl_cod_pickup_mode", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("cl_dynamiccrosshair", OnOff, "on", Yes, Shows),
        c("cl_voteno", Action(Some("Vote No")), "", Yes, Runs),
        c("cl_votestart", Action(Some("Starts Voting")), "", Yes, Shows),
        c("cl_voteyes", Action(Some("Vote Yes")), "", Yes, Runs),
        c("clear_log", Action(None), "", Yes, Runs),
        c("demo_play", Action(None), "", Yes, Runs),
        c("demo_record", Action(None), "", Yes, Shows),
        c("demo_record_4step", Int { min: 0, max: 1 }, "0", No, Shows),
        c("demo_record_clear_points", Action(None), "", Yes, Runs),
        c("demo_record_step", Int { min: 0, max: 3 }, "0", No, Shows),
        c("demo_record_take_point", Action(None), "", Yes, Runs),
        c("demo_record_use_points", Int { min: 0, max: 1 }, "0", No, Shows),
        c("disconnect", Action(None), "", Yes, Runs),
        c("dump_open_files", Action(None), "", Yes, Shows),
        c("dump_resources", Action(None), "", Yes, Runs),
        c("flush", Action(None), "", Yes, Runs),
        c("g_always_run", OnOff, "on", Yes, Shows),
        c("g_autopickup", OnOff, "off", Yes, Shows),
        c("g_backrun", OnOff, "off", Yes, Shows),
        c("g_corpsenum", Int { min: 0, max: 100 }, "10", Yes, Shows),
        c("g_eventdelay", Int { min: 0, max: 1000 }, "0", Yes, Shows),
        c("g_game_difficulty", List(DIFFICULTIES), "gd_master", Yes, Shows),
        c("g_kill", Action(Some("player kill")), "", Yes, Runs),
        c("g_restart", Action(Some("restart game")), "", Yes, Runs),
        c("g_restart_fast", Action(Some("restart game fast")), "", Yes, Runs),
        c("g_swapteams", Action(Some("swap teams for artefacthunt game")), "", Yes, Runs),
        c("get_server_address", Action(Some("List Players")), "", Yes, Runs),
        c("help", Action(None), "", Yes, Runs),
        c("hud_crosshair", OnOff, "on", Yes, Shows),
        c("hud_crosshair_dist", OnOff, "off", Yes, Shows),
        c("hud_info", OnOff, "on", Yes, Shows),
        c("hud_weapon", OnOff, "on", Yes, Shows),
        c("list_actions", Action(None), "", Yes, Runs),
        c("load", Action(None), "", Yes, Runs),
        c("load_last_save", Action(None), "", Own, Runs),
        c("main_menu", Action(None), "", Yes, Runs),
        c("mm_mm_net_srv_dedicated", OnOff, "off", Yes, Shows),
        c("mm_net_con_publicserver", OnOff, "off", Yes, Shows),
        c("mm_net_con_spectator", Int { min: 1, max: 32 }, "20", Yes, Shows),
        c("mm_net_con_spectator_on", OnOff, "off", Yes, Shows),
        c("mm_net_filter_battleye", OnOff, "on", Yes, Shows),
        c("mm_net_filter_empty", OnOff, "on", Yes, Shows),
        c("mm_net_filter_full", OnOff, "on", Yes, Shows),
        c("mm_net_filter_listen", OnOff, "on", Yes, Shows),
        c("mm_net_filter_pass", OnOff, "on", Yes, Shows),
        c("mm_net_filter_wo_ff", OnOff, "on", Yes, Shows),
        c("mm_net_filter_wo_pass", OnOff, "on", Yes, Shows),
        c("mm_net_player_name", Text { size: 64 }, "", No, Shows),
        c("mm_net_srv_gamemode", List(GAME_MODES), "st_deathmatch", Yes, Shows),
        c("mm_net_srv_maxplayers", Int { min: 2, max: 32 }, "32", Yes, Shows),
        c("mm_net_srv_name", Text { size: 64 }, "", Yes, Shows),
        c("mm_net_srv_reinforcement_type", Int { min: 0, max: 2 }, "1", Yes, Shows),
        c("mm_net_use_battleye", OnOff, "off", Yes, Shows),
        c("mm_net_weather_rateofchange", Real { min: 0.0, max: 100.0 }, "1.", Yes, Shows),
        c("mouse_invert", OnOff, "off", Yes, Shows),
        c("mouse_sens", Real { min: 0.05, max: 0.6 }, "0.12", Yes, Shows),
        c("name", Name, "", No, Shows),
        c("net_cl_clearstats", Action(Some("clear client net statistic")), "", Yes, Runs),
        c("net_cl_icurvesize", Int { min: 0, max: 2000 }, "0", Yes, Shows),
        c("net_cl_icurvetype", Int { min: 0, max: 2 }, "0", Yes, Shows),
        c("net_cl_interpolation", Real { min: -1.0, max: 1.0 }, "0.", Yes, Shows),
        c("net_cl_log_data", OnOff, "off", Yes, Shows),
        c("net_cl_resync", Action(Some("resyncronize client")), "", Yes, Runs),
        c("net_compressor_enabled", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("net_compressor_gather_stats", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("net_compressor_status", Action(Some("valid arguments is [info info_full on off]")), "", Yes, Shows),
        c("net_dbg_dump_export_obj", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("net_dbg_dump_import_obj", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("net_dbg_dump_update_read", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("net_dbg_dump_update_write", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("net_dedicated_sleep", Int { min: 0, max: 64 }, "5", Yes, Shows),
        c("net_sv_clearstats", Action(Some("clear server net statistic")), "", Yes, Runs),
        c("net_sv_gpmode", Int { min: 0, max: 2 }, "0", Yes, Shows),
        c("net_sv_log_data", OnOff, "off", Yes, Shows),
        c("net_sv_pending_lim", Int { min: 0, max: 10 }, "2", Yes, Shows),
        c("net_sv_update_rate", Int { min: 1, max: 100 }, "30", Yes, Shows),
        c("ph_frequency", Frequency, "100.00000", Yes, Shows),
        c("ph_iterations", Int { min: 15, max: 50 }, "18", Yes, Shows),
        c("quit", Action(None), "", Yes, Runs),
        c("r1_dlights", OnOff, "on", Yes, Shows),
        c("r1_dlights_clip", Real { min: 10.0, max: 150.0 }, "30.", Yes, Shows),
        c("r1_glows_per_frame", Int { min: 2, max: 32 }, "16", Yes, Shows),
        c("r1_lmodel_lerp", Real { min: 0.0, max: 0.333 }, "0.1", Yes, Shows),
        c("r1_pps_u", Real { min: -1.0, max: 1.0 }, "0.", Yes, Shows),
        c("r1_pps_v", Real { min: -1.0, max: 1.0 }, "0.", Yes, Shows),
        c("r1_ssa_lod_a", Real { min: 16.0, max: 96.0 }, "64.", Yes, Shows),
        c("r1_ssa_lod_b", Real { min: 16.0, max: 64.0 }, "48.", Yes, Shows),
        c("r1_tf_mipbias", Real { min: -0.5, max: 0.5 }, "0.", Yes, Shows),
        c("r2_aa", OnOff, "off", Yes, Shows),
        c("r2_aa_break", Vector { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }, "0.800000,0.100000,0.000000", Yes, Shows),
        c("r2_aa_kernel", Real { min: 0.3, max: 0.7 }, "0.5", Yes, Shows),
        c("r2_aa_weight", Vector { min: [0.0, 0.0, 0.0], max: [1.0, 1.0, 1.0] }, "0.250000,0.250000,0.000000", Yes, Shows),
        c("r2_allow_r1_lights", OnOff, "off", Yes, Shows),
        c("r2_gi", OnOff, "off", Yes, Shows),
        c("r2_gi_clip", Real { min: 0.00001, max: 0.1 }, "0.001", Yes, Shows),
        c("r2_gi_depth", Int { min: 1, max: 5 }, "1", Yes, Shows),
        c("r2_gi_photons", Int { min: 8, max: 256 }, "16", Yes, Shows),
        c("r2_gi_refl", Real { min: 0.001, max: 0.99 }, "0.9", Yes, Shows),
        c("r2_gloss_factor", Real { min: 0.0, max: 10.0 }, "1.", Yes, Shows),
        c("r2_ls_bloom_fast", OnOff, "off", Yes, Shows),
        c("r2_ls_bloom_kernel_b", Real { min: 0.01, max: 1.0 }, "0.7", Yes, Shows),
        c("r2_ls_bloom_kernel_g", Real { min: 1.0, max: 7.0 }, "3.3", Yes, Shows),
        c("r2_ls_bloom_kernel_scale", Real { min: 0.5, max: 2.0 }, "1.", Yes, Shows),
        c("r2_ls_bloom_speed", Real { min: 0.0, max: 100.0 }, "10.", Yes, Shows),
        c("r2_ls_bloom_threshold", Real { min: 0.0, max: 1.0 }, "0.3", Yes, Shows),
        c("r2_ls_depth_bias", Real { min: -0.5, max: 0.5 }, "-0.0001", Yes, Shows),
        c("r2_ls_depth_scale", Real { min: 0.5, max: 1.5 }, "1.00001", Yes, Shows),
        c("r2_ls_dsm_kernel", Real { min: 0.1, max: 3.0 }, "0.7", Yes, Shows),
        c("r2_ls_psm_kernel", Real { min: 0.1, max: 3.0 }, "0.7", Yes, Shows),
        c("r2_ls_squality", Real { min: 0.5, max: 1.0 }, "1.", Yes, Shows),
        c("r2_ls_ssm_kernel", Real { min: 0.1, max: 3.0 }, "0.7", Yes, Shows),
        c("r2_mblur", Real { min: 0.0, max: 1.0 }, "0.5", Yes, Shows),
        c("r2_parallax_h", Real { min: 0.0, max: 0.5 }, "0.02", Yes, Shows),
        c("r2_slight_fade", Real { min: 0.02, max: 2.0 }, "1.", Yes, Shows),
        c("r2_ssa_lod_a", Real { min: 16.0, max: 96.0 }, "48.", Yes, Shows),
        c("r2_ssa_lod_b", Real { min: 32.0, max: 64.0 }, "32.", Yes, Shows),
        c("r2_sun", OnOff, "on", Yes, Shows),
        c("r2_sun_depth_far_bias", Real { min: -0.5, max: 0.5 }, "0.", Yes, Shows),
        c("r2_sun_depth_far_scale", Real { min: 0.5, max: 1.5 }, "1.", Yes, Shows),
        c("r2_sun_depth_near_bias", Real { min: -0.5, max: 0.5 }, "-0.00004", Yes, Shows),
        c("r2_sun_depth_near_scale", Real { min: 0.5, max: 1.5 }, "1.00001", Yes, Shows),
        c("r2_sun_details", OnOff, "off", Yes, Shows),
        c("r2_sun_focus", OnOff, "off", Yes, Shows),
        c("r2_sun_lumscale", Real { min: -1.0, max: 3.0 }, "1.", Yes, Shows),
        c("r2_sun_lumscale_amb", Real { min: 0.0, max: 3.0 }, "1.", Yes, Shows),
        c("r2_sun_lumscale_hemi", Real { min: 0.0, max: 3.0 }, "1.", Yes, Shows),
        c("r2_sun_near", Real { min: 1.0, max: 50.0 }, "12.", Yes, Shows),
        c("r2_sun_near_border", Real { min: 0.5, max: 1.0 }, "0.66", Yes, Shows),
        c("r2_sun_tsm", OnOff, "off", Yes, Shows),
        c("r2_sun_tsm_bias", Real { min: -0.5, max: 0.5 }, "-0.05", Yes, Shows),
        c("r2_sun_tsm_proj", Real { min: 0.001, max: 0.8 }, "0.18", Yes, Shows),
        c("r2_tf_mipbias", Real { min: -0.5, max: 0.5 }, "0.", Yes, Shows),
        c("r2_tonemap", OnOff, "off", Yes, Shows),
        c("r2_tonemap_adaptation", Real { min: 0.01, max: 10.0 }, "5.", Yes, Shows),
        c("r2_tonemap_amount", Real { min: 0.0, max: 1.0 }, "0.5", Yes, Shows),
        c("r2_tonemap_lowlum", Real { min: 0.0001, max: 1.0 }, "0.001", Yes, Shows),
        c("r2_tonemap_middlegray", Real { min: 0.0, max: 2.0 }, "0.25", Yes, Shows),
        c("r2_wait_sleep", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("r2_zfill", OnOff, "off", Yes, Shows),
        c("r2_zfill_depth", Real { min: 0.001, max: 0.5 }, "0.1", Yes, Shows),
        c("r2em", Real { min: 0.0, max: 4.0 }, "0.", Yes, Shows),
        c("r__detail_density", Real { min: 0.2, max: 0.6 }, "0.3", Yes, Shows),
        c("r__geometry_lod", Real { min: 0.1, max: 1.2 }, "1.", Yes, Shows),
        c("r__supersample", Int { min: 1, max: 4 }, "1", Yes, Shows),
        c("r__tf_aniso", Int { min: 1, max: 16 }, "4", Yes, Shows),
        c("ra", Action(None), "", No, Shows),
        c("renderer", List(RENDERERS), "renderer_r1", Own, Shows),
        c("rs_c_brightness", Real { min: 0.5, max: 1.5 }, "1.", Yes, Shows),
        c("rs_c_contrast", Real { min: 0.5, max: 1.5 }, "1.", Yes, Shows),
        c("rs_c_gamma", Real { min: 0.5, max: 1.5 }, "1.", Yes, Shows),
        c("rs_fullscreen", OnOff, "on", Yes, Shows),
        c("rs_refresh_60hz", OnOff, "off", Yes, Shows),
        c("rs_stats", OnOff, "off", Yes, Shows),
        c("rs_v_sync", OnOff, "off", Yes, Shows),
        c("rs_vis_distance", Real { min: 0.4, max: 1.5 }, "1.", Yes, Shows),
        c("save", Action(None), "", Yes, Runs),
        c("screenshot", Action(None), "", Yes, Shows),
        c("snd_acceleration", OnOff, "on", Yes, Shows),
        c("snd_cache_size", Int { min: 4, max: 32 }, "16", Yes, Shows),
        c("snd_efx", OnOff, "on", Yes, Shows),
        c("snd_restart", Action(None), "", Yes, Runs),
        c("snd_targets", Int { min: 4, max: 32 }, "16", Yes, Shows),
        c("snd_volume_eff", Real { min: 0.0, max: 1.0 }, "1.", Yes, Shows),
        c("snd_volume_music", Real { min: 0.0, max: 1.0 }, "0.7", Yes, Shows),
        c("start", Action(None), "", Yes, Shows),
        c("stat_memory", Action(None), "", Yes, Runs),
        c("sv_addmap", Action(Some("Adds map to map rotation list")), "", Yes, Shows),
        c("sv_anomalies_enabled", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_anomalies_length", Int { min: 0, max: 180 }, "3", Yes, Shows),
        c("sv_artefact_respawn_delta", Int { min: 0, max: 600 }, "30", Yes, Shows),
        c("sv_artefact_spawn_force", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("sv_artefact_stay_time", Int { min: 0, max: 180 }, "3", Yes, Shows),
        c("sv_artefacts_count", Int { min: 1, max: 100 }, "10", Yes, Shows),
        c("sv_auto_team_balance", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("sv_auto_team_swap", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_banplayer", Action(Some("Ban Player by Name")), "", Yes, Shows),
        c("sv_banplayer_ip", Action(Some("Ban Player by IP")), "", Yes, Shows),
        c("sv_bearercantsprint", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_changegametype", Action(Some("Changing Game Type")), "", Yes, Shows),
        c("sv_changelevel", Action(Some("Changing Game Type")), "", Yes, Shows),
        c("sv_changelevelgametype", Action(Some("Changing level and game type")), "", Yes, Shows),
        c("sv_client_reconnect_time", Int { min: 0, max: 60 }, "0", Yes, Shows),
        c("sv_console_update_rate", Int { min: 1, max: 100 }, "1", Yes, Shows),
        c("sv_dedicated_server_update_rate", Int { min: 1, max: 1000 }, "100", Yes, Shows),
        c("sv_dmgblockindicator", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_dmgblocktime", Int { min: 0, max: 600 }, "0", Yes, Shows),
        c("sv_dump_online_statistics", Action(Some("Shows current server settings")), "", Yes, Runs),
        c("sv_dump_online_statistics_period", Int { min: 0, max: 60 }, "0", Yes, Shows),
        c("sv_forcerespawn", Int { min: 0, max: 3600 }, "0", Yes, Shows),
        c("sv_fraglimit", Int { min: 0, max: 100 }, "10", Yes, Shows),
        c("sv_friendly_indicators", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("sv_friendly_names", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("sv_friendlyfire", Real { min: 0.0, max: 2.0 }, "1.", Yes, Shows),
        c("sv_hail_to_winner_time", Int { min: 0, max: 60 }, "7", Yes, Shows),
        c("sv_kick", Action(Some("Kick Player by name")), "", Yes, Shows),
        c("sv_listmaps", Action(Some("List maps in map rotation list")), "", Yes, Runs),
        c("sv_listplayers", Action(Some("List Players")), "", Yes, Runs),
        c("sv_listplayers_banned", Action(Some("List of Banned Players")), "", Yes, Runs),
        c("sv_max_ping_limit", Int { min: 1, max: 2000 }, "2000", Yes, Shows),
        c("sv_nextanomalyset", Action(Some("Activating pointed Anomaly set")), "", Yes, Shows),
        c("sv_nextmap", Action(Some("Switch to Next Map in map rotation list")), "", Yes, Runs),
        c("sv_no_auth_check", Int { min: 0, max: 1 }, "0", No, Shows),
        c("sv_pda_hunt", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_prevmap", Action(Some("Switch to Previous Map in map rotation list")), "", Yes, Runs),
        c("sv_reinforcement_time", Int { min: -1, max: 3600 }, "20", Yes, Shows),
        c("sv_remove_corpse", Int { min: -1, max: 1 }, "1", Yes, Shows),
        c("sv_remove_weapon", Int { min: -1, max: 1 }, "1", Yes, Shows),
        c("sv_return_to_base", Action(None), "", Yes, Shows),
        c("sv_returnplayers", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_rpoint_freeze_time", Int { min: 0, max: 60000 }, "0", Yes, Shows),
        c("sv_setenvtime", Action(None), "", Yes, Shows),
        c("sv_setweather", Action(Some("Set new weather")), "", Yes, Shows),
        c("sv_shieldedbases", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_spectr_firsteye", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_spectr_freefly", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("sv_spectr_freelook", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_spectr_lookat", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_spectr_teamcamera", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_startteammoney", Action(Some("Set Start Team Money")), "", Yes, Runs),
        c("sv_statistic_collect", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("sv_statistic_save", Action(Some("saving statistic data")), "", Yes, Runs),
        c("sv_status", Action(Some("Shows current server settings")), "", Yes, Runs),
        c("sv_teamkill_limit", Int { min: 0, max: 100 }, "3", Yes, Shows),
        c("sv_teamkill_punish", Int { min: 0, max: 1 }, "1", Yes, Shows),
        c("sv_timelimit", Int { min: 0, max: 180 }, "0", Yes, Shows),
        c("sv_unbanplayer_ip", Action(Some("UnBan Player by IP")), "", Yes, Shows),
        c("sv_vote_enabled", Int { min: 0, max: 255 }, "255", Yes, Shows),
        c("sv_vote_participants", Int { min: 0, max: 1 }, "0", Yes, Shows),
        c("sv_vote_quota", Real { min: 0.0, max: 1.0 }, "0.51", Yes, Shows),
        c("sv_vote_time", Real { min: 0.5, max: 10.0 }, "1.", Yes, Shows),
        c("sv_votestop", Action(Some("Stops Current Voting")), "", Yes, Runs),
        c("sv_warm_up", Int { min: 0, max: 3600 }, "0", Yes, Shows),
        c("texture_lod", Int { min: 0, max: 4 }, "0", Yes, Shows),
        c("unbind", Action(None), "", Yes, Runs),
        c("unbind_console", Action(None), "", Yes, Shows),
        c("unbind_sec", Action(None), "", Yes, Runs),
        c("unbindall", Action(None), "", Yes, Runs),
        c("vid_mode", Mode, "1024x768", Yes, Shows),
        c("vid_restart", Action(None), "", Yes, Runs),
    ]
};
