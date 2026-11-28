// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's spawned objects drawn at rest on New Game: which records of
//! `spawns\all.spawn` are online (docs/behavior/offline-simulation.md 6.5,
//! the offline simulation's switch pass, [`crate::core::simulation`]), each
//! one's visual and transform (docs/behavior/object-visual-and-transform.md,
//! cited as `OV`), its model file (docs/formats/skeleton-ogf.md,
//! [`volya_engine_formats::model`]) drawn at its stored positions quantized
//! as the first renderer stores them (docs/behavior/model-shading.md 1, 2),
//! and the hanging lamps' lights as structure
//! (docs/behavior/hanging-lamps.md).
//!
//! Loading runs in the level's loading stages ([`crate::level::NewGameLoad`]):
//! the switch pass, the models (taken from the game type's prefetched ones,
//! [`Prefetched`], where they hold the file), their surfaces, the light
//! inputs. Each model file is read once and shared by every object showing
//! it; the meshes of the models not prefetched are placed in chunks of the
//! level's own geometry as they are laid out, each kept by the platform's
//! renderer once full (or by the presenter with the world); the prefetched
//! ones are drawn from the geometry the renderer keeps for the game type.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::core::collision::CollisionWorld;
use crate::core::object_light;
use crate::core::simulation::{NO_ID, Simulation};
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use crate::render::world::{DetailWindow, LEVEL_CHUNK, ObjectGeometry, skinned_vertex};
use volya_engine_formats::bone_physics::UsedShape;
use volya_engine_formats::lanims;
use volya_engine_formats::model::{self, Mat4, Model, transform, transform_point};
use volya_engine_formats::object::{HangingLamp, Kind, Object, ObjectRecord, SpawnHeader};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// An object of the current level whose visual is loaded with the level:
/// one online at the start (offline-simulation.md 9.1) or one that may come online later
/// (offline-simulation.md 6.2).
#[derive(Clone, Debug)]
pub struct LevelObject {
    /// Its simulation id.
    pub id: u16,
    /// Object index in the spawn file (its id where it has none).
    pub index: u32,
    /// Online at the start.
    pub online: bool,
    pub record: ObjectRecord,
    /// The visual it shows when not its record's: a carried item's section
    /// `visual` (docs/behavior/character-items.md 8.5).
    pub visual: Option<String>,
}

/// The objects of `simulation`'s current level whose visuals the level
/// loads: those of its list without a parent that are online, or for which
/// the engine's *on* can hold (offline-simulation.md 6.5.2). The others
/// never come online here: the scripts' replacements only make *on* false
/// where the engine's is (offline-simulation.md 6.5.4; a job-online value
/// set true by a smart terrain's script would not, but no script runs yet).
/// Objects inside a container are left out (our choice: none on the level
/// has one, offline-simulation.md 9.1).
pub fn level_objects(simulation: &Simulation) -> Vec<LevelObject> {
    simulation
        .level_list()
        .filter_map(|id| simulation.object(id))
        .filter(|o| o.record.header.parent_id == NO_ID && (o.online || o.may_switch_online()))
        .map(|o| LevelObject {
            id: o.id,
            index: o.spawn_index.unwrap_or(u32::from(o.id)),
            online: o.online,
            record: o.record.clone(),
            visual: None,
        })
        .collect()
}

/// The visual name an online object shows (`OV` 2): the record's
/// Visual block, which a stalker's or trader's specific character has
/// already set (docs/behavior/character-identity.md 3 step 2, 4.2): a
/// stalker shows its supplies choice's `visual` (`OV` 2 item 3);
/// `None` without a model (`OV` 3: no Visual block, an
/// empty name, or the actor, whose body the first-person view does not
/// draw). The configuration's `visual` key is overridden by the Visual
/// block in every kind that has one, so it is not read. A dead body's
/// `corpse_visual` (`OV` 2, item 4) is not handled: it applies only to
/// a body with a non-zero death time, and every dead body of the shipped
/// file has death time 0 (our omission). [`LevelObject::visual`], when
/// set, is shown instead.
pub fn visual_name(object: &LevelObject) -> Option<String> {
    if let Some(v) = &object.visual {
        return Some(v.clone());
    }
    let record = &object.record;
    if record.kind() == Kind::Player {
        return None;
    }
    let name = &record.object.visual()?.model;
    (!name.is_empty()).then(|| name.to_string())
}

/// The file of visual name `name` (`OV` 1): `name` as given, then
/// `$level$` + n′, then `$game_meshes$` + n′, n′ = `name` with `.ogf`
/// appended if it has no `.`. Not found is the game's fatal error. Without
/// a level (`dir` empty: the prefetching, before any level is known) the
/// `$level$` candidate is left out.
pub fn locate_model(tree: &dyn Vfs, data: &GameData, dir: &str, name: &str) -> io::Result<String> {
    let full = if name.contains('.') {
        name.to_owned()
    } else {
        format!("{name}.ogf")
    };
    let candidates = [
        Some(Ok(name.to_owned())),
        (!dir.is_empty()).then(|| Ok(format!("{dir}\\{full}"))),
        Some(data.aliases.path("$game_meshes$", &full)),
    ];
    for c in candidates.into_iter().flatten() {
        let c = c?;
        if tree.exists(&c) {
            return Ok(c);
        }
    }
    Err(invalid(format!("Can't find model file '{name}'.")))
}

/// A name in its owner's [`Names`]: a byte range of its buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Name {
    at: u32,
    len: u32,
}

/// An owner's names, each once, in one buffer (docs/architecture/memory.md,
/// rule 3): its models' paths, their meshes' textures and shaders and their
/// bones' names, which the models refer to by [`Name`].
#[derive(Debug, Default)]
pub struct Names {
    text: String,
    /// While names are added: each name by its text's hash, and those
    /// whose hash an earlier name has.
    by_hash: HashMap<u64, Name>,
    collided: Vec<Name>,
}

impl Names {
    /// `name`'s text.
    pub fn get(&self, name: Name) -> &str {
        &self.text[name.at as usize..(name.at + name.len) as usize]
    }

    /// The name of `text`, added unless an earlier one has it.
    pub fn add(&mut self, text: &str) -> Name {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();
        let first = self.by_hash.get(&hash).copied();
        if let Some(n) = first {
            if self.get(n) == text {
                return n;
            }
            if let Some(&n) = self.collided.iter().find(|&&n| self.get(n) == text) {
                return n;
            }
        }
        let name = Name {
            at: self.text.len() as u32,
            len: text.len() as u32,
        };
        self.text.push_str(text);
        if first.is_some() {
            self.collided.push(name);
        } else {
            self.by_hash.insert(hash, name);
        }
        name
    }

