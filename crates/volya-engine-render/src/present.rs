// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation (docs/behavior/frame-presentation.md): the world
//! drawn into an off-screen target sized by `r__supersample` and reduced
//! to the screen by the presentation quad, through `postprocess` or, after
//! a distortion pass, `postprocess_d`.

use wgpu::util::DeviceExt;

use crate::world::DEPTH_FORMAT;

/// The screen-effect values the renderer uses (section 2.1), as stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenEffects {
    pub blur: f32,
    pub gray: f32,
    /// dual.h, dual.v: absolute values.
    pub dual: [f32; 2],
    pub noise_intensity: f32,
    pub noise_grain: f32,
    /// |value| + 10⁻⁷.
    pub noise_fps: f32,
    pub color_base: [u8; 3],
    pub color_gray: [u8; 3],
    pub color_add: [u8; 3],
}

/// The ten values of an effect set as the engine combines them (2.2),
/// colours as reals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectValues {
    pub blur: f32,
    pub gray: f32,
    pub dual: [f32; 2],
    pub noise_intensity: f32,
    pub noise_grain: f32,
    pub noise_fps: f32,
    pub color_base: [f32; 3],
    pub color_gray: [f32; 3],
    pub color_add: [f32; 3],
}

impl EffectValues {
    /// The engine's identity (2.1).
    pub const IDENTITY: Self = Self {
        blur: 0.0,
        gray: 0.0,
        dual: [0.0; 2],
        noise_intensity: 0.0,
        noise_grain: 1.0,
        noise_fps: 30.0,
        color_base: [0.5; 3],
        color_gray: [0.333; 3],
        color_add: [0.0; 3],
    };
}

/// The renderer's margin below which a value counts as off (2.4).
const OFF: f32 = 0.001;

impl ScreenEffects {
    /// The renderer's own defaults, in force after the target is (re)built
    /// until the first hand-over (2.3).
    pub const DEFAULTS: Self = Self {
        blur: 0.0,
        gray: 0.0,
        dual: [0.0; 2],
        noise_intensity: 0.0,
        noise_grain: 1.0,
        noise_fps: 25.0,
        color_base: [127; 3],
        color_gray: [85; 3],
        color_add: [0; 3],
    };

    /// The set as the engine hands it to the renderer (2.2 steps 4, 5)
    /// and the renderer stores it (2.1).
    pub fn handed_over(v: &EffectValues) -> Self {
        let grain = if v.noise_grain < 0.0 {
            1.0
        } else {
            v.noise_grain
        };
        let byte = |c: f32| (c * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
        Self {
            blur: v.blur,
            gray: v.gray,
            dual: v.dual.map(f32::abs),
            noise_intensity: v.noise_intensity,
            noise_grain: grain.clamp(0.001, 1000.0),
            noise_fps: v.noise_fps.abs() + 1e-7,
            color_base: v.color_base.map(byte),
            color_gray: v.color_gray.map(byte),
            color_add: v.color_add.map(byte),
        }
    }

    /// The identity handed over: what a level hands over while no effect
    /// contributes (2.2 step 3) and while the main menu shows.
    pub fn identity() -> Self {
        Self::handed_over(&EffectValues::IDENTITY)
    }

    /// Whether the effects count as active (2.4); the user interface never
    /// asks for the post-process here (its world is not drawn under the
    /// menu).
    pub fn active(&self) -> bool {
        !(self.blur <= OFF
            && self.gray <= OFF
            && self.noise_intensity <= OFF
            && self.dual.iter().all(|&d| d <= OFF)
            && self.color_base.iter().all(|&b| (125..=129).contains(&b))
            && self.color_add.iter().all(|&b| b <= 2))
    }
}

/// What a frame's presentation takes from the engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presentation {
    /// `r__supersample` as the last video restart applied it, 1 … 4.
    pub supersample: u32,
    /// `r1_pps_u`, `r1_pps_v`: the quad's offset in pixels.
    pub pps: [f32; 2],
    /// The engine's frame time, seconds (the noise's countdown, 3.2).
    pub frame_time: f32,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            supersample: 1,
            pps: [0.0; 2],
            frame_time: 0.0,
        }
    }
}

