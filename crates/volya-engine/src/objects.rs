// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's spawned objects drawn at rest on New Game
//! (docs/behavior/level-objects.md): which records of `spawns\all.spawn`
//! are online (section 2, the offline simulation's switch pass,
//! [`crate::core::simulation`]), each one's visual
//! and transform (section 3), its model file (section 4,
//! [`volya_engine_formats::model`]) drawn at its stored positions
//! quantized as the first renderer stores them (4.7, 4.8), and the
//! hanging lamps' lights as structure (section 6).
//!
//! Loading runs in the level's loading stages ([`crate::level::NewGameLoad`]):
//! the switch pass, the models (taken from the game type's prefetched ones,
//! [`Prefetched`], where they hold the file), their surfaces, the light
//! inputs. Each model file is read once and shared by every object showing
//! it; its vertices go into one buffer for all models, which the presenter
//! uploads with the static world and then drops.

use std::collections::HashMap;
use std::io;
use std::sync::Mutex;

use serde::Serialize;

use crate::core::character::SpecificCharacter;
use crate::core::collision::CollisionWorld;
use crate::core::config::value;
use crate::core::object_light;
use crate::core::simulation::{NO_ID, Simulation};
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use crate::render::world::{DetailWindow, ObjectGeometry, model_vertex};
use volya_engine_formats::cp1251;
use volya_engine_formats::lanims;
use volya_engine_formats::model::{self, Mat4, Model, transform, transform_point};
use volya_engine_formats::object::{HangingLamp, Kind, Object, ObjectRecord, TraderLike};

use volya_engine_formats::xml;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// An object of the current level whose visual is loaded with the level:
/// one online at the start (section 2.6) or one that may come online later
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
}

/// The objects of `simulation`'s current level whose visuals the level
/// loads: those of its list without a parent that are online, or for which
/// the engine's *on* can hold (level-objects.md 2.3). The others never come
/// online here: the scripts' replacements only make *on* false where the
/// engine's is (section 2.5; a job-online value set true by a smart
/// terrain's script would not, but no script runs yet). Objects inside a
/// container are left out (our choice: none on the level has one, section
/// 2.1).
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
        })
        .collect()
}

/// The visual name an online object shows (section 3.2): the record's
/// Visual block, replaced for a trader by its specific character's
/// `<visual>`; `None` without a model (section 3.3: no Visual block, an
/// empty name, or the actor, whose body the first-person view does not
/// draw). The configuration's `visual` key is overridden by the Visual
/// block in every kind that has one, so it is not read. A dead body's
/// `corpse_visual` (section 3.2, item 4) is not handled: it applies only to
/// a body with a non-zero death time, and every dead body of the shipped
/// file has death time 0 (our omission).
pub fn visual_name(
    object: &LevelObject,
    character_visual: &mut dyn FnMut(&TraderLike) -> io::Result<Option<String>>,
) -> io::Result<Option<String>> {
    let record = &object.record;
    if record.kind() == Kind::Player {
        return Ok(None);
    }
    let Some(visual) = record.object.visual() else {
        return Ok(None);
    };
    let mut name = visual.model.clone();
    if let Object::Trader(t) = &record.object
        && let Some(v) = character_visual(&t.trader)?
    {
        name = v;
    }
    Ok((!name.is_empty()).then_some(name))
}

/// A visual name without its extension, lower-cased (section 3.2).
fn plain_name(name: &str) -> String {
    let name = name.to_lowercase();
    match name.rfind('.') {
        Some(dot) if !name[dot..].contains('\\') => name[..dot].to_owned(),
        _ => name,
    }
}

/// The specific characters (section 3.2, item 3; player-state.md 2.2),
/// read on first use: `config\gameplay\npc_profile.xml` and the files of
/// `[profiles] specific_characters_files`.
pub struct Characters<'a> {
    tree: &'a dyn Vfs,
    data: &'a GameData,
    /// Profile id → its named specific character.
    profiles: Option<HashMap<String, String>>,
    /// Specific character id → its description.
    descriptions: Option<HashMap<String, SpecificCharacter>>,
}