    /// Ends the adding: the lookup goes, the buffer is shrunk to its names.
    fn seal(&mut self) {
        self.by_hash = HashMap::new();
        self.collided = Vec::new();
        self.text.shrink_to_fit();
    }
}

/// A child mesh of a loaded model: its shader pair and its range in the
/// objects' geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshRange {
    /// In its owner's [`Names`].
    pub texture: Name,
    pub shader: Name,
    /// First vertex in [`ObjectGeometry`], and the indices every detail
    /// window draws from `first_index` (object-detail-level.md 4).
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub first_index: u32,
    pub index_count: u32,
    /// Its detail windows, `[first, count]` of [`ObjectGeometry::windows`]
    /// (count 0: a skinned mesh, drawn whole).
    pub windows: [u32; 2],
    /// The mesh's header sphere in model space: centre, radius
    /// (object-detail-level.md 3).
    pub sphere: [f32; 4],
    /// The chunk of the kept geometry holding it, a prefetched model's
    /// ([`Prefetched`]): its first vertex and index are then the chunk's,
    /// its indices counted from the chunk's first vertex.
    pub kept: Option<u32>,
}

/// A bone of a loaded model: its name and its bind matrix's first three
/// columns, the fourth (0, 0, 0, 1).
#[derive(Clone, Copy, Debug)]
struct Bone {
    name: Name,
    bind: [[f32; 3]; 4],
}

/// A model file loaded for drawing: its header and meshes, the vertices
/// already in the objects' geometry; its names in its owner's [`Names`].
#[derive(Clone, Debug)]
pub struct LoadedModel {
    pub path: Name,
    pub header: model::Header,
    pub meshes: Box<[MeshRange]>,
    /// Bone names and their bind matrices, for the lamps' lights and the
    /// artefacts.
    bones: Box<[Bone]>,
    /// Every bone's bind matrix whole when one's fourth column is not
    /// exactly (0, 0, 0, 1); empty otherwise.
    binds: Box<[Mat4]>,
    /// Each bone's parent (skeleton-ogf.md 4); empty when not given.
    parents: Box<[Option<u16>]>,
    /// Each bone's used shape in its own bind space (skeleton-ogf.md 4,
    /// bone-physics.md 2.2), for the crosshair's pick; empty when not
    /// given.
    shapes: Box<[Option<UsedShape>]>,
}

impl LoadedModel {
    /// The model with `bones`, each a name and its bind matrix.
    pub fn new(
        path: Name,
        header: model::Header,
        meshes: Box<[MeshRange]>,
        bones: &[(Name, Mat4)],
    ) -> Self {
        let affine = |m: &Mat4| {
            (0..3).all(|r| m[r][3].to_bits() == 0.0f32.to_bits())
                && m[3][3].to_bits() == 1.0f32.to_bits()
        };
        let binds = if bones.iter().all(|(_, m)| affine(m)) {
            Box::default()
        } else {
            bones.iter().map(|(_, m)| *m).collect()
        };
        Self {
            path,
            header,
            meshes,
            bones: (bones.iter())
                .map(|&(name, m)| Bone {
                    name,
                    bind: m.map(|r| [r[0], r[1], r[2]]),
                })
                .collect(),
            binds,
            parents: Box::default(),
            shapes: Box::default(),
        }
    }

    /// The model with its bones' used shapes.
    pub fn with_shapes(mut self, shapes: Box<[Option<UsedShape>]>) -> Self {
        self.shapes = shapes;
        self
    }

    /// Bone `i`'s used shape, if it has one (and the shapes were given).
    pub fn bone_shape(&self, i: usize) -> Option<&UsedShape> {
        self.shapes.get(i)?.as_ref()
    }

    /// The model with its bones' parents.
    pub fn with_parents(mut self, parents: Box<[Option<u16>]>) -> Self {
        self.parents = parents;
        self
    }

    /// Bone `i`'s parent, if it has one (and the parents were given).
    pub fn bone_parent(&self, i: usize) -> Option<usize> {
        self.parents.get(i).copied().flatten().map(usize::from)
    }

    /// How many bones it has.
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    /// Bone `i`'s name.
    pub fn bone_name(&self, i: usize) -> Name {
        self.bones[i].name
    }

    /// Bone `i`'s bind matrix.
    pub fn bind(&self, i: usize) -> Mat4 {
        match self.binds.get(i) {
            Some(m) => *m,
            None => {
                let b = &self.bones[i].bind;
                std::array::from_fn(|r| [b[r][0], b[r][1], b[r][2], if r == 3 { 1.0 } else { 0.0 }])
            }
        }
    }

    /// The bind matrix of its bone named `name` (`names` its owner's), if
    /// it has one.
    pub fn bone(&self, names: &Names, name: &str) -> Option<Mat4> {
        let i = self.bones.iter().position(|b| names.get(b.name) == name)?;
        Some(self.bind(i))
    }

    /// Its copy for another owner as the file at `path`, its names added
    /// to that owner's `to` from its own `from`.
    fn renamed(&self, path: &str, from: &Names, to: &mut Names) -> Self {
        let mut model = self.clone();
        model.path = to.add(path);
        for mesh in &mut model.meshes {
            mesh.texture = to.add(from.get(mesh.texture));
            mesh.shader = to.add(from.get(mesh.shader));
        }
        for bone in &mut model.bones {
            bone.name = to.add(from.get(bone.name));
        }
        model
    }
}

