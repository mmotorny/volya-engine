// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-window-description-files-fonts-and-textures.md
//! on every target, one check per section, over window description files
//! of our own.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::text::string_table::StringTable;
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::ui::layout::Rect;
use volya_engine_formats::{ltx, xml};

use super::ensure;
use super::loading::scripts_with;
use super::services::run;
use crate::ErrorKind;
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::window_files::{WindowFiles, WindowKind, argb};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 reader and ParseFile", reader),
    ("3.1 building and attaching", attaching),
    ("3.2 the 24 methods", methods),
    ("3.3 InitWindow and InitAutoStaticGroup", init_window),
    ("3.4 worked example", worked_example),
    ("4.1 GetFont", fonts),
    ("4.2 CGameFont", font_constants),
    ("4.3 GetARGB", get_argb),
    ("5 texture descriptions", textures),
    ("6 translate_string", translate_string),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("window files {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const TEST_XML: &[u8] = br#"<window>
  <main x="10" y="20" width="100" height="50">
    <window_name>main_wnd</window_name>
    <font font="medium" r="1" g="2" b="3"/>
    <auto_static x="1" y="2" width="3" height="4"><texture>t</texture></auto_static>
    <auto_static x="5" y="6" width="7" height="8"/>
  </main>
  <main x="1" y="2" width="3" height="4"/>
  <item_0 x="0" y="0" width="10" height="10"><text>a</text></item_0>
  <item_1 x="0" y="10" width="10" height="10"/>
  <item_2 x="0" y="20" width="10" height="10"/>
  <centred x="50" y="50" width="20" height="10" alignment="c"/>
  <view x="0" y="0" width="100" height="100">
    <text x="1" y="1" width="5" height="5">one</text>
    <text x="1" y="6" width="5" height="5">two</text>
  </view>
  <tab x="0" y="0" width="100" height="20">
    <button x="0" y="0" width="50" height="20"/>
    <button x="50" y="0" width="50" height="20"/>
  </tab>
  <menu x="590" y="419" width="400" height="300">
    <shniaga x="9" y="0">
      <magnifire y_offset="1"/><left_anim/><right_anim/><left_grating x="0" y="4"/><right_grating/>
    </shniaga>
    <buttons_region/>
  </menu>
  <bad_font><text font="nope">x</text></bad_font>
  <group><auto_static x="1"/><auto_static x="2"/><auto_static x="3"/></group>
  <empty_group/>
  <progress_bar x="1" y="2" width="3" height="4"/>
</window>
"#;

/// The `_16` variant (UL 1.1).
const TEST_16_XML: &[u8] = br#"<window><main x="7" y="8" width="9" height="10"/></window>"#;

const TEXTURES_XML: &[u8] = br#"<ui_texture>
  <file_name>ui\ui_iconsTotal</file_name>
  <texture id="ui_iconsTotal_found_thing" x="0" y="94" width="83" height="47"/>
  <texture id="kept" x="9" y="9" width="9" height="9"/>
</ui_texture>
"#;

const GAME_FILES: &[(&str, &[u8])] = &[
    ("config/ui/test.xml", TEST_XML),
    ("config/ui/test_16.xml", TEST_16_XML),
    ("config/ui/broken.xml", b"<window><a x=\"1\""),
    ("config/ui/textures.xml", TEXTURES_XML),
    (
        "config/localization.ltx",
        b"[string_table]\r\nlanguage = rus\r\nfiles = st_test\r\n",
    ),
    (
        "config/text/rus/st_test.xml",
        b"<string_table><string id=\"st_tip\"><text>tip</text></string></string_table>",
    ),
];

const FSGAME: &[u8] = b"$game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
    $game_scripts$ = true | false| $game_data$ | scripts\\\r\n\
    $game_config$ = true | false| $game_data$ | config\\\r\n";

/// A script window class, as the shipped interface scripts derive theirs
/// (3.3's example).
const SCRIPT_WINDOW: &str = "class \"test_wnd\" (CUIScriptWnd)\n\
    function test_wnd:__init() super() end\n";

struct State {
    s: Scripts,
    services: Rc<RefCell<EngineServices>>,
    files: Rc<RefCell<WindowFiles>>,
}

/// A game script state with the services and the window files' names
/// registered and `_G` loaded, over [`GAME_FILES`]; the UI fonts created,
/// and the registry holding `kept` (1, 1, 1, 1).
fn state() -> Result<State, String> {
    let (mut s, trees) =
        scripts_with(&[("_g.script", SCRIPT_WINDOW)], GAME_FILES, Prefix::Default)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(b"").map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system)));
    let mut window_files = WindowFiles {
        fonts_created: true,
        ..WindowFiles::default()
    };
    let kept = xml::parse(
        b"<ui_texture><file_name>first</file_name>\
          <texture id=\"kept\" x=\"1\" y=\"1\" width=\"1\" height=\"1\"/></ui_texture>",
    )
    .map_err(|e| e.to_string())?;
    window_files
        .textures
        .add_file(&kept)
        .map_err(|e| e.to_string())?;
    let files = Rc::new(RefCell::new(window_files));
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    s.register_window_files(Rc::clone(&services), Rc::clone(&files), trees, aliases)
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    Ok(State { s, services, files })
}

