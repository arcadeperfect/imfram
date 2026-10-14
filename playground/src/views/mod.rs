//! One tab per view module in imfram, each with its own state and
//! nothing behind it. Each file is one lane: fill it in, touch nothing
//! else here.

pub mod dialog;
pub mod imageview;
pub mod keymap;
pub mod outliner;
pub mod palette;
pub mod props;
pub mod table;
pub mod theme;
pub mod timeline;
pub mod toast;

#[derive(Default)]
pub struct Views {
    pub toast: toast::State,
    pub outliner: outliner::State,
    pub table: table::State,
    pub palette: palette::State,
    pub dialog: dialog::State,
    pub timeline: timeline::State,
    pub imageview: imageview::State,
    pub props: props::State,
    pub theme: theme::State,
    pub keymap: keymap::State,
}