impl<'a> Characters<'a> {
    pub fn new(tree: &'a dyn Vfs, data: &'a GameData) -> Self {
        Self {
            tree,
            data,
            profiles: None,
            descriptions: None,
        }
    }

    fn document(&self, name: &str) -> io::Result<xml::Document> {
        let aliases = &self.data.aliases;
        let path = aliases.path("$game_config$", &format!("gameplay\\{name}"))?;
        let text = vfs::read(self.tree, &path)?;
        let text = xml::preprocess(&text, &mut |include| {
            let path = aliases.path("$game_config$", &cp1251::decode(include))?;
            vfs::read(self.tree, &path)
        })?;
        xml::parse(&text).map_err(|e| invalid(format!("{path}: {e}")))
    }

    /// The `<visual>` of the trader-like block's specific character
    /// ([`Self::specific_character`]) as a visual name; without one the
    /// record's visual is kept (our choice).
    pub fn visual(&mut self, trader: &TraderLike) -> io::Result<Option<String>> {
        Ok(self
            .specific_character(trader)?
            .map(|c| plain_name(&c.visual))
            .filter(|v| !v.is_empty()))
    }

    /// The trader-like block's specific character: the block's own if
    /// named, else its profile's `<specific_character>`. Profiles and
    /// descriptions are found by their `id` attribute, and a file named in
    /// `specific_characters_files` without `.xml` gets it appended (our
    /// reading: the spec says only that they name them). A profile that
    /// names none chooses at random among the descriptions of its class
    /// (level-objects.md 3.2, item 3); we give none then.
    // TODO(#338): the random choice.
    pub fn specific_character(
        &mut self,
        trader: &TraderLike,
    ) -> io::Result<Option<&SpecificCharacter>> {
        let id = if trader.specific_character.is_empty() {
            if self.profiles.is_none() {
                let doc = self.document("npc_profile.xml")?;
                let mut profiles = HashMap::new();
                if let Some(root) = doc.root() {
                    for c in root.children_named("character") {
                        if let (Some(id), Some(sc)) = (
                            c.attr("id"),
                            c.child("specific_character").and_then(|e| e.value()),
                        ) {
                            profiles.insert(id.to_owned(), sc.to_owned());
                        }
                    }
                }
                self.profiles = Some(profiles);
            }
            match self
                .profiles
                .as_ref()
                .and_then(|p| p.get(&trader.character_profile))
            {
                Some(id) => id.clone(),
                None => return Ok(None),
            }
        } else {
            trader.specific_character.clone()
        };
        if self.descriptions.is_none() {
            let files = self
                .data
                .system
                .get("profiles", "specific_characters_files")
                .flatten()
                .unwrap_or("");
            let mut descriptions = HashMap::new();
            for file in files.split(',').map(str::trim).filter(|f| !f.is_empty()) {
                let name = if file.ends_with(".xml") {
                    file.to_owned()
                } else {
                    format!("{file}.xml")
                };
                let doc = self.document(&name)?;
                for d in doc.nodes.iter().filter_map(|n| match n {
                    xml::Node::Element(e) => Some(e),
                    xml::Node::Text(_) => None,
                }) {
                    for c in d.children_named("specific_character") {
                        let Some(id) = c.attr("id") else {
                            continue;
                        };
                        let text = |name: &str| {
                            c.child(name)
                                .and_then(|e| e.value())
                                .unwrap_or("")
                                .to_owned()
                        };
                        // Rank and reputation as integers (`00` is 0).
                        let int = |name: &str| value::int(&text(name)) as i32;
                        descriptions.insert(
                            id.to_owned(),
                            SpecificCharacter {
                                id: id.to_owned(),
                                community: text("community"),
                                rank: int("rank"),
                                reputation: int("reputation"),
                                name: text("name"),
                                visual: text("visual"),
                                // The text writes its line breaks as `\n`
                                // (player-state.md 3.1 item 3; that they
                                // break lines is our reading).
                                supplies: c
                                    .child("supplies")
                                    .and_then(|e| e.raw_value())
                                    .unwrap_or("")
                                    .replace("\\n", "\n"),
                            },
                        );
                    }
                }
            }
            self.descriptions = Some(descriptions);
        }
        Ok(self.descriptions.as_ref().and_then(|d| d.get(&id)))
    }
}

