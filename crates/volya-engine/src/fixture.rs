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
<menu_main_single btn_height="30" font="graffiti22" a="255" r="0" g="0" b="0">
  <btn name="btn_ret" caption="ui_mm_return_game"/><btn name="btn_lastsave" caption="ui_mm_last_save"/>
  <btn name="btn_load" caption="ui_mm_load_game"/><btn name="btn_save" caption="ui_mm_save_game"/>
  <btn name="btn_options" caption="ui_mm_options_n"/><btn name="btn_credits" caption="ui_mm_credits"/>
  <btn name="btn_quit_to_mm" caption="ui_mm_quit_game"/><btn name="btn_quit" caption="ui_mm_quit2windows"/>
</menu_main_single>
<menu_sound random="0"><menu_music>music\wasteland2</menu_music></menu_sound>
</window>"#;

/// The inventory screen's layout with the values of
/// docs/behavior/inventory-screen.md section 3 (normal layout), its
/// textures reduced to what the fixture holds.
pub const INVENTORY: &str = r#"<w>
<properties_box><texture>ui_cb_listbox</texture>
<list x="0" y="0" width="10" height="10" item_height="18" bottom_indent="10" right_ident="10" left_ident="20">
<font_s r="218" g="217" b="213"/><font r="131" g="131" b="131" font="letterica16"/></list></properties_box>
<belt_slots x="0" y="80" width="1024" height="172"/>
<back x="10" y="243" width="1005" height="483"><texture>ui_inventory_main</texture></back>
<bottom_static x="0" y="736" width="1024" height="32"/>
<dragdrop_bag x="22" y="33" width="310" height="442" cell_width="41" cell_height="41" rows_num="10" cols_num="7" unlimited="1" group_similar="1"/>
<dragdrop_belt x="645" y="136" width="410" height="60" cell_width="60" cell_height="60" rows_num="1" cols_num="5"/>
<dragdrop_outfit x="858" y="290" width="104" height="260" cell_width="57" cell_height="57" rows_num="4" cols_num="4" custom_placement="0"/>
<dragdrop_pistol x="38" y="118" width="156" height="104" cell_width="52" cell_height="52" rows_num="2" cols_num="3" custom_placement="0"/>
<dragdrop_automatic x="237" y="118" width="312" height="104" cell_width="52" cell_height="52" rows_num="2" cols_num="6" custom_placement="0"/>
<bag_static x="15" y="240" width="350" height="480" complex_mode="1"><text x="30" y="8" font="graffiti22">ui_inv_bag</text></bag_static>
<money_static x="744" y="530" width="108" height="33" stretch="1"><text font="graffiti22" align="c" vert_align="c"/></money_static>
<descr_static x="370" y="253" width="335" height="475"><text x="25" y="0" font="graffiti22">ui_inv_description</text></descr_static>
<character_frame_window x="730" y="260" width="268" height="460"><title x="80" y="-8" width="240" height="60"><text font="graffiti22">ui_inv_personal</text></title></character_frame_window>
<outfit_info x="740" y="550" width="260" height="180"><auto_static x="140" y="0"><text font="graffiti22" complex_mode="0">ui_st_params</text></auto_static>
<scroll_view x="10" y="33" width="240" height="147" always_show_scroll="0"/>
<static_burn_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_burn_immunity>
<static_strike_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_strike_immunity>
<static_shock_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_shock_immunity>
<static_wound_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_wound_immunity>
<static_radiation_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_radiation_immunity>
<static_telepatic_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_telepatic_immunity>
<static_chemical_burn_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_chemical_burn_immunity>
<static_explosion_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_explosion_immunity>
<static_fire_wound_immunity width="230" height="15" complex_mode="1"><text font="letterica16" x="17"/></static_fire_wound_immunity>
</outfit_info>
<progress_background x="739" y="340" width="59" height="215"/>
<progress_bar_health x="8" y="22" width="10" height="124" horz="0" min="0" max="100"><progress/></progress_bar_health>
<progress_bar_psy x="26" y="22" width="10" height="124" horz="0" min="0" max="100"><progress/></progress_bar_psy>
<progress_bar_radiation x="45" y="22" width="10" height="124" horz="0" min="0" max="100"><progress/></progress_bar_radiation>
<time_static x="15" y="730" width="200" height="30"><text x="20" y="8" font="letterica16">ui_inv_time</text></time_static>
<time_static_str x="115" y="7" width="107" height="30"><text font="letterica16"/></time_static_str>
<exit_button x="870" y="725" width="157" height="48"><texture>ui_button_main02</texture><text font="graffiti22">ui_inv_exit</text></exit_button>
<action_sounds><snd_open>interface\inv_open</snd_open><snd_close>interface\inv_close</snd_close>
<snd_item_to_slot>interface\inv_slot</snd_item_to_slot><snd_item_to_belt>interface\inv_belt</snd_item_to_belt>
<snd_item_to_ruck>interface\inv_ruck</snd_item_to_ruck><snd_properties>interface\inv_properties</snd_properties>
<snd_drop_item>interface\inv_drop</snd_drop_item><snd_attach_addon>interface\inv_attach_addon</snd_attach_addon>
<snd_detach_addon>interface\inv_detach_addon</snd_detach_addon><snd_item_use>interface\inv_slot</snd_item_use></action_sounds>
</w>"#;