/// `code` must stop a new state with a fatal error whose report includes
/// `rows`.
fn fatal(code: &str, rows: &[&str]) -> Result<(), String> {
    fatal_in(state()?, code, rows)
}

fn fatal_in(mut st: State, code: &str, rows: &[&str]) -> Result<(), String> {
    let result =
        st.s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = st.services.borrow().console.log().join("\n");
    for row in rows {
        ensure(log.contains(row), || {
            format!("{code}: no {row:?} in the log:\n{log}")
        })?;
    }
    Ok(())
}

/// The reader methods (1).
const READER_METHODS: [&str; 28] = [
    "ParseFile",
    "ParseShTexInfo",
    "InitWindow",
    "InitAutoStaticGroup",
    "InitStatic",
    "InitAnimStatic",
    "InitCheck",
    "InitButton",
    "Init3tButton",
    "InitEditBox",
    "InitCDkey",
    "InitFrame",
    "InitFrameLine",
    "InitLabel",
    "InitSpinNum",
    "InitSpinFlt",
    "InitSpinText",
    "InitComboBox",
    "InitList",
    "InitScrollView",
    "InitTab",
    "InitTrackBar",
    "InitProgressBar",
    "InitMMShniaga",
    "InitKeyBinding",
    "InitServerList",
    "InitMapList",
    "InitMapInfo",
];

/// Section 1: what is registered, and what arguments match.
pub fn registered() -> Result<(), String> {
    let mut st = state()?;
    let methods = READER_METHODS
        .iter()
        .map(|m| format!("{m:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    run(
        &mut st.s,
        "1",
        &format!(
            r##"
local class_mt = getmetatable(CScriptXmlInit)
for _, c in ipairs({{"CScriptXmlInit", "CGameFont", "TEX_INFO"}}) do
  eq(getmetatable(_G[c]) == class_mt, true, c .. " is a class")
end
local xml = CScriptXmlInit()
for _, m in ipairs({{{methods}}}) do eq(type(xml[m]), "function", m) end
for _, f in ipairs({{"GetFontSmall", "GetFontMedium", "GetFontDI", "GetFontGraffiti19Russian",
    "GetFontGraffiti22Russian", "GetFontGraffiti32Russian", "GetFontGraffiti50Russian",
    "GetFontLetterica16Russian", "GetFontLetterica18Russian", "GetFontLetterica25",
    "GetTextureName", "GetTextureRect", "GetTextureInfo", "GetARGB"}}) do
  eq(type(_G[f]), "function", f)
end
eq(type(game.translate_string), "function", "game.translate_string")
eq(rawget(_G, "__volya_window_files_translate_string"), nil, "no hidden name")
raises(function() return CGameFont() end, "", "no CGameFont constructor")
raises(function() return TEX_INFO() end, "", "no TEX_INFO constructor")
xml:ParseFile("test.xml")
raises(function() xml:InitStatic(1, nil) end, "string expected", "a number path")
raises(function() xml:InitStatic("main", 1) end, "CUIWindow expected", "a number parent")
raises(function() xml:InitStatic("main", xml) end, "CUIWindow expected", "a reader parent")
raises(function() xml:InitWindow("main", "0", nil) end, "number expected", "a text index")
raises(function() GetARGB("1", 0, 0, 0) end, "number expected", "a text colour")
"##
        ),
    )
}

/// Section 2: a new reader's empty document, ParseFile's loading with the
/// `_16` rule, a file that can't be opened, a parse error.
pub fn reader() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "2",
        r##"
local xml = CScriptXmlInit()
xml:ParseFile("missing.xml")
xml:ParseFile("test.xml")
xml:ParseFile("missing.xml")
eq(xml:InitStatic("main", nil) ~= nil, true, "the document is left as it was")
"##,
    )?;
    let main = st.files.borrow().windows[0].rect;
    ensure(main == Rect::new(10.0, 20.0, 100.0, 50.0), || {
        format!("main: {main:?}")
    })?;
    st.files.borrow_mut().resolution = Resolution::new(1280, 800);
    run(
        &mut st.s,
        "2 _16",
        r##"
local xml = CScriptXmlInit()
xml:ParseFile("test.xml")
xml:InitStatic("main", nil)
"##,
    )?;
    let wide = st.files.borrow().windows.last().map(|w| w.rect);
    ensure(wide == Some(Rect::new(7.0, 8.0, 9.0, 10.0)), || {
        format!("the _16 file's main: {wide:?}")
    })?;
    // A document without a root stops the game at the first element read
    // (UL 1.4), naming the file.
    fatal(
        "local xml = CScriptXmlInit() xml:InitStatic('main', nil)",
        &["no root element"],
    )?;
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('missing.xml') xml:InitStatic('main', nil)",
        &["no root element"],
    )?;
    fatal("CScriptXmlInit():ParseFile('broken.xml')", &["broken.xml"])
}