/// The off-screen target's size for a `width` × `height` screen and
/// `r__supersample` `s` (section 1.1): each axis floor(N √s + 0.5),
/// clamped to 128 … 2048, then reduced by 1 if odd.
pub fn target_size(width: u32, height: u32, s: u32) -> (u32, u32) {
    let root = (s.clamp(1, 4) as f32).sqrt();
    let axis = |n: u32| {
        let v = ((n as f32 * root + 0.5).floor() as u32).clamp(128, 2048);
        v - (v & 1)
    };
    (axis(width), axis(height))
}

/// One corner of the quad: its Direct3D 9 screen position and texture
/// coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub position: [f32; 2],
    pub tc0: [f32; 2],
    pub tc1: [f32; 2],
    pub tc2: [f32; 2],
}

/// The noise cell n (3.2), at least 1: the grain's clamp keeps it so.
fn noise_cell(grain: f32) -> u32 {
    ((grain * 256.0 + 1e-7).ceil() as u32).max(1)
}

/// The quad's corners (3.2) in the order left top, right top, left bottom,
/// right bottom, for a `screen` of W × H, the `target`'s W_t × H_t, the
/// offset `pps` (U, V) and the noise offsets (o_x, o_y). `effects.blur`
/// must already be 1 where H_t ≠ H.
pub fn quad(
    screen: (u32, u32),
    target: (u32, u32),
    effects: &ScreenEffects,
    [u, v]: [f32; 2],
    [ox, oy]: [u32; 2],
) -> [Corner; 4] {
    let (w, h) = (screen.0 as f32, screen.1 as f32);
    let beta = effects.blur;
    let [dh, dv] = effects.dual;
    let a0 = (0.5 + 0.5 * beta) / target.0 as f32;
    let b0 = (0.5 + 0.5 * beta) / target.1 as f32;
    let (a1, b1) = (1.0 + a0, 1.0 + b0);
    let (a1p, b1p) = (a1 - dh / 2.0, b1 - dv / 2.0);
    let (a0p, b0p) = (a0 + dh / 2.0, b0 + dv / 2.0);
    let n = noise_cell(effects.noise_grain);
    let x0 = (ox as f32 + 0.5) / n as f32;
    let y0 = (oy as f32 + 0.5) / n as f32;
    let x1 = (screen.0 / n) as f32 + x0 + 1.0;
    let y1 = (screen.1 / n) as f32 + y0 + 1.0;
    [
        Corner {
            position: [u, v],
            tc0: [a0, b0],
            tc1: [a0p, b0p],
            tc2: [x0, y0],
        },
        Corner {
            position: [u + w, v],
            tc0: [a1p, b0],
            tc1: [a1, b0p],
            tc2: [x1, y0],
        },
        Corner {
            position: [u, v + h],
            tc0: [a0, b1p],
            tc1: [a0p, b1],
            tc2: [x0, y1],
        },
        Corner {
            position: [u + w, v + h],
            tc0: [a1p, b1p],
            tc1: [a1, b1],
            tc2: [x1, y1],
        },
    ]
}

/// The target and what is built with it (1.2): the colour target
/// `$user$rendertarget`, the distortion target `$user$distort` and a depth
/// buffer, all W_t × H_t.
#[derive(Clone)]
pub(crate) struct Targets {
    screen: (u32, u32),
    supersample: u32,
    pub(crate) size: (u32, u32),
    pub(crate) colour: wgpu::Texture,
    pub(crate) distort: wgpu::Texture,
    pub(crate) depth: wgpu::Texture,
}

/// Bytes of the quad's constants.
const CONSTANTS: u64 = 12 * 16;