/// The information panel (inventory-screen.md section 4.6).
pub const INVENTORY_ITEM: &str = r#"<w>
<static_name x="23" y="188" width="185" height="30" stretch="1"><text font="letterica18" complex_mode="1" align="l" vert_align="c"/></static_name>
<static_weight x="23" y="222" width="90" height="20" stretch="1"><text align="l" font="letterica18"/></static_weight>
<static_cost x="215" y="188" width="110" height="30" stretch="1"><text font="letterica18" align="c" vert_align="c"/></static_cost>
<static_condition x="113" y="222" width="95" height="20" stretch="1"><text align="r" font="letterica18">st_condition</text></static_condition>
<descr_list x="24" y="245" width="300" height="193" left_ident="16"><font font="letterica16" r="240" g="217" b="182"/></descr_list>
<image_static x="170" y="100" width="300" height="160" alignment="c" stretch="1"/>
<condition_progress x="215" y="227" width="107" height="8" horz="1" min="0" max="100"><min_color r="255" g="0" b="0"/><max_color r="0" g="255" b="0"/><progress/></condition_progress>
</w>"#;

/// The menu's boxes (save-load-screens.md section 6), all at the same
/// rectangles.
fn message_boxes() -> String {
    let mut xml = "<window>".to_owned();
    for (name, kind, text) in [
        (
            "message_box_quit_windows",
            "quit_windows",
            "ui_mm_quit_windows_message",
        ),
        (
            "message_box_quit_game",
            "quit_game",
            "ui_st_want_to_leave_game_q",
        ),
        (
            "message_box_confirm_load_save",
            "yes_no",
            "ui_st_load_game_confirm",
        ),
        (
            "message_box_empty_file_name",
            "ok",
            "ui_st_file_name_is_empty",
        ),
        (
            "message_box_file_already_exist",
            "yes_no",
            "ui_st_file_exist_owerwrite_q",
        ),
        (
            "message_box_delete_file_name",
            "yes_no",
            "ui_mm_delete_existing_file_q",
        ),
        (
            "message_box_invalid_saved_game",
            "ok",
            "ui_st_invalid_saved_game",
        ),
    ] {
        let button = |e: &str, x: u32, key: &str| {
            format!(
                r#"<{e} x="{x}" y="154" width="117" height="29"><texture>ui_button_ordinary</texture><text font="letterica16" r="216" g="186" b="140">{key}</text></{e}>"#
            )
        };
        let buttons = if kind == "ok" {
            button("button_ok", 370, "ui_st_btn_ok")
        } else {
            button("button_yes", 250, "ui_st_btn_yes") + &button("button_no", 370, "ui_st_btn_no")
        };
        xml.push_str(&format!(
            r#"<{name} type="{kind}" x="200" y="250" width="510" height="206"><texture>ui_frame_error</texture>
<picture x="35" y="65" width="77" height="77"><texture>ui_frame_error_sign_red</texture></picture>
<message_text x="170" y="40" width="310" height="60" complex_mode="1"><text font="letterica18" r="216" g="186" b="140">{text}</text></message_text>
{buttons}</{name}>"#
        ));
    }
    xml.push_str("</window>");
    xml
}

