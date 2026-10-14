//! Text input: a line, a paragraph, or code. Code gets a monospace face, a
//! line-number gutter, tabs as spaces and a small pluggable lexer that tints
//! comments, strings, numbers and keywords. The default lexer is C-like,
//! which fits WGSL, expressions and JSON well enough; an app hands in its
//! own [`Lexer`] for anything else. Colours come from the context's
//! `Visuals`, so a theme reaches here without this module knowing it.

use egui::text::LayoutJob;
use egui::{Color32, FontId, Response, TextFormat, Ui};

/// One field of text on a line.
pub fn line(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY).show(ui).response.response
}

/// A paragraph, `rows` lines tall, wrapping.
pub fn text(ui: &mut Ui, text: &mut String, rows: usize, hint: &str) -> Response {
    egui::TextEdit::multiline(text).hint_text(hint).desired_rows(rows).desired_width(f32::INFINITY).show(ui).response.response
}

/// What a lexer calls a span of code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Plain,
    Comment,
    String,
    Number,
    Keyword,
    /// A type or a builtin: tinted a little differently from a keyword.
    Type,
    Punct,
}

/// Splits code into tinted spans: `(byte range end, kind)` for each span
/// in order, covering the whole text.
pub type Lexer = fn(&str) -> Vec<(usize, Kind)>;

/// How a code field looks and behaves.
#[derive(Clone)]
pub struct Code {
    pub line_numbers: bool,
    pub font_size: f32,
    /// Spaces a Tab inserts (0: a real tab).
    pub tab_width: usize,
    pub rows: usize,
    pub lexer: Lexer,
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
}

impl Default for Code {
    fn default() -> Self {
        Self { line_numbers: true, font_size: 12.0, tab_width: 4, rows: 12, lexer: lex_clike, keywords: CLIKE_KEYWORDS, types: CLIKE_TYPES }
    }
}

impl Code {
    pub fn rows(mut self, n: usize) -> Self {
        self.rows = n;
        self
    }
    pub fn plain(mut self) -> Self {
        self.lexer = lex_plain;
        self
    }
    pub fn lexer(mut self, l: Lexer) -> Self {
        self.lexer = l;
        self
    }
    pub fn words(mut self, keywords: &'static [&'static str], types: &'static [&'static str]) -> Self {
        self.keywords = keywords;
        self.types = types;
        self
    }
}

/// A code field. Returns the editor's response (`changed()` when typed in).
pub fn code(ui: &mut Ui, text: &mut String, cfg: &Code) -> Response {
    let font = FontId::monospace(cfg.font_size);
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    let vis = ui.visuals().clone();
    let tint = Tints::from(&vis);
    let words = (cfg.keywords, cfg.types);
    let lexer = cfg.lexer;
    let fnt = font.clone();
    let mut layouter = move |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let s = buf.as_str();
        let mut job = LayoutJob::default();
        job.wrap.max_width = wrap;
        let mut at = 0;
        for (end, kind) in lexer(s) {
            let end = end.min(s.len());
            if end <= at {
                continue;
            }
            let piece = &s[at..end];
            let kind = refine(piece, kind, words);
            job.append(piece, 0.0, TextFormat { font_id: fnt.clone(), color: tint.of(kind), ..Default::default() });
            at = end;
        }
        if at < s.len() {
            job.append(&s[at..], 0.0, TextFormat { font_id: fnt.clone(), color: tint.of(Kind::Plain), ..Default::default() });
        }
        ui.fonts_mut(|f| f.layout_job(job))
    };

    let gutter_w = if cfg.line_numbers {
        let lines = text.lines().count().max(1);
        let digits = lines.to_string().len().max(2);
        digits as f32 * cfg.font_size * 0.62 + 10.0
    } else {
        0.0
    };

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let frame = egui::Frame::new().fill(vis.extreme_bg_color).inner_margin(egui::Margin::same(4));
        frame
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let gutter = if cfg.line_numbers {
                        let h = (cfg.rows as f32 * row_h).max(text.lines().count().max(1) as f32 * row_h);
                        Some(ui.allocate_rect(egui::Rect::from_min_size(ui.cursor().min, egui::vec2(gutter_w, h)), egui::Sense::hover()).rect)
                    } else {
                        None
                    };
                    let out = egui::TextEdit::multiline(text)
                        .font(font.clone())
                        .code_editor()
                        .desired_rows(cfg.rows)
                        .desired_width(f32::INFINITY)
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .frame(egui::Frame::NONE)
                        .show(ui);
                    if let Some(g) = gutter {
                        let p = ui.painter();
                        let top = out.galley_pos.y;
                        let n = out.galley.rows.len();
                        let mut line = 1;
                        for (i, row) in out.galley.rows.iter().enumerate() {
                            // Wrapped continuations are not new lines.
                            let starts_line = i == 0 || out.galley.rows[i - 1].ends_with_newline;
                            if starts_line {
                                let y = top + row.pos.y;
                                p.text(
                                    egui::pos2(g.max.x - 6.0, y),
                                    egui::Align2::RIGHT_TOP,
                                    line,
                                    FontId::monospace(cfg.font_size * 0.9),
                                    vis.weak_text_color(),
                                );
                                line += 1;
                            }
                        }
                        let _ = n;
                    }
                    if cfg.tab_width > 0 {
                        tabs_to_spaces(ui, &out, text, cfg.tab_width);
                    }
                    out.response.response
                })
                .inner
            })
            .inner
    })
    .inner
}

