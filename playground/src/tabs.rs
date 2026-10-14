//! The playground's tabs and windows, and what each tab shows: a widget
//! zoo with nothing behind it.

use egui::Ui;
use imfram::colorpicker::{self, PickSpec};
use imfram::dock::{self, Panels};
use imfram::filebrowser::{Action, Browser};
use imfram::fonts::Fonts;
use imfram::history::History;
use imfram::hudkeys::{self, HudKey, TopStrip};
use imfram::nav::GestureFilter;
use imfram::nodeview::{self, NodeView};
use imfram::rgbslider::{self, RgbSpec};
use imfram::textedit::{self, Code};
use imfram::tiltmenu::TiltBar;
use imfram::widgets::group::{group, switched, Start};
use imfram::widgets::keystrip::Reveal;
use imfram::widgets::{self, tipped};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Tab {
    Widgets,
    Colour,
    Text,
    Files,
    Canvas,
    Nodes,
    Keys,
    Nothing,
    Props,
    Outliner,
    Table,
    Timeline,
    Image,
    Palette,
    Dialogs,
    Toasts,
    Theme,
    Keymap,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Role {
    Main,
    Side,
    Spare,
    Views,
    Detached,
}

impl dock::Tab for Tab {
    type Role = Role;
    fn all() -> &'static [Self] {
        &[
            Tab::Widgets,
            Tab::Colour,
            Tab::Text,
            Tab::Files,
            Tab::Canvas,
            Tab::Nodes,
            Tab::Keys,
            Tab::Nothing,
            Tab::Props,
            Tab::Outliner,
            Tab::Table,
            Tab::Timeline,
            Tab::Image,
            Tab::Palette,
            Tab::Dialogs,
            Tab::Toasts,
            Tab::Theme,
            Tab::Keymap,
        ]
    }
    fn title(self) -> &'static str {
        match self {
            Tab::Widgets => "Widgets",
            Tab::Colour => "Colour",
            Tab::Text => "Text",
            Tab::Files => "Files",
            Tab::Canvas => "Canvas",
            Tab::Nodes => "Nodes",
            Tab::Keys => "Keys",
            Tab::Nothing => "Nothing",
            Tab::Props => "Props",
            Tab::Outliner => "Outliner",
            Tab::Table => "Table",
            Tab::Timeline => "Timeline",
            Tab::Image => "Image",
            Tab::Palette => "Palette",
            Tab::Dialogs => "Dialogs",
            Tab::Toasts => "Toasts",
            Tab::Theme => "Theme",
            Tab::Keymap => "Keymap",
        }
    }
    fn help(self) -> &'static str {
        match self {
            Tab::Widgets => "sliders with drag rates (hold Z X C to slow, V B N to speed), groups, number fields",
            Tab::Colour => "the RGB triple slider and the colour picker",
            Tab::Text => "a line, a paragraph, and code with a gutter; undo is snapshot history",
            Tab::Files => "the file browser",
            Tab::Canvas => "the canvas: vector shapes, a picture, a playing clip and notes on Blender's hands; G R S, box select, drop a PNG",
            Tab::Nodes => "the node view over a toy graph: wire, cut, G, magnet, splice, dots, E, T, Tab",
            Tab::Keys => "hotkey caps: the top strip and the key strip",
            Tab::Nothing => "a tab with nothing in it, for moving about",
            Tab::Props => "the properties / inspector panel pattern",
            Tab::Outliner => "a tree view: select, rename, drag to reorder",
            Tab::Table => "a table: sortable columns, virtual rows",
            Tab::Timeline => "a timeline: range, playhead, keys, scrub",
            Tab::Image => "an image viewer: zoom, pan, pixel readout",
            Tab::Palette => "a command palette: fuzzy search over actions",
            Tab::Dialogs => "modal confirm, progress, a settings frame",
            Tab::Toasts => "toasts and the status bar",
            Tab::Theme => "the theme engine: slots, links, the colours window",
            Tab::Keymap => "the key map engine: bindings, conflicts, rebinding",
        }
    }
    fn home(self) -> Role {
        match self {
            Tab::Widgets | Tab::Colour | Tab::Text | Tab::Files => Role::Main,
            Tab::Canvas | Tab::Nodes | Tab::Keys => Role::Side,
            Tab::Nothing => Role::Spare,
            _ => Role::Views,
        }
    }
}

