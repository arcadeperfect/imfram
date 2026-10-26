//! The source side: where a control is (its key), how it is picked up, and
//! how it is drawn again into a composer's slot.
//!
//! A key is the path of [`scope`]s around a control, then its name:
//! `"Sky/Gradient/gradient follow"`. A second control with the same path in
//! one pass gets `#2`, a third `#3`. Every `widgets::group` is a scope, so
//! an app wraps only its pages.
//!
//! The walk (the path so far, the names seen) lives in the context, so
//! each OS window has its own. A harvest (a composer running its sources)
//! swaps in a fresh one and puts the window's back when done.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Align2, Context, DragAndDrop, Id, LayerId, Order, Rect, Sense, Style, Ui, UiBuilder};

/// A control picked up to go in a composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub key: String,
    /// What it is called where it was (the last part of its key).
    pub name: String,
}

/// Pick mode: on, a click on any control picks it, in any window. (A
/// static, not context data, so a composer in one OS window hears a click
/// in another.)
static PICK_MODE: AtomicBool = AtomicBool::new(false);
static PICKED: Mutex<Vec<Pick>> = Mutex::new(Vec::new());

pub fn pick_mode() -> bool {
    PICK_MODE.load(Ordering::Relaxed)
}

pub fn set_pick_mode(on: bool) {
    PICK_MODE.store(on, Ordering::Relaxed);
}

/// A control carried by an Alt-drag, for a composer in ANOTHER OS window
/// (egui's drag and drop is one window's): where the pointer is on the
/// screen, and when it was let go. The window it came from drops it as
/// usual; any other takes it if it was let go over its composer.
#[derive(Clone, Debug)]
struct Carry {
    pick: Pick,
    from: u64,
    at: Option<[f64; 2]>,
    released: Option<std::time::Instant>,
}

static CARRY: Mutex<Option<Carry>> = Mutex::new(None);

/// How long a carry let go over no composer waits to be taken.
const CARRY_KEPT: std::time::Duration = std::time::Duration::from_millis(1000);

fn carry<R>(f: impl FnOnce(&mut Option<Carry>) -> R) -> R {
    f(&mut CARRY.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A control is being carried (or was just let go): every window with a
/// composer should draw, to show the drop or take it.
pub fn carrying() -> bool {
    carry(|c| c.as_ref().is_some_and(|c| c.released.is_none_or(|t| t.elapsed() < CARRY_KEPT)))
}

/// One number per context, so a carry knows the window it came from.
fn ctx_tag(ctx: &Context) -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(Id::new("imfram-composer-ctx-tag"), || NEXT.fetch_add(1, Ordering::Relaxed)))
}

fn origin_id() -> Id {
    Id::new("imfram-screen-origin")
}

/// Where this context's window is on the screen (its inner top-left, in
/// physical pixels) and its scale: set by the dock before each frame, so a
/// drag can cross windows. Without it, a carry stays in its window.
pub fn set_screen_origin(ctx: &Context, origin: [f64; 2], scale: f64) {
    ctx.data_mut(|d| d.insert_temp(origin_id(), (origin, scale)));
}

fn to_screen(ctx: &Context, p: egui::Pos2) -> Option<[f64; 2]> {
    let (o, k): ([f64; 2], f64) = ctx.data(|d| d.get_temp(origin_id()))?;
    Some([o[0] + p.x as f64 * k, o[1] + p.y as f64 * k])
}

fn from_screen(ctx: &Context, s: [f64; 2]) -> Option<egui::Pos2> {
    let (o, k): ([f64; 2], f64) = ctx.data(|d| d.get_temp(origin_id()))?;
    Some(egui::pos2(((s[0] - o[0]) / k) as f32, ((s[1] - o[1]) / k) as f32))
}

/// A control carried from another window: what, where the pointer is in
/// this window's points, and whether it has been let go.
pub(super) fn carried_here(ctx: &Context) -> Option<(Pick, egui::Pos2, bool)> {
    let me = ctx_tag(ctx);
    let c = carry(|c| c.clone())?;
    if c.from == me || c.released.is_some_and(|t| t.elapsed() >= CARRY_KEPT) {
        return None;
    }
    Some((c.pick, from_screen(ctx, c.at?)?, c.released.is_some()))
}

