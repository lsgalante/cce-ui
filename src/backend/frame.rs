//! Building a frame: everything between "the window needs a frame" and "hand
//! the renderer its data" that knows nothing of the window system. The app is
//! asked for its display list, damage, custom vertices and overlays; widgets
//! are shaped against the glyph pass's font system; the list is tessellated
//! and its text gathered; popovers are collected for the text-occlusion
//! clamp. What comes out is a [`BuiltFrame`] — plain data the renderer draws
//! — and a shell presents it however its window system presents.
//!
//! The Wayland shell's half (`EngineState::render`) is what is left: the
//! grid patch and input region, uploading the glyphs, the extent gate and
//! buffer scale, the frame callback, `stage_renderer` and the draw. Order
//! between the two halves is the order the frame always had — the app's
//! damage is taken here even when the shell then skips the present.

use cosmic_text::FontSystem;

use super::app::{Application, LogicalSize};
use super::tessellate::{quad_vertices, tessellate_display_list, DlBatch, Vertex};
use super::text::{collect_dl_text, dl_text_spans, TextBounds};
use crate::draw::{Batch2D, Frame2D, ImageQuad, TextSpan};
use super::text::DlText;

/// One frame, built and owned: the renderer's [`Frame2D`] is a borrow of it
/// ([`frame2d`](Self::frame2d)). Its display-list text is shaped into the
/// `text_items` that [`build_frame`] was handed, and lent to the glyph pass
/// through [`text_spans`](Self::text_spans).
pub struct BuiltFrame {
    pub verts: Vec<Vertex>,
    pub batches: Vec<Batch2D>,
    /// Drawn after the text pass.
    pub overlay_verts: Vec<Vertex>,
    pub images: Vec<ImageQuad>,
    pub plate_features: Vec<[f32; 12]>,
    /// Linear, alpha as given.
    pub clear_color: [f32; 4],
    /// The part of the surface that changed, physical px; `None` = all of it.
    pub damage: Option<(u32, u32, u32, u32)>,
    /// The app's text is the display list's (`Application::display_list_text`).
    /// When false the app stages the renderer's text itself and the glyph
    /// pass must be left alone.
    pub dl_text: bool,
    /// Popover rects (and the in-window context menu) that page text is
    /// clamped away from, logical px, in stacking order.
    overlay_rects: Vec<(f32, f32, f32, f32)>,
    scale: f32,
    physical: (u32, u32),
}

impl BuiltFrame {
    /// The display-list text as glyph spans, borrowing `items` — the vector
    /// [`build_frame`] shaped this frame's text into.
    pub fn text_spans<'a>(&self, items: &'a [DlText]) -> Vec<TextSpan<'a>> {
        let (pw, ph) = self.physical;
        let bounds = TextBounds { left: 0, top: 0, right: pw as i32, bottom: ph as i32 };
        // All text is display-list text: each Text prim with the default
        // mapping (scale + surface clamp) plus the popover-occlusion clamp
        // against the app's registered popovers.
        dl_text_spans(items, self.scale, bounds, &self.overlay_rects)
    }

    /// The frame as the renderer takes it.
    pub fn frame2d(&self) -> Frame2D<'_> {
        Frame2D {
            verts: &self.verts,
            batches: &self.batches,
            overlay_verts: &self.overlay_verts,
            images: &self.images,
            plate_features: &self.plate_features,
            clear_color: self.clear_color,
            damage: self.damage,
        }
    }
}

