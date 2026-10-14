//! The canvas camera: pan / zoom, the transformed layer the nodes draw on,
//! and the gestures the canvas claims before egui's widgets see them.

use egui::emath::TSTransform;
use egui::{vec2, LayerId, PointerButton, Pos2, Rect, Ui, UiBuilder, Vec2};

use super::style::{ZOOM_MAX, ZOOM_MIN};
use super::wiring::Cutter;
use crate::nav::{self, DragNav, Press};

enum Gesture {
    Pan,
    /// Drag-zoom about this window position.
    Zoom(Pos2),
    Cut(Cutter),
}

pub struct View {
    /// Canvas -> window.
    to_window: TSTransform,
    /// Frame the whole graph once its node sizes are known.
    fit_pending: bool,
    gesture: Option<(PointerButton, Gesture)>,
}

/// What this frame's claimed presses meant, beyond moving the camera.
#[derive(Default)]
pub struct NavOut {
    /// A pan, zoom or cut began.
    pub gesture_started: bool,
}

impl Default for View {
    fn default() -> Self {
        Self { to_window: TSTransform::IDENTITY, fit_pending: true, gesture: None }
    }
}

impl View {
    /// Re-frame the whole graph next frame.
    pub fn request_fit(&mut self) {
        self.fit_pending = true;
    }

    /// Zoom and pan.
    pub fn transform(&self) -> (f32, Vec2) {
        (self.to_window.scaling, self.to_window.translation)
    }

    pub fn scale(&self) -> f32 {
        self.to_window.scaling
    }

    pub fn to_canvas(&self, window: Pos2) -> Pos2 {
        self.to_window.inverse() * window
    }

    pub fn to_window(&self, canvas: Pos2) -> Pos2 {
        self.to_window * canvas
    }

    /// The ui the nodes draw into: its own layer, transformed as a whole
    /// and clipped to `outer`.
    pub fn canvas_ui(&self, ui: &mut Ui, outer: Rect) -> Ui {
        let layer = LayerId::new(ui.layer_id().order, ui.id().with("canvas"));
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        ui.ctx().set_transform_layer(layer, self.to_window);
        let mut canvas = ui.new_child(UiBuilder::new().layer_id(layer).max_rect(Rect::from_min_size(Pos2::ZERO, Vec2::splat(4096.0))));
        canvas.set_clip_rect(self.to_window.inverse() * outer);
        canvas
    }

    /// Start and end claimed gestures, and apply pan / zoom (drags, wheel,
    /// trackpad) for this frame.
    pub fn navigate(&mut self, ui: &Ui, outer: Rect, hovered: bool, presses: &[Press], releases: &[PointerButton]) -> NavOut {
        let mut out = NavOut::default();
        for press in presses {
            if self.gesture.is_some() || !outer.contains(press.pos) {
                continue;
            }
            let p = self.to_canvas(press.pos);
            let gesture = match nav::drag_nav(press.button, press.modifiers) {
                Some(DragNav::Pan) => Gesture::Pan,
                Some(DragNav::Zoom) => Gesture::Zoom(press.pos),
                None if press.modifiers.ctrl => Gesture::Cut(Cutter::start(p)),
                // Nothing else is claimed (see `claims`).
                None => continue,
            };
            self.gesture = Some((press.button, gesture));
            out.gesture_started = true;
        }
        if self.gesture.as_ref().is_some_and(|(b, _)| releases.contains(b)) {
            self.gesture = None;
        }

        let delta = ui.input(|i| i.pointer.delta());
        match &self.gesture {
            Some((_, Gesture::Pan)) => self.to_window.translation += delta,
            Some((_, Gesture::Zoom(anchor))) => self.zoom_about(*anchor, nav::drag_zoom(delta)),
            _ => {}
        }

        if hovered {
            let wheel = nav::wheel_nav(ui);
            self.to_window.translation += wheel.pan;
            if wheel.zoom != 1.0 {
                if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
                    self.zoom_about(p, wheel.zoom);
                }
            }
        }
        out
    }

    /// Zoom by `factor`, keeping the canvas point under window position `p` fixed.
    fn zoom_about(&mut self, p: Pos2, factor: f32) {
        let t = &mut self.to_window;
        let scale = (t.scaling * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let anchor = (p.to_vec2() - t.translation) / t.scaling;
        t.translation = p.to_vec2() - anchor * scale;
        t.scaling = scale;
    }

    /// If a fit was requested, frame `bounds` (canvas space) inside `outer`:
    /// padded, and never zoomed in past 1:1.
    pub fn fit_if_pending(&mut self, ui: &Ui, outer: Rect, bounds: Rect) {
        if !self.fit_pending || !bounds.is_positive() {
            return;
        }
        let padded = bounds.size() + vec2(120.0, 120.0);
        let scale = (outer.width() / padded.x).min(outer.height() / padded.y).clamp(ZOOM_MIN, 1.0);
        self.to_window = TSTransform::new(outer.center().to_vec2() - bounds.center().to_vec2() * scale, scale);
        self.fit_pending = false;
        ui.ctx().request_repaint();
    }

    /// The wire cutter, while a ctrl-drag is in progress.
    pub fn cutter(&mut self) -> Option<&mut Cutter> {
        match &mut self.gesture {
            Some((_, Gesture::Cut(cutter))) => Some(cutter),
            _ => None,
        }
    }
}
