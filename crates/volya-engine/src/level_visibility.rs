// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's camera pass (docs/behavior/visibility-scope.md): its
//! sectors, portals and static visuals (`volya-engine-core::visibility`),
//! its occlusion (`volya-engine-core::occlusion`), the camera's sector, and
//! the latest pass's result, which the dynamic items are tested against
//! (docs/behavior/object-visibility.md 4 – 6).

use std::io;

use crate::core::camera::Vec3;
use crate::core::collision::CollisionWorld;
use crate::core::occlusion::{Cached, Occlusion, OcclusionMesh, View};
use crate::core::particles::random::Generator;
use crate::core::visibility::{CameraFrame, CameraSector, Sectors, triangle_below};
use volya_engine_formats::level::{Level, Thresholds};

/// The camera pass's state for a level's run.
#[derive(Debug)]
pub struct LevelVisibility {
    sectors: Sectors,
    occlusion: Occlusion,
    camera: CameraSector,
    frame: CameraFrame,
    /// The latest pass's view.
    view: Option<View>,
}

impl Default for LevelVisibility {
    /// A level without sectors: nothing is ever visited.
    fn default() -> Self {
        Self {
            sectors: Sectors::default(),
            occlusion: Occlusion::new(None, 0),
            camera: CameraSector::default(),
            frame: CameraFrame::default(),
            view: None,
        }
    }
}

impl LevelVisibility {
    /// The sectors of `level` and its occlusion mesh, none without
    /// `level.hom`.
    pub fn new(level: &Level, mesh: Option<OcclusionMesh>) -> io::Result<Self> {
        let sectors = Sectors::new(level)?;
        let occlusion = Occlusion::new(mesh, sectors.visual_count());
        Ok(Self {
            sectors,
            occlusion,
            ..Self::default()
        })
    }

    /// The camera pass of `view` (visibility-scope.md 1): the camera's
    /// sector found again when it moved (level.md 9.1), then the grid, the
    /// traversal and the static visuals, the tests' values from `random`.
    pub fn camera_pass(
        &mut self,
        view: View,
        thresholds: &Thresholds,
        collision: &CollisionWorld,
        random: &mut Generator,
    ) {
        let sector = self.camera.update(view.eye, &self.sectors, collision);
        self.frame =
            (self.sectors).camera_pass(sector, &view, thresholds, &mut self.occlusion, random);
        self.view = Some(view);
    }

    /// The camera's sector, none until one is found (visibility-scope.md 6).
    pub fn camera_sector(&self) -> Option<u16> {
        self.camera.get()
    }

    /// The latest camera pass's result.
    pub fn frame(&self) -> &CameraFrame {
        &self.frame
    }

    /// The sector the downward ray of level.md 9.1 finds from `p`, if any.
    pub fn sector_of(&self, collision: &CollisionWorld, p: Vec3) -> Option<u16> {
        (self.sectors).sector_below(p, triangle_below(collision, p))
    }

    /// The sector and frusta test (object-visibility.md 5): the first
    /// frustum `sector` was reached with in the latest pass that the
    /// sphere is not outside, none when the sector was not visited.
    pub fn reached(&self, sector: u16, centre: Vec3, radius: f32) -> Option<usize> {
        (self.frame.traversal.frusta.get(usize::from(sector))?)
            .iter()
            .position(|f| f.meets(centre, radius))
    }

    /// The cached occlusion test (occlusion-tests.md 5) of the box `min` …
    /// `max` with the item's cache, in the latest pass's view; visible
    /// before any pass.
    pub fn cached_visible(
        &mut self,
        cache: &mut Cached,
        min: Vec3,
        max: Vec3,
        random: &mut Generator,
    ) -> bool {
        let Some(view) = self.view else {
            return true;
        };
        (self.occlusion).cached_visible(cache, min, max, &view, random)
    }

    /// Runs `f`, the light passes (occlusion-tests.md 4), with occlusion
    /// off.
    pub fn light_passes<R>(&mut self, f: impl FnOnce() -> R) -> R {
        self.occlusion.light_passes(|_| f())
    }
}