/// Appends `model`'s vertices, quantized (model-shading.md 1), and its meshes'
/// indices and detail windows to `geometry`. Every mesh is quantized whatever its
/// render mode: the software mode keeps floats, but needs a bone index
/// above 78 with 256 vertex shader constants, which no shipped model has
/// (our choice: the constant count of a GPU device has no meaning here).
/// Its names are added to `names`, its owner's.
pub fn add_model(
    path: &str,
    model: &Model,
    geometry: &mut ObjectGeometry,
    names: &mut Names,
) -> io::Result<LoadedModel> {
    let pose = model.bind_pose()?;
    let mut meshes = Vec::with_capacity(model.meshes.len());
    for m in &model.meshes {
        let first_vertex =
            (geometry.vertices.len() / crate::render::world::MODEL_VERTEX_BYTES) as u32;
        for v in &m.vertices {
            // model-shading.md 1, 2: the bones and the quantized weight,
            // which a posed object's draw skins by.
            geometry.vertices.extend(skinned_vertex(
                v.position.map(model::quantize_position),
                model::quantize_normal(v.normal),
                v.uv.map(model::quantize_uv),
                v.bones,
                model::quantize_weight(v.weight),
            ));
        }
        // Every detail window's indices; the renderer draws the window of
        // each frame's detail value (object-detail-level.md 4).
        let span = m.detail_span();
        let indices = &m.indices[span.clone()];
        let first_index = geometry.indices.len() as u32;
        geometry.indices.extend_from_slice(indices);
        let first_window = geometry.windows.len() as u32;
        if let Some(w) = &m.windows {
            geometry.windows.extend(
                w.steps.iter().map(|s| {
                    DetailWindow::new(s.index_offset - span.start as u32, s.triangle_count)
                }),
            );
        }
        let windows = [first_window, geometry.windows.len() as u32 - first_window];
        meshes.push(MeshRange {
            texture: names.add(&m.texture),
            shader: names.add(&m.shader),
            first_vertex,
            vertex_count: m.vertices.len() as u32,
            first_index,
            index_count: indices.len() as u32,
            windows,
            sphere: {
                let (c, r) = (m.header.sphere_centre, m.header.sphere_radius);
                [c[0], c[1], c[2], r]
            },
            kept: None,
        });
    }
    let bones: Vec<(Name, Mat4)> = (model.bones.iter().zip(pose))
        .map(|(b, m)| (names.add(&b.name), m))
        .collect();
    let parents = (model.bones.iter())
        .map(|b| b.parent.map(|p| p as u16))
        .collect();
    let shapes = (model.bones.iter())
        .map(|b| b.physics.shape.used())
        .collect();
    Ok(
        LoadedModel::new(names.add(path), model.header, meshes.into(), &bones)
            .with_parents(parents)
            .with_shapes(shapes),
    )
}

/// The section listing the models prefetched for the single-player game
/// type (console-diagnostics.md 2.4.1: `prefetch_visuals_single`, its keys
/// the visual names).
pub const PREFETCH_VISUALS: &str = "prefetch_visuals_single";

/// The models loaded under `st_prefetching_objects` and kept while the game
/// type stays (console-diagnostics.md 2.4.1; docs/architecture/memory.md,
/// Lifetimes): each listed visual's file read once, parsed and laid out as
/// [`add_model`] lays it out, its meshes' vertices and indices then placed
/// in chunks of the kept geometry, which the renderer keeps
/// (`Renderer::keep_geometry`), and its detail windows in a block of its
/// own. A level's objects take their models from here ([`spawn`]) and draw
/// them from the kept chunks; their surfaces' texture files are listed
/// beside, for the platform to load with them.
#[derive(Debug, Default)]
pub struct Prefetched {
    /// Which set of kept geometry its chunks are ([`GeometryChunk::set`]).
    set: u64,
    /// Its models' names.
    names: Names,
    /// Each model's located path, lower-cased, and its index, in the
    /// paths' order.
    by_path: Box<[(Name, u32)]>,
    models: Vec<PrefetchedModel>,
    /// The texture files of the models' surfaces, each once.
    pub textures: Vec<String>,
    /// Its chunks, and the bytes of their vertices and indices.
    chunks: u32,
    vertices: u64,
    indices: u64,
    /// The chunks made without a platform to keep them, until a presenter
    /// keeps them ([`Prefetched::take_pending`]).
    pending: Mutex<Vec<GeometryChunk>>,
}

/// A prefetched model: its meshes' ranges in the kept geometry's chunks,
/// their detail windows in a block of their own.
#[derive(Debug)]
struct PrefetchedModel {
    model: LoadedModel,
    windows: Box<[DetailWindow]>,
}

/// Most model vertices a chunk of the kept geometry holds: its 16-bit
/// indices count from its first vertex (WebGL 2 has no base vertex).
pub const CHUNK_VERTICES: usize = 1 << 16;

/// A chunk of the prefetched models' geometry, or of a level's own models'
/// (its index with [`LEVEL_CHUNK`]), for the renderer to keep: whole
/// meshes' model vertices, and their indices, 16-bit little-endian, each
/// counted from the chunk's first vertex (padded to whole words).
#[derive(Debug, Default)]
pub struct GeometryChunk {
    /// The [`Prefetched`] or [`Spawned`] whose chunk it is
    /// ([`Prefetched::set`], [`Spawned::set`]).
    pub set: u64,
    pub index: u32,
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
}

impl GeometryChunk {
    fn vertex_count(&self) -> usize {
        self.vertices.len() / crate::render::world::MODEL_VERTEX_BYTES
    }
}

/// The making of a [`Prefetched`] a step at a time ([`Prefetching::step`]),
/// so a platform keeps each chunk as it is made and the CPU holds no more
/// than one.
#[derive(Debug)]
pub struct Prefetching {
    /// The visual names to load, in the section's key order.
    visuals: Vec<String>,
    next: usize,
    made: Prefetched,
    /// By located path, lower-cased: the model's index.
    by_path: HashMap<String, usize>,
    chunks: Chunks,
    /// One model's layout at a time.
    scratch: ObjectGeometry,
}

/// Meshes placed in chunks of kept geometry as they are laid out, one
/// chunk open at a time, each handed on once full.
#[derive(Debug)]
struct Chunks {
    /// The chunk being filled.
    open: GeometryChunk,
    /// The chunks handed on, and the bytes of their vertices and indices.
    count: u32,
    vertices: u64,
    indices: u64,
}

impl Chunks {
    /// Chunks of set `set`, numbered from `first`.
    fn new(set: u64, first: u32) -> Self {
        Self {
            open: GeometryChunk {
                set,
                index: first,
                ..Default::default()
            },
            count: 0,
            vertices: 0,
            indices: 0,
        }
    }

    /// Places `mesh` of the file at `path`, laid out in `scratch` by
    /// [`add_model`], in the open chunk, first handing that to `keep` if
    /// the mesh doesn't fit beside its vertices; whether it handed one.
    fn place(
        &mut self,
        path: &str,
        mesh: &mut MeshRange,
        scratch: &ObjectGeometry,
        keep: &mut dyn FnMut(GeometryChunk),
    ) -> io::Result<bool> {
        let bytes = crate::render::world::MODEL_VERTEX_BYTES;
        let count = mesh.vertex_count as usize;
        let mut kept = false;
        if self.open.vertex_count() + count > CHUNK_VERTICES && !self.open.vertices.is_empty() {
            self.close(keep);
            kept = true;
        }
        let base = self.open.vertex_count();
        let first = mesh.first_vertex as usize * bytes;
        let vertices = &scratch.vertices[first..first + count * bytes];
        self.open.vertices.extend_from_slice(vertices);
        let first_index = self.open.indices.len() / 2;
        let from = mesh.first_index as usize;
        let indices = &scratch.indices[from..from + mesh.index_count as usize];
        for &i in indices {
            let i = u16::try_from(usize::from(i) + base)
                .map_err(|_| invalid(format!("{path}: an index past 16 bits")))?;
            self.open.indices.extend_from_slice(&i.to_le_bytes());
        }
        mesh.kept = Some(self.open.index);
        mesh.first_vertex = base as u32;
        mesh.first_index = first_index as u32;
        Ok(kept)
    }