/// A save or load screen (save-load-screens.md tables 4.1 and 5.1).
fn save_load_dialog(save: bool) -> String {
    let button = |name: &str, x: u32, key: &str| {
        format!(
            r#"<{name} x="{x}" y="427" width="157" height="48"><texture>ui_button_main01</texture><text font="graffiti22">{key}</text>
<text_color><e r="227" g="199" b="178"/><t r="180" g="153" b="155"/><d r="106" g="95" b="91"/><h r="0" g="0" b="0"/></text_color></{name}>"#
        )
    };
    let (caption, list_y, list_h, action) = if save {
        (
            "ui_mm_save_game",
            106,
            287,
            button("btn_save", 65, "ui_mm_save"),
        )
    } else {
        (
            "ui_mm_load_game",
            231,
            180,
            button("btn_load", 65, "ui_mm_load"),
        )
    };
    let extra = if save {
        r#"<edit x="65" y="55" width="257" height="23"><texture>ui_linetext_e</texture><text font="letterica18" r="240" g="217" b="182"/></edit>"#
    } else {
        r#"<picture x="105" y="71" width="128" height="128" clipper="1"><texture>ui\ui_noise</texture></picture>
<file_info x="47" y="55" width="497" height="171"><texture x="0" y="768" width="497" height="171">ui\ui_static_mm_back_04</texture></file_info>
<file_caption x="299" y="109" width="512" height="20" complex_mode="1"><text font="letterica18" r="238" g="155" b="23"/></file_caption>
<file_data x="299" y="129" width="512" height="50" complex_mode="1"><text font="letterica18" r="216" g="186" b="140"/></file_data>"#
    };
    format!(
        r#"<window>
<back_video x="0" y="0" width="1024" height="430"><texture x="0" y="0" width="1024" height="430">ui\ui_mm_window_back_crop</texture></back_video>
<background x="0" y="0" width="1024" height="768"><texture>ui\ui_static_mm_back_04</texture></background>
<newspaper_video x="137" y="512" width="256" height="256"><texture>ui\ui_mm_newspaper</texture></newspaper_video>
<form x="415" y="168" width="560" height="460"><texture>ui_menu_options_dlg</texture>
<caption x="65" y="10" width="500" height="25"><text font="graffiti22" r="184" g="168" b="131">{caption}</text></caption>
{extra}
<list_frame x="65" y="{}" width="449" height="{}"><texture>ui_tablist_textbox</texture></list_frame>
<list x="75" y="{list_y}" width="429" height="{list_h}" item_height="18"/>
{action}{}{}
</form></window>"#,
        list_y - 10,
        list_h + 20,
        button("btn_delete", 221, "ui_mm_delete"),
        button("btn_cancel", 377, "ui_mm_cancel"),
    )
}

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

