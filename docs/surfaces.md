# Surfaces: plates, wells, fields, materials and relief

The full surface model. `CLAUDE.md` has the summary vocabulary; this is the detail you need when
painting a control, adding a prim, or touching the plate shader or the `style.surface` config.
History is in `CHANGELOG.md`; the material design record is `rfc-material.md`.

## The vocabulary in full

- **A plate** is a lit, bounded surface with a silhouette (per-corner radius in the DE's
  superellipse family, `corner_shape`) and a stance on the surface beneath:
  - **raised** — floats above it: a `Bevel` (fill plus rolled edge) or a `Boss` (edges only, the
    surface below as its face). Menus, popovers, raised buttons, a ButtonStrip's selected
    plateau, a floating Breadcrumb.
  - **flush** — level with it inside a groove ring: `PaintCtx::inset_plate`. Buttons, dropdown
    triggers, breadcrumb runs, font selectors. Its face is the surface below unless a fill is set.
  - **flat** — `PlateStance::Flat`: a quad of the pane's material and nothing else. The silhouette
    equals the rect (the relief stances carve inside the footprint, so their visible edge sits
    half a carve in), it can be frosted (a stroke-laid face cannot carry the blur sentinel), its
    focus tint is a ring, and it carries one radius, not four. For bars and toolbars that should
    read as panes at a smaller scale (`Button::with_flat`, `Dropdown::with_flat`).
- **Plates nest in three rungs of one object**: the **root plate** (the window; `plate { root }`),
  **pane plates** (carry the corner dock, `widget/plate_dock.rs`), **control plates**.
- **A well** is an opening cut into a plate: `Recess`, or `Trough` when it holds a moving part.
  Text boxes, keybind and spinbox fields, slider and progress tracks, the trackpad, the
  ColorSelector's recess. A well's floor may carry fills (a progress fill, a swatch) — floor
  segments, not plates. A **canvas well** (Trackpad, Slider2D, the bevel and ramp previews) is
  cut from one material: the plate darkened for its floor (`color::WELL_FLOOR`) and the recess
  for its rim (`PaintCtx::well_floor` / `well_rim`). With relief off a well is its frame, the one
  hairline `color::well_frame_color`, lit in the highlight while active.
- **Segments** share one silhouette, parted by **seams** (`Groove`s dying into the rolled edge):
  Breadcrumb and ButtonStrip segments, the ColorSelector's text/swatch split. A `Separator` is the
  same cut with no segments to part.
- **A group** (`widget::Group`) owns nothing: a set of member ids; its frame is the padded hull
  of where the host put them, with a title tab on its top edge (`PaintCtx::section_well` under
  relief). Given its plate (`with_plate`) and `with_fit`, sides within `snap` of the plate's edge
  take the edge one padding in and corners follow concentrically. It never hits.
- **A dialog** (`widget::Dialog`) is a raised plate around a lasso, in the menu's material, with
  a title band. `open(ctx, members)` makes it modal (`UiContext::open_modal`): the Tab walk is
  trapped among the members, everything outside reads as covered (`is_coordinate_covered`),
  focus moves in and returns on `close`, and the members nest under it in the accessibility tree.
  Paint the dialog, not its members, after everything it covers; `set_backdrop` dims the window;
  Escape is the host's; hide the members while closed.
