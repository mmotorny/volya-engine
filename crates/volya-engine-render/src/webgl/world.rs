// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's static world on WebGL 2. Until #476 a stand-in: a world keeps
//! its counts and a frame of it shows its clear colour.

use std::rc::Rc;

use volya_engine_formats::level_geom::{GeomLayout, GeomSource, LevelGeom};

use crate::sky::SkyTextures;
use crate::{Renderer, Target, Texture};

pub use crate::shared::world::*;

/// The world's programs (#476).
pub(crate) struct WorldPrograms;

struct Inner {
    draws: usize,
}

/// A static world uploaded to the GPU; cheap to clone (a handle).
#[derive(Clone)]
pub struct World(Rc<Inner>);

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("draws", &self.0.draws)
            .finish()
    }
}

impl World {
    pub fn gpu_bytes(&self) -> u64 {
        0
    }

    pub fn texture_bytes(&self) -> u64 {
        0
    }

    pub fn draw_count(&self) -> usize {
        self.0.draws
    }

    pub fn object_draw_count(&self) -> usize {
        0
    }

    pub fn object_gpu_bytes(&self) -> u64 {
        0
    }

    pub fn object_texture_bytes(&self) -> u64 {
        0
    }

    pub fn skipped(&self) -> usize {
        0
    }

    pub fn sky_error(&self) -> Option<&str> {
        None
    }

    pub fn draw_sphere(&self, _draw: usize) -> Option<[f32; 4]> {
        None
    }

    pub fn same(&self, other: &World) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A level's geometry on its way to the GPU (#476).
pub struct GeometryUpload {
    draws: usize,
}

impl GeometryUpload {
    pub fn step(
        &mut self,
        _renderer: &Renderer,
        _geom: &mut dyn GeomSource,
        _objects: &ObjectGeometry,
        _budget: u64,
    ) -> Result<bool, String> {
        Ok(true)
    }

    pub fn bytes(&self) -> u64 {
        0
    }
}

impl Renderer {
    pub fn create_world(
        &mut self,
        geom: &LevelGeom,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        self.create_world_with(geom, &ObjectGeometry::default(), surfaces, draws, sky)
    }

    pub fn create_world_with(
        &mut self,
        geom: &LevelGeom,
        objects: &ObjectGeometry,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        self.create_world_from(&mut geom.source(), objects, surfaces, draws, sky)
    }

    pub fn create_world_from(
        &mut self,
        geom: &mut dyn GeomSource,
        objects: &ObjectGeometry,
        surfaces: &[Surface<Texture>],
        draws: &[WorldDraw],
        sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        let mut upload = self.begin_geometry(geom.layout(), objects, draws)?;
        upload.step(self, geom, objects, u64::MAX)?;
        self.finish_world(upload, surfaces, draws, sky)
    }

    pub fn begin_geometry(
        &mut self,
        _layout: &GeomLayout,
        _objects: &ObjectGeometry,
        draws: &[WorldDraw],
    ) -> Result<GeometryUpload, String> {
        Ok(GeometryUpload { draws: draws.len() })
    }

    pub fn finish_world(
        &mut self,
        upload: GeometryUpload,
        _surfaces: &[Surface<Texture>],
        _draws: &[WorldDraw],
        _sky: Option<&SkyTextures<Texture>>,
    ) -> Result<World, String> {
        Ok(World(Rc::new(Inner {
            draws: upload.draws,
        })))
    }

    pub fn place_draw(&self, _world: &World, _draw: usize, _transform: &Mat4, _sphere: [f32; 4]) {}

    pub(crate) fn make_world_programs(&mut self) {
        self.world_programs = Some(WorldPrograms);
    }

    /// One world frame into `screen`: until #476 its clear colour, the
    /// quads drawn after.
    pub(crate) fn present_frame(
        &mut self,
        screen: &Target,
        frame: &WorldFrame,
        _quads: &[crate::Quad],
        _batches: &[(usize, std::ops::Range<u32>)],
    ) -> bool {
        let [r, g, b] = frame.clear;
        Self::begin_pass(
            &self.gl,
            &screen.framebuffer,
            screen.size(),
            Some([r, g, b, 1.0]),
            None,
        );
        false
    }

    pub fn world_calls_emitted(&self) -> [u32; 5] {
        EMITTED.get()
    }

    pub fn set_unblended_trees(&mut self, on: bool) {
        self.unblended_trees = on;
        self.make_world_programs();
    }

    pub fn light_draws(&self) -> usize {
        self.light_draws
    }

    pub fn set_light_textures(&mut self, textures: LightTextures) {
        self.light_textures = Some(textures);
    }
}
