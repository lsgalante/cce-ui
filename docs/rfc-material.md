# RFC: Material — what a plate is made of

**Status:** Draft / proposal (2026-09-20)
**Scope:** The substance of a surface in `cce-ui` — tint, frost and finish — as one value
that a plate carries, at every rung (root, pane, control) and in config.
**Appetite:** Additive first, breaking later. Every step leaves the tree building and the
pixels identical until the step that is *meant* to change them. Every crate must continue
to build standalone.
**Reads before this:** `CLAUDE.md` § "Plates, wells and seams" (the vocabulary this RFC
gives a type to), `docs/rfc-core-rebuild.md` § 7 (the plate rungs, RFC 7a–7c).

---

## 1. Why

The toolkit already talks about materials. `PlateStance::Flat` is documented as "a control
made of its pane's material". `ControlPlate::face_from_fill` forces a face opaque because a
translucent one "would read as a second material". A canvas well's floor is "cut from the
same material" as its plate. The `Droplet` carries its own gleam, shine and rim because "a
drop is wetter than the DE's plates". None of these has a type to point at. What a plate is
made of is spread over five mechanisms with four different scopes:

| Property | Where it lives today | Scope |
|---|---|---|
| Tint + opacity | `PlateSpec.color`, `color::param_plate_fill`, `plate_color`, the menubar/statusbar colours | per prim (vertex colour) |
| Frost on/off | `PlateSpec.blur`, `color::plate_blur`, `Menu::with_blur`, `layout::graph_blur` | per prim (negative-alpha sentinel) |
| Frost recipe: `backdrop_compression`, `refraction` | `WindowInfo.backdrop_meta` uniform | per **window** |
| Blur kernel | `resolve_blur`: 7×7 taps at 5.5 px stride, sigma ≈ 11 px | global, literal |
| Finish: shading strength, specular, shininess, curvature | `relief_shade::Material` → `PlatePush.material` (`p_mat`) | per prim, but only `strength` is configurable (`bevel_depth`); 0.4 / 24 / 0.2 are literals |
| Edge: roll width | `Prim::Plate.depth` | per prim |
| Edge: roll height, carve drop, profiles | `relief_meta`, `profile`, `roll_profile` uniforms | per window |
| Corner exponent | `corner_shape` uniform; `Prim::Plate.shape` override | per window / per prim |

Three consequences, each of which has already cost something:

- **The DE has exactly one substance.** Every frosted plate in a window shares one glass
  recipe. cce-designer's Alt+D dialog wanted `#05050840` at compression 0.6, and got it —
  for every frosted surface in the process, because the knob is a window uniform. A menu
  and a dialog cannot differ; a control cannot be plastic on a glass pane.
- **Three of the four finish numbers cannot be set.** `cce-relief` edits the edge geometry
  and the light strength and calls itself a material editor; it cannot touch specular or
  shininess because nothing can. The Droplet needed different values and got them by
  overwriting the push-constant slots per prim (`window_runner.rs`, the `material:
  [plate_mat[0], spec.gleam, spec.shine, spec.rim]` line) — the second material, done by
  hand, once.
- **The sentinel is decided in three places.** `PlateSpec::fill` negates alpha for a nested
  frosted plate; `color::param_plate_fill` does the same for the params plate; `Graph`
  does it again with `graph_blur`. A fourth surface that wants frost copies the trick, and
  the root-versus-nested regime (the compositor's blur, not ours, on a root plate) is only
  encoded in one of the three.

### What is already good (keep it)

- **The rung ladder.** Root, pane and control plates are one object at three scales
  (`PlateSpec` for root and pane, `ControlPlate` for control), and `PaintCtx::control_plate`
  is the one place a control face's relief is composed. A material slots into that ladder;
  it does not replace it.
- **The lighting model is shared and predictable.** `relief_shade` is the Rust twin of the
  shader's arithmetic, checked against the shader source by test. The finish stays there.
- **The per-prim push block is already the carrier for per-plate lighting.** `p_mat` is per
  batch. The frost recipe is the odd one out, not the rule.
- **Frost has a measured theory now.** Compression is the legibility axis, refraction the
  objecthood axis, and the two are orthogonal (the 24-cell sweep in `CLAUDE.md`). A
  material is the right place to *hold* that pair; nothing about the pair changes.

---

## 2. Goals / non-goals

**Goals**

1. One type, `Material`, that says what a surface is made of: tint, frost, finish.
2. Every plate rung carries one by value. The sentinel encoding happens in exactly one
   function, given the plate's role.
3. Per-plate frost: compression and refraction travel with the plate, not the window.
4. The finish is fully configurable, and the Droplet stops being a special case.
5. Named materials in config, bound per rung, with every existing key surviving as an
   alias — **no configured appearance changes until a config opts in.**
6. Derived materials are functions of a material, not colour constants: a well floor is
   its plate's material darkened; a `Flat` control is its pane's material verbatim.

**Non-goals**

- **The light is not material.** Azimuth and elevation are the scene's (`light_vector`,
  `window_manager.light_source_position`) and stay per window.
