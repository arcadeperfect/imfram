//! How a tile and a section header look. Colours from `Visuals`.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Rect, Stroke, StrokeKind, Ui};

use super::thumbs::Shown;

pub(super) const RADIUS: u8 = 4;
const BADGE_PAD: f32 = 3.0;
const SCRUB_H: f32 = 2.0;

pub(super) struct Look {
    pub text: Color32,
    pub weak: Color32,
    pub well: Color32,
    pub sel_fill: Color32,
    pub sel_stroke: Stroke,
    pub sel_text: Color32,
    pub hover: Stroke,
    pub cursor: Stroke,
    pub badge_bg: Color32,
    pub badge_text: Color32,
    pub font: FontId,
    pub small: FontId,
}

impl Look {
    pub fn from(ui: &Ui) -> Self {
        let v = ui.visuals();
        Self {
            text: v.text_color(),
            weak: v.weak_text_color(),
            well: v.extreme_bg_color,
            sel_fill: v.selection.bg_fill,
            sel_stroke: Stroke::new(2.0, v.selection.bg_fill),
            sel_text: v.selection.stroke.color,
            hover: Stroke::new(1.0, v.widgets.hovered.bg_stroke.color),
            cursor: Stroke::new(1.0, v.text_color().gamma_multiply(0.6)),
            badge_bg: Color32::from_black_alpha(170),
            badge_text: Color32::from_gray(230),
            font: egui::TextStyle::Body.resolve(ui.style()),
            small: egui::TextStyle::Small.resolve(ui.style()),
        }
    }
}

pub(super) struct Tile<'a> {
    pub label: &'a str,
    pub kind: Option<&'a str>,
    pub badge: Option<&'a str>,
    pub selected: bool,
    pub hovered: bool,
    pub cursor: bool,
    pub dim: bool,
    pub failed: bool,
    pub fill: bool,
    /// Scrubbing: where along the clip (0..1) and what to call the frame.
    pub scrub: Option<(f32, String)>,
}

/// Where the image goes in `area` and which part of it: whole and
/// letterboxed, or cropped to fill.
pub fn fit(area: Rect, size: [usize; 2], fill: bool) -> (Rect, Rect) {
    let full_uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if size[0] == 0 || size[1] == 0 {
        return (area, full_uv);
    }
    let a = size[0] as f32 / size[1] as f32;
    let b = area.width() / area.height();
    if fill {
        let uv = if a > b {
            let w = b / a;
            Rect::from_min_max(pos2(0.5 - w / 2.0, 0.0), pos2(0.5 + w / 2.0, 1.0))
        } else {
            let h = a / b;
            Rect::from_min_max(pos2(0.0, 0.5 - h / 2.0), pos2(1.0, 0.5 + h / 2.0))
        };
        (area, uv)
    } else {
        let s = if a > b { vec2(area.width(), area.width() / a) } else { vec2(area.height() * a, area.height()) };
        (Rect::from_center_size(area.center(), s), full_uv)
    }
}

pub(super) fn tile(p: &Painter, cell: Rect, area: Rect, shown: Option<&Shown>, t: &Tile, look: &Look) {
    let r = CornerRadius::same(RADIUS);
    thumb(p, area, shown, t, look);
    if t.selected {
        p.rect_stroke(area, r, look.sel_stroke, StrokeKind::Outside);
    } else if t.hovered {
        p.rect_stroke(area, r, look.hover, StrokeKind::Outside);
    }
    if t.cursor && !t.selected {
        p.rect_stroke(area.expand(2.0), r, look.cursor, StrokeKind::Outside);
    }
    let label_rect = Rect::from_min_max(pos2(cell.left(), area.bottom()), cell.right_bottom());
    if label_rect.height() > 4.0 && !t.label.is_empty() {
        label(p, label_rect, t.label, t.selected, look);
    }
}

/// The picture box alone: the well, the image (or what it is), the corner
/// badge, the scrub bar and frame name. The list draws its thumbnails with it.
pub(super) fn thumb(p: &Painter, area: Rect, shown: Option<&Shown>, t: &Tile, look: &Look) {
    let r = CornerRadius::same(RADIUS);
    p.rect_filled(area, r, look.well);
    match shown {
        Some(s) => {
            let (img, uv) = fit(area, s.size, t.fill);
            let tint = if t.dim { Color32::from_gray(110) } else { Color32::WHITE };
            p.with_clip_rect(area.intersect(p.clip_rect())).image(s.tex.id(), img, uv, tint);
        }
        None => {
            let what = match t.kind {
                Some(k) => k,
                None if t.failed => "no preview",
                None => "",
            };
            p.text(area.center(), Align2::CENTER_CENTER, what, look.small.clone(), look.weak);
        }
    }
    if let Some(b) = t.badge.filter(|_| t.scrub.is_none()) {
        badge(p, area.right_bottom() - vec2(BADGE_PAD, BADGE_PAD), Align2::RIGHT_BOTTOM, b, look);
    }
    if let Some((frac, name)) = &t.scrub {
        let y = area.bottom() - SCRUB_H;
        p.rect_filled(Rect::from_min_max(pos2(area.left(), y), area.right_bottom()), 0.0, look.badge_bg);
        let x = area.left() + area.width() * frac.clamp(0.0, 1.0);
        p.rect_filled(Rect::from_min_max(pos2(area.left(), y), pos2(x, area.bottom())), 0.0, look.sel_fill);
        badge(p, area.right_bottom() - vec2(BADGE_PAD, BADGE_PAD + SCRUB_H), Align2::RIGHT_BOTTOM, name, look);
    }
}

fn label(p: &Painter, rect: Rect, text: &str, selected: bool, look: &Look) {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat { font_id: look.small.clone(), color: if selected { look.sel_text } else { look.text }, ..Default::default() });
    job.wrap = TextWrapping::truncate_at_width(rect.width() - 4.0);
    let g = p.layout_job(job);
    let at = pos2(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0);
    if selected {
        let pill = Rect::from_min_size(at, g.size()).expand2(vec2(4.0, 1.0));
        p.rect_filled(pill, CornerRadius::same(RADIUS), look.sel_fill);
    }
    p.galley(at, g, look.text);
}

fn badge(p: &Painter, at: egui::Pos2, align: Align2, text: &str, look: &Look) {
    let g = p.layout_no_wrap(text.to_owned(), look.small.clone(), look.badge_text);
    let rect = align.anchor_size(at, g.size() + vec2(2.0 * BADGE_PAD, 0.0));
    p.rect_filled(rect, CornerRadius::same(3), look.badge_bg);
    p.galley(rect.min + vec2(BADGE_PAD, 0.0), g, look.badge_text);
}

pub(super) fn header(p: &Painter, rect: Rect, text: &str, gap: f32, look: &Look) {
    let at = pos2(rect.left() + gap, rect.center().y);
    let g = p.layout_no_wrap(text.to_owned(), look.font.clone(), look.text);
    let w = g.size().x;
    p.galley(pos2(at.x, at.y - g.size().y / 2.0), g, look.text);
    let x0 = at.x + w + gap;
    if x0 < rect.right() - gap {
        p.hline(x0..=rect.right() - gap, at.y, Stroke::new(1.0, look.weak.gamma_multiply(0.4)));
    }
}