    /// Hands the chunk being filled to `keep` and begins the next.
    fn close(&mut self, keep: &mut dyn FnMut(GeometryChunk)) {
        let next = GeometryChunk {
            set: self.open.set,
            index: self.open.index + 1,
            ..Default::default()
        };
        let mut chunk = std::mem::replace(&mut self.open, next);
        // Whole words.
        if !chunk.indices.len().is_multiple_of(4) {
            chunk.indices.extend_from_slice(&[0, 0]);
        }
        self.count += 1;
        self.vertices += chunk.vertices.len() as u64;
        self.indices += chunk.indices.len() as u64;
        keep(chunk);
    }

    /// Hands the chunk being filled to `keep`, if it holds any mesh.
    fn finish(&mut self, keep: &mut dyn FnMut(GeometryChunk)) {
        if !self.open.vertices.is_empty() {
            self.close(keep);
        }
    }
}

/// The sets of kept geometry made so far.
static SETS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Prefetching {
    /// Begins loading the models of the system configuration's section
    /// [`PREFETCH_VISUALS`].
    pub fn new(data: &GameData) -> Self {
        let set = SETS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let visuals = (data.system.section(PREFETCH_VISUALS).into_iter())
            .flat_map(|s| s.entries().map(|e| e.key().to_owned()))
            .collect();
        Self {
            visuals,
            next: 0,
            by_path: HashMap::new(),
            made: Prefetched {
                set,
                ..Default::default()
            },
            chunks: Chunks::new(set, 0),
            scratch: ObjectGeometry::default(),
        }
    }

    /// Loads the next models, in the section's key order, each file once
    /// (names locating the same file share it), until a chunk is full and
    /// handed to `keep`, or every model is loaded and the last chunk
    /// handed; returns whether every model is. A name that locates no file,
    /// or a file that does not parse, fails it, as an object's model fails
    /// the level's load. Each file is held in the file cache while it is
    /// read (docs/architecture/memory.md, File-backed data).
    pub fn step(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        keep: &mut dyn FnMut(GeometryChunk),
    ) -> io::Result<bool> {
        let mut kept = false;
        while !kept && let Some(name) = self.visuals.get(self.next) {
            self.next += 1;
            let path = locate_model(tree, data, "", name)?;
            let key = path.to_lowercase();
            if self.by_path.contains_key(&key) {
                continue;
            }
            let mut model = {
                let file = crate::core::file_cache::read(tree, &path)?;
                let parsed = Model::parse(&file).map_err(|e| invalid(format!("{path}: {e}")))?;
                self.scratch.vertices.clear();
                self.scratch.indices.clear();
                self.scratch.windows.clear();
                add_model(&path, &parsed, &mut self.scratch, &mut self.made.names)?
            };
            for mesh in &mut model.meshes {
                kept |= self.chunks.place(&path, mesh, &self.scratch, keep)?;
            }
            self.made.models.push(PrefetchedModel {
                model,
                windows: self.scratch.windows.as_slice().into(),
            });
            self.by_path.insert(key, self.made.models.len() - 1);
        }
        if self.next < self.visuals.len() {
            return Ok(false);
        }
        self.chunks.finish(keep);
        self.scratch = ObjectGeometry::default();
        Ok(true)
    }

    /// The models loaded, once [`Prefetching::step`] has loaded every one.
    pub fn finish(self) -> Prefetched {
        let mut made = self.made;
        let c = &self.chunks;
        (made.chunks, made.vertices, made.indices) = (c.count, c.vertices, c.indices);
        let mut by_path: Vec<(String, u32)> = (self.by_path.into_iter())
            .map(|(path, i)| (path, i as u32))
            .collect();
        by_path.sort_unstable();
        made.by_path = (by_path.iter())
            .map(|(path, i)| (made.names.add(path), *i))
            .collect();
        made.names.seal();
        made
    }
}

impl Prefetched {
    /// Loads every model of [`PREFETCH_VISUALS`] at once
    /// ([`Prefetching::step`]), handing its chunks to `keep`.
    pub fn load(
        tree: &dyn Vfs,
        data: &GameData,
        keep: &mut dyn FnMut(GeometryChunk),
    ) -> io::Result<Self> {
        let mut p = Prefetching::new(data);
        while !p.step(tree, data, keep)? {}
        Ok(p.finish())
    }

    /// [`Prefetched::load`] keeping its chunks with it until a presenter
    /// keeps them ([`Prefetched::take_pending`]): without a platform that
    /// keeps them as they are made.
    pub fn load_pending(tree: &dyn Vfs, data: &GameData) -> io::Result<Self> {
        let mut pending = Vec::new();
        let p = Self::load(tree, data, &mut |mut c| {
            c.vertices.shrink_to_fit();
            c.indices.shrink_to_fit();
            pending.push(c);
        })?;
        *p.pending.lock().unwrap_or_else(|e| e.into_inner()) = pending;
        Ok(p)
    }

    /// Which set of kept geometry its chunks are.
    pub fn set(&self) -> u64 {
        self.set
    }

    /// Its chunks, and the bytes of their vertices and of their indices.
    pub fn geometry(&self) -> (u32, u64, u64) {
        (self.chunks, self.vertices, self.indices)
    }

    /// The chunks not yet kept, once: whoever takes them keeps them.
    pub fn take_pending(&self) -> Vec<GeometryChunk> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// How many models it holds.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// The index of the model of the file at `path`, if it holds it.
    fn index(&self, path: &str) -> Option<usize> {
        let key = path.to_lowercase();
        let at = (self.by_path)
            .binary_search_by(|&(n, _)| self.names.get(n).cmp(&key))
            .ok()?;
        Some(self.by_path[at].1 as usize)
    }

    /// Whether it holds the model of the file at `path`.
    pub fn holds(&self, path: &str) -> bool {
        self.index(path).is_some()
    }

    /// Every model, as loaded (its ranges in the kept chunks), its names
    /// in [`Prefetched::names`].
    pub fn models(&self) -> impl Iterator<Item = &LoadedModel> {
        self.models.iter().map(|m| &m.model)
    }

    /// Its models' names.
    pub fn names(&self) -> &Names {
        &self.names
    }

