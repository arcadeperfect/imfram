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
    let ctx = ui.ctx().clone();
    let key = next_key(&ctx, name);
    let here = with_walk(&ctx, |w| match &w.harvest {
        None => Here::Normal,
        Some(h) => h.slots.get(&key).cloned().map_or(Here::Hidden, Here::Slot),
    });
    match here {
        Here::Hidden => body(ui),
        Here::Slot(slot) => {
            let mut sub = Ui::new(ctx.clone(), slot.id, UiBuilder::new().layer_id(slot.layer).max_rect(slot.rect).style(slot.style));
            sub.set_clip_rect(slot.clip);
            if !ui.is_enabled() {
                sub.disable();
            }
            crate::widgets::group::fit_sliders(&mut sub);
            let r = body(&mut sub);
            let h = sub.min_rect().height();
            with_walk(&ctx, |w| {
                if let Some(hv) = w.harvest.as_mut() {
                    hv.drawn.insert(key, h);
                }
            });
            r
        }
        Here::Normal => {
            let r = ui.scope(body);
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
