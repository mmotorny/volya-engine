// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What is drawn of a shell (docs/behavior/physics-shells.md 7, 8, cited by
//! section): the shell's bone callbacks, which make the bones of its
//! elements follow their bodies (8.1), and the object's transform taken
//! from the root element (8.2). The bodies and their frames are the
//! solver's (`physics::ode::shell`); this is the arithmetic between them
//! and the model's bones.

use crate::physics::shell::{Shell, inverse_rigid};
use volya_engine_formats::model::{Mat4, multiply};

/// A shell's bone callbacks (8.1) and the object's transform it gives
/// (8.2).
#[derive(Clone, Debug, PartialEq)]
pub struct BoneCallbacks {
    /// By element: the bone that made it, which has the shell's bone
    /// callback; `None` for a shell not built from bones (type 0, 8.2).
    bones: Vec<Option<usize>>,
    /// The kept inverse of the root element's bone pose (its first call).
    root_inverse: Option<Mat4>,
}

impl BoneCallbacks {
    /// The callbacks of `shell`'s elements: the bone that made each one
    /// gets the shell's bone callback; bones that joined an element get
    /// none (8.1).
    pub fn of(shell: &Shell) -> Self {
        Self {
            bones: shell.elements.iter().map(|e| e.bone).collect(),
            root_inverse: None,
        }
    }

    /// Whether the shell was built from the bones (else: type 0, whose
    /// object's transform is its element's frame, 8.2).
    pub fn from_bones(&self) -> bool {
        self.bones.first().is_some_and(Option::is_some)
    }

    /// By element, the bone with its callback.
    pub fn bones(&self) -> &[Option<usize>] {
        &self.bones
    }

    /// The first call of each element's bone's callback after activation
    /// (8.1), with `pose` every bone's current pose (model space) and `x`
    /// the shell's transform: each element's frame becomes its bone's pose,
    /// and its body is re-placed there, followed by the shell's transform
    /// (the world frames returned, by element); the root element's inverse
    /// is kept. The bones' poses are left as computed.
    pub fn first_call(&mut self, pose: &[Mat4], x: &Mat4) -> Vec<(usize, Mat4)> {
        let mut placed = Vec::new();
        for (e, bone) in self.bones.iter().enumerate() {
            let Some(t) = bone.and_then(|b| pose.get(b)) else {
                continue;
            };
            if e == 0 {
                self.root_inverse = Some(inverse_rigid(t));
            }
            placed.push((e, multiply(t, x)));
        }
        placed
    }

    /// 8.2: the object's transform from the root element's world frame
    /// `root`: (the kept inverse of the root bone's pose) followed by it;
    /// for a type-0 shell the element's frame itself.
    pub fn object_transform(&self, root: &Mat4) -> Mat4 {
        match &self.root_inverse {
            Some(inverse) => multiply(inverse, root),
            None => *root,
        }
    }
}

/// Every later call of a bone's callback (8.1): its pose is the element's
/// world frame `frame` followed by the inverse of the shell's transform `x`.
pub fn bone_pose(frame: &Mat4, x: &Mat4) -> Mat4 {
    multiply(frame, &inverse_rigid(x))
}

/// The bones' poses (model space) of a model without motions, `binds`
/// their bind poses and `parents` their parents, under bone callbacks:
/// a bone `follow` gives a pose takes it; every other bone is computed from
/// its parent's pose and its own bind pose (7, 8.1: **believed**), into
/// `out`.
pub fn rigid_pose(
    binds: &[Mat4],
    parents: &[Option<usize>],
    follow: impl Fn(usize) -> Option<Mat4>,
    out: &mut Vec<Mat4>,
) {
    let n = binds.len();
    out.clear();
    out.resize(
        n,
        binds
            .first()
            .copied()
            .unwrap_or(crate::physics::shell::IDENTITY),
    );
    let mut done = vec![false; n];
    for start in 0..n {
        // The chain up to a bone already computed, then back down.
        let mut chain = vec![start];
        while let Some(p) = parents
            .get(*chain.last().expect("not empty"))
            .copied()
            .flatten()
        {
            if p >= n || done[p] || chain.len() > n {
                break;
            }
            chain.push(p);
        }
        for &b in chain.iter().rev() {
            if done[b] {
                continue;
            }
            out[b] = match follow(b) {
                Some(t) => t,
                None => match parents[b].filter(|&p| p < n && done[p]) {
                    // T = L · T(parent), L = B · B(parent)⁻¹.
                    Some(p) => {
                        let local = multiply(&binds[b], &inverse_rigid(&binds[p]));
                        multiply(&local, &out[p])
                    }
                    None => binds[b],
                },
            };
            done[b] = true;
        }
    }
}

#[cfg(test)]
mod tests;