    /// The model of the file at `path`, if prefetched, its meshes drawn
    /// from the kept chunks, its detail windows appended to `geometry`'s
    /// and its names added to `names`, its new owner's.
    pub fn append(
        &self,
        path: &str,
        geometry: &mut ObjectGeometry,
        names: &mut Names,
    ) -> Option<LoadedModel> {
        let m = &self.models[self.index(path)?];
        let window = geometry.windows.len() as u32;
        geometry.windows.extend_from_slice(&m.windows);
        let mut model = m.model.renamed(path, &self.names, names);
        for mesh in &mut model.meshes {
            mesh.windows[0] += window;
        }
        Some(model)
    }
}

/// An object drawn with a model.
#[derive(Clone, Debug)]
pub struct Instance {
    /// Its simulation id.
    pub id: u16,
    /// Object index in the spawn file.
    pub index: u32,
    /// Online at the start; offline ones are loaded but not drawn.
    pub online: bool,
    pub name: String,
    pub class_id: String,
    pub kind: Kind,
    /// The visual name it shows.
    pub visual: String,
    pub model: usize,
    /// Model space to world (`OV` 4).
    pub transform: Mat4,
    /// The model's header sphere in world space: centre, radius.
    pub sphere: [f32; 4],
}

impl Instance {
    pub fn new(object: &LevelObject, visual: String, model: usize, loaded: &LoadedModel) -> Self {
        let h = &object.record.header;
        let mut instance = Self {
            id: object.id,
            index: object.index,
            online: object.online,
            name: h.name.to_string(),
            class_id: object.record.class_id.as_str().to_owned(),
            kind: object.record.kind(),
            visual,
            model,
            transform: Mat4::default(),
            sphere: [0.0; 4],
        };
        instance.place(h, loaded);
        instance
    }

    /// Places it at `header`'s position and angles (`OV` 4).
    pub fn place(&mut self, header: &SpawnHeader, loaded: &LoadedModel) {
        self.transform = transform(header.position, header.angles);
        let c = transform_point(&self.transform, loaded.header.sphere_centre);
        self.sphere = [c[0], c[1], c[2], loaded.header.sphere_radius];
    }
}

/// The kind of a lamp's light (hanging-lamps.md 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LightKind {
    Point,
    Spot,
}

/// A light as hanging-lamps.md 3.1 sets it up; at rest, its placement from the bind
/// pose.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Light {
    pub kind: LightKind,
    pub shadows: bool,
    /// Linear RGB, already times the brightness (or the ambient power).
    pub colour: [f32; 3],
    pub range: f32,
    /// Cone angle, radians (spot lights).
    pub cone: f32,
    pub texture: String,
    /// An entry of `lanims.xr` (docs/behavior/hanging-lamps.md 4.3).
    pub colour_animator: String,
    /// Translation row of the placement matrix (believed).
    pub position: [f32; 3],
    /// Third row.
    pub direction: [f32; 3],
    /// First row.
    pub right: [f32; 3],
}

/// A glow at the light's position (hanging-lamps.md 3.3; its drawing
/// is glows.md's).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Glow {
    pub texture: String,
    pub radius: f32,
    pub position: [f32; 3],
}

/// The lights of a hanging lamp (docs/behavior/hanging-lamps.md 2,
/// 3), at rest.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LampLights {
    pub name: String,
    /// Its simulation id.
    pub id: u16,
    /// Online at the start.
    pub online: bool,
    /// The record's health.
    pub health: f32,
    pub brightness: f32,
    pub ambient_power: f32,
    /// The lamp's health is above 0.
    pub on: bool,
    /// Light flags bit 0x01.
    pub physics_shell: bool,
    /// The lamp has a model.
    pub has_model: bool,
    pub main: Light,
    /// Bit 0x20.
    pub ambient: Option<Light>,
    pub glow: Option<Glow>,
    /// The main light's matrix M = B(light bone) · X, and the ambient
    /// light's M′ (hanging-lamps.md 4.2), at rest.
    #[serde(skip)]
    pub main_matrix: Mat4,
    #[serde(skip)]
    pub ambient_matrix: Mat4,
}

/// Light flags (hanging-lamps.md 2.2).
const LAMP_PHYSICS: u16 = 0x01;
const LAMP_SHADOWS: u16 = 0x02;
const LAMP_SPOT: u16 = 0x10;
const LAMP_AMBIENT: u16 = 0x20;

fn row(m: &Mat4, r: usize) -> [f32; 3] {
    [m[r][0], m[r][1], m[r][2]]
}

