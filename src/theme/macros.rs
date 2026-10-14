//! The [`slots!`](crate::slots) macro: one invocation declares an app's
//! whole colour table and generates its API. Every path inside the
//! expansion is `$crate::theme::…`, so an app invokes it from its own crate.

/// Declares an app's theme slots and generates, in the invoking module:
///
/// * `enum Slot` — one variant per slot; the variant name is its prefs id
/// * `DEFS: &[Def]`, `N`, `ALL: [Slot; N]`, and `TABLE: Table` (the engine
///   handle: the resolved live colours, the overrides, a generation counter)
/// * `get(Slot)`, `set(Slot, Color32)`, `reset(Slot)`, `reset_all()`,
///   `is_overridden(Slot)`, `any_overridden()`
/// * `overrides() -> BTreeMap<String, String>` / `install(&map)` — prefs
///   hold only the overrides, as hex
/// * `write_visuals(&mut Visuals)` / `sync(&Context)` — egui follows the
///   interface slots
/// * one accessor per slot, named after it: `bg()`, `accent()`, …
/// * the table literals `rgb` `gray` `rgba` (imported) and the links
///   `like(Slot)` / `shade(Slot, Base)` (generated), so a default reads
///   `shade(Slot::bg, gray(10))`
///
/// Syntax: optional `core;` (the crate's interface / status / canvases
/// groups, spliced in first so egui's `Visuals` are driven) followed by any
/// number of groups:
///
/// ```ignore
/// imfram::slots! {
///     core;
///     "node graph" {
///         /// Shown on hover in the colours window.
///         graph_bg: "background" = shade(Slot::bg, gray(0x14));
///         wire: "wires" [alpha] = rgb(0xb8, 0x8a, 0x40);
///         wire_held: "wire held" = like(Slot::highlight);
///     }
/// }
/// ```
///
/// Links must point to an EARLIER slot (a test on the table holds that).
/// A process has one live table: an app's superset table drives the
/// crate's widgets through `Visuals`, never through the crate's own core
/// accessors.
#[macro_export]
macro_rules! slots {
    (@alpha $base:expr) => { !$crate::theme::default_rgb($base).is_opaque() };
    (@alpha $base:expr, alpha) => { true };

    (core; $($rest:tt)*) => {
        $crate::slots! { @table
            "interface" {
                /// Panels, and egui's frame fill behind non-interactive widgets.
                bg: "background" = gray(27);
                /// Floating windows and popups.
                window: "windows" = shade(Slot::bg, gray(27));
                /// Text fields, drag values and other sunk wells.
                field: "fields" = shade(Slot::bg, gray(10));
                /// Separators, window outlines, indentation lines.
                border: "borders" = gray(60);
                /// Outline of a hovered box.
                border_hover: "borders, hovered" = shade(Slot::border, gray(0x6a));
                /// Labels — every hover/active text shade egui keeps follows it.
                text: "text" = gray(140);
                /// Hints and placeholders.
                text_weak: "text, faint" = shade(Slot::text, gray(90));
                /// Labels painted on canvases and HUDs that must read clearly.
                text_strong: "text, bright" = shade(Slot::text, gray(0xd8));
                /// A hovered or active painted label.
                text_hot: "text, highlighted" = shade(Slot::text, gray(0xff));
                /// Button, checkbox and slider-handle fill.
                button: "buttons" = gray(60);
                button_hover: "buttons, hovered" = shade(Slot::button, gray(70));
                button_active: "buttons, pressed" = shade(Slot::button, gray(55));
                /// Selected text, selectable labels, toggled buttons.
                accent: "accent" = rgb(0, 92, 128);
                /// Text and outlines on the accent; the text cursor.
                accent_text: "accent text" = rgb(192, 222, 255);
                link: "links" = shade(Slot::accent_text, rgb(90, 170, 255));
                /// Outline of a selected thing on any canvas: nodes, clips, keys.
                selection: "selection" = gray(0xff);
                /// An on or pinned state that is not a selection: the hud toggle,
                /// X-pinned handles, the tool you are inside.
                highlight: "highlight" = rgb(0xd8, 0xa0, 0x30);
                /// A hovered row, tile or keycap, laid over whatever it sits on.
                hover_fill: "hover fill" = rgba(0xff, 0xff, 0xff, 12);
                /// Faintly raised: idle cards, striped rows, inset boxes.
                raised_fill: "raised fill" = shade(Slot::hover_fill, rgba(0xff, 0xff, 0xff, 5));
                /// The plate behind floating readouts, chips, pies and scrims. Each
                /// site keeps its own opacity; this colour's alpha scales them all.
                hud_bg: "HUD backdrop" [alpha] = gray(0);
                /// Drop shadow under floating boxes.
                shadow: "shadows" = shade(Slot::hud_bg, rgba(0, 0, 0, 96));
            }
            "status" {
                ok: "ok" = rgb(0x6c, 0xc2, 0x8a);
                /// Neutral state worth a colour.
                info: "info" = rgb(110, 150, 200);
                warn: "warning" = rgb(0xff, 0xc4, 0x4d);
                error: "error" = rgb(0xe0, 0x60, 0x50);
            }
            "canvases" {
                /// The dark well behind a plot, a curve editor, a poster.
                well: "wells" = shade(Slot::field, gray(0x12));
                grid_line: "grid lines" = shade(Slot::well, gray(0x22));
                /// Zero, neutral and identity lines.
                axis_line: "axis lines" = shade(Slot::well, gray(0x3a));
                /// Pucks, knobs, points and box handles.
                handle: "handles" = gray(0xd4);
                handle_hot: "handles, hovered" = shade(Slot::handle, gray(0xff));
                /// The dark rim that keeps a handle readable on anything.
                handle_rim: "handle rims" = shade(Slot::hud_bg, gray(0x14));
                marquee: "box select" = shade(Slot::handle, gray(0x9a));
                marquee_fill: "box select fill" = shade(Slot::hover_fill, rgba(0xff, 0xff, 0xff, 6));
                /// Outline of a scalable selection and the rotate stalk.
                selection_box: "selection box" = shade(Slot::border, gray(0x55));
                /// A parameter curve with no channel colour of its own.
                curve_line: "curves" = like(Slot::handle);
                /// Scalar channels in the animation curve editor.
                curve_scalar: "animation curves" = shade(Slot::highlight, rgb(0xc8, 0xb0, 0x60));
                curve_selected: "selected points" = like(Slot::warn);
            }
            $($rest)*
        }
    };

    (@table $( $group:literal {
        $( $(#[doc = $doc:literal])* $id:ident : $label:literal $([$flag:ident])? = $base:expr ; )*
    } )*) => {
        /// One themable colour. The variant name is its prefs id.
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Slot { $($( $id, )*)* }

        #[allow(unused_imports)]
        use $crate::theme::lit::{gray, rgb, rgba};
        /// The same colour as `s`, whatever that is.
        #[allow(dead_code)]
        const fn like(s: Slot) -> $crate::theme::Base {
            $crate::theme::Base::Like(s as usize)
        }
        /// `own`, shifted by however far `s` has moved from its default.
        #[allow(dead_code)]
        const fn shade(s: Slot, own: $crate::theme::Base) -> $crate::theme::Base {
            $crate::theme::shade_of(s as usize, own)
        }

        pub const DEFS: &[$crate::theme::Def] = &[
            $($( $crate::theme::Def {
                id: stringify!($id),
                group: $group,
                label: $label,
                tip: concat!($( $doc, "\n", )* ""),
                base: $base,
                alpha: $crate::slots!(@alpha $base $(, $flag)?),
            }, )*)*
        ];

        #[allow(dead_code)]
        pub const N: usize = DEFS.len();

        #[allow(dead_code)]
        pub const ALL: [Slot; N] = [ $($( Slot::$id, )*)* ];

        static LIVE: [::std::sync::atomic::AtomicU32; N] = [
            $($( ::std::sync::atomic::AtomicU32::new(
                $crate::theme::pack($crate::theme::default_of(DEFS, Slot::$id as usize))
            ), )*)*
        ];
        static OVERRIDES: $crate::theme::Overrides = $crate::theme::Overrides::new(Vec::new());
        static GENERATION: ::std::sync::atomic::AtomicU32 = ::std::sync::atomic::AtomicU32::new(1);

        /// The engine handle over this table: what the colours window edits.
        pub static TABLE: $crate::theme::Table = $crate::theme::Table {
            defs: DEFS,
            live: &LIVE,
            overrides: &OVERRIDES,
            generation: &GENERATION,
        };

        // An app's module may not reach every one of these.
        #[allow(dead_code)]
        #[inline]
        pub fn get(s: Slot) -> $crate::theme::Color32 {
            TABLE.get(s as usize)
        }
        /// Recolour a slot. Setting it to what it would inherit anyway
        /// REMOVES the override, so prefs stay a diff and the link comes back.
        #[allow(dead_code)]
        pub fn set(s: Slot, c: $crate::theme::Color32) {
            TABLE.set(s as usize, c)
        }
        #[allow(dead_code)]
        pub fn reset(s: Slot) {
            TABLE.reset(s as usize)
        }
        #[allow(dead_code)]
        pub fn reset_all() {
            TABLE.reset_all()
        }
        #[allow(dead_code)]
        pub fn is_overridden(s: Slot) -> bool {
            TABLE.is_overridden(s as usize)
        }
        #[allow(dead_code)]
        pub fn any_overridden() -> bool {
            TABLE.any_overridden()
        }
        /// The overrides as prefs strings, ready to serialize.
        #[allow(dead_code)]
        pub fn overrides() -> ::std::collections::BTreeMap<String, String> {
            TABLE.overrides()
        }
        /// Replace the overrides from prefs strings.
        #[allow(dead_code)]
        pub fn install(map: &::std::collections::BTreeMap<String, String>) {
            TABLE.install(map)
        }
        /// egui's dark visuals, driven by the interface slots.
        #[allow(dead_code)]
        pub fn write_visuals(v: &mut $crate::theme::__egui::Visuals) {
            TABLE.write_visuals(v)
        }
        /// Bring a context's visuals up to date; cheap when nothing moved.
        #[allow(dead_code)]
        pub fn sync(ctx: &$crate::theme::__egui::Context) {
            TABLE.sync(ctx)
        }

        $($(
            $(#[doc = $doc])*
            #[inline]
            // Some slots only ever reach egui's visuals, never a call site.
            #[allow(dead_code)]
            pub fn $id() -> $crate::theme::Color32 {
                get(Slot::$id)
            }
        )*)*
    };

    ($($rest:tt)*) => {
        $crate::slots! { @table $($rest)* }
    };
}
