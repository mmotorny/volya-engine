// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game data the startup sequence and the title menu need, loaded from
//! the game tree during the loading period (docs/behavior/startup.md
//! section 4), and the per-resolution caches scene building uses (texture
//! sizes, fonts).

use std::collections::HashMap;
use std::io::{self, Cursor};
use std::sync::{Arc, OnceLock};

use crate::controls::{Bindings, Controls};
use crate::core::config::{self, Config, PathAliases};
use crate::core::console::{Console, ConsoleFiles, PRESET_FILES};
use crate::core::info_portions::Declarations;
use crate::core::input::Key;
use crate::core::movement::Settings;
use crate::core::text::{self, Fonts, Localization, StringTable};
use crate::core::ui::{self, TextureSource};
use crate::core::vfs::{self, Vfs};
use crate::scene::TextureId;
use crate::startup::Sequence;
use volya_engine_formats::audio::{VorbisStream, ogg_end_granule};
use volya_engine_formats::cp1251;
use volya_engine_formats::dds::Dds;
use volya_engine_formats::particles::Library;
use volya_engine_formats::ui::layout::{StaticDesc, TextureRef};
use volya_engine_formats::ui::movies::{MovieItemKind, MovieSequence};
use volya_engine_formats::ui::picture::Picture;
use volya_engine_formats::ui::texture_desc::TextureDescriptions;
use volya_engine_formats::ui::title_menu::TitleMenuLayout;
use volya_engine_formats::ui::{ColorDefs, Resolution};
use volya_engine_formats::video::TheoraVideo;

/// The path configuration this edition's installer writes as `fsgame.ltx`
/// (docs/formats/ltx-config.md section 10). The installer generates it at
/// install time, so the imported files lack it; it is used when the game
/// root has no `fsgame.ltx`. `{userdocs}` stays unreplaced: the user
/// folder has no place on this platform and no data read here lives there.
pub const INSTALLER_FSGAME: &str = r"$app_data_root$ = true | false| {userdocs}\Stalker-SHOC
$game_data$ =     false| true | $fs_root$      | gamedata\
$game_ai$ =       true | false| $game_data$    | ai\
$game_spawn$ =    true | false| $game_data$    | spawns\
$game_anims$ =    true | true | $game_data$    | anims\
$game_levels$ =   true | false| $game_data$    | levels\
$game_meshes$ =   true | true | $game_data$    | meshes\
$game_dm$ =       true | true | $game_data$    | meshes\
$game_shaders$ =  true | true | $game_data$    | shaders\
$game_sounds$ =   true | true | $game_data$    | sounds\
$game_textures$ = true | true | $game_data$    | textures\
$game_scripts$ =  true | false| $game_data$    | scripts\
$game_config$ =   true | false| $game_data$    | config\
$level$ =         false| false| $game_levels$
$game_saves$ =    true | false| $app_data_root$| savedgames\
$logs$ =          true | false| $app_data_root$| logs\
$screenshots$ =   true | false| $app_data_root$| screenshots\
";

/// The default user settings the installer copies into the user folder
/// (startup.md section 4), relative to the game root. This platform has no
/// user folder, so the installer's copy is read in its place.
pub const DEFAULT_USER_LTX: &str = "__support/userdocs/user.ltx";

/// The game's version string, which the title menu's caption shows
/// whatever the registry holds (title-menu.md section 3.4).
pub const BUILT_IN_VERSION: &str = "1.0006";

const GAME_CONFIG: &str = "$game_config$";
const GAME_SOUNDS: &str = "$game_sounds$";
const GAME_TEXTURES: &str = "$game_textures$";

fn context(what: &str, err: io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{what}: {err}"))
}

/// The message the game stops with when a root configuration is missing
/// (startup.md section 4).
fn cannot_find(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("Cannot find file {path}.\nReinstalling application may fix this problem."),
    )
}