/// The lights of `lamp` placed at transform `x` with its model's bones
/// (`None` without a visual; its names in `names`): each light's matrix is
/// its bone's bind matrix · X, or X when the bone is not one of the
/// visual's.
pub fn lamp_lights(
    object: (&str, u16, bool),
    lamp: &HangingLamp,
    x: &Mat4,
    model: Option<&LoadedModel>,
    names: &Names,
) -> LampLights {
    let flags = lamp.light_flags;
    let place = |bone: &str| -> Mat4 {
        match model.and_then(|m| m.bone(names, bone)) {
            Some(b) => model::multiply(&b, x),
            None => *x,
        }
    };
    // hanging-lamps.md 3.2: (v × K) × brightness, each product in f32.
    let c = lamp.colour;
    let rgb = [(c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF].map(|v| v as f32 * lanims::K);
    let main_m = place(&lamp.light_bone);
    let main = Light {
        kind: if flags & LAMP_SPOT != 0 {
            LightKind::Spot
        } else {
            LightKind::Point
        },
        shadows: flags & LAMP_SHADOWS != 0,
        colour: rgb.map(|v| v * lamp.brightness),
        range: lamp.range,
        cone: lamp.spot_cone_angle,
        texture: lamp.light_texture.to_string(),
        colour_animator: lamp.colour_animator.to_string(),
        position: row(&main_m, 3),
        direction: row(&main_m, 2),
        right: row(&main_m, 0),
    };
    // hanging-lamps.md 3.4, 4.2, 10: an empty ambient bone is no bone, so
    // the ambient light is placed at X (every record is of version 118;
    // the light bone stands in only in older records).
    let ambient_m = place(&lamp.ambient_light_bone);
    let ambient = (flags & LAMP_AMBIENT != 0).then(|| {
        let m = ambient_m;
        Light {
            kind: LightKind::Point,
            shadows: false,
            colour: main.colour.map(|v| v * lamp.ambient_power),
            range: lamp.ambient_radius,
            cone: 0.0,
            texture: lamp.ambient_texture.to_string(),
            colour_animator: String::new(),
            position: row(&m, 3),
            direction: row(&m, 2),
            right: row(&m, 0),
        }
    });
    let glow = (!lamp.glow_texture.is_empty()).then(|| Glow {
        texture: lamp.glow_texture.to_string(),
        radius: lamp.glow_radius,
        position: main.position,
    });
    LampLights {
        name: object.0.to_owned(),
        id: object.1,
        online: object.2,
        health: lamp.health,
        brightness: lamp.brightness,
        ambient_power: lamp.ambient_power,
        main_matrix: main_m,
        ambient_matrix: ambient_m,
        on: lamp.health > 0.0,
        physics_shell: flags & LAMP_PHYSICS != 0,
        has_model: model.is_some(),
        main,
        ambient,
        glow,
    }
}

/// The objects spawned on the level, once their models are loaded.
#[derive(Debug, Default)]
pub struct Spawned {
    /// Online objects at the start (offline-simulation.md 9.1).
    pub online: usize,
    /// Online objects per class id.
    pub online_by_class: Vec<(String, usize)>,
    pub models: Vec<LoadedModel>,
    pub instances: Vec<Instance>,
    pub lamps: Vec<LampLights>,
    /// The detail windows, and the meshes too large for a chunk.
    pub geometry: ObjectGeometry,
    /// Which set of kept geometry the chunks of the models not prefetched
    /// are ([`GeometryChunk::set`]; their indices with [`LEVEL_CHUNK`]).
    pub set: u64,
    /// Those chunks no platform kept as they were made, for a presenter to
    /// keep.
    pub chunks: Vec<GeometryChunk>,
    /// The models' names.
    pub names: Names,
    /// The prefetched models the models taken from them are drawn from.
    pub prefetched: Option<Arc<Prefetched>>,
}

/// Loads the visuals of `objects` (sections 3, 4): each model file once,
/// taken from `prefetched` where it holds the file. The others' meshes are
/// placed in chunks of the level's kept geometry as they are laid out,
/// each handed to `keep` once full, so the CPU holds one at a time
/// (docs/architecture/memory.md, #788); a mesh of more vertices than a
/// chunk holds stays in [`Spawned::geometry`].
pub fn spawn(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    objects: &[LevelObject],
    prefetched: Option<&Arc<Prefetched>>,
    keep: &mut dyn FnMut(GeometryChunk),
) -> io::Result<Spawned> {
    let set = SETS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let mut chunks = Chunks::new(set, LEVEL_CHUNK);
    let mut scratch = ObjectGeometry::default();
    // Models by file path, and by visual name (located once each).
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut s = Spawned {
        online: objects.iter().filter(|o| o.online).count(),
        set,
        ..Default::default()
    };
    let mut classes: HashMap<&str, usize> = HashMap::new();
    for o in objects {
        if o.online {
            *classes.entry(o.record.class_id.as_str()).or_default() += 1;
        }
        let name = visual_name(o);
        let model = match &name {
            Some(name) if by_name.contains_key(&name.to_lowercase()) => {
                Some(by_name[&name.to_lowercase()])
            }
            Some(name) => {
                let path = locate_model(tree, data, dir, name)?;
                let key = path.to_lowercase();
                let index = match by_path.get(&key) {
                    Some(&i) => i,
                    None => {
                        let taken =
                            prefetched.and_then(|p| p.append(&path, &mut s.geometry, &mut s.names));
                        let model = match taken {
                            Some(model) => {
                                s.prefetched = prefetched.cloned();
                                model
                            }
                            None => {
                                let bytes = vfs::read(tree, &path)?;
                                let parsed = Model::parse(&bytes)
                                    .map_err(|e| invalid(format!("{path}: {e}")))?;
                                scratch.vertices.clear();
                                scratch.indices.clear();
                                scratch.windows.clear();
                                let mut model =
                                    add_model(&path, &parsed, &mut scratch, &mut s.names)?;
                                place_own(
                                    &path,
                                    &mut model,
                                    &scratch,
                                    &mut chunks,
                                    &mut s.geometry,
                                    keep,
                                )?;
                                model
                            }
                        };
                        s.models.push(model);
                        by_path.insert(key, s.models.len() - 1);
                        s.models.len() - 1
                    }
                };
                by_name.insert(name.to_lowercase(), index);
                Some(index)
            }
            None => None,
        };
        if let Some(m) = model {
            s.instances.push(Instance::new(
                o,
                name.clone().unwrap_or_default(),
                m,
                &s.models[m],
            ));
        }
        if let Object::HangingLamp(lamp) = &o.record.object {
            let h = &o.record.header;
            let x = transform(h.position, h.angles);
            let model = model.map(|m| &s.models[m]);
            // hanging-lamps.md 3.4 step 4: a physics shell's fixed bones
            // (comma-separated; empty: the root) must be bones of the
            // model. The shell itself is not built (TODO(#347)); with the
            // flag and no model the game logs a warning, which Volya
            // Engine has nowhere to write.
            if lamp.light_flags & LAMP_PHYSICS != 0
                && let Some(m) = model
            {
                for bone in lamp.fixed_bones.split(',').filter(|b| !b.is_empty()) {
                    if m.bone(&s.names, bone).is_none() {
                        return Err(invalid(format!("{}: wrong fixed bone", h.name)));
                    }
                }
            }
            let lights = lamp_lights((&h.name, o.id, o.online), lamp, &x, model, &s.names);
            s.lamps.push(lights);
        }
    }
    let mut by_class: Vec<(String, usize)> = classes
        .into_iter()
        .map(|(c, n)| (c.to_owned(), n))
        .collect();
    by_class.sort();
    s.online_by_class = by_class;
    chunks.finish(keep);
    // Grown model by model: the spare capacity (a third of it for
    // `l01_escape`, measured) goes, shrunk in place (#357).
    s.geometry.vertices.shrink_to_fit();
    s.geometry.indices.shrink_to_fit();
    s.geometry.windows.shrink_to_fit();
    s.names.seal();
    Ok(s)
}

/// Places the meshes of `model`, the file at `path` laid out in `scratch`
/// by [`add_model`], in `chunks`, each chunk handed to `keep` once full; a
/// mesh of more vertices than a chunk holds goes to `geometry`, where its
/// detail windows go too.
fn place_own(
    path: &str,
    model: &mut LoadedModel,
    scratch: &ObjectGeometry,
    chunks: &mut Chunks,
    geometry: &mut ObjectGeometry,
    keep: &mut dyn FnMut(GeometryChunk),
) -> io::Result<()> {
    let bytes = crate::render::world::MODEL_VERTEX_BYTES;
    let window = geometry.windows.len() as u32;
    geometry.windows.extend_from_slice(&scratch.windows);
    for mesh in &mut model.meshes {
        mesh.windows[0] += window;
        if mesh.vertex_count as usize <= CHUNK_VERTICES {
            chunks.place(path, mesh, scratch, keep)?;
            continue;
        }
        let first = mesh.first_vertex as usize * bytes;
        let to = first + mesh.vertex_count as usize * bytes;
        mesh.first_vertex = (geometry.vertices.len() / bytes) as u32;
        geometry
            .vertices
            .extend_from_slice(&scratch.vertices[first..to]);
        let from = mesh.first_index as usize;
        mesh.first_index = geometry.indices.len() as u32;
        (geometry.indices)
            .extend_from_slice(&scratch.indices[from..from + mesh.index_count as usize]);
    }
    Ok(())
}

/// One object mesh's draw: a range of [`ObjectGeometry`] or of a kept
/// chunk with a surface, the object's transform, the mesh's sphere and
/// `L_dynamic_props`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectDraw {
    /// Its instance in [`LevelObjects::instances`].
    pub instance: usize,
    pub surface: u32,
    pub first_vertex: u32,
    pub first_index: u32,
    pub index_count: u32,
    /// [`MeshRange::windows`].
    pub windows: [u32; 2],
    /// [`MeshRange::kept`].
    pub kept: Option<u32>,
    pub transform: Mat4,
    /// The mesh's own sphere, its centre placed by the transform and its
    /// radius as stored: each part's small-part test
    /// (object-detail-level.md 3).
    pub sphere: [f32; 4],
    /// The mesh's sphere in model space ([`MeshRange::sphere`]).
    pub mesh_sphere: [f32; 4],
    pub dynamic_props: [f32; 4],
}

