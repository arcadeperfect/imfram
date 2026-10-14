//! User-remappable hotkeys, generic over an app's actions (wobble's
//! `keymap.rs` with its `Act` / `DEFS` tables taken out).
//!
//! The app declares its actions as an enum implementing [`Action`]: a
//! stable name (what prefs store), a label, the scope (pane) that owns it,
//! the default [`Binding`] and how it matches. A [`Keymap`] holds the
//! OVERRIDES only, so an action whose binding never moved follows any
//! future default, and a stale name from an old build is skipped with a
//! warning rather than wedging the map. Sites read LIVE through
//! [`Keymap::pressed`] / [`released`](Keymap::released) /
//! [`down`](Keymap::down) / [`consume`](Keymap::consume), so a rebind
//! takes effect the same frame with nothing to invalidate.
//!
//! Matching:
//!
//! * [`MatchMode::Exact`] compares the key event's OWN modifiers with
//!   `Modifiers::matches_exact`. `P` and `shift+P` are two bindings, a
//!   plain letter ignores its cmd chord cousin without guards at the call
//!   site, and — because the modifiers come off the EVENT — synthetic
//!   test input needs no global modifier state.
//! * [`MatchMode::KeyOnly`] matches the bare key, for sites that read the
//!   modifiers themselves as part of the gesture (a brush pair on shift /
//!   alt, a held swipe, play / pause matched on release).
//!
//! Conflicts are flagged WITHIN a scope only: the pane under the cursor
//! owns single-letter keys, so a graph `D` and an app `D` coexisting is
//! the design, not a clash.
//!
//! Own the map (`struct App { keys: Keymap<Act> }`), make it a `static`
//! (`new` is const), or reach the per-type global through [`global`] and
//! the free functions [`pressed`] etc. [`KeysUi`] is the rebind window: a
//! drawn keyboard lit with the chosen command's chord, a searchable list,
//! click a row's key then press the new chord (or click a cap).
//!
//! NOT lifted from wobble: `take_search` (the Tab steal for node search),
//! `num_tab_pre` (Tab between number fields) and `scrub_key_down` (the
//! viewer's held-Tab scrub). Those are wobble's own hands on the Tab key
//! and stay there; this engine reads egui's input and strips nothing.

mod board;
mod caps;
mod strings;
#[cfg(test)]
mod tests;
mod ui;

pub use strings::{display, format, parse};
pub use ui::{Fixed, KeysUi};

use egui::{Key, Modifiers};
use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

/// An app's remappable actions: one enum, every variant in [`all`](Self::all).
pub trait Action: Copy + Eq + Hash + Debug + 'static {
    /// Every action, in the order the keys window lists them.
    fn all() -> &'static [Self];
    /// Stable prefs key (`"graph.tidy"`). Never rename one that has shipped.
    fn name(self) -> &'static str;
    /// What the list and the inspector call it.
    fn label(self) -> &'static str;
    /// The pane that owns it; conflicts are checked within a scope.
    fn scope(self) -> &'static str;
    fn default(self) -> Binding;
    fn mode(self) -> MatchMode {
        MatchMode::Exact
    }
    /// A FIXED synonym key, matched with the same rule, never remapped and
    /// shown greyed in the window (Delete / Backspace, D / Escape).
    fn also(self) -> Option<Key> {
        None
    }
    /// A FIXED second chord: the default key with these modifiers fires
    /// too, while the binding is the default (select-all is ctrl+A AND ⌘A).
    fn also_mods(self) -> Option<Modifiers> {
        None
    }
}

/// A concrete key chord. `mods` is NORMALIZED (see [`norm`]): `command`
/// covers ⌘ / ctrl cross-platform, `ctrl` alone means the mac ctrl key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub mods: Modifiers,
}

const fn mods(alt: bool, ctrl: bool, shift: bool, command: bool) -> Modifiers {
    Modifiers { alt, ctrl, shift, mac_cmd: false, command }
}

impl Binding {
    pub const fn new(key: Key, mods: Modifiers) -> Self {
        Self { key, mods }
    }
    pub const fn plain(key: Key) -> Self {
        Self { key, mods: Modifiers::NONE }
    }
    pub const fn cmd(key: Key) -> Self {
        Self { key, mods: Modifiers::COMMAND }
    }
    pub const fn cmd_shift(key: Key) -> Self {
        Self { key, mods: mods(false, false, true, true) }
    }
    /// The Control key itself — on a Mac, the one beside ⌥, not ⌘.
    pub const fn ctrl(key: Key) -> Self {
        Self { key, mods: mods(false, true, false, false) }
    }
    /// Control + shift — Blender's modifier chords on a Mac too.
    pub const fn ctrl_shift(key: Key) -> Self {
        Self { key, mods: mods(false, true, true, false) }
    }
    /// ⌥ on a Mac.
    pub const fn alt(key: Key) -> Self {
        Self { key, mods: mods(true, false, false, false) }
    }
    pub const fn shift(key: Key) -> Self {
        Self { key, mods: mods(false, false, true, false) }
    }
    /// The same chord with normalized modifiers.
    pub fn normalized(self) -> Self {
        Self { key: self.key, mods: norm(self.mods) }
    }
}

