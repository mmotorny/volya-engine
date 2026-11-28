// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Window description files, fonts and textures (script API G3,
//! docs/behavior/script-api-window-description-files-fonts-and-textures.md;
//! section numbers are its): the reader `CScriptXmlInit` and the windows it
//! builds (2, 3), `CGameFont` and the `GetFont…` functions (4.1, 4.2),
//! `GetARGB` (4.3), the texture registry's functions and `TEX_INFO` (5) and
//! `game.translate_string` (6), over a [`WindowFiles`] the engine shares.
//! See docs/architecture/scripts.md, Window description files.
//!
//! The windows the readers build are records in [`WindowFiles::windows`]:
//! which kind, the description read, the rectangle, font and name of the
//! window reading (W), where each is attached. What each window then does
//! with its description belongs to specs G1, G2 and G5, as the window
//! classes' members do (TODO(#207), TODO(#208)).

use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::text;
use volya_engine_core::ui::{find_ui_file, read_ui_file};
use volya_engine_formats::cp1251;
use volya_engine_formats::text::font::UiFont;
use volya_engine_formats::text::string_table::StringTable;
use volya_engine_formats::ui::layout::{FontDesc, Rect, StaticDesc, WindowDesc, font_attr};
use volya_engine_formats::ui::texture_desc::{TextureDesc, TextureDescriptions};
use volya_engine_formats::ui::{ColorDefs, Resolution, float_attr};
use volya_engine_formats::xml::{self, Document, Element};

use crate::classes::{EngineClass, EnginePart, ObjectMaker, Objects, Operand};
use crate::loader::{Scripts, Trees};
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::Frect;
use crate::{Error, Value};

/// A window's place in [`WindowFiles::windows`].
pub type WindowId = usize;

/// The kinds of window (3.2's "Makes a"), and those made otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    /// A `CUIWindow` scripts construct.
    Window,
    /// A `CUIScriptWnd` scripts construct.
    ScriptWindow,
    Static,
    AnimatedStatic,
    CheckButton,
    Button,
    ThreeStateButton,
    EditBox,
    KeyEntryBox,
    FrameWindow,
    FrameLine,
    Label,
    IntegerSpinBox,
    NumberSpinBox,
    TextSpinBox,
    ComboBox,
    ListWindow,
    ScrollView,
    TabControl,
    TrackBar,
    ProgressBar,
    TitleMenuItemList,
    KeyBindingList,
    ServerList,
    MapList,
    MapInformation,
    /// A scroll view's text item (`InitScrollView`'s `text` children).
    TextItem,
}

/// One window the readers built or scripts constructed.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub kind: WindowKind,
    /// The rectangle relative to the parent (UL 4.1).
    pub rect: Rect,
    /// The font of the window reading (UL 4.2).
    pub font: Option<FontDesc>,
    /// The window name: `window_name`'s value, or `auto_static_<n>`.
    pub name: Option<String>,
    /// The element the window was read from, as G1, G2 and G5 interpret it.
    pub description: Option<Element>,
    /// The window it is attached to: a child of it, or an item of its
    /// content when it is a scroll view.
    pub parent: Option<WindowId>,
    /// The children, drawn in this order (UL 4.3).
    pub children: Vec<WindowId>,
    /// A scroll view's content (UL 9.1).
    pub items: Vec<WindowId>,
    /// A scroll view marked to lay its items out again.
    pub relayout: bool,
    /// The "auto-delete" mark: the window it is attached to destroys it.
    pub owned: bool,
}

impl Window {
    fn new(kind: WindowKind) -> Self {
        Window {
            kind,
            rect: Rect::default(),
            font: None,
            name: None,
            description: None,
            parent: None,
            children: Vec::new(),
            items: Vec::new(),
            relayout: false,
            owned: false,
        }
    }
}

/// What the names read and change, shared by the engine across the state's
/// rebuilds.
pub struct WindowFiles {
    /// The output resolution the `_16` rule reads (UL 1.1), current when
    /// each file is loaded.
    pub resolution: Resolution,
    /// The named colours (UL 2).
    pub colors: ColorDefs,
    /// The texture registry (5.3): the main menu's descriptions, then what
    /// `ParseShTexInfo` adds.
    pub textures: TextureDescriptions,
    /// The string table (TF 1.2); built from the game's files at the first
    /// lookup when the engine gave none.
    pub strings: Option<Arc<StringTable>>,
    /// Whether the game's UI has created its fonts (TF 2.1).
    pub fonts_created: bool,
    /// Every window built or constructed, by [`WindowId`]; none is ever
    /// removed (believed, 3.1: a window with a nil parent stays allocated).
    pub windows: Vec<Window>,
}

