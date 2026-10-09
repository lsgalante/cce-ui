# Platforms, 3D and compute

The non-Wayland shells, the WebGPU renderer, compute jobs, 3D scenes and the path tracer, and
the probes that hold the renderers to each other. Present tense; history and measurements over
time are in `CHANGELOG.md`.

## The browser (wasm32)

The library builds for `wasm32-unknown-unknown`; `scripts/check-wasm` type-checks it with and
without the optional features (CI's `wasm` job). Native-only modules and crates are
`cfg(not(target_arch = "wasm32"))` (see `CLAUDE.md`, "Module map").

**web-sys's WebGPU bindings need `--cfg=web_sys_unstable_apis`.** `.cargo/config.toml` sets it for
the wasm target, and cargo reads that file from the directory it runs in — so build the browser
half from inside `cce-ui`, and a client crate that builds cce-ui for the browser needs the same
line. An environment `RUSTFLAGS` replaces the config file's, so CI sets the cfg itself.

**`WebRenderer`** (`src/web`): the Vulkan renderer's 2D path on WebGPU through web-sys, from the
same `Frame2D`, shaders, glyph atlas and image queue. Differences: an sRGB VIEW of the canvas's
unorm format; the parameter block as a dynamic-offset uniform (WebGPU has no push constants;
`shader2d_for_webgpu()` swaps the push-block line for a `@group(1)` uniform at
`WEBGPU_BLOCK_STRIDE`); a 1×1 backdrop; the blur snapshot as end-pass / copy / resume; every
frame drawn whole; the backdrop sampled with `textureSampleLevel(…, 0.0)` (no implicit LOD in a
non-uniform branch). `request_device` asks for the adapter's own limits where a caller needs more
than the spec defaults.

**The browser shell** (`web::run::<App>(canvas, fonts, sizing).await`, `src/web/shell.rs`) runs an
`Application` over a `<canvas>` on the same `Driver` and `Pacer`:

- **Events** on the canvas, mapped by `backend::dom` (portable, tested natively): pointer
  (captured on press so a drag outside still ends), wheel (`wheel_frame`: a notch-sized pixel
  delta — Chromium's 100 px — or a line/page delta is a wheel notch, anything else a finger; a
  finger's lift is synthesised after `FINGER_LIFT`, 120 ms, since a page reports none), keys
  (`map_key` gives the text xkb's `utf8` would: Tab `"\t"`, Enter `"\r"`, Ctrl+letter its control
  code). Browser key repeats are dropped (the driver repeats). On a Mac, Command is the shortcut
  key. A page has no grabs, so a press on a CSD border is the app's.
- **Pacing**: a turn per animation frame at the pacer's ACTIVE cadence, a timer at its idle one;
  any event or `AppSender::send` (`backend::app::set_wake`) wakes the loop.
- **Size**: `Sizing::App` sizes the canvas from `WindowSettings` / `desired_size` (CSS px);
  `Sizing::Page` leaves it to the page's CSS and ignores size requests. A ResizeObserver wakes a
  turn; `devicePixelRatio` is the scale. The context menu is drawn in the canvas.
- **Fonts** (`web::Fonts`): the page supplies the files and the generic serif / sans / mono
  families (no font directory, no fontconfig). `lib::page_fonts` holds them, and on wasm every
  font database the toolkit builds loads them (the shell's, `geometry_font_system`,
  `widget::input::get_font_db`). cosmic-text has no family fallback on wasm, so
  `page_fonts::stand_in_for_missing` maps the families the toolkit names to the first family of
  the Linux fallback list the set has. Order matters: the measuring fallback is the first face
  with the glyph.
- **`web::capture().await`** reads the next frame back from the GPU (headless Chromium leaves a
  WebGPU canvas out of screenshots and `toDataURL`).
- **Clipboard**: `widget::clipboard`'s synchronous pair is backed by the page's clipboard events,
  since a page may read the clipboard only inside a `paste` event. ⌘/Ctrl+C, X, V keep their
  defaults (`dom::clipboard_key`); a ⌘/Ctrl+V is HELD from the app until its `paste` event hands
  over the text (or a zero timer, when a read answers the page's own last copy or paste). A copy
  writes through `navigator.clipboard.writeText` where available (checked first — calling into
  undefined throws through the wasm frames) and the `copy` / `cut` event carries it too.
- **Keyboard and IME**: keys go to a hidden `<textarea>` (the keyboard sink), since a page composes
  input-method text only into an editable element. It takes the focus a press on the canvas
  gave; its keys are the app's except one the IME takes (`isComposing`, keyCode 229); `input`
  events while composing are the composition (`Driver::preedit`; cursor via
  `dom::utf16_range_to_bytes`), `compositionend` the commit, text with no composition (emoji
  panel, dictation) a commit as it comes. After each frame the sink moves to the caret
  (`ime::caret`); a dropped composition is cancelled by blurring and refocusing inside the turn.

Not supported in the browser: drag and drop, file dialogs, an app whose text is not the display
list's (`display_list_text` false), lit meshes (`Stage3D::lit()` is `None`), accessibility.

## macOS (type-checked only)

The AppKit shell (`src/mac`) runs over the same `Driver`, `Pacer`, `build_frame` and the
**Vulkan renderer on Metal through MoltenVK**. **It has never run**: Linux can type-check an
Apple target but not link it. `scripts/check-mac` type-checks the library, demo, examples and
tests for `aarch64-apple-darwin`; CI's `macos` job builds, links and runs the tests (GPU tests
skip there). On a Mac, MoltenVK and the Vulkan loader must be installed; `cargo run` is the test.

- `vk::SurfaceTarget` names a window's surface source: `Wayland { display, surface }` or
  `Metal { layer }`; `VkRenderer::try_new_for` / `attach_surface_to` take one. The instance
  enables `VK_EXT_metal_surface` when offered and, on macOS only,
  `VK_KHR_portability_enumeration`; a device offering `VK_KHR_portability_subset` gets it enabled.
- `engine::run::<App>()` is the AppKit shell's `run` on macOS.
- **The window**: transparent, its titlebar transparent over full-size content (traffic lights on
  the root plate's corner). AppKit resizes from its own edges, so the CSD resize band is the app's
  (`PressSite::own_edges`); a press read as a move calls `performWindowDragWithEvent:`.
- **Events** through `backend::appkit` (portable, tested on Linux): named keys from the hardware
  key code; text from `characters`, or with ⌘/Ctrl from `charactersIgnoringModifiers`; Command
  reads as `ctrl`; a Control-click is a right click. AppKit sends no keyUp for a ⌘-combination,
  so the shell releases it as it presses it. Key repeats and scroll MOMENTUM are dropped (the
  toolkit coasts itself). A wheel notch is turned back to the wheel's own direction;
  `isDirectionInvertedFromDevice` sets `input::force_natural_scroll` for trackpads.
- **Pacing**: main-queue dispatches (`dispatch2`); a wake from any thread
  (`app::set_wake`, process-wide on macOS) asks for a turn at most one ACTIVE frame after the
  last. ⌘Q and the close button ask the app to exit.
- **Fonts**: the system set is always loaded. **Clipboard**: the general `NSPasteboard`.
- **Input methods**: the view is an `NSTextInputClient`. While a widget edits text, a key without
  ⌘ goes through `interpretKeyEvents:` first (`setMarkedText:` = composition, `insertText:` =
  commit, except a plain key typing its own characters with nothing marked, left to the key path).
  `firstRectForCharacterRange:` is the caret on screen. With nothing editing, keys skip the input
  method.

Not supported on macOS: drag and drop, the context menu in a popup window, blur behind the window,
a menu bar beyond Quit.

## Compute jobs

What a job IS lives in the portable `crate::compute`: `Kernel`, `Binding`, `BindKind`,
`workgroups`, `MAX_BINDINGS`, and the rules checked before any device sees it (`check_job`, the
ping-pong `slot_for` / `result_slot`, `parse_kernel` — naga's WGSL frontend validates a kernel and
reads its `@workgroup_size`). `vk::compute` re-exports them.

- **`vk::ComputeDevice::run`** uploads the bindings, dispatches on a headless device, waits, and
  reads the read-write ones back; buffers are host-visible and mapped; every failure (a bad user
  kernel included) is an `Err`, never a panic. Built for cce-designer's solver operators.
- **`web::ComputeDevice`** takes the same jobs; its `run`, `run_over`, `run_passes`,
  `run_passes_over` and `workgroup_size` are `async` (readback is a promise). Its layouts
  distinguish read-only from read-write storage (`ParsedKernel::read_only_storage`); it asks for the
  adapter's storage-buffer limit (a job past it is an `Err` naming the limit); WebGPU validation
  errors are caught in an error scope and returned.
- `arrayLength` counts the 16-byte padding on both devices: a CPU reference for a length that is
  not a multiple of four floats must know that.
- `examples/compute_probe/jobs.rs` is the check, run by `compute_native` and
  `scripts/web-probe/compute`; the two devices' outputs agree to the bit.

## 3D scenes (`Stage3D`)

An app stages a scene through the trait `draw::scene::Stage3D` (`create_mesh`, `update_mesh`,
`stage_scene`, `stage_scene_images`, `set_scene_light`), implemented by `VkRenderer` and
`WebRenderer`. Types: `Vertex3D`, `MeshId`, `SceneDraw`, `SceneImage`, `scene_uniforms`,
`wire_base_bias`; shaders `scene3d.wgsl` / `scene3d_image.wgsl` in `draw/`. The hooks are
`Application::init_3d` (once per renderer: make meshes) and `stage_3d` (every frame before the
draw; true asks for another frame).

- **The backdrop**: a full-size pass in the canvas's sRGB format with a depth32 buffer, copied
  under a UI pass that loads it and samples it for blur, and kept until the next staged scene. On
  WebGPU depth bias is pipeline state and lines are one pixel, so the biased fill is a pipeline at
  `wire_base_bias(1.0)` (what a Vulkan device without wideLines uses). `scene3d.wgsl` takes its
  derivatives at the top of `fs_main` (WebGPU rejects them under a varying branch).
- **A mesh update does not wait for the GPU** (`SceneStage::update_mesh` / `update_lit_mesh`
  through `Mesh::replace`): new vertices go into a spare buffer no submitted frame reads, the
  replaced buffer becomes a spare tagged with the frames submitted so far
  (`SceneStage::submitted`), and after each fence wait `frame_waited` releases undersized spares
  and keeps two — steady playback allocates nothing.
- **Instancing** (`SceneDraw::instances`): a draw naming an instance mesh draws its `mesh` once
  per instance vertex — position offset added, colour multiplied. `instances: None` draws one
  instance at the origin in white (`draw::scene::UNIT_INSTANCE`), leaving vertices bit-for-bit
  unchanged. An empty instance mesh draws nothing. Every mesh pipeline has a second per-instance
  vertex binding (locations 2 and 3).
- **Screen-space draws say so** (`SceneDraw::screen_space`, carried in the uniform block):
  `scene3d.wgsl` places such vertices at their own xy on the far plane, unlit, without the mvp.
  Never key behaviour off vertex values.
- **Lit and textured meshes** (`draw::lit`): `LitVertex` (position, normal, uv, colour) drawn by
  `scene3d_lit.wgsl` with a `LitMaterial` (base colour, optional base-colour image id, metallic,
  roughness) under a `LitLight` (key, fill, sky/ground ambient): Lambert plus GGX/Smith/Schlick per
  light, the hemisphere as diffuse ambient and as what a metal reflects; diffuse has no 1/π so a
  matte lit draw matches a baked one. Reached through `Stage3D::lit()` — `Some` on Vulkan
  (`LitStage3D`: `create_lit_mesh`, `update_lit_mesh`, `set_lit_light`, `stage_lit`), `None` on
  WebGPU, so a host checks. `LitDraw::before` interleaves lit draws with `SceneDraw`s. On Vulkan:
  a pipeline on the image pipeline's layout, no culling (the shader flips back-face normals),
  dynamic depth bias, uniform slot stride `SLOT_SIZE` (224 bytes); an untextured or non-resident
  draw binds a 1×1 white image. Textures are ordinary RGBA8 sRGB image ids that die with the
  renderer (re-upload in `init_3d`); the sampler clamps, so the shader wraps uvs with `fract` and
  samples with the unwrapped uvs' gradients (`textureSampleGrad`) to avoid a mip seam. cce-model
  is the consumer.

## The path tracer

`Stage3D` carries it too: `set_rt_scene` / `set_rt_scene_with_image`, `set_rt_environment`,
`set_rt_background`, `stage_rt`, `rt_accumulating`. The schema, binned-SAH BVH, buffer packing
(`pack_scene`) and parameter blocks (`rt_params`, `denoise_params`) live in `draw::rt`; shaders
`rt_common` / `rt_bvh` / `rt_query` / `rt_denoise` in `draw/`. `vk::rt` keeps frames in flight,
the ray-query tier and `RtOffscreen`.

- **Tiers**: Vulkan ray query where the device has it, else the compute tier (`CCE_VK_RT=compute`
  forces it; `CCE_VK_RT=0` disables RT). WebGPU (`web/rt.rs`) has only the compute tier: one
  sample a frame plus three à-trous denoise iterations in one pass, and — since WebGPU copies
  cannot convert — a small render pass that writes the `rgba8unorm` result through the backdrop's
  sRGB view.
- **Large scenes are prepared off the UI thread**: `PreparedRtScene::new(tris, mats, image,
  with_bvh)` is the CPU half (packing plus BVH), `Send + Sync`, built on a worker;
  `Stage3D::set_rt_scene_prepared(&scene)` only uploads and keeps it for reconnects. Ask
  `Stage3D::rt_needs_bvh()` on the UI thread first (false on the ray-query tier, which builds its
  BLAS on the GPU); a scene prepared without a BVH on a compute-tier renderer gets one at upload
  (`PackedScene::with_bvh`), so a wrong answer costs time, never a wrong image.
- GPU tests: `cargo test --lib rt -- --ignored`, per tier with `VK_DRIVER_FILES` pinned (Intel =
  compute, NVIDIA = ray query, NVIDIA + `CCE_VK_RT=compute`).

## Probes: the renderers held to each other

All need the same fonts on both halves and a screenshot without a cursor (mask it; never use
sway's `hide_cursor`). Chromium runs headless with `--use-angle=swiftshader
--enable-unsafe-swiftshader --disable-gpu-compositing` beside the WebGPU flags (`browser.mjs`);
with GPU compositing it loses the device on first present.

| Probe | Native half | Browser half | Compares |
|---|---|---|---|
| 2D renderer: one 1280×800 frame of nearly every prim (`examples/probe/scene.rs`) | `cargo run --example probe_native` (with `CCE_LOAD_SYSTEM_FONTS=1`), screenshot | `scripts/web-probe/run <out.rgba>` (fonts from `$PROBE_FONTS_DIR`) | `scripts/web-probe/compare.py native.png out.rgba 1280 800` |
| The demo, 24 scripted steps (`examples/demo_web.rs` includes `src/main.rs`) | the native harness's screenshots | `scripts/web-probe/demo <dir>` (`drive.mjs`; `DEMO_FAMILIES` to match native's fonts) | `compare.py --mask` |
| 3D scene (`examples/probe3d/scene.rs`) | `probe3d_native` (`PROBE3D_TRACE=1` for the traced mode, eight frames) | `scripts/web-probe/probe3d <out> [traced]` | `compare.py` |
| Compute jobs (`examples/compute_probe/jobs.rs`) | `compute_native` | `scripts/web-probe/compute` | exact |
| Clipboard, IME | — | `scripts/web-probe/clipboard`, `scripts/web-probe/ime` (CDP `Input.imeSetComposition`) | pass/fail |

Expected agreement (lavapipe vs SwiftShader): the 2D probe within 2 levels except rounding at
antialiased edges; the demo identical except 1 px of rasterizer tie-break at the slider band's
tips (key-repeat steps differ by timing, not routing); the 3D probe within 2 except along 1 px
wires; the traced pane's mean within 0.1 of a level at eight samples; compute to the bit.

**Not yet run in a browser**: the WebGPU halves of instancing (`GpuVertexStepMode::Instance`,
`draw_with_instance_count` in `web/scene.rs`) and of screen-space draws. CI type-checks them;
`scripts/web-probe/probe3d` on a machine with the wasm32 target is the first real test.
