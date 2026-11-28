// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The motions the level's objects play (docs/behavior/motion-playback.md,
//! TC1): an object's model's motion sets read when it first plays one
//! (docs/formats/skeletal-motions.md 1), its playbacks advanced by the
//! tracks update each frame (section 4), and its bones' render matrices
//! (model-shading.md 2), which the drawing skins its meshes by.
//!
//! Every animated object's matrices are computed each frame
//! `time_global()` changes, as if each request insisted (motion-playback
//! 7.4).
// TODO(#1082): which requests for bone matrices the drawing makes (7.4:
// the 100 ms gate), and whether an object not drawn is updated (4.1), are
// not covered by the spec.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use crate::core::head_bone_turning::{turn, turning_angle};
use crate::core::motion_playback::{AnimatedModel, Clock, Motions, Notified};
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::objects::{LoadedModel, Names};
use crate::render::world::{BoneMatrix, bone_matrix};
use volya_engine_formats::model::Mat4;
use volya_engine_formats::skeletal_motions::{self, ModelMotions, Motion, MotionParams};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn context(path: &str, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{path}: {e}"))
}

/// The file of motion-file list item `item` (skeletal-motions 1.2):
/// `$level$` + item + `.omf`, else `$game_meshes$` + item + `.omf`; neither
/// is "Can't find motion file".
pub fn locate_motion_file(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    item: &str,
) -> io::Result<String> {
    let name = format!("{item}.omf");
    let level = format!("{dir}\\{name}");
    if tree.exists(&level) {
        return Ok(level);
    }
    let meshes = data.aliases.path("$game_meshes$", &name)?;
    if !tree.exists(&meshes) {
        return Err(invalid(format!("Can't find motion file '{item}'.")));
    }
    Ok(meshes)
}

/// The motion sets of the model file at `path`, in the order of its motion
/// files (or its embedded set), each with its motions decoded with `bones`
/// tracks (skeletal-motions 1, 3, 4.1).
pub fn read_motion_sets(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    path: &str,
    bones: usize,
) -> io::Result<Vec<(MotionParams, Vec<Motion>)>> {
    let mut file = tree.open(path).map_err(|e| context(path, e))?;
    let read = |file: &mut dyn ReadSeek, what: &str, params: MotionParams| {
        let count = params.definitions.len();
        let motions = skeletal_motions::read_motions(&mut &mut *file, count, bones, what)
            .map_err(|e| context(what, e))?;
        Ok((params, motions))
    };
    match skeletal_motions::read_model_motions(&mut file, path).map_err(|e| context(path, e))? {
        ModelMotions::Files(items) => items
            .iter()
            .map(|item| {
                let p = locate_motion_file(tree, data, dir, item)?;
                let mut f = tree.open(&p).map_err(|e| context(&p, e))?;
                let set =
                    skeletal_motions::read_motion_file(&mut f, &p).map_err(|e| context(&p, e))?;
                read(&mut f, &p, set.params)
            })
            .collect(),
        ModelMotions::Embedded(set) => Ok(vec![read(&mut file, path, set.params)?]),
        ModelMotions::None => Ok(Vec::new()),
    }
}

/// `Read + Seek` as one object.
trait ReadSeek: io::Read + io::Seek {}
impl<T: io::Read + io::Seek> ReadSeek for T {}

/// The motions of `model` (its names `names`), read as
/// [`read_motion_sets`] reads them, with its bones.
pub fn model_motions(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    model: &LoadedModel,
    names: &Names,
) -> io::Result<Motions> {
    let path = names.get(model.path);
    let n = model.bone_count();
    let sets = read_motion_sets(tree, data, dir, path, n)?;
    let bone_names: Vec<&str> = (0..n).map(|i| names.get(model.bone_name(i))).collect();
    let parents: Vec<Option<usize>> = (0..n).map(|i| model.bone_parent(i)).collect();
    let binds: Vec<Mat4> = (0..n).map(|i| model.bind(i)).collect();
    Ok(Motions::new(&bone_names, &parents, &binds, sets))
}

/// The objects' render matrices of a frame: every animated instance's,
/// one after another, and where each instance's start (`None`: at rest).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BonePoses {
    pub matrices: Vec<BoneMatrix>,
    pub first: Vec<Option<u32>>,
}

/// A trader's head callback (head-bone-turning.md 2): its model's head
/// bone and the trader's transform *X*.
#[derive(Clone, Copy, Debug)]
pub struct HeadCallback {
    pub bone: usize,
    pub transform: Mat4,
}