impl Default for WindowFiles {
    /// At 1024 × 768 (no `_16` files), with nothing loaded.
    fn default() -> Self {
        WindowFiles {
            resolution: Resolution::new(1024, 768),
            colors: ColorDefs::default(),
            textures: TextureDescriptions::default(),
            strings: None,
            fonts_created: false,
            windows: Vec::new(),
        }
    }
}

impl WindowFiles {
    fn add(&mut self, window: Window) -> WindowId {
        self.windows.push(window);
        self.windows.len() - 1
    }

    /// Attaches `child` to `parent` (3.1): an item of a scroll view's
    /// content, marked to lay out again, unless `direct`; otherwise the
    /// parent's last child.
    fn attach(&mut self, child: WindowId, parent: WindowId, direct: bool) {
        self.windows[child].parent = Some(parent);
        let p = &mut self.windows[parent];
        if p.kind == WindowKind::ScrollView && !direct {
            p.items.push(child);
            p.relayout = true;
        } else {
            p.children.push(child);
        }
    }
}

/// A window object's engine part: the window it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowPart(pub WindowId);

/// A reader's engine part (2.1).
#[derive(Default)]
pub struct Reader {
    /// The file last parsed, as named.
    file: String,
    document: Document,
}

/// A `CGameFont`'s part: the font it refers to (4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameFont(pub UiFont);

/// A `TEX_INFO`'s part (5.2).
#[derive(Clone, Debug, PartialEq)]
pub struct TexInfo {
    pub file: String,
    pub rect: Frect,
}

/// The engine classes of the windows the readers return, each derived from
/// `CUIWindow` (1: a window argument takes them). Their members are G1's
/// and G2's (TODO(#207), TODO(#208)).
pub const WINDOW_CLASSES: [&str; 21] = [
    "CUIStatic",
    "CUICheckButton",
    "CUIButton",
    "CUI3tButton",
    "CUIEditBox",
    "CUIFrameWindow",
    "CUIFrameLineWnd",
    "CUILabel",
    "CUISpinNum",
    "CUISpinFlt",
    "CUISpinText",
    "CUIComboBox",
    "CUIListWnd",
    "CUIScrollView",
    "CUITabControl",
    "CUITrackBar",
    "CUIProgressBar",
    "CUIMMShniaga",
    "CServerList",
    "CUIMapList",
    "CUIMapInfo",
];

/// The main reading of an `Init…` method's element (3.2).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// W.
    Window,
    /// S (W and the static attributes).
    Static,
    /// `x`, `y`, `width`, `height`, `alignment` only.
    Rect,
}

/// How an `Init…` method reads its description beyond the main reading.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Extra {
    None,
    /// An optional `hint` child read as S (`Init3tButton`).
    Hint,
    /// An optional `title` child read as S.
    Title,
    /// A `font` child (`InitList`).
    ListFont,
    /// Each `text` child made a text item of the content.
    TextItems,
    /// Each `button` child made a three-state button of the tab control.
    TabButtons,
    /// `progress` (S, optional), then `background` (S, optional) only
    /// when it is present.
    Progress,
}

/// One `Init…` method (3.2).
struct Maker {
    method: &'static str,
    kind: WindowKind,
    class: &'static str,
    reading: Reading,
    extra: Extra,
    /// Children of the element read by readers that require them (UL
    /// 1.4), as `:`-separated paths below it, with their readings.
    required: &'static [(&'static str, Reading)],
}

const fn maker(
    method: &'static str,
    kind: WindowKind,
    class: &'static str,
    reading: Reading,
    extra: Extra,
) -> Maker {
    Maker {
        method,
        kind,
        class,
        reading,
        extra,
        required: &[],
    }
}

use Reading::{Rect as R, Static as S, Window as W};

