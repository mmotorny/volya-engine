// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The loading screen of a New Game (docs/behavior/loading-screen.md): the
//! stage captions in order (section 5), each beginning a new image of
//! background, progress strip, caption and, once the level is known, the
//! level's picture (sections 3, 4).
//!
//! Loading is synchronous on the engine thread, and a browser shows what
//! was presented only when the thread yields, so [`Loading::step`] runs one
//! stage per engine frame: the frame that begins a caption draws its image,
//! the next runs that caption's work (with the image on screen) and begins
//! the next caption. Each caption thus stays up for its work plus up to one
//! display frame (our choice; the spec shows it for exactly its work,
//! section 5). The ordinary frame the spec presents after the first
//! caption's work, whose content is not established (section 2, step 2),
//! is not presented: the next caption's image follows directly.
//!
//! After the last stage the app draws the image over the level's first
//! [`OVERLAY_FRAMES`] frames with sound muted (sections 2, 6), and starts
//! the New Game intro under its last frames ([`intro_due`], section 7).

use std::io;

use serde::Serialize;

use crate::core::text::StringTable;
use crate::core::vfs::Vfs;
use crate::data::{self, Assets, GameData};
use crate::draw::Ctx;
use crate::level::{LevelScreen, NewGameLoad};
use crate::scene::{Scene, Sprite, TextureId};
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::ui::layout::HAlign;
use volya_engine_formats::ui::picture::Quad;
use volya_engine_formats::ui::{Color, Resolution};

/// The game type of a New Game (`start server(all/single/…)`,
/// level-loading.md section 2.1).
pub const GAME_TYPE: &str = "single";
/// The client address: the start command's client parameter `localhost`
/// up to its first `/` (section 5, caption 6).
pub const CLIENT_ADDRESS: &str = "localhost";
/// The expected caption total *N* of a New Game from the menu (section
/// 4.2). This platform has no command line, so it never contains `alife`.
pub const EXPECTED_CAPTIONS: u32 = 14;
/// Rendered frames of the running level the loading image stays over
/// (section 2, step 4; `-gpu_ref` does not exist here).
pub const OVERLAY_FRAMES: u32 = 30;
/// The fourth server parameter of a New Game's start command
/// `start server(all/single/alife/new)` (level-loading.md section 2.1).
pub const SERVER_NEW_GAME: &str = "new";
/// The intro starts once at most this many overlay frames remain
/// (section 7).
pub const INTRO_OVERLAY_FRAMES: u32 = 2;
/// The caption colour 0xFF9D8C78 (section 4.4).
pub const CAPTION_COLOR: Color = Color::rgba(157, 140, 120, 255);
/// The caption font, `[ui_font_graffiti19_russian]` (section 4.4).
pub const CAPTION_FONT: UiFont = UiFont::Graffiti19;
/// Vertex columns of the progress strip (section 4.2).
pub const STRIP_COLUMNS: usize = 41;
/// Longest caption drawn, in bytes (section 4.3).
pub const MAX_CAPTION_BYTES: usize = 255;

/// The background's and the strip's texture `textures\ui\ui_load.dds`
/// (section 4.1).
pub const BACKGROUND: &str = "ui\\ui_load";
/// The level picture `textures\intro\intro_<level>.dds` and its fallback
/// (section 4.5).
pub const PICTURE_PREFIX: &str = "intro\\intro_";
pub const NO_PICTURE: &str = "intro\\intro_no_start_picture";

/// Whether the New Game intro starts on an overlay frame (section 7): a
/// level is loaded, at most [`INTRO_OVERLAY_FRAMES`] overlay frames remain
/// counting this one (so the intro's first frames are under at most two of
/// them; our reading), and the fourth server parameter is `new`, compared
/// case-insensitively. This platform has no command line, so no
/// `-nointro`. The caller checks once per started game.
pub fn intro_due(level_loaded: bool, overlay_frames_left: u32, server_parameter: &str) -> bool {
    level_loaded
        && overlay_frames_left <= INTRO_OVERLAY_FRAMES
        && server_parameter.eq_ignore_ascii_case(SERVER_NEW_GAME)
}

/// A loading stage, named by its caption (section 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    PrefetchingObjects,
    ServerStarting,
    LoadingAlifeSimulator,
    CreatingNewGame,
    ServerConnecting,
    ClientConnectingTo,
    OpeningStream,
    LoadingCform,
    LoadingShaders,
    LoadingGeometry,
    LoadingSpatialDb,
    LoadingDetails,
    LoadingSectorsPortals,
    LoadingAiObjects,
    ClientSpawning,
    LoadingTextures,
    ClientSynchronising,
}