/// Keys of the effective bindings of the `quit` action (startup.md
/// section 4): `bind quit <key>` lines of `config\default_controls.ltx`,
/// then of the user settings on top (a later `bind` of the action replaces
/// the earlier). A binding to a key this engine does not receive (a mouse
/// button, a key no keyboard reports) leaves no usable key, and then Escape
/// is kept so the sequence stays skippable (our choice). Other console
/// commands (`bind_sec`, `unbind`) are not covered by the spec and are
/// ignored.
pub fn quit_keys(files: &[&[u8]]) -> Vec<Key> {
    let mut bound = None;
    for file in files {
        for line in cp1251::decode(file).lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if let ["bind", "quit", key, ..] = words.as_slice() {
                bound = Some((*key).to_owned());
            }
        }
    }
    let key = bound.and_then(|b| Key::from_dik_name(&b));
    vec![key.unwrap_or(Key::Escape)]
}

/// The quit keys and the player's controls of `console`'s settings: read
/// from the file it saves, whose `unbindall` and `bind` lines give the
/// effective bindings (player-movement.md sections 1.2 and 2.1).
fn derived(console: &Console) -> (Vec<Key>, Controls) {
    let saved = console.save();
    (
        quit_keys(&[&saved]),
        Controls::new(Bindings::read(&[&saved]), Settings::read(&[&saved])),
    )
}

/// The path aliases of the game root's `fsgame.ltx`, or of the installer's
/// when the root has none.
pub fn read_aliases(root: Option<&dyn Vfs>) -> io::Result<PathAliases> {
    let fsgame = root
        .and_then(|r| vfs::read(r, "fsgame.ltx").ok())
        .unwrap_or_else(|| INSTALLER_FSGAME.as_bytes().into());
    PathAliases::parse(&fsgame, "")
}

/// Everything loaded before the logo sequence starts.
pub struct GameData {
    pub aliases: PathAliases,
    pub system: Config,
    pub descriptions: TextureDescriptions,
    pub colors: ColorDefs,
    pub localization: Localization,
    pub strings: StringTable,
    /// The particles' effect library `particles.xr`, the same for every
    /// level: read once here rather than with each level (our choice;
    /// docs/architecture/memory.md), empty when the file is absent.
    pub effects: Arc<Library>,
    pub menu: TitleMenuLayout,
    pub intro: MovieSequence,
    /// Per intro item, how long it lasts in seconds after its delay
    /// (startup.md sections 6.3 and 6.4): an image item its `length_sec`; a
    /// video item the length of its left sound, or without a sound its
    /// video's.
    pub intro_lengths: Vec<f64>,
    /// The New Game intro `intro_game` of the same file
    /// (loading-screen.md section 7), when the file defines it: a New Game
    /// without it fails when it reaches the intro.
    pub intro_game: Option<MovieSequence>,
    /// Per `intro_game` item, as [`GameData::intro_lengths`].
    pub intro_game_lengths: Vec<f64>,
    pub quit_keys: Vec<Key>,
    /// The player's bindings and the console settings movement reads.
    pub controls: Controls,
    /// The game files the console runs (options-screens.md sections 8 and
    /// 11.5).
    pub console_files: ConsoleFiles,
    /// The user configuration file read at startup (section 12.2): Volya
    /// Engine's own copy when the platform has one
    /// ([`GameData::set_user_config`]), else the installer's
    /// [`DEFAULT_USER_LTX`].
    pub user_config: Vec<u8>,
    /// `textures\ui\ui_ani_cursor`: the frames of the menu cursor.
    pub cursor: volya_engine_formats::ui::seq::Sequence,
    /// The information portions, articles and dialogs declared
    /// ([`GameData::info_portions`]).
    info_portions: OnceLock<Arc<Declarations>>,
}

impl GameData {
    /// Loads from the combined archive tree `tree` and the game root `root`
    /// (loose files such as `fsgame.ltx` and the default user settings;
    /// `None` when there are none), at output `resolution` (the `_16` rule).
    /// Everything the startup sequence and the title menu draw or play is
    /// checked to exist, so a partial import fails here, naming the file.
    pub fn load(
        tree: &dyn Vfs,
        root: Option<&dyn Vfs>,
        resolution: Resolution,
    ) -> io::Result<Self> {
        let mut data = Self::read(tree, root, resolution)?;
        data.measure_intro(tree)?;
        data.check_media(tree)?;
        Ok(data)
    }