/// The 24 methods (3.2). Of the elements the composite windows read, only
/// whether the required ones are there is checked here; reading them as
/// the windows' parts is G2's (TODO(#208)), the title menu's item lists
/// title-menu.md's, `InitKeyBinding`'s items G5's.
static MAKERS: [Maker; 24] = [
    maker(
        "InitStatic",
        WindowKind::Static,
        "CUIStatic",
        S,
        Extra::None,
    ),
    maker(
        "InitAnimStatic",
        WindowKind::AnimatedStatic,
        "CUIStatic",
        S,
        Extra::None,
    ),
    maker(
        "InitCheck",
        WindowKind::CheckButton,
        "CUICheckButton",
        S,
        Extra::None,
    ),
    maker(
        "InitButton",
        WindowKind::Button,
        "CUIButton",
        S,
        Extra::None,
    ),
    maker(
        "Init3tButton",
        WindowKind::ThreeStateButton,
        "CUI3tButton",
        W,
        Extra::Hint,
    ),
    maker(
        "InitEditBox",
        WindowKind::EditBox,
        "CUIEditBox",
        W,
        Extra::None,
    ),
    maker(
        "InitCDkey",
        WindowKind::KeyEntryBox,
        "CUIEditBox",
        W,
        Extra::None,
    ),
    maker(
        "InitFrame",
        WindowKind::FrameWindow,
        "CUIFrameWindow",
        W,
        Extra::Title,
    ),
    maker(
        "InitFrameLine",
        WindowKind::FrameLine,
        "CUIFrameLineWnd",
        R,
        Extra::Title,
    ),
    maker("InitLabel", WindowKind::Label, "CUILabel", R, Extra::Title),
    maker(
        "InitSpinNum",
        WindowKind::IntegerSpinBox,
        "CUISpinNum",
        W,
        Extra::None,
    ),
    maker(
        "InitSpinFlt",
        WindowKind::NumberSpinBox,
        "CUISpinFlt",
        W,
        Extra::None,
    ),
    maker(
        "InitSpinText",
        WindowKind::TextSpinBox,
        "CUISpinText",
        W,
        Extra::None,
    ),
    maker(
        "InitComboBox",
        WindowKind::ComboBox,
        "CUIComboBox",
        W,
        Extra::None,
    ),
    maker(
        "InitList",
        WindowKind::ListWindow,
        "CUIListWnd",
        R,
        Extra::ListFont,
    ),
    maker(
        "InitScrollView",
        WindowKind::ScrollView,
        "CUIScrollView",
        W,
        Extra::TextItems,
    ),
    maker(
        "InitTab",
        WindowKind::TabControl,
        "CUITabControl",
        W,
        Extra::TabButtons,
    ),
    maker(
        "InitTrackBar",
        WindowKind::TrackBar,
        "CUITrackBar",
        W,
        Extra::None,
    ),
    maker(
        "InitProgressBar",
        WindowKind::ProgressBar,
        "CUIProgressBar",
        R,
        Extra::Progress,
    ),
    Maker {
        required: &[
            ("shniaga:magnifire", S),
            ("shniaga", S),
            ("shniaga:left_anim", S),
            ("shniaga:right_anim", S),
            ("shniaga:left_grating", S),
            ("shniaga:right_grating", S),
            ("buttons_region", W),
        ],
        ..maker(
            "InitMMShniaga",
            WindowKind::TitleMenuItemList,
            "CUIMMShniaga",
            W,
            Extra::None,
        )
    },
    Maker {
        required: &[
            ("scroll_view", W),
            ("frame", W),
            ("header_1", R),
            ("header_2", R),
            ("header_3", R),
        ],
        ..maker(
            "InitKeyBinding",
            WindowKind::KeyBindingList,
            "CUIWindow",
            W,
            Extra::None,
        )
    },
    // The paths of the 7 column headers are not given (3.2): not checked.
    Maker {
        required: &[
            ("list", R),
            ("list_server_properties", R),
            ("list_players_list", R),
            ("frame", W),
            ("list_server_properties:frame", W),
            ("list_players_list:frame", W),
            ("edit_gs_filter", W),
            ("cap_server_properties", R),
            ("cap_players_list", R),
            ("cap_frags", R),
            ("cap_death", R),
        ],
        ..maker(
            "InitServerList",
            WindowKind::ServerList,
            "CServerList",
            W,
            Extra::None,
        )
    },
    Maker {
        required: &[
            ("header_1", R),
            ("header_2", R),
            ("frame_1", W),
            ("frame_2", W),
            ("list_1", W),
            ("list_2", W),
            ("btn_left", W),
            ("btn_right", W),
            ("btn_up", W),
            ("btn_down", W),
        ],
        ..maker(
            "InitMapList",
            WindowKind::MapList,
            "CUIMapList",
            W,
            Extra::None,
        )
    },
    maker(
        "InitMapInfo",
        WindowKind::MapInformation,
        "CUIMapInfo",
        W,
        Extra::None,
    ),
];