/// The file of visual name `name` (section 3.1): `name` as given, then
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

/// A child mesh of a loaded model: its shader pair and its range in the
/// objects' geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshRange {
    pub texture: String,
    pub shader: String,
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
}

/// A model file loaded for drawing: its header and meshes, the vertices
/// already in the objects' geometry.
#[derive(Clone, Debug)]
pub struct LoadedModel {
    pub path: String,
    pub header: model::Header,
    pub meshes: Vec<MeshRange>,
    /// Bone names and their bind matrices, for the lamps' lights.
    pub bones: Vec<(String, Mat4)>,
}

/// Appends `model`'s vertices, quantized (section 4.7), and its meshes'
/// indices and detail windows to `geometry`. Every mesh is quantized whatever its
/// render mode: the software mode keeps floats, but needs a bone index
/// above 78 with 256 vertex shader constants, which no shipped model has
/// (our choice: the constant count of a GPU device has no meaning here).
pub fn add_model(
    path: &str,
    model: &Model,
    geometry: &mut ObjectGeometry,
) -> io::Result<LoadedModel> {
    let pose = model.bind_pose()?;
    let mut meshes = Vec::with_capacity(model.meshes.len());
    for m in &model.meshes {
        let first_vertex =
            (geometry.vertices.len() / crate::render::world::MODEL_VERTEX_BYTES) as u32;
        for v in &m.vertices {
            geometry.vertices.extend(model_vertex(
                v.position.map(model::quantize_position),
                model::quantize_normal(v.normal),
                v.uv.map(model::quantize_uv),
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
            geometry
                .windows
                .extend(w.steps.iter().map(|s| DetailWindow {
                    offset: s.index_offset - span.start as u32,
                    index_count: 3 * u32::from(s.triangle_count),
                }));
        }
        let windows = [first_window, geometry.windows.len() as u32 - first_window];
        meshes.push(MeshRange {
            texture: m.texture.clone(),
            shader: m.shader.clone(),
            first_vertex,
            vertex_count: m.vertices.len() as u32,
            first_index,
            index_count: indices.len() as u32,
            windows,
            sphere: {
                let (c, r) = (m.header.sphere_centre, m.header.sphere_radius);
                [c[0], c[1], c[2], r]
            },
        });
    }
    Ok(LoadedModel {
        path: path.to_owned(),
        header: model.header,
        meshes,
        bones: model
            .bones
            .iter()
            .zip(pose)
            .map(|(b, m)| (b.name.clone(), m))
            .collect(),
    })
}

/// The section listing the models prefetched for the single-player game
/// type (console-diagnostics.md 2.4.1: `prefetch_visuals_single`, its keys
/// the visual names).
pub const PREFETCH_VISUALS: &str = "prefetch_visuals_single";

/// The models loaded under `st_prefetching_objects` and kept while the game
/// type stays (console-diagnostics.md 2.4.1; docs/architecture/memory.md,
/// Lifetimes): each listed visual's file read once, parsed and laid out as
/// [`add_model`] lays it out, each model's vertices, indices and detail
/// windows in blocks of its own (so the set grows by one model at a time,
/// with no growing block's spare). A level's objects take their models from
/// here ([`spawn`]); their surfaces' texture files are listed beside, for
/// the platform to load with them.
#[derive(Debug, Default)]
pub struct Prefetched {
    /// By located path, lower-cased: the model's index.
    by_path: HashMap<String, usize>,
    models: Vec<PrefetchedModel>,
    /// The texture files of the models' surfaces, each once.
    pub textures: Vec<String>,
}

/// A prefetched model, as [`add_model`] made it into a geometry of its own.
#[derive(Debug)]
struct PrefetchedModel {
    model: LoadedModel,
    vertices: Box<[u8]>,
    indices: Box<[u16]>,
    windows: Box<[DetailWindow]>,
}

impl Prefetched {
    /// Loads the models of the system configuration's section
    /// [`PREFETCH_VISUALS`], in its key order, each file once (names
    /// locating the same file share it). A name that locates no file, or a
    /// file that does not parse, fails it, as an object's model fails the
    /// level's load. Each file is held in the file cache while it is read
    /// (docs/architecture/memory.md, File-backed data).
    pub fn load(tree: &dyn Vfs, data: &GameData) -> io::Result<Self> {
        let mut p = Self::default();
        let Some(section) = data.system.section(PREFETCH_VISUALS) else {
            return Ok(p);
        };
        // One model's layout at a time, then copied into blocks its size.
        let mut scratch = ObjectGeometry::default();
        for entry in section.entries() {
            let path = locate_model(tree, data, "", entry.key())?;
            let key = path.to_lowercase();
            if p.by_path.contains_key(&key) {
                continue;
            }
            let model = {
                let bytes = crate::core::file_cache::read(tree, &path)?;
                let parsed = Model::parse(&bytes).map_err(|e| invalid(format!("{path}: {e}")))?;
                scratch.vertices.clear();
                scratch.indices.clear();
                scratch.windows.clear();
                add_model(&path, &parsed, &mut scratch)?
            };
            p.models.push(PrefetchedModel {
                model,
                vertices: scratch.vertices.as_slice().into(),
                indices: scratch.indices.as_slice().into(),
                windows: scratch.windows.as_slice().into(),
            });
            p.by_path.insert(key, p.models.len() - 1);
        }
        Ok(p)
    }

    /// How many models it holds.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Whether it holds the model of the file at `path`.
    pub fn holds(&self, path: &str) -> bool {
        self.by_path.contains_key(&path.to_lowercase())
    }

    /// Every model, as loaded (its ranges in its own geometry).
    pub fn models(&self) -> impl Iterator<Item = &LoadedModel> {
        self.models.iter().map(|m| &m.model)
    }

    /// The model of the file at `path`, if prefetched, appended to
    /// `geometry` as [`add_model`] appends the file's.
    pub fn append(&self, path: &str, geometry: &mut ObjectGeometry) -> Option<LoadedModel> {
        let m = &self.models[*self.by_path.get(&path.to_lowercase())?];
        let vertex = (geometry.vertices.len() / crate::render::world::MODEL_VERTEX_BYTES) as u32;
        let index = geometry.indices.len() as u32;
        let window = geometry.windows.len() as u32;
        geometry.vertices.extend_from_slice(&m.vertices);
        geometry.indices.extend_from_slice(&m.indices);
        geometry.windows.extend_from_slice(&m.windows);
        let mut model = m.model.clone();
        model.path = path.to_owned();
        for mesh in &mut model.meshes {
            mesh.first_vertex += vertex;
            mesh.first_index += index;
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
    /// Model space to world (section 3.4).
    pub transform: Mat4,
    /// The model's header sphere in world space: centre, radius.
    pub sphere: [f32; 4],
}

impl Instance {
    pub fn new(object: &LevelObject, visual: String, model: usize, loaded: &LoadedModel) -> Self {
        let h = &object.record.header;
        let transform = transform(h.position, h.angles);
        let c = transform_point(&transform, loaded.header.sphere_centre);
        Self {
            id: object.id,
            index: object.index,
            online: object.online,
            name: h.name.clone(),
            class_id: object.record.class_id.as_str().to_owned(),
            kind: object.record.kind(),
            visual,
            model,
            transform,
            sphere: [c[0], c[1], c[2], loaded.header.sphere_radius],
        }
    }
}

/// The kind of a lamp's light (section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LightKind {
    Point,
    Spot,
}

/// A light as section 6 sets it up; at rest, its placement from the bind
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

/// A glow at the light's position (section 6; its drawing is
/// unestablished).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Glow {
    pub texture: String,
    pub radius: f32,
    pub position: [f32; 3],
}

/// The lights of a hanging lamp (section 6; docs/behavior/hanging-lamps.md
/// 2, 3), at rest.
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

/// Light flags (section 6).
const LAMP_PHYSICS: u16 = 0x01;
const LAMP_SHADOWS: u16 = 0x02;
const LAMP_SPOT: u16 = 0x10;
const LAMP_AMBIENT: u16 = 0x20;

fn row(m: &Mat4, r: usize) -> [f32; 3] {
    [m[r][0], m[r][1], m[r][2]]
}

/// The lights of `lamp` placed at transform `x` with its model's bones
/// (`None` without a visual): each light's matrix is its bone's bind
/// matrix · X, or X when the bone is not one of the visual's.
pub fn lamp_lights(
    object: (&str, u16, bool),
    lamp: &HangingLamp,
    x: &Mat4,
    model: Option<&LoadedModel>,
) -> LampLights {
    let flags = lamp.light_flags;
    let place = |bone: &str| -> Mat4 {
        match model.and_then(|m| m.bones.iter().find(|(n, _)| n == bone)) {
            Some((_, b)) => model::multiply(b, x),
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
        texture: lamp.light_texture.clone(),
        colour_animator: lamp.colour_animator.clone(),
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
            texture: lamp.ambient_texture.clone(),
            colour_animator: String::new(),
            position: row(&m, 3),
            direction: row(&m, 2),
            right: row(&m, 0),
        }
    });
    let glow = (!lamp.glow_texture.is_empty()).then(|| Glow {
        texture: lamp.glow_texture.clone(),
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
    /// Online objects at the start (section 2.6).
    pub online: usize,
    /// Online objects per class id.
    pub online_by_class: Vec<(String, usize)>,
    pub models: Vec<LoadedModel>,
    pub instances: Vec<Instance>,
    pub lamps: Vec<LampLights>,
    pub geometry: ObjectGeometry,
}

/// Loads the visuals of `objects` (sections 3, 4): each model file once,
/// taken from `prefetched` where it holds the file.
pub fn spawn(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    objects: &[LevelObject],
    prefetched: Option<&Prefetched>,
) -> io::Result<Spawned> {
    let mut characters = Characters::new(tree, data);
    // Models by file path, and by visual name (located once each).
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut s = Spawned {
        online: objects.iter().filter(|o| o.online).count(),
        ..Default::default()
    };
    let mut classes: HashMap<&str, usize> = HashMap::new();
    for o in objects {
        if o.online {
            *classes.entry(o.record.class_id.as_str()).or_default() += 1;
        }
        let name = visual_name(o, &mut |t| characters.visual(t))?;
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
                        let prefetched = prefetched.and_then(|p| p.append(&path, &mut s.geometry));
                        let model = match prefetched {
                            Some(model) => model,
                            None => {
                                let bytes = vfs::read(tree, &path)?;
                                let parsed = Model::parse(&bytes)
                                    .map_err(|e| invalid(format!("{path}: {e}")))?;
                                add_model(&path, &parsed, &mut s.geometry)?
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
                    if !m.bones.iter().any(|(n, _)| n == bone) {
                        return Err(invalid(format!("{}: wrong fixed bone", h.name)));
                    }
                }
            }
            s.lamps
                .push(lamp_lights((&h.name, o.id, o.online), lamp, &x, model));
        }
    }
    let mut by_class: Vec<(String, usize)> = classes
        .into_iter()
        .map(|(c, n)| (c.to_owned(), n))
        .collect();
    by_class.sort();
    s.online_by_class = by_class;
    // Grown model by model: the spare capacity (a third of it for
    // `l01_escape`, measured) goes, shrunk in place (#357).
    s.geometry.vertices.shrink_to_fit();
    s.geometry.indices.shrink_to_fit();
    Ok(s)
}

/// One object mesh's draw: a range of [`ObjectGeometry`] with a surface,
/// the object's transform, the mesh's sphere and `L_dynamic_props`.
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
    pub draws: Vec<ObjectDraw>,
    pub lamps: Vec<LampLights>,
    /// Shader pairs not drawn, with why.
    pub not_drawn: Vec<(String, String)>,
    geometry: Mutex<Option<ObjectGeometry>>,
}

impl LevelObjects {
    /// The geometry, once: whoever takes it owns the only copy.
    pub fn take_geometry(&self) -> Option<ObjectGeometry> {
        self.geometry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
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
            draws,
            lamps: spawned.lamps,
            not_drawn,
            geometry: Mutex::new(Some(spawned.geometry)),
        }
    }
}