- **Roll width is not material.** It is geometry — already per prim as `depth`, already
  capped per control by the plate's own height. It stays where it is.
- **Profiles are not material, yet.** The carve and roll profiles are 8-vec4 uniform LUTs;
  making them per prim is a renderer redesign this RFC does not attempt. See § 9.
- **No new shader modes, no new blur kernel** in the first three steps. The kernel becomes
  a material knob in step 4, not before.
- **The compositor's blur is not re-implemented.** A root plate's frost is the compositor's
  and remains so; § 5 says what the compositor is *given*, not what it must draw.

---

## 3. The type

```rust
// src/scene/material.rs

/// What a surface is made of: its colour, whether and how it frosts what is
/// behind it, and how it answers the DE's light. Carried BY VALUE on the plate
/// made of it; ~14 floats, copied freely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Linear RGBA. Alpha is opacity and is ALWAYS non-negative here — the
    /// blur-behind sentinel is an encoding detail of `fill`, never state.
    pub tint: [f32; 4],
    pub frost: Frost,
    pub finish: Finish,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frost {
    /// The tint alone, composited at its alpha. Not a sample of the backdrop.
    Opaque,
    /// Frosted glass: the backdrop blurred, luminance-compressed toward the
    /// tint's key, tinted at the tint's alpha; the rim refracts.
    Frosted {
        /// `style.surface.plate.backdrop_compression` today. 0..1.
        compression: f32,
        /// `style.surface.plate.refraction` today. 0..1.
        refraction: f32,
        /// Blur radius in logical px (the kernel's sigma). Default
        /// `Frost::DEFAULT_RADIUS` = today's literal (≈ 11 px at scale 1).
        /// 0 is a CLEAR plate: one clean sample, tinted — expressible for the
        /// first time. Decided in § 11 (2).
        radius: f32,
    },
}

/// Today's `relief_shade::Material`, renamed: how the surface answers light.
/// `[strength, spec, shininess, curvature]` as `PlatePush.material`, plus the
/// two depth ratios the shader reads from `WindowInfo`.
pub struct Finish { strength, spec, shininess, curvature, carve_depth, roll_height }
```

**Why `Frost` is an enum and not two floats with a bool.** An opaque plate has no
compression and no refraction — not zero of each, none. Making the recipe unreachable when
the plate is not frosted is what keeps `Material::fill` from having to ask two questions.

**Why the finish is inside the material and not beside it.** Because the Droplet already
proved a surface's gleam belongs to the surface: water and plastic under the same light are
different *materials*, not different lights.

### 3.1 The one encoding function

```rust
impl Material {
    /// The vertex colour the renderer consumes, for a plate in `role`:
    /// - `Root`  → alpha positive whatever `frost` says. A root plate's frost
    ///             is the compositor's blur-behind, never the in-app pass.
    /// - nested + `Frosted` → the in-app frost pass's negative-alpha sentinel.
    /// - nested + `Opaque`  → alpha as is.
    pub fn fill(&self, role: PlateRole) -> [f32; 4];
}
```

This absorbs `PlateSpec::fill` and `color::param_plate_fill`'s negation, and it is the only
place a negative alpha is ever written. `Graph`'s `graph_blur` becomes a `Frosted` material
on the graph's plate. `PlateRole` is `PlateSpec::is_root()` given a name: `Root` when all
four corners are window corners, `Nested` otherwise; `ControlPlate` is always nested.

### 3.2 Derived materials

