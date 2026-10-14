//! Settings the simulation reads, which two machines playing together
//! must agree on: they cannot change during network play (the other side
//! would not change with them, and the simulations would part). Each such
//! setting goes in a `section`: marked "offline only" for whoever is
//! tuning or modding, and greyed out while linked.
//!
//! What only looks different (colours, light, sound, the HUD) or goes
//! across tagged with its tick (the objects panel's commands) needs none.

use std::sync::atomic::{AtomicBool, Ordering};

use egui::Ui;

/// Two machines are playing together (`net::pump` keeps it up to date).
static LOCKED: AtomicBool = AtomicBool::new(false);

const HINT: &str = "the simulation reads these, and two machines playing together must agree on them: \
                    they cannot change during network play";

pub fn set_locked(on: bool) {
    LOCKED.store(on, Ordering::Relaxed);
}

pub fn locked() -> bool {
    LOCKED.load(Ordering::Relaxed)
}

/// Settings that cannot change during network play: marked, and greyed
/// out while linked.
pub fn section<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let locked = locked();
    ui.weak(if locked { "locked: playing over the network" } else { "offline only" }).on_hover_text(HINT);
    ui.add_enabled_ui(!locked, body).inner
}