    /// The configuration and layout part of [`GameData::load`]: no media
    /// file is opened, and [`GameData::intro_lengths`] stays empty.
    pub fn read(
        tree: &dyn Vfs,
        root: Option<&dyn Vfs>,
        resolution: Resolution,
    ) -> io::Result<Self> {
        let root_file = |path: &str| root.and_then(|r| vfs::read(r, path).ok());
        let aliases = read_aliases(root)?;

        let required = |name: &str| -> io::Result<Config> {
            let path = aliases.path(GAME_CONFIG, name)?;
            let config = config::load(tree, &path)?;
            if config.is_empty() {
                return Err(cannot_find(&path));
            }
            Ok(config)
        };
        let system = required("system.ltx")?;
        required("game.ltx")?;

        let default_controls = vfs::read(tree, &aliases.path(GAME_CONFIG, "default_controls.ltx")?)
            .unwrap_or_default();
        let user = root_file(DEFAULT_USER_LTX).unwrap_or_default();
        let mut presets: [Vec<u8>; 5] = Default::default();
        for (script, file) in presets.iter_mut().zip(PRESET_FILES) {
            // A missing preset script does nothing (our choice).
            *script = vfs::read(
                tree,
                &aliases.path(GAME_CONFIG, &format!("rspec_{file}.ltx"))?,
            )
            .unwrap_or_default();
        }
        let console_files = ConsoleFiles {
            default_controls,
            presets,
        };
        let (quit_keys, controls) = derived(&Console::startup(console_files.clone(), &user));

        let descriptions = ui::load_texture_descriptions(tree, &aliases, &system, resolution)?;
        let colors = ui::load_color_defs(tree, &aliases, resolution)?;
        let localization = text::load_localization(tree, &aliases)?;
        let strings = text::load_string_table(tree, &aliases, &localization)?;
        let menu = ui::load_title_menu(tree, &aliases, &colors, resolution)?;
        let path = aliases.path("$game_data$", "particles.xr")?;
        let effects = Arc::new(match vfs::read(tree, &path) {
            Ok(bytes) => Library::parse(&bytes)
                .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Library::default(),
            Err(e) => return Err(e),
        });

        // startup.md section 6.1: `intro_logo`, reached from
        // `game_tutorials.xml` through its include of `ui_movies.xml`.
        let tutorials = ui::load_ui_file(tree, &aliases, "game_tutorials.xml", resolution)?;
        let intro = tutorials.find("intro_logo", 0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "game_tutorials.xml: no intro_logo",
            )
        })?;
        let intro = MovieSequence::read(intro, &colors).map_err(|e| context("intro_logo", e))?;
        // A New Game without it fails when it reaches the intro
        // (loading-screen.md 7; `App`'s intro).
        // TODO(#330): its files are measured here, at data load; the specs
        // do not say when the original opens them.
        let intro_game = tutorials
            .find("intro_game", 0)
            .map(|e| MovieSequence::read(e, &colors).map_err(|e| context("intro_game", e)))
            .transpose()?;

        let cursor = match ui::texture_source(tree, &aliases, "ui\\ui_ani_cursor")? {
            TextureSource::Sequence(_, seq) => seq,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("ui\\ui_ani_cursor is not a frame sequence: {other:?}"),
                ));
            }
        };

        Ok(Self {
            aliases,
            system,
            descriptions,
            colors,
            localization,
            strings,
            effects,
            menu,
            intro,
            intro_lengths: Vec::new(),
            intro_game,
            intro_game_lengths: Vec::new(),
            quit_keys,
            controls,
            console_files,
            user_config: user,
            cursor,
            info_portions: OnceLock::new(),
        })
    }

    /// The information portions, articles and dialogs declared
    /// (info-portions.md 1, dialog-files.md 1): read from `tree` the first
    /// time a game needs them, then kept for the run. The original builds
    /// the index once at its game data's initialisation, a moment the spec
    /// leaves unestablished (1.2); reading it with the first New Game keeps
    /// it off the startup (our choice).
    pub fn info_portions(&self, tree: &dyn Vfs) -> io::Result<Arc<Declarations>> {
        if let Some(d) = self.info_portions.get() {
            return Ok(Arc::clone(d));
        }
        let d = Arc::new(Declarations::read(tree, &self.aliases, &self.system)?);
        Ok(Arc::clone(self.info_portions.get_or_init(|| d)))
    }

    /// Replaces the user configuration file read at startup, and what it
    /// sets.
    pub fn set_user_config(&mut self, user: Vec<u8>) {
        self.user_config = user;
        self.apply(&self.console());
    }

    /// The console as the game starts (section 12.2).
    pub fn console(&self) -> Console {
        Console::startup(self.console_files.clone(), &self.user_config)
    }

    /// Takes the bindings and settings of `console`: what the next level
    /// and the menus use.
    pub fn apply(&mut self, console: &Console) {
        (self.quit_keys, self.controls) = derived(console);
    }

    /// The logo sequence with its lengths.
    pub fn intro_logo(&self) -> Sequence<'_> {
        Sequence {
            movies: &self.intro,
            lengths: &self.intro_lengths,
        }
    }

    /// The New Game intro with its lengths, once measured.
    pub fn intro_game(&self) -> Option<Sequence<'_>> {
        let movies = self.intro_game.as_ref()?;
        (movies.items.len() == self.intro_game_lengths.len()).then_some(Sequence {
            movies,
            lengths: &self.intro_game_lengths,
        })
    }

    /// Measures [`GameData::intro_lengths`] and
    /// [`GameData::intro_game_lengths`]; the second file of each sound pair
    /// must exist too.
    pub fn measure_intro(&mut self, tree: &dyn Vfs) -> io::Result<()> {
        self.intro_lengths = self.lengths(tree, &self.intro)?;
        if let Some(game) = &self.intro_game {
            self.intro_game_lengths = self
                .lengths(tree, game)
                .map_err(|e| context("intro_game", e))?;
        }
        Ok(())
    }

    /// The length of each item of `sequence` in seconds after its delay.
    fn lengths(&self, tree: &dyn Vfs, sequence: &MovieSequence) -> io::Result<Vec<f64>> {
        let data = self;
        sequence
            .items
            .iter()
            .map(|item| match &item.kind {
                MovieItemKind::Video {
                    sound: Some(sound), ..
                } => data.sound_files(sound).and_then(|[left, right]| {
                    vfs::read(tree, &right).map_err(|e| context(&right, e))?;
                    sound_length(tree, &left)
                }),
                MovieItemKind::Video { texture, .. } => {
                    let path = data
                        .aliases
                        .path(GAME_TEXTURES, &format!("{}.ogm", texture.name))?;
                    video_length(tree, &path)
                }
                MovieItemKind::Image { length_sec, .. } => Ok(f64::from(*length_sec)),
            })
            .collect()
    }

    /// The pair of files of sound `sound` (startup.md section 6.2): `_l` and
    /// `_r` under `sounds\`.
    pub fn sound_files(&self, sound: &str) -> io::Result<[String; 2]> {
        Ok([
            self.aliases.path(GAME_SOUNDS, &format!("{sound}_l.ogg"))?,
            self.aliases.path(GAME_SOUNDS, &format!("{sound}_r.ogg"))?,
        ])
    }

    /// Every picture the intros and the menu draw resolves to an existing
    /// texture, and the menu's music files exist.
    pub fn check_media(&self, tree: &dyn Vfs) -> io::Result<()> {
        let mut statics: Vec<&StaticDesc> = Vec::new();
        let mut refs: Vec<&TextureRef> = Vec::new();
        for sequence in std::iter::once(&self.intro).chain(&self.intro_game) {
            if let Some(global) = &sequence.global_wnd {
                statics.extend(&global.window.auto_statics);
            }
            for item in &sequence.items {
                match &item.kind {
                    MovieItemKind::Video { texture, .. } => refs.push(texture),
                    MovieItemKind::Image { pictures, .. } => {
                        statics.extend(pictures.iter().map(|p| &p.desc))
                    }
                }
            }
        }
        let m = &self.menu;
        statics.extend([
            &m.back_movie,
            &m.background,
            &m.fire_movie,
            &m.shniaga,
            &m.left_grating,
            &m.right_grating,
            &m.left_anim,
            &m.right_anim,
        ]);
        refs.extend(statics.iter().filter_map(|s| s.texture.as_ref()));
        for r in refs {
            let picture = Picture::resolve(r, &self.descriptions);
            ui::texture_source(tree, &self.aliases, &picture.texture)
                .map_err(|e| context(&format!("texture {}", r.name), e))?;
        }
        for frame in &self.cursor.frames {
            ui::still_image(tree, &self.aliases, frame)?;
        }
        for track in &self.menu.menu_sound.music {
            for file in crate::core::audio::music_files(track) {
                let path = self.aliases.path("$game_data$", &file)?;
                tree.open(&path).map_err(|e| context(&path, e))?;
            }
        }
        Ok(())
    }

    /// The text of string-table key `key` as the font's bytes
    /// (text-and-fonts.md section 1.5); characters outside Windows-1251,
    /// which the tables of this edition do not contain, become `?`.
    pub fn text(&self, key: &str) -> Vec<u8> {
        encode(self.strings.get(key))
    }
}