```rust
impl Material {
    /// The well floor cut into this plate: the same material, darkened by
    /// `WELL_FLOOR`'s 18% (10% lifted). Frost and finish carried through, so
    /// a well in a glass pane is a deeper piece of the same glass — decided,
    /// § 11 (3). Under `Frost::Opaque` this is today's overlay exactly.
    pub fn floor(&self, lifted: bool) -> Material;
    /// A `Flat`-stance control on this pane: the material verbatim.
    /// Exists so the call site says what it means.
    pub fn flat_control(&self) -> Material { *self }
    /// A control face from a configured fill under today's opacity rule
    /// (`ControlPlate::face_from_fill`): opaque, or `None` for edges-only.
    pub fn control_face(raw: [f32; 4], finish: Finish) -> Option<Material>;
}
```

`colors::WELL_FLOOR` and `WELL_FLOOR_LIFTED` become the two constants `floor` uses and stop
being drawn directly. `PaintCtx::well_floor` / `canvas_well` take the host plate's material.

### 3.3 The defaults

```rust
impl Material {
    /// The rung defaults, resolved from the style registry — the SAME
    /// getters the rungs read today, so step 1 changes no pixel:
    pub fn root()    -> Self   // root_plate colour + opacity, root blur flag, Finish::from_style
    pub fn pane()    -> Self   // param_bg_color × plate_opacity, plate_blur, compression, refraction
    pub fn control() -> Self   // control fill getters, never frosted (§ 6.2)
}
```

`Finish::from_style` is `relief_shade::Material::from_style` unchanged — `strength` from
`bevel_depth`, the other three from new keys (§ 5) with today's literals as defaults.

---

## 4. Where it is carried

| Today | After |
|---|---|
| `PlateSpec { rect, color, blur, window_corners, depth }` | `PlateSpec { rect, material, window_corners, depth }` |
| `ControlPlate { rect, radii, stance, face: [f32;4], depth, tint }` | `ControlPlate { …, face: Option<Material>, … }` — `None` is today's transparent face (edges only) |
| `Prim::Plate { rect, radii, color, depth, shape }` | `Prim::Plate { rect, radii, material, depth, shape }` |
| `Prim::Bevel { rect, radii, color, depth, tint }` | `Prim::Bevel { rect, radii, material, depth, tint }` |
| `PaintCtx::plate(rect, radii, color, depth)` | `PaintCtx::plate(rect, radii, &Material, depth)` |
| `PaintCtx::inset_plate(rect, radii, color, depth)` | takes `&Material` |
| `PaintCtx::well_floor(rect, radius, lifted)` | `well_floor(rect, radius, &host_material, lifted)` |
| `PaintCtx::plate_spec(&spec)` | unchanged signature; calls `spec.material.fill(spec.role())` |

Carves (`Recess`, `Boss`, `Ridge`, `Trough`, `Groove`, `Lattice`, `CarveUnion`,
`ConcaveFillet`) emit shading only and have no material of their own; they shade *whatever
is beneath*. They take the host's `Finish` for their lighting response, which is what
`plate_mat` already gives them — unchanged in kind, now sourced from the host plate.

`Sphere` and `Droplet` carry a material like a plate. The Droplet's `gleam` / `shine` /
`rim` fields move into its `Finish` (spec / shininess / curvature); `DropletSpec` keeps its
shape fields and loses its finish fields, and the hand-written override in
`window_runner.rs` becomes the general path.

**Client call sites** (from a grep on 2026-09-20): `PlateSpec` is built at one site each in
cce-authenticator, cce-cloud, cce-designer (`render.rs`), cce-files, cce-data-editor,
cce-lock, cce-list, cce-system-interface, cce-terminal, and the demo (`src/main.rs`) — ten
sites, all of the form `color: …, blur: …`. Each becomes `material: Material::root()` or
`Material::pane()` unless it had its own colour, in which case `Material::pane().with_tint(c)`.

---

## 5. Config

### 5.1 Named materials, bound per rung

```kdl
style {
    surface {
        material {
            glass {
                color (rgba)"#05050840"
                frost backdrop_compression=(f64)0.6 refraction=(f64)0.3 radius=(f64)5.5
                finish light=(f64)0.15 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2
            }
            plastic {
                color (rgba)"#26263380"
                finish light=(f64)0.15 spec=(f64)0.25 shininess=(f64)12.0
            }
        }
        plate material="glass" {        // the pane rung
            root material="glass"
        }
    }
    control material="plastic"
}
```