/// Section 3.1: a new window each call, owned, attached as the parent's
/// last child or a scroll view's item; `InitSpinText` directly; a missing
/// element stops the game.
pub fn attaching() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "3.1",
        r##"
local xml = CScriptXmlInit()
xml:ParseFile("test.xml")
local parent = xml:InitStatic("main", nil)
local a = xml:InitStatic("item_0", parent)
local b = xml:InitStatic("item_1", parent)
eq(rawequal(a, b), false, "a new object")
local view = xml:InitScrollView("view", parent)
xml:InitStatic("item_2", view)
xml:InitSpinText("centred", view)
local w = test_wnd()
xml:InitStatic("item_0", w)
"##,
    )?;
    let files = st.files.borrow();
    let w = &files.windows;
    // 0 main with its 2 auto statics (1, 2); 3 item_0; 4 item_1; 5 view
    // with its text items (6, 7); 8 item_2; 9 the spin box; 10 the
    // constructed window; 11 item_0.
    ensure(w.len() == 12, || format!("{} windows", w.len()))?;
    ensure(w[0].parent.is_none() && w[0].owned, || {
        format!("main: {:?}", w[0])
    })?;
    ensure(w[0].children == [1, 2, 3, 4, 5], || {
        format!("main's children: {:?}", w[0].children)
    })?;
    ensure(w[5].kind == WindowKind::ScrollView, || "view's kind".into())?;
    ensure(w[5].items == [6, 7, 8] && w[5].relayout, || {
        format!("view's items: {:?}", w[5].items)
    })?;
    ensure(w[5].children == [9], || {
        format!("the spin box is the view's child: {:?}", w[5].children)
    })?;
    ensure(w[9].rect == Rect::new(40.0, 45.0, 20.0, 10.0), || {
        format!("centred (UL 4.1): {:?}", w[9].rect)
    })?;
    ensure(
        w[10].kind == WindowKind::ScriptWindow && w[10].children == [11],
        || format!("a script window: {:?}", w[10]),
    )?;
    ensure(
        w.iter()
            .skip(1)
            .all(|w| w.owned || w.kind == WindowKind::ScriptWindow),
        || "the owned marks".into(),
    )?;
    drop(files);
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') xml:InitStatic('none', nil)",
        &["XML node not found", "none"],
    )?;
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') xml:InitStatic('bad_font', nil)",
        &["unknown font"],
    )
}

/// Section 3.2: each method's kind and class, the required elements.
pub fn methods() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "3.2",
        r##"
local classes = {
  InitStatic = "CUIStatic", InitAnimStatic = "CUIStatic", InitCheck = "CUICheckButton",
  InitButton = "CUIButton", Init3tButton = "CUI3tButton", InitEditBox = "CUIEditBox",
  InitCDkey = "CUIEditBox", InitFrame = "CUIFrameWindow", InitFrameLine = "CUIFrameLineWnd",
  InitLabel = "CUILabel", InitSpinNum = "CUISpinNum", InitSpinFlt = "CUISpinFlt",
  InitSpinText = "CUISpinText", InitComboBox = "CUIComboBox", InitList = "CUIListWnd",
  InitScrollView = "CUIScrollView", InitTab = "CUITabControl", InitTrackBar = "CUITrackBar",
  InitProgressBar = "CUIProgressBar", InitMapInfo = "CUIMapInfo",
}
local xml = CScriptXmlInit()
xml:ParseFile("test.xml")
local order = {}
for m in pairs(classes) do table.insert(order, m) end
table.sort(order)
for _, m in ipairs(order) do
  local w = xml[m](xml, "item_1", nil)
  eq(w ~= nil, true, m .. " makes a window")
  -- The returned object is of its class: the class's members (none yet,
  -- G2) aside, a window argument takes it.
  xml:InitStatic("item_2", w)
