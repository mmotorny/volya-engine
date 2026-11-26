// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The statistics overlay of `rs_stats` (docs/behavior/display-mode.md
//! part B): drawn over everything of a frame while the variable is on, its
//! values measured only meanwhile. Volya Engine measures the frame (FPS),
//! the engine's update, the drawing and the sound, and counts the draw
//! calls; every other timer and count of B.4 has nothing here to measure
//! and shows 0.

use std::io;

use crate::data::{Assets, GameData};
use crate::scene::{Scene, Sprite, TextureId};
use volya_engine_formats::ltx::Config;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::layout::draw_line;
use volya_engine_formats::ui::Color;
use volya_engine_formats::ui::layout::HAlign;

use crate::core::vfs::Vfs;

/// The layer the overlay's glyphs are drawn in, over everything.
pub const LAYER: &str = "stats";

/// One frame's measurements (B.4.1), milliseconds, from the platform.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameTimes {
    /// The engine's update.
    pub engine_ms: f64,
    /// Drawing the frame, from its start to its presentation.
    pub render_ms: f64,
    /// The sound system's frame.
    pub sound_ms: f64,
    /// Draw calls.
    pub draws: u32,
}

/// A timer's shown value (B.4.1): the frame's total when above it, else
/// 0.99 of it plus 0.01 of the total.
fn decay(v: &mut f64, t: f64) {
    *v = if t > *v { t } else { 0.99 * *v + 0.01 * t };
}

/// The overlay's values between frames.
#[derive(Clone, Debug, PartialEq)]
pub struct StatisticsOverlay {
    /// `[evaluation]` `line1` … `line3` (B.3).
    evaluation: [String; 3],
    fps: f64,
    rfps: f64,
    tps: f64,
    engine: f64,
    render: f64,
    sound: f64,
    draws: u32,
    /// The latest frame's measurements, taken at the next overlay.
    latest: FrameTimes,
    /// Frames drawn with the overlay (B.3's notice alternates by it).
    frames: u64,
}