(As built: the materials are CHILDREN of one `material` node, named by node name, not
`material "glass"` with a string argument — the config converter keys objects by node
name and a node's argument would be lost; and `control` is `style.control`, the node the
control rung's other keys already live under.)

A `material` node with no `frost` child is `Opaque`. Missing `finish` keys take the rung
default; a missing `color` keeps the rung's tint. A rung with no `material=` binding reads
its material from the legacy keys below — which is how every existing config keeps its
look. `Material::named(name)` hands an app any defined material for its own surfaces.

### 5.2 Aliases: every existing key survives

| Existing key | Resolves into |
|---|---|
| `style.surface.plate.color` / `.blur` / `.backdrop_compression` / `.refraction` / `.radius` | the pane rung's default material (`radius`, new: the default frost's blur sigma) |
| `style.surface.param.color`, `style.surface.plate.opacity` | the pane rung's tint |
| `style.surface.plate.root.color` / `.blur` | the root rung's default material |
| `style.surface.relief.depth` / `.light` | every rung's `finish.strength` (and the free carves') |
| `style.surface.relief.spec` / `.shininess` / `.curvature` | the DE finish's other three terms (new; were literals) — every rung's, and the carves' |
| `style.surface.relief.width` / `.height` / `.edge_height` | unchanged: geometry, not material |
| `style.surface.control.fill` (and the per-widget fills) | the control rung's tint |

The registry key names (`bevel_depth` etc.) are untouched; the alias table in
`layout.rs` (the `"style.surface.relief.depth" => "bevel_depth"` block) is where the new
paths join. **Precedence:** a `material=` binding wins over the legacy keys; a legacy key
never overrides a bound material. Unbound is the default, so a config written today parses
into exactly the material it draws today.

### 5.3 The compositor is given the root material, not the frost pass

A root plate's frost is scenefx blur-behind, driven from the compositor's own reading of
`plate.root` (`cce-compositor/src/server/config.rs`: `window_blur`,
`window_backdrop_blur_ignore_transparent`, …). The compositor does not link the toolkit's
paint path, so nothing here changes what it draws. What changes is that the root rung's
material is now *nameable*: the compositor can read `plate.root.material` and the named
node under it, and its tint/opacity/blur derive from the same recipe the client's nested
plates use.

This matters for RFC 7c. A detached pane flips regimes (nested sentinel → compositor blur)
and today that flip changes the look, because the two blurs share nothing. With a material
the compositor has a recipe to approximate — compression in particular is a per-pixel
luminance remap scenefx could carry. **Out of scope here**; recorded so the config shape is
not chosen in a way that forecloses it.

---

## 6. Renderer

### 6.1 The push-constant budget is the constraint

`PlatePush` is exactly **128 bytes**, the Vulkan-guaranteed minimum, and `renderer.rs`
asserts it at compile time (`PUSH_CONSTANT_BYTES <= 128`). It cannot grow without a
device-limit query and a fallback path. So per-plate frost has to fit in slots that are
free *in the plate mode*:

- `p_host` in `MODE_PLATE` (mode 1) uses only `x`/`y` (feature offset and count). **`z` and
  `w` are free.** Mode 2's use of `p_host` as the host-box fade, mode 13's as the lattice
  period and mode 14's as the union run are other modes and unaffected.
- `rect1` is spoken for: `.x`/`.y` the clip radius and flag, `.z` the mode, `.w` the shape.
- `p_mat` is full and stays `[strength, spec, shininess, curvature]` — the `Finish`.
- `p_spec_tint.xyz` is the accent RGB, `.w` the tinted-plate flag: untouched.

Two free floats, three frost scalars. **The layout (decided, § 11 (2)):**

| Slot | Carries | Encoding |
|---|---|---|
| `p_host.z` | `compression` and `refraction` | 12-bit fixed point each: `round(c·4095)·4096 + round(r·4095)`, an integer < 2²⁴ — the largest range f32 holds exactly. Shader: `hi = floor(v / 4096)`, `lo = v − hi·4096`, both `/ 4095`. |
| `p_host.w` | `radius` | the kernel sigma in physical px (logical × scale); `0` = one clean sample, no kernel. |

