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

/// What a presented frame drew, kept so the next frame can be diffed against
/// it ([`derive_damage`]). One per window; physical px throughout.
#[derive(Default)]
pub struct FrameRecord {
    prev: Option<FrameSig>,
}

/// A box as (x0, y0, x1, y1), physical px.
type PxBox = (i32, i32, i32, i32);

struct FrameSig {
    physical: (u32, u32),
    clear: [u32; 4],
    features: u64,
    batches: Vec<(u64, Option<PxBox>)>,
    texts: Vec<(u64, Option<PxBox>)>,
    images: Vec<(u64, Option<PxBox>, u32)>,
    overlay: (u64, Option<PxBox>),
}

fn hash_of(f: impl FnOnce(&mut std::collections::hash_map::DefaultHasher)) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    f(&mut h);
    h.finish()
}

fn union_box(a: Option<PxBox>, b: Option<PxBox>) -> Option<PxBox> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))),
        (a, b) => a.or(b),
    }
}

fn clip_box(b: Option<PxBox>, clip: Option<(u32, u32, u32, u32)>) -> Option<PxBox> {
    let b = b?;
    let Some((cx, cy, cw, ch)) = clip else { return Some(b) };
    let c = (cx as i32, cy as i32, (cx + cw) as i32, (cy + ch) as i32);
    let r = (b.0.max(c.0), b.1.max(c.1), b.2.min(c.2), b.3.min(c.3));
    (r.0 < r.2 && r.1 < r.3).then_some(r)
}

fn verts_box(verts: &[Vertex], (pw, ph): (u32, u32)) -> Option<PxBox> {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for v in verts {
        let px = (v.position[0] + 1.0) * 0.5 * pw as f32;
        let py = (1.0 - v.position[1]) * 0.5 * ph as f32;
        x0 = x0.min(px);
        y0 = y0.min(py);
        x1 = x1.max(px);
        y1 = y1.max(py);
    }
    (x0 <= x1).then(|| ((x0.floor() as i32) - 2, (y0.floor() as i32) - 2, (x1.ceil() as i32) + 2, (y1.ceil() as i32) + 2))
}

impl FrameSig {
    fn of(frame: &BuiltFrame, texts: &[DlText]) -> Self {
        use std::hash::Hash;
        let physical = frame.physical;
        let batches = frame
            .batches
            .iter()
            .map(|b| {
                let verts = frame.verts.get(b.start as usize..b.end as usize).unwrap_or(&[]);
                let key = hash_of(|h| {
                    bytemuck::cast_slice::<Vertex, u8>(verts).hash(h);
                    b.scissor.hash(h);
                    b.clip_rrect.map(|c| c.map(f32::to_bits)).hash(h);
                    b.blur_behind.hash(h);
                    if let Some(p) = &b.plate {
                        format!("{p:?}").hash(h);
                    }
                });
                (key, clip_box(verts_box(verts, physical), b.scissor))
            })
            .collect();
        let s = frame.scale;
        let texts = texts
            .iter()
            .map(|t| {
                let m = t.buffer.metrics();
                let (mut w, mut bottom) = (0.0f32, 0.0f32);
                let key = hash_of(|h| {
                    for line in &t.buffer.lines {
                        line.text().hash(h);
                    }
                    for run in t.buffer.layout_runs() {
                        w = w.max(run.line_w);
                        bottom = bottom.max(run.line_top + run.line_height);
                    }
                    (t.x.to_bits(), t.y.to_bits(), t.color.0, m.font_size.to_bits()).hash(h);
                    t.bounds.map(|b| b.map(f32::to_bits)).hash(h);
                    t.clip_circle.map(|c| c.map(f32::to_bits)).hash(h);
                    t.clip_rrect.map(|c| c.map(f32::to_bits)).hash(h);
                });
                // Generous: a glyph can overhang its advance box, and the
                // run's top is not the ink's.
                let pad = m.font_size.max(m.line_height);
                let (x, y) = (t.x * s, t.y * s);
                let b = (
                    (x - pad).floor() as i32,
                    (y - pad - m.line_height).floor() as i32,
                    (x + w + pad).ceil() as i32,
                    (y + bottom + pad).ceil() as i32,
                );
                (key, Some(b))
            })
            .collect();
        let images = frame
            .images
            .iter()
            .map(|q| {
                let key = hash_of(|h| {
                    (q.image, q.rect.0.to_bits(), q.rect.1.to_bits(), q.rect.2.to_bits(), q.rect.3.to_bits()).hash(h);
                    (q.alpha.to_bits(), q.z_before, q.clip).hash(h);
                });
                let r = q.rect;
                let b = Some((r.0.floor() as i32 - 1, r.1.floor() as i32 - 1, (r.0 + r.2).ceil() as i32 + 1, (r.1 + r.3).ceil() as i32 + 1));
                (key, clip_box(b, q.clip), q.image)
            })
            .collect();
        let overlay = (
            hash_of(|h| bytemuck::cast_slice::<Vertex, u8>(&frame.overlay_verts).hash(h)),
            verts_box(&frame.overlay_verts, physical),
        );
        FrameSig {
            physical,
            clear: frame.clear_color.map(f32::to_bits),
            features: hash_of(|h| frame.plate_features.iter().for_each(|f| f.map(f32::to_bits).hash(h))),
            batches,
            texts,
            images,
            overlay,
        }
    }
}

