// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Every program translates to WebGL 2's GLSL ES 3.00, as wgpu's GL backend
//! translates it (#113): the world's in its WebGL form
//! ([`volya_engine_render::world::world_source`]), whose per-draw data comes
//! from a texture; its WebGPU form, reading a storage buffer in its vertex
//! programs, does not translate.

use naga::back::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// Translates every entry point of `source`: the names of those that fail,
/// with why.
fn untranslatable(source: &str) -> Vec<String> {
    let module = naga::front::wgsl::parse_str(source).expect("parses");
    // WebGL 2 offers none of naga's optional capabilities.
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .expect("validates");
    let options = glsl::Options {
        version: glsl::Version::Embedded {
            version: 300,
            is_webgl: true,
        },
        writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE,
        ..Default::default()
    };
    let mut failed = Vec::new();
    for entry in &module.entry_points {
        let pipeline = glsl::PipelineOptions {
            shader_stage: entry.stage,
            entry_point: entry.name.clone(),
            multiview: None,
        };
        let mut out = String::new();
        let result = glsl::Writer::new(
            &mut out,
            &module,
            &info,
            &options,
            &pipeline,
            naga::proc::BoundsCheckPolicies::default(),
        )
        .and_then(|mut w| w.write());
        if let Err(e) = result {
            failed.push(format!("{}: {e}", entry.name));
        }
    }
    failed
}

#[test]
fn programs_translate_to_webgl2() {
    for (name, source) in [
        ("ui", include_str!("../src/ui.wgsl")),
        ("yuv", include_str!("../src/yuv.wgsl")),
        ("sky", include_str!("../src/sky.wgsl")),
        ("particle", include_str!("../src/particle.wgsl")),
        ("distortion", include_str!("../src/distortion.wgsl")),
        ("ramp", include_str!("../src/ramp.wgsl")),
    ] {
        assert_eq!(untranslatable(source), Vec::<String>::new(), "{name}");
    }
    let world = volya_engine_render::world::world_source(true);
    assert_eq!(
        untranslatable(&world),
        Vec::<String>::new(),
        "world (WebGL)"
    );
    // The WebGPU form's vertex programs read a storage buffer.
    let webgpu = untranslatable(&volya_engine_render::world::world_source(false));
    assert!(
        webgpu.iter().any(|e| e.starts_with("vs_")),
        "world (WebGPU) translated: {webgpu:?}"
    );
}