impl ObjectDraw {
    /// [`ObjectDraw::sphere`] with the object at `transform`.
    pub fn sphere_at(&self, transform: &Mat4) -> [f32; 4] {
        let s = self.mesh_sphere;
        let c = transform_point(transform, [s[0], s[1], s[2]]);
        [c[0], c[1], c[2], s[3]]
    }
}

/// The objects as the level keeps them.
#[derive(Debug, Default)]
pub struct LevelObjects {
    /// Online at the start.
    pub online: usize,
    pub online_by_class: Vec<(String, usize)>,
    /// Every object whose visual is loaded, online or not.
    pub instances: Vec<Instance>,
    pub models: Vec<LoadedModel>,
    /// The models' names.
    pub names: Names,
    pub draws: Vec<ObjectDraw>,
    pub lamps: Vec<LampLights>,
    /// Shader pairs not drawn, with why.
    pub not_drawn: Vec<(String, String)>,
    geometry: Mutex<Option<ObjectGeometry>>,
    /// The set of kept geometry its own models' chunks are
    /// ([`Spawned::set`]), and those not kept yet.
    pub set: u64,
    chunks: Mutex<Vec<GeometryChunk>>,
    /// The prefetched models whose kept geometry the draws with
    /// [`ObjectDraw::kept`] read.
    pub prefetched: Option<Arc<Prefetched>>,
}