/// The boxes that differ between two runs of (key, box), old and new: the
/// common prefix and suffix are unchanged, and everything between them —
/// inserted, removed or altered — is damage, where it was AND where it is.
fn diff_runs<T>(old: &[T], new: &[T], key: impl Fn(&T) -> u64, bx: impl Fn(&T) -> Option<PxBox>) -> Option<PxBox> {
    let pre = old.iter().zip(new).take_while(|(a, b)| key(a) == key(b)).count();
    let suf = old[pre..].iter().rev().zip(new[pre..].iter().rev()).take_while(|(a, b)| key(a) == key(b)).count();
    let mut d = None;
    for x in &old[pre..old.len() - suf] {
        d = union_box(d, bx(x).or(Some((i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2))));
    }
    for x in &new[pre..new.len() - suf] {
        d = union_box(d, bx(x).or(Some((i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2))));
    }
    d
}

/// Damage the runner works out for itself, for an app that does not report
/// its own (`Application::take_damage`): what this frame drew that the last
/// presented one did not, by diffing the tessellated batches, the text and
/// the images. The renderer repaints only that (growing it around frosted
/// plates) and reports only it to the compositor. Until 2026-10-06 every such
/// frame was a full repaint — a one-button hover change repainted and
/// recomposited the whole window.
///
/// Conservative: anything it cannot see a frame's difference in makes the
/// frame full — a new size, scale or clear colour, changed plate carves, the
/// first frame, an app that stages its own text, an image whose pixels were
/// replaced in place is damaged where it is drawn. `CCE_UI_FULL_DAMAGE=1`
/// turns it off.
pub fn derive_damage(frame: &mut BuiltFrame, record: &mut FrameRecord, texts: &[DlText], damage_owed: bool) {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let off = *OFF.get_or_init(|| std::env::var_os("CCE_UI_FULL_DAMAGE").is_some());
    let updated = crate::draw::images::take_updated_ids();
    let sig = FrameSig::of(frame, texts);
    let prev = record.prev.replace(sig);
    if off || damage_owed || frame.damage.is_some() || !frame.dl_text {
        return;
    }
    let (Some(prev), Some(cur)) = (prev, record.prev.as_ref()) else { return };
    if prev.physical != cur.physical || prev.clear != cur.clear || prev.features != cur.features {
        return;
    }
    let mut d = diff_runs(&prev.batches, &cur.batches, |b| b.0, |b| b.1);
    d = union_box(d, diff_runs(&prev.texts, &cur.texts, |t| t.0, |t| t.1));
    d = union_box(d, diff_runs(&prev.images, &cur.images, |i| i.0, |i| i.1));
    if prev.overlay.0 != cur.overlay.0 {
        d = union_box(d, union_box(prev.overlay.1, cur.overlay.1));
    }
    for (_, b, id) in &cur.images {
        if updated.contains(id) {
            d = union_box(d, *b);
        }
    }
    let (pw, ph) = cur.physical;
    let (x0, y0, x1, y1) = d.unwrap_or((0, 0, 1, 1));
    let (x0, y0) = (x0.clamp(0, pw as i32), y0.clamp(0, ph as i32));
    let (x1, y1) = (x1.clamp(x0, pw as i32), y1.clamp(y0, ph as i32));
    if x1 - x0 >= pw as i32 && y1 - y0 >= ph as i32 {
        return;
    }
    frame.damage = Some((x0 as u32, y0 as u32, (x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32));
    if crate::vk::present_debug() {
        eprintln!("[vk] derived damage {}x{}+{}+{} of {pw}x{ph}", x1 - x0, y1 - y0, x0, y0);
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

#[cfg(test)]
mod damage_tests {
    use super::*;

    fn k(v: &[(u64, i32)]) -> Vec<(u64, Option<PxBox>)> {
        v.iter().map(|&(key, x)| (key, Some((x, 0, x + 10, 10)))).collect()
    }

    /// Unchanged runs are no damage; a changed, inserted or removed item is
    /// damaged where it was and where it is, and nothing beyond the common
    /// prefix and suffix is.
    #[test]
    fn diff_runs_takes_the_middle() {
        let (a, b) = (k(&[(1, 0), (2, 20), (3, 40)]), k(&[(1, 0), (2, 20), (3, 40)]));
        assert_eq!(diff_runs(&a, &b, |x| x.0, |x| x.1), None);
        let changed = k(&[(1, 0), (9, 25), (3, 40)]);
        assert_eq!(diff_runs(&a, &changed, |x| x.0, |x| x.1), Some((20, 0, 35, 10)));
        let inserted = k(&[(1, 0), (7, 60), (2, 20), (3, 40)]);
        assert_eq!(diff_runs(&a, &inserted, |x| x.0, |x| x.1), Some((60, 0, 70, 10)));
        let removed = k(&[(1, 0), (3, 40)]);
        assert_eq!(diff_runs(&a, &removed, |x| x.0, |x| x.1), Some((20, 0, 30, 10)));
    }

    /// An item with no box (nothing to bound it by) damages everything.
    #[test]
    fn an_unbounded_change_is_everything() {
        let a = vec![(1u64, None)];
        let b = vec![(2u64, None)];
        let d = diff_runs(&a, &b, |x: &(u64, Option<PxBox>)| x.0, |x| x.1).unwrap();
        assert!(d.0 < -1_000_000 && d.2 > 1_000_000);
    }
}
