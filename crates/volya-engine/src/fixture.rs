// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Test fixture: a synthetic game tree holding what the startup sequence and
//! the title menu read, with the layout values the specs give
//! (docs/behavior/title-menu.md sections 3–6, docs/behavior/startup.md 6.1)
//! and blank textures of the stated sizes. Videos and sounds are placeholders:
//! their headers are given to the caches directly.

use std::collections::HashMap;
use std::io::{self, Cursor};

use crate::audio::{AudioOut, Voice};
use crate::core::audio::Playback;
use crate::core::vfs::{self, DirEntry, Vfs, VfsFile};
use crate::data::{GameData, ResolvedTexture, VideoMeta};
use volya_engine_formats::cp1251;
use volya_engine_formats::dds::build::{PixelFormat, file};
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::ui::Resolution;

#[derive(Default)]
pub struct MemVfs(HashMap<String, Vec<u8>>);

struct MemFile(Cursor<Vec<u8>>);

impl io::Read for MemFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Seek for MemFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl VfsFile for MemFile {
    fn size(&self) -> u64 {
        self.0.get_ref().len() as u64
    }
}

impl Vfs for MemVfs {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let key = vfs::normalize(path)?.to_ascii_lowercase();
        match self.0.get(&key) {
            Some(data) => Ok(Box::new(MemFile(Cursor::new(data.clone())))),
            None => Err(io::Error::new(io::ErrorKind::NotFound, key)),
        }
    }

    fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
        Err(io::Error::other("not listed"))
    }
}

fn dds(width: u32, height: u32) -> Vec<u8> {
    file(
        width,
        height,
        0,
        false,
        PixelFormat::FourCc(b"DXT5"),
        &vec![0; (width * height) as usize],
    )
}

const LAYOUT: &str = r#"<window>
<back_movie x="148" y="0" width="876" height="400"><texture x="0" y="0" width="876" height="400">ui\ui_mm_main_back_crop</texture></back_movie>
<background x="0" y="0" width="1024" height="768"><texture>ui_menu_backgraund</texture></background>
<fire_movie x="525" y="459" width="128" height="256"><texture>ui\ui_mm_fire</texture></fire_movie>
<shniaga_wnd x="590" y="419" width="392" height="280">
  <buttons_region x="110" y="0" width="164" height="280" always_show_scroll="false"/>
  <shniaga x="9" y="0" width="392" height="73"><texture>ui_menu_move</texture>
    <magnifire x="76" y="22" width="232" height="36" stretch="1"><texture>ui\ui_magnifier</texture><y_offset>0</y_offset></magnifire>
    <left_grating x="0" y="4" width="55" height="67"><texture>ui_menu_grating_l</texture></left_grating>
    <right_grating x="337" y="4" width="55" height="67"><texture>ui_menu_grating_r</texture></right_grating>
    <left_anim x="43" y="37" width="55" height="55" alignment="c" stretch="1" heading="1"><texture>ui_menu_bigroll_l</texture></left_anim>
    <right_anim x="340" y="37" width="55" height="55" alignment="c" stretch="1" heading="1"><texture>ui_menu_bigroll_r</texture></right_anim>
  </shniaga>
</shniaga_wnd>
<static_version x="15" y="740" width="100" height="30"><text align="l" font="graffiti19" r="135" g="123" b="116" complex_mode="0"/></static_version>
<menu_main btn_height="30" font="graffiti22" a="255" r="0" g="0" b="0">
  <btn name="btn_new_game" caption="ui_mm_newgame"/><btn name="btn_load" caption="ui_mm_load_game"/>
  <btn name="btn_multiplayer" caption="ui_mm_multiplayer_n"/><btn name="btn_options" caption="ui_mm_options_n"/>
  <btn name="btn_credits" caption="ui_mm_credits"/><btn name="btn_quit" caption="ui_mm_quit2windows"/>
</menu_main>
<menu_new_game btn_height="30" font="graffiti22" a="255" r="0" g="0" b="0">
  <btn name="btn_novice" caption="ui_mm_novice"/><btn name="btn_stalker" caption="ui_mm_stalker"/>
  <btn name="btn_veteran" caption="ui_mm_veteran"/><btn name="btn_master" caption="ui_mm_master"/>
  <btn name="btn_new_back" caption="ui_mm_back"/>
</menu_new_game>
<menu_sound random="0"><menu_music>music\wasteland2</menu_music></menu_sound>
</window>"#;

const MOVIES: &str = r#"<intro_logo>
<global_wnd x="0" y="0" width="1024" height="768">
  <auto_static x="0" y="0" width="1024" height="768" stretch="1"><texture>intro\intro_back</texture></auto_static>
</global_wnd>
<play_each_item>1</play_each_item>
<item type="video"><delay>1</delay><sound>v\logo</sound>
  <video_wnd x="0" y="0" width="1024" height="768" stretch="1"><texture x="0" y="1" width="720" height="484">intro\logo</texture></video_wnd></item>