impl StatisticsOverlay {
    /// With the system configuration's `[evaluation]` lines, which must
    /// exist (B.3: fatal otherwise).
    pub fn new(system: &Config) -> io::Result<Self> {
        let section = system
            .section("evaluation")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no section [evaluation]"))?;
        let line = |key: &str| {
            section
                .get(key)
                .map(|v| v.unwrap_or("").trim().trim_matches('"').to_owned())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("no key {key} in [evaluation]"),
                    )
                })
        };
        Ok(Self {
            evaluation: [line("line1")?, line("line2")?, line("line3")?],
            fps: 30.0,
            rfps: 30.0,
            tps: 0.0,
            engine: 0.0,
            render: 0.0,
            sound: 0.0,
            draws: 0,
            latest: FrameTimes::default(),
            frames: 0,
        })
    }

    /// The platform's measurements of the frame just drawn.
    pub fn measured(&mut self, times: FrameTimes) {
        self.latest = times;
    }

    /// A frame with the overlay, `dt` seconds after the previous one: the
    /// values updated as B.4.1 says (polygons are not counted here: TPS
    /// takes 0).
    pub fn frame(&mut self, dt: f64) {
        let t = std::mem::take(&mut self.latest);
        decay(&mut self.engine, t.engine_ms);
        decay(&mut self.render, t.render_ms);
        decay(&mut self.sound, t.sound_ms);
        self.draws = t.draws;
        if dt > 1e-7 {
            self.fps = 0.7 * self.fps + 0.3 / dt;
            let r = self.render;
            if r > 1e-7 {
                self.rfps = 0.7 * self.rfps + 300.0 / r;
                self.tps *= 0.7;
            }
        }
        self.frames += 1;
    }

    /// Block 2's entries (B.4), `None` for a blank line.
    pub fn engine_lines(&self) -> Vec<Option<String>> {
        let d = self.draws;
        let (e, r, s) = (self.engine, self.render, self.sound);
        let z = "0.00";
        let pe = |v: f64| if e > 0.0 { 100.0 * v / e } else { 0.0 };
        let mut lines: Vec<Option<String>> = vec![
            Some(format!("FPS/RFPS:    {:3.1}/{:3.1}", self.fps, self.rfps)),
            Some(format!("TPS:         {:2.2} M", self.tps)),
            Some("VERT:        0/0".into()),
            Some("POLY:        0/0".into()),
            Some(format!("DIP/DP:      {d}")),
            Some("xforms:      0".into()),
            None,
            Some(format!("*** ENGINE:  {e:2.2}ms")),
            Some(format!("Memory:      {z}a")),
            Some(format!(
                "uClients:    {z}ms, {:2.1}%, crow(0)/active(0)/total(0)",
                pe(0.0)
            )),
            Some(format!("uSheduler:   {z}ms, {:2.1}%", pe(0.0))),
            Some(format!("uSheduler_L: {z}ms")),
            Some("uParticles:  Qstart[0] Qactive[0] Qdestroy[0]".into()),
            Some("spInsert:    o[0.00ms, 0.0%], p[0.00ms, 0.0%]".into()),
            Some("spRemove:    o[0.00ms, 0.0%], p[0.00ms, 0.0%]".into()),
            Some(format!("Physics:     {z}ms, {:2.1}%", pe(0.0))),
            Some(format!("  collider:  {z}ms")),
        ];
        let counted = |label: &str| Some(format!("{label}{z}ms, 0"));
        lines.extend([
            counted("  solver:    "),
            counted("aiThink:     "),
            counted("  aiRange:   "),
            counted("  aiPath:    "),
            counted("  aiNode:    "),
            counted("aiVision:    "),
            Some(format!("  Query:     {z}ms")),
            Some(format!("  RayCast:   {z}ms")),
            None,
            Some(format!("*** RENDER:  {r:2.2}ms")),
            Some(format!("R_CALC:      {z}ms, 0.0%")),
            counted("  HOM:       "),
            counted("  Skeletons: "),
            Some(format!("R_DUMP:      {z}ms, 0.0%")),
        ]);
        for label in [
            "  Wait-L:    ",
            "  Wait-S:    ",
            "  Skinning:  ",
            "  DT_Vis/Cnt:",
            "  DT_Render: ",
            "  DT_Cache:  ",
        ] {
            lines.push(Some(format!("{label}{z}ms")));
        }
        lines.extend([
            Some(format!("  Wallmarks: {z}ms, 0/0 - 0")),
            Some(format!("  Glows:     {z}ms")),
            counted("  Lights:    "),
            counted("  RT:        "),
            Some(format!("  HUD:       {z}ms")),
            Some(format!("  P_calc:    {z}ms")),
            Some(format!("  S_calc:    {z}ms")),
            counted("  S_render:  "),
            None,
            Some(format!("*** SOUND:   {s:2.2}ms")),
            Some("  TGT/SIM/E: 0/0/0".into()),
            Some("  HIT/MISS:  0/0".into()),
            None,
            Some(format!("Input:       {z}ms")),
            Some(format!("clRAY:       {z}ms, 0,  0K")),
            Some(format!("clBOX:       {z}ms, 0,  0K")),
            counted("clFRUSTUM:   "),
            None,
            counted("netClientRecv:   "),
            counted("netClientSend:   "),
            counted("netServer:   "),
            None,
            None,
        ]);
        for k in 0..4 {
            lines.push(counted(&format!("TEST {k}:      ")));
        }
        lines.extend([None, Some("qpc[  0]".into()), None]);
        for label in [
            "static:        ",
            "flora:         ",
            "  flora_lods:  ",
            "dynamic:       ",
            "  dynamic_sw:  ",
            "  dynamic_inst:",
            "  dynamic_1B:  ",
            "  dynamic_2B:  ",
            "details:       ",
        ] {
            lines.push(Some(format!("{label}0.0/0")));
        }
        lines
    }

    /// Block 3 (B.5): the first renderer's lines, its two counts 0 here.
    pub fn renderer_lines() -> Vec<Option<String>> {
        vec![
            Some(format!(" **** Occ-Q({:03.1}) **** ", 0.0)),
            Some(format!(" total  : {:2}", 0)),
            Some(format!(" culled : {:2}", 0)),
            None,
        ]
    }

    /// Adds the overlay's glyphs to `scene` at output height `height`, the
    /// camera at `camera` (B.3 – B.6).
    pub fn draw(
        &self,
        tree: &dyn Vfs,
        data: &GameData,
        assets: &mut Assets,
        scene: &mut Scene,
        camera: [f32; 3],
    ) -> io::Result<()> {
        let height = scene.resolution.height;
        let mut font = assets.fonts(tree, data, height)?.get(UiFont::Small).clone();
        let path = Assets::font_texture(data, &font.texture)?;
        let size = assets.image_size(tree, &path)?;
        let white = Color::rgba(255, 255, 255, 255);
        let mut block = |font: &volya_engine_formats::text::font::Font,
                         lines: &[Option<String>],
                         (x, mut y): (f32, f32)| {
            for line in lines {
                if let Some(text) = line {
                    let text = &text.as_bytes()[..text.len().min(511)];
                    for g in draw_line(font, text, (x, y), white, HAlign::Left) {
                        scene.sprites.push(Sprite::glyph(
                            LAYER,
                            TextureId::Image { path: path.clone() },
                            &g,
                            size,
                        ));
                    }
                }
                y += font.height;
            }
        };
        font.height = 0.01 * height as f32;
        if self.frames % 2000 < 1000 {
            let notice: Vec<Option<String>> =
                self.evaluation.iter().map(|l| Some(l.clone())).collect();
            block(&font, &notice, (0.0, 0.0));
        }
        block(&font, &self.engine_lines(), (0.0, 0.0));
        block(&font, &Self::renderer_lines(), (200.0, 0.0));
        font.height = 0.02 * height as f32;
        let [x, y, z] = camera;
        let line = format!("CAMERA POSITION:  [{x:3.2},{y:3.2},{z:3.2}]");
        block(&font, &[Some(line)], (10.0, 600.0));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay() -> StatisticsOverlay {
        let system = volya_engine_formats::ltx::parse(
            b"[evaluation]\nline1 = \"\"\nline2 = \"\"\nline3 = \"\"\n",
        )
        .unwrap();
        StatisticsOverlay::new(&system).unwrap()
    }

    #[test]
    fn block_2_has_67_lines_and_8_blank_steps() {
        // B.8 2: 67 text lines and 8 blank-line steps, one of them double
        // (two blank lines here).
        let lines = overlay().engine_lines();
        assert_eq!(lines.iter().filter(|l| l.is_some()).count(), 67);
        assert_eq!(lines.iter().filter(|l| l.is_none()).count(), 9);
        assert_eq!(lines[0].as_deref(), Some("FPS/RFPS:    30.0/30.0"));
    }

    #[test]
    fn values_follow_b_4_1() {
        let mut o = overlay();
        o.measured(FrameTimes {
            engine_ms: 4.0,
            render_ms: 10.0,
            sound_ms: 0.0,
            draws: 7,
        });
        o.frame(0.02);
        // FPS 0.7 · 30 + 0.3 / 0.02; RFPS 0.7 · 30 + 300 / 10.
        assert!((o.fps - 36.0).abs() < 1e-9);
        assert!((o.rfps - 51.0).abs() < 1e-9);
        assert_eq!((o.engine, o.render, o.draws), (4.0, 10.0, 7));
        // A smaller total decays the peak.
        o.measured(FrameTimes {
            engine_ms: 2.0,
            ..Default::default()
        });
        o.frame(0.0);
        assert!((o.engine - (0.99 * 4.0 + 0.02)).abs() < 1e-9);
        assert_eq!(o.engine_lines()[4].as_deref(), Some("DIP/DP:      0"));
    }

    #[test]
    fn the_evaluation_lines_must_exist() {
        let system = volya_engine_formats::ltx::parse(b"[evaluation]\nline1 = a\n").unwrap();
        assert!(StatisticsOverlay::new(&system).is_err());
    }
}