impl dock::Role for Role {
    fn title(self) -> &'static str {
        match self {
            Role::Main => "playground",
            Role::Side => "playground — side",
            Role::Spare => "playground — spare",
            Role::Views => "playground — views",
            Role::Detached => "playground",
        }
    }
    fn size(self) -> (u32, u32) {
        match self {
            Role::Main => (1100, 720),
            Role::Side => (640, 720),
            Role::Spare => (400, 300),
            Role::Views => (900, 700),
            Role::Detached => (520, 600),
        }
    }
    fn detached() -> Self {
        Role::Detached
    }
}

pub struct Playground {
    pub bar: TiltBar,
    pub quit: bool,
    pub toggles: Vec<Tab>,
    pub zoom: f32,
    pub theme: usize,
    pub strip: bool,
    pub hidden: bool,
    pub fonts: Fonts,
    /// Changed, not yet applied (applied between pointer drags: a face
    /// that changes under the slider dragging it would chase itself).
    pub fonts_dirty: bool,
    /// The Files tab is picking a font file.
    pub font_pick: bool,
    pub font_err: Option<String>,
    // widgets
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub on: bool,
    pub count: i32,
    // colour
    pub rgb: [f32; 3],
    pub gang: bool,
    pub rgba: [f32; 4],
    pub rgba2: [f32; 4],
    // text
    pub line: String,
    pub para: String,
    pub code: String,
    pub history: History<String>,
    // files
    pub files: Browser,
    pub picked: Option<String>,
    // canvas
    pub reveal: Reveal,
    pub views: crate::views::Views,
    pub graph: nodeview::demo::Demo,
    pub nodes_view: NodeView<u64>,
    pub gestures: GestureFilter,
    pub last_edit: nodeview::Edit,
}

impl Default for Playground {
    fn default() -> Self {
        let code = "// a shader, say\nfn pass1(uv: vec2f, frag: vec2f) -> vec4f {\n    let k = 0.5;\n    return vec4f(uv.x * k, uv.y, \"not code\", 1.0);\n}\n".to_string();
        Self {
            bar: TiltBar::default(),
            quit: false,
            toggles: vec![],
            zoom: 1.0,
            theme: 0,
            strip: true,
            hidden: false,
            fonts: Fonts::default(),
            fonts_dirty: false,
            font_pick: false,
            font_err: None,
            a: 0.3,
            b: 10.0,
            c: 0.5,
            on: true,
            count: 3,
            rgb: [0.8, 0.4, 0.2],
            gang: false,
            rgba: [0.2, 0.5, 0.9, 1.0],
            rgba2: [0.9, 0.9, 0.1, 0.5],
            line: String::new(),
            para: "A paragraph that wraps.\nTwo lines of it.".into(),
            history: History::new(&code),
            code,
            files: Browser::new(std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(|| "/".into())),
            picked: None,
            reveal: Reveal::default(),
            views: Default::default(),
            graph: nodeview::demo::Demo::sample(),
            nodes_view: NodeView::default(),
            gestures: GestureFilter::default(),
            last_edit: Default::default(),
        }
    }
}

impl Playground {
    pub fn take_toggles(&mut self) -> Vec<Tab> {
        std::mem::take(&mut self.toggles)
    }
}

impl Panels<Tab> for Playground {
    fn raw_input(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput, tabs: &[Tab]) {
        if tabs.contains(&Tab::Nodes) {
            self.gestures.process(raw, nodeview::claims);
        }
    }