/// The carry was dropped somewhere: no other window takes it too.
pub(super) fn end_carry() {
    carry(|c| *c = None);
}

/// Controls clicked in pick mode since the last call.
pub(super) fn take_picked() -> Vec<Pick> {
    std::mem::take(&mut *PICKED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Where a composer wants a key drawn this frame.
#[derive(Clone)]
pub(super) struct Slot {
    pub id: Id,
    pub rect: Rect,
    pub layer: LayerId,
    pub clip: Rect,
    pub style: Arc<Style>,
    /// The name the user gave the row, shown in place of the control's own.
    pub label: Option<String>,
}

#[derive(Default)]
pub(super) struct Harvest {
    pub slots: HashMap<String, Slot>,
    /// Each key drawn into its slot, and the height it took.
    pub drawn: HashMap<String, f32>,
}

#[derive(Default)]
struct Walk {
    pass: u64,
    stack: Vec<String>,
    seen: HashMap<String, u32>,
    harvest: Option<Harvest>,
}

fn walk_id() -> Id {
    Id::new("imfram-composer-walk")
}

fn with_walk<R>(ctx: &Context, f: impl FnOnce(&mut Walk) -> R) -> R {
    let walk: Arc<Mutex<Walk>> = ctx.data_mut(|d| d.get_temp_mut_or_default::<Arc<Mutex<Walk>>>(walk_id()).clone());
    let mut w = walk.lock().unwrap_or_else(|e| e.into_inner());
    if w.harvest.is_none() && w.pass != ctx.cumulative_pass_nr() {
        // A new pass: names count from one again. (A harvest counts from
        // its own start.)
        w.pass = ctx.cumulative_pass_nr();
        w.seen.clear();
        w.stack.clear();
    }
    f(&mut w)
}

/// Run `body` with `name` on the path of every control in it.
pub fn scope<R>(ui: &mut Ui, name: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let depth = with_walk(&ctx, |w| {
        w.stack.push(name.to_owned());
        w.stack.len()
    });
    let r = body(ui);
    with_walk(&ctx, |w| w.stack.truncate(depth - 1));
    r
}

/// Whether this is a composer drawing its sources: groups unfold (a
/// folded group's controls must still be reached) and draw no header.
pub fn harvesting(ui: &Ui) -> bool {
    with_walk(ui.ctx(), |w| w.harvest.is_some())
}

fn next_key(ctx: &Context, name: &str) -> String {
    with_walk(ctx, |w| {
        let mut key = w.stack.join("/");
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(name);
        let n = w.seen.entry(key.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            key = format!("{key}#{n}");
        }
        key
    })
}

/// Run `sources` as a harvest: every control a slot names is drawn into
/// its slot, the rest nowhere. What was drawn, and how tall.
pub(super) fn harvest(ui: &mut Ui, slots: HashMap<String, Slot>, sources: impl FnOnce(&mut Ui)) -> HashMap<String, f32> {
    let ctx = ui.ctx().clone();
    let saved = with_walk(&ctx, |w| {
        let saved = (std::mem::take(&mut w.stack), std::mem::take(&mut w.seen), w.harvest.take());
        w.harvest = Some(Harvest { slots, drawn: HashMap::new() });
        saved
    });
    // Laid out as they would be, seen nowhere and touched by nothing (a
    // clip that holds no point; still enabled, so a switched-off group's
    // controls come out greyed in their slots and the rest do not).
    let mut hidden = ui.new_child(UiBuilder::new().id_salt("composer-sources"));
    hidden.set_clip_rect(Rect::NOTHING);
    sources(&mut hidden);
    with_walk(&ctx, |w| {
        let done = w.harvest.take().map(|h| h.drawn).unwrap_or_default();
        (w.stack, w.seen, w.harvest) = saved;
        done
    })
}

enum Here {
    Normal,
    Hidden,
    Slot(Slot),
}

/// A control (or a row of them) that can go in a composer, under `name`.
/// Drawn as usual; with Alt held, or in pick mode, it lights on hover and
/// can be dragged into a composer (or, in pick mode, clicked).
pub fn item<R>(ui: &mut Ui, name: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    item_with(ui, name, |ui, _| body(ui))
}

/// Where an [`item`] is: its key, and the text it should show (its own
/// name, or the name the user gave its row in a composer).
#[derive(Clone, Copy, Debug)]
pub struct ItemAt<'a> {
    pub key: &'a str,
    pub label: &'a str,
}

/// [`item`], with the control told its key and its label: a control that
/// draws its name should draw `label`, so a renamed composer row reads as
/// the user named it.
pub fn item_with<R>(ui: &mut Ui, name: &str, body: impl FnOnce(&mut Ui, ItemAt) -> R) -> R {
    let ctx = ui.ctx().clone();
    let key = next_key(&ctx, name);
    let here = with_walk(&ctx, |w| match &w.harvest {
        None => Here::Normal,
        Some(h) => h.slots.get(&key).cloned().map_or(Here::Hidden, Here::Slot),
    });
    match here {
        Here::Hidden => body(ui, ItemAt { key: &key, label: name }),
        Here::Slot(slot) => {
            let label = slot.label.clone().unwrap_or_else(|| name.to_owned());
            let mut sub = Ui::new(ctx.clone(), slot.id, UiBuilder::new().layer_id(slot.layer).max_rect(slot.rect).style(slot.style));
            sub.set_clip_rect(slot.clip);
            if !ui.is_enabled() {
                sub.disable();
            }
            crate::widgets::group::fit_sliders(&mut sub);
            let r = body(&mut sub, ItemAt { key: &key, label: &label });
            let h = sub.min_rect().height();
            with_walk(&ctx, |w| {
                if let Some(hv) = w.harvest.as_mut() {
                    hv.drawn.insert(key, h);
                }
            });
            r
        }
        Here::Normal => {
            let r = ui.scope(|ui| body(ui, ItemAt { key: &key, label: name }));
            grab(ui, key, name, r.response.rect);
            r.inner
        }
    }
}

/// The modifier that arms every control for a drag into a composer.
pub fn armed(ui: &Ui) -> bool {
    pick_mode() || ui.input(|i| i.modifiers.alt)
}

fn grab(ui: &mut Ui, key: String, name: &str, rect: Rect) {
    if !armed(ui) || !ui.is_rect_visible(rect) {
        return;
    }
    let id = Id::new(("composer-grab", &key));
    // Registered after the control, so it is on top: armed, the control
    // itself takes nothing.
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let lit = resp.hovered() || resp.dragged();
    if lit {
        let c = ui.visuals().selection.stroke.color;
        ui.painter().rect_stroke(rect.expand(1.0), 2.0, (1.0, c), egui::StrokeKind::Outside);
        ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
    }
    let pick = Pick { key, name: name.to_owned() };
    if resp.drag_started() {
        DragAndDrop::set_payload(ui.ctx(), pick.clone());
    }
    // (Followed on the screen too, for a composer in another window.)
    if resp.dragged() || resp.drag_stopped() {
        let at = ui.ctx().pointer_latest_pos().and_then(|p| to_screen(ui.ctx(), p));
        let from = ctx_tag(ui.ctx());
        let released = resp.drag_stopped().then(std::time::Instant::now);
        carry(|c| *c = Some(Carry { pick: pick.clone(), from, at, released }));
        ui.ctx().request_repaint();
    }
    if resp.dragged() {
        if let Some(at) = ui.ctx().pointer_interact_pos() {
            let p = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, id));
            let font = egui::TextStyle::Body.resolve(ui.style());
            let galley = p.layout_no_wrap(format!("+ {name}"), font, ui.visuals().strong_text_color());
            let r = Align2::LEFT_BOTTOM.anchor_size(at + egui::vec2(12.0, -4.0), galley.size()).expand(4.0);
            p.rect_filled(r, 3.0, ui.visuals().window_fill);
            p.rect_stroke(r, 3.0, ui.visuals().window_stroke, egui::StrokeKind::Inside);
            p.galley(r.min + egui::vec2(4.0, 4.0), galley, ui.visuals().strong_text_color());
        }
    }
    if resp.clicked() && pick_mode() {
        PICKED.lock().unwrap_or_else(|e| e.into_inner()).push(pick.clone());
        ui.ctx().request_repaint();
    }
    if resp.hovered() && !resp.dragged() {
        resp.on_hover_text(format!("{}: drag into a composer{}", pick.key, if pick_mode() { ", or click" } else { "" }));
    }
}