A `Frost::pack(scale) -> [f32; 2]` / `unpack` pair lives beside the type with a round-trip
test over the grid, and the `relief_shade`-style shader-text test checks the literals
(`FROST_PACK_BASE`, `FROST_PACK_MAX`, `LEGACY_STRIDE`) against `shader2d.wgsl`. 12 bits is
0.00024 resolution: the designer's 0.6 / 0.3 survive to better than a 1/255 step (10 bits
was the first draft; 12 costs nothing since the sum still fits an exact f32 integer).

`resolve_blur` takes `k` and the kernel stride as arguments (it already took `refract` and
`clarity` from the plate branch); the plate branch unpacks `p_host.zw`. The window
uniform's `backdrop_meta` is retired in the same step. The negative-alpha branch for a
batch with NO plate block survives as the no-recipe fallback — a raw vertex pushed from
outside the display list (a legacy host's own quads, or the `bevel_shader 0` path): the
kernel default (`LEGACY_STRIDE`) and no compression. Everything the display list frosts is
a plate batch (§ 6.2) and never reaches it.

**The Droplet is the exception.** Mode 10 uses every slot: `p_host` is the sheet radius,
clarity, dome and attach radius, `p_spec_tint` the core, shadow reach, shadow strength and
bow, `p_mat` the finish. There is no room for a frost recipe, so a Droplet's
`Frost::Frosted` is honoured as on/off at the kernel default — exactly what it draws today
(`resolve_blur(frag, vcol, vec2f(0.0), 0.0)`, no compression, its refraction being the
compositor's `refr`). `Material::fill` does not care; the runner documents the clamp.
Freeing a slot by packing `core` (0..2) with `shadow` (0..1) is possible and deferred (§ 9).

### 6.2 The plain frosted quad has no push block

The non-plate blur-behind branch at the bottom of `fs_main` (a negative-alpha vertex colour
with `mode == MODE_NONE`) carries no push constants. It is what a `Flat`-stance control,
the `Menu`, the `Graph` and the status bar's frosted fill use today. Three ways to give it a
per-plate recipe; **decided: the first** (§ 11 (1)):

1. **Promote it to a plate.** A `Flat` face is `Prim::Plate` with `depth = 0` — the shader's
   plate path with `t = 0.001` shades nothing at the rim and the fill is a rounded rect,
   which is exactly what `Flat` draws now. Cost: one batch per flat frosted quad instead of
   sharing the vertex stream, which the Menu and status bar already pay for their rolls.
   Benefit: one frost path in the shader, and § 6.3 comes for free.
2. Keep the quad path on the window uniform. Cheap, but then a `Flat` control cannot be a
   different glass from its window, which contradicts § 2 goal 3 for the one stance whose
   whole point is "the pane's material at control scale".
3. A second vertex attribute. Touches the vertex layout every client's `custom_vertices`
   emits; not worth it for two floats.

Options 2 and 3 are recorded as considered, not as fallbacks: if step 3's batch-count
measurement shows a real cost on some client, that client's widget drops frost on its flat
faces rather than reintroducing the uniform path.

Control faces under `Raised` / `Flush` are laid through a stroke the sentinel cannot reach
(`CLAUDE.md`, the `Flat` entry); they stay `Opaque` and `Material::control()` asserts it.
Frost at the control rung is `Flat` only. This is today's rule stated as a type.

### 6.3 Blur radius as a material knob (step 3)

`resolve_blur`'s 49 taps are the most expensive path in the shader, and every frosted plate
pays them. `Frost::Frosted.radius` (§ 3) is the kernel's sigma in logical px, carried in
`p_host.w` in physical px; the 7×7 kernel's tap stride is half of it. It joins in step 3
with the other two scalars so `Frost` changes shape once (§ 11 (2)). A small control plate
blurring at a third of the body's radius reads the same and samples a tighter footprint;
`radius = 0` short-circuits to one clean sample — a *clear* plate, the tint over an
unblurred backdrop, which nothing could express before. Config exposure (`frost radius=`)
waits for step 4.

**`Frost::DEFAULT_RADIUS` is 5.5 logical px, and why it is not 11.** The old kernel was a
fixed 5.5 PHYSICAL px stride (sigma 11 physical) — half the blur on a scale-2 panel that
it was on a scale-1 one, and the panel every frosted surface was tuned on is scale 2. A
material cannot know the scale, so the default is stated in logical px at the value that
reproduces that panel exactly: 5.5 logical = 11 physical at scale 2. A scale-1 display now
gets the same logical blur instead of twice it; only headless scale-1 shadows notice.

---

## 7. Migration plan (staged; every stage leaves the tree building)

Each step's exit test is named. Steps 1–2 must be **prim-identical** against a display-list
dump of every client's default view — the technique the `ControlPlate` migration used.

**Step 1 — the type, the rename, the encoding.** *DONE 2026-09-20.* As specified, plus
one thing found on the way: five plate getters (`plate_blur` among them) read their
`RwLock` directly and so could not see a test's per-thread write; they go through
`style_read` now. `Finish` lives in `material.rs`; `relief_shade` re-exports it and
keeps the `Material` alias. `Frost::Frosted` already carries `radius` at
`DEFAULT_RADIUS`, unread until step 3.
- Add `scene/material.rs`: `Material`, `Frost`, `Finish` (= `relief_shade::Material`
  moved and renamed; `relief_shade` keeps a `pub use` so `cce-relief`, `vk/rt.rs`,
  `cce-designer/geometry.rs` and `vk_smoke.rs` need no edit until they choose to).
- `Material::fill(role)`, `Material::{root,pane,control}()`, `floor`, `control_face`.
- `PlateSpec::fill` and `color::param_plate_fill` become one-line calls into it.
- New `finish` getters in `color.rs` for spec / shininess / curvature, defaults 0.4 / 24 /
  0.2; `Finish::from_style` reads them.
- *Exit:* `cargo test -p cce-ui scene::`; the `relief_shade` shader-constant test still
  passes; every client builds; prim dump identical.

**Step 2 — thread it through the rungs.** *DONE 2026-09-20*, as four commits (2a–2d),
each prim-identical against a tessellation dump taken before 2a (`tests/plate_golden.rs`,
148518 lines, two scales × both edge paths). Deviations from the text below, each
deliberate: `DropletSpec` KEEPS `gleam` / `shine` / `rim` as the config spelling and
`DropletSpec::finish()` derives the `Finish` the material carries — the runner's hand-packed
slots are gone, which was the point. `PaintCtx::inset_plate` takes `Option<&Material>`
(`None` = the surface below is the face), as does `ControlPlate.face`. `well_floor` keeps
today's overlay emission: switching it to `host.floor()` is not prim-identical for a
translucent or frosted pane (a darkened fill at the pane's alpha composites differently
from a darkening overlay on the resolved plate), so it moves to step 3, where pixels may
change. `Material::from_fill` / `face` bridge the colour-typed flat path
(`layout::RenderTarget`), which is unchanged. `layout::tree_blur` has no reader and was
left alone. `Material::popover` is the menu recipe the three menu sites had each spelled.
- `PlateSpec`, `ControlPlate`, `Prim::Plate`, `Prim::Bevel`, `Prim::Sphere`,
  `Prim::Droplet` carry a `Material`; `PaintCtx` signatures per § 4.
- `DropletSpec` loses `gleam` / `shine` / `rim` to its material's `Finish`; the runner's
  hand-packed `material:` line goes.
- The ten client sites and the toolkit's own pane widgets (`ParametersBg`, `Spreadsheet`,
  `InfoBox`, `Menu`, `StatusBar`, `Graph`).
- `Graph`'s `graph_blur` and `TreeList`'s `tree_blur` become materials on their plates.
- *Exit:* prim dump identical, including the status bar's droplet.

**Step 3 — per-plate frost.** *DONE 2026-09-20* as 3a–3c plus the measurement below.
As specified except: the no-recipe fallback branch survives for raw vertices (§ 6.1);
the pack is 12-bit; `DEFAULT_RADIUS` is 5.5 logical (§ 6.3); promoted fills are
zero-depth **Bevels** in the tessellator, not `Prim::Plate`s in the display list — a
widget-scale fill wants nominal radii and circular corners (`shape` 2), and leaving the
list untouched keeps the legacy bridges that extract `RoundedRect`s working; and the
well floor (deferred from step 2) is its host's material only for a FROSTED host, an
opaque host keeping the exact darkening overlay (`PaintCtx::well_floor`).
Batch count: a frosted quad was already its own never-merged batch (the blur snapshot),
so promotion adds none; a frosted `Border` splits fill and stroke, +1.
**Measured** (`examples/frost_pair.rs` in a scale-2 shadow: three plates of the
designer's tint over a white/black checker, mean sRGB luminance under the bright and
dark columns, and the largest per-pixel luminance step across a column edge):

| plate | bright | dark | swing | edge step |
|---|---|---|---|---|
| bare checker | 0.989 | 0.005 | 0.984 | — |
| compression 0, default radius | 0.873 | 0.078 | 0.795 | 0.173 |
| compression 0.85, default radius | 0.367 | 0.039 | 0.329 | 0.075 |
| clear (radius 0) | 0.882 | 0.004 | 0.878 | 0.878 |

Three recipes in one window, each doing what its own numbers say. **Control:** the same
example built at the pre-step-3 commit (c86d6f6) in its own shadow draws all three plates
as the window-wide default (swing 0.795 each); HEAD's default-recipe plate is
pixel-identical to it (max |diff| 0/255 over the plate), so the default moved nothing on
the scale-2 panel and the other two plates differ only by their own recipes.
- `Frost::pack` into `p_host.zw` in mode 1 (§ 6.1's layout); `resolve_blur` reads its
  three arguments; the Droplet clamps to the kernel default.
- `Flat` faces, `Menu`, `Graph` and the status bar fill promoted to zero-depth plates
  (§ 6.2); the `MODE_NONE` sentinel branch and `backdrop_meta` go in the same commit. The
  `window_info_layout_matches_the_uniform_size` test shrinks with it.
- `radius` carried at its default; the pack round-trip and shader-literal tests land here.
- *Exit:* a shadow sweep that gives one plate compression 0.85 and another 0 in the same
  window and diffs the two against the 2026-09-20 sweep's numbers; `CCE_FRAME_DEBUG`
  batch count before/after on cce-designer's default view (expect +N for the flat frosted
  quads, N small).

**Step 4 — config and editor.** *DONE 2026-09-20.* The named-material nodes and the three
rung bindings (§ 5.1, in the child-node shape), `MaterialDef::resolve` over the rung's
legacy material, `Material::named`, the DE finish keys `relief.spec / shininess /
curvature` and the default frost's `plate.radius`; live reload replaces nodes and bindings
wholesale. cce-relief grew two columns — Finish (Specular / Shininess / Curvature) and
Frost (Compression / Refraction / Blur radius) — seeded from the pane rung's effective
material, applied live, and saved into the bound material's node when the pane is bound,
else into the DE keys: **Save never restructures a config that has no materials**; the
named form is opted into by writing the binding. In `--key` mode the material sliders
are not part of a `(relief)` value and are not written. *Exit:* no live config or backup
carries a `material` node or binding (grep), so all resolve as before by construction;
`named_material_round_trips_the_legacy_spelling` pins the designer's `#05050840` / 0.6 /
0.3 as a bound `glass` to the SAME `Material` as the legacy keys (same Material, same
bytes — steps 2–3), and `material_keys_write_as_frost_and_finish_props` pins the
writer's shape. Not exercised: a Save click in the shadow (the utility window is taller
than the headless output).
- `material "<name>"` nodes; per-rung `material=` bindings; the alias table (§ 5.2).
- `cce-relief` grows a Finish section (spec / shininess / curvature) and a Frost section,
  and Save writes a named material.