    fn begin(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx();
        // Zoom and fonts land between drags: a slider that rescales as it
        // is dragged moves under the pointer and chases itself.
        let settled = !ctx.input(|i| i.pointer.any_down());
        if settled && (ctx.zoom_factor() - self.zoom).abs() > 1e-3 {
            ctx.set_zoom_factor(self.zoom);
        }
        if settled && self.fonts_dirty {
            self.fonts_dirty = false;
            self.font_err = self.fonts.apply(ctx).err();
        }
        ctx.set_theme(if self.theme == 1 { egui::Theme::Light } else { egui::Theme::Dark });
    }

    fn bar(&mut self, ui: &mut Ui) {
        let menus = crate::menus::menus(self);
        let mut events = self.bar.bar(ui, &menus);
        events.extend(self.bar.floating_windows(ui.ctx(), &menus));
        crate::menus::apply(self, events);
    }

    fn tab(&mut self, ui: &mut Ui, tab: Tab) {
        match tab {
            Tab::Widgets => self.widgets(ui),
            Tab::Colour => self.colour(ui),
            Tab::Text => self.text(ui),
            Tab::Files => self.files_tab(ui),
            Tab::Canvas => self.canvas(ui),
            Tab::Nodes => self.nodes(ui),
            Tab::Keys => self.keys(ui),
            Tab::Nothing => {
                ui.centered_and_justified(|ui| ui.weak("nothing here"));
            }
            Tab::Props => crate::views::props::ui(ui, &mut self.views.props),
            Tab::Outliner => crate::views::outliner::ui(ui, &mut self.views.outliner),
            Tab::Table => crate::views::table::ui(ui, &mut self.views.table),
            Tab::Timeline => crate::views::timeline::ui(ui, &mut self.views.timeline),
            Tab::Image => crate::views::imageview::ui(ui, &mut self.views.imageview),
            Tab::Palette => crate::views::palette::ui(ui, &mut self.views.palette),
            Tab::Dialogs => crate::views::dialog::ui(ui, &mut self.views.dialog),
            Tab::Toasts => crate::views::toast::ui(ui, &mut self.views.toast),
            Tab::Theme => crate::views::theme::ui(ui, &mut self.views.theme),
            Tab::Keymap => crate::views::keymap::ui(ui, &mut self.views.keymap),
        }
    }

    fn foot(&mut self, ui: &mut Ui) {
        // (The status bar takes its room before the dock; the toasts float.)
        crate::views::toast::overlay(ui, &mut self.views.toast);
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            Tab::Widgets | Tab::Colour | Tab::Text | Tab::Props | Tab::Dialogs | Tab::Toasts => [false, true],
            _ => [false, false],
        }
    }
}

impl Playground {
    fn widgets(&mut self, ui: &mut Ui) {
        group(ui, "sliders", "hold Z X C while dragging to slow, V B N to speed", Start::Open, |ui| {
            widgets::slider(ui, &mut self.a, (0.0, 1.0), "amount", false, "a plain 0..1");
            widgets::slider(ui, &mut self.b, (0.1, 1000.0), "size", true, "a log slider");
            widgets::slider(ui, &mut self.c, (-1.0, 1.0), "bias", false, "");
        });
        switched(ui, &mut self.on, "a switched group", "off, its controls grey out", Start::Open, |ui| {
            tipped(ui, "a whole row with one tooltip", |ui| {
                ui.label("count");
                let speed = widgets::rated(ui, 0.1);
                imfram::numfield::fixed(ui, 64.0, 18.0, egui::DragValue::new(&mut self.count).speed(speed));
            });
            ui.horizontal(|ui| {
                let _ = ui.button("a button");
                let _ = ui.small_button("small");
                let mut tick = true;
                ui.checkbox(&mut tick, "tick");
            });
        });
        group(ui, "folded by default", "", Start::Folded, |ui| {
            ui.label("you found it");
        });
    }