/// Drop `mac_cmd` (an event detail `command` already carries) and fold the
/// non-mac ctrl mirror into `command`, so a binding captured on one
/// platform reads back on another. `ctrl` survives alone only where it is
/// a real, separate key: the mac ctrl.
pub fn norm(m: Modifiers) -> Modifiers {
    Modifiers { alt: m.alt, ctrl: m.ctrl && !m.command, shift: m.shift, mac_cmd: false, command: m.command }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MatchMode {
    /// Event modifiers must equal the binding's (`matches_exact`).
    #[default]
    Exact,
    /// Bare key; the site reads the modifiers itself.
    KeyOnly,
}

/// The overrides for one action set, and the capture flag. Every method
/// takes `&self`: the map can be a `static` or shared by reference.
pub struct Keymap<A: Action> {
    overrides: RwLock<BTreeMap<&'static str, Binding>>,
    /// True from the frame a capture is armed until the captured key is
    /// RELEASED — the app's key reads stand down for the whole press, or
    /// capturing "Q" would also fire whatever Q does (and releasing a
    /// captured Space would start playback).
    capturing: AtomicBool,
    _actions: PhantomData<fn() -> A>,
}

impl<A: Action> Default for Keymap<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Action> Keymap<A> {
    pub const fn new() -> Self {
        Self { overrides: RwLock::new(BTreeMap::new()), capturing: AtomicBool::new(false), _actions: PhantomData }
    }

    /// The scopes, in order of first appearance in [`Action::all`].
    pub fn scopes() -> Vec<&'static str> {
        let mut v: Vec<&'static str> = Vec::new();
        for a in A::all() {
            if !v.contains(&a.scope()) {
                v.push(a.scope());
            }
        }
        v
    }

    pub fn by_name(name: &str) -> Option<A> {
        A::all().iter().copied().find(|a| a.name() == name)
    }

    pub fn binding(&self, a: A) -> Binding {
        self.overrides.read().unwrap().get(a.name()).copied().unwrap_or(a.default())
    }

    /// Set a binding. Setting an action back to its default REMOVES the
    /// override, so prefs stay a diff.
    pub fn rebind(&self, a: A, b: Binding) {
        let b = b.normalized();
        let mut o = self.overrides.write().unwrap();
        if b == a.default() {
            o.remove(a.name());
        } else {
            o.insert(a.name(), b);
        }
    }

    pub fn reset(&self, a: A) {
        self.overrides.write().unwrap().remove(a.name());
    }

    pub fn reset_all(&self) {
        self.overrides.write().unwrap().clear();
    }

    pub fn is_overridden(&self, a: A) -> bool {
        self.overrides.read().unwrap().contains_key(a.name())
    }

    /// The overrides as prefs strings (name → `"cmd+shift+S"`), ready to
    /// serialize.
    pub fn overrides(&self) -> BTreeMap<String, String> {
        self.overrides.read().unwrap().iter().map(|(id, b)| (id.to_string(), format(*b))).collect()
    }

    /// Replace the overrides from prefs strings. A name this build does not
    /// know, or a string that does not parse, is skipped with a warning — a
    /// downgraded build must not wedge on a newer map.
    pub fn install(&self, map: &BTreeMap<String, String>) {
        let mut o = self.overrides.write().unwrap();
        o.clear();
        for (id, s) in map {
            let Some(a) = Self::by_name(id) else {
                log::warn!("keymap: unknown action {id:?} in prefs, skipped");
                continue;
            };
            match parse(s) {
                Some(b) if b != a.default() => {
                    o.insert(a.name(), b);
                }
                Some(_) => {} // stored default: treat as no override
                None => log::warn!("keymap: unparsable binding {s:?} for {id}, skipped"),
            }
        }
    }

    /// A rebind capture is live: key reads should stand down.
    pub fn capturing(&self) -> bool {
        self.capturing.load(Ordering::Relaxed)
    }

    pub(super) fn set_capturing(&self, on: bool) {
        self.capturing.store(on, Ordering::Relaxed);
    }

    /// The fixed second chord, while the binding is still the default (a
    /// rebind means the user chose the key; the synonym does not follow).
    pub(super) fn also_chord(&self, a: A) -> Option<Binding> {
        if self.is_overridden(a) {
            return None;
        }
        a.also_mods().map(|mods| Binding { key: a.default().key, mods })
    }

    /// Every chord that fires `a`: its binding, and the fixed second chord.
    fn chords(&self, a: A) -> Vec<Binding> {
        std::iter::once(self.binding(a)).chain(self.also_chord(a)).collect()
    }

    /// The action's key went DOWN this frame (repeats excluded; modifiers
    /// off the event, so synthetic test input needs no global state).
    pub fn pressed(&self, i: &egui::InputState, a: A) -> bool {
        let b = self.binding(a);
        let second = self.also_chord(a);
        i.events.iter().any(|e| {
            matches!(e, egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. }
                if event_hits(b, a.mode(), a.also(), *key, *modifiers)
                    || second.is_some_and(|c| *key == c.key && modifiers.matches_exact(c.mods)))
        })
    }

    /// The action's key came UP this frame. Key-only: the modifier state
    /// at release is nobody's contract.
    pub fn released(&self, i: &egui::InputState, a: A) -> bool {
        let b = self.binding(a);
        i.events.iter().any(|e| {
            matches!(e, egui::Event::Key { key, pressed: false, .. } if *key == b.key || a.also() == Some(*key))
        })
    }

    /// The action's key is HELD (key-only, for gesture guards).
    pub fn down(&self, i: &egui::InputState, a: A) -> bool {
        let b = self.binding(a);
        i.key_down(b.key) || a.also().is_some_and(|k| i.key_down(k))
    }

    /// Consume-flavoured press for the chord actions (save / open / undo):
    /// removes the event so nothing downstream sees it.
    pub fn consume(&self, i: &mut egui::InputState, a: A) -> bool {
        let b = self.binding(a);
        i.consume_key(b.mods, b.key)
            || a.also().is_some_and(|k| i.consume_key(b.mods, k))
            || self.also_chord(a).is_some_and(|c| i.consume_key(c.mods, c.key))
    }

    /// The live binding for panels and help ("⌘⇧Z" on a Mac).
    pub fn label(&self, ctx: &egui::Context, a: A) -> String {
        display(ctx, self.binding(a))
    }

    /// Pairs of actions in the SAME scope whose bindings collide. A
    /// key-only action collides on the bare key (it would swallow every
    /// modifier flavour); two exact actions collide only on the full chord.
    pub fn conflicts(&self) -> Vec<(A, A)> {
        let all = A::all();
        let mut out = Vec::new();
        for (n, &a) in all.iter().enumerate() {
            for &b in &all[n + 1..] {
                if a.scope() != b.scope() {
                    continue;
                }
                let loose = a.mode() == MatchMode::KeyOnly || b.mode() == MatchMode::KeyOnly;
                let clash = self
                    .chords(a)
                    .iter()
                    .any(|ca| self.chords(b).iter().any(|cb| ca.key == cb.key && (loose || ca.mods == cb.mods)));
                if clash {
                    out.push((a, b));
                }
            }
        }
        out
    }

    /// The action `a` collides with, if any (the first by list order).
    pub fn conflict_of(&self, a: A) -> Option<A> {
        self.conflicts().into_iter().find_map(|(x, y)| (x == a).then_some(y).or((y == a).then_some(x)))
    }
}