- Optionally `radius` (§ 6.3).
- *Exit:* every config under `~/.config/cce` and its backups round-trips to the same
  materials with no `material=` binding present; the designer's `#05050840` / 0.6 / 0.3
  config re-expressed as `material "glass"` draws pixel-identical.

---

## 8. Risks & mitigations

- **`relief_shade` drift.** The Rust twin of the shader is checked against shader source
  text by test. Renaming `Material` → `Finish` keeps the struct and the constants; the test
  is untouched. A new `Frost` field never enters `relief_shade`, since it predicts shading,
  not compositing.
- **The hex trap, again.** `Material::pane()` reads the same gamma-decoded getters as
  today; a `material` node's `color` goes through the same `parse_hex`. Nothing new is
  decoded differently — but the § 7 step-4 exit test exists because "same look" was
  claimed and wrong once already (`CLAUDE.md`, the compression sweep).
- **Batch count.** § 6.2 option 1 adds a batch per flat frosted quad. Measured, not
  assumed, at step 3; if a client shows a real cost, its flat faces fall back to option 2
  per widget, not DE-wide.
- **Stale clients.** A toolkit change is three stages (commit, rebuild dependents,
  relaunch — `WORKSPACE.md`). Step 2 changes public signatures, so every client fails to
  build until migrated, which is the safe failure. Step 1 does not, so a client can run
  the old encoding beside a toolkit that has the new one; both produce the same alpha.