impl Stage {
    /// Every stage of a first New Game, in order (section 5).
    pub const ALL: [Stage; 17] = [
        Self::PrefetchingObjects,
        Self::ServerStarting,
        Self::LoadingAlifeSimulator,
        Self::CreatingNewGame,
        Self::ServerConnecting,
        Self::ClientConnectingTo,
        Self::OpeningStream,
        Self::LoadingCform,
        Self::LoadingShaders,
        Self::LoadingGeometry,
        Self::LoadingSpatialDb,
        Self::LoadingDetails,
        Self::LoadingSectorsPortals,
        Self::LoadingAiObjects,
        Self::ClientSpawning,
        Self::LoadingTextures,
        Self::ClientSynchronising,
    ];

    /// The caption's string-table id.
    pub fn caption_id(self) -> &'static str {
        match self {
            Self::PrefetchingObjects => "st_prefetching_objects",
            Self::ServerStarting => "st_server_starting",
            Self::LoadingAlifeSimulator => "st_loading_alife_simulator",
            Self::CreatingNewGame => "st_creating_new_game",
            Self::ServerConnecting => "st_server_connecting",
            Self::ClientConnectingTo => "st_client_connecting_to",
            Self::OpeningStream => "st_opening_stream",
            Self::LoadingCform => "st_loading_cform",
            Self::LoadingShaders => "st_loading_shaders",
            Self::LoadingGeometry => "st_loading_geometry",
            Self::LoadingSpatialDb => "st_loading_spatial_db",
            Self::LoadingDetails => "st_loading_details",
            Self::LoadingSectorsPortals => "st_loading_sectors_portals",
            Self::LoadingAiObjects => "st_loading_ai_objects",
            Self::ClientSpawning => "st_client_spawning",
            Self::LoadingTextures => "st_loading_textures",
            Self::ClientSynchronising => "st_client_synchronising",
        }
    }
}

/// The stages of a New Game: `st_prefetching_objects` first only when the
/// game type differs from the previous game's in this run (section 5).
pub fn stages(game_type_changed: bool) -> &'static [Stage] {
    if game_type_changed {
        &Stage::ALL
    } else {
        &Stage::ALL[1..]
    }
}

/// The displayed caption of `stage` (section 4.3): the translation of its
/// id (for the connecting stage, the translation, a space and the client
/// address, looked up again) followed by `...`, as Windows-1251 bytes, at
/// most [`MAX_CAPTION_BYTES`] (longer is cut there; our choice).
pub fn caption_text(strings: &StringTable, stage: Stage) -> Vec<u8> {
    let id = stage.caption_id();
    let text = match stage {
        Stage::ClientConnectingTo => {
            let composed = format!("{} {CLIENT_ADDRESS}", strings.get(id));
            strings.get(&composed).to_owned()
        }
        _ => strings.get(id).to_owned(),
    };
    let mut bytes = data::encode(&text);
    bytes.extend_from_slice(b"...");
    bytes.truncate(MAX_CAPTION_BYTES);
    bytes
}

/// The strip's alpha per vertex column after `n` captions of an expected
/// `total` (section 4.2): a logistic step at column distance
/// *c* = 20 · (*n* + 1) / *N* from each end, mirrored about the centre.
pub fn strip_alpha(n: u32, total: u32) -> [u8; STRIP_COLUMNS] {
    let c = 20.0 * (n as f32 + 1.0) / total as f32;
    std::array::from_fn(|i| {
        let mirrored = if i <= 20 { i } else { 40 - i } as f32;
        let t = 1.0 / (1.0 + (0.5 * (mirrored - c)).exp());
        (255.0 * t).floor().clamp(0.0, 255.0) as u8
    })
}

/// The caption's pen position (section 4.4): floor(*W* · 0.5),
/// floor((0.815 + 1) · *H* · 0.5) in single precision.
pub fn caption_pen(resolution: Resolution) -> (f32, f32) {
    let (w, h) = (resolution.width as f32, resolution.height as f32);
    ((w * 0.5).floor(), ((0.815f32 + 1.0) * h * 0.5).floor())
}

/// The background's screen rectangle (section 3): the half-pixel offset
/// scaled with the screen.
pub fn background_rect((sx, sy): (f32, f32)) -> [f32; 4] {
    [-0.5 * sx, -0.5 * sy, 1023.5 * sx, 767.5 * sy]
}