/// The presentation's pipelines, the target and the values in force.
pub(crate) struct PostProcess {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    postprocess: wgpu::RenderPipeline,
    postprocess_d: wgpu::RenderPipeline,
    constants: wgpu::Buffer,
    scene_sampler: wgpu::Sampler,
    noise_sampler: wgpu::Sampler,
    /// `fx\fx_noise2` once given; white until then.
    noise: wgpu::TextureView,
    pub(crate) targets: Option<Targets>,
    /// The values last handed over, or the defaults since the target was
    /// built (2.3).
    pub(crate) effects: ScreenEffects,
    /// The noise's countdown T and offsets (3.2).
    countdown: f32,
    offsets: [u32; 2],
    /// The shared generator's state for the noise offsets: not
    /// reproducible in the original, so any start serves.
    random: u32,
    /// The previous frame had a distortion pass (3.1).
    pub(crate) distorted: bool,
    /// The last world frame went through the target.
    pub(crate) used: bool,
}

impl PostProcess {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("postprocess"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(CONSTANTS),
                    },
                    count: None,
                },
                texture(1),
                sampler(2),
                texture(3),
                sampler(4),
                texture(5),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("postprocess"),
            source: wgpu::ShaderSource::Wgsl(include_str!("postprocess.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("postprocess"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // Pass states (3.4): no depth test or write, no blending.
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_quad"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let postprocess = pipeline("fs_postprocess");
        let postprocess_d = pipeline("fs_postprocess_d");
        let constants = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("postprocess"),
            size: CONSTANTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // s_base0, s_base1: clamped, linear without mip; s_noise wrapped,
        // linear without mip (3.4).
        let scene_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("postprocess scene"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("postprocess noise"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let noise = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("postprocess no noise"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &[255; 4],
            )
            .create_view(&Default::default());
        Self {
            format,
            layout,
            postprocess,
            postprocess_d,
            constants,
            scene_sampler,
            noise_sampler,
            noise,
            targets: None,
            effects: ScreenEffects::DEFAULTS,
            countdown: 0.0,
            offsets: [0; 2],
            random: 1,
            distorted: false,
            used: false,
        }
    }

    /// The noise texture: its top level only is sampled (3.4).
    pub(crate) fn set_noise(&mut self, texture: &wgpu::Texture) {
        self.noise = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: 0,
            mip_level_count: Some(1),
            array_layer_count: Some(1),
            ..Default::default()
        });
    }

    /// Builds the target for a `screen` and `supersample` when either
    /// differs from the one built (as the original at device creation and
    /// every reset, 1.2): the values in force return to the defaults.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, screen: (u32, u32), supersample: u32) {
        let built = self
            .targets
            .as_ref()
            .is_some_and(|t| t.screen == screen && t.supersample == supersample);
        if built || screen.0 == 0 || screen.1 == 0 {
            return;
        }
        let size = target_size(screen.0, screen.1, supersample);
        let make = |label, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let colour = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        self.targets = Some(Targets {
            screen,
            supersample,
            size,
            colour: make("$user$rendertarget", self.format, colour),
            distort: make("$user$distort", self.format, colour),
            depth: make(
                "$user$rendertarget depth",
                DEPTH_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        });
        self.effects = ScreenEffects::DEFAULTS;
        self.countdown = 0.0;
    }

    /// Whether the frame goes through the target (3.1 step 1).
    pub(crate) fn use_target(&self, supersample: u32) -> bool {
        self.targets.is_some() && (self.effects.active() || supersample > 1 || self.distorted)
    }

    /// One draw of the shared generator.
    fn draw_random(&mut self) -> u32 {
        self.random = self.random.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.random >> 16) & 0x7FFF
    }

    /// The presentation quad (3.1 step 5) into `screen`, with
    /// `postprocess_d` when `distorted`.
    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        screen: &wgpu::TextureView,
        presentation: &Presentation,
        distorted: bool,
    ) {
        let Some(t) = self.targets.clone() else {
            return;
        };
        if t.size.1 != t.screen.1 {
            self.effects.blur = 1.0;
        }
        let n = noise_cell(self.effects.noise_grain);
        self.countdown -= presentation.frame_time;
        if self.countdown < 0.0 {
            let ox = self.draw_random() % n;
            let oy = self.draw_random() % n;
            self.offsets = [ox, oy];
            while self.countdown < 0.0 {
                self.countdown += 1.0 / self.effects.noise_fps;
            }
        }
        let e = &self.effects;
        let corners = quad(t.screen, t.size, e, presentation.pps, self.offsets);
        let mut bytes = Vec::with_capacity(CONSTANTS as usize);
        let mut push = |v: [f32; 4]| {
            for x in v {
                bytes.extend_from_slice(&x.to_le_bytes());
            }
        };
        for c in &corners {
            push([c.position[0], c.position[1], c.tc0[0], c.tc0[1]]);
            push([c.tc1[0], c.tc1[1], c.tc2[0], c.tc2[1]]);
        }
        let unit = |b: u8| f32::from(b) / 255.0;
        let alpha = |v: f32| ((1.0 - v) * 255.0).floor().clamp(0.0, 255.0) / 255.0;
        let [r, g, b] = e.color_base.map(unit);
        push([r, g, b, alpha(e.noise_intensity)]);
        let [r, g, b] = e.color_gray.map(unit);
        push([r, g, b, alpha(e.gray)]);
        let [r, g, b] = e.color_add.map(unit);
        push([r, g, b, 0.0]);
        push([t.screen.0 as f32, t.screen.1 as f32, 0.0, 0.0]);
        queue.write_buffer(&self.constants, 0, &bytes);
        let colour = t.colour.create_view(&Default::default());
        let distort = t.distort.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("postprocess"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.constants.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&colour),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.scene_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.noise),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.noise_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&distort),
                },
            ],
        });
        let timed = crate::timing::pass_writes("presentation", false);
        crate::stats::pass();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("postprocess"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: screen,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: crate::timing::writes(&timed),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        crate::stats::pipeline();
        pass.set_pipeline(if distorted {
            &self.postprocess_d
        } else {
            &self.postprocess
        });
        crate::stats::bind_group();
        pass.set_bind_group(0, &group, &[]);
        crate::stats::draw();
        pass.draw(0..4, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_sizes() {
        // Section 1.1's table.
        let cases = [
            ((1024, 768), 1, (1024, 768)),
            ((1024, 768), 2, (1448, 1086)),
            ((1024, 768), 3, (1774, 1330)),
            ((1024, 768), 4, (2048, 1536)),
            ((800, 600), 2, (1130, 848)),
            ((1280, 1024), 2, (1810, 1448)),
            ((1920, 1080), 2, (2048, 1526)),
            ((2560, 1440), 1, (2048, 1440)),
        ];
        for ((w, h), s, want) in cases {
            assert_eq!(target_size(w, h, s), want, "{w}×{h} s {s}");
        }
    }

    /// Where pixel `i` of a screen `n` wide samples the target, in target
    /// texels with centres at integers (3.5).
    fn sample_at(c: &[Corner; 4], i: f32, n: f32, target: f32, axis: usize) -> f32 {
        let (a, b) = (c[0].tc0[axis], c[3].tc0[axis]);
        let at = c[0].position[axis];
        (a + (i - at) * (b - a) / n) * target - 0.5
    }

    #[test]
    fn the_shipped_presentation_samples_between_texels() {
        // 3.5 and section 5: 1024 × 768 at r__supersample 2, the identity.
        let target = target_size(1024, 768, 2);
        let mut e = ScreenEffects::identity();
        // H_t ≠ H: β = 1.
        e.blur = 1.0;
        let c = quad((1024, 768), target, &e, [0.0; 2], [0; 2]);
        for i in [0.0, 1.0, 100.0, 1023.0] {
            let p = sample_at(&c, i, 1024.0, 1448.0, 0);
            assert!((p - (0.5 + 1.414_062_5 * i)).abs() < 1e-3, "{i}: {p}");
            let p = sample_at(&c, i, 768.0, 1086.0, 1);
            assert!((p - (0.5 + 1.414_062_5 * i)).abs() < 1e-3, "{i}: {p}");
        }
        // No duality: both look-ups alike.
        assert!(c.iter().all(|c| c.tc0 == c.tc1));
    }

    #[test]
    fn at_screen_size_each_pixel_takes_one_texel() {
        let e = ScreenEffects::identity();
        let c = quad((1024, 768), (1024, 768), &e, [0.0; 2], [0; 2]);
        for i in [0.0, 511.0, 1023.0] {
            assert!((sample_at(&c, i, 1024.0, 1024.0, 0) - i).abs() < 1e-3);
        }
        // Duality: the second look-up shifted by half the values (3.2).
        let mut e = e;
        e.dual = [0.1, 0.2];
        let c = quad((1024, 768), (1024, 768), &e, [0.0; 2], [0; 2]);
        assert!((c[0].tc1[0] - c[0].tc0[0] - 0.05).abs() < 1e-6);
        assert!((c[3].tc0[1] - (1.0 + 1.0 / 1536.0 - 0.1)).abs() < 1e-6);
    }

    #[test]
    fn noise_coordinates() {
        // 3.2: n = 256 at grain 1; x₁ = ⌊W / n⌋ + x₀ + 1.
        let e = ScreenEffects::identity();
        let c = quad((1024, 768), (1024, 768), &e, [0.0; 2], [3, 7]);
        assert_eq!(c[0].tc2, [3.5 / 256.0, 7.5 / 256.0]);
        assert_eq!(c[3].tc2, [4.0 + 3.5 / 256.0 + 1.0, 3.0 + 7.5 / 256.0 + 1.0]);
        // The pps offset moves the positions only.
        let moved = quad((1024, 768), (1024, 768), &e, [0.5, -1.0], [3, 7]);
        assert_eq!(moved[3].position, [1024.5, 767.0]);
        assert_eq!(moved[3].tc0, c[3].tc0);
    }

    #[test]
    fn the_identity_and_the_defaults_are_inactive() {
        // 2.1, 2.4.
        let id = ScreenEffects::identity();
        assert_eq!(id.color_base, [128; 3]);
        assert_eq!(id.color_gray, [85; 3]);
        assert_eq!(id.color_add, [0; 3]);
        assert!((id.noise_fps - 30.0).abs() < 1e-5);
        assert!(!id.active() && !ScreenEffects::DEFAULTS.active());
        for change in [
            |e: &mut ScreenEffects| e.blur = 0.01,
            |e: &mut ScreenEffects| e.gray = 0.01,
            |e: &mut ScreenEffects| e.dual[1] = 0.01,
            |e: &mut ScreenEffects| e.noise_intensity = 0.01,
            |e: &mut ScreenEffects| e.color_base[2] = 130,
            |e: &mut ScreenEffects| e.color_base[0] = 124,
            |e: &mut ScreenEffects| e.color_add[1] = 3,
        ] {
            let mut e = id;
            change(&mut e);
            assert!(e.active(), "{e:?}");
        }
        // color_gray, grain and fps never make them active.
        let mut e = id;
        e.color_gray = [200; 3];
        e.noise_grain = 3.0;
        e.noise_fps = 1.0;
        assert!(!e.active());
        // Step 4, 5: a negative grain is 1, then clamped; dual stored absolute.
        let mut v = EffectValues::IDENTITY;
        v.noise_grain = -2.0;
        v.dual = [-0.5, 0.25];
        let h = ScreenEffects::handed_over(&v);
        assert_eq!((h.noise_grain, h.dual), (1.0, [0.5, 0.25]));
        v.noise_grain = 5000.0;
        assert_eq!(ScreenEffects::handed_over(&v).noise_grain, 1000.0);
    }
}