/// The playbacks of the level's object instances.
#[derive(Debug, Default)]
pub struct LevelMotions {
    /// By instance: its playbacks, once it has played a motion.
    animated: Vec<Option<AnimatedModel>>,
    /// By instance: the head callback attached to its head bone.
    head_callbacks: HashMap<usize, HeadCallback>,
    /// The level's controlled object's position (head-bone-turning.md
    /// 3.1), set before each update.
    pub controlled: Option<[f32; 3]>,
    /// The motions of each model (by its index) read so far.
    loaded: HashMap<usize, Arc<Motions>>,
    pose: Vec<Mat4>,
    render: Vec<Mat4>,
    /// The latest frame's matrices, shared with its scene.
    poses: Arc<BonePoses>,
    /// The latest frame's notifications, by instance (5.2).
    notified: Vec<(usize, Notified)>,
}

impl LevelMotions {
    pub fn new(instances: usize) -> Self {
        Self {
            animated: (0..instances).map(|_| None).collect(),
            poses: Arc::new(BonePoses {
                matrices: Vec::new(),
                first: vec![None; instances],
            }),
            ..Default::default()
        }
    }

    /// Instance `instance`'s playbacks, made with the motions of model
    /// `model` (read by `read` the first time a model plays).
    pub fn animate(
        &mut self,
        instance: usize,
        model: usize,
        read: impl FnOnce() -> io::Result<Motions>,
    ) -> io::Result<&mut AnimatedModel> {
        if self.animated[instance].is_none() {
            let motions = match self.loaded.get(&model) {
                Some(m) => m.clone(),
                None => {
                    let m = Arc::new(read()?);
                    self.loaded.insert(model, m.clone());
                    m
                }
            };
            self.animated[instance] = Some(AnimatedModel::new(motions));
        }
        Ok(self.animated[instance].as_mut().expect("just made"))
    }

    /// Attaches the head callback to instance `instance`'s head bone
    /// (head-bone-turning.md 2), replacing any it had.
    pub fn attach_head_callback(&mut self, instance: usize, callback: HeadCallback) {
        self.head_callbacks.insert(instance, callback);
    }

    /// Instance `instance`'s head callback, if one is attached.
    pub fn head_callback(&self, instance: usize) -> Option<&HeadCallback> {
        self.head_callbacks.get(&instance)
    }

    /// Instance `instance`'s playbacks, if it has played a motion.
    pub fn model(&self, instance: usize) -> Option<&AnimatedModel> {
        self.animated.get(instance)?.as_ref()
    }

    /// The frame's tracks updates (4.1) and the animated instances'
    /// render matrices.
    pub fn update(&mut self, clock: Clock) {
        if self.animated.iter().all(Option::is_none) {
            return;
        }
        let mut poses = BonePoses {
            matrices: Vec::new(),
            first: vec![None; self.animated.len()],
        };
        self.notified.clear();
        for (i, a) in self.animated.iter_mut().enumerate() {
            let Some(a) = a else { continue };
            a.update_tracks(clock);
            (self.notified).extend(a.take_notifications().into_iter().map(|n| (i, n)));
            // head-bone-turning.md 4, 5: the head bone turned by A each
            // time its matrix is computed, under script control too. The
            // level's player is its controlled object from its spawn on
            // (3.1), so `controlled` is always set on a level.
            let head = (self.head_callbacks.get(&i))
                .zip(self.controlled)
                .map(|(h, target)| (h.bone, turning_angle(target, &h.transform)));
            a.pose_following(&mut self.pose, |bone, t| {
                if let Some((head, angle)) = head
                    && bone == head
                {
                    turn(t, angle);
                }
            });
            a.render_matrices(&self.pose, &mut self.render);
            poses.first[i] = Some(poses.matrices.len() as u32);
            poses.matrices.extend(self.render.iter().map(bone_matrix));
        }
        self.poses = Arc::new(poses);
    }

    /// Instance `instance`'s playbacks to start cycles on, if it has played
    /// a motion.
    pub fn model_mut(&mut self, instance: usize) -> Option<&mut AnimatedModel> {
        self.animated.get_mut(instance)?.as_mut()
    }

    /// The notifications of the latest frame's tracks updates, by
    /// instance, in their order (5.2); those not taken are dropped at the
    /// next frame.
    pub fn take_notifications(&mut self) -> Vec<(usize, Notified)> {
        std::mem::take(&mut self.notified)
    }

    /// The latest frame's render matrices.
    pub fn poses(&self) -> Arc<BonePoses> {
        self.poses.clone()
    }
}