impl LevelObjects {
    /// The geometry, once: whoever takes it owns the only copy.
    pub fn take_geometry(&self) -> Option<ObjectGeometry> {
        self.geometry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// The chunks of its own models' geometry not kept yet, once: whoever
    /// takes them keeps them.
    pub fn take_chunks(&self) -> Vec<GeometryChunk> {
        std::mem::take(&mut *self.chunks.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// The objects with their draws: `mesh_surfaces[m][k]` is the surface
    /// of mesh k of model m (`None`: not drawn), `props` each instance's
    /// `L_dynamic_props`.
    pub fn new(
        spawned: Spawned,
        mesh_surfaces: &[Vec<Option<u32>>],
        props: &[[f32; 4]],
        not_drawn: Vec<(String, String)>,
    ) -> Self {
        let mut draws = Vec::new();
        for (i, (instance, props)) in spawned.instances.iter().zip(props).enumerate() {
            let model = &spawned.models[instance.model];
            for (mesh, surface) in model.meshes.iter().zip(&mesh_surfaces[instance.model]) {
                let Some(surface) = *surface else { continue };
                if mesh.index_count == 0 {
                    continue;
                }
                draws.push(ObjectDraw {
                    instance: i,
                    surface,
                    first_vertex: mesh.first_vertex,
                    first_index: mesh.first_index,
                    index_count: mesh.index_count,
                    windows: mesh.windows,
                    kept: mesh.kept,
                    transform: instance.transform,
                    sphere: {
                        let s = mesh.sphere;
                        let c = transform_point(&instance.transform, [s[0], s[1], s[2]]);
                        [c[0], c[1], c[2], s[3]]
                    },
                    mesh_sphere: mesh.sphere,
                    dynamic_props: *props,
                });
            }
        }
        Self {
            online: spawned.online,
            online_by_class: spawned.online_by_class,
            instances: spawned.instances,
            models: spawned.models,
            names: spawned.names,
            draws,
            lamps: spawned.lamps,
            not_drawn,
            geometry: Mutex::new(Some(spawned.geometry)),
            set: spawned.set,
            chunks: Mutex::new(spawned.chunks),
            prefetched: spawned.prefetched,
        }
    }
}

/// Each instance's converged `L_dynamic_props` (object-projected-light.md 5.2,
/// 5.3; model-shading.md 6) against the
/// static collision `world` under sunlight travelling along `sun_dir`.
pub fn light_inputs(
    world: &CollisionWorld,
    instances: &[Instance],
    sun_dir: [f32; 3],
) -> Vec<[f32; 4]> {
    instances
        .iter()
        .map(|i| {
            let o = object_light::ray_origin([i.sphere[0], i.sphere[1], i.sphere[2]], i.sphere[3]);
            let h = object_light::sky_visibility(world, o);
            let s = object_light::sun_visibility(world, o, sun_dir);
            object_light::dynamic_props(h, s)
        })
        .collect()
}

/// What the engine interface reports about the objects.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ObjectsReport {
    /// Online objects of the level.
    pub online: usize,
    /// Online objects drawn with a model.
    pub with_model: usize,
    /// Objects whose visual is loaded, online or not.
    pub loaded: usize,
    pub models: usize,
    pub draws: usize,
    pub vertices: usize,
    pub indices: usize,
    pub lamps: usize,
    pub not_drawn: usize,
}

impl LevelObjects {
    pub fn report(&self) -> ObjectsReport {
        let (vertices, indices) = self
            .models
            .iter()
            .flat_map(|m| &m.meshes)
            .fold((0, 0), |(v, i), m| {
                (v + m.vertex_count as usize, i + m.index_count as usize)
            });
        ObjectsReport {
            online: self.online,
            with_model: self.instances.iter().filter(|i| i.online).count(),
            loaded: self.instances.len(),
            models: self.models.len(),
            draws: self.draws.len(),
            vertices,
            indices,
            lamps: self.lamps.len(),
            not_drawn: self.not_drawn.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// object-detail-level.md 3: a part's sphere is its own header
    /// sphere, the centre placed by the object's transform, the radius as
    /// stored even under a scale.
    #[test]
    fn a_part_is_tested_on_its_own_sphere() {
        let mut x = [[0.0; 4]; 4];
        for (i, row) in x.iter_mut().enumerate().take(3) {
            row[i] = 2.0;
        }
        x[3] = [10.0, 20.0, 30.0, 1.0];
        let draw = ObjectDraw {
            instance: 0,
            surface: 0,
            first_vertex: 0,
            first_index: 0,
            index_count: 3,
            windows: [0, 0],
            kept: None,
            transform: x,
            sphere: [0.0; 4],
            mesh_sphere: [1.0, 0.0, -1.0, 0.25],
            dynamic_props: [0.0; 4],
        };
        assert_eq!(draw.sphere_at(&x), [12.0, 20.0, 28.0, 0.25]);
    }

    /// Each name is kept once.
    #[test]
    fn names_are_kept_once() {
        let mut names = Names::default();
        let a = names.add("models\\model");
        let b = names.add("act\\act_barman");
        assert_eq!(names.add("models\\model"), a);
        assert_ne!(a, b);
        assert_eq!(
            (names.get(a), names.get(b)),
            ("models\\model", "act\\act_barman")
        );
        names.seal();
        assert_eq!(names.get(b), "act\\act_barman");
    }

    /// A bind matrix whose fourth column is not exactly (0, 0, 0, 1) keeps
    /// every one of its model's matrices whole.
    #[test]
    fn bind_matrices_stay_exact() {
        let mut names = Names::default();
        let header = model::Header {
            kind: 10,
            shader: 0,
            box_min: [0.0; 3],
            box_max: [0.0; 3],
            sphere_centre: [0.0; 3],
            sphere_radius: 1.0,
        };
        let a = transform([1.0, 2.0, 3.0], [0.5, 0.25, 0.0]);
        let mut b = a;
        b[0][3] = -0.0;
        let (na, nb) = (names.add("a"), names.add("b"));
        let affine = LoadedModel::new(na, header, Box::default(), &[(na, a)]);
        assert!(affine.binds.is_empty());
        assert_eq!(affine.bind(0), a);
        let whole = LoadedModel::new(na, header, Box::default(), &[(na, a), (nb, b)]);
        assert_eq!(whole.binds.len(), 2);
        assert_eq!(
            whole.bone(&names, "b").map(|m| m[0][3].to_bits()),
            Some((-0.0f32).to_bits())
        );
        assert_eq!(whole.bone(&names, "a"), Some(a));
    }

    #[test]
    fn lamp_lights_take_the_lamp_block() {
        use volya_engine_formats::object::{ObjectBlock, Skeleton, Visual};
        let lamp = HangingLamp {
            object: ObjectBlock {
                game_graph_vertex: 0,
                distance: 0.0,
                direct_control: 1,
                level_graph_node: 0,
                state_flags: 3,
                custom_data: String::new().into(),
                story_id: -1,
                spawn_story_id: -1,
            },
            visual: Visual {
                model: String::new().into(),
                flags: 0,
            },
            skeleton: Skeleton {
                startup_animation: String::new().into(),
                flags: 0,
                source_id: 0xFFFF,
                bone_state: None,
            },
            colour: 0x0080_40FF,
            brightness: 0.5,
            colour_animator: "light 40 hz".into(),
            range: 2.0,
            light_flags: 0xB57C,
            startup_animation: String::new().into(),
            fixed_bones: String::new().into(),
            health: 100.0,
            virtual_size: 0.1,
            ambient_radius: 3.0,
            ambient_power: 0.5,
            ambient_texture: "lights\\x".into(),
            light_texture: String::new().into(),
            light_bone: "bone_lamp".into(),
            spot_cone_angle: 2.0,
            glow_texture: "glow\\glow_orange".into(),
            glow_radius: 0.3,
            ambient_light_bone: String::new().into(),
        };
        let x = transform([1.0, 2.0, 3.0], [0.0; 3]);
        let mut bone = transform([0.0, 0.5, 0.0], [0.0; 3]);
        bone[2] = [0.0, -1.0, 0.0, 0.0];
        let mut names = Names::default();
        let model = LoadedModel::new(
            names.add(""),
            model::Header {
                kind: 10,
                shader: 0,
                box_min: [0.0; 3],
                box_max: [0.0; 3],
                sphere_centre: [0.0; 3],
                sphere_radius: 1.0,
            },
            Box::default(),
            &[(names.add("bone_lamp"), bone)],
        );
        assert_eq!(model.bind(0), bone, "kept exactly");
        let l = lamp_lights(("tv", 7, true), &lamp, &x, Some(&model), &names);
        assert_eq!(
            (l.main.kind, l.main.shadows, l.on),
            (LightKind::Spot, false, true)
        );
        assert!(!l.physics_shell);
        // (v × K) × brightness (hanging-lamps.md 3.2).
        let k = lanims::K;
        assert_eq!(
            l.main.colour,
            [128.0 * k * 0.5, 64.0 * k * 0.5, 255.0 * k * 0.5]
        );
        assert_eq!((l.id, l.online, l.health), (7, true, 100.0));
        // At the bone: its bind matrix · X.
        assert_eq!(l.main.position, [1.0, 2.5, 3.0]);
        assert_eq!(l.main.direction, [0.0, -1.0, 0.0]);
        let ambient = l.ambient.expect("bit 0x20");
        assert_eq!((ambient.kind, ambient.range), (LightKind::Point, 3.0));
        assert_eq!(ambient.colour, l.main.colour.map(|c| c * 0.5));
        // An empty ambient bone is no bone: at X (hanging-lamps.md 4.2).
        assert_eq!(ambient.position, [1.0, 2.0, 3.0]);
        assert_eq!(l.glow.unwrap().position, [1.0, 2.5, 3.0]);
        let at_bone = HangingLamp {
            ambient_light_bone: "bone_lamp".into(),
            ..lamp.clone()
        };
        let l = lamp_lights(("tv", 7, true), &at_bone, &x, Some(&model), &names);
        assert_eq!(l.ambient.unwrap().position, [1.0, 2.5, 3.0]);
        // Without the bone, at X.
        let l = lamp_lights(("tv", 7, true), &lamp, &x, None, &names);
        assert_eq!(l.main.position, [1.0, 2.0, 3.0]);
        let off = HangingLamp {
            health: 0.0,
            light_flags: 0xB55D,
            ..lamp
        };
        let l = lamp_lights(("lamp", 8, false), &off, &x, None, &names);
        assert!(!l.on && l.physics_shell && l.ambient.is_none());
    }
}