/// The strip's screen rectangle (section 3): offset by exactly −0.5 pixel.
pub fn strip_rect((sx, sy): (f32, f32)) -> [f32; 4] {
    [
        379.0 * sx - 0.5,
        726.0 * sy - 0.5,
        647.0 * sx - 0.5,
        763.0 * sy - 0.5,
    ]
}

/// The level picture's screen rectangle (section 3).
pub fn picture_rect((sx, sy): (f32, f32)) -> [f32; 4] {
    [256.5 * sx, 368.5 * sy, 768.5 * sx, 624.5 * sy]
}

fn quad(r: [f32; 4], uv_min: (f32, f32), uv_max: (f32, f32), color: Color) -> Quad {
    Quad {
        min: (r[0], r[1]),
        max: (r[2], r[3]),
        uv_min,
        uv_max,
        color,
    }
}

/// What the loading image shows: the current caption, how many have begun
/// and the level's picture once known.
#[derive(Clone, Debug, Default)]
pub struct LoadingScreen {
    stage: Option<Stage>,
    /// Captions begun so far in this loading (*n*).
    captions: u32,
    caption: Vec<u8>,
    /// The picture's texture path, once the level is determined.
    picture: Option<String>,
}

/// What the engine interface reports.
#[derive(Clone, Debug, Serialize)]
pub struct LoadingScreenState {
    pub stage: Option<Stage>,
    pub captions: u32,
    pub expected: u32,
    pub caption: String,
    pub picture: Option<String>,
}

impl LoadingScreen {
    /// Begins the caption of `stage` (*n* rises by one).
    pub fn begin(&mut self, data: &GameData, stage: Stage) {
        self.stage = Some(stage);
        self.captions += 1;
        self.caption = caption_text(&data.strings, stage);
    }

    /// Selects the picture of level directory `level` (section 4.5):
    /// `intro\intro_<level>`, or the fallback when its file is absent.
    pub fn set_level(&mut self, tree: &dyn Vfs, data: &GameData, level: &str) -> io::Result<()> {
        let path = |name: &str| data.aliases.path("$game_textures$", &format!("{name}.dds"));
        let own = path(&format!("{PICTURE_PREFIX}{level}"))?;
        self.picture = Some(if tree.open(&own).is_ok() {
            own
        } else {
            path(NO_PICTURE)?
        });
        Ok(())
    }

    pub fn state(&self) -> LoadingScreenState {
        LoadingScreenState {
            stage: self.stage,
            captions: self.captions,
            expected: EXPECTED_CAPTIONS,
            caption: volya_engine_formats::cp1251::decode(&self.caption),
            picture: self.picture.clone(),
        }
    }

    /// The loading image alone at `resolution`.
    pub fn scene(
        &self,
        tree: &dyn Vfs,
        data: &GameData,
        assets: &mut Assets,
        resolution: Resolution,
    ) -> io::Result<Scene> {
        let mut scene = Scene::new(resolution);
        let mut ctx = Ctx {
            tree,
            data,
            assets,
            resolution,
            clock_ms: 0,
        };
        self.draw(&mut ctx, &mut scene)?;
        Ok(scene)
    }