/// A Tab pressed in the focused field becomes spaces to the next stop.
fn tabs_to_spaces(ui: &Ui, out: &egui::text_edit::TextEditOutput, text: &mut String, width: usize) {
    if !out.response.has_focus() {
        return;
    }
    let Some(range) = out.cursor_range else { return };
    // (The cursor counts chars; the text is indexed in bytes.)
    let ci: usize = range.primary.index.into();
    let i = text.char_indices().nth(ci).map_or(text.len(), |(b, _)| b);
    // egui inserts the literal '\t' before we see it: swap it for spaces.
    if i > 0 && text.as_bytes()[i - 1] == b'\t' {
        let line_start = text[..i - 1].rfind('\n').map_or(0, |p| p + 1);
        let col = text[line_start..i - 1].chars().count();
        let n = width - col % width;
        text.replace_range(i - 1..i, &" ".repeat(n));
        let mut st = out.state.clone();
        let c = egui::text::CCursor::new(ci - 1 + n);
        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(c)));
        st.store(ui.ctx(), out.response.id);
    }
}

struct Tints {
    plain: Color32,
    comment: Color32,
    string: Color32,
    number: Color32,
    keyword: Color32,
    ty: Color32,
    punct: Color32,
}

impl Tints {
    fn from(v: &egui::Visuals) -> Self {
        let plain = v.text_color();
        let accent = v.selection.bg_fill;
        let warm = v.warn_fg_color;
        let hyper = v.hyperlink_color;
        Self {
            plain,
            comment: v.weak_text_color(),
            string: warm,
            number: hyper,
            keyword: accent.gamma_multiply(1.6).to_opaque(),
            ty: plain.lerp_to_gamma(accent, 0.5),
            punct: plain.gamma_multiply(0.75),
        }
    }
    fn of(&self, k: Kind) -> Color32 {
        match k {
            Kind::Plain => self.plain,
            Kind::Comment => self.comment,
            Kind::String => self.string,
            Kind::Number => self.number,
            Kind::Keyword => self.keyword,
            Kind::Type => self.ty,
            Kind::Punct => self.punct,
        }
    }
}

/// A word the lexer called plain may be a keyword or a type.
fn refine(piece: &str, kind: Kind, (keywords, types): (&[&str], &[&str])) -> Kind {
    if kind != Kind::Plain {
        return kind;
    }
    if keywords.contains(&piece) {
        Kind::Keyword
    } else if types.contains(&piece) {
        Kind::Type
    } else {
        Kind::Plain
    }
}

pub fn lex_plain(s: &str) -> Vec<(usize, Kind)> {
    vec![(s.len(), Kind::Plain)]
}

/// Comments `//` and `/* */`, strings in double quotes, numbers, words,
/// and everything else as punctuation. Words come back `Plain`; `refine`
/// sorts the keywords out so one lexer serves many word lists.
pub fn lex_clike(s: &str) -> Vec<(usize, Kind)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start = i;
        let kind = if c == b'/' && b.get(i + 1) == Some(&b'/') {
            i = s[i..].find('\n').map_or(b.len(), |p| i + p);
            Kind::Comment
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = s[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
            Kind::Comment
        } else if c == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            Kind::String
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            Kind::Number
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            Kind::Plain
        } else if c.is_ascii_whitespace() {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            Kind::Plain
        } else {
            // One or more bytes of punctuation (a UTF-8 char is taken whole).
            i += s[i..].chars().next().map_or(1, char::len_utf8);
            Kind::Punct
        };
        debug_assert!(i > start);
        out.push((i, kind));
    }
    out
}

pub const CLIKE_KEYWORDS: &[&str] = &[
    "fn", "let", "var", "const", "return", "if", "else", "for", "while", "loop", "break", "continue", "struct", "switch", "case",
    "default", "true", "false", "override", "discard", "enable", "mut", "pub", "use", "match", "impl", "in",
];
pub const CLIKE_TYPES: &[&str] = &[
    "f32", "f16", "i32", "u32", "bool", "vec2", "vec3", "vec4", "vec2f", "vec3f", "vec4f", "vec2i", "vec3i", "vec4i", "vec2u", "vec3u",
    "vec4u", "mat2x2", "mat3x3", "mat4x4", "array", "texture_2d", "sampler", "ptr", "atomic", "f64", "i64", "u64", "usize", "String",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<(&str, Kind)> {
        let mut at = 0;
        lex_clike(s)
            .into_iter()
            .map(|(end, k)| {
                let piece = &s[at..end];
                at = end;
                (piece, refine(piece, k, (CLIKE_KEYWORDS, CLIKE_TYPES)))
            })
            .collect()
    }

    #[test]
    fn the_lexer_covers_every_byte_in_order() {
        let s = "fn f(a: f32) -> vec4f { return vec4f(a, 0.5, \"s\", 1); } // end\n/* c */";
        let spans = lex_clike(s);
        assert_eq!(spans.last().unwrap().0, s.len());
        assert!(spans.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn comments_strings_numbers_keywords_and_types_are_told_apart() {
        let k = kinds("let x: f32 = 1.5; // hi\n\"str\"");
        assert!(k.contains(&("let", Kind::Keyword)));
        assert!(k.contains(&("f32", Kind::Type)));
        assert!(k.contains(&("1.5", Kind::Number)));
        assert!(k.contains(&("// hi", Kind::Comment)));
        assert!(k.contains(&("\"str\"", Kind::String)));
        assert!(k.contains(&("x", Kind::Plain)));
    }

    #[test]
    fn an_unterminated_string_or_comment_runs_to_the_end_without_panicking() {
        let _ = lex_clike("\"open");
        let _ = lex_clike("/* open");
        let _ = lex_clike("x = \"a\\");
    }
}