/// The 10 `GetFont…` functions and their fonts (4.1).
pub const FONT_FUNCTIONS: [(&str, UiFont); 10] = [
    ("GetFontSmall", UiFont::Small),
    ("GetFontMedium", UiFont::Medium),
    ("GetFontDI", UiFont::Di),
    ("GetFontGraffiti19Russian", UiFont::Graffiti19),
    ("GetFontGraffiti22Russian", UiFont::Graffiti22),
    ("GetFontGraffiti32Russian", UiFont::Graffiti32),
    ("GetFontGraffiti50Russian", UiFont::Graffiti50),
    ("GetFontLetterica16Russian", UiFont::Letterica16),
    ("GetFontLetterica18Russian", UiFont::Letterica18),
    ("GetFontLetterica25", UiFont::Letterica25),
];

/// `GetARGB(a, r, g, b)` (4.3): each argument rounded to nearest (ties to
/// even, the x87 unit's believed mode) and its low 16 bits kept, of which
/// the colour uses the low 8; read as a signed 32-bit integer. A NaN or a
/// value no integer conversion holds gives the x87 unit's "indefinite"
/// integer, whose low bits are 0.
pub fn argb(a: f64, r: f64, g: f64, b: f64) -> i32 {
    fn byte(n: f64) -> u32 {
        let n = n.round_ties_even();
        if n.is_nan() || n.abs() >= 9_223_372_036_854_775_808.0 {
            0
        } else {
            u32::from(n as i64 as u16) & 0xFF
        }
    }
    (byte(a) << 24 | byte(r) << 16 | byte(g) << 8 | byte(b)) as i32
}

/// A texture description's rectangle as a `Frect` (5.1): each sum in f32.
pub fn texture_rect(d: &TextureDesc) -> Frect {
    Frect {
        x1: d.rect.x,
        y1: d.rect.y,
        x2: d.rect.x + d.rect.width,
        y2: d.rect.y + d.rect.height,
    }
}

/// What the names share.
#[derive(Clone)]
struct Context {
    services: Rc<RefCell<EngineServices>>,
    files: Rc<RefCell<WindowFiles>>,
    trees: Rc<dyn Trees>,
    aliases: PathAliases,
    fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// The game stops (UL 1): reported and the state stopped. The error.
    fn stop(&self, function: &str, line: u32, description: &str, arguments: &[&str]) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description,
                arguments,
            },
        )
    }

    /// Reads the `ui` file `name` (UL 1.1, 1.2) and parses it (1.3):
    /// `Ok(None)` when it can't be opened, an include that can't be opened
    /// or a parse error stops the game.
    fn load(&self, function: &str, name: &str) -> Result<Option<Document>, String> {
        let resolution = self.files.borrow().resolution;
        let mut loaded: io::Result<Option<(String, Vec<u8>)>> = Ok(None);
        let lent = self.trees.with_trees(&mut |tree, _| {
            loaded = match find_ui_file(tree, &self.aliases, name, resolution) {
                Ok(Some(_)) => read_ui_file(tree, &self.aliases, name, resolution).map(Some),
                other => other.map(|_| None),
            };
        });
        let loaded = lent.and(loaded);
        let loaded = loaded.map_err(|e| self.stop(function, line!(), &e.to_string(), &[name]))?;
        let Some((path, text)) = loaded else {
            return Ok(None);
        };
        xml::parse(&text)
            .map(Some)
            .map_err(|e| self.stop(function, line!(), &e.to_string(), &[&path]))
    }

    /// The string table, built at the first lookup (TF 1.2).
    fn strings(&self, function: &str) -> Result<Arc<StringTable>, String> {
        if let Some(strings) = &self.files.borrow().strings {
            return Ok(Arc::clone(strings));
        }
        let mut built = Err(io::Error::other("the game's files are unavailable"));
        let lent = self.trees.with_trees(&mut |tree, _| {
            built = text::load_localization(tree, &self.aliases)
                .and_then(|l| text::load_string_table(tree, &self.aliases, &l));
        });
        let strings = lent
            .and(built)
            .map_err(|e| self.stop(function, line!(), &e.to_string(), &[]))?;
        let strings = Arc::new(strings);
        self.files.borrow_mut().strings = Some(Arc::clone(&strings));
        Ok(strings)
    }
}

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// Text argument `i` (1: only a Lua string matches; our wording for a
/// mismatch, depends on A).
fn text_arg(args: &[Value], i: usize, what: &str) -> Result<String, String> {
    match arg(args, i) {
        Value::String(s) => Ok(cp1251::decode(s)),
        _ => Err(format!("{what}: string expected")),
    }
}