<item type="image"><length_sec>9</length_sec><main_wnd>
  <auto_static start_time="0" length_sec="4" x="0" y="0" width="1024" height="768" stretch="1" light_anim="asus_logo_01" la_alpha="1" la_texture="1"><texture>ui_asus_01</texture></auto_static>
  <auto_static start_time="4" length_sec="4" x="0" y="0" width="1024" height="768" stretch="1" light_anim="asus_logo_01" la_alpha="1" la_texture="1"><texture>ui_asus_02</texture></auto_static>
</main_wnd></item>
</intro_logo>
<intro_game>
<global_wnd x="0" y="0" width="1024" height="768">
  <auto_static x="0" y="0" width="1024" height="768" stretch="1"><texture>intro\intro_back</texture></auto_static>
</global_wnd>
<play_each_item>1</play_each_item>
<item type="video"><pause_state>on</pause_state><sound>v\intro</sound>
  <video_wnd x="0" y="0" width="1024" height="768" stretch="1"><texture x="0" y="0" width="512" height="288">intro\intro_stalker</texture></video_wnd></item>
</intro_game>"#;

const STRINGS: [(&str, &str); 14] = [
    ("st_prefetching_objects", "Кэширование объектов"),
    ("st_server_starting", "Сервер: Старт"),
    ("st_client_connecting_to", "Клиент: Соединение с"),
    ("ui_mm_newgame", "Новая игра"),
    ("ui_mm_load_game", "Загрузить игру"),
    ("ui_mm_multiplayer_n", "Сетевая игра"),
    ("ui_mm_options_n", "Опции"),
    ("ui_mm_credits", "Титры"),
    ("ui_mm_quit2windows", "Выйти в Windows"),
    ("ui_mm_novice", "Новичок"),
    ("ui_mm_stalker", "Сталкер"),
    ("ui_mm_veteran", "Ветеран"),
    ("ui_mm_master", "Мастер"),
    ("ui_mm_back", "Назад"),
];

/// Width of every printable glyph of the fixture's font.
pub const GLYPH: u32 = 8;
/// The fixture font's height.
pub const FONT_HEIGHT: u32 = 22;

pub fn tree() -> MemVfs {
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut add = |path: &str, data: Vec<u8>| {
        files.insert(path.to_ascii_lowercase(), data);
    };
    let c = "gamedata/config/";
    add(
        &format!("{c}system.ltx"),
        b"[texture_desc]\nfiles = ui_mainmenu, ui_asus_intro\n".into(),
    );
    add(&format!("{c}game.ltx"), b"[levels]\nx = 1\n".into());
    add(
        &format!("{c}default_controls.ltx"),
        b"bind quit kESCAPE\r\n".into(),
    );
    add(
        &format!("{c}ui/ui_mainmenu.xml"),
        br#"<ui_texture><file_name>ui\ui_mainMenu</file_name>
        <texture id="ui_menu_backgraund" x="0" y="0" width="1024" height="768"/>
        <texture id="ui_menu_move" x="7" y="951" width="392" height="73"/>
        <texture id="ui_menu_bigroll_l" x="723" y="878" width="86" height="86"/>
        <texture id="ui_menu_bigroll_r" x="635" y="878" width="86" height="86"/>
        <texture id="ui_menu_grating_l" x="865" y="878" width="46" height="70"/>
        <texture id="ui_menu_grating_r" x="810" y="878" width="55" height="67"/>
        </ui_texture>"#
            .into(),
    );
    add(
        &format!("{c}ui/ui_asus_intro.xml"),
        br#"<ui_texture><file_name>ui\ui_asus_intro</file_name>
        <texture id="ui_asus_01" x="0" y="0" width="512" height="512"/>
        <texture id="ui_asus_02" x="512" y="0" width="512" height="512"/></ui_texture>"#
            .into(),
    );
    add(&format!("{c}ui/color_defs.xml"), b"<colors/>".into());
    add(&format!("{c}ui/ui_mm_main.xml"), LAYOUT.into());
    add(
        &format!("{c}ui/game_tutorials.xml"),
        b"<window>\r\n#include \"ui\\ui_movies.xml\"\r\n</window>".into(),
    );
    add(&format!("{c}ui/ui_movies.xml"), MOVIES.into());
    add(
        &format!("{c}localization.ltx"),
        b"[string_table]\nlanguage = rus\nfiles = ui_st_mm\n".into(),
    );
    let mut table = b"<string_table>".to_vec();
    for (id, text) in STRINGS {
        table.extend(format!("<string id=\"{id}\"><text>").bytes());
        table.extend(cp1251::encode(text).unwrap());
        table.extend(b"</text></string>");
    }
    table.extend(b"</string_table>");
    add(&format!("{c}text/rus/ui_st_mm.xml"), table);
    let fonts: String = UiFont::ALL
        .iter()
        .map(|f| format!("[{}]\nshader = font\ntexture = ui\\font\n", f.section()))
        .collect();
    add(&format!("{c}fonts.ltx"), fonts.into());
    let mut ini = format!("[symbol_coords]\nheight = {FONT_HEIGHT}\n");
    for i in 0..256u32 {
        let (x, y) = ((i % 16) * 16, (i / 16) * 16);
        let w = if i < 32 { 0 } else { GLYPH };
        ini.push_str(&format!(
            "{i:03} = {x}, {y}, {}, {}\n",
            x + w,
            y + FONT_HEIGHT
        ));
    }
    let t = "gamedata/textures/";
    add(&format!("{t}ui/font.ini"), ini.into());
    add(&format!("{t}ui/font.dds"), dds(256, 256));
    add(&format!("{t}ui/ui_mainmenu.dds"), dds(1024, 1024));
    add(&format!("{t}ui/ui_asus_intro.dds"), dds(1024, 512));
    add(&format!("{t}intro/intro_back.dds"), dds(4, 4));
    // The loading screen (loading-screen.md sections 4.1, 4.5).
    add(&format!("{t}ui/ui_load.dds"), dds(1024, 1024));
    add(&format!("{t}intro/intro_l00_fixture.dds"), dds(512, 256));
    add(
        &format!("{t}intro/intro_no_start_picture.dds"),
        dds(512, 256),
    );
    let mut seq = "19\r\n".to_owned();
    for i in 1..=8 {
        seq.push_str(&format!("ui\\ui_ani_cursor_{i:02}\r\n"));
        add(&format!("{t}ui/ui_ani_cursor_{i:02}.dds"), dds(64, 64));
    }
    add(&format!("{t}ui/ui_ani_cursor.seq"), seq.into());
    for video in [
        "ui/ui_mm_main_back_crop.ogm",
        "ui/ui_mm_fire.ogm",
        "intro/logo.ogm",
        "intro/intro_stalker.ogm",
    ] {
        add(&format!("{t}{video}"), Vec::new());
    }
    for sound in [
        "music/wasteland2_l.ogg",
        "music/wasteland2_r.ogg",
        "v/logo_l.ogg",
        "v/logo_r.ogg",
        "v/intro_l.ogg",
        "v/intro_r.ogg",
    ] {
        add(&format!("gamedata/sounds/{sound}"), Vec::new());
    }
    MemVfs(files)
}