/// Build the app's frame at `size` and `scale`.
///
/// `damage_owed` is the shell's: true when the last built frame was not
/// presented, so this one repaints everything. It is set true here; the
/// shell clears it once a frame is actually presented. `text_items` is where
/// the display-list text is shaped and held, for [`BuiltFrame::text_spans`].
pub fn build_frame<A: Application>(
    app: &mut A,
    fs: &mut FontSystem,
    size: LogicalSize,
    scale: f64,
    damage_owed: &mut bool,
    text_items: &mut Vec<DlText>,
) -> BuiltFrame {
    let (logical_w, logical_h) = (size.width, size.height);

    // The editing widget reports its caret as it paints (`ime::report_caret`):
    // where an input method's candidates go, and whether text is wanted.
    crate::ime::begin_frame();

    // 0. Shape every registered widget against the SAME FontSystem the glyph pass draws
    // with, before the app builds its frame. A widget's caret/selection/click→index math
    // reads per-glyph advances its `prepare_text` records; nothing else calls it on the
    // display-list path (the paint walk is `&dyn`, and apps were left to remember —
    // cce-list, cce-secrets, and the reference DemoApp all forgot, so their carets fell
    // back to `measure_text_width("M")`, an inked extent that drifts off the glyphs).
    // The flat path shapes in `layout::render_widget`; apps that hand-shape still work —
    // their call and this one hit the same shaped-buffer cache. Pointers are collected
    // first so the registry borrow ends before any widget is mutated (the missed-press
    // walk dereferences the same registry the same way).
    {
        let ptrs: Vec<*mut (dyn crate::widget::WidgetHost + 'static)> = app
            .ui_context()
            .map(|ctx| ctx.tree.iter_registered().map(|(_, p)| p).collect())
            .unwrap_or_default();
        for ptr in ptrs {
            unsafe {
                if let Some(w) = ptr.as_mut() {
                    w.prepare_text(fs);
                }
            }
        }
    }

    // 1. The frame's geometry IS the app's display list — the single paint path. Tessellated
    // below as one batched, GPU-scissor-clipped pass. An app that draws nothing returns
    // `None`, giving an empty frame (the legacy view*/tuple-wrapping path is gone).
    let dl = app
        .display_list(size, scale)
        .unwrap_or_else(|| crate::scene::paint::PaintCtx::new().finish());
    crate::ime::end_frame();
    // Taken with the display list it describes. A frame that took the
    // app's damage and then was not presented owes those pixels, so the
    // next one that is presented repaints everything.
    let app_damage = app.take_damage(size, scale);
    let damage = match (app_damage, *damage_owed) {
        (Some((x, y, w, h)), false) => {
            // Outward to whole physical pixels, plus one for an
            // antialiased edge.
            let s = scale as f32;
            let x0 = ((x * s).floor() - 1.0).max(0.0);
            let y0 = ((y * s).floor() - 1.0).max(0.0);
            let x1 = ((x + w) * s).ceil() + 1.0;
            let y1 = ((y + h) * s).ceil() + 1.0;
            Some((x0 as u32, y0 as u32, (x1 - x0).max(0.0) as u32, (y1 - y0).max(0.0) as u32))
        }
        _ => None,
    };
    *damage_owed = true;

    // 1a. Phase 6 display-list text: shape the list's Text prims through the shared buffer
    // cache and hold them for the glyph pass (the TextSpans the shell builds borrow these).
    // Clip = the paint walk's item clip ∩ the prim's own bounds, in logical space.
    text_items.clear();
    let dl_text = app.display_list_text();
    if dl_text {
        collect_dl_text(fs, &dl, text_items);
    }

    let (mut verts, mut dl_batches, dl_images, plate_features) =
        tessellate_display_list(&dl, logical_w, logical_h, scale as f32);
    // A pending height-field export (`CCE_HEIGHTMAP`, or an app's
    // `scene::heightfield::request`): the plates of THIS frame, sampled
    // as the geometry the shader is about to shade.
    if let Some(req) = crate::scene::heightfield::take_request() {
        let s = scale as f32;
        let (pw, ph) = ((logical_w * s).round() as usize, (logical_h * s).round() as usize);
        let hf = crate::scene::heightfield::HeightField::from_frame(&dl_batches, &plate_features, pw, ph, s);
        let (lo, hi) = hf.range_px();
        match crate::scene::heightfield::export_png(&hf, &req.path, req.mm_per_sample) {
            Ok(()) => log::info!(
                "[heightfield] wrote {} ({}x{} px, {:.3}..{:.3} mm, metric {})",
                req.path.display(), pw, ph, lo / hf.px_per_mm, hi / hf.px_per_mm, hf.source.as_str()
            ),
            Err(e) => log::warn!("[heightfield] export to {} failed: {e}", req.path.display()),
        }
    }
    // custom_vertices (e.g. graph geometry) is appended as a final unclipped batch drawn on top.
    let pre_custom = verts.len() as u32;
    app.custom_vertices(&mut verts, size, scale);
    if (verts.len() as u32) > pre_custom {
        dl_batches.push(DlBatch {
            scissor: None,
            clip_rrect: None,
            start: pre_custom,
            end: verts.len() as u32,
            plate: None,
            blur_behind: false,
        });
    }

    // 1b. Overlay quads (drawn after the text pass).
    let mut overlay_quads = Vec::new();
    app.overlay_quads(&mut overlay_quads, size, scale);
    let mut overlay_verts = Vec::new();
    for &(qx, qy, qw, qh, qc) in &overlay_quads {
        overlay_verts.extend(quad_vertices(qx, qy, qw, qh, logical_w, logical_h, qc));
    }

    // 2. What page text is clamped away from: the app's open popovers…
    let scale_f32 = scale as f32;
    let mut overlay_rects: Vec<(f32, f32, f32, f32)> = Vec::new();
    if let Some(ctx) = app.ui_context() {
        for &pop_id in &ctx.active_popovers {
            if let Some(ptr) = ctx.tree.get_ptr(pop_id) {
                unsafe {
                    if let Some((x, y, w, h)) = (*ptr).popover_rect() {
                        let (dx, dy) = app.popover_offset(pop_id);
                        overlay_rects.push((x + dx, y + dy, w, h));
                    }
                }
            }
        }
    }
    // …and the global context menu, which draws into the app's display list,
    // so it gets the same occlusion: the menu rect clamps list text beneath,
    // and the menu's own labels are exempt because they carry bounds equal to
    // the rect. Hosted in its popup surface, the menu covers the window from
    // above and nothing of it is in the list.
    if crate::widget::context_menu::is_visible() && !crate::widget::context_menu::is_hosted() {
        overlay_rects.push((
            crate::widget::context_menu::x(),
            crate::widget::context_menu::y(),
            crate::widget::context_menu::w(),
            crate::widget::context_menu::h(),
        ));
    }

    // 3. Images and batches in physical px.
    let images: Vec<ImageQuad> = dl_images
        .iter()
        .map(|di| ImageQuad {
            image: di.image,
            rect: (
                di.rect.x * scale_f32,
                di.rect.y * scale_f32,
                di.rect.width * scale_f32,
                di.rect.height * scale_f32,
            ),
            alpha: di.alpha,
            z_before: di.at,
            clip: di.clip.map(|c| {
                (
                    (c.x * scale_f32).max(0.0) as u32,
                    (c.y * scale_f32).max(0.0) as u32,
                    (c.width * scale_f32) as u32,
                    (c.height * scale_f32) as u32,
                )
            }),
        })
        .collect();

    let batches = super::tessellate::dl_batches_2d(&dl_batches, scale_f32);

    let cc = app.clear_color();
    let clear_color = [cc[0].powf(2.2), cc[1].powf(2.2), cc[2].powf(2.2), cc[3]];

    BuiltFrame {
        verts,
        batches,
        overlay_verts,
        images,
        plate_features,
        clear_color,
        damage,
        dl_text,
        overlay_rects,
        scale: scale_f32,
        physical: ((logical_w * scale_f32) as u32, (logical_h * scale_f32) as u32),
    }
}

#[cfg(test)]
mod tests {
    //! The builder with no window and no GPU: a mock app, an empty font
    //! database, and the frame's data read back.
    use super::*;
    use crate::backend::app::{AppSender, LogicalPosition, WindowSettings};
    use crate::widget::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};

    #[derive(Default)]
    struct Mock {
        damage: Option<(f32, f32, f32, f32)>,
        custom: usize,
        overlay: bool,
    }

    impl Application for Mock {
        type Message = ();
        fn create(_: AppSender<()>) -> Self {
            unreachable!("built directly")
        }
        fn settings(&self) -> WindowSettings {
            WindowSettings { title: String::new(), app_id: "mock".into(), width: 100, height: 50, fullscreen: false, min_size: None }
        }
        fn update(&mut self, _: (), _: &mut bool, _: &mut bool) {}
        fn tick(&mut self, _: f32, _: &mut bool) {}
        fn handle_pointer_move(&mut self, _: LogicalPosition, _: &mut bool) {}
        fn handle_mouse_input(&mut self, _: MouseButton, _: ElementState, _: LogicalPosition, _: &mut bool) -> Option<()> {
            None
        }
        fn handle_mouse_wheel(&mut self, _: &MouseScrollDelta, _: LogicalPosition, _: &mut bool) {}
        fn handle_key_input(&mut self, _: &KeyEvent, _: &mut bool) -> Option<()> {
            None
        }
        fn take_damage(&mut self, _: LogicalSize, _: f64) -> Option<(f32, f32, f32, f32)> {
            self.damage
        }
        fn custom_vertices(&mut self, verts: &mut Vec<Vertex>, size: LogicalSize, _: f64) {
            for _ in 0..self.custom {
                verts.extend(quad_vertices(0.0, 0.0, 1.0, 1.0, size.width, size.height, [1.0; 4]));
            }
        }
        fn overlay_quads(&mut self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _: LogicalSize, _: f64) {
            if self.overlay {
                quads.push((1.0, 2.0, 3.0, 4.0, [1.0; 4]));
            }
        }
        fn clear_color(&self) -> [f32; 4] {
            [0.5, 0.0, 1.0, 0.25]
        }
    }

    fn fonts() -> FontSystem {
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic_text::fontdb::Database::new())
    }

    const SIZE: LogicalSize = LogicalSize { width: 100.0, height: 50.0 };

    #[test]
    fn damage_is_physical_and_owed_until_a_present_clears_it() {
        let mut app = Mock { damage: Some((10.2, 4.0, 5.0, 3.0)), ..Mock::default() };
        let (mut fs, mut items) = (fonts(), Vec::new());

        // Nothing owed: the app's rect, outward to whole pixels at 2x plus one.
        let mut owed = false;
        let f = build_frame(&mut app, &mut fs, SIZE, 2.0, &mut owed, &mut items);
        assert_eq!(f.damage, Some((19, 7, 13, 8)));
        assert!(owed, "a built frame owes its pixels until the shell presents it");

        // The last frame was never presented: this one repaints everything.
        let f = build_frame(&mut app, &mut fs, SIZE, 2.0, &mut owed, &mut items);
        assert_eq!(f.damage, None);
    }

    #[test]
    fn custom_vertices_are_a_last_unclipped_batch_and_overlays_their_own() {
        let mut app = Mock { custom: 2, overlay: true, ..Mock::default() };
        let (mut fs, mut items, mut owed) = (fonts(), Vec::new(), false);
        let f = build_frame(&mut app, &mut fs, SIZE, 1.0, &mut owed, &mut items);
        // An empty display list: the custom quads are the whole vertex list,
        // in one batch on top with no scissor.
        assert_eq!(f.verts.len(), 12);
        let last = f.batches.last().expect("a batch for the custom vertices");
        assert_eq!((last.start, last.end, last.scissor), (0, 12, None));
        assert_eq!(f.overlay_verts.len(), 6);

        let none = build_frame(&mut Mock::default(), &mut fs, SIZE, 1.0, &mut owed, &mut items);
        assert!(none.verts.is_empty() && none.batches.is_empty());
    }

    #[test]
    fn the_clear_colour_is_linearized_and_its_alpha_kept() {
        let (mut fs, mut items, mut owed) = (fonts(), Vec::new(), false);
        let f = build_frame(&mut Mock::default(), &mut fs, SIZE, 1.0, &mut owed, &mut items);
        let [r, g, b, a] = f.clear_color;
        assert!((r - 0.5f32.powf(2.2)).abs() < 1e-6 && g == 0.0 && b == 1.0 && a == 0.25);
        // And the frame lends the renderer exactly what it built.
        let f2 = f.frame2d();
        assert_eq!((f2.clear_color, f2.damage), (f.clear_color, f.damage));
    }
}