/// `text` as Windows-1251 bytes, `?` for unencodable characters.
pub fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| cp1251::encode_char(c).unwrap_or(b'?'))
        .collect()
}

/// The length in seconds of the Ogg Vorbis file at `path`: its end granule
/// over its sample rate.
pub fn sound_length(tree: &dyn Vfs, path: &str) -> io::Result<f64> {
    let bytes = vfs::read(tree, path).map_err(|e| context(path, e))?;
    let granule = ogg_end_granule(&bytes).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, format!("{path}: no Ogg page"))
    })?;
    let rate = VorbisStream::open(Cursor::new(&bytes))
        .map_err(|e| context(path, e))?
        .sample_rate();
    Ok(granule as f64 / f64::from(rate.max(1)))
}

/// The length in seconds of the video at `path`, counting its frames by
/// decoding them.
pub fn video_length(tree: &dyn Vfs, path: &str) -> io::Result<f64> {
    // Streamed from the file: videos can be large.
    let file = tree.open(path).map_err(|e| context(path, e))?;
    let mut video = TheoraVideo::open(file).map_err(|e| context(path, e))?;
    let info = video.info().clone();
    let mut frames = 0u64;
    while video.next_frame()?.is_some() {
        frames += 1;
    }
    Ok(frames as f64 * f64::from(info.fps_denominator) / f64::from(info.fps_numerator.max(1)))
}