end
local menu = xml:InitMMShniaga("menu", nil)
eq(menu ~= nil, true, "InitMMShniaga")
local tab = xml:InitTab("tab", nil)
"##,
    )?;
    let files = st.files.borrow();
    let tab = (files.windows.iter())
        .rfind(|w| w.kind == WindowKind::TabControl)
        .ok_or("no tab")?;
    ensure(
        tab.kind == WindowKind::TabControl && tab.children.len() == 2,
        || format!("the tab control's buttons: {tab:?}"),
    )?;
    ensure(
        tab.children
            .iter()
            .all(|&b| files.windows[b].kind == WindowKind::ThreeStateButton),
        || "three-state buttons".into(),
    )?;
    drop(files);
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') xml:InitMMShniaga('item_0', nil)",
        &["XML node not found", "item_0:shniaga:magnifire"],
    )?;
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') xml:InitKeyBinding('item_0', nil)",
        &["XML node not found", "item_0:scroll_view"],
    )
}

/// Section 3.3: `InitWindow` on an existing window, `InitAutoStaticGroup`.
pub fn init_window() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "3.3",
        r##"
local xml = CScriptXmlInit()
xml:ParseFile("test.xml")
local w = test_wnd()
eq(xml:InitWindow("main", 0, w), nil, "InitWindow returns nothing")
local second = test_wnd()
xml:InitWindow("main", 1.9, second)
local p = test_wnd()
eq(xml:InitAutoStaticGroup("group", p), nil, "InitAutoStaticGroup returns nothing")
xml:InitAutoStaticGroup("empty_group", nil)
xml:InitAutoStaticGroup("none", nil)
"##,
    )?;
    let files = st.files.borrow();
    let w = &files.windows;
    ensure(w[0].rect == Rect::new(10.0, 20.0, 100.0, 50.0), || {
        format!("InitWindow's rectangle: {:?}", w[0].rect)
    })?;
    ensure(w[0].name.as_deref() == Some("main_wnd"), || {
        format!("window name: {:?}", w[0].name)
    })?;
    ensure(w[0].font.is_some(), || "the font".into())?;
    ensure(w[0].children == [1, 2], || {
        format!("auto statics: {:?}", w[0].children)
    })?;
    ensure(
        w[1].name.as_deref() == Some("auto_static_0")
            && w[2].name.as_deref() == Some("auto_static_1")
            && w[1].rect == Rect::new(1.0, 2.0, 3.0, 4.0),
        || format!("auto statics: {:?} {:?}", w[1], w[2]),
    )?;
    // Index 1.9 truncates to the second `main`.
    ensure(w[3].rect == Rect::new(1.0, 2.0, 3.0, 4.0), || {
        format!("index 1: {:?}", w[3].rect)
    })?;
    ensure(w[4].children == [5, 6, 7], || {
        format!("the group: {:?}", w[4].children)
    })?;
    ensure(
        (5..8).all(|i| w[i].owned && w[i].kind == WindowKind::Static)
            && w[7].name.as_deref() == Some("auto_static_2")
            && w[7].rect.x == 3.0,
        || "the group's statics".into(),
    )?;
    ensure(w.len() == 8, || format!("{} windows", w.len()))?;
    drop(files);
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') xml:InitWindow('main', 2, test_wnd())",
        &["XML node not found", "main"],
    )?;
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') xml:InitWindow('main', 0, nil)",
        &["the window is nil"],
    )?;
    fatal(
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') xml:InitAutoStaticGroup('group', nil)",
        &["the parent is nil"],
    )
}

/// Section 3.4: `ui_game_over.script`'s loop stops the game at `item_3`.
pub fn worked_example() -> Result<(), String> {
    let st = state()?;
    let files = Rc::clone(&st.files);
    fatal_in(
        st,
        "local xml = CScriptXmlInit() xml:ParseFile('test.xml') \
         for n = 0, 30 do if xml:InitStatic('item_' .. n, nil) == nil then break end end",
        &["XML node not found", "item_3"],
    )?;
    let made = files.borrow().windows.len();
    ensure(made == 3, || format!("{made} statics made before"))
}