const STRINGS: [(&str, &str); 20] = [
    ("ui_mm_return_game", "Возврат к игре"),
    ("ui_mm_save_game", "Сохранить игру"),
    ("ui_mm_quit_game", "Завершить игру"),
    ("ui_st_btn_yes", "Да"),
    ("ui_st_btn_no", "Нет"),
    ("ui_st_btn_ok", "ОК"),
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
        // The inventory screen's item sections (it reads their panel keys).
        format!(
            "[texture_desc]\nfiles = ui_mainmenu, ui_asus_intro, ui_common, ui_inventory\n\
             [evaluation]\nline1 = \"\"\nline2 = \"\"\nline3 = \"\"\n{}",
            crate::inventory_screen::tests::ITEMS
        )
        .into(),
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
    // The colours the inventory screen's texts name (inventory-screen.md
    // 4.5, 4.8).
    add(
        &format!("{c}ui/color_defs.xml"),
        br#"<colors><color name="UI_orange" r="238" g="153" b="26"/>
        <color name="red" r="255" g="0" b="0"/><color name="green" r="0" g="255" b="0"/></colors>"#
            .into(),
    );
    add(&format!("{c}ui/message_box.xml"), message_boxes().into());
    add(
        &format!("{c}ui/ui_mm_save_dlg.xml"),
        save_load_dialog(true).into(),
    );
    add(
        &format!("{c}ui/ui_mm_load_dlg.xml"),
        save_load_dialog(false).into(),
    );
    add(
        &format!("{c}ui/scroll_bar.xml"),
        br#"<window><default height="16"><up_arrow><texture_e>ui_scroll_btn_up</texture_e></up_arrow>
<down_arrow><texture_e>ui_scroll_btn_down</texture_e></down_arrow><back_v><texture>ui_scroll_back</texture></back_v>
<box_v><texture>ui_scroll_box</texture></box_v></default></window>"#
            .into(),
    );
    add(
        &format!("{c}ui/ui_common.xml"),
        br#"<ui_texture><file_name>ui\ui_common</file_name>
        <texture id="ui_frame_error" x="213" y="0" width="510" height="206"/>
        <texture id="ui_frame_error_sign_red" x="800" y="0" width="77" height="77"/>
        <texture id="ui_button_ordinary_e" x="0" y="115" width="117" height="29"/>
        <texture id="ui_button_ordinary_h" x="0" y="144" width="117" height="29"/>
        <texture id="ui_button_ordinary_t" x="0" y="173" width="117" height="29"/>
        <texture id="ui_button_main01_e" x="0" y="470" width="157" height="48"/>
        <texture id="ui_button_main01_h" x="0" y="566" width="157" height="48"/>
        <texture id="ui_button_main01_t" x="0" y="518" width="157" height="48"/>
        <texture id="ui_menu_options_dlg" x="164" y="232" width="566" height="461"/>
        <texture id="ui_listline_b" x="0" y="37" width="4" height="18"/>
        <texture id="ui_listline_back" x="4" y="37" width="28" height="18"/>
        <texture id="ui_listline_e" x="32" y="37" width="4" height="18"/>
        <texture id="ui_button_main02_e" x="0" y="662" width="157" height="48"/>
        <texture id="ui_button_main02_h" x="0" y="758" width="157" height="48"/>
        <texture id="ui_button_main02_t" x="0" y="710" width="157" height="48"/>
        <texture id="ui_button_main02_d" x="0" y="614" width="157" height="48"/>
        </ui_texture>"#
            .into(),
    );
    add(&format!("{c}ui/ui_mm_main.xml"), LAYOUT.into());
    add(&format!("{c}ui/inventory_new.xml"), INVENTORY.into());
    add(&format!("{c}ui/inventory_item.xml"), INVENTORY_ITEM.into());
    add(
        &format!("{c}ui/ui_inventory.xml"),
        br#"<ui_texture><file_name>ui\ui_inventory</file_name>
        <texture id="ui_inventory_main" x="0" y="0" width="1005" height="483"/>
        <texture id="npc_icon_without_outfit" x="384" y="384" width="128" height="320"/>
        </ui_texture>"#
            .into(),
    );
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
    add(&format!("{t}ui/ui_common.dds"), dds(1024, 1024));
    // The inventory screen (inventory-screen.md section 9).
    add(&format!("{t}ui/ui_inventory.dds"), dds(1024, 1024));
    add(&format!("{t}ui/ui_icon_equipment.dds"), dds(1024, 2048));
    add(&format!("{t}ui/ui_grid.dds"), dds(128, 64));
    add(&format!("{t}ui/ui_static_mm_back_04.dds"), dds(1024, 1024));
    add(
        &format!("{t}ui/ui_noise.seq"),
        b"25\r\nui\\ui_noise_00\r\n".into(),
    );
    add(&format!("{t}ui/ui_noise_00.dds"), dds(256, 256));
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
        "ui/ui_mm_window_back_crop.ogm",
        "ui/ui_mm_newspaper.ogm",
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
        (
            "ui\\ui_mm_window_back_crop",
            video("ui\\ui_mm_window_back_crop", 1024, 430, 30.0),
        ),
        (
            "ui\\ui_mm_newspaper",
            video("ui\\ui_mm_newspaper", 256, 256, 33.0),
        ),
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
    /// Every category volume set, in order.
    pub volumes: Vec<(crate::core::audio::Category, f32)>,
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

    fn set_volume(&mut self, category: crate::core::audio::Category, volume: f32) {
        self.volumes.push((category, volume));
    }
}