/// A video's picture size and frame rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoMeta {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

impl VideoMeta {
    /// The texture's size: the next power of two in each dimension
    /// (title-menu.md section 3.2).
    pub fn texture_size(&self) -> (u32, u32) {
        (
            self.width.next_power_of_two(),
            self.height.next_power_of_two(),
        )
    }
}

/// A picture's texture as drawn.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedTexture {
    Image { path: String, size: (u32, u32) },
    Video { path: String, meta: VideoMeta },
}

impl ResolvedTexture {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Self::Image { size, .. } => *size,
            Self::Video { meta, .. } => meta.texture_size(),
        }
    }

    /// The sprite texture; videos start at `start_ms`.
    pub fn id(&self, start_ms: u64, looping: bool) -> TextureId {
        match self {
            Self::Image { path, .. } => TextureId::Image { path: path.clone() },
            Self::Video { path, .. } => TextureId::Video {
                path: path.clone(),
                start_ms,
                looping,
            },
        }
    }
}

/// Caches of what scene building reads from the game tree: texture kinds and
/// sizes, video headers, fonts per output height.
#[derive(Default)]
pub struct Assets {
    textures: HashMap<String, ResolvedTexture>,
    images: HashMap<String, (u32, u32)>,
    fonts: HashMap<u32, Fonts>,
}

