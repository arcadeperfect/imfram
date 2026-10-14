# imfram

The egui UI layer that wobble and graav share, so a widget built for one
is the other's too. Blender muscle memory on every canvas; one set of
controls; one window shell. The name is manifesto rule 8: *hands carry
over from wobble*.

In other frameworks' terms: an immediate-mode toolkit on egui, the way
Dear ImGui's docking branch plus a node-editor add-on serve C++ tool
authors. Not an app framework (no eframe, no widget tree, no web stack):
the app owns its winit loop and wgpu device and hands state in through
closures and traits. Docked panels are real OS windows, like Blender's
and Nuke's, not viewports inside one.

```
cargo test                 # the crate alone
cargo run -p playground    # every widget, two dock windows, a tilt menubar
cargo run -p playground -- --shot DIR   # each window once to DIR/<role>.png
```

## What is here

| module        | from   | what it owns                                                    |
|---------------|--------|-----------------------------------------------------------------|
| `shell`       | graav  | `Gpu`, `EguiLayer` (egui into any view), `UiWindow` (an OS window of egui, on-demand repaint, offscreen), `capture` (texture → PNG) |
| `dock`        | own    | panels as tabs in a split tree per tool window; drag a tab and every leaf glides to where it would sit (a live preview of the layout), let go to commit; out of the window: into another window (previewed there too) or a window of its own. `Tab` / `Role` / `Panels` are the app's |
| `tiltmenu`    | wobble | the menubar's swung dropdowns: checks, sliders, choices, submenus, actions |
| `hudkeys`     | wobble | hotkey caps fanned along a view's top                           |
| `widgets`     | graav  | `slider` with drag rates (Z X C / V B N), `group` / `switched`, `tipped` rows, `keystrip` |
| `nav`         | both   | Blender-style pan / zoom classifiers for any 2D canvas          |
| `numfield`    | wobble | a DragValue in a fixed box, decimals dropping to fit            |
| `rgbslider`   | wobble | the RGB triple slider: tinted lanes, gang, held colour-axis keys |
| `colorpicker` | wobble | swatch + popup picker (wheel, rgb, hsl, Oklch), hex, palette, eyedropper hook |
| `eyedrop`     | wobble | hover a colour control, press E, sample a wgpu texture          |
| `textedit`    | new    | a line, a paragraph, or code: gutter, tabs as spaces, a small lexer |
| `fonts`       | new    | body / code size and a user's font file, applied to a context   |
| `filebrowser` | new    | places, crumbs, filter, folders first, pick; lists off-thread   |
| `history`     | graav  | snapshot undo that settles an edit burst into one step          |
| `nodeview`    | graav  | a node canvas for any graph (`Graph` trait): wires, G-carry, ctrl-cut, magnet, splice, corner dots, E-stretch, Tab search, T tidy with glide |
| `props`       | new    | the properties sheet: aligned rows, reset dots, key dots        |
| `outliner`    | new    | a tree view: select, rename, drag to reorder or reparent        |
| `table`       | new    | sortable, resizable columns over virtual rows                   |
| `palette`     | new    | a command palette: fuzzy search over actions, recent first      |
| `dialog`      | new    | modal confirm, prompt, progress; a settings-window frame        |
| `toast` / `statusbar` | new | transient notices; a bottom strip with a flash and progress |
| `timeline`    | new    | range, playhead, keys, scrub, transport                         |
| `imageview`   | new    | an image at zoom and pan with a pixel readout                   |
| `theme`       | wobble | the slot engine: `slots!` macro, links, hex overrides, the colours window |
| `keymap`      | wobble | bindings, overrides, conflicts, the rebind keyboard window      |
| `filedialog`  | wobble | rfd on native, a stand-in on the web                            |
| `bg`          | wobble | fire-and-forget work: a thread, or inline on the web            |
| `snapshot`    | wobble | test: CPU-rasterized egui output for geometry asserts           |
| `uishot`      | wobble | test: the real renderer to a PNG, to look at                    |

Everything depends on egui only (wgpu / winit for the shell). Colours
come from the context's `Visuals`, so an app's theme reaches every widget
without the crate knowing the theme. The web build (wobble's) gets the
widgets; `shell`, `dock`, `filebrowser` and `uishot` are native.

Versions: egui 0.36, wgpu 30, winit 0.30 (no egui_dock: the dock is ours). wobble is on
egui 0.35 / wgpu 29 and bumps when it adopts the crate (the lifted code
already compiles against 0.36: the only changes were texture deltas that
must be cleared, `ModifiersChanged` events, and wgpu's `Result` from
`get_mapped_range`).

## How the dock is used

```rust
enum Tab { Graphs, Objects, .. }      impl imfram::dock::Tab for Tab { .. }
enum Role { Editor, Params, Detached } impl imfram::dock::Role for Role { .. }
struct Tools<'a> { .. }               impl imfram::dock::Panels<Tab> for Tools<'_> { fn tab(..) }

let mut docks = Docks::new("graav", headless);
let mut editor = Tree::new(vec![Tab::Graphs]);
editor.split(Side::Right, 0.76, vec![Tab::Objects]);
docks.add(el, &gpu, Role::Editor, editor);
// window_event:   if docks.event(&gpu, i, &event) { docks.frame(i, &gpu, &mut tools, None); }
// about_to_wait:  docks.apply(Some(el), &gpu); docks.poll();
```

## Lanes

In the order Alex asked for them.

1. **Docking, streamlined.** DONE (2026-10-13): egui_dock is gone;
   `dock/tree.rs` is our split tree, `dock/frame.rs` glides every leaf
   to its proposed place while a tab is dragged (a leaf's middle: a tab
   there; a side: split off; the window's edge: split the whole; another
   window previews the landing too). Still to do: layouts saved and
   restored (the tree is plain data, serde is a derive away); the
   dragged tab drawn between windows; window pick by real z-order;
   keyboard focus handed to the leaf under the pointer.
2. **Node view.** A general canvas with the niceties ported to graav:
   magnet wires, E-stretch, corner dots, splice on drop, tidy layout,
   Tab search, hover tooltips, hotkey strip. Generic over the app's graph
   (a trait for nodes, ports and wires). wobble's own node work (tiles,
   patch bay, cost noodles, families) stays in wobble.
3. **Theme engine.** wobble's `theme.rs` slot machinery (`slots!`, links,
   overrides as hex prefs, the colours window) split from its wobble slot
   table; each app invokes the macro with its own slots. The colour
   designer (`wobble/colors`) then drops its `#[path]` includes.
4. **Key map engine.** wobble's `keymap.rs` binding / override / conflict
   / rebind-window machinery split from its `Act` and `DEFS` tables.
5. **More from wobble**: the colour wheels (`wheels.rs`, after theme and
   the sensitivity helpers), the curve editor (`curveui.rs`).
6. **Adoption**: graav first (its `dock/`, `egui_layer.rs`, `ui_window.rs`,
   `widgets/{slider,rate,group,tip,keystrip}.rs`, `editor/history.rs`,
   `nav.rs` become `use imfram::..`), then wobble (egui 0.36 bump, then
   tiltmenu, colorpicker, rgbslider, eyedrop, hudkeys, numfield, nav).

## Rules

Those of the two apps that bear on a shared crate: depend on egui only;
colours from `Visuals`, sizes as named constants; tests named as
sentences; `cargo build` warning-free and `cargo test` green; split a
file past ~300 lines (the lifted ones are grandfathered); never commit
unless Alex asks.