fn number_arg(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// Window argument `i`: nil, or an object that is a window (1; our wording
/// for a mismatch, depends on A).
fn window_arg(
    o: &Objects,
    args: &[Value],
    i: usize,
    what: &str,
) -> Result<Option<WindowId>, String> {
    match arg(args, i) {
        Value::Nil => Ok(None),
        v => match o.get::<WindowPart>(v) {
            Some(Operand::Other(WindowPart(id))) => Ok(Some(id)),
            _ => Err(format!("{what}: CUIWindow expected")),
        },
    }
}

/// `text` as the game's bytes.
fn lua_text(text: &str) -> Value {
    Value::String(cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec()))
}

/// The readings of one element (3.2), the windows made of its parts aside.
struct Read {
    rect: Rect,
    font: Option<FontDesc>,
    name: Option<String>,
    auto_statics: Vec<StaticDesc>,
}

/// Reads `e` as `reading`; an unknown font name stops the game (UL 7.2).
fn read(e: &Element, reading: Reading, colors: &ColorDefs) -> io::Result<Read> {
    let window = |w: WindowDesc| Read {
        rect: w.rect(),
        font: w.font,
        name: w.window_name,
        auto_statics: w.auto_statics,
    };
    match reading {
        Reading::Window => WindowDesc::read(e, colors).map(window),
        Reading::Static => StaticDesc::read(e, colors).map(|s| window(s.window)),
        Reading::Rect => {
            let w = WindowDesc {
                name: e.name.clone(),
                x: float_attr(e, "x", 0.0),
                y: float_attr(e, "y", 0.0),
                width: float_attr(e, "width", 0.0),
                height: float_attr(e, "height", 0.0),
                centred: e.attr("alignment").is_some_and(|a| a.contains('c')),
                font: None,
                window_name: None,
                auto_statics: Vec::new(),
            };
            // The `text` child's font, which labels read (3.2).
            if let Some(t) = e.child("text") {
                font_attr(t)?;
            }
            Ok(window(w))
        }
    }
}

/// A reader's element at `path` and `index` (UL 1.4): a document without a
/// root, or no element there, stops the game.
fn element<'a>(
    c: &Context,
    reader: &'a Reader,
    function: &str,
    path: &str,
    index: Option<usize>,
) -> Result<&'a Element, String> {
    if reader.document.root().is_none() {
        // Our wording: the error text names the file (UL 1.4).
        return Err(c.stop(
            function,
            line!(),
            "the window description file has no root element",
            &[&reader.file],
        ));
    }
    index
        .and_then(|i| reader.document.find(path, i))
        .ok_or_else(|| c.stop(function, line!(), "XML node not found", &[path]))
}

/// Makes a window of `kind` from `e` read as `reading`, with its
/// `auto_static` children attached (3.3).
fn build(
    c: &Context,
    function: &str,
    kind: WindowKind,
    e: &Element,
    reading: Reading,
) -> Result<WindowId, String> {
    let mut files = c.files.borrow_mut();
    let r = read(e, reading, &files.colors)
        .map_err(|err| c.stop(function, line!(), &err.to_string(), &[&e.name]))?;
    let mut window = Window::new(kind);
    window.rect = r.rect;
    window.font = r.font;
    window.name = r.name;
    window.description = Some(e.clone());
    let id = files.add(window);
    add_auto_statics(&mut files, id, &r.auto_statics, e);
    Ok(id)
}

/// The `auto_static` children of `e` made statics of `parent` (3.3): named
/// `auto_static_<n>`, owned, the parent's last children in document order.
fn add_auto_statics(
    files: &mut WindowFiles,
    parent: WindowId,
    statics: &[StaticDesc],
    e: &Element,
) {
    for (n, (s, element)) in statics
        .iter()
        .zip(e.children_named("auto_static"))
        .enumerate()
    {
        let mut window = Window::new(WindowKind::Static);
        window.rect = s.window.rect();
        window.font = s.window.font.clone();
        window.name = Some(format!("auto_static_{n}"));
        window.description = Some(element.clone());
        window.owned = true;
        let id = files.add(window);
        files.attach(id, parent, true);
    }
}