    fn colour(&mut self, ui: &mut Ui) {
        ui.label("rgb slider (hold R G B to move one lane, shift-drag gangs)");
        rgbslider::show(ui, "zoo", &mut self.rgb, &mut self.gang, RgbSpec::scale(0.0, 2.0, [0.2126, 0.7152, 0.0722]), None, false);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("pickers");
            colorpicker::button(ui, "p1", &mut self.rgba, PickSpec::rgba());
            colorpicker::button(ui, "p2", &mut self.rgba2, PickSpec::rgba());
        });
        ui.add_space(8.0);
        let c = egui::Color32::from_rgb((self.rgb[0] * 127.5) as u8, (self.rgb[1] * 127.5) as u8, (self.rgb[2] * 127.5) as u8);
        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 4.0, c);
    }

    fn text(&mut self, ui: &mut Ui) {
        textedit::line(ui, &mut self.line, "a line of text");
        ui.add_space(4.0);
        textedit::text(ui, &mut self.para, 3, "a paragraph");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("code");
            let (u, r) = self.history.depth();
            ui.weak(format!("undo {u} · redo {r}"));
            if ui.add_enabled(self.history.can_undo(), egui::Button::new("undo")).clicked() {
                self.history.undo(&mut self.code);
            }
            if ui.add_enabled(self.history.can_redo(), egui::Button::new("redo")).clicked() {
                self.history.redo(&mut self.code);
            }
        });
        let r = textedit::code(ui, &mut self.code, &Code::default().rows(10));
        if r.changed() {
            self.history.touched();
        }
        if self.history.pending() && self.history.settled() {
            self.history.commit(&self.code);
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
            if ui.input(|i| i.modifiers.shift) {
                self.history.redo(&mut self.code);
            } else {
                self.history.undo(&mut self.code);
            }
        }
    }

    fn files_tab(&mut self, ui: &mut Ui) {
        if self.font_pick {
            ui.label("pick a .ttf or .otf for the interface");
        } else if let Some(p) = &self.picked {
            ui.label(format!("picked: {p}"));
        }
        if let Some(e) = &self.font_err {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        match self.files.ui(ui) {
            Action::Pick(p) if self.font_pick => {
                self.fonts.file = Some(p);
                self.fonts_dirty = true;
                self.font_pick = false;
                self.files.exts.clear();
            }
            Action::Pick(p) | Action::PickDir(p) => self.picked = Some(p.display().to_string()),
            Action::Close => {
                self.picked = None;
                self.font_pick = false;
                self.files.exts.clear();
            }
            Action::None => {}
        }
    }

    fn canvas(&mut self, ui: &mut Ui) {
        crate::views::canvas::ui(ui, &mut self.views.canvas, self.strip, &mut self.reveal);
    }

    fn nodes(&mut self, ui: &mut Ui) {
        let typing = ui.ctx().text_edit_focused();
        let (tab, grab, stretch, shift, tidy) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Tab) && !typing,
                i.key_pressed(egui::Key::G) && !typing,
                i.key_down(egui::Key::E) && !typing,
                i.modifiers.shift_only(),
                i.key_pressed(egui::Key::T) && !typing,
            )
        });
        let input = nodeview::Input { presses: &self.gestures.presses, releases: &self.gestures.releases, tab, grab, stretch, shift, tidy };
        let edit = self.nodes_view.show(ui, &mut self.graph, &input);
        if edit.changed || edit.moved {
            self.last_edit = edit;
        }
    }

    fn keys(&mut self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        const KEYS: &[HudKey] = &[
            hudkeys::key("G", "grab a wire", egui::Key::G),
            hudkeys::key("E", "stretch", egui::Key::E),
            hudkeys::chord("⌘Z", "undo"),
            hudkeys::chord("Tab", "search"),
            hudkeys::key("A", "select all", egui::Key::A),
        ];
        let items = hudkeys::items(ui, KEYS);
        let strip = TopStrip::new(&p, rect, &items);
        strip.draw_caps(&p);
        let hover = ui.input(|i| i.pointer.hover_pos()).is_some_and(|h| strip.fan(&p).contains(h) || strip.row().contains(h));
        let reveal = ui.ctx().animate_bool(ui.id().with("strip-reveal"), hover);
        strip.draw_words(&p, reveal);
        p.text(rect.center(), egui::Align2::CENTER_CENTER, "hover the caps along the top; hold a key to light it", egui::FontId::proportional(13.0), ui.visuals().weak_text_color());
    }
}
