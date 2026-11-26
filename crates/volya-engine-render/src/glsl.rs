// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The programs in WebGL 2's GLSL ES 3.00, translated from their WGSL at
//! build time (`build.rs`) as wgpu's GL backend translates them
//! (docs/architecture/rendering.md, Design: direct WebGL 2).

// Until the GL renderer links them (#475).
#![allow(dead_code)]

/// One entry point of a program.
#[derive(Debug)]
pub struct Stage {
    /// The WGSL file's name, without `.wgsl`.
    pub module: &'static str,
    /// The entry point's name.
    pub entry: &'static str,
    /// A vertex stage, else a fragment stage.
    pub vertex: bool,
    pub source: &'static str,
    /// Its samplers: each a texture, with the sampler it is read through.
    pub textures: &'static [Sampled],
    /// Its uniform blocks.
    pub blocks: &'static [Block],
}

/// A GLSL sampler: the WGSL texture and sampler it combines, as (group,
/// binding).
#[derive(Debug)]
pub struct Sampled {
    pub name: &'static str,
    pub texture: (u32, u32),
    pub sampler: Option<(u32, u32)>,
}

/// A GLSL uniform block and the WGSL uniform buffer it is, as (group,
/// binding).
#[derive(Debug)]
pub struct Block {
    pub name: &'static str,
    pub binding: (u32, u32),
}

include!(concat!(env!("OUT_DIR"), "/shaders.rs"));

/// The entry point `entry` of the program `module`.
pub fn stage(module: &str, entry: &str) -> Option<&'static Stage> {
    STAGES
        .iter()
        .find(|s| s.module == module && s.entry == entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_point_is_glsl_es_300() {
        assert!(STAGES.len() > 20, "{} stages", STAGES.len());
        for s in STAGES {
            assert!(
                s.source.starts_with("#version 300 es\n"),
                "{} {}",
                s.module,
                s.entry
            );
        }
        let ui = stage("ui", "fs").expect("ui fs");
        assert!(!ui.vertex);
        assert_eq!(ui.textures.len(), 1);
        assert_eq!(ui.textures[0].texture, (0, 0));
        assert_eq!(ui.textures[0].sampler, Some((0, 1)));
        assert!(
            STAGES
                .iter()
                .any(|s| s.module == "world" && !s.vertex && s.textures.len() > 2)
        );
    }

    #[test]
    fn uniform_blocks_name_their_buffers() {
        let ramp = stage("ramp", "fs_ramp").expect("ramp fs");
        assert_eq!(ramp.blocks.len(), 1);
        assert_eq!(ramp.blocks[0].binding, (0, 0));
    }
}