/// Section 4.1: the fonts, each call a new object; before the UI's fonts,
/// a fatal error (ours).
pub fn fonts() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "4.1",
        r##"
local a, b = GetFontMedium(), GetFontMedium()
eq(a ~= nil and not rawequal(a, b), true, "a new object each call")
eq(getmetatable(a) ~= nil, true, "an object")
"##,
    )?;
    let st = state()?;
    st.files.borrow_mut().fonts_created = false;
    fatal_in(st, "GetFontSmall()", &["the UI fonts are not created"])
}

/// Section 4.2: `CGameFont`'s constants.
pub fn font_constants() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "4.2",
        r##"
eq(CGameFont.alLeft, 0, "alLeft")
eq(CGameFont.alRight, 1, "alRight")
eq(CGameFont.alCenter, 2, "alCenter")
"##,
    )
}

/// Section 4.3: `GetARGB`'s examples.
pub fn get_argb() -> Result<(), String> {
    let examples: [([f64; 4], i32); 6] = [
        ([255.0, 255.0, 255.0, 255.0], -1),
        ([255.0, 0.0, 255.0, 0.0], -16_711_936),
        ([255.0, 128.0, 128.0, 128.0], -8_355_712),
        ([0.0, 1.0, 2.0, 3.0], 66_051),
        ([127.0, 0.0, 0.0, 0.0], 2_130_706_432),
        ([256.0, 255.0, 0.0, 0.0], 16_711_680),
    ];
    for ([a, r, g, b], want) in examples {
        let got = argb(a, r, g, b);
        ensure(got == want, || {
            format!("GetARGB({a}, {r}, {g}, {b}) = {got}")
        })?;
    }
    ensure(argb(0.5, 1.5, 2.5, -1.0) == 0x0002_02FF, || {
        "rounding to nearest even, -1's low bits".into()
    })?;
    let mut st = state()?;
    run(
        &mut st.s,
        "4.3",
        "eq(GetARGB(255, 0, 255, 0), -16711936, 'GetARGB')\n\
         eq(GetARGB(256, 255, 0, 0), 16711680, 'alpha 0')",
    )
}

/// Section 5: the registry's lookups, `TEX_INFO`, `ParseShTexInfo`.
pub fn textures() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "5",
        r##"
local xml = CScriptXmlInit()
xml:ParseShTexInfo("textures.xml")
xml:ParseShTexInfo("missing.xml")
local info = GetTextureInfo("ui_iconsTotal_found_thing", "ui_iconsTotal_found_thing")
eq(info:get_file_name(), "ui\\ui_iconsTotal", "the file")
local r = info:get_rect()
eq(r.x1, 0, "x1") eq(r.y1, 94, "y1") eq(r.x2, 83, "x2") eq(r.y2, 141, "y2")
eq(rawequal(info:get_rect(), r), false, "a new Frect")
eq(GetTextureInfo("none", "kept"):get_file_name(), "first", "the default id")
eq(GetTextureName("kept"), "first", "the first definition wins")
local k = GetTextureRect("kept")
eq(k.x1, 1, "kept x1") eq(k.x2, 2, "kept x2") eq(k.y2, 2, "kept y2")
"##,
    )?;
    fatal("GetTextureName('none')", &["Can't find texture", "none"])?;
    fatal("GetTextureRect('none')", &["Can't find texture", "none"])?;
    fatal(
        "GetTextureInfo('none', 'none either')",
        &["Can't find texture", "none"],
    )
}

/// Section 6: the string table's text or the key; the table built at the
/// first lookup when the engine gave none.
pub fn translate_string() -> Result<(), String> {
    let mut st = state()?;
    run(
        &mut st.s,
        "6 built",
        r##"
eq(game.translate_string("st_tip"), "tip", "an entry")
eq(game.translate_string("st_none"), "st_none", "the key")
eq(game.translate_string(""), "", "the empty key")
"##,
    )?;
    let mut st = state()?;
    st.files.borrow_mut().strings = Some(Arc::new(StringTable::new([(
        "st_tip".to_owned(),
        "given".to_owned(),
    )])));
    run(
        &mut st.s,
        "6 given",
        "eq(game.translate_string('st_tip'), 'given', 'the engine\\'s table')",
    )
}