/// An `Init…` method (3.1).
fn init(
    c: &Context,
    m: &Maker,
    reader: &Reader,
    o: &Objects,
    args: &[Value],
) -> Result<Vec<Value>, String> {
    c.stopped()?;
    let function = format!("CScriptXmlInit.{}", m.method);
    let path = text_arg(args, 0, &function)?;
    let parent = window_arg(o, args, 1, &function)?;
    let e = element(c, reader, &function, &path, Some(0))?;
    for (child, reading) in m.required {
        let full = format!("{path}:{child}");
        let sub = e
            .find(child)
            .ok_or_else(|| c.stop(&function, line!(), "XML node not found", &[&full]))?;
        let colors = c.files.borrow().colors.clone();
        read(sub, *reading, &colors)
            .map_err(|err| c.stop(&function, line!(), &err.to_string(), &[&full]))?;
    }
    let id = build(c, &function, m.kind, e, m.reading)?;
    extra(c, &function, m.extra, e, id)?;
    let mut files = c.files.borrow_mut();
    files.windows[id].owned = true;
    if let Some(parent) = parent {
        files.attach(id, parent, m.kind == WindowKind::TextSpinBox);
    }
    Ok(vec![o.new_object(m.class, WindowPart(id))])
}

/// The readings of 3.2 beyond the main one.
fn extra(
    c: &Context,
    function: &str,
    extra: Extra,
    e: &Element,
    id: WindowId,
) -> Result<(), String> {
    let check = |child: Option<&Element>, reading| -> Result<(), String> {
        let Some(child) = child else {
            return Ok(());
        };
        let colors = c.files.borrow().colors.clone();
        read(child, reading, &colors)
            .map(drop)
            .map_err(|err| c.stop(function, line!(), &err.to_string(), &[&child.name]))
    };
    match extra {
        Extra::None => Ok(()),
        Extra::Hint => check(e.child("hint"), Reading::Static),
        Extra::Title => check(e.child("title"), Reading::Static),
        Extra::ListFont => match e.child("font") {
            Some(f) => font_attr(f)
                .map(drop)
                .map_err(|err| c.stop(function, line!(), &err.to_string(), &[&e.name])),
            None => Ok(()),
        },
        Extra::Progress => match e.child("progress") {
            // Without `progress`, nothing more is read.
            None => Ok(()),
            progress => {
                check(progress, Reading::Static)?;
                check(e.child("background"), Reading::Static)
            }
        },
        Extra::TextItems => {
            for t in e.children_named("text") {
                let item = build(c, function, WindowKind::TextItem, t, Reading::Rect)?;
                let mut files = c.files.borrow_mut();
                files.windows[item].owned = true;
                files.attach(item, id, false);
            }
            Ok(())
        }
        Extra::TabButtons => {
            for b in e.children_named("button") {
                let button = build(
                    c,
                    function,
                    WindowKind::ThreeStateButton,
                    b,
                    Reading::Window,
                )?;
                check(b.child("hint"), Reading::Static)?;
                let mut files = c.files.borrow_mut();
                files.windows[button].owned = true;
                files.attach(button, id, true);
            }
            Ok(())
        }
    }
}

