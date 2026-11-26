// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Texture quality and texture filtering (docs/behavior/texture-quality-
//! filtering.md): how many top mip levels a texture file loses when it is
//! loaded (`texture_lod` and `[reduce_lod_texture_list]`, part 1), and the
//! maximum anisotropy and mip bias in force on the samplers (`r__tf_aniso`,
//! `r1_tf_mipbias` and the engine's own values after a device reset,
//! part 2).

use std::sync::Arc;

use volya_engine_formats::dds::Dds;
use volya_engine_formats::ltx::Config;

use crate::console::Console;

/// The section of the system configuration listing textures loaded one
/// step lower (section 1.4).
pub const LIST_SECTION: &str = "reduce_lod_texture_list";

/// `texture_lod` and the list, as a texture file is loaded (section 1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureQuality {
    /// `texture_lod`, 0 … 4.
    pub lod: u32,
    /// The keys of [`LIST_SECTION`], case kept.
    list: Arc<[String]>,
}

impl TextureQuality {
    /// `texture_lod` `lod` (clamped to 0 … 4) with the list `list`.
    pub fn new(lod: u32, list: Arc<[String]>) -> Self {
        Self {
            lod: lod.min(4),
            list,
        }
    }

    /// The keys of [`LIST_SECTION`] of the system configuration `system`
    /// (section 1.4: the entries are the keys, case kept). Without the
    /// section the list is empty.
    // TODO(#423): whether a missing section is fatal is not established.
    pub fn read_list(system: &Config) -> Arc<[String]> {
        system
            .section(LIST_SECTION)
            .map(|s| s.entries().map(|e| e.key().to_owned()).collect())
            .unwrap_or_default()
    }

    /// Whether the file at `path` is listed: a key occurs in its lowercased
    /// path (section 1.4), its separators backslashes as the keys write
    /// them. Keys with an upper-case letter never match.
    pub fn listed(&self, path: &str) -> bool {
        let path = path.to_ascii_lowercase().replace('/', "\\");
        self.list.iter().any(|k| path.contains(k.as_str()))
    }

    /// The number n of top levels to drop from the file at `path`
    /// (section 1.3): a listed file as if the setting were one step higher,
    /// at most 2.
    pub fn levels_to_drop(&self, path: &str) -> u32 {
        if self.listed(path) {
            self.lod.div_ceil(2)
        } else {
            self.lod / 2
        }
    }

    /// The texture `dds`, read from `path`, as it is loaded (section 1.5):
    /// d = min(n, L − 1) top levels dropped from a 2D texture; one-level
    /// files and cube maps as stored (sections 1.2, 1.5).
    pub fn apply<'a>(&self, path: &str, dds: &Dds<'a>) -> Dds<'a> {
        if dds.cube || dds.mip_levels <= 1 {
            return dds.clone();
        }
        dds.without_top_levels(self.levels_to_drop(path))
    }
}

/// The values of the two per-stage sampler states (section 5): the maximum
/// anisotropy and the mip level-of-detail bias.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplerState {
    /// 1 … 16.
    pub anisotropy: u16,
    pub bias: f32,
}

/// The engine's own values, set at device creation and after every device
/// reset (section 7.1).
pub const ENGINE_DEFAULTS: SamplerState = SamplerState {
    anisotropy: 4,
    bias: -0.5,
};

/// The variables (section 4).
const ANISOTROPY: &str = "r__tf_aniso";
const BIAS: &str = "r1_tf_mipbias";

/// The sampler states in force: the last writer of each wins (sections 5–7).
#[derive(Clone, Debug, PartialEq)]
pub struct SamplerStates {
    in_force: SamplerState,
    /// Each variable's execution count when last applied.
    applied: [u64; 2],
    /// Device resets since creation.
    resets: u32,
}

impl SamplerStates {
    /// At device creation (7.2): the engine's values, then both variables
    /// applied; the settings are in force.
    pub fn at_creation(console: &Console) -> Self {
        let mut s = Self {
            in_force: ENGINE_DEFAULTS,
            applied: [0; 2],
            resets: 0,
        };
        s.apply_anisotropy(console);
        s.apply_bias(console);
        s
    }

    /// The values in force.
    pub fn in_force(&self) -> SamplerState {
        self.in_force
    }

    /// The device resets ([`SamplerStates::reset`]) since creation.
    pub fn resets(&self) -> u32 {
        self.resets
    }

    /// Applies each variable executed since the last call (6.1: every
    /// execution with an argument, accepted or not, and every read of its
    /// status); each independently.
    pub fn follow(&mut self, console: &Console) {
        if console.executions(ANISOTROPY) != self.applied[0] {
            self.apply_anisotropy(console);
        }
        if console.executions(BIAS) != self.applied[1] {
            self.apply_bias(console);
        }
    }