- **Concurrent sessions in cce-ui.** Steps 1 and 2 touch `paint.rs`, `color.rs` and
  `window_runner.rs`, the files every plate change touches. Small commits, explicit
  paths, and the log checked before each.

---

## 9. Deferred (designed-for, not built now)

- **Profiles per material.** The carve and roll profile LUTs are 8-vec4 uniforms. A
  material-owned profile means an SSBO of profiles indexed from the push block, with the
  index in `p_light.w`'s spare precision or a real slot. A `Material` grows a
  `profile: Option<ProfileId>` then; nothing in § 3 forecloses it.
- **The compositor matching the client's frost** (§ 5.3): compression in scenefx.
- **A frost recipe for the Droplet.** Packing `core` (0..2) with `shadow` (0..1) in
  `p_spec_tint.x` frees `.z` for `Frost::pack`'s first float; the radius would still need
  a second. Only worth it if a droplet ever wants compression.
- **Texture.** A material with a normal map or a grain is a `Finish` with a sampler; the
  push block cannot carry one, so it waits on the profile SSBO.
- **Materials as widget theme.** Once controls read `Material::control()`, a per-widget
  material (`Button::with_material`) is the obvious next override and should replace the
  per-widget `corner_radius`-style fill keys rather than join them. Not in this RFC.