- **Bands** (the slider's swelling band) are outside the vocabulary.

## Fields: a well with a run in it

A dropdown, a button, a text row with its picker, a spinbox, a toggle and a check box are ONE
object: a well cut into a plate with a flush plate — the **run** — standing in it, one outline
round both (`Prim::Field`). `scene::paint::Field` is the object and `PaintCtx::field(&Field)` the
one way to paint it: the outline (rect and radii — a widget takes it through `carve_inside`), the
wall width, the run's span clamped to the outline (`run_span`, `None` for a well), and a rim
`tint` (`with_tint`).

| form | constructor | run | well |
|---|---|---|---|
| text box | `Field::well` | none | the whole field |
| flush control (dropdown trigger, button, breadcrumb run…) | `Field::run` | the whole field | none |
| text row with picker; spinbox with its −/+ run | `Field::ending_in_run(split)` | the right end | left of it |
| toggle | `Field::sliding_run(width, t)` | half the field, gliding with `t` | the other half |
| check box | `Field::well`, plus a square `Field::run` plate when checked (`Checkbox::box_plate`) | none, or a centred square | all round |

- Build a field by its form, never by numbers that encode one. Only `Field::prim_span` turns a
  run that reaches an end of the field into the prim's encoding.
- **A field with no run paints a `Prim::Recess`**, because a recess groups into the plate under
  it and a `Prim::Field` never does.
- **The shading is one outline**: the outer half is the recess's fall and shoulder all the way
  round; only the inner half differs — down to the floor in the well, mirrored back up to the
  face in the run, blended across one wall about each seam (`MODE_FIELD` in `shader2d.wgsl`). Two
  prims side by side (a recess and a trough) each turn their own corner at the seam and the
  outline reads broken; that is why this is one prim. The run's face is a rounded rect inset half
  a wall on every side, with the same corners at both ends. A run that stops short of the right
  end mirrors everything on its right.
- **The run is laid out a wall wider than its face**, so the button reaches the well and what it
  carries is centred: a textpick picker is `PICK_W` plus a wall with its arrow centred
  (`Dropdown::center_arrow`, in the trigger's arrow slot `dropdown::arrow_slot`, so every ▼ stands
  in one column); a spinbox's run begins a wall before its buttons (`SpinGeom::run_x`), halved by
  the −/+ seam. Hit zones, washes, glyphs and relief all read `SpinGeom`.
- **Every flush control wears the run's edge**: `PaintCtx::inset_plate` draws a face and a
  field that is all run — so dropdowns, buttons, breadcrumbs, menubar triggers and apps' own
  plates have the same edge as the run at the end of a text row. `Prim::Trough` remains for an
  explicit valley and cce-relief's preview.
- **Check boxes** are a square as tall as the control (at most a toggle's height) left of the
  label, an empty well unchecked and a centred square plate (`PLATE_SHARE` of the side) checked.
  An inline check (`Checkbox::paint_inline`, cce-list rows, Markdown tasks, the doc editor) is
  built through the same `Checkbox::box_field` / `box_plate`, its corner the toggle's in
  proportion. In a parameter pane a checkbox row is a Toggle.
- Who hands fields out: `TextBox::well`, `Toggle::field` (and `Toggle::face` for the relief-off
  path), `Checkbox::field`, `Spinbox::relief_parts` (a `SpinRelief`), `ParametersBg::fields` (a
  `Vec<Field>`, tinted when hovered). The legacy banded path and the flat-host bridge draw a well
  box either side of the run.

Tests: `a_toggle_is_a_field_whose_run_glides`,
`a_checkbox_is_a_well_with_a_square_plate_in_it_or_not`, `an_inline_check_is_the_widgets_box`,
`a_spinbox_is_one_field_with_its_run_at_the_right_end`, `textpick_rows_carry_a_picker`,
`a_pickers_arrow_lines_up_with_a_dropdowns`, `an_inset_plate_is_a_field_that_is_all_run` and
`a_dropdown_trigger_wears_the_runs_edge` are the tests.

## Carves on plates

**Grouping.** A full-ring, untinted `Prim::Recess` / `Boss` emitted while a plate's grouping
window is open becomes a CSG feature of that plate's one draw (`MODE_PLATE`); otherwise it is its
own overlay (`MODE_RECESS` / `MODE_BOSS`). Any ordinary geometry painted after a plate closes its
window, so grouping is rare (the demo groups 2 of 10). `CCE_PLATE_DEBUG=1` prints, per frame,
what grouped and why each fallback happened (ridge, edge-suppressed, tinted, feature budget, no
enclosing plate, window closed).

**A grouped carve shades exactly as its overlay does.** The plate lights its roll alone
(multiply plus glint and the shade line on the roll's own slope, `roll_shade_line(sv_rim)`) and
composites what the carves ADD — the summed normal's diffuse and glint less the roll's, plus
their curvature — as an overlay is blended (`carve_over`: screen toward white, multiply toward
black). A plate with no carves composites zero and is unchanged.
`a_grouped_carve_is_drawn_as_its_overlay_is` is the test: it renders a grouped and a forced-overlay
plate through `vk::plate_probe` and holds them equal to the pixel.

**`vk::plate_probe`** (`cfg(test)`) renders a `DisplayList` offscreen through the runner's own
`tessellate_display_list` and `dl_batches_2d` with the LIVE pipeline (shared
`create_ui_pipeline`, `batch_push_constants`, `window_info_data`, `relief_px_at`) and reads it
back. It draws plates, carves and flat geometry; no blur-behind, text or images. With no Vulkan
device it returns `None` and the test skips with a note. Use it for any 2D look worth pinning.

**`Prim::Frame`** (`PaintCtx::frame(rect, hole, hole_radii, material, depth)`) is a plate turned
inside out: its face is everything in `rect` outside `hole`, its rolled edge running round the
hole and falling into it, so the hole's corners are coves (inside corners) — which no box prim
can draw. The shader is `MODE_PLATE` with the hole's SDF negated (`MODE_FRAME`); it hosts carves
like a Bevel; its outer edges are not rolled (lay them past the window or under something). The
banded path fills only below the hole. The designer's playbar is the consumer.
`a_frame_is_an_inside_out_plate_that_hosts_its_carves` is the test.

**The focus ring is the rim, recoloured.** `ControlPlate::with_tint` tints the rim (a tinted
`Trough`, `Boss` or `Bevel`; `recess_tinted` for a well while editing). The tint composites the
rim's light in the accent and its shadow in a dark accent (`FOCUS_SHADOW_LUM`, both at
`FOCUS_GAIN`), so a focused plate still shows which edges face the lamp. Flat-path hosts get it
through `RenderTarget::inset_plate_tinted` and `CarveKind::Boss { tint }`.

## Materials

What a plate is made of is a `scene::Material`: tint, `Frost` (`Unfrosted`, or `Frosted {
compression, refraction, radius }`) and `Finish` (how it answers the light). Rung defaults:
`Material::root()` / `pane()` / `control()`, `popover(base)`; a well floor is `host.floor(lifted)`.
`PlateSpec`, `ControlPlate.face` (`Option<Material>`: `None` = the surface below is the face) and
the `Plate` / `Bevel` / `Sphere` / `Droplet` prims carry one; the tessellator reads fill and
finish from it. `Material::fill_tint` is the one place the blur-behind sentinel (negative alpha)
is written. `Material::from_fill` / `face` decode a colour a legacy colour-typed site still holds;
new code says `Material::opaque` / `with_frost`. `tests/plate_golden.rs` is the exit test for any
change that must not move a pixel: dump before, compare after.

**Frost is per plate.** The recipe packs into the plate's push block (`Frost::pack`), so two
plates in one window can differ; the style keys give the DEFAULT material
(`Frost::from_style`). `radius` is the kernel sigma in logical px (`Frost::DEFAULT_RADIUS` 5.5; 0
is a clear plate). A frosted flat fill (Quad, RoundedRect, Border fill) is promoted to a
zero-depth plate batch so it carries its recipe. `examples/frost_pair.rs` shows three recipes in
one window.

**The blur reads a mip chain.** The 7×7 taps stand a whole stride apart; at level 0 a hairline
behind the plate was caught by some taps and missed by others, drawing faint bands. The snapshot
carries `snapshot_levels` mips (at most `SNAPSHOT_LEVELS_MAX`, 7), rebuilt by linear blits after
each copy (`snapshot_mip_chain`), and `resolve_blur` reads level `log2(stride)`. Clean samples
(clear plates, the rim) stay at level 0. A format that cannot be linearly blitted gets one level.

**"Unfrosted" is not "solid".** `Frost::Unfrosted` never samples its backdrop; a translucent tint
stays translucent with a sharp view through it.

**Legibility is `compression`, not opacity.** Blur destroys a backdrop's detail but keeps its mean
luminance, and text contrast is a mean-luminance property: the closing
`mix(backdrop, plate, opacity)` passes the backdrop's brightness through at `1 - opacity`.
`compression` (0..1, default 0) remaps the blurred backdrop's luminance toward the plate's key
before the tint, symmetrically (bright pulled down, dark pulled up), keeping chromaticity.
**It only works with a tint dark enough to converge on.** Contrast of a `#ccccd4` label over the
designer's dialog plate, bright / dark viewport (and how much backdrop still reads through):

| tint | k=0 | k=0.4 | k=0.6 | k=0.85 |
|---|---|---|---|---|
| `#595969` (stock) | 1.16 / 6.65 | 2.25 / 4.87 | 3.10 / 4.54 | 4.10 / 4.34 |
| `#1a1a24` | 1.23 / 9.97 | 2.99 / 10.34 | **5.08 / 10.53** | 9.36 / 10.70 |
| `#050508` | 1.24 / 10.47 | 3.08 / 11.67 | **5.41 / 12.14** | 10.76 / 12.60 |
| backdrop showing (σ×100, bright / dark) | 7.3 / 0.9 | 3.7 / 0.5 | 2.3 / 0.2 | 0.4 / 0.1 |

The stock tint never clears 4.5:1 on a bright backdrop at any k and gets worse on a dark one; tint
does nothing without k; k ≈ 0.6 is the knee. **k buys independence from the backdrop; the tint
picks the key it converges on.** cce-designer ships `#05050840` at k = 0.6.

**`refraction`** (0..1, default 0) bends the backdrop through the rim along its real tilt
(`sv_rim`), scaled by the roll width. It samples the CLEAN backdrop (blurred structure would show
no bend), cross-fading to the frosted body on `f*f`, and the clear rim is exempt from compression
in proportion to its clarity. It buys no legibility; it says "this is an object". Useful range
≈ 0.3–0.6.

**Named materials** (`style.surface.material { <name> { color; frost …; finish … } }`) bind to a
rung with `plate material="…"`, `plate { root material="…" }` or `style.control.material`,
resolved by `MaterialDef::resolve` over the rung's legacy material (unset fields fall back; no
`frost` child = unfrosted; a binding wins over legacy keys; an undefined name warns). cce-relief
edits them. KDL trap in fixtures: two nodes on one line need a `;`, and `a { b }` on one line is a
parse error the loader swallows into an empty document.

## The `style.surface` config

```kdl
style {
    surface {
        plate material="glass" {                 // optional: bind the pane rung to a material
            pane color=(rgba)"#6c6c7bf2"         // pane tint; alpha is the whole strength
            frost radius=(f64)5.5 compression=(f64)0.0 refraction=(f64)0.0   // absent = unfrosted
            border_color (rgba)"#9595a9ff"        // the flat border, relief OFF only
            border_thickness (f64)1.0
            padding (i64)20                       // the pane rung's inset
            root {
                color (rgba)"#5e657acf"
                blur (f64)0.1                     // the COMPOSITOR's blur-behind, not client frost
                corner_radius (i64)24             // the pane radius falls back to this
            }
        }
        material {
            glass {
                color (rgba)"#05050840"
                frost compression=(f64)0.6 refraction=(f64)0.3 radius=(f64)5.5
                finish light=(f64)0.15 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2
            }
        }
        relief light=(f64)0.15 width=(f64)9.3 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2 shader=(bool)true {
            wall height=(mm)0.3 profile="smooth;…"   // a carve's side: height = drop
            edge height=4.0    profile="smooth;…"   // a plate's perimeter roll: height = rise
        }
        menu color=(rgba)"#101018ff" opacity=(f64)0.06 compression=(f64)0.8 corner_radius=(i64)24
    }
}
```

- `frost` is the one frost spelling: a bare `frost` frosts at defaults, `frost (bool)false` is off.
- `relief`: `light` is the strength, NOT a length (registry key `bevel_depth` →
  `Finish.strength`); `width` is the ONE run of every roll and wall; `spec` / `shininess` /
  `curvature` the DE finish; `shader=(bool)false` renders the relief through the legacy banded
  vertex shading for A/B comparison (`layout::bevel_shader`).
- The legacy `style.surface.param.color` is still read as the pane tint, multiplied by the
  top-level `plate_opacity` (`color::pane_color_is_whole` says which applies).

**Retired spellings** are reported by path (`color::retired_surface_keys`) and NOT read; cce-relief
seeds from each once and its Save writes the current spelling and removes the old:
`plate.blur` / `.radius` / `.backdrop_compression` / `.refraction`, `frost.backdrop_compression`,
`plate.bevel_width`, `relief.depth`, a material's `finish depth=`, `relief.height` / `.profile` /
`.edge_height` / `.edge_profile`, `window_manager.bevel_depth` / `.bevel_width` / `.bevel_shader`.
A config with `blur=true` and no `frost` block is therefore sharp, and the warning says why.
`relief.wall.knobs` / `edge.knobs` (and the older `profile_knobs` / `edge_knobs`) are not style
at all: cce-relief's slider state, kept in its own `~/.config/cce/cce-relief/state.kdl`.

## Relief geometry

**Two shapes.** A **wall** is a carve's side (recess, boss, ridge, trough: buttons, wells, rows),
shaded as a translucent light-and-shadow overlay. An **edge** is a plate's perimeter roll, shaded
as a multiply on the plate's fill plus a specular crest. Each has `height` (a length: the wall's
drop, the edge's rise) and `profile` (a ramp spec; absent or the identity sentinel = the analytic
curve — smoothstep for a wall, the superellipse quadrant for an edge). Unset, a carve drops
`relief_shade::RECESS_DEPTH` (0.6) of its wall and the roll is a quarter-round of radius `width`.
`layout::carve_depth_px` states the drop rule once (for the tessellator's CSG features and,
through `WindowInfo.relief_meta`, the shader's free carves); `carve_depth_ratio` /
`roll_height_ratio` feed `Finish.carve_depth` / `roll_height`. A `(relief)` value carries the drop
as `h=` beside `w=` and `d=` (light; `l=` is an alias). Registry keys: `bevel_depth`,
`bevel_width`, `bevel_shader`, `bevel_height` / `roll_height`, `bevel_profile_spec` /
`roll_profile_spec`.

**There is one roll width**, `style.surface.relief.width`: the root plate's perimeter, pane plates,
bordered widget plates under relief (`color::plate_bevel_width` returns it), every control wall.
`the_pane_roll_is_the_relief_width` is the test.

**cce-relief's Height knob** writes `wall.height` in the unit the config already spells (an
untouched slider verbatim, a moved one converted through the same metric that seeded it), choosing
`(mm)` for a new height only when the metric is real (`height_len_for`).

