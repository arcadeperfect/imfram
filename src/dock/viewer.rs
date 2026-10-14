//! Where a tab's button was drawn (scripted UI self-tests read it).

use egui::Id;

use super::Tab;

pub(super) fn tab_rect_id<T: Tab>(tab: T) -> Id {
    Id::new(("dock tab rect", tab))
}

/// Where `tab`'s button was drawn last frame in its window.
pub fn tab_rect<T: Tab>(ctx: &egui::Context, tab: T) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp(tab_rect_id(tab)))
}