/// `CScriptXmlInit` (2, 3).
fn reader_class(ctx: &Context) -> EngineClass {
    let mut class = EngineClass::new("CScriptXmlInit")
        .constructor(0, |_| Some(0), |_, _| Ok(Reader::default()))
        .constructible();
    let c = ctx.clone();
    class = class.method("ParseFile", move |r: &mut Reader, args| {
        c.stopped()?;
        let name = text_arg(args, 0, "CScriptXmlInit.ParseFile")?;
        // A file that can't be opened leaves the document as it was (2.2).
        // A second ParseFile on one reader is unestablished (2.2): ours, the
        // new document replaces the earlier one.
        if let Some(document) = c.load("CScriptXmlInit.ParseFile", &name)? {
            r.file = name;
            r.document = document;
        }
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method("ParseShTexInfo", move |_: &mut Reader, args| {
        c.stopped()?;
        let name = text_arg(args, 0, "CScriptXmlInit.ParseShTexInfo")?;
        // TODO(#1145): a file that can't be opened, or one without a
        // `file_name` child, adds nothing (the simplest reading).
        if let Some(document) = c.load("CScriptXmlInit.ParseShTexInfo", &name)? {
            let _ = c.files.borrow_mut().textures.add_file(&document);
        }
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method_with_objects("InitWindow", move |r: &mut Reader, o, args| {
        c.stopped()?;
        let function = "CScriptXmlInit.InitWindow";
        let path = text_arg(args, 0, function)?;
        let index = number_arg(args, 1, function)?.trunc();
        let window = window_arg(o, args, 2, function)?;
        let index = (index >= 0.0).then_some(index as usize);
        let e = element(&c, r, function, &path, index)?;
        // Believed a crash (3.3; ours, a fatal error).
        let window =
            window.ok_or_else(|| c.stop(function, line!(), "the window is nil", &[&path]))?;
        let mut files = c.files.borrow_mut();
        let w = WindowDesc::read(e, &files.colors)
            .map_err(|err| c.stop(function, line!(), &err.to_string(), &[&path]))?;
        let target = &mut files.windows[window];
        target.rect = w.rect();
        if w.font.is_some() {
            target.font = w.font.clone();
        }
        if w.window_name.is_some() {
            target.name = w.window_name.clone();
        }
        add_auto_statics(&mut files, window, &w.auto_statics, e);
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method_with_objects("InitAutoStaticGroup", move |r: &mut Reader, o, args| {
        c.stopped()?;
        let function = "CScriptXmlInit.InitAutoStaticGroup";
        let path = text_arg(args, 0, function)?;
        let parent = window_arg(o, args, 1, function)?;
        // TODO(#1145): no element at the path does nothing (the simplest
        // reading); a document without a root still stops the game.
        let e = if r.document.root().is_none() {
            Some(element(&c, r, function, &path, Some(0))?)
        } else {
            r.document.find(&path, 0)
        };
        let Some(e) = e else {
            return Ok(Vec::new());
        };
        let mut statics = Vec::new();
        for s in e.children_named("auto_static") {
            let colors = c.files.borrow().colors.clone();
            let s = StaticDesc::read(s, &colors)
                .map_err(|err| c.stop(function, line!(), &err.to_string(), &[&path]))?;
            statics.push(s);
        }
        if statics.is_empty() {
            return Ok(Vec::new());
        }
        // Believed a crash on the first child (3.3; ours, a fatal error).
        let parent =
            parent.ok_or_else(|| c.stop(function, line!(), "the parent is nil", &[&path]))?;
        add_auto_statics(&mut c.files.borrow_mut(), parent, &statics, e);
        Ok(Vec::new())
    });
    for m in &MAKERS {
        let c = ctx.clone();
        class = class.method_with_objects(m.method, move |r: &mut Reader, o, args| {
            init(&c, m, r, o, args)
        });
    }
    class
}

/// `TEX_INFO` (5.2).
fn tex_info_class() -> EngineClass {
    EngineClass::new("TEX_INFO")
        .method("get_file_name", |t: &mut TexInfo, _| {
            // Believed: no text for an empty file name.
            Ok(vec![if t.file.is_empty() {
                Value::Nil
            } else {
                lua_text(&t.file)
            }])
        })
        .method_with_objects("get_rect", |t: &mut TexInfo, o, _| {
            Ok(vec![o.new_object("Frect", t.rect)])
        })
}

/// `CUIWindow` and `CUIScriptWnd`, constructible as windows of their own
/// (scripts derive from them and pass their objects as windows, 3.3), and
/// the classes of [`WINDOW_CLASSES`]. Their members and the rest of their
/// hierarchy are G1's and G2's (TODO(#207), TODO(#208)).
fn window_classes(ctx: &Context) -> Vec<EngineClass> {
    let constructed = |kind: WindowKind| {
        let c = ctx.clone();
        move |_: EnginePart, _: &[Value]| -> Result<WindowPart, String> {
            Ok(WindowPart(c.files.borrow_mut().add(Window::new(kind))))
        }
    };
    let mut classes = vec![
        EngineClass::new("CUIWindow").constructor(0, |_| Some(0), constructed(WindowKind::Window)),
        EngineClass::new("CUIScriptWnd")
            .base("CUIWindow")
            .constructor(0, |_| Some(0), constructed(WindowKind::ScriptWindow)),
    ];
    classes.extend(
        WINDOW_CLASSES
            .iter()
            .map(|name| EngineClass::new(name).base("CUIWindow")),
    );
    classes
}

/// Moves the hidden registration into `game`, made unless another spec
/// made it.
const NAMESPACES: &[u8] = br#"
local game = rawget(_G, "game")
if game == nil then
  game = {}
  rawset(_G, "game", game)
end
game.translate_string = rawget(_G, "__volya_window_files_translate_string")
rawset(_G, "__volya_window_files_translate_string", nil)
"#;

fn texture<'a>(
    c: &Context,
    textures: &'a TextureDescriptions,
    function: &str,
    id: &str,
) -> Result<&'a TextureDesc, String> {
    textures
        .get(id)
        .ok_or_else(|| c.stop(function, line!(), "Can't find texture", &[id]))
}

fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    for class in window_classes(ctx) {
        scripts.register_class(class)?;
    }
    scripts.register_class(reader_class(ctx))?;
    let mut font = EngineClass::new("CGameFont");
    for (name, value) in [("alLeft", 0), ("alRight", 1), ("alCenter", 2)] {
        font = font.constant(name, value);
    }
    scripts.register_class(font)?;
    scripts.register_class(tex_info_class())?;
    let lua = scripts.lua_mut();
    for (name, font) in FONT_FUNCTIONS {
        let c = ctx.clone();
        lua.register_with_objects(name, move |maker: &ObjectMaker, _| {
            c.stopped()?;
            // Before the UI's fonts exist: unestablished, believed a crash
            // (4.1; ours, a fatal error).
            if !c.files.borrow().fonts_created {
                return Err(c.stop(name, line!(), "the UI fonts are not created", &[]));
            }
            Ok(vec![maker.new_object("CGameFont", GameFont(font))])
        })?;
    }
    let c = ctx.clone();
    lua.register("GetARGB", move |args| {
        c.stopped()?;
        let mut n = [0.0; 4];
        for (i, n) in n.iter_mut().enumerate() {
            *n = number_arg(args, i, "GetARGB")?;
        }
        Ok(vec![f64::from(argb(n[0], n[1], n[2], n[3])).into()])
    })?;
    let c = ctx.clone();
    lua.register("GetTextureName", move |args| {
        c.stopped()?;
        let id = text_arg(args, 0, "GetTextureName")?;
        let files = c.files.borrow();
        let d = texture(&c, &files.textures, "GetTextureName", &id)?;
        Ok(vec![lua_text(&d.file)])
    })?;
    let c = ctx.clone();
    lua.register_with_objects("GetTextureRect", move |maker, args| {
        c.stopped()?;
        let id = text_arg(args, 0, "GetTextureRect")?;
        let files = c.files.borrow();
        let d = texture(&c, &files.textures, "GetTextureRect", &id)?;
        Ok(vec![maker.new_object("Frect", texture_rect(d))])
    })?;
    let c = ctx.clone();
    lua.register_with_objects("GetTextureInfo", move |maker, args| {
        c.stopped()?;
        let id = text_arg(args, 0, "GetTextureInfo")?;
        let default_id = text_arg(args, 1, "GetTextureInfo")?;
        let files = c.files.borrow();
        let d = match files.textures.get(&id) {
            Some(d) => d,
            None => texture(&c, &files.textures, "GetTextureInfo", &default_id)
                .map_err(|_| c.stop("GetTextureInfo", line!(), "Can't find texture", &[&id]))?,
        };
        let info = TexInfo {
            file: d.file.clone(),
            rect: texture_rect(d),
        };
        Ok(vec![maker.new_object("TEX_INFO", info)])
    })?;
    let c = ctx.clone();
    lua.register("__volya_window_files_translate_string", move |args| {
        c.stopped()?;
        let what = "game.translate_string";
        let key = text_arg(args, 0, what)?;
        // The "has no entry" log line's setting is unestablished (6): the
        // line is never written.
        Ok(vec![lua_text(c.strings(what)?.get(&key))])
    })?;
    lua.exec("=window_files", NAMESPACES).map(drop)
}

impl Scripts {
    /// Registers the window description files' names (1) over `files`,
    /// reading the game's files over `trees` and reporting fatal errors to
    /// the console of `services`: before [`Scripts::load_globals`].
    pub fn register_window_files(
        &mut self,
        services: Rc<RefCell<EngineServices>>,
        files: Rc<RefCell<WindowFiles>>,
        trees: Rc<dyn Trees>,
        aliases: PathAliases,
    ) -> Result<(), Error> {
        let ctx = Context {
            services,
            files,
            trees,
            aliases,
            fatal: self.fatal_flag(),
        };
        register(self, &ctx)
    }
}