## Units, the metric and the height field

- **`units::Len`**: a value with a unit (`px`, `mm`, `cm`, `in`, `pt`), parsed from `"2mm"`. In
  config, a type annotation: `width=(mm)2.0`. `config::kdl_to_json` turns it into `"2mm"`, the
  writer back into `(mm)2`; `reload_config` stores it in the registry's `lens` map and
  `get_float` resolves it against the live metric at every read, so every getter is unit-aware and
  a metric that arrives or changes later is honoured without a reload. `get_len` returns the
  configured unit for editors.
- **`units::Metric`**: logical px per mm for this display, plus its source — `measured` (EDID via
  `wl_output`, or the compositor's `size_mm`), `forced` (`CCE_FORCE_PPI`), or `assumed` (96 px/in,
  a headless shadow). `Metric::is_real()` lets fabrication refuse a guess. The runner reports it
  through `window_state::set_metric`; `units::metric` asks cce-ui first
  (`units::set_metric_resolver`). The live laptop panel: 5.58 px/mm (141.8 ppi), so the 9.3 px roll
  is 1.67 mm.
- **Why logical px inside**: UI sizes are perceptual and angular; a hit target should not become
  8 mm on a projector. Documents and fabrication live in real units and convert at view time.
- **The height field** (`scene/heightfield/`): integrates the slopes the shader lights back into
  heights, samples a frame's plates per physical px (plates stack, carves etch), and writes a
  16-bit PNG with a JSON sidecar (pitch and range in mm via the metric, datum, metric source).
  `CCE_HEIGHTMAP=<file>` in any client, or `heightfield::request`. On an assumed metric the
  millimetres are a guess and the sidecar says so.
