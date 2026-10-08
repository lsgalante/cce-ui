# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> This is the `cce-ui` crate. It lives inside the larger **`cce` Cargo workspace** — read the
> workspace guide `../cce-compositor/WORKSPACE.md` first for the multi-repo layout, the
> standalone-build rule (no `[workspace.dependencies]`), the KDL config system, and the
> Unix-socket IPC convention. This file covers only what is specific to `cce-ui`.

## What this crate is

`cce-ui` is the **shared, custom retained-mode GUI toolkit** every `cce-*` client depends on
(a git pin on GitHub that the workspace root's `[patch]` redirects to this tree). Its GUI-free
half is the sibling crate `cce-core`, which the compositor depends on instead. It is
not a wrapper around an existing framework — it owns its transport, rendering, layout, and widget
set outright.

- **Transport**: raw `wayland-client` 0.31 + `smithay-client-toolkit` 0.19, driven by a `calloop`
  event loop. Clients are real Wayland surfaces (xdg toplevels, xdg popups, and `wlr-layer-shell`
  layer surfaces), not toolkit-owned windows.
- **Rendering**: raw Vulkan via **ash** (`src/vk/`, `VkRenderer`) for all geometry, and
  **cosmic-text** + swash for text (shaped into a self-managed glyph atlas by `src/vk/text.rs`).
  `src/vk/compute.rs` is the one non-drawing seam: `ComputeDevice::run` uploads a list of
  `Binding`s, dispatches a WGSL `Kernel` on a headless device, waits, and reads the read-write
  ones back — buffers are host-visible and mapped, so upload and readback are memcpys, and every
  failure (a user's bad WGSL included) is an `Err`, never a panic. Built for cce-designer's
  solver operators (its `shapeshifter.md`, Phase 7 step 4); its tests run on whatever Vulkan the
  machine has and skip with a note where there is none.
  The wgpu path is retired; cosmic-text used to be reached through **glyphon**, which is gone
  too — every `glyphon::` item used here was a cosmic-text re-export, and dropping it takes
  wgpu out of the build. There is no HTML/DOM — the UI is GPU primitives (quads, rounded rects with
  per-corner radii, vectors with caps, arcs, circles, and the **relief primitives** — the
  lit-surface family: bevels, plates, recesses, bosses, ridges, fillets, grooves, lattices, box unions; see the
  `Prim` enum doc in `src/scene/paint.rs`). Tessellators live in
  `backend/tessellate.rs` and are re-exported through `backend/window_runner.rs` and
  `src/engine.rs`.
- It is **both a library and a binary.** `src/lib.rs` is the toolkit; `src/main.rs` is
  `DemoApp`, the reference `Application` — a small widget gallery on the Phase 6 target
  architecture (display-list frame, scene-solver layout, routed events, in-frame
  popovers). Copy it when starting a new client.

## Build, test, run

Use cargo directly (the `Makefile` wraps `cargo build --release` and
`ccebuild install --no-build cce-ui`, which installs its one bin, the demo; the relief
editors `cce-relief` and `cce-ramp` are the `cce-relief` crate since 2026-10-08). Prefer `-p cce-ui` from anywhere in the workspace so you don't rebuild the compositor.

```sh
cargo build -p cce-ui                          # build the toolkit (+ demo binary)
cargo test  -p cce-ui                           # run the test suite (headless unit tests)
cargo test  -p cce-ui scene::arena              # tests in one module
cargo test  -p cce-ui --lib color::             # tests in one lib module path
cargo run   -p cce-ui                            # run the demo/reference app (needs a Wayland session)
```

Tests are headless unit tests colocated in `#[cfg(test)]` modules — concentrated in `src/scene/*`
(the arena/layout/paint/anim engine) and `src/color/`, `src/layout/`, plus a
scattering of widgets (`text_box`, `slider`, `dropdown`, `treelist`, …). When touching the scene
engine, that module's tests are the fast feedback loop; run `cargo test -p cce-ui scene::` before
anything else.

**The tests never read the machine's config** (since 2026-10-07). Under `cfg(test)`,
`config::config_home()` is a per-process directory nobody creates (`config`, `input` and
`motion` live in `cce-core` now, where that gate is its `test-isolation` feature, which
cce-ui's dev-dependency turns on — see "The GUI-free half is cce-core" below), so `config.kdl`, the per-app
overrides and `input.kdl` are all absent and every getter answers its default. A test that needs a
setting loads it from a string (`color::reload_colors`) or pins it on its thread. Before this, six
heightfield and material tests failed on the desktop, whose `relief edge height` and frost keys
overrode what they set, and passed elsewhere. Fonts are another matter: `fonts_dir()` still reads
`~/Dropbox/Fonts` (or `$CCE_FONTS_DIR`), and the text-shaping tests need those faces.

**The library also builds for the browser** (`wasm32-unknown-unknown`, since 2026-10-04):
`scripts/check-wasm` type-checks it, with and without the optional features. The native
shell and renderer — `vk`, the Wayland shell (`backend::{window_runner, menu_popup, dnd}`),
`wayland`, `protocol`, `ipc`, `mcp`, `file_dialog` — and their crates (ash, smithay, calloop,
wayland-*, libc, rfd) are `cfg(not(target_arch = "wasm32"))`; so are the Wayland-typed parts
of the client contract: `Application::new(qh, …)`, `layer()` and `LayerSettings`,
`register_sources`, and `renderer_init` / `stage_renderer`, which take a `VkRenderer`.
`WindowAction::Resize` takes `app::WindowEdge` — xdg's `ResizeEdge` on Linux, as before.
Since macOS joined (2026-10-05) "native" is two things: the Vulkan renderer and the native
services (`vk`, `ipc`, `file_dialog`; ash, libc, rfd) are `cfg(not(target_arch = "wasm32"))`,
and the WAYLAND shell — `backend::{window_runner, menu_popup, dnd}`, `wayland`, `protocol`,
`mcp`, the crates smithay / calloop / wayland-* / xkeysym, and the contract's `new(qh, …)`,
`layer()`, `LayerSettings` and `register_sources` — is
`cfg(not(any(target_arch = "wasm32", target_os = "macos")))`. `renderer_init` /
`stage_renderer` are on macOS too, since it has the Vulkan renderer.
Portable code keeps time with `web_time::Instant` (std's own type natively; std's panics in
the browser), and reaches what a renderer draws through `crate::draw`, not `crate::vk`. A
change that makes portable code call into a native module fails `check-wasm` first — put
the native half behind the cfg, as `color_selector::place_picker_at_pointer` does.

**It draws in the browser too** (`src/web`, `WebRenderer`, since 2026-10-04): the Vulkan
renderer's 2D path on WebGPU through web-sys, from the same `Frame2D`, shaders, glyph atlas
and image queue. web-sys still ships its WebGPU bindings behind `--cfg=web_sys_unstable_apis`;
`.cargo/config.toml` sets it for the wasm target, and **cargo reads that file from the
directory it is run in** — so build the browser half from inside `cce-ui` (as
`check-wasm` does), and a client crate that builds cce-ui for the browser needs the same line
in its own config.

**And an `Application` runs in a page** (`web::run::<App>(canvas, fonts, sizing).await`, since
2026-10-05): the browser shell (`src/web/shell.rs`) is the Wayland shell's counterpart over a
`<canvas>`, on the same `Driver` and `Pacer`. What it does in the page's terms:

- **Events**: pointer (captured on press, so a drag outside the canvas still ends), wheel,
  key and focus events on the canvas, mapped by `backend::dom` — `map_key` gives a key the
  TEXT xkb's `utf8` would (Tab "\t", Enter "\r", Ctrl+letter its control code: the Wayland
  shell hands widgets exactly that, and a focused text box inserts Tab's), `wheel_frame`
  reads a whole notch-sized pixel delta (Chromium's 100 px) or a line / page delta as a wheel
  notch and anything else as a finger, and a finger gesture's lift is synthesized after
  120 ms without a frame (`FINGER_LIFT`), since a page reports none. The browser's own key
  repeats are dropped: the driver repeats, as on Wayland. On a Mac, Command is the shortcut
  key (⌘Z is undo). A page has no grabs, so a press on a CSD border is the app's.
- **Pacing**: a turn per animation frame at the pacer's ACTIVE cadence, a timer at its idle
  one; any event, and any `AppSender::send` (through `backend::app::set_wake`), wakes the
  loop for the next frame. Measured idle: 5 turns in 5 s, the native count.
- **Size**: `Sizing::App` sizes the canvas from `WindowSettings` and `desired_size` (CSS px),
  as a window; `Sizing::Page` leaves it to the page's CSS and ignores size requests (a tiling
  compositor's answer); a ResizeObserver wakes a turn on relayout, and `devicePixelRatio` is
  the scale. The context menu is drawn in the canvas and kept inside it, as on a layer surface.
- **Fonts** (`web::Fonts`): the files, and the generic serif / sans / mono families — a page
  has no font directory and no fontconfig, so it says both. `lib::page_fonts` holds them, and
  on wasm EVERY font database the toolkit builds loads them: the shell's, the widget-geometry
  one (`geometry_font_system`) and the text-measurement one (`widget::input::get_font_db`,
  resvg's — the toggle's label is centred by it). cosmic-text has no family fallback list on
  wasm (`fallback/other.rs` is empty), where on Linux it walks Noto Sans → DejaVu Sans → …,
  so `page_fonts::stand_in_for_missing` gives the families the toolkit names (the configured
  fonts, "Berkeley Mono") the faces of the first family of that Linux list the set has; a
  family an app names itself must be in the set. Order matters: the measuring fallback is the
  first face with the glyph.
- **`web::capture().await`**: the next frame, read back from the GPU. Headless Chromium
  composites in software and leaves a WebGPU canvas out of its screenshots and `toDataURL`.
- **The clipboard** (since 2026-10-05): `widget::clipboard` is one synchronous text pair
  (`copy_to_clipboard` / `read_from_clipboard`, every widget's copy, cut and paste) with a
  backend per platform — `wl-copy` / `wl-paste` (`xclip`) on Wayland, `NSPasteboard` on
  macOS, and in a page the page's own clipboard events, because a page may read the
  clipboard only inside a `paste` event. So the canvas lets ⌘/Ctrl+C, X and V keep their
  defaults (`dom::clipboard_key`), and a ⌘/Ctrl+V is HELD from the app until its `paste`
  event has handed over the text (then a read answers it) — or, if none comes, until a
  zero timer, when a read answers the page's own last copy or paste; a release never
  overtakes it. A copy writes through `navigator.clipboard.writeText` where the page has it
  (a secure context; checked first, since calling into undefined throws through the wasm
  frames), and the `copy` / `cut` event the key raises carries it too, which needs no
  secure context. Until then a copy in a page panicked (`std::thread::spawn`).
  `scripts/web-probe/clipboard` is the check: copy, paste, copy with `writeText` refused,
  and paste with the `paste` event swallowed, each read back from the system clipboard —
  all four pass in headless Chromium (2026-10-05). (Five since the IME: a `paste`
  swallowed with its default kept pastes into the keyboard sink, an `input` of type
  `insertFromPaste`, which is the paste too.)
- **The keyboard is a hidden `<textarea>`'s** (the keyboard sink, since 2026-10-05), not
  the canvas's: a page composes input-method text only into an editable element. It takes
  the focus a press on the canvas gave the canvas (a canvas focused another way hands it
  over, and a blur over to the canvas is not a focus loss); its keys are the app's as the
  canvas's were, except one the input method takes (`isComposing`, keyCode 229); its
  `input` events while composing are the composition (`Driver::preedit`, the cursor from
  its selection via `dom::utf16_range_to_bytes`), `compositionend` the commit, and text
  with no composition (an emoji panel, dictation) a commit as it comes. After each frame
  it is moved to the editing widget's caret (`ime::caret`), where the candidates open; a
  composition a widget dropped (`ime::take_reset`) is cancelled by blurring and refocusing
  it INSIDE the turn, where the events that raises reach no handler.
  `scripts/web-probe/ime` is the check, through Chromium's own IME path (CDP
  `Input.imeSetComposition` / `insertText`): a composition shown with the sink at its
  caret, the commit replacing it, a cancelled one leaving the box, a no-composition
  insert, and plain keys — all six pass (2026-10-05), and the 24-step demo replay is
  identical to the pixel to the run before the sink through step 18.

Not there yet: drag and drop, file dialogs, and an app whose
text is not the display list's (`display_list_text` false — it stages its own through the
native-only `stage_renderer`, so draws no text here).

**Compute jobs run in the browser too** (`web::ComputeDevice`, since 2026-10-05). What a job
IS moved out of `vk` into the portable `crate::compute` — `Kernel`, `Binding`, `BindKind`,
`workgroups`, `MAX_BINDINGS`, and the rules a job is held to before any device sees it
(`check_job`, the ping-pong `slot_for` / `result_slot`, `parse_kernel`: naga's WGSL
frontend, now a dependency on every target, validates a kernel and reads its
`@workgroup_size` — WebGPU can report neither) — and `vk::compute` re-exports every one at
its old path. The browser device takes the same jobs and answers them the same way, with
one difference the platform makes: readback is a promise, so its `run`, `run_over`,
`run_passes`, `run_passes_over` and `workgroup_size` are `async`. Two things WebGPU does
differently underneath: its layouts tell read-only storage from read-write (the module
says which, `ParsedKernel::read_only_storage`), and a device starts at the spec's default
of eight storage buffers a stage, so it asks for the adapter's own (SwiftShader offers
ten; a job past the adapter's ceiling is an `Err` naming the limit). What WebGPU rejects
is caught in a validation error scope and returned. `examples/compute_probe/jobs.rs` is
the check — a map, a uniform, a 33- and a 34-pass ping-pong, a 2D dispatch, ten bindings,
a bad kernel and a missing entry, each exact against a CPU reference in f32 — run by
`compute_native` and by `scripts/web-probe/compute`: the two outputs are identical to the
bit (lavapipe vs SwiftShader, 2026-10-05). A reference written for a length that is not
a multiple of four floats must know that `arrayLength` counts the 16-byte padding, on
both devices.

**3D scenes draw in the browser too** (`Stage3D`, since 2026-10-05). What an app stages a
scene through is a trait, `draw::scene::Stage3D` (`create_mesh`, `update_mesh`,
`stage_scene`, `stage_scene_images`, `set_scene_light`), implemented by `VkRenderer` (each
method its inherent one) and `WebRenderer`; the scene's types (`Vertex3D`, `MeshId`,
`SceneDraw`, `SceneImage`), its uniform blocks (`scene_uniforms`), image quads and the
wire-base depth bias (`wire_base_bias`) moved to `draw::scene`, and `scene3d.wgsl` /
`scene3d_image.wgsl` to `draw/`, shared by both renderers; `vk` and `engine` re-export them.
Two portable `Application` hooks take a `&mut dyn Stage3D`: **`init_3d`** (once per
renderer — make meshes) and **`stage_3d`** (every frame, just before the draw; true asks
for another frame). Natively `renderer_init` and `stage_renderer` forward to them by
default, as `new` forwards to `create`, so an app that overrides the native hooks (the
designer) is untouched, and one that moves to the portable pair runs on both shells. The
WebGPU pass (`web/scene.rs`) is the Vulkan `SceneStage`'s port: a full-size backdrop in the
canvas's sRGB view format with a depth32 buffer, copied into the canvas under a UI pass
that LOADS it and samples it for blur — and kept, as on Vulkan, until the next staged
scene. Depth bias is pipeline state in WebGPU, and a WebGPU line is one pixel (no
wideLines), so the biased fill is a pipeline of its own at `wire_base_bias(1.0)` — what a
Vulkan device without wideLines uses. `scene3d.wgsl` takes its derivatives at the top of
`fs_main` (WebGPU rejects one under a branch on a varying); natively pixel-identical.
`examples/probe3d/scene.rs` is the check, on the portable hooks — background quad, flat and
prelit fills, a wire-carrying fill and its wires, a see-through fill and its edges, an
image in the scene before the translucent draw, a host light, frost over the pane — run by
`probe3d_native` and `scripts/web-probe/probe3d`: 2026-10-05, lavapipe vs SwiftShader,
195 px differ by more than 8 levels, all on 1 px wires (where along its length a line
steps a row is the rasterizer's), everything else within 2.

**A mesh update does not wait for the GPU** (since 2026-10-07,
`SceneStage::update_mesh` / `update_lit_mesh` through `Mesh::replace`). It
called `device_wait_idle` first, reasoning that geometry updates are rare;
a playing simulation updates several meshes every frame, and the wait made
each frame's upload wait out the previous frame's GPU work. Now the new
vertices go into another buffer — a spare of the mesh's that no submitted
frame still reads, or a new one — and the replaced buffer becomes a spare
tagged with the frames submitted so far (`SceneStage::submitted`, counted
at each submit). After each frame-slot fence wait, `frame_waited` knows
which frames have finished, releases spares too small for their mesh and
keeps two of the rest, so a steady playback allocates nothing. Measured in
the designer's replay at 57k points: the stage pass of a drawn frame 5.6–6.0 ms
to 3.6–3.8.

**A scene draw can be instanced** (since 2026-10-06, `SceneDraw::instances`). A draw
naming an instance mesh draws its `mesh` once per vertex of that mesh: an instance is a
`Vertex3D` read as an offset added to every vertex and a colour multiplying theirs, so a
white mesh takes each instance's colour. `scene3d.wgsl`'s vertex stage takes the instance
at locations 2 and 3, and every mesh pipeline on both renderers has a second, per-instance
vertex binding. A draw with `instances: None` is drawn for ONE instance at the origin in
white (`draw::scene::UNIT_INSTANCE`, a 24-byte buffer each stage keeps), which leaves its
vertices bit for bit as they were (`x + 0.0`, `c * 1.0`), so nothing that does not ask
for instancing changed; the pipelines and their count are the same. An instance mesh with
no vertices draws nothing. It exists for the designer's point markers, which were a
240-vertex sphere copied to every point and uploaded whole each frame of a playing
simulation — 46 MB at ten thousand points, where instanced they are 240 KB. The probe has
a row of instanced cubes, and Vulkan draws it (2026-10-06, a shadow); the WebGPU half
(`web/scene.rs`: the instance buffer layout with `GpuVertexStepMode::Instance`, slot 1,
`draw_with_instance_count`) was written alongside and NOT built — this machine's
toolchain has no wasm32 target — so `scripts/web-probe/probe3d` is the first thing to run
on one that has.

**A screen-space draw says so** (since 2026-10-07, `SceneDraw::screen_space`). A pane's
background quad is a mesh whose vertices are NDC corners; the draw sets `screen_space`, the
uniform block carries it in what was `_pad`, and `scene3d.wgsl` places those vertices at
their own xy on the far plane, unlit, without the mvp. Until then the signal was IN the
vertex data: any vertex of any mesh within 0.01 of z = 9.99 became a background corner, so
real geometry spanning that plane in its own units (cce-model's 25 mm STL sphere; a
designer Transform at z = 9.99) tore into spikes across the pane. Never key behaviour off
vertex values again. The probe has a regression for it: a sphere modelled around z = 9.99
and scaled into place by its mvp, which the old shader shredded and this one draws whole
(Vulkan, scale-2 shadow; the WebGPU half shares the shader and `scene_uniforms` and was
not built — still no wasm32 target here).

**A mesh can be lit and textured** (since 2026-10-07, `draw::lit`). Beside the
`Vertex3D` path, which is a position and a colour shaded flat or pre-lit by the host,
the scene pass has a second mesh kind for model files: a `LitVertex` (position,
normal, uv, colour), drawn by `scene3d_lit.wgsl` with a `LitMaterial` (base colour,
optional base-colour image id, metallic, roughness) under one `LitLight` (key, fill,
sky/ground ambient). Lambert plus GGX/Smith/Schlick per light, the hemisphere as the
diffuse ambient and as what a metal reflects; the diffuse has no 1/pi, the scale
hosts' baked light always had, so a matte lit draw matches a baked one. It is reached
through `Stage3D::lit()`, which **defaults to `None`**: the Vulkan renderer answers
`Some(self)` (`LitStage3D`: `create_lit_mesh`, `update_lit_mesh`, `set_lit_light`,
`stage_lit`), the WebGPU one does not yet, so it needed no change and a host checks.
`LitDraw::before` interleaves lit draws with the staged `SceneDraw`s as
`SceneImage::before` does (a host's background and grid first, wire overlays after).
What the Vulkan stage does (`vk/scene.rs`): a lit pipeline on the image pipeline's
layout (set 0 the per-frame dynamic uniforms, set 1 an image's descriptor set), no
culling (the shader flips a back face's normal), dynamic depth bias for
`wire_base_width`; lit uniform blocks share the per-frame buffer after the scene's and
the images', so the slot stride is now the larger of the two blocks (`SLOT_SIZE`, 224
bytes against the scene block's 128 — nothing that does not use lit draws changes but
that stride); an untextured draw, or one whose image is not resident, binds a 1x1 white
image the renderer uploads with the first lit mesh. Textures are ordinary image ids:
RGBA8 sRGB, so texels reach the shader linear; they die with the renderer (re-upload
in `init_3d`). The image sampler clamps, so the shader wraps uvs with `fract` and
samples with the UNWRAPPED uvs' gradients (`textureSampleGrad`), or the wrap would
pick the smallest mip and draw a seam. cce-model is the consumer. Verified 2026-10-07
in a scale-2 shadow: a textured globe, a gold metal sphere and a rough red cube, the
highlights moving with the camera; and `probe3d` drawn pixel-identical before and
after the change (the flat path untouched).

**And so does the path tracer** (since 2026-10-05). `Stage3D` carries the tracer's half
too — `set_rt_scene` / `set_rt_scene_with_image`, `set_rt_environment`,
`set_rt_background`, `stage_rt`, `rt_accumulating` — and what it traces from moved to
`draw::rt`: the schema (`RtTriangle`, `RtMaterial`, `RtImage`, `RtCamera`,
`RtEnvironment`), the binned-SAH BVH and its tests, the buffers (`pack_scene`) and
parameter blocks (`rt_params`, `denoise_params`) as the shaders read them, and the
constants; the shaders (`rt_common` / `rt_bvh` / `rt_query` / `rt_denoise`) moved to `draw/`.
`vk::rt` re-exports the schema and keeps its own state (frames in flight, the ray-query
tier, `RtOffscreen`) — it now packs and lays out through `draw::rt`, verified by its
GPU tests (`cargo test --lib rt -- --ignored`, lavapipe). `web/rt.rs` is the compute tier on
WebGPU: the same shaders and packing, the same rules for restarting the accumulation, one
sample a frame plus the three à-trous iterations in one compute pass. Two differences:
WebGPU has no ray tracing, so there is no ray-query tier (the compute tier is what every
Vulkan device without RT cores runs too); and Vulkan BLITS the tracer's `rgba8unorm`
image into the sRGB backdrop, converting as it copies, which WebGPU's copies cannot — a
small render pass loads each texel and writes it through the backdrop's sRGB view, the
same conversion. The probe's traced mode (`Probe3d<true>`, `PROBE3D_TRACE=1` natively,
`scripts/web-probe/probe3d <out> traced`) stages exactly eight frames and stops, so both
are compared at eight samples: 2026-10-05, Vulkan compute tier (`CCE_VK_RT=compute`) on
lavapipe vs SwiftShader, the traced pane's mean differs by 0.10 of a level, every pixel
within 8, 47 channels in the frame past 8 — a few paths that diverged.

**A large traced scene is prepared off the UI thread** (since 2026-10-07).
`set_rt_scene` builds the BVH where it is called, and an app only has the stage inside
`stage_3d`, so 5M triangles froze cce-model for 2.3 s. `PreparedRtScene::new(tris, mats,
image, with_bvh)` is the CPU half — packing into the buffer layouts plus the BVH — and is
`Send + Sync`, built on a worker; `Stage3D::set_rt_scene_prepared(&scene)` only uploads
(and keeps it: a reconnect re-uploads the same one). `with_bvh` is `Stage3D::rt_needs_bvh()`,
asked on the UI thread first: false on the Vulkan ray-query tier, which builds its BLAS on
the GPU from the packed triangles and ignores a BVH; a scene prepared without one on a
compute-tier renderer gets it built at upload (`PackedScene::with_bvh`), so a wrong answer
costs time, never a wrong image. `set_rt_scene` / `set_rt_scene_with_image` are wrappers
over the two halves. The GPU tests run per tier with `VK_DRIVER_FILES` pinned (Intel =
compute, NVIDIA = ray-query, NVIDIA + `CCE_VK_RT=compute`); no lavapipe ICD is installed.

**The reference app runs on both, through one input script.** `examples/demo_web.rs` is
`src/main.rs`'s `DemoApp` (included by `#[path]`, hence `pub(crate)`) in a page;
`scripts/web-probe/demo <dir>` builds it, serves it with the machine's fonts and replays the
native harness's 24 steps (`drive.mjs`: moves, clicks, a drag, a wheel, typing, undo, the
menu, Tab, held keys, the CSD bands), one captured frame per step, which `compare.py` diffs
against the native run's screenshots (`--mask` the cursor's box; never sway's
`hide_cursor`, which clears pointer focus, so the native app drops its hover). Measured
2026-10-05 against lavapipe: steps 00–18 differ only at the slider band's two pointed tips,
1 px of rasterizer tie-break (≤ 63 channels beyond 8 levels; everything else within 2),
with `DEMO_FAMILIES=FreeSerif,FreeSans,FreeMono` — what native's fontdb made of this
machine's fontconfig, not `fc-match`'s DejaVu. Steps 19–20 hold a key, and the driver
repeats once per turn: SwiftShader takes ~250 ms a frame, so the page gets fewer repeats
than native in the same 1.5 s. Timing, not routing.

**The renderer probe holds the two renderers to each other.** `examples/probe/scene.rs`
is one 1280x800 frame of nearly every prim — root, pane and frosted plates, every control
stance, fields, carves, bevel, sphere, grooves, vector caps, text at four sizes in two
families, an image at two sizes. `cargo run --example probe_native` draws it through
Vulkan (a Wayland session; screenshot it); `scripts/web-probe/run <out.rgba>` builds
`probe_web` for wasm, binds it with wasm-bindgen-cli (the version in Cargo.lock) and draws
it in headless Chromium on SwiftShader, reading the frame back from the GPU; and
`scripts/web-probe/compare.py native.png out.rgba 1280 800` diffs them. Both halves must
have the same fonts — the native one with `CCE_LOAD_SYSTEM_FONTS=1`, the web one handed the
DejaVu files (`$PROBE_FONTS_DIR`) — and the screenshot must not carry a cursor (sway:
`seat * hide_cursor 200`), which is a difference the diff cannot tell from the renderer's.
Lavapipe against SwiftShader, 2026-10-04: 96.8% of channels equal, every other within 2
levels but ONE at 3 — rounding at antialiased edges and in the blur, no shading difference.
Chromium needs `--use-angle=swiftshader --enable-unsafe-swiftshader
--disable-gpu-compositing` beside the WebGPU flags (`browser.mjs`): headless, with GPU
compositing it has no shared-image backing for a WebGPU canvas and loses the device on the
first present ("A valid external Instance reference no longer exists").

**And on a Mac, type-checked only** (`src/mac`, since 2026-10-05). The fourth shell is an
AppKit window over the same `Driver`, `Pacer`, `build_frame` and **Vulkan renderer, on
Metal through MoltenVK**: `vk::SurfaceTarget` names what a window's `VkSurfaceKHR` is made
from — `Wayland { display, surface }` or `Metal { layer }` (a `CAMetalLayer`) —
`VkRenderer::try_new_for` / `attach_surface_to` and `VkCore::new_for_surface` /
`create_surface` take one, and the Wayland-pointer forms forward to them unchanged. The
instance enables VK_EXT_metal_surface when the loader offers it, and on macOS only
VK_KHR_portability_enumeration (the loader lists MoltenVK to no instance that does not ask);
a device that offers VK_KHR_portability_subset gets it enabled, as the spec requires, and
no Linux driver offers it — so a Linux instance and device are the ones they always were.
`engine::run::<App>()` is the AppKit shell's `run` on macOS, so a client's `main` does not
change. What the shell does, in AppKit's terms (module doc in `src/mac/mod.rs`):

- **The window**: transparent, its titlebar transparent over full-size content, so the root
  plate fills it with the traffic lights on its corner. AppKit resizes from its own window
  edges, so the driver's CSD resize band is the app's (`PressSite::own_edges`, new; false
  on the other shells); a press the driver reads as a move drags the window
  (`performWindowDragWithEvent:`).
- **Events**: a flipped, layer-hosting `NSView` maps mouse, scroll, magnify and keys through
  `backend::appkit` (portable, tested on Linux like `dom`): named keys from the hardware key
  code (AppKit spells them in private-use characters, and Backspace as DEL), the rest from
  `characters`, or with ⌘/Ctrl from `charactersIgnoringModifiers` so ⌘Z is z typing ^Z;
  Command reads as `ctrl`, as in a page on a Mac; a Control-click is a right click. AppKit
  sends NO keyUp for a ⌘-combination, so the shell releases one as it presses it (else the
  driver repeats ⌘Z until focus is lost). Its key repeats are dropped (the driver repeats)
  and so is the system's scroll MOMENTUM (the toolkit coasts a flick itself; both would
  coast twice). The system has applied natural scrolling to the wheel too, where a Linux
  compositor applies it to the trackpad alone, so a wheel notch is turned back to the
  wheel's own direction, and each trackpad event's `isDirectionInvertedFromDevice` sets
  `input::force_natural_scroll` on the main thread: the system setting rules, not input.kdl's.
- **Pacing**: main-queue dispatches (`dispatch2`); an event or any `AppSender::send`, from
  any thread (`app::set_wake`, process-wide on macOS, per thread in a page), asks for a turn
  at most one ACTIVE frame after the last; between, the pacer's sleep. A superseded turn is
  dropped by its generation. Quit (⌘Q) and the close button ask the app to exit as a
  compositor's close does; the run loop is stopped once it has.
- **Fonts**: the system set is always loaded on macOS (`build_font_system`) — it is what
  cosmic-text's macOS fallback list names.
- **Clipboard**: the general `NSPasteboard`'s plain-text type, behind the same
  `widget::clipboard` pair every widget uses; ⌘C / ⌘X / ⌘V reach the widgets as Ctrl+C /
  X / V do on Linux, since Command reads as `ctrl`.
- **Input methods**: the view is an `NSTextInputClient`. While a widget is editing text
  (`ime::caret` is set) a key press without ⌘ goes through `interpretKeyEvents:` first:
  `setMarkedText:` is the composition, `insertText:` the commit — unless it is a plain
  key typing its own characters with nothing marked, which is left to the key path so it
  keeps its named key and the driver's repeat — and `doCommandBySelector:` leaves the key
  to the key path. `firstRectForCharacterRange:` is the caret in screen coordinates. With
  nothing editing, keys skip the input method, so one left on does not eat an app's
  single-key commands. After each frame a dropped composition is discarded through the
  input context (the marked text cleared first, so the `unmarkText` that may call commits
  nothing) and a moved caret invalidates the character coordinates.

Not there yet: drag and drop, the context menu
in a popup window (it is drawn in the window, as on a layer surface), blur behind the window,
a menu bar beyond Quit. **None of it has run**: this is Linux, where an Apple target can be
type-checked but not linked. `scripts/check-mac` type-checks the library, the demo, every
example and the tests for `aarch64-apple-darwin` (`rustup target add aarch64-apple-darwin`);
the four examples that still used the legacy `new(qh, …)` moved to `create`, and
`plate_probe` / two integration tests reach the tessellator at `backend::tessellate` rather
than through `window_runner`. On a Mac, MoltenVK and the Vulkan loader must be installed
(the LunarG SDK, or Homebrew's `molten-vk` and `vulkan-loader`); `cargo run` is the test.

**Input-method composition is one model for every shell** (`crate::ime`, since
2026-10-05). Three things cross between the text widget and the shell's input method:
the COMMIT is delivered as typed text (`Driver::commit_text`: a press of a key whose text
it is, then its release — never a shortcut, never repeated, past the chords), so every
widget that inserts a key's text takes it unchanged (`TextBox`, `LineEdit`, the
`DocEditor`, an app's own field); the COMPOSITION (`ime::Preedit`: text and the input
method's cursor as a byte range) is shared per thread, set through `Driver::preedit`; and
the CARET goes back — a widget editing text reports it as it paints
(`ime::report_caret`, in window px with the `PaintCtx` offset), `build_frame` brackets
the frame (`begin_frame` / `end_frame`), and `ime::caret()` is where the candidates go and
whether text is wanted at all. **`TextBox` shows a composition as a PROVISIONAL run** in
`edit_buffer` (`composing`: its char start and length), so wrap, scroll, caret and the
glyph advances draw it as typed text, and `selection_quads` underlines it; it is never
held (`committed_buffer`, which `take_change` publishes under `update_on_type`), never in
the history, and the box takes no key while it composes. It is applied in `prepare_text`
and at the top of `handle_key`, against `ime::generation`; a press, or editing ending,
drops it and asks the input method to cancel (`ime::request_reset`). A composition begun
over a selection replaces it, as typing would. `a_composition_is_shown_in_place_and_the_
commit_is_typed` is the test.

**`LineEdit` and the `DocEditor` show it too** (since 2026-10-05), each without letting it
into what it holds — a host reads `LineEdit::text` directly and saves the `DocEditor`'s
buffer, so neither ever contains it. `LineEdit` splices it into `display()` at the caret,
and `display_index` / `text_index` map across it (the caret lands where the input method
has its cursor, a point inside the composition is the caret, one after it is the text it is
drawn after); `composition_range` is the span to underline, a masked field shows bullets.
The app, which draws the field, calls `sync_ime` each frame the field has the keyboard,
reports the caret it draws (`ime::report_caret` — also what tells the shell text is
wanted), and `drop_composition` when the field loses it. The `DocEditor` lays out the
caret's line WITH the composition (an active line, raw anyway) and maps every column read
off that layout across it (`laid_col` / `source_col`: the caret, `caret_rect`, `pos_at`);
it underlines it, and reports its caret itself while painted focused; a host calls
`drop_composition` when the editor loses the keyboard. Both take no key while a
composition is up, take the commit as typed, and treat a press as dropping the
composition (cancelled in the input method) and placing the caret — the `DocEditor`'s read
through the line as drawn. `a_composition_is_shown_at_the_caret_and_never_held` and
`a_composition_is_laid_out_in_place_and_never_held` are the tests; cce-notes, built against
this tree, was driven under the headless sway with the stand-in input method (2026-10-05):
the composition underlined at the caret, the commit typed, a second composition left up
through the editor's autosave and then dropped by a click — and the note on disk held the
commit and never the composition. cce-browser's URL bar, bookmarks search and dialog
fields are the `LineEdit` hosts (its `keyboard_field` / `sync_ime`, lsgalante/cce-browser#1).

Not there yet: no shell sends surrounding text, so an input
method's `delete_surrounding_text` (text-input-v3) is not applied; and a password box is
announced with the normal content purpose.

**On Wayland it is `text-input-v3`** (`backend/text_input.rs`, since 2026-10-05), relayed
by the compositor to an `input-method-v2` client (fcitx5, IBus's Wayland frontend). The
text input is the first keyboard seat's, made with the keyboard. After each render
(`EngineState::sync_text_input`) it is ENABLED while the seat's text-input focus is on our
surface (`enter`) and a widget is editing (`ime::caret`), with a normal content type and
the caret as the cursor rectangle (surface px — the app's logical px times a forced scale,
as pointer input is divided), re-sent when the caret moves; DISABLED when nothing is
editing; and disabled-then-enabled for a composition a widget dropped (`ime::take_reset`),
which resets the input method. Every change is one `commit`, counted (`TextInput::commits`,
what a current `done`'s serial is). `preedit_string` / `commit_string` /
`delete_surrounding_text` are double-buffered and applied on `done` in the protocol's
order (`Batch::apply_order`: the old composition out, the commit typed, the new one in;
a batch with no `preedit_string` ends the composition, a cursor of -1 hides it); `leave`
drops the composition. The decisions are pure (`TextInput::plan`, `Batch`) and tested
with no compositor (`backend::text_input::tests`). Verified end to end under the headless
sway with a scriptable `input-method-v2` client standing in for fcitx5 (2026-10-05): no
activation until a box is clicked into; a composition shown underlined at the caret; the
commit replacing it; a cancel; a press mid-composition dropping it with a
disable-and-enable; Escape disabling — and under `WAYLAND_DEBUG` the cursor rectangle
following the caret through every step, each `done`'s serial equal to the commits sent.
Sway routes text-input focus only while an input method is bound, so with none (the
24-step harness) nothing changes: 0 px.

CI (`.github/workflows/ci.yml`, every push and PR) has five jobs, warnings as errors in the first four:

- **`test`** (Ubuntu 24.04) builds and tests with default and with all features. It installs
  `libwayland-dev` and `libxkbcommon-dev` (the two native libraries the build links, through
  pkg-config), Mesa's lavapipe, a software Vulkan device, so the GPU tests (`vk::compute`,
  `vk::plate_probe`) RUN rather than skip — a last step fails the job if they printed a skip
  note, since a skipped test passes — and `fonts-liberation`, named as `CCE_FONTS_DIR`, so the
  text tests have real faces (with none they fell back to the image's leftovers, and the
  tests needing a second face or a fallback glyph kept the workflow red from 2026-10-07 to
  10-08). It also runs the path tracer's `#[ignore]`d GPU tests on lavapipe's compute tier
  (`CCE_VK_RT=compute cargo test --lib vk::rt -- --ignored`).
- **`clippy`** runs `cargo clippy --all-targets --all-features -- -D warnings` (since
  2026-10-08). Five lints are allowed as house style in `Cargo.toml`'s `[lints.clippy]`, each
  with its reason (too many arguments, complex tuple types, precise colour constants, index
  loops, `new` without `Default`); anything else clippy reports is fixed, not allowed —
  locally, `cargo clippy -p cce-ui --all-features --all-targets -- -D warnings` is the check.
  Linux only: `src/web` and `src/mac` are compiled out there.
- **`miri`** runs the `widget::owned` tests under Miri, Stacked and Tree Borrows (nightly):
  the app's access to a widget and the registry's taking turns (see "The registry holds
  pointers, and knows when they die").
- **`wasm`** runs `scripts/check-wasm`: the library, its features and the four wasm examples,
  type-checked for the browser. Its `RUSTFLAGS` carries `--cfg=web_sys_unstable_apis` itself,
  since an environment `RUSTFLAGS` replaces `.cargo/config.toml`'s. Its first run (2026-10-08)
  found the browser build broken since 10-06 by two native-only calls in portable code.
- **`macos`** builds every target, LINKED, on a macOS runner and runs the tests — what
  `scripts/check-mac` can only type-check on Linux. `ash` loads Vulkan at run time, so no
  MoltenVK is needed to build, and the GPU tests skip there.

To match the `test` job locally: `apt install libwayland-dev libxkbcommon-dev
mesa-vulkan-drivers fonts-liberation`, then `CCE_FONTS_DIR=/usr/share/fonts/truetype/liberation
RUSTFLAGS="-D warnings" cargo test --all-features`.

Wayland protocol bindings are generated **inline at compile time** by `wayland-scanner` macros in
`src/protocol.rs` from `protocol/*.xml` (`cce-inspector-v1`, `cce-window-management-v1`). The one
build step is `build.rs`, which compiles the renderer's fixed WGSL shaders to SPIR-V once, so a
WGSL error is a build failure and no process pays naga at launch
(`precompiled_spirv_matches_runtime_compile` holds it to the run-time compile).

## The `Application` trait — the client contract

Every client implements `Application` (`src/backend/app.rs`, re-exported from
`window_runner` and `engine.rs`). A client's `main.rs` is typically a struct implementing it plus a one-line
`cce_ui::engine::run::<MyApp>();`. When adding a widget or client, **mirror an existing client**
(e.g. `cce-status-interface`) — do not invent a new structure.

Key methods (see the trait def in `backend/app.rs`):
- `create(sender)`, `settings()` (→ `WindowSettings`), `layer()` (→ optional `LayerSettings` for
  layer-shell surfaces like the status bar), `update(msg, needs_rebuild, exit)`, `tick(dt, …)`.
  **`tick` is not a clock.** Since 2026-09-11 the runner sleeps between ticks while the
  window is idle (no redraw pending, no animation, no key held, no warm-down) — up to
  `IDLE_DISPATCH` (1 s, `CCE_UI_IDLE_MS` overrides) — and is woken by Wayland events and
  by messages on the `AppSender` handed to `create`. It used to tick a flat 16 ms
  forever: every client awake 60×/s doing nothing. So: deliver background results
  through that sender, never by draining a `std::sync::mpsc` in `tick`; if a widget
  or app must poll something the loop cannot see, say so — a widget returns `true`
  from `tick` while the session is live (ColorSelector's picker), an app overrides
  `Application::idle_poll_interval` (cce-authenticator, cce-system-interface,
  cce-designer while a pane is detached). Any animation keeps the frame cadence by
  itself because it reports a change.
- **Construction is `create(sender: AppSender<Self::Message>)`** (since 2026-10-03).
  `AppSender` is cce-ui's own handle — `send`, `Clone`, `Send`, and `From` both ways
  with `calloop::channel::Sender` for a client that still stores calloop's type — so
  the constructor names no window system, which is what lets a second shell (macOS,
  the browser) run the same `Application`. `create` is REQUIRED (since 2026-10-08):
  the legacy `new(qh, sender)`, whose queue handle no client ever used, is gone, and
  so is the default that panicked at startup when an app implemented neither, so a
  missing constructor is a compile error. An app that keeps calloop's sender converts
  on the first line (`let tx: calloop::channel::Sender<_> = sender.into();`).
  `register_sources` stays a calloop-only hook: it is the Wayland shell's, not part of
  the portable contract.
- **Draw**: `display_list()` returns the frame (the one paint path, below);
  `display_list_text` opts its `Prim::Text` into the glyph pass. Two side channels remain:
  `overlay_quads` (flat quads over everything, the status bar's) and `custom_vertices` (raw
  vertices appended as a final unclipped batch).
- **Input**: `handle_pointer_move`, `handle_mouse_input`, `handle_mouse_wheel`,
  `handle_key_input` — most return an optional `Message`. `needs_rebuild: &mut bool` is how a
  handler requests a redraw; the loop is demand-driven and idles when nothing sets it.
- **3D**: `init_3d(stage)` / `stage_3d(stage, size, scale)` — the portable pair, through
  `Stage3D` (see "3D scenes draw in the browser too"); the native `renderer_init` /
  `stage_renderer` take the `VkRenderer` itself and forward to them by default.
- `ui_context()` / `ui_context_mut()` expose the widget tree (`UiContext`) for apps built on the
  retained widget system rather than immediate drawing.
- **Undo/redo**: the runner owns the routing. A press matching the `undo` / `redo` chord
  (`input.kdl`, cce-ui domain defaults `ctrl+z` / `ctrl+shift+z`) goes to the focused widget
  as `ContextAction::Undo` / `Redo` (a TextBox that is editing steps its own typing), then to
  the app's `undo(needs_rebuild)` / `redo(needs_rebuild)` hooks (default false); only if both
  decline does the key reach `handle_key_input`. Apps keep their own document history on
  `cce_ui::history::History<T>` — snapshots of the app's state type, with gesture/group
  coalescing and the fork-on-new-edit rule built in (module doc in `src/history.rs`).

The frame loop is demand-driven (single `redraw` dirty bool, gated by a Wayland frame-callback
vsync) — it idles correctly when nothing changes. Don't add per-frame I/O to the render hot path.

### A lost surface ends the session; it does not panic (since 2026-09-25)

`VkRenderer::try_new` returns `vk::SurfaceLost` when the display connection under the
surface is already dead — Mesa's Wayland WSI answers the surface queries with a roundtrip,
so `vkGetPhysicalDeviceSurfaceFormatsKHR` is the first call to find out, with
`ERROR_SURFACE_LOST_KHR`. The runner ends that session as `ConnectionLost`: a reconnect if
the compositor is still there, a clean exit if it is not. Mid-session, a swapchain
rebuild, acquire or present that reports the surface lost latches `surface_lost()` and
skips draws (one WARN) until the event loop sees the dead connection itself; the menu
popup just closes. `try_new` is the ONLY constructor: the panicking `new` was removed
once its last callers (cce-cloud, cce-lock, designer's `vk-smoke`) had moved over, so a
new client cannot pick the one that takes the process down at logout. Found as
cce-cloud's daemon panicking at logout on `No surface formats`: it had outlived a
compositor and asked for a window over its connection. Reproduced by opening a
`wl_surface`, killing the shadow compositor, then constructing: `try_new` returns the
error where the old `new` panicked.

### A daemon can outlive its compositor (`Application::outlives_compositor`, 2026-09-25)

When the compositor is gone (`SessionEnd::NoCompositor` — nothing at the socket) the
runner EXITS by default: the compositor saves windows for restore and its successor
respawns them, so a client that rejoined came up beside its own copy (2724002). That is
wrong for a process the compositor does not restore — a systemd user service like the
status bar or the notifier, which must outlive it and whose D-Bus names other programs
depend on. Such an app returns true from `outlives_compositor`; the runner then waits for
the successor's socket (`await_compositor_socket`, a 250 ms poll) and rejoins it with the
same `Application`. Found when the status bar was rebuilt against 2724002: at every logout
its modules exited, the launcher's backoff grew while nobody was logged in, and the tray's
StatusNotifierWatcher came back seconds after the next login — Dropbox, starting into the
gap, reported no tray.

### The toolkit names no app (since 2026-10-08)

Nothing in cce-ui branches on WHICH app is running. The runner used to read the app id:
anything whose `app_id` began `cce-status` lost its client-side move, resize and resize
cursors, and had its input region pinned to its launch size. The status bar and its OSD
now say so through the contract — `Application::standard_csd` returns false — and the
input region went (the segments' menus grow the surface past it, and a row below the old
region took the click before and after; checked in a shadow). The same sweep moved the
designer's pane geometry (`SplitterLayout`, `CircularPaneLayout`) into cce-designer and
the relief editors (`cce-relief`, `cce-ramp`, until then bins of this crate) into the
`cce-relief` crate. When an app needs the runner to behave differently, add a defaulted
`Application` hook; never test the app id. (Two widgets LAUNCH a DE app by name — the
colour selector `cce-color-editor`, a default its host can replace, and the font selector
`cce-fonts` — which is the DE's toolkit using the DE, not a branch on the app.)

### The runner works out each frame's damage (`backend::frame::derive_damage`, 2026-10-06)

An app that does not report its own damage (`take_damage`, which only cce-grid does) no
longer repaints its whole window per frame: the Wayland shell diffs the tessellated
batches (vertex bytes, scissor, clip, plate push, blur flag), the display-list text (line
text, position, colour, size, clips) and the image quads against the last built frame,
and damages what changed — where it was and where it is, over the common prefix and
suffix. Ids whose pixels `update_pixels`/`update_pixel_regions` replaced are damaged where
drawn. It gives up (full frame) on a new size, scale or clear colour, changed plate
carves, a frame owed after a skipped present, an app that stages its own text
(`display_list_text` false), and a 3D backdrop. `CCE_UI_FULL_DAMAGE=1` turns it off;
`CCE_PRESENT_DEBUG` logs each derived rect.

A frosted (blur-behind) batch used to force every frame full. Now the renderer grows the
partial region instead: a frosted plate the region touches is repainted whole plus its
blur's reach (`BLUR_REACH_PX`), until nothing more is touched — its blur reads a snapshot
that is only right inside the region. The first-drawn frosted batch over a transparent
clear with no scene (the root plate, frosted in every themed app) samples the zeroed
backdrop instead of a snapshot (`first_frost_exempt`): the same pixels, no copy, and no
dependence on what lies outside the region. `write_window_info` marks every image stale.

Checked by pixel A/B in a scale-2 shadow, full repaint forced vs derived, the window shot
after each step: the demo (hover grid, then focus + typing), cce-gallery and the settings
app's Notifications page matched exactly; differences on its System and Power pages were
live data (uptime, temperatures, a different saved plan per shadow home).

### A layer app can have no surface while it is empty (`Application::wants_surface`, 2026-10-05)

A layer-shell app that is usually empty — the notifier, between notifications — returns
false from `wants_surface` while it has nothing to show. On that turn the runner detaches
its renderer (`VkRenderer::detach_surface`: the swapchain and `VkSurfaceKHR` go, the device,
pipelines, atlases and image table stay) and drops the layer surface (SCTK destroys the
role, then the `wl_surface`); on the turn it says true again it builds a fresh `wl_surface`,
re-attaches the same layer role, moves the SAME renderer onto it (`attach_surface`, as the
menu popup's renderer moves between popups), and the first configure makes it presentable
as at session start. The app
keeps running throughout — its calloop sources, D-Bus thread and state are untouched; only
the surface goes. Why bother: an always-mapped transparent overlay still made the
compositor blur behind it whenever anything under it changed, and it kept a fullscreen
client off direct scanout (scenefx scans out only a one-entry render list).

Since the renderer is the same one, image ids stay good across the gap and
`renderer_init` does not run; ids uploaded while hidden are drained into it at its next
frame. Until 2026-10-08 the runner dropped the renderer and built a new one on show — a
device and every pipeline, about 45 ms before a card after an empty spell appeared — and a
`surface_hidden` hook told the app, whose `renderer_init` then had to skip re-uploading what
was already queued (the notifier kept a flag for it). Both are gone. Only if attaching fails
does the surface stay hidden with no renderer; the next show makes a new one and calls
`renderer_init` as for a replacement. It is also what a launcher like cce-cloud, which keeps
one renderer for its life and moves it between popups, would need from the runner. Checked
in a scale-2 shadow on a private session bus: a card, its expiry, a second card with a
thumbnail after the empty spell — drawn identically to the pixel by the old binary and the
new. Default true; xdg windows ignore it.

### `renderer_init` — GPU handles do not survive a reconnect

A connection is one **session**. A Wayland transport cannot be repaired once it breaks,
so `run` opens a *new* session around the same live `Application` — same app state, same
calloop loop, same message channel, but a new surface, a new swapchain and **a new
`VkRenderer`**. `renderer_init(&mut self, renderer)` is called once per session: the
first call is the process's own renderer, every later call is a replacement.

What that costs you: an id from `vk::upload_rgba` names an entry in **one renderer's**
image table, and `Frame2D` **skips a draw for an unknown id without logging it**. So any
image id cached across frames — in a struct field, an LRU, a `static` — silently stops
drawing after a reconnect, while every other part of the window keeps working. That
asymmetry is the tell: numbers and text intact, pictures gone.

The fix shape, in every client that needed it, is one method:

```rust
fn renderer_init(&mut self, _r: &mut cce_ui::vk::VkRenderer) {
    if std::mem::replace(&mut self.seen_renderer, true) {
        // …drop the dead ids and arrange for the pixels to be produced again
    }
}
```

Act only on the second and later renderer: uploads queued before the first one existed
are drained into it, so dropping them there just uploads, destroys and re-uploads
everything before the first frame. Freeing a stale id is always safe and worth doing —
`ImageStage::destroy_image` returns early on an id it does not hold, and `NEXT_ID` never
resets, so a stale id can never collide with a live one. The failure is always "draws
nothing", never "draws the wrong picture".

Three traps, each of which cost a session real time in the 2026-09-19 sweep:

- **A widget can hold the id too.** `Button::with_icon(id, …)` captures what you hand it
  and outlives the renderer, so invalidating a cache underneath it changes nothing on
  screen. For bundled cce-icons artwork use `Button::with_icon_name` / `Button::new_icon`,
  which hold the NAME and re-resolve through `upload_icon` per read; `upload_icon`'s own
  cache is keyed on `vk::renderer_epoch()`. `with_icon` still means "the app owns this
  upload", which is right for app-rendered content — and carries the app's duty to
  re-set it from here. `ImageView` borrows its id on the same terms.
- **A WPE client does not self-heal.** Nothing provokes a repaint of a page that has
  finished loading, so `pump` finds no buffer held and the stale id just stays stale.
  cce-mail replays `MailWebView::last_frame`; cce-browser has to remap the active view,
  the same nudge `activate` uses. (A page that happens to animate *would* recover on its
  own, because `update_pixels` recreates an image under an id the new table lacks — which
  is exactly how this hides from whatever page you test with.)
- **An in-flight worker result can carry a dead id.** A thread that uploaded just before
  the drop delivers an id naming nothing, and a store caches it as an entry that draws
  blank for as long as it stays resident. `cce-preview`'s `PageStore` and `cce-map`'s
  `TileManager` carry a generation for this and free a mismatched result on arrival.

Verify with `CCE_UI_FAULT_RECONNECT` (below) — **and run the pre-change binary through
the same fault first.** A fix that passes a test which never reproduced the bug is worth
nothing, and both of the above traps first showed up as a "fixed" build that still drew
nothing.

## Rendering: one paint path (the Phase 3 state)

The backend `render()` **always builds a `scene::paint::DisplayList` and tessellates that single
list** (`backend::frame::build_frame`, which the Wayland shell's `EngineState::render` presents).
An app feeds it by returning `Some(DisplayList)` from `Application::display_list()`; `None` is
an empty frame. (Until RFC phase 6al a `None` made the backend wrap the app's legacy `view*`
tuples into a list instead; those sinks are gone, and every app implements `display_list`.) `custom_vertices` is
appended as a final unclipped batch drawn on top.

## The context menu draws in its own popup surface (since 2026-09-25)

`widget::context_menu` is one global menu that every app shows, paints into its own
display list and dispatches by window coordinates. Drawn in the window it was cut off
at the window's edge, and a menu taller than the room left could not be seen at all.
So on an xdg toplevel the runner mirrors the open menu into an `xdg_popup`
(`backend/menu_popup.rs`, with the reasoning in its module docs): the compositor may
put it anywhere on the output, and the positioner's flip-y / slide / resize-y keeps it
there. A menu cut short scrolls: `ContextMenuState` keeps `content_h` (all the rows)
apart from `h` (what is shown) and a `scroll`, and `row_at` / `row_y` / `hit_test`
answer for the rows as DRAWN — every host that dispatches through them scrolls for free.

A popup path existed before and was deleted in July (Phase 6x) for drawing in one
place and hit-testing in another. Two rules make this one different, and both are
load-bearing:

- **The configure is written back.** Where the compositor put the popup is where the
  menu IS (`context_menu::place`), so the rect apps hit-test is the rect on screen.
- **The popup takes its own pointer input**, translated by its offset into window
  coordinates (`pointer_frame`), so apps need no change — their menu coordinates may
  now simply lie outside the window. The CSD move/resize checks are skipped for it.

While the popup is up the menu is `hosted`: the apps' in-window `paint*`,
`text_labels` and `extra_quads` draw nothing, and the runner paints a copy at the
origin (`paint_hosted`). There the plate is the surface's ROOT, so its frost is the
compositor's blur-behind, not the in-app pass — which has nothing to sample inside a
popup and resolves to flat opaque grey. The compositor's blur cannot compress luminance
the way the in-app frost does, so the root plate's alpha is raised to
`1 - (1 - a)(1 - k)` to let the backdrop through by the same amount. The compositor
blurs popups since the same date (`xdg_popup.rs`'s `update_blur`).

The popup's renderer is kept across opens: `VkRenderer::detach_surface` /
`attach_surface` move it from one popup's `wl_surface` to the next, so a re-open costs a
swapchain rather than a device and every pipeline. **Detach before the popup drops** —
the drop destroys the `wl_surface`, and a swapchain must not outlive it.

Layer surfaces keep the in-window menu, placed by `context_menu::constrain_to` with the
same flip / slide / shorten rules inside the window; so does any app run with
`CCE_UI_MENU_POPUP=0`.

### A float row can be two, three or four wide (since 2026-10-01)

`float2:lo:hi` and `float4:lo:hi` parameter rows are the `float3` row's group with two
or four sliders (X Y, X Y Z W): `Float3::set_components(n)` / `with_components(n)`,
four sliders stored with the first `n` laid out, drawn, hit and written
(`value_string` joins `n` components, `scaled_values` / `set_values_n` read and write
them). `ParametersBg` treats every `floatN` row alike (`is_vec_row`, `vec_row_n`) and
sizes it with `Float3::preferred_height_for(labeled, n)`; a row whose width changes on
a re-read takes the new width in place. Only a three-wide group has a trackball — a
direction is three numbers — so `float4:…:trackball` has none. The designer's
Attribute node presents its Value through these.
`float2_and_float4_rows_are_the_group_with_two_or_four_sliders` is the test.

### A parameter pane can draw separators (since 2026-10-01)

A row of type `separator` (`parameters_bg::SEPARATOR`) is a hairline between two runs
of rows: one pixel tall with the ordinary row gap either side, drawn in
`plain_quads` in the theme's `surface_border`, and never hovered, focused, edited or
given a backing. Its key and value mean nothing — a host writing rows back finds no
parameter by them and skips it. Unlike a `section` it has no title and collapses
nothing; it is what a host puts between groups of parameters that are about different
things (the designer derives them from its templates' `group` metadata).
`a_separator_row_is_a_rule_between_rows` is the test.

### A slider's range can be soft (since 2026-10-01)

`Slider::set_soft` / `with_soft`, `Float3::set_soft`, and in `ParametersBg` a `soft`
segment after the range (`slider:lo:hi:dec:soft`, `float3:lo:hi:trackball:soft`,
`is_soft_row`): a value TYPED into the readout past either end widens the range to hold
it, where a hard range clamps it to the end. A drag and the wheel still stop at the
ends. It is for a value with no natural bounds whose range is only a scale to drag
over — the host is expected to choose the range around the value and re-choose it (the
designer's Attribute Value row). The pane now writes a slider row back from the
slider's OWN value (`get_scaled_value`) rather than its fraction over the row's
declared range, which was the same thing until a range could widen.
`a_soft_range_widens_to_a_typed_value` and `a_soft_row_writes_back_what_its_slider_holds`
are the tests.

### The keyboard can walk a menu (since 2026-10-01)

`context_menu::set_hovered_item(Some(idx))` highlights a row as the pointer would, and
`context_menu::step_hovered(dir)` moves the highlight to the next row that can run
(down for `dir > 0`), skipping header rows and `-` separators, stopping at either end
rather than wrapping. Both scroll a shortened menu to the row, WITHOUT re-hovering the
row under the pointer the way `scroll_by` does: the keyboard put the highlight there.
The menu itself still reads no keys — a host that wants a walkable menu routes Up/Down
to `step_hovered` and runs `hovered_item()` on Enter.
`the_keyboard_steps_the_highlight_over_what_cannot_run` is the test.

### A dropdown says whether it is taking input (since 2026-10-01)

`Dropdown::is_expanded()` is `open && !closing`: `open` alone stays true through the
closing animation, while the plate is still drawn but presses and keys are no longer
the dropdown's. A host that routes input to a dropdown ahead of what is under it (the
designer's dialog hosts one) asks this. Such a host should also know that the runner
hands every left press to each registered popover whose hit test MISSES it, before the
app sees the press (`close_popovers_missed_by_press`) — and a popover covered by an
outer popover's claim always misses — so the dropdown may already have taken the press
by the time the app is asked.

### A dropdown's text names its font (since 2026-10-01)

The trigger's text and ▼ are emitted with `control_label_font_detached()` named on the
prim, where they used to leave the font to the host. Through the widget walk the
adapter attached `widget_font` and nothing changed; but a dropdown painted as a STAMP
by another widget (the designer's palette paints its choice rows so) took that
widget's font, and the menu `draw_popover` grows out of it — which has always named
the font — opened in a different one. The whole configured string is passed, family
and size together (`Berkeley Mono 14`): the runner parses both and the size wins over
the prim's, which is how the menu's hard-coded 12 px rows and 10 px ▼ come out at the
trigger's size. Passing the family alone breaks exactly that.

A host that draws an open dropdown should hand `render_popover` a real `PaintCtx`, as
cce-files does: a `PopoverCollector` keeps fills as plain rects, so the grown plate
loses its relief and its corners (`inset_plate` degrades to a flat fill there). The
designer's params pane went through one until the same day.

### A well with a flush run at its end is ONE field (since 2026-10-01)

`Prim::Field { rect, radii, depth, split, tint }` (`PaintCtx::field`) is a sunken well
that ends in a flush run: left of `split` the interior one step down, right of it back at
the surface's level, as a flush control plate's face. Two controls are drawn so: a
`ParametersBg` textpick row (the TextBox and its completion picker, a menu-button Dropdown
`PICK_W` wide) and a `Spinbox` (its value and its -/+ run). In both the run is the
control's right end, reaching its outer edge on the top, right and bottom as a dropdown
trigger's plate does.

It is one prim because two did not work: a recess for the well and a trough for the run,
side by side, each shade their OWN box, so at the seam each turns its own square corner
and the strong line of the edge jumps — a recess is lit at its outer rim, a trough at its
inner lip — and the outline reads broken exactly where the two meet. The field's outline
is evaluated once (the whole rect, its own radii, `MODE_FIELD` in `shader2d.wgsl`). Its
outer half is ONE profile all the way round — the recess's fall and shoulder — and only
the inner half differs: on to the floor in the well, mirrored back up to the face in the
run, blended across one wall width about the seam. Note the run's valley is NOT
`Prim::Trough`'s, which fits its whole fall and rise into the wall's width, so its outer
half is a compressed copy of a step that read differently from the well's beside it. The
run's FACE is a rounded rect of its own, inset half a wall on every side — from the
outline on the top, right and bottom, from the seam on the left — whose lip is the
well's fall mirrored back up, and whose left corners are its right corners, so the
button has the same padding and rounding at both ends (since 2026-10-02; until then
the lip was the outline's own inner half and the face met the seam square). Where the
face's rounded corner leaves room by the straight outline is the valley's flat floor.
The seam is the well's own right wall's inner half, from half the step at `split`
down to the floor, meeting the face's lip where both stand at half the step; it fades
out at the outline, which runs straight across. **The run is laid out a wall wider than
its face**, so the BUTTON is what reaches the well and what it carries stands in its
middle: a textpick picker is `PICK_W` plus a wall, its arrow centred
(`Dropdown::center_arrow`) — which is a trigger's ARROW SLOT (`dropdown::arrow_slot`),
the right end every dropdown centres its ▼ in, so the picker's arrow and every other
trigger's stand in one column (since the same day; they stood 18 px in from the right
end, two pixels off the picker's; `a_pickers_arrow_lines_up_with_a_dropdowns`) — and a spinbox's run begins a wall before its flat layout's
buttons (`SpinGeom::run_x`), its face halved at its middle by the -/+ seam with each
glyph in the middle of its half — hit zones, washes, glyphs and relief all read
`SpinGeom`. For a few hours on 2026-10-02 the seam was the right side mirrored instead
— the button's valley rising to a ridge at the surface's level, the well's wall falling
from it a whole wall further left — which read as a strip of new surface between the
well and the button rather than a wider button; the well's floor ends where it did
then, the button having taken the strip. (Before that, the face began half a wall from
the seam with the arrow at the trigger's usual right-hand place, and its left side read
narrower than its right.) A textpick row's TextBox ends where its picker begins, so its
text stops at the well. Never grouped into a host
plate (its profile is not a monotonic step); the overlay's host-box slot carries `split`.
The legacy banded path and flat hosts (`layout::bridge`) draw the two-box form.

**Every flush control wears the run's edge** (since 2026-10-02): `PaintCtx::inset_plate`
— the one flush control plate, which `ControlPlate`'s Flush stance, every widget and the
apps' own plates all come through — draws a face and a field that is all run (its seam
`FIELD_RUN_ONLY` px to its left), where it drew a `Prim::Trough`, whose outer half is a
compressed copy of a step and read differently from a well beside it. So a dropdown
trigger and its grown menu, a button, a breadcrumb's run, the font selector, a menubar's
triggers, and the calendar's, cce-cloud's, cce-files' and the system interface's own
plates have the edge of the run at the end of a text row's field. It went in a widget at
a time the day before (`ControlPlate::with_run_edge`, `PaintCtx::flush_run`), both gone
now that the plate itself has it. In a parameter pane the dropdown and button rows are
fields too (`fields`; the pane's `troughs()` list went, and its button rows had been a
raised boss the button never drew). The flat-host bridge pairs a plate's face with its
field as it did with its trough. `Prim::Trough` / `PaintCtx::trough` stay for an
explicit valley and `cce-relief`'s preview. `a_dropdown_trigger_wears_the_runs_edge`,
`focus_lights_the_plate_rim`, `a_breadcrumb_and_a_font_selector_wear_the_runs_edge` and
`an_inset_plate_is_a_field_that_is_all_run` are the tests.

**A plain text box is a `Prim::Recess`**, grouped into the plate under it when the
plate's grouping window is open. For a few hours on 2026-10-02 it was a field that was
all well (`PaintCtx::well_field`, 6634ba3), to dodge a doubled outline that grouped
recesses drew; the cause was fixed in the plate shader the same day (see "A grouped carve
shades as its overlay does" below) and the workaround reverted, since a grouped recess
and an overlaid one now draw the same pixels.

The pieces that feed it: `ParametersBg::fields` (the pane's list, drawn after its troughs,
hover-tinted like them; textpick rows and spinboxes with a run are in neither `reliefs`
nor `troughs`), `TextBox::joined_right` (the box stops at the seam), `Dropdown::set_radii`
(the picker's own raised paint, square at the seam), and `Spinbox::relief_parts`, which
returns a `SpinRelief` — the outline as handed over, its radius and depth, and the run's
`split` with the engraved -/+ seam; the widget's own paint carves it inside as every well
is. `textpick_rows_carry_a_picker` and `a_spinbox_is_one_field_with_its_run_at_the_right_end`
are the tests. Until the same day the picker and the run were nested INSIDE a full-width
well, their faces stopping at the base of its wall, so they never reached the edge a
dropdown's ▼ does.

### A field is one object, and a toggle is a field whose run glides (since 2026-10-02)

A dropdown, a button, a text row with its picker, a spinbox and a toggle are ONE
object: a well cut into a plate with a flush plate, the RUN, standing in it, one
outline round both (`Prim::Field`). They differ only in where the run is:

| form | `Field::` constructor | run | well |
| --- | --- | --- | --- |
| text box | `well` | none | the whole field |
| flush control plate (dropdown trigger, button, breadcrumb run, …) | `run` | the whole field | none |
| text row with its picker, spinbox with its -/+ run | `ending_in_run(split)` | the right end | left of it |
| toggle | `sliding_run(width, t)` | half the field: the left end off, the right end on | the other half; both sides mid-glide |
| check box | `well` (`Checkbox::box_field`), and checked a `run` plate in it (`Checkbox::box_plate`) | none, or a square plate in its middle | the whole field, showing all round the plate |

**`scene::paint::Field` is the object** (since the same day), and
`PaintCtx::field(&Field)` the one way to paint one: the outline (rect and radii,
the carve's boundary — a widget takes it through `carve_inside`), the wall width,
the run's span clamped to the outline (`run_span`, `None` for a well;
`Field::spanning(a, b)` is the general form the others are), and a rim `tint`
(`with_tint`). What builds a field asks for it by its FORM, never by numbers that
encode one — the old `field(rect, radii, depth, split, tint)` took a seam put
`FIELD_RUN_ONLY` px off the field to mean "no well", and every host spelled that
sentinel itself. Now only `Field::prim_span` does, turning a run that reaches an
end of the field (within the shader's half pixel) into the prim's encoding. A
field with NO run paints a `Prim::Recess`, as a plain well always has, because a
recess groups into the plate under it and a `Prim::Field` never does — so the
text box is a form of the object without changing what it draws. The widgets
hand theirs out: `TextBox::well`, `Toggle::field`, and `ParametersBg::fields` is
a `Vec<Field>` (the pane tints the hovered one); `PaintCtx::inset_plate` is a
face and `Field::run`.

`Prim::Field`'s run is a span, `split` to `end`. Where it stops short of the
right end, a well lies to its right too, and `MODE_FIELD`
mirrors everything it does on the left: the face is inset half a wall from that
seam with the corners of the run's LEFT end, and the seam is that well's left
wall's inner half. A run reaching the right end takes the same path through the
shader as before, term for term, so every field that was drawn is drawn as it was.
The host-box slot carries both ends (`p_host.x`, `.y`); the legacy banded path and
the flat-host bridge draw a well box either side of the run.

**The Toggle is drawn so** (`Toggle::field`, its sliding field, lit while focused;
`Toggle::face` what the relief-off path lights; `ParametersBg::fields` takes the
same, and its `reliefs` no longer carries toggles). Until this it was a recess
with a raised `Boss` on its floor: the one control whose nested plate stood ABOVE
the surface where every other stood flush with it, and whose outline turned its
own corner round the plate instead of running round the whole control. The run
is laid out half the field and a wall wider than its face, as a picker is, so the
face reaches the well; it glides by `slide_t` as the boss did. Focus lights the
field's rim (`ControlPlate::focus_tint`), where it lit the boss's.
`a_toggle_is_a_field_whose_run_glides` is the test.

**The Checkbox widget is a field too** (`Checkbox::field`): a square as tall as
the control (at most a toggle's height) at the left of its label, the largest
square in the rect when it has none — an empty well unchecked, and checked the
same well with a square plate centred in it (`Checkbox::box_plate`, `PLATE_SHARE`
of the side, all run), the well showing all round. Since 2026-10-05 (bae3712):
until then a checked box drew the toggle's run, half the box's width and its
whole height, which in a square box is a tall bar, and a ticked box read as
having narrowed. A box FILLED with a run was tried before that and dropped: at a
control's size an all-run field's outline is an empty well's, and the two states
were hard to tell apart. In a parameter pane a `checkbox` row has always been a
Toggle. `a_checkbox_is_a_well_with_a_square_plate_in_it_or_not` is the test.

**A check drawn inline is the same box** (`Checkbox::paint_inline(ctx, cx, cy,
half, checked)`): cce-list's rows, a markdown task item, the doc editor. Both it
and the widget build the box through `Checkbox::box_field(square)` and, checked,
`Checkbox::box_plate(&well)`, so they cannot disagree; the corner is the toggle's IN PROPORTION (its radius over
its height), which is the toggle's corner exactly at a toggle's height and keeps a
14px box (`Checkbox::INLINE_HALF`, cce-list's) a rounded square where the
toggle's radius taken whole would make it a disc. They drew a ring with a blue
dot until the same day (`paint_round_mark`, gone). Rendered at 10–14px it reads at
1x and 2x, the smallest least clearly; the state is the plate, with no colour.
`an_inline_check_is_the_widgets_box` is the test.

### A grouped carve shades as its overlay does (since 2026-10-02)

A full-ring, untinted `Prim::Recess` / `Boss` emitted while a plate's grouping window is
open becomes a CSG feature of that plate's one draw (`MODE_PLATE` in `shader2d.wgsl`);
otherwise it is its own overlay (`MODE_RECESS` / `MODE_BOSS`). The two must look the same
on the plate's face. They did not: the grouped one drew a **doubled outline**, two thin
black lines down its shadowed wall and two bright ones down its lit wall, where the
overlay drew one soft edge. Two causes, both in how the plate path applied the carves:

- **The shade line took the carves' slope.** The plate's colour subtracted
  `roll_shade_line(sv)` with `sv` the roll's slope PLUS every carve's. The shade line is
  a narrow lobe at the half-vector's tilt (22.5° at the DE's 45° light); a carve wall's
  tilt rises through that angle and falls back through it, so the lobe fired twice per
  wall — subtracted in colour units from a face near 0.016 linear, both hits went to
  black. The overlay path, and `relief_shade::carve_shade`, never had a shade line: it
  exists for the raised roll. It is now `roll_shade_line(sv_rim)`, the roll's alone.
- **Diffuse and curvature were a multiply on the face; the glint was added whole.** On a
  dark face a multiply barely moves the pixel, so the overlay's lit shoulder vanished,
  and the glint's two crossings (the same twice-through-the-angle as above) no longer
  cancelled against the fillet's darkening as they do inside the overlay's one signed
  value. The plate now lights its roll alone as before (multiply plus glint and shade
  line), and composites what the carves ADD — the summed normal's diffuse and glint less
  the roll's, plus their curvature — as an overlay is blended (`carve_over`: screen
  toward white, multiply toward black). On the face that is the overlay's `v` term for
  term; across the roll the normal is still the summed one, the junction grouping is
  for. A plate with no carves composites `v = 0` and is unchanged to the bit.

Measured with `examples/grouped_recess_probe.rs` (a plate whose recesses group beside the
same recesses forced to overlay by a transparent quad): grouped and overlay columns
5,462 px apart before, 0 after; in the designer with grouped text wells, 0 px from the
`well_field` rendering the text boxes then had (since reverted, above).

**And it is tested by rendering, not by reading** (`vk::plate_probe`, `cfg(test)`): an
offscreen 2D render — a `DisplayList` through the runner's own `tessellate_display_list`
and `dl_batches_2d`, drawn with the LIVE pipeline into an image and read back. The
renderer's pieces it needs are shared functions, not copies, so the two cannot draw
differently: `create_ui_pipeline` (descriptor layout, push range, vertex layout, blend),
`batch_push_constants` (a batch's 32-float block, feature rebase included),
`window_info_data` and `relief_px_at`. It draws plates, carves and flat geometry; it
refuses blur-behind (the snapshot is the swapchain path's) and draws no text or images.
`render` returns `None` with no Vulkan device and the test skips with a note; it is not
`#[ignore]`d, since a device is the normal case here and a regression nobody runs is
not caught. `a_grouped_carve_is_drawn_as_its_overlay_is` renders two plates, one grouped
and one forced to overlay, asserts the grouping really happened (five features), and
holds them equal to the pixel: against the pre-fix shader it fails at 32,504 px, worst
channel 54. Clean under `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`. Opening the
device adds about 2 s to the suite. A test of any other 2D look can use the same harness.
(Across devices the live path agrees to within 5/255 on about 125 edge pixels of the
probe — float rounding at antialiased edges, not a shading difference.)

**It was never the GPU.** The report was "doubled on the NVIDIA card, single on the
Iris Xe"; the 2D path is identical to the pixel on both, before the fix and after. Two
things made it look GPU-specific. Grouping is decided per frame by what is painted
between a plate and its carves, so two captures of "the same" pane can differ in what
grouped. And inside a `cce-shadow` session the NVIDIA ICD does not load at all unless
the process can reach an X display (`DISPLAY=:0` and `XAUTHORITY=$HOME/.Xauthority`):
`vk_icdGetInstanceProcAddr` fails, the loader skips the ICD, and `CCE_VK_DEVICE=discrete`
fell back to the Intel device SILENTLY. It says so on stderr now (below, "Debug
environment variables"); `grep -c nvidia /proc/<pid>/maps` is the check from outside.

### A plate can be turned inside out: `Prim::Frame` (since 2026-10-06)

`PaintCtx::frame(rect, hole, hole_radii, material, depth)` is a Bevel whose face is
everything in `rect` OUTSIDE `hole`, its rolled edge running round the hole and falling
INTO it. A corner of the hole is therefore an inside corner of the plate — a **cove**,
rounded at the hole's radius in the DE's corner family — which no box prim can draw: box
radii round only convex corners, and `ConcaveFillet` shades with the carve WALL profile,
not a plate's roll, so a fillet beside a Bevel edge changes profile at the join.

- **The shader is the plate branch, unchanged** (`MODE_FRAME` = 17): the hole's SDF is
  negated (`gd = -gd0`), so the depth into the face is the distance outside the box and
  the outward gradient points into the hole, and everything after is `MODE_PLATE`'s —
  roll, crest, frost, focus tint and CSG carves.
- **It is a carve host over `rect`**, like a Bevel: carves inside it group into its draw
  (the feature offset is rebased for mode 17 as for 1 and 14). Its own outer edges are
  NOT rolled — lay them past the window or under something.
- **The legacy banded path fills only below the hole**, square: it has no inside-out SDF.

The first consumer is the designer's playbar, a shelf of the window's bottom edge whose
top meets each side lip in a cove. `a_frame_is_an_inside_out_plate_that_hosts_its_carves`
is the test.

### A row can lead to a page, and a side swipe turns it (since 2026-10-02)

`context_menu::set_row_page(idx)`, called after `show` like `set_row_slider`, makes a
row a PAGE row: it wears `›` at its right end, and a press on it, or a two-finger swipe
to the side with the pointer on it, asks to TURN the menu into what the row leads to —
another list of rows, or another plate altogether (the designer's dialog). **The menu
recognizes the turn; the host shows the page**, since only the host knows what is
there:

- **A turn is a `PageTurn`**: `Into(row)` or `Back`. A press is read with
  `turn_at(x, y)` (the standard `mouse_input` path records it instead of hiding); a
  swipe arrives through `mouse_wheel` and is drained with `take_turn()`.
- **`show_page(x, y, back, options, header_count, target)`** shows a page with its
  top-left at the corner of the plate it replaces (`x()`, `y()` read before), so the
  menu reads as turning rather than a second menu arriving. With `back` naming the
  plate it came from, a BACK BAND across its top reads `‹ Title`: a press on it is
  `Back`, it hovers like a row, and the rows begin under it (`row_y`, `row_at` and the
  height all count it). A swipe back from anywhere on a page with a band is `Back`; on
  a menu that was opened rather than turned to, it goes nowhere. A page is `turned`:
  placed in a window, or by the popup's positioner, it SLIDES on screen and never flips
  up from the corner it took over.
- **`refill(options, sliders)`** changes the shown rows' labels and slider values in
  place (hover, scroll, band, page rows and a held slider kept) — how a host re-marks a
  switch on a page that stays up after it ran. A different row count is refused.
- **The swipe is `widget::side_swipe`**, one recognizer per window (`side_swipe::feed`)
  shared by the menu and any plate a host turns into, so one gesture turns one page
  however many plates pass under the fingers; the lift (`ScrollPhase::FingerEnd`) or a
  250 ms pause readies the next. It fires once the fingers have gone `SWIPE_PX` (40) to
  the side, half again more sideways than vertical; a tilt-wheel notch is a whole swipe.
  **Forward follows the content**: the delta that scrolls a list to show what is to its
  right, so under natural scrolling the fingers go LEFT to go in and right to go back,
  as on every touch surface, and the user's scrolling setting flips both. A test that
  swipes twice calls `side_swipe::end_gesture()` between, since a gesture's phase is the
  window's (a thread's own in a test).
- **The rest of a gesture that turned is the turn's**: `side_swipe::swallow(delta)`,
  asked at the top of a host's wheel handling, is true for it until the lift, and the
  host drops the event. Without it a page narrower than the plate it replaced left the
  fingers over the scene, and the end of a swipe back orbited the camera.
- **A page MOVES the popup that is up** (`menu_popup.rs`, `xdg_popup.reposition`,
  version 3), anchored at the corner, without `FlipY`; only where it cannot (no popup
  placed yet, an older protocol) is a new one opened. A NEW surface under a pointer that
  has not moved gets no pointer focus until it moves, so with a replaced popup the rest
  of a swipe and the swipe back went to nothing — found in a shadow session, where the
  first cut turned forward and then would not turn back.

- **A turn is animated** (`TURN_MS`, 180 ms, eased out): the plate grows or shrinks
  from the size of the one it replaced to its own at the shared corner (`drawn_rect`),
  the rows it had slide away `TURN_SLIDE` px and fade, and the page's slide in from the
  side the turn comes from and come up — forward from the right for a page with a back
  band, back from the left for one without. The old plate is a clone taken in
  `show_page`, from a menu that is up or was hidden in the same moment (a host that
  closes one menu and shows the next in one dispatch); `turn_from_size(w, h, forward)`
  is for a turn from a plate the menu does not draw (the designer's dialog). While it
  runs `natural_geometry` is the larger of the two sizes, so the popup is repositioned
  to hold both and again to the page's own when it lands, and the runner asks for
  frames (`is_turning`). **Both sets of rows fade, geometry and all**: each is
  drawn aside (`paint_rows`) and replayed moved through `Prim::faded`, which scales a
  colour's alpha, a text's or an image's, and a GROOVE's `strength` — the factor on
  its shading, specular and AO, since a carve has no colour to fade (a separator was
  the one relief prim a menu's rows draw; the slider rows are coloured quads). The
  relief prims with no colour or strength come back as they are. Squared fades
  (`turn_fades`), so the two are seldom both legible at once.
  `a_turn_fades_the_separators_of_both_plates` is the test. **A turned page that needs a NEW popup is handed over from the
  window** (`MenuPopup::handoff`): it stays drawn in the window until the popup's
  first configure, and the frame after commits the popup ahead of the window
  (`take_menu_popup_lead`). Hosted at once, as a menu opened at the pointer is, a
  swipe back from the designer's dialog left a frame with neither plate — the dialog
  gone, the popup not placed — and, drawn after the window, the popup arrived up to a
  window frame's draw late. `CCE_UI_TURN_MS` slows it down, to capture a turn frame by frame in a
  shadow session. `a_page_turn_grows_the_plate_from_the_one_it_replaced` is the test.

**Until this a row could open a SUBMENU** (2026-09-29 to 2026-10-02): a second
`ContextMenuState` flying out beside the row on hover, in a child popup, with a
hover-intent triangle. It went with the change, the `SUBMENU` thread-local, the
`submenu` module, `SubmenuSpec` and the child popup included: the designer, its only
consumer, had flyouts on some rows and plate swaps on others, two gestures for one
idea, and the user asked for one. `context_menu_page_tests` covers the menu,
`side_swipe::tests` the recognizer.

## Plates, wells and seams — the surface vocabulary

Everything cce draws is a lit surface, and the words below name those surfaces
so that a description of how a screen should look or behave can be given in
them. Use them in code comments, commit messages, and conversation; when a new
widget does not fit one of them, say so rather than stretching a word.

- **A plate is any lit, bounded surface with a silhouette and a stance.** The
  silhouette is its corner radius (the DE's superellipse corner family,
  `corner_shape`). The stance is how it sits on the surface beneath it:
  - **raised** — it floats above that surface, drawn as a `Bevel` (fill plus
    rolled edge) or a `Boss` (edges only, the surface below as its face):
    menus, popovers, raised buttons, a ButtonStrip's selected plateau, a
    Breadcrumb in its floating stance.
  - **flush** — it sits level with that surface inside a groove ring, drawn as
    an `inset_plate`: buttons, dropdown triggers, breadcrumb runs, font
    selectors. Its face is the surface below unless a fill is configured.
- **Plates nest, and the ladder has three rungs of the same object.** The
  **root plate** is a window's background (RFC 7a; `plate { root }` in
  config). **Pane plates** are the surfaces controls and content sit on inside
  a window; they carry the corner dock (`widget/plate_dock.rs`). **Control
  plates** are the things you press. A control plate is not a different kind
  of object from a root plate — it is a plate at a smaller scale.
- **Wells are not plates.** A well is an opening cut into a plate that you look
  into or type into, drawn as a `Recess` (a `Trough` when it holds a moving
  part): text boxes, keybind and spinbox fields, slider and progress tracks,
  the trackpad pane, the ColorSelector's recess. Things you press are plates;
  things you enter are wells. A well's floor can carry fills (a progress
  fill, a colour swatch) — those are segments of the floor, not plates.
  A **canvas well** is a well you look into or draw in — Trackpad, Slider2D,
  the bevel and ramp previews — and every one is cut from the same material:
  the plate darkened for its floor (`colors::WELL_FLOOR`) and the recess for
  its rim, through `PaintCtx::well_floor` / `well_rim` (`canvas_well`),
  rounded like the text wells. The Ramp editor's plot is the reference look.
  With relief off, a well is its frame: the one hairline
  `colors::well_frame_color` gives every well (lit in the highlight while it
  is active, the relief rim's focus cue), and still no floor of its own.
- **A group is a lasso.** `widget::Group` owns nothing: it is a set of member
  ids, and its frame is the padded hull of wherever the host's layout put
  them, with a title tab flush on the top edge — the section's frame
  (`PaintCtx::section_well` under relief, the section outline otherwise), so
  a group is a segment of the plate it sits on, parted by a section carve
  rather than a seam. Given its plate (`with_plate`) and `with_fit`, sides
  within `snap` of the plate's edge take the edge one padding in and corners
  on the plate's corner follow it concentrically: on a narrow pane a group is
  that pane's inset lining, on a wide one a lasso. A group never hits.
- **A dialog is a raised plate around a lasso** (`widget::Dialog`, since
  2026-10-08). Like a group it owns nothing: the host lays its members out,
  and its plate is their padded hull with a title band above, in the menu's
  material. `open(ctx, members)` makes it MODAL through the context
  (`UiContext::open_modal`): the Tab walk is trapped among the members, every
  widget outside reads as covered (`is_coordinate_covered`, which every hit
  test and hover asks) so nothing behind takes a press, focus moves in and is
  given back on `close`, and the members are linked as its children, so the
  accessibility tree nests them under a modal `Dialog` node. Paint the dialog,
  not its members (it paints them on its plate), after everything it covers;
  `set_backdrop` dims the window. Escape is the host's to read. Hide the
  members while it is closed, or they are stops nobody can see. The demo's
  Options… dialog is the pattern.
- **Segments are plates or floors sharing one silhouette, parted by seams.**
  A seam is a `Groove` cut across the shared surface, dying into its rolled
  edge: Breadcrumb segments, ButtonStrip segments, the ColorSelector's
  text/swatch split. One silhouette, one relief pass, seams between. A
  `Separator` is the same cut made in the plate it sits on, with no segment
  to part: a groove that dies out at its own ends (flat: a hairline).
- **Bands sit outside this vocabulary on purpose.** The Slider's swelling
  band is a band. (The round Checkbox mark was the other exception, a mark;
  since 2026-10-02 a check box is a field wherever it is drawn.) Do not call
  them plates or wells.

What this buys, and where the code is heading:

- **Navigation is stated in plate terms.** `Input::focus_role` says what a
  widget is to the keyboard: a `Plate` (a thing you press — Enter / Space act
  on it while focused), a `Well` (opens for typing when focused), or `None`
  (not a stop). `UiContext::focus_step` walks the stops in reading order (row,
  then x) with a `Group`'s members as one contiguous run where the group's
  first member falls (`focus_clusters`), wrapping; `focus_step_group` jumps
  between runs (input.kdl `focus_next_group` / `focus_prev_group`, defaults
  `ctrl+tab` / `ctrl+shift+tab`). The runner calls them for Tab / Shift+Tab
  and the chords unless the app opts OUT with `Application::plate_navigation`
  (default ON since 2026-10-08, accessibility RFC phase 3; the designer, the
  display manager and cce-notes give Tab meanings of their own and return
  false), and tells the app through `Application::focus_stepped` — an app that caches
  its geometry until its own rebuild flag raises it there. A focused widget
  that types Tab keeps it (`Input::keeps_tab`: a multi-line `TextBox` while
  editing), and the group chord leaves it. The walk needs the
  app's context exposed (`ui_context_mut`), so an app without one (a terminal,
  a web view) gets Tab as before; a widget registered but never drawn is a
  stop nobody can see, so register a widget only while it is shown. The ring reaches flat-path hosts
  through `RenderTarget::inset_plate_tinted` and `CarveKind::Boss { tint }`.
  `CCE_FOCUS_DEBUG=1` prints the stops in walk order.
  The focus ring is the plate's own silhouette: `ControlPlate::with_tint`
  tints the rim (a tinted `Trough`, `Boss` or `Bevel`), the same treatment a
  well's `recess_tinted` gives its rim while editing — never extra geometry.
  The tint recolours the relief rather than replacing it: the rim's light
  composites in the accent instead of white and its shadow in a dark accent
  instead of black (`FOCUS_SHADOW_LUM`, both at `FOCUS_GAIN`, in
  `shader2d.wgsl`), so a focused plate still reads which edges face the lamp.
  A Checkbox and a Toggle light the rim of their field, as every field is
  lit.
  Roles today: Button, Checkbox, Toggle, Dropdown, FontSelector, ButtonStrip
  (arrows move the selection between its segment plates), RadioGroup (one
  stop; arrows move the choice, which follows them) and Breadcrumb
  (arrows walk its visible segments, Enter navigates) are plates; TextBox,
  Spinbox, ColorSelector, KeybindRecorder, TreeList, Slider (a band, but
  entered and adjusted in place — arrows step it, Enter opens the readout)
  and RangeSlider (one stop, two ends: arrows step the focused end, Up / Down
  switch ends) are wells. A new focusable widget declares its role and handles `FocusIn`
  / `FocusOut`.
- **What a plate is made of is a `scene::Material`** — tint, `Frost` (opaque, or
  frosted with compression / refraction / radius) and `Finish` (how it answers the
  light: the old `relief_shade::Material`). `docs/rfc-material.md` is the design and
  its phase tracker. `Material::fill_tint` is the ONE place the blur-behind sentinel
  (a negative alpha) is written; `PlateSpec::fill` and `param_plate_fill` call it.
  Rung defaults: `Material::root()` / `pane()` / `control()`, `popover(base)` for
  menus; a well floor is `host.floor(lifted)`. `PlateSpec`, `ControlPlate.face`
  (`Option<Material>`: `None` = the surface below IS the face) and the prims
  `Plate` / `Bevel` / `Sphere` / `Droplet` carry one, and `PaintCtx::plate` /
  `bevel` / `sphere` / `droplet` / `inset_plate` take one; the tessellator reads
  each prim's fill and push-constant finish from it, and only the carves still take
  the DE finish. `Material::from_fill` / `face` decode a colour a legacy site still
  holds (the flat-path `RenderTarget` is colour-typed) — a new site says
  `Material::opaque` / `with_frost` instead. **`tests/plate_golden.rs` is the exit
  test for any change that must not move a pixel**: dump before, compare after.
- **One plate spec per rung, not five copies.** The root and pane rungs are
  `scene::paint::PlateSpec` (RFC 7b, painted by `PaintCtx::plate`). The
  control rung is `scene::paint::ControlPlate` (re-exported from `widget`):
  footprint, per-corner silhouette, `PlateStance` (raised, flush or flat), face
  and depth, painted by `PaintCtx::control_plate` — the ONE place a control
  face's relief is composed (raised with a face = bevel; raised faceless =
  carve inside + boss; flush = carve inside + inset plate). Button, Dropdown,
  FontSelector, Breadcrumb and the ButtonStrip's selected plateau draw through
  it; the migration was prim-identical against a dump of every face. A new
  control face goes through `ControlPlate`, never a hand-rolled carve.
- **`Flat` is the stance for a control made of its pane's material.** The two
  relief stances both carve INSIDE the footprint, which costs a control two
  things a pane has: its visible edge sits half the carve depth in, so a
  control laid out on the same numbers as a pane does not line up with one;
  and its face is laid through a stroke, which the blur-behind sentinel (a
  negative alpha) does not reach, so it cannot be frosted. `Flat` fills the
  footprint with a quad and nothing else — silhouette equal to the rect,
  frost carried, focus `tint` drawn as a ring since there is no rim to light.
  It carries ONE radius, not four, so the concentric corner adjustment a
  nested relief control computes has no equivalent. Reach for it when a bar
  or a toolbar should read as plates at a smaller scale rather than as
  controls of a different kind (`Button::with_flat`, `Dropdown::with_flat`);
  leave the relief stances alone for things that should feel pressable.
- **Radii are configured per rung, overridden per widget.** Root:
  `style.surface.plate.root.corner_radius` (`color::root_plate_corner_radius`).
  Pane: `plate_corner_radius`, falling back to the root's. Control:
  `style.control.corner_radius` (`layout::control_corner_radius`, default 8) —
  every control-scale getter (button, dropdown, font selector, slider,
  spinbox, textbox, toggle, list and tree wells; the ColorSelector's two via
  the textbox) falls back to it when the widget's own `corner_radius` key is
  unset, so a per-widget key is an override, not a requirement. Do not give a
  new control-scale radius getter a literal default; fall back to the rung.
- **A config hex is gamma-decoded; a built-in default colour is not.** The
  style loader's `parse_hex` runs every channel through `srgb_to_linear`
  (alpha excepted), so `"#595969"` arrives as `[0.10, 0.10, 0.14]` — which is
  exactly `PARAM_BG`'s default. The constants in `color` are already
  linear, so **the hex that pins a default is not that default's floats times
  255.** `PARAM_BG = [0.10, 0.10, 0.14]` reads as `#1a1a24` if you scale it
  naively, and `#1a1a24` decodes to `[0.010, 0.010, 0.018]` — a plate ten
  times darker than the one you were trying to preserve, silently, because
  both spellings are valid config.

  Round-trip a default with `l2s(c) = 1.055·c^(1/2.4) − 0.055` (the inverse of
  `srgb_to_linear`) before writing it into a config, or read the value back
  out of the running app. This cost a measurement round on 2026-09-19: a
  `backdrop_compression` sweep meant to hold the tint constant was silently
  sweeping the tint too, and the two halves of the experiment disagreed by 3x
  on the plate's luminance.

  Two smaller edges of the same knife: an **8-digit** hex keeps its alpha raw
  (`a/255`, no decode), so `#05050840` really is a quarter opacity; a
  **6-digit** hex sets alpha to **1.0**, so dropping the last byte off a
  translucent plate colour makes it fully opaque rather than leaving it
  alone.
- **Frost is one block, and "unfrosted" is not "solid"** (2026-09-28). The
  default plate material's recipe is spelled `style.surface.plate { frost
  radius=5.5 compression=0 refraction=0 }` — the same `frost` child a named
  material has — with a bare `frost` frosted at the defaults and `frost
  (bool)false` off. The four keys it replaced (`blur` as the switch,
  `radius`, `backdrop_compression`, `refraction`) were aliases for the rest
  of that day and are RETIRED: the loader reports one it finds
  (`color::retired_surface_keys`, a warning naming the path) and does not read
  it — so a config with `blur=true` and no block is sharp, and the warning
  is what says why. A material's `frost` child spells its compression
  `compression` too; its `backdrop_compression` alias went with them.
  cce-relief writes the block (only for a frosted plate, since the block is
  what frosts one), seeds from a retired key it still finds, and removes
  the retired keys on Save, so a file migrates the first time it is saved.
  The enum variant is `Frost::Unfrosted`
  (was `Opaque`): it means the plate never samples its backdrop, and a
  translucent tint stays translucent with a SHARP view through it — which
  is what the old name kept reading as "covers everything". The pane tint
  has a clear spelling too, `style.surface.plate.pane.color`, whose alpha
  is the whole tint strength; the legacy `style.surface.param.color` is
  still multiplied by the top-level `plate_opacity` line, as it always was
  (`color::pane_color_is_whole`).
- **A frosted plate's legibility is `backdrop_compression`, not opacity.**
  Blur destroys a backdrop's spatial DETAIL and preserves its mean LUMINANCE,
  and text contrast is a mean-luminance property — so `resolve_blur`'s closing
  `mix(backdrop, plate, opacity)` hands the backdrop's brightness through at
  `1 - opacity` whatever the kernel does. At the designer dialog's 0.25 that is
  75% of whatever is behind it. Measured on a row label (`#ccccd4`) over the
  designer's Alt+D plate at the stock tint: **1.16:1 over a white viewport,
  6.65:1 over the dark one** — the bright end not a contrast ratio so much as
  its absence. More blur moves neither number, which is the whole of the
  "liquid glass" legibility problem, and why refraction and specular cannot
  help: they are shape cues, and legibility is a luminance budget.

  `style.surface.plate { frost compression=… }` (0..1, `color::plate_backdrop_
  compression`, **default 0** — every existing config keeps today's look)
  remaps the blurred backdrop's luminance toward the plate's own key before
  the tint, holding its chromaticity. It is not opacity and not "darken": it
  is SYMMETRIC, pulling a bright backdrop down and a dark one UP, so both ends
  converge on the plate's key. The contrast stops depending on what is behind
  the window, which is the actual goal; hue, chroma and movement still read
  through it.

  **It only works with a tint dark enough to converge ON.** A 24-cell sweep
  (k x tint x backdrop, 2026-09-20) — contrast on the bright/dark viewports,
  with `show` the luminance sigma across bare plate (x100), a proxy for how
  much backdrop still reads through:

  | tint | k=0 | k=0.4 | k=0.6 | k=0.85 |
  |---|---|---|---|---|
  | `#595969` (stock) | 1.16 / 6.65 | 2.25 / 4.87 | 3.10 / 4.54 | 4.10 / 4.34 |
  | `#1a1a24` | 1.23 / 9.97 | 2.99 / 10.34 | **5.08 / 10.53** | 9.36 / 10.70 |
  | `#050508` | 1.24 / 10.47 | 3.08 / 11.67 | **5.41 / 12.14** | 10.76 / 12.60 |
  | *show* (bright/dark) | 7.3 / 0.9 | 3.7 / 0.5 | 2.3 / 0.2 | 0.4 / 0.1 |

  Three readings. **The stock tint cannot be rescued at any k** — it never
  clears 4.5:1 on the bright backdrop, and on the DARK one it gets WORSE as k
  rises (6.65 -> 4.34), because `#595969` is lighter than the scene and
  compression lifts the plate toward it. **Tint does nothing without k**: at
  k=0 the three tints read 1.16/1.23/1.24, indistinguishable, because at 0.25
  opacity the tint barely participates — which is why "just darken it" was a
  dead end before this existed. And **k ~ 0.6 is the knee**: both dark tints
  clear the floor on both backdrops with a third of the backdrop variation
  intact, where 0.85 doubles contrast for 95% of the remaining glass.

  So the pair is orthogonal, and that is the point: **k buys independence from
  the backdrop, the tint picks the key it becomes independent at.** cce-designer
  ships `#05050840` at k=0.6 (5.41:1 / 12.14:1). Note `show` is 0.2-0.9 on a
  dark backdrop at EVERY k: there is little luminance variation behind the
  plate there to begin with, so "glass" on a dark desktop is carried by the rim
  and bevel, not by the backdrop.
- **The recipe is per plate.** Since 2026-09-20 (RFC material step 3) compression,
  refraction and the blur radius are a plate's own `Material.frost`
  (`Frost::Frosted { compression, refraction, radius }`), packed into `p_host.zw` of
  its push block by `Frost::pack` — the two style keys above are the DEFAULT
  material's values (`Frost::from_style`), not a window setting, and two plates in
  one window can differ. `radius` is the kernel sigma in logical px;
  `Frost::DEFAULT_RADIUS` (5.5) reproduces the old fixed 5.5-physical-px stride on
  the scale-2 panel; 0 is a clear plate. A frosted FLAT fill of any kind (Quad,
  RoundedRect, Border fill — a `Flat` face, a menu, a popover) is promoted by the
  tessellator to a zero-depth plate batch so it carries its recipe too; only a raw
  vertex from outside the display list falls to the no-recipe branch.
  `examples/frost_pair.rs` is the visual test: three recipes in one window, run in a
  shadow, measured in the RFC's step-3 note.
- **The kernel reads a mip chain, or thin detail bands** (since 2026-10-06). The 7x7
  taps stand a whole stride apart (5.5 physical px by default), and at level 0 a tap
  reads only the texel or two it lands between: a hairline, a well's edge or a glyph
  behind the plate was picked up whole by the taps that hit it and missed by the rest,
  seven faint copies a stride apart — horizontal bands under an open dropdown over the
  params rows, measured as an 8–16-level sawtooth down a column that is now a smooth
  ramp. The blur snapshot carries `snapshot_levels` mips (at most
  `SNAPSHOT_LEVELS_MAX`, 7), rebuilt by linear blits after every frame-so-far copy
  (`snapshot_mip_chain`), the sampler filters between levels, and `resolve_blur`
  reads level `log2(stride)`, so each tap is the average of its stride-sized cell.
  That adds about 3% to the blur's sigma. The clean samples (a clear plate, the rim)
  stay at level 0, and the scene backdrop keeps one level — only the zeroed-backdrop
  exempt plate reads it, and it holds nothing. A surface format that cannot be
  blitted with a linear filter gets one level and the old look.
- **Named materials in config** (RFC step 4): `style.surface.material { <name> { color;
  frost …; finish … } }` and a binding per rung — `plate material="…"`, `plate { root
  material="…" }`, `style.control.material` — resolved by `MaterialDef::resolve` over
  the rung's legacy material (unset fields fall back; no `frost` child = opaque; a
  binding wins over the legacy keys; an undefined name warns and degrades to legacy).
  The DE finish's three fixed terms are `style.surface.relief.spec / shininess /
  curvature`; the default frost's blur sigma is `plate { frost radius=… }`. cce-relief
  edits them (Finish and Frost columns) and writes into the bound material's node or the
  DE keys — never restructuring an unbound config. KDL trap when writing fixtures: two
  nodes on one line need a `;`, and `a { b }` on one line is a parse error the loader
  swallows into an empty document.
- **`plate { frost refraction=… }` (0..1, default 0) is the rim, and it buys
  no legibility.** It is the answer to the other half of the question — not
  "can I read this" but "is this an object". The roll is a real surface with a
  real tilt, and `sv_rim` IS that tilt (the unnormalized normal's horizontal
  part, already computed for the specular), so displacing the backdrop sample
  along it is what a curved edge does to what you see through it. Scaled by the
  roll width, so a 12px bevel bends more than a 2px one.

  **It samples the CLEAN backdrop, not the blurred one**, cross-fading to the
  frosted body on `f*f`. Refraction has to bend something with STRUCTURE or it
  is invisible: displacing a field already blurred to sigma ~11px just moves
  smooth values around. A thin edge scattering over a shorter path than a thick
  middle is also what a real slab does — the droplet branch trades on the same
  thing ("thin edges are clearer water"). One extra tap, not three: per-channel
  dispersion inside a band this narrow is invisible once the body is 49 taps,
  and paying for it would triple the most expensive path in this shader to be
  erased.

  **The clear rim is exempt from `backdrop_compression`**, in proportion to how
  clear it is. Compression is a legibility control and the rim carries no text;
  tone-mapping it pulls the refracted view back toward the plate's own key,
  which is the exact contrast the rim exists to show. Measured, the two
  fighting made the effect nearly invisible — exempting the rim made it **5.9x
  stronger** at the same setting (rim pixel change 1.50 -> 8.86 of 255 at 0.3),
  with the body still under 0.4. Useful range is ~0.3-0.6; the effect is in the
  roll and stays there.

### The surface config shape (`style.surface`, as of 2026-09-28)

The block every plate, wall and roll in the toolkit reads, in the spelling
the loader treats as current — written down once because the rules below
were settled one at a time across a day and each paragraph names only its
own key. What a key is, in a line each:

```kdl
style {
    surface {
        plate material="glass" {                 // optional: bind the pane rung to a material node
            pane color=(rgba)"#6c6c7bf2"         // the pane tint, alpha = strength (legacy: param.color × top-level plate_opacity)
            frost radius=(f64)5.5 compression=(f64)0.0 refraction=(f64)0.0   // the ONE frost spelling; absent = unfrosted
            border_color (rgba)"#9595a9ff"        // the flat border — control_relief OFF only
            border_thickness (f64)1.0
            padding (i64)20                       // the pane rung's inset
            root {                                // the root rung
                color (rgba)"#5e657acf"
                blur (f64)0.1                     // the COMPOSITOR's blur-behind, not the client frost
                corner_radius (i64)24             // the pane radius falls back to this
            }
        }
        material {                                // named materials (RFC material, step 4)
            glass {
                color (rgba)"#05050840"
                frost compression=(f64)0.6 refraction=(f64)0.3 radius=(f64)5.5
                finish light=(f64)0.15 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2
            }
        }
        relief light=(f64)0.15 width=(f64)9.3 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2 shader=(bool)true {
            // light: the strength, NOT a length; width: the ONE run of every roll and wall;
            // spec / shininess / curvature: the DE finish beyond its strength;
            // shader: false = the legacy banded edge shading, for A/B comparison
            wall height=(mm)0.3 profile="smooth;…"   // a carve's side (buttons, wells, rows): height = drop, profile = ramp spec
            edge height=4.0    profile="smooth;…"   // a plate's perimeter roll: height = rise (unset = quarter-round of width)
        }
        menu color=(rgba)"#101018ff" opacity=(f64)0.06 compression=(f64)0.8 corner_radius=(i64)24   // every popover and the designer's dialog
    }
}
```

Spellings that are NOT current, and what the loader does with each:

| Spelling | Status |
|---|---|
| `param.color` (+ top-level `plate_opacity`) | alias of `plate.pane.color`, multiplied by the opacity line |
| `plate.blur` / `.radius` / `.backdrop_compression` / `.refraction`, `frost.backdrop_compression`, `plate.bevel_width`, `relief.depth`, a material's `finish depth=`, `relief.height` / `.profile` / `.edge_height` / `.edge_profile`, `window_manager.bevel_depth` / `.bevel_width` / `.bevel_shader` | RETIRED: reported by path (`color::retired_surface_keys`), not read; cce-relief seeds from each once and its Save writes the current spelling and removes the old (the shader toggle it carries across as `relief.shader`) |
| `relief.wall.knobs` / `edge.knobs`, `profile_knobs` / `edge_knobs` | not style: cce-relief's own state (`~/.config/cce/cce-relief/state.kdl`); read once as a seed, removed on its next Save |

Where each rule is argued, by its lead-in: **Frost is one block** and
**Named materials in config** above; **The relief is two shapes**, **The
editor's knobs are not a style key** and **There is one roll width** under
"Units" (the geometry is unit-aware, which is why they sit there); the
`menu` block under "The context menu draws in its own popup surface".
There is no alias precedence to decide any more — every legacy spelling of
the relief is retired, so a file is what it says — and
`color::retired_surface_keys` is the one place a retired key is named;
cce-relief's Save is the migration for all of it: it seeds from a retired
key once, writes the current spelling and removes every superseded one it
finds.

## The standard app — root plate, rungs, and the spacing ladder

Every cce app is built the same way, and this section is the standard.
`scripts/style-audit` checks the sibling app crates against it (one row per
app; `--strict` fails on any off-standard row); `src/main.rs`, the demo, is
the reference implementation.

**Anatomy.** A window is a **root plate** with things standing on it. The root
plate is the first prim of every frame — `pc.root_plate(w, h)`, which emits
`PlateSpec::window(w, h)`: the root rung's material (`Material::root()`, the
DE's `style.surface.plate.root.color` at its opacity unless a `material=` is
bound), all four corners on the shared silhouette, the perimeter rolled over
`bevel_width`. Nothing else paints a window base — not a quad, not a rounded
rect at the silhouette radius, not a stroked border. On the root plate stand
**pane plates** (`PlateSpec` with the corners that touch the window edge
flagged, or `plate` / `rounded_rect` at `plate_corner_radius`) and **carves**
(`inset_plate`, `recess_edges`: a menubar or status band stepping down into
the surface, a well you type into). Which to use is the vocabulary above:
things you press and content you read sit on plates; things you enter and
bands that are part of the window's own surface are carved. On the pane
plates sit **control plates**. A window that is deliberately not a plate — a
transparent bar whose modules are the plates, a notification stack, a black
lock or screensaver surface, the desktop grid overlay — says so with a
`// style-audit: opt-out <reason>` comment and is listed as an opt-out.

**Spacing is a ladder, and an app never names a number.** Three rungs, each a
config key read through the style registry (so a nested KDL key works and
live-reloads), each with a getter in `layout`:

| rung | inset from the rim | gap between siblings |
|---|---|---|
| root plate | `root_plate_inset()` = `bevel_width` + `style.surface.plate.root.padding` | `root_plate_gap()` (`…root.gap`) |
| pane plate | `plate_padding()` (`style.surface.plate.padding`) | `plate_gap()` (`…plate.gap`, unset = the root gap) |
| controls | — (inside a pane or root inset) | `control_gap()` (`style.control.gap`, unset = `CONTROL_GAP`) |

The root inset carries the roll because the padding is a run of FLAT face —
the same run the gap leaves between two panes — and the face only begins
where the roll ends; a bare padding at a window edge measured 4px of visible
flat against 12 between panes. The legacy keys (`page_margin`, `column_gap`,
`control_panel_{padding,gap}`) are honoured when set and land on their rung
when unset, the radius rule applied to spacing; do not add a new one.

In the box model the ladder is presets — `Style::root_column()` /
`root_row()`, `pane_column()` / `pane_row()`, `controls_column()` /
`controls_row()` — and the container layouts' `Default`s read the same
getters. An app picks the
rung; a literal padding or gap in an app (`const PAD`, `+ 12.0`) is a number
the ladder should be supplying, and the audit counts them.

**Rules, restated as the audit checks them:**
- The first prim of the frame is `root_plate(w, h)`, or the crate declares an
  opt-out.
- Every inset and gap comes from a rung getter or a preset; the app declares
  no spacing constants of its own.
- A deliberate deviation — the system settings' own tint, an overlay's
  shallower roll — goes through `PlateSpec::window(..).with_material(..)` /
  `.with_depth(..)` and a comment saying it is one, never a hand-built spec.
- Migrating an app is pixel-neutral for the plate (`tests/plate_golden.rs`)
  and a measured change for spacing: screenshot in a shadow session, count
  the columns of flat face at the edges and across a split, and they match.

## Accessibility and locale are on the roadmap (read `docs/rfc-accessibility-locale.md`)

Accessibility reaches Linux screen readers only, and only for an app that opts in: the
widget tree is published over AT-SPI through AccessKit (`backend::a11y_unix`, the `a11y`
feature, `Application::publishes_accessibility` or `CCE_A11Y=1`; phase 2, proven on
cce-data-editor). There is nothing yet on macOS or in the browser. The toolkit's own words
are translatable (`crate::l10n`, phase 5): `tr("id")` looks a message up in
`locale/en-US/cce-ui.ftl`'s English or a translation `<tag>/cce-ui.ftl` (`cce_core::l10n`
for where), and a new string the toolkit shows goes there, never into the source;
`every_message_the_toolkit_names_is_in_its_english` holds the two to each other. And
right-to-left text is edited where it is drawn (carets, clicks and selections follow
it, a right-to-left paragraph is set against the right, the `DocEditor` draws styled runs
in bidi order, a multiline `TextBox` wraps by shaped width — phase 4). The RFC has
the measured state and a phased plan. Until it lands, two rules keep the retrofit cheap:

- **A new widget declares what it is**: its `focus_role`, and a label that names it to a
  person (not to a host).
- **An action is keyed by an ID, never by its label text.** Give a context-menu row its
  action with `context_menu::set_row_actions` (or build the menu with
  `UiContext::show_context_menu_rows`); a press runs the row's action, and matching the
  English label (`context_menu::legacy_action_for_label`) is only the fallback for menus that
  set none. Do not add code that matches on displayed text.

Phase 0 is done (2026-10-08): every font system is built with `locale::locale()` (from
`cce-core`): `LC_ALL`, else `LC_CTYPE`, else `LANG`, as a BCP 47 tag; in the browser,
`navigator.language` through `locale::set_locale` before the first font system.
`every_font_system_is_built_with_the_users_locale` is the test.

## The `scene/` core rebuild (read `docs/rfc-core-rebuild.md` before touching it)

`src/scene/` is a **retained scene graph being grown additively** to replace three overlaid legacy
subsystems (tripled tree ownership via raw widget pointers; three uncoordinated render paths;
layout smeared across five mechanisms). The RFC (`docs/rfc-core-rebuild.md`) is the authoritative
design and phase tracker — its inline "DONE" notes are the source of truth for what has landed.
Modules:

- `arena.rs` — `Arena` / `NodeId` / `Node`: a generational forest, the single source of truth for
  tree ownership. Generational keys turn use-after-free into a `None` lookup, not UB.
- `tree.rs` — `WidgetTree`: arena-backed replacement for `UiContext`'s old `widget_registry` +
  `layout_tree` twin stores, keyed `WidgetId → NodeId` so the public `WidgetId` API is preserved.
- `layout.rs` — the hand-rolled measure→arrange solver (`Style`/`Size`/`Rect`/`LayoutBox`).
  Deliberately **not** taffy: a compact row/column + flex + align + gap/padding box model.
- `paint.rs` / `painter.rs` — `DisplayList` + `PaintCtx` (clip/transform stack) and the single
  paint walk. Each widget emits its own geometry via `WidgetHost::paint_self`; the walk owns
  recursion and clipping (`WidgetHost::clips_children`), instead of every container re-deriving
  intersections. `renders_own_subtree` is an escape hatch for legacy subtree painters.
- `anim.rs` — `Animated<T>` (tween + spring + easing), the Phase 4 animation primitive replacing
  ad-hoc bool flips.
- `heightfield.rs` — the relief as a height field: the geometry the plate shader shades
  (plate rolls, CSG features, free carves) integrated back from the slopes it lights,
  sampled per physical px and exported as a 16-bit PNG + JSON sidecar in millimetres
  through the display metric. `CCE_HEIGHTMAP=<file>` in any client's environment, or
  `heightfield::request` from an app. See the Units section.

### `WidgetHost` (formerly the `Element` god-trait)

`WidgetHost` (`src/widget/mod.rs`) is the single 31-method host surface the machinery
(context routing, paint walk, render loop, app dyn broadcasts) sees, produced by the RFC's 6bd
shrink-then-rename of the old ~125-method `Element` god-trait. Its ONE production implementor
is `Adapted<W>`; concrete widget behavior lives on the narrow `Layout`/`Paint`/`Input` traits
(`src/widget/model.rs`). `base()` is guaranteed (`&Widget`, no Option). The direct-dispatch
block (mouse/key/drag) and the value/polling block (`take_click`/`take_change`/value strings)
are GONE from the trait — events route through `handle_event`, and apps drain widget state
through the concrete inherent `Adapted<W>` methods. See the RFC's blueprint notes before
adding anything to this trait.

**What a host's widget model answers is not a trait slot** (since 2026-10-08, 57 → 31).
The host hands out its widget as its narrow traits — `layout_model()`, `paint_model()`,
`input_model()` / `input_model_mut()` (`Adapted` returns its inner widget; a test shim that
implements `WidgetHost` directly gets `NoModel`'s defaults, or returns itself after
implementing the narrow trait it needs) — and `WidgetHostExt`, blanket-implemented for every
host, `dyn` included, carries what used to be one-line forwards: `focus_role`, `keeps_tab`,
`blocks_root_plate_drag`, `wants_tick`, `is_scrollable`, the `a11y_*` reads and acts,
`set_modifiers`, `context_action`, `color`, `solid_border`, `widget_font`,
`clips_children`, `renders_own_subtree`, `z_index`, `preferred_height`, plus the pure
derivations `label`, `corner_radii`, `mark_dirty`, and `content_rect` / `painted_prims` (what
the widget's model paints, as prims). Call them with `cce_ui::widget::WidgetHostExt` in
scope.

**The legacy tuple views are gone** (2026-10-08): `extra_quads`, `extra_arcs`,
`extra_circles`, `all_quads`, `all_rounded_quads`, `highlight_quad` and the host-side
`corner_style` — what a widget paints projected onto the pre-display-list surface — with
the model hooks that served only them (`serves_legacy_plain_quads` / `legacy_plain_quads`,
`aggregates_child_extra_quads`, `forwarded_highlight`), the painter's `paint_legacy_leaf`
and the tessellator's `widget_vertices`. Every host paints a widget through `paint_self` or
the paint walk (the designer, the gallery, the greeter, the settings app and cce-secrets
moved the same day, each checked by pixel A/B). A composite that draws a child's chrome in
its own order — the params pane's rows, the ramp's key editor, the menubar's strip — reads
the child's `painted_prims` (`widget::shown_prims` / `shown_quads` / `shown_rounded_quads`,
crate-private). `Paint::corner_style` stays: it is what a widget says about its silhouette,
read by `corner_radii`, `append_widget_plate` and the bridge. `append_widget_plate` is the
plate alone now; it drew the widget's arcs too, which a host painting the widget after it
drew twice. A method stays ON the trait only when the host
adds something the model cannot (visibility gating, the content rect, child recursion,
registry state). `plate_bevel` is gone: nothing overrode it, so it was always `None`.
`Owned` forwards the trait's methods and the four accessors; the extension trait needs no
forwarding.

### Global state has a plan (`docs/rfc-global-state.md`)

About 300 statics and 18 thread-locals: style (≈200 `RwLock`s beside the style registry),
interaction state (context menu, hover highlight, composition — per thread), properties of
"the" window (scale, metric, scroll phase), and caches (fine). The RFC sorts them and
phases the moves. Phase 1 is done: keyboard focus has ONE store, `UiContext::focused_widget`
(the `widget::focus` thread-local is gone; a widget asks `EventCtx::is_focused`, claims with
`request_focus`), and the context's dead hover and context-menu twins are deleted. Phase 2
is done: a window's interaction state — the context menu, the hover highlight and its
cursor, the side swipe, the input-method composition — is a `window_state::WindowState` the
window OWNS, which its shell makes current (`window_state::enter`) while it runs that
window's code; the modules' free functions (`context_menu::show`, `ime::caret`, …) act on
the current one, so no caller changed, and with none entered (a test) each thread has a
default. `context_menu::with_state` replaces reaching for the old `CONTEXT_MENU`. Phase 3
is done: the style is ONE snapshot (`crate::style::Style`: colour slots, layout slots, named
materials, the registry), published as an `Arc`; each former style `RwLock` is a
`style::StyleCell` handle with the same `.read()` / `.write()` API, reads take no lock, and
a reload runs as one `style::batch`, published once. A new style value is a field in its
module's `style_slots!` block and a `StyleCell` handle, not a new lock. Phase 4 is done: a
window's properties — scale, display metric, app id, fullscreen, maximized, vertical text,
and the scroll phase — are its `WindowState`'s (`window_state::Props`). The window whose
code runs reads its own; a thread with no window (a worker) reads the process-wide value,
which every setter also writes, so one window in a process reads exactly what it did.
`units::metric` asks cce-ui first (`units::set_metric_resolver`); a runner reports a
metric through `window_state::set_metric`. An app
that drives a widget's focus itself calls `UiContext::focus_widget` / `unfocus_widget`
rather than `w.focus()` / `w.unfocus()`, so the window's record of focus follows. Do not
add a static for state that belongs to a window: give it a field in `WindowState`.

### The registry holds pointers, and knows when they die

`UiContext`'s tree (`scene::tree::WidgetTree`) does not own its widgets: the app does, and
registers raw pointers to them. Every entry keeps a watch on a liveness token beside its pointer,
and every accessor (`get_ptr`, `children_ptrs`, `iter_registered`, …) resolves a pointer only
while that token exists.

**App widgets live in `Owned` boxes** (`widget::Owned<W>`, since 2026-10-07). An `Owned` keeps
the widget in a heap allocation of its own and carries a token for that ALLOCATION. Moving the
`Owned` (a `Vec` reallocating, a struct returned by value) does not move the widget, and the token
dies only when the box is freed. `Owned` is itself a `WidgetHost`, forwarding every trait method, and
reports the boxed widget through `WidgetHost::stable_target`. So `register_host(&mut self.x)`,
`set_focused`, `render_widget` and `link_parent_child` all record the boxed widget and the box's
token without the caller doing anything. `Deref`/`DerefMut` reach the widget, so
`self.x.set_text(..)` reads as before. Fields are `Owned<Adapted<X>>`, and are built with
`Owned::new(..)` or `.into()`.

A widget registered OUTSIDE an `Owned` falls back to its own address and its base's `Liveness`
token. That catches a drop but not a move, so `register_host` prints once per widget type to
stderr: `cce-ui: register_host: a <Type> is registered outside an Owned box`. A clean run of every
app prints none. The toolkit's own embedded children (a tree list's fields, a paginator's menu)
sit inside their parent's allocation and register through the crate-private
`register_embedded`, which does not warn.

Two shapes the sweep met:
- A widget used only as a paint STAMP and never registered (the designer dialog's
  toggle/slider/dropdown stamps) stays a bare `Adapted`.
- A roster that compares widget ADDRESSES compares the boxed widget, which is what the registry
  holds. The designer's `get_dyn` hands out the inner widget for that reason; its `get_dyn_mut`
  hands out the `Owned`, so registering through it records the box.

The pointer-taking entry points say so:
- `WidgetTree::register`, `UiContext::register_widget`, `set_focused_ptr`,
  `show_context_menu` and `handle_right_click` are `unsafe fn`.
- `Adapted::set_parent` takes its parent by reference.
- **Register a widget with `register_host(&mut w)`.** Every app uses it.
- `register_widget` remains for the paths that only have a pointer: `link_parent_child`,
  whose trait objects are not `'static`, and tests that exercise raw pointers.
- The demo's `register_roots` is the pattern to copy.

**The aliasing is sound since 2026-10-08, and checked by Miri.** Three things were not:

- **`Owned` held its widget in a `Box`.** A `Box` is a unique pointer to the language, so every
  reborrow of the widget through it — each `self.button.take_click()` — invalidated the raw
  pointer the registry had taken, and the registry's next dispatch was undefined behaviour,
  every frame in every app. Miri reported it ("trying to retag … but that tag does not exist in
  the borrow stack", at the registry's write). `Owned` now holds the allocation as a raw
  `NonNull` ROOT, which the app's `Deref` and the registry both derive their references from.
- **A registration from inside a widget replaced that root.** Opening a context menu
  (`show_context_menu`), focusing by pointer (`set_focused_ptr`), `set_parent` and the like
  re-registered the widget with a pointer taken from its own `&mut self` — a reborrow the next
  app access invalidates. `WidgetTree::register` now KEEPS a live `Owned` root against a
  pointer to the same address (compared by address, never read through); a widget swapped out
  of its box is elsewhere and registers as usual.
- **A widget focused itself through the registry mid-event.** The tree list's click handler
  called `set_focused_ptr` on itself, and the context delivered FocusIn back through the
  registry — a second `&mut` while its own was live. `UiContext::claim_focus` records the
  focus and tells the old holder without re-entering the claimant (what
  `EventCtx::request_focus` does too); the widget sets its own focused state.

The toolkit's embedded children (the tree list's search box, add-key button and popover,
inline editor; the paginator's menu) are `Owned` fields now, so they have roots of their own.
The rule left is the ordinary one: a `&mut` reached through the registry must not overlap one
taken through the `Owned` — an app does not hold `&mut self.x` across a `UiContext` call that
reaches `x`, and a `UiContext` call handed the widget uses what it was handed. The end state
that makes even that the compiler's job is a registry that owns its widgets and lends them out
by handle; it would touch every widget access in every app.

`a_dropped_widget_is_never_handed_out`, `a_clone_has_a_liveness_of_its_own`,
`an_owned_widget_survives_its_vec_reallocating`,
`swapping_the_widget_out_of_its_box_never_leaves_a_dangling_entry`,
`an_app_and_the_registry_take_turns_soundly` and `an_inner_registration_keeps_the_root` are
the tests; CI's `miri` job runs the `widget::owned` ones under Stacked and Tree Borrows.

**Runtime verification matters here.** Several scene changes are "compiles + tests pass; runtime
verification pending" per the RFC — the headless tests can't catch paint/event regressions. When
changing scene wiring, `cargo run` a real client (cce-files, cce-designer, cce-graph,
cce-system-interface) to confirm behavior, not just the test suite.

## Module map (where things live)

- `layout/` — the style getters and the code that was filed beside them, one module whose
  `mod.rs` re-exports every submodule, so `crate::layout::…` paths are unchanged (split
  2026-10-07 from one 7.4k-line file):
  - `src/layout/mod.rs` (~2.3k lines) — the sizing constants and the style getters/setters
    (heights, radii, fonts — many `*_font_parsed()` — gaps, the relief and bevel profile
    state), `reload_config`, and `read_preferred_fonts` / font-family resolution used by the
    cosmic-text path. It names no widget: what does is in the modules below.
  - `registry.rs` — the style registry (config flattened to one map of keys), its test overlay,
    the font-string helpers. **A layout style key has one home, the registry** (since
    2026-10-08): its getter reads `registry_float` / `registry_string` / `registry_bool`
    with the default, its setter writes the registry (a test's write lands in the
    per-thread overlay), and a font's parse is cached against the string it came from
    (`parsed_font`). About fifty keys also had a slot of their own, filled by a second scan
    of the same flattened lines by prefix (so `button_height` matched a longer key too),
    and thirty-five getters re-read and re-parsed the whole `config.kdl` once each on
    first use; a `(mm)` length never reached a slot. Do not add a slot for a config key.
    One thing it changed on screen: those first-use scans ran after an app's own setter and
    overwrote it, so cce-system-interface's `set_grid_gap(root_plate_gap())` lost to the
    config's `layout { grid_gap 18 }` — the compositor's window-tiling gap. Its multi-column
    pages now stand their sections the root gap apart, as the app asks (kept by choice).
    **And another program's key never becomes the toolkit's**: the flatten maps the paths
    the toolkit reads to its keys and leaves every other path whole (`layout.grid_gap`).
    Until the same day it cut an unmapped path down — the compositor's `layout` and
    `transparency` blocks to their bare keys, anything else past its first segment — which
    is how the tiling gap arrived as `grid_gap`. A new key the toolkit reads from a nested
    block needs its mapping; `another_programs_keys_do_not_become_the_toolkits`.
  - `bridge.rs` — the flat-host render bridge: `RenderTarget`, `PopoverCollector`,
    `render_widget`, `render_popovers`, the carve types that cross it.
  - `section.rs` — a settings page's sections: `PageFlow` places them (a masonry of
    columns as wide as fit `grid_min_col_width`), `PageLayoutBuilder` draws each ONCE in
    the slot the flow gives it and hands its height back, and `SectionContext` frames a
    section (title tab, well) around what a page puts in it. cce-system-interface is its
    user. Since 2026-10-08:
    until then it was `legacy.rs`, and every section was drawn TWICE, once into a
    throwaway target to measure it, through the `LayoutStrategy` trait, whose `allocate`
    took the height before the position. A section's position never depended on its own
    height, so drawing once places everything where it was (all 14 settings pages
    pixel-identical before and after in a scale-2 shadow, live readings aside).
    `LayoutStrategy`, `ColumnLayout`, `AdaptiveGrid`, `FlexLayout`, `RadialLayout` and the
    unused `Column` / `Row` / `Section` / `UiFrame` / `Radial` went with it; cce-files'
    browse page is a `scene::layout` column now, and the container layouts
    (`widget::ContainerLayout`, the gallery's Layout exhibit) are a trait of their own,
    `layout` and `measure`, without the cursor. New layout is `scene::layout`.
  - `form.rs` — what goes INSIDE a section, on `scene::layout` (since 2026-10-08): a page
    asks the section for a `Form` (`SectionContext::form`), declares its contents into it
    — retained widgets (`widget`, `widget_w`), text (`text`, `text_fill`, `lines`), rows
    and columns, `block`s of text lines with no gap, `rule`s, `space`, pieces it paints
    itself (`draw`), and one that takes the rest of the page (`fill`, with
    `Form::fill_height`) — and hands it back (`SectionContext::place`), which solves the
    tree across the content box and paints each piece where it landed, in declaration
    order. The spacing is the ladder's: the form a `controls_column`, a row a
    `controls_row`, both `control_gap` apart; a page states a size only where a piece has
    one of its own (a list's height, a button's width). Until the same day a section
    placed its contents with a cursor (`VStack`, `add_row` / `add_row_for`, `row_layout`,
    `text`, `spacing`) and a hidden one-or-two-column grid that widgets fell into unless
    their type name said otherwise, and every page added insets of its own (`+ 14`,
    `- 28`, `44.0`); the 14 settings pages moved onto the form and the cursor went.
- `color/` — the colour model and named colours (`colors` re-export module in `lib.rs`),
  split the same way: `mod.rs` the constants, statics and getters; `load.rs` reading the
  config into them (and `retired_surface_keys`); `math.rs` sRGB/linear, OKLab and the
  perceptual fade; `materials.rs` the named materials and rung bindings; `chords.rs` the
  tree/list search keys (input.kdl chords with a legacy colour-file fallback).
- **Vertical text** is `backend::text::set_vertical_text(Some(bar_thickness))` /
  `vertical_text()` — the status bar's mode when it stands on a screen edge: labels stack
  their characters and text shapes at a 1.05 line height. Process-wide on purpose (a property
  of the app); it was two bare `pub static`s at the crate root until 2026-10-07.
- **`config`, `input`, `motion`, `units`, `relief_spec`, `ipc`** — re-exported from
  `cce-core` (see "The GUI-free half is cce-core"), as are `color`'s hex/sRGB helpers,
  `scene::paint::DropletSpec` and the ramp spec functions (`widget::{format,parse}_ramp_spec`,
  `layout::sample_ramp_keys`).
- `context.rs` — `UiContext`: the retained widget tree, event routing, spatial grid, dirty
  tracking, hit-testing.
- `compute.rs` — what a compute job is, apart from the device that runs it: `Kernel`,
  `Binding`, the job rules and naga's parse (see "Compute jobs run in the browser too").
  `vk::ComputeDevice` and `web::ComputeDevice` run them.
- `a11y.rs` — the accessibility tree, in AccessKit's schema: `app_tree(&mut app, scale)` is a
  window's `TreeUpdate` — its registered widgets (role, name, value, bounds, actions, focus;
  `WidgetHost::a11y_role` / `a11y_value` are what a widget says about itself), the nodes an
  app without widgets declares (`Application::accessibility`, `AppNodes`), and an open context
  menu; a widget's parts of its own (a radio group's radio buttons) are `A11yItem`s
  (`Input::a11y_items`). `backend::a11y_unix` publishes it over AT-SPI (the `a11y` feature; see
  `docs/rfc-accessibility-locale.md`, phase 2).
- `l10n.rs` — the toolkit's catalogue (`tr`, `tr_args`, `catalog`) over `cce_core::l10n`;
  its English is `locale/en-US/cce-ui.ftl`.
- `style.rs` — the style snapshot: `Style`, `StyleCell`, `batch`, `style_slots!`.
- `window_state.rs` — a window's interaction state (`WindowState`, `enter`).
- `ime.rs` — input-method composition shared between the editing widget and the shell:
  `Preedit`, the composition and its generation, the reported caret, the reset request
  (see "Input-method composition is one model for every shell").
- `history.rs` — `History<T>`: the undo/redo snapshot stack (cap, gestures, grouped runs).
  The toolkit defines the stack and the routing, never the step — see the trait section.
- `widget/` — `container/` (vbox/hbox/scroll/menu/treelist/…), `input/` (button/slider/text_box/
  dropdown/…), `display/` (label/graph/svg/…), plus `editor.rs` (`TextEditorState`, the
  model behind `TextBox`), `line_edit.rs` (`LineEdit`: the text, caret, selection and keymap
  of a one-line field an app draws itself — cce-browser's URL bar and dialog fields) and
  `core.rs`. (The KDL/JSON-driven `json_layout.rs` is dissolved; `scene/layout.rs` is the
  box model.)
- `backend/` — the runner, split (since 2026-10-03) so a second shell (macOS, the browser)
  can share everything that is not Wayland: `app.rs` (the `Application` trait, `AppSender`,
  the plain types it speaks in), `driver.rs` (`Driver`: input state and routing — modifiers,
  key repeat, the undo/redo and plate-navigation chords, the CSD hit zones, the
  outside-press popover close, held-button release on a lost pointer, the scroll phase,
  the pinch fallback — fed in cce-ui's own terms and unit-tested with no compositor),
  `dom.rs` (the DOM's key and wheel vocabulary as the driver's: `map_key`, `wheel_frame` —
  portable, so tested natively), `appkit.rs` (AppKit's, likewise: key codes and
  characters, scroll deltas and phases, modifier flags and buttons), `frame.rs` (`build_frame`: the app's display list, damage, custom vertices and overlays,
  widget shaping, text and the popover-occlusion rects, tessellated into a `BuiltFrame` the
  renderer draws — no window system in it, tested with no GPU), `shell.rs` (the `Shell`
  trait — a window system's side of the run loop: exit, size requests, per-turn sync,
  title, the frame gate, configured, present — and `Pacer`, one turn of the loop over any
  shell: the tick's `dt` and its idle clamp, `desired_size`, key repeat, the title, the
  present-or-warm-down decision, and the ACTIVE / idle cadence; tested against a mock
  shell), `tessellate.rs`, `text.rs`, and `window_runner.rs`, the Wayland shell
  (`EngineState` implements `Shell`; its loop is dispatch, the connection's health checks,
  `pacer.turn`, and the close fade): it maps evdev
  buttons, xkb keysyms and `wl_pointer` axis frames into driver calls and carries out the
  grabs and cursors the driver asks for, and presents what `build_frame` built (grid patch,
  input region, glyph upload, the extent gate and buffer scale, the frame callback,
  `stage_renderer`, the draw). A routing change belongs in `driver.rs`, a change to
  what a frame contains in `frame.rs` and a pacing change in `shell.rs`, never in the
  Wayland code. A second shell implements `Shell` and calls `Pacer::turn` from its own
  loop (an animation frame, a run-loop observer), sleeping or scheduling for the `Step`. `menu_popup.rs`, `dnd.rs` and `text_input.rs` (`text-input-v3`, the input method's way in) are Wayland-only.
- `draw/` — what a renderer draws, with no renderer in it (since 2026-10-04): `Frame2D`,
  `Batch2D`, `PlatePush` and `batch_push_constants` (the one layout of a batch's 32-float
  parameter block — Vulkan pushes it, a renderer without push constants puts it in a
  uniform), `TextSpan`, `ImageQuad`, and `draw::images`, the image-id queue
  (`upload_rgba`, `update_pixels`, `free_image`, `renderer_epoch`, …) that a renderer
  drains with `take_pending`. They lived in `vk/` while Vulkan was the only renderer;
  `vk` re-exports every one at its old path, so `cce_ui::vk::upload_rgba` and the rest
  are unchanged for clients. Also here, shared by every renderer: `draw::glyphs`
  (`GlyphAtlas` — rasterizing, packing and the glyph quads; a renderer uploads
  `pixels()` when `generation()` moves — and `image_quad_vertices`), `window_info_data`
  (shader2d's `WindowInfo` block), and `draw::shaders`: `shader2d.wgsl` and `glyph.wgsl`
  live in `src/draw/` now, one source for both renderers. WebGPU has no push constants,
  so `shader2d_for_webgpu()` swaps the one push-block line for a `@group(1)` uniform read
  at a per-batch dynamic offset (`WEBGPU_BLOCK_STRIDE`); the backdrop is sampled with
  `textureSampleLevel(…, 0.0)` because WebGPU rejects implicit-LOD sampling in the
  non-uniform blur branch (the backdrop has one level, so the texel is the same —
  `frost_pair` is identical to the pixel either way). And the 3D halves: `draw::scene`
  (the raster scene's types, uniforms and the `Stage3D` trait) and `draw::rt` (the path
  tracer's schema, BVH and parameter blocks), with their shaders beside the 2D ones.
- `web/` — wasm32 only: `WebRenderer` (`new(canvas).await`, `resize`, `prepare_text`,
  `draw_frame_2d`, and `capture_next_frame` / `take_capture().await` or
  `take_pending_capture` to read a frame back). Its module doc lists what differs from the
  Vulkan path: an sRGB VIEW of the canvas's unorm format, the parameter block as a
  dynamic-offset uniform, a 1x1 backdrop, the blur snapshot as end-pass / copy / resume,
  every frame drawn whole. And `shell.rs`, the browser shell: `run`, `Fonts`, `Sizing`,
  `capture` (see "And an `Application` runs in a page" above); `scene.rs`, the 3D pass;
  `rt.rs`, the path tracer's compute tier;
  `compute.rs`, the async
  `ComputeDevice`; and `request_device`, the adapter and device every one of them asks
  for (with the limits a caller names raised to the adapter's).
- `mac/` — macOS only: the AppKit shell, `run` (see "And on a Mac, type-checked only").
- `protocol.rs` — inline-generated Wayland protocol bindings.
- `ipc` (in `cce-core`) — the `/tmp/<prefix>-<WAYLAND_DISPLAY>.sock` helpers (`socket_path`,
  `send_command`, the bounded `read_request_line`, `focus_window`), and `ipc::instance`:
  single-instance claim-or-forward for apps that run once per session.
- `icon.rs` — XDG icon-theme lookup: a `.desktop` `Icon=` key (or an SNI tray icon
  name) → a file on disk, plus `upload_themed` to rasterize/decode and upload it.
  **Not** `lib.rs`'s `upload_icon`, which loads a *bundled* cce-icons glyph by its
  own name for in-widget use; this one resolves names any installed app may ship.
- `file_dialog.rs` (rfd), `scale.rs` (HiDPI), `wayland.rs` (surface/scale detection, and
  `detect_metric` — the display's logical px per mm from its `wl_output` geometry).
- `units` (in `cce-core`) — lengths with units and the display metric; see the Units section below.

### The GUI-free half is cce-core (since 2026-10-07)

`config`, `input`, `motion`, `units`, `relief_spec` and `ipc` — and the parsers for the specs the
DE writes (hex colours, ramps, droplets) — moved to the sibling crate `cce-core`
(github.com/lsgalante/cce-core). cce-ui depends on it and re-exports each at its old path
(`pub use cce_core::config;` in `lib.rs`, `pub use cce_core::droplet::DropletSpec` in
`scene::paint`, …), so `cce_ui::config::…` and `crate::config::…` resolve as they always did and
no app changed. Two things did change:

- `DropletSpec::finish()` became the extension trait `scene::paint::DropletFinish` (a `Finish` is
  a renderer type `cce-core` cannot name): a call site writes
  `use cce_ui::scene::paint::DropletFinish;`.
- The `cfg(test)` gates in those modules are `cfg(any(test, feature = "test-isolation"))` there,
  and cce-ui's `[dev-dependencies]` names `cce-core` with that feature, so this suite still never
  reads the machine. Two tests of the style layer that sat in `config.rs`'s suite are
  `src/config_style_tests.rs`.

The compositor depends on `cce-core` alone (it used cce-ui only for config, input bindings,
`motion::enabled` and the relief/droplet parsers, and linked the whole toolkit for it), as do
`cce-browser-open` (the instance client, no copy any more) and cce-window-manager (the ramp, with
`default-features = false`: no config half, so no KDL or JSON). A change to these modules is a
change to `cce-core`; push it, then `bump-revs.sh` repins cce-ui and the rest.

## Markdown: `MarkdownView` and `DocEditor` (features, 2026-10-01)

Two opt-in features for the clients that show notes (Obsidian-on-cce):

- **`markdown`** — `widget::markdown`, the reading view: `cce_vault`'s
  blocks laid out at a width into draw items and click targets
  (`layout`, `Layout::paint` / `paint_scaled`). cce-notes' reading mode
  and cce-grid's note cards draw through it. Brings in `cce-vault`.
- **`doc_editor`** — `widget::doc_editor::DocEditor`, the editor with
  Markdown **live preview**: markup is hidden except on the caret's lines
  (the selection's, or the whole fenced block the caret is in), where it
  shows dimmed; `preview = false` is source mode. No extra dependencies.
  - `buffer` — lines, caret/selection as (line, byte), edits with merged
    typing/deleting undo runs, and a log of `Change`s for the layout.
  - `preview` — styles ONE line: block kind (heading, list, task, quote,
    rule, code, fence, frontmatter, table) plus inline segments that map
    1:1 onto source bytes. Markup is never replaced, only hidden, so
    caret maths never translates between screen and source.
  - `layout` — one styled line wrapped into runs, with the x of every
    byte (`ShapingMeasure::offsets`), so drawing, caret and clicks agree.
  - **Incremental:** a line is shaped only when it is drawn and has
    changed; undrawn lines keep an estimated height. A 5000-line note
    shapes one screen (`a_long_document_shapes_only_what_shows`).
  - **Host-driven, not a registered widget:** the app forwards keys,
    presses, motion and the wheel and paints it (`prepare` then
    `paint_prepared_with`, which takes a link resolver so unresolved
    links fade without a relayout). Answers come back as `Response`
    (`Follow(Target)` for a rendered-link click or a Ctrl+click). Undo
    and redo are the host's `Application::undo` / `redo` hooks calling
    `DocEditor::undo` / `redo` — the runner routes the chord there
    because the editor is not a focused widget.
  - Measure with the app's own font set: `DocEditor::new(.., system_fonts)`
    must match `Application::load_system_fonts`, or widths are not drawn
    widths. `widget::shaping` holds `Measure` / `ShapingMeasure`, shared
    with the reading view; a width includes trailing spaces (max of glyph
    x + w), which is what a run placed after it needs.
  - **Frontmatter is the Properties table** while the caret is outside
    it (`preview::properties` gives each line a role, `style_property`
    its row): a "Properties" header, keys in a column as wide as the
    reading view's, values inline-styled (links follow), list values as
    pills — a one-per-line YAML list is one pill per line, its key drawn
    by the first item and the key line itself zero height — and
    `true`/`false` as a checkbox that flips the bytes in place
    (`LineLayout::toggle`, undoable). The caret anywhere in the block
    shows all of it raw, like a fenced block; an unclosed block, nested
    maps and block scalars stay raw. `set_text` starts the caret past the
    block so a note opens on the table.
  - The caret does not blink (a blink is a frame every half second for
    as long as the window is open).

## A graph's wires are strokes in a style (since 2026-09-30)

`Graph` draws its wires in one of four `WireStyle`s: **orthogonal** (down,
across, down — what every wire was), **rounded** (the
same with the two bends rounded, the radius at most half a node's height),
**bezier** (a cubic that leaves the output and reaches the input heading
down, so a wire back up the graph loops) and **straight**. The style is
`style.surface.graph.node.wire_style` unless the host sets one
(`Graph::set_wire_style`, `None` to follow the config again). Colour and
width are `wire_color` and `wire_size` (px at 100%, scaled with the node
body) in the same block — both parsed since long before and READ BY
NOTHING until this change, when the wires were a hard-coded cyan 3 px.

- **The wires are not in `geometry_quads_tagged` any more.** They are
  `Graph::paint_wires` (also on `GraphController`), `Prim::Vector`s and
  `Prim::Arc`s, since only one style is axis-aligned. `Paint::paint` calls
  it after the grid; a host drawing the quads itself (the designer) calls
  it between `paint_grid` and the bodies.
- **The run across is on the first lattice line below the source**
  (`Graph::wire_turn_y`, since 2026-10-06), for orthogonal and rounded
  wires running down. It was halfway between the ports, so a wire spanning
  several rows ran down its source's column through any node standing
  there before it turned (row -1 to row 3 turned on row 1's line, through
  the node on it). Between adjacent rows no line lies between the bodies,
  and a wire running up has the source's own line first: both turn
  halfway, as before, and so does the connection being dragged. A rounded
  bend's radius fits the shorter leg. `a_wire_turns_on_the_first_line_below_its_source`.
- **One path, drawn and hit**: `wire_path` derives each style's pieces, and
  the splice hit test (`splice_wire_at`) walks the same pieces against the
  dragged ghost, so a drop lands on the wire as drawn in any style.
- **The orthogonal joins do not overlap** — the across run is widened by
  half a thickness to fill the corners and the down runs stop at its edge —
  so a translucent wire is one alpha throughout. A bezier is flat-capped
  pieces about 6 px of control net apiece, fine enough that no notch shows
  at the joins.
- **A wire may be thinner than a pixel** (the same day). `wire_size` has no
  floor; what is drawn does (`wire_stroke`): one DEVICE pixel, half a
  logical one at 2x, since the 2D pass has no antialiasing and a narrower
  axis-aligned quad covers a row of pixel centres or none. A wire under
  that is the pixel at the share of it the wire covers, so it reads
  thinner by reading fainter. Until then the stroke was clamped to one
  LOGICAL pixel, two device pixels at 2x, and `wire_size` below 1 did
  nothing.

`every_wire_style_runs_from_port_to_port` and the splice test, run in all
four styles, are the tests.

## A node has as many wires as the host says (since 2026-09-30)

`Graph::wire_pairs` draws a wire for EVERY parameter a host types `node`,
the k-th into input port k (`node_wires`, public so a host can read the
rule back) — a Switch's four inputs, a Boolean's With, a Transfer's From.
Until then it drew one, the parameter NAMED `input`, so every second
operand was a real connection with no line. A host that types no parameter
`node` keeps exactly that (cce-files and cce-graph pass `("input", name,
"string")`), so nothing changed for them. An empty value is a port with
nothing wired; a wire past the node's ports lands on port 0.

A connection the pointer makes reports its port too:
`GraphController::take_pending_connection_to_port` gives (input node id,
output node name, port), and the old `take_pending_connection` — which
takes the same connection — is the default for hosts that do not care.
Splicing a dragged node onto a wire takes only a wire into port 0, since
the splice rewires Inputs. `every_node_parameter_is_a_wire_into_its_own_port`
is the test.

## A node dropped on a node can swap with it (since 2026-10-06)

`Graph::set_swap_on_drop(true)` makes a node dropped on another node SWAP
places with it: the dragged node takes the other's cell and the other the
cell the dragged node was picked up from, and
`GraphController::take_pending_swap` hands the host (dragged id, other id)
to trade whatever else the two own — the designer trades their wires. Off
by default, where a drop on an occupied cell walks to the nearest free
one as it always did, so cce-files and cce-graph see no change.

- **The swap target** (`swap_target`, by id like `splice_target`) is the
  node on the cell nearest the ghost, set in `drag_update`. While it holds
  it is coloured as the dragged node is, `drop_target_cell_rect` is ITS
  cell (no walk), and it wins over a wire: a node's own wires run into its
  body, so a ghost over a node always touches one, and a swap and a splice
  are never both reported.
- **A host turns it off for a multi-node drag**: the widget drags one node,
  and one of a group trading places would scatter the rest.
- **A snapped drag sits only where it could land** (`drag_update`, since
  the same day): the nearest crossing when it is free or a swap target,
  else the nearest free one — `find_empty_cell`, the walk `commit_drag`
  makes — so the ghost never stands over a node it cannot stay on, and
  `drop_target_cell_rect` is where it is. It snapped to the nearest
  crossing whatever stood there until then. Unsnapped drags (a host with
  `grid_snap` off) are unchanged.

`a_node_dropped_on_a_node_swaps_with_it` is the test.

## Units — logical px inside, real lengths at the edges

The toolkit's working unit is and stays the **logical pixel**: every layout
node, style slot and widget measure is an `f32` of logical px. `units` (in `cce-core`)
adds the bridge to real lengths, in two parts:

- **`Len`** — a value with a unit (`px`, `mm`, `cm`, `in`, `pt`), parsed from
  `"2mm"` and resolved to logical px through a `Metric`. In config a length
  carries its unit as a KDL type annotation, the same way `(rgba)` and
  `(relief)` do: `width=(mm)2.0`. A bare number is a logical px, forever —
  nothing migrates. `config::kdl_to_json` turns an annotated number into the
  string `"2mm"`; the writer turns it back into `(mm)2`; `reload_config`
  stores it in the style registry's `lens` map, and `get_float` resolves it
  against the live metric at every read. So `layout::bevel_width()` and every
  other getter are unit-aware without knowing it, and a metric that arrives
  after config load (outputs come in after the first style read) or changes
  with the display is honoured without a reload. `get_len` returns the
  configured unit for editors that should show what the user typed.
- **`Metric`** — logical px per mm for the display this process is on, plus
  its **source**: `measured` (EDID via `wl_output` geometry, or the
  compositor's configured `size_mm` in its place — the client cannot tell
  them apart; `ccectl outputs` can), `forced` (`CCE_FORCE_PPI`), or
  `assumed` — the CSS 96 px/in convention when nothing is known (a headless
  shadow, a projector with no EDID). The source is carried so fabrication
  can refuse a guess: `Metric::is_real()`. The window runner installs it
  beside `scale::set_scale_factor` (`units::set_metric`); apps read
  `units::metric()`, `units::mm(v)`, or `Len::to_px()`.

**The relief is two shapes, and the config says which (2026-09-28).** A
**wall** is a carve's side — a recess, boss, ridge or trough cut into a
surface: buttons, wells, text boxes, the rows of a params pane — shaded as a
translucent light-and-shadow overlay on whatever is under it. An **edge** is
a plate's perimeter roll, the face curving down to its silhouette, shaded as a
multiply on the plate's own fill plus a specular crest. Each is a node under
`style.surface.relief` with the same three keys:

```kdl
relief light=0.15 width=9.3 {
    wall height=(mm)0.3 profile="smooth;…"
    edge height=4.0    profile="smooth;…"
}
```

`width` (the run of both, one number — see the roll-width note below) and
`light` (how hard the light falls across either shape — NOT a length, it is
`bevel_depth` → `Finish.strength`; `depth`, what every config said until
2026-09-28, was its alias for the rest of that day and is retired — reported
by path, not read, seeded from once by cce-relief whose Save writes `light`
and takes `depth` off) stay on the node itself, since both shapes share
them. `height` is a length — the wall's drop, the edge's rise —
and `height=(mm)0.3` is honest geometry resolved through the metric; unset, a
carve drops `relief_shade::RECESS_DEPTH` (0.6) of its wall (saturating at the
DE roll width) and the roll is a quarter-round of radius width, the look every
config had. `profile` is the curve as a ramp spec (absent or the identity
sentinel = the analytic curve: smoothstep for a wall, the superellipse
quadrant for an edge). **`shader`** on the relief node is the A/B switch
for how every one of those edges is LIT: `shader=(bool)false` renders the
relief prims through the legacy banded vertex shading instead of
shader2d's per-pixel SDF-lit branch (`layout::bevel_shader`; the registry
key keeps the bevel name because it selects how the shared lit edge is
computed, not which shapes exist). It was `window_manager.bevel_shader`
until 2026-09-28 — the block the relief keys were born in — and only a
number ever switched it, since a `(bool)` flattens to the string "false"
that the float read never saw; `shader_on` reads both now, and the old
spelling is retired with the block's other bevel keys (cce-relief's Save
carries a value it finds across). The registry keys never moved —
`bevel_depth`, `bevel_width`, `bevel_shader`, `bevel_height` /
`roll_height`, `bevel_profile_spec` / `roll_profile_spec` — so nothing
downstream of the registry knows. Until 2026-09-28 the keys were flat on the
node with the wall UNNAMED (`height`, `profile`) and the edge prefixed
(`edge_height`, `edge_profile`), which read as one shape with an "edge"
variant rather than two shapes; those spellings were aliases for the rest
of that day and are RETIRED — not read, reported by path with the other
retired surface keys, reaching no registry key (the flatten test asserts
it). While they were aliases a `prefer_relief_spellings` pass dropped a flat
one whenever its node spelling was present, because two spellings of one
registry key were otherwise decided by line order; with nothing left to
prefer, the pass is gone. `cce-relief` seeds from a flat key once, and its
Save writes the node spellings and REMOVES the flat ones
(`config::remove_config_value`), so a file migrates the first time it is
saved; `wall` and `edge` are `PROP_NODES` members so their keys land as
properties.

**The editor's knobs are not a style key.** cce-relief's Shoulder / Base /
Bias triples — the slider positions behind each profile spec — rode in the
style block as `(bevel)`-typed keys (`relief.wall.knobs` / `edge.knobs`,
before that `profile_knobs` / `edge_knobs`) so the editor could reopen where
it was left: editor state beside the values that draw, and the one relief
key nothing but the editor read. They live in that app's own
`~/.config/cce/cce-relief/state.kdl` now, one `knobs` node per Save target
(`shared`, a retargeted file's path, or `<path>#<key>` for a `(relief)`
value — two materials must not seed each other's sliders; `knob_state` in
cce-relief's `src/main.rs`). The registry keys `bevel_profile_knobs` /
`roll_profile_knobs` and their flatten arms are gone; a config that still
carries a knob key seeds the editor once, off the raw file, and the next
Save takes the key off under either spelling. The `(bevel)` type and
`parse_bevel_knobs` stay, because a `(relief)` value still carries its own
`k=` ride-along and the data editor's preview of such a value draws it. `layout::carve_depth_px` states the drop rule
once for the tessellator's CSG features and, through `WindowInfo.relief_meta`,
the shader's free carves; `carve_depth_ratio` / `roll_height_ratio` feed the
shading twin (`Finish.carve_depth` / `roll_height`). A `(relief)` value
carries the drop as `h=` (a length: `h=0.5mm`, or bare px) beside `w=` and
`d=` (light; `l=` reads as an alias). `cce-relief`'s Height knob is the editor:
its section's depth numbers read in mm when the metric is real, and Save
writes `wall.height` in the unit the config already spells (an untouched
slider verbatim, a moved one converted through the same metric that seeded
it — so a headless session never turns `(mm)0.3` into `(px)1.1339`, which
it did until 2026-09-28), choosing a unit only for a height the config
never had: `(mm)` when the metric is real, px otherwise
(`height_len_for` in cce-relief's `src/main.rs`).

**There is one roll width.** `style.surface.relief.width` is the run of every
roll and wall: the root plate's perimeter (`PlateSpec::window`), a `PlateSpec`
pane plate, a bordered widget plate under relief (`append_widget_plate`, via
`colors::plate_bevel_width`), every control wall, and the length
`edge.height` is a rise against. Until 2026-09-28 the widget-plate path had a
width of its own — `style.surface.plate.bevel_width`, default 6 against the
relief's 9.3 — so two pane plates in one window rolled over different widths
depending on which painter drew them, and no single key made a pane match the
window lip. `plate_bevel_width()` now returns the relief width and nothing
else: the old key was an explicit override for the rest of that day (so a
config carrying it kept its look through the change) and is RETIRED — a
config still carrying it is reported by path with the other retired surface
keys and the key is not read, since an override that keeps working is a
second width by another name. `the_pane_roll_is_the_relief_width` pins it,
and cce-relief's Save removes the key.

**And the relief can leave the screen.** `scene/heightfield.rs` integrates the
height curves the shader only differentiates and samples a frame's plates into a
height field — plates stack, carves etch, exactly the composite model the shader
lights — then writes it as a 16-bit PNG whose sidecar carries the pitch and range
in millimetres via the metric. A pinned `height=(mm)0.3` is 0.3 mm in that file.
The sidecar states the metric's source; on an assumed metric the millimetres are
a guess, and a fabrication tool should say so.

Why not millimetres inside: UI sizes are perceptual and angular, not physical
— a hit target should not become 8 mm on a projector three metres away.
Documents and fabrication content live in real units and convert at view
time. Two domains, one bridge.

On the live laptop panel (3840×2400 over 344×215 mm at scale 2) the metric is
5.58 logical px/mm (141.8 ppi); the default 9.3 px relief roll is 1.67 mm, and
the 96 ppi assumption would have called it 2.46 mm.

## Fonts & assets

`lib.rs` builds the cosmic-text `FontSystem` (re-exported as `cce_ui::cosmic_text` so clients
need no text dependency of their own). Bundled fonts load from `$CCE_FONTS_DIR` (else
`~/Dropbox/Fonts`); bundled icons from `$CCE_ICONS_DIR` (else `~/projects/cce/cce-icons/svg`).
System fonts are loaded only when `$CCE_LOAD_SYSTEM_FONTS` is set (or via
`create_font_system_with_system_fonts()`, used by the font picker). Configured custom font
families are validated at startup with a warning if missing.

### Every symbol is a cce-icons glyph (since 2026-10-05)

The DE's one icon source is `cce-icons/svg`, and the toolkit draws a symbol
— a chevron, a check, a mark, a + or a − — ONLY as one of those glyphs, never
as a character (`▼`, `✓`, `●`, `›`, `+`, an emoji) in whatever face the font
falls back to, and never built from primitives. Apps hold to the same rule.

- **`PaintCtx::icon(name, rect, color)`** draws a glyph tinted like the text
  beside it (raw sRGB, as a text colour is; its alpha the image's), rasterized
  at twice the rect so it is crisp on a 2x output. `icon_untinted` is for the
  `weather-*` family, the one set that carries its own colours.
  `RenderTarget::icon` is the same call for the legacy target (a
  `PopoverCollector` draws nothing). Underneath, `upload_icon_tinted` /
  `icon_tint` / `icon_pixels`: the artwork is white, so multiplying is
  tinting.
- **A context-menu row MARK is a glyph**: a label beginning `MARK_CHECK`
  (`"✓ "`), `MARK_ON` (`"● "`) or `MARK_OFF` (`"○ "`) draws `check`, `circle`
  or `circle-outline` at its left and the label without it. The text stays the
  row's identity, so hosts that match their own labels still match. A page
  row's `›` and the back band's `‹` are `chevron-right` / `chevron-left`.
  **A `Dropdown` list honours the same marks** (`mark_column`): a marked
  option draws its glyph in a column every row then keeps, and the closed
  trigger shows its value without the mark — so a "View" menu-button marks
  its switches as a menu does (`a_marked_option_draws_its_glyph`).
- **The menu popup has images now.** Its renderer used to pass none
  (`images: &[]`). It keeps its OWN copies (`menu_icon_ids`): a glyph painted
  by the window renderer's id is looked up with `icon_source` and uploaded
  once into the popup's table (`VkRenderer::upload_rgba_now`). And it no
  longer drains the shared upload queue (`set_shared_uploads(false)`) — it
  did, so an upload queued between the window's frame and the popup's landed
  in the popup's table and never drew in the window.
- **A vertical `ButtonStrip` / `Paginator` takes glyphs by name**
  (`with_icons`). It took the label's first character when that was a word of
  its own ("📁 Browse") and drew it as text.
- Converted: dropdown arrow (`arrow_rect`, `ARROW_SIDE`), the params pane's
  picker face (it drew "▾" AND the arrow), menubar title arrow and checked
  items, spreadsheet sort marks (`chevron-up` / `chevron-down`), spinbox −/+,
  font selector ("Aa" → `font`), breadcrumb overflow (`more-horizontal`),
  markdown's unrenderable embed (`Draw::Icon`, `link`), the tree list's
  missing-icon fallback (now an empty slot) and the copy button's fallback
  ("📋" → "Copy").
- **A flat host gets glyphs, strokes, arcs and discs.** `render_widget`
  replays a widget onto a host's `RenderTarget` and dropped every image,
  vector, arc and circle: after the symbols became glyphs a flat host lost
  them all, and a graph there (cce-files' Graph page) never showed a wire. A
  bundled glyph now goes to `RenderTarget::icon`, a stroke to `line`, an arc
  to `arc`, a disc to `circle` — each a no-op by default, so a host draws what
  it implements (`a_widget_glyph_reaches_a_flat_host`,
  `a_graphs_wires_reach_a_flat_host`).
- **`ParametersBg` paints its controls' glyphs itself** (2026-10-06). The
  pane draws its rows' chrome and collects their TEXT (`own_text_labels`)
  without ever running a control's `Paint::paint` into the frame, so when
  the symbols became glyphs every dropdown, picker and spinbox in it lost
  its arrow or −/+. `Adapted::own_glyphs` is the image half of that bridge,
  and `paint_ui` / `paint` emit `child_glyphs` over the chrome. A container
  that collects its children's labels must collect their glyphs too
  (`the_pane_draws_its_controls_glyphs`).
- **Kept as shapes**, being indicators and not symbols: `StatusDot`'s LED
  disc and the plate dock's corner dot.

`every_glyph_the_toolkit_names_is_in_the_icon_set` scans the source for glyph
names and fails on one with no file — a missing glyph draws NOTHING, silently
(skipped where the icon set is not checked out). `menu_marks_and_chevrons_are_glyphs`
holds the menu convention. **A shadow session needs `CCE_ICONS_DIR`**: its
HOME is isolated, so the default path finds no icons and every glyph is blank.

## Debug environment variables

All opt-in, all read once, all quiet when unset — set one and run any client.

- `CCE_PLATE_DEBUG=1` — per frame, how many relief carves grouped into their host plate
  as exact CSG features vs fell back to standalone overlay shading, and for each fallback
  **why** (one of six rules: ridge, edge-suppressed, tinted, feature budget, no enclosing
  plate, host's feature run closed). The two paths shade junctions differently, and three
  of those rules are dynamic, so this is the answer to "why does this widget's carve look
  different here?". Note what it reveals: grouping is *rare* — the reference demo groups
  2 of 10, cce-files 0 of 7, because any ordinary geometry painted after a plate closes
  its grouping window (correctly — the carve's shading is baked into the plate's earlier
  draw).
- `CCE_PRESENT_DEBUG=1` — swapchain present/acquire tracing.
- `CCE_A11Y=1` — publish the accessibility tree over AT-SPI for an app that has not opted in
  (`Application::publishes_accessibility`; needs the `a11y` feature). `CCE_A11Y_DEBUG=1` logs
  each reader connection, action, publish (node count, focus, build time) and window-focus
  change. A shadow window holds no keyboard until `ctl focus-window <app_id>`, and nothing
  reads FOCUSED until it does. See `docs/rfc-accessibility-locale.md`, phase 2.
- `CCE_UI_MENU_POPUP=0` — keep the context menu in the window instead of its popup
  surface (see "The context menu draws in its own popup surface").
- `CCE_VK_DEVICE=<substring>` — force a physical device; `CCE_VK_RT=0` disables ray tracing.
  `integrated`, `discrete` or a name substring; any of them also lifts a session-wide ICD
  pin (`VK_DRIVER_FILES`) for the process. **A preference the chosen device does not meet
  is printed to stderr** (since 2026-10-02, once per process, `unmet_device_preference` in
  `vk/core.rs`): what was asked, what was taken, and every device the loader offered — a
  driver that failed to LOAD is in no list, which is the case the line points at
  (`VK_LOADER_DEBUG=error` says why). Until then the fallback was silent, and a fallback
  renders exactly as the asked-for device would, so nothing on screen gave it away.
- The style registry loads config LAZILY: a value read before the first load and one read
  after come from two configurations. The suite no longer reads the machine's config at all
  (see "The tests never read the machine's config"), but the lazy load still holds within a
  run, so a test asserting on shading numbers pins its inputs (`relief_shade`'s tests:
  `pinned_light`, `pinned_finish`); `deeper_carve_shades_harder` failed run alone and passed
  in the full suite until it did.
- `CCE_FORCE_SCALE=<f>` — override HiDPI scale detection.
- `CCE_FORCE_PPI=<f>` — pin the display metric (logical px per inch) regardless of what
  the outputs report; a headless shadow has no EDID and would run `assumed`. The live
  panel is 141.8.
- `CCE_HEIGHTMAP=<file.png>` — export the client's third rendered frame as a relief
  height field (16-bit greyscale PNG + `<file>.json` sidecar: pitch, range in mm, datum,
  metric source); `CCE_HEIGHTMAP_MM=<mm>` resamples to that pitch. `scene/heightfield.rs`.
- `CCE_UI_FAULT_RECONNECT=<seconds>` — drop the session that many seconds after it
  starts, exactly as a transport error would, so the reconnect path (and the second
  `renderer_init`) can be exercised on demand instead of waited for. One-shot per
  process: the app reconnects and then stays up. A float, so `0.5` works; logs
  `CCE_UI_FAULT_RECONNECT: dropping the session` at WARN. In a shadow, note that the
  window MOVES across the reconnect (off-view recall), so capture with
  `shot-window <id>` and re-read `ctl windows` before any pointer work.

### The params pane stacks its labels when it is narrow

`ParametersBg` has two label layouts: **inline**, the pane's own column of
labels beside unlabelled controls, and **stacked**, each control carrying
its label above itself. `param_label_layout` in the style config is the
PREFERENCE (`stacked`, or inline by default). Since 2026-09-28 the rows
decide on top of it: an inline pane whose label column would leave any
visible row's TRACK shorter than `MIN_INLINE_TRACK_W` (120 px) stacks, and
takes the column back when there is room. The track is what is measured —
the control's rect less its chrome (`control_chrome`: a slider's 60 px
readout and its gap, plus a float3's axis column) — because the first cut
measured the rect and let a slider's track shrink to 52 px before the
labels moved; a control with no track is measured whole. One decision for
the pane, taken by its shortest track. The rows' widgets are built with or
without their label and the two layouts have different row heights, so the
flip is a relabel and a re-layout on every metrics refresh — a rect
assignment, a rebuild, a section collapsing (`apply_label_layout` /
`relabel_rows`) — not a flag; `Adapted::clear_label` is the way a label
comes OFF a widget, and `slider::detached_strip` reads an empty label as
none for the widgets that store whatever they are handed.

### A value control reads the wheel as "up is more", a natural finger too

`MouseScrollDelta::value_notches_y` (2026-09-30) is what a VALUE control —
`Slider`, `Slider2D`, `Spinbox`, a context menu's slider row — turns by:
a wheel notch up is positive, and a finger's pixel delta is taken as it
comes with natural scrolling off and NEGATED with it on
(`input::natural_scroll`, input.kdl's `trackpad { natural_scroll }`, read
once). The delta the runner hands out is what a LIST scrolls by, and a
natural list moves its content the way the fingers went; a value has no
content to move, so under natural scrolling the fingers going up is
more. Until then each control read `notches_y` with a sign of its own:
the slider was right for a natural trackpad and backwards for a wheel,
the spinbox and the menu slider the other way round. Under `cfg(test)`
the toolkit's suite reads natural as off; `force_natural_scroll` sets it
per THREAD for a test that drives a control with a finger — a dependent's
test binary links cce-ui without `cfg(test)` and would otherwise read the
machine's. `a_value_control_reads_up_as_more_on_a_wheel_and_a_natural_finger`
is the test.

### A flick coasts with animations off

The animations switch (`motion::enabled`) stops a wheel notch's GLIDE and
not a trackpad flick's COAST (`scroll_motion::with_animations`, since
2026-09-29). Until then it turned both off, so on a power mode with
animations off a two-finger scroll stopped dead at the lift in every
pane that scrolls through `ScrollMotion` — lists, the params pane, the
spreadsheet, a graph's pan. The glide is an animation the toolkit adds;
the coast is the rest of a gesture the hand made, and its own setting is
input.kdl's `kinetic_scroll`. Two value controls still follow the switch,
deliberately left alone: a slider's and a ramp key's drift after a scroll
over them, which changes a VALUE after the hand has stopped.
`the_animations_switch_stops_the_glide_and_not_the_coast` is the test.

### A finger scrolls (touchscreens, since 2026-10-05)

The runner binds `wl_touch` when the seat offers it, and `backend/touch.rs`
turns the first finger into pointer input by what it does: a **tap** clicks
where it landed, a finger that **moves** past `SLOP` (10 px) scrolls — a
`PixelDelta` equal to the finger's travel, `ScrollPhase::Finger`, dispatched
at the down point, then `FingerEnd` at the lift so a flick coasts through
`ScrollMotion` like a trackpad's — and a finger **held** `HOLD_MS` (400 ms)
before moving is a held left button (a slider thumb, a text selection, a
scrollbar). The hold needs no timer: nothing is sent while the finger rests
inside the slop, so the choice is made at the first motion past it. Other
fingers are ignored until the first lifts. `TouchTracker` is the pure state
machine (tested in that file, and portable); what its actions do is the
driver's (`Driver::touch`, routing like every other input), and only the
`TouchHandler` impl beside the tracker is Wayland's, so `window_runner.rs`
carries only the fields and the capability hook.

A finger is always natural — the content goes where it is pushed — so the
dispatch runs inside `input::with_natural_scroll(true, …)` and a value
control's `value_notches_y` reads the finger's real direction whatever the
trackpad's setting. No per-app trackpad factor either: 1:1 keeps the content
under the finger. Not by finger: CSD moves/resizes and
`Application::take_window_action`, since the compositor checks those serials
against a pointer grab; the runner drains a queued action after a touch so it
cannot fire on the next pointer press. Binding `wl_touch` is also what moves a
cce-ui window off the compositor's emulated-pointer route
(`cce-compositor`'s `cursor::TouchRoute`), where a finger drag was a held
button and selected rather than scrolled. `CCE_SCROLL_DEBUG=1` logs each
touch scroll (`[scroll] touch: …`); in a shadow, `ccectl touch down|motion|up`
drives it.

### A field being edited says so (text-input-v3, since 2026-10-05)

A widget open for typing calls `cce_ui::text_input::claim(x, y, w, h)` from
its paint, every frame (window px, the `PaintCtx` offset added). A claim per
frame, not an enable/disable pair, because a field leaves editing on many
paths (Enter, Escape, a click elsewhere, focus loss, its page dropped) and a
widget that stops painting has stopped claiming. `claim` is
`ime::report_caret` by its first name — the two were written the same day on
two branches and merged into one: the frame's last claim is `ime::caret()`,
which every shell reads (see "On Wayland it is `text-input-v3`" above for the
Wayland half, `backend/text_input.rs`). The compositor raises the on-screen
keyboard on an enable that follows a touch (`cce-compositor`'s `osk.rs`).
`Spinbox`, `Slider`'s readout, `ColorSelector`, the params pane's code rows
claim their field; `TextBox` and a focused `DocEditor` claim their field (the
viewport) and then report the caret once it is drawn, which wins; an app that
draws its own text (a `LineEdit`, a terminal, an editor) must claim from
`display_list` while it has a caret, or the board will not follow it. Keys
still come over `wl_keyboard`; an input method's commit is typed through
`Driver::commit_text`. On `enter` the last frame's claim is applied at once,
and `leave` disables an enabled text input: wlroots keeps the enabled state
across a leave, and a stale "enabled" turns the next enable into a plain
commit the compositor ignores.

Three things learned taking it to the apps (2026-10-05):

- **A press re-announces an open field.** The compositor reacts only to an
  enable or a commit right after a touch, and a field already open (a focused
  terminal claims from the moment it maps; a text box still editing) sends
  neither when tapped again. So the driver marks a pointer or touch press
  (`ime::note_press`) and the next plan that still has a caret commits once
  more, unchanged (`TextInput::plan`'s `pressed`); a press that ends the
  editing disables instead, so tapping away never flashes the board. A mouse
  click re-commits too and the compositor ignores it (no finger armed it).
- **An app that replays cached geometry must replay the claim.** A host that
  paints its widgets only in a `rebuild_layout` (cce-system-interface,
  cce-files) claims on rebuild frames alone, and the first replayed frame
  disables the field: in a shadow the board was already gone two seconds
  after the tap. Run the rebuild under `text_input::capture` and claim what
  it returns on every frame.
- **A widget drawn from its host's aggregates never claims.** `ParametersBg`
  paints its hosted text boxes, spinboxes, sliders, colours and vectors from
  its own views, not through their `paint`, so none of their claims ran in
  the designer; the pane claims for the row being typed into
  (`claim_typing`), from `paint_ui` as well as `paint`.

### A host may name the phase; a test may pin the settings (2026-09-30)

The phase a wheel event belongs to (`Finger`, `FingerEnd`, `Wheel`) is the
window's (`window_state`), published by the runner before each dispatch, and
`ScrollMotion::apply_px` reads it. Until 2026-10-08 it was one process-wide
atomic, so a test setting it changed what every other test's pixel delta
meant; with no window entered it is now each thread's own. `ScrollMotion::apply_phase` takes the phase
as an argument (`apply_px` is it with the published one), for a host that
reads the phase itself and hands it on; cce-designer's viewport does, and
its test drives a flick without touching the global.

`scroll_motion::force_scroll_settings` pins what `scroll_settings()`
answers on the calling THREAD, over input.kdl and the animations switch
alike, as `force_natural_scroll` pins natural scrolling: a dependent's test
binary links cce-ui without `cfg(test)`, so a test whose result hangs on a
coast would otherwise pass or fail by the machine's `kinetic_scroll`.

### The suite's animations switch is its own

`motion::enabled()` reads `/run/cce/animations` in a shipped binary (see
"Animations switch" in `../cce-compositor/WORKSPACE.md`). Under `cfg(test)`
it does NOT: it answers on, or whatever `motion::force_for_test` set on the
calling thread. Until 2026-09-28 the test binary read the machine's file,
so the spreadsheet's wheel-glide tests and the scroll region's fade test
passed on mains and failed on battery — the same lesson as cce-designer's
pinned settings path and lattice, one layer down. A test wanting the
snap-instead-of-ease path forces it for its thread; nothing sets
`CCE_ANIMATIONS`, which is process-wide and would race the parallel suite.
This pins only cce-ui's own suite: a dependent's test binary links cce-ui
without `cfg(test)`, so a dependent test that eases still follows the
machine — none does today.

### A colour test that reloads a knob puts it back

`color::style_write` (every `set_*`) is a per-thread overlay under
`cfg(test)`, but `reload_colors` writes the process-wide globals — and an
absent frost knob KEEPS its last value by design, so `reload_colors("")`
resets the named materials and bindings (replaced wholesale) and nothing
else. A test that reloads `frost radius=3.0` and closes with the empty
reload leaves radius 3 behind for every test after it; that is what had
`frost_from_style_and_flag` fail one run in eight (2026-09-28, the radius
read 3.0 for the default whenever its neighbour ran first). Two rules: a
test that ASSERTS a knob pins it on its own thread through the setter, the
radius included, not just the ones it is about; and a test that RELOADS a
knob reloads its default back before the empty reload, and asserts the
globals are back. `test_color_state_lock` orders the reloaders against
each other; it cannot undo what one of them left behind.

### A scroll gesture in the params pane belongs to what it begins on

`ParametersBg`'s wheel arm gives a gesture to the VALUE CONTROL it begins
on and to the pane otherwise, for a wheel notch and a trackpad finger
gesture alike: a slider and each row of a float3 by the band's halo
(`Slider::scroll_hit`), a spinbox by its row. The control that acquired it
keeps it until the gesture ends (`scroll_initiate_widget_id`, 250 ms of
quiet), so the latched control is asked first and a gesture the pane or
another control owns never lands on a band that slides under the pointer.
From 2026-09-21 to 2026-09-28 every FINGER gesture was the pane's from
anywhere, on the reasoning that a pane of mostly controls was scrollable
only from a label; that reasoning was about the designer's Alt+D Settings
list, which stopped being a `ParametersBg` on 2026-09-24, and what the
rule left behind was sliders a trackpad could not turn. The pane still
scrolls from the label column, the gaps between bands, and every row that
is not a value control.

### A slider's notch follows the value on a wide range

One wheel notch or arrow press moves a `Slider` by `notch_step`: 2% of the
range for a range up to `FINE_SPAN` (20) wide, which leaves every slider
the toolkit had exactly as it was. A WIDER range steps 2% of a span that
grows with the value's magnitude — 20 times it, floored at 20 and capped
at the range — so -1000..1000 moves 0.4 a notch near zero, 4 at ten, and
the old 40 only from a hundred up. By magnitude rather than a finer flat
step: one fine enough to set 0.06 takes thousands of notches to reach
1000, and this takes under sixty. A drag still maps the pointer to the
whole range; the readout is still where an exact value is typed.

### A float3 group can carry a trackball

`Float3::set_trackball` (a `ParametersBg` row typed
`float3:lo:hi:trackball`) stands a ball left of the three rows, as tall as
they are. The vector is drawn on it from the centre — X right, Y up, Z
toward the viewer; bright on the near side, dim pointing away — and
dragging the ball rolls it under the pointer, turning the vector and
keeping its length. The rows stay: a direction is turned on the ball, a
component typed or a length changed on a row. Three things to know. The
drag turns its OWN full-precision copy (`BallDrag`), because the rows hold
the vector rounded and a host writes the rounded string back between
moves; turning that loses every step smaller than a readout tick. With
the ball on the rows read to three decimals, since at two a vector of
length 0.06 has seven directions. And the ball is painted by
`paint_ball` through the host's scene path (`paint_scene_rows`), not
`Paint::paint`: a sphere is not a prim the legacy flat views carry. A
vector of no length is given a length of one by the first drag. The ball
counts as chrome in `ParametersBg::control_chrome`, so the label layout
is decided by the track it leaves.

**The ball can be seen from a host's camera** (`Float3::set_view`,
`ParametersBg::set_trackball_view`): three rows, the camera's right, its
up and the direction toward it, in the vector's space. The vector and its
rings are then drawn as the host's 3D view shows them, and a drag or a
scroll rolls about the CAMERA's axes — pushing the ball right swings the
vector to the right of the screen, whatever that is in the scene. Only
what the ball shows and how it turns: the rows and the value stay in the
vector's own space. Without one the view is X right, Y up, Z toward the
viewer. The pane keeps the view for rows built later.

**The ball carries rings** (`Float3::ring`, `RING_ANGLES`): five circles
of latitude about the vector as their pole, thirty degrees apart, the
near half of each drawn in short strokes. A lit ball is the same from
every side, so without them a drag showed the vector move and the ball
stand still; the rings are the vector's own, concentric circles when it
points at the viewer and foreshortening into ellipses as it turns, which
is how a rotation is read. They follow the full-precision direction a
drag or a scroll is turning, so they move on every pixel.

**A scroll rolls the ball** (`ball_scroll`), as content is scrolled: its
surface moves the way a page under the pointer would, a two-finger
gesture in both axes at once and a wheel notch in one, by `SCROLL_TURN`
(15 degrees) a notch — the fine handle beside the drag's 1:1. The ball
has an id of its own (`ball_id`) in the scroll-gesture bookkeeping, so a
gesture that begins on it is the ball's until it ends and one a band
holds stays the band's over the ball; `wheel_zone` / `wheel_latched` are
what the pane asks. A scroll keeps its own full-precision copy too
(`fine`), reused while the rows still hold what it rounds to: a trackpad
sends a pixel at a time, a quarter of a degree, which on a short vector
is less than the rows can hold.

### A spreadsheet is columns of values, written as they are painted

`Spreadsheet` holds its table as COLUMNS (since 2026-10-07, `SheetColumn`:
`Text`, `Int`, or `Float` to a number of decimals) and writes a cell's text
only when it paints the cell. `SpreadsheetController::set_spreadsheet_columns`
takes them; `set_spreadsheet_data` (rows of strings) still works, its cells
becoming text columns. A sort compares the values — numbers by number, text
as before (numerically where both cells parse) — instead of parsing every
cell twice a comparison. It exists for a host that refills the table every
frame: the designer did so during a simulation's playback with every cell of
every row formatted into a `String`, ten thousand rows of them for a pane
that shows thirty. A float cell is `format!("{:.*}", decimals, value)`, the
string a host formatting it itself would have written.
`a_column_table_is_written_as_painted_and_sorts_by_value` is the test.

### A spreadsheet's columns are as wide as their content

Each column of a `Spreadsheet` (since 2026-10-07) is as wide as its header
with room for the sort glyph (`SORT_MARK_CHARS`, kept whether or not the
column is sorted, so a sort moves nothing) or its widest cell, whichever is
wider, plus `CELL_PAD` — at least `MIN_COL_CHARS` characters. The widths are
counted in characters when the table is set (`set_columns`), from the values
rather than from written text (`SheetColumn::max_chars`: a whole number's
least or greatest, a float's sign and integer digits and its decimals, the
non-finite spellings), and made pixels by the label font's character width,
the font being monospace (`col_edges`). The table does not stretch: what is
left of a wide pane is empty, a line marking where the last column ends; a
table wider than the pane scrolls as before. Until then every column was an
even share of the pane, floored at 76 px, so a point index or a 0/1 column
was as wide as a four-decimal float. A host whose header is its widest cell
saves the most by keeping headers short. `columns_fit_their_content_and_overflow_scrolls`
and `a_columns_widest_cell_is_worked_out_from_its_values` are the tests.

### A spreadsheet's rows can be selected

`Spreadsheet` keeps a selection of rows (since 2026-09-29): a press on a
row selects it alone, or clears it where it was the whole selection; with
ctrl the press toggles that row and leaves the rest; with shift it selects
the run from the last row pressed without shift to this one. A press on
the empty body under the last row clears. The host pushes the modifiers in
ahead of the press (`set_modifiers`), as it does for every widget.

**The selection is of rows of the DATA, not of places in the pane.** A
sort moves where a selected row is drawn and not what is selected, and a
shift run is the rows DISPLAYED between the two, which under a sort is
what the eye sees. It stands across `set_spreadsheet_data`, less the rows
a shorter table no longer has: a host that re-sets the table on every
frame of a playback keeps its selection, and one whose table has become
something else clears it itself.

`SpreadsheetController` carries it: `selected_rows` (ascending),
`set_selected_rows`, and `take_selection_change`, which is true once after
anything changed the selection. A press on a RAISED scrollbar is the drag
surface's and selects nothing (`body_row_at`); a sunk one is behind the
plate and the press is the row's. Selected rows wear
`highlight_primary_color` at 28% over the zebra.
`rows_select_alone_toggled_and_in_runs` is the test.

### A spreadsheet's scrollbars are a cross behind its plate

`Spreadsheet`'s two bars (since 2026-10-06) ride the CENTRE lines of what
they scroll — the vertical one the pane's width, the horizontal one the
body's height — so with both they cross at the middle of the body, over
the cells, reserving no lane; the params pane's bar and the designer
dialog's ride theirs the same way. Until then they were 6 px strips at the
right and bottom edges, always in front. They are `scrollbar_width` × 1.6
pills in the shared track and thumb colours, and they share one
`ScrollbarActivity`: a wheel, a key, a glide or coast in motion, or a
thumb drag raises them; a pointer over a raised bar holds them up; with
nothing holding them for the hold window they sink. Sunk they take no
press (`drag_begin` and `body_row_at` ask the latch). The widget paints
the FORE copy at the activity's fade; the copy that idles behind the
plate is the host's, before the plate, through `paint_scrollbars(rect,
ctx, 1.0)` (`scrollbars_shown` says whether there is one) — the plate is
the host's too. The vertical bar owns the middle of the cross for a drag.
`the_scrollbars_cross_at_the_body_and_sink_until_scrolled` is the test.

### Every scrollbar rides a centre line, behind the plate

The rule the spreadsheet's cross follows is the DE's one scrollbar design
(2026-10-06), and every scrolling list and pane in the toolkit offers it:

- **It rides the CENTRE line of what it scrolls** — the vertical bar down
  the middle of the width, the horizontal one across the middle of the
  viewport — so with both they cross there. Over the content, reserving no
  lane. `layout::centred_scrollbar_width()` thick (`scrollbar_width` × 1.6:
  over rows the stock width reads too slim), pills in the shared track and
  thumb colours.
- **It idles BEHIND the host's translucent plate** and takes no press
  there: a press on its lane is a press on the row under it.
- **A scroll raises it in front with a fade** (`ScrollbarActivity`): a
  wheel, a key, a glide or coast in motion, a host's own scroll
  (`notify_scrolled`), a thumb drag. A pointer over a RAISED bar holds it
  up; hover never raises a sunk one. With nothing holding it for
  `SCROLL_ACTIVE_HOLD` it sinks, fading out over `SCROLL_FADE_SECS`.
- **Two copies, the host's and the widget's.** The idle copy is drawn at
  full alpha BEFORE the plate, every frame — raised or not, since the fore
  copy fades in over it and dropping it at the latch would blink the bar.
  The fore copy is drawn after the content at the activity's `fade()`.

Who draws what:

| Widget | Opt in | Idle copy (before the plate) | Fore copy (after the content) |
|---|---|---|---|
| `Spreadsheet` | always | host: `paint_scrollbars(rect, ctx, 1.0)` | the widget's own paint |
| `ParametersBg` | always | host: `scrollbar_quads()` as pills | host: the same at `scrollbar_fade()` |
| `ScrollRegion` | `with_sink_behind(true)` | framed: `push_prims`, under its bg; frameless: host, `push_scrollbar_prims` | host: `push_scrollbar_fore` |
| `ScrollBox` | `sink_behind = true` | host: `paint_scrollbar_pills(pc, 1.0)` | host: `paint_scrollbar_pills(pc, scrollbar_fade())` |
| `TreeList` | always (its `ScrollBox`) | the widget, under its own plate | the widget, over the rows and the well's wall |

For `ScrollRegion` and `ScrollBox`, sinking IS centring: a region or box
that does not opt in keeps its always-on bar at the right/bottom edge
(`edge_inset` applies to those only), and a sink-behind one ignores it.
A sink-behind `ScrollRegion`'s `push_prims` no longer draws the fore copy,
which would land under the rows a host draws after it. The tuple path
(`push_quads` / `push_scrollbar_quads`) is flat squares on a hard flip,
vertical only; every sink-behind host is on the prim path. The relief
scrollbar (`paint_relief_scrollbar`, carved groove and bevelled thumb)
is for edge bars only: shader-lit relief does not fade with a vertex
alpha. Tests: `sink_behind_bars_cross_at_the_centre`,
`the_fore_copy_is_drawn_after_the_rows`,
`a_sink_behind_bar_rides_the_centre_and_sinks_until_scrolled`.

**Not on it, deliberately:** a multi-line `TextBox`'s position indicator
stays at the right edge, always shown and non-interactive — an editor
keeps its place marker (the user's call, 2026-10-06).