    /// Adds the loading image to `scene` (section 4: background, strip,
    /// caption, picture).
    pub(crate) fn draw(&self, ctx: &mut Ctx, scene: &mut Scene) -> io::Result<()> {
        let scale = ctx.scale();
        let (w, h) = (ctx.resolution.width as f32, ctx.resolution.height as f32);
        let path = ctx
            .data
            .aliases
            .path("$game_textures$", &format!("{BACKGROUND}.dds"))?;
        let Ok((tw, th)) = ctx.assets.image_size(ctx.tree, &path) else {
            // Section 4.1: without the background's shader the screen is
            // cleared to black and nothing else is drawn. A missing or
            // unreadable texture counts as that here (our choice).
            scene.sprites.push(Sprite::picture(
                "loading black",
                TextureId::White,
                &quad(
                    [-0.5, -0.5, w - 0.5, h - 0.5],
                    (0.0, 0.0),
                    (1.0, 1.0),
                    Color::rgba(0, 0, 0, 255),
                ),
            ));
            return Ok(());
        };
        let (tw, th) = (tw as f32, th as f32);
        let texture = TextureId::Image { path };
        scene.sprites.push(Sprite::picture(
            "loading background",
            texture.clone(),
            &quad(
                background_rect(scale),
                (0.0, 0.0),
                (1024.0 / tw, 768.0 / th),
                Color::WHITE,
            ),
        ));

        // Section 4.2: 40 cells between 41 columns, alpha per column.
        let alpha = strip_alpha(self.captions, EXPECTED_CAPTIONS);
        let [left, top, right, bottom] = strip_rect(scale);
        // Columns from the unshifted edges, then the −0.5.
        let (x0, x1) = (left + 0.5, right + 0.5);
        let column = |i: usize| x0 + i as f32 * (x1 - x0) / 40.0 - 0.5;
        let u = |i: usize| i as f32 * 268.0 / 40.0 / tw;
        let (v0, v1) = (768.0 / th, 805.0 / th);
        for i in 0..STRIP_COLUMNS - 1 {
            let white = |a: u8| Color::rgba(255, 255, 255, a);
            let (a, b) = (white(alpha[i]), white(alpha[i + 1]));
            let mut cell = Sprite::picture(
                "loading strip",
                texture.clone(),
                &quad(
                    [column(i), top, column(i + 1), bottom],
                    (u(i), v0),
                    (u(i + 1), v1),
                    a,
                ),
            );
            cell.corner_colors = Some([a, b, b, a]);
            scene.sprites.push(cell);
        }

        // Section 4.4.
        ctx.line(
            scene,
            "loading caption",
            &self.caption,
            caption_pen(ctx.resolution),
            CAPTION_COLOR,
            HAlign::Centre,
            CAPTION_FONT,
        )?;

        // Section 4.5: the whole texture, shader `font` (its alpha test
        // changes nothing on these opaque pictures, section 8.2).
        if let Some(path) = &self.picture {
            scene.sprites.push(Sprite::picture(
                "loading picture",
                TextureId::Image { path: path.clone() },
                &quad(picture_rect(scale), (0.0, 0.0), (1.0, 1.0), Color::WHITE),
            ));
        }
        Ok(())
    }
}

/// A New Game loading through its stages, one per engine frame (see the
/// module docs).
pub struct Loading {
    stages: &'static [Stage],
    /// The index in `stages` of the caption shown.
    current: usize,
    screen: LoadingScreen,
    load: NewGameLoad,
}

impl Loading {
    /// Begins the first caption.
    pub fn start(data: &GameData, game_type_changed: bool) -> Self {
        let stages = stages(game_type_changed);
        let mut screen = LoadingScreen::default();
        screen.begin(data, stages[0]);
        Self {
            stages,
            current: 0,
            screen,
            load: NewGameLoad::default(),
        }
    }

    pub fn screen(&self) -> &LoadingScreen {
        &self.screen
    }

    /// The caption shown.
    pub fn stage(&self) -> Stage {
        self.stages[self.current]
    }