impl Assets {
    /// Caches what texture path `path` resolves to.
    #[cfg(test)]
    pub fn insert(&mut self, path: &str, texture: ResolvedTexture) {
        self.textures.insert(path.to_owned(), texture);
    }

    /// The texture of texture path `path` (ui-layout.md section 6.3). A
    /// sequence resolves to its frame at `clock_ms`.
    pub fn texture(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        path: &str,
        clock_ms: u64,
    ) -> io::Result<ResolvedTexture> {
        if let Some(t) = self.textures.get(path) {
            return Ok(t.clone());
        }
        let resolved = match ui::texture_source(tree, &data.aliases, path)? {
            TextureSource::Video(file) => ResolvedTexture::Video {
                meta: video_meta(tree, &file)?,
                path: file,
            },
            TextureSource::Image(file) => ResolvedTexture::Image {
                size: self.image_size(tree, &file)?,
                path: file,
            },
            TextureSource::Sequence(_, seq) => {
                // Not cached: the frame changes with the clock.
                let file = ui::still_image(tree, &data.aliases, seq.frame(clock_ms))?;
                let size = self.image_size(tree, &file)?;
                return Ok(ResolvedTexture::Image { path: file, size });
            }
            TextureSource::Avi(file) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("{file}: AVI videos are not supported"),
                ));
            }
        };
        self.textures.insert(path.to_owned(), resolved.clone());
        Ok(resolved)
    }

    /// The size of the `.dds` file at `file`.
    pub fn image_size(&mut self, tree: &dyn Vfs, file: &str) -> io::Result<(u32, u32)> {
        if let Some(&size) = self.images.get(file) {
            return Ok(size);
        }
        let bytes = vfs::read(tree, file).map_err(|e| context(file, e))?;
        let dds = Dds::parse(&bytes).map_err(|e| context(file, e))?;
        let size = (dds.width, dds.height);
        self.images.insert(file.to_owned(), size);
        Ok(size)
    }

    /// The UI fonts for output height `height` (text-and-fonts.md 2.3).
    pub fn fonts(&mut self, tree: &dyn Vfs, data: &GameData, height: u32) -> io::Result<&Fonts> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.fonts.entry(height) {
            e.insert(Fonts::load(
                tree,
                &data.aliases,
                &data.localization,
                height,
            )?);
        }
        Ok(&self.fonts[&height])
    }

    /// The `.dds` path of a font texture name (relative to `textures\`).
    pub fn font_texture(data: &GameData, texture: &str) -> io::Result<String> {
        data.aliases.path(GAME_TEXTURES, &format!("{texture}.dds"))
    }
}

/// Reads the headers of the video at `file`.
pub fn video_meta(tree: &dyn Vfs, file: &str) -> io::Result<VideoMeta> {
    // Only the headers are read.
    let reader = tree.open(file).map_err(|e| context(file, e))?;
    let video = TheoraVideo::open(reader).map_err(|e| context(file, e))?;
    let info = video.info();
    Ok(VideoMeta {
        width: info.width,
        height: info.height,
        fps: f64::from(info.fps_numerator) / f64::from(info.fps_denominator.max(1)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_is_escape() {
        assert_eq!(
            quit_keys(&[b"bind quit kESCAPE\r\nbind left kA\r\n"]),
            [Key::Escape]
        );
        assert_eq!(quit_keys(&[]), [Key::Escape]);
        assert_eq!(
            quit_keys(&[b"bind quit kESCAPE\n", b"bind quit kF10\n"]),
            [Key::F10]
        );
        // A key this engine does not receive: Escape is kept.
        assert_eq!(
            quit_keys(&[b"bind quit kESCAPE\n", b"bind quit mouse1\n"]),
            [Key::Escape]
        );
    }

    #[test]
    fn installer_fsgame_parses() {
        let a = PathAliases::parse(INSTALLER_FSGAME.as_bytes(), "").unwrap();
        assert_eq!(a.dir("$game_config$").unwrap(), "gamedata\\config\\");
        assert_eq!(a.dir("$game_sounds$").unwrap(), "gamedata\\sounds\\");
    }
}