fn event_hits(b: Binding, mode: MatchMode, also: Option<Key>, key: Key, mods: Modifiers) -> bool {
    let key_ok = key == b.key || also == Some(key);
    key_ok
        && match mode {
            MatchMode::Exact => mods.matches_exact(b.mods),
            MatchMode::KeyOnly => true,
        }
}

// ---------------------------------------------------------------------------
// One map per action type, for the free-function ergonomics.

static GLOBALS: Mutex<Vec<(TypeId, &'static (dyn Any + Send + Sync))>> = Mutex::new(Vec::new());

/// The process-wide map for `A`, made on first use.
pub fn global<A: Action>() -> &'static Keymap<A> {
    let mut g = GLOBALS.lock().unwrap();
    let id = TypeId::of::<A>();
    if let Some((_, k)) = g.iter().find(|(t, _)| *t == id) {
        return k.downcast_ref().expect("keyed by its own TypeId");
    }
    let k: &'static Keymap<A> = Box::leak(Box::new(Keymap::new()));
    g.push((id, k));
    k
}

pub fn binding<A: Action>(a: A) -> Binding {
    global::<A>().binding(a)
}
pub fn pressed<A: Action>(i: &egui::InputState, a: A) -> bool {
    global::<A>().pressed(i, a)
}
pub fn released<A: Action>(i: &egui::InputState, a: A) -> bool {
    global::<A>().released(i, a)
}
pub fn down<A: Action>(i: &egui::InputState, a: A) -> bool {
    global::<A>().down(i, a)
}
pub fn consume<A: Action>(i: &mut egui::InputState, a: A) -> bool {
    global::<A>().consume(i, a)
}