    /// Runs the work of the caption shown, then begins the next; after the
    /// last returns the level, shown from engine clock `now`.
    pub fn step(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        now: u64,
    ) -> io::Result<Option<LevelScreen>> {
        let stage = self.stage();
        self.load.run(stage, tree, data)?;
        if stage == Stage::CreatingNewGame
            && let Some(level) = self.load.level_name()
        {
            // Section 4.5: the level is determined here.
            self.screen.set_level(tree, data, level)?;
        }
        if self.current + 1 < self.stages.len() {
            self.current += 1;
            self.screen.begin(data, self.stages[self.current]);
            return Ok(None);
        }
        std::mem::take(&mut self.load).finish(now).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn a_first_new_game_shows_seventeen_captions_a_later_one_sixteen() {
        assert_eq!(stages(true).len(), 17);
        assert_eq!(stages(true)[0].caption_id(), "st_prefetching_objects");
        assert_eq!(stages(false).len(), 16);
        assert_eq!(stages(false)[0], Stage::ServerStarting);
        let ids: Vec<_> = stages(true).iter().map(|s| s.caption_id()).collect();
        // Section 5's table, in order.
        assert_eq!(
            ids,
            [
                "st_prefetching_objects",
                "st_server_starting",
                "st_loading_alife_simulator",
                "st_creating_new_game",
                "st_server_connecting",
                "st_client_connecting_to",
                "st_opening_stream",
                "st_loading_cform",
                "st_loading_shaders",
                "st_loading_geometry",
                "st_loading_spatial_db",
                "st_loading_details",
                "st_loading_sectors_portals",
                "st_loading_ai_objects",
                "st_client_spawning",
                "st_loading_textures",
                "st_client_synchronising",
            ]
        );
    }

    #[test]
    fn the_intro_is_due_once_at_most_two_overlay_frames_remain() {
        let due: Vec<u32> = (1..=OVERLAY_FRAMES)
            .filter(|&n| intro_due(true, n, SERVER_NEW_GAME))
            .collect();
        assert_eq!(due, [1, 2]);
        assert!(!intro_due(false, 2, "new"), "no level loaded");
        assert!(intro_due(true, 2, "NEW"), "case-insensitive");
        assert!(!intro_due(true, 2, "load"));
    }

    #[test]
    fn strip_alpha_matches_the_verification_table() {
        // Section 10.3.
        assert_eq!(
            strip_alpha(0, 14)[..21],
            [
                171, 141, 109, 79, 55, 36, 23, 14, 9, 5, 3, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
        assert_eq!(
            strip_alpha(10, 14)[..21],
            [
                254, 254, 254, 254, 254, 253, 253, 251, 249, 246, 241, 232, 220, 202, 179, 150,
                118, 87, 61, 41, 26
            ]
        );
        let a = strip_alpha(7, 14);
        assert!((0..41).all(|i| a[i] == a[40 - i]), "mirrored");
        let sums: Vec<u32> = (0..=17)
            .map(|n| strip_alpha(n, 14).iter().map(|&a| u32::from(a)).sum())
            .collect();
        assert_eq!(
            sums,
            [
                1296, 1866, 2510, 3190, 3898, 4612, 5333, 6053, 6764, 7465, 8132, 8753, 9289, 9715,
                10017, 10203, 10312, 10365
            ]
        );
        let sum = |n| {
            strip_alpha(n, 17)
                .iter()
                .map(|&a| u32::from(a))
                .sum::<u32>()
        };
        assert_eq!((sum(1), sum(17)), (1656, 9971));
    }

    #[test]
    #[allow(clippy::excessive_precision)] // The spec's values as given.
    fn caption_pen_and_rectangles_match_the_verification_tables() {
        for (h, y) in [
            (600, 544.0),
            (720, 653.0),
            (768, 696.0),
            (900, 816.0),
            (1024, 929.0),
            (1080, 980.0),
            (1200, 1089.0),
        ] {
            assert_eq!(caption_pen(Resolution::new(1001, h)), (500.0, y), "{h}");
        }
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
        // Section 10.1 (background, strip, picture).
        for (r, bg, strip, pic) in [
            (
                (800, 600),
                [-0.390625, -0.390625, 799.609375, 599.609375],
                [295.59375, 566.6875, 504.96875, 595.59375],
                [200.390625, 287.890625, 600.390625, 487.890625],
            ),
            (
                (1280, 1024),
                [-0.625, -0.6667, 1279.375, 1023.3333],
                [473.25, 967.5, 808.25, 1016.8333],
                [320.625, 491.3333, 960.625, 832.6667],
            ),
            (
                (1920, 1080),
                [-0.9375, -0.703125, 1919.0625, 1079.296875],
                [710.125, 1020.4375, 1212.625, 1072.46875],
                [480.9375, 518.203125, 1440.9375, 878.203125],
            ),
        ] {
            let s = Resolution::new(r.0, r.1).scale();
            assert!(
                close(background_rect(s), bg),
                "{r:?} {:?}",
                background_rect(s)
            );
            assert!(close(strip_rect(s), strip), "{r:?} {:?}", strip_rect(s));
            assert!(close(picture_rect(s), pic), "{r:?} {:?}", picture_rect(s));
        }
    }

    #[test]
    fn captions_are_translations_with_three_full_stops() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let text = |s| cp1251_text(&caption_text(&data.strings, s));
        assert_eq!(text(Stage::ServerStarting), "Сервер: Старт...");
        assert_eq!(
            text(Stage::ClientConnectingTo),
            "Клиент: Соединение с localhost..."
        );
        // A missing key shows the id itself.
        assert_eq!(text(Stage::LoadingCform), "st_loading_cform...");
    }

    fn cp1251_text(bytes: &[u8]) -> String {
        volya_engine_formats::cp1251::decode(bytes)
    }

    #[test]
    fn the_image_has_background_strip_caption_then_picture() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut assets = fixture::assets();
        let mut loading = Loading::start(&data, true);
        let scene = |l: &Loading, assets: &mut Assets| {
            l.screen()
                .scene(&tree, &data, assets, Resolution::new(1024, 768))
                .unwrap()
        };
        let s = scene(&loading, &mut assets);
        let bg = s.first("loading background").unwrap();
        assert_eq!(bg.bounds(), [-0.5, -0.5, 1023.5, 767.5]);
        assert_eq!((bg.uv_min, bg.uv_max), ([0.0, 0.0], [1.0, 0.75]));
        let cells: Vec<_> = s.layer("loading strip").collect();
        assert_eq!(cells.len(), 40);
        assert_eq!(
            s.layer_bounds("loading strip").unwrap(),
            [378.5, 725.5, 646.5, 762.5]
        );
        assert!(
            (cells[1].corners[0][0] - 385.2).abs() < 1e-3,
            "column step 6.7"
        );
        assert_eq!(cells[0].uv_min, [0.0, 0.75]);
        assert!((cells[39].uv_max[0] - 268.0 / 1024.0).abs() < 1e-6);
        assert!((cells[0].uv_max[1] - 805.0 / 1024.0).abs() < 1e-6);
        // n = 1: the first column's alpha from the n = 1 row.
        let alpha = strip_alpha(1, 14);
        let corners = cells[0].corner_colors.unwrap().map(|c| c.a);
        assert_eq!(corners, [alpha[0], alpha[1], alpha[1], alpha[0]]);
        assert_eq!(alpha[0], 205);
        // The caption: centred at the pen, glyph tops at y.
        let caption = s.layer_bounds("loading caption").unwrap();
        let width = "Кэширование объектов...".chars().count() as f32 * fixture::GLYPH as f32;
        assert_eq!(caption[0], 512.0 - (width / 2.0).floor());
        assert_eq!(caption[1], 696.0);
        assert_eq!(s.first("loading caption").unwrap().color, CAPTION_COLOR);
        assert!(s.first("loading picture").is_none(), "level not known yet");
        let layers: Vec<&str> = s.sprites.iter().map(|s| s.layer.as_str()).collect();
        assert_eq!(layers[0], "loading background");
        assert_eq!(layers[1..41], ["loading strip"; 40]);
        assert_eq!(*layers.last().unwrap(), "loading caption");

        // The fixture has no spawn file: creating the new game fails there.
        for stage in &Stage::ALL[1..4] {
            assert!(loading.step(&tree, &data, 0).unwrap().is_none());
            assert_eq!(loading.stage(), *stage);
        }
        assert_eq!(loading.screen().state().captions, 4);
        let e = loading.step(&tree, &data, 0).err().unwrap();
        assert!(e.to_string().contains("spawn"), "{e}");
    }

    #[test]
    fn the_picture_is_the_levels_or_the_fallback() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut assets = fixture::assets();
        let mut screen = LoadingScreen::default();
        screen.begin(&data, Stage::ServerConnecting);
        screen.set_level(&tree, &data, "l01_escape").unwrap();
        assert_eq!(
            screen.state().picture.unwrap(),
            "gamedata\\textures\\intro\\intro_no_start_picture.dds"
        );
        screen.set_level(&tree, &data, "l00_fixture").unwrap();
        assert_eq!(
            screen.state().picture.unwrap(),
            "gamedata\\textures\\intro\\intro_l00_fixture.dds"
        );
        let s = screen
            .scene(&tree, &data, &mut assets, Resolution::new(1024, 768))
            .unwrap();
        let picture = s.sprites.last().unwrap();
        assert_eq!(picture.layer, "loading picture");
        assert_eq!(picture.bounds(), [256.5, 368.5, 768.5, 624.5]);
        assert_eq!((picture.uv_min, picture.uv_max), ([0.0, 0.0], [1.0, 1.0]));
    }

    #[test]
    fn without_the_background_the_screen_is_black() {
        let tree = fixture::MemVfs::default();
        let fixture_tree = fixture::tree();
        let data = fixture::data(&fixture_tree);
        let mut screen = LoadingScreen::default();
        screen.begin(&data, Stage::ServerStarting);
        let s = screen
            .scene(
                &tree,
                &data,
                &mut Assets::default(),
                Resolution::new(64, 48),
            )
            .unwrap();
        assert_eq!(s.sprites.len(), 1);
        assert_eq!(s.sprites[0].layer, "loading black");
        assert_eq!(s.sprites[0].bounds(), [-0.5, -0.5, 63.5, 47.5]);
    }
}