/// The game data of [`tree`], with the media lengths the placeholders
/// cannot give: the logo's sound lasts 2.5 s, the intro's 3 s.
pub fn data(tree: &MemVfs) -> GameData {
    let mut data = GameData::read(tree, None, Resolution::new(1024, 768)).unwrap();
    data.intro_lengths = vec![2.5, 9.0];
    data.intro_game_lengths = vec![3.0];
    data.check_media(tree).unwrap();
    data
}

/// Caches holding the fixture's video headers.
pub fn assets() -> crate::data::Assets {
    let mut assets = crate::data::Assets::default();
    for (path, texture) in videos() {
        assets.insert(path, texture);
    }
    assets
}

/// The video headers of the fixture.
pub fn videos() -> Vec<(&'static str, ResolvedTexture)> {
    let video = |path: &str, width, height, fps| ResolvedTexture::Video {
        path: format!("gamedata\\textures\\{path}.ogm"),
        meta: VideoMeta { width, height, fps },
    };
    vec![
        (
            "ui\\ui_mm_main_back_crop",
            video("ui\\ui_mm_main_back_crop", 876, 400, 24.0),
        ),
        ("ui\\ui_mm_fire", video("ui\\ui_mm_fire", 128, 256, 33.0)),
        ("intro\\logo", video("intro\\logo", 720, 486, 29.97)),
        (
            "intro\\intro_stalker",
            video("intro\\intro_stalker", 512, 288, 30.0),
        ),
    ]
}

/// Audio that records what plays; voices play until stopped or ended.
#[derive(Default)]
pub struct FakeAudio {
    pub started: Vec<(Voice, String, Playback)>,
    pub playing: Vec<Voice>,
    pub next: Voice,
    /// Every master gain set, in order.
    pub master_gains: Vec<f32>,
}

impl FakeAudio {
    /// Ends `voice` as if its file finished.
    pub fn end(&mut self, voice: Voice) {
        self.playing.retain(|&v| v != voice);
    }
}

impl AudioOut for FakeAudio {
    fn play(&mut self, path: &str, playback: Playback) -> Option<Voice> {
        let v = self.next;
        self.next += 1;
        self.started.push((v, path.to_owned(), playback));
        self.playing.push(v);
        Some(v)
    }

    fn stop(&mut self, voice: Voice) {
        self.end(voice);
    }

    fn is_playing(&self, voice: Voice) -> bool {
        self.playing.contains(&voice)
    }

    fn set_master_gain(&mut self, gain: f32) {
        self.master_gains.push(gain);
    }
}