    /// A device reset (7.3): the renderer's part applies both variables,
    /// then the engine's values are set, which stay in force until a
    /// variable is next applied.
    pub fn reset(&mut self, console: &Console) {
        self.resets = self.resets.wrapping_add(1);
        self.follow(console);
        self.apply_anisotropy(console);
        self.apply_bias(console);
        self.in_force = ENGINE_DEFAULTS;
    }

    /// clamp(`r__tf_aniso`, 1, 16) (section 5).
    fn apply_anisotropy(&mut self, console: &Console) {
        self.in_force.anisotropy = (console.number(ANISOTROPY) as i64).clamp(1, 16) as u16;
        self.applied[0] = console.executions(ANISOTROPY);
    }

    /// `r1_tf_mipbias` as stored (section 5).
    fn apply_bias(&mut self, console: &Console) {
        self.in_force.bias = console.number(BIAS);
        self.applied[1] = console.executions(BIAS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::ConsoleFiles;

    fn quality(lod: u32) -> TextureQuality {
        TextureQuality::new(
            lod,
            ["act\\act_antigas", "terrain\\terrain_escape", "wpn\\wpn_PM"]
                .map(str::to_owned)
                .into(),
        )
    }

    #[test]
    fn levels_to_drop_by_value_and_list() {
        // Section 1.3's table.
        let listed = "gamedata\\textures\\act\\act_antigas.dds";
        let other = "gamedata\\textures\\ui\\ui_common.dds";
        for (v, l, o) in [(0, 0, 0), (1, 1, 0), (2, 1, 1), (3, 2, 1), (4, 2, 2)] {
            assert_eq!(quality(v).levels_to_drop(listed), l, "listed at {v}");
            assert_eq!(quality(v).levels_to_drop(other), o, "other at {v}");
        }
    }

    #[test]
    fn list_matches_substrings_of_the_lowercased_path() {
        // Section 1.4's consequences.
        let q = quality(1);
        assert!(q.listed("gamedata\\textures\\act\\act_antigas_bump.dds"));
        assert!(q.listed("gamedata\\levels\\l01_escape\\terrain\\terrain_escape.dds"));
        assert!(q.listed("gamedata\\levels\\l01_escape\\terrain\\terrain_escape_lm.dds"));
        assert!(q.listed("GAMEDATA\\TEXTURES\\ACT\\ACT_ANTIGAS.DDS"));
        assert!(q.listed("gamedata/textures/act/act_antigas.dds"));
        // A key with an upper-case letter never matches.
        assert!(!q.listed("gamedata\\textures\\wpn\\wpn_pm.dds"));
        assert!(!q.listed("gamedata\\textures\\wpn\\wpn_PM.dds"));
        assert!(!q.listed("gamedata\\textures\\act\\act_antigaz.dds"));
    }

    #[test]
    fn sampler_states_follow_creation_settings_and_resets() {
        let mut c = Console::new(ConsoleFiles::default());
        c.exec("r__tf_aniso 16");
        c.exec("r1_tf_mipbias 0.");
        // 7.2: after creation the settings are in force.
        let mut s = SamplerStates::at_creation(&c);
        assert_eq!(
            s.in_force(),
            SamplerState {
                anisotropy: 16,
                bias: 0.0
            }
        );
        // 7.3: after a reset the engine's values, until a variable is
        // applied again, each independently.
        s.reset(&c);
        assert_eq!(s.in_force(), ENGINE_DEFAULTS);
        s.follow(&c);
        assert_eq!(s.in_force(), ENGINE_DEFAULTS);
        c.exec("r__tf_aniso 16");
        s.follow(&c);
        assert_eq!(
            s.in_force(),
            SamplerState {
                anisotropy: 16,
                bias: -0.5
            }
        );
        // 6.1: a rejected value applies the current one again; so does a
        // status read (the bare name).
        s.reset(&c);
        c.exec("r1_tf_mipbias 3");
        s.follow(&c);
        assert_eq!(
            s.in_force(),
            SamplerState {
                anisotropy: 4,
                bias: 0.0
            }
        );
        s.reset(&c);
        c.exec("r__tf_aniso");
        s.follow(&c);
        assert_eq!(
            s.in_force(),
            SamplerState {
                anisotropy: 16,
                bias: -0.5
            }
        );
        c.exec("r1_tf_mipbias 0.25");
        s.follow(&c);
        assert_eq!(
            s.in_force(),
            SamplerState {
                anisotropy: 16,
                bias: 0.25
            }
        );
    }
}