---

## 10. Open questions

All four resolved 2026-09-20; see § 11. Kept here so the alternatives stay on record.

1. § 6.2: promote flat frosted quads to zero-depth plates, or keep them on the window
   uniform? → promote.
2. Should `Frost` carry the blur `radius` from step 3, or wait for step 4? → step 3, slot
   decided in § 6.1.
3. Does a **well floor** frost? The alternative — floors always `Opaque` — is what
   `WELL_FLOOR` does today, a darkening overlay on an already-resolved plate. → carry the
   host's frost through.
4. Naming for the lighting response. Alternatives considered: `Surface` (taken by the
   config node), `Lighting` (the light's, not the material's), `Shading` (what the shader
   does, not what the material is), `Sheen` (reads as specular alone). → `Finish`.

## 11. Decisions (resolved 2026-09-20)

1. **Flat frosted quads become zero-depth plates.** One frost path in the shader; `Flat`
   is literally the pane's material at control scale, and the `backdrop_meta` uniform is
   retired. (Built: the `MODE_NONE` negative-alpha branch stays as the no-recipe fallback
   for raw vertices from outside the display list — § 6.1.) The batch-count measurement at step 3 is a
   check on cost, not a vote on the design: a client that measures badly drops frost on
   its flat faces per widget, and the uniform path does not come back.
2. **`radius` joins `Frost::Frosted` at step 3, in `p_host.w`; compression and refraction
   share `p_host.z` as fixed point** (12-bit as built; 10 was the draft). `Frost` changes shape once. Config exposure of
   `radius` waits for step 4. The Droplet, whose push block is full, honours `Frosted` as
   on/off at the kernel default — what it draws today.
3. **A well floor carries its host's frost.** `floor()` copies the material and darkens the
   tint; under `Frost::Opaque` this is today's `WELL_FLOOR` overlay exactly, and under
   `Frosted` a well in glass is deeper glass rather than the one opaque patch in a frosted
   pane. Cost: a frosted well is a second 49-tap plate inside its pane; wells are small.
4. **The lighting response is `Finish`.** `relief_shade::Material` is renamed; `Material`
   is the composite (tint, frost, finish). `relief_shade` keeps `pub use Finish as
   Material` through step 2 so cce-relief, `vk/rt.rs` and cce-designer's geometry build
   untouched until they choose to update, then the alias goes.