/// Each instance's converged `L_dynamic_props` (section 5.4) against the
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
            transform: x,
            sphere: [0.0; 4],
            mesh_sphere: [1.0, 0.0, -1.0, 0.25],
            dynamic_props: [0.0; 4],
        };
        assert_eq!(draw.sphere_at(&x), [12.0, 20.0, 28.0, 0.25]);
    }

    #[test]
    fn visual_names_lose_extensions() {
        assert_eq!(
            plain_name("Actors\\Trader\\trader.ogf"),
            "actors\\trader\\trader"
        );
        assert_eq!(plain_name("weapons\\vodka\\vodka"), "weapons\\vodka\\vodka");
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
                custom_data: String::new(),
                story_id: -1,
                spawn_story_id: -1,
            },
            visual: Visual {
                model: String::new(),
                flags: 0,
            },
            skeleton: Skeleton {
                startup_animation: String::new(),
                flags: 0,
                source_id: 0xFFFF,
                bone_state: None,
            },
            colour: 0x0080_40FF,
            brightness: 0.5,
            colour_animator: "light 40 hz".into(),
            range: 2.0,
            light_flags: 0xB57C,
            startup_animation: String::new(),
            fixed_bones: String::new(),
            health: 100.0,
            virtual_size: 0.1,
            ambient_radius: 3.0,
            ambient_power: 0.5,
            ambient_texture: "lights\\x".into(),
            light_texture: String::new(),
            light_bone: "bone_lamp".into(),
            spot_cone_angle: 2.0,
            glow_texture: "glow\\glow_orange".into(),
            glow_radius: 0.3,
            ambient_light_bone: String::new(),
        };
        let x = transform([1.0, 2.0, 3.0], [0.0; 3]);
        let mut bone = transform([0.0, 0.5, 0.0], [0.0; 3]);
        bone[2] = [0.0, -1.0, 0.0, 0.0];
        let model = LoadedModel {
            path: String::new(),
            header: model::Header {
                kind: 10,
                shader: 0,
                box_min: [0.0; 3],
                box_max: [0.0; 3],
                sphere_centre: [0.0; 3],
                sphere_radius: 1.0,
            },
            meshes: Vec::new(),
            bones: vec![("bone_lamp".into(), bone)],
        };
        let l = lamp_lights(("tv", 7, true), &lamp, &x, Some(&model));
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
        let l = lamp_lights(("tv", 7, true), &at_bone, &x, Some(&model));
        assert_eq!(l.ambient.unwrap().position, [1.0, 2.5, 3.0]);
        // Without the bone, at X.
        let l = lamp_lights(("tv", 7, true), &lamp, &x, None);
        assert_eq!(l.main.position, [1.0, 2.0, 3.0]);
        let off = HangingLamp {
            health: 0.0,
            light_flags: 0xB55D,
            ..lamp
        };
        let l = lamp_lights(("lamp", 8, false), &off, &x, None);
        assert!(!l.on && l.physics_shell && l.ambient.is_none());
    }
}
